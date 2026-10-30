//! NVIDIA Mosaic (Surround) read and set through nvapi64.dll, ported call for call from the
//! proven SNV scripts (`docs/reference/nvapi/nvapi_grid_read.py`, `nvapi_grid_cycle.py`,
//! `nvapi_grid_reform_keep.py`): NvAPI_Initialize, NvAPI_Mosaic_EnumDisplayGrids and
//! NvAPI_Mosaic_SetDisplayGrids with NV_MOSAIC_GRID_TOPO v2 (1832 bytes).

use wb_core::topology::Grid;

/// NV_MOSAIC_GRID_TOPO_DISPLAY v? (28 bytes).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RawDisplay {
    pub version: u32,
    pub display_id: u32,
    pub overlap_x: i32,
    pub overlap_y: i32,
    pub rotation: i32,
    pub clone_group: u32,
    pub pixel_shift_type: i32,
}

/// NV_MOSAIC_DISPLAY_SETTING v1 (20 bytes).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RawSetting {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub bpp: u32,
    pub freq: u32,
}

/// NV_MOSAIC_GRID_TOPO v2.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawGrid {
    pub version: u32,
    pub rows: u32,
    pub columns: u32,
    pub display_count: u32,
    pub flags: u32,
    pub displays: [RawDisplay; 64],
    pub setting: RawSetting,
}

pub const GRID_SIZE: usize = std::mem::size_of::<RawGrid>();
pub const GRID_VER: u32 = GRID_SIZE as u32 | (2 << 16);

impl Default for RawGrid {
    fn default() -> Self {
        RawGrid {
            version: GRID_VER,
            rows: 0,
            columns: 0,
            display_count: 0,
            flags: 0,
            displays: [RawDisplay::default(); 64],
            setting: RawSetting::default(),
        }
    }
}

impl RawGrid {
    pub fn from_bytes(b: &[u8]) -> Option<RawGrid> {
        (b.len() == GRID_SIZE)
            .then(|| unsafe { std::ptr::read_unaligned(b.as_ptr() as *const RawGrid) })
    }

    pub fn to_grid(&self) -> Grid {
        let n = (self.display_count as usize).min(64);
        Grid {
            rows: self.rows,
            cols: self.columns,
            ids: self.displays[..n].iter().map(|d| d.display_id).collect(),
            width: self.setting.width,
            height: self.setting.height,
            freq: self.setting.freq,
        }
    }

    /// A grid to set: per-display entries and the setting are copied from `template` (the
    /// pinned grid of 13.10.2026 or what the driver reported), ids and shape from `g`.
    pub fn from_grid(g: &Grid, template: &RawGrid) -> RawGrid {
        let mut r = *template;
        r.version = GRID_VER;
        // Surround flags belong to the template's topology only: a 1x1 grid carrying the 1x3
        // wall's flags is rejected by SetDisplayGrids (-5, measured on SNV).
        if (g.rows, g.cols) != (template.rows, template.columns) {
            r.flags = 0;
        }
        r.rows = g.rows;
        r.columns = g.cols;
        r.display_count = g.ids.len() as u32;
        let proto = template.displays[0];
        for (k, &id) in g.ids.iter().enumerate() {
            let src = template.displays[..template.display_count.min(64) as usize]
                .iter()
                .find(|d| d.display_id == id)
                .copied()
                .unwrap_or(proto);
            r.displays[k] = RawDisplay {
                display_id: id,
                ..src
            };
        }
        for d in r.displays[g.ids.len()..].iter_mut() {
            *d = RawDisplay::default();
        }
        r.setting.width = g.width;
        r.setting.height = g.height;
        r.setting.freq = g.freq;
        r
    }
}

/// The 1x3 wall grid pinned on 13.10.2026 (A 0x80061086, B 0x80061087, C 0x80061083,
/// 2560x1080@60 each).
pub const PINNED: &[u8] = include_bytes!("../pinned-1x3-20261013.bin");

#[cfg(windows)]
mod nv {
    use super::*;
    use windows::core::{s, w};
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    type QueryInterface = unsafe extern "C" fn(u32) -> *const std::ffi::c_void;
    type Init = unsafe extern "C" fn() -> i32;
    type EnumGrids = unsafe extern "C" fn(*mut RawGrid, *mut u32) -> i32;
    type SetGrids = unsafe extern "C" fn(*const RawGrid, u32, u32) -> i32;
    type DriverVersion = unsafe extern "C" fn(*mut u32, *mut u8) -> i32;

    const ID_INIT: u32 = 0x0150_E828;
    const ID_ENUM_GRIDS: u32 = 0xDF28_87AF;
    const ID_SET_GRIDS: u32 = 0x4D95_9A89;
    const ID_DRIVER_VERSION: u32 = 0x2926_AAAD;

    pub struct Nvapi {
        enum_grids: EnumGrids,
        set_grids: SetGrids,
        driver_version: DriverVersion,
    }

    impl Nvapi {
        pub fn load() -> Result<Nvapi, String> {
            unsafe {
                let lib =
                    LoadLibraryW(w!("nvapi64.dll")).map_err(|e| format!("nvapi64.dll: {e:?}"))?;
                let qi = GetProcAddress(lib, s!("nvapi_QueryInterface"))
                    .ok_or("nvapi_QueryInterface missing")?;
                let qi: QueryInterface = std::mem::transmute(qi);
                let get = |id: u32| {
                    let p = qi(id);
                    if p.is_null() {
                        Err(format!("nvapi function 0x{id:08x} not available"))
                    } else {
                        Ok(p)
                    }
                };
                let init: Init = std::mem::transmute(get(ID_INIT)?);
                let r = init();
                if r != 0 {
                    return Err(format!("NvAPI_Initialize = {r}"));
                }
                Ok(Nvapi {
                    enum_grids: std::mem::transmute::<*const std::ffi::c_void, EnumGrids>(get(
                        ID_ENUM_GRIDS,
                    )?),
                    set_grids: std::mem::transmute::<*const std::ffi::c_void, SetGrids>(get(
                        ID_SET_GRIDS,
                    )?),
                    driver_version: std::mem::transmute::<*const std::ffi::c_void, DriverVersion>(
                        get(ID_DRIVER_VERSION)?,
                    ),
                })
            }
        }

        pub fn read_raw(&self) -> Result<Vec<RawGrid>, String> {
            unsafe {
                let mut n = 0u32;
                let r = (self.enum_grids)(std::ptr::null_mut(), &mut n);
                if r != 0 {
                    return Err(format!("EnumDisplayGrids(count) = {r}"));
                }
                let mut grids = vec![RawGrid::default(); n.max(1) as usize];
                let r = (self.enum_grids)(grids.as_mut_ptr(), &mut n);
                if r != 0 {
                    return Err(format!("EnumDisplayGrids = {r}"));
                }
                grids.truncate(n as usize);
                Ok(grids)
            }
        }

        /// Driver version as NVAPI reports it (581.57 = 58157).
        pub fn driver_version(&self) -> Result<u32, String> {
            let mut v = 0u32;
            let mut branch = [0u8; 64];
            let r = unsafe { (self.driver_version)(&mut v, branch.as_mut_ptr()) };
            if r == 0 {
                Ok(v)
            } else {
                Err(format!("SYS_GetDriverAndBranchVersion = {r}"))
            }
        }

        pub fn set_raw(&self, grids: &[RawGrid]) -> Result<(), String> {
            let r = unsafe { (self.set_grids)(grids.as_ptr(), grids.len() as u32, 0) };
            if r == 0 {
                Ok(())
            } else {
                Err(format!("SetDisplayGrids = {r}"))
            }
        }
    }
}

#[cfg(windows)]
pub use nv::Nvapi;
