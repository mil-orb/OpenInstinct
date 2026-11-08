"""Patch the Instinct (006-B3126) main firmware inside a GUPDATE.GCD and fix
every integrity field we know about.

Integrity layers found in 19.10:
  1. image SHA-1: sha1(img[:H]) stored at img[H:H+20], H = 0x2d9220
  2. image byte sum: sum(img) & 0xff == 0, fixed with the image's last byte
  3. GCD rectifiers: each type-0x0001 record byte makes the running sum of
     the file (from offset 0 through that byte) == 0 mod 256

Address map (main image, TLV type 0x02bd):
  img[0x000000:0x0fe000] -> 0x00002000 (internal flash)
  img[0x0fe000:]         -> 0x10060000 (external XIP flash)

Usage:
  python -I patchgcd.py IN.GCD OUT.GCD [--drop-0505] [--force] [--version N] [ADDR:HEXBYTES ...]
  python -I patchgcd.py IN.GCD OUT.GCD 0x102132dc:5761746368204661636521
With no patches it rebuilds the file, which must match IN byte for byte.

Install-control options (see docs/RECOVERY.md):
  --drop-0505   remove the 0x0505 loader component (keep the one on the watch)
  --force       set field 0xb (first byte of the 0x02bd type-7 header) to 1, the
                flag Garmin's own System_Backdate_1901 file uses; the updater then
                skips its "installed < file" version check for this component
  --version N   set the version the updater compares: the type-7 header field
                0x100d and the u16 at img+0x6c inside the image info record
                (magic 0x389db9f0 at img+0x50). The firmware's compiled version
                constant (0x776) is untouched, so no downgrade/settings reset.
"""
import hashlib
import struct
import sys

MAIN_TYPE = 0x02BD
SHA_OFF = 0x2D9220
SPLIT = 0xFE000
INT_BASE = 0x00002000
EXT_BASE = 0x10060000


def addr_to_img(a):
    if INT_BASE <= a < INT_BASE + SPLIT:
        return a - INT_BASE
    if a >= EXT_BASE:
        return a - EXT_BASE + SPLIT
    raise ValueError("address %#x outside image" % a)


def find_sha(img):
    """SHA-1 of everything before it sits right after the content, then 0xff
    padding. Its offset differs per build (19.10: 0x2d9220, 19.01: 0x2d9210)."""
    h = hashlib.sha1()
    h.update(img[:len(img) - 0x4000])
    for i in range(len(img) - 0x4000, len(img) - 20):
        if h.copy().digest() == img[i:i + 20]:
            return i
        h.update(img[i:i + 1])
    raise SystemExit("no embedded SHA-1 found")


def tlvs(buf):
    off = 8
    while off < len(buf):
        t, n = struct.unpack_from("<HH", buf, off)
        yield off, t, n
        off += 4 + n
        if t == 0xFFFF:
            break


def drop_0505(buf):
    """Remove the 0x0006/0x0007/0x0505 group and the rectifier after it."""
    recs = list(tlvs(buf))
    for i, (off, t, n) in enumerate(recs):
        if t == 0x0505:
            assert recs[i - 1][1] == 0x0007 and recs[i - 2][1] == 0x0006
            assert recs[i + 1][1] == 0x0001
            start = recs[i - 2][0]
            end = recs[i + 1][0] + 4 + recs[i + 1][2]
            print("drop 0x0505 group: file %#x-%#x (%d bytes)" % (start, end, end - start))
            return buf[:start] + buf[end:]
    print("no 0x0505 component present")
    return buf


def main_header(buf):
    """Offset of the type-7 record that describes the 0x02bd component."""
    recs = list(tlvs(buf))
    for i, (off, t, n) in enumerate(recs):
        if t == MAIN_TYPE:
            assert recs[i - 1][1] == 0x0007 and recs[i - 1][2] == 16
            h = recs[i - 1][0] + 4
            assert struct.unpack_from("<H", buf, h + 2)[0] == MAIN_TYPE
            return h
    raise SystemExit("no 0x02bd component")


def main():
    argv = sys.argv[1:]
    drop = "--drop-0505" in argv
    force = "--force" in argv
    version = None
    if "--version" in argv:
        k = argv.index("--version")
        version = int(argv[k + 1], 0)
        del argv[k:k + 2]
    argv = [a for a in argv if a not in ("--drop-0505", "--force")]
    src, dst = argv[0], argv[1]
    sys.argv = [sys.argv[0]] + argv
    # ADDR:NEWHEX        overwrite
    # ADDR:OLDHEX>NEWHEX overwrite only if the current bytes are OLDHEX
    # ADDR@FILE          inject a file; the target must be 0xff padding
    patches = []
    for p in sys.argv[3:]:
        if "@" in p:
            a, f = p.split("@", 1)
            patches.append((int(a, 0), open(f, "rb").read(), "ff"))
            continue
        a, hx = p.split(":")
        old = None
        if ">" in hx:
            old, hx = hx.split(">")
            old = bytes.fromhex(old)
        patches.append((int(a, 0), bytes.fromhex(hx), old))

    buf = bytearray(open(src, "rb").read())
    if drop:
        buf = drop_0505(buf)
    h = main_header(buf)
    if force:
        print("force flag (field 0xb): %d -> 1" % buf[h])
        buf[h] = 1
    if version is not None:
        old = struct.unpack_from("<H", buf, h + 14)[0]
        print("header version (field 0x100d): %d -> %d" % (old, version))
        struct.pack_into("<H", buf, h + 14, version)
    recs = list(tlvs(buf))
    chunks = [(off + 4, n) for off, t, n in recs if t == MAIN_TYPE]
    img = bytearray(b"".join(buf[o:o + n] for o, n in chunks))

    global SHA_OFF
    SHA_OFF = find_sha(img)
    print("image len %#x, SHA-1 at img+%#x" % (len(img), SHA_OFF))
    assert sum(img) & 0xFF == 0, "input image byte sum mismatch"

    if version is not None:
        assert struct.unpack_from("<II", img, 0x50) == (0x389DB9F0, 0xC762460F), "info record not at img+0x50"
        old = struct.unpack_from("<H", img, 0x6C)[0]
        print("image info record version (img+0x6c): %d -> %d" % (old, version))
        struct.pack_into("<H", img, 0x6C, version)

    for a, data, old in patches:
        i = addr_to_img(a)
        assert i + len(data) <= SHA_OFF, "patch overlaps SHA-1 / tail"
        assert addr_to_img(a + len(data) - 1) == i + len(data) - 1, "patch crosses the int/ext split"
        cur = bytes(img[i:i + len(data)])
        if old == "ff":
            assert cur == b"\xff" * len(data), "inject target %#x is not free (0xff) space" % a
            print("inject %#010x (img+%#x): %d bytes into 0xff padding" % (a, i, len(data)))
        else:
            if old is not None:
                assert cur == old, "at %#x expected %s, found %s" % (a, old.hex(), cur.hex())
            print("patch %#010x (img+%#x): %s -> %s" % (a, i, cur.hex(), data.hex()))
        img[i:i + len(data)] = data

    img[SHA_OFF:SHA_OFF + 20] = hashlib.sha1(img[:SHA_OFF]).digest()
    img[-1] = 0
    img[-1] = (-sum(img)) & 0xFF

    pos = 0
    for o, n in chunks:
        buf[o:o + n] = img[pos:pos + n]
        pos += n
    assert pos == len(img)

    for off, t, n in recs:
        if t == 0x0001:
            buf[off + 4] = 0
            buf[off + 4] = (-sum(buf[:off + 5])) & 0xFF

    open(dst, "wb").write(buf)
    orig = open(src, "rb").read()
    diff = sum(1 for x, y in zip(orig, buf) if x != y)
    print("wrote %s, %d bytes differ from input" % (dst, diff))


if __name__ == "__main__":
    main()
