//! The `app` binary end to end: help, precedence, redaction, and startup errors
//! (runtime.md AC-1 to AC-4, stack-architecture.md AC-5, database.md AC-1).

// The no-panic rule applies outside tests (security.md SEC-11). clippy.toml allows these in
// #[test] functions only, and integration-test helpers are plain functions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::{Command, Output};

const SECRET: &str = "0123456789abcdef0123456789abcdef-test-only";

/// A fresh, empty working directory, so no real `.env` is picked up.
fn workdir(test: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("rs-boilerplate-cli-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs `app` with only the given environment variables.
fn app(dir: &PathBuf, env: &[(&str, &str)], args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_app"))
        .current_dir(dir)
        .env_clear()
        .envs(env.iter().copied())
        .args(args)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn runtime_ac_1_help_works_on_every_command() {
    let dir = workdir("help");
    let commands: &[&[&str]] = &[
        &[],
        &["serve"],
        &["consume"],
        &["all"],
        &["db"],
        &["db", "migrate"],
        &["db", "rollback"],
        &["db", "seed"],
        &["info"],
        &["info", "env"],
        &["info", "features"],
    ];
    for command in commands {
        let mut args = command.to_vec();
        args.push("--help");
        let output = app(&dir, &[], &args);
        assert!(output.status.success(), "{args:?}: {}", stderr(&output));
        assert!(stdout(&output).contains("Usage: app"), "{args:?}");
    }
}

#[test]
fn runtime_ac_1_top_level_help_shows_the_about_text() {
    let dir = workdir("help-about");
    let output = app(&dir, &[], &["--help"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let help = stdout(&output);
    assert_eq!(
        help.lines().next(),
        Some("rs-boilerplate service"),
        "{help}"
    );
}

#[test]
fn runtime_ac_2_flag_beats_env_beats_dotenv() {
    let dir = workdir("precedence");
    std::fs::write(
        dir.join(".env"),
        "APP_NAME=from-dotenv\nAPP_ENV=from-dotenv\nLOG_LEVEL=from-dotenv\n",
    )
    .unwrap();
    let output = app(
        &dir,
        &[("APP_ENV", "from-env"), ("LOG_LEVEL", "from-env")],
        &["--log-level", "from-flag", "info", "env"],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("APP_NAME=from-dotenv\n"), "{out}");
    assert!(out.contains("APP_ENV=from-env\n"), "{out}");
    assert!(out.contains("LOG_LEVEL=from-flag\n"), "{out}");
}

#[test]
fn runtime_ac_3_info_env_redacts_secrets() {
    let dir = workdir("redact");
    let output = app(
        &dir,
        &[
            (
                "DATABASE_URL",
                "postgres://app:db-password@localhost:5432/app",
            ),
            ("REDIS_URL", "redis://:redis-password@localhost:6379"),
            ("AUTH_JWT_SECRET", SECRET),
        ],
        &["info", "env"],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let out = stdout(&output);
    for secret in ["db-password", "redis-password", SECRET] {
        assert!(!out.contains(secret), "{secret} leaked:\n{out}");
    }
    assert!(
        out.contains("DATABASE_URL=postgres://app:REDACTED@localhost:5432/app\n"),
        "{out}"
    );
    assert!(out.contains("AUTH_JWT_SECRET=REDACTED\n"), "{out}");
}

#[test]
fn runtime_ac_4_info_features_lists_each_feature() {
    let dir = workdir("features");
    let output = app(&dir, &[("ENABLE_FEATURES", "cache")], &["info", "features"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "cache    enabled\nkafka    disabled\n");
}

#[test]
fn runtime_ac_4_every_feature_is_off_by_default() {
    let dir = workdir("features-default");
    let output = app(&dir, &[], &["info", "features"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "cache    disabled\nkafka    disabled\n");
}

#[test]
fn runtime_ac_4_unknown_feature_stops_startup() {
    let dir = workdir("unknown-feature");
    let output = app(
        &dir,
        &[("ENABLE_FEATURES", "cache,metrics")],
        &["info", "features"],
    );
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(err.contains("ENABLE_FEATURES"), "{err}");
    assert!(err.contains("supported: cache, kafka"), "{err}");
}

#[test]
fn runtime_ac_4_consume_refuses_when_kafka_is_not_enabled() {
    let dir = workdir("consume-disabled");
    let output = app(&dir, &[], &["consume"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("`kafka` is not in ENABLE_FEATURES"));
}

#[test]
fn kafka_consume_refuses_when_no_handler_is_registered() {
    let dir = workdir("consume-no-handlers");
    let output = app(
        &dir,
        &[
            ("DATABASE_URL", "postgres://app:app@localhost:5432/app"),
            ("ENABLE_FEATURES", "kafka"),
            ("KAFKA_BROKERS", "localhost:9092"),
        ],
        &["consume"],
    );
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(
        err.contains("no Kafka command handler is registered"),
        "{err}"
    );
}

#[test]
fn stack_ac_5_missing_setting_names_the_env_var() {
    let dir = workdir("missing");
    let output = app(&dir, &[], &["db", "migrate"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("DATABASE_URL is required"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn stack_ac_5_short_secret_stops_serve_without_printing_it() {
    let dir = workdir("short-secret");
    let output = app(
        &dir,
        &[
            ("DATABASE_URL", "postgres://app:app@localhost:5432/app"),
            ("AUTH_JWT_SECRET", "short-secret-value"),
        ],
        &["serve"],
    );
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(err.contains("AUTH_JWT_SECRET is invalid"), "{err}");
    assert!(!err.contains("short-secret-value"), "{err}");
    assert!(!stdout(&output).contains("short-secret-value"));
}

#[test]
fn stack_ac_5_a_malformed_env_line_is_not_printed() {
    let dir = workdir("malformed-env");
    std::fs::write(
        dir.join(".env"),
        "AUTH_JWT_SECRET=\"unterminated-secret-value\n",
    )
    .unwrap();
    let output = app(&dir, &[], &["info", "env"]);
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(err.contains("cannot load .env"), "{err}");
    assert!(!err.contains("unterminated-secret-value"), "{err}");
    assert!(!stdout(&output).contains("unterminated-secret-value"));
}

#[test]
fn database_ac_1_unsupported_platform_points_to_the_recipe() {
    let dir = workdir("platform");
    let output = app(&dir, &[("DB_PLATFORM", "sqlite")], &["db", "migrate"]);
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(err.contains("supported: postgres"), "{err}");
    assert!(err.contains("docs/conventions/database.md"), "{err}");
}

#[test]
fn cache_debug_header_stops_startup_in_production() {
    let dir = workdir("debug-header-production");
    let env = [
        ("DATABASE_URL", "postgres://app:app@localhost:5432/app"),
        ("AUTH_JWT_SECRET", SECRET),
        ("AUTH_ISSUER", "iss"),
        ("AUTH_AUDIENCE", "aud"),
        ("APP_ENV", "production"),
        ("CACHE_DEBUG_HEADER", "true"),
    ];
    for command in ["serve", "all"] {
        let output = app(&dir, &env, &[command]);
        assert!(!output.status.success(), "{command} started");
        let err = stderr(&output);
        assert!(
            err.contains("CACHE_DEBUG_HEADER is invalid"),
            "{command}: {err}"
        );
        assert!(err.contains("APP_ENV=production"), "{command}: {err}");
    }
}
