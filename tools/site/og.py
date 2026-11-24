# usage: python tools/site/og.py <repo root>. Windows fonts (malgun*.ttf).
# Draws site/og.png (1200x630), the link preview: title on the left, KakaoTalk for Mac (light) on the right.
from PIL import Image, ImageDraw, ImageFont, ImageFilter
import math, sys
root = sys.argv[1]
W, H, S = 1200, 630, 2            # drawn at 2x, then scaled down for smooth edges
img = Image.new("RGB", (W*S, H*S), "#fbfcfe")
d = ImageDraw.Draw(img)
bold = lambda px: ImageFont.truetype("C:/Windows/Fonts/malgunbd.ttf", px*S)
reg = lambda px: ImageFont.truetype("C:/Windows/Fonts/malgun.ttf", px*S)
ACC, ACC2, TEXT, MUTED = (31,143,214), (63,214,174), (20,24,36), (93,104,121)

def grad_rect(box, c1, c2, radius):
    x0,y0,x1,y1 = box; w,h = x1-x0, y1-y0
    g = Image.new("RGB", (w,h))
    gd = ImageDraw.Draw(g)
    for i in range(w+h):
        t = i/(w+h); c = tuple(int(c1[k]+(c2[k]-c1[k])*t) for k in range(3))
        gd.line([(i,0),(i-h,h)], fill=c, width=2)
    m = Image.new("L", (w,h), 0); ImageDraw.Draw(m).rounded_rectangle([0,0,w-1,h-1], radius, fill=255)
    img.paste(g, (x0,y0), m)

def dashed_round_rect(box, r, color, width, dash, gap):
    x0,y0,x1,y1 = box
    # walk the outline as a path of segments and arcs
    pts = []
    def arc(cx,cy,a0,a1):
        for k in range(0, 31): a = math.radians(a0+(a1-a0)*k/30); pts.append((cx+r*math.cos(a), cy+r*math.sin(a)))
    pts.append((x0+r,y0)); pts.append((x1-r,y0)); arc(x1-r,y0+r,-90,0); pts.append((x1,y1-r)); arc(x1-r,y1-r,0,90)
    pts.append((x0+r,y1)); arc(x0+r,y1-r,90,180); pts.append((x0,y0+r)); arc(x0+r,y0+r,180,270)
    pts.append(pts[0])
    on, left = True, dash
    for (ax,ay),(bx,by) in zip(pts, pts[1:]):
        seg = math.hypot(bx-ax, by-ay); pos = 0
        while pos < seg:
            step = min(left, seg-pos)
            t0, t1 = pos/seg, (pos+step)/seg
            if on: d.line([(ax+(bx-ax)*t0, ay+(by-ay)*t0), (ax+(bx-ax)*t1, ay+(by-ay)*t1)], fill=color, width=width)
            pos += step; left -= step
            if left <= 0: on = not on; left = dash if on else gap

# right: soft panel and the window
# (no panel behind the window, like the page)
shot = Image.open(f"{root}/site/kakaotalk-mac-light.png").convert("RGBA")
sh = 600*S; sw = int(shot.width*sh/shot.height)
shot = shot.resize((sw, sh), Image.LANCZOS)
sx, sy = 925*S - sw//2, 22*S
img.paste(shot, (sx, sy), shot)
# the dashed box on the emptied ad slot (same numbers as the page)
def P(fx, fy): return (sx + fx*sw, sy + fy*sh)
x0,y0 = P(0.262+0.016, 0.768+0.013); x1,y1 = P(0.262+0.628-0.016, 0.768+0.133-0.013)
dashed_round_rect((x0,y0,x1,y1), 7*S, ACC, 3*S, 9*S, 6*S)

# left: name, title, line
icon = Image.open(f"{root}/site/icon-256.png").convert("RGBA").resize((64*S,64*S), Image.LANCZOS)
img.paste(icon, (72*S, 88*S), icon)
d.text((150*S, 120*S), "낮잠 TalkNap", font=bold(30), fill=TEXT, anchor="lm")
d.text((72*S, 250*S), "카카오톡 광고 칸을", font=bold(60), fill=TEXT, anchor="ls")
t = "빈 칸으로"; f = bold(60)
bx0, by0, bx1, by1 = d.textbbox((0,0), t, font=f)
tx, ty = 72*S + 16*S, 345*S
d.text((tx, ty), t, font=f, fill=TEXT, anchor="ls")
asc = d.textbbox((tx,ty), t, font=f, anchor="ls")
dashed_round_rect((72*S, asc[1]-14*S, asc[2]+16*S, asc[3]+12*S), 16*S, ACC, 4*S, 12*S, 8*S)
d.text((72*S, 430*S), "광고 대신 배경색만 칠한 빈 칸을 보여 줍니다.", font=reg(26), fill=MUTED, anchor="ls")
# chips
x = 72*S
for label in ["Windows", "macOS", "무료 · 오픈 소스"]:
    f2 = bold(20); w = d.textlength(label, font=f2)
    d.rounded_rectangle([x, 480*S, x + w + 32*S, 520*S], 20*S, fill=(238,242,247))
    d.text((x + 16*S, 500*S), label, font=f2, fill=MUTED, anchor="lm")
    x += w + 32*S + 10*S

img.resize((W, H), Image.LANCZOS).save(f"{root}/site/og.png", optimize=True)
print("ok")
