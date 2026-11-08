"""Procedural art helpers shared by the face scripts. Everything is drawn
with the renderer's own polygon fill so edges match the watch exactly."""
import math, os, sys, random
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
from facekit import Bitmap, Canvas, Font, fill_poly, isin, icos

WIN = "C:/Windows/Fonts/"


def draw(w, h, fn):
    """Run fn(cv) on a scratch canvas, return the top-left w x h as a Bitmap."""
    cv = Canvas()
    fn(cv)
    return Bitmap(w, h, [row[:w] for row in cv.px[:h]])


def poly(cv, pts_px, ink=1, pat=0):
    """Polygon with float pixel coordinates."""
    fill_poly(cv, [(round(x * 16), round(y * 16)) for x, y in pts_px], ink, pat)


def disc(cv, cx, cy, r, ink=1, pat=0, n=32):
    poly(cv, [(cx + r * math.sin(2 * math.pi * k / n), cy - r * math.cos(2 * math.pi * k / n)) for k in range(n)],
         ink, pat)


def ring(cv, cx, cy, r_out, r_in, ink=1, a0=0, a1=360, n=72, pat=0):
    """Annular sector from angle a0 to a1 (degrees, 0 = up, clockwise)."""
    steps = max(2, int(n * (a1 - a0) / 360))
    outer = [(cx + r_out * math.sin(math.radians(a0 + (a1 - a0) * k / steps)),
              cy - r_out * math.cos(math.radians(a0 + (a1 - a0) * k / steps))) for k in range(steps + 1)]
    inner = [(cx + r_in * math.sin(math.radians(a0 + (a1 - a0) * k / steps)),
              cy - r_in * math.cos(math.radians(a0 + (a1 - a0) * k / steps))) for k in range(steps, -1, -1)]
    if a1 - a0 >= 360:
        # full ring: outer disc then inner hole
        disc(cv, cx, cy, r_out, ink, pat, n)
        disc(cv, cx, cy, r_in, 1 - ink, 0, n)
    else:
        poly(cv, outer + inner, ink, pat)


def hexagon(cv, cx, cy, r, ink=1, pat=0, flat=False):
    off = 0 if flat else 30
    poly(cv, [(cx + r * math.sin(math.radians(off + 60 * k)), cy - r * math.cos(math.radians(off + 60 * k)))
              for k in range(6)], ink, pat)


def line(cv, x0, y0, x1, y1, w=1.0, ink=1):
    dx, dy = x1 - x0, y1 - y0
    L = math.hypot(dx, dy) or 1
    nx, ny = -dy / L * w / 2, dx / L * w / 2
    poly(cv, [(x0 + nx, y0 + ny), (x1 + nx, y1 + ny), (x1 - nx, y1 - ny), (x0 - nx, y0 - ny)], ink)


def text_bitmap(text, path, px, threshold=128, index=0, squash=1.0):
    """Render text with a TTF/TTC at px pixel em, trimmed to ink."""
    from PIL import Image, ImageDraw, ImageFont
    ss = 4
    f = ImageFont.truetype(path, px * ss, index=index)
    bb = f.getbbox(text)
    im = Image.new("L", (bb[2] - bb[0] + 8 * ss, bb[3] - bb[1] + 8 * ss), 255)
    ImageDraw.Draw(im).text((4 * ss - bb[0], 4 * ss - bb[1]), text, font=f, fill=0)
    im = im.resize((max(1, round(im.width / ss * squash)), round(im.height / ss)), Image.LANCZOS)
    return trim(Bitmap.from_image(im, threshold))


def trim(bm):
    rows = bm.rows
    ys = [y for y, r in enumerate(rows) if any(r)]
    xs = [x for x in range(bm.w) if any(r[x] for r in rows)]
    if not ys:
        return Bitmap(1, 1, [[0]])
    rows = [r[xs[0]:xs[-1] + 1] for r in rows[ys[0]:ys[-1] + 1]]
    return Bitmap(len(rows[0]), len(rows), rows)


def vtext(chars, path, px, gap=2, threshold=128, index=0):
    """Vertical text: glyphs stacked top to bottom, centred."""
    gl = [text_bitmap(c, path, px, threshold, index) for c in chars]
    w = max(g.w for g in gl)
    rows = []
    for i, g in enumerate(gl):
        pad = (w - g.w) // 2
        rows += [[0] * pad + r + [0] * (w - g.w - pad) for r in g.rows]
        if i < len(gl) - 1:
            rows += [[0] * w for _ in range(gap)]
    return Bitmap(w, len(rows), rows)


def invert(bm):
    return Bitmap(bm.w, bm.h, [[1 - v for v in r] for r in bm.rows])


def frame_box(bm, pad=1):
    w, h = bm.w + 2 * pad, bm.h + 2 * pad
    rows = [[0] * w for _ in range(h)]
    for y, r in enumerate(bm.rows):
        rows[y + pad][pad:pad + bm.w] = r
    return Bitmap(w, h, rows)


# ---- seven segment ---------------------------------------------------------
SEGS = {"0": "abcdef", "1": "bc", "2": "abged", "3": "abgcd", "4": "fgbc", "5": "afgcd",
        "6": "afgedc", "7": "abc", "8": "abcdefg", "9": "abcdfg", "-": "g", " ": ""}


def seg_font(w, h, t, slant=0.0, gap=0.8, colon_w=None):
    """Seven-segment digits with hexagonal segments, optional italic slant
    (pixels of shift per pixel of height)."""
    def sh(pts):
        return [(x + (h - y) * slant, y) for x, y in pts]

    def hseg(x0, x1, y):
        return [(x0, y), (x0 + t / 2, y - t / 2), (x1 - t / 2, y - t / 2), (x1, y), (x1 - t / 2, y + t / 2),
                (x0 + t / 2, y + t / 2)]

    def vseg(x, y0, y1):
        return [(x, y0), (x + t / 2, y0 + t / 2), (x + t / 2, y1 - t / 2), (x, y1), (x - t / 2, y1 - t / 2),
                (x - t / 2, y0 + t / 2)]

    L, R, T, B, M = t / 2, w - t / 2, t / 2, h - t / 2, h / 2
    geo = {"a": hseg(L + gap, R - gap, T), "d": hseg(L + gap, R - gap, B), "g": hseg(L + gap, R - gap, M),
           "f": vseg(L, T + gap, M - gap), "b": vseg(R, T + gap, M - gap),
           "e": vseg(L, M + gap, B - gap), "c": vseg(R, M + gap, B - gap)}
    ww = int(math.ceil(w + h * slant)) + 1
    glyphs = {}
    for ch, segs in SEGS.items():
        def fn(cv, segs=segs):
            for sgm in segs:
                poly(cv, sh(geo[sgm]))
        glyphs[ch] = draw(ww, int(h) + 1, fn)
    cw = colon_w or max(3, int(t) + 2)
    dot = t * 0.9

    def colon(cv):
        for yy in (h * 0.3, h * 0.7):
            x = cw / 2 + (h - yy) * slant
            poly(cv, [(x - dot / 2, yy - dot / 2), (x + dot / 2, yy - dot / 2), (x + dot / 2, yy + dot / 2),
                      (x - dot / 2, yy + dot / 2)])
    glyphs[":"] = trim_h(draw(int(cw + h * slant) + 2, int(h) + 1, colon))
    return Font(int(h) + 1, glyphs, spacing=2)


def trim_h(bm):
    xs = [x for x in range(bm.w) if any(r[x] for r in bm.rows)]
    if not xs:
        return bm
    return Bitmap(xs[-1] - xs[0] + 1, bm.h, [r[xs[0]:xs[-1] + 1] for r in bm.rows])


# ---- nature -------------------------------------------------------------------
def ridge(w, base, amp, seed, rough=0.55, steps=7):
    """Midpoint-displacement mountain ridge heights (pixel y per column)."""
    rnd = random.Random(seed)
    n = 2 ** steps
    pts = [0.0] * (n + 1)
    pts[0], pts[n] = rnd.uniform(-1, 1), rnd.uniform(-1, 1)
    step, scale = n, 1.0
    while step > 1:
        half = step // 2
        for i in range(half, n, step):
            pts[i] = (pts[i - half] + pts[i + half]) / 2 + rnd.uniform(-scale, scale)
        step, scale = half, scale * rough
    return [base - amp * pts[int(x * n / (w - 1))] for x in range(w)]


def moon_phase(r, age, total=29.53, rim_pat=True):
    """Moon disc of radius r at the given age (days), lit side white (0) on
    ink background? No: lit = ink 1. Unlit face drawn as a sparse dither."""
    phi = 2 * math.pi * age / total
    size = 2 * r + 1
    rows = [[0] * size for _ in range(size)]
    for y in range(size):
        for x in range(size):
            dx, dy = x - r, y - r
            if dx * dx + dy * dy > r * r + r * 0.6:
                continue
            xr = math.sqrt(max(0.0, r * r - dy * dy))
            if phi <= math.pi:
                lit = dx > xr * math.cos(phi)
            else:
                lit = dx < -xr * math.cos(phi)
            edge = dx * dx + dy * dy >= (r - 1) * (r - 1)
            if lit or (rim_pat and edge and (x + y) % 2 == 0):
                rows[y][x] = 1
    return Bitmap(size, size, rows)


def stars(w, h, n, seed, big=0.15):
    rnd = random.Random(seed)
    rows = [[0] * w for _ in range(h)]
    for _ in range(n):
        x, y = rnd.randrange(1, w - 1), rnd.randrange(1, h - 1)
        rows[y][x] = 1
        if rnd.random() < big:
            for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                rows[y + dy][x + dx] = 1
    return Bitmap(w, h, rows)


def streaks(w, h, n, seed, length=(3, 6), slope=0.5):
    rnd = random.Random(seed)
    rows = [[0] * w for _ in range(h)]
    for _ in range(n):
        x, y = rnd.randrange(w), rnd.randrange(h)
        for k in range(rnd.randint(*length)):
            xx, yy = int(x - k * slope), y + k
            if 0 <= xx < w and 0 <= yy < h:
                rows[yy][xx] = 1
    return Bitmap(w, h, rows)


def flakes(w, h, n, seed):
    rnd = random.Random(seed)
    rows = [[0] * w for _ in range(h)]
    for _ in range(n):
        x, y = rnd.randrange(1, w - 1), rnd.randrange(1, h - 1)
        for dx, dy in ((0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)):
            rows[y + dy][x + dx] = 1
        rows[y][x] = 0
    return Bitmap(w, h, rows)


def cloud(cv, x, y, s=1.0, ink=1, pat=0):
    for cx, cy, r in ((0, 0, 5), (6, -3, 6), (12, 0, 5), (6, 2, 5)):
        disc(cv, x + cx * s, y + cy * s, r * s, ink, pat, 20)


def bolt(cv, x, y, s=1.0, ink=1):
    poly(cv, [(x + 4 * s, y), (x + 9 * s, y), (x + 6 * s, y + 6 * s), (x + 9 * s, y + 6 * s), (x + 2 * s, y + 16 * s),
              (x + 4 * s, y + 9 * s), (x + 1 * s, y + 9 * s)], ink)


def weather_icons(size=16):
    """Eight icons indexed by WX_COND."""
    c = size / 2

    def sun(cv, r=4.0, cx=c, cy=c):
        disc(cv, cx, cy, r)
        for k in range(8):
            a = math.radians(45 * k)
            line(cv, cx + (r + 1.5) * math.sin(a), cy - (r + 1.5) * math.cos(a),
                 cx + (r + 3.5) * math.sin(a), cy - (r + 3.5) * math.cos(a), 1.2)

    def cl(cv, dy=0):
        cloud(cv, 1.5, c + 2 + dy, 0.9)
        cloud(cv, 2.5, c + 2.5 + dy, 0.75, ink=0)

    icons = [
        lambda cv: sun(cv),
        lambda cv: (sun(cv, 3, c + 2, c - 2), cloud(cv, 1.5, c + 3, 0.8, ink=0), cl(cv, 1)),
        lambda cv: cl(cv),
        lambda cv: (cl(cv, -3), [line(cv, 4 + 4 * k, 12, 3 + 4 * k, 15, 1.1) for k in range(3)]),
        lambda cv: (cl(cv, -3), [disc(cv, 4 + 4 * k, 13.5, 1.1, n=8) for k in range(3)]),
        lambda cv: (cl(cv, -3), bolt(cv, 5, 7, 0.55)),
        lambda cv: [line(cv, 2 + (k % 2) * 2, 4 + 3 * k, 14 - (k % 2) * 2, 4 + 3 * k, 1.2) for k in range(4)],
        lambda cv: [line(cv, 2, 5 + 4 * k, 12 - 2 * k, 5 + 4 * k, 1.2) for k in range(3)],
    ]
    return [draw(size, size, f) for f in icons]
