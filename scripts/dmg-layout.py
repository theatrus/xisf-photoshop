"""Set or verify the Finder layout on a mounted DMG, without running Finder."""
import argparse
from pathlib import Path

from ds_store import DSStore
from mac_alias import Alias

POSITIONS = {
    "Photoshop Plug-ins": (186, 327),
    "SeizaFITS.plugin": (494, 327),
    "SeizaXISF.plugin": (742, 327),
    "Read me first.txt": (700, 592),
    "Documentation": (826, 592),
}
BOUNDS = "{{100, 100}, {920, 660}}"


def configure(volume):
    background = volume / ".background" / "instructions.png"
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
            "backgroundImageAlias": Alias.for_file(str(background)).to_bytes(),
            "iconSize": 72.0,
            "textSize": 14.0,
            "labelOnBottom": True,
            "arrangeBy": "none",
            "showItemInfo": False,
            "showIconPreview": False,
            "gridOffsetX": 0.0,
            "gridOffsetY": 0.0,
            "gridSpacing": 100.0,
            "scrollPositionX": 0.0,
            "scrollPositionY": 0.0,
        }
        for name, position in POSITIONS.items():
            store[name]["Iloc"] = position


def verify(volume):
    with DSStore.open(str(volume / ".DS_Store"), "r") as store:
        assert store["."]["bwsp"]["WindowBounds"] == BOUNDS
        assert store["."]["icvl"] == (b"type", b"icnv")
        view = store["."]["icvp"]
        assert view["backgroundType"] == 2 and view["arrangeBy"] == "none"
        alias = Alias.from_bytes(view["backgroundImageAlias"])
        assert alias.target.filename == "instructions.png"
        # The alias must refer to this disk image, not the build machine's disk.
        assert alias.volume.name.startswith("FITS and XISF ")
        for name, position in POSITIONS.items():
            assert store[name]["Iloc"] == position, name
    assert (volume / ".background/instructions.png").read_bytes() == Path(__file__).with_name("dmg-background.png").read_bytes()
    notes = (volume / "Read me first.txt").read_text()
    assert "Double-click" in notes and "that Finder window" in notes
    print("Finder layout verified: instruction background, window size, and all icon positions.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("volume", type=Path)
    parser.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    if not args.verify:
        configure(args.volume)
    verify(args.volume)
