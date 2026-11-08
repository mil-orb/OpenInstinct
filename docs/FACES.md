# Making watch faces

A face is one `.FACE` file, at most 32 KB. You write it as a short Python script with [faces/facekit.py](../faces/facekit.py), which builds the file and renders a preview PNG on your PC. The watch runs a line-for-line Rust port of the same renderer ([payload/src/face.rs](../payload/src/face.rs)), so the preview is what you get on the watch, pixel for pixel. `compare.py` checks that on every change.

## Five-minute start

```bash
pip install pillow
python faces/src/hello.py
```

That writes `faces/out/HELLO.FACE` and `faces/out/HELLO_preview.png`. Open the PNG, change something in [faces/src/hello.py](../faces/src/hello.py), run it again and look again. When you're happy, copy the `.FACE` to the watch as `GARMIN/FACES/ACTIVE.FACE`. It appears at the next minute tick.

Copy `hello.py` to start your own. Change `Face("HELLO")` to your face's name (it becomes the file name).

## The screen

- **128 x 128 pixels, 1 bit.** Each pixel is ink or background. The watch's "dark" and "light" face settings swap the two, so design in ink and background, not black and white.
- **The visible window is round.** The bezel hides the corners. Keep content inside radius 68 of the centre (64, 64). The outermost visible ring is about r 70 to 74.
- **The porthole** (top right) is a second small display behind a black ring, centred near (106, 21.5) with radius about 23. Its ring hides pixels out to about r 31 from that centre. Keep text and fine detail out of it unless the face draws the porthole itself (`own_inset`, below).
- **Coordinates** are pixels from the top left. Hands, ticks and dots use 1/16 px internally, so (64, 64) is the exact centre.

`python faces/bezel_audit.py` lists every op in every built face that draws outside the visible window. `python faces/bezel_sheet.py` makes a contact sheet with hidden ink shown in red.

## Building blocks

All ops are methods on `Face`. The full signatures are in `facekit.py`.

| Op | What it draws |
|---|---|
| `rect(x, y, w, h, ink=1, pat=0)` | A filled rectangle, optionally with one of 11 fill patterns (dots, checks, scanlines, diagonals). |
| `image(bm, x, y, mode)` | A 1-bit bitmap. Modes: ink, opaque, erase, XOR. |
| `text(x, y, font, src, fmt, ...)` | A value or literal text in a font you embed. Left, centre or right aligned. Optional max width with `..`, and word wrap for long strings. |
| `hand(cx, cy, src, length, ...)` | A tapered clock hand. |
| `ticks(cx, cy, r_in, r_out, n, ...)` | A ring of ticks. With a source, it's a gauge: the first k ticks are lit. |
| `bar(x, y, w, h, src, lo, hi, ...)` | A progress bar in any of 4 directions. |
| `frames(bms, x, y, src, lo, hi)` | Picks 1 of n bitmaps from a value: weather icons, moon phases, animation-like strips. |
| `dot(cx, cy, src, radius, r16, ...)` | A disc orbiting a centre, by hand angle or by value. |
| `with f.when(src, cmp, val):` | Draws the ops inside only when the condition holds. Nests. |
| `strings(kind, names)` | Names for weekdays, months, AM/PM. |
| `inset(src, fmt, ...)` | Puts a value in the stock porthole. |
| `own_inset(*types)` | The face draws the porthole itself for the given inset settings. |

`ink` is 0 (background), 1 (ink) or 2 (invert what's under it).

## Values you can show

| Source | Meaning |
|---|---|
| `HOUR`, `HOUR24`, `HOUR12`, `MINUTE`, `SECOND`, `AMPM`, `IS24H` | Time. `HOUR` follows the watch's 12/24 h setting. |
| `DAY`, `MONTH`, `YEAR`, `WEEKDAY` | Date. Use `F_NAME` with `strings()` for names. |
| `HR`, `STEPS`, `STEP_GOAL`, `STEP_PCT`, `CALORIES`, `FLOORS`, `INTENSITY` | Activity. |
| `BATTERY`, `NOTIFS`, `BT` | Battery percent, unread notifications, phone connected. |
| `SUNRISE`, `SUNSET`, `DAYLIGHT`, `SUN_PROGRESS`, `MOON_AGE`, `MINUTE_OF_DAY` | Sun and moon, worked out on the watch from your position and the date. |
| `WX_COND`, `WX_TEMP`, `WX_HI`, `WX_LO`, `WX_PRECIP` | Weather from the phone's last report, in the watch's C/F setting. |
| `MUSIC_TITLE`, `MUSIC_ARTIST`, `MUSIC_ALBUM`, `MUSIC_PLAYING`, `MUSIC_POS`, `MUSIC_DUR`, `MUSIC_PCT` | The song playing on the phone, via Gadgetbridge. |
| `INSET_TYPE` | What the watch's inset setting shows (for `own_inset`). |

Defined in the format but **not filled on the watch yet** (they show `--`): `ALTITUDE`, `TEMP` (sensor), `ALARM`, `DND`, `WX_WIND`, and the phone data slots `DATA0`..`DATA7`, `STR0`..`STR7`. The `inset()` op is also ignored for now: the porthole keeps the watch's own inset setting unless the face uses `own_inset`. These are good first contributions.

A value can be missing: no heart rate yet, no phone, nothing playing. Text shows `--`, and `when(src, C_NONE)` lets you draw something else instead.

Formats for `text`: `F_INT`, `F_ZPAD` (zero padded), `F_SPAD` (space padded), `F_CLOCK` (minutes as h:mm), `F_NAME`, `F_COMPACT` (8.4k), `F_STR`, `F_HEX`, `F_MMSS`, and `F_LIT` for fixed text.

## Fonts

Fonts are embedded in the face, so a face can use any lettering you draw.

- [faces/src/pixfonts.py](../faces/src/pixfonts.py) has a public-domain 5x7 and a hand-drawn 3x5. They can be scaled up as dot-matrix digits. They build on any OS.
- `Font.from_ttf(path, px, chars)` rasterises a TrueType font at a pixel size and keeps only the characters you list. Several of the bundled faces use Windows fonts this way, so they only build on Windows. Before you share a face built from a font, check the font's license allows it. OFL fonts are a safe choice.
- `Font.from_patterns` builds a font from strings of `#` and `.`.

## Art

[faces/src/art.py](../faces/src/art.py) has helpers that draw into a scratch canvas and return a bitmap: lines with width, discs, rings and arcs, seven-segment digits, moon phases, stars, clouds and weather icons. The builder crops every bitmap to its set pixels, so big art costs only what it covers. Look at the bundled faces for patterns:

| Script | Faces |
|---|---|
| [build_faces.py](../faces/src/build_faces.py) | TEST, TITLE, BLADE |
| [faces2.py](../faces/src/faces2.py) | SYNC, VOTE, POWER, BU_ENSO, BU_SKY, BU_KATANA, HUD, ORBIT, UPLINK |
| [faces3.py](../faces/src/faces3.py) | GHOST, CAMO, NEON, GLITCH, NETRUN, TOPO |
| [faces4.py](../faces/src/faces4.py) | SOUND, SOUND2, VINYL, TAPE, POSTER |

## Checking a face

1. **Preview**: the PNG each script writes. The porthole shows as a red circle.
2. **Bezel**: `python faces/bezel_audit.py` and `python faces/bezel_sheet.py`.
3. **Python vs Rust**: build the Rust renderer for your PC, then compare every face in `faces/out` across fixed and random values:
   ```bash
   rustc -O --edition 2021 payload/host/facetest.rs -o payload/host/facetest.exe
   python payload/host/compare.py 300
   ```
   It should report 0 mismatches.
4. **Fuzz**, if you changed `face.rs`:
   ```bash
   rustc -O -C overflow-checks=on --edition 2021 payload/host/fuzz.rs -o payload/host/fuzz.exe
   payload/host/fuzz.exe 300000 faces/out/*.FACE
   ```

## On the watch

- `GARMIN/FACES/ACTIVE.FACE` is the face that's drawn. Swap faces by copying another file over it.
- Faces repaint when the stock face would: once a minute, or about 4 times a minute while music plays. A face adds no wake-ups of its own.
- **Crash guard**: before the first draw of a new face file the loader marks it in `STATE.BIN`, and marks it good after the draw. If the watch restarts mid-draw, that file is never drawn again. Copying a different or changed file, or deleting `STATE.BIN`, retries it.
- `GARMIN/FACES/OFF` (any file with this name) turns custom faces off.
- A file that fails its checks (magic, lengths, bounds) is never drawn. You get the stock face.
