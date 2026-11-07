//! Scheduling priorities. SNV runs SongPlayer and obs64 in the High class; the engine must not
//! depend on how it was started (Task Scheduler's default gave BelowNormal, 6.11.2026).
//! - process: High;
//! - vblank thread: TIME_CRITICAL (15, the top of the non-realtime range, above every
//!   ordinary High thread, not realtime so MMCSS audio threads still win);
//! - API and keeper threads: BELOW_NORMAL, so the High class does not lift them over SongPlayer.

use windows::core::Result;
use windows::Win32::System::Threading::*;

pub fn raise_process() -> Result<()> {
    unsafe { SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS) }
}

pub fn vblank_thread() -> Result<()> {
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) }
}

/// Failure only costs priority, so it is logged and the thread goes on.
pub fn helper_thread() {
    if let Err(e) = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) } {
        tracing::warn!(err = ?e, "could not lower helper thread priority");
    }
}

pub fn class_name(class: u32) -> &'static str {
    match PROCESS_CREATION_FLAGS(class) {
        IDLE_PRIORITY_CLASS => "Idle",
        BELOW_NORMAL_PRIORITY_CLASS => "BelowNormal",
        NORMAL_PRIORITY_CLASS => "Normal",
        ABOVE_NORMAL_PRIORITY_CLASS => "AboveNormal",
        HIGH_PRIORITY_CLASS => "High",
        REALTIME_PRIORITY_CLASS => "Realtime",
        _ => "unknown",
    }
}

/// The effective class of this process and priority of the calling thread, for the log.
pub fn effective() -> (&'static str, i32) {
    unsafe {
        (
            class_name(GetPriorityClass(GetCurrentProcess())),
            GetThreadPriority(GetCurrentThread()),
        )
    }
}
