"""README media: round-cropped GIFs of the plane finder, the watch faces and
turn-by-turn, rendered by the same code the watch runs.

    python tools/media/make_gifs.py        # writes media/*.gif

Faces come from faces/out/<NAME>.FACE (run the scripts in faces/src first)
or faces/examples/. Plane and nav frames come from tools/mock/screens.py.
"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "faces"))
sys.path.insert(0, str(ROOT / "tools" / "mock"))
import facekit as fk  # noqa: E402
import screens  # noqa: E402
from PIL import Image, ImageDraw  # noqa: E402

MEDIA = ROOT / "media"
SCALE = 3
FG, BG = fk.MIP_FG, fk.MIP_BG
RING = (52, 54, 50)          # the porthole's black rim
KEY = (255, 0, 255)          # transparent outside the round window
PORT_C, PORT_R = (106, 21.5), 23.5


def frame(cv, invert=False, porthole=True):
    """1-bit canvas -> round watch window, scaled, porthole rim drawn."""
    fg, bg = (BG, FG) if invert else (FG, BG)
    im = Image.new("RGB", (128, 128), bg)
    p = im.load()
    for y in range(128):
        for x in range(128):
            if cv.px[y][x]:
                p[x, y] = fg
    im = im.resize((128 * SCALE, 128 * SCALE), Image.NEAREST)
    d = ImageDraw.Draw(im)
    if porthole:
        cx, cy = PORT_C[0] * SCALE, PORT_C[1] * SCALE
        r = PORT_R * SCALE
        d.ellipse([cx - r - 9, cy - r - 9, cx + r + 9, cy + r + 9], outline=RING, width=10)
    # round window: everything outside r 64 becomes the transparent key
    mask = Image.new("L", im.size, 0)
    ImageDraw.Draw(mask).ellipse([1, 1, im.width - 2, im.height - 2], fill=255)
    out = Image.new("RGB", im.size, KEY)
    out.paste(im, (0, 0), mask)
    return out


def save_gif(frames, durations, name):
    MEDIA.mkdir(exist_ok=True)
    # one fixed palette for every frame, key colour at index 0 = transparent
    colours = [KEY, FG, BG, RING, fk.HIDDEN]
    pimg = Image.new("P", (1, 1))
    pimg.putpalette([c for rgb in colours for c in rgb] + [0] * (768 - 3 * len(colours)))
    pal = [f.quantize(palette=pimg, dither=Image.Dither.NONE) for f in frames]
    path = MEDIA / f"{name}.gif"
    pal[0].save(path, save_all=True, append_images=pal[1:], duration=durations, loop=0,
                disposal=2, transparency=0, optimize=False)
    print(path, len(frames), "frames")


def plane_finder():
    seq = {k: v for k, v in screens.cases("planemock", screens.REPLY).items() if k.startswith("seq_")}
    hold = {"ready": 1400, "hold": 260, "search": 450, "card": 2600, "next": 2200, "map": 2600}
    frames, durs = [], []
    for name, ops in seq.items():
        frames.append(frame(screens.render_ops(ops)))
        durs.append(hold[name.split("_", 2)[2]])
    save_gif(frames, durs, "plane_finder")


def nav():
    cs = screens.cases("navmock")
    pick = ["turn_right", "keep_left", "photo_left", "arrive"]
    frames = [frame(screens.render_ops(cs[n])) for n in pick if n in cs]
    save_gif(frames, [1800] * len(frames), "turn_by_turn")


def face_blob(name):
    for p in (ROOT / "faces" / "out" / f"{name}.FACE", ROOT / "faces" / "examples" / f"{name}.FACE"):
        if p.exists():
            return p.read_bytes()
    raise SystemExit(f"{name}.FACE not built: run the scripts in faces/src first")


def face_frame(name, vals):
    cv, _ = fk.render(face_blob(name), vals)
    return frame(cv)


def base(h, m, s=0):
    v = dict(fk.SAMPLE)
    v.update({fk.HOUR24: h, fk.MINUTE: m, fk.SECOND: s, fk.MINUTE_OF_DAY: h * 60 + m})
    return v


def faces_tour():
    names = ["SOUND", "BU_SKY", "ORBIT", "NETRUN", "HUD", "TOPO", "VINYL", "BLADE", "NEON", "TAPE"]
    frames = [face_frame(n, base(10, 8 + i)) for i, n in enumerate(names)]
    save_gif(frames, [1500] * len(frames), "faces")


def music_face():
    frames, durs = [], []
    idle = base(22, 8)
    frames.append(face_frame("SOUND", idle))
    durs.append(1600)
    for i, pos in enumerate((15, 60, 105, 150, 195)):
        v = base(22, 9 + i)
        v.update({fk.MUSIC_TITLE: "Night Drive", fk.MUSIC_ARTIST: "Static Coast", fk.MUSIC_ALBUM: "Signal",
                  fk.MUSIC_PLAYING: 1, fk.MUSIC_POS: pos, fk.MUSIC_DUR: 226})
        frames.append(face_frame("SOUND", v))
        durs.append(900)
    save_gif(frames, durs, "music_face")


def sky_day():
    frames = []
    for i, minute in enumerate(range(5 * 60, 29 * 60, 90)):
        m = minute % 1440
        v = base(m // 60, m % 60)
        rise, sset = v[fk.SUNRISE], v[fk.SUNSET]
        day = rise <= m < sset
        if day:
            prog = (m - rise) * 1000 // (sset - rise)
        else:
            night = (1440 - sset) + rise
            prog = 1000 + ((m - sset) % 1440) * 1000 // night
        v.update({fk.DAYLIGHT: int(day), fk.SUN_PROGRESS: prog})
        frames.append(face_frame("BU_SKY", v))
    save_gif(frames, [450] * len(frames), "sky_day")


if __name__ == "__main__":
    plane_finder()
    nav()
    faces_tour()
    music_face()
    sky_day()
