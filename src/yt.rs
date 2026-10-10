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
    /// A live stream that is on now (`views` then counts the people watching).
    #[serde(default)]
    pub live: bool,
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
    concurrent_view_count: Option<u64>,
    #[serde(default)]
    is_live: Option<bool>,
    #[serde(default)]
    live_status: Option<String>,
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
    let mut child = crate::prefetch::ytdlp()
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
    let out = crate::prefetch::ytdlp()
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

/// A video's description, as its uploader wrote it; `on` gets it once (empty when there is none).
pub fn description(cfg: &Config, video_id: &str, on: &mut dyn FnMut(String)) -> Result<(), String> {
    #[derive(Deserialize)]
    struct Info {
        #[serde(default)]
        description: Option<String>,
    }
    let out = crate::prefetch::ytdlp()
        .env("PYCRYPTODOME_DISABLE_GMP", "1")
        .args(["--no-update", "--no-warnings", "--skip-download", "-j", "--no-playlist"])
        .args(cfg.cookie_args())
        .arg(format!("https://www.youtube.com/watch?v={video_id}"))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run yt-dlp: {e}"))?;
    if !out.status.success() {
        return Err(short_error(&String::from_utf8_lossy(&out.stderr)));
    }
    let info: Info = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    on(info.description.unwrap_or_default().trim().to_string());
    Ok(())
}

pub fn short_error(stderr: &str) -> String {
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
    let live = e.is_live == Some(true) || e.live_status.as_deref() == Some("is_live");
    Some(Video {
        short: e.url.as_deref().is_some_and(|u| u.contains("/shorts/")),
        title: e.title.unwrap_or_else(|| id.clone()),
        id,
        channel: e.channel.or(e.uploader),
        channel_url: e.channel_url.or(e.uploader_url),
        duration: e.duration,
        views: if live { e.concurrent_view_count } else { e.view_count },
        watched: None,
        live,
    })
}

fn videos(cfg: &Config, target: &str, limit: usize, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    let start = std::time::Instant::now();
    let r = stream(cfg, &[target.to_string()], limit, |e| to_video(e).into_iter().for_each(&mut *on));
    timing(&format!("yt-dlp {target}"), start);
    r
}

/// `UNBLOATEDTUBE_TIMING=1`: log how long each list took and which way it came, on stderr.
/// The yt-dlp runs mpv starts write to the app's stderr too (`TIMING_TO`).
pub fn timing(what: &str, start: std::time::Instant) {
    if !timing_on() {
        return;
    }
    let line = format!("unbloatedtube: {what}: {} ms\n", start.elapsed().as_millis());
    match std::env::var_os(TIMING_TO).and_then(|to| std::fs::OpenOptions::new().append(true).open(to).ok()) {
        Some(mut f) => drop(std::io::Write::write_all(&mut f, line.as_bytes())),
        None => eprint!("{line}"),
    }
}

pub const TIMING_TO: &str = "UNBLOATEDTUBE_TIMING_TO";

pub fn timing_on() -> bool {
    static ON: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| std::env::var_os("UNBLOATEDTUBE_TIMING").is_some_and(|v| v != "0"));
    *ON
}

/// A list asked of YouTube's own API first (one request per page, no yt-dlp start-up), and of
/// yt-dlp when that fails or finds nothing. Once videos arrived they are kept, never fetched twice.
fn fast_first(
    what: &str,
    ask: impl FnOnce(&mut dyn FnMut(Video)) -> Result<usize, String>,
    on: &mut dyn FnMut(Video),
    fallback: impl FnOnce(&mut dyn FnMut(Video)) -> Result<(), String>,
) -> Result<(), String> {
    let start = std::time::Instant::now();
    let mut first = None;
    let mut counted = |v: Video| {
        first.get_or_insert_with(|| start.elapsed().as_millis());
        on(v)
    };
    let r = ask(&mut counted);
    match r {
        Ok(n) if n > 0 => {
            timing(&format!("innertube {what} ({n}, first shown after {} ms)", first.unwrap_or(0)), start);
            Ok(())
        }
        r => {
            if let Err(e) = r {
                timing(&format!("innertube {what} failed ({e})"), start);
            }
            fallback(on)
        }
    }
}

/// The InnerTube browse id of a list yt-dlp would be given `url` for: the subscriptions feed and
/// playlists. Channels and anything else stay with yt-dlp.
fn browse_id(url: &str) -> Option<String> {
    match url {
        ":ytsubs" => return Some("FEsubscriptions".into()),
        ":ytwatchlater" => return Some("VLWL".into()),
        ":ytrec" => return Some("FEwhat_to_watch".into()),
        _ => {}
    }
    let list = url.strip_prefix("https://www.youtube.com/playlist?list=")?;
    let list = list.split('&').next()?;
    (!list.is_empty() && list.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')).then(|| format!("VL{list}"))
}

/// A list by its yt-dlp target, via InnerTube when it has a browse id.
fn list(cfg: &Config, target: &str, limit: usize, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    let Some(id) = browse_id(target) else { return videos(cfg, target, limit, on) };
    fast_first(
        target,
        // Nothing at all may mean kept cookies that YouTube no longer takes as a login.
        |on| crate::account::with_shared(cfg, |a| a.browse_videos(&id, limit, on).and_then(|n| if n > 0 { Ok(n) } else { Err("empty".into()) })),
        on,
        |on| videos(cfg, target, limit, on),
    )
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
    list(cfg, ":ytsubs", 150, on)
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
        None => crate::account::with_shared(cfg, |a| a.history(150)),
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
    list(cfg, ":ytrec", 60, on)
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

/// What a pasted YouTube link points to.
pub enum YtLink {
    Video(VideoLink),
    /// `url` is the channel's page without a tab, e.g. https://www.youtube.com/@mkbhd.
    Channel { url: String },
    Playlist(String),
}

/// A link split into host (without www. / m. / music.), path (no leading slash), query and fragment.
struct LinkParts<'a> {
    host: &'a str,
    path: &'a str,
    query: &'a str,
    fragment: &'a str,
}

impl LinkParts<'_> {
    fn param(&self, key: &str) -> Option<&str> {
        self.query.split('&').filter_map(|kv| kv.split_once('=')).find(|(k, _)| *k == key).map(|(_, v)| v)
    }
}

fn link_parts(text: &str) -> Option<LinkParts<'_>> {
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
    Some(LinkParts { host, path, query, fragment })
}

fn is_id_chars(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The video a pasted YouTube link points to: `watch?v=`, `youtu.be/`, `/shorts/`, `/live/`,
/// `/embed/` (with or without the scheme), and a start time from `t=` / `start=`.
pub fn parse_video_link(text: &str) -> Option<VideoLink> {
    let p = link_parts(text)?;
    let id = match p.host {
        "youtu.be" => p.path.split('/').next(),
        "youtube.com" | "youtube-nocookie.com" => {
            let mut parts = p.path.split('/');
            match parts.next()? {
                "watch" => p.param("v"),
                "shorts" | "live" | "embed" | "v" => parts.next(),
                _ => None,
            }
        }
        _ => None,
    }?;
    if id.len() != 11 || !is_id_chars(id) {
        return None;
    }
    let start = p.param("t").or_else(|| p.param("start")).or_else(|| p.fragment.strip_prefix("t=")).and_then(parse_offset);
    Some(VideoLink { id: id.to_string(), start: start.filter(|t| *t > 0.) })
}

/// A pasted YouTube link: a video (also `watch?v=…&list=…`, which plays the video), a channel
/// (`/@handle`, `/channel/UC…`, `/c/name`, `/user/name`) or a playlist (`/playlist?list=`).
pub fn parse_link(text: &str) -> Option<YtLink> {
    if let Some(video) = parse_video_link(text) {
        return Some(YtLink::Video(video));
    }
    let p = link_parts(text)?;
    if p.host != "youtube.com" {
        return None;
    }
    let mut parts = p.path.split('/');
    let (first, second) = (parts.next()?, parts.next().unwrap_or(""));
    let channel = |path: String| Some(YtLink::Channel { url: format!("https://www.youtube.com/{path}") });
    match first {
        "playlist" => p.param("list").filter(|l| l.len() >= 2 && is_id_chars(l)).map(|l| YtLink::Playlist(l.to_string())),
        "channel" if second.len() == 24 && second.starts_with("UC") && is_id_chars(second) => channel(format!("channel/{second}")),
        "c" | "user" if !second.is_empty() => channel(format!("{first}/{second}")),
        h if h.len() > 1 && h.starts_with('@') => channel(h.to_string()),
        _ => None,
    }
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

/// The captions of a video as a .vtt file in `dir` (the first of `langs` that exists), downloaded
/// by yt-dlp and cached there. mpv can't fetch them itself: YouTube answers its request with
/// HTTP 429 and only serves yt-dlp's browser-like one. `Ok(None)`: no captions in those languages.
pub fn subtitles(cfg: &Config, id: &str, langs: &str, auto: bool, dir: &std::path::Path) -> Result<Option<std::path::PathBuf>, String> {
    let langs: Vec<&str> = langs.split(',').map(str::trim).filter(|l| !l.is_empty()).collect();
    let find = || langs.iter().map(|l| dir.join(format!("{id}.{l}.vtt"))).find(|p| p.exists());
    // Rewrites YouTube's rolling auto-captions (once: a tidied file has no word times left).
    let tidy = |file: &std::path::Path| {
        if let Some(text) = std::fs::read_to_string(file).ok().as_deref().and_then(tidy_auto_captions) {
            let _ = std::fs::write(file, text);
        }
    };
    if let Some(file) = find() {
        tidy(&file);
        return Ok(Some(file));
    }
    let _ = std::fs::create_dir_all(dir);
    // Old caption files go: two weeks.
    let old = std::time::Duration::from_secs(14 * 24 * 3600);
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let aged = entry.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|e| e > old);
        if aged {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let out = crate::prefetch::ytdlp()
        .env("PYCRYPTODOME_DISABLE_GMP", "1")
        .args(["--no-update", "--no-warnings", "--skip-download", "--no-playlist", "--write-subs"])
        .args(auto.then_some("--write-auto-subs"))
        .args(["--sub-format", "vtt", "--sub-langs", &langs.join(",")])
        .arg("-o")
        .arg(dir.join("%(id)s.%(ext)s"))
        .args(cfg.cookie_args())
        .arg(format!("https://www.youtube.com/watch?v={id}"))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run yt-dlp: {e}"))?;
    if let Some(file) = find() {
        tidy(&file);
        return Ok(Some(file));
    }
    let err = String::from_utf8_lossy(&out.stderr);
    if err.contains("429") {
        Err("YouTube refused the captions (HTTP 429); signing in helps".into())
    } else {
        Ok(None)
    }
}

/// A caption track a video offers: its language code (what `subtitles` takes), YouTube's name
/// for it, and whether it is generated (speech recognition, or a machine translation of it).
#[derive(Clone, Debug)]
pub struct CaptionLang {
    pub code: String,
    pub name: String,
    pub auto: bool,
}

/// The caption tracks of a video: its own ones first, then the generated ones (the speech
/// recognition track, `xx-orig`, and its translations into every language YouTube offers).
pub fn caption_langs(cfg: &Config, id: &str) -> Result<Vec<CaptionLang>, String> {
    let out = crate::prefetch::ytdlp()
        .env("PYCRYPTODOME_DISABLE_GMP", "1")
        .args(["--no-update", "--no-warnings", "--skip-download", "-j", "--no-playlist"])
        .args(cfg.cookie_args())
        .arg(format!("https://www.youtube.com/watch?v={id}"))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run yt-dlp: {e}"))?;
    if !out.status.success() {
        return Err(short_error(&String::from_utf8_lossy(&out.stderr)));
    }
    let info: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let mut langs = Vec::new();
    for (key, auto) in [("subtitles", false), ("automatic_captions", true)] {
        let Some(tracks) = info[key].as_object() else { continue };
        let mut these: Vec<CaptionLang> = tracks
            .iter()
            // "live_chat" is a replay of a stream's chat, not captions.
            .filter(|(code, _)| *code != "live_chat")
            .map(|(code, formats)| CaptionLang {
                code: code.clone(),
                name: formats[0]["name"].as_str().filter(|n| !n.is_empty()).unwrap_or(code).to_string(),
                auto,
            })
            .collect();
        these.sort_by(|a, b| a.name.cmp(&b.name));
        langs.extend(these);
    }
    Ok(langs)
}

/// A video's captions as (start in seconds, text) lines, for the Transcript tab. The same file the
/// subtitles use (`subtitles`), so it is often there already. No lines: no captions.
pub fn transcript(cfg: &Config, id: &str, langs: &str, auto: bool, dir: &std::path::Path, on: &mut dyn FnMut((f64, String))) -> Result<(), String> {
    let Some(file) = subtitles(cfg, id, langs, auto, dir)? else { return Ok(()) };
    let vtt = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
    let mut last = String::new();
    for (start, text) in vtt_cues(&vtt) {
        // Rolling captions repeat a line in the next cue.
        if text != last {
            on((start, text.clone()));
            last = text;
        }
    }
    Ok(())
}

/// Each cue's start and its text on one line, without tags (`<i>`, `<c>`, word times) or entities.
fn vtt_cues(vtt: &str) -> Vec<(f64, String)> {
    let mut out = Vec::new();
    for block in vtt.replace("\r\n", "\n").split("\n\n") {
        let mut lines = block.lines().skip_while(|l| !l.contains("-->"));
        let Some(start) = lines.next().and_then(|t| t.split_once("-->")).and_then(|(from, _)| vtt_time(from)) else { continue };
        let mut text = String::new();
        for line in lines {
            let mut in_tag = false;
            for c in line.chars() {
                match c {
                    '<' => in_tag = true,
                    '>' if in_tag => in_tag = false,
                    c if !in_tag => text.push(c),
                    _ => {}
                }
            }
            text.push(' ');
        }
        let text = text.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&nbsp;", " ");
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !text.is_empty() {
            out.push((start, text));
        }
    }
    out
}

/// Seconds of a WebVTT timestamp, "00:01:02.345" or "01:02.345".
fn vtt_time(s: &str) -> Option<f64> {
    let parts: Vec<f64> = s.trim().split(':').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    Some(parts.iter().fold(0., |acc, p| acc * 60. + p))
}

fn vtt_stamp(t: f64) -> String {
    let ms = (t.max(0.) * 1000.).round() as u64;
    format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

/// YouTube's auto-generated captions are "rolling": every cue repeats the line before it and adds
/// a new one whose words carry their own times (`<00:00:02.601><c>details </c>`), with 40 ms cues
/// in between. Shown as is, two lines scroll and the new words appear before they are spoken.
/// This rebuilds ordinary captions from the word times: short phrases of one or two lines, each
/// shown while it is spoken. None when the file has no word times (manual captions are fine).
fn tidy_auto_captions(vtt: &str) -> Option<String> {
    if !vtt.contains("<c>") {
        return None;
    }
    // (start, end, text) per word
    let mut words: Vec<(f64, f64, String)> = Vec::new();
    for block in vtt.split("\n\n") {
        let mut lines = block.lines().skip_while(|l| !l.contains("-->"));
        let Some(timing) = lines.next() else { continue };
        let Some((from, to)) = timing.split_once("-->") else { continue };
        let (Some(start), Some(end)) = (vtt_time(from), vtt_time(to.split_whitespace().next().unwrap_or(""))) else { continue };
        // The new line is the one with word times.
        let Some(line) = lines.find(|l| l.contains("<c>")) else { continue };
        let line = line.replace("<c>", "").replace("</c>", "");
        // "from <00:00:04.206>hurricanes <00:00:04.661>and": text, then (time, text) pairs.
        let mut pieces = line.split('<');
        let lead = pieces.next().unwrap_or("");
        let mut timed: Vec<(f64, String)> = lead.split_whitespace().map(|w| (start, w.to_string())).collect();
        for piece in pieces {
            let Some((stamp, text)) = piece.split_once('>') else { continue };
            let Some(at) = vtt_time(stamp) else { continue };
            timed.extend(text.split_whitespace().map(|w| (at, w.to_string())));
        }
        for i in 0..timed.len() {
            let next = timed.get(i + 1).map_or(end, |n| n.0);
            words.push((timed[i].0, next.max(timed[i].0 + 0.05), timed[i].1.clone()));
        }
    }
    if words.is_empty() {
        return None;
    }
    // Phrases: up to ~42 characters or 5 seconds, split at pauses and sentence ends.
    let mut chunks: Vec<(f64, f64, String)> = Vec::new();
    for (start, end, word) in words {
        let extend = chunks.last().is_some_and(|(c_start, c_end, text)| {
            let sentence_end = text.ends_with(['.', '?', '!']);
            !sentence_end && start - c_end < 0.9 && end - c_start <= 5. && text.len() + 1 + word.len() <= 42
        });
        match chunks.last_mut() {
            Some(last) if extend => {
                last.1 = end;
                last.2.push(' ');
                last.2.push_str(&word);
            }
            _ => chunks.push((start, end, word)),
        }
    }
    let mut out = String::from("WEBVTT\nKind: captions\n\n");
    for i in 0..chunks.len() {
        let (start, end, text) = &chunks[i];
        // Gone shortly after the last word (but not too briefly), and before the next phrase starts.
        let mut end = (end + 0.3).max(start + 0.7);
        if let Some(next) = chunks.get(i + 1) {
            end = end.min(next.0);
        }
        let end = end.max(start + 0.1);
        // Two balanced lines for a long phrase.
        let text = if text.len() > 26 {
            let mid = text.len() / 2;
            let cut = text.char_indices().filter(|(_, c)| *c == ' ').map(|(i, _)| i).min_by_key(|i| i.abs_diff(mid));
            cut.map_or(text.clone(), |i| format!("{}\n{}", &text[..i], &text[i + 1..]))
        } else {
            text.clone()
        };
        out.push_str(&format!("{} --> {}\n{}\n\n", vtt_stamp(*start), vtt_stamp(end), text));
    }
    Some(out)
}

/// Likes and dislikes of a video from the Return YouTube Dislike API (YouTube hides dislikes; the
/// project estimates them from its users' votes). No key; the request names the video.
pub fn votes(id: &str) -> Result<(u64, u64), String> {
    let v: serde_json::Value = crate::http::agent().get(&format!("https://returnyoutubedislikeapi.com/votes?videoId={id}"))
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    match (v["likes"].as_u64(), v["dislikes"].as_u64()) {
        (Some(likes), Some(dislikes)) => Ok((likes, dislikes)),
        _ => Err("no counts for this video".into()),
    }
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
    // Signed in, results follow the account (as on the site); without a login they are anonymous.
    let ask = |on: &mut dyn FnMut(Video)| match cfg.has_auth() {
        true => crate::account::with_shared(cfg, |a| a.search_videos(query, 50, on)),
        false => Account::anonymous().search_videos(query, 50, on),
    };
    fast_first("search", ask, on, |on| videos(cfg, &format!("ytsearch50:{query}"), 50, on))
}

pub fn group_videos(cfg: &Config, url: &str, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    list(cfg, url, 150, on)
}

/// All of a playlist (up to YouTube's 5000): saved videos go to its end.
/// Put a video in the account's YouTube history, as watching it does (yt-dlp's `--mark-watched`
/// sends the same playback ping the site does). Needs the login; no download happens.
pub fn mark_watched(cfg: &Config, id: &str) -> Result<(), String> {
    let out = crate::prefetch::ytdlp()
        .env("PYCRYPTODOME_DISABLE_GMP", "1")
        .args(["--no-update", "--no-warnings", "--skip-download", "--no-playlist", "--mark-watched"])
        .args(cfg.cookie_args())
        .arg(format!("https://www.youtube.com/watch?v={id}"))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("can't run yt-dlp: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    Err(err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("yt-dlp failed").to_string())
}

/// The first `n` videos of a playlist (to cast it without opening it).
pub fn playlist_head(cfg: &Config, url: &str, n: usize, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    list(cfg, url, n, on)
}

pub fn playlist_videos(cfg: &Config, url: &str, on: &mut dyn FnMut(Video)) -> Result<(), String> {
    list(cfg, url, 5000, on)
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
