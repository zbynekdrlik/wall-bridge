//! wall-bridge MVP (#259): Arena Spout screen -> NVIDIA Surround span, one present per vblank,
//! with a per-second record of what was received and what was shown.
//! Measurement prototype, not product code.

mod dc;

use std::ffi::CString;
use std::io::Write;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use windows::core::{s, w, Interface, PCSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Media::timeBeginPeriod;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const SAMPLES: u32 = 16; // 8x8 boxes spread over the frame, the new-frame signature

struct Args {
    sender: String,
    rect: (i32, i32, i32, i32),
    seconds: u64,
    phase_ms: f64,
    guard_us: u64,
    dc: Option<String>,
    arrivals: Option<String>,
    log: String,
}

fn parse_args() -> Args {
    let mut a = Args {
        sender: "Arena - LG wall spout".into(),
        rect: (3840, 0, 7680, 1080),
        seconds: 60,
        phase_ms: 0.0,
        guard_us: 3500,
        dc: None,
        arrivals: None,
        log: r"C:\ProgramData\SongPlayer\diag\wall-bridge\mvp\bridge.jsonl".into(),
    };
    let v: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i + 1 < v.len() {
        let val = v[i + 1].clone();
        match v[i].as_str() {
            "--sender" => a.sender = val,
            "--seconds" => a.seconds = val.parse().expect("seconds"),
            "--phase-ms" => a.phase_ms = val.parse().expect("phase-ms"),
            "--log" => a.log = val,
            "--guard-us" => a.guard_us = val.parse().expect("guard-us"),
            "--dc" => a.dc = Some(val),
            "--arrivals" => a.arrivals = Some(val),
            "--rect" => {
                let p: Vec<i32> = val.split(',').map(|x| x.parse().expect("rect")).collect();
                a.rect = (p[0], p[1], p[2], p[3]);
            }
            other => panic!("unknown argument {other}"),
        }
        i += 2;
    }
    a
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SharedTextureInfo {
    share_handle: u32,
    width: u32,
    height: u32,
    format: u32,
    usage: u32,
    description: [u8; 256],
    partner_id: u32,
}

fn read_sender(name: &str) -> Option<SharedTextureInfo> {
    let c = CString::new(name).ok()?;
    unsafe {
        let h = OpenFileMappingA(FILE_MAP_READ.0, false, PCSTR(c.as_ptr() as _)).ok()?;
        let view = MapViewOfFile(
            h,
            FILE_MAP_READ,
            0,
            0,
            std::mem::size_of::<SharedTextureInfo>(),
        );
        let info = if view.Value.is_null() {
            None
        } else {
            let i = *(view.Value as *const SharedTextureInfo);
            let _ = UnmapViewOfFile(view);
            Some(i)
        };
        let _ = CloseHandle(h);
        info
    }
}

fn list_senders() -> Vec<String> {
    let mut out = vec![];
    unsafe {
        let Ok(h) = OpenFileMappingA(FILE_MAP_READ.0, false, s!("SpoutSenderNames")) else {
            return out;
        };
        let view = MapViewOfFile(h, FILE_MAP_READ, 0, 0, 0);
        if !view.Value.is_null() {
            let base = view.Value as *const u8;
            for k in 0..64 {
                let slot = std::slice::from_raw_parts(base.add(k * 256), 256);
                if slot[0] == 0 {
                    break;
                }
                let n = slot.iter().position(|&b| b == 0).unwrap_or(256);
                out.push(String::from_utf8_lossy(&slot[..n]).into_owned());
            }
            let _ = UnmapViewOfFile(view);
        }
        let _ = CloseHandle(h);
    }
    out
}

unsafe extern "system" fn wndproc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    DefWindowProcW(h, m, wp, lp)
}

fn pct(v: &mut Vec<u64>) -> (f64, f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    v.sort_unstable();
    let g = |q: f64| v[((v.len() - 1) as f64 * q) as usize] as f64 / 1000.0;
    (g(0.1), g(0.5), g(0.9))
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

struct Stats {
    vblanks: u64,
    new_frames: u64,
    repeats: u64,
    lock_timeouts: u64,
    present_gaps: u64,
    runs: [u64; 9], // how many vblanks each picture stayed: 1..8, 9 = longer
    run: u32,
    copy_us_max: u128,
}

impl Stats {
    fn new() -> Self {
        Stats {
            vblanks: 0,
            new_frames: 0,
            repeats: 0,
            lock_timeouts: 0,
            present_gaps: 0,
            runs: [0; 9],
            run: 0,
            copy_us_max: 0,
        }
    }
    fn close_run(&mut self) {
        if self.run > 0 {
            let i = (self.run.min(9) - 1) as usize;
            self.runs[i] += 1;
        }
        self.run = 0;
    }
}

fn main() -> windows::core::Result<()> {
    let args = parse_args();
    if let Some(dir) = std::path::Path::new(&args.log).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&args.log)
        .expect("log");
    let mut say = |line: String| {
        println!("{line}");
        let _ = writeln!(log, "{line}");
    };
    say(format!("{{\"t\":{},\"event\":\"start\",\"sender\":\"{}\",\"rect\":\"{:?}\",\"phase_ms\":{},\"senders\":{:?}}}",
        now_ms(), args.sender, args.rect, args.phase_ms, list_senders()));
    say(format!(
        "{{\"t\":{},\"event\":\"targets\",\"list\":{:?}}}",
        now_ms(),
        dc::list_targets().unwrap_or_default()
    ));

    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        timeBeginPeriod(1);
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);

        // The adapter and output that own the span.
        let factory: IDXGIFactory2 = CreateDXGIFactory1()?;
        let (rx, ry, rw, rh) = args.rect;
        let mut found: Option<(IDXGIAdapter1, IDXGIOutput)> = None;
        let mut ai = 0;
        while let Ok(ad) = factory.EnumAdapters1(ai) {
            let mut oi = 0;
            while let Ok(out) = ad.EnumOutputs(oi) {
                let d = out.GetDesc()?;
                let r = d.DesktopCoordinates;
                if r.left == rx && r.top == ry && r.right - r.left == rw && r.bottom - r.top == rh {
                    found = Some((ad.clone(), out));
                }
                oi += 1;
            }
            ai += 1;
        }
        if found.is_none() && args.dc.is_some() {
            let mut ai = 0;
            while let Ok(ad) = factory.EnumAdapters1(ai) {
                let d = ad.GetDesc1()?;
                let name = String::from_utf16_lossy(&d.Description);
                if name.contains("NVIDIA") {
                    let out = ad.EnumOutputs(0)?;
                    found = Some((ad, out));
                    break;
                }
                ai += 1;
            }
        }
        let Some((adapter, output)) = found else {
            say(format!(
                "{{\"t\":{},\"event\":\"fatal\",\"why\":\"no output at {:?}\"}}",
                now_ms(),
                args.rect
            ));
            std::process::exit(2);
        };
        let mut device: Option<ID3D11Device> = None;
        let mut ctx: Option<ID3D11DeviceContext> = None;
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut ctx),
        )?;
        let device = device.unwrap();
        let ctx = ctx.unwrap();

        let mut dcout: Option<dc::DcOut> = match &args.dc {
            Some(want) => match dc::open(want, &device) {
                Ok(d) => {
                    say(format!(
                        "{{\"t\":{},\"event\":\"dc_open\",\"w\":{},\"h\":{},\"hz\":{:.4}}}",
                        now_ms(),
                        d.width,
                        d.height,
                        d.refresh
                    ));
                    Some(d)
                }
                Err(e) => {
                    say(format!(
                        "{{\"t\":{},\"event\":\"fatal\",\"why\":\"dc open: {}\"}}",
                        now_ms(),
                        e
                    ));
                    std::process::exit(3);
                }
            },
            None => None,
        };
        let (mut hwnd, mut sc, mut waitable, mut back): (
            Option<HWND>,
            Option<IDXGISwapChain1>,
            Option<HANDLE>,
            Option<ID3D11Texture2D>,
        ) = (None, None, None, None);
        if dcout.is_none() {
            // A borderless topmost window over the span.
            let hinst = GetModuleHandleW(None)?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: hinst.into(),
                lpszClassName: w!("WallBridgeMvp"),
                ..Default::default()
            };
            RegisterClassW(&wc);
            let h = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("WallBridgeMvp"),
                w!("wall-bridge-mvp"),
                WS_POPUP | WS_VISIBLE,
                rx,
                ry,
                rw,
                rh,
                None,
                None,
                hinst,
                None,
            )?;
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: rw as u32,
                Height: rh as u32,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                Flags: DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT.0 as u32,
                ..Default::default()
            };
            let s1: IDXGISwapChain1 =
                factory.CreateSwapChainForHwnd(&device, h, &desc, None, None)?;
            let s2: IDXGISwapChain2 = s1.cast()?;
            s2.SetMaximumFrameLatency(1)?;
            waitable = Some(s2.GetFrameLatencyWaitableObject());
            back = Some(s1.GetBuffer(0)?);
            sc = Some(s1);
            hwnd = Some(h);
        }

        // Own copy of the latest Spout picture, and the signature staging.
        let tex_desc =
            |w: u32, h: u32, usage: D3D11_USAGE, cpu: u32, bind: u32| D3D11_TEXTURE2D_DESC {
                Width: w,
                Height: h,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: usage,
                BindFlags: bind,
                CPUAccessFlags: cpu,
                MiscFlags: 0,
            };
        let mut work: Option<ID3D11Texture2D> = None;
        device.CreateTexture2D(
            &tex_desc(rw as u32, rh as u32, D3D11_USAGE_DEFAULT, 0, 0),
            None,
            Some(&mut work),
        )?;
        let work = work.unwrap();
        let mut ring: Vec<ID3D11Texture2D> = vec![];
        for _ in 0..5 {
            let mut t: Option<ID3D11Texture2D> = None;
            device.CreateTexture2D(
                &tex_desc(rw as u32, rh as u32, D3D11_USAGE_DEFAULT, 0, 0),
                None,
                Some(&mut t),
            )?;
            ring.push(t.unwrap());
        }
        let mut queue: std::collections::VecDeque<(usize, u64)> = Default::default();
        let mut vb_index: u64 = 0;
        let mut phase_c: f64 = 1.0;
        let mut phase_s: f64 = 0.0; // circular mean of the arrival phase
        let mut shown: Option<usize> = None;
        let mut acc: f64 = 0.0;
        let mut interval_vb: f64 = 2.0;
        let mut last_arrival: Option<Instant> = None;
        let mut drops: u64 = 0;
        let mut underruns: u64 = 0;
        let mut catchups: u64 = 0;
        let mut stage: Option<ID3D11Texture2D> = None;
        device.CreateTexture2D(
            &tex_desc(
                8 * SAMPLES,
                8,
                D3D11_USAGE_STAGING,
                D3D11_CPU_ACCESS_READ.0 as u32,
                0,
            ),
            None,
            Some(&mut stage),
        )?;
        let stage = stage.unwrap();

        let mutex_name: Vec<u16> = format!("{}_SpoutAccessMutex", args.sender)
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut shared: Option<(u32, ID3D11Texture2D, u32, u32, u32)> = None;
        let mut mutex: Option<HANDLE> = None;
        let mut prev_sig: Vec<u8> = vec![];
        let mut stats = Stats::new();
        let mut last_present_refresh: Option<u32> = None;
        let start = Instant::now();
        let mut window_start = Instant::now();
        let mut frame_no: u64 = 0;
        let mut pending_new = false;
        let mut arrival_us: Vec<u64> = vec![];
        let period_us: u64 = 16_683;
        let mut arrivals_file = args
            .arrivals
            .as_ref()
            .map(|p| std::fs::File::create(p).expect("arrivals file"));

        while start.elapsed().as_secs() < args.seconds {
            match &dcout {
                Some(d) => {
                    let _ = d.wait_vblank();
                }
                None => {
                    WaitForSingleObjectEx(waitable.unwrap(), 100, true);
                }
            }
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if args.phase_ms > 0.0 {
                std::thread::sleep(std::time::Duration::from_micros(
                    (args.phase_ms * 1000.0) as u64,
                ));
            }
            frame_no += 1;

            // (Re)open the sender when its handle changes, checked every 30 vblanks.
            if shared.is_none() || frame_no % 30 == 0 {
                if let Some(info) = read_sender(&args.sender) {
                    let stale = shared
                        .as_ref()
                        .map(|s| s.0 != info.share_handle)
                        .unwrap_or(true);
                    if stale {
                        let h = HANDLE(info.share_handle as usize as *mut _);
                        let mut opened: Option<ID3D11Texture2D> = None;
                        match device
                            .OpenSharedResource(h, &mut opened)
                            .and_then(|_| opened.ok_or_else(|| windows::core::Error::from(E_FAIL)))
                        {
                            Ok(t) => {
                                say(format!("{{\"t\":{},\"event\":\"sender\",\"w\":{},\"h\":{},\"format\":{},\"handle\":\"0x{:x}\"}}",
                                    now_ms(), info.width, info.height, info.format, info.share_handle));
                                shared = Some((
                                    info.share_handle,
                                    t,
                                    info.width,
                                    info.height,
                                    info.format,
                                ));
                                mutex = OpenMutexW(
                                    SYNCHRONIZATION_SYNCHRONIZE,
                                    false,
                                    windows::core::PCWSTR(mutex_name.as_ptr()),
                                )
                                .ok();
                            }
                            Err(e) => say(format!(
                                "{{\"t\":{},\"event\":\"open_failed\",\"err\":\"{e}\"}}",
                                now_ms()
                            )),
                        }
                    }
                } else if shared.is_some() {
                    say(format!("{{\"t\":{},\"event\":\"sender_gone\"}}", now_ms()));
                    shared = None;
                }
            }

            stats.vblanks += 1;
            // 1) Each picture is shown at a fixed delay after the vblank interval it was written in.
            //    The interval is decided against the arrival phase's running mean, so a picture
            //    written near a vblank is never split between two intervals (no 1/3 runs).
            vb_index += 1;
            let mut advanced = false;
            let mut take: Option<usize> = None;
            while let Some(&(slot, at)) = queue.front() {
                if at <= vb_index {
                    if take.is_some() {
                        drops += 1;
                    }
                    take = Some(slot);
                    queue.pop_front();
                } else {
                    break;
                }
            }
            if let Some(next) = take {
                match (&mut dcout, &back) {
                    (Some(d), _) => {
                        let _ = d.show(&ctx, &ring[next]);
                    }
                    (None, Some(bk)) => ctx.CopyResource(bk, &ring[next]),
                    _ => {}
                }
                shown = Some(next);
                advanced = true;
            }
            if advanced {
                stats.new_frames += 1;
                stats.close_run();
            } else {
                stats.repeats += 1;
                if let Some(sh) = shown {
                    if let Some(bk) = &back {
                        ctx.CopyResource(bk, &ring[sh]);
                    }
                }
            }
            stats.run += 1;
            if let Some(c) = &sc {
                let _ = c.Present(1, DXGI_PRESENT(0));
            }
            let wake = Instant::now();
            pending_new = false;

            // 2) Until shortly before the next vblank, watch the sender every ~1 ms and take a
            //    new picture the moment Arena has written it (adaptive phase, no fixed offset).
            if let Some((_, tex, sw, sh, _)) = &shared {
                let budget =
                    std::time::Duration::from_micros(period_us.saturating_sub(args.guard_us));
                while wake.elapsed() < budget && !pending_new {
                    let t0 = Instant::now();
                    let locked = match mutex {
                        Some(m) => {
                            let r = WaitForSingleObject(m, 2);
                            r == WAIT_OBJECT_0 || r == WAIT_ABANDONED
                        }
                        None => true,
                    };
                    if !locked {
                        stats.lock_timeouts += 1;
                        continue;
                    }
                    // Probe: the signature boxes straight from the shared texture.
                    for k in 0..SAMPLES {
                        let x = ((k * 2 + 1) * (*sw) / (SAMPLES * 2)).min(*sw - 8);
                        let y = (((k % 4) * 2 + 1) * (*sh) / 8).min(*sh - 8);
                        let b = D3D11_BOX {
                            left: x,
                            top: y,
                            front: 0,
                            right: x + 8,
                            bottom: y + 8,
                            back: 1,
                        };
                        ctx.CopySubresourceRegion(&stage, 0, k * 8, 0, 0, tex, 0, Some(&b));
                    }
                    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                    ctx.Map(&stage, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                    let mut sig = Vec::with_capacity((8 * SAMPLES * 4 * 8) as usize);
                    for row in 0..8 {
                        let p = (mapped.pData as *const u8).add(row * mapped.RowPitch as usize);
                        sig.extend_from_slice(std::slice::from_raw_parts(
                            p,
                            (8 * SAMPLES * 4) as usize,
                        ));
                    }
                    ctx.Unmap(&stage, 0);
                    if sig != prev_sig {
                        let free = (0..ring.len())
                            .find(|i| Some(*i) != shown && !queue.iter().any(|q| q.0 == *i));
                        let slot = match free {
                            Some(i) => i,
                            None => {
                                drops += 1;
                                queue.pop_front().unwrap().0
                            }
                        };
                        if *sw == rw as u32 && *sh == rh as u32 {
                            ctx.CopyResource(&ring[slot], tex);
                        } else {
                            let b = D3D11_BOX {
                                left: 0,
                                top: 0,
                                front: 0,
                                right: (*sw).min(rw as u32),
                                bottom: (*sh).min(rh as u32),
                                back: 1,
                            };
                            ctx.CopySubresourceRegion(&ring[slot], 0, 0, 0, 0, tex, 0, Some(&b));
                        }
                        ctx.Flush();
                        // Arrival phase in [0,1) of the period; assign the interval it belongs to.
                        let off = wake.elapsed().as_micros() as f64 / period_us as f64;
                        let ang = off.fract() * std::f64::consts::TAU;
                        phase_c = 0.97 * phase_c + 0.03 * ang.cos();
                        phase_s = 0.97 * phase_s + 0.03 * ang.sin();
                        let mean = phase_s.atan2(phase_c).rem_euclid(std::f64::consts::TAU)
                            / std::f64::consts::TAU;
                        // Distance of this arrival from the mean, wrapped to [-0.5, 0.5).
                        let d = ((off.fract() - mean + 1.5).rem_euclid(1.0)) - 0.5;
                        // The interval the mean arrival falls in; this arrival belongs to it when close.
                        let m = off.fract() - d;
                        let belongs = if m < 0.0 {
                            vb_index - 1
                        } else if m >= 1.0 {
                            vb_index + 1
                        } else {
                            vb_index
                        };
                        queue.push_back((slot, belongs + 2));
                        prev_sig = sig;
                        pending_new = true;
                        // One line per new picture: its epoch arrival (us) and the vblank it is shown at.
                        if let Some(f) = arrivals_file.as_mut() {
                            let us = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_micros();
                            let _ = writeln!(f, "{} {} {}", us, vb_index, belongs + 2);
                        }
                        let now = Instant::now();
                        if let Some(prev) = last_arrival {
                            let iv = now.duration_since(prev).as_micros() as f64 / period_us as f64;
                            if iv > 0.5 && iv < 6.0 {
                                interval_vb = 0.95 * interval_vb + 0.05 * iv;
                            }
                        }
                        last_arrival = Some(now);
                        arrival_us.push(wake.elapsed().as_micros() as u64);
                    }
                    if let Some(m) = mutex {
                        let _ = ReleaseMutex(m);
                    }
                    stats.copy_us_max = stats.copy_us_max.max(t0.elapsed().as_micros());
                    if !pending_new {
                        std::thread::sleep(std::time::Duration::from_micros(800));
                    }
                }
            }

            let mut fs = DXGI_FRAME_STATISTICS::default();
            if sc
                .as_ref()
                .map(|c| c.GetFrameStatistics(&mut fs).is_ok())
                .unwrap_or(false)
            {
                if let Some(prev) = last_present_refresh {
                    if fs.PresentRefreshCount > prev + 1 {
                        stats.present_gaps += (fs.PresentRefreshCount - prev - 1) as u64;
                    }
                }
                last_present_refresh = Some(fs.PresentRefreshCount);
            }

            if window_start.elapsed().as_secs() >= 10 {
                say(format!("{{\"t\":{},\"event\":\"window\",\"vblanks\":{},\"new\":{},\"repeat_vblanks\":{},\"lock_timeouts\":{},\"missed_vblanks\":{},\"runs\":{:?},\"copy_us_max\":{},\"arrival_ms_p10_p50_p90\":{:?},\"interval_vb\":{:.3},\"phase_mean\":{:.3},\"queue\":{},\"drops\":{},\"underruns\":{},\"catchups\":{}}}",
                    now_ms(), stats.vblanks, stats.new_frames, stats.repeats, stats.lock_timeouts, stats.present_gaps, stats.runs, stats.copy_us_max, pct(&mut arrival_us), interval_vb, phase_s.atan2(phase_c).rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU, queue.len(), drops, underruns, catchups));
                drops = 0;
                underruns = 0;
                catchups = 0;
                arrival_us.clear();
                stats = Stats::new();
                window_start = Instant::now();
            }
        }
        say(format!("{{\"t\":{},\"event\":\"end\"}}", now_ms()));
        if let Some(h) = hwnd {
            let _ = DestroyWindow(h);
        }
        let _ = output;
    }
    Ok(())
}
