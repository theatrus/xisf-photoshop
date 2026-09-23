"""Set or verify the Finder layout on a mounted DMG, without running Finder."""
import argparse
from pathlib import Path
import subprocess
import tempfile

from ds_store import DSStore
from mac_alias import Alias

POSITIONS = {
    "Photoshop Plug-ins": (186, 327),
    "SeizaFITS.plugin": (494, 327),
    "SeizaXISF.plugin": (742, 327),
    "Read me first.txt": (700, 592),
    "Documentation": (826, 592),
}
# Leave room for Finder's title bar above the 920 x 660 artwork.
BOUNDS = "{{100, 100}, {920, 704}}"


def configure(volume):
    # mac_alias gets the mount point from macOS, which resolves /var to
    # /private/var. Use the same path so its relative path stays on the DMG.
    background = (volume / ".background" / "instructions.png").resolve(strict=True)
    with DSStore.open(str(volume / ".DS_Store"), "w+") as store:
        store["."]["vSrn"] = ("long", 1)
        store["."]["icvl"] = ("type", b"icnv")
        store["."]["bwsp"] = {
            "WindowBounds": BOUNDS,
            "ShowToolbar": False,
            "ShowStatusBar": False,
            "ShowPathbar": False,
            "ShowSidebar": False,
            "ShowTabView": False,
            "ContainerShowSidebar": False,
        }
        store["."]["icvp"] = {
            "viewOptionsVersion": 1,
            "backgroundType": 2,
            # Finder rejects incomplete icon-view settings, even for a picture.
            "backgroundColorRed": 1.0,
            "backgroundColorGreen": 1.0,
            "backgroundColorBlue": 1.0,
            "backgroundImageAlias": Alias.for_file(str(background)).to_bytes(),
            "iconSize": 72.0,
            "textSize": 14.0,
            "labelOnBottom": True,
            "arrangeBy": "none",
            "showItemInfo": False,
            "showIconPreview": False,
            "gridOffsetX": 0.0,
            "gridOffsetY": 0.0,
            "gridSpacing": 80.0,
            "scrollPositionX": 0.0,
            "scrollPositionY": 0.0,
        }
        for name, position in POSITIONS.items():
            store[name]["Iloc"] = position


def verify(volume):
    background = (volume / ".background" / "instructions.png").resolve(strict=True)
    with DSStore.open(str(volume / ".DS_Store"), "r") as store:
        assert store["."]["bwsp"]["WindowBounds"] == BOUNDS
        assert store["."]["icvl"] == (b"type", b"icnv")
        view = store["."]["icvp"]
        assert view["backgroundType"] == 2 and view["arrangeBy"] == "none"
        assert 0.0 < view["gridSpacing"] < 100.0
        for channel in ("Red", "Green", "Blue"):
            assert view["backgroundColor" + channel] == 1.0
        alias = Alias.from_bytes(view["backgroundImageAlias"])
        assert alias.target.filename == "instructions.png"
        # The alias must refer to this disk image, not the build machine's disk.
        assert alias.volume.name.startswith("FITS and XISF ")
        assert alias.target.posix_path == "/.background/instructions.png", alias.target.posix_path
        assert alias.target.carbon_path == (
            alias.volume.name + ":.background:\0instructions.png"
        ).encode("utf-8")
        assert alias.target.cnid == background.stat().st_ino
        assert alias.target.folder_cnid == background.parent.stat().st_ino
        assert alias.target.cnid_path == (background.parent.stat().st_ino,)
        # Parsing our own metadata is not enough: ask macOS to resolve it.
        # The final DMG test runs this again at a different mount point.
        with tempfile.TemporaryDirectory() as work:
            record = Path(work) / "background.alias"
            record.write_bytes(view["backgroundImageAlias"])
            subprocess.run([
                "xcrun", "swift", str(Path(__file__).with_name("verify-dmg-alias.swift")),
                str(record), str(background),
            ], check=True)
        for name, position in POSITIONS.items():
            assert store[name]["Iloc"] == position, name
    assert (volume / ".background/instructions.png").read_bytes() == Path(__file__).with_name("dmg-background.png").read_bytes()
    notes = (volume / "Read me first.txt").read_text()
    assert "Double-click" in notes and "that Finder window" in notes
    print("Finder metadata verified: resolvable background, window size, and all icon positions.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("volume", type=Path)
    parser.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    if not args.verify:
        configure(args.volume)
    verify(args.volume)
