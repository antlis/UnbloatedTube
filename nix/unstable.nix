# Keep the revision in step with flake.nix. nixpkgs-unstable, pinned so nix caches it (an unpinned channel URL is re-downloaded each time).
# YouTube breaks old yt-dlp quickly (403 on streams), so yt-dlp, deno and mpv come from here.
import (fetchTarball {
  url = "https://github.com/NixOS/nixpkgs/archive/c59305bab2065cfecc4944690d9eedbb56f3a9fa.tar.gz";
  sha256 = "16rsfnnxk6294sz6asx0shblirkhm4yyvkimq3v00c0y2114gp7b";
}) { }
