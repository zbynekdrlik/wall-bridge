//! Overall health from the parts of the status.

use crate::pacing::PacingWindow;
use crate::sender::SenderState;
use crate::status::{DisplayState, TopologyState};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    DisplayNotAcquired,
    /// The engine holds its target, but Windows shows the desktop on the wall.
    DisplayOnDesktop,
    TopologyGaveUp,
    TopologyMismatch,
    SenderLost,
    MissedVblanks(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Ok,
    /// The wall still shows something, but not everything is right.
    Degraded(Vec<Reason>),
    /// The wall does not show the bridge picture.
    Failed(Vec<Reason>),
}

impl State {
    pub fn label(&self) -> &'static str {
        match self {
            State::Ok => "ok",
            State::Degraded(_) => "degraded",
            State::Failed(_) => "failed",
        }
    }

    pub fn reasons(&self) -> &[Reason] {
        match self {
            State::Ok => &[],
            State::Degraded(r) | State::Failed(r) => r,
        }
    }
}

pub struct Inputs<'a> {
    pub display: &'a DisplayState,
    pub topology: &'a TopologyState,
    pub sender: &'a SenderState,
    pub pacing: &'a PacingWindow,
}

pub fn evaluate(i: &Inputs) -> State {
    let mut failed = vec![];
    if !i.display.target_acquired {
        failed.push(Reason::DisplayNotAcquired);
    } else if !i.display.specialized {
        failed.push(Reason::DisplayOnDesktop);
    }
    if i.topology.gave_up {
        failed.push(Reason::TopologyGaveUp);
    }
    if !failed.is_empty() {
        return State::Failed(failed);
    }
    let mut degraded = vec![];
    if !i.topology.matches {
        degraded.push(Reason::TopologyMismatch);
    }
    if !i.sender.connected {
        degraded.push(Reason::SenderLost);
    }
    if i.pacing.missed_vblanks > 0 {
        degraded.push(Reason::MissedVblanks(i.pacing.missed_vblanks));
    }
    if degraded.is_empty() {
        State::Ok
    } else {
        State::Degraded(degraded)
    }
}
