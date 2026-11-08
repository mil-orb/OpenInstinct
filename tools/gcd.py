"""Minimal Garmin GCD parser/dumper.

Format: 8-byte magic "GARMINd\\0", then TLV records (u16 type, u16 len, data),
little endian, ending with type 0xFFFF. Consecutive records of the same type
are chunks of one binary and get concatenated.

Usage:
  python -I gcd.py list GUPDATE.GCD
  python -I gcd.py dump GUPDATE.GCD OUTDIR
"""
import struct
import sys
import os


def records(buf):
    if buf[:8] != b"GARMINd\x00":
        raise ValueError("bad magic %r" % buf[:8])
    off = 8
    while off < len(buf):
        t, n = struct.unpack_from("<HH", buf, off)
        yield off, t, buf[off + 4: off + 4 + n]
        off += 4 + n
        if t == 0xFFFF:
            break


def groups(buf):
    """Merge runs of identical types."""
    cur = None
    for off, t, d in records(buf):
        if cur and cur[1] == t:
            cur[2].append(d)
            cur[3] += 1
        else:
            if cur:
                yield cur
            cur = [off, t, [d], 1]
    if cur:
        yield cur


def checksum_ok(buf):
    """Each type-0x0001 rectifier zeroes the running byte sum up to itself."""
    return all(sum(buf[:off + 5]) & 0xFF == 0 for off, t, _ in records(buf) if t == 1)


def main():
    cmd, path = sys.argv[1], sys.argv[2]
    buf = open(path, "rb").read()
    print("file %s  size %d  rectifiers %s" % (
        path, len(buf), "ok" if checksum_ok(buf) else "BAD"))
    out = sys.argv[3] if cmd == "dump" else None
    if out:
        os.makedirs(out, exist_ok=True)
    for i, (off, t, chunks, cnt) in enumerate(groups(buf)):
        data = b"".join(chunks)
        preview = ""
        if len(data) <= 64:
            txt = data.rstrip(b"\x00")
            preview = repr(txt) if txt and all(32 <= c < 127 for c in txt) else data.hex()
        print("%3d @%08x type %04x  chunks %4d  len %8d  %s" % (i, off, t, cnt, len(data), preview))
        if out and len(data) > 64:
            with open(os.path.join(out, "%02d_type%04x_off%08x.bin" % (i, t, off)), "wb") as f:
                f.write(data)


if __name__ == "__main__":
    main()
