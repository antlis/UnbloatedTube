# Architecture

UnbloatedTube is one native process: a GPUI window, an mpv drawing into a child window of it,
and yt-dlp and YouTube's InnerTube API for data. This file is the map of the code; the README
describes the features.

## Layers

```
app/ + ui/   GPUI: state, behaviour, drawing        (what the user sees and presses)
backend/     no GPUI: YouTube, mpv, cast, disk      (what the app asks for)
platform/    the only OS-specific code              (Linux/X11 today)
```

Code may use the layers below it, never above: `platform` <- `backend` <- `app` / `ui`. The one
exception is `platform/mpris.rs`, which hands its requests over as `backend::cli::Command`s, the
same type the command line uses. A cross-platform port starts in `platform/`; the compiler then
lists what is still missing.

## Modules

| Path | Role |
| --- | --- |
| `main.rs` | Entry point: the command line and the window. Everything else is in the three modules below. |
| `app/` | The GPUI app. `mod.rs` holds its state (`Unbloated`) and the per-tick loop; one file per feature: `playback`, `navigation` (Next, back / forward), `casting`, `browse` (tabs, lists, search, links), `subscriptions` (flags, groups, notifications), `session` (login, like, save, Watch later), `library` (downloads), `subtitles`, `video_info`, `input` (shortcuts, Vim mode, hint mode), `menus`, `layout`, `images`, `settings`, `commands` (command line and media-key requests). `app/view/` is the rendering: the player, lists, settings, menus, overlays, description / transcript / comments. |
| `ui/` | Pieces that don't depend on the app's state: theme colours and the tables of toggles and shortcuts, text editing, small widgets (tooltips, skeletons, icon buttons), `icons.rs` (single-colour SVG icons compiled into the binary) and `textsel.rs` (selecting text with the mouse). |
| `backend/yt.rs` | Lists: the subscriptions feed, playlists, recommendations, search, channel tabs and the subscribed-channels list come from `account.rs` (InnerTube) page by page, with `yt-dlp --flat-playlist -j` as the fallback, streamed line by line. Also downloads. History comes from `account.rs`, with `:ythistory` as the fallback. |
| `backend/account.rs` | Account actions and the watch history (with Shorts and each entry's day, which yt-dlp's `:ythistory` lacks) through YouTube's internal InnerTube API, authenticated like the web app: browser cookies plus a `SAPISIDHASH` header. Cookies stay in memory. |
| `backend/auth.rs` | The login source (`auth.json`): browser or cookies file. Browser login exports the cookies with yt-dlp and remembers the keyring suffix that worked (e.g. `brave+gnomekeyring`). |
| `backend/player.rs` | One long-lived mpv process, controlled over its JSON IPC socket; builds mpv options from settings; restarts mpv only when options change. |
| `backend/store.rs` | `config.toml`, `settings.json`, watch history with resume positions, seen videos, groups, cached lists. |
| `backend/thumbs.rs` | Downloads and caches thumbnails and avatars (and the storyboard sheets). |
| `backend/storyboard.rs` | The seek bar's preview pictures: reads YouTube's storyboards (sheets of small frames) from the lookup the yt-dlp shim kept, says which frame shows a moment, and turns downloaded sheets into raw pixels (`cache/sb-raw/`) that mpv's own controls (`controls.lua`, via `overlay-add` and the `user-data/unbloated/storyboard` property) draw above the bar. The bar under the video draws them in GPUI instead. |
| `backend/proxy.rs` | The local proxy behind Settings → Network: Bypass (the TLS ClientHello split into two records and/or two TCP segments, inside the server name) or an upstream SOCKS5/HTTP proxy; local addresses direct. mpv's stream links come to it as http:// and it makes the TLS connection (OpenSSL). |
| `backend/prefetch.rs` | Looks videos up before they are played and keeps the answers; mpv's (and the app's own) yt-dlp calls go through this program, which answers from them or runs yt-dlp, by a Python helper that stays running when it can. Unix by design (file descriptors, locks). |
| `backend/cast.rs`, `backend/cli.rs`, `backend/dearrow.rs`, `backend/links.rs`, `backend/http.rs` | Cast receivers (a command or the bot's remote API); the command line and its socket; DeArrow titles and thumbnails; clickable parts of text; the one shared HTTP client. |
| `platform/` | The operating system's side: `embed.rs` creates an X11 child window inside the app's window for `mpv --wid` and keeps it over the player area, and `embed_wayland.rs` with `libmpv.rs` does the same job under Wayland (mpv as a library, drawn into a subsurface); `mpris.rs` is the media keys over D-Bus; `ipc.rs` (local sockets), `process.rs` (mpv dies with the app, own executable), `paths.rs`, `fs.rs` (private files, locks) and `desktop.rs` (open a link, default browser). This is where a port to another system starts. |

`main.rs` also holds the two pieces that run before any window exists: the command line
(`unbloatedtube <link>` and the control commands, sent to a running instance over a socket) and
the yt-dlp shim mode (see *The yt-dlp shim*).

## How the app is put together

**One state struct, many files.** `Unbloated` (`app/mod.rs`) owns all state: lists, the player,
settings, menus, text fields. Its behaviour is spread over `impl Unbloated` blocks, one file per
feature under `app/`; each starts with `use super::*;`, so it sees the imports of `app/mod.rs`.
The drawing is more `impl Unbloated` blocks in `app/view/`. Items one file defines for another
are `pub(crate)`; fields of the state struct stay private to `app/`.

**Nothing blocks the UI thread.** A request is started from a handler with
`cx.background_executor().spawn(...)` (yt-dlp, InnerTube, mpv's socket, files), and its result
is applied back on the UI thread with `cx.spawn(...)` and `this.update(...)`. `fetch` does it for
lists, `with_account` for account calls. `tick` (every 250 ms, in `app/mod.rs`) asks mpv for the
playback state on a background thread and reacts to the answer.

**Lists are loaded as `Load<T>`** (`app/state.rs`): idle, loading (with what already arrived),
ready or failed. A `generation` per list makes a slow answer to an old request harmless.

**Input.** `app/input.rs` holds the shortcuts, Vim mode and hint mode. Hint mode works from
`HintReg`: every clickable element registers itself with `on_click_hinted` while it is drawn, and
the hint keys then run the same handlers. The tables of toggles and shortcuts are data in
`ui/tables.rs`; the Settings page and the `?` sheet are built from them.

**Colours** come from `themed(NAME)` (`ui/theme.rs`), which swaps the palette for the light theme.

## Playing a video

1. `play` / `start` (`app/playback.rs`) hand the URL and the settings' mpv options to
   `backend/player.rs`, which keeps one long-lived mpv, restarts it only when its options change,
   and controls it over mpv's JSON IPC socket.
2. mpv draws into an X11 child window (`platform/embed.rs`) that the app keeps over the player area.
   Under Wayland there is no such window: the app runs mpv as a library inside its own process
   (`platform/libmpv.rs`) and draws into a subsurface of its window (`platform/embed_wayland.rs`, see
   *Video under Wayland*).
3. mpv finds the streams by running yt-dlp. It is pointed at this program instead (see below).
4. The app polls mpv for position, pause, chapters and so on; the hover bar over the video is a Lua
   script (`controls.lua`) loaded into mpv that talks back through mpv user data.

### The yt-dlp shim

`backend/prefetch.rs`. Looking a video up with yt-dlp takes seconds, so:
- the app runs the lookup ahead of time (the row under the pointer, the first rows of a list, the
  next video) and keeps the answer for a while;
- mpv, and the app's own yt-dlp calls, run *this program* as their yt-dlp (`UNBLOATED_YTDL_SHIM`
  in the environment, handled first thing in `main`): it answers from a kept answer, or runs the
  request, with the manifests skipped for an ordinary finished video (never for a live one);
- when the installed yt-dlp is a Python script, a helper (`ytdl_helper.py`) keeps it loaded and
  forks per request, so a request starts in milliseconds.

It uses file descriptors and locks, so it is Unix by design; it is the one place outside
`platform/` that touches them.

## The account session

`backend/account.rs` talks to InnerTube with the browser's cookies, read once with yt-dlp and kept
in memory (`shared`) and, for up to 12 hours, in a private file. YouTube rotates those cookies, and
a login it no longer accepts still gets answers, as a signed-out visitor: empty lists and 401s.
So `post` turns such an answer into `SIGNED_OUT` for requests that need the login (not for public
pages), and the callers (`with_shared` for lists, `with_account` for actions) re-read the browser
once and try again; if that fails too, the Settings page shows the failed step.

The copy is also read again every 15 minutes (`keep_fresh`, a thread started in `main`), and the
yt-dlp shim (`backend/prefetch.rs`) deletes the kept file when a run that used it failed
(`forget_kept`), so the next run reads the browser. Every list goes through the account first
(`yt::playlists`, `history`, `subscriptions`); yt-dlp is the fallback, never the first choice,
because it would use its own copy of the cookies and could disagree with the app.

The app never owns the login, it borrows the browser's, so it can only be as fresh as what the
browser wrote to disk. The README's *Limits of the cookie login* lists what can go wrong; "Sign in
again" (`sign_in_again` in `app/session.rs`) drops the app's copies and sends the user to
YouTube's sign-in in the browser the login is read from.

## Casting

`backend/cast.rs` talks to a receiver: a command from `config.toml` (`catt` and the like) or the
Telegram bot's remote API (play, queue, status, controls). `app/casting.rs` is the app's side: it
polls the receiver, shows its state in place of the local player, sends a list in chunks and
marks what played in the YouTube history.

## Platform seam

Everything operating-system-specific is in `platform/`:

| Module | What it does | Linux today |
| --- | --- | --- |
| `embed.rs` | the surface mpv draws into | X11 child window (`x11rb`), or the Wayland subsurface below |
| `embed_wayland.rs`, `libmpv.rs` | the video under Wayland: a `wl_subsurface`, EGL, and mpv as a library | `wayland-client` on gpui's connection; EGL, libwayland-egl and libmpv loaded at run time |
| `mpris.rs` | media keys, the desktop's player widget | D-Bus (`zbus`) |
| `ipc.rs` | local sockets, stale-socket cleanup, passing file descriptors | Unix sockets |
| `process.rs` | mpv ends with the app, group, own executable and stderr | `prctl`, `/proc` |
| `fs.rs` | files only the user can read, file locks | mode 0600, `flock` |
| `desktop.rs` | open a link, the default browser | `xdg-open`, `xdg-settings` |
| `paths.rs` | where per-session files go | `$XDG_RUNTIME_DIR` |

`libc` and `x11rb` / `zbus` are dependencies for Unix and Linux only (`Cargo.toml`). A port gives
each module a version for its system. Not attempted yet.

## Video under Wayland

mpv's `--wid` is X11-only, so `platform/embed_wayland.rs` does what the X11 child window does in
another way:

- gpui hands out the app window's `wl_display` and `wl_surface`; the app makes a `wl_subsurface`
  of it (on gpui's own connection, through `wayland-backend`'s foreign-display support), with an
  empty input region so the pointer goes to the app's window underneath.
- A render thread owns the EGL context (desktop OpenGL 3.3, else OpenGL ES 3) and mpv's render
  context. mpv runs in this process (`libmpv.rs`, loaded at run time) with the same options as the
  mpv program (`Player::play_in_process` turns `--name=value` into library options, joining
  repeated `--script` / `-append` ones) and the same IPC socket, so the rest of the app doesn't
  know the difference. The UI thread only places and sizes the subsurface (`WaylandVideo::place`).
- The subsurface is sized with `wp_viewporter` when the compositor has it, so its buffer is the
  video area in exact device pixels and the compositor scales it to the logical size; without it,
  the buffer is `ceil(scale)` times the logical size (whole numbers only) and can be a touch soft
  at fractional scales.
- mpv has no window to hand the pointer to, so the app passes it on: `Player::pointer` sends
  `mouse x y [button]` commands from the video area's mouse handlers in `app/view/mod.rs`.
- Missing libmpv or EGL: `Embed::new` returns None and mpv opens in its own window.
- Testing without a Wayland desktop: headless sway (`WLR_BACKENDS=headless`), `grim` for
  screenshots, and a virtual pointer client that stays alive (one that exits drops the seat's
  pointer, and the app sees enter/leave without motion).

## Where things go

- A feature's behaviour: the matching file under `app/`, or a new one (`mod name;` in `app/mod.rs`).
  Its drawing: `app/view/`.
- A setting: the field in `Settings` (`backend/store.rs`, with a default) and a row in a `*_TOGGLES`
  table (`ui/tables.rs`).
- Anything that asks YouTube, runs a program or touches the disk: `backend/`, called from a
  background thread.
- Anything that differs between operating systems: `platform/`.
- A user-visible change: a line in `CHANGELOG.md`.

## Checking a change

`cargo test` runs the unit tests (link parsing, cast, command line, platform helpers). The UI has
none: run the app on a private X display (Xvfb with a window manager, private config, data and
cache folders, lavapipe for Vulkan), drive it with `xdotool` and compare screenshots. See
`CLAUDE.md` for the details.
