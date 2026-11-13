//! When to stop waiting for Arena's `SpoutAccessMutex` (#20).
//!
//! If a reader dies while it holds the mutex, the mutex is abandoned. The next waiter gets it
//! with `WAIT_ABANDONED`. When that waiter is Arena and it never releases the mutex, every
//! other reader's wait times out forever. This happened on 13.11. 04:30Z, and the wall stayed
//! black. After `STUCK_MS` of nothing but timeouts, the reader copies without the mutex. A
//! rare torn picture is better than a black wall. Meanwhile it keeps trying the mutex
//! without waiting, and goes back to locking once it gets it.

/// Timeouts only, for this long: read without the mutex.
pub const STUCK_MS: u64 = 1000;

#[derive(Debug, Default, Clone)]
pub struct LockGate {
    /// Start of the current run of timeouts.
    stuck_since: Option<u64>,
}

impl LockGate {
    /// The mutex was obtained.
    pub fn on_locked(&mut self) {
        self.stuck_since = None;
    }

    /// The wait for the mutex timed out at `now_ms`.
    pub fn on_timeout(&mut self, now_ms: u64) {
        self.stuck_since.get_or_insert(now_ms);
    }

    /// Read without the mutex now?
    pub fn bypass(&self, now_ms: u64) -> bool {
        self.stuck_since
            .is_some_and(|s| now_ms.saturating_sub(s) >= STUCK_MS)
    }

    /// How long to wait for the mutex: `max_ms` normally, 0 while bypassing.
    pub fn wait_ms(&self, max_ms: u32, now_ms: u64) -> u32 {
        if self.bypass(now_ms) {
            0
        } else {
            max_ms
        }
    }
}
