use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Tab {
    Subscriptions,
    Playlists,
    History,
    Downloads,
    Search,
    Settings,
}

/// A split being dragged: the column divider, the one between player and recommendations,
/// or the one under History's Continue watching.
#[derive(Clone, Copy)]
pub(crate) enum Split {
    Columns,
    Player,
    Continue,
}

/// Comments fetched per page: the first fetch, and each "Load more".
pub(crate) const COMMENTS_PAGE: usize = 40;

/// The "New uploads" pseudo-channel at the top of Subscriptions.
pub(crate) const FEED_ID: &str = "feed";

/// Where the project lives; opened by the button at the bottom of Settings.
pub(crate) const REPO_URL: &str = "https://github.com/antlis/UnbloatedTube";

/// A list being fetched. `Loading` already holds whatever has arrived (or the previous list
/// during a refresh), so the UI can show it right away.
pub(crate) enum Load<T> {
    Idle,
    Loading(Vec<T>),
    Ready(Vec<T>),
    Failed(String),
}

impl<T> Load<T> {
    pub(crate) fn items(&self) -> &[T] {
        match self {
            Load::Loading(v) | Load::Ready(v) => v,
            _ => &[],
        }
    }
}

/// A tab listing groups (channels or playlists); clicking one shows its videos.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Lower {
    Recommended,
    Description,
    Transcript,
    UpNext,
    Chapters,
    Comments,
    WatchLater,
}

/// An entry of the left column's list, for Vim navigation.
#[derive(Clone)]
pub(crate) enum Item {
    Group(Group),
    /// A video and the list it plays from (for Prev / Next).
    Video(Video, Arc<[Video]>),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum ChannelView {
    Videos,
    Shorts,
    /// A channel's live streams (its /streams page).
    Live,
}

pub(crate) struct Browser {
    pub(crate) groups: Load<Group>,
    pub(crate) open: Option<Group>,
    pub(crate) videos: Load<Video>,
    /// Videos or Shorts tab of an open channel.
    pub(crate) view: ChannelView,
}

impl Browser {
    pub(crate) fn new() -> Self {
        Self { groups: Load::Idle, open: None, videos: Load::Idle, view: ChannelView::Videos }
    }
}

/// Entries streamed by a background yt-dlp run, plus its result once finished.
pub(crate) type Inbox<T> = Arc<Mutex<(Vec<T>, Option<Result<(), String>>)>>;

/// The video is playing on a receiver that speaks the remote API: the app's controls drive it.
pub(crate) struct Casting {
    pub(crate) name: String,
    pub(crate) remote: cast::Remote,
    /// The last answer of the receiver; None until the first one.
    pub(crate) status: Option<cast::Status>,
    /// Whether the receiver has played it yet: it may take seconds to start, and only after that
    /// does "nothing playing" mean it ended.
    pub(crate) seen: bool,
    pub(crate) started: Instant,
    pub(crate) polling: bool,
    /// A list was sent, so the receiver has a queue: Next and Previous drive it.
    pub(crate) listed: bool,
    /// The videos sent (what the receiver's queue position indexes), and which of them were
    /// already put in the YouTube history.
    pub(crate) items: Vec<Video>,
    pub(crate) marked: std::collections::HashSet<usize>,
    /// How many of `items` the receiver has been sent: it takes 200 at a time, the rest follows
    /// when its queue runs low.
    pub(crate) sent: usize,
}

/// Progress of the Connect YouTube / Test connection flow in Settings.
#[derive(Clone, Default)]
pub(crate) enum Connect {
    #[default]
    Idle,
    /// Steps run 1..=3 in the background: profile, session, feed.
    Running(u8),
    /// The step that failed, and why.
    Failed { step: u8, error: String },
    /// Everything passed; `me` is what YouTube showed for the account.
    Done { me: Me },
}

/// The Connect panel's three checks, in order.
pub(crate) const PROBE_STEPS: [&str; 3] = ["Browser profile found", "YouTube session valid", "Subscription feed reachable"];

impl Connect {
    /// The three checks with their marks, shown while they run and after they finished.
    pub(crate) fn steps(&self) -> Vec<AnyElement> {
        (0..PROBE_STEPS.len())
            .map(|i| {
                let n = i as u8 + 1;
                let (mark, mark_color) = match self {
                    Connect::Running(s) if *s > n => ("✓", themed(TEXT)),
                    Connect::Running(s) if *s == n => ("…", themed(ACCENT)),
                    Connect::Failed { step, .. } if *step > n => ("✓", themed(TEXT)),
                    Connect::Failed { step, .. } if *step == n => ("✕", themed(ACCENT)),
                    Connect::Done { .. } => ("✓", themed(TEXT)),
                    _ => ("·", themed(MUTED)),
                };
                let reached = match self {
                    Connect::Running(s) => *s >= n,
                    Connect::Failed { step, .. } => *step >= n,
                    Connect::Done { .. } => true,
                    Connect::Idle => false,
                };
                div()
                    .px_4()
                    .py(px(1.))
                    .flex()
                    .gap_2()
                    .text_sm()
                    .child(div().w(px(14.)).flex_none().text_color(mark_color).child(mark))
                    .child(div().text_color(if reached { themed(TEXT) } else { themed(MUTED) }).child(PROBE_STEPS[i]))
                    .into_any_element()
            })
            .collect()
    }
}

/// A video saved with Download: listed in the Downloads tab and played from its file.
#[derive(Clone, Serialize, serde::Deserialize)]
pub(crate) struct Saved {
    pub(crate) video: Video,
    pub(crate) path: String,
}

impl Saved {
    /// The library, without entries whose file was deleted or moved outside the app.
    pub(crate) fn load() -> Vec<Saved> {
        let mut list: Vec<Saved> = store::load_data("downloads").unwrap_or_default();
        list.retain(|s| std::path::Path::new(&s.path).exists());
        list
    }
}

/// A row of the player menu's subtitle list.
#[derive(Clone, PartialEq)]
pub(crate) enum SubChoice {
    Off,
    /// A caption track: its language code and whether it is generated.
    Lang(String, bool),
    /// A line that says something (loading, an error), not a choice.
    Info,
}

/// The sleep timer: pause at a time, or let the playing video end without starting another.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Sleep {
    At(Instant),
    EndOfVideo,
}

#[derive(Clone)]
pub(crate) struct Download {
    pub(crate) progress: f32,
    pub(crate) result: Option<Result<String, String>>,
    /// When it finished; the result is shown for a few seconds, then hidden.
    pub(crate) done_at: Option<std::time::Instant>,
}

impl Download {
    /// Status line under the player: progress while running, the result briefly after.
    pub(crate) fn visible_status(&self) -> Option<String> {
        self.done_at.is_none_or(|t| t.elapsed() < Duration::from_secs(6)).then(|| self.status())
    }

    pub(crate) fn status(&self) -> String {
        match &self.result {
            None => format!("Downloading… {:.0}%", self.progress),
            Some(Ok(file)) => format!("Downloaded to {file}"),
            Some(Err(e)) => format!("Download failed: {e}"),
        }
    }
}

/// A draggable 5px bar between two panes.
/// A list that failed (most often without a login) may load again after the login changed.
pub(crate) fn clear_failed<T>(load: &mut Load<T>) {
    if matches!(load, Load::Failed(_)) {
        *load = Load::Idle;
    }
}

/// Friendlier wording for a session YouTube rejected: the browser cookies expired.
/// Whether a list's error reads like a login problem (and not, say, a network one).
pub(crate) fn looks_like_login_error(error: &str) -> bool {
    let e = error.to_lowercase();
    ["sign in", "log in", "login", "cookie", "unauthorized", "403"].iter().any(|p| e.contains(p))
}

pub(crate) fn session_hint(error: String) -> String {
    if error.contains("cookies database") {
        "Couldn't find that browser's profile: is it installed, and has it been opened once?".into()
    } else if error.contains("no YouTube login cookies") {
        "That browser isn't signed in to YouTube. Sign in there, then try again (Chromium-based browsers also need an unlocked keyring).".into()
    } else if error.contains("refused") || error.to_lowercase().contains("sign in") {
        format!("{error} — your YouTube session may have expired; sign in to YouTube in your browser, then try again")
    } else {
        error
    }
}
