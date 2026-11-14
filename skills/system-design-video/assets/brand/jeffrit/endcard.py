"""Jeffrit end card (reusable across episodes). Pure Pillow/NumPy, deterministic.

This module lives in the skill brand kit, next to layers/. Put that directory
on sys.path, then:

    from endcard import EndCard
    EC=EndCard(board_rgb)                     # board_rgb: the episode's 1080x1920 chalkboard background (PIL RGB)
    img,state=EC.frame(t_local, prev=last_frame_of_previous_scene)   # t_local in 0..EC.LEN
    EC.cues(t0)                               # absolute times for SFX (ring draw, pop, chalk letters, underline, glint, final fade)
    EC.boxes                                  # layout boxes for frame checks ('brand' kind, text 'Jeffrit')
Timeline (local seconds): 0.00-0.60 crossfade from the previous frame to the board (no hard cut); 0.30-0.95 chalk ring draws in;
0.60-1.00 the bust pops in (back-out scale); 1.02-1.95 'Jeffrit' writes itself in chalk; 1.90-2.25 underline; 2.20 jaw glint;
last 0.45 s gentle fade to near black so the picture resolves with the music tail. No other text is drawn."""
from pathlib import Path
import math,numpy as np
from PIL import Image,ImageDraw,ImageFilter
B=Path(__file__).parent;LY=B/'layers'
W,H=1080,1920
def _ease(k):k=max(0.0,min(1.0,k));return k*k*(3-2*k)
def _lin(t,a,b):return max(0.0,min(1.0,(t-a)/max(1e-9,b-a)))
def _backout(k,s=1.9):k=max(0.0,min(1.0,k));k-=1;return 1+k*k*((s+1)*k+s)
class EndCard:
  LEN=2.9;T_IN=0.60;RING=(0.30,0.95);POP=(0.60,1.00);WORD=(1.02,1.95);UL=(1.90,2.25);GLINT=2.20;OUT=0.45
  def __init__(self,board,badge_w=760,badge_top=330,word_w=700,word_gap=46):
    self.board=board.convert('RGB');self.LEN=type(self).LEN
    v=(LY/'badge-geom.txt').read_text().split();cx,cy,R,S,Hb=float(v[1]),float(v[2]),float(v[4]),int(v[6]),int(v[7])
    s=badge_w/S;self.s=s;sz=(badge_w,round(Hb*s));self.bx=(W-badge_w)//2;self.by=badge_top
    ld=lambda n,size:Image.open(LY/n).convert('RGBA').resize(size,Image.Resampling.LANCZOS)
    self.disk=ld('disk.png',sz);self.ring=ld('ring.png',sz)
    front=Image.alpha_composite(ld('bust-clipped.png',sz),ld('bust-outline.png',sz));self.front=front
    self.cx,self.cy,self.R=self.bx+cx*s,self.by+cy*s,R*s
    yy,xx=np.mgrid[0:sz[1],0:sz[0]];self.ring_ang=((np.arctan2(yy-cy*s,xx-cx*s)+math.pi/2)%(2*math.pi))/(2*math.pi)
    self.ring_a=np.asarray(self.ring)[...,3].astype(float)
    wm=Image.open(LY/'word.png').convert('RGBA');ws=word_w/wm.width;wsz=(word_w,round(wm.height*ws))
    self.word=wm.resize(wsz,Image.Resampling.LANCZOS);self.ul=Image.open(LY/'underline.png').convert('RGBA').resize(wsz,Image.Resampling.LANCZOS)
    wa=np.asarray(self.word)[...,3];cols=np.where(wa.max(0)>40)[0];rows=np.where(wa.max(1)>40)[0]
    self.wx=(W-word_w)//2;self.wy=round(self.cy+self.R)+word_gap-int(rows[0])  # glyph tops sit word_gap px under the ring
    self.gx0,self.gx1,self.gy0,self.gy1=cols[0],cols[-1],rows[0],rows[-1]
    ua=np.asarray(self.ul)[...,3];ucols=np.where(ua.max(0)>40)[0];urows=np.where(ua.max(1)>40)[0];self.ux0,self.ux1=ucols[0],ucols[-1]
    self.word_a=wa.astype(float);self.ul_a=ua.astype(float)
    # the glint sits on the chiseled jaw (bust layer coords measured on jeffrit-bust.png)
    self.glint_xy=(self.bx+(20+600)*s,self.by+640*s)
    fb=front.getbbox();self.boxes=[('person',(self.bx+min(fb[0],self.cx-self.bx-self.R),self.by+fb[1],self.bx+max(fb[2],self.cx-self.bx+self.R),self.cy+self.R+8),'jeffrit badge'),
      ('brand',(self.wx+self.gx0,self.wy+self.gy0,self.wx+max(self.gx1,self.ux1),self.wy+urows[-1]),'Jeffrit')]
    rng=np.random.default_rng(7);self.dust=[(rng.uniform(-14,14),rng.uniform(-10,30),rng.uniform(1.5,3.5),rng.uniform(0.15,0.4)) for _ in range(40)]
  def cues(self,t0):
    c={'endcard_in':t0,'ring':t0+self.RING[0],'pop':t0+self.POP[0]+0.08,'word':[t0+self.WORD[0]+k*(self.WORD[1]-self.WORD[0])/7 for k in range(7)],
       'underline':t0+self.UL[0],'glint':t0+self.GLINT,'fade':t0+self.LEN-self.OUT,'end':t0+self.LEN}
    return c
  def _masked(self,lay,alpha):
    a=np.asarray(lay).copy();a[...,3]=np.clip(alpha,0,255).astype(np.uint8);return Image.fromarray(a,'RGBA')
  def frame(self,t,prev=None):
    f=self.board.copy().convert('RGBA');st={}
    kr=_ease(_lin(t,*self.RING));st['ring']=kr
    kp=_lin(t,*self.POP);sc=_backout(kp) if kp>0 else 0;st['pop']=kp
    if kp>0:
      al=min(1.0,kp*2.5);w_=max(2,round(self.disk.width*sc));h_=max(2,round(self.disk.height*sc))
      ox=round(self.cx-(self.cx-self.bx)*sc);oy=round(self.cy-(self.cy-self.by)*sc)
      for lay in (self.disk,):
        l2=lay.resize((w_,h_),Image.Resampling.BILINEAR);a=np.asarray(l2)[...,3]*al;f.alpha_composite(self._masked(l2,a),(ox,oy)) if ox>=0 and oy>=0 else f.paste(self._masked(l2,a),(ox,oy),self._masked(l2,a))
    if kr>0:
      a=self.ring_a*(self.ring_ang<=kr);f.alpha_composite(self._masked(self.ring,a),(self.bx,self.by))
      if kr<1:  # chalk tip on the ring
        th=-math.pi/2+2*math.pi*kr;tx,ty=self.cx+self.R*math.cos(th),self.cy+self.R*math.sin(th);d=ImageDraw.Draw(f);d.ellipse((tx-7,ty-7,tx+7,ty+7),fill=(245,248,246,230))
    if kp>0:
      l2=self.front.resize((w_,h_),Image.Resampling.BILINEAR);a=np.asarray(l2)[...,3]*al;m=self._masked(l2,a)
      f.paste(m,(ox,oy),m) if (ox<0 or oy<0) else f.alpha_composite(m,(ox,oy))
    kw=_lin(t,*self.WORD);st['word']=kw
    if kw>0:
      xw=self.gx0+(self.gx1-self.gx0+30)*_ease(kw)*1.0;cols=np.arange(self.word.width)[None,:]
      ramp=np.clip((xw-cols)/18.0,0,1);f.alpha_composite(self._masked(self.word,self.word_a*ramp),(self.wx,self.wy))
      if kw<1:
        d=ImageDraw.Draw(f);tx=self.wx+xw;ty=self.wy+(self.gy0+self.gy1)/2
        for dx,dy,r,a0 in self.dust:d.ellipse((tx+dx-r,ty+dy-r,tx+dx+r,ty+dy+r),fill=(235,240,238,int(255*a0)))
    ku=_ease(_lin(t,*self.UL));st['underline']=ku
    if ku>0:
      xu=self.ux0+(self.ux1-self.ux0+20)*ku;cols=np.arange(self.ul.width)[None,:];ramp=np.clip((xu-cols)/14.0,0,1)
      f.alpha_composite(self._masked(self.ul,self.ul_a*ramp),(self.wx,self.wy))
    kg=_lin(t,self.GLINT,self.GLINT+0.45);st['glint']=kg
    if 0<kg<1:
      d=ImageDraw.Draw(f);x,y=self.glint_xy;r=34*math.sin(math.pi*kg);a=int(255*math.sin(math.pi*kg))
      d.polygon([(x,y-r),(x+r*0.2,y-r*0.2),(x+r,y),(x+r*0.2,y+r*0.2),(x,y+r),(x-r*0.2,y+r*0.2),(x-r,y),(x-r*0.2,y-r*0.2)],fill=(220,250,255,a))
    f=f.convert('RGB')
    if prev is not None and t<self.T_IN:f=Image.blend(prev.convert('RGB'),f,_ease(t/self.T_IN))
    ko=_lin(t,self.LEN-self.OUT,self.LEN);st['out']=ko
    if ko>0:f=Image.blend(f,Image.new('RGB',(W,H),(4,7,9)),0.92*_ease(ko))
    return f,st
if __name__=='__main__':
  # standalone preview on a plain dark board: still at 2.4 s plus a contact strip
  bd=Image.new('RGB',(W,H),(17,25,29));ec=EndCard(bd);im,_=ec.frame(2.4);im.save(B/'jeffrit-endcard-preview.png')
  ts=[0.2,0.45,0.7,0.85,1.2,1.6,2.0,2.3,2.6,2.85];sh=Image.new('RGB',(len(ts)*216,384))
  for i,t in enumerate(ts):sh.paste(ec.frame(t,prev=Image.new('RGB',(W,H),(40,40,40)))[0].resize((216,384)),(i*216,0))
  sh.save(B/'layers/endcard-strip-preview.png');print('ok',ec.boxes)
