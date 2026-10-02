//! Actions on the logged-in YouTube account (subscribe, like, save to playlist), done through
//! YouTube's internal "InnerTube" API the way yt-dlp authenticates its own requests: browser
//! cookies plus a SAPISIDHASH Authorization header. Cookies stay in memory only.

use crate::store::Config;
use serde_json::{Value, json};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const ORIGIN: &str = "https://www.youtube.com";
const CLIENT_VERSION: &str = "2.20260708.00.00";

pub struct Account {
    cookie_header: String,
    sapisid: String,
}

/// What the account thinks of one video.
#[derive(Clone, Debug, Default)]
pub struct VideoStatus {
    pub channel_id: Option<String>,
    pub subscribed: bool,
    pub liked: bool,
    pub disliked: bool,
}

impl Account {
    /// Read the YouTube cookies: from `cookies_file`, or exported once from the browser via yt-dlp
    /// into a private runtime file that is deleted right after reading.
    pub fn load(cfg: &Config) -> Result<Self, String> {
        let text = match (&cfg.cookies_file, &cfg.cookies_from_browser) {
            (Some(f), _) => std::fs::read_to_string(f).map_err(|e| format!("{f}: {e}"))?,
            (None, Some(browser)) => export_browser_cookies(browser)?,
            _ => return Err("not logged in".into()),
        };
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
        let (mut sub, mut like) = (None, None);
        walk(&next, &mut |key, v| match key {
            "subscribeButtonRenderer" if sub.is_none() => sub = Some(v.clone()),
            "likeStatus" if like.is_none() => like = v.as_str().map(String::from),
            _ => {}
        });
        let sub = sub.unwrap_or_default();
        Ok(VideoStatus {
            channel_id: sub["channelId"].as_str().map(String::from),
            subscribed: sub["subscribed"].as_bool().unwrap_or(false),
            liked: like.as_deref() == Some("LIKE"),
            disliked: like.as_deref() == Some("DISLIKE"),
        })
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

    pub fn save_to_playlist(&self, playlist_id: &str, video_id: &str) -> Result<(), String> {
        let body = json!({
            "playlistId": playlist_id,
            "actions": [{ "action": "ACTION_ADD_VIDEO", "addedVideoId": video_id }],
        });
        self.post("browse/edit_playlist", body).map(drop)
    }
}

/// Call `f` for every key/value pair in a JSON tree.
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

fn export_browser_cookies(browser: &str) -> Result<String, String> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let file = dir.join(format!("unbloated-youtube-cookies-{}.txt", std::process::id()));
    // yt-dlp saves its cookie jar on exit, so any quick request will do.
    let out = Command::new("yt-dlp")
        .env("PYCRYPTODOME_DISABLE_GMP", "1")
        .args(["--no-update", "--no-warnings", "--flat-playlist", "--playlist-end", "1", "--simulate"])
        .args(["--cookies-from-browser", browser, "--cookies"])
        .arg(&file)
        .arg(":ytwatchlater")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let text = std::fs::read_to_string(&file);
    let _ = std::fs::remove_file(&file);
    out.map_err(|e| format!("cannot run yt-dlp: {e}"))?;
    text.map_err(|_| "couldn't read browser cookies".into())
}
