//! `db migrate`, `db rollback`, and `db seed` (database.md AC-2, AC-3).

use std::path::PathBuf;

use anyhow::Context;
use secrecy::ExposeSecret;

use crate::config::{DbPlatform, Settings};
use infrastructure::postgres;

pub async fn migrate(settings: &Settings) -> anyhow::Result<()> {
    let db = settings.database()?;
    match db.platform {
        DbPlatform::Postgres => {
            let pool = postgres::connect(db.url.expose_secret()).await?;
            let applied = postgres::migrate(&pool).await?;
            tracing::info!(
                platform = db.platform.name(),
                ?applied,
                "migrations applied"
            );
        }
    }
    Ok(())
}

pub async fn rollback(settings: &Settings) -> anyhow::Result<()> {
    let db = settings.database()?;
    match db.platform {
        DbPlatform::Postgres => {
            let pool = postgres::connect(db.url.expose_secret()).await?;
            match postgres::rollback(&pool).await? {
                Some(version) => {
                    tracing::info!(platform = db.platform.name(), version, "migration reverted");
                }
                None => tracing::info!(platform = db.platform.name(), "nothing to roll back"),
            }
        }
    }
    Ok(())
}

pub async fn seed(settings: &Settings) -> anyhow::Result<()> {
    let db = settings.database()?;
    // Read at run time, relative to the working directory (the repo root, or /app in the image).
    let dir = PathBuf::from("seeds").join(db.platform.name());
    match db.platform {
        DbPlatform::Postgres => {
            let pool = postgres::connect(db.url.expose_secret()).await?;
            let files = postgres::seed(&pool, &dir)
                .await
                .with_context(|| format!("db seed from {}", dir.display()))?;
            tracing::info!(platform = db.platform.name(), ?files, "seeds applied");
        }
    }
    Ok(())
}
