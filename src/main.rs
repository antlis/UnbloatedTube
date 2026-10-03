mod account;
mod embed;
mod icons;
mod player;
mod store;
mod thumbs;
mod yt;

use account::{Account, VideoStatus};
use embed::Embed;
use gpui::{
    Animation, AnimationExt, Pixels, AnyElement, App, Application, ClipboardItem, Bounds, Context, CursorStyle, ElementId, FocusHandle, Hsla, KeyDownEvent, MouseButton,
    MouseMoveEvent, ObjectFit, SharedString, Stateful, Task,
    Transformation, TitlebarOptions, Window, WindowBounds, WindowOptions, canvas, div, img, prelude::*, px, radians, relative, rgb, size, svg,
    uniform_list, ScrollStrategy, UniformListScrollHandle, Bounds as GBounds, Pixels as GPixels,
};
use player::Player;
use serde::{Serialize, de::DeserializeOwned};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use store::{ChannelFlags, ChannelGroup, Config, History, Seen, Settings};
use yt::{Group, Video, fmt_duration};

const BG: u32 = 0x0f0f0f;
const PANEL: u32 = 0x161616;
const HOVER: u32 = 0x242424;
const BORDER: u32 = 0x2a2a2a;
const TEXT: u32 = 0xe6e6e6;
const MUTED: u32 = 0x8c8c8c;
const ACCENT: u32 = 0xff4e45;
const ROW_H: f32 = 64.;
const SEEK_SEGMENTS: usize = 80;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Tab {
    Subscriptions,
    Playlists,
    History,
    Search,
    Settings,
}

/// A split being dragged: the column divider, or the one between player and recommendations.
#[derive(Clone, Copy)]
enum Split {
    Columns,
    Player,
}

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
    Some(Group { id, title, url: format!("{}/videos", url.trim_end_matches('/')), thumb: None })
}

/// Channel id (UC…) of a video, from its channel URL.
fn channel_of(v: &Video) -> Option<&str> {
    v.channel_url.as_deref()?.split("/channel/").nth(1).map(|id| id.trim_end_matches('/'))
}

/// Settings page toggles: label, hint, field.
type Toggle = (&'static str, &'static str, fn(&mut Settings) -> &mut bool);

const TOGGLES: [Toggle; 7] = [
    ("Subscriptions", "Your subscribed channels", |s| &mut s.subscriptions),
    ("Playlists", "Watch later, Liked and your playlists", |s| &mut s.playlists),
    ("History", "What you watched, here and on YouTube", |s| &mut s.history),
    ("Recommendations", "Your YouTube home feed under the player", |s| &mut s.recommendations),
    ("Shorts", "Shorts tab on channels, and Shorts in feeds and search", |s| &mut s.shorts),
    ("Vim mode", "j/k move, Enter opens, h goes back, f shows click hints; ? lists all keys", |s| &mut s.vim),
    ("Window buttons", "Minimize, maximize and close, top right", |s| &mut s.window_buttons),
];

const PLAYER_TOGGLES: [Toggle; 5] = [
    ("Autoplay next", "Play the next video of the list when one ends", |s| &mut s.autoplay),
    ("Audio only", "Don't fetch or show video, e.g. for music and podcasts", |s| &mut s.audio_only),
    ("Prefer hardware-friendly codecs", "Skip AV1, which many GPUs can't decode, for lower CPU use", |s| &mut s.prefer_hw_codecs),
    ("Hardware decoding", "Decode video on the GPU (mpv --hwdec=auto-safe)", |s| &mut s.hwdec),
    ("Block in-video ads (SponsorBlock)", "Skip sponsor reads and other segments marked by the community", |s| &mut s.sponsorblock),
];

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
const SPEEDS: [f32; 6] = [0.75, 1.0, 1.25, 1.5, 1.75, 2.0];

/// Settings text fields: label, hint, field.
const TEXT_FIELDS: [(&str, &str, fn(&mut Settings) -> &mut String); 3] = [
    ("Subtitles", "Language code, e.g. en or ru; empty for none", |s| &mut s.sub_lang),
    ("Extra mpv options", "e.g. --volume=70 --deband", |s| &mut s.mpv_args),
    ("Download folder", "Empty for your Downloads folder; ~/ works", |s| &mut s.download_dir),
];

const BUTTON_TOGGLES: [Toggle; 6] = [
    ("Subscribe", "Subscribe / unsubscribe to the video's channel", |s| &mut s.subscribe_button),
    ("Save to playlist", "Add the video to Watch later or one of your playlists", |s| &mut s.save_button),
    ("Like", "Like the video, or remove your like", |s| &mut s.like_button),
    ("Dislike", "Dislike the video, or remove your dislike", |s| &mut s.dislike_button),
    ("Share", "Copy the video's link", |s| &mut s.share_button),
    ("Download", "Save the video to your Downloads folder", |s| &mut s.download_button),
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
    UpNext,
}

/// Keyboard shortcuts (also listed in Settings). Keys reach mpv instead while the pointer is
/// over the video; mpv's own defaults there are similar (Space, arrows, f).
const SHORTCUTS: [(&str, &str); 12] = [
    ("Space / K", "Play / pause"),
    ("← / →", "Back / forward 5 seconds"),
    ("J / L", "Back / forward 10 seconds"),
    ("F", "Fullscreen (Esc or f to leave)"),
    ("M", "Mute"),
    ("Click / Double-click", "Pause / fullscreen (on the video)"),
    ("N", "Next (Up next first)"),
    ("P", "Previous"),
    ("/", "Search"),
    ("?", "Show these shortcuts"),
    ("Esc", "Close the playlist picker, or go back from a channel"),
    ("Esc", "Close this sheet"),
];

/// Cheatsheet columns: title and how many SHORTCUTS entries it takes, in order.
const SHEET_GROUPS: [(&str, usize); 2] = [("Playback", 6), ("Navigation", 6)];

/// Vim mode's keys (case matters: ⇧ means Shift).
const VIM_SHORTCUTS: [(&str, &str); 18] = [
    ("Space", "Play / pause"),
    ("← / →", "Back / forward 5 seconds"),
    (", / .", "Back / forward 10 seconds"),
    ("⇧F", "Fullscreen (Esc or f to leave)"),
    ("m", "Mute"),
    ("n / p", "Next / previous video"),
    ("j / k", "Move down / up the list"),
    ("g g / ⇧G", "First / last item"),
    ("Ctrl-d / Ctrl-u", "Move 10 down / up"),
    ("Enter / l", "Open or play"),
    ("h / Backspace", "Back"),
    ("⇧H / ⇧L", "Previous / next tab"),
    ("x", "Add the selected video to Up next"),
    ("f", "Click hints: type the label to click"),
    ("/", "Search"),
    ("?", "Show these shortcuts"),
    ("Esc", "Cancel / close / back"),
    ("Click / Double-click", "Pause / fullscreen (on the video)"),
];
const VIM_SHEET_GROUPS: [(&str, usize); 2] = [("Playback", 6), ("Navigation", 12)];

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

struct Unbloated {
    cfg: Arc<Config>,
    settings: Settings,
    dragging: Option<Split>,
    tab: Tab,
    subs: Browser,
    yt_history: Load<Video>,
    playlists: Browser,
    recs: Load<Video>,
    /// Latest uploads across subscriptions, for the "New uploads" row and per-channel counts.
    feed: Load<Video>,
    seen: Seen,
    query: String,
    search: Load<Video>,
    search_focus: FocusHandle,
    save_filter: String,
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
    /// First "g" of "gg" was pressed.
    vim_g: bool,
    /// Selection per list, restored when going back to it.
    vim_positions: HashMap<String, usize>,
    /// Clickable elements and their click actions, collected every frame (Vim mode) for `f`.
    hint_targets: Rc<RefCell<Vec<HintTarget>>>,
    /// Hint mode: the targets when `f` was pressed, and the letters typed so far.
    hints: Option<(Vec<HintTarget>, String)>,
    /// Picture-in-picture: mpv plays in its own small always-on-top window.
    pip: bool,
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
    /// The keyboard shortcuts card (opened with ?).
    show_keys: bool,
    settings_filter: String,
    settings_focus: FocusHandle,
    /// Focus of the settings text fields, in TEXT_FIELDS order.
    field_focus: [FocusHandle; 3],
    /// Latest fetch per list; older fetches of the same list are ignored.
    generations: HashMap<&'static str, u64>,
    history: History,
    current: Option<Video>,
    player: Player,
    /// X11 child window mpv renders into; None until created, or on Wayland.
    embed: Option<Rc<RefCell<Embed>>>,
    /// The last-watched video has been loaded (paused) into mpv at startup.
    preloaded: bool,
    fullscreen: bool,
    /// A video was requested and mpv hasn't started playing it yet.
    loading: bool,
    /// Hide mpv's window while loading, so the old video's last frame doesn't linger.
    /// Not on a fresh mpv start: mpv keeps its window unmapped if ours is hidden then.
    hide_while_loading: bool,
    /// The list the current video was picked from, for Prev / Next.
    queue: Arc<[Video]>,
    state: Option<player::State>,
    thumbs_requested: HashSet<String>,
    /// Thumbnails whose download failed: shown as a plain box, not a pulsing skeleton.
    thumbs_failed: HashSet<String>,
    /// Logged-in account for subscribe / like / save; loaded on first use.
    account: Option<Arc<Account>>,
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
    /// Downloads by video id: progress, then the saved file path or an error.
    downloads: HashMap<String, Download>,
    ticks: u32,
    _poll: Task<()>,
}

impl Unbloated {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let history = History::load();
        let settings = Settings::load();
        let root_focus = cx.focus_handle();
        window.focus(&root_focus);
        let tab = [(settings.subscriptions, Tab::Subscriptions), (settings.playlists, Tab::Playlists), (settings.history, Tab::History)]
            .into_iter()
            .find_map(|(on, tab)| on.then_some(tab))
            .unwrap_or(Tab::Settings);
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
                    Some(socket) => cx.background_executor().spawn(async move { player::query(&socket) }).await,
                    None => None,
                };
                if this.update_in(cx, |this, window, cx| this.tick(state, window, cx)).is_err() {
                    break;
                }
            }
        });
        let mut app = Self {
            cfg: Arc::new(Config::load()),
            settings,
            dragging: None,
            tab,
            subs: Browser::new(),
            yt_history: Load::Idle,
            playlists: Browser::new(),
            recs: Load::Idle,
            feed: Load::Idle,
            seen: Seen::load(),
            query: String::new(),
            search: Load::Idle,
            search_focus: cx.focus_handle(),
            save_filter: String::new(),
            save_focus: cx.focus_handle(),
            root_focus,
            up_next: store::load_data("up_next").unwrap_or_default(),
            lower: Lower::Recommended,
            show_keys: false,
            confirm_unsub: None,
            pip: false,
            searching: false,
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
            vim_g: false,
            vim_positions: HashMap::new(),
            hint_targets: Rc::new(RefCell::new(Vec::new())),
            hints: None,
            settings_filter: String::new(),
            settings_focus: cx.focus_handle(),
            field_focus: [cx.focus_handle(), cx.focus_handle(), cx.focus_handle()],
            generations: HashMap::new(),
            current: history.last().map(|w| w.video.clone()),
            history,
            player: Player::new(),
            embed: None,
            preloaded: false,
            fullscreen: false,
            loading: false,
            hide_while_loading: false,
            queue: Arc::new([]),
            state: None,
            thumbs_requested: HashSet::new(),
            thumbs_failed: HashSet::new(),
            account: None,
            status: None,
            status_requested: None,
            ended: None,
            saving: false,
            notice: None,
            downloads: HashMap::new(),
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
        // the History tab refreshes it.
        if app.settings.history {
            if let Some(items) = store::load_list("history") {
                app.yt_history = Load::Ready(items);
            }
        }
        if app.current.is_none() && app.settings.history {
            // No local history yet: fall back to YouTube's own history for "last watched".
            app.fetch(cx, "history", |s| &mut s.yt_history, Some("history".into()), |cfg, on| yt::history(cfg, on));
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
            if let Some(items) = store::load_list(name) {
                *slot(self) = Load::Ready(items);
                self.after_load(key);
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
        cx.background_executor()
            .spawn(async move {
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
                        *slot(this) = match res {
                            Err(e) if items.is_empty() => Load::Failed(e),
                            _ => Load::Ready(items),
                        };
                        this.after_load(key);
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

    fn after_load(&mut self, key: &str) {
        match key {
            "subs" => {
                if let Load::Ready(g) = &mut self.subs.groups {
                    g.sort_by_key(|g| g.title.to_lowercase());
                }
            }
            "history" if self.current.is_none() => self.current = self.yt_history.items().first().cloned(),
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
            Tab::Search => self.run_search(cx),
            Tab::History => self.fetch(cx, "history", |s| &mut s.yt_history, Some("history".into()), |cfg, on| yt::history(cfg, on)),
            tab => match self.browser(tab).open.clone() {
                Some(_) => self.load_group_videos(tab, cx),
                None if tab == Tab::Subscriptions => {
                    self.fetch(cx, "subs", |s| &mut s.subs.groups, Some("subs".into()), |cfg, on| yt::subscriptions(cfg, on));
                    if self.cfg.has_auth() && !matches!(self.feed, Load::Loading(_)) {
                        self.fetch(cx, "feed", |s| &mut s.feed, Some("feed".into()), |cfg, on| yt::feed(cfg, on));
                    }
                }
                None => self.fetch(cx, "playlists", |s| &mut s.playlists.groups, Some("playlists".into()), |cfg, on| yt::playlists(cfg, on)),
            },
        }
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
        match edit_text(&mut self.query, ev, cx) {
            Edit::Submit => {
                self.run_search(cx);
                window.blur();
            }
            Edit::Cancel => self.close_search(window, cx),
            Edit::Changed => {}
            // Arrows etc. inside the field mustn't reach the shortcuts.
            Edit::Ignored => cx.stop_propagation(),
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Enter search mode (/ or the search icon): full-width field, focused.
    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            .child(div().px_3().pb_1().text_xs().text_color(rgb(MUTED)).child("RECENT SEARCHES"))
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
                    .text_color(rgb(TEXT))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(HOVER)))
                    .child(svg().path(icons::path("recent")).size(px(14.)).text_color(rgb(MUTED)))
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
                    .text_color(rgb(MUTED))
                    .cursor_pointer()
                    .hover(|d| d.text_color(rgb(TEXT)))
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
            // "Liked videos" isn't a playlist you can add to.
            .filter(|g| g.id != "LL" && g.title.to_lowercase().contains(&filter))
            .cloned()
            .collect()
    }

    /// Type to filter, Enter saves to the first match, Esc closes.
    fn save_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        match edit_text(&mut self.save_filter, ev, cx) {
            Edit::Submit => {
                if let Some(g) = self.save_targets().into_iter().next() {
                    self.save_to(g, cx);
                }
            }
            Edit::Cancel => self.close_save(cx),
            Edit::Changed => {}
            Edit::Ignored => {
                cx.stop_propagation();
                return;
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Show mpv's window only when it has something current to show and nothing covers it.
    fn sync_embed(&mut self) {
        let visible = self.player.alive()
            && !(self.loading && self.hide_while_loading)
            && !self.saving
            && !self.show_keys
            // Audio only: keep showing the thumbnail.
            && !self.settings.audio_only;
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
        let mut counts = HashMap::new();
        for v in self.feed.items() {
            if self.seen.ids.contains(&v.id) || played.contains(v.id.as_str()) || !self.in_active_group(v) || self.is_muted(v) {
                continue;
            }
            *counts.entry(FEED_ID.to_string()).or_default() += 1;
            if let Some(ch) = channel_of(v) {
                *counts.entry(ch.to_string()).or_default() += 1;
            }
        }
        counts
    }

    /// Desktop notifications for feed videos of channels with the bell on, not notified yet.
    fn notify_uploads(&mut self) {
        let fresh: Vec<Video> = self
            .feed
            .items()
            .iter()
            .filter(|v| channel_of(v).is_some_and(|c| self.flags.notify.contains(c)))
            .filter(|v| !self.flags.notified.contains(&v.id) && !self.seen.ids.contains(&v.id))
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
                let _ = notify_rust::Notification::new().appname("unbloated-youtube").summary(&summary).body(&body).show();
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
        let videos = b.videos.items();
        if b.open.as_ref().is_some_and(|g| g.id == FEED_ID) {
            videos.iter().filter(|v| self.in_active_group(v) && !self.is_muted(v)).cloned().collect()
        } else {
            videos.to_vec()
        }
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
        match edit_text(name, ev, cx) {
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
            .bg(if on { rgb(ACCENT) } else { rgb(HOVER) })
            .cursor_pointer()
            .hover(|d| d.opacity(0.85))
            .child(svg().path(icons::path(icon)).size(px(14.)).text_color(rgb(TEXT)))
            .tooltip(tip(tooltip))
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
            .bg(if on { rgb(ACCENT) } else { rgb(HOVER) })
            .text_color(rgb(TEXT))
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
                    .border_color(rgb(MUTED))
                    .text_xs()
                    .text_color(if name.is_empty() && !focused { rgb(MUTED) } else { rgb(TEXT) })
                    .child(if name.is_empty() && !focused { "Group name".to_string() } else { format!("{name}▏") })
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
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                self.chip("group-all", "All", active.is_none())
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.select_group(None, cx)),
            );
        for (i, g) in self.groups.iter().enumerate() {
            let name = g.name.clone();
            bar = bar.child(
                self.chip(("group", i), format!("{} {}", g.name, g.channels.len()), active.as_deref() == Some(g.name.as_str()))
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.select_group(Some(name.clone()), cx)),
            );
        }
        bar = bar.child(self.new_group_chip(window, cx));
        if let Some(name) = active {
            let confirming = self.confirm_delete_group.as_deref() == Some(name.as_str());
            bar = bar.child(
                self.chip("delete-group", if confirming { "Delete group?" } else { "Delete" }, confirming)
                    .ml_auto()
                    .when(!confirming, |d| d.text_color(rgb(MUTED)))
                    .tooltip(tip("Click twice to delete this group (channels stay subscribed)"))
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.delete_group(name.clone(), cx)),
            );
        }
        bar
    }

    /// In a channel's header: which groups the channel belongs to (click to toggle).
    fn render_group_editor(&self, channel: &str, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        let mut row = div().flex().flex_wrap().gap_1().px_3().py_2().border_b_1().border_color(rgb(BORDER));
        if self.groups.is_empty() {
            row = row.child(div().text_xs().text_color(rgb(MUTED)).mr_2().child("No groups yet:"));
        }
        for (i, g) in self.groups.iter().enumerate() {
            let member = g.channels.iter().any(|c| c == channel);
            let (name, ch) = (g.name.clone(), channel.to_string());
            row = row.child(
                self.chip(("member", i), g.name.clone(), member)
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.toggle_membership(&name, &ch, cx)),
            );
        }
        row.child(self.new_group_chip(window, cx))
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
        let shorts = b.view == ChannelView::Shorts;
        let url = if shorts { group.url.replace("/videos", "/shorts") } else { group.url.clone() };
        // Revisiting a channel or playlist shows its last list instantly, then refreshes.
        let cache = Some(format!("group-{}{}", group.id, if shorts { "-shorts" } else { "" }));
        b.videos = Load::Idle;
        if tab == Tab::Subscriptions {
            self.fetch(cx, "subs.videos", |s| &mut s.subs.videos, cache, move |cfg, on| yt::group_videos(cfg, &url, on));
        } else {
            self.fetch(cx, "playlists.videos", |s| &mut s.playlists.videos, cache, move |cfg, on| {
                yt::group_videos(cfg, &url, on)
            });
        }
    }

    fn select_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.searching = false;
        let idle = match tab {
            Tab::Search | Tab::Settings => false,
            Tab::History => matches!(self.yt_history, Load::Idle),
            _ => matches!(self.browser(tab).groups, Load::Idle),
        };
        if idle {
            self.load_tab(cx);
        }
        cx.notify();
    }

    fn tab_loading(&self) -> bool {
        match self.tab {
            Tab::Settings => false,
            Tab::Search => matches!(self.search, Load::Loading(_)),
            Tab::History => matches!(self.yt_history, Load::Loading(_)),
            tab => {
                let b = self.browser_ref(tab);
                if b.open.is_some() {
                    matches!(b.videos, Load::Loading(_))
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
            self.recs = Load::Failed("Nothing to recommend yet — play something.".into());
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
        if let Some(i) = self.up_next.iter().position(|v| v.id == video.id) {
            self.up_next.remove(i);
            store::save_data("up_next", &self.up_next);
        }
        self.history.touch(&video);
        self.history.save();
        let channel_changed = self.current.as_ref().map(|c| &c.channel_url) != Some(&video.channel_url);
        self.start(video, false);
        if !self.cfg.has_auth() && channel_changed {
            self.load_recs(cx);
        }
        cx.notify();
    }

    /// Load `video` into mpv at its resume position.
    fn start(&mut self, video: Video, paused: bool) {
        if self.embed.is_none() {
            self.embed = Embed::new().map(|e| Rc::new(RefCell::new(e)));
            if self.embed.is_none() {
                eprintln!("unbloated-youtube: can't embed video (not X11?), using a separate mpv window");
            }
        }
        let wid = self.embed.as_ref().map(|e| {
            // mpv keeps its window unmapped if ours is hidden when it starts.
            e.borrow_mut().set_visible(true);
            e.borrow().id()
        });
        let start = self.history.position(&video.id);
        let options = player::options(&self.cfg, &self.settings);
        // Hide the old video's last frame only when mpv keeps running: a freshly started mpv
        // (first video, or changed options) never shows its picture if ours is hidden then.
        let restarts = self.player.options() != options.as_slice();
        self.hide_while_loading = !restarts;
        if let (true, Some(e)) = (self.hide_while_loading, &self.embed) {
            // Show the new thumbnail right away instead of the old video's last frame.
            e.borrow_mut().set_visible(false);
        }
        if let Err(e) = self.player.play(&options, &video.url(), start, self.settings.speed, wid, paused) {
            eprintln!("{e}");
        }
        self.saving = false;
        self.notice = None;
        self.current = Some(video);
        self.loading = true;
    }

    /// Save `video` with yt-dlp in the background, using the player's quality settings.
    fn download(&mut self, video: Video, cx: &mut Context<Self>) {
        let home = dirs::home_dir().unwrap_or_default();
        let dir = match self.settings.download_dir.trim() {
            "" => dirs::download_dir().unwrap_or_else(|| home.join("Downloads")),
            d => d.strip_prefix("~/").map_or_else(|| PathBuf::from(d), |rest| home.join(rest)),
        };
        let (cfg, format, url, id) = (self.cfg.clone(), player::format(&self.settings), video.url(), video.id.clone());
        self.downloads.insert(id.clone(), Download { progress: 0., result: None });
        let shared: Arc<Mutex<Download>> = Arc::new(Mutex::new(Download { progress: 0., result: None }));
        let sink = shared.clone();
        cx.background_executor()
            .spawn(async move {
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

    fn account_buttons(&self) -> bool {
        let st = &self.settings;
        self.cfg.has_auth() && (st.subscribe_button || st.save_button || st.like_button || st.dislike_button)
    }

    /// Run `f` with the account on a background thread (loading it first if needed),
    /// then `done` on the UI thread.
    fn with_account<R: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&Account) -> Result<R, String> + Send + 'static,
        done: impl FnOnce(&mut Self, Result<R, String>, &mut Context<Self>) + 'static,
    ) {
        let (cfg, account) = (self.cfg.clone(), self.account.clone());
        let task = cx.background_executor().spawn(async move {
            let account = match account {
                Some(a) => a,
                None => Arc::new(Account::load(&cfg)?),
            };
            let res = f(&account);
            Ok::<_, String>((account, res))
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                let res = match res {
                    Ok((account, res)) => {
                        this.account = Some(account);
                        res
                    }
                    Err(e) => Err(e),
                };
                done(this, res, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Fetch whether the current video is liked and its channel subscribed.
    fn load_status(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        if !self.account_buttons() || self.status_requested.as_ref() == Some(&id) {
            return;
        }
        self.status_requested = Some(id.clone());
        let vid = id.clone();
        self.with_account(cx, move |a| a.status(&vid), move |this, res, _| match res {
            Ok(st) if this.current.as_ref().is_some_and(|v| v.id == id) => this.status = Some((id, st)),
            Ok(_) => {}
            Err(e) => this.notice = Some(e),
        });
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
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        self.close_save(cx);
        self.notice = Some(format!("Saving to {}…", playlist.title));
        let pid = playlist.id.clone();
        self.with_account(cx, move |a| a.save_to_playlist(&pid, &id), move |this, res, _| {
            this.notice = Some(match res {
                Ok(()) => format!("Saved to {}", playlist.title),
                Err(e) => e,
            });
        });
    }

    /// What Next / autoplay plays: the Up next queue first, then the list the video came from.
    fn next_video(&self) -> Option<Video> {
        self.up_next.first().cloned().or_else(|| self.neighbor(1))
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

    fn toggle_play(&mut self, cx: &mut Context<Self>) {
        if self.state.is_some() {
            self.player.toggle_pause();
        } else if let Some(v) = self.current.clone() {
            self.play(v, None, cx);
        }
    }

    /// Keyboard shortcuts; see SHORTCUTS. Text fields stop the keys they use from reaching here.
    fn shortcut(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        if self.settings.vim && self.vim_key(ev, window, cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if k.modifiers.control || k.modifiers.alt || k.modifiers.platform {
            return;
        }
        let active = self.state.is_some();
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
            "left" if active => self.player.seek_relative(-5.),
            "right" if active => self.player.seek_relative(5.),
            "j" if active => self.player.seek_relative(-10.),
            "l" if active => self.player.seek_relative(10.),
            "f" if active => self.player.set_fullscreen(true),
            "m" => self.player.toggle_mute(),
            "n" => {
                if let Some(v) = self.next_video() {
                    self.play(v, None, cx);
                }
            }
            "p" => {
                if let Some(v) = self.neighbor(-1) {
                    self.play(v, None, cx);
                }
            }
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
        let active = self.state.is_some();
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
            "g" if pending_g => self.vim_move(isize::MIN, len, window),
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
            "f" => {
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
            "left" if active => self.player.seek_relative(-5.),
            "right" if active => self.player.seek_relative(5.),
            "," if active => self.player.seek_relative(-10.),
            "." if active => self.player.seek_relative(10.),
            "F" if active => self.player.set_fullscreen(true),
            "m" => self.player.toggle_mute(),
            "n" => {
                if let Some(v) = self.next_video() {
                    self.play(v, None, cx);
                }
            }
            "p" => {
                if let Some(v) = self.neighbor(-1) {
                    self.play(v, None, cx);
                }
            }
            "/" => self.open_search(window, cx),
            _ => return false,
        }
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

    /// Where `on_click_hinted` registers click targets this frame.
    fn hint_reg(&self) -> HintReg {
        HintReg { targets: self.hint_targets.clone(), on: self.settings.vim }
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
            Tab::Subscriptions | Tab::Playlists if self.browser_ref(self.tab).open.is_none() => (0, 44.),
            _ => (0, ROW_H),
        };
        let Some(ix) = cursor.checked_sub(offset) else { return };
        let visible = ((f32::from(window.viewport_size().height) - 160.) / row_h).max(1.) as usize;
        let top = self.vim_scroll.0.borrow().base_handle.logical_scroll_top().0;
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

    /// Header tabs in order, as shown.
    fn tab_list(&self) -> Vec<Tab> {
        let st = &self.settings;
        let mut tabs: Vec<Tab> = [(st.subscriptions, Tab::Subscriptions), (st.playlists, Tab::Playlists), (st.history, Tab::History)]
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
            Tab::Search => videos(self.search.items().to_vec()),
            Tab::History => {
                let (partial, all) = self.history_items();
                let mut items = videos(partial);
                items.extend(videos(all));
                items
            }
            tab => match &self.browser_ref(tab).open {
                Some(_) => videos(self.browser_videos(tab)),
                None => self.group_items(tab, &self.unseen_counts()).into_iter().map(Item::Group).collect(),
            },
        }
    }

    /// Videos as lists show them (Shorts hidden when turned off).
    fn visible(&self, videos: &[Video]) -> Vec<Video> {
        videos.iter().filter(|v| self.settings.shorts || !v.short).cloned().collect()
    }

    /// History tab: Continue watching (started, not finished), then local + YouTube history.
    fn history_items(&self) -> (Vec<Video>, Vec<Video>) {
        let mut all: Vec<Video> = self.history.items.iter().map(|w| w.video.clone()).collect();
        let seen: HashSet<String> = all.iter().map(|v| v.id.clone()).collect();
        all.extend(self.yt_history.items().iter().filter(|v| !seen.contains(&v.id)).cloned());
        let partial = self
            .history
            .items
            .iter()
            .filter(|w| w.position > 30. && !w.finished)
            .take(4)
            .map(|w| w.video.clone())
            .collect();
        (partial, all)
    }

    /// A browser tab's groups in display order. Subscriptions: New uploads first, then channels
    /// with new videos, then the rest (each part A–Z).
    fn group_items(&self, tab: Tab, counts: &HashMap<String, usize>) -> Vec<Group> {
        let mut g: Vec<Group> = self.browser_ref(tab).groups.items().to_vec();
        if tab == Tab::Subscriptions {
            if let Some(group) = self.active_group() {
                g.retain(|c| group.channels.contains(&c.id));
            }
            g.sort_by_key(|g| !counts.contains_key(&g.id));
            g.insert(0, Group { id: FEED_ID.into(), title: "New uploads".into(), url: ":ytsubs".into(), thumb: None });
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
    fn neighbor(&self, step: isize) -> Option<Video> {
        let current = self.current.as_ref()?;
        let i = self.queue.iter().position(|v| v.id == current.id)?;
        self.queue.get(i.checked_add_signed(step)?).cloned()
    }

    fn tick(&mut self, state: Option<player::State>, window: &mut Window, cx: &mut Context<Self>) {
        self.ticks += 1;
        self.preload(cx);
        self.load_status(cx);
        let url = self.current.as_ref().map(|v| v.url());
        let was_loading = self.loading;
        if let Some(s) = &state {
            if s.idle || (s.playing && Some(&s.path) == url.as_ref()) {
                self.loading = false;
            }
        }
        if !self.player.alive() {
            self.loading = false;
        }
        // Until mpv plays the new video, its state still describes the previous one.
        if state.is_some() {
            self.player.bind_mouse();
        }
        if state.as_ref().is_some_and(|s| s.help) {
            self.player.clear_help();
            self.show_keys = !self.show_keys;
            cx.notify();
        }
        let state = state.filter(|s| !s.idle && !self.loading);
        if let (Some(s), Some(v)) = (&state, &self.current) {
            // Finished videos restart from the beginning next time, and show as watched.
            let finished = s.duration > 0. && s.position > s.duration - 15.;
            let pos = if finished { 0. } else { s.position };
            self.history.set_position(&v.id, pos, finished || (self.history.is_finished(&v.id) && pos < 30.));
            if self.ticks % 20 == 0 {
                self.history.save();
            }
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
    /// inside unbloated-youtube; our X11 window may need a few ticks to show up in the WM's client list.
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
        if path.exists() {
            return Thumb::Ready(path);
        }
        if url.is_none() || self.thumbs_failed.contains(key) {
            return Thumb::Missing;
        }
        if let Some(url) = url.filter(|_| self.thumbs_requested.insert(key.to_string())) {
            let dest = path.clone();
            let key = key.to_string();
            let task = cx.background_executor().spawn(async move { thumbs::download(&url, &dest) });
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
        let frame = div().w(px(w)).h(px(h)).flex_none().overflow_hidden().rounded(radius).bg(rgb(HOVER));
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
                .hover(|d| d.bg(rgb(BORDER)))
                .child(svg().path(icons::path(icon)).size(px(14.)).text_color(rgb(TEXT)))
                .tooltip(tip(tooltip))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if in_up_next { this.dequeue(&v.id, cx) } else { this.enqueue(v.clone(), cx) }
                }))
        };
        let meta = [video.channel.clone(), video.duration.map(fmt_duration)]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("  ·  ");
        let thumb = self.thumb_el(&video.id, Some(video.thumb_url()), 96., 54., px(4.), cx);
        div()
            .id(id)
            .w_full()
            .overflow_hidden()
            .h(px(ROW_H))
            .px_3()
            .flex()
            .items_center()
            .gap_3()
            .cursor_pointer()
            .group("video-row")
            .when(playing, |d| d.bg(rgb(HOVER)))
            .when(selected, |d| d.bg(rgb(BORDER)))
            // Watched videos are dimmed, back to full on hover.
            .when(watched && !playing && !selected, |d| d.opacity(0.45))
            .hover(|d| d.bg(rgb(HOVER)).opacity(1.))
            .child(
                div().relative().child(thumb).when_some(progress, |d, p| {
                    d.child(div().absolute().bottom_0().left_0().h(px(3.)).w(px(96. * p)).bg(rgb(ACCENT)))
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    // Titles are truncated; hovering shows the whole one.
                    .child(
                        div()
                            .id("title")
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .truncate()
                            .child(video.title.clone())
                            .tooltip(tip(video.title.clone())),
                    )
                    .child(div().text_xs().text_color(rgb(MUTED)).truncate().child(meta)),
            )
            .child(action)
            .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.play(video.clone(), Some(queue.clone()), cx))
    }

    /// `left`: this is the left column's list, whose rows start at that index for Vim selection.
    fn video_list(&self, list_id: &'static str, videos: &[Video], left: Option<usize>, cx: &mut Context<Self>) -> AnyElement {
        // With Shorts turned off, they're hidden from every list.
        let videos: Arc<[Video]> = self.visible(videos).into();
        let in_up_next = list_id == "up-next";
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
                        this.video_row(i, videos[i].clone(), videos.clone(), seen, in_up_next, selected, cx)
                    })
                    .collect()
            }),
        )
        .when(left.is_some(), |l| l.track_scroll(self.vim_scroll.clone()))
        .flex_1()
        .into_any_element()
    }

    fn status(&self, msg: impl Into<SharedString>) -> AnyElement {
        div().p_4().text_sm().text_color(rgb(MUTED)).child(msg.into()).into_any_element()
    }

    fn failed(&self, e: &str) -> AnyElement {
        let hint = if self.cfg.has_auth() {
            String::new()
        } else {
            format!(
                "\n\nNot logged in. Add to {}:\ncookies_from_browser = \"firefox\"   # or cookies_file = \"/path/cookies.txt\"",
                store::config_dir().join("config.toml").display()
            )
        };
        self.status(format!("{e}{hint}"))
    }

    /// What to show instead of a list that has no items (yet), or None if it has some.
    fn placeholder<T>(&self, load: &Load<T>, empty: &str, rows: Rows) -> Option<AnyElement> {
        match load {
            Load::Failed(e) => Some(self.failed(e)),
            Load::Ready(v) if v.is_empty() => Some(self.status(empty.to_string())),
            Load::Ready(_) => None,
            Load::Loading(v) if !v.is_empty() => None,
            _ => Some(skeleton(rows)),
        }
    }

    fn render_list(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.tab {
            Tab::History => {}
            Tab::Settings => return self.render_settings(window, cx),
            Tab::Search if self.query.trim().is_empty() || matches!(self.search, Load::Idle) => {
                return self.render_recent_searches(cx);
            }
            Tab::Search => {
                return match self.placeholder(&self.search, "No results.", Rows::Videos) {
                    Some(p) => p,
                    None => self.video_list("search", &self.search.items().to_vec(), Some(0), cx),
                };
            }
            tab => return self.render_browser(tab, window, cx),
        }
        // Videos played in unbloated-youtube first, then the rest of YouTube's history.
        let (partial, videos) = self.history_items();
        if videos.is_empty() {
            if let Some(p) = self.placeholder(&self.yt_history, "Nothing here.", Rows::Videos) {
                return p;
            }
        }
        if partial.is_empty() {
            return self.video_list("history", &videos, Some(0), cx);
        }
        let label = |text: &'static str| div().px_3().pt_3().pb_1().text_xs().text_color(rgb(MUTED)).child(text);
        let queue: Arc<[Video]> = partial.clone().into();
        let offset = partial.len();
        let rows: Vec<_> = partial
            .into_iter()
            .enumerate()
            .map(|(i, v)| {
                let selected = self.vim_selected(i);
                self.video_row(("continue", i), v, queue.clone(), false, false, selected, cx)
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(label("CONTINUE WATCHING"))
            .children(rows)
            .child(label("HISTORY"))
            .child(div().flex().flex_col().flex_1().min_h_0().child(self.video_list("history", &videos, Some(offset), cx)))
            .into_any_element()
    }

    fn toggle(&mut self, field: fn(&mut Settings) -> &mut bool, cx: &mut Context<Self>) {
        let on = field(&mut self.settings);
        *on = !*on;
        self.settings.save();
        if self.settings.recommendations && matches!(self.recs, Load::Idle) {
            self.load_recs(cx);
        }
        self.apply_player_settings(cx);
    }

    /// Apply player settings to the running video now: speed directly; anything else that
    /// changes mpv's options reloads the current video at the same position.
    fn apply_player_settings(&mut self, cx: &mut Context<Self>) {
        self.player.set_speed(self.settings.speed);
        let options = player::options(&self.cfg, &self.settings);
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
            .child(div().text_xs().text_color(rgb(MUTED)).child("Skip these segments:"))
            .child(div().flex().flex_wrap().gap_1().children(SEGMENTS.iter().enumerate().map(|(i, &(label, name))| {
                let on = self.settings.skip_segments.iter().any(|s| s == name);
                div()
                    .id(("segment", i))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_xs()
                    .bg(if on { rgb(ACCENT) } else { rgb(HOVER) })
                    .text_color(rgb(TEXT))
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
            .child(div().flex_1().text_sm().text_color(rgb(TEXT)).child(label))
            .children(options.into_iter().enumerate().map(|(i, (text, on))| {
                div()
                    .id((label, i))
                    .ml_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_xs()
                    .bg(if on { rgb(ACCENT) } else { rgb(HOVER) })
                    .text_color(rgb(TEXT))
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
            .child(div().text_sm().text_color(rgb(TEXT)).child(label))
            .child(
                div()
                    .id(("field", i))
                    .track_focus(&focus)
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(HOVER))
                    .border_1()
                    .border_color(if focused { rgb(MUTED) } else { rgb(BORDER) })
                    .text_sm()
                    .cursor_text()
                    .map(|d| match (value.is_empty(), focused) {
                        (true, false) => d.text_color(rgb(MUTED)).child(hint),
                        _ => d.text_color(rgb(TEXT)).child(format!("{value}{}", if focused { "▏" } else { "" })),
                    })
                    .on_click_hinted(&self.hint_reg(), cx, move |_, _, window, cx| {
                        window.focus(&focus);
                        cx.notify();
                    })
                    .on_key_down(cx.listener(move |this, ev: &KeyDownEvent, window, cx| {
                        match edit_text(field(&mut this.settings), ev, cx) {
                            Edit::Submit | Edit::Cancel => {
                                window.blur();
                                // Apply once editing is done, not on every keystroke.
                                this.apply_player_settings(cx);
                            }
                            Edit::Changed => this.settings.save(),
                            Edit::Ignored => {
                cx.stop_propagation();
                return;
            }
                        }
                        cx.stop_propagation();
                        cx.notify();
                    })),
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
                .bg(if on { rgb(ACCENT) } else { rgb(BORDER) })
                .child(div().size(px(14.)).rounded_full().bg(rgb(TEXT)));
            div()
                .id(("toggle", i))
                .w_full()
                .px_4()
                .py_3()
                .flex()
                .items_center()
                .cursor_pointer()
                .hover(|d| d.bg(rgb(HOVER)))
                .child(
                    // min_w_0 lets a long hint wrap instead of pushing the switch out of view.
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_sm().text_color(rgb(TEXT)).child(label))
                        .child(div().text_xs().text_color(rgb(MUTED)).child(hint)),
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
            ("NOTIFICATIONS", &NOTIFY_TOGGLES[..]),
        ] {
            let rows = toggles
                .iter()
                .enumerate()
                .filter(|(_, t)| matches(t.0, t.1))
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
        for (i, (label, hint, _)) in TEXT_FIELDS.iter().enumerate() {
            if matches(label, hint) {
                player.push(self.text_field(i, window, cx).into_any_element());
            }
        }
        if self.settings.notifications && matches("Check every", "notifications minutes interval") {
            let row = self.choice_row(
                "Check every",
                NOTIFY_MINUTES.iter().map(|m| (format!("{m} min"), *m == self.settings.notify_minutes)).collect(),
                |s, i| s.notify_minutes = NOTIFY_MINUTES[i],
                cx,
            );
            sections[3].1.push(row.into_any_element());
        }
        let show_account = matches("Account", "login logged in cookies");
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
            .bg(rgb(HOVER))
            .border_1()
            .border_color(if focused { rgb(MUTED) } else { rgb(BORDER) })
            .text_sm()
            .cursor_text()
            .map(|d| match (self.settings_filter.is_empty(), focused) {
                (true, false) => d.text_color(rgb(MUTED)).child("Search settings"),
                _ => d.text_color(rgb(TEXT)).child(format!("{}{}", self.settings_filter, if focused { "▏" } else { "" })),
            })
            .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                window.focus(&this.settings_focus);
                cx.notify();
            })
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match edit_text(&mut this.settings_filter, ev, cx) {
                    Edit::Submit => window.blur(),
                    Edit::Cancel => {
                        this.settings_filter.clear();
                        window.blur();
                    }
                    Edit::Changed => {}
                    Edit::Ignored => {
                cx.stop_propagation();
                return;
            }
                }
                cx.stop_propagation();
                cx.notify();
            }));
        let login = match (&self.cfg.cookies_file, &self.cfg.cookies_from_browser) {
            (Some(f), _) => format!("Logged in with cookies from {f}"),
            (None, Some(b)) => format!("Logged in with cookies from {b}"),
            _ => "Not logged in".to_string(),
        };
        div()
            .id("settings-page")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .py_2()
            .child(search)
            .when(nothing, |d| d.child(self.status("No settings match.")))
            .children(sections.into_iter().filter(|(_, rows)| !rows.is_empty()).map(|(title, rows)| {
                div()
                    .flex()
                    .flex_col()
                    .child(div().px_4().pt_4().pb_2().text_xs().text_color(rgb(MUTED)).child(title))
                    .children(rows)
            }))
            .when(query.is_empty() || matches("Changes apply", "player"), |d| {
                d.child(
                    div()
                        .px_4()
                        .pt_2()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child("Changes apply right away (text fields: after Enter)."),
                )
            })
            .when(self.settings.sponsorblock && player::sponsorblock_script().is_none(), |d| {
                d.child(div().px_4().pt_1().text_xs().text_color(rgb(ACCENT)).child(
                    "SponsorBlock script not found: start the app from its nix-shell (sets UNBLOATED_SPONSORBLOCK).",
                ))
            })
            .when(self.shortcuts().iter().any(|(k, what)| matches(k, what)) || matches("Keyboard shortcuts", "keys"), |d| {
                d.child(div().px_4().pt_4().pb_2().text_xs().text_color(rgb(MUTED)).child("KEYBOARD SHORTCUTS")).children(
                    self.shortcuts().iter().filter(|(k, what)| matches(k, what) || matches("Keyboard shortcuts", "keys")).map(|(k, what)| {
                        div()
                            .px_4()
                            .py_1()
                            .flex()
                            .text_sm()
                            .child(div().w(px(190.)).flex_none().text_color(rgb(TEXT)).child(*k))
                            .child(div().text_color(rgb(MUTED)).child(*what))
                    }),
                )
            })
            .when(show_account, |d| {
                d.child(div().px_4().pt_4().pb_2().text_xs().text_color(rgb(MUTED)).child("ACCOUNT"))
                    .child(div().px_4().text_sm().text_color(rgb(TEXT)).child(login))
                    .child(
                div()
                    .px_4()
                    .pt_1()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(format!("Edit {} to change", store::config_dir().join("config.toml").display())),
                    )
            })
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
                self.icon_chip("channel-groups", "folder", self.editing_groups, "Add this channel to groups")
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
            let group_editor = (is_channel && self.editing_groups).then(|| self.render_group_editor(&open.id, window, cx));

            let view = self.browser_ref(tab).view;
            // Channels (not the New uploads feed or playlists) get Videos | Shorts tabs.
            let channel_tabs = tab == Tab::Subscriptions && open.id != FEED_ID && self.settings.shorts;
            let videos = &self.browser_ref(tab).videos;
            let empty = if view == ChannelView::Shorts { "No Shorts." } else { "No videos." };
            let body = match self.placeholder(videos, empty, Rows::Videos) {
                Some(p) => p,
                None => self.video_list("group", &self.browser_videos(tab), Some(0), cx),
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
                        .border_color(rgb(BORDER))
                        .hover(|d| d.bg(rgb(HOVER)))
                        .child(svg().path(icons::path("arrow-left")).size(px(16.)).mr_3().flex_none().text_color(rgb(MUTED)))
                        .tooltip(tip(back.trim_start_matches("← ").to_string()))
                        .items_center()
                        .child(div().flex_1().min_w_0().text_color(rgb(TEXT)).truncate().child(open.title.clone()))
                        .children(flag_chips)
                        .children(groups_button)
                        .children(sub_button)
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
                            .border_color(rgb(BORDER))
                            .child(
                                tab_button("Videos", view == ChannelView::Videos)
                                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.set_channel_view(ChannelView::Videos, cx)),
                            )
                            .child(
                                tab_button("Shorts", view == ChannelView::Shorts)
                                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.set_channel_view(ChannelView::Shorts, cx)),
                            ),
                    )
                })
                .child(body)
                .into_any_element();
        }
        let groups = &self.browser_ref(tab).groups;
        if let Some(p) = self.placeholder(groups, "Nothing here.", Rows::Groups) {
            return p;
        }
        let counts = Arc::new(if tab == Tab::Subscriptions { self.unseen_counts() } else { HashMap::new() });
        let g: Arc<[Group]> = self.group_items(tab, &counts).into();
        let bar = (tab == Tab::Subscriptions).then(|| self.render_group_bar(window, cx));
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
                                .bg(rgb(ACCENT))
                                .child(svg().path(icons::path("new")).size(px(14.)).text_color(rgb(TEXT)))
                                .into_any_element()
                        } else {
                            this.thumb_el(&group.id, group.thumb.clone(), 28., 28., px(14.), cx)
                        };
                        let new = counts.get(&group.id).copied().unwrap_or(0);
                        let selected = this.vim_selected(i);
                        let muted = this.flags.muted.contains(&group.id);
                        let bell = this.flags.notify.contains(&group.id);
                        div()
                            .id(i)
                            .when(selected, |d| d.bg(rgb(BORDER)))
                            .w_full()
                            .h(px(44.))
                            .px_3()
                            .flex()
                            .items_center()
                            .gap_3()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(HOVER)))
                            .child(avatar)
                            .child(
                                div()
                                    .id("name")
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .when(muted, |d| d.text_color(rgb(MUTED)))
                                    .child(group.title.clone())
                                    .tooltip(tip(group.title.clone())),
                            )
                            .when(bell, |d| d.child(svg().path(icons::path("bell")).size(px(12.)).flex_none().text_color(rgb(MUTED))))
                            .when(muted, |d| d.child(svg().path(icons::path("muted")).size(px(13.)).flex_none().text_color(rgb(MUTED))))
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
                                        .bg(rgb(ACCENT))
                                        .text_xs()
                                        .text_color(rgb(TEXT))
                                        .child(new.to_string()),
                                )
                                .tooltip(tip(format!("{new} new video{}", if new == 1 { "" } else { "s" })))
                            })
                            .on_click_hinted(&this.hint_reg(), cx, move |this, _, _, cx| this.open_group(tab, group.clone(), cx))
                    })
                    .collect()
            }),
        )
        .track_scroll(self.vim_scroll.clone())
        .flex_1();
        div().flex().flex_col().flex_1().min_h_0().children(bar).child(list).into_any_element()
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
            Thumb::Pending => screen = screen.child(pulse("screen-skeleton", div().size_full().bg(rgb(HOVER)))),
            Thumb::Missing => {}
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
            return div().p_4().text_sm().text_color(rgb(MUTED)).child("Pick a video on the left.");
        };
        let resume = self.history.position(&video.id);
        let screen = self.screen(&video, false, cx);
        let (pos, dur, paused, active) = match &self.state {
            Some(s) => (s.position, s.duration, s.paused, true),
            None => (resume, video.duration.unwrap_or(0.), true, false),
        };
        let filled = if dur > 0. { ((pos / dur) * SEEK_SEGMENTS as f64) as usize } else { 0 };
        let chapters = self.state.as_ref().map(|s| s.chapters.clone()).unwrap_or_default();

        let (play_icon, play_tip) = match (active, paused) {
            (true, true) => ("play", "Play (Space)".to_string()),
            (true, false) => ("pause", "Pause (Space)".to_string()),
            (false, _) if resume > 0. => ("play", format!("Resume at {}", fmt_duration(resume))),
            (false, _) => ("play", "Play".to_string()),
        };
        let time = if self.loading {
            "Loading…".to_string()
        } else {
            format!("{} / {}", fmt_duration(pos), fmt_duration(dur))
        };
        let replay = video.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .child(screen)
            .child(div().text_color(rgb(TEXT)).line_clamp(2).child(video.title.clone()))
            .child({
                let name = video.channel.clone().unwrap_or_default();
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
                                .bg(rgb(HOVER))
                                .text_xs()
                                .text_color(rgb(TEXT))
                                .cursor_pointer()
                                .hover(|d| d.bg(rgb(BORDER)))
                                .child(avatar)
                                .child(name)
                                .child(div().text_color(rgb(MUTED)).child("›"))
                                .tooltip(tip("Show this channel's videos"))
                                .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.show_channel(g.clone(), cx)),
                        )
                    }
                    None => div().text_xs().text_color(rgb(MUTED)).child(name),
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
                            .child(div().size_full().bg(if i < filled { rgb(ACCENT) } else { rgb(BORDER) }))
                            .when(dur > 0., |d| d.tooltip(tip(tooltip)))
                            .when(active, |d| {
                                d.on_click(cx.listener(move |this, _, _, _| {
                                    this.player.seek_absolute(dur * i as f64 / SEEK_SEGMENTS as f64)
                                }))
                            })
                    }))
                    .into_any_element()
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(icon_button("play", play_icon, play_tip, true).on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                        if this.state.is_some() {
                            this.player.toggle_pause();
                        } else {
                            this.play(replay.clone(), None, cx);
                        }
                    }))
                    .child(self.step_button("prev", "prev", "Previous (P)", -1, cx))
                    .child(self.step_button("next", "next", "Next (N)", 1, cx))
                    .child(
                        icon_button("back10", "back", "Back 10 seconds (J)", active)
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, _| this.player.seek_relative(-10.)),
                    )
                    .child(
                        icon_button("fwd10", "forward", "Forward 10 seconds (L)", active)
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, _| this.player.seek_relative(10.)),
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
                            .bg(rgb(HOVER))
                            .text_xs()
                            .text_color(rgb(TEXT))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(BORDER)))
                            .child(format!("{}×", self.settings.speed))
                            .tooltip(tip("Playback speed (click to change)"))
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                let i = SPEEDS.iter().position(|s| *s == this.settings.speed).map_or(1, |i| (i + 1) % SPEEDS.len());
                                this.settings.speed = SPEEDS[i];
                                this.settings.save();
                                this.player.set_speed(this.settings.speed);
                                cx.notify();
                            }),
                    )
                    .when(self.account_buttons() || self.settings.share_button || self.settings.download_button, |d| {
                        d.child(div().w(px(8.)))
                    })
                    .when(self.account_buttons(), |d| d.children(self.account_buttons_els(cx)))
                    .when(self.settings.download_button, |d| {
                        let (tip_text, enabled) = match self.downloads.get(&video.id) {
                            Some(Download { result: None, progress }) => (format!("Downloading {progress:.0}%"), false),
                            Some(Download { result: Some(Ok(f)), .. }) => (format!("Downloaded to {f}"), false),
                            _ => ("Download".to_string(), true),
                        };
                        let v = video.clone();
                        d.child(
                            icon_button("download", "download", tip_text, enabled)
                                .when(enabled, |d| d.on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.download(v.clone(), cx))),
                        )
                    })
                    .when(self.settings.share_button, |d| {
                        let link = format!("https://youtu.be/{}", video.id);
                        d.child(icon_button("share", "share", "Copy link", true).on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                            this.notice = Some(format!("Link copied: {link}"));
                            cx.notify();
                        }))
                    })
                    .child(div().ml_auto().pl_2().flex_none().text_xs().text_color(rgb(MUTED)).child(time)),
            )
            .when_some(self.notice.clone(), |d, n| d.child(div().text_xs().text_color(rgb(MUTED)).truncate().child(n)))
            .when_some(self.downloads.get(&video.id).map(Download::status), |d, n| {
                d.child(div().text_xs().text_color(rgb(MUTED)).truncate().child(n))
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
                    .when(ready && !s.subscribed, |d| d.bg(rgb(ACCENT)))
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
        if set.like_button {
            let (icon, tip) = if s.liked { ("liked", "Remove like") } else { ("like", "Like") };
            out.push(
                icon_button("like", icon, tip, ready)
                    // Red while liked, so the state is obvious.
                    .when(ready && s.liked, |d| d.bg(rgb(ACCENT)))
                    .when(ready, |d| d.on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.toggle_like(cx))),
            );
        }
        if set.dislike_button {
            let (icon, tip) = if s.disliked { ("disliked", "Remove dislike") } else { ("dislike", "Dislike") };
            out.push(
                icon_button("dislike", icon, tip, ready)
                    .when(ready && s.disliked, |d| d.bg(rgb(ACCENT)))
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
            if self.playlists.groups.items().is_empty() {
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
                                .text_color(rgb(TEXT))
                                .cursor_pointer()
                                .hover(|d| d.bg(rgb(HOVER)))
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
            .bg(rgb(PANEL))
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
                            .child(div().text_color(rgb(TEXT)).child("Save to playlist"))
                            .child(div().text_xs().text_color(rgb(MUTED)).truncate().child(title)),
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
                    .bg(rgb(HOVER))
                    .border_1()
                    .border_color(if focused { rgb(MUTED) } else { rgb(BORDER) })
                    .text_sm()
                    .map(|d| match (self.save_filter.is_empty(), focused) {
                        (true, true) => d.text_color(rgb(MUTED)).child("Type to filter…"),
                        (true, false) => d.text_color(rgb(MUTED)).child("Click here, then type to filter"),
                        _ => d.text_color(rgb(TEXT)).child(format!("{}{}", self.save_filter, if focused { "▏" } else { "" })),
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
                .py(px(3.))
                .flex()
                .justify_center()
                .rounded_md()
                .bg(rgb(HOVER))
                .border_1()
                .border_b_2()
                .border_color(rgb(BORDER))
                .text_sm()
                .text_color(rgb(TEXT))
                .child(k.to_string())
        };
        let (mut rest, groups) = if self.settings.vim { (&VIM_SHORTCUTS[..], &VIM_SHEET_GROUPS[..]) } else { (&SHORTCUTS[..], &SHEET_GROUPS[..]) };
        let columns = groups.iter().map(|&(title, n)| {
            let (items, tail) = rest.split_at(n);
            rest = tail;
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().pb_1().text_xs().text_color(rgb(ACCENT)).child(title.to_uppercase()))
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
                        .child(div().flex_1().min_w_0().text_sm().text_color(rgb(MUTED)).child(*what))
                }))
        });
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
            .bg(rgb(BG))
            .on_click(cx.listener(|this, _, _, cx| {
                this.show_keys = false;
                this.sync_embed();
                cx.notify();
            }))
            .child(
                div()
                    .w(px(980.))
                    .max_w(relative(0.92))
                    .p_8()
                    .rounded_xl()
                    .bg(rgb(PANEL))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .flex()
                    .flex_col()
                    .gap_6()
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .child(div().flex_1().text_2xl().text_color(rgb(TEXT)).child(if self.settings.vim {
                                "Keyboard shortcuts · Vim mode"
                            } else {
                                "Keyboard shortcuts"
                            }))
                            .child(div().text_xs().text_color(rgb(MUTED)).child("? or Esc to close")),
                    )
                    .child(div().flex().gap_10().children(columns)),
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
        let target = if step > 0 { self.next_video() } else { self.neighbor(step) };
        let tip = match &target {
            Some(v) => format!("{tip}: {}", v.title),
            None => tip.to_string(),
        };
        icon_button(id, icon, tip, target.is_some()).when_some(target, |d, video| {
            d.on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.play(video.clone(), None, cx))
        })
    }

    fn render_recs(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(p) = self.placeholder(&self.recs, "No recommendations.", Rows::Videos) {
            return p;
        }
        let current = self.current.as_ref().map(|c| c.id.clone());
        let v: Vec<Video> = self.recs.items().iter().filter(|r| Some(&r.id) != current.as_ref()).cloned().collect();
        self.video_list("recs", &v, None, cx)
    }
}

#[derive(Clone)]
struct Download {
    progress: f32,
    result: Option<Result<String, String>>,
}

impl Download {
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
}

/// A click target for `f` hints: where it is on screen and what clicking it does.
type HintTarget = (GBounds<GPixels>, Rc<dyn Fn(&mut Unbloated, &mut Window, &mut Context<Unbloated>)>);

#[derive(Clone)]
struct HintReg {
    targets: Rc<RefCell<Vec<HintTarget>>>,
    /// Vim mode is on (otherwise nothing is registered).
    on: bool,
}

/// `on_click` that also registers the element for `f` hints, which run the same handler.
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
        if !h.on {
            return el;
        }
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
    let bar = |w: f32, h: f32| div().w(relative(w)).h(px(h)).rounded_sm().bg(rgb(HOVER));
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
                .child(div().w(px(96.)).h(px(54.)).flex_none().rounded(px(4.)).bg(rgb(HOVER)))
                .child(div().flex_1().flex().flex_col().gap_2().child(bar(w, 12.)).child(bar(w * 0.45, 10.))),
            Rows::Groups => div()
                .h(px(44.))
                .px_3()
                .flex()
                .items_center()
                .gap_3()
                .child(div().size(px(28.)).flex_none().rounded_full().bg(rgb(HOVER)))
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
            .bg(rgb(ACCENT))
            .with_animation(id, Animation::new(Duration::from_millis(1200)).repeat(), |d, t| {
                d.left(relative(t * 1.3 - 0.3))
            }),
    )
}

/// A draggable 5px bar between two panes.
fn divider(id: &'static str, split: Split, cx: &mut Context<Unbloated>) -> Stateful<gpui::Div> {
    let bar = div().id(id).flex_none().bg(rgb(BORDER)).hover(|d| d.bg(rgb(MUTED)));
    let bar = match split {
        Split::Columns => bar.w(px(5.)).h_full().cursor(CursorStyle::ResizeLeftRight),
        Split::Player => bar.h(px(5.)).w_full().cursor(CursorStyle::ResizeUpDown),
    };
    bar.on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, _| this.dragging = Some(split)))
}

enum Edit {
    Changed,
    Submit,
    Cancel,
    Ignored,
}

/// Minimal one-line text editing: type (any layout), Backspace, Ctrl+Backspace, Ctrl+V, Enter, Esc.
fn edit_text(text: &mut String, ev: &KeyDownEvent, cx: &App) -> Edit {
    let k = &ev.keystroke;
    let m = &k.modifiers;
    match k.key.as_str() {
        "enter" => return Edit::Submit,
        "escape" => return Edit::Cancel,
        "backspace" if m.control => {
            let kept = text.trim_end().rfind(' ').map_or(0, |i| i + 1);
            text.truncate(kept);
        }
        "backspace" => {
            text.pop();
        }
        "v" if m.control => {
            if let Some(clip) = cx.read_from_clipboard().and_then(|c| c.text()) {
                text.push_str(clip.lines().next().unwrap_or_default());
            }
        }
        _ if !(m.control || m.alt || m.platform) && k.key_char.is_some() => text.push_str(k.key_char.as_deref().unwrap()),
        _ => return Edit::Ignored,
    }
    Edit::Changed
}

/// Hover tooltip content.
struct Tip(SharedString);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(rgb(HOVER))
            .border_1()
            .border_color(rgb(BORDER))
            .text_xs()
            .text_color(rgb(TEXT))
            .child(self.0.clone())
    }
}

fn tip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    let text = text.into();
    move |_, cx| cx.new(|_| Tip(text.clone())).into()
}

/// Square icon button with a hover tooltip; `enabled: false` greys it out (add clicks only when enabled).
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
        .bg(rgb(HOVER))
        .child(svg().path(icons::path(icon)).size(px(16.)).text_color(if enabled { rgb(TEXT) } else { rgb(BORDER) }))
        .when(enabled, |d| d.cursor_pointer().hover(|d| d.bg(rgb(BORDER))))
        .tooltip(tip(tooltip))
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
        .border_color(if active { rgb(ACCENT).into() } else { Hsla::transparent_black() })
        .text_color(if active { rgb(TEXT) } else { rgb(MUTED) })
        .hover(|d| d.text_color(rgb(TEXT)))
        .child(label)
}

impl Render for Unbloated {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let (true, Some(video)) = (self.fullscreen, self.current.clone()) {
            // Only the video; f / double-click / Esc on it (handled by mpv) leave fullscreen.
            return div().size_full().bg(gpui::black()).child(self.screen(&video, true, cx));
        }
        let st = &self.settings;
        let tabs: Vec<_> = [
            (st.subscriptions, "Subscriptions", Tab::Subscriptions),
            (st.playlists, "Playlists", Tab::Playlists),
            (st.history, "History", Tab::History),
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
                .hover(|d| d.bg(rgb(HOVER)))
                .child(svg().path(icons::path(icon)).size(px(16.)).text_color(rgb(MUTED)))
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
                .bg(rgb(HOVER))
                .border_1()
                .border_color(if focused { rgb(MUTED) } else { rgb(BORDER) })
                .text_sm()
                .truncate()
                .cursor_text()
                .map(|d| match (self.query.is_empty(), focused) {
                    (true, _) => d.text_color(rgb(MUTED)).child(if focused { "▏Search YouTube" } else { "Search YouTube" }),
                    (false, true) => d.text_color(rgb(TEXT)).child(format!("{}▏", self.query)),
                    (false, false) => d.text_color(rgb(TEXT)).child(self.query.clone()),
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
                .border_color(rgb(BORDER))
                .child(
                    header_icon("search-back", "arrow-left")
                        .tooltip(tip("Back (Esc)"))
                        .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| this.close_search(window, cx)),
                )
                .child(field)
                .when(!self.query.is_empty(), |d| {
                    d.child(
                        header_icon("search-clear", "close")
                            .tooltip(tip("Clear"))
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
            .border_color(rgb(BORDER))
            .children(tabs.into_iter().map(|(label, tab)| {
                tab_button(label, self.tab == tab).on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.select_tab(tab, cx))
            }))
            .child(div().flex_1())
            .child(
                header_icon("search-open", "search")
                    .tooltip(tip("Search (/)"))
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| this.open_search(window, cx)),
            )
            .child({
                let icon = svg().path(icons::path("refresh")).size(px(16.)).text_color(rgb(MUTED));
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
                    .hover(|d| d.bg(rgb(HOVER)))
                    .child(icon)
                    .tooltip(tip("Refresh"))
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
                    .border_color(if self.tab == Tab::Settings { rgb(ACCENT).into() } else { Hsla::transparent_black() })
                    .child(svg().path(icons::path("settings")).size(px(16.)).text_color(if self.tab == Tab::Settings {
                        rgb(TEXT)
                    } else {
                        rgb(MUTED)
                    }))
                    .tooltip(tip("Settings"))
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.select_tab(Tab::Settings, cx)),
            )
        };

        let header_bar = if self.tab_loading() {
            loading_bar("list-loading").into_any_element()
        } else {
            div().h(px(2.)).into_any_element()
        };
        let left = div()
            .flex()
            .flex_col()
            .w(relative(self.settings.split))
            .flex_none()
            .min_w_0()
            .child(header)
            .child(header_bar)
            .child(div().flex().flex_col().flex_1().min_h_0().child(self.render_list(window, cx)))
;

        let player = div().flex().flex_col().min_h_0().child(self.render_player(cx));
        let right = div().relative().flex().flex_col().flex_1().min_w_0().bg(rgb(PANEL));
        let right = if self.settings.window_buttons {
            let control = |id: &'static str, icon: &'static str, tooltip: &'static str, hover: u32, color: u32| {
                div()
                    .id(id)
                    .p(px(7.))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |d| d.bg(rgb(hover)))
                    .child(svg().path(icons::path(icon)).size(px(14.)).text_color(rgb(color)))
                    .tooltip(tip(tooltip))
            };
            right.child(
                div()
                    .flex()
                    .justify_end()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(control("win-min", "minimize", "Minimize", HOVER, MUTED).on_click(|_, window, _| window.minimize_window()))
                    .child(control("win-max", "maximize", "Maximize", HOVER, MUTED).on_click(|_, window, _| window.zoom_window()))
                    .child(control("win-close", "close", "Close", ACCENT, TEXT).on_click(|_, window, _| window.remove_window())),
            )
        } else {
            right
        };
        let show_recs = self.settings.recommendations;
        let right = if show_recs || !self.up_next.is_empty() {
            let lower = if show_recs { self.lower } else { Lower::UpNext };
            let body = match lower {
                Lower::Recommended => self.render_recs(cx),
                Lower::UpNext if self.up_next.is_empty() => self.status("Nothing queued. Hover a video and press + to add it."),
                Lower::UpNext => self.video_list("up-next", &self.up_next.clone(), None, cx),
            };
            right
                .child(player.h(relative(self.settings.player)).flex_none())
                .child(divider("split-player", Split::Player, cx))
                .child(
                    div()
                        .flex()
                        .px_2()
                        .border_b_1()
                        .border_color(rgb(BORDER))
                        .when(show_recs, |d| {
                            d.child(
                                tab_button("Recommended", lower == Lower::Recommended).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                    this.lower = Lower::Recommended;
                                    cx.notify();
                                }),
                            )
                        })
                        .child(
                            tab_button(
                                if self.up_next.is_empty() { "Up next".to_string() } else { format!("Up next ({})", self.up_next.len()) },
                                lower == Lower::UpNext,
                            )
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                this.lower = Lower::UpNext;
                                cx.notify();
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
            self.vim_scroll.scroll_to_item(self.vim_cursor, ScrollStrategy::Center);
        }
        // Collected again during this frame's paint (see hint_target).
        self.hint_targets.borrow_mut().clear();
        let hint_overlay = self.hints.as_ref().map(|(targets, typed)| hint_labels(targets, typed));
        let sheet = self.show_keys.then(|| self.render_cheatsheet(cx));
        div()
            .size_full()
            .relative()
            .flex()
            .track_focus(&self.root_focus)
            .on_key_down(cx.listener(Self::shortcut))
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .child(left)
            .child(divider("split-columns", Split::Columns, cx))
            .child(right)
            .children(sheet)
            .children(hint_overlay)
            // While dragging, X keeps sending us pointer events even over mpv's window.
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                let Some(split) = this.dragging else { return };
                let size = window.viewport_size();
                match split {
                    Split::Columns => this.settings.split = (e.position.x / size.width).clamp(0.2, 0.8),
                    Split::Player => this.settings.player = (e.position.y / size.height).clamp(0.25, 0.9),
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

fn main() {
    store::migrate_old_dirs();
    Application::new().with_assets(icons::Assets).run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("unbloated-youtube".into()),
                titlebar: Some(TitlebarOptions { title: Some("unbloated-youtube".into()), ..Default::default() }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Unbloated::new(window, cx)),
        )
        .unwrap();
        cx.on_window_closed(|cx| cx.quit()).detach();
    });
}
