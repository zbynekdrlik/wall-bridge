//! `GET /api/status` (JSON) and `GET /` (dashboard).

use axum::extract::State;
use axum::http::header;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Json, Router};
use std::sync::{Arc, RwLock};
use wb_core::status::Status;

pub type Shared = Arc<RwLock<Status>>;

const DASHBOARD: &str = include_str!("../static/index.html");

pub fn router(status: Shared) -> Router {
    Router::new()
        .route("/api/status", get(status_json))
        .route("/", get(dashboard))
        .with_state(status)
}

async fn status_json(State(s): State<Shared>) -> impl IntoResponse {
    let body = s.read().expect("status lock poisoned").clone();
    ([(header::CACHE_CONTROL, "no-store")], Json(body))
}

async fn dashboard(State(s): State<Shared>) -> Html<String> {
    let v = s.read().expect("status lock poisoned").version.clone();
    Html(DASHBOARD.replace("{{VERSION}}", &v))
}
