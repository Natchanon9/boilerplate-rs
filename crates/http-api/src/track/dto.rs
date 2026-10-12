//! Request and response bodies. The requests only capture raw fields; the application parser
//! validates them (api.md AC-8).

use application::input::{Field, UnknownFields};
use application::track::commands::{CreateTrackInput, UpdateTrackInput};
use domain::track::Track;
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, UtcOffset};
use uuid::Uuid;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CreateTrackRequest {
    title: Field<String>,
    artist: Field<String>,
    duration_ms: Field<i32>,
    #[serde(flatten)]
    unknown: UnknownFields,
}

impl From<CreateTrackRequest> for CreateTrackInput {
    fn from(r: CreateTrackRequest) -> Self {
        Self {
            title: r.title,
            artist: r.artist,
            duration_ms: r.duration_ms,
            unknown: r.unknown,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct UpdateTrackRequest {
    title: Field<String>,
    artist: Field<String>,
    duration_ms: Field<i32>,
    #[serde(flatten)]
    unknown: UnknownFields,
}

impl From<UpdateTrackRequest> for UpdateTrackInput {
    fn from(r: UpdateTrackRequest) -> Self {
        Self {
            title: r.title,
            artist: r.artist,
            duration_ms: r.duration_ms,
            unknown: r.unknown,
        }
    }
}

/// A track as the API returns it: all seven fields, timestamps in RFC 3339 UTC.
#[derive(Debug, Serialize)]
pub struct TrackResponse {
    id: Uuid,
    title: String,
    artist: String,
    duration_ms: i32,
    version: i64,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

impl From<Track> for TrackResponse {
    fn from(track: Track) -> Self {
        Self {
            id: track.id(),
            title: track.title().as_str().to_owned(),
            artist: track.artist().as_str().to_owned(),
            duration_ms: track.duration().get(),
            version: track.version(),
            created_at: track.created_at().to_offset(UtcOffset::UTC),
            updated_at: track.updated_at().to_offset(UtcOffset::UTC),
        }
    }
}
