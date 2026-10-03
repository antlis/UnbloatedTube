# Changelog

All notable changes to unbloated-youtube. Newest first.
Versions follow [Semantic Versioning](https://semver.org): while below 1.0, a minor bump (0.2.0)
may add features or change behaviour, a patch bump (0.1.1) only fixes bugs.

## 0.8.1 - 2026-10-03

### Added
- **MIT license** (`LICENSE`), also included in the release archive.
- **AUR package files** (`packaging/aur`, `unbloated-youtube-bin`), installing the prebuilt binary.

## 0.8.0 - 2026-10-03

### Added
- **`O` opens the video in your browser** (`o` in Vim mode), like the Open in browser button.
- **Prebuilt Linux binary** on each GitHub release (x86_64, glibc 2.35 or newer), with a desktop
  entry and icon, so you don't have to compile it.
- **Nix package** (`default.nix`, `package.nix`): installs the app with mpv, yt-dlp and deno on
  its `PATH`, plus the desktop entry and icon.

## 0.7.0 - 2026-10-03

### Added
- **Comments tab** under the player, off by default (Settings → Comments). The top 40 comments
  are fetched only when you open the tab, once per video; "Load more comments" at the bottom
  fetches 40 more.

## 0.6.0 - 2026-10-03

### Added
- **Hover controls on the video**: a YouTube-style bar appears when the pointer is over the video
  and hides after a moment: a seek line, play/pause, previous/next, mute and volume, the time,
  ±10 s, speed, picture-in-picture and fullscreen. While it is on, the playback row under the video
  is gone (the account/action buttons stay); turn it off to get the two button rows back.
  Settings > Player > *Hover controls on the video* (on by default), independent of mpv's own
  controls. It is a small Lua script, `src/controls.lua`, run by mpv, because the video is a native
  window that the app can't draw over.

### Changed
- **mpv's own controls and key bindings are off over the video by default**: no on-screen
  controller on hover, no mpv hotkeys, and the app's shortcuts now also work while the pointer is
  over the video. Settings > Player > *mpv controls and hotkeys* brings them back. The
  picture-in-picture window always keeps them, since it has nothing else.
- `f` and `Esc` leave fullscreen from the app, and a double click still toggles it.

## 0.5.0 - 2026-10-03

### Changed
- One tab scheme for both columns: numbers `1`–`4` pick the header tabs and `5`–`7` the lower
  pane's (Recommended, Chapters, Up next), and `[` / `]` cycle through all seven in that order,
  wrapping around, in regular and Vim mode. **`[` / `]` used to step only the lower pane's tabs.**

## 0.4.0 - 2026-10-03

### Added
- **Keyboard focus ring**: `Tab` / `⇧Tab` move a visible ring through every clickable thing, the
  left column first and then the right one; `Enter` or `Space` presses what has the ring, `Esc`
  or a click clears it. Works in regular and Vim mode; the click targets are the same ones hint
  mode uses, so they are now collected in regular mode too.

## 0.3.0 - 2026-10-03

### Added
- **Copy link at the current time**: a clock button after Share, `⇧C` (`y t` in Vim mode); the
  link opens the video at the current position (`?t=`). Switch in Settings > Player buttons.
- Right-click a channel in Subscriptions for a menu with **Unsubscribe** (asks for a second click
  to confirm). The menu closes by itself 2 seconds after the pointer leaves it, or on Esc or a click elsewhere.
- **Lower pane tab keys**: `[` and `]` switch between Recommended, Chapters and Up next (both modes).
- **Tab keys**: `1`–`4` jump to Subscriptions, Playlists, History and Settings (as shown), in
  both regular and Vim mode.
- **Player full height** with `⇧E`: hides the lower pane, the counterpart of `E`.
- **Collapse the right column** with `⇧B`, or by dragging the divider to the right edge. Only one
  column is hidden at a time.
- **Collapse the left column** with `B` (`b` in Vim mode) or by dragging its divider to the window
  edge; drag it back out to restore your width. Search, filter and tab keys bring it back.

### Changed
- The divider between the player and the lower pane (Recommended, Chapters, Up next) can be
  dragged all the way up, and `E` (`e` in Vim mode) toggles the lower pane to the full height of the
  right column. The video window hides itself when there is no room for it.
- Player buttons are in two rows again: playback and volume on top, account and other actions
  (subscribe, save, like, download, share, share at time, open in browser) below. The time stays
  beside the views and date.

### Fixed
- `j`/`k` in Vim mode: going up now scrolls the list back, so the selected row can no longer
  end up out of view.

## 0.2.0 - 2026-10-03

### Added
- **Volume**: mute button and a ten-step volume bar next to the speed button (hide it in
  Settings > Player buttons > Volume), `↑`/`↓` keys (`+`/`-` in Vim mode) that also work with
  the pointer over the video, listed in the `?` cheat sheets. The volume is remembered.
- **Open in browser** button: opens the video's page in your default browser
  (Settings > Player buttons > Open in browser).
- **Chapters tab** under the player (Settings > Show > Chapters): a clickable list of the
  video's chapters that highlights the current one. It appears only for videos with chapters.

### Changed
- The playback time moved to the right end of the views and date line, which keeps all
  player buttons in one row. The "Loading…" text next to it is gone; the red bar shows loading.
- The player pane has a minimum height so the controls always fit under the video.

## 0.1.0 - 2026-10-03

First tagged release. It includes everything under "Development history" below, plus:

### Added
- **Light theme**: Settings > Show > Light theme. Applies immediately and is remembered.
- **Video info**: views, upload date and channel subscriber count, each with its own switch in
  Settings > Video info. The playing video shows `views · date` under its title and the
  subscriber count on its channel chip (needs the login). Lists show view counts where yt-dlp
  provides them (search, playlists); the channel list shows subscriber counts.

### Fixed
- View count missing for videos uploaded in the last day or so (YouTube omits the short form).

## Development history before 0.1.0

### 2026-10-03

#### Added
- Filter any list as you type (Ctrl+F): channels, playlists, a channel's or playlist's videos,
  history. Local and instant.
- Resizable *Continue watching* in History (drag bar).
- Remove a video from a playlist (trash icon on hover; in Liked videos it unlikes).
- Add a whole playlist to Up next.
- Notices as toasts in the bottom-right corner, fading after a few seconds.
- Text fields: real cursor, Home/End, word jumps, Shift selection, Ctrl+A/C/X/V.
- Picture-in-picture: play in a small always-on-top mpv window.
- Channel groups: filter Subscriptions and New uploads by group.
- Mute a channel in New uploads; desktop notifications for uploads of channels with the bell on.
- Shorts tab, keyboard control, Up next queue, watched state, Vim mode with click hints.
- Copy-link hotkeys: `C`, or `yy` in Vim mode.
- Clickable channel chip under the player.
- `./run` launcher that caches the nix-shell environment.
- Full title on hover for truncated titles.
- README: motivation, features, settings, keys, architecture.

#### Changed
- Search is a mode instead of a tab.
- Compact channel header with icon buttons.
- Group bar: full-width filter on top, group chips with dim counts below.
- Left and right headers have the same height when window buttons are on.
- Shortcuts sheet scrolls when it doesn't fit the window.
- Clearer like/dislike/save feedback; refused playlist edits say why.

#### Fixed
- Playlists now load in full and saving to one confirms with a notice.
- "N new" tooltip no longer cropped by the video.
- Black video after changing player settings.

### 2026-10-02

First version: yt-dlp listings with settings, history and caches; a single mpv embedded in the
window over IPC; account actions (subscribe, like, dislike, save to playlist) through
InnerTube; built-in SVG icons; the GPUI interface.
