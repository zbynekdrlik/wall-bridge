//! Vblank pacing, extracted from the MVP engine loop.
//!
//! Each new picture is shown at a fixed delay (2 vblanks) after the vblank interval it was
//! written in. The interval is decided against the arrival phase's running circular mean, so a
//! picture written near a vblank is never split between two intervals (no 1/3 runs).

use serde::Serialize;

/// Vblanks between a picture's interval and the vblank it is shown at.
const SHOW_DELAY_VB: u64 = 2;
/// Weight of a new arrival in the circular mean of the arrival phase.
const PHASE_ALPHA: f64 = 0.03;
/// Weight of a new inter-arrival interval in its running mean.
const INTERVAL_ALPHA: f64 = 0.05;
/// Arrival histogram size; runs of 9 or more vblanks share the last bucket.
const RUN_BUCKETS: usize = 9;

/// What the engine puts on the wall at a vblank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Show {
    /// A new picture from this ring slot.
    New(usize),
    /// Keep the picture already on the wall (`None` before the first one).
    Repeat(Option<usize>),
}

/// Counters for one reporting window (the MVP's 10 s `window` line).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PacingWindow {
    pub vblanks: u64,
    pub new: u64,
    pub repeats: u64,
    pub drops: u64,
    /// `runs[k]` = pictures that stayed on the wall for k + 1 vblanks.
    pub runs: [u64; RUN_BUCKETS],
    /// Running mean of the inter-arrival interval, in vblanks.
    pub interval_vb: f64,
    /// Circular mean of the arrival phase in [0, 1).
    pub phase_mean: f64,
    /// Pictures waiting to be shown.
    pub queue: usize,
}

pub struct Pacer {
    ring_len: usize,
    vb_index: u64,
    /// (ring slot, vblank to show it at), oldest first.
    queue: std::collections::VecDeque<(usize, u64)>,
    shown: Option<usize>,
    phase_c: f64,
    phase_s: f64,
    interval_vb: f64,
    last_arrival_vb: Option<f64>,
    run: u64,
    win: PacingWindow,
}

impl Pacer {
    pub fn new(ring_len: usize) -> Pacer {
        assert!(ring_len >= 2, "the ring needs a shown slot and a free slot");
        Pacer {
            ring_len,
            vb_index: 0,
            queue: Default::default(),
            shown: None,
            phase_c: 1.0,
            phase_s: 0.0,
            interval_vb: 2.0,
            last_arrival_vb: None,
            run: 0,
            win: PacingWindow::default(),
        }
    }

    /// Index of the last vblank passed to [`Pacer::on_vblank`].
    pub fn vb_index(&self) -> u64 {
        self.vb_index
    }

    /// Call right after each vblank. Returns what to put on the wall for the next refresh.
    pub fn on_vblank(&mut self) -> Show {
        self.vb_index += 1;
        self.win.vblanks += 1;
        let mut take = None;
        while let Some(&(slot, at)) = self.queue.front() {
            if at > self.vb_index {
                break;
            }
            if take.is_some() {
                self.win.drops += 1;
            }
            take = Some(slot);
            self.queue.pop_front();
        }
        let show = match take {
            Some(slot) => {
                self.shown = Some(slot);
                self.win.new += 1;
                self.close_run();
                Show::New(slot)
            }
            None => {
                self.win.repeats += 1;
                Show::Repeat(self.shown)
            }
        };
        self.run += 1;
        show
    }

    /// A new picture arrived `off_vb` vblank periods after the last vblank.
    /// Returns the ring slot the engine must copy it into; it is queued for display.
    pub fn arrive(&mut self, off_vb: f64) -> usize {
        let slot = match (0..self.ring_len)
            .find(|i| Some(*i) != self.shown && !self.queue.iter().any(|q| q.0 == *i))
        {
            Some(i) => i,
            None => {
                self.win.drops += 1;
                self.queue
                    .pop_front()
                    .expect("a full ring has queued slots")
                    .0
            }
        };

        let frac = off_vb.fract();
        let ang = frac * std::f64::consts::TAU;
        self.phase_c = (1.0 - PHASE_ALPHA) * self.phase_c + PHASE_ALPHA * ang.cos();
        self.phase_s = (1.0 - PHASE_ALPHA) * self.phase_s + PHASE_ALPHA * ang.sin();
        let mean = self.phase_mean();
        // Distance of this arrival from the mean, wrapped to [-0.5, 0.5).
        let d = ((frac - mean + 1.5).rem_euclid(1.0)) - 0.5;
        // The interval the mean arrival falls in; this arrival belongs to it when close.
        let m = frac - d;
        let belongs = if m < 0.0 {
            self.vb_index.saturating_sub(1)
        } else if m >= 1.0 {
            self.vb_index + 1
        } else {
            self.vb_index
        };
        self.queue.push_back((slot, belongs + SHOW_DELAY_VB));

        let now_vb = self.vb_index as f64 + off_vb;
        if let Some(prev) = self.last_arrival_vb {
            let iv = now_vb - prev;
            if iv > 0.5 && iv < 6.0 {
                self.interval_vb = (1.0 - INTERVAL_ALPHA) * self.interval_vb + INTERVAL_ALPHA * iv;
            }
        }
        self.last_arrival_vb = Some(now_vb);
        slot
    }

    /// Returns the counters since the last call and starts a new window.
    pub fn window(&mut self) -> PacingWindow {
        let mut w = std::mem::take(&mut self.win);
        w.interval_vb = self.interval_vb;
        w.phase_mean = self.phase_mean();
        w.queue = self.queue.len();
        w
    }

    fn phase_mean(&self) -> f64 {
        self.phase_s
            .atan2(self.phase_c)
            .rem_euclid(std::f64::consts::TAU)
            / std::f64::consts::TAU
    }

    fn close_run(&mut self) {
        if self.run > 0 {
            let i = (self.run.min(RUN_BUCKETS as u64) - 1) as usize;
            self.win.runs[i] += 1;
        }
        self.run = 0;
    }
}
