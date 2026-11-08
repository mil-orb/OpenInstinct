"""Music faces: now playing from the phone (Gadgetbridge music info), with
time, date, heart rate, battery and steps always on screen.
Run: python -X utf8 src/faces4.py"""
import os, sys, math
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
sys.path.insert(0, HERE)
from facekit import *
from facekit import render, preview
from pixfonts import font5x7, font3x5
from build_faces import clock, WEEKDAYS, MONTHS, with_degree, temp_readout, DEG
import art
from art import WIN, Bitmap
from faces2 import DEMO, box, black
from faces3 import patterned, outline
JP = WIN + "YuGothB.ttc"

OUT = os.path.join(HERE, "..", "out")
# with_degree adds the degree ring for temperature readouts. One shared copy per
# size keeps each face at one font table per size.
S = with_degree(font5x7())
T = with_degree(font3x5())
TXT = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789:'&-.,!?()/+ "

MUSIC = dict(DEMO)
MUSIC.update({MUSIC_TITLE: "We Are The End", MUSIC_ARTIST: "Alexisonfire",
              MUSIC_ALBUM: "Crisis", MUSIC_PLAYING: 1, MUSIC_POS: 15, MUSIC_DUR: 226})
IDLE = dict(DEMO)

# Layout: the round bezel hides the corners and the stock inset covers a circle
# at (105, 22) r 27, so readable content stays inside facekit.visible().
# Spans quoted below are facekit.safe_span() results. The top rows only have
# x 34..79; y 50..80 is the one band with nearly the full width.


def date_line(f, x, y, ink=1, font=None):
    """WEEKDAY DD MON: 40 px wide in the 3x5 font, 58 px in 5x7."""
    font = font or T
    f.text(x, y, font, WEEKDAY, F_NAME, ink=ink)
    f.text(x + font.width("SAT") + 4, y, font, DAY, F_ZPAD, pad=2, ink=ink)
    f.text(x + font.width("SAT 00") + 6, y, font, MONTH, F_NAME, ink=ink)


def readout(f, x, y, lab, src, fmt=F_INT, ink=1, gap=2):
    """3x5 label then a 5x7 value, the label sitting on the value's baseline."""
    f.text(x, y + 2, T, lit=lab, ink=ink)
    f.text(x + T.width(lab) + gap, y, S, src, fmt, ink=ink)


def cutout(f, x, y, w, lab, src, fmt=F_INT, ink_bg=1):
    """Cut-out box, 12 px tall: label left, value right."""
    fg = 1 - ink_bg
    f.rect(x, y, w, 12, ink=ink_bg)
    box(f, x, y, w, 12, ink=fg)
    f.text(x + 3, y + 4, T, lit=lab, ink=fg)
    f.text(x + w - 3, y + 3, S, src, fmt, align=RIGHT, ink=fg)


# ============================================================ panel: sound only
CLOCK_CX = 39


# The inset as seen through its ring on the watch: a disc
# centred (106, 21.5) in face pixels, radius about 23. Content stays within
# INSET_R of the centre so nothing touches the ring.
INSET_C = (106, 21.5)
INSET_R = 22
BIG_SIZE, BIG_Y = 27, 3
SMALL_SIZE, SMALL_Y = 21, 6
KANJI_Y = 31


def panel_temp_inset(f):
    """The inset when the watch's inset is set to weather temperature, in the
    show's title-card manner: a heavy condensed serif reading over the kanji
    気温 (air temperature), dark on a light disc (the face is light on dark,
    so the inset reads as a separate panel). Every reading is centred on the
    visible disc."""
    cx, cy = INSET_C
    small = with_degree(Font.from_ttf(WIN + "timesbd.ttf", SMALL_SIZE, "0123456789-", squash=0.6, spacing=1,
                                      fixed_digits=True, threshold=150))
    big = with_degree(Font.from_ttf(WIN + "timesbd.ttf", BIG_SIZE, "0123456789-", squash=0.58, spacing=1,
                                    fixed_digits=True, threshold=150))  # at 120 the thin strokes of 7 and 2 break
    # 気温 drawn by hand at 10 x 10: the TTF downsampled to this size loses
    # the water radical of 温
    ki = [".#........",
          ".#########",
          "#.........",
          "..#######.",
          "..........",
          "#######...",
          "......#...",
          ".#..#.#...",
          "..##..#..#",
          ".#..#..##."]
    on = ["#..#######",
          ".#.#.....#",
          "...#######",
          "#..#.....#",
          ".#.#######",
          "..........",
          ".#.#######",
          ".#.#.#.#.#",
          "#..#.#.#.#",
          "#.########"]
    kanji = Bitmap.from_strings([k + "." + o for k, o in zip(ki, on)])
    # light disc over the whole inset circle (105, 22) r 27, above the y 48
    # rule; the ring hides its edge
    disc = Bitmap.from_strings(["".join("#" if (x + .5 - 27) ** 2 + (y + .5 - 22) ** 2 <= 27.5 ** 2 else "."
                                        for x in range(55)) for y in range(47)])

    def reading(font, sample, y):
        # right edge for temp_readout so the digits plus the ring are centred
        wn, wd = font.width(sample), font.width(DEG)
        xr = round((2 * cx + 7 + wn - wd) / 2)
        temp_readout(f, xr, y, font, ink=1)

    f.own_inset(INSET_TEMP)
    with f.when(INSET_TYPE, C_EQ, INSET_TEMP):
        f.image(disc, 78, 0, mode=M_ERASE)
        with f.when(WX_TEMP, C_NONE):
            reading(big, "--", BIG_Y)
        for lo, hi, sample, font, y in ((-9, 0, "-8", big, BIG_Y), (0, 10, "8", big, BIG_Y),
                                         (10, 100, "88", big, BIG_Y), (-99, -9, "-88", small, SMALL_Y),
                                         (100, 200, "188", small, SMALL_Y)):
            with f.when(WX_TEMP, C_GE, lo):
                with f.when(WX_TEMP, C_LT, hi):
                    reading(font, sample, y)
        f.image(kanji, round(cx - kanji.w / 2), KANJI_Y, mode=M_INK)


def panel_sound():
    f = Face("SOUND")
    f.strings(S_WEEKDAY, WEEKDAYS)
    f.strings(S_MONTH, MONTHS)
    # Laid out for the measured visible circle (r ~70). Above y 50 the stock
    # inset clears everything right of x 77 (on the watch the steps value
    # at x 72..88, y 46 vanished), so the top block is the date and a tall clock
    # in x 10..77, and the status strip sits below y 50 where the row is free.
    seg = art.seg_font(9, 24, 3.0, slant=0.12)
    datef = Font.from_ttf(WIN + "bahnschrift.ttf", 10, "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 ", squash=0.7,
                          spacing=1)
    serif = Font.from_ttf(WIN + "timesbd.ttf", 11, TXT, squash=0.62, spacing=1, threshold=120)
    serif_big = Font.from_ttf(WIN + "timesbd.ttf", 16, TXT, squash=0.6, spacing=1, threshold=120)
    big = Font.from_ttf(WIN + "bahnschrift.ttf", 12, "0123456789-", squash=0.9, spacing=1, fixed_digits=True)
    black(f)
    panel_temp_inset(f)
    dx, dy = 18, 10                                  # y 10..19; at y 9 the W's corner sat past r 70
    f.text(dx, dy, datef, WEEKDAY, F_NAME, ink=0)
    f.text(dx + datef.width("WED") + 3, dy, datef, DAY, F_ZPAD, pad=2, ink=0)
    f.text(dx + datef.width("WED00") + 6, dy, datef, MONTH, F_NAME, ink=0)
    assert dx + datef.width("WED00") + 6 + datef.width("MMM") <= 78, "date runs into the inset"
    clock(f, seg, CLOCK_CX, 21, ink=0, zpad=True)    # y 21..45
    f.rect(0, 48, 128, 1, ink=0)
    # status strip, full width below the inset (y 51..57)
    # The inset already shows the temperature, so the strip carries steps,
    # left, clear of the inset ring (which reaches r ~31 on the watch: the
    # steps value at x 83..112 and the clock's last digit vanished there).
    f.text(8, 51, S, lit="STEP", ink=0)               # 5x7, y 51..57
    f.text(35, 51, S, STEPS, F_INT, ink=0)
    f.rect(0, 59, 128, 1, ink=0)
    # monolith (y 62..95)
    f.rect(12, 62, 24, 34, ink=0)
    f.rect(13, 63, 22, 32, ink=1)
    f.text(24, 70, T, lit="SOUND", align=CENTRE, ink=0)
    f.text(24, 77, T, lit="ONLY", align=CENTRE, ink=0)
    with f.when(MUSIC_TITLE):
        with f.when(MUSIC_PLAYING, C_Z):
            f.rect(19, 85, 3, 7, ink=0)
            f.rect(25, 85, 3, 7, ink=0)
        # serif title, two lines 12 px apart (11 px glyphs: the old 10 px step
        # ran line two into the artist), artist, then progress
        f.text(40, 61, serif, MUSIC_TITLE, F_STR, line=1, maxw=80, ink=0)
        f.text(40, 73, serif, MUSIC_TITLE, F_STR, line=2, maxw=80, last=True, ink=0)
        f.text(40, 85, S, MUSIC_ARTIST, F_STR, maxw=76, ink=0)
        f.text(40, 93, T, MUSIC_POS, F_MMSS, ink=0)
        f.rect(59, 95, 34, 1, ink=0, pat=1)
        f.bar(59, 94, 34, 3, MUSIC_PCT, 0, 100, ink=0)
        f.text(112, 93, T, MUSIC_DUR, F_MMSS, align=RIGHT, ink=0)
    with f.when(MUSIC_TITLE, C_Z):
        f.text(40, 64, serif_big, lit="NO SIGNAL", ink=0)
        f.text(40, 83, T, lit="AWAITING", ink=0)
        f.text(40, 90, T, lit="AUDIO LINK", ink=0)
    # hazard band with two big read-outs (y 101..116: x 20..108)
    f.rect(0, 99, 128, 29, ink=0, pat=7)
    for x, w, lab, src in ((19, 43, "HR", HR), (65, 44, "PWR", BATTERY)):
        f.rect(x, 101, w, 16, ink=1)
        box(f, x, 101, w, 16, ink=0)
        f.text(x + 3, 105, S, lit=lab, ink=0)       # 5x7, y 105..111
        f.text(x + w - 3, 103, big, src, F_INT, align=RIGHT, ink=0)
    f.inset(SECOND, F_ZPAD)
    return f


# ============================================================ panel: sound only, three speakers
def panel_sound3():
    f = Face("SOUND2")
    f.strings(S_WEEKDAY, WEEKDAYS)
    f.strings(S_MONTH, MONTHS)
    card = Font.from_ttf(WIN + "timesbd.ttf", 30, "0123456789:", squash=0.5, spacing=2, fixed_digits=True,
                         threshold=128)
    title = Font.from_ttf(WIN + "timesbd.ttf", 15, TXT, squash=0.58, spacing=1, threshold=120)
    small = Font.from_ttf(WIN + "timesbd.ttf", 8, TXT, squash=0.75, spacing=1, threshold=120)
    black(f)
    date_line(f, 38, 8, ink=0)                       # y 8..12: x 34..79
    clock(f, card, 51, 16, ink=0)                    # y 16..45: x 24..78
    f.rect(0, 48, 128, 1, ink=0)
    # four monoliths, numbered, each holding one reading, in the wide band (y 51..88: x 7..121).
    # Each has 24 px inside, too narrow for a step count like "23.4K", so the third shows goal percent.
    for i, (lab, src, fmt) in enumerate((("HR", HR, F_INT), ("PWR", BATTERY, F_INT),
                                         ("STEP%", STEP_PCT, F_INT), ("EXT", WX_TEMP, F_INT))):
        x = 7 + i * 29
        f.rect(x, 51, 27, 38, ink=0)
        f.rect(x + 1, 52, 25, 36, ink=1)
        f.text(x + 3, 53, small, lit="0%d" % (i + 1), ink=0)
        f.text(x + 23, 61, T, lit=lab, align=RIGHT, ink=0)
        if src == WX_TEMP:
            temp_readout(f, x + 24, 68, S, ink=0)
        else:
            f.text(x + 24, 68, S, src, fmt, align=RIGHT, maxw=24, ink=0)
        f.text(x + 14, 77, T, lit="SOUND", align=CENTRE, ink=0)
        f.text(x + 14, 83, T, lit="ONLY", align=CENTRE, ink=0)
    # now playing below them, centred on the narrowing chords
    with f.when(MUSIC_TITLE):
        f.text(64, 91, title, MUSIC_TITLE, F_STR, line=1, maxw=94, last=True, align=CENTRE, ink=0)
        f.text(64, 107, small, MUSIC_ARTIST, F_STR, maxw=68, align=CENTRE, ink=0)
        f.bar(39, 116, 50, 2, MUSIC_PCT, 0, 100, ink=0)
        with f.when(MUSIC_PLAYING):
            f.text(64, 120, T, MUSIC_POS, F_MMSS, align=CENTRE, ink=0)
        with f.when(MUSIC_PLAYING, C_Z):
            f.text(64, 120, T, lit="PAUSE", align=CENTRE, ink=0)
    with f.when(MUSIC_TITLE, C_Z):
        f.text(64, 92, title, lit="NO SIGNAL", align=CENTRE, ink=0)
        f.text(64, 108, small, lit="AWAITING LINK", align=CENTRE, ink=0)
        f.rect(39, 116, 50, 2, ink=0, pat=7)
    f.inset(SECOND, F_ZPAD)
    return f


# ============================================================ VINYL
REC = (50, 88)   # record centre; radius 28 keeps the whole record inside radius 62


def tonearm(cv, pct):
    """Arm from a pivot right of the record to a needle whose radius on the
    record shrinks as the track plays. pct None = parked."""
    px, py = 100, 59
    rcx, rcy = REC
    if pct is None:
        nx, ny = 84, 52
    else:
        r = 25 - 12 * pct
        a = math.radians(40 + 25 * pct)  # needle sits on the upper-right part of the record
        nx, ny = rcx + r * math.sin(a), rcy - r * math.cos(a)
    art.disc(cv, px, py, 4.5)
    art.disc(cv, px, py, 2, ink=0)
    hx, hy = nx + (px - nx) * 0.2, ny + (py - ny) * 0.2
    art.line(cv, px, py, hx, hy, 2.0)
    art.line(cv, hx, hy, nx, ny, 3.2)
    art.poly(cv, [(px + 3, py - 3), (px + 8, py - 4), (px + 9, py + 1), (px + 4, py + 3)])  # counterweight


def vinyl():
    f = Face("VINYL")
    f.strings(S_WEEKDAY, WEEKDAYS)
    f.strings(S_MONTH, MONTHS)
    big = Font.from_ttf(WIN + "bahnschrift.ttf", 20, "0123456789:", squash=0.85, spacing=2, fixed_digits=True)
    date_line(f, 38, 9)                              # y 9..13: x 34..79
    clock(f, big, 50, 17)                            # y 17..36: x 23..78
    cx, cy = REC
    rec = art.draw(128, 128, lambda cv: (
        art.disc(cv, cx, cy, 28, n=64),
        [art.ring(cv, cx, cy, r + 0.35, r - 0.35, ink=0, n=64, pat=3) for r in range(25, 13, -3)],
        art.disc(cv, cx, cy, 11, ink=0, n=40), art.disc(cv, cx, cy, 1.5, n=8)))
    f.image(rec, 0, 0, M_INK)
    # glare: a highlight wedge that moves round the record every minute
    glare = [art.draw(128, 128, lambda cv, k=k: art.ring(cv, cx, cy, 25, 13, ink=1, a0=k * 45, a1=k * 45 + 18,
                                                          n=64, pat=9)) for k in range(8)]
    f.frames(glare, 0, 0, MINUTE, 0, 8, M_XOR, wrap=True)
    with f.when(MUSIC_TITLE):
        f.text(8, 40, S, MUSIC_TITLE, F_STR, maxw=76)           # y 40..46: x 7..85
        f.text(8, 49, T, MUSIC_ARTIST, F_STR, maxw=76)
        f.text(cx, cy - 5, T, MUSIC_POS, F_MMSS, align=CENTRE)
        f.text(cx, cy + 2, T, lit="PLAY", align=CENTRE)
        f.frames([art.draw(128, 128, lambda cv, p=p: tonearm(cv, p / 9)) for p in range(10)], 0, 0,
                 MUSIC_PCT, 0, 101, M_INK)
    with f.when(MUSIC_TITLE, C_Z):
        f.text(8, 40, S, lit="SIDE A EMPTY")
        f.text(cx, cy - 2, T, lit="NO", align=CENTRE)
        f.image(art.draw(128, 128, lambda cv: tonearm(cv, None)), 0, 0, M_INK)
    # vitals column right of the record, right edges following the bezel
    for i, (lab, src, fmt, xr) in enumerate((("HR", HR, F_INT, 118), ("PWR", BATTERY, F_INT, 115),
                                             ("STEP", STEPS, F_COMPACT, 106))):
        y = 68 + i * 14
        f.text(xr, y, T, lit=lab, align=RIGHT)
        f.text(xr, y + 6, S, src, fmt, align=RIGHT)
    f.inset(SECOND, F_ZPAD)
    return f


# ============================================================ TAPE
def reels(p):
    """Cassette window art, p = 0..1 played. Tape moves from the left reel to the right."""
    def fn(cv):
        for cx, r in ((47, 8 - 4 * p), (81, 4 + 4 * p)):
            art.disc(cv, cx, 9, r, pat=1, n=32)            # tape pack
            art.ring(cv, cx, 9, 3.6, 2.4, n=20)            # hub
            for k in range(6):
                a = math.radians(60 * k)
                art.line(cv, cx + 1.2 * math.sin(a), 9 - 1.2 * math.cos(a), cx + 2.4 * math.sin(a),
                         9 - 2.4 * math.cos(a), 1)
        art.line(cv, 47, 9 + 8 - 4 * p, 81, 9 + 4 + 4 * p, 1)  # tape run across the bottom
    return art.draw(128, 18, fn)


def tape():
    f = Face("TAPE")
    f.strings(S_WEEKDAY, WEEKDAYS)
    f.strings(S_MONTH, MONTHS)
    big = Font.from_ttf(WIN + "bahnschrift.ttf", 20, "0123456789:", squash=0.85, spacing=2, fixed_digits=True)
    black(f)
    date_line(f, 38, 9, ink=0)                       # y 9..13: x 34..79
    clock(f, big, 50, 17, ink=0)                     # y 17..36: x 23..78
    # shell: 96 x 53 at (16, 49), the largest cassette that clears the bezel and the inset
    shell = art.draw(128, 128, lambda cv: (
        art.poly(cv, [(16, 49), (112, 49), (112, 102), (16, 102)]),
        art.poly(cv, [(18, 51), (110, 51), (110, 100), (18, 100)], ink=0),
        art.poly(cv, [(36, 100), (42, 91), (86, 91), (92, 100)]),
        art.poly(cv, [(38, 99), (43, 93), (85, 93), (90, 99)], ink=0)))
    f.image(shell, 0, 0, M_ERASE)
    f.rect(21, 53, 86, 16, ink=0)             # label
    f.rect(21, 68, 86, 1, ink=1, pat=1)
    f.rect(30, 71, 68, 19, ink=0)             # window frame
    f.rect(31, 72, 66, 17, ink=1)
    with f.when(MUSIC_TITLE):
        f.text(24, 55, S, MUSIC_TITLE, F_STR, maxw=80, ink=1)
        f.text(24, 62, T, MUSIC_ARTIST, F_STR, maxw=58, ink=1)
        f.text(105, 62, T, MUSIC_POS, F_MMSS, align=RIGHT, ink=1)
        f.frames([reels(k / 7) for k in range(8)], 0, 71, MUSIC_PCT, 0, 101, M_ERASE)
        with f.when(MUSIC_PLAYING):
            f.text(64, 94, T, lit="PLAY >", align=CENTRE, ink=0)
        with f.when(MUSIC_PLAYING, C_Z):
            f.text(64, 94, T, lit="PAUSE", align=CENTRE, ink=0)
    with f.when(MUSIC_TITLE, C_Z):
        f.text(64, 58, S, lit="NO TAPE", align=CENTRE, ink=1)
        f.image(reels(0), 0, 71, M_ERASE)
        f.text(64, 94, T, lit="STOP", align=CENTRE, ink=0)
    # vitals on the two chords under the shell (y 104..110: x 23..105, y 113..119: x 37..91)
    readout(f, 27, 104, "HR", HR, ink=0)
    readout(f, 61, 104, "PWR", BATTERY, ink=0)
    readout(f, 42, 113, "STEP", STEPS, F_COMPACT, ink=0)
    f.inset(SECOND, F_ZPAD)
    return f


# ============================================================ POSTER: the song title is the face
def poster():
    f = Face("POSTER")
    f.strings(S_WEEKDAY, WEEKDAYS)
    f.strings(S_MONTH, MONTHS)
    heavy = Font.from_ttf(WIN + "impact.ttf", 14, TXT, squash=0.8, spacing=1, threshold=120)
    huge = Font.from_ttf(WIN + "impact.ttf", 44, "0123456789:", squash=0.78, spacing=2, fixed_digits=True)
    mid = Font.from_ttf(WIN + "bahnschrift.ttf", 16, "0123456789:", squash=0.9, spacing=1, fixed_digits=True)
    with f.when(MUSIC_TITLE):
        clock(f, mid, 53, 12)                        # y 12..27: x 28..78
        date_line(f, 14, 31)                         # y 31..35: x 12..79
        f.rect(10, 39, 4, 4)
        f.text(17, 38, T, lit="NOW PLAYING")
        # three title lines, centred, each as wide as its chord allows
        for i, (y, w) in enumerate(((49, 116), (63, 116), (77, 106))):
            f.text(64, y, heavy, MUSIC_TITLE, F_STR, line=i + 1, maxw=w, last=(i == 2), align=CENTRE)
        f.text(64, 92, S, MUSIC_ARTIST, F_STR, maxw=96, align=CENTRE)
        f.bar(18, 100, 92, 2, MUSIC_PCT, 0, 100)
    with f.when(MUSIC_TITLE, C_Z):
        date_line(f, 18, 36, font=S)                 # y 36..42: x 8..83
        clock(f, huge, 64, 51)                       # y 51..94: x 11..117
        f.rect(18, 98, 92, 2)
    # vitals band: two centred chords (y 105..111: x 24..104, y 114..120: x 38..90)
    f.rect(0, 103, 128, 25)
    readout(f, 32, 105, "HR", HR, ink=0)
    readout(f, 66, 105, "PWR", BATTERY, ink=0)
    readout(f, 44, 114, "STEP", STEPS, F_COMPACT, ink=0)
    f.inset(SECOND, F_ZPAD)
    return f


ALL = [panel_sound, panel_sound3, vinyl, tape, poster]


def main():
    from PIL import Image, ImageDraw
    os.makedirs(OUT, exist_ok=True)
    tiles = []
    for fn in ALL:
        face = fn()
        blob = face.build()
        with open(os.path.join(OUT, face.name + ".FACE"), "wb") as fh:
            fh.write(blob)
        for tag, vals in (("playing", MUSIC), ("idle", IDLE)):
            cv, inset = render(blob, vals)
            im = preview(cv, os.path.join(OUT, "%s_%s.png" % (face.name, tag)), inset, scale=2)
            tiles.append((face.name + " " + tag, im))
        print("%-10s %6d bytes" % (face.name, len(blob)))
    g = 10
    sheet = Image.new("RGB", (len(ALL) * (256 + g) + g, 2 * (256 + 22) + g), (60, 60, 60))
    for i, (lab, im) in enumerate(tiles):
        col, row = i // 2, i % 2
        x, y = g + col * (256 + g), g + row * (256 + 22)
        sheet.paste(im, (x, y))
        ImageDraw.Draw(sheet).text((x + 2, y + 258), lab, fill=(230, 230, 230))
    sheet.save(os.path.join(OUT, "sheet_music.png"))


if __name__ == "__main__":
    main()
