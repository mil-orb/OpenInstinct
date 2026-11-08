"""Contact sheet of every face with the bezel applied: hidden screen area
dimmed, ink that falls in it shown red. Run: python -X utf8 faces/bezel_sheet.py [RADIUS]"""
import os, sys, glob
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE); sys.path.insert(0, os.path.join(HERE, "..", "payload", "host"))
from PIL import Image, ImageDraw
from facekit import render, MIP_BG, MIP_FG
import compare as C
from bezel_audit import outside

HIDDEN_BG, HIDDEN_FG = (70, 72, 68), (220, 40, 30)


def face_img(buf, vals, r, scale=2):
    cv, _ = render(buf, vals)
    im = Image.new("RGB", (128, 128))
    p = im.load()
    for y in range(128):
        for x in range(128):
            on = cv.px[y][x]
            if outside(x, y, r):
                p[x, y] = HIDDEN_FG if on else HIDDEN_BG
            else:
                p[x, y] = MIP_FG if on else MIP_BG
    return im.resize((128 * scale, 128 * scale), Image.NEAREST)


def main():
    r = float(sys.argv[1]) if len(sys.argv) > 1 else 62
    faces = [f for f in sorted(glob.glob(os.path.join(HERE, "out", "*.FACE"))) if not f.endswith("CAL.FACE")]
    cols, cell, pad = 6, 256, 22
    rows = (len(faces) + cols - 1) // cols
    sheet = Image.new("RGB", (cols * (cell + 8) + 8, rows * (cell + pad + 8) + 8), (30, 30, 30))
    d = ImageDraw.Draw(sheet)
    for i, fp in enumerate(faces):
        buf = open(fp, "rb").read()
        vals = C.MUSIC if "SOUND" in fp or fp.endswith(("TAPE.FACE", "VINYL.FACE")) else C.DEMO
        x = 8 + (i % cols) * (cell + 8)
        y = 8 + (i // cols) * (cell + pad + 8)
        sheet.paste(face_img(buf, vals, r), (x, y + pad))
        d.text((x, y + 4), os.path.basename(fp)[:-5], fill=(230, 230, 230))
    out = os.path.join(HERE, "out", "BEZEL_SHEET_r%d.png" % r)
    sheet.save(out)
    print(out)


if __name__ == "__main__":
    main()
