//! `cargo xtask kafka-produce`: sends a command envelope, or a raw value, to a resource's
//! commands topic (kafka.md).

use std::time::Duration;

use clap::Args;
use clap::builder::NonEmptyStringValueParser;
use rdkafka::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use serde_json::{Map, Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

/// How long one send may take before the task gives up, so a wrong `KAFKA_BROKERS` fails in
/// seconds and not after librdkafka's default `message.timeout.ms`, 300000
/// ([CONFIGURATION.md](https://github.com/confluentinc/librdkafka/blob/v2.12.1/CONFIGURATION.md)).
/// Own proposal.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// The envelope's `schema_version`.
const SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Args)]
pub struct ProduceArgs {
    /// Command type `<res>.<op>`, for example widget.create. The topic defaults to `<res>s.commands.v1`;
    /// use --topic for an irregular plural
    #[arg(long = "type", value_name = "RES.OP", value_parser = NonEmptyStringValueParser::new())]
    pub message_type: String,

    /// JSON object for the envelope payload. Its `id` is the message key; a create without
    /// `id` gets a UUIDv7
    #[arg(long, value_name = "JSON", required_unless_present = "raw", conflicts_with_all = ["raw", "key"])]
    pub payload: Option<String>,

    /// Send this text as the message value, with no envelope (for the DLQ demo)
    #[arg(long, value_name = "TEXT", requires = "key")]
    pub raw: Option<String>,

    /// The message key for --raw
    #[arg(long, value_name = "KEY", requires = "raw")]
    pub key: Option<String>,

    /// Send to this topic instead of `<res>s.commands.v1`. The topic a handler declares is
    /// authoritative; use this when the plural is irregular, for example `categories.commands.v1`
    #[arg(long, value_name = "NAME", value_parser = NonEmptyStringValueParser::new())]
    pub topic: Option<String>,

    /// Send the identical message this many times (to show idempotent handling)
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
    pub repeat: u32,
}

/// Why nothing was sent.
#[derive(Debug, thiserror::Error)]
pub enum ProduceError {
    #[error("KAFKA_BROKERS is required (set it in the environment or .env)")]
    MissingBrokers,
    #[error(
        "--type `{0}` must be <res>.<op> in lowercase letters, digits, and underscores, such as product.create"
    )]
    InvalidType(String),
    #[error("--payload is not valid JSON: {0}")]
    PayloadNotJson(serde_json::Error),
    #[error("--payload must be a JSON object")]
    PayloadNotObject,
    #[error("--payload needs a string `id`, which is the message key (only create may omit it)")]
    InvalidId,
    #[error("cannot format occurred_at: {0}")]
    Time(#[from] time::error::Format),
    #[error("cannot create the Kafka producer: {0}")]
    Producer(String),
    #[error("cannot send to `{topic}`: {cause}")]
    Send { topic: String, cause: String },
    #[error("cannot start the async runtime: {0}")]
    Runtime(#[from] std::io::Error),
}

/// The values the envelope builder cannot derive from the arguments. Injected, so tests are
/// deterministic.
#[derive(Debug, Clone, Copy)]
pub struct Generated {
    pub message_id: Uuid,
    pub correlation_id: Uuid,
    pub occurred_at: OffsetDateTime,
    /// The `payload.id` of a create that has none.
    pub new_entity_id: Uuid,
}

impl Generated {
    /// UUIDv7 ids and the current time.
    pub fn now() -> Self {
        Self {
            message_id: Uuid::now_v7(),
            correlation_id: Uuid::now_v7(),
            occurred_at: OffsetDateTime::now_utc(),
            new_entity_id: Uuid::now_v7(),
        }
    }
}

/// A message ready to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub topic: String,
    pub key: String,
    pub value: String,
    /// `None` for a raw message, which has no envelope.
    pub message_id: Option<Uuid>,
}

/// The commands topic of a resource: `<res>s.commands.v1`. The plural is the resource name plus
/// `s` (own proposal), which is wrong for irregular plurals; `--topic` overrides it
/// (kafka.md K8).
pub fn topic_for(message_type: &str) -> Result<String, ProduceError> {
    let invalid = || ProduceError::InvalidType(message_type.to_owned());
    let (resource, operation) = message_type.split_once('.').ok_or_else(invalid)?;
    let name_ok = |name: &str| {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    if !name_ok(resource) || !name_ok(operation) {
        return Err(invalid());
    }
    Ok(format!("{resource}s.commands.v1"))
}

/// Builds the message for the arguments: a raw value as it is, or the envelope around
/// `--payload`.
pub fn build(args: &ProduceArgs, generated: &Generated) -> Result<Message, ProduceError> {
    let default_topic = topic_for(&args.message_type)?;
    let topic = args.topic.clone().unwrap_or(default_topic);
    if let (Some(raw), Some(key)) = (&args.raw, &args.key) {
        return Ok(Message {
            topic,
            key: key.clone(),
            value: raw.clone(),
            message_id: None,
        });
    }

    let text = args.payload.as_deref().unwrap_or_default();
    let Value::Object(mut payload) =
        serde_json::from_str(text).map_err(ProduceError::PayloadNotJson)?
    else {
        return Err(ProduceError::PayloadNotObject);
    };
    let is_create = args.message_type.ends_with(".create");
    if is_create && !payload.contains_key("id") {
        payload.insert(
            "id".to_owned(),
            Value::String(generated.new_entity_id.to_string()),
        );
    }
    let key = entity_id(&payload)?;
    let envelope = json!({
        "message_id": generated.message_id.to_string(),
        "correlation_id": generated.correlation_id.to_string(),
        "type": args.message_type,
        "schema_version": SCHEMA_VERSION,
        "occurred_at": generated.occurred_at.format(&Rfc3339)?,
        "payload": payload,
    });
    Ok(Message {
        topic,
        key,
        value: envelope.to_string(),
        message_id: Some(generated.message_id),
    })
}

fn entity_id(payload: &Map<String, Value>) -> Result<String, ProduceError> {
    match payload.get("id") {
        Some(Value::String(id)) if !id.is_empty() => Ok(id.clone()),
        _ => Err(ProduceError::InvalidId),
    }
}

/// Builds the message, sends it `--repeat` times, and prints what was sent. Reads
/// `KAFKA_BROKERS` through `env`.
pub fn run(args: &ProduceArgs, env: impl Fn(&str) -> Option<String>) -> Result<(), ProduceError> {
    let brokers = env("KAFKA_BROKERS")
        .filter(|value| !value.trim().is_empty())
        .ok_or(ProduceError::MissingBrokers)?;
    let message = build(args, &Generated::now())?;
    send(brokers.trim(), &message, args.repeat)?;
    if let Some(message_id) = message.message_id {
        println!("message_id: {message_id}");
    }
    println!("key: {}", message.key);
    println!("topic: {}", message.topic);
    println!("sent: {}", args.repeat);
    Ok(())
}

fn send(brokers: &str, message: &Message, repeat: u32) -> Result<(), ProduceError> {
    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("message.timeout.ms", SEND_TIMEOUT.as_millis().to_string())
        .create()
        .map_err(|err| ProduceError::Producer(err.to_string()))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        // One send at a time, so the copies reach the partition in order.
        for _ in 0..repeat {
            let record = FutureRecord::to(&message.topic)
                .key(&message.key)
                .payload(&message.value);
            producer
                .send(record, SEND_TIMEOUT)
                .await
                .map_err(|(err, _)| ProduceError::Send {
                    topic: message.topic.clone(),
                    cause: err.to_string(),
                })?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use time::{Date, Month};

    use super::*;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(flatten)]
        args: ProduceArgs,
    }

    fn args(argv: &[&str]) -> Result<ProduceArgs, clap::Error> {
        let mut full = vec!["kafka-produce"];
        full.extend_from_slice(argv);
        TestCli::try_parse_from(full).map(|cli| cli.args)
    }

    fn generated() -> Generated {
        Generated {
            message_id: Uuid::from_u128(1),
            correlation_id: Uuid::from_u128(2),
            occurred_at: Date::from_calendar_date(2026, Month::October, 10)
                .unwrap()
                .with_hms(12, 30, 0)
                .unwrap()
                .assume_utc(),
            new_entity_id: Uuid::from_u128(3),
        }
    }

    fn envelope_of(message: &Message) -> Value {
        serde_json::from_str(&message.value).unwrap()
    }

    #[test]
    fn topic_overrides_the_default_topic() {
        let payload = r#"{"id": "abc"}"#;
        let plain = args(&["--type", "category.update", "--payload", payload]).unwrap();
        let message = build(&plain, &generated()).unwrap();
        assert_eq!(message.topic, "categorys.commands.v1");

        let named = args(&[
            "--type",
            "category.update",
            "--payload",
            payload,
            "--topic",
            "categories.commands.v1",
        ])
        .unwrap();
        let message = build(&named, &generated()).unwrap();
        assert_eq!(message.topic, "categories.commands.v1");
        assert!(
            args(&[
                "--type",
                "category.update",
                "--payload",
                payload,
                "--topic",
                ""
            ])
            .is_err()
        );

        let raw = args(&[
            "--type",
            "category.update",
            "--raw",
            "x",
            "--key",
            "k",
            "--topic",
            "t.v1",
        ])
        .unwrap();
        assert_eq!(build(&raw, &generated()).unwrap().topic, "t.v1");
    }

    #[test]
    fn the_envelope_has_the_six_fields_and_the_key_is_the_payload_id() {
        let args = args(&[
            "--type",
            "product.update",
            "--payload",
            r#"{"id": "abc", "name": "Pen"}"#,
        ])
        .unwrap();
        let message = build(&args, &generated()).unwrap();

        assert_eq!(message.topic, "products.commands.v1");
        assert_eq!(message.key, "abc");
        assert_eq!(message.message_id, Some(Uuid::from_u128(1)));
        assert_eq!(
            envelope_of(&message),
            json!({
                "message_id": "00000000-0000-0000-0000-000000000001",
                "correlation_id": "00000000-0000-0000-0000-000000000002",
                "type": "product.update",
                "schema_version": 1,
                "occurred_at": "2026-10-10T12:30:00Z",
                "payload": {"id": "abc", "name": "Pen"},
            })
        );
    }

    #[test]
    fn a_create_without_an_id_gets_a_generated_one_that_is_also_the_key() {
        let args = args(&[
            "--type",
            "product.create",
            "--payload",
            r#"{"name": "Pen"}"#,
        ])
        .unwrap();
        let message = build(&args, &generated()).unwrap();

        let id = "00000000-0000-0000-0000-000000000003";
        assert_eq!(message.key, id);
        assert_eq!(
            envelope_of(&message)["payload"],
            json!({"id": id, "name": "Pen"})
        );
    }

    #[test]
    fn a_create_keeps_the_id_it_has() {
        let args = args(&["--type", "product.create", "--payload", r#"{"id": "mine"}"#]).unwrap();
        assert_eq!(build(&args, &generated()).unwrap().key, "mine");
    }

    #[test]
    fn only_a_create_may_omit_the_id() {
        for message_type in ["product.update", "product.delete"] {
            let args = args(&["--type", message_type, "--payload", r#"{"name": "Pen"}"#]).unwrap();
            assert!(
                matches!(build(&args, &generated()), Err(ProduceError::InvalidId)),
                "{message_type}"
            );
        }
        for payload in [r#"{"id": 5}"#, r#"{"id": null}"#, r#"{"id": ""}"#] {
            let args = args(&["--type", "product.create", "--payload", payload]).unwrap();
            assert!(matches!(
                build(&args, &generated()),
                Err(ProduceError::InvalidId)
            ));
        }
    }

    #[test]
    fn a_payload_must_be_a_json_object() {
        let not_json = args(&["--type", "product.create", "--payload", "{"]).unwrap();
        assert!(matches!(
            build(&not_json, &generated()),
            Err(ProduceError::PayloadNotJson(_))
        ));
        let array = args(&["--type", "product.create", "--payload", "[1]"]).unwrap();
        assert!(matches!(
            build(&array, &generated()),
            Err(ProduceError::PayloadNotObject)
        ));
    }

    #[test]
    fn the_topic_is_the_resource_name_plus_s() {
        assert_eq!(topic_for("product.create").unwrap(), "products.commands.v1");
        assert_eq!(
            topic_for("order_line.delete").unwrap(),
            "order_lines.commands.v1"
        );
        for bad in [
            "product",
            "product.",
            ".create",
            "Product.create",
            "a.b.c",
            "a b.c",
        ] {
            assert!(topic_for(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn raw_sends_the_text_and_key_as_they_are_to_the_type_topic() {
        let args = args(&[
            "--raw",
            "not json",
            "--key",
            "k1",
            "--type",
            "product.create",
        ])
        .unwrap();
        assert_eq!(
            build(&args, &generated()).unwrap(),
            Message {
                topic: "products.commands.v1".to_owned(),
                key: "k1".to_owned(),
                value: "not json".to_owned(),
                message_id: None,
            }
        );
    }

    #[test]
    fn the_arguments_exclude_each_other() {
        assert!(args(&["--type", "p.create", "--raw", "x"]).is_err());
        assert!(args(&["--type", "p.create", "--key", "k", "--payload", "{}"]).is_err());
        assert!(
            args(&[
                "--type",
                "p.create",
                "--payload",
                "{}",
                "--raw",
                "x",
                "--key",
                "k"
            ])
            .is_err()
        );
        assert!(args(&["--type", "p.create"]).is_err());
        assert!(args(&["--type", "p.create", "--payload", "{}", "--repeat", "0"]).is_err());
        assert_eq!(
            args(&["--type", "p.create", "--payload", "{}", "--repeat", "3"])
                .unwrap()
                .repeat,
            3
        );
    }
}
