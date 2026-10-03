# nix-build; or nix-env -f . -i; or add it to environment.systemPackages / home.packages.
{ pkgs ? import <nixpkgs> { } }:
let
  unstable = import ./nix/unstable.nix;
in
pkgs.callPackage ./package.nix {
  inherit (unstable) yt-dlp deno;
  mpv = unstable.mpv-unwrapped;
}
