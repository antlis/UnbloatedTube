# Changelog

All notable changes to unbloated-youtube. Newest first.

## Unreleased

### Added
- **Light theme**: Settings > Show > Light theme. Applies immediately and is remembered.
- **Video info**: views, upload date and channel subscriber count, each with its own switch in
  Settings > Video info. The playing video shows `views · date` under its title and the
  subscriber count on its channel chip (needs the login). Lists show view counts where yt-dlp
  provides them (search, playlists); the channel list shows subscriber counts.

### Fixed
- View count missing for videos uploaded in the last day or so (YouTube omits the short form).

## 2026-10-03

### Added
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

### Changed
- Search is a mode instead of a tab.
- Compact channel header with icon buttons.
- Group bar: full-width filter on top, group chips with dim counts below.
- Left and right headers have the same height when window buttons are on.
- Shortcuts sheet scrolls when it doesn't fit the window.
- Clearer like/dislike/save feedback; refused playlist edits say why.

### Fixed
- Playlists now load in full and saving to one confirms with a notice.
- "N new" tooltip no longer cropped by the video.
- Black video after changing player settings.

## 2026-10-02

First version: yt-dlp listings with settings, history and caches; a single mpv embedded in the
window over IPC; account actions (subscribe, like, dislike, save to playlist) through
InnerTube; built-in SVG icons; the GPUI interface.
