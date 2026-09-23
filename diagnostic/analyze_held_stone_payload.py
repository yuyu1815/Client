#!/usr/bin/env python3
"""CPU audit of saved held-stone payload/projections; never a GPU readback."""
from __future__ import annotations
import argparse, json, math, statistics, zipfile
from pathlib import Path
from compare_item_overlay import png_read


def mul(m, v):
    return [sum(m[c * 4 + r] * v[c] for c in range(4)) for r in range(4)]


def matmul(a, b):
    return [sum(a[k * 4 + r] * b[c * 4 + k] for k in range(4)) for c in range(4) for r in range(4)]


def clip(poly):
    planes = (lambda p:p[0]+p[3], lambda p:p[3]-p[0], lambda p:p[1]+p[3], lambda p:p[3]-p[1], lambda p:p[2], lambda p:p[3]-p[2])
    for dist in planes:
        if not poly: break
        out=[]; prev=poly[-1]; dp=dist(prev)
        for cur in poly:
            dc=dist(cur)
            if (dc >= 0) != (dp >= 0):
                t=dp/(dp-dc); out.append([prev[i]+t*(cur[i]-prev[i]) for i in range(len(cur))])
            if dc >= 0: out.append(cur)
            prev,dp=cur,dc
        poly=out
    return poly


def cross(a,b,p): return (b[0]-a[0])*(p[1]-a[1])-(b[1]-a[1])*(p[0]-a[0])


def raster_triangle(poly, width, height):
    """Clip one actual triangle, fan-triangulate every resulting polygon, emit pixel samples."""
    clipped=clip(poly)
    out=[]
    if len(clipped)<3: return out
    screen=[((q[0]/q[3]+1)*width/2, (q[1]/q[3]+1)*height/2) for q in clipped]
    for i in range(1,len(clipped)-1):
        qs=[clipped[0],clipped[i],clipped[i+1]]
        ss=[screen[0],screen[i],screen[i+1]]
        area=cross(ss[0],ss[1],ss[2])
        if abs(area)<1e-12: continue
        x0=max(0,math.floor(min(p[0] for p in ss))); x1=min(width,math.ceil(max(p[0] for p in ss)))
        y0=max(0,math.floor(min(p[1] for p in ss))); y1=min(height,math.ceil(max(p[1] for p in ss)))
        for y in range(y0,y1):
            for x in range(x0,x1):
                p=(x+.5,y+.5)
                b0=cross(ss[1],ss[2],p)/area; b1=cross(ss[2],ss[0],p)/area; b2=1-b0-b1
                if min(b0,b1,b2)<-1e-8: continue
                ws=[1/q[3] for q in qs]; bs=[b0,b1,b2]; den=sum(bs[j]*ws[j] for j in range(3))
                if den == 0: continue
                interp=lambda k:sum(bs[j]*ws[j]*qs[j][k] for j in range(3))/den
                # Vulkan depth is interpolated from z/w in screen space, not perspective varying interpolation.
                z=sum(bs[j]*qs[j][2]/qs[j][3] for j in range(3))
                out.append((x,y,area,z,interp(4),interp(5)))
    return out


def gray_png(data):
    import struct, zlib
    pos=8; chunks=bytearray(); width=height=None
    while pos<len(data):
        n=struct.unpack('>I',data[pos:pos+4])[0]; typ=data[pos+4:pos+8]; body=data[pos+8:pos+8+n]; pos+=12+n
        if typ==b'IHDR': width,height,depth,kind=struct.unpack('>IIBB',body[:10]); assert depth==8 and kind==0
        elif typ==b'IDAT': chunks.extend(body)
        elif typ==b'IEND': break
    raw=zlib.decompress(chunks); rows=[]; prev=bytearray(width); off=0
    for _ in range(height):
        f=raw[off]; off+=1; cur=bytearray(raw[off:off+width]); off+=width
        for i in range(width):
            left=cur[i-1] if i else 0; up=prev[i]; ul=prev[i-1] if i else 0
            if f==1: cur[i]=(cur[i]+left)&255
            elif f==2: cur[i]=(cur[i]+up)&255
            elif f==3: cur[i]=(cur[i]+((left+up)//2))&255
            elif f==4:
                q=left+up-ul; ds=(abs(q-left),abs(q-up),abs(q-ul)); cur[i]=(cur[i]+(left,up,ul)[ds.index(min(ds))])&255
            elif f!=0: raise ValueError(f'unsupported PNG filter {f}')
        rows.append([(v,v,v) for v in cur]); prev=cur
    return width,height,rows


def metric(points, java, rust):
    diffs=[abs(java[2][y][x][c]-rust[2][y][x][c]) for x,y in points for c in range(3)]
    gt8=sum(any(abs(java[2][y][x][c]-rust[2][y][x][c])>8 for c in range(3)) for x,y in points)
    return {'pixels':len(points),'rgbMAE':sum(diffs)/len(diffs) if diffs else None,'maxChannelDiff':max(diffs,default=0),'pixelsAnyChannelGt8':gt8,'gt8Rate':gt8/len(points) if points else None}


def self_check():
    # A triangle clipped into a quad must rasterize as two fan triangles; clipping a
    # closed synthetic cube must give equal front-winding and two-sided silhouettes.
    tri=[[-2,-.5,.5,1,0,0],[.8,-.5,.5,1,1,0],[.8,.8,.5,1,1,1]]
    clipped=clip(tri); assert len(clipped)==4
    coverage=raster_triangle(tri,32,32); assert len({(r[0],r[1]) for r in coverage})>100
    vertices=[]
    faces=[([(-1,-1,1),(-1,1,1),(1,1,1),(1,-1,1)],(0,0,1)), # front
           ([(1,-1,-1),(1,1,-1),(-1,1,-1),(-1,-1,-1)],(0,0,-1)),
           ([(-1,1,-1),(-1,1,1),(1,1,1),(1,1,-1)],(0,1,0)),
           ([(-1,-1,1),(-1,-1,-1),(1,-1,-1),(1,-1,1)],(0,-1,0)),
           ([(1,-1,1),(1,-1,-1),(1,1,-1),(1,1,1)],(1,0,0)),
           ([(-1,-1,-1),(-1,-1,1),(-1,1,1),(-1,1,-1)],(-1,0,0))]
    # Use a perspective-like projective transform; no clipping, all six faces closed.
    project=lambda p:[p[0],p[1],(4-p[2])*.5,4-p[2],0,0]
    both=set(); ccw=set(); cw=set()
    for corners,_ in faces:
        for ids in ((0,1,2),(0,2,3)):
            samples=raster_triangle([project(corners[i]) for i in ids],64,64)
            both.update((r[0],r[1]) for r in samples)
            (ccw if samples and samples[0][2]<0 else cw).update((r[0],r[1]) for r in samples)
    # For consistently outward-wound closed faces one winding's union is the silhouette.
    assert ccw == both and len(cw) < len(both)
    print('SELF-CHECK PASS clip-fan=4-vertex-polygon closed-cube=6-quads/12-triangles')


def main():
    p=argparse.ArgumentParser(); p.add_argument('--root',type=Path); p.add_argument('--client-jar',type=Path); p.add_argument('--out',type=Path); p.add_argument('--self-check',action='store_true'); a=p.parse_args()
    if a.self_check: self_check(); return
    if not (a.root and a.client_jar and a.out): p.error('--root, --client-jar and --out required')
    root=a.root; rust_debug=json.loads((root/'rust/results/held-stone-on.render-debug.json').read_text()); java_debug=json.loads((root/'java/results/held-stone-on.render-debug.json').read_text())
    draw=rust_debug['heldItemPipeline']['draw']; payload=draw['vertexPayload']; verts=payload['vertices']; assert len(verts)==draw['vertexCount']==36 and payload['mappedAllocationBytes']==len(verts)*payload['strideBytes']==1008
    jprep=java_debug['heldItemPipeline']['lastActualFeaturePrepare']; jmodel=matmul(jprep['renderSystemModelViewMatrixColumnMajor'],jprep['finalFeaturePoseMatrixColumnMajor']); model_errors=[]
    for v in verts:
        jp=mul(jmodel,[v['position'][0]+.5,v['position'][1]+.5,v['position'][2]+.5,1]); rp=mul(draw['modelMatrixColumnMajor'],[*v['position'],1]); model_errors.extend(abs(jp[i]-rp[i]) for i in range(3))
    mapping=json.loads((root/'analysis/held-java-rust-quad-projection-map.json').read_text())
    assert mapping['matching']['matchedQuads']==6 and mapping['matching']['matchedExpandedTriangleVertices']==36
    java_vp=matmul(mapping['java']['projectionMatrixColumnMajor'],jmodel)
    edge_use={}; cube_vertices=set(); winding_normal_matches=True
    for matched_face in mapping['matching']['faces']:
        corners=matched_face['corners']; positions=[tuple(round(float(x),6) for x in c['javaPosition']) for c in corners]; cube_vertices.update(positions)
        edge_a=[positions[1][i]-positions[0][i] for i in range(3)]; edge_b=[positions[2][i]-positions[0][i] for i in range(3)]
        cross_n=(edge_a[1]*edge_b[2]-edge_a[2]*edge_b[1],edge_a[2]*edge_b[0]-edge_a[0]*edge_b[2],edge_a[0]*edge_b[1]-edge_a[1]*edge_b[0])
        winding_normal_matches &= all(abs(cross_n[i]-matched_face['normal'][i])<1e-6 for i in range(3))
        for i in range(4):
            edge=tuple(sorted((positions[i],positions[(i+1)%4]))); edge_use[edge]=edge_use.get(edge,0)+1
    assert len(cube_vertices)==8 and len(edge_use)==12 and set(edge_use.values())=={2} and winding_normal_matches
    projection_xy_errors=[]
    for matched_face in mapping['matching']['faces']:
        for corner in matched_face['corners']:
            jc=mul(java_vp,[*corner['javaPosition'],1]); rc=mul(draw['viewProjectionMatrixColumnMajor'],mul(draw['modelMatrixColumnMajor'],[*corner['rustCenteredPosition'],1]))
            projection_xy_errors.extend((abs(jc[0]/jc[3]-rc[0]/rc[3]),abs(jc[1]/jc[3]+rc[1]/rc[3])))
    java_quad_for_face={f['rustVboFaceIndex']:f for f in mapping['matching']['faces']}; assert set(java_quad_for_face)==set(range(6))
    java=png_read(root/'java/results/held-stone-on.png'); rust=png_read(root/'rust/results/held-stone-on.png'); w,h=rust[:2]; assert java[:2]==rust[:2]==(1280,720)
    with zipfile.ZipFile(a.client_jar) as jar: tex=jar.read('assets/minecraft/textures/block/stone.png')
    tw,th,texels=gray_png(tex); region=rust_debug['itemTintDiagnostics']['candidates'][-1]['atlasRegions']['stone']; assert (tw,th)==(16,16) and region['pixelRect'][2:]==[16,16]
    faces={}; all_samples=[]; mask_by_winding={-1:set(),1:set()}; allmask=set(); triangles=[]
    for face_index in range(6):
        face=verts[face_index*6:face_index*6+6]; java_face=java_quad_for_face[face_index]; name=java_face['normalDirection'].title()
        normal={tuple(v['normalBytes'][:3]) for v in face}; assert len(normal)==1 and tuple(java_face['rustNormalBytes'])==next(iter(normal))
        samples=[]; areas=[]
        for base in range(0,6,3):
            source=[]
            for v in face[base:base+3]:
                q=mul(draw['viewProjectionMatrixColumnMajor'],mul(draw['modelMatrixColumnMajor'],[*v['position'],1])); source.append([*q,*v['uv']])
            clipped=clip(source); raster=raster_triangle(source,w,h)
            fan_areas=[round(cross(((clipped[0][0]/clipped[0][3]+1)*w/2,(clipped[0][1]/clipped[0][3]+1)*h/2),((clipped[i][0]/clipped[i][3]+1)*w/2,(clipped[i][1]/clipped[i][3]+1)*h/2),((clipped[i+1][0]/clipped[i+1][3]+1)*w/2,(clipped[i+1][1]/clipped[i+1][3]+1)*h/2)),4) for i in range(1,len(clipped)-1)] if len(clipped)>=3 else []
            areas.extend(fan_areas)
            triangles.append({'drawTriangleIndex':face_index*2+base//3,'rustVboFaceIndex':face_index,'javaQuadIndex':java_face['javaQuadIndex'],'normalDirection':name,'inputVertexRange':[face_index*6+base,face_index*6+base+3],'clippedPolygonVertexCount':len(clipped),'fanTriangleCount':len(fan_areas),'fanSignedAreasFramebuffer':fan_areas,'coveredPixelCenters':len({(r[0],r[1]) for r in raster}),'negativeAreaSamples':sum(r[2]<0 for r in raster),'positiveAreaSamples':sum(r[2]>0 for r in raster),'uvBounds':[min((r[4] for r in raster),default=None),min((r[5] for r in raster),default=None),max((r[4] for r in raster),default=None),max((r[5] for r in raster),default=None)]})
            for x,y,area,z,u,v in raster:
                samples.append((x,y,name,z,u,v,area)); allmask.add((x,y)); mask_by_winding[-1 if area<0 else 1].add((x,y))
        for row in samples:
            x,y,_,z,u,v,area=row
            tx=max(0,min(15,int((u-region['uv'][0])/(region['uv'][2]-region['uv'][0])*16))); ty=max(0,min(15,int((v-region['uv'][1])/(region['uv'][3]-region['uv'][1])*16)))
            all_samples.append((x,y,name,z,u,v,tx,ty,area))
        faces[name]={'rustVboFaceIndex':face_index,'javaQuadIndex':java_face['javaQuadIndex'],'normalDirection':java_face['normalDirection'],'rustNormalBytes':list(next(iter(normal))),'trianglePixelCentersBeforeDepth':len({(r[0],r[1]) for r in samples}),'triangleSignedAreasFramebuffer':areas,'uvRect':region['uv']}
    # Back-face convention is established against the complete two-sided closed-cube silhouette,
    # not assumed from screen-coordinate intuition. Pipeline source: CCW + BACK cull.
    front_sign=-1 if mask_by_winding[-1] == allmask else 1 if mask_by_winding[1] == allmask else None
    assert front_sign is not None, f'neither CCW interpretation covers the complete cube silhouette: +/-={len(mask_by_winding[-1])}/{len(mask_by_winding[1])}, silhouette={len(allmask)}'
    nearest={}
    for row in all_samples:
        x,y,name,z,*_,area=row
        if (-1 if area<0 else 1) != front_sign: continue
        key=(x,y)
        if key not in nearest or z<nearest[key][3]: nearest[key]=row
    visible=set(nearest)
    bbox=[min(x for x,y in allmask),min(y for x,y in allmask),max(x for x,y in allmask)+1,max(y for x,y in allmask)+1]; bbox_pixels={(x,y) for y in range(bbox[1],bbox[3]) for x in range(bbox[0],bbox[2])}
    def regions(points):
        boundary={p for p in points if any((p[0]+dx,p[1]+dy) not in points for dx,dy in ((1,0),(-1,0),(0,1),(0,-1)))}
        return points-boundary,boundary
    interior,boundary=regions(allmask); hud={(x,y) for x,y in allmask if 416<=x<864 and y>=660}
    byface={}
    for name,record in faces.items():
        pts={(x,y) for x,y,n,*_ in nearest.values() if n==name}; byface[name]={**record,**metric(pts,java,rust)}
        rows=[r for r in nearest.values() if r[2]==name]
        if rows:
            row=rows[len(rows)//2]; x,y,_,z,u,v,tx,ty,_area=row
            byface[name]['sample']={'pixelXY':[x,y],'texelXY':[tx,ty],'sourceStoneRGB':texels[ty][tx],'javaRGB':java[2][y][x],'rustRGB':rust[2][y][x],'estimatedWindowDepth':z,'uv':[u,v]}
    report={'schema':2,'status':'saved-payload-cpu-projection-and-raw-png-analysis','artifactRoot':str(root),'rawCommonBBox':{'xyxy':bbox,'allPixelsIncludingBackground':True,**metric(bbox_pixels,java,rust)},'projectedPayloadMask':{'pixelCenters':len(allmask),'xyxy':bbox,'rawJavaRustRGB':metric(allmask,java,rust),'frontFaceDepthVisibleEstimate':metric(visible,java,rust),'silhouetteVsFrontFacePixelSymmetricDifference':len(allmask ^ mask_by_winding[front_sign]),'frontFaceFramebufferSignedAreaSign':front_sign,'interior':metric(interior,java,rust),'boundary':metric(boundary,java,rust),'estimatedNativeHudOverlap':metric(hud,java,rust),'backgroundWithinBBox':metric(bbox_pixels-allmask,java,rust),'hudOverlapHeuristic':'x=[416,864), y>=660; supplementary classification only, not excluded from raw metrics'},'faceProjectionAndUV':byface,'triangleCoverageDepthUvAudit':{'drawOrder':'36 vertices consumed as 12 actual TriangleList primitives, three vertices each; six-face association comes from verified Java quad/Rust VBO matching map','triangles':triangles},'geometryProof':{'javaMatchedFaces':len(mapping['matching']['faces']),'uniqueCubeCorners':len(cube_vertices),'uniqueBoundaryEdges':len(edge_use),'edgeIncidenceCounts':sorted(set(edge_use.values())),'quadWindingMatchesRecordedOutwardNormal':winding_normal_matches,'faceNormalsDerivedFromJavaQuadDataAndMatchedRustSNORM':'yes'},'actualPayload':{'vertexCount':payload['vertexCount'],'strideBytes':payload['strideBytes'],'mappedBytes':payload['mappedAllocationBytes'],'javaUncenteredPlusRustCenteredModelMaxWorldPositionAbsError':max(model_errors),'javaRustFramebufferNdcXYMaxAbsErrorAfterOpenGlToVulkanYFlip':max(projection_xy_errors),'javaProjectionMatrixColumnMajor':mapping['java']['projectionMatrixColumnMajor'],'javaCombinedModelViewProjectionMatrixColumnMajor':java_vp,'javaProjectionUseFrameIndex':mapping['java']['projectionUseFrameIndex'],'javaModelViewMatrixColumnMajor':jprep['renderSystemModelViewMatrixColumnMajor'],'javaFeaturePoseMatrixColumnMajor':jprep['finalFeaturePoseMatrixColumnMajor'],'rustModelMatrixColumnMajor':draw['modelMatrixColumnMajor'],'rustViewProjectionMatrixColumnMajor':draw['viewProjectionMatrixColumnMajor'],'javaVertexSpace':'actual baked quad corners are uncentered [0,1]; Rust VBO corners are centered [-0.5,0.5]','javaToRustVertexCorrespondence':'verified 6 Java quads x 4 corners match rustVboFaceIndex positions/UV after +0.5 centering in held-java-rust-quad-projection-map.json','normalPacking':'R8G8B8A8Snorm','atlasFormat':region['atlasFormat'],'filter':rust_debug['environment']['samplerState'] if 'environment' in rust_debug else 'NEAREST recorded in render-debug environment'},'rasterState':{'topology':'TriangleList; process actual VBO in groups of 3 vertices','pipelineFrontFace':'COUNTER_CLOCKWISE','cullMode':'BACK','depthCompare':'LESS_OR_EQUAL','viewport':'positive height; framebuffer y increases with NDC y under Vulkan viewport transform','clipZ':'0<=z<=w','depth':'screen-linear interpolation of z/w; perspective-correct UV via 1/w'},'method':'CPU homogeneous clipping, fan triangulation of every clipped triangle polygon, pixel-center coverage, perspective-correct UV and depth estimate, same-coordinate saved PNG comparison. Not GPU raster/readback.'}
    a.out.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8'); print(json.dumps({'bbox':bbox,'bboxRGBMAE':report['rawCommonBBox']['rgbMAE'],'silhouettePixels':len(allmask),'frontDepthPixels':len(visible),'silhouetteFrontSymDiff':len(allmask^mask_by_winding[front_sign]),'frontSign':front_sign,'faces':{k:{'pixels':v['pixels'],'MAE':v['rgbMAE']} for k,v in byface.items()}},sort_keys=True))

if __name__=='__main__': main()
