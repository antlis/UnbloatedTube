# unbloated-youtube

A lightweight, configurable YouTube desktop client, written in Rust with
[GPUI](https://crates.io/crates/gpui) (the UI framework behind the Zed editor), with
[mpv](https://mpv.io) for playback and [yt-dlp](https://github.com/yt-dlp/yt-dlp) for data.

It is **keyboard-driven**: playback, lists, tabs, panes and column layout all have shortcuts;
`Tab` moves a visible focus ring through every button, row and switch (`Enter` presses it); and an
optional Vim mode adds Vimium-style [hint mode](#vim-mode) to click anything without the mouse.

## Why

I wanted YouTube as **its own app**, so that YouTube tabs stop piling up in my browser, and
with a UI that does what I need and nothing else.

The YouTube web UI gets in the way of the basics:

- **Playlists are hard to manage.** Adding a video to a playlist opens a small popup with no
  search, so with many playlists you scroll and hunt. Seeing a playlist's contents, or
  removing a video, is several clicks deep.
- **Subscriptions are buried.** The channel list is tucked away in a sidebar or a separate
  page, and the subscriptions feed mixes everything into one endless scroll.
- **Shorts everywhere.** I'm usually not interested in them, and the web UI keeps pushing them.
- **Recommendations and ads.** The home page is a recommendation feed, and videos have
  sponsor reads in them.

So here most things can be turned off in Settings: Shorts, recommendations, whole tabs, every
button under the player, and sponsor segments (via SponsorBlock). What's left is a channel
list, your playlists, your history, search and a player.

## Features

### Browsing

- **Subscriptions as a channel list**: avatars, A–Z, with channels that have new uploads on
  top with an unseen count. Click a channel for its videos (**Videos | Shorts** tabs).
- **Unsubscribe from the list**: right-click a channel, then confirm with a second click.
- **New uploads**: one feed of the latest videos from all your subscriptions.
- **Channel groups**: put channels into groups (e.g. *Music*, *Tech*) and filter the channel
  list and the New uploads feed by group.
- **Mute a channel**: hide its uploads from New uploads without unsubscribing.
- **Upload notifications**: turn the bell on in a channel's header to get a desktop
  notification when it uploads (checked every 5–60 minutes, configurable).
- **Playlists**: Watch later, Liked videos and your own playlists, loaded in full.
- **History**: *Continue watching* (started but not finished, resizable with a drag bar), then
  everything you watched, here and on YouTube.
- **Search**: a search mode with recent searches. Nothing is sent to YouTube until you press
  Enter.
- **Filter any list as you type** (Ctrl+F): channels, playlists, a channel's or playlist's
  videos, history. This is local and instant, with no request to YouTube.
- **Video info**: views, upload date and subscriber count for the playing video (needs your login),
  view counts in lists that provide them, subscriber counts in the channel list. Each is a Settings toggle.
- Watched videos are dimmed, and partly watched ones show a progress bar on the thumbnail.
- Lists appear instantly from a local cache and refresh in the background; long lists stream
  in as yt-dlp produces them.

### Playlists done right

- **Save to playlist** opens a full-height picker **with search**: type a few letters, press
  Enter.
- **Remove from playlist**: hover a video in an open playlist and click the trash icon (in
  Liked videos this unlikes it).
- **Add a whole playlist to Up next** with one button in the playlist header.
- A notice confirms every action (or says why YouTube refused it, e.g. for playlists you
  saved from someone else, which you can't edit).

### Player

- mpv **embedded in the window**, with seek bar, chapters (hover for title and time), time
  display, prev/next, ±10 s, fullscreen and speed.
- Opens on **the last video you watched, at the position you left it**; every video resumes
  where you stopped.
- **Up next** queue (add with the `+` on any row); otherwise Next/autoplay continues through the
  list you picked the video from.
- **Picture-in-picture**: move playback into a small always-on-top mpv window and keep
  browsing.
- **Volume**: mute button and a volume bar next to the speed button, or `↑`/`↓` (`+`/`-` in Vim
  mode), also while the pointer is over the video. Remembered between runs.
- **Chapters tab** under the player for videos that have chapters: click one to jump there; the
  current chapter is highlighted and kept in view.
- **Comments tab** under the player, off by default (Settings → Comments): the top 40 comments are
  fetched only when you open the tab, and "Load more comments" at the bottom gets 40 more.
- **Resizable player**: drag the divider between the video and the lower pane anywhere, up to
  the top; `E` gives the lower pane the whole right column and back; `⇧E` does the same for the player. The left column collapses too
  (`B`, or drag its divider to the edge), and so does the right one (`⇧B`, or drag the divider
  to the right edge).
- **Hover controls**: move the pointer over the video and a YouTube-style bar appears: a seek
  line, then play/pause, previous/next, mute and volume and the time on the left, and ±10 s, speed,
  picture-in-picture and fullscreen on the right. It hides itself after a moment. With it on, the
  row of playback buttons under the video is gone and only the account/action buttons remain; with
  it off (Settings, *Hover controls on the video*) you get the two button rows instead. The bar is
  drawn by mpv from a small bundled script, since the video is a separate native window the app
  can't draw over.
- **No mpv chrome by default**: mpv's own on-screen controls and key bindings are switched off
  over the video, so the app's shortcuts work there too. Turn them back on in Settings (*mpv
  controls and hotkeys*) if you want mpv's `osc`, wheel volume, `s` for screenshots and so on.
- Click the video to pause, double-click for fullscreen. The app's own shortcuts work with the
  pointer over the video too, since mpv's key bindings are off by default.
- **Recommendations** under the player (your YouTube home feed), or turn them off.

### Account actions

Subscribe/unsubscribe, save to playlist, like, dislike, copy link (also at the current time), open in browser and download, all as
icon buttons with tooltips. Each one can be hidden in Settings (like and dislike are hidden by
default).

### Settings

Everything is a toggle or a field on the Settings page, which has its own search box:

| Group | Options |
| --- | --- |
| Tabs & lists | Subscriptions, Playlists, History, Recommendations, Chapters, Comments (off by default), **Shorts** (everywhere) |
| Buttons | Subscribe, Save to playlist, Like, Dislike, Volume, Share, Share at current time, Open in browser, Download |
| Player | Max quality (480p–4K), autoplay, **audio only**, prefer hardware-friendly codecs (skip AV1), hardware decoding, hover controls on the video, mpv's own controls and hotkeys, speed |
| SponsorBlock | **Skip sponsored segments**, choosing which: sponsor, self-promotion, like/subscribe reminders, intro, credits, preview, filler, non-music |
| Video info | **Views**, **upload date** (playing video), **subscriber counts** (channels) |
| Other | Subtitles language, extra mpv options, download folder, upload notifications and their interval, Vim mode, window buttons, **light theme** |

Window layout (column width, player height, Continue watching height) is set by dragging and
remembered.

### Keyboard

`?` shows every shortcut. Defaults:

| Key | Action |
| --- | --- |
| Space / K | Play / pause |
| ← / → | Back / forward 5 s |
| J / L | Back / forward 10 s |
| F | Fullscreen |
| M | Mute |
| ↑ / ↓ | Volume up / down 5% (`+` / `-` in Vim mode) |
| N / P | Next / previous |
| C | Copy the video's link |
| ⇧C | Copy the link at the current time (`y t` in Vim mode) |
| E | Lower pane (Recommended, Chapters, Comments, Up next) full height, and back (`e` in Vim mode) |
| ⇧E | Player full height (hide the lower pane), and back |
| Tab / ⇧Tab | Move a focus ring through every clickable thing (tabs, rows, buttons, switches); Enter or Space presses it, Esc clears it |
| 1 – 4 | Switch tab: Subscriptions, Playlists, History, Settings |
| 5 – 8 | Switch lower pane tab: Recommended, Chapters, Comments, Up next (as shown) |
| [ / ] | Previous / next tab: through 1–4, then 5–8, wrapping around |
| B | Hide or show the left column (`b` in Vim mode) |
| ⇧B | Hide or show the right column |
| / | Search |
| Ctrl+F | Filter the list |
| Esc | Close / back |

Text fields support Home/End, arrows (Ctrl: by word), Shift to select, Ctrl+A/C/X/V.

#### Vim mode

Turn it on in Settings. It adds `j`/`k`, `gg`/`G`, `Ctrl-d`/`Ctrl-u` to move through lists,
`Enter`/`l` to open, `h` to go back, `H`/`L` to switch tabs, `x` to queue a video, `yy` to copy
the link (`yt` at the current time), and the rest of the keys marked "Vim mode" above.

**Hint mode.** Press `f` and every clickable thing on screen (tabs, rows, buttons, chips, switches)
gets a short letter label; type the label to click it, `Esc` to cancel. It is the same idea as
the link hints in browser extensions like [Vimium](https://github.com/philc/vimium),
[Tridactyl](https://github.com/tridactyl/tridactyl) and
[Surfingkeys](https://github.com/brookhong/Surfingkeys): you never need the mouse, even for
things that have no shortcut of their own. See
[Vimium's description of link hints](https://github.com/philc/vimium#keyboard-bindings)
(the `f` command) if you haven't used one.

## Running

Built and tested on Linux with X11; the embedded player relies on X11 window embedding
(`mpv --wid`), and Wayland is untested. Needs `mpv`, `yt-dlp` (recent: YouTube breaks old versions
quickly) and `deno` (yt-dlp uses it for YouTube's JavaScript challenges).

### NixOS

`shell.nix` provides everything, with yt-dlp, deno and mpv pinned to a recent nixpkgs-unstable:

```sh
nix-shell --run 'cargo build --release'
./run          # launcher: caches the nix-shell environment, so starts take <1 s
```

`./run` captures the nix-shell environment into `.nix-env` once (and again when
`shell.nix` changes) and starts the newest build, so you can bind it to a key or a desktop
entry.

### Login

Subscriptions, playlists, history, recommendations and the account buttons need your YouTube
login, taken from your browser's cookies. Create `~/.config/unbloated-youtube/config.toml`:

```toml
cookies_from_browser = "brave+gnomekeyring"   # yt-dlp syntax: "firefox", "chromium", …
# cookies_file = "/path/to/cookies.txt"       # or an exported Netscape cookies file
```

Without it, search and playback still work.

## Architecture

```
┌──────────────────────── unbloated-youtube (one process) ────────────────────────┐
│  GPUI app (main.rs): views, state, keyboard, settings                           │
│     │                         │                          │                      │
│     │ background threads      │ background threads       │ X11 child window     │
│     ▼                         ▼                          ▼                      │
│  yt.rs ── spawns ──► yt-dlp   account.rs ── HTTPS ──►     embed.rs               │
│  (lists, search,    (JSON     YouTube InnerTube API       (mpv draws into it)    │
│   downloads)        lines)    (subscribe, like, save)                           │
│                                                          player.rs ── JSON IPC ─┼──► mpv
│  store.rs: config, settings, history, caches on disk                            │
│  thumbs.rs: thumbnail cache        icons.rs: built-in SVG icons                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

| Module | Role |
| --- | --- |
| `main.rs` | The GPUI app: all views (tabs, lists, player panel, settings, overlays), state, shortcuts, Vim mode and hints, notices. |
| `yt.rs` | Runs `yt-dlp --flat-playlist -j` for subscriptions, feeds, channels, playlists, history, recommendations and search, streaming entries line by line; also downloads. |
| `account.rs` | Account actions through YouTube's internal InnerTube API, authenticated like the web app: browser cookies plus a `SAPISIDHASH` header. Cookies stay in memory. |
| `player.rs` | One long-lived mpv process, controlled over its JSON IPC socket; builds mpv options from settings; restarts mpv only when options change. |
| `embed.rs` | Creates an X11 child window inside the app's window for `mpv --wid`, and keeps it positioned over the player area. |
| `store.rs` | `config.toml`, `settings.json`, watch history with resume positions, seen videos, groups, cached lists. |
| `thumbs.rs` | Downloads and caches thumbnails and avatars. |
| `icons.rs` | Small single-color SVG icons compiled into the binary. |

### Design notes

- **Nothing blocks the UI thread.** yt-dlp runs, InnerTube requests and mpv queries all happen
  on background threads; results come back to the UI as they arrive.
- **Streaming lists.** yt-dlp prints one JSON object per entry; the UI shows entries every
  150 ms while the rest loads. A cached copy of each list is shown immediately on revisit.
- **mpv state polling.** Position, duration, pause, chapters and fullscreen are queried over
  IPC every 250 ms in the background. mpv answers late while it opens a video, which is why
  this never happens on the UI thread.
- **Smooth streaming.** YouTube throttles a single long download to ~150 KB/s. mpv is given
  `request_size=10 MB` (FFmpeg 9), so it fetches in range requests at full speed.
- **Fast yt-dlp starts.** `PYCRYPTODOME_DISABLE_GMP=1` saves ~1.7 s per yt-dlp run on NixOS
  (pycryptodome otherwise invokes the C compiler looking for libgmp).
- **mpv dies with the app.** It's started with `PR_SET_PDEATHSIG`, so closing the window never
  leaves audio playing.
- **SponsorBlock** runs inside mpv as the `sponsorblock_minimal` script, with the categories
  you chose.

## Files

- `~/.config/unbloated-youtube/`: `config.toml` (login), `settings.json` (everything from the Settings page)
- `~/.local/share/unbloated-youtube/`: watch history with resume positions, seen videos, groups, channel flags, Up next, recent searches
- `~/.cache/unbloated-youtube/`: thumbnails, cached lists, `mpv.log`

Folders from the app's old name (`jtube`) are moved over automatically on first start.
