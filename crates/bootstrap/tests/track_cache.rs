//! The tracks cache decorator through the wired router, against real Postgres and Redis
//! containers (the tracks spec AC-8, cache.md AC-1 to AC-3 and AC-5).

// The no-panic rule applies outside tests (security.md SEC-11). clippy.toml allows these in
// #[test] functions only, and integration-test helpers are plain functions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use axum::Router;
use axum::http::{Method, StatusCode};
use bootstrap::wiring::{self, Infra};
use infrastructure::redis::cache_aside::RedisCache;
use serde_json::json;
use support::{
    Reply, STEP, call, create_track, full_token, migrate, postgres_container, redis_container,
    router, track_path, valid_body,
};

/// A TTL that differs from the 300 s default, to show the decorator inherits `CACHE_TTL_SECS`.
const TTL: Duration = Duration::from_secs(123);

async fn get(router: &Router, id: &str) -> Reply {
    call(
        router,
        Method::GET,
        &track_path(id),
        Some(&full_token()),
        None,
    )
    .await
}

fn x_cache(reply: &Reply) -> Option<&str> {
    reply.header("x-cache")
}

async fn raw<T: redis::FromRedisValue>(cache: &RedisCache, cmd: &mut redis::Cmd) -> T {
    let mut conn = cache.connection();
    tokio::time::timeout(STEP, cmd.query_async::<T>(&mut conn))
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn ac_8_first_get_is_a_miss_and_the_second_a_hit() {
    let db = postgres_container().await;
    let (_redis, mut config) = redis_container().await;
    config.ttl = TTL;
    let infra = Infra::connect(&db.config, Some(&config)).unwrap();
    migrate(&infra).await;
    let app = router(&infra);
    let cache = wiring::redis_cache(&config).unwrap();

    let created = create_track(&app, valid_body()).await;
    let id = created["id"].as_str().unwrap();

    let first = get(&app, id).await;
    assert_eq!(
        (first.status, x_cache(&first)),
        (StatusCode::OK, Some("MISS"))
    );
    let second = get(&app, id).await;
    assert_eq!(
        (second.status, x_cache(&second)),
        (StatusCode::OK, Some("HIT"))
    );
    assert_eq!(first.body, created);
    assert_eq!(second.body, created);

    // The value is the row's JSON under the conventional key, with the inherited TTL.
    let key = cache.key("track", id);
    assert_eq!(key, format!("rs-boilerplate:test:v1:track:{id}"));
    let stored: String = raw(&cache, redis::cmd("GET").arg(&key)).await;
    let stored: serde_json::Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(stored["title"], "Clair de lune");
    let ttl: i64 = raw(&cache, redis::cmd("TTL").arg(&key)).await;
    assert!(ttl > 100 && ttl <= 123, "TTL {ttl}");
}

#[tokio::test]
async fn ac_8_put_invalidates_and_the_next_get_is_a_miss_with_the_new_value() {
    let db = postgres_container().await;
    let (_redis, config) = redis_container().await;
    let infra = Infra::connect(&db.config, Some(&config)).unwrap();
    migrate(&infra).await;
    let app = router(&infra);

    let id = create_track(&app, valid_body()).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    get(&app, &id).await;
    assert_eq!(x_cache(&get(&app, &id).await), Some("HIT"));

    let put = call(
        &app,
        Method::PUT,
        &track_path(&id),
        Some(&full_token()),
        Some(json!({"title": "Gymnopedie No. 1", "artist": "Satie", "duration_ms": 190_000})),
    )
    .await;
    assert_eq!(put.status, StatusCode::OK, "{}", put.body);

    let after = get(&app, &id).await;
    assert_eq!(
        (after.status, x_cache(&after)),
        (StatusCode::OK, Some("MISS"))
    );
    assert_eq!(after.body["title"], "Gymnopedie No. 1");
    assert_eq!(after.body["version"], 2);
    assert_eq!(x_cache(&get(&app, &id).await), Some("HIT"));
}

#[tokio::test]
async fn ac_8_delete_invalidates_and_the_next_get_is_404() {
    let db = postgres_container().await;
    let (_redis, config) = redis_container().await;
    let infra = Infra::connect(&db.config, Some(&config)).unwrap();
    migrate(&infra).await;
    let app = router(&infra);
    let cache = wiring::redis_cache(&config).unwrap();

    let id = create_track(&app, valid_body()).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    get(&app, &id).await;
    let key = cache.key("track", &id);
    assert_eq!(raw::<i64>(&cache, redis::cmd("EXISTS").arg(&key)).await, 1);

    let deleted = call(
        &app,
        Method::DELETE,
        &track_path(&id),
        Some(&full_token()),
        None,
    )
    .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    assert_eq!(raw::<i64>(&cache, redis::cmd("EXISTS").arg(&key)).await, 0);

    let after = get(&app, &id).await;
    assert_eq!(
        (after.status, x_cache(&after)),
        (StatusCode::NOT_FOUND, Some("MISS"))
    );
    // No negative caching: a 404 writes no key.
    assert_eq!(raw::<i64>(&cache, redis::cmd("EXISTS").arg(&key)).await, 0);
}

#[tokio::test]
async fn ac_8_without_the_cache_get_is_bypass_and_redis_holds_no_track_keys() {
    let db = postgres_container().await;
    let (_redis, config) = redis_container().await;
    let infra = Infra::connect(&db.config, None).unwrap();
    migrate(&infra).await;
    let app = router(&infra);
    // A client on the same Redis, to prove the app wrote nothing there.
    let probe = wiring::redis_cache(&config).unwrap();

    let id = create_track(&app, valid_body()).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for _ in 0..2 {
        let reply = get(&app, &id).await;
        assert_eq!(
            (reply.status, x_cache(&reply)),
            (StatusCode::OK, Some("BYPASS"))
        );
    }
    let keys: Vec<String> = raw(&probe, redis::cmd("KEYS").arg("*:track:*")).await;
    assert!(keys.is_empty(), "{keys:?}");
}
