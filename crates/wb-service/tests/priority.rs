//! The engine shares SNV with High-class processes (SongPlayer, obs64 through Process Lasso).
//! Started by Task Scheduler at its default priority it ran BelowNormal, where the vblank
//! thread's HIGHEST is base 8 and every ordinary High thread (13) preempts it (SNV 6.11.2026).
#![cfg(windows)]

use wb_service::priority;
use windows::Win32::System::Threading::*;

fn current_thread() -> i32 {
    unsafe { GetThreadPriority(GetCurrentThread()) }
}

#[test]
fn process_runs_high() {
    priority::raise_process().expect("raise process");
    assert_eq!(
        unsafe { GetPriorityClass(GetCurrentProcess()) },
        HIGH_PRIORITY_CLASS.0
    );
}

#[test]
fn vblank_thread_is_time_critical() {
    std::thread::spawn(|| {
        priority::vblank_thread().expect("vblank thread");
        assert_eq!(current_thread(), THREAD_PRIORITY_TIME_CRITICAL.0);
    })
    .join()
    .unwrap();
}

#[test]
fn helper_threads_are_below_normal() {
    std::thread::spawn(|| {
        priority::helper_thread();
        assert_eq!(current_thread(), THREAD_PRIORITY_BELOW_NORMAL.0);
    })
    .join()
    .unwrap();
}

#[test]
fn class_names_for_the_log() {
    assert_eq!(priority::class_name(HIGH_PRIORITY_CLASS.0), "High");
    assert_eq!(
        priority::class_name(BELOW_NORMAL_PRIORITY_CLASS.0),
        "BelowNormal"
    );
    assert_eq!(priority::class_name(NORMAL_PRIORITY_CLASS.0), "Normal");
    assert_eq!(priority::class_name(0x1234), "unknown");
}

// The wiring main.rs uses: a helper thread, the API runtime's threads and the log writer's
// thread all run BELOW_NORMAL, so the High class lifts none of them over SongPlayer.

#[test]
fn spawned_helper_runs_below_normal() {
    let level = priority::spawn_helper("helper", current_thread)
        .expect("spawn")
        .join()
        .unwrap();
    assert_eq!(level, THREAD_PRIORITY_BELOW_NORMAL.0);
}

#[test]
fn runtime_threads_run_below_normal() {
    let rt = priority::helper_runtime(2).expect("runtime");
    let (worker, blocking) = rt.block_on(async {
        let w = tokio::spawn(async { current_thread() }).await.unwrap();
        let b = tokio::task::spawn_blocking(current_thread).await.unwrap();
        (w, b)
    });
    assert_eq!(worker, THREAD_PRIORITY_BELOW_NORMAL.0);
    assert_eq!(blocking, THREAD_PRIORITY_BELOW_NORMAL.0);
}

#[test]
fn log_writer_lowers_its_thread() {
    use std::io::Write;
    std::thread::spawn(|| {
        let mut w = priority::Lowered::new(Vec::new());
        w.write_all(b"line\n").unwrap();
        assert_eq!(current_thread(), THREAD_PRIORITY_BELOW_NORMAL.0);
        assert_eq!(w.into_inner(), b"line\n");
    })
    .join()
    .unwrap();
}

#[test]
fn effective_names_the_calling_thread() {
    let (_, _, tid) = priority::effective();
    assert_eq!(tid, unsafe { GetCurrentThreadId() });
}
