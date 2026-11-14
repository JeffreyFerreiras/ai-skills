"""Speech-recognition check on the FINAL MP4 audio (music + SFX in): per paragraph N01-N05 with 0.5 s padding and per sentence with 0.15 s padding;
faster-whisper small.en and base.en, beam 5 and greedy (4 decodes)."""
import sys,json,subprocess,numpy as np
from pathlib import Path
from faster_whisper import WhisperModel
F=sys.argv[1] if len(sys.argv)>1 else 'transactional-outbox-final-v1-master.mp4';SR=16000
y=np.frombuffer(subprocess.run(['ffmpeg','-loglevel','error','-i',F,'-f','f32le','-ac','1','-ar','16000','-'],capture_output=True,check=True).stdout,dtype='<f4').copy();dur=len(y)/SR
perf=json.load(open(sys.argv[2] if len(sys.argv)>2 else Path(__file__).resolve().parent/'narration'/'fenrir-performance.json'))['paragraphs']
ms={n:WhisperModel(n,device='cpu',compute_type='int8') for n in ('small.en','base.en')};out={'paragraphs':[],'sentences':[]}
def dec(a,b,pad):
 seg=y[max(0,int((a-pad)*SR)):int(min(dur,b+pad)*SR)];r=[]
 for n,m in ms.items():
  for bs in (5,1):r.append(' '.join(x.text.strip() for x in m.transcribe(seg,language='en',beam_size=bs,condition_on_previous_text=False)[0]))
 return r
for p in perf:
 if p['sentences'][-1]['end']>dur:continue
 out['paragraphs'].append({'para':p['para'],'heard':dec(p['sentences'][0]['start'],p['sentences'][-1]['end'],0.5)})
 for s in p['sentences']:out['sentences'].append({'sid':s['sid'],'text':s['text'],'heard':dec(s['start'],s['end'],0.15)})
json.dump(out,open('evidence/asr-final-mp4.json','w'),indent=1)
for k in ('paragraphs','sentences'):
 for o in out[k]:print(o.get('para',o.get('sid')),'|',' || '.join(o['heard']))
