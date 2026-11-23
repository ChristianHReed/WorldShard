"""Draws the Home Screen / app icon (first pass): the shard floating in
space, with the Rim Sea around green land and the snowy core.

    python3 tools/make_icons.py      # writes game/web/icons/*.png
"""
import math
import os
import random

from PIL import Image, ImageDraw, ImageFilter

S = 1024
OUT = os.path.join(os.path.dirname(__file__), "..", "game", "web", "icons")


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(len(a)))


def draw():
    img = Image.new("RGB", (S, S))
    d = ImageDraw.Draw(img)
    # space: deep blue at the top to violet-blue at the bottom
    for y in range(S):
        d.line([(0, y), (S, y)], fill=lerp((8, 12, 30), (26, 30, 66), y / S))
    rnd = random.Random(7)
    for _ in range(90):
        x, y, r = rnd.randrange(S), rnd.randrange(S), rnd.choice([2, 2, 3, 4])
        a = rnd.randrange(120, 255)
        d.ellipse([x - r, y - r, x + r, y + r], fill=(a, a, min(255, a + 20)))

    cx, top = 512, 430  # centre of the island's top
    rx, ry = 330, 120   # the island's outline seen from a little above

    # underside: rock that narrows to a point, with strata
    pts = []
    for i in range(0, 181, 6):
        a = math.radians(i)
        pts.append((cx + rx * math.cos(a), top + ry * math.sin(a) * 0.35))
    under = [pts[0]]
    jag = random.Random(3)
    for k in range(1, 12):
        t = k / 12
        x = cx + rx * (1 - t) ** 1.25 * (1 if k % 2 else 0.93)
        under.append((x + jag.uniform(-14, 14), top + 40 + t * 480))
    under.append((cx + 8, top + 540))
    for k in range(11, 0, -1):
        t = k / 12
        x = cx - rx * (1 - t) ** 1.25 * (1 if k % 2 else 0.93)
        under.append((x + jag.uniform(-14, 14), top + 40 + t * 480))
    under.append(pts[-1])
    rock = Image.new("L", (S, S), 0)
    ImageDraw.Draw(rock).polygon(under, fill=255)
    layers = Image.new("RGB", (S, S))
    ld = ImageDraw.Draw(layers)
    for y in range(top, top + 560):
        t = (y - top) / 560
        band = 0.82 + 0.18 * math.sin(y / 13.0)
        c = lerp((122, 88, 60), (58, 40, 34), t)
        ld.line([(0, y), (S, y)], fill=tuple(int(v * band) for v in c))
    img.paste(layers, (0, 0), rock)

    # the top: Rim Sea, then land, then the snowy core
    d = ImageDraw.Draw(img)
    d.ellipse([cx - rx, top - ry, cx + rx, top + ry], fill=(46, 112, 168))
    d.ellipse([cx - rx + 26, top - ry + 16, cx + rx - 26, top + ry - 16], fill=(68, 136, 72))
    d.ellipse([cx - 210, top - 70, cx + 120, top + 74], fill=(92, 156, 84))
    # mountains of the core: grey rock with snow caps
    peaks = [(cx - 70, top - 150, 120), (cx + 30, top - 210, 150), (cx + 120, top - 120, 100)]
    for px, py, w in peaks:
        base = top + 20
        d.polygon([(px - w, base), (px, py), (px + w, base)], fill=(128, 130, 138))
        cap = py + (base - py) * 0.38
        hw = w * 0.38
        d.polygon([(px - hw, cap), (px, py), (px + hw, cap)], fill=(244, 246, 250))
    # a soft glow of cloud around the rim
    glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    gd.ellipse([cx - rx - 40, top - ry - 26, cx + rx + 40, top + ry + 26], outline=(255, 255, 255, 70), width=22)
    glow = glow.filter(ImageFilter.GaussianBlur(14))
    img = Image.alpha_composite(img.convert("RGBA"), glow).convert("RGB")
    return img


def main():
    os.makedirs(OUT, exist_ok=True)
    big = draw()
    for size in (512, 192, 180):
        big.resize((size, size), Image.LANCZOS).save(os.path.join(OUT, f"icon-{size}.png"), optimize=True)
    print("icons written to", os.path.normpath(OUT))


if __name__ == "__main__":
    main()
