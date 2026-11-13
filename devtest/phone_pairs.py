"""Builds paired images (original left, Slint right) and a contact sheet from the raw phone captures."""
import os, sys
from PIL import Image, ImageDraw, ImageFont

RAW = "devtest-out/phone/design"
OUT = "docs/phone/design"
PAIRS = [
    ("firstrun", "firstrun-orig", "firstrun-slint"),
] + [(n, f"o-{n}", f"s-{n}") for n in sys.argv[1:]]
W = 400
os.makedirs(OUT, exist_ok=True)
try:
    font = ImageFont.truetype("arial.ttf", 20)
except OSError:
    font = ImageFont.load_default()
made = []
for name, a, b in PAIRS:
    pa, pb = f"{RAW}/{a}.png", f"{RAW}/{b}.png"
    if not (os.path.exists(pa) and os.path.exists(pb)):
        print("missing", name)
        continue
    ims = []
    for p in (pa, pb):
        im = Image.open(p).convert("RGB")
        ims.append(im.resize((W, int(im.height * W / im.width)), Image.LANCZOS))
    h = max(i.height for i in ims)
    sheet = Image.new("RGB", (W * 2 + 8, h + 28), (90, 90, 90))
    d = ImageDraw.Draw(sheet)
    d.text((6, 3), f"{name}: ORIGINAL", fill=(255, 255, 255), font=font)
    d.text((W + 14, 3), "SLINT", fill=(255, 255, 255), font=font)
    sheet.paste(ims[0], (0, 28))
    sheet.paste(ims[1], (W + 8, 28))
    sheet.save(f"{OUT}/{name}.jpg", quality=80)
    made.append(name)
# contact sheet: pairs at 110 px per side, 8 per row
tw = 110
th = 240
cols = 8
rows = (len(made) + cols - 1) // cols
cs = Image.new("RGB", (cols * (tw * 2 + 14), rows * (th + 22)), (60, 60, 60))
d = ImageDraw.Draw(cs)
try:
    small = ImageFont.truetype("arial.ttf", 13)
except OSError:
    small = font
for k, name in enumerate(made):
    a, b = [n for nm, *n in PAIRS if nm == name][0]
    x, y = (k % cols) * (tw * 2 + 14), (k // cols) * (th + 22)
    d.text((x + 3, y + 3), name, fill=(255, 255, 255), font=small)
    for j, f in enumerate((a, b)):
        im = Image.open(f"{RAW}/{f}.png").convert("RGB")
        im = im.resize((tw, int(im.height * tw / im.width)), Image.LANCZOS).crop((0, 0, tw, th))
        cs.paste(im, (x + j * (tw + 4), y + 20))
cs.save(f"{OUT}/overview.jpg", quality=80)
print(len(made), "pairs")
