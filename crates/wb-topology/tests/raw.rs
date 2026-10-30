use wb_core::topology::Grid;
use wb_topology::{RawGrid, GRID_SIZE, GRID_VER, PINNED};

#[test]
fn grid_layout_matches_nvapi() {
    assert_eq!(GRID_SIZE, 1832);
    assert_eq!(GRID_VER, 1832 | (2 << 16));
}

#[test]
fn pinned_grid_is_the_snv_wall() {
    let g = RawGrid::from_bytes(PINNED).unwrap().to_grid();
    assert_eq!((g.rows, g.cols), (1, 3));
    assert_eq!(g.ids, vec![0x8006_1086, 0x8006_1087, 0x8006_1083]);
    assert_eq!((g.width, g.height, g.freq), (2560, 1080, 60));
}

#[test]
fn from_grid_keeps_template_entries_and_clears_the_rest() {
    let pinned = RawGrid::from_bytes(PINNED).unwrap();
    let single = Grid {
        rows: 1,
        cols: 1,
        ids: vec![0x8006_1087],
        width: 2560,
        height: 1080,
        freq: 60,
    };
    let r = RawGrid::from_grid(&single, &pinned);
    assert_eq!(r.version, GRID_VER);
    assert_eq!(r.display_count, 1);
    assert_eq!(r.displays[0].display_id, 0x8006_1087);
    assert_eq!(r.displays[0].version, pinned.displays[1].version);
    assert_eq!(r.displays[1].display_id, 0);
    assert_eq!(r.to_grid(), single);
    assert_eq!(r.setting.bpp, pinned.setting.bpp);
}

#[test]
fn short_blob_is_rejected() {
    assert!(RawGrid::from_bytes(&PINNED[..100]).is_none());
}

#[test]
fn separated_display_drops_the_surround_flags() {
    // Live on SNV 30.10.: 1x1 grids carrying the 1x3 template's flags (0x12) made
    // SetDisplayGrids return -5. The reference dissolve script sets 1x1 grids with flags 0.
    let pinned = RawGrid::from_bytes(PINNED).unwrap();
    assert_eq!(pinned.flags, 0x12);
    let single = Grid {
        rows: 1,
        cols: 1,
        ids: vec![0x8006_1086],
        width: 2560,
        height: 1080,
        freq: 60,
    };
    assert_eq!(RawGrid::from_grid(&single, &pinned).flags, 0);
    // The re-formed wall keeps them, as the reference re-form sets the pinned grid as is.
    let wall = pinned.to_grid();
    assert_eq!(RawGrid::from_grid(&wall, &pinned).flags, 0x12);
}
