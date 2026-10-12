//! The tracks table, seeds, and Postgres repository against a real Postgres container
//! (the tracks spec AC-9 to AC-11, database.md AC-4).

// The no-panic rule applies outside tests (security.md SEC-11). clippy.toml allows these in
// #[test] functions only, and integration-test helpers are plain functions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use application::track::contract::track_repository_contract;
use bootstrap::wiring::Infra;
use infrastructure::postgres::track_repo::PgTrackRepository;
use infrastructure::redis::track_cache::CachedTrackRepository;
use support::{Database, STEP, migrate, postgres_container, redis_container, workspace_root};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ContainerAsync;
use testcontainers_modules::testcontainers::core::ExecCommand;

struct Psql {
    exit_code: Option<i64>,
    stdout: String,
    stderr: String,
}

/// Runs one statement with `psql` inside the container, so the test needs no SQL client of its
/// own. `VERBOSITY=verbose` puts the SQLSTATE in the error text.
async fn psql(container: &ContainerAsync<Postgres>, sql: &str) -> Psql {
    let command = ExecCommand::new([
        "psql",
        "-U",
        "postgres",
        "-d",
        "postgres",
        "-X",
        "-A",
        "-t",
        "-v",
        "VERBOSITY=verbose",
        "-c",
        sql,
    ]);
    let run = async {
        let mut result = container.exec(command).await.unwrap();
        let stdout = result.stdout_to_vec().await.unwrap();
        let stderr = result.stderr_to_vec().await.unwrap();
        let exit_code = result.exit_code().await.unwrap();
        Psql {
            exit_code,
            stdout: String::from_utf8_lossy(&stdout).trim().to_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        }
    };
    tokio::time::timeout(STEP, run).await.unwrap()
}

async fn migrated() -> (Database, Infra) {
    let db = postgres_container().await;
    let infra = Infra::connect(&db.config, None).unwrap();
    migrate(&infra).await;
    (db, infra)
}

fn insert(title: &str, artist: &str, duration_ms: i64) -> String {
    format!(
        "INSERT INTO tracks (id, title, artist, duration_ms) \
         VALUES (gen_random_uuid(), {title}, {artist}, {duration_ms})"
    )
}

// AC-9

#[tokio::test]
async fn ac_9_raw_inserts_that_break_a_rule_fail_with_a_check_violation() {
    let (db, _infra) = migrated().await;
    let long_artist = "repeat('a', 201)";
    let cases = [
        (insert("'t'", "'a'", 0), "tracks_duration_ms_range"),
        (insert("'t'", "'a'", 86_400_001), "tracks_duration_ms_range"),
        (insert("''", "'a'", 1000), "tracks_title_length"),
        (insert("'t'", long_artist, 1000), "tracks_artist_length"),
        (
            insert("repeat('a', 201)", "'a'", 1000),
            "tracks_title_length",
        ),
        (insert("'t'", "''", 1000), "tracks_artist_length"),
    ];
    for (sql, constraint) in cases {
        let result = psql(&db.container, &sql).await;
        assert_ne!(result.exit_code, Some(0), "{sql}");
        assert!(result.stderr.contains("23514"), "{sql}: {}", result.stderr);
        assert!(
            result.stderr.contains(constraint),
            "{sql}: {}",
            result.stderr
        );
    }
    let count = psql(&db.container, "SELECT count(*) FROM tracks").await;
    assert_eq!(count.stdout, "0");
}

#[tokio::test]
async fn ac_9_the_boundary_values_are_accepted() {
    let (db, _infra) = migrated().await;
    for sql in [
        insert("'t'", "'a'", 1),
        insert("repeat('a', 200)", "repeat('b', 200)", 86_400_000),
    ] {
        let result = psql(&db.container, &sql).await;
        assert_eq!(result.exit_code, Some(0), "{sql}: {}", result.stderr);
    }
    let count = psql(&db.container, "SELECT count(*) FROM tracks").await;
    assert_eq!(count.stdout, "2");
}

// AC-10

fn app(dir: &Path, database_url: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_app"))
        .current_dir(dir)
        .env_clear()
        .env("DATABASE_URL", database_url)
        .args(args)
        .output()
        .unwrap()
}

/// A fresh working directory with a link to the repo's `seeds/`, as `db seed` reads
/// `seeds/<platform>` relative to the working directory. No real `.env` is picked up.
fn workdir() -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("rs-boilerplate-tracks-seed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::os::unix::fs::symlink(workspace_root().join("seeds"), dir.join("seeds")).unwrap();
    dir
}

async fn run_app(dir: &Path, url: &str, args: &'static [&'static str]) -> Output {
    let (dir, url) = (dir.to_path_buf(), url.to_owned());
    let task = tokio::task::spawn_blocking(move || app(&dir, &url, args));
    tokio::time::timeout(STEP, task).await.unwrap().unwrap()
}

#[tokio::test]
async fn ac_10_db_seed_inserts_25_tracks_and_a_second_run_leaves_25() {
    let db = postgres_container().await;
    let dir = workdir();

    let migrate = run_app(&dir, &db.url, &["db", "migrate"]).await;
    assert!(
        migrate.status.success(),
        "{}",
        String::from_utf8_lossy(&migrate.stderr)
    );

    for run in 1..=2 {
        let seed = run_app(&dir, &db.url, &["db", "seed"]).await;
        assert!(
            seed.status.success(),
            "run {run}: {}",
            String::from_utf8_lossy(&seed.stderr)
        );
        let count = psql(&db.container, "SELECT count(*) FROM tracks").await;
        assert_eq!(count.stdout, "25", "after run {run}");
    }

    // Every seeded id is a UUIDv7 (version nibble 7).
    let v7 = psql(
        &db.container,
        "SELECT count(*) FROM tracks WHERE substr(id::text, 15, 1) = '7'",
    )
    .await;
    assert_eq!(v7.stdout, "25");
}

// AC-11

#[tokio::test]
async fn ac_11_postgres_repository_passes_the_contract() {
    let (_db, infra) = migrated().await;
    let repo = PgTrackRepository::new(infra.pool.clone());
    tokio::time::timeout(STEP, track_repository_contract(&repo))
        .await
        .unwrap();
}

#[tokio::test]
async fn ac_11_cached_repository_passes_the_contract_too() {
    let db = postgres_container().await;
    let (_redis, config) = redis_container().await;
    let infra = Infra::connect(&db.config, Some(&config)).unwrap();
    migrate(&infra).await;
    let cache = infra.cache.clone().unwrap();
    let repo = CachedTrackRepository::new(
        PgTrackRepository::new(infra.pool.clone()),
        cache.redis,
        cache.ttl,
    );
    tokio::time::timeout(STEP, track_repository_contract(&repo))
        .await
        .unwrap();
}
