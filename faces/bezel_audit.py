"""List face ops that put pixels outside the round bezel (corners hidden on
the Instinct). Plain fills (RECT) are skipped: filling the corners is fine.
Run: python -X utf8 faces/bezel_audit.py [RADIUS]"""
import os, sys, glob, struct
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE); sys.path.insert(0, os.path.join(HERE, "..", "payload", "host"))
from facekit import render
import compare as C

NAMES = {2: "BITMAP", 3: "TEXT", 4: "HAND", 5: "TICKS", 6: "BAR", 10: "FRAMES", 11: "DOT"}


from facekit import visible


def outside(x, y, r):
    """True if the pixel cannot carry content: outside radius r or under the inset."""
    return not visible(x, y, r)


def describe(buf, p, code):
    from facekit import SOURCE_NAMES, cstr
    b = p + 2
    x, y = struct.unpack_from("<hh", buf, b)
    if code == 3:
        src, fmt, loff = buf[b + 5], buf[b + 6], struct.unpack_from("<I", buf, b + 14)[0]
        what = repr(cstr(buf, loff)) if fmt == 5 else SOURCE_NAMES.get(src, "src%d" % src)
        return "TEXT (%d,%d) %s" % (x, y, what)
    if code in (2, 10):
        w, h = struct.unpack_from("<HH", buf, b + 4)
        return "%s (%d,%d) %dx%d" % (NAMES[code], x, y, w, h)
    if code == 6:
        w, h = struct.unpack_from("<HH", buf, b + 4)
        return "BAR (%d,%d) %dx%d %s" % (x, y, w, h, buf[b + 8])
    return "%s (%d,%d)" % (NAMES[code], x, y)


def audit(buf, vals, r, detail=None):
    hl, ol = struct.unpack_from("<HI", buf, 6)
    pos, end, ops = hl, hl + ol, []
    while pos < end and buf[pos]:
        ops.append((pos, buf[pos])); pos += buf[pos + 1]
    prev = render(buf[:8] + struct.pack("<I", 0) + buf[12:], vals)[0].px
    hits = {}
    for i, (p, code) in enumerate(ops):
        nxt = ops[i + 1][0] if i + 1 < len(ops) else end
        cut = buf[:8] + struct.pack("<I", nxt - hl) + buf[12:]
        cur = render(cut, vals)[0].px
        if code in NAMES:
            n = sum(1 for y in range(128) for x in range(128) if cur[y][x] != prev[y][x] and outside(x, y, r))
            if n:
                hits.setdefault(NAMES[code], 0); hits[NAMES[code]] += n
                if detail is not None:
                    detail[i] = max(detail.get(i, (0, ""))[0], n), describe(buf, p, code)
        prev = cur
    return hits


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    ops = "--ops" in sys.argv
    r = float(args[0]) if args and args[0][0].isdigit() else 62
    names = [a.upper() for a in args if not a[0].isdigit()]
    for fp in sorted(glob.glob(os.path.join(HERE, "out", "*.FACE"))):
        if names and os.path.basename(fp)[:-5] not in names:
            continue
        if fp.endswith("CAL.FACE"):
            continue
        buf = open(fp, "rb").read()
        h = {}
        det = {} if ops else None
        for vals in (C.DEMO, C.MUSIC):
            for k, n in audit(buf, vals, r, det).items():
                h[k] = max(h.get(k, 0), n)
        print("%-12s %s" % (os.path.basename(fp)[:-5], ", ".join("%s %d px" % kv for kv in sorted(h.items())) or "clear"))
        for i, (n, d) in sorted((det or {}).items()):
            print("    op%-3d %4d px  %s" % (i, n, d))


if __name__ == "__main__":
    main()
