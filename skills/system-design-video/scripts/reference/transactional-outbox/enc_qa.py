"""Encoded-frame QA on the one clean master: contact sheet (every 1 s), phone-size frames (360x640), every scene transition, code-card closeups."""
import subprocess,numpy as np
from PIL import Image,ImageDraw
import timeline as TL
W,H=1080,1920;M='transactional-outbox-final-v1-master.mp4'
def grab(t):return Image.frombytes('RGB',(W,H),subprocess.check_output(['ffmpeg','-v','error','-ss',f'{t:.3f}','-i',M,'-frames:v','1','-f','rawvideo','-pix_fmt','rgb24','pipe:1']))
def sheet(ims,cols,w,lab,out,h=None):
 h=h or int(w*16/9);rows=(len(ims)+cols-1)//cols;c=Image.new('RGB',(cols*w,rows*(h+18)),(40,40,40));d=ImageDraw.Draw(c)
 for i,(im,l) in enumerate(zip(ims,lab)):x,y=(i%cols)*w,(i//cols)*(h+18);c.paste(im.resize((w,h)),(x,y+18));d.text((x+4,y+3),l,fill=(255,255,0))
 c.save(out)
ts=[x+0.5 for x in range(int(TL.TOTAL))];sheet([grab(t) for t in ts],12,150,[f'{t:.1f}s' for t in ts],'evidence/enc/contact-1s.png')
ph=[0.5,1.5,2.5,3.4,4.5,6.0,7.5,9.4,11.4,12.6,14.2,16.2,17.9,19.6,23.0,26.0,28.0,30.5,33.0,34.4,36.5,39.0,42.0,45.5,47.5,49.5,52.0,54.5,56.2,56.9,57.7,58.5,60.0,62.5,65.0,68.0,70.5,72.5,75.0,77.5,79.0,81.5,84.0,86.0,87.0,87.8,88.6,89.5,90.3]
ims=[grab(t).resize((360,640),Image.Resampling.LANCZOS) for t in ph]
for k in range(0,len(ims),16):sheet(ims[k:k+16],8,360,[f'{t:.1f}s' for t in ph[k:k+16]],f'evidence/enc/phone-360-{chr(97+k//16)}.png')
for name,a,b in TL.SCENES[1:]:
 tt=[a-0.25,a-0.083,a,a+0.083,a+0.25,a+0.5];sheet([grab(t) for t in tt],len(tt),240,[f'{t:.2f}' for t in tt],f'evidence/enc/strip-into-{name}.png')
tt=[2.9+k*0.125 for k in range(int(2.1/0.125))];sheet([grab(t) for t in tt],9,200,[f'{t:.2f}' for t in tt],'evidence/enc/strip-laptop-every-3-frames.png')
tt=[26.9+k*0.125 for k in range(int(2.4/0.125))];sheet([grab(t) for t in tt],10,200,[f'{t:.2f}' for t in tt],'evidence/enc/strip-publish-first-swap.png')
grab(12.6).crop((0,230,1080,600)).resize((360,123),Image.Resampling.LANCZOS).save('evidence/enc/close-code-phone-width.png')
tt=[55.6+k*0.125 for k in range(int(3.4/0.125))];sheet([grab(t) for t in tt],9,200,[f'{t:.2f}' for t in tt],'evidence/enc/strip-relay-retry.png')
tt=[85.6+k*0.25 for k in range(int(4.8/0.25))];sheet([grab(t) for t in tt],10,200,[f'{t:.2f}' for t in tt],'evidence/enc/strip-close-endcard.png')
print('ok')
