{ lib
, rustPlatform
, pkg-config
, cmake
, clang
, makeWrapper
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
}:
let
  buildInputs = [
    fontconfig freetype libxkbcommon wayland vulkan-loader
    xorg.libxcb xorg.libX11 xorg.libXcursor xorg.libXi xorg.libXrandr
    openssl zstd alsa-lib
  ];
in
rustPlatform.buildRustPackage {
  pname = "unbloated-youtube";
  version = (lib.importTOML ./Cargo.toml).package.version;

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./src ./packaging ];
  };
  cargoLock.lockFile = ./Cargo.lock;

  # There are no automated tests; running them would also compile the whole test profile.
  doCheck = false;

  nativeBuildInputs = [ pkg-config cmake clang makeWrapper ];
  inherit buildInputs;

  # The app runs mpv and yt-dlp (which needs deno for YouTube's JavaScript challenges) as programs,
  # and gpui loads Vulkan and the window system libraries at run time. SponsorBlock is a script mpv loads.
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
    platforms = lib.platforms.linux;
  };
}
