//! Domain layer: entities, value objects, domain errors, and domain events.
//!
//! Pure business rules with no IO. May use only `thiserror`, `uuid`, and `time`
//! (stack-architecture.md AC-2).
pub mod track;
