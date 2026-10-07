//! Resolving videos before they are played.
//!
//! mpv finds a video's streams by running yt-dlp, which takes seconds. The app runs the same
//! command ahead of time (for a row the pointer rests on, the next video) and keeps the answer;
//! mpv is pointed at this program as its yt-dlp (`ytdl_hook-ytdl_path`, with `ENV` set), which
//! prints the kept answer when there is one and otherwise just runs the real yt-dlp. mpv's
//! usual path stays in charge of chapters, titles, subtitles and the format, so nothing else
//! changes. What a kept answer skips is YouTube's "watched" ping, which yt-dlp sends as part of
//! that command, so the ping is sent in the background instead.

use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use crate::store::{self, Config};
use crate::yt;

/// Set in mpv's environment: this process is mpv's yt-dlp (see `run_shim`).
pub const ENV: &str = "UNBLOATED_YTDL_SHIM";

/// How long an answer is used; the stream links in it last hours.
pub const TTL: Duration = Duration::from_secs(3600);

fn dir() -> PathBuf {
    store::cache_dir().join("ytdl")
}

fn file(id: &str, format: &str) -> PathBuf {
    // FNV-1a: the format selector is part of the answer, and not fit for a file name.
    let hash = format.bytes().fold(0x811c9dc5u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193));
    dir().join(format!("{id}-{hash:08x}.json"))
}

fn fresh(path: &Path) -> bool {
    path.metadata().and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age < TTL)
}

/// Resolve `id` as mpv's yt-dlp hook would for `format` (the flags are the ones it passes, minus
/// the watched ping) and keep the answer. Blocking: run it on a background thread.
pub fn fetch(cfg: &Config, id: &str, format: &str) -> Result<(), String> {
    let path = file(id, format);
    if fresh(&path) {
        return Ok(());
    }
    let out = Command::new("yt-dlp")
        .env("PYCRYPTODOME_DISABLE_GMP", "1")
        .args(["--no-update", "--no-warnings", "-J", "--flat-playlist", "--sub-format", "ass/srt/best", "--format", format])
        .args(["--sub-langs", "all", "--write-srt", "--no-playlist"])
        .args(cfg.cookie_args())
        .arg("--")
        .arg(format!("https://www.youtube.com/watch?v={id}"))
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("yt-dlp: {e}"))?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(yt::short_error(&String::from_utf8_lossy(&out.stderr)));
    }
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    // Written whole before it has its name: mpv may ask at any moment.
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &out.stdout).and_then(|()| std::fs::rename(&tmp, &path)).map_err(|e| e.to_string())
}

/// Drop answers nobody can use any more (background thread, at startup).
pub fn sweep() {
    for entry in std::fs::read_dir(dir()).into_iter().flatten().flatten() {
        if !fresh(&entry.path()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The video id and format of a command line mpv gives its yt-dlp, when it asks about one video.
fn request(args: &[String]) -> Option<(String, String)> {
    let format = args.iter().position(|a| a == "--format").and_then(|i| args.get(i + 1))?;
    let id = yt::parse_video_link(args.last()?)?.id;
    Some((id, format.clone()))
}

/// Be mpv's yt-dlp: print the kept answer, or become the real yt-dlp.
pub fn run_shim() -> ! {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(json) = request(&args).map(|(id, format)| file(&id, &format)).filter(|p| fresh(p)).and_then(|p| std::fs::read(p).ok()) {
        if args.iter().any(|a| a == "--mark-watched") {
            // The ping a fresh run would have sent; its output is not needed.
            let _ = Command::new("yt-dlp")
                .args(&args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn();
        }
        // mpv closed the pipe: nobody wants an answer, and yt-dlp must not run a second time.
        std::process::exit(if std::io::stdout().write_all(&json).is_ok() { 0 } else { 1 });
    }
    let err = Command::new("yt-dlp").args(&args).exec();
    eprintln!("unbloated-youtube: cannot run yt-dlp: {err}");
    std::process::exit(127)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn a_single_video_request_is_recognised() {
        let a = args(&["--no-warnings", "-J", "--format", "bv+ba", "--mark-watched", "--no-playlist", "--", "https://youtu.be/jNQXAC9IVRw"]);
        assert_eq!(request(&a), Some(("jNQXAC9IVRw".into(), "bv+ba".into())));
    }

    #[test]
    fn other_requests_are_not() {
        assert_eq!(request(&args(&["--version"])), None);
        assert_eq!(request(&args(&["--format", "b", "--", "https://www.youtube.com/@mkbhd"])), None);
        assert_eq!(request(&args(&["-J", "--", "https://youtu.be/jNQXAC9IVRw"])), None);
    }

    #[test]
    fn a_format_has_a_file_of_its_own() {
        assert_ne!(file("jNQXAC9IVRw", "best"), file("jNQXAC9IVRw", "bestvideo+bestaudio"));
        assert_eq!(file("jNQXAC9IVRw", "best"), file("jNQXAC9IVRw", "best"));
    }
}
