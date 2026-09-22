#!/usr/bin/env bash
# Inspect a read-only mount; never install into the actual Photoshop directory.
set -euo pipefail
if [[ "$(uname -s)" != Darwin || $# -ne 2 ]]; then
  echo "usage (macOS): $0 <image.dmg> <source bundle directory>" >&2
  exit 64
fi
image="$1"
directory="$2"
hdiutil verify "$image"
work="$(mktemp -d)"
mount="$work/volume"
attached=''
cleanup() {
  if [[ -n "$attached" ]]; then
    hdiutil detach "$mount" || hdiutil detach -force "$mount" || return 1
  fi
  rm -rf "$work"
}
trap cleanup EXIT
mkdir "$mount"
hdiutil attach -readonly -nobrowse -mountpoint "$mount" "$image"
attached=1
test -L "$mount/Photoshop Plug-ins"
test "$(readlink "$mount/Photoshop Plug-ins")" = '/Library/Application Support/Adobe/Plug-Ins/CC'
test -s "$mount/Read me first.txt"
for doc in README.md NOTICE LICENSE; do
  test -s "$mount/Documentation/$doc"
done
for format in FITS XISF; do
  bundle="$mount/Seiza${format}.plugin"
  test -x "$bundle/Contents/MacOS/Seiza${format}"
  xcrun lipo "$bundle/Contents/MacOS/Seiza${format}" -verify_arch arm64 x86_64
  codesign --verify --deep --strict --verbose=2 "$bundle"
  # Includes executable bytes, metadata, signatures, resources, and stapled tickets.
  diff -r "$directory/Seiza${format}.plugin" "$bundle"
done
echo 'DMG verified: both universal plugins unchanged, destination shortcut and installation instructions present.'
