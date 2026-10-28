# The AppImage, libmpv, and replacing mpv or yt-dlp

State on 2026-10-28. Items marked *untested* have not run on a real machine yet.

## What the AppImage is

Built by `packaging/appimage/build.sh` in the release workflow (`appimage` job) from the release
binary on Ubuntu 22.04 (glibc 2.35); the AppImage itself is assembled on Ubuntu 24.04 (see
*Wayland crash* below), so it needs glibc 2.39. It bundles the libraries the binary links against, `yt-dlp`
and `deno` as fallbacks, and `libmpv` with its own libraries (checked in a clean container, see below). `AppRun` puts
the bundled helpers last on `PATH`, so a yt-dlp or deno of your own wins.

## yt-dlp stays current (`backend/ytdlp_update.rs`)

A bundled yt-dlp is frozen at release day, and YouTube breaks old versions within weeks. The
AppImage is read-only, so when no yt-dlp of the user's is on `PATH` the app copies the bundled one
to `~/.local/share/unbloatedtube/bin/` and runs its own `yt-dlp -U` on it at most once a day.
`AppRun` puts that folder on `PATH` between the user's and the bundled one. It only starts in an
AppImage (`AppRun` sets `UNBLOATED_BUNDLED_YTDLP`). Unit tests cover the copy-once and
update-once-a-day logic with a fake script. *Untested:* the real `yt-dlp_linux -U` from a copied
location.

## mpv: libmpv inside the app on X11 as well

Wayland already ran libmpv inside the app (`platform/embed_wayland.rs`), controlled over the same
IPC socket as the mpv program. X11 now does the same without a render context
(`platform/libmpv.rs`, `Inline`): libmpv gets the `wid` of the app's X11 child window and draws
itself. If libmpv can't be loaded, the app starts the `mpv` program as before; delete that
fallback once the in-process path is proven.

The AppImage sets `UNBLOATED_LIBMPV` to the bundled library, so nothing needs installing for
playback. Risks, all *untested*:

- Ubuntu 22.04's libmpv is mpv 0.34; whether it supports everything the Wayland render path uses
  is unknown.
- The file grows (libmpv pulls in ffmpeg and more); how much is unknown.
- Hardware decoding from a bundle depends on host drivers and may fail (software decoding should
  still work).

### Tested in a clean container (2026-10-28)

The first AppImage with libmpv (CI run of commit 191c7ae, 164.7 MB, libmpv from mpv 0.34.1 with
190 libraries) was run in a clean `ubuntu:22.04` container (no mpv, yt-dlp or deno; Xvfb, i3,
Mesa's software Vulkan driver; script: `packaging/appimage/check-clean.sh` for the libraries):

- **It did not start playing at first:** libmpv needs `libjack.so.0`, and linuxdeploy's excludelist
  leaves out libjack, libharfbuzz, libfribidi and (via libusb) a few more because it assumes every
  desktop has them. A clean machine does not, so loading libmpv failed and the app fell back to
  the (missing) `mpv` program. CI's own `ldd` check could not see it, as the build host has them
  all. Fixed: `build.sh` now copies those libraries and what they need (glib, graphite2 came with
  harfbuzz) and sets their RPATH to `$ORIGIN`; CI runs `check-clean.sh` in a clean container.
- **After the fix, with the same bundle:** no missing library (196 in the bundle), libmpv mapped
  inside the app, no `mpv` process, and an X11 child window of the video's size holding libmpv's
  own `gl`/`mpv` window, so the embedding works. mpv's log shows the whole chain: the page URL
  tried directly (mpv 0.34 does that first), then the app's yt-dlp shim ("youtube-dl succeeded",
  about 6.5 s cold), the video, audio and subtitle tracks found, and the video output starting on
  `x11egl`.
- **Not seen:** a picture. The app's own window was black in that container before any video, so
  its UI does not draw there (probably the old Mesa software Vulkan driver in Ubuntu 22.04; not
  investigated). The `yt-dlp` copy was made in the data folder; whether `yt-dlp -U` updated it is
  unknown (the bundled version was still the newest).
- **Test quirk:** a NixOS host with `binfmt_misc` for AppImages can't run one inside a container
  (the handler is missing there, so exec fails with "No such file or directory"); blanking the three
  marker bytes at offset 8 in a copy makes it run.

### Why the in-process X11 path could not be tested on the dev machine

A debug build made in the dev shell links glibc 2.40; the libmpv on this NixOS machine needs glibc
2.43 or newer, so loading it fails (`version GLIBC_2.43 not found`). Real packages avoid this (the
Nix package builds everything consistently; the AppImage ships its own libraries). To test X11,
use the Nix package from the flake, then check: no `mpv` process while a video plays
(`pgrep -a mpv`), the video inside the window, hover bar, volume keys, right-click menu and seeking
working, and no "starting the mpv program instead" line on stderr.

The fallback path was exercised in a private Xvfb + i3 session (libmpv failed to load, the `mpv`
program started and played), which is how the glibc problem showed up. `LibMpv::load()` now lists
every path it tried, so a failure names its real cause.

## Could a lighter or more portable player replace mpv?

From memory, not tested:

- **libVLC**: cross-platform and easy to embed, but heavier; no clear gain over mpv.
- **GStreamer**: modular, good Rust bindings; more work to get YouTube's separate audio and video
  streams, subtitles and hardware decoding as good as mpv's.
- **ffmpeg plus your own player**: lightest in theory; you would rebuild A/V sync, seeking,
  subtitles and hardware decoding.
- **Native players** (AVPlayer, Media Foundation): a separate player per OS.

mpv runs on Windows and macOS as libmpv. The weak part was the X11 embedding (the `mpv` program with
`--wid`), which libmpv in-process now replaces. Using libmpv everywhere is the route to the
cross-platform port.

## Could something replace yt-dlp?

Reimplementing it means taking on YouTube's constant changes alone (signature and challenge
solving, PO tokens, client changes). Rust libraries and Invidious/Piped servers are less
maintained or depend on third-party servers that get blocked. yt-dlp stays. The weight is its
JavaScript runtime: see [javascript-runtimes.md](javascript-runtimes.md).

## Wayland crash with mpv 0.34, and the move to Ubuntu 24.04 (2026-10-28)

The first AppImage with libmpv (Ubuntu 22.04, mpv 0.34.1) segfaulted on a real Wayland session
(niri) as soon as a video started: exit code 139, no window. The crash report
(`coredumpctl info`) puts it in the `video` thread, in `mpv_render_context_create` inside the
bundled `libmpv.so.1`, ending in a `strlen`. It does not depend on user data (an empty config
crashes too), nor on hardware decoding (off made no difference), nor on the bundled Vulkan
loader (removing it made no difference). The same AppImage with the system's libmpv 0.41 swapped
in (`UNBLOATED_LIBMPV`) ran for 80 s without a crash. X11 does not use the render API, so it is
not affected by this particular crash (and was still never seen to draw a picture).

Fix tried: assemble the AppImage on Ubuntu 24.04, whose libmpv2 is mpv 0.37. Not known whether
0.37 is new enough; only 0.41 was tested. Cost: the AppImage needs glibc 2.39 (the plain archive
still needs 2.35). If 0.37 also crashes, build libmpv from source in CI instead.

### Testing on NixOS: two traps

- `appimage-run` resets `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `XDG_CACHE_HOME`, so a "private"
  config does not stay private: the test used the real `~/.local/share/unbloatedtube` (a played
  test video landed in `history.json`). Use a throwaway `HOME`, or the extracted `AppRun` inside the
  clean container from `check-clean.sh`, not `appimage-run`, when playing anything.
- `appimage-run` extracts into the real `~/.cache/appimage-run/<hash>/`; a variant AppImage for
  bisecting (original runtime header plus a new squashfs of a modified AppDir, via `mksquashfs`) is
  a way to test removing one library at a time.
