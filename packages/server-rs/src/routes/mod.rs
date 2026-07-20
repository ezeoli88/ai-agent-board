//! HTTP route handlers for the agent-board API.
//!
//! This module organizes all API routes into sub-routers by domain:
//! - `data` - Bulk data export/import/delete
//! - `repos` - Repository CRUD and learned patterns
//! - `secrets` - Encrypted secret management (AI keys, GitHub/GitLab tokens)
//! - `setup` - Application setup, agent detection, settings, OAuth
//! - `specs` - Standalone spec-driven development demo workflows
//! - `tasks` - Task CRUD and lifecycle management

pub mod data;
pub mod repos;
pub mod secrets;
pub mod setup;
pub mod specs;
pub mod tasks;

use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Router,
};
use futures::stream::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::AppState;

/// Builds the combined API router with all route groups nested under `/api`.
///
/// Route groups:
/// - `/api/events` - Global SSE data-change stream
/// - `/api/data/*` - Data export/import/delete
/// - `/api/repos/*` - Repository management
/// - `/api/secrets/*` - Secret management
/// - `/api/setup/*` - Setup and configuration
/// - `/api/specs/*` - Standalone spec-driven development demo workflows
/// - `/api/tasks/*` - Task management
pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/api/events", get(data_events_sse))
        .route("/api/mcp", post(crate::mcp::mcp_handler))
        .nest("/api/data", data::router())
        .nest("/api/repos", repos::router())
        .nest("/api/secrets", secrets::router())
        .nest("/api/setup", setup::router())
        .nest("/api/specs", specs::router())
        .nest("/api/tasks", tasks::router())
}

/// GET /api/events — SSE stream of data-change events.
///
/// The frontend subscribes to this stream so TanStack Query can
/// invalidate caches in real time when repos/tasks change.
async fn data_events_sse(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.data_emitter.subscribe();

    let stream = BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(event) => {
                let json = serde_json::to_string(&event).unwrap_or_default();
                Some(Ok(Event::default().event("data-change").data(json)))
            }
            Err(_) => None, // lagged — skip
        }
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("ping"),
    )
}
