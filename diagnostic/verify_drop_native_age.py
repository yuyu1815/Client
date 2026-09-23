#!/usr/bin/env python3
"""Verify paired frozen/step captures use native item age and retain draw evidence."""
from __future__ import annotations
import argparse
import hashlib
import json
import math
from pathlib import Path

CASES = ("drop-age-frozen-a", "drop-age-frozen-b", "drop-age-step-3", "drop-age-time-reset")
EXPECTED_AGE = (0, 0, 3, 3)
EXPECTED_TIME = (6000, 6000, 6003, 6000)
UUID = "00000000-0000-4000-8000-000000000001"


def sha(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def read(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def verify(root: Path) -> dict:
    rows = []
    for case, age, time in zip(CASES, EXPECTED_AGE, EXPECTED_TIME):
        jm = read(root / "java/results" / f"{case}.json")
        rm = read(root / "rust/results" / f"{case}.json")
        jd = read(root / "java/results" / f"{case}.render-debug.json")
        rd = read(root / "rust/results" / f"{case}.render-debug.json")
        j = jd["heldItemPipeline"]["latestRenderFrame"]["actualDroppedEntity"]
        r = rd["itemEntityPipeline"]
        assert jm["serverFrozen"] and rm["serverFrozen"]
        assert jm["clientTime"] == rm["clientTime"] == time
        assert jm["gameMode"] == rm["gameMode"] == "creative"
        assert j["targetEntityUUID"] == r["targetEntityUUID"] == UUID
        assert j["stackCount"] == r["stackCount"] == 1
        assert j["position"] == r["position"] == [0.5, 64.0, 3.5]
        assert not j["controlledPhase"] and not r["controlledPhase"]
        assert j["bobControlled"] and r["bobControlled"]
        assert j["controlledBobInput"] == r["controlledBobInput"] == 0.0
        assert j["actualAgeTicks"] == r["actualAge"] == age
        assert j["entityTickCount"] == age
        assert j["partialTick"] == 1.0
        assert math.isclose(j["renderAge"], age + 1.0, abs_tol=1e-6)
        assert math.isclose(r["renderAge"], age + 1.0, abs_tol=1e-6)
        assert math.isclose(j["renderSpin"], j["renderAge"] / 20.0, abs_tol=1e-6)
        assert math.isclose(r["spin"], r["renderAge"] / 20.0, abs_tol=1e-6)
        jdraws = [d for d in jd["heldItemPipeline"]["latestRenderFrame"]["actualGroundDraws"]
                  if d.get("actualGpuInputs", {}).get("status") == "captured"]
        assert len(jdraws) == 1
        jdraw = jdraws[0]
        jgpu = jdraw["actualGpuInputs"]
        assert jgpu["gpuReadback"] and jgpu["readBytes"] == 864
        payload = r["vertexPayload"]
        assert r["status"] == "submitted" and payload["vertexCount"] == 36
        assert len(payload["vertices"]) == 36
        rows.append({
            "caseId": case, "expectedTime": time, "age": age,
            "java": {"itemAge": j["actualAgeTicks"], "entityTickCount": j["entityTickCount"],
                     "renderAge": j["renderAge"], "partialTick": j["partialTick"],
                     "bobOffset": j["bobOffset"], "spin": j["renderSpin"],
                     "modelViewSha256": sha(json.dumps(jdraw["actualGroundFeaturePrepare"]["renderSystemModelViewMatrixColumnMajor"], separators=(",", ":")).encode()),
                     "submittedDraw": {"pipeline": jdraw["pipeline"], "vertices": jgpu["derivedVertexCount"],
                                       "stride": jgpu["vertexStride"], "bytes": jgpu["readBytes"],
                                       "rawVertexSha256": sha(bytes.fromhex(jgpu["rawVertexHex"]))}},
            "rust": {"itemAge": r["actualAge"], "renderAge": r["renderAge"],
                     "partialTick": r["actualRenderAge"] - r["actualAge"],
                     "bobOffset": r["bobOffset"], "spin": r["spin"],
                     "modelMatrixSha256": sha(json.dumps(r["modelMatrixColumnMajor"], separators=(",", ":")).encode()),
                     "submittedDraw": {"vertices": payload["vertexCount"], "stride": payload["strideBytes"],
                                       "bytes": payload["mappedAllocationBytes"],
                                       "vertexPayloadSha256": sha(json.dumps(payload["vertices"], sort_keys=True, separators=(",", ":")).encode())}},
        })
    return {"schema": 1, "status": "PASS", "sameProcessRun": root.name,
            "assertions": ["frozen age holds across separated captures", "tick step 3 adds exactly 3",
                           "time reset 6003->6000 does not rewind age", "native age/partial drives spin",
                           "shared opt-in bob only; age phase override disabled", "actual model and submitted-draw evidence present"],
            "cases": rows}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()
    report = verify(args.root)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"native item age verification: {report['status']}; captures={len(report['cases'])}; age=0,0,3,3; bob=shared 0; spin=native")


if __name__ == "__main__":
    main()
