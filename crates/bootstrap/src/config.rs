//! Typed configuration (runtime.md).
//!
//! Every setting is a `clap` argument with `env = "NAME"`, so a flag wins over the
//! environment, which wins over `.env` (loaded in `main` with `dotenvy`, which never
//! overrides a variable that is already set). Each command then resolves only the
//! settings it needs, so `db migrate` does not ask for `AUTH_JWT_SECRET`.
//!
//! Errors name the env var and never print a secret value (stack-architecture.md AC-5,
//! security.md SEC-9).

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use clap::{ArgAction, Args};
use secrecy::{ExposeSecret, SecretString};
use url::Url;

use crate::features::{Feature, Features};

/// Minimum HS256 secret length: at least the hash output, 256 bits
/// ([RFC 7518 3.2](https://www.rfc-editor.org/rfc/rfc7518#section-3.2)).
pub const MIN_JWT_SECRET_BYTES: usize = 32;

/// Shown instead of a secret value.
pub const REDACTED: &str = "REDACTED";

/// The reserved `APP_ENV` value of a production deployment, where `CACHE_DEBUG_HEADER`
/// is refused (own proposal; cache.md C6, security.md SEC-6).
pub const PRODUCTION_ENV: &str = "production";

const DB_RECIPE: &str = "docs/conventions/database.md (Recipe: add a database platform)";

/// The database engine behind the repository ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbPlatform {
    Postgres,
}

impl DbPlatform {
    /// Every supported platform. Add a variant only by following the recipe (database.md).
    pub const ALL: [DbPlatform; 1] = [DbPlatform::Postgres];

    pub fn name(self) -> &'static str {
        match self {
            DbPlatform::Postgres => "postgres",
        }
    }

    /// URL schemes accepted in `DATABASE_URL` for this platform.
    fn url_schemes(self) -> &'static [&'static str] {
        match self {
            DbPlatform::Postgres => &["postgres", "postgresql"],
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|platform| platform.name() == value)
            .ok_or_else(|| {
                let supported: Vec<&str> = Self::ALL.iter().map(|p| p.name()).collect();
                format!(
                    "unsupported platform `{value}`; supported: {}. To add one, follow {DB_RECIPE}",
                    supported.join(", ")
                )
            })
    }
}

fn parse_secret(value: &str) -> Result<SecretString, std::convert::Infallible> {
    Ok(SecretString::from(value))
}

// Raw settings, as parsed by `clap` from flags, the environment, and `.env`.
//
// Secrets can also be passed as flags, but prefer the env var: flags are visible in the
// process list. (These are `//` comments on purpose: a doc comment on a flattened `Args`
// struct replaces the `about` text of `app --help`.)
#[derive(Debug, Clone, Args)]
pub struct Settings {
    /// Service name: the cache key prefix and the Kafka consumer group
    #[arg(
        long,
        env = "APP_NAME",
        default_value = "rs-boilerplate",
        global = true
    )]
    pub app_name: String,

    /// Deployment environment, used in the cache key
    #[arg(long, env = "APP_ENV", default_value = "local", global = true)]
    pub app_env: String,

    /// tracing filter, for example `info` or `info,sqlx=warn`
    #[arg(long, env = "LOG_LEVEL", default_value = "info", global = true)]
    pub log_level: String,

    /// Comma-separated features to switch on: cache, kafka (empty: none)
    #[arg(
        long,
        env = "ENABLE_FEATURES",
        default_value = "",
        hide_default_value = true,
        value_parser = Features::parse_enabled,
        global = true
    )]
    pub enable_features: Features,

    /// Address every router binds to
    #[arg(long, env = "HOST", default_value = "0.0.0.0", global = true)]
    pub host: IpAddr,

    /// Port of the protected router (/api/v1, JWT auth)
    // IANA http-alt (runtime.md)
    #[arg(
        long,
        env = "PROTECTED_API_PORT",
        default_value_t = 8080,
        global = true
    )]
    pub protected_api_port: u16,

    /// Port of the public router (no auth)
    // Neighbour of 8080, own proposal (runtime.md)
    #[arg(long, env = "PUBLIC_API_PORT", default_value_t = 8081, global = true)]
    pub public_api_port: u16,

    /// Port of the hidden router (/healthz, /readyz). It binds HOST like the others, so keep it off the network (compose publishes it on loopback only)
    // Neighbour of 8080, own proposal (runtime.md)
    #[arg(long, env = "HIDDEN_API_PORT", default_value_t = 8079, global = true)]
    pub hidden_api_port: u16,

    /// Database engine
    #[arg(
        long,
        env = "DB_PLATFORM",
        default_value = "postgres",
        value_parser = DbPlatform::parse,
        global = true
    )]
    pub db_platform: DbPlatform,

    /// Database connection URL
    #[arg(long, env = "DATABASE_URL", hide_env_values = true, value_parser = parse_secret, global = true)]
    pub database_url: Option<SecretString>,

    /// Redis connection URL (needed while the cache feature is enabled)
    #[arg(long, env = "REDIS_URL", hide_env_values = true, value_parser = parse_secret, global = true)]
    pub redis_url: Option<SecretString>,

    /// Comma-separated Kafka brokers, host:port (needed while the kafka feature is enabled)
    #[arg(long, env = "KAFKA_BROKERS", global = true)]
    pub kafka_brokers: Option<String>,

    /// HS256 secret for access tokens, at least 32 bytes
    #[arg(long, env = "AUTH_JWT_SECRET", hide_env_values = true, value_parser = parse_secret, global = true)]
    pub auth_jwt_secret: Option<SecretString>,

    /// Required `iss` claim
    #[arg(long, env = "AUTH_ISSUER", global = true)]
    pub auth_issuer: Option<String>,

    /// Required `aud` claim
    #[arg(long, env = "AUTH_AUDIENCE", global = true)]
    pub auth_audience: Option<String>,

    /// Cache TTL in seconds
    // 300 s (cache.md AC-5)
    #[arg(long, env = "CACHE_TTL_SECS", default_value_t = 300, global = true)]
    pub cache_ttl_secs: u64,

    /// Send the X-Cache debug header (development only; refused when APP_ENV=production)
    #[arg(
        long,
        env = "CACHE_DEBUG_HEADER",
        default_value_t = false,
        action = ArgAction::Set,
        global = true
    )]
    pub cache_debug_header: bool,
}

/// A setting that is missing or invalid. The message names the env var and never
/// contains a secret value.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("{var} is required (or pass --{flag})")]
    Missing {
        var: &'static str,
        flag: &'static str,
    },
    #[error("{var} is invalid: {reason}")]
    Invalid { var: &'static str, reason: String },
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub platform: DbPlatform,
    pub url: SecretString,
}

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub protected: SocketAddr,
    pub public: SocketAddr,
    pub hidden: SocketAddr,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub jwt_secret: SecretString,
    pub issuer: String,
    pub audience: String,
}

#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub redis_url: SecretString,
    /// The default TTL, `CACHE_TTL_SECS`. A resource whose spec sets its own TTL uses a
    /// constant instead (cache.md AC-5).
    pub ttl: Duration,
    /// `APP_NAME` and `APP_ENV`, the cache key prefix.
    pub app_name: String,
    pub app_env: String,
}

#[derive(Debug, Clone)]
pub struct KafkaConfig {
    pub brokers: String,
    /// One consumer group per service: `APP_NAME`.
    pub group_id: String,
}

impl Settings {
    pub fn features(&self) -> Features {
        self.enable_features
    }

    pub fn http(&self) -> HttpConfig {
        HttpConfig {
            protected: SocketAddr::new(self.host, self.protected_api_port),
            public: SocketAddr::new(self.host, self.public_api_port),
            hidden: SocketAddr::new(self.host, self.hidden_api_port),
        }
    }

    pub fn database(&self) -> Result<DatabaseConfig, ConfigError> {
        let url = required(&self.database_url, "DATABASE_URL", "database-url")?;
        let parsed = parse_url(url.expose_secret(), "DATABASE_URL")?;
        let schemes = self.db_platform.url_schemes();
        if !schemes.contains(&parsed.scheme()) {
            return Err(ConfigError::Invalid {
                var: "DATABASE_URL",
                reason: format!(
                    "DB_PLATFORM `{}` expects a URL starting with {}",
                    self.db_platform.name(),
                    schemes
                        .iter()
                        .map(|s| format!("{s}://"))
                        .collect::<Vec<_>>()
                        .join(" or ")
                ),
            });
        }
        Ok(DatabaseConfig {
            platform: self.db_platform,
            url: url.clone(),
        })
    }

    pub fn auth(&self) -> Result<AuthConfig, ConfigError> {
        let secret = required(&self.auth_jwt_secret, "AUTH_JWT_SECRET", "auth-jwt-secret")?;
        if secret.expose_secret().len() < MIN_JWT_SECRET_BYTES {
            return Err(ConfigError::Invalid {
                var: "AUTH_JWT_SECRET",
                reason: format!(
                    "must be at least {MIN_JWT_SECRET_BYTES} bytes (RFC 7518 3.2); \
                     generate one with `openssl rand -hex 32`"
                ),
            });
        }
        Ok(AuthConfig {
            jwt_secret: secret.clone(),
            issuer: required_text(&self.auth_issuer, "AUTH_ISSUER", "auth-issuer")?,
            audience: required_text(&self.auth_audience, "AUTH_AUDIENCE", "auth-audience")?,
        })
    }

    /// `None` unless `ENABLE_FEATURES` contains `cache`.
    pub fn cache(&self) -> Result<Option<CacheConfig>, ConfigError> {
        if !self.features().is_enabled(Feature::Cache) {
            return Ok(None);
        }
        let url = required(&self.redis_url, "REDIS_URL", "redis-url")?;
        let parsed = parse_url(url.expose_secret(), "REDIS_URL")?;
        if !matches!(parsed.scheme(), "redis" | "rediss") {
            return Err(ConfigError::Invalid {
                var: "REDIS_URL",
                reason: "expects a URL starting with redis:// or rediss://".to_owned(),
            });
        }
        if self.cache_ttl_secs == 0 {
            return Err(ConfigError::Invalid {
                var: "CACHE_TTL_SECS",
                reason: "must be greater than 0".to_owned(),
            });
        }
        Ok(Some(CacheConfig {
            redis_url: url.clone(),
            ttl: Duration::from_secs(self.cache_ttl_secs),
            app_name: self.app_name.clone(),
            app_env: self.app_env.clone(),
        }))
    }

    /// `CACHE_DEBUG_HEADER`, independent of the `cache` feature, because a cache that is not enabled
    /// still reports `BYPASS`. Refused when `APP_ENV` is `production` in any letter case,
    /// so the header can never reach production clients (cache.md C6, security.md SEC-6).
    pub fn cache_debug_header(&self) -> Result<bool, ConfigError> {
        if self.cache_debug_header && self.app_env.trim().eq_ignore_ascii_case(PRODUCTION_ENV) {
            return Err(ConfigError::Invalid {
                var: "CACHE_DEBUG_HEADER",
                reason: format!(
                    "must be false when APP_ENV={PRODUCTION_ENV} (docs/conventions/security.md SEC-6)"
                ),
            });
        }
        Ok(self.cache_debug_header)
    }

    /// `None` unless `ENABLE_FEATURES` contains `kafka`.
    pub fn kafka(&self) -> Result<Option<KafkaConfig>, ConfigError> {
        if !self.features().is_enabled(Feature::Kafka) {
            return Ok(None);
        }
        let brokers = required_text(&self.kafka_brokers, "KAFKA_BROKERS", "kafka-brokers")?;
        for broker in brokers.split(',').map(str::trim) {
            let valid = broker
                .rsplit_once(':')
                .is_some_and(|(host, port)| !host.is_empty() && port.parse::<u16>().is_ok());
            if !valid {
                return Err(ConfigError::Invalid {
                    var: "KAFKA_BROKERS",
                    reason: format!("`{broker}` is not host:port"),
                });
            }
        }
        Ok(Some(KafkaConfig {
            brokers,
            group_id: self.app_name.clone(),
        }))
    }

    /// The effective configuration as `NAME=value` lines, with every secret redacted
    /// (runtime.md AC-3, security.md SEC-9).
    pub fn redacted_env(&self) -> Vec<(&'static str, String)> {
        let unset = || "<unset>".to_owned();
        let secret_url = |value: &Option<SecretString>| {
            value
                .as_ref()
                .map_or_else(unset, |url| redact_url(url.expose_secret()))
        };
        vec![
            ("APP_NAME", self.app_name.clone()),
            ("APP_ENV", self.app_env.clone()),
            ("LOG_LEVEL", self.log_level.clone()),
            ("ENABLE_FEATURES", self.enable_features.enabled_list()),
            ("HOST", self.host.to_string()),
            ("PROTECTED_API_PORT", self.protected_api_port.to_string()),
            ("PUBLIC_API_PORT", self.public_api_port.to_string()),
            ("HIDDEN_API_PORT", self.hidden_api_port.to_string()),
            ("DB_PLATFORM", self.db_platform.name().to_owned()),
            ("DATABASE_URL", secret_url(&self.database_url)),
            ("REDIS_URL", secret_url(&self.redis_url)),
            (
                "KAFKA_BROKERS",
                self.kafka_brokers.clone().unwrap_or_else(unset),
            ),
            (
                "AUTH_JWT_SECRET",
                self.auth_jwt_secret
                    .as_ref()
                    .map_or_else(unset, |_| REDACTED.to_owned()),
            ),
            (
                "AUTH_ISSUER",
                self.auth_issuer.clone().unwrap_or_else(unset),
            ),
            (
                "AUTH_AUDIENCE",
                self.auth_audience.clone().unwrap_or_else(unset),
            ),
            ("CACHE_TTL_SECS", self.cache_ttl_secs.to_string()),
            ("CACHE_DEBUG_HEADER", self.cache_debug_header.to_string()),
        ]
    }
}

fn required<'a>(
    value: &'a Option<SecretString>,
    var: &'static str,
    flag: &'static str,
) -> Result<&'a SecretString, ConfigError> {
    value
        .as_ref()
        .filter(|secret| !secret.expose_secret().trim().is_empty())
        .ok_or(ConfigError::Missing { var, flag })
}

fn required_text(
    value: &Option<String>,
    var: &'static str,
    flag: &'static str,
) -> Result<String, ConfigError> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .ok_or(ConfigError::Missing { var, flag })
}

/// Parses a URL without ever putting the value in the error.
fn parse_url(value: &str, var: &'static str) -> Result<Url, ConfigError> {
    Url::parse(value).map_err(|err| ConfigError::Invalid {
        var,
        reason: format!("not a valid URL ({err})"),
    })
}

/// Replaces the password, and any `password` query parameter, with `REDACTED`.
fn redact_url(value: &str) -> String {
    let Ok(mut url) = Url::parse(value) else {
        return format!("{REDACTED} (not a valid URL)");
    };
    if url.password().is_some() && url.set_password(Some(REDACTED)).is_err() {
        return REDACTED.to_owned();
    }
    if url.query().is_some() {
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(key, value)| {
                let value = if key.to_ascii_lowercase().contains("password") {
                    REDACTED.to_owned()
                } else {
                    value.into_owned()
                };
                (key.into_owned(), value)
            })
            .collect();
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        settings: Settings,
    }

    /// The CLI with every env lookup removed, so tests ignore the real environment.
    fn command() -> clap::Command {
        <TestCli as clap::CommandFactory>::command().mut_args(|arg| arg.env(None::<&str>))
    }

    fn settings(args: &[&str]) -> Settings {
        let mut argv = vec!["app"];
        argv.extend_from_slice(args);
        let matches = command().try_get_matches_from(argv).unwrap();
        <TestCli as clap::FromArgMatches>::from_arg_matches(&matches)
            .unwrap()
            .settings
    }

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn defaults_follow_the_conventions() {
        let s = settings(&[]);
        assert_eq!(s.http().protected.port(), 8080);
        assert_eq!(s.http().public.port(), 8081);
        assert_eq!(s.http().hidden.port(), 8079);
        assert_eq!(s.db_platform, DbPlatform::Postgres);
        assert_eq!(s.cache_ttl_secs, 300);
        assert!(!s.cache_debug_header);
        assert_eq!(s.features(), Features::default());
    }

    #[test]
    fn missing_database_url_names_the_env_var() {
        let err = settings(&[]).database().unwrap_err();
        assert_eq!(
            err.to_string(),
            "DATABASE_URL is required (or pass --database-url)"
        );
    }

    #[test]
    fn database_url_must_match_the_platform_and_is_never_echoed() {
        let err = settings(&["--database-url", "mysql://u:hunter2@db/app"])
            .database()
            .unwrap_err();
        let message = err.to_string();
        assert!(message.starts_with("DATABASE_URL is invalid"), "{message}");
        assert!(!message.contains("hunter2"), "{message}");
    }

    #[test]
    fn unsupported_db_platform_lists_supported_values_and_the_recipe() {
        let err = command()
            .try_get_matches_from(["app", "--db-platform", "mysql"])
            .unwrap_err()
            .to_string();
        assert!(err.contains("supported: postgres"), "{err}");
        assert!(err.contains("Recipe: add a database platform"), "{err}");
    }

    #[test]
    fn short_jwt_secret_is_rejected_without_echoing_it() {
        let err = settings(&[
            "--auth-jwt-secret",
            "too-short-secret",
            "--auth-issuer",
            "iss",
            "--auth-audience",
            "aud",
        ])
        .auth()
        .unwrap_err();
        let message = err.to_string();
        assert!(
            message.starts_with("AUTH_JWT_SECRET is invalid"),
            "{message}"
        );
        assert!(!message.contains("too-short-secret"), "{message}");
    }

    #[test]
    fn auth_needs_issuer_and_audience() {
        let err = settings(&["--auth-jwt-secret", SECRET]).auth().unwrap_err();
        assert_eq!(
            err,
            ConfigError::Missing {
                var: "AUTH_ISSUER",
                flag: "auth-issuer"
            }
        );
    }

    #[test]
    fn cache_is_off_by_default_and_needs_no_redis_url() {
        assert!(settings(&[]).cache().unwrap().is_none());
        assert!(settings(&[]).kafka().unwrap().is_none());
    }

    #[test]
    fn enabled_cache_needs_redis_url() {
        let err = settings(&["--enable-features", "cache"])
            .cache()
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "REDIS_URL is required (or pass --redis-url)"
        );
    }

    #[test]
    fn cache_config_carries_the_key_prefix_parts() {
        let cache = settings(&[
            "--enable-features",
            "cache",
            "--redis-url",
            "redis://localhost:6379",
            "--app-name",
            "svc",
            "--app-env",
            "staging",
        ])
        .cache()
        .unwrap()
        .unwrap();
        assert_eq!(
            (cache.app_name.as_str(), cache.app_env.as_str()),
            ("svc", "staging")
        );
        assert_eq!(cache.ttl, Duration::from_secs(300));
    }

    #[test]
    fn cache_debug_header_is_refused_in_production() {
        for env in ["production", "Production", " PRODUCTION "] {
            let err = settings(&["--cache-debug-header", "true", "--app-env", env])
                .cache_debug_header()
                .unwrap_err();
            let message = err.to_string();
            assert!(message.contains("CACHE_DEBUG_HEADER"), "{message}");
            assert!(message.contains("APP_ENV=production"), "{message}");
        }
    }

    #[test]
    fn cache_debug_header_is_allowed_outside_production() {
        let s = settings(&["--cache-debug-header", "true", "--app-env", "staging"]);
        assert_eq!(s.cache_debug_header(), Ok(true));
        // Off by default, and production without the header starts.
        assert_eq!(settings(&[]).cache_debug_header(), Ok(false));
        let s = settings(&["--app-env", "production"]);
        assert_eq!(s.cache_debug_header(), Ok(false));
    }

    #[test]
    fn cache_debug_header_does_not_need_the_cache_feature() {
        let s = settings(&["--cache-debug-header", "true"]);
        assert!(s.cache().unwrap().is_none());
        assert_eq!(s.cache_debug_header(), Ok(true));
    }

    #[test]
    fn kafka_brokers_must_be_host_port() {
        let err = settings(&["--enable-features", "kafka", "--kafka-brokers", "localhost"])
            .kafka()
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "KAFKA_BROKERS is invalid: `localhost` is not host:port"
        );
        let kafka = settings(&[
            "--enable-features",
            "kafka",
            "--kafka-brokers",
            "a:9092, b:9092",
            "--app-name",
            "svc",
        ])
        .kafka()
        .unwrap()
        .unwrap();
        assert_eq!(kafka.group_id, "svc");
    }

    #[test]
    fn redacted_env_hides_every_secret() {
        let s = settings(&[
            "--database-url",
            "postgres://app:hunter2@localhost:5432/app?sslpassword=pw2",
            "--redis-url",
            "redis://:redispw@localhost:6379",
            "--auth-jwt-secret",
            SECRET,
        ]);
        let env = s.redacted_env();
        let rendered: String = env.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
        for secret in ["hunter2", "pw2", "redispw", SECRET] {
            assert!(!rendered.contains(secret), "{secret} leaked:\n{rendered}");
        }
        assert!(rendered.contains("DATABASE_URL=postgres://app:REDACTED@localhost:5432/app"));
        assert!(rendered.contains("AUTH_JWT_SECRET=REDACTED"));
    }
}
