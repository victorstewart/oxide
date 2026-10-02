#!/usr/bin/env python3
"""Compare immutable baseline/candidate binaries offscreen, one mechanism at a time."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess

import run_macos


def fixed(binary, directory, case, cycles):
    directory.mkdir(parents=True)
    receipt = directory / 'receipt.json'
    environment = os.environ.copy()
    environment.update({'OXIDE_MAC_OFFSCREEN': '1', 'OXIDE_MAC_PROFILE_SECONDS': '12',
                        'OXIDE_MAC_BENCH_CYCLES': str(cycles), 'OXIDE_MAC_PROFILE_CONTINUOUS': '1',
                        'OXIDE_MAC_CASE': case, 'OXIDE_MAC_OUTPUT': str(receipt)})
    with (directory / 'runner.log').open('w') as log:
        process = subprocess.Popen([str(binary)], env=environment, stdout=log, stderr=log)
        guard_command = run_macos.keep_awake_command(process.pid, offscreen=True)
        guard = subprocess.Popen(guard_command) if guard_command else None
        try:
            code = process.wait(timeout=120)
        finally:
            run_macos.stop_process(process)
            run_macos.stop_process(guard)
    result = json.loads(receipt.read_text())
    if code or result['errors'] or result['complete_cycles'] != cycles or not result['measured_frames']:
        raise ValueError('invalid fixed-work receipt')
    result['cpu_us_per_frame'] = result['measured_cpu_us'] / result['measured_frames']
    result['wall_us_per_frame'] = result['measured_wall_seconds'] * 1e6 / result['measured_frames']
    return result


def captures(directory, case):
    from PIL import Image
    receipt = json.loads((directory / 'runs' / case / 'capture-00' / 'receipt.json').read_text())
    records = []
    for path in receipt['capture_files']:
        with Image.open(path) as img:
            if img.size != (1170, 2532):
                raise ValueError('unexpected target dimensions')
            records.append(hashlib.sha256(img.convert('RGBA').tobytes()).hexdigest())
    if len(records) != 7 or any(records[a] != records[b] for a, b in [(0,3),(1,4),(2,5),(0,6)]):
        raise ValueError('checkpoint restoration failed')
    return records


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', required=True, type=Path)
    parser.add_argument('--candidate', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--cases', default='')
    parser.add_argument('--mode', choices=['timing','accounting','capture','fixed'], default='timing')
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--cycles', type=int, default=10)
    args = parser.parse_args()
    if args.repeats < 1 or not 1 <= args.cycles <= 1000:
        parser.error('invalid repeats/cycles')
    for key in list(os.environ):
        if key.startswith('OXIDE_MAC_') or key.startswith('MallocStackLogging'):
            os.environ.pop(key)
    binaries = {'baseline': args.baseline.resolve(), 'candidate': args.candidate.resolve()}
    cases = run_macos.parse_cases(args.cases)
    output = args.output.resolve()
    if output.exists() and any(output.iterdir()):
        parser.error('output must be new or empty')
    output.mkdir(parents=True, exist_ok=True)
    repo = Path(__file__).resolve().parents[3]
    results = {side: [] for side in binaries}
    plan = {'mode': args.mode, 'cases': cases, 'repeats': args.repeats,
            'fixed_cycles': args.cycles if args.mode == 'fixed' else None,
            'binaries': {side: {'path': str(binary), 'sha256': run_macos.sha256_file(binary),
                               'source': json.loads(binary.with_suffix('.json').read_text())}
                         for side, binary in binaries.items()}}
    run_macos.write_json(output / 'plan.json', plan)
    count = args.repeats if args.mode in ('timing','fixed') else 1
    for case in cases:
        for repeat in range(count):
            for side in (('baseline','candidate') if repeat % 2 == 0 else ('candidate','baseline')):
                if args.mode == 'fixed':
                    item = fixed(binaries[side], output / side / case / str(repeat), case, args.cycles)
                    item.update({'case': case, 'repeat': repeat})
                else:
                    item = run_macos.run_one(binaries[side], repo, output / side, case, repeat,
                                            args.mode, 6, 12, offscreen=True, text_path='immediate')
                    valid = (item['exit_code'] == 0 and not item['timed_out'] and not item['receipt_error']
                             and not item['reduced']['errors'] and item['capture_validity']['exactly_seven_files']
                             if args.mode == 'capture' else item['reduced']['valid'])
                    if not valid:
                        raise ValueError('invalid receipt: ' + repr(item))
                results[side].append(item)
                run_macos.write_json(output / 'progress.json', results)
        print(json.dumps({'case': case, 'mode': args.mode, 'pairs': count}), flush=True)
    comparison = {}
    for case in cases:
        row = {}
        for side in binaries:
            runs = [r for r in results[side] if r['case'] == case]
            if args.mode == 'fixed':
                population = [r['measured_frames'] for r in runs]
                row[side] = {key: [r[key] for r in runs] for key in ['cpu_us_per_frame','wall_us_per_frame','init_ms']}
                row[side]['frames'] = population
            elif args.mode == 'capture':
                row[side] = captures(output / side, case)
            else:
                row[side] = {key.removesuffix('_ms') + '_us': [r['reduced']['metrics'][key]['p50'] * 1000 for r in runs]
                             for key in ['cpu_draw_ms','prepare_ms','encode_submit_ms']}
                row[side]['cpu_us_per_frame'] = [r['reduced']['cpu_us_per_measured_frame'] for r in runs]
        if args.mode == 'capture':
            row['pixels_equal'] = row['baseline'] == row['candidate']
            if not row['pixels_equal']:
                raise ValueError('pixel mismatch: ' + case)
        else:
            if args.mode == 'fixed' and row['baseline']['frames'] != row['candidate']['frames']:
                raise ValueError('fixed replay populations differ')
            row['median_percent_change'] = {key: (statistics.median(row['candidate'][key]) / statistics.median(row['baseline'][key]) - 1) * 100
                                           for key in row['baseline'] if key != 'frames' and statistics.median(row['baseline'][key])}
        comparison[case] = row
    run_macos.write_json(output / 'comparison.json', {'plan': plan, 'cases': comparison})
    print(json.dumps(comparison), flush=True)


if __name__ == '__main__':
    main()
