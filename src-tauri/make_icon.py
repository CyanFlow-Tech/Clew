"""Generate the app icon (a small branch glyph) as PNG + multi-size ICO."""

import os

from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "icons")
os.makedirs(OUT, exist_ok=True)

BG = (20, 22, 26, 255)
FG = (111, 178, 255, 255)
DIM = (124, 133, 149, 255)


def draw(size: int) -> Image.Image:
    scale = 4
    s = size * scale
    img = Image.new("RGBA", (s, s), BG)
    d = ImageDraw.Draw(img)

    # three nodes on a vertical spine, one branch to the right
    w = max(2, s // 16)
    x0 = s * 0.30
    ys = [s * 0.26, s * 0.5, s * 0.74]
    d.line([(x0, ys[0]), (x0, ys[2])], fill=FG, width=w)
    d.line([(x0, ys[1]), (s * 0.68, ys[1])], fill=FG, width=w)

    r = s * 0.075
    for (cx, cy) in [(x0, ys[0]), (x0, ys[1]), (x0, ys[2])]:
        d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=FG)
    d.ellipse([s * 0.68 - r, ys[1] - r, s * 0.68 + r, ys[1] + r], outline=DIM, width=w)

    return img.resize((size, size), Image.LANCZOS)


sizes = [16, 32, 48, 64, 128, 256]
frames = [draw(n) for n in sizes]
frames[-1].save(os.path.join(OUT, "icon.png"))
frames[-1].save(
    os.path.join(OUT, "icon.ico"),
    format="ICO",
    sizes=[(n, n) for n in sizes],
)
print("wrote", OUT)
