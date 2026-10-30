//! Status → tray icon colour, tooltip and status line (plain Slovak for the operator).

use serde::Deserialize;

/// The part of `/api/status` the tray reads.
#[derive(Debug, Clone, Deserialize)]
pub struct Status {
    pub version: String,
    pub state: String,
    #[serde(default)]
    pub reasons: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Light {
    Green,
    Amber,
    Red,
    /// The service does not answer.
    Grey,
}

impl Light {
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Light::Green => [0x1a, 0x7f, 0x37],
            Light::Amber => [0xd2, 0x99, 0x22],
            Light::Red => [0xcf, 0x22, 0x2e],
            Light::Grey => [0x80, 0x80, 0x80],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub light: Light,
    /// First menu line and tooltip.
    pub line: String,
}

fn reason_text(r: &serde_json::Value) -> String {
    match r {
        serde_json::Value::String(s) => match s.as_str() {
            "sender_lost" => "chýba obraz z Arény".into(),
            "topology_mismatch" => "Surround nesedí s definíciou".into(),
            "topology_gave_up" => "Surround sa nedá obnoviť".into(),
            "display_not_acquired" => "stena nie je získaná".into(),
            "display_on_desktop" => "stena je na ploche Windows".into(),
            other => other.into(),
        },
        serde_json::Value::Object(o) => match o.get("missed_vblanks") {
            Some(n) => format!("zmeškané vblanky: {n}"),
            None => r.to_string(),
        },
        other => other.to_string(),
    }
}

/// `None` = the service did not answer.
pub fn view(status: Option<&Status>) -> View {
    let Some(s) = status else {
        return View {
            light: Light::Grey,
            line: "wall-bridge: služba neodpovedá".into(),
        };
    };
    let reasons: Vec<String> = s.reasons.iter().map(reason_text).collect();
    let (light, head) = match s.state.as_str() {
        "ok" => (Light::Green, "Stena OK"),
        "degraded" => (Light::Amber, "Stena beží, niečo nesedí"),
        _ => (Light::Red, "Stena nie je na mostíku"),
    };
    let mut line = format!("{head} (v{})", s.version);
    if !reasons.is_empty() {
        line.push_str(" – ");
        line.push_str(&reasons.join(", "));
    }
    View { light, line }
}

/// A 16×16 RGBA picture of the wall (three panels) in the light's colour.
pub fn icon_rgba(light: Light) -> Vec<u8> {
    const N: usize = 16;
    // The wall: three panels side by side (x 0-4, 6-9, 11-15), one pixel apart, rows 4-11.
    // A plain disc looked like DanteSync's icon in the same tray (owner 30.10.2026).
    let [r, g, b] = light.rgb();
    let mut px = Vec::with_capacity(N * N * 4);
    for y in 0..N {
        for x in 0..N {
            let panel = (4..=11).contains(&y) && x != 5 && x != 10;
            px.extend_from_slice(&if panel { [r, g, b, 255] } else { [0, 0, 0, 0] });
        }
    }
    px
}
