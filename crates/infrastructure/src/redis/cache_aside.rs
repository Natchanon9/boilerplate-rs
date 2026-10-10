//! Cache-aside, written once and used by every `Cached<Res>Repository`
//! (cache.md).
//!
//! Reads go through [`get_or_load`] and writes call [`invalidate`] after the DB commit.
//! Redis never fails a request: on a Redis error the read falls back to the loader and the
//! write still succeeds (fail-open, cache.md AC-4). Each read records its outcome in
//! `application::cache_status` for the `X-Cache` header.
//!
//! HIT, MISS, and ERROR are `debug` events; Redis failures and undecodable values are
//! warnings (cache.md C5). See them with `LOG_LEVEL=info,infrastructure=debug`.

use std::fmt::Display;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use application::cache_status::{self, CacheStatus};
use redis::aio::ConnectionManager;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Bump when the cached value shape changes.
pub const KEY_VERSION: &str = "v1";

/// The Redis connection and the key prefix, built once in wiring and cloned into every
/// decorator. Cloning is cheap: the connection is multiplexed.
#[derive(Clone)]
pub struct RedisCache {
    conn: ConnectionManager,
    prefix: Arc<str>,
}

impl RedisCache {
    /// `app_name` and `app_env` are `APP_NAME` and `APP_ENV`.
    pub fn new(conn: ConnectionManager, app_name: &str, app_env: &str) -> Self {
        Self {
            conn,
            prefix: key_prefix(app_name, app_env).into(),
        }
    }

    /// `{APP_NAME}:{APP_ENV}:v1:{resource}:{id}` (cache.md). `resource` is the
    /// singular module name, for example `widget`.
    pub fn key(&self, resource: &str, id: impl Display) -> String {
        format!("{}:{resource}:{id}", self.prefix)
    }

    /// The shared connection, for the `/readyz` check.
    pub fn connection(&self) -> ConnectionManager {
        self.conn.clone()
    }
}

impl std::fmt::Debug for RedisCache {
    // The connection info holds the Redis URL, which may hold a password (security.md SEC-9).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisCache")
            .field("prefix", &self.prefix)
            .finish_non_exhaustive()
    }
}

fn key_prefix(app_name: &str, app_env: &str) -> String {
    format!("{app_name}:{app_env}:{KEY_VERSION}")
}

/// Returns the value at `key`, or runs `loader` and caches what it returns.
///
/// - HIT: the cached JSON decodes, and the loader does not run.
/// - MISS: no value, or a value that fails to decode (cache.md C4). The loader runs, and
///   `SET key json EX ttl` follows only when it returns `Some`: there is no negative caching.
/// - ERROR: Redis failed. The loader runs, and nothing is written.
///
/// Errors come only from the loader. Redis errors are logged, never returned.
pub async fn get_or_load<T, E, F, Fut>(
    cache: &RedisCache,
    key: &str,
    ttl: Duration,
    loader: F,
) -> Result<Option<T>, E>
where
    T: Serialize + DeserializeOwned,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Option<T>, E>>,
{
    let mut conn = cache.conn.clone();
    // Bytes, not a String: a value that is not UTF-8 is undecodable data, not a Redis error.
    let cached = redis::cmd("GET")
        .arg(key)
        .query_async::<Option<Vec<u8>>>(&mut conn)
        .await;
    match cached {
        Ok(Some(bytes)) => match serde_json::from_slice::<T>(&bytes) {
            Ok(value) => {
                cache_status::record(CacheStatus::Hit);
                tracing::debug!(key, "cache hit");
                return Ok(Some(value));
            }
            Err(err) => {
                tracing::warn!(key, error = %err, "cached value does not decode; reloading it");
            }
        },
        Ok(None) => {}
        Err(err) => {
            cache_status::record(CacheStatus::Error);
            tracing::warn!(key, error = %err, "cache read failed; reading from the database");
            return loader().await;
        }
    }

    cache_status::record(CacheStatus::Miss);
    tracing::debug!(key, "cache miss");
    let loaded = loader().await?;
    if let Some(value) = &loaded {
        store(&mut conn, key, value, ttl).await;
    }
    Ok(loaded)
}

/// Deletes `key` after a write has committed. A failure is logged and never fails the
/// write: the TTL bounds how long the old value can be served (cache.md AC-5).
pub async fn invalidate(cache: &RedisCache, key: &str) {
    let mut conn = cache.conn.clone();
    if let Err(err) = redis::cmd("DEL")
        .arg(key)
        .query_async::<()>(&mut conn)
        .await
    {
        tracing::warn!(key, error = %err, "cache delete failed; the TTL bounds the stale value");
    }
}

async fn store<T: Serialize>(conn: &mut ConnectionManager, key: &str, value: &T, ttl: Duration) {
    let json = match serde_json::to_vec(value) {
        Ok(json) => json,
        Err(err) => {
            tracing::warn!(key, error = %err, "cannot encode the value; not caching it");
            return;
        }
    };
    // Redis rejects `EX 0`, so a sub-second TTL rounds up to one second.
    let seconds = ttl.as_secs().max(1);
    if let Err(err) = redis::cmd("SET")
        .arg(key)
        .arg(json)
        .arg("EX")
        .arg(seconds)
        .query_async::<()>(conn)
        .await
    {
        tracing::warn!(key, error = %err, "cache write failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A lazy manager spawns its background task, so it needs a Tokio runtime.
    #[tokio::test]
    async fn key_follows_the_convention() {
        let conn = crate::redis::connect_lazy("redis://127.0.0.1:1").unwrap();
        let cache = RedisCache::new(conn, "rs-boilerplate", "local");
        assert_eq!(
            cache.key("product", "0190a6b4-0000-7000-8000-000000000001"),
            "rs-boilerplate:local:v1:product:0190a6b4-0000-7000-8000-000000000001"
        );
    }

    #[test]
    fn key_prefix_carries_app_env_and_version() {
        assert_eq!(key_prefix("svc", "staging"), "svc:staging:v1");
    }
}
