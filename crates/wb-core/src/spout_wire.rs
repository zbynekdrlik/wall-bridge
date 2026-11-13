//! Spout's shared-memory layout: the sender list and the per-sender texture info.
//! The Windows side maps the memory; parsing lives here so it is tested on Linux.

/// Size of `SharedTextureInfo` (handle, width, height, format, usage, 256 B description, partner id).
pub const INFO_LEN: usize = 280;
/// One name slot in the `SpoutSenderNames` mapping.
pub const NAME_SLOT: usize = 256;
/// Probe boxes (8×8 px each) that make up a picture's signature.
pub const SAMPLES: u32 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureInfo {
    pub handle: u32,
    pub width: u32,
    pub height: u32,
    /// DXGI format number (87 = B8G8R8A8_UNORM).
    pub format: u32,
}

fn le_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// `None` for a short buffer or a sender that has not published a texture yet.
pub fn parse_info(b: &[u8]) -> Option<TextureInfo> {
    if b.len() < INFO_LEN {
        return None;
    }
    let i = TextureInfo {
        handle: le_u32(b, 0),
        width: le_u32(b, 4),
        height: le_u32(b, 8),
        format: le_u32(b, 12),
    };
    (i.handle != 0 && i.width != 0 && i.height != 0).then_some(i)
}

/// Names in the `SpoutSenderNames` mapping, up to the first empty slot.
pub fn parse_names(b: &[u8]) -> Vec<String> {
    b.as_chunks::<NAME_SLOT>()
        .0
        .iter()
        .take_while(|slot| slot[0] != 0)
        .map(|slot| {
            let n = slot.iter().position(|&c| c == 0).unwrap_or(NAME_SLOT);
            String::from_utf8_lossy(&slot[..n]).into_owned()
        })
        .collect()
}

/// Top-left corners of the probe boxes, as in the MVP.
pub fn sample_boxes(width: u32, height: u32) -> Vec<(u32, u32)> {
    let max_x = width.saturating_sub(8);
    let max_y = height.saturating_sub(8);
    (0..SAMPLES)
        .map(|k| {
            let x = ((k * 2 + 1) * width / (SAMPLES * 2)).min(max_x);
            let y = (((k % 4) * 2 + 1) * height / 8).min(max_y);
            (x, y)
        })
        .collect()
}

/// What a held picture showed, from its probe signature (#15): holds are content-based, so a
/// black or one-colour picture (a song's tail, black between songs) reads as a hold too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Content {
    /// Every sampled B, G, R value is at most BLACK_MAX.
    Black,
    /// One colour: each channel varies by at most SOLID_SPREAD across the samples.
    Solid,
    /// Anything else: a still image, or a moving one that froze.
    Image,
    /// No signature yet.
    Unknown,
}

/// Video black (16 in limited range) still counts as black.
const BLACK_MAX: u8 = 16;
/// Per-channel spread that still counts as one colour (compression noise).
const SOLID_SPREAD: u8 = 4;

impl Content {
    pub fn label(self) -> &'static str {
        match self {
            Content::Black => "black",
            Content::Solid => "solid",
            Content::Image => "image",
            Content::Unknown => "?",
        }
    }
}

/// Classifies a BGRA probe signature (alpha ignored).
pub fn classify_sig(sig: &[u8]) -> Content {
    if sig.len() < 4 {
        return Content::Unknown;
    }
    let mut lo = [u8::MAX; 3];
    let mut hi = [0u8; 3];
    for p in sig.as_chunks::<4>().0 {
        for c in 0..3 {
            lo[c] = lo[c].min(p[c]);
            hi[c] = hi[c].max(p[c]);
        }
    }
    if hi.iter().all(|&v| v <= BLACK_MAX) {
        Content::Black
    } else if (0..3).all(|c| hi[c] - lo[c] <= SOLID_SPREAD) {
        Content::Solid
    } else {
        Content::Image
    }
}
