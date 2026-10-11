"""Export model cue sidecars with bounded groups and exact text coverage."""
import argparse
import json
from pathlib import Path
from audio import audit, check_cues


def stamp(value):
    milliseconds = round(value * 1000)
    return f'{milliseconds//3600000:02}:{milliseconds//60000%60:02}:{milliseconds//1000%60:02},{milliseconds%1000:03}'


def export(words, group=6, breaks=()):
    sections, section = [], []
    for word in words:
        if section and (len(section) >= group or word.get('paragraph', 0) != section[-1].get('paragraph', 0) or any(section[-1]['start'] < boundary <= word['start'] for boundary in breaks)):
            sections.append(section)
            section = []
        section.append(word)
        if word['text'].endswith(('.', '!', '?', ';')):
            sections.append(section)
            section = []
    if section:
        sections.append(section)
    rows = []
    for section in sections:
        rows.append(f"{len(rows)+1}\n{stamp(section[0]['start'])} --> {stamp(section[-1]['end'])}\n" + ' '.join(w['text'] for w in section))
    return '\n\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('words', type=Path)
    parser.add_argument('script', type=Path)
    parser.add_argument('wav', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--group', type=int, default=6)
    parser.add_argument('--project', type=Path, help='Use scene starts and explicit caption_breaks')
    args = parser.parse_args()
    if not 1 <= args.group <= 12:
        parser.error('group must be 1..12')
    words = json.loads(args.words.read_text(encoding='utf-8'))
    check_cues(args.script.read_text(encoding='utf-8'), words, audit(args.wav)['duration'])
    breaks = []
    if args.project:
        project = json.loads(args.project.read_text(encoding='utf-8'))
        breaks = project.get('caption_breaks', []) + [scene['start'] for scene in project['scenes']]
    with args.output.open('x', encoding='utf-8') as output:
        output.write(export(words, args.group, breaks))
