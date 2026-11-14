"""Transactional-outbox final-v1 sound: Fenrir narration (N01-N10) + light music bed + soft, high SFX at 48 kHz.
No rumble and no low buzzy textures. Every SFX cue is high-passed at 300 Hz; the music bed at 230 Hz.
The music helpers come from the earlier thread-pool sound module via CHALK_SOUND_HELPERS. That module is not bundled.
The bed swells into the end card, then hands over to a C add9 chord above 230 Hz that reaches digital silence 60 ms before the file ends."""
import json,subprocess,importlib.util,wave
from pathlib import Path
import numpy as np
import timeline as TL
P=Path(__file__).parent;AU=P/'audio';AU.mkdir(exist_ok=True)
import os
helper=os.environ.get('CHALK_SOUND_HELPERS')
if not helper: raise SystemExit('Set CHALK_SOUND_HELPERS to build_sound.py from the earlier thread-pool sound module. That helper is not bundled.')
spec=importlib.util.spec_from_file_location('tp',helper);tp=importlib.util.module_from_spec(spec);spec.loader.exec_module(tp)
SR=48000;TOTAL=TL.TOTAL;N=int(round(TOTAL*SR));tp.DURATION_S=TOTAL+4.0;tp.NO_SFX=[]
EV=json.loads((P/'events.json').read_text())
def readf(p):return np.frombuffer(subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-i',str(p),'-f','f32le','-ac','1','-ar','48000','-af','aresample=resampler=soxr','-'],capture_output=True,check=True).stdout,dtype='<f4').astype(np.float64)
perf=readf(TL.NAR/'fenrir-performance.wav');cut=int(round(TL.NARR_END*SR));narr=np.zeros(N);seg=perf[:cut].copy();f=int(0.01*SR);seg[-f:]*=np.linspace(1,0,f);narr[:len(seg)]=seg
HP_SFX=300.0;HP_MUSIC=230.0
def hp(sig,lo=HP_SFX):return np.column_stack([tp.fft_bandpass(sig[:,c],SR,lo,20000) for c in range(sig.shape[1])])
music=tp.make_music(N+4*SR,SR)[:N];music=hp(music,HP_MUSIC)
fi=int(1.2*SR);music[:fi]*=np.linspace(0,1,fi)[:,None]
env=np.ones(N)
def ramp(t0,t1,v0,v1):i0,i1=int(t0*SR),int(t1*SR);env[i0:i1]=np.linspace(v0,v1,i1-i0);env[i1:]=v1
c0=EV['music']['calm'];ramp(c0,c0+1.0,1.0,0.8)          # thinner under the failure replays
ramp(EV['d_dual']-0.5,EV['d_dual']+0.4,0.8,1.0)            # lifts for the name of the problem
ramp(EV['music']['lift'],EV['music']['lift']+1.0,1.0,1.05) # the fix
ECV=EV['endcard'];T_SWELL=ECV['endcard_in']-0.4;T_RES=ECV['pop']
ramp(EV['music']['end_fade'],EV['music']['end_fade']+0.6,1.05,0.85)
ramp(T_SWELL,T_RES,0.85,1.7)                               # swell under the laptop-to-logo crossfade (narration has ended)
i0,i1=int(T_RES*SR),int((T_RES+0.7)*SR);env[i0:i1]*=0.5+0.5*np.cos(np.linspace(0,np.pi,i1-i0));env[i1:]=0.0   # bed hands over to the final chord
music*=env[:,None];speak=tp.speech_amount(narr);LA=int(0.25*SR);speak=np.maximum(speak,np.r_[speak[LA:],np.zeros(LA)])
music,under_db=tp.duck_music(music,speak,narr)
def final_chord():
 n=N-int(T_RES*SR);t=np.arange(n)/SR;y=np.zeros(n)
 for f_,a_,dc in ((261.63,0.30,0),(329.63,0.24,3),(392.0,0.22,-3),(523.25,0.16,2),(587.33,0.10,-2)):
  for det in (-dc,dc):y+=a_*0.5*np.sin(2*np.pi*f_*2**(det/1200)*t)
 y*=(1-np.exp(-t/0.06))*np.exp(-t/0.95)
 for f_,a_ in ((1046.5,0.10),(1568.0,0.06),(2093.0,0.035)):y+=a_*np.sin(2*np.pi*f_*t)*np.exp(-t/0.5)*(1-np.exp(-t/0.004))
 tail=int(0.9*SR);y[-tail:]*=0.5+0.5*np.cos(np.linspace(0,np.pi,tail));z=int(0.06*SR);y[-z:]=0.0
 st=np.column_stack([y,np.r_[np.zeros(int(0.012*SR)),y[:-int(0.012*SR)]]*0.94+0.06*y])
 return hp(st,HP_MUSIC)
chord=final_chord();rms=lambda v:float(np.sqrt(np.mean(v**2)))
chord*=1.5*rms(music[int((T_RES-0.3)*SR):int(T_RES*SR)])/rms(chord[int(0.1*SR):int(0.5*SR)])
music[int(T_RES*SR):]+=chord;music[-int(0.06*SR):]=0.0
sfx=np.zeros((N,2));cues=[]
def cue(name,t,sig,gain):
 if t<0 or t>=TOTAL-0.05:return
 sig=hp(sig);tp.mix_at(sfx,sig,t,gain);cues.append({'id':name,'t':round(t,3),'gain':gain})
def click(seed,f0=2400,dur=0.035):
 r=np.random.default_rng(seed);n=int(SR*dur);t=np.arange(n)/SR
 s=tp.fft_bandpass(r.standard_normal(n),SR,f0*0.6,f0*1.8)*np.exp(-t/0.006)+0.4*np.sin(2*np.pi*f0*0.5*t)*np.exp(-t/0.01)
 return tp.to_stereo(tp.fade_edges(s,SR,0.001,0.01),float(r.uniform(-0.25,0.25)))
def lid_speed(n):k=np.linspace(0,1,n);e=1-(1-k**1.6)**2.6;v=np.gradient(e);return v/v.max()
def swish(dur,seed,lo=2200,hi=5200,shape=None):
 r=np.random.default_rng(seed);n=int(SR*dur);t=np.arange(n)/SR;x=r.standard_normal(n)
 y=tp.fft_bandpass(x,SR,lo,hi)*(lid_speed(n)**1.3 if shape is None else shape(t/dur));y/=np.abs(y).max();return tp.to_stereo(tp.fade_edges(y,SR,0.02,0.06),-0.08)
def tight_click(seed):
 r=np.random.default_rng(seed);n=int(SR*0.06);t=np.arange(n)/SR;snap=tp.fft_bandpass(r.standard_normal(n),SR,3000,10000)*np.exp(-t/0.0012)
 tick=np.sin(2*np.pi*3150*t)*np.exp(-t/0.004)*0.55+np.sin(2*np.pi*5620*t)*np.exp(-t/0.012)*0.12;y=snap/np.abs(snap).max()+tick;y/=np.abs(y).max()
 return tp.to_stereo(tp.fade_edges(y,SR,0.0005,0.01),0.05)
def bell(freqs,dur=0.9):
 n=int(SR*dur);t=np.arange(n)/SR;y=np.zeros(n)
 for j,f_ in enumerate(freqs):d=0.045*j;i=int(d*SR);tt=t[:n-i];y[i:]+=(0.55**j)*np.sin(2*np.pi*f_*tt)*np.exp(-tt/(0.4-0.08*j))*(1-np.exp(-tt/0.012))
 y/=np.abs(y).max();return tp.to_stereo(tp.fade_edges(y,SR,0.004,0.12),0.0)
def soft_nope():
 n=int(SR*0.42);t=np.arange(n)/SR;y=np.zeros(n)
 for f_,d in ((987.77,0.0),(783.99,0.13)):
  i=int(d*SR);tt=t[:n-i];g_=np.exp(-tt/0.11)*(1-np.exp(-tt/0.006))
  if d==0.0:g_*=np.clip((0.14-tt)/0.02,0,1)
  y[i:]+=np.sin(2*np.pi*f_*tt)*g_
 y/=np.abs(y).max();return tp.to_stereo(tp.fade_edges(y,SR,0.003,0.05),0.0)
def air(dur,seed):return swish(dur,seed,3000,8000,lambda u:np.sin(np.pi*np.clip(u,0,1))**1.5)
def glitch(seed):  # app crash: short bright digital stutter, high-passed (no thump)
 r=np.random.default_rng(seed);n=int(SR*0.32);t=np.arange(n)/SR;y=np.zeros(n)
 for k in range(6):
  i=int(k*0.045*SR);m=int(0.03*SR);y[i:i+m]+=tp.fft_bandpass(r.standard_normal(m),SR,1800+400*k,7000)*(0.85**k)
 y/=np.abs(y).max();return tp.to_stereo(tp.fade_edges(y,SR,0.001,0.04),0.0)
def poof(seed):return swish(0.5,seed,4000,9000,lambda u:(1-np.clip(u,0,1))**2)
E_=EV
# hook
cue('order_pop',E_['order_pop'],tp.join_pop(SR,0.18,1),0.05);cue('saved',E_['saved']+0.05,bell((1318.51,1760.0),0.5),0.04)
cue('env_try',E_['env_try'],air(0.8,2),0.035);cue('env_fail',E_['env_fail'],soft_nope(),0.045)
# v1 final: no scene-cut whooshes (every cut sits inside a sentence or paragraph pause)
a,b=E_['lid_open'];cue('lid_swish',a+0.05,swish(b-a-0.05,3),0.07);cue('lid_click',E_['lid_click'],tight_click(4),0.16)
cue('screen_on',E_['screen_on'],bell((1318.51,1975.53,2637.02),1.0),0.04)
for i,t in enumerate(E_['logs']):
 if i%2==0:cue(f'log{i}',t,click(40+i,3800,0.012),0.016)
# code
for i,t in enumerate(E_['keys']):
 if i%2==0:cue(f'key{i}',t,click(1000+i,3200,0.02),0.022)
cue('nodes',E_['nodes'],tp.pen_scratch(SR,0.35,60),0.035)
cue('save',E_['save'][0],air(E_['save'][1]-E_['save'][0]+0.2,61),0.04);cue('db_ok',E_['db_ok'],tp.join_pop(SR,0.16,62),0.05)
cue('pub',E_['pub'][0],air(E_['pub'][1]-E_['pub'][0]+0.2,63),0.04);cue('broker',E_['broker'],tp.join_pop(SR,0.16,64),0.05)
cue('em',E_['em'][1],bell((1318.51,1760.0),0.5),0.035);cue('inv',E_['inv'][1],bell((1174.66,1567.98),0.5),0.035)
cue('two',E_['two']+0.55,tp.highlight(SR,0.12,70),0.04)   # after the short sentence, so the soft 'Two' onset stays clear
for k_ in ('one1','one2'):cue(k_,E_[k_]+0.25,tp.highlight(SR,0.12,70+len(cues)),0.04)
cue('string',E_['string'],tp.pen_scratch(SR,0.45,75),0.04);cue('snap',E_['snap'],tight_click(76),0.10)
# crash
cue('c_save',E_['c_save'][0],air(0.7,80),0.04);cue('c_dbok',E_['c_dbok'],tp.join_pop(SR,0.16,81),0.05)
cue('c_crash',E_['c_crash'],glitch(82),0.06);cue('c_down',E_['c_down'],soft_nope(),0.05);cue('c_commit',E_['c_commit'],tp.highlight(SR,0.12,83),0.04)
cue('c_gone',E_['c_gone'],poof(84),0.05);cue('c_email',E_['c_email'],tp.pen_scratch(SR,0.3,85),0.03);cue('c_inv',E_['c_inv'],tp.pen_scratch(SR,0.3,86),0.03)
# rollback
cue('r_swap',E_['r_swap'][0],swish(0.5,90,3000,7000,lambda u:np.sin(np.pi*np.clip(u,0,1))),0.04)
cue('r_pub',E_['r_pub'][0],air(0.6,91),0.04);cue('r_broker',E_['r_broker'],tp.join_pop(SR,0.16,92),0.045)
cue('r_em',E_['r_em'][1],bell((1318.51,1760.0),0.4),0.03);cue('r_in',E_['r_in'][1],bell((1174.66,1567.98),0.4),0.03)
cue('r_save',E_['r_save'][0],air(0.5,93),0.035);cue('r_rb',E_['r_rb'][0],soft_nope(),0.05);cue('r_ghost',E_['r_ghost'][0],tp.pen_scratch(SR,0.5,94),0.035)
cue('r_exist',E_['r_exist'],tp.highlight(SR,0.12,95),0.045)
# dual
cue('d_unswap',E_['d_unswap'][0],swish(0.4,96,3000,7000,lambda u:np.sin(np.pi*np.clip(u,0,1))),0.035)
cue('d_dual',E_['d_dual'],tp.highlight(SR,0.12,97),0.045);cue('d_split',E_['d_split'][0],tight_click(98),0.10)
# ---------------- the fix (N06)
F_=E_['fix'];cue('f_tray',F_['tray'][0],tp.pen_scratch(SR,0.4,300),0.04);cue('f_dont',F_['dont'],soft_nope(),0.045)
for i,t in enumerate(E_['good_keys']):
 if i%2==0:cue(f'gkey{i}',t,click(2000+i,3200,0.02),0.022)
cue('f_order',F_['order'],tp.join_pop(SR,0.16,301),0.045);cue('f_env',F_['env'],tp.join_pop(SR,0.16,302),0.045);cue('f_band',F_['band'][0],tp.pen_scratch(SR,0.45,303),0.04)
cue('f_move',F_['move'][0],air(F_['move'][1]-F_['move'][0]+0.15,304),0.04);cue('f_land',F_['land'],tp.join_pop(SR,0.18,305),0.05);cue('f_ok',F_['ok'],bell((1318.51,1760.0,2637.02),0.8),0.04)
cue('f_both',F_['neither']-0.14,tp.highlight(SR,0.12,306),0.04);cue('f_neither',F_['neither']+0.1,soft_nope(),0.045)
# ---------------- the relay (N07)
R_=E_['relay']
for i,t in enumerate(E_['relay_keys']):
 if i%3==0:cue(f'rkey{i}',t,click(3000+i,3200,0.018),0.02)
cue('r_poll',R_['poll'],tight_click(310),0.07);cue('rl_pubA',R_['pubA'],air(0.7,311),0.04);cue('rl_gotA',R_['gotA'],tp.join_pop(SR,0.16,312),0.045)
cue('rl_emA',R_['emA'][1],bell((1318.51,1760.0),0.45),0.03);cue('rl_inA',R_['inA'][1],bell((1174.66,1567.98),0.45),0.03);cue('rl_markA',R_['markA'],tight_click(313),0.07)
cue('rl_down',R_['down']-0.02,soft_nope(),0.045);cue('rl_tryB',R_['tryB'],air(0.6,314),0.035);cue('rl_bounce',R_['bounce'],click(315,1800,0.05),0.05)
cue('rl_wait',R_['wait'],tp.highlight(SR,0.12,316),0.035);cue('rl_up',R_['up'],bell((1567.98,2093.0),0.4),0.03);cue('rl_retry',R_['retry'],air(0.7,317),0.04)
cue('rl_gotB',R_['gotB'],tp.join_pop(SR,0.16,318),0.045);cue('rl_emB',R_['emB'][1],bell((1318.51,1760.0),0.4),0.03);cue('rl_markB',R_['markB'],tight_click(319),0.06)
# ---------------- trade-offs (N08)
T_=E_['trade'];cue('t_pub1',T_['pub1'],air(0.7,320),0.04);cue('t_con1',T_['con1'][0],air(0.5,321),0.03);cue('t_rec1',T_['rec1'],tp.pen_scratch(SR,0.3,322),0.04)
cue('t_crack',T_['crack'],glitch(323),0.055);cue('t_heal',T_['heal'][1],bell((1567.98,2093.0),0.4),0.03);cue('t_pub2',T_['pub2'],air(0.7,324),0.04)
cue('t_con2',T_['con2'][0],air(0.5,325),0.03);cue('t_arr2',T_['con2'][1],tp.join_pop(SR,0.16,326),0.045);cue('t_twice',T_['twice']+1.0,tp.highlight(SR,0.12,327),0.04)
cue('t_idem',T_['idem']+0.1,tp.highlight(SR,0.12,328),0.035);cue('t_record',T_['record']+0.25,tight_click(329),0.06);cue('t_drop',T_['drop'][0],soft_nope(),0.045)
# ---------------- delay and cleanup (N09)
D_=E_['delay'];cue('d_new',D_['new'],tp.join_pop(SR,0.16,330),0.04)
a_,b_=D_['tick']
for i in range(5):cue(f'd_tick{i}',a_+(b_-a_)*i/5,click(331+i,4200,0.015),0.035)
cue('d_pub',D_['pub'],air(0.7,340),0.04);cue('d_em',D_['em'][1],bell((1318.51,1760.0),0.45),0.03);cue('d_mark',D_['mark'],tight_click(341),0.06)
a_,b_=D_['grow']
for i in range(4):cue(f'd_grow{i}',a_+(b_-a_)*(i+0.5)/4,tp.join_pop(SR,0.14,342+i),0.035)
a_,b_=D_['sweep'];cue('d_sweep',a_,swish(b_-a_,350,2500,6500,lambda u:np.sin(np.pi*np.clip(u,0,1))),0.045)
# ---------------- recap (N10)
C_=E_['recap'];cue('rc_order',C_['order'],tp.join_pop(SR,0.16,360),0.04);cue('rc_env',C_['env'],tp.join_pop(SR,0.16,361),0.04);cue('rc_svc',C_['svc'],tp.highlight(SR,0.12,362),0.035)
cue('rc_band',C_['band'][0],tp.pen_scratch(SR,0.4,363),0.04);cue('rc_move',C_['move'][0],air(0.6,364),0.04);cue('rc_ok',C_['ok'],bell((1318.51,1760.0,2637.02),0.8),0.04)
cue('rc_gear',C_['gear'],tight_click(365),0.06);cue('rc_pub',C_['pub'],air(0.7,366),0.04);cue('rc_em',C_['em'][1],bell((1318.51,1760.0),0.45),0.03);cue('rc_inv',C_['inv'][1],bell((1174.66,1567.98),0.45),0.03)
cue('rc_mark',C_['mark'],tight_click(367),0.05)
# ---------------- close + Jeffrit end card
a_,b_=E_['lid_close'];cue('lid_close_swish',a_,swish(b_-a_,1310,2000,5000,lambda u:np.sin(np.pi*np.clip(u,0,1))**0.8),0.06);cue('lid_shut',b_,tight_click(1311),0.16)
cue('ec_in',ECV['endcard_in']-0.05,swish(0.7,1400,2500,6500,lambda u:np.sin(np.pi*np.clip(u,0,1))),0.04)
cue('ec_ring',ECV['ring'],tp.pen_scratch(SR,0.6,1401),0.045);cue('ec_pop',ECV['pop'],tp.join_pop(SR,0.18,1402),0.07)
for i,t in enumerate(ECV['word']):cue(f'ec_letter{i}',t,tp.pen_scratch(SR,0.13,1410+i),0.035)
cue('ec_underline',ECV['underline'],tp.pen_scratch(SR,0.35,1420),0.04);cue('ec_glint',ECV['glint'],bell((2637.02,3520.0),0.5),0.03)
SFX_TRIM=0.45;sfx*=SFX_TRIM;sfx[-int(0.06*SR):]=0.0
onset=np.ones(N)
for p in [s_ for q in TL.PERF['paragraphs'] for s_ in q['sentences']]:
 if p['start']>=TOTAL:continue
 i0=int(p['start']*SR);i1=min(N,int((p['start']+0.12+0.45)*SR));r=int(0.04*SR)
 if i1-i0<2*r:continue
 e_=np.full(i1-i0,10**(-9/20));e_[:r]=np.linspace(1,10**(-9/20),r);e_[-r:]=np.linspace(10**(-9/20),1,r);onset[i0:i1]=np.minimum(onset[i0:i1],e_)
sfx=sfx*onset[:,None]
mix=narr[:,None]*np.array([1.0,1.0])+music+sfx
def wav(path,x):
 x=np.clip(x,-1,1);pcm=(x*8388607).astype(np.int32)
 with wave.open(str(path),'wb') as w:
  w.setnchannels(x.shape[1] if x.ndim>1 else 1);w.setsampwidth(3);w.setframerate(SR);w.writeframes(np.frombuffer(pcm.astype('<i4').tobytes(),dtype=np.uint8).reshape(-1,4)[:,:3].tobytes())
wav(AU/'narration.wav',narr[:,None]);wav(AU/'music-bed.wav',music);wav(AU/'sfx.wav',sfx);wav(AU/'premaster.wav',mix)
def low_share(x,cut):
 X=np.abs(np.fft.rfft(x.mean(1) if x.ndim>1 else x))**2;f_=np.fft.rfftfreq(len(x),1/SR);return float(X[f_<cut].sum()/max(1e-12,X.sum()))
spec_={'sfx_energy_below_150Hz':low_share(sfx,150),'sfx_energy_below_250Hz':low_share(sfx,250),'music_energy_below_150Hz':low_share(music,150),'music_energy_below_250Hz':low_share(music,250)}
(AU/'cue-sheet.json').write_text(json.dumps({'narration':'fenrir-performance.wav 0 -> %.3f s (N01-N10)'%TL.NARR_END,'music_under_voice_db':round(under_db,2),'hp_sfx_hz':HP_SFX,'hp_music_hz':HP_MUSIC,'spectral':spec_,'cues':cues},indent=1))
print('under_db',round(under_db,2),'cues',len(cues),'peak',float(np.abs(mix).max()),spec_)
