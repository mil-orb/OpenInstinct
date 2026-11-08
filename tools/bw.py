"""Thumb-2 B.W (T4) encoder and ELF symbol lookup for hook patches.
  bw.py SRC DST            -> hex bytes of "b.w DST" placed at SRC
  bw.py --bl SRC DST       -> hex bytes of "bl DST" placed at SRC
  bw.py --sym ELF NAME     -> symbol address"""
import struct, sys


def bl(src, dst):
    """Thumb-2 BL (T1): the B.W encoding with bit 14 of the second halfword set."""
    hw1, hw2 = struct.unpack("<HH", bw(src, dst))
    return struct.pack("<HH", hw1, hw2 | 0x4000)


def bw(src, dst):
    off = dst - (src + 4)
    assert -(1 << 24) <= off < (1 << 24) and off % 2 == 0, hex(off)
    v = off & 0x1ffffff
    s = (v >> 24) & 1
    i1 = (v >> 23) & 1
    i2 = (v >> 22) & 1
    imm10 = (v >> 12) & 0x3ff
    imm11 = (v >> 1) & 0x7ff
    j1 = (1 - i1) ^ s
    j2 = (1 - i2) ^ s
    hw1 = 0xF000 | (s << 10) | imm10
    hw2 = 0x9000 | (j1 << 13) | (j2 << 11) | imm11
    return struct.pack("<HH", hw1, hw2)


def sym(elf, name):
    d = open(elf, "rb").read()
    shoff, = struct.unpack_from("<I", d, 0x20)
    shentsize, shnum = struct.unpack_from("<HH", d, 0x2e)
    secs = [struct.unpack_from("<IIIIIIIIII", d, shoff + i * shentsize) for i in range(shnum)]
    for s in secs:
        if s[1] == 2:  # SHT_SYMTAB
            strtab = secs[s[6]]
            for o in range(s[4], s[4] + s[5], 16):
                nm, val = struct.unpack_from("<II", d, o)
                n = d[strtab[4] + nm:d.index(b"\0", strtab[4] + nm)].decode()
                if n == name:
                    return val & ~1
    raise KeyError(name)


if __name__ == "__main__":
    if sys.argv[1] == "--bl":
        print(bl(int(sys.argv[2], 0), int(sys.argv[3], 0)).hex())
    elif sys.argv[1] == "--sym":
        print(hex(sym(sys.argv[2], sys.argv[3])))
    else:
        print(bw(int(sys.argv[1], 0), int(sys.argv[2], 0)).hex())
