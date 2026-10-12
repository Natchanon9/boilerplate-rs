//! `CachedTrackRepository`: cache-aside for get by id (cache.md). The cached value is the JSON
//! of `TrackRow`. Writes invalidate the key after the database commits. TTL is the default
//! `CACHE_TTL_SECS`, passed in by wiring (cache.md AC-5).

use std::time::Duration;

use application::error::AppError;
use application::pagination::{Page, PageRequest};
use application::track::ports::{InsertOutcome, TrackRepository};
use async_trait::async_trait;
use domain::track::{NewTrack, Track};
use uuid::Uuid;

use super::cache_aside::{self, RedisCache};
use crate::postgres::track_repo::TrackRow;

pub struct CachedTrackRepository<R> {
    inner: R,
    cache: RedisCache,
    ttl: Duration,
}

impl<R> CachedTrackRepository<R> {
    pub fn new(inner: R, cache: RedisCache, ttl: Duration) -> Self {
        Self { inner, cache, ttl }
    }

    fn key(&self, id: Uuid) -> String {
        self.cache.key("track", id)
    }
}

#[async_trait]
impl<R: TrackRepository> TrackRepository for CachedTrackRepository<R> {
    async fn insert(&self, track: &NewTrack) -> Result<InsertOutcome, AppError> {
        self.inner.insert(track).await
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Track>, AppError> {
        let row = cache_aside::get_or_load(&self.cache, &self.key(id), self.ttl, || async {
            let found = self.inner.find_by_id(id).await?;
            Ok::<_, AppError>(found.as_ref().map(TrackRow::from))
        })
        .await?;
        // A cached row that breaks a domain rule is data corruption.
        row.map(Track::try_from)
            .transpose()
            .map_err(|e| AppError::Internal(e.to_string()))
    }

    async fn list(&self, page: PageRequest) -> Result<Page<Track>, AppError> {
        self.inner.list(page).await
    }

    async fn update(&self, track: &Track) -> Result<Track, AppError> {
        let updated = self.inner.update(track).await?;
        cache_aside::invalidate(&self.cache, &self.key(updated.id())).await;
        Ok(updated)
    }

    async fn delete(&self, id: Uuid) -> Result<bool, AppError> {
        let deleted = self.inner.delete(id).await?;
        cache_aside::invalidate(&self.cache, &self.key(id)).await;
        Ok(deleted)
    }
}
