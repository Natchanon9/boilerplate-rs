//! `serve`, `consume`, and `all` (runtime.md AC-1, AC-4, AC-5).

use std::net::SocketAddr;

use anyhow::{Context, bail};
use axum::Router;
use kafka_api::{ConsumerConfig, Dispatcher};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::config::{AuthConfig, KafkaConfig, Settings};
use crate::features::Feature;
use crate::wiring::{self, Infra};

/// Runs the protected, public, and hidden routers until shutdown.
pub async fn serve(settings: &Settings, shutdown: CancellationToken) -> anyhow::Result<()> {
    let db = settings.database()?;
    let auth = settings.auth()?;
    let cache = settings.cache()?;
    // CACHE_DEBUG_HEADER=true with APP_ENV=production stops startup (security.md SEC-6).
    let debug_header = settings.cache_debug_header()?;
    log_features(settings);

    let infra = Infra::connect(&db, cache.as_ref())?;
    serve_http(settings, &infra, &auth, debug_header, shutdown).await
}

/// Runs the routers on `infra`, which the caller built once for the whole process.
async fn serve_http(
    settings: &Settings,
    infra: &Infra,
    auth: &AuthConfig,
    debug_header: bool,
    shutdown: CancellationToken,
) -> anyhow::Result<()> {
    let http = settings.http();
    let app = wiring::http_app(infra, auth, debug_header);
    tokio::try_join!(
        run_router(
            "protected",
            "PROTECTED_API_PORT",
            http.protected,
            app.protected,
            shutdown.clone()
        ),
        run_router(
            "public",
            "PUBLIC_API_PORT",
            http.public,
            app.public,
            shutdown.clone()
        ),
        run_router(
            "hidden",
            "HIDDEN_API_PORT",
            http.hidden,
            app.hidden,
            shutdown
        ),
    )?;
    Ok(())
}

/// Runs the Kafka consumer until shutdown. Refuses to start unless `kafka` is enabled or when no
/// resource registered a command handler.
pub async fn consume(settings: &Settings, shutdown: CancellationToken) -> anyhow::Result<()> {
    let Some(kafka) = settings.kafka()? else {
        bail!("consume refuses to start: `kafka` is not in ENABLE_FEATURES");
    };
    let db = settings.database()?;
    let cache = settings.cache()?;
    log_features(settings);
    let infra = Infra::connect(&db, cache.as_ref())?;
    let dispatcher = wiring::kafka_dispatcher(&infra)?;
    if dispatcher.is_empty() {
        bail!(
            "consume refuses to start: no Kafka command handler is registered, so there is no \
             topic to consume (a resource registers its handler in wiring::kafka_handlers)"
        );
    }
    run_consumer(&kafka, &dispatcher, shutdown).await
}

/// Runs the HTTP routers and, only when `kafka` is enabled and a handler is registered, the
/// consumer in one process.
pub async fn all(settings: &Settings, shutdown: CancellationToken) -> anyhow::Result<()> {
    // Validate everything first, so a bad setting never starts half the process.
    let db = settings.database()?;
    let auth = settings.auth()?;
    let cache = settings.cache()?;
    let debug_header = settings.cache_debug_header()?;
    let kafka = settings.kafka()?;
    log_features(settings);
    // One pool and one Redis connection for the whole process: HTTP, Kafka, and `/readyz`.
    let infra = Infra::connect(&db, cache.as_ref())?;
    let dispatcher = wiring::kafka_dispatcher(&infra)?;
    match kafka {
        Some(kafka) if !dispatcher.is_empty() => {
            tokio::try_join!(
                serve_http(settings, &infra, &auth, debug_header, shutdown.clone()),
                run_consumer(&kafka, &dispatcher, shutdown)
            )?;
        }
        Some(_) => {
            tracing::warn!(
                "no Kafka command handler is registered, so the consumer is not started; \
                 running the HTTP routers only"
            );
            serve_http(settings, &infra, &auth, debug_header, shutdown).await?;
        }
        None => {
            tracing::info!("kafka is not enabled; running the HTTP routers only");
            serve_http(settings, &infra, &auth, debug_header, shutdown).await?;
        }
    }
    Ok(())
}

/// Consumes until shutdown. The message in flight finishes first. Fails when the DLQ cannot
/// be written, which ends the process with a non-zero exit code (kafka.md).
async fn run_consumer(
    kafka: &KafkaConfig,
    dispatcher: &Dispatcher,
    shutdown: CancellationToken,
) -> anyhow::Result<()> {
    let config = ConsumerConfig {
        brokers: kafka.brokers.clone(),
        group_id: kafka.group_id.clone(),
    };
    kafka_api::run(&config, dispatcher, shutdown)
        .await
        .context("the Kafka consumer stopped")
}

async fn run_router(
    name: &'static str,
    port_var: &'static str,
    addr: SocketAddr,
    router: Router,
    shutdown: CancellationToken,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("cannot bind the {name} router to {addr} (set {port_var})"))?;
    tracing::info!(router = name, %addr, "listening");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await
        .with_context(|| format!("{name} router failed"))
}

fn log_features(settings: &Settings) {
    let features = settings.features();
    for feature in Feature::ALL {
        tracing::info!(
            feature = feature.name(),
            enabled = features.is_enabled(feature),
            "feature"
        );
    }
}
