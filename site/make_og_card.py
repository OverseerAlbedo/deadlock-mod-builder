"""One-off generator for site/static/og-card.png, the 1200x630 sharing image.

Not part of the site build and not run in CI: the PNG it writes is committed.
Run it again only when the card should change. It needs Pillow and the
Segoe UI fonts that ship with Windows:

    python -m pip install pillow==13.0.0
    python site/make_og_card.py
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "site" / "static" / "og-card.png"
SCREENSHOT = ROOT / "docs" / "images" / "character-and-model.png"
FONTS = Path("C:/Windows/Fonts")

W, H = 1200, 630
BG = "#121518"
TEXT = "#f1f3f5"
SECONDARY = "#afb7bf"
ACCENT = "#7fe0b0"
BORDER = "#30363d"


def cubic(p0, p1, p2, p3, steps=48):
    pts = []
    for i in range(1, steps + 1):
        t = i / steps
        u = 1 - t
        pts.append((
            u**3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t**3 * p3[0],
            u**3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t**3 * p3[1],
        ))
    return pts


def draw_icon(size):
    """The app's "D" icon (src-tauri/icons/icon.svg), drawn at 4x and reduced."""
    k = size * 4 / 256
    img = Image.new("RGBA", (size * 4, size * 4), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    def sc(points):
        return [(x * k, y * k) for x, y in points]

    d.rounded_rectangle(sc([(8, 8), (248, 248)]), radius=48 * k, fill="#242424")
    outer = [(68, 59), (118, 59)]
    outer += cubic((118, 59), (166, 59), (195, 86), (195, 128))
    outer += cubic((195, 128), (195, 170), (166, 197), (118, 197))
    outer += [(68, 197)]
    d.polygon(sc(outer), fill="#e7e5df")
    hole = [(96, 86), (96, 170), (119, 170)]
    hole += cubic((119, 170), (149, 170), (167, 154), (167, 128))
    hole += cubic((167, 128), (167, 102), (149, 86), (119, 86))
    d.polygon(sc(hole), fill="#242424")
    d.rectangle(sc([(48, 113), (111, 143)]), fill="#b9b6af")
    return img.resize((size, size), Image.LANCZOS)


def main():
    card = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(card)

    # Screenshot on the right, running off the right edge.
    shot = Image.open(SCREENSHOT).convert("RGB")
    sw = 760
    sh = round(shot.height * sw / shot.width)
    shot = shot.resize((sw, sh), Image.LANCZOS)
    sx, sy = 500, (H - sh) // 2
    d.rounded_rectangle((sx - 2, sy - 2, sx + sw + 40, sy + sh + 1), radius=10, fill=BORDER)
    card.paste(shot, (sx, sy))

    icon = draw_icon(96)
    card.paste(icon, (64, 96), icon)

    bold = ImageFont.truetype(str(FONTS / "segoeuib.ttf"), 66)
    regular = ImageFont.truetype(str(FONTS / "segoeui.ttf"), 32)
    small = ImageFont.truetype(str(FONTS / "segoeui.ttf"), 26)
    d.text((62, 220), "Deadlock", font=bold, fill=TEXT)
    d.text((62, 295), "Mod Builder", font=bold, fill=TEXT)
    d.text((64, 396), "Make Deadlock", font=regular, fill=SECONDARY)
    d.text((64, 438), "character mods", font=regular, fill=SECONDARY)
    d.text((64, 512), "Free and open source for Windows", font=small, fill=ACCENT)

    OUT.parent.mkdir(parents=True, exist_ok=True)
    card.save(OUT, optimize=True)
    print(f"Wrote {OUT.relative_to(ROOT)} ({OUT.stat().st_size // 1024} KB)")


if __name__ == "__main__":
    main()
