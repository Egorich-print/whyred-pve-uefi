#!/usr/bin/env python3
"""Analyze Android devinfo partition dumps for unlock-flag structures.

Usage: analyze-devinfo.py <dump.bin> [more.bin ...]
Looks for: known magics, ASCII lock-related strings, non-zero regions.
"""
import re
import sys


def nonzero_regions(data: bytes, min_run=16):
    """Yield (start, end) of contiguous non-zero runs >= min_run."""
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
    b"is_unlocked", b"unlocked", b"locked", b"IsUnlock", b"UNLOCK",
    b"devinfo", b"DEVINFO", b"lock_state", b"unlock_ability", b"bootloader",
    b"orange", b"green", b"yellow", b"red", b"verifiedbootstate",
    b"antirollback", b"anti", b"ROLLBACK", b"flash", b"secure",
    b"CHROMEOS", b"CR50", b"hlos", b"HLOS",
]


def main(paths):
    for path in paths:
        data = open(path, "rb").read()
        print(f"\n===== {path} ({len(data)} bytes) =====")

        # 1. magic candidates in first sector
        print("\n-- first 256 bytes --")
        hexdump(data[:256])

        # 2. keyword hits with context
        print("\n-- keyword hits --")
        seen = set()
        for kw in KEYWORDS:
            for m in re.finditer(re.escape(kw), data, re.IGNORECASE):
                pos = m.start()
                bucket = pos & ~0xF
                key = (kw.lower(), bucket)
                if key in seen:
                    continue
                seen.add(key)
                ctx = data[max(0, pos - 32) : pos + 64]
                asc = "".join(chr(b) if 32 <= b < 127 else "." for b in ctx)
                print(f"{pos:#010x} [{kw.decode():20}] {asc}")

        # 3. non-zero region map (partition is 8 MiB; most is padding)
        regions = nonzero_regions(data)
        print(f"\n-- non-zero regions (>=16B): {len(regions)} --")
        total = sum(e - s for s, e in regions)
        print(f"   total non-zero: {total} bytes")
        for s, e in regions[:40]:
            print(f"   {s:#010x}-{e:#010x} ({e - s} B)")
        if len(regions) > 40:
            print("   ...")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    main(sys.argv[1:])
