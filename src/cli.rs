//! The command line: `unbloated-youtube <link>` and the control commands.
//!
//! The running app listens on a small socket (one JSON line in, one out); a command line that
//! names a command is sent to it. Only `open` (and a bare link) may start the app instead.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const HELP: &str = "\
unbloated-youtube: lightweight YouTube client (GPUI, mpv, yt-dlp)

Usage:
  unbloated-youtube                 start the app
  unbloated-youtube <link>          play a YouTube link (video, channel or playlist)
  unbloated-youtube <command> ...   control the running app

Commands:
  open <link>     play a link; starts the app if it is not running
  queue <link>    add a video to Up next
  pause           pause (what is playing here, or on the cast receiver)
  play            resume
  toggle          pause or resume
  next, prev      next or previous video
  seek <secs>     jump to a time; +10 or -10 move from the current one
  status          what is playing now
  raise           bring the window to the front
  quit            close the app (a cast keeps playing)

Options:
  -h, --help      show this help
  -V, --version   show the version

A command prints its answer and exits 0, or prints why it failed and exits 1.";

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "lowercase")]
pub enum Command {
    Open { url: String },
    Queue { url: String },
    Pause,
    Play,
    Toggle,
    Next,
    Prev,
    /// `relative`: move from the current time instead of going to `secs`.
    Seek { secs: f64, relative: bool },
    Status,
    Raise,
    Quit,
}

pub enum Parsed {
    /// Start the app (with a command to run once it is up, for `open`).
    Launch(Option<Command>),
    Run(Command),
    Print(String),
}

/// Read the arguments after the program name; the error is the message to show.
pub fn parse(args: &[String]) -> Result<Parsed, String> {
    let Some(first) = args.first() else { return Ok(Parsed::Launch(None)) };
    let arg = || args.get(1).cloned().ok_or_else(|| format!("{first}: missing argument (see --help)"));
    let command = match first.as_str() {
        "-h" | "--help" | "help" => return Ok(Parsed::Print(HELP.into())),
        "-V" | "--version" => return Ok(Parsed::Print(format!("unbloated-youtube {}", env!("CARGO_PKG_VERSION")))),
        "open" => Command::Open { url: arg()? },
        "queue" => Command::Queue { url: arg()? },
        "pause" => Command::Pause,
        "play" => Command::Play,
        "toggle" => Command::Toggle,
        "next" => Command::Next,
        "prev" | "previous" => Command::Prev,
        "seek" => {
            let text = arg()?;
            let secs = text.parse::<f64>().ok().filter(|s| s.is_finite()).ok_or_else(|| format!("seek: '{text}' is not a number of seconds"))?;
            Command::Seek { secs, relative: text.starts_with(['+', '-']) }
        }
        "status" => Command::Status,
        "raise" => Command::Raise,
        "quit" => Command::Quit,
        // Anything else is taken for a link, which the app checks.
        other if !other.starts_with('-') => Command::Open { url: other.to_string() },
        other => return Err(format!("unknown option '{other}' (see --help)")),
    };
    if args.len() > 2 || (args.len() == 2 && arg_free(&command)) {
        return Err(format!("{first}: too many arguments (see --help)"));
    }
    Ok(match command {
        Command::Open { .. } => Parsed::Launch(Some(command)),
        _ => Parsed::Run(command),
    })
}

/// A command that takes no argument.
fn arg_free(c: &Command) -> bool {
    !matches!(c, Command::Open { .. } | Command::Queue { .. } | Command::Seek { .. })
}

fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("unbloated-youtube.sock")
}

/// What a command got back from the app: its answer, or why it failed.
pub type Reply = Result<String, String>;

/// Send a command to the running app. The outer error means nothing is listening.
pub fn send(command: &Command) -> std::io::Result<Reply> {
    let mut stream = UnixStream::connect(socket_path())?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    writeln!(stream, "{}", serde_json::to_string(command).map_err(std::io::Error::other)?)?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    let answer: serde_json::Value = serde_json::from_str(&line).map_err(std::io::Error::other)?;
    let text = |key: &str| answer[key].as_str().unwrap_or_default().to_string();
    Ok(if answer["ok"].as_bool() == Some(true) { Ok(text("message")) } else { Err(text("error")) })
}

pub struct Request {
    pub command: Command,
    pub reply: mpsc::Sender<Reply>,
}

/// Listen for commands on a thread; they arrive on the returned channel and each is answered
/// through its `reply`. None when another instance already listens (this one then just runs).
pub fn listen() -> Option<mpsc::Receiver<Request>> {
    let path = socket_path();
    if UnixStream::connect(&path).is_ok() {
        return None;
    }
    // A socket nobody answers on is left over from a killed instance.
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).ok()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            std::thread::spawn(move || serve(stream, tx));
        }
    });
    Some(rx)
}

fn serve(stream: UnixStream, tx: mpsc::Sender<Request>) {
    let mut line = String::new();
    if BufReader::new(&stream).take(1 << 16).read_line(&mut line).is_err() {
        return;
    }
    let reply = match serde_json::from_str::<Command>(&line) {
        Ok(command) => {
            let (reply, answer) = mpsc::channel();
            match tx.send(Request { command, reply }) {
                Ok(()) => answer.recv_timeout(Duration::from_secs(8)).unwrap_or_else(|_| Err("the app did not answer".into())),
                Err(_) => Err("the app is closing".into()),
            }
        }
        Err(e) => Err(format!("not a command: {e}")),
    };
    let answer = match reply {
        Ok(message) => serde_json::json!({ "ok": true, "message": message }),
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    };
    let _ = writeln!(&stream, "{answer}");
}

/// Remove the socket when the app ends; only the instance that listens calls this.
pub fn cleanup() {
    let _ = std::fs::remove_file(socket_path());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Parsed, String> {
        parse(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn a_bare_link_opens() {
        assert!(matches!(p(&["https://youtu.be/abc"]), Ok(Parsed::Launch(Some(Command::Open { url }))) if url == "https://youtu.be/abc"));
        assert!(matches!(p(&[]), Ok(Parsed::Launch(None))));
    }

    #[test]
    fn seek_tells_absolute_from_relative() {
        assert_eq!(p(&["seek", "90"]).ok().map(|r| matches!(r, Parsed::Run(Command::Seek { secs, relative: false }) if secs == 90.)), Some(true));
        assert_eq!(p(&["seek", "-10"]).ok().map(|r| matches!(r, Parsed::Run(Command::Seek { secs, relative: true }) if secs == -10.)), Some(true));
        assert!(p(&["seek", "soon"]).is_err());
        assert!(p(&["seek"]).is_err());
    }

    #[test]
    fn arguments_are_checked() {
        assert!(p(&["pause", "now"]).is_err());
        assert!(p(&["--nope"]).is_err());
        assert!(p(&["queue"]).is_err());
        assert!(matches!(p(&["--help"]), Ok(Parsed::Print(_))));
    }

    #[test]
    fn commands_survive_the_wire() {
        let c = Command::Seek { secs: -10., relative: true };
        assert_eq!(serde_json::from_str::<Command>(&serde_json::to_string(&c).unwrap()).unwrap(), c);
    }
}
