use wb_core::config::Config;
use wb_core::health::{evaluate, Inputs, Reason, State};
use wb_core::pacing::PacingWindow;
use wb_core::sender::SenderState;
use wb_core::status::{DisplayState, Status, TopologyState};

fn sender(connected: bool, age: Option<u64>) -> SenderState {
    SenderState {
        name: "Arena - Screen 2".into(),
        connected,
        width: 7680,
        height: 1080,
        last_picture_age_ms: age,
    }
}

fn display(acquired: bool) -> DisplayState {
    DisplayState {
        specialized: true,
        target_acquired: acquired,
        width: 7680,
        height: 1080,
        hz: 59.978,
    }
}

fn topo(matches: bool, failures: u32) -> TopologyState {
    TopologyState {
        matches,
        last_restore_ms: None,
        failures,
        gave_up: false,
    }
}

fn window(missed: u64) -> PacingWindow {
    PacingWindow {
        vblanks: 600,
        new: 300,
        repeats: 300,
        missed_vblanks: missed,
        ..Default::default()
    }
}

#[test]
fn healthy_is_ok() {
    let s = evaluate(&Inputs {
        display: &display(true),
        topology: &topo(true, 0),
        sender: &sender(true, Some(40)),
        pacing: &window(0),
    });
    assert_eq!(s, State::Ok);
}

#[test]
fn lost_sender_is_degraded() {
    let s = evaluate(&Inputs {
        display: &display(true),
        topology: &topo(true, 0),
        sender: &sender(false, Some(5000)),
        pacing: &window(0),
    });
    assert_eq!(s, State::Degraded(vec![Reason::SenderLost]));
}

#[test]
fn no_target_is_failed() {
    let s = evaluate(&Inputs {
        display: &display(false),
        topology: &topo(true, 0),
        sender: &sender(true, Some(40)),
        pacing: &window(0),
    });
    assert_eq!(s, State::Failed(vec![Reason::DisplayNotAcquired]));
}

#[test]
fn topology_mismatch_and_missed_vblanks_listed() {
    let s = evaluate(&Inputs {
        display: &display(true),
        topology: &topo(false, 1),
        sender: &sender(true, Some(40)),
        pacing: &window(3),
    });
    assert_eq!(
        s,
        State::Degraded(vec![Reason::TopologyMismatch, Reason::MissedVblanks(3)])
    );
}

#[test]
fn topology_gave_up_is_failed() {
    let mut t = topo(false, 3);
    t.gave_up = true;
    let s = evaluate(&Inputs {
        display: &display(true),
        topology: &t,
        sender: &sender(true, Some(40)),
        pacing: &window(0),
    });
    assert_eq!(s, State::Failed(vec![Reason::TopologyGaveUp]));
}

#[test]
fn status_json_shape() {
    let st = Status {
        version: "0.2.0".into(),
        state: State::Degraded(vec![Reason::SenderLost]),
        uptime_s: 12,
        display: display(true),
        topology: topo(true, 0),
        sender: sender(false, None),
        pacing: window(0),
        warnings: vec!["nvidia driver 590.1 != 581.57".into()],
    };
    let v: serde_json::Value = serde_json::to_value(&st).unwrap();
    // JSON object key order carries no meaning: compare the key set.
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "display", "pacing", "reasons", "sender", "state", "topology", "uptime_s", "version",
            "warnings"
        ]
    );
    assert_eq!(v["state"], "degraded");
    assert_eq!(v["reasons"][0], "sender_lost");
    assert_eq!(v["sender"]["last_picture_age_ms"], serde_json::Value::Null);
    assert_eq!(v["display"]["hz"], 59.978);
    assert_eq!(v["pacing"]["missed_vblanks"], 0);
}

#[test]
fn config_defaults() {
    let c = Config::from_toml("").unwrap();
    assert_eq!(c.api.port, 8930);
    assert_eq!(c.sender.name, "Arena - Screen 2");
    assert_eq!(c.wall.monitor, "GSM779465795");
    assert_eq!(c.wall.width, 7680);
    assert_eq!(c.wall.height, 1080);
    assert_eq!(c.pacing.guard_us, 3500);
    assert_eq!(c.pacing.ring, 5);
}

#[test]
fn config_overrides_and_rejects_unknown_keys() {
    let c = Config::from_toml("[api]\nport = 9000\n[sender]\nname = \"Test\"\n").unwrap();
    assert_eq!(c.api.port, 9000);
    assert_eq!(c.sender.name, "Test");
    assert!(Config::from_toml("[api]\nprot = 1\n").is_err());
}

fn temp_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("wb-config-{tag}-{}", std::process::id()))
}

#[test]
fn config_load_missing_file_gives_defaults() {
    let c = Config::load(&temp_path("missing")).unwrap();
    assert_eq!(c, Config::default());
}

#[test]
fn config_load_reads_the_file() {
    let p = temp_path("read");
    std::fs::write(&p, "[api]\nport = 9100\n").unwrap();
    let c = Config::load(&p);
    std::fs::remove_file(&p).unwrap();
    assert_eq!(c.unwrap().api.port, 9100);
}

#[test]
fn config_load_reports_bad_toml_and_unreadable_paths() {
    let p = temp_path("bad");
    std::fs::write(&p, "[api]\nport = \"x\"\n").unwrap();
    let bad = Config::load(&p);
    std::fs::remove_file(&p).unwrap();
    assert!(bad.unwrap_err().contains("wb-config-bad"));
    // A directory is not a missing file: it must not silently become defaults.
    let d = temp_path("dir");
    std::fs::create_dir_all(&d).unwrap();
    let dir = Config::load(&d);
    std::fs::remove_dir(&d).unwrap();
    assert!(dir.is_err());
}

#[test]
fn specialized_from_target_list() {
    use wb_core::status::specialized_from_targets;
    let t = vec![
        " usage=0 connected=false".to_string(),
        "CEN93200_1E_07E4_EC usage=0 connected=true".to_string(),
        "GSM779465795_0F_07DD_BC usage=2 connected=true".to_string(),
    ];
    assert!(specialized_from_targets(&t, "GSM779465795"));
    assert!(!specialized_from_targets(&t, "CEN93200"));
    assert!(!specialized_from_targets(&t, "NOPE"));
    let on_desktop = vec!["GSM779465795_0F_07DD_BC usage=1 connected=true".to_string()];
    assert!(!specialized_from_targets(&on_desktop, "GSM779465795"));
    let disconnected = vec!["GSM779465795_0F_07DD_BC usage=2 connected=false".to_string()];
    assert!(!specialized_from_targets(&disconnected, "GSM779465795"));
}
