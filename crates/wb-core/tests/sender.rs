use wb_core::sender::{SenderAction, SenderSeen, SenderWatch, STALL_REOPEN_MAX_MS};

fn seen(handle: u32) -> Option<SenderSeen> {
    Some(SenderSeen {
        handle,
        width: 7680,
        height: 1080,
        listed: true,
    })
}

#[test]
fn first_sight_opens() {
    let mut w = SenderWatch::new("Arena - Screen 2");
    assert_eq!(
        w.tick(0, seen(0x4000_2c42), false),
        SenderAction::Open(0x4000_2c42)
    );
    assert!(w.state(0).connected);
}

#[test]
fn reopen_on_handle_change() {
    let mut w = SenderWatch::new("Arena - Screen 2");
    w.tick(0, seen(0x4000_2c42), true);
    assert_eq!(w.tick(500, seen(0x4000_2c42), true), SenderAction::Keep);
    assert_eq!(
        w.tick(1000, seen(0x4000_3000), false),
        SenderAction::Reopen(0x4000_3000)
    );
}

#[test]
fn hold_while_missing() {
    let mut w = SenderWatch::new("Arena - Screen 2");
    w.tick(0, seen(0x4000_2c42), true);
    assert_eq!(w.tick(500, None, false), SenderAction::Hold);
    let s = w.state(500);
    assert!(!s.connected);
    assert_eq!(s.last_picture_age_ms, Some(500));
    // Arena is back with a new handle: open it, not keep the dead one.
    assert_eq!(
        w.tick(9000, seen(0x4000_5555), false),
        SenderAction::Open(0x4000_5555)
    );
}

#[test]
fn static_scene_is_not_loss() {
    let mut w = SenderWatch::new("Arena - Screen 2");
    w.tick(0, seen(0x4000_2c42), true);
    for t in (500..=600_000).step_by(500) {
        // Never a loss; a periodic re-open of the same handle is allowed (stall re-open).
        let a = w.tick(t, seen(0x4000_2c42), false);
        assert!(
            a == SenderAction::Keep || a == SenderAction::Reopen(0x4000_2c42),
            "{a:?} at {t}"
        );
    }
    let s = w.state(600_000);
    assert!(s.connected);
    assert_eq!(s.last_picture_age_ms, Some(600_000));
}

#[test]
fn unlisted_sender_is_missing() {
    // The 280 B mapping can outlive Arena for a moment; the sender list is the truth.
    let mut w = SenderWatch::new("Arena - Screen 2");
    w.tick(0, seen(0x4000_2c42), true);
    let gone = Some(SenderSeen {
        listed: false,
        ..seen(0x4000_2c42).unwrap()
    });
    assert_eq!(w.tick(500, gone, false), SenderAction::Hold);
    assert!(!w.state(500).connected);
}

#[test]
fn state_reports_size_and_name() {
    let mut w = SenderWatch::new("Arena - Screen 2");
    w.tick(
        0,
        Some(SenderSeen {
            handle: 1,
            width: 3840,
            height: 540,
            listed: true,
        }),
        true,
    );
    let s = w.state(0);
    assert_eq!(s.name, "Arena - Screen 2");
    assert_eq!((s.width, s.height), (3840, 540));
    assert_eq!(s.last_picture_age_ms, Some(0));
}

#[test]
fn no_picture_yet_has_no_age() {
    let mut w = SenderWatch::new("Arena - Screen 2");
    w.tick(0, None, false);
    assert_eq!(w.state(100).last_picture_age_ms, None);
}

#[test]
fn forget_after_failed_open_opens_again() {
    // C2: the engine could not open the handle (or a probe failed): the next check must
    // open the same handle again, not keep the closed one.
    let mut w = SenderWatch::new("Arena - Screen 2");
    assert_eq!(
        w.tick(0, seen(0x4000_2c42), false),
        SenderAction::Open(0x4000_2c42)
    );
    w.forget();
    assert!(!w.state(0).connected);
    assert_eq!(
        w.tick(500, seen(0x4000_2c42), false),
        SenderAction::Open(0x4000_2c42)
    );
    assert!(w.state(500).connected);
}

#[test]
fn stalled_sender_is_reopened_periodically() {
    // Spec: re-open when no new pictures come for a while although the sender is listed.
    // A static scene is harmless to re-open: the last picture stays on the wall.
    let mut w = SenderWatch::new("Arena - Screen 2");
    w.tick(0, seen(1), true);
    assert_eq!(w.tick(9_000, seen(1), false), SenderAction::Keep);
    assert_eq!(w.tick(10_000, seen(1), false), SenderAction::Reopen(1));
    assert_eq!(w.tick(20_000, seen(1), false), SenderAction::Keep);
    assert_eq!(w.tick(30_000, seen(1), false), SenderAction::Reopen(1));
    // A new picture resets the stall clock and the back-off.
    assert_eq!(w.tick(31_000, seen(1), true), SenderAction::Keep);
    assert_eq!(w.tick(40_000, seen(1), false), SenderAction::Keep);
    assert_eq!(w.tick(41_000, seen(1), false), SenderAction::Reopen(1));
    assert!(w.state(41_000).connected);
}

#[test]
fn picture_age_counts_from_the_picture_not_the_check() {
    // Seen on the SNV dashboard: "last new picture 496 ms" at 30 pictures / s, because the age
    // only moved at the 500 ms sender check.
    let mut w = SenderWatch::new("Arena - Screen 2");
    w.tick(0, seen(1), false);
    w.picture(480);
    assert_eq!(w.state(500).last_picture_age_ms, Some(20));
}

/// Times of the stall re-opens of a static scene, checked every 500 ms up to `until`.
fn stall_reopens(w: &mut SenderWatch, from: u64, until: u64) -> Vec<u64> {
    (from..=until)
        .step_by(500)
        .filter(|&t| w.tick(t, seen(1), false) == SenderAction::Reopen(1))
        .collect()
}

#[test]
fn static_scene_reopens_with_growing_gaps() {
    // Arena publishes no Spout frame counter (no `<name>_Count_Semaphore`, SNV 30.10.2026), so a
    // static scene cannot be told from a dead texture. The safety re-open backs off from 10 s
    // to at most 5 min instead of firing every 10 s.
    assert_eq!(STALL_REOPEN_MAX_MS, 300_000);
    let mut w = SenderWatch::new("Arena - Screen 2");
    w.tick(0, seen(1), true);
    assert_eq!(
        stall_reopens(&mut w, 500, 1_000_000),
        vec![10_000, 30_000, 70_000, 150_000, 310_000, 610_000, 910_000]
    );
}

#[test]
fn a_new_handle_resets_the_back_off() {
    let mut w = SenderWatch::new("Arena - Screen 2");
    w.tick(0, seen(1), true);
    assert_eq!(
        stall_reopens(&mut w, 500, 70_000),
        vec![10_000, 30_000, 70_000]
    );
    assert_eq!(w.tick(70_500, seen(2), false), SenderAction::Reopen(2));
    w.tick(71_000, seen(1), false);
    assert_eq!(stall_reopens(&mut w, 71_500, 100_500), vec![81_000]);
}
