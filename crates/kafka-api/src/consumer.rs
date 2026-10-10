//! The single receive loop (kafka.md).
//!
//! One loop handles one message at a time, so the commands of one key (one partition) run in
//! the order they were produced (kafka.md AC-5). Auto-commit is off: the offset is committed
//! after the handler succeeds, or after the DLQ write succeeds. If the DLQ write fails, the
//! loop stops with an error and the process exits, so the message is read again on restart.

use std::time::Duration;

use rdkafka::ClientConfig;
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::message::{Headers, Message};
use tokio_util::sync::CancellationToken;

use crate::dispatcher::{Dispatcher, Handled};
use crate::dlq::{DlqError, DlqRecord, DlqSink, Incoming, KafkaDlq};
use crate::reason::Rejection;

/// How long the loop waits after a receive error before it polls again, so a broker outage
/// cannot turn the loop into a busy loop. Own proposal.
const RECEIVE_ERROR_PAUSE: Duration = Duration::from_millis(500);

/// What the consumer needs to connect (`KAFKA_BROKERS` and `APP_NAME`).
#[derive(Debug, Clone)]
pub struct ConsumerConfig {
    pub brokers: String,
    /// The consumer group: one per service.
    pub group_id: String,
}

/// Why the consumer stopped with an error.
#[derive(Debug, thiserror::Error)]
pub enum ConsumerError {
    #[error("no command handler is registered, so there is no topic to consume")]
    NoTopics,
    #[error("cannot create the Kafka consumer: {0}")]
    Create(String),
    #[error("cannot subscribe to {topics}: {cause}")]
    Subscribe { topics: String, cause: String },
    /// The DLQ write failed. The message stays uncommitted.
    #[error("the DLQ write failed, the consumer stopped: {0}")]
    Dlq(#[from] DlqError),
}

/// What happened to one message.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Processed(Handled),
    DeadLettered(Rejection),
}

/// Consumes the topics of the dispatcher's handlers until `shutdown` is cancelled, and returns
/// `Ok`. The message in flight when `shutdown` is cancelled is finished and committed first.
/// Returns an error when the DLQ cannot be written.
pub async fn run(
    config: &ConsumerConfig,
    dispatcher: &Dispatcher,
    shutdown: CancellationToken,
) -> Result<(), ConsumerError> {
    if dispatcher.is_empty() {
        return Err(ConsumerError::NoTopics);
    }
    let dlq = KafkaDlq::new(&config.brokers)?;
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &config.brokers)
        .set("group.id", &config.group_id)
        // The offset is committed by hand, after the message is handled or dead-lettered.
        .set("enable.auto.commit", "false")
        // A new group starts at the beginning; librdkafka's default is `largest`.
        .set("auto.offset.reset", "earliest")
        .create()
        .map_err(|err| ConsumerError::Create(err.to_string()))?;
    let topics: Vec<&str> = dispatcher.topics().iter().map(String::as_str).collect();
    consumer
        .subscribe(&topics)
        .map_err(|err| ConsumerError::Subscribe {
            topics: topics.join(", "),
            cause: err.to_string(),
        })?;
    tracing::info!(topics = %topics.join(","), group_id = %config.group_id, "kafka consumer started");

    let result = receive_loop(&consumer, dispatcher, &dlq, &shutdown).await;

    // Dropping a consumer closes it, which polls until the group is left. Do that off the
    // async threads.
    if let Err(err) = tokio::task::spawn_blocking(move || drop(consumer)).await {
        tracing::warn!(error = %err, "the Kafka consumer did not close cleanly");
    }
    tracing::info!("kafka consumer stopped");
    result
}

async fn receive_loop(
    consumer: &StreamConsumer,
    dispatcher: &Dispatcher,
    dlq: &dyn DlqSink,
    shutdown: &CancellationToken,
) -> Result<(), ConsumerError> {
    loop {
        // Only waiting for a message can be interrupted. A message that was received is
        // always finished below.
        let received = tokio::select! {
            biased;
            () = shutdown.cancelled() => return Ok(()),
            received = consumer.recv() => received,
        };
        let message = match received {
            Ok(message) => message,
            Err(err) => {
                tracing::warn!(error = %err, "kafka receive error");
                tokio::select! {
                    () = shutdown.cancelled() => return Ok(()),
                    () = tokio::time::sleep(RECEIVE_ERROR_PAUSE) => {}
                }
                continue;
            }
        };

        let headers: Vec<(String, Option<Vec<u8>>)> = message
            .headers()
            .map(|headers| {
                headers
                    .iter()
                    .map(|header| (header.key.to_owned(), header.value.map(<[u8]>::to_vec)))
                    .collect()
            })
            .unwrap_or_default();
        let incoming = Incoming {
            topic: message.topic(),
            partition: message.partition(),
            offset: message.offset(),
            key: message.key(),
            value: message.payload(),
            headers: &headers,
        };

        if let Err(err) = process(dispatcher, dlq, &incoming).await {
            tracing::error!(
                topic = incoming.topic,
                partition = incoming.partition,
                offset = incoming.offset,
                error = %err,
                "cannot write the message to the DLQ; stopping without committing it"
            );
            return Err(err.into());
        }

        // An asynchronous commit does not block the runtime. If it is lost, the message is
        // read again, and every handler is idempotent (kafka.md AC-2).
        if let Err(err) = consumer.commit_message(&message, CommitMode::Async) {
            tracing::warn!(
                topic = incoming.topic,
                partition = incoming.partition,
                offset = incoming.offset,
                error = %err,
                "cannot commit the offset; the message may be read again"
            );
        }
    }
}

/// Handles one message: runs the handler, or writes the message to the DLQ. Returns once the
/// outcome is final, so the caller may commit. An error means the DLQ write failed and the
/// message must not be committed. Never logs the payload.
pub async fn process(
    dispatcher: &Dispatcher,
    dlq: &dyn DlqSink,
    incoming: &Incoming<'_>,
) -> Result<Outcome, DlqError> {
    match dispatcher
        .dispatch(incoming.topic, incoming.key, incoming.value)
        .await
    {
        Ok(handled) => {
            tracing::info!(
                message_id = %handled.message_id,
                message_type = %handled.message_type,
                topic = incoming.topic,
                partition = incoming.partition,
                offset = incoming.offset,
                "command processed"
            );
            Ok(Outcome::Processed(handled))
        }
        Err(rejection) => {
            dlq.send(DlqRecord::new(incoming, &rejection)).await?;
            tracing::warn!(
                message_id = ?rejection.message_id,
                topic = incoming.topic,
                partition = incoming.partition,
                offset = incoming.offset,
                reason = rejection.reason.as_str(),
                cause = %rejection.cause,
                "command sent to the DLQ"
            );
            Ok(Outcome::DeadLettered(rejection))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use application::error::AppError;
    use async_trait::async_trait;

    use super::*;
    use crate::dispatcher::tests::{ID, Recorder, Reply, command};
    use crate::handler::CommandHandler;
    use crate::reason::DlqReason;

    #[derive(Default)]
    struct FakeDlq {
        fail: bool,
        sent: Mutex<Vec<DlqRecord>>,
    }

    #[async_trait]
    impl DlqSink for FakeDlq {
        async fn send(&self, record: DlqRecord) -> Result<(), DlqError> {
            if self.fail {
                return Err(DlqError::Send {
                    topic: record.topic,
                    cause: "broker down".to_owned(),
                });
            }
            if let Ok(mut sent) = self.sent.lock() {
                sent.push(record);
            }
            Ok(())
        }
    }

    fn dispatcher(result: Reply) -> Dispatcher {
        let handler: Arc<dyn CommandHandler> =
            Recorder::new("products.commands.v1", &["product.create"], result);
        Dispatcher::new(vec![handler]).unwrap()
    }

    fn incoming<'a>(key: Option<&'a [u8]>, value: Option<&'a [u8]>) -> Incoming<'a> {
        Incoming {
            topic: "products.commands.v1",
            partition: 0,
            offset: 7,
            key,
            value,
            headers: &[],
        }
    }

    #[tokio::test]
    async fn a_handled_command_writes_nothing_to_the_dlq() {
        let dlq = FakeDlq::default();
        let value = command("product.create");
        let outcome = process(
            &dispatcher(|| Ok(())),
            &dlq,
            &incoming(Some(ID.as_bytes()), Some(&value)),
        )
        .await
        .unwrap();
        assert!(matches!(outcome, Outcome::Processed(_)));
        assert!(dlq.sent.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_rejected_command_is_written_to_the_dlq_with_its_reason() {
        let dlq = FakeDlq::default();
        let outcome = process(
            &dispatcher(|| Ok(())),
            &dlq,
            &incoming(Some(b"k1"), Some(b"not json")),
        )
        .await
        .unwrap();
        let Outcome::DeadLettered(rejection) = outcome else {
            panic!("expected a dead letter");
        };
        assert_eq!(rejection.reason, DlqReason::MalformedJson);
        let sent = dlq.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].topic, "products.commands.v1.dlq");
        assert_eq!(sent[0].value.as_deref(), Some(&b"not json"[..]));
    }

    #[tokio::test]
    async fn a_failing_handler_goes_to_the_dlq_with_the_mapped_reason() {
        let dlq = FakeDlq::default();
        let value = command("product.create");
        let outcome = process(
            &dispatcher(|| Err(AppError::NotFound)),
            &dlq,
            &incoming(Some(ID.as_bytes()), Some(&value)),
        )
        .await
        .unwrap();
        let Outcome::DeadLettered(rejection) = outcome else {
            panic!("expected a dead letter");
        };
        assert_eq!(rejection.reason, DlqReason::NotFound);
        assert_eq!(dlq.sent.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_failed_dlq_write_is_an_error_so_the_caller_does_not_commit() {
        let dlq = FakeDlq {
            fail: true,
            ..FakeDlq::default()
        };
        let err = process(
            &dispatcher(|| Ok(())),
            &dlq,
            &incoming(Some(b"k1"), Some(b"not json")),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, DlqError::Send { .. }), "{err}");
    }

    #[tokio::test]
    async fn running_without_handlers_is_an_error() {
        let config = ConsumerConfig {
            brokers: "localhost:1".to_owned(),
            group_id: "g".to_owned(),
        };
        let dispatcher = Dispatcher::new(vec![]).unwrap();
        let err = run(&config, &dispatcher, CancellationToken::new())
            .await
            .unwrap_err();
        assert!(matches!(err, ConsumerError::NoTopics), "{err}");
    }
}
