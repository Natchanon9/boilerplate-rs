//! The `Track` entity: one piece of music in the shared catalog (the tracks spec).

mod error;
mod value_objects;

use time::OffsetDateTime;
use uuid::Uuid;

pub use error::TrackError;
pub use value_objects::{TrackArtist, TrackDuration, TrackTitle};

/// Longest title or artist, in characters. An own proposal (the tracks spec, Title and
/// artist length).
pub const TEXT_MAX_CHARS: usize = 200;

/// Shortest duration in ms. An own proposal (the tracks spec, Duration bounds).
pub const DURATION_MIN_MS: i32 = 1;

/// Longest duration in ms (24 h). An own proposal (the tracks spec, Duration bounds).
pub const DURATION_MAX_MS: i32 = 86_400_000;

/// A track that is not stored yet: it has no `version` and no timestamps. The database
/// assigns them on insert, so the domain never reads the clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTrack {
    id: Uuid,
    title: TrackTitle,
    artist: TrackArtist,
    duration: TrackDuration,
}

impl NewTrack {
    /// Every field is already a valid value object, so this cannot fail.
    pub fn new(id: Uuid, title: TrackTitle, artist: TrackArtist, duration: TrackDuration) -> Self {
        Self {
            id,
            title,
            artist,
            duration,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn title(&self) -> &TrackTitle {
        &self.title
    }

    pub fn artist(&self) -> &TrackArtist {
        &self.artist
    }

    pub fn duration(&self) -> TrackDuration {
        self.duration
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    id: Uuid,
    title: TrackTitle,
    artist: TrackArtist,
    duration: TrackDuration,
    version: i64,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl Track {
    /// A stored track, as the repository returns it. This is the only way to build one.
    /// Fails when the stored version is below 1.
    pub fn from_parts(
        id: Uuid,
        title: TrackTitle,
        artist: TrackArtist,
        duration: TrackDuration,
        version: i64,
        created_at: OffsetDateTime,
        updated_at: OffsetDateTime,
    ) -> Result<Self, TrackError> {
        if version < 1 {
            return Err(TrackError::InvalidVersion);
        }
        Ok(Self {
            id,
            title,
            artist,
            duration,
            version,
            created_at,
            updated_at,
        })
    }

    /// Replaces the three client fields. `id`, `version`, and both timestamps stay as
    /// loaded: the repository advances `version` and `updated_at` when it stores the change
    /// and returns the stored row.
    pub fn replace(&mut self, title: TrackTitle, artist: TrackArtist, duration: TrackDuration) {
        self.title = title;
        self.artist = artist;
        self.duration = duration;
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn title(&self) -> &TrackTitle {
        &self.title
    }

    pub fn artist(&self) -> &TrackArtist {
        &self.artist
    }

    pub fn duration(&self) -> TrackDuration {
        self.duration
    }

    pub fn version(&self) -> i64 {
        self.version
    }

    pub fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }

    pub fn updated_at(&self) -> OffsetDateTime {
        self.updated_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(title: &str, artist: &str, ms: i32) -> (TrackTitle, TrackArtist, TrackDuration) {
        (
            TrackTitle::try_from(title.to_owned()).unwrap(),
            TrackArtist::try_from(artist.to_owned()).unwrap(),
            TrackDuration::try_from(ms).unwrap(),
        )
    }

    fn track() -> Track {
        let (title, artist, duration) = parts("Clair de lune", "Debussy", 300_000);
        let at = OffsetDateTime::UNIX_EPOCH;
        Track::from_parts(Uuid::now_v7(), title, artist, duration, 1, at, at).unwrap()
    }

    #[test]
    fn new_track_holds_only_the_client_fields_and_the_id() {
        let (title, artist, duration) = parts("Clair de lune", "Debussy", 300_000);
        let id = Uuid::now_v7();
        let t = NewTrack::new(id, title, artist, duration);
        assert_eq!(t.id(), id);
        assert_eq!(t.title().as_str(), "Clair de lune");
        assert_eq!(t.artist().as_str(), "Debussy");
        assert_eq!(t.duration().get(), 300_000);
    }

    #[test]
    fn from_parts_keeps_the_stored_values() {
        let t = track();
        assert_eq!(t.version(), 1);
        assert_eq!(t.created_at(), t.updated_at());
        assert_eq!(t.title().as_str(), "Clair de lune");
        assert_eq!(t.artist().as_str(), "Debussy");
        assert_eq!(t.duration().get(), 300_000);
    }

    #[test]
    fn replace_changes_only_the_three_client_fields() {
        let mut t = track();
        let (id, created, updated) = (t.id(), t.created_at(), t.updated_at());
        let (title, artist, duration) = parts("Gymnopedie No. 1", "Satie", 190_000);
        t.replace(title, artist, duration);
        assert_eq!(t.title().as_str(), "Gymnopedie No. 1");
        assert_eq!(t.artist().as_str(), "Satie");
        assert_eq!(t.duration().get(), 190_000);
        assert_eq!(
            (t.id(), t.version(), t.created_at(), t.updated_at()),
            (id, 1, created, updated)
        );
    }

    #[test]
    fn from_parts_rejects_a_version_below_one() {
        let (title, artist, duration) = parts("a", "b", 1);
        let at = OffsetDateTime::UNIX_EPOCH;
        let build = |version| {
            Track::from_parts(
                Uuid::now_v7(),
                title.clone(),
                artist.clone(),
                duration,
                version,
                at,
                at,
            )
        };
        assert_eq!(build(0), Err(TrackError::InvalidVersion));
        assert_eq!(build(-1), Err(TrackError::InvalidVersion));
        assert!(build(1).is_ok());
    }
}
