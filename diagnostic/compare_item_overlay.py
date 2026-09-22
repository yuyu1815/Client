#!/usr/bin/env python3
"""Compare fixed GUI item rectangles with only Python stdlib; no alignment/correction."""
from __future__ import annotations
import argparse, json, struct, zlib
from pathlib import Path


def png_read(path: Path):
    data = path.read_bytes(); assert data[:8] == b"\x89PNG\r\n\x1a\n"
    pos = 8; raw = bytearray(); w = h = depth = kind = None
    while pos < len(data):
        n = struct.unpack(">I", data[pos:pos+4])[0]; typ = data[pos+4:pos+8]; chunk = data[pos+8:pos+8+n]; pos += 12+n
        if typ == b"IHDR": w, h, depth, kind = struct.unpack(">IIBB", chunk[:10]); assert depth == 8 and kind in (2, 6)
        elif typ == b"IDAT": raw.extend(chunk)
        elif typ == b"IEND": break
    channels = 3 if kind == 2 else 4; stride = w * channels; decoded = zlib.decompress(raw); rows=[]; prev=bytearray(stride); off=0
    for _ in range(h):
        f=decoded[off]; off += 1; cur=bytearray(decoded[off:off+stride]); off += stride
        for i in range(stride):
            a=cur[i-channels] if i >= channels else 0; b=prev[i]; c=prev[i-channels] if i >= channels else 0
            if f == 1: cur[i]=(cur[i]+a)&255
            elif f == 2: cur[i]=(cur[i]+b)&255
            elif f == 3: cur[i]=(cur[i]+((a+b)//2))&255
            elif f == 4:
                p=a+b-c; pa=abs(p-a); pb=abs(p-b); pc=abs(p-c); pr=a if pa<=pb and pa<=pc else (b if pb<=pc else c); cur[i]=(cur[i]+pr)&255
            elif f != 0: raise ValueError(f"unsupported PNG filter {f}")
        rows.append([tuple(cur[i:i+3]) for i in range(0,stride,channels)]); prev=cur
    return w,h,rows


def png_write(path, w, h, rows):
    raw=b"".join(b"\0"+bytes(c for px in row for c in px) for row in rows)
    def chunk(t,p): return struct.pack(">I",len(p))+t+p+struct.pack(">I",zlib.crc32(t+p)&0xffffffff)
    path.write_bytes(b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",w,h,8,2,0,0,0))+chunk(b"IDAT",zlib.compress(raw,9))+chunk(b"IEND",b""))


def crop(img, rect):
    w,h,rows=img; x,y,rw,rh=rect
    if x<0 or y<0 or x+rw>w or y+rh>h: raise ValueError(f"rect outside PNG: {rect} / {w}x{h}")
    return [[rows[y+j][x+i] for i in range(rw)] for j in range(rh)]


def metrics(a,b,background):
    values=[abs(x-y) for ar,br in zip(a,b) for pa,pb in zip(ar,br) for x,y in zip(pa,pb)]; pixels=len(a)*len(a[0]); threshold=sum(any(abs(x-y)>8 for x,y in zip(pa,pb)) for ar,br in zip(a,b) for pa,pb in zip(ar,br))
    return {"pixels":pixels,"rgbMAE":sum(values)/len(values),"maxChannelDiff":max(values,default=0),"channelThreshold":8,"channelThresholdFraction":threshold/pixels,"javaNeutralBackgroundPixels":sum(px==tuple(background) for row in a for px in row),"rustNeutralBackgroundPixels":sum(px==tuple(background) for row in b for px in row),"neutralBackgroundIncluded":True,"textureAlpha0PixelsIncluded":True}

FONT={"A":"01110/10001/10001/11111/10001/10001/10001","B":"11110/10001/11110/10001/10001/10001/11110","C":"01111/10000/10000/10000/10000/10000/01111","D":"11110/10001/10001/10001/10001/10001/11110","E":"11111/10000/10000/11110/10000/10000/11111","F":"11111/10000/10000/11110/10000/10000/10000","G":"01111/10000/10000/10111/10001/10001/01111","H":"10001/10001/10001/11111/10001/10001/10001","I":"11111/00100/00100/00100/00100/00100/11111","J":"00111/00010/00010/00010/10010/10010/01100","L":"10000/10000/10000/10000/10000/10000/11111","N":"10001/11001/10101/10011/10001/10001/10001","O":"01110/10001/10001/10001/10001/10001/01110","P":"11110/10001/10001/11110/10000/10000/10000","R":"11110/10001/10001/11110/10100/10010/10001","S":"01111/10000/10000/01110/00001/00001/11110","T":"11111/00100/00100/00100/00100/00100/00100","U":"10001/10001/10001/10001/10001/10001/01110","V":"10001/10001/10001/10001/10001/01010/00100","W":"10001/10001/10001/10101/10101/11011/10001","Y":"10001/10001/01010/00100/00100/00100/00100","_":"00000/00000/00000/00000/00000/00000/11111"," ":"00000/00000/00000/00000/00000/00000/00000"}

def label(canvas,x,y,text):
    for ch in text.upper():
        bits=FONT.get(ch,FONT["_"]).split("/")
        for yy,row in enumerate(bits):
            for xx,v in enumerate(row):
                if v=="1" and 0<=y+yy<len(canvas) and 0<=x+xx<len(canvas[0]): canvas[y+yy][x+xx]=(255,255,255)
        x+=6

def contact(path,java,rust,entries):
    scale=2; cw=entries[0][1][2]*scale; ch=entries[0][1][3]*scale; gap=8; lh=9; outw=cw*2+gap*3; outh=gap+len(entries)*(ch+lh+gap); canvas=[[(18,18,18)]*outw for _ in range(outh)]
    for i,(name,(x,y,w,h)) in enumerate(entries):
        top=gap+i*(ch+lh+gap)
        for left,src,title in ((gap,java,"JAVA"),(gap*2+cw,rust,"RUST")):
            c=crop(src,(x,y,w,h))
            for yy,row in enumerate(c):
                for xx,px in enumerate(row):
                    for dy in range(scale):
                        for dx in range(scale): canvas[top+yy*scale+dy][left+xx*scale+dx]=px
            label(canvas,left,top+ch,title+" "+name)
    png_write(path,outw,outh,canvas)


def main():
    p=argparse.ArgumentParser(); p.add_argument("--java-png",type=Path,required=True); p.add_argument("--rust-png",type=Path,required=True); p.add_argument("--java-meta",type=Path,required=True); p.add_argument("--rust-meta",type=Path,required=True); p.add_argument("--java-debug",type=Path,required=True); p.add_argument("--rust-debug",type=Path,required=True); p.add_argument("--out",type=Path,required=True); a=p.parse_args()
    jm=json.loads(a.java_meta.read_text(encoding="utf-8")); rm=json.loads(a.rust_meta.read_text(encoding="utf-8")); jd=json.loads(a.java_debug.read_text(encoding="utf-8")); rd=json.loads(a.rust_debug.read_text(encoding="utf-8")); jo=jm["itemOverlay"]; ro=rm["itemOverlay"]
    entries=[(i["id"],tuple(i["physicalRect"])) for i in jo["items"]]; assert entries==[(i["id"],tuple(i["physicalRect"])) for i in ro["items"]]; assert jo["panelPhysicalRect"]==ro["panelPhysicalRect"] and jo["backgroundRGB"]==ro["backgroundRGB"]
    assert jo["guiHidden"] is False and rm["hudHidden"] is False; trace=rd["guiItemOverlay"]; assert trace["guiItemDrawConfirmed"] is True
    java=png_read(a.java_png); rust=png_read(a.rust_png); report={"schema":1,"status":"visible-comparison","comparator":"production per-item fixed physical rectangle; no terrain projection","sameScreenROI":True,"fixedPhysicalPixels":True,"backgroundRGB":jo["backgroundRGB"],"metricLimit":jo["metricLimit"],"javaHudActualSource":jo["stage"],"rustActualSource":trace["pipeline"],"sourceEvidence":{"javaItemTintDiagnostics":jd.get("itemTintDiagnostics"),"rustItemTintDiagnostics":rd.get("itemTintDiagnostics")},"javaReadback":{"captureSource":jm.get("captureSource"),"frameReadbackCompletedAt":jm.get("frameReadbackCompletedAt")},"rustReadback":{"captureSource":rm.get("captureSource"),"frameReadbackCompletedAt":rm.get("frameReadbackCompletedAt"),"frameSubmission":trace.get("frameSubmission")},"items":[]}
    for name,rect in entries: report["items"].append({"id":name,"physicalRect":list(rect),**metrics(crop(java,rect),crop(rust,rect),jo["backgroundRGB"])})
    report["aggregate"]={"itemCount":len(entries),"meanRgbMAE":sum(i["rgbMAE"] for i in report["items"])/len(entries)}; a.out.parent.mkdir(parents=True,exist_ok=True); a.out.write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8"); contact(a.out.with_name("item-overlay-contact-sheet.png"),java,rust,entries); print(json.dumps(report["aggregate"],sort_keys=True))

if __name__=="__main__": main()
