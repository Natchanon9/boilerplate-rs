//! Inbound HTTP adapter: axum routers, DTOs, and problem+json errors.
//!
//! Calls `application` use cases. Never touches sqlx, redis, rdkafka, or `infrastructure`
//! (stack-architecture.md AC-2).

pub mod auth;
pub mod error;
pub mod extract;
pub mod pagination;
pub mod routers;
pub mod track;
pub mod x_cache;
