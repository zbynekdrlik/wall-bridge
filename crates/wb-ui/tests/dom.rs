//! The Dashboard component in a real browser (`wasm-bindgen-test`, headless Chrome in CI).
#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use wasm_bindgen_test::*;
use wb_ui::{Dashboard, DOWN_TEXT};

wasm_bindgen_test_configure!(run_in_browser);

fn text(id: &str) -> String {
    document()
        .get_element_by_id(id)
        .unwrap_or_else(|| panic!("no #{id}"))
        .text_content()
        .unwrap_or_default()
}

#[wasm_bindgen_test]
fn shows_down_then_the_status() {
    let (status, set_status) = signal(None);
    let _handle = mount_to_body(move || view! { <Dashboard status=status/> });
    assert_eq!(text("state"), DOWN_TEXT);
    assert!(text("version").starts_with('v'));
    let json = r#"{"version":"1.2.3","state":"degraded","reasons":["sender_lost"],"uptime_s":1,
        "display":{"specialized":true,"target_acquired":true,"width":7680,"height":1080,"hz":59.978},
        "topology":{"matches":true},
        "sender":{"name":"Arena - Screen 2","connected":false,"width":0,"height":0,"last_picture_age_ms":null},
        "pacing":{"new":0,"repeats":600,"missed_vblanks":0,"arrival_ms_p50":0.0,"arrival_ms_p90":0.0},
        "warnings":[]}"#;
    set_status.set(wb_ui::parse(json));
    assert_eq!(text("state"), "Stena beží, ale niečo nesedí — sender_lost");
    assert!(text("rows").contains("7680×1080 @ 59.978 Hz"));
    assert!(text("rows").contains("Arena - Screen 2 — chýba"));
}
