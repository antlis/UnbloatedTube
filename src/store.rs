//! Config (~/.config/unbloated-youtube/config.toml) and local watch history (~/.local/share/unbloated-youtube/history.json).

use crate::auth::Auth;
use crate::yt::Video;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Config {
    /// Passed to yt-dlp `--cookies-from-browser`, e.g. "firefox" or "firefox:/path/to/profile".
    pub cookies_from_browser: Option<String>,
    /// Netscape cookies.txt, passed to yt-dlp `--cookies`.
    pub cookies_file: Option<String>,
    /// Where the login actually comes from: config.toml's two fields above, or auth.json
    /// written by the Connect panel. Resolved in `load`, never read from config.toml itself.
    #[serde(skip)]
    pub auth: Auth,
    /// Places to send the playing video to: `[cast.<name>]` tables with a `command`.
    #[serde(default)]
    pub cast: std::collections::BTreeMap<String, CastTarget>,
}

/// A cast target: either a `url` (+ `token`) of a receiver that speaks the tg-mpv-bot remote API,
/// which the app then also controls, or a `command` to run with `{url}`, `{start}`, `{id}` and
/// `{title}` filled in (see `cast.rs`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct CastTarget {
    pub command: Vec<String>,
    pub url: Option<String>,
    pub token: Option<String>,
}

impl Config {
    pub fn load() -> Self {
        let path = config_dir().join("config.toml");
        let mut cfg: Self = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| toml::from_str(&s).map_err(|e| eprintln!("{}: {e}", path.display())).ok())
            .unwrap_or_default();
        cfg.auth = Auth::resolve(cfg.cookies_file.clone(), cfg.cookies_from_browser.clone());
        cfg
    }

    pub fn cookie_args(&self) -> Vec<String> {
        self.auth.cookie_args()
    }

    pub fn has_auth(&self) -> bool {
        !self.auth.is_none()
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

/// Comment out config.toml's login lines (everything else in the file, including other
/// comments, is left untouched; uncomment to log back in). Ok(false) = no login lines.
pub fn disable_login_in_config() -> Result<bool, String> {
    let path = config_dir().join("config.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut changed = false;
    let mut out = text
        .lines()
        .map(|line| {
            let key = line.trim_start().split('=').next().unwrap_or("").trim();
            if key == "cookies_from_browser" || key == "cookies_file" {
                changed = true;
                format!("# {line}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if changed {
        if text.ends_with('\n') {
            out.push('\n');
        }
        std::fs::write(&path, out).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(changed)
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
    /// Chapters tab under the player, for videos that have chapters.
    pub chapters: bool,
    /// Description tab under the player; the description is fetched when the tab is opened.
    pub description: bool,
    /// Transcript tab under the player: the captions as text, searchable, a click jumps there.
    pub transcript: bool,
    /// Comments tab under the player; comments are fetched only when the tab is opened.
    pub comments: bool,
    /// Downloads tab on the left: the videos saved with Download, played from the file.
    pub downloads_tab: bool,
    /// Watch later tab under the player; the list is fetched when the tab is first opened.
    pub watch_later_tab: bool,
    pub shorts: bool,
    /// Vim-style keys: j/k move a selection in lists, f shows click hints.
    pub vim: bool,
    /// Desktop notifications for uploads of channels with the bell on.
    pub notifications: bool,
    /// How often to check for new uploads, in minutes.
    pub notify_minutes: u32,
    /// Draw minimize / maximize / close, for desktops (or tiling WMs) without a title bar.
    pub window_buttons: bool,
    pub light_theme: bool,
    pub subscribe_button: bool,
    pub save_button: bool,
    pub like_button: bool,
    pub dislike_button: bool,
    pub watch_later_button: bool,
    pub share_button: bool,
    pub subtitles_button: bool,
    pub cast_button: bool,
    pub share_time_button: bool,
    pub browser_button: bool,
    /// Back / forward buttons through the videos played (first in the row under the progress bar).
    pub history_buttons: bool,
    pub download_button: bool,
    /// Video info: view counts, upload date, channel subscriber counts.
    pub show_views: bool,
    pub show_date: bool,
    pub show_subs: bool,
    /// Player
    pub autoplay: bool,
    pub max_quality: u32,
    /// Skip AV1, which many GPUs can't decode in hardware.
    pub prefer_hw_codecs: bool,
    pub hwdec: bool,
    /// Resolve videos before they are played (the one under the pointer, the next one).
    pub prefetch: bool,
    /// mpv's own on-screen controls and key bindings over the video (off: only the app's).
    pub native_controls: bool,
    /// The app's own control bar, drawn by mpv over the video on hover (see controls.lua).
    pub video_controls: bool,
    pub speed: f32,
    /// Volume, 0-100.
    pub volume: f32,
    /// Volume bar next to the speed button.
    pub volume_control: bool,
    pub sponsorblock: bool,
    /// SponsorBlock segment categories to skip (its API names, e.g. "sponsor", "selfpromo").
    pub skip_segments: Vec<String>,
    pub audio_only: bool,
    /// Show subtitles on videos (the CC button turns them on and off for one video).
    pub subtitles: bool,
    /// Subtitle language code(s), e.g. "en" or "en,ru"; empty = the system language.
    pub sub_lang: String,
    /// Command that casts the video (see `cast.rs`), e.g. `catt -d "Living Room" cast {url}`; when
    /// set it replaces the `[cast.*]` targets of config.toml.
    pub cast_command: String,
    /// Also use YouTube's automatic (and translated) captions when a video has none of its own.
    pub sub_auto: bool,
    /// Subtitle size, as mpv's `sub-scale` (1 = normal).
    pub sub_scale: f32,
    /// Extra mpv command-line options, space separated.
    pub mpv_args: String,
    /// Where downloads go; empty = the system Downloads folder. `~/` is expanded.
    pub download_dir: String,
    /// Comma-separated words: videos whose title says one are hidden from feeds, channels,
    /// recommendations and search (not from your own lists).
    pub hide_words: String,
    /// Left column width, as a fraction of the window.
    pub split: f32,
    /// Player height in the right column, as a fraction of the window.
    pub player: f32,
    /// Height of History's Continue watching list, in pixels.
    pub continue_height: f32,
    /// The first-run "Connect YouTube" screen was already dismissed. Missing from an
    /// existing settings.json means false, so launching logged out shows it once as the
    /// login screen; logging out sets it back to false.
    #[serde(default)]
    pub welcome_seen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            subscriptions: true,
            playlists: true,
            history: true,
            recommendations: true,
            chapters: true,
            description: true,
            transcript: true,
            comments: false,
            downloads_tab: true,
            watch_later_tab: false,
            shorts: true,
            vim: false,
            notifications: true,
            notify_minutes: 15,
            window_buttons: true,
            light_theme: false,
            subscribe_button: true,
            save_button: true,
            like_button: false,
            dislike_button: false,
            share_button: true,
            subtitles_button: true,
            cast_button: true,
            share_time_button: true,
            browser_button: true,
            history_buttons: true,
            download_button: true,
            watch_later_button: true,
            show_views: true,
            show_date: true,
            show_subs: true,
            autoplay: true,
            max_quality: 1080,
            // On by default: software-decoding 1080p AV1/VP9 stutters on many laptops, and
            // auto-safe falls back to the CPU when the GPU can't decode a video.
            prefer_hw_codecs: true,
            hwdec: true,
            prefetch: true,
            native_controls: false,
            video_controls: true,
            speed: 1.0,
            volume: 100.,
            volume_control: true,
            sponsorblock: false,
            skip_segments: ["sponsor", "selfpromo", "interaction"].map(String::from).to_vec(),
            audio_only: false,
            subtitles: false,
            sub_lang: String::new(),
            cast_command: String::new(),
            sub_auto: true,
            sub_scale: 1.0,
            mpv_args: String::new(),
            download_dir: String::new(),
            hide_words: String::new(),
            split: 0.5,
            player: 0.62,
            continue_height: 4. * crate::ROW_H,
            welcome_seen: false,
        }
    }
}


impl Settings {
    fn path() -> PathBuf {
        config_dir().join("settings.json")
    }

    pub fn load() -> Self {
        let bytes = std::fs::read(Self::path()).ok();
        let mut s: Self = bytes.as_deref().and_then(|b| serde_json::from_slice(b).ok()).unwrap_or_default();
        // Before the on/off switch existed, a subtitle language meant subtitles on.
        let has_switch = bytes
            .as_deref()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok())
            .is_some_and(|v| v.get("subtitles").is_some());
        if !has_switch && !s.sub_lang.trim().is_empty() {
            s.subtitles = true;
        }
        s
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
