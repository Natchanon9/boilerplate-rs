//! One method per track operation. A missing track is `AppError::NotFound`.

use std::sync::Arc;

use domain::track::{NewTrack, Track};
use uuid::Uuid;

use super::commands::{CreateTrack, UpdateTrack};
use super::ports::{InsertOutcome, TrackRepository};
use crate::error::AppError;
use crate::pagination::{Page, PageRequest};

pub struct TrackUseCases {
    repo: Arc<dyn TrackRepository>,
}

impl TrackUseCases {
    pub fn new(repo: Arc<dyn TrackRepository>) -> Self {
        Self { repo }
    }

    /// Creates a track under a new UUIDv7 id.
    pub async fn create(&self, command: CreateTrack) -> Result<Track, AppError> {
        let track = NewTrack::new(
            Uuid::now_v7(),
            command.title,
            command.artist,
            command.duration,
        );
        match self.repo.insert(&track).await? {
            InsertOutcome::Inserted(stored) | InsertOutcome::AlreadyExists(stored) => Ok(stored),
        }
    }

    pub async fn get(&self, id: Uuid) -> Result<Track, AppError> {
        self.repo.find_by_id(id).await?.ok_or(AppError::NotFound)
    }

    pub async fn list(&self, page: PageRequest) -> Result<Page<Track>, AppError> {
        self.repo.list(page).await
    }

    pub async fn update(&self, id: Uuid, command: UpdateTrack) -> Result<Track, AppError> {
        let mut track = self.get(id).await?;
        track.replace(command.title, command.artist, command.duration);
        self.repo.update(&track).await
    }

    pub async fn delete(&self, id: Uuid) -> Result<(), AppError> {
        if self.repo.delete(id).await? {
            Ok(())
        } else {
            Err(AppError::NotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use domain::track::{TrackArtist, TrackDuration, TrackTitle};

    use super::*;
    use crate::track::fakes::InMemoryTrackRepository;

    fn use_cases() -> TrackUseCases {
        TrackUseCases::new(Arc::new(InMemoryTrackRepository::default()))
    }

    fn command(title: &str, ms: i32) -> CreateTrack {
        CreateTrack {
            title: TrackTitle::try_from(title.to_owned()).unwrap(),
            artist: TrackArtist::try_from("Artist".to_owned()).unwrap(),
            duration: TrackDuration::try_from(ms).unwrap(),
        }
    }

    fn update_command(title: &str, ms: i32) -> UpdateTrack {
        let c = command(title, ms);
        UpdateTrack {
            title: c.title,
            artist: c.artist,
            duration: c.duration,
        }
    }

    #[tokio::test]
    async fn create_generates_a_v7_id_and_version_one() {
        let uc = use_cases();
        let track = uc.create(command("One", 1000)).await.unwrap();
        assert_eq!(track.id().get_version_num(), 7);
        assert_eq!(track.version(), 1);
        assert_eq!(uc.get(track.id()).await.unwrap(), track);
    }

    #[tokio::test]
    async fn get_update_and_delete_of_an_unknown_id_are_not_found() {
        let uc = use_cases();
        let id = Uuid::now_v7();
        assert!(matches!(uc.get(id).await, Err(AppError::NotFound)));
        assert!(matches!(
            uc.update(id, update_command("x", 1)).await,
            Err(AppError::NotFound)
        ));
        assert!(matches!(uc.delete(id).await, Err(AppError::NotFound)));
    }

    #[tokio::test]
    async fn update_replaces_fields_and_bumps_the_version() {
        let uc = use_cases();
        let created = uc.create(command("Old", 1000)).await.unwrap();
        let updated = uc
            .update(created.id(), update_command("New", 2000))
            .await
            .unwrap();
        assert_eq!(updated.title().as_str(), "New");
        assert_eq!(updated.duration().get(), 2000);
        assert_eq!(updated.version(), 2);
        assert_eq!(updated.created_at(), created.created_at());
        assert!(updated.updated_at() >= created.updated_at());
    }

    #[tokio::test]
    async fn delete_removes_the_track() {
        let uc = use_cases();
        let created = uc.create(command("Gone", 1000)).await.unwrap();
        uc.delete(created.id()).await.unwrap();
        assert!(matches!(
            uc.get(created.id()).await,
            Err(AppError::NotFound)
        ));
        assert!(matches!(
            uc.delete(created.id()).await,
            Err(AppError::NotFound)
        ));
    }
}
