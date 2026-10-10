//! MPRIS: the app as a media player on the session D-Bus, so media keys, the desktop's player
//! widget, headphone buttons, KDE Connect and `playerctl` see what plays and control it.
//!
//! The bus runs on threads of its own. Requests come out as `cli::Command`s (the same ones the
//! command line sends) for the UI to run; the UI hands in a `Snapshot` of what plays, and a
//! change of it is announced on the bus (PropertiesChanged) from another thread.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use crate::cli::Command;

const PATH: &str = "/org/mpris/MediaPlayer2";
const NAME: &str = "org.mpris.MediaPlayer2.unbloatedtube";

/// What plays, as MPRIS shows it.
#[derive(Clone, Default, PartialEq)]
pub struct Snapshot {
    /// "Playing", "Paused" or "Stopped".
    pub status: &'static str,
    pub id: String,
    pub title: String,
    pub channel: String,
    pub url: String,
    pub art: String,
    /// Seconds; 0 when unknown.
    pub duration: f64,
}

#[derive(Default)]
struct Shared {
    now: Snapshot,
    position: f64,
}

pub struct Mpris {
    shared: Arc<Mutex<Shared>>,
    changed: Sender<()>,
    commands: Receiver<Command>,
}

impl Mpris {
    /// Connect on a background thread; without a session bus it quietly does nothing.
    pub fn start() -> Self {
        let shared: Arc<Mutex<Shared>> = Default::default();
        let (commands_tx, commands) = channel();
        let (changed, changed_rx) = channel::<()>();
        let state = shared.clone();
        std::thread::spawn(move || {
            let conn = match connect(commands_tx, state.clone()) {
                Ok(c) => c,
                Err(e) => return eprintln!("unbloatedtube: no MPRIS (media keys): {e}"),
            };
            // Announce each change; the bus keeps serving requests on its own threads.
            while changed_rx.recv().is_ok() {
                while changed_rx.try_recv().is_ok() {}
                let now = state.lock().unwrap().now.clone();
                let props = HashMap::from([("PlaybackStatus", Value::from(now.status)), ("Metadata", Value::from(metadata(&now)))]);
                let _ = conn.emit_signal(
                    None::<()>,
                    PATH,
                    "org.freedesktop.DBus.Properties",
                    "PropertiesChanged",
                    &("org.mpris.MediaPlayer2.Player", props, Vec::<String>::new()),
                );
            }
        });
        Self { shared, changed, commands }
    }

    /// What plays now; a change other than the position is announced.
    pub fn update(&self, now: Snapshot, position: f64) {
        let mut shared = self.shared.lock().unwrap();
        shared.position = position;
        if shared.now != now {
            shared.now = now;
            let _ = self.changed.send(());
        }
    }

    /// Requests from the bus since the last call.
    pub fn commands(&self) -> Vec<Command> {
        self.commands.try_iter().collect()
    }
}

fn connect(commands: Sender<Command>, shared: Arc<Mutex<Shared>>) -> zbus::Result<zbus::blocking::Connection> {
    let conn = zbus::blocking::connection::Builder::session()?
        .serve_at(PATH, Root { commands: commands.clone() })?
        .serve_at(PATH, Player { commands, shared })?
        .build()?;
    // A second app (another user's session sharing the bus, a test copy) gets its own name.
    if conn.request_name(NAME).is_err() {
        conn.request_name(format!("{NAME}.instance{}", std::process::id()))?;
    }
    Ok(conn)
}

/// The track's metadata: id, title, channel as artist, link, thumbnail and length.
fn metadata(now: &Snapshot) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    let mut put = |key: &str, value: Value| {
        if let Ok(v) = value.try_into() {
            m.insert(key.to_string(), v);
        }
    };
    // Object path elements allow only [A-Za-z0-9_]; video ids also have `-`.
    let track = if now.id.is_empty() { "/org/mpris/MediaPlayer2/TrackList/NoTrack".to_string() } else { format!("{PATH}/track/{}", now.id.replace('-', "_2d")) };
    if let Ok(path) = ObjectPath::try_from(track) {
        put("mpris:trackid", Value::from(path));
    }
    if now.id.is_empty() {
        return m;
    }
    put("xesam:title", Value::from(now.title.clone()));
    put("xesam:artist", Value::from(vec![now.channel.clone()]));
    put("xesam:url", Value::from(now.url.clone()));
    put("mpris:artUrl", Value::from(now.art.clone()));
    if now.duration > 0. {
        put("mpris:length", Value::from((now.duration * 1e6) as i64));
    }
    m
}

struct Root {
    commands: Sender<Command>,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {
        let _ = self.commands.send(Command::Raise);
    }

    fn quit(&self) {
        let _ = self.commands.send(Command::Quit);
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> &str {
        "UnbloatedTube"
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> &str {
        "unbloatedtube"
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec!["https".into()]
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct Player {
    commands: Sender<Command>,
    shared: Arc<Mutex<Shared>>,
}

impl Player {
    fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        self.send(Command::Next);
    }

    fn previous(&self) {
        self.send(Command::Prev);
    }

    fn pause(&self) {
        self.send(Command::Pause);
    }

    fn play_pause(&self) {
        self.send(Command::Toggle);
    }

    /// Stop only pauses: the video stays loaded, as with the app's own controls.
    fn stop(&self) {
        self.send(Command::Pause);
    }

    fn play(&self) {
        self.send(Command::Play);
    }

    /// `offset` in microseconds from the current position.
    fn seek(&self, offset: i64) {
        self.send(Command::Seek { secs: offset as f64 / 1e6, relative: true });
    }

    fn set_position(&self, _track: ObjectPath<'_>, position: i64) {
        self.send(Command::Seek { secs: position as f64 / 1e6, relative: false });
    }

    fn open_uri(&self, uri: String) {
        self.send(Command::Open { url: uri });
    }

    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> String {
        let status = self.shared.lock().unwrap().now.status;
        if status.is_empty() { "Stopped" } else { status }.to_string()
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        metadata(&self.shared.lock().unwrap().now)
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        1.
    }

    /// Microseconds.
    #[zbus(property)]
    fn position(&self) -> i64 {
        (self.shared.lock().unwrap().position * 1e6) as i64
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}
