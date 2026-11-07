"""Store graphics for Google Play, per language: the 1024x500 feature graphic.

Run from the repo root after devtest/store_shots.sh:  python store/make_graphics.py [lang...]
Draws an HTML page with Brave (headless) and crops it to the exact size with Pillow.
Out: store/graphics/feature-graphic-<lang>.png
"""
import pathlib
import subprocess
import sys
import tempfile

from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
BRAVE = r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe"
W, H = 1024, 500

TEXT = {
    "en": ("Wake up for real.", "An alarm you only stop by solving a challenge."),
    "nl": ("Word echt wakker.", "Een wekker die pas stopt als je een uitdaging oplost."),
    "es": ("Despiértate de verdad.", "Un despertador que solo se apaga si superas un reto."),
    "fr": ("Réveillez-vous pour de vrai.", "Un réveil qui ne s'arrête qu'après un défi."),
    "de": ("Wach werden, wirklich.", "Ein Wecker, der erst nach einer Aufgabe verstummt."),
}

PAGE = """<!doctype html><html><head><meta charset="utf-8"><style>
html, body {{ margin: 0; width: {w}px; height: {h}px; overflow: hidden; }}
body {{
  font-family: "Segoe UI", system-ui, sans-serif; color: #fff;
  background:
    radial-gradient(ellipse 520px 380px at 88% 118%, rgba(255,140,60,.95), rgba(255,140,60,0) 70%),
    radial-gradient(ellipse 420px 260px at 62% -20%, rgba(80,170,255,.35), rgba(80,170,255,0) 70%),
    radial-gradient(ellipse 380px 220px at 18% 110%, rgba(130,90,255,.35), rgba(130,90,255,0) 70%),
    linear-gradient(160deg, #0b1230 0%, #151a3f 55%, #2a1f3c 100%);
  position: relative;
}}
.left {{ position: absolute; left: 64px; top: 0; bottom: 0; width: 560px; display: flex; flex-direction: column; justify-content: center; }}
.brand {{ display: flex; align-items: center; gap: 22px; }}
.icon {{ width: 104px; height: 104px; border-radius: 26px; box-shadow: 0 10px 30px rgba(0,0,0,.35); }}
.name {{ font-size: 64px; font-weight: 700; letter-spacing: -1px; }}
.tag {{ font-size: 40px; font-weight: 650; margin-top: 30px; line-height: 1.15; }}
.sub {{ font-size: 24px; color: rgba(255,255,255,.82); margin-top: 12px; line-height: 1.35; max-width: 520px; }}
.phone {{
  position: absolute; right: 70px; top: 46px; width: 236px; height: 420px; border-radius: 34px;
  background: #05060c; padding: 9px; box-shadow: 0 24px 60px rgba(0,0,0,.55), 0 0 0 2px rgba(255,255,255,.12);
  transform: rotate(6deg);
}}
.phone img {{ width: 100%; height: 100%; border-radius: 26px; object-fit: cover; object-position: top; display: block; }}
</style></head><body>
<div class="left">
  <div class="brand"><img class="icon" src="{icon}"><div class="name">AlarmFree</div></div>
  <div class="tag">{tag}</div>
  <div class="sub">{sub}</div>
</div>
<div class="phone"><img src="{shot}"></div>
</body></html>"""


def render(lang: str) -> pathlib.Path:
    tag, sub = TEXT[lang]
    shot = ROOT / "store" / "screenshots" / lang / "02_ringing.png"
    if not shot.exists():
        shot = ROOT / "store" / "screenshots" / "en" / "02_ringing.png"
    html = PAGE.format(w=W, h=H, tag=tag, sub=sub,
                       icon=(ROOT / "store" / "graphics" / "icon-512.png").as_uri(), shot=shot.as_uri())
    out = ROOT / "store" / "graphics" / f"feature-graphic-{lang}.png"
    with tempfile.TemporaryDirectory() as tmp:
        page = pathlib.Path(tmp) / "fg.html"
        page.write_text(html, encoding="utf-8")
        raw = pathlib.Path(tmp) / "raw.png"
        # Headless windows lose some height to nothing visible: render taller, crop to size.
        subprocess.run([BRAVE, "--headless=new", "--disable-gpu", "--hide-scrollbars", "--window-size=1024,800",
                        f"--screenshot={raw}", page.as_uri()], check=True, capture_output=True)
        img = Image.open(raw).convert("RGB")
        if img.width < W or img.height < H:
            raise SystemExit(f"{lang}: rendered only {img.size}")
        img.crop((0, 0, W, H)).save(out, optimize=True)
    return out


if __name__ == "__main__":
    for lang in sys.argv[1:] or list(TEXT):
        print(render(lang))
