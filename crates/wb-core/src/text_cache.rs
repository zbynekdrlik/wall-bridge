//! A text refreshed at most once per period, shared by every reader (#31 review): the
//! dashboard sockets all send the same status JSON, so it is built once per second however
//! many dashboards are open, and the engine's status lock sees one reader per second.

use std::sync::Arc;

pub struct TextCache {
    period_ms: u64,
    filled: Option<(u64, Arc<str>)>,
}

impl TextCache {
    pub fn new(period_ms: u64) -> TextCache {
        TextCache {
            period_ms,
            filled: None,
        }
    }

    /// The cached text, refilled by `fill` when it is `period_ms` old or older (or the clock
    /// went back).
    pub fn get(&mut self, now_ms: u64, fill: impl FnOnce() -> String) -> Arc<str> {
        match &self.filled {
            Some((at, text)) if now_ms >= *at && now_ms - *at < self.period_ms => text.clone(),
            _ => {
                let text: Arc<str> = fill().into();
                self.filled = Some((now_ms, text.clone()));
                text
            }
        }
    }
}
