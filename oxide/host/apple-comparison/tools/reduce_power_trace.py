#!/usr/bin/env python3
"""Reduce Power Profiler SystemPowerLevel samples into battery percentage points.

Power Profiler exports a whole-device rate, not app-attributed energy.  This
reducer accepts only explicit trace-clock windows and integrates the native
``percent-per-hour`` samples as battery percentage points.  It intentionally
does not infer joules, app impact, or phase boundaries from another clock.
"""
import argparse
import json
import math
from pathlib import Path
import xml.etree.ElementTree as ET

from reduce_core_trace import Trace, process_pid


NANOSECONDS_PER_SECOND = 1_000_000_000
NANOSECOND_ROUNDING = 1
MAX_USABLE_GAP_NS = 1_000
MAX_USABLE_MISSING_NS_PER_SECOND = 1_000
ENERGY_PHASES = (('CoreWarmup', 30), ('CoreControlBefore', 30),
                 ('CoreRun', 120), ('CoreDrain', 10), ('CoreControlAfter', 30))
MAX_EARLY_PHASE_SECONDS = .01
MAX_CALIBRATION_SPREAD_NS = 100_000
MAX_RECEIPT_ANCHOR_ERROR_NS = 1_000_000
WINDOW_METADATA = ('target_seconds', 'duration_deviation_seconds', 'window_source',
                   'observed_marker_count', 'missing_marker_count', 'offset_spread_ns')


class PowerTrace:
    def __init__(self, xml):
        self.root = ET.parse(xml).getroot()
        self.ids = {element.get('id'): element for element in self.root.iter() if element.get('id')}

    def deref(self, element):
        seen = set()
        while element.get('ref'):
            reference = element.get('ref')
            if reference in seen:
                raise ValueError('cyclic Instruments reference')
            seen.add(reference)
            try:
                element = self.ids[reference]
            except KeyError as error:
                raise ValueError(f'unresolved Instruments reference: {reference}') from error
        return element

    def raw(self, element):
        element = self.deref(element)
        return (element.text or '') + ''.join(self.raw(child) + (child.tail or '') for child in element)


def finite_number(trace, element, field):
    try:
        value = float(trace.raw(element).strip())
    except ValueError as error:
        if str(error).startswith('unresolved Instruments reference') or str(error) == 'cyclic Instruments reference':
            raise
        raise ValueError(f'invalid SystemPowerLevel {field}') from error
    if not math.isfinite(value):
        raise ValueError(f'non-finite SystemPowerLevel {field}')
    return value


def system_power_rows(xml):
    """Read validated SystemPowerLevel rows, resolving exported XML references."""
    trace = PowerTrace(xml)
    table = next((node for node in trace.root.findall('node')
                  if node.find("schema[@name='SystemPowerLevel']") is not None), None)
    if table is None:
        raise ValueError('SystemPowerLevel table unavailable')
    columns = table.find('schema').findall('col')
    engineering_types = [column.findtext('engineering-type') for column in columns]
    if engineering_types[:3] != ['start-time', 'duration', 'percent-per-hour']:
        raise ValueError('SystemPowerLevel requires start-time, duration, and percent-per-hour columns')

    samples = []
    for row in table.findall('row'):
        cells = list(row)
        if len(cells) < 3:
            raise ValueError('malformed SystemPowerLevel row')
        if cells[0].tag != 'start-time' or cells[1].tag != 'duration' or cells[2].tag != 'percent-per-hour':
            raise ValueError('unexpected SystemPowerLevel row units')
        start = finite_number(trace, cells[0], 'start')
        duration = finite_number(trace, cells[1], 'duration')
        rate = finite_number(trace, cells[2], 'percent-per-hour')
        if not start.is_integer():
            raise ValueError('SystemPowerLevel start must be an integer nanosecond')
        if not duration.is_integer() or duration <= 0:
            raise ValueError('SystemPowerLevel duration must be positive integer nanoseconds')
        if rate < 0:
            raise ValueError('SystemPowerLevel percent-per-hour must be non-negative')
        start, duration = int(start), int(duration)
        samples.append({'start_ns': start, 'stop_ns': start + duration,
                        'duration_ns': duration, 'percent_per_hour': rate})
    if not samples:
        raise ValueError('SystemPowerLevel has no rows')
    return deduplicate_samples(samples)


def deduplicate_samples(samples):
    """Drop exact samples and reject every non-rounding overlap."""
    unique = {}
    duplicate_rows = 0
    for sample in samples:
        key = (sample['start_ns'], sample['duration_ns'], sample['percent_per_hour'])
        if key in unique:
            duplicate_rows += 1
        else:
            unique[key] = sample
    ordered = sorted(unique.values(), key=lambda sample: (sample['start_ns'], sample['stop_ns']))
    for previous, current in zip(ordered, ordered[1:]):
        overlap = previous['stop_ns'] - current['start_ns']
        if overlap > NANOSECOND_ROUNDING:
            raise ValueError('conflicting SystemPowerLevel intervals overlap beyond nanosecond rounding')
    return ordered, duplicate_rows


def normalize_windows(windows):
    """Accept a windows list or an object containing one, without clock conversion."""
    if isinstance(windows, dict):
        windows = windows.get('windows')
    if not isinstance(windows, list) or not windows:
        raise ValueError('at least one explicit power window is required')
    normalized = []
    names = set()
    for index, window in enumerate(windows):
        if not isinstance(window, dict):
            raise ValueError('power window must be an object')
        name = window.get('name', f'window-{index + 1}')
        start = window.get('start_ns')
        stop = window.get('stop_ns', window.get('end_ns'))
        if not isinstance(name, str) or not name or name in names:
            raise ValueError('power windows require unique names')
        if type(start) is not int or type(stop) is not int or stop <= start:
            raise ValueError('power windows require positive integer-nanosecond start_ns and stop_ns')
        names.add(name)
        normalized_window = {'name': name, 'start_ns': start, 'stop_ns': stop}
        for key in WINDOW_METADATA:
            if key in window:
                normalized_window[key] = window[key]
        normalized.append(normalized_window)
    return normalized


def reduce_window(samples, window):
    start, stop = window['start_ns'], window['stop_ns']
    cursor = start
    percentage_points = 0.0
    covered_ns = 0
    samples_used = 0
    gaps = []
    for sample in samples:
        lo, hi = max(start, sample['start_ns']), min(stop, sample['stop_ns'])
        if hi <= lo:
            continue
        if lo > cursor:
            gaps.append({'start_ns': cursor, 'stop_ns': lo})
        lo = max(lo, cursor)
        if hi <= lo:
            continue
        duration_ns = hi - lo
        percentage_points += sample['percent_per_hour'] * (duration_ns / NANOSECONDS_PER_SECOND) / 3600
        covered_ns += duration_ns
        samples_used += 1
        cursor = hi
    if cursor < stop:
        gaps.append({'start_ns': cursor, 'stop_ns': stop})
    uncovered_ns = sum(gap['stop_ns'] - gap['start_ns'] for gap in gaps)
    usable_coverage = (all(gap['stop_ns'] - gap['start_ns'] <= MAX_USABLE_GAP_NS for gap in gaps)
                       and uncovered_ns * NANOSECONDS_PER_SECOND <= (stop - start) * MAX_USABLE_MISSING_NS_PER_SECOND)
    result = {'name': window['name'], 'start_ns': start, 'stop_ns': stop,
            'duration_seconds': (stop - start) / NANOSECONDS_PER_SECOND,
            'covered_seconds': covered_ns / NANOSECONDS_PER_SECOND,
            'battery_percentage_points': percentage_points,
            'samples_used': samples_used, 'coverage_complete': not gaps,
            'usable_coverage': usable_coverage,
            'coverage_fraction': covered_ns / (stop - start),
            'uncovered_ns': uncovered_ns, 'uncovered_ranges': gaps}
    for key in WINDOW_METADATA:
        if key in window:
            result[key] = window[key]
    return result


def reduce_power_trace(xml, windows, receipt_valid=None):
    """Return whole-device power totals for the supplied trace-clock windows."""
    samples, duplicate_rows = system_power_rows(xml)
    windows = normalize_windows(windows)
    reduced_windows = [reduce_window(samples, window) for window in windows]
    valid_power_windows = all(window['usable_coverage'] for window in reduced_windows)
    return {'schema_version': 1, 'valid_power_windows': valid_power_windows,
            'exact_power_window_coverage': all(window['coverage_complete'] for window in reduced_windows),
            'valid_receipt': receipt_valid,
            'valid_energy': valid_power_windows and receipt_valid if receipt_valid is not None else None,
            'power_schema': 'SystemPowerLevel',
            'power_units': 'percent-per-hour',
            'energy_units': 'battery-percentage-points',
            'scope': 'whole-device Power Profiler rate; not app-attributed impact',
            'raw_rows': len(samples) + duplicate_rows,
            'exact_duplicate_rows': duplicate_rows,
            'unique_rows': len(samples), 'windows': reduced_windows}


def receipt_phase_boundaries(receipt):
    boundaries = receipt.get('phase_boundaries')
    expected = [(name, event) for name, _ in ENERGY_PHASES for event in ('begin', 'end')]
    observed = [(boundary.get('phase'), boundary.get('event')) for boundary in boundaries or []
                if boundary.get('phase') in dict(ENERGY_PHASES)]
    if observed != expected:
        raise ValueError('receipt phase boundaries must contain one ordered begin/end pair for every phase')
    selected = [boundary for boundary in boundaries if boundary.get('phase') in dict(ENERGY_PHASES)]
    try:
        times = [float(boundary['time']) for boundary in selected]
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError('receipt phase boundaries require finite timestamps') from error
    if not all(math.isfinite(time) for time in times) or any(later < earlier for earlier, later in zip(times, times[1:])):
        raise ValueError('receipt phase boundary timestamps are disordered')
    pairs = {}
    for index, (name, seconds) in enumerate(ENERGY_PHASES):
        begin, end = selected[index * 2:index * 2 + 2]
        duration = float(end['time']) - float(begin['time'])
        if duration < seconds - MAX_EARLY_PHASE_SECONDS:
            raise ValueError(f'{name} receipt duration {duration:.9f}s is shorter than the {seconds}s protocol')
        pairs[(name, 'Begin')] = begin
        pairs[(name, 'End')] = end
    return pairs


def phase_windows(signposts, toc, app, pid, receipt=None):
    """Build the five protocol windows from one exact, PID-owned signpost stream."""
    trace = Trace(signposts, toc)
    events = [event for event in trace.signs(app) if event['name'] in dict(ENERGY_PHASES)]
    expected = [(name, kind) for name, _ in ENERGY_PHASES for kind in ('Begin', 'End')]
    observed = [(event['name'], event['type']) for event in events]
    if any(process_pid(event['process']) != str(pid) for event in events):
        raise ValueError('energy phase signposts do not all belong to the expected PID')
    if events and any(event['process'] != events[0]['process'] for event in events):
        raise ValueError('energy phase signposts do not share one process')
    if len(set(observed)) != len(observed):
        raise ValueError('energy phase signposts contain duplicate phase marker identities')
    try:
        positions = [expected.index(identity) for identity in observed]
    except ValueError as error:
        raise ValueError('energy phase signposts contain an unexpected phase marker') from error
    if positions != sorted(positions):
        raise ValueError('energy phase signposts are disordered')
    if observed != expected and len(events) >= len(expected):
        raise ValueError('energy phase signposts must contain one ordered Begin/End pair for every phase')
    if observed != expected:
        return calibrated_receipt_windows(events, receipt)
    if any(later['ns'] < earlier['ns'] for earlier, later in zip(events, events[1:])):
        raise ValueError('energy phase signpost timestamps are disordered')
    windows = []
    for index, (name, seconds) in enumerate(ENERGY_PHASES):
        begin, end = events[index * 2:index * 2 + 2]
        if end['ns'] <= begin['ns']:
            raise ValueError(f'{name} signpost end must follow begin')
        duration = (end['ns'] - begin['ns']) / NANOSECONDS_PER_SECOND
        if duration < seconds - MAX_EARLY_PHASE_SECONDS:
            raise ValueError(f'{name} duration {duration:.9f}s is shorter than the {seconds}s protocol')
        windows.append({'name': name, 'start_ns': begin['ns'], 'stop_ns': end['ns'],
                        'target_seconds': seconds, 'duration_deviation_seconds': duration - seconds,
                        'window_source': 'signposts', 'observed_marker_count': len(events),
                        'missing_marker_count': 0, 'offset_spread_ns': None})
    return windows


def calibrated_receipt_windows(events, receipt):
    """Map complete receipt boundaries only through surviving source-clock markers."""
    if receipt is None:
        raise ValueError('incomplete phase signposts require a calibrated receipt')
    boundaries = receipt_phase_boundaries(receipt)
    if len(events) < 2 or any(event['source'] is None or not math.isfinite(event['source']) for event in events):
        raise ValueError('incomplete phase signposts require at least two source-clock calibration markers')
    offsets = [event['ns'] - event['source'] * NANOSECONDS_PER_SECOND for event in events]
    offset = sorted(offsets)[len(offsets) // 2]
    spread = max(offsets) - min(offsets)
    if spread > MAX_CALIBRATION_SPREAD_NS:
        raise ValueError('receipt-to-trace calibration offset spread exceeds 100 microseconds')
    for event in events:
        boundary = boundaries.get((event['name'], event['type']))
        if boundary is None:
            raise ValueError('surviving phase marker has no matching receipt boundary')
        mapped = boundary['time'] * NANOSECONDS_PER_SECOND + offset
        if abs(event['ns'] - mapped) > MAX_RECEIPT_ANCHOR_ERROR_NS:
            raise ValueError('surviving phase marker does not match its receipt boundary within 1 ms')
    windows = []
    for name, seconds in ENERGY_PHASES:
        begin, end = boundaries[(name, 'Begin')], boundaries[(name, 'End')]
        start = int(round(begin['time'] * NANOSECONDS_PER_SECOND + offset))
        stop = int(round(end['time'] * NANOSECONDS_PER_SECOND + offset))
        windows.append({'name': name, 'start_ns': start, 'stop_ns': stop,
                        'target_seconds': seconds,
                        'duration_deviation_seconds': (stop - start) / NANOSECONDS_PER_SECOND - seconds,
                        'window_source': 'receipt-calibrated-to-trace',
                        'observed_marker_count': len(events),
                        'missing_marker_count': len(ENERGY_PHASES) * 2 - len(events),
                        'offset_spread_ns': spread})
    return windows


def validate_receipt(receipt, run_id):
    """Validate receipt-only admission facts without converting its clock."""
    # Earlier energy builds used diagnostic-complete; preserve historical reduction.
    if receipt.get('status') not in ('energy-complete', 'diagnostic-complete') or receipt.get('mode') != 'energy':
        raise ValueError('receipt does not describe a successful energy run')
    if receipt.get('error') or receipt.get('run_id') != run_id:
        raise ValueError('receipt error or run_id does not match the requested energy run')
    if receipt.get('charging_or_plugged_at_end') or receipt.get('low_power_mode_at_end'):
        raise ValueError('receipt reports charging or low-power mode at completion')
    boundaries = receipt.get('phase_boundaries')
    receipt_phase_boundaries(receipt)
    if any(boundary.get('charging_or_plugged') or boundary.get('low_power_mode') for boundary in boundaries):
        raise ValueError('receipt phase boundary reports charging or low-power mode')
    return True


def reduce_energy_capture(xml, signposts, toc, receipt, app, pid, run_id):
    """Reduce a qualified energy capture using trace-clock phase signposts."""
    validate_receipt(receipt, run_id)
    return reduce_power_trace(xml, phase_windows(signposts, toc, app, pid, receipt), receipt_valid=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--xml', required=True)
    parser.add_argument('--windows', help='JSON file with explicit trace-clock windows')
    parser.add_argument('--signposts', help='full trace XML containing os-signpost')
    parser.add_argument('--toc', help='trace TOC used to resolve os-signpost rows')
    parser.add_argument('--receipt', help='successful energy receipt JSON')
    parser.add_argument('--app', help='expected app process name for signpost reduction')
    parser.add_argument('--pid', type=int, help='expected app PID for signpost reduction')
    parser.add_argument('--run-id', help='expected receipt run ID for signpost reduction')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    if args.windows:
        if any((args.signposts, args.toc, args.receipt, args.app, args.pid, args.run_id)):
            parser.error('--windows cannot be combined with signpost reduction arguments')
        result = reduce_power_trace(args.xml, json.loads(Path(args.windows).read_text()))
    else:
        if not all((args.signposts, args.toc, args.receipt, args.app, args.pid is not None, args.run_id)):
            parser.error('use --windows or all of --signposts, --toc, --receipt, --app, --pid and --run-id')
        result = reduce_energy_capture(args.xml, args.signposts, args.toc,
                                       json.loads(Path(args.receipt).read_text()), args.app,
                                       args.pid, args.run_id)
    Path(args.output).write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
