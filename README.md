# FITS and XISF for Photoshop

Open and save astronomy images directly in Photoshop on Windows and macOS.
Import as 32-bit float or rescaled 16-bit integer, optionally debayer to RGB,
and retain source metadata when saving in the same format.

## Download and install

Both FITS and XISF plugins are included in every download.

| Platform | Download v0.5.1 | Installation |
| --- | --- | --- |
| Windows 10/11 x64 | [Installer (.exe)](https://github.com/theatrus/xisf-photoshop/releases/download/v0.5.1/Seiza-Photoshop-Windows-x64-Setup-0.5.1.exe) | [Windows instructions](#optional-windows-installer) |
| macOS â€” Apple silicon and Intel | [Drag-and-drop DMG](https://github.com/theatrus/xisf-photoshop/releases/download/v0.5.1/Seiza-Photoshop-macOS-universal-0.5.1.dmg) | [macOS instructions](#macos-installation-dmg) |

For manual installation, [Windows ZIP](https://github.com/theatrus/xisf-photoshop/releases/download/v0.5.1/Seiza-Photoshop-Windows-x64.zip)
and [macOS ZIP](https://github.com/theatrus/xisf-photoshop/releases/download/v0.5.1/Seiza-Photoshop-macOS-universal.zip)
downloads are also available. See [all releases](https://github.com/theatrus/xisf-photoshop/releases).

Windows downloads are signed by **StackFoundry LLC**. The macOS DMG and plugins
are Developer ID signed and notarized. SHA-256 checksums accompany each download.

### Optional Windows installer

Close Photoshop, run the installer, approve the administrator prompt, and restart
Photoshop. Both plugins install into Adobe's shared folder:

`C:\Program Files\Common Files\Adobe\Plug-Ins\CC\Seiza`

This keeps them available across Photoshop upgrades. Run a newer installer to
update. Setup backs up and removes detected older manual copies; remove any
copies in custom plugin folders yourself to avoid duplicate format entries.
Backups are kept in `C:\ProgramData\Seiza\Photoshop\InstallerBackups`.

To uninstall, close Photoshop and remove **FITS and XISF for Photoshop** through
**Windows Settings â†’ Apps**. Your preferences and images are preserved.
Native Windows ARM64 Photoshop is not supported.

### Manual Windows installation (ZIP)

Close Photoshop and extract both `.8bi` files into its `Plug-ins/Seiza` folder.
Approve the administrator prompt if needed, then restart Photoshop. Replace old
copies when updating; remove the two files to uninstall.

### macOS installation (DMG)

1. Close Photoshop and open the DMG.
2. Drag **both SeizaFITS.plugin and SeizaXISF.plugin** onto **Photoshop Plug-ins**.
3. Approve Finder's administrator prompt if asked, eject the DMG, and restart Photoshop.

The shortcut points to Adobe's shared folder at
`/Library/Application Support/Adobe/Plug-Ins/CC`, which survives Photoshop upgrades.
If that folder is missing, create it or copy both bundles into your Photoshop
application's `Plug-ins` folder instead.

When updating, replace the same-name bundles. Remove older copies from other
Photoshop or custom plugin folders to avoid duplicates. To uninstall, close
Photoshop and remove these two bundles from their installation folder.

For the alternative ZIP, extract it on your Mac and copy both `.plugin` bundles
from the `macos` folder into either location above.

## Supported image workflow

Open `.fits`, `.fit`, `.fts`, or `.xisf` as grayscale or RGB. The import dialog
asks which depth to use:

| Import depth | Behavior |
| --- | --- |
| **32-bit float** | Retains decoded floating-point values, including negative and HDR values, without rescaling or clipping. |
| **16-bit integer** | Rescales the full image range without clipping, using the same range for all RGB channels. This loses absolute pixel scale and precision; Photoshop's 16-bit mode has 32,769 levels. |

Linear astronomy images may look dark until you stretch them in Photoshop.
Neither import option applies a display stretch or gamma correction. Unsigned
integer input is normalized to a fixed full-scale range; saving does not restore
an original camera ADU scale that was normalized or rescaled on import.

**Save and Save As match the document's depth without prompting:** 16-bit
documents save as UInt16, and 32-bit documents as Float32. Convert 8-bit documents
through **Image â†’ Mode â†’ 16 Bits/Channel** or **32 Bits/Channel** before saving.

You can override the saved depth in settings. Saving a 32-bit document as UInt16
rounds samples and clips values outside 0â€“1; the settings and save dialogs warn
about this loss. Saving as Float32 later cannot restore precision or scale lost
during a 16-bit import.

## Plugin settings (Help menu)

Open **Help â†’ About Plug-In â†’ FITS** or **XISF** on Windows, or
**Photoshop â†’ About Plug-In â†’ FITS** or **XISF** on macOS. Both entries open the
same settings, even without an image open.

| Setting | Default |
| --- | --- |
| Import depth | 32-bit float |
| Debayer tagged images to RGB | Enabled; uses recognized Bayer metadata |
| Saved sample type | Match document depth |
| Ask on every Open | Enabled |
| Ask on every Save / Save As | Disabled |

The Open dialog starts with **Remember choice** unchecked. Check it to save
your import choice for both formats and stop future Open prompts. Re-enable
**Ask on every Open** in settings to show them again. Cancel never remembers a choice.

For a 16-bit workflow, choose 16-bit import and leave the saved sample type at
**Match document depth**. This follows later changes to the document's mode too.

Settings persist across restarts and updates. Existing documents retain their
import and save choices; to override a document's saved type, enable
**Ask on every Save / Save As** and use Save As. One-off save choices do not
change global defaults. Revert reuses the document's import choice.

## Bayer / CFA import

For single-channel images, the Open dialog includes a **Color filter array** selector:

- **Keep raw grayscale:** retain the sensor mosaic without interpolation.
- **Debayer to RGB â€” Auto (metadata):** use the file's Bayer pattern. Missing or
  unsupported patterns stay grayscale; existing RGB images are unchanged.
- **RGGB / BGGR / GRBG / GBRG:** choose a pattern manually when metadata is missing or incorrect.

Debayering uses bilinear interpolation before any 16-bit rescaling. It does not
apply white balance or a camera color correction. Other mosaics, such as X-Trans,
are not supported.

**Remember choice** stores raw versus automatic debayering along with import
depth. A manual pattern stays with that document; remembering it globally selects
Auto so the same pattern is not forced onto unrelated files.

Debayered saves contain RGB pixels and omit active Bayer tags. Raw grayscale
saves retain those tags when dimensions are unchanged. Keep the original if you
want to debayer again later.

## Metadata retention

Source metadata follows the Photoshop document through Save and Save As:

- **FITS â†’ FITS:** retains primary-image header cards, including acquisition
  fields, comments, history, and long-string records.
- **XISF â†’ XISF:** retains the first image's metadata and file-level metadata,
  including FITS keywords, typed properties, and attached binary data.
- **Switching formats:** transfers compatible FITS keywords on a best-effort
  basis. XISF-only properties are omitted from FITS output. Saving a copy does
  not remove source metadata from the open document.

Storage fields are updated for the saved image, and obsolete thumbnails are
removed. Known WCS fields are removed when dimensions change. **Re-solve after
geometric edits:** rotations and flips can invalidate retained coordinates even
when dimensions stay the same. Acquisition metadata describes the source image;
it does not undo changes to pixel values.

Metadata is carried in the document's XMP, so a PSD/PSB intermediate can carry it
too. Workflows that strip XMP remove this metadata. If an older plugin discarded
metadata, reopen the original FITS/XISF with version 0.5.0 or later to capture it.

## Color profiles

XISF's embedded ICC profile is passed to Photoshop on Open, for both 16-bit and
32-bit imports. Photoshop's color-management settings control whether to retain
it, convert it, or ask about a profile mismatch.

XISF saves embed the document's current profile, including changes made with
**Assign Profile** or **Convert to Profile**. Turning off **ICC Profile**
in Photoshop's save dialog, or saving an untagged document, omits it. The source
profile stored with other metadata is not restored over that choice.

Only profiles matching the imported color mode are assigned: a grayscale profile
is not applied to debayered RGB. Untagged images use Photoshop's normal missing-profile
behavior. FITS does not carry an ICC profile through these plugins.

## Format limits

- Only the first image is opened and saved. Additional FITS HDUs and XISF images
  are not loaded as layers or copied on save.
- Output is one flattened grayscale or RGB image. Layers, alpha channels, and
  masks are not retained; use PSD/PSB for your Photoshop editing document.
- XISF output is uncompressed. Compressed XISF input supports zlib, LZ4/LZ4HC,
  zstd, and byte shuffling.
- ICC profiles are limited to 16 MiB. Invalid profile headers, tag offsets, or
  checksums produce an error.
- Higher-precision input can lose precision when converted to 32-bit float.
  Images containing NaN or infinite samples are rejected.
- Metadata is limited to 64 MiB, with a 16 MiB XISF XML-header limit. External
  XISF metadata blocks are unsupported; use a monolithic XISF file. Missing or
  malformed metadata produces an error.
- Photoshop Actions use remembered document choices or saved defaults; custom
  import/export choices are not recorded in Actions.

## Source and support

Report problems through [GitHub Issues](https://github.com/theatrus/xisf-photoshop/issues).
The plugins use the [Seiza](https://github.com/theatrus/seiza) FITS and XISF codecs.
For build, testing, and release instructions, see the [development guide](https://github.com/theatrus/xisf-photoshop/blob/main/DEVELOPMENT.md).
See [LICENSE](LICENSE) and [third-party notices](NOTICE).
