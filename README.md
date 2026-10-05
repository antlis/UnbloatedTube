# unbloated-youtube

A lightweight, configurable YouTube desktop client, written in Rust with
[GPUI](https://crates.io/crates/gpui) (the UI framework behind the Zed editor), with
[mpv](https://mpv.io) for playback and [yt-dlp](https://github.com/yt-dlp/yt-dlp) for data.

It is **keyboard-driven**: playback, lists, tabs, panes and column layout all have shortcuts;
`Tab` moves a visible focus ring through every button, row and switch (`Enter` presses it); and an
optional Vim mode adds Vimium-style [hint mode](#vim-mode) to click anything without the mouse.

![unbloated-youtube: History on the left; the player, its buttons and the Recommended, Watch later, Comments and Up next tabs on the right](docs/screenshot.png)

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
  list and the New uploads feed by group. Right-click a channel to tick the groups it belongs to;
  a folder icon in the list marks channels that are in a group (hover for the names). Groups are
  kept only in this app (`groups.json` in the data folder), not on YouTube.
- **Mute a channel**: hide its uploads from New uploads without unsubscribing.
- **Upload notifications**: turn the bell on in a channel's header to get a desktop
  notification when it uploads (checked every 5–60 minutes, configurable).
- **Playlists**: Watch later, Liked videos and your own playlists, loaded in full.
- **History**: *Continue watching* (started but not finished, resizable with a drag bar), then
  your YouTube history in YouTube's own order, each entry with the day you watched it
  ("Today", "Saturday", …), then what only this app has seen. With Shorts on, Videos and
  Shorts have their own tabs.
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
- **Watch later tab** under the player (off by default, Settings → Watch later tab): your Watch
  later list, loaded when you open the tab, with a remove button on each row.

### Account actions

Subscribe/unsubscribe, save to playlist, Watch later (one click, `W`; also on every video row), like, dislike, copy link (also at the current time), open in browser and download, all as
icon buttons with tooltips. Each one can be hidden in Settings (like and dislike are hidden by
default).

### Settings

Everything is a toggle or a field on the Settings page, which has its own search box:

| Group | Options |
| --- | --- |
| Account | **Connect YouTube**: pick a browser and test its session, or import a `cookies.txt` (see [Login](#login)) |
| Tabs & lists | Subscriptions, Playlists, History, Recommendations, Chapters, Comments and Watch later tabs (both off by default), **Shorts** (everywhere) |
| Buttons | Subscribe, Save to playlist, Watch later, Like, Dislike, Volume, Share, Share at current time, Open in browser, Download |
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
| O | Open the video in your browser (`o` in Vim mode) |
| W | Add the video to Watch later (`w` in Vim mode) |
| E | Lower pane (Recommended, Chapters, Watch later, Comments, Up next) full height, and back (`e` in Vim mode) |
| ⇧E | Player full height (hide the lower pane), and back |
| Tab / ⇧Tab | Move a focus ring through every clickable thing (tabs, rows, buttons, switches); Enter or Space presses it, Esc clears it |
| 1 – 4 | Switch tab: Subscriptions, Playlists, History, Settings |
| 5 – 9 | Switch lower pane tab: Recommended, Chapters, Watch later, Comments, Up next (as shown) |
| [ / ] | Previous / next tab: through 1–4, then 5–9, wrapping around |
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
(`mpv --wid`), and Wayland is untested. Windows and macOS are planned (see [TODO](#todo)).
Needs `mpv`, `yt-dlp` (recent: YouTube breaks old versions quickly) and `deno` (yt-dlp uses it
for YouTube's JavaScript challenges).

### Prebuilt binary

Releases on GitHub have a Linux x86_64 archive (built on Ubuntu 22.04, so it needs glibc 2.35 or
newer: Ubuntu 22.04+, Debian 12+, Fedora, Arch). Unpack it and run `unbloated-youtube`; the
`.desktop` file and icon in it are for your launcher. Install separately:

- the tools above: `mpv`, `yt-dlp`, `deno`
- libraries the binary links against, which most desktops already have: xkbcommon (and its X11
  part, `libxkbcommon-x11`) and xcb; it also loads the Vulkan loader and Wayland client library
  at run time, so install those and a Vulkan driver for your GPU (Mesa's, or
  `mesa-vulkan-drivers`)

Check the download with the `.sha256` file next to it (`sha256sum -c`).

### NixOS

To install it, `default.nix` builds a package (`package.nix`) that puts mpv, yt-dlp and deno on its
`PATH` and installs the desktop entry and icon; they come from a pinned nixpkgs-unstable:

```sh
nix-build              # ./result/bin/unbloated-youtube
nix-env -f . -i        # or add it to environment.systemPackages / home.packages
```

This compiles all ~730 crates (about 25 minutes on 2 cores, with several GB of disk); see the TODO
on prebuilt binaries. For development, `shell.nix` provides the same tools:

```sh
nix-shell --run 'cargo build --release'
./run          # launcher: caches the nix-shell environment, so starts take <1 s
```

`./run` captures the nix-shell environment into `.nix-env` once (and again when
`shell.nix` changes) and starts the newest build, so you can bind it to a key or a desktop
entry.

### Login

Subscriptions, playlists, history, recommendations and the account buttons need your YouTube
login, taken from your browser's cookies. On first run the app offers **Connect YouTube**, and
later under Settings → Connect YouTube: pick a browser, the app reads that browser's own
YouTube session (you stay signed in in your browser; no password is ever asked or stored),
checks it in three quick steps and shows your account. An exported `cookies.txt` can be
imported under Advanced instead. **Log out** forgets the app's copy; for a `config.toml`
login it comments those lines out instead (uncomment them to return).

While logged out, the app starts on that same **Connect YouTube** screen (or shows it after
logging out), the header's account tabs are replaced by **Home** and **Sign in** (which opens
Settings, with Account at the top), and the left column and Recommendations pane show an
**anonymous feed** — YouTube's own home and trending
need a login, so the app shuffles random topic searches into a mixed video list instead (a
new mix per refresh, cached); a random video of it waits in the player, not playing. Search
and channels work without an account; groups, searches and resume positions are kept locally.
Right after the first Connect, the latest video of your YouTube history waits there instead.

A `~/.config/unbloated-youtube/config.toml` login still works and wins over the app's choice:

```toml
cookies_from_browser = "brave+gnomekeyring"   # yt-dlp syntax: "firefox", "chromium", …
# cookies_file = "/path/to/cookies.txt"       # or an exported Netscape cookies file
```

Without a login, search and playback still work; the account lists (subscriptions,
playlists, history, Watch later) stay hidden until you connect.

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
| `yt.rs` | Runs `yt-dlp --flat-playlist -j` for subscriptions, feeds, channels, playlists, recommendations and search, streaming entries line by line; also downloads. History comes from `account.rs`, with `:ythistory` as the fallback. |
| `account.rs` | Account actions and the watch history (with Shorts and each entry's day, which yt-dlp's `:ythistory` lacks) through YouTube's internal InnerTube API, authenticated like the web app: browser cookies plus a `SAPISIDHASH` header. Cookies stay in memory. |
| `auth.rs` | The login source (`auth.json`): browser or cookies file. Browser login exports the cookies with yt-dlp and remembers the keyring suffix that worked (e.g. `brave+gnomekeyring`). |
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
- **Playing marks it watched.** mpv's yt-dlp gets `mark-watched`, so what you play here lands in
  your YouTube history. Loading a video into mpv (the paused preload of the last watched one)
  counts as playing, so a video that is only shown in the player is never preloaded.

## TODO
- **History: channel links on Shorts.** History's Shorts entries carry no channel, so they have
  no channel button; the Videos entries do.
- **Test the Connect flow on more setups.** Verified with Brave (keyring) and a failing
  Firefox profile; Chrome, Chromium, Edge and Flatpak/Snap profiles are untested.

- **Cross-platform (Windows, macOS).** Today it is Linux/X11 only. What's in the way:
  - the embedded player (`embed.rs`): an X11 child window for `mpv --wid`; needs a per-platform
    replacement or another way to show mpv's picture inside the window
  - the "mpv dies with the app" guarantee (`PR_SET_PDEATHSIG`, Linux only)
  - `xdg-open` for Open in browser (`open` on macOS, `start` on Windows)
  - the config, data and cache locations (`~/.config`, `~/.local/share`, `~/.cache`)
- **Wayland** is untested.
- **Prebuilt binaries (packaging).** Compiling from source is slow: the build pulls in about
  728 crates (mostly GPUI's dependencies) and took well over 15 minutes in a clean Nix build on 8
  cores, so every user of a source package (Nix, AUR source or `-git`, `cargo install`) would wait
  that long. Build once in CI instead and let everyone else download the result:
  - done: `.github/workflows/release.yml` builds a Linux x86_64 release binary on every `v*` tag
    (on ubuntu-22.04, about 7 minutes), packs it with the README, changelog, desktop entry and
    icon into a `.tar.gz` with a checksum, and publishes a GitHub release whose notes are that
    version's changelog section. First published release: v0.8.0 (needs glibc 2.35; checksum
    verified on the earlier test build, not on the release asset). Still to do: run the binary on
    another distro, and a way to attach more assets (the `.deb`, `.rpm` and so on below) to the
    same release
  - build on an older distro image (not the newest) so the binary's glibc requirement stays low
    and it runs on more distros
  - the same build feeds the `.deb` and `.rpm` packages and an AUR `-bin` package (below)
  - for Nix, a binary cache (e.g. Cachix, or a cache filled by CI) so `nix run` downloads the
    package rather than compiling it; or get it into nixpkgs, where the build servers cache it
  - a cache of the Cargo build in CI (e.g. `Swatinem/rust-cache`) keeps the CI builds themselves
    fast; arm64 and other targets later, if wanted
  - unchecked: the CI and cache details, free-tier limits, and how long the CI build takes
- **Nix packaging.** `default.nix` and `package.nix` build and wrap the app (done, tested with
  `nix-build`: it starts from a clean environment with mpv, yt-dlp and deno on its `PATH`). Still
  open: a flake, a binary cache so users don't compile it (see *Prebuilt binaries*), and a
  `nixpkgs` submission. A first build compiles every crate. The package pins its own nixpkgs
  (`nix/unstable.nix`) for yt-dlp, deno and mpv, which is fine for `nix-env` but not what a
  nixpkgs package would do.
- **AUR package** (Arch). Published as `unbloated-youtube-bin`
  (https://aur.archlinux.org/packages/unbloated-youtube-bin): `packaging/aur/PKGBUILD` is the
  template. It downloads the release archive, pins its sha256, depends on `mpv`, `yt-dlp`,
  `deno`, xkbcommon, xcb, Wayland and a Vulkan loader and driver, and installs the binary,
  desktop entry, icon, README and LICENSE. Tested with `makepkg --nodeps` on NixOS; not tested
  on Arch, so the dependency names are from memory.
  - `packaging/aur/publish.sh VERSION` publishes a release: it downloads the archive, checks its
    `.sha256`, sets `pkgver` and `sha256sums`, generates `.SRCINFO` (identical to `makepkg`'s for
    0.8.1) and pushes to the AUR, or does nothing if the AUR already has that version.
  - Automatic: the release workflow's `aur` job runs the script after each `v*` tag, using the
    repository secret `AUR_SSH_KEY` (a dedicated key registered on the AUR account, no
    passphrase). Without the secret that job only prints a notice. It has run once, on the v0.8.2 tag,
    and pushed that version to the AUR. It trusts `ssh-keyscan` for the AUR's host key rather
    than a pinned one.
  - By hand: `packaging/aur/publish.sh 0.8.2` with any key the AUR knows (`GIT_SSH_COMMAND` picks
    one); `--dry-run` stops before the push.
  - It doesn't bump `pkgrel`: a packaging-only fix to the PKGBUILD needs a manual push.
  A source or `-git` package (compiles all ~730 crates) comes second.
- **crates.io.** `cargo install unbloated-youtube`. All dependencies are already on crates.io
  (no git or path dependencies), and `Cargo.toml` now has `description`, `license` and
  `repository`, so it is ready to publish (`cargo publish --dry-run` not tried yet). It
  needs the same system libraries and runtime tools (`mpv`, `yt-dlp`, `deno`) as a source build,
  so the README must say so.
- **More packaging** (ideas, none started; the `.deb` and `.rpm` can be built from the release binary):
  - `.deb` and `.rpm` packages declaring `mpv`, `yt-dlp` and `deno` as dependencies, built in CI
    (e.g. with `cargo-deb` and `cargo-generate-rpm`) and attached to GitHub releases
  - an AppImage with the binary and `mpv`, `yt-dlp` and `deno` bundled; the Vulkan driver stays
    the host's, and a bundled `yt-dlp` goes stale quickly
  - a Flatpak; `mpv` is easy to include, but the X11 embedding, `yt-dlp` updates and reading
    the browser's cookies from inside the sandbox need care
- **Casting (play on another device).** A cast button that sends the playing video to a TV,
  speaker or another machine. Nothing here is built or tested; it is a design with the
  considerations written down. Two ways to send to your own mpv (a homelab box wired to the TV,
  e.g. [tg-mpv-bot](https://github.com/antlis/tg-mpv-bot)), then other targets.

  *Shared design.* The two projects are in different languages (Rust, Python), so what they share
  is a protocol, not code. The app has no knowledge of any particular receiver: it only runs
  whatever the chosen cast target says, and the receiver-specific part lives in the receiver's own
  repository or in your dotfiles. Targets are named tables in `config.toml`:
  ```toml
  [cast.tv]
  command = ["ssh", "homelab", "mpv-send", "{url}", "{start}"]
  ```
  - Placeholders: `{url}` (the video's page URL), `{start}` (current position in seconds), `{id}`,
    `{title}`. The command is an argument list, never a shell string: URLs and titles come from
    YouTube, so they must not be re-parsed by a shell. Over `ssh` the remote shell does parse them
    again, so the receiving script must treat its arguments as data (quote them, or read them
    from stdin).
  - Runs on a background thread (nothing may block the UI), with a timeout; the exit code and the
    first line of stderr go into a notice ("Sent to tv" / "Cast failed: …").
  - The UI: a cast button in the account-actions row (hidden by default, like the other buttons,
    with a Settings toggle), a hotkey, a picker when more than one target is configured, and an
    indicator while something is being cast. The local video should pause when a cast starts
    (otherwise both play); whether the cast counts as watched in History is still open.
  - Sending the page URL rather than a stream URL is deliberate: the receiver's own mpv and yt-dlp
    resolve it at full quality, and the app doesn't proxy any video. The catch is that the receiver
    needs `yt-dlp` and `deno` and its own login for videos that need one (age-restricted, members,
    private); the app's cookies are not sent. Resolved stream URLs are an alternative but expire
    after hours and are tied to the requesting IP and client.

  *Option 1: a command (no changes to the receiver).*
  - The command talks to the receiver's mpv directly: a small script on the receiver that sends
    `loadfile <url> replace start=<seconds>` to mpv's JSON IPC socket (`/tmp/mpv-socket` in
    tg-mpv-bot's config), reached with `ssh` (key-based login; `ControlMaster` keeps repeat casts
    fast), or `socat` over a tunnel.
  - Cheap and works today, and the same mechanism covers other tools: `catt cast {url}` for a
    Chromecast, `kdeconnect-cli` and so on.
  - Limits: fire and forget. The app can start playback but not pause, seek or show what is playing
    unless more commands are configured (`pause`, `seek`, `status` templates, which gets clumsy).
    The bot does not know about the playback: no history entry, no now-playing panel, and a
    `loadfile` on its socket may confuse its own playlist state (unchecked).

  *Option 2: an HTTP endpoint in tg-mpv-bot.*
  - The bot gets a small authenticated endpoint, e.g. `POST /play` with `{"url": …, "start": …}`
    and a bearer token, which runs the same code as a link sent in Telegram. History, the
    now-playing panel and the rest of the bot's features then follow along. It already depends on
    aiohttp. The app's cast command becomes a `curl` call, so the app needs no change beyond
    option 1; later `GET /status`, `/pause` and `/seek` could give the app real remote control.
  - It gives up a property the bot advertises: it only makes outbound connections, so nothing is
    exposed. A listener changes that. Bind it to the LAN or a Tailscale interface only, require
    the token (compared in constant time, kept out of logs, stored in `config.toml` with
    owner-only permissions), accept only `http(s)` URLs (mpv can open files, `ytdl://` and
    options), and apply the bot's `ALLOWED_USERS`-style restriction by token rather than user id.
  - Using Telegram itself as the transport does not work: a bot token can't message the bot.
    Only a user account (MTProto) could, which is a different and heavier thing.

  *Other targets (same button, different command or a built-in client).*
  - *Chromecast, Default Media Receiver*: find devices over mDNS and load a stream URL that yt-dlp
    resolved; the device fetches it itself. A single combined or HLS format works best, often
    capped around 720p–1080p. Needs a CastV2 client (e.g. the `rust_cast` crate) or `catt`.
  - *Chromecast, YouTube receiver*: start the YouTube app on the device with the video id, the way
    YouTube's own cast button does. Best quality and no streaming through the PC, but it needs the
    undocumented Lounge pairing protocol, which can change.
  - *DLNA/UPnP* (many TVs) and *AirPlay* (Apple TVs, some TVs).
  - Casting sits outside mpv, so position, pause state and resume have to be synced with the
    local history by hand, and the queue (Up next, a playlist) needs a rule: replace or append.
  - Plan: option 1 first (generic, small, useful at once); option 2 if the missing bot features
    start to matter; built-in Chromecast only if `catt` through option 1 isn't enough.
- **Bundling mpv, yt-dlp and deno** (ideas, none started). They are separate programs the app
  starts by name through `PATH`, so bundling means looking in a folder of the app's own first.
  - yt-dlp and deno publish standalone Linux binaries (deno is large, probably ~100 MB; unchecked).
    yt-dlp breaks every few weeks, so a bundled copy needs a self-update step.
  - mpv has no official Linux binary; a self-contained build means FFmpeg and the graphics and
    audio libraries, hardware decoding from a bundle is fragile, and mpv is GPL, so shipping it
    in an MIT archive means providing its source and license.
  - Options, easiest first: download yt-dlp and deno on first run into the data folder (keeps the
    archive small; mpv stays a system dependency); a "full" archive with yt-dlp and deno inside;
    an AppImage with all three; a Flatpak (solves mpv properly, but see the X11 and cookie notes
    above). Nix and the AUR already solve it through dependencies.
  - Conflicts with a copy the user already has: a private folder (e.g. the data folder or
    `/usr/lib/unbloated-youtube/`, never `/usr/bin`) doesn't clash with the system install, and
    the app only changes `PATH` for the programs it starts. Rule: use the user's own yt-dlp, deno
    and mpv when found, and the bundled or downloaded ones only as a fallback. The app starts mpv
    with its own IPC socket, so it doesn't talk to another running mpv. It doesn't pass
    `--no-config` or `--config-dir`, so the user's `~/.config/mpv` applies to any mpv it starts;
    isolating that is a choice to make on purpose.
- **Tool checks.** On startup, check that `mpv`, `yt-dlp` and `deno` are found and say which one
  is missing; downloading `yt-dlp` and `deno` into the app's data directory on first run is
  another option.
- **Overlays over the video.** The video is a separate native window drawn above everything the app
  draws, so menus and tooltips that reach into it are hidden. Handled so far: the channel menu is
  kept inside the left column, and the save-to-playlist overlay and the shortcuts sheet hide the
  video while open. Tooltips that open over the video are not handled; hiding the video while
  one is showing, or drawing overlays in their own window, would be the options.
- **Remove from YouTube's history.** The History list's remove button only forgets the entry
  here; an entry that came from YouTube's own history comes back on the next refresh, because the
  app can't change that history yet (it would need InnerTube's feedback tokens).
- **Comments, next steps** (the Comments tab shows only top-level comments for now):
  - replies to a comment (yt-dlp can fetch them)
  - sort by top or newest
  - timestamps in comments that seek the video
  - posting comments (needs InnerTube like the account actions; more work and riskier)
  - "Load more" refetches from the top (yt-dlp can't continue), so each click is slower than the
    last; fetching further pages directly through InnerTube would fix that
  - cache fetched comments per video, so reopening a video doesn't refetch
  - author avatars (via the thumbnail cache), and markers for the creator, verified and hearted
    comments
  - links and @mentions in comment text are plain text for now
  - keyboard scrolling in the list (j/k and the arrow keys in Vim mode)
  - the total comment count on the tab
- **Login beyond the browser-cookie picker.** Settings → Connect YouTube (and the first-run
  screen) already covers the guided cookie capture, one default profile per browser. Ideas for
  going further, none of them tried yet:
  - *Per-profile pickers*: list each installed browser's profiles and let you choose one, instead
    of only its default. Reading another browser's cookies stays fragile (keyring on Linux,
    app-bound encryption in newer Chrome on Windows), the same as for yt-dlp.
  - *In-app login window*: a webview (wry) on `accounts.google.com`; the app reads the cookies
    from its cookie store. Works the same on every platform, but Google sometimes refuses
    sign-in in embedded webviews, and it adds a dependency and a window to maintain.
  - *Browser extension* that hands the cookies to a local port of the app: reliable, but an
    extension to ship and keep updated.
  - *Google OAuth device flow*: no cookies at all, but the public API doesn't cover
    subscriptions or history the way InnerTube does, and yt-dlp dropped its YouTube OAuth
    support because the tokens stopped working.
- **Potential improvements.** Ideas only, nothing decided or started:
  - *Playlists*: create, rename and delete playlists (today you can only add to and remove from
    existing ones); reorder videos in a playlist you own; choose public, unlisted or private when
    creating one; move or copy a video between playlists; remove all watched videos from Watch
    later in one click
  - *Playback*: save the Up next queue as a playlist, and reorder it by dragging; a subtitles
    toggle and picker on the hover bar (today only a language code in Settings); a quality picker
    for the playing video (today only a max quality); a loop or repeat button; a sleep timer
  - *Browsing*: a Watch later button on every row (today via the save picker); hide watched
    videos in the New uploads feed; a description tab under the player, next to Comments and
    Chapters; search filters for duration, upload date and type
  - *Maintenance*: export and import settings, groups and channel flags. Groups live in one file
    on one machine (`groups.json`, with no sync and no backup), so this is also how they would
    move between computers; a setting for the data folder (point it at a synced folder) would do
    the same

## Files

- `~/.config/unbloated-youtube/`: `config.toml` (optional login override), `auth.json` (the app's own login choice, set from Settings), `settings.json` (everything from the Settings page)
- `~/.local/share/unbloated-youtube/`: watch history with resume positions, seen videos, groups, channel flags, Up next, recent searches
- `~/.cache/unbloated-youtube/`: thumbnails, cached lists, `mpv.log`

Folders from the app's old name (`jtube`) are moved over automatically on first start.

## License

[MIT](LICENSE). Contributions are welcome, and are licensed under the same terms.
