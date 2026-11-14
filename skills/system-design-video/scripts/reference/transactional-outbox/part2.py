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
 if num is not None:text(d,(c[0],c[1]-13*s-4),str(num),int(26*s),dim(IV,alpha),True,kind='label',center=True)
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
RDB=(200,1330);RDBW=120;RDBH=190;GEAR=(455,1330);GR_R=56;RBOX=(575,1250,795,1410);REM=(950,1210);RIN=(950,1450);RNR=56
TRAY_C=[(RDB[0]+18+(i%2)*52,RDB[1]-40+(i//2)*44) for i in range(6)]   # outbox rows (two columns inside the tray half)
P_TG=[(RDB[0]+RDBW-2,RDB[1]),(GEAR[0]-GR_R-4,GEAR[1])];P_GB=[(GEAR[0]+GR_R+4,GEAR[1]),(RBOX[0]-2,GEAR[1])]
P_BE=[(RBOX[2]+2,RBOX[1]+40),(REM[0]-RNR-4,REM[1]+10)];P_BI=[(RBOX[2]+2,RBOX[3]-40),(RIN[0]-RNR-4,RIN[1]-10)]
def trip(t,a,src):  # one message copy: tray row -> gear -> broker, positions along the joined path
 k=lin(t,a,a+0.62);pts=[src]+P_TG[1:]+P_GB;return (along(pts,k) if 0<k<1 else None),k
def relay_base(d,t,tray,alpha=1.0,gear_ang=0.0,crack=0.0,down=0.0,got=0.0,em=0.0,inv=0.0,inv_lift=0.0):
 outbox_db(d,RDB,RDBW,RDBH,alpha);gear(d,GEAR,GR_R,gear_ang,alpha,crack);broker_box(d,RBOX,alpha,down,got)
 node2(d,REM,RNR,em,'email',alpha);node2(d,RIN,RNR,inv,'inventory',alpha,box_lift=inv_lift)
 for p in (P_TG,P_GB,P_BE,P_BI):arrow(d,p,1.0,dim((90,100,120),alpha*0.8),3)
 for i,(num,sent,a_) in enumerate(tray):
  if a_>0.02:num_env(d,TRAY_C[i],0.62,None,a_*alpha,VI if sent<1 else dim(GR,0.8),sent)
RX={'poll':RCARD.done()+0.1}
RX['pubA']=max(WT('N07','publishes')+0.05,RX['poll']+0.5);RX['gotA']=RX['pubA']+0.62;RX['emA']=(RX['gotA']+0.05,RX['gotA']+0.4);RX['inA']=(RX['gotA']+0.15,RX['gotA']+0.5)
RX['markA']=max(WT('N07','marks'),RX['inA'][1]+0.1);RX['down']=WT('N07','Broker');RX['tryB']=RX['down']+0.45;RX['bounce']=RX['tryB']+0.62
RX['wait']=WT('N07','waits');RX['up']=WT('N07','relay',1)-0.35;RX['retry']=WT('N07','tries');RX['gotB']=RX['retry']+0.62;RX['emB']=(RX['gotB']+0.03,RX['gotB']+0.3)
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
 relay_base(d,t,[(1,st['markA'],1.0),(2,st['markB'],1.0)],1.0,ang,0,st['down'],0,st['em'],st['inv'],st['inv'])
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
 relay_base(d,t,[(7,0.0,1.0)],1.0,ang,st['crack'],0,0,0,0)
 arrow(d,P_BC,1.0,dim((90,100,120),0.8),3)
 node2(d,TCON,TCON_R,0,'email',1.0,ring=bump(t,TX['idem'],TX['idem']+1.0))
 rows=[5,6,7] if st['rec1']>0 else [5,6]
 ledger(d,LEDGER,rows,1.0,hi=2 if st['rec1']>0 else None,hi_k=max(bump(t,TX['rec1'],TX['rec1']+0.6),bump(t,TX['record'],TX['record']+1.1),st['match'] if st['drop']<1 else 0),new_k=st['rec1'] if st['rec1']>0 else 1.0,
  check_k=prog(t,TX['record']+0.2,TX['record']+0.5))
 for a,src in ((TX['pub1'],TRAY_C[0]),(TX['pub2'],TRAY_C[0])):
  p,k=trip(t,a,src)
  if p:num_env(d,p,0.75,7)
 for a,b in (TX['con1'],TX['con2']):
  if a<=t<b:num_env(d,along(P_BC,lin(t,a,b)),0.75,7)
 if TX['con1'][1]<=t<TX['rec1']+0.3:   # first copy handled: it goes into the ledger
  k=prog(t,TX['con1'][1],TX['rec1']+0.3);num_env(d,mix((TCON[0],TCON[1]-TCON_R-40),(LEDGER[0]+70,LEDGER[1]+96+2*86+20),k),0.75,7,1-k)
 if st['arr2'] and st['drop']<1:   # the duplicate waits beside the consumer, then matches row 7 and is skipped
  c=(TCON[0]-TCON_R-70,TCON[1]-20);a=1-st['drop'];num_env(d,c,0.9,7,a)
  tw=prog(t,TX['twice'],TX['twice']+0.3)
  if tw>0:text(d,(c[0]-56,c[1]-110),'×2',46,dim(AM,tw*a),True,kind='label')
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
 tray=[(0,st['mark'],prog(t,DX['new'],DX['new']+0.25))]
 for i in range(4):tray.append((0,1.0,1.0 if i<nb else 0.0))
 tray.append((0,0.0,prog(t,DX['late'],DX['late']+0.25)))   # one new unsent row arrives: it must survive the sweep
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
 assert DX['new']<DX['tick'][1]<DX['pub']<DX['got']<DX['mark']<DX['grow'][0]<DX['late']<DX['sweep'][0]<DX['sweep'][1]<se('delay'),('delay order',DX)
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
