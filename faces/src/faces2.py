"""Second batch of faces: more warning-panel, bushido, high-tech and
time/weather-driven backgrounds. Run: python src/faces2.py"""
import os, sys, math, random
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
sys.path.insert(0, HERE)
from facekit import *
from facekit import render, preview, SAMPLE
from pixfonts import font5x7, font3x5
from build_faces import clock, WEEKDAYS, WEEKDAYS_LONG, MONTHS
import art
from art import WIN

OUT = os.path.join(HERE, "..", "out")
S = font5x7()
T = font3x5()
JP = WIN + "YuGothB.ttc"


def black(f):
    f.rect(0, 0, 128, 128, ink=1)


def box(f, x, y, w, h, ink=0):
    f.rect(x, y, w, 1, ink=ink)
    f.rect(x, y + h - 1, w, 1, ink=ink)
    f.rect(x, y, 1, h, ink=ink)
    f.rect(x + w - 1, y, 1, h, ink=ink)


# Degree mark for temperatures. The loader converts weather to the watch's own
# C/F setting, so faces draw a unit-free ring instead of a letter.
DEG = Bitmap.from_strings(["###", "#.#", "###"])


def deg(f, x, y, ink=1):
    f.image(DEG, x, y, M_INK if ink else M_ERASE)


# ============================================================ panel: power timer
# Layout note for every face below: the round bezel hides the corners and the
# stock inset covers a circle at (105, 22) r 27, so readable content stays
# inside facekit.visible() (radius 62). Rows 0..49 are usable only left of the
# inset, which is why headers sit in the x 20..77 pocket.
def panel_timer():
    f = Face("POWER")
    f.strings(S_WEEKDAY, WEEKDAYS)
    f.strings(S_MONTH, MONTHS)
    seg = art.seg_font(17, 36, 4.5, slant=0.12)
    black(f)
    # umbilical (phone link) vs internal power, lit box = active source
    for y, lab, cmp in ((8, "EXTERNAL", C_NZ), (18, "INTERNAL", C_Z)):
        box(f, 37, y, 37, 9)
        with f.when(BT, cmp):
            f.rect(38, y + 1, 35, 7, ink=0)
            f.text(55, y + 2, T, lit=lab, align=CENTRE, ink=1)
        with f.when(BT, C_Z if cmp == C_NZ else C_NZ):
            f.text(55, y + 2, T, lit=lab, align=CENTRE, ink=0)
    f.rect(25, 18, 10, 9, ink=0, pat=7)
    f.text(45, 30, T, lit="ACTIVE TIME LEFT", align=CENTRE, ink=0)
    # pilot readouts: heart rate and outside temperature (phone weather)
    f.text(14, 39, T, lit="HR", ink=0)
    f.text(34, 39, T, HR, F_INT, align=RIGHT, ink=0)
    f.text(44, 39, T, lit="TEMP", ink=0)
    f.text(73, 39, T, WX_TEMP, F_INT, align=RIGHT, ink=0)
    deg(f, 75, 39, ink=0)
    f.rect(8, 46, 80, 1, ink=0, pat=1)
    clock(f, seg, 64, 50, ink=0, zpad=True)
    # 20-cell power gauge
    for k in range(20):
        x = 14 + k * 5
        f.rect(x, 89, 4, 1, ink=0)
        with f.when(BATTERY, C_GE, 5 * (k + 1)):
            f.rect(x, 91, 4, 6, ink=0)
    f.rect(14, 99, 99, 1, ink=0)
    f.text(19, 101, T, lit="INTERNAL BATTERY", ink=0)
    f.text(110, 101, T, lit="%", align=RIGHT, ink=0)
    f.text(105, 101, T, BATTERY, F_INT, align=RIGHT, ink=0)
    f.rect(0, 108, 128, 1, ink=0)
    f.text(37, 111, S, WEEKDAY, F_NAME, ink=0)
    f.text(59, 111, S, DAY, F_ZPAD, pad=2, ink=0)
    f.text(75, 111, S, MONTH, F_NAME, ink=0)
    with f.when(BATTERY, C_LT, 15):
        f.rect(0, 109, 128, 19, ink=0, pat=7)
        f.rect(30, 110, 68, 10, ink=1)
        f.text(64, 112, S, lit="POWER LOW", align=CENTRE, ink=0)
    f.inset(SECOND, F_ZPAD)
    return f


# ============================================================ panel: three-way vote
def panel_vote():
    f = Face("VOTE")
    f.strings(S_WEEKDAY, WEEKDAYS_LONG)
    f.strings(S_MONTH, MONTHS)
    serif = Font.from_ttf(WIN + "timesbd.ttf", 30, "0123456789:", squash=0.5, spacing=2, fixed_digits=True)
    serif_s = Font.from_ttf(WIN + "timesbd.ttf", 11, "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789:", squash=0.75)
    ok = art.text_bitmap("承認", JP, 13)
    ng = art.text_bitmap("否決", JP, 13)
    black(f)
    clock(f, serif, 52, 15, ink=0)
    f.text(10, 46, serif_s, WEEKDAY, F_NAME, ink=0)
    f.rect(0, 58, 128, 1, ink=0)
    f.rect(0, 60, 128, 1, ink=0, pat=1)

    panels = [("HR", HR, C_LT, 100, F_INT), ("STEP", STEP_PCT, C_GE, 50, F_INT), ("PWR", BATTERY, C_GE, 30, F_INT)]
    w, h = 32, 33
    for i, (lab, src, cmp, val, fmt) in enumerate(panels):
        x0 = 12 + i * 36

        def shape(cv):
            art.poly(cv, [(4, 0), (w, 0), (w, h - 4), (w - 4, h), (0, h), (0, 4)])
        def shape_inner(cv):
            art.poly(cv, [(5, 1), (w - 1, 1), (w - 1, h - 5), (w - 5, h - 1), (1, h - 1), (1, 5)], ink=0)
        outline = art.draw(w + 1, h + 1, lambda cv: (shape(cv), shape_inner(cv)))
        solid = art.draw(w + 1, h + 1, shape)
        y0 = 62
        f.image(outline, x0, y0, M_ERASE)
        with f.when(src, cmp, val):
            f.image(solid, x0, y0, M_ERASE)
            f.text(x0 + 5, y0 + 3, T, lit=lab, ink=1)
            f.text(x0 + w - 4, y0 + 11, S, src, fmt, align=RIGHT, ink=1)
            f.image(ok, x0 + (w - ok.w) // 2 + 1, y0 + 20, M_INK)
        with f.when(src, C_LT if cmp == C_GE else C_GE, val):
            f.text(x0 + 5, y0 + 3, T, lit=lab, ink=0)
            f.text(x0 + w - 4, y0 + 11, S, src, fmt, align=RIGHT, ink=0)
            f.image(ng, x0 + (w - ng.w) // 2 + 1, y0 + 20, M_ERASE)
        # no reading (HR off the wrist): label and "--", no vote mark. A real 0
        # also lands here and draws the same pixels as the branch above.
        with f.when(src, C_Z):
            f.text(x0 + 5, y0 + 3, T, lit=lab, ink=0)
            f.text(x0 + w - 4, y0 + 11, S, src, fmt, align=RIGHT, ink=0)
        f.text(x0 + w // 2, 98, serif_s, lit=["I", "II", "III"][i], align=CENTRE, ink=0)
    f.text(64, 111, T, lit="BY MAJORITY VOTE", align=CENTRE, ink=0)
    f.text(44, 118, T, lit="TEMP", ink=0)
    f.text(73, 118, T, WX_TEMP, F_INT, align=RIGHT, ink=0)
    deg(f, 75, 118, ink=0)
    f.inset(DAY, F_INT)
    return f


# ============================================================ panel: sync ring
def panel_sync():
    f = Face("SYNC")
    seg = art.seg_font(8, 16, 2.4, slant=0.1)
    black(f)
    # ring centre sits down-left so the ring clears both the bezel and the inset
    cx, cy = 54, 74
    f.image(art.draw(128, 128, lambda cv: (art.ring(cv, cx, cy, 45, 44), art.ring(cv, cx, cy, 30, 29.3))), 0, 0, M_ERASE)
    f.ticks(cx, cy, 38, 43, 60, hw=10, src=STEP_PCT, maxv=100, ink=0, unlit=1)
    f.ticks(cx, cy, 32, 36, 12, hw=14, ink=0)
    # crosshair
    f.rect(cx - 26, cy, 15, 1, ink=0)
    f.rect(cx + 12, cy, 15, 1, ink=0)
    f.rect(cx, cy - 26, 1, 8, ink=0)
    f.rect(cx, cy + 19, 1, 8, ink=0)
    f.dot(cx, cy, 0, 34, 35, ink=0)          # hour: dot on the inner ring
    f.dot(cx, cy, 1, 41, 25, ink=1)          # minute: black notch cut into the lit ring
    f.dot(cx, cy, 1, 41, 12, ink=0)
    clock(f, seg, cx, cy - 8, ink=0, zpad=True)
    f.text(cx, cy + 11, T, lit="SYNC", align=CENTRE, ink=0)
    # pilot block in the pocket left of the inset
    f.text(38, 8, T, lit="TEMP", ink=0)
    f.text(70, 8, T, WX_TEMP, F_INT, align=RIGHT, ink=0)
    deg(f, 72, 8, ink=0)
    f.text(30, 15, S, lit="HR", ink=0)
    f.text(66, 15, S, HR, F_INT, align=RIGHT, ink=0)
    f.text(22, 23, T, lit="RATIO", ink=0)
    f.text(54, 23, T, STEP_PCT, F_INT, align=RIGHT, ink=0)
    f.text(55, 23, T, lit="%", ink=0)
    # battery column right of the ring, below the inset
    f.rect(101, 50, 16, 48, ink=0, pat=7)
    f.rect(103, 52, 12, 44, ink=1)
    f.bar(105, 54, 8, 40, BATTERY, 0, 100, D_UP, ink=0)
    f.text(108, 99, T, lit="E", align=CENTRE, ink=0)
    f.inset(SECOND, F_ZPAD)
    return f


# ============================================================ BUSHIDO: enso analog
def enso(cv, cx, cy, r, seed=3, a0=25, a1=365):
    rnd = random.Random(seed)
    n = 120
    outer, inner = [], []
    for k in range(n + 1):
        t = k / n
        a = math.radians(a0 + (a1 - a0) * t)
        th = 1.2 + 7.5 * math.sin(math.pi * min(1, t * 1.15)) ** 0.7 * (1 - 0.5 * t)
        wob = 0.8 * math.sin(t * 17 + seed)
        ro, ri = r + th / 2 + wob, r - th / 2 + wob
        outer.append((cx + ro * math.sin(a), cy - ro * math.cos(a)))
        inner.append((cx + ri * math.sin(a), cy - ri * math.cos(a)))
    art.poly(cv, outer + inner[::-1])
    # dry-brush streaks in the tail
    for k in range(40):
        t = rnd.uniform(0.55, 1.0)
        a = math.radians(a0 + (a1 - a0) * t)
        rr = r + rnd.uniform(-3, 3)
        x, y = int(cx + rr * math.sin(a)), int(cy - rr * math.cos(a))
        if 0 <= x < 128 and 0 <= y < 128:
            cv.px[y][x] = 0


def bushi_enso():
    f = Face("BU_ENSO")
    cx, cy = 52, 72
    # the brush gap (thin start and end of the stroke) faces the inset
    f.image(art.draw(128, 128, lambda cv: enso(cv, cx, cy, 40, a0=55, a1=395)), 0, 0, M_INK)
    hours = [art.text_bitmap(k, JP, 12) for k in "一二三四五六七八九十"] + \
            [art.text_bitmap("十一", JP, 12), art.text_bitmap("十二", JP, 12)]
    hours = [art.trim(h) for h in hours]
    # hour word in the centre-bottom: "十時" etc. drawn as frames (same size boxes)
    hw = max(h.w for h in hours)
    hh = max(h.h for h in hours)
    boxed = []
    for h in hours:
        rows = [[0] * hw for _ in range(hh)]
        ox, oy = (hw - h.w) // 2, (hh - h.h) // 2
        for y, r in enumerate(h.rows):
            rows[oy + y][ox:ox + h.w] = r
        boxed.append(art.Bitmap(hw, hh, rows))
    ji = art.text_bitmap("時", JP, 12)
    f.frames(boxed, cx - (hw + ji.w + 1) // 2, cy + 15, HOUR12, 1, 13)
    f.image(ji, cx - (hw + ji.w + 1) // 2 + hw + 1, cy + 15 + (hh - ji.h) // 2)
    # brush hands
    f.hand(cx, cy, 0, 22, 4, hw_base=40, hw_tip=6)
    f.hand(cx, cy, 1, 33, 6, hw_base=28, hw_tip=4)
    f.image(art.draw(9, 9, lambda cv: art.disc(cv, 4.5, 4.5, 4)), cx - 4, cy - 4)
    f.image(art.draw(5, 5, lambda cv: art.disc(cv, 2.5, 2.5, 1.6)), cx - 2, cy - 2, M_ERASE)
    # seal: weekday kanji reversed out of a red-seal square
    days = [art.text_bitmap(d, JP, 11) for d in "日月火水木金土"]
    dw = max(d.w for d in days); dh = max(d.h for d in days)
    seals = []
    for d in days:
        rows = [[1] * (dw + 6) for _ in range(dh + 6)]
        ox, oy = 3 + (dw - d.w) // 2, 3 + (dh - d.h) // 2
        for y, r in enumerate(d.rows):
            for x, v in enumerate(r):
                if v:
                    rows[oy + y][ox + x] = 0
        rows[0][0] = rows[0][-1] = rows[-1][0] = rows[-1][-1] = 0
        seals.append(art.Bitmap(dw + 6, dh + 6, rows))
    # right-hand column below the inset: vertical motto over the seal
    motto = art.vtext("武士道", JP, 10, gap=1)
    f.image(motto, 111, 52)
    f.frames(seals, 98, 84, WEEKDAY, 0, 7, M_OPAQUE)
    # day of month and moon phase above the enso
    moons = [art.moon_phase(5, (i + 0.5) * 29.53 / 15) for i in range(15)]
    f.frames(moons, 36, 12, MOON_AGE, 0, 30)
    f.text(51, 14, S, DAY, F_INT)
    f.image(art.text_bitmap("日", JP, 9), 64, 13)
    f.inset(STEPS, F_COMPACT)
    return f


# ============================================================ BUSHIDO: living landscape
def sky_layers():
    near = art.ridge(128, 108, 10, seed=11, rough=0.5)
    far = art.ridge(128, 92, 16, seed=4, rough=0.6)
    peak = lambda x: 62 + abs(x - 90) * 0.9  # one tall cone-shaped peak, snowcap
    def far_fn(cv):
        art.poly(cv, [(x, min(far[x], max(peak(x), 0))) for x in range(128)] + [(127, 128), (0, 128)])
    def snow(cv):
        art.poly(cv, [(90 - 14, 62 + 14 * 0.9), (90, 62), (90 + 14, 62 + 14 * 0.9),
                      (96, 78), (90, 74), (83, 79)])
    def near_fn(cv):
        art.poly(cv, [(x, near[x]) for x in range(128)] + [(128, near[127]), (128, 128), (0, 128)])
        # pine silhouettes on the near ridge
        rnd = random.Random(8)
        for x in range(4, 128, 9):
            if rnd.random() < 0.55:
                hgt = rnd.randint(6, 11)
                b = near[x] + 1
                art.poly(cv, [(x - 3, b), (x, b - hgt), (x + 3, b)])
        # torii gate on the right shoulder
        gx, gy = 30, near[30] + 1
        art.poly(cv, [(gx - 9, gy - 13), (gx + 9, gy - 13), (gx + 8, gy - 11), (gx - 8, gy - 11)])
        art.poly(cv, [(gx - 7, gy - 9), (gx + 7, gy - 9), (gx + 7, gy - 8), (gx - 7, gy - 8)])
        for px in (gx - 5, gx + 4):
            art.poly(cv, [(px, gy - 11), (px + 1.5, gy - 11), (px + 1.8, gy), (px - 0.3, gy)])
    nb = art.draw(128, 128, near_fn)
    rim = art.Bitmap(128, 128, [[1 if nb.rows[y][x] and (y == 0 or not nb.rows[y - 1][x]
                                  or (x > 0 and not nb.rows[y][x - 1]) or (x < 127 and not nb.rows[y][x + 1]))
                                  else 0 for x in range(128)] for y in range(128)])
    return art.draw(128, 128, far_fn), art.draw(128, 128, snow), nb, rim


def bushi_sky():
    f = Face("BU_SKY")
    f.strings(S_MONTH, MONTHS)
    far, snow, near, rim = sky_layers()
    big = Font.from_ttf(WIN + "bahnschrift.ttf", 19, "0123456789:", squash=0.85, spacing=2, fixed_digits=True)
    # sky by time of day: night = black with stars; dawn/dusk = banded glow
    with f.when(SUN_PROGRESS, C_GE, 1000):
        f.rect(0, 0, 128, 128, ink=1)
        f.image(art.stars(128, 90, 45, 5), 0, 0, M_ERASE)
    with f.when(SUN_PROGRESS, C_LT, 120):          # dawn
        for i, p in enumerate((9, 2, 3, 1)):
            f.rect(0, 50 + i * 12, 128, 12, ink=1, pat=p)
    with f.when(SUN_PROGRESS, C_LT, 1000):
        with f.when(SUN_PROGRESS, C_GE, 880):      # dusk
            for i, p in enumerate((9, 2, 3, 1, 10)):
                f.rect(0, 40 + i * 12, 128, 12, ink=1, pat=p)
    # sun (day) and moon (night) ride the same arc, rising left, setting right;
    # the arc is sized so the disc stays inside the bezel at both horizons
    with f.when(DAYLIGHT):
        with f.when(WX_COND, C_LT, WX_CLOUDY):
            f.dot(64, 96, SUN_PROGRESS, 38, 7 * 16, maxv=2000, start=270, ink=1)
    with f.when(DAYLIGHT, C_Z):
        f.dot(64, 96, SUN_PROGRESS, 38, 6 * 16, maxv=2000, start=90, ink=0)
        f.dot(64, 96, SUN_PROGRESS, 38, 5 * 16, maxv=2000, start=90 + 4, ink=1)
    # mountains: far range dithered (day: 50 % grey, night: moonlit specks), near ridge solid
    pat = lambda bm, p: art.Bitmap(bm.w, bm.h, [[v and (PATTERNS[p][y & 7] >> (7 - (x & 7))) & 1
                                                 for x, v in enumerate(r)] for y, r in enumerate(bm.rows)])
    f.image(far, 0, 0, M_INK)
    with f.when(DAYLIGHT):
        f.image(far, 0, 0, M_ERASE)
        f.image(pat(far, 1), 0, 0, M_INK)
    with f.when(DAYLIGHT, C_Z):
        f.image(pat(far, 2), 0, 0, M_ERASE)
    f.image(snow, 0, 0, M_ERASE)
    f.image(near, 0, 0, M_INK)
    with f.when(DAYLIGHT, C_Z):
        f.image(rim, 0, 0, M_ERASE)
    # weather overlays: black on the day sky, white on the night sky
    rain_bm = art.streaks(128, 100, 70, 3, (4, 7), 0.4)
    storm_bm = art.streaks(128, 100, 90, 4, (5, 8), 0.5)
    bolt_bm = art.draw(30, 40, lambda cv: art.bolt(cv, 8, 0, 2.2))
    snow_bm = art.flakes(128, 110, 40, 6)
    for day, mode in ((C_NZ, M_INK), (C_Z, M_ERASE)):
        with f.when(DAYLIGHT, day):
            with f.when(WX_COND, C_EQ, WX_RAIN):
                f.image(rain_bm, 0, 0, mode)
            with f.when(WX_COND, C_EQ, WX_STORM):
                f.image(storm_bm, 0, 0, mode)
                f.image(bolt_bm, 80, 50, mode)
            with f.when(WX_COND, C_EQ, WX_SNOW):
                f.image(snow_bm, 0, 0, mode)
    with f.when(WX_COND, C_EQ, WX_FOG):
        f.rect(0, 70, 128, 8, ink=0, pat=1)
        f.rect(0, 84, 128, 6, ink=0, pat=3)
    for cond, n in ((WX_PARTLY, 1), (WX_CLOUDY, 3)):
        with f.when(WX_COND, C_EQ, cond):
            for k in range(n):
                cl = art.draw(40, 20, lambda cv: (art.cloud(cv, 6, 12, 1.2, ink=1), art.cloud(cv, 7.5, 12.8, 1.05, ink=0)))
                cx_, cy_ = [(62, 50), (12, 56), (38, 64)][k]
                f.image(art.draw(40, 20, lambda cv: art.cloud(cv, 6, 12, 1.2)), cx_, cy_, M_ERASE)
                f.image(cl, cx_, cy_, M_INK)
    # time: white plate in the pocket left of the inset so it reads on any sky
    f.rect(20, 20, 58, 30, ink=0)
    box(f, 20, 20, 58, 30, ink=1)
    clock(f, big, 49, 22)
    f.text(24, 43, T, MONTH, F_NAME)
    f.text(38, 43, T, DAY, F_INT)
    deg(f, 72, 43)
    f.text(70, 43, T, WX_TEMP, F_INT, align=RIGHT)
    f.inset(HR, F_INT)
    return f


# ============================================================ BUSHIDO: katana
def katana(cv, x0, y, length):
    # blade: gentle curve, edge at the bottom, hamon line inside
    pts_top, pts_bot = [], []
    for k in range(41):
        t = k / 40
        x = x0 + 26 + t * (length - 26)
        curve = 3.0 * t * t
        w = 5.0 - 1.2 * t
        pts_top.append((x, y - curve))
        pts_bot.append((x, y + w - curve - (t ** 8) * 4))
    art.poly(cv, pts_top + pts_bot[::-1])
    # handle (tsuka) with diamond wrap, guard (tsuba)
    art.poly(cv, [(x0, y - 1), (x0 + 20, y - 1), (x0 + 20, y + 6), (x0, y + 6)])
    for k in range(5):
        xx = x0 + 2 + k * 4
        art.poly(cv, [(xx, y + 2.5), (xx + 2, y + 0.5), (xx + 4, y + 2.5), (xx + 2, y + 4.5)], ink=0)
    art.poly(cv, [(x0 + 21, y - 6), (x0 + 24, y - 6), (x0 + 24, y + 11), (x0 + 21, y + 11)])
    art.poly(cv, [(x0 - 2, y - 1.5), (x0 + 1, y - 1.5), (x0 + 1, y + 6.5), (x0 - 2, y + 6.5)])


def bushi_katana():
    f = Face("BU_KATANA")
    f.strings(S_WEEKDAY, WEEKDAYS)
    f.strings(S_MONTH, MONTHS)
    brush = Font.from_ttf(WIN + "impact.ttf", 40, "0123456789:", squash=0.78, spacing=2, fixed_digits=True)
    motto = art.vtext("武士道", JP, 15, gap=3)
    # black strip on the left edge; its text sits where the bezel leaves it wide
    f.rect(0, 0, 26, 128, ink=1)
    f.image(motto, 9, 38, M_ERASE)
    f.rect(9, 86, 14, 1, ink=0)
    f.text(17, 88, T, lit="HR", align=CENTRE, ink=0)
    f.text(18, 94, T, HR, F_INT, align=CENTRE, ink=0)
    # date in the pocket left of the inset
    f.text(34, 28, T, WEEKDAY, F_NAME)
    f.text(48, 26, S, DAY, F_ZPAD, pad=2)
    f.text(62, 28, T, MONTH, F_NAME)
    clock(f, brush, 73, 50)
    # blade = step progress: steel fills from the guard toward the tip
    blade = art.draw(128, 128, lambda cv: katana(cv, 30, 98, 80))
    f.image(blade, 0, 0, M_INK)
    f.bar(57, 94, 52, 10, STEP_PCT, 0, 100, D_RIGHT, ink=0, pat=1)
    f.text(34, 112, T, lit="STEPS", ink=1)
    f.text(94, 112, T, STEPS, F_INT, align=RIGHT, ink=1)
    f.inset(BATTERY, F_INT)
    return f


# ============================================================ TECH: fighter HUD
def tape(w, h, step_px, every, big_every, label_offset=0, mirror=False):
    """Vertical tick tape frames: n frames shifted by 1/n of a tick spacing."""
    n = 10
    out = []
    for fr in range(n):
        shift = fr * step_px / n
        def fn(cv, shift=shift):
            k = 0
            y = -step_px * 2 + shift
            while y < h + step_px:
                long = (k % big_every) == 0
                ln = 6 if long else 3
                x0 = 0 if not mirror else w - ln
                yy = int(round(y))
                if 0 <= yy < h:
                    for xx in range(x0, x0 + ln):
                        cv.px[yy][xx] = 1
                y += step_px
                k += 1
        out.append(art.draw(w, h, fn))
    return out


def tech_hud():
    f = Face("HUD")
    f.strings(S_WEEKDAY, WEEKDAYS)
    seg = art.seg_font(11, 20, 3, slant=0.0)
    black(f)
    # flight path marker + pitch ladder (fixed), time below the marker
    fy = 63
    hudc = art.draw(128, 128, lambda cv: (
        art.ring(cv, 64, fy, 5, 3.6),
        art.line(cv, 52, fy, 58.5, fy, 1.2), art.line(cv, 69.5, fy, 76, fy, 1.2), art.line(cv, 64, fy - 6.5, 64, fy - 10, 1.2),
        [art.line(cv, x0, y, x0 + 14, y, 1) for y in (48, 97) for x0 in (34, 80)],
        [art.line(cv, x, y, x, y + (4 if y < fy else -4), 1) for y in (48, 97) for x in (34, 94)],
    ))
    f.image(hudc, 0, 0, M_ERASE)
    clock(f, seg, 64, 72, ink=0, zpad=True)
    # left tape: heart rate, right tape: altitude; tick frames scroll with the value
    f.frames(tape(10, 44, 7, 1, 5), 12, 51, HR, 0, 5, M_ERASE, wrap=True)
    f.frames(tape(10, 44, 7, 1, 5, mirror=True), 106, 51, ALTITUDE, 0, 10, M_ERASE, wrap=True)
    for x, src, lab, al in ((24, HR, "HR", LEFT), (104, ALTITUDE, "ALT", RIGHT)):
        bx = x if al == LEFT else x - 22
        f.rect(bx, 59, 22, 11, ink=0)
        f.rect(bx + 1, 60, 20, 9, ink=1)
        f.text(bx + 11, 61, S, src, F_INT, align=CENTRE, ink=0)
        f.text(x, 53, T, lit=lab, align=al, ink=0)
    # header: day, then weather from the phone
    f.text(57, 10, T, WEEKDAY, F_NAME, align=CENTRE, ink=0)
    icons = art.weather_icons(16)
    f.frames(icons, 22, 22, WX_COND, 0, 8, M_ERASE)
    f.text(58, 23, S, WX_TEMP, F_INT, align=RIGHT, ink=0)
    deg(f, 60, 23, ink=0)
    f.text(41, 32, T, lit="H", ink=0)
    f.text(45, 32, T, WX_HI, F_INT, ink=0)
    f.text(57, 32, T, lit="L", ink=0)
    f.text(61, 32, T, WX_LO, F_INT, ink=0)
    f.text(16, 40, T, lit="RAIN", ink=0)
    f.text(44, 40, T, WX_PRECIP, F_INT, align=RIGHT, ink=0)
    f.text(46, 40, T, lit="%", ink=0)
    f.text(53, 40, T, lit="W", ink=0)
    f.text(57, 40, T, WX_WIND, F_INT, ink=0)
    # footer: steps bar, notifications and battery, phone link
    f.rect(16, 99, 96, 1, ink=0)
    f.text(20, 102, T, lit="STEP", ink=0)
    f.bar(37, 102, 48, 5, STEP_PCT, 0, 100, ink=0)
    box(f, 37, 102, 48, 5)
    f.text(88, 102, T, STEPS, F_COMPACT, ink=0)
    f.text(30, 110, T, lit="MSG", ink=0)
    f.text(44, 110, T, NOTIFS, F_ZPAD, pad=2, ink=0)
    f.text(56, 110, T, lit="PWR", ink=0)
    f.text(70, 110, T, BATTERY, F_INT, ink=0)
    f.text(47, 117, T, lit="LINK", ink=0)
    with f.when(BT):
        f.text(66, 117, T, lit="ON", ink=0)
    with f.when(BT, C_Z):
        f.text(66, 117, T, lit="OFF", ink=0)
    f.inset(SECOND, F_ZPAD)
    return f


# ============================================================ TECH: orbit
def tech_orbit():
    f = Face("ORBIT")
    black(f)
    # system centre sits down-left so the outer orbit clears the bezel and the inset
    cx, cy = 54, 74
    f.image(art.stars(128, 128, 60, 9, 0.1), 0, 0, M_ERASE)
    rings = art.draw(128, 128, lambda cv: [art.ring(cv, cx, cy, r + 0.6, r - 0.6) for r in (39, 28, 17)])
    f.image(rings, 0, 0, M_ERASE)
    f.image(art.draw(128, 128, lambda cv: art.disc(cv, cx, cy, 7)), 0, 0, M_ERASE)  # star
    f.image(art.draw(128, 128, lambda cv: art.ring(cv, cx, cy, 9.5, 8.5)), 0, 0, M_ERASE)
    f.ticks(cx, cy, 41, 44, 30, hw=8, src=STEP_PCT, maxv=100, ink=0)
    f.dot(cx, cy, 0, 17, 3 * 16 + 8, ink=1)
    f.dot(cx, cy, 0, 17, 3 * 16, ink=0)          # hour planet
    f.dot(cx, cy, 1, 28, 4 * 16 + 8, ink=1)
    f.dot(cx, cy, 1, 28, 4 * 16, ink=0)          # minute planet
    f.dot(cx, cy, 1, 28, 2 * 16, ink=1)          # ...with a dark core (ringed planet look)
    f.dot(cx, cy, BATTERY, 39, 2 * 16 + 8, maxv=100, ink=0)  # battery moon
    f.text(56, 7, T, lit="ORBIT", align=CENTRE, ink=0)
    f.text(37, 14, S, HOUR, F_ZPAD, pad=2, ink=0)
    f.text(50, 14, S, lit=":", ink=0)
    f.text(54, 14, S, MINUTE, F_ZPAD, pad=2, ink=0)
    f.text(36, 23, T, lit="PWR", ink=0)
    f.text(50, 23, T, BATTERY, F_INT, ink=0)
    f.inset(HR, F_INT)
    return f


# ============================================================ TECH: phone uplink dashboard
def tech_uplink():
    f = Face("UPLINK")
    f.strings(S_WEEKDAY, WEEKDAYS)
    f.strings(S_MONTH, MONTHS)
    big = Font.from_ttf(WIN + "bahnschrift.ttf", 20, "0123456789:", squash=0.8, spacing=2, fixed_digits=True)
    # header pocket left of the inset: date, clock, vitals
    f.text(37, 9, T, WEEKDAY, F_NAME)
    f.text(51, 9, T, DAY, F_ZPAD, pad=2)
    f.text(61, 9, T, MONTH, F_NAME)
    clock(f, big, 50, 17)
    for x, lab, src in ((9, "HR", HR), (33, "PWR", BATTERY), (63, "MSG", NOTIFS)):
        f.text(x, 40, T, lit=lab)
        f.text(x + 4 * len(lab) + 2, 40, T, src, F_INT)
    f.rect(0, 50, 128, 1)
    # tile grid: weather | sun | moon
    icons = art.weather_icons(16)
    f.frames(icons, 6, 54, WX_COND, 0, 8)
    f.text(43, 55, S, WX_TEMP, F_INT, align=RIGHT)
    deg(f, 45, 55)
    f.text(33, 64, T, WX_LO, F_INT, align=RIGHT)
    f.text(35, 64, T, lit="/")
    f.text(39, 64, T, WX_HI, F_INT)
    f.text(7, 73, T, lit="RAIN")
    f.bar(25, 73, 22, 5, WX_PRECIP, 0, 100)
    box(f, 25, 73, 22, 5)
    f.rect(50, 52, 1, 31, pat=1)
    # sun arc with position marker
    arc = art.draw(128, 128, lambda cv: art.ring(cv, 70, 74, 16.5, 15.7, a0=270, a1=450, n=90))
    f.image(arc, 0, 0)
    f.rect(52, 74, 37, 1)
    with f.when(DAYLIGHT):
        f.dot(70, 74, SUN_PROGRESS, 16, 2 * 16 + 8, maxv=2000, start=270)
    f.text(52, 77, T, SUNRISE, F_CLOCK)
    f.text(89, 77, T, SUNSET, F_CLOCK, align=RIGHT)
    f.rect(91, 52, 1, 31, pat=1)
    moons = [art.moon_phase(6, (i + 0.5) * 29.53 / 15) for i in range(15)]
    f.frames(moons, 100, 55, MOON_AGE, 0, 30)
    f.text(106, 71, T, MOON_AGE, F_INT, align=CENTRE)
    f.rect(0, 84, 128, 1)
    # phone feed: two text lines + one number from a phone automation;
    # maxw keeps long strings inside the circle (they end in '..')
    f.text(14, 87, T, lit="FEED")
    f.text(114, 87, T, DATA0, F_INT, align=RIGHT)
    f.text(15, 94, S, STR0, F_STR, maxw=96)
    f.text(23, 103, S, STR0 + 1, F_STR, maxw=82)
    f.rect(20, 112, 88, 1)
    f.text(41, 116, T, lit="STEP")
    f.text(85, 115, S, STEPS, F_COMPACT, align=RIGHT)
    f.inset(SECOND, F_ZPAD)
    return f


ALL = [panel_timer, panel_vote, panel_sync, bushi_enso, bushi_sky, bushi_katana, tech_hud, tech_orbit, tech_uplink]

DEMO = dict(SAMPLE)
DEMO.update({DATA0: 4, STR0: "BUS 14 IN 6 MIN", STR0 + 1: "DENTIST 15:30"})


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
