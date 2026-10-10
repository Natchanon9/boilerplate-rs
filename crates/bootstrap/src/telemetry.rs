//! JSON logs through `tracing` (stack-architecture.md).

use tracing_subscriber::EnvFilter;

use crate::config::ConfigError;

/// Installs the global JSON subscriber, filtered by `LOG_LEVEL`.
pub fn init(log_level: &str) -> Result<(), ConfigError> {
    let filter = EnvFilter::try_new(log_level).map_err(|err| ConfigError::Invalid {
        var: "LOG_LEVEL",
        reason: err.to_string(),
    })?;
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .with_current_span(false)
        .init();
    Ok(())
}
