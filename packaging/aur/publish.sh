#!/usr/bin/env bash
# Publish a GitHub release to the AUR as unbloated-youtube-bin: downloads the release archive,
# checks it against the release's .sha256, sets pkgver and sha256sums in PKGBUILD, generates
# .SRCINFO and pushes both to the AUR. The copy of PKGBUILD in this repo is the template; the
# AUR repo is where the current version lives, so nothing is written back here.
#
#   packaging/aur/publish.sh 0.8.2            # needs an SSH key the AUR knows (see GIT_SSH_COMMAND)
#   packaging/aur/publish.sh v0.8.2 --dry-run # everything except the push; prints the files
#
# Run by the release workflow after each tag when the AUR_SSH_KEY secret is set.
set -euo pipefail

version=${1:?usage: publish.sh VERSION [--dry-run]}
version=${version#v}
dry=${2:-}
repo=antlis/unbloated-youtube
aur=ssh://aur@aur.archlinux.org/unbloated-youtube-bin.git
author_name=antlis
author_email=antlis@protonmail.com

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

archive=unbloated-youtube-$version-x86_64-linux.tar.gz
base=https://github.com/$repo/releases/download/v$version
curl -fsSL -o "$work/$archive" "$base/$archive"
curl -fsSL -o "$work/$archive.sha256" "$base/$archive.sha256"
(cd "$work" && sha256sum -c "$archive.sha256")
sum=$(cut -d' ' -f1 "$work/$archive.sha256")

mkdir "$work/pkg"
sed -e "s/^pkgver=.*/pkgver=$version/" \
    -e "s/^pkgrel=.*/pkgrel=1/" \
    -e "s/^sha256sums=.*/sha256sums=('$sum')/" "$here/PKGBUILD" > "$work/pkg/PKGBUILD"

# .SRCINFO from the PKGBUILD, so this doesn't need makepkg (and so works off Arch).
# The variables below come from the PKGBUILD.
# shellcheck disable=SC2154
(
  # shellcheck source=/dev/null
  source "$work/pkg/PKGBUILD"
  list() { local key=$1; shift; for v in "$@"; do printf '\t%s = %s\n' "$key" "$v"; done; }
  {
    echo "pkgbase = $pkgname"
    printf '\tpkgdesc = %s\n\tpkgver = %s\n\tpkgrel = %s\n\turl = %s\n' "$pkgdesc" "$pkgver" "$pkgrel" "$url"
    list arch "${arch[@]}"
    list license "${license[@]}"
    list depends "${depends[@]}"
    list provides "${provides[@]}"
    list conflicts "${conflicts[@]}"
    list options "${options[@]}"
    list source "${source[@]}"
    list sha256sums "${sha256sums[@]}"
    printf '\npkgname = %s\n' "$pkgname"
  } > "$work/pkg/.SRCINFO"
)

git clone -q "$aur" "$work/aur"
cp "$work/pkg/PKGBUILD" "$work/pkg/.SRCINFO" "$work/aur/"
cd "$work/aur"
git add PKGBUILD .SRCINFO
if git diff --cached --quiet; then
  echo "AUR already has $version"
  exit 0
fi
git diff --cached --stat
git -c user.name="$author_name" -c user.email="$author_email" commit -q -m "Update to $version"
if [ "$dry" = --dry-run ]; then
  echo "dry run: not pushing"
  cat .SRCINFO
else
  git push -q origin HEAD:master
  echo "pushed $version to the AUR"
fi
