#!/usr/bin/env python3
"""Source/payload and unaligned raw-ROI audit for one owned dropped stone."""
from __future__ import annotations
import argparse, json, math
from pathlib import Path
from compare_drop_stone_capture import bbox, mat_vec, metrics, raster_tri, screen_clip
from compare_item_overlay import png_read

UUID = "00000000-0000-4000-8000-000000000001"


def camera_points(rust_draw, vertices):
    camera = rust_draw["cameraPositionRelativeToAnchor"]
    vp = rust_draw["viewProjectionMatrixColumnMajor"]
    out = []
    for vertex in vertices:
        pos = vertex["position"]
        clip = mat_vec(vp, [pos[i] - camera[i] for i in range(3)] + [1.0])
        out.append(screen_clip(clip, False))
    return out


def model_mask(draw):
    vertices = draw["vertexPayload"]["vertices"]
    model = draw["modelMatrixColumnMajor"]
    camera = draw["cameraPositionRelativeToAnchor"]
    vp = draw["viewProjectionMatrixColumnMajor"]
    screen = []
    for vertex in vertices:
        p = vertex["position"]
        world = mat_vec(model, [*p, 1.0])
        rel = [world[i] - camera[i] for i in range(3)] + [world[3]]
        screen.append(screen_clip(mat_vec(vp, rel), False))
    pixels = set()
    for base in range(0, len(screen), 3):
        pixels |= raster_tri(screen[base:base + 3], 1280, 720)
    return pixels


def shadow_mask(draw, shadow):
    points = camera_points(draw, shadow["emittedVertices"])
    pixels = set()
    for base in range(0, len(points), 3):
        pixels |= raster_tri(points[base:base + 3], 1280, 720)
    return pixels


def audit(before: Path, after: Path, before_case: str) -> dict:
    case = "drop-age-frozen-a"
    java_debug = json.loads((after / f"java/results/{case}.render-debug.json").read_text())
    rust_debug = json.loads((after / f"rust/results/{case}.render-debug.json").read_text())
    jm = json.loads((after / f"java/results/{case}.json").read_text())
    rm = json.loads((after / f"rust/results/{case}.json").read_text())
    jshadow = java_debug["heldItemPipeline"]["latestRenderFrame"]["actualDroppedShadow"]
    jentity = java_debug["heldItemPipeline"]["latestRenderFrame"]["actualDroppedEntity"]
    rdraw = rust_debug["itemEntityPipeline"]
    rshadow = rdraw["shadow"]
    jverts = jshadow["actualEmittedVertices"]
    rverts = rshadow["emittedVertices"]
    assert jentity["targetEntityUUID"] == jshadow["targetEntityUUID"] == rshadow["targetEntityUUID"] == UUID
    assert len(jshadow["actualShadowPieces"]) == len(rshadow["pieces"]) == 1
    assert len(jverts) == 4 and len(rverts) == 6 and rshadow["draw"] == "submitted"
    assert jshadow["entityShadowsOption"] and jshadow["shadowRadius"] == rshadow["radius"] == 0.15
    assert jentity["itemId"].removeprefix("minecraft:") == rdraw["itemId"] == "stone"
    assert jentity["stackCount"] == rdraw["stackCount"] == 1 and jentity["position"] == rdraw["position"]
    assert jm["serverFrozen"] and rm["serverFrozen"] and jm["clock"]["totalTicks"] == rm["clock"]["totalTicks"] == 6000
    assert jm["hudHidden"] is False and rm["hudHidden"] is False

    # Java emits QUADS (0,1,2,3); Vulkan expands the same corners to two triangles.
    expanded = [0, 1, 2, 0, 2, 3]
    corner_matches = [0, 1, 2, 5]
    camera = rdraw["cameraPositionRelativeToAnchor"]
    vp = rdraw["viewProjectionMatrixColumnMajor"]
    vertex_errors = []
    for jv, ri in zip(jverts, corner_matches):
        rv = rverts[ri]
        rust_camera_relative = [rv["position"][i] - camera[i] for i in range(3)]
        java_screen = screen_clip(mat_vec(vp, [*jv["positionTransformed"], 1.0]), False)
        rust_screen = screen_clip(mat_vec(vp, [*rust_camera_relative, 1.0]), False)
        vertex_errors.append({
            "javaCameraRelative": jv["positionTransformed"], "rustCameraRelative": rust_camera_relative,
            "screenErrorPx": math.dist(java_screen, rust_screen),
            "javaUv": jv["uv"], "rustUv": rv["uv"], "alphaByteJava": jv["alphaByte"], "alphaByteRust": rv["rgba"][3],
        })
        assert jv["alphaByte"] == rv["rgba"][3]
        assert max(abs(jv["uv"][i] - rv["uv"][i]) for i in range(2)) < 1e-6
    piece_j = jshadow["actualShadowPieces"][0]
    piece_r = rshadow["pieces"][0]
    assert max(abs(piece_j["shapeBounds"][i] - piece_r["bounds"][i]) for i in range(6)) < 1e-7
    assert abs(piece_j["alpha"] - piece_r["alpha"]) < 1e-7
    assert max(abs(piece_j[k] - piece_r["relative"][i]) for i, k in enumerate(("relativeX", "relativeY", "relativeZ"))) < 1e-7
    assert piece_r["brightness"] == 15 and abs(piece_r["powerAtDepth"] - piece_r["alpha"] * 2) < 1e-6
    assert max(v["screenErrorPx"] for v in vertex_errors) < 1e-4

    model_roi = model_mask(rdraw)
    shadow_roi = shadow_mask(rdraw, rshadow)
    old_j, old_r = (png_read(before / f"{side}/results/{before_case}.png") for side in ("java", "rust"))
    new_j, new_r = (png_read(after / f"{side}/results/{case}.png") for side in ("java", "rust"))
    assert old_j[:2] == old_r[:2] == new_j[:2] == new_r[:2] == (1280, 720)
    skew = json.loads((after / f"pair-results/{case}.json").read_text())["skewMs"]
    before_java_debug = json.loads((before / f"java/results/{before_case}.render-debug.json").read_text())
    before_rust_debug = json.loads((before / f"rust/results/{before_case}.render-debug.json").read_text())
    before_java_meta = json.loads((before / f"java/results/{before_case}.json").read_text())
    before_rust_meta = json.loads((before / f"rust/results/{before_case}.json").read_text())
    before_java = before_java_debug["heldItemPipeline"]["latestRenderFrame"]["actualDroppedEntity"]
    before_rust = before_rust_debug["itemEntityPipeline"]
    for key in ("actualAgeTicks", "actualRenderAge", "renderSpin", "controlledBobInput"):
        assert before_java.get(key) == jentity.get(key), f"Java native phase mismatch: {key}"
    for key in ("actualAge", "actualRenderAge", "spin", "controlledBobInput"):
        assert before_rust.get(key) == rdraw.get(key), f"Rust native phase mismatch: {key}"
    before_java_vignette = before_java_debug["heldItemPipeline"]["screenCompositionStages"]["vignette"]
    before_rust_vignette = before_rust_meta["vignetteTrace"]
    after_java_vignette = java_debug["heldItemPipeline"]["screenCompositionStages"]["vignette"]
    after_rust_vignette = rm["vignetteTrace"]
    vignette = {"before": {"java": before_java_vignette["actualVignetteBrightness"], "rust": before_rust_vignette["brightness"]},
                "after": {"java": after_java_vignette["actualVignetteBrightness"], "rust": after_rust_vignette["brightness"]}}
    assert max(*vignette["before"].values(), *vignette["after"].values()) <= 0.005, f"vignette not converged: {vignette}"
    assert before_java_meta["clock"]["totalTicks"] == before_rust_meta["clock"]["totalTicks"] == jm["clock"]["totalTicks"] == rm["clock"]["totalTicks"] == 6000
    for axis in ("cameraX", "cameraY", "cameraZ", "cameraYaw", "cameraPitch", "fov"):
        assert before_java_meta[axis] == jm[axis] and before_rust_meta[axis] == rm[axis], f"before/after camera mismatch: {axis}"
    assert before_java_meta["serverFrozen"] and before_rust_meta["serverFrozen"] and jm["serverFrozen"] and rm["serverFrozen"]
    assert before_rust_debug["itemEntityPipeline"].get("shadow", {}).get("diagnosticDisabled") is True
    assert before_java_vignette["maxLocalRawBrightness"] == before_rust_vignette["eyeSkyLight"] == 15
    assert before_rust_vignette["eyeBlockLight"] == 0 and after_rust_vignette["eyeSkyLight"] == 15 and after_rust_vignette["eyeBlockLight"] == 0
    return {
        "schema": 1, "status": "drop-shadow-source-payload-and-raw-roi", "beforeRoot": str(before), "afterRoot": str(after),
        "condition": {"UUID": UUID, "item": "stone x1", "position": jentity["position"], "camera": [jm["cameraX"], jm["cameraY"], jm["cameraZ"]],
          "frameTime": 6000, "frozen": True, "hudHidden": False, "peerFilter": "paired Luna names", "pairSkewMs": skew,
          "nativeAge": {"java": jentity["actualAgeTicks"], "rust": rdraw["actualAge"]}, "controlledBob": {"java": jentity.get("controlledBobInput"), "rust": rdraw.get("controlledBobInput")}},
        "frameLightVignette": {"vignetteBrightness": vignette, "maximumVignetteBrightness": max(*vignette["before"].values(), *vignette["after"].values()),
          "eyeLight": {"javaBefore": before_java_vignette["maxLocalRawBrightness"], "rustBeforeSkyBlock": [before_rust_vignette["eyeSkyLight"], before_rust_vignette["eyeBlockLight"]], "javaAfter": after_java_vignette["maxLocalRawBrightness"], "rustAfterSkyBlock": [after_rust_vignette["eyeSkyLight"], after_rust_vignette["eyeBlockLight"]]},
          "frameIds": {"beforeJava": before_java_debug["heldItemPipeline"]["latestRenderFrame"]["frameIndex"], "beforeRust": before_rust_meta.get("captureFrame"), "afterJava": java_debug["heldItemPipeline"]["latestRenderFrame"]["frameIndex"], "afterRust": rm.get("captureFrame")},
          "pairSkewMsBeforeAfter": [json.loads((before / f"pair-results/{before_case}.json").read_text())["skewMs"], skew], "clockDay": 6000, "frameMatching": "paired capture snapshots, not same GPU frame"},
        "beforeAfterPhaseNote": {"sameCameraClock": True, "itemModelPhaseMatched": True,
          "before": {"javaRenderAge": before_java.get("actualRenderAge"), "javaControlledPhase": before_java.get("controlledPhase"), "rustRenderAge": before_rust.get("actualRenderAge"), "rustControlledPhase": before_rust.get("controlledPhase")},
          "after": {"javaRenderAge": jentity.get("actualRenderAge"), "javaControlledPhase": jentity.get("controlledPhase"), "rustRenderAge": rdraw.get("actualRenderAge"), "rustControlledPhase": rdraw.get("controlledPhase")},
          "reason": "Both captures use the same native frozen age, render partial, bob input, camera, item count and native spin path."},
        "beforeRustShadowControl": before_rust_debug["itemEntityPipeline"].get("shadow"),
        "sourcePayload": {"javaRadius": jshadow["shadowRadius"], "javaOptionEnabled": jshadow["entityShadowsOption"], "pieceCountJavaRust": [len(jshadow["actualShadowPieces"]), len(rshadow["pieces"])],
          "textureDescriptor": {k: rshadow[k] for k in ("textureAsset", "textureFormat", "sampler", "depthState", "projectionOffset", "scope")},
          "gateCoverage": "formula table tests disabled option/invisibility/distance/light/surface; production shadow wiring only uses default-on and rendered-item visibility because Rust does not yet track those toggles/metadata",
          "javaAlpha": piece_j["alpha"], "rustAlpha": piece_r["alpha"], "javaAlphaByte": jverts[0]["alphaByte"], "rustAlphaByte": rverts[0]["rgba"][3],
          "rustBrightness": piece_r["brightness"], "rustPowerAtDepth": piece_r["powerAtDepth"], "vertexCountJavaRust": [len(jverts), len(rverts)],
          "maxScreenVertexErrorPx": max(v["screenErrorPx"] for v in vertex_errors), "maxUVError": max(math.dist(v["javaUv"], v["rustUv"]) for v in vertex_errors), "expandedVertexMatches": vertex_errors,
          "blendContract": "source alpha unchanged; Java framebuffer RGBA8_UNORM encoded-byte blend vs Rust B8G8R8A8_SRGB linear-light blend can differ; no alpha gain or swapchain change; raw ROI residual remains measured, not claimed as full parity"},

        "rawROI": {
          "itemModelROI": {"pixelCount": len(model_roi), "beforeJavaVsRust": metrics(old_j, old_r, model_roi), "afterJavaVsRust": metrics(new_j, new_r, model_roi),
            "javaBeforeVsAfter": metrics(old_j, new_j, model_roi), "rustBeforeVsAfter": metrics(old_r, new_r, model_roi)},
          "shadowProjectedROI": {"pixelCount": len(shadow_roi), "bounds": bbox(shadow_roi), "beforeJavaVsRust": metrics(old_j, old_r, shadow_roi), "afterJavaVsRust": metrics(new_j, new_r, shadow_roi),
            "javaBeforeVsAfter": metrics(old_j, new_j, shadow_roi), "rustBeforeVsAfter": metrics(old_r, new_r, shadow_roi)},
          "originalPNGWholeFramesRetained": True, "alignment": False, "colorAlignment": False, "postHocThresholdOrMaskChange": False,
          "ROI": "projection from actual emitted payload; every pixel in the projected polygon retained, including transparent texture pixels and background"}
    }


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--before", type=Path, required=True)
    p.add_argument("--before-case", default="drop-age-frozen-a")
    p.add_argument("--after", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    report = audit(a.before, a.after, a.before_case)
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"itemModelROI": report["rawROI"]["itemModelROI"], "shadowProjectedROI": report["rawROI"]["shadowProjectedROI"], "sourcePayload": {k: report["sourcePayload"][k] for k in ("javaAlpha", "rustAlpha", "maxScreenVertexErrorPx", "maxUVError")}}, sort_keys=True))


if __name__ == "__main__":
    main()
