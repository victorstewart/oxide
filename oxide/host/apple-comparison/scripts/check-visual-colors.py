#!/usr/bin/env python3
"""Check named flat interiors, without averaging away scene mismatches."""
import argparse
import json
import pathlib
from PIL import Image

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('capture', type=pathlib.Path)
p.add_argument('--cases', nargs='+')
a = p.parse_args()
# Points are authored in the shared 390x844 scene; no search for favorable pixels.
SAMPLES = {
    'controls': {'button': (40, 74), 'disabled': (230, 74), 'slider-track': (350, 324), 'half-progress': (80, 592), 'full-progress': (80, 632)},
    'editing': {'empty-background': (350, 100), 'editable-background': (350, 200), 'secure-background': (350, 300), 'validation-background': (350, 400)},
    'typography': {'background': (370, 810)},
    'composition': {'outer-panel': (30, 80), 'tile-or-restored-background': (100, 150), 'translucent-overlap': (190, 200), 'cached-tile-or-restored-panel': (100, 520), 'cached-panel': (340, 650)},
    'layout': {'wide-container': (30, 270), 'narrow-container': (240, 270), 'row-background': (350, 380)},
    'opacity': {'panel': (30, 80), 'red': (70, 120), 'overlap': (160, 160), 'nested-overlap': (195, 390), 'fade-restore': (160, 640)},
    'images': {'opaque-center': (75, 135), 'alpha-on-white': (90, 310), 'alpha-on-dark': (280, 310), 'nine-center': (250, 460), 'replacement-or-panel': (190, 670)},
    'geometry': {'blue-shape': (100, 270), 'green-shape': (285, 270), 'background': (380, 800)},
    'editing-edges': {'long-background': (350, 110), 'placeholder-background': (350, 200), 'composition-background': (350, 292), 'otp-slot': (45, 381), 'background': (380, 800)},
    'pickers': {'picker-band': (30, 180), 'overlay-background': (380, 810), 'popup-interior': (80, 480), 'popover-or-restored-background': (35, 630)},
}
results = []
for case in a.cases or SAMPLES:
    points = SAMPLES[case]
    for stage in range(3):
        images = [Image.open(a.capture / f'{side}-visual-{case}-{stage}-crop.png').convert('RGB') for side in ['oxide', 'uikit']]
        for name, (x, y) in points.items():
            neighborhoods = [[im.getpixel((x * 3 + dx, y * 3 + dy)) for dx in [-1, 0, 1] for dy in [-1, 0, 1]] for im in images]
            spreads = [max(max(pixel[channel] for pixel in pixels) - min(pixel[channel] for pixel in pixels) for channel in range(3)) for pixels in neighborhoods]
            colors = [im.getpixel((x * 3, y * 3)) for im in images]
            delta = max(abs(left - right) for left, right in zip(*colors))
            results.append({'case': case, 'stage': stage, 'sample': name, 'point': [x, y], 'oxide': colors[0], 'uikit': colors[1], 'channel_delta': delta, 'neighborhood_spread': spreads, 'pass': delta <= 1 and max(spreads) <= 1})
output = {'threshold': 1, 'samples': len(results), 'passed': sum(row['pass'] for row in results), 'results': results}
(a.capture / 'flat-colors.json').write_text(json.dumps(output, indent=2) + '\n')
for result in results:
    if not result['pass']:
        print(json.dumps(result))
print(f"{output['passed']}/{output['samples']} flat interiors agree within one 8-bit channel value")
raise SystemExit(0 if output['passed'] == output['samples'] else 1)
