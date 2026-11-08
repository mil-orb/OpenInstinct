"""Mock-ups of the nav and plane screens, drawn by the same Rust layout code the
watch runs (payload/src/navcore.rs, planecore.rs) compiled for your PC.

    python tools/mock/screens.py            # PNGs and contact sheets in out/mock/

The host programs payload/host/navmock.rs and planemock.rs print drawing calls
(R rect, P polygon, T text); this script renders them with facekit's canvas.
Fonts are stand-ins for the watch's own (which load at run time): a bold sans
thresholded to 1 bit, sized to match text widths in a photo of the watch. The
layout gets the cell sizes the watch's fonts report as measured from that
photo (small 16 px with 14 px ascent, medium 27 with 21), and each glyph sits
on the baseline the watch would use. Set MOCK_FONT to a .ttf path to use another font, and
HOST_TOOLCHAIN (for example "stable-x86_64-pc-windows-gnu") if your default
Rust toolchain can't link host programs.
"""
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "faces"))
import facekit as fk  # noqa: E402
from PIL import Image, ImageDraw, ImageFont  # noqa: E402

OUT = ROOT / "out" / "mock"
REPLY = Path(__file__).resolve().parent / "sample_reply.json"
EXE = ".exe" if os.name == "nt" else ""

# per font id (0 SMALL, 1 MED): PIL size, and the watch's (cell height, ascent)
SIZES = {0: 12, 1: 13}
CELLS = {0: (16, 14), 1: (27, 21)}
FONT_CANDIDATES = [
    os.environ.get("MOCK_FONT", ""),
    "C:/Windows/Fonts/arialbd.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    "/Library/Fonts/Arial Bold.ttf",
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
]


def _font(size):
    for p in FONT_CANDIDATES:
        if p and os.path.exists(p):
            return ImageFont.truetype(p, size)
    sys.exit("no bold TrueType font found; set MOCK_FONT to a .ttf path")


FONTS = {k: _font(v) for k, v in SIZES.items()}


def metrics(f):
    a, d = f.getmetrics()
    return a + d, a


def adv64(f, ch):
    return round(f.getlength(ch) * 64)


def width(fid, s):
    return (sum(adv64(FONTS[fid], c) for c in s) + 32) // 64


def write_metrics(path):
    lines = []
    for fid, f in FONTS.items():
        h, a = CELLS[fid]
        lines.append(f"M {fid} {h} {a}")
        for cp in list(range(32, 127)) + list(range(160, 256)):
            lines.append(f"W {fid} {cp} {adv64(f, chr(cp))}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def draw_text(cv, fid, x, top, align, s):
    f = FONTS[fid]
    w = width(fid, s)
    x0 = x - w // 2 if align == 1 else x
    h, a = metrics(f)
    im = Image.new("L", (w + 8, h + 4), 255)
    ImageDraw.Draw(im).text((2, 0), s, font=f, fill=0)
    p = im.load()
    # baseline on the watch's: its glyphs end at cell row ascent - 1
    dy = top + CELLS[fid][1] - a
    for yy in range(im.height):
        for xx in range(im.width):
            if p[xx, yy] < 128:
                X, Y = x0 + xx - 2, dy + yy
                if 0 <= X < 128 and 0 <= Y < 128:
                    cv.px[Y][X] = 1


def render_ops(ops):
    """Drawing calls -> facekit Canvas (128x128, 1 bit)."""
    cv = fk.Canvas()
    for op in ops:
        k, rest = op[0], op[2:]
        if k == "R":
            ink, x, y, w, h = map(int, rest.split())
            for yy in range(y, y + h):
                if 0 <= yy < 128:
                    cv.span(yy, max(0, x), min(128, x + w), ink)
        elif k == "P":
            v = list(map(int, rest.split()))
            ink, pts = v[0], v[1:]
            fk.fill_poly(cv, [(pts[i] * 16 + 8, pts[i + 1] * 16 + 8) for i in range(0, len(pts), 2)], ink)
        elif k == "T":
            fid, x, top, align, s = rest.split(" ", 4)
            draw_text(cv, int(fid), int(x), int(top), int(align), s)
    return cv


def build_host(name):
    """Compile payload/host/<name>.rs for this PC, once per source change."""
    OUT.mkdir(parents=True, exist_ok=True)
    src = ROOT / "payload" / "host" / f"{name}.rs"
    exe = OUT / f"{name}{EXE}"
    deps = [src] + list((ROOT / "payload" / "src").glob("*.rs"))
    if not exe.exists() or exe.stat().st_mtime < max(p.stat().st_mtime for p in deps):
        tc = os.environ.get("HOST_TOOLCHAIN")
        cmd = ["rustc"] + ([f"+{tc}"] if tc else []) + ["-O", "--edition", "2021", str(src), "-o", str(exe)]
        subprocess.run(cmd, check=True)
    return exe


def cases(name, *args):
    """Run a host mock, return {case name: [drawing calls]} in order."""
    exe = build_host(name)
    mfile = OUT / "metrics.txt"
    write_metrics(mfile)
    txt = subprocess.run([str(exe), str(mfile), *map(str, args)], capture_output=True,
                         check=True).stdout.decode("utf-8")
    out, cur = {}, None
    for line in txt.splitlines():
        if line.startswith("CASE "):
            cur = line[5:]
            out[cur] = []
        elif cur:
            out[cur].append(line)
    return out


def sheet(tiles, path, cols=4):
    w, h = tiles[0][1].size
    rows = (len(tiles) + cols - 1) // cols
    im = Image.new("RGB", (cols * (w + 10) + 10, rows * (h + 30) + 10), (255, 255, 255))
    d = ImageDraw.Draw(im)
    for i, (name, tile) in enumerate(tiles):
        x, y = 10 + (i % cols) * (w + 10), 10 + (i // cols) * (h + 30)
        im.paste(tile, (x, y))
        d.text((x, y + h + 4), name, fill=(0, 0, 0))
    im.save(path)
    print(path)


def main():
    for prog, args, prefix in (("navmock", (), "nav"), ("planemock", (REPLY,), "plane")):
        tiles = []
        for name, ops in cases(prog, *args).items():
            if name.startswith("seq_"):
                continue
            png = OUT / f"{prefix}_{name}.png"
            tiles.append((name, fk.preview(render_ops(ops), str(png), scale=3, show_inset=False)))
        sheet(tiles, OUT / f"{prefix}_sheet.png")


if __name__ == "__main__":
    main()
