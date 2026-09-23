#!/usr/bin/env python3
"""Fail closed unless one Java held draw links actual baked quads and projection to Rust's actual VBO."""
import hashlib, json, sys
from pathlib import Path

root = Path(sys.argv[1])
case = sys.argv[2]
java = json.loads((root / f"java/results/{case}.render-debug.json").read_text())['heldItemPipeline']
frame = java['latestRenderFrame']
prep = java['lastActualFeaturePrepare']
assert frame['actualDrawCount'] == frame['featurePrepareCount'] == 1
assert prep['frameIndex'] == frame['frameIndex']
links = [x for x in frame['executedRenderPipelines'] if x.get('actualItemFeaturePrepare', {}).get('frameIndex') == frame['frameIndex']]
assert len(links) == 1
link = links[0]
upload = link['actualProjectionUse']
assert upload['linkedAtActualDraw'] and upload['linkedActualDrawFrameIndex'] == frame['frameIndex']
assert upload['projectionBufferIdentity'] and 'GameRenderer.renderLevel' in upload['source'] and 'ProjectionMatrixBuffer.getBuffer' in upload['source']
quads = prep['bakedQuads']
assert 0 < len(quads) <= 6 and prep['capturedCornerCount'] == 4 * len(quads) <= 24
assert prep['expandedTriangleVertexCount'] == 6 * len(quads) <= 36
off_java = json.loads((root / "java/results/held-stone-off.render-debug.json").read_text())['heldItemPipeline']['latestRenderFrame']
off_rust = json.loads((root / "rust/results/held-stone-off.render-debug.json").read_text())['heldItemPipeline']['gate']
assert off_java['actualDrawCount'] == 0 and off_rust['actualDrawCount'] == 0
rust_debug = json.loads((root / f"rust/results/{case}.render-debug.json").read_text())
rust = rust_debug['heldItemPipeline']['draw']
region = rust_debug['itemTintDiagnostics']['candidates'][-1]['atlasRegions']['stone']['uv']
payload = rust['vertexPayload']
assert rust['actualDrawCount'] == 1 and rust['vertexCount'] == payload['vertexCount'] == 36
assert payload['boundByteOffset'] == 0 and payload['mappedAllocationBytes'] == 36 * payload['strideBytes']
verts = payload['vertices']
assert len(verts) == 36
# Java baked corners are uncentered [0,1]; Rust's emitted VBO is centered [-.5,.5].
def corner_key(position, uv):
    return (tuple(round(float(p) - .5, 6) for p in position), tuple(round(float(v), 6) for v in uv))
rust_faces = []
for base in range(0, 36, 6):
    face = verts[base:base+6]
    points = {(tuple(round(float(p), 6) for p in v['position']), (round((v['uv'][0]-region[0])/(region[2]-region[0]), 6), round((v['uv'][1]-region[1])/(region[3]-region[1]), 6))) for v in face}
    normals = {tuple(v['normalBytes'][:3]) for v in face}
    lights = sorted({tuple(v['lightTintBytes']) for v in face})
    rust_faces.append((points, normals, lights))
face_map = []
used = set()
for qi, quad in enumerate(quads):
    corners = quad['corners']
    source_points = {corner_key(c['position'], c['uvSprite01']) for c in corners}
    hits = [i for i, (points, _, _) in enumerate(rust_faces) if i not in used and source_points.issubset(points)]
    assert len(hits) == 1, f"Java quad {qi} did not uniquely match a Rust 6-vertex face: {hits}"
    ri = hits[0]; used.add(ri)
    points, normals, lights = rust_faces[ri]
    face_map.append({"javaQuadIndex": qi, "rustVboFaceIndex": ri, "sprite": quad['sprite'], "atlas": quad['atlas'], "spriteRectPixels": quad['spriteRectPixels'], "spriteRectUv": quad['spriteRectUv'], "normalDirection": quad['normalDirection'], "normal": quad['normal'], "cornerCount": 4, "rustTriangleVertexCount": 6, "cornerPositionUvExactAfterCentering": True, "rustNormalBytes": list(next(iter(normals))), "rustLightTintBytes": [list(x) for x in lights], "corners": [{"javaPosition": c['position'], "rustCenteredPosition": [round(x-.5, 6) for x in c['position']], "uvAtlas": c['uvAtlas'], "uvSprite01": c['uvSprite01']} for c in corners]})
assert len(used) == 6
analysis = json.loads((root / 'analysis/held-stone-rgb-face-analysis.json').read_text())
report = {"schema": 1, "status": "verified", "caseId": case, "scope": "actual Java held ItemFeatureRenderer.prepareMainSubmit baked quads joined to same-frame PreparedRenderType draw and live Projection argument used by ProjectionMatrixBuffer; paired with Rust actual bound mapped VBO; no independent rebake", "java": {"actualDrawFrameIndex": frame['frameIndex'], "actualDrawCount": frame['actualDrawCount'], "actualDrawAt": frame['actualDrawAt'], "renderTarget": link['outputTarget'], "renderTargetIdentity": link['renderTargetIdentity'], "pipeline": link['pipeline'], "indexCount": link['indexCount'], "prepareContext": prep['context'], "lightCoordsPacked": prep['lightCoordsPacked'], "bakedQuadCount": len(quads), "cornerCount": prep['capturedCornerCount'], "projectionBufferIdentity": upload['projectionBufferIdentity'], "projectionUseFrameIndex": upload['frameIndex'], "projectionUsedAt": upload['usedAt'], "projectionMatrixColumnMajor": upload['matrixColumnMajor'], "projectionProvenance": upload['source']}, "onOffGate": {"javaOnActualDrawCount": frame['actualDrawCount'], "javaOffActualDrawCount": off_java['actualDrawCount'], "rustOnActualDrawCount": rust['actualDrawCount'], "rustOffActualDrawCount": off_rust['actualDrawCount']}, "rust": {"actualDrawAt": rust['actualDrawAt'], "buffer": payload['buffer'], "boundByteOffset": payload['boundByteOffset'], "vertexCount": payload['vertexCount'], "strideBytes": payload['strideBytes'], "mappedAllocationOffset": payload['mappedAllocationOffset'], "mappedAllocationBytes": payload['mappedAllocationBytes'], "light": rust['light'], "lightMode": rust['lightMode'], "provenance": payload['provenance']}, "matching": {"matchedQuads": len(face_map), "matchedCorners": sum(x['cornerCount'] for x in face_map), "matchedExpandedTriangleVertices": sum(x['rustTriangleVertexCount'] for x in face_map), "faces": face_map}, "savedTriangleUvTexelFaceRgbComparison": analysis['faceProjectionAndUV'], "pixelComparison": analysis['rawCommonBBox'], "provenanceLimit": "RGB values are same-coordinate saved PNG pixels; per-face texels are CPU perspective-UV samples from pinned official stone texture and CPU clipped/depth/cull face estimate, not GPU fragment/lightmap readback"}
out = root / 'analysis/held-java-rust-quad-projection-map.json'
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
print(f"PASS JavaDrawFrame={frame['frameIndex']} projectionBuffer={upload['projectionBufferIdentity']} quads={len(quads)} corners={prep['capturedCornerCount']} RustVBO=36 mappedBytes={payload['mappedAllocationBytes']} -> {out}")
