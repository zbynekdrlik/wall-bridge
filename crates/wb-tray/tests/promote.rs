use wb_tray::promote::{to_promote, IconEntry};

fn e(key: &str, path: &str, promoted: Option<u32>) -> IconEntry {
    IconEntry {
        key: key.into(),
        path: path.into(),
        promoted,
    }
}

const EXE: &str = r"C:\ProgramData\WallBridge\app\0.2.4\wall-bridge-tray.exe";

#[test]
fn a_new_version_hidden_by_windows_is_promoted() {
    // What SNV showed on 30.10.: one entry per installed version, IsPromoted unset.
    let entries = vec![
        e(
            "5731",
            r"C:\ProgramData\WallBridge\app\0.2.3\wall-bridge-tray.exe",
            Some(1),
        ),
        e("1755", EXE, None),
        e("9000", r"C:\Program Files\Other\other.exe", None),
    ];
    assert_eq!(to_promote(&entries, EXE), vec!["1755".to_string()]);
}

#[test]
fn hidden_by_hand_is_promoted_again() {
    let entries = vec![e("1755", EXE, Some(0))];
    assert_eq!(to_promote(&entries, EXE), vec!["1755".to_string()]);
}

#[test]
fn already_visible_is_left_alone() {
    let entries = vec![e("1755", EXE, Some(1))];
    assert!(to_promote(&entries, EXE).is_empty());
}

#[test]
fn path_match_ignores_case_and_slashes() {
    let entries = vec![e(
        "1755",
        "c:/programdata/wallbridge/APP/0.2.4/Wall-Bridge-Tray.exe",
        None,
    )];
    assert_eq!(to_promote(&entries, EXE), vec!["1755".to_string()]);
}

#[test]
fn no_entry_yet_promotes_nothing() {
    assert!(to_promote(&[], EXE).is_empty());
}
