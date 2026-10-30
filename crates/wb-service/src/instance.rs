//! One engine per machine: a second copy would fight over the display target.

use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

/// Holds the named mutex for the life of the process.
pub struct Instance(#[allow(dead_code)] HANDLE);

/// `None` when another engine already runs.
pub fn acquire() -> windows::core::Result<Option<Instance>> {
    let h = unsafe { CreateMutexW(None, true, w!("Global\\WallBridgeEngine"))? };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return Ok(None);
    }
    Ok(Some(Instance(h)))
}
