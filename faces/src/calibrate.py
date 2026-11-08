"""CAL.FACE: measures how much of the 128x128 screen the round bezel shows.
Short arcs at radius 54..64 px around the screen centre, one angle per radius,
labelled just inside, in the three corners away from the inset window; the inset window's assumed outline (INSET_C,
INSET_R in facekit) is drawn with a cross at its centre. On the watch, the
outermost ring that is visible all the way round is the usable radius.
Run: python -X utf8 faces/src/calibrate.py"""
import os, sys, math
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..")); sys.path.insert(0, HERE)
from facekit import *
from facekit import render, preview, INSET_C, INSET_R
from pixfonts import font3x5

OUT = os.path.join(HERE, "..", "out")
RINGS = [54, 56, 58, 60, 62, 64]
QUADS = (135, 225, 315)                   # skip the top-right (inset) corner


def build():
    T = font3x5()
    rows = [[0] * 128 for _ in range(128)]
    arcs = [(q - 25 + i * 10, r) for q in QUADS for i, r in enumerate(RINGS)]
    for y in range(128):
        for x in range(128):
            d = math.hypot(x + 0.5 - 64, y + 0.5 - 64)
            ang = math.degrees(math.atan2(x + 0.5 - 64, 64 - (y + 0.5))) % 360
            for a, r in arcs:
                if abs(d - r) < 0.6 and abs((ang - a + 180) % 360 - 180) <= 3.5:
                    rows[y][x] = 1
            di = math.hypot(x + 0.5 - INSET_C[0], y + 0.5 - INSET_C[1])
            if abs(di - INSET_R) < 0.5:
                rows[y][x] = 1
    cx, cy = INSET_C
    for k in range(-3, 4):
        rows[cy][cx + k] = rows[cy + k][cx] = 1
    f = Face("CAL")
    f.image(Bitmap(128, 128, rows), 0, 0)
    for quad in QUADS:
        for i, r in enumerate(RINGS):
            a = math.radians(quad - 25 + i * 10)
            x = round(64 + (r - 9) * math.sin(a))
            y = round(64 - (r - 9) * math.cos(a))
            f.text(x, y - 2, T, lit=str(r), align=CENTRE)
    f.text(64, 42, T, lit="BEZEL CHECK", align=CENTRE)
    f.text(64, 50, T, HOUR, F_INT, align=RIGHT)
    f.text(65, 50, T, lit=":")
    f.text(68, 50, T, MINUTE, F_ZPAD, pad=2)
    f.text(64, 61, T, lit="LARGEST NUMBER WITH", align=CENTRE)
    f.text(64, 68, T, lit="ITS ARC FULLY VISIBLE", align=CENTRE)
    return f


CORNER_RINGS = [66, 70, 74, 78, 82, 86]   # the screen corner pixel is at r ~89


def build2():
    """CAL2.FACE: on the watch every CAL arc up to 64 was visible, so
    this one looks past 64. In each of the three free corners, six short ticks
    cross the diagonal at radius 66..86 (CORNER_RINGS). The wearer counts the
    ticks visible in each corner. A full 64 ring is the reference."""
    T = font3x5()
    rows = [[0] * 128 for _ in range(128)]
    for y in range(128):
        for x in range(128):
            px, py = x + 0.5 - 64, 64 - (y + 0.5)
            if abs(math.hypot(px, py) - 64) < 0.5:
                rows[y][x] = 1
            for q in QUADS:
                a = math.radians(q)
                ux, uy = math.sin(a), math.cos(a)           # along the diagonal
                along = px * ux + py * uy
                across = -px * uy + py * ux
                for r in CORNER_RINGS:
                    if abs(along - r) < 0.75 and abs(across) <= 3.5:
                        rows[y][x] = 1
    f = Face("CAL2")
    f.image(Bitmap(128, 128, rows), 0, 0)
    f.text(64, 40, T, lit="CORNER CHECK", align=CENTRE)
    f.text(64, 50, T, HOUR, F_INT, align=RIGHT)
    f.text(65, 50, T, lit=":")
    f.text(68, 50, T, MINUTE, F_ZPAD, pad=2)
    f.text(64, 61, T, lit="COUNT THE TICKS YOU", align=CENTRE)
    f.text(64, 68, T, lit="SEE IN EACH CORNER", align=CENTRE)
    f.text(64, 75, T, lit="1-6 = R66-86", align=CENTRE)
    return f


def main():
    for name, fn in (("CAL", build), ("CAL2", build2)):
        blob = fn().build()
        open(os.path.join(OUT, name + ".FACE"), "wb").write(blob)
        cv, inset = render(blob, SAMPLE)
        preview(cv, os.path.join(OUT, name + "_preview.png"), inset, show_inset=False, bezel=False)
        print("%s %d bytes" % (name, len(blob)))


if __name__ == "__main__":
    main()
