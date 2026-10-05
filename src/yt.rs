//! YouTube data via yt-dlp. Lists are streamed (`--flat-playlist -j`, one JSON entry per line)
//! so the UI can show them page by page. All functions block; run them off the UI thread.

use crate::account::Account;
use crate::store::Config;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::Arc;

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
    /// View count, when the listing has it (search and playlists do; channel tabs and feeds don't).
    #[serde(default)]
    pub views: Option<u64>,
    /// When it was watched ("Today", "Saturday", …) — only on YouTube history entries.
    #[serde(default)]
    pub watched: Option<String>,
}

impl Video {
    pub fn url(&self) -> String {
        format!("https://www.youtube.com/watch?v={}", self.id)
    }
    pub fn thumb_url(&self) -> String {
        format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", self.id)
    }
}

/// A top-level comment on a video.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    pub author: String,
    pub text: String,
    pub likes: Option<u64>,
    /// "2 years ago", as YouTube words it.
    pub age: String,
    pub pinned: bool,
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
    /// Channel subscriber count, when the listing has it.
    #[serde(default)]
    pub subscribers: Option<u64>,
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
    #[serde(default)]
    view_count: Option<u64>,
    #[serde(default)]
    channel_follower_count: Option<u64>,
    // Channel tabs list uploads without per-entry channel info; yt-dlp adds the listing's.
    #[serde(default)]
    playlist_channel: Option<String>,
    #[serde(default)]
    playlist_channel_id: Option<String>,
    #[serde(default)]
    thumbnails: Vec<Thumb>,
}

/// Run yt-dlp on `targets`, calling `on` for each entry as soon as yt-dlp prints it.
/// PYCRYPTODOME_DISABLE_GMP: pycryptodome otherwise hunts for libgmp via ctypes' find_library,
/// which on NixOS shells out to the C compiler and adds ~1.7s to every yt-dlp start.
fn stream(cfg: &Config, targets: &[String], limit: usize, mut on: impl FnMut(Entry)) -> Result<(), String> {
    let mut child = Command::new("yt-dlp")
        .env("PYCRYPTODOME_DISABLE_GMP", "1")
        .args(["--no-update", "--flat-playlist", "-j", "--no-warnings"])
        .args(["--playlist-end", &limit.to_string()])
        .args(cfg.cookie_args())
        .args(targets)
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

/// The top `limit` comments of a video (no replies). yt-dlp can't stream them: one JSON
/// document arrives when it is done, so `on` is called for all of them at the end.
pub fn comments(cfg: &Config, video_id: &str, limit: usize, on: &mut dyn FnMut(Comment)) -> Result<(), String> {
    #[derive(Deserialize)]
    struct Info {
        #[serde(default)]
        comments: Option<Vec<Raw>>,
    }
    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        author: Option<String>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        like_count: Option<u64>,
        #[serde(default, rename = "_time_text")]
        time_text: Option<String>,
        #[serde(default)]
        is_pinned: bool,
    }
    let out = Command::new("yt-dlp")
        .env("PYCRYPTODOME_DISABLE_GMP", "1")
        .args(["--no-update", "--no-warnings", "--skip-download", "--write-comments", "-j", "--no-playlist"])
        .args(["--extractor-args", &format!("youtube:max_comments={limit},{limit},0,0;comment_sort=top")])
        .args(cfg.cookie_args())
        .arg(format!("https://www.youtube.com/watch?v={video_id}"))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run yt-dlp: {e}"))?;
    if !out.status.success() {
        return Err(short_error(&String::from_utf8_lossy(&out.stderr)));
    }
    let info: Info = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    for c in info.comments.unwrap_or_default() {
        on(Comment {
            author: c.author.unwrap_or_default(),
            text: c.text.unwrap_or_default(),
            likes: c.like_count,
            age: c.time_text.unwrap_or_default(),
            pinned: c.is_pinned,
        });
    }
    Ok(())
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
        views: e.view_count,
        watched: None,
    })
}

fn videos(cfg: &Config, target: &str, limit: usize, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    stream(cfg, &[target.to_string()], limit, |e| to_video(e).into_iter().for_each(&mut *on))
}

fn thumb(e: &Entry) -> Option<String> {
    let url = &e.thumbnails.first()?.url;
    Some(if url.starts_with("//") { format!("https:{url}") } else { url.clone() })
}

/// Subscribed channels, in YouTube's order (the UI sorts them once complete).
pub fn subscriptions(cfg: &Config, on: &mut dyn FnMut(Group)) -> Result<(), String> {
    stream(cfg, &["https://www.youtube.com/feed/channels".to_string()], 5000, |e| {
        let thumb = thumb(&e);
        let (Some(id), Some(url)) = (e.id, e.channel_url.or(e.url)) else { return };
        let title = e.title.map(|t| t.trim().to_string()).unwrap_or_else(|| url.clone());
        on(Group { id, title, url: format!("{url}/videos"), thumb, subscribers: e.channel_follower_count });
    })
}

/// Latest uploads of all subscribed channels, newest first.
pub fn feed(cfg: &Config, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    videos(cfg, ":ytsubs", 150, on)
}

/// The login-gated path works: one entry from the subscriptions feed. Used by the
/// Connect panel's check, so it must fail loudly when the cookies are not accepted.
pub fn auth_probe(cfg: &Config) -> Result<(), String> {
    stream(cfg, &[":ytsubs".to_string()], 1, |_| {}).map(drop)
}

/// YouTube's watch history, newest first, with the day of each entry. Asked of YouTube
/// directly: yt-dlp's `:ythistory` leaves out Shorts and some recent entries. Falls back to
/// yt-dlp when the account can't be reached that way.
pub fn history(cfg: &Config, account: Option<Arc<Account>>, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    let list = match account {
        Some(a) => a.history(150),
        None => Account::load(cfg).and_then(|a| a.history(150)),
    };
    match list {
        Ok(list) if !list.is_empty() => {
            list.into_iter().for_each(on);
            Ok(())
        }
        _ => videos(cfg, ":ythistory", 150, on),
    }
}

pub fn recommendations(cfg: &Config, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    videos(cfg, ":ytrec", 60, on)
}

/// Fallback "recommendations" without login: latest uploads of the given channel.
pub fn channel_uploads(cfg: &Config, channel_url: &str, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    videos(cfg, &format!("{}/videos", channel_url.trim_end_matches('/')), 40, on)
}

/// The logged-out home: a shuffled mix of random topic searches. YouTube's own home feed,
/// trending and popular pages all require a login, so anonymous sessions get this instead.
pub fn anonymous(cfg: &Config, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    const TOPICS: &[&str] = &[
        "official music video",
        "video game gameplay",
        "space science documentary",
        "cooking recipe",
        "funny moments compilation",
        "football highlights",
        "tech review",
        "nature wildlife",
        "travel vlog",
        "car review",
        "history documentary",
        "stand up comedy",
        "diy woodworking",
        "movie trailer",
        "coding tutorial",
        "art timelapse",
    ];
    // No rand dependency: a xorshift seeded from the process's (randomized) hash state.
    let mut state = {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        let mut h = RandomState::new().build_hasher();
        h.write_u64(std::process::id() as u64);
        h.finish() | 1
    };
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    // Five distinct topics, picked by partial Fisher-Yates.
    let mut pool: Vec<&str> = TOPICS.to_vec();
    for i in 0..5 {
        let j = i + (next() as usize % (pool.len() - i));
        pool.swap(i, j);
    }
    let targets: Vec<String> = pool[..5].iter().map(|t| format!("ytsearch15:{t}")).collect();
    let mut found: Vec<Video> = Vec::new();
    stream(cfg, &targets, 20, |e| {
        if let Some(v) = to_video(e) {
            if !found.iter().any(|w| w.id == v.id) {
                found.push(v);
            }
        }
    })?;
    // Interleave the topics: collect first, so the rows aren't one topic's block at a time.
    for i in (1..found.len()).rev() {
        found.swap(i, (next() as usize) % (i + 1));
    }
    found.into_iter().for_each(&mut *on);
    Ok(())
}

/// A link to one YouTube video, with the start time the link carries.
pub struct VideoLink {
    pub id: String,
    pub start: Option<f64>,
}

/// The video a pasted YouTube link points to: `watch?v=`, `youtu.be/`, `/shorts/`, `/live/`,
/// `/embed/` (with or without the scheme), and a start time from `t=` / `start=`.
pub fn parse_video_link(text: &str) -> Option<VideoLink> {
    let text = text.trim();
    if text.contains(char::is_whitespace) {
        return None;
    }
    let rest = text.strip_prefix("https://").or_else(|| text.strip_prefix("http://")).unwrap_or(text);
    let (host, path_query) = rest.split_once('/')?;
    let host = host
        .strip_prefix("www.")
        .or_else(|| host.strip_prefix("m."))
        .or_else(|| host.strip_prefix("music."))
        .unwrap_or(host);
    let (path_query, fragment) = path_query.split_once('#').unwrap_or((path_query, ""));
    let (path, query) = path_query.split_once('?').unwrap_or((path_query, ""));
    let param = |key: &str| query.split('&').filter_map(|kv| kv.split_once('=')).find(|(k, _)| *k == key).map(|(_, v)| v);
    let id = match host {
        "youtu.be" => path.split('/').next(),
        "youtube.com" | "youtube-nocookie.com" => {
            let mut parts = path.split('/');
            match parts.next()? {
                "watch" => param("v"),
                "shorts" | "live" | "embed" | "v" => parts.next(),
                _ => None,
            }
        }
        _ => None,
    }?;
    if id.len() != 11 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return None;
    }
    let start = param("t").or_else(|| param("start")).or_else(|| fragment.strip_prefix("t=")).and_then(parse_offset);
    Some(VideoLink { id: id.to_string(), start: start.filter(|t| *t > 0.) })
}

/// "90", "90s" or "1h2m3s" in seconds.
fn parse_offset(s: &str) -> Option<f64> {
    if let Ok(n) = s.parse::<u64>() {
        return Some(n as f64);
    }
    let (mut total, mut num) = (0u64, String::new());
    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        let n: u64 = num.parse().ok()?;
        num.clear();
        total += n * match c {
            'h' => 3600,
            'm' => 60,
            's' => 1,
            _ => return None,
        };
    }
    num.is_empty().then_some(total as f64)
}

/// One video's details by id, for a pasted link.
pub fn video(cfg: &Config, id: &str) -> Result<Video, String> {
    let mut found = None;
    stream(cfg, &[format!("https://www.youtube.com/watch?v={id}")], 1, |e| {
        if found.is_none() {
            found = to_video(e);
        }
    })?;
    found.ok_or_else(|| "video not found".to_string())
}

pub fn search(cfg: &Config, query: &str, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    videos(cfg, &format!("ytsearch50:{query}"), 50, on)
}

pub fn group_videos(cfg: &Config, url: &str, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    videos(cfg, url, 150, on)
}

/// All of a playlist (up to YouTube's 5000): saved videos go to its end.
pub fn playlist_videos(cfg: &Config, url: &str, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    videos(cfg, url, 5000, on)
}

pub fn playlists(cfg: &Config, on: &mut dyn FnMut(Group)) -> Result<(), String> {
    on(Group { id: "WL".into(), title: "Watch later".into(), url: ":ytwatchlater".into(), thumb: None, subscribers: None });
    on(Group {
        id: "LL".into(),
        title: "Liked videos".into(),
        url: "https://www.youtube.com/playlist?list=LL".into(),
        thumb: None,
        subscribers: None,
    });
    stream(cfg, &["https://www.youtube.com/feed/playlists".to_string()], 200, |e| {
        let thumb = thumb(&e);
        let (Some(id), Some(url)) = (e.id, e.url) else { return };
        on(Group { id, title: e.title.unwrap_or_else(|| "(untitled)".into()), url, thumb, subscribers: None });
    })
}

/// 1234 → "1.2K", 265000 → "265K", 1500000 → "1.5M" (as YouTube abbreviates counts).
pub fn fmt_count(n: u64) -> String {
    let (div, unit) = match n {
        0..1_000 => return n.to_string(),
        1_000..1_000_000 => (1e3, "K"),
        1_000_000..1_000_000_000 => (1e6, "M"),
        _ => (1e9, "B"),
    };
    let v = n as f64 / div;
    if v >= 100. { format!("{v:.0}{unit}") } else { format!("{:.1}{unit}", (v * 10.).floor() / 10.).replace(".0", "") }
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
