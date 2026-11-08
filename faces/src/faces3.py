"""Third batch: cyberpunk and net-diver styles. Run: python -X utf8 src/faces3.py"""
import os, sys, math, random
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
sys.path.insert(0, HERE)
from facekit import *
from facekit import render, preview, SAMPLE, PATTERNS
from pixfonts import font5x7, font3x5, F3X5
from build_faces import clock, WEEKDAYS, MONTHS
import art
from art import WIN, Bitmap
from faces2 import DEMO, box, black

OUT = os.path.join(HERE, "..", "out")
S = font5x7()
T = font3x5()
JP = WIN + "YuGothB.ttc"


def pad_bm(bm, p):
    rows = [[0] * (bm.w + 2 * p) for _ in range(bm.h + 2 * p)]
    for y, r in enumerate(bm.rows):
        rows[y + p][p:p + bm.w] = r
    return Bitmap(bm.w + 2 * p, bm.h + 2 * p, rows)


def dilate(bm):
    g = lambda x, y: 0 <= x < bm.w and 0 <= y < bm.h and bm.rows[y][x]
    return Bitmap(bm.w, bm.h, [[1 if any(g(x + dx, y + dy) for dx in (-1, 0, 1) for dy in (-1, 0, 1)) else 0
                                for x in range(bm.w)] for y in range(bm.h)])


def outline(bm):
    g = lambda x, y: 0 <= x < bm.w and 0 <= y < bm.h and bm.rows[y][x]
    return Bitmap(bm.w, bm.h, [[1 if bm.rows[y][x] and not all(g(x + dx, y + dy) for dx, dy in
                                                                   ((1, 0), (-1, 0), (0, 1), (0, -1))) else 0
                                for x in range(bm.w)] for y in range(bm.h)])


def sub(a, b):
    return Bitmap(a.w, a.h, [[1 if a.rows[y][x] and not b.rows[y][x] else 0 for x in range(a.w)] for y in range(a.h)])


def add(a, b):
    return Bitmap(a.w, a.h, [[a.rows[y][x] | b.rows[y][x] for x in range(a.w)] for y in range(a.h)])


def patterned(bm, p, phase=0):
    return Bitmap(bm.w, bm.h, [[v and (PATTERNS[p][(y + phase) & 7] >> (7 - ((x + phase) & 7))) & 1
                                for x, v in enumerate(r)] for y, r in enumerate(bm.rows)])


def clip_vis(bm, x0, y0):
    """Drop the pixels of bm (placed at x0, y0) that the bezel or the inset would hide."""
    return Bitmap(bm.w, bm.h, [[v if visible(x0 + x, y0 + y) else 0 for x, v in enumerate(r)]
                               for y, r in enumerate(bm.rows)])


def neon_font(font, glow=True):
    """Glyphs become a 1 px tube (outline of the glyph) with a dotted glow halo."""
    g2 = {}
    for ch, bm in font.glyphs.items():
        b = pad_bm(bm, 2)
        tube = outline(b)
        if glow:
            halo = sub(dilate(dilate(b)), dilate(b))
            tube = add(tube, patterned(halo, 1))
        g2[ch] = tube
    return Font(font.height + 4, g2, spacing=font.spacing - 3)


def neon_sign(bm, pad=3):
    """Kanji sign: double tube frame around the text."""
    b = pad_bm(bm, pad + 3)
    w, h = b.w, b.h
    rows = [r[:] for r in b.rows]
    for (x0, y0, x1, y1) in ((0, 0, w - 1, h - 1), (2, 2, w - 3, h - 3)):
        for x in range(x0, x1 + 1):
            rows[y0][x] = rows[y1][x] = 1
        for y in range(y0, y1 + 1):
            rows[y][x0] = rows[y][x1] = 1
    return Bitmap(w, h, rows)


# ============================================================ GHOST: data dive
def glyph_noise(seed):
    """Original 3x5 'machine script' glyphs, mixed with digits."""
    rnd = random.Random(seed)
    out = []
    for _ in range(24):
        rows = [[1 if rnd.random() < 0.45 else 0 for _ in range(3)] for _ in range(5)]
        rows[0][rnd.randrange(3)] = 1
        rows[4][rnd.randrange(3)] = 1
        out.append(rows)
    for d in "0123456789":
        out.append([[1 if c == "#" else 0 for c in row] for row in F3X5[d].split(" ")])
    return out


def data_rain(frame, n_frames, seed=21):
    rnd = random.Random(seed)
    glyphs = glyph_noise(seed)
    W_, H_ = 128, 128
    rows = [[0] * W_ for _ in range(H_)]
    for col in range(0, W_ // 4):
        x = col * 4
        speed = rnd.choice([1, 2, 3])
        length = rnd.randint(4, 14)
        head0 = rnd.randrange(H_ // 6 + 20)
        seq = [rnd.randrange(len(glyphs)) for _ in range(40)]
        head = (head0 + frame * speed * 3) % (H_ // 6 + 20)
        dim = rnd.random() < 0.4
        for k in range(length):
            cell = head - k
            if cell < 0 or cell * 6 >= H_:
                continue
            g = glyphs[seq[(cell + frame) % 40] if k < 2 else seq[cell % 40]]
            fade = k > length * 0.6
            for gy in range(5):
                for gx in range(3):
                    if g[gy][gx]:
                        yy, xx = cell * 6 + gy, x + gx
                        if dim or fade:
                            if (xx + yy) % 2:
                                continue
                        if yy < H_:
                            rows[yy][xx] = 1
    return Bitmap(W_, H_, rows)


def ghost_dive():
    f = Face("GHOST")
    f.strings(S_WEEKDAY, WEEKDAYS)
    f.strings(S_MONTH, MONTHS)
    black(f)
    n = 6
    f.frames([data_rain(i, n) for i in range(n)], 0, 0, MINUTE, 0, n, M_ERASE, wrap=True)
    thin = Font.from_ttf(WIN + "bahnschrift.ttf", 22, "0123456789:", squash=0.9, spacing=2, fixed_digits=True,
                         threshold=150)
    # console panel: starts just under the inset, corners pulled in to the bezel circle
    px, pw = 11, 107
    f.rect(px, 50, pw, 47, ink=1)
    box(f, px, 50, pw, 47, ink=0)
    f.rect(px, 50, pw, 12, ink=0)
    dennou = art.text_bitmap("電脳", JP, 10)
    f.image(dennou, px + 3, 51, M_INK)
    f.text(px + 25, 54, T, lit="DIVE SESSION", ink=1)
    with f.when(BT):
        f.text(px + pw - 4, 54, T, lit="LINK", align=RIGHT, ink=1)
    with f.when(BT, C_Z):
        f.text(px + pw - 4, 54, T, lit="SOLO", align=RIGHT, ink=1)
    clock(f, thin, 47, 65, ink=0, zpad=True)
    f.rect(84, 65, 1, 23, ink=0, pat=1)
    for i, (lab, src) in enumerate((("PULSE", HR), ("SYNC%", STEP_PCT), ("CELL%", BATTERY))):
        f.text(88, 66 + i * 8, T, lit=lab[:4], ink=0)
        f.text(px + pw - 4, 66 + i * 8, T, src, F_INT, align=RIGHT, ink=0)
    f.text(15, 90, T, WEEKDAY, F_NAME, ink=0)
    f.text(29, 90, T, DAY, F_ZPAD, pad=2, ink=0)
    f.text(39, 90, T, MONTH, F_NAME, ink=0)
    f.text(px + pw - 4, 90, T, lit="GHOST OK", align=RIGHT, ink=0)
    f.inset(SECOND, F_ZPAD)
    return f


# ============================================================ GHOST: optic camouflage
def figure(cv, cx, top, s=1.0, ink=1):
    P = lambda pts: art.poly(cv, [(cx + x * s, top + y * s) for x, y in pts], ink)
    art.disc(cv, cx + 0.5 * s, top + 5 * s, 4.6 * s, ink, n=20)                     # head
    P([(-1.5, 9), (2.5, 9), (2.5, 12), (-1.5, 12)])                                 # neck
    P([(-8, 12.5), (9, 12.5), (7, 32), (6, 38), (-5, 38), (-6, 32)])                # torso
    P([(-8, 13), (-4.5, 14), (-7, 27), (-9, 37), (-12, 36.5), (-10.5, 26)])         # left arm
    P([(9, 13), (5.5, 14), (8.5, 26), (14, 33), (12, 35.5), (6, 28)])               # right arm, bent
    P([(-5, 37), (0.5, 37), (-1, 55), (-1.5, 70), (-6, 70), (-5.5, 55)])            # left leg
    P([(0.5, 37), (6, 37), (7.5, 54), (11, 68), (6.5, 69.5), (2.5, 55)])            # right leg, stepping


def ghost_camo():
    f = Face("CAMO")
    f.strings(S_WEEKDAY, WEEKDAYS)
    big = Font.from_ttf(WIN + "bahnschrift.ttf", 30, "0123456789:", squash=0.8, spacing=2, fixed_digits=True)
    # background: fine city grid the figure refracts, kept inside the bezel circle
    bg = art.draw(128, 128, lambda cv: [cv.span(y, 66, 128, 1, 3) for y in range(52, 128) if y % 6 == 0])
    f.image(clip_vis(bg, 0, 0), 0, 0, M_INK)
    fig = art.draw(128, 128, lambda cv: figure(cv, 91, 53, 0.77))
    edge = outline(fig)
    # camouflage strength follows battery: full charge = almost invisible
    levels = []
    for fill, edge_p in ((0, 0), (10, 0), (1, 0), (3, 0), (9, 1)):
        lv = add(patterned(fig, fill), patterned(edge, edge_p))
        levels.append(lv)
    f.frames(levels, 0, 0, BATTERY, 0, 101, M_XOR)
    # tracking reticle around the figure
    for (x, y, dx, dy) in ((78, 50, 1, 1), (106, 50, -1, 1), (78, 108, 1, -1), (106, 108, -1, -1)):
        f.rect(x if dx > 0 else x - 5, y, 6, 1)
        f.rect(x, y if dy > 0 else y - 5, 1, 6)
    # status column in the chord left of the inset
    f.text(36, 9, T, lit="OPTIC CAMO")
    f.text(42, 17, S, BATTERY, F_INT, align=RIGHT)
    f.text(44, 17, S, lit="%")
    with f.when(BATTERY, C_GE, 60):
        f.text(53, 19, T, lit="LOST")
    with f.when(BATTERY, C_LT, 60):
        f.text(53, 19, T, lit="TRACK")
    f.bar(17, 28, 58, 3, BATTERY, 0, 100)
    box(f, 16, 27, 60, 5)
    f.text(12, 36, T, lit="HR")
    f.text(34, 36, T, HR, F_INT, align=RIGHT)
    f.text(42, 36, T, lit="STEP")
    f.text(70, 36, T, STEP_PCT, F_INT, align=RIGHT)
    f.text(71, 36, T, lit="%")
    f.rect(6, 45, 74, 1)
    clock(f, big, 41, 49)
    f.text(16, 84, T, WEEKDAY, F_NAME)
    f.text(30, 84, T, DAY, F_INT)
    f.inset(HR, F_INT)
    return f


# ============================================================ CYBERPUNK: neon alley
def cable(cv, x0, y0, x1, y1, sag):
    for i in range(129):
        t = i / 128
        x = x0 + (x1 - x0) * t
        y = y0 + (y1 - y0) * t + sag * 4 * t * (1 - t)
        xi, yi = int(x), int(y)
        if 0 <= xi < 128 and 0 <= yi < 128:
            cv.px[yi][xi] = 1


def neon_alley():
    f = Face("NEON")
    black(f)
    base = Font.from_ttf(WIN + "bahnschrift.ttf", 30, "0123456789:", squash=0.85, spacing=4, fixed_digits=True)
    neon = neon_font(base)
    # hanging signs
    # hanging signs sit in the chord left of the inset
    vx, vy = 23, 17
    v = neon_sign(art.vtext("電脳街", JP, 11, gap=2), pad=1)
    f.image(v, vx, vy, M_ERASE)
    open_ = neon_sign(art.text_bitmap("営業中", JP, 9), pad=1)
    dim = patterned(open_, 3)
    ox = vx + v.w + 2
    f.frames([open_, open_, open_, dim, open_], ox, 9, MINUTE, 0, 5, M_ERASE, wrap=True)
    kusuri = art.text_bitmap("薬", JP, 12)
    plate = pad_bm(kusuri, 3)
    f.rect(ox, 30, plate.w, plate.h, ink=0)
    f.image(plate, ox, 30, M_INK)
    f.image(art.draw(128, 128, lambda cv: (cable(cv, vx + v.w - 2, vy + 3, 127, 30, 8),
                                           cable(cv, vx + v.w - 2, vy + 36, 92, 50, 4))), 0, 0, M_ERASE)
    # the time is the biggest sign on the street
    clock(f, neon, 71, 60, ink=0)
    # wet street: mirror of the hanging sign, broken up by ripples, cut at the bezel
    f.rect(0, 94, 128, 1, ink=0)
    refl = Bitmap(v.w, min(v.h, 30), [patterned(v, 1).rows[v.h - 1 - y] for y in range(min(v.h, 30))])
    rip = art.draw(v.w, refl.h, lambda cv: [cv.span(y, 0, v.w, 1) for y in range(refl.h) if y % 3 == 2])
    f.image(clip_vis(sub(refl, rip), vx, 96), vx, 96, M_ERASE)
    # readout box narrows with the bottom of the circle
    bx, bw = 37, 55
    f.rect(bx, 96, bw, 23, ink=0)
    f.rect(bx + 1, 97, bw - 2, 21, ink=1)
    x0 = bx + 3
    f.text(x0, 99, T, lit="HR", ink=0)
    f.text(x0 + 21, 99, T, HR, F_INT, align=RIGHT, ink=0)
    f.text(x0 + 27, 99, T, lit="MSG", ink=0)
    f.text(bx + bw - 3, 99, T, NOTIFS, F_INT, align=RIGHT, ink=0)
    f.text(x0, 106, T, lit="PWR", ink=0)
    f.text(x0 + 21, 106, T, BATTERY, F_INT, align=RIGHT, ink=0)
    f.text(x0 + 22, 106, T, lit="%", ink=0)
    f.text(x0, 113, T, lit="STEP", ink=0)
    f.text(bx + bw - 3, 113, T, STEPS, F_INT, align=RIGHT, ink=0)
    # rain inverts whatever it crosses
    f.image(art.streaks(128, 128, 55, 12, (3, 6), 0.3), 0, 0, M_XOR)
    f.inset(SECOND, F_ZPAD)
    return f


# ============================================================ CYBERPUNK: glitch
def glitch_frame(seed, y0=14, y1=96):
    """Tears land on rows y0..y1; dead pixel blocks on y0..y1+4."""
    rnd = random.Random(seed)
    rows = [[0] * 128 for _ in range(128)]
    for _ in range(rnd.randint(5, 9)):
        y = rnd.randrange(y0, y1)
        h = rnd.choice([1, 1, 2, 3, 5])
        x0 = rnd.randrange(0, 100)
        x1 = min(128, x0 + rnd.randint(12, 70))
        for yy in range(y, min(128, y + h)):
            for xx in range(x0, x1):
                rows[yy][xx] = 1
    for _ in range(rnd.randint(2, 4)):  # dead pixel blocks
        x, y = rnd.randrange(0, 120), rnd.randrange(y0, y1 + 4)
        for yy in range(y, y + 3):
            for xx in range(x, x + rnd.randint(2, 6)):
                if xx < 128:
                    rows[yy][xx] = 1
    return Bitmap(128, 128, rows)


def glitch():
    f = Face("GLITCH")
    f.strings(S_WEEKDAY, WEEKDAYS)
    f.strings(S_MONTH, MONTHS)
    heavy = Font.from_ttf(WIN + "impact.ttf", 41, "0123456789:", squash=0.78, spacing=2, fixed_digits=True)
    # header stacked in the chord left of the inset
    f.text(38, 13, T, WEEKDAY, F_NAME)
    f.text(52, 13, T, DAY, F_ZPAD, pad=2)
    f.text(62, 13, T, MONTH, F_NAME)
    f.text(16, 25, T, lit="SIGNAL//CORRUPT")
    f.text(10, 39, T, lit="DEC> HR STEP PWR")
    # time drawn twice: solid, then an inverted copy slipped 3 px right (split-channel ghost),
    # both below the inset
    clock(f, heavy, 62, 52)
    clock(f, heavy, 65, 50, ink=INVERT)
    # a new tear pattern every minute
    # tears stay on the clock band so the header keeps reading
    f.frames([glitch_frame(i, 47, 88) for i in range(8)], 0, 0, MINUTE, 0, 8, M_XOR, wrap=True)
    f.rect(0, 93, 128, 35, ink=1, pat=4)
    f.rect(2, 95, 124, 33, ink=0)
    # hex readouts: two cells on the wide row, one per row below as the circle narrows
    def cell(x, y, lab, src, pad):
        f.text(x, y + 1, T, lit=lab)
        f.text(x + 13, y, S, lit="0X")
        f.text(x + 25, y, S, src, F_HEX, pad=pad)
    cell(25, 97, "ERR", HR, 2)
    cell(67, 97, "SEG", BATTERY, 2)
    cell(40, 106, "PID", STEPS, 4)
    cell(46, 115, "MSG", NOTIFS, 2)
    f.inset(HR, F_INT)
    return f


# ============================================================ CYBERPUNK: netrunner hexdump
def netrun():
    f = Face("NETRUN")
    black(f)
    rnd = random.Random(77)
    mid = font5x7(cell=3, dot=2, spacing=1)
    f.text(30, 12, T, lit="NETRUN 2.077", ink=0)
    f.text(24, 20, T, lit="ICE BREACH", ink=0)
    f.text(72, 20, T, STEP_PCT, F_INT, align=RIGHT, ink=0)
    f.text(73, 20, T, lit="%", ink=0)
    f.bar(16, 29, 60, 5, STEP_PCT, 0, 100, ink=0)
    box(f, 15, 28, 62, 7)
    clock(f, mid, 46, 38, ink=0, zpad=True)
    # hexdump seen through the round window: a fixed grid, and each row prints only the
    # cells that fit inside the bezel. Live values are spliced in and highlighted.
    live = {(0, 2): (HR, 2), (1, 4): (BATTERY, 2), (3, 0): (STEPS, 4), (5, 3): (HOUR24, 2),
            (5, 4): (MINUTE, 2), (6, 5): (NOTIFS, 2), (7, 1): (DAY, 2), (7, 2): (MONTH, 2)}
    ascii_ = "ABCDEFGHKMNPRSTUVXZ0123456789.:/-"
    skip = set()
    sep_rows = []
    for row in range(8):
        y = 62 + row * 7
        a, b = safe_span(y - 1, y + 6, side="wide")
        fits = lambda x, w: a <= x and x + w <= b
        if fits(4, 19):
            f.text(4, y, T, lit="%04X" % (0x1F40 + row * 6), ink=0)
            f.text(21, y, T, lit=":", ink=0)
        for col in range(6):
            if (row, col) in skip:
                continue
            x = 25 + col * 12
            if (row, col) in live:
                src, pad = live[(row, col)]
                fmt = F_HEX if src in (HR, BATTERY, STEPS, NOTIFS) else F_ZPAD
                w = 7 if pad == 2 else 19
                assert fits(x - 1, w + 2), (row, col)
                f.text(x, y, T, src, fmt, pad=pad, ink=0)
                f.rect(x - 1, y - 1, w + 2, 7, ink=INVERT)
                if pad == 4:
                    skip.add((row, col + 1))
            else:
                byte = "%02X" % rnd.randrange(256)
                if fits(x, 7):
                    f.text(x, y, T, lit=byte, ink=0)
        chars = "".join(rnd.choice(ascii_) for _ in range(8))
        n = 0
        while n < 8 and fits(98 + 4 * n, 3):
            n += 1
        if n:
            f.text(98, y, T, lit=chars[:n], ink=0)
            sep_rows.append(y)
    if sep_rows:
        f.rect(95, sep_rows[0] - 1, 1, sep_rows[-1] - sep_rows[0] + 7, ink=0, pat=1)
    f.inset(SECOND, F_ZPAD)
    return f


# ============================================================ ORIGINAL: shifting topo map
def value_noise(seed, cells=6):
    rnd = random.Random(seed)
    g = [[rnd.random() for _ in range(cells + 1)] for _ in range(cells + 1)]

    def n(x, y):
        fx, fy = x * cells, y * cells
        ix, iy = min(cells - 1, int(fx)), min(cells - 1, int(fy))
        tx, ty = fx - ix, fy - iy
        sx, sy = tx * tx * (3 - 2 * tx), ty * ty * (3 - 2 * ty)
        a = g[iy][ix] + (g[iy][ix + 1] - g[iy][ix]) * sx
        b = g[iy + 1][ix] + (g[iy + 1][ix + 1] - g[iy + 1][ix]) * sx
        return a + (b - a) * sy
    return n


def topo_frames(n_frames=6):
    na, nb, nc = value_noise(1, 4), value_noise(2, 4), value_noise(3, 9)
    out = []
    for fr in range(n_frames):
        th = 2 * math.pi * fr / n_frames
        h = [[0.0] * 128 for _ in range(128)]
        for y in range(128):
            for x in range(128):
                u, v = x / 128, y / 128
                h[y][x] = na(u, v) * math.cos(th) + nb(u, v) * math.sin(th) + 0.35 * nc(u, v)
        lv = lambda z: math.floor(z * 9)
        rows = [[0] * 128 for _ in range(128)]
        for y in range(127):
            for x in range(127):
                a, b, c = lv(h[y][x]), lv(h[y][x + 1]), lv(h[y + 1][x])
                if a != b or a != c:
                    rows[y][x] = 1
                    if max(a, b, c) % 5 == 0 and x + 1 < 128:  # index contour, thicker
                        rows[y + 1][x] = 1
        out.append(Bitmap(128, 128, rows))
    return out


def topo():
    f = Face("TOPO")
    f.strings(S_WEEKDAY, WEEKDAYS)
    f.frames(topo_frames(6), 0, 0, HOUR24, 0, 6, M_INK, wrap=True)
    big = Font.from_ttf(WIN + "bahnschrift.ttf", 24, "0123456789:", squash=0.85, spacing=2, fixed_digits=True)
    # "you are here": marker orbits by minute, in the open ground below the clock plate
    for r16, ink in ((7 * 16, 0), (5 * 16, 1), (3 * 16, 0), (1 * 16 + 8, 1)):
        f.dot(60, 92, 1, 17, r16, ink=ink)
    # map frame: graticule ticks just inside the bezel, skipping the arc under the inset
    f.ticks(64, 64, 52, 60, 19, hw=56, start=90, sweep=270, ink=0)   # clear a halo in the contours
    f.ticks(64, 64, 54, 60, 19, hw=16, start=90, sweep=270)
    # clock plate in the chord left of the inset
    f.rect(11, 34, 68, 32, ink=0)
    box(f, 11, 34, 68, 32)
    clock(f, big, 45, 36)
    f.text(15, 60, T, WEEKDAY, F_NAME)
    f.text(44, 60, T, lit="ELEV", )
    f.text(76, 60, T, ALTITUDE, F_INT, align=RIGHT)
    # readout box under the inset, on the wide part of the circle
    f.rect(84, 52, 38, 26, ink=0)
    box(f, 84, 52, 38, 26)
    f.text(87, 55, T, lit="STEP")
    f.text(119, 62, T, STEPS, F_COMPACT, align=RIGHT)
    f.text(87, 69, T, lit="HR")
    f.text(119, 69, T, HR, F_INT, align=RIGHT)
    f.inset(SECOND, F_ZPAD)
    return f


ALL = [ghost_dive, ghost_camo, neon_alley, glitch, netrun, topo]


def main(names=None):
    os.makedirs(OUT, exist_ok=True)
    for fn in ALL:
        face = fn()
        if names and face.name not in names:
            continue
        blob = face.build()
        with open(os.path.join(OUT, face.name + ".FACE"), "wb") as fh:
            fh.write(blob)
        cv, inset = render(blob, DEMO)
        preview(cv, os.path.join(OUT, face.name + "_preview.png"), inset)
        print("%-10s %6d bytes" % (face.name, len(blob)))


if __name__ == "__main__":
    main(sys.argv[1:])
