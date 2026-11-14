"""Transactional-outbox final-v1 timeline: Fenrir performance played 1:1 (0 to the end of "That's the dual write problem."),
word times from narration/words.json (made monotonic), captions from the spoken words. Scene names are internal only, never drawn.
narration/ is the episode take directory beside this script. The WAV, words, and performance JSON are not bundled."""
import json,re
from pathlib import Path
ROOT=Path(__file__).resolve().parent;NAR=ROOT/'narration'
FPS=24;PARAS=['N01','N02','N03','N04','N05','N06','N07','N08','N09','N10']
PERF=json.load(open(NAR/'fenrir-performance.json'));_p={p['para']:p for p in PERF['paragraphs']}
_W={p['para']:p for p in json.load(open(NAR/'words.json'))}
NARR_END=_p['N10']['end']+0.06
def key(w):return re.sub(r'[^a-z0-9]','',w.lower())
WORDS={}
for pid in PARAS:
  ws=[];prev_s=_p[pid]['start']+0.05;prev_e=prev_s
  for w in _W[pid]['words']:
    s_,e=w['start'],w['end']
    if s_<prev_s:s_=prev_e      # Whisper gave a few out-of-order starts; keep words monotonic
    e=max(e,s_+0.08);ws.append({'w':w['full'],'tok':w['w'],'start':round(s_,3),'end':round(e,3)});prev_s,prev_e=s_,e
  m=[]
  for x in ws:
    if m and '-' in x['w'] and m[-1]['w']==x['w']:m[-1]['end']=x['end'];continue
    m.append(x)
  WORDS[pid]=m
def wt(pid,word,nth=0):
  hits=[w for w in WORDS[pid] if key(w['tok'])==key(word) or key(w['w'])==key(word)];return hits[nth]['start']
def wend(pid,word,nth=0):
  hits=[w for w in WORDS[pid] if key(w['tok'])==key(word) or key(w['w'])==key(word)];return hits[nth]['end']
def sent(sid):return next(s for p in PERF['paragraphs'] for s in p['sentences'] if s['sid']==sid)
_b1=round(sent('N01S3')['start']-0.30,3)    # customer -> laptop
_b2=round(sent('N02S1')['start']-0.30,3)    # laptop -> code + flow
_b3=round(sent('N04S1')['start']-0.35,3)    # two writes -> crash replay
_b4=round(sent('N04S4')['start']-0.30,3)    # crash -> publish-first replay
_b5=round(sent('N05S1')['start']-0.35,3)    # -> dual write
_b6=round(sent('N06S1')['start']-0.35,3)    # -> the fix: outbox in the same transaction
_b7=round(sent('N07S1')['start']-0.35,3)    # -> the relay
_b8=round(sent('N08S1')['start']-0.35,3)    # -> trade-offs: duplicates
_b9=round(sent('N09S1')['start']-0.35,3)    # -> delay and cleanup
_b10=round(sent('N10S1')['start']-0.35,3)   # -> rule of thumb recap
_b11=round(NARR_END+0.45,3)                 # -> laptop closes by itself
CLOSE_LEN=1.55;END_LEN=2.9                  # Jeffrit end card after the laptop closes (crossfade in, no hard cut)
CL_END=int(round((_b11+CLOSE_LEN)*FPS))/FPS;COUNT=int(round(CL_END*FPS+END_LEN*FPS));TOTAL=COUNT/FPS
SCENES=[('hook',0.0,_b1),('laptop',_b1,_b2),('code',_b2,_b3),('crash',_b3,_b4),('rollback',_b4,_b5),('dual',_b5,_b6),('fix',_b6,_b7),
 ('relay',_b7,_b8),('trade',_b8,_b9),('delay',_b9,_b10),('recap',_b10,_b11),('close',_b11,CL_END),('endcard',CL_END,TOTAL)]
def captions(maxw=7,maxc=34):
  caps=[]
  for pid in PARAS:
    ws=WORDS[pid];phr=[];cur=[]
    for w in ws:
      cur.append(w)
      if w['w'][-1] in ',.?!:':phr.append(cur);cur=[]
    if cur:phr.append(cur)
    sp=[]
    for p in phr:
      while len(p)>maxw or (len(' '.join(x['w'] for x in p))>maxc and len(p)>2):
        def cost(i):
          c=abs(i-len(p)/2)
          if key(p[i]['w']) in ('and','so','for','with','not','but','into','on','to','then','has'):c-=1.6
          if key(p[i]['w']) in ('framework','core','placed','broker'):c+=5
          return c
        k=min(range(2,len(p)-1),key=cost);sp.append(p[:k]);p=p[k:]
      sp.append(p)
    merged=[]
    for p in sp:
      if merged and len(merged[-1])+len(p)<=maxw and len(' '.join(x['w'] for x in merged[-1]+p))<=maxc and (merged[-1][-1]['w'][-1] not in '.?,' or (len(p)==1 and merged[-1][-1]['w'][-1]==',')) and (len(merged[-1])<3 or len(p)<3):merged[-1]=merged[-1]+p
      else:merged.append(p)
    for p in merged:caps.append({'start':p[0]['start'],'end':p[-1]['end'],'text':' '.join(x['w'] for x in p),'words':p,'para':pid})
  for c,nx in zip(caps,caps[1:]+[None]):
    c['end']=round(nx['start'] if nx is not None and nx['start']-c['end']<0.9 else c['end']+0.45,3)
  return caps
if __name__=='__main__':
  print(COUNT,TOTAL,SCENES)
  for c in captions():print(round(c['start'],2),round(c['end'],2),c['text'])
