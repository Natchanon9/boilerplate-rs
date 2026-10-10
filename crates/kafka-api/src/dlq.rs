//! Writing rejected messages to the DLQ topic (kafka.md AC-3).
//!
//! The producer lives here, in `kafka-api`, and not in `infrastructure` behind a port: the crate
//! map gives the DLQ to this crate, which may use `rdkafka`, and the consumer is the only caller.
//! A port in `application` would add a trait that nothing else implements. `infrastructure`
//! keeps the Kafka producer for events, which are deferred. [`DlqSink`] exists so the consume
//! logic can be unit-tested without a broker.

use std::time::Duration;

use async_trait::async_trait;
use rdkafka::ClientConfig;
use rdkafka::message::{Header, OwnedHeaders};
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::util::Timeout;

use crate::reason::{DETAIL_HEADER, REASON_HEADER, Rejection};

/// The DLQ topic of a commands topic: `<res_plural>.commands.v1.dlq`.
pub fn dlq_topic(topic: &str) -> String {
    format!("{topic}.dlq")
}

/// How long one DLQ write may take before the consumer treats it as failed and stops. A failed
/// write must not hang the consumer for librdkafka's default `message.timeout.ms`, 300000
/// ([CONFIGURATION.md](https://github.com/confluentinc/librdkafka/blob/v2.12.1/CONFIGURATION.md)).
/// Own proposal.
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);

/// A message as the consumer received it.
#[derive(Debug, Clone, Copy)]
pub struct Incoming<'a> {
    pub topic: &'a str,
    pub partition: i32,
    pub offset: i64,
    pub key: Option<&'a [u8]>,
    pub value: Option<&'a [u8]>,
    pub headers: &'a [(String, Option<Vec<u8>>)],
}

/// A message for the DLQ topic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlqRecord {
    pub topic: String,
    pub key: Option<Vec<u8>>,
    pub value: Option<Vec<u8>>,
    pub headers: Vec<(String, Option<Vec<u8>>)>,
}

impl DlqRecord {
    /// The original key, value, and headers, plus `error.reason` and `error.detail`.
    pub fn new(incoming: &Incoming<'_>, rejection: &Rejection) -> Self {
        let mut headers = incoming.headers.to_vec();
        headers.push((
            REASON_HEADER.to_owned(),
            Some(rejection.reason.as_str().as_bytes().to_vec()),
        ));
        headers.push((
            DETAIL_HEADER.to_owned(),
            Some(rejection.detail.as_bytes().to_vec()),
        ));
        Self {
            topic: dlq_topic(incoming.topic),
            key: incoming.key.map(<[u8]>::to_vec),
            value: incoming.value.map(<[u8]>::to_vec),
            headers,
        }
    }
}

/// The DLQ write failed. The consumer stops, and the message is read again on restart.
#[derive(Debug, thiserror::Error)]
pub enum DlqError {
    #[error("cannot create the DLQ producer: {0}")]
    Create(String),
    #[error("cannot write to the DLQ topic `{topic}`: {cause}")]
    Send { topic: String, cause: String },
}

/// Where rejected messages go.
#[async_trait]
pub trait DlqSink: Send + Sync {
    /// Returns once the broker acknowledged the message.
    async fn send(&self, record: DlqRecord) -> Result<(), DlqError>;
}

/// The Kafka DLQ producer. Idempotent, so a retry inside librdkafka cannot duplicate a message
/// ([Kafka docs: `enable.idempotence`](https://kafka.apache.org/documentation/#producerconfigs_enable.idempotence)).
pub struct KafkaDlq {
    producer: FutureProducer,
}

impl KafkaDlq {
    pub fn new(brokers: &str) -> Result<Self, DlqError> {
        let producer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("enable.idempotence", "true")
            .set(
                "message.timeout.ms",
                DELIVERY_TIMEOUT.as_millis().to_string(),
            )
            .create::<FutureProducer>()
            .map_err(|err| DlqError::Create(err.to_string()))?;
        Ok(Self { producer })
    }
}

#[async_trait]
impl DlqSink for KafkaDlq {
    async fn send(&self, record: DlqRecord) -> Result<(), DlqError> {
        let mut headers = OwnedHeaders::new_with_capacity(record.headers.len());
        for (key, value) in &record.headers {
            headers = headers.insert(Header {
                key,
                value: value.as_deref(),
            });
        }
        let mut message = FutureRecord::<[u8], [u8]>::to(&record.topic).headers(headers);
        if let Some(key) = &record.key {
            message = message.key(key);
        }
        if let Some(value) = &record.value {
            message = message.payload(value);
        }
        self.producer
            .send(message, Timeout::After(DELIVERY_TIMEOUT))
            .await
            .map(|_| ())
            .map_err(|(err, _)| DlqError::Send {
                topic: record.topic.clone(),
                cause: err.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reason::DlqReason;

    #[test]
    fn the_dlq_topic_appends_dlq() {
        assert_eq!(
            dlq_topic("products.commands.v1"),
            "products.commands.v1.dlq"
        );
    }

    #[test]
    fn the_dlq_record_keeps_key_value_and_headers_and_adds_the_error_headers() {
        let headers = vec![
            ("traceparent".to_owned(), Some(b"00-abc".to_vec())),
            ("empty".to_owned(), None),
        ];
        let incoming = Incoming {
            topic: "products.commands.v1",
            partition: 2,
            offset: 41,
            key: Some(b"k1"),
            value: Some(b"not json"),
            headers: &headers,
        };
        let rejection = Rejection::new(DlqReason::MalformedJson, "cause", None);

        let record = DlqRecord::new(&incoming, &rejection);

        assert_eq!(record.topic, "products.commands.v1.dlq");
        assert_eq!(record.key.as_deref(), Some(&b"k1"[..]));
        assert_eq!(record.value.as_deref(), Some(&b"not json"[..]));
        assert_eq!(
            record.headers,
            vec![
                ("traceparent".to_owned(), Some(b"00-abc".to_vec())),
                ("empty".to_owned(), None),
                ("error.reason".to_owned(), Some(b"malformed_json".to_vec())),
                (
                    "error.detail".to_owned(),
                    Some(b"the message value is not valid JSON".to_vec())
                ),
            ]
        );
    }

    #[test]
    fn a_message_without_key_or_value_stays_without_them() {
        let incoming = Incoming {
            topic: "t",
            partition: 0,
            offset: 0,
            key: None,
            value: None,
            headers: &[],
        };
        let rejection = Rejection::new(DlqReason::MalformedJson, "no value", None);
        let record = DlqRecord::new(&incoming, &rejection);
        assert_eq!((record.key, record.value), (None, None));
    }
}
