//! Thin handlers: extract, parse, call the use case, map to a response.

use std::sync::Arc;

use application::track::commands::{parse_create, parse_update};
use application::track::use_cases::TrackUseCases;
use axum::Json;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;

use super::dto::{CreateTrackRequest, TrackResponse, UpdateTrackRequest};
use crate::error::ApiError;
use crate::extract::{IdPath, JsonBody};
use crate::pagination::{ListQuery, PageResponse};

type Uc = State<Arc<TrackUseCases>>;

pub async fn create(
    State(uc): Uc,
    JsonBody(body): JsonBody<CreateTrackRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let track = uc.create(parse_create(body.into())?).await?;
    let location = format!("/api/v1/tracks/{}", track.id());
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, location)],
        Json(TrackResponse::from(track)),
    ))
}

pub async fn fetch(State(uc): Uc, IdPath(id): IdPath) -> Result<Json<TrackResponse>, ApiError> {
    Ok(Json(uc.get(id).await?.into()))
}

pub async fn list(
    State(uc): Uc,
    ListQuery(page): ListQuery,
) -> Result<Json<PageResponse<TrackResponse>>, ApiError> {
    let page = uc.list(page).await?;
    Ok(Json(PageResponse::from_page(page, TrackResponse::from)))
}

pub async fn replace(
    State(uc): Uc,
    IdPath(id): IdPath,
    JsonBody(body): JsonBody<UpdateTrackRequest>,
) -> Result<Json<TrackResponse>, ApiError> {
    let track = uc.update(id, parse_update(body.into())?).await?;
    Ok(Json(track.into()))
}

pub async fn remove(State(uc): Uc, IdPath(id): IdPath) -> Result<StatusCode, ApiError> {
    uc.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
