//! Actions on the logged-in YouTube account (subscribe, like, save to playlist), done through
//! YouTube's internal "InnerTube" API the way yt-dlp authenticates its own requests: browser
//! cookies plus a SAPISIDHASH Authorization header. Cookies stay in memory only.

use crate::auth::Auth;
use crate::store::Config;
use crate::yt::{Comment, Group, Video};
use serde_json::{Value, json};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const ORIGIN: &str = "https://www.youtube.com";
const CLIENT_VERSION: &str = "2.20260708.00.00";

#[derive(Clone)]
pub struct Account {
    cookie_header: String,
    sapisid: String,
    /// Made from cookies kept from an earlier export (`kept`), which YouTube may have rotated since.
    pub from_kept: bool,
}

/// The account the list fetches use (see `yt.rs`), so each doesn't export the browser's cookies
/// again: the last one the app loaded, else a fresh load. Forgotten when the login changes.
static SHARED: Mutex<Option<Arc<Account>>> = Mutex::new(None);
static LOADING: Mutex<()> = Mutex::new(());
/// Held while the browser's cookies are read again (`refresh`, `refresh_kept`).
static REFRESHING: Mutex<()> = Mutex::new(());

pub fn remember(account: &Arc<Account>) {
    *SHARED.lock().unwrap() = Some(account.clone());
}

pub fn forget() {
    *SHARED.lock().unwrap() = None;
    let _ = std::fs::remove_file(kept_path());
}

pub fn shared(cfg: &Config) -> Result<Arc<Account>, String> {
    // Lists asked for together (feed and recommendations at start) wait for one cookie export
    // instead of each running their own.
    let _one = LOADING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(a) = SHARED.lock().unwrap().clone() {
        return Ok(a);
    }
    let a = Arc::new(Account::load(cfg)?);
    remember(&a);
    Ok(a)
}

/// `f` with the shared account; once more with cookies read from the browser now when kept ones
/// were refused (`f` must not have delivered anything when it fails).
pub fn with_shared<R>(cfg: &Config, mut f: impl FnMut(&Account) -> Result<R, String>) -> Result<R, String> {
    let a = shared(cfg)?;
    match f(&a) {
        Err(e) if a.from_kept => {
            crate::yt::timing(&format!("kept cookies refused ({e})"), std::time::Instant::now());
            f(&*refresh(cfg, &a)?)
        }
        r => r,
    }
}

/// The account with cookies read from the browser now, because `stale` was refused; for the
/// lists too. Lists refused together wait for one read.
pub fn refresh(cfg: &Config, stale: &Arc<Account>) -> Result<Arc<Account>, String> {
    let _one = REFRESHING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(a) = SHARED.lock().unwrap().clone().filter(|a| !Arc::ptr_eq(a, stale)) {
        return Ok(a);
    }
    let a = Arc::new(Account::load_fresh(cfg)?);
    remember(&a);
    Ok(a)
}

/// At start: read the browser's cookies again in the background when kept ones are in use. A
/// browser that is running rotates them, and YouTube soon stops taking the old ones; lists start
/// with the kept ones meanwhile, and one they turn out refused for waits for this read.
pub fn refresh_kept(cfg: &Config) {
    let Auth::Browser(browser) = &cfg.auth else { return };
    if kept(browser).is_none() {
        return;
    }
    let _one = REFRESHING.lock().unwrap_or_else(|e| e.into_inner());
    // A refused list may have read them first.
    if SHARED.lock().unwrap().as_ref().is_some_and(|a| !a.from_kept) {
        return;
    }
    if let Ok(a) = Account::load_fresh(cfg) {
        remember(&Arc::new(a));
    }
}

/// Reading the browser's cookies takes seconds (yt-dlp, the keyring), so the last export is kept
/// for the next start: in the session's private runtime folder (gone at logout), readable only by
/// the user, for at most `KEEP`, and only for the same browser. A login change deletes it.
const KEEP: std::time::Duration = std::time::Duration::from_secs(12 * 3600);

fn kept_path() -> std::path::PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) => std::path::PathBuf::from(dir).join("unbloatedtube-cookies.txt"),
        None => dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("unbloatedtube").join("cookies.txt"),
    }
}

fn kept(browser: &str) -> Option<String> {
    let path = kept_path();
    let age = std::fs::metadata(&path).ok()?.modified().ok()?.elapsed().ok()?;
    let text = std::fs::read_to_string(&path).ok()?;
    let (first, rest) = text.split_once('\n')?;
    (age < KEEP && first == format!("# browser: {browser}")).then(|| rest.to_string())
}

fn keep(browser: &str, text: &str) {
    let path = kept_path();
    let tmp = path.with_extension("tmp");
    if write_private(&tmp, &format!("# browser: {browser}\n{text}")).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// A copy of the kept cookies for one yt-dlp run, so it skips reading the browser (yt-dlp writes
/// its jar back to the file it is given, so each run gets its own). The caller deletes it.
pub fn kept_copy(browser: &str) -> Option<std::path::PathBuf> {
    let text = kept(browser)?;
    Account::from_cookies(&text).ok()?;
    let path = kept_path().with_file_name(format!("unbloatedtube-cookies-run-{}.txt", std::process::id()));
    write_private(&path, &text).ok()?;
    Some(path)
}

/// Write a new file only the user can read.
fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::remove_file(path);
    std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?.write_all(text.as_bytes())
}

/// The signed-in account as far as YouTube's guide endpoint shows it: handle like
/// "@username", and the avatar the guide draws next to it.
#[derive(Clone, Debug, Default)]
pub struct Me {
    pub handle: Option<String>,
    pub avatar: Option<String>,
}

/// What the account thinks of one video.
#[derive(Clone, Debug, Default)]
pub struct VideoStatus {
    pub channel_id: Option<String>,
    pub subscribed: bool,
    pub liked: bool,
    pub disliked: bool,
    /// The channel's avatar URL.
    pub avatar: Option<String>,
    /// As YouTube words them: "8.5K views", "1 year ago", "814 subscribers".
    pub views: Option<String>,
    pub date: Option<String>,
    pub subscribers: Option<String>,
}

/// A playlist the account can add a video to, as YouTube's Save menu lists it.
#[derive(Clone, Debug)]
pub struct SaveOption {
    pub id: String,
    pub title: String,
    /// The video is already in it.
    pub contains: bool,
}

impl Account {
    /// Read the YouTube cookies: from a cookies file, or from the browser (see `load_fresh`),
    /// using the ones kept from the last export when there are.
    pub fn load(cfg: &Config) -> Result<Self, String> {
        if let Auth::Browser(browser) = &cfg.auth {
            if let Some(Ok(a)) = kept(browser).map(|text| Self::from_cookies(&text)) {
                return Ok(Self { from_kept: true, ..a });
            }
        }
        Self::load_fresh(cfg)
    }

    /// Read the YouTube cookies: from a cookies file, or exported from the browser via yt-dlp
    /// into a private runtime file that is deleted right after reading (and kept, see `KEEP`).
    pub fn load_fresh(cfg: &Config) -> Result<Self, String> {
        let text = match &cfg.auth {
            Auth::CookiesFile(f) => std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?,
            Auth::Browser(browser) => {
                let start = std::time::Instant::now();
                let (text, spec) = export_browser_cookies(browser)?;
                crate::yt::timing(&format!("cookies from {spec}"), start);
                // yt-dlp sometimes needs a keyring suffix ("brave+gnomekeyring") to read
                // Chromium's encrypted cookies. Remember the spec that worked so every
                // later call agrees — but only when auth.json (not config.toml) is the
                // login's source; hand-edited values stay hand-edited.
                // Skipped when auth.json changed meanwhile (log out, another browser): a
                // slow export must not write a stale login back.
                if &spec != browser && cfg.cookies_file.is_none() && cfg.cookies_from_browser.is_none() && Auth::load() == cfg.auth {
                    Auth::Browser(spec).save();
                }
                text
            }
            Auth::None => return Err("not logged in".into()),
        };
        let account = Self::from_cookies(&text)?;
        // Only a login is kept: anything else would stand in for the browser's for hours.
        if let Auth::Browser(browser) = &cfg.auth {
            keep(browser, &text);
        }
        Ok(account)
    }

    /// No login: what YouTube answers anyone (search, public playlists).
    pub fn anonymous() -> Self {
        Self { cookie_header: String::new(), sapisid: String::new(), from_kept: false }
    }

    /// Parse a Netscape cookies.txt; needs at least one YouTube login cookie.
    /// Also used to validate a cookies.txt before importing it.
    pub fn from_cookies(text: &str) -> Result<Self, String> {
        let cookies: Vec<(String, String)> = text
            .lines()
            .map(|l| l.strip_prefix("#HttpOnly_").unwrap_or(l))
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                (f.len() == 7 && f[0].ends_with("youtube.com")).then(|| (f[5].to_string(), f[6].to_string()))
            })
            .collect();
        let get = |name: &str| cookies.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone());
        // YouTube itself falls back to __Secure-3PAPISID when SAPISID is missing.
        let sapisid = get("SAPISID").or_else(|| get("__Secure-3PAPISID")).ok_or("no YouTube login cookies")?;
        let cookie_header = cookies.iter().map(|(n, v)| format!("{n}={v}")).collect::<Vec<_>>().join("; ");
        Ok(Self { cookie_header, sapisid, from_kept: false })
    }

    /// Who is signed in: the guide endpoint's entry for your own channel. A missing handle
    /// is not an error — only a failed request is (expired cookies answer 401/403).
    pub fn me(&self) -> Result<Me, String> {
        let guide = self.post("guide", json!({}))?;
        let (mut handle, mut avatar) = (None, None);
        walk(&guide, &mut |key, v| match key {
            "channelHandleRenderer" if handle.is_none() => {
                handle = v.get("channelHandleText").and_then(renderer_text);
                avatar = v["avatar"]["thumbnails"].as_array().and_then(|t| t.last()).and_then(|t| t["url"].as_str()).map(String::from);
            }
            "accountName" if handle.is_none() => handle = renderer_text(v),
            _ => {}
        });
        Ok(Me { handle, avatar })
    }

    fn post(&self, endpoint: &str, mut body: Value) -> Result<Value, String> {
        body["context"] = json!({ "client": { "clientName": "WEB", "clientVersion": CLIENT_VERSION, "hl": "en" } });
        let mut req = crate::http::agent()
            .post(&format!("{ORIGIN}/youtubei/v1/{endpoint}?prettyPrint=false"))
            .set("Origin", ORIGIN)
            .set("X-Origin", ORIGIN)
            .set("X-YouTube-Client-Name", "1");
        if !self.sapisid.is_empty() {
            let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            let hash = sha1_smol::Sha1::from(format!("{ts} {} {ORIGIN}", self.sapisid)).digest().to_string();
            req = req
                .set("Cookie", &self.cookie_header)
                .set("Authorization", &format!("SAPISIDHASH {ts}_{hash}"))
                .set("X-Goog-AuthUser", "0");
        }
        let resp = req
            .set("X-YouTube-Client-Version", CLIENT_VERSION)
            .send_json(body)
            .map_err(|e| match e {
                ureq::Error::Status(code, _) => format!("YouTube refused ({code})"),
                e => e.to_string(),
            })?;
        resp.into_json().map_err(|e| e.to_string())
    }

    /// Like and subscription state for a video (read-only).
    pub fn status(&self, video_id: &str) -> Result<VideoStatus, String> {
        let next = self.watch_page(video_id)?;
        // The first subscribe button / like state is the watched video's; related videos come later.
        let (mut sub, mut like, mut owner, mut primary) = (None, None, None, None);
        walk(&next, &mut |key, v| match key {
            "subscribeButtonRenderer" if sub.is_none() => sub = Some(v.clone()),
            "videoOwnerRenderer" if owner.is_none() => owner = Some(v.clone()),
            "videoPrimaryInfoRenderer" if primary.is_none() => primary = Some(v.clone()),
            "likeStatus" if like.is_none() => like = v.as_str().map(String::from),
            _ => {}
        });
        let sub = sub.unwrap_or_default();
        Ok(VideoStatus {
            channel_id: sub["channelId"].as_str().map(String::from),
            subscribed: sub["subscribed"].as_bool().unwrap_or(false),
            liked: like.as_deref() == Some("LIKE"),
            disliked: like.as_deref() == Some("DISLIKE"),
            views: {
                let count = |key: &str| text(&primary, &["viewCount", "videoViewCountRenderer", key, "simpleText"]);
                // Newer videos lack the short form, or give it without the word "views".
                count("shortViewCount")
                    .or_else(|| count("extraShortViewCount").map(|n| format!("{n} views")))
                    .or_else(|| count("viewCount"))
            },
            date: text(&primary, &["relativeDateText", "simpleText"]),
            subscribers: text(&owner, &["subscriberCountText", "simpleText"]),
            avatar: owner.and_then(|o| o["thumbnail"]["thumbnails"][0]["url"].as_str().map(String::from)),
        })
    }

    /// The watch history, newest first, each entry with its day ("Today", "Saturday", …) in
    /// `watched`. Shorts are in it too (`short`). Follows the continuation pages until `limit`
    /// entries are there; a video watched on several days is listed once, at its latest.
    pub fn history(&self, limit: usize) -> Result<Vec<Video>, String> {
        let mut out: Vec<Video> = Vec::new();
        let mut day = String::new();
        let mut resp = self.post("browse", json!({ "browseId": "FEhistory" }))?;
        for _ in 0..8 {
            collect_history(&resp, &mut day, &mut out);
            let mut token = None;
            walk(&resp, &mut |key, v| {
                if key == "continuationCommand" && token.is_none() {
                    token = v["token"].as_str().map(String::from);
                }
            });
            match token {
                Some(token) if out.len() < limit => resp = self.post("browse", json!({ "continuation": token }))?,
                _ => break,
            }
        }
        let mut seen = std::collections::HashSet::new();
        out.retain(|v| seen.insert(v.id.clone()));
        out.truncate(limit);
        Ok(out)
    }

    /// The videos of a browse page ("FEsubscriptions", "FEwhat_to_watch", "VL<playlist id>"),
    /// handed to `on` page by page as they arrive, until `limit`. The number delivered.
    pub fn browse_videos(&self, browse_id: &str, limit: usize, on: &mut dyn FnMut(Video)) -> Result<usize, String> {
        let first = self.post("browse", json!({ "browseId": browse_id }))?;
        self.paged("browse", first, limit, collect_videos, |v| &v.id, on)
    }

    /// Search results (videos only, like yt-dlp's `ytsearch`), page by page, until `limit`.
    pub fn search_videos(&self, query: &str, limit: usize, on: &mut dyn FnMut(Video)) -> Result<usize, String> {
        let first = self.post("search", json!({ "query": query, "params": "EgIQAQ==" }))?;
        self.paged("search", first, limit, collect_videos, |v| &v.id, on)
    }

    /// A video's watch page data (`next`): its like and subscribe state, the way to its comments.
    /// The last one is kept, so the comments of the playing video don't ask for it again.
    fn watch_page(&self, video_id: &str) -> Result<Arc<Value>, String> {
        static LAST: Mutex<Option<(String, Arc<Value>)>> = Mutex::new(None);
        if let Some((_, page)) = LAST.lock().unwrap().clone().filter(|(id, _)| id == video_id) {
            return Ok(page);
        }
        let page = Arc::new(self.post("next", json!({ "videoId": video_id }))?);
        *LAST.lock().unwrap() = Some((video_id.to_string(), page.clone()));
        Ok(page)
    }

    /// A video's description as its uploader wrote it (the plain text, links as they were typed,
    /// which the watch page shows rewritten). Empty when there is none.
    pub fn description(&self, video_id: &str) -> Result<String, String> {
        let player = self.post("player", json!({ "videoId": video_id, "contentCheckOk": true, "racyCheckOk": true }))?;
        let details = &player["videoDetails"];
        if details["videoId"].as_str() != Some(video_id) {
            return Err("no video details".into());
        }
        Ok(details["shortDescription"].as_str().unwrap_or("").trim().to_string())
    }

    /// The top comments of a video (YouTube's "Top comments" order), page by page, until `limit`.
    pub fn comments(&self, video_id: &str, limit: usize, on: &mut dyn FnMut(Comment)) -> Result<usize, String> {
        let page = self.watch_page(video_id)?;
        // The comments section is a continuation of the watch page; turned off comments have none.
        let mut token = None;
        walk(&page, &mut |key, v| {
            if key == "itemSectionRenderer" && v["sectionIdentifier"].as_str() == Some("comment-item-section") {
                walk(v, &mut |key, v| {
                    if key == "continuationCommand" && token.is_none() {
                        token = v["token"].as_str().map(String::from);
                    }
                });
            }
        });
        let token = token.ok_or("no comments")?;
        let first = self.post("next", json!({ "continuation": token }))?;
        self.paged("next", first, limit, collect_comments, |c: &(String, Comment)| &c.0, &mut |(_, c)| on(c))
    }

    /// One tab of a channel, page by page, until `limit`. `channel`: a channel id (UC…), or a
    /// channel page's link of any kind (@handle, /c/, /user/), which YouTube resolves first.
    pub fn channel_videos(&self, channel: &str, tab: ChannelTab, limit: usize, on: &mut dyn FnMut(Video)) -> Result<usize, String> {
        let id = match channel.starts_with("UC") {
            true => channel.to_string(),
            false => {
                let resp = self.post("navigation/resolve_url", json!({ "url": channel }))?;
                resp["endpoint"]["browseEndpoint"]["browseId"].as_str().filter(|id| id.starts_with("UC")).ok_or("not a channel")?.to_string()
            }
        };
        let first = self.post("browse", json!({ "browseId": id, "params": tab.params() }))?;
        // Parameters YouTube no longer knows get the channel's Home tab instead: a mix of
        // shelves, not this list. Only the tab asked for is taken.
        let mut selected = None;
        walk(&first, &mut |key, v| {
            if key == "tabRenderer" && v["selected"].as_bool() == Some(true) {
                selected = selected.take().or_else(|| v["title"].as_str().map(String::from));
            }
        });
        if selected.as_deref() != Some(tab.title()) {
            return Err(format!("YouTube answered with the {} tab", selected.as_deref().unwrap_or("wrong")));
        }
        // The rows of a channel's own tab don't name the channel.
        let name = first["metadata"]["channelMetadataRenderer"]["title"].as_str().map(String::from);
        let url = format!("https://www.youtube.com/channel/{id}");
        let mut named = |mut v: Video| {
            v.channel = v.channel.or_else(|| name.clone());
            v.channel_url = v.channel_url.or_else(|| Some(url.clone()));
            on(v)
        };
        self.paged("browse", first, limit, collect_videos, |v| &v.id, &mut named)
    }

    /// The channels the account is subscribed to (YouTube's own list of them), page by page.
    pub fn subscribed_channels(&self, on: &mut dyn FnMut(Group)) -> Result<usize, String> {
        let first = self.post("browse", json!({ "browseId": "FEchannels" }))?;
        self.paged("browse", first, 5000, collect_channels, |g| &g.id, on)
    }

    /// Follow a list's continuation pages. Stops at `limit`, at the end, or at a page that adds
    /// nothing new; a failed later page keeps what arrived.
    fn paged<T>(
        &self,
        endpoint: &str,
        mut resp: Value,
        limit: usize,
        collect: impl Fn(&Value) -> Vec<T>,
        key: impl Fn(&T) -> &String,
        on: &mut dyn FnMut(T),
    ) -> Result<usize, String> {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let before = seen.len();
            for v in collect(&resp) {
                if seen.len() < limit && seen.insert(key(&v).clone()) {
                    on(v);
                }
            }
            if seen.len() >= limit || seen.len() == before {
                break;
            }
            // The page's last continuation is its "more" one (earlier ones are filter chips).
            let mut token = None;
            walk(&resp, &mut |key, v| {
                if key == "continuationCommand" {
                    token = v["token"].as_str().map(String::from).or(token.take());
                }
            });
            let Some(token) = token else { break };
            match self.post(endpoint, json!({ "continuation": token })) {
                Ok(next) => resp = next,
                Err(_) if !seen.is_empty() => break,
                Err(e) => return Err(e),
            }
        }
        Ok(seen.len())
    }

    pub fn subscribe(&self, channel_id: &str, on: bool) -> Result<(), String> {
        let ep = if on { "subscription/subscribe" } else { "subscription/unsubscribe" };
        self.post(ep, json!({ "channelIds": [channel_id] })).map(drop)
    }

    pub fn like(&self, video_id: &str, on: bool) -> Result<(), String> {
        let ep = if on { "like/like" } else { "like/removelike" };
        self.post(ep, json!({ "target": { "videoId": video_id } })).map(drop)
    }

    pub fn dislike(&self, video_id: &str, on: bool) -> Result<(), String> {
        let ep = if on { "like/dislike" } else { "like/removelike" };
        self.post(ep, json!({ "target": { "videoId": video_id } })).map(drop)
    }

    /// The playlists the video can be saved to (only your own, Watch later left out) and which
    /// already hold it: what YouTube's own Save menu shows. Read-only.
    pub fn save_options(&self, video_id: &str) -> Result<Vec<SaveOption>, String> {
        let resp = self.post("playlist/get_add_to_playlist", json!({ "videoIds": [video_id], "excludeWatchLater": true }))?;
        let mut out = Vec::new();
        walk(&resp, &mut |key, v| {
            if key == "playlistAddToOptionRenderer" {
                if let (Some(id), Some(title)) = (v["playlistId"].as_str(), renderer_text(&v["title"])) {
                    // "ALL" when the (one) video is in it.
                    out.push(SaveOption { id: id.into(), title, contains: v["containsSelectedVideos"].as_str() == Some("ALL") });
                }
            }
        });
        Ok(out)
    }

    pub fn save_to_playlist(&self, playlist_id: &str, video_id: &str) -> Result<(), String> {
        let body = json!({
            "playlistId": playlist_id,
            "actions": [{ "action": "ACTION_ADD_VIDEO", "addedVideoId": video_id }],
        });
        // YouTube answers 200 either way; the status says whether it really happened.
        let resp = self.post("browse/edit_playlist", body)?;
        match resp["status"].as_str() {
            Some("STATUS_SUCCEEDED") => Ok(()),
            _ => Err(format!("YouTube didn't add it ({})", refusal(&resp))),
        }
    }

    /// The id of one entry of `playlist_id` holding `video_id` (the first, when it is in there
    /// several times): what removing exactly one copy needs. Follows the playlist's pages.
    fn playlist_entry(&self, playlist_id: &str, video_id: &str) -> Result<Option<String>, String> {
        let mut body = json!({ "browseId": format!("VL{playlist_id}") });
        // Pages hold 100 entries; a playlist is at most 5000 long.
        for _ in 0..60 {
            let resp = self.post("browse", body)?;
            let mut found = None;
            let mut next = None;
            walk(&resp, &mut |key, v| match key {
                "playlistVideoRenderer" if found.is_none() && v["videoId"].as_str() == Some(video_id) => {
                    found = v["setVideoId"].as_str().map(String::from);
                }
                "continuationCommand" if next.is_none() => next = v["token"].as_str().map(String::from),
                _ => {}
            });
            if found.is_some() {
                return Ok(found);
            }
            let Some(token) = next else { return Ok(None) };
            body = json!({ "continuation": token });
        }
        Ok(None)
    }

    /// Remove one copy of `video_id` from `playlist_id`. (`ACTION_REMOVE_VIDEO_BY_VIDEO_ID` would
    /// remove every copy of it.)
    pub fn remove_from_playlist(&self, playlist_id: &str, video_id: &str) -> Result<(), String> {
        let Some(entry) = self.playlist_entry(playlist_id, video_id)? else {
            return Err("It is not in that playlist (any more)".into());
        };
        let body = json!({
            "playlistId": playlist_id,
            "actions": [{ "action": "ACTION_REMOVE_VIDEO", "setVideoId": entry }],
        });
        let resp = self.post("browse/edit_playlist", body)?;
        match resp["status"].as_str() {
            Some("STATUS_SUCCEEDED") => Ok(()),
            _ => Err(format!("YouTube didn't remove it ({})", refusal(&resp))),
        }
    }
}

/// A string at `path` inside the JSON, if there.
fn text(v: &Option<Value>, path: &[&str]) -> Option<String> {
    path.iter().try_fold(v.as_ref()?, |v, k| v.get(k))?.as_str().map(String::from)
}

/// Text of a renderer node whose wording is either a SimpleText or runs.
fn renderer_text(v: &Value) -> Option<String> {
    v.get("simpleText").and_then(Value::as_str).map(String::from).or_else(|| v["runs"][0]["text"].as_str().map(String::from))
}

/// The videos of a history page in order, each tagged with its section's day. A page continues
/// the previous one's last day when its first section has no header.
fn collect_history(v: &Value, day: &mut String, out: &mut Vec<Video>) {
    match v {
        Value::Object(m) => match m.get("itemSectionRenderer") {
            Some(section) => {
                if let Some(title) = section["header"]["itemSectionHeaderRenderer"].get("title").and_then(renderer_text) {
                    *day = title;
                }
                walk(section, &mut |key, v| match key {
                    "lockupViewModel" => out.extend(history_video(v, day)),
                    "shortsLockupViewModel" => out.extend(history_short(v, day)),
                    _ => {}
                });
            }
            None => m.values().for_each(|v| collect_history(v, day, out)),
        },
        Value::Array(a) => a.iter().for_each(|v| collect_history(v, day, out)),
        _ => {}
    }
}

fn history_video(v: &Value, day: &str) -> Option<Video> {
    if v["contentType"] != "LOCKUP_CONTENT_TYPE_VIDEO" {
        return None;
    }
    let id = v["contentId"].as_str()?.to_string();
    let meta = &v["metadata"]["lockupMetadataViewModel"];
    let parts = &meta["metadata"]["contentMetadataViewModel"]["metadataRows"][0]["metadataParts"];
    let (mut channel_id, mut duration) = (None, None);
    walk(meta, &mut |key, v| {
        if key == "browseEndpoint" && channel_id.is_none() {
            channel_id = v["browseId"].as_str().map(String::from);
        }
    });
    let mut live = false;
    walk(v, &mut |key, v| {
        if key == "thumbnailBadgeViewModel" {
            let text = v["text"].as_str().unwrap_or_default();
            live |= text.eq_ignore_ascii_case("live") || v["badgeStyle"].as_str().is_some_and(|s| s.contains("LIVE"));
            if duration.is_none() {
                duration = parse_duration(text);
            }
        }
    });
    Some(Video {
        title: meta["title"]["content"].as_str().unwrap_or(&id).to_string(),
        channel: parts[0]["text"]["content"].as_str().map(String::from),
        channel_url: channel_id.map(|c| format!("https://www.youtube.com/channel/{c}")),
        duration,
        short: false,
        views: parts[1]["text"]["content"].as_str().and_then(parse_views),
        watched: Some(day.to_string()).filter(|d| !d.is_empty()),
        live,
        id,
    })
}

/// The videos of a list page, in order, in whichever of YouTube's shapes it uses: the newer
/// `lockupViewModel` (as History has), `videoRenderer` and its grid/compact forms (search,
/// feeds), `playlistVideoRenderer` (playlists), and Shorts.
fn collect_videos(page: &Value) -> Vec<Video> {
    let mut out = Vec::new();
    walk(page, &mut |key, v| match key {
        "lockupViewModel" => out.extend(history_video(v, "")),
        "shortsLockupViewModel" => out.extend(history_short(v, "")),
        "videoRenderer" | "gridVideoRenderer" | "compactVideoRenderer" | "playlistVideoRenderer" => out.extend(renderer_video(v)),
        _ => {}
    });
    out
}

/// A `videoRenderer`-like node; None without an id and a title (or for an unplayable playlist
/// entry, "[Deleted video]").
fn renderer_video(v: &Value) -> Option<Video> {
    let id = v["videoId"].as_str().filter(|id| id.len() == 11)?.to_string();
    if v["isPlayable"] == false {
        return None;
    }
    let title = full_text(&v["title"])?;
    let byline = ["ownerText", "longBylineText", "shortBylineText"].iter().map(|k| &v[*k]).find(|b| b["runs"].is_array());
    let channel = byline.and_then(|b| b["runs"][0]["text"].as_str()).map(String::from);
    let channel_id = byline.and_then(|b| b["runs"][0]["navigationEndpoint"]["browseEndpoint"]["browseId"].as_str());
    let live = v["badges"].as_array().is_some_and(|b| b.iter().any(|b| b["metadataBadgeRenderer"]["style"] == "BADGE_STYLE_TYPE_LIVE_NOW"))
        || v["thumbnailOverlays"].as_array().is_some_and(|o| o.iter().any(|o| o["thumbnailOverlayTimeStatusRenderer"]["style"] == "LIVE"));
    let duration = v.get("lengthText").and_then(renderer_text).and_then(|t| parse_duration(&t)).or_else(|| v["lengthSeconds"].as_str()?.parse().ok());
    let views = v.get("viewCountText").and_then(full_text).or_else(|| v["videoInfo"]["runs"][0]["text"].as_str().map(String::from));
    Some(Video {
        title,
        channel,
        channel_url: channel_id.map(|c| format!("https://www.youtube.com/channel/{c}")),
        duration,
        short: v["navigationEndpoint"]["reelWatchEndpoint"].is_object(),
        views: views.as_deref().and_then(parse_views),
        watched: None,
        live,
        id,
    })
}

/// All of a text node: a SimpleText, or its runs joined.
fn full_text(v: &Value) -> Option<String> {
    if let Some(s) = v["simpleText"].as_str() {
        return Some(s.to_string());
    }
    let text: String = v["runs"].as_array()?.iter().filter_map(|r| r["text"].as_str()).collect();
    (!text.is_empty()).then_some(text)
}

fn history_short(v: &Value, day: &str) -> Option<Video> {
    let id = v["onTap"]["innertubeCommand"]["reelWatchEndpoint"]["videoId"].as_str()?.to_string();
    let meta = &v["overlayMetadata"];
    Some(Video {
        title: meta["primaryText"]["content"].as_str().unwrap_or(&id).to_string(),
        channel: None,
        channel_url: None,
        duration: None,
        short: true,
        views: meta["secondaryText"]["content"].as_str().and_then(parse_views),
        watched: Some(day.to_string()).filter(|d| !d.is_empty()),
        live: false,
        id,
    })
}

/// "38:32" or "1:24:57" in seconds; None for "LIVE" and the like.
fn parse_duration(s: &str) -> Option<f64> {
    s.trim().split(':').try_fold(0u64, |acc, p| Some(acc * 60 + p.parse::<u64>().ok()?)).map(|n| n as f64)
}

/// "17K views", "1.2M views", "218 views", "1,234 watching" as a number.
fn parse_views(s: &str) -> Option<u64> {
    let s = s.trim();
    parse_count(["views", "view", "watching", "subscribers", "subscriber"].iter().find_map(|w| s.strip_suffix(w))?)
}

/// "1,234", "1.2K", "3M" as a number.
fn parse_count(n: &str) -> Option<u64> {
    let n = n.trim();
    let (num, mult) = match n.chars().last()? {
        'K' | 'k' => (&n[..n.len() - 1], 1e3),
        'M' | 'm' => (&n[..n.len() - 1], 1e6),
        'B' | 'b' => (&n[..n.len() - 1], 1e9),
        _ => (n, 1.),
    };
    num.replace(',', "").parse::<f64>().ok().map(|x| (x * mult) as u64)
}

/// A tab of a channel's page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChannelTab {
    Videos,
    Shorts,
    Live,
}

impl ChannelTab {
    /// The browse parameters that open the tab, as the page's own tab links carry them.
    fn params(self) -> &'static str {
        match self {
            ChannelTab::Videos => "EgZ2aWRlb3PyBgQKAjoA",
            ChannelTab::Shorts => "EgZzaG9ydHPyBgUKA5oBAA%3D%3D",
            ChannelTab::Live => "EgdzdHJlYW1z8gYECgJ6AA%3D%3D",
        }
    }

    /// Its title on the page (requests ask for English).
    fn title(self) -> &'static str {
        match self {
            ChannelTab::Videos => "Videos",
            ChannelTab::Shorts => "Shorts",
            ChannelTab::Live => "Live",
        }
    }
}

/// The comments of a comments page, in order, with their ids. YouTube sends them two ways: as
/// `commentRenderer`s inside the threads, or as threads that only name a comment whose content
/// comes separately (`commentEntityPayload`, under `frameworkUpdates`).
fn collect_comments(page: &Value) -> Vec<(String, Comment)> {
    let mut payloads = std::collections::HashMap::new();
    walk(page, &mut |key, v| {
        if key == "commentEntityPayload" {
            if let Some(k) = v["key"].as_str() {
                payloads.insert(k.to_string(), v.clone());
            }
        }
    });
    let mut out = Vec::new();
    walk(page, &mut |key, t| {
        if key != "commentThreadRenderer" {
            return;
        }
        let view = &t["commentViewModel"]["commentViewModel"];
        let old = &t["comment"]["commentRenderer"];
        if let Some(p) = view["commentKey"].as_str().and_then(|k| payloads.get(k)) {
            let props = &p["properties"];
            let Some(id) = props["commentId"].as_str() else { return };
            out.push((
                id.to_string(),
                Comment {
                    author: p["author"]["displayName"].as_str().unwrap_or("").to_string(),
                    text: props["content"]["content"].as_str().unwrap_or("").to_string(),
                    likes: p["toolbar"]["likeCountNotliked"].as_str().map(str::trim).and_then(|n| if n.is_empty() { Some(0) } else { parse_count(n) }),
                    age: props["publishedTime"].as_str().unwrap_or("").to_string(),
                    pinned: !view["pinnedText"].is_null(),
                },
            ));
        } else if let Some(id) = old["commentId"].as_str() {
            out.push((
                id.to_string(),
                Comment {
                    author: full_text(&old["authorText"]).unwrap_or_default(),
                    text: full_text(&old["contentText"]).unwrap_or_default(),
                    likes: full_text(&old["voteCount"]).and_then(|n| parse_count(&n)).or(Some(0)),
                    age: full_text(&old["publishedTimeText"]).unwrap_or_default(),
                    pinned: !old["pinnedCommentBadge"].is_null(),
                },
            ));
        }
    });
    out
}

/// The channels listed on a page (`channelRenderer`s, as on the subscriptions page).
fn collect_channels(page: &Value) -> Vec<Group> {
    let mut out = Vec::new();
    walk(page, &mut |key, c| {
        if key != "channelRenderer" {
            return;
        }
        let Some(id) = c["channelId"].as_str().filter(|id| id.starts_with("UC")) else { return };
        let Some(title) = full_text(&c["title"]).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) else { return };
        let thumb = c["thumbnail"]["thumbnails"].as_array().and_then(|t| t.last()).and_then(|t| t["url"].as_str()).map(|u| match u.starts_with("//") {
            true => format!("https:{u}"),
            false => u.to_string(),
        });
        // Since handles, the subscriber count sits where the video count was (and the handle
        // where the subscriber count was): take whichever says "subscribers".
        let subscribers = [&c["subscriberCountText"], &c["videoCountText"]].into_iter().filter_map(full_text).find_map(|t| parse_views(&t));
        out.push(Group { id: id.to_string(), title, url: format!("https://www.youtube.com/channel/{id}/videos"), thumb, subscribers });
    });
    out
}

/// Call `f` for every key/value pair in a JSON tree.
/// What YouTube said when it did not do an edit: its status, and its message if it gave one.
fn refusal(resp: &Value) -> String {
    let status = resp["status"].as_str().unwrap_or("no status");
    let mut message = None;
    walk(resp, &mut |key, v| {
        if message.is_none() && matches!(key, "errorMessage" | "message") {
            message = renderer_text(v).or_else(|| v.as_str().map(String::from));
        }
    });
    match message {
        Some(m) => format!("{status}: {m}"),
        None => status.to_string(),
    }
}

fn walk(v: &Value, f: &mut impl FnMut(&str, &Value)) {
    match v {
        Value::Object(m) => m.iter().for_each(|(k, v)| {
            f(k, v);
            walk(v, f)
        }),
        Value::Array(a) => a.iter().for_each(|v| walk(v, f)),
        _ => {}
    }
}

fn export_browser_cookies(browser: &str) -> Result<(String, String), String> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    // Exports can overlap (the Connect check next to a status fetch), so each call gets
    // its own file — a shared path would let one run delete or truncate another's jar.
    static N: AtomicU64 = AtomicU64::new(0);
    let file = dir.join(format!(
        "unbloatedtube-cookies-{}-{}.txt",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    // yt-dlp saves its cookie jar on exit, even after a failed request. A file URL fails at
    // once, before anything goes over the network (a YouTube one cost a round trip, seconds).
    let run = |spec: &str| -> Result<(String, bool), String> {
        let out = crate::prefetch::ytdlp()
            .env("PYCRYPTODOME_DISABLE_GMP", "1")
            .args(["--no-update", "--no-warnings", "--simulate"])
            .args(["--cookies-from-browser", spec, "--cookies"])
            .arg(&file)
            .args(["--", "file:///nonexistent"])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run yt-dlp: {e}"))?;
        let report = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        // Chromium keeps its cookie keys in the system keyring; yt-dlp's automatic
        // backend can miss them and export only the plain cookies.
        let locked = ["could not be decrypted", "cannot decrypt", "no key found"].iter().any(|p| report.contains(p));
        let text = std::fs::read_to_string(&file).map_err(|_| {
            let detail = report.lines().rev().filter(|l| !l.contains("file://")).find_map(|l| l.strip_prefix("ERROR: ")).unwrap_or("").trim();
            format!("couldn't read {spec}'s cookies: is the browser installed, and has it been opened once?{}", if detail.is_empty() { String::new() } else { format!(" ({detail})") })
        });
        let _ = std::fs::remove_file(&file);
        let text = text?;
        Ok((text, locked))
    };
    let (text, locked) = run(browser)?;
    if locked && !browser.contains('+') {
        let spec = format!("{browser}+gnomekeyring");
        // The retry can fail (no keyring running); the plain cookies may still be enough.
        match run(&spec) {
            Ok((text, _)) => Ok((text, spec)),
            Err(_) => Ok((text, browser.to_string())),
        }
    } else {
        Ok((text, browser.to_string()))
    }
}
