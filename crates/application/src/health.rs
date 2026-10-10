//! Readiness port: each dependency the service needs reports whether it is reachable.
//! Implemented in `infrastructure`, served on the hidden router by `http-api`
//! (runtime.md AC-5).

use async_trait::async_trait;

/// One dependency checked by `/readyz`.
#[async_trait]
pub trait HealthCheck: Send + Sync {
    /// Name shown in the `/readyz` body, for example `database`.
    fn name(&self) -> &'static str;

    /// `Ok` when the dependency answers.
    async fn check(&self) -> Result<(), HealthError>;
}

/// Why a dependency is not ready. Logged, never sent to the client (security.md SEC-6).
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct HealthError(pub String);
