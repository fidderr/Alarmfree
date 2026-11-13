import sys
from PIL import Image
ims = [Image.open(f"docs/phone/design/{n}.jpg") for n in sys.argv[2:]]
h = max(i.height for i in ims)
s = Image.new("RGB", (sum(i.width for i in ims) + 10 * (len(ims) - 1), h), (30, 30, 30))
x = 0
for i in ims:
    s.paste(i, (x, 0)); x += i.width + 10
s.save(sys.argv[1])
