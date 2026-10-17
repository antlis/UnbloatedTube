# UnbloatedTube

Lightweight, configurable YouTube desktop client: Rust + GPUI 0.2.2, mpv embedded via X11
(`--wid`) for playback, yt-dlp for data, YouTube's InnerTube API for account actions.
README.md describes features and architecture; CHANGELOG.md lists changes (update it with
every user-visible change).

## Build and run
- Building needs the nix-shell environment (linking fails without it):
  `source .nix-env && cargo build` (debug) or `cargo build --release`. `./run` starts the newest build.
- Debug and release are separate binaries: rebuild the one the user runs.
- `cargo test` runs the unit tests (parsing, cast, platform helpers); there are none for the UI: verify
  UI changes by running the app (see below).

## Layout
The map of the code, how it fits together and where new code goes: [ARCHITECTURE.md](ARCHITECTURE.md). In short:
- `src/main.rs`: entry point only (command line, window). The rest is three layers; code uses the ones
  below it, never above (`platform` <- `backend` <- `app` / `ui`):
  - `src/app/`: the GPUI app. `mod.rs` holds the state (`Unbloated`) and `tick`; one file per feature
    (`playback`, `navigation`, `casting`, `browse`, `subscriptions`, `session`, `library`, `subtitles`,
    `input`, `menus`, `layout`, `commands`, ...), each an `impl Unbloated` block that starts with
    `use super::*;`. `app/view/` is the rendering (player, browser, settings, menus, overlays, info).
  - `src/ui/`: pieces that don't depend on the app's state: `theme.rs` (colours, `themed`), `tables.rs`
    (toggles and shortcut tables), `text_edit.rs`, `components.rs`, `icons.rs`, `textsel.rs`.
  - `src/backend/`: no GPUI. `yt.rs` yt-dlp listings, `account.rs` InnerTube, `player.rs` mpv IPC,
    `store.rs` settings/history/caches, `prefetch.rs` yt-dlp shim and helper, `cast.rs`, `cli.rs`,
    `proxy.rs`, `thumbs.rs`, `storyboard.rs`, `dearrow.rs`, `links.rs`, `http.rs`, `auth.rs`.
  - `src/platform/`: **all** OS-specific code (X11 embed, MPRIS, sockets, process control, private
    files and locks, opening links). Don't put `libc`, `/proc`, Unix sockets or `xdg-*` anywhere else
    (the yt-dlp shim's file descriptors in `backend/prefetch.rs` are the one deliberate exception).
- Items shared between files are `pub(crate)`; a new feature goes in the matching `app/` file or a new one
  (add `mod name;` in `app/mod.rs`), its drawing in `app/view/`.
- Colors: always `themed(NAME)` (never `rgb(NAME)`); use `ON_ACCENT` for text on the accent color.
- New toggle: add the field to `Settings` (`backend/store.rs`, with default) and a row in a `*_TOGGLES`
  const (`ui/tables.rs`).
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
