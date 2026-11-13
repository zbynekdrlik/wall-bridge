//! `GET /api/status` (JSON), `GET /` (dashboard) and `POST /api/stop` (loopback only).

use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tracing::{info, warn};
use wb_core::status::Status;

pub type Shared = Arc<RwLock<Status>>;

const DASHBOARD: &str = include_str!("../static/index.html");

#[derive(Clone)]
struct ApiState {
    status: Shared,
    /// Set by `POST /api/stop`; the engine leaves its loop between probes and exits (#20).
    stop: Arc<AtomicBool>,
}

/// The server must be served with `into_make_service_with_connect_info::<SocketAddr>()`,
/// so `/api/stop` can tell loopback callers from the LAN.
pub fn router(status: Shared, stop: Arc<AtomicBool>) -> Router {
    Router::new()
        .route("/api/status", get(status_json))
        .route("/api/stop", post(stop_engine))
        .route("/", get(dashboard))
        .with_state(ApiState { status, stop })
}

async fn status_json(State(s): State<ApiState>) -> impl IntoResponse {
    let body = s.status.read().expect("status lock poisoned").clone();
    ([(header::CACHE_CONTROL, "no-store")], Json(body))
}

/// Header the stop request must carry. A web page in a browser on this PC can POST to
/// 127.0.0.1 without a CORS preflight, but cannot add a custom header without one, and the
/// API grants no CORS.
pub const STOP_HEADER: &str = "X-WallBridge-Stop";

async fn stop_engine(
    State(s): State<ApiState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> StatusCode {
    if !peer.ip().is_loopback() {
        warn!(%peer, "stop request refused: not from this PC");
        return StatusCode::FORBIDDEN;
    }
    if headers.get(STOP_HEADER).and_then(|v| v.to_str().ok()) != Some("1") {
        warn!(%peer, "stop request refused: no {STOP_HEADER} header");
        return StatusCode::FORBIDDEN;
    }
    info!(%peer, "graceful stop requested");
    s.stop.store(true, Ordering::SeqCst);
    StatusCode::ACCEPTED
}

async fn dashboard(State(s): State<ApiState>) -> Html<String> {
    let v = s
        .status
        .read()
        .expect("status lock poisoned")
        .version
        .clone();
    Html(DASHBOARD.replace("{{VERSION}}", &v))
}
