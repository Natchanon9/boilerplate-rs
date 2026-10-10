//! Composition root: builds the adapters for the configured platform and features, and
//! hands them to the inbound adapters (stack-architecture.md).

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use application::health::HealthCheck;
use axum::Router;
use http_api::auth::JwtVerifier;
use http_api::x_cache::XCache;
use kafka_api::{CommandHandler, Dispatcher};
use secrecy::ExposeSecret;

use crate::config::{AuthConfig, CacheConfig, DatabaseConfig, DbPlatform};
use infrastructure::postgres::PgPool;
use infrastructure::redis::cache_aside::RedisCache;
use infrastructure::{postgres, redis};

/// The connections a process opens, built once by [`Infra::connect`] and handed to `/readyz`,
/// the HTTP resource builders, and the Kafka handler builders, so all of them use the same
/// pool and the same Redis connection. A resource builder never opens its own.
#[derive(Clone)]
pub struct Infra {
    /// The Postgres pool (the only platform in the MVP), lazy: the process starts while the
    /// database is down, and `/readyz` reports it.
    pub pool: PgPool,
    /// `Some` only while `ENABLE_FEATURES` contains `cache`.
    pub cache: Option<CacheInfra>,
}

/// The shared Redis connection and the default TTL the cache decorators are built with.
#[derive(Clone)]
pub struct CacheInfra {
    pub redis: RedisCache,
    /// `CACHE_TTL_SECS`, unless a resource's spec sets its own constant (cache.md AC-5).
    pub ttl: Duration,
}

impl Infra {
    /// Opens the pool for `db.platform` and, when `cache` is `Some`, the Redis connection.
    /// Both are lazy and nothing here connects yet. Call it once per process.
    pub fn connect(db: &DatabaseConfig, cache: Option<&CacheConfig>) -> anyhow::Result<Self> {
        let pool = match db.platform {
            DbPlatform::Postgres => {
                postgres::connect_lazy(db.url.expose_secret()).context("DATABASE_URL is invalid")?
            }
        };
        let cache = cache
            .map(|config| {
                Ok::<_, anyhow::Error>(CacheInfra {
                    redis: redis_cache(config)?,
                    ttl: config.ttl,
                })
            })
            .transpose()?;
        Ok(Self { pool, cache })
    }
}

/// The three routers, ready to serve.
pub struct HttpApp {
    pub protected: Router,
    pub public: Router,
    pub hidden: Router,
}

/// Builds every router on the shared `infra`. `debug_header` is the validated
/// `CACHE_DEBUG_HEADER` (`Settings::cache_debug_header`).
pub fn http_app(infra: &Infra, auth: &AuthConfig, debug_header: bool) -> HttpApp {
    // `/readyz` checks the same pool and Redis connection the resources use, so it reports
    // what they see. It checks Redis only while the cache is enabled.
    let mut checks: Vec<Arc<dyn HealthCheck>> =
        vec![Arc::new(postgres::PgHealthCheck::new(infra.pool.clone()))];
    if let Some(cache) = &infra.cache {
        checks.push(Arc::new(redis::RedisHealthCheck::new(
            cache.redis.connection(),
        )));
    }

    // Resources merge their `/api/v1` routes here. While `infra.cache` is `Some`, each
    // repository is wrapped in its decorator with `cache.ttl`, or the constant its spec sets:
    // `Cached<Res>Repository::new(repo, cache.redis.clone(), ttl)`.
    let api_v1 = Router::new();

    HttpApp {
        protected: protected_router(api_v1, auth, infra.cache.is_some(), debug_header),
        public: http_api::routers::public(),
        hidden: http_api::routers::hidden(checks),
    }
}

/// The Kafka command handlers: the one place a resource registers its
/// `kafka_api::CommandHandler` (layers.md section 6). A resource
/// whose spec declares commands builds its use cases from the shared `infra`, the same
/// instance HTTP uses, and pushes `Arc::new(<Res>CommandHandler::new(use_cases))`. The
/// consumer subscribes to the topics of these handlers, and nothing else names a topic.
/// Rename `_infra` when the first handler uses it.
pub fn kafka_handlers(_infra: &Infra) -> anyhow::Result<Vec<Arc<dyn CommandHandler>>> {
    let handlers: Vec<Arc<dyn CommandHandler>> = Vec::new();
    Ok(handlers)
}

/// The dispatcher over [`kafka_handlers`]. It is empty while no resource declares commands.
pub fn kafka_dispatcher(infra: &Infra) -> anyhow::Result<Dispatcher> {
    Dispatcher::new(kafka_handlers(infra)?).context("cannot register the Kafka handlers")
}

/// The shared cache: a lazy connection, so the service starts while Redis is down and
/// reads fail open (cache.md AC-4), and the `{APP_NAME}:{APP_ENV}:v1` key prefix.
/// The error never contains the URL, which may hold a password (security.md SEC-9).
pub fn redis_cache(cache: &CacheConfig) -> anyhow::Result<RedisCache> {
    let conn =
        redis::connect_lazy(cache.redis_url.expose_secret()).context("REDIS_URL is invalid")?;
    Ok(RedisCache::new(conn, &cache.app_name, &cache.app_env))
}

/// The protected router, with the `X-Cache` middleware only when `debug_header` is set
/// (security.md SEC-6). Without the cache, the middleware reports `BYPASS`.
pub fn protected_router(
    api_v1: Router,
    auth: &AuthConfig,
    cache_enabled: bool,
    debug_header: bool,
) -> Router {
    let x_cache = debug_header.then_some(XCache { cache_enabled });
    http_api::routers::protected(api_v1, jwt_verifier(auth), x_cache)
}

/// The HS256 verifier for the protected router. The secret
/// leaves its `SecretString` only here, as the key bytes.
pub fn jwt_verifier(auth: &AuthConfig) -> JwtVerifier {
    JwtVerifier::hs256(
        auth.jwt_secret.expose_secret().as_bytes(),
        &auth.issuer,
        &auth.audience,
    )
}
