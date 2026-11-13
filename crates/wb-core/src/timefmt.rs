//! Time formatting for log lines.

/// `HH:MM:SS.mmm` (UTC) of a Unix time in ms, for hold timestamps in the window line.
pub fn hms_utc(unix_ms: u64) -> String {
    let ms = unix_ms % 86_400_000;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}
