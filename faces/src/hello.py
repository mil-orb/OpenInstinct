"""HELLO.FACE: the smallest useful face, a starting point for your own.

    python faces/src/hello.py      # writes faces/out/HELLO.FACE and a preview PNG

Uses only the bundled pixel fonts, so it builds on any OS. Copy it, rename
the face, change the ops, rebuild, look at the preview, then copy the .FACE
to the watch as GARMIN/FACES/ACTIVE.FACE.
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
sys.path.insert(0, HERE)
from facekit import (Face, SAMPLE, render, preview, HOUR, MINUTE, DAY, WEEKDAY, STEPS, STEP_PCT,
                     BATTERY, HR, S_WEEKDAY, F_INT, F_ZPAD, F_NAME, CENTRE, RIGHT, LEFT, D_RIGHT)
from pixfonts import font5x7, font3x5

OUT = os.path.join(HERE, "..", "out")
WEEKDAYS = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"]


def hello():
    f = Face("HELLO")
    f.strings(S_WEEKDAY, WEEKDAYS)          # names for F_NAME on WEEKDAY
    big = font5x7(cell=3, dot=2, spacing=2)  # 5x7 digits as 2 px dots on a 3 px grid
    small = font5x7()
    tiny = font3x5()

    # time: hour right-aligned before a fixed colon, minutes after it, the
    # whole "00:00" centred on x 64 (widths come from the font)
    dw, cw, sp = big.width("00"), big.width(":"), big.spacing
    xc = 64 - (2 * dw + cw + 2 * sp) // 2 + dw + sp
    f.text(xc - sp, 54, big, HOUR, F_INT, align=RIGHT)
    f.text(xc, 54, big, lit=":")
    f.text(xc + cw + sp, 54, big, MINUTE, F_ZPAD, pad=2)

    # day line above, left of centre so it stays clear of the inset porthole
    f.text(50, 38, small, WEEKDAY, F_NAME, align=RIGHT)
    f.text(55, 38, small, DAY, F_INT)

    # step progress bar and three small readouts below
    f.rect(24, 86, 80, 6)                   # outline: solid, then cut the inside
    f.rect(25, 87, 78, 4, ink=0)
    f.bar(25, 87, 78, 4, STEP_PCT, 0, 100, direction=D_RIGHT)
    f.text(24, 96, tiny, STEPS, F_INT, align=LEFT)
    f.text(64, 96, tiny, HR, F_INT, align=CENTRE)
    f.text(104, 96, tiny, BATTERY, F_INT, align=RIGHT)
    return f


def main():
    os.makedirs(OUT, exist_ok=True)
    face = hello()
    blob = face.build()
    path = os.path.join(OUT, face.name + ".FACE")
    with open(path, "wb") as fh:
        fh.write(blob)
    cv, inset = render(blob, SAMPLE)
    preview(cv, os.path.join(OUT, face.name + "_preview.png"), inset)
    print(path, len(blob), "bytes")


if __name__ == "__main__":
    main()
