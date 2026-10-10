//! DeArrow (https://dearrow.ajay.app): titles and thumbnails the community wrote to replace
//! clickbait ones, from the SponsorBlock authors' free, keyless API.
//!
//! Asked as the browser extension asks: by the first 4 hex digits of the video id's SHA-256, so
//! the server answers for a bucket of videos and doesn't learn which one is being looked at.
//! Which submission counts follows the extension too: the top one, if it isn't voted down
//! (or is locked), and isn't a vote for the original.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::{Condvar, Mutex};

const API: &str = "https://sponsor.ajay.app";
const THUMBS: &str = "https://dearrow-thumb.ajay.app";

/// What DeArrow has for a video; nothing set means YouTube's own title and thumbnail stand.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Branding {
    pub title: Option<String>,
    /// Where in the video the replacement thumbnail is (seconds), a frame the server renders.
    pub thumb: Option<f64>,
}

/// Requests at once: the rows on screen ask together; a few at a time is plenty for a free API.
const PARALLEL: usize = 4;
static RUNNING: Mutex<usize> = Mutex::new(0);
static FREED: Condvar = Condvar::new();

/// DeArrow's answer for `id` (blocking: run it on a background thread).
pub fn branding(id: &str) -> Result<Branding, String> {
    let mut running = FREED.wait_while(RUNNING.lock().unwrap(), |n| *n >= PARALLEL).unwrap();
    *running += 1;
    drop(running);
    let res = fetch(id);
    *RUNNING.lock().unwrap() -= 1;
    FREED.notify_one();
    res
}

fn fetch(id: &str) -> Result<Branding, String> {
    let hash = Sha256::digest(id.as_bytes()).iter().map(|b| format!("{b:02x}")).collect::<String>();
    let json: Value = match crate::http::agent().get(&format!("{API}/api/branding/{}", &hash[..4])).query("fetchAll", "true").call() {
        Ok(resp) => resp.into_json().map_err(|e| e.to_string())?,
        // Nothing submitted for any video in the bucket.
        Err(ureq::Error::Status(404, _)) => return Ok(Branding::default()),
        Err(e) => return Err(e.to_string()),
    };
    Ok(parse(&json[id]))
}

/// One video's entry of the answer (`{"titles": [...], "thumbnails": [...], ...}`).
fn parse(v: &Value) -> Branding {
    let counts = |s: &Value| s["locked"].as_bool() == Some(true) || s["votes"].as_i64().is_some_and(|n| n >= 0);
    let top = |key: &str| v[key].as_array().and_then(|a| a.first()).filter(|s| counts(s) && s["original"].as_bool() != Some(true));
    Branding {
        title: top("titles").and_then(|s| s["title"].as_str()).map(clean).filter(|t| !t.is_empty()),
        thumb: top("thumbnails").and_then(|s| s["timestamp"].as_f64()).filter(|t| t.is_finite() && *t >= 0.),
    }
}

/// A submitted title as shown: `>` before a word only tells the extension not to change its case.
fn clean(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut prev_space = true;
    let mut chars = title.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '>' && prev_space && chars.peek().is_some_and(|n| !n.is_whitespace()) {
            continue;
        }
        prev_space = c.is_whitespace();
        out.push(c);
    }
    out.trim().to_string()
}

/// The replacement thumbnail: the frame at `time`, rendered by DeArrow's thumbnail server.
pub fn thumb_url(id: &str, time: f64) -> String {
    format!("{THUMBS}/api/v1/getThumbnail?videoID={id}&time={time}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_top_submission_counts_unless_voted_down_or_original() {
        let v = json!({
            "titles": [{"title": "How >iPhone cameras work", "original": false, "votes": 2, "locked": false}],
            "thumbnails": [{"timestamp": 12.5, "original": false, "votes": 0, "locked": false}],
        });
        assert_eq!(parse(&v), Branding { title: Some("How iPhone cameras work".into()), thumb: Some(12.5) });
        let down = json!({"titles": [{"title": "x", "original": false, "votes": -1, "locked": false}]});
        assert_eq!(parse(&down), Branding::default());
        let locked = json!({"titles": [{"title": "x", "original": false, "votes": -1, "locked": true}]});
        assert_eq!(parse(&locked).title.as_deref(), Some("x"));
        let original = json!({"titles": [{"title": "Orig", "original": true, "votes": 5, "locked": false}], "thumbnails": [{"original": true, "votes": 3, "locked": false}]});
        assert_eq!(parse(&original), Branding::default());
        assert_eq!(parse(&Value::Null), Branding::default());
    }

    #[test]
    fn the_case_marker_is_dropped() {
        assert_eq!(clean(">iPhone 15 vs >Pixel 8"), "iPhone 15 vs Pixel 8");
        assert_eq!(clean("a > b"), "a > b");
        assert_eq!(clean("x->y  "), "x->y");
    }
}
