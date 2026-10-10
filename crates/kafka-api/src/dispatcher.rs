//! Routes a command to the handler that owns its type, and derives the subscribed topics from
//! the registered handlers (kafka.md).

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use uuid::Uuid;

use crate::envelope;
use crate::handler::CommandHandler;
use crate::reason::{DlqReason, Rejection};

/// A handler registration that cannot work.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DispatcherError {
    #[error("two Kafka handlers register the command type `{0}`")]
    DuplicateType(String),
    #[error("a Kafka handler declares an empty topic or an empty command type")]
    EmptyName,
}

struct Route {
    topic: String,
    handler: Arc<dyn CommandHandler>,
}

/// The registered handlers, indexed by command type.
pub struct Dispatcher {
    routes: HashMap<String, Route>,
    topics: Vec<String>,
}

/// A command that a handler accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handled {
    pub message_id: Uuid,
    pub message_type: String,
}

impl Dispatcher {
    /// Indexes the handlers. A command type belongs to exactly one handler.
    pub fn new(handlers: Vec<Arc<dyn CommandHandler>>) -> Result<Self, DispatcherError> {
        let mut routes = HashMap::new();
        let mut topics = BTreeSet::new();
        for handler in handlers {
            let topic = handler.topic().to_owned();
            if topic.is_empty() {
                return Err(DispatcherError::EmptyName);
            }
            for message_type in handler.types() {
                if message_type.is_empty() {
                    return Err(DispatcherError::EmptyName);
                }
                let route = Route {
                    topic: topic.clone(),
                    handler: Arc::clone(&handler),
                };
                if routes.insert((*message_type).to_owned(), route).is_some() {
                    return Err(DispatcherError::DuplicateType((*message_type).to_owned()));
                }
            }
            topics.insert(topic);
        }
        Ok(Self {
            routes,
            topics: topics.into_iter().collect(),
        })
    }

    /// `true` when no handler is registered, so there is nothing to consume.
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    /// The topics of the registered handlers, sorted, without duplicates.
    pub fn topics(&self) -> &[String] {
        &self.topics
    }

    /// Checks the envelope and the key, finds the handler, and runs the command. Any failure is
    /// the [`Rejection`] to dead-letter. The envelope is checked before the type, so a message
    /// with a bad key and an unknown type is an `invalid_envelope`.
    pub async fn dispatch(
        &self,
        topic: &str,
        key: Option<&[u8]>,
        value: Option<&[u8]>,
    ) -> Result<Handled, Rejection> {
        let envelope = envelope::parse(key, value)?;
        let message_id = envelope.message_id;
        let route = self
            .routes
            .get(&envelope.message_type)
            .filter(|route| route.topic == topic)
            .ok_or_else(|| {
                Rejection::new(
                    DlqReason::UnknownType,
                    format!(
                        "no handler for type `{}` on topic `{topic}`",
                        envelope.message_type
                    ),
                    Some(message_id),
                )
            })?;
        route
            .handler
            .handle(&envelope.message_type, envelope.payload)
            .await
            .map_err(|error| Rejection::from_app_error(&error, message_id))?;
        Ok(Handled {
            message_id,
            message_type: envelope.message_type,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Mutex;

    use application::error::AppError;
    use async_trait::async_trait;
    use serde_json::{Value, json};

    use super::*;

    /// What a [`Recorder`] answers with.
    pub(crate) type Reply = fn() -> Result<(), AppError>;

    pub(crate) const ID: &str = "0199c0aa-0000-7000-8000-000000000001";

    /// Records every command and answers with a fixed result.
    pub(crate) struct Recorder {
        topic: &'static str,
        types: &'static [&'static str],
        result: Reply,
        pub(crate) seen: Mutex<Vec<(String, Value)>>,
    }

    impl Recorder {
        pub(crate) fn new(
            topic: &'static str,
            types: &'static [&'static str],
            result: Reply,
        ) -> Arc<Self> {
            Arc::new(Self {
                topic,
                types,
                result,
                seen: Mutex::new(Vec::new()),
            })
        }
    }

    #[async_trait]
    impl CommandHandler for Recorder {
        fn topic(&self) -> &str {
            self.topic
        }

        fn types(&self) -> &[&'static str] {
            self.types
        }

        async fn handle(&self, message_type: &str, payload: Value) -> Result<(), AppError> {
            if let Ok(mut seen) = self.seen.lock() {
                seen.push((message_type.to_owned(), payload));
            }
            (self.result)()
        }
    }

    pub(crate) fn command(message_type: &str) -> Vec<u8> {
        json!({
            "message_id": "0199c0aa-1111-7000-8000-000000000002",
            "correlation_id": "0199c0aa-2222-7000-8000-000000000003",
            "type": message_type,
            "schema_version": 1,
            "occurred_at": "2026-10-09T10:00:00Z",
            "payload": {"id": ID, "name": "Pen"},
        })
        .to_string()
        .into_bytes()
    }

    fn products(result: Reply) -> Arc<Recorder> {
        Recorder::new(
            "products.commands.v1",
            &["product.create", "product.update", "product.delete"],
            result,
        )
    }

    fn dispatcher(handlers: Vec<Arc<Recorder>>) -> Dispatcher {
        let handlers: Vec<Arc<dyn CommandHandler>> = handlers
            .into_iter()
            .map(|h| h as Arc<dyn CommandHandler>)
            .collect();
        Dispatcher::new(handlers).unwrap()
    }

    #[tokio::test]
    async fn a_valid_command_reaches_the_handler_with_its_payload() {
        let handler = products(|| Ok(()));
        let dispatcher = dispatcher(vec![Arc::clone(&handler)]);
        let handled = dispatcher
            .dispatch(
                "products.commands.v1",
                Some(ID.as_bytes()),
                Some(&command("product.create")),
            )
            .await
            .unwrap();
        assert_eq!(handled.message_type, "product.create");
        let seen = handler.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "product.create");
        assert_eq!(seen[0].1, json!({"id": ID, "name": "Pen"}));
    }

    #[tokio::test]
    async fn an_unknown_type_is_rejected_without_calling_a_handler() {
        let handler = products(|| Ok(()));
        let dispatcher = dispatcher(vec![Arc::clone(&handler)]);
        let rejection = dispatcher
            .dispatch(
                "products.commands.v1",
                Some(ID.as_bytes()),
                Some(&command("product.archive")),
            )
            .await
            .unwrap_err();
        assert_eq!(rejection.reason, DlqReason::UnknownType);
        assert!(rejection.message_id.is_some());
        assert!(handler.seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_type_owned_by_another_topic_is_an_unknown_type() {
        let products = products(|| Ok(()));
        let categories = Recorder::new("categories.commands.v1", &["category.create"], || Ok(()));
        let dispatcher = dispatcher(vec![Arc::clone(&products), Arc::clone(&categories)]);
        let rejection = dispatcher
            .dispatch(
                "categories.commands.v1",
                Some(ID.as_bytes()),
                Some(&command("product.create")),
            )
            .await
            .unwrap_err();
        assert_eq!(rejection.reason, DlqReason::UnknownType);
        assert!(products.seen.lock().unwrap().is_empty());
        assert!(categories.seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_envelope_is_checked_before_the_type() {
        let dispatcher = dispatcher(vec![products(|| Ok(()))]);
        let rejection = dispatcher
            .dispatch(
                "products.commands.v1",
                None,
                Some(&command("product.archive")),
            )
            .await
            .unwrap_err();
        assert_eq!(rejection.reason, DlqReason::InvalidEnvelope);
    }

    #[tokio::test]
    async fn a_handler_error_becomes_the_mapped_rejection() {
        let cases: [(Reply, DlqReason); 5] = [
            (|| Err(AppError::NotFound), DlqReason::NotFound),
            (
                || Err(AppError::Conflict("taken".into())),
                DlqReason::Conflict,
            ),
            (
                || Err(AppError::Unavailable("down".into())),
                DlqReason::Unavailable,
            ),
            (
                || Err(AppError::Internal("bad".into())),
                DlqReason::Internal,
            ),
            (|| Err(AppError::Validation(vec![])), DlqReason::Validation),
        ];
        for (result, reason) in cases {
            let dispatcher = dispatcher(vec![products(result)]);
            let rejection = dispatcher
                .dispatch(
                    "products.commands.v1",
                    Some(ID.as_bytes()),
                    Some(&command("product.update")),
                )
                .await
                .unwrap_err();
            assert_eq!(rejection.reason, reason);
        }
    }

    #[test]
    fn topics_come_from_the_registered_handlers() {
        let dispatcher = dispatcher(vec![
            Recorder::new("products.commands.v1", &["product.create"], || Ok(())),
            Recorder::new("categories.commands.v1", &["category.create"], || Ok(())),
            Recorder::new("products.commands.v1", &["product.delete"], || Ok(())),
        ]);
        assert_eq!(
            dispatcher.topics(),
            ["categories.commands.v1", "products.commands.v1"]
        );
        assert!(!dispatcher.is_empty());
    }

    #[test]
    fn no_handlers_means_no_topics() {
        let dispatcher = Dispatcher::new(vec![]).unwrap();
        assert!(dispatcher.is_empty());
        assert!(dispatcher.topics().is_empty());
    }

    #[test]
    fn registering_a_type_twice_is_an_error() {
        let a: Arc<dyn CommandHandler> =
            Recorder::new("products.commands.v1", &["product.create"], || Ok(()));
        let b: Arc<dyn CommandHandler> =
            Recorder::new("other.commands.v1", &["product.create"], || Ok(()));
        assert_eq!(
            Dispatcher::new(vec![a, b]).err(),
            Some(DispatcherError::DuplicateType("product.create".to_owned()))
        );
    }

    #[test]
    fn an_empty_topic_or_type_is_an_error() {
        let no_topic: Arc<dyn CommandHandler> = Recorder::new("", &["a.create"], || Ok(()));
        assert_eq!(
            Dispatcher::new(vec![no_topic]).err(),
            Some(DispatcherError::EmptyName)
        );
        let no_type: Arc<dyn CommandHandler> = Recorder::new("a.commands.v1", &[""], || Ok(()));
        assert_eq!(
            Dispatcher::new(vec![no_type]).err(),
            Some(DispatcherError::EmptyName)
        );
    }
}
