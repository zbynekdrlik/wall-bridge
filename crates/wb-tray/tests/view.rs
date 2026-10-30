use wb_tray::view::{icon_rgba, view, Light, Status};

fn st(state: &str, reasons: serde_json::Value) -> Status {
    serde_json::from_value(serde_json::json!({
        "version": "0.2.0", "state": state, "reasons": reasons, "uptime_s": 1
    }))
    .unwrap()
}

#[test]
fn ok_status_label() {
    let v = view(Some(&st("ok", serde_json::json!([]))));
    assert_eq!(v.light, Light::Green);
    assert_eq!(v.line, "Stena OK (v0.2.0)");
}

#[test]
fn degraded_shows_reason() {
    let v = view(Some(&st(
        "degraded",
        serde_json::json!(["sender_lost", {"missed_vblanks": 3}]),
    )));
    assert_eq!(v.light, Light::Amber);
    assert_eq!(
        v.line,
        "Stena beží, niečo nesedí (v0.2.0) – chýba obraz z Arény, zmeškané vblanky: 3"
    );
}

#[test]
fn failed_is_red() {
    let v = view(Some(&st(
        "failed",
        serde_json::json!(["display_not_acquired"]),
    )));
    assert_eq!(v.light, Light::Red);
    assert!(v.line.ends_with("stena nie je získaná"));
}

#[test]
fn down_when_unreachable() {
    let v = view(None);
    assert_eq!(v.light, Light::Grey);
    assert_eq!(v.line, "wall-bridge: služba neodpovedá");
}

#[test]
fn unknown_reason_passes_through() {
    let v = view(Some(&st(
        "degraded",
        serde_json::json!(["something_new", 7]),
    )));
    assert!(v.line.ends_with("something_new, 7"), "{}", v.line);
}

#[test]
fn icon_is_a_coloured_disc() {
    let px = icon_rgba(Light::Red);
    assert_eq!(px.len(), 16 * 16 * 4);
    let at = |x: usize, y: usize| &px[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4];
    assert_eq!(at(8, 8), &[0xcf, 0x22, 0x2e, 255]);
    assert_eq!(at(0, 0)[3], 0, "corner is transparent");
    assert_ne!(icon_rgba(Light::Green), px);
}
