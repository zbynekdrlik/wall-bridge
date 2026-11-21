//! The dashboard's text, computed from the `/api/status` JSON (#31). The Leptos UI only
//! renders what these functions return.

use wb_core::dashboard::{headline, rows, StatusView};
use wb_core::health::{Reason, State};
use wb_core::pacing::PacingWindow;
use wb_core::sender::SenderState;
use wb_core::status::{DisplayState, Status, TopologyState};

fn status(state: State) -> Status {
    Status {
        version: "0.2.24".into(),
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
            listed: true,
            width: 7680,
            height: 1080,
            last_picture_age_ms: Some(33),
        },
        pacing: PacingWindow {
            vblanks: 600,
            new: 300,
            repeats: 300,
            missed_vblanks: 2,
            arrival_ms_p50: 3.26,
            arrival_ms_p90: 9.04,
            ..Default::default()
        },
        warnings: vec![],
        slow_shows: vec![],
        slow_shows_total: 0,
    }
}

/// Through the wire, exactly as the browser receives it.
fn view(s: &Status) -> StatusView {
    serde_json::from_str(&serde_json::to_string(s).unwrap()).unwrap()
}

fn value<'a>(r: &'a [(String, String)], label: &str) -> &'a str {
    &r.iter()
        .find(|(l, _)| l == label)
        .unwrap_or_else(|| panic!("no row {label}"))
        .1
}

#[test]
fn the_view_reads_the_status_json() {
    let v = view(&status(State::Ok));
    assert_eq!(v.version, "0.2.24");
    assert_eq!(v.state, "ok");
    assert_eq!(v.uptime_s, 42);
    assert_eq!(v.sender.last_picture_age_ms, Some(33));
}

#[test]
fn headline_ok() {
    assert_eq!(
        headline(&view(&status(State::Ok))),
        ("ok".to_string(), "Stena OK".to_string())
    );
}

#[test]
fn headline_degraded_lists_reasons() {
    let v = view(&status(State::Degraded(vec![
        Reason::SenderLost,
        Reason::MissedVblanks(3),
    ])));
    assert_eq!(
        headline(&v),
        (
            "degraded".to_string(),
            "Stena beží, ale niečo nesedí — sender_lost, {\"missed_vblanks\":3}".to_string()
        )
    );
}

#[test]
fn headline_failed() {
    let v = view(&status(State::Failed(vec![Reason::DisplayNotAcquired])));
    assert_eq!(
        headline(&v),
        (
            "failed".to_string(),
            "Stena nie je na mostíku — display_not_acquired".to_string()
        )
    );
}

#[test]
fn rows_show_the_wall_and_the_sender() {
    let r = rows(&view(&status(State::Ok)));
    assert_eq!(value(&r, "Displej"), "7680×1080 @ 59.978 Hz");
    assert_eq!(value(&r, "Mimo desktopu"), "áno");
    assert_eq!(
        value(&r, "Zdroj (Spout)"),
        "Arena - Screen 2 — pripojený 7680×1080"
    );
    assert_eq!(value(&r, "Posledný nový obraz"), "33 ms");
    assert_eq!(value(&r, "Nové / opakované (10 s)"), "300 / 300");
    assert_eq!(value(&r, "Zmeškané vblanky (10 s)"), "2");
    assert_eq!(value(&r, "Príchod p50 / p90"), "3.3 / 9.0 ms");
    assert_eq!(value(&r, "Surround"), "podľa definície");
    assert_eq!(value(&r, "Beží"), "42 s");
    assert!(r.iter().all(|(l, _)| l != "Upozornenia"));
}

#[test]
fn rows_for_a_lost_wall() {
    let mut s = status(State::Ok);
    s.display.target_acquired = false;
    s.display.specialized = false;
    s.sender.connected = false;
    s.sender.last_picture_age_ms = None;
    s.topology.matches = false;
    s.warnings = vec!["prvé".into(), "druhé".into()];
    let r = rows(&view(&s));
    assert_eq!(value(&r, "Displej"), "nezískaný");
    assert_eq!(value(&r, "Mimo desktopu"), "nie");
    assert_eq!(value(&r, "Zdroj (Spout)"), "Arena - Screen 2 — chýba");
    assert_eq!(value(&r, "Posledný nový obraz"), "—");
    assert_eq!(value(&r, "Surround"), "nezhoduje sa");
    assert_eq!(value(&r, "Upozornenia"), "prvé\ndruhé");
}

#[test]
fn rows_keep_their_order() {
    let r = rows(&view(&status(State::Ok)));
    let labels: Vec<&str> = r.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Displej",
            "Mimo desktopu",
            "Zdroj (Spout)",
            "Posledný nový obraz",
            "Nové / opakované (10 s)",
            "Zmeškané vblanky (10 s)",
            "Príchod p50 / p90",
            "Surround",
            "Beží",
        ]
    );
}
