//! `PgTrackRepository`: the PostgreSQL implementation of `TrackRepository`.
//! Timestamps and `version` come from the database (layers.md section 4), and every
//! statement is a `sqlx::query_as!` or `query!` macro (security.md SEC-8).

use application::error::AppError;
use application::pagination::{Page, PageRequest};
use application::track::ports::{InsertOutcome, TrackRepository};
use async_trait::async_trait;
use domain::track::{NewTrack, Track, TrackArtist, TrackDuration, TrackError, TrackTitle};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use super::unavailable;

/// One `tracks` row. It is also the value the cache decorator stores as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct TrackRow {
    pub id: Uuid,
    pub title: String,
    pub artist: String,
    pub duration_ms: i32,
    pub version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl TryFrom<TrackRow> for Track {
    type Error = TrackError;

    fn try_from(row: TrackRow) -> Result<Self, Self::Error> {
        Track::from_parts(
            row.id,
            TrackTitle::try_from(row.title)?,
            TrackArtist::try_from(row.artist)?,
            TrackDuration::try_from(row.duration_ms)?,
            row.version,
            row.created_at,
            row.updated_at,
        )
    }
}

impl From<&Track> for TrackRow {
    fn from(track: &Track) -> Self {
        Self {
            id: track.id(),
            title: track.title().as_str().to_owned(),
            artist: track.artist().as_str().to_owned(),
            duration_ms: track.duration().get(),
            version: track.version(),
            created_at: track.created_at(),
            updated_at: track.updated_at(),
        }
    }
}

pub struct PgTrackRepository {
    pool: PgPool,
}

impl PgTrackRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Pool and connection errors are `Unavailable`; a unique violation is `Conflict`; anything
    /// else, such as a check violation or a failed decode, is `Internal`.
    fn map_err(&self, error: sqlx::Error) -> AppError {
        match &error {
            sqlx::Error::PoolTimedOut
            | sqlx::Error::PoolClosed
            | sqlx::Error::Io(_)
            | sqlx::Error::Tls(_)
            | sqlx::Error::WorkerCrashed => unavailable(&self.pool, &error),
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                // The database message names the constraint and the key, so it is logged and
                // never sent to the client (security.md SEC-6).
                tracing::info!(
                    constraint = db.constraint().unwrap_or("unknown"),
                    error = %db.message(),
                    "track insert or update hit a unique constraint"
                );
                AppError::Conflict("The track conflicts with an existing track.".to_owned())
            }
            _ => AppError::Internal(error.to_string()),
        }
    }
}

fn to_track(row: TrackRow) -> Result<Track, AppError> {
    // A stored row that breaks a domain rule is data corruption, not a client error.
    Track::try_from(row).map_err(|e| AppError::Internal(e.to_string()))
}

#[async_trait]
impl TrackRepository for PgTrackRepository {
    async fn insert(&self, track: &NewTrack) -> Result<InsertOutcome, AppError> {
        let inserted = sqlx::query_as!(
            TrackRow,
            "INSERT INTO tracks (id, title, artist, duration_ms)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (id) DO NOTHING
             RETURNING id, title, artist, duration_ms, version, created_at, updated_at",
            track.id(),
            track.title().as_str(),
            track.artist().as_str(),
            track.duration().get(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| self.map_err(e))?;
        if let Some(row) = inserted {
            return to_track(row).map(InsertOutcome::Inserted);
        }
        match self.find_by_id(track.id()).await? {
            Some(existing) => Ok(InsertOutcome::AlreadyExists(existing)),
            None => Err(AppError::Internal(
                "insert conflicted on a track that is gone".to_owned(),
            )),
        }
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Track>, AppError> {
        sqlx::query_as!(
            TrackRow,
            "SELECT id, title, artist, duration_ms, version, created_at, updated_at
             FROM tracks WHERE id = $1",
            id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| self.map_err(e))?
        .map(to_track)
        .transpose()
    }

    async fn list(&self, page: PageRequest) -> Result<Page<Track>, AppError> {
        let rows = sqlx::query_as!(
            TrackRow,
            "SELECT id, title, artist, duration_ms, version, created_at, updated_at
             FROM tracks
             WHERE ($1::uuid IS NULL OR id < $1)
             ORDER BY id DESC
             LIMIT $2",
            page.after(),
            page.fetch_limit(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| self.map_err(e))?;
        let tracks = rows
            .into_iter()
            .map(to_track)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Page::from_overfetch(tracks, page, Track::id))
    }

    async fn update(&self, track: &Track) -> Result<Track, AppError> {
        sqlx::query_as!(
            TrackRow,
            "UPDATE tracks
             SET title = $2, artist = $3, duration_ms = $4,
                 version = version + 1, updated_at = now()
             WHERE id = $1
             RETURNING id, title, artist, duration_ms, version, created_at, updated_at",
            track.id(),
            track.title().as_str(),
            track.artist().as_str(),
            track.duration().get(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| self.map_err(e))?
        .map(to_track)
        .transpose()?
        .ok_or(AppError::NotFound)
    }

    async fn delete(&self, id: Uuid) -> Result<bool, AppError> {
        let result = sqlx::query!("DELETE FROM tracks WHERE id = $1", id)
            .execute(&self.pool)
            .await
            .map_err(|e| self.map_err(e))?;
        Ok(result.rows_affected() > 0)
    }
}
