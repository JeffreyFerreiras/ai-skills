"""Measure sentence/paragraph pauses in the FINAL MP4 audio (music + SFX present), two ways:
1) Silero VAD (faster-whisper bundled, threshold 0.5, no speech padding, 30 ms windows): gap between speech regions around each boundary.
2) Voice-band envelope: 250-4000 Hz, 10 ms RMS; gate = gap floor + 8 dB (floor = median level in boundary centres, i.e. ducked music/SFX).
Boundaries come from narration-fenrir-v2/fenrir-performance.json (sentence takes). Also per-sentence ASR on the MP4 (no padding)."""
import json,subprocess,sys,re,difflib,numpy as np
from pathlib import Path
from faster_whisper.vad import get_speech_timestamps,VadOptions
from faster_whisper import WhisperModel
F=sys.argv[1] if len(sys.argv)>1 else 'transactional-outbox-final-v1-master.mp4';SR=16000
y=np.frombuffer(subprocess.run(['ffmpeg','-v','error','-i',F,'-f','f32le','-ac','1','-ar',str(SR),'-'],capture_output=True,check=True).stdout,'<f4').copy()
dur=len(y)/SR
perf=json.load(open(sys.argv[2] if len(sys.argv)>2 else Path(__file__).resolve().parent/'narration'/'fenrir-performance.json'))
S=[s|{'para':p['para']} for p in perf['paragraphs'] for s in p['sentences'] if s['end']<dur]
B=[(a,b) for a,b in zip(S,S[1:])]
vs=get_speech_timestamps(y,VadOptions(threshold=0.5,min_silence_duration_ms=60,speech_pad_ms=0,min_speech_duration_ms=60))
vs=[(v['start']/SR,v['end']/SR) for v in vs]
Y=np.fft.rfft(y);f=np.fft.rfftfreq(len(y),1/SR);Y[(f<250)|(f>4000)]=0;yb=np.fft.irfft(Y,len(y))
hop=int(0.01*SR);n=len(yb)//hop;lv=20*np.log10(np.sqrt((yb[:n*hop].reshape(n,hop)**2).mean(1))+1e-9)
mids=[(a['end']+b['start'])/2 for a,b in B]
floor=float(np.median([lv[int(m*100)-5:int(m*100)+5].mean() for m in mids]));gate=floor+8
sp_lv=float(np.median(np.concatenate([lv[int(s['start']*100)+15:int(s['end']*100)-15] for s in S])))
out=[]
for (a,b),m in zip(B,mids):
  # VAD: region ending before m and starting after m
  e=max([v[1] for v in vs if v[1]<=m+0.05] or [np.nan]);s0=min([v[0] for v in vs if v[0]>=m-0.05] or [np.nan])
  i=int(m*100);j=i
  while i>0 and lv[i]<gate:i-=1
  while j<n-1 and lv[j]<gate:j+=1
  out.append({'after':a['sid'],'kind':'paragraph' if a['para']!=b['para'] else 'sentence','stem_s':next(x['silence_s'] for x in perf['boundaries'] if x['after']==a['sid']),
   'mp4_vad_s':round(s0-e,3),'mp4_envelope_s':round((j-i-1)/100,3)})
m=WhisperModel('small.en',device='cpu',compute_type='int8');asr=[]
def nm(t):return re.sub(r'[^a-z0-9 ]','',t.lower().replace('-',' ')).split()
for s in S:
  seg=y[int((s['start']+0.02)*SR):int((s['end']+0.04)*SR)];seg=np.r_[np.zeros(4000,'f4'),seg,np.zeros(4000,'f4')]
  r=' '.join(x.text.strip() for x in m.transcribe(seg,language='en',beam_size=5,condition_on_previous_text=False)[0])
  asr.append({'sid':s['sid'],'text':s['text'],'heard':r})
def st(k,kind):
  v=[o[k] for o in out if o['kind']==kind];return {'n':len(v),'min':min(v),'median':float(np.median(v)),'max':max(v)}
rep={'file':F,'duration_s':round(dur,3),'envelope_floor_db':round(floor,1),'envelope_gate_db':round(gate,1),'speech_level_db':round(sp_lv,1),
 'summary':{k:{kind:st(k,kind) for kind in ('sentence','paragraph')} for k in ('stem_s','mp4_vad_s','mp4_envelope_s')},'boundaries':out,'sentence_asr':asr}
json.dump(rep,open('evidence/pauses-final-mp4.json','w'),indent=1)
for o in out:print(o)
print(json.dumps(rep['summary'],indent=0),'floor',round(floor,1),'gate',round(gate,1),'speech',round(sp_lv,1))
for a in asr:print(a['sid'],'|',a['heard'])
# words per minute measured on the MP4: spoken words / (first to last VAD speech) and / file duration
nw=sum(len(s['text'].split()) for s in S);sp0=min(v[0] for v in vs);sp1=max(v[1] for v in vs);spk=sum(b-a for a,b in vs)
w={'words':nw,'speech_span_s':round(sp1-sp0,3),'wpm_span':round(nw/(sp1-sp0)*60,1),'wpm_file':round(nw/dur*60,1),'vad_speech_s':round(spk,3),'wpm_articulation':round(nw/spk*60,1),'first_word_s':round(sp0,3),'last_word_end_s':round(sp1,3)}
rep['wpm']=w;json.dump(rep,open('evidence/pauses-final-mp4.json','w'),indent=1);print(w)
import re as _r
print('acronym hits in sentence STT:',[a for a in asr if _r.search(r'\b(e\.?\s?f\.?|ef|db|api)\b',a['heard'],_r.I)])
