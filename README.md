# Seiza astronomy formats for Photoshop

[![Build and test plugins](https://github.com/theatrus/xisf-photoshop/actions/workflows/ci.yml/badge.svg)](https://github.com/theatrus/xisf-photoshop/actions/workflows/ci.yml)

Native Photoshop file-format plug-ins backed by `seiza-fits 0.2.2` and
`seiza-xisf 0.2.1`. The build produces **SeizaFITS** and **SeizaXISF** (`.8bi` on
Windows, `.plugin` on macOS), with entries in Photoshop's Open and Save dialogs.

**Development status:** Initial test builds for Windows x64 and universal macOS
(Intel and Apple silicon), compiled against Adobe's 2026 SDK v2. CI gates plugin
downloads on nine Rust tests, a compiled C++/Rust ABI test, and a host harness
that loads both plugins and exercises Adobe's `FormatRecord` interface.
Live Photoshop validation remains pending. macOS bundles are ad-hoc signed;
Developer ID signing and notarization are not configured. Plaintext SDKs are
not redistributed.

## Supported image workflow

- Open `.fits`, `.fit`, `.fts`, and `.xisf` into **32 Bits/Channel** grayscale or RGB.
- Save 32-bit grayscale/RGB composite pixels as Float32 FITS or XISF. Convert
  8/16-bit documents via **Image → Mode → 32 Bits/Channel** before saving.
- Floating-point inputs retain their physical values, including negative values
  and values above 1. No histogram normalization, automatic stretch, clipping,
  debayering, or gamma conversion is applied.
- Unsigned integer camera data uses a fixed full-scale divisor: 255 for 8-bit,
  65535 for 16-bit, and 4294967295 for XISF UInt32. Signed FITS integers and
  nonstandard FITS scaling retain physical units. FITS `BZERO`/`BSCALE` are applied
  once. Saving integer inputs consequently produces normalized Float32 pixels,
  not the original integer storage or ADU scale.
- XISF attached mono/RGB data supports the sample types, byte order, checksums,
  zlib, LZ4/LZ4HC, zstd, and byte shuffling provided by `seiza-xisf`.
- Float64/Int32/UInt32 inputs can lose precision when converted to Photoshop f32.
  Images containing NaN or infinite samples are rejected with an error.

Linear astro images may appear very dark. Apply your desired stretch in
Photoshop; the importer does not alter it for display. The plug-ins do not embed
an ICC profile, so color appearance depends on Photoshop's color settings.

## Current limits

- CI builds Windows x64 and universal macOS. Automated host tests run on Windows
  x64 and the Mac runner's native architecture; interactive Photoshop checks are
  still required on each supported architecture.
- FITS primary HDU only; extra HDUs are not opened. XISF opens its first image.
  FITS cubes other than one/three planes are rejected.
- Bayer/CFA data opens as grayscale; debayer before opening if you need color.
- Save stores one flattened image. Layers, alpha channels, masks, WCS/acquisition
  headers, XISF properties, ICC profiles, and other metadata are not round-tripped.
  Keep an original scientific image and use PSD/PSB for layered Photoshop work.
- XISF output is uncompressed Float32. FITS output uses `BITPIX=-32`.
- The first implementation retains the encoded input and decoded image in memory
  while opening; saving retains a planar f32 image. It is not an out-of-core codec.
  Cancellation is checked during host I/O and pixel transfer; upstream decoding
  itself is synchronous and cannot be interrupted mid-decode.

## Build

Requires Windows x64, Rust (pinned to 1.98.0 by `rust-toolchain.toml`), Visual Studio C++ build tools, a Windows SDK,
and the **Adobe Photoshop C++ SDK** (not the UXP or Connection SDK).
Adobe's [SDK download instructions](https://medium.com/adobetech/locate-and-download-the-photoshop-c-sdk-4f0e55f091ae)
link to the [Developer Console](https://developer.adobe.com/console/servicesandapis/ps).
Download/extract the SDK using your Adobe account; it is not checked into this project.

```powershell
# Codec tests, optimized Rust static library, and compiled C++/Rust ABI smoke test:
.\scripts\build.ps1 -BackendOnly

# Complete plug-in build and distributable ZIP:
.\scripts\build.ps1 -PhotoshopSdk C:\SDKs\PhotoshopSDK
# Or set PHOTOSHOP_SDK to the extracted SDK root.
```

The script locates Visual Studio, `PIFormat.h`, and Adobe's `cnvtpipl.exe`, then
builds the real PiPL resources and links Rust into each DLL. Outputs are in `dist`.
No separate Rust DLL or external conversion process is required. The build uses
the dynamic Microsoft C runtime supplied by supported Photoshop installations.

## Install and verify in Photoshop

After a successful native build, close Photoshop and install both plug-ins:

```powershell
.\scripts\install.ps1 -PluginDirectory 'C:\Program Files\Adobe\Adobe Photoshop 2026\Plug-ins'
```

Writing to Program Files requires an elevated PowerShell on this machine. Run
the installation command there, or copy the two `.8bi` files from `dist` into
Photoshop's `Plug-ins/Seiza` folder and approve Windows' administrator prompt.
The installer does not request elevation or change folder permissions itself.
Existing same-name plug-ins are backed up with `.bak`. Restart Photoshop after
installation. To uninstall, close Photoshop and remove the two `.8bi` files.

Generate known test images:

```powershell
cargo run --locked --example make_fixtures
```

1. Open all four images in `build/fixtures`. Verify 128×96 dimensions, 32-bit mode,
   and grayscale/RGB channels. Mono is a left-to-right ramp. RGB has red increasing
   left-to-right, green increasing top-to-bottom, and blue only in the upper-left.
2. Save copies using each Seiza format, reopen them, and verify the same pixels.
3. Repeat with a real compressed XISF, unsigned 16-bit FITS, and a Float32 HDR file.
4. Cancel a large import/export; confirm Photoshop remains usable and the original
   source file is intact. Reopen a valid image after a malformed-file error.
5. Confirm unavailable modes/extra alpha channels cannot be silently exported.

These in-Photoshop checks remain pending. The separate SDK host harness has
already verified both real DLLs with mono/RGB/HDR pixels, a 32768-pixel-wide image,
signature filtering, file-position restoration, cancellation/cleanup, unsupported
save depths, and malformed files. It does not emulate Photoshop's UI or color engine.

## macOS build

On a Mac, install Xcode, Rust, and Python 3, and extract the Mac Photoshop SDK:

```bash
bash scripts/build-macos.sh /path/to/photoshopsdk
```

This builds both `aarch64-apple-darwin` and `x86_64-apple-darwin`, combines them
with `lipo`, and creates `dist/macos/SeizaFITS.plugin` and `SeizaXISF.plugin` plus
a universal ZIP. The modern `PiPLs.json` bundle resources follow the supplied
2026 SDK's format. macOS uses Photoshop's borrowed POSIX file descriptors.
Run this on macOS; downloading the Adobe Mac SDK alone does not provide Apple's
compiler, macOS system headers, or the ability to test the plug-in from Windows.

Local bundles are ad-hoc signed. Set `CODE_SIGN_IDENTITY` to your Developer ID
for a signed build; public distribution also requires notarization. After saving
your documents and closing Photoshop, copy the bundles into its `Plug-ins` folder
and restart. The SDK and resource generators are never included in the bundles.

## Architecture and tests

`src/lib.rs` owns sample semantics; `src/ffi.rs` exposes a panic-contained C ABI.
`native/plugin.cpp` uses Adobe's `FormatRecord` directly and transfers planar rows
through `advanceState`. Two PiPL resources register separate formats so Save
chooses the requested encoder independently of the output filename.

`cargo test` covers FITS scaling, signed values, fixed integer normalization,
Float32 round trips, endian conversion, shuffled compression, planar channels,
malformed input, non-finite values, FFI ownership, and write failures.
`native/codec_smoke.cpp` links the actual release Rust library into a C++ executable
and verifies both formats across the language boundary. `native/host_smoke.cpp`
loads the compiled plug-ins through `PluginMain` and uses the real SDK structures.
Both platform build scripts run that harness before packaging.

## CI and downloads

[GitHub Actions](https://github.com/theatrus/xisf-photoshop/actions/workflows/ci.yml)
tests the Rust backend on Windows and macOS for pushes and pull requests. Pushes
to `main`, `v*` tags, and manual runs also compile both native plugins using the
Adobe 2026 v2 SDK. Successful builds provide two artifacts (GitHub login required):

- `Seiza-Photoshop-Windows-x64`: ZIP containing both `.8bi` plugins and docs.
- `Seiza-Photoshop-macOS-universal`: ZIP containing both `.plugin` bundles for
  Intel and Apple silicon, ad-hoc signed, plus docs. Preserve the inner ZIP when
  copying it to a Mac so bundle permissions and signatures survive.

Artifacts expire after 30 days; run the workflow manually to rebuild them.
Native builds run the C++/Rust ABI test and load both compiled plugins in a
minimal SDK host harness. The macOS runner tests its native architecture;
`lipo` checks that both architectures are in each bundle. These tests do not
replace interactive testing in Photoshop. Developer ID signing and notarization
are not configured.

The `sdk-2026-v2` prerelease holds **encrypted build inputs, not installable
plugins**. Adobe's SDK remains outside Git and plugin artifacts. CI decrypts it
with the `PHOTOSHOP_SDK_PASSPHRASE` repository secret, verifies the original ZIP's
SHA-256, and removes plaintext inputs after the build. This follows GitHub's
[large secret storage pattern](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-secrets#storing-large-secrets).
Pull requests never receive the SDK or its key; no `pull_request_target` workflow
is used. Fork owners can run backend tests or build locally with their own SDK.

To update the SDK, encrypt both new ZIPs using GnuPG AES256 with a strong shared
random passphrase supplied through stdin, upload the `.gpg` files to a new build
input release, update `scripts/ci_sdk.py` hashes and the workflow release/asset
names, and set the Actions secret to that passphrase. Never upload plaintext SDK
archives or log decrypted contents. Updating both archives and the secret together
avoids mixing encryption keys across platforms.

The Photoshop C++ SDK is Adobe's documented route for adding file formats:
[Photoshop extensibility](https://developer.adobe.com/photoshop/).
