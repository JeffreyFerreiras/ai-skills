"""Mix quiet event beeps into a separate PCM16 copy; retain exact native WAV."""
import argparse
import array
import json
import math
from pathlib import Path
import sys
import wave
from audio import audit


def mix(source, events, output):
    if output.exists() or source.resolve() == output.resolve():
        raise FileExistsError('Preserve existing audio')
    with wave.open(str(source), 'rb') as wav:
        params = wav.getparams()
        if wav.getnchannels() != 1 or wav.getsampwidth() != 2:
            raise ValueError('Expected mono PCM16 source')
        samples = array.array('h', wav.readframes(wav.getnframes()))
        if sys.byteorder != 'little':
            samples.byteswap()
    for event in events:
        time, duration = event['time'], event.get('duration', .08)
        gain, hz = event.get('gain', .015), event.get('hz', 880)
        if not all(math.isfinite(v) for v in (time, duration, gain, hz)) or not 0 < duration <= .3 or not 0 <= gain <= .03 or not 100 <= hz <= 3000:
            raise ValueError('Invalid restrained event sound')
        begin, count = round(time*params.framerate), round(duration*params.framerate)
        if begin < 0 or begin+count > len(samples):
            raise ValueError('Event outside narration')
        for index in range(count):
            delta = round(32768*gain*math.sin(math.pi*index/count)**2*math.sin(2*math.pi*hz*index/params.framerate))
            value = samples[begin+index]+delta
            if not -32767 <= value <= 32766:
                raise ValueError('Mix would clip; reduce event gain')
            samples[begin+index] = value
    if sys.byteorder != 'little':
        samples.byteswap()
    with wave.open(str(output), 'wb') as wav:
        wav.setparams(params)
        wav.writeframes(samples.tobytes())
    return {'original': audit(source), 'mix': audit(output), 'events': len(events), 'listened': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('events', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    print(json.dumps(mix(args.source, json.loads(args.events.read_text()), args.output), indent=2))
