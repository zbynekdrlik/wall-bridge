//! The `/api/status` body, also read by SongPlayer's wall report and the tray.

use crate::health::{Reason, State};
use crate::pacing::PacingWindow;
use crate::sender::SenderState;
use serde::{Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DisplayState {
    /// The wall is off the Windows desktop.
    pub specialized: bool,
    /// The engine holds the Display.Core target and scans out to it.
    pub target_acquired: bool,
    pub width: u32,
    pub height: u32,
    pub hz: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TopologyState {
    /// The Surround layout matches the wall definition.
    pub matches: bool,
    /// Epoch ms of the last restore, if any.
    pub last_restore_ms: Option<u64>,
    /// Failed restores in the current back-off period.
    pub failures: u32,
    /// The keeper stopped trying and needs attention.
    pub gave_up: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub version: String,
    pub state: State,
    pub uptime_s: u64,
    pub display: DisplayState,
    pub topology: TopologyState,
    pub sender: SenderState,
    pub pacing: PacingWindow,
    pub warnings: Vec<String>,
}

impl Serialize for Status {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            version: &'a str,
            state: &'static str,
            reasons: &'a [Reason],
            uptime_s: u64,
            display: &'a DisplayState,
            topology: &'a TopologyState,
            sender: &'a SenderState,
            pacing: &'a PacingWindow,
            warnings: &'a [String],
        }
        Wire {
            version: &self.version,
            state: self.state.label(),
            reasons: self.state.reasons(),
            uptime_s: self.uptime_s,
            display: &self.display,
            topology: &self.topology,
            sender: &self.sender,
            pacing: &self.pacing,
            warnings: &self.warnings,
        }
        .serialize(s)
    }
}

/// True when the wall's target (`monitor` substring of its StableMonitorId) is connected and
/// marked special-purpose (`usage=2`, "removed from desktop"). Lines come from
/// `wb_scanout::list_targets`: `"<id> usage=<n> connected=<bool>"`.
pub fn specialized_from_targets(targets: &[String], monitor: &str) -> bool {
    targets.iter().any(|t| {
        let mut parts = t.split_whitespace();
        let id = parts.next().unwrap_or("");
        id.contains(monitor) && t.contains(" usage=2 ") && t.ends_with(" connected=true")
    })
}

/// The wall's target is connected (on the desktop or not).
pub fn wall_target_present(targets: &[String], monitor: &str) -> bool {
    targets.iter().any(|t| {
        let id = t.split_whitespace().next().unwrap_or("");
        id.contains(monitor) && t.ends_with(" connected=true")
    })
}

/// A connected Surround target (`NVS…`) while the wall's own target is absent: the span was
/// formed from generic EDIDs (TVs off), it is not the wall.
pub fn foreign_span(targets: &[String], monitor: &str) -> Option<String> {
    if wall_target_present(targets, monitor) {
        return None;
    }
    targets
        .iter()
        .filter(|t| t.ends_with(" connected=true"))
        .filter_map(|t| t.split_whitespace().next())
        .find(|id| id.starts_with("NVS"))
        .map(str::to_string)
}
