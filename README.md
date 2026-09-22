# Seiza astronomy formats for Photoshop

[![Build and test plugins](https://github.com/theatrus/xisf-photoshop/actions/workflows/ci.yml/badge.svg)](https://github.com/theatrus/xisf-photoshop/actions/workflows/ci.yml)

Native Photoshop file-format plug-ins backed by `seiza-fits 0.2.2` and
`seiza-xisf 0.2.1`. Photoshop lists the formats as **FITS** and **XISF**. Plugin files retain the names
**SeizaFITS** and **SeizaXISF** (`.8bi` on
Windows, `.plugin` on macOS), with entries in Photoshop's Open and Save dialogs.

## Download and install

Get the plugins from [GitHub Releases](https://github.com/theatrus/xisf-photoshop/releases/latest).
Release downloads are public, require no GitHub login, and do not expire like CI artifacts.
Both FITS and XISF are included in every package.

| Platform | v0.5.1 download | Installation |
| --- | --- | --- |
| Windows x64 | [Installer (.exe)](https://github.com/theatrus/xisf-photoshop/releases/download/v0.5.1/Seiza-Photoshop-Windows-x64-Setup-0.5.1.exe) | Close Photoshop, run setup, approve the administrator prompt, then restart Photoshop. |
| Windows x64 | [Portable plugin ZIP](https://github.com/theatrus/xisf-photoshop/releases/download/v0.5.1/Seiza-Photoshop-Windows-x64.zip) | Close Photoshop, extract both `.8bi` files into its `Plug-ins/Seiza` folder, then restart. |
| macOS Intel / Apple silicon | [Drag-and-drop DMG](https://github.com/theatrus/xisf-photoshop/releases/download/v0.5.1/Seiza-Photoshop-macOS-universal-0.5.1.dmg) | Close Photoshop, open the DMG, drag both `.plugin` bundles onto **Photoshop Plug-ins**, approve any administrator prompt, eject, and restart Photoshop. |
| macOS Intel / Apple silicon | [Alternative plugin ZIP](https://github.com/theatrus/xisf-photoshop/releases/download/v0.5.1/Seiza-Photoshop-macOS-universal.zip) | Extract on your Mac and copy both `.plugin` bundles from `macos` into the shared folder described below or your Photoshop application's `Plug-ins` folder. |

The Windows installer handles updates and removal of detected older manual copies;
see [Windows installation details](#optional-windows-installer). Releases after
v0.4.0 are Authenticode-signed as StackFoundry LLC, both the installer and the
`.8bi` plugins inside it; v0.4.0 and earlier are unsigned, so Windows may show
an unknown-publisher or SmartScreen prompt for those.
The macOS DMG and bundles are Developer ID signed, notarized, and stapled. The
DMG shortcut targets Adobe's shared Plug-ins folder; see [macOS installation](#macos-installation-dmg).
If using the alternative ZIP, extract it on macOS to preserve bundle permissions and signatures.
Each release download has a matching `.sha256` file on the release page.

Configure defaults through **Help → About Plug-In → FITS/XISF** on Windows or
**Photoshop → About Plug-In → FITS/XISF** on macOS. See [plugin settings](#plugin-settings-help-menu).

**Development status:** Initial releases for Windows x64 and universal macOS
(Intel and Apple silicon), compiled against Adobe's 2026 SDK v2. CI gates plugin
downloads on Rust codec and metadata tests, a compiled C++/Rust ABI test, and a host harness
that loads both plugins and exercises Adobe's `FormatRecord` interface.
Live Photoshop validation remains pending. CI macOS bundles are Developer ID
signed, notarized, and stapled; local builds are ad-hoc signed. Plaintext SDKs
are not redistributed.

## Supported image workflow

- Open `.fits`, `.fit`, `.fts`, and `.xisf` into grayscale or RGB using the saved
  **32-bit float** or **16-bit integer** import choice. By default, each Open asks
  which depth to use, with **Remember choice** unchecked. Save follows the
  document depth without prompting.
- 32-bit import retains decoded floating-point values, including negatives and
  values above 1, without rescaling or clipping.
- 16-bit import linearly rescales the global image minimum/maximum to Photoshop's
  internal **0..32768** range and rounds to integers. All RGB channels share one
  range; there is no per-channel stretching or clipping. Constant images become
  zero. Settings explain the loss of original absolute scale and precision;
  optional per-file dialogs also show the source range. No gamma conversion is applied. When enabled, debayering runs in Float32
  before measuring the shared RGB range and converting to 16-bit.
- Save **16- or 32-bit** grayscale/RGB documents at their current Photoshop depth
  by default: 16-bit becomes UInt16, and 32-bit becomes Float32. Settings also
  offer explicit Float32 or UInt16 overrides. Eight-bit documents must first
  be converted via **Image → Mode → 16 Bits/Channel** or **32 Bits/Channel**.
- Float32 output preserves the current document's sample values; it does
  not restore the original scale or precision after a 16-bit import. UInt16
  output rounds 0..1 to 0..65535. Exporting a 32-bit document as UInt16 clips
  negative/HDR values, with this loss stated in settings and the optional save dialog. Import rescaling
  already puts 16-bit documents into the valid range.
- Unsigned integer camera data uses a fixed full-scale divisor: 255 for 8-bit,
  65535 for 16-bit, and 4294967295 for XISF UInt32. Signed FITS integers and
  nonstandard FITS scaling retain physical units. FITS `BZERO`/`BSCALE` are applied
  once. Float32 output uses normalized pixels for unsigned camera data, not the
  original ADU scale; a 16-bit import additionally rescales to the image range.
- XISF attached mono/RGB data supports the sample types, byte order, checksums,
  zlib, LZ4/LZ4HC, zstd, and byte shuffling provided by `seiza-xisf`.
- Float64/Int32/UInt32 inputs can lose precision when converted to Photoshop f32.
  Images containing NaN or infinite samples are rejected with an error.

Linear astro images may appear very dark. Apply your desired stretch in
Photoshop; 32-bit import does not alter it for display, while 16-bit import applies
only the linear min/max rescaling described above. The plug-ins do not embed
an ICC profile into Photoshop, so color appearance depends on Photoshop's color settings.
Existing XISF profile data is retained as source metadata on same-format saves;
it is not synchronized with Photoshop profile conversions.

## Plugin settings (Help menu)

Open **Help → About Plug-In → FITS** (or **XISF**) on Windows.
On macOS, use **Photoshop → About Plug-In → FITS/XISF**.
The plugin's About entry opens **FITS / XISF - Default settings**; it works without an
open document. Both formats share the same preferences:

- **Default import depth:** Float32 or rescaled 16-bit integer.
- **Debayer tagged images to RGB (bilinear):** automatically converts recognized
  Bayer mosaics; untagged monochrome and existing RGB files stay unchanged.
- **Default saved sample type:** Match document depth (default), Float32, or UInt16.
- **Ask on every Open:** enabled by default. Re-enable it here to show the import
  dialog again after remembering a choice.
- **Ask on every Save / Save As:** disabled by default; enable for per-file export
  overrides. Revert and previews never prompt.

New installations ask on every Open, with Float32 initially selected and
**Remember choice** unchecked. Leaving it unchecked applies the choice only to
that document and asks again next time. Checking it and confirming saves the
selected import depth for **both FITS and XISF**, turns off **Ask on every Open**,
and skips future import prompts. Cancel never remembers a choice. Existing saved
preferences are preserved when upgrading.

Save defaults to **Match document depth**, without prompting. Matching is resolved
on every save, including after changing the Photoshop document mode; it does not
remember an old numeric depth.
For a 16-bit editing workflow, select 16-bit import, leave the saved sample type at Match document depth,
leave both checkboxes off, and click **Save defaults**. Conversion warnings appear
in settings before you apply a lossy default. Cancel changes nothing.

Settings take effect for new imports and documents without remembered options.
Revert retains a document's import choice, and Save retains its match-document policy or explicit sample-type override.
To override an existing document's saved type, enable **Ask on every Save / Save As**
and use Save As. Import choices change global defaults only when **Remember choice**
is checked; one-off save choices do not change them.
Settings survive restarts and plugin updates; changes from either plugin are
visible to the other without restarting Photoshop.

Preferences are stored per user at `%APPDATA%\Seiza\Photoshop\defaults-v1.txt`
on Windows and `~/Library/Application Support/Seiza/Photoshop/defaults-v1.txt`
on macOS. Missing or malformed preferences fall back to Float32 import and matching export,
with the Open prompt on and Save prompt off. Settings and document options from
0.3.0 and earlier migrate
the save type to Match document depth while retaining the import choice. The
preferences filename stays unchanged; its contents use the version 3 schema.
Older preferences and document options retain raw grayscale imports until you
choose to enable debayering.
Tests use an isolated path via `SEIZA_PHOTOSHOP_PREFERENCES` and never modify
your actual settings.

## Bayer / CFA import

For single-channel files, the Open dialog adds a **Color filter array** selector:

- **Keep raw grayscale:** preserves the sensor mosaic without interpolation.
- **Debayer to RGB - Auto (metadata):** uses recognized FITS `BAYERPAT` or XISF
  `ColorFilterArray` / FITS-compatible metadata. Auto is the fresh-install default.
  Missing or unsupported patterns stay grayscale; no pattern is inferred from pixels.
- **RGGB / BGGR / GRBG / GBRG:** explicit manual overrides for missing or incorrect
  pattern metadata. The dialog displays the detected pattern and origin offsets.

Bayer patterns describe the stored pixel order. `XBAYROFF` and `YBAYROFF` are
honored modulo two, including negative offsets; the plugin does not guess a row
flip from camera conventions. Invalid offsets and images narrower or shorter
than two pixels must be opened raw or corrected before conversion.

Conversion uses `seiza-fits` bilinear interpolation in linear Float32, preserving
measured samples and negative/HDR values. It creates full-size planar RGB before
any 16-bit min/max rescaling. Existing RGB images hide these controls and are
never debayered again. Previews use the configured conversion without prompting;
Revert retains the document's conversion and manual pattern.

**Remember choice** saves raw versus automatic debayering along with import depth.
A manual pattern is retained for that document's Revert, but remembering it globally
selects **Auto** for future files so unrelated monochrome data is not forced through
the same Bayer pattern. Use **Help -> About Plug-In -> FITS/XISF** on Windows (the
**Photoshop -> About Plug-In** menu on macOS) to change the shared default later.

Saved debayered images contain RGB samples, not a Bayer mosaic, and omit active
CFA tags. Raw grayscale saves retain source CFA tags while the dimensions stay
unchanged. Keep the original scientific file if you need to debayer it again later.

## Metadata retention

Starting with 0.5.0, metadata is captured on Open and attached to the Photoshop
document. Ordinary Save and Save As retain it by default, including when the
save options dialog is skipped. Metadata is stored in a private XMP namespace,
so a PSD/PSB intermediate can carry it with the document; workflows that strip
XMP also strip this payload. Files opened or saved by earlier plugin versions
cannot recover metadata that those versions discarded. Reopen the original
FITS/XISF with the new plugin to capture its metadata.

- **FITS → FITS:** preserves the primary image's header cards, their order,
  comments, duplicate HISTORY/COMMENT records, HIERARCH cards, and long-string
  CONTINUE cards. Acquisition fields such as target, exposure, filter, camera,
  gain, and observation time remain available to other FITS readers.
- **XISF → XISF:** preserves the first image's metadata and file-level metadata,
  including FITSKeyword records, typed properties, Unicode strings, inline and
  embedded values, and attached binary blocks. Attached metadata retains its
  compression and checksums; offsets are relocated in the new file. Source ICC,
  resolution, working-space, and display-function metadata is retained.
- **Switching formats:** copies compatible FITS keywords on a best-effort basis.
  XISF-only properties and binary blocks have no generic FITS mapping and are
  omitted from FITS output. No private FITS extensions are added. Saving a copy
  in another format does not remove the source metadata from the open Photoshop
  document, so saving that document back to its original format still retains it.

Image storage fields are regenerated for the saved dimensions, channels, and
sample type. Obsolete pixel checksums, extrema, and thumbnails are removed.
Bayer tags are removed after debayering, for RGB output, or after a dimension
change. Known WCS keywords and PixInsight astrometric-solution properties are
removed when the dimensions change; otherwise they are retained unchanged.
The plugin cannot track every Photoshop transform: rotations, flips, or edits
that leave the dimensions unchanged can invalidate a retained solution. Re-solve
after geometric edits. Retained acquisition metadata describes the source;
16-bit rescaling still loses the original absolute pixel scale and precision.

The scope remains one image: additional FITS HDUs and XISF images are not loaded
as layers or copied on save. Layers, alpha, and masks are flattened/unsupported
as described below; use PSD/PSB for the Photoshop editing document.
Metadata is limited to 64 MiB of serialized payload, with a 16 MiB XISF XML-header
limit. Missing attachment data, external XISF metadata blocks, or malformed
metadata cause an explicit error rather than a successful save that silently
drops fields. Use a monolithic XISF source for metadata with binary attachments.

## Current limits

- CI builds Windows x64 and universal macOS. Automated host tests run on Windows
  x64 and the Mac runner's native architecture; interactive Photoshop checks are
  still required on each supported architecture.
- FITS primary HDU only; extra HDUs are not opened. XISF opens its first image.
  FITS cubes other than one/three planes are rejected.
- Debayering supports the four 2-by-2 Bayer patterns using bilinear interpolation.
  Other CFA layouts (such as X-Trans) are not automatically converted. No white
  balance, camera color matrix, gamma correction, or advanced demosaic method is applied.
- Save stores one flattened image. Layers, alpha channels, and masks are not
  round-tripped. Source metadata follows the retention rules above.
  Keep an original scientific image and use PSD/PSB for layered Photoshop work.
- XISF output is uncompressed Float32 or UInt16. FITS output uses `BITPIX=-32`
  for Float32, or `BITPIX=16`, `BZERO=32768`, `BSCALE=1` for unsigned integer data.
- Revert reuses the import choice. Ordinary Save reuses the output choice when
  Photoshop skips its options dialog; enable the save prompt and use Save As to select another type.
  Dialog-suppressed automation uses remembered document options, otherwise
  uses saved defaults. Custom choices are not recorded in Actions descriptors yet.
  Preview reads are always Float32 and never display a dialog.
- The first implementation retains the encoded input and decoded image in memory
  while opening; saving retains a planar f32 image and an encoded output buffer.
  Document XMP also holds a serialized copy of the source metadata. It is not an out-of-core codec.
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

## Install and verify in Photoshop

### Optional Windows installer

Download `Seiza-Photoshop-Windows-x64-Setup-<version>.exe` from the
[latest release](https://github.com/theatrus/xisf-photoshop/releases/latest),
close Photoshop, and run it.
Approve the Windows administrator prompt, then restart Photoshop after setup.
Release installers and their plugins from 0.5.0 onward are Authenticode-signed
as StackFoundry LLC. Local builds and releases through 0.4.0 are unsigned.
Windows 10/11 x64 is supported;
native Windows ARM64 Photoshop is not supported.

Both plugins go in
`C:\Program Files\Common Files\Adobe\Plug-Ins\CC\Seiza`, Adobe's
[shared Photoshop plugin location](https://helpx.adobe.com/ca/photoshop/kb/plug-ins-photoshop-troubleshooting.html),
so they are available to installed Photoshop CC versions and remain available
after Photoshop upgrades. The installer can run before Photoshop is installed.
Run a newer installer to update both plugins. Setup and uninstall require
Photoshop to be closed and never close it or discard documents automatically.

Setup finds older `SeizaFITS.8bi` and `SeizaXISF.8bi` copies directly in standard
or registry-listed Photoshop plugin folders, their `Seiza` subfolders, and the
shared plugin folder. It lists these copies before installation, backs them up to
`C:\ProgramData\Seiza\Photoshop\InstallerBackups`, records their original paths,
and removes the active old copies to avoid duplicate format entries. If you used
a different custom plugin folder, remove those old copies yourself before restarting
Photoshop. Failed setup restores removed copies where possible; backups remain
available if a file cannot be restored.

To uninstall, close Photoshop and remove **FITS and XISF for Photoshop** from
**Windows Settings → Apps**. Preferences, legacy backups, and unrelated plugins
are preserved. Old copies are not automatically restored on uninstall.
Configure the plugins through **Help → About Plug-In → FITS/XISF** as described above.

### Manual Windows installation (ZIP)

The plugin ZIP is also available from [Releases](https://github.com/theatrus/xisf-photoshop/releases/latest).
After extracting it or completing a native
build, close Photoshop and copy both `.8bi` files into its `Plug-ins/Seiza` folder.
From a source checkout, you can also use:

```powershell
.\scripts\install.ps1 -PluginDirectory 'C:\Program Files\Adobe\Adobe Photoshop 2026\Plug-ins'
```

Writing to Program Files requires an elevated PowerShell on this machine. Run
the installation command there, or copy the two `.8bi` files from `dist` into
Photoshop's `Plug-ins/Seiza` folder and approve Windows' administrator prompt.
This manual copy script does not request elevation or change folder permissions itself.
Existing same-name plug-ins are backed up with `.bak`. Restart Photoshop after
installation. To uninstall, close Photoshop and remove the two `.8bi` files.

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

These in-Photoshop checks remain pending. The separate SDK host harness has
already verified both real DLLs with mono/RGB/HDR pixels, a 32768-pixel-wide image,
signature filtering, file-position restoration, cancellation/cleanup, unsupported
save depths, and malformed files. It does not emulate Photoshop's UI or color engine.

### macOS installation (DMG)

Download `Seiza-Photoshop-macOS-universal-<version>.dmg`, close Photoshop, and
open the disk image. Drag **both SeizaFITS.plugin and SeizaXISF.plugin** onto
the **Photoshop Plug-ins** shortcut. Approve Finder's administrator prompt if
asked, eject the disk image, and restart Photoshop.

The shortcut points to `/Library/Application Support/Adobe/Plug-Ins/CC`, Adobe's
[shared Photoshop plugin location](https://helpx.adobe.com/photoshop/kb/plug-ins-photoshop-troubleshooting.html),
which is used by installed Photoshop CC versions and survives app upgrades.
If that folder does not exist, create it first or copy the bundles into your
Photoshop application's `Plug-ins` folder instead. The DMG's **Read me first.txt**
includes these instructions. Opening the DMG does not change your system.

When updating, replace the same-name bundles. If you previously installed from
the ZIP into a Photoshop application or custom folder, remove those older
SeizaFITS/SeizaXISF copies before using the shared folder to avoid duplicate
format entries. To uninstall, close Photoshop and remove just these two bundles
from their installation folder; preferences and images are left untouched.

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

## CI and downloads

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
