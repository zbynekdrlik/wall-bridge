//! `GET /api/status` (JSON), `GET /api/ws` (the same JSON, pushed every second), the dashboard
//! (the Leptos app from `crates/wb-ui`, embedded at build time, #31) and `POST /api/stop`
//! (loopback only).

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tracing::{debug, info, warn};
use wb_core::status::Status;

pub type Shared = Arc<RwLock<Status>>;

/// How often `/api/ws` pushes the status.
pub const WS_PUSH: Duration = Duration::from_secs(1);

/// Trunk's `dist/` of `crates/wb-ui`. Built before wb-service (`trunk build --release` in
/// `crates/wb-ui`, or the CI `ui` job); the build fails without it, so no exe ships without
/// the dashboard.
#[derive(rust_embed::Embed)]
#[folder = "../wb-ui/dist/"]
#[exclude = ".*"]
struct Ui;

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
        .route("/api/ws", get(status_ws))
        .route("/api/stop", post(stop_engine))
        .fallback(ui_file)
        .with_state(ApiState { status, stop })
}

fn status_text(s: &ApiState) -> String {
    let body = s.status.read().expect("status lock poisoned").clone();
    serde_json::to_string(&body).expect("status serializes")
}

async fn status_json(State(s): State<ApiState>) -> impl IntoResponse {
    let body = s.status.read().expect("status lock poisoned").clone();
    ([(header::CACHE_CONTROL, "no-store")], Json(body))
}

async fn status_ws(
    State(s): State<ApiState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| push_status(socket, s, peer))
}

/// Sends the status at once and then every [`WS_PUSH`] until the client goes away.
async fn push_status(mut socket: WebSocket, s: ApiState, peer: SocketAddr) {
    info!(%peer, "dashboard socket opened");
    let mut tick = tokio::time::interval(WS_PUSH);
    let mut sent: u64 = 0;
    loop {
        tick.tick().await;
        if let Err(e) = socket.send(Message::Text(status_text(&s).into())).await {
            info!(%peer, sent, err = ?e, "dashboard socket closed");
            return;
        }
        sent += 1;
    }
}

/// The dashboard's files; `/` is `index.html`. Unknown paths are 404.
async fn ui_file(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match Ui::get(path) {
        Some(f) => {
            debug!(path, bytes = f.data.len(), "dashboard file");
            let mime = f.metadata.mimetype().to_string();
            // index.html and the service worker must not be cached across upgrades; the
            // hashed JS/WASM names change with every build.
            let cache = if path == "index.html" || path == "sw.js" {
                "no-cache"
            } else {
                "public, max-age=31536000, immutable"
            };
            (
                [
                    (header::CONTENT_TYPE, mime),
                    (header::CACHE_CONTROL, cache.into()),
                ],
                f.data.into_owned(),
            )
                .into_response()
        }
        None => {
            debug!(path, "no such dashboard file");
            StatusCode::NOT_FOUND.into_response()
        }
    }
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
