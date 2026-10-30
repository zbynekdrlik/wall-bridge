//! Keep the tray icon out of the hidden overflow.
//!
//! Windows 11 keeps one `HKCU\Control Panel\NotifyIconSettings\<key>` entry per notify icon,
//! keyed by the exe path, and starts a new one hidden. The tray lives under
//! `app\<version>\`, so every upgrade is a new, hidden icon. Explorer shows an entry with
//! `IsPromoted = 1` in the taskbar at once (measured on SNV 30.10.2026).

/// One `NotifyIconSettings` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconEntry {
    pub key: String,
    /// `ExecutablePath`.
    pub path: String,
    /// `IsPromoted`, absent until the user moves the icon.
    pub promoted: Option<u32>,
}

fn norm(p: &str) -> String {
    p.replace('/', "\\").to_lowercase()
}

/// Keys of this exe's entries that are not promoted yet.
pub fn to_promote(entries: &[IconEntry], exe: &str) -> Vec<String> {
    let exe = norm(exe);
    entries
        .iter()
        .filter(|e| norm(&e.path) == exe && e.promoted != Some(1))
        .map(|e| e.key.clone())
        .collect()
}
