//! `C:\ProgramData\WallBridge\config.toml`. Every key has a default; unknown keys are errors.

use serde::Deserialize;

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub api: ApiCfg,
    pub sender: SenderCfg,
    pub wall: WallCfg,
    pub pacing: PacingCfg,
    pub topology: TopologyCfg,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ApiCfg {
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SenderCfg {
    pub name: String,
    /// Sender re-check interval, in vblanks.
    pub check_every_vb: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WallCfg {
    /// Substring of the wall's Display.Core StableMonitorId.
    pub monitor: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PacingCfg {
    /// Stop polling the sender this long before the next vblank.
    pub guard_us: u64,
    /// Ring of picture copies.
    pub ring: usize,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TopologyCfg {
    /// The keeper checks and restores the Surround layout.
    pub enabled: bool,
    pub check_every_s: u64,
    /// NVAPI display ids of the wall, left to right.
    pub ids: Vec<u32>,
    /// Mode of each wall display inside the grid.
    pub width: u32,
    pub height: u32,
    pub freq: u32,
    /// Displays that stay their own 1x1 grid (the grabber).
    pub keep_single: Vec<u32>,
    /// Name of the span in Settings ▸ Advanced display (for the specialization toggle).
    pub settings_name: String,
    /// The working recipe (SNV history, #1): Windows build and NVIDIA driver (581.57 = 58157).
    pub expected_os_build: u32,
    pub expected_driver: u32,
}

impl Default for TopologyCfg {
    fn default() -> Self {
        TopologyCfg {
            enabled: true,
            check_every_s: 10,
            ids: vec![0x8006_1086, 0x8006_1087, 0x8006_1083],
            width: 2560,
            height: 1080,
            freq: 60,
            keep_single: vec![0x8006_1081],
            settings_name: "NV Surround".into(),
            expected_os_build: 26200,
            expected_driver: 58157,
        }
    }
}

impl TopologyCfg {
    pub fn wall_def(&self) -> crate::topology::WallDef {
        crate::topology::WallDef {
            ids: self.ids.clone(),
            width: self.width,
            height: self.height,
            freq: self.freq,
            keep_single: self.keep_single.clone(),
        }
    }
}

/// Warnings when the box drifts from the working recipe (25H2 + NVIDIA 581.57).
pub fn drift_warnings(
    cfg: &TopologyCfg,
    os_build: Option<u32>,
    driver: Option<u32>,
) -> Vec<String> {
    let mut w = vec![];
    match os_build {
        Some(b) if b != cfg.expected_os_build => w.push(format!(
            "Windows build {b} != {} (25H2)",
            cfg.expected_os_build
        )),
        None => w.push("Windows build unknown".into()),
        _ => {}
    }
    let fmt = |v: u32| format!("{}.{:02}", v / 100, v % 100);
    match driver {
        Some(d) if d != cfg.expected_driver => w.push(format!(
            "NVIDIA driver {} != {}",
            fmt(d),
            fmt(cfg.expected_driver)
        )),
        None => w.push("NVIDIA driver version unknown".into()),
        _ => {}
    }
    w
}

impl Default for ApiCfg {
    fn default() -> Self {
        ApiCfg { port: 8930 }
    }
}

impl Default for SenderCfg {
    fn default() -> Self {
        SenderCfg {
            name: "Arena - Screen 2".into(),
            check_every_vb: 30,
        }
    }
}

impl Default for WallCfg {
    fn default() -> Self {
        WallCfg {
            monitor: "GSM779465795".into(),
            width: 7680,
            height: 1080,
        }
    }
}

impl Default for PacingCfg {
    fn default() -> Self {
        PacingCfg {
            guard_us: 3500,
            ring: 5,
        }
    }
}

impl Config {
    pub fn from_toml(text: &str) -> Result<Config, toml::de::Error> {
        toml::from_str(text)
    }

    /// Reads the file; a missing file means all defaults.
    pub fn load(path: &std::path::Path) -> Result<Config, String> {
        match std::fs::read_to_string(path) {
            Ok(t) => Config::from_toml(&t).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}
