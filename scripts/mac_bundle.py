"""Generate modern Adobe JSON PiPL and macOS bundle metadata (no SDK copied)."""
import argparse
import json
import plistlib
from pathlib import Path


def make_bundle(destination: Path, format_name: str) -> Path:
    name = "Seiza" + format_name
    bundle = destination / (name + ".plugin")
    contents = bundle / "Contents"
    (contents / "MacOS").mkdir(parents=True, exist_ok=True)
    (contents / "Resources").mkdir(exist_ok=True)
    metadata = {
        "CFBundleIdentifier": "org.seiza.photoshop." + format_name.lower(),
        "CFBundleExecutable": name,
        "CFBundleName": format_name,
        "CFBundlePackageType": "8BIF",
        "CFBundleSignature": "8BIM",
        "CFBundleVersion": "0.5.5",
        "CFBundleShortVersionString": "0.5.5",
        "LSMinimumSystemVersion": "11.0",
    }
    with (contents / "Info.plist").open("wb") as file:
        plistlib.dump(metadata, file)
    extensions = ["fits", "fit ", "fts "] if format_name == "FITS" else ["xisf"]
    pipl = {
        "Kind": "Format", "Name": format_name, "Version": 1, "SubVersion": 0,
        "ComponentVersionShortNum": 0, "ComponentVersionMinorRevNum": 5,
        "ComponentVersionDotRevNum": 5, "ComponentName": name,
        "CodeMacARM64": "PluginMain", "CodeMacIntel64": "PluginMain",
        "SupportsPOSIXIO": True,
        "SupportedModes": {mode: mode in ("GrayScale", "RGBColor") for mode in (
            "Bitmap", "GrayScale", "IndexedColor", "RGBColor", "CMYKColor", "HSLColor",
            "HSBColor", "Multichannel", "Duotone", "LABColor")},
        "EnableInfo": "in (PSHOP_ImageMode, Gray16Mode, RGB48Mode, Gray32Mode, RGB96Mode)",
        "PlugInMaxSize": [300000, 300000], "FormatMaxSize": [32767, 32767],
        "FormatMaxChannels": [0, 1, 0, 3, 0, 0, 0, 0, 0, 0, 1, 3, 0, 0, 0, 0, 3, 1],
        "FmtFileType": {"Type": format_name, "Creator": "8BIM"},
        "ReadExtensions": extensions, "WriteExtensions": extensions,
        "FilteredExtensions": extensions,
        "FormatFlags": {"SavesImageResources": False, "CanRead": True, "CanWrite": True,
            "CanWriteIfRead": True, "CanWriteTransparency": False, "CanCreateThumbnail": False},
        "FormatICCFlags": {"CanEmbedGray": format_name == "XISF", "CanEmbedIndexed": False,
            "CanEmbedRGB": format_name == "XISF", "CanEmbedCMYK": False},
    }
    (contents / "Resources" / "PiPLs.json").write_text(json.dumps({"PiPLs": [pipl]}, indent=2) + "\n", encoding="utf-8")
    return bundle


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    for format_name in ("FITS", "XISF"):
        print(make_bundle(args.destination, format_name))
