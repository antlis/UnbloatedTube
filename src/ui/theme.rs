use gpui::rgb;

pub(crate) const BG: u32 = 0x0f0f0f;
pub(crate) const PANEL: u32 = 0x161616;
pub(crate) const HOVER: u32 = 0x242424;
pub(crate) const BORDER: u32 = 0x2a2a2a;
pub(crate) const TEXT: u32 = 0xe6e6e6;
pub(crate) const MUTED: u32 = 0x8c8c8c;
pub(crate) const ACCENT: u32 = 0xff4e45;

/// Light theme on (set from the settings): `themed` swaps the palette above for `LIGHT`.
pub(crate) static LIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Light counterparts of BG, PANEL, HOVER, BORDER, TEXT, MUTED (ACCENT stays).
pub(crate) fn themed(c: u32) -> gpui::Rgba {
    let light = LIGHT.load(std::sync::atomic::Ordering::Relaxed);
    rgb(match c {
        BG if light => 0xfafafa,
        PANEL if light => 0xf0f0f0,
        HOVER if light => 0xe9e9e9,
        BORDER if light => 0xd9d9d9,
        TEXT if light => 0x1a1a1a,
        MUTED if light => 0x6b6b6b,
        c => c,
    })
}

/// Text and icons on the accent color: light in both themes.
pub(crate) const ON_ACCENT: u32 = 0xf5f5f5;
pub(crate) const ROW_H: f32 = 64.;

/// Height of the column headers (tabs on the left, window buttons on the right), border included.
pub(crate) const HEADER_H: f32 = 41.;
pub(crate) const SEEK_SEGMENTS: usize = 80;
