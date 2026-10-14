//! How the app signs in to YouTube: a browser session (yt-dlp reads the profile's
//! cookies) or an imported cookies.txt. The Connect panel only ever offers these two
//! choices — cookie values themselves never appear in the interface.

use crate::backend::store::config_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Where the login comes from. `resolve` picks the winner: config.toml (hand-edited,
/// deliberate) over the app's own auth.json. `Browser` keeps the raw yt-dlp spec, so
/// hand-edited values like "firefox:/path/to/profile" still round-trip. What lands in
/// auth.json is e.g. {"type":"browser","value":"firefox"}.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Auth {
    #[default]
    None,
    Browser(String),
    CookiesFile(PathBuf),
}

/// The browsers the Connect panel offers: display name, yt-dlp's `--cookies-from-browser`
/// value, and the executables looked up in PATH to detect them.
pub const BROWSERS: [(&str, &str, &[&str]); 5] = [
    ("Firefox", "firefox", &["firefox", "firefox-bin"]),
    ("Chrome", "chrome", &["google-chrome", "google-chrome-stable", "chrome"]),
    ("Chromium", "chromium", &["chromium", "chromium-browser"]),
    ("Brave", "brave", &["brave-browser", "brave"]),
    ("Edge", "edge", &["microsoft-edge", "microsoft-edge-stable"]),
];

impl Auth {
    /// The browser part of a yt-dlp spec ("firefox:/path" → "firefox",
    /// "brave+gnomekeyring" → "brave").
    fn browser_name(spec: &str) -> &str {
        spec.split([':', '+']).next().unwrap_or(spec)
    }

    /// The programs that open the configured browser (to send the user to YouTube's sign-in in the
    /// browser the login is read from). Empty for a cookies file or a profile given by path
    /// ("firefox:/path"), where which browser it is can't be told: the system's default opens it.
    pub fn browser_programs(&self) -> &'static [&'static str] {
        match self {
            Auth::Browser(spec) if !spec.contains(':') => {
                let name = Self::browser_name(spec);
                BROWSERS.iter().find(|(_, yt_dlp, _)| *yt_dlp == name).map_or(&[], |(_, _, programs)| *programs)
            }
            _ => &[],
        }
    }

    /// config.toml's `cookies_file` / `cookies_from_browser` first, then auth.json.
    pub fn resolve(cookies_file: Option<String>, cookies_from_browser: Option<String>) -> Auth {
        match (cookies_file, cookies_from_browser) {
            (Some(f), _) => Auth::CookiesFile(PathBuf::from(f)),
            (None, Some(b)) => Auth::Browser(b),
            _ => Auth::load(),
        }
    }

    /// yt-dlp's `--cookies` / `--cookies-from-browser` arguments; empty when not connected.
    pub fn cookie_args(&self) -> Vec<String> {
        match self {
            Auth::CookiesFile(p) => vec!["--cookies".into(), p.display().to_string()],
            Auth::Browser(spec) => vec!["--cookies-from-browser".into(), spec.clone()],
            Auth::None => vec![],
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Auth::None)
    }

    /// What to show after "Connected via …": the browser's display name, or the
    /// cookies file's path.
    pub fn source_label(&self) -> Option<String> {
        match self {
            Auth::Browser(spec) => Some(
                BROWSERS
                    .iter()
                    .find(|(_, s, _)| *s == Self::browser_name(spec))
                    .map_or_else(|| spec.clone(), |(name, ..)| (*name).to_string()),
            ),
            Auth::CookiesFile(p) => Some(p.display().to_string()),
            Auth::None => None,
        }
    }

    /// The default browser's yt-dlp spec: $XDG_DEFAULT_BROWSER, then `xdg-settings`
    /// (both normally give a desktop file id like "firefox.desktop").
    pub fn default_browser() -> Option<&'static str> {
        let raw = crate::platform::desktop::default_browser_id()?;
        let id = raw.strip_suffix(".desktop").unwrap_or(&raw).to_ascii_lowercase();
        // Desktop ids are usually just the executable name ("google-chrome.desktop").
        BROWSERS.iter().find(|(_, _, bins)| bins.iter().any(|b| *b == id)).map(|(_, spec, _)| *spec)
    }

    /// Is this browser's executable on PATH? (Used to mark detected browsers and to
    /// fall back when no usable default was found.)
    pub fn on_path(spec: &str) -> bool {
        let Some((.., bins)) = BROWSERS.iter().find(|(_, s, _)| *s == Self::browser_name(spec)) else {
            return false;
        };
        std::env::var_os("PATH")
            .map(|path| std::env::split_paths(&path).any(|dir| bins.iter().any(|b| dir.join(b).is_file())))
            .unwrap_or(false)
    }

    fn path() -> PathBuf {
        config_dir().join("auth.json")
    }

    /// The app's private copy of an imported cookies.txt (0600).
    pub fn imported_cookies_path() -> PathBuf {
        config_dir().join("cookies.txt")
    }

    pub fn load() -> Auth {
        std::fs::read(Self::path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(config_dir());
        if let Auth::None = self {
            Self::clear();
        } else if let Ok(json) = serde_json::to_vec(self) {
            let _ = std::fs::write(Self::path(), json);
        }
    }

    /// Forget the app-managed login: auth.json plus our imported copy of cookies.txt
    /// (never the user's original file — the copy lives in our config dir).
    pub fn clear() {
        let _ = std::fs::remove_file(Self::path());
        let _ = std::fs::remove_file(Self::imported_cookies_path());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_configured_browser_opens_the_sign_in() {
        assert_eq!(Auth::Browser("brave+gnomekeyring".into()).browser_programs(), &["brave-browser", "brave"]);
        assert_eq!(Auth::Browser("firefox".into()).browser_programs(), &["firefox", "firefox-bin"]);
        // A profile by path could be any Firefox-based browser (Zen, LibreWolf): the default opens it.
        assert!(Auth::Browser("firefox:/home/x/.config/zen/profile".into()).browser_programs().is_empty());
        assert!(Auth::Browser("vivaldi".into()).browser_programs().is_empty());
        assert!(Auth::None.browser_programs().is_empty());
        assert!(Auth::CookiesFile("/tmp/c.txt".into()).browser_programs().is_empty());
    }
}
