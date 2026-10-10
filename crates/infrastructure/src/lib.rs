//! Infrastructure: database adapters and the Redis cache decorator. The Kafka producer for the DLQ lives in
//! `kafka-api`, so there is no Kafka module here.
//!
//! Implements the ports in `application`. Never depends on an inbound adapter
//! (stack-architecture.md AC-2).

pub mod postgres;
pub mod redis;
