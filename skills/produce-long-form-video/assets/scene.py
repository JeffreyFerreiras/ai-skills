"""Editable layered demo. Author concept-specific scenes for the actual lesson.

Generalizes the production's shallow diagrams, curved causal paths, internal
actor/screen/LED changes and separate measured screen-space captions.
"""
import json
import math
from bisect import bisect_right
from PIL import Image, ImageDraw, ImageFont, ImageOps

C, P, W, BG = '#8cdeea', '#c5a5ee', '#edf2f7', '#101922'
_cache = {}
_caption_cache = None


def file_version(path):
    stat = path.stat()
    return str(path.resolve()), stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns


def caption_plan(config, root, sx, sy, font):
    """Compile timing and measured glyph positions once for unchanged inputs."""
    global _caption_cache
    words_path = root / config['words']
    breaks = config.get('caption_breaks', []) + [s['start'] for s in config['scenes']]
    key = (file_version(words_path), file_version(root / config['font']),
           sx, sy, font.size, tuple(config['caption_box']), tuple(breaks), config['scenes'][-1]['end'])
    if _caption_cache is not None and _caption_cache[0] == key:
        return _caption_cache[1]
    words = json.loads(words_path.read_text(encoding='utf-8'))
    groups, group = [], []
    for word in words:
        if group and (word.get('paragraph', 0) != group[-1].get('paragraph', 0) or len(group) >= 6 or any(group[-1]['start'] < b <= word['start'] for b in breaks)):
            groups.append(group)
            group = []
        group.append(word)
        if word['text'].endswith(('.', '!', '?', ';')):
            groups.append(group)
            group = []
    if group:
        groups.append(group)
    draw = ImageDraw.Draw(Image.new('RGB', (1, 1)))
    l, top, r, bottom = config['caption_box']
    space = draw.textlength(' ', font=font)
    plans = []
    for index, group in enumerate(groups):
        end = config['scenes'][-1]['end']
        if index + 1 < len(groups):
            following = groups[index + 1]
            end = following[0]['start'] if group[-1].get('paragraph', 0) == following[0].get('paragraph', 0) else group[-1]['end']
        lines, line = [], []
        for word in group:
            if line and draw.textlength(' '.join(w['text'] for w in line + [word]), font=font) > r-l-24*sx:
                lines.append(line)
                line = []
            line.append(word)
        lines.append(line)
        if len(lines) > 2:
            raise ValueError('Caption exceeds two lines; revise chunk/font/safe area')
        glyphs = []
        for row, line in enumerate(lines):
            x = (l+r-draw.textlength(' '.join(w['text'] for w in line), font=font))/2
            y = top + 18*sy + row*30*sy
            for word in line:
                length = draw.textlength(word['text'], font=font)
                box = draw.textbbox((x, y), word['text'], font=font)
                if box[0] < l or box[2] > r or box[1] < top or box[3] > bottom:
                    raise ValueError('Caption outside configured safe area')
                glyphs.append((word, x, y, length))
                x += length + space
        plans.append({'start': group[0]['start'], 'end': end, 'glyphs': glyphs})
    plan = ([p['start'] for p in plans], plans)
    # Keep only the latest project plan rather than retaining whole transcripts.
    _caption_cache = key, plan
    return plan


def draw_captions(image, t, config, root, sx, sy, font):
    starts, plans = caption_plan(config, root, sx, sy, font)
    index = bisect_right(starts, t) - 1
    if index < 0 or t >= plans[index]['end']:
        return []
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle(config['caption_box'], radius=9*sx, fill=BG)
    for word, x, y, length in plans[index]['glyphs']:
        current = word['start'] <= t < word['end']
        if current:
            draw.rounded_rectangle((x-3*sx, y-3*sy, x+length+3*sx, y+26*sy), radius=4*sx, fill=P)
        draw.text((x, y), word['text'], font=font, fill=BG if current else W)
    return [{'id': 'captions', 'kind': 'caption', 'box': config['caption_box']}]


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


def state_text(node, local, duration):
    """Select the current readout without exposing a future state on seeks."""
    text, previous = None, -1
    for state in node.get('states', []):
        at = state['at']
        if isinstance(at, bool) or not isinstance(at, (int, float)) or not math.isfinite(at) or not 0 <= at < duration or at <= previous:
            raise ValueError('State cues must increase within the scene duration')
        if not isinstance(state.get('text'), str) or not state['text'].strip():
            raise ValueError('State readouts need nonempty text')
        previous = at
        if at <= local:
            text = state['text']
    return text


def frame(t, config, root):
    width, height = config['width'], config['height']
    # Missing design dimensions retain the original portrait starter contract.
    design_width, design_height = config.get('design_width', 540), config.get('design_height', 960)
    if any(isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0 for value in (design_width, design_height)):
        raise ValueError('Design dimensions must be finite and positive')
    sx, sy = width / design_width, height / design_height
    if not math.isclose(sx, sy):
        raise ValueError('Recompose the scene for the output aspect ratio; do not stretch the design canvas')
    image = Image.new('RGBA', (width, height), BG)
    if config.get('background'):
        path = (root / config['background']).resolve()
        if not path.is_relative_to(root.resolve()):
            raise ValueError('Background must be inside the source project')
        key = ('background', file_version(path), width, height)
        if key not in _cache:
            with Image.open(path) as background:
                # Center smaller originals at native resolution; never stretch texture pixels.
                fitted = ImageOps.contain(background.convert('RGBA'),
                                          (min(width, background.width), min(height, background.height)),
                                          Image.Resampling.LANCZOS)
                canvas = Image.new('RGBA', (width, height), BG)
                canvas.paste(fitted, ((width-fitted.width)//2, (height-fitted.height)//2))
                _cache[key] = canvas
        image = _cache[key].copy()
    draw = ImageDraw.Draw(image)
    objects = []
    def rect(box):
        return tuple(v * (sx if i % 2 == 0 else sy) for i, v in enumerate(box))
    def font(size):
        font_path = (root / config['font']).resolve()
        if not font_path.is_relative_to(root.resolve()) or not font_path.is_file():
            raise ValueError('Font must be a preserved file inside the source project')
        key = (str(font_path), round(size * sx), file_version(font_path))
        if key not in _cache:
            _cache[key] = ImageFont.truetype(key[0], key[1])
        return _cache[key]
    def label(identity, xy, text, size=20, color=W):
        x, y = xy[0] * sx, xy[1] * sy
        f = font(size)
        box = draw.textbbox((x, y), text, font=f, anchor='mm')
        draw.text((x, y), text, font=f, anchor='mm', fill=color)
        objects.append({'id': identity, 'kind': 'label', 'box': box, 'text': text})
    def surface(identity, box, shape='panel', accent=C, muted=False):
        x, y, w, h = box
        fill = '#15222b' if muted else '#172d40'
        edge = '#415461' if muted else accent
        draw.rounded_rectangle(rect((x+5, y+7, x+w+5, y+h+7)), radius=9*sx, fill='#070e18')
        if shape == 'database':
            lip = h*.18
            draw.rectangle(rect((x, y+lip, x+w, y+h-lip)), fill=fill, outline=edge, width=max(1, round(sx)))
            draw.ellipse(rect((x, y+h-2*lip, x+w, y+h)), fill=fill, outline=edge, width=max(1, round(sx)))
            draw.rectangle(rect((x+1, y+lip, x+w-1, y+h-lip)), fill=fill)
            draw.ellipse(rect((x, y, x+w, y+2*lip)), fill='#233e50' if not muted else fill, outline=edge, width=max(1, round(sx)))
        elif shape == 'tag':
            points = [(x, y+22), (x+22, y), (x+w, y), (x+w, y+h), (x, y+h)]
            draw.polygon([(a*sx,b*sy) for a,b in points], fill=fill, outline=edge, width=max(1, round(2*sx)))
            draw.ellipse(rect((x+12,y+12,x+21,y+21)), fill=BG, outline=edge)
        elif shape == 'receipt':
            points = [(x,y), (x+w,y), (x+w,y+h)]
            for i in range(20, -1, -1):
                points.append((x+w*i/20, y+h-(7 if i%2 else 0)))
            draw.polygon([(a*sx,b*sy) for a,b in points], fill=fill, outline=edge, width=max(1, round(sx)))
            for i in range(3):
                draw.line(rect((x+15+i*12,y+12,x+22+i*12,y+12)), fill=edge, width=max(1, round(sx)))
        else:
            if shape == 'copy':
                for offset in (10, 5):
                    draw.rounded_rectangle(rect((x+offset,y-offset,x+w+offset,y+h-offset)), radius=8*sx, fill='#223849', outline=edge, width=max(1, round(sx)))
            draw.rounded_rectangle(rect((x,y,x+w,y+h)), radius=8*sx, fill=fill, outline=edge, width=max(1, round(sx)))
            if shape == 'browser':
                draw.line(rect((x,y+20,x+w,y+20)), fill=edge, width=max(1, round(sx)))
                for i in range(3):
                    draw.ellipse(rect((x+10+i*10,y+7,x+14+i*10,y+11)), fill=edge)
        objects.append({'id': identity+'-body', 'kind': 'shape', 'box': rect((x,y-10 if shape == 'copy' else y,x+w+10,y+h+7))})
    scenes = config['scenes']
    scene = next((s for s in scenes if s['start'] <= t < s['end']), scenes[-1])
    local = t - scene['start']
    label('title', (design_width/2, min(70, design_height*.1)), scene['title'], 22)
    if scene['type'] == 'comparison':
        if len(scene['comparisons']) != 2:
            raise ValueError('Comparison view requires exactly two alternatives')
        for index, item in enumerate(scene['comparisons']):
            x, y, w, h = design_width*(.075 + index*.48), design_height*.26, design_width*.37, design_height*.37
            surface(item['id'], (x,y,w,h), item.get('shape', 'panel'), C if index == 0 else P)
            label(item['id'], (x+w/2, y+h*.18), item['label'], 19)
            value = state_text(item, local, scene['end']-scene['start'])
            if value is not None:
                label(item['id']+'-state', (x+w/2, y+h*.5), value, item.get('value_size', 42), C if index == 0 else P)
            label(item['id']+'-detail', (x+w/2, y+h*.82), item.get('detail', ''), 15)
    if scene['type'] == 'timeline':
        events = scene['events']
        values = [event['time'] for event in events]
        if len(values) < 2 or any(isinstance(v, bool) or not isinstance(v, (int, float)) or not math.isfinite(v) for v in values) or any(a >= b for a, b in zip(values, values[1:])):
            raise ValueError('Timeline needs increasing finite event times')
        state_text({'states': [{'at': e['at'], 'text': e['label']} for e in events]}, local, scene['end']-scene['start'])
        y, left, right = design_height*.43, design_width*.09, design_width*.91
        draw.line(rect((left, y, right, y)), fill='#415d71', width=max(1, round(3*sx)))
        for index, event in enumerate(events):
            if local < event['at']:
                continue
            x = left + (event['time']-values[0])/(values[-1]-values[0])*(right-left)
            draw.ellipse(rect((x-5, y-5, x+5, y+5)), fill=P)
            objects.append({'id': f'event-{index}', 'kind': 'shape', 'box': rect((x-5, y-5, x+5, y+5))})
            # Alternating lanes leave close events readable without moving their time positions.
            lane = 1 if index % 2 else -1
            label(f'event-{index}-time', (x, y+lane*35), f"{event['time']:g}s", 18, P)
            label(f'event-{index}-label', (x, y+lane*62), event['label'], 16)
            label(f'event-{index}-value', (x, y+lane*87), event['value'], 18, C)
        label('timeline-note', (design_width/2, design_height*.69), scene.get('note', 'Illustrative timeline'), 15)
    # Subtle context terrace; diagram nodes retain level readouts.
    if scene['type'] == 'context':
        stage_x, stage_y = design_width/2-270, design_height*.54-520
        draw.polygon([((x+stage_x)*sx, (y+stage_y)*sy) for x, y in [(30, 570), (340, 590), (505, 470), (195, 450)]], fill='#172d40')
        draw.polygon([((x+stage_x)*sx, (y+stage_y)*sy) for x, y in [(30, 570), (340, 590), (340, 602), (30, 582)]], fill='#0d1a28')
        x, y = design_width*.1574*sx, design_height*.38*sy
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
        surface(node['id'], (x,y,w,h), node.get('shape', 'panel'), muted=node.get('muted', False))
        readout = state_text(node, local, scene['end']-scene['start'])
        label(node['id'], (x+w/2, y+h*(.44 if node.get('shape') == 'database' else .32 if node.get('states') else .5)), node['label'], 19, '#8d9ca7' if node.get('muted') else W)
        if readout is not None:
            label(node['id']+'-state', (x+w/2, y+h*.74), readout, 17, '#8d9ca7' if node.get('muted') else P)
        # Event-driven screen/LED response, rather than continuous flicker.
        active = local >= node.get('activate', 1e9)
        if node.get('shape', 'panel') == 'panel':
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
    transition_seconds = config.get('transition_seconds', 0)
    transition = min(1, max(0, local / transition_seconds)) if transition_seconds > 0 else 1
    if transition < 1:
        image = Image.blend(Image.new('RGBA', image.size, BG), image, transition)
        draw = ImageDraw.Draw(image)
    objects.extend(draw_captions(image, t, config, root, sx, sy, font(config.get('caption_font_size', 23))))
    return image, objects
