import sys
from PIL import Image
out, w = sys.argv[1], int(sys.argv[2])
ims = [Image.open(p).convert("RGB") for p in sys.argv[3:]]
ims = [i.resize((w, int(i.height * w / i.width))) for i in ims]
sheet = Image.new("RGB", (w * len(ims) + 6 * (len(ims) - 1), max(i.height for i in ims)), (128, 128, 128))
for k, i in enumerate(ims):
    sheet.paste(i, (k * (w + 6), 0))
sheet.save(out)
