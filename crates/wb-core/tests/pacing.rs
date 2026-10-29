use wb_core::pacing::{Pacer, Show};

const WARMUP_VB: u64 = 120;

/// Drives a Pacer with sources arriving every `interval_vb` vblanks, first at phase `phase`.
/// `jitter(i)` shifts arrival i (in vblank periods). The first 120 vblanks are warm-up (the
/// phase mean settles); returns the window of the vblanks after that.
fn drive(
    interval_vb: f64,
    phase: f64,
    vblanks: u64,
    jitter: impl Fn(u64) -> f64,
) -> wb_core::pacing::PacingWindow {
    let mut p = Pacer::new(5);
    let mut next_arrival = phase;
    let mut i = 0u64;
    for vb in 0..vblanks + WARMUP_VB {
        if vb == WARMUP_VB {
            p.window();
        }
        p.on_vblank();
        // Arrivals inside this vblank interval [vb, vb + 1).
        loop {
            let t = next_arrival + jitter(i);
            if t >= (vb + 1) as f64 {
                break;
            }
            p.arrive((t - vb as f64).max(0.0));
            i += 1;
            next_arrival += interval_vb;
        }
    }
    p.window()
}

#[test]
fn steady_30fps_alternates_2_vblanks() {
    let w = drive(2.0, 0.3, 600, |_| 0.0);
    let total: u64 = w.runs.iter().sum();
    assert!(w.runs[1] * 100 >= total * 95, "runs {:?}", w.runs);
    assert_eq!(w.drops, 0);
    assert!(
        (w.interval_vb - 2.0).abs() < 0.01,
        "interval {}",
        w.interval_vb
    );
}

#[test]
fn jitter_near_boundary_keeps_cadence() {
    // Arrivals sit right on the vblank boundary and wobble across it.
    let w = drive(2.0, 0.99, 600, |i| if i % 2 == 0 { 0.0 } else { 0.04 });
    assert_eq!(w.runs[0], 0, "1-vblank runs from jitter: {:?}", w.runs);
    assert_eq!(w.runs[2], 0, "3-vblank runs from jitter: {:?}", w.runs);
    assert_eq!(w.drops, 0);
}

#[test]
fn steady_25fps_gives_2_3_cadence() {
    // 25 fps on 59.978 Hz: 2.399 vblanks per picture, a 2/3 pattern. Arrivals walk through
    // every phase, so the phase mean wanders and a few pictures land off-pattern (the MVP does
    // the same); at most 2 % of runs may be 1 or 4+ vblanks.
    let w = drive(59.978 / 25.0, 0.5, 1200, |i| {
        ((i * 7919) % 11) as f64 * 0.01 - 0.05
    });
    let total: u64 = w.runs.iter().sum();
    let off_pattern = total - w.runs[1] - w.runs[2];
    assert!(off_pattern * 50 <= total, "runs {:?}", w.runs);
    assert!(
        w.runs[1] > w.runs[2],
        "2-runs should dominate: {:?}",
        w.runs
    );
    assert_eq!(w.drops, 0);
}

#[test]
fn picture_shows_two_vblanks_after_its_interval() {
    let mut p = Pacer::new(5);
    assert_eq!(p.on_vblank(), Show::Repeat(None)); // vb 1
    let slot = p.arrive(0.4); // belongs to vb 1 -> shown at vb 3
    assert_eq!(p.on_vblank(), Show::Repeat(None)); // vb 2
    assert_eq!(p.on_vblank(), Show::New(slot)); // vb 3
    assert_eq!(p.on_vblank(), Show::Repeat(Some(slot))); // vb 4
    assert_eq!(p.vb_index(), 4);
}

#[test]
fn ring_full_evicts_oldest_and_counts_drop() {
    let mut p = Pacer::new(5);
    p.on_vblank();
    let first = p.arrive(0.1);
    let slots: Vec<usize> = (0..4).map(|_| p.arrive(0.2)).collect();
    assert!(!slots.contains(&first));
    let sixth = p.arrive(0.3);
    assert_eq!(sixth, first, "the oldest queued slot is reused");
    assert_eq!(p.window().drops, 1);
}

#[test]
fn two_due_pictures_show_newest_and_count_drop() {
    let mut p = Pacer::new(5);
    p.on_vblank(); // vb 1
    p.arrive(0.1);
    let b = p.arrive(0.6);
    p.on_vblank(); // vb 2
    assert_eq!(p.on_vblank(), Show::New(b)); // vb 3: both due, newest wins
    let w = p.window();
    assert_eq!(w.drops, 1);
    assert_eq!(w.new, 1);
    assert_eq!(w.repeats, 2);
}

#[test]
fn window_resets_counters() {
    let mut p = Pacer::new(5);
    for _ in 0..10 {
        p.on_vblank();
    }
    let w = p.window();
    assert_eq!(w.repeats, 10);
    let w2 = p.window();
    assert_eq!(w2.repeats, 0);
    assert_eq!(w2.new, 0);
}

#[test]
fn window_counts_vblanks() {
    let mut p = Pacer::new(5);
    for _ in 0..7 {
        p.on_vblank();
    }
    assert_eq!(p.window().vblanks, 7);
}

#[test]
fn phase_mean_is_the_circular_ema() {
    let mut p = Pacer::new(5);
    p.on_vblank();
    p.arrive(0.25);
    // c = 0.97 * 1 + 0.03 * cos(pi/2), s = 0.03 * sin(pi/2)
    let want = 0.03f64.atan2(0.97) / std::f64::consts::TAU;
    let got = p.window().phase_mean;
    assert!((got - want).abs() < 1e-12, "{got} vs {want}");
}

#[test]
fn phase_mean_converges_to_arrival_phase() {
    let mut p = Pacer::new(5);
    for _ in 0..400 {
        p.on_vblank();
        p.arrive(0.3);
    }
    let got = p.window().phase_mean;
    assert!((got - 0.3).abs() < 1e-4, "{got}");
}

/// Settles the phase mean at `phase`, one picture per vblank.
fn settled(phase: f64) -> Pacer {
    let mut p = Pacer::new(5);
    for _ in 0..400 {
        p.on_vblank();
        p.arrive(phase);
    }
    p
}

#[test]
fn early_arrival_belongs_to_previous_interval() {
    // Mean at 0.95: an arrival 0.02 after the vblank is late for the previous interval.
    let mut p = settled(0.95);
    p.on_vblank();
    let x = p.arrive(0.02);
    assert_eq!(p.on_vblank(), Show::New(x));
}

#[test]
fn late_arrival_belongs_to_next_interval() {
    // Mean at 0.05: an arrival 0.97 after the vblank is early for the next interval.
    let mut p = settled(0.05);
    p.on_vblank();
    let x = p.arrive(0.97);
    assert!(matches!(p.on_vblank(), Show::New(s) if s != x));
    assert!(matches!(p.on_vblank(), Show::Repeat(_)));
    assert_eq!(p.on_vblank(), Show::New(x));
}

#[test]
fn arrival_near_mean_stays_in_its_interval() {
    let mut p = settled(0.5);
    p.on_vblank();
    let x = p.arrive(0.9);
    assert!(matches!(p.on_vblank(), Show::New(s) if s != x));
    assert_eq!(p.on_vblank(), Show::New(x));
}

fn interval_after(gap_vb: u64, off_a: f64, off_b: f64) -> f64 {
    let mut p = Pacer::new(5);
    p.on_vblank();
    p.arrive(off_a);
    for _ in 0..gap_vb {
        p.on_vblank();
    }
    p.arrive(off_b);
    p.window().interval_vb
}

#[test]
fn interval_is_an_ema_of_arrival_gaps() {
    assert!((interval_after(3, 0.0, 0.0) - (0.95 * 2.0 + 0.05 * 3.0)).abs() < 1e-12);
    assert!((interval_after(5, 0.0, 0.5) - (0.95 * 2.0 + 0.05 * 5.5)).abs() < 1e-12);
    assert!((interval_after(0, 0.0, 0.75) - (0.95 * 2.0 + 0.05 * 0.75)).abs() < 1e-12);
}

#[test]
fn interval_ignores_gaps_out_of_range() {
    assert_eq!(interval_after(0, 0.25, 0.75), 2.0); // exactly 0.5
    assert_eq!(interval_after(6, 0.0, 0.0), 2.0); // exactly 6.0
    assert_eq!(interval_after(0, 0.1, 0.2), 2.0); // 0.1
}

#[test]
fn arrival_exactly_on_the_mean_belongs_to_its_own_interval() {
    // Fresh pacer: mean 0, an arrival at phase 0 is in the interval of the vblank just passed.
    let mut p = Pacer::new(5);
    p.on_vblank(); // vb 1
    let x = p.arrive(0.0);
    assert_eq!(p.on_vblank(), Show::Repeat(None)); // vb 2
    assert_eq!(p.on_vblank(), Show::New(x)); // vb 3
}

#[test]
fn arrival_half_a_period_from_the_mean_goes_forward() {
    // Mean 0, arrival at 0.5: the distance wraps to -0.5, so it belongs to the next interval.
    let mut p = Pacer::new(5);
    p.on_vblank(); // vb 1
    p.on_vblank(); // vb 2
    let x = p.arrive(0.5);
    for _ in 0..2 {
        assert_eq!(p.on_vblank(), Show::Repeat(None)); // vb 3, 4
    }
    assert_eq!(p.on_vblank(), Show::New(x)); // vb 5
}
