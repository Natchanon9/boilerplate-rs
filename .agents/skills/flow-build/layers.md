# Layers reference (Rust, clean architecture)

This file is derived from [docs/conventions/stack-architecture.md](../../../docs/conventions/stack-architecture.md), and covers the MVP only. When that convention changes, update this file in the same commit. Never build anything a convention lists under `## Deferred`.
`<res>` is the resource module name, singular snake_case (`widget`). `<Res>` is the type name (`Widget`). `<res_plural>` is the route and table name (`widgets`).

## Dependency rule

```
bootstrap -> http-api, kafka-api, infrastructure -> application -> domain
```
Inner crates never name outer crates or frameworks. `deny.toml` bans enforce this, so a violation fails the build. Do not work around it with feature flags or re-exports.
On a bare boilerplate no crate depends on `domain` yet. The first resource adds `domain = { workspace = true }` to every crate that names a domain type (application, infrastructure, http-api), and `uuid` to infrastructure with `features = ["serde"]`, because the cached row holds a `Uuid`.

## 1. Schema
- Path: `migrations/<platform>/<timestamp>_<verb>_<res_plural>.{up,down}.sql`, created with `sqlx migrate add -r --source migrations/postgres <verb>_<res_plural>`. Every migration has a down script ([database.md](../../../docs/conventions/database.md)).
- Optional demo data: `seeds/<platform>/NNNN_<res_plural>.sql`, with fixed ids and `ON CONFLICT DO NOTHING`, run by `app db seed`.
- Every table has `id uuid primary key`, `version bigint not null default 1`, `created_at timestamptz not null default now()`, and `updated_at timestamptz not null default now()`.
- Mirror every domain invariant that the DB can express as a `CHECK`, `UNIQUE`, `NOT NULL`, or FK constraint.
- Add an index for every unique rule and every query the spec declares. The MVP list is newest first, so it uses the primary key (keyset on `id`).
- Never edit an applied migration. Add a new one instead.

## 2. Domain (`crates/domain/src/<res>/`)
- `mod.rs`: the entity `<Res>` with private fields, getters, and a constructor `<Res>::new(...) -> Result<Self, <Res>Error>`.
- Value objects are newtypes validated in `TryFrom` (`<Res>Name(String)`). An invalid value cannot be constructed.
- Business operations are methods that enforce invariants (`rename`, `change_price`). They never touch IO.
- `error.rs`: `<Res>Error` via `thiserror`.
- No serde, tokio, sqlx, or axum. Allowed: `thiserror`, `uuid`, and `time`.

## 3. Application (`crates/application/src/<res>/`)
- `ports.rs`: `#[async_trait] pub trait <Res>Repository: Send + Sync` with `insert` (returns `Inserted(<Res>)` or `AlreadyExists(<Res>)`, which makes Kafka creates idempotent), `find_by_id`, `list(PageRequest)`, `update(entity)`, and `delete` (returns whether a row was deleted). Port types are domain types or plain application structs, never row or DTO types.
- `commands.rs`: raw input structs (`Create<Res>Input`, `Update<Res>Input`), and the parser that turns them into commands made of domain value objects. The parser collects every field error into `AppError::Validation` ([api.md](../../../docs/conventions/api.md), input error rules). Both inbound adapters call it.
- `use_cases.rs`: `<Res>UseCases { repo: Arc<dyn <Res>Repository> }`, with one method per enabled operation. Generate the UUIDv7 for HTTP creates. Map domain errors to `AppError`.
- `fakes.rs` (`#[cfg(test)]` or a `testing` feature): in-memory implementations of the ports.
- `contract.rs` (feature `testing`; `crates/application/Cargo.toml` has no `testing` feature on a bare boilerplate, so the first resource build adds `[features] testing = []` there and enables it as a dev-dependency feature in `infrastructure` and `bootstrap`): `pub async fn <res>_repository_contract(repo: &dyn <Res>Repository)`. It is written once against the port, covering insert (and `AlreadyExists`), unique conflicts, get, keyset paging, update, and delete. The fake must pass it in a unit test ([database.md](../../../docs/conventions/database.md) AC-4).
- Errors: `AppError { NotFound, Conflict, Validation(Vec<FieldError>), Unavailable, Unauthorized, Forbidden, Internal }`.
- No sqlx, redis, axum, or rdkafka. `tokio` is allowed only for the shared `cache_status` task-local; resource modules never use it.

**Input and parser shape** (`commands.rs`; a fictional `widget` with a required single-line `name` and an optional multi-line `note`). The parser records every failure, then returns one `Validation`. `optional` returns `None` only after it records an error, so `.flatten()` is safe; a failed field leaves `errors` non-empty, which turns the result into `Err`. `parse_update` has the same shape.
```rust
use crate::error::{AppError, FieldCode, FieldError};
use crate::input::{Field, FieldErrors, Lines, UnknownFields, uuid_v7};
use domain::widget::{WidgetName, WidgetNote};
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct CreateWidgetInput {
    pub id: Option<Field<String>>, // Kafka only; HTTP leaves it None
    pub name: Field<String>,       // required, single-line
    pub note: Field<String>,       // optional, multi-line
    pub unknown: UnknownFields,
}

pub struct CreateWidget {
    pub id: Option<Uuid>, // None: the use case makes a UUIDv7
    pub name: WidgetName,
    pub note: Option<WidgetNote>,
}
```

```rust
pub fn parse_create(input: CreateWidgetInput) -> Result<CreateWidget, AppError> {
    let mut errors = FieldErrors::new();
    errors.unknown(&input.unknown);
    let id = input
        .id
        .and_then(|id| id.required("id", &mut errors))
        .and_then(|s| errors.check(uuid_v7("id", &s)));
    let name = input
        .name
        .required("name", &mut errors)
        .filter(|s| errors.text("name", s, Lines::Single))
        .and_then(|s| errors.check(WidgetName::try_from(s).map_err(|e| field_error("name", &e))));
    let note = input
        .note
        .optional("note", &mut errors)
        .flatten()
        .filter(|s| errors.text("note", s, Lines::Multi))
        .and_then(|s| errors.check(WidgetNote::try_from(s).map_err(|e| field_error("note", &e))));
    match name {
        Some(name) if errors.is_empty() => Ok(CreateWidget { id, name, note }),
        _ => Err(errors.into_error()),
    }
}
// field_error(field, &WidgetError) -> FieldError maps each domain error to its api.md code.
```
Every producer-supplied id goes through `input::uuid_v7`, never a plain `Uuid::parse_str`: [kafka.md](../../../docs/conventions/kafka.md) requires a UUIDv7, and the newest-first list depends on ids being time-ordered (a v4 or nil id would sort above every v7 id). Ids that are only looked up (update and delete paths) need to be UUIDs, not v7.

## 4. Infrastructure (`crates/infrastructure/src/`)
- `<platform>/<res>_repo.rs` (the MVP platform is `postgres`): `Pg<Res>Repository` implements the port, and an integration test runs `<res>_repository_contract` against it. SQL is native to the engine ([database.md](../../../docs/conventions/database.md)).
  - `<Res>Row` (`sqlx::FromRow`, serde) plus `TryFrom<<Res>Row> for <Res>`.
  - Only `sqlx::query!` or `query_as!` macros. Never use `format!` to build SQL.
  - Timestamps come from the database: `DEFAULT now()` on insert, `updated_at = now()` on update, and `RETURNING` the stored row.
  - Insert: `ON CONFLICT (id) DO NOTHING`. When no row is inserted, load the existing row and return `AlreadyExists`.
  - Update: `UPDATE ... WHERE id = $1`, which also sets `version = version + 1` (no version check in the MVP; the last write wins). If no row is affected, return `NotFound`.
  - Map a unique violation to `Conflict`, pool or connection errors with `postgres::unavailable(&self.pool, &err)` (never a hand-written `Unavailable`; its cause tells a full pool from a down database, [database.md](../../../docs/conventions/database.md) Pool errors), and any other error to `Internal`.
  - List: keyset on `id` descending, with `WHERE id < $cursor` and `LIMIT page.limit + 1` to detect a next page ([api.md](../../../docs/conventions/api.md)).
- `redis/<res>_cache.rs` (only when the spec says `**Cache**: enabled`; with `disabled`, skip this file and every cache step below): `Cached<Res>Repository<R>` wraps any `<Res>Repository`, built with `new(inner, cache: RedisCache, ttl: Duration)` ([cache.md](../../../docs/conventions/cache.md)).
  - Reads use `cache_aside::get_or_load(&self.cache, &key, self.ttl, || ...)`. The cached value is the JSON of `<Res>Row`, so convert the entity to the row before storing and back after a hit.
  - Writes call `inner`, then `cache_aside::invalidate(&self.cache, &key)` after the commit. A failed `DEL` is logged inside `invalidate` and never fails the write.
  - The key comes from the shared `self.cache.key("<res>", id)`.
  - Never build the key, the TTL, the fail-open path, or the `X-Cache` outcome by hand: `cache_aside` does all of them.

**Decorator shape** (`redis/widget_cache.rs`). The loader returns the row, so `get_or_load` stores the row's JSON. A row that fails `TryFrom` is data corruption, so it maps to `Internal`.
```rust
use super::cache_aside::{self, RedisCache};
use crate::postgres::widget_repo::WidgetRow;
// ... plus the AppError, WidgetRepository, Widget, and Uuid imports

pub struct CachedWidgetRepository<R> {
    inner: R,
    cache: RedisCache,
    ttl: Duration,
}
// new(inner, cache, ttl); insert and list delegate to `inner`

#[async_trait]
impl<R: WidgetRepository> WidgetRepository for CachedWidgetRepository<R> {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Widget>, AppError> {
        let key = self.cache.key("widget", id);
        let row = cache_aside::get_or_load(&self.cache, &key, self.ttl, || async {
            let found = self.inner.find_by_id(id).await?;
            Ok::<_, AppError>(found.as_ref().map(WidgetRow::from))
        })
        .await?;
        row.map(Widget::try_from).transpose().map_err(|e| AppError::Internal(e.to_string()))
    }

    async fn update(&self, widget: &Widget) -> Result<Widget, AppError> {
        let updated = self.inner.update(widget).await?; // commits first
        cache_aside::invalidate(&self.cache, &self.cache.key("widget", updated.id())).await;
        Ok(updated)
    }
    // delete: the same, invalidating after `inner.delete(id)` returns
}
```

## 5. Inbound adapters
**HTTP (`crates/http-api/src/<res>/`)**
- `dto.rs`: `Create<Res>Request` and `Update<Res>Request`, where every field is `application::input::Field<T>` and `#[serde(flatten)] unknown: UnknownFields` captures the rest ([api.md](../../../docs/conventions/api.md), input error rules). They do no validation; they convert into the application's raw input structs. `<Res>Response` lists every field explicitly, emits optional fields as `null`, and writes timestamps in RFC 3339 UTC. Never serialize an entity or row directly.
- `handlers.rs`: each handler is thin (extract, map to a command, call the use case, map to a response). No business logic and no SQL.
- `routes.rs`: `/api/v1/<res_plural>` on the **protected** router, with `RequireScope("<res_plural>:read" | "<res_plural>:write")` on every route ([runtime.md](../../../docs/conventions/runtime.md)).

**Request DTO** (`dto.rs`). It converts to the application's raw input and does no checks. `WidgetResponse` (not shown) lists every field and has a `From<Widget>`.
```rust
use application::input::{Field, UnknownFields};
use application::widget::commands::CreateWidgetInput;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CreateWidgetRequest {
    name: Field<String>,
    note: Field<String>,
    #[serde(flatten)]
    unknown: UnknownFields,
}

impl From<CreateWidgetRequest> for CreateWidgetInput {
    fn from(r: CreateWidgetRequest) -> Self {
        Self {
            id: None,
            name: r.name,
            note: r.note,
            unknown: r.unknown,
        }
    }
}
```

**Handlers** (`handlers.rs`). `?` turns an `AppError` into an `ApiError`. The parser runs in the handler, so a validation error returns before the use case.
```rust
use std::sync::Arc;

use application::widget::{commands::parse_create, use_cases::WidgetUseCases};
use axum::Json;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;

use super::dto::{CreateWidgetRequest, WidgetResponse};
use crate::error::ApiError;
use crate::extract::{IdPath, JsonBody};
use crate::pagination::{ListQuery, PageResponse};

type Uc = State<Arc<WidgetUseCases>>;

pub async fn create(
    State(uc): Uc,
    JsonBody(body): JsonBody<CreateWidgetRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let widget = uc.create(parse_create(body.into())?).await?;
    let location = format!("/api/v1/widgets/{}", widget.id());
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, location)],
        Json(WidgetResponse::from(widget)),
    ))
}

pub async fn fetch(State(uc): Uc, IdPath(id): IdPath) -> Result<Json<WidgetResponse>, ApiError> {
    Ok(Json(uc.get(id).await?.into()))
}

pub async fn list(
    State(uc): Uc,
    ListQuery(page): ListQuery,
) -> Result<Json<PageResponse<WidgetResponse>>, ApiError> {
    let page = uc.list(page).await?;
    Ok(Json(PageResponse::from_page(page, WidgetResponse::from)))
}
```

**Routes** (`routes.rs`). Layer each method router with its own scope before merging them: a later `route_layer` on a combined method router would wrap the earlier method too.
```rust
use std::sync::Arc;

use application::widget::use_cases::WidgetUseCases;
use axum::Router;
use axum::routing::{get, post};

use super::handlers::{create, fetch, list};
use crate::auth::RequireScope;

// Paths are relative to `/api/v1`: wiring merges this router into `api_v1`.
pub fn routes(uc: Arc<WidgetUseCases>) -> Router {
    Router::new()
        .route(
            "/widgets",
            get(list)
                .route_layer(RequireScope("widgets:read"))
                .merge(post(create).route_layer(RequireScope("widgets:write"))),
        )
        .route(
            "/widgets/{id}",
            get(fetch).route_layer(RequireScope("widgets:read")),
        )
        .with_state(uc)
}
```

**Kafka (`crates/kafka-api/src/<res>/`)** (only when the spec declares commands)
- `handlers.rs`: a `kafka_api::CommandHandler` for the resource. It declares its topic (`<res_plural>.commands.v1`, which is authoritative: it must match the `kafka-init` names, and `cargo xtask kafka-produce` needs `--topic <name>` when the plural is irregular) and its types (`<res>.create`, `<res>.update`, `<res>.delete`), and `handle(type, payload)` calls the use cases. It never touches the envelope, offsets, or the DLQ: the generic consumer parses the envelope, writes the DLQ message from the returned `AppError`, and commits ([kafka.md](../../../docs/conventions/kafka.md)).
- Payloads: create = the POST body plus `id`, update = the PUT body plus `id`, delete = `{id}`. Declare each as a `#[serde(default)]` struct of `application::input::Field<T>` fields with `#[serde(flatten)] unknown: UnknownFields` (the same shape as the HTTP request DTO plus `id`; `kafka-api` cannot import `http-api`), decode it with `kafka_api::payload::decode`, and pass it to the same application parser as HTTP. Never write a second validation path.

**Handler shape** (`handlers.rs`). It decodes, builds the same raw input as HTTP, and calls the same parser, so it holds no validation. `?` returns the parser's `AppError` to the consumer, which writes the DLQ message. Create is idempotent because the use case treats `AlreadyExists` as success.
```rust
use application::input::{Field, UnknownFields};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default)]
struct CreatePayload {
    id: Field<String>, // the HTTP request fields plus `id`
    name: Field<String>,
    note: Field<String>,
    #[serde(flatten)]
    unknown: UnknownFields,
}
```

```rust
use crate::handler::CommandHandler;
use crate::payload::decode;
// ... plus AppError, async_trait, Arc, CreateWidgetInput, parse_create, and WidgetUseCases

pub struct WidgetCommandHandler {
    uc: Arc<WidgetUseCases>,
}

#[async_trait]
impl CommandHandler for WidgetCommandHandler {
    fn topic(&self) -> &str {
        "widgets.commands.v1"
    }

    fn types(&self) -> &[&'static str] {
        &["widget.create", "widget.update", "widget.delete"]
    }

    async fn handle(&self, message_type: &str, payload: serde_json::Value) -> Result<(), AppError> {
        match message_type {
            "widget.create" => {
                let p: CreatePayload = decode(payload)?;
                let input = CreateWidgetInput { id: Some(p.id), name: p.name, note: p.note, unknown: p.unknown };
                self.uc.create(parse_create(input)?).await.map(drop)
            }
            // "widget.update" and "widget.delete": the same shape
            _ => Err(AppError::Internal("unhandled command type".into())),
        }
    }
}
```
- Register the handler in `wiring.rs` (section 6), which is the one place Kafka handlers are registered.
- Add `<res_plural>.commands.v1` and `<res_plural>.commands.v1.dlq` to `TOPICS` in the `kafka-init` service of `docker-compose.yml` ([kafka.md](../../../docs/conventions/kafka.md), topic setup).

## 6. Wiring (`crates/bootstrap/src/`)
- `wiring::Infra { pool, cache: Option<CacheInfra { redis, ttl }> }` is built once per process by `Infra::connect` (in `serve`, `consume`, and `all`) and passed to `http_app` (which gives `/readyz` the same pool and Redis connection) and to `kafka_handlers`. In `all`, HTTP and Kafka share one instance. `cache` is `Some` only while `ENABLE_FEATURES` contains `cache`.
- A resource builder takes `&Infra`. **Never open a pool or Redis connection in a resource builder** (`postgres::connect_lazy`, `redis_cache`): every extra pool adds up to the sqlx 0.9.0 default of 10 connections ([`PoolOptions::new`](https://docs.rs/sqlx-core/0.9.0/src/sqlx_core/pool/options.rs.html)) per process, and `/readyz` would then check a pool that serves no resource.
- Build the repository from `infra.pool` (the MVP has only `postgres`; a new engine changes `Infra::connect`, [database.md](../../../docs/conventions/database.md)). Wrap it in `Cached<Res>Repository` only when the spec says `**Cache**: enabled` and `infra.cache` is `Some` ([runtime.md](../../../docs/conventions/runtime.md)). A resource with `**Cache**: disabled` always uses the plain repository.
  - Pass `cache.redis.clone()` and the TTL: `cache.ttl` (`CACHE_TTL_SECS`) by default. When the spec sets its own TTL, pass a `<RES>_CACHE_TTL: Duration` constant in `redis/<res>_cache.rs` whose comment links to the spec. Never add a per-resource env var ([cache.md](../../../docs/conventions/cache.md) AC-5).
- Build `<Res>UseCases`, merge the router, and push the resource's `CommandHandler` into the list `wiring::kafka_handlers` returns (only when the spec declares commands). The dispatcher subscribes to the topics of the registered handlers, so nothing else names a topic.

**Wiring shape** (`wiring.rs`). One helper builds the use cases for both transports from the shared `infra`.
```rust
// Both transports call this with the same `infra`. `**Cache**: disabled`: ignore `infra.cache` and always use `Arc::new(repo)`.
fn widget_use_cases(infra: &Infra) -> Arc<WidgetUseCases> {
    let repo = PgWidgetRepository::new(infra.pool.clone());
    let repo: Arc<dyn WidgetRepository> = match &infra.cache {
        Some(cache) => Arc::new(CachedWidgetRepository::new(repo, cache.redis.clone(), cache.ttl)),
        None => Arc::new(repo),
    };
    Arc::new(WidgetUseCases::new(repo))
}
```

```rust
// in `http_app(infra, ...)`:
let widgets = widget_use_cases(infra);
let api_v1 = Router::new().merge(http_api::widget::routes::routes(widgets));
```

```rust
// in `kafka_handlers(infra)` (rename `_infra`): the same `infra` as HTTP, so one pool in `all`
let widgets = widget_use_cases(infra);
let handlers: Vec<Arc<dyn CommandHandler>> = vec![
    Arc::new(WidgetCommandHandler::new(widgets)), // one line per resource
];
```

## 7. Integration tests (`crates/bootstrap/tests/<res>_*.rs`)
- Use testcontainers for Postgres, Redis, and Kafka as needed.
- Write one or more tests for each AC, named `ac_<n>_<behavior>`. Cover the conventions from [docs/conventions/api.md](../../../docs/conventions/api.md): 201 and Location, 404, 422 with field codes, and pagination.
- Tests must be deterministic: no sleeps for ordering, and every await on the system under test has a timeout.

## Comments
- Follow the Code comments row in [stack-architecture.md](../../../docs/conventions/stack-architecture.md): cite the convention once in the module's `//!` header, cite a rule on an item only where the choice is not obvious, and use the form `(<file>.md <ID>)`. `make ref-check` fails on a reference that does not resolve.

## Forbidden everywhere (a review blocker)
- `unwrap`, `expect`, or `panic!` outside tests
- SQL built with `format!`
- Business logic in handlers or consumers
- An unbounded query (no `LIMIT`)
- Blocking calls inside async code
- Holding a DB connection across an external IO await
- `anyhow` outside bootstrap
- Copying a generic building block into a resource module
