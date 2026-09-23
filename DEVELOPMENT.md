# Development guide

Build, testing, and release information for contributors. For downloads and usage,
see the [README](README.md).

## Windows build

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

# Optional Windows installer (after the complete plugin build):
.\scripts\build-installer.ps1
# Exercise installation, upgrades, rollback, and uninstall in an isolated tree:
.\scripts\test-installer.ps1
```

The script locates Visual Studio, `PIFormat.h`, and Adobe's `cnvtpipl.exe`, then
builds the real PiPL resources and links Rust into each DLL. Outputs are in `dist`.
No separate Rust DLL or external conversion process is required. The build uses
the dynamic Microsoft C runtime supplied by supported Photoshop installations.

The installer build downloads a SHA-256-pinned Inno Setup 6.7.3 compiler into
`build/tools` in portable mode. It packages only the tested plugins and docs;
neither Adobe's SDK nor Inno Setup's compiler is distributed. The output is
`dist/Seiza-Photoshop-Windows-x64-Setup-<version>.exe` with a SHA-256 file.
Installer tests use a separate app identity and folders under `build/installer-tests`;
they do not install plugins into Photoshop or require closing your real Photoshop.

## macOS build

On a Mac, install Xcode, Rust, and Python 3, and extract the Mac Photoshop SDK:

```bash
bash scripts/build-macos.sh /path/to/photoshopsdk
```

This builds both `aarch64-apple-darwin` and `x86_64-apple-darwin`, combines them
with `lipo`, and creates `dist/macos/SeizaFITS.plugin` and `SeizaXISF.plugin` plus
a universal ZIP and an unsigned DMG with the installation shortcut. The modern `PiPLs.json` bundle resources follow the supplied
2026 SDK's format. macOS uses Photoshop's borrowed POSIX file descriptors.
Run this on macOS; downloading the Adobe Mac SDK alone does not provide Apple's
compiler, macOS system headers, or the ability to test the plug-in from Windows.

Local bundles are ad-hoc signed, which is enough for your own Mac. To sign,
notarize, and staple them for distribution:

```bash
APPLE_API_KEY_PATH=~/AuthKey_KEYID.p8 APPLE_API_KEY=KEYID APPLE_API_ISSUER=ISSUER \
  bash scripts/sign-macos.sh dist/macos "Developer ID Application: Name (TEAMID)"
```

The script checks both bundles, signs them with the hardened runtime and a
secure timestamp, submits one ZIP to `notarytool`, prints Apple's log if the
submission is rejected, and staples the tickets. Leave the three `APPLE_API_*`
variables unset to sign without notarizing. After saving your documents and
closing Photoshop, copy the bundles into its `Plug-ins` folder and restart. The
SDK and resource generators are never included in the bundles.

To package signed bundles in a signed, notarized DMG, move the earlier unsigned
DMG aside first, then run:

```bash
APPLE_API_KEY_PATH=~/AuthKey_KEYID.p8 APPLE_API_KEY=KEYID APPLE_API_ISSUER=ISSUER \
  bash scripts/build-macos-dmg.sh dist/macos "Developer ID Application: Name (TEAMID)"
```

The script creates `dist/Seiza-Photoshop-macOS-universal-<version>.dmg`, signs
and notarizes it, staples its ticket, verifies Gatekeeper acceptance, then
mounts it read-only to check the shortcut, both architectures, signatures, and
unchanged bundle contents. It writes the checksum after stapling. Verification
never installs into the real Photoshop folder. Omit the identity and API
variables for a local unsigned DMG around the existing bundles.

## Architecture and tests

`src/lib.rs` owns sample semantics; `src/ffi.rs` exposes a panic-contained C ABI.
The pinned Seiza crates provide readers and Float32 writers. `src/integer_writer.rs`
adds minimal UInt16 writers until upstream offers that sample type; tests verify
all 65536 integer codes, FITS scaling/endianness, XISF sample type, and round trips
through the Seiza readers. Import rescaling and Photoshop's 0..32768 representation
are handled by the native adapter.
`native/plugin.cpp` uses Adobe's `FormatRecord` directly and transfers planar rows
through `advanceState`. Two PiPL resources register separate formats so Save
chooses the requested encoder independently of the output filename.

`cargo test` covers FITS scaling, signed values, fixed integer normalization,
Float32 round trips, endian conversion, shuffled compression, planar channels,
malformed input, non-finite values, FFI ownership, and write failures. Metadata
tests cover raw FITS cards, XISF property scopes and binary attachments, header
growth and offset relocation, depth conversion, CFA/WCS cleanup, and XMP parsing.
`native/codec_smoke.cpp` links the actual release Rust library into a C++ executable
and verifies both formats across the language boundary. `native/host_smoke.cpp`
loads the compiled plug-ins through `PluginMain` and uses the real SDK structures.
The harness also exercises document XMP, Save As without shared format options,
and isolation between documents. Both platform build scripts run it before packaging.

## Manual Photoshop checks

Generate known test images:

```powershell
cargo run --locked --example make_fixtures
```

1. Configure import defaults in About Plug-In (or enable **Ask on every Open**).
   Open all four images in `build/fixtures`. Try both import depths and verify
   128×96 dimensions and grayscale/RGB channels. Mono is a left-to-right ramp. RGB has red increasing
   left-to-right, green increasing top-to-bottom, and blue only in the upper-left.
2. Save copies using each format and output depth. Float32 retains document
   samples; UInt16 rounds them. Reopen as Float32 to inspect saved values without
   applying another import rescaling. Test Revert retains the document depth.
3. Repeat with a real compressed XISF, unsigned 16-bit FITS, and a Float32 HDR file.
4. Cancel a large import/export; confirm Photoshop remains usable and the original
   source file is intact. Reopen a valid image after a malformed-file error.
5. Confirm unavailable modes/extra alpha channels cannot be silently exported.

## CI and release packaging

Use [GitHub Releases](https://github.com/theatrus/xisf-photoshop/releases/latest)
for published versions, including the optional Windows installer, Windows plugin
ZIP, signed universal macOS DMG and alternative ZIP, and SHA-256 files. Release
assets are the original validated CI packages; signed Mac packages are not repacked.

[GitHub Actions](https://github.com/theatrus/xisf-photoshop/actions/workflows/ci.yml)
tests the Rust backend on Windows and macOS for pushes and pull requests. Pushes
to `main`, `v*` tags, and manual runs also compile both native plugins using the
Adobe 2026 v2 SDK. Successful builds provide these artifacts (GitHub login required):

- `Seiza-Photoshop-Windows-x64`: ZIP containing both `.8bi` plugins and docs.
- `Seiza-Photoshop-Windows-x64-Installer`: optional setup `.exe` and SHA-256 file,
  with automatic shared-folder installation, upgrades, and uninstall support.
- `Seiza-Photoshop-macOS-universal`: drag-and-drop DMG and alternative ZIP with
  both `.plugin` bundles for Intel and Apple silicon, Developer ID signed,
  notarized, and stapled, plus docs and SHA-256 files. Preserve the original
  DMG or inner ZIP when copying to a Mac so permissions and signatures survive.

Actions artifacts are development snapshots and expire after 30 days; release
downloads remain available. Run the workflow manually to rebuild development snapshots.
Native builds run the C++/Rust ABI test and load both compiled plugins in a
minimal SDK host harness. The macOS runner tests its native architecture;
`lipo` checks that both architectures are in each bundle. These tests do not
replace interactive testing in Photoshop.
The Windows job also tests installer migration of manual copies, rollback on a
locked file, blocking installation/uninstall while Photoshop is running (using
a stand-in process), repeated installation, and uninstall preservation of
backups and unrelated files. macOS also mounts and verifies the DMG, its
destination shortcut, and both packaged plugin bundles without installing them.

### macOS signing

The macOS plugins job uploads its ad-hoc signed bundles as a one-day
`Seiza-Photoshop-macOS-universal-unsigned` artifact. A separate `sign-macos` job
in the `signing` GitHub environment downloads that ZIP, imports the Developer ID
certificate into a throwaway keychain, runs `scripts/sign-macos.sh`, then builds
and notarizes the DMG with `scripts/build-macos-dmg.sh`. Both packages are
uploaded with checksums. The environment is limited to `main` and `v*` tags, needs no
manual approval, and holds six environment secrets:

- `APPLE_BUILD_CERTIFICATE`: base64 Developer ID Application `.p12` with its key;
- `APPLE_BUILD_CERTIFICATE_PASSWORD`: the `.p12` password;
- `KEYCHAIN_PASSWORD`: a random password for the ephemeral CI keychain;
- `APPLE_API_ISSUER`: App Store Connect team API issuer ID;
- `APPLE_API_KEY`: App Store Connect API key ID; and
- `APPLE_API_KEY_PRIVATE`: base64 `AuthKey_<KEY_ID>.p8`.

The workflow selects the identity by the name `Developer ID Application`, so a
renewed certificate needs only new `APPLE_BUILD_CERTIFICATE*` secrets. The job
deletes the keychain, certificate, and API key even when a step fails. The SDK
and its passphrase never reach this job.

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
