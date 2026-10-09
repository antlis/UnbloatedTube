//! A single long-lived mpv process, controlled over its JSON IPC socket.

use crate::auth::Auth;
use crate::store::{Config, Settings};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// URL of the loaded file, to tell the new video from the previous one while switching.
    pub path: String,
    /// Nothing loaded (e.g. the last load failed).
    pub idle: bool,
    /// mpv has a playback position, i.e. the file at `path` is actually playing.
    pub playing: bool,
    /// Playback reached the end (mpv keeps the last frame, `--keep-open`).
    pub ended: bool,
    /// YouTube chapters: start time and title.
    pub chapters: Vec<(f64, String)>,
    /// "?" was pressed over the video since the last query.
    pub help: bool,
    /// Something the hover bar (controls.lua) wants the app to do: "prev", "next", "speed", "pip"
    /// or "menu X Y" / "menu {json with x and y}" (right click; see `menu_position`).
    pub action: String,
    pub position: f64,
    pub duration: f64,
    pub paused: bool,
    pub muted: bool,
    /// mpv's volume, 0-100 (also changed by its own key bindings over the video).
    pub volume: Option<f64>,
    /// mpv's own fullscreen flag (toggled by f / double-click / Esc); unbloated-youtube mirrors it.
    pub fullscreen: bool,
    /// Subtitle tracks the app added (downloaded files). mpv's own ones, which are URLs that
    /// YouTube refuses to serve it, don't count.
    pub sub_tracks: u32,
    /// The first such track's id, to select it.
    pub sub_id: Option<i64>,
    /// One of them is selected and visible.
    pub sub_on: bool,
}

/// Where a right click happened, in mpv's window pixels, from its `menu ...` action: the hover
/// bar's script sends "menu 12 34", the plain binding mpv's `mouse-pos` as JSON.
pub fn menu_position(action: &str) -> Option<(f64, f64)> {
    let rest = action.strip_prefix("menu")?.trim();
    if let Ok(v) = serde_json::from_str::<Value>(rest) {
        return Some((v["x"].as_f64()?, v["y"].as_f64()?));
    }
    let mut n = rest.split_whitespace().map(str::parse::<f64>);
    Some((n.next()?.ok()?, n.next()?.ok()?))
}

pub struct Player {
    socket: PathBuf,
    child: Option<Child>,
    /// Options the running mpv was started with.
    options: Vec<String>,
    /// Our mouse bindings were sent to the running mpv.
    bound: bool,
    /// Volume (0-100) for the next mpv start; a running mpv is changed with `set_volume`.
    volume: f32,
}

impl Player {
    pub fn new(volume: f32) -> Self {
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        // Sockets of unbloated-youtube instances that were killed (and so never cleaned up after themselves).
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let pid = name.strip_prefix("unbloated-youtube-mpv-").and_then(|n| n.strip_suffix(".sock"));
            if pid.is_some_and(|pid| !std::path::Path::new(&format!("/proc/{pid}")).exists()) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Self { socket: dir.join(format!("unbloated-youtube-mpv-{}.sock", std::process::id())), child: None, options: Vec::new(), bound: false, volume }
    }

    pub fn alive(&mut self) -> bool {
        matches!(self.child.as_mut().map(|c| c.try_wait()), Some(Ok(None)))
    }

    /// `wid`: X11 window to render into; None opens mpv's own window. `options` come from
    /// `options()`; if they differ from the running mpv's, mpv is restarted to apply them.
    pub fn play(
        &mut self,
        options: &[String],
        url: &str,
        start: f64,
        speed: f32,
        wid: Option<u32>,
        paused: bool,
    ) -> Result<(), String> {
        if self.alive() && self.options != options {
            if let Some(mut c) = self.child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
        if !self.alive() {
            let _ = std::fs::remove_file(&self.socket);
            let mut cmd = Command::new("mpv");
            // mpv's yt-dlp inherits this; see `crate::yt` for why it matters.
            cmd.env("PYCRYPTODOME_DISABLE_GMP", "1")
                .arg(format!("--input-ipc-server={}", self.socket.display()))
                .args(["--idle=yes", "--force-window=yes", "--keep-open=yes", "--title=unbloated-youtube"])
                // Embedded in our X11 window: pick X11 EGL directly. mpv's auto-probing of GPU
                // contexts segfaults with the nixos-unstable mpv on some drivers (Intel/Mesa 25.2).
                // Before `options`, so a --gpu-context in the user's extra options still wins.
                .args(wid.map(|_| "--gpu-context=x11egl"))
                // mpv's yt-dlp is this program, which answers from what was resolved ahead of time.
                .args(crate::prefetch::self_exe().map(|exe| format!("--script-opts-append=ytdl_hook-ytdl_path={}", exe.display())))
                .envs(crate::prefetch::environment())
                .args(options)
                .args(["--volume-max=100".to_string(), format!("--volume={}", self.volume)])
                .arg(format!("--speed={speed}"))
                .arg(format!("--start={start}"))
                .args(wid.map(|w| format!("--wid={w}")))
                .args(paused.then_some("--pause"))
                .arg(url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(mpv_log());
            // Stop mpv when we exit: closing the window ends the process without running our
            // Drop, and a crash wouldn't either, which would leave the video playing.
            // SAFETY: prctl is async-signal-safe, as pre_exec requires.
            unsafe {
                cmd.pre_exec(|| {
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                    Ok(())
                });
            }
            self.child = Some(cmd.spawn().map_err(|e| format!("cannot start mpv: {e}"))?);
            self.options = options.to_vec();
            self.bound = false;
            return Ok(());
        }
        self.command(json!(["set_property", "start", start.to_string()]))?;
        // mpv keeps "subtitles off" across files, so a CC click would silently hide the
        // captions of every later video: each new video starts with them on again.
        let _ = self.command(json!(["set_property", "sub-visibility", true]));
        self.command(json!(["set_property", "pause", paused]))?;
        self.command(json!(["loadfile", url, "replace"]))
    }

    /// Stop playback and exit mpv (logout, or the connect screen coming up).
    pub fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.bound = false;
        self.options.clear();
    }

    /// Options the running mpv was started with (empty if none).
    pub fn options(&mut self) -> &[String] {
        if self.alive() { &self.options } else { &[] }
    }

    /// Click on the video toggles pause (mpv's default double-click still toggles fullscreen).
    /// Sent once mpv answers, since its IPC socket isn't up right after start.
    pub fn bind_mouse(&mut self) {
        if !self.bound && self.alive() {
            // "?" over the video: flag it for us (read in `query`) instead of mpv's own help.
            // Volume keys over the video (mpv's default Up/Down seek a minute).
            self.bound = self.command(json!(["keybind", "MBTN_LEFT", "cycle pause"])).is_ok()
                && self.command(json!(["keybind", "?", "set user-data/unbloated/help yes"])).is_ok()
                // mpv's default (a double click toggles fullscreen), which the setting above removes.
                && self.command(json!(["keybind", "MBTN_LEFT_DBL", "cycle fullscreen"])).is_ok()
                // Right click: the app's video menu, at the pointer (also without the hover bar's script).
                && self
                    .command(json!(["keybind", "MBTN_RIGHT", "expand-properties set user-data/unbloated/action \"menu ${mouse-pos}\""]))
                    .is_ok()
                && [("UP", 5), ("=", 5), ("+", 5), ("DOWN", -5), ("-", -5)]
                    .iter()
                    .all(|(key, step)| self.command(json!(["keybind", key, format!("add volume {step}")])).is_ok());
        }
    }

    /// Reset the hover bar's request after acting on it.
    pub fn clear_action(&self) {
        let _ = self.command(json!(["set", "user-data/unbloated/action", ""]));
    }

    /// Reset the "?" flag after acting on it.
    pub fn clear_help(&self) {
        let _ = self.command(json!(["set", "user-data/unbloated/help", "no"]));
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        let _ = self.command(json!(["set_property", "volume", volume]));
    }

    pub fn set_speed(&self, speed: f32) {
        let _ = self.command(json!(["set_property", "speed", speed]));
    }

    fn connect(&self) -> Result<UnixStream, String> {
        let s = UnixStream::connect(&self.socket).map_err(|e| e.to_string())?;
        s.set_read_timeout(Some(Duration::from_millis(300))).ok();
        Ok(s)
    }

    pub fn command(&self, cmd: Value) -> Result<(), String> {
        let mut s = self.connect()?;
        writeln!(s, "{}", json!({ "command": cmd })).map_err(|e| e.to_string())
    }

    pub fn toggle_pause(&self) {
        let _ = self.command(json!(["cycle", "pause"]));
    }

    pub fn seek_relative(&self, secs: f64) {
        let _ = self.command(json!(["seek", secs, "relative"]));
    }

    pub fn pause(&self) {
        let _ = self.command(json!(["set_property", "pause", true]));
    }

    pub fn toggle_mute(&self) {
        let _ = self.command(json!(["cycle", "mute"]));
    }

    /// Load a subtitle file and show it.
    pub fn add_subtitle(&self, file: &Path) {
        let _ = self.command(json!(["sub-add", file.display().to_string(), "select"]));
    }

    /// Show the subtitle track `id`, or hide the subtitles.
    pub fn show_subtitles(&self, on: bool, id: Option<i64>) {
        let _ = self.command(json!(["set_property", "sub-visibility", on]));
        let _ = self.command(json!(["set_property", "sid", if on { json!(id.unwrap_or(1)) } else { json!("no") }]));
    }

    pub fn set_fullscreen(&self, on: bool) {
        let _ = self.command(json!(["set_property", "fullscreen", on]));
    }

    /// Loop the current video (mpv's `loop-file`), showing the new state over the picture.
    pub fn toggle_loop(&self) {
        let _ = self.command(json!(["cycle-values", "loop-file", "inf", "no"]));
    }

    /// mpv's "stats for nerds" overlay.
    pub fn toggle_stats(&self) {
        let _ = self.command(json!(["script-binding", "stats/display-stats-toggle"]));
    }

    pub fn seek_absolute(&self, secs: f64) {
        let _ = self.command(json!(["seek", secs, "absolute"]));
    }

    /// The IPC socket if mpv is running, for `query` on a background thread.
    pub fn socket_if_alive(&mut self) -> Option<PathBuf> {
        self.alive().then(|| self.socket.clone())
    }
}

/// yt-dlp format selector for the user's quality / codec / audio-only settings.
pub fn format(s: &Settings) -> String {
    let q = s.max_quality;
    let codec = if s.prefer_hw_codecs { "[vcodec!^=av01]" } else { "" };
    if s.audio_only {
        "bestaudio/best".to_string()
    } else {
        // Fall back to any codec, then to a single combined stream, if the preferred one is missing.
        format!("bestvideo[height<=?{q}]{codec}+bestaudio/bestvideo[height<=?{q}]+bestaudio/best[height<=?{q}]/best")
    }
}

/// mpv command-line options for the user's settings (everything but per-video ones).
/// `pip`: play in mpv's own small always-on-top window instead of embedded.
pub fn options(cfg: &Config, s: &Settings, pip: bool) -> Vec<String> {
    let mut out = vec![
        format!("--ytdl-format={}", format(s)),
        // Fetch in 10 MB range requests: YouTube throttles one long request to ~150 KB/s.
        "--stream-lavf-o-append=request_size=10485760".into(),
    ];
    let mut raw = vec!["mark-watched=".to_string()];
    match &cfg.auth {
        Auth::CookiesFile(f) => raw.push(format!("cookies={}", f.display())),
        Auth::Browser(b) => raw.push(format!("cookies-from-browser={b}")),
        Auth::None => raw.clear(),
    }
    if !raw.is_empty() {
        out.push(format!("--ytdl-raw-options={}", raw.join(",")));
    }
    if s.hwdec {
        out.push("--hwdec=auto-safe".into());
    }
    // Embedded in the app, mpv's own controls and keys are off: the app has its own (the PiP
    // window has nothing else, so it keeps them). `mpv_args` below can still override these.
    if !pip && !s.native_controls {
        out.extend(
            ["--osc=no", "--osd-level=0", "--input-default-bindings=no", "--input-builtin-bindings=no", "--input-vo-keyboard=no"]
                .map(String::from),
        );
    }
    // Subtitles are downloaded by the app (YouTube refuses mpv's own request for them) and
    // added with `sub-add`, so mpv has none of its own.
    out.push(format!("--sub-scale={}", s.sub_scale));
    if let (true, Some(script)) = (s.sponsorblock, sponsorblock_script()) {
        out.push(format!("--script={script}"));
        out.push(format!("--script-opts=sponsorblock_minimal-categories={}", s.skip_segments.join(";")));
    }
    // The app's hover bar, independent of mpv's own controls above. After SponsorBlock's
    // `--script-opts=`, which would replace this one; `--script=` adds to the scripts.
    if let (false, true, Some(script)) = (pip, s.video_controls, controls_script()) {
        out.push(format!("--script={script}"));
        out.push("--script-opts-append=unbloated-controls-accent=454EFF".into());
    }
    if pip {
        // Bottom-right corner; the title lets tiling WMs float it (e.g. i3 for_window rules).
        out.extend(["--ontop", "--geometry=480x270-24-24", "--title=unbloated-youtube PiP"].map(String::from));
    }
    out.extend(s.mpv_args.split_whitespace().map(String::from));
    out
}

/// The hover-controls script (controls.lua), written to the cache dir so mpv can load it.
fn controls_script() -> Option<String> {
    let src = include_str!("controls.lua");
    let dir = crate::store::cache_dir();
    let path = dir.join("controls.lua");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(src) {
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(&path, src).ok()?;
    }
    Some(path.display().to_string())
}

/// Path of the SponsorBlock mpv script, if the environment provides it (see shell.nix).
pub fn sponsorblock_script() -> Option<String> {
    std::env::var("UNBLOATED_SPONSORBLOCK").ok().filter(|p| std::path::Path::new(p).exists())
}

/// Current playback state, or None if mpv doesn't answer. Blocks while mpv is busy opening a
/// video (it answers only after yt-dlp finishes), so never call it on the UI thread.
pub fn query(socket: &Path) -> Option<State> {
    let mut s = UnixStream::connect(socket).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok();
    let props = ["time-pos", "duration", "pause", "fullscreen", "path", "idle-active", "eof-reached", "chapter-list", "user-data/unbloated/help", "mute", "volume", "user-data/unbloated/action", "track-list", "sub-visibility"];
    for (i, p) in props.iter().enumerate() {
        writeln!(s, "{}", json!({ "command": ["get_property", p], "request_id": i })).ok()?;
    }
    let mut vals = vec![Value::Null; props.len()];
    let mut got = 0;
    for line in BufReader::new(s).lines() {
        let v: Value = serde_json::from_str(&line.ok()?).ok()?;
        if let Some(i) = v.get("request_id").and_then(Value::as_u64) {
            vals[i as usize] = v.get("data").cloned().unwrap_or(Value::Null);
            got += 1;
            if got == props.len() {
                break;
            }
        }
    }
    let tracks = vals[12].as_array().cloned().unwrap_or_default();
    let ours: Vec<&Value> = tracks
        .iter()
        .filter(|t| t["type"] == "sub" && t["external-filename"].as_str().is_some_and(|f| f.starts_with('/')))
        .collect();
    Some(State {
        position: vals[0].as_f64().unwrap_or(0.0),
        playing: vals[0].is_number(),
        duration: vals[1].as_f64().unwrap_or(0.0),
        paused: vals[2].as_bool().unwrap_or(false),
        fullscreen: vals[3].as_bool().unwrap_or(false),
        path: vals[4].as_str().unwrap_or_default().to_string(),
        idle: vals[5].as_bool().unwrap_or(false),
        ended: vals[6].as_bool().unwrap_or(false),
        chapters: vals[7]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|c| Some((c["time"].as_f64()?, c["title"].as_str().unwrap_or_default().to_string())))
                    .collect()
            })
            .unwrap_or_default(),
        help: vals[8].as_str() == Some("yes") || vals[8].as_bool() == Some(true),
        muted: vals[9].as_bool().unwrap_or(false),
        volume: vals[10].as_f64(),
        action: vals[11].as_str().unwrap_or_default().to_string(),
        sub_tracks: ours.len() as u32,
        sub_id: ours.first().and_then(|t| t["id"].as_i64()),
        sub_on: vals[13].as_bool().unwrap_or(false) && ours.iter().any(|t| t["selected"] == true),
    })
}

impl Drop for Player {
    fn drop(&mut self) {
        if let Some(c) = self.child.as_mut() {
            let _ = c.kill();
        }
        let _ = std::fs::remove_file(&self.socket);
    }
}

fn mpv_log() -> Stdio {
    let dir = crate::store::cache_dir();
    let _ = std::fs::create_dir_all(&dir);
    std::fs::File::create(dir.join("mpv.log")).map_or(Stdio::null(), Stdio::from)
}
