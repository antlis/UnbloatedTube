# Install and run

Ways to get the app, what each one needs, and where it keeps its files.

Built for Linux, on X11 and on Wayland (see *Wayland* below); Windows and macOS are planned (see
[TODO](roadmap.md)). Needs `mpv` (with its library, `libmpv`, which the usual `mpv` packages include),
`yt-dlp` (recent: YouTube breaks old versions quickly) and `deno` (yt-dlp uses it for YouTube's
JavaScript challenges).

## Wayland

Both run mpv as a library (`libmpv`) inside the app's own process. On X11 mpv draws into a child
window of the app's window (`wid`). Wayland has no such thing, so there the app draws the video into
a subsurface of its window with OpenGL. (On X11, if `libmpv` can't be loaded, the app starts the
`mpv` program instead, as it used to.) Nothing to set up: the app notices it is on Wayland. Needs
`libmpv` (the `mpv` package has it), `libEGL` and `libwayland-egl` (Mesa / libglvnd and wayland);
without them mpv opens in a window of its own, as it did before, and the app says so on stderr.
`UNBLOATED_LIBMPV=/path/to/libmpv.so.2` points it at a particular libmpv (the Nix package sets it).

What differs from X11, and what is not checked:

- Your own `mpv.conf` is not read in this mode (only the app's options and Settings → Player →
  extra mpv options), because the library doesn't read it and a `vo` or `gpu-context` in it would
  break the embedding.
- The first video of a session blocks the window for a moment (a fraction of a second) while mpv
  starts. Picture-in-picture still opens mpv's own window.
- Tested in a headless sway (software rendering): playing, the hover bar, click, double click and
  fullscreen, resizing. Also tested on niri with a 1.25 display scale (no real GPU). Not tested: a
  real GPU and hardware decoding, other compositors, audio, right click and keys over the video,
  long sessions.

## Prebuilt binary

Releases on GitHub have a Linux x86_64 archive (built on Ubuntu 22.04, so it needs glibc 2.35 or
newer: Ubuntu 22.04+, Debian 12+, Fedora, Arch). Unpack it and run `unbloatedtube`; the
`.desktop` file and icon in it are for your launcher. Install separately:

- the tools above: `mpv`, `yt-dlp`, `deno`
- libraries the binary links against, which most desktops already have: xkbcommon (and its X11
  part, `libxkbcommon-x11`) and xcb; it also loads the Vulkan loader and Wayland client library
  at run time, so install those and a Vulkan driver for your GPU (Mesa's, or
  `mesa-vulkan-drivers`); under Wayland also EGL (`libglvnd` / `libegl1`) and `libmpv` (the
  `mpv` package has it, or `libmpv2` on Debian)

Check the download with the `.sha256` file next to it (`sha256sum -c`).

## AppImage

Releases also have an `unbloatedtube-X.Y.Z-x86_64.AppImage` (with a `.sha256`): one file that
runs on most Linux distros, with no unpacking. Make it executable and run it:

```sh
chmod +x unbloatedtube-*-x86_64.AppImage
./unbloatedtube-*-x86_64.AppImage
```

It carries `yt-dlp` and `deno` as a fallback: it looks for those on your `PATH` first, so a
yt-dlp you keep up to date wins over the bundled one. Without a yt-dlp of your own, the app copies
the bundled one to `~/.local/share/unbloatedtube/bin/` and updates that copy itself (`yt-dlp -U`,
at most once a day, in the background; it only stays as old as the release if you are offline),
because YouTube breaks old versions within weeks. It carries `libmpv` (with the libraries it needs) as the player, so
nothing else has to be installed for playback. What it does not carry: `libxkbcommon` and `libxkbcommon-x11` (any
desktop with X11 has them), the Vulkan loader and a GPU driver, and glibc, so it needs a distro
with glibc 2.39 or newer (Ubuntu 24.04, Debian 13, Fedora 40, current Arch; the prebuilt archive above
needs only 2.35, because the AppImage carries libmpv from Ubuntu 24.04). The file is large (around 165 MB)
because of the bundled libmpv with ffmpeg, yt-dlp and deno. On
distros without FUSE 2, run it with `--appimage-extract-and-run`. Not on NixOS: use the flake.

## NixOS

To install it with Nix, the repository is a flake. Its package (`package.nix`) puts mpv, yt-dlp
and deno on the app's `PATH` and installs the desktop entry and icon; they come from a pinned
nixpkgs-unstable:

```sh
nix profile install github:antlis/UnbloatedTube     # into your profile
nix run github:antlis/UnbloatedTube                 # or just try it
```

In a flake-based NixOS / home-manager config, add it as an input and the package to
`home.packages` (or `environment.systemPackages`):

```nix
inputs.unbloatedtube.url = "github:antlis/UnbloatedTube";
# …
home.packages = [ inputs.unbloatedtube.packages.${pkgs.system}.default ];
```

Without flakes, `nix-build` (or `nix-env -f . -i`) builds the same package through `default.nix`.

The first build compiles all ~730 crates (about 25 minutes on 2 cores, with several GB of disk);
the flake builds them as a separate derivation (crane), so after that an update recompiles only
the app, in about a minute and a half (a release that changes `Cargo.lock` compiles the
dependencies again).

### Prebuilt binary (binary cache)

Every release is built by CI and pushed to a public Cachix cache, `antlis`, so Nix can download
the app instead of compiling it. Tell Nix about the cache once; it is read-only and needs no
token. On NixOS, in your configuration:

```nix
nix.settings.extra-substituters = [ "https://antlis.cachix.org" ];
nix.settings.extra-trusted-public-keys = [
  "antlis.cachix.org-1:OutAVhWPSk0V9YHf4Il10wMjJmDAvGnAT+mzedJKaLs="
];
```

Elsewhere, put the same two lines in `nix.conf` (`/etc/nix/nix.conf` or `~/.config/nix/nix.conf`;
your user must be a trusted user for the first one to count):

```
extra-substituters = https://antlis.cachix.org
extra-trusted-public-keys = antlis.cachix.org-1:OutAVhWPSk0V9YHf4Il10wMjJmDAvGnAT+mzedJKaLs=
```

To use it for a single command, without changing any configuration, pass them as options, e.g.
`nix profile install github:antlis/UnbloatedTube --option extra-substituters https://antlis.cachix.org --option extra-trusted-public-keys "antlis.cachix.org-1:OutAVhWPSk0V9YHf4Il10wMjJmDAvGnAT+mzedJKaLs="`.

What to know:

- The cache has what CI built from each release tag and each push to master (a few minutes after
  the push), using the flake's own pinned nixpkgs. If you
  use the flake as an overlay on a different nixpkgs, the package comes out different and Nix
  compiles it locally.
- CI pushes it a few minutes after a release is tagged (about 8). A rebuild before that compiles
  locally, as without the cache. `nix path-info --store https://antlis.cachix.org <path>` says
  whether a given build is there.
- Other install methods (AppImage, tarball, `.deb`/`.rpm`, AUR) are prebuilt binaries already and
  don't use it.

For development, `shell.nix` provides the same tools:

```sh
nix-shell --run 'cargo build --release'
./run          # launcher: caches the nix-shell environment, so starts take <1 s
```

`./run` captures the nix-shell environment into `.nix-env` once (and again when
`shell.nix` changes) and starts the newest build, so you can bind it to a key or a desktop
entry.


## Files

- `~/.config/unbloatedtube/`: `config.toml` (optional login override), `auth.json` (the app's own login choice, set from Settings), `settings.json` (everything from the Settings page)
- `~/.local/share/unbloatedtube/`: watch history with resume positions, seen videos, groups, channel flags, Up next, recent searches
- `~/.cache/unbloatedtube/`: thumbnails, cached lists, `mpv.log`

Folders from the app's old names (`unbloated-youtube`, before that `jtube`) are moved over
automatically on first start.
