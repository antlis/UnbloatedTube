//! Config (~/.config/unbloated-youtube/config.toml) and local watch history (~/.local/share/unbloated-youtube/history.json).

use crate::yt::Video;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Config {
    /// Passed to yt-dlp `--cookies-from-browser`, e.g. "firefox" or "firefox:/path/to/profile".
    pub cookies_from_browser: Option<String>,
    /// Netscape cookies.txt, passed to yt-dlp `--cookies`.
    pub cookies_file: Option<String>,
}

impl Config {
    pub fn load() -> Self {
        let path = config_dir().join("config.toml");
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| toml::from_str(&s).map_err(|e| eprintln!("{}: {e}", path.display())).ok())
            .unwrap_or_default()
    }

    pub fn cookie_args(&self) -> Vec<String> {
        match (&self.cookies_file, &self.cookies_from_browser) {
            (Some(f), _) => vec!["--cookies".into(), f.clone()],
            (None, Some(b)) => vec!["--cookies-from-browser".into(), b.clone()],
            _ => vec![],
        }
    }

    pub fn has_auth(&self) -> bool {
        self.cookies_file.is_some() || self.cookies_from_browser.is_some()
    }
}

/// The app used to be called jtube: move its folders over once so settings, login config,
/// history and caches survive the rename.
pub fn migrate_old_dirs() {
    for base in [dirs::config_dir(), dirs::data_dir(), dirs::cache_dir()].into_iter().flatten() {
        let (old, new) = (base.join("jtube"), base.join("unbloated-youtube"));
        if old.is_dir() && !new.exists() {
            let _ = std::fs::rename(old, new);
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("unbloated-youtube")
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("unbloated-youtube")
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(|| PathBuf::from(".")).join("unbloated-youtube")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Watched {
    pub video: Video,
    /// Seconds into the video where playback stopped.
    pub position: f64,
    /// Watched to the end.
    #[serde(default)]
    pub finished: bool,
}

/// Most recent first.
#[derive(Default, Serialize, Deserialize)]
pub struct History {
    pub items: Vec<Watched>,
}

impl History {
    fn path() -> PathBuf {
        data_dir().join("history.json")
    }

    pub fn load() -> Self {
        std::fs::read(Self::path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(data_dir());
        if let Ok(json) = serde_json::to_vec(self) {
            let _ = std::fs::write(Self::path(), json);
        }
    }

    pub fn last(&self) -> Option<&Watched> {
        self.items.first()
    }

    pub fn position(&self, id: &str) -> f64 {
        self.items.iter().find(|w| w.video.id == id).map_or(0.0, |w| w.position)
    }

    /// Move `video` to the top, keeping a known resume position.
    pub fn touch(&mut self, video: &Video) {
        let position = self.position(&video.id);
        self.items.retain(|w| w.video.id != video.id);
        let finished = self.is_finished(&video.id);
        self.items.insert(0, Watched { video: video.clone(), position, finished });
        self.items.truncate(1000);
    }

    pub fn set_position(&mut self, id: &str, position: f64, finished: bool) {
        if let Some(w) = self.items.iter_mut().find(|w| w.video.id == id) {
            w.position = position;
            w.finished = finished;
        }
    }

    pub fn is_finished(&self, id: &str) -> bool {
        self.items.iter().any(|w| w.video.id == id && w.finished)
    }
}

/// Small app state kept in the data dir (e.g. the Up next queue).
pub fn load_data<T: DeserializeOwned>(name: &str) -> Option<T> {
    serde_json::from_slice(&std::fs::read(data_dir().join(format!("{name}.json"))).ok()?).ok()
}

pub fn save_data<T: Serialize>(name: &str, value: &T) {
    let _ = std::fs::create_dir_all(data_dir());
    if let Ok(json) = serde_json::to_vec(value) {
        let _ = std::fs::write(data_dir().join(format!("{name}.json")), json);
    }
}

fn list_path(key: &str) -> PathBuf {
    cache_dir().join("lists").join(format!("{key}.json"))
}

/// Last complete copy of a list (subscriptions, history, …), shown instantly on startup.
pub fn load_list<T: DeserializeOwned>(key: &str) -> Option<Vec<T>> {
    serde_json::from_slice(&std::fs::read(list_path(key)).ok()?).ok()
}

pub fn save_list<T: Serialize>(key: &str, items: &[T]) {
    let path = list_path(key);
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    if let Ok(json) = serde_json::to_vec(items) {
        let _ = std::fs::write(path, json);
    }
}

/// In-app preferences, written by unbloated-youtube itself (~/.config/unbloated-youtube/settings.json).
/// Kept apart from config.toml so saving never rewrites the user's hand-edited file.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub subscriptions: bool,
    pub playlists: bool,
    pub history: bool,
    pub recommendations: bool,
    pub shorts: bool,
    /// Vim-style keys: j/k move a selection in lists, f shows click hints.
    pub vim: bool,
    /// Desktop notifications for uploads of channels with the bell on.
    pub notifications: bool,
    /// How often to check for new uploads, in minutes.
    pub notify_minutes: u32,
    /// Draw minimize / maximize / close, for desktops (or tiling WMs) without a title bar.
    pub window_buttons: bool,
    pub subscribe_button: bool,
    pub save_button: bool,
    pub like_button: bool,
    pub dislike_button: bool,
    pub share_button: bool,
    pub download_button: bool,
    /// Player
    pub autoplay: bool,
    pub max_quality: u32,
    /// Skip AV1, which many GPUs can't decode in hardware.
    pub prefer_hw_codecs: bool,
    pub hwdec: bool,
    pub speed: f32,
    pub sponsorblock: bool,
    /// SponsorBlock segment categories to skip (its API names, e.g. "sponsor", "selfpromo").
    pub skip_segments: Vec<String>,
    pub audio_only: bool,
    /// Subtitle language code (e.g. "en"); empty = no subtitles.
    pub sub_lang: String,
    /// Extra mpv command-line options, space separated.
    pub mpv_args: String,
    /// Where downloads go; empty = the system Downloads folder. `~/` is expanded.
    pub download_dir: String,
    /// Left column width, as a fraction of the window.
    pub split: f32,
    /// Player height in the right column, as a fraction of the window.
    pub player: f32,
    /// Height of History's Continue watching list, in pixels.
    pub continue_height: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            subscriptions: true,
            playlists: true,
            history: true,
            recommendations: true,
            shorts: true,
            vim: false,
            notifications: true,
            notify_minutes: 15,
            window_buttons: true,
            subscribe_button: true,
            save_button: true,
            like_button: false,
            dislike_button: false,
            share_button: true,
            download_button: true,
            autoplay: true,
            max_quality: 1080,
            // On by default: software-decoding 1080p AV1/VP9 stutters on many laptops, and
            // auto-safe falls back to the CPU when the GPU can't decode a video.
            prefer_hw_codecs: true,
            hwdec: true,
            speed: 1.0,
            sponsorblock: false,
            skip_segments: ["sponsor", "selfpromo", "interaction"].map(String::from).to_vec(),
            audio_only: false,
            sub_lang: String::new(),
            mpv_args: String::new(),
            download_dir: String::new(),
            split: 0.5,
            player: 0.62,
            continue_height: 4. * crate::ROW_H,
        }
    }
}

impl Settings {
    fn path() -> PathBuf {
        config_dir().join("settings.json")
    }

    pub fn load() -> Self {
        std::fs::read(Self::path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(config_dir());
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(Self::path(), json);
        }
    }
}

/// Videos you've seen in a channel's list or played, for the "new" counts on Subscriptions.
#[derive(Default, Serialize, Deserialize)]
pub struct Seen {
    pub ids: std::collections::HashSet<String>,
    /// Set once the first feed has been recorded, so a fresh install doesn't show everything as new.
    #[serde(default)]
    pub baseline: bool,
}

impl Seen {
    fn path() -> PathBuf {
        data_dir().join("seen.json")
    }

    pub fn load() -> Self {
        std::fs::read(Self::path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(data_dir());
        if let Ok(json) = serde_json::to_vec(self) {
            let _ = std::fs::write(Self::path(), json);
        }
    }
}

/// A user-made group of channels ("Music", "Tech", …), kept only in this app.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ChannelGroup {
    pub name: String,
    /// Channel ids (or handles, for channels without a known id).
    pub channels: Vec<String>,
}

/// Per-channel switches (by channel id): muted in New uploads, notify on upload.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ChannelFlags {
    #[serde(default)]
    pub muted: std::collections::HashSet<String>,
    #[serde(default)]
    pub notify: std::collections::HashSet<String>,
    /// Videos already notified about, so restarts don't repeat them.
    #[serde(default)]
    pub notified: std::collections::HashSet<String>,
}
