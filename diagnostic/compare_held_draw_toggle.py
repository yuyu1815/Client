#!/usr/bin/env python3
"""Measure same-client first-person on/off PNGs without alignment or correction."""
from __future__ import annotations
import argparse, hashlib, json
from pathlib import Path
from compare_item_overlay import crop, png_read, png_write


def compare(on_path: Path, off_path: Path, diff_path: Path):
    on = png_read(on_path); off = png_read(off_path)
    if on[:2] != off[:2]:
        raise ValueError(f"dimension mismatch: on={on[:2]} off={off[:2]}")
    width, height = on[:2]
    changed = []; max_channel = total = 0
    diff = []
    for y in range(height):
        row = []
        for x in range(width):
            a, b = on[2][y][x], off[2][y][x]
            delta = tuple(abs(a[c] - b[c]) for c in range(3))
            if any(delta): changed.append((x, y))
            max_channel = max(max_channel, *delta)
            total += sum(delta)
            row.append(delta)
        diff.append(row)
    bbox = None if not changed else [min(x for x, _ in changed), min(y for _, y in changed), max(x for x, _ in changed) + 1, max(y for _, y in changed) + 1]
    roi = [800, 450, min(480, width - 800), min(270, height - 450)]
    roi_on, roi_off = crop(on, roi), crop(off, roi)
    roi_pixels = len(roi_on) * len(roi_on[0])
    roi_changed = sum(a != b for ar, br in zip(roi_on, roi_off) for a, b in zip(ar, br))
    diff_path.parent.mkdir(parents=True, exist_ok=True)
    png_write(diff_path, width, height, diff)
    return {
        "status": "measured-png-difference" if changed else "pixel-identical",
        "size": [width, height], "onPngSha256": hashlib.sha256(on_path.read_bytes()).hexdigest(),
        "offPngSha256": hashlib.sha256(off_path.read_bytes()).hexdigest(),
        "changedPixelCount": len(changed), "changedPixelFraction": len(changed) / (width * height),
        "changedPixelBboxXYXY": bbox, "maxChannelDifference": max_channel,
        "sumAbsoluteRgbDifference": total, "roiXYWH": roi, "roiChangedPixelCount": roi_changed,
        "roiPixels": roi_pixels, "roiChangedPixelFraction": roi_changed / roi_pixels if roi_pixels else None,
        "metric": "raw 8-bit RGB byte difference; same-client frames, no alignment/correction/mask",
        "diffImage": str(diff_path),
    }


def main():
    p = argparse.ArgumentParser(); p.add_argument("--on", type=Path, required=True); p.add_argument("--off", type=Path, required=True)
    p.add_argument("--diff", type=Path, required=True); p.add_argument("--report", type=Path, required=True); a = p.parse_args()
    report = compare(a.on, a.off, a.diff)
    a.report.parent.mkdir(parents=True, exist_ok=True); a.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: report[k] for k in ("status", "changedPixelCount", "changedPixelBboxXYXY", "maxChannelDifference", "roiChangedPixelCount", "roiChangedPixelFraction")}))

if __name__ == "__main__": main()
