//! Scheduling priorities. SNV runs SongPlayer and obs64 in the High class; the engine must not
//! depend on how it was started (Task Scheduler's default gave BelowNormal, 6.11.2026).
//! - process: High;
//! - vblank thread: TIME_CRITICAL (15, the top of the non-realtime range, above every
//!   ordinary High thread, not realtime so MMCSS audio threads still win);
//! - API, keeper and log writer threads: BELOW_NORMAL, so the High class does not lift them
//!   over SongPlayer.

use std::cell::Cell;
use std::io::{self, Write};
use std::thread::JoinHandle;
use windows::core::Result;
use windows::Win32::System::Threading::*;

pub fn raise_process() -> Result<()> {
    unsafe { SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS) }
}

pub fn vblank_thread() -> Result<()> {
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) }
}

/// Failure only costs priority, so it is logged and the thread goes on. Each lowered thread
/// logs its id; `e2e/snv/verify.ps1` checks those threads on SNV.
pub fn helper_thread() {
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("unnamed");
    let tid = unsafe { GetCurrentThreadId() };
    match unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) } {
        Ok(()) => tracing::debug!(thread = name, tid, "thread lowered"),
        Err(e) => {
            tracing::warn!(thread = name, tid, err = ?e, "could not lower helper thread priority")
        }
    }
}

/// A named thread that lowers itself before it runs `f`.
pub fn spawn_helper<F, T>(name: &str, f: F) -> io::Result<JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            helper_thread();
            f()
        })
}

/// A multi-thread tokio runtime whose workers and blocking threads all run BELOW_NORMAL.
pub fn helper_runtime(workers: usize) -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .on_thread_start(helper_thread)
        .enable_all()
        .build()
}

/// A writer that lowers the thread it is written from, once. tracing-appender spawns its
/// worker thread itself, so this is the only hook into that thread.
pub struct Lowered<W> {
    inner: W,
}

thread_local! {
    static LOWERED: Cell<bool> = const { Cell::new(false) };
}

impl<W> Lowered<W> {
    pub fn new(inner: W) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for Lowered<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !LOWERED.get() {
            helper_thread();
            LOWERED.set(true);
        }
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
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

/// The effective class of this process, and the priority and id of the calling thread, for
/// the log (`e2e/snv/verify.ps1` finds the vblank thread by that id).
pub fn effective() -> (&'static str, i32, u32) {
    unsafe {
        (
            class_name(GetPriorityClass(GetCurrentProcess())),
            GetThreadPriority(GetCurrentThread()),
            GetCurrentThreadId(),
        )
    }
}
