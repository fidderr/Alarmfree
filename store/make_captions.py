"""Captioned phone screenshots for Google Play, per language.

Run from the repo root after devtest/store_shots.sh:  python store/make_captions.py [lang...]
Takes store/screenshots/<lang>/NN_name.png (1080x1920, no text) and writes
store/screenshots/<lang>/captioned/NN_name.png (1080x1920): a short caption on top, the screen below it.
Upload the captioned ones. The captions are in the 5 app languages.
"""
import pathlib
import sys

from PIL import Image, ImageDraw, ImageFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
SHOTS = ROOT / "store" / "screenshots"
W, H = 1080, 1920
CAPTION_H = 330
FONTS = [r"C:\Windows\Fonts\seguisb.ttf", r"C:\Windows\Fonts\segoeuib.ttf", "DejaVuSans-Bold.ttf"]

CAPTIONS = {
    "01_alarms": {
        "en": "All your alarms at a glance",
        "nl": "Al je wekkers in één oogopslag",
        "es": "Todas tus alarmas de un vistazo",
        "fr": "Toutes vos alarmes en un coup d'œil",
        "de": "Alle Wecker auf einen Blick",
    },
    "02_ringing": {
        "en": "An alarm that rings on time",
        "nl": "Een wekker die op tijd afgaat",
        "es": "Una alarma que suena a su hora",
        "fr": "Une alarme qui sonne à l'heure",
        "de": "Ein Wecker, der pünktlich klingelt",
    },
    "03_challenge_math": {
        "en": "Solve a sum to switch it off",
        "nl": "Los een som op om hem uit te zetten",
        "es": "Resuelve una suma para apagarla",
        "fr": "Résolvez un calcul pour l'éteindre",
        "de": "Rechne, um ihn auszuschalten",
    },
    "04_challenges": {
        "en": "Math, memory, steps, scan and more",
        "nl": "Rekenen, geheugen, stappen, scannen en meer",
        "es": "Cálculo, memoria, pasos, escaneo y más",
        "fr": "Calcul, mémoire, pas, scan et plus",
        "de": "Rechnen, Gedächtnis, Schritte, Scannen und mehr",
    },
    "05_alarm_screens": {
        "en": "Choose how the alarm looks",
        "nl": "Kies hoe de wekker eruitziet",
        "es": "Elige el aspecto de la alarma",
        "fr": "Choisissez l'apparence de l'alarme",
        "de": "Wähle das Aussehen des Weckers",
    },
    "06_editor": {
        "en": "Set up every alarm your way",
        "nl": "Stel elke wekker op jouw manier in",
        "es": "Configura cada alarma a tu manera",
        "fr": "Réglez chaque alarme à votre façon",
        "de": "Jeden Wecker nach deinem Geschmack",
    },
    "07_history": {
        "en": "See how you wake up",
        "nl": "Zie hoe je wakker wordt",
        "es": "Mira cómo te despiertas",
        "fr": "Voyez comment vous vous réveillez",
        "de": "Sieh, wie du aufwachst",
    },
    "08_themes": {
        "en": "Your colours, light or dark",
        "nl": "Jouw kleuren, licht of donker",
        "es": "Tus colores, claro u oscuro",
        "fr": "Vos couleurs, clair ou sombre",
        "de": "Deine Farben, hell oder dunkel",
    },
    "09_reliability": {
        "en": "Tells you what could stop an alarm",
        "nl": "Zegt wat een wekker kan tegenhouden",
        "es": "Te dice qué puede detener una alarma",
        "fr": "Dit ce qui peut arrêter une alarme",
        "de": "Zeigt, was einen Wecker stoppen kann",
    },
}


def font(size: int) -> ImageFont.FreeTypeFont:
    for path in FONTS:
        try:
            return ImageFont.truetype(path, size)
        except OSError:
            continue
    return ImageFont.load_default()


def wrap(draw: ImageDraw.ImageDraw, text: str, f: ImageFont.FreeTypeFont, width: int) -> list[str]:
    lines, line = [], ""
    for word in text.split():
        trial = f"{line} {word}".strip()
        if draw.textlength(trial, font=f) <= width or not line:
            line = trial
        else:
            lines.append(line)
            line = word
    return lines + [line]


def background() -> Image.Image:
    img = Image.new("RGB", (W, H))
    px = img.load()
    top, bottom = (11, 18, 48), (42, 31, 60)
    for y in range(H):
        t = y / (H - 1)
        row = tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3))
        for x in range(W):
            px[x, y] = row
    return img


def caption_one(src: pathlib.Path, text: str, out: pathlib.Path) -> None:
    img = background()
    draw = ImageDraw.Draw(img)
    size = 76
    while True:
        f = font(size)
        lines = wrap(draw, text, f, W - 120)
        if len(lines) <= 2 or size <= 52:
            break
        size -= 4
    line_h = int(size * 1.2)
    y = (CAPTION_H - line_h * len(lines)) // 2 + 20
    for line in lines:
        draw.text((W // 2, y), line, font=f, fill="white", anchor="mt")
        y += line_h
    shot = Image.open(src).convert("RGB")
    room_h = H - CAPTION_H - 60
    scale = room_h / shot.height
    shot = shot.resize((round(shot.width * scale), room_h), Image.LANCZOS)
    mask = Image.new("L", shot.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, *shot.size), radius=48, fill=255)
    x = (W - shot.width) // 2
    frame = Image.new("RGB", (shot.width + 12, shot.height + 12), (5, 6, 12))
    fm = Image.new("L", frame.size, 0)
    ImageDraw.Draw(fm).rounded_rectangle((0, 0, *frame.size), radius=54, fill=255)
    img.paste(frame, (x - 6, CAPTION_H - 6), fm)
    img.paste(shot, (x, CAPTION_H), mask)
    out.parent.mkdir(parents=True, exist_ok=True)
    img.save(out, optimize=True)


def run(lang: str) -> int:
    count = 0
    for src in sorted((SHOTS / lang).glob("[0-9][0-9]_*.png")):
        captions = CAPTIONS.get(src.stem)
        if not captions:
            print(f"{lang}: no caption for {src.stem}")
            continue
        out = SHOTS / lang / "captioned" / src.name
        caption_one(src, captions[lang], out)
        count += 1
    return count


if __name__ == "__main__":
    for lang in sys.argv[1:] or ["en", "nl", "es", "fr", "de"]:
        print(lang, run(lang), "captioned")
