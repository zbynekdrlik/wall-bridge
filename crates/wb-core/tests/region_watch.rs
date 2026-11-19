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

use wb_core::region_watch::{GapWatch, MonoMs, LATE_GAP_MS, MAX_GAPS};

#[test]
fn gaps_under_the_threshold_are_not_logged() {
    let mut g = GapWatch::new();
    g.picture(MonoMs::from_ms(1000), 1000);
    g.picture(MonoMs::from_ms(1033), 1033);
    g.picture(
        MonoMs::from_ms(1033 + LATE_GAP_MS - 1),
        1033 + LATE_GAP_MS - 1,
    );
    assert_eq!(g.take_window(), (vec![], 0));
}

#[test]
fn a_late_picture_is_logged_with_its_gap() {
    let mut g = GapWatch::new();
    g.picture(MonoMs::from_ms(1000), 1000);
    g.picture(MonoMs::from_ms(1050), 1050);
    g.picture(MonoMs::from_ms(1067), 1067);
    assert_eq!(g.take_window(), (vec![(1050, 50)], 0));
    assert_eq!(g.take_window(), (vec![], 0));
}

#[test]
fn reset_starts_a_new_chain() {
    let mut g = GapWatch::new();
    g.picture(MonoMs::from_ms(1000), 1000);
    g.reset();
    g.picture(MonoMs::from_ms(5000), 5000);
    assert_eq!(g.take_window(), (vec![], 0));
}

#[test]
fn gaps_past_the_cap_are_counted() {
    let mut g = GapWatch::new();
    for i in 0..(MAX_GAPS as u64 + 2) {
        g.picture(MonoMs::from_ms(i * 100), i * 100);
    }
    let (gaps, dropped) = g.take_window();
    assert_eq!(gaps.len(), MAX_GAPS);
    assert_eq!(dropped, 1);
    assert_eq!(gaps[0], (100, 100));
}

use wb_core::region_watch::{StageRing, STAGES};

#[test]
fn stages_are_read_in_queue_order() {
    let mut r = StageRing::new();
    assert_eq!(r.oldest(), None);
    assert_eq!(r.queue(10), Some(0));
    assert_eq!(r.queue(20), Some(1));
    assert_eq!(r.in_flight(), 2);
    assert_eq!(r.oldest(), Some((0, 10, false)));
    r.release();
    assert_eq!(r.oldest(), Some((1, 20, false)));
    assert_eq!(r.queue(30), Some(2));
    r.release();
    assert_eq!(r.oldest(), Some((2, 30, false)));
    r.release();
    assert_eq!((r.in_flight(), r.oldest()), (0, None));
}

#[test]
fn stage_index_wraps_around() {
    let mut r = StageRing::new();
    for i in 0..STAGES as u64 {
        r.queue(i);
        r.release();
    }
    assert_eq!(r.queue(99), Some(0));
    assert_eq!(r.oldest(), Some((0, 99, false)));
}

#[test]
fn full_ring_skips_and_breaks_before_the_next_copy() {
    let mut r = StageRing::new();
    for i in 0..STAGES as u64 {
        assert!(r.queue(i).is_some());
    }
    assert_eq!(r.queue(100), None);
    assert_eq!(r.in_flight(), STAGES);
    // Copies queued before the skip keep their chain.
    for i in 0..STAGES as u64 {
        assert_eq!(r.oldest(), Some((i as usize, i, false)));
        r.release();
    }
    // The first copy after the skip carries the break; the one after it does not.
    assert_eq!(r.queue(200), Some(0));
    assert_eq!(r.queue(233), Some(1));
    assert_eq!(r.oldest(), Some((0, 200, true)));
    r.release();
    assert_eq!(r.oldest(), Some((1, 233, false)));
}

#[test]
fn a_break_waits_for_the_next_queued_copy() {
    let mut r = StageRing::new();
    r.queue(1);
    r.mark_break();
    r.queue(2);
    assert_eq!(r.oldest(), Some((0, 1, false)));
    r.release();
    assert_eq!(r.oldest(), Some((1, 2, true)));
}

#[test]
fn late_gaps_use_the_monotonic_clock() {
    let mut g = GapWatch::new();
    // Wall clock jumps forward 10 s between two on-time pictures: no late gap.
    g.picture(MonoMs::from_ms(1000), 50_000);
    g.picture(MonoMs::from_ms(1033), 60_000);
    // A late picture is logged with its wall-clock time and the monotonic gap.
    g.picture(MonoMs::from_ms(1100), 60_067);
    assert_eq!(g.take_window(), (vec![(60_067, 67)], 0));
}

#[test]
fn release_on_an_empty_ring_is_a_no_op() {
    let mut r = StageRing::new();
    r.release();
    assert_eq!((r.in_flight(), r.oldest()), (0, None));
    assert_eq!(r.queue(5), Some(0));
    assert_eq!(r.oldest(), Some((0, 5, false)));
}

use wb_core::region_watch::{break_chain, FailAction, FailStreak, MAP_FAIL_LIMIT};

#[test]
fn break_chain_resets_gaps_and_marks_the_ring() {
    let mut g = GapWatch::new();
    let mut r = StageRing::new();
    g.picture(MonoMs::from_ms(1000), 1000);
    r.queue(1000);
    break_chain(&mut g, Some(&mut r));
    // No gap across the break.
    g.picture(MonoMs::from_ms(9000), 9000);
    assert_eq!(g.take_window(), (vec![], 0));
    // The copy queued after the break carries it.
    r.queue(9000);
    r.release();
    assert_eq!(r.oldest(), Some((1, 9000, true)));
}

#[test]
fn break_chain_without_a_probe_still_resets_gaps() {
    let mut g = GapWatch::new();
    g.picture(MonoMs::from_ms(1000), 1000);
    break_chain(&mut g, None);
    g.picture(MonoMs::from_ms(2000), 2000);
    assert_eq!(g.take_window(), (vec![], 0));
}

#[test]
fn fail_streak_logs_the_first_and_stops_at_the_limit() {
    let mut f = FailStreak::new();
    assert!(!f.dead());
    assert_eq!(f.fail(), FailAction::Log);
    for _ in 1..MAP_FAIL_LIMIT - 1 {
        assert_eq!(f.fail(), FailAction::Quiet);
    }
    assert!(
        !f.dead(),
        "{} failures must not stop it",
        MAP_FAIL_LIMIT - 1
    );
    assert_eq!(f.fail(), FailAction::Stop);
    assert!(f.dead());
    // Past the limit: no second Stop, the probe is already off.
    assert_eq!(f.fail(), FailAction::Quiet);
    assert!(f.dead());
}

#[test]
fn fail_streak_ends_on_a_good_read() {
    let mut f = FailStreak::new();
    for _ in 0..MAP_FAIL_LIMIT - 1 {
        f.fail();
    }
    f.ok();
    assert_eq!(
        f.fail(),
        FailAction::Log,
        "first failure after a good read is logged again"
    );
    assert!(!f.dead());
}

#[test]
fn a_gap_of_exactly_the_threshold_is_logged() {
    let mut g = GapWatch::new();
    g.picture(MonoMs::from_ms(1000), 1000);
    g.picture(MonoMs::from_ms(1000 + LATE_GAP_MS), 7000);
    assert_eq!(g.take_window(), (vec![(7000, LATE_GAP_MS)], 0));
}

#[test]
fn mono_since_reads_the_monotonic_clock() {
    let start = std::time::Instant::now() - std::time::Duration::from_millis(250);
    let m = MonoMs::since(start);
    assert!(
        m >= MonoMs::from_ms(250) && m < MonoMs::from_ms(60_000),
        "{m:?}"
    );
}
