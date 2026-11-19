//! Diagnostic: does one region of Arena's output repeat its content across Arena's new
//! pictures (#24)? The whole-canvas probe cannot tell, because other layers change on every
//! Arena frame. Off unless `[diag] region` is set in config.toml.

/// A region `[x0, y0, x1, y1]` (x1/y1 exclusive) clamped to a `w`×`h` picture. `None` when
/// nothing of it is left.
pub fn clamp_region(r: [u32; 4], w: u32, h: u32) -> Option<[u32; 4]> {
    let [x0, y0, x1, y1] = r;
    let (x1, y1) = (x1.min(w), y1.min(h));
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    Some([x0, y0, x1, y1])
}

/// Most repeat runs kept per reporting window; more are only counted.
pub const MAX_RUNS: usize = 64;

/// One run of new pictures whose region equalled the picture before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepeatRun {
    /// Unix ms of the first repeated picture's arrival.
    pub start_ms: u64,
    /// Repeated pictures in a row.
    pub pictures: u32,
}

/// What one reporting window saw.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RegionWindow {
    /// New pictures whose region was read.
    pub pictures: u64,
    /// Of those, the ones equal to the picture before.
    pub repeats: u64,
    /// Finished runs, oldest first.
    pub runs: Vec<RepeatRun>,
    /// Runs past MAX_RUNS.
    pub runs_dropped: u64,
}

/// Tracks runs of repeated region content. No allocation after `new`: `observe` runs on the
/// vblank path.
pub struct RegionWatch {
    seen_any: bool,
    open: Option<RepeatRun>,
    win: RegionWindow,
}

impl Default for RegionWatch {
    fn default() -> Self {
        Self::new()
    }
}

impl RegionWatch {
    pub fn new() -> RegionWatch {
        RegionWatch {
            seen_any: false,
            open: None,
            win: RegionWindow {
                runs: Vec::with_capacity(MAX_RUNS),
                ..RegionWindow::default()
            },
        }
    }

    /// One new picture's region, read at `at_ms`; `same` = equal to the previous one read.
    /// The first picture has nothing to compare with and counts only as a picture.
    pub fn observe(&mut self, at_ms: u64, same: bool) {
        self.win.pictures += 1;
        if !self.seen_any {
            self.seen_any = true;
            return;
        }
        if same {
            self.win.repeats += 1;
            match &mut self.open {
                Some(run) => run.pictures += 1,
                None => {
                    self.open = Some(RepeatRun {
                        start_ms: at_ms,
                        pictures: 1,
                    })
                }
            }
        } else if let Some(run) = self.open.take() {
            self.close(run);
        }
    }

    /// The comparison chain broke (a skipped picture, a sender re-open): the next picture is
    /// not compared, and an open run ends.
    pub fn reset(&mut self) {
        if let Some(run) = self.open.take() {
            self.close(run);
        }
        self.seen_any = false;
    }

    fn close(&mut self, run: RepeatRun) {
        if self.win.runs.len() < MAX_RUNS {
            self.win.runs.push(run);
        } else {
            self.win.runs_dropped += 1;
        }
    }

    /// The window so far, then a fresh one. A run still open stays open into the next window.
    pub fn take_window(&mut self) -> RegionWindow {
        let out = self.win.clone();
        self.win.pictures = 0;
        self.win.repeats = 0;
        self.win.runs.clear();
        self.win.runs_dropped = 0;
        out
    }
}

/// Monotonic milliseconds on the engine's clock. A type of its own so it cannot be swapped with
/// a Unix ms timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MonoMs(u64);

impl MonoMs {
    /// Milliseconds since `start` on the monotonic clock: what the engine passes.
    pub fn since(start: std::time::Instant) -> MonoMs {
        MonoMs(start.elapsed().as_millis() as u64)
    }

    /// A given monotonic reading, for tests and replays of logged timings.
    pub fn from_ms(ms: u64) -> MonoMs {
        MonoMs(ms)
    }
}

/// A gap this long (ms) or longer between two new Arena pictures is logged (#24): at 30 fps a
/// picture is due every 33 ms, so 45 ms means one came at least a refresh late.
pub const LATE_GAP_MS: u64 = 45;
/// Most gaps kept per reporting window; more are only counted.
pub const MAX_GAPS: usize = 32;

/// Gaps of LATE_GAP_MS or more between new pictures: (Unix ms of the late picture, gap ms).
/// No allocation after `new`.
pub struct GapWatch {
    /// Monotonic ms of the previous picture.
    prev_ms: Option<MonoMs>,
    gaps: Vec<(u64, u64)>,
    dropped: u64,
}

impl Default for GapWatch {
    fn default() -> Self {
        Self::new()
    }
}

impl GapWatch {
    pub fn new() -> GapWatch {
        GapWatch {
            prev_ms: None,
            gaps: Vec::with_capacity(MAX_GAPS),
            dropped: 0,
        }
    }

    /// A new picture arrived at monotonic `mono_ms`, Unix `at_ms`.
    pub fn picture(&mut self, mono_ms: MonoMs, at_ms: u64) {
        if let Some(prev) = self.prev_ms {
            let gap = mono_ms.0.saturating_sub(prev.0);
            if gap >= LATE_GAP_MS {
                if self.gaps.len() < MAX_GAPS {
                    self.gaps.push((at_ms, gap));
                } else {
                    self.dropped += 1;
                }
            }
        }
        self.prev_ms = Some(mono_ms);
    }

    /// The sender went away or re-opened: the next picture starts a new chain.
    pub fn reset(&mut self) {
        self.prev_ms = None;
    }

    /// Gaps of this window (oldest first) and how many did not fit; then a fresh window.
    pub fn take_window(&mut self) -> (Vec<(u64, u64)>, u64) {
        let out = (self.gaps.clone(), self.dropped);
        self.gaps.clear();
        self.dropped = 0;
        out
    }
}

/// Staging copies the region probe can have in flight: under GPU load one copy can take more
/// than the 2 vblanks between Arena's pictures (0.2.21 skipped up to 50 per window).
pub const STAGES: usize = 4;

/// Order bookkeeping of the region probe's staging copies (#24). Copies are queued in picture
/// order and read back oldest first. A break in the comparison chain (a picture not copied, a
/// sender re-open) travels with the next queued copy, so it applies when that copy is read,
/// not to a copy queued before the break.
pub struct StageRing {
    /// Arrival (Unix ms) of the picture in each stage, and whether the chain broke before it.
    pending: [Option<(u64, bool)>; STAGES],
    head: usize,
    in_flight: usize,
    break_next: bool,
}

impl Default for StageRing {
    fn default() -> Self {
        Self::new()
    }
}

impl StageRing {
    pub fn new() -> StageRing {
        StageRing {
            pending: [None; STAGES],
            head: 0,
            in_flight: 0,
            break_next: false,
        }
    }

    /// The chain broke: the next queued picture must not be compared with the one before it.
    pub fn mark_break(&mut self) {
        self.break_next = true;
    }

    /// Stage to copy a new picture (arrived at `at_ms`) into; `None` when every stage is in
    /// flight, which also breaks the chain.
    pub fn queue(&mut self, at_ms: u64) -> Option<usize> {
        if self.in_flight == STAGES {
            self.break_next = true;
            return None;
        }
        let k = (self.head + self.in_flight) % STAGES;
        self.pending[k] = Some((at_ms, std::mem::take(&mut self.break_next)));
        self.in_flight += 1;
        Some(k)
    }

    /// The oldest copy in flight: (stage, arrival ms, chain broke before it).
    pub fn oldest(&self) -> Option<(usize, u64, bool)> {
        if self.in_flight == 0 {
            return None;
        }
        self.pending[self.head].map(|(at, brk)| (self.head, at, brk))
    }

    /// Frees the oldest copy.
    pub fn release(&mut self) {
        if self.in_flight == 0 {
            return;
        }
        self.pending[self.head] = None;
        self.head = (self.head + 1) % STAGES;
        self.in_flight -= 1;
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight
    }
}

/// The sender re-opened or went away: late gaps start a new chain, and the region probe's next
/// queued copy is not compared with the one before it.
pub fn break_chain(gaps: &mut GapWatch, ring: Option<&mut StageRing>) {
    gaps.reset();
    if let Some(r) = ring {
        r.mark_break();
    }
}

/// Map failures in a row (other than "still drawing") after which the region probe turns
/// itself off.
pub const MAP_FAIL_LIMIT: u32 = 32;

/// What the probe does after a map failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailAction {
    /// First failure of a streak: log it.
    Log,
    /// Further failure: count only.
    Quiet,
    /// MAP_FAIL_LIMIT in a row: log once and turn the probe off.
    Stop,
}

/// Map failures in a row of the region probe.
#[derive(Debug, Default)]
pub struct FailStreak {
    n: u32,
}

impl FailStreak {
    pub fn new() -> FailStreak {
        FailStreak { n: 0 }
    }

    /// One more failure, and what to do about it.
    pub fn fail(&mut self) -> FailAction {
        self.n = self.n.saturating_add(1);
        if self.n == MAP_FAIL_LIMIT {
            FailAction::Stop
        } else if self.n == 1 {
            FailAction::Log
        } else {
            FailAction::Quiet
        }
    }

    /// A read succeeded: the streak ends.
    pub fn ok(&mut self) {
        self.n = 0;
    }

    /// MAP_FAIL_LIMIT failures in a row: the probe should stop.
    pub fn dead(&self) -> bool {
        self.n >= MAP_FAIL_LIMIT
    }
}
