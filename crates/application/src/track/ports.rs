//! The repository port the use cases need. Types here are domain or plain application types,
//! never rows or DTOs.

use async_trait::async_trait;
use domain::track::{NewTrack, Track};
use uuid::Uuid;

use crate::error::AppError;
use crate::pagination::{Page, PageRequest};

/// The result of [`TrackRepository::insert`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    /// The track was stored. It holds the row as stored, with `version` 1 and the timestamps
    /// the repository assigned.
    Inserted(Track),
    /// A track with this id exists already. It holds the existing row, which is not changed.
    AlreadyExists(Track),
}

#[async_trait]
pub trait TrackRepository: Send + Sync {
    async fn insert(&self, track: &NewTrack) -> Result<InsertOutcome, AppError>;

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Track>, AppError>;

    /// Newest first (by `id` descending), after `page.after()`.
    async fn list(&self, page: PageRequest) -> Result<Page<Track>, AppError>;

    /// Stores the title, artist, and duration of `track` under its id, advances the stored
    /// `version` by 1 and `updated_at`, and returns the stored row. The last write wins.
    /// `AppError::NotFound` when no such track exists.
    async fn update(&self, track: &Track) -> Result<Track, AppError>;

    /// Hard delete. Returns whether a row was deleted.
    async fn delete(&self, id: Uuid) -> Result<bool, AppError>;
}
