use wb_core::topology::{decide, Grid, KeeperAction, KeeperHistory, Step, WallDef, CONFIRM_READS};

const A: u32 = 0x8006_1086;
const B: u32 = 0x8006_1087;
const C: u32 = 0x8006_1083;
const GRABBER: u32 = 0x8006_1081;
const MIN: u64 = 60_000;

fn def() -> WallDef {
    WallDef {
        ids: vec![A, B, C],
        width: 2560,
        height: 1080,
        freq: 60,
        keep_single: vec![GRABBER],
    }
}

fn grid(rows: u32, cols: u32, ids: &[u32]) -> Grid {
    Grid {
        rows,
        cols,
        ids: ids.to_vec(),
        width: 2560,
        height: 1080,
        freq: 60,
    }
}

fn wall() -> Vec<Grid> {
    vec![grid(1, 3, &[A, B, C]), grid(1, 1, &[GRABBER])]
}

fn collapsed() -> Vec<Grid> {
    let mut g = grid(1, 1, &[0x8006_1088]);
    g.width = 3840;
    g.height = 2160;
    g.freq = 30;
    vec![g, grid(1, 1, &[GRABBER])]
}

const RESTORE: [Step; 4] = [
    Step::Separate,
    Step::FormGrid,
    Step::VerifyGrid,
    Step::Specialize,
];

/// Feeds the same read until the keeper may act (CONFIRM_READS reads, 10 s apart); the reads
/// before the last must not act. Returns the last action and the time after it.
fn confirmed(read: &[Grid], h: &mut KeeperHistory, t0: u64) -> (KeeperAction, u64) {
    let mut t = t0;
    for _ in 1..CONFIRM_READS {
        assert_eq!(
            decide(&def(), Ok(read.to_vec()), false, h, t),
            KeeperAction::None
        );
        t += 10_000;
    }
    (decide(&def(), Ok(read.to_vec()), false, h, t), t + 10_000)
}

#[test]
fn matching_topology_no_action() {
    let mut h = KeeperHistory::default();
    for t in 0..10 {
        assert_eq!(
            decide(&def(), Ok(wall()), false, &mut h, t * 10_000),
            KeeperAction::None
        );
    }
    assert!(h.matches);
}

#[test]
fn collapsed_span_restores_via_separate() {
    let mut h = KeeperHistory::default();
    // Mismatching reads may be a driver mid-switch: act only on the third one in a row.
    assert_eq!(CONFIRM_READS, 3);
    assert_eq!(
        decide(&def(), Ok(collapsed()), false, &mut h, 0),
        KeeperAction::None
    );
    assert!(!h.matches);
    assert_eq!(
        decide(&def(), Ok(collapsed()), false, &mut h, 10_000),
        KeeperAction::None
    );
    assert_eq!(
        decide(&def(), Ok(collapsed()), false, &mut h, 20_000),
        KeeperAction::Restore(RESTORE.to_vec())
    );
}

#[test]
fn a_matching_read_breaks_the_confirmation() {
    let mut h = KeeperHistory::default();
    decide(&def(), Ok(collapsed()), false, &mut h, 0);
    decide(&def(), Ok(collapsed()), false, &mut h, 10_000);
    decide(&def(), Ok(wall()), false, &mut h, 20_000);
    assert_eq!(
        decide(&def(), Ok(collapsed()), false, &mut h, 30_000),
        KeeperAction::None
    );
}

#[test]
fn wrong_order_is_drift() {
    let mut h = KeeperHistory::default();
    let swapped = vec![grid(1, 3, &[B, A, C]), grid(1, 1, &[GRABBER])];
    assert_eq!(
        confirmed(&swapped, &mut h, 0).0,
        KeeperAction::Restore(RESTORE.to_vec())
    );
}

#[test]
fn wrong_mode_is_drift() {
    let mut h = KeeperHistory::default();
    let mut g = wall();
    g[0].freq = 30;
    assert!(matches!(
        confirmed(&g, &mut h, 0).0,
        KeeperAction::Restore(_)
    ));
}

#[test]
fn failed_read_is_not_drift() {
    let mut h = KeeperHistory::default();
    decide(&def(), Ok(wall()), false, &mut h, 0);
    for t in 1..=3 {
        assert_eq!(
            decide(&def(), Err("NVAPI_ERROR".into()), false, &mut h, t * 10_000),
            KeeperAction::None
        );
    }
    assert_eq!(h.read_errors, 3);
    // A read error between mismatches breaks the confirmation run.
    decide(&def(), Ok(collapsed()), false, &mut h, 40_000);
    decide(&def(), Ok(collapsed()), false, &mut h, 50_000);
    decide(&def(), Err("x".into()), false, &mut h, 60_000);
    let (a, _) = confirmed(&collapsed(), &mut h, 70_000);
    assert!(matches!(a, KeeperAction::Restore(_)));
    assert_eq!(h.read_errors, 0, "a good read resets the error count");
}

#[test]
fn gives_up_after_3_failed_restores_in_30_min() {
    let mut h = KeeperHistory::default();
    let mut t = 0;
    for _ in 0..3 {
        let (a, next) = confirmed(&collapsed(), &mut h, t);
        assert!(matches!(a, KeeperAction::Restore(_)));
        h.restore_finished(next, false);
        t = next + 2 * MIN;
    }
    let (a, next) = confirmed(&collapsed(), &mut h, t);
    assert_eq!(a, KeeperAction::GiveUp);
    assert!(h.gave_up);
    // Every action needs a fresh confirmation; still given up a bit later.
    assert_eq!(
        confirmed(&collapsed(), &mut h, next + 5 * MIN).0,
        KeeperAction::GiveUp
    );
    // Tries again once the 30 min window has passed.
    let (a, _) = confirmed(&collapsed(), &mut h, t + 31 * MIN);
    assert!(matches!(a, KeeperAction::Restore(_)));
    assert!(!h.gave_up);
}

#[test]
fn successful_restore_clears_failures() {
    let mut h = KeeperHistory::default();
    let (_, t) = confirmed(&collapsed(), &mut h, 0);
    h.restore_finished(t, false);
    let (_, t) = confirmed(&collapsed(), &mut h, t);
    h.restore_finished(t, true);
    assert_eq!(h.failures(t), 0);
    assert_eq!(h.last_restore_ms, Some(t));
}

#[test]
fn grabber_grid_untouched() {
    let d = def();
    let sep = d.separate_grids(&collapsed());
    let ids: Vec<Vec<u32>> = sep.iter().map(|g| g.ids.clone()).collect();
    assert_eq!(ids, vec![vec![A], vec![B], vec![C], vec![GRABBER]]);
    assert!(sep[..3]
        .iter()
        .all(|g| g.rows == 1 && g.cols == 1 && g.width == 2560));
    let formed = d.formed_grids(&collapsed());
    assert_eq!(formed[0].ids, vec![A, B, C]);
    assert_eq!((formed[0].rows, formed[0].cols), (1, 3));
    assert_eq!(formed[1].ids, vec![GRABBER]);
    // The grabber keeps whatever mode it has.
    assert_eq!(formed[1], collapsed()[1]);
}

#[test]
fn absent_grabber_is_not_invented() {
    let d = def();
    let only_wall = vec![grid(1, 1, &[0x8006_1088])];
    assert_eq!(d.formed_grids(&only_wall).len(), 1);
    assert_eq!(d.separate_grids(&only_wall).len(), 3);
}

#[test]
fn config_topology_defaults_are_the_snv_wall() {
    let c = wb_core::config::Config::from_toml("").unwrap();
    assert!(c.topology.enabled);
    assert_eq!(c.topology.wall_def(), def());
    assert_eq!(c.topology.settings_name, "NV Surround");
}

#[test]
fn drift_warnings_name_the_difference() {
    use wb_core::config::{drift_warnings, TopologyCfg};
    let t = TopologyCfg::default();
    assert!(drift_warnings(&t, Some(26200), Some(58157)).is_empty());
    assert_eq!(
        drift_warnings(&t, Some(22631), Some(62101)),
        vec![
            "Windows build 22631 != 26200 (25H2)".to_string(),
            "NVIDIA driver 621.01 != 581.57".to_string()
        ]
    );
    assert_eq!(
        drift_warnings(&t, None, None),
        vec![
            "Windows build unknown".to_string(),
            "NVIDIA driver version unknown".to_string()
        ]
    );
    assert_eq!(
        drift_warnings(&t, Some(26200), Some(58105)),
        vec!["NVIDIA driver 581.05 != 581.57".to_string()]
    );
}

#[test]
fn live_specialized_span_matches_without_its_grid() {
    // Measured on SNV: with the wall removed from the desktop NVAPI lists only the grabber.
    let mut h = KeeperHistory::default();
    let only_grabber = vec![grid(1, 1, &[GRABBER])];
    for t in 0..5 {
        assert_eq!(
            decide(&def(), Ok(only_grabber.clone()), true, &mut h, t * 10_000),
            KeeperAction::None
        );
        assert!(h.matches);
    }
    // The same read without a live span shows no wall display at all: unknown, never acted on.
    for t in 5..10 {
        assert_eq!(
            decide(&def(), Ok(only_grabber.clone()), false, &mut h, t * 10_000),
            KeeperAction::None
        );
        assert!(!h.matches);
        assert!(h.unknown);
    }
    // A wall display showing up again (collapsed span) is acted on.
    assert!(matches!(
        confirmed(&collapsed(), &mut h, 100_000).0,
        KeeperAction::Restore(_)
    ));
    assert!(!h.unknown);
}
