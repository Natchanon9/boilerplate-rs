//! Validated track values. An invalid value cannot be constructed.

use super::{DURATION_MAX_MS, DURATION_MIN_MS, TEXT_MAX_CHARS, TrackError};

/// Trims with `str::trim`, then counts Unicode scalar values, as PostgreSQL `char_length` does.
fn trimmed(text: &str, empty: TrackError, too_long: TrackError) -> Result<String, TrackError> {
    let text = text.trim();
    match text.chars().count() {
        0 => Err(empty),
        n if n > TEXT_MAX_CHARS => Err(too_long),
        _ => Ok(text.to_owned()),
    }
}

/// A track title: trimmed, 1 to 200 characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackTitle(String);

impl TrackTitle {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for TrackTitle {
    type Error = TrackError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        trimmed(&value, TrackError::TitleEmpty, TrackError::TitleTooLong).map(Self)
    }
}

/// An artist name: trimmed, 1 to 200 characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackArtist(String);

impl TrackArtist {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for TrackArtist {
    type Error = TrackError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        trimmed(&value, TrackError::ArtistEmpty, TrackError::ArtistTooLong).map(Self)
    }
}

/// A track length in milliseconds: 1 to 86 400 000.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackDuration(i32);

impl TrackDuration {
    pub fn get(self) -> i32 {
        self.0
    }
}

impl TryFrom<i32> for TrackDuration {
    type Error = TrackError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        if (DURATION_MIN_MS..=DURATION_MAX_MS).contains(&value) {
            Ok(Self(value))
        } else {
            Err(TrackError::DurationOutOfRange)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title(text: &str) -> Result<TrackTitle, TrackError> {
        TrackTitle::try_from(text.to_owned())
    }

    fn artist(text: &str) -> Result<TrackArtist, TrackError> {
        TrackArtist::try_from(text.to_owned())
    }

    #[test]
    fn title_is_trimmed() {
        assert_eq!(
            title("  Clair de lune \t").unwrap().as_str(),
            "Clair de lune"
        );
    }

    #[test]
    fn title_trims_unicode_whitespace() {
        // U+3000 is Unicode whitespace that PostgreSQL `btrim` would keep.
        assert_eq!(title("\u{3000}a\u{3000}").unwrap().as_str(), "a");
    }

    #[test]
    fn title_empty_after_trimming_is_rejected() {
        assert_eq!(title(""), Err(TrackError::TitleEmpty));
        assert_eq!(title(" \t\u{3000}"), Err(TrackError::TitleEmpty));
    }

    #[test]
    fn title_bounds_are_one_and_two_hundred_characters() {
        assert!(title("a").is_ok());
        assert!(title(&"a".repeat(200)).is_ok());
        assert_eq!(title(&"a".repeat(201)), Err(TrackError::TitleTooLong));
    }

    #[test]
    fn title_counts_characters_not_bytes() {
        // 200 three-byte characters are 600 bytes but within the limit.
        assert!(title(&"ก".repeat(200)).is_ok());
        assert_eq!(title(&"ก".repeat(201)), Err(TrackError::TitleTooLong));
    }

    #[test]
    fn title_length_is_measured_after_trimming() {
        let padded = format!("  {}  ", "a".repeat(200));
        assert!(title(&padded).is_ok());
    }

    #[test]
    fn artist_follows_the_same_bounds() {
        assert_eq!(artist("  Debussy ").unwrap().as_str(), "Debussy");
        assert_eq!(artist("  "), Err(TrackError::ArtistEmpty));
        assert!(artist(&"a".repeat(200)).is_ok());
        assert_eq!(artist(&"a".repeat(201)), Err(TrackError::ArtistTooLong));
    }

    #[test]
    fn duration_bounds_are_one_and_a_day_in_ms() {
        assert_eq!(TrackDuration::try_from(1).unwrap().get(), 1);
        assert_eq!(
            TrackDuration::try_from(86_400_000).unwrap().get(),
            86_400_000
        );
        for bad in [0, -1, 86_400_001, i32::MIN, i32::MAX] {
            assert_eq!(
                TrackDuration::try_from(bad),
                Err(TrackError::DurationOutOfRange),
                "{bad}"
            );
        }
    }
}
