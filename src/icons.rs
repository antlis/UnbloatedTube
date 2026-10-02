//! Button icons: small single-color SVGs compiled into the binary and served to gpui's `svg()`.
//! (Unicode symbols like ⏮ render as color emoji on many setups, so they can't be tinted.)

use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

macro_rules! icon {
    ($body:literal) => {
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" "#,
            r#"stroke-width="2" stroke-linecap="round" stroke-linejoin="round">"#,
            $body,
            "</svg>"
        )
    };
}

const ICONS: &[(&str, &str)] = &[
    ("play", icon!(r#"<path d="M7 4l13 8-13 8z" fill="black"/>"#)),
    ("pause", icon!(r#"<path d="M7 4h3v16H7zM14 4h3v16h-3z" fill="black"/>"#)),
    ("prev", icon!(r#"<path d="M19 20L9 12l10-8z" fill="black"/><path d="M5 19V5"/>"#)),
    ("next", icon!(r#"<path d="M5 4l10 8-10 8z" fill="black"/><path d="M19 5v14"/>"#)),
    ("back", icon!(r#"<path d="M11 19l-9-7 9-7zM22 19l-9-7 9-7z" fill="black"/>"#)),
    ("forward", icon!(r#"<path d="M13 19l9-7-9-7zM2 19l9-7-9-7z" fill="black"/>"#)),
    ("fullscreen", icon!(r#"<path d="M8 3H3v5M16 3h5v5M21 16v5h-5M3 16v5h5"/>"#)),
    ("subscribe", icon!(r#"<circle cx="9" cy="8" r="4"/><path d="M2 21v-1a5 5 0 0 1 5-5h4a5 5 0 0 1 5 5v1M19 8v6M16 11h6"/>"#)),
    ("subscribed", icon!(r#"<circle cx="9" cy="8" r="4"/><path d="M2 21v-1a5 5 0 0 1 5-5h4a5 5 0 0 1 5 5v1M16 11l2 2 4-4"/>"#)),
    ("save", icon!(r#"<path d="M3 6h12M3 12h12M3 18h8M18 14v8M14 18h8"/>"#)),
    ("like", icon!(r#"<path d="M7 11v10H3V11zM7 11l4-8c1.5 0 2.5 1 2.5 2.5L13 9h6a2 2 0 0 1 2 2.3l-1.3 7.7a2 2 0 0 1-2 1.9H7"/>"#)),
    ("liked", icon!(r#"<path d="M7 11v10H3V11zM7 11l4-8c1.5 0 2.5 1 2.5 2.5L13 9h6a2 2 0 0 1 2 2.3l-1.3 7.7a2 2 0 0 1-2 1.9H7z" fill="black"/>"#)),
    ("share", icon!(r#"<path d="M4 12v7a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-7M16 6l-4-4-4 4M12 2v13"/>"#)),
    ("new", icon!(r#"<path d="M12 3l2.6 5.6 6.1.7-4.5 4.2 1.2 6L12 16.6l-5.4 2.9 1.2-6-4.5-4.2 6.1-.7z"/>"#)),
    ("download", icon!(r#"<path d="M12 3v12M7 10l5 5 5-5M4 21h16"/>"#)),
    ("dislike", icon!(r#"<g transform="rotate(180 12 12)"><path d="M7 11v10H3V11zM7 11l4-8c1.5 0 2.5 1 2.5 2.5L13 9h6a2 2 0 0 1 2 2.3l-1.3 7.7a2 2 0 0 1-2 1.9H7"/></g>"#)),
    ("disliked", icon!(r#"<g transform="rotate(180 12 12)"><path d="M7 11v10H3V11zM7 11l4-8c1.5 0 2.5 1 2.5 2.5L13 9h6a2 2 0 0 1 2 2.3l-1.3 7.7a2 2 0 0 1-2 1.9H7z" fill="black"/></g>"#)),
    ("minimize", icon!(r#"<path d="M5 12h14"/>"#)),
    ("maximize", icon!(r#"<rect x="5" y="5" width="14" height="14" rx="1"/>"#)),
    ("close", icon!(r#"<path d="M6 6l12 12M18 6L6 18"/>"#)),
    ("refresh", icon!(r#"<path d="M21 12a9 9 0 1 1-2.6-6.4L21 8M21 3v5h-5"/>"#)),
    ("settings", icon!(r#"<path d="M3 6h11M18 6h3M3 12h3M10 12h11M3 18h13M20 18h1"/><circle cx="16" cy="6" r="2"/><circle cx="8" cy="12" r="2"/><circle cx="18" cy="18" r="2"/>"#)),
];

/// Path for `svg().path(..)`.
pub fn path(name: &str) -> SharedString {
    format!("icons/{name}.svg").into()
}

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let name = path.strip_prefix("icons/").and_then(|p| p.strip_suffix(".svg"));
        Ok(ICONS.iter().find(|(n, _)| Some(*n) == name).map(|(_, svg)| Cow::Borrowed(svg.as_bytes())))
    }

    fn list(&self, _path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(ICONS.iter().map(|(n, _)| path(n)).collect())
    }
}
