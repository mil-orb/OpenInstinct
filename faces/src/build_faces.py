"""Build the test faces into faces/out: <NAME>.FACE plus a 4x preview PNG."""
import os, sys, random
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
sys.path.insert(0, HERE)
from facekit import *
from facekit import Canvas, fill_poly, render, preview, SAMPLE
from pixfonts import font5x7, font3x5

OUT = os.path.join(HERE, "..", "out")
WIN = "C:/Windows/Fonts/"
WEEKDAYS = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"]
WEEKDAYS_LONG = ["SUNDAY", "MONDAY", "TUESDAY", "WEDNESDAY", "THURSDAY", "FRIDAY", "SATURDAY"]
MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"]


def canvas_bitmap(w, h, draw):
    """Draw with the face renderer's own primitives into a w x h bitmap."""
    cv = Canvas()
    draw(cv)
    return Bitmap(w, h, [row[:w] for row in cv.px[:h]])


def clock(face, font, cx, y, ink=1, zpad=False, colon=":"):
    """hour right of a fixed colon, minutes left of it, centred on cx for '00:00'."""
    cw = font.width(colon)
    dw = font.width("00")
    total = dw * 2 + cw + font.spacing * 2
    x0 = cx - total // 2
    xc = x0 + dw + font.spacing
    face.text(xc - font.spacing, y, font, HOUR, F_ZPAD if zpad else F_INT, pad=2, align=RIGHT, ink=ink)
    face.text(xc, y, font, lit=colon, ink=ink)
    face.text(xc + cw + font.spacing, y, font, MINUTE, F_ZPAD, pad=2, ink=ink)


# Text ops carry ASCII only, so the degree ring is stored under the backtick.
DEG = "`"


def with_degree(font):
    """Copy of a pixel font with a 3x3 degree ring added on DEG. Use the copy for
    every text op in a face so the face carries one font table, not two."""
    h = font.height
    ring = [[1, 1, 1], [1, 0, 1], [1, 1, 1]] + [[0, 0, 0] for _ in range(h - 3)]
    glyphs = dict(font.glyphs)
    glyphs[DEG] = Bitmap(3, h, ring)
    return Font(h, glyphs, font.spacing)


def temp_readout(face, xr, y, font, src=WX_TEMP, ink=1):
    """Weather temperature with a degree ring, right edge at xr (exclusive).
    font must come from with_degree. No weather yet reads '--' plus the ring."""
    face.text(xr - 4, y, font, src, F_INT, align=RIGHT, ink=ink)
    face.text(xr - 3, y, font, lit=DEG, ink=ink)


# Layout note for every face below: the Instinct's round bezel hides the
# corners and the stock inset covers a circle at (105, 22) r 27. Readable
# content stays inside facekit.visible() (radius 62), so the top rows only
# have x 34..79 and the bottom rows narrow fast. Spans quoted in comments are
# facekit.safe_span() results.


# ---------------------------------------------------------------- plain test
def plain():
    f = Face("TEST")
    s = font5x7()
    t = font3x5()
    big = Font.from_ttf(WIN + "bahnschrift.ttf", 22, "0123456789:", squash=0.85, fixed_digits=True, spacing=2)
    f.strings(S_WEEKDAY, WEEKDAYS)
    f.strings(S_MONTH, MONTHS)
    f.strings(S_AMPM, ["AM", "PM"])
    # date on two short lines in the top chord (y 9..24: x 34..78)
    f.text(38, 9, s, WEEKDAY, F_NAME)
    f.text(60, 9, s, DAY, F_ZPAD, pad=2)
    f.text(38, 17, s, MONTH, F_NAME)
    f.text(60, 17, s, AMPM, F_NAME)
    # clock left of the inset (y 26..48: x 15..78), 59 px wide
    clock(f, big, 47, 26)
    f.rect(0, 49, 128, 1)
    # every source, two columns of six rows; y 52..99 is at least x 12..116 wide
    rows = [("HR", HR, F_INT), ("STEP", STEPS, F_INT), ("BAT%", BATTERY, F_INT), ("KCAL", CALORIES, F_INT),
            ("FLR", FLOORS, F_INT), ("INT", INTENSITY, F_INT), ("ALT", ALTITUDE, F_INT), ("TEMP", TEMP, F_INT),
            ("RISE", SUNRISE, F_CLOCK), ("SET", SUNSET, F_CLOCK), ("NOTE", NOTIFS, F_INT), ("SEC", SECOND, F_ZPAD)]
    for i, (lab, src, fmt) in enumerate(rows):
        x = 13 + (i % 2) * 54
        y = 52 + (i // 2) * 8
        f.text(x, y + 1, t, lit=lab)
        f.text(x + 49, y, s, src, fmt, pad=2, align=RIGHT)
    f.rect(64, 52, 1, 46, pat=1)
    f.bar(18, 101, 92, 3, STEP_PCT, 0, 100)
    f.rect(18, 105, 92, 1)
    # status flags on the y 107..113 chord (x 27..101)
    with f.when(BT):
        f.text(33, 108, s, lit="BT")
    with f.when(ALARM):
        f.text(49, 108, s, lit="ALM")
    with f.when(DND):
        f.text(71, 108, s, lit="DND")
    f.inset(SECOND, F_ZPAD)
    return f


# ---------------------------------------------------------------- title card
def hexagon(cv, cx16, cy16, r16, ink, pat=0):
    pts = []
    for k in range(6):
        d = 30 + 60 * k  # pointy-top hexagon
        pts.append((cx16 + (r16 * isin(d) >> 10), cy16 - (r16 * icos(d) >> 10)))
    fill_poly(cv, pts, ink, pat)


def title():
    f = Face("TITLE")
    serif_big = Font.from_ttf(WIN + "timesbd.ttf", 42, "0123456789:", squash=0.50, spacing=2,
                              fixed_digits=True, threshold=128)
    serif_ep = Font.from_ttf(WIN + "timesbd.ttf", 10, "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789:", squash=0.56,
                             spacing=1, threshold=120)
    serif_mid = Font.from_ttf(WIN + "timesbd.ttf", 11, "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789:", squash=0.62,
                              spacing=1, threshold=120)
    s = with_degree(font5x7())
    t = font3x5()
    f.strings(S_WEEKDAY, WEEKDAYS_LONG)
    f.strings(S_MONTH, MONTHS)

    # black field
    f.rect(0, 0, 128, 128, ink=1)

    # title card block, left of the inset: EPISODE:dd (y 14..23: x 28..78), weekday (y 26..36: x 15..78)
    ep_w = serif_ep.width("EPISODE:")
    f.text(29, 14, serif_ep, lit="EPISODE:", ink=0)
    f.text(29 + ep_w + 1, 14, serif_ep, DAY, F_INT, ink=0)
    f.text(17, 26, serif_mid, WEEKDAY, F_NAME, ink=0)

    # sync gauge: 7 hexes, lit by step progress (one per 1/7 of goal), zigzag on y 36..55
    hr16 = 6 * 16
    outline = canvas_bitmap(14, 14, lambda cv: (hexagon(cv, 7 * 16, 7 * 16, hr16, 1),
                                                hexagon(cv, 7 * 16, 7 * 16, hr16 - 24, 0)))
    solid = canvas_bitmap(14, 14, lambda cv: hexagon(cv, 7 * 16, 7 * 16, hr16, 1))
    for i in range(7):
        x = 5 + i * 11
        yy = 42 if i % 2 else 36
        f.image(outline, x, yy, M_ERASE)
        with f.when(STEP_PCT, C_GE, (i + 1) * 100 // 7):
            f.image(solid, x, yy, M_ERASE)

    # double rule
    f.rect(0, 57, 128, 2, ink=0)
    f.rect(0, 60, 128, 1, ink=0)

    # time, condensed heavy serif, in the widest band
    clock(f, serif_big, 64, 62, ink=0)

    # side readouts beside the clock: outside temperature left, sync percentage right
    f.text(5, 66, t, lit="EXT", ink=0)
    temp_readout(f, 27, 73, s, ink=0)
    f.text(123, 66, t, lit="SYNC", align=RIGHT, ink=0)
    f.text(123, 73, s, STEP_PCT, F_INT, align=RIGHT, ink=0)

    # hazard band with two cut-out readouts on the y 105..117 chord (x 31..97)
    f.rect(0, 104, 128, 1, ink=0)
    f.rect(0, 106, 128, 22, ink=0, pat=7)
    for x, lab, src, w in ((28, "HR", HR, 32), (62, "PWR", BATTERY, 37)):
        f.rect(x, 105, w, 12, ink=1)
        f.rect(x + 1, 106, w - 2, 10, ink=0)
        f.rect(x + 2, 107, w - 4, 8, ink=1)
        f.text(x + 3, 109, t, lit=lab, ink=0)
        f.text(x + w - 3, 108, s, src, F_INT, align=RIGHT, ink=0)
    # low battery: the PWR label turns into an inverted LOW tag
    with f.when(BATTERY, C_LT, 20):
        f.rect(64, 107, 14, 8, ink=0)
        f.text(65, 109, t, lit="LOW", ink=1)

    f.inset(SECOND, F_ZPAD)
    return f


# ---------------------------------------------------------------- blade-runner-style
def skyline(seed=2019):
    """Returns (silhouette, windows): draw the silhouette in ink, then erase windows."""
    rnd = random.Random(seed)
    W_, H_ = 128, 36
    sil = [[0] * W_ for _ in range(H_)]
    win = [[0] * W_ for _ in range(H_)]
    x = 0
    towers = []
    while x < W_:
        w = rnd.choice([5, 7, 7, 9, 11])
        h = rnd.randint(8, 30)
        towers.append((x, w, h))
        x += w + rnd.choice([3, 5, 8, 12])
    for x, w, h in towers:
        top = H_ - h
        for y in range(top, H_):
            for xx in range(x, min(W_, x + w)):
                sil[y][xx] = 1
        if w >= 9:  # stepped crown
            for xx in range(x + 2, min(W_, x + w - 2)):
                for y in range(top - 3, top):
                    sil[y][xx] = 1
        for y in range(top + 2, H_ - 2, 3):
            for xx in range(x + 1, min(W_, x + w - 1), 2):
                if rnd.random() < 0.3:
                    win[y][xx] = 1
        if h > 22:  # mast with a lit beacon
            ax = x + w // 2
            for y in range(max(0, top - 7), top):
                sil[y][ax] = 1
            win[max(0, top - 7)][ax] = 1
    return Bitmap(W_, H_, sil), Bitmap(W_, H_, win)


def rain(seed=7):
    rnd = random.Random(seed)
    W_, H_ = 128, 14
    rows = [[0] * W_ for _ in range(H_)]
    for _ in range(14):
        x, y = rnd.randrange(W_), rnd.randrange(H_)
        ln = rnd.choice([3, 4, 5])
        for k in range(ln):
            xx, yy = x - k // 2, y + k
            if 0 <= xx < W_ and 0 <= yy < H_:
                rows[yy][xx] = 1
    return Bitmap(W_, H_, rows)


def bracket(face, x, y, dx, dy, n=6, ink=0):
    face.rect(x if dx > 0 else x - n + 1, y, n, 1, ink=ink)
    face.rect(x, y if dy > 0 else y - n + 1, 1, n, ink=ink)


def blade():
    f = Face("BLADE")
    dots = font5x7(cell=4, dot=3, spacing=1)
    s = font5x7()
    t = font3x5()
    f.strings(S_WEEKDAY, WEEKDAYS)
    f.strings(S_MONTH, MONTHS)

    f.rect(0, 0, 128, 128, ink=1)

    # header readouts (3x5 terminal font), stepped down the left of the inset
    f.text(38, 9, t, lit="LA//", ink=0)                 # y 9..13: x 34..79
    f.text(57, 9, t, WEEKDAY, F_NAME, ink=0)
    f.text(32, 16, t, YEAR, F_INT, ink=0)               # y 16..20: x 24..78
    f.text(48, 16, t, lit=".", ink=0)
    f.text(50, 16, t, MONTH, F_ZPAD, pad=2, ink=0)
    f.text(58, 16, t, lit=".", ink=0)
    f.text(60, 16, t, DAY, F_ZPAD, pad=2, ink=0)
    f.text(24, 23, t, lit="ELEV", ink=0)                # y 23..27: x 17..78
    f.text(42, 23, t, ALTITUDE, F_INT, ink=0)
    f.rect(14, 30, 64, 1, ink=0, pat=1)
    f.text(13, 33, t, lit="UNIT STATUS", ink=0)         # y 33..37: x 10..81
    with f.when(BT):
        f.text(10, 40, t, lit="UPLINK OK", ink=0)       # y 40..44: x 7..85
    with f.when(BT, C_Z):
        f.text(10, 40, t, lit="UPLINK LOST", ink=0)

    # time in a bracketed frame, below the inset (y 50..77 is x 3..125)
    bracket(f, 6, 47, 1, 1)
    bracket(f, 121, 47, -1, 1)
    bracket(f, 6, 80, 1, -1)
    bracket(f, 121, 80, -1, -1)
    clock(f, dots, 64, 51, ink=0, zpad=True)

    # readout strip (y 84..96: x 11..117)
    f.text(14, 84, t, lit="PULSE", ink=0)
    f.text(58, 84, t, HR, F_ZPAD, pad=3, align=RIGHT, ink=0)
    f.text(14, 91, t, lit="STEPS", ink=0)
    f.text(58, 91, t, STEPS, F_ZPAD, pad=5, align=RIGHT, ink=0)
    f.text(66, 84, t, lit="CELL", ink=0)
    f.text(114, 84, t, BATTERY, F_INT, align=RIGHT, ink=0)
    f.text(66, 91, t, lit="MSGS", ink=0)
    f.text(114, 91, t, NOTIFS, F_ZPAD, pad=2, align=RIGHT, ink=0)
    f.rect(61, 84, 1, 12, ink=0, pat=1)

    # city: haze, skyline, rain. Its lower edge sits behind the bezel.
    sil, win = skyline()
    f.rect(0, 100, 128, 4, ink=0, pat=9)
    f.rect(0, 104, 128, 6, ink=0, pat=2)
    f.rect(0, 110, 128, 6, ink=0, pat=3)
    f.rect(0, 116, 128, 7, ink=0, pat=1)
    f.rect(0, 123, 128, 5, ink=0, pat=10)
    f.image(sil, 0, 97, M_INK)
    f.image(win, 0, 97, M_ERASE)
    f.image(rain(), 0, 99, M_ERASE)
    # step progress as a street-level light strip on the y 117..119 chord (x 38..90)
    f.rect(38, 116, 52, 4, ink=1)
    f.bar(40, 117, 48, 2, STEP_PCT, 0, 100, ink=0)

    f.inset(HR, F_INT)
    return f


def main():
    os.makedirs(OUT, exist_ok=True)
    for fn in (plain, title, blade):
        face = fn()
        blob = face.build()
        path = os.path.join(OUT, face.name + ".FACE")
        with open(path, "wb") as fh:
            fh.write(blob)
        cv, inset = render(blob, SAMPLE)
        preview(cv, os.path.join(OUT, face.name + "_preview.png"), inset)
        print("%-6s %6d bytes  inset=%s" % (face.name, len(blob), inset))


if __name__ == "__main__":
    main()
