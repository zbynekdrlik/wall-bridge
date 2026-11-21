//! The status JSON is serialized at most once per period for all dashboard sockets, so the
//! number of open dashboards does not add reads of the engine's status lock (#31 review).

use wb_core::text_cache::TextCache;

#[test]
fn serves_the_cached_text_within_the_period() {
    let mut c = TextCache::new(1000);
    let mut calls = 0;
    let mut fill = |s: &str| {
        calls += 1;
        s.to_string()
    };
    assert_eq!(&*c.get(0, || fill("a")), "a");
    assert_eq!(&*c.get(999, || fill("b")), "a");
    assert_eq!(calls, 1);
}

#[test]
fn refreshes_after_the_period() {
    let mut c = TextCache::new(1000);
    assert_eq!(&*c.get(0, || "a".to_string()), "a");
    assert_eq!(&*c.get(1000, || "b".to_string()), "b");
    assert_eq!(&*c.get(1500, || "c".to_string()), "b");
    assert_eq!(&*c.get(2000, || "d".to_string()), "d");
}

#[test]
fn a_clock_going_back_refreshes() {
    let mut c = TextCache::new(1000);
    c.get(5000, || "a".to_string());
    assert_eq!(&*c.get(100, || "b".to_string()), "b");
}
