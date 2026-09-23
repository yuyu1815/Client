#!/usr/bin/env python3
"""Strict same-target Java/Rust dropped-stone capture audit; no image alignment/correction."""
from __future__ import annotations
import argparse, json, math, struct
from pathlib import Path
from compare_item_overlay import png_read


def mat_vec(m, v):
    return [sum(m[c * 4 + r] * v[c] for c in range(4)) for r in range(4)]


def mat_mul(a, b):
    return [sum(a[k * 4 + r] * b[c * 4 + k] for k in range(4)) for c in range(4) for r in range(4)]


def screen(mvp, p, java):
    return screen_clip(mat_vec(mvp, [*p, 1.0]), java)


def screen_clip(q, java):
    if q[3] <= 0:
        return None
    x, y = q[0] / q[3], q[1] / q[3]
    return ((x + 1) * 640, (1 - y) * 360) if java else ((x + 1) * 640, (y + 1) * 360)


def raster_tri(points, w, h):
    a, b, c = points
    area = (b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])
    if abs(area) < 1e-8:
        return set()
    x0=max(0,math.floor(min(p[0] for p in points))); x1=min(w,math.ceil(max(p[0] for p in points)))
    y0=max(0,math.floor(min(p[1] for p in points))); y1=min(h,math.ceil(max(p[1] for p in points)))
    out=set()
    for y in range(y0,y1):
        for x in range(x0,x1):
            px,py=x+.5,y+.5
            s0=(b[0]-px)*(c[1]-py)-(b[1]-py)*(c[0]-px)
            s1=(c[0]-px)*(a[1]-py)-(c[1]-py)*(a[0]-px)
            s2=(a[0]-px)*(b[1]-py)-(a[1]-py)*(b[0]-px)
            if (s0 >= -1e-7 and s1 >= -1e-7 and s2 >= -1e-7) or (s0 <= 1e-7 and s1 <= 1e-7 and s2 <= 1e-7):
                out.add((x,y))
    return out


def metrics(java, rust, points):
    diffs=[abs(java[2][y][x][c]-rust[2][y][x][c]) for x,y in points for c in range(3)]
    gt8=sum(any(abs(java[2][y][x][c]-rust[2][y][x][c])>8 for c in range(3)) for x,y in points)
    return {"pixels":len(points),"rgbMAE":sum(diffs)/len(diffs) if diffs else None,"maxChannelDiff":max(diffs,default=0),"pixelsAnyChannelGt8":gt8,"gt8Rate":gt8/len(points) if points else None}


def bbox(points):
    if not points: return None
    return [min(x for x,y in points),min(y for x,y in points),max(x for x,y in points)+1,max(y for x,y in points)+1]


def audit(root: Path):
    java_meta=json.loads((root/'java/results/drop-stone-ground.json').read_text())
    rust_meta=json.loads((root/'rust/results/drop-stone-ground.json').read_text())
    java_debug=json.loads((root/'java/results/drop-stone-ground.render-debug.json').read_text())
    rust_debug=json.loads((root/'rust/results/drop-stone-ground.render-debug.json').read_text())
    jtrace=java_debug['heldItemPipeline']; jframe=jtrace['latestRenderFrame']; entity=jframe['actualDroppedEntity']; shadow=jframe['actualDroppedShadow']
    jdraws=[x for x in jframe['actualGroundDraws'] if x.get('actualGpuInputs',{}).get('status')=='captured']
    assert len(jdraws)==1, f'expected exactly one bounded Java VBO readback, got {len(jdraws)}'
    jdraw=jdraws[0]; jprep=jdraw['actualGroundFeaturePrepare']; jgpu=jdraw['actualGpuInputs']
    rdraw=rust_debug['itemEntityPipeline']; payload=rdraw['vertexPayload']; rverts=payload['vertices']
    assert entity['targetEntityUUID']==rdraw['targetEntityUUID']=='00000000-0000-4000-8000-000000000001'
    assert entity['itemId'].removeprefix('minecraft:')==rdraw['itemId']=='stone'
    assert entity['stackCount']==rdraw['stackCount']==1
    assert entity['position']==rdraw['position']
    assert entity['displayContext']==jprep['context']==rdraw['displayContext']=='GROUND'
    assert shadow['targetEntityUUID']==entity['targetEntityUUID'] and shadow['entityShadowsOption'] and shadow['shadowRadius']==0.15 and len(shadow['actualShadowPieces'])>0
    assert entity['controlledPhase'] and rdraw['controlledPhase']
    assert entity['controlledPhaseInputs']==rdraw['controlledPhaseInputs']
    assert entity['controlledPhaseInputs']=={'age':0.0,'bobOffset':0.0,'spin':0.0}
    assert jgpu['gpuReadback'] and jgpu['vertexStride']==36 and jgpu['derivedVertexCount']==24 and jgpu['readBytes']==864
    assert len(bytes.fromhex(jgpu['rawVertexHex']))==864
    assert payload['vertexCount']==rdraw['vertexCount']==36 and payload['strideBytes']==28 and payload['mappedAllocationBytes']==1008
    assert java_meta['cameraMode']==rust_meta['cameraMode']=='FIRST_PERSON'
    assert java_meta['hudHidden']==rust_meta['hudHidden']==False
    assert java_meta['gameMode']==rust_meta['gameMode']=='creative'
    assert java_meta['clock']['totalTicks']==rust_meta['clock']['totalTicks']==6000

    jp=jdraw['actualProjectionUse']['matrixColumnMajor']; jm=jprep['renderSystemModelViewMatrixColumnMajor']
    j_mvp=mat_mul(jp,jm); r_vp=rdraw['viewProjectionMatrixColumnMajor']; r_model=rdraw['modelMatrixColumnMajor']; r_camera=rdraw['cameraPositionRelativeToAnchor']
    jraw=bytes.fromhex(jgpu['rawVertexHex']); jpoints=[]; rpoints=[]; j_screen=[]; r_screen=[]
    for i in range(24):
        p=struct.unpack_from('<3f',jraw,i*36); q=screen(j_mvp,p,True); j_screen.append(q)
    for v in rverts:
        p=tuple(map(float,v['position'])) if isinstance(v['position'],list) else tuple(map(float,v['position'].split()))
        world=mat_vec(r_model,[*p,1.0]); relative=[world[i]-r_camera[i] for i in range(3)]+[world[3]]
        q=screen_clip(mat_vec(r_vp,relative),False); r_screen.append(q)
    jgpu_vertices=jgpu['vertices']; jexpanded=[]
    for face in range(6):
        rect=jprep['bakedQuads'][face]['spriteRectUv']; u0,v0,u1,v1=rect
        face_vertices=jgpu_vertices[face*4:face*4+4]
        for k in (0,1,2,2,3,0):
            v=face_vertices[k]; uv=v['UV0']; n=v['Normal'][:3]
            jexpanded.append({'screen':screen(j_mvp,v['Position'],True),'uv':[(uv[0]-u0)/(u1-u0),(uv[1]-v0)/(v1-v0)],'normal':n,'color':v['Color']})
    atlas=next(c for c in rust_debug['itemTintDiagnostics']['candidates'] if c['item']=='stone')['atlasRegions']['stone']['uv']; ru0,rv0,ru1,rv1=atlas
    rexpanded=[]
    normal_matrix=rdraw['normalMatrixColumnMajor']
    for i,v in enumerate(rverts):
        uv=v['uv'] if isinstance(v['uv'],list) else tuple(map(float,v['uv'].split()))
        local_uv=[(uv[0]-ru0)/(ru1-ru0),(uv[1]-rv0)/(rv1-rv0)]
        nb=v['normalBytes'] if isinstance(v['normalBytes'],list) else tuple(map(int,v['normalBytes'].split()))
        n=[q/127.0 for q in nb[:3]]
        transformed=[sum(normal_matrix[c*3+r]*n[c] for c in range(3)) for r in range(3)]
        length=math.sqrt(sum(q*q for q in transformed)); transformed=[q/length for q in transformed]
        rexpanded.append({'screen':r_screen[i],'uv':local_uv,'normal':transformed,'color':v['lightTintBytes']})
    unused=set(range(len(rexpanded))); vertex_matches=[]
    for jv in jexpanded:
        index=min(unused,key=lambda n:sum((jv['screen'][k]-rexpanded[n]['screen'][k])**2 for k in range(2))+10000*sum((jv['uv'][k]-rexpanded[n]['uv'][k])**2 for k in range(2))+1000*sum((jv['normal'][k]-rexpanded[n]['normal'][k])**2 for k in range(3)))
        unused.remove(index); rv= rexpanded[index]
        vertex_matches.append({'javaScreen':jv['screen'],'rustScreen':rv['screen'],'screenErrorPx':math.dist(jv['screen'],rv['screen']),'javaSpriteUv':jv['uv'],'rustSpriteUv':rv['uv'],'spriteUvError':math.dist(jv['uv'],rv['uv']),'javaNormal':jv['normal'],'rustTransformedNormal':rv['normal'],'normalError':math.dist(jv['normal'],rv['normal']),'javaColor':jv['color'],'rustPackedLightTint':rv['color']})
    j_tris=[]
    for face in range(6):
        quad=j_screen[face*4:face*4+4]
        for tri in ((0,1,2),(0,2,3)):
            cov=raster_tri([quad[k] for k in tri],1280,720)
            jpoints.extend(cov); j_tris.append(len(cov))
    for base in range(0,36,3):
        cov=raster_tri(r_screen[base:base+3],1280,720); rpoints.extend(cov)
    js=set(jpoints); rs=set(rpoints); union=js|rs
    bbox_union=bbox(union)
    png_java=png_read(root/'java/results/drop-stone-ground.png'); png_rust=png_read(root/'rust/results/drop-stone-ground.png')
    assert png_java[:2]==png_rust[:2]==(1280,720)
    full={(x,y) for y in range(720) for x in range(1280)}
    bbox_points={(x,y) for y in range(bbox_union[1],bbox_union[3]) for x in range(bbox_union[0],bbox_union[2])}
    envelope=[max(0,bbox_union[0]-20),max(0,bbox_union[1]-20),min(1280,bbox_union[2]+20),min(720,bbox_union[3]+20)]
    envelope_points={(x,y) for y in range(envelope[1],envelope[3]) for x in range(envelope[0],envelope[2])}
    outside_coverage=envelope_points-union
    below_points={(x,y) for y in range(bbox_union[3],min(720,bbox_union[3]+20)) for x in range(max(0,bbox_union[0]-20),min(1280,bbox_union[2]+20))}
    outliers=[{"x":x,"y":y,"javaRGB":png_java[2][y][x],"rustRGB":png_rust[2][y][x]} for x,y in bbox_points if any(abs(png_java[2][y][x][c]-png_rust[2][y][x][c])>8 for c in range(3))]
    # Compare silhouette projections without image warping; Rust y is already Vulkan-flipped.
    j_bounds=bbox({(round(x),round(y)) for x,y in j_screen if x is not None})
    r_bounds=bbox({(round(x),round(y)) for x,y in r_screen if x is not None})
    report={"schema":1,"status":"actual-drop-capture","root":str(root),"targetEntityUUID":entity['targetEntityUUID'],
      "conditions":{"itemId":entity['itemId'],"stackCount":entity['stackCount'],"position":entity['position'],"displayContext":"GROUND","cameraMode":java_meta['cameraMode'],"cameraPosition":[java_meta.get('cameraX'),java_meta.get('cameraY'),java_meta.get('cameraZ')],"cameraYawPitch":[java_meta.get('cameraYaw'),java_meta.get('cameraPitch')],"time":6000,"serverFrozen":java_meta.get('serverFrozen'),"pairSkewMs":json.loads((root/'pair-results/drop-stone-ground.json').read_text()).get('skewMs')},
      "actualJavaShadow":{"optionEnabled":shadow['entityShadowsOption'],"shadowRadius":shadow['shadowRadius'],"actualShadowPieceCount":len(shadow['actualShadowPieces']),"pieces":shadow['actualShadowPieces'],"source":"EntityRenderer.extractShadow then EntityRenderDispatcher.submitShadow/ShadowFeatureRenderer"},"rustItemShadow":{"itemPipelineSubmitsShadow":False,"source":"ItemEntityPipeline::draw submits item mesh only; no shadow pipeline was attributed to this entity"},
      "actualPhaseInputs":{"java":{"ageTicks":entity['actualAgeTicks'],"renderAgeBeforeControl":entity['actualRenderAge'],"bobOffsetBeforeControl":entity['actualBobOffset'],"spinBeforeControl":entity['actualSpin']},"rust":{"ageTicks":rdraw['actualAge'],"renderAgeBeforeControl":rdraw['actualRenderAge'],"bobOffsetBeforeControl":rdraw['actualBobOffset'],"spinBeforeControl":rdraw['actualSpin']},"controlled":{"age":entity['renderAge'],"bobOffset":entity['bobOffset'],"spin":entity['renderSpin'],"diagnosticOnly":True}},"gameMode":java_meta['gameMode'],
      "actualSubmission":{"java":{"pipeline":jdraw['pipeline'],"context":jprep['context'],"quads":jprep['quadCount'],"corners":jprep['capturedCornerCount'],"gpuBufferReadback":True,"vertexCount":jgpu['derivedVertexCount'],"strideBytes":jgpu['vertexStride'],"readBytes":jgpu['readBytes'],"modelPose":jprep['finalFeaturePoseMatrixColumnMajor'],"modelView":jm,"projection":jp,"normalAndVertexBytes":"rawVertexHex in Java render-debug"},"rust":{"context":rdraw['displayContext'],"status":rdraw['status'],"vertexCount":rdraw['vertexCount'],"strideBytes":payload['strideBytes'],"mappedBytes":payload['mappedAllocationBytes'],"model":rdraw['modelMatrixColumnMajor'],"viewProjection":rdraw['viewProjectionMatrixColumnMajor'],"cameraPositionRelativeToAnchor":r_camera,"normalMatrix":rdraw['normalMatrixColumnMajor'],"mappedVbo":"vertexPayload.vertices"},"vertexCorrespondence":{"matchedExpandedVertices":len(vertex_matches),"maximumScreenErrorPixels":max(x['screenErrorPx'] for x in vertex_matches),"maximumNormalizedSpriteUvError":max(x['spriteUvError'] for x in vertex_matches),"maximumTransformedNormalError":max(x['normalError'] for x in vertex_matches),"matches":vertex_matches}},
      "projection":{"javaVertexScreenBounds":j_bounds,"rustVertexScreenBounds":r_bounds,"javaTrianglePixelCoverage":[len(raster_tri([j_screen[i] for i in tri],1280,720)) for face in range(6) for tri in ((face*4,face*4+1,face*4+2),(face*4,face*4+2,face*4+3))],"rustTrianglePixelCoverage":[len(raster_tri(r_screen[i:i+3],1280,720)) for i in range(0,36,3)],"javaVertexScreenXY":j_screen,"rustVertexScreenXY":r_screen,"rawProjectedBBox":bbox_union,"javaCoveragePixelCenters":len(js),"rustCoveragePixelCenters":len(rs),"coverageSymmetricDifference":len(js^rs)},
      "rawImageMetrics":{"fullFrame":metrics(png_java,png_rust,full),"rawProjectedBBox":metrics(png_java,png_rust,bbox_points),"projectedCoverageUnion":metrics(png_java,png_rust,union),"projectedEnvelope":envelope,"outsideModelCoverageInEnvelope":metrics(png_java,png_rust,outside_coverage),"belowProjectedModelBounds":metrics(png_java,png_rust,below_points),"rawProjectedBBoxGt8Outliers":outliers,"alignmentOrCorrection":False,"backgroundIncluded":True,"shadowAndOverlaySeparated":"not semantically masked; report projected model coverage separately from surrounding raw envelope"}}
    return report


def main():
    p=argparse.ArgumentParser();p.add_argument('--root',type=Path,required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args();report=audit(a.root);a.out.parent.mkdir(parents=True,exist_ok=True);a.out.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8');print(json.dumps({k:report['rawImageMetrics'][k] for k in report['rawImageMetrics'] if k in ('fullFrame','rawProjectedBBox','projectedCoverageUnion')},sort_keys=True))

if __name__=='__main__':main()
