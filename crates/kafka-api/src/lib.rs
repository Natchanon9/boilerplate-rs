//! Inbound Kafka adapter: the generic consumer, envelope, dispatcher, and DLQ
//! (kafka.md).
//!
//! Resources plug in a [`CommandHandler`]; this crate parses the envelope, routes the command,
//! writes failures to the DLQ, and commits offsets. It calls the same `application` use cases
//! as HTTP, through the handlers. Never touches axum, sqlx, redis, or `infrastructure`
//! (stack-architecture.md AC-2).

pub mod consumer;
pub mod dispatcher;
pub mod dlq;
pub mod envelope;
pub mod handler;
pub mod payload;
pub mod reason;

pub use consumer::{ConsumerConfig, ConsumerError, run};
pub use dispatcher::{Dispatcher, DispatcherError};
pub use handler::CommandHandler;
pub use reason::DlqReason;
