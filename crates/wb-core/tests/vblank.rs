use wb_core::vblank::{missed_between, percentiles_ms};

const P: u64 = 16_673; // 59.978 Hz

#[test]
fn on_time_vblank_misses_nothing() {
    assert_eq!(missed_between(P, P), 0);
    assert_eq!(missed_between(P + P / 3, P), 0); // late wake, same refresh
}

#[test]
fn gaps_count_skipped_refreshes() {
    assert_eq!(missed_between(2 * P, P), 1);
    assert_eq!(missed_between(3 * P + 100, P), 2);
    assert_eq!(missed_between(P * 16 / 10, P), 1); // 1.6 periods: one refresh skipped
    assert_eq!(missed_between(P * 14 / 10, P), 0); // 1.4 periods: a late wake
}

#[test]
fn zero_period_is_safe() {
    assert_eq!(missed_between(1000, 0), 0);
}

#[test]
fn percentiles_of_arrivals() {
    let mut v: Vec<u64> = (1..=10).map(|k| k * 1000).collect(); // 1..10 ms
    assert_eq!(percentiles_ms(&mut v), (5.0, 9.0));
    assert_eq!(percentiles_ms(&mut []), (0.0, 0.0));
    assert_eq!(percentiles_ms(&mut [2500]), (2.5, 2.5));
}
