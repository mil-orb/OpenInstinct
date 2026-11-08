"""Flatten an ARM ELF32 payload to raw bytes and print its symbols.

Usage: python -I elf2bin.py IN.elf OUT.bin
Writes the PT_LOAD segments as one contiguous blob starting at the lowest
load address, and prints that address, the size, and the global symbols.
"""
import struct
import sys


def main():
    elf = open(sys.argv[1], "rb").read()
    assert elf[:4] == b"\x7fELF" and elf[4] == 1, "need ELF32"
    (e_phoff, e_shoff, _, _, e_phentsize, e_phnum, e_shentsize, e_shnum, e_shstrndx) = \
        struct.unpack_from("<IIIHHHHHH", elf, 0x1c)
    segs = []
    for i in range(e_phnum):
        p_type, p_off, p_vaddr, p_paddr, p_filesz, p_memsz, p_flags, _ = \
            struct.unpack_from("<8I", elf, e_phoff + i * e_phentsize)
        if p_type == 1 and p_memsz:
            assert p_filesz == p_memsz, "segment with bss-like tail"
            segs.append((p_paddr, elf[p_off:p_off + p_filesz]))
    segs.sort()
    base = segs[0][0]
    end = max(a + len(d) for a, d in segs)
    out = bytearray(b"\xff" * (end - base))
    for a, d in segs:
        out[a - base:a - base + len(d)] = d
    open(sys.argv[2], "wb").write(out)
    print("base %#x size %#x (%d bytes)" % (base, len(out), len(out)))

    secs = [struct.unpack_from("<10I", elf, e_shoff + i * e_shentsize) for i in range(e_shnum)]
    for s in secs:
        if s[1] == 2:  # SHT_SYMTAB
            strtab = secs[s[6]]
            for j in range(s[5] // 16):
                name, value, size, info, _, shndx = struct.unpack_from("<IIIBBH", elf, s[4] + j * 16)
                if info >> 4 == 1 and shndx:  # global, defined
                    nm = elf[strtab[4] + name:elf.index(b"\0", strtab[4] + name)].decode()
                    print("  %08x %5d %s" % (value, size, nm))


if __name__ == "__main__":
    main()
