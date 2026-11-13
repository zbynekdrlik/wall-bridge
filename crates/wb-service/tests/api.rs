use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tower::ServiceExt;
use wb_core::health::State;
use wb_core::pacing::PacingWindow;
use wb_core::sender::SenderState;
use wb_core::status::{DisplayState, Status, TopologyState};
use wb_service::api::router;

fn status() -> Status {
    Status {
        version: env!("CARGO_PKG_VERSION").into(),
        state: State::Ok,
        uptime_s: 5,
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
            last_picture_age_ms: Some(20),
        },
        pacing: PacingWindow::default(),
        warnings: vec![],
    }
}

async fn get(path: &str) -> (StatusCode, String) {
    let app = router(
        Arc::new(RwLock::new(status())),
        Arc::new(AtomicBool::new(false)),
    );
    let res = app
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let code = res.status();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    (code, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn api_status_returns_version() {
    let (code, body) = get("/api/status").await;
    assert_eq!(code, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(v["state"], "ok");
    assert_eq!(v["sender"]["last_picture_age_ms"], 20);
}

#[tokio::test]
async fn dashboard_shows_version() {
    let (code, body) = get("/").await;
    assert_eq!(code, StatusCode::OK);
    assert!(body.contains(&format!("v{}", env!("CARGO_PKG_VERSION"))));
    assert!(!body.contains("{{VERSION}}"));
}

#[tokio::test]
async fn unknown_path_is_404() {
    let (code, _) = get("/nope").await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

/// POSTs /api/stop as if from `peer`; returns the status code and the stop flag after it.
async fn stop_from(peer: &str) -> (StatusCode, bool) {
    let stop = Arc::new(AtomicBool::new(false));
    let app = router(Arc::new(RwLock::new(status())), stop.clone());
    let mut req = Request::post("/api/stop").body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
    let res = app.oneshot(req).await.unwrap();
    (res.status(), stop.load(Ordering::SeqCst))
}

#[tokio::test]
async fn stop_from_loopback_asks_the_engine_to_stop() {
    // The installer asks for a graceful stop so the engine never dies holding Arena's
    // SpoutAccessMutex (13.11. 04:30Z: an abandoned mutex Arena kept, wall black, #20).
    assert_eq!(
        stop_from("127.0.0.1:50000").await,
        (StatusCode::ACCEPTED, true)
    );
    assert_eq!(stop_from("[::1]:50000").await, (StatusCode::ACCEPTED, true));
}

#[tokio::test]
async fn stop_from_the_lan_is_refused() {
    // The API listens on 0.0.0.0 for the dashboard; nobody on the LAN may stop the wall.
    assert_eq!(
        stop_from("10.77.9.195:50000").await,
        (StatusCode::FORBIDDEN, false)
    );
}

#[tokio::test]
async fn stop_needs_post() {
    let (code, _) = get("/api/stop").await;
    assert_eq!(code, StatusCode::METHOD_NOT_ALLOWED);
}
