mod account;
mod auth;
mod cast;
mod cli;
mod dearrow;
mod embed;
mod http;
mod icons;
mod links;
mod mpris;
mod player;
mod prefetch;
mod proxy;
mod store;
mod thumbs;
mod yt;

use account::{Account, Me, VideoStatus};
use auth::{Auth, BROWSERS};
use embed::Embed;
use gpui::{
    Animation, AnimationExt, HighlightStyle, InteractiveText, StyledText, UnderlineStyle, Pixels, AnyElement, App, Application, ClipboardItem, Bounds, Context, CursorStyle, ElementId, FocusHandle, Hsla, KeyDownEvent, MouseButton,
    MouseMoveEvent, ObjectFit, SharedString, Stateful, Task,
    Transformation, TitlebarOptions, Window, WindowBounds, WindowOptions, canvas, div, img, prelude::*, px, radians, relative, rgb, size, svg,
    uniform_list, ScrollStrategy, UniformListScrollHandle, Bounds as GBounds, Pixels as GPixels,
};
use player::Player;
use serde::{Serialize, de::DeserializeOwned};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use store::{ChannelFlags, ChannelGroup, Config, History, Seen, Settings};
use yt::{Comment, Group, Video, fmt_count, fmt_duration};

const BG: u32 = 0x0f0f0f;
const PANEL: u32 = 0x161616;
const HOVER: u32 = 0x242424;
const BORDER: u32 = 0x2a2a2a;
const TEXT: u32 = 0xe6e6e6;
const MUTED: u32 = 0x8c8c8c;
const ACCENT: u32 = 0xff4e45;

/// Light theme on (set from the settings): `themed` swaps the palette above for `LIGHT`.
static LIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Light counterparts of BG, PANEL, HOVER, BORDER, TEXT, MUTED (ACCENT stays).
fn themed(c: u32) -> gpui::Rgba {
    let light = LIGHT.load(std::sync::atomic::Ordering::Relaxed);
    rgb(match c {
        BG if light => 0xfafafa,
        PANEL if light => 0xf0f0f0,
        HOVER if light => 0xe9e9e9,
        BORDER if light => 0xd9d9d9,
        TEXT if light => 0x1a1a1a,
        MUTED if light => 0x6b6b6b,
        c => c,
    })
}
/// Text and icons on the accent color: light in both themes.
const ON_ACCENT: u32 = 0xf5f5f5;
const ROW_H: f32 = 64.;
/// Height of the column headers (tabs on the left, window buttons on the right), border included.
const HEADER_H: f32 = 41.;
const SEEK_SEGMENTS: usize = 80;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Tab {
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
enum Split {
    Columns,
    Player,
    Continue,
}

/// Comments fetched per page: the first fetch, and each "Load more".
const COMMENTS_PAGE: usize = 40;

/// The "New uploads" pseudo-channel at the top of Subscriptions.
const FEED_ID: &str = "feed";

/// The video's channel as a browsable group, if its channel URL is known.
fn channel_group(v: &Video) -> Option<Group> {
    let url = v.channel_url.clone()?;
    // Used for the cache file name too, so keep it to the id or handle.
    let id = channel_of(v)
        .map(String::from)
        .unwrap_or_else(|| url.trim_end_matches('/').rsplit('/').next().unwrap_or_default().to_string());
    let title = v.channel.clone().unwrap_or_else(|| id.clone());
    Some(Group { id, title, url: format!("{}/videos", url.trim_end_matches('/')), thumb: None, subscribers: None })
}

/// Whether `title` says one of `words` (lower-case) as a word of its own: "art" hides "Art
/// school" but not "start".
fn says_any(title: &str, words: &[String]) -> bool {
    let title = title.to_lowercase();
    let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
    words.iter().any(|w| {
        title.match_indices(w.as_str()).any(|(i, _)| {
            let (before, after) = (title[..i].chars().next_back(), title[i + w.len()..].chars().next());
            // A word that itself starts or ends with a symbol (#shorts) needs no edge there.
            (edge(before) || !w.starts_with(char::is_alphanumeric)) && (edge(after) || !w.ends_with(char::is_alphanumeric))
        })
    })
}

/// Channel id (UC…) of a video, from its channel URL.
fn channel_of(v: &Video) -> Option<&str> {
    v.channel_url.as_deref()?.split("/channel/").nth(1).map(|id| id.trim_end_matches('/'))
}

/// Settings page toggles: label, hint, field.
type Toggle = (&'static str, &'static str, fn(&mut Settings) -> &mut bool);

const TOGGLES: [Toggle; 14] = [
    ("Subscriptions", "Your subscribed channels", |s| &mut s.subscriptions),
    ("Playlists", "Watch later, Liked and your playlists", |s| &mut s.playlists),
    ("History", "What you watched, here and on YouTube", |s| &mut s.history),
    ("Recommendations", "Your YouTube home feed under the player", |s| &mut s.recommendations),
    ("Chapters", "Chapters tab under the player, for videos that have them", |s| &mut s.chapters),
    ("Description", "Description tab under the player, loaded when you open it", |s| &mut s.description),
    ("Transcript", "Transcript tab under the player: the captions as text, searchable; click a line to jump there", |s| &mut s.transcript),
    ("Comments", "Comments tab under the player, loaded when you open it", |s| &mut s.comments),
    ("Downloads tab", "Videos you downloaded, played from the file (also offline); shows once there is one", |s| &mut s.downloads_tab),
    ("Watch later tab", "Your Watch later list under the player (needs your login), loaded when you open it", |s| &mut s.watch_later_tab),
    ("Shorts", "Shorts tab on channels, and Shorts in feeds and search", |s| &mut s.shorts),
    ("Vim mode", "j/k move, Enter opens, h goes back, ⇧F shows click hints; ? lists all keys", |s| &mut s.vim),
    ("Window buttons", "Minimize, maximize and close, top right", |s| &mut s.window_buttons),
    ("Light theme", "Light colors instead of dark", |s| &mut s.light_theme),
];

const PLAYER_TOGGLES: [Toggle; 8] = [
    ("Autoplay next", "Play the next video of the list when one ends", |s| &mut s.autoplay),
    ("Audio only", "Don't fetch or show video, e.g. for music and podcasts", |s| &mut s.audio_only),
    ("Prefer hardware-friendly codecs", "Skip AV1, which many GPUs can't decode, for lower CPU use", |s| &mut s.prefer_hw_codecs),
    ("Hardware decoding", "Decode video on the GPU (mpv --hwdec=auto-safe)", |s| &mut s.hwdec),
    ("Load videos ahead", "Look up the streams of the video under the pointer, the first rows of a list, the head of Up next and the next video, so they start sooner; off: nothing is requested until you play", |s| &mut s.prefetch),
    ("Hover controls on the video", "A bar with play, seek, volume and fullscreen when the pointer is over the video", |s| &mut s.video_controls),
    ("mpv controls and hotkeys", "mpv's own on-screen controls and key bindings over the video; off: only this app's", |s| &mut s.native_controls),
    ("Block in-video ads (SponsorBlock)", "Skip sponsor reads and other segments marked by the community", |s| &mut s.sponsorblock),
];

/// Toggles that do nothing without a login; hidden in Settings until one is connected.
const ACCOUNT_ONLY: [&str; 9] = [
    "Subscriptions",
    "Playlists",
    "History",
    "Watch later tab",
    "Subscribe",
    "Save to playlist",
    "Watch later",
    "Like",
    "Dislike",
];

const SUB_TOGGLES: [Toggle; 2] = [
    ("Subtitles", "Show subtitles on videos (the CC button or V turns them off for one video)", |s| &mut s.subtitles),
    ("Auto-generated captions", "Also use YouTube's automatic and translated captions when a video has none of its own", |s| &mut s.sub_auto),
];
/// Subtitle sizes: label and mpv's `sub-scale`.
const SUB_SCALES: [(&str, f32); 4] = [("Small", 0.8), ("Normal", 1.0), ("Large", 1.4), ("Huge", 1.8)];

const QUALITIES: [u32; 5] = [480, 720, 1080, 1440, 2160];
const NOTIFY_MINUTES: [u32; 4] = [5, 15, 30, 60];

const NOTIFY_TOGGLES: [Toggle; 1] = [(
    "Upload notifications",
    "Desktop notification when a channel with the bell on (in its header) uploads",
    |s| &mut s.notifications,
)];

/// SponsorBlock categories: label, API name.
const SEGMENTS: [(&str, &str); 8] = [
    ("Sponsor", "sponsor"),
    ("Self-promotion", "selfpromo"),
    ("Like/subscribe reminders", "interaction"),
    ("Intro", "intro"),
    ("Credits", "outro"),
    ("Preview/recap", "preview"),
    ("Filler", "filler"),
    ("Non-music in music videos", "music_offtopic"),
];
/// Where the project lives; opened by the button at the bottom of Settings.
const REPO_URL: &str = "https://github.com/antlis/UnbloatedTube";

const SPEEDS: [f32; 6] = [0.75, 1.0, 1.25, 1.5, 1.75, 2.0];

/// Settings text fields: label, hint, field.
const TEXT_FIELDS: [(&str, &str, fn(&mut Settings) -> &mut String); 6] = [
    ("Cast command", "catt -d \"Living Room\" cast {url}", |s| &mut s.cast_command),
    ("Subtitle language", "Code, e.g. en or ru, or several (en,ru); empty uses your system language", |s| &mut s.sub_lang),
    ("Extra mpv options", "e.g. --volume=70 --deband", |s| &mut s.mpv_args),
    ("Download folder", "Empty for your Downloads folder; ~/ works", |s| &mut s.download_dir),
    ("Hide videos with words", "Comma-separated, e.g. reaction, prank, #shorts: hidden from feeds, channels, recommendations and search", |s| &mut s.hide_words),
    ("Proxy address", "socks5://127.0.0.1:9050 (Tor), socks5://127.0.0.1:1080 (ByeDPI), or http://host:port; user:password@ before the host if it needs a login", |s| &mut s.proxy),
];

/// Settings → Network → Connection: label, value.
const CONNECTIONS: [(&str, &str); 3] = [("Direct", "direct"), ("Bypass slowdown", "bypass"), ("Proxy", "proxy")];
/// How Bypass splits the first packet: label, value.
const BYPASS_METHODS: [(&str, &str); 3] = [("TLS + TCP split", "both"), ("TLS split", "tls"), ("TCP split", "tcp")];
/// Proxy presets: label, address (the programs' default ports).
const PROXY_PRESETS: [(&str, &str); 2] = [("Tor", "socks5://127.0.0.1:9050"), ("ByeDPI", "socks5://127.0.0.1:1080")];

/// Route the app's connections as Settings → Network says (see proxy.rs).
fn apply_network(s: &Settings) -> Result<(), String> {
    let route = match s.connection.as_str() {
        "bypass" => proxy::Route::Bypass(match s.bypass_method.as_str() {
            "tls" => proxy::Split::Tls,
            "tcp" => proxy::Split::Tcp,
            _ => proxy::Split::Both,
        }),
        "proxy" if s.proxy.trim().is_empty() => return Ok(proxy::set_route(proxy::Route::Direct)),
        "proxy" => match proxy::parse_upstream(&s.proxy) {
            Ok(up) => proxy::Route::Upstream(up),
            Err(e) => {
                proxy::set_route(proxy::Route::Direct);
                return Err(format!("Proxy address: {e}"));
            }
        },
        _ => proxy::Route::Direct,
    };
    proxy::set_route(route);
    Ok(())
}

const BUTTON_TOGGLES: [Toggle; 13] = [
    ("Back / forward", "Arrows to the video you played before, and back again (Alt+← / Alt+→); the player's own previous / next follow the list", |s| &mut s.history_buttons),
    ("Subscribe", "Subscribe / unsubscribe to the video's channel", |s| &mut s.subscribe_button),
    ("Save to playlist", "Add the video to Watch later or one of your playlists", |s| &mut s.save_button),
    ("Watch later", "Add the video to Watch later in one click (W)", |s| &mut s.watch_later_button),
    ("Like", "Like the video, or remove your like", |s| &mut s.like_button),
    ("Dislike", "Dislike the video, or remove your dislike", |s| &mut s.dislike_button),
    ("Volume", "Mute button and volume bar next to the speed button", |s| &mut s.volume_control),
    ("Subtitles", "CC button: subtitles on / off for the video (V); on the hover bar over the video, or under the player without it (hover controls off, picture-in-picture, casting); language and size are under Subtitles", |s| &mut s.subtitles_button),
    ("Cast", "Send the video to another device (T); the button shows once a Cast command (Settings) or a [cast] target (config.toml) exists", |s| &mut s.cast_button),
    ("Share", "Copy the video's link", |s| &mut s.share_button),
    ("Share at current time", "Copy the video's link so it opens at the current time", |s| &mut s.share_time_button),
    ("Open in browser", "Open the video's page in your default browser", |s| &mut s.browser_button),
    ("Download", "Save the video to your Downloads folder", |s| &mut s.download_button),
];

const INFO_TOGGLES: [Toggle; 4] = [
    ("Views", "View counts on videos (the playing video needs your login)", |s| &mut s.show_views),
    ("Upload date", "When the playing video was posted (needs your login)", |s| &mut s.show_date),
    ("Likes and dislikes", "Counts for the playing video from the Return YouTube Dislike project (dislikes are its estimate; it learns which video you watch)", |s| &mut s.show_votes),
    ("Subscribers", "Subscriber counts of channels", |s| &mut s.show_subs),
];

const DEARROW_TOGGLES: [Toggle; 2] = [
    ("Better titles (DeArrow)", "Titles the community wrote to replace clickbait ones; hover a title for the original. Asks DeArrow by a hash prefix of the video id, so it doesn't learn which video", |s| &mut s.dearrow_titles),
    ("Better thumbnails (DeArrow)", "A frame from the video the community picked, instead of the clickbait thumbnail", |s| &mut s.dearrow_thumbs),
];

/// A list being fetched. `Loading` already holds whatever has arrived (or the previous list
/// during a refresh), so the UI can show it right away.
enum Load<T> {
    Idle,
    Loading(Vec<T>),
    Ready(Vec<T>),
    Failed(String),
}

impl<T> Load<T> {
    fn items(&self) -> &[T] {
        match self {
            Load::Loading(v) | Load::Ready(v) => v,
            _ => &[],
        }
    }
}

/// A tab listing groups (channels or playlists); clicking one shows its videos.
#[derive(Clone, Copy, PartialEq)]
enum Lower {
    Recommended,
    Description,
    Transcript,
    UpNext,
    Chapters,
    Comments,
    WatchLater,
}

/// Keyboard shortcuts (also listed in Settings). Keys reach mpv instead while the pointer is
/// over the video; mpv's own defaults there are similar (Space, arrows, f).
const SHORTCUTS: [(&str, &str); 30] = [
    ("Space / K", "Play / pause"),
    ("← / →", "Back / forward 5 seconds"),
    ("J / L", "Back / forward 10 seconds"),
    ("F", "Fullscreen (Esc or f to leave)"),
    ("M", "Mute"),
    ("V", "Subtitles on / off"),
    ("T", "Cast the video to the first cast target (config.toml)"),
    ("↑ / ↓", "Volume up / down 5%"),
    ("C", "Copy the video's link"),
    ("⇧C", "Copy the link at the current time"),
    ("O", "Open the video in your browser"),
    ("W", "Add the video to Watch later"),
    ("Click / Double-click", "Pause / fullscreen (on the video)"),
    ("N", "Next (Up next first)"),
    ("P", "Previous"),
    ("Alt+← / Alt+→", "Back / forward through the videos you played"),
    ("E", "Lower pane full height, and back"),
    ("⇧E", "Player full height (hide the lower pane), and back"),
    ("[ / ]", "Previous / next tab (header tabs, then the lower pane's)"),
    ("1 - 4", "Switch tab: Subscriptions, Playlists, History, (Downloads,) Settings"),
    ("5 - 9", "Switch lower tab: Recommended, Description, Transcript, Chapters, Watch later, Comments, Up next"),
    ("Tab / ⇧Tab", "Move the focus ring (Enter or Space presses, Esc clears)"),
    ("B", "Hide or show the left column"),
    ("⇧B", "Hide or show the right column"),
    ("/", "Search (a pasted YouTube link plays)"),
    ("Ctrl-v", "Play the YouTube link on the clipboard"),
    ("Ctrl-f", "Filter the list (channels, videos, history)"),
    ("?", "Show these shortcuts"),
    ("Esc", "Close the playlist picker, or go back from a channel"),
    ("Esc", "Close this sheet"),
];

/// Cheatsheet groups: title, how many SHORTCUTS entries it takes (in order), and its column.
const SHEET_GROUPS: [(&str, usize, usize); 2] = [("Playback", 9, 0), ("Navigation", 15, 1)];

/// Vim mode's keys (case matters: ⇧ means Shift).
const VIM_SHORTCUTS: [(&str, &str); 36] = [
    ("Space", "Play / pause"),
    ("← / →", "Back / forward 5 seconds"),
    (", / .", "Back / forward 10 seconds"),
    ("f", "Fullscreen (Esc or f to leave)"),
    ("m", "Mute"),
    ("v", "Subtitles on / off"),
    ("t", "Cast the video to the first cast target (config.toml)"),
    ("+ / -", "Volume up / down 5%"),
    ("n / p", "Next / previous video"),
    ("y y", "Copy the video's link"),
    ("y t", "Copy the link at the current time"),
    ("o", "Open the video in your browser"),
    ("w", "Add the video to Watch later"),
    ("Click / Double-click", "Pause / fullscreen (on the video)"),
    ("j / k", "Move down / up the list"),
    ("g g / ⇧G", "First / last item"),
    ("Ctrl-d / Ctrl-u", "Move 10 down / up"),
    ("Enter / l", "Open or play"),
    ("h / Backspace", "Back"),
    ("⇧H / ⇧L", "Previous / next tab"),
    ("x", "Add the selected video to Up next"),
    ("⇧F", "Click hints: type the label to click"),
    ("Alt+← / Alt+→", "Back / forward through the videos you played"),
    ("e", "Lower pane full height, and back"),
    ("⇧E", "Player full height (hide the lower pane), and back"),
    ("[ / ]", "Previous / next tab (header tabs, then the lower pane's)"),
    ("1 - 4", "Switch tab: Subscriptions, Playlists, History, (Downloads,) Settings"),
    ("5 - 9", "Switch lower tab: Recommended, Description, Transcript, Chapters, Watch later, Comments, Up next"),
    ("Tab / ⇧Tab", "Move the focus ring (Enter or Space presses, Esc clears)"),
    ("b", "Hide or show the left column"),
    ("⇧B", "Hide or show the right column"),
    ("/", "Search (a pasted YouTube link plays)"),
    ("Ctrl-v", "Play the YouTube link on the clipboard"),
    ("Ctrl-f", "Filter the list (channels, videos, history)"),
    ("?", "Show these shortcuts"),
    ("Esc", "Cancel / close / back"),
];
const VIM_SHEET_GROUPS: [(&str, usize, usize); 3] = [("Playback", 10, 0), ("Navigation", 16, 1), ("General", 4, 0)];

/// An entry of the left column's list, for Vim navigation.
#[derive(Clone)]
enum Item {
    Group(Group),
    /// A video and the list it plays from (for Prev / Next).
    Video(Video, Arc<[Video]>),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum ChannelView {
    Videos,
    Shorts,
    /// A channel's live streams (its /streams page).
    Live,
}

struct Browser {
    groups: Load<Group>,
    open: Option<Group>,
    videos: Load<Video>,
    /// Videos or Shorts tab of an open channel.
    view: ChannelView,
}

impl Browser {
    fn new() -> Self {
        Self { groups: Load::Idle, open: None, videos: Load::Idle, view: ChannelView::Videos }
    }
}

/// Entries streamed by a background yt-dlp run, plus its result once finished.
type Inbox<T> = Arc<Mutex<(Vec<T>, Option<Result<(), String>>)>>;

/// The video is playing on a receiver that speaks the remote API: the app's controls drive it.
struct Casting {
    name: String,
    remote: cast::Remote,
    /// The last answer of the receiver; None until the first one.
    status: Option<cast::Status>,
    /// Whether the receiver has played it yet: it may take seconds to start, and only after that
    /// does "nothing playing" mean it ended.
    seen: bool,
    started: Instant,
    polling: bool,
    /// A list was sent, so the receiver has a queue: Next and Previous drive it.
    listed: bool,
    /// The videos sent (what the receiver's queue position indexes), and which of them were
    /// already put in the YouTube history.
    items: Vec<Video>,
    marked: std::collections::HashSet<usize>,
    /// How many of `items` the receiver has been sent: it takes 200 at a time, the rest follows
    /// when its queue runs low.
    sent: usize,
}

struct Unbloated {
    cfg: Arc<Config>,
    settings: Settings,
    dragging: Option<Split>,
    /// Pointer y and Continue watching height when that drag started.
    drag_from: (f32, f32),
    continue_scroll: UniformListScrollHandle,
    tab: Tab,
    subs: Browser,
    yt_history: Load<Video>,
    /// When YouTube's history was last asked for in this session (the startup list is the one
    /// saved last time).
    yt_history_asked: Option<Instant>,
    playlists: Browser,
    recs: Load<Video>,
    /// Watch later, for its tab under the player; fetched when the tab is first shown.
    watch_later: Load<Video>,
    /// Description of the video `description_for` (one item); fetched when its tab is first shown.
    description: Load<String>,
    description_for: Option<String>,
    /// Caption lines (start, text) of the video `transcript_for`; fetched when the Transcript tab
    /// is first shown. `transcript_query` searches them; `transcript_shown` is the line scrolled
    /// to when the list appeared (it follows the video only then, not while you read).
    transcript: Load<(f64, String)>,
    transcript_for: Option<String>,
    transcript_query: String,
    transcript_focus: FocusHandle,
    transcript_scroll: UniformListScrollHandle,
    transcript_shown: Option<usize>,
    /// Comments of the video `comments_for`; fetched when the Comments tab is first shown.
    comments: Load<Comment>,
    comments_for: Option<String>,
    /// How many comments the last fetch asked for; "Load more" raises it.
    comments_limit: usize,
    /// Latest uploads across subscriptions, for the "New uploads" row and per-channel counts.
    feed: Load<Video>,
    /// The logged-out home: a random anonymous mix (see `yt::anonymous`).
    anon: Load<Video>,
    seen: Seen,
    query: String,
    search: Load<Video>,
    search_focus: FocusHandle,
    save_filter: String,
    /// Ids of the playlists the open Save list may offer (YouTube's own list of ones you can add to):
    /// None while it loads, Some(None) when YouTube didn't say (then every playlist is shown).
    save_editable: Option<Option<HashSet<String>>>,
    save_focus: FocusHandle,
    /// Focused when no text field is, so keyboard shortcuts work.
    root_focus: FocusHandle,
    /// Videos queued with "+", played before the list's next video.
    up_next: Vec<Video>,
    /// Which list is shown under the player.
    lower: Lower,
    /// Vim mode: selected index in the left column's list, which list it is, and its scroll.
    vim_cursor: usize,
    vim_list: String,
    vim_scroll: UniformListScrollHandle,
    /// Chapters list scroll, and the chapter it last scrolled to.
    chapters_scroll: UniformListScrollHandle,
    chapter_shown: Option<usize>,
    /// First "g" of "gg" was pressed.
    vim_g: bool,
    /// First "y" of "yy" was pressed.
    vim_y: bool,
    /// Selection per list, restored when going back to it.
    vim_positions: HashMap<String, usize>,
    /// Clickable elements and their click actions, collected every frame (Vim mode) for `f`.
    hint_targets: Rc<RefCell<Vec<HintTarget>>>,
    /// The element the Tab focus ring is on (where it was last seen); None when the ring is off.
    kb_focus: Option<GBounds<GPixels>>,
    /// `[` / `]` are cycling in the lower pane's tabs (otherwise in the header tabs).
    cycle_lower: bool,
    /// Hint mode: the targets when `f` was pressed, and the letters typed so far.
    hints: Option<(Vec<HintTarget>, String)>,
    /// Picture-in-picture: mpv plays in its own small always-on-top window.
    pip: bool,
    /// Set while the video plays on a cast receiver (see `Casting`).
    casting: Option<Casting>,
    /// Closing the window was asked while casting: the dialog offers to stop the receiver too.
    quit_prompt: bool,
    /// Live filter for the left list (channels, a channel's videos, History), and its focus.
    list_filter: String,
    filter_focus: FocusHandle,
    /// Cursor and selection of the text field being edited.
    caret: Caret,
    /// Search mode: the header shows a full-width search field instead of the tabs.
    searching: bool,
    /// Tab to return to when leaving search.
    prev_tab: Tab,
    /// Last queries, newest first.
    recent_searches: Vec<String>,
    /// Muted / notify-on-upload channels.
    flags: ChannelFlags,
    /// Re-checks the feed for notifications every few minutes.
    _notify_poll: Task<()>,
    /// Channel groups, and the one Subscriptions is filtered to.
    groups: Vec<ChannelGroup>,
    active_group: Option<String>,
    /// The group editor (membership chips) is open in the channel header.
    editing_groups: bool,
    /// Name being typed for a new group, and its field's focus.
    new_group: Option<String>,
    new_group_focus: FocusHandle,
    /// Group waiting for the second click on "Delete?".
    confirm_delete_group: Option<String>,
    /// Channel waiting for the second click on "Unsubscribe?".
    confirm_unsub: Option<String>,
    /// The lower pane (Recommended, Chapters, Up next) takes the whole right column.
    lower_full: bool,
    /// The lower pane is hidden; the player takes the whole right column.
    player_full: bool,
    /// The left column is hidden (its width setting is kept for when it comes back).
    left_collapsed: bool,
    /// The right column (player and lower pane) is hidden; the left one takes the window.
    right_collapsed: bool,
    /// Right-click menu on a subscription: the channel, where it was opened and when (it closes by itself).
    channel_menu: Option<(Group, gpui::Point<Pixels>, Instant)>,
    /// Right-click menu of a video in any list: the video, where it opened, and whether mpv's
    /// window must hide because the menu can reach over it (clicks outside the left column), and
    /// when the pointer was last on it: like the subscription menu, it closes by itself after the
    /// pointer has been away for 2 seconds.
    video_menu: Option<(Video, gpui::Point<Pixels>, bool, Instant)>,
    /// Right-click menu of a playlist in the list of playlists (Cast); closes like the others.
    playlist_menu: Option<(Group, gpui::Point<Pixels>, Instant)>,
    /// The list the right-clicked video is in, for "Cast from here"; None on the playing video.
    menu_queue: Option<Arc<[Video]>>,
    /// The playlists that menu offers for the video id, with whether it is in each; None while loading.
    menu_lists: Option<(String, Option<Vec<account::SaveOption>>)>,
    /// That menu was opened by right-clicking the playing video, so it also has the player rows.
    player_menu: bool,
    /// A quality picked for one video (its id, and a height or 0 for audio only) instead of the
    /// settings' maximum; dropped when another video starts.
    quality: Option<(String, u32)>,
    /// Likes and dislikes of the video `votes.0` (None while they load, or when there are none).
    votes: Option<(String, Option<(u64, u64)>)>,
    /// Settings → Network → Test connection's result.
    net_test: Option<String>,
    /// DeArrow's answer per video id; None while asked (or after a failure, not asked again).
    dearrow: HashMap<String, Option<dearrow::Branding>>,
    /// The player menu's quality list is open.
    quality_menu: bool,
    sleep: Option<Sleep>,
    /// Captions picked for one video in the player menu: its id, the language code and whether
    /// generated; the subtitles and the Transcript tab use them instead of Settings → Subtitles.
    sub_pick: Option<(String, String, bool)>,
    /// The player menu's subtitle list is open.
    sub_menu: bool,
    /// The caption tracks of the video `caption_langs.0` (None while they load).
    caption_langs: Option<(String, Option<Result<Vec<yt::CaptionLang>, String>>)>,
    /// The player menu's sleep timer list is open.
    sleep_menu: bool,
    /// The pointer is over the menu (it closes 2 seconds after the pointer is away).
    menu_hovered: bool,
    /// The keyboard shortcuts card (opened with ?).
    show_keys: bool,
    settings_filter: String,
    settings_focus: FocusHandle,
    /// Focus of the settings text fields, in TEXT_FIELDS order.
    field_focus: [FocusHandle; TEXT_FIELDS.len()],
    /// Latest fetch per list; older fetches of the same list are ignored.
    generations: HashMap<&'static str, u64>,
    history: History,
    current: Option<Video>,
    player: Player,
    /// When we last set the volume: mpv's reported volume is ignored for a moment after.
    volume_set: Instant,
    /// X11 child window mpv renders into; None until created, or on Wayland.
    embed: Option<Rc<RefCell<Embed>>>,
    /// The last-watched video has been loaded (paused) into mpv at startup.
    preloaded: bool,
    /// The video the pointer rests on, and when each video was last resolved ahead of time.
    hover_video: Option<String>,
    /// Subscriptions shows the streams live now (the Live tab) instead of the channel list.
    subs_live: bool,
    /// Videos played, oldest first (seeded from the watch history), and where the current one is:
    /// what the back / forward buttons walk.
    nav: Vec<Video>,
    nav_pos: Option<usize>,
    nav_moving: bool,
    prefetched: HashMap<String, Instant>,
    prefetching: usize,
    /// Videos or Shorts list of the History tab.
    history_view: ChannelView,
    /// Start time of a pasted link, taken by the next `start`.
    link_start: Option<f64>,
    /// A video whose captions the next tick should fetch.
    subs_wanted: Option<String>,
    /// Captions being downloaded for this video id.
    subs_loading: Option<String>,
    /// Downloaded captions, waiting for mpv to have that video open.
    subs_pending: Option<(String, PathBuf)>,
    /// The video id we found no captions for.
    subs_none: Option<String>,
    fullscreen: bool,
    /// A video was requested and mpv hasn't started playing it yet.
    loading: bool,
    /// When the current video was handed to mpv (for `UNBLOATEDTUBE_TIMING`).
    load_started: Instant,
    /// Since when the playing video hasn't moved, and from where (see `watch_stall`).
    stall: Option<(Instant, f64)>,
    /// The video reloaded for not moving, and whether that was said to have failed too.
    stall_retried: Option<(String, bool)>,
    /// Hide mpv's window while loading, so the old video's last frame doesn't linger.
    /// Not on a fresh mpv start: mpv keeps its window unmapped if ours is hidden then.
    hide_while_loading: bool,
    /// The list the current video was picked from, for Prev / Next.
    queue: Arc<[Video]>,
    state: Option<player::State>,
    thumbs_requested: HashSet<String>,
    /// Thumbnails known to be on disk: no file check for them on every frame.
    thumbs_ready: HashSet<String>,
    /// Thumbnails whose download failed: shown as a plain box, not a pulsing skeleton.
    thumbs_failed: HashSet<String>,
    /// Logged-in account for subscribe / like / save; loaded on first use.
    account: Option<Arc<Account>>,
    /// The Connect YouTube flow (Settings, and offered on first run).
    connect: Connect,
    /// The browser the Connect panel has selected (yt-dlp spec, e.g. "firefox").
    connect_browser: String,
    /// Pending cookies.txt path in Settings → Advanced.
    import_path: String,
    import_focus: FocusHandle,
    /// Result of the last cookies.txt import, shown under the field.
    import_msg: Option<(bool, String)>,
    /// Like/subscription state of the video with this id.
    status: Option<(String, VideoStatus)>,
    /// Video whose state was last requested, so a failure isn't retried every tick.
    status_requested: Option<String>,
    /// Video whose end already triggered autoplay.
    ended: Option<String>,
    /// The "Save to playlist" overlay is open.
    saving: bool,
    /// Result of the last account action, shown next to its buttons.
    notice: Option<String>,
    /// The notice as last seen by `tick`, and when it appeared; it's cleared after a few seconds.
    notice_seen: Option<(String, std::time::Instant)>,
    /// Downloads by video id: progress, then the saved file path or an error.
    downloads: HashMap<String, Download>,
    /// Videos saved with Download, newest first: the Downloads tab (stored as `downloads.json`).
    library: Vec<Saved>,
    ticks: u32,
    _poll: Task<()>,
}

/// Progress of the Connect YouTube / Test connection flow in Settings.
#[derive(Clone, Default)]
enum Connect {
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
const PROBE_STEPS: [&str; 3] = ["Browser profile found", "YouTube session valid", "Subscription feed reachable"];

impl Connect {
    /// The three checks with their marks, shown while they run and after they finished.
    fn steps(&self) -> Vec<AnyElement> {
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

impl Unbloated {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let history = History::load();
        // Back goes through what was watched before (the newest, the last played, is where we start).
        let nav: Vec<Video> = history.items.iter().take(50).rev().map(|w| w.video.clone()).collect();
        let nav_pos = nav.len().checked_sub(1);
        let settings = Settings::load();
        // Before anything goes out: lists, thumbnails and mpv follow Settings → Network.
        if let Err(e) = apply_network(&settings) {
            eprintln!("unbloatedtube: {e}");
        }
        let cfg = Arc::new(Config::load());
        let volume = settings.volume;
        LIGHT.store(settings.light_theme, std::sync::atomic::Ordering::Relaxed);
        let root_focus = cx.focus_handle();
        window.focus(&root_focus);
        // A close from the window manager asks the same question as our close button.
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            this.update(cx, |this, cx| {
                let casting = this.casting.is_some();
                this.quit_prompt |= casting;
                cx.notify();
                !casting
            })
            .unwrap_or(true)
        });
        // Logged out: no account tabs, no restored video — the anonymous home takes over.
        let restore = cfg.has_auth().then(|| history.last()).flatten().map(|w| w.video.clone());
        let tab = if cfg.has_auth() {
            [(settings.subscriptions, Tab::Subscriptions), (settings.playlists, Tab::Playlists), (settings.history, Tab::History)]
                .into_iter()
                .find_map(|(on, tab)| on.then_some(tab))
                .unwrap_or(Tab::Settings)
        } else {
            Tab::Subscriptions
        };
        // Refresh the feed every few minutes while some channel has notifications on.
        let notify_poll = cx.spawn(async move |this, cx| {
            loop {
                let Ok(minutes) = this.update(cx, |this, _| this.settings.notify_minutes.max(1)) else { break };
                cx.background_executor().timer(Duration::from_secs(minutes as u64 * 60)).await;
                let res = this.update(cx, |this, cx| {
                    let wanted = this.settings.notifications && !this.flags.notify.is_empty() && this.cfg.has_auth();
                    if wanted && !matches!(this.feed, Load::Loading(_)) {
                        this.fetch(cx, "feed", |s| &mut s.feed, Some("feed".into()), |cfg, on| yt::feed(cfg, on));
                    }
                });
                if res.is_err() {
                    break;
                }
            }
        });
        let poll = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(250)).await;
                let Ok(socket) = this.update(cx, |this, _| this.player.socket_if_alive()) else { break };
                // mpv can take seconds to answer while it opens a video; ask off the UI thread.
                let state = match socket {
                    Some(socket) => blocking::unblock(move || { player::query(&socket) }).await,
                    None => None,
                };
                if this.update_in(cx, |this, window, cx| this.tick(state, window, cx)).is_err() {
                    break;
                }
            }
        });
        // Preselect the system default browser if it's one of ours, else the first found in PATH.
        let connect_browser = Auth::default_browser()
            .filter(|s| Auth::on_path(s))
            .or_else(|| BROWSERS.iter().map(|(_, spec, _)| *spec).find(|spec| Auth::on_path(spec)))
            .unwrap_or(BROWSERS[0].1)
            .to_string();
        let mut app = Self {
            cfg,
            settings,
            dragging: None,
            drag_from: (0., 0.),
            continue_scroll: UniformListScrollHandle::new(),
            tab,
            subs: Browser::new(),
            yt_history: Load::Idle,
            yt_history_asked: None,
            playlists: Browser::new(),
            recs: Load::Idle,
            watch_later: Load::Idle,
            description: Load::Idle,
            description_for: None,
            transcript: Load::Idle,
            transcript_for: None,
            transcript_query: String::new(),
            transcript_focus: cx.focus_handle(),
            transcript_scroll: UniformListScrollHandle::new(),
            transcript_shown: None,
            comments: Load::Idle,
            comments_for: None,
            comments_limit: COMMENTS_PAGE,
            feed: Load::Idle,
            anon: Load::Idle,
            seen: Seen::load(),
            query: String::new(),
            search: Load::Idle,
            search_focus: cx.focus_handle(),
            save_filter: String::new(),
            save_editable: None,
            save_focus: cx.focus_handle(),
            root_focus,
            up_next: store::load_data("up_next").unwrap_or_default(),
            lower: Lower::Recommended,
            show_keys: false,
            confirm_unsub: None,
            lower_full: false,
            player_full: false,
            left_collapsed: false,
            right_collapsed: false,
            channel_menu: None,
            video_menu: None,
            playlist_menu: None,
            menu_queue: None,
            player_menu: false,
            quality: None,
            votes: None,
            net_test: None,
            dearrow: HashMap::new(),
            quality_menu: false,
            sleep: None,
            sub_pick: None,
            sub_menu: false,
            caption_langs: None,
            sleep_menu: false,
            menu_lists: None,
            menu_hovered: false,
            pip: false,
            casting: None,
            quit_prompt: false,
            searching: false,
            list_filter: String::new(),
            filter_focus: cx.focus_handle(),
            caret: Caret::default(),
            prev_tab: Tab::Subscriptions,
            recent_searches: store::load_data("searches").unwrap_or_default(),
            groups: store::load_data("groups").unwrap_or_default(),
            flags: store::load_data("channel_flags").unwrap_or_default(),
            _notify_poll: notify_poll,
            active_group: None,
            editing_groups: false,
            new_group: None,
            new_group_focus: cx.focus_handle(),
            confirm_delete_group: None,
            vim_cursor: 0,
            vim_list: String::new(),
            vim_scroll: UniformListScrollHandle::new(),
            chapters_scroll: UniformListScrollHandle::new(),
            chapter_shown: None,
            vim_g: false,
            vim_y: false,
            vim_positions: HashMap::new(),
            hint_targets: Rc::new(RefCell::new(Vec::new())),
            kb_focus: None,
            cycle_lower: false,
            hints: None,
            settings_filter: String::new(),
            settings_focus: cx.focus_handle(),
            field_focus: std::array::from_fn(|_| cx.focus_handle()),
            generations: HashMap::new(),
            current: restore,
            history,
            player: Player::new(volume),
            volume_set: Instant::now(),
            embed: None,
            preloaded: false,
            hover_video: None,
            subs_live: false,
            nav,
            nav_pos,
            nav_moving: false,
            prefetched: HashMap::new(),
            prefetching: 0,
            history_view: ChannelView::Videos,
            link_start: None,
            subs_wanted: None,
            subs_loading: None,
            subs_pending: None,
            subs_none: None,
            fullscreen: false,
            loading: false,
            load_started: Instant::now(),
            stall: None,
            stall_retried: None,
            hide_while_loading: false,
            queue: Arc::new([]),
            state: None,
            thumbs_requested: HashSet::new(),
            thumbs_ready: HashSet::new(),
            thumbs_failed: HashSet::new(),
            account: None,
            connect: Connect::Idle,
            connect_browser,
            import_path: String::new(),
            import_focus: cx.focus_handle(),
            import_msg: None,
            status: None,
            status_requested: None,
            ended: None,
            saving: false,
            notice: None,
            notice_seen: None,
            downloads: HashMap::new(),
            library: Saved::load(),
            ticks: 0,
            _poll: poll,
        };
        app.load_tab(cx);
        if app.settings.subscriptions && app.cfg.has_auth() && !matches!(app.feed, Load::Loading(_)) {
            app.fetch(cx, "feed", |s| &mut s.feed, Some("feed".into()), |cfg, on| yt::feed(cfg, on));
        }
        if app.settings.recommendations {
            app.load_recs(cx);
        }
        // Cached YouTube history (no network) so watched videos are dimmed from the start;
        // the History tab refreshes it. Account-derived, so not while logged out.
        if app.settings.history && app.cfg.has_auth() {
            if let Some(items) = store::load_list("history") {
                app.yt_history = Load::Ready(items);
            }
        }
        if app.current.is_none() && app.settings.history && app.cfg.has_auth() {
            // No local history yet: fall back to YouTube's own history for "last watched".
            app.load_yt_history(cx);
        }
        app
    }

    /// Stream a yt-dlp listing into `slot` in the background. New entries appear as they arrive,
    /// unless the slot already shows a list (cached or a refresh), which stays until the new one
    /// is complete. With `cache` (a file name), the finished list is saved and shown instantly
    /// next time. `key` identifies the slot: a newer fetch for the same key supersedes this one.
    fn fetch<T: Clone + Send + Serialize + DeserializeOwned + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        key: &'static str,
        slot: fn(&mut Self) -> &mut Load<T>,
        cache: Option<String>,
        f: impl FnOnce(&Config, &mut dyn FnMut(T)) -> Result<(), String> + Send + 'static,
    ) {
        let generation = self.generations.get(key).map_or(0, |g| g + 1);
        self.generations.insert(key, generation);
        if let (Some(name), Load::Idle) = (&cache, slot(self)) {
            // The list caches belong to the account; a logged-out session shows the
            // connect hint instead of last time's lists. The anonymous home ("anon")
            // has no account behind it and is kept across sessions.
            if self.cfg.has_auth() || name == "anon" {
                if let Some(items) = store::load_list(name) {
                    *slot(self) = Load::Ready(items);
                    self.after_load(key, cx);
                }
            }
        }
        let keep_old = !slot(self).items().is_empty();
        let old = match std::mem::replace(slot(self), Load::Idle) {
            Load::Ready(v) | Load::Loading(v) if keep_old => v,
            _ => Vec::new(),
        };
        *slot(self) = Load::Loading(old);

        let inbox: Inbox<T> = Default::default();
        let (cfg, sink) = (self.cfg.clone(), inbox.clone());
        blocking::unblock(move || {
                let res = f(&cfg, &mut |item| sink.lock().unwrap().0.push(item));
                sink.lock().unwrap().1 = Some(res);
            })
            .detach();
        cx.spawn(async move |this, cx| {
            let mut fresh = Vec::new();
            loop {
                cx.background_executor().timer(Duration::from_millis(150)).await;
                let (new, done) = {
                    let mut inbox = inbox.lock().unwrap();
                    (std::mem::take(&mut inbox.0), inbox.1.take())
                };
                let finished = done.is_some();
                let current = this.update(cx, |this, cx| {
                    if this.generations.get(key) != Some(&generation) {
                        return false;
                    }
                    if keep_old {
                        fresh.extend(new);
                    } else if let Load::Loading(v) = slot(this) {
                        if !new.is_empty() {
                            v.extend(new);
                            cx.notify();
                        }
                    }
                    if let Some(res) = done {
                        let items = match std::mem::replace(slot(this), Load::Idle) {
                            Load::Loading(v) if !keep_old => v,
                            // A failed refresh keeps showing the previous list.
                            Load::Loading(old) if fresh.is_empty() && res.is_err() => old,
                            _ => std::mem::take(&mut fresh),
                        };
                        let ok = res.is_ok();
                        let failure = res.as_ref().err().filter(|_| items.is_empty()).cloned();
                        *slot(this) = match res {
                            Err(e) if items.is_empty() => Load::Failed(e),
                            _ => Load::Ready(items),
                        };
                        if let Some(e) = failure {
                            this.note_login_failure(key, &e);
                        }
                        this.after_load(key, cx);
                        if let (Some(name), true) = (&cache, ok) {
                            store::save_list(name, slot(this).items());
                        }
                        cx.notify();
                    }
                    true
                });
                if finished || !current.unwrap_or(false) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// An account list failed like a login that stopped working (expired cookies, a browser
    /// profile that is gone): show it in Settings → Account, where Try again / Choose another
    /// browser are, instead of leaving only failed lists.
    fn note_login_failure(&mut self, key: &str, error: &str) {
        if ["subs", "feed", "history", "playlists", "recs"].contains(&key) {
            self.login_failed(error);
        }
    }

    fn login_failed(&mut self, error: &str) {
        if self.cfg.has_auth() && looks_like_login_error(error) && matches!(self.connect, Connect::Idle) {
            // The browser's cookies couldn't be read (step 1), or YouTube didn't accept them (2).
            let step = if error.to_lowercase().contains("cookie") { 1 } else { 2 };
            self.connect = Connect::Failed { step, error: session_hint(error.to_string()) };
        }
    }

    fn after_load(&mut self, key: &str, cx: &mut Context<Self>) {
        match key {
            "subs" => {
                if let Load::Ready(g) = &mut self.subs.groups {
                    g.sort_by_key(|g| g.title.to_lowercase());
                }
            }
            "history" if self.current.is_none() => self.current = self.yt_history.items().iter().find(|v| !v.short).cloned(),
            // Logged out: a random video waits in the player (not playing), like the last
            // watched one does with a login.
            "anon" if self.current.is_none() && !self.cfg.has_auth() => {
                let pool: Vec<&Video> = self.anon.items().iter().filter(|v| !v.short).collect();
                let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos() as usize);
                self.current = (!pool.is_empty()).then(|| pool[nanos % pool.len()].clone());
                // Only shown: loading it into mpv would be harmless logged out, but a login
                // later makes yt-dlp mark it watched on YouTube.
                self.preloaded = true;
                // Recommended then shows that video's channel instead of repeating the home list.
                self.load_recs(cx);
            }
            "feed" => {
                if !self.seen.baseline && matches!(self.feed, Load::Ready(_)) {
                    // First run: what's in the feed now is old news; count only what comes after.
                    self.seen.ids.extend(self.feed.items().iter().map(|v| v.id.clone()));
                    self.seen.baseline = true;
                    self.seen.save();
                }
                self.notify_uploads();
            }
            _ => {}
        }
    }

    fn load_tab(&mut self, cx: &mut Context<Self>) {
        match self.tab {
            Tab::Settings => {}
            Tab::Downloads => self.prune_library(),
            Tab::Search => self.run_search(cx),
            Tab::History if !self.cfg.has_auth() => self.load_anon(cx),
            Tab::History => {
                if self.cfg.has_auth() {
                    self.load_yt_history(cx);
                }
            }
            tab => match self.browser(tab).open.clone() {
                Some(_) => self.load_group_videos(tab, cx),
                None if !self.cfg.has_auth() => self.load_anon(cx),
                None if tab == Tab::Subscriptions => {
                    self.fetch(cx, "subs", |s| &mut s.subs.groups, Some("subs".into()), |cfg, on| yt::subscriptions(cfg, on));
                    if !matches!(self.feed, Load::Loading(_)) {
                        self.fetch(cx, "feed", |s| &mut s.feed, Some("feed".into()), |cfg, on| yt::feed(cfg, on));
                    }
                }
                None => {
                    self.fetch(cx, "playlists", |s| &mut s.playlists.groups, Some("playlists".into()), |cfg, on| yt::playlists(cfg, on));
                }
            },
        }
    }

    /// YouTube's watch history (cached; the cache shows until the fresh one is complete).
    fn load_yt_history(&mut self, cx: &mut Context<Self>) {
        self.yt_history_asked = Some(Instant::now());
        let account = self.account.clone();
        self.fetch(cx, "history", |s| &mut s.yt_history, Some("history".into()), move |cfg, on| yt::history(cfg, account, on));
    }

    /// The logged-out home: a fresh random mix; the cached one shows until it arrives.
    fn load_anon(&mut self, cx: &mut Context<Self>) {
        self.fetch(cx, "anon", |s| &mut s.anon, Some("anon".into()), |cfg, on| yt::anonymous(cfg, on));
    }

    fn run_search(&mut self, cx: &mut Context<Self>) {
        let query = self.query.trim().to_string();
        if query.is_empty() {
            return;
        }
        self.tab = Tab::Search;
        self.recent_searches.retain(|q| *q != query);
        self.recent_searches.insert(0, query.clone());
        self.recent_searches.truncate(10);
        store::save_data("searches", &self.recent_searches);
        // A new query replaces the old results right away instead of after it finishes.
        self.search = Load::Idle;
        self.fetch(cx, "search", |s| &mut s.search, None, move |cfg, on| yt::search(cfg, &query, on));
    }

    fn search_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match edit_text(&mut self.caret, "search", &mut self.query, ev, cx) {
            Edit::Submit => match yt::parse_link(&self.query) {
                // A pasted YouTube link plays instead of being searched for.
                Some(link) => {
                    self.query.clear();
                    self.close_search(window, cx);
                    self.open_link(link, cx);
                }
                None => {
                    self.run_search(cx);
                    window.blur();
                }
            },
            Edit::Cancel => self.close_search(window, cx),
            Edit::Changed => {}
            Edit::Moved => {}
            // Arrows etc. inside the field mustn't reach the shortcuts.
            Edit::Ignored => cx.stop_propagation(),
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Open a YouTube link: a video plays, a channel or playlist opens in the left column.
    fn open_link(&mut self, link: yt::YtLink, cx: &mut Context<Self>) {
        match link {
            yt::YtLink::Video(video) => self.open_video_link(video, cx),
            yt::YtLink::Channel { url } => {
                let id = url.trim_end_matches('/').rsplit('/').next().unwrap_or_default().to_string();
                let channel = Group { title: id.clone(), url: format!("{url}/videos"), id, thumb: None, subscribers: None };
                self.show_channel(channel, cx);
            }
            yt::YtLink::Playlist(id) => {
                let url = format!("https://www.youtube.com/playlist?list={id}");
                let playlist = Group { title: "Playlist".into(), id, url, thumb: None, subscribers: None };
                self.tab = Tab::Playlists;
                self.open_group(Tab::Playlists, playlist, cx);
            }
        }
        cx.notify();
    }

    /// Play a video link: yt-dlp looks the video up, then it plays like a picked one.
    fn open_video_link(&mut self, link: yt::VideoLink, cx: &mut Context<Self>) {
        self.notice = Some("Opening the link…".into());
        let (cfg, id) = (self.cfg.clone(), link.id.clone());
        let task = blocking::unblock(move || { yt::video(&cfg, &id) });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(video) => {
                        this.notice = None;
                        this.link_start = link.start;
                        this.play(video, None, cx);
                    }
                    Err(e) => this.notice = Some(format!("Couldn't open the link: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Enter search mode (/ or the search icon): full-width field, focused.
    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.left_collapsed = false;
        if !self.searching {
            self.prev_tab = if self.tab == Tab::Search { Tab::Subscriptions } else { self.tab };
            self.searching = true;
            self.tab = Tab::Search;
        }
        window.focus(&self.search_focus);
        cx.notify();
    }

    /// Leave search mode, back to the tab it was opened from.
    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.searching = false;
        self.tab = self.prev_tab;
        window.blur();
        cx.notify();
    }

    /// Search mode with nothing searched yet (or the field cleared): recent queries.
    fn render_recent_searches(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.recent_searches.is_empty() {
            return self.status("Type and press Enter to search YouTube.");
        }
        div()
            .flex()
            .flex_col()
            .py_2()
            .child(div().px_3().pb_1().text_xs().text_color(themed(MUTED)).child("RECENT SEARCHES"))
            .children(self.recent_searches.iter().enumerate().map(|(i, q)| {
                let q = q.clone();
                div()
                    .id(("recent", i))
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_3()
                    .text_sm()
                    .text_color(themed(TEXT))
                    .cursor_pointer()
                    .hover(|d| d.bg(themed(HOVER)))
                    .child(svg().path(icons::path("recent")).size(px(14.)).text_color(themed(MUTED)))
                    .child(q.clone())
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, window, cx| {
                        this.query = q.clone();
                        this.run_search(cx);
                        window.blur();
                    })
            }))
            .child(
                div()
                    .id("clear-recent")
                    .px_3()
                    .pt_2()
                    .text_xs()
                    .text_color(themed(MUTED))
                    .cursor_pointer()
                    .hover(|d| d.text_color(themed(TEXT)))
                    .child("Clear recent searches")
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                        this.recent_searches.clear();
                        store::save_data("searches", &this.recent_searches);
                        cx.notify();
                    }),
            )
            .into_any_element()
    }

    fn open_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.saving = true;
        self.save_filter.clear();
        self.save_editable = None;
        if let Some(video) = self.current.as_ref().map(|v| v.id.clone()) {
            let id = video.clone();
            self.with_account(cx, move |a| a.save_options(&id), move |this, res, cx| {
                // Ignore an answer for a video the list has moved on from.
                if this.saving && this.current.as_ref().is_some_and(|c| c.id == video) {
                    this.save_editable = Some(res.ok().map(|o| o.into_iter().map(|o| o.id).collect()));
                    cx.notify();
                }
            });
        } else {
            self.save_editable = Some(None);
        }
        window.focus(&self.save_focus);
        if matches!(self.playlists.groups, Load::Idle) {
            self.fetch(cx, "playlists", |s| &mut s.playlists.groups, Some("playlists".into()), |cfg, on| yt::playlists(cfg, on));
        }
        self.sync_embed();
        cx.notify();
    }

    fn close_save(&mut self, cx: &mut Context<Self>) {
        self.saving = false;
        self.sync_embed();
        cx.notify();
    }

    /// Playlists you can save to, narrowed by the typed filter.
    fn save_targets(&self) -> Vec<Group> {
        let filter = self.save_filter.to_lowercase();
        self.playlists
            .groups
            .items()
            .iter()
            // "Liked videos" isn't a playlist you can add to, nor are ones you only saved from others
            // (Watch later is not in YouTube's list, and always works).
            .filter(|g| g.id != "LL" && g.title.to_lowercase().contains(&filter))
            .filter(|g| match &self.save_editable {
                Some(Some(editable)) => g.id == "WL" || editable.contains(&g.id),
                Some(None) => true,
                None => false,
            })
            .cloned()
            .collect()
    }

    /// Type to filter, Enter saves to the first match, Esc closes.
    fn save_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        match edit_text(&mut self.caret, "save", &mut self.save_filter, ev, cx) {
            Edit::Submit => {
                if let Some(g) = self.save_targets().into_iter().next() {
                    self.save_to(g, cx);
                }
            }
            Edit::Cancel => self.close_save(cx),
            Edit::Changed => {}
            Edit::Moved => {}
            Edit::Ignored => {
                cx.stop_propagation();
                return;
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Move the video into / out of its own always-on-top window (restarts mpv in place).
    /// Right click → Play in separate window: the video in an mpv window of its own, from where
    /// you left it; the app's player keeps what it plays.
    fn play_separate(&mut self, video: &Video, cx: &mut Context<Self>) {
        let start = self.history.position(&video.id);
        self.notice = Some(match player::play_separate(&self.cfg, &self.settings, &self.media_path(video), start) {
            Ok(()) => format!("Playing \"{}\" in a separate window", video.title),
            Err(e) => e,
        });
        cx.notify();
    }

    fn toggle_pip(&mut self, cx: &mut Context<Self>) {
        self.pip = !self.pip;
        if let (true, Some(video)) = (self.player.alive(), self.current.clone()) {
            let paused = self.state.as_ref().is_some_and(|s| s.paused);
            self.start(video, paused);
        }
        self.sync_embed();
        cx.notify();
    }

    /// Show mpv's window only when it has something current to show and nothing covers it.
    fn sync_embed(&mut self) {
        let visible = self.player.alive()
            && !(self.loading && self.hide_while_loading)
            && !self.saving
            && !self.show_keys
            && !self.pip
            && !self.video_menu.as_ref().is_some_and(|m| m.2)
            && self.casting.is_none()
            && !self.lower_full
            && !self.right_collapsed
            // The connect screen is a GPUI overlay; the X11 child window would float above it.
            && !self.welcome_visible()
            // Audio only: keep showing the thumbnail.
            && !self.settings.audio_only
            && self.picked_quality() != Some(0);
        if let Some(e) = &self.embed {
            e.borrow_mut().set_visible(visible);
        }
    }

    fn browser(&mut self, tab: Tab) -> &mut Browser {
        if tab == Tab::Subscriptions { &mut self.subs } else { &mut self.playlists }
    }

    fn browser_ref(&self, tab: Tab) -> &Browser {
        if tab == Tab::Subscriptions { &self.subs } else { &self.playlists }
    }

    /// Per channel id, feed videos you haven't seen in its list or played; total under FEED_ID.
    /// One pass over the feed, so it's cheap enough to compute on every render.
    fn unseen_counts(&self) -> HashMap<String, usize> {
        let played: HashSet<&str> = self.history.items.iter().map(|w| w.video.id.as_str()).collect();
        let words = self.hide_words();
        let mut counts = HashMap::new();
        for v in self.feed.items() {
            if self.seen.ids.contains(&v.id) || played.contains(v.id.as_str()) || !self.in_active_group(v) || self.is_muted(v) || says_any(&v.title, &words) {
                continue;
            }
            *counts.entry(FEED_ID.to_string()).or_default() += 1;
            if let Some(ch) = channel_of(v) {
                *counts.entry(ch.to_string()).or_default() += 1;
            }
        }
        counts
    }

    /// Notices ("Link copied", "Saved to …") disappear after 4 seconds; a finished download's
    /// line after 6. Watching the value here keeps every place that sets them simple.
    fn expire_notices(&mut self, cx: &mut Context<Self>) {
        let now = std::time::Instant::now();
        match (&self.notice, &self.notice_seen) {
            (None, _) => self.notice_seen = None,
            (Some(n), Some((seen, at))) if n == seen => {
                if now.duration_since(*at) > Duration::from_secs(6) {
                    self.notice = None;
                    self.notice_seen = None;
                    cx.notify();
                }
            }
            (Some(n), _) => self.notice_seen = Some((n.clone(), now)),
        }
        if !self.menu_hovered && self.channel_menu.as_ref().is_some_and(|(_, _, at)| at.elapsed() > Duration::from_secs(2)) {
            self.channel_menu = None;
            self.confirm_unsub = None;
            cx.notify();
        }
        if !self.menu_hovered && self.video_menu.as_ref().is_some_and(|m| m.3.elapsed() > Duration::from_secs(2)) {
            self.close_video_menu(cx);
        }
        if !self.menu_hovered && self.playlist_menu.as_ref().is_some_and(|m| m.2.elapsed() > Duration::from_secs(2)) {
            self.playlist_menu = None;
            cx.notify();
        }
        // Re-render once when a finished download's line should disappear.
        if self.downloads.values().any(|d| d.done_at.is_some_and(|t| (6.0..6.3).contains(&t.elapsed().as_secs_f32()))) {
            cx.notify();
        }
    }

    /// Desktop notifications for feed videos of channels with the bell on, not notified yet.
    fn notify_uploads(&mut self) {
        let words = self.hide_words();
        let fresh: Vec<Video> = self
            .feed
            .items()
            .iter()
            .filter(|v| channel_of(v).is_some_and(|c| self.flags.notify.contains(c)))
            .filter(|v| !self.flags.notified.contains(&v.id) && !self.seen.ids.contains(&v.id))
            .filter(|v| !says_any(&v.title, &words))
            .cloned()
            .collect();
        // Forget ids that left the feed, so the set stays small.
        let in_feed: HashSet<&str> = self.feed.items().iter().map(|v| v.id.as_str()).collect();
        self.flags.notified.retain(|id| in_feed.contains(id.as_str()));
        if fresh.is_empty() {
            store::save_data("channel_flags", &self.flags);
            return;
        }
        self.flags.notified.extend(fresh.iter().map(|v| v.id.clone()));
        store::save_data("channel_flags", &self.flags);
        if !self.settings.notifications {
            return;
        }
        // A few: one each; many (e.g. after a long time closed): one summary.
        let messages: Vec<(String, String)> = if fresh.len() <= 3 {
            fresh.iter().map(|v| (v.channel.clone().unwrap_or_default(), v.title.clone())).collect()
        } else {
            let mut names: Vec<String> = fresh.iter().filter_map(|v| v.channel.clone()).collect();
            names.dedup();
            vec![(format!("{} new videos", fresh.len()), names.join(", "))]
        };
        std::thread::spawn(move || {
            for (summary, body) in messages {
                let _ = notify_rust::Notification::new().appname("UnbloatedTube").summary(&summary).body(&body).show();
            }
        });
    }

    /// Turn mute / notify on or off for a channel.
    fn toggle_flag(&mut self, notify: bool, channel: &str, cx: &mut Context<Self>) {
        let set = if notify { &mut self.flags.notify } else { &mut self.flags.muted };
        let on = set.insert(channel.to_string());
        if !on {
            set.remove(channel);
        }
        if notify && on {
            // Only uploads from now on: what's in the feed already isn't news.
            let ids: Vec<String> = self.feed.items().iter().filter(|v| channel_of(v) == Some(channel)).map(|v| v.id.clone()).collect();
            self.flags.notified.extend(ids);
        }
        store::save_data("channel_flags", &self.flags);
        self.notice = Some(match (notify, on) {
            (true, true) => "Notifications on for this channel".into(),
            (true, false) => "Notifications off for this channel".into(),
            (false, true) => "Muted: hidden from New uploads".into(),
            (false, false) => "Unmuted".into(),
        });
        cx.notify();
    }

    fn is_muted(&self, v: &Video) -> bool {
        channel_of(v).is_some_and(|c| self.flags.muted.contains(c))
    }

    /// The group Subscriptions is filtered to, if any.
    fn active_group(&self) -> Option<&ChannelGroup> {
        let name = self.active_group.as_deref()?;
        self.groups.iter().find(|g| g.name == name)
    }

    /// Whether a feed video's channel is in the active group (always true without one).
    fn in_active_group(&self, v: &Video) -> bool {
        self.active_group().is_none_or(|g| channel_of(v).is_some_and(|c| g.channels.iter().any(|m| m == c)))
    }

    /// An open channel or playlist's videos; New uploads is limited to the active group.
    fn browser_videos(&self, tab: Tab) -> Vec<Video> {
        let b = self.browser_ref(tab);
        let feed = b.open.as_ref().is_some_and(|g| g.id == FEED_ID);
        b.videos
            .items()
            .iter()
            .filter(|v| !feed || (self.in_active_group(v) && !self.is_muted(v)))
            .filter(|v| self.video_matches(v))
            .cloned()
            .collect()
    }

    /// Whether `text` matches the left list's filter (case-insensitive; empty matches all).
    fn filter_match(&self, text: &str) -> bool {
        let q = self.list_filter.trim().to_lowercase();
        q.is_empty() || text.to_lowercase().contains(&q)
    }

    fn video_matches(&self, v: &Video) -> bool {
        self.filter_match(&format!("{} {}", v.title, v.channel.as_deref().unwrap_or_default()))
    }

    /// The left list's filter field (Ctrl+F). Filters as you type; Esc clears.
    fn filter_field(&self, placeholder: &'static str, window: &Window, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let focused = self.filter_focus.is_focused(window);
        let empty = self.list_filter.is_empty();
        div()
            .id("list-filter")
            .track_focus(&self.filter_focus)
            .flex()
            .items_center()
            .gap_2()
            .min_w(px(160.))
            .px_2()
            .py(px(3.))
            .rounded_md()
            .bg(themed(HOVER))
            .border_1()
            .border_color(if focused { themed(MUTED) } else { themed(BORDER) })
            .text_xs()
            .cursor_text()
            .child(svg().path(icons::path("search")).size(px(12.)).flex_none().text_color(themed(MUTED)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(if empty { themed(MUTED) } else { themed(TEXT) })
                    .map(|d| match (empty, focused) {
                        (_, true) => d.text_color(themed(TEXT)).child(self.caret_text("filter", &self.list_filter, placeholder)),
                        (true, false) => d.child(format!("{placeholder} (Ctrl+F)")),
                        (false, false) => d.child(self.list_filter.clone()),
                    }),
            )
            .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                window.focus(&this.filter_focus);
                cx.notify();
            })
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match edit_text(&mut this.caret, "filter", &mut this.list_filter, ev, cx) {
                    Edit::Submit => window.blur(),
                    Edit::Cancel => {
                        this.list_filter.clear();
                        window.blur();
                    }
                    Edit::Changed => {
                        // The list changed under the selection.
                        this.vim_cursor = 0;
                    }
                    Edit::Moved => {}
                    Edit::Ignored => {}
                }
                cx.stop_propagation();
                cx.notify();
            }))
    }

    /// A focused text field's text with its cursor and selection; `placeholder` when empty.
    fn caret_text(&self, id: &'static str, text: &str, placeholder: &str) -> gpui::Div {
        let (pos, anchor) = self.caret.get(id, text);
        let (start, end) = (pos.min(anchor), pos.max(anchor));
        // Stretched to the line's height by the row.
        let bar = || div().flex_none().w(px(1.)).bg(themed(TEXT));
        let part = |t: &str| (!t.is_empty()).then(|| div().flex_none().child(t.to_string()));
        let row = div().flex().min_w_0().overflow_hidden().whitespace_nowrap();
        if text.is_empty() {
            return row.child(bar()).child(div().text_color(themed(MUTED)).child(placeholder.to_string()));
        }
        row.children(part(&text[..start]))
            .when(pos == start, |d| d.child(bar()))
            .children(part(&text[start..end]).map(|d| d.bg(gpui::rgba((ACCENT << 8) | 0x66))))
            .when(pos == end && start != end, |d| d.child(bar()))
            .children(part(&text[end..]))
    }

    /// A full-width bar holding the filter field.
    fn filter_bar(&self, placeholder: &'static str, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        div()
            .flex()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(themed(BORDER))
            .child(self.filter_field(placeholder, window, cx).flex_1())
    }

    /// Ctrl+F: focus the filter, where the current list has one.
    fn focus_filter(&mut self, window: &mut Window) -> bool {
        let has = matches!(self.tab, Tab::Subscriptions | Tab::Playlists | Tab::History | Tab::Downloads);
        if has {
            window.focus(&self.filter_focus);
        }
        has
    }

    fn save_groups(&self) {
        store::save_data("groups", &self.groups);
    }

    fn select_group(&mut self, name: Option<String>, cx: &mut Context<Self>) {
        self.active_group = if self.active_group == name { None } else { name };
        self.confirm_delete_group = None;
        cx.notify();
    }

    fn toggle_membership(&mut self, group: &str, channel: &str, cx: &mut Context<Self>) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.name == group) {
            match g.channels.iter().position(|c| c == channel) {
                Some(i) => {
                    g.channels.remove(i);
                }
                None => g.channels.push(channel.to_string()),
            }
            self.save_groups();
        }
        cx.notify();
    }

    fn delete_group(&mut self, name: String, cx: &mut Context<Self>) {
        if self.confirm_delete_group.as_deref() != Some(name.as_str()) {
            self.confirm_delete_group = Some(name);
        } else {
            self.groups.retain(|g| g.name != name);
            self.save_groups();
            self.active_group = None;
            self.confirm_delete_group = None;
        }
        cx.notify();
    }

    /// Typing a new group's name: Enter creates it (and, in a channel, adds that channel).
    fn new_group_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = &mut self.new_group else { return };
        match edit_text(&mut self.caret, "group", name, ev, cx) {
            Edit::Submit => {
                let name = name.trim().to_string();
                self.new_group = None;
                window.blur();
                if !name.is_empty() && !self.groups.iter().any(|g| g.name == name) {
                    let channel = self.subs.open.as_ref().filter(|g| g.id != FEED_ID).map(|g| g.id.clone());
                    self.groups.push(ChannelGroup { name: name.clone(), channels: channel.into_iter().collect() });
                    self.save_groups();
                    if self.subs.open.is_none() {
                        self.active_group = Some(name);
                    }
                }
            }
            Edit::Cancel => {
                self.new_group = None;
                window.blur();
            }
            Edit::Changed => {}
            Edit::Moved => {}
            Edit::Ignored => {}
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// A round icon button for the channel header; red when `on`.
    fn icon_chip(&self, id: &'static str, icon: &str, on: bool, tooltip: impl Into<SharedString>) -> Stateful<gpui::Div> {
        div()
            .id(id)
            .flex_none()
            .size(px(28.))
            .ml_1()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(if on { themed(ACCENT) } else { themed(HOVER) })
            .cursor_pointer()
            .hover(|d| d.opacity(0.85))
            .child(svg().path(icons::path(icon)).size(px(14.)).text_color(if on { themed(ON_ACCENT) } else { themed(TEXT) }))
            .tooltip(tip_left(tooltip))
    }

    /// A small rounded chip (filter bar, group editor).
    fn chip(&self, id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool) -> Stateful<gpui::Div> {
        div()
            .id(id)
            .flex_none()
            .flex()
            .items_center()
            .px_3()
            .py(px(3.))
            .rounded_full()
            .text_xs()
            .bg(if on { themed(ACCENT) } else { themed(HOVER) })
            .text_color(if on { themed(ON_ACCENT) } else { themed(TEXT) })
            .cursor_pointer()
            .hover(|d| d.opacity(0.85))
            .child(label.into())
    }

    /// The "+ Group" chip, or the field for typing a new group's name.
    fn new_group_chip(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        match &self.new_group {
            Some(name) => {
                let focused = self.new_group_focus.is_focused(window);
                div()
                    .id("new-group-field")
                    .track_focus(&self.new_group_focus)
                    .flex_none()
                    .min_w(px(110.))
                    .px_3()
                    .py(px(3.))
                    .rounded_full()
                    .border_1()
                    .border_color(themed(MUTED))
                    .text_xs()
                    .text_color(if name.is_empty() && !focused { themed(MUTED) } else { themed(TEXT) })
                    .map(|d| if focused { d.child(self.caret_text("group", name, "Group name")) } else { d.child(if name.is_empty() { "Group name".to_string() } else { name.clone() }) })
                    .on_key_down(cx.listener(Self::new_group_key))
            }
            None => self.chip("new-group", "+ Group", false).on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                this.new_group = Some(String::new());
                window.focus(&this.new_group_focus);
                cx.notify();
            }),
        }
    }

    /// Above the channel list: All, each group, + Group, and Delete for the selected group.
    fn render_group_bar(&self, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        let active = self.active_group.clone();
        let mut bar = div()
            .flex()
            .flex_wrap()
            .gap_1()
            .child(
                self.chip("group-all", "All", active.is_none())
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.select_group(None, cx)),
            );
        for (i, g) in self.groups.iter().enumerate() {
            let name = g.name.clone();
            bar = bar.child(
                self.chip(("group", i), g.name.clone(), active.as_deref() == Some(g.name.as_str()))
                    // Channel count, dimmer than the name.
                    .child(div().ml(px(6.)).opacity(0.6).child(g.channels.len().to_string()))
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.select_group(Some(name.clone()), cx)),
            );
        }
        bar = bar.child(self.new_group_chip(window, cx));
        if let Some(name) = active {
            let confirming = self.confirm_delete_group.as_deref() == Some(name.as_str());
            bar = bar.child(
                self.chip("delete-group", if confirming { "Delete group?" } else { "Delete" }, confirming)
                    .when(!confirming, |d| d.text_color(themed(MUTED)))
                    .tooltip(tip("Click twice to delete this group (channels stay subscribed)"))
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.delete_group(name.clone(), cx)),
            );
        }
        // The filter on top, where the other tabs have it; the group chips under it.
        div()
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(themed(BORDER))
            .child(self.filter_field("Filter channels", window, cx))
            .child(bar)
    }

    /// In a channel's header: which groups the channel belongs to (click to toggle).
    fn render_group_editor(&self, channel: &str, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        let mut row = div().flex().flex_wrap().gap_1().px_3().py_2().border_b_1().border_color(themed(BORDER));
        if self.groups.is_empty() {
            row = row.child(div().text_xs().text_color(themed(MUTED)).mr_2().child("No groups yet:"));
        }
        for (i, g) in self.groups.iter().enumerate() {
            let member = g.channels.iter().any(|c| c == channel);
            let (name, ch) = (g.name.clone(), channel.to_string());
            row = row.child(
                self.chip(("member", i), g.name.clone(), member)
                    // Channel count, as in the group bar of the list.
                    .child(div().ml(px(6.)).opacity(0.6).child(g.channels.len().to_string()))
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.toggle_membership(&name, &ch, cx)),
            );
        }
        row.child(self.new_group_chip(window, cx))
    }

    /// Open the right-click menu of a video at `pos`. Playlists (for the list at the bottom) are
    /// fetched when you're logged in and they aren't loaded yet.
    fn open_video_menu(&mut self, video: Video, queue: Option<Arc<[Video]>>, pos: gpui::Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        self.channel_menu = None;
        self.menu_queue = queue;
        // The left column's menu stays inside it; anywhere else it may cover the video.
        let left_only = !self.left_collapsed
            && !self.right_collapsed
            && f32::from(pos.x) < self.settings.split * f32::from(window.viewport_size().width);
        self.video_menu = Some((video, pos, !left_only, Instant::now()));
        self.player_menu = false;
        self.menu_hovered = false;
        // Your editable playlists, and which hold this video, as YouTube's Save menu has them.
        self.menu_lists = None;
        if self.cfg.has_auth() {
            let id = self.video_menu.as_ref().map(|m| m.0.id.clone()).unwrap_or_default();
            self.menu_lists = Some((id.clone(), None));
            let vid = id.clone();
            self.with_account(cx, move |a| a.save_options(&vid), move |this, res, cx| {
                if let Some((cur, slot)) = &mut this.menu_lists {
                    if *cur == id {
                        *slot = Some(res.unwrap_or_default());
                        cx.notify();
                    }
                }
            });
        }
        self.sync_embed();
        cx.notify();
    }

    /// Right click on the playing video (mpv's window, so the position comes from mpv, in its
    /// pixels): the same menu with the player rows. mpv's window hides while it is open, which
    /// shows the thumbnail behind it.
    fn open_player_menu(&mut self, at: (f64, f64), window: &Window, cx: &mut Context<Self>) {
        let (Some(video), Some(embed)) = (self.current.clone(), self.embed.clone()) else { return };
        let s = window.scale_factor() as f64;
        let (ox, oy) = embed.borrow().origin();
        let pos = gpui::point(px(((ox as f64 + at.0) / s) as f32), px(((oy as f64 + at.1) / s) as f32));
        self.open_video_menu(video, None, pos, window, cx);
        if let Some(m) = &mut self.video_menu {
            m.2 = true;
        }
        self.player_menu = true;
        self.sync_embed();
        cx.notify();
    }

    fn close_video_menu(&mut self, cx: &mut Context<Self>) {
        self.video_menu = None;
        self.player_menu = false;
        self.quality_menu = false;
        self.sleep_menu = false;
        self.sub_menu = false;
        self.sync_embed();
        cx.notify();
    }

    /// Play `videos` here as the queue, from the first. Up next is emptied first: it would otherwise
    /// take over at the first Next, and the point of playing a playlist is to hear it through.
    fn play_all(&mut self, videos: Vec<Video>, cx: &mut Context<Self>) {
        let Some(first) = videos.first().cloned() else { return };
        if !self.up_next.is_empty() {
            self.up_next.clear();
            store::save_data("up_next", &self.up_next);
        }
        self.play(first, Some(videos.into()), cx);
    }

    /// Play a playlist here without opening it: all its videos are listed, then the first plays
    /// with the rest as the queue (Next and autoplay go through them).
    fn play_playlist(&mut self, group: Group, cx: &mut Context<Self>) {
        self.playlist_menu = None;
        if self.playlists.open.as_ref().is_some_and(|g| g.id == group.id) {
            if let Load::Ready(v) = &self.playlists.videos {
                let videos = self.visible(v);
                if !videos.is_empty() {
                    self.play_all(videos, cx);
                    return;
                }
            }
        }
        self.notice = Some(format!("Loading {}…", group.title));
        let (cfg, url) = (self.cfg.clone(), group.url.clone());
        let task = blocking::unblock(move || {
            let mut out = Vec::new();
            yt::playlist_videos(&cfg, &url, &mut |v| out.push(v)).map(|_| out)
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| match res {
                Ok(videos) => {
                    let videos = this.visible(&videos);
                    if videos.is_empty() {
                        this.notice = Some(format!("{} has no videos", group.title));
                        cx.notify();
                    } else {
                        this.play_all(videos, cx);
                    }
                }
                Err(e) => {
                    this.notice = Some(format!("Couldn't list {}: {e}", group.title));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Cast a playlist without opening it: its first 2000 videos are listed, then sent in chunks.
    fn cast_playlist(&mut self, group: Group, target: String, cx: &mut Context<Self>) {
        self.playlist_menu = None;
        // The one that is open is already loaded.
        if self.playlists.open.as_ref().is_some_and(|g| g.id == group.id) {
            if let Load::Ready(v) = &self.playlists.videos {
                let videos = self.visible(v);
                self.cast_list(Some(target), videos, cx);
                return;
            }
        }
        self.notice = Some(format!("Loading {}…", group.title));
        let (cfg, url) = (self.cfg.clone(), group.url.clone());
        let task = blocking::unblock(move || {
            let mut out = Vec::new();
            yt::playlist_head(&cfg, &url, cast::MAX_LIST, &mut |v| out.push(v)).map(|_| out)
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| match res {
                Ok(videos) if !videos.is_empty() => {
                    let videos = this.visible(&videos);
                    this.cast_list(Some(target), videos, cx);
                }
                Ok(_) => {
                    this.notice = Some(format!("{} has no videos", group.title));
                    cx.notify();
                }
                Err(e) => {
                    this.notice = Some(format!("Couldn't list {}: {e}", group.title));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// The right-click menu of a playlist: Play here, and Cast (one row per target). A backdrop closes it.
    fn render_playlist_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let (group, pos, _) = self.playlist_menu.clone()?;
        let targets = self.cast_targets();
        let many = targets.len() > 1;
        const MENU_W: f32 = 220.;
        let win = window.viewport_size();
        let win_w = f32::from(win.width);
        let column_right = if self.left_collapsed || self.right_collapsed { win_w } else { self.settings.split * win_w };
        let left = f32::from(pos.x).min(column_right - MENU_W - 8.).max(8.);
        let top = f32::from(pos.y).min(f32::from(win.height) - 48. - 28. - 36. * (targets.len() + 1) as f32).max(8.);
        let play_group = group.clone();
        let rows: Vec<_> = targets
            .into_keys()
            .enumerate()
            .map(|(i, name)| {
                let (g, target) = (group.clone(), name.clone());
                div()
                    .id(("playlist-menu-cast", i))
                    .px_3()
                    .py_2()
                    .text_sm()
                    .cursor_pointer()
                    .text_color(themed(TEXT))
                    .hover(|d| d.bg(themed(BORDER)))
                    .child(if many { format!("Cast to {name}") } else { "Cast".to_string() })
                    .on_click(cx.listener(move |this, _, _, cx| this.cast_playlist(g.clone(), target.clone(), cx)))
            })
            .collect();
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(
                    div()
                        .id("playlist-menu-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| { this.playlist_menu = None; cx.notify(); }))
                        .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| { this.playlist_menu = None; cx.notify(); })),
                )
                .child(
                    div()
                        .id("playlist-menu")
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .occlude()
                        .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                            this.menu_hovered = *hovered;
                            if let (false, Some((_, _, at))) = (*hovered, &mut this.playlist_menu) {
                                *at = Instant::now();
                            }
                        }))
                        .w(px(MENU_W))
                        .py_1()
                        .rounded_md()
                        .bg(themed(HOVER))
                        .border_1()
                        .border_color(themed(BORDER))
                        .shadow_lg()
                        .child(div().px_3().py_1().text_xs().text_color(themed(MUTED)).truncate().child(group.title.clone()))
                        .child(
                            div()
                                .id("playlist-menu-play")
                                .px_3()
                                .py_2()
                                .text_sm()
                                .cursor_pointer()
                                .text_color(themed(TEXT))
                                .hover(|d| d.bg(themed(BORDER)))
                                .child("Play")
                                .on_click(cx.listener(move |this, _, _, cx| this.play_playlist(play_group.clone(), cx))),
                        )
                        .children(rows),
                ),
        )
    }

    /// The right-click menu of a video: copy link, Up next, Watch later, then the playlists in a
    /// scrolling list. A backdrop closes it.
    fn render_video_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let (video, pos, over_player, _) = self.video_menu.clone()?;
        let auth = self.cfg.has_auth();
        let queued = self.up_next.iter().any(|v| v.id == video.id);
        let downloaded = self.library.iter().any(|s| s.video.id == video.id);
        let row = |id: &'static str| div().id(id).px_3().py_2().text_sm().cursor_pointer().text_color(themed(TEXT)).hover(|d| d.bg(themed(BORDER)));
        // Ticked when the video is already in the playlist; clicking toggles, like the groups menu.
        let lists = self.menu_lists.as_ref().filter(|(id, _)| *id == video.id).and_then(|(_, l)| l.clone());
        let loading = lists.is_none();
        let list_rows: Vec<_> = lists
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, o)| {
                let (v, member) = (video.clone(), o.contains);
                let pl = Group { id: o.id.clone(), title: o.title.clone(), url: String::new(), thumb: None, subscribers: None };
                div()
                    .id(("video-menu-playlist", i))
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .cursor_pointer()
                    .text_color(themed(TEXT))
                    .hover(|d| d.bg(themed(BORDER)))
                    .child(div().w(px(14.)).flex_none().when(member, |d| d.child(svg().path(icons::path("check")).size(px(14.)).text_color(themed(ACCENT)))))
                    .child(div().truncate().child(o.title.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.close_video_menu(cx);
                        if member {
                            this.remove_from_playlist(pl.clone(), v.clone(), cx);
                        } else {
                            this.save_video_to(v.clone(), pl.clone(), cx);
                        }
                    }))
            })
            .collect();
        let n_lists = list_rows.len();
        // Fixed rows: title, Copy link, Up next, [Delete download], [Watch later, divider, "Save to playlist"]. The
        // list shows at most 5½ rows (the cut-off one says it scrolls), less if the window is short.
        const MENU_W: f32 = 240.;
        const ROW_H: f32 = 36.;
        const EDGE: f32 = 48.;
        let fixed_h = 28. + ROW_H * if downloaded { 3. } else { 2. } + if self.menu_queue.is_some() && self.cast_available() { ROW_H * self.cast_targets().len() as f32 } else { 0. } + if auth { ROW_H + 9. + 24. } else { 0. } + if self.player_menu { ROW_H * (8. + if self.quality_menu { 10. } else { 0. } + if self.sleep_menu { 7. } else { 0. } + if self.sub_menu { self.sub_choices().len() as f32 } else { 0. }) + 9. } else { 0. };
        let win = window.viewport_size();
        let (win_w, mut win_h) = (f32::from(win.width), f32::from(win.height));
        if let Some(display) = window.display(cx) {
            let top_on_screen = f32::from(window.window_bounds().get_bounds().origin.y);
            win_h = win_h.min(f32::from(display.bounds().size.height) - top_on_screen);
        }
        let list_max = (ROW_H * 5.5).min(win_h - fixed_h - EDGE).max(ROW_H);
        let list_h = if auth { (n_lists as f32 * ROW_H).max(ROW_H).min(list_max) } else { 0. };
        let column_right = if over_player { win_w } else { self.settings.split * win_w };
        let left = f32::from(pos.x).min(column_right - MENU_W - 8.).max(8.);
        // Too short a window for every row: the menu itself scrolls (the playlists already
        // shrank to one row above).
        let avail = (win_h - EDGE - 8.).max(ROW_H * 4.);
        let top = f32::from(pos.y).min(win_h - (fixed_h + list_h).min(avail) - EDGE).max(8.);
        let (v_copy, v_queue, v_later, v_separate) = (video.clone(), video.clone(), video.clone(), video.clone());
        let player_menu = self.player_menu;
        let speed = self.settings.speed;
        // Quality: only for a video streaming here (not a downloaded file, not while casting).
        let streaming = self.casting.is_none() && self.current.as_ref().is_some_and(|v| self.media_path(v) == v.url());
        let picked = self.picked_quality();
        let quality_label = match (picked, self.state.as_ref().and_then(|s| s.height)) {
            (Some(0), _) => "audio only".to_string(),
            (_, Some(h)) => format!("{h}p"),
            _ => "…".to_string(),
        };
        // Subtitles: the language list (see `sub_choices`); the row says what shows now.
        let sub_choices = if self.sub_menu { self.sub_choices() } else { Vec::new() };
        let sub_label = match &self.sub_pick {
            _ if !self.state.as_ref().is_some_and(|s| s.sub_on) => "off".to_string(),
            Some((v, code, _)) if self.current.as_ref().is_some_and(|c| c.id == *v) => code.clone(),
            _ => "on".to_string(),
        };
        let sub_rows: Vec<_> = sub_choices
            .into_iter()
            .enumerate()
            .map(|(i, (label, choice, ticked))| {
                let info = choice == SubChoice::Info;
                div()
                    .id(("video-menu-sub-option", i))
                    .pl_6()
                    .pr_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .text_color(if info { themed(MUTED) } else { themed(TEXT) })
                    .when(!info, |d| d.cursor_pointer().hover(|d| d.bg(themed(BORDER))))
                    .child(div().w(px(14.)).flex_none().when(ticked, |d| d.child(svg().path(icons::path("check")).size(px(14.)).text_color(themed(ACCENT)))))
                    .child(div().min_w_0().truncate().child(label.clone()))
                    .when(!info, |d| {
                        d.on_click(cx.listener(move |this, _, _, cx| {
                            this.close_video_menu(cx);
                            this.pick_sub(choice.clone(), label.clone(), cx);
                        }))
                    })
            })
            .collect();
        // Sleep timer: off, end of this video (not while casting: the receiver goes on by
        // itself), or minutes; ticked: the end-of-video choice when set.
        let sleep_label = self.sleep_status().map_or("off".to_string(), |s| s.trim_start_matches("Sleep ").to_string());
        let sleep_choices: [(Option<u64>, &str); 7] =
            [(None, "Off"), (Some(0), "End of this video"), (Some(15), "15 min"), (Some(30), "30 min"), (Some(45), "45 min"), (Some(60), "60 min"), (Some(90), "90 min")];
        let sleep_rows: Vec<_> = sleep_choices
            .into_iter()
            .filter(|_| self.sleep_menu)
            .filter(|(m, _)| *m != Some(0) || self.casting.is_none())
            .enumerate()
            .map(|(i, (minutes, label))| {
                let ticked = match minutes {
                    None => self.sleep.is_none(),
                    Some(0) => self.sleep == Some(Sleep::EndOfVideo),
                    Some(_) => false,
                };
                div()
                    .id(("video-menu-sleep-option", i))
                    .pl_6()
                    .pr_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .cursor_pointer()
                    .text_color(themed(TEXT))
                    .hover(|d| d.bg(themed(BORDER)))
                    .child(div().w(px(14.)).flex_none().when(ticked, |d| d.child(svg().path(icons::path("check")).size(px(14.)).text_color(themed(ACCENT)))))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.close_video_menu(cx);
                        let sleep = minutes.map(|m| if m == 0 { Sleep::EndOfVideo } else { Sleep::At(Instant::now() + Duration::from_secs(m * 60)) });
                        this.set_sleep(sleep, cx);
                    }))
            })
            .collect();
        // The list: the settings' maximum, the usual heights, audio only; ticked: in use.
        let quality_rows: Vec<_> = [None, Some(2160), Some(1440), Some(1080), Some(720), Some(480), Some(360), Some(240), Some(144), Some(0)]
            .into_iter()
            .filter(|_| self.quality_menu)
            .enumerate()
            .map(|(i, q)| {
                let label = match q {
                    None => format!("Auto (up to {}p)", self.settings.max_quality),
                    Some(0) => "Audio only".to_string(),
                    Some(h) => format!("{h}p"),
                };
                div()
                    .id(("video-menu-quality-option", i))
                    .pl_6()
                    .pr_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .cursor_pointer()
                    .text_color(themed(TEXT))
                    .hover(|d| d.bg(themed(BORDER)))
                    .child(div().w(px(14.)).flex_none().when(q == picked, |d| d.child(svg().path(icons::path("check")).size(px(14.)).text_color(themed(ACCENT)))))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.close_video_menu(cx);
                        this.pick_quality(q, cx);
                    }))
            })
            .collect();
        // One row per cast target: this video and the ones after it in its list.
        let rest: Option<Vec<Video>> = self
            .menu_queue
            .as_ref()
            .filter(|_| self.cast_available())
            .map(|q| q.iter().skip_while(|v| v.id != video.id).cloned().collect::<Vec<_>>())
            .filter(|r| !r.is_empty());
        let cast_rows: Vec<_> = rest
            .map(|rest| {
                let targets = self.cast_targets();
                let many = targets.len() > 1;
                let label = if rest.len() > 1 { "from here" } else { "this video" };
                targets
                    .into_keys()
                    .enumerate()
                    .map(|(i, name)| {
                        let (rest, target) = (rest.clone(), name.clone());
                        div()
                            .id(("video-menu-cast", i))
                            .px_3()
                            .py_2()
                            .text_sm()
                            .cursor_pointer()
                            .text_color(themed(TEXT))
                            .hover(|d| d.bg(themed(BORDER)))
                            .child(if many { format!("Cast {label} to {name}") } else { format!("Cast {label}") })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.cast_list(Some(target.clone()), rest.clone(), cx);
                            }))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(
                    div()
                        .id("video-menu-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close_video_menu(cx)))
                        .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| this.close_video_menu(cx))),
                )
                .child(
                    div()
                        .id("video-menu")
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .max_h(px(avail))
                        .overflow_y_scroll()
                        .occlude()
                        .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                            this.menu_hovered = *hovered;
                            if let (false, Some((_, _, _, at))) = (*hovered, &mut this.video_menu) {
                                *at = Instant::now();
                            }
                        }))
                        .w(px(MENU_W))
                        .py_1()
                        .rounded_md()
                        .bg(themed(HOVER))
                        .border_1()
                        .border_color(themed(BORDER))
                        .shadow_lg()
                        .child(div().px_3().py_1().text_xs().text_color(themed(MUTED)).truncate().child(video.title.clone()))
                        .when(player_menu, |d| {
                            d.child(row("video-menu-time").child("Copy link at current time").on_click(cx.listener(|this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.copy_link_at_time(cx);
                            })))
                            .child(row("video-menu-loop").child("Loop").on_click(cx.listener(|this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.player.toggle_loop();
                            })))
                            .child(row("video-menu-speed").child(format!("Speed: {speed}×, next")).on_click(cx.listener(|this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.cycle_speed(cx);
                            })))
                            .when(streaming, |d| {
                                d.child(row("video-menu-quality").child(format!("Quality: {quality_label} ›")).on_click(cx.listener(|this, _, _, cx| {
                                    this.quality_menu = !this.quality_menu;
                                    cx.notify();
                                })))
                                .children(quality_rows)
                            })
                            .child(row("video-menu-sleep").child(format!("Sleep: {sleep_label} ›")).on_click(cx.listener(|this, _, _, cx| {
                                this.sleep_menu = !this.sleep_menu;
                                cx.notify();
                            })))
                            .children(sleep_rows)
                            .child(row("video-menu-subs").child(format!("Subtitles: {sub_label} ›")).on_click(cx.listener(|this, _, _, cx| {
                                this.sub_menu = !this.sub_menu;
                                if this.sub_menu {
                                    this.load_caption_langs(cx);
                                }
                                cx.notify();
                            })))
                            .children(sub_rows)
                            .child(row("video-menu-stats").child("Stats for nerds").on_click(cx.listener(|this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.player.toggle_stats();
                            })))
                            .child(row("video-menu-browser").child("Open in browser").on_click(cx.listener(|this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.open_in_browser(cx);
                            })))
                            .child(div().my_1().h(px(1.)).bg(themed(BORDER)))
                        })
                        .children(cast_rows)
                        .child(row("video-menu-separate").child("Play in separate window").on_click(cx.listener(move |this, _, _, cx| {
                            this.close_video_menu(cx);
                            this.play_separate(&v_separate, cx);
                        })))
                        .child(row("video-menu-copy").child("Copy link").on_click(cx.listener(move |this, _, _, cx| {
                            this.close_video_menu(cx);
                            this.copy_video_link(&v_copy, cx);
                        })))
                        .child(
                            row("video-menu-queue").child(if queued { "Remove from Up next" } else { "Add to Up next" }).on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.close_video_menu(cx);
                                    if queued {
                                        this.dequeue(&v_queue.id, cx);
                                    } else {
                                        this.enqueue(v_queue.clone(), cx);
                                    }
                                },
                            )),
                        )
                        .when(downloaded, |d| {
                            let id = video.id.clone();
                            d.child(row("video-menu-delete-download").child("Delete downloaded file").on_click(cx.listener(move |this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.delete_download(&id, cx);
                            })))
                        })
                        .when(auth, |d| {
                            d.child(row("video-menu-later").child("Save to Watch later").on_click(cx.listener(move |this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.add_to_watch_later(v_later.clone(), cx);
                            })))
                            .child(div().my_1().h(px(1.)).bg(themed(BORDER)))
                            .child(div().px_3().pb_1().text_xs().text_color(themed(MUTED)).child("Save to playlist"))
                            .child(div().id("video-menu-playlists").max_h(px(list_max)).overflow_y_scroll().children(list_rows))
                            .when(n_lists == 0, |d| {
                                d.child(div().px_3().py_1().text_xs().text_color(themed(MUTED)).child(if loading { "Loading playlists…" } else { "No playlists" }))
                            })
                        }),
                ),
        )
    }

    /// The right-click menu of a subscription: a backdrop that closes it and the menu at the click.
    fn render_channel_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let (g, pos, _) = self.channel_menu.clone()?;
        let confirming = self.confirm_unsub.as_deref() == Some(g.id.as_str());
        // One row per group, ticked when the channel is in it; clicking toggles.
        let mut group_rows = Vec::new();
        for (i, grp) in self.groups.iter().enumerate() {
            let member = grp.channels.iter().any(|c| *c == g.id);
            let (name, ch) = (grp.name.clone(), g.id.clone());
            group_rows.push(
                div()
                    .id(("channel-menu-group", i))
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .cursor_pointer()
                    .text_color(themed(TEXT))
                    .hover(|d| d.bg(themed(BORDER)))
                    .child(div().w(px(14.)).flex_none().when(member, |d| d.child(svg().path(icons::path("check")).size(px(14.)).text_color(themed(ACCENT)))))
                    .child(div().truncate().child(grp.name.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_membership(&name, &ch, cx))),
            );
        }
        let no_groups = group_rows.is_empty();
        // Fixed rows: channel name, Unsubscribe, divider, "Add to group". The group list shows at
        // most 5½ rows (the cut-off one says it scrolls), less if the window is short, and the menu
        // is moved up/left to stay inside the window, with a margin: windows can run past the screen.
        const MENU_W: f32 = 220.;
        const FIXED_H: f32 = 28. + 36. + 9. + 24.;
        const ROW_H: f32 = 36.;
        const EDGE: f32 = 48.;
        let win = window.viewport_size();
        let (win_w, mut win_h) = (f32::from(win.width), f32::from(win.height));
        // The window can be taller than the screen (minimum size, display scaling): use the part
        // of it that is visible. The bounds' origin is the window's position on the screen.
        if let Some(display) = window.display(cx) {
            let top_on_screen = f32::from(window.window_bounds().get_bounds().origin.y);
            win_h = win_h.min(f32::from(display.bounds().size.height) - top_on_screen);
        }
        let list_max = (ROW_H * 5.5).min(win_h - FIXED_H - EDGE).max(ROW_H);
        let list_h = (group_rows.len() as f32 * ROW_H).max(ROW_H).min(list_max);
        // Stay inside the left column: the video is a native window drawn over everything on its side.
        let column_right = if self.left_collapsed || self.right_collapsed { win_w } else { self.settings.split * win_w };
        let left = f32::from(pos.x).min(column_right - MENU_W - 8.).max(8.);
        // Too short a window for every row: the menu itself scrolls too.
        let avail = (win_h - EDGE - 8.).max(ROW_H * 4.);
        let top = f32::from(pos.y).min(win_h - (FIXED_H + list_h).min(avail) - EDGE).max(8.);
        fn close(this: &mut Unbloated, cx: &mut Context<Unbloated>) {
            this.channel_menu = None;
            this.confirm_unsub = None;
            cx.notify();
        }
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(
                    div()
                        .id("channel-menu-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| close(this, cx)))
                        .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| close(this, cx))),
                )
                .child(
                    div()
                        .id("channel-menu")
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .max_h(px(avail))
                        .overflow_y_scroll()
                        .occlude()
                        .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                            this.menu_hovered = *hovered;
                            if let (false, Some((_, _, at))) = (*hovered, &mut this.channel_menu) {
                                *at = Instant::now();
                            }
                        }))
                        .w(px(220.))
                        .py_1()
                        .rounded_md()
                        .bg(themed(HOVER))
                        .border_1()
                        .border_color(themed(BORDER))
                        .shadow_lg()
                        .child(div().px_3().py_1().text_xs().text_color(themed(MUTED)).truncate().child(g.title.clone()))
                        .child(
                            div()
                                .id("channel-menu-unsub")
                                .px_3()
                                .py_2()
                                .text_sm()
                                .cursor_pointer()
                                .text_color(if confirming { themed(ACCENT) } else { themed(TEXT) })
                                .hover(|d| d.bg(themed(BORDER)))
                                .child(if confirming { "Click again to unsubscribe" } else { "Unsubscribe" })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_channel_sub(g.clone(), true, cx);
                                    // The second click did it; the first only asked for confirmation.
                                    if this.confirm_unsub.is_none() {
                                        this.channel_menu = None;
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(div().my_1().h(px(1.)).bg(themed(BORDER)))
                        .child(div().px_3().pb_1().text_xs().text_color(themed(MUTED)).child("Add to group"))
                        .child(
                            div()
                                .id("channel-menu-groups")
                                .max_h(px(list_max))
                                .overflow_y_scroll()
                                .children(group_rows),
                        )
                        .when(no_groups, |d| d.child(div().px_3().py_1().text_xs().text_color(themed(MUTED)).child("No groups yet: make one with + Group"))),
                ),
        )
    }

    /// Subscribe to `channel`, or unsubscribe after a confirming second click.
    fn toggle_channel_sub(&mut self, channel: Group, subscribed: bool, cx: &mut Context<Self>) {
        if subscribed && self.confirm_unsub.as_deref() != Some(channel.id.as_str()) {
            self.confirm_unsub = Some(channel.id.clone());
            cx.notify();
            return;
        }
        self.confirm_unsub = None;
        let (id, on) = (channel.id.clone(), !subscribed);
        self.with_account(cx, move |a| a.subscribe(&id, on), move |this, res, _| match res {
            Ok(()) => {
                if let Load::Ready(groups) | Load::Loading(groups) = &mut this.subs.groups {
                    groups.retain(|g| g.id != channel.id);
                    if on {
                        groups.push(channel.clone());
                        groups.sort_by_key(|g| g.title.to_lowercase());
                    }
                }
                store::save_list("subs", this.subs.groups.items());
                // Keep the player's subscribe button in sync if it's the same channel.
                if let Some((_, st)) = &mut this.status {
                    if st.channel_id.as_deref() == Some(channel.id.as_str()) {
                        st.subscribed = on;
                    }
                }
                this.notice = Some(format!("{} {}", if on { "Subscribed to" } else { "Unsubscribed from" }, channel.title));
            }
            Err(e) => this.notice = Some(e),
        });
        cx.notify();
    }

    /// Show a channel's videos in the left column (subscribed or not).
    fn show_channel(&mut self, channel: Group, cx: &mut Context<Self>) {
        self.tab = Tab::Subscriptions;
        self.open_group(Tab::Subscriptions, channel, cx);
    }

    fn open_group(&mut self, tab: Tab, group: Group, cx: &mut Context<Self>) {
        if tab == Tab::Subscriptions && group.id != FEED_ID {
            // Opening a channel shows its new videos, so they no longer count as new.
            let ids: Vec<String> = self.feed.items().iter().filter(|v| channel_of(v) == Some(&group.id)).map(|v| v.id.clone()).collect();
            if !ids.is_empty() {
                self.seen.ids.extend(ids);
                self.seen.save();
            }
        }
        self.confirm_unsub = None;
        self.editing_groups = false;
        let b = self.browser(tab);
        b.open = Some(group);
        b.view = ChannelView::Videos;
        self.load_group_videos(tab, cx);
    }

    fn set_channel_view(&mut self, view: ChannelView, cx: &mut Context<Self>) {
        if self.subs.view != view {
            self.subs.view = view;
            self.load_group_videos(Tab::Subscriptions, cx);
        }
    }

    /// (Re)load the open group's list: a channel's videos or Shorts, or a playlist.
    fn load_group_videos(&mut self, tab: Tab, cx: &mut Context<Self>) {
        let b = self.browser(tab);
        let Some(group) = b.open.clone() else { return };
        let (page, suffix) = match b.view {
            ChannelView::Shorts => ("/shorts", "-shorts"),
            ChannelView::Live => ("/streams", "-live"),
            ChannelView::Videos => ("/videos", ""),
        };
        let url = group.url.replace("/videos", page);
        // Revisiting a channel or playlist shows its last list instantly, then refreshes.
        let cache = Some(format!("group-{}{suffix}", group.id));
        b.videos = Load::Idle;
        if tab == Tab::Subscriptions {
            self.fetch(cx, "subs.videos", |s| &mut s.subs.videos, cache, move |cfg, on| yt::group_videos(cfg, &url, on));
        } else {
            self.fetch(cx, "playlists.videos", |s| &mut s.playlists.videos, cache, move |cfg, on| {
                yt::playlist_videos(cfg, &url, on)
            });
        }
    }

    fn select_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.cycle_lower = false;
        self.left_collapsed = false;
        self.searching = false;
        if tab == Tab::Downloads {
            self.prune_library();
        }
        let idle = match tab {
            Tab::Search | Tab::Settings | Tab::Downloads => false,
            // Logged out: only fetch the anonymous home the first time (or after a failure).
            _ if !self.cfg.has_auth() => matches!(self.anon, Load::Idle | Load::Failed(_)),
            // The list shown may be the one saved last time, or from a while ago: ask again
            // (it stays on screen until the new one is complete).
            Tab::History => {
                !matches!(self.yt_history, Load::Loading(_)) && self.yt_history_asked.is_none_or(|t| t.elapsed() > Duration::from_secs(60))
            }
            _ => matches!(self.browser(tab).groups, Load::Idle),
        };
        if idle {
            self.load_tab(cx);
        }
        cx.notify();
    }

    fn tab_loading(&self) -> bool {
        match self.tab {
            Tab::Settings | Tab::Downloads => false,
            Tab::Search => matches!(self.search, Load::Loading(_)),
            Tab::History if self.cfg.has_auth() => matches!(self.yt_history, Load::Loading(_)),
            tab => {
                let b = self.browser_ref(tab);
                if b.open.is_some() {
                    matches!(b.videos, Load::Loading(_))
                } else if !self.cfg.has_auth() {
                    matches!(self.anon, Load::Loading(_))
                } else {
                    matches!(b.groups, Load::Loading(_))
                }
            }
        }
    }

    /// Home feed when logged in; otherwise more uploads from the current video's channel.
    fn load_recs(&mut self, cx: &mut Context<Self>) {
        let channel = self.current.as_ref().and_then(|v| v.channel_url.clone());
        let auth = self.cfg.has_auth();
        if !auth && channel.is_none() {
            // Logged out with nothing playing: `render_recs` falls back to the anonymous home.
            self.recs = Load::Idle;
            return;
        }
        // Only the home feed is the same every time, so only it is cached.
        self.fetch(cx, "recs", |s| &mut s.recs, auth.then(|| "recs".into()), move |cfg, on| {
            if auth {
                match yt::recommendations(cfg, on) {
                    Err(e) if channel.is_none() => return Err(e),
                    Err(_) => {}
                    ok => return ok,
                }
            }
            yt::channel_uploads(cfg, &channel.unwrap(), on)
        });
    }

    /// Play `video`; `queue` is the list it was picked from (None keeps the current one).
    fn play(&mut self, video: Video, queue: Option<Arc<[Video]>>, cx: &mut Context<Self>) {
        if let Some(queue) = queue {
            self.queue = queue;
        }
        if !self.nav_moving {
            // Playing something new drops what "forward" would have gone to, as in a browser.
            let keep = self.nav_here().map_or(self.nav.len(), |p| p + 1);
            self.nav.truncate(keep);
            if self.nav.last().is_none_or(|v| v.id != video.id) {
                self.nav.push(video.clone());
            }
            self.nav.drain(..self.nav.len().saturating_sub(100));
            self.nav_pos = self.nav.len().checked_sub(1);
        }
        if let Some(i) = self.up_next.iter().position(|v| v.id == video.id) {
            self.up_next.remove(i);
            store::save_data("up_next", &self.up_next);
        }
        self.history.touch(&video);
        self.history.save();
        let channel_changed = self.current.as_ref().map(|c| &c.channel_url) != Some(&video.channel_url);
        if let Some(c) = self.casting.as_mut() {
            // While casting, a picked video goes to the receiver (which resolves it with its own
            // login) and the cast view stays. After "Back to this screen" videos play here again.
            // The cast starts over now: until the receiver plays the new video, the old one (or
            // nothing, while it loads) must neither be shown nor end the cast.
            // No polls (`polling`) until the receiver has taken it; see cast_list.
            let name = c.name.clone();
            *c = Casting { name: name.clone(), remote: c.remote.clone(), status: None, seen: false, started: Instant::now(), polling: true, listed: false, items: vec![video.clone()], marked: Default::default(), sent: 1 };
            self.cast_list(Some(name), vec![video.clone()], cx);
            self.current = Some(video);
        } else {
            self.start(video, false);
        }
        if !self.cfg.has_auth() && channel_changed {
            self.load_recs(cx);
        }
        // What Next would play, so it starts at once.
        if let Some(next) = self.next_video() {
            self.prefetch(&next.id, cx);
        }
        cx.notify();
    }

    /// Resolve a video in the background so playing it starts sooner (see prefetch.rs).
    fn prefetch(&mut self, id: &str, cx: &mut Context<Self>) {
        let recent = |t: &Instant| t.elapsed() < prefetch::TTL - Duration::from_secs(600);
        if !self.settings.prefetch || self.current.as_ref().is_some_and(|c| c.id == id) || self.prefetching >= 2 || self.prefetched.get(id).is_some_and(recent) {
            return;
        }
        self.prefetched.insert(id.to_string(), Instant::now());
        self.prefetching += 1;
        let (cfg, format, id) = (self.cfg.clone(), player::format(&self.settings), id.to_string());
        let task = blocking::unblock(move || prefetch::fetch(&cfg, &id, &format));
        cx.spawn(async move |this, cx| {
            // A video whose lookup failed (members only, upcoming, removed…) isn't asked again
            // until the entry expires: it would fail again, seconds each time, over and over.
            let _ = task.await;
            this.update(cx, |this, _| this.prefetching -= 1).ok();
        })
        .detach();
    }

    /// The pointer entered or left a video row: resolve it once it has rested there a moment.
    fn prefetch_hover(&mut self, id: &str, hovered: bool, cx: &mut Context<Self>) {
        if !hovered {
            if self.hover_video.as_deref() == Some(id) {
                self.hover_video = None;
            }
            return;
        }
        self.hover_video = Some(id.to_string());
        let id = id.to_string();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(150)).await;
            this.update(cx, |this, cx| {
                if this.hover_video.as_deref() == Some(id.as_str()) {
                    this.prefetch(&id, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// The streams that are live now from your subscriptions (the feed's live rows, in the active
    /// group and not muted); `filtered`: also by the text in the list filter.
    fn live_videos(&self, filtered: bool) -> Vec<Video> {
        self.feed
            .items()
            .iter()
            .filter(|v| v.live && self.in_active_group(v) && !self.is_muted(v))
            .filter(|v| !filtered || self.video_matches(v))
            .cloned()
            .collect()
    }

    /// The first rows of the list on screen, and the head of Up next: what is most likely played.
    fn prefetch_top(&mut self, cx: &mut Context<Self>) {
        if !self.settings.prefetch {
            return;
        }
        let mut ids: Vec<String> = self.up_next.first().map(|v| v.id.clone()).into_iter().collect();
        ids.extend(self.left_items().into_iter().filter_map(|i| if let Item::Video(v, _) = i { Some(v.id) } else { None }).take(3));
        for id in ids {
            self.prefetch(&id, cx);
        }
    }

    /// Vim mode: the same for the video the cursor rests on.
    fn prefetch_cursor(&mut self, cx: &mut Context<Self>) {
        let cursor = self.vim_cursor;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(150)).await;
            this.update(cx, |this, cx| {
                if this.vim_cursor == cursor && this.settings.vim {
                    if let Some(Item::Video(v, _)) = this.left_items().get(cursor) {
                        let id = v.id.clone();
                        this.prefetch(&id, cx);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// Load `video` into mpv at its resume position.
    fn start(&mut self, video: Video, paused: bool) {
        if self.embed.is_none() {
            self.embed = Embed::new().map(|e| Rc::new(RefCell::new(e)));
            if self.embed.is_none() {
                eprintln!("unbloatedtube: can't embed video (not X11?), using a separate mpv window");
            }
        }
        let wid = self.embed.as_ref().filter(|_| !self.pip).map(|e| {
            // mpv keeps its window unmapped if ours is hidden when it starts.
            e.borrow_mut().set_visible(true);
            e.borrow().id()
        });
        self.casting = None;
        let start = self.link_start.take().unwrap_or_else(|| self.history.position(&video.id));
        self.subs_pending = None;
        self.subs_wanted = self.settings.subtitles.then(|| video.id.clone());
        let options = player::options(&self.cfg, &self.settings, self.pip);
        // Hide the old video's last frame only when mpv keeps running: a freshly started mpv
        // (first video, or changed options) never shows its picture if ours is hidden then.
        let restarts = self.player.options() != options.as_slice();
        self.hide_while_loading = !restarts;
        if let (true, Some(e)) = (self.hide_while_loading, &self.embed) {
            // Show the new thumbnail right away instead of the old video's last frame.
            e.borrow_mut().set_visible(false);
        }
        // A quality picked for another video is over; the running mpv gets this one's format (a
        // restarted mpv starts with the settings' one from `options`).
        if self.quality.as_ref().is_some_and(|(id, _)| *id != video.id) {
            self.quality = None;
        }
        if !restarts {
            self.player.set_format(&player::format_at(&self.settings, self.quality.as_ref().map(|(_, h)| *h)));
        }
        if let Err(e) = self.player.play(&options, &self.media_path(&video), start, self.settings.speed, wid, paused) {
            eprintln!("{e}");
        }
        self.saving = false;
        self.notice = None;
        self.current = Some(video);
        self.loading = true;
        self.load_started = Instant::now();
    }

    /// A video that plays without moving: mpv shows a black picture at 0:00 when YouTube's video
    /// server drops the connection (or stops mid-video when it stops sending). After STALL, its
    /// kept lookup is dropped and it is loaded once more at the same place, which usually gets
    /// another server; if that one doesn't move either, the notice says so.
    fn watch_stall(&mut self, state: Option<&player::State>, url: Option<&str>, cx: &mut Context<Self>) {
        const STALL: Duration = Duration::from_secs(10);
        let moving = state.filter(|s| s.playing && !s.paused && !s.ended && !s.idle && Some(s.path.as_str()) == url);
        let (Some(s), Some(video), None, false) = (moving, self.current.clone(), &self.casting, self.loading) else {
            self.stall = None;
            return;
        };
        match self.stall {
            Some((since, at)) if (s.position - at).abs() < 0.25 => {
                if since.elapsed() < STALL {
                    return;
                }
                self.stall = None;
                match &mut self.stall_retried {
                    Some((id, told)) if *id == video.id => {
                        if !*told {
                            *told = true;
                            self.notice = Some("YouTube's video server isn't sending this video; try again in a moment".into());
                            cx.notify();
                        }
                    }
                    _ => {
                        self.stall_retried = Some((video.id.clone(), false));
                        prefetch::forget(&video.id);
                        self.link_start = Some(s.position);
                        self.start(video, false);
                        cx.notify();
                    }
                }
            }
            _ => self.stall = Some((Instant::now(), s.position)),
        }
    }

    /// What the sleep timer waits for, for the notice corner and the menu.
    fn sleep_status(&self) -> Option<String> {
        Some(match self.sleep? {
            Sleep::EndOfVideo => "Sleep after this video".to_string(),
            Sleep::At(t) => {
                let left = t.saturating_duration_since(Instant::now()).as_secs();
                if left < 60 { format!("Sleep in {left} s") } else { format!("Sleep in {} min", left.div_ceil(60)) }
            }
        })
    }

    /// Set the sleep timer (None: off).
    fn set_sleep(&mut self, sleep: Option<Sleep>, cx: &mut Context<Self>) {
        self.sleep = sleep;
        self.notice = Some(self.sleep_status().map_or("Sleep timer off".to_string(), |s| format!("Sleep timer: {}", s.trim_start_matches("Sleep "))));
        cx.notify();
    }

    /// The quality picked for the current video (a height, or 0 for audio only), if any.
    fn picked_quality(&self) -> Option<u32> {
        let id = &self.current.as_ref()?.id;
        self.quality.as_ref().filter(|(q, _)| q == id).map(|(_, h)| *h)
    }

    /// Play the current video again at another quality (None: the settings' maximum again), from
    /// where it is and paused or not as it was. Only for a video streaming here.
    fn pick_quality(&mut self, pick: Option<u32>, cx: &mut Context<Self>) {
        let Some(video) = self.current.clone() else { return };
        let Some((position, paused)) = self.state.as_ref().filter(|s| s.playing).map(|s| (s.position, s.paused)) else { return };
        self.quality = pick.map(|h| (video.id.clone(), h));
        self.link_start = Some(position);
        self.start(video, paused);
        // After `start`, which clears notices.
        self.notice = Some(match pick {
            None => format!("Quality: up to {}p (Settings)", self.settings.max_quality),
            Some(0) => "Quality: audio only".into(),
            Some(h) => format!("Quality: up to {h}p"),
        });
        self.sync_embed();
        cx.notify();
    }

    /// Save `video` with yt-dlp in the background, using the player's quality settings.
    fn download(&mut self, video: Video, cx: &mut Context<Self>) {
        let home = dirs::home_dir().unwrap_or_default();
        let dir = match self.settings.download_dir.trim() {
            "" => dirs::download_dir().unwrap_or_else(|| home.join("Downloads")),
            d => d.strip_prefix("~/").map_or_else(|| PathBuf::from(d), |rest| home.join(rest)),
        };
        let (cfg, format, url, id) = (self.cfg.clone(), player::format(&self.settings), video.url(), video.id.clone());
        let saved = video.clone();
        self.downloads.insert(id.clone(), Download { progress: 0., result: None, done_at: None });
        let shared: Arc<Mutex<Download>> = Arc::new(Mutex::new(Download { progress: 0., result: None, done_at: None }));
        let sink = shared.clone();
        blocking::unblock(move || {
                let res = yt::download(&cfg, &url, &format, &dir, |p| sink.lock().unwrap().progress = p);
                sink.lock().unwrap().result = Some(res);
            })
            .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(300)).await;
                let now = shared.lock().unwrap().clone();
                let done = now.result.is_some();
                if this.update(cx, |this, cx| {
                    let mut now = now;
                    if done {
                        now.done_at = Some(std::time::Instant::now());
                        if let Some(Ok(path)) = &now.result {
                            this.add_to_library(saved.clone(), path.clone());
                        }
                    }
                    this.downloads.insert(id.clone(), now);
                    cx.notify();
                })
                .is_err()
                    || done
                {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// The Downloads tab is on and has something in it.
    fn downloads_on(&self) -> bool {
        self.settings.downloads_tab && !self.library.is_empty()
    }

    /// A finished download goes to the top of the library (replacing an older copy).
    fn add_to_library(&mut self, video: Video, path: String) {
        self.library.retain(|s| s.video.id != video.id);
        self.library.insert(0, Saved { video, path });
        store::save_data("downloads", &self.library);
    }

    /// Forget downloads whose file is gone (deleted or moved outside the app).
    fn prune_library(&mut self) {
        let before = self.library.len();
        self.library.retain(|s| std::path::Path::new(&s.path).exists());
        if self.library.len() != before {
            store::save_data("downloads", &self.library);
        }
    }

    /// Delete a downloaded file and forget it; a file already gone is only forgotten.
    fn delete_download(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(i) = self.library.iter().position(|s| s.video.id == id) else { return };
        let path = self.library[i].path.clone();
        match std::fs::remove_file(&path) {
            Ok(()) => self.notice = Some(format!("Deleted {path}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => self.notice = Some("Removed from Downloads".into()),
            Err(e) => {
                self.notice = Some(format!("Couldn't delete {path}: {e}"));
                cx.notify();
                return;
            }
        }
        self.library.remove(i);
        self.downloads.remove(id);
        store::save_data("downloads", &self.library);
        // The tab goes away with its last video.
        if self.tab == Tab::Downloads && !self.downloads_on() {
            self.select_tab(self.tab_list()[0], cx);
        }
        cx.notify();
    }

    /// The Downloads tab's videos, as the list filter narrows them.
    fn library_videos(&self) -> Vec<Video> {
        self.library.iter().map(|s| s.video.clone()).filter(|v| self.video_matches(v)).collect()
    }

    /// What mpv plays for `video`: its downloaded file when there is one, else its page.
    fn media_path(&self, video: &Video) -> String {
        self.library
            .iter()
            .find(|s| s.video.id == video.id && std::path::Path::new(&s.path).exists())
            .map_or_else(|| video.url(), |s| s.path.clone())
    }

    /// The Downloads tab: videos saved with Download, newest first, played from their files.
    fn render_downloads(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let videos = self.library_videos();
        let body = if videos.is_empty() {
            self.status("No matches.")
        } else {
            self.video_list("downloads", &videos, Some(0), cx)
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(self.filter_bar("Filter downloads", window, cx))
            .child(div().flex().flex_col().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    /// The CC button under the player: only where the hover bar's can't be used (hover controls
    /// off, picture-in-picture, casting), since that one does the same.
    fn cc_under_player(&self) -> bool {
        self.settings.subtitles_button && (!self.settings.video_controls || self.pip || self.casting.is_some())
    }

    fn account_buttons(&self) -> bool {
        let st = &self.settings;
        self.cfg.has_auth() && (st.subscribe_button || st.save_button || st.watch_later_button || st.like_button || st.dislike_button)
    }

    /// Some of the playing video's info (views, date, subscribers) is switched on and available.
    fn info_wanted(&self) -> bool {
        let st = &self.settings;
        self.cfg.has_auth() && (st.show_views || st.show_date || st.show_subs)
    }

    /// Run `f` with the account on a background thread (loading it first if needed),
    /// then `done` on the UI thread.
    fn with_account<R: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        f: impl Fn(&Account) -> Result<R, String> + Send + 'static,
        done: impl FnOnce(&mut Self, Result<R, String>, &mut Context<Self>) + 'static,
    ) {
        let (cfg, account) = (self.cfg.clone(), self.account.clone());
        let task = blocking::unblock(move || {
            let cached = account.is_some();
            let mut account = match account {
                Some(a) => a,
                None => account::shared(&cfg)?,
            };
            // Cookies kept from an earlier start may have been rotated since, too.
            let cached = cached || account.from_kept;
            let mut res = f(&account);
            // A login kept for long enough stops being accepted (YouTube rotates its cookies):
            // try once more with the cookies as the browser has them now.
            if res.is_err() && cached {
                if let Ok(fresh) = Account::load_fresh(&cfg) {
                    account = Arc::new(fresh);
                    res = f(&account);
                }
            }
            Ok::<_, String>((account, res))
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                let res = match res {
                    Ok((account, res)) => {
                        account::remember(&account);
                        this.account = Some(account);
                        res
                    }
                    Err(e) => Err(e),
                };
                if let Err(e) = &res {
                    this.login_failed(e);
                }
                done(this, res, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The first-run screen is up: not dismissed yet, and no login configured.
    fn welcome_visible(&self) -> bool {
        !self.settings.welcome_seen && !self.cfg.has_auth()
    }

    /// Dismiss the first-run screen (Esc, or "Continue without account").
    fn finish_welcome(&mut self, cx: &mut Context<Self>) {
        self.settings.welcome_seen = true;
        self.settings.save();
        self.sync_embed();
        cx.notify();
    }

    /// "Connect YouTube" on the first-run screen: go to Settings, where the Connect panel lives.
    fn welcome_connect(&mut self, cx: &mut Context<Self>) {
        self.finish_welcome(cx);
        self.select_tab(Tab::Settings, cx);
    }

    /// Pick up the login from disk again (config.toml + auth.json) after it changed, and let
    /// lists that failed without one reload. mpv gets the new cookie options; a playing video
    /// restarts at its position, like after any player-setting change.
    fn reload_auth(&mut self, cx: &mut Context<Self>) {
        self.cfg = Arc::new(Config::load());
        self.account = None;
        account::forget();
        clear_failed(&mut self.subs.groups);
        clear_failed(&mut self.feed);
        clear_failed(&mut self.yt_history);
        clear_failed(&mut self.playlists.groups);
        clear_failed(&mut self.recs);
        clear_failed(&mut self.watch_later);
        if !matches!(self.tab, Tab::Settings | Tab::Search) {
            self.load_tab(cx);
        }
        if self.welcome_visible() {
            // The login vanished from disk: the connect screen takes over. Stopped first
            // so apply_player_settings doesn't restart the video with the new options.
            self.player.stop();
            self.current = None;
            self.state = None;
        }
        self.apply_player_settings(cx);
        self.sync_embed();
    }

    /// Save the selected browser as the login and run the three-step check.
    fn start_connect(&mut self, cx: &mut Context<Self>) {
        if self.cfg.cookies_file.is_some() || self.cfg.cookies_from_browser.is_some() {
            self.import_msg = Some((false, "config.toml sets a login, which wins over this one: remove it there first.".into()));
            cx.notify();
            return;
        }
        Auth::Browser(self.connect_browser.clone()).save();
        self.reload_auth(cx);
        self.connect = Connect::Running(1);
        self.run_probe(true, cx);
    }

    /// The check failed. A login that never worked (`rollback`: the first attempt) is dropped
    /// again, so the account tabs don't show up empty. Ignored after a log out meanwhile.
    fn fail_probe(&mut self, step: u8, error: String, rollback: bool, cx: &mut Context<Self>) {
        if !self.cfg.has_auth() {
            return;
        }
        self.connect = Connect::Failed { step, error: session_hint(error) };
        if rollback && self.cfg.cookies_file.is_none() && self.cfg.cookies_from_browser.is_none() {
            Auth::clear();
            self.reload_auth(cx);
        }
        cx.notify();
    }

    /// The check, step by step, off the UI thread: profile readable → session valid →
    /// feed reachable. Each step reports back with `cx.notify()`.
    fn run_probe(&mut self, rollback: bool, cx: &mut Context<Self>) {
        self.import_msg = None;
        let cfg = self.cfg.clone();
        cx.spawn(async move |this, cx| {
            let loaded = blocking::unblock({ let cfg = cfg.clone(); move || Account::load_fresh(&cfg) }).await;
            let account = match loaded {
                Ok(account) => account,
                Err(error) => {
                    this.update(cx, |this, cx| this.fail_probe(1, error, rollback, cx)).ok();
                    return;
                }
            };
            this.update(cx, |this, cx| {
                if !this.cfg.has_auth() {
                    return;
                }
                // The export may have persisted a keyring-qualified spec ("brave+gnomekeyring");
                // pick it up so lists and player restart with the credentials that work.
                if this.cfg.cookies_file.is_none() && this.cfg.cookies_from_browser.is_none()
                    && Auth::load() != this.cfg.auth
                {
                    this.reload_auth(cx);
                }
                this.connect = Connect::Running(2);
                cx.notify();
            })
            .ok();
            let loaded = { let account = account.clone(); blocking::unblock(move || { account.me() }).await };
            let me = match loaded {
                Ok(me) => me,
                Err(error) => {
                    this.update(cx, |this, cx| this.fail_probe(2, error, rollback, cx)).ok();
                    return;
                }
            };
            this.update(cx, |this, cx| {
                if !this.cfg.has_auth() {
                    return;
                }
                this.connect = Connect::Running(3);
                cx.notify();
            })
            .ok();
            // Read the config again — the export may have learned the keyring spec.
            let cfg = Arc::new(Config::load());
            let probe = { let cfg = cfg.clone(); blocking::unblock(move || { yt::auth_probe(&cfg) }).await };
            this.update(cx, |this, cx| {
                if !this.cfg.has_auth() {
                    return;
                }
                match probe {
                    Ok(()) => {
                        let account = Arc::new(account);
                        account::remember(&account);
                        this.account = Some(account);
                        this.connect = Connect::Done { me };
                        // Re-fetch the playing video's like/subscribe state under the new login.
                        this.status = None;
                        this.status_requested = None;
                        if rollback {
                            // First connect: like after a launch with a login, the latest
                            // YouTube history video waits in the player (not playing; set by
                            // after_load once the list is there), and the home feed loads.
                            // A video that isn't playing is only the logged-out placeholder: replace it.
                            // Neither is preloaded: mpv's `mark-watched` would add it to YouTube's
                            // history, though it was only shown.
                            if this.state.is_none() {
                                this.current = None;
                            }
                            this.preloaded = true;
                            this.load_yt_history(cx);
                            if this.settings.recommendations {
                                this.load_recs(cx);
                            }
                        }
                    }
                    Err(error) => {
                        this.fail_probe(3, error, rollback, cx);
                        return;
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Log out: forget the app-managed login; for a config.toml login, comment its two
    /// lines out instead (the rest of the file is untouched, uncomment to return).
    fn logout(&mut self, cx: &mut Context<Self>) {
        let from_toml = self.cfg.cookies_file.is_some() || self.cfg.cookies_from_browser.is_some();
        let word = if from_toml {
            match store::disable_login_in_config() {
                Ok(true) => "Logged out — the login in config.toml was commented out, uncomment it to return.",
                Ok(false) => "Logged out.",
                Err(e) => {
                    self.import_msg = Some((false, e));
                    cx.notify();
                    return;
                }
            }
        } else {
            "Logged out."
        };
        // The session's video goes with the account: stop it and clear the pane before
        // reload_auth, so it can't restart the video with the new (empty) cookie options.
        // The local history on disk stays for the next login.
        self.player.stop();
        self.current = None;
        self.state = None;
        self.ended = None;
        self.lower = Lower::Recommended;
        self.comments = Load::Idle;
        self.comments_for = None;
        Auth::clear();
        self.reload_auth(cx);
        // Account fetches still running must not land in the slots cleared below.
        for key in ["subs", "feed", "history", "playlists", "recs"] {
            *self.generations.entry(key).or_insert(0) += 1;
        }
        // Account lists are dropped from memory (the disk caches return with the next
        // login), and the connect screen comes back, now and on the next launch.
        self.subs.groups = Load::Idle;
        self.subs.videos = Load::Idle;
        self.playlists.groups = Load::Idle;
        self.playlists.videos = Load::Idle;
        self.yt_history = Load::Idle;
        self.recs = Load::Idle;
        self.feed = Load::Idle;
        self.watch_later = Load::Idle;
        // No open channels behind the tabs that just disappeared.
        self.subs.open = None;
        self.playlists.open = None;
        self.tab = match self.tab {
            Tab::Playlists | Tab::History => Tab::Subscriptions,
            t => t,
        };
        // The logged-out home, so the left list is ready behind the connect screen.
        self.load_anon(cx);
        self.settings.welcome_seen = false;
        self.settings.save();
        self.connect = Connect::Idle;
        self.import_msg = Some((true, word.to_string()));
        self.sync_embed();
        cx.notify();
    }

    /// Back to browser selection after a failed attempt (app-managed login only).
    fn choose_another_browser(&mut self, cx: &mut Context<Self>) {
        Auth::clear();
        self.reload_auth(cx);
        self.connect = Connect::Idle;
        cx.notify();
    }

    /// Validate a cookies.txt and keep a private 0600 copy of it as the login.
    fn import_cookies(&mut self, cx: &mut Context<Self>) {
        let given = self.import_path.trim();
        if given.is_empty() {
            return;
        }
        if self.cfg.cookies_file.is_some() || self.cfg.cookies_from_browser.is_some() {
            self.import_msg = Some((false, "config.toml sets a login, which wins over this one: remove it there first.".into()));
            cx.notify();
            return;
        }
        let path = match given.strip_prefix("~/") {
            Some(rest) => dirs::home_dir().unwrap_or_default().join(rest),
            None => PathBuf::from(given),
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                self.import_msg = Some((false, format!("{}: {e}", path.display())));
                cx.notify();
                return;
            }
        };
        if let Err(e) = Account::from_cookies(&text) {
            self.import_msg = Some((false, format!("{e} — expected a Netscape cookies.txt from a browser logged in to YouTube")));
            cx.notify();
            return;
        }
        let dest = Auth::imported_cookies_path();
        let _ = std::fs::create_dir_all(store::config_dir());
        // Created 0600 from the start, so the session is never readable by others.
        let copy = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&dest)
            .and_then(|mut f| std::io::Write::write_all(&mut f, text.as_bytes()));
        if let Err(e) = copy {
            self.import_msg = Some((false, format!("{}: {e}", dest.display())));
            cx.notify();
            return;
        }
        Auth::CookiesFile(dest.clone()).save();
        self.reload_auth(cx);
        self.connect = Connect::Idle;
        self.import_path.clear();
        self.import_msg = Some((true, format!("YouTube cookies imported to {}", dest.display())));
        cx.notify();
    }

    /// Fetch whether the current video is liked and its channel subscribed.
    fn load_status(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        // Also for a video that came without its channel (a Short from YouTube's history): the
        // watch page names it.
        let no_channel = self.cfg.has_auth() && self.current.as_ref().is_some_and(|v| v.channel_url.is_none());
        if !(self.account_buttons() || self.info_wanted() || no_channel) || self.status_requested.as_ref() == Some(&id) {
            return;
        }
        self.status_requested = Some(id.clone());
        let vid = id.clone();
        self.with_account(cx, move |a| a.status(&vid), move |this, res, _| match res {
            Ok(st) if this.current.as_ref().is_some_and(|v| v.id == id) => {
                this.fill_channel(&id, &st);
                this.status = Some((id, st));
            }
            Ok(_) => {}
            Err(e) => this.notice = Some(e),
        });
    }

    /// A video without its channel (a Short from YouTube's history) gets it from its watch page:
    /// the playing one (so the channel button shows), and its rows in History.
    fn fill_channel(&mut self, id: &str, st: &account::VideoStatus) {
        let (Some(name), Some(channel)) = (&st.channel, &st.channel_id) else { return };
        let url = format!("https://www.youtube.com/channel/{channel}");
        let fill = |v: &mut Video| {
            if v.id == id && v.channel_url.is_none() {
                v.channel = Some(name.clone());
                v.channel_url = Some(url.clone());
            }
        };
        if let Some(v) = self.current.as_mut() {
            fill(v);
        }
        if let Load::Ready(list) | Load::Loading(list) = &mut self.yt_history {
            if list.iter().any(|v| v.id == id && v.channel_url.is_none()) {
                list.iter_mut().for_each(fill);
                store::save_list("history", list);
            }
        }
        self.history.set_channel(id, name, &url);
        self.history.save();
    }

    /// DeArrow's title and thumbnail for a video, when either is turned on: asked once, the first
    /// time a row or the player shows the video (like thumbnails, only for what is on screen).
    fn dearrow(&mut self, id: &str, cx: &mut Context<Self>) -> Option<dearrow::Branding> {
        if !self.settings.dearrow_titles && !self.settings.dearrow_thumbs {
            return None;
        }
        if let Some(known) = self.dearrow.get(id) {
            return known.clone();
        }
        self.dearrow.insert(id.to_string(), None);
        let id = id.to_string();
        let task = {
            let id = id.clone();
            blocking::unblock(move || dearrow::branding(&id))
        };
        let start = Instant::now();
        cx.spawn(async move |this, cx| {
            // Quietly nothing on a failure: YouTube's own title and thumbnail stay.
            let res = task.await;
            yt::timing(&format!("dearrow {id}: {}", match &res { Ok(b) => format!("{b:?}"), Err(e) => format!("failed ({e})") }), start);
            if let Ok(b) = res {
                this.update(cx, |this, cx| {
                    let changed = b != dearrow::Branding::default();
                    this.dearrow.insert(id, Some(b));
                    if changed {
                        cx.notify();
                    }
                })
                .ok();
            }
        })
        .detach();
        None
    }

    /// The title to show for `video`, and YouTube's own when DeArrow's replaces it.
    fn shown_title(&mut self, video: &Video, cx: &mut Context<Self>) -> (String, Option<String>) {
        let better = self.dearrow(&video.id, cx).and_then(|b| b.title).filter(|_| self.settings.dearrow_titles);
        match better {
            Some(t) if t != video.title => (t, Some(video.title.clone())),
            _ => (video.title.clone(), None),
        }
    }

    /// The thumbnail to show for `video`: DeArrow's frame when there is one and it loaded, else
    /// YouTube's (also while DeArrow's is still on its way).
    fn video_thumb(&mut self, video: &Video, w: f32, h: f32, radius: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let frame = self.dearrow(&video.id, cx).and_then(|b| b.thumb).filter(|_| self.settings.dearrow_thumbs);
        if let Some(time) = frame {
            let key = format!("{}-dearrow-{}", video.id, (time * 1000.) as u64);
            if !self.thumbs_failed.contains(&key) {
                let url = dearrow::thumb_url(&video.id, time);
                if let Thumb::Ready(_) = self.thumb(&key, Some(url), cx) {
                    return self.thumb_el(&key, None, w, h, radius, cx);
                }
            }
        }
        self.thumb_el(&video.id, Some(video.thumb_url()), w, h, radius, cx)
    }

    /// Fetch the playing video's like and dislike counts once (Return YouTube Dislike).
    fn load_votes(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        if !self.settings.show_votes || self.votes.as_ref().is_some_and(|(v, _)| *v == id) {
            return;
        }
        self.votes = Some((id.clone(), None));
        let task = {
            let id = id.clone();
            blocking::unblock(move || { yt::votes(&id) })
        };
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                // Quietly nothing on a failure: the counts are a nicety.
                if let (Ok(counts), Some((v, slot))) = (res, this.votes.as_mut()) {
                    if *v == id {
                        *slot = Some(counts);
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn current_status(&self) -> Option<&VideoStatus> {
        let (id, st) = self.status.as_ref()?;
        (self.current.as_ref()?.id == *id).then_some(st)
    }

    fn toggle_subscribe(&mut self, cx: &mut Context<Self>) {
        let Some(st) = self.current_status().cloned() else { return };
        let Some(channel) = st.channel_id.clone() else { return };
        let on = !st.subscribed;
        if let Some((_, s)) = &mut self.status {
            s.subscribed = on;
        }
        self.with_account(cx, move |a| a.subscribe(&channel, on), move |this, res, _| {
            this.notice = Some(match res {
                Ok(()) if on => "Subscribed".into(),
                Ok(()) => "Unsubscribed".into(),
                Err(e) => {
                    if let Some((_, s)) = &mut this.status {
                        s.subscribed = !on;
                    }
                    e
                }
            });
        });
    }

    fn toggle_like(&mut self, cx: &mut Context<Self>) {
        let Some(st) = self.current_status().cloned() else { return };
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        let on = !st.liked;
        if let Some((_, s)) = &mut self.status {
            s.liked = on;
            // YouTube keeps one rating: liking replaces a dislike.
            s.disliked &= !on;
        }
        self.with_account(cx, move |a| a.like(&id, on), move |this, res, _| match res {
            Ok(()) => this.notice = Some(if on { "Liked".into() } else { "Like removed".into() }),
            Err(e) => {
                if let Some((_, s)) = &mut this.status {
                    s.liked = !on;
                }
                this.notice = Some(e);
            }
        });
    }

    fn toggle_dislike(&mut self, cx: &mut Context<Self>) {
        let Some(st) = self.current_status().cloned() else { return };
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        let on = !st.disliked;
        if let Some((_, s)) = &mut self.status {
            s.disliked = on;
            s.liked &= !on;
        }
        self.with_account(cx, move |a| a.dislike(&id, on), move |this, res, _| match res {
            Ok(()) => this.notice = Some(if on { "Disliked".into() } else { "Dislike removed".into() }),
            Err(e) => {
                if let Some((_, s)) = &mut this.status {
                    s.disliked = !on;
                }
                this.notice = Some(e);
            }
        });
    }

    fn save_to(&mut self, playlist: Group, cx: &mut Context<Self>) {
        let Some(video) = self.current.clone() else { return };
        self.close_save(cx);
        self.save_video_to(video, playlist, cx);
    }

    /// Add any video to a playlist (Watch later too, which also updates its tab's list).
    fn save_video_to(&mut self, video: Video, playlist: Group, cx: &mut Context<Self>) {
        if playlist.id == "WL" {
            self.add_to_watch_later(video, cx);
            return;
        }
        let id = video.id;
        self.notice = Some(format!("Saving to {}…", playlist.title));
        let pid = playlist.id.clone();
        self.with_account(cx, move |a| a.save_to_playlist(&pid, &id), move |this, res, _| {
            this.notice = Some(match res {
                Ok(()) => format!("Saved to {}", playlist.title),
                Err(e) => e,
            });
        });
    }

    /// Remove `video` from `playlist` on YouTube (Liked videos: unlike it), then from the open list.
    fn remove_from_playlist(&mut self, playlist: Group, video: Video, cx: &mut Context<Self>) {
        self.notice = Some(format!("Removing from {}…", playlist.title));
        let (pid, vid) = (playlist.id.clone(), video.id.clone());
        self.with_account(
            cx,
            move |a| if pid == "LL" { a.like(&vid, false) } else { a.remove_from_playlist(&pid, &vid) },
            move |this, res, _| {
                this.notice = Some(match res {
                    Ok(()) => {
                        if playlist.id == "WL" {
                            if let Load::Ready(v) | Load::Loading(v) = &mut this.watch_later {
                                v.retain(|x| x.id != video.id);
                                store::save_list("watch-later", v);
                            }
                        }
                        if let Load::Ready(v) | Load::Loading(v) = &mut this.playlists.videos {
                            v.retain(|x| x.id != video.id);
                            if this.playlists.open.as_ref().is_some_and(|g| g.id == playlist.id) {
                                store::save_list(&format!("group-{}", playlist.id), v);
                            }
                        }
                        format!("Removed from {}", playlist.title)
                    }
                    Err(e) => e,
                });
            },
        );
    }

    /// What Next / autoplay plays: the Up next queue first, then the list the video came from.
    fn next_video(&self) -> Option<Video> {
        self.up_next.first().cloned().or_else(|| self.neighbor(1))
    }

    /// Add the open playlist's videos to Up next (skipping ones already queued).
    fn enqueue_all(&mut self, tab: Tab, cx: &mut Context<Self>) {
        let videos = self.visible(&self.browser_videos(tab));
        let before = self.up_next.len();
        for v in videos {
            if !self.up_next.iter().any(|q| q.id == v.id) {
                self.up_next.push(v);
            }
        }
        store::save_data("up_next", &self.up_next);
        let added = self.up_next.len() - before;
        self.lower = Lower::UpNext;
        if self.current.is_none() && !self.up_next.is_empty() {
            let v = self.up_next.remove(0);
            store::save_data("up_next", &self.up_next);
            self.start(v, false);
        }
        // After `start`, which clears notices.
        self.notice = Some(format!("Added {added} videos to Up next"));
        cx.notify();
    }

    fn enqueue(&mut self, video: Video, cx: &mut Context<Self>) {
        if !self.up_next.iter().any(|v| v.id == video.id) {
            self.notice = Some(format!("Added to Up next: {}", video.title));
            self.up_next.push(video);
            store::save_data("up_next", &self.up_next);
        }
        cx.notify();
    }

    fn dequeue(&mut self, id: &str, cx: &mut Context<Self>) {
        self.up_next.retain(|v| v.id != id);
        store::save_data("up_next", &self.up_next);
        cx.notify();
    }

    /// Copy the current video's link (Share button, C, or yy in Vim mode).
    fn copy_link(&mut self, cx: &mut Context<Self>) {
        let Some(video) = self.current.clone() else { return };
        self.copy_video_link(&video, cx);
    }

    fn copy_video_link(&mut self, video: &Video, cx: &mut Context<Self>) {
        let link = format!("https://youtu.be/{}", video.id);
        cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
        self.notice = Some(format!("Link copied: {link}"));
        cx.notify();
    }

    /// Drop `id` from the history list here. YouTube's own history can't be changed from the app,
    /// so an entry that came from it is hidden until the next refresh brings it back.
    fn forget_video(&mut self, id: &str, cx: &mut Context<Self>) {
        self.history.items.retain(|w| w.video.id != id);
        self.history.save();
        let from_youtube = self.yt_history.items().iter().any(|v| v.id == id);
        if let Load::Ready(v) | Load::Loading(v) = &mut self.yt_history {
            v.retain(|x| x.id != id);
        }
        store::save_list("history", self.yt_history.items());
        self.notice = Some(if from_youtube {
            "Removed here. It stays in your YouTube history and may come back on refresh.".into()
        } else {
            "Removed from history".into()
        });
        cx.notify();
    }

    /// Add the playing video to Watch later (the button and the `w` key).
    fn watch_later(&mut self, cx: &mut Context<Self>) {
        if let Some(v) = self.current.clone() {
            self.add_to_watch_later(v, cx);
        }
    }

    /// Add any video to Watch later (the row buttons, the player button and the key), and to the
    /// Watch later tab's list if that is loaded.
    fn add_to_watch_later(&mut self, video: Video, cx: &mut Context<Self>) {
        self.notice = Some("Adding to Watch later…".into());
        let vid = video.id.clone();
        self.with_account(cx, move |a| a.save_to_playlist("WL", &vid), move |this, res, _| {
            this.notice = Some(match res {
                Ok(()) => {
                    if let Load::Ready(v) | Load::Loading(v) = &mut this.watch_later {
                        if !v.iter().any(|x| x.id == video.id) {
                            v.insert(0, video.clone());
                        }
                        store::save_list("watch-later", v);
                    }
                    "Added to Watch later".to_string()
                }
                Err(e) => e,
            });
        });
    }

    /// Watch later as a group, for the remove button on its rows.
    fn watch_later_group() -> Group {
        Group { id: "WL".into(), title: "Watch later".into(), url: ":ytwatchlater".into(), thumb: None, subscribers: None }
    }

    /// The Watch later tab is on and you are logged in.
    fn watch_later_on(&self) -> bool {
        self.settings.watch_later_tab && self.cfg.has_auth()
    }

    /// The Watch later tab: fetched the first time it is shown.
    fn render_watch_later(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if matches!(self.watch_later, Load::Idle) {
            self.fetch(cx, "watch-later", |s| &mut s.watch_later, Some("watch-later".into()), |cfg, on| yt::group_videos(cfg, ":ytwatchlater", on));
        }
        if let Some(p) = self.placeholder(&self.watch_later, "Watch later is empty.", Rows::Videos) {
            return p;
        }
        let v = self.watch_later.items().to_vec();
        self.video_list("watch-later", &v, None, cx)
    }

    fn open_in_browser(&mut self, cx: &mut Context<Self>) {
        let Some(url) = self.current.as_ref().map(|v| v.url()) else { return };
        self.open_url(&url, cx);
    }

    /// Open a link in the default browser.
    fn open_url(&mut self, url: &str, cx: &mut Context<Self>) {
        match std::process::Command::new("xdg-open").arg(url).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
            // Reaped in the background so it doesn't linger as a zombie.
            Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
            Err(e) => self.notice = Some(format!("Cannot open the browser: {e}")),
        }
        cx.notify();
    }

    /// At most one column is hidden: hiding one brings the other back.
    fn toggle_left_collapsed(&mut self, cx: &mut Context<Self>) {
        self.left_collapsed = !self.left_collapsed;
        self.right_collapsed = false;
        self.sync_embed();
        cx.notify();
    }

    fn toggle_right_collapsed(&mut self, cx: &mut Context<Self>) {
        self.right_collapsed = !self.right_collapsed;
        self.left_collapsed = false;
        self.sync_embed();
        cx.notify();
    }

    /// Give the lower pane the whole right column, or take it back. Nothing to do without a lower pane.
    fn toggle_lower_full(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        if self.lower_full || self.settings.recommendations || !self.up_next.is_empty() || !self.chapter_list().is_empty() {
            self.lower_full = !self.lower_full;
            self.player_full = false;
            self.sync_embed();
            cx.notify();
        }
    }

    /// Next playback speed in SPEEDS, wrapping around.
    fn cycle_speed(&mut self, cx: &mut Context<Self>) {
        let i = SPEEDS.iter().position(|s| *s == self.settings.speed).map_or(1, |i| (i + 1) % SPEEDS.len());
        self.settings.speed = SPEEDS[i];
        self.settings.save();
        self.player.set_speed(self.settings.speed);
        cx.notify();
    }

    /// Hide the lower pane so the player takes the whole right column, or bring it back.
    fn toggle_player_full(&mut self, cx: &mut Context<Self>) {
        self.player_full = !self.player_full;
        self.lower_full = false;
        self.sync_embed();
        cx.notify();
    }

    /// Copy a link that opens at the current playback position (the resume position when not playing).
    fn copy_link_at_time(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        let secs = self.state.as_ref().map_or_else(|| self.history.position(&id), |s| s.position) as u64;
        let link = if secs > 0 { format!("https://youtu.be/{id}?t={secs}") } else { format!("https://youtu.be/{id}") };
        cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
        self.notice = Some(format!("Link copied at {}: {link}", fmt_duration(secs as f64)));
        cx.notify();
    }

    /// Whether the Cast button shows: a target in config.toml, and the button on in Settings.
    fn cast_available(&self) -> bool {
        self.settings.cast_button && !self.cast_targets().is_empty()
    }

    /// Where the video can be cast: the command from Settings, which wins, else config.toml's targets.
    fn cast_targets(&self) -> std::collections::BTreeMap<String, store::CastTarget> {
        let command = cast::split_command(&self.settings.cast_command);
        if command.is_empty() {
            return self.cfg.cast.clone();
        }
        [("device".to_string(), store::CastTarget { command, ..Default::default() })].into()
    }

    /// Send the playing video to a cast target (the first one for the T key).
    fn cast_to(&mut self, name: Option<String>, cx: &mut Context<Self>) {
        let Some(video) = self.current.clone() else { return };
        self.cast_list(name, vec![video], cx);
    }

    /// Send `videos` to a cast target, the first now and the rest after it. A target with a `url`
    /// is asked over the remote API (a list becomes the receiver's queue, 200 videos at most) and
    /// then controlled from here; a command target gets the first video only. Either way happens
    /// on a background thread, then the local video pauses.
    fn cast_list(&mut self, name: Option<String>, videos: Vec<Video>, cx: &mut Context<Self>) {
        let Some(video) = videos.first().cloned() else { return };
        let targets = self.cast_targets();
        let Some((name, target)) = name
            .and_then(|n| targets.get(&n).map(|t| (n, t.clone())))
            .or_else(|| targets.iter().next().map(|(n, t)| (n.clone(), t.clone())))
        else {
            self.notice = Some("No cast target: add [cast.<name>] to config.toml".into());
            cx.notify();
            return;
        };
        // Where the first one is now (the saved position unless it is the one playing here).
        let live = self.current.as_ref().is_some_and(|c| c.id == video.id).then(|| self.state.as_ref().map(|s| s.position)).flatten();
        let start = live.unwrap_or_else(|| self.history.position(&video.id)).max(0.) as u64;
        let title = video.title.clone();
        let remote = target.url.as_deref().map(|u| cast::Remote::new(u, target.token.as_deref().unwrap_or_default()));
        let command = cast::expand(&target.command, &cast::Playing { url: &video.url(), id: &video.id, title: &title, start });
        // The receiver holds 2000 at most, and takes 200 per request.
        let items: Vec<Video> = videos.iter().take(cast::MAX_LIST).cloned().collect();
        let urls: Vec<String> = items.iter().take(cast::CHUNK).map(|v| v.url()).collect();
        let sent = urls.len();
        let listed = remote.is_some() && items.len() > 1;
        self.notice = Some(format!("Casting to {name}…"));
        let task_remote = remote.clone();
        let items_len = items.len();
        let task = blocking::unblock(move || {
            match task_remote {
                Some(r) if items_len > 1 => r.play_list(&urls, 0, start),
                Some(r) => r.play(&urls[0], start),
                None => cast::run(&command, Duration::from_secs(60)),
            }
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(()) => {
                        this.notice = Some(format!("Sent to {name}"));
                        // Otherwise it plays here too.
                        this.player.pause();
                        this.casting = remote.map(|remote| Casting {
                            name,
                            remote,
                            status: None,
                            seen: false,
                            started: Instant::now(),
                            polling: false,
                            listed,
                            items: items.clone(),
                            marked: Default::default(),
                            sent,
                        });
                        // A command target can't be asked what plays: its one video counts now.
                        if this.casting.is_none() {
                            this.mark_watched(&items[0].id, cx);
                        }
                        this.sync_embed();
                    }
                    // A video picked in the cast view didn't go out: the receiver still plays
                    // the old one, which the view no longer describes.
                    Err(e) if this.casting.as_ref().is_some_and(|c| c.polling && c.status.is_none()) => {
                        this.end_cast(&format!("Cast to {name} failed: {e}"), cx)
                    }
                    Err(e) => this.notice = Some(format!("Cast to {name} failed: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Send the next chunk of a long list when the receiver's queue is within 50 of its end.
    fn refill_cast(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.casting.as_mut().filter(|c| c.listed) else { return };
        let Some((pos, _)) = c.status.as_ref().and_then(|s| s.queue) else { return };
        let Some(range) = cast::next_chunk(pos, c.sent, c.items.len()) else { return };
        let chunk: Vec<String> = c.items[range].iter().map(|v| v.url()).collect();
        let (remote, before) = (c.remote.clone(), c.sent);
        c.sent += chunk.len();
        let task = blocking::unblock(move || { remote.queue_add(&chunk) });
        cx.spawn(async move |this, cx| {
            if let Err(e) = task.await {
                this.update(cx, |this, cx| {
                    // The rest isn't sent (an older bot has no /queue): the list ends where it was.
                    if let Some(c) = this.casting.as_mut() {
                        c.items.truncate(before);
                        c.sent = before;
                    }
                    this.notice = Some(format!("Couldn't add more of the list: {e}"));
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Record `id` in the account's YouTube history (a background yt-dlp call); only when logged in.
    fn mark_watched(&mut self, id: &str, _cx: &mut Context<Self>) {
        if !self.cfg.has_auth() {
            return;
        }
        let (cfg, id) = (self.cfg.clone(), id.to_string());
        blocking::unblock(move || {
                if let Err(e) = yt::mark_watched(&cfg, &id) {
                    eprintln!("unbloatedtube: couldn't add {id} to the history: {e}");
                }
            })
            .detach();
    }

    /// A command from the command line (see cli.rs); the answer it prints.
    fn run_command(&mut self, command: cli::Command, window: &mut Window, cx: &mut Context<Self>) -> cli::Reply {
        use cli::Command::*;
        let paused = self.casting.as_ref().map_or_else(
            || self.state.as_ref().filter(|s| s.playing).map(|s| s.paused),
            |c| c.status.as_ref().filter(|s| s.playing).map(|s| s.paused),
        );
        let nothing = || Err("nothing is playing".to_string());
        match command {
            Open { url } => {
                let link = yt::parse_link(&url).ok_or_else(|| format!("not a YouTube link: {url}"))?;
                // The link replaces the last-watched video, so loading that first would only flash it.
                self.preloaded = true;
                self.open_link(link, cx);
                window.activate_window();
                Ok("Opening".into())
            }
            Queue { url } => {
                let link = yt::parse_video_link(&url).ok_or_else(|| format!("not a YouTube video link: {url}"))?;
                let (cfg, id) = (self.cfg.clone(), link.id);
                let task = blocking::unblock(move || { yt::video(&cfg, &id) });
                cx.spawn(async move |this, cx| {
                    let res = task.await;
                    this.update(cx, |this, cx| match res {
                        Ok(video) => this.enqueue(video, cx),
                        Err(e) => {
                            this.notice = Some(format!("Couldn't add the link to Up next: {e}"));
                            cx.notify();
                        }
                    })
                    .ok();
                })
                .detach();
                Ok("Adding to Up next".into())
            }
            Pause | Play | Toggle => {
                let Some(now) = paused else { return nothing() };
                if matches!((&command, now), (Pause, false) | (Play, true) | (Toggle, _)) {
                    self.pause_toggle(cx);
                }
                Ok(if matches!((&command, now), (Pause, _) | (Toggle, false)) { "Paused" } else { "Playing" }.into())
            }
            Next | Prev => {
                if paused.is_none() && self.current.is_none() {
                    return nothing();
                }
                self.next_prev(matches!(command, Next), cx);
                Ok("OK".into())
            }
            Seek { secs, relative } => {
                if paused.is_none() {
                    return nothing();
                }
                if relative { self.seek_by(secs, cx) } else { self.seek_to(secs.max(0.), cx) }
                Ok("OK".into())
            }
            Status => Ok(self.status_line()),
            Raise => {
                window.activate_window();
                Ok("OK".into())
            }
            Quit => {
                self.quit_cast(false, window);
                Ok("Closing".into())
            }
        }
    }

    /// A media key or the desktop's player widget (MPRIS): like the command line, except that
    /// play on a video that isn't loaded starts it.
    fn run_mpris(&mut self, command: cli::Command, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(command, cli::Command::Play | cli::Command::Toggle) && self.mpris_snapshot().0.status == "Stopped" {
            if let Some(v) = self.current.clone() {
                return self.play(v, None, cx);
            }
        }
        let _ = self.run_command(command, window, cx);
    }

    /// What plays, for MPRIS (the receiver's video while casting), and the position.
    fn mpris_snapshot(&self) -> (mpris::Snapshot, f64) {
        let Some(v) = &self.current else { return (mpris::Snapshot { status: "Stopped", ..Default::default() }, 0.) };
        let playing = match &self.casting {
            Some(c) => c.status.as_ref().filter(|s| s.playing).map(|s| (s.paused, s.position, s.duration)),
            None => self.state.as_ref().filter(|s| s.playing && s.path == self.media_path(v)).map(|s| (s.paused, s.position, s.duration)),
        };
        let (status, position, duration) = match playing {
            Some((paused, position, duration)) => (if paused { "Paused" } else { "Playing" }, position, duration),
            None => ("Stopped", 0., 0.),
        };
        let now = mpris::Snapshot {
            status,
            id: v.id.clone(),
            title: v.title.clone(),
            channel: v.channel.clone().unwrap_or_default(),
            url: v.url(),
            art: v.thumb_url(),
            duration: if duration > 0. { duration } else { v.duration.unwrap_or(0.) },
        };
        (now, position)
    }

    /// What `unbloatedtube status` prints.
    fn status_line(&self) -> String {
        let (title, state, position, duration, target) = match &self.casting {
            Some(c) => match c.status.as_ref().filter(|s| s.playing) {
                Some(s) => (Some(s.title.clone()), if s.paused { "paused" } else { "playing" }, s.position, s.duration, Some(c.name.clone())),
                None => (None, "idle", 0., 0., Some(c.name.clone())),
            },
            None => match self.state.as_ref().filter(|s| s.playing) {
                Some(s) => (self.current.as_ref().map(|v| v.title.clone()), if s.paused { "paused" } else { "playing" }, s.position, s.duration, None),
                None => (None, "idle", 0., 0., None),
            },
        };
        let mut lines = vec![format!("state: {state}")];
        lines.extend(title.map(|t| format!("title: {t}")));
        if state != "idle" {
            lines.push(format!("position: {} / {}", fmt_duration(position), fmt_duration(duration)));
        }
        lines.extend(target.map(|t| format!("casting to: {t}")));
        lines.push(format!("up next: {}", self.up_next.len()));
        lines.join("\n")
    }

    /// Next or previous: the receiver's queue while casting a list, else the list here.
    fn next_prev(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.casting.as_ref().is_some_and(|c| c.listed) {
            self.cast_ctl(serde_json::json!({ "action": if forward { "next" } else { "prev" } }), cx);
        } else if forward {
            self.play_next(cx);
        } else if let Some(v) = self.neighbor(-1) {
            self.play(v, None, cx);
        }
    }

    /// Play what Next plays here (Up next first, then the list), and say so when it came from Up
    /// next: that is the part that looks like a bug when you were going through a playlist.
    fn play_next(&mut self, cx: &mut Context<Self>) {
        let from_queue = !self.up_next.is_empty();
        let Some(v) = self.next_video() else { return };
        let title = v.title.clone();
        self.play(v, None, cx);
        // After `play`, which clears notices.
        if from_queue {
            self.notice = Some(format!("Next from Up next: {title}"));
        }
    }

    /// Ask the cast receiver how it is doing, at most one question at a time. It ends the cast
    /// view when the receiver has stopped (finished, closed, or someone pressed stop on the TV).
    fn poll_cast(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.casting.as_mut().filter(|c| !c.polling) else { return };
        c.polling = true;
        let (remote, started) = (c.remote.clone(), c.started);
        let task = blocking::unblock(move || { remote.status() });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                // Gone, or another video was cast meanwhile: this answer is about the old one.
                let Some(c) = this.casting.as_mut().filter(|c| c.started == started) else { return };
                c.polling = false;
                let alive = matches!(&res, Ok(s) if s.playing || s.queue.is_some());
                if let Ok(s) = res {
                    c.seen |= s.playing || s.queue.is_some();
                    c.status = Some(s);
                }
                // The video that just started counts as watched: it goes in the YouTube history.
                let now = c.status.as_ref().filter(|s| s.playing).map(|s| s.queue.map_or(0, |(i, _)| i));
                let fresh = now.filter(|i| *i < c.items.len() && c.marked.insert(*i)).map(|i| c.items[i].id.clone());
                if let Some(id) = fresh {
                    this.mark_watched(&id, cx);
                }
                // The main video area follows the receiver: title, picture and details of the
                // video that plays there. (Like playing it here, it leaves Up next.)
                let shown = now.and_then(|i| this.casting.as_ref().and_then(|c| c.items.get(i).cloned()));
                if let Some(v) = shown.filter(|v| this.current.as_ref().is_none_or(|c| c.id != v.id)) {
                    if let Some(i) = this.up_next.iter().position(|q| q.id == v.id) {
                        this.up_next.remove(i);
                        store::save_data("up_next", &this.up_next);
                    }
                    this.history.touch(&v);
                    this.current = Some(v);
                    cx.notify();
                }
                this.refill_cast(cx);
                let Some(c) = this.casting.as_mut() else { return };
                if (c.seen && !alive) || (!c.seen && c.started.elapsed() > Duration::from_secs(90)) {
                    this.end_cast("Cast ended", cx);
                } else {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Leave the cast view and show the local (still paused) video again.
    fn end_cast(&mut self, notice: &str, cx: &mut Context<Self>) {
        self.casting = None;
        // The main area followed the receiver; an mpv holding another video is dropped, so
        // pressing play starts the one shown (a paused leftover would show the wrong picture).
        if let (Some(s), Some(v)) = (&self.state, &self.current) {
            if s.path != self.media_path(v) {
                self.player.stop();
                self.state = None;
            }
        }
        self.notice = Some(notice.to_string());
        self.sync_embed();
        cx.notify();
    }

    /// Close the window; while casting, ask first whether the receiver should stop too.
    fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.casting.is_some() {
            self.quit_prompt = true;
            cx.notify();
        } else {
            window.remove_window();
        }
    }

    /// The close dialog's answer: stop the receiver (briefly waiting for it) or leave it playing.
    fn quit_cast(&mut self, stop: bool, window: &mut Window) {
        if let (true, Some(c)) = (stop, &self.casting) {
            let _ = c.remote.stop_within(Duration::from_secs(2));
        }
        self.casting = None;
        window.remove_window();
    }

    fn render_quit_prompt(&self, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let name = self.casting.as_ref().filter(|_| self.quit_prompt)?.name.clone();
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(div().id("quit-backdrop").absolute().top_0().left_0().size_full().occlude().bg(gpui::black().opacity(0.5)))
                .child(
                    div().absolute().top_0().left_0().size_full().flex().items_center().justify_center().child(
                        div()
                            .id("quit-prompt")
                            .occlude()
                            .w(px(340.))
                            .p_4()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .rounded_md()
                            .bg(themed(HOVER))
                            .border_1()
                            .border_color(themed(BORDER))
                            .shadow_lg()
                            .child(div().text_sm().text_color(themed(TEXT)).child(format!("Still casting to {name}")))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .child(
                                        self.chip("quit-stop", format!("Stop {name} and quit"), true)
                                            .on_click_hinted(&self.hint_reg(), cx, |this, _, window, _| this.quit_cast(true, window)),
                                    )
                                    .child(
                                        self.chip("quit-keep", "Keep playing and quit", true)
                                            .on_click_hinted(&self.hint_reg(), cx, |this, _, window, _| this.quit_cast(false, window)),
                                    )
                                    .child(self.chip("quit-cancel", "Cancel", true).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                        this.quit_prompt = false;
                                        cx.notify();
                                    })),
                            ),
                    ),
                ),
        )
    }

    /// Stop the receiver and leave the cast view.
    fn stop_cast(&mut self, cx: &mut Context<Self>) {
        let Some(c) = &self.casting else { return };
        let remote = c.remote.clone();
        blocking::unblock(move || { remote.ctl(serde_json::json!({ "action": "stop" })) }).detach();
        self.end_cast("Stopped casting", cx);
    }

    /// Send a control to the receiver and show its effect at once; the next poll corrects it.
    fn cast_ctl(&mut self, body: serde_json::Value, cx: &mut Context<Self>) {
        let Some(c) = &self.casting else { return };
        let remote = c.remote.clone();
        let task = blocking::unblock(move || { remote.ctl(body) });
        cx.spawn(async move |this, cx| {
            if let Err(e) = task.await {
                this.update(cx, |this, cx| {
                    this.notice = Some(format!("Cast: {e}"));
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Play or pause: the receiver while casting, else the local player.
    fn pause_toggle(&mut self, cx: &mut Context<Self>) {
        if let Some(st) = self.casting.as_mut().and_then(|c| c.status.as_mut()) {
            st.paused = !st.paused;
            self.cast_ctl(serde_json::json!({ "action": "toggle" }), cx);
            cx.notify();
        } else if self.casting.is_none() {
            self.player.toggle_pause();
            if let Some(st) = self.state.as_mut() {
                st.paused = !st.paused;
            }
        }
    }

    /// Seek by `secs`: the receiver while casting, else the local player.
    fn seek_by(&mut self, secs: f64, cx: &mut Context<Self>) {
        match self.casting.as_ref().and_then(|c| c.status.as_ref()) {
            Some(st) => self.seek_to((st.position + secs).max(0.), cx),
            None if self.casting.is_none() => self.player.seek_relative(secs),
            None => {}
        }
    }

    /// Seek to `secs`: the receiver while casting, else the local player.
    fn seek_to(&mut self, secs: f64, cx: &mut Context<Self>) {
        if let Some(st) = self.casting.as_mut().and_then(|c| c.status.as_mut()) {
            let secs = if st.duration > 0. { secs.min(st.duration) } else { secs };
            st.position = secs;
            self.cast_ctl(serde_json::json!({ "action": "seek", "position": secs }), cx);
            cx.notify();
        } else if self.casting.is_none() {
            self.player.seek_absolute(secs);
        }
    }

    /// The subtitle languages to look for: the setting, or the system language ("en" if unknown).
    fn sub_langs(&self) -> String {
        let setting = self.settings.sub_lang.trim();
        if !setting.is_empty() {
            return setting.to_string();
        }
        std::env::var("LANG")
            .ok()
            .and_then(|l| l.split(['_', '.']).next().map(str::to_string))
            .filter(|l| (2..=3).contains(&l.len()) && l.chars().all(|c| c.is_ascii_lowercase()))
            .unwrap_or_else(|| "en".into())
    }

    /// The caption languages and kind for video `id`: a pick from the player menu, else
    /// Settings → Subtitles.
    fn video_sub_langs(&self, id: &str) -> (String, bool) {
        match &self.sub_pick {
            Some((v, code, auto)) if v == id => (code.clone(), *auto),
            _ => (self.sub_langs(), self.settings.sub_auto),
        }
    }

    /// Ask yt-dlp which caption tracks the current video has (once per video).
    fn load_caption_langs(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        if self.caption_langs.as_ref().is_some_and(|(v, r)| *v == id && !matches!(r, Some(Err(_)))) {
            return;
        }
        self.caption_langs = Some((id.clone(), None));
        let cfg = self.cfg.clone();
        let task = {
            let id = id.clone();
            blocking::unblock(move || { yt::caption_langs(&cfg, &id) })
        };
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                if let Some((_, slot)) = this.caption_langs.as_mut().filter(|(v, _)| *v == id) {
                    *slot = Some(res);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The player menu's subtitle list: Off, the video's own tracks, its generated one, and
    /// generated translations into the languages of Settings → Subtitles (YouTube offers ~100;
    /// those are the ones you read). Label, choice, ticked.
    fn sub_choices(&self) -> Vec<(String, SubChoice, bool)> {
        let Some(video) = &self.current else { return Vec::new() };
        let on = self.state.as_ref().is_some_and(|s| s.sub_on);
        let picked = self.sub_pick.as_ref().filter(|(v, _, _)| *v == video.id).map(|(_, c, a)| (c.clone(), *a));
        let mut out = vec![("Off".to_string(), SubChoice::Off, !on)];
        match self.caption_langs.as_ref().filter(|(v, _)| *v == video.id).map(|(_, r)| r) {
            Some(Some(Ok(langs))) => {
                let mut push = |label: String, code: &str, auto: bool| {
                    let ticked = on && picked.as_ref().is_some_and(|(c, a)| c == code && *a == auto);
                    out.push((label, SubChoice::Lang(code.to_string(), auto), ticked));
                };
                for l in langs.iter().filter(|l| !l.auto) {
                    push(l.name.clone(), &l.code, false);
                }
                for l in langs.iter().filter(|l| l.auto && l.code.ends_with("-orig")) {
                    push(format!("{} (auto-generated)", l.name.trim_end_matches(" (Original)")), &l.code, true);
                }
                let has_auto = langs.iter().any(|l| l.auto);
                for want in self.sub_langs().split(',').map(str::trim) {
                    let own = langs.iter().any(|l| !l.auto && l.code == want);
                    if let Some(l) = langs.iter().find(|l| has_auto && l.auto && l.code == want).filter(|_| !own) {
                        push(format!("{} (auto-translated)", l.name), &l.code, true);
                    }
                }
                if out.len() == 1 {
                    out.push(("No captions for this video".into(), SubChoice::Info, false));
                }
            }
            Some(Some(Err(e))) => out.push((e.clone(), SubChoice::Info, false)),
            _ => out.push(("Loading languages…".into(), SubChoice::Info, false)),
        }
        out
    }

    /// Show the captions picked in the player menu for the current video (fetched like the
    /// subtitles), or hide them.
    fn pick_sub(&mut self, choice: SubChoice, label: String, cx: &mut Context<Self>) {
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        match choice {
            SubChoice::Off => {
                self.player.show_subtitles(false, None);
                self.notice = Some("Subtitles off".into());
            }
            SubChoice::Lang(code, auto) => {
                self.sub_pick = Some((id.clone(), code, auto));
                self.subs_none = None;
                self.subs_pending = None;
                self.subs_wanted = Some(id);
                // The Transcript tab follows the pick.
                self.transcript_for = None;
                self.notice = Some(format!("Subtitles: {label}"));
            }
            SubChoice::Info => {}
        }
        cx.notify();
    }

    /// Download the captions of video `id` (yt-dlp, cached); they are added to mpv by `poll_subtitles`.
    fn fetch_subtitles(&mut self, id: String, cx: &mut Context<Self>) {
        let (cfg, (langs, auto)) = (self.cfg.clone(), self.video_sub_langs(&id));
        let dir = store::cache_dir().join("captions");
        // The first version cached the captions as YouTube writes them, in a "subs" folder.
        let _ = std::fs::remove_dir_all(store::cache_dir().join("subs"));
        self.subs_loading = Some(id.clone());
        self.subs_none = None;
        let task = {
            let id = id.clone();
            blocking::unblock(move || { yt::subtitles(&cfg, &id, &langs, auto, &dir) })
        };
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                if this.subs_loading.as_deref() != Some(id.as_str()) {
                    return;
                }
                this.subs_loading = None;
                let current = this.current.as_ref().is_some_and(|v| v.id == id);
                match res {
                    Ok(Some(file)) if current => this.subs_pending = Some((id, file)),
                    Ok(None) => this.subs_none = Some(id),
                    Err(e) if current => this.notice = Some(format!("Subtitles: {e}")),
                    _ => {}
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Start the caption download a new video asked for, and give downloaded captions to mpv
    /// once it has that video open.
    fn poll_subtitles(&mut self, state: Option<&player::State>, cx: &mut Context<Self>) {
        if let Some(id) = self.subs_wanted.take() {
            self.fetch_subtitles(id, cx);
        }
        let Some((id, file)) = self.subs_pending.clone() else { return };
        let Some(video) = self.current.as_ref().filter(|v| v.id == id) else {
            self.subs_pending = None;
            return;
        };
        if state.is_some_and(|s| s.playing && s.path == self.media_path(video)) {
            self.player.add_subtitle(&file);
            self.subs_pending = None;
        }
    }

    /// The CC button / V: subtitles on or off for the playing video. With none yet it turns the
    /// setting on and fetches them.
    fn toggle_subtitles(&mut self, cx: &mut Context<Self>) {
        let (tracks, on, track) = self.state.as_ref().map_or((0, false, None), |s| (s.sub_tracks, s.sub_on, s.sub_id));
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        if tracks > 0 {
            self.player.show_subtitles(!on, track);
        } else if self.subs_loading.is_some() || self.subs_pending.is_some() {
            self.notice = Some("The subtitles are still loading…".into());
        } else if !self.settings.subtitles {
            self.settings.subtitles = true;
            self.settings.save();
            self.notice = Some("Subtitles on".into());
            self.subs_wanted = Some(id);
        } else if self.subs_none.as_deref() == Some(id.as_str()) {
            self.notice = Some(format!("No \"{}\" subtitles for this video", self.sub_langs()));
        } else {
            self.subs_wanted = Some(id);
        }
        cx.notify();
    }

    /// Change the volume by `delta` percent (unmuting when raising it).
    fn change_volume(&mut self, delta: f32, cx: &mut Context<Self>) {
        if delta > 0. && self.state.as_ref().is_some_and(|s| s.muted) {
            self.player.toggle_mute();
        }
        self.set_volume(self.settings.volume + delta, cx);
    }

    fn set_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        let volume = volume.clamp(0., 100.).round();
        self.settings.volume = volume;
        self.settings.save();
        self.volume_set = Instant::now();
        self.player.set_volume(volume);
        if !self.settings.volume_control {
            self.notice = Some(format!("Volume {volume}%"));
        }
        cx.notify();
    }

    fn toggle_play(&mut self, cx: &mut Context<Self>) {
        if self.casting.is_some() {
            self.pause_toggle(cx);
        } else if self.state.is_some() {
            self.player.toggle_pause();
        } else if let Some(v) = self.current.clone() {
            self.play(v, None, cx);
        }
    }

    /// Keyboard shortcuts; see SHORTCUTS. Text fields stop the keys they use from reaching here.
    fn shortcut(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        if k.key == "escape" && self.welcome_visible() {
            self.finish_welcome(cx);
            cx.stop_propagation();
            return;
        }
        if self.fullscreen && k.key == "escape" {
            self.player.set_fullscreen(false);
            cx.stop_propagation();
            return;
        }
        if self.quit_prompt {
            // The dialog is modal: Escape cancels it, other keys do nothing.
            if k.key == "escape" {
                self.quit_prompt = false;
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }
        if self.playlist_menu.is_some() && k.key == "escape" {
            self.playlist_menu = None;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.video_menu.is_some() && k.key == "escape" {
            self.close_video_menu(cx);
            cx.stop_propagation();
            return;
        }
        if self.channel_menu.is_some() && k.key == "escape" {
            self.channel_menu = None;
            self.confirm_unsub = None;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if k.key == "tab" && !k.modifiers.control && !k.modifiers.alt && !k.modifiers.platform {
            self.kb_step(k.modifiers.shift, window);
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.kb_focus.is_some() {
            match k.key.as_str() {
                "enter" | "space" if !k.modifiers.control && !k.modifiers.alt && self.kb_activate(window, cx) => {
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                "escape" => {
                    self.kb_focus = None;
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                // Any other key goes back to being a shortcut (Space pauses again, and so on).
                _ => self.kb_focus = None,
            }
        }
        if self.settings.vim && self.vim_key(ev, window, cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if k.modifiers.control && k.key == "v" {
            // Ctrl+V outside a text field: play the YouTube link on the clipboard.
            match cx.read_from_clipboard().and_then(|c| c.text()).and_then(|t| yt::parse_link(&t)) {
                Some(link) => self.open_link(link, cx),
                None => self.notice = Some("No YouTube link on the clipboard".into()),
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if k.modifiers.control && k.key == "f" && self.focus_filter(window) {
            self.left_collapsed = false;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if k.modifiers.alt && !k.modifiers.control && !k.modifiers.platform && matches!(k.key.as_str(), "left" | "right") {
            self.nav_go(k.key == "right", cx);
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if k.modifiers.control || k.modifiers.alt || k.modifiers.platform {
            return;
        }
        let active = self.state.is_some() || self.casting.is_some();
        // "?" is Shift+/ on most layouts: check the typed character before the "/" key.
        if k.key_char.as_deref() == Some("?") || (self.show_keys && k.key == "escape") {
            self.show_keys = !self.show_keys;
            self.sync_embed();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        match k.key.as_str() {
            "space" | "k" => self.toggle_play(cx),
            "left" if active => self.seek_by(-5., cx),
            "right" if active => self.seek_by(5., cx),
            "j" if active => self.seek_by(-10., cx),
            "l" if active => self.seek_by(10., cx),
            "f" if active && self.casting.is_none() => self.player.set_fullscreen(!self.fullscreen),
            "m" => self.player.toggle_mute(),
            "v" => self.toggle_subtitles(cx),
            "t" => self.cast_to(None, cx),
            "up" | "=" => self.change_volume(5., cx),
            "down" | "-" => self.change_volume(-5., cx),
            "e" if k.modifiers.shift => self.toggle_player_full(cx),
            "e" => self.toggle_lower_full(window, cx),
            "b" if k.modifiers.shift => self.toggle_right_collapsed(cx),
            "b" => self.toggle_left_collapsed(cx),
            "[" => self.cycle_tabs(-1, cx),
            "]" => self.cycle_tabs(1, cx),
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" if !k.modifiers.shift => self.tab_number(k.key.parse().unwrap_or(0), cx),
            "c" if k.modifiers.shift => self.copy_link_at_time(cx),
            "c" => self.copy_link(cx),
            "o" => self.open_in_browser(cx),
            "w" => self.watch_later(cx),
            "n" => self.next_prev(true, cx),
            "p" => self.next_prev(false, cx),
            "/" => self.open_search(window, cx),
            "escape" if self.saving => self.close_save(cx),
            "escape" if self.searching => self.close_search(window, cx),
            "escape" if matches!(self.tab, Tab::Subscriptions | Tab::Playlists) => self.browser(self.tab).open = None,
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Vim mode keys; returns whether the key was used. See VIM_SHORTCUTS.
    fn vim_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let k = &ev.keystroke;
        if k.modifiers.alt || k.modifiers.platform {
            return false;
        }
        // The typed character when there is one ("G" for Shift+g), else the key's name.
        let token = match (&k.key_char, k.modifiers.control) {
            (_, true) => format!("C-{}", k.key),
            (Some(c), false) if c != " " => c.clone(),
            _ => k.key.clone(),
        };
        if self.hints.is_some() {
            self.hint_key(&token, window, cx);
            return true;
        }
        let pending_g = std::mem::take(&mut self.vim_g);
        let pending_y = std::mem::take(&mut self.vim_y);
        let active = self.state.is_some() || self.casting.is_some();
        let len = self.left_items().len();
        let page = 10;
        match token.as_str() {
            "?" => {
                self.show_keys = !self.show_keys;
                self.sync_embed();
            }
            "escape" if self.show_keys => {
                self.show_keys = false;
                self.sync_embed();
            }
            "j" | "down" => self.vim_move(1, len, window),
            "k" | "up" => self.vim_move(-1, len, window),
            "C-d" => self.vim_move(page, len, window),
            "C-u" => self.vim_move(-page, len, window),
            "C-f" => {
                self.focus_filter(window);
            }
            "g" if pending_g => self.vim_move(isize::MIN, len, window),
            "y" if pending_y => self.copy_link(cx),
            "t" if pending_y => self.copy_link_at_time(cx),
            "o" => self.open_in_browser(cx),
            "w" => self.watch_later(cx),
            "E" => self.toggle_player_full(cx),
            "e" => self.toggle_lower_full(window, cx),
            "B" => self.toggle_right_collapsed(cx),
            "b" => self.toggle_left_collapsed(cx),
            "[" => self.cycle_tabs(-1, cx),
            "]" => self.cycle_tabs(1, cx),
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => self.tab_number(token.parse().unwrap_or(0), cx),
            "y" => self.vim_y = true,
            "g" => self.vim_g = true,
            "G" => self.vim_move(isize::MAX, len, window),
            "enter" | "l" => self.vim_activate(cx),
            "h" | "backspace" | "escape" => {
                if self.saving {
                    self.close_save(cx);
                } else if self.searching {
                    self.close_search(window, cx);
                } else if matches!(self.tab, Tab::Subscriptions | Tab::Playlists) {
                    self.browser(self.tab).open = None;
                }
            }
            "H" => self.vim_tab(-1, cx),
            "L" => self.vim_tab(1, cx),
            "F" => {
                let window_size = window.viewport_size();
                // Only what's on screen (the list may have laid out rows below the fold).
                let targets: Vec<_> = self
                    .hint_targets
                    .borrow()
                    .iter()
                    .filter(|(b, _)| {
                        let c = b.center();
                        c.y > px(0.) && c.y < window_size.height && c.x > px(0.) && c.x < window_size.width
                    })
                    .cloned()
                    .collect();
                if !targets.is_empty() {
                    self.hints = Some((targets, String::new()));
                }
            }
            "x" => {
                if let Some(Item::Video(v, _)) = self.left_items().get(self.vim_cursor).cloned() {
                    self.enqueue(v, cx);
                }
            }
            "space" => self.toggle_play(cx),
            "left" if active => self.seek_by(-5., cx),
            "right" if active => self.seek_by(5., cx),
            "," if active => self.seek_by(-10., cx),
            "." if active => self.seek_by(10., cx),
            "m" => self.player.toggle_mute(),
            "v" => self.toggle_subtitles(cx),
            "t" => self.cast_to(None, cx),
            "+" | "=" => self.change_volume(5., cx),
            "-" => self.change_volume(-5., cx),
            "n" => self.next_prev(true, cx),
            "p" => self.next_prev(false, cx),
            "/" => self.open_search(window, cx),
            _ => return false,
        }
        self.prefetch_cursor(cx);
        true
    }

    /// A key in hint mode: narrow down by label, click the target once a label is complete.
    fn hint_key(&mut self, token: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some((targets, typed)) = &mut self.hints else { return };
        match token {
            "escape" => self.hints = None,
            "backspace" => {
                typed.pop();
            }
            t if t.len() == 1 && HINT_CHARS.contains(t) => {
                typed.push_str(t);
                let labels = hint_label_list(targets.len());
                let matching: Vec<usize> = (0..targets.len()).filter(|&i| labels[i].starts_with(typed.as_str())).collect();
                match matching.as_slice() {
                    [] => self.hints = None,
                    [i] if labels[*i] == *typed => {
                        // Run the element's own click handler.
                        let action = targets[*i].1.clone();
                        self.hints = None;
                        action(self, window, cx);
                    }
                    _ => {}
                }
            }
            _ => self.hints = None,
        }
    }

    /// Clickable things on screen in Tab order: the left column first, then the right one, each
    /// from top to bottom.
    fn kb_targets(&self, window: &Window) -> Vec<HintTarget> {
        let size = window.viewport_size();
        let divider_x = if self.left_collapsed {
            px(0.)
        } else if self.right_collapsed {
            size.width
        } else {
            size.width * self.settings.split
        };
        let mut targets: Vec<HintTarget> = self
            .hint_targets
            .borrow()
            .iter()
            .filter(|(b, _)| {
                let c = b.center();
                b.size.width > px(1.) && b.size.height > px(1.) && c.x > px(0.) && c.x < size.width && c.y > px(0.) && c.y < size.height
            })
            .cloned()
            .collect();
        targets.sort_by_key(|(b, _)| ((b.center().x >= divider_x) as u8, (f32::from(b.origin.y) / 12.).round() as i32, f32::from(b.origin.x) as i32));
        targets
    }

    /// The target the ring is on: the one closest to where it was last seen.
    fn kb_current(&self, targets: &[HintTarget]) -> Option<usize> {
        let at = self.kb_focus?;
        let gap = |b: &GBounds<GPixels>| {
            [b.origin.x - at.origin.x, b.origin.y - at.origin.y, b.size.width - at.size.width, b.size.height - at.size.height]
                .iter()
                .map(|d| f32::from(*d).abs())
                .sum::<f32>()
        };
        (0..targets.len()).min_by(|&i, &j| gap(&targets[i].0).total_cmp(&gap(&targets[j].0)))
    }

    /// Tab / Shift+Tab: move the focus ring to the next / previous clickable thing.
    fn kb_step(&mut self, back: bool, window: &mut Window) {
        if self.show_keys || self.saving || self.channel_menu.is_some() || self.video_menu.is_some() || self.playlist_menu.is_some() || self.hints.is_some() {
            return;
        }
        // Leave a text field, so typing doesn't go into it any more.
        window.focus(&self.root_focus);
        let targets = self.kb_targets(window);
        let n = targets.len();
        if n == 0 {
            return;
        }
        let next = match (self.kb_current(&targets), back) {
            (Some(i), false) => (i + 1) % n,
            (Some(i), true) => (i + n - 1) % n,
            (None, false) => 0,
            (None, true) => n - 1,
        };
        self.kb_focus = Some(targets[next].0);
    }

    /// Enter / Space on the focus ring: do what clicking that thing does.
    fn kb_activate(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let targets = self.kb_targets(window);
        let Some(i) = self.kb_current(&targets) else { return false };
        let action = targets[i].1.clone();
        action(self, window, cx);
        true
    }

    /// Where `on_click_hinted` registers click targets this frame.
    fn hint_reg(&self) -> HintReg {
        HintReg { targets: self.hint_targets.clone() }
    }

    /// Move the selection, scrolling only when it would leave the visible rows.
    fn vim_move(&mut self, by: isize, len: usize, window: &Window) {
        if len == 0 {
            return;
        }
        let cursor = (self.vim_cursor as isize).saturating_add(by).clamp(0, len as isize - 1) as usize;
        self.vim_cursor = cursor;
        // History shows Continue watching rows above its scrolling list.
        let (offset, row_h) = match self.tab {
            Tab::History => (self.history_items().0.len(), ROW_H),
            Tab::Subscriptions | Tab::Playlists if self.browser_ref(self.tab).open.is_none() && !(self.tab == Tab::Subscriptions && self.subs_live) => (0, 44.),
            _ => (0, ROW_H),
        };
        let Some(ix) = cursor.checked_sub(offset) else {
            // In Continue watching, which scrolls on its own.
            let (top, visible) = scroll_window(&self.continue_scroll, ROW_H, self.settings.continue_height);
            if cursor < top {
                self.continue_scroll.scroll_to_item(cursor, ScrollStrategy::Top);
            } else if cursor >= top + visible {
                self.continue_scroll.scroll_to_item(cursor, ScrollStrategy::Bottom);
            }
            return;
        };
        let (top, visible) = scroll_window(&self.vim_scroll, row_h, f32::from(window.viewport_size().height) - 160.);
        if ix < top {
            self.vim_scroll.scroll_to_item(ix, ScrollStrategy::Top);
        } else if ix >= top + visible {
            self.vim_scroll.scroll_to_item(ix, ScrollStrategy::Bottom);
        }
    }

    fn vim_activate(&mut self, cx: &mut Context<Self>) {
        match self.left_items().get(self.vim_cursor).cloned() {
            Some(Item::Group(g)) => self.open_group(self.tab, g, cx),
            Some(Item::Video(v, queue)) => self.play(v, Some(queue), cx),
            None => {}
        }
    }

    fn vim_tab(&mut self, step: isize, cx: &mut Context<Self>) {
        let tabs = self.tab_list();
        let i = tabs.iter().position(|t| *t == self.tab).unwrap_or(0) as isize;
        let next = (i + step).rem_euclid(tabs.len() as isize) as usize;
        self.select_tab(tabs[next], cx);
    }

    /// The lower pane's tabs as shown: Recommended (if on), Chapters (if the video has them), Up next.
    fn lower_tabs(&self) -> Vec<Lower> {
        let mut tabs = Vec::new();
        if self.settings.recommendations {
            tabs.push(Lower::Recommended);
        }
        if self.description_on() {
            tabs.push(Lower::Description);
        }
        if self.transcript_on() {
            tabs.push(Lower::Transcript);
        }
        if !self.chapter_list().is_empty() {
            tabs.push(Lower::Chapters);
        }
        if self.watch_later_on() {
            tabs.push(Lower::WatchLater);
        }
        if self.comments_on() {
            tabs.push(Lower::Comments);
        }
        tabs.push(Lower::UpNext);
        tabs
    }

    /// The lower pane tab shown now (see the lower pane in `render`).
    fn shown_lower(&self, tabs: &[Lower]) -> Lower {
        match self.lower {
            Lower::Chapters | Lower::Comments | Lower::WatchLater | Lower::Recommended | Lower::Description | Lower::Transcript if !tabs.contains(&self.lower) => tabs[0],
            tab => tab,
        }
    }

    /// `[` / `]`: go to the previous (-1) or next (1) tab, through the header tabs and then the
    /// lower pane's, wrapping around: the same seven tabs as the number keys.
    fn cycle_tabs(&mut self, step: isize, cx: &mut Context<Self>) {
        let header = self.tab_list();
        let lower = self.lower_tabs();
        let at = if self.cycle_lower {
            header.len() + lower.iter().position(|t| *t == self.shown_lower(&lower)).unwrap_or(0)
        } else {
            header.iter().position(|t| *t == self.tab).unwrap_or(0)
        };
        let next = (at as isize + step).rem_euclid((header.len() + lower.len()) as isize) as usize;
        if next < header.len() {
            self.select_tab(header[next], cx);
        } else {
            self.show_lower(lower[next - header.len()], cx);
        }
    }

    fn show_lower(&mut self, tab: Lower, cx: &mut Context<Self>) {
        self.lower = tab;
        if tab == Lower::WatchLater && !matches!(self.watch_later, Load::Idle) {
            // Pick up changes made elsewhere; the list shown stays until the new one arrives.
            self.fetch(cx, "watch-later", |s| &mut s.watch_later, Some("watch-later".into()), |cfg, on| yt::group_videos(cfg, ":ytwatchlater", on));
        }
        self.cycle_lower = true;
        self.player_full = false;
        cx.notify();
    }

    /// Jump to a tab by number, as shown: 1-4 are the header tabs, 5-9 the lower pane's.
    /// Nothing if there are fewer.
    fn tab_number(&mut self, n: usize, cx: &mut Context<Self>) {
        // The lower pane starts at 5, or after the header's tabs when Downloads makes them five.
        let header = self.tab_list();
        let first_lower = header.len().max(4) + 1;
        if n < first_lower {
            if let Some(tab) = header.get(n.wrapping_sub(1)).copied() {
                self.select_tab(tab, cx);
            }
        } else if let Some(tab) = self.lower_tabs().get(n - first_lower).copied() {
            self.show_lower(tab, cx);
        }
    }

    /// Header tabs in order, as shown.
    fn tab_list(&self) -> Vec<Tab> {
        let st = &self.settings;
        if !self.cfg.has_auth() {
            // No account tabs: the anonymous home (Subscriptions), Downloads and Settings.
            let mut tabs = vec![Tab::Subscriptions];
            tabs.extend(self.downloads_on().then_some(Tab::Downloads));
            tabs.push(Tab::Settings);
            return tabs;
        }
        let mut tabs: Vec<Tab> = [(st.subscriptions, Tab::Subscriptions), (st.playlists, Tab::Playlists), (st.history, Tab::History), (self.downloads_on(), Tab::Downloads)]
            .into_iter()
            .filter_map(|(on, t)| on.then_some(t))
            .collect();
        tabs.push(Tab::Settings);
        tabs
    }

    /// Identifies the list in the left column; the Vim selection resets when it changes.
    fn left_key(&self) -> String {
        match self.tab {
            Tab::Subscriptions | Tab::Playlists => {
                let b = self.browser_ref(self.tab);
                format!("{:?}-{:?}-{:?}", self.tab, b.open.as_ref().map(|g| &g.id), b.view)
            }
            Tab::History => format!("History-{}", self.history_shorts()),
            tab => format!("{tab:?}"),
        }
    }

    /// The left column's list in display order (same order the views render).
    fn left_items(&self) -> Vec<Item> {
        let videos = |list: Vec<Video>| -> Vec<Item> {
            let list: Arc<[Video]> = self.visible(&list).into();
            list.iter().map(|v| Item::Video(v.clone(), list.clone())).collect()
        };
        match self.tab {
            Tab::Settings => Vec::new(),
            Tab::Search => videos(self.wanted(self.search.items().to_vec())),
            Tab::Downloads => videos(self.library_videos()),
            Tab::History if self.cfg.has_auth() => {
                let (partial, all) = self.history_items();
                let mut items = videos(partial);
                items.extend(videos(all));
                items
            }
            tab => match &self.browser_ref(tab).open {
                Some(_) if tab == Tab::Playlists => videos(self.browser_videos(tab)),
                Some(_) => videos(self.wanted(self.browser_videos(tab))),
                // Logged out: the anonymous home list.
                None if !self.cfg.has_auth() => videos(self.wanted(self.anon.items().to_vec())),
                None if tab == Tab::Subscriptions && self.subs_live => videos(self.wanted(self.live_videos(true))),
                None => self.group_items(tab, &self.unseen_counts()).into_iter().map(Item::Group).collect(),
            },
        }
    }

    /// Settings → Hide videos with words, lower-cased.
    fn hide_words(&self) -> Vec<String> {
        self.settings.hide_words.split(',').map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).collect()
    }

    /// `videos` without the ones whose title says a hidden word.
    fn wanted(&self, videos: Vec<Video>) -> Vec<Video> {
        let words = self.hide_words();
        if words.is_empty() {
            return videos;
        }
        videos.into_iter().filter(|v| !says_any(&v.title, &words)).collect()
    }

    /// Videos as lists show them (Shorts hidden when turned off).
    fn visible(&self, videos: &[Video]) -> Vec<Video> {
        videos.iter().filter(|v| self.settings.shorts || !v.short).cloned().collect()
    }

    /// The History tab shows its Shorts list (only while Shorts are on in Settings).
    fn history_shorts(&self) -> bool {
        self.settings.shorts && self.history_view == ChannelView::Shorts
    }

    /// History tab: Continue watching (started, not finished), then local + YouTube history.
    fn history_items(&self) -> (Vec<Video>, Vec<Video>) {
        // YouTube's own order first (what was played here is in it, via mark-watched), then
        // what only this app knows; without YouTube's list, just the local one.
        let mut all: Vec<Video> = self.yt_history.items().to_vec();
        let seen: HashSet<String> = all.iter().map(|v| v.id.clone()).collect();
        all.extend(self.history.items.iter().filter(|w| !seen.contains(&w.video.id)).map(|w| w.video.clone()));
        let shorts = self.history_shorts();
        all.retain(|v| self.video_matches(v) && v.short == shorts);
        let partial = self
            .history
            .items
            .iter()
            .filter(|w| w.position > 30. && !w.finished && self.video_matches(&w.video) && w.video.short == shorts)
            .take(50)
            .map(|w| w.video.clone())
            .collect();
        (partial, all)
    }

    /// A browser tab's groups in display order. Subscriptions: New uploads first, then channels
    /// with new videos, then the rest (each part A–Z).
    fn group_items(&self, tab: Tab, counts: &HashMap<String, usize>) -> Vec<Group> {
        let mut g: Vec<Group> = self.browser_ref(tab).groups.items().to_vec();
        let filtering = !self.list_filter.trim().is_empty();
        g.retain(|c| self.filter_match(&c.title));
        if tab == Tab::Subscriptions {
            if let Some(group) = self.active_group() {
                g.retain(|c| group.channels.contains(&c.id));
            }
            g.sort_by_key(|g| !counts.contains_key(&g.id));
            if !filtering {
                g.insert(0, Group { id: FEED_ID.into(), title: "New uploads".into(), url: ":ytsubs".into(), thumb: None, subscribers: None });
            }
        }
        g
    }

    fn shortcuts(&self) -> &'static [(&'static str, &'static str)] {
        if self.settings.vim { &VIM_SHORTCUTS } else { &SHORTCUTS }
    }

    /// Whether row `i` of the left list is the Vim selection.
    fn vim_selected(&self, i: usize) -> bool {
        self.settings.vim && self.vim_cursor == i
    }

    /// The video `step` places away from the current one in the queue.
    /// Where the playing video is in `nav` (the entry we moved to, else its latest one).
    fn nav_here(&self) -> Option<usize> {
        let id = &self.current.as_ref()?.id;
        self.nav_pos.filter(|&p| self.nav.get(p).is_some_and(|v| &v.id == id)).or_else(|| self.nav.iter().rposition(|v| &v.id == id))
    }

    /// The `nav` entries back and forward from the playing video.
    fn nav_targets(&self) -> (Option<usize>, Option<usize>) {
        match self.nav_here() {
            Some(p) => (p.checked_sub(1), Some(p + 1).filter(|&n| n < self.nav.len())),
            // The playing video isn't in the list (a link, say): back goes to the last one played.
            None => (self.nav.len().checked_sub(1), None),
        }
    }

    /// Back (`forward` false) or forward through the videos played.
    fn nav_go(&mut self, forward: bool, cx: &mut Context<Self>) {
        let (back, next) = self.nav_targets();
        let Some(i) = (if forward { next } else { back }) else { return };
        let video = self.nav[i].clone();
        self.nav_moving = true;
        self.nav_pos = Some(i);
        self.play(video, None, cx);
        self.nav_moving = false;
    }

    fn neighbor(&self, step: isize) -> Option<Video> {
        let current = self.current.as_ref()?;
        let i = self.queue.iter().position(|v| v.id == current.id)?;
        self.queue.get(i.checked_add_signed(step)?).cloned()
    }

    fn tick(&mut self, state: Option<player::State>, window: &mut Window, cx: &mut Context<Self>) {
        self.ticks += 1;
        self.expire_notices(cx);
        if self.ticks % 15 == 0 {
            self.poll_cast(cx);
        }
        self.preload(cx);
        if self.ticks % 8 == 0 {
            self.prefetch_top(cx);
        }
        self.load_status(cx);
        self.load_votes(cx);
        self.poll_subtitles(state.as_ref(), cx);
        let url = self.current.as_ref().map(|v| self.media_path(v));
        let was_loading = self.loading;
        if let Some(s) = &state {
            if s.idle || (s.playing && Some(&s.path) == url.as_ref()) {
                self.loading = false;
            }
            // Volume keys over the video are handled by mpv itself: remember its volume.
            if let Some(v) = s.volume.filter(|v| (*v as f32 - self.settings.volume).abs() >= 1. && self.volume_set.elapsed() > Duration::from_secs(1)) {
                self.settings.volume = v as f32;
                self.settings.save();
                self.player.set_volume(self.settings.volume);
                cx.notify();
            }
        }
        if !self.player.alive() {
            self.loading = false;
        }
        self.watch_stall(state.as_ref(), url.as_deref(), cx);
        // Until mpv plays the new video, its state still describes the previous one.
        if state.is_some() {
            self.player.bind_mouse();
        }
        if self.pip && !self.player.alive() {
            // The PiP window was closed: next video plays in the app again.
            self.pip = false;
            cx.notify();
        }
        if let Some(action) = state.as_ref().map(|s| s.action.clone()).filter(|a| !a.is_empty()) {
            self.player.clear_action();
            match action.as_str() {
                "next" => self.play_next(cx),
                "prev" => {
                    if let Some(v) = self.neighbor(-1) {
                        self.play(v, None, cx);
                    }
                }
                "speed" => self.cycle_speed(cx),
                "pip" => self.toggle_pip(cx),
                "subtitles" => self.toggle_subtitles(cx),
                a if a.starts_with("menu") && !self.fullscreen => {
                    if let Some(at) = player::menu_position(a) {
                        self.open_player_menu(at, window, cx);
                    }
                }
                _ => {}
            }
        }
        if state.as_ref().is_some_and(|s| s.help) {
            self.player.clear_help();
            self.show_keys = !self.show_keys;
            cx.notify();
        }
        let state = state.filter(|s| !s.idle && !self.loading);
        if let (Some(s), Some(v), true) = (&state, &self.current, self.casting.is_none()) {
            // Finished videos restart from the beginning next time, and show as watched.
            let finished = s.duration > 0. && s.position > s.duration - 15.;
            let pos = if finished { 0. } else { s.position };
            self.history.set_position(&v.id, pos, finished || (self.history.is_finished(&v.id) && pos < 30.));
            if self.ticks % 20 == 0 {
                self.history.save();
            }
        }
        // Sleep timer: pause when the time is up (the receiver while casting), or end with this
        // video (marking it ended, so autoplay below leaves it there).
        match self.sleep {
            Some(Sleep::At(t)) if Instant::now() >= t => {
                self.sleep = None;
                let _ = self.run_command(cli::Command::Pause, window, cx);
                self.notice = Some("Sleep timer: paused".into());
                cx.notify();
            }
            Some(Sleep::EndOfVideo) => {
                if let (Some(s), Some(v)) = (&state, &self.current) {
                    if s.ended && self.ended.as_ref() != Some(&v.id) {
                        self.ended = Some(v.id.clone());
                        self.sleep = None;
                        self.notice = Some("Sleep timer: stopped after the video".into());
                        cx.notify();
                    }
                }
            }
            _ => {}
        }
        if let (true, Some(s), Some(v)) = (self.settings.autoplay, &state, &self.current) {
            if s.ended && self.ended.as_ref() != Some(&v.id) {
                self.ended = Some(v.id.clone());
                if let Some(next) = self.next_video() {
                    self.play(next, None, cx);
                    return;
                }
            }
        }
        let full = state.as_ref().is_some_and(|s| s.fullscreen);
        if full != self.fullscreen {
            self.fullscreen = full;
            window.toggle_fullscreen();
            cx.notify();
        }
        if was_loading && !self.loading {
            yt::timing("video start", self.load_started);
        }
        let changed = was_loading != self.loading
            || match (&state, &self.state) {
                (Some(a), Some(b)) => a.position as u64 != b.position as u64 || a.paused != b.paused,
                (a, b) => a.is_some() != b.is_some(),
            };
        self.state = state;
        self.sync_embed();
        if changed {
            cx.notify();
        }
    }

    /// Load the last-watched video paused, so Resume starts instantly. Only when it can play
    /// inside unbloatedtube; our X11 window may need a few ticks to show up in the WM's client list.
    fn preload(&mut self, cx: &mut Context<Self>) {
        if self.preloaded || self.player.alive() {
            self.preloaded = true;
            return;
        }
        let Some(video) = self.current.clone() else { return };
        if self.embed.is_none() {
            self.embed = Embed::new().map(|e| Rc::new(RefCell::new(e)));
        }
        if self.embed.is_some() {
            self.start(video, true);
            self.preloaded = true;
            cx.notify();
        } else if self.ticks > 20 {
            self.preloaded = true;
        }
    }

    /// Cached thumbnail, kicking off a download the first time a key is seen.
    fn thumb(&mut self, key: &str, url: Option<String>, cx: &mut Context<Self>) -> Thumb {
        let path = thumbs::path(key);
        if self.thumbs_ready.contains(key) || path.exists() {
            self.thumbs_ready.insert(key.to_string());
            return Thumb::Ready(path);
        }
        if url.is_none() || self.thumbs_failed.contains(key) {
            return Thumb::Missing;
        }
        if let Some(url) = url.filter(|_| self.thumbs_requested.insert(key.to_string())) {
            let dest = path.clone();
            let key = key.to_string();
            let task = blocking::unblock(move || { thumbs::download(&url, &dest) });
            cx.spawn(async move |this, cx| {
                let ok = task.await.is_ok();
                this.update(cx, |this, cx| {
                    if !ok {
                        this.thumbs_failed.insert(key);
                    }
                    cx.notify()
                })
                .ok();
            })
            .detach();
        }
        Thumb::Pending
    }

    /// A thumbnail box: the image, a pulsing skeleton while it downloads, or a plain box.
    fn thumb_el(&mut self, key: &str, url: Option<String>, w: f32, h: f32, radius: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let frame = div().w(px(w)).h(px(h)).flex_none().overflow_hidden().rounded(radius).bg(themed(HOVER));
        match self.thumb(key, url, cx) {
            // Round the image itself: the frame's overflow clip is rectangular, so a rounded
            // frame alone would leave square corners (e.g. on round avatars).
            Thumb::Ready(path) => frame.child(img(path).size_full().rounded(radius).object_fit(ObjectFit::Cover)).into_any_element(),
            Thumb::Pending => pulse(SharedString::from(format!("thumb-{key}")), frame),
            Thumb::Missing => frame.into_any_element(),
        }
    }

    /// `watched`: dim the row. `in_up_next`: the row is in the Up next list (✕ instead of +).
    fn video_row(
        &mut self,
        id: impl Into<ElementId>,
        video: Video,
        queue: Arc<[Video]>,
        watched: bool,
        in_up_next: bool,
        selected: bool,
        playlist: Option<Group>,
        in_history: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        let playing = self.current.as_ref().is_some_and(|c| c.id == video.id);
        let progress = match (self.history.position(&video.id), video.duration) {
            _ if self.history.is_finished(&video.id) => Some(1.),
            (p, Some(d)) if p > 0. && d > 0. => Some((p / d).min(1.) as f32),
            _ => None,
        };
        let action = {
            let v = video.clone();
            let (icon, tooltip) = if in_up_next { ("close", "Remove from Up next") } else { ("add", "Add to Up next") };
            div()
                .id("row-action")
                .flex_none()
                .p(px(6.))
                .rounded_md()
                .invisible()
                .group_hover("video-row", |s| s.visible())
                .hover(|d| d.bg(themed(BORDER)))
                .child(svg().path(icons::path(icon)).size(px(14.)).text_color(themed(TEXT)))
                .tooltip(tip(tooltip))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if in_up_next { this.dequeue(&v.id, cx) } else { this.enqueue(v.clone(), cx) }
                }))
        };
        // Everywhere but Watch later itself (and a request needs your login): add to Watch later.
        let later = (self.cfg.has_auth() && playlist.as_ref().is_none_or(|g| g.id != "WL")).then(|| {
            let v = video.clone();
            div()
                .id("row-later")
                .flex_none()
                .p(px(6.))
                .rounded_md()
                .invisible()
                .group_hover("video-row", |s| s.visible())
                .hover(|d| d.bg(themed(BORDER)))
                .child(svg().path(icons::path("watch-later")).size(px(14.)).text_color(themed(TEXT)))
                .tooltip(tip_left("Add to Watch later"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.add_to_watch_later(v.clone(), cx);
                }))
        });
        // In an open playlist: remove the video from it.
        let remove = playlist.map(|g| {
            let v = video.clone();
            div()
                .id("row-remove")
                .flex_none()
                .p(px(6.))
                .rounded_md()
                .invisible()
                .group_hover("video-row", |s| s.visible())
                .hover(|d| d.bg(themed(BORDER)))
                .child(svg().path(icons::path("trash")).size(px(14.)).text_color(themed(TEXT)))
                .tooltip(tip_left(if g.id == "LL" { "Unlike (remove from Liked videos)".to_string() } else { format!("Remove from {}", g.title) }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.remove_from_playlist(g.clone(), v.clone(), cx);
                }))
        });
        // In History (and Continue watching): forget this video.
        let forget = in_history.then(|| {
            let id = video.id.clone();
            div()
                .id("row-forget")
                .flex_none()
                .p(px(6.))
                .rounded_md()
                .invisible()
                .group_hover("video-row", |s| s.visible())
                .hover(|d| d.bg(themed(BORDER)))
                .child(svg().path(icons::path("trash")).size(px(14.)).text_color(themed(TEXT)))
                .tooltip(tip_left("Remove from history"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.forget_video(&id, cx);
                }))
        });
        let views = video.views.filter(|_| self.settings.show_views).map(|v| format!("{} {}", fmt_count(v), if video.live { "watching" } else { "views" }));
        let meta = [video.channel.clone(), views, video.duration.map(fmt_duration), video.watched.clone()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("  ·  ");
        let thumb = self.video_thumb(&video, 96., 54., px(4.), cx);
        let (title, original) = self.shown_title(&video, cx);
        let title_tip = match &original {
            Some(o) => format!("{title}\n\nOriginal title: {o}"),
            None => title.clone(),
        };
        let menu_video = video.clone();
        let menu_queue = queue.clone();
        let hover_id = video.id.clone();
        div()
            .id(id)
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| this.prefetch_hover(&hover_id, *hovered, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, ev: &gpui::MouseDownEvent, window, cx| {
                    this.open_video_menu(menu_video.clone(), Some(menu_queue.clone()), ev.position, window, cx)
                }),
            )
            .w_full()
            .overflow_hidden()
            .h(px(ROW_H))
            .px_3()
            .flex()
            .items_center()
            .gap_3()
            .cursor_pointer()
            .group("video-row")
            .when(playing, |d| d.bg(themed(HOVER)))
            .when(selected, |d| d.bg(themed(BORDER)))
            // Watched videos are dimmed, back to full on hover.
            .when(watched && !playing && !selected, |d| d.opacity(0.45))
            .hover(|d| d.bg(themed(HOVER)).opacity(1.))
            .child(
                div()
                    .relative()
                    .child(thumb)
                    .when_some(progress.filter(|_| !video.live), |d, p| {
                        d.child(div().absolute().bottom_0().left_0().h(px(3.)).w(px(96. * p)).bg(themed(ACCENT)))
                    })
                    .when(video.live, |d| d.child(div().absolute().bottom(px(3.)).right(px(3.)).child(live_badge(false)))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    // Titles are truncated; hovering shows the whole one (and YouTube's, for a
                    // DeArrow title).
                    .child(
                        div()
                            .id("title")
                            .text_sm()
                            .text_color(themed(TEXT))
                            .truncate()
                            .child(title)
                            .tooltip(tip(title_tip)),
                    )
                    .child(div().text_xs().text_color(themed(MUTED)).truncate().child(meta)),
            )
            .children(remove)
            .children(forget)
            .children(later)
            .child(action)
            .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.play(video.clone(), Some(queue.clone()), cx))
    }

    /// `left`: this is the left column's list, whose rows start at that index for Vim selection.
    fn video_list(&self, list_id: &'static str, videos: &[Video], left: Option<usize>, cx: &mut Context<Self>) -> AnyElement {
        // With Shorts turned off, they're hidden from every list; hidden words from all but
        // your own lists (what you put there stays).
        let own = matches!(list_id, "history" | "up-next" | "watch-later" | "downloads") || (list_id == "group" && self.tab == Tab::Playlists);
        let videos: Arc<[Video]> = if own { self.visible(videos) } else { self.visible(&self.wanted(videos.to_vec())) }.into();
        let in_up_next = list_id == "up-next";
        let in_history = list_id == "history";
        let playlist = if list_id == "group" && self.tab == Tab::Playlists {
            self.playlists.open.clone()
        } else if list_id == "watch-later" {
            Some(Self::watch_later_group())
        } else {
            None
        };
        // Watched: finished here, or in your YouTube history. Not dimmed in History itself.
        let watched: Arc<HashSet<String>> = Arc::new(if list_id == "history" {
            HashSet::new()
        } else {
            self.history
                .items
                .iter()
                .filter(|w| w.finished)
                .map(|w| w.video.id.clone())
                .chain(self.yt_history.items().iter().map(|v| v.id.clone()))
                .collect()
        });
        uniform_list(
            list_id,
            videos.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|i| {
                        let seen = watched.contains(&videos[i].id);
                        let selected = left.is_some_and(|o| this.vim_selected(o + i));
                        this.video_row(i, videos[i].clone(), videos.clone(), seen, in_up_next, selected, playlist.clone(), in_history, cx)
                    })
                    .collect()
            }),
        )
        .when(left.is_some(), |l| l.track_scroll(self.vim_scroll.clone()))
        .flex_1()
        .into_any_element()
    }

    fn status(&self, msg: impl Into<SharedString>) -> AnyElement {
        div().p_4().text_sm().text_color(themed(MUTED)).child(msg.into()).into_any_element()
    }

    /// The hint appended to any error while logged out, also shown on its own for a list
    /// that never loaded.
    fn needs_login(&self) -> &'static str {
        "This list needs your YouTube account.\nConnect it in Settings → Connect YouTube."
    }

    fn failed(&self, e: &str) -> AnyElement {
        if self.cfg.has_auth() && looks_like_login_error(e) {
            self.status(format!("{}\n\nYour YouTube login isn't working. Settings → Account lets you try again or connect another browser.", session_hint(e.to_string())))
        } else if self.cfg.has_auth() {
            self.status(e.to_string())
        } else {
            self.status(format!("{e}\n\n{}", self.needs_login()))
        }
    }

    /// What to show instead of a list that has no items (yet), or None if it has some.
    fn placeholder<T>(&self, load: &Load<T>, empty: &str, rows: Rows) -> Option<AnyElement> {
        match load {
            Load::Failed(e) => Some(self.failed(e)),
            // Never fetched (logged out): the connect hint instead of a skeleton.
            Load::Idle if !self.cfg.has_auth() => Some(self.status(self.needs_login().to_string())),
            Load::Ready(v) if v.is_empty() => Some(self.status(empty.to_string())),
            Load::Ready(_) => None,
            Load::Loading(v) if !v.is_empty() => None,
            _ => Some(skeleton(rows)),
        }
    }

    /// The anonymous video list: the left column's home and the Recommendations pane
    /// while logged out.
    fn render_anon(&mut self, id: &'static str, cx: &mut Context<Self>) -> AnyElement {
        match &self.anon {
            Load::Failed(e) => return self.status(e.clone()),
            Load::Ready(v) if v.is_empty() => return self.status("No videos.".to_string()),
            // A refresh keeps the previous mix on screen until the new one arrives.
            Load::Ready(v) | Load::Loading(v) if !v.is_empty() => {}
            _ => return skeleton(Rows::Videos),
        }
        let videos = self.anon.items().to_vec();
        self.video_list(id, &videos, Some(0), cx)
    }

    fn render_list(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.tab {
            Tab::History if self.cfg.has_auth() => {
                let body = self.render_history(cx);
                return div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .when(self.settings.shorts, |d| {
                        let shorts = self.history_shorts();
                        d.child(
                            div()
                                .flex()
                                .px_2()
                                .border_b_1()
                                .border_color(themed(BORDER))
                                .child(tab_button("Videos", !shorts).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                    this.history_view = ChannelView::Videos;
                                    cx.notify();
                                }))
                                .child(tab_button("Shorts", shorts).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                    this.history_view = ChannelView::Shorts;
                                    cx.notify();
                                })),
                        )
                    })
                    .child(self.filter_bar("Filter history", window, cx))
                    .child(div().flex().flex_col().flex_1().min_h_0().child(body))
                    .into_any_element();
            }
            Tab::Settings => return self.render_settings(window, cx),
            Tab::Downloads => return self.render_downloads(window, cx),
            Tab::Search if self.query.trim().is_empty() || matches!(self.search, Load::Idle) => {
                return self.render_recent_searches(cx);
            }
            Tab::Search => {
                return match self.placeholder(&self.search, "No results.", Rows::Videos) {
                    Some(p) => p,
                    None => self.video_list("search", &self.search.items().to_vec(), Some(0), cx),
                };
            }
            // Logged out: an open channel still shows through render_browser; anything
            // else shares the anonymous home.
            tab if !self.cfg.has_auth() => match tab {
                Tab::Subscriptions | Tab::Playlists if self.browser_ref(tab).open.is_some() => {
                    self.render_browser(tab, window, cx)
                }
                _ => self.render_anon("home", cx),
            },
            tab => self.render_browser(tab, window, cx),
        }
    }

    fn render_history(&mut self, cx: &mut Context<Self>) -> AnyElement {
        // YouTube's history first, then what only this app has seen.
        let (partial, videos) = self.history_items();
        if videos.is_empty() && !self.list_filter.trim().is_empty() {
            return self.status("No matches.");
        }
        if videos.is_empty() {
            if let Some(p) = self.placeholder(&self.yt_history, "Nothing here.", Rows::Videos) {
                return p;
            }
            if self.history_shorts() {
                return self.status("No Shorts.");
            }
        }
        if partial.is_empty() {
            return self.video_list("history", &videos, Some(0), cx);
        }
        let label = |text: &'static str| div().px_3().pt_3().pb_1().text_xs().text_color(themed(MUTED)).child(text);
        let queue: Arc<[Video]> = partial.into();
        let offset = queue.len();
        let rows = uniform_list(
            "continue",
            queue.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|i| {
                        let selected = this.vim_selected(i);
                        this.video_row(("continue", i), queue[i].clone(), queue.clone(), false, false, selected, None, true, cx)
                    })
                    .collect()
            }),
        )
        .track_scroll(self.continue_scroll.clone())
        // Never taller than its rows.
        .h(px(self.settings.continue_height.min(offset as f32 * ROW_H)));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(label("CONTINUE WATCHING"))
            .child(rows)
            .child(div().pt_2().child(divider("split-continue", Split::Continue, cx)))
            .child(label("HISTORY"))
            .child(div().flex().flex_col().flex_1().min_h_0().child(self.video_list("history", &videos, Some(offset), cx)))
            .into_any_element()
    }

    fn toggle(&mut self, field: fn(&mut Settings) -> &mut bool, cx: &mut Context<Self>) {
        let on = field(&mut self.settings);
        *on = !*on;
        self.settings.save();
        LIGHT.store(self.settings.light_theme, std::sync::atomic::Ordering::Relaxed);
        if self.settings.recommendations && matches!(self.recs, Load::Idle) {
            self.load_recs(cx);
        }
        if self.tab == Tab::Downloads && !self.downloads_on() {
            self.select_tab(self.tab_list()[0], cx);
        }
        self.apply_player_settings(cx);
    }

    /// Apply player settings to the running video now: speed directly; anything else that
    /// changes mpv's options reloads the current video at the same position.
    fn apply_player_settings(&mut self, cx: &mut Context<Self>) {
        if let Err(e) = apply_network(&self.settings) {
            self.notice = Some(e);
        }
        self.player.set_speed(self.settings.speed);
        let options = player::options(&self.cfg, &self.settings, self.pip);
        let changed = !self.player.options().is_empty() && self.player.options() != options.as_slice();
        if let (true, Some(video)) = (changed, self.current.clone()) {
            let paused = self.state.as_ref().is_none_or(|s| s.paused);
            self.start(video, paused);
        }
        self.sync_embed();
        cx.notify();
    }

    /// Which SponsorBlock categories to skip (multi-select).
    fn segment_chips(&self, cx: &mut Context<Self>) -> gpui::Div {
        div()
            .px_4()
            .pb_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_xs().text_color(themed(MUTED)).child("Skip these segments:"))
            .child(div().flex().flex_wrap().gap_1().children(SEGMENTS.iter().enumerate().map(|(i, &(label, name))| {
                let on = self.settings.skip_segments.iter().any(|s| s == name);
                div()
                    .id(("segment", i))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_xs()
                    .bg(if on { themed(ACCENT) } else { themed(HOVER) })
                    .text_color(if on { themed(ON_ACCENT) } else { themed(TEXT) })
                    .cursor_pointer()
                    .hover(|d| d.opacity(0.85))
                    .child(label)
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                        let list = &mut this.settings.skip_segments;
                        match list.iter().position(|s| s == name) {
                            Some(i) => {
                                list.remove(i);
                            }
                            None => list.push(name.to_string()),
                        }
                        this.settings.save();
                        this.apply_player_settings(cx);
                    })
            })))
    }

    /// A label with a row of mutually exclusive options; `set` applies the picked index.
    fn choice_row(
        &self,
        label: &'static str,
        options: Vec<(String, bool)>,
        set: fn(&mut Settings, usize),
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        div()
            .px_4()
            .py_3()
            .flex()
            .items_center()
            .child(div().flex_1().text_sm().text_color(themed(TEXT)).child(label))
            .children(options.into_iter().enumerate().map(|(i, (text, on))| {
                div()
                    .id((label, i))
                    .ml_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_xs()
                    .bg(if on { themed(ACCENT) } else { themed(HOVER) })
                    .text_color(if on { themed(ON_ACCENT) } else { themed(TEXT) })
                    .cursor_pointer()
                    .hover(|d| d.opacity(0.85))
                    .child(text)
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                        set(&mut this.settings, i);
                        this.settings.save();
                        this.apply_player_settings(cx);
                    })
            }))
    }

    /// Settings → Network: the connection, then the bypass method or the proxy's address, presets
    /// and a test.
    fn network_rows(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let matches = |label: &str, hint: &str| query.is_empty() || format!("{label} {hint}").to_lowercase().contains(query);
        let mut rows = Vec::new();
        if !matches("Connection", "network proxy bypass blocked slowdown russia tor byedpi vpn") {
            return rows;
        }
        let conn = self.settings.connection.clone();
        rows.push(
            self.choice_row("Connection", CONNECTIONS.iter().map(|(l, v)| (l.to_string(), *v == conn)).collect(), |s, i| s.connection = CONNECTIONS[i].1.into(), cx)
                .into_any_element(),
        );
        let hint = |text: &'static str| div().px_4().pb_2().text_xs().text_color(themed(MUTED)).child(text).into_any_element();
        match conn.as_str() {
            "bypass" => {
                rows.push(hint(
                    "For where YouTube is slowed down or filtered (Russia, for one): the first packet of each connection goes out split, so the filter doesn't see it's YouTube. Nothing to install; no other server involved. Doesn't help where YouTube's addresses are blocked outright: use a proxy then. If videos stay slow, try another method.",
                ));
                let method = self.settings.bypass_method.clone();
                rows.push(
                    self.choice_row("Method", BYPASS_METHODS.iter().map(|(l, v)| (l.to_string(), *v == method)).collect(), |s, i| s.bypass_method = BYPASS_METHODS[i].1.into(), cx)
                        .into_any_element(),
                );
            }
            "proxy" => {
                rows.push(hint(
                    "Everything goes through a proxy running on your computer or elsewhere: Tor, ByeDPI, a V2Ray/Xray/sing-box or VPN client's local port, a server of your own. The program itself has to be running.",
                ));
                let current = self.settings.proxy.trim().to_string();
                rows.push(
                    self.choice_row("Preset", PROXY_PRESETS.iter().map(|(l, v)| (l.to_string(), *v == current)).collect(), |s, i| s.proxy = PROXY_PRESETS[i].1.into(), cx)
                        .into_any_element(),
                );
                let field = TEXT_FIELDS.iter().position(|(l, _, _)| *l == "Proxy address").unwrap();
                rows.push(self.text_field(field, window, cx).into_any_element());
            }
            _ => {}
        }
        if conn != "direct" {
            let result = self.net_test.clone();
            rows.push(
                div()
                    .px_4()
                    .pb_3()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.chip("net-test", "Test connection", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.test_connection(cx)))
                    .when_some(result, |d, r| d.child(div().text_xs().text_color(themed(MUTED)).child(r)))
                    .into_any_element(),
            );
        }
        rows
    }

    /// Settings → Network → Test connection: one request to YouTube the way the app sends them.
    fn test_connection(&mut self, cx: &mut Context<Self>) {
        self.net_test = Some("Testing…".into());
        let task = blocking::unblock(|| {
            let start = Instant::now();
            let res = http::agent().get("https://www.youtube.com/generate_204").call();
            (res.map(drop).map_err(|e| e.to_string()), start.elapsed())
        });
        cx.spawn(async move |this, cx| {
            let (res, took) = task.await;
            this.update(cx, |this, cx| {
                this.net_test = Some(match res {
                    Ok(()) => format!("YouTube answers: {} ms", took.as_millis()),
                    Err(e) => format!("Doesn't work: {e}"),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// One of TEXT_FIELDS: click to focus, type, Enter/Esc to finish. Saved on every change.
    fn text_field(&self, i: usize, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        let (label, hint, field) = TEXT_FIELDS[i];
        let focus = self.field_focus[i].clone();
        let focused = focus.is_focused(window);
        let value = field(&mut self.settings.clone()).clone();
        div()
            .px_4()
            .py_3()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_sm().text_color(themed(TEXT)).child(label))
            .child(
                div()
                    .id(("field", i))
                    .track_focus(&focus)
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(themed(HOVER))
                    .border_1()
                    .border_color(if focused { themed(MUTED) } else { themed(BORDER) })
                    .text_sm()
                    .cursor_text()
                    .map(|d| match (value.is_empty(), focused) {
                        (true, false) => d.text_color(themed(MUTED)).child(hint),
                        (_, true) => d.text_color(themed(TEXT)).child(self.caret_text(label, &value, hint)),
                        _ => d.text_color(themed(TEXT)).child(value.clone()),
                    })
                    .on_click_hinted(&self.hint_reg(), cx, move |_, _, window, cx| {
                        window.focus(&focus);
                        cx.notify();
                    })
                    .on_key_down(cx.listener(move |this, ev: &KeyDownEvent, window, cx| {
                        match edit_text(&mut this.caret, label, field(&mut this.settings), ev, cx) {
                            Edit::Submit | Edit::Cancel => {
                                window.blur();
                                // Apply once editing is done, not on every keystroke.
                                this.apply_player_settings(cx);
                            }
                            Edit::Changed => this.settings.save(),
                            Edit::Moved => {}
                            Edit::Ignored => {
                cx.stop_propagation();
                return;
            }
                        }
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .when(label == "Cast command", |d| {
                d.child(div().text_xs().text_color(themed(MUTED)).child(
                    "Casts the video with this command (T or the Cast button) and replaces the [cast] targets in config.toml; empty uses those. \
                     {url} is the video's link; {start} (seconds), {id} and {title} work too. Quote arguments that contain spaces. \
                     It is not run through a shell, and it only sends the video: control of the receiver needs a target in config.toml.",
                ))
            })
    }

    /// A button in the Connect panel: accent when `primary`, neutral otherwise.
    fn cta(&self, id: impl Into<ElementId>, label: impl Into<SharedString>, primary: bool) -> Stateful<gpui::Div> {
        div()
            .id(id)
            .flex_none()
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(if primary { themed(ACCENT) } else { themed(BORDER) })
            .bg(if primary { themed(ACCENT) } else { themed(HOVER) })
            .text_sm()
            .text_color(if primary { themed(ON_ACCENT) } else { themed(TEXT) })
            .cursor_pointer()
            .hover(|d| d.opacity(0.85))
            .child(label.into())
    }

    /// The ACCOUNT section: the Connect YouTube panel (browser, cookies.txt, config.toml),
    /// the three-step check, and cookies.txt import under Advanced.
    fn connect_section(&mut self, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        let from_toml = self.cfg.cookies_file.is_some() || self.cfg.cookies_from_browser.is_some();
        let source = self.cfg.auth.source_label().unwrap_or_default();
        let config_toml = store::config_dir().join("config.toml");
        let connect = self.connect.clone();
        let detected: Vec<&str> = BROWSERS.iter().filter(|(_, spec, _)| Auth::on_path(spec)).map(|(name, ..)| *name).collect();
        let dot_line = |glyph: &'static str, text: String, color: u32| {
            div()
                .px_4()
                .pb_2()
                .flex()
                .items_center()
                .gap_2()
                .text_sm()
                .child(div().text_color(themed(ACCENT)).child(glyph))
                .child(div().text_color(themed(color)).child(text))
        };
        let mut d = div().flex().flex_col().child(div().px_4().pt_4().pb_2().text_xs().text_color(themed(MUTED)).child("ACCOUNT"));
        match &connect {
            Connect::Idle if !self.cfg.has_auth() => {
                d = d
                    .child(div().px_4().text_sm().text_color(themed(TEXT)).child("Connect YouTube"))
                    .child(div().px_4().pt_1().text_xs().text_color(themed(MUTED)).child(
                        "See your subscriptions, playlists and history. No password is ever asked: the app uses your browser's own YouTube session.",
                    ))
                    .child(
                        div().px_4().py_3().flex().flex_wrap().gap_1().children(BROWSERS.iter().enumerate().map(|(i, (name, spec, _))| {
                            self.chip(("browser", i), *name, self.connect_browser == *spec)
                                .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                                    this.connect_browser = (*spec).to_string();
                                    cx.notify();
                                })
                        })),
                    )
                    .child(div().px_4().text_xs().text_color(themed(MUTED)).child(if detected.is_empty() {
                        "No supported browser found in PATH — importing a cookies.txt below still works.".to_string()
                    } else {
                        format!("Detected: {}", detected.join(", "))
                    }))
                    .child(div().px_4().pt_3().child(self.cta("connect-go", "Connect", true).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                        this.start_connect(cx);
                    })))
                    .child(div().px_4().pt_4().pb_1().text_xs().text_color(themed(MUTED)).child("ADVANCED"));
                // Path field for a Netscape cookies.txt (own field, not a Settings value).
                let focused = self.import_focus.is_focused(window);
                d = d.child(
                    div()
                        .px_4()
                        .py_2()
                        .flex()
                        .gap_2()
                        .child(
                            div()
                                .id("import-field")
                                .track_focus(&self.import_focus)
                                .flex_1()
                                .min_w_0()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(themed(HOVER))
                                .border_1()
                                .border_color(if focused { themed(MUTED) } else { themed(BORDER) })
                                .text_sm()
                                .cursor_text()
                                .map(|d| match (self.import_path.is_empty(), focused) {
                                    (true, false) => d.text_color(themed(MUTED)).child("~/cookies.txt"),
                                    (_, true) => d.text_color(themed(TEXT)).child(self.caret_text("import", &self.import_path, "~/cookies.txt")),
                                    _ => d.text_color(themed(TEXT)).child(self.import_path.clone()),
                                })
                                .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                                    window.focus(&this.import_focus);
                                    cx.notify();
                                })
                                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                                    match edit_text(&mut this.caret, "import", &mut this.import_path, ev, cx) {
                                        Edit::Submit => {
                                            this.import_cookies(cx);
                                            window.blur();
                                        }
                                        Edit::Cancel => window.blur(),
                                        Edit::Changed | Edit::Moved => {}
                                        Edit::Ignored => {
                                            cx.stop_propagation();
                                            return;
                                        }
                                    }
                                    cx.stop_propagation();
                                    cx.notify();
                                })),
                        )
                        .child(self.cta("import-go", "Import", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                            this.import_cookies(cx);
                        })),
                );
                d = d.child(div().px_4().pt_1().text_xs().text_color(themed(MUTED)).child(
                    "A Netscape cookies.txt exported from a browser. It contains your YouTube session, so keep it private.",
                ));
            }
            Connect::Idle => {
                d = d.child(dot_line("●", format!("Connected via {source}"), TEXT));
                d = d.child(
                    div().px_4().py_2().flex().gap_2()
                        .child(self.cta("connect-test", "Test connection", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                            this.connect = Connect::Running(1);
                            this.run_probe(false, cx);
                        }))
                        .child(self.cta("connect-drop", "Log out", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                            this.logout(cx);
                        })),
                );
            }
            Connect::Running(_) => {
                d = d
                    .child(dot_line("●", "Checking your connection…".into(), TEXT))
                    .children(connect.steps())
                    .child(div().px_4().pt_2().text_xs().text_color(themed(MUTED)).child("This can take up to half a minute."));
            }
            Connect::Failed { error, .. } => {
                d = d
                    .child(dot_line("✕", "Connection problem".into(), ACCENT))
                    .children(connect.steps())
                    .child(div().px_4().pt_2().text_sm().text_color(themed(MUTED)).child(error.clone()))
                    .child(
                        div().px_4().py_2().flex().gap_2()
                            .child(self.cta("connect-retry", "Try again", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                // A failed first connect dropped its login again: save it anew.
                                if this.cfg.has_auth() {
                                    this.connect = Connect::Running(1);
                                    this.run_probe(false, cx);
                                } else {
                                    this.start_connect(cx);
                                }
                            }))
                            .when(!from_toml, |d| {
                                d.child(self.cta("connect-other", "Choose another browser", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                    this.choose_another_browser(cx);
                                }))
                            })
                            .when(from_toml, |d| {
                                d.child(self.cta("connect-drop", "Log out", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                    this.logout(cx);
                                }))
                            }),
                    );
            }
            Connect::Done { me } => {
                d = d.child(dot_line("●", format!("Connected via {source}"), TEXT));
                let avatar = me.avatar.clone().map(|url| self.thumb_el("connect-avatar", Some(url), 20., 20., px(10.), cx));
                let word = match &me.handle {
                    Some(handle) => format!("Signed in as {handle}"),
                    None => "Everything checks out.".to_string(),
                };
                d = d.child(div().px_4().pb_1().flex().items_center().gap_2().text_sm().text_color(themed(MUTED)).children(avatar).child(word));
                d = d.children(connect.steps());
                d = d.child(
                    div().px_4().py_2().flex().gap_2()
                        .child(self.cta("connect-test", "Test connection", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                            this.connect = Connect::Running(1);
                            this.run_probe(false, cx);
                        }))
                        .child(self.cta("connect-drop", "Log out", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                            this.logout(cx);
                        })),
                );
            }
        }
        if from_toml {
            d = d.child(div().px_4().pb_2().text_xs().text_color(themed(MUTED)).child(format!("Edit {} to change", config_toml.display())));
        }
        if let Some((ok, msg)) = &self.import_msg {
            d = d.child(
                div()
                    .px_4()
                    .pb_2()
                    .text_xs()
                    .text_color(if *ok { themed(MUTED) } else { themed(ACCENT) })
                    .child(format!("{} {msg}", if *ok { "✓" } else { "✕" })),
            );
        }
        d
    }

    /// The first-run screen: offer to connect, or continue without an account (Esc does the same).
    fn render_welcome(&self, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        div()
            .id("welcome")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(themed(BG))
            .child(
                div()
                    .w(px(560.))
                    .max_w(relative(0.9))
                    .p_8()
                    .rounded_xl()
                    .bg(themed(PANEL))
                    .border_1()
                    .border_color(themed(BORDER))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_2xl().text_color(themed(TEXT)).child("UnbloatedTube"))
                    .child(div().text_sm().text_color(themed(MUTED)).child(
                        "Connect your YouTube account to see your subscriptions, playlists and history. The app never asks for a password: it uses your browser's own YouTube session.",
                    ))
                    .child(
                        div().pt_2().flex().gap_2().child(self.cta("welcome-connect", "Connect YouTube", true).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                            this.welcome_connect(cx);
                        })).child(
                            self.cta("welcome-skip", "Continue without account", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                this.finish_welcome(cx);
                            }),
                        ),
                    )
                    .child(div().pt_1().text_xs().text_color(themed(MUTED)).child("Esc continues without an account.")),
            )
    }

    fn render_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let query = self.settings_filter.to_lowercase();
        let matches = |label: &str, hint: &str| {
            query.is_empty() || label.to_lowercase().contains(&query) || hint.to_lowercase().contains(&query)
        };
        let toggle_row = |i: usize, &(label, hint, field): &Toggle, cx: &mut Context<Self>| {
            let on = *field(&mut self.settings.clone());
            let switch = div()
                .w(px(34.))
                .h(px(18.))
                .p(px(2.))
                .rounded_full()
                .flex()
                .when(on, |d| d.justify_end())
                .bg(if on { themed(ACCENT) } else { themed(BORDER) })
                .child(div().size(px(14.)).rounded_full().bg(themed(ON_ACCENT)));
            div()
                .id(("toggle", i))
                .w_full()
                .px_4()
                .py_3()
                .flex()
                .items_center()
                .cursor_pointer()
                .hover(|d| d.bg(themed(HOVER)))
                .child(
                    // min_w_0 lets a long hint wrap instead of pushing the switch out of view.
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_sm().text_color(themed(TEXT)).child(label))
                        .child(div().text_xs().text_color(themed(MUTED)).child(hint)),
                )
                .child(switch.flex_none().ml_3())
                .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.toggle(field, cx))
                .into_any_element()
        };
        // Each section: header and its matching rows (hidden when none match).
        let mut sections: Vec<(&str, Vec<AnyElement>)> = Vec::new();
        let mut offset = 0;
        for (title, toggles) in [
            ("SHOW", &TOGGLES[..]),
            ("PLAYER BUTTONS", &BUTTON_TOGGLES[..]),
            ("PLAYER", &PLAYER_TOGGLES[..]),
            ("SUBTITLES", &SUB_TOGGLES[..]),
            ("NOTIFICATIONS", &NOTIFY_TOGGLES[..]),
            ("VIDEO INFO", &INFO_TOGGLES[..]),
            ("DEARROW", &DEARROW_TOGGLES[..]),
        ] {
            let rows = toggles
                .iter()
                .enumerate()
                .filter(|(_, t)| matches(t.0, t.1) && (self.cfg.has_auth() || !ACCOUNT_ONLY.contains(&t.0)))
                .map(|(i, t)| toggle_row(offset + i, t, cx))
                .collect();
            offset += toggles.len();
            sections.push((title, rows));
        }
        let player = &mut sections[2].1;
        if self.settings.sponsorblock && (matches("SponsorBlock segments", "sponsor ads skip")) {
            player.push(self.segment_chips(cx).into_any_element());
        }
        if matches("Max quality", "resolution 1080p") {
            player.push(
                self.choice_row(
                    "Max quality",
                    QUALITIES.iter().map(|q| (format!("{q}p"), *q == self.settings.max_quality)).collect(),
                    |s, i| s.max_quality = QUALITIES[i],
                    cx,
                )
                .into_any_element(),
            );
        }
        if matches("Default speed", "playback speed") {
            player.push(
                self.choice_row(
                    "Default speed",
                    SPEEDS.iter().map(|v| (format!("{v}×"), *v == self.settings.speed)).collect(),
                    |s, i| s.speed = SPEEDS[i],
                    cx,
                )
                .into_any_element(),
            );
        }
        let mut sub_rows: Vec<AnyElement> = Vec::new();
        for (i, (label, hint, _)) in TEXT_FIELDS.iter().enumerate() {
            // The proxy's address goes with the rest of Network.
            if matches(label, hint) && *label != "Proxy address" {
                let row = self.text_field(i, window, cx).into_any_element();
                if *label == "Subtitle language" { sub_rows.push(row) } else { player.push(row) }
            }
        }
        if matches("Subtitle size", "subtitles captions text size") {
            sub_rows.push(
                self.choice_row(
                    "Subtitle size",
                    SUB_SCALES.iter().map(|(name, v)| (name.to_string(), *v == self.settings.sub_scale)).collect(),
                    |s, i| s.sub_scale = SUB_SCALES[i].1,
                    cx,
                )
                .into_any_element(),
            );
        }
        sections[3].1.extend(sub_rows);
        if self.settings.notifications && matches("Check every", "notifications minutes interval") {
            let row = self.choice_row(
                "Check every",
                NOTIFY_MINUTES.iter().map(|m| (format!("{m} min"), *m == self.settings.notify_minutes)).collect(),
                |s, i| s.notify_minutes = NOTIFY_MINUTES[i],
                cx,
            );
            sections[4].1.push(row.into_any_element());
        }
        let net = self.network_rows(&query, window, cx);
        sections.push(("NETWORK", net));
        let show_account = matches("Connect YouTube", "account login logged in sign in browser connect cookies import");
        let nothing = sections.iter().all(|(_, rows)| rows.is_empty())
            && !show_account
            && !self.shortcuts().iter().any(|(k, what)| matches(k, what))
            && !matches("Keyboard shortcuts", "keys");
        let focused = self.settings_focus.is_focused(window);
        let search = div()
            .id("settings-search")
            .track_focus(&self.settings_focus)
            .mx_4()
            .my_2()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(themed(HOVER))
            .border_1()
            .border_color(if focused { themed(MUTED) } else { themed(BORDER) })
            .text_sm()
            .cursor_text()
            .map(|d| match (self.settings_filter.is_empty(), focused) {
                (true, false) => d.text_color(themed(MUTED)).child("Search settings"),
                (_, true) => d.text_color(themed(TEXT)).child(self.caret_text("settings", &self.settings_filter, "Search settings")),
                _ => d.text_color(themed(TEXT)).child(self.settings_filter.clone()),
            })
            .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                window.focus(&this.settings_focus);
                cx.notify();
            })
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match edit_text(&mut this.caret, "settings", &mut this.settings_filter, ev, cx) {
                    Edit::Submit => window.blur(),
                    Edit::Cancel => {
                        this.settings_filter.clear();
                        window.blur();
                    }
                    Edit::Changed => {}
                    Edit::Moved => {}
                    Edit::Ignored => {
                cx.stop_propagation();
                return;
            }
                }
                cx.stop_propagation();
                cx.notify();
            }));
        div()
            .id("settings-page")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .py_2()
            .child(search)
            .when(show_account, |d| d.child(self.connect_section(window, cx)))
            .when(nothing, |d| d.child(self.status("No settings match.")))
            .children(sections.into_iter().filter(|(_, rows)| !rows.is_empty()).map(|(title, rows)| {
                div()
                    .flex()
                    .flex_col()
                    .child(div().px_4().pt_4().pb_2().text_xs().text_color(themed(MUTED)).child(title))
                    .children(rows)
            }))
            .when(query.is_empty() || matches("Changes apply", "player"), |d| {
                d.child(
                    div()
                        .px_4()
                        .pt_2()
                        .text_xs()
                        .text_color(themed(MUTED))
                        .child("Changes apply right away (text fields: after Enter)."),
                )
            })
            .when(self.settings.sponsorblock && player::sponsorblock_script().is_none(), |d| {
                d.child(div().px_4().pt_1().text_xs().text_color(themed(ACCENT)).child(
                    "SponsorBlock script not found: start the app from its nix-shell (sets UNBLOATED_SPONSORBLOCK).",
                ))
            })
            .when(self.shortcuts().iter().any(|(k, what)| matches(k, what)) || matches("Keyboard shortcuts", "keys"), |d| {
                d.child(div().px_4().pt_4().pb_2().text_xs().text_color(themed(MUTED)).child("KEYBOARD SHORTCUTS")).children(
                    self.shortcuts().iter().filter(|(k, what)| matches(k, what) || matches("Keyboard shortcuts", "keys")).map(|(k, what)| {
                        div()
                            .px_4()
                            .py_1()
                            .flex()
                            .text_sm()
                            .child(div().w(px(190.)).flex_none().text_color(themed(TEXT)).child(*k))
                            .child(div().text_color(themed(MUTED)).child(*what))
                    }),
                )
            })
            .child(
                div()
                    .px_4()
                    .pt_4()
                    .pb_4()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .id("about-github")
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py(px(7.))
                            .rounded_md()
                            .bg(themed(HOVER))
                            .cursor_pointer()
                            .hover(|d| d.bg(themed(BORDER)))
                            .tooltip(tip(REPO_URL))
                            .child(svg().path(icons::path("github")).size(px(16.)).text_color(themed(TEXT)))
                            .child(div().text_sm().text_color(themed(TEXT)).child("GitHub"))
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.open_url(REPO_URL, cx)),
                    )
                    .child(div().text_xs().text_color(themed(MUTED)).child(format!("UnbloatedTube {}", env!("CARGO_PKG_VERSION")))),
            )
            .into_any_element()
    }

    fn render_browser(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (back, list_id) = match tab {
            Tab::Subscriptions => ("← Subscriptions", "subs"),
            _ => ("← Playlists", "playlists"),
        };
        if let Some(open) = self.browser_ref(tab).open.clone() {
            // A channel opened from a video's channel link may not be a subscription.
            let subscribed = open.id == FEED_ID || self.subs.groups.items().iter().any(|g| g.id == open.id);
            let back = if tab == Tab::Subscriptions && !subscribed { "← Back" } else { back };
            // Subscribe / unsubscribe for a real channel (UC… id), when logged in.
            let sub_button = (tab == Tab::Subscriptions && open.id.starts_with("UC") && self.cfg.has_auth()).then(|| {
                let confirming = self.confirm_unsub.as_deref() == Some(open.id.as_str());
                let g = open.clone();
                let button = if confirming {
                    // The confirm step stays spelled out.
                    self.chip("channel-sub", "Unsubscribe?", true).ml_1()
                } else if subscribed {
                    self.icon_chip("channel-sub", "subscribed", false, "Subscribed — click twice to unsubscribe")
                } else {
                    self.icon_chip("channel-sub", "subscribe", true, "Subscribe")
                };
                button
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                        // The header row itself means "back"; don't trigger it.
                        cx.stop_propagation();
                        this.toggle_channel_sub(g.clone(), subscribed, cx);
                    })
            });
            let is_channel = tab == Tab::Subscriptions && open.id != FEED_ID;
            let groups_button = is_channel.then(|| {
                let in_groups: Vec<&str> = self.groups.iter().filter(|g| g.channels.contains(&open.id)).map(|g| g.name.as_str()).collect();
                let tooltip =
                    if in_groups.is_empty() { "Add this channel to groups".to_string() } else { format!("In: {} (click to change)", in_groups.join(", ")) };
                let count = in_groups.len();
                self.icon_chip("channel-groups", "folder", self.editing_groups, tooltip)
                    .when(count > 0, |d| {
                        // A count badge, so you can tell the channel is in a group without opening the editor.
                        d.relative().child(
                            div()
                                .absolute()
                                .top(px(-4.))
                                .right(px(-4.))
                                .min_w(px(14.))
                                .h(px(14.))
                                .px(px(3.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(themed(ACCENT))
                                .border_1()
                                .border_color(themed(BORDER))
                                .text_size(px(9.))
                                .text_color(themed(ON_ACCENT))
                                .child(count.to_string()),
                        )
                    })
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                        cx.stop_propagation();
                        this.editing_groups = !this.editing_groups;
                        cx.notify();
                    })
            });
            let flag_chips = is_channel.then(|| {
                let (notify, muted) = (self.flags.notify.contains(&open.id), self.flags.muted.contains(&open.id));
                let (a, b) = (open.id.clone(), open.id.clone());
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .child(
                        self.icon_chip(
                            "channel-notify",
                            "bell",
                            notify,
                            if notify { "Notifications on (click to turn off)" } else { "Notify me about new uploads" },
                        )
                        .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle_flag(true, &a, cx);
                            }),
                    )
                    .child(
                        self.icon_chip(
                            "channel-mute",
                            "muted",
                            muted,
                            if muted { "Muted: hidden from New uploads (click to unmute)" } else { "Hide this channel's uploads from New uploads" },
                        )
                        .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle_flag(false, &b, cx);
                            }),
                    )
            });
            let queue_button = (tab == Tab::Playlists).then(|| {
                self.icon_chip("queue-all", "save", false, "Add all to Up next").on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.enqueue_all(tab, cx);
                })
            });
            let play_button = (tab == Tab::Playlists).then(|| {
                self.icon_chip("play-all", "play", false, "Play the whole playlist").on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                    cx.stop_propagation();
                    let videos = this.visible(&this.browser_videos(tab));
                    this.play_all(videos, cx);
                })
            });
            let cast_button = (tab == Tab::Playlists && self.cast_available()).then(|| {
                self.icon_chip("cast-all", "cast", false, "Cast the whole playlist").on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                    cx.stop_propagation();
                    let videos = this.visible(&this.browser_videos(tab));
                    this.cast_list(None, videos, cx);
                })
            });
            let group_editor = (is_channel && self.editing_groups).then(|| self.render_group_editor(&open.id, window, cx));

            let view = self.browser_ref(tab).view;
            // Channels (not the New uploads feed or playlists) get Videos | Shorts | Live tabs.
            let channel_tabs = tab == Tab::Subscriptions && open.id != FEED_ID;
            let shorts_tab = self.settings.shorts;
            let videos = &self.browser_ref(tab).videos;
            let empty = match view {
                ChannelView::Shorts => "No Shorts.",
                ChannelView::Live => "No live streams.",
                ChannelView::Videos => "No videos.",
            };
            let shown = self.browser_videos(tab);
            let body = match self.placeholder(videos, empty, Rows::Videos) {
                Some(p) => p,
                None if shown.is_empty() && !self.list_filter.trim().is_empty() => self.status("No matches."),
                None => self.video_list("group", &shown, Some(0), cx),
            };
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .id("back")
                        .px_3()
                        .py_2()
                        .flex()
                        .text_sm()
                        .cursor_pointer()
                        .border_b_1()
                        .border_color(themed(BORDER))
                        .hover(|d| d.bg(themed(HOVER)))
                        .child(svg().path(icons::path("arrow-left")).size(px(16.)).mr_3().flex_none().text_color(themed(MUTED)))
                        .tooltip(tip(back.trim_start_matches("← ").to_string()))
                        .items_center()
                        .child(div().flex_1().min_w_0().text_color(themed(TEXT)).truncate().child(open.title.clone()))
                        .children(flag_chips)
                        .children(groups_button)
                        .children(sub_button)
                        .children(queue_button)
                        .children(play_button)
                        .children(cast_button)
                        .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                            this.browser(tab).open = None;
                            cx.notify();
                        }),
                )
                .children(group_editor)
                .when(channel_tabs, |d| {
                    d.child(
                        div()
                            .flex()
                            .px_2()
                            .border_b_1()
                            .border_color(themed(BORDER))
                            .child(
                                tab_button("Videos", view == ChannelView::Videos)
                                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.set_channel_view(ChannelView::Videos, cx)),
                            )
                            .when(shorts_tab, |d| {
                                d.child(
                                    tab_button("Shorts", view == ChannelView::Shorts)
                                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.set_channel_view(ChannelView::Shorts, cx)),
                                )
                            })
                            .child(
                                tab_button("Live", view == ChannelView::Live)
                                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.set_channel_view(ChannelView::Live, cx)),
                            ),
                    )
                })
                .child(self.filter_bar("Filter videos", window, cx))
                .child(body)
                .into_any_element();
        }
        let groups = &self.browser_ref(tab).groups;
        if let Some(p) = self.placeholder(groups, "Nothing here.", Rows::Groups) {
            return p;
        }
        let counts = Arc::new(if tab == Tab::Subscriptions { self.unseen_counts() } else { HashMap::new() });
        let g: Arc<[Group]> = self.group_items(tab, &counts).into();
        let bar = if tab == Tab::Subscriptions {
            self.render_group_bar(window, cx)
        } else {
            self.filter_bar("Filter playlists", window, cx)
        };
        let list = uniform_list(
            list_id,
            g.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|i| {
                        let group = g[i].clone();
                        let avatar = if group.id == FEED_ID {
                            div()
                                .size(px(28.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(themed(ACCENT))
                                .child(svg().path(icons::path("new")).size(px(14.)).text_color(themed(ON_ACCENT)))
                                .into_any_element()
                        } else {
                            this.thumb_el(&group.id, group.thumb.clone(), 28., 28., px(14.), cx)
                        };
                        let new = counts.get(&group.id).copied().unwrap_or(0);
                        let selected = this.vim_selected(i);
                        let muted = this.flags.muted.contains(&group.id);
                        let bell = this.flags.notify.contains(&group.id);
                        let in_groups: Vec<String> =
                            this.groups.iter().filter(|g| g.channels.contains(&group.id)).map(|g| g.name.clone()).collect();
                        // Right-click a subscribed channel (not "New uploads") for its menu.
                        let menu_group = (tab == Tab::Subscriptions && group.id.starts_with("UC") && this.cfg.has_auth()).then(|| group.clone());
                        // Right-click a playlist: cast it without opening it.
                        let menu_playlist = (tab == Tab::Playlists).then(|| group.clone());
                        div()
                            .id(i)
                            .when_some(menu_playlist, |d, g| {
                                d.on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(move |this, ev: &gpui::MouseDownEvent, _, cx| {
                                        this.channel_menu = None;
                                        this.video_menu = None;
                                        this.playlist_menu = Some((g.clone(), ev.position, Instant::now()));
                                        this.menu_hovered = false;
                                        cx.notify();
                                    }),
                                )
                            })
                            .when_some(menu_group, |d, g| {
                                d.on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(move |this, ev: &gpui::MouseDownEvent, _, cx| {
                                        this.confirm_unsub = None;
                                        this.video_menu = None;
                                        this.channel_menu = Some((g.clone(), ev.position, Instant::now()));
                                        this.menu_hovered = false;
                                        cx.notify();
                                    }),
                                )
                            })
                            .when(selected, |d| d.bg(themed(BORDER)))
                            .w_full()
                            .h(px(44.))
                            .px_3()
                            .flex()
                            .items_center()
                            .gap_3()
                            .text_sm()
                            .text_color(themed(TEXT))
                            .cursor_pointer()
                            .hover(|d| d.bg(themed(HOVER)))
                            .child(avatar)
                            .child(
                                div()
                                    .id("name")
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .when(muted, |d| d.text_color(themed(MUTED)))
                                    .child(group.title.clone())
                                    .tooltip(tip(group.title.clone())),
                            )
                            .when_some(group.subscribers.filter(|_| this.settings.show_subs), |d, n| {
                                d.child(div().flex_none().text_xs().text_color(themed(MUTED)).child(fmt_count(n)))
                            })
                            .when(!in_groups.is_empty(), |d| {
                                d.child(
                                    div()
                                        .id(("in-groups", i))
                                        .flex_none()
                                        .child(svg().path(icons::path("folder")).size(px(12.)).text_color(themed(MUTED)))
                                        .tooltip(tip_left(format!("In: {}", in_groups.join(", ")))),
                                )
                            })
                            .when(bell, |d| d.child(svg().path(icons::path("bell")).size(px(12.)).flex_none().text_color(themed(MUTED))))
                            .when(muted, |d| d.child(svg().path(icons::path("muted")).size(px(13.)).flex_none().text_color(themed(MUTED))))
                            .when(new > 0, |d| {
                                d.child(
                                    div()
                                        .flex_none()
                                        .min_w(px(20.))
                                        .px(px(6.))
                                        .flex()
                                        .justify_center()
                                        .whitespace_nowrap()
                                        .rounded_full()
                                        .bg(themed(ACCENT))
                                        .text_xs()
                                        .text_color(themed(ON_ACCENT))
                                        .child(new.to_string()),
                                )
.tooltip(tip_left(format!("{new} new video{}", if new == 1 { "" } else { "s" })))
                            })
                            .on_click_hinted(&this.hint_reg(), cx, move |this, _, _, cx| this.open_group(tab, group.clone(), cx))
                    })
                    .collect()
            }),
        )
        .track_scroll(self.vim_scroll.clone())
        .flex_1();
        // Subscriptions: the channel list, or what is live now (one click from the top).
        if tab == Tab::Subscriptions {
            let live_count = self.live_videos(false).len();
            let tabs = div()
                .flex()
                .px_2()
                .border_b_1()
                .border_color(themed(BORDER))
                .child(
                    tab_button("Channels", !self.subs_live)
                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.set_subs_live(false, cx)),
                )
                .child(
                    tab_button(if live_count > 0 { format!("Live ({live_count})") } else { "Live".to_string() }, self.subs_live)
                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.set_subs_live(true, cx)),
                );
            let body = if self.subs_live {
                let live = self.live_videos(true);
                match self.placeholder(&self.feed, "No live streams right now.", Rows::Videos) {
                    Some(p) => p,
                    None if live.is_empty() && !self.live_videos(false).is_empty() => self.status("No matches."),
                    None if live.is_empty() => self.status("No live streams right now."),
                    None => self.video_list("live", &live, Some(0), cx),
                }
            } else {
                list.into_any_element()
            };
            return div().flex().flex_col().flex_1().min_h_0().child(tabs).child(bar).child(body).into_any_element();
        }
        div().flex().flex_col().flex_1().min_h_0().child(bar).child(list).into_any_element()
    }

    fn set_subs_live(&mut self, live: bool, cx: &mut Context<Self>) {
        if self.subs_live != live {
            self.subs_live = live;
            self.vim_cursor = 0;
            cx.notify();
        }
    }

    /// The video area: thumbnail underneath, mpv's embedded window placed on top of it.
    fn screen(&mut self, video: &Video, full: bool, cx: &mut Context<Self>) -> gpui::Div {
        let mut screen = div().relative().bg(gpui::black()).overflow_hidden();
        if full {
            screen = screen.size_full();
        } else {
            // Fills the player section; mpv letterboxes the video inside.
            screen = screen.w_full().flex_1().min_h_0();
        }
        match self.thumb(&video.id, Some(video.thumb_url()), cx) {
            Thumb::Ready(path) => screen = screen.child(img(path).size_full().object_fit(ObjectFit::Contain)),
            Thumb::Pending => screen = screen.child(pulse("screen-skeleton", div().size_full().bg(themed(HOVER)))),
            Thumb::Missing => {}
        }
        if self.pip {
            screen = screen.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .bg(gpui::black().opacity(0.7))
                    .child(div().text_sm().text_color(themed(TEXT)).child("Playing in picture-in-picture"))
                    .child(
                        self.chip("pip-back", "Bring it back here", true)
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.toggle_pip(cx)),
                    ),
            );
        }
        if let Some(c) = &self.casting {
            let (name, status) = (c.name.clone(), c.status.clone());
            let line = match &status {
                Some(s) if s.playing => format!("{} / {}", fmt_duration(s.position), fmt_duration(s.duration)),
                _ => "Starting…".to_string(),
            };
            let listed = self.casting.as_ref().is_some_and(|c| c.listed);
            let queue_line = status.as_ref().and_then(|s| s.queue).map(|(i, n)| format!("{} of {n}", i + 1));
            let now_title = status.as_ref().map(|s| s.title.clone()).filter(|t| !t.is_empty());
            let paused = status.as_ref().is_some_and(|s| s.paused);
            screen = screen.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .bg(gpui::black().opacity(0.85))
                    .child(div().text_sm().text_color(themed(TEXT)).child(format!("Casting to {name}")))
                    .children(queue_line.map(|q| div().text_xs().text_color(themed(MUTED)).child(q)))
                    .children(now_title.map(|t| div().max_w(px(420.)).px_4().text_sm().text_color(themed(TEXT)).truncate().child(t)))
                    .child(div().text_xs().text_color(themed(MUTED)).child(line))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .when(listed, |d| {
                                d.child(
                                    self.chip("cast-prev", "Previous", true)
                                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.next_prev(false, cx)),
                                )
                            })
                            .child(
                                self.chip("cast-back", "−10s", status.is_some())
                                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.seek_by(-10., cx)),
                            )
                            .child(
                                self.chip("cast-pause", if paused { "Play" } else { "Pause" }, status.is_some())
                                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.pause_toggle(cx)),
                            )
                            .child(
                                self.chip("cast-fwd", "+10s", status.is_some())
                                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.seek_by(10., cx)),
                            )
                            .when(listed, |d| {
                                d.child(
                                    self.chip("cast-next", "Next", true)
                                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.next_prev(true, cx)),
                                )
                            })
                            .child(
                                self.chip("cast-stop", "Stop", true)
                                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.stop_cast(cx)),
                            ),
                    )
                    .child(
                        self.chip("cast-here", "Back to this screen", true)
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.end_cast("Cast view closed, it keeps playing there", cx)),
                    ),
            );
        }
        // Nothing playing yet (e.g. after startup): clicking the picture starts it. The playback
        // buttons under the video are replaced by the hover bar, which exists only in mpv's window.
        if !full && !self.pip && !self.loading && self.state.is_none() && self.settings.video_controls {
            let v = video.clone();
            screen = screen.child(
                div()
                    .id("screen-play")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .child(svg().path(icons::path("play")).size(px(56.)).text_color(gpui::white().opacity(0.85)))
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.play(v.clone(), None, cx)),
            );
        }
        if let Some(embed) = self.embed.clone() {
            screen = screen.child(
                canvas(|_, _, _| {}, move |b, _, window, _| {
                    let s = window.scale_factor();
                    let px = |v: gpui::Pixels| (f32::from(v) * s).round();
                    embed.borrow_mut().place(
                        px(b.origin.x) as i32,
                        px(b.origin.y) as i32,
                        px(b.size.width) as u32,
                        px(b.size.height) as u32,
                    );
                })
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        }
        screen
    }

    fn render_player(&mut self, cx: &mut Context<Self>) -> gpui::Div {
        let Some(video) = self.current.clone() else {
            return div().p_4().text_sm().text_color(themed(MUTED)).child("Pick a video on the left.");
        };
        let resume = self.history.position(&video.id);
        let screen = self.screen(&video, false, cx);
        let cast_status = self.casting.as_ref().map(|c| c.status.clone());
        let (pos, dur, paused, active) = match (&cast_status, &self.state) {
            (Some(Some(c)), _) => (c.position, c.duration, c.paused, true),
            (Some(None), _) => (resume, video.duration.unwrap_or(0.), true, false),
            (None, Some(s)) => (s.position, s.duration, s.paused, true),
            (None, None) => (resume, video.duration.unwrap_or(0.), true, false),
        };
        let filled = if dur > 0. { ((pos / dur) * SEEK_SEGMENTS as f64) as usize } else { 0 };
        let chapters = self.state.as_ref().filter(|_| self.casting.is_none()).map(|s| s.chapters.clone()).unwrap_or_default();

        let (play_icon, play_tip) = match (active, paused) {
            (true, true) => ("play", "Play (Space)".to_string()),
            (true, false) => ("pause", "Pause (Space)".to_string()),
            (false, _) if resume > 0. => ("play", format!("Resume at {}", fmt_duration(resume))),
            (false, _) => ("play", "Play".to_string()),
        };
        let time = if self.loading {
            String::new()
        } else {
            format!("{} / {}", fmt_duration(pos), fmt_duration(dur))
        };
        let replay = video.clone();
        let info = {
            let st = self.current_status();
            let parts: Vec<String> = [
                st.and_then(|s| s.views.clone()).filter(|_| self.settings.show_views),
                st.and_then(|s| s.date.clone()).filter(|_| self.settings.show_date),
                self.votes
                    .as_ref()
                    .filter(|(id, _)| self.settings.show_votes && *id == video.id)
                    .and_then(|(_, c)| *c)
                    .map(|(likes, dislikes)| format!("{} likes  ·  {} dislikes", fmt_count(likes), fmt_count(dislikes))),
            ]
            .into_iter()
            .flatten()
            .collect();
            div()
                .flex()
                .items_center()
                .text_xs()
                .text_color(themed(MUTED))
                .when(video.live, |d| d.child(div().mr_2().child(live_badge(true))))
                .child(parts.join("  ·  "))
                // A live stream's position and length mean little: the badge says it.
                .child(div().ml_auto().pl_2().flex_none().child(if video.live { String::new() } else { time }))
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .child(screen)
            .child({
                // A DeArrow title, with YouTube's own under it, small.
                let (title, original) = self.shown_title(&video, cx);
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_color(themed(TEXT)).line_clamp(2).child(title))
                    .when_some(original, |d, o| d.child(div().text_xs().text_color(themed(MUTED)).truncate().child(format!("Original title: {o}"))))
            })
            .child(info)
            .child({
                let name = video.channel.clone().unwrap_or_default();
                let subs = self.current_status().and_then(|s| s.subscribers.clone()).filter(|_| self.settings.show_subs);
                match channel_group(&video) {
                    // Styled as a chip (avatar, name, chevron) so it reads as clickable.
                    Some(g) => {
                        // Avatar from your subscriptions, else from YouTube's info on this video.
                        let thumb = self
                            .subs
                            .groups
                            .items()
                            .iter()
                            .find(|s| s.id == g.id)
                            .and_then(|s| s.thumb.clone())
                            .or_else(|| self.current_status().and_then(|st| st.avatar.clone()));
                        let avatar = self.thumb_el(&g.id, thumb, 20., 20., px(10.), cx);
                        div().flex().child(
                            div()
                                .id("channel-link")
                                .flex()
                                .items_center()
                                .gap_2()
                                .pl_1()
                                .pr_3()
                                .py_1()
                                .rounded_full()
                                .bg(themed(HOVER))
                                .text_xs()
                                .text_color(themed(TEXT))
                                .cursor_pointer()
                                .hover(|d| d.bg(themed(BORDER)))
                                .child(avatar)
                                .child(name)
                                .children(subs.map(|s| div().text_color(themed(MUTED)).child(s)))
                                .child(div().text_color(themed(MUTED)).child("›"))
                                .tooltip(tip("Show this channel's videos"))
                                .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.show_channel(g.clone(), cx)),
                        )
                    }
                    None => div().text_xs().text_color(themed(MUTED)).child(name),
                }
            })
            .child(if self.loading {
                div().h(px(10.)).py(px(3.)).child(loading_bar("video-loading")).into_any_element()
            } else {
                div()
                    .flex()
                    .h(px(10.))
                    .items_center()
                    .children((0..SEEK_SEGMENTS).map(|i| {
                        let step = dur / SEEK_SEGMENTS as f64;
                        let at = step * i as f64;
                        // Hover shows the time there and the chapter it falls in; chapter starts
                        // get a small gap so the bar shows where chapters begin.
                        let chapter = chapters.iter().rev().find(|(t, _)| *t <= at).map(|(_, title)| title.clone());
                        let starts_chapter = i > 0 && chapters.iter().any(|(t, _)| *t > 0. && *t >= at && *t < at + step);
                        let tooltip = match chapter.filter(|c| !c.is_empty()) {
                            Some(c) => format!("{} · {c}", fmt_duration(at)),
                            None => fmt_duration(at),
                        };
                        div()
                            .id(("seek", i))
                            .flex_1()
                            .h_full()
                            .py(px(3.))
                            .when(starts_chapter, |d| d.ml(px(3.)))
                            .cursor_pointer()
                            .child(div().size_full().bg(if i < filled { themed(ACCENT) } else { themed(BORDER) }))
                            .when(dur > 0., |d| d.tooltip(tip(tooltip)))
                            .when(active, |d| {
                                d.on_click(cx.listener(move |this, _, _, cx| {
                                    this.seek_to(dur * i as f64 / SEEK_SEGMENTS as f64, cx)
                                }))
                            })
                    }))
                    .into_any_element()
            })
            // With the hover bar on the video, playback controls live there instead.
            .when(!self.settings.video_controls, |d| d.child(
                div()
                    .flex()
                    .items_center()
                    .child(icon_button("play", play_icon, play_tip, true).on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                        if this.casting.is_some() {
                            this.pause_toggle(cx);
                        } else if this.state.is_some() {
                            this.player.toggle_pause();
                        } else {
                            this.play(replay.clone(), None, cx);
                        }
                    }))
                    .child(self.step_button("prev", "prev", "Previous (P)", -1, cx))
                    .child(self.step_button("next", "next", "Next (N)", 1, cx))
                    .child(
                        icon_button("back10", "back", "Back 10 seconds (J)", active)
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.seek_by(-10., cx)),
                    )
                    .child(
                        icon_button("fwd10", "forward", "Forward 10 seconds (L)", active)
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.seek_by(10., cx)),
                    )
                    .child(
                        icon_button("pip", "pip", if self.pip { "Back into the app" } else { "Picture-in-picture" }, active || self.pip)
                            .when(self.pip, |d| d.bg(themed(ACCENT)))
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.toggle_pip(cx)),
                    )
                    .child(
                        icon_button("full", "fullscreen", "Fullscreen (F, Esc to leave)", active)
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, _| this.player.set_fullscreen(true)),
                    )
                    .child(
                        div()
                            .id("speed")
                            .mr_2()
                            .h(px(30.))
                            .px_2()
                            .flex()
                            .items_center()
                            .rounded_md()
                            .bg(themed(HOVER))
                            .text_xs()
                            .text_color(themed(TEXT))
                            .cursor_pointer()
                            .hover(|d| d.bg(themed(BORDER)))
                            .child(format!("{}×", self.settings.speed))
                            .tooltip(tip("Playback speed (click to change)"))
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.cycle_speed(cx)),
                    )
                    .when(self.settings.volume_control, |d| d.child(self.volume_bar(cx)))
            ))
            // Second row: what you can do with this video.
            .when(self.account_buttons() || self.cast_available() || self.cc_under_player() || self.settings.share_button || self.settings.share_time_button || self.settings.browser_button || self.settings.download_button || self.settings.history_buttons, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                            .when(self.settings.history_buttons, |d| {
                                let (back, next) = self.nav_targets();
                                let idle = self.casting.is_none();
                                let (can_back, can_next) = (idle && back.is_some(), idle && next.is_some());
                                let title = |i: Option<usize>| i.and_then(|i| self.nav.get(i)).map(|v| v.title.clone());
                                d.child(
                                    icon_button("nav-back", "arrow-left", format!("Back: {} (Alt+←)", title(back).unwrap_or_else(|| "nothing earlier".into())), can_back)
                                        .when(can_back, |d| d.on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.nav_go(false, cx))),
                                )
                                .child(
                                    icon_button("nav-forward", "arrow-right", format!("Forward: {} (Alt+→)", title(next).unwrap_or_else(|| "nothing later".into())), can_next)
                                        .when(can_next, |d| d.on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.nav_go(true, cx))),
                                )
                            })
                            .when(self.account_buttons(), |d| d.children(self.account_buttons_els(cx)))
                            .when(self.settings.download_button, |d| {
                                let saved = self.library.iter().find(|s| s.video.id == video.id).map(|s| s.path.clone());
                                let (tip_text, enabled) = match self.downloads.get(&video.id) {
                                    _ if saved.is_some() => (format!("Downloaded to {}", saved.unwrap_or_default()), false),
                                    Some(Download { result: None, progress, .. }) => (format!("Downloading {progress:.0}%"), false),
                                    Some(Download { result: Some(Ok(f)), .. }) => (format!("Downloaded to {f}"), false),
                                    _ => ("Download".to_string(), true),
                                };
                                let v = video.clone();
                                d.child(
                                    icon_button("download", "download", tip_text, enabled)
                                        .when(enabled, |d| d.on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.download(v.clone(), cx))),
                                )
                            })
                            .when(self.cast_available(), |d| {
                                let targets = self.cast_targets();
                                let many = targets.len() > 1;
                                d.children(targets.into_keys().enumerate().map(|(i, name)| {
                                    let target = name.clone();
                                    icon_button(("cast", i), "cast", format!("Cast to {name}{}", if i == 0 && !many { " (T)" } else { "" }), true)
                                        .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.cast_to(Some(target.clone()), cx))
                                }))
                            })
                            .when(self.cc_under_player(), |d| {
                                let on = self.state.as_ref().is_some_and(|s| s.sub_on);
                                let key = if self.settings.vim { "v" } else { "V" };
                                let tip_text = format!("Subtitles {} ({key})", if on { "on" } else { "off" });
                                d.child(
                                    icon_button("subtitles", if on { "cc" } else { "cc-off" }, tip_text, true)
                                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.toggle_subtitles(cx)),
                                )
                            })
                            .when(self.settings.share_button, |d| {
                                let key = if self.settings.vim { "yy" } else { "C" };
                                d.child(
                                    icon_button("share", "share", format!("Copy link ({key})"), true)
                                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.copy_link(cx)),
                                )
                            })
                            .when(self.settings.share_time_button, |d| {
                                let key = if self.settings.vim { "yt" } else { "⇧C" };
                                d.child(
                                    icon_button("share-time", "recent", format!("Copy link at the current time ({key})"), true)
                                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.copy_link_at_time(cx)),
                                )
                            })
                            .when(self.settings.browser_button, |d| {
                                d.child(
                                    icon_button("browser", "browser", if self.settings.vim { "Open in browser (o)" } else { "Open in browser (O)" }, true)
                                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.open_in_browser(cx)),
                                )
                            })
                )
            })
    }

    /// Mute button and a ten-step volume bar (click a step to set the volume).
    fn volume_bar(&self, cx: &mut Context<Self>) -> gpui::Div {
        let muted = self.state.as_ref().is_some_and(|s| s.muted);
        let volume = self.settings.volume;
        let filled = if muted { 0 } else { (volume / 10.).round() as usize };
        let silent = muted || volume == 0.;
        div()
            .flex()
            .items_center()
            .mr_2()
            .child(
                icon_button("mute", if silent { "volume-off" } else { "volume" }, if muted { "Unmute (M)" } else { "Mute (M)" }, true)
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, _| this.player.toggle_mute()),
            )
            .child(div().flex().items_center().h(px(30.)).children((1..=10usize).map(|i| {
                div()
                    .id(("volume", i))
                    .h_full()
                    .px(px(1.5))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .child(div().w(px(5.)).h(px(14.)).rounded(px(1.)).bg(if i <= filled { themed(ACCENT) } else { themed(BORDER) }))
                    .tooltip(tip(format!("Volume {}%", i * 10)))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_volume(i as f32 * 10., cx)))
            })))
    }

    /// Notices and the current video's download status, in the bottom-right corner (under the
    /// controls they'd be covered by the lower pane when the player is short).
    fn render_toasts(&self) -> Option<gpui::Div> {
        let download = self.current.as_ref().and_then(|v| self.downloads.get(&v.id)).and_then(Download::visible_status);
        let lines: Vec<String> = self.notice.iter().cloned().chain(download).chain(self.sleep_status()).collect();
        (!lines.is_empty()).then(|| {
            div()
                .absolute()
                .bottom(px(16.))
                .right(px(16.))
                .flex()
                .flex_col()
                .items_end()
                .gap_2()
                .children(lines.into_iter().map(|n| {
                    div()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(themed(HOVER))
                        .border_1()
                        .border_l_4()
                        .border_color(themed(ACCENT))
                        .shadow_lg()
                        .text_sm()
                        .text_color(themed(TEXT))
                        .max_w(px(480.))
                        .line_clamp(2)
                        .child(n)
                }))
        })
    }

    /// Subscribe / Save / Like icons, as enabled in settings. Greyed until the account's
    /// state for this video is known.
    fn account_buttons_els(&mut self, cx: &mut Context<Self>) -> Vec<Stateful<gpui::Div>> {
        let st = self.current_status().cloned();
        let ready = st.is_some();
        let s = st.unwrap_or_default();
        let set = &self.settings;
        let mut out = Vec::new();
        if set.subscribe_button && (!ready || s.channel_id.is_some()) {
            let (icon, tip) = if s.subscribed { ("subscribed", "Unsubscribe") } else { ("subscribe", "Subscribe") };
            out.push(
                icon_button("subscribe", icon, tip, ready)
                    .when(ready && !s.subscribed, |d| d.bg(themed(ACCENT)))
                    .when(ready, |d| d.on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.toggle_subscribe(cx))),
            );
        }
        if set.save_button {
            out.push(icon_button("save", "save", "Save to playlist", ready).when(ready, |d| {
                d.on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                    if this.saving { this.close_save(cx) } else { this.open_save(window, cx) }
                })
            }));
        }
        if set.watch_later_button {
            out.push(icon_button("watch-later", "watch-later", "Watch later (W)", ready).when(ready, |d| {
                d.on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.watch_later(cx))
            }));
        }
        if set.like_button {
            let (icon, tip) = if s.liked { ("liked", "Remove like") } else { ("like", "Like") };
            out.push(
                icon_button("like", icon, tip, ready)
                    // Red while liked, so the state is obvious.
                    .when(ready && s.liked, |d| d.bg(themed(ACCENT)))
                    .when(ready, |d| d.on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.toggle_like(cx))),
            );
        }
        if set.dislike_button {
            let (icon, tip) = if s.disliked { ("disliked", "Remove dislike") } else { ("dislike", "Dislike") };
            out.push(
                icon_button("dislike", icon, tip, ready)
                    .when(ready && s.disliked, |d| d.bg(themed(ACCENT)))
                    .when(ready, |d| d.on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.toggle_dislike(cx))),
            );
        }
        out
    }

    /// Full-height "Save to playlist" list over the right column.
    fn render_save_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let title = self.current.as_ref().map(|v| v.title.clone()).unwrap_or_default();
        let focused = self.save_focus.is_focused(window);
        let lists: Arc<[Group]> = self.save_targets().into();
        let body = if lists.is_empty() {
            if self.playlists.groups.items().is_empty() || self.save_editable.is_none() {
                skeleton(Rows::Groups)
            } else {
                self.status("No matching playlists.")
            }
        } else {
            let lists = lists.clone();
            uniform_list(
                "save-list",
                lists.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|i| {
                            let g = lists[i].clone();
                            let cover = this.thumb_el(&g.id, g.thumb.clone(), 64., 36., px(4.), cx);
                            div()
                                .id(i)
                                .w_full()
                                .h(px(52.))
                                .px_4()
                                .flex()
                                .items_center()
                                .gap_3()
                                .text_sm()
                                .text_color(themed(TEXT))
                                .cursor_pointer()
                                .hover(|d| d.bg(themed(HOVER)))
                                .child(cover)
                                .child(div().truncate().child(g.title.clone()))
                                .on_click_hinted(&this.hint_reg(), cx, move |this, _, _, cx| this.save_to(g.clone(), cx))
                        })
                        .collect()
                }),
            )
            .flex_1()
            .into_any_element()
        };
        div()
            .id("save-overlay")
            // Block hover/clicks from reaching the player controls underneath.
            .occlude()
            .track_focus(&self.save_focus)
            .on_key_down(cx.listener(Self::save_key))
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .flex_col()
            .bg(themed(PANEL))
            .child(
                div()
                    .flex()
                    .items_center()
                    .px_4()
                    .pt_4()
                    .pb_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().text_color(themed(TEXT)).child("Save to playlist"))
                            .child(div().text_xs().text_color(themed(MUTED)).truncate().child(title)),
                    )
                    .child(icon_button("save-close", "close", "Close (Esc)", true).mr_0().on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.close_save(cx))),
            )
            .child(
                div()
                    .mx_4()
                    .mb_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(themed(HOVER))
                    .border_1()
                    .border_color(if focused { themed(MUTED) } else { themed(BORDER) })
                    .text_sm()
                    .map(|d| match (self.save_filter.is_empty(), focused) {
                        (_, true) => d.text_color(themed(TEXT)).child(self.caret_text("save", &self.save_filter, "Type to filter…")),
                        (true, false) => d.text_color(themed(MUTED)).child("Click here, then type to filter"),
                        _ => d.text_color(themed(TEXT)).child(self.save_filter.clone()),
                    }),
            )
            .child(div().flex().flex_col().flex_1().min_h_0().child(body))
    }

    /// Full-window keyboard cheatsheet (?): keys as keycaps, grouped in columns.
    fn render_cheatsheet(&self, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let keycap = |k: &str| {
            div()
                .min_w(px(28.))
                .px_2()
                .py(px(2.))
                .flex()
                .justify_center()
                .rounded_md()
                .bg(themed(HOVER))
                .border_1()
                .border_b_2()
                .border_color(themed(BORDER))
                .text_sm()
                .text_color(themed(TEXT))
                .child(k.to_string())
        };
        let (mut rest, groups) = if self.settings.vim { (&VIM_SHORTCUTS[..], &VIM_SHEET_GROUPS[..]) } else { (&SHORTCUTS[..], &SHEET_GROUPS[..]) };
        let mut columns = [div().flex_1().min_w_0().flex().flex_col().gap_6(), div().flex_1().min_w_0().flex().flex_col().gap_6()];
        for &(title, n, col) in groups {
            let (items, tail) = rest.split_at(n);
            rest = tail;
            let group = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().pb_1().text_xs().text_color(themed(ACCENT)).child(title.to_uppercase()))
                .children(items.iter().map(|(keys, what)| {
                    div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .child(
                            div()
                                .w(px(170.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap_1()
                                .children(keys.split(" / ").map(keycap)),
                        )
                        // min_w_0 so long descriptions wrap inside the card.
                        .child(div().flex_1().min_w_0().text_sm().text_color(themed(MUTED)).child(*what))
                }));
            let c = std::mem::replace(&mut columns[col], div());
            columns[col] = c.child(group);
        }
        div()
            .id("cheatsheet")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(themed(BG))
            .on_click(cx.listener(|this, _, _, cx| {
                this.show_keys = false;
                this.sync_embed();
                cx.notify();
            }))
            .child(
                div()
                    .w(px(980.))
                    .max_w(relative(0.92))
                    .max_h(relative(0.92))
                    .p_8()
                    .rounded_xl()
                    .bg(themed(PANEL))
                    .border_1()
                    .border_color(themed(BORDER))
                    .flex()
                    .flex_col()
                    .gap_6()
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .child(div().flex_1().text_2xl().text_color(themed(TEXT)).child(if self.settings.vim {
                                "Keyboard shortcuts · Vim mode"
                            } else {
                                "Keyboard shortcuts"
                            }))
                            .child(div().text_xs().text_color(themed(MUTED)).child("? or Esc to close")),
                    )
                    // Scrolls when the window is too short for it.
                    .child(div().id("sheet-body").flex_1().min_h_0().overflow_y_scroll().flex().items_start().gap_10().children(columns)),
            )
    }

    /// Prev / Next: play the neighbouring video of the queue; greyed out at its ends.
    fn step_button(
        &self,
        id: &'static str,
        icon: &'static str,
        tip: &'static str,
        step: isize,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        // Casting a list: these drive the receiver's queue.
        if self.casting.as_ref().is_some_and(|c| c.listed) {
            return icon_button(id, icon, format!("{tip} (on the receiver)"), true)
                .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.next_prev(step > 0, cx));
        }
        let target = if step > 0 { self.next_video() } else { self.neighbor(step) };
        let queued = step > 0 && !self.up_next.is_empty();
        let tip = match &target {
            Some(v) if queued => format!("{tip}, from Up next: {}", v.title),
            Some(v) => format!("{tip}: {}", v.title),
            None => tip.to_string(),
        };
        icon_button(id, icon, tip, target.is_some()).when_some(target, |d, video| {
            d.on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.play(video.clone(), None, cx))
        })
    }

    /// Description tab is on and there is a video to show it for.
    fn description_on(&self) -> bool {
        self.settings.description && self.current.is_some()
    }

    /// Description of the current video, fetched on first view (and again after the video changes).
    fn render_description(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let id = self.current.as_ref().map(|v| v.id.clone());
        if self.description_for != id {
            self.description_for = id.clone();
            // Not the previous video's description while this one loads.
            self.description = Load::Idle;
            let id = id.unwrap_or_default();
            self.fetch(cx, "description", |s| &mut s.description, None, move |cfg, on| yt::description(cfg, &id, on));
        }
        let text = match &self.description {
            Load::Failed(e) => return self.failed(e),
            Load::Ready(v) => v.first().cloned().unwrap_or_default(),
            _ => return skeleton(Rows::Comments),
        };
        if text.is_empty() {
            return self.status("No description.".to_string());
        }
        div()
            .id("description")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_3()
            .py_2()
            .flex()
            .flex_col()
            .gap_2()
            // One element per paragraph: GPUI keeps single line breaks, blank lines separate them.
            .children(text.split("\n\n").enumerate().map(|(i, p)| {
                div().text_sm().text_color(themed(TEXT)).child(self.linked_text(("description-text", i), p.trim().to_string(), cx))
            }))
            .into_any_element()
    }

    /// `text` with its links, timestamps, #tags and @handles clickable (see `open_text_link`).
    fn linked_text(&self, id: impl Into<ElementId>, text: String, cx: &mut Context<Self>) -> AnyElement {
        let (ranges, targets): (Vec<_>, Vec<_>) = links::find(&text).into_iter().unzip();
        if ranges.is_empty() {
            return div().child(text).into_any_element();
        }
        let style = HighlightStyle {
            color: Some(themed(ACCENT).into()),
            underline: Some(UnderlineStyle { thickness: px(1.), ..Default::default() }),
            ..Default::default()
        };
        let styled = StyledText::new(text).with_highlights(ranges.iter().map(|r| (r.clone(), style)));
        let this = cx.entity().downgrade();
        InteractiveText::new(id, styled)
            .on_click(ranges, move |i, _, cx| {
                let target = targets[i].clone();
                this.update(cx, |this, cx| this.open_text_link(target, cx)).ok();
            })
            .into_any_element()
    }

    /// A link clicked in the description or a comment: a YouTube link (and an @handle) opens in
    /// the app, any other in the browser; a time seeks there; a #tag is searched for.
    fn open_text_link(&mut self, link: links::Link, cx: &mut Context<Self>) {
        match link {
            links::Link::Url(url) => match yt::parse_link(&url) {
                Some(link) => self.open_link(link, cx),
                None => self.open_url(&url, cx),
            },
            links::Link::Time(secs) => self.seek_link(secs, cx),
            links::Link::Tag(tag) => {
                self.query = tag;
                self.left_collapsed = false;
                self.run_search(cx);
            }
            links::Link::Handle(handle) => self.open_link(yt::YtLink::Channel { url: format!("https://www.youtube.com/{handle}") }, cx),
        }
        cx.notify();
    }

    /// A timestamp clicked in the description or a comment: play from there (starting the
    /// video first when it isn't loaded).
    fn seek_link(&mut self, secs: f64, cx: &mut Context<Self>) {
        let Some(video) = self.current.clone() else { return };
        let loaded = self.state.as_ref().filter(|s| s.path == self.media_path(&video));
        if self.casting.is_some() {
            self.seek_to(secs, cx);
        } else if let Some(paused) = loaded.map(|s| s.paused) {
            self.seek_to(secs, cx);
            if paused {
                self.player.toggle_pause();
            }
        } else {
            self.link_start = Some(secs);
            self.play(video, None, cx);
        }
    }

    /// Transcript tab is on and there is a video to show it for.
    fn transcript_on(&self) -> bool {
        self.settings.transcript && self.current.is_some()
    }

    /// The current video's captions as a list of lines, fetched on first view (and again after
    /// the video changes): a search field narrows them, a click jumps there.
    fn render_transcript(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let id = self.current.as_ref().map(|v| v.id.clone());
        if self.transcript_for != id {
            self.transcript_for = id.clone();
            // Not the previous video's lines while these load.
            self.transcript = Load::Idle;
            self.transcript_shown = None;
            self.transcript_query.clear();
            let id = id.unwrap_or_default();
            let ((langs, auto), dir) = (self.video_sub_langs(&id), store::cache_dir().join("captions"));
            self.fetch(cx, "transcript", |s| &mut s.transcript, None, move |cfg, on| yt::transcript(cfg, &id, &langs, auto, &dir, on));
        }
        let q = self.transcript_query.trim().to_lowercase();
        let all = self.transcript.items();
        let lines: Arc<[(f64, String)]> = all.iter().filter(|(_, t)| q.is_empty() || t.to_lowercase().contains(&q)).cloned().collect();
        let count = (!q.is_empty() && !all.is_empty()).then(|| match lines.len() {
            1 => "1 match".to_string(),
            n => format!("{n} matches"),
        });
        let body = match &self.transcript {
            Load::Failed(e) => self.failed(e),
            Load::Ready(v) if v.is_empty() => self.status("No captions in your subtitle languages (Settings → Subtitles)."),
            Load::Ready(_) if lines.is_empty() => self.status("Not said in this video."),
            Load::Ready(_) => self.transcript_list(lines, &q, cx),
            _ => skeleton(Rows::Comments),
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(themed(BORDER))
                    .child(self.transcript_field(window, cx).flex_1())
                    .children(count.map(|c| div().flex_none().text_xs().text_color(themed(MUTED)).child(c))),
            )
            .child(div().flex().flex_col().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    /// The lines, the playing one highlighted, matches of `q` marked; a click jumps there.
    fn transcript_list(&mut self, lines: Arc<[(f64, String)]>, q: &str, cx: &mut Context<Self>) -> AnyElement {
        let position = match &self.casting {
            Some(c) => c.status.as_ref().map_or(0., |s| s.position),
            None => self.state.as_ref().map_or(0., |s| s.position),
        };
        let current = lines.iter().rposition(|(t, _)| *t <= position);
        // Scrolled to the playing line when the list appears, then left where you read.
        if self.transcript_shown.is_none() {
            self.transcript_shown = Some(current.unwrap_or(0));
            self.transcript_scroll.scroll_to_item(current.unwrap_or(0), ScrollStrategy::Center);
        }
        let q = q.to_string();
        uniform_list(
            "transcript",
            lines.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|i| {
                        let (start, text) = &lines[i];
                        let (start, is_current) = (*start, Some(i) == current);
                        // The match, where lower-casing kept the text's byte offsets.
                        let lower = text.to_lowercase();
                        let found = (!q.is_empty() && lower.len() == text.len()).then(|| lower.find(&q)).flatten().map(|at| at..at + q.len());
                        let mark = HighlightStyle { background_color: Some(gpui::rgba((ACCENT << 8) | 0x66).into()), ..Default::default() };
                        let line = StyledText::new(text.clone()).with_highlights(found.map(|r| (r, mark)));
                        div()
                            .id(i)
                            .w_full()
                            .h(px(32.))
                            .px_3()
                            .flex()
                            .items_center()
                            .gap_3()
                            .cursor_pointer()
                            .when(is_current, |d| d.bg(themed(HOVER)))
                            .hover(|d| d.bg(themed(HOVER)))
                            .child(
                                div()
                                    .w(px(64.))
                                    .flex_none()
                                    .text_xs()
                                    .text_color(if is_current { themed(ACCENT) } else { themed(MUTED) })
                                    .child(fmt_duration(start)),
                            )
                            .child(div().flex_1().min_w_0().truncate().text_sm().text_color(themed(TEXT)).child(line))
                            .on_click_hinted(&this.hint_reg(), cx, move |this, _, _, cx| this.seek_link(start, cx))
                    })
                    .collect()
            }),
        )
        .track_scroll(self.transcript_scroll.clone())
        .flex_1()
        .into_any_element()
    }

    /// The Transcript tab's search field: filters the lines as you type; Esc clears.
    fn transcript_field(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let focused = self.transcript_focus.is_focused(window);
        let empty = self.transcript_query.is_empty();
        div()
            .id("transcript-search")
            .track_focus(&self.transcript_focus)
            .flex()
            .items_center()
            .gap_2()
            .min_w(px(160.))
            .px_2()
            .py(px(3.))
            .rounded_md()
            .bg(themed(HOVER))
            .border_1()
            .border_color(if focused { themed(MUTED) } else { themed(BORDER) })
            .text_xs()
            .cursor_text()
            .child(svg().path(icons::path("search")).size(px(12.)).flex_none().text_color(themed(MUTED)))
            .child(div().flex_1().min_w_0().truncate().text_color(if empty { themed(MUTED) } else { themed(TEXT) }).map(|d| match (empty, focused) {
                (_, true) => d.text_color(themed(TEXT)).child(self.caret_text("transcript", &self.transcript_query, "Search in this video")),
                (true, false) => d.child("Search in this video"),
                (false, false) => d.child(self.transcript_query.clone()),
            }))
            .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                window.focus(&this.transcript_focus);
                cx.notify();
            })
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match edit_text(&mut this.caret, "transcript", &mut this.transcript_query, ev, cx) {
                    Edit::Submit => window.blur(),
                    Edit::Cancel => {
                        this.transcript_query.clear();
                        this.transcript_shown = None;
                        window.blur();
                    }
                    // A new list: start at its top (or, emptied, at the playing line again).
                    Edit::Changed => {
                        this.transcript_shown = if this.transcript_query.trim().is_empty() { None } else { Some(0) };
                        this.transcript_scroll.scroll_to_item(0, ScrollStrategy::Top);
                    }
                    Edit::Moved | Edit::Ignored => {}
                }
                cx.stop_propagation();
                cx.notify();
            }))
    }

    /// Comments tab is on and there is a video to show them for.
    fn comments_on(&self) -> bool {
        self.settings.comments && self.current.is_some()
    }

    /// Comments of the current video, fetched on first view (and again after the video changes).
    fn render_comments(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let id = self.current.as_ref().map(|v| v.id.clone());
        if self.comments_for != id {
            self.comments_for = id;
            self.comments_limit = COMMENTS_PAGE;
            // Not the previous video's comments while these load.
            self.comments = Load::Idle;
            self.fetch_comments(cx);
        }
        if let Some(p) = self.placeholder(&self.comments, "No comments.", Rows::Comments) {
            return p;
        }
        // A full page suggests there are more (yt-dlp can't continue, so more means refetching).
        let loading = matches!(self.comments, Load::Loading(_));
        let more = loading || self.comments.items().len() >= self.comments_limit;
        div()
            .id("comments")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(self.comments.items().iter().enumerate().map(|(i, c)| {
                let meta = [Some(c.author.clone()), (!c.age.is_empty()).then(|| c.age.clone()), c.likes.filter(|n| *n > 0).map(|n| format!("{} likes", fmt_count(n)))]
                    .into_iter()
                    .flatten()
                    .chain(c.pinned.then(|| "pinned".to_string()))
                    .collect::<Vec<_>>()
                    .join(" · ");
                div()
                    .id(i)
                    .w_full()
                    .px_3()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .border_b_1()
                    .border_color(themed(BORDER))
                    .child(div().text_xs().text_color(themed(MUTED)).child(meta))
                    .child(div().text_sm().text_color(themed(TEXT)).child(self.linked_text(("comment-text", i), c.text.clone(), cx)))
            }))
            .when(more, |d| {
                d.child(
                    div().p_3().flex().justify_center().child(
                        div()
                            .id("comments-more")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .text_sm()
                            .text_color(themed(MUTED))
                            .when(!loading, |b| b.cursor_pointer().hover(|b| b.bg(themed(HOVER))))
                            .flex()
                            .items_center()
                            .gap_2()
                            .when(loading, |b| {
                                b.child(svg().path(icons::path("refresh")).size(px(14.)).text_color(themed(MUTED)).with_animation(
                                    "comments-spin",
                                    Animation::new(Duration::from_millis(900)).repeat(),
                                    |s, t| s.with_transformation(Transformation::rotate(radians(t * std::f32::consts::TAU))),
                                ))
                            })
                            .child(if loading { "Loading more comments…" } else { "Load more comments" })
                            .when(!loading, |b| {
                                b.on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                    this.comments_limit += COMMENTS_PAGE;
                                    this.fetch_comments(cx);
                                })
                            }),
                    ),
                )
            })
            .when(!more, |d| {
                let n = self.comments.items().len();
                d.child(div().p_3().flex().justify_center().text_xs().text_color(themed(MUTED)).child(format!("All {n} comments shown")))
            })
            .into_any_element()
    }

    /// Fetch the first `comments_limit` comments of the current video; what is shown stays until they arrive.
    fn fetch_comments(&mut self, cx: &mut Context<Self>) {
        let id = self.current.as_ref().map(|v| v.id.clone()).unwrap_or_default();
        let limit = self.comments_limit;
        self.fetch(cx, "comments", |s| &mut s.comments, None, move |cfg, on| yt::comments(cfg, &id, limit, on));
    }

    /// The playing video's chapters (empty when off in Settings or the video has none).
    fn chapter_list(&self) -> Arc<[(f64, String)]> {
        match &self.state {
            Some(s) if self.settings.chapters && self.casting.is_none() => s.chapters.clone().into(),
            _ => Arc::from(Vec::new()),
        }
    }

    /// Clickable chapters; the current one is highlighted and scrolled into view when it changes.
    fn render_chapters(&mut self, chapters: Arc<[(f64, String)]>, cx: &mut Context<Self>) -> AnyElement {
        let position = self.state.as_ref().map_or(0., |s| s.position);
        let current = chapters.iter().rposition(|(t, _)| *t <= position).unwrap_or(0);
        if self.chapter_shown != Some(current) {
            self.chapter_shown = Some(current);
            self.chapters_scroll.scroll_to_item(current, ScrollStrategy::Center);
        }
        uniform_list(
            "chapters",
            chapters.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|i| {
                        let (start, title) = &chapters[i];
                        let (start, is_current) = (*start, i == current);
                        let title = if title.is_empty() { format!("Chapter {}", i + 1) } else { title.clone() };
                        div()
                            .id(i)
                            .w_full()
                            .h(px(40.))
                            .px_3()
                            .flex()
                            .items_center()
                            .gap_3()
                            .cursor_pointer()
                            .when(is_current, |d| d.bg(themed(HOVER)))
                            .hover(|d| d.bg(themed(HOVER)))
                            .child(
                                div()
                                    .w(px(64.))
                                    .flex_none()
                                    .text_xs()
                                    .text_color(if is_current { themed(ACCENT) } else { themed(MUTED) })
                                    .child(fmt_duration(start)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .text_color(if is_current { themed(TEXT) } else { themed(MUTED) })
                                    .child(title),
                            )
                            .on_click_hinted(&this.hint_reg(), cx, move |this, _, _, cx| this.seek_to(start, cx))
                    })
                    .collect()
            }),
        )
        .track_scroll(self.chapters_scroll.clone())
        .flex_1()
        .into_any_element()
    }

    fn render_recs(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if !self.cfg.has_auth() && self.recs.items().is_empty() {
            return self.render_anon("recs", cx);
        }
        if let Some(p) = self.placeholder(&self.recs, "No recommendations.", Rows::Videos) {
            return p;
        }
        let current = self.current.as_ref().map(|c| c.id.clone());
        let v: Vec<Video> = self.recs.items().iter().filter(|r| Some(&r.id) != current.as_ref()).cloned().collect();
        self.video_list("recs", &v, None, cx)
    }
}

/// A video saved with Download: listed in the Downloads tab and played from its file.
#[derive(Clone, Serialize, serde::Deserialize)]
struct Saved {
    video: Video,
    path: String,
}

impl Saved {
    /// The library, without entries whose file was deleted or moved outside the app.
    fn load() -> Vec<Saved> {
        let mut list: Vec<Saved> = store::load_data("downloads").unwrap_or_default();
        list.retain(|s| std::path::Path::new(&s.path).exists());
        list
    }
}

/// A row of the player menu's subtitle list.
#[derive(Clone, PartialEq)]
enum SubChoice {
    Off,
    /// A caption track: its language code and whether it is generated.
    Lang(String, bool),
    /// A line that says something (loading, an error), not a choice.
    Info,
}

/// The sleep timer: pause at a time, or let the playing video end without starting another.
#[derive(Clone, Copy, PartialEq)]
enum Sleep {
    At(Instant),
    EndOfVideo,
}

#[derive(Clone)]
struct Download {
    progress: f32,
    result: Option<Result<String, String>>,
    /// When it finished; the result is shown for a few seconds, then hidden.
    done_at: Option<std::time::Instant>,
}

impl Download {
    /// Status line under the player: progress while running, the result briefly after.
    fn visible_status(&self) -> Option<String> {
        self.done_at.is_none_or(|t| t.elapsed() < Duration::from_secs(6)).then(|| self.status())
    }

    fn status(&self) -> String {
        match &self.result {
            None => format!("Downloading… {:.0}%", self.progress),
            Some(Ok(file)) => format!("Downloaded to {file}"),
            Some(Err(e)) => format!("Download failed: {e}"),
        }
    }
}

enum Thumb {
    Ready(PathBuf),
    /// Downloading: shown as a skeleton.
    Pending,
    /// No image, or the download failed.
    Missing,
}

/// Shape of skeleton rows for a list that hasn't loaded yet.
#[derive(Clone, Copy)]
enum Rows {
    Videos,
    Groups,
    Comments,
}

/// A click target for `f` hints: where it is on screen and what clicking it does.
type HintTarget = (GBounds<GPixels>, Rc<dyn Fn(&mut Unbloated, &mut Window, &mut Context<Unbloated>)>);

#[derive(Clone)]
struct HintReg {
    targets: Rc<RefCell<Vec<HintTarget>>>,
}

/// `on_click` that also registers the element for `f` hints and Tab focus, which run the same handler.
trait OnClickHinted: StatefulInteractiveElement + ParentElement + Styled + Sized {
    fn on_click_hinted(
        self,
        h: &HintReg,
        cx: &mut Context<Unbloated>,
        f: impl Fn(&mut Unbloated, &gpui::ClickEvent, &mut Window, &mut Context<Unbloated>) + 'static,
    ) -> Self {
        let f = Rc::new(f);
        let click = f.clone();
        let el = self.on_click(cx.listener(move |this, ev, window, cx| click(this, ev, window, cx)));
        let targets = h.targets.clone();
        let action: Rc<dyn Fn(&mut Unbloated, &mut Window, &mut Context<Unbloated>)> =
            Rc::new(move |this, window, cx| f(this, &gpui::ClickEvent::default(), window, cx));
        el.relative().child(
            canvas(move |bounds, _, _| targets.borrow_mut().push((bounds, action.clone())), |_, _, _, _| {})
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
        )
    }
}

impl OnClickHinted for Stateful<gpui::Div> {}

/// Letters used for hint labels (home row first).
const HINT_CHARS: &str = "asdfghjklqwertyuiopzxcvbnm";

/// Labels for `n` targets: single letters when they fit, else two letters for all (so no label
/// is a prefix of another).
fn hint_label_list(n: usize) -> Vec<String> {
    let chars: Vec<char> = HINT_CHARS.chars().collect();
    if n <= chars.len() {
        chars.iter().take(n).map(|c| c.to_string()).collect()
    } else {
        chars.iter().flat_map(|a| chars.iter().map(move |b| format!("{a}{b}"))).take(n).collect()
    }
}

/// Hint mode overlay: a yellow label on each target still matching what was typed.
fn hint_labels(targets: &[HintTarget], typed: &str) -> gpui::Div {
    let labels = hint_label_list(targets.len());
    div().absolute().top_0().left_0().size_full().children(targets.iter().zip(labels).filter(|(_, l)| l.starts_with(typed)).map(
        |((b, _), label)| {
            div()
                .absolute()
                .left(b.origin.x)
                .top(b.origin.y)
                .px_1()
                .rounded_sm()
                .bg(rgb(0xffd54a))
                .border_1()
                .border_color(rgb(0x8a6d00))
                .text_xs()
                .text_color(rgb(0x1a1a1a))
                .child(label.to_uppercase())
        },
    ))
}

/// Fade an element in and out, as a loading placeholder.
fn pulse(id: impl Into<ElementId>, el: gpui::Div) -> AnyElement {
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(1400)).repeat().with_easing(gpui::pulsating_between(0.35, 0.9)),
        |d, t| d.opacity(t),
    )
    .into_any_element()
}

/// Grey rows shaped like the real ones, pulsing while a list loads.
fn skeleton(rows: Rows) -> AnyElement {
    let bar = |w: f32, h: f32| div().w(relative(w)).h(px(h)).rounded_sm().bg(themed(HOVER));
    let widths = [0.7, 0.55, 0.8, 0.45, 0.65, 0.6, 0.75, 0.5];
    let list = div().flex().flex_col().children((0..12).map(|i| {
        let w = widths[i % widths.len()];
        match rows {
            Rows::Videos => div()
                .h(px(ROW_H))
                .px_3()
                .flex()
                .items_center()
                .gap_3()
                .child(div().w(px(96.)).h(px(54.)).flex_none().rounded(px(4.)).bg(themed(HOVER)))
                .child(div().flex_1().flex().flex_col().gap_2().child(bar(w, 12.)).child(bar(w * 0.45, 10.))),
            Rows::Comments => div().px_3().py_2().flex().flex_col().gap_2().child(bar(0.3, 10.)).child(bar(w, 12.)).child(bar(w * 0.6, 12.)),
            Rows::Groups => div()
                .h(px(44.))
                .px_3()
                .flex()
                .items_center()
                .gap_3()
                .child(div().size(px(28.)).flex_none().rounded_full().bg(themed(HOVER)))
                .child(div().flex_1().child(bar(w * 0.5, 12.))),
        }
    }));
    pulse("skeleton", list)
}

/// Indeterminate progress: a short accent bar sweeping left to right.
fn loading_bar(id: &'static str) -> impl IntoElement {
    div().relative().w_full().h(px(2.)).overflow_hidden().child(
        div()
            .absolute()
            .top_0()
            .h_full()
            .w(relative(0.3))
            .bg(themed(ACCENT))
            .with_animation(id, Animation::new(Duration::from_millis(1200)).repeat(), |d, t| {
                d.left(relative(t * 1.3 - 0.3))
            }),
    )
}

/// A draggable 5px bar between two panes.
/// A list that failed (most often without a login) may load again after the login changed.
fn clear_failed<T>(load: &mut Load<T>) {
    if matches!(load, Load::Failed(_)) {
        *load = Load::Idle;
    }
}

/// Friendlier wording for a session YouTube rejected: the browser cookies expired.
/// Whether a list's error reads like a login problem (and not, say, a network one).
fn looks_like_login_error(error: &str) -> bool {
    let e = error.to_lowercase();
    ["sign in", "log in", "login", "cookie", "unauthorized", "403"].iter().any(|p| e.contains(p))
}

fn session_hint(error: String) -> String {
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

fn divider(id: &'static str, split: Split, cx: &mut Context<Unbloated>) -> Stateful<gpui::Div> {
    let bar = div().id(id).flex_none().bg(themed(BORDER)).hover(|d| d.bg(themed(MUTED)));
    let bar = match split {
        Split::Columns => bar.w(px(5.)).h_full().cursor(CursorStyle::ResizeLeftRight),
        Split::Player | Split::Continue => bar.h(px(5.)).w_full().cursor(CursorStyle::ResizeUpDown),
    };
    bar.on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, e: &gpui::MouseDownEvent, _, _| {
            this.dragging = Some(split);
            this.drag_from = (f32::from(e.position.y), this.settings.continue_height);
        }),
    )
}

enum Edit {
    Changed,
    /// Only the cursor or selection moved.
    Moved,
    Submit,
    Cancel,
    Ignored,
}

/// Minimal one-line text editing: type (any layout), Backspace, Ctrl+Backspace, Ctrl+V, Enter, Esc.
/// Where the text cursor is, in the one text field being edited (`id`), and where a
/// selection started (`anchor`; equal to `pos` when nothing is selected). Byte offsets.
#[derive(Default)]
struct Caret {
    id: &'static str,
    pos: usize,
    anchor: usize,
}

impl Caret {
    /// (pos, anchor) in field `id`; a field the caret wasn't in starts with it at the end.
    fn get(&self, id: &'static str, text: &str) -> (usize, usize) {
        let fix = |p: usize| if p <= text.len() && text.is_char_boundary(p) { p } else { text.len() };
        if self.id == id { (fix(self.pos), fix(self.anchor)) } else { (text.len(), text.len()) }
    }
}

fn prev_char(t: &str, p: usize) -> usize {
    t[..p].char_indices().next_back().map_or(0, |(i, _)| i)
}

fn next_char(t: &str, p: usize) -> usize {
    t[p..].chars().next().map_or(p, |c| p + c.len_utf8())
}

fn prev_word(t: &str, p: usize) -> usize {
    t[..p].trim_end_matches(' ').rfind(' ').map_or(0, |i| i + 1)
}

fn next_word(t: &str, p: usize) -> usize {
    let rest = &t[p..];
    let word = rest.trim_start_matches(' ');
    p + (rest.len() - word.len()) + word.find(' ').unwrap_or(word.len())
}

/// Shared key handling for text fields: typing, Home/End, arrows (Ctrl: by word), Shift to
/// select, Ctrl+A/C/X/V, Backspace/Delete.
fn edit_text(caret: &mut Caret, id: &'static str, text: &mut String, ev: &KeyDownEvent, cx: &App) -> Edit {
    let k = &ev.keystroke;
    let m = &k.modifiers;
    let (mut pos, mut anchor) = caret.get(id, text);
    let (start, end) = (pos.min(anchor), pos.max(anchor));
    let selected = start != end;
    let mut edit = Edit::Changed;
    // Move the cursor; with Shift, the selection's other end stays.
    let mut move_to = |to: usize, pos: &mut usize, anchor: &mut usize| {
        *pos = to;
        if !m.shift {
            *anchor = to;
        }
        edit = Edit::Moved;
    };
    match k.key.as_str() {
        "enter" => return Edit::Submit,
        "escape" => return Edit::Cancel,
        "home" => move_to(0, &mut pos, &mut anchor),
        "end" => move_to(text.len(), &mut pos, &mut anchor),
        "left" if selected && !m.shift => move_to(start, &mut pos, &mut anchor),
        "right" if selected && !m.shift => move_to(end, &mut pos, &mut anchor),
        "left" => move_to(if m.control { prev_word(text, pos) } else { prev_char(text, pos) }, &mut pos, &mut anchor),
        "right" => move_to(if m.control { next_word(text, pos) } else { next_char(text, pos) }, &mut pos, &mut anchor),
        "a" if m.control => {
            (anchor, pos) = (0, text.len());
            edit = Edit::Moved;
        }
        "c" if m.control => {
            if selected {
                cx.write_to_clipboard(ClipboardItem::new_string(text[start..end].to_string()));
            }
            edit = Edit::Moved;
        }
        "x" if m.control && selected => {
            cx.write_to_clipboard(ClipboardItem::new_string(text[start..end].to_string()));
            text.replace_range(start..end, "");
            (pos, anchor) = (start, start);
        }
        "backspace" | "delete" if selected => {
            text.replace_range(start..end, "");
            (pos, anchor) = (start, start);
        }
        "backspace" => {
            let from = if m.control { prev_word(text, pos) } else { prev_char(text, pos) };
            text.replace_range(from..pos, "");
            (pos, anchor) = (from, from);
        }
        "delete" => {
            let to = if m.control { next_word(text, pos) } else { next_char(text, pos) };
            text.replace_range(pos..to, "");
        }
        key => {
            let typed = if key == "v" && m.control {
                cx.read_from_clipboard().and_then(|c| c.text()).map(|c| c.lines().next().unwrap_or_default().to_string())
            } else if !(m.control || m.alt || m.platform) {
                k.key_char.clone()
            } else {
                None
            };
            let Some(typed) = typed else { return Edit::Ignored };
            text.replace_range(start..end, &typed);
            pos = start + typed.len();
            anchor = pos;
        }
    }
    *caret = Caret { id, pos, anchor };
    edit
}

/// Hover tooltip content.
/// `left`: opens to the left of the pointer, for things at the left column's right edge,
/// where a tooltip would run under the (always on top) video.
struct Tip(SharedString, bool);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let tip = div()
            .whitespace_nowrap()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(themed(HOVER))
            .border_1()
            .border_color(themed(BORDER))
            .text_xs()
            .text_color(themed(TEXT))
            .child(self.0.clone());
        if self.1 {
            // A zero-width anchor at the pointer, with the tooltip hanging off to its left.
            div().relative().w_0().child(tip.absolute().right(px(8.)).top_0())
        } else {
            tip
        }
    }
}

fn tip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    let text = text.into();
    move |_, cx| cx.new(|_| Tip(text.clone(), false)).into()
}

fn tip_left(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    let text = text.into();
    move |_, cx| cx.new(|_| Tip(text.clone(), true)).into()
}

/// Square icon button with a hover tooltip; `enabled: false` greys it out (add clicks only when enabled).
/// The red "LIVE" tag on live streams (small on thumbnails, `big` beside the playing video).
fn live_badge(big: bool) -> gpui::Div {
    div()
        .flex_none()
        .px(px(if big { 6. } else { 4. }))
        .py(px(1.))
        .rounded(px(3.))
        .bg(themed(ACCENT))
        .text_color(themed(ON_ACCENT))
        .text_size(px(if big { 11. } else { 9. }))
        .font_weight(gpui::FontWeight::BOLD)
        .child("LIVE")
}

fn icon_button(
    id: impl Into<ElementId>,
    icon: &str,
    tooltip: impl Into<SharedString>,
    enabled: bool,
) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .mr_2()
        .p(px(7.))
        .rounded_md()
        .bg(themed(HOVER))
        .child(svg().path(icons::path(icon)).size(px(16.)).text_color(if enabled { themed(TEXT) } else { themed(BORDER) }))
        .when(enabled, |d| d.cursor_pointer().hover(|d| d.bg(themed(BORDER))))
        .tooltip(tip(tooltip))
}

/// First visible row and how many rows fit, for a uniform list of `row_h`-tall rows. (GPUI's own
/// `logical_scroll_top` always says 0 for uniform lists.) `fallback_h` is used until the list has
/// been laid out once.
fn scroll_window(handle: &UniformListScrollHandle, row_h: f32, fallback_h: f32) -> (usize, usize) {
    let state = handle.0.borrow();
    let top = (f32::from(-state.base_handle.offset().y) / row_h).max(0.).floor() as usize;
    let h = f32::from(state.base_handle.bounds().size.height);
    let visible = (if h > 0. { h } else { fallback_h } / row_h).floor().max(1.) as usize;
    (top, visible)
}

fn tab_button(label: impl Into<SharedString>, active: bool) -> Stateful<gpui::Div> {
    let label: SharedString = label.into();
    // Id from the label's first word, so "Up next (3)" keeps one id as its count changes.
    let id = SharedString::from(label.split(' ').next().unwrap_or_default().to_string());
    div()
        .id(ElementId::Name(id))
        .px_3()
        .py_2()
        .text_sm()
        .cursor_pointer()
        .border_b_2()
        .border_color(if active { themed(ACCENT).into() } else { Hsla::transparent_black() })
        .text_color(if active { themed(TEXT) } else { themed(MUTED) })
        .hover(|d| d.text_color(themed(TEXT)))
        .child(label)
}

impl Render for Unbloated {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let (true, Some(video)) = (self.fullscreen, self.current.clone()) {
            // Only the video; f / Esc (here, or in mpv with its hotkeys on) and a double click leave fullscreen.
            if window.focused(cx).is_none() {
                window.focus(&self.root_focus);
            }
            return div()
                .size_full()
                .bg(gpui::black())
                .track_focus(&self.root_focus)
                .on_key_down(cx.listener(Self::shortcut))
                .child(self.screen(&video, true, cx));
        }
        let st = &self.settings;
        let auth = self.cfg.has_auth();
        let tabs: Vec<_> = [
            (auth && st.subscriptions, "Subscriptions", Tab::Subscriptions),
            (auth && st.playlists, "Playlists", Tab::Playlists),
            (auth && st.history, "History", Tab::History),
            (auth && self.downloads_on(), "Downloads", Tab::Downloads),
        ]
        .into_iter()
        .filter_map(|(on, label, tab)| on.then_some((label, tab)))
        .collect();
        let header_icon = |id: &'static str, icon: &'static str| {
            div()
                .id(id)
                .flex_none()
                .p(px(7.))
                .rounded_md()
                .cursor_pointer()
                .hover(|d| d.bg(themed(HOVER)))
                .child(svg().path(icons::path(icon)).size(px(16.)).text_color(themed(MUTED)))
        };
        let header = if self.searching {
            // Search mode: ← back, a full-width field, ✕ to clear.
            let focused = self.search_focus.is_focused(window);
            let field = div()
                .id("search")
                .track_focus(&self.search_focus)
                .flex_1()
                .min_w_0()
                .mx_1()
                .px_3()
                .py_1()
                .rounded_md()
                .bg(themed(HOVER))
                .border_1()
                .border_color(if focused { themed(MUTED) } else { themed(BORDER) })
                .text_sm()
                .truncate()
                .cursor_text()
                .map(|d| match (self.query.is_empty(), focused) {
                    (_, true) => d.text_color(themed(TEXT)).child(self.caret_text("search", &self.query, "Search YouTube")),
                    (true, false) => d.text_color(themed(MUTED)).child("Search YouTube, or paste a link"),
                    (false, false) => d.text_color(themed(TEXT)).child(self.query.clone()),
                })
                .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                    window.focus(&this.search_focus);
                    cx.notify();
                })
                .on_key_down(cx.listener(Self::search_key));
            div()
                .flex()
                .items_center()
                .px_2()
                .py(px(5.))
                .border_b_1()
                .border_color(themed(BORDER))
                .child(
                    header_icon("search-back", "arrow-left")
                        .tooltip(tip("Back (Esc)"))
                        .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| this.close_search(window, cx)),
                )
                .child(field)
                .when(!self.query.is_empty(), |d| {
                    d.child(
                        header_icon("search-clear", "close")
                            .tooltip(tip_left("Clear"))
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                                this.query.clear();
                                window.focus(&this.search_focus);
                                cx.notify();
                            }),
                    )
                })
        } else {
            div()
            .flex()
            .items_center()
            .px_2()
            .border_b_1()
            .border_color(themed(BORDER))
            .children(tabs.into_iter().map(|(label, tab)| {
                tab_button(label, self.tab == tab).on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.select_tab(tab, cx))
            }))
            // Logged out: the account tabs are replaced by the home list and one way in.
            .when(!auth, |d| {
                d.child(
                    tab_button("Home", !matches!(self.tab, Tab::Settings | Tab::Downloads))
                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.select_tab(Tab::Subscriptions, cx)),
                )
                .when(self.downloads_on(), |d| {
                    d.child(
                        tab_button("Downloads", self.tab == Tab::Downloads)
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.select_tab(Tab::Downloads, cx)),
                    )
                })
                .child(
                    tab_button("Sign in", self.tab == Tab::Settings)
                        .tooltip(tip_left("Connect your YouTube account (Settings)"))
                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.select_tab(Tab::Settings, cx)),
                )
            })
            .child(div().flex_1())
            .child(
                header_icon("search-open", "search")
                    .tooltip(tip_left("Search (/)"))
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| this.open_search(window, cx)),
            )
            .child({
                let icon = svg().path(icons::path("refresh")).size(px(16.)).text_color(themed(MUTED));
                let icon = if self.tab_loading() {
                    // Spins while the current list loads.
                    icon.with_animation("refresh-spin", Animation::new(Duration::from_millis(900)).repeat(), |s, t| {
                        s.with_transformation(Transformation::rotate(radians(t * std::f32::consts::TAU)))
                    })
                    .into_any_element()
                } else {
                    icon.into_any_element()
                };
                div()
                    .id("refresh")
                    .ml_2()
                    .p(px(7.))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|d| d.bg(themed(HOVER)))
                    .child(icon)
                    .tooltip(tip_left("Refresh"))
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.load_tab(cx))
            })
            .child(
                div()
                    .id("settings")
                    .ml_1()
                    .px(px(7.))
                    .py(px(9.))
                    .cursor_pointer()
                    .border_b_2()
                    .border_color(if self.tab == Tab::Settings { themed(ACCENT).into() } else { Hsla::transparent_black() })
                    .child(svg().path(icons::path("settings")).size(px(16.)).text_color(if self.tab == Tab::Settings {
                        themed(TEXT)
                    } else {
                        themed(MUTED)
                    }))
                    .tooltip(tip_left("Settings"))
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.select_tab(Tab::Settings, cx)),
            )
        };

        let header = header.h(px(HEADER_H)).flex_none();
        let header_bar = if self.tab_loading() {
            loading_bar("list-loading").into_any_element()
        } else {
            div().h(px(2.)).into_any_element()
        };
        let left = div()
            .flex()
            .flex_col()
            .when(self.right_collapsed, |d| d.flex_1())
            .when(!self.right_collapsed, |d| d.w(relative(self.settings.split)).flex_none())
            .min_w_0()
            .child(header)
            .child(header_bar)
            .child(div().flex().flex_col().flex_1().min_h_0().child(self.render_list(window, cx)))
;

        let player = div().flex().flex_col().min_h_0().child(self.render_player(cx));
        let right = div().relative().flex().flex_col().flex_1().min_w_0().bg(themed(PANEL));
        let right = if self.settings.window_buttons {
            let control = |id: &'static str, icon: &'static str, tooltip: &'static str, hover: u32, color: u32| {
                div()
                    .id(id)
                    .p(px(7.))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |d| d.bg(themed(hover)))
                    .child(svg().path(icons::path(icon)).size(px(14.)).text_color(themed(color)))
                    .tooltip(tip(tooltip))
            };
            right.child(
                div()
                    .flex()
                    .justify_end()
                    .items_center()
                    .h(px(HEADER_H))
                    .flex_none()
                    .px_2()
                    .border_b_1()
                    .border_color(themed(BORDER))
                    .child(control("win-min", "minimize", "Minimize", HOVER, MUTED).on_click(|_, window, _| window.minimize_window()))
                    .child(control("win-max", "maximize", "Maximize", HOVER, MUTED).on_click(|_, window, _| window.zoom_window()))
                    .child(control("win-close", "close", "Close", ACCENT, TEXT).on_click(cx.listener(|this, _, window, cx| this.request_close(window, cx)))),
            )
        } else {
            right
        };
        let show_recs = self.settings.recommendations;
        let chapters = self.chapter_list();
        let show_comments = self.comments_on();
        let show_desc = self.description_on();
        let show_transcript = self.transcript_on();
        let show_later = self.watch_later_on();
        let right = if (show_recs || !self.up_next.is_empty() || !chapters.is_empty() || show_comments || show_desc || show_transcript || show_later) && !self.player_full {
            let lower = match self.lower {
                Lower::Description if show_desc => Lower::Description,
                Lower::Transcript if show_transcript => Lower::Transcript,
                Lower::Comments if show_comments => Lower::Comments,
                Lower::WatchLater if show_later => Lower::WatchLater,
                Lower::Chapters if !chapters.is_empty() => Lower::Chapters,
                Lower::Chapters | Lower::Comments | Lower::WatchLater | Lower::Recommended | Lower::Description | Lower::Transcript if show_recs => Lower::Recommended,
                _ => Lower::UpNext,
            };
            let body = match lower {
                Lower::Chapters => self.render_chapters(chapters.clone(), cx),
                Lower::Description => self.render_description(cx),
                Lower::Transcript => self.render_transcript(window, cx),
                Lower::Comments => self.render_comments(cx),
                Lower::WatchLater => self.render_watch_later(cx),
                Lower::Recommended => self.render_recs(cx),
                Lower::UpNext if self.up_next.is_empty() => self.status("Nothing queued. Hover a video and press + to add it."),
                Lower::UpNext => {
                let list = self.video_list("up-next", &self.up_next.clone(), None, cx);
                if self.cast_available() && !self.up_next.is_empty() {
                    let n = self.up_next.len();
                    div()
                        .flex()
                        .flex_col()
                        .size_full()
                        .child(
                            div().flex().flex_none().px_3().py_1().child(
                                self.chip("up-next-cast", format!("Cast all {n}"), true).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                    let videos = this.up_next.clone();
                                    this.cast_list(None, videos, cx);
                                }),
                            ),
                        )
                        .child(div().flex().flex_col().flex_1().min_h_0().child(list))
                        .into_any_element()
                } else {
                    list
                }
            }
            };
            right
                .child(player.h(if self.lower_full { px(0.).into() } else { relative(self.settings.player) }).overflow_hidden().flex_none())
                .child(divider("split-player", Split::Player, cx))
                .child(
                    div()
                        .flex()
                        .px_2()
                        .border_b_1()
                        .border_color(themed(BORDER))
                        .when(show_recs, |d| {
                            d.child(
                                tab_button("Recommended", lower == Lower::Recommended).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                    this.show_lower(Lower::Recommended, cx);
                                }),
                            )
                        })
                        .when(show_desc, |d| {
                            d.child(tab_button("Description", lower == Lower::Description).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                this.show_lower(Lower::Description, cx);
                            }))
                        })
                        .when(show_transcript, |d| {
                            d.child(tab_button("Transcript", lower == Lower::Transcript).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                this.show_lower(Lower::Transcript, cx);
                            }))
                        })
                        .when(!chapters.is_empty(), |d| {
                            d.child(
                                tab_button(format!("Chapters ({})", chapters.len()), lower == Lower::Chapters).on_click_hinted(
                                    &self.hint_reg(),
                                    cx,
                                    |this, _, _, cx| {
                                        this.show_lower(Lower::Chapters, cx);
                                    },
                                ),
                            )
                        })
                        .when(show_later, |d| {
                            let label = match &self.watch_later {
                                Load::Ready(v) if !v.is_empty() => format!("Watch later ({})", v.len()),
                                _ => "Watch later".to_string(),
                            };
                            d.child(tab_button(label, lower == Lower::WatchLater).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                this.show_lower(Lower::WatchLater, cx);
                            }))
                        })
                        .when(show_comments, |d| {
                            d.child(tab_button("Comments", lower == Lower::Comments).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                this.show_lower(Lower::Comments, cx);
                            }))
                        })
                        .child(
                            tab_button(
                                if self.up_next.is_empty() { "Up next".to_string() } else { format!("Up next ({})", self.up_next.len()) },
                                lower == Lower::UpNext,
                            )
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                this.show_lower(Lower::UpNext, cx);
                            }),
                        ),
                )
                .child(div().flex().flex_col().flex_1().min_h_0().child(body))
        } else {
            right.child(player.flex_1())
        };
        let right = if self.saving { right.child(self.render_save_overlay(window, cx)) } else { right };

        // Nothing focused (e.g. after leaving a text field): give focus back to the root so
        // keyboard shortcuts keep working.
        if window.focused(cx).is_none() {
            window.focus(&self.root_focus);
        }
        let key = self.left_key();
        if key != self.vim_list {
            // Remember where we were in the old list; come back to the new list's last spot.
            self.vim_positions.insert(std::mem::take(&mut self.vim_list), self.vim_cursor);
            self.vim_cursor = self.vim_positions.get(&key).copied().unwrap_or(0);
            self.vim_list = key;
            self.list_filter.clear();
            self.vim_scroll.scroll_to_item(self.vim_cursor, ScrollStrategy::Center);
        }
        // The ring follows its element if the layout moved it since the last frame.
        if self.kb_focus.is_some() {
            let targets = self.kb_targets(window);
            self.kb_focus = self.kb_current(&targets).map(|i| targets[i].0).or(self.kb_focus);
        }
        let kb_ring = self.kb_focus.map(|b| {
            div()
                .absolute()
                .left(b.origin.x - px(2.))
                .top(b.origin.y - px(2.))
                .w(b.size.width + px(4.))
                .h(b.size.height + px(4.))
                .rounded_md()
                .border_2()
                .border_color(themed(ACCENT))
        });
        // Collected again during this frame's paint (see hint_target).
        self.hint_targets.borrow_mut().clear();
        let hint_overlay = self.hints.as_ref().map(|(targets, typed)| hint_labels(targets, typed));
        let sheet = self.show_keys.then(|| self.render_cheatsheet(cx));
        let welcome = self.welcome_visible().then(|| self.render_welcome(cx));
        div()
            .size_full()
            .relative()
            .flex()
            .track_focus(&self.root_focus)
            .on_key_down(cx.listener(Self::shortcut))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.kb_focus.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .bg(themed(BG))
            .text_color(themed(TEXT))
            .children((!self.left_collapsed).then_some(left))
            .child(divider("split-columns", Split::Columns, cx))
            .children((!self.right_collapsed).then_some(right))
            .children(kb_ring)
            .children(self.render_channel_menu(window, cx))
            .children(self.render_video_menu(window, cx))
            .children(self.render_playlist_menu(window, cx))
            .children(self.render_quit_prompt(cx))
            .children(self.render_toasts())
            .children(sheet)
            .children(hint_overlay)
            .children(welcome)
            // While dragging, X keeps sending us pointer events even over mpv's window.
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                let Some(split) = this.dragging else { return };
                let size = window.viewport_size();
                match split {
                    Split::Columns => {
                        // Dragged to an edge: hide that column, keeping the width for later.
                        let at = e.position.x / size.width;
                        this.left_collapsed = at < 0.08;
                        this.right_collapsed = at > 0.92;
                        if !this.left_collapsed && !this.right_collapsed {
                            this.settings.split = at.clamp(0.2, 0.8);
                        }
                        this.sync_embed();
                    }
                    Split::Player => {
                        this.settings.player = (e.position.y / size.height).clamp(0.02, 0.9);
                        this.lower_full = false;
                        this.player_full = false;
                    }
                    Split::Continue => {
                        let (y, h) = this.drag_from;
                        let max = (f32::from(size.height) - 250.).max(ROW_H);
                        this.settings.continue_height = (h + f32::from(e.position.y) - y).clamp(ROW_H, max);
                    }
                }
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    if this.dragging.take().is_some() {
                        this.settings.save();
                    }
                }),
            )
    }
}

/// Print a command's answer and exit with its status.
fn finish(reply: cli::Reply) -> ! {
    match reply {
        Ok(message) => {
            println!("{message}");
            std::process::exit(0)
        }
        Err(error) => {
            eprintln!("unbloatedtube: {error}");
            std::process::exit(1)
        }
    }
}

fn main() {
    // mpv runs this program as its yt-dlp (see prefetch.rs); nothing else of the app starts then.
    if std::env::var_os(prefetch::ENV).is_some() {
        prefetch::run_shim();
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let start = match cli::parse(&args) {
        Ok(cli::Parsed::Print(text)) => return println!("{text}"),
        Ok(cli::Parsed::Run(command)) => match cli::send(&command) {
            Ok(reply) => finish(reply),
            Err(_) => {
                eprintln!("unbloatedtube: not running");
                std::process::exit(1);
            }
        },
        // A link goes to the running app; only without one does it start the app.
        Ok(cli::Parsed::Launch(Some(command))) => match cli::send(&command) {
            Ok(reply) => finish(reply),
            Err(_) => {
                if let cli::Command::Open { url } = &command {
                    if yt::parse_link(url).is_none() {
                        finish(Err(format!("not a YouTube link: {url}")));
                    }
                }
                Some(command)
            }
        },
        Ok(cli::Parsed::Launch(None)) => None,
        Err(message) => {
            eprintln!("unbloatedtube: {message}");
            std::process::exit(2);
        }
    };
    store::migrate_old_dirs();
    std::thread::spawn(prefetch::sweep);
    prefetch::start_helper();
    std::thread::spawn(|| account::refresh_kept(&Config::load()));
    Application::new().with_assets(icons::Assets).run(move |cx: &mut App| {
        // At 1280x800, but no bigger than the screen: with display scaling that can be larger than it,
        // and the bottom of the window (and anything there) would be off-screen.
        let want = size(px(1280.), px(800.));
        let fit = cx.primary_display().map_or(want, |d| {
            let screen = d.bounds().size;
            size(px(f32::from(want.width).min(f32::from(screen.width) - 40.)), px(f32::from(want.height).min(f32::from(screen.height) - 80.)))
        });
        let bounds = Bounds::centered(None, fit, cx);
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("unbloatedtube".into()),
                titlebar: Some(TitlebarOptions { title: Some("UnbloatedTube".into()), ..Default::default() }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Unbloated::new(window, cx)),
        )
        .unwrap();
        // The link or command the app was started with.
        if let Some(command) = start {
            window.update(cx, |this, window, cx| this.run_command(command, window, cx)).ok();
        }
        // Commands from other command lines; the socket is only kept while this window lives.
        if let Some(requests) = cli::listen() {
            cx.spawn(async move |cx| {
                loop {
                    cx.background_executor().timer(Duration::from_millis(100)).await;
                    while let Ok(request) = requests.try_recv() {
                        let reply = window
                            .update(cx, |this, window, cx| this.run_command(request.command, window, cx))
                            .unwrap_or_else(|_| Err("the window is closed".into()));
                        let _ = request.reply.send(reply);
                    }
                }
            })
            .detach();
            cx.on_app_quit(|_| async { cli::cleanup() }).detach();
        }
        // Media keys, the desktop's player widget and playerctl (MPRIS, see mpris.rs).
        let mpris = mpris::Mpris::start();
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(100)).await;
                let alive = window.update(cx, |this, window, cx| {
                    for command in mpris.commands() {
                        this.run_mpris(command, window, cx);
                    }
                    let (now, position) = this.mpris_snapshot();
                    mpris.update(now, position);
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.on_app_quit(|_| async { prefetch::cleanup() }).detach();
        cx.on_window_closed(|cx| cx.quit()).detach();
    });
}
