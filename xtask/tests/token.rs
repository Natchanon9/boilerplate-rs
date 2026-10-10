//! `cargo xtask token` end to end: `.env` loading, env precedence, and stdout holding only
//! the token (security.md).

// The no-panic rule applies outside tests (security.md SEC-11). clippy.toml allows these in
// #[test] functions only, and integration-test helpers are plain functions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::{Command, Output};

use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde_json::Value;

const SECRET: &str = "0123456789abcdef0123456789abcdef-test-only";

/// A fresh working directory, so the repository's `.env` is not picked up.
fn workdir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rs-boilerplate-xtask-{test}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn xtask(dir: &PathBuf, env: &[(&str, &str)], args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .current_dir(dir)
        .env_clear()
        .envs(env.iter().copied())
        .args(args)
        .output()
        .unwrap()
}

fn decode(token: &str, issuer: &str) -> Value {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_required_spec_claims(&["sub", "exp", "iss", "aud"]);
    validation.set_issuer(&[issuer]);
    validation.set_audience(&["test-audience"]);
    jsonwebtoken::decode::<Value>(
        token,
        &DecodingKey::from_secret(SECRET.as_bytes()),
        &validation,
    )
    .unwrap()
    .claims
}

#[test]
fn security_sec_1_token_reads_dot_env_and_the_environment_wins() {
    let dir = workdir("dotenv");
    std::fs::write(
        dir.join(".env"),
        format!("AUTH_JWT_SECRET={SECRET}\nAUTH_ISSUER=from-dotenv\nAUTH_AUDIENCE=test-audience\n"),
    )
    .unwrap();
    let output = xtask(
        &dir,
        &[("AUTH_ISSUER", "from-env")],
        &[
            "token",
            "--sub",
            "alice",
            "--scope",
            "products:read",
            "--ttl",
            "30m",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // stdout is the token and nothing else, so `TOKEN=$(cargo xtask token ...)` works.
    let stdout = String::from_utf8(output.stdout).unwrap();
    let token = stdout.strip_suffix('\n').unwrap();
    assert!(
        !token.contains('\n') && token.split('.').count() == 3,
        "{stdout}"
    );

    let claims = decode(token, "from-env");
    assert_eq!(claims["sub"], "alice");
    assert_eq!(claims["scope"], "products:read");
    let lifetime = claims["exp"].as_u64().unwrap() - claims["iat"].as_u64().unwrap();
    assert_eq!(lifetime, 1800);
}

#[test]
fn security_sec_9_token_errors_never_print_the_secret() {
    let dir = workdir("short");
    let output = xtask(
        &dir,
        &[
            ("AUTH_JWT_SECRET", "short-secret-value"),
            ("AUTH_ISSUER", "i"),
            ("AUTH_AUDIENCE", "a"),
        ],
        &["token", "--sub", "alice", "--scope", "products:read"],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("AUTH_JWT_SECRET is invalid"), "{stderr}");
    assert!(!stderr.contains("short-secret-value"), "{stderr}");
    assert!(output.stdout.is_empty());
}
