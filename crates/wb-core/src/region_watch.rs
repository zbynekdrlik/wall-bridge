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
