//! wall-bridge service: engine (Windows), status API and dashboard, logging.

pub mod api;
pub mod logging;

#[cfg(windows)]
pub mod engine;
#[cfg(windows)]
pub mod instance;
