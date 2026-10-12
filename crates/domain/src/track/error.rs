//! Why a track value or a stored row is rejected.

/// A broken track invariant. The application parser maps each variant to a field error code
/// (api.md AC-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TrackError {
    #[error("title is empty after trimming")]
    TitleEmpty,
    #[error("title is longer than {max} characters", max = super::TEXT_MAX_CHARS)]
    TitleTooLong,
    #[error("artist is empty after trimming")]
    ArtistEmpty,
    #[error("artist is longer than {max} characters", max = super::TEXT_MAX_CHARS)]
    ArtistTooLong,
    #[error(
        "duration is outside {min} to {max} ms",
        min = super::DURATION_MIN_MS,
        max = super::DURATION_MAX_MS
    )]
    DurationOutOfRange,
    #[error("version must be at least 1")]
    InvalidVersion,
}
