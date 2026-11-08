"""Render every faces/out/*.FACE with facekit.render (Python reference) and with
payload/src/face.rs (via host/facetest.exe), for fixed and random value sets,
and report any pixel or inset difference. Run: python payload/host/compare.py [N_RANDOM]"""
import os, sys, glob, random, subprocess, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, "..", "..")
sys.path.insert(0, os.path.join(ROOT, "faces"))
from facekit import *
from facekit import render, SAMPLE

EXE = os.path.join(HERE, "facetest.exe")
DEMO = dict(SAMPLE); DEMO.update({DATA0: 4, STR0: "BUS 14 IN 6 MIN", STR0 + 1: "DENTIST 15:30"})
MUSIC = dict(DEMO); MUSIC.update({MUSIC_TITLE: "Neon Rain Over Kowloon Bay", MUSIC_ARTIST: "Static Bloom",
                                  MUSIC_ALBUM: "Night Market Tapes", MUSIC_PLAYING: 1, MUSIC_POS: 83, MUSIC_DUR: 245})
TIME_ONLY = {HOUR24: 9, MINUTE: 5, SECOND: 0, IS24H: 1}
NUMERIC = [HOUR24, MINUTE, SECOND, DAY, MONTH, YEAR, WEEKDAY, HR, STEPS, STEP_GOAL, BATTERY, CALORIES,
           FLOORS, INTENSITY, ALTITUDE, TEMP, SUNRISE, SUNSET, NOTIFS, BT, ALARM, DND, IS24H, INSET_TYPE,
           WX_COND, WX_TEMP, WX_HI, WX_LO, WX_PRECIP, WX_WIND] + list(range(70, 78)) + [93, 94, 95]
WORDS = "a an the night bus 14 DENTIST supercalifragilistic x ok 15:30 hello WORLD Kowloon -- ..".split()


def rand_vals(r):
    v = {}
    for s in NUMERIC:
        k = r.random()
        if k < 0.15:
            continue
        if k < 0.25:
            v[s] = r.choice([0, -1, 1, 59, 60, 1439, 9999, 10000, 99999, -32768, 100000])
        elif s in (HOUR24,):
            v[s] = r.randrange(24)
        elif s in (MINUTE, SECOND):
            v[s] = r.randrange(60)
        elif s == MONTH:
            v[s] = r.randrange(1, 13)
        elif s == DAY:
            v[s] = r.randrange(1, 32)
        elif s == YEAR:
            v[s] = r.randrange(1990, 2100)
        elif s == WEEKDAY:
            v[s] = r.randrange(7)
        elif s in (SUNRISE, SUNSET):
            v[s] = r.randrange(1440)
        else:
            v[s] = r.randrange(-50, 20000)
    for s in list(range(80, 88)) + [90, 91, 92]:
        if r.random() < 0.5:
            v[s] = " ".join(r.choice(WORDS) for _ in range(r.randrange(0, 9)))
    return v


def write_vals(path, v):
    with open(path, "w", newline="\n") as fh:
        for s, x in v.items():
            fh.write("%d s %s\n" % (s, x) if isinstance(x, str) else "%d %d\n" % (s, x))


def main():
    n_rand = int(sys.argv[1]) if len(sys.argv) > 1 else 40
    r = random.Random(1)
    hr_inset = dict(SAMPLE); hr_inset[INSET_TYPE] = 1
    sets = [("SAMPLE", SAMPLE), ("DEMO", DEMO), ("MUSIC", MUSIC), ("TIME_ONLY", TIME_ONLY), ("EMPTY", {}),
            ("HR_INSET", hr_inset)]
    # temperature inset edges: big size for -9..99 and no reading, small otherwise
    for t in (-10, -9, 0, 99, 100, None):
        v = dict(SAMPLE); v[INSET_TYPE] = INSET_TEMP
        if t is None:
            v.pop(WX_TEMP, None)
        else:
            v[WX_TEMP] = t
        sets.append(("WX%s" % t, v))
    for h in range(0, 24, 5):
        t = dict(DEMO); t.update({HOUR24: h, MINUTE: (h * 17) % 60, SECOND: (h * 31) % 60, IS24H: h % 2})
        sets.append(("H%02d" % h, t))
    sets += [("R%03d" % i, rand_vals(r)) for i in range(n_rand)]
    faces = sorted(glob.glob(os.path.join(ROOT, "faces", "out", "*.FACE")))
    tmp = tempfile.mkdtemp()
    vp, op = os.path.join(tmp, "v.txt"), os.path.join(tmp, "o.bin")
    runs = bad = skipped = 0
    for fp in faces:
        buf = open(fp, "rb").read()
        for name, v in sets:
            try:
                cv, inset = render(buf, v)
            except Exception as e:
                skipped += 1
                continue
            write_vals(vp, v)
            subprocess.run([EXE, fp, vp, op], check=True)
            out = open(op, "rb").read()
            want = bytearray(2048)
            for y in range(128):
                for x in range(128):
                    if cv.px[y][x]:
                        want[y * 16 + (x >> 3)] |= 0x80 >> (x & 7)
            got_inset = out[2049:].decode("latin1") if out[2048] & 3 == 2 else None
            got_own = bool(out[2048] & 4)
            runs += 1
            diffs = [(i % 16 * 8, i // 16) for i in range(2048) if want[i] != out[i]]
            if diffs or got_inset != inset or got_own != cv.own:
                bad += 1
                if bad <= 15:
                    print("MISMATCH %s %s: %d bytes differ, first at x~%d y=%d; inset py=%r rs=%r own py=%r rs=%r"
                          % (os.path.basename(fp), name, len(diffs), *(diffs[0] if diffs else (-1, -1)), inset, got_inset,
                             cv.own, got_own))
    print("faces %d, value sets %d, compared %d, mismatches %d, python errors skipped %d"
          % (len(faces), len(sets), runs, bad, skipped))


if __name__ == "__main__":
    main()
