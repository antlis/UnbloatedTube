#!/usr/bin/env bash
# Does the AppImage bring every library it needs? Run it on a clean ubuntu:22.04 (the build host
# has everything installed, so ldd there proves nothing) with only the libraries a desktop has:
#
#   docker run --rm -v "$PWD:/in:ro" -v "$PWD/packaging/appimage/check-clean.sh:/check.sh:ro" ubuntu:22.04 /check.sh
#
# Prints the libraries still missing; exits 1 if there are any.
set -u
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq >/dev/null
apt-get install -y -qq --no-install-recommends mesa-vulkan-drivers libvulkan1 libxkbcommon0 libxkbcommon-x11-0 \
  libx11-xcb1 libxcb1 libegl1 libgl1 libasound2 libwayland-client0 libfontconfig1 libxcb-xkb1 ca-certificates >/dev/null
cp /in/unbloatedtube-*-x86_64.AppImage /tmp/app
# A host that hands AppImages to a helper (binfmt_misc) can't run it in here: blank the marker.
printf '\0\0\0' | dd of=/tmp/app bs=1 seek=8 conv=notrunc 2>/dev/null
chmod +x /tmp/app
(cd /tmp && ./app --appimage-extract >/dev/null)
root=/tmp/squashfs-root
export LD_LIBRARY_PATH=$root/usr/lib
missing=$(for f in "$root/usr/bin/unbloatedtube" "$root"/usr/lib/*.so*; do ldd "$f" 2>/dev/null | grep 'not found'; done | sed 's/^[[:space:]]*//; s/ => not found//' | sort | uniq -c | sort -rn)
if [ -n "$missing" ]; then echo "missing libraries:"; echo "$missing"; exit 1; fi
echo "no library missing"
