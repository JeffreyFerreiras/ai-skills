"""Transactional-outbox final-v1 (N01-N05): deterministic Pillow/NumPy chalk renderer, cartesian-explosion style.
One geometry + state model feeds drawing AND per-frame checks. No scene IDs or track labels are drawn (asserted per frame).
Scenes (internal only): hook (customer: order saved, email never came), laptop (physical laptop opens by itself, quiet logs),
code (bad.cs types in; save -> database, publish -> broker -> email + inventory; two writes; nothing ties them),
crash (commit, then crash / broker down: event lost), rollback (publish first, then rollback: ghost order), dual (two writes split)."""
import json,math,os,subprocess,sys
from pathlib import Path
import numpy as np
from PIL import Image,ImageDraw,ImageFilter
import chalkkit as K
from chalkkit import W,H,FPS,CY,IV,AM,GR,VI,MUT,MG,DARK,ease,prog,lin,mix,dim,path_upto,along,plen,text,font,chalk_line,check,xmark,draw_db,order_card,code_tokens,type_times,overlap,LIMB_RE
import timeline as TL
P=Path(__file__).parent;ROOT=TL.ROOT
COUNT=TL.COUNT;TOTAL=TL.TOTAL;WT=TL.wt;WE=TL.wend
def ss(s):return next(a for x,a,b in TL.SCENES if x==s)
def se(s):return next(b for x,a,b in TL.SCENES if x==s)
def scene_of(t):return next((s for s,a,b in TL.SCENES if a<=t<b),TL.SCENES[-1][0])
BOARD=K.board();RD=(255,96,96)
def person(path,w):im=Image.open(path).convert('RGBA');return im.resize((w,round(im.height*w/im.width)),Image.Resampling.LANCZOS)
CUST=person(K.A/'customer.png',470);CUST_XY=(30,330)
DEV=person(K.A/'jeffrit-bust-video.png',340);DEV_XY=(12,560)   # v2: Jeffrey's hand-drawn chalk gigachad bust (brand/jeffrit) replaces the generic developer (slightly larger for phone readability)
def paste_person(f,im,xy,a,name):
 if a<=0:return
 if a<1:im=im.copy();im.putalpha(Image.fromarray((np.asarray(im.getchannel('A')).astype(float)*a).astype(np.uint8)))
 f.paste(im,xy,im);K.LOG.append(('person',(xy[0],xy[1],xy[0]+im.width,xy[1]+im.height),name))
# ---------------------------------------------------------------- shared objects
def envelope(d,c,s=1.0,alpha=1.0,col=VI,dashed=False,log=True):
 w,h=64*s,44*s;x0,y0,x1,y1=c[0]-w/2,c[1]-h/2,c[0]+w/2,c[1]+h/2
 if dashed:
  for (a,b) in [((x0,y0),(x1,y0)),((x1,y0),(x1,y1)),((x1,y1),(x0,y1)),((x0,y1),(x0,y0))]:
   L=math.dist(a,b);n=int(L//14)
   for k in range(0,n,2):d.line((mix(a,b,k/n),mix(a,b,min(1,(k+1)/n))),fill=dim(col,alpha),width=3)
 else:
  d.rectangle((x0,y0,x1,y1),fill=dim((40,24,52),max(alpha,0.25)),outline=dim(col,alpha),width=3)
  d.line((x0+3,y0+3,c[0],c[1]+h*0.12,x1-3,y0+3),fill=dim(col,alpha),width=3)
 if log:K.LOG.append(('obj',(x0,y0,x1,y1),'envelope'))
def arrow_head(d,p,q,col,sz=18):
 a=math.atan2(q[1]-p[1],q[0]-p[0]);d.polygon([q,(q[0]-sz*math.cos(a-0.45),q[1]-sz*math.sin(a-0.45)),(q[0]-sz*math.cos(a+0.45),q[1]-sz*math.sin(a+0.45))],fill=col)
def arrow(d,pts,k,col,w=5,dashed=False):
 if k<=0:return
 pp=path_upto(pts,k)
 if dashed:
  L=plen(pp);n=max(1,int(L//18))
  for i in range(0,n,2):d.line([along(pp,i/n),along(pp,min(1,(i+1)/n))],fill=col,width=w)
 else:chalk_line(d,pp,col,w)
 if k>=1:arrow_head(d,pts[-2],pts[-1],col)
 xs=[x for x,y in pts];ys=[y for x,y in pts];K.LOG.append(('route',(min(xs)-2,min(ys)-2,max(xs)+2,max(ys)+2),'arrow'))
def dust(d,c,k,col=IV,n=18,seed=3):
 r=np.random.default_rng(seed)
 for i in range(n):
  a=r.uniform(0,6.283);sp=r.uniform(20,70);x=c[0]+math.cos(a)*sp*k;y=c[1]+math.sin(a)*sp*k+25*k*k;rr=r.uniform(2,4)*(1-k)
  if rr>0.3:d.ellipse((x-rr,y-rr,x+rr,y+rr),fill=dim(col,1-k))
# ---------------------------------------------------------------- hook: the customer, an order saved, the email that never arrives
CHECKOUT=(560,360,1030,820);ORD_BIG=(615,470,845,610);INBOX=(560,900,1030,1240);SLOT=(735,1015,855,1097)
T_ORDER=WT('N01','order');T_SAVED=WT('N01','saved.');T_EMAIL=WT('N01','email');T_NEVER=WT('N01','never')
ENV_PATH=[(795,640),(795,780),(795,930),(795,1055)]
def hook_state(t):
 st={'cust':prog(t,0.0,0.35),'panel':prog(t,0.05,0.35),'order':prog(t,T_ORDER-0.05,T_ORDER+0.2),'check':prog(t,T_SAVED,T_SAVED+0.25),
  'inbox':prog(t,T_EMAIL-0.45,T_EMAIL-0.15),'env':lin(t,T_EMAIL+0.05,T_NEVER+0.55)}
 st['env_alpha']=0.0 if st['env']<=0 else max(0.0,1-st['env']/0.55)   # fades out before reaching the tray: it never arrives
 st['arrived']=False;st['slot_pulse']=prog(t,T_NEVER+0.15,T_NEVER+0.45)
 return st
def hook(t):
 f=BOARD.copy();st=hook_state(t);paste_person(f,CUST,CUST_XY,st['cust'],'customer front-on');d=ImageDraw.Draw(f);k=st['panel']
 if k>0:
  d.rounded_rectangle(CHECKOUT,20,fill=(13,22,31),outline=dim((120,150,170),k),width=4);d.rectangle((CHECKOUT[0]+4,CHECKOUT[1]+4,CHECKOUT[2]-4,CHECKOUT[1]+40),fill=dim((26,38,50),k))
  for j,c in enumerate([(255,110,110),(255,200,90),(110,220,140)]):d.ellipse((CHECKOUT[0]+18+j*24,CHECKOUT[1]+15,CHECKOUT[0]+30+j*24,CHECKOUT[1]+27),fill=dim(c,k))
  K.LOG.append(('panel',CHECKOUT,'checkout'))
 if st['order']>0:
  s=0.6+0.4*st['order'];cx,cy=(ORD_BIG[0]+ORD_BIG[2])/2,(ORD_BIG[1]+ORD_BIG[3])/2;w,h=(ORD_BIG[2]-ORD_BIG[0])*s,(ORD_BIG[3]-ORD_BIG[1])*s
  order_card(d,(cx-w/2,cy-h/2,cx+w/2,cy+h/2),st['order'])
 if st['check']>0:c=(940,540);r=44*st['check'];d.ellipse((c[0]-r,c[1]-r,c[0]+r,c[1]+r),fill=(14,40,28),outline=dim(GR,st['check']),width=5);check(d,(c[0]+2,c[1]+4),26*st['check'],dim(GR,st['check']),7)
 if st['inbox']>0:
  a=st['inbox'];d.rounded_rectangle(INBOX,20,fill=(13,22,31),outline=dim((120,150,170),a),width=4);K.LOG.append(('panel',INBOX,'inbox'))
  for y in (950,1150,1190):d.line((INBOX[0]+40,y,INBOX[2]-40,y),fill=dim((60,74,86),a),width=3)
  col=MG if st['slot_pulse']>0 else (90,104,116);envelope(d,((SLOT[0]+SLOT[2])/2,(SLOT[1]+SLOT[3])/2),1.6,a*(0.55+0.45*st['slot_pulse']),col,dashed=True,log=False)
 if 0<st['env']<1 and st['env_alpha']>0:envelope(d,along(ENV_PATH,st['env']*0.62),2.0,st['env_alpha'],VI)
 return f,st
# ---------------------------------------------------------------- laptop: the physical laptop opens by itself (no hand), the logs scroll quietly
PW,PH=1000,616;OPEN_A,OPEN_B=ss('laptop')+0.06,ss('laptop')+0.85;GLOW=(OPEN_B-0.05,OPEN_B+0.30);T_LOGS=WT('N01','logs')
def lid_th(t):return K.TH_OPEN*K.lid_ease((t-OPEN_A)/(OPEN_B-OPEN_A))
def log_panel(t):
 im=Image.new('RGB',(PW,PH),(10,16,22));d=ImageDraw.Draw(im);d.rectangle((0,0,PW,46),fill=(22,32,44))
 for k,c in enumerate([(255,110,110),(255,200,90),(110,220,140)]):d.ellipse((20+k*28,15,36+k*28,31),fill=c)
 n=int(max(0,(t-GLOW[0]-0.1))/0.16);rows=[]
 r=np.random.default_rng(5);spec=[(r.integers(0,3),r.uniform(0.3,0.85)) for _ in range(60)]
 for i in range(n):rows.append(spec[i%60])
 rows=rows[-8:]
 for j,(kind,ln) in enumerate(rows):
  y=74+j*66;col=[(130,150,165),(90,190,220),(120,175,145)][kind];d.rectangle((40,y,40+110,y+38),fill=dim(col,0.95));d.rectangle((175,y,175+(PW-240)*ln,y+38),fill=dim(col,0.7))
 return im,len(rows)
def laptop(t):
 f=K.PLATE.copy();K.draw_lid_th(f,lid_th(t));g=prog(t,*GLOW);st={'lid':lid_th(t),'glow':g,'logs':0,'warn':0}
 if g>0:
  pim,n=log_panel(t);st['logs']=n;w=pim.transform((W,H),Image.Transform.PERSPECTIVE,K.GLASS_COEF,Image.Resampling.BICUBIC);f.paste(w,(0,0),Image.eval(K.GM,lambda v:int(v*g)))
 return f,st
# ---------------------------------------------------------------- flow layout (code, crash, rollback, dual)
APP=(60,1045,320,1210);DB=(470,1392);DBRX,DBH=92,140;BROKER=(500,700,800,880);EMAIL=(895,1075);INV=(895,1372);NR=72
A_DB=[(300,1215),(395,1318)];A_BR=[(322,1046),(497,812)];B_EM=[(803,800),(895,800),(895,998)];B_IN=[(700,883),(700,1372),(818,1372)]
DB_SLOT=(DB[0],DB[1]-12);SHELF_BOX=(INV[0]-26,INV[1]-30,INV[0]+26,INV[1]+10)
def app_icon(d,crash=0.0,dead=0.0,alpha=1.0,pending=0.0):
 x0,y0,x1,y1=APP;sc=(x0+20,y0+8,x1-20,y1-52);base=[(x0,y1-40),(x1,y1-40),(x1+14,y1),(x0-14,y1)]
 d.polygon(base,fill=(30,38,46),outline=dim((150,165,180),alpha));scol=(14,40,52) if dead<=0 else (8,10,12)
 d.rectangle(sc,fill=scol,outline=dim((150,165,180),alpha),width=4)
 if dead<=0:
  for i in range(4):d.line((sc[0]+16,sc[1]+22+i*22,sc[0]+16+(sc[2]-sc[0]-32)*(0.8-0.15*i),sc[1]+22+i*22),fill=dim(CY,alpha*0.55),width=4)
 if 0<crash<1:d.rectangle(sc,fill=dim((240,245,250),1-crash))
 if dead>0:
  pts=[(sc[0]+30,sc[1]+10),(sc[0]+80,sc[1]+60),(sc[0]+60,sc[1]+80),(sc[0]+130,sc[1]+120)];d.line(path_upto(pts,dead),fill=dim(RD,alpha),width=4)
 K.LOG.append(('node',(x0-14,y0,x1+14,y1),'app laptop'))
def broker(d,alpha=1.0,down=0.0,fill=0):
 x0,y0,x1,y1=BROKER;d.rounded_rectangle(BROKER,16,fill=(26,20,36),outline=dim(VI,alpha*(1-0.5*down)),width=5)
 d.polygon([(x0-12,y0+6),((x0+x1)/2,y0-46),(x1+12,y0+6)],outline=dim(VI,alpha*(1-0.5*down)),fill=(26,20,36))
 d.rectangle(((x0+x1)/2-60,y0+26,(x0+x1)/2+60,y0+40),fill=(8,10,14))   # mail slot
 for i in range(4):
  xx=x0+40+i*60;d.rectangle((xx,y0+70,xx+40,y1-20),outline=dim((110,96,140),alpha),width=3)
  if i<fill:envelope(d,(xx+20,y1-60),0.5,alpha,VI,log=False)
 if down>0:xmark(d,((x0+x1)/2,(y0+y1)/2),int(60*down),dim(RD,alpha),9)
 K.LOG.append(('node',(x0-12,y0-46,x1+12,y1),'broker'))
def node(d,c,alpha,lit,kind,box_lift=0.0,grey=0.0):
 col=GR if lit>0 else (120,150,170);col=dim(col,alpha*(1-0.55*grey))
 if lit>0:
  for g in range(3):e=6+g*7;d.ellipse((c[0]-NR-e,c[1]-NR-e,c[0]+NR+e,c[1]+NR+e),outline=dim(GR,lit*(0.45-0.13*g)),width=3)
 d.ellipse((c[0]-NR,c[1]-NR,c[0]+NR,c[1]+NR),fill=(14,26,34),outline=col,width=5)
 if kind=='email':
  envelope(d,c,1.25,alpha*(1-0.5*grey),col,log=False)
 else:
  d.line((c[0]-46,c[1]+12,c[0]+46,c[1]+12),fill=col,width=5)
  y=-46*box_lift;d.rectangle((SHELF_BOX[0],SHELF_BOX[1]+y,SHELF_BOX[2],SHELF_BOX[3]+y),fill=(48,40,26),outline=dim(AM,alpha*(1-0.5*grey)),width=3)
  d.line((SHELF_BOX[0],SHELF_BOX[1]+14+y,SHELF_BOX[2],SHELF_BOX[1]+14+y),fill=dim(AM,alpha*0.6),width=2)
 if lit>=1:check(d,(c[0]+NR-8,c[1]-NR+14),18,GR,6)
 K.LOG.append(('node',(c[0]-NR,c[1]-NR,c[0]+NR,c[1]+NR),kind))
def mini_order(d,c,alpha=1.0):order_card(d,(c[0]-40,c[1]-28,c[0]+40,c[1]+28),alpha)
def ghost_order(d,c,alpha,col=MG):
 x0,y0,x1,y1=c[0]-40,c[1]-28,c[0]+40,c[1]+28
 for (a,b) in [((x0,y0),(x1,y0)),((x1,y0),(x1,y1)),((x1,y1),(x0,y1)),((x0,y1),(x0,y0))]:
  L=math.dist(a,b);n=int(L//12)
  for k in range(0,n,2):d.line((mix(a,b,k/n),mix(a,b,min(1,(k+1)/n))),fill=dim(col,alpha),width=5)
 K.LOG.append(('obj',(x0,y0,x1,y1),'ghost order'))
# ---------------------------------------------------------------- code card: bad.cs types in; rows can move (publish-first swap) and glow when they execute
SNIP=(ROOT/'planning/script-v1/code/bad.cs').read_text().rstrip('\n');ROWS=SNIP.split('\n')
CODE_N=44;CODE_LH=62;CODE_X=72;CARD_TOP=250;CODE_Y=CARD_TOP+26;ADV=font(CODE_N,mono=True).getlength('M');CARET_HZ=2.0
assert max(len(r) for r in ROWS)<=34
TYPE=(ss('code')+0.42,WT('N02','publish')-0.25)
CODE_T=type_times(SNIP,*TYPE);CODE_TOK=code_tokens(SNIP)
ROW_TOKS=[];_cur=[[]]
for v,c in CODE_TOK:
 parts=v.split('\n')
 for i,pp in enumerate(parts):
  if i>0:_cur.append([])
  if pp:_cur[-1].append((pp,c))
ROW_TOKS=_cur;assert len(ROW_TOKS)==len(ROWS)
ROW_START=[sum(len(r)+1 for r in ROWS[:i]) for i in range(len(ROWS))]
CARD_BOTTOM=CODE_Y+len(ROWS)*CODE_LH+14
def row_typed(i):return CODE_T[ROW_START[i]+len(ROWS[i])-1]
def code_card(d,t,slots,glows,alpha=1.0,ra=(1,1,1,1)):
 """slots[i]: row i's vertical slot (float); glows[i]: (strength,color)"""
 d.rounded_rectangle((CODE_X-30,CARD_TOP,W-CODE_X+30,CARD_BOTTOM),22,fill=(11,22,31),outline=dim((80,130,150),alpha),width=3);K.LOG.append(('card',(CODE_X-30,CARD_TOP,W-CODE_X+30,CARD_BOTTOM),'bad'))
 vis=''
 for i,toks in enumerate(ROW_TOKS):
  y=CODE_Y+slots[i]*CODE_LH;g,gc=glows[i]
  if ra[i]<0.03:continue
  g*=ra[i]
  if g>0:d.rounded_rectangle((CODE_X-12,y-4,CODE_X+ADV*len(ROWS[i])+12,y+56),8,fill=tuple(int(v*0.16*g+11*(1-0.16*g)) for v in gc),outline=dim(gc,g*0.8),width=2)
  x=CODE_X;pos=ROW_START[i]
  for v,c in toks:
   n=sum(1 for k in range(len(v)) if CODE_T[pos+k]<=t);
   if n:text(d,(x,y),v[:n],CODE_N,dim(c,alpha*ra[i]),mono=True,kind='code');vis+=v[:n]
   x+=ADV*len(v);pos+=len(v)
 nxt=next((k for k in range(len(SNIP)) if CODE_T[k]>t),None)
 if nxt is not None and t>=TYPE[0]-0.3 and ((t*CARET_HZ)%1.0)<0.6:
  pre=SNIP[:nxt];r=pre.count('\n');cc=len(pre)-(pre.rfind('\n')+1);x=CODE_X+ADV*cc;y=CODE_Y+slots[r]*CODE_LH;d.rectangle((x,y+4,x+3,y+52),fill=IV)
 return vis.replace(' ','')
# ---------------------------------------------------------------- timings (all tied to spoken words; every arrival after its send)
TRAV=0.55
# code scene
T_NODES=(ss('code')+0.15,ss('code')+0.55)
T_SAVE=(max(row_typed(1)+0.15,WT('N02','Entity')-0.2),);T_SAVE=(T_SAVE[0],T_SAVE[0]+TRAV);T_DBOK=T_SAVE[1]+0.05
T_PUB=(max(row_typed(3)+0.15,WT('N02','publish')+0.05),WT('N02','broker')-0.25);T_BROK=T_PUB[1]
T_EM=(max(T_BROK+0.15,WT('N02','email')-0.2),);T_EM=(T_EM[0],T_EM[0]+0.5);T_IN=(max(T_EM[0]+0.3,WT('N02','inventory')-0.2),);T_IN=(T_IN[0],T_IN[0]+0.6)
T_TWO=WT('N03','Two');T_ONE1=WT('N03','One');T_ONE2=WT('N03','one',1);T_NOTH=WT('N03','Nothing');T_TIES=WT('N03','ties');T_SNAP=WE('N03','together.')-0.15
# crash scene
C0=ss('crash');C_SAVE=(C0+0.25,C0+0.25+TRAV);C_DBOK=C_SAVE[1]+0.05;C_PEND=C_DBOK+0.05
C_CRASH=max(WT('N04','crashes')+0.05,C_DBOK+0.2);C_DOWN=WT('N04','broker');C_COMMIT=WT('N04','committed,');C_GONE=WT('N04','gone.')-0.1
C_EMAIL=WT('N04','Email');C_INV=WT('N04','Inventory')
# rollback scene
R0=ss('rollback');R_SWAP=(WT('N04','Publish'),WT('N04','Publish')+0.5)
R_PUB=(R_SWAP[1]+0.12,R_SWAP[1]+0.12+0.45);R_BROK=R_PUB[1];R_EM=(R_BROK+0.08,R_BROK+0.5);R_IN=(R_BROK+0.18,R_BROK+0.68)
R_SAVE=(max(R_IN[1]+0.05,WT('N04','rollback')),);R_SAVE=(R_SAVE[0],R_SAVE[0]+0.45);R_RB=(R_SAVE[1]+0.05,R_SAVE[1]+0.65)
R_GHOST=(max(R_RB[1]+0.05,WT('N04','reacting')),);R_GHOST=(R_GHOST[0],R_GHOST[0]+0.7);R_EXIST=WT('N04','doesn\'t')
# dual scene
D0=ss('dual');D_UNSWAP=(D0+0.05,D0+0.45);D_ARROWS=(D0+0.1,D0+0.5);D_DUAL=WT('N05','dual');D_SPLIT=(WT('N05','problem.'),WT('N05','problem.')+0.45)
STR_A=(DB[0]+30,DB[1]-DBH/2-30);STR_B=(BROKER[0]+50,BROKER[3]+4)   # a tie between the two destinations
def flow_state(t,s):
 st={'scene':s,'nodes':prog(t,*T_NODES) if s=='code' else 1.0,'order_pos':None,'order_at_db':0.0,'db_ok':0.0,'env_pos':None,'env_at_app':0.0,'broker_fill':0,'broker_down':0.0,
  'email':0.0,'inv':0.0,'box_lift':0.0,'arr_db':0.0,'arr_br':0.0,'pulse_db':0.0,'pulse_br':0.0,'string':0.0,'snap':0.0,'crash':0.0,'dead':0.0,'gone':0.0,'em_grey':0.0,'in_grey':0.0,
  'stall_em':0.0,'stall_in':0.0,'rollback':0.0,'ghost':0.0,'ghost_pulse':0.0,'swap':0.0,'em_env':None,'in_env':None,'order_sent':False,'env_sent':False,'broker_got':False,'split':0.0}
 if s=='code':
  st['arr_db']=1.0 if t>=T_SAVE[0] else 0.0;st['arr_br']=1.0 if t>=T_PUB[0] else 0.0
  if T_SAVE[0]<=t<T_SAVE[1]:st['order_pos']=lin(t,*T_SAVE)
  st['order_sent']=t>=T_SAVE[0];st['order_at_db']=1.0 if t>=T_SAVE[1] else 0.0;st['db_ok']=prog(t,T_DBOK,T_DBOK+0.25)
  if T_PUB[0]<=t<T_PUB[1]:st['env_pos']=lin(t,*T_PUB)
  st['env_sent']=t>=T_PUB[0];st['broker_got']=t>=T_BROK;st['broker_fill']=1 if t>=T_BROK else 0
  if T_EM[0]<=t<T_EM[1]:st['em_env']=lin(t,*T_EM)
  if T_IN[0]<=t<T_IN[1]:st['in_env']=lin(t,*T_IN)
  st['email']=prog(t,T_EM[1],T_EM[1]+0.25);st['inv']=prog(t,T_IN[1],T_IN[1]+0.25);st['box_lift']=prog(t,T_IN[1],T_IN[1]+0.4)
  if t>=T_EM[1]:st['broker_fill']=0
  pul=lambda a:math.sin(math.pi*lin(t,a,a+0.7))
  st['pulse_db']=max(pul(T_TWO),pul(T_ONE1));st['pulse_br']=max(pul(T_TWO),pul(T_ONE2))
  st['string']=prog(t,T_NOTH,T_TIES+0.25);st['snap']=prog(t,T_SNAP,T_SNAP+0.5)
 elif s=='crash':
  st['arr_db']=1.0;st['arr_br']=1.0
  if C_SAVE[0]<=t<C_SAVE[1]:st['order_pos']=lin(t,*C_SAVE)
  st['order_sent']=t>=C_SAVE[0];st['order_at_db']=1.0 if t>=C_SAVE[1] else 0.0;st['db_ok']=prog(t,C_DBOK,C_DBOK+0.25)
  st['env_at_app']=prog(t,C_PEND,C_PEND+0.25)*(1-prog(t,C_GONE,C_GONE+0.6));st['gone']=lin(t,C_GONE,C_GONE+0.6)
  st['crash']=lin(t,C_CRASH,C_CRASH+0.18);st['dead']=prog(t,C_CRASH+0.12,C_CRASH+0.45);st['broker_down']=prog(t,C_DOWN,C_DOWN+0.35)
  st['db_glow']=math.sin(math.pi*lin(t,C_COMMIT,C_COMMIT+0.8))
  st['stall_em']=prog(t,C_EMAIL,C_EMAIL+0.5);st['em_grey']=prog(t,C_EMAIL+0.2,C_EMAIL+0.6);st['stall_in']=prog(t,C_INV,C_INV+0.5);st['in_grey']=prog(t,C_INV+0.2,C_INV+0.6)
 elif s=='rollback':
  st['arr_db']=1.0;st['arr_br']=1.0;st['swap']=prog(t,*R_SWAP)
  if R_PUB[0]<=t<R_PUB[1]:st['env_pos']=lin(t,*R_PUB)
  st['env_sent']=t>=R_PUB[0];st['broker_got']=t>=R_BROK
  if R_EM[0]<=t<R_EM[1]:st['em_env']=lin(t,*R_EM)
  if R_IN[0]<=t<R_IN[1]:st['in_env']=lin(t,*R_IN)
  st['email']=prog(t,R_EM[1],R_EM[1]+0.25);st['inv']=prog(t,R_IN[1],R_IN[1]+0.25);st['box_lift']=prog(t,R_IN[1],R_IN[1]+0.4)
  if R_SAVE[0]<=t<R_SAVE[1]:st['order_pos']=lin(t,*R_SAVE)
  st['order_sent']=t>=R_SAVE[0];st['order_at_db']=(1.0 if t>=R_SAVE[1] else 0.0);st['rollback']=lin(t,*R_RB)
  st['ghost']=prog(t,*R_GHOST);st['ghost_pulse']=math.sin(math.pi*lin(t,R_EXIST,R_EXIST+0.7))
 elif s=='dual':
  st['swap']=1-prog(t,*D_UNSWAP);st['arr_db']=prog(t,*D_ARROWS);st['arr_br']=prog(t,*D_ARROWS)
  st['pulse_db']=st['pulse_br']=math.sin(math.pi*lin(t,D_DUAL,D_DUAL+0.8));st['string']=prog(t,D_DUAL-0.1,D_DUAL+0.3);st['snap']=prog(t,*D_SPLIT)
 return st
def card_layout(t,s,st):
 k=st['swap'];m=ease(prog(k,0.3,0.7));slots=[0,1 if k<0.5 else 3,2-m,3-m]   # publish-first: SaveChanges fades out, the publish rows move up, SaveChanges fades back in at the bottom
 st['row_alpha']=[1,1-prog(k,0.0,0.3) if k<0.5 else prog(k,0.7,1.0),1,1]
 G=[(0.0,CY)]*4;glows=list(G)
 if s=='code':
  g1=max(math.sin(math.pi*lin(t,T_SAVE[0]-0.05,T_DBOK+0.3)),st['pulse_db']);g2=max(math.sin(math.pi*lin(t,T_PUB[0]-0.05,T_BROK+0.3)),st['pulse_br'])
  glows=[(g1,CY),(g1,CY),(g2,VI),(g2,VI)]
 elif s=='crash':g1=math.sin(math.pi*lin(t,C_SAVE[0]-0.05,C_DBOK+0.3));glows=[(g1,CY),(g1,CY),(0.0,VI),(0.0,VI)]
 elif s=='rollback':
  g2=math.sin(math.pi*lin(t,R_PUB[0]-0.05,R_BROK+0.3));g1=math.sin(math.pi*lin(t,R_SAVE[0]-0.05,R_RB[1]))
  glows=[(g1,CY),(g1,MG if st['rollback']>0 else CY),(g2,VI),(g2,VI)]
 elif s=='dual':glows=[(st['pulse_db'],CY),(st['pulse_db'],CY),(st['pulse_br'],VI),(st['pulse_br'],VI)]
 return slots,glows
def flow_draw(d,f,t,st):
 a=st['nodes']
 if a<=0:return
 app_icon(d,st['crash'],st['dead'],a)
 rb=st['rollback'];draw_db(d,DB,DBRX,DBH,MG if 0<rb<1 else IV,alpha=a)
 broker(d,a,st['broker_down'],st['broker_fill'])
 node(d,EMAIL,a,st['email'],'email',grey=st['em_grey']);node(d,INV,a,st['inv'],'inventory',st['box_lift'],grey=st['in_grey'])
 for pts,k,pul,col in ((A_DB,st['arr_db'],st['pulse_db'],CY),(A_BR,st['arr_br'],st['pulse_br'],VI)):
  if k>0:arrow(d,pts,k,dim(col,a*(0.7+0.3*pul)),5+int(4*pul))
 # fan-out routes (faint until used)
 arrow(d,B_EM,1.0 if st['env_sent'] or st['scene']!='code' else a,dim((90,80,110),a*0.8),3);arrow(d,B_IN,1.0,dim((90,80,110),a*0.8),3)
 if st['string']>0:
  sn=st['snap']
  if sn<=0:
   chalk_line(d,path_upto([STR_A,mix(STR_A,STR_B,0.5),STR_B],st['string']),dim(AM,0.95),6)
   if st['string']>=1:m=mix(STR_A,STR_B,0.5);d.ellipse((m[0]-11,m[1]-11,m[0]+11,m[1]+11),outline=AM,width=5)
  else:
   m=mix(STR_A,STR_B,0.5);dr=40*sn;al=1-sn
   chalk_line(d,[STR_A,(m[0]-30*sn,m[1]+30+dr)],dim(AM,0.95*al),6);chalk_line(d,[STR_B,(m[0]+30*sn,m[1]-30+dr*0.3)],dim(AM,0.95*al),6)
 if st['db_ok']>0 and st['order_at_db']>0:
  mini_order(d,DB_SLOT,1.0);c=(DB[0]+DBRX-8,DB[1]-DBH/2-36);check(d,c,20*st['db_ok'],dim(GR,st['db_ok']),6)
  if st.get('db_glow',0)>0:d.ellipse((DB[0]-DBRX-14,DB[1]-DBH/2-40,DB[0]+DBRX+14,DB[1]+DBH/2+40),outline=dim(GR,st['db_glow']),width=4)
 if st['order_pos'] is not None:mini_order(d,along(A_DB,st['order_pos']))
 if st['scene']=='rollback' and st['order_at_db']>0 and rb<1:
  k=rb;c=along(A_DB[::-1],k*0.6) if k>0 else DB_SLOT;mini_order(d,c,1-k)
  if k>0:xmark(d,(DB[0],DB[1]+20),int(46*min(1,k*2)),dim(MG,1-0.5*k),8)
 if st['env_pos'] is not None:envelope(d,along(A_BR,st['env_pos']),0.9)
 if st['env_at_app']>0:envelope(d,(APP[2]+26,APP[1]-10),0.9,st['env_at_app'])
 if 0<st['gone']<1:dust(d,(APP[2]+26,APP[1]-10),st['gone'],VI)
 if st['em_env'] is not None:envelope(d,along(B_EM,st['em_env']),0.8)
 if st['in_env'] is not None:envelope(d,along(B_IN,st['in_env']),0.8)
 if st['stall_em']>0:arrow(d,B_EM,st['stall_em'],dim(MUT,0.7),4,dashed=True)
 if st['stall_in']>0:arrow(d,B_IN,st['stall_in'],dim(MUT,0.7),4,dashed=True)
 if st['ghost']>0:
  ghost_order(d,(DB_SLOT[0],DB_SLOT[1]-4),min(1,st['ghost']*1.6)*(0.75+0.25*st['ghost_pulse']));gp=st['ghost_pulse']
  if gp>0:d.ellipse((DB_SLOT[0]-60-14*gp,DB_SLOT[1]-50-10*gp,DB_SLOT[0]+60+14*gp,DB_SLOT[1]+42+10*gp),outline=dim(MG,gp),width=4)
  for src in ((EMAIL[0]-NR,EMAIL[1]+30),(INV[0]-NR,INV[1])):arrow(d,[src,(DB_SLOT[0]+48,DB_SLOT[1])],st['ghost'],dim(MG,0.75),4,dashed=True)
def flow_scene(t,s):
 st=flow_state(t,s);base=BOARD.copy();paste_person(base,DEV,DEV_XY,prog(t,ss('code'),ss('code')+0.35),'jeffrit gigachad bust front-on');d=ImageDraw.Draw(base)
 slots,glows=card_layout(t,s,st);st['visible']=code_card(d,t,slots,glows,ra=st['row_alpha'])
 full=base.copy();d2=ImageDraw.Draw(full);flow_draw(d2,full,t,st)
 a,b=ss(s),se(s);k=1.0
 if s!='code':k=min(k,lin(t,a,a+0.22))
 if s not in ('code','dual') or s=='code':
  nxt=next((x for x,aa,bb in TL.SCENES if abs(aa-b)<1e-9),None)
  if nxt in ('crash','rollback','dual'):k=min(k,1-lin(t,b-0.16,b))
 if s in ('crash','rollback','dual'):
  nxt=next((x for x,aa,bb in TL.SCENES if abs(aa-b)<1e-9),None);k=min(k,1-lin(t,b-0.16,b))
 st['flow_k']=k
 return (Image.blend(base,full,k) if k<1 else full),st
# ---------------------------------------------------------------- captions (spoken words only)
CAPS=TL.captions();CAP_Y=1590;CAP_N=55
def draw_captions(f,t):
 d=ImageDraw.Draw(f);c=next((c for c in CAPS if c['start']<=t<c['end']),None)
 if not c:return None
 words=c['text'].split();lines=[];cur=[]
 for w in words:
  if d.textlength(' '.join(cur+[w]),font=font(CAP_N,True))>890 and cur:lines.append(cur);cur=[w]
  else:cur.append(w)
 if cur:lines.append(cur)
 assert len(lines)<=2,('caption too tall',c['text'])
 active=next((i for i,a in enumerate(c['words']) if a['start']<=t<a['end']),None);y=CAP_Y;off=0
 for L in lines:
  txt=' '.join(L);ww=d.textlength(txt,font=font(CAP_N,True));x=(W-ww)/2
  d.rounded_rectangle((x-16,y-8,x+ww+16,y+72),12,fill=(5,12,18));text(d,(x,y),txt,CAP_N,IV,True,kind='caption')
  if active is not None and off<=active<off+len(L):
   j=active-off;xx=x+d.textlength(' '.join(L[:j])+(' ' if j else ''),font=font(CAP_N,True));text(d,(xx,y),L[j],CAP_N,CY,True,kind='caption-hi')
  off+=len(L);y+=84
 return c
# ---------------------------------------------------------------- frame + checks
SCN={'hook':hook,'laptop':laptop}
FADE=0.12
def frame(t,check_=True):
 K.LOG.clear();s=scene_of(t)
 f,st=(SCN[s](t) if s in SCN else flow_scene(t,s))
 a,b=ss(s),se(s);k=1.0
 if s in ('hook','laptop','code'):
  if s!='hook' and t<a+FADE:k=lin(t,a,a+FADE)
  if s in ('hook','laptop') and t>b-FADE:k=min(k,1-lin(t,b-FADE,b))
 if k<1:f=Image.blend(BOARD,f,k)
 draw_captions(f,t)
 if check_:check_frame(t,s,st,list(K.LOG))
 return f
def check_frame(t,s,st,items):
 for kd,(x0,y0,x1,y1),n in items:
  if kd=='person':assert not LIMB_RE.search(str(n).lower()),(t,'limb asset drawn',n)
  assert -2<=x0<=x1<=W+2 and -2<=y0<=y1<=H+2,(t,kd,n,(x0,y0,x1,y1))
  if kd in ('code','card','label','person','obj','node','route','panel'):assert y1<1560,(t,'content enters caption band',kd,n,y1)
  if kd=='caption':assert 1570<=y0 and y1<=1780,(t,'caption outside safe band',n)
  if kd in ('label','code'):assert x0>=36 and x1<=W-36,(t,'text outside side margins',kd,n)
  assert kd not in ('label','review','brand'),(t,'label / scene id / extra text drawn',kd,n)   # this sample draws no labels at all
 tx=[(kd,b,n) for kd,b,n in items if kd=='code']
 for i in range(len(tx)):
  for j in range(i+1,len(tx)):
   if abs(tx[i][1][1]-tx[j][1][1])>8:assert not overlap(tx[i][1],tx[j][1],-2),(t,'code rows overlap',tx[i],tx[j])
 check_state(t,s,st)
def check_state(t,s,st):
 if s=='hook':
  assert not st['arrived'],(t,'the email must never arrive')
  if st['check']>0:assert st['order']>=1,(t,'saved before the order exists')
 if s=='laptop':assert st['warn']==0,(t,'logs must stay quiet')
 if s in ('code','crash','rollback','dual'):
  assert set(st['visible'])<=set(SNIP.replace(' ','').replace('\n','')),(t,'unknown text on card')
  if st['db_ok']>0:assert st['order_at_db']>0,(t,'database check before the order arrived')
  if st['broker_got']:assert st['env_sent'],(t,'broker got an event nobody sent')
  if st['email']>0 or st['inv']>0:assert st['broker_got'],(t,'subscriber reacted before the broker had the event')
  if st['em_env'] is not None or st['in_env'] is not None:assert st['broker_got'],(t,'fan-out before the broker had the event')
 if s=='code' and st['env_sent']:assert st['visible'].count('Publish')>=1 or t>=T_PUB[0],(t,'publish before typed')
 if s=='crash':
  assert st['email']==0 and st['inv']==0 and not st['env_sent'],(t,'event delivered in the crash replay')
  if st['crash']>0:assert st['db_ok']>0,(t,'crash must come after the commit')
 if s=='rollback':
  if st['rollback']>0:assert st['order_at_db']>0 and st['email']>=1,(t,'rollback before the save / before subscribers reacted')
  if st['ghost']>0:assert st['rollback']>=1,(t,'ghost order before the rollback')
  if st['env_sent']:assert st['swap']>=1,(t,'publish-first ran before the rows swapped')
def selftest():
 assert row_typed(1)<T_SAVE[0] and row_typed(3)<T_PUB[0],('execute after typing',TYPE,T_SAVE,T_PUB)
 assert T_DBOK<T_PUB[0]<T_BROK<T_EM[0]<T_EM[1]<=T_IN[1] and T_IN[1]<T_TWO,('code flow order',T_SAVE,T_PUB,T_EM,T_IN,T_TWO)
 assert T_SNAP<se('code'),'snap inside the code scene'
 assert C_DBOK<C_CRASH<C_DOWN<C_GONE<C_EMAIL<se('crash'),('crash order',C_DBOK,C_CRASH,C_DOWN,C_GONE,C_EMAIL)
 assert R_SWAP[1]<R_PUB[0]<R_BROK<R_EM[1]<R_SAVE[0]<R_RB[0]<R_GHOST[0]<R_EXIST<se('rollback'),('rollback order',R_SWAP,R_PUB,R_EM,R_SAVE,R_RB,R_GHOST)
 assert D_SPLIT[1]<se('dual')-0.2
 fx=[('email arrived',lambda:check_state(2.0,'hook',{'arrived':True,'check':0,'order':1})),
  ('db check early',lambda:check_state(7.0,'code',{'visible':'','db_ok':0.5,'order_at_db':0,'broker_got':False,'env_sent':False,'email':0,'inv':0,'em_env':None,'in_env':None})),
  ('subscriber early',lambda:check_state(9.0,'code',{'visible':'','db_ok':0,'order_at_db':0,'broker_got':False,'env_sent':True,'email':0.3,'inv':0,'em_env':None,'in_env':None})),
  ('crash before commit',lambda:check_state(19.0,'crash',{'visible':'','db_ok':0,'order_at_db':0,'broker_got':False,'env_sent':False,'email':0,'inv':0,'em_env':None,'in_env':None,'crash':0.5})),
  ('ghost before rollback',lambda:check_state(29.0,'rollback',{'visible':'','db_ok':0,'order_at_db':1,'broker_got':True,'env_sent':True,'email':1,'inv':1,'em_env':None,'in_env':None,'rollback':0.5,'ghost':0.2,'swap':1})),
  ('label drawn',lambda:check_frame(1.0,'hook',{'arrived':False,'check':0,'order':0},[('label',(100,100,300,150),'OrderPlaced')])),
  ('scene id drawn',lambda:check_frame(1.0,'hook',{'arrived':False,'check':0,'order':0},[('review',(60,96,470,140),'S1')])),
  ('hand asset',lambda:check_frame(1.0,'hook',{'arrived':False,'check':0,'order':0},[('person',(10,10,100,100),'lid-hand')])),
  ('caption band',lambda:check_frame(1.0,'hook',{'arrived':False,'check':0,'order':0},[('obj',(10,1500,100,1600),'x')]))]
 out={}
 for name,fn in fx:
  try:fn();raise SystemExit('NEGATIVE FIXTURE NOT CAUGHT: '+name)
  except AssertionError:out[name]='caught'
 out['times']={'scenes':TL.SCENES,'total':TOTAL,'type':TYPE,'save':T_SAVE,'pub':T_PUB,'em':T_EM,'inv':T_IN,'snap':T_SNAP,'crash':[C_SAVE,C_CRASH,C_DOWN,C_GONE,C_EMAIL,C_INV],
  'rollback':[R_SWAP,R_PUB,R_EM,R_IN,R_SAVE,R_RB,R_GHOST,R_EXIST],'dual':[D_DUAL,D_SPLIT]}
 return out
def events():
 return {'cust_in':0.0,'order_pop':T_ORDER,'saved':T_SAVED,'env_try':T_EMAIL+0.05,'env_fail':T_NEVER+0.15,'cuts':[ss(x) for x in ('laptop','code','crash','rollback','dual')],
  'lid_open':[OPEN_A,OPEN_B],'lid_click':OPEN_B,'screen_on':GLOW[0],'logs':[GLOW[0]+0.1+0.16*i for i in range(int((ss('code')-GLOW[0]-0.1)/0.16))],
  'keys':[x for x,ch in zip(CODE_T,SNIP) if not ch.isspace()],'nodes':T_NODES[0],'save':T_SAVE,'db_ok':T_DBOK,'pub':T_PUB,'broker':T_BROK,'em':T_EM,'inv':T_IN,
  'two':T_TWO,'one1':T_ONE1,'one2':T_ONE2,'string':T_NOTH,'snap':T_SNAP,
  'c_save':C_SAVE,'c_dbok':C_DBOK,'c_crash':C_CRASH,'c_down':C_DOWN,'c_commit':C_COMMIT,'c_gone':C_GONE,'c_email':C_EMAIL,'c_inv':C_INV,
  'r_swap':R_SWAP,'r_pub':R_PUB,'r_broker':R_BROK,'r_em':R_EM,'r_in':R_IN,'r_save':R_SAVE,'r_rb':R_RB,'r_ghost':R_GHOST,'r_exist':R_EXIST,
  'd_unswap':D_UNSWAP,'d_dual':D_DUAL,'d_split':D_SPLIT,'end':TOTAL,'music':{'calm':ss('crash')}}
def render(out):
 cmd=['ffmpeg','-hide_banner','-nostdin','-y','-f','rawvideo','-pix_fmt','rgb24','-s',f'{W}x{H}','-r',str(FPS),'-i','pipe:0','-c:v','libx264','-preset','medium','-crf','18','-pix_fmt','yuv420p','-movflags','+faststart',str(out)]
 proc=subprocess.Popen(cmd,stdin=subprocess.PIPE,stderr=open(P/'evidence/render-video.log','w'))
 for i in range(COUNT):
  proc.stdin.write(frame(i/FPS).tobytes())
  if i%120==0:print('frame',i,'/',COUNT,flush=True)
 proc.stdin.close();assert proc.wait()==0
# ================================================================ final-v1: the fix, the relay, trade-offs, delay, recap, close, end card
# Generic code card (the good card and the relay card); the bad card above keeps its own row-swap logic.
class Card:
 def __init__(s,src,top,n,lh,typ,name):
  s.src=src.rstrip('\n');s.rows=s.src.split('\n');s.top=top;s.n=n;s.lh=lh;s.y0=top+24;s.name=name;s.adv=font(n,mono=True).getlength('M')
  assert max(len(r) for r in s.rows)<=34,(name,'line over 34 characters')
  s.T=type_times(s.src,*typ) if typ else [0.0]*len(s.src);s.typ=typ;s.bottom=s.y0+len(s.rows)*lh+12
  tok=code_tokens(s.src);cur=[[]]
  for v,c in tok:
   for i,pp in enumerate(v.split('\n')):
    if i>0:cur.append([])
    if pp:cur[-1].append((pp,c))
  s.rt=cur;assert len(cur)==len(s.rows);s.rs=[sum(len(r)+1 for r in s.rows[:i]) for i in range(len(s.rows))]
 def row_typed(s,i):return s.T[s.rs[i]+len(s.rows[i])-1]
 def done(s):return s.T[-1]
 def draw(s,d,t,glows=None,alpha=1.0,ra=None):
  d.rounded_rectangle((CODE_X-30,s.top,W-CODE_X+30,s.bottom),22,fill=(11,22,31),outline=dim((80,130,150),alpha),width=3);K.LOG.append(('card',(CODE_X-30,s.top,W-CODE_X+30,s.bottom),s.name))
  vis=''
  for i,toks in enumerate(s.rt):
   a_=alpha*(ra[i] if ra else 1.0)
   if a_<0.03:continue
   y=s.y0+i*s.lh;g,gc=(glows[i] if glows else (0.0,CY))
   if g>0:d.rounded_rectangle((CODE_X-12,y-4,CODE_X+s.adv*len(s.rows[i])+12,y+s.lh-6),8,fill=tuple(int(v*0.16*g+11*(1-0.16*g)) for v in gc),outline=dim(gc,g*0.8*a_),width=2)
   x=CODE_X;pos=s.rs[i]
   for v,c in toks:
    n=sum(1 for k in range(len(v)) if s.T[pos+k]<=t)
    if n:text(d,(x,y),v[:n],s.n,dim(c,a_),mono=True,kind='code');vis+=v[:n]
    x+=s.adv*len(v);pos+=len(v)
  nxt=next((k for k in range(len(s.src)) if s.T[k]>t),None)
  if s.typ and nxt is not None and t>=s.typ[0]-0.3 and ((t*CARET_HZ)%1.0)<0.6:
   pre=s.src[:nxt];r=pre.count('\n');cc=len(pre)-(pre.rfind('\n')+1);x=CODE_X+s.adv*cc;y=s.y0+r*s.lh;d.rectangle((x,y+4,x+3,y+s.lh-10),fill=IV)
  return vis.replace(' ','')
def glow_rows(n,spec):
 g=[(0.0,CY)]*n
 for rows,val,col in spec:
  for r in rows:
   if val>g[r][0]:g[r]=(val,col)
 return g
def bump(t,a,b):return math.sin(math.pi*lin(t,a,b))
GOOD=(ROOT/'planning/script-v1/code/good-outbox.cs').read_text();RELAY=(ROOT/'planning/script-v1/code/relay.cs').read_text()
F0=ss('fix');FT=(WT('N06','transactional'),WT('N06','Add')-0.15)
GCARD=Card(GOOD,CARD_TOP,CODE_N,CODE_LH,FT,'good')
GCARD_STATIC=Card(GOOD,CARD_TOP,CODE_N,CODE_LH,None,'good')
RL0=ss('relay');RCARD=Card(RELAY,214,38,47,(RL0+0.3,WT('N07','polls')+0.35),'relay')
assert RCARD.bottom<1030,RCARD.bottom
# ---------------------------------------------------------------- shared drawings
def gear(d,c,r,ang,alpha=1.0,crack=0.0,col=CY):
 col=dim(col if crack<=0 else MUT,alpha);pts=[]
 for i in range(16):
  a=ang+i*math.pi/8;rr=r if i%2==0 else r*0.80
  for da in (-0.13,0.13):pts.append((c[0]+rr*math.cos(a+da),c[1]+rr*math.sin(a+da)))
 d.polygon(pts,fill=(16,30,40),outline=col);d.line(pts+[pts[0]],fill=col,width=4)
 d.ellipse((c[0]-r*0.48,c[1]-r*0.48,c[0]+r*0.48,c[1]+r*0.48),outline=col,width=4)
 d.line((c[0],c[1],c[0]+r*0.38*math.cos(ang*2-math.pi/2),c[1]+r*0.38*math.sin(ang*2-math.pi/2)),fill=col,width=4)   # clock hand: ticks once per poll
 if crack>0:
  z=[(c[0]-r*0.9,c[1]-r*0.5),(c[0]-r*0.2,c[1]-r*0.05),(c[0]-r*0.45,c[1]+r*0.2),(c[0]+r*0.3,c[1]+r*0.35),(c[0]+r*0.1,c[1]+r*0.6),(c[0]+r*0.85,c[1]+r*0.75)]
  d.line(path_upto(z,crack),fill=dim(RD,alpha),width=6)
 K.LOG.append(('node',(c[0]-r,c[1]-r,c[0]+r,c[1]+r),'relay gear'))
def broker_box(d,box,alpha=1.0,down=0.0,got=0.0):
 x0,y0,x1,y1=box;col=dim(VI,alpha*(1-0.5*down));d.rounded_rectangle(box,14,fill=(26,20,36),outline=col,width=5)
 d.polygon([(x0-10,y0+5),((x0+x1)/2,y0-38),(x1+10,y0+5)],outline=col,fill=(26,20,36));d.rectangle(((x0+x1)/2-46,y0+20,(x0+x1)/2+46,y0+32),fill=(8,10,14))
 for i in range(3):xx=x0+28+i*((x1-x0-56)/3);d.rectangle((xx+4,y0+52,xx+(x1-x0-56)/3-4,y1-16),outline=dim((110,96,140),alpha),width=3)
 if got>0:d.rounded_rectangle((x0-8,y0-8,x1+8,y1+8),18,outline=dim(GR,got),width=3)
 if down>0:xmark(d,((x0+x1)/2,(y0+y1)/2),int(52*down),dim(RD,alpha),9)
 K.LOG.append(('node',(x0-10,y0-38,x1+10,y1+8),'broker'))
def node2(d,c,r,lit=0.0,kind='email',alpha=1.0,grey=0.0,ring=0.0,box_lift=0.0):
 col=dim(GR if lit>0 else (120,150,170),alpha*(1-0.55*grey))
 if lit>0:
  for g in range(3):e=6+g*7;d.ellipse((c[0]-r-e,c[1]-r-e,c[0]+r+e,c[1]+r+e),outline=dim(GR,lit*(0.45-0.13*g)),width=3)
 if ring>0:d.ellipse((c[0]-r-10,c[1]-r-10,c[0]+r+10,c[1]+r+10),outline=dim(CY,ring),width=4)
 d.ellipse((c[0]-r,c[1]-r,c[0]+r,c[1]+r),fill=(14,26,34),outline=col,width=5)
 if kind=='email':envelope(d,c,r/58,alpha,col,log=False)
 else:
  s_=r/72;d.line((c[0]-46*s_,c[1]+12*s_,c[0]+46*s_,c[1]+12*s_),fill=col,width=5);y=-46*s_*box_lift
  d.rectangle((c[0]-26*s_,c[1]-30*s_+y,c[0]+26*s_,c[1]+10*s_+y),fill=(48,40,26),outline=dim(AM,alpha),width=3)
 if lit>=1:check(d,(c[0]+r-8,c[1]-r+14),16,GR,6)
 K.LOG.append(('node',(c[0]-r,c[1]-r,c[0]+r,c[1]+r),kind))
def num_env(d,c,s,num,alpha=1.0,col=VI,sent=0.0):
 envelope(d,c,s,alpha,col)
 if num is not None:   # message id on a small badge at the envelope corner (numbers only)
  bc=(c[0]+32*s,c[1]-22*s);r=17*s;d.ellipse((bc[0]-r,bc[1]-r,bc[0]+r,bc[1]+r),fill=(40,30,12),outline=dim(AM,alpha),width=2)
  text(d,(bc[0],bc[1]-14*s),str(num),int(24*s),dim(IV,alpha),True,kind='label',center=True)
 if sent>0:check(d,(c[0]+24*s,c[1]-16*s),12*s,dim(GR,sent),5)
def band(d,box,k,col=AM,alpha=1.0):
 if k<=0:return
 x0,y0,x1,y1=box;pts=[(x0,y0),(x1,y0),(x1,y1),(x0,y1),(x0,y0)];pp=path_upto(pts,k)
 d.line(pp,fill=dim(col,alpha),width=6,joint='curve');K.LOG.append(('obj',box,'transaction band'))
def outbox_db(d,c,rx,h,alpha=1.0,tray=1.0,col=IV):
 draw_db(d,c,rx,h,col,alpha=alpha);x,y=c
 if tray>0:
  d.line((x+4,y-h/2+rx*0.3+6,x+4,y+h/2+4),fill=dim(MUT,alpha*tray),width=3)   # divider: orders | outbox
  d.rounded_rectangle((x+16,y-h/2+rx*0.3+10,x+rx-14,y+h/2-4),8,outline=dim(AM,alpha*tray),width=3)
 K.LOG.append(('node',(x-rx,y-h/2-rx*0.3,x+rx,y+h/2+rx*0.3),'database'))
def broom(d,c,alpha=1.0):
 x,y=c;d.line((x+40,y-120,x,y-20),fill=dim(AM,alpha),width=7)
 d.polygon([(x-38,y+22),(x+38,y+22),(x+18,y-22),(x-12,y-22)],fill=(48,40,26),outline=dim(AM,alpha))
 for k in range(7):xx=x-34+k*11;d.line((xx+4,y-14,xx,y+22),fill=dim(AM,alpha*0.8),width=2)
 K.LOG.append(('obj',(x-40,y-120,x+42,y+24),'broom'))
def clock(d,c,r,ang,alpha=1.0,sweep=0.0):
 d.ellipse((c[0]-r,c[1]-r,c[0]+r,c[1]+r),fill=(14,26,34),outline=dim(IV,alpha),width=6)
 for i in range(12):a=i*math.pi/6;d.line((c[0]+r*0.82*math.cos(a),c[1]+r*0.82*math.sin(a),c[0]+r*0.94*math.cos(a),c[1]+r*0.94*math.sin(a)),fill=dim(MUT,alpha),width=4)
 if sweep>0:d.pieslice((c[0]-r*0.8,c[1]-r*0.8,c[0]+r*0.8,c[1]+r*0.8),-90,-90+360*sweep,fill=dim((60,52,30),alpha))
 a=-math.pi/2+ang;d.line((c[0],c[1],c[0]+r*0.72*math.cos(a),c[1]+r*0.72*math.sin(a)),fill=dim(AM,alpha),width=7);d.ellipse((c[0]-9,c[1]-9,c[0]+9,c[1]+9),fill=dim(AM,alpha))
 K.LOG.append(('node',(c[0]-r,c[1]-r,c[0]+r,c[1]+r),'clock'))
def ledger(d,box,rows,alpha=1.0,hi=None,hi_k=0.0,new_k=1.0,check_k=0.0):
 x0,y0,x1,y1=box;d.rounded_rectangle(box,16,fill=(14,22,30),outline=dim((150,170,185),alpha),width=4);K.LOG.append(('panel',box,'ledger'))
 d.line((x0+20,y0+70,x1-20,y0+70),fill=dim(MUT,alpha),width=3)
 for i,num in enumerate(rows):
  y=y0+96+i*86;a=alpha*(new_k if i==len(rows)-1 else 1.0)
  if a<=0.02:continue
  if hi==i and hi_k>0:d.rounded_rectangle((x0+16,y-12,x1-16,y+62),10,fill=(18,40,40),outline=dim(CY,hi_k),width=3)
  text(d,(x0+44,y),str(num),48,dim(IV,a),True,kind='label');d.line((x0+120,y+30,x1-40,y+30),fill=dim(MUT,a*0.7),width=3)
  if hi==i and check_k>0:check(d,(x1-60,y+24),18,dim(GR,check_k),6)
# ---------------------------------------------------------------- the fix (N06): outbox row in the same transaction
DBW=118;ORD_SLOT=(DB[0]-52,DB[1]+6);TRAY_SLOT=(DB[0]+60,DB[1]+6);ST_ORD=(415,1180);ST_ENV=(520,1180);BAND=(358,1132,575,1230)
FX={'swap':(F0+0.03,F0+0.4),'tray':(WT('N06','outbox')-0.1,WT('N06','outbox')+0.4),'dont':WT('N06',"Don't"),'order':WT('N06','Add'),
 'env':WT('N06','event'),'band':(WT('N06','same'),WT('N06','same')+0.5),'save':WT('N06','Save'),'move':(WT('N06','once.')-0.1,WT('N06','once.')+0.55),
 'one':WT('N06','one'),'both':WT('N06','Both'),'neither':WT('N06','neither')}
FX['land']=FX['move'][1];FX['ok']=max(FX['one']+0.1,FX['land']+0.3)
def fix_state(t):
 st={'scene':'fix','order':prog(t,FX['order'],FX['order']+0.25),'env':prog(t,FX['env'],FX['env']+0.25),'band':prog(t,*FX['band']),'move':lin(t,*FX['move']),
  'landed':t>=FX['land'],'ok':prog(t,FX['ok'],FX['ok']+0.3),'both':bump(t,FX['both'],FX['both']+0.7),'neither':prog(t,FX['neither'],FX['neither']+0.35),
  'dont':prog(t,FX['dont'],FX['dont']+0.35),'tray':prog(t,*FX['tray']),'bad_a':1-prog(t,*FX['swap'])}
 st['band_moving']=0<st['move']<1;return st
def fix_flow(d,t,st):
 app_icon(d);outbox_db(d,DB,DBW,DBH+10,1.0,st['tray']);broker(d,1.0);node(d,EMAIL,1.0,0,'email');node(d,INV,1.0,0,'inventory')
 arrow(d,A_DB,1.0,dim(CY,0.85),5)
 if st['dont']>0:
  arrow(d,A_BR,1.0,dim(MUT,0.35+0.4*(1-st['dont'])),4,dashed=True);m=along(A_BR,0.5);xmark(d,m,int(22*st['dont']),dim(MG,1.0),7)
 else:arrow(d,A_BR,1.0,dim(VI,0.7),5)
 arrow(d,B_EM,1.0,dim((90,80,110),0.8),3);arrow(d,B_IN,1.0,dim((90,80,110),0.8),3)
 if not st['landed']:
  k=st['move'];off=(along(A_DB,k)[0]-A_DB[0][0],along(A_DB,k)[1]-A_DB[0][1]) if k>0 else (0,0)
  to_o=mix(ST_ORD,ORD_SLOT,ease(k));to_e=mix(ST_ENV,TRAY_SLOT,ease(k))
  if st['band']>0:
   bx=(BAND[0]+(to_o[0]-ST_ORD[0]),BAND[1]+(to_o[1]-ST_ORD[1]),BAND[2]+(to_e[0]-ST_ENV[0]),BAND[3]+(to_e[1]-ST_ENV[1]))
   if k>0:sh=1-0.35*ease(k);cx_,cy_=(bx[0]+bx[2])/2,(bx[1]+bx[3])/2;w_,h_=(bx[2]-bx[0])*sh/2,(bx[3]-bx[1])*sh/2;bx=(cx_-w_,cy_-h_,cx_+w_,cy_+h_)
   band(d,bx,st['band'])
  if st['order']>0:mini_order(d,to_o,st['order'])
  if st['env']>0:num_env(d,to_e,0.8,None,st['env'])
 else:
  mini_order(d,ORD_SLOT,1.0);num_env(d,TRAY_SLOT,0.8,None,1.0)
  g=st['ok'];band(d,(DB[0]-DBW-14,DB[1]-DBH/2-58,DB[0]+DBW+14,DB[1]+DBH/2+52),1.0,AM,0.45+0.55*g)
  if g>0:check(d,(DB[0]+DBW+6,DB[1]-DBH/2-70),24*g,dim(GR,g),7)
  if st['both']>0:
   for c in (ORD_SLOT,TRAY_SLOT):d.ellipse((c[0]-52,c[1]-40,c[0]+52,c[1]+40),outline=dim(GR,st['both']),width=4)
 if st['neither']>0:   # the other outcome: one failure rolls both back, neither row exists
  a=st['neither'];ghost_order(d,ST_ORD,a,MG);envelope(d,ST_ENV,0.8,a,MG,dashed=True);xmark(d,((ST_ORD[0]+ST_ENV[0])/2,ST_ORD[1]),int(34*a),dim(MG,a),7)
def fix_card(d,t,st):
 if st['bad_a']>0.03:code_card(d,t,[0,1,2,3],[(0.0,CY)]*4,alpha=st['bad_a'])
 g=glow_rows(4,[([2],bump(t,FX['order']-0.05,FX['band'][1]+0.4),AM),([3],bump(t,FX['save']-0.1,FX['land']+0.3),CY),([0,1,2,3],bump(t,FX['ok'],FX['ok']+0.8)*0.8,GR)])
 return GCARD.draw(d,t,g)
# ---------------------------------------------------------------- relay layout (N07, N09) and trade-offs layout (N08)
RDB=(180,1330);RDBW=148;RDBH=210;GEAR=(478,1330);GR_R=56;RBOX=(598,1250,808,1410);REM=(950,1210);RIN=(950,1450);RNR=56
TRAY_C=[(RDB[0]+50+(i%2)*60,RDB[1]-52+(i//2)*50) for i in range(6)]   # outbox rows (two columns inside the tray half)
P_TG=[(RDB[0]+RDBW-2,RDB[1]),(GEAR[0]-GR_R-4,GEAR[1])];P_GB=[(GEAR[0]+GR_R+4,GEAR[1]),(RBOX[0]-2,GEAR[1])]
P_BE=[(RBOX[2]+2,RBOX[1]+40),(REM[0]-RNR-4,REM[1]+10)];P_BI=[(RBOX[2]+2,RBOX[3]-40),(RIN[0]-RNR-4,RIN[1]-10)]
def trip(t,a,src):  # one message copy: tray row -> gear -> broker, positions along the joined path
 k=lin(t,a,a+0.62);pts=[src]+P_TG[1:]+P_GB;return (along(pts,k) if 0<k<1 else None),k
def relay_base(d,t,tray,alpha=1.0,gear_ang=0.0,crack=0.0,down=0.0,got=0.0,em=0.0,inv=0.0,inv_lift=0.0,subs=True):
 outbox_db(d,RDB,RDBW,RDBH,alpha);gear(d,GEAR,GR_R,gear_ang,alpha,crack);broker_box(d,RBOX,alpha,down,got)
 if subs:node2(d,REM,RNR,em,'email',alpha);node2(d,RIN,RNR,inv,'inventory',alpha,box_lift=inv_lift)
 for p in ((P_TG,P_GB,P_BE,P_BI) if subs else (P_TG,P_GB)):arrow(d,p,1.0,dim((90,100,120),alpha*0.8),3)
 for i,(num,sent,a_) in enumerate(tray):
  if a_>0.02:num_env(d,TRAY_C[i],0.78,num,a_*alpha,VI if sent<1 else dim(GR,0.8),sent)
RX={'poll':RCARD.done()+0.1}
RX['pubA']=max(WT('N07','publishes')+0.05,RX['poll']+0.5);RX['gotA']=RX['pubA']+0.62;RX['emA']=(RX['gotA']+0.05,RX['gotA']+0.4);RX['inA']=(RX['gotA']+0.15,RX['gotA']+0.5)
RX['markA']=max(WT('N07','marks'),RX['inA'][1]+0.1);RX['down']=WT('N07','Broker');RX['tryB']=WT('N07','message',1)-0.1;RX['bounce']=RX['tryB']+0.62   # the failed try runs under "The message", not in the pause
RX['wait']=max(WT('N07','waits'),RX['bounce']+0.12);RX['up']=WT('N07','relay',1)-0.6;RX['retry']=WT('N07','relay',1)-0.1;RX['gotB']=RX['retry']+0.62;RX['emB']=(RX['gotB']+0.03,RX['gotB']+0.3)
RX['inB']=(RX['gotB']+0.08,RX['gotB']+0.35);RX['markB']=RX['inB'][1]+0.05
def relay_state(t):
 st={'scene':'relay','down':prog(t,RX['down'],RX['down']+0.3)*(1-prog(t,RX['up'],RX['up']+0.3)),'gotA':t>=RX['gotA'],'gotB':t>=RX['gotB'],
  'markA':prog(t,RX['markA'],RX['markA']+0.25),'markB':prog(t,RX['markB'],RX['markB']+0.25)}
 st['em']=max(prog(t,RX['emA'][1],RX['emA'][1]+0.2)*(1-prog(t,RX['down']-0.3,RX['down'])),prog(t,RX['emB'][1],RX['emB'][1]+0.15))
 st['inv']=max(prog(t,RX['inA'][1],RX['inA'][1]+0.2)*(1-prog(t,RX['down']-0.3,RX['down'])),prog(t,RX['inB'][1],RX['inB'][1]+0.15))
 st['em_after_got']=(st['em']<=0) or st['gotA'];st['broker_down_at_bounce']=True
 return st
def relay_flow(d,t,st):
 ang=math.pi/4*(prog(t,RX['poll'],RX['poll']+0.3)+prog(t,RX['tryB']-0.3,RX['tryB'])+prog(t,RX['retry']-0.3,RX['retry']))
 relay_base(d,t,[(None,st['markA'],1.0),(None,st['markB'],1.0)],1.0,ang,0,st['down'],0,st['em'],st['inv'],st['inv'])
 sc=bump(t,RX['poll'],RX['poll']+0.6)
 if sc>0:
  for c in TRAY_C[:2]:d.line((c[0],c[1],GEAR[0]-GR_R,GEAR[1]),fill=dim(CY,sc*0.7),width=3)
 p,k=trip(t,RX['pubA'],TRAY_C[0])
 if p:num_env(d,p,0.7,None)
 for a,b,P in ((RX['emA'][0],RX['emA'][1],P_BE),(RX['inA'][0],RX['inA'][1],P_BI),(RX['emB'][0],RX['emB'][1],P_BE),(RX['inB'][0],RX['inB'][1],P_BI)):
  if a<=t<b:num_env(d,along(P,lin(t,a,b)),0.6,None)
 kb=lin(t,RX['tryB'],RX['bounce']);pts=[TRAY_C[1]]+P_TG[1:]+P_GB
 if 0<kb<1:num_env(d,along(pts,kb*0.85),0.7,None)
 kr=lin(t,RX['bounce'],RX['bounce']+0.45)
 if 0<kr<1:num_env(d,along(pts[::-1],kr*0.85+0.15) ,0.7,None,1.0,MG)
 if RX['bounce']<=t<RX['bounce']+0.3:xmark(d,(RBOX[0]-18,GEAR[1]),18,MG,6)
 w=bump(t,RX['wait'],RX['wait']+0.9)
 if w>0:c=TRAY_C[1];d.rounded_rectangle((c[0]-28,c[1]-22,c[0]+28,c[1]+22),6,outline=dim(AM,w),width=4)
 p,k=trip(t,RX['retry'],TRAY_C[1])
 if p:num_env(d,p,0.7,None)
def relay_card(d,t,st):
 g=glow_rows(16,[([3,4,5,6],bump(t,RX['poll'],RX['pubA']+0.2),CY),([9],bump(t,RX['pubA']-0.1,RX['gotA']+0.2),VI),([11],bump(t,RX['markA']-0.1,RX['markA']+0.6),GR),
  ([9,10],bump(t,RX['tryB'],RX['bounce']+0.5),MG),([14],bump(t,RX['wait'],RX['retry']),AM),([9],bump(t,RX['retry']-0.05,RX['gotB']+0.2),VI),([11],bump(t,RX['markB']-0.1,RX['markB']+0.5),GR)])
 return RCARD.draw(d,t,g)
# trade-offs: duplicates and an idempotent consumer with a ledger
T0=ss('trade');TBUST=(40,330);TBUST_W=380;LEDGER=(560,300,1040,700);TCON=(800,860);TCON_R=86
TBUST_IM=person(K.A/'jeffrit-bust-video.png',TBUST_W)
TX={'pub1':WT('N08','crashes')-0.05}
TX['got1']=TX['pub1']+0.62;TX['con1']=(TX['got1']+0.05,TX['got1']+0.55);TX['rec1']=TX['con1'][1]+0.1;TX['crack']=max(WT('N08','before'),TX['rec1']+0.1)
TX['heal']=(TX['crack']+0.7,TX['crack']+1.0);TX['pub2']=max(WT('N08','publishes')-0.1,TX['heal'][1]+0.15);TX['got2']=TX['pub2']+0.62;TX['con2']=(TX['got2']+0.05,TX['got2']+0.55)
TX['twice']=WT('N08','at-least-once');TX['idem']=WT('N08','idempotent.');TX['record']=WT('N08','Record');TX['match']=(WT('N08','skip')-0.25,WT('N08','skip')+0.1);TX['drop']=(TX['match'][1],TX['match'][1]+0.6)
P_BC=[(RBOX[2]+2,RBOX[1]+30),(TCON[0],RBOX[1]+30),(TCON[0],TCON[1]+TCON_R+4)]
def trade_state(t):
 st={'scene':'trade','got1':t>=TX['got1'],'rec1':prog(t,TX['rec1'],TX['rec1']+0.3),'crack':lin(t,TX['crack'],TX['crack']+0.3)*(1-prog(t,*TX['heal'])),'pub2':t>=TX['pub2'],
  'got2':t>=TX['got2'],'arr2':t>=TX['con2'][1],'match':lin(t,*TX['match']),'drop':lin(t,*TX['drop'])}
 return st
def trade_scene(t):
 f=BOARD.copy();paste_person(f,TBUST_IM,TBUST,prog(t,T0,T0+0.3),'jeffrit gigachad bust front-on');d=ImageDraw.Draw(f);st=trade_state(t)
 ang=math.pi/4*(prog(t,TX['pub1']-0.3,TX['pub1'])+prog(t,TX['pub2']-0.3,TX['pub2']))
 relay_base(d,t,[(7,0.0,1.0)],1.0,ang,st['crack'],0,0,0,0,subs=False)
 arrow(d,P_BC,1.0,dim((90,100,120),0.8),3)
 node2(d,TCON,TCON_R,0,'email',1.0,ring=bump(t,TX['idem'],TX['idem']+1.0))
 rows=[5,6,7] if st['rec1']>0 else [5,6]
 ledger(d,LEDGER,rows,1.0,hi=2 if st['rec1']>0 else None,hi_k=max(bump(t,TX['rec1'],TX['rec1']+0.6),bump(t,TX['record'],TX['record']+1.1),st['match'] if st['drop']<1 else 0),new_k=st['rec1'] if st['rec1']>0 else 1.0,
  check_k=prog(t,TX['record']+0.2,TX['record']+0.5))
 for a,src in ((TX['pub1'],TRAY_C[0]),(TX['pub2'],TRAY_C[0])):
  p,k=trip(t,a,src)
  if p:num_env(d,p,0.85,7)
 for a,b in (TX['con1'],TX['con2']):
  if a<=t<b:num_env(d,along(P_BC,lin(t,a,b)),0.85,7)
 if TX['con1'][1]<=t<TX['rec1']+0.3:   # first copy handled: it goes into the ledger
  k=prog(t,TX['con1'][1],TX['rec1']+0.3);num_env(d,mix((TCON[0],TCON[1]-TCON_R-40),(LEDGER[0]+70,LEDGER[1]+96+2*86+20),k),0.75,7,1-k)
 if st['arr2'] and st['drop']<1:   # the duplicate waits beside the consumer, then matches row 7 and is skipped
  c=(TCON[0]-TCON_R-90,TCON[1]+10);a=1-st['drop'];num_env(d,c,1.3,7,a)
  tw=prog(t,TX['twice'],TX['twice']+0.3)
  if tw>0:text(d,(c[0]-60,c[1]-104),'×2',52,dim(AM,tw*a),True,kind='label')
  if st['match']>0:arrow(d,[(c[0]+10,c[1]-30),(LEDGER[0]+20,LEDGER[1]+96+2*86+24)],st['match'],dim(CY,a),4,dashed=True)
  if st['drop']>0:xmark(d,c,int(30*min(1,st['drop']*2)),dim(MG,a),7)
 return f,st
# delay and cleanup (N09)
D9=ss('delay');DCLK=(790,560);DCLK_R=170
DX={'new':WT('N09','Delivery')+0.1,'tick':(WT('N09','Polling'),WT('N09','delay.')+0.1)};DX['pub']=DX['tick'][1]+0.05;DX['got']=DX['pub']+0.62;DX['em']=(DX['got']+0.05,DX['got']+0.4);DX['mark']=DX['em'][1]+0.1
DX['grow']=(WT('N09','table'),WT('N09','growing,')+0.35);DX['late']=DX['grow'][1]+0.1;DX['sweep']=(WT('N09','clean')-0.05,WT('N09','rows.')+0.2)
def delay_state(t):
 st={'scene':'delay','got':t>=DX['got'],'em':prog(t,DX['em'][1],DX['em'][1]+0.2),'mark':prog(t,DX['mark'],DX['mark']+0.25),'grow':lin(t,*DX['grow']),'sweep':lin(t,*DX['sweep'])}
 return st
def delay_scene(t):
 f=BOARD.copy();paste_person(f,TBUST_IM,TBUST,prog(t,D9,D9+0.3),'jeffrit gigachad bust front-on');d=ImageDraw.Draw(f);st=delay_state(t)
 sw=lin(t,*DX['tick']);clock(d,DCLK,DCLK_R,2*math.pi*ease(sw),1.0,sw if 0<sw<1 else 0)
 nb=int(round(4*st['grow']));bx=st['sweep']
 tray=[(None,st['mark'],prog(t,DX['new'],DX['new']+0.25))]
 for i in range(4):tray.append((None,1.0,1.0 if i<nb else 0.0))
 tray.append((None,0.0,prog(t,DX['late'],DX['late']+0.25)))   # one new unsent row arrives: it must survive the sweep
 bxx=RDB[0]-RDBW+bx*(2*RDBW+40)
 vis=[]
 for i,(n_,sent,a_) in enumerate(tray):
  if sent>=1 and bx>0 and TRAY_C[i][0]<bxx:a_=0.0
  vis.append((n_,sent,a_))
 ang=math.pi/4*prog(t,DX['pub']-0.3,DX['pub'])
 relay_base(d,t,vis,1.0,ang,0,0,0,st['em'],0)
 for i,(n_,sent,a_) in enumerate(tray):
  if sent>=1 and bx>0 and TRAY_C[i][0]<bxx and a_>0:dust(d,TRAY_C[i],min(1,(bxx-TRAY_C[i][0])/120),GR,8,i)
 p,k=trip(t,DX['pub'],TRAY_C[0])
 if p:num_env(d,p,0.7,None)
 if DX['em'][0]<=t<DX['em'][1]:num_env(d,along(P_BE,lin(t,*DX['em'])),0.6,None)
 w=bump(t,DX['new']+0.2,DX['pub'])
 if w>0:c=TRAY_C[0];d.rounded_rectangle((c[0]-28,c[1]-22,c[0]+28,c[1]+22),6,outline=dim(AM,w),width=4)
 if 0<bx<1:broom(d,(bxx,RDB[1]+20))
 st['unsent_kept']=vis[5][2]>0 or t<DX['late'];st['swept_sent_only']=all(not(v[2]==0 and tray[i][2]>0 and tray[i][1]<1) for i,v in enumerate(vis))
 return f,st
# ---------------------------------------------------------------- recap (N10): main layout, one transaction, then the relay does the talking
RC0=ss('recap');RGEAR=(625,1165);RGR=40
P_TRG=[(TRAY_SLOT[0]+10,TRAY_SLOT[1]-40),(RGEAR[0]-6,RGEAR[1]+RGR+4)];P_RGB=[(RGEAR[0],RGEAR[1]-RGR-4),(RGEAR[0],BROKER[3]+2)]
RCX={'order':WT('N10','database'),'env':WT('N10','tell'),'svc':WT('N10','services,'),'band':(WT('N10','write'),WT('N10','write')+0.4),'move':(WT('N10','same')-0.1,WT('N10','same')+0.45)}
RCX['land']=RCX['move'][1];RCX['ok']=RCX['land']+0.1;RCX['gear']=WT('N10','relay')-0.6;RCX['pub']=WT('N10','relay')-0.1;RCX['got']=RCX['pub']+0.6
RCX['em']=(RCX['got']+0.03,RCX['got']+0.35);RCX['inv']=(RCX['got']+0.1,RCX['got']+0.45);RCX['mark']=RCX['inv'][1]+0.05
def recap_state(t):
 st={'scene':'recap','landed':t>=RCX['land'],'ok':prog(t,RCX['ok'],RCX['ok']+0.3),'got':t>=RCX['got'],'em':prog(t,RCX['em'][1],RCX['em'][1]+0.2),'inv':prog(t,RCX['inv'][1],RCX['inv'][1]+0.2),
  'mark':prog(t,RCX['mark'],RCX['mark']+0.25),'pub':t>=RCX['pub']}
 return st
def recap_flow(d,t,st):
 app_icon(d);outbox_db(d,DB,DBW,DBH+10,1.0,1.0);broker(d,1.0,0,0);node(d,EMAIL,1.0,st['em'],'email');node(d,INV,1.0,st['inv'],'inventory',st['inv'])
 arrow(d,A_DB,1.0,dim(CY,0.85),5);arrow(d,B_EM,1.0,dim((90,80,110),0.8),3);arrow(d,B_IN,1.0,dim((90,80,110),0.8),3)
 sv=bump(t,RCX['svc']-0.3,RCX['svc']+0.6)
 if sv>0:
  for c in (EMAIL,INV):d.ellipse((c[0]-NR-12,c[1]-NR-12,c[0]+NR+12,c[1]+NR+12),outline=dim(CY,sv),width=4)
 ga=prog(t,RCX['gear'],RCX['gear']+0.3)
 if ga>0:
  for p in (P_TRG,P_RGB):arrow(d,p,ga,dim((90,100,120),0.8),3)
  gear(d,RGEAR,RGR,math.pi/4*prog(t,RCX['pub']-0.3,RCX['pub']),ga)
 k=lin(t,*RCX['move'])
 if not st['landed']:
  to_o=mix(ST_ORD,ORD_SLOT,ease(k));to_e=mix(ST_ENV,TRAY_SLOT,ease(k));bk=prog(t,*RCX['band'])
  if bk>0:
   bx=(BAND[0]+(to_o[0]-ST_ORD[0]),BAND[1]+(to_o[1]-ST_ORD[1]),BAND[2]+(to_e[0]-ST_ENV[0]),BAND[3]+(to_e[1]-ST_ENV[1]))
   if k>0:sh=1-0.35*ease(k);cx_,cy_=(bx[0]+bx[2])/2,(bx[1]+bx[3])/2;w_,h_=(bx[2]-bx[0])*sh/2,(bx[3]-bx[1])*sh/2;bx=(cx_-w_,cy_-h_,cx_+w_,cy_+h_)
   band(d,bx,bk)
  o=prog(t,RCX['order'],RCX['order']+0.25);e=prog(t,RCX['env'],RCX['env']+0.25)
  if o>0:mini_order(d,to_o,o)
  if e>0:num_env(d,to_e,0.8,None,e)
 else:
  mini_order(d,ORD_SLOT,1.0);num_env(d,TRAY_SLOT,0.8,None,1.0,VI if st['mark']<1 else dim(GR,0.8),st['mark'])
  g=st['ok'];band(d,(DB[0]-DBW-14,DB[1]-DBH/2-58,DB[0]+DBW+14,DB[1]+DBH/2+52),1.0,AM,0.3+0.6*g*(1-0.5*prog(t,RCX['pub'],RCX['pub']+0.4)))
  if g>0:check(d,(DB[0]+DBW+6,DB[1]-DBH/2-70),24*g,dim(GR,g),7)
 kp=lin(t,RCX['pub'],RCX['got']);pts=P_TRG+P_RGB
 if 0<kp<1:num_env(d,along(pts,kp),0.7,None)
 for a,b,P in ((RCX['em'][0],RCX['em'][1],B_EM),(RCX['inv'][0],RCX['inv'][1],B_IN)):
  if a<=t<b:num_env(d,along(P,lin(t,a,b)),0.7,None)
def recap_card(d,t,st):
 g=glow_rows(4,[([1,2],bump(t,RCX['band'][0],RCX['land']+0.4),AM),([3],bump(t,RCX['move'][0]-0.1,RCX['ok']+0.4),CY)])
 return GCARD_STATIC.draw(d,t,g)
def main_scene(t,s):
 st=fix_state(t) if s=='fix' else recap_state(t);base=BOARD.copy();a0=ss(s)
 paste_person(base,DEV,DEV_XY,1.0 if s=='fix' else prog(t,a0,a0+0.3),'jeffrit gigachad bust front-on');d=ImageDraw.Draw(base)
 st['visible']=(fix_card if s=='fix' else recap_card)(d,t,st)
 full=base.copy();(fix_flow if s=='fix' else recap_flow)(ImageDraw.Draw(full),t,st)
 k=lin(t,a0,a0+0.25)
 return (Image.blend(base,full,k) if k<1 else full),st
def relay_scene(t):
 st=relay_state(t);f=BOARD.copy();d=ImageDraw.Draw(f);st['visible']=relay_card(d,t,st);relay_flow(d,t,st);return f,st
# ---------------------------------------------------------------- close: the physical laptop closes by itself (no hand/arm); end card
T_CL=(ss('close')+0.2,ss('close')+1.05)
def end_panel():
 im=Image.new('RGB',(PW,PH),(12,20,30));d=ImageDraw.Draw(im);d.rectangle((0,0,PW,46),fill=(22,32,44))
 for k,c in enumerate([(255,110,110),(255,200,90),(110,220,140)]):d.ellipse((20+k*28,15,36+k*28,31),fill=c)
 d.ellipse((PW/2-120,PH/2-100,PW/2+120,PH/2+140),fill=(14,40,28),outline=GR,width=8);check(d,(PW/2+4,PH/2+22),60,GR,14);return im
END_PANEL=end_panel()
def close(t):
 k=lin(t,*T_CL);th=K.TH_OPEN*(1-ease(k));f=K.PLATE.copy();K.draw_lid_th(f,th);g=1-prog(t,T_CL[0],T_CL[0]+0.3)
 if g>0:
  warped=END_PANEL.transform((W,H),Image.Transform.PERSPECTIVE,K.GLASS_COEF,Image.Resampling.BICUBIC);f.paste(warped,(0,0),Image.eval(K.GM,lambda v:int(v*g)))
 return f,{'scene':'close','lid':th,'glow':g}
sys.path.insert(0, str(Path(__file__).resolve().parents[3]/'assets'/'brand'/'jeffrit'));from endcard import EndCard
EC=EndCard(BOARD);_PREV=[]
def endcard(t):
 if not _PREV:_PREV.append(close(se('close')-1e-6)[0])
 f,st=EC.frame(t-ss('endcard'),prev=_PREV[0]);st=dict(st);st['scene']='endcard'
 for k,b,n in EC.boxes:K.LOG.append((k,tuple(float(x) for x in b),n))
 return f,st
# ---------------------------------------------------------------- frame (overrides the sample version)
SCN.update({'fix':lambda t:main_scene(t,'fix'),'recap':lambda t:main_scene(t,'recap'),'relay':relay_scene,'trade':trade_scene,'delay':delay_scene,'close':close,'endcard':endcard})
MAIN=('code','crash','rollback','dual','fix')
def frame(t,check_=True):
 K.LOG.clear();s=scene_of(t)
 f,st=(SCN[s](t) if s in SCN else flow_scene(t,s))
 a,b=ss(s),se(s);k=1.0
 prv=next((x for x,aa,bb in TL.SCENES if abs(bb-a)<1e-9),None);nxt=next((x for x,aa,bb in TL.SCENES if abs(aa-b)<1e-9),None)
 if prv is not None and s!='endcard' and not (prv in MAIN and s in MAIN) and t<a+FADE:k=lin(t,a,a+FADE)
 if nxt is not None and nxt!='endcard' and not (s in MAIN and nxt in MAIN) and t>b-FADE:k=min(k,1-lin(t,b-FADE,b))
 if k<1:f=Image.blend(BOARD,f,k)
 draw_captions(f,t)
 if check_:check_frame(t,s,st,list(K.LOG))
 return f
def check_frame(t,s,st,items):
 for kd,(x0,y0,x1,y1),n in items:
  if kd=='person':assert not LIMB_RE.search(str(n).lower()),(t,'limb asset drawn',n)
  assert -2<=x0<=x1<=W+2 and -2<=y0<=y1<=H+2,(t,kd,n,(x0,y0,x1,y1))
  if s!='endcard' and kd in ('code','card','label','person','obj','node','route','panel'):assert y1<1560,(t,'content enters caption band',kd,n,y1)
  if kd=='caption':assert 1570<=y0 and y1<=1780,(t,'caption outside safe band',n)
  if kd in ('label','code'):assert x0>=36 and x1<=W-36,(t,'text outside side margins',kd,n)
  if kd=='label':assert K.ALLOWED_TEXT.match(str(n)),(t,'label is not a number',n)
  if kd=='brand':assert n=='Jeffrit' and s=='endcard',(t,'brand text other than the Jeffrit wordmark',n)
  assert kd!='review',(t,'scene id / review text drawn',n)
 tx=[(kd,b,n) for kd,b,n in items if kd=='code']
 for i in range(len(tx)):
  for j in range(i+1,len(tx)):
   if abs(tx[i][1][1]-tx[j][1][1])>8:assert not overlap(tx[i][1],tx[j][1],-2),(t,'code rows overlap',tx[i],tx[j])
 if s in ('hook','laptop','code','crash','rollback','dual'):check_state(t,s,st)
 else:check_state2(t,s,st)
def check_state2(t,s,st):
 if s=='fix':
  assert set(st['visible'])<=set(GOOD.replace(' ','').replace('\n','')+SNIP.replace(' ','').replace('\n','')),(t,'unknown text on card')
  if st['move']>0:assert st['band']>=1 and st['order']>=1 and st['env']>=1,(t,'moved to the database before both were wrapped in one transaction')
  if st['ok']>0:assert st['landed'],(t,'commit check before both rows landed')
 if s=='relay':
  assert set(st['visible'])<=set(RELAY.replace(' ','').replace('\n','')),(t,'unknown text on card')
  if st['markA']>0:assert st['gotA'],(t,'marked sent before the broker had it')
  if st['markB']>0:assert st['gotB'],(t,'second row marked sent before the broker had it')
  assert st['em_after_got'],(t,'subscriber reacted before the broker had the event')
 if s=='trade':
  if st['rec1']>0:assert st['got1'],(t,'ledger recorded before delivery')
  if st['drop']>0:assert st['rec1']>=1 and st['arr2'],(t,'duplicate dropped before the ledger had the first copy')
  if st['pub2']:assert st['crack']<1 or t>TX['heal'][1],(t,'republish while the relay is down')
 if s=='delay':
  if st['mark']>0:assert st['got'],(t,'marked sent before delivery')
  if st['em']>0:assert st['got'],(t,'email before the broker had it')
  assert st['swept_sent_only'],(t,'the broom removed an unsent row')
 if s=='recap':
  assert set(st['visible'])<=set(GOOD.replace(' ','').replace('\n','')),(t,'unknown text on card')
  if st['em']>0 or st['inv']>0:assert st['got'],(t,'subscriber before the broker had it')
  if st['pub']:assert st['landed'] and st['ok']>=1,(t,'relay published before the commit')
  if st['mark']>0:assert st['got'],(t,'marked sent before delivery')
 if s=='endcard':
  if st['word']>0:assert st['pop']>=1,(t,'wordmark before the badge landed')
  if st['out']>0:assert st['underline']>=1 and st['word']>=1,(t,'fade-out before the logo finished')
_selftest1=selftest
def selftest():
 out=_selftest1()
 assert FT[1]<FX['order'] and FX['band'][1]<=FX['move'][0] and FX['land']<FX['ok']<FX['neither'],('fix order',FX)
 assert RCARD.done()<RX['poll']<RX['pubA']<RX['gotA']<RX['emA'][1]<RX['markA']<RX['down']<RX['tryB']<RX['bounce']<RX['wait']<RX['up']<RX['retry']<RX['gotB']<RX['markB']<se('relay')-0.05,('relay order',RX,se('relay'))
 assert TX['got1']<TX['rec1']<TX['crack']<TX['heal'][1]<TX['pub2']<TX['got2']<TX['con2'][1]<TX['twice']<TX['record']<TX['match'][0]<TX['drop'][1]<se('trade'),('trade order',TX)
 assert DX['new']<DX['tick'][1]<DX['pub']<DX['got']<DX['mark']<DX['late'] and DX['grow'][0]<DX['late']<DX['sweep'][0]<DX['sweep'][1]<se('delay'),('delay order',DX)
 assert RCX['order']<RCX['env']<RCX['band'][1]<=RCX['move'][0]<RCX['land']<RCX['ok']+0.3<RCX['pub']<RCX['got']<RCX['mark']<se('recap'),('recap order',RCX)
 assert T_CL[1]<se('close')-0.3 and 2.5<=se('endcard')-ss('endcard')<=3.0 and TOTAL<120,('close/endcard',T_CL,TOTAL)
 base={'scene':'relay','visible':'','gotA':False,'gotB':False,'markA':0.5,'markB':0,'em_after_got':True}
 fx=[('relay mark before broker',lambda:check_state2(53.0,'relay',base)),
  ('dup dropped before ledger',lambda:check_state2(70.0,'trade',{'rec1':0,'got1':True,'drop':0.3,'arr2':True,'pub2':True,'crack':0})),
  ('broom ate unsent',lambda:check_state2(77.0,'delay',{'mark':0,'got':False,'em':0,'swept_sent_only':False})),
  ('fix check before landing',lambda:check_state2(44.0,'fix',{'visible':'','move':0,'band':1,'order':1,'env':1,'ok':0.5,'landed':False})),
  ('recap relay before commit',lambda:check_state2(84.0,'recap',{'visible':'','em':0,'inv':0,'got':False,'pub':True,'landed':False,'ok':0,'mark':0})),
  ('word label',lambda:check_frame(50.0,'relay',{**base,'markA':0},[('label',(100,100,300,150),'OrderPlaced')])),
  ('foreign brand',lambda:check_frame(89.0,'endcard',{'word':1,'pop':1,'out':0,'underline':1},[('brand',(300,1200,700,1400),'Follow me')]))]
 for name,fn in fx:
  try:fn();raise SystemExit('NEGATIVE FIXTURE NOT CAUGHT: '+name)
  except AssertionError:out[name]='caught'
 out['times2']={'fix':FX,'relay':RX,'trade':TX,'delay':DX,'recap':RCX,'close':T_CL}
 return out
_events1=events
def events():
 e=_events1();e.pop('end',None)
 e.update({'cuts':[ss(x) for x in ('laptop','code','crash','rollback','dual','fix','relay','trade','delay','recap','close')],'fix':FX,'relay':RX,'relay_keys':[x for x,ch in zip(RCARD.T,RELAY) if not ch.isspace()],
  'good_keys':[x for x,ch in zip(GCARD.T,GOOD) if not ch.isspace()],'trade':TX,'delay':DX,'recap':RCX,'lid_close':T_CL,'endcard':EC.cues(ss('endcard')),'endcard_in':ss('endcard'),
  'music':{'calm':ss('crash'),'lift':ss('fix'),'end_fade':ss('close')}})
 return e

if __name__=='__main__':
 cmd=sys.argv[1] if len(sys.argv)>1 else 'stills'
 if cmd=='selftest':(P/'evidence/selftest.json').write_text(json.dumps(selftest(),indent=1));print('selftest ok')
 elif cmd=='events':(P/'events.json').write_text(json.dumps(events(),indent=1));print('events ok')
 elif cmd=='stills':
  for t in [float(x) for x in sys.argv[2:]]:frame(t).save(P/f'evidence/src-{t:06.2f}.png');print('saved',t)
 elif cmd=='checkall':
  for i in range(COUNT):frame(i/FPS)
  print('all',COUNT,'frames passed bounds/state/caption/label/limb checks')
 elif cmd=='render':render(P/'video-only.mp4')
