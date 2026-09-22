#!/usr/bin/env bash
# Developer ID sign, notarize, and staple the macOS plug-in bundles.
#
#   bash scripts/sign-macos.sh dist/macos "Developer ID Application: Name (TEAMID)"
#
# Signing alone needs only the identity. Notarization and stapling also run
# when all three App Store Connect API variables are set:
#
#   APPLE_API_KEY_PATH   path to AuthKey_<KEY_ID>.p8
#   APPLE_API_KEY        the key id
#   APPLE_API_ISSUER     the team issuer id
#
# The bundles come from build-macos.sh, which leaves them ad-hoc signed; this
# replaces that signature. Run on macOS with Xcode.
set -euo pipefail
if [[ "$(uname -s)" != Darwin ]]; then
  echo "Signing requires macOS and Xcode's codesign, notarytool, and stapler." >&2
  exit 1
fi
if [[ $# -ne 2 ]]; then
  echo "usage: $0 <bundle directory> <signing identity>" >&2
  exit 64
fi
directory="$1"
identity="$2"
if [[ ! -d "$directory" || -z "$identity" ]]; then
  echo "usage: $0 <bundle directory> <signing identity>" >&2
  exit 64
fi
if [[ "$identity" == "-" ]]; then
  echo "Refusing to notarize an ad-hoc signature; pass a Developer ID Application identity." >&2
  exit 1
fi

notarize=""
if [[ -n "${APPLE_API_KEY_PATH:-}" || -n "${APPLE_API_KEY:-}" || -n "${APPLE_API_ISSUER:-}" ]]; then
  if [[ -z "${APPLE_API_KEY_PATH:-}" || -z "${APPLE_API_KEY:-}" || -z "${APPLE_API_ISSUER:-}" ]]; then
    echo "Set all of APPLE_API_KEY_PATH, APPLE_API_KEY, and APPLE_API_ISSUER to notarize, or none to sign only." >&2
    exit 1
  fi
  test -f "$APPLE_API_KEY_PATH"
  notarize=1
fi

bundles=()
for format in FITS XISF; do
  bundle="$directory/Seiza${format}.plugin"
  executable="$bundle/Contents/MacOS/Seiza${format}"
  # Treat the bundle as data first: it may have arrived as a CI artifact.
  test -d "$bundle"
  test -f "$executable"
  test -f "$bundle/Contents/Resources/PiPLs.json"
  if [[ -n "$(find "$bundle" -type l -print -quit)" ]]; then
    echo "Unexpected symlink inside $bundle" >&2
    exit 1
  fi
  identifier="$(plutil -extract CFBundleIdentifier raw "$bundle/Contents/Info.plist")"
  test "$identifier" = "org.seiza.photoshop.$(printf '%s' "$format" | tr '[:upper:]' '[:lower:]')"
  test "$(plutil -extract CFBundlePackageType raw "$bundle/Contents/Info.plist")" = "8BIF"
  xcrun lipo "$executable" -verify_arch arm64 x86_64
  bundles+=("$bundle")
done

for bundle in "${bundles[@]}"; do
  # Hardened runtime and a secure timestamp are notarization requirements.
  codesign --force --options runtime --timestamp --sign "$identity" "$bundle"
  codesign --verify --deep --strict --verbose=4 "$bundle"
  codesign --display --verbose=2 "$bundle" 2>&1 | grep -q '^Authority=Developer ID Application'
  codesign --display --verbose=2 "$bundle" 2>&1 | grep -q '^Timestamp='
done
echo "Signed ${bundles[*]} as $identity"

if [[ -z "$notarize" ]]; then
  echo 'Not notarized: set APPLE_API_KEY_PATH, APPLE_API_KEY, and APPLE_API_ISSUER to notarize and staple.'
  exit 0
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
submission="$work/Seiza-Photoshop-notarization.zip"
# One submission covers both bundles; each gets its own ticket to staple.
ditto -c -k --sequesterRsrc --keepParent "$directory" "$submission"
bash "$(dirname "$0")/notarize-macos.sh" "$submission" "${bundles[@]}"
echo "Notarized and stapled ${bundles[*]}"
