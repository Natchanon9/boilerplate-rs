//! The tracks HTTP API against a real Postgres container, through the wired protected router
//! (the tracks spec AC-1 to AC-7). The cache is off here; AC-8 is in `track_cache.rs`.

// The no-panic rule applies outside tests (security.md SEC-11). clippy.toml allows these in
// #[test] functions only, and integration-test helpers are plain functions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::HashSet;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use support::{
    App, Reply, app, call, create_track, full_token, seed, token, track_path, valid_body,
};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

fn timestamp(track: &Value, field: &str) -> OffsetDateTime {
    OffsetDateTime::parse(track[field].as_str().unwrap(), &Rfc3339).unwrap()
}

async fn post(app: &App, body: Value) -> Reply {
    call(
        &app.router,
        Method::POST,
        "/api/v1/tracks",
        Some(&full_token()),
        Some(body),
    )
    .await
}

async fn put(app: &App, id: &str, body: Value) -> Reply {
    call(
        &app.router,
        Method::PUT,
        &track_path(id),
        Some(&full_token()),
        Some(body),
    )
    .await
}

async fn get(app: &App, path: &str) -> Reply {
    call(&app.router, Method::GET, path, Some(&full_token()), None).await
}

fn pair(field: &str, code: &str) -> (String, String) {
    (field.to_owned(), code.to_owned())
}

fn sorted(mut errors: Vec<(String, String)>) -> Vec<(String, String)> {
    errors.sort();
    errors
}

// AC-1

#[tokio::test]
async fn ac_1_create_returns_201_location_and_the_full_trimmed_track() {
    let app = app().await;
    let reply = post(
        &app,
        json!({"title": "  Clair de lune ", "artist": "  Debussy ", "duration_ms": 300_000}),
    )
    .await;

    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    let track = &reply.body;
    let id = track["id"].as_str().unwrap();
    assert_eq!(
        reply.header("location"),
        Some(format!("/api/v1/tracks/{id}").as_str())
    );
    assert_eq!(Uuid::parse_str(id).unwrap().get_version_num(), 7);
    assert_eq!(track["title"], "Clair de lune");
    assert_eq!(track["artist"], "Debussy");
    assert_eq!(track["duration_ms"], 300_000);
    assert_eq!(track["version"], 1);
    assert_eq!(
        timestamp(track, "created_at"),
        timestamp(track, "updated_at")
    );

    let mut keys: Vec<_> = track.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "artist",
            "created_at",
            "duration_ms",
            "id",
            "title",
            "updated_at",
            "version"
        ]
    );
}

#[tokio::test]
async fn ac_1_duplicate_tracks_are_allowed_and_get_distinct_ids() {
    let app = app().await;
    let first = create_track(&app.router, valid_body()).await;
    let second = create_track(&app.router, valid_body()).await;
    assert_ne!(first["id"], second["id"]);
}

// AC-2

#[tokio::test]
async fn ac_2_every_failing_field_is_listed_in_one_422() {
    let app = app().await;
    let reply = post(
        &app,
        json!({"title": "", "artist": "a\u{7}b", "duration_ms": 0, "id": "x"}),
    )
    .await;
    assert_eq!(
        reply.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        reply.body
    );
    assert_eq!(
        sorted(reply.field_errors()),
        [
            pair("artist", "invalid_format"),
            pair("duration_ms", "out_of_range"),
            pair("id", "unknown_field"),
            pair("title", "too_short"),
        ]
    );
}

#[tokio::test]
async fn ac_2_missing_and_null_fields_are_required() {
    let app = app().await;
    for body in [
        json!({}),
        json!({"title": null, "artist": null, "duration_ms": null}),
    ] {
        let reply = post(&app, body).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            sorted(reply.field_errors()),
            [
                pair("artist", "required"),
                pair("duration_ms", "required"),
                pair("title", "required"),
            ]
        );
    }
}

#[tokio::test]
async fn ac_2_text_length_and_control_character_rules() {
    let app = app().await;
    let cases = [
        (json!("   "), "too_short"),
        (json!("a".repeat(201)), "too_long"),
        (json!("a\u{0}b"), "invalid_format"),
        (json!("a\nb"), "invalid_format"),
        (json!(5), "invalid_type"),
    ];
    for field in ["title", "artist"] {
        for (value, code) in &cases {
            let mut body = valid_body();
            body[field] = value.clone();
            let reply = post(&app, body).await;
            assert_eq!(
                reply.status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{field} {value}"
            );
            assert_eq!(reply.field_errors(), [pair(field, code)], "{field} {value}");
        }
    }
}

#[tokio::test]
async fn ac_2_text_at_the_limits_is_accepted() {
    let app = app().await;
    let mut body = valid_body();
    body["title"] = json!(format!("  {}  ", "a".repeat(200)));
    body["artist"] = json!("a");
    let track = create_track(&app.router, body).await;
    assert_eq!(track["title"].as_str().unwrap().chars().count(), 200);
}

#[tokio::test]
async fn ac_2_duration_rules() {
    let app = app().await;
    let rejected = [
        (json!(86_400_001), "out_of_range"),
        (json!(-5), "out_of_range"),
        (json!("300"), "invalid_type"),
        (json!(1.5), "invalid_type"),
        (json!(2_147_483_648_i64), "invalid_type"),
    ];
    for (value, code) in rejected {
        let mut body = valid_body();
        body["duration_ms"] = value.clone();
        let reply = post(&app, body).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{value}");
        assert_eq!(reply.field_errors(), [pair("duration_ms", code)], "{value}");
    }
    for ok in [1, 86_400_000] {
        let mut body = valid_body();
        body["duration_ms"] = json!(ok);
        assert_eq!(create_track(&app.router, body).await["duration_ms"], ok);
    }
}

#[tokio::test]
async fn ac_2_update_body_follows_the_same_rules() {
    let app = app().await;
    let id = create_track(&app.router, valid_body()).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let reply = put(
        &app,
        &id,
        json!({"title": "", "id": id, "duration_ms": "x"}),
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        sorted(reply.field_errors()),
        [
            pair("artist", "required"),
            pair("duration_ms", "invalid_type"),
            pair("id", "unknown_field"),
            pair("title", "too_short"),
        ]
    );
}

// AC-3

#[tokio::test]
async fn ac_3_get_returns_the_track_or_404() {
    let app = app().await;
    let created = create_track(&app.router, valid_body()).await;
    let id = created["id"].as_str().unwrap();

    let found = get(&app, &track_path(id)).await;
    assert_eq!(found.status, StatusCode::OK);
    assert_eq!(found.body, created);

    let unknown = get(&app, &track_path(&Uuid::now_v7().to_string())).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    let not_a_uuid = get(&app, "/api/v1/tracks/not-a-uuid").await;
    assert_eq!(not_a_uuid.status, StatusCode::NOT_FOUND);
}

// AC-4

#[tokio::test]
async fn ac_4_list_pages_newest_first_without_repeats() {
    let app = app().await;
    seed(&app.infra).await;

    let mut sizes = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    let last_next = loop {
        let path = match &cursor {
            Some(c) => format!("/api/v1/tracks?limit=10&cursor={c}"),
            None => "/api/v1/tracks?limit=10".to_owned(),
        };
        let reply = get(&app, &path).await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
        let data = reply.body["data"].as_array().unwrap();
        sizes.push(data.len());
        ids.extend(data.iter().map(|t| t["id"].as_str().unwrap().to_owned()));
        match reply.body["next_cursor"].as_str() {
            Some(next) => cursor = Some(next.to_owned()),
            None => break reply.body["next_cursor"].clone(),
        }
    };

    assert_eq!(sizes, [10, 10, 5]);
    assert_eq!(last_next, Value::Null);
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), 25, "no repeats");
    let mut newest_first = ids.clone();
    newest_first.sort_by(|a, b| b.cmp(a));
    assert_eq!(ids, newest_first);
    assert_eq!(ids[0], "0192f1b7-f900-7000-8000-000000000019");
}

#[tokio::test]
async fn ac_4_a_new_track_is_first_and_the_default_limit_is_ten() {
    let app = app().await;
    seed(&app.infra).await;
    let created = create_track(&app.router, valid_body()).await;
    let reply = get(&app, "/api/v1/tracks").await;
    let data = reply.body["data"].as_array().unwrap();
    assert_eq!(data.len(), 10);
    assert_eq!(data[0]["id"], created["id"]);
    assert!(reply.body["next_cursor"].is_string());
}

#[tokio::test]
async fn ac_4_bad_list_parameters_are_422() {
    let app = app().await;
    for (path, field, code) in [
        ("/api/v1/tracks?limit=101", "limit", "out_of_range"),
        ("/api/v1/tracks?limit=0", "limit", "out_of_range"),
        ("/api/v1/tracks?sort=title", "sort", "unknown_field"),
        ("/api/v1/tracks?cursor=!!", "cursor", "invalid_format"),
    ] {
        let reply = get(&app, path).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{path}");
        assert_eq!(reply.field_errors(), [pair(field, code)], "{path}");
    }
}

// AC-5

#[tokio::test]
async fn ac_5_put_replaces_the_fields_and_bumps_the_version() {
    let app = app().await;
    let created = create_track(&app.router, valid_body()).await;
    let id = created["id"].as_str().unwrap();

    let reply = put(
        &app,
        id,
        json!({"title": " Gymnopedie No. 1 ", "artist": "Satie", "duration_ms": 190_000}),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let updated = &reply.body;
    assert_eq!(updated["title"], "Gymnopedie No. 1");
    assert_eq!(updated["artist"], "Satie");
    assert_eq!(updated["duration_ms"], 190_000);
    assert_eq!(updated["version"], 2);
    assert_eq!(updated["id"], created["id"]);
    assert_eq!(updated["created_at"], created["created_at"]);
    assert!(timestamp(updated, "updated_at") >= timestamp(&created, "updated_at"));

    let stored = get(&app, &track_path(id)).await;
    assert_eq!(&stored.body, updated);

    let again = put(&app, id, valid_body()).await;
    assert_eq!(again.body["version"], 3);
}

#[tokio::test]
async fn ac_5_put_to_an_unknown_id_is_404_and_an_invalid_body_is_422() {
    let app = app().await;
    let unknown = put(&app, &Uuid::now_v7().to_string(), valid_body()).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);

    let id = create_track(&app.router, valid_body()).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let invalid = put(&app, &id, json!({"title": "x"})).await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
    // Nothing changed.
    let stored = get(&app, &track_path(&id)).await;
    assert_eq!(stored.body["version"], 1);
}

// AC-6

#[tokio::test]
async fn ac_6_delete_removes_the_row() {
    let app = app().await;
    let id = create_track(&app.router, valid_body()).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let path = track_path(&id);

    let deleted = call(
        &app.router,
        Method::DELETE,
        &path,
        Some(&full_token()),
        None,
    )
    .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let second = call(
        &app.router,
        Method::DELETE,
        &path,
        Some(&full_token()),
        None,
    )
    .await;
    assert_eq!(second.status, StatusCode::NOT_FOUND);
    assert_eq!(get(&app, &path).await.status, StatusCode::NOT_FOUND);

    let listed = get(&app, "/api/v1/tracks").await;
    assert!(listed.body["data"].as_array().unwrap().is_empty());
}

// AC-7

fn routes(id: &str) -> Vec<(Method, String, Option<Value>, &'static str)> {
    vec![
        (
            Method::POST,
            "/api/v1/tracks".to_owned(),
            Some(valid_body()),
            "tracks:write",
        ),
        (
            Method::GET,
            "/api/v1/tracks".to_owned(),
            None,
            "tracks:read",
        ),
        (Method::GET, track_path(id), None, "tracks:read"),
        (
            Method::PUT,
            track_path(id),
            Some(valid_body()),
            "tracks:write",
        ),
        (Method::DELETE, track_path(id), None, "tracks:write"),
    ]
}

#[tokio::test]
async fn ac_7_every_route_requires_a_token() {
    let app = app().await;
    let id = Uuid::now_v7().to_string();
    for (method, path, body, _) in routes(&id) {
        let reply = call(&app.router, method.clone(), &path, None, body).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
    let garbage = call(
        &app.router,
        Method::GET,
        "/api/v1/tracks",
        Some("not.a.jwt"),
        None,
    )
    .await;
    assert_eq!(garbage.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ac_7_a_read_only_token_is_refused_every_write() {
    let app = app().await;
    let id = create_track(&app.router, valid_body()).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let read_only = token("tracks:read");
    for (method, path, body, scope) in routes(&id) {
        let reply = call(&app.router, method.clone(), &path, Some(&read_only), body).await;
        if scope == "tracks:write" {
            assert_eq!(reply.status, StatusCode::FORBIDDEN, "{method} {path}");
            let challenge = reply.header("www-authenticate").unwrap();
            assert!(
                challenge.contains(r#"error="insufficient_scope""#),
                "{challenge}"
            );
            assert!(challenge.contains(r#"scope="tracks:write""#), "{challenge}");
        } else {
            assert_eq!(reply.status, StatusCode::OK, "{method} {path}");
        }
    }
    // The refused writes changed nothing.
    let stored = get(&app, &track_path(&id)).await;
    assert_eq!(stored.body["version"], 1);
}

#[tokio::test]
async fn ac_7_a_write_only_token_is_refused_get_and_list() {
    let app = app().await;
    let id = Uuid::now_v7().to_string();
    let write_only = token("tracks:write");
    for (method, path, body, scope) in routes(&id) {
        if scope != "tracks:read" {
            continue;
        }
        let reply = call(&app.router, method.clone(), &path, Some(&write_only), body).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{method} {path}");
        let challenge = reply.header("www-authenticate").unwrap();
        assert!(challenge.contains(r#"scope="tracks:read""#), "{challenge}");
    }
    let created = call(
        &app.router,
        Method::POST,
        "/api/v1/tracks",
        Some(&write_only),
        Some(valid_body()),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
}

#[tokio::test]
async fn ac_7_a_token_for_other_scopes_is_refused_everywhere() {
    let app = app().await;
    let id = Uuid::now_v7().to_string();
    let other = token("widgets:read widgets:write");
    for (method, path, body, _) in routes(&id) {
        let reply = call(&app.router, method.clone(), &path, Some(&other), body).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{method} {path}");
    }
}
