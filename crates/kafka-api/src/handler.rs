//! The port a resource implements to receive Kafka commands (kafka.md).

use application::error::AppError;
use async_trait::async_trait;

/// Handles the commands of one resource. A handler is registered in
/// `bootstrap::wiring::kafka_handlers`, the one place that knows the resources.
///
/// The handler never sees the envelope, offsets, or the DLQ: it gets the command `type` and the
/// `payload`, calls the same application parser and use cases as HTTP, and returns the
/// `AppError`. The consumer turns an error into a DLQ message.
#[async_trait]
pub trait CommandHandler: Send + Sync {
    /// The topic that carries this resource's commands, `<res_plural>.commands.v1`. The
    /// dispatcher subscribes to it, and a command of this handler's types that arrives on any
    /// other topic is rejected as `unknown_type`.
    fn topic(&self) -> &str;

    /// The command types this handler owns, such as `widget.create`.
    fn types(&self) -> &[&'static str];

    /// Runs one command. `payload` is the envelope's payload object; its `id` equals the
    /// message key. Handling the same command twice must give the same final state.
    async fn handle(&self, message_type: &str, payload: serde_json::Value) -> Result<(), AppError>;
}
