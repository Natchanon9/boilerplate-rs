//! Raw track input and the parser that turns it into domain values. It collects every field
//! error into one `AppError::Validation` (api.md AC-8).

use domain::track::{TrackArtist, TrackDuration, TrackError, TrackTitle};

use crate::error::{AppError, FieldCode, FieldError};
use crate::input::{Field, FieldErrors, Lines, UnknownFields};

/// The body of a create request.
#[derive(Debug, Default)]
pub struct CreateTrackInput {
    pub title: Field<String>,
    pub artist: Field<String>,
    pub duration_ms: Field<i32>,
    pub unknown: UnknownFields,
}

/// The body of an update request: the same fields as a create.
#[derive(Debug, Default)]
pub struct UpdateTrackInput {
    pub title: Field<String>,
    pub artist: Field<String>,
    pub duration_ms: Field<i32>,
    pub unknown: UnknownFields,
}

/// A validated create command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateTrack {
    pub title: TrackTitle,
    pub artist: TrackArtist,
    pub duration: TrackDuration,
}

/// A validated update command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateTrack {
    pub title: TrackTitle,
    pub artist: TrackArtist,
    pub duration: TrackDuration,
}

pub fn parse_create(input: CreateTrackInput) -> Result<CreateTrack, AppError> {
    let CreateTrackInput {
        title,
        artist,
        duration_ms,
        unknown,
    } = input;
    parse(title, artist, duration_ms, &unknown).map(|(title, artist, duration)| CreateTrack {
        title,
        artist,
        duration,
    })
}

pub fn parse_update(input: UpdateTrackInput) -> Result<UpdateTrack, AppError> {
    let UpdateTrackInput {
        title,
        artist,
        duration_ms,
        unknown,
    } = input;
    parse(title, artist, duration_ms, &unknown).map(|(title, artist, duration)| UpdateTrack {
        title,
        artist,
        duration,
    })
}

type Parsed = (TrackTitle, TrackArtist, TrackDuration);

fn parse(
    title: Field<String>,
    artist: Field<String>,
    duration_ms: Field<i32>,
    unknown: &UnknownFields,
) -> Result<Parsed, AppError> {
    let mut errors = FieldErrors::new();
    errors.unknown(unknown);
    let title = title
        .required("title", &mut errors)
        .filter(|s| errors.text("title", s, Lines::Single))
        .and_then(|s| errors.check(TrackTitle::try_from(s).map_err(field_error)));
    let artist = artist
        .required("artist", &mut errors)
        .filter(|s| errors.text("artist", s, Lines::Single))
        .and_then(|s| errors.check(TrackArtist::try_from(s).map_err(field_error)));
    let duration = duration_ms
        .required("duration_ms", &mut errors)
        .and_then(|ms| errors.check(TrackDuration::try_from(ms).map_err(field_error)));
    match (title, artist, duration) {
        (Some(title), Some(artist), Some(duration)) if errors.is_empty() => {
            Ok((title, artist, duration))
        }
        _ => Err(errors.into_error()),
    }
}

/// Maps a domain error to the field and code api.md AC-8 defines.
fn field_error(error: TrackError) -> FieldError {
    let (field, code) = match error {
        TrackError::TitleEmpty => ("title", FieldCode::TooShort),
        TrackError::TitleTooLong => ("title", FieldCode::TooLong),
        TrackError::ArtistEmpty => ("artist", FieldCode::TooShort),
        TrackError::ArtistTooLong => ("artist", FieldCode::TooLong),
        TrackError::DurationOutOfRange => ("duration_ms", FieldCode::OutOfRange),
        // A stored-row error; no input value produces it.
        TrackError::InvalidVersion => ("version", FieldCode::OutOfRange),
    };
    FieldError::of(field, code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(title: Field<String>, artist: Field<String>, ms: Field<i32>) -> CreateTrackInput {
        CreateTrackInput {
            title,
            artist,
            duration_ms: ms,
            unknown: UnknownFields::default(),
        }
    }

    fn present(text: &str) -> Field<String> {
        Field::Present(text.to_owned())
    }

    fn codes(result: Result<CreateTrack, AppError>) -> Vec<(String, FieldCode)> {
        match result {
            Err(AppError::Validation(errors)) => {
                errors.into_iter().map(|e| (e.field, e.code)).collect()
            }
            other => panic!("expected validation, got {other:?}"),
        }
    }

    fn pair(field: &str, code: FieldCode) -> (String, FieldCode) {
        (field.to_owned(), code)
    }

    #[test]
    fn valid_input_is_trimmed() {
        let parsed = parse_create(input(
            present("  Clair de lune "),
            present("Debussy"),
            Field::Present(300_000),
        ))
        .unwrap();
        assert_eq!(parsed.title.as_str(), "Clair de lune");
        assert_eq!(parsed.artist.as_str(), "Debussy");
        assert_eq!(parsed.duration.get(), 300_000);
    }

    #[test]
    fn missing_null_and_wrong_type_are_required_and_invalid_type() {
        assert_eq!(
            codes(parse_create(input(
                Field::Missing,
                Field::Null,
                Field::Invalid
            ))),
            [
                pair("title", FieldCode::Required),
                pair("artist", FieldCode::Required),
                pair("duration_ms", FieldCode::InvalidType),
            ]
        );
        assert_eq!(
            codes(parse_create(input(
                Field::Invalid,
                present("a"),
                Field::Null
            ))),
            [
                pair("title", FieldCode::InvalidType),
                pair("duration_ms", FieldCode::Required),
            ]
        );
    }

    #[test]
    fn text_and_duration_rules_map_to_their_codes() {
        let long = "a".repeat(201);
        assert_eq!(
            codes(parse_create(input(
                present("  "),
                present(&long),
                Field::Present(0)
            ))),
            [
                pair("title", FieldCode::TooShort),
                pair("artist", FieldCode::TooLong),
                pair("duration_ms", FieldCode::OutOfRange),
            ]
        );
        assert_eq!(
            codes(parse_create(input(
                present("a"),
                present("b"),
                Field::Present(86_400_001)
            ))),
            [pair("duration_ms", FieldCode::OutOfRange)]
        );
    }

    #[test]
    fn control_characters_are_invalid_format() {
        assert_eq!(
            codes(parse_create(input(
                present("a\nb"),
                present("a\u{7}b"),
                Field::Present(1)
            ))),
            [
                pair("title", FieldCode::InvalidFormat),
                pair("artist", FieldCode::InvalidFormat),
            ]
        );
    }

    #[test]
    fn every_error_is_reported_at_once_including_unknown_fields() {
        let mut raw = input(present(""), present("a\u{7}b"), Field::Present(0));
        raw.unknown = UnknownFields(vec!["id".to_owned()]);
        assert_eq!(
            codes(parse_create(raw)),
            [
                pair("id", FieldCode::UnknownField),
                pair("title", FieldCode::TooShort),
                pair("artist", FieldCode::InvalidFormat),
                pair("duration_ms", FieldCode::OutOfRange),
            ]
        );
    }

    #[test]
    fn update_uses_the_same_rules() {
        let ok = parse_update(UpdateTrackInput {
            title: present(" x "),
            artist: present("y"),
            duration_ms: Field::Present(1),
            unknown: UnknownFields::default(),
        })
        .unwrap();
        assert_eq!(ok.title.as_str(), "x");

        let bad = parse_update(UpdateTrackInput::default());
        assert!(matches!(bad, Err(AppError::Validation(e)) if e.len() == 3));
    }

}
