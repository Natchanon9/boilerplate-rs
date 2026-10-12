//! Application layer: use cases and the ports they need.
//!
//! Depends only on `domain` (the dependency is added by the first resource build; none of
//! the shared building blocks needs it yet). Never names a framework such as axum, sqlx, redis,
//! rdkafka, or tower (stack-architecture.md AC-2).

pub mod cache_status;
pub mod error;
pub mod health;
pub mod input;
pub mod pagination;
pub mod track;
