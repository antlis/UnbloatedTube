{
  description = "unbloated-youtube: lightweight, configurable YouTube desktop client (GPUI, embedded mpv, yt-dlp)";

  # The same nixpkgs-unstable revision as nix/unstable.nix: YouTube breaks old yt-dlp quickly,
  # so yt-dlp, deno and mpv come from a recent one. (Bump both together.)
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/c59305bab2065cfecc4944690d9eedbb56f3a9fa";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      build = pkgs: pkgs.callPackage ./package.nix { mpv = pkgs.mpv-unwrapped; };
    in
    {
      packages = forAllSystems (pkgs: rec {
        unbloated-youtube = build pkgs;
        default = unbloated-youtube;
      });

      # For a NixOS / home-manager config that has its own nixpkgs: pkgs.unbloated-youtube.
      overlays.default = final: _prev: { unbloated-youtube = build final; };
    };
}
