#!/usr/bin/env bash
# Run on a Mac with Xcode, Python 3, Rust and the extracted Photoshop C++ SDK.
set -euo pipefail
if [[ "$(uname -s)" != Darwin ]]; then
  echo "This build requires macOS and Xcode's macOS SDK." >&2
  exit 1
fi
sdk="${1:-${PHOTOSHOP_SDK:-}}"
if [[ -z "$sdk" || ! -d "$sdk" ]]; then
  echo "Usage: bash scripts/build-macos.sh /path/to/photoshopsdk" >&2
  exit 1
fi
sdk="$(cd "$sdk" && pwd)"
headers=()
while IFS= read -r path; do headers+=("$path"); done < <(find "$sdk" -name PIFormat.h -not -path '*/__MACOSX/*')
if [[ ${#headers[@]} != 1 ]]; then
  echo "SDK must contain exactly one photoshopapi/photoshop/PIFormat.h" >&2
  exit 1
fi
api="$(dirname "$(dirname "${headers[0]}")")"
plugin_sdk="$(dirname "$api")"
cd "$(dirname "$0")/.."
export MACOSX_DEPLOYMENT_TARGET=11.0
includes=(-I"$api/photoshop" -I"$api/pica_sp" -I"$api/resources" -I"$plugin_sdk/samplecode/common/includes")
cargo fmt --check
cargo test --locked
rustup target add aarch64-apple-darwin x86_64-apple-darwin
mkdir -p build/native-macos dist/macos
python3 scripts/mac_bundle.py dist/macos
for arch in arm64 x86_64; do
  target=x86_64-apple-darwin
  [[ "$arch" == arm64 ]] && target=aarch64-apple-darwin
  cargo build --locked --release --target "$target"
  library="target/$target/release/libseiza_photoshop.a"
  for format in FITS XISF; do
    id=1
    [[ "$format" == XISF ]] && id=2
    xcrun clang++ -std=c++17 -O2 -fvisibility=hidden -arch "$arch" -mmacosx-version-min=11.0 \
      -bundle -DSEIZA_FORMAT="$id" "${includes[@]}" native/plugin.cpp "$library" \
      -framework CoreFoundation -framework Security -framework SystemConfiguration -liconv \
      -o "build/native-macos/Seiza${format}-${arch}"
  done
done
for format in FITS XISF; do
  bundle="dist/macos/Seiza${format}.plugin"
  xcrun lipo -create "build/native-macos/Seiza${format}-arm64" "build/native-macos/Seiza${format}-x86_64" \
    -output "$bundle/Contents/MacOS/Seiza${format}"
  xcrun lipo "$bundle/Contents/MacOS/Seiza${format}" -verify_arch arm64 x86_64
  codesign --force --sign "${CODE_SIGN_IDENTITY:--}" "$bundle"
  codesign --verify --strict "$bundle"
done
host_target=x86_64-apple-darwin
[[ "$(uname -m)" == arm64 ]] && host_target=aarch64-apple-darwin
host_library="target/$host_target/release/libseiza_photoshop.a"
xcrun clang++ -std=c++17 -O2 native/codec_smoke.cpp "$host_library" \
  -framework CoreFoundation -framework Security -framework SystemConfiguration -liconv \
  -o build/native-macos/codec_smoke
build/native-macos/codec_smoke
xcrun clang++ -std=c++17 -O2 "${includes[@]}" native/host_smoke.cpp "$host_library" \
  -framework CoreFoundation -framework Security -framework SystemConfiguration -liconv \
  -o build/native-macos/host_smoke
build/native-macos/host_smoke
cp README.md NOTICE LICENSE dist/macos/
ditto -c -k --sequesterRsrc --keepParent dist/macos dist/Seiza-Photoshop-macOS-universal.zip
echo 'Built dist/macos/SeizaFITS.plugin and dist/macos/SeizaXISF.plugin'
echo 'Local builds are ad-hoc signed. Public distribution needs Developer ID signing and notarization.'
