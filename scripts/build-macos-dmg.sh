#!/usr/bin/env bash
# Package both bundles beside a shortcut to Adobe's shared Photoshop plug-in folder.
# Optional Developer ID identity signs the DMG; APPLE_API_* also notarizes it.
set -euo pipefail
if [[ "$(uname -s)" != Darwin ]]; then
  echo 'DMG packaging requires macOS.' >&2
  exit 1
fi
if [[ $# -lt 1 || $# -gt 2 || ! -d "$1" ]]; then
  echo "usage: $0 <bundle directory> [Developer ID Application identity]" >&2
  exit 64
fi
directory="$(cd "$1" && pwd)"
identity="${2:-}"
cd "$(dirname "$0")/.."
if [[ "$identity" == '-' ]]; then
  echo 'Use a Developer ID Application identity, or omit it for a local unsigned DMG.' >&2
  exit 1
fi
notarize=''
if [[ -n "${APPLE_API_KEY_PATH:-}${APPLE_API_KEY:-}${APPLE_API_ISSUER:-}" ]]; then
  if [[ -z "$identity" || -z "${APPLE_API_KEY_PATH:-}" || -z "${APPLE_API_KEY:-}" || -z "${APPLE_API_ISSUER:-}" ]]; then
    echo 'Notarizing the DMG requires a signing identity and all three APPLE_API_* variables.' >&2
    exit 1
  fi
  notarize=1
fi
version="$(plutil -extract CFBundleShortVersionString raw "$directory/SeizaFITS.plugin/Contents/Info.plist")"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo 'Invalid bundle version.' >&2
  exit 1
fi
for format in FITS XISF; do
  bundle="$directory/Seiza${format}.plugin"
  test "$(plutil -extract CFBundleShortVersionString raw "$bundle/Contents/Info.plist")" = "$version"
  test "$(plutil -extract CFBundleIdentifier raw "$bundle/Contents/Info.plist")" = "org.seiza.photoshop.$(printf '%s' "$format" | tr '[:upper:]' '[:lower:]')"
  codesign --verify --deep --strict "$bundle"
  xcrun lipo "$bundle/Contents/MacOS/Seiza${format}" -verify_arch arm64 x86_64
done

mkdir -p dist
output="$(pwd)/dist/Seiza-Photoshop-macOS-universal-${version}.dmg"
# Never replace a previously signed release accidentally.
if [[ -e "$output" ]]; then
  echo "Output already exists: $output. Move it aside before rebuilding." >&2
  exit 1
fi
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
stage="$work/staging"
mkdir "$stage"
for format in FITS XISF; do
  ditto "$directory/Seiza${format}.plugin" "$stage/Seiza${format}.plugin"
done
# An absolute symlink resolves on the user's Mac, even when absent on the runner.
# Do not create or modify the real destination during packaging or verification.
ln -s '/Library/Application Support/Adobe/Plug-Ins/CC' "$stage/Photoshop Plug-ins"
cp scripts/macos-install.txt "$stage/Read me first.txt"
mkdir "$stage/Documentation"
cp README.md NOTICE LICENSE "$stage/Documentation/"
hdiutil create -volname "FITS and XISF ${version}" -srcfolder "$stage" -fs HFS+ -format UDZO "$output"

if [[ -n "$identity" ]]; then
  codesign --force --timestamp --sign "$identity" --identifier org.seiza.photoshop.disk-image "$output"
  codesign --verify --strict --verbose=2 "$output"
fi
if [[ -n "$notarize" ]]; then
  bash scripts/notarize-macos.sh "$output" "$output"
  spctl --assess --type open --context context:primary-signature --verbose=2 "$output"
fi
bash scripts/test-macos-dmg.sh "$output" "$directory"
if [[ -n "$notarize" ]]; then
  xcrun stapler validate "$output"
fi
(cd dist && shasum -a 256 "$(basename "$output")" > "$(basename "$output").sha256")
echo "Built and verified $output"
