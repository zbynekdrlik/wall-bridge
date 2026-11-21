//! The UI's own logic, on the host (`cargo test`).

use std::time::Duration;
use wb_ui::{next_attempt, parse, reconnect_delay, DOWN_TEXT, VERSION};

#[test]
fn backoff_resets_after_an_open_socket() {
    assert_eq!(next_attempt(true, 3), 0);
    assert_eq!(next_attempt(false, 0), 1);
    assert_eq!(next_attempt(false, 2), 3);
}

#[test]
fn reconnect_delay_grows_and_caps() {
    assert_eq!(reconnect_delay(0), Duration::from_millis(500));
    assert_eq!(reconnect_delay(1), Duration::from_millis(1000));
    assert_eq!(reconnect_delay(3), Duration::from_millis(5000));
    assert_eq!(reconnect_delay(99), Duration::from_millis(5000));
}

#[test]
fn parse_reads_a_status_and_rejects_junk() {
    let json = r#"{"version":"1.2.3","state":"ok","reasons":[],"uptime_s":1,
        "display":{"specialized":true,"target_acquired":true,"width":7680,"height":1080,"hz":60.0},
        "topology":{"matches":true,"last_restore_ms":null,"failures":0,"gave_up":false},
        "sender":{"name":"Arena - Screen 2","connected":true,"listed":true,"width":7680,"height":1080,"last_picture_age_ms":5},
        "pacing":{"new":300,"repeats":300,"missed_vblanks":0,"arrival_ms_p50":1.0,"arrival_ms_p90":2.0},
        "warnings":[],"slow_shows":[],"slow_shows_total":0}"#;
    assert_eq!(parse(json).unwrap().version, "1.2.3");
    assert!(parse("not json").is_none());
}

#[test]
fn version_and_down_text() {
    assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    assert_eq!(DOWN_TEXT, "Služba neodpovedá");
}
