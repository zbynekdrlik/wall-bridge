//! `C:\ProgramData\WallBridge\config.toml`. Every key has a default; unknown keys are errors.

use serde::Deserialize;

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub api: ApiCfg,
    pub sender: SenderCfg,
    pub wall: WallCfg,
    pub pacing: PacingCfg,
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
