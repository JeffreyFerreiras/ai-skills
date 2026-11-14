"""Cover v2 ('shorter'): restack the v1 cover's own art (headline, diagram, gigachad bust) smaller and tighter so every
headline letter and the key art sit inside the centered 3:4 band (y 240 to 1680) and clear of the bottom 250 px (y < 1670).
No new text; canvas stays 1080x1920; background is plain board taken from v1."""
from pathlib import Path
import json,numpy as np
from PIL import Image,ImageDraw,ImageFilter
root=Path(__file__).resolve().parent;ev=root/'evidence'
v1=Image.open(root/'transactional-outbox-tiktok-cover-1080x1920.png').convert('RGB');W,H=1080,1920
# plain board: v1 rows 0..230 (no art), mirrored tiles, light blur to hide seams
strip=v1.crop((0,0,W,230));tile=Image.new('RGB',(W,460));tile.paste(strip,(0,0));tile.paste(strip.transpose(Image.FLIP_TOP_BOTTOM),(0,230))
bg=Image.new('RGB',(W,H))
for y in range(0,H,460):bg.paste(tile,(0,y))
bg=bg.filter(ImageFilter.GaussianBlur(1.5))
def piece(box,scale,fade_bottom=0,feather=26):
  p=v1.crop(box);w,h=p.size;p=p.resize((round(w*scale),round(h*scale)),Image.Resampling.LANCZOS);w,h=p.size
  m=np.ones((h,w),np.float32);f=feather
  r=np.clip(np.arange(h)/f,0,1);m*=np.minimum(r,r[::-1])[:,None]
  if fade_bottom:m[h-fade_bottom:]*=np.linspace(1,0,fade_bottom)[:,None]**1.4
  c=np.clip(np.arange(w)/f,0,1);m*=np.minimum(c,c[::-1])[None,:]
  return p,Image.fromarray((m*255).astype(np.uint8))
# (source box in v1, scale, fade) ; stacked top-down from y0 with gaps
parts=[((0,236,W,646),0.90,0),((0,650,W,1166),0.74,0),((0,1150,W,1920),0.70,150)]
y=296;gap=10;placed=[]
for box,s,fb in parts:
  p,m=piece(box,s,fb);x=(W-p.size[0])//2;bg.paste(p,(x,y),m);placed.append({'src':box,'scale':s,'dst':[x,y,x+p.size[0],y+p.size[1]]});y+=p.size[1]+gap
final=bg;dst=root/'transactional-outbox-tiktok-cover-1080x1920-v2.png';final.save(dst)
A=np.asarray(final).astype(int);ink=(A.max(2)>120)&((A.max(2)-A.min(2)>60)|(A.min(2)>150));r=np.where(ink.sum(1)>3)[0];c=np.where(ink.sum(0)>3)[0]
meta={'source':'v1 cover art restacked','parts':placed,'content_bbox':[int(c.min()),int(r.min()),int(c.max()),int(r.max())],'band_3x4':[0,240,1080,1680],'bottom_clear_y':1670}
(ev/'composition-v2.json').write_text(json.dumps(meta,indent=1));print(meta)
final.crop((0,240,W,1680)).save(ev/'cover-v2-center-3x4-crop.png')
final.resize((240,427),Image.Resampling.LANCZOS).save(ev/'cover-v2-thumb-240.png')
final.crop((0,240,W,1680)).resize((240,320),Image.Resampling.LANCZOS).save(ev/'cover-v2-grid-3x4-thumb-240.png')
ov=Image.new('RGBA',final.size,(0,0,0,0));d=ImageDraw.Draw(ov)
for q in [(0,0,W,240),(0,1670,W,H)]:d.rectangle(q,fill=(255,0,0,70))
Image.alpha_composite(final.convert('RGBA'),ov).convert('RGB').resize((540,960)).save(ev/'cover-v2-safe-zone-overlay.png')
