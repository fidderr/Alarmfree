"""Pairs for the design fixes: original left, Slint right, 540 px per side, JPEG 80.
Usage (from the repo root): python -I devtest/phone_pair_after.py <raw dir> <out dir> <name>...
Reads <raw dir>/o-<name>.png and s-<name>.png, writes <out dir>/<name>-orig-slint.jpg."""
import os
import sys
from PIL import Image

raw, out = sys.argv[1], sys.argv[2]
os.makedirs(out, exist_ok=True)
W = 540
for name in sys.argv[3:]:
    pa, pb = f"{raw}/o-{name}.png", f"{raw}/s-{name}.png"
    if not (os.path.exists(pa) and os.path.exists(pb)):
        print("missing", name)
        continue
    ims = []
    for p in (pa, pb):
        im = Image.open(p).convert("RGB")
        ims.append(im.resize((W, int(im.height * W / im.width)), Image.LANCZOS))
    h = max(i.height for i in ims)
    sheet = Image.new("RGB", (W * 2 + 8, h), (90, 90, 90))
    sheet.paste(ims[0], (0, 0))
    sheet.paste(ims[1], (W + 8, 0))
    sheet.save(f"{out}/{name}-orig-slint.jpg", quality=80)
    print("made", name, os.path.getsize(f"{out}/{name}-orig-slint.jpg"))
