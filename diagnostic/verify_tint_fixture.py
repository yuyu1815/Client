"""Strictly verify actual-world TintSource -> Rust emitted/upload bytes.

A candidate is verified only when the fixture input, actual Java sample state,
actual runtime color fields, selected Rust trace target, and every traced raw
RGB byte agree. Source-only discovery remains diagnostic-only and exits 1.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any

CANDIDATES = {"potted_fern", "bush", "sugar_cane", "lily_pad", "pink_petals", "wildflowers"}
ACTUAL_SAMPLE_PROVENANCE = "actual ClientLevel state at sampled world position"
ROOT = Path(__file__).resolve().parents[2]


def load(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8-sig"))


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest().upper()


def case_name(root: Path) -> str:
    cases = load(root / "run-manifest.json")["cases"]
    return cases[0] if isinstance(cases, list) else cases


def rgb(node: dict[str, Any]) -> list[int]:
    color = node.get("colorInWorld")
    if not isinstance(color, dict) or not all(k in color for k in ("red", "green", "blue")):
        raise ValueError("missing actual colorInWorld fields")
    values = [color[k] for k in ("red", "green", "blue")]
    if not all(isinstance(v, int) and 0 <= v <= 255 for v in values):
        raise ValueError("invalid actual colorInWorld RGB fields")
    return values


def world_rows(path: Path) -> dict[tuple[int, int, int], dict[str, Any]]:
    rows = {}
    for line in path.read_text(encoding="utf-8-sig").splitlines():
        if line.strip():
            row = json.loads(line)
            rows[(row["x"], row["y"], row["z"])] = row
    return rows


def actual_world_claim(sample: dict[str, Any], java_rows: dict, rust_rows: dict) -> dict[str, Any]:
    position = (sample["x"], sample["y"], sample["z"])
    expected = sample.get("stateKey")
    if sample.get("tintProvenance") != ACTUAL_SAMPLE_PROVENANCE:
        raise ValueError("missing exact actual-sample provenance")
    if not isinstance(expected, str):
        raise ValueError("missing expected stateKey")
    states = []
    for label, rows in (("java", java_rows), ("rust", rust_rows)):
        row = rows.get(position)
        if not row or row.get("stateKey") != expected:
            raise ValueError(f"{label} currentState != expectedState")
        states.append(row["stateKey"])
    diagnosis = sample.get("runtimeDiagnosis")
    if diagnosis is not None:
        if diagnosis.get("claimStatus") != "actual-world-state":
            raise ValueError("runtimeDiagnosis is not actual-world-state")
        if diagnosis.get("expectedStateKey") != expected or diagnosis.get("actualWorldStateKey") != expected:
            raise ValueError("runtimeDiagnosis expected/current state mismatch")
        if diagnosis.get("chunkLoaded") is not True:
            raise ValueError("runtimeDiagnosis chunkLoaded is not true")
    return {
        "claimStatus": "actual-world-state",
        "chunkLoaded": True,
        "expectedState": expected,
        "currentState": states[0],
        "position": list(position),
        "provenance": sample["tintProvenance"],
    }


def validate_trace(records: list[dict[str, Any]], target: dict[str, Any], expected: dict[str, list[int]]) -> tuple[int, int, int, int, list[list[int]], list[str]]:
    selected = [r for r in records if r.get("target") == target]
    valid = 0
    total = matches = 0
    upload_colors: list[list[int]] = []
    errors: list[str] = []
    for record in selected:
        packed = record.get("finalPackBytesDecoded")
        index = str(record.get("tintIndex"))
        if not isinstance(packed, list) or not isinstance(record.get("tint"), str):
            errors.append("trace quad missing tint/finalPackBytesDecoded")
            continue
        if record.get("tint") == "None":
            valid += 1
            continue
        wanted = expected.get(index)
        if wanted is None:
            errors.append(f"trace tint index {index} has no actual Java source color")
            continue
        record_ok = True
        for item in packed:
            value = item.get("lightTintBytes") if isinstance(item, dict) else None
            if not isinstance(value, list) or len(value) != 4 or not all(isinstance(v, int) and 0 <= v <= 255 for v in value):
                record_ok = False
                errors.append("trace packed lightTintBytes is invalid")
                continue
            color = value[1:4]
            total += 1
            upload_colors.append(color)
            matches += int(color == wanted)
        valid += int(record_ok)
    return len(selected), valid, matches, total, upload_colors, errors


def source_hashes() -> dict[str, str]:
    paths = {
        "javaDebug": ROOT / "fabric-render-probe/src/client/java/com/mine_rust/renderprobe/RenderProbeDebug.java",
        "javaClient": ROOT / "fabric-render-probe/src/client/java/com/mine_rust/renderprobe/RenderProbeClient.java",
        "rustModel": ROOT / "Client/pomme-client/src/world/block/model.rs",
        "rustMesher": ROOT / "Client/pomme-client/src/renderer/chunk/mesher.rs",
        "fixtureScript": ROOT / "fabric-render-probe/fixtures/tint/populate-tint-fixture.ps1",
        "fixtureScriptCases": ROOT / "fabric-render-probe/fixtures/tint/cases.json",
        "verifier": Path(__file__).resolve(),
    }
    return {name: sha256(path) for name, path in paths.items() if path.exists()}


def verify_root(root: Path) -> tuple[dict[str, Any], list[str]]:
    root = root.resolve()
    case = case_name(root)
    errors: list[str] = []
    java_dir, rust_dir = root / "java/results", root / "rust/results"
    jm, jd = load(java_dir / f"{case}.json"), load(java_dir / f"{case}.render-debug.json")
    rm, rd = load(rust_dir / f"{case}.json"), load(rust_dir / f"{case}.render-debug.json")
    pair = load(root / "pair-results" / f"{case}.json")
    manifest = load(root / "run-manifest.json")
    input_comparison = load(root / "input-comparison.json")
    java_world = world_rows(java_dir / f"{case}.world-input.jsonl")
    rust_world = world_rows(rust_dir / f"{case}.world-input.jsonl")
    samples = {s["block"].split(":")[-1]: s for s in jd.get("samples", []) if s.get("block", "").split(":")[-1] in CANDIDATES}
    traces = rd.get("environment", {}).get("actualDrawTrace", {})
    config = traces.get("config") or {}
    records = traces.get("records") or []
    if traces.get("enabled") is not True:
        errors.append("actualDrawTrace.enabled is not true")
    if not isinstance(records, list):
        errors.append("actualDrawTrace.records is missing")
        records = []

    coverage: dict[str, Any] = {}
    available = sorted(CANDIDATES & set(samples))
    if not available:
        errors.append("no actual candidate samples; source-only report")
    for name in available:
        local: list[str] = []
        sample = samples.get(name)
        if sample is None:
            local.append("missing actual Java sample")
            coverage[name] = {"claimStatus": "source-only-unverified", "errors": local}
            errors.extend(f"{case}:{name}:{e}" for e in local)
            continue
        try:
            claim = actual_world_claim(sample, java_world, rust_world)
        except (KeyError, ValueError, TypeError) as exc:
            claim = {"claimStatus": "source-only-unverified"}
            local.append(str(exc))
        sources = {str(x["tintIndex"]): x for x in sample.get("runtimeTintSources", []) if isinstance(x, dict) and "tintIndex" in x}
        expected: dict[str, list[int]] = {}
        for index, source in sources.items():
            try:
                expected[index] = rgb(source)
            except ValueError as exc:
                local.append(f"tintIndex {index}: {exc}")
        quads = sample.get("javaModelDraw", {}).get("upQuads", [])
        qindices = [q.get("tintIndex") for q in quads if isinstance(q, dict) and q.get("tintIndex", -1) >= 0]
        target = {"block": name, "x": sample["x"], "y": sample["y"], "z": sample["z"]}
        selected_config = any(
            isinstance(t, dict) and all(t.get(k) == target[k] for k in target)
            for t in config.get("targets", [])
        )
        if not selected_config:
            local.append("selectedTarget is false")
        selected, valid, matches, total, upload_colors, trace_errors = validate_trace(records, target, expected)
        local.extend(trace_errors)
        if selected == 0:
            local.append("numberOfTracedQuadsSelectedTarget is zero")
        if valid == 0:
            local.append("numberOfTracedQuadsValid is zero")
        if total == 0:
            local.append("rawRGBTotal is zero")
        if matches != total:
            local.append(f"partial raw RGB match {matches}/{total}")
        if pair.get("measurementStatus") != "success":
            local.append("measurementStatus is not success")
        if not input_comparison.get("success"):
            local.append("input comparison success is false")
        if input_comparison.get("matchedCells") != input_comparison.get("totalCells"):
            local.append("input comparison is partial")
        if not isinstance(pair.get("skewMs"), (int, float)) or pair["skewMs"] > pair.get("allowedSkewMs", float("inf")):
            local.append("actual snapshot time/skew is invalid")
        clocks = (jd.get("clock", {}), rd.get("clock", {}))
        if clocks[0].get("dayTime") != clocks[1].get("dayTime") or clocks[0].get("frozen") is not True:
            local.append("actual Java/Rust capture clock fields are not paired")
        peer = manifest.get("peerFilter", {})
        if peer.get("mode") != "paired" or not peer.get("peerName") or not peer.get("provenance"):
            local.append("peer filter provenance is missing")
        if not claim.get("claimStatus") == "actual-world-state":
            local.append("claimStatus is not actual-world-state")
        status = "verified" if not local else "source-only-unverified"
        coverage[name] = {
            **claim,
            "case": case,
            "javaSourceColors": expected,
            "javaQuadCount": len(quads),
            "javaTintIndexCounts": {str(i): qindices.count(i) for i in sorted(set(qindices))},
            "numberOfTracedQuadsSelectedTarget": selected,
            "numberOfTracedQuadsValid": valid,
            "selectedTarget": selected_config,
            "rustUploadColors": [list(x) for x in sorted({tuple(x) for x in upload_colors})],
            "rawRGBMatches": matches,
            "rawRGBTotal": total,
            "status": status,
            "errors": local,
        }
        if local:
            errors.extend(f"{case}:{name}:{e}" for e in local)

    result = {
        "schema": 2,
        "status": "verified" if not errors and coverage else "diagnostic-only",
        "source": "actual Java ClientLevel sample/world state and runtime BlockTintSource colorInWorld fields paired with Rust emitted/upload trace; no hardcoded actual claim",
        "candidateCoverage": coverage,
        "targetStateCoverage": {name: coverage.get(name, {}).get("expectedState") for name in sorted(coverage)},
        "strictRequirements": {
            "claimStatus": "actual-world-state",
            "currentStateEqualsExpectedState": True,
            "chunkLoaded": True,
            "rawRGBTotalPositive": True,
            "matchesEqualsTotal": True,
            "numberOfTracedQuadsValidPositive": True,
            "selectedTarget": True,
            "inputComparisonSuccess": True,
            "actualColorFields": "colorInWorld.red/green/blue",
            "actualSnapshotTime": "pair-results measurementStatus/skewMs and paired clocks",
            "peer": "run-manifest peerFilter",
        },
        "runs": [{
            "runRoot": str(root),
            "case": case,
            "manifestSha256": (root / "run-manifest.sha256").read_text(encoding="utf-8").split()[0],
            "manifestComputedSha256": sha256(root / "run-manifest.json"),
            "actualSkewMs": pair.get("skewMs"),
            "worldInputCountJava": jm.get("recordCount"),
            "worldInputCountRust": rm.get("recordCount"),
            "stateCountJava": jm.get("stateCount"),
            "stateCountRust": rm.get("stateCount"),
            "clock": {"java": jd.get("clock"), "rust": rd.get("clock")},
            "inputComparison": {"success": input_comparison.get("success"), "matchedCells": input_comparison.get("matchedCells"), "totalCells": input_comparison.get("totalCells")},
            "peer": manifest.get("peerFilter"),
        }],
        "sourceHashes": source_hashes(),
        "errors": errors,
        "unmeasured": ["GPU fragment/readback color", "full 64-case image parity", "untargeted quad fields beyond the bounded trace"],
    }
    return result, errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("roots", nargs="+", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    reports = []
    errors: list[str] = []
    coverage: dict[str, Any] = {}
    for root in args.roots:
        report, local_errors = verify_root(root)
        reports.append(report)
        errors.extend(local_errors)
        coverage.update(report.get("candidateCoverage", {}))
    if len(reports) == 1:
        output = reports[0]
        output["status"] = "verified" if len(coverage) == len(CANDIDATES) and not errors else "diagnostic-only"
    else:
        output = {
            "schema": 2,
            "status": "verified" if len(coverage) == len(CANDIDATES) and not errors else "diagnostic-only",
            "source": "strict per-run reports; no hardcoded actual claim",
            "candidateCoverage": coverage,
            "targetStateCoverage": {name: coverage.get(name, {}).get("expectedState") for name in sorted(coverage)},
            "reports": reports,
            "errors": errors,
            "sourceHashes": source_hashes(),
        }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(output, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    sidecar = args.out.with_suffix(args.out.suffix + ".sha256")
    sidecar.write_text(f"{sha256(args.out)}  {args.out.name}\n", encoding="utf-8")
    print(f"{output['status']} candidates={len(coverage)}/{len(CANDIDATES)} errors={len(errors)} out={args.out} sha256={sha256(args.out)}")
    return 0 if output["status"] == "verified" and not errors else 1


if __name__ == "__main__":
    raise SystemExit(main())
