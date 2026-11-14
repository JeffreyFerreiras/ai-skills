"""Shared chalk kit (copied unchanged from cartesian-explosion final-v2 render.py, lines noted) for the transactional-outbox renderer."""
from pathlib import Path
import json,math,re
import numpy as np
from PIL import Image,ImageDraw,ImageFont,ImageFilter
from pygments import lex
from pygments.lexers import CSharpLexer
from pygments.token import Token
W,H,FPS=1080,1920,24
CY=(80,230,255);IV=(235,244,239);AM=(255,207,124);GR=(128,244,177);VI=(228,163,247);MUT=(158,184,198);MG=(255,92,170);DARK=(9,17,25)
FONT='/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf';BOLD='/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf';MONO='/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf'
FONTS={}
def font(n,b=False,mono=False):
 k=(n,b,mono)
 if k not in FONTS:FONTS[k]=ImageFont.truetype(MONO if mono else BOLD if b else FONT,n)
 return FONTS[k]
def ease(k):k=max(0.0,min(1.0,k));return k*k*(3-2*k)
def prog(t,a,b):return ease((t-a)/max(1e-9,b-a))
def lin(t,a,b):return max(0.0,min(1.0,(t-a)/max(1e-9,b-a)))
def mix(a,b,k):return (a[0]+(b[0]-a[0])*k,a[1]+(b[1]-a[1])*k)
def mixbox(a,b,k):return tuple(x+(y-x)*k for x,y in zip(a,b))
def dim(c,a):a=max(0.0,min(1.0,a));return tuple(int(x*a+12*(1-a)) for x in c)
def plen(pts):return sum(math.dist(a,b) for a,b in zip(pts,pts[1:]))
def along(pts,k):
 lens=[math.dist(a,b) for a,b in zip(pts,pts[1:])];r=max(0,min(1,k))*sum(lens)
 for a,b,L in zip(pts,pts[1:],lens):
  if r<=L:return mix(a,b,r/max(1e-9,L))
  r-=L
 return pts[-1]
def path_upto(pts,k):
 if k<=0:return [pts[0],pts[0]]
 L=plen(pts);r=k*L;out=[pts[0]]
 for a,b in zip(pts,pts[1:]):
  d=math.dist(a,b)
  if r<=d:out.append(mix(a,b,r/max(1e-9,d)));return out
  out.append(b);r-=d
 return out
LOG=[]
ALLOWED_TEXT=re.compile(r'^[0-9,=×+ ]+$')
def text(d,xy,s,n=46,col=IV,b=False,mono=False,kind='label',center=False,right=False):
 assert '\u2014' not in s and '\u2013' not in s,('dash in on-screen text',s)
 f=font(n,b,mono);x,y=xy
 if center:x-=d.textlength(s,font=f)/2
 if right:x-=d.textlength(s,font=f)
 bb=d.textbbox((x,y),s,font=f);LOG.append((kind,bb,s));d.text((x,y),s,font=f,fill=col);return bb
def board():
 rng=np.random.default_rng(11);y=np.linspace(0,1,H)[:,None,None];x=np.linspace(0,1,W)[None,:,None]
 base=np.array([16,24,28])*(1-0.25*y)+np.array([6,10,12])*0.6;vign=1-0.35*((x-0.5)**2*2+(y-0.45)**2*1.2)
 img=base*vign+np.zeros((H,W,3))
 n=rng.standard_normal((H//4,W//4));n=np.asarray(Image.fromarray(((n*30)+128).clip(0,255).astype(np.uint8)).resize((W,H),Image.Resampling.BICUBIC)).astype(float)-128
 img+=n[...,None]*0.09;im=Image.fromarray(img.clip(0,255).astype(np.uint8));d=ImageDraw.Draw(im)
 for k in range(26):
  cx,cy=rng.uniform(0,W),rng.uniform(0,H);r=rng.uniform(60,220)
  d.arc((cx-r,cy-r*0.4,cx+r,cy+r*0.4),rng.uniform(0,360),rng.uniform(0,360)+rng.uniform(40,140),fill=(27,35,39),width=int(rng.uniform(6,18)))
 return im.filter(ImageFilter.GaussianBlur(2.2))
BOARD=board()
def chalk_line(d,pts,col,w=4,seed=0):
 """slightly doubled chalk stroke"""
 d.line(pts,fill=col,width=w,joint='curve')
 if len(pts)>1:d.line([(x+0.8,y-0.8) for x,y in pts],fill=dim(col,0.55),width=max(1,w//2))
def check(d,c,r,col=GR,w=6):x,y=c;d.line([(x-r,y),(x-r*0.3,y+r*0.7),(x+r,y-r*0.7)],fill=col,width=w,joint='curve')
def xmark(d,c,r,col=MG,w=6):x,y=c;d.line((x-r,y-r,x+r,y+r),fill=col,width=w);d.line((x-r,y+r,x+r,y-r),fill=col,width=w)
def orb(d,c,r,col):d.ellipse((c[0]-r,c[1]-r,c[0]+r,c[1]+r),fill=col,outline=IV,width=2)
def draw_db(d,c,rx,h,col=IV,alpha=1.0,fill=(16,30,40),log=True):
 x,y=c;ry=rx*0.3;top=y-h/2;bot=y+h/2;col=dim(col,alpha)
 d.rectangle((x-rx,top,x+rx,bot),fill=fill);d.ellipse((x-rx,bot-ry,x+rx,bot+ry),fill=fill)
 for k in range(10):
  xx=x-rx+7+k*(rx*0.075 if k<6 else rx*0.16);u=(xx-x)/rx
  if abs(u)<0.97:d.line((xx,top+ry*0.9*math.sqrt(1-u*u),xx,bot+ry*0.85*math.sqrt(1-u*u)),fill=dim((64,84,96),alpha),width=2)
 for yy in (top+h/3,top+2*h/3):d.arc((x-rx,yy-ry,x+rx,yy+ry),0,180,fill=col,width=3)
 d.line((x-rx,top,x-rx,bot),fill=col,width=4);d.line((x+rx,top,x+rx,bot),fill=col,width=4);d.arc((x-rx,bot-ry,x+rx,bot+ry),0,180,fill=col,width=4)
 d.ellipse((x-rx,top-ry,x+rx,top+ry),fill=(24,40,52),outline=col,width=4);d.arc((x-rx*0.7,top-ry*0.6,x+rx*0.7,top+ry*0.6),200,320,fill=dim((200,215,225),alpha),width=2)
 bb=(x-rx-2,top-ry-2,x+rx+2,bot+ry+2)
 if log:LOG.append(('node',bb,'db'))
 return bb
def db_outline(d,box,alpha,k=1.0):
 """big faint database cylinder that frames the tables (drawn progressively with k)"""
 x0,top,x1,bot=box;cx=(x0+x1)/2;rx=(x1-x0)/2;ry=34;col=dim((120,150,170),alpha)
 if k<=0:return
 d.arc((x0,top-ry,x1,top+ry),180,180+360*min(1,k*1.3),fill=col,width=4)
 L=bot-top;yy=top+L*min(1,k)
 d.line((x0,top,x0,yy),fill=col,width=4);d.line((x1,top,x1,yy),fill=col,width=4)
 if k>=1:
  d.arc((x0,bot-ry,x1,bot+ry),0,180,fill=col,width=4)
  for q in (1/3,2/3):d.arc((x0,top+L*q-ry,x1,top+L*q+ry),0,180,fill=dim((70,95,110),alpha),width=2)
 LOG.append(('dbframe',(x0-2,top-ry-2,x1+2,bot+ry+2),'db-outline'))
# ---------------------------------------------------------------- data objects: order card, line chip, event pill, row tile (same art everywhere)
def order_card(d,box,alpha=1.0,glow=0.0,mini=False):
 x0,y0,x1,y1=box;w=x1-x0;h=y1-y0;col=dim(AM,alpha)
 if glow>0:
  for g in range(3):e=4+g*5;d.rounded_rectangle((x0-e,y0-e,x1+e,y1+e),10+e,outline=dim(AM,glow*(0.5-0.14*g)),width=3)
 d.rounded_rectangle(box,max(3,min(w,h)*0.12),fill=(30,26,18),outline=col,width=2 if mini else 4)
 if mini:
  d.line((x0+w*0.18,y0+h*0.38,x1-w*0.18,y0+h*0.38),fill=dim(AM,alpha*0.7),width=2);d.line((x0+w*0.18,y0+h*0.68,x0+w*0.6,y0+h*0.68),fill=dim(AM,alpha*0.5),width=2)
 else:
  d.rectangle((x0+4,y0+4,x1-4,y0+h*0.24),fill=dim((90,72,40),alpha))
  for q,ln in ((0.42,0.8),(0.6,0.62),(0.78,0.7)):d.line((x0+w*0.1,y0+h*q,x0+w*(0.1+ln*0.8),y0+h*q),fill=dim(AM,alpha*0.65),width=3)
  d.ellipse((x1-w*0.16,y0+h*0.07,x1-w*0.06,y0+h*0.07+w*0.1),outline=dim(AM,alpha),width=2)
 LOG.append(('obj',box,'order'))
def chip(d,box,alpha=1.0,glow=0.0):  # an order line
 x0,y0,x1,y1=box;h=y1-y0;col=dim(CY,alpha)
 d.rounded_rectangle(box,6,fill=dim((14,40,52),max(alpha,0.3)),outline=dim(CY,min(1,alpha+glow*0.5)),width=3 if glow<0.3 else 4)
 d.rectangle((x0+6,y0+h*0.3,x0+6+h*0.4,y0+h*0.7),fill=col);d.line((x0+h*0.75+6,y0+h/2,x1-8,y0+h/2),fill=dim(CY,alpha*0.6),width=3)
 LOG.append(('obj',box,'line'))
def pill(d,box,alpha=1.0,glow=0.0):  # a status event
 x0,y0,x1,y1=box;h=y1-y0
 d.rounded_rectangle(box,h/2,fill=dim((44,26,52),max(alpha,0.3)),outline=dim(VI,min(1,alpha+glow*0.5)),width=3 if glow<0.3 else 4)
 c=((x0+x1)/2,(y0+y1)/2);r=h*0.2;d.ellipse((c[0]-r,c[1]-r,c[0]+r,c[1]+r),fill=dim(VI,alpha))
 LOG.append(('obj',box,'event'))
def tile(d,box,alpha=1.0,tint=None):  # one result row = order copy + line + event
 x0,y0,x1,y1=box;w=x1-x0
 d.rectangle(box,fill=(18,24,30),outline=dim(tint or (150,170,180),alpha),width=1)
 for k,c in enumerate((AM,CY,VI)):
  xa=x0+2+k*(w-4)/3;d.rectangle((xa,y0+2,xa+(w-4)/3-1,y1-2),fill=dim(c if tint is None else tint,alpha*(0.85 if tint is None else 0.6)))
PW,PH=1000,616
GLASS=[(231.,576.),(872.,577.),(873.,972.),(230.,971.)]
def homography(src,dst):
 a=[];b=[]
 for (x,y),(u,v) in zip(src,dst):a.extend([[x,y,1,0,0,0,-u*x,-u*y],[0,0,0,x,y,1,-v*x,-v*y]]);b.extend([u,v])
 return np.append(np.linalg.solve(np.array(a,float),np.array(b,float)),1).reshape(3,3)
def persp_coeffs(src,dst):Hm=homography(src,dst);Inv=np.linalg.inv(Hm);Inv/=Inv[2,2];return tuple(Inv.flatten()[:8])
GLASS_COEF=persp_coeffs([(0,0),(PW,0),(PW,PH),(0,PH)],GLASS);GLASS_H=homography([(0,0),(PW,0),(PW,PH),(0,PH)],GLASS)
GM=Image.new('L',(W,H));ImageDraw.Draw(GM).polygon(GLASS,fill=255)
def code_tokens(src):
 raw=list(lex(src,CSharpLexer(stripnl=False,stripall=False,ensurenl=False)));assert ''.join(v for k,v in raw)==src
 out=[]
 for i,(k,v) in enumerate(raw):
  if k in Token.Keyword or v in ('async','await','var','return','new','true','false'):c=VI
  elif k in Token.Literal.String:c=GR
  elif k in Token.Comment or k in Token.Literal.Number:c=AM
  elif k in Token.Name:
   later=[val for kk,val in raw[i+1:] if not val.isspace()];c=CY if later and later[0] in ['(','<'] else IV
  else:c=MUT if k in Token.Punctuation or k in Token.Operator else IV
  out.append((v,c))
 return out
def type_times(src,first,end):
 w=[];fixed=[];lead=True
 for k,ch in enumerate(src):
  if ch=='\n':w.append(0.30);fixed.append(0.0);lead=True;continue
  pause=0.06 if k and src[k-1]=='\n' else 0.0
  if ch==' ' and lead:w.append(0.12)
  else:lead=False;jit=1+0.18*math.sin(k*2.39996+0.7)*math.cos(k*0.913);w.append((0.8 if ch==' ' else 1.0)*jit)
  fixed.append(pause)
 w[0]=0.0;u=(end-first-sum(fixed))/sum(w);acc=first;T=[]
 for n in range(len(src)):acc+=w[n]*u+fixed[n];T.append(acc)
 assert all(b>=a for a,b in zip(T,T[1:])) and abs(T[-1]-end)<1e-6;return T
def overlap(a,b,pad=0):return not(a[2]+pad<=b[0] or b[2]+pad<=a[0] or a[3]+pad<=b[1] or b[3]+pad<=a[1])
LIMB_RE=re.compile(r'(^|[^a-z])(hand|hands|arm|arms|finger|fingers|forearm|lid-hand)([^a-z]|$)')
A=Path(__file__).parent/'assets';LIDFACE=Image.open(A/'lid-face.png').convert('RGB');PLATE=Image.open(A/'clean-plate.png').convert('RGB')
GM=Image.new('L',(W,H));ImageDraw.Draw(GM).polygon(GLASS,fill=255)
HINGE_Y=1023;LID_X0,LID_X1=204,897;CXL=(LID_X0+LID_X1)/2;HW0=(LID_X1-LID_X0)/2;TH_OPEN=math.acos(-0.17)
def lid_ease(k):k=max(0.0,min(1.0,k));return 1-(1-k**1.6)**2.6
def draw_lid_th(f,th):
 sy=HINGE_Y+217*math.cos(th)-454*math.sin(th);hw=HW0+105.5*max(math.cos(th),0);d=ImageDraw.Draw(f);edge=[(CXL-hw,sy),(CXL+hw,sy)]
 if sy>HINGE_Y+1.5:
  q=[(LID_X0,HINGE_Y),(LID_X1,HINGE_Y),edge[1],edge[0]];d.polygon(q,fill=(38,43,51))
  for k in range(1,9):yy=HINGE_Y+(sy-HINGE_Y)*k/9;hk=HW0+(hw-HW0)*k/9;d.line((CXL-hk+18,yy,CXL+hk-18,yy),fill=(52,58,68),width=1)
  d.line(q+[q[0]],fill=(128,142,158),width=3);mid=(CXL,(HINGE_Y+sy)/2);rr=max(2,(sy-HINGE_Y)*0.10);d.ellipse((mid[0]-rr*2.2,mid[1]-rr,mid[0]+rr*2.2,mid[1]+rr),outline=(92,104,118),width=2)
  thick=10*max(0.3,math.cos(th));d.polygon([edge[0],edge[1],(edge[1][0]-2,sy+thick),(edge[0][0]+2,sy+thick)],fill=(148,156,170),outline=(200,210,222))
 elif sy<HINGE_Y-1.5:
  q=[edge[0],edge[1],(LID_X1,HINGE_Y),(LID_X0,HINGE_Y)];coef=persp_coeffs([(0,0),(LIDFACE.width,0),(LIDFACE.width,LIDFACE.height),(0,LIDFACE.height)],q)
  face=LIDFACE.transform((W,H),Image.Transform.PERSPECTIVE,coef,Image.Resampling.BILINEAR);shade=0.55+0.45*min(1,(HINGE_Y-sy)/(HINGE_Y-539))
  m=Image.new('L',(W,H));ImageDraw.Draw(m).polygon(q,fill=255);face=Image.eval(face,lambda v:int(v*shade));f.paste(face,(0,0),m)
  if th<TH_OPEN-0.02:d.line(edge,fill=(170,180,194),width=3)
 else:d.line([(LID_X0,HINGE_Y),(LID_X1,HINGE_Y)],fill=(170,180,194),width=6)
 return sy
