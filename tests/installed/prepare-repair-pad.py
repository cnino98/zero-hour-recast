#!/usr/bin/env python3
"""Make the declared Repair Pad custom map from an owned MapsZH.big."""

import argparse
import hashlib
from pathlib import Path
import struct
import tempfile


PREVIOUS_FIXTURE_SHA256 = "6cb9ed2746b52905799864a21e78b497347a2517d589b13b6747335ca87e37c8"


def read_map(archive):
    with archive.open("rb") as stream:
        header = stream.read(16)
        if header[:4] not in (b"BIGF", b"BIG4"):
            raise ValueError("Expected a Zero Hour BIG archive")
        count, = struct.unpack_from(">I", header, 8)
        for _ in range(count):
            offset, size = struct.unpack(">II", stream.read(8))
            name = bytearray()
            while (byte := stream.read(1)) != b"\0":
                if not byte:
                    raise ValueError("Unterminated BIG archive entry")
                name.extend(byte)
            if name.lower() == b"maps\\hostile dawn\\hostile dawn.map":
                stream.seek(offset)
                return stream.read(size)
    raise ValueError("Original Hostile Dawn map is absent")


def decode_map(data):
    original = "de79d1709bdadbf420166d21540efc5d5494847dbe53bcfeae8f5f92717b62db"
    if hashlib.sha256(data).hexdigest() != original:
        raise ValueError("Hostile Dawn differs from the frozen original")
    cursor = 13  # Frozen map uses EAR with the five-byte RefPack header.
    output = bytearray()
    while cursor < len(data):
        first = data[cursor]
        cursor += 1
        distance = length = 0
        if first < 0x80:
            second = data[cursor]
            cursor += 1
            literals = first & 3
            distance = ((first & 0x60) << 3) + second + 1
            length = ((first & 0x1c) >> 2) + 3
        elif first < 0xc0:
            second, third = data[cursor:cursor + 2]
            cursor += 2
            literals = second >> 6
            distance = ((second & 0x3f) << 8) + third + 1
            length = (first & 0x3f) + 4
        elif first < 0xe0:
            second, third, fourth = data[cursor:cursor + 3]
            cursor += 3
            literals = first & 3
            distance = ((first & 0x10) << 12) + (second << 8) + third + 1
            length = ((first & 0x0c) << 6) + fourth + 5
        elif first < 0xfc:
            literals = ((first & 0x1f) << 2) + 4
        else:
            literals = first & 3
        output.extend(data[cursor:cursor + literals])
        cursor += literals
        for _ in range(length):
            output.append(output[-distance])
        if first >= 0xfc:
            break
    if cursor != len(data) or len(output) != struct.unpack_from("<I", data, 4)[0]:
        raise ValueError("Hostile Dawn compression lengths disagree")
    return output


def clear_official_title(data):
    count, = struct.unpack_from("<I", data, 4)
    cursor = 8
    symbols = {}
    for _ in range(count):
        length = data[cursor]
        cursor += 1
        name = bytes(data[cursor:cursor + length])
        cursor += length
        symbol, = struct.unpack_from("<I", data, cursor)
        cursor += 4
        symbols[symbol] = name
    title_symbol, = (key for key, name in symbols.items() if name == b"mapName")
    title = b"MAP:HostileDawn"
    field = struct.pack("<IH", title_symbol << 8 | 3, len(title)) + title
    while cursor < len(data):
        symbol, version, size = struct.unpack_from("<IHI", data, cursor)
        start = cursor + 10
        end = start + size
        if end > len(data):
            raise ValueError("Hostile Dawn chunk exceeds map length")
        if symbols[symbol] == b"WorldInfo":
            payload = data[start:end]
            if version != 1 or payload.count(field) != 1:
                raise ValueError("Expected one original Hostile Dawn title in WorldInfo")
            # An empty mapName selects the custom filename in the chooser.
            updated = payload.replace(field, field[:4] + b"\0\0", 1)
            struct.pack_into("<I", data, cursor + 6, len(updated))
            data[start:end] = updated
            return
        cursor = end
    raise ValueError("Hostile Dawn has no WorldInfo chunk")


def build_fixture(data):
    original = struct.pack("<ffffIH", 579.785888671875, 2922.479248046875, 0,
                           -0.7853981852531433, 0, len(b"TechRepairbay")) + b"TechRepairbay"
    if not data.startswith(b"CkMp") or data.count(original) != 1:
        raise ValueError("Expected exactly one original Hostile Dawn Repair Bay placement")
    offset = data.index(original) + len(original) - len(b"TechRepairbay")
    # Same template-name length preserves all native chunk sizes and other records.
    data[offset:offset + len(b"TechRepairbay")] = b"TechRepairPad"
    clear_official_title(data)
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--maps-archive", type=Path, required=True)
    parser.add_argument("--runtime-root", type=Path, required=True)
    args = parser.parse_args()
    root = args.runtime_root.resolve()
    repository = Path(__file__).resolve().parents[2]
    if root == repository or repository in root.parents:
        parser.error("--runtime-root must be outside the repository")
    destination = root / "UserData/Maps/Repair Pad Fixture/Repair Pad Fixture.map"
    fixture = build_fixture(decode_map(read_map(args.maps_archive)))
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.is_symlink():
        raise ValueError(f"Expected a regular fixture file: {destination}")
    if destination.exists():
        existing = destination.read_bytes()
        if existing != fixture:
            if hashlib.sha256(existing).hexdigest() != PREVIOUS_FIXTURE_SHA256:
                raise ValueError(f"Existing fixture differs: {destination}")
            with tempfile.TemporaryDirectory(dir=destination.parent) as directory:
                temporary = Path(directory) / destination.name
                temporary.write_bytes(fixture)
                temporary.replace(destination)
    else:
        with destination.open("xb") as stream:
            stream.write(fixture)
    print(f"Prepared declared Repair Pad fixture at {destination}")


if __name__ == "__main__":
    main()
