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
