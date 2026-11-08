#!/bin/sh
# Build OpenInstinct: both payloads from source, then patch your own copy of
# Garmin's 19.10 update into out/GARMIN/GUPDATE.GCD (faces + plane finder +
# navigation, marked 1909 and forced so the stock 19.10 file can undo it).
#
#   sh tools/build.sh path/to/System_1910/GUPDATE.GCD
#
# The stock file is checked by SHA-256 first; anything else is refused. The
# script refuses to build if a payload leaves its region, two patches overlap,
# or a patch target is not the expected stock bytes. It writes files only and
# flashes nothing.
set -e
STOCK_SHA=3d628860900757ce8c7ecf3f7b611b5a6dc0c990e26a55287e25ff1c0d7f09ef
BASE="${1:-$STOCK}"
[ -n "$BASE" ] && [ -f "$BASE" ] || { echo "usage: sh tools/build.sh path/to/stock/19.10/GUPDATE.GCD"; exit 1; }
BASE="$(cd "$(dirname "$BASE")" && pwd)/$(basename "$BASE")"
cd "$(dirname "$0")/.."
PY="${PYTHON:-python3}"; "$PY" -c "" >/dev/null 2>&1 || PY=python
GOT=$("$PY" -I -c "import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())" "$BASE")
[ "$GOT" = "$STOCK_SHA" ] || { echo "$BASE is not Garmin's stock 19.10 GUPDATE.GCD (sha256 $GOT)"; exit 1; }
export BASE

sh payload/build.sh
sh payload/build_face.sh

"$PY" -I - <<'EOF'
import os, struct, subprocess, sys
sys.path.insert(0, "tools")
from bw import bw, bl, sym

NAV_ELF, NAV_BIN = "payload/build/navhook.elf", "payload/build/navhook.bin"
FACE_ELF, FACE_BIN = "payload/build/facehook.elf", "payload/build/facehook.bin"
# free 0xff run in 19.10: 0xf41ca-0xfffff. Faces at 0xf41d0 may grow up to
# the nav payload, which takes the top 10.25 KB.
FACE_AT, NAV_AT, FREE_END = 0xF41D0, 0xFD700, 0x100000

def hook10(at, old, target):
    # ldr.w pc, [pc, #4]; nop; .word target|1  (at must be 2 mod 4 so the
    # literal at at+6 is word aligned)
    assert at % 4 == 2, hex(at)
    return (at, bytes.fromhex(old), bytes.fromhex("dff804f000bf") + struct.pack("<I", target | 1))

def b4(at, old, target):
    return (at, bytes.fromhex(old), bw(at, target))

nav_size = os.path.getsize(NAV_BIN)
face_size = os.path.getsize(FACE_BIN)
assert NAV_AT + nav_size <= FREE_END, "nav payload %#x-%#x leaves the free region" % (NAV_AT, NAV_AT + nav_size)
assert FACE_AT + face_size <= NAV_AT, "faces payload %#x-%#x runs into the nav payload" % (FACE_AT, FACE_AT + face_size)

s = lambda e, n: sym(e, n)
nav = [
    hook10(0x100F8496, "a24b1b68dc680121a07a", s(NAV_ELF, "nav_entry")),
    hook10(0x100F84F2, "8c4b1c78002c00f02d81", s(NAV_ELF, "tick_entry")),
    hook10(0x100F8A36, "4ff0ff31062087f0c4fc", s(NAV_ELF, "vibe_entry")),
    b4(0x000B3080, "70b50b1d", s(NAV_ELF, "evt_entry")),
    b4(0x00054B84, "f0b50446", s(NAV_ELF, "alert_entry")),
]
faces = [b4(0x00062858, "2de9f04f", s(FACE_ELF, "face_entry")),
         b4(0x000562E0, "2de9f04f", s(FACE_ELF, "page_entry")),
         # 1 Hz repaint call in FUN_00055cd0: bl FUN_00062858 -> bl tick_entry
         (0x00055CEE, bytes.fromhex("0cf0b3fd"), bl(0x00055CEE, s(FACE_ELF, "tick_entry"))),
         # key setter FUN_0005bd00, no-hotkey path: movs r2,#0xff -> movs r2,#0x21,
         # so holding BACK always sends hold code 0x21 (opens plane lookup);
         # a quick BACK press then fires on release
         (0x0005BD46, bytes.fromhex("ff22"), bytes.fromhex("2122")),
         # page flags lookup FUN_100ae200: the plane page gets flags 0x1 (stock
         # pages are all in its table, ours is not)
         hook10(0x100AE202, "0c4a0c4c002340f25f21", s(FACE_ELF, "page_flags_entry"))]

# every byte range the build writes, checked pairwise
ranges = [(NAV_AT, NAV_AT + nav_size, "nav payload"), (FACE_AT, FACE_AT + face_size, "faces payload")]
ranges += [(a, a + len(new), "hook %#x" % a) for a, old, new in nav + faces]
for i, (a0, a1, n0) in enumerate(ranges):
    for b0, b1, n1 in ranges[i + 1:]:
        assert a1 <= b0 or b1 <= a0, "%s %#x-%#x overlaps %s %#x-%#x" % (n0, a0, a1, n1, b0, b1)
for a, old, new in nav + faces:
    assert len(old) == len(new), hex(a)

def lines(payloads, hooks):
    out = ["%#x@%s" % (at, f) for at, f in payloads]
    out += ["%#x:%s>%s" % (a, old.hex(), new.hex()) for a, old, new in hooks]
    return out

full = lines([(NAV_AT, NAV_BIN), (FACE_AT, FACE_BIN)], nav + faces)
os.makedirs("out/GARMIN", exist_ok=True)
with open("out/patches.txt", "w") as fh:
    fh.writelines(line + chr(10) for line in full)
subprocess.check_call([sys.executable, "-I", "tools/patchgcd.py", os.environ["BASE"], "out/GARMIN/GUPDATE.GCD",
                       "--drop-0505", "--force", "--version", "1909"] + full, stdout=subprocess.DEVNULL)
print("nav   %#x-%#x (%d bytes)" % (NAV_AT, NAV_AT + nav_size, nav_size))
print("faces %#x-%#x (%d bytes)" % (FACE_AT, FACE_AT + face_size, face_size))
print("free after faces: %d bytes (up to the nav payload)" % (NAV_AT - FACE_AT - face_size))
print("free after nav: %d bytes" % (FREE_END - NAV_AT - nav_size))
print("\n".join(full))
EOF
"$PY" -I -c "import hashlib; print('out/GARMIN/GUPDATE.GCD sha256', hashlib.sha256(open('out/GARMIN/GUPDATE.GCD','rb').read()).hexdigest())"
