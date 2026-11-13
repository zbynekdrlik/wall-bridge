//! Ring of slow presents with their own timestamps, read on `/api/status` (#22).
//!
//! The window line's `show_us_max` is one maximum per 10 s, so a reader cannot place an
//! 8–14 ms present against an event 1.6 s away (songplayer#317). The ring keeps each one.

use crate::timefmt::hms_utc;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

/// A present this slow (µs) or slower goes into the ring.
pub const SLOW_SHOW_RING_US: u64 = 8_000;
/// Entries kept; the newest overwrites the oldest.
pub const SLOW_SHOW_RING_LEN: usize = 256;

/// One slow present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlowShow {
    /// Unix ms at the start of the present.
    pub at_ms: u64,
    /// How long the present took, µs.
    pub us: u64,
    /// Present number since the display target was acquired.
    pub present: u64,
}

impl Serialize for SlowShow {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("SlowShow", 4)?;
        st.serialize_field("at_ms", &self.at_ms)?;
        st.serialize_field("at_utc", &hms_utc(self.at_ms))?;
        st.serialize_field("us", &self.us)?;
        st.serialize_field("present", &self.present)?;
        st.end()
    }
}

/// Fixed-capacity ring. Allocates only in [`SlowShowRing::new`]: `record` runs on the
/// vblank path.
pub struct SlowShowRing {
    buf: Vec<SlowShow>,
    cap: usize,
    /// Next write position once the ring is full.
    next: usize,
    total: u64,
}

impl SlowShowRing {
    pub fn new(cap: usize) -> SlowShowRing {
        assert!(cap > 0, "the ring needs room for one entry");
        SlowShowRing {
            buf: Vec::with_capacity(cap),
            cap,
            next: 0,
            total: 0,
        }
    }

    /// Records the present when it took `SLOW_SHOW_RING_US` or more. Returns whether it did.
    pub fn record(&mut self, at_ms: u64, us: u64, present: u64) -> bool {
        if us < SLOW_SHOW_RING_US {
            return false;
        }
        let e = SlowShow { at_ms, us, present };
        if self.buf.len() < self.cap {
            self.buf.push(e);
        } else {
            self.buf[self.next] = e;
            self.next = (self.next + 1) % self.cap;
        }
        self.total += 1;
        true
    }

    /// Slow presents recorded since the ring was made, including overwritten ones.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// The kept entries, oldest first.
    pub fn snapshot(&self) -> Vec<SlowShow> {
        let mut v = Vec::with_capacity(self.buf.len());
        v.extend_from_slice(&self.buf[self.next..]);
        v.extend_from_slice(&self.buf[..self.next]);
        v
    }
}
