"""Custom watch faces for the Garmin Instinct (128x128, 1 bit).

A face is one file (.FACE) the watch payload reads from 0:/GARMIN/FACES/.
This module holds the format, a builder, and the reference renderer. The
payload's renderer (payload/src/face.rs) is a line-for-line port: integer
maths only, same polygon rule, same pattern table, so preview == watch.

File layout (little endian)
  0  "IFC1"
  4  u16 version (1)
  6  u16 header length (16)
  8  u32 ops length
  12 u32 file length
  16 ops, then data (bitmaps, fonts, strings)

Each op: u8 code, u8 size (whole op, header included), fields. A renderer
skips codes it does not know, so new ops do not break old firmware.
Bitmaps: rows top to bottom, MSB = leftmost pixel, stride (w + 7) // 8.
Font block: u8 height, i8 spacing, u8 count, u8 0, then count x
  {u8 code, u8 width, u16 0, u32 offset}, glyph bitmaps elsewhere in data.
Geometry for hands and ticks: points in 1/16 px, (cx, cy) is a pixel corner,
so (64, 64) is the exact centre of the screen.
"""
import math
import struct
from contextlib import contextmanager

W = H = 128

# ---- ops ------------------------------------------------------------------
OP_RECT, OP_BITMAP, OP_TEXT, OP_HAND, OP_TICKS, OP_BAR, OP_IF, OP_STRINGS, OP_INSET, OP_FRAMES, OP_DOT, OP_OWN = range(1, 13)

# ---- value sources ----------------------------------------------------------
NONE = 0
HOUR, HOUR24, HOUR12, MINUTE, SECOND, AMPM, DAY, MONTH, YEAR, WEEKDAY = range(1, 11)
HR, STEPS, STEP_GOAL, STEP_PCT, BATTERY, CALORIES, FLOORS, INTENSITY, ALTITUDE, TEMP, \
    SUNRISE, SUNSET, NOTIFS, BT, ALARM, DND = range(20, 36)
IS24H = 40
# the watch's inset setting (face cfg[7]): 7 = weather temperature, 0xff = off
INSET_TYPE = 41
INSET_TEMP = 7
# derived on the watch from the values above (no firmware lookup needed)
MINUTE_OF_DAY = 11      # 0..1439
DAYLIGHT = 36           # 1 between sunrise and sunset
SUN_PROGRESS = 37       # 0..999 sunrise->sunset, 1000..1999 sunset->sunrise
MOON_AGE = 38           # days since new moon, 0..29
# weather pushed by Gadgetbridge
WX_COND, WX_TEMP, WX_HI, WX_LO, WX_PRECIP, WX_WIND = range(50, 56)
# phone data channel: values and short strings a phone automation sends
DATA0 = 70              # DATA0..DATA7 = 70..77 (integers)
STR0 = 80               # STR0..STR7 = 80..87 (text, shown with F_STR)
# music now playing, sent by Gadgetbridge
MUSIC_TITLE, MUSIC_ARTIST, MUSIC_ALBUM = 90, 91, 92   # strings (F_STR)
MUSIC_PLAYING, MUSIC_POS, MUSIC_DUR = 93, 94, 95      # 1/0, seconds, seconds
MUSIC_PCT = 96                                         # derived 0..100
# WX_COND values
WX_CLEAR, WX_PARTLY, WX_CLOUDY, WX_RAIN, WX_SNOW, WX_STORM, WX_FOG, WX_WINDY = range(8)

SOURCE_NAMES = {v: k for k, v in dict(
    HOUR=HOUR, HOUR24=HOUR24, HOUR12=HOUR12, MINUTE=MINUTE, SECOND=SECOND, AMPM=AMPM, DAY=DAY,
    MONTH=MONTH, YEAR=YEAR, WEEKDAY=WEEKDAY, HR=HR, STEPS=STEPS, STEP_GOAL=STEP_GOAL,
    STEP_PCT=STEP_PCT, BATTERY=BATTERY, CALORIES=CALORIES, FLOORS=FLOORS, INTENSITY=INTENSITY,
    ALTITUDE=ALTITUDE, TEMP=TEMP, SUNRISE=SUNRISE, SUNSET=SUNSET, NOTIFS=NOTIFS, BT=BT,
    ALARM=ALARM, DND=DND, IS24H=IS24H, INSET_TYPE=INSET_TYPE, MINUTE_OF_DAY=MINUTE_OF_DAY, DAYLIGHT=DAYLIGHT,
    SUN_PROGRESS=SUN_PROGRESS, MOON_AGE=MOON_AGE, WX_COND=WX_COND, WX_TEMP=WX_TEMP, WX_HI=WX_HI,
    WX_LO=WX_LO, WX_PRECIP=WX_PRECIP, WX_WIND=WX_WIND).items()}

# text formats
F_INT, F_ZPAD, F_CLOCK, F_NAME, F_COMPACT, F_LIT, F_SPAD, F_STR, F_HEX, F_MMSS = range(10)
# string tables
S_WEEKDAY, S_MONTH, S_AMPM = range(3)
NAME_TABLE = {WEEKDAY: S_WEEKDAY, MONTH: S_MONTH, AMPM: S_AMPM}
# IF comparisons
C_NZ, C_Z, C_GE, C_LT, C_EQ, C_NONE = range(6)  # C_NONE: no number (missing)
# bitmap modes
M_INK, M_OPAQUE, M_ERASE, M_XOR = range(4)
# ink values: 0 background, 1 foreground, 2 invert (XOR)
INVERT = 2
# bar directions
D_RIGHT, D_LEFT, D_UP, D_DOWN = range(4)
# alignment
LEFT, CENTRE, RIGHT = range(3)

# 8x8 patterns, one byte per row, MSB = x % 8 == 0. Index 0 = solid.
PATTERNS = [
    [0xff] * 8,                                              # 0 solid
    [0xaa, 0x55] * 4,                                        # 1 50 % checker
    [0x88, 0x00, 0x22, 0x00] * 2,                            # 2 12.5 % dots
    [0xaa, 0x00] * 4,                                        # 3 25 % dots
    [0xff, 0x00] * 4,                                        # 4 scanlines, every 2nd row
    [0xff, 0x00, 0x00] * 2 + [0xff, 0x00],                   # 5 scanlines, every 3rd row (approx)
    [0x11, 0x22, 0x44, 0x88] * 2,                            # 6 thin diagonal /
    [0x0f, 0x1e, 0x3c, 0x78, 0xf0, 0xe1, 0xc3, 0x87],        # 7 hazard stripes (wide /)
    [0xaa] * 8,                                              # 8 vertical lines
    [0x80, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00],        # 9 sparse dots
    [0x77, 0xdd] * 4,                                        # 10 75 %
]

# sin(d) * 1024 for d = 0..90, rounded. Copied verbatim into the payload.
SIN = [round(math.sin(math.radians(d)) * 1024) for d in range(91)]


def isin(d):
    d %= 360
    if d <= 90:
        return SIN[d]
    if d <= 180:
        return SIN[180 - d]
    if d <= 270:
        return -SIN[d - 180]
    return -SIN[360 - d]


def icos(d):
    return isin(d + 90)


def floordiv(a, b):
    return a // b  # Python floors; the payload uses div_euclid with b > 0


# ---- bitmaps and fonts -------------------------------------------------------

class Bitmap:
    """1-bit image: rows of 0/1, w x h."""

    def __init__(self, w, h, rows):
        self.w, self.h, self.rows = w, h, rows

    @classmethod
    def from_image(cls, img, threshold=128, invert=False):
        """PIL image; dark pixels become ink (1) unless invert."""
        img = img.convert("L")
        w, h = img.size
        px = img.load()
        rows = [[1 if ((px[x, y] < threshold) != invert) else 0 for x in range(w)] for y in range(h)]
        return cls(w, h, rows)

    @classmethod
    def from_strings(cls, lines, on="#"):
        w = max(len(l) for l in lines)
        return cls(w, len(lines), [[1 if (x < len(l) and l[x] in on) else 0 for x in range(w)] for l in lines])

    def pack(self):
        stride = (self.w + 7) // 8
        out = bytearray()
        for r in self.rows:
            row = bytearray(stride)
            for x, v in enumerate(r):
                if v:
                    row[x >> 3] |= 0x80 >> (x & 7)
            out += row
        return bytes(out)


class Font:
    def __init__(self, height, glyphs, spacing=1):
        self.height = height
        self.glyphs = glyphs  # {char: Bitmap}, all glyphs have this height
        self.spacing = spacing

    def width(self, s):
        gs = [self.glyphs[c] for c in s if c in self.glyphs]
        return sum(g.w for g in gs) + self.spacing * max(0, len(gs) - 1)

    @classmethod
    def from_ttf(cls, path, px, chars, squash=1.0, spacing=1, threshold=110, embolden=0, ss=4,
                 fixed_digits=False):
        """Rasterise glyphs from a TrueType font at px pixels tall (cap height),
        squash < 1 condenses horizontally. Supersampled then thresholded."""
        from PIL import Image, ImageDraw, ImageFont, ImageFilter
        f = ImageFont.truetype(path, px * ss * 13 // 10)
        # cap height from 'H' / digit bbox
        ref = [f.getbbox(c) for c in "0H" if f.getbbox(c)]
        top = min(b[1] for b in ref)
        bot = max(b[3] for b in ref)
        scale = px * ss / (bot - top)
        f = ImageFont.truetype(path, int(px * ss * 13 // 10 * scale))
        ref = [f.getbbox(c) for c in "0H"]
        top = min(b[1] for b in ref)
        bot = max(b[3] for b in ref)
        glyphs = {}
        for ch in chars:
            if ch == " ":
                glyphs[ch] = Bitmap(max(2, px // 4), px, [[0] * max(2, px // 4) for _ in range(px)])
                continue
            bb = f.getbbox(ch)
            gw = bb[2] - bb[0] + 4 * ss
            im = Image.new("L", (gw, bot - top + 4 * ss), 255)
            ImageDraw.Draw(im).text((2 * ss - bb[0], 2 * ss - top), ch, font=f, fill=0)
            if embolden:
                im = im.filter(ImageFilter.MinFilter(embolden * 2 + 1))
            tw = max(1, round(im.width * squash / ss))
            th = round(im.height / ss)
            im = im.resize((tw, th), Image.LANCZOS)
            bm = Bitmap.from_image(im, threshold)
            # vertical: crop the 2 px margin to exactly px rows (cap height box)
            rows = bm.rows[2:2 + px]
            while len(rows) < px:
                rows.append([0] * bm.w)
            # horizontal: trim empty columns
            cols = [x for x in range(bm.w) if any(r[x] for r in rows)]
            if cols:
                x0, x1 = cols[0], cols[-1] + 1
                rows = [r[x0:x1] for r in rows]
            glyphs[ch] = Bitmap(len(rows[0]), px, rows)
        if fixed_digits:
            dw = max(glyphs[c].w for c in "0123456789" if c in glyphs)
            for c in "0123456789":
                if c in glyphs:
                    g = glyphs[c]
                    lpad = (dw - g.w) // 2
                    glyphs[c] = Bitmap(dw, px, [[0] * lpad + r + [0] * (dw - g.w - lpad) for r in g.rows])
        return cls(px, glyphs, spacing)

    @classmethod
    def from_patterns(cls, pats, cell=1, dot=None, spacing=1):
        """pats: {char: ["#.#", ...]} all the same height. Each '#' becomes a
        cell x cell block; with dot < cell, a dot x dot square inside the cell
        (dot-matrix look)."""
        dot = dot or cell
        hgt = len(next(iter(pats.values())))
        glyphs = {}
        for ch, lines in pats.items():
            w = max(len(l) for l in lines) if lines else 1
            rows = [[0] * (w * cell) for _ in range(hgt * cell)]
            for y, l in enumerate(lines):
                for x, c in enumerate(l):
                    if c == "#":
                        for yy in range(dot):
                            for xx in range(dot):
                                rows[y * cell + yy][x * cell + xx] = 1
            # drop trailing gap column(s) of the dot grid
            gw = w * cell - (cell - dot)
            glyphs[ch] = Bitmap(gw, hgt * cell - (cell - dot), [r[:gw] for r in rows[:hgt * cell - (cell - dot)]])
        return cls(hgt * cell - (cell - dot), glyphs, spacing)


def crop_union(bms, x, y):
    """Trim all bitmaps to the box holding any set pixel; only valid for
    modes that draw set pixels alone (not M_OPAQUE)."""
    xs, ys = [], []
    for b in bms:
        for yy, r in enumerate(b.rows):
            if any(r):
                ys.append(yy)
                xs += [i for i, v in enumerate(r) if v][:1] + [max(i for i, v in enumerate(r) if v)]
    if not xs:
        return [Bitmap(1, 1, [[0]]) for _ in bms], x, y
    x0, x1, y0, y1 = min(xs), max(xs) + 1, min(ys), max(ys) + 1
    return [Bitmap(x1 - x0, y1 - y0, [r[x0:x1] for r in b.rows[y0:y1]]) for b in bms], x + x0, y + y0


# ---- builder -----------------------------------------------------------------

class Face:
    def __init__(self, name):
        self.name = name
        self.ops = []          # list of bytes, or ("if", placeholder index)
        self.data = bytearray()
        self._dedup = {}

    # data blob; offsets are from file start, patched in build()
    def _blob(self, b):
        if b in self._dedup:
            return self._dedup[b]
        off = len(self.data)
        self.data += b
        while len(self.data) % 4:
            self.data += b"\0"
        self._dedup[b] = off
        return off

    def _op(self, code, fmt, *vals, data_fields=()):
        body = struct.pack("<" + fmt, *vals)
        self.ops.append([code, body, data_fields])

    def rect(self, x, y, w, h, ink=1, pat=0):
        self._op(OP_RECT, "hhHHBB", x, y, w, h, ink, pat)

    def image(self, bm, x, y, mode=M_INK):
        if mode != M_OPAQUE:
            (bm,), x, y = crop_union([bm], x, y)
        off = self._blob(bm.pack())
        self._op(OP_BITMAP, "hhHHBBI", x, y, bm.w, bm.h, mode, 0, off, data_fields=(10,))

    def _font(self, font):
        key = ("font", id(font))
        if key in self._dedup:
            return self._dedup[key]
        codes = sorted(font.glyphs)
        table = bytearray(struct.pack("<BbBB", font.height, font.spacing, len(codes), 0))
        offs = [self._blob(font.glyphs[c].pack()) for c in codes]
        for c, o in zip(codes, offs):
            table += struct.pack("<BBHI", ord(c), font.glyphs[c].w, 0, o)
        off = len(self.data)
        self.data += table
        while len(self.data) % 4:
            self.data += b"\0"
        self._dedup[key] = off
        # glyph offsets inside the table are data-relative too; record for patching
        self._fixups = getattr(self, "_fixups", []) + [off + 4 + 8 * i + 4 for i in range(len(codes))]
        return off

    def text(self, x, y, font, src=NONE, fmt=F_INT, pad=0, align=LEFT, ink=1, lit=None, maxw=0, line=0,
             last=False):
        """maxw: clip to this many px (even), ending in '..'. For F_STR, line=k
        shows the k-th line of the word-wrapped string; last=True ends it in
        '..' when more text follows."""
        f = self._font(font)
        lo = self._blob(lit.encode("ascii") + b"\0") + 1 if lit is not None else 0
        if lit is not None:
            fmt = F_LIT if src == NONE else fmt
        if fmt == F_STR:
            pad = line | (0x80 if last else 0)
        self._op(OP_TEXT, "hhBBBBBBII", x, y, align, src, fmt, pad, ink, min(255, maxw // 2), f, lo,
                 data_fields=(10, 14))

    def hand(self, cx, cy, src, length, tail=0, hw_base=24, hw_tip=8, ink=1):
        """half widths in 1/16 px."""
        self._op(OP_HAND, "hhBBBBBBBB", cx, cy, src, ink, length, tail, hw_base, hw_tip, 0, 0)

    def ticks(self, cx, cy, r_in, r_out, n, hw=8, start=0, sweep=360, src=NONE, maxv=0, ink=1, unlit=0):
        """n ticks spread over sweep degrees (sweep 360: n slots around; else
        n ticks with the last at start + sweep). With src, value * n / maxv
        ticks are drawn lit; unlit=1 draws the rest as thin hairlines."""
        self._op(OP_TICKS, "hhBBBBhhBBHBB", cx, cy, r_in, r_out, n, hw, start, sweep, src, ink, maxv, unlit, 0)

    def bar(self, x, y, w, h, src, lo, hi, direction=D_RIGHT, ink=1, pat=0):
        self._op(OP_BAR, "hhHHBBhhBB", x, y, w, h, src, direction, lo, hi, ink, pat)

    @contextmanager
    def when(self, src, cmp=C_NZ, val=0):
        i = len(self.ops)
        self._op(OP_IF, "BBhH", src, cmp, val, 0)
        yield
        skip = sum(2 + len(o[1]) for o in self.ops[i + 1:])
        self.ops[i][1] = struct.pack("<BBhH", src, cmp, val, skip)

    def strings(self, kind, names):
        off = self._blob(b"".join(n.encode("ascii") + b"\0" for n in names))
        self._op(OP_STRINGS, "BBI", kind, len(names), off, data_fields=(2,))

    def frames(self, bms, x, y, src, lo, hi, mode=M_INK, wrap=False):
        """Pick one of n same-size bitmaps by value: (v - lo) * n // (hi - lo)."""
        assert 1 <= len(bms) <= 59
        if mode != M_OPAQUE:
            bms, x, y = crop_union(bms, x, y)
        w, h = bms[0].w, bms[0].h
        offs = [self._blob(b.pack()) for b in bms]
        self._op(OP_FRAMES, "hhHHBBhhBB" + "I" * len(offs), x, y, w, h, mode, src, lo, hi, len(offs), 1 if wrap else 0, *offs,
                 data_fields=tuple(16 + 4 * i for i in range(len(offs))))

    def dot(self, cx, cy, src, radius, r16, maxv=0, ink=1, start=0):
        """Filled dot of radius r16 (1/16 px) orbiting (cx, cy) at radius px.
        src 0/1/2 = hour/minute/second hand angle, else value * 360 // maxv."""
        self._op(OP_DOT, "hhBBBBHh", cx, cy, src, ink, radius, r16, maxv, start)

    def inset(self, src, fmt=F_INT, icon=0xffff, lit=None):
        lo = self._blob(lit.encode("ascii") + b"\0") + 1 if lit is not None else 0
        self._op(OP_INSET, "BBHI", src, fmt, icon, lo, data_fields=(4,))

    def own_inset(self, *types):
        """The face draws the inset circle itself when the watch's inset
        setting is one of `types` (INSET_TEMP, ...): the stock inset painter is
        told to draw nothing and the loader blits the face inside the circle
        too. Draw the content under f.when(INSET_TYPE, C_EQ, t). Other inset
        settings keep the stock inset."""
        mask = 0
        for t in types:
            assert 0 <= t < 32
            mask |= 1 << t
        self._op(OP_OWN, "I", mask)

    def build(self):
        ops = bytearray()
        for code, body, _ in self.ops:
            ops += bytes([code, 2 + len(body)]) + body
        base = 16 + len(ops)
        while base % 4:
            ops += b"\0"  # code 0 padding: renderer stops at op code 0
            base += 1
        # patch data-relative offsets to file offsets
        out = bytearray(ops)
        pos = 0
        for code, body, fields in self.ops:
            for f in fields:
                at = pos + 2 + f
                v = struct.unpack_from("<I", out, at)[0]
                lit_field = (code == OP_TEXT and f == 14) or (code == OP_INSET and f == 4)
                if lit_field:
                    if v == 0:
                        continue  # no literal
                    v -= 1  # literal offsets carry +1 so that 0 means none
                struct.pack_into("<I", out, at, v + base)
            pos += 2 + len(body)
        data = bytearray(self.data)
        for at in getattr(self, "_fixups", []):
            v = struct.unpack_from("<I", data, at)[0]
            struct.pack_into("<I", data, at, v + base)
        blob = bytes(out) + bytes(data)
        hdr = b"IFC1" + struct.pack("<HHII", 1, 16, len(out), 16 + len(blob))
        return hdr + blob


# ---- reference renderer ------------------------------------------------------

class Canvas:
    def __init__(self):
        self.px = [[0] * W for _ in range(H)]

    def span(self, y, x0, x1, ink, pat=0):
        if y < 0 or y >= H:
            return
        x0 = max(0, x0)
        x1 = min(W, x1)
        p = PATTERNS[pat] if 0 <= pat < len(PATTERNS) else PATTERNS[0]
        row = self.px[y]
        bits = p[y & 7]
        for x in range(x0, x1):
            if bits & (0x80 >> (x & 7)):
                row[x] = (row[x] ^ 1) if ink == 2 else ink


def fill_poly(cv, pts, ink, pat=0):
    """pts in 1/16 px. Pixel (x, y) is filled when its centre lies inside."""
    ys = [p[1] for p in pts]
    y0 = max(0, floordiv(min(ys), 16) - 1)
    y1 = min(H - 1, floordiv(max(ys), 16) + 1)
    n = len(pts)
    for y in range(y0, y1 + 1):
        sy = y * 16 + 8
        xs = []
        for i in range(n):
            ax, ay = pts[i]
            bx, by = pts[(i + 1) % n]
            if ay == by:
                continue
            if ay > by:
                ax, ay, bx, by = bx, by, ax, ay
            if ay <= sy < by:
                xs.append(ax + floordiv((sy - ay) * (bx - ax), by - ay))
        xs.sort()
        for i in range(0, len(xs) - 1, 2):
            xa, xb = xs[i], xs[i + 1]
            # first pixel whose centre >= xa, last whose centre < xb
            p0 = -floordiv(-(xa - 8), 16)
            p1 = -floordiv(-(xb - 8), 16)
            cv.span(y, p0, p1, ink, pat)


def thick_line(cv, x0, y0, x1, y1, deg, hw, ink):
    """Quad from (x0,y0) to (x1,y1), both in 1/16 px, half width hw (1/16 px)
    perpendicular to heading deg."""
    px = (hw * icos(deg)) >> 10
    py = (hw * isin(deg)) >> 10
    fill_poly(cv, [(x0 + px, y0 + py), (x1 + px, y1 + py), (x1 - px, y1 - py), (x0 - px, y0 - py)], ink)


def polar(cx, cy, deg, r16):
    """point at radius r16 (1/16 px) along heading deg (0 = 12 o'clock, clockwise)."""
    return cx * 16 + ((r16 * isin(deg)) >> 10), cy * 16 - ((r16 * icos(deg)) >> 10)


def fmt_value(v, fmt, pad, src, tables, vals):
    if fmt == F_LIT:
        return None
    if v is None:
        return "--"
    if fmt == F_STR:
        return str(v)
    if fmt == F_MMSS:
        return "%d:%02d" % (v // 60, v % 60)
    if fmt == F_HEX:
        return ("%X" % (v & 0xffffffff)).rjust(pad, "0")
    if fmt == F_ZPAD:
        return str(v).rjust(pad, "0")
    if fmt == F_SPAD:
        return str(v).rjust(pad, " ")
    if fmt == F_CLOCK:
        h, m = divmod(v, 60)
        if not vals.get(IS24H):
            h = h % 12 or 12
        return "%d:%02d" % (h, m)
    if fmt == F_NAME:
        t = tables.get(NAME_TABLE.get(src, -1), [])
        i = v - 1 if src == MONTH else v
        return t[i] if 0 <= i < len(t) else "--"
    if fmt == F_COMPACT:
        if v >= 10000:
            return "%d.%dk" % (v // 1000, (v % 1000) // 100)
        return str(v)
    return str(v)


def derived(vals):
    v = dict(vals)
    h = v.get(HOUR24)
    if h is not None:
        v.setdefault(HOUR12, h % 12 or 12)
        v.setdefault(AMPM, 1 if h >= 12 else 0)
        v.setdefault(HOUR, h if v.get(IS24H) else (h % 12 or 12))
    if v.get(STEPS) is not None and v.get(STEP_GOAL):
        v.setdefault(STEP_PCT, min(100, v[STEPS] * 100 // v[STEP_GOAL]))
    if h is not None and v.get(MINUTE) is not None:
        mod = h * 60 + v[MINUTE]
        v.setdefault(MINUTE_OF_DAY, mod)
        sr, ss = v.get(SUNRISE), v.get(SUNSET)
        if sr is not None and ss is not None and ss > sr:
            if sr <= mod < ss:
                v.setdefault(DAYLIGHT, 1)
                v.setdefault(SUN_PROGRESS, (mod - sr) * 1000 // (ss - sr))
            else:
                night = 1440 - (ss - sr)
                v.setdefault(DAYLIGHT, 0)
                v.setdefault(SUN_PROGRESS, 1000 + ((mod - ss) % 1440) * 1000 // night)
    if v.get(MUSIC_POS) is not None and v.get(MUSIC_DUR):
        v.setdefault(MUSIC_PCT, min(100, v[MUSIC_POS] * 100 // v[MUSIC_DUR]))
    if None not in (v.get(YEAR), v.get(MONTH), v.get(DAY)):
        d = days_from_civil(v[YEAR], v[MONTH], v[DAY]) - days_from_civil(2000, 1, 6)
        v.setdefault(MOON_AGE, ((d * 1000 - 760) % 29531) // 1000)
    return v


def days_from_civil(y, m, d):
    """Days since 1970-01-01 (H. Hinnant's integer algorithm, ported as-is to Rust)."""
    y -= m <= 2
    era = y // 400
    yoe = y - era * 400
    doy = (153 * (m + (-3 if m > 2 else 9)) + 2) // 5 + d - 1
    doe = yoe * 365 + yoe // 4 - yoe // 100 + doy
    return era * 146097 + doe - 719468


def read_font(buf, off):
    hgt, sp, n, _ = struct.unpack_from("<BbBB", buf, off)
    g = {}
    for i in range(n):
        code, w, _, go = struct.unpack_from("<BBHI", buf, off + 4 + 8 * i)
        g[code] = (w, go)
    return hgt, sp, g


def wrap(s, maxw, width):
    """Greedy word wrap by pixel width; over-long words are cut. The payload
    uses the same rule."""
    lines, cur = [], ""
    for word in s.split(" "):
        if not word:
            continue
        cand = word if not cur else cur + " " + word
        if width(cand) <= maxw:
            cur = cand
            continue
        if cur:
            lines.append(cur)
        while width(word) > maxw:
            n = len(word)
            while n > 1 and width(word[:n]) > maxw:
                n -= 1
            lines.append(word[:n])
            word = word[n:]
        cur = word
    if cur:
        lines.append(cur)
    return lines


def cstr(buf, off):
    e = buf.index(b"\0", off)
    return buf[off:e].decode("ascii")


def blit(cv, buf, off, x, y, w, h, mode, ink=1):
    stride = (w + 7) // 8
    for r in range(h):
        row = buf[off + r * stride: off + (r + 1) * stride]
        yy = y + r
        if yy < 0 or yy >= H:
            continue
        for c in range(w):
            on = row[c >> 3] & (0x80 >> (c & 7))
            xx = x + c
            if xx < 0 or xx >= W:
                continue
            if on:
                if mode == M_XOR or ink == 2:
                    cv.px[yy][xx] ^= 1
                else:
                    cv.px[yy][xx] = (0 if mode == M_ERASE else ink)
            elif mode == M_OPAQUE:
                cv.px[yy][xx] = 1 - ink


def render(buf, vals):
    """Draw a .FACE file. vals: {source: int or None}. Returns (canvas, inset text)."""
    assert buf[:4] == b"IFC1", "not a face file"
    ver, hl, ol, fl = struct.unpack_from("<HHII", buf, 4)
    assert fl == len(buf), "length mismatch"
    vals = derived(vals)
    cv = Canvas()
    cv.own = False
    tables = {}
    inset = None
    pos, end = hl, hl + ol
    while pos < end:
        code = buf[pos]
        if code == 0:
            break
        size = buf[pos + 1]
        b = pos + 2
        if code == OP_RECT:
            x, y, w, h, ink, pat = struct.unpack_from("<hhHHBB", buf, b)
            for yy in range(y, y + h):
                cv.span(yy, x, x + w, ink, pat)
        elif code == OP_BITMAP:
            x, y, w, h, mode, _, off = struct.unpack_from("<hhHHBBI", buf, b)
            blit(cv, buf, off, x, y, w, h, mode)
        elif code == OP_STRINGS:
            kind, n, off = struct.unpack_from("<BBI", buf, b)
            t, o = [], off
            for _ in range(n):
                s = cstr(buf, o)
                t.append(s)
                o += len(s) + 1
            tables[kind] = t
        elif code == OP_TEXT:
            x, y, align, src, fmt, pad, ink, mw, foff, loff = struct.unpack_from("<hhBBBBBBII", buf, b)
            s = cstr(buf, loff) if fmt == F_LIT else fmt_value(vals.get(src), fmt, pad, src, tables, vals)
            hgt, sp, g = read_font(buf, foff)
            look = lambda c: g.get(ord(c), g.get(ord(c.upper())))
            width = lambda t: max(0, sum(look(c)[0] + sp for c in t if look(c)) - sp)
            maxw = mw * 2
            force = False
            if fmt == F_STR and (pad & 0x7f) and maxw:
                lines = wrap(s, maxw, width)
                k = (pad & 0x7f) - 1
                force = bool(pad & 0x80) and k + 1 < len(lines)
                s = lines[k] if k < len(lines) else ""
            if maxw and (force or width(s) > maxw):
                while s and width(s + "..") > maxw:
                    s = s[:-1]
                s = s.rstrip() + ".."
            gs = [look(c) for c in s if look(c)]
            tw = sum(w for w, _ in gs) + sp * max(0, len(gs) - 1)
            xx = x - (tw // 2 if align == CENTRE else tw if align == RIGHT else 0)
            for w, go in gs:
                blit(cv, buf, go, xx, y, w, hgt, M_INK, ink)
                xx += w + sp
        elif code == OP_HAND:
            cx, cy, src, ink, ln, tail, hwb, hwt, _, _ = struct.unpack_from("<hhBBBBBBBB", buf, b)
            h24 = vals.get(HOUR24) or 0
            m = vals.get(MINUTE) or 0
            s = vals.get(SECOND) or 0
            deg = [((h24 % 12) * 60 + m) // 2, m * 6 + s // 10, s * 6][min(src, 2)]
            bx, by = polar(cx, cy, deg + 180, tail * 16)
            tx, ty = polar(cx, cy, deg, ln * 16)
            pxb, pyb = (hwb * icos(deg)) >> 10, (hwb * isin(deg)) >> 10
            pxt, pyt = (hwt * icos(deg)) >> 10, (hwt * isin(deg)) >> 10
            fill_poly(cv, [(bx + pxb, by + pyb), (tx + pxt, ty + pyt), (tx - pxt, ty - pyt), (bx - pxb, by - pyb)], ink)
        elif code == OP_TICKS:
            cx, cy, ri, ro, n, hw, st, sw, src, ink, mx, unlit, _ = struct.unpack_from("<hhBBBBhhBBHBB", buf, b)
            lit = n
            if src:
                v = vals.get(src)
                lit = 0 if v is None or not mx else max(0, min(n, v * n // mx))
            for i in range(n):
                d = st + (sw * i // n if sw == 360 or n < 2 else sw * i // (n - 1))
                x0, y0 = polar(cx, cy, d, ri * 16)
                x1, y1 = polar(cx, cy, d, ro * 16)
                if i < lit:
                    thick_line(cv, x0, y0, x1, y1, d, hw, ink)
                elif unlit:
                    thick_line(cv, x0, y0, x1, y1, d, 6, ink)
        elif code == OP_BAR:
            x, y, w, h, src, d, lo, hi, ink, pat = struct.unpack_from("<hhHHBBhhBB", buf, b)
            v = vals.get(src)
            f = 0 if v is None or hi <= lo else max(0, min(1024, (v - lo) * 1024 // (hi - lo)))
            if d in (D_RIGHT, D_LEFT):
                n = w * f >> 10
                xs = x if d == D_RIGHT else x + w - n
                for yy in range(y, y + h):
                    cv.span(yy, xs, xs + n, ink, pat)
            else:
                n = h * f >> 10
                ys = y + h - n if d == D_UP else y
                for yy in range(ys, ys + n):
                    cv.span(yy, x, x + w, ink, pat)
        elif code == OP_IF:
            src, cmp, val, skip = struct.unpack_from("<BBhH", buf, b)
            v = vals.get(src)
            num = isinstance(v, int)  # string sources only answer NZ / Z (empty or not)
            ok = {C_NZ: bool(v), C_Z: not v, C_GE: num and v >= val,
                  C_LT: num and v < val, C_EQ: num and v == val, C_NONE: not num}.get(cmp, True)
            if not ok:
                pos += size + skip
                continue
        elif code == OP_FRAMES:
            x, y, w, h, mode, src, lo, hi, n, fl = struct.unpack_from("<hhHHBBhhBB", buf, b)
            v = vals.get(src)
            if v is None or hi <= lo:
                i = 0
            elif fl & 1:
                i = ((v - lo) % (hi - lo)) * n // (hi - lo)
            else:
                i = max(0, min(n - 1, (v - lo) * n // (hi - lo)))
            off = struct.unpack_from("<I", buf, b + 16 + 4 * i)[0]
            blit(cv, buf, off, x, y, w, h, mode)
        elif code == OP_DOT:
            cx, cy, src, ink, rad, r16, mx, st = struct.unpack_from("<hhBBBBHh", buf, b)
            if src < 3:
                h24 = vals.get(HOUR24) or 0
                m = vals.get(MINUTE) or 0
                s_ = vals.get(SECOND) or 0
                deg = [((h24 % 12) * 60 + m) // 2, m * 6 + s_ // 10, s_ * 6][src]
            else:
                v = vals.get(src)
                deg = None if v is None or not mx else v * 360 // mx
            if deg is not None:
                deg += st
                px, py = polar(cx, cy, deg, rad * 16)
                fill_poly(cv, [(px + ((r16 * isin(a)) >> 10), py - ((r16 * icos(a)) >> 10))
                               for a in range(0, 360, 24)], ink)
        elif code == OP_OWN:
            mask, = struct.unpack_from("<I", buf, b)
            t = vals.get(INSET_TYPE)
            if t is not None and 0 <= t < 32 and mask >> t & 1:
                cv.own = True
        elif code == OP_INSET:
            src, fmt, icon, loff = struct.unpack_from("<BBHI", buf, b)
            inset = cstr(buf, loff) if fmt == F_LIT else fmt_value(vals.get(src), fmt, 0, src, tables, vals)
        pos += size
    return cv, inset


# Screen geometry. The stock inset painter (FUN_00057d94) fills a circle at
# (105, 22), radius 27, after the face is drawn, and the loader never paints
# inside it: treat it as unusable. The round bezel hides the corners: BEZEL_R
# is the visible radius and readable content stays inside LAYOUT_R.
# Measured on the watch: every CAL arc to 64 visible, and CAL2's
# diagonal ticks at 66 and 70 visible but not 74. So the visible circle is
# between r 70 and 74; 72 for previews, 68 (2 px margin) for layout. The 24
# faces were laid out at 62 before this was measured, so they all still fit.
INSET_C = (105, 22)
INSET_R = 27
BEZEL_R = 72
LAYOUT_R = 68


def in_inset(x, y):
    return (2 * x + 1 - 2 * INSET_C[0]) ** 2 + (2 * y + 1 - 2 * INSET_C[1]) ** 2 <= (2 * INSET_R) ** 2


def visible(x, y, r=LAYOUT_R):
    """Pixel (x, y) can carry content: inside radius r and outside the inset."""
    return (2 * x + 1 - 128) ** 2 + (2 * y + 1 - 128) ** 2 <= (2 * r) ** 2 and not in_inset(x, y)


def safe_span(y0, y1, r=LAYOUT_R, side="left"):
    """Widest x range [x0, x1) usable on every row y0..y1-1. Rows beside the
    inset have two pieces; side picks the one left of it (default) or the
    widest."""
    best = None
    ok = [all(visible(x, y, r) for y in range(y0, y1)) for x in range(W)]
    runs, x = [], 0
    while x < W:
        if ok[x]:
            s = x
            while x < W and ok[x]:
                x += 1
            runs.append((s, x))
        x += 1
    if not runs:
        return (64, 64)
    return runs[0] if side == "left" else max(runs, key=lambda r: r[1] - r[0])


MIP_BG = (184, 190, 176)
MIP_FG = (24, 26, 24)
HIDDEN = (70, 72, 68)


def preview(cv, path, inset=None, scale=4, invert=False, show_inset=True, bezel=True):
    from PIL import Image, ImageDraw, ImageFont
    fg, bg = (MIP_BG, MIP_FG) if invert else (MIP_FG, MIP_BG)
    if getattr(cv, "own", False):
        show_inset = False  # the face draws the inset itself
    im = Image.new("RGB", (W, H), bg)
    p = im.load()
    for y in range(H):
        for x in range(W):
            if bezel and (2 * x + 1 - 128) ** 2 + (2 * y + 1 - 128) ** 2 > (2 * BEZEL_R) ** 2:
                p[x, y] = HIDDEN
            elif show_inset and in_inset(x, y):
                p[x, y] = bg
            elif cv.px[y][x]:
                p[x, y] = fg
    im = im.resize((W * scale, H * scale), Image.NEAREST)
    if show_inset:
        d = ImageDraw.Draw(im)
        cx, cy = INSET_C
        r = INSET_R
        d.ellipse([(cx - r) * scale, (cy - r) * scale, (cx + r) * scale, (cy + r) * scale],
                  outline=(200, 60, 40), width=2)
        if inset:
            try:
                f = ImageFont.truetype("C:/Windows/Fonts/consolab.ttf", 11 * scale // 2)
            except OSError:
                f = ImageFont.load_default()
            d.text((cx * scale, cy * scale), inset, fill=(200, 60, 40), anchor="mm", font=f)
    im.save(path)
    return im


SAMPLE = {HOUR24: 22, MINUTE: 8, SECOND: 36, DAY: 7, MONTH: 11, YEAR: 2026, WEEKDAY: 6,
          HR: 62, STEPS: 8432, STEP_GOAL: 10000, BATTERY: 73, CALORIES: 1840, FLOORS: 9,
          INTENSITY: 42, ALTITUDE: 154, TEMP: 18, SUNRISE: 6 * 60 + 41, SUNSET: 16 * 60 + 58,
          NOTIFS: 2, BT: 1, ALARM: 1, DND: 0, IS24H: 0, INSET_TYPE: INSET_TEMP,
          WX_COND: WX_PARTLY, WX_TEMP: 14, WX_HI: 17, WX_LO: 9, WX_PRECIP: 30, WX_WIND: 12}
