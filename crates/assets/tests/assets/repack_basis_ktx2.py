#!/usr/bin/env python3
"""Repack a one-image, opaque ETC1S or UASTC `.basis` file into KTX2, copying every payload unchanged.

`ramp_etc1s.ktx2` and `ramp_uastc.ktx2` come from the crates.io `basis-universal` 0.3.1 encoder
(basisu 1.16), whose bindings write `.basis` only: a 32x32 RGBA ramp (`encoded_ramp` in
`crates/assets/src/basis.rs`), `ColorSpace::Srgb`, `set_generate_mipmaps(true)`, default quality, then
`python3 repack_basis_ktx2.py ramp_etc1s.basis ramp_etc1s.ktx2`.
"""

import struct
import sys
from pathlib import Path

KTX2_MAGIC = b"\xabKTX 20\xbb\r\n\x1a\n"
BASIS_SIGNATURE = 0x4273
ETC1S, UASTC = 0, 1
HAS_ALPHA_SLICES = 4


def u16(data: bytes, offset: int) -> int:
    return struct.unpack_from("<H", data, offset)[0]


def u24(data: bytes, offset: int) -> int:
    return data[offset] | data[offset + 1] << 8 | data[offset + 2] << 16


def u32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def read_levels(data: bytes) -> list[dict]:
    """The slices of image 0, level 0 first."""
    slices = []
    for index in range(u24(data, 14)):
        at = u32(data, 65) + 23 * index
        if u24(data, at) != 0:
            sys.exit("only single-image .basis files are repacked")
        start = u32(data, at + 13)
        slices.append(
            {
                "level": data[at + 3],
                "width": u16(data, at + 5),
                "height": u16(data, at + 7),
                "data": data[start : start + u32(data, at + 17)],
            }
        )
    slices.sort(key=lambda level: level["level"])
    return slices


def dfd(etc1s: bool) -> bytes:
    """A 44-byte basic DFD: BT.709 primaries, sRGB transfer, one RGB sample over a 4x4 block."""
    model, plane_bytes, sample_bits = (163, 0, 63) if etc1s else (166, 16, 127)
    words = [44, 0, 2 | 40 << 16, model | 1 << 8 | 2 << 16, 0x0303, plane_bytes, 0, sample_bits << 16, 0, 0, 0xFFFFFFFF]
    return struct.pack("<11I", *words)


def basislz_global_data(data: bytes, levels: list[dict]) -> bytes:
    """BasisLZ global data: counts and lengths, one image descriptor per level, then the codebooks and tables."""
    endpoints = data[u32(data, 41) : u32(data, 41) + u24(data, 45)]
    selectors = data[u32(data, 50) : u32(data, 50) + u24(data, 54)]
    tables = data[u32(data, 57) : u32(data, 57) + u32(data, 61)]
    out = struct.pack("<HHIIII", u16(data, 39), u16(data, 48), len(endpoints), len(selectors), len(tables), 0)
    for level in levels:
        out += struct.pack("<5I", 0, 0, len(level["data"]), 0, 0)
    return out + endpoints + selectors + tables


def repack(data: bytes) -> bytes:
    if u16(data, 0) != BASIS_SIGNATURE:
        sys.exit("not a .basis file")
    if data[20] not in (ETC1S, UASTC) or u16(data, 21) & HAS_ALPHA_SLICES:
        sys.exit("only opaque ETC1S and UASTC LDR are repacked")
    etc1s = data[20] == ETC1S
    levels = read_levels(data)
    descriptor = dfd(etc1s)
    dfd_offset = 80 + 24 * len(levels)
    end = dfd_offset + len(descriptor)
    global_data = basislz_global_data(data, levels) if etc1s else b""
    sgd_offset = (end + 7) & ~7 if global_data else 0
    end = sgd_offset + len(global_data) if global_data else end
    align = 1 if etc1s else 16
    offsets = {}
    # KTX2 stores the smallest level first.
    for level in reversed(levels):
        end = (end + align - 1) // align * align
        offsets[level["level"]] = end
        end += len(level["data"])
    out = bytearray(end)
    out[:12] = KTX2_MAGIC
    base = levels[0]
    struct.pack_into("<9I", out, 12, 0, 1, base["width"], base["height"], 0, 0, 1, len(levels), 1 if etc1s else 0)
    struct.pack_into("<4I2Q", out, 48, dfd_offset, len(descriptor), 0, 0, sgd_offset, len(global_data))
    for level in levels:
        length, at = len(level["data"]), offsets[level["level"]]
        struct.pack_into("<3Q", out, 80 + 24 * level["level"], at, length, 0 if etc1s else length)
        out[at : at + length] = level["data"]
    out[dfd_offset : dfd_offset + len(descriptor)] = descriptor
    out[sgd_offset : sgd_offset + len(global_data)] = global_data
    return bytes(out)


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit("usage: repack_basis_ktx2.py <in.basis> <out.ktx2>")
    Path(sys.argv[2]).write_bytes(repack(Path(sys.argv[1]).read_bytes()))


if __name__ == "__main__":
    main()
