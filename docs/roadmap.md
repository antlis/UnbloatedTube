# Roadmap

What is not done yet, most important first, then the details of each open item. The version history is in [CHANGELOG.md](../CHANGELOG.md).

**Priorities**, most important first. Ranked by how much each one helps people who are put off by
YouTube's own site (the criticism of its 2026 redesign: search, subscriptions, playlist tools,
Shorts, accessibility) and by how cheap it is. The items below this list hold the details; the
ones marked *new* are described only here.

1. **AppImage: check the bundled `libmpv`** (done in the build, untested). The AppImage now carries
   `libmpv` from Ubuntu 22.04 (mpv 0.34) and the app uses it on X11 and Wayland, so nothing needs
   installing. Still to check: that it plays on a machine with no mpv, the size of the file, whether
   mpv 0.34 is new enough for the Wayland render path, and hardware decoding from a bundle. Then
   delete the fallback that starts the `mpv` program on X11 (`backend/player.rs`).
2. **Search filters and sorting** (*new*, widens "search filters" under *Potential improvements*):
   sort by upload date, leave Shorts out, filter by duration and type. Search is the most
   complained-about part of YouTube's site and the gap is still open here. Check first what
   InnerTube's search filters offer, since yt-dlp's search has none.
3. **README: a short demo (GIF) of the player and a playlist action** (*new*). The README now says
   what the app does and links the cookie-login limits in [login.md](login.md); a moving demo is
   what is still missing for someone deciding in ten seconds.
4. **Subscription management at scale** (*new*): sort channels (A–Z, last upload, most active),
   find the ones with no upload in a year, unsubscribe from several at once. Groups and the chronological
   New uploads feed exist; this is the bulk side.
5. **Subtitles: style** (position, background); subtitles that cover the picture are a common
   complaint. See *Subtitles: picker and more*.
6. **Accessibility** (*new*): a high-contrast theme, a larger-controls option, a progress bar that
   stays visible on any picture. The theme system (`ui/theme.rs`) is the place for the first.
7. **Open YouTube links from other programs.** See *Open YouTube links in the app*.
8. **Check: saving one video to several playlists in a row** (*new*). YouTube's own dialog closes
   after each pick; ours should stay open. Verify it does, then say so in the README.
9. **Playlists: reorder videos; move or copy a video between playlists.**
10. **Up next: reorder by dragging; a loop or repeat button.**
11. **Browsing: hide watched videos in New uploads.**
12. **Test Connect on more setups** (Chrome, Chromium, Edge, Flatpak, Snap).
13. **Prebuilt packages**: `.deb` and `.rpm`, then crates.io, then Flatpak. See *More packaging*.
14. **Cross-platform (Windows, macOS).** Big, and `platform/embed.rs` is the main obstacle.

Decided already, with the measurements behind each: keep deno as yt-dlp's JavaScript runtime
(QuickJS adds about 6.5 s to every video start), keep yt-dlp and mpv instead of replacing them
(see [research/javascript-runtimes.md](research/javascript-runtimes.md) and
[research/appimage-and-mpv.md](research/appimage-and-mpv.md)).

Everything else below is unranked: ideas and open details, none of them urgent.

- **Open YouTube links in the app** (instead of the browser). `unbloatedtube <url>` works
  now (see [Command line](integrations.md#command-line)); what's left is links from other programs:
  - A `.desktop` file can only claim a whole URL scheme, not one domain, so a small
    router is set as the default browser: `youtube.com` / `youtu.be` go to the app, everything
    else to the real browser. Links clicked inside the browser itself don't go through it (needs
    an extension or the browser's "open in external app"). In a NixOS setup it would live in the
    dotfiles next to the desktop entries.
- **Subtitles: picker and more.** The CC toggle and the Settings → Subtitles section work. Still
  open:
  - subtitle style beyond size (position, background)
  - captions YouTube keeps refusing (HTTP 429) even after the retry: a way to get them anyway
    (yt-dlp's PO token plugins)
- **Feature ideas** (none started; roughly smallest first):
  - *A daily watch-time limit*: a budget, a quiet counter and a soft stop when it runs out,
    optionally only at certain hours or for certain groups
  - *Open in a new window*, beyond Play in separate window:
    - several lists open at once: switchable left-column views, like browser tabs (no extra player)
    - a real second app window, with its own embedded player, would mean per-window player state
      and a second X11 embed (`platform/embed.rs`, already the main obstacle to Windows and macOS): not
      worth it before cross-platform embedding exists
- **Test the Connect flow on more setups.** Verified with Brave (keyring) and a failing
  Firefox profile; Chrome, Chromium, Edge and Flatpak/Snap profiles are untested.

- **Cross-platform (Windows, macOS).** Today it is Linux/X11 only. All OS-specific code is in
  `src/platform/` (plus the yt-dlp shim in `backend/prefetch.rs`, Unix by design), so a port means
  giving those modules a version for the other system; the compiler then lists what is missing.
  Not attempted yet. What's in the way:
  - the embedded player (`platform/embed.rs`): an X11 child window for libmpv's `wid`; needs a
    per-platform replacement or another way to show mpv's picture inside the window
  - `platform/process.rs`: the "mpv dies with the app" guarantee (`PR_SET_PDEATHSIG`, Linux only)
    and `/proc/<pid>/exe`
  - `platform/ipc.rs`: Unix sockets for mpv's control, the command line and the helper (named
    pipes on Windows)
  - `platform/desktop.rs`: `xdg-open` and `xdg-settings` (`open` on macOS, `start` on Windows)
  - `platform/mpris.rs`: media keys over D-Bus (Windows and macOS have their own APIs)
  - the config, data and cache locations (`~/.config`, `~/.local/share`, `~/.cache`)
- **A login the app owns** (see *Limits of the cookie login*). Idea: Settings → Sign in opens a
  browser window with a profile of the app's own; you sign in to Google and YouTube there once;
  the app keeps that profile, reads its cookies (a dedicated Chromium or Brave profile keeps them
  on disk and yt-dlp reads them without the keyring; verified on a throwaway profile), and now and
  then loads youtube.com in it headless so YouTube rotates them. Verified: YouTube sends no new
  cookies on the app's API requests, so the page load is the only way to refresh. Not verified:
  whether a headless load keeps the session valid for days, and whether Google lets you sign in
  in a browser started with a debug port.
- **Wayland**: done and tested in a headless sway, and on niri at a 1.25 display scale; see
  *Running → Wayland* for what is and isn't checked. Still open: dragging the seek line over the
  video, and trying it on a real GPU and other compositors.
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
  - for Nix: done in the repository, not switched on: the Rust dependencies build as a derivation
    of their own (crane, see *Nix packaging*), and the release workflow's `nix-cache` job builds
    the package and pushes it to the `antlis` Cachix cache (switched on for 0.44.0; see *Prebuilt
    binary* under NixOS). Or get it into nixpkgs, where the build servers cache it
  - a cache of the Cargo build in CI (e.g. `Swatinem/rust-cache`) keeps the CI builds themselves
    fast; arm64 and other targets later, if wanted
  - unchecked: the CI and cache details, free-tier limits, and how long the CI build takes
- **Nix packaging.** `flake.nix` (also `default.nix`) and `package.nix` build and wrap the app
  (done, tested with `nix-build`: it starts from a clean environment with mpv, yt-dlp and deno
  on its `PATH`; the flake exposes `packages.<system>.default` and an overlay). The flake
  builds the Rust dependencies with [crane](https://github.com/ipetkov/crane) as a derivation of
  their own, from the manifests with this app's version blanked out: a new release (or any source
  change) compiles only this crate, and the ~730 dependency crates are reused from the Nix store
  until `Cargo.lock` changes. `default.nix` (no flake) still builds in one go. Still open: a
  `nixpkgs` submission. A first build compiles every crate.

  **Nix binary cache** (so nobody compiles at all): the workflow's `nix-cache` job runs on every
  version tag (and by hand with `gh workflow run Release`) and pushes to the public Cachix cache
  `antlis`: the repository variable `CACHIX_CACHE` names it, the secret `CACHIX_AUTH_TOKEN` is the
  write token. Checked: the 0.44.0 package (`nix eval` of the flake at that revision) is in the
  cache (`nix path-info --store https://antlis.cachix.org`), and a NixOS rebuild with the cache
  configured downloaded it and finished much faster than compiling (the author's machine). Not yet
  checked: a machine other than the author's, and the free plan's storage limit as releases
  accumulate. The package pins its own nixpkgs for yt-dlp, deno and mpv
  (`nix/unstable.nix` and the flake's input, which must be bumped together), which is fine for a
  profile but not what a nixpkgs package would do.
- **AUR package** (Arch). Published as `unbloatedtube-bin`
  (https://aur.archlinux.org/packages/unbloatedtube-bin): `packaging/aur/PKGBUILD` is the
  template. It downloads the release archive, pins its sha256, depends on `mpv`, `yt-dlp`,
  `deno`, xkbcommon, xcb, Wayland, libglvnd (EGL) and a Vulkan loader and driver, and installs the binary,
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
  - The rename (0.34.0): the AUR can't rename a package, so `unbloatedtube-bin` is new (the
    first push of 0.34.0 created it) and `unbloated-youtube-bin` became a transitional package
    that only depends on it (`packaging/aur/transitional/PKGBUILD`, pushed once with
    `packaging/aur/transitional.sh`). The new package provides `unbloated-youtube` and installs
    `/usr/bin/unbloated-youtube` and `/usr/bin/ubt` as links. Later, a merge request on the old
    package's AUR page (old → new) moves its votes and comments and retires it.
  A source or `-git` package (compiles all ~730 crates) comes second.
- **crates.io.** `cargo install unbloatedtube`. All dependencies are already on crates.io
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
- **Casting: what's left.** The button, the config targets and tg-mpv-bot's remote play API
  exist (see [Cast targets](integrations.md#cast-targets)), also for lists. Still open:
  - *Remote control beyond tg-mpv-bot*: only receivers with its API are controlled, and volume,
    subtitles and next/previous aren't wired to the app's controls yet.
  - *A target picker* instead of one button per target, once there are many.
  - *Other receivers with a built-in client*: Chromecast (Default Media Receiver, or the YouTube
    receiver through the Lounge protocol), DLNA/UPnP and AirPlay. `catt` already covers
    Chromecast through a command target.
  - *Cast and resume positions*: a cast doesn't save a resume position in the app (the video starts
    again from the beginning when played here); only the bot's own history (`/history` in Telegram)
    has it as well.
- **Bundling mpv, yt-dlp and deno** (ideas; the AppImage bundles yt-dlp and deno and updates yt-dlp
  itself, mpv is the open part). They are separate programs the app
  starts by name through `PATH`, so bundling means looking in a folder of the app's own first.
  - yt-dlp and deno publish standalone Linux binaries (deno is large, probably ~100 MB; unchecked).
    yt-dlp breaks every few weeks, so a bundled copy needs a self-update step (done in the
    AppImage: `backend/ytdlp_update.rs`).
  - mpv has no official Linux binary; a self-contained build means FFmpeg and the graphics and
    audio libraries, hardware decoding from a bundle is fragile, and mpv is GPL, so shipping it
    in the archive means providing its source and license.
  - Options, easiest first: download yt-dlp and deno on first run into the data folder (keeps the
    archive small; mpv stays a system dependency); a "full" archive with yt-dlp and deno inside;
    an AppImage with all three; a Flatpak (solves mpv properly, but see the X11 and cookie notes
    above). Nix and the AUR already solve it through dependencies.
  - Conflicts with a copy the user already has: a private folder (e.g. the data folder or
    `/usr/lib/unbloatedtube/`, never `/usr/bin`) doesn't clash with the system install, and
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
  - *Playlists*: reorder videos in a playlist you own; move or copy a video between playlists
  - *Playback*: reorder the Up next queue by dragging; a loop or repeat
    button
  - *Browsing*: a Watch later button on every row (today via the save picker); hide watched
    videos in the New uploads feed; search filters for duration, upload date and type
  - *Maintenance*: export and import settings, groups and channel flags. Groups live in one file
    on one machine (`groups.json`, with no sync and no backup), so this is also how they would
    move between computers; a setting for the data folder (point it at a synced folder) would do
    the same
