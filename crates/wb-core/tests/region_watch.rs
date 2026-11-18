use wb_core::config::Config;
use wb_core::region_watch::{clamp_region, RegionWatch, RepeatRun, MAX_RUNS};

#[test]
fn clamp_keeps_inside_and_cuts_overhang() {
    assert_eq!(
        clamp_region([1008, 593, 5090, 617], 7680, 1080),
        Some([1008, 593, 5090, 617])
    );
    assert_eq!(
        clamp_region([7000, 1000, 9000, 2000], 7680, 1080),
        Some([7000, 1000, 7680, 1080])
    );
}

#[test]
fn clamp_refuses_empty_or_outside() {
    assert_eq!(clamp_region([10, 10, 10, 20], 100, 100), None);
    assert_eq!(clamp_region([10, 30, 20, 20], 100, 100), None);
    assert_eq!(clamp_region([100, 0, 200, 10], 100, 100), None);
}

#[test]
fn first_picture_is_not_a_repeat() {
    let mut w = RegionWatch::new();
    w.observe(1000, true);
    let win = w.take_window();
    assert_eq!((win.pictures, win.repeats), (1, 0));
    assert!(win.runs.is_empty());
}

#[test]
fn runs_are_timed_from_their_first_repeat() {
    let mut w = RegionWatch::new();
    w.observe(1000, false);
    w.observe(1033, true);
    w.observe(1066, true);
    w.observe(1100, false);
    w.observe(1133, true);
    w.observe(1166, false);
    let win = w.take_window();
    assert_eq!((win.pictures, win.repeats), (6, 3));
    assert_eq!(
        win.runs,
        vec![
            RepeatRun {
                start_ms: 1033,
                pictures: 2
            },
            RepeatRun {
                start_ms: 1133,
                pictures: 1
            }
        ]
    );
}

#[test]
fn open_run_carries_into_the_next_window() {
    let mut w = RegionWatch::new();
    w.observe(1, false);
    w.observe(2, true);
    let first = w.take_window();
    assert!(first.runs.is_empty());
    assert_eq!(first.repeats, 1);
    w.observe(3, true);
    w.observe(4, false);
    let second = w.take_window();
    assert_eq!(
        second.runs,
        vec![RepeatRun {
            start_ms: 2,
            pictures: 2
        }]
    );
    assert_eq!((second.pictures, second.repeats), (2, 1));
}

#[test]
fn reset_closes_the_run_and_skips_the_next_compare() {
    let mut w = RegionWatch::new();
    w.observe(1, false);
    w.observe(2, true);
    w.reset();
    w.observe(3, true);
    let win = w.take_window();
    assert_eq!(
        win.runs,
        vec![RepeatRun {
            start_ms: 2,
            pictures: 1
        }]
    );
    assert_eq!((win.pictures, win.repeats), (3, 1));
}

#[test]
fn runs_past_the_cap_are_counted() {
    let mut w = RegionWatch::new();
    w.observe(0, false);
    for i in 0..(MAX_RUNS as u64 + 3) {
        w.observe(i * 2 + 1, true);
        w.observe(i * 2 + 2, false);
    }
    let win = w.take_window();
    assert_eq!(win.runs.len(), MAX_RUNS);
    assert_eq!(win.runs_dropped, 3);
    assert_eq!(win.runs[0].start_ms, 1);
}

#[test]
fn diag_region_is_off_by_default_and_reads_from_toml() {
    assert_eq!(Config::default().diag.region, None);
    let c = Config::from_toml("[diag]\nregion = [1008, 593, 5090, 617]\n").unwrap();
    assert_eq!(c.diag.region, Some([1008, 593, 5090, 617]));
    assert!(Config::from_toml("[diag]\nbogus = 1\n").is_err());
}
