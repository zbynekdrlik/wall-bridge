use wb_core::spout_lock::{LockGate, STUCK_MS};

#[test]
fn a_fresh_gate_waits_for_the_mutex() {
    let g = LockGate::default();
    assert!(!g.bypass(0));
    assert_eq!(g.wait_ms(2, 0), 2);
}

#[test]
fn short_timeouts_do_not_bypass() {
    let mut g = LockGate::default();
    g.on_timeout(1000);
    assert!(!g.bypass(1000 + STUCK_MS - 1));
    g.on_locked();
    g.on_timeout(5000);
    assert!(!g.bypass(5000 + STUCK_MS - 1), "the stuck time restarts after a lock");
}

#[test]
fn a_mutex_stuck_for_stuck_ms_is_bypassed() {
    // 13.11. 04:30Z: an abandoned SpoutAccessMutex stayed owned by Arena; every wait timed
    // out and the wall stayed black. After STUCK_MS of timeouts only, read without it.
    let mut g = LockGate::default();
    g.on_timeout(1000);
    g.on_timeout(1500);
    assert!(g.bypass(1000 + STUCK_MS));
    // While bypassing, only try the mutex without waiting.
    assert_eq!(g.wait_ms(2, 1000 + STUCK_MS), 0);
}

#[test]
fn getting_the_mutex_again_ends_the_bypass() {
    let mut g = LockGate::default();
    g.on_timeout(0);
    assert!(g.bypass(STUCK_MS));
    g.on_locked();
    assert!(!g.bypass(STUCK_MS + 10_000));
    assert_eq!(g.wait_ms(2, STUCK_MS + 10_000), 2);
}

#[test]
fn stuck_ms_is_one_second() {
    assert_eq!(STUCK_MS, 1000);
}
