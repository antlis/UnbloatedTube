//! Actions on the logged-in YouTube account (subscribe, like, save to playlist), done through
//! YouTube's internal "InnerTube" API the way yt-dlp authenticates its own requests: browser
//! cookies plus a SAPISIDHASH Authorization header. Cookies stay in memory only.

use crate::auth::Auth;
use crate::store::Config;
use crate::yt::Video;
use serde_json::{Value, json};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const ORIGIN: &str = "https://www.youtube.com";
const CLIENT_VERSION: &str = "2.20260708.00.00";

#[derive(Clone)]
pub struct Account {
    cookie_header: String,
    sapisid: String,
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
    /// Read the YouTube cookies: from a cookies file, or exported once from the browser via
    /// yt-dlp into a private runtime file that is deleted right after reading.
    pub fn load(cfg: &Config) -> Result<Self, String> {
        let text = match &cfg.auth {
            Auth::CookiesFile(f) => std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?,
            Auth::Browser(browser) => {
                let (text, spec) = export_browser_cookies(browser)?;
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
        Self::from_cookies(&text)
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
        Ok(Self { cookie_header, sapisid })
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
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let hash = sha1_smol::Sha1::from(format!("{ts} {} {ORIGIN}", self.sapisid)).digest().to_string();
        let resp = ureq::post(&format!("{ORIGIN}/youtubei/v1/{endpoint}?prettyPrint=false"))
            .set("Cookie", &self.cookie_header)
            .set("Authorization", &format!("SAPISIDHASH {ts}_{hash}"))
            .set("Origin", ORIGIN)
            .set("X-Origin", ORIGIN)
            .set("X-Goog-AuthUser", "0")
            .set("X-YouTube-Client-Name", "1")
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
        let next = self.post("next", json!({ "videoId": video_id }))?;
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
    walk(v, &mut |key, v| {
        if key == "thumbnailBadgeViewModel" && duration.is_none() {
            duration = v["text"].as_str().and_then(parse_duration);
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
        live: false,
        id,
    })
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

/// "17K views", "1.2M views", "218 views" as a number.
fn parse_views(s: &str) -> Option<u64> {
    let n = s.trim().strip_suffix("views")?.trim();
    let (num, mult) = match n.chars().last()? {
        'K' | 'k' => (&n[..n.len() - 1], 1e3),
        'M' | 'm' => (&n[..n.len() - 1], 1e6),
        'B' | 'b' => (&n[..n.len() - 1], 1e9),
        _ => (n, 1.),
    };
    num.replace(',', "").parse::<f64>().ok().map(|x| (x * mult) as u64)
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
    // yt-dlp saves its cookie jar on exit, so any quick request will do.
    let run = |spec: &str| -> Result<(String, bool), String> {
        let out = Command::new("yt-dlp")
            .env("PYCRYPTODOME_DISABLE_GMP", "1")
            .args(["--no-update", "--no-warnings", "--flat-playlist", "--playlist-end", "1", "--simulate"])
            .args(["--cookies-from-browser", spec, "--cookies"])
            .arg(&file)
            .arg(":ytwatchlater")
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run yt-dlp: {e}"))?;
        let report = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        // Chromium keeps its cookie keys in the system keyring; yt-dlp's automatic
        // backend can miss them and export only the plain cookies.
        let locked = ["could not be decrypted", "cannot decrypt", "no key found"].iter().any(|p| report.contains(p));
        let text = std::fs::read_to_string(&file).map_err(|_| {
            let detail = report.lines().rev().find_map(|l| l.strip_prefix("ERROR: ")).unwrap_or("").trim();
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
