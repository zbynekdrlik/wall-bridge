use wb_core::slow_show::{SlowShowRing, SLOW_SHOW_RING_US};

#[test]
fn records_only_at_or_above_threshold() {
    let mut r = SlowShowRing::new(4);
    assert!(!r.record(1000, SLOW_SHOW_RING_US - 1, 1));
    assert!(r.record(2000, SLOW_SHOW_RING_US, 2));
    assert!(r.record(3000, 14_000, 3));
    assert_eq!(r.total(), 2);
    let s = r.snapshot();
    assert_eq!(s.len(), 2);
    assert_eq!((s[0].at_ms, s[0].us, s[0].present), (2000, 8_000, 2));
    assert_eq!((s[1].at_ms, s[1].us, s[1].present), (3000, 14_000, 3));
}

#[test]
fn wraps_oldest_first_and_counts_overwritten() {
    let mut r = SlowShowRing::new(3);
    for i in 1..=5u64 {
        r.record(i * 100, 9_000, i);
    }
    assert_eq!(r.total(), 5);
    let presents: Vec<u64> = r.snapshot().iter().map(|e| e.present).collect();
    assert_eq!(presents, vec![3, 4, 5]);
    r.record(600, 9_000, 6);
    let presents: Vec<u64> = r.snapshot().iter().map(|e| e.present).collect();
    assert_eq!(presents, vec![4, 5, 6]);
}

#[test]
fn empty_ring_snapshot_is_empty() {
    let r = SlowShowRing::new(2);
    assert!(r.snapshot().is_empty());
    assert_eq!(r.total(), 0);
}

#[test]
fn entry_json_carries_utc_text() {
    let mut r = SlowShowRing::new(2);
    // 13.11.2026 15:12:46.123 UTC
    let at = 1_794_582_766_123u64;
    r.record(at, 12_345, 77);
    let v = serde_json::to_value(r.snapshot()).unwrap();
    assert_eq!(v[0]["at_ms"], at);
    assert_eq!(v[0]["at_utc"], "15:12:46.123");
    assert_eq!(v[0]["us"], 12_345);
    assert_eq!(v[0]["present"], 77);
}

#[test]
#[should_panic(expected = "room for one entry")]
fn zero_capacity_is_refused() {
    SlowShowRing::new(0);
}
