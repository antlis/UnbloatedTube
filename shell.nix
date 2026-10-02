{ pkgs ? import <nixpkgs> { } }:
let
  # YouTube breaks old yt-dlp quickly (403 on streams); take it, plus deno for JS challenges, from unstable.
  # Pinned so nix caches it; an unpinned channel URL is re-downloaded on every nix-shell (~30s).
  unstable = import (fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/c59305bab2065cfecc4944690d9eedbb56f3a9fa.tar.gz";
    sha256 = "16rsfnnxk6294sz6asx0shblirkhm4yyvkimq3v00c0y2114gp7b";
  }) { };
in
pkgs.mkShell rec {
  nativeBuildInputs = with pkgs; [ pkg-config cmake clang ];
  buildInputs = with pkgs; [
    rustc cargo
    fontconfig freetype libxkbcommon wayland vulkan-loader
    xorg.libxcb xorg.libX11 xorg.libXcursor xorg.libXi xorg.libXrandr
    openssl zstd alsa-lib
  ];
  # mpv on FFmpeg 9 for its http `request_size` option: YouTube throttles mpv's single long
  # download to ~150 KB/s (stutter); 10 MB range requests stream at full speed.
  # The unwrapped mpv uses the yt-dlp below (the wrapped one bundles its own).
  packages = [ unstable.yt-dlp unstable.deno unstable.mpv-unwrapped ];
  # Optional SponsorBlock support (Settings → Player), loaded into mpv as a script.
  UNBLOATED_SPONSORBLOCK = "${pkgs.mpvScripts.sponsorblock-minimal}/share/mpv/scripts/sponsorblock_minimal.lua";
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
}
