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
    let v = view(Some(&st(
        "failed",
        serde_json::json!(["display_on_desktop"]),
    )));
    assert!(v.line.ends_with("stena je na ploche Windows"), "{}", v.line);
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
fn icon_is_the_three_panel_wall() {
    // A plain disc looked like DanteSync's icon in the same tray (owner 30.10.2026).
    let px = icon_rgba(Light::Red);
    assert_eq!(px.len(), 16 * 16 * 4);
    let at = |x: usize, y: usize| &px[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4];
    let red = [0xcf, 0x22, 0x2e, 255];
    for x in [0, 2, 4, 6, 8, 9, 11, 13, 15] {
        assert_eq!(at(x, 8), &red, "panel pixel at x={x}");
    }
    for x in [5, 10] {
        assert_eq!(at(x, 8)[3], 0, "gap between panels at x={x}");
    }
    for y in [0, 3, 12, 15] {
        assert_eq!(at(8, y)[3], 0, "above/below the wall at y={y}");
    }
    assert_eq!(at(0, 4), &red, "top-left panel corner");
    assert_eq!(at(15, 11), &red, "bottom-right panel corner");
    assert_ne!(icon_rgba(Light::Green), px);
}
