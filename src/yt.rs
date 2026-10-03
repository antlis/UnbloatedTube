//! YouTube data via yt-dlp. Lists are streamed (`--flat-playlist -j`, one JSON entry per line)
//! so the UI can show them page by page. All functions block; run them off the UI thread.

use crate::store::Config;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Video {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub channel_url: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    /// A YouTube Short (vertical, under a minute).
    #[serde(default)]
    pub short: bool,
}

impl Video {
    pub fn url(&self) -> String {
        format!("https://www.youtube.com/watch?v={}", self.id)
    }
    pub fn thumb_url(&self) -> String {
        format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", self.id)
    }
}

/// A subscribed channel or a playlist: a title plus the URL listing its videos.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub title: String,
    pub url: String,
    /// Channel avatar / playlist cover.
    #[serde(default)]
    pub thumb: Option<String>,
}

#[derive(Deserialize)]
struct Thumb {
    url: String,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    uploader: Option<String>,
    #[serde(default)]
    channel_url: Option<String>,
    #[serde(default)]
    uploader_url: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
    // Channel tabs list uploads without per-entry channel info; yt-dlp adds the listing's.
    #[serde(default)]
    playlist_channel: Option<String>,
    #[serde(default)]
    playlist_channel_id: Option<String>,
    #[serde(default)]
    thumbnails: Vec<Thumb>,
}

/// Run yt-dlp on `target`, calling `on` for each entry as soon as yt-dlp prints it.
/// PYCRYPTODOME_DISABLE_GMP: pycryptodome otherwise hunts for libgmp via ctypes' find_library,
/// which on NixOS shells out to the C compiler and adds ~1.7s to every yt-dlp start.
fn stream(cfg: &Config, target: &str, limit: usize, mut on: impl FnMut(Entry)) -> Result<(), String> {
    let mut child = Command::new("yt-dlp")
        .env("PYCRYPTODOME_DISABLE_GMP", "1")
        .args(["--no-update", "--flat-playlist", "-j", "--no-warnings"])
        .args(["--playlist-end", &limit.to_string()])
        .args(cfg.cookie_args())
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run yt-dlp: {e}"))?;
    let mut any = false;
    for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
        let Ok(mut e) = serde_json::from_str::<Entry>(&line) else { continue };
        e.channel = e.channel.take().or(e.playlist_channel.take());
        e.channel_url = e
            .channel_url
            .take()
            .or_else(|| e.playlist_channel_id.take().map(|id| format!("https://www.youtube.com/channel/{id}")));
        any = true;
        on(e);
    }
    let mut err = String::new();
    let _ = child.stderr.take().unwrap().read_to_string(&mut err);
    let ok = child.wait().map_err(|e| e.to_string())?.success();
    if ok || any { Ok(()) } else { Err(short_error(&err)) }
}

fn short_error(stderr: &str) -> String {
    let line = stderr.lines().rev().find(|l| l.starts_with("ERROR")).unwrap_or(stderr.trim());
    let line = line.trim_start_matches("ERROR: ");
    // Drop the "[extractor] " prefix and yt-dlp's long help text after the first sentence.
    let line = line.split_once("] ").map_or(line, |(_, rest)| rest);
    line.split(". ").next().unwrap_or(line).to_string()
}

fn to_video(e: Entry) -> Option<Video> {
    let id = e.id?;
    // Shorts/channels/playlists also show up in feeds; keep plain 11-char video ids.
    if id.len() != 11 {
        return None;
    }
    Some(Video {
        short: e.url.as_deref().is_some_and(|u| u.contains("/shorts/")),
        title: e.title.unwrap_or_else(|| id.clone()),
        id,
        channel: e.channel.or(e.uploader),
        channel_url: e.channel_url.or(e.uploader_url),
        duration: e.duration,
    })
}

fn videos(cfg: &Config, target: &str, limit: usize, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    stream(cfg, target, limit, |e| to_video(e).into_iter().for_each(&mut *on))
}

fn thumb(e: &Entry) -> Option<String> {
    let url = &e.thumbnails.first()?.url;
    Some(if url.starts_with("//") { format!("https:{url}") } else { url.clone() })
}

/// Subscribed channels, in YouTube's order (the UI sorts them once complete).
pub fn subscriptions(cfg: &Config, on: &mut dyn FnMut(Group)) -> Result<(), String> {
    stream(cfg, "https://www.youtube.com/feed/channels", 5000, |e| {
        let thumb = thumb(&e);
        let (Some(id), Some(url)) = (e.id, e.channel_url.or(e.url)) else { return };
        let title = e.title.map(|t| t.trim().to_string()).unwrap_or_else(|| url.clone());
        on(Group { id, title, url: format!("{url}/videos"), thumb });
    })
}

/// Latest uploads of all subscribed channels, newest first.
pub fn feed(cfg: &Config, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    videos(cfg, ":ytsubs", 150, on)
}

pub fn history(cfg: &Config, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    videos(cfg, ":ythistory", 150, on)
}

pub fn recommendations(cfg: &Config, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    videos(cfg, ":ytrec", 60, on)
}

/// Fallback "recommendations" without login: latest uploads of the given channel.
pub fn channel_uploads(cfg: &Config, channel_url: &str, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    videos(cfg, &format!("{}/videos", channel_url.trim_end_matches('/')), 40, on)
}

pub fn search(cfg: &Config, query: &str, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    videos(cfg, &format!("ytsearch50:{query}"), 50, on)
}

pub fn group_videos(cfg: &Config, url: &str, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    videos(cfg, url, 150, on)
}

pub fn playlists(cfg: &Config, on: &mut dyn FnMut(Group)) -> Result<(), String> {
    on(Group { id: "WL".into(), title: "Watch later".into(), url: ":ytwatchlater".into(), thumb: None });
    on(Group {
        id: "LL".into(),
        title: "Liked videos".into(),
        url: "https://www.youtube.com/playlist?list=LL".into(),
        thumb: None,
    });
    stream(cfg, "https://www.youtube.com/feed/playlists", 200, |e| {
        let thumb = thumb(&e);
        let (Some(id), Some(url)) = (e.id, e.url) else { return };
        on(Group { id, title: e.title.unwrap_or_else(|| "(untitled)".into()), url, thumb });
    })
}

pub fn fmt_duration(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// Download a video into `dir`, reporting progress (0–100) as yt-dlp prints it.
/// Returns the final file path.
pub fn download(cfg: &Config, url: &str, format: &str, dir: &std::path::Path, mut progress: impl FnMut(f32)) -> Result<String, String> {
    let mut child = Command::new("yt-dlp")
        .env("PYCRYPTODOME_DISABLE_GMP", "1")
        // --print implies --quiet, which would hide progress; --progress brings it back.
        .args(["--no-update", "--no-warnings", "--newline", "--progress", "--no-playlist", "-f", format])
        .args(["--progress-template", "download:PROGRESS %(progress._percent_str)s"])
        .args(["--print", "after_move:FILE %(filepath)s"])
        .arg("-P")
        .arg(dir)
        .args(["-o", "%(title)s [%(id)s].%(ext)s"])
        .args(cfg.cookie_args())
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run yt-dlp: {e}"))?;
    let mut file = None;
    for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
        if let Some(p) = line.strip_prefix("PROGRESS ") {
            if let Ok(p) = p.trim().trim_end_matches('%').parse() {
                progress(p);
            }
        } else if let Some(f) = line.strip_prefix("FILE ") {
            file = Some(f.to_string());
        }
    }
    let mut err = String::new();
    let _ = child.stderr.take().unwrap().read_to_string(&mut err);
    let ok = child.wait().map_err(|e| e.to_string())?.success();
    match file {
        Some(f) if ok => Ok(f),
        _ => Err(short_error(&err)),
    }
}
