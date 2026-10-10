{
  description = "unbloatedtube: lightweight, configurable YouTube desktop client (GPUI, embedded mpv, yt-dlp)";

  # The same nixpkgs-unstable revision as nix/unstable.nix: YouTube breaks old yt-dlp quickly,
  # so yt-dlp, deno and mpv come from a recent one. (Bump both together.)
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/c59305bab2065cfecc4944690d9eedbb56f3a9fa";

  # Builds the Rust dependencies as their own derivation, so a new release of the app doesn't
  # recompile all ~730 crates (see package.nix).
  inputs.crane.url = "github:ipetkov/crane";

  outputs = { self, nixpkgs, crane }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      build = pkgs: pkgs.callPackage ./package.nix {
        mpv = pkgs.mpv-unwrapped;
        craneLib = crane.mkLib pkgs;
      };
    in
    {
      packages = forAllSystems (pkgs: rec {
        unbloatedtube = build pkgs;
        default = unbloatedtube;
        # The name before 0.34.0, so existing configs keep building.
        unbloated-youtube = unbloatedtube;
      });

      # For a NixOS / home-manager config that has its own nixpkgs: pkgs.unbloatedtube.
      overlays.default = final: _prev: rec {
        unbloatedtube = build final;
        unbloated-youtube = unbloatedtube;
      };
    };
}
