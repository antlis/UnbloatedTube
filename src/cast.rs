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
