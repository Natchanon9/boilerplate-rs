//! `cargo xtask token`: signs an HS256 access token the app accepts
//! (security.md).

use std::time::Duration;

use clap::Args;
use clap::builder::NonEmptyStringValueParser;
use jsonwebtoken::{Algorithm, EncodingKey, Header, get_current_timestamp};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;

/// Minimum HS256 secret length, the same rule as the app: at least the hash output, 256 bits
/// ([RFC 7518 3.2](https://www.rfc-editor.org/rfc/rfc7518#section-3.2)).
const MIN_JWT_SECRET_BYTES: usize = 32;

#[derive(Debug, Args)]
pub struct TokenArgs {
    /// Subject (`sub` claim), for example `alice`
    #[arg(long, value_parser = NonEmptyStringValueParser::new())]
    pub sub: String,

    /// Space-delimited scopes (`scope` claim), as `<res_plural>:read` and `<res_plural>:write`, for example "widgets:read widgets:write"
    #[arg(long, value_parser = NonEmptyStringValueParser::new())]
    pub scope: String,

    /// Lifetime: <n>s, <n>m, or <n>h
    // 1h: one demo session, own proposal
    #[arg(long, default_value = "1h", value_parser = parse_ttl)]
    pub ttl: Duration,
}

/// Why no token was signed. Names the env var and never contains the secret (security.md SEC-9).
#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("{0} is required (set it in the environment or .env)")]
    Missing(&'static str),
    #[error(
        "AUTH_JWT_SECRET is invalid: must be at least {MIN_JWT_SECRET_BYTES} bytes \
         (RFC 7518 3.2); generate one with `openssl rand -hex 32`"
    )]
    ShortSecret,
    #[error("cannot sign the token: {0}")]
    Sign(#[from] jsonwebtoken::errors::Error),
}

#[derive(Debug, Serialize)]
struct Claims<'a> {
    sub: &'a str,
    scope: &'a str,
    iss: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

/// Reads `AUTH_JWT_SECRET`, `AUTH_ISSUER`, and `AUTH_AUDIENCE` through `env` and signs a
/// token that is valid from now for `--ttl`.
pub fn sign_from_env(
    args: &TokenArgs,
    env: impl Fn(&str) -> Option<String>,
) -> Result<String, TokenError> {
    let read = |name: &'static str| {
        env(name)
            .filter(|value| !value.trim().is_empty())
            .ok_or(TokenError::Missing(name))
    };
    let secret = SecretString::from(read("AUTH_JWT_SECRET")?);
    if secret.expose_secret().len() < MIN_JWT_SECRET_BYTES {
        return Err(TokenError::ShortSecret);
    }
    let issuer = read("AUTH_ISSUER")?;
    let audience = read("AUTH_AUDIENCE")?;
    let now = get_current_timestamp();
    let claims = Claims {
        sub: &args.sub,
        scope: &args.scope,
        iss: issuer.trim(),
        aud: audience.trim(),
        iat: now,
        exp: now.saturating_add(args.ttl.as_secs()),
    };
    let key = EncodingKey::from_secret(secret.expose_secret().as_bytes());
    Ok(jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &key,
    )?)
}

/// `30s`, `30m`, or `1h`. Zero is rejected.
fn parse_ttl(text: &str) -> Result<Duration, String> {
    const EXPECTED: &str = "expected <n>s, <n>m, or <n>h, for example 30m";
    let (number, unit_secs) = if let Some(n) = text.strip_suffix('s') {
        (n, 1)
    } else if let Some(n) = text.strip_suffix('m') {
        (n, 60)
    } else if let Some(n) = text.strip_suffix('h') {
        (n, 3600)
    } else {
        return Err(EXPECTED.to_owned());
    };
    let count: u64 = number.parse().map_err(|_| EXPECTED.to_owned())?;
    if count == 0 {
        return Err("must be greater than 0".to_owned());
    }
    count
        .checked_mul(unit_secs)
        .map(Duration::from_secs)
        .ok_or_else(|| "is too long".to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use clap::Parser;
    use jsonwebtoken::{DecodingKey, Validation};
    use serde_json::Value;

    use super::*;

    const SECRET: &str = "0123456789abcdef0123456789abcdef-test-only";

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(flatten)]
        args: TokenArgs,
    }

    fn args(argv: &[&str]) -> Result<TokenArgs, clap::Error> {
        let mut full = vec!["token"];
        full.extend_from_slice(argv);
        TestCli::try_parse_from(full).map(|cli| cli.args)
    }

    fn env(pairs: &[(&'static str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    fn full_env() -> impl Fn(&str) -> Option<String> {
        env(&[
            ("AUTH_JWT_SECRET", SECRET),
            ("AUTH_ISSUER", "test-issuer"),
            ("AUTH_AUDIENCE", "test-audience"),
        ])
    }

    #[test]
    fn security_sec_1_token_verifies_with_the_app_rules() {
        let args = args(&["--sub", "alice", "--scope", "products:read products:write"]).unwrap();
        let token = sign_from_env(&args, full_env()).unwrap();

        // The same rules as http_api::auth::JwtVerifier.
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_required_spec_claims(&["sub", "exp", "iss", "aud"]);
        validation.set_issuer(&["test-issuer"]);
        validation.set_audience(&["test-audience"]);
        let data = jsonwebtoken::decode::<Value>(
            &token,
            &DecodingKey::from_secret(SECRET.as_bytes()),
            &validation,
        )
        .unwrap();
        assert_eq!(data.header.alg, Algorithm::HS256);
        let claims = data.claims;
        assert_eq!(claims["sub"], "alice");
        assert_eq!(claims["scope"], "products:read products:write");
        let (iat, exp) = (
            claims["iat"].as_u64().unwrap(),
            claims["exp"].as_u64().unwrap(),
        );
        assert_eq!(exp - iat, 3600, "--ttl defaults to 1h");
    }

    #[test]
    fn security_sec_1_sub_and_scope_are_required() {
        assert!(args(&["--scope", "products:read"]).is_err());
        assert!(args(&["--sub", "alice"]).is_err());
        assert!(args(&["--sub", "", "--scope", "products:read"]).is_err());
        let err = args(&["--sub", "alice", "--scope", "x", "--ttl", "5x"])
            .unwrap_err()
            .to_string();
        assert!(err.contains("--ttl"), "{err}");
    }

    #[test]
    fn security_sec_1_ttl_accepts_seconds_minutes_and_hours() {
        assert_eq!(parse_ttl("3600s"), Ok(Duration::from_secs(3600)));
        assert_eq!(parse_ttl("30m"), Ok(Duration::from_secs(1800)));
        assert_eq!(parse_ttl("1h"), Ok(Duration::from_secs(3600)));
        for bad in ["", "h", "1", "1d", "-1h", "1.5h", "0m", "1 h"] {
            assert!(parse_ttl(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn security_sec_9_short_secret_is_rejected_without_printing_it() {
        let args = args(&["--sub", "alice", "--scope", "products:read"]).unwrap();
        let err = sign_from_env(
            &args,
            env(&[
                ("AUTH_JWT_SECRET", "too-short-secret"),
                ("AUTH_ISSUER", "i"),
                ("AUTH_AUDIENCE", "a"),
            ]),
        )
        .unwrap_err()
        .to_string();
        assert!(err.starts_with("AUTH_JWT_SECRET is invalid"), "{err}");
        assert!(!err.contains("too-short-secret"), "{err}");
    }

    #[test]
    fn security_sec_9_missing_settings_name_the_env_var() {
        let args = args(&["--sub", "alice", "--scope", "products:read"]).unwrap();
        let err = sign_from_env(&args, env(&[])).unwrap_err().to_string();
        assert_eq!(
            err,
            "AUTH_JWT_SECRET is required (set it in the environment or .env)"
        );
        let err = sign_from_env(&args, env(&[("AUTH_JWT_SECRET", SECRET)]))
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("AUTH_ISSUER is required"), "{err}");
    }
}
