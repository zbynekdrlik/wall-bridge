use wb_core::sender::{SenderAction, SenderSeen, SenderWatch};

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
        assert_eq!(w.tick(t, seen(0x4000_2c42), false), SenderAction::Keep);
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
