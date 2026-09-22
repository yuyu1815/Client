#!/usr/bin/env python3
"""Compare actual Java ItemTintSource samples with Rust item mesh tint payloads."""
from __future__ import annotations
import json
import sys
from pathlib import Path

EXPECTED = {
    "fern": ([124, 189, 107], "grass"),
    "bush": ([124, 189, 107], "grass"),
    "lily_pad": ([113, 195, 92], "constant"),
    "sugar_cane": ([255, 255, 255], "untinted"),
    "pink_petals": ([255, 255, 255], "untinted"),
    "wildflowers": ([255, 255, 255], "untinted"),
}


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: verify_item_tint.py JAVA_RENDER_DEBUG.json RUST_RENDER_DEBUG.json", file=sys.stderr)
        return 2
    java = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    rust = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
    java_rows = {row["item"]: row for row in java["itemTintDiagnostics"]}
    rust_rows = {row["item"]: row for row in rust["itemTintDiagnostics"]["candidates"]}
    failures = []
    for name, (rgb, source) in EXPECTED.items():
        j = java_rows.get(name, {})
        r = rust_rows.get(name, {})
        jrgb = [j.get("color", {}).get(channel) for channel in ("red", "green", "blue")]
        rrgb = r.get("itemTint", {}).get("rgb")
        packed = r.get("meshUpload", {}).get("packedRgb", [])
        if jrgb != rgb or rrgb != rgb or packed != [rgb]:
            failures.append(f"{name}: java={jrgb} rust={rrgb} packed={packed} expected={rgb}")
        if source not in r.get("itemTint", {}).get("source", ""):
            failures.append(f"{name}: source={r.get('itemTint', {}).get('source')} expected={source}")
        if "no block color source" not in r.get("provenance", "").lower():
            failures.append(f"{name}: block tint provenance leaked")
    if failures:
        print("item tint verification: FAIL")
        print("\n".join(failures))
        return 1
    print("item tint verification: PASS")
    print("candidates=6; rgb/source/packed upload exact; blockTintSource=not used")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
