//! Serves the real dashboard and status API with a fixed status, for the Playwright test
//! (`e2e/`). Usage: `cargo run -p wb-service --example dashboard_fixture -- <port> <state>`.

use std::sync::{Arc, RwLock};
use wb_core::health::{Reason, State};
use wb_core::pacing::PacingWindow;
use wb_core::sender::SenderState;
use wb_core::status::{DisplayState, Status, TopologyState};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let port: u16 = args.next().and_then(|p| p.parse().ok()).unwrap_or(18930);
    let state = match args.next().as_deref() {
        Some("degraded") => State::Degraded(vec![Reason::SenderLost]),
        Some("failed") => State::Failed(vec![Reason::DisplayNotAcquired]),
        _ => State::Ok,
    };
    let status = Status {
        version: env!("CARGO_PKG_VERSION").into(),
        state,
        uptime_s: 42,
        display: DisplayState {
            specialized: true,
            target_acquired: true,
            width: 7680,
            height: 1080,
            hz: 59.978,
        },
        topology: TopologyState {
            matches: true,
            last_restore_ms: None,
            failures: 0,
            gave_up: false,
        },
        sender: SenderState {
            name: "Arena - Screen 2".into(),
            connected: true,
            width: 7680,
            height: 1080,
            last_picture_age_ms: Some(33),
        },
        pacing: PacingWindow {
            vblanks: 600,
            new: 300,
            repeats: 300,
            ..Default::default()
        },
        warnings: vec![],
    };
    let app = wb_service::api::router(Arc::new(RwLock::new(status)));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("bind");
    println!("listening on {port}");
    axum::serve(listener, app).await.expect("serve");
}
