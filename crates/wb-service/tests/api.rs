use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
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
    let app = router(Arc::new(RwLock::new(status())));
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
