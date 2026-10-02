#!/usr/bin/env python3
"""One fixed CoreComparison capture, or its five-pair scorecard. Standard library only."""
import argparse
import json
import math
import re
import statistics
from pathlib import Path
import xml.etree.ElementTree as ET

CASES = ('shapes', 'text', 'images', 'local', 'animation', 'scroll')


def distribution(values):
    values = sorted(values)
    if not values:
        return None
    def percentile(p):
        index = (len(values) - 1) * p
        lo, hi = math.floor(index), math.ceil(index)
        return values[lo] + (values[hi] - values[lo]) * (index - lo)
    return {'count': len(values), 'p50': percentile(.5), 'p95': percentile(.95), 'max': values[-1]}


def metadata_number(message, key):
    match = re.search(r'\b' + re.escape(key) + r'=\s*([-+]?\d[\d,]*(?:\.\d+)?(?:[eE][-+]?\d+)?)', message)
    return float(match.group(1).replace(',', '')) if match else None


def process_pid(process):
    match = re.search(r'\((\d+)\)\s*$', process)
    return match.group(1) if match else None


class Trace:
    def __init__(self, xml, toc):
        self.root = ET.parse(xml).getroot()
        self.ids = {e.get('id'): e for e in self.root.iter() if e.get('id')}
        tables = ET.parse(toc).getroot().findall('.//run/data/table')
        self.tables = {}
        for node in self.root.findall('node'):
            index = int(re.search(r'table\[(\d+)\]', node.get('xpath')).group(1)) - 1
            schema = tables[index].get('schema')
            self.tables.setdefault(schema, []).extend(node.findall('row'))

    def deref(self, element):
        seen = set()
        while element.get('ref'):
            ref = element.get('ref')
            if ref in seen:
                raise ValueError('cyclic Instruments reference')
            seen.add(ref)
            element = self.ids[ref]
        return element

    def raw(self, element):
        element = self.deref(element)
        # Metadata fmt rounds timestamps and inserts separators; raw child values
        # retain the host-clock precision needed for scheduled-update latency.
        return (element.text or '') + ''.join(self.raw(child) + (child.tail or '') for child in element)

    def fmt(self, element):
        element = self.deref(element)
        return element.get('fmt', self.raw(element))

    def number(self, element):
        return int(self.raw(element).strip())

    def signs(self, app):
        signs = []
        for row in self.tables.get('os-signpost', []):
            c = list(row)
            process = self.fmt(c[2])
            if self.fmt(c[9]) != 'com.oxide.compare-core' or not process.startswith(app + ' ('):
                continue
            message = self.raw(c[11])
            signs.append({'ns': self.number(c[0]), 'process': process,
                          'type': self.fmt(c[3]), 'name': self.fmt(c[6]),
                          'generation': metadata_number(message, 'generation'),
                          'scheduled': metadata_number(message, 'scheduled'),
                          'source': metadata_number(message, 'source'),
                          'presented': metadata_number(message, 'time'),
                          'gpu_frame': metadata_number(message, 'frame'),
                          'gpu_ms': metadata_number(message, 'ms'),
                          'memory_bytes': metadata_number(message, 'bytes')})
        return signs


def summarize_process_cpu(trace, process, start, stop, mutations):
    """Return process Running time as core-ms, never wall-clock utilization.

    Thread-state intervals can overlap after export retries or scheduler changes.
    Unioning each thread before summing keeps a duplicated interval from becoming
    invented CPU work, while concurrent threads intentionally contribute core-ms.
    """
    rows = trace.tables.get('thread-state')
    if rows is None:
        return {'valid': False, 'reason': 'thread-state table unavailable'}
    by_thread = {}
    expected_pid = process_pid(process)
    malformed = 0
    for row in rows:
        c = list(row)
        if len(c) < 5 or trace.fmt(c[2]) != 'Running':
            continue
        owner = trace.fmt(c[4])
        thread = trace.fmt(c[1])
        owner_matches = owner == process
        thread_matches = expected_pid is not None and re.search(r'\bpid:\s*' + re.escape(expected_pid) + r'\b', thread) is not None
        if not owner_matches and not (not owner and thread_matches):
            continue
        try:
            lo = trace.number(c[0])
            hi = lo + trace.number(c[3])
        except (ValueError, TypeError):
            malformed += 1
            continue
        lo, hi = max(start, lo), min(stop, hi)
        if hi > lo:
            by_thread.setdefault(thread, []).append((lo, hi))
    if not by_thread:
        return {'valid': False, 'reason': 'no target-process Running intervals', 'malformed_intervals': malformed}
    total_ns = 0
    intervals = 0
    for values in by_thread.values():
        last_lo = last_hi = None
        for lo, hi in sorted(values):
            if last_hi is None:
                last_lo, last_hi = lo, hi
            elif lo <= last_hi:
                last_hi = max(last_hi, hi)
            else:
                total_ns += last_hi - last_lo
                intervals += 1
                last_lo, last_hi = lo, hi
        total_ns += last_hi - last_lo
        intervals += 1
    total_ms = total_ns / 1e6
    return {'valid': malformed == 0, 'scope': 'target-process thread-state Running intervals, clipped to CoreRun and unioned per thread',
            'app_process_running_ms': total_ms,
            'average_cpu_cores': total_ns / (stop - start),
            'app_process_running_ms_per_mutation': total_ms / mutations if mutations else None,
            'running_threads': len(by_thread), 'unioned_intervals': intervals,
            'malformed_intervals': malformed}


def reduce_trace(xml, toc, app, case, completion, *, trace=None, signs=None, include_events=False):
    trace = trace if trace is not None else Trace(xml, toc)
    signs = trace.signs(app) if signs is None else signs
    begin = [s for s in signs if s['name'] == 'CoreRun' and s['type'] == 'Begin']
    end = [s for s in signs if s['name'] == 'CoreRun' and s['type'] == 'End']
    if len(begin) != 1 or len(end) != 1:
        return {'valid_capture': False, 'reason': 'exactly one CoreRun begin/end is required'}
    start, stop = begin[0]['ns'], end[0]['ns']
    process = begin[0]['process']
    mutations = [s for s in signs if s['name'] == 'CoreMutation' and start <= s['ns'] <= stop]
    submissions = [s for s in signs if s['name'] == 'CoreSubmitted' and start <= s['ns'] <= stop]
    by_generation = {s['generation']: s for s in mutations}
    duration = (stop - start) / 1e9
    measured_seconds = completion.get('measured_seconds', 20)
    valid = (measured_seconds - .25 <= duration <= measured_seconds + .25 and len(mutations) == completion['updates']
             and len(by_generation) == len(mutations) and end[0]['process'] == process)
    swaps = {}
    display_times = []
    for row in trace.tables.get('display-surface-swap', []):
        c = list(row)
        timestamp = trace.number(c[0])
        if start <= timestamp <= stop:
            display_times.append(timestamp)
        # Metal swaps may have no compositor surface identity. They still
        # provide system-wide diagnostic timing, but cannot establish app ownership.
        # The exported swap row carries frame ID at column 5 and surface ID at
        # column 3. Column 12 is optional metadata, not the surface identity.
        if trace.raw(c[5]).strip() and trace.raw(c[3]).strip():
            key = (trace.number(c[5]), trace.number(c[3]))
            swaps.setdefault(key, []).append(timestamp)
    display_times = sorted(set(display_times))
    system_cadence = distribution([(b-a)/1e6 for a, b in zip(display_times, display_times[1:])])
    hitches = []
    for row in trace.tables.get('hitches', []):
        c = list(row)
        if trace.fmt(c[2]) == process and trace.number(c[3]) == 0:
            lo = max(start, trace.number(c[0]))
            hi = min(stop, trace.number(c[0]) + trace.number(c[1]))
            if hi > lo:
                hitches.append((hi-lo)/1e6)
    latency_events = {}
    presentation_events = []
    presentation_identities = set()
    ambiguous_identities = set()
    unresolved_transactions = []
    def record_presentation(identity, generation, timestamp, latency=None):
        if identity is None:
            unresolved_transactions.append('missing-generation')
            return
        if identity in presentation_identities:
            ambiguous_identities.add(identity)
        presentation_identities.add(identity)
        presentation_events.append({'identity': identity, 'generation': generation, 'timestamp': timestamp})
        if latency is not None:
            latency_events.setdefault(generation, []).append((timestamp, latency))
    excluded_zero = 0
    if app == 'OxideBenchIOS':
        offsets = [event['ns'] - event['source'] * 1e9 for event in mutations if event['source'] is not None]
        trace_offset = statistics.median(offsets) if offsets else None
        for event in signs:
            if event['name'] != 'CorePresented':
                continue
            presented = event['presented']
            if presented is None or presented <= 0:
                excluded_zero += 1
                continue
            if trace_offset is None:
                unresolved_transactions.append('no-common-clock-mapping')
                continue
            timestamp = int(round(presented * 1e9 + trace_offset))
            identity = ('presented', event['generation'], timestamp)
            mutation = by_generation.get(event['generation'])
            latency = None
            if mutation is not None and mutation['scheduled'] is not None:
                candidate = (presented - mutation['scheduled']) * 1000
                if 0 <= candidate < 5000:
                    latency = candidate
            record_presentation(identity, event['generation'], timestamp, latency)
        attribution = 'app drawable presentedTime mapped to the trace host clock from CoreMutation source timestamps; zero timestamps excluded'
    else:
        seen_transactions = set()
        for row in trace.tables.get('hitches-updates', []):
            c = list(row)
            if trace.fmt(c[2]) != process:
                continue
            lo, hi = trace.number(c[0]), trace.number(c[0]) + trace.number(c[1])
            frame, surface = trace.number(c[4]), trace.number(c[5])
            # Instruments exports update and hitch phase rows for the same
            # transaction. Their color/phase metadata is not another presentation.
            transaction = (lo, hi, process, trace.fmt(c[3]), frame, surface)
            if transaction in seen_transactions:
                continue
            seen_transactions.add(transaction)
            times = swaps.get((frame, surface), [])
            if len(times) != 1:
                if start <= hi and lo <= stop:
                    unresolved_transactions.append('swap-identity-not-exactly-one')
                continue
            # App ownership comes from the transaction's PID and exact frame /
            # surface identity, independently of how many mutations it contains.
            ms = [m for m in mutations if lo <= m['ns'] <= hi]
            ss = [s for s in submissions if lo <= s['ns'] <= hi]
            mutation = None
            if len(ms) == len(ss) == 1:
                if ms[0]['generation'] == ss[0]['generation']:
                    mutation = ms[0]
                else:
                    unresolved_transactions.append('mutation-submission-generation-mismatch')
            elif start <= times[0] <= stop:
                unresolved_transactions.append('mutation-submission-not-one-to-one')
            latency = None
            if mutation is not None and mutation['source'] is not None and mutation['scheduled'] is not None:
                candidate = (times[0]-mutation['ns'])/1e6 + (mutation['source']-mutation['scheduled'])*1000
                if 0 <= candidate < 5000:
                    latency = candidate
            record_presentation(('swap', frame, surface, times[0]), mutation['generation'] if mutation else None, times[0], latency)
        attribution = 'app-PID transaction joined to exact swap/frame/surface identity; latency additionally requires one matching mutation/submission'
    gpu_samples = {}
    for event in signs:
        if (event['name'] == 'CoreGPU' and start <= event['ns'] <= stop
                and event['gpu_frame'] and event['gpu_ms'] and event['gpu_ms'] > 0):
            gpu_samples.setdefault(event['gpu_frame'], event['gpu_ms'])
    memory_samples = [int(event['memory_bytes']) for event in signs
                      if event['name'] == 'CoreMemory' and event['process'] == process
                      and start <= event['ns'] <= stop and event.get('memory_bytes') is not None
                      and event['memory_bytes'] >= 0]
    memory = {'valid': bool(memory_samples), 'scope': 'sampled app TASK_VM_INFO physical footprint; not guaranteed peak',
              'units': 'bytes', 'samples': len(memory_samples),
              'sampled_max_bytes': max(memory_samples) if memory_samples else None,
              'distribution': distribution(memory_samples)}
    discrete = case in ('text', 'local') or case.startswith('visual-')
    covered = len(latency_events)
    window_presentations = sorted(event['timestamp'] for event in presentation_events if start <= event['timestamp'] <= stop)
    window_monotonic = all(b > a for a, b in zip(window_presentations, window_presentations[1:]))
    # This is deliberately an observed presentation stream only. The trace
    # proves neither that every app drawable is visible here nor that every
    # source mutation reached it; those coverage claims belong to latency.
    cadence = distribution([(b-a)/1e6 for a, b in zip(window_presentations, window_presentations[1:])]) if not ambiguous_identities and window_monotonic else None
    drain_stop = stop + 5_000_000_000
    source_generations = {s['generation'] for s in mutations if s['generation'] is not None and s['scheduled'] is not None}
    source_latency = {}
    for generation, values in latency_events.items():
        values = [(timestamp, latency) for timestamp, latency in values if start <= timestamp <= drain_stop]
        if len(values) == 1:
            source_latency[generation] = values[0][1]
        elif len(values) > 1:
            unresolved_transactions.append('source-generation-presented-more-than-once')
    actions = [event for event in signs if event['name'] == 'CoreAction' and event['process'] == process and start <= event['ns'] <= stop]
    action_latencies = []
    missing_actions = []
    for action in actions:
        candidates = latency_events.get(action['generation'], [])
        if len(candidates) == 1 and action['source'] is not None and action['scheduled'] is not None:
            latency = (candidates[0][0] - action['ns']) / 1e6 + (action['source'] - action['scheduled']) * 1000
            if 0 <= latency < 5000:
                action_latencies.append(latency)
                continue
        missing_actions.append(action['generation'])
    transition_coverage = {'declared_actions': len(actions), 'attributed_actions': len(action_latencies),
                           'missing_generations': missing_actions,
                           'expected_actions': math.ceil(measured_seconds / 2) if case.startswith('visual-') else None}
    source_cohort_complete = (source_generations == set(source_latency) and len(source_generations) == len(mutations)
                              and completion.get('skipped_updates', 0) == 0)
    source_cohort_latency = distribution(list(source_latency.values())) if source_cohort_complete else None
    cadence_valid = cadence is not None and len(window_presentations) >= 2
    unavailable = []
    if not source_cohort_complete and discrete:
        unavailable.append('scheduled latency headline requires every scheduled update to be attributable')
    if not cadence_valid:
        unavailable.append('app presentation cadence requires at least two unique, monotonic presentation identities')
    unavailable.append('cross-framework hitch time unavailable: Metal app-hitch classification was not calibrated')
    cpu = summarize_process_cpu(trace, process, start, stop, len(mutations))
    result = {'schema_version': 2, 'valid_capture': valid, 'case': case, 'app': app,
            'process': process, 'duration_seconds': duration, 'mutations': len(mutations),
            'skipped_updates': completion.get('skipped_updates', 0),
            'presentation_interval_ms': cadence, 'window_presentation_interval_ms': cadence,
            'displayed_swaps': len(display_times),
            'cadence_attribution': attribution,
            'system_display_interval_ms_diagnostic': system_cadence,
            'attributed_presentation_interval_ms_diagnostic': cadence,
            'ambiguous_presentation_generations': sorted({event['generation'] for event in presentation_events if event['identity'] in ambiguous_identities and event['generation'] is not None}),
            'ambiguous_presentation_identities': [list(identity) for identity in sorted(ambiguous_identities)],
            'unresolved_transactions': sorted(set(unresolved_transactions)),
            'missing_presentation_generations': sorted(set(by_generation) - set(source_latency)),
            'presentation_generations_monotonic': window_monotonic,
            'refresh': {key: completion.get(key) for key in (
                'screen_maximum_fps', 'requested_fps', 'display_link_scheduled_120hz_ticks',
                'display_link_scheduled_other_ticks')},
            'uikit_app_hitch_time_ms_diagnostic': sum(hitches) if app == 'UIKitBenchIOS' else None,
            'scheduled_update_to_present_ms': source_cohort_latency if discrete else None,
            'source_cohort_update_to_present_ms': source_cohort_latency,
            'attributed_latency_ms_diagnostic': distribution(list(source_latency.values())) if discrete else None,
            'latency_attribution': attribution, 'attributed_updates': covered,
            'transition_latency_ms': distribution(action_latencies), 'transition_coverage': transition_coverage,
            'zero_presented_timestamps_excluded': excluded_zero,
            'validity': {'capture': {'valid': valid, 'scope': 'single CoreRun duration and source mutation accounting'},
                         'presentation_cadence': {'valid': cadence_valid, 'coverage': 'observed-stream-only; complete app-stream coverage is unknown', 'scope': 'unique identity-proven actual presentation timestamps inside CoreRun; independent of source mutation coverage'},
                         'window_presentation_cadence': {'valid': cadence_valid, 'coverage': 'observed-stream-only; complete app-stream coverage is unknown', 'scope': 'unique identity-proven actual presentation timestamps inside CoreRun'},
                         'source_cohort_latency': {'valid': source_cohort_latency is not None, 'scope': 'every scheduled source mutation in CoreRun has one attributable presentation by five-second post-window drain'},
                         'app_process_cpu': {'valid': cpu['valid'], 'scope': cpu.get('scope', cpu.get('reason'))},
                         'energy': {'valid': False, 'scope': 'unavailable: no Power Profiler schema or physical-unit parser'}},
            'cpu': cpu, 'memory': memory, 'gpu': distribution(list(gpu_samples.values())),
            'gpu_scope': 'latest completed command-buffer samples observed during measurement; boundary frames may precede measurement; not a UIKit comparison',
            'unavailable': unavailable}
    if include_events:
        result['_presentation_events'] = presentation_events
        result['_latency_events'] = latency_events
    return result


def probe_signature(event):
    return tuple(event.get(key) for key in ('ns', 'process', 'type', 'name', 'generation',
                                             'scheduled', 'source', 'presented'))


def reduce_probe_trace(xml, toc, app, completion, pid, run_id, normal_metrics=None):
    """Reduce one PID-owned presentation probe without changing legacy captures."""
    trace = Trace(xml, toc)
    events = [event for event in trace.signs(app) if process_pid(event['process']) == str(pid)]
    unique, duplicate_rows = [], 0
    seen = set()
    for event in events:
        signature = probe_signature(event)
        if signature in seen:
            duplicate_rows += 1
        else:
            seen.add(signature)
            unique.append(event)
    begin = [event for event in unique if event['name'] == 'CoreProbe' and event['type'] == 'Begin']
    end = [event for event in unique if event['name'] == 'CoreProbe' and event['type'] == 'End']
    if len(begin) != 1 or len(end) != 1 or end[0]['ns'] <= begin[0]['ns']:
        return {'valid_capture': False, 'reason': 'exactly one PID-owned CoreProbe begin/end is required',
                'duplicate_event_rows': duplicate_rows}
    start, stop = begin[0]['ns'], end[0]['ns']
    window = [event for event in unique if start <= event['ns'] <= stop]
    duration = (stop - start) / 1e9
    measured = completion.get('measured_seconds')
    receipt_valid = (completion.get('status') == 'diagnostic-complete' and not completion.get('error')
                     and completion.get('run_id') == run_id and measured == 20
                     and type(completion.get('injected_delay')) is bool)
    if not receipt_valid:
        return {'valid_capture': False, 'reason': 'probe receipt must match successful run ID and twenty-second duration'}
    mutations = [event for event in window if event['name'] == 'ProbeMutation']
    submissions = [event for event in window if event['name'] == 'ProbeUpdateSubmitted']
    presentations = [event for event in window if event['name'] == 'OxidePresented']
    def conflicting(events_for_name):
        by_generation = {}
        for event in events_for_name:
            if event['generation'] is not None:
                by_generation.setdefault(event['generation'], []).append(event)
        return sorted(generation for generation, values in by_generation.items() if len(values) != 1)
    ambiguous_generations = sorted(set(conflicting(mutations) + conflicting(submissions) + conflicting(presentations)))
    mutation_by_generation = {event['generation']: event for event in mutations
                              if event['generation'] is not None and event['generation'] not in ambiguous_generations}
    names = {'CoreProbe': 'CoreRun', 'ProbeMutation': 'CoreMutation',
             'ProbeUpdateSubmitted': 'CoreSubmitted', 'OxidePresented': 'CorePresented'}
    normalized = [{**event, 'name': names.get(event['name'], event['name'])} for event in unique]
    shared = reduce_trace(xml, toc, app, 'probe', completion, trace=trace, signs=normalized, include_events=True)
    presentation_events = sorted((event for event in shared.pop('_presentation_events')
                                   if start <= event['timestamp'] <= stop), key=lambda event: event['timestamp'])
    latency_events = shared.pop('_latency_events')
    latency_by_generation = {generation: values[0] for generation, values in latency_events.items()
                             if len(values) == 1 and start <= values[0][0] <= stop + 5_000_000_000}
    cadence = shared['presentation_interval_ms']
    cadence_valid = shared['validity']['presentation_cadence']['valid']
    mutation_generations = {event['generation'] for event in mutations if event['generation'] is not None}
    missing_generations = sorted(mutation_generations - set(latency_by_generation))
    delayed = completion.get('injected_delay') is True
    delays = [event for event in window if event['name'] == 'InjectedDelay']
    delay_valid = not delayed and not delays
    stall_reason = 'normal probe must contain no InjectedDelay'
    stall_evidence = None
    if delayed:
        delay_begin = [event for event in delays if event['type'] == 'Begin']
        delay_end = [event for event in delays if event['type'] == 'End']
        delay_valid = (len(delay_begin) == len(delay_end) == 1
                       and start < delay_begin[0]['ns'] < delay_end[0]['ns'] < stop)
        stall_reason = 'InjectedDelay begin/end is missing or ambiguous'
        baseline_valid = (normal_metrics and normal_metrics.get('app') == app
                          and not normal_metrics.get('injected_delay')
                          and normal_metrics.get('valid_capture')
                          and normal_metrics.get('validity', {}).get('presentation_cadence', {}).get('valid')
                          and normal_metrics.get('validity', {}).get('stall_detection', {}).get('valid'))
        if delay_valid and baseline_valid:
            before_delay = [event for event in mutations if event['ns'] <= delay_begin[0]['ns']
                            and event['generation'] in mutation_by_generation]
            delayed_generation = max(before_delay, key=lambda event: event['ns'])['generation'] if before_delay else None
            normal_latency = (normal_metrics.get('attributed_latency_ms') or {}).get('p50')
            delayed_latency = (delayed_generation in latency_by_generation and normal_latency is not None
                               and latency_by_generation[delayed_generation][0] > delay_end[0]['ns']
                               and latency_by_generation[delayed_generation][1] - normal_latency >= 80)
            median = (normal_metrics.get('presentation_interval_ms') or {}).get('p50')
            def valid_gap(earlier, later):
                sequential = (earlier['generation'] is not None
                              and later['generation'] == earlier['generation'] + 1
                              and earlier['generation'] in latency_by_generation
                              and later['generation'] in latency_by_generation)
                return (cadence_valid and sequential
                        and earlier['timestamp'] <= delay_begin[0]['ns']
                        and later['timestamp'] >= delay_end[0]['ns']
                        and (later['timestamp'] - earlier['timestamp']) / 1e6 - median >= 80)
            gaps = [(earlier, later) for earlier, later in zip(presentation_events, presentation_events[1:])
                    if valid_gap(earlier, later)] if median is not None else []
            delay_valid = delayed_latency or bool(gaps)
            stall_evidence = {'injected_duration_ms': (delay_end[0]['ns'] - delay_begin[0]['ns']) / 1e6,
                              'generation': delayed_generation,
                              'latency_excess_ms': latency_by_generation[delayed_generation][1] - normal_latency if delayed_latency else None,
                              'gap_excess_ms': [(later['timestamp'] - earlier['timestamp']) / 1e6 - median for earlier, later in gaps]}
            stall_reason = 'PID-attributed delayed latency or >=80 ms excess presentation gap required'
        elif delay_valid:
            delay_valid = False
            stall_reason = 'delayed probe requires normal metrics'
    valid_capture = (receipt_valid and shared['valid_capture'] and not ambiguous_generations)
    return {'schema_version': 3, 'valid_capture': valid_capture, 'app': app, 'injected_delay': delayed, 'process': begin[0]['process'],
            'duration_seconds': duration, 'mutations': len(mutations), 'duplicate_event_rows': duplicate_rows,
            'stall_evidence': stall_evidence,
            'ambiguous_generations': ambiguous_generations, 'missing_presentation_generations': missing_generations,
            'unresolved_transactions': shared['unresolved_transactions'],
            'ambiguous_presentation_identities': shared['ambiguous_presentation_identities'],
            'presentation_interval_ms': cadence, 'attributed_latency_ms': distribution([value[1] for value in latency_by_generation.values()]),
            'validity': {'capture': {'valid': valid_capture, 'scope': 'one PID-owned CoreProbe and matching receipt'},
                         'presentation_cadence': shared['validity']['presentation_cadence'],
                         'stall_detection': {'valid': delay_valid, 'scope': stall_reason}}}


def summarize(directory):
    directory = Path(directory)
    runs = json.loads((directory/'runs.json').read_text())
    identity = json.loads((directory/'identity.json').read_text())
    visuals = identity.get('visual_evidence', {}).get('cases', {})
    score = []
    for case in CASES:
        pairs = [p for p in runs if p['case'] == case and p['accepted']]
        metric = 'scheduled_update_to_present_ms' if case in ('text', 'local') else 'presentation_interval_ms'
        deltas, relative = [], []
        for pair in pairs:
            sides = {r['side']: r['metrics'].get(metric) for r in pair['runs']}
            if sides.get('oxide') and sides.get('uikit'):
                ox, ui = sides['oxide']['p95'], sides['uikit']['p95']
                deltas.append(ox-ui)
                relative.append((ox-ui)/ui if ui else 0)
        outcome = 'unavailable'
        if len(pairs) == len(deltas) == 5:
            outcome = 'unresolved'
            if all(d < 0 for d in deltas) and statistics.median(relative) <= -.05:
                outcome = 'Oxide lower p95 in all five pairs'
            elif all(d > 0 for d in deltas) and statistics.median(relative) >= .05:
                outcome = 'UIKit lower p95 in all five pairs'
        if metric == 'presentation_interval_ms' and any(
                r['metrics'].get('schema_version', 1) >= 2 for p in pairs for r in p['runs']):
            outcome = 'observed cadence only; complete presentation coverage unverified'
        visual = visuals.get(case, {})
        reason = visual.get('reason', '') if visual.get('status') != 'passed' else ''
        failures = [p.get('error') for p in runs if p['case'] == case and not p['accepted']]
        if reason:
            outcome = 'visual equivalence blocked'
        elif len(pairs) < 5 and failures:
            reason = failures[-1]
        score.append({'reason': reason, 'case': case, 'accepted_pairs': len(pairs), 'metric': metric, 'result': outcome,
                      'paired_oxide_minus_uikit_p95_ms': deltas,
                      'median_paired_difference_ms': statistics.median(deltas) if deltas else None})
    (directory/'scorecard.json').write_text(json.dumps(score, indent=2)+'\n')
    lines = ['# Six-case iOS baseline', '',
             'Schema-2 cadence reports identity-proven observations with explicit completeness limits. Source-cohort latency requires complete update attribution. System-wide swaps are diagnostic only. These workload-specific results do not establish general renderer superiority.', '',
             '| Case | Accepted pairs | Metric | Result | Median Oxide − UIKit p95 (ms) |',
             '| --- | ---: | --- | --- | ---: |']
    for row in score:
        value = row['median_paired_difference_ms']
        lines.append(f"| {row['case']} | {row['accepted_pairs']}/5 | {row['metric']} | {row['result']} | {value if value is not None else 'unavailable'} |")
    lines += ['', *[f"- {row['case']}: {row['reason']}" for row in score if row['reason']]]
    lines += ['', 'A directional result requires all five paired differences to agree and a median relative difference of at least 5%. Otherwise the comparison is unresolved. Individual attempts, failures, timing distributions and coverage are retained in runs.json. Cross-framework hitch time, CPU, memory and GPU comparisons remain unavailable.', '']
    (directory/'scorecard.md').write_text('\n'.join(lines))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--summarize')
    parser.add_argument('--probe', action='store_true')
    parser.add_argument('--pid', type=int)
    parser.add_argument('--run-id')
    parser.add_argument('--normal-metrics')
    for name in ('xml', 'toc', 'app', 'case', 'completion', 'output'):
        parser.add_argument('--'+name)
    args = parser.parse_args()
    if args.summarize:
        summarize(args.summarize)
        return
    required = ('xml', 'toc', 'app', 'completion', 'output') if args.probe else ('xml', 'toc', 'app', 'case', 'completion', 'output')
    if not all(getattr(args, name) for name in required):
        parser.error(', '.join(required) + ' are required')
    completion = json.loads(Path(args.completion).read_text())
    if args.probe:
        if args.pid is None or not args.run_id:
            parser.error('--probe requires --pid and --run-id')
        normal = json.loads(Path(args.normal_metrics).read_text()) if args.normal_metrics else None
        result = reduce_probe_trace(args.xml, args.toc, args.app, completion, args.pid, args.run_id, normal)
    else:
        result = reduce_trace(args.xml, args.toc, args.app, args.case, completion)
    Path(args.output).write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
