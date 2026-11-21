//! What the dashboard shows, computed from the `/api/status` JSON (#31). Pure, so it is tested
//! and mutated on Linux; the Leptos UI (`wb-ui`) only renders it. Labels are Slovak (owner).

use serde::Deserialize;
use serde_json::Value;

/// The parts of the `/api/status` body the dashboard reads. Unknown fields are ignored, so the
/// API can grow without breaking an open page.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct StatusView {
    pub version: String,
    /// "ok", "degraded" or "failed".
    pub state: String,
    /// A reason is a string ("sender_lost") or an object ({"missed_vblanks": 3}).
    pub reasons: Vec<Value>,
    pub uptime_s: u64,
    pub display: DisplayView,
    pub topology: TopologyView,
    pub sender: SenderView,
    pub pacing: PacingView,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DisplayView {
    pub specialized: bool,
    pub target_acquired: bool,
    pub width: u32,
    pub height: u32,
    pub hz: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TopologyView {
    pub matches: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SenderView {
    pub name: String,
    pub connected: bool,
    pub width: u32,
    pub height: u32,
    pub last_picture_age_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PacingView {
    pub new: u64,
    pub repeats: u64,
    pub missed_vblanks: u64,
    pub arrival_ms_p50: f64,
    pub arrival_ms_p90: f64,
}

fn state_label(state: &str) -> &'static str {
    match state {
        "ok" => "Stena OK",
        "degraded" => "Stena beží, ale niečo nesedí",
        _ => "Stena nie je na mostíku",
    }
}

fn reason_text(r: &Value) -> String {
    match r {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The state line: (CSS class, text).
pub fn headline(s: &StatusView) -> (String, String) {
    let mut text = state_label(&s.state).to_string();
    if !s.reasons.is_empty() {
        let reasons: Vec<String> = s.reasons.iter().map(reason_text).collect();
        text.push_str(" — ");
        text.push_str(&reasons.join(", "));
    }
    (s.state.clone(), text)
}

/// The table, in display order: (label, value). A multi-line value uses `\n`.
pub fn rows(s: &StatusView) -> Vec<(String, String)> {
    let d = &s.display;
    let snd = &s.sender;
    let p = &s.pacing;
    let mut out = vec![
        (
            "Displej",
            if d.target_acquired {
                format!("{}×{} @ {:.3} Hz", d.width, d.height, d.hz)
            } else {
                "nezískaný".to_string()
            },
        ),
        (
            "Mimo desktopu",
            if d.specialized { "áno" } else { "nie" }.to_string(),
        ),
        (
            "Zdroj (Spout)",
            if snd.connected {
                format!("{} — pripojený {}×{}", snd.name, snd.width, snd.height)
            } else {
                format!("{} — chýba", snd.name)
            },
        ),
        (
            "Posledný nový obraz",
            snd.last_picture_age_ms
                .map_or("—".to_string(), |ms| format!("{ms} ms")),
        ),
        (
            "Nové / opakované (10 s)",
            format!("{} / {}", p.new, p.repeats),
        ),
        ("Zmeškané vblanky (10 s)", p.missed_vblanks.to_string()),
        (
            "Príchod p50 / p90",
            format!("{:.1} / {:.1} ms", p.arrival_ms_p50, p.arrival_ms_p90),
        ),
        (
            "Surround",
            if s.topology.matches {
                "podľa definície"
            } else {
                "nezhoduje sa"
            }
            .to_string(),
        ),
        ("Beží", format!("{} s", s.uptime_s)),
    ];
    if !s.warnings.is_empty() {
        out.push(("Upozornenia", s.warnings.join("\n")));
    }
    out.into_iter().map(|(l, v)| (l.to_string(), v)).collect()
}
