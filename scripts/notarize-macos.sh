#!/usr/bin/env bash
# Submit a ZIP or DMG, then staple every supplied bundle/disk-image target.
set -euo pipefail
if [[ "$(uname -s)" != Darwin || $# -lt 2 ]]; then
  echo "usage (macOS): $0 <submission.zip|dmg> <staple target> [...]" >&2
  exit 64
fi
: "${APPLE_API_KEY_PATH:?Set APPLE_API_KEY_PATH}"
: "${APPLE_API_KEY:?Set APPLE_API_KEY}"
: "${APPLE_API_ISSUER:?Set APPLE_API_ISSUER}"
test -f "$APPLE_API_KEY_PATH"
submission="$1"
shift
test -f "$submission"
for target in "$@"; do test -e "$target"; done
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
result="$work/submission.json"
xcrun notarytool submit "$submission" \
  --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER" \
  --wait --timeout 30m --output-format json > "$result" || true
read_result() {
  # notarytool may print progress objects before the final one; use the last.
  python3 - "$result" "$1" <<'PY'
import json, sys
text = open(sys.argv[1]).read()
decoder, index, last = json.JSONDecoder(), 0, {}
while True:
    start = text.find("{", index)
    if start < 0:
        break
    try:
        last, index = decoder.raw_decode(text, start)
    except ValueError:
        index = start + 1
print(last.get(sys.argv[2], "") if isinstance(last, dict) else "")
PY
}
submission_id="$(read_result id)"
status="$(read_result status)"
if [[ -z "$submission_id" ]]; then
  echo 'notarytool returned no submission id:' >&2
  cat "$result" >&2
  exit 1
fi
echo "Notarization $submission_id: ${status:-unknown}"
if [[ "$status" != Accepted ]]; then
  xcrun notarytool log "$submission_id" \
    --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER" >&2 || true
  exit 1
fi
for target in "$@"; do
  xcrun stapler staple "$target"
  xcrun stapler validate "$target"
  codesign --verify --deep --strict --verbose=4 "$target"
done
