//! The application: its state (`Unbloated`), its behaviour in one file per feature, and the views.

use crate::backend::account::{Account, Me, VideoStatus};
use crate::backend::auth::{Auth, BROWSERS};
use crate::platform::embed::Embed;
use gpui::{
    Animation, AnimationExt, HighlightStyle, Pixels, AnyElement, ClipboardItem, Context, CursorStyle, ElementId, FocusHandle, Hsla, KeyDownEvent, MouseButton,
    MouseMoveEvent, ObjectFit, SharedString, Stateful, Task,
    Transformation, Window, canvas, div, img, prelude::*, px, radians, relative, rgb, svg,
    uniform_list, ScrollStrategy, UniformListScrollHandle, Bounds as GBounds, Pixels as GPixels,
};
use crate::backend::player::Player;
use serde::{Serialize, de::DeserializeOwned};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::backend::store::{ChannelFlags, ChannelGroup, Config, History, Seen, Settings};
use crate::backend::yt::{Comment, Group, Video, fmt_count, fmt_duration};
use crate::backend::{account, cast, cli, dearrow, http, links, player, prefetch, proxy, store, thumbs, yt};
use crate::platform::{self, mpris};
use crate::ui::{icons, textsel};
use crate::ui::*;

mod browse;
mod casting;
mod commands;
mod hints;
mod images;
mod input;
mod layout;
mod library;
mod menus;
mod navigation;
mod playback;
mod session;
mod settings;
mod state;
mod subscriptions;
mod subtitles;
mod video_info;
mod view;

pub(crate) use state::*;
pub(crate) use subscriptions::*;
pub(crate) use settings::*;
pub(crate) use hints::*;

pub(crate) struct Unbloated {
    pub(crate) cfg: Arc<Config>,
    pub(crate) settings: Settings,
    pub(crate) dragging: Option<Split>,
    /// Pointer y and Continue watching height when that drag started.
    pub(crate) drag_from: (f32, f32),
    pub(crate) continue_scroll: UniformListScrollHandle,
    pub(crate) tab: Tab,
    pub(crate) subs: Browser,
    pub(crate) yt_history: Load<Video>,
    /// When YouTube's history was last asked for in this session (the startup list is the one
    /// saved last time).
    pub(crate) yt_history_asked: Option<Instant>,
    pub(crate) playlists: Browser,
    pub(crate) recs: Load<Video>,
    /// Watch later, for its tab under the player; fetched when the tab is first shown.
    pub(crate) watch_later: Load<Video>,
    /// Description of the video `description_for` (one item); fetched when its tab is first shown.
    pub(crate) description: Load<String>,
    pub(crate) description_for: Option<String>,
    /// Caption lines (start, text) of the video `transcript_for`; fetched when the Transcript tab
    /// is first shown. `transcript_query` searches them; `transcript_shown` is the line scrolled
    /// to when the list appeared (it follows the video only then, not while you read).
    pub(crate) transcript: Load<(f64, String)>,
    pub(crate) transcript_for: Option<String>,
    pub(crate) transcript_query: String,
    pub(crate) transcript_focus: FocusHandle,
    pub(crate) transcript_scroll: UniformListScrollHandle,
    pub(crate) transcript_shown: Option<usize>,
    /// Comments of the video `comments_for`; fetched when the Comments tab is first shown.
    pub(crate) comments: Load<Comment>,
    pub(crate) comments_for: Option<String>,
    /// How many comments the last fetch asked for; "Load more" raises it.
    pub(crate) comments_limit: usize,
    /// Latest uploads across subscriptions, for the "New uploads" row and per-channel counts.
    pub(crate) feed: Load<Video>,
    /// The logged-out home: a random anonymous mix (see `yt::anonymous`).
    pub(crate) anon: Load<Video>,
    pub(crate) seen: Seen,
    pub(crate) query: String,
    pub(crate) search: Load<Video>,
    pub(crate) search_focus: FocusHandle,
    pub(crate) save_filter: String,
    /// Ids of the playlists the open Save list may offer (YouTube's own list of ones you can add to):
    /// None while it loads, Some(None) when YouTube didn't say (then every playlist is shown).
    pub(crate) save_editable: Option<Option<HashSet<String>>>,
    pub(crate) save_focus: FocusHandle,
    /// Focused when no text field is, so keyboard shortcuts work.
    pub(crate) root_focus: FocusHandle,
    /// Videos queued with "+", played before the list's next video.
    pub(crate) up_next: Vec<Video>,
    /// Which list is shown under the player.
    pub(crate) lower: Lower,
    /// Vim mode: selected index in the left column's list, which list it is, and its scroll.
    pub(crate) vim_cursor: usize,
    pub(crate) vim_list: String,
    pub(crate) vim_scroll: UniformListScrollHandle,
    /// Chapters list scroll, and the chapter it last scrolled to.
    pub(crate) chapters_scroll: UniformListScrollHandle,
    pub(crate) chapter_shown: Option<usize>,
    /// First "g" of "gg" was pressed.
    pub(crate) vim_g: bool,
    /// First "y" of "yy" was pressed.
    pub(crate) vim_y: bool,
    /// Selection per list, restored when going back to it.
    pub(crate) vim_positions: HashMap<String, usize>,
    /// Clickable elements and their click actions, collected every frame (Vim mode) for `f`.
    pub(crate) hint_targets: Rc<RefCell<Vec<HintTarget>>>,
    /// The element the Tab focus ring is on (where it was last seen); None when the ring is off.
    pub(crate) kb_focus: Option<GBounds<GPixels>>,
    /// `[` / `]` are cycling in the lower pane's tabs (otherwise in the header tabs).
    pub(crate) cycle_lower: bool,
    /// Hint mode: the targets when `f` was pressed, and the letters typed so far.
    pub(crate) hints: Option<(Vec<HintTarget>, String)>,
    /// Picture-in-picture: mpv plays in its own small always-on-top window.
    pub(crate) pip: bool,
    /// Set while the video plays on a cast receiver (see `Casting`).
    pub(crate) casting: Option<Casting>,
    /// Closing the window was asked while casting: the dialog offers to stop the receiver too.
    pub(crate) quit_prompt: bool,
    /// Live filter for the left list (channels, a channel's videos, History), and its focus.
    pub(crate) list_filter: String,
    pub(crate) filter_focus: FocusHandle,
    /// Cursor and selection of the text field being edited.
    pub(crate) caret: Caret,
    /// The mouse's selection over a text block (title, description, comment): which block,
    /// its text, and where the press and the pointer are in it (see `textsel`).
    pub(crate) text_sel: Option<textsel::TextSel>,
    /// Set while the press that started `text_sel` is still down, so the root's own press
    /// handler — which runs after the block's, as the ancestor — doesn't clear it.
    pub(crate) text_press: bool,
    /// Search mode: the header shows a full-width search field instead of the tabs.
    pub(crate) searching: bool,
    /// Tab to return to when leaving search.
    pub(crate) prev_tab: Tab,
    /// Last queries, newest first.
    pub(crate) recent_searches: Vec<String>,
    /// Muted / notify-on-upload channels.
    pub(crate) flags: ChannelFlags,
    /// Re-checks the feed for notifications every few minutes.
    pub(crate) _notify_poll: Task<()>,
    /// Channel groups, and the one Subscriptions is filtered to.
    pub(crate) groups: Vec<ChannelGroup>,
    pub(crate) active_group: Option<String>,
    /// The group editor (membership chips) is open in the channel header.
    pub(crate) editing_groups: bool,
    /// Name being typed for a new group, and its field's focus.
    pub(crate) new_group: Option<String>,
    pub(crate) new_group_focus: FocusHandle,
    /// Group waiting for the second click on "Delete?".
    pub(crate) confirm_delete_group: Option<String>,
    /// Channel waiting for the second click on "Unsubscribe?".
    pub(crate) confirm_unsub: Option<String>,
    /// The lower pane (Recommended, Chapters, Up next) takes the whole right column.
    pub(crate) lower_full: bool,
    /// The lower pane is hidden; the player takes the whole right column.
    pub(crate) player_full: bool,
    /// The left column is hidden (its width setting is kept for when it comes back).
    pub(crate) left_collapsed: bool,
    /// The right column (player and lower pane) is hidden; the left one takes the window.
    pub(crate) right_collapsed: bool,
    /// Right-click menu on a subscription: the channel, where it was opened and when (it closes by itself).
    pub(crate) channel_menu: Option<(Group, gpui::Point<Pixels>, Instant)>,
    /// Right-click menu of a video in any list: the video, where it opened, and whether mpv's
    /// window must hide because the menu can reach over it (clicks outside the left column), and
    /// when the pointer was last on it: like the subscription menu, it closes by itself after the
    /// pointer has been away for 2 seconds.
    pub(crate) video_menu: Option<(Video, gpui::Point<Pixels>, bool, Instant)>,
    /// Right-click menu of a playlist in the list of playlists (Cast); closes like the others.
    pub(crate) playlist_menu: Option<(Group, gpui::Point<Pixels>, Instant)>,
    /// The list the right-clicked video is in, for "Cast from here"; None on the playing video.
    pub(crate) menu_queue: Option<Arc<[Video]>>,
    /// The playlists that menu offers for the video id, with whether it is in each; None while loading.
    pub(crate) menu_lists: Option<(String, Option<Vec<account::SaveOption>>)>,
    /// That menu was opened by right-clicking the playing video, so it also has the player rows.
    pub(crate) player_menu: bool,
    /// A quality picked for one video (its id, and a height or 0 for audio only) instead of the
    /// settings' maximum; dropped when another video starts.
    pub(crate) quality: Option<(String, u32)>,
    /// Likes and dislikes of the video `votes.0` (None while they load, or when there are none).
    pub(crate) votes: Option<(String, Option<(u64, u64)>)>,
    /// Settings → Network → Test connection's result.
    pub(crate) net_test: Option<String>,
    /// DeArrow's answer per video id; None while asked (or after a failure, not asked again).
    pub(crate) dearrow: HashMap<String, Option<dearrow::Branding>>,
    /// The player menu's quality list is open.
    pub(crate) quality_menu: bool,
    pub(crate) sleep: Option<Sleep>,
    /// Captions picked for one video in the player menu: its id, the language code and whether
    /// generated; the subtitles and the Transcript tab use them instead of Settings → Subtitles.
    pub(crate) sub_pick: Option<(String, String, bool)>,
    /// The player menu's subtitle list is open.
    pub(crate) sub_menu: bool,
    /// The caption tracks of the video `caption_langs.0` (None while they load).
    pub(crate) caption_langs: Option<(String, Option<Result<Vec<yt::CaptionLang>, String>>)>,
    /// The player menu's sleep timer list is open.
    pub(crate) sleep_menu: bool,
    /// The pointer is over the menu (it closes 2 seconds after the pointer is away).
    pub(crate) menu_hovered: bool,
    /// The keyboard shortcuts card (opened with ?).
    pub(crate) show_keys: bool,
    pub(crate) settings_filter: String,
    pub(crate) settings_focus: FocusHandle,
    /// Focus of the settings text fields, in TEXT_FIELDS order.
    pub(crate) field_focus: [FocusHandle; TEXT_FIELDS.len()],
    /// Latest fetch per list; older fetches of the same list are ignored.
    pub(crate) generations: HashMap<&'static str, u64>,
    pub(crate) history: History,
    pub(crate) current: Option<Video>,
    pub(crate) player: Player,
    /// When we last set the volume: mpv's reported volume is ignored for a moment after.
    pub(crate) volume_set: Instant,
    /// X11 child window mpv renders into; None until created, or on Wayland.
    pub(crate) embed: Option<Rc<RefCell<Embed>>>,
    /// The last-watched video has been loaded (paused) into mpv at startup.
    pub(crate) preloaded: bool,
    /// The video the pointer rests on, and when each video was last resolved ahead of time.
    pub(crate) hover_video: Option<String>,
    /// Subscriptions shows the streams live now (the Live tab) instead of the channel list.
    pub(crate) subs_live: bool,
    /// Videos played, oldest first (seeded from the watch history), and where the current one is:
    /// what the back / forward buttons walk.
    pub(crate) nav: Vec<Video>,
    pub(crate) nav_pos: Option<usize>,
    pub(crate) nav_moving: bool,
    pub(crate) prefetched: HashMap<String, Instant>,
    pub(crate) prefetching: usize,
    /// Videos or Shorts list of the History tab.
    pub(crate) history_view: ChannelView,
    /// Start time of a pasted link, taken by the next `start`.
    pub(crate) link_start: Option<f64>,
    /// A video whose captions the next tick should fetch.
    pub(crate) subs_wanted: Option<String>,
    /// Captions being downloaded for this video id.
    pub(crate) subs_loading: Option<String>,
    /// Downloaded captions, waiting for mpv to have that video open.
    pub(crate) subs_pending: Option<(String, PathBuf)>,
    /// The video id we found no captions for.
    pub(crate) subs_none: Option<String>,
    pub(crate) fullscreen: bool,
    /// A video was requested and mpv hasn't started playing it yet.
    pub(crate) loading: bool,
    /// When the current video was handed to mpv (for `UNBLOATEDTUBE_TIMING`).
    pub(crate) load_started: Instant,
    /// Since when the playing video hasn't moved, and from where (see `watch_stall`).
    pub(crate) stall: Option<(Instant, f64)>,
    /// The video reloaded for not moving, and whether that was said to have failed too.
    pub(crate) stall_retried: Option<(String, bool)>,
    /// Hide mpv's window while loading, so the old video's last frame doesn't linger.
    /// Not on a fresh mpv start: mpv keeps its window unmapped if ours is hidden then.
    pub(crate) hide_while_loading: bool,
    /// The list the current video was picked from, for Prev / Next.
    pub(crate) queue: Arc<[Video]>,
    pub(crate) state: Option<player::State>,
    pub(crate) thumbs_requested: HashSet<String>,
    /// Thumbnails known to be on disk: no file check for them on every frame.
    pub(crate) thumbs_ready: HashSet<String>,
    /// Thumbnails whose download failed: shown as a plain box, not a pulsing skeleton.
    pub(crate) thumbs_failed: HashSet<String>,
    /// Logged-in account for subscribe / like / save; loaded on first use.
    pub(crate) account: Option<Arc<Account>>,
    /// The Connect YouTube flow (Settings, and offered on first run).
    pub(crate) connect: Connect,
    /// The browser the Connect panel has selected (yt-dlp spec, e.g. "firefox").
    pub(crate) connect_browser: String,
    /// Pending cookies.txt path in Settings → Advanced.
    pub(crate) import_path: String,
    pub(crate) import_focus: FocusHandle,
    /// Result of the last cookies.txt import, shown under the field.
    pub(crate) import_msg: Option<(bool, String)>,
    /// Like/subscription state of the video with this id.
    pub(crate) status: Option<(String, VideoStatus)>,
    /// Video whose state was last requested, so a failure isn't retried every tick.
    pub(crate) status_requested: Option<String>,
    /// Video whose end already triggered autoplay.
    pub(crate) ended: Option<String>,
    /// The "Save to playlist" overlay is open.
    pub(crate) saving: bool,
    /// Result of the last account action, shown next to its buttons.
    pub(crate) notice: Option<String>,
    /// The notice as last seen by `tick`, and when it appeared; it's cleared after a few seconds.
    pub(crate) notice_seen: Option<(String, std::time::Instant)>,
    /// Downloads by video id: progress, then the saved file path or an error.
    pub(crate) downloads: HashMap<String, Download>,
    /// Videos saved with Download, newest first: the Downloads tab (stored as `downloads.json`).
    pub(crate) library: Vec<Saved>,
    pub(crate) ticks: u32,
    pub(crate) _poll: Task<()>,
}

impl Unbloated {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
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
            text_sel: None,
            text_press: false,
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

    /// Notices ("Link copied", "Saved to …") disappear after 4 seconds; a finished download's
    /// line after 6. Watching the value here keeps every place that sets them simple.
    pub(crate) fn expire_notices(&mut self, cx: &mut Context<Self>) {
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

    pub(crate) fn tick(&mut self, state: Option<player::State>, window: &mut Window, cx: &mut Context<Self>) {
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
}
