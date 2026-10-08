{ lib
, rustPlatform
, pkg-config
, cmake
, clang
, makeWrapper
, runCommand
, fontconfig
, freetype
, libxkbcommon
, wayland
, vulkan-loader
, xorg
, openssl
, zstd
, alsa-lib
, mpv
, yt-dlp
, deno
, mpvScripts
  # crane's library (the flake passes it): it builds the dependencies as a derivation of their own,
  # so a new version of this app recompiles only this crate. Without it (default.nix) everything
  # is built in one go.
, craneLib ? null
}:
let
  buildInputs = [
    fontconfig freetype libxkbcommon wayland vulkan-loader
    xorg.libxcb xorg.libX11 xorg.libXcursor xorg.libXi xorg.libXrandr
    openssl zstd alsa-lib
  ];

  common = {
    pname = "unbloated-youtube";
    version = (lib.importTOML ./Cargo.toml).package.version;

    src = lib.fileset.toSource {
      root = ./.;
      fileset = lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./src ./packaging ];
    };

    # There are no automated tests; running them would also compile the whole test profile.
    doCheck = false;

    nativeBuildInputs = [ pkg-config cmake clang makeWrapper ];
    inherit buildInputs;
  };

  # What the dependency build sees: the two manifests with the app's own version replaced, and an
  # empty main.rs for crane to put a dummy program in. Releases only change that version, so this
  # (and with it the compiled dependencies) stays the same derivation until Cargo.lock changes
  # something else.
  depsSrc = runCommand "unbloated-youtube-deps-src" { } ''
    mkdir -p $out/src
    touch $out/src/main.rs
    sed '0,/^version = ".*"/s//version = "0.0.0"/' ${./Cargo.toml} > $out/Cargo.toml
    sed '/^name = "unbloated-youtube"$/{n;s/.*/version = "0.0.0"/}' ${./Cargo.lock} > $out/Cargo.lock
  '';

  # The app runs mpv and yt-dlp (which needs deno for YouTube's JavaScript challenges) as programs,
  # and gpui loads Vulkan and the window system libraries at run time. SponsorBlock is a script mpv loads.
  rest = {
    postInstall = ''
      install -Dm644 packaging/unbloated-youtube.desktop $out/share/applications/unbloated-youtube.desktop
      install -Dm644 packaging/unbloated-youtube.svg $out/share/icons/hicolor/scalable/apps/unbloated-youtube.svg
      wrapProgram $out/bin/unbloated-youtube \
        --prefix PATH : ${lib.makeBinPath [ mpv yt-dlp deno ]} \
        --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath buildInputs} \
        --set-default UNBLOATED_SPONSORBLOCK ${mpvScripts.sponsorblock-minimal}/share/mpv/scripts/sponsorblock_minimal.lua
    '';

    meta = {
      description = "Lightweight, configurable YouTube desktop client: GPUI, embedded mpv, yt-dlp";
      homepage = "https://github.com/antlis/unbloated-youtube";
      mainProgram = "unbloated-youtube";
      license = lib.licenses.agpl3Only;
      platforms = lib.platforms.linux;
    };
  };
in
if craneLib == null then
  rustPlatform.buildRustPackage (common // rest // { cargoLock.lockFile = ./Cargo.lock; })
else
  craneLib.buildPackage (common // rest // {
    # Compiled from depsSrc (see above), so it is reused from release to release.
    cargoArtifacts = craneLib.buildDepsOnly (common // { src = depsSrc; version = "0.0.0"; });
  })
