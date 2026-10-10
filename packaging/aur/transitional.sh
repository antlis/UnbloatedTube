#!/usr/bin/env bash
# Push packaging/aur/transitional/PKGBUILD to the AUR as the last unbloated-youtube-bin: an
# empty package that depends on unbloatedtube-bin, so upgrading users are moved to the new
# name. Run it once, by hand, after unbloatedtube-bin exists on the AUR (the 0.34.0 release).
#
#   packaging/aur/transitional.sh            # needs an SSH key the AUR knows (see GIT_SSH_COMMAND)
#   packaging/aur/transitional.sh --dry-run  # everything except the push; prints .SRCINFO
set -euo pipefail

dry=${1:-}
aur=ssh://aur@aur.archlinux.org/unbloated-youtube-bin.git
author_name=antlis
author_email=antlis@protonmail.com

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# .SRCINFO from the PKGBUILD, as publish.sh does (no makepkg needed).
# shellcheck disable=SC2154
(
  # shellcheck source=/dev/null
  source "$here/transitional/PKGBUILD"
  list() { local key=$1; shift; for v in "$@"; do printf '\t%s = %s\n' "$key" "$v"; done; }
  {
    echo "pkgbase = $pkgname"
    printf '\tpkgdesc = %s\n\tpkgver = %s\n\tpkgrel = %s\n\turl = %s\n' "$pkgdesc" "$pkgver" "$pkgrel" "$url"
    list arch "${arch[@]}"
    list license "${license[@]}"
    list depends "${depends[@]}"
    printf '\npkgname = %s\n' "$pkgname"
  } > "$work/SRCINFO"
)

git clone -q "$aur" "$work/aur"
cp "$here/transitional/PKGBUILD" "$work/aur/PKGBUILD"
cp "$work/SRCINFO" "$work/aur/.SRCINFO"
cd "$work/aur"
git add PKGBUILD .SRCINFO
if git diff --cached --quiet; then
  echo "the AUR already has the transitional package"
  exit 0
fi
git diff --cached --stat
git -c user.name="$author_name" -c user.email="$author_email" commit -q -m "Transitional package: renamed to unbloatedtube-bin"
if [ "$dry" = --dry-run ]; then
  echo "dry run: not pushing"
  cat .SRCINFO
else
  git push -q origin HEAD:master
  echo "pushed the transitional unbloated-youtube-bin"
fi
