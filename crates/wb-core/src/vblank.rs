//! Vblank timing helpers for the engine's 10 s window.

/// Refreshes skipped between two consecutive vblank wakes `dt_us` apart.
/// A wake up to 1.5 periods late is the next refresh; each further period is one miss.
pub fn missed_between(dt_us: u64, period_us: u64) -> u64 {
    if period_us == 0 {
        return 0;
    }
    ((dt_us + period_us / 2) / period_us).saturating_sub(1)
}

/// p50 / p90 of arrival offsets (µs after the vblank), in ms; sorts `v`.
pub fn percentiles_ms(v: &mut [u64]) -> (f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    v.sort_unstable();
    let at = |q: f64| v[((v.len() - 1) as f64 * q) as usize] as f64 / 1000.0;
    (at(0.5), at(0.9))
}
