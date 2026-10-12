//! Redis connection for the cache-aside decorator (cache.md).
//! The shared helper is [`cache_aside`]; per-resource decorators live next to this file
//! as `<res>_cache.rs`.

pub mod cache_aside;
pub mod track_cache;

use application::health::{HealthCheck, HealthError};
use async_trait::async_trait;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};

#[derive(Debug, thiserror::Error)]
#[error("invalid Redis configuration")]
pub struct RedisConfigError(#[source] redis::RedisError);

/// Builds a multiplexed, auto-reconnecting connection without connecting, so the service
/// starts while Redis is down and the cache fails open. Timeouts are the library defaults
/// (500 ms response, 1 s connection). Reconnect retries are 0, not the default 6: with
/// retries, every cache call during an outage waited about 9.5 s for the whole reconnect
/// cycle. Now each call makes one connection attempt and fails open, and the next call
/// tries again (cache.md C9).
pub fn connect_lazy(url: &str) -> Result<ConnectionManager, RedisConfigError> {
    let client = redis::Client::open(url).map_err(RedisConfigError)?;
    let config = ConnectionManagerConfig::new().set_number_of_retries(0);
    ConnectionManager::new_lazy_with_config(client, config).map_err(RedisConfigError)
}

/// `/readyz` check for Redis, used while the `cache` feature is enabled.
pub struct RedisHealthCheck {
    conn: ConnectionManager,
}

impl RedisHealthCheck {
    pub fn new(conn: ConnectionManager) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl HealthCheck for RedisHealthCheck {
    fn name(&self) -> &'static str {
        "cache"
    }

    async fn check(&self) -> Result<(), HealthError> {
        let mut conn = self.conn.clone();
        redis::cmd("PING")
            .query_async::<String>(&mut conn)
            .await
            .map(drop)
            .map_err(|err| HealthError(err.to_string()))
    }
}
