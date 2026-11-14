"""Master premaster.wav to -14 LUFS integrated, true peak <= -2 dBTP, and mux with video-only.mp4 into the ONE clean output.
Measures the final AAC inside the MP4 (ebur128 true peak) and iterates the gain."""
import subprocess,re,json
from pathlib import Path
P=Path(__file__).parent;OUT=P/'transactional-outbox-final-v1-master.mp4';VID=P/'video-only.mp4';PRE=P/'audio/premaster.wav'
def measure(f):
 r=subprocess.run(['ffmpeg','-hide_banner','-nostdin','-i',str(f),'-map','0:a','-af','ebur128=peak=true','-f','null','-'],capture_output=True,text=True).stderr
 s=r[r.rindex('Summary:'):];I=float(re.search(r'I:\s+(-?[\d.]+) LUFS',s).group(1));TP=float(re.search(r'Peak:\s+(-?[\d.]+) dBFS',s).group(1));LRA=float(re.search(r'LRA:\s+(-?[\d.]+) LU',s).group(1));return I,TP,LRA
g=0.0;I0,_,_=measure(PRE);g=-14.0-I0;hist=[]
for it in range(10):
 af=f'volume={g:.3f}dB,aresample=192000,alimiter=limit=0.70:attack=1:release=60:level=disabled,aresample=48000'
 subprocess.run(['ffmpeg','-hide_banner','-nostdin','-loglevel','error','-y','-i',str(VID),'-i',str(PRE),'-map','0:v','-map','1:a','-c:v','copy','-af',af,'-c:a','aac','-b:a','192k','-ar','48000','-shortest','-movflags','+faststart',str(OUT)],check=True)
 I,TP,LRA=measure(OUT);hist.append({'gain_db':round(g,3),'I':I,'TP':TP,'LRA':LRA});print(hist[-1],flush=True)
 if abs(I+14.0)<=0.05 and TP<=-2.0:break
 g+=(-14.0-I)*(1.0 if it<3 else 1.5)
 if TP>-2.0:print('true peak over; limiter will hold at 0.70 so retry')
json.dump({'file':OUT.name,'iterations':hist},open(P/'evidence/loudness.json','w'),indent=1)
