"""Editable layered demo. Author concept-specific scenes for the actual lesson.

Generalizes the production's shallow diagrams, curved causal paths, internal
actor/screen/LED changes and separate measured screen-space captions.
"""
import json
import math
from PIL import Image, ImageDraw, ImageFont

C, P, W, BG = '#8cdeea', '#c5a5ee', '#edf2f7', '#101922'
_cache = {}


def smooth(points):
    """Rounded polyline adapted from the source scene path helper."""
    out = [points[0]]
    for index in range(1, len(points) - 1):
        a, b, c = points[index - 1:index + 2]
        d1, d2 = math.dist(a, b), math.dist(b, c)
        if not d1 or not d2:
            continue
        radius = min(16, d1 * .23, d2 * .23)
        u = [b[j] + (a[j] - b[j]) * radius / d1 for j in range(2)]
        v = [b[j] + (c[j] - b[j]) * radius / d2 for j in range(2)]
        out.append(u)
        for step in range(1, 7):
            q = step / 6
            out.append([(1-q)**2*u[j] + 2*(1-q)*q*b[j] + q*q*v[j] for j in range(2)])
    out.append(points[-1])
    return out


def along(points, progress):
    distances = [math.dist(a, b) for a, b in zip(points, points[1:])]
    remaining = sum(distances) * max(0, min(1, progress))
    for index, distance in enumerate(distances):
        if remaining <= distance and distance:
            q = remaining / distance
            return tuple(points[index][j] * (1-q) + points[index+1][j] * q for j in range(2))
        remaining -= distance
    return points[-1]


def frame(t, config, root):
    width, height = config['width'], config['height']
    sx, sy = width / 540, height / 960
    image = Image.new('RGBA', (width, height), BG)
    draw = ImageDraw.Draw(image)
    objects = []
    def rect(box):
        return tuple(v * (sx if i % 2 == 0 else sy) for i, v in enumerate(box))
    def font(size):
        font_path = (root / config['font']).resolve()
        if not font_path.is_relative_to(root.resolve()) or not font_path.is_file():
            raise ValueError('Font must be a preserved file inside the source project')
        key = (str(font_path), round(size * sx))
        if key not in _cache:
            _cache[key] = ImageFont.truetype(key[0], key[1])
        return _cache[key]
    def label(identity, xy, text, size=20, color=W):
        x, y = xy[0] * sx, xy[1] * sy
        f = font(size)
        box = draw.textbbox((x, y), text, font=f, anchor='mm')
        draw.text((x, y), text, font=f, anchor='mm', fill=color)
        objects.append({'id': identity, 'kind': 'label', 'box': box})
    scenes = config['scenes']
    scene = next((s for s in scenes if s['start'] <= t < s['end']), scenes[-1])
    local = t - scene['start']
    label('title', (270, 70), scene['title'], 22)
    # Subtle context terrace; diagram nodes retain level readouts.
    if scene['type'] == 'context':
        draw.polygon([(30*sx, 570*sy), (340*sx, 590*sy), (505*sx, 470*sy), (195*sx, 450*sy)], fill='#172d40')
        draw.polygon([(30*sx, 570*sy), (340*sx, 590*sy), (340*sx, 602*sy), (30*sx, 582*sy)], fill='#0d1a28')
        x, y = 85*sx, 365*sy
        draw.ellipse((x-10*sx, y-50*sy, x+10*sx, y-30*sy), fill=P)
        draw.line([(x, y-28*sy), (x, y+20*sy)], fill=P, width=max(2, round(5*sx)))
        reach = min(1, max(0, local / 1.4))
        draw.line([(x, y-18*sy), (x+(20+18*reach)*sx, y-(8+10*reach)*sy)], fill=P, width=max(2, round(4*sx)))
        draw.line([(x, y+20*sy), (x-12*sx, y+50*sy)], fill=P, width=max(2, round(4*sx)))
        draw.line([(x, y+20*sy), (x+12*sx, y+50*sy)], fill=P, width=max(2, round(4*sx)))
        objects.append({'id': 'operator', 'kind': 'actor', 'box': (x-14*sx, y-50*sy, x+40*sx, y+52*sy)})
    for node in scene.get('nodes', []):
        if local < node.get('reveal', 0):
            continue
        x, y, w, h = node['box']
        draw.rounded_rectangle(rect((x+4, y+6, x+w+4, y+h+6)), radius=8*sx, fill='#070e18')
        draw.rounded_rectangle(rect((x, y, x+w, y+h)), radius=8*sx, fill='#172d40', outline=C, width=max(1, round(sx)))
        objects.append({'id': node['id']+'-body', 'kind': 'shape', 'box': rect((x, y, x+w+4, y+h+6))})
        label(node['id'], (x+w/2, y+h/2), node['label'], 19)
        # Event-driven screen/LED response, rather than continuous flicker.
        active = local >= node.get('activate', 1e9)
        draw.ellipse(rect((x+10, y+8, x+16, y+14)), fill='#abdfbb' if active else '#3a4959')
        if active:
            draw.line([(x+10)*sx, (y+h-10)*sy, (x+w-10)*sx, (y+h-10)*sy], fill=P, width=max(1, round(2*sx)))
    for path in scene.get('paths', []):
        endpoints = [node for node in scene.get('nodes', []) if node['id'] in (path.get('from'), path.get('to'))]
        if local < path.get('reveal', 0) or any(local < node.get('reveal', 0) for node in endpoints):
            continue
        points = smooth(path['points'])
        draw.line([(x*sx, y*sy) for x, y in points], fill='#415d71', width=max(1, round(sx)))
        distance = sum(math.dist(a, b) for a, b in zip(points, points[1:]))
        count = max(2, math.ceil(distance/12))
        for n in range(count+1):
            x, y = along(points, n/count)
            draw.ellipse(rect((x-1, y-1, x+1, y+1)), fill=C)
        a, b = points[-2:]
        angle = math.atan2(b[1]-a[1], b[0]-a[0])
        arrow = [b, (b[0]-8*math.cos(angle)+4*math.sin(angle), b[1]-8*math.sin(angle)-4*math.cos(angle)),
                 (b[0]-8*math.cos(angle)-4*math.sin(angle), b[1]-8*math.sin(angle)+4*math.cos(angle))]
        draw.polygon([(x*sx, y*sy) for x, y in arrow], fill=C)
        for index, (a, b) in enumerate(zip(points, points[1:])):
            objects.append({'id': path['id']+f'-segment-{index}', 'kind': 'path',
                            'box': rect((min(a[0],b[0])-1, min(a[1],b[1])-1, max(a[0],b[0])+1, max(a[1],b[1])+1))})
        objects.append({'id': path['id']+'-arrow', 'kind': 'path',
                        'box': rect((min(p[0] for p in arrow), min(p[1] for p in arrow), max(p[0] for p in arrow), max(p[1] for p in arrow)))})
        start, end = path['start'], path['end']
        if start <= local <= end:
            x, y = along(points, (local-start)/(end-start))
            # Alpha-aware overlay and bounded halo, independent of captions.
            overlay = Image.new('RGBA', image.size)
            od = ImageDraw.Draw(overlay)
            od.ellipse(rect((x-9, y-9, x+9, y+9)), fill=(140, 222, 234, 35))
            od.ellipse(rect((x-4, y-4, x+4, y+4)), fill=C)
            image = Image.alpha_composite(image, overlay)
            draw = ImageDraw.Draw(image)
            objects.append({'id': path['id'], 'kind': 'glow', 'box': rect((x-9, y-9, x+9, y+9))})
    # Fade only the scene before drawing fixed screen-space captions.
    transition = min(1, max(0, local / config.get('transition_seconds', .35)))
    if transition < 1:
        image = Image.blend(Image.new('RGBA', image.size, BG), image, transition)
        draw = ImageDraw.Draw(image)
    key = str(root / config['words'])
    if key not in _cache:
        _cache[key] = json.loads((root / config['words']).read_text(encoding='utf-8'))
    words = _cache[key]
    groups, group = [], []
    breaks = config.get('caption_breaks', []) + [scene['start'] for scene in scenes]
    for word in words:
        if group and (word.get('paragraph', 0) != group[-1].get('paragraph', 0) or len(group) >= 6 or any(group[-1]['start'] < boundary <= word['start'] for boundary in breaks)):
            groups.append(group)
            group = []
        group.append(word)
        if word['text'].endswith(('.', '!', '?', ';')):
            groups.append(group)
            group = []
    if group:
        groups.append(group)
    group = None
    for index, candidate in enumerate(groups):
        end = scenes[-1]['end']
        if index+1 < len(groups):
            following = groups[index+1]
            end = following[0]['start'] if candidate[-1].get('paragraph', 0) == following[0].get('paragraph', 0) else candidate[-1]['end']
        if candidate[0]['start'] <= t < end:
            group = candidate
            break
    if group:
        f = font(config.get('caption_font_size', 23))
        l, top, r, bottom = config['caption_box']
        available = r-l-24*sx
        lines, line = [], []
        for word in group:
            candidate = ' '.join(w['text'] for w in line+[word])
            if draw.textlength(candidate, font=f) > available and line:
                lines.append(line)
                line = []
            line.append(word)
        lines.append(line)
        if len(lines) > 2:
            raise ValueError('Caption exceeds two lines; revise chunk/font/safe area')
        draw.rounded_rectangle((l, top, r, bottom), radius=9*sx, fill='#101922')
        for row, line in enumerate(lines):
            text = ' '.join(w['text'] for w in line)
            x = (l+r-draw.textlength(text, font=f))/2
            y = top + 18*sy + row*30*sy
            for word in line:
                length = draw.textlength(word['text'], font=f)
                current = word['start'] <= t < word['end']
                if current:
                    draw.rounded_rectangle((x-3*sx, y-3*sy, x+length+3*sx, y+26*sy), radius=4*sx, fill=P)
                box = draw.textbbox((x, y), word['text'], font=f)
                if box[0] < l or box[2] > r or box[1] < top or box[3] > bottom:
                    raise ValueError('Caption outside configured safe area')
                draw.text((x, y), word['text'], font=f, fill=BG if current else W)
                x += length + draw.textlength(' ', font=f)
        objects.append({'id': 'captions', 'kind': 'caption', 'box': config['caption_box']})
    return image, objects
