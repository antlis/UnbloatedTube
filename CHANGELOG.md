# Changelog

All notable changes to unbloated-youtube. Newest first.
Versions follow [Semantic Versioning](https://semver.org): while below 1.0, a minor bump (0.2.0)
may add features or change behaviour, a patch bump (0.1.1) only fixes bugs.

## Unreleased

### Added
- **Copy link at the current time**: a clock button after Share, `⇧C` (`y t` in Vim mode); the
  link opens the video at the current position (`?t=`). Switch in Settings > Player buttons.
- Right-click a channel in Subscriptions for a menu with **Unsubscribe** (asks for a second click
  to confirm). The menu closes by itself 2 seconds after the pointer leaves it, or on Esc or a click elsewhere.
- **Lower pane tab keys**: `[` and `]` switch between Recommended, Chapters and Up next (both modes).
- **Tab keys**: `1`–`4` jump to Subscriptions, Playlists, History and Settings (as shown), in
  both regular and Vim mode.
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
