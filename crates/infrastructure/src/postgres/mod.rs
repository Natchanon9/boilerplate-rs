//! The PostgreSQL adapter (database.md): the pool, migrations, and seeds.
//! Repositories live next to this file as `<res>_repo.rs`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use application::error::AppError;

use sqlx::migrate::{Migrate, MigrateError, Migrator};
pub use sqlx::postgres::PgPool;
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, Connection};

pub mod track_repo;

/// Reversible migrations from `migrations/postgres/`, embedded at compile time.
pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations/postgres");

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("cannot connect to the database")]
    Connect(#[source] sqlx::Error),
    #[error("migration failed")]
    Migrate(#[from] MigrateError),
    #[error("cannot read seed directory {}", dir.display())]
    SeedDir {
        dir: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("seed {} failed", file.display())]
    Seed {
        file: PathBuf,
        #[source]
        source: SeedFailure,
    },
}

/// Why a seed file failed.
#[derive(Debug, thiserror::Error)]
pub enum SeedFailure {
    #[error("cannot read the file")]
    Read(#[from] std::io::Error),
    #[error("SQL error")]
    Sql(#[from] sqlx::Error),
}

/// Connects now and fails fast. For one-off commands such as `db migrate`.
pub async fn connect(url: &str) -> Result<PgPool, DbError> {
    PgPoolOptions::new()
        .connect(url)
        .await
        .map_err(DbError::Connect)
}

/// How long a pool acquire waits before it fails (database.md P1). The sqlx default
/// is 30 s, and during an outage each acquire retried for 30 s.
pub const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(1);

/// How long a pool acquire may take before sqlx logs a slow-acquire warning (database.md P1).
/// The sqlx default is 2 s, which a 1 s `ACQUIRE_TIMEOUT` would never reach, so
/// the warning would never fire.
pub const ACQUIRE_SLOW_THRESHOLD: Duration = Duration::from_millis(500);

/// Builds the pool without connecting, so a long-running process starts even while the
/// database is down, and `/readyz` reports it. `max_connections` is the sqlx default (10);
/// the acquire timeout and slow threshold are set by `ACQUIRE_TIMEOUT` and
/// `ACQUIRE_SLOW_THRESHOLD`.
pub fn connect_lazy(url: &str) -> Result<PgPool, DbError> {
    PgPoolOptions::new()
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .acquire_slow_threshold(ACQUIRE_SLOW_THRESHOLD)
        .connect_lazy(url)
        .map_err(DbError::Connect)
}

/// Maps a pool or connection error to `AppError::Unavailable`. On `PoolTimedOut` the cause
/// carries the pool state, so the log tells a full pool from an unreachable database.
pub fn unavailable(pool: &PgPool, error: &sqlx::Error) -> AppError {
    AppError::Unavailable(cause(pool, error))
}

/// The error text, plus the pool state on `PoolTimedOut`. Only numbers are added: no URL,
/// host, or secret.
fn cause(pool: &PgPool, error: &sqlx::Error) -> String {
    match error {
        sqlx::Error::PoolTimedOut => format!(
            "{error} (open {}/{}, idle {})",
            pool.size(),
            pool.options().get_max_connections(),
            pool.num_idle()
        ),
        _ => error.to_string(),
    }
}

/// Checks that a connection can be acquired and answers a ping.
pub async fn ping(pool: &PgPool) -> Result<(), sqlx::Error> {
    pool.acquire().await?.ping().await
}

/// Applies every pending migration and returns the versions it applied.
pub async fn migrate(pool: &PgPool) -> Result<Vec<i64>, DbError> {
    let before = applied_versions(pool).await?;
    MIGRATOR.run(pool).await?;
    let after = applied_versions(pool).await?;
    Ok(after.into_iter().filter(|v| !before.contains(v)).collect())
}

/// Reverts the most recently applied migration and returns its version, or `None` when
/// nothing is applied.
pub async fn rollback(pool: &PgPool) -> Result<Option<i64>, DbError> {
    let applied = applied_versions(pool).await?;
    let Some((&last, rest)) = applied.split_last() else {
        return Ok(None);
    };
    let target = rest.last().copied().unwrap_or(0);
    MIGRATOR.undo(pool, target).await?;
    Ok(Some(last))
}

/// Applied migration versions, oldest first.
async fn applied_versions(pool: &PgPool) -> Result<Vec<i64>, DbError> {
    let mut conn = pool.acquire().await.map_err(MigrateError::from)?;
    conn.ensure_migrations_table(&MIGRATOR.table_name).await?;
    let mut versions: Vec<i64> = conn
        .list_applied_migrations(&MIGRATOR.table_name)
        .await?
        .into_iter()
        .map(|m| m.version)
        .collect();
    versions.sort_unstable();
    Ok(versions)
}

/// Runs every `*.sql` file in `dir` in file-name order, each in its own transaction, and
/// returns the files it ran. Seeds use fixed ids and
/// `ON CONFLICT DO NOTHING`, so running them twice changes nothing.
pub async fn seed(pool: &PgPool, dir: &Path) -> Result<Vec<PathBuf>, DbError> {
    let seed_dir_error = |source| DbError::SeedDir {
        dir: dir.to_path_buf(),
        source,
    };
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(seed_dir_error)? {
        let path = entry.map_err(seed_dir_error)?.path();
        if path.extension().is_some_and(|ext| ext == "sql") {
            files.push(path);
        }
    }
    files.sort();

    for file in &files {
        run_seed_file(pool, file)
            .await
            .map_err(|source| DbError::Seed {
                file: file.clone(),
                source,
            })?;
    }
    Ok(files)
}

async fn run_seed_file(pool: &PgPool, file: &Path) -> Result<(), SeedFailure> {
    let sql = std::fs::read_to_string(file)?;
    let mut tx = pool.begin().await?;
    // Seed files are trusted, version-controlled SQL, like migrations. This is the only
    // place raw SQL is allowed (security.md SEC-8, checked in `make verify`).
    sqlx::raw_sql(AssertSqlSafe(sql)).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// `/readyz` check for the database.
pub struct PgHealthCheck {
    pool: PgPool,
}

impl PgHealthCheck {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl application::health::HealthCheck for PgHealthCheck {
    fn name(&self) -> &'static str {
        "database"
    }

    async fn check(&self) -> Result<(), application::health::HealthError> {
        ping(&self.pool)
            .await
            .map_err(|err| application::health::HealthError(cause(&self.pool, &err)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pool_timeout_cause_has_the_pool_state_and_no_host() {
        let pool = connect_lazy("postgres://u:p@127.0.0.1:1/db").unwrap();
        let text = cause(&pool, &sqlx::Error::PoolTimedOut);
        eprintln!("{text}");
        assert!(text.contains("(open 0/10, idle 0)"), "{text}");
        assert!(!text.contains("127.0.0.1"), "{text}");
        assert!(matches!(
            unavailable(&pool, &sqlx::Error::PoolTimedOut),
            AppError::Unavailable(t) if t == text
        ));
    }
}
