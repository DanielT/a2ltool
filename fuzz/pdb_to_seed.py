#!/usr/bin/env python3
"""Convert a PDB file into the structured input format used by the fuzz_pdb target.

The output is the content of each MSF stream, separated by b"\\xffMSF" (see
PDB_STREAM_SEP in src/fuzz.rs). The fuzz harness rebuilds a valid MSF container
around these streams.

usage: pdb_to_seed.py input.pdb output
"""
import struct
import sys

MAGIC = b"Microsoft C/C++ MSF 7.00\r\n\x1a\x44\x53\x00\x00\x00"
SEP = b"\xffMSF"


def read_streams(data):
    if not data.startswith(MAGIC):
        sys.exit("not an MSF 7.00 file")
    page_size, _fpm, _num_pages, dir_size, _ = struct.unpack_from("<5I", data, 32)

    def pages(page_list, size):
        return b"".join(data[p * page_size:(p + 1) * page_size] for p in page_list)[:size]

    dir_pages = -(-dir_size // page_size)
    map_pages = struct.unpack_from(f"<{-(-dir_pages * 4 // page_size)}I", data, 52)
    dir_page_list = struct.unpack_from(f"<{dir_pages}I", pages(map_pages, dir_pages * 4))
    directory = pages(dir_page_list, dir_size)

    count = struct.unpack_from("<I", directory)[0]
    sizes = struct.unpack_from(f"<{count}I", directory, 4)
    pos = 4 + 4 * count
    streams = []
    for size in sizes:
        if size == 0xFFFFFFFF:
            size = 0
        n = -(-size // page_size)
        streams.append(pages(struct.unpack_from(f"<{n}I", directory, pos), size))
        pos += 4 * n
    return streams


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    with open(sys.argv[1], "rb") as f:
        streams = read_streams(f.read())
    if any(SEP in s for s in streams):
        print("warning: a stream contains the separator; the seed will be split there")
    with open(sys.argv[2], "wb") as f:
        f.write(SEP.join(streams))


if __name__ == "__main__":
    main()
