# UnbloatedTube

Lightweight, configurable YouTube desktop client: Rust + GPUI 0.2.2, mpv embedded via X11
(`--wid`) for playback, yt-dlp for data, YouTube's InnerTube API for account actions.
README.md describes features and architecture; CHANGELOG.md lists changes (update it with
every user-visible change).

## Build and run
- Building needs the nix-shell environment (linking fails without it):
  `source .nix-env && cargo build` (debug) or `cargo build --release`. `./run` starts the newest build.
- Debug and release are separate binaries: rebuild the one the user runs.
- There are no automated tests; verify UI changes by running the app (see below).

## Layout
- `src/main.rs`: the whole GPUI app (views, state, keys, settings page). `yt.rs` yt-dlp
  listings, `account.rs` InnerTube, `player.rs` mpv IPC, `embed.rs` X11 child window,
  `store.rs` settings/history/caches, `thumbs.rs`, `icons.rs`.
- Colors: always `themed(NAME)` (never `rgb(NAME)`); use `ON_ACCENT` for text on the accent color.
- New toggle: add the field to `Settings` (store.rs, with default) and a row in a `*_TOGGLES` const.
- Nothing may block the UI thread: yt-dlp, InnerTube and mpv queries run on background threads.

## Testing the UI
- Run against a private X display (Xvfb :99 + i3) with private XDG_CONFIG/DATA/CACHE dirs and
  `VK_ICD_FILENAMES` pointing at lavapipe; drive it with xdotool, capture with ImageMagick `import`.
- Test instances have no cookies: symlink the browser profile into the private config dir.
- Never kill the user's running instance (kill test processes by PID, not `pkill -f`).
- Don't change the user's real playlists or account while testing; undo anything you did.

## Releases
- Semantic versioning. To release: move the changelog's entries under a new `## X.Y.Z - date`
  heading, bump `version` in Cargo.toml (and let Cargo.lock follow), commit "Release X.Y.Z",
  then tag it: `git tag -a vX.Y.Z -m "unbloatedtube X.Y.Z"`. Push the tag only when asked.

## Conventions
- Minimal, surgical changes; match the surrounding style and comment density.
- Don't commit or push unless asked.
