//! Output on a SPECIALIZED display ("Remove display from desktop") through
//! Windows.Devices.Display.Core: the wall is not part of the Windows desktop, the bridge owns it.

use windows::core::{Interface, HSTRING};
use windows::Devices::Display::Core::*;
use windows::Devices::Display::DisplayMonitorUsageKind;
use windows::Foundation::Collections::IIterable;
use windows::Graphics::DirectX::{DirectXColorSpace, DirectXPixelFormat};
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::System::WinRT::Display::IDisplayDeviceInterop;

pub struct DcOut {
    pub device: DisplayDevice,
    pub source: DisplaySource,
    pub textures: Vec<ID3D11Texture2D>,
    scanouts: Vec<DisplayScanout>,
    pool: DisplayTaskPool,
    query: ID3D11Query,
    next: usize,
    pub width: u32,
    pub height: u32,
    pub refresh: f64,
    pub adapter_luid: (u32, i32),
    _manager: DisplayManager,
}

pub fn list_targets() -> windows::core::Result<Vec<String>> {
    let m = DisplayManager::Create(DisplayManagerOptions::None)?;
    let mut out = vec![];
    for t in m.GetCurrentTargets()? {
        let usage = t.UsageKind().map(|u| u.0).unwrap_or(-1);
        let id = t.StableMonitorId().map(|h| h.to_string()).unwrap_or_default();
        out.push(format!("{} usage={} connected={}", id, usage, t.IsConnected().unwrap_or(false)));
    }
    Ok(out)
}

/// Picks the special-purpose target whose StableMonitorId contains `want` (or the first one).
fn step<T>(name: &str, r: windows::core::Result<T>) -> windows::core::Result<T> {
    r.map_err(|e| windows::core::Error::new(e.code(), format!("{name}: {}", e.message())))
}

pub fn open(want: &str, d3d: &ID3D11Device) -> windows::core::Result<DcOut> {
    let manager = DisplayManager::Create(DisplayManagerOptions::EnforceSourceOwnership)?;
    let mut target: Option<DisplayTarget> = None;
    for t in manager.GetCurrentTargets()? {
        if t.UsageKind()? == DisplayMonitorUsageKind::SpecialPurpose && t.IsConnected()? {
            let id = t.StableMonitorId()?.to_string();
            if want.is_empty() || id.contains(want) {
                target = Some(t);
                break;
            }
        }
    }
    let target = target.ok_or_else(|| windows::core::Error::new(E_FAIL, "no connected special-purpose display target"))?;

    let list: IIterable<DisplayTarget> = <IIterable<DisplayTarget> as TryFrom<Vec<Option<DisplayTarget>>>>::try_from(vec![Some(target.clone())])?;
    let res = step("acquire", manager.TryAcquireTargetsAndCreateEmptyState(&list))?;
    if res.ErrorCode()? != DisplayManagerResult::Success {
        return Err(windows::core::Error::new(E_FAIL, format!("acquire failed: {:?}", res.ErrorCode()?.0)));
    }
    let state = res.State()?;
    let path = step("connect", state.ConnectTarget(&target))?;
    let nointerlace: windows::Foundation::IReference<bool> = windows::Foundation::PropertyValue::CreateBoolean(false)?.cast()?;
    step("interlace", path.SetIsInterlaced(&nointerlace))?;
    step("scaling", path.SetScaling(DisplayPathScaling::Identity))?;
    let mut modes = step("modes", path.FindModes(DisplayModeQueryOptions::OnlyPreferredResolution))?;
    if modes.Size()? == 0 {
        modes = step("modes_all", path.FindModes(DisplayModeQueryOptions::None))?;
    }
    eprintln!("modes: {}", modes.Size()?);
    let mut best: Option<(DisplayModeInfo, f64)> = None;
    for m in modes {
        let r = m.PresentationRate()?;
        let hz = r.VerticalSyncRate.Numerator as f64 / r.VerticalSyncRate.Denominator.max(1) as f64;
        let sr = m.SourceResolution()?;
        eprintln!("  mode {}x{} {:.3} Hz interlaced={} stereo={}", sr.Width, sr.Height, hz, m.IsInterlaced()?, m.IsStereo()?);
        if m.IsInterlaced()? || m.IsStereo()? {
            continue;
        }
        let score = (hz - 60.0).abs();
        if best.as_ref().map(|b| score < (b.1 - 60.0).abs()).unwrap_or(true) {
            best = Some((m, hz));
        }
    }
    let (mode, hz) = best.ok_or_else(|| windows::core::Error::new(E_FAIL, "no mode"))?;
    step("applymode", path.ApplyPropertiesFromMode(&mode))?;
    let applied = step("tryapply", state.TryApply(DisplayStateApplyOptions::None))?;
    if applied.Status()? != DisplayStateOperationStatus::Success {
        return Err(windows::core::Error::new(E_FAIL, format!("apply failed: {:?}", applied.Status()?.0)));
    }
    let size: SizeInt32 = mode.SourceResolution()?;
    let reread = step("reread", manager.TryAcquireTargetsAndReadCurrentState(&list))?;
    let _state2 = reread.State()?;
    let _path2 = step("path2", _state2.GetPathForTarget(&target))?;
    let target = step("ptarget", _path2.Target())?;
    let (w, h) = (size.Width as u32, size.Height as u32);

    let device = step("device", manager.CreateDisplayDevice(&target.Adapter()?))?;
    let source = step("source", device.CreateScanoutSource(&target))?;
    let desc = DisplayPrimaryDescription::CreateInstance(
        w,
        h,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        DirectXColorSpace::RgbFullG22NoneP709,
        false,
        windows::Graphics::DirectX::Direct3D11::Direct3DMultisampleDescription { Count: 1, Quality: 0 },
    );
    let desc = step("desc", desc)?;
    let interop: IDisplayDeviceInterop = device.cast()?;
    let d3d1: ID3D11Device1 = d3d.cast()?;
    let mut textures = vec![];
    let mut scanouts = vec![];
    for _ in 0..2 {
        let surface = step("primary", device.CreatePrimary(&target, &desc))?;
        let handle = unsafe { step("sharedhandle", interop.CreateSharedHandle(&surface, std::ptr::null(), GENERIC_ALL.0, &HSTRING::new()))? };
        let tex: ID3D11Texture2D = unsafe { step("openshared", d3d1.OpenSharedResource1(handle))? };
        unsafe { let _ = CloseHandle(handle); }
        scanouts.push(step("scanout", device.CreateSimpleScanout(&source, &surface, 0, 1))?);
        textures.push(tex);
    }
    let pool = device.CreateTaskPool()?;
    let mut query: Option<ID3D11Query> = None;
    unsafe { d3d.CreateQuery(&D3D11_QUERY_DESC { Query: D3D11_QUERY_EVENT, MiscFlags: 0 }, Some(&mut query))? };
    let luid = target.Adapter()?.Id()?;
    Ok(DcOut {
        device, source, textures, scanouts, pool, query: query.unwrap(), next: 0,
        width: w, height: h, refresh: hz, adapter_luid: (luid.LowPart, luid.HighPart), _manager: manager,
    })
}

impl DcOut {
    pub fn wait_vblank(&self) -> windows::core::Result<()> {
        self.device.WaitForVBlank(&self.source)
    }
    /// Copy `src` into the next primary and queue it for scanout on the next vblank.
    pub fn show(&mut self, ctx: &ID3D11DeviceContext, src: &ID3D11Texture2D) -> windows::core::Result<()> {
        let i = self.next;
        unsafe {
            let mut sd = D3D11_TEXTURE2D_DESC::default();
            src.GetDesc(&mut sd);
            if sd.Width == self.width && sd.Height == self.height {
                ctx.CopyResource(&self.textures[i], src);
            } else {
                // MVP: top-left crop when sizes differ (scaling comes with the shader pass).
                let b = D3D11_BOX { left: 0, top: 0, front: 0, right: sd.Width.min(self.width), bottom: sd.Height.min(self.height), back: 1 };
                ctx.CopySubresourceRegion(&self.textures[i], 0, 0, 0, 0, src, 0, Some(&b));
            }
            ctx.End(&self.query);
            ctx.Flush();
            let mut done = BOOL(0);
            loop {
                let _ = ctx.GetData(&self.query, Some(&mut done as *mut BOOL as *mut _), std::mem::size_of::<BOOL>() as u32, 0);
                if done.as_bool() {
                    break;
                }
                std::thread::yield_now();
            }
        }
        let task = self.pool.CreateTask()?;
        task.SetScanout(&self.scanouts[i])?;
        let _ = self.pool.TryExecuteTask(&task)?;
        self.next ^= 1;
        Ok(())
    }
}
