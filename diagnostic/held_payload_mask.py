#!/usr/bin/env python3
"""CPU-clipped pixel-center mask for the exact held VBO payload and on/off captures."""
from __future__ import annotations
import argparse, json
from pathlib import Path
from compare_item_overlay import png_read


def mul(a, v):
    return [sum(a[c * 4 + r] * v[c] for c in range(4)) for r in range(4)]


def clip(poly):
    planes = (lambda p: p[0]+p[3], lambda p: p[3]-p[0], lambda p: p[1]+p[3], lambda p: p[3]-p[1], lambda p: p[2], lambda p: p[3]-p[2])
    for dist in planes:
        if not poly: break
        out=[]; prev=poly[-1]; dp=dist(prev)
        for cur in poly:
            dc=dist(cur)
            if (dc >= 0) != (dp >= 0):
                t=dp/(dp-dc); out.append([prev[i]+t*(cur[i]-prev[i]) for i in range(4)])
            if dc >= 0: out.append(cur)
            prev,dp=cur,dc
        poly=out
    return poly


def inside(px, py, poly):
    sign=None
    for i,a in enumerate(poly):
        b=poly[(i+1)%len(poly)]; e=(b[0]-a[0])*(py-a[1])-(b[1]-a[1])*(px-a[0])
        if abs(e)<1e-8: continue
        current=e>0
        if sign is not None and current != sign: return False
        sign=current
    return True


def bbox(points):
    return None if not points else [min(x for x,y in points),min(y for x,y in points),max(x for x,y in points)+1,max(y for x,y in points)+1]


def main():
    p=argparse.ArgumentParser(); p.add_argument('--trace',type=Path,required=True); p.add_argument('--java-on',type=Path,required=True); p.add_argument('--java-off',type=Path,required=True); p.add_argument('--rust-on',type=Path,required=True); p.add_argument('--rust-off',type=Path,required=True); p.add_argument('--out',type=Path,required=True); a=p.parse_args()
    d=json.loads(a.trace.read_text(encoding='utf-8'))['heldItemPipeline']['draw']; payload=d['vertexPayload']; verts=payload['vertices']; assert len(verts)==d['vertexCount']==36 and payload['boundByteOffset']==0
    width,height,rows=png_read(a.rust_on); assert (width,height)==(1280,720)
    model=d['modelMatrixColumnMajor']; vp=d['viewProjectionMatrixColumnMajor']; mask=set(); face_polys=[]
    for base in range(0,len(verts),3):
        tri=[]
        for v in verts[base:base+3]:
            w=mul(model,[*v['position'],1.0]); tri.append(mul(vp,w))
        poly=clip(tri)
        if len(poly)<3: continue
        screen=[((q[0]/q[3]+1)*width/2,(q[1]/q[3]+1)*height/2) for q in poly]
        face_polys.append(screen); x0=max(0,int(min(x for x,y in screen)-1)); x1=min(width,int(max(x for x,y in screen)+1)); y0=max(0,int(min(y for x,y in screen)-1)); y1=min(height,int(max(y for x,y in screen)+1))
        for y in range(y0,y1):
            for x in range(x0,x1):
                if inside(x+.5,y+.5,screen): mask.add((x,y))
    images={name:png_read(path) for name,path in [('java',a.java_on),('javaOff',a.java_off),('rust',a.rust_on),('rustOff',a.rust_off)]}
    assert all((w,h)==(width,height) for w,h,_ in images.values())
    results={}
    for client,on,off in [('java','java','javaOff'),('rust','rust','rustOff')]:
        changed=[(x,y) for x,y in mask if images[on][2][y][x] != images[off][2][y][x]]
        results[client]={'changedPixelsInsideSubmittedPayloadMask':len(changed),'maskChangedFraction':len(changed)/len(mask) if mask else None,'changedBBoxWithinMask':bbox(changed)}
    report={'schema':1,'status':'cpu-projected-payload-mask','imageSize':[width,height],'maskPixelCenters':len(mask),'maskBBoxXYXY':bbox(mask),'perClient':results,'submittedDraw':{'item':d['item'],'buffer':payload['buffer'],'boundByteOffset':payload['boundByteOffset'],'vertexCount':payload['vertexCount'],'strideBytes':payload['strideBytes'],'mappedAllocationOffset':payload['mappedAllocationOffset'],'mappedAllocationBytes':payload['mappedAllocationBytes'],'vertexBounds':[[min(v['position'][i] for v in verts),max(v['position'][i] for v in verts)] for i in range(3)],'modelMatrixColumnMajor':model,'viewProjectionMatrixColumnMajor':vp,'source':payload['provenance']},'method':'CPU homogeneous clipping against Vulkan x/y/z planes, then pixel-center polygon membership; diagnostic estimate, not GPU rasterization/readback; raw on/off PNG bytes, no alignment or correction'}
    a.out.parent.mkdir(parents=True,exist_ok=True); a.out.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8'); print(json.dumps({'maskPixels':len(mask),'maskBBoxXYXY':report['maskBBoxXYXY'],'perClient':results},sort_keys=True))

if __name__=='__main__': main()
