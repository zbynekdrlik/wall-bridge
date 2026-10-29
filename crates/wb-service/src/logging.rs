//! Daily rolling log files (14 kept).

use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

pub const KEEP_FILES: usize = 14;

/// Keep the guard alive for the whole process, or buffered lines are lost.
pub fn init(dir: &Path) -> Result<WorkerGuard, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("log dir {}: {e:?}", dir.display()))?;
    let file = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("engine")
        .filename_suffix("log")
        .max_log_files(KEEP_FILES)
        .build(dir)
        .map_err(|e| format!("log appender {}: {e:?}", dir.display()))?;
    let (writer, guard) = tracing_appender::non_blocking(file);
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug")))
        // File only: a console write blocks the engine thread for milliseconds.
        .with(fmt::layer().with_ansi(false).with_writer(writer))
        .try_init()
        .map_err(|e| format!("tracing init: {e:?}"))?;
    Ok(guard)
}
