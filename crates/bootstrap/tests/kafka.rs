//! The Kafka consumer against a real broker (kafka.md AC-1, AC-3, AC-5).
//!
//! A dummy handler stands in for a resource: widget handlers come with the widgets resource.

// The no-panic rule applies outside tests (security.md SEC-11). clippy.toml allows these in
// #[test] functions only, and integration-test helpers are plain functions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use application::error::AppError;
use async_trait::async_trait;
use kafka_api::{CommandHandler, ConsumerConfig, ConsumerError, Dispatcher};
use rdkafka::ClientConfig;
use rdkafka::admin::{AdminClient, AdminOptions, NewTopic, TopicReplication};
use rdkafka::client::DefaultClientContext;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::{Headers, Message, OwnedMessage};
use rdkafka::producer::{FutureProducer, FutureRecord};
use serde_json::{Value, json};
use testcontainers_modules::kafka::apache::{self, Kafka};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Every await on the system under test is bounded (layers.md section 7). The broker
/// waits before the first consumer group forms, so this is longer than the HTTP tests' bound.
const STEP: Duration = Duration::from_secs(60);

/// Starting the container may pull the image first.
const START: Duration = Duration::from_secs(180);

/// The image tag of docker-compose.yml.
const KAFKA_TAG: &str = "4.3.1";

const TOPIC: &str = "widgets.commands.v1";
const DLQ_TOPIC: &str = "widgets.commands.v1.dlq";

async fn within<T>(what: &str, future: impl Future<Output = T>) -> T {
    match tokio::time::timeout(STEP, future).await {
        Ok(value) => value,
        Err(_) => panic!("timed out after {STEP:?}: {what}"),
    }
}

/// A command the consumer handed to the dummy handler.
#[derive(Debug)]
struct Received {
    message_type: String,
    payload: Value,
}

/// Records every command. A command with an even `seq` takes longer, so a loop that handled
/// messages concurrently would reorder them.
struct Dummy {
    received: UnboundedSender<Received>,
}

#[async_trait]
impl CommandHandler for Dummy {
    fn topic(&self) -> &str {
        TOPIC
    }

    fn types(&self) -> &[&'static str] {
        &["widget.create", "widget.update"]
    }

    async fn handle(&self, message_type: &str, payload: Value) -> Result<(), AppError> {
        if payload["seq"].as_u64().is_some_and(|seq| seq % 2 == 0) {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        self.received
            .send(Received {
                message_type: message_type.to_owned(),
                payload,
            })
            .map_err(|_| AppError::Internal("the test stopped listening".to_owned()))
    }
}

/// A broker, the consumer under test, and the clients the tests use.
struct Harness {
    producer: FutureProducer,
    dlq: StreamConsumer,
    received: UnboundedReceiver<Received>,
    shutdown: CancellationToken,
    consumer: JoinHandle<Result<(), ConsumerError>>,
    // Last, so it is dropped last: a client that closes after its broker is gone waits for
    // the broker to answer.
    _broker: ContainerAsync<Kafka>,
}

impl Harness {
    async fn start() -> Self {
        let broker = tokio::time::timeout(
            START,
            Kafka::default()
                .with_jvm_image()
                .with_tag(KAFKA_TAG)
                .start(),
        )
        .await
        .unwrap()
        .unwrap();
        let port = within("broker port", broker.get_host_port_ipv4(apache::KAFKA_PORT))
            .await
            .unwrap();
        let brokers = format!("127.0.0.1:{port}");

        // 3 partitions, as in docker-compose.yml, so keys spread over partitions.
        let admin: AdminClient<DefaultClientContext> = ClientConfig::new()
            .set("bootstrap.servers", &brokers)
            .create()
            .unwrap();
        let topics = [
            NewTopic::new(TOPIC, 3, TopicReplication::Fixed(1)),
            NewTopic::new(DLQ_TOPIC, 1, TopicReplication::Fixed(1)),
        ];
        let created = within(
            "create topics",
            admin.create_topics(&topics, &AdminOptions::new()),
        )
        .await
        .unwrap();
        for result in created {
            result.unwrap();
        }

        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", &brokers)
            .create()
            .unwrap();
        let dlq: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &brokers)
            .set("group.id", "dlq-reader")
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "false")
            .create()
            .unwrap();
        dlq.subscribe(&[DLQ_TOPIC]).unwrap();

        let (tx, received) = unbounded_channel();
        let handler: Arc<dyn CommandHandler> = Arc::new(Dummy { received: tx });
        let dispatcher = Dispatcher::new(vec![handler]).unwrap();
        let config = ConsumerConfig {
            brokers,
            group_id: "rs-boilerplate-test".to_owned(),
        };
        let shutdown = CancellationToken::new();
        let consumer = tokio::spawn({
            let shutdown = shutdown.clone();
            async move { kafka_api::run(&config, &dispatcher, shutdown).await }
        });

        Self {
            producer,
            dlq,
            received,
            shutdown,
            consumer,
            _broker: broker,
        }
    }

    async fn produce(&self, key: &str, value: &str) {
        let record = FutureRecord::to(TOPIC).key(key).payload(value);
        within("produce", self.producer.send(record, STEP))
            .await
            .unwrap_or_else(|(err, _)| panic!("cannot produce: {err}"));
    }

    async fn next_received(&mut self) -> Received {
        within("the handler to receive a command", self.received.recv())
            .await
            .unwrap()
    }

    async fn next_dead_letter(&self) -> OwnedMessage {
        within("a DLQ message", async {
            loop {
                match self.dlq.recv().await {
                    Ok(message) => return message.detach(),
                    // The group is still forming.
                    Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
                }
            }
        })
        .await
    }

    /// Shuts the consumer down, as the process does on SIGTERM, and checks it stopped cleanly.
    async fn finish(self) {
        self.shutdown.cancel();
        within("the consumer to stop", self.consumer)
            .await
            .unwrap()
            .unwrap();
        // Closing a consumer blocks until it left its group, so not on an async thread, and
        // while the broker still runs.
        let dlq = self.dlq;
        within(
            "the DLQ reader to close",
            tokio::task::spawn_blocking(move || drop(dlq)),
        )
        .await
        .unwrap();
    }
}

fn header(message: &OwnedMessage, name: &str) -> String {
    let headers = message.headers().unwrap();
    let found = headers.iter().find(|header| header.key == name).unwrap();
    String::from_utf8(found.value.unwrap().to_vec()).unwrap()
}

fn id(n: u64) -> String {
    format!("0199c0aa-0000-7000-8000-{n:012}")
}

/// A valid envelope. `n` makes the message id unique.
fn envelope(n: u64, message_type: &str, payload: Value) -> String {
    json!({
        "message_id": id(1000 + n),
        "correlation_id": id(2000 + n),
        "type": message_type,
        "schema_version": 1,
        "occurred_at": "2026-10-09T10:00:00Z",
        "payload": payload,
    })
    .to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kafka_ac_1_dummy_handler_receives_a_valid_command() {
    let mut harness = Harness::start().await;

    let payload = json!({"id": id(1), "name": "Pen", "seq": 1});
    harness
        .produce(&id(1), &envelope(1, "widget.create", payload.clone()))
        .await;

    let received = harness.next_received().await;
    assert_eq!(received.message_type, "widget.create");
    assert_eq!(received.payload, payload);

    harness.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kafka_ac_3_malformed_json_goes_to_dlq_and_the_next_message_is_processed() {
    let mut harness = Harness::start().await;

    harness.produce("k1", "not json").await;
    let payload = json!({"id": id(2), "name": "Ink", "seq": 1});
    harness
        .produce(&id(2), &envelope(2, "widget.create", payload.clone()))
        .await;

    // The consumer kept going: the message after the bad one is handled.
    let received = harness.next_received().await;
    assert_eq!(received.payload, payload);

    let dead = harness.next_dead_letter().await;
    assert_eq!(dead.key(), Some(&b"k1"[..]));
    assert_eq!(dead.payload(), Some(&b"not json"[..]));
    assert_eq!(header(&dead, "error.reason"), "malformed_json");
    assert_eq!(
        header(&dead, "error.detail"),
        "the message value is not valid JSON"
    );

    harness.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kafka_unknown_type_goes_to_dlq() {
    let mut harness = Harness::start().await;

    let unknown = envelope(3, "widget.archive", json!({"id": id(3)}));
    harness.produce(&id(3), &unknown).await;
    let payload = json!({"id": id(4), "name": "Cap", "seq": 1});
    harness
        .produce(&id(4), &envelope(4, "widget.create", payload.clone()))
        .await;

    let dead = harness.next_dead_letter().await;
    assert_eq!(dead.key(), Some(id(3).as_bytes()));
    assert_eq!(dead.payload(), Some(unknown.as_bytes()));
    assert_eq!(header(&dead, "error.reason"), "unknown_type");
    assert_eq!(
        header(&dead, "error.detail"),
        "no handler accepts this type on this topic"
    );

    // The handler never saw the unknown type: the first command it gets is the next one.
    let received = harness.next_received().await;
    assert_eq!(received.payload, payload);

    harness.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kafka_ac_5_commands_for_one_key_are_processed_in_order() {
    const KEYS: u64 = 3;
    const PER_KEY: u64 = 6;
    let mut harness = Harness::start().await;

    // Interleave the keys, so each key's messages are separated by other keys' messages.
    for seq in 0..PER_KEY {
        for key in 0..KEYS {
            let payload = json!({"id": id(key), "seq": seq});
            let n = seq * KEYS + key;
            harness
                .produce(&id(key), &envelope(n, "widget.update", payload))
                .await;
        }
    }

    let mut seen: HashMap<String, Vec<u64>> = HashMap::new();
    for _ in 0..KEYS * PER_KEY {
        let received = harness.next_received().await;
        let key = received.payload["id"].as_str().unwrap().to_owned();
        seen.entry(key)
            .or_default()
            .push(received.payload["seq"].as_u64().unwrap());
    }

    let expected: Vec<u64> = (0..PER_KEY).collect();
    for key in 0..KEYS {
        assert_eq!(seen[&id(key)], expected, "commands for key {key}");
    }

    harness.finish().await;
}
