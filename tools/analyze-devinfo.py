#!/usr/bin/env python3
"""Heuristic survey of an Android `devinfo` partition dump.

Usage: analyze-devinfo.py <dump.bin> [more.bin ...]

This tool only PRINTS observations. It does not know the device_info layout
and does not locate patch targets: a keyword hit or a non-zero run is a place
to look, never an offset to write. Confirm any field against the Qualcomm LK
`struct device_info` (aboot.c) and against a stock dump of the same model
before touching bytes.
"""
import sys


def nonzero_regions(data: bytes, min_run=16):
    out = []
    start = None
    for i, b in enumerate(data):
        if b != 0 and start is None:
            start = i
        elif b == 0 and start is not None:
            if i - start >= min_run:
                out.append((start, i))
            start = None
    if start is not None and len(data) - start >= min_run:
        out.append((start, len(data)))
    return out


def hexdump(data: bytes, base: int = 0, limit: int = 512):
    for off in range(0, min(len(data), limit), 16):
        chunk = data[off : off + 16]
        hexs = " ".join(f"{b:02x}" for b in chunk)
        asc = "".join(chr(b) if 32 <= b < 127 else "." for b in chunk)
        print(f"{base + off:08x}  {hexs:<48}  |{asc}|")


KEYWORDS = [
    b"is_unlocked",
    b"unlock_ability",
    b"lock_state",
    b"verifiedbootstate",
    b"antirollback",
    b"devinfo",
    b"bootloader",
    b"unlocked",
    b"locked",
]


def keyword_hits(data: bytes):
    out = []
    for kw in KEYWORDS:
        low = data.lower()
        start = 0
        while True:
            pos = low.find(kw, start)
            if pos < 0:
                break
            out.append((pos, kw))
            start = pos + 1
    return sorted(out)


def main(paths):
    for path in paths:
        data = open(path, "rb").read()
        print(f"\n===== {path} ({len(data)} bytes) =====")

        print("\n-- first 256 bytes --")
        hexdump(data[:256])

        print("\n-- keyword hits (context only, NOT patch offsets) --")
        hits = keyword_hits(data)
        if not hits:
            print("   none")
        for pos, kw in hits[:80]:
            ctx = data[max(0, pos - 32) : pos + 64]
            asc = "".join(chr(b) if 32 <= b < 127 else "." for b in ctx)
            print(f"   {pos:#010x} {kw.decode():16} |{asc}|")
        if len(hits) > 80:
            print(f"   ... {len(hits) - 80} more")

        regions = nonzero_regions(data)
        total = sum(e - s for s, e in regions)
        print(f"\n-- non-zero regions (>=16B): {len(regions)}, {total} bytes --")
        for s, e in regions[:40]:
            print(f"   {s:#010x}-{e:#010x} ({e - s} B)")
        if len(regions) > 40:
            print("   ...")
        print("\nNothing above identifies a writable field. Cross-check the LK")
        print("device_info layout and a stock dump before patching anything.")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    main(sys.argv[1:])
