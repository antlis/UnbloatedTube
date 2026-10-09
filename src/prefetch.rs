//! Finding a video's streams sooner: kept answers, a faster lookup, and a yt-dlp that stays running.
//!
//! mpv finds a video's streams by running yt-dlp, which takes seconds. mpv is pointed at this
//! program as its yt-dlp (`ytdl_hook-ytdl_path`, with `ENV` set), and the app runs its own yt-dlp
//! calls through it too (`ytdlp`). What it does with a request:
//!
//! 1. A single-video request it has an answer for (kept from a lookup the app made ahead of time,
//!    for a row the pointer rests on or the next video) is answered from the file. mpv's usual
//!    path stays in charge of chapters, titles, subtitles and the format. What a kept answer
//!    skips is YouTube's "watched" ping, which yt-dlp sends as part of that command, so the ping
//!    is sent in the background instead.
//! 2. Any other single-video request is tried with the HLS and DASH manifests skipped (they only
//!    repeat the formats the page lists, and cost half a second). That answer is used only when it
//!    is plainly an ordinary finished video; a live stream, a premiere, anything odd or any
//!    failure is run again, exactly as asked.
//! 3. The run goes to the helper (`ytdl_helper.py`: yt-dlp already loaded, one fork per request)
//!    when it is up, else to the real yt-dlp.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::{AsFd, AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use crate::store::{self, Config};
use crate::yt;

/// Set in the environment of whatever runs this program as yt-dlp (see `run_shim`).
pub const ENV: &str = "UNBLOATED_YTDL_SHIM";
/// Where the helper listens, for the same.
pub const SOCK_ENV: &str = "UNBLOATED_YTDL_SOCK";

/// How long an answer is used; the stream links in it last hours.
pub const TTL: Duration = Duration::from_secs(3600);

const HELPER: &str = include_str!("ytdl_helper.py");

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

fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

/// Where this app's helper listens (its process id keeps instances apart).
fn socket_path() -> PathBuf {
    runtime_dir().join(format!("unbloated-youtube-ytdl-{}.sock", std::process::id()))
}

/// What to give a program that should run yt-dlp through this one (mpv, `ytdlp`).
pub fn environment() -> [(&'static str, String); 2] {
    [(ENV, "1".into()), (SOCK_ENV, socket_path().display().to_string())]
}

/// This program, to run again as yt-dlp. On Linux through `/proc/<pid>/exe`, which reaches the
/// running binary even after a rebuild replaced the file (`current_exe()` then names a path that
/// is gone, and every yt-dlp run would fail until a restart). mpv can use it too: it is the app's
/// pid, not mpv's.
pub fn self_exe() -> Option<PathBuf> {
    let proc = PathBuf::from(format!("/proc/{}/exe", std::process::id()));
    if proc.exists() { Some(proc) } else { std::env::current_exe().ok() }
}

/// yt-dlp for the app's own requests: this program as the shim, so they get the helper too.
pub fn ytdlp() -> Command {
    let Some(exe) = self_exe() else { return Command::new("yt-dlp") };
    let mut cmd = Command::new(exe);
    cmd.envs(environment());
    cmd
}

/// Resolve `id` as mpv's yt-dlp hook would for `format` (the flags are the ones it passes, minus
/// the watched ping) and keep the answer. Blocking: run it on a background thread.
pub fn fetch(cfg: &Config, id: &str, format: &str) -> Result<(), String> {
    let path = file(id, format);
    if fresh(&path) {
        return Ok(());
    }
    // The shim keeps the answer itself when it is one worth keeping.
    let out = ytdlp()
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
    keep(&path, &out.stdout);
    Ok(())
}

/// Keep an answer, if it is for a video that is not live (a live stream's links are only good
/// for what is on now). Whole before it has its name: mpv may ask at any moment.
fn keep(path: &Path, json: &[u8]) {
    if !finished(json) {
        return;
    }
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    if std::fs::create_dir_all(dir()).is_ok() && std::fs::write(&tmp, json).and_then(|()| std::fs::rename(&tmp, path)).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// An ordinary finished video: not live, not upcoming, with a length and streams to play.
fn finished(json: &[u8]) -> bool {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(json) else { return false };
    v["live_status"].as_str() == Some("not_live")
        && v["is_live"].as_bool() != Some(true)
        && v["duration"].as_f64().is_some_and(|d| d > 0.)
        && (v["requested_formats"].as_array().is_some_and(|f| !f.is_empty()) || v["url"].as_str().is_some())
}

/// Drop answers and helper sockets nobody can use any more (background thread, at startup).
pub fn sweep() {
    for entry in std::fs::read_dir(dir()).into_iter().flatten().flatten() {
        if !fresh(&entry.path()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    // A helper socket whose app is gone (killed, so never cleaned up).
    for entry in std::fs::read_dir(runtime_dir()).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let pid = name.strip_prefix("unbloated-youtube-ytdl-").and_then(|n| n.strip_suffix(".sock"));
        if pid.is_some_and(|pid| !Path::new(&format!("/proc/{pid}")).exists()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The python interpreter and the script of the installed yt-dlp, found by following its
/// launcher (a shell wrapper, as on Nix, or the script pip and pipx install). None for what has
/// no script to load, such as the standalone binary: that one is simply run each time.
fn find_script() -> Option<(PathBuf, PathBuf)> {
    let mut script = std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join("yt-dlp")).find(|p| p.is_file())?;
    for _ in 0..4 {
        let mut head = vec![0u8; 8192];
        let n = File::open(&script).ok()?.read(&mut head).ok()?;
        let text = String::from_utf8(head[..n].to_vec()).ok()?;
        let first = text.strip_prefix("#!")?.lines().next()?.to_string();
        let mut words = first.split_whitespace();
        let interpreter = words.next()?;
        let name = Path::new(interpreter).file_name()?.to_str()?;
        if name == "env" {
            let python = words.find(|w| !w.starts_with('-') && w.starts_with("python"))?;
            let found = std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(python)).find(|p| p.is_file())?;
            return Some((found, script));
        }
        if name.starts_with("python") {
            return Some((PathBuf::from(interpreter), script));
        }
        // A shell wrapper: follow the last absolute path it runs (`exec -a "$0" "/…/.yt-dlp-wrapped"`).
        let whole = std::fs::read_to_string(&script).ok()?;
        let line = whole.lines().rev().find(|l| l.trim_start().starts_with("exec "))?;
        script = line.split('"').skip(1).step_by(2).filter(|s| s.starts_with('/')).last().map(PathBuf::from)?;
    }
    None
}

/// Start the helper (background thread; it lives as long as the app). Without one, requests are
/// served by yt-dlp itself, only slower.
pub fn start_helper() {
    std::thread::spawn(|| {
        let Some((python, script)) = find_script() else { return };
        let socket = socket_path();
        let mut cmd = Command::new(python);
        cmd.env("PYCRYPTODOME_DISABLE_GMP", "1")
            .env("PYTHONUTF8", "1")
            .args(["-c", HELPER])
            .arg(&socket)
            .arg(&script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: prctl is async-signal-safe, as pre_exec requires. The signal comes when the thread
        // that started the helper ends, hence the wait below: this thread lives as long as the app.
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        if let Ok(mut child) = cmd.spawn() {
            let _ = child.wait();
            let _ = std::fs::remove_file(socket);
        }
    });
}

/// Remove the helper's socket when the app ends (the helper goes with it, but cannot clean up).
pub fn cleanup() {
    let _ = std::fs::remove_file(socket_path());
}

/// Send `payload` and the file descriptors over a unix socket in one message.
fn send_fds(sock: &UnixStream, payload: &[u8], fds: &[RawFd]) -> std::io::Result<()> {
    let len = std::mem::size_of_val(fds) as u32;
    // SAFETY: the buffers outlive the call, the control buffer is aligned and sized by CMSG_SPACE.
    unsafe {
        let mut iov = libc::iovec { iov_base: payload.as_ptr() as *mut _, iov_len: payload.len() };
        let space = libc::CMSG_SPACE(len) as usize;
        let mut control = vec![0u64; space.div_ceil(8)];
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = control.as_mut_ptr().cast();
        msg.msg_controllen = space as _;
        let header = libc::CMSG_FIRSTHDR(&msg);
        (*header).cmsg_level = libc::SOL_SOCKET;
        (*header).cmsg_type = libc::SCM_RIGHTS;
        (*header).cmsg_len = libc::CMSG_LEN(len) as _;
        std::ptr::copy_nonoverlapping(fds.as_ptr(), libc::CMSG_DATA(header).cast::<RawFd>(), fds.len());
        if libc::sendmsg(sock.as_raw_fd(), &msg, libc::MSG_NOSIGNAL) < 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Hand a run to the helper: it writes into `out` and `err`; the exit code when `wait`.
/// None when the helper cannot be reached (nothing was run then).
fn helper_run(args: &[String], out: &File, err: &File, wait: bool) -> Option<i32> {
    let mut stream = UnixStream::connect(std::env::var_os(SOCK_ENV)?).ok()?;
    let payload = serde_json::to_vec(&serde_json::json!({ "argv": args })).ok()?;
    send_fds(&stream, &payload, &[out.as_raw_fd(), err.as_raw_fd()]).ok()?;
    if !wait {
        return Some(0);
    }
    let mut answer = String::new();
    stream.read_to_string(&mut answer).ok();
    // The helper going away mid-run leaves no code: that run failed.
    Some(answer.trim().parse().unwrap_or(1))
}

/// Run yt-dlp with `args`: by the helper, else itself. Output goes to the given files.
fn run(args: &[String], out: &File, err: &File) -> i32 {
    helper_run(args, out, err, true).unwrap_or_else(|| {
        let (Ok(out), Ok(err)) = (out.try_clone(), err.try_clone()) else { return 127 };
        // 127: there is no yt-dlp to run.
        Command::new("yt-dlp").args(args).stdin(Stdio::null()).stdout(out).stderr(err).status().map_or(127, |status| status.code().unwrap_or(1))
    })
}

fn err_file() -> File {
    File::from(std::io::stderr().as_fd().try_clone_to_owned().expect("stderr"))
}

/// The video id and format of a command line mpv gives its yt-dlp, when it asks about one video.
fn request(args: &[String]) -> Option<(String, String)> {
    let format = args.iter().position(|a| a == "--format").and_then(|i| args.get(i + 1))?;
    let id = yt::parse_video_link(args.last()?)?.id;
    Some((id, format.clone()))
}

/// The single-video lookup tried first, with the manifests skipped; its answer if it is an
/// ordinary finished video (see `finished`), else None and the caller runs the real request.
fn quick(args: &[String]) -> Option<Vec<u8>> {
    std::fs::create_dir_all(dir()).ok()?;
    let path = dir().join(format!("quick-{}-{:x}", std::process::id(), SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).ok()?.as_nanos()));
    let capture = File::options().read(true).write(true).create_new(true).open(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let nothing = File::options().write(true).open("/dev/null").ok()?;
    let mut tried = vec!["--extractor-args".to_string(), "youtube:skip=hls,dash".to_string()];
    tried.extend_from_slice(args);
    if run(&tried, &capture, &nothing) != 0 {
        return None;
    }
    let mut json = Vec::new();
    // The run wrote through the same open file: read it from the start.
    let mut capture = capture;
    capture.seek(SeekFrom::Start(0)).ok()?;
    capture.read_to_end(&mut json).ok()?;
    finished(&json).then_some(json)
}

/// Be mpv's yt-dlp (or the app's): see the module docs.
pub fn run_shim() -> ! {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = File::from(std::io::stdout().as_fd().try_clone_to_owned().expect("stdout"));
    let stderr = err_file();
    let ask = request(&args);
    let mut json = ask.as_ref().map(|(id, format)| file(id, format)).filter(|p| fresh(p)).and_then(|p| std::fs::read(p).ok());
    if json.is_some() {
        if args.iter().any(|a| a == "--mark-watched") {
            // The ping a fresh run would have sent; nobody waits for it (nor for its output).
            if let Ok(null) = File::options().write(true).open("/dev/null") {
                if helper_run(&args, &null, &null, false).is_none() {
                    let _ = Command::new("yt-dlp").args(&args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0).spawn();
                }
            }
        }
    } else if let Some((id, format)) = &ask {
        json = quick(&args);
        if let Some(json) = &json {
            keep(&file(id, format), json);
        }
    }
    if let Some(json) = json {
        // mpv closed the pipe: nobody wants an answer, and yt-dlp must not run a second time.
        std::process::exit(if (&stdout).write_all(&json).is_ok() { 0 } else { 1 });
    }
    // Everything else, and a single video that is not the ordinary kind (live, premiere, odd):
    // exactly what was asked.
    let code = run(&args, &stdout, &stderr);
    if code == 127 {
        eprintln!("unbloated-youtube: cannot run yt-dlp");
    }
    std::process::exit(code)
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

    fn video(extra: &str) -> Vec<u8> {
        format!(r#"{{"id":"x","requested_formats":[{{"format_id":"1"}}]{extra}}}"#).into_bytes()
    }

    #[test]
    fn only_an_ordinary_finished_video_counts() {
        assert!(finished(&video(r#","live_status":"not_live","is_live":false,"duration":19"#)));
        // Live, upcoming, over-but-still-processing, no length, nothing to play, not even JSON.
        assert!(!finished(&video(r#","live_status":"is_live","is_live":true"#)));
        assert!(!finished(&video(r#","live_status":"is_live","is_live":false,"duration":19"#)));
        assert!(!finished(&video(r#","live_status":"is_upcoming","duration":19"#)));
        assert!(!finished(&video(r#","live_status":"post_live","duration":19"#)));
        assert!(!finished(&video(r#","live_status":"was_live","duration":19"#)));
        assert!(!finished(&video(r#","live_status":"not_live""#)));
        assert!(!finished(&video(r#","duration":19"#)));
        assert!(!finished(br#"{"live_status":"not_live","duration":19}"#));
        assert!(!finished(b"not json"));
    }

    #[test]
    fn the_script_behind_a_launcher_is_found() {
        let dir = std::env::temp_dir().join(format!("ytdl-find-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wrapped = dir.join(".yt-dlp-wrapped");
        std::fs::write(&wrapped, "#!/usr/bin/python3\nimport sys\n").unwrap();
        let wrapper = dir.join("yt-dlp");
        std::fs::write(&wrapper, format!("#!/bin/bash -e\nexport A=\"b\"\nexec -a \"$0\" \"{}\"  \"$@\"\n", wrapped.display())).unwrap();
        let old = std::env::var_os("PATH");
        // SAFETY: only this test touches PATH, and no other test reads it.
        unsafe { std::env::set_var("PATH", &dir) };
        let found = find_script();
        // The shell wrapper is followed to the python script behind it.
        assert_eq!(found, Some((PathBuf::from("/usr/bin/python3"), wrapped.clone())));
        // A script that is not text (the standalone binary) has none.
        std::fs::write(&wrapper, [0x7f, b'E', b'L', b'F', 0xff, 0xfe]).unwrap();
        assert_eq!(find_script(), None);
        if let Some(old) = old {
            unsafe { std::env::set_var("PATH", old) };
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
