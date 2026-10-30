//! Surround keeper decisions: does the NVIDIA Mosaic layout match the wall definition, and
//! when must it be restored. The NVAPI calls live in `wb-topology`; this module decides.
//!
//! Restoring goes through 3 separate displays first: re-forming the 1x3 grid works only from
//! that state, never from a collapsed span (SNV history, #1).

/// One NVAPI Mosaic display grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    pub rows: u32,
    pub cols: u32,
    /// NVAPI display ids, row-major.
    pub ids: Vec<u32>,
    pub width: u32,
    pub height: u32,
    pub freq: u32,
}

/// The wall definition: a 1 x N grid of these displays, in this order, at this per-display mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallDef {
    pub ids: Vec<u32>,
    pub width: u32,
    pub height: u32,
    pub freq: u32,
    /// Displays that stay their own 1x1 grid (the grabber); kept as they are on every set.
    pub keep_single: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Set each wall display as its own 1x1 grid.
    Separate,
    /// Set the 1 x N wall grid.
    FormGrid,
    /// Read back and compare with the definition.
    VerifyGrid,
    /// Make sure the wall is off the desktop again.
    Specialize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeeperAction {
    None,
    Restore(Vec<Step>),
    /// Too many failed restores: stop and show it.
    GiveUp,
}

/// Failed restores counted inside this window decide giving up.
pub const FAIL_WINDOW_MS: u64 = 30 * 60_000;
pub const MAX_FAILS: usize = 3;
/// Mismatching reads in a row needed before acting.
pub const CONFIRM_READS: u32 = 3;

#[derive(Debug, Clone, Default)]
pub struct KeeperHistory {
    pub matches: bool,
    pub read_errors: u32,
    /// Last read showed no wall display at all and no live span: nothing safe to act on.
    pub unknown: bool,
    pub gave_up: bool,
    pub last_restore_ms: Option<u64>,
    mismatches: u32,
    /// Reads in a row with the grid intact but the wall back on the desktop.
    on_desktop: u32,
    failed_at: Vec<u64>,
}

impl KeeperHistory {
    /// Record the outcome of a restore started by [`decide`].
    pub fn restore_finished(&mut self, now_ms: u64, ok: bool) {
        self.last_restore_ms = Some(now_ms);
        if ok {
            self.failed_at.clear();
            self.gave_up = false;
        } else {
            self.failed_at.push(now_ms);
        }
    }

    /// Failed restores inside the window ending at `now_ms`.
    pub fn failures(&self, now_ms: u64) -> usize {
        self.failed_at
            .iter()
            .filter(|&&t| now_ms.saturating_sub(t) < FAIL_WINDOW_MS)
            .count()
    }
}

impl WallDef {
    pub fn matches(&self, grids: &[Grid]) -> bool {
        grids.iter().any(|g| {
            g.rows == 1
                && g.cols as usize == self.ids.len()
                && g.ids == self.ids
                && (g.width, g.height, g.freq) == (self.width, self.height, self.freq)
        })
    }

    fn kept(&self, current: &[Grid]) -> Vec<Grid> {
        current
            .iter()
            .filter(|g| g.ids.len() == 1 && self.keep_single.contains(&g.ids[0]))
            .cloned()
            .collect()
    }

    fn single(&self, id: u32) -> Grid {
        Grid {
            rows: 1,
            cols: 1,
            ids: vec![id],
            width: self.width,
            height: self.height,
            freq: self.freq,
        }
    }

    /// Grids for [`Step::Separate`]: each wall display alone, plus the kept grids as they are.
    pub fn separate_grids(&self, current: &[Grid]) -> Vec<Grid> {
        let mut out: Vec<Grid> = self.ids.iter().map(|&id| self.single(id)).collect();
        out.extend(self.kept(current));
        out
    }

    /// Grids for [`Step::FormGrid`]: the wall grid, plus the kept grids as they are.
    pub fn formed_grids(&self, current: &[Grid]) -> Vec<Grid> {
        let mut out = vec![Grid {
            rows: 1,
            cols: self.ids.len() as u32,
            ids: self.ids.clone(),
            width: self.width,
            height: self.height,
            freq: self.freq,
        }];
        out.extend(self.kept(current));
        out
    }
}

/// One keeper check. A read error never triggers an action and breaks a confirmation run.
///
/// `span_live`: the engine scans out to the wall's specialized Display.Core target at the wall
/// size. NVAPI does not list the Mosaic grid of a span that is removed from the desktop
/// (measured on SNV 30.10.2026), so a live span counts as matching.
///
/// `specialized`: the wall's Display.Core target is marked special-purpose. A grid that NVAPI
/// lists intact while the span is neither live nor specialized is back on the desktop: only
/// [`Step::Specialize`] is needed.
pub fn decide(
    def: &WallDef,
    read: Result<Vec<Grid>, String>,
    span_live: bool,
    specialized: bool,
    h: &mut KeeperHistory,
    now_ms: u64,
) -> KeeperAction {
    let grids = match read {
        Ok(g) => g,
        Err(_) => {
            h.read_errors += 1;
            h.mismatches = 0;
            h.on_desktop = 0;
            return KeeperAction::None;
        }
    };
    h.read_errors = 0;
    h.matches = span_live || def.matches(&grids);
    let wall_visible = grids
        .iter()
        .any(|g| g.ids.iter().any(|id| !def.keep_single.contains(id)));
    h.unknown = !h.matches && !wall_visible;
    if h.unknown {
        // Re-forming blind could take displays from Arena: report, do not act.
        h.mismatches = 0;
        return KeeperAction::None;
    }
    if h.matches {
        h.mismatches = 0;
        h.gave_up = false;
        if span_live || specialized {
            h.on_desktop = 0;
            return KeeperAction::None;
        }
        h.on_desktop += 1;
        if h.on_desktop < CONFIRM_READS {
            return KeeperAction::None;
        }
        h.on_desktop = 0;
        return KeeperAction::Restore(vec![Step::Specialize]);
    }
    h.on_desktop = 0;
    h.mismatches += 1;
    if h.mismatches < CONFIRM_READS {
        return KeeperAction::None;
    }
    h.mismatches = 0;
    if h.failures(now_ms) >= MAX_FAILS {
        h.gave_up = true;
        return KeeperAction::GiveUp;
    }
    h.gave_up = false;
    KeeperAction::Restore(vec![
        Step::Separate,
        Step::FormGrid,
        Step::VerifyGrid,
        Step::Specialize,
    ])
}
