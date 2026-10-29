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
