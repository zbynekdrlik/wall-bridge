//! Spout sender watch: decides when to open, re-open or hold the sender.
//!
//! The MVP kept a dead shared handle after an Arena restart (frozen picture, `new=0`).
//! Here a changed handle re-opens, a missing sender holds the last picture, and a static
//! scene (alive, no new pictures) is never treated as a loss.

use serde::Serialize;

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
}

impl SenderWatch {
    pub fn new(name: &str) -> SenderWatch {
        SenderWatch {
            name: name.to_string(),
            open: None,
            connected: false,
            size: (0, 0),
            last_picture_ms: None,
        }
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
        }
        match seen {
            Some(s) if s.listed => {
                self.connected = true;
                self.size = (s.width, s.height);
                match self.open {
                    None => {
                        self.open = Some(s.handle);
                        SenderAction::Open(s.handle)
                    }
                    Some(h) if h != s.handle => {
                        self.open = Some(s.handle);
                        SenderAction::Reopen(s.handle)
                    }
                    Some(_) => SenderAction::Keep,
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
