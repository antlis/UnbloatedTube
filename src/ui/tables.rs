use crate::backend::store::Settings;

/// Settings page toggles: label, hint, field.
pub(crate) type Toggle = (&'static str, &'static str, fn(&mut Settings) -> &mut bool);

pub(crate) const TOGGLES: [Toggle; 14] = [
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

pub(crate) const PLAYER_TOGGLES: [Toggle; 8] = [
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
pub(crate) const ACCOUNT_ONLY: [&str; 9] = [
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

pub(crate) const SUB_TOGGLES: [Toggle; 2] = [
    ("Subtitles", "Show subtitles on videos (the CC button or V turns them off for one video)", |s| &mut s.subtitles),
    ("Auto-generated captions", "Also use YouTube's automatic and translated captions when a video has none of its own", |s| &mut s.sub_auto),
];

/// Subtitle sizes: label and mpv's `sub-scale`.
pub(crate) const SUB_SCALES: [(&str, f32); 4] = [("Small", 0.8), ("Normal", 1.0), ("Large", 1.4), ("Huge", 1.8)];
pub(crate) const QUALITIES: [u32; 5] = [480, 720, 1080, 1440, 2160];
pub(crate) const NOTIFY_MINUTES: [u32; 4] = [5, 15, 30, 60];

pub(crate) const NOTIFY_TOGGLES: [Toggle; 1] = [(
    "Upload notifications",
    "Desktop notification when a channel with the bell on (in its header) uploads",
    |s| &mut s.notifications,
)];

/// SponsorBlock categories: label, API name.
pub(crate) const SEGMENTS: [(&str, &str); 8] = [
    ("Sponsor", "sponsor"),
    ("Self-promotion", "selfpromo"),
    ("Like/subscribe reminders", "interaction"),
    ("Intro", "intro"),
    ("Credits", "outro"),
    ("Preview/recap", "preview"),
    ("Filler", "filler"),
    ("Non-music in music videos", "music_offtopic"),
];

pub(crate) const SPEEDS: [f32; 6] = [0.75, 1.0, 1.25, 1.5, 1.75, 2.0];

/// Settings text fields: label, hint, field.
pub(crate) const TEXT_FIELDS: [(&str, &str, fn(&mut Settings) -> &mut String); 6] = [
    ("Cast command", "catt -d \"Living Room\" cast {url}", |s| &mut s.cast_command),
    ("Subtitle language", "Code, e.g. en or ru, or several (en,ru); empty uses your system language", |s| &mut s.sub_lang),
    ("Extra mpv options", "e.g. --volume=70 --deband", |s| &mut s.mpv_args),
    ("Download folder", "Empty for your Downloads folder; ~/ works", |s| &mut s.download_dir),
    ("Hide videos with words", "Comma-separated, e.g. reaction, prank, #shorts: hidden from feeds, channels, recommendations and search", |s| &mut s.hide_words),
    ("Proxy address", "socks5://127.0.0.1:9050 (Tor), socks5://127.0.0.1:1080 (ByeDPI), or http://host:port; user:password@ before the host if it needs a login", |s| &mut s.proxy),
];

/// Settings → Network → Connection: label, value.
pub(crate) const CONNECTIONS: [(&str, &str); 3] = [("Direct", "direct"), ("Bypass slowdown", "bypass"), ("Proxy", "proxy")];

/// How Bypass splits the first packet: label, value.
pub(crate) const BYPASS_METHODS: [(&str, &str); 3] = [("TLS split (recommended)", "tls"), ("TLS + TCP split", "both"), ("TCP split", "tcp")];

/// Proxy presets: label, address (the programs' default ports).
pub(crate) const PROXY_PRESETS: [(&str, &str); 2] = [("Tor", "socks5://127.0.0.1:9050"), ("ByeDPI", "socks5://127.0.0.1:1080")];

pub(crate) const BUTTON_TOGGLES: [Toggle; 13] = [
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

pub(crate) const INFO_TOGGLES: [Toggle; 4] = [
    ("Views", "View counts on videos (the playing video needs your login)", |s| &mut s.show_views),
    ("Upload date", "When the playing video was posted (needs your login)", |s| &mut s.show_date),
    ("Likes and dislikes", "Counts for the playing video from the Return YouTube Dislike project (dislikes are its estimate; it learns which video you watch)", |s| &mut s.show_votes),
    ("Subscribers", "Subscriber counts of channels", |s| &mut s.show_subs),
];

pub(crate) const DEARROW_TOGGLES: [Toggle; 2] = [
    ("Better titles (DeArrow)", "Titles the community wrote to replace clickbait ones; hover a title for the original. Asks DeArrow by a hash prefix of the video id, so it doesn't learn which video", |s| &mut s.dearrow_titles),
    ("Better thumbnails (DeArrow)", "A frame from the video the community picked, instead of the clickbait thumbnail", |s| &mut s.dearrow_thumbs),
];

/// Keyboard shortcuts (also listed in Settings). Keys reach mpv instead while the pointer is
/// over the video; mpv's own defaults there are similar (Space, arrows, f).
pub(crate) const SHORTCUTS: [(&str, &str); 30] = [
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
pub(crate) const SHEET_GROUPS: [(&str, usize, usize); 2] = [("Playback", 9, 0), ("Navigation", 15, 1)];

/// Vim mode's keys (case matters: ⇧ means Shift).
pub(crate) const VIM_SHORTCUTS: [(&str, &str); 36] = [
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

pub(crate) const VIM_SHEET_GROUPS: [(&str, usize, usize); 3] = [("Playback", 10, 0), ("Navigation", 16, 1), ("General", 4, 0)];
