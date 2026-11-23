# usage: python tools/site/draw-ad.py <repo root>. Works on the original screenshots (wip/site-screenshot 13bc3e4):
# it paints over them in place. Windows fonts (malgunbd.ttf).
# Paints the drawn ad into the "with an ad" screenshots: the slot (x 112-912, y 1155-1356 of 1024x1504) gets a plain
# fill over the blurred banner, then the gradient ad 20 px inside it, with its two bars and the AD badge.
from PIL import Image, ImageDraw, ImageFont
import sys
root = sys.argv[1]
SS = 4                                   # supersampling for smooth edges
X0, X1, Y0, Y1 = 112, 912, 1155, 1356    # the slot (window width, below the list)
AX0, AX1, AY0, AY1 = 132, 892, 1175, 1336
for name in ["kakaotalk-mac-ad-light.png", "kakaotalk-mac-ad-dark.png"]:
    p = f"{root}/site/{name}"
    im = Image.open(p).convert("RGBA")
    # fill: the banner's own colour, sampled just inside its top-left; only fully opaque pixels (keep the window's
    # rounded corners and edge)
    fill = im.getpixel((X0 + 6, Y0 + 4))[:3]
    px = im.load()
    for y in range(Y0, Y1):
        for x in range(X0, X1):
            if px[x, y][3] == 255: px[x, y] = fill + (255,)
    w, h = AX1 - AX0, AY1 - AY0
    big = Image.new("RGBA", (w * SS, h * SS), (0, 0, 0, 0))
    # gradient 110deg from #c8b9ff to #ffa6c9
    g = Image.new("RGBA", big.size)
    gd = ImageDraw.Draw(g)
    c1, c2 = (0xc8, 0xb9, 0xff), (0xff, 0xa6, 0xc9)
    W, H = big.size
    for x in range(W):
        t = x / (W - 1)
        gd.line([(x, 0), (x, H)], fill=tuple(int(c1[i] + (c2[i] - c1[i]) * t) for i in range(3)) + (255,))
    mask = Image.new("L", big.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, W - 1, H - 1], 22 * SS, fill=255)
    big.paste(g, (0, 0), mask)
    d = ImageDraw.Draw(big, "RGBA")
    pad = int(w * 0.07); inner = w - 2 * pad
    bh, gap = round(h * 0.13), round(h * 0.10)
    top = (h - (2 * bh + gap)) / 2
    for i, frac in enumerate([0.62, 0.38]):
        y = top + i * (bh + gap)
        d.rounded_rectangle([pad * SS, y * SS, (pad + inner * frac) * SS, (y + bh) * SS], bh * SS / 2, fill=(255, 255, 255, 191))
    # AD badge, 16 px from the top and the right
    f = ImageFont.truetype("C:/Windows/Fonts/malgunbd.ttf", 22 * SS)
    tw = d.textlength("AD", font=f) / SS
    bw, bhh = tw + 24, 30
    bx1, by0 = w - 16, 16
    d.rounded_rectangle([(bx1 - bw) * SS, by0 * SS, bx1 * SS, (by0 + bhh) * SS], bhh * SS / 2, fill=(255, 255, 255, 255))
    d.text(((bx1 - bw / 2) * SS, (by0 + bhh / 2) * SS), "AD", font=f, fill=(0x7a, 0x5b, 0xd6, 255), anchor="mm")
    small = big.resize((w, h), Image.LANCZOS)
    im.alpha_composite(small, (AX0, AY0))
    im.save(p, optimize=True)
    print(name, "fill", fill)
