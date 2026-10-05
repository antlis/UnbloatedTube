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
    ("cc", icon!(r#"<rect x="2" y="5" width="20" height="14" rx="2"/><path d="M10 10a2.5 2.5 0 1 0 0 4M17 10a2.5 2.5 0 1 0 0 4"/>"#)),
("cc-off", icon!(r#"<rect x="2" y="5" width="20" height="14" rx="2"/><path d="M10 10a2.5 2.5 0 1 0 0 4M17 10a2.5 2.5 0 1 0 0 4M3 3l18 18"/>"#)),
("download", icon!(r#"<path d="M12 3v12M7 10l5 5 5-5M4 21h16"/>"#)),
    ("dislike", icon!(r#"<g transform="rotate(180 12 12)"><path d="M7 11v10H3V11zM7 11l4-8c1.5 0 2.5 1 2.5 2.5L13 9h6a2 2 0 0 1 2 2.3l-1.3 7.7a2 2 0 0 1-2 1.9H7"/></g>"#)),
    ("disliked", icon!(r#"<g transform="rotate(180 12 12)"><path d="M7 11v10H3V11zM7 11l4-8c1.5 0 2.5 1 2.5 2.5L13 9h6a2 2 0 0 1 2 2.3l-1.3 7.7a2 2 0 0 1-2 1.9H7z" fill="black"/></g>"#)),
    ("minimize", icon!(r#"<path d="M5 12h14"/>"#)),
    ("maximize", icon!(r#"<rect x="5" y="5" width="14" height="14" rx="1"/>"#)),
    ("add", icon!(r#"<path d="M12 5v14M5 12h14"/>"#)),
    ("bell", icon!(r#"<path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9M10.3 21a1.9 1.9 0 0 0 3.4 0"/>"#)),
    ("muted", icon!(r#"<path d="M3 3l18 18M10.6 10.6a2 2 0 0 0 2.8 2.8M9.9 5.1A10.4 10.4 0 0 1 12 5c7 0 10 7 10 7a13 13 0 0 1-1.7 2.7M6.6 6.6A13.5 13.5 0 0 0 2 12s3 7 10 7a9.7 9.7 0 0 0 5.4-1.6"/>"#)),
    ("search", icon!(r#"<circle cx="11" cy="11" r="7"/><path d="M21 21l-4.3-4.3"/>"#)),
    ("arrow-left", icon!(r#"<path d="M19 12H5M12 19l-7-7 7-7"/>"#)),
    ("recent", icon!(r#"<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>"#)),
    ("watch-later", icon!(r#"<circle cx="11" cy="13" r="8"/><path d="M11 9v4l3 2M19 3v6M16 6h6"/>"#)),
    ("check", icon!(r#"<path d="M5 12l5 5L20 7"/>"#)),
    ("folder", icon!(r#"<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>"#)),
    ("pip", icon!(r#"<rect x="3" y="5" width="18" height="14" rx="2"/><rect x="12" y="11" width="7" height="6" rx="1" fill="black"/>"#)),
    ("trash", icon!(r#"<path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/>"#)),
    ("close", icon!(r#"<path d="M6 6l12 12M18 6L6 18"/>"#)),
    ("refresh", icon!(r#"<path d="M21 12a9 9 0 1 1-2.6-6.4L21 8M21 3v5h-5"/>"#)),
    ("volume", icon!(r#"<path d="M11 5L6 9H2v6h4l5 4zM15.5 8.5a5 5 0 0 1 0 7M19 5a10 10 0 0 1 0 14"/>"#)),
    ("volume-off", icon!(r#"<path d="M11 5L6 9H2v6h4l5 4zM22 9l-6 6M16 9l6 6"/>"#)),
    ("browser", icon!(r#"<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/>"#)),
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
