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
if [[ -z "${SEIZA_DMG_PYTHON:-}" ]]; then
  python3 -c 'import sys; sys.exit("DMG verification requires Python 3.10 or newer on PATH.") if sys.version_info < (3, 10) else None'
  python3 -m venv "$work/python"
  "$work/python/bin/python" -m pip install --disable-pip-version-check -r "$(dirname "$0")/dmg-requirements.txt"
  SEIZA_DMG_PYTHON="$work/python/bin/python"
fi
"$SEIZA_DMG_PYTHON" "$(dirname "$0")/dmg-layout.py" --verify "$mount"
for doc in README.md NOTICE LICENSE; do
  test -s "$mount/Documentation/$doc"
done
for format in FITS XISF; do
  bundle="$mount/Seiza${format}.plugin"
  test -x "$bundle/Contents/MacOS/Seiza${format}"
  for arch in arm64 x86_64; do
    xcrun lipo "$bundle/Contents/MacOS/Seiza${format}" -verify_arch "$arch"
  done
  codesign --verify --deep --strict --verbose=2 "$bundle"
  # Includes executable bytes, metadata, signatures, resources, and stapled tickets.
  diff -r "$directory/Seiza${format}.plugin" "$bundle"
done
echo 'DMG verified: both universal plugins unchanged, destination shortcut and installation instructions present.'
