//! The engine loop: Spout → pacing → Display.Core scanout, on the wall's vblanks.
//!
//! Recovery without a process restart:
//! - the display target is lost or cannot be opened → retry with back-off, status `failed`;
//! - the sender's handle changes (Arena restart) → re-open, the last picture stays meanwhile;
//! - the sender disappears → hold the last picture, status `degraded`.

use crate::api::Shared;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};
use wb_core::config::Config;
use wb_core::health::{evaluate, Inputs};
use wb_core::pacing::{hms_utc, Pacer, Show};
use wb_core::sender::{SenderAction, SenderWatch};
use wb_core::spout_wire::Content;
use wb_core::status::{specialized_from_targets, DisplayState};
use wb_core::vblank::{missed_between, percentiles_ms};
use wb_scanout::Scanout;
use wb_spout::{Probe, Reader};
use windows::core::{Interface, Result};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Media::timeBeginPeriod;

/// Back-off between attempts to open the display target.
const REOPEN_BACKOFF_S: [u64; 4] = [1, 2, 5, 10];
/// Reporting window.
const WINDOW_S: u64 = 10;
/// Sleep between sender probes when the picture did not change.
const PROBE_SLEEP_US: u64 = 800;
/// Sleep while the GPU is still sampling the probe boxes.
const BUSY_SLEEP_US: u64 = 200;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// D3D11 device on the NVIDIA adapter that drives the wall (as the MVP), else the default one.
fn create_device() -> Result<(ID3D11Device, ID3D11DeviceContext, String)> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1()? };
    let mut chosen: Option<(IDXGIAdapter1, String)> = None;
    let mut i = 0;
    while let Ok(ad) = unsafe { factory.EnumAdapters1(i) } {
        let d = unsafe { ad.GetDesc1()? };
        let name = String::from_utf16_lossy(&d.Description)
            .trim_end_matches('\0')
            .to_string();
        debug!(adapter = i, name = %name, "dxgi adapter");
        if chosen.is_none() && name.contains("NVIDIA") {
            chosen = Some((ad, name));
        }
        i += 1;
    }
    let mut device = None;
    let mut ctx = None;
    let name = match &chosen {
        Some((ad, name)) => {
            let ad: IDXGIAdapter = ad.cast()?;
            unsafe {
                D3D11CreateDevice(
                    &ad,
                    D3D_DRIVER_TYPE_UNKNOWN,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut ctx),
                )?
            };
            name.clone()
        }
        None => {
            unsafe {
                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_HARDWARE,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut ctx),
                )?
            };
            "default hardware adapter".to_string()
        }
    };
    Ok((device.expect("device"), ctx.expect("context"), name))
}

fn ring(device: &ID3D11Device, n: usize, w: u32, h: u32) -> Result<Vec<ID3D11Texture2D>> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: w,
        Height: h,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        // Render target: slots are cleared when a smaller sender would leave old edges.
        BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    (0..n)
        .map(|_| {
            let mut t = None;
            unsafe { device.CreateTexture2D(&desc, None, Some(&mut t))? };
            Ok(t.expect("texture"))
        })
        .collect()
}

/// Black slots, so a sender smaller than the wall leaves no old picture at the edges.
fn clear(device: &ID3D11Device, ctx: &ID3D11DeviceContext, slots: &[ID3D11Texture2D]) {
    for t in slots {
        let mut rtv: Option<ID3D11RenderTargetView> = None;
        match unsafe { device.CreateRenderTargetView(t, None, Some(&mut rtv)) } {
            Ok(()) => {
                if let Some(v) = rtv {
                    unsafe { ctx.ClearRenderTargetView(&v, &[0.0, 0.0, 0.0, 1.0]) };
                }
            }
            Err(e) => warn!(err = ?e, "could not clear a ring slot"),
        }
    }
}

fn publish_state(status: &Shared, display: DisplayState, uptime_s: u64) {
    let mut st = status.write().expect("status lock");
    st.display = display;
    st.uptime_s = uptime_s;
    st.state = evaluate(&Inputs {
        display: &st.display,
        topology: &st.topology,
        sender: &st.sender,
        pacing: &st.pacing,
    });
}

fn display_state(cfg: &Config, scanout: Option<&Scanout>) -> DisplayState {
    let targets = wb_scanout::list_targets().unwrap_or_else(|e| {
        warn!(err = ?e, "list display targets failed");
        vec![]
    });
    DisplayState {
        specialized: specialized_from_targets(&targets, &cfg.wall.monitor),
        target_acquired: scanout.is_some(),
        width: scanout.map(|s| s.width).unwrap_or(0),
        height: scanout.map(|s| s.height).unwrap_or(0),
        hz: scanout.map(|s| s.refresh).unwrap_or(0.0),
    }
}

/// Runs forever. Only a broken D3D device at startup returns an error.
/// `reacquire`: the keeper re-specialized the wall; drop the target and acquire it again.
pub fn run(cfg: Config, status: Shared, reacquire: Arc<AtomicBool>) -> Result<()> {
    unsafe {
        timeBeginPeriod(1);
    }
    if let Err(e) = crate::priority::vblank_thread() {
        warn!(err = ?e, "could not raise vblank thread priority");
    }
    let (class, thread, tid) = crate::priority::effective();
    info!(class, vblank_thread = thread, tid, "scheduling priority");
    let (device, ctx, adapter) = create_device()?;
    info!(adapter = %adapter, "d3d11 device created");
    let mut reader = Reader::new(&device, &cfg.sender.name)?;
    let mut watch = SenderWatch::new(&cfg.sender.name);
    let started = Instant::now();
    let mut attempt = 0usize;

    loop {
        // 1) Acquire the wall.
        let mut scanout = match wb_scanout::open(&cfg.wall.monitor, &device) {
            Ok(s) => {
                info!(
                    w = s.width,
                    h = s.height,
                    hz = s.refresh,
                    "display target acquired"
                );
                attempt = 0;
                s
            }
            Err(e) => {
                let wait = REOPEN_BACKOFF_S[attempt.min(REOPEN_BACKOFF_S.len() - 1)];
                error!(err = ?e, monitor = %cfg.wall.monitor, retry_s = wait, "display target not acquired");
                attempt += 1;
                publish_state(
                    &status,
                    display_state(&cfg, None),
                    started.elapsed().as_secs(),
                );
                std::thread::sleep(std::time::Duration::from_secs(wait));
                continue;
            }
        };
        let period_us = (1_000_000.0 / scanout.refresh.max(1.0)) as u64;
        let guard_us = cfg.pacing.guard_us;
        let ring_t = Instant::now();
        let slots = match ring(&device, cfg.pacing.ring, scanout.width, scanout.height) {
            Ok(s) => s,
            Err(e) => {
                error!(err = ?e, "ring allocation failed, re-acquiring");
                std::thread::sleep(std::time::Duration::from_secs(REOPEN_BACKOFF_S[1]));
                continue;
            }
        };
        clear(&device, &ctx, &slots);
        info!(
            slots = cfg.pacing.ring,
            ms = ring_t.elapsed().as_millis() as u64,
            "picture ring allocated and cleared"
        );
        let mut pacer = Pacer::new(cfg.pacing.ring);
        let display = display_state(&cfg, Some(&scanout));
        // Publish at once: the keeper and the updater's health check read it.
        publish_state(&status, display.clone(), started.elapsed().as_secs());
        let mut arrivals: Vec<u64> = vec![];
        // Holds of LONG_HOLD_VB+ vblanks in this window: (Unix ms the new picture ended it,
        // length in vblanks). Preallocated: no allocation on the vblank path (#15).
        const MAX_HOLDS: usize = 32;
        let mut holds: Vec<(u64, u64, Content)> = Vec::with_capacity(MAX_HOLDS);
        // Content of the last two new pictures: when a hold ends, the newest one ends it and the
        // one before is what stayed on the wall (queue is normally 1).
        let mut content_prev = Content::Unknown;
        let mut content_last = Content::Unknown;
        let mut holds_dropped = 0u64;
        // Presents slower than SLOW_SHOW_US: (Unix ms at its start, µs, present number since
        // the target was acquired), so a box-wide stall can be timed exactly (#20).
        const SLOW_SHOW_US: u64 = 20_000;
        const MAX_SLOW: usize = 16;
        let mut slow_shows: Vec<(u64, u64, u64)> = Vec::with_capacity(MAX_SLOW);
        let mut presents = 0u64;
        let mut missed = 0u64;
        let mut lock_timeouts = 0u64;
        // Per-window maxima (µs) of the steps on the vblank path, to see where time goes.
        let mut show_us_max = 0u64;
        let mut probe_us_max = 0u64;
        let mut check_us_max = 0u64;
        let mut loop_end_us_max = 0u64;
        let mut last_vblank: Option<Instant> = None;
        let mut window_start = Instant::now();

        // 2) Run on the wall's vblanks until the target is lost.
        loop {
            if let Err(e) = scanout.wait_vblank() {
                error!(err = ?e, "wait for vblank failed, re-acquiring the display");
                break;
            }
            let wake = Instant::now();
            if let Some(prev) = last_vblank {
                // Counted only; the window line reports it (no log write on the vblank path).
                missed += missed_between(wake.duration_since(prev).as_micros() as u64, period_us);
            }
            last_vblank = Some(wake);

            if let Show::New(slot) = pacer.on_vblank() {
                let t = Instant::now();
                let t_ms = now_ms();
                if let Err(e) = scanout.show(&ctx, &slots[slot]) {
                    error!(err = ?e, "scanout show failed, re-acquiring the display");
                    break;
                }
                let us = t.elapsed().as_micros() as u64;
                presents += 1;
                show_us_max = show_us_max.max(us);
                if us >= SLOW_SHOW_US && slow_shows.len() < MAX_SLOW {
                    slow_shows.push((t_ms, us, presents));
                }
            }
            if let Some(len) = pacer.take_long_hold() {
                if holds.len() < MAX_HOLDS {
                    holds.push((now_ms(), len, content_prev));
                } else {
                    holds_dropped += 1;
                }
            }

            // Sender check every N vblanks.
            if pacer
                .vb_index()
                .is_multiple_of(cfg.sender.check_every_vb.max(1))
            {
                let t = Instant::now();
                if reacquire.swap(false, Ordering::SeqCst) {
                    warn!("the keeper re-specialized the wall, re-acquiring the display");
                    break;
                }
                let seen = reader.seen();
                match watch.tick(now_ms(), seen) {
                    SenderAction::Keep => {}
                    SenderAction::Open(h) | SenderAction::Reopen(h) => {
                        let what = if reader.open_handle().is_some() {
                            "reopen"
                        } else {
                            "open"
                        };
                        match reader.open(h) {
                            Ok(()) => {
                                info!(
                                    action = what,
                                    handle = format_args!("0x{h:x}"),
                                    size = ?reader.size(),
                                    "sender opened"
                                );
                                // Pay the shared texture's first-use cost here, timed, instead
                                // of inside the first present (137 ms at present #2, #20).
                                match reader.warm_up(&ctx, 500) {
                                    Ok(Some(us)) => info!(us, "sender warm-up copy done"),
                                    Ok(None) => warn!(
                                        "sender warm-up copy skipped or not done within 500 ms"
                                    ),
                                    Err(e) => warn!(err = ?e, "sender warm-up copy failed"),
                                }
                                if reader.size() != Some((display.width, display.height)) {
                                    clear(&device, &ctx, &slots);
                                }
                            }
                            Err(e) => {
                                error!(action = what, handle = format_args!("0x{h:x}"), err = ?e, "sender open failed, retrying at the next check");
                                reader.close();
                                watch.forget();
                            }
                        }
                    }
                    SenderAction::Hold => {
                        if reader.open_handle().is_some() {
                            warn!(sender = %cfg.sender.name, "sender lost, holding the last picture");
                            reader.close();
                        }
                    }
                }
                check_us_max = check_us_max.max(t.elapsed().as_micros() as u64);
            }

            // Watch the sender until shortly before the next vblank.
            let budget = std::time::Duration::from_micros(period_us.saturating_sub(guard_us));
            while wake.elapsed() < budget {
                let mut arrived_off_us = None;
                let t = Instant::now();
                // Never wait for Arena's mutex past the guard.
                let left_ms = budget.saturating_sub(wake.elapsed()).as_millis() as u32;
                let probe = reader.poll(&ctx, left_ms.min(2), || {
                    let off_us = wake.elapsed().as_micros() as u64;
                    arrived_off_us = Some(off_us);
                    let slot = pacer.arrive(off_us as f64 / period_us as f64);
                    slots[slot].clone()
                });
                probe_us_max = probe_us_max.max(t.elapsed().as_micros() as u64);
                match probe {
                    Ok(Probe::New) => {
                        if let Some(us) = arrived_off_us {
                            arrivals.push(us);
                        }
                        watch.picture(now_ms());
                        content_prev = content_last;
                        content_last = reader.last_content();
                        break;
                    }
                    Ok(Probe::Same) => {
                        std::thread::sleep(std::time::Duration::from_micros(PROBE_SLEEP_US))
                    }
                    Ok(Probe::LockTimeout) => lock_timeouts += 1,
                    Ok(Probe::Busy) => {
                        std::thread::sleep(std::time::Duration::from_micros(BUSY_SLEEP_US))
                    }
                    Err(e) => {
                        error!(err = ?e, "sender probe failed, re-opening at the next check");
                        reader.close();
                        watch.forget();
                        break;
                    }
                }
            }

            loop_end_us_max = loop_end_us_max.max(wake.elapsed().as_micros() as u64);

            // Reporting window.
            if window_start.elapsed().as_secs() >= WINDOW_S {
                let mut w = pacer.window();
                w.missed_vblanks = missed;
                (w.arrival_ms_p50, w.arrival_ms_p90) = percentiles_ms(&mut arrivals);
                let sender = watch.state(now_ms());
                // Each hold as "<start>-<end>/<vblanks>/<black|solid|image>" in UTC; start = end -
                // length; the content says whether a still picture explains it (#15). Both are
                // wall times; Arena's own pictures came ~SHOW_DELAY (2 vblanks) earlier.
                let period_ms = period_us as f64 / 1000.0;
                let holds_txt = holds
                    .iter()
                    .map(|&(end, len, content)| {
                        let start = end.saturating_sub((len as f64 * period_ms) as u64);
                        format!(
                            "{}-{}/{len}/{}",
                            hms_utc(start),
                            hms_utc(end),
                            content.label()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                // Each slow present as "<UTC start>/<µs>@<present number since acquire>".
                let slow_txt = slow_shows
                    .iter()
                    .map(|&(at, us, n)| format!("{}/{us}@{n}", hms_utc(at)))
                    .collect::<Vec<_>>()
                    .join(",");
                info!(
                    vblanks = w.vblanks,
                    new = w.new,
                    repeats = w.repeats,
                    drops = w.drops,
                    missed_vblanks = w.missed_vblanks,
                    lock_timeouts,
                    show_us_max,
                    probe_us_max,
                    check_us_max,
                    loop_end_us_max,
                    runs = ?w.runs,
                    interval_vb = w.interval_vb,
                    phase_mean = w.phase_mean,
                    queue = w.queue,
                    arrival_ms_p50 = w.arrival_ms_p50,
                    arrival_ms_p90 = w.arrival_ms_p90,
                    sender_connected = sender.connected,
                    picture_age_ms = ?sender.last_picture_age_ms,
                    holds = %holds_txt,
                    holds_dropped,
                    slow_shows = %slow_txt,
                    lock_bypassed = reader.take_bypassed(),
                    "window"
                );
                holds.clear();
                holds_dropped = 0;
                slow_shows.clear();
                let mut st = status.write().expect("status lock");
                // The keeper owns `specialized`: it reads the live target list.
                st.display = DisplayState {
                    specialized: st.display.specialized,
                    ..display.clone()
                };
                st.sender = sender;
                st.pacing = w;
                st.uptime_s = started.elapsed().as_secs();
                st.state = evaluate(&Inputs {
                    display: &st.display,
                    topology: &st.topology,
                    sender: &st.sender,
                    pacing: &st.pacing,
                });
                drop(st);
                arrivals.clear();
                missed = 0;
                lock_timeouts = 0;
                show_us_max = 0;
                probe_us_max = 0;
                check_us_max = 0;
                loop_end_us_max = 0;
                window_start = Instant::now();
            }
        }
        drop(scanout);
        publish_state(
            &status,
            display_state(&cfg, None),
            started.elapsed().as_secs(),
        );
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
