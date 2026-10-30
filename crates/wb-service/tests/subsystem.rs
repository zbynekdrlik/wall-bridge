//! `wall-bridge.exe` runs at logon of the operator's account: a console-subsystem exe opens a
//! console window on his desktop (seen on SNV 30.10.2026). It must be a GUI-subsystem exe.
#![cfg(windows)]

/// IMAGE_OPTIONAL_HEADER.Subsystem of a PE file.
fn subsystem(path: &str) -> u16 {
    let b = std::fs::read(path).expect("read exe");
    let pe = u32::from_le_bytes(b[0x3c..0x40].try_into().unwrap()) as usize;
    assert_eq!(&b[pe..pe + 4], b"PE\0\0", "PE signature");
    // Signature (4) + COFF header (20), then Subsystem at offset 68 of the optional header.
    let at = pe + 24 + 68;
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}

#[test]
fn engine_opens_no_console_window() {
    const IMAGE_SUBSYSTEM_WINDOWS_GUI: u16 = 2;
    assert_eq!(
        subsystem(env!("CARGO_BIN_EXE_wall-bridge")),
        IMAGE_SUBSYSTEM_WINDOWS_GUI
    );
}
