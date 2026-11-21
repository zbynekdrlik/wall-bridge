//! wall-bridge dashboard (#31): Leptos CSR, built to WASM by Trunk and embedded in
//! `wall-bridge.exe`. Live state comes over the `/api/ws` WebSocket; what is shown comes from
//! `wb_core::dashboard`, so the text is tested on Linux.

use leptos::prelude::*;
use std::time::Duration;
use wasm_bindgen::prelude::*;
use wb_core::dashboard::{headline, rows, StatusView};

/// The version the page was built with (the same workspace version as the engine).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Delay before reconnecting after the socket closed, doubling up to the last value.
pub const RECONNECT_MS: [u64; 4] = [500, 1000, 2000, 5000];

/// Shown while no status arrives.
pub const DOWN_TEXT: &str = "Služba neodpovedá";

/// The reconnect attempt after a close: back to the first delay when the socket had opened,
/// else one step further.
pub fn next_attempt(opened: bool, attempt: usize) -> usize {
    if opened {
        0
    } else {
        attempt + 1
    }
}

/// Delay before reconnect attempt `attempt`.
pub fn reconnect_delay(attempt: usize) -> Duration {
    Duration::from_millis(RECONNECT_MS[attempt.min(RECONNECT_MS.len() - 1)])
}

/// Parses one WebSocket message (the `/api/status` JSON).
pub fn parse(msg: &str) -> Option<StatusView> {
    serde_json::from_str(msg).ok()
}

/// The page for a given status (`None` = the service does not answer).
#[component]
pub fn Dashboard(status: ReadSignal<Option<StatusView>>) -> impl IntoView {
    let line = move || match status.get() {
        Some(s) => headline(&s),
        None => ("down".to_string(), DOWN_TEXT.to_string()),
    };
    let table = move || {
        status
            .get()
            .map(|s| rows(&s))
            .unwrap_or_default()
            .into_iter()
            .map(|(label, value)| view! { <tr><td>{label}</td><td>{value}</td></tr> })
            .collect_view()
    };
    view! {
        <h1>"wall-bridge " <span id="version">{format!("v{VERSION}")}</span></h1>
        <div id="state" class=move || line().0>{move || line().1}</div>
        <table id="rows">{table}</table>
    }
}

fn ws_url() -> Option<String> {
    let loc = web_sys::window()?.location();
    let scheme = if loc.protocol().ok()? == "https:" {
        "wss"
    } else {
        "ws"
    };
    Some(format!("{scheme}://{}/api/ws", loc.host().ok()?))
}

/// Opens the status socket; on close, clears the status and reconnects with back-off.
fn connect(set: WriteSignal<Option<StatusView>>, attempt: usize) {
    let Some(url) = ws_url() else { return };
    let ws = match web_sys::WebSocket::new(&url) {
        Ok(ws) => ws,
        Err(_) => {
            retry(set, attempt);
            return;
        }
    };
    let on_msg =
        Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
            if let Some(text) = e.data().as_string() {
                if let Some(s) = parse(&text) {
                    set.set(Some(s));
                }
            }
        });
    ws.set_onmessage(Some(on_msg.as_ref().unchecked_ref()));
    on_msg.forget();
    let opened = std::rc::Rc::new(std::cell::Cell::new(false));
    let on_open = {
        let opened = opened.clone();
        Closure::<dyn FnMut()>::new(move || opened.set(true))
    };
    ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
    on_open.forget();
    let on_close =
        Closure::<dyn FnMut(web_sys::CloseEvent)>::new(move |_e: web_sys::CloseEvent| {
            set.set(None);
            retry(set, next_attempt(opened.get(), attempt));
        });
    ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
    on_close.forget();
}

fn retry(set: WriteSignal<Option<StatusView>>, attempt: usize) {
    set_timeout(move || connect(set, attempt), reconnect_delay(attempt));
}

fn register_service_worker() {
    if let Some(w) = web_sys::window() {
        // The promise is not awaited: without a service worker the page still works.
        let _ = w.navigator().service_worker().register("/sw.js");
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    let (status, set_status) = signal(None::<StatusView>);
    mount_to_body(move || view! { <Dashboard status=status/> });
    connect(set_status, 0);
    register_service_worker();
}
