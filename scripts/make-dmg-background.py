"""Regenerate the committed Finder background (Pillow; supply two font paths)."""
import argparse
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("regular_font", type=Path)
parser.add_argument("bold_font", type=Path)
args = parser.parse_args()

SCALE = 2
image = Image.new("RGB", (920 * SCALE, 660 * SCALE), "#f3f5fa")
draw = ImageDraw.Draw(image)


def box(bounds, fill, radius=0, outline=None):
    draw.rounded_rectangle(tuple(v * SCALE for v in bounds), radius * SCALE,
                           fill=fill, outline=outline, width=SCALE)


def text(x, y, value, size=18, bold=False, color="#192638"):
    font = ImageFont.truetype(str(args.bold_font if bold else args.regular_font), size * SCALE)
    draw.text((x * SCALE, y * SCALE), value, font=font, fill=color, anchor="lt")


def step(x, y, number):
    box((x, y, x + 34, y + 34), "#4056bc", 17)
    text(x + 11, y + 7, str(number), 20, True, "white")


text(36, 28, "FITS + XISF", 36, True)
text(37, 78, "Install in Photoshop", 23, color="#506079")
box((36, 120, 884, 153), "#e5eaf5", 7)
text(49, 128, "Before you start: save your work and quit Photoshop.", 16)

box((36, 176, 336, 474), "white", 16, "#d9dfeb")
box((356, 176, 884, 474), "white", 16, "#d9dfeb")
step(54, 195, 1)
text(100, 201, "Open the folder", 23, True)
text(56, 245, "Double-click the shortcut below.", 17)
# Finder supplies the actual shortcut at (186, 327).
text(56, 428, "A new Finder window will open.", 17, color="#506079")

step(375, 195, 2)
text(422, 201, "Copy both plugins", 23, True)
text(377, 245, "Drag these into the Finder window you just opened.", 17)
# Finder supplies the actual plugin icons at (494, 327) and (742, 327).
text(378, 428, "Do not drop them directly onto the shortcut.", 17, color="#85501a")

step(36, 502, 3)
text(83, 509, "Approve the administrator prompt if asked.", 20, True)
text(83, 546, "Eject the disk image, then reopen Photoshop.", 18, color="#506079")
text(36, 613, "More help is included in the files on the right.", 16, color="#506079")

output = Path(__file__).with_name("dmg-background.png")
image.resize((920, 660), Image.Resampling.LANCZOS).save(output)
print(output)
