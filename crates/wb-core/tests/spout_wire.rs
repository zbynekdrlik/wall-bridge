use wb_core::spout_wire::{parse_info, parse_names, sample_boxes, INFO_LEN, NAME_SLOT, SAMPLES};

fn info_bytes(handle: u32, w: u32, h: u32, format: u32) -> Vec<u8> {
    let mut b = vec![0u8; INFO_LEN];
    b[0..4].copy_from_slice(&handle.to_le_bytes());
    b[4..8].copy_from_slice(&w.to_le_bytes());
    b[8..12].copy_from_slice(&h.to_le_bytes());
    b[12..16].copy_from_slice(&format.to_le_bytes());
    b
}

#[test]
fn info_is_280_bytes() {
    assert_eq!(INFO_LEN, 280);
}

#[test]
fn parses_texture_info() {
    let i = parse_info(&info_bytes(0x4000_2c42, 7680, 1080, 87)).unwrap();
    assert_eq!(i.handle, 0x4000_2c42);
    assert_eq!((i.width, i.height, i.format), (7680, 1080, 87));
}

#[test]
fn short_or_empty_info_is_none() {
    assert!(parse_info(&info_bytes(1, 7680, 1080, 87)[..279]).is_none());
    // A zero handle or zero size is a sender that has not published a texture yet.
    assert!(parse_info(&info_bytes(0, 7680, 1080, 87)).is_none());
    assert!(parse_info(&info_bytes(1, 0, 1080, 87)).is_none());
    assert!(parse_info(&info_bytes(1, 7680, 0, 87)).is_none());
}

fn names_bytes(names: &[&str]) -> Vec<u8> {
    let mut b = vec![0u8; NAME_SLOT * 64];
    for (k, n) in names.iter().enumerate() {
        b[k * NAME_SLOT..k * NAME_SLOT + n.len()].copy_from_slice(n.as_bytes());
    }
    b
}

#[test]
fn parses_sender_names_until_empty_slot() {
    let b = names_bytes(&["Arena - Screen 2", "SP-program-MAX"]);
    assert_eq!(parse_names(&b), vec!["Arena - Screen 2", "SP-program-MAX"]);
}

#[test]
fn names_stop_at_buffer_end() {
    let b = names_bytes(&["A", "B"]);
    assert_eq!(parse_names(&b[..NAME_SLOT + 10]), vec!["A"]);
    assert!(parse_names(&[]).is_empty());
}

#[test]
fn full_width_name_has_no_terminator() {
    let mut b = vec![b'x'; NAME_SLOT];
    b.extend(vec![0u8; NAME_SLOT]);
    assert_eq!(parse_names(&b), vec!["x".repeat(NAME_SLOT)]);
}

#[test]
fn sample_boxes_spread_inside_the_frame() {
    let boxes = sample_boxes(7680, 1080);
    assert_eq!(boxes.len(), SAMPLES as usize);
    assert_eq!(boxes[0], (240, 135)); // MVP: x = (2k+1) w / 32, y = ((k % 4) * 2 + 1) h / 8
    assert_eq!(boxes[4], (2160, 135)); // the y pattern repeats every 4 boxes
    assert_eq!(boxes[5], (2640, 405));
    assert_eq!(boxes[15], (7440, 945));
    for &(x, y) in &boxes {
        assert!(x + 8 <= 7680 && y + 8 <= 1080);
    }
}

#[test]
fn sample_boxes_clamp_on_tiny_frames() {
    for &(x, y) in &sample_boxes(16, 8) {
        assert!(x <= 8 && y == 0, "({x},{y})");
    }
}
