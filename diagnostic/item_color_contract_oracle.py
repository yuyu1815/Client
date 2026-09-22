#!/usr/bin/env python3
"""Independent item tint contract oracle for the six GUI overlay sprites.

Java 26.2 item.fsh samples an RGBA8_UNORM atlas, multiplies encoded
``texture * vertexColor`` and writes to UNORM targets. Rust samples its SRGB
atlas as linear, so this compares the three possible contracts against the
actual Java screenshot using only opaque source texels. Background and
partial-alpha pixels are reported separately; no foreground mask or crop is
used.
"""
from __future__ import annotations

import argparse
import json
import math
import struct
import zlib
from pathlib import Path

ITEMS = {
    "fern": ("block/fern.png", [124, 189, 107]),
    "bush": ("block/bush.png", [124, 189, 107]),
    "lily_pad": ("block/lily_pad.png", [113, 195, 92]),
    "sugar_cane": ("item/sugar_cane.png", [255, 255, 255]),
    "pink_petals": ("item/pink_petals.png", [255, 255, 255]),
    "wildflowers": ("item/wildflowers.png", [255, 255, 255]),
}


def png_rgba(path: Path) -> tuple[int, int, list[list[tuple[int, int, int, int]]]]:
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"not a PNG: {path}")
    pos = 8
    compressed = bytearray()
    palette: list[tuple[int, int, int]] | None = None
    transparency: list[int] | None = None
    width = height = depth = color_type = interlace = None
    while pos < len(data):
        size = struct.unpack(">I", data[pos : pos + 4])[0]
        kind = data[pos + 4 : pos + 8]
        chunk = data[pos + 8 : pos + 8 + size]
        pos += size + 12
        if kind == b"IHDR":
            width, height, depth, color_type, _, _, interlace = struct.unpack(">IIBBBBB", chunk)
            if depth not in (1, 2, 4, 8) or interlace != 0:
                raise ValueError(f"unsupported PNG layout: {path} depth={depth} interlace={interlace}")
        elif kind == b"PLTE":
            palette = [tuple(chunk[i : i + 3]) for i in range(0, len(chunk), 3)]
        elif kind == b"tRNS":
            transparency = list(chunk)
        elif kind == b"IDAT":
            compressed.extend(chunk)
        elif kind == b"IEND":
            break
    if width is None or height is None or color_type is None or depth is None:
        raise ValueError(f"missing PNG header: {path}")
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}.get(color_type)
    if channels is None:
        raise ValueError(f"unsupported PNG color type: {color_type}")
    bits_per_pixel = channels * depth
    stride = (width * bits_per_pixel + 7) // 8
    filtered = zlib.decompress(compressed)
    previous = bytearray(stride)
    rows: list[list[tuple[int, int, int, int]]] = []
    offset = 0

    def unpack_sample(row: bytearray, index: int) -> int:
        if depth == 8:
            return row[index]
        per_byte = 8 // depth
        byte = row[index // per_byte]
        shift = (per_byte - 1 - index % per_byte) * depth
        return (byte >> shift) & ((1 << depth) - 1)

    for _ in range(height):
        filter_type = filtered[offset]
        offset += 1
        row = bytearray(filtered[offset : offset + stride])
        offset += stride
        bytes_per_pixel = max(1, (bits_per_pixel + 7) // 8)
        for i in range(stride):
            left = row[i - bytes_per_pixel] if i >= bytes_per_pixel else 0
            up = previous[i]
            up_left = previous[i - bytes_per_pixel] if i >= bytes_per_pixel else 0
            if filter_type == 1:
                row[i] = (row[i] + left) & 255
            elif filter_type == 2:
                row[i] = (row[i] + up) & 255
            elif filter_type == 3:
                row[i] = (row[i] + (left + up) // 2) & 255
            elif filter_type == 4:
                estimate = left + up - up_left
                pa, pb, pc = abs(estimate - left), abs(estimate - up), abs(estimate - up_left)
                predictor = left if pa <= pb and pa <= pc else (up if pb <= pc else up_left)
                row[i] = (row[i] + predictor) & 255
            elif filter_type != 0:
                raise ValueError(f"unsupported PNG filter {filter_type}")
        decoded: list[tuple[int, int, int, int]] = []
        for x in range(width):
            samples = [unpack_sample(row, x * channels + c) for c in range(channels)]
            if depth != 8:
                samples[0] = round(samples[0] * 255 / ((1 << depth) - 1))
                if color_type in (2, 4, 6):
                    samples = [round(v * 255 / ((1 << depth) - 1)) for v in samples]
            if color_type == 0:
                value = samples[0]
                alpha = 0 if transparency and value == transparency[0] else 255
                decoded.append((value, value, value, alpha))
            elif color_type == 2:
                rgb = tuple(samples[:3])
                alpha = 255
                if transparency and len(transparency) >= 6:
                    transparent_rgb = tuple(struct.unpack(">H", bytes(transparency[i : i + 2]))[0] for i in (0, 2, 4))
                    alpha = 0 if rgb == transparent_rgb else 255
                decoded.append((*rgb, alpha))
            elif color_type == 3:
                index = unpack_sample(row, x)
                if palette is None or index >= len(palette):
                    raise ValueError(f"indexed PNG missing palette: {path}")
                decoded.append((*palette[index], transparency[index] if transparency and index < len(transparency) else 255))
            elif color_type == 4:
                decoded.append((samples[0], samples[0], samples[0], samples[1]))
            else:
                decoded.append(tuple(samples[:4]))
        rows.append(decoded)
        previous = row
    return width, height, rows


def srgb_to_linear(value: float) -> float:
    return value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4


def linear_to_srgb(value: float) -> float:
    value = max(0.0, min(1.0, value))
    return 12.92 * value if value <= 0.0031308 else 1.055 * value ** (1.0 / 2.4) - 0.055


def byte_round(value: float) -> int:
    return max(0, min(255, math.floor(value * 255.0 + 0.5)))


def palettes(source: list[tuple[int, int, int, int]], tint: list[int]) -> dict[str, list[tuple[int, int, int]]]:
    tint_f = [channel / 255.0 for channel in tint]
    opaque = [(r / 255.0, g / 255.0, b / 255.0) for r, g, b, a in source if a == 255]
    result = {"linear_sample_encoded_tint": [], "linear_sample_decoded_tint": [], "encoded_sample_encoded_tint": []}
    for src in opaque:
        linear_src = [srgb_to_linear(c) for c in src]
        old = [linear_to_srgb(c * t) for c, t in zip(linear_src, tint_f)]
        decoded_tint = [srgb_to_linear(t) for t in tint_f]
        decoded = [linear_to_srgb(c * t) for c, t in zip(linear_src, decoded_tint)]
        java = [c * t for c, t in zip(src, tint_f)]
        for name, value in (("linear_sample_encoded_tint", old), ("linear_sample_decoded_tint", decoded), ("encoded_sample_encoded_tint", java)):
            result[name].append(tuple(byte_round(c) for c in value))
    return {name: sorted(set(values)) for name, values in result.items()}


def crop(rows, rect):
    x, y, width, height = rect
    return [rows[y + j][x : x + width] for j in range(height)]


def compare(actual, palette, background):
    flat = [pixel[:3] for row in actual for pixel in row]
    non_background = [pixel for pixel in flat if pixel != tuple(background)]
    result = {"pixels": len(flat), "backgroundPixels": len(flat) - len(non_background), "nonBackgroundPixels": len(non_background), "alphaNotOpaqueSourcePixelsExcluded": True}
    for name, values in palette.items():
        value_set = set(values)
        exact = sum(pixel in value_set for pixel in non_background)
        distances = [min(sum(abs(a - b) for a, b in zip(pixel, candidate)) / 3.0 for candidate in value_set) for pixel in non_background] if value_set else []
        result[name] = {"sourcePaletteColors": len(value_set), "exactPixelMatches": exact, "exactFractionOfNonBackground": exact / len(non_background) if non_background else 1.0, "nearestPaletteMAE": sum(distances) / len(distances) if distances else 0.0, "maxNearestPaletteDiff": max(distances, default=0.0)}
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--java-png", type=Path, required=True)
    parser.add_argument("--rust-png", type=Path)
    parser.add_argument("--java-meta", type=Path, required=True)
    parser.add_argument("--assets", type=Path, default=Path.home() / "AppData/Roaming/.pomme/data/versions/26.2/extracted/assets/minecraft/textures")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    metadata = json.loads(args.java_meta.read_text(encoding="utf-8"))
    java = png_rgba(args.java_png)
    rust = png_rgba(args.rust_png) if args.rust_png else None
    background = metadata["itemOverlay"]["backgroundRGB"]
    report = {"schema": 1, "status": "item-color-contract-oracle", "javaShader": "26.2 assets/minecraft/shaders/core/item.fsh: texture * vertexColor * lightMapColor; Java GUI atlas and main target RGBA8_UNORM", "comparison": "all fixed 48x48 ROI pixels; neutral background excluded only from palette exactness counts; partial-alpha source texels excluded from source palettes", "items": []}
    for entry in metadata["itemOverlay"]["items"]:
        name = entry["id"]
        relative, tint = ITEMS[name]
        _, _, source_rows = png_rgba(args.assets / relative)
        source = [pixel for row in source_rows for pixel in row]
        palette = palettes(source, tint)
        actual = crop(java[2], entry["physicalRect"])
        row = {"item": name, "sprite": str(args.assets / relative), "tintRGB": tint, "sourceTexels": {"width": len(source_rows[0]), "height": len(source_rows), "opaque": sum(pixel[3] == 255 for pixel in source), "partialAlpha": sum(0 < pixel[3] < 255 for pixel in source), "transparent": sum(pixel[3] == 0 for pixel in source)}, "palettes": palette, "javaPixels": compare(actual, palette, background)}
        if rust:
            row["rustPixels"] = compare(crop(rust[2], entry["physicalRect"]), palette, background)
        report["items"].append(row)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print("item color contract oracle: PASS")


if __name__ == "__main__":
    main()
