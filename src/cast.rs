//! Cast: send the playing video to another device by running a command from config.toml
//! (`[cast.<name>]`, `command = [...]`). The app knows nothing about any receiver; the command does.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// What a cast command can use: `{url}` (the video's page), `{start}` (whole seconds), `{id}`, `{title}`.
pub struct Playing<'a> {
    pub url: &'a str,
    pub id: &'a str,
    pub title: &'a str,
    pub start: u64,
}

/// The command with its placeholders filled in. Each argument stays one argument: the result is
/// never split or parsed by a shell, and text that was substituted is not scanned again.
pub fn expand(args: &[String], p: &Playing) -> Vec<String> {
    args.iter().map(|arg| expand_one(arg, p)).collect()
}

fn expand_one(arg: &str, p: &Playing) -> String {
    let start = p.start.to_string();
    let names = [("{url}", p.url), ("{id}", p.id), ("{title}", p.title), ("{start}", start.as_str())];
    let mut out = String::new();
    let mut rest = arg;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        match names.iter().find(|(name, _)| rest.starts_with(name)) {
            Some(&(name, value)) => {
                out.push_str(value);
                rest = &rest[name.len()..];
            }
            None => {
                out.push('{');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A command typed in Settings as an argument list: arguments split at spaces, `'…'` and `"…"`
/// keep spaces inside one argument, and a backslash takes the next character literally. Like
/// config.toml's lists it never goes through a shell.
pub fn split_command(text: &str) -> Vec<String> {
    let (mut args, mut cur, mut quote, mut started) = (Vec::new(), String::new(), None, false);
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (c, quote) {
            ('\\', _) => {
                cur.extend(chars.next());
                started = true;
            }
            ('\'' | '"', None) => {
                quote = Some(c);
                started = true;
            }
            (c, Some(q)) if c == q => quote = None,
            (c, None) if c.is_whitespace() => {
                if started {
                    args.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (c, _) => {
                cur.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push(cur);
    }
    args
}

/// Run the command and wait for it, at most `timeout`. The error is the receiver's own reason
/// (the `error` of a JSON answer, else the first line it printed), else the exit status.
pub fn run(args: &[String], timeout: Duration) -> Result<(), String> {
    let (program, rest) = args.split_first().ok_or("the command is empty")?;
    let mut child = Command::new(program)
        .args(rest)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("no answer within {}s", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e.to_string()),
        }
    };
    if status.success() {
        return Ok(());
    }
    let read = |pipe: Option<&mut dyn Read>| {
        let mut text = String::new();
        if let Some(pipe) = pipe {
            let _ = pipe.read_to_string(&mut text);
        }
        text
    };
    let stdout = read(child.stdout.as_mut().map(|p| p as &mut dyn Read));
    let stderr = read(child.stderr.as_mut().map(|p| p as &mut dyn Read));
    let reason = serde_json::from_str::<serde_json::Value>(stdout.trim())
        .ok()
        .and_then(|v| v["error"].as_str().map(String::from))
        .or_else(|| stdout.lines().chain(stderr.lines()).map(str::trim).find(|l| !l.is_empty()).map(String::from));
    Err(reason.map_or_else(|| status.to_string(), |r| r.chars().take(160).collect()))
}

/// A receiver that speaks the remote API (`POST /play`, `GET /status`, `POST /ctl`, bearer token),
/// so the app can show and control what it plays.
#[derive(Clone)]
pub struct Remote {
    base: String,
    token: String,
}

/// What the receiver is playing now.
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
    pub paused: bool,
}

impl Remote {
    pub fn new(url: &str, token: &str) -> Self {
        Self { base: url.trim_end_matches('/').to_string(), token: token.to_string() }
    }

    fn call(&self, method: &str, path: &str, body: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
        self.call_within(method, path, body, Duration::from_secs(60))
    }

    fn call_within(&self, method: &str, path: &str, body: Option<serde_json::Value>, limit: Duration) -> Result<serde_json::Value, String> {
        let req = ureq::request(method, &format!("{}{path}", self.base))
            .set("Authorization", &format!("Bearer {}", self.token))
            .timeout(limit);
        let res = match body {
            Some(body) => req.send_json(body),
            None => req.call(),
        };
        match res {
            Ok(res) => res.into_json().map_err(|e| e.to_string()),
            // The receiver's own reason: `{"ok": false, "error": "..."}`.
            Err(ureq::Error::Status(code, res)) => Err(res
                .into_json::<serde_json::Value>()
                .ok()
                .and_then(|v| v["error"].as_str().map(String::from))
                .unwrap_or_else(|| format!("HTTP {code}"))),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn play(&self, url: &str, start: u64) -> Result<(), String> {
        self.call("POST", "/play", Some(serde_json::json!({ "url": url, "start": start }))).map(drop)
    }

    pub fn ctl(&self, body: serde_json::Value) -> Result<(), String> {
        self.call("POST", "/ctl", Some(body)).map(drop)
    }

    /// Stop the receiver, waiting at most `limit`: for when the app is closing and can't wait long.
    pub fn stop_within(&self, limit: Duration) -> Result<(), String> {
        self.call_within("POST", "/ctl", Some(serde_json::json!({ "action": "stop" })), limit).map(drop)
    }

    pub fn status(&self) -> Result<Status, String> {
        let v = self.call("GET", "/status", None)?;
        Ok(Status {
            playing: v["playing"].as_bool().unwrap_or(false),
            position: v["position"].as_f64().unwrap_or(0.),
            duration: v["duration"].as_f64().unwrap_or(0.),
            paused: v["paused"].as_bool().unwrap_or(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::split_command;

    fn split(text: &str) -> Vec<String> {
        split_command(text)
    }

    #[test]
    fn splits_at_spaces_and_keeps_quoted_text_together() {
        assert_eq!(split("catt -d \"Living Room\" cast {url}"), ["catt", "-d", "Living Room", "cast", "{url}"]);
        assert_eq!(split("  a   'b c'  d\\ e "), ["a", "b c", "d e"]);
        assert_eq!(split("say '' x"), ["say", "", "x"]);
        assert_eq!(split("a\"b c\"d"), ["ab cd"]);
    }

    #[test]
    fn empty_and_unterminated_input() {
        assert!(split("").is_empty());
        assert!(split("   ").is_empty());
        assert_eq!(split("echo \"a b"), ["echo", "a b"]);
    }
}
