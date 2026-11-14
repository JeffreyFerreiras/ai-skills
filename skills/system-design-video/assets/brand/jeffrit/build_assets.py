"""Jeffrit brand assets (reusable). Builds, from src/jeffrit-gigachad-raw.jpg (GenerateImage chalk caricature on magenta):
  jeffrit-bust.png          keyed head-and-shoulders drawing, alpha
  jeffrit-badge.png         round chalk badge: bust pops out of the top of a chalk ring, alpha
  jeffrit-wordmark.png      'Jeffrit' chalk lettering + underline, alpha
  jeffrit-logo.png          badge over wordmark lockup, alpha (static logo for thumbnails etc.)
Layers used by endcard.py animation are saved under layers/. Deterministic (fixed seeds)."""
from pathlib import Path
import numpy as np,math
from PIL import Image,ImageDraw,ImageFilter,ImageFont,ImageChops
B=Path(__file__).parent;L=B/'layers';L.mkdir(exist_ok=True)
CHALK=(236,242,238);FONT='/usr/share/fonts/truetype/sand-box/google/Caveat Brush/CaveatBrush-Regular.ttf'
def grain(size,seed,lo=0.62):
  """chalk texture: fine speckle plus streaks along x, values lo..1"""
  r=np.random.default_rng(seed);n=r.random((size[1],size[0]))
  n=np.asarray(Image.fromarray((n*255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(1.1))).astype(float)/255
  st=r.random((size[1],max(2,size[0]//24)));st=np.asarray(Image.fromarray((st*255).astype(np.uint8)).resize(size,Image.Resampling.BICUBIC)).astype(float)/255
  v=0.6*(n-0.5)/0.12+0.4*(st-0.5)/0.25
  return np.clip(1.0-(1-lo)*np.clip(-v,0,1.5)/1.5,lo,1.0)
def key_bust():
  im=Image.open(B/'src/jeffrit-gigachad-raw.jpg').convert('RGB');a=np.asarray(im).astype(float)
  bg=np.median(np.r_[a[:30,:30].reshape(-1,3),a[:30,-30:].reshape(-1,3),a[600:700,:20].reshape(-1,3)],0)
  d=np.sqrt(((a-bg)**2).sum(-1));al=np.clip((d-45)/55,0,1)
  # only background connected to the border is removed (protects the pink/cyan code glow on the cheeks)
  bin_=Image.fromarray(((al<0.5)*255).astype(np.uint8)).copy();W_,H_=bin_.size
  for seed in [(0,0),(W_-1,0),(0,600),(W_-1,600),(W_//2,0)]:ImageDraw.floodfill(bin_,seed,128)
  bgc=np.asarray(bin_)==128;al=np.where(bgc,al,1.0)
  al=np.asarray(Image.fromarray((al*255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(0.8))).astype(float)/255
  # un-mix the magenta from soft edge pixels, then clamp any leftover spill on the rim
  al3=np.maximum(al,0.02)[...,None];a=np.where(al[...,None]<0.999,(a-(1-al3)*bg)/al3,a).clip(0,255)
  edge=np.asarray(Image.fromarray(((al<0.98)*255).astype(np.uint8)).filter(ImageFilter.MaxFilter(13)))>0
  r,g,b_=a[...,0].copy(),a[...,1].copy(),a[...,2].copy()
  a[...,0]=np.where(edge,np.minimum(r,np.maximum(g,b_)+4),r);a[...,2]=np.where(edge,np.minimum(b_,np.maximum(g,r)+18),b_)
  # hair strands near the rim carry a magenta cast: pull any magenta (min(R,B) above G) back to grey within 48 px of the rim
  band=np.asarray(Image.fromarray(((al<0.98)*255).astype(np.uint8)).resize((W_//4,H_//4)).filter(ImageFilter.MaxFilter(25)).resize((W_,H_)))>0
  mg=np.clip(np.minimum(a[...,0],a[...,2])-a[...,1],0,None)*band;a[...,0]-=mg;a[...,2]-=mg
  out=np.dstack([a,al*255]).clip(0,255).astype(np.uint8);bust=Image.fromarray(out,'RGBA');bust.save(B/'jeffrit-bust.png');return bust
def rough_ring(d,c,r,w,seed,col=CHALK,k0=0.0,k1=1.0):
  rng=np.random.default_rng(seed)
  for p in range(2):
    n=360;ph=rng.uniform(0,6.28);pts=[]
    for i in range(int(n*k0),int(n*k1)+1):
      th=-math.pi/2+2*math.pi*i/n;rr=r+3*math.sin(th*3+ph)+rng.normal(0,0.7)+p*5
      pts.append((c[0]+rr*math.cos(th),c[1]+rr*math.sin(th)))
    if len(pts)>1:d.line(pts,fill=col+((255 if p==0 else 150),),width=w if p==0 else max(2,w//2),joint='curve')
def badge(bust):
  W_,H_=bust.size;cx,cy,R=W_//2,600,405;S=W_+40
  canvas=Image.new('RGBA',(S,H_),(0,0,0,0));ox=20
  disk=Image.new('RGBA',(S,H_),(0,0,0,0));dd=ImageDraw.Draw(disk);dd.ellipse((ox+cx-R,cy-R,ox+cx+R,cy+R),fill=(30,40,46,235))
  ring=Image.new('RGBA',(S,H_),(0,0,0,0));rough_ring(ImageDraw.Draw(ring),(ox+cx,cy),R,14,3)
  g=grain((S,H_),5);ra=np.asarray(ring).astype(float);ra[...,3]*=g;ring=Image.fromarray(ra.astype(np.uint8),'RGBA')
  m=Image.new('L',(S,H_),0);md=ImageDraw.Draw(m);md.ellipse((ox+cx-R+8,cy-R+8,ox+cx+R-8,cy+R-8),fill=255);md.rectangle((0,0,S,cy-60),fill=255)
  bb=Image.new('RGBA',(S,H_),(0,0,0,0));bb.paste(bust,(ox,0));ba=np.asarray(bb).astype(float);ba[...,3]*=np.asarray(m).astype(float)/255;bb=Image.fromarray(ba.astype(np.uint8),'RGBA')
  # chalk sticker outline around the part of the bust that pops out above the ring
  sil=bb.getchannel('A').point(lambda v:255 if v>110 else 0);top=Image.new('L',(S,H_),0);ImageDraw.Draw(top).rectangle((0,0,S,cy-40),fill=255)
  ring_out=ImageChops.subtract(sil.filter(ImageFilter.MaxFilter(15)),sil);ring_out=ImageChops.multiply(ring_out,top)
  inside=Image.new('L',(S,H_),0);ImageDraw.Draw(inside).ellipse((ox+cx-R-4,cy-R-4,ox+cx+R+4,cy+R+4),fill=255)
  ring_out=ImageChops.subtract(ring_out,inside)
  oa=np.asarray(ring_out).astype(float)*grain((S,H_),9)
  outline=Image.fromarray(np.dstack([np.full((H_,S),c,np.uint8) for c in CHALK]+[oa.astype(np.uint8)]),'RGBA')
  disk.save(L/'disk.png');ring.save(L/'ring.png');bb.save(L/'bust-clipped.png');outline.save(L/'bust-outline.png')
  out=Image.alpha_composite(Image.alpha_composite(Image.alpha_composite(disk,ring),bb),outline)  # ring sits behind the head: he pops out of the badge
  out.save(B/'jeffrit-badge.png');(B/'layers/badge-geom.txt').write_text(f'center {ox+cx} {cy} radius {R} size {S} {H_}\n');return out
def wordmark():
  f=ImageFont.truetype(FONT,330);txt='Jeffrit';tmp=ImageDraw.Draw(Image.new('L',(10,10)))
  x0,y0,x1,y1=tmp.textbbox((0,0),txt,font=f);Wm,Hm=x1-x0+60,y1-y0+150
  m=Image.new('L',(Wm,Hm),0);ImageDraw.Draw(m).text((30-x0,20-y0),txt,font=f,fill=255)
  ul=Image.new('L',(Wm,Hm),0);ud=ImageDraw.Draw(ul);pts=[(40+i*(Wm-80)/40,y1-y0+70+10*math.sin(i/40*math.pi)-14*(i/40)) for i in range(41)];ud.line(pts,fill=255,width=16,joint='curve')
  for name,lay,seed in (('word',m,21),('underline',ul,22)):
    a=np.asarray(lay).astype(float)*grain((Wm,Hm),seed,0.45)
    Image.fromarray(np.dstack([np.full((Hm,Wm),c,np.uint8) for c in CHALK]+[a.astype(np.uint8)]),'RGBA').save(L/f'{name}.png')
  # per-letter x extents for the draw-in
  xs=[];acc=''
  for ch in txt:
    a0=tmp.textlength(acc,font=f);acc+=ch;xs.append((30+a0,30+tmp.textlength(acc,font=f)))
  (L/'word-letters.txt').write_text('\n'.join(f'{c} {a:.1f} {b:.1f}' for c,(a,b) in zip(txt,xs)))
  w=Image.alpha_composite(Image.open(L/'word.png'),Image.open(L/'underline.png'));w.save(B/'jeffrit-wordmark.png');return w
def lockup(bd,wm):
  s=900/bd.width;bd2=bd.resize((900,round(bd.height*s)),Image.Resampling.LANCZOS);wm2=wm.resize((760,round(wm.height*760/wm.width)),Image.Resampling.LANCZOS)
  out=Image.new('RGBA',(1000,bd2.height+wm2.height-20),(0,0,0,0));out.alpha_composite(bd2,(50,0));out.alpha_composite(wm2,(120,bd2.height-40));out.save(B/'jeffrit-logo.png')
if __name__=='__main__':
  b=key_bust();bd=badge(b);wm=wordmark();lockup(bd,wm);print('ok',b.size,bd.size,wm.size)
