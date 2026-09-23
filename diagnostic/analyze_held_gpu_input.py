#!/usr/bin/env python3
"""Compare actual Java held VBO/Lighting readbacks with Rust held shader inputs."""
import argparse
import json
import math
import hashlib
from pathlib import Path
from compare_item_overlay import png_read


def norm(v):
    length = math.sqrt(sum(x * x for x in v))
    return [x / length for x in v] if length else [0.0, 0.0, 0.0]


def shade(normal, light0, light1):
    a = max(0.0, sum(x * y for x, y in zip(normal, light0)))
    b = max(0.0, sum(x * y for x, y in zip(normal, light1)))
    return min(1.0, (a + b) * 0.6 + 0.4)


def matrix3_mul(m, v):
    # JOML column-major mat3, as serialized by the Rust held draw trace.
    return [sum(m[c * 3 + r] * v[c] for c in range(3)) for r in range(3)]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=Path, required=True)
    ap.add_argument("--sampler-evidence", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args()
    java = json.loads((a.root / "java/results/held-stone-on.render-debug.json").read_text())
    rust = json.loads((a.root / "rust/results/held-stone-on.render-debug.json").read_text())
    jtrace = java["heldItemPipeline"]["latestRenderFrame"]
    jdraw = jtrace["executedRenderPipelines"][0]
    jgpu = jdraw["actualGpuInputs"]
    rdraw = rust["heldItemPipeline"]["draw"]
    payload = rdraw["vertexPayload"]
    assert jtrace["actualDrawCount"] == 1 and jgpu["status"] == "captured" and jgpu["gpuReadback"]
    assert jgpu["derivedVertexCount"] == 24 and jgpu["indexCount"] == 36
    assert jgpu["readBytes"] == 24 * jgpu["vertexStride"] and len(jgpu["vertices"]) == 24
    lighting = jgpu["lightingUniform"]
    assert lighting["status"] == "captured_actual_RenderSystem_Lighting_slice" and lighting["gpuReadback"]
    assert payload["vertexCount"] == 36

    sampler_debug = json.loads(a.sampler_evidence.read_text())
    # Use the saved actual Sampler2 descriptor/readback; do not acquire another capture.
    actual_textures = sampler_debug["heldItemPipeline"]["latestRenderFrame"]["actualSamplerAndLightmap"]["actualTextureBindings"]
    sampler2 = next(t for t in actual_textures if t["binding"] == "Sampler2")
    lightmap = sampler2["gpuLightmapReadback"]
    assert sampler2["textureLabel"] == "Lightmap" and (sampler2["width"], sampler2["height"]) == (16, 16)

    light0, light1 = lighting["Light0_Direction"], lighting["Light1_Direction"]
    java_normals = {}
    for v in jgpu["vertices"]:
        n = tuple(v["Normal"][:3])
        java_normals[n] = shade(n, light0, light1)
    rust_normals = {}
    for v in payload["vertices"]:
        source = [x / 127.0 for x in v["normalBytes"][:3]]
        transformed = norm(matrix3_mul(rdraw["normalMatrixColumnMajor"], source))
        key = tuple(v["normalBytes"][:3])
        rust_normals[key] = {"inputSNORM": source, "postNormalMatrixNormalized": transformed,
                             "shade": shade(transformed, light0, light1)}

    nearest = []
    for jn, js in java_normals.items():
        rn, rv = max(rust_normals.items(), key=lambda item: sum(x*y for x,y in zip(jn, item[1]["postNormalMatrixNormalized"])))
        dot = sum(x*y for x,y in zip(jn, rv["postNormalMatrixNormalized"]))
        nearest.append({"javaFinalSNORMDecodedNormal": list(jn), "javaVertexShaderShade": js,
                        "closestRustNormalBytes": list(rn), **rv, "directionDot": dot,
                        "shadeDeltaRustMinusJava": rv["shade"] - js})

    uv_rows = []
    for i, v in enumerate(jgpu["vertices"]):
        u, w = v["UV2"]
        raw = [u / 256.0 + 0.5 / 16.0, w / 256.0 + 0.5 / 16.0]
        clamped = [max(0.5 / 16.0, min(15.5 / 16.0, x)) for x in raw]
        uv_rows.append({"vertex": i, "UV2": [u, w], "textureCoordinateBeforeClamp": raw,
                        "textureCoordinateClamped": clamped,
                        "texelCenterXY": [clamped[0] * 16.0 - 0.5, clamped[1] * 16.0 - 0.5]})
    unique_uv = {tuple(row["UV2"]) for row in uv_rows}
    all_white_color = all(v["Color"] == [1.0, 1.0, 1.0, 1.0] for v in jgpu["vertices"])
    java_png = a.root / "java/results/held-stone-on.png"
    rust_png = a.root / "rust/results/held-stone-on.png"
    jp, rp = png_read(java_png), png_read(rust_png)
    assert jp[:2] == rp[:2] == (1280, 720)
    raw_samples = {}
    for name, (x, y) in {"prior_up_reference": (1097, 625), "prior_west_reference": (993, 665)}.items():
        raw_samples[name] = {"xy": [x, y], "javaRGB": list(jp[2][y][x]), "rustRGB": list(rp[2][y][x]),
                             "provenance": "raw PNG bytes at unchanged coordinates from prior held CPU face analysis; not a GPU fragment readback"}
    report = {
        "schema": 1,
        "status": "actual-java-gpu-buffer-readback-and-independent-shader-input-oracle",
        "runId": json.loads((a.root / "run-manifest.json").read_text())["runId"],
        "javaDraw": {"pipeline": jdraw["pipeline"], "target": jdraw["outputTarget"],
                     "vertexFormat": jgpu["vertexFormat"], "strideBytes": jgpu["vertexStride"],
                     "indexCount": jgpu["indexCount"], "derivedVertexCount": jgpu["derivedVertexCount"],
                     "bufferId": jgpu["glBufferId"], "byteOffset": jgpu["readOffsetBytes"],
                     "readBytes": jgpu["readBytes"], "rawHexSha256": __import__("hashlib").sha256(bytes.fromhex(jgpu["rawVertexHex"])).hexdigest(),
                     "attributesFromOfficialVertexFormatGetters": jgpu["vertexElements"], "gpuReadback": True},
        "javaLightingUniform": {"actualReadback": True, "std140Bytes": lighting["byteLength"],
                                "Light0_Direction": light0, "Light1_Direction": light1,
                                "paddingSlotsExcludedFromShaderInputs": True},
        "javaAttributeSummary": {"allVertexColorsWhite": all_white_color,
                                 "javaFinalNormalToShade": nearest,
                                 "UV2Formula": "texture(lightMap, clamp((UV2 / 256.0) + 0.5 / 16.0, vec2(0.5/16.0), vec2(15.5/16.0)))",
                                 "UV2PerVertex": uv_rows, "uniqueUV2": [list(x) for x in sorted(unique_uv)],
                                 "constantAcrossEachFace": all(len({tuple(jgpu["vertices"][i+k]["UV2"]) for k in range(4)}) == 1 for i in range(0,24,4))},
        "savedActualSamplerEvidence": {"binding": "Sampler2", "filter": sampler2["minFilter"],
                                       "magFilter": sampler2["magFilter"], "maxLod": sampler2["maxLod"],
                                       "lightmapDimensions": [sampler2["width"], sampler2["height"]],
                                       "gpuTexel0_15": lightmap["texel0_15"], "readbackStatus": lightmap["status"],
                                       "sourceArtifact": str(a.sampler_evidence)},
        "rustActualVboAndShader": {"vertexCount": payload["vertexCount"], "strideBytes": payload["strideBytes"],
                                    "normalMatrixColumnMajor": rdraw["normalMatrixColumnMajor"],
                                    "normalBytesAndShaderShade": [{"normalBytes": list(k), **v} for k,v in sorted(rust_normals.items())],
                                    "lightingEquation": "normalize(normal_matrix * normal); min(1,(max(dot(L0,n),0)+max(dot(L1,n),0))*0.6+0.4)"},
        "imageEvidence": {"javaPng": "java/results/held-stone-on.png", "rustPng": "rust/results/held-stone-on.png",
                          "javaPngSha256": hashlib.sha256(java_png.read_bytes()).hexdigest(),
                          "rustPngSha256": hashlib.sha256(rust_png.read_bytes()).hexdigest(),
                          "rawRGBAtPriorFaceReferencePixels": raw_samples,
                          "interpretation": "raw PNG bytes are reported separately from vertex/shader arithmetic; no per-fragment colors inferred"},
        "scope": "Java final VBO and bound Lighting uniform are actual synchronous GL45 named-buffer readbacks; Rust payload is actual host-mapped bound VBO plus actual submitted normal matrix; shader outputs below are independent CPU calculations, not GPU fragment readbacks."
    }
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"runId": report["runId"], "javaVertices": jgpu["derivedVertexCount"],
                      "javaStride": jgpu["vertexStride"], "uv2": [list(x) for x in sorted(unique_uv)],
                      "sampler2": sampler2["minFilter"], "lightmapTexel0_15": lightmap["texel0_15"],
                      "javaNormals": len(java_normals), "rustNormals": len(rust_normals),
                      "report": str(a.out)}, sort_keys=True))


if __name__ == "__main__":
    main()
