//! Surround keeper: reads the Mosaic layout every few seconds, restores it through NVAPI when
//! it has drifted or collapsed (Separate → FormGrid → VerifyGrid → Specialize), and reports
//! into the status. It never touches a layout that matches the definition.

use crate::api::Shared;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};
use wb_core::config::{drift_warnings, Config};
use wb_core::status::specialized_from_targets;
use wb_core::topology::{decide, Grid, KeeperAction, KeeperHistory, Step, WallDef};
use wb_topology::{Nvapi, RawGrid, PINNED};

/// Let the driver settle after each grid change (the reference scripts wait 3 s).
const SETTLE: Duration = Duration::from_secs(3);
/// Wait after the Settings toggle before reading the specialization again.
const SPECIALIZE_SETTLE: Duration = Duration::from_secs(5);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn os_build() -> Option<u32> {
    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "/v",
            "CurrentBuild",
        ])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .last()?
        .parse()
        .ok()
}

/// Raw grids to set: wall displays from the pinned template, kept grids exactly as read.
fn to_raw(grids: &[Grid], def: &WallDef, read: &[RawGrid], pinned: &RawGrid) -> Vec<RawGrid> {
    grids
        .iter()
        .map(|g| {
            let kept = g.ids.len() == 1 && def.keep_single.contains(&g.ids[0]);
            match read.iter().find(|r| kept && r.to_grid() == *g) {
                Some(r) => *r,
                None => RawGrid::from_grid(g, pinned),
            }
        })
        .collect()
}

fn is_specialized(monitor: &str) -> bool {
    match wb_scanout::list_targets() {
        Ok(t) => specialized_from_targets(&t, monitor),
        Err(e) => {
            warn!(err = ?e, "list display targets failed");
            false
        }
    }
}

/// The proven Settings toggle (`tools\spec_toggle.ps1`, UIA on "Remove display from desktop").
fn specialize(script: &PathBuf, settings_name: &str) -> Result<(), String> {
    let out = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(script)
        .args(["-Target", settings_name, "-Want", "on"])
        .output()
        .map_err(|e| format!("spawn {}: {e:?}", script.display()))?;
    info!(
        code = ?out.status.code(),
        stdout = %String::from_utf8_lossy(&out.stdout).trim(),
        stderr = %String::from_utf8_lossy(&out.stderr).trim(),
        "specialization toggle ran"
    );
    // The toggle leaves Settings open; it must not stay over the operator's screens.
    let _ = std::process::Command::new("taskkill")
        .args(["/IM", "SystemSettings.exe", "/F"])
        .output();
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("spec_toggle exit {:?}", out.status.code()))
    }
}

fn restore(
    nv: &Nvapi,
    def: &WallDef,
    steps: &[Step],
    cfg: &Config,
    pinned: &RawGrid,
    script: &PathBuf,
) -> bool {
    for step in steps {
        let read = match nv.read_raw() {
            Ok(r) => r,
            Err(e) => {
                error!(?step, err = %e, "restore: read failed");
                return false;
            }
        };
        let current: Vec<Grid> = read.iter().map(RawGrid::to_grid).collect();
        info!(?step, ?current, "restore step");
        match step {
            Step::Separate | Step::FormGrid => {
                let want = if *step == Step::Separate {
                    def.separate_grids(&current)
                } else {
                    def.formed_grids(&current)
                };
                if let Err(e) = nv.set_raw(&to_raw(&want, def, &read, pinned)) {
                    error!(?step, ?want, err = %e, "restore: set grids failed");
                    return false;
                }
                std::thread::sleep(SETTLE);
            }
            Step::VerifyGrid => {
                if !def.matches(&current) {
                    error!(?current, "restore: layout still differs after re-forming");
                    return false;
                }
            }
            Step::Specialize => {
                if is_specialized(&cfg.wall.monitor) {
                    continue;
                }
                warn!("wall is back on the desktop, removing it again");
                if let Err(e) = specialize(script, &cfg.topology.settings_name) {
                    error!(err = %e, "restore: specialization toggle failed");
                }
                std::thread::sleep(SPECIALIZE_SETTLE);
                if !is_specialized(&cfg.wall.monitor) {
                    error!("restore: wall still on the desktop");
                    return false;
                }
            }
        }
    }
    true
}

pub fn run(cfg: Config, status: Shared) {
    let def = cfg.topology.wall_def();
    let pinned = RawGrid::from_bytes(PINNED).expect("pinned grid blob");
    let script = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("tools").join("spec_toggle.ps1")))
        .unwrap_or_else(|| PathBuf::from(r"tools\spec_toggle.ps1"));
    let mut h = KeeperHistory::default();
    let mut nv: Option<Nvapi> = None;
    let os = os_build();
    info!(?def, os_build = ?os, script = %script.display(), "surround keeper starting");

    loop {
        if nv.is_none() {
            match Nvapi::load() {
                Ok(n) => nv = Some(n),
                Err(e) => warn!(err = %e, "NVAPI not available, retrying"),
            }
        }
        let driver = nv.as_ref().and_then(|n| n.driver_version().ok());
        let read = match &nv {
            Some(n) => n
                .read_raw()
                .map(|r| r.iter().map(RawGrid::to_grid).collect::<Vec<_>>()),
            None => Err("NVAPI not loaded".to_string()),
        };
        if let Err(e) = &read {
            warn!(err = %e, "topology read failed");
        }
        let span_live = {
            let st = status.read().expect("status lock");
            st.display.target_acquired
                && st.display.specialized
                && st.display.width == cfg.wall.width
                && st.display.height == cfg.wall.height
        };
        let was = h.matches;
        // Off the vblank path: this thread may list the Display.Core targets.
        let specialized = is_specialized(&cfg.wall.monitor);
        let action = decide(&def, read.clone(), span_live, specialized, &mut h, now_ms());
        if was != h.matches {
            info!(matches = h.matches, span_live, unknown = h.unknown, grids = ?read, "topology changed");
        }
        match action {
            KeeperAction::None => {}
            KeeperAction::GiveUp => {
                error!(
                    failures = h.failures(now_ms()),
                    "surround keeper gave up, needs attention"
                )
            }
            KeeperAction::Restore(steps) => {
                warn!(?steps, grids = ?read, "surround layout differs from the definition, restoring");
                let ok = nv
                    .as_ref()
                    .map(|n| restore(n, &def, &steps, &cfg, &pinned, &script))
                    .unwrap_or(false);
                h.restore_finished(now_ms(), ok);
                if ok {
                    h.matches = true;
                    info!("surround layout restored");
                } else {
                    error!(failures = h.failures(now_ms()), "surround restore failed");
                }
            }
        }

        let mut warnings = drift_warnings(&cfg.topology, os, driver);
        if h.unknown {
            warnings.push("Surround: wall displays not visible and the wall not acquired".into());
        }
        if h.read_errors >= 3 {
            warnings.push(format!(
                "Surround layout unreadable ({} reads)",
                h.read_errors
            ));
        }
        let mut st = status.write().expect("status lock");
        st.topology.matches = h.matches;
        st.topology.last_restore_ms = h.last_restore_ms;
        st.topology.failures = h.failures(now_ms()) as u32;
        st.topology.gave_up = h.gave_up;
        st.warnings = warnings;
        drop(st);
        std::thread::sleep(Duration::from_secs(cfg.topology.check_every_s.max(1)));
    }
}
