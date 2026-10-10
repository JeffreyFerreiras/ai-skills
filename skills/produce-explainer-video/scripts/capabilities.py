"""Read-only capability detection; no downloads or implicit paid services."""
import importlib.util
import json
import shutil

if __name__ == '__main__':
    print(json.dumps({
        'executables': {n: shutil.which(n) for n in ('python', 'node', 'ffmpeg', 'ffprobe')},
        'python_modules': {n: importlib.util.find_spec(n) is not None for n in ('PIL', 'kokoro', 'numpy', 'soundfile')},
        'notes': ['Verify a licensed font and asset provenance.',
                  'Node/resvg and Three.js are optional; inspect project dependency lockfiles.',
                  'Kokoro weights are external runtime data; preserved PCM renders without them.',
                  'Independent reviewers and continuous playback/listening need separate capability checks.']
    }, indent=2))
