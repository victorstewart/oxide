#!/usr/bin/env python3
"""Capture the finite visual-only suite from two built simulator apps.

Requires Pillow for viewport crops/gallery contact sheets. Raw screenshots and
checkpoint receipts remain intact. This tool never reports performance numbers.
"""
import argparse
import hashlib
import html
import json
import pathlib
import subprocess
import time
from PIL import Image, ImageDraw

CASES = ['controls', 'editing', 'typography', 'composition', 'layout', 'pickers', 'opacity', 'images', 'geometry', 'editing-edges']
CORE_CASES = ['shapes', 'text', 'images', 'local', 'animation', 'scroll']
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--device', required=True)
p.add_argument('--products', type=pathlib.Path, required=True)
p.add_argument('--output', type=pathlib.Path, required=True)
p.add_argument('--suite', choices=['visual', 'core'], default='visual')
p.add_argument('--cases', nargs='+')
a = p.parse_args()
cases = CORE_CASES if a.suite == 'core' else CASES
a.cases = a.cases or cases
if any(case not in cases for case in a.cases):
    p.error('Unknown case for selected suite')
stages = [0, 10, 19.9] if a.suite == 'core' else [0, 1, 2]
prefix = '' if a.suite == 'core' else 'visual-'
if a.output.exists() and any(a.output.iterdir()):
    p.error("Use a new output directory to preserve existing evidence")
a.output.mkdir(parents=True, exist_ok=True)
root = pathlib.Path(__file__).resolve().parents[4]
def run(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT, timeout=45).strip()
def sim(*args):
    return run('xcrun', 'simctl', *args)
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
devices = json.loads(sim('list', 'devices', '--json'))['devices']
selected_device = [{'runtime': runtime, **device} for runtime, entries in devices.items() for device in entries if device['udid'] == a.device]
if len(selected_device) != 1 or selected_device[0]['state'] != 'Booted':
    p.error('The selected simulator must be booted')
identity = {'commit': run('git', '-C', str(root), 'rev-parse', 'HEAD'),
            'device': a.device, 'configuration': selected_device[0],
            'xcode': run('xcodebuild', '-version'),
            'sdk': run('xcrun', '--sdk', 'iphonesimulator', '--show-sdk-version'),
            'fixture_sha256': sha(root / 'oxide/host/apple-comparison/fixtures/visual.json'),
            'binaries': {}, 'sources': {}}
for path in list((root / 'oxide/host/apple-comparison').rglob('*')) + list((root / 'oxide/crates').rglob('*')):
    if path.is_file() and path.suffix in ['.rs', '.swift', '.json', '.metal', '.h', '.toml', '.lock', '.yml', '.plist', '.pbxproj', '.ttf', '.png']:
        identity['sources'][str(path.relative_to(root))] = sha(path)
for relative in ['oxide/Cargo.lock', 'rust-toolchain.toml']:
    path = root / relative
    if path.exists():
        identity['sources'][relative] = sha(path)
for side, app in [('oxide', 'OxideBenchIOS'), ('uikit', 'UIKitBenchIOS')]:
    product = a.products / (app + '.app')
    identity['binaries'][side] = sha(product / app)
    sim('install', a.device, str(product))
    bundle = 'com.oxide.comparison.' + side + 'benchios'
    container = pathlib.Path(sim('get_app_container', a.device, bundle, 'data'))
    for case in a.cases:
        for stage in stages:
            name = f'{side}-{prefix}{case}-{stage}'
            run_id = name + '-' + str(time.time_ns())
            receipt = container / 'Documents/core-result.json'
            receipt.unlink(missing_ok=True)
            (container / "tmp/oxide-visual-render-error.txt").unlink(missing_ok=True)
            sim('launch', '--terminate-running-process', a.device, bundle,
                '-oxide-core-case', prefix + case, '-oxide-core-checkpoint', str(stage),
                '-oxide-core-run-id', run_id)
            result = None
            for _ in range(100):
                if receipt.exists():
                    result = json.loads(receipt.read_text())
                    if result.get('run_id') == run_id:
                        if result['status'] == 'checkpoint-ready':
                            break
                        if result.get('error'):
                            (a.output / (name + '.json')).write_text(json.dumps(result, indent=2) + '\n')
                            diagnostic = container / 'tmp/oxide-visual-render-error.txt'
                            if diagnostic.exists():
                                (a.output / (name + '-error.txt')).write_text(diagnostic.read_text())
                            raise RuntimeError(result)
                time.sleep(.2)
            else:
                raise RuntimeError((run_id, result))
            time.sleep(.25)
            sim('io', a.device, 'screenshot', str(a.output / (name + '.png')))
            (a.output / (name + '.json')).write_text(json.dumps(result, indent=2) + '\n')
            # Fixed 390x844-point content centered in the simulator screenshot.
            screenshot = Image.open(a.output / (name + '.png')).convert('RGB')
            width, height = screenshot.size
            left, top = (width - 1170) // 2, (height - 2532) // 2
            screenshot.crop((left, top, left + 1170, top + 2532)).save(a.output / (name + '-crop.png'))
            print(name, flush=True)
(a.output / 'identities.json').write_text(json.dumps(identity, indent=2) + '\n')
parts = ['<!doctype html><meta charset="utf-8"><title>Oxide / UIKit visual boards</title>',
         '<style>body{font:16px system-ui;margin:24px;background:#eee}section{display:grid;grid-template-columns:1fr 1fr;gap:12px}img{width:100%}h2,h3{grid-column:1/-1}</style>',
         '<h1>Oxide / UIKit visual parity</h1><p>Simulator correctness captures. Left: Oxide. Right: UIKit. Stages: initial, changed, settled.</p>']
for case in cases:
    if not (a.output / f'oxide-{prefix}{case}-0-crop.png').exists():
        continue
    sheet = Image.new('RGB', (780 * 3, 880), 'white')
    draw = ImageDraw.Draw(sheet)
    parts.append('<h2>' + html.escape(case) + '</h2><section>')
    for index, stage in enumerate(stages):
        parts.append('<h3>' + (f'{stage} seconds' if a.suite == 'core' else ['Initial', 'Changed', 'Settled'][index]) + '</h3>')
        for col, side in enumerate(['oxide', 'uikit']):
            name = f'{side}-{prefix}{case}-{stage}-crop.png'
            parts.append(f'<a href="{name}"><img src="{name}" alt="{side} {case} {stage}"></a>')
            with Image.open(a.output / name) as image:
                sheet.paste(image.resize((390, 844)), (index * 780 + col * 390, 36))
            draw.text((index * 780 + col * 390 + 10, 10), f'{side} — {stage}', fill='black')
    sheet.save(a.output / f'{case}-contact.png')
    parts.append('</section>')
(a.output / 'gallery.html').write_text('\n'.join(parts))
print(a.output / 'gallery.html', flush=True)

(a.output / 'artifacts-sha256.json').write_text(json.dumps({path.name: sha(path) for path in sorted(a.output.iterdir()) if path.is_file()}, indent=2) + '\n')
