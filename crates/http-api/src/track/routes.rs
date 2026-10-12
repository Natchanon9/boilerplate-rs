//! `/api/v1/tracks` on the protected router. Each method carries its own scope (security.md SEC-2).

use std::sync::Arc;

use application::track::use_cases::TrackUseCases;
use axum::Router;
use axum::routing::{delete, get, post, put};

use super::handlers::{create, fetch, list, remove, replace};
use crate::auth::RequireScope;

// Paths are relative to `/api/v1`: wiring merges this router into `api_v1`.
pub fn routes(uc: Arc<TrackUseCases>) -> Router {
    Router::new()
        .route(
            "/tracks",
            get(list)
                .route_layer(RequireScope("tracks:read"))
                .merge(post(create).route_layer(RequireScope("tracks:write"))),
        )
        .route(
            "/tracks/{id}",
            get(fetch)
                .route_layer(RequireScope("tracks:read"))
                .merge(put(replace).route_layer(RequireScope("tracks:write")))
                .merge(delete(remove).route_layer(RequireScope("tracks:write"))),
        )
        .with_state(uc)
}
