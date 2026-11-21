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
            listed: true,
            width: 7680,
            height: 1080,
            last_picture_age_ms: Some(20),
        },
        pacing: PacingWindow::default(),
        warnings: vec![],
        slow_shows: vec![],
        slow_shows_total: 0,
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

/// GET returning (status, content-type, body).
async fn get_typed(path: &str) -> (StatusCode, String, Vec<u8>) {
    let app = router(
        Arc::new(RwLock::new(status())),
        Arc::new(AtomicBool::new(false)),
    );
    let res = app
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let code = res.status();
    let ct = res
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    (code, ct, body.to_vec())
}

// The dashboard is the Leptos app built by Trunk (#31); the version is rendered by the WASM
// (Playwright reads it from the DOM), so the page itself carries the app's assets.
#[tokio::test]
async fn dashboard_serves_the_leptos_app() {
    let (code, ct, body) = get_typed("/").await;
    assert_eq!(code, StatusCode::OK);
    assert!(ct.starts_with("text/html"), "{ct}");
    let html = String::from_utf8(body).unwrap();
    assert!(html.contains("_bg.wasm"), "no wasm in the page");
    assert!(html.contains(r#"rel="manifest""#));
}

#[tokio::test]
async fn wasm_and_script_are_served_with_their_types() {
    let (_, _, body) = get_typed("/").await;
    let html = String::from_utf8(body).unwrap();
    let asset = |ext: &str| -> String {
        let end = html.find(ext).expect("asset in page") + ext.len();
        let start = html[..end].rfind('/').unwrap();
        html[start..end].to_string()
    };
    let (code, ct, wasm) = get_typed(&asset("_bg.wasm")).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(ct, "application/wasm");
    assert_eq!(&wasm[..4], b"\0asm");
    let (code, ct, _) = get_typed(&asset(".js")).await;
    assert_eq!(code, StatusCode::OK);
    assert!(ct.contains("javascript"), "{ct}");
}

#[tokio::test]
async fn pwa_files_are_served() {
    let (code, ct, _) = get_typed("/manifest.webmanifest").await;
    assert_eq!(code, StatusCode::OK);
    assert!(ct.contains("manifest") || ct.contains("json"), "{ct}");
    let (code, ct, _) = get_typed("/sw.js").await;
    assert_eq!(code, StatusCode::OK);
    assert!(ct.contains("javascript"), "{ct}");
    let (code, _, _) = get_typed("/icon.svg").await;
    assert_eq!(code, StatusCode::OK);
}

#[tokio::test]
async fn ws_pushes_the_status() {
    use futures_util::StreamExt;
    let app = router(
        Arc::new(RwLock::new(status())),
        Arc::new(AtomicBool::new(false)),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/api/ws"))
        .await
        .unwrap();
    for _ in 0..2 {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(3), ws.next())
            .await
            .expect("a status within 3 s")
            .unwrap()
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
        assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(v["state"], "ok");
    }
}

#[tokio::test]
async fn unknown_path_is_404() {
    let (code, _) = get("/nope").await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

/// POSTs /api/stop as if from `peer`; returns the status code and the stop flag after it.
async fn stop_from(peer: &str) -> (StatusCode, bool) {
    stop_with(peer, true).await
}

async fn stop_with(peer: &str, header: bool) -> (StatusCode, bool) {
    let stop = Arc::new(AtomicBool::new(false));
    let app = router(Arc::new(RwLock::new(status())), stop.clone());
    let mut b = Request::post("/api/stop");
    if header {
        b = b.header("X-WallBridge-Stop", "1");
    }
    let mut req = b.body(Body::empty()).unwrap();
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

#[tokio::test]
async fn stop_without_the_header_is_refused() {
    // A web page in a browser on the PC can POST to 127.0.0.1 without preflight; a custom
    // header cannot be sent cross-site without CORS, which the API does not grant.
    assert_eq!(
        stop_with("127.0.0.1:50000", false).await,
        (StatusCode::FORBIDDEN, false)
    );
}

#[tokio::test]
async fn only_hashed_assets_are_cached_forever() {
    let (_, _, body) = get_typed("/").await;
    let html = String::from_utf8(body).unwrap();
    let end = html.find("_bg.wasm").unwrap() + "_bg.wasm".len();
    let wasm = &html[html[..end].rfind('/').unwrap()..end];
    let cache = |path: String| async move {
        let app = router(
            Arc::new(RwLock::new(status())),
            Arc::new(AtomicBool::new(false)),
        );
        let res = app
            .oneshot(Request::get(path.as_str()).body(Body::empty()).unwrap())
            .await
            .unwrap();
        res.headers()["cache-control"].to_str().unwrap().to_string()
    };
    assert!(cache(wasm.to_string()).await.contains("immutable"));
    for p in ["/", "/sw.js", "/manifest.webmanifest", "/icon.svg"] {
        assert_eq!(cache(p.to_string()).await, "no-cache", "{p}");
    }
}

#[tokio::test]
async fn ws_connections_are_capped() {
    let app = router(
        Arc::new(RwLock::new(status())),
        Arc::new(AtomicBool::new(false)),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let url = format!("ws://127.0.0.1:{port}/api/ws");
    let mut open = vec![];
    for _ in 0..wb_service::api::MAX_SOCKETS {
        open.push(tokio_tungstenite::connect_async(&url).await.unwrap());
    }
    assert!(tokio_tungstenite::connect_async(&url).await.is_err());
    drop(open.pop());
    // The freed slot is usable again once the server notices the close.
    let mut ok = false;
    for _ in 0..30 {
        if tokio_tungstenite::connect_async(&url).await.is_ok() {
            ok = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(ok, "slot not freed");
}

#[tokio::test]
async fn ws_answers_a_client_close() {
    use futures_util::{SinkExt, StreamExt};
    let app = router(
        Arc::new(RwLock::new(status())),
        Arc::new(AtomicBool::new(false)),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/api/ws"))
        .await
        .unwrap();
    ws.next().await.unwrap().unwrap();
    // The browser closes a silent socket and waits for the server's Close before it fires
    // onclose (and reconnects); the server must answer it.
    ws.send(tokio_tungstenite::tungstenite::Message::Close(None))
        .await
        .unwrap();
    let answered = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while let Some(m) = ws.next().await {
            match m {
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) | Err(_) => return true,
                Ok(_) => continue,
            }
        }
        true
    })
    .await;
    assert_eq!(answered, Ok(true), "no Close from the server within 3 s");
}
