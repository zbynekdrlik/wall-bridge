//! Spout sender watch: decides when to open, re-open or hold the sender.
//!
//! The MVP kept a dead shared handle after an Arena restart (frozen picture, `new=0`).
//! Here a changed handle re-opens, a missing sender holds the last picture, and a static
//! scene (alive, no new pictures) is never treated as a loss.

use serde::Serialize;

/// Re-open a listed sender after this long without a new picture (a dead texture behind a
/// live name). Harmless for a static scene: the last picture stays on the wall.
pub const STALL_REOPEN_MS: u64 = 10_000;
/// Longest gap between stall re-opens of the same handle.
pub const STALL_REOPEN_MAX_MS: u64 = 0;

/// What the Windows side read for the sender at one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SenderSeen {
    /// Shared texture handle from the 280 B `SharedTextureInfo` mapping.
    pub handle: u32,
    pub width: u32,
    pub height: u32,
    /// The name is in the `SpoutSenderNames` list.
    pub listed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenderAction {
    /// Nothing to do.
    Keep,
    /// No texture open yet: open this handle.
    Open(u32),
    /// The open handle is stale (Arena restarted): open this one instead.
    Reopen(u32),
    /// The sender is gone: keep the last picture on the wall and wait.
    Hold,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SenderState {
    pub name: String,
    pub connected: bool,
    pub width: u32,
    pub height: u32,
    /// Time since the last new picture; `None` before the first one.
    pub last_picture_age_ms: Option<u64>,
}

pub struct SenderWatch {
    name: String,
    open: Option<u32>,
    connected: bool,
    size: (u32, u32),
    last_picture_ms: Option<u64>,
    /// Start of the current stall clock (last new picture or last stall re-open).
    stall_since_ms: Option<u64>,
}

impl SenderWatch {
    pub fn new(name: &str) -> SenderWatch {
        SenderWatch {
            name: name.to_string(),
            open: None,
            connected: false,
            size: (0, 0),
            last_picture_ms: None,
            stall_since_ms: None,
        }
    }

    /// A new picture arrived (called by the engine for every one, between checks).
    pub fn picture(&mut self, now_ms: u64) {
        self.last_picture_ms = Some(now_ms);
        self.stall_since_ms = Some(now_ms);
    }

    /// The engine could not open the handle or a probe failed: drop it, so the next check
    /// opens the sender again.
    pub fn forget(&mut self) {
        self.open = None;
        self.connected = false;
    }

    /// One check. `seen` is `None` when the sender's mapping cannot be read.
    /// `new_picture` is true when a new picture arrived since the last check.
    pub fn tick(
        &mut self,
        now_ms: u64,
        seen: Option<SenderSeen>,
        new_picture: bool,
    ) -> SenderAction {
        if new_picture {
            self.last_picture_ms = Some(now_ms);
            self.stall_since_ms = Some(now_ms);
        }
        match seen {
            Some(s) if s.listed => {
                self.connected = true;
                self.size = (s.width, s.height);
                match self.open {
                    None => {
                        self.open = Some(s.handle);
                        self.stall_since_ms = Some(now_ms);
                        SenderAction::Open(s.handle)
                    }
                    Some(h) if h != s.handle => {
                        self.open = Some(s.handle);
                        self.stall_since_ms = Some(now_ms);
                        SenderAction::Reopen(s.handle)
                    }
                    Some(h) => {
                        let since = self.stall_since_ms.unwrap_or(now_ms);
                        if now_ms.saturating_sub(since) >= STALL_REOPEN_MS {
                            self.stall_since_ms = Some(now_ms);
                            SenderAction::Reopen(h)
                        } else {
                            SenderAction::Keep
                        }
                    }
                }
            }
            _ => {
                self.connected = false;
                // Forget the handle: whatever comes back is opened fresh.
                self.open = None;
                SenderAction::Hold
            }
        }
    }

    pub fn state(&self, now_ms: u64) -> SenderState {
        SenderState {
            name: self.name.clone(),
            connected: self.connected,
            width: self.size.0,
            height: self.size.1,
            last_picture_age_ms: self.last_picture_ms.map(|t| now_ms.saturating_sub(t)),
        }
    }
}
