//! The cache outcome of the current request, for the `X-Cache` debug header
//! (cache.md C6).
//!
//! The cache decorator in `infrastructure` records the outcome, and the `x_cache` middleware
//! in `http-api` reads it. Neither may name the other, so the slot lives here, the one crate
//! both can reach. Use cases never touch it.

use std::cell::Cell;
use std::future::Future;

/// What the cache did for a read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheStatus {
    /// The value came from Redis.
    Hit,
    /// Redis had no usable value, so the read went to the database.
    Miss,
    /// The `cache` feature is not enabled, so Redis was not consulted.
    Bypass,
    /// Redis failed, and the read fell back to the database (fail-open).
    Error,
}

impl CacheStatus {
    /// The `X-Cache` header value.
    pub fn as_str(self) -> &'static str {
        match self {
            CacheStatus::Hit => "HIT",
            CacheStatus::Miss => "MISS",
            CacheStatus::Bypass => "BYPASS",
            CacheStatus::Error => "ERROR",
        }
    }
}

tokio::task_local! {
    static CACHE_STATUS: Cell<Option<CacheStatus>>;
}

/// Records the outcome for the enclosing [`scope`]. Outside a scope, for example on the
/// Kafka path, it does nothing. The last outcome recorded in a scope wins.
pub fn record(status: CacheStatus) {
    // `try_with` fails only when no scope is set, which is the expected no-op case.
    let _ = CACHE_STATUS.try_with(|slot| slot.set(Some(status)));
}

/// Runs `future` with an empty slot, or with `initial` already recorded, and returns its
/// output together with the outcome recorded by the end.
pub async fn scope<F: Future>(
    initial: Option<CacheStatus>,
    future: F,
) -> (F::Output, Option<CacheStatus>) {
    CACHE_STATUS
        .scope(Cell::new(initial), async move {
            let output = future.await;
            let status = CACHE_STATUS.try_with(Cell::get).ok().flatten();
            (output, status)
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_values_match_the_convention() {
        let values = [
            CacheStatus::Hit,
            CacheStatus::Miss,
            CacheStatus::Bypass,
            CacheStatus::Error,
        ]
        .map(CacheStatus::as_str);
        assert_eq!(values, ["HIT", "MISS", "BYPASS", "ERROR"]);
    }

    #[test]
    fn record_without_a_scope_does_nothing() {
        record(CacheStatus::Hit);
    }

    #[tokio::test]
    async fn record_without_a_scope_does_nothing_inside_a_task() {
        tokio::spawn(async { record(CacheStatus::Miss) })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn scope_returns_the_recorded_outcome() {
        let (output, status) = scope(None, async {
            record(CacheStatus::Miss);
            42
        })
        .await;
        assert_eq!((output, status), (42, Some(CacheStatus::Miss)));
    }

    #[tokio::test]
    async fn scope_with_nothing_recorded_returns_the_initial_value() {
        assert_eq!(scope(None, async {}).await.1, None);
        assert_eq!(
            scope(Some(CacheStatus::Bypass), async {}).await.1,
            Some(CacheStatus::Bypass)
        );
    }

    #[tokio::test]
    async fn the_last_outcome_wins_and_survives_an_await() {
        let (_, status) = scope(None, async {
            record(CacheStatus::Error);
            tokio::task::yield_now().await;
            record(CacheStatus::Hit);
        })
        .await;
        assert_eq!(status, Some(CacheStatus::Hit));
    }

    #[tokio::test]
    async fn scopes_do_not_leak_into_each_other() {
        let (_, outer) = scope(None, async {
            let (_, inner) = scope(None, async { record(CacheStatus::Hit) }).await;
            assert_eq!(inner, Some(CacheStatus::Hit));
        })
        .await;
        assert_eq!(outer, None);
    }
}
