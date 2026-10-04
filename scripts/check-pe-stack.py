#!/usr/bin/env python3
"""Assert a Windows x64 PE executable reserves at least the requested stack."""
import os
import struct
import sys


path = sys.argv[1]
minimum = int(sys.argv[2]) if len(sys.argv) > 2 else 4 * 1024 * 1024
size = os.path.getsize(path)
with open(path, "rb") as exe:
    assert size >= 0x40, f"truncated DOS header: {size} bytes"
    assert exe.read(2) == b"MZ", "missing DOS MZ signature"
    exe.seek(0x3C)
    pe_offset = struct.unpack("<I", exe.read(4))[0]
    assert 0x40 <= pe_offset <= size - 24, f"invalid PE offset: {pe_offset}"
    exe.seek(pe_offset)
    assert exe.read(4) == b"PE\0\0", "missing PE signature"
    machine, _, _, _, _, optional_size, _ = struct.unpack("<HHIIIHH", exe.read(20))
    assert machine == 0x8664, f"expected x86-64 PE, got 0x{machine:04x}"
    assert optional_size >= 0x58, f"short PE32+ optional header: {optional_size} bytes"
    assert pe_offset + 24 + optional_size <= size, "optional header exceeds file bounds"
    magic = struct.unpack("<H", exe.read(2))[0]
    assert magic == 0x20B, f"expected PE32+ optional header, got 0x{magic:04x}"
    exe.seek(pe_offset + 24 + 0x48)
    reserve, commit = struct.unpack("<QQ", exe.read(16))
    assert reserve >= minimum, f"stack reserve {reserve} is below {minimum}"

print(f"{path}: machine=0x{machine:04x} magic=0x{magic:04x} stack_reserve={reserve} stack_commit={commit} (minimum={minimum})")
