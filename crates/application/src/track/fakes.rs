//! An in-memory [`TrackRepository`] for unit tests. It plays the database: it assigns
//! `version` and the timestamps itself, and it passes `track_repository_contract`.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use domain::track::{NewTrack, Track};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use super::ports::{InsertOutcome, TrackRepository};
use crate::error::AppError;
use crate::pagination::{Page, PageRequest};

#[derive(Debug, Default)]
pub struct InMemoryTrackRepository {
    tracks: Mutex<BTreeMap<Uuid, Track>>,
}

impl InMemoryTrackRepository {
    fn lock(&self) -> Result<MutexGuard<'_, BTreeMap<Uuid, Track>>, AppError> {
        self.tracks
            .lock()
            .map_err(|_| AppError::Internal("fake repository lock poisoned".to_owned()))
    }
}

fn internal(error: domain::track::TrackError) -> AppError {
    AppError::Internal(error.to_string())
}

#[async_trait]
impl TrackRepository for InMemoryTrackRepository {
    async fn insert(&self, track: &NewTrack) -> Result<InsertOutcome, AppError> {
        let mut tracks = self.lock()?;
        if let Some(existing) = tracks.get(&track.id()) {
            return Ok(InsertOutcome::AlreadyExists(existing.clone()));
        }
        let now = OffsetDateTime::now_utc();
        let stored = Track::from_parts(
            track.id(),
            track.title().clone(),
            track.artist().clone(),
            track.duration(),
            1,
            now,
            now,
        )
        .map_err(internal)?;
        tracks.insert(stored.id(), stored.clone());
        Ok(InsertOutcome::Inserted(stored))
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Track>, AppError> {
        Ok(self.lock()?.get(&id).cloned())
    }

    async fn list(&self, page: PageRequest) -> Result<Page<Track>, AppError> {
        let tracks = self.lock()?;
        let rows: Vec<Track> = tracks
            .values()
            .rev()
            .filter(|t| page.after().is_none_or(|after| t.id() < after))
            .take(page.fetch_limit() as usize)
            .cloned()
            .collect();
        Ok(Page::from_overfetch(rows, page, Track::id))
    }

    async fn update(&self, track: &Track) -> Result<Track, AppError> {
        let mut tracks = self.lock()?;
        let stored = tracks.get(&track.id()).ok_or(AppError::NotFound)?;
        // Like the database: only the three client fields come from the caller. The stored
        // row advances its own version, and `updated_at` strictly (at least 1 us), so a test
        // never sees two writes with the same timestamp.
        let updated_at =
            OffsetDateTime::now_utc().max(stored.updated_at() + Duration::microseconds(1));
        let updated = Track::from_parts(
            stored.id(),
            track.title().clone(),
            track.artist().clone(),
            track.duration(),
            stored.version() + 1,
            stored.created_at(),
            updated_at,
        )
        .map_err(internal)?;
        tracks.insert(updated.id(), updated.clone());
        Ok(updated)
    }

    async fn delete(&self, id: Uuid) -> Result<bool, AppError> {
        Ok(self.lock()?.remove(&id).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::contract::track_repository_contract;

    #[tokio::test]
    async fn the_fake_passes_the_repository_contract() {
        track_repository_contract(&InMemoryTrackRepository::default()).await;
    }
}
