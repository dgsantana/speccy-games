"""Decoding a .z80 snapshot into a flat 64K image.

Shared by the Jet Set Willy and Jet Set Willy II generators. Only version 1
snapshots are handled, which is what the common dumps of both games are: a
30-byte header then the 48K of RAM, optionally run-length compressed.
"""

import struct
import sys


def load_z80(path):
    """A 65536-byte image of the machine as the snapshot left it."""
    data = open(path, "rb").read()
    if struct.unpack("<H", data[6:8])[0] == 0:
        sys.exit(f"{path} is a version 2 or 3 snapshot, which this does not read")

    body = data[30:]
    if (data[12] >> 5) & 1:
        if body[-4:] == b"\x00\xed\xed\x00":
            body = body[:-4]
        ram = bytearray()
        index = 0
        while index < len(body):
            if body[index] == 0xED and index + 1 < len(body) and body[index + 1] == 0xED:
                ram += bytes([body[index + 3]]) * body[index + 2]
                index += 4
            else:
                ram.append(body[index])
                index += 1
    else:
        ram = bytearray(body)

    memory = bytearray(65536)
    memory[16384:16384 + len(ram)] = ram[:49152]
    return memory


def word(memory, addr):
    """The little-endian word at `addr`."""
    return memory[addr] | (memory[addr + 1] << 8)
