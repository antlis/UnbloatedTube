#!/usr/bin/env bash
# Builds the AppImage from the release archive the workflow already made (the same binary, built
# on Ubuntu 22.04, glibc 2.35), with the libraries of the machine this runs on (Ubuntu 24.04, so
# glibc 2.39 is needed to run it):
#
#   packaging/appimage/build.sh unbloatedtube-X.Y.Z-x86_64-linux.tar.gz
#
# Writes unbloatedtube-X.Y.Z-x86_64.AppImage and its .sha256 into the current directory.
# Needs curl, unzip, file, patchelf and libfuse2 (or FUSE-less: the tools run extracted), and
# the libraries the binary links against, and libmpv2, installed, because linuxdeploy copies them from here.
set -euo pipefail

archive=$(readlink -f "${1:?usage: build.sh RELEASE_ARCHIVE.tar.gz}")
here=$(cd "$(dirname "$0")" && pwd)
out=$(pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"

# The release archive: the binary, the desktop entry and the icon.
tar xzf "$archive"
dir=$(find . -maxdepth 1 -type d -name 'unbloatedtube-*' | head -1)
version=$(basename "$dir" | sed -E 's/^unbloatedtube-(.*)-x86_64-linux$/\1/')

get() { curl -fsSL --retry 3 -o "$2" "$1"; chmod +x "$2"; }
get https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage linuxdeploy
get https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage appimagetool

# Fallback copies of what the app runs besides mpv. They sit at the end of PATH (see AppRun), so
# a yt-dlp or deno you installed yourself is used instead.
mkdir helpers
base=https://github.com/yt-dlp/yt-dlp/releases/latest/download
curl -fsSL --retry 3 -o helpers/yt-dlp "$base/yt-dlp_linux"
curl -fsSL --retry 3 "$base/SHA2-256SUMS" | grep ' yt-dlp_linux$' | sed 's/yt-dlp_linux/helpers\/yt-dlp/' | sha256sum -c -
base=https://github.com/denoland/deno/releases/latest/download
curl -fsSL --retry 3 -o deno.zip "$base/deno-x86_64-unknown-linux-gnu.zip"
echo "$(curl -fsSL --retry 3 "$base/deno-x86_64-unknown-linux-gnu.zip.sha256sum" | awk '{print $1}')  deno.zip" | sha256sum -c -
unzip -q deno.zip -d helpers
chmod +x helpers/yt-dlp helpers/deno

# AppDir: the binary with the libraries it links against, found through its RUNPATH. Not bundled:
# glibc, the GPU and the Vulkan loader (they come from the system), and libxkbcommon, whose
# compose tables are the system's: a bundled older copy prints errors about newer ones.
# libmpv is what plays the video (the app loads it at run time, so linuxdeploy has to be told); its
# own libraries (ffmpeg, libass, ...) come along.
libmpv=$(ldconfig -p | awk '/libmpv\.so\.[0-9]+ / { print $NF; exit }')
[ -n "$libmpv" ] || { echo "libmpv is not installed here (apt install libmpv2)" >&2; exit 1; }
./linuxdeploy --appimage-extract-and-run --appdir AppDir \
  --exclude-library 'libxkbcommon*' \
  --library "$libmpv" \
  --executable "$dir/unbloatedtube" \
  --desktop-file "$dir/unbloatedtube.desktop" \
  --icon-file "$dir/unbloatedtube.svg" \
  --custom-apprun "$here/AppRun"
# linuxdeploy leaves out libraries it assumes every desktop has (its excludelist), and libmpv needs
# some a clean machine may lack (found by check-clean.sh): copy those and what they need.
host_only='^(linux-vdso|ld-linux|libc|libm|libdl|libpthread|librt|libresolv|libutil|libnsl|libgcc_s|libstdc\+\+|libGL|libEGL|libGLX|libGLdispatch|libOpenGL|libvulkan|libdrm|libgbm|libxcb|libX|libxkbcommon|libwayland)[.-]'
extra() {
  local name=$1 path
  [ -e "AppDir/usr/lib/$name" ] && return 0
  [[ $name =~ $host_only ]] && return 0
  path=$(ldconfig -p | awk -v n="$name" '$1 == n && /x86-64/ { print $NF; exit }')
  [ -n "$path" ] || { echo "cannot find $name here" >&2; exit 1; }
  cp -L "$path" "AppDir/usr/lib/$name"
  # The copy finds its own libraries next to it, as the ones linuxdeploy deployed do.
  patchelf --set-rpath '$ORIGIN' "AppDir/usr/lib/$name"
  ldd "$path" | awk '/=> \//{ print $1 }' | while read -r dep; do extra "$dep"; done
}
for name in libharfbuzz.so.0 libfribidi.so.0 libusb-1.0.so.0 libjack.so.0; do extra "$name"; done

mkdir -p AppDir/usr/helpers
cp helpers/yt-dlp helpers/deno AppDir/usr/helpers/
cp "$dir/LICENSE" AppDir/usr/share/doc-LICENSE 2>/dev/null || true

name="unbloatedtube-$version-x86_64.AppImage"
ARCH=x86_64 ./appimagetool --appimage-extract-and-run --no-appstream AppDir "$name"
cp "$name" "$out/"
(cd "$out" && sha256sum "$name" > "$name.sha256")
echo "built $out/$name"
