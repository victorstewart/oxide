import json
import tempfile
import unittest
from pathlib import Path

from reduce_core_trace import Trace, metadata_number, reduce_probe_trace, reduce_trace, summarize


class TraceTests(unittest.TestCase):
    def fixture(self, app='UIKitBenchIOS', submitted=1, presented=None, end=True, thread_rows=''):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        def sign(ns, name, kind='Event', message=''):
            return f'<row><event-time>{ns}</event-time><thread/><process ref="p"/><event-type>{kind}</event-type><string/><id/><string>{name}</string><string/><sentinel/><string>com.oxide.compare-core</string><string/><os-log-metadata>{message}</os-log-metadata></row>'
        rows = sign(1000000000, 'CoreRun', 'Begin')
        rows += sign(2000000000, 'CoreMutation', message='generation=1 scheduled=101.900000000 source=102.000000000')
        rows += sign(2001000000, 'CoreSubmitted', message=f'generation={submitted}')
        if end:
            rows += sign(21000000000, 'CoreRun', 'End')
        if presented is not None:
            for value in presented:
                rows += sign(22000000000, 'CorePresented', message=f'generation=1 time={value}')
        update = '<row><start-time>1999000000</start-time><duration>11000000</duration><process ref="p"/><display/><uint32>7</uint32><uint32>8</uint32><color/><uint32/><string>0x7</string></row>'
        # Match the physical Instruments export: frame ID 7 / surface ID 8;
        # the later optional fields are empty sentinels.
        swap = '<row><start-time>2020000000</start-time><start-time/><display/><displayed-surface-swap>8</displayed-surface-swap><uint32/><displayed-surface-swap>7</displayed-surface-swap><depth/><string/><start-time/><start-time/><uint32/><start-time/><sentinel/><sentinel/></row>'
        # IDs deliberately cross table boundaries and point forward.
        xml = f'<trace-query-result><node xpath="//table[1]">{rows}</node><node xpath="//table[2]">{update}</node><node xpath="//table[3]">{swap}</node><node xpath="//table[4]">{thread_rows}</node><process id="p" fmt="{app} (42)"><pid>42</pid></process><process id="other" fmt="OtherApp (99)"><pid>99</pid></process></trace-query-result>'
        (root/'trace.xml').write_text(xml)
        (root/'toc.xml').write_text('<trace-toc><run><data><table schema="os-signpost"/><table schema="hitches-updates"/><table schema="display-surface-swap"/><table schema="thread-state"/></data></run></trace-toc>')
        return root

    def reduce(self, root, app='UIKitBenchIOS'):
        return reduce_trace(root/'trace.xml', root/'toc.xml', app, 'text', {'updates': 1, 'skipped_updates': 0})

    def probe_fixture(self, delayed=False, duplicate=False, conflicting=False, delay_outside=False):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        def sign(ns, name, kind='Event', message=''):
            return f'<row><event-time>{ns}</event-time><thread/><process ref="p"/><event-type>{kind}</event-type><string/><id/><string>{name}</string><string/><sentinel/><string>com.oxide.compare-core</string><string/><os-log-metadata>{message}</os-log-metadata></row>'
        rows = sign(1_000_000_000, 'CoreProbe', 'Begin')
        mutation = sign(2_000_000_000, 'ProbeMutation', message='generation=1 scheduled=1.900 source=2.000')
        rows += mutation + sign(2_010_000_000, 'ProbeUpdateSubmitted', message='generation=1')
        rows += sign(2_020_000_000, 'OxidePresented', message='generation=1 time=2.050')
        rows += sign(3_000_000_000, 'ProbeMutation', message='generation=2 scheduled=2.900 source=3.000')
        if delayed:
            lo, hi = (22_000_000_000, 22_100_000_000) if delay_outside else (3_010_000_000, 3_110_000_000)
            rows += sign(lo, 'InjectedDelay', 'Begin') + sign(hi, 'InjectedDelay', 'End')
        rows += sign(3_120_000_000, 'ProbeUpdateSubmitted', message='generation=2')
        rows += sign(3_130_000_000, 'OxidePresented', message=f'generation=2 time={"3.250" if delayed else "3.150"}')
        if duplicate:
            rows += mutation
        if conflicting:
            rows += sign(4_000_000_000, 'ProbeMutation', message='generation=1 scheduled=3.900 source=4.000')
        rows += sign(21_000_000_000, 'CoreProbe', 'End')
        (root/'trace.xml').write_text('<trace-query-result><node xpath="//table[1]">' + rows + '</node>'
                                      '<process id="p" fmt="OxideBenchIOS (42)"><pid>42</pid></process></trace-query-result>')
        (root/'toc.xml').write_text('<trace-toc><run><data><table schema="os-signpost"/></data></run></trace-toc>')
        return root

    def probe_receipt(self, delayed=False):
        return {'status': 'diagnostic-complete', 'error': '', 'run_id': 'probe-1',
                'measured_seconds': 20, 'updates': 2, 'injected_delay': delayed}

    def uikit_probe_fixture(self, delayed=False):
        root = self.probe_fixture(delayed=delayed)
        path = root/'trace.xml'
        xml = path.read_text().replace('OxideBenchIOS (42)', 'UIKitBenchIOS (42)')
        updates = ('<node xpath="//table[2]">'
                   '<row><start-time>2000000000</start-time><duration>11000000</duration><process ref="p"/><display/><uint32>7</uint32><uint32>8</uint32></row>'
                   '<row><start-time>3000000000</start-time><duration>121000000</duration><process ref="p"/><display/><uint32>9</uint32><uint32>8</uint32></row></node>')
        second = '3300000000' if delayed else '3130000000'
        swaps = ('<node xpath="//table[3]">'
                 '<row><start-time>2020000000</start-time><start-time/><display/><displayed-surface-swap>8</displayed-surface-swap><uint32/><displayed-surface-swap>7</displayed-surface-swap></row>'
                 f'<row><start-time>{second}</start-time><start-time/><display/><displayed-surface-swap>8</displayed-surface-swap><uint32/><displayed-surface-swap>9</displayed-surface-swap></row></node>')
        path.write_text(xml.replace('</node>', '</node>' + updates + swaps, 1))
        (root/'toc.xml').write_text('<trace-toc><run><data><table schema="os-signpost"/><table schema="hitches-updates"/><table schema="display-surface-swap"/></data></run></trace-toc>')
        return root

    def test_uikit_transaction_phase_rows_are_one_presentation(self):
        import xml.etree.ElementTree as ET
        root = self.uikit_probe_fixture()
        path = root/'trace.xml'
        tree = ET.parse(path)
        node = tree.getroot().findall('node')[1]
        duplicate = ET.fromstring(ET.tostring(node.find('row')))
        node.append(duplicate)
        tree.write(path)
        result = reduce_probe_trace(path, root/'toc.xml', 'UIKitBenchIOS', self.probe_receipt(), 42, 'probe-1')
        self.assertTrue(result['validity']['presentation_cadence']['valid'])
        self.assertEqual(result['presentation_interval_ms']['count'], 1)
        self.assertEqual(result['ambiguous_presentation_identities'], [])

    def test_sampled_physical_footprint_is_window_and_process_scoped(self):
        root = self.fixture()
        path = root/'trace.xml'
        def sample(ns, owner, size):
            return f'<row><event-time>{ns}</event-time><thread/><process ref="{owner}"/><event-type>Event</event-type><string/><id/><string>CoreMemory</string><string/><sentinel/><string>com.oxide.compare-core</string><string/><os-log-metadata>bytes={size}</os-log-metadata></row>'
        path.write_text(path.read_text().replace('</node>', sample(2000000000, 'p', 123456) + sample(0, 'p', 999999) + sample(2000000000, 'other', 999999) + '</node>', 1))
        result = self.reduce(root)
        self.assertTrue(result['memory']['valid'])
        self.assertEqual(result['memory']['samples'], 1)
        self.assertEqual(result['memory']['sampled_max_bytes'], 123456)
        self.assertIn('not guaranteed peak', result['memory']['scope'])

    def test_visual_action_latency_uses_action_deadline_and_reports_coverage(self):
        root = self.fixture()
        path = root/'trace.xml'
        action = '<row><event-time>2000000000</event-time><thread/><process ref="p"/><event-type>Event</event-type><string/><id/><string>CoreAction</string><string/><sentinel/><string>com.oxide.compare-core</string><string/><os-log-metadata>generation=1 scheduled=101.900 source=102.000</os-log-metadata></row>'
        path.write_text(path.read_text().replace('</node>', action + '</node>', 1))
        result = reduce_trace(path, root/'toc.xml', 'UIKitBenchIOS', 'visual-controls', {'updates': 1})
        self.assertAlmostEqual(result['transition_latency_ms']['p50'], 120)
        self.assertEqual(result['transition_coverage']['declared_actions'], 1)
        self.assertEqual(result['transition_coverage']['attributed_actions'], 1)
        self.assertEqual(result['transition_coverage']['expected_actions'], 10)

    def test_probe_normal_and_delayed_stall_detection(self):
        normal = self.probe_fixture()
        normal_result = reduce_probe_trace(normal/'trace.xml', normal/'toc.xml', 'OxideBenchIOS',
                                           self.probe_receipt(), 42, 'probe-1')
        self.assertTrue(normal_result['valid_capture'])
        self.assertTrue(normal_result['validity']['presentation_cadence']['valid'])
        self.assertTrue(normal_result['validity']['stall_detection']['valid'])
        delayed = self.probe_fixture(delayed=True)
        delayed_result = reduce_probe_trace(delayed/'trace.xml', delayed/'toc.xml', 'OxideBenchIOS',
                                            self.probe_receipt(True), 42, 'probe-1', normal_result)
        self.assertTrue(delayed_result['valid_capture'])
        self.assertTrue(delayed_result['validity']['stall_detection']['valid'])

    def test_probe_rejects_wrong_pid_and_delay_outside_window(self):
        root = self.probe_fixture()
        result = reduce_probe_trace(root/'trace.xml', root/'toc.xml', 'OxideBenchIOS', self.probe_receipt(), 99, 'probe-1')
        self.assertFalse(result['valid_capture'])
        root = self.probe_fixture(delayed=True, delay_outside=True)
        result = reduce_probe_trace(root/'trace.xml', root/'toc.xml', 'OxideBenchIOS', self.probe_receipt(True),
                                    42, 'probe-1', {'presentation_interval_ms': {'p50': 10}})
        self.assertFalse(result['validity']['stall_detection']['valid'])
        path = root/'trace.xml'
        path.write_text(path.read_text().replace('<event-time>22000000000</event-time>', '<event-time>1000000000</event-time>'))
        result = reduce_probe_trace(path, root/'toc.xml', 'OxideBenchIOS', self.probe_receipt(True),
                                    42, 'probe-1', {'presentation_interval_ms': {'p50': 10}})
        self.assertFalse(result['validity']['stall_detection']['valid'])

    def test_probe_deduplicates_only_identical_events_and_marks_conflicts(self):
        root = self.probe_fixture(duplicate=True)
        result = reduce_probe_trace(root/'trace.xml', root/'toc.xml', 'OxideBenchIOS', self.probe_receipt(), 42, 'probe-1')
        self.assertEqual(result['duplicate_event_rows'], 1)
        self.assertTrue(result['valid_capture'])
        root = self.probe_fixture(conflicting=True)
        result = reduce_probe_trace(root/'trace.xml', root/'toc.xml', 'OxideBenchIOS', self.probe_receipt(), 42, 'probe-1')
        self.assertIn(1, result['ambiguous_generations'])
        self.assertFalse(result['valid_capture'])

    def test_uikit_probe_uses_transaction_swaps_for_cadence_and_delay_gap(self):
        normal = self.uikit_probe_fixture()
        normal_result = reduce_probe_trace(normal/'trace.xml', normal/'toc.xml', 'UIKitBenchIOS',
                                           self.probe_receipt(), 42, 'probe-1')
        self.assertTrue(normal_result['validity']['presentation_cadence']['valid'])
        delayed = self.uikit_probe_fixture(delayed=True)
        delayed_result = reduce_probe_trace(delayed/'trace.xml', delayed/'toc.xml', 'UIKitBenchIOS',
                                            self.probe_receipt(True), 42, 'probe-1', normal_result)
        self.assertTrue(delayed_result['validity']['stall_detection']['valid'])

    def test_uikit_probe_unresolved_submission_cannot_prove_stall(self):
        normal = self.uikit_probe_fixture()
        baseline = reduce_probe_trace(normal/'trace.xml', normal/'toc.xml', 'UIKitBenchIOS', self.probe_receipt(), 42, 'probe-1')
        delayed = self.uikit_probe_fixture(delayed=True)
        path = delayed/'trace.xml'
        path.write_text(path.read_text().replace('<string>ProbeUpdateSubmitted</string>', '<string>MissingSubmission</string>'))
        result = reduce_probe_trace(path, delayed/'toc.xml', 'UIKitBenchIOS', self.probe_receipt(True), 42, 'probe-1', baseline)
        self.assertTrue(result['validity']['presentation_cadence']['valid'])
        self.assertFalse(result['validity']['stall_detection']['valid'])
        self.assertTrue(result['unresolved_transactions'])

    def test_probe_rejects_wrong_receipt_and_baseline(self):
        root = self.probe_fixture()
        receipt = self.probe_receipt()
        receipt['measured_seconds'] = None
        result = reduce_probe_trace(root/'trace.xml', root/'toc.xml', 'OxideBenchIOS', receipt, 42, 'probe-1')
        self.assertFalse(result['valid_capture'])
        baseline = reduce_probe_trace(root/'trace.xml', root/'toc.xml', 'OxideBenchIOS', self.probe_receipt(), 42, 'probe-1')
        baseline['app'] = 'UIKitBenchIOS'
        delayed = self.probe_fixture(delayed=True)
        result = reduce_probe_trace(delayed/'trace.xml', delayed/'toc.xml', 'OxideBenchIOS', self.probe_receipt(True), 42, 'probe-1', baseline)
        self.assertFalse(result['validity']['stall_detection']['valid'])

    def test_probe_rejects_fast_delay_and_missing_callback_gap(self):
        normal = self.probe_fixture()
        normal_result = reduce_probe_trace(normal/'trace.xml', normal/'toc.xml', 'OxideBenchIOS',
                                           self.probe_receipt(), 42, 'probe-1')
        delayed = self.probe_fixture(delayed=True)
        path = delayed/'trace.xml'
        path.write_text(path.read_text().replace('time=3.250', 'time=3.151'))
        result = reduce_probe_trace(path, delayed/'toc.xml', 'OxideBenchIOS', self.probe_receipt(True),
                                    42, 'probe-1', normal_result)
        self.assertFalse(result['validity']['stall_detection']['valid'])
        path.write_text(path.read_text().replace('<string>OxidePresented</string>', '<string>OtherCallback</string>'))
        result = reduce_probe_trace(path, delayed/'toc.xml', 'OxideBenchIOS', self.probe_receipt(True),
                                    42, 'probe-1', normal_result)
        self.assertFalse(result['validity']['stall_detection']['valid'])

    def test_uikit_probe_cadence_does_not_require_mutation_latency(self):
        root = self.uikit_probe_fixture()
        path = root/'trace.xml'
        path.write_text(path.read_text().replace('<string>ProbeMutation</string>', '<string>OtherEvent</string>'))
        result = reduce_probe_trace(path, root/'toc.xml', 'UIKitBenchIOS', self.probe_receipt(), 42, 'probe-1')
        self.assertTrue(result['validity']['presentation_cadence']['valid'])

    def test_app_containment_and_scheduled_delay(self):
        result = self.reduce(self.fixture())
        self.assertTrue(result['valid_capture'])
        self.assertEqual(result['attributed_updates'], 1)
        self.assertAlmostEqual(result['scheduled_update_to_present_ms']['p95'], 120)

    def test_different_submission_generation_is_not_a_match(self):
        result = self.reduce(self.fixture(submitted=2))
        self.assertTrue(result['valid_capture'])
        self.assertIsNone(result['scheduled_update_to_present_ms'])

    def test_surface_identity_does_not_come_from_optional_swap_metadata(self):
        root = self.fixture()
        path = root/'trace.xml'
        path.write_text(path.read_text().replace('<sentinel/><sentinel/></row>', '<uint32>99</uint32><sentinel/></row>'))
        result = self.reduce(root)
        self.assertEqual(result['attributed_updates'], 1)
        self.assertAlmostEqual(result['scheduled_update_to_present_ms']['p95'], 120)

    def test_zero_presented_time_is_excluded_and_drain_is_allowed(self):
        result = self.reduce(self.fixture('OxideBenchIOS', presented=['0.000000000', '102.050000000']), 'OxideBenchIOS')
        self.assertEqual(result['zero_presented_timestamps_excluded'], 1)
        self.assertAlmostEqual(result['scheduled_update_to_present_ms']['p95'], 150)

    def test_missing_surface_identity_does_not_invent_latency(self):
        root = self.fixture()
        path = root/'trace.xml'
        path.write_text(path.read_text().replace('<displayed-surface-swap>8</displayed-surface-swap>', '<sentinel/>'))
        result = self.reduce(root)
        self.assertTrue(result['valid_capture'])
        self.assertEqual(result['displayed_swaps'], 1)
        self.assertEqual(result['attributed_updates'], 0)
        self.assertIsNone(result['scheduled_update_to_present_ms'])

    def test_missing_end_is_invalid(self):
        self.assertFalse(self.reduce(self.fixture(end=False))['valid_capture'])

    def continuous_fixture(self):
        root = self.fixture('OxideBenchIOS', presented=['102.050000000'])
        path = root/'trace.xml'
        xml = path.read_text()
        def sign(name, message):
            return f'<row><event-time>2008000000</event-time><thread/><process ref="p"/><event-type>Event</event-type><string/><id/><string>{name}</string><string/><sentinel/><string>com.oxide.compare-core</string><string/><os-log-metadata>{message}</os-log-metadata></row>'
        rows = sign('CoreMutation', 'generation=2 scheduled=101.908000000 source=102.008000000')
        rows += sign('CoreSubmitted', 'generation=2')
        rows += sign('CorePresented', 'generation=2 time=102.058000000')
        path.write_text(xml.replace('</node>', rows + '</node>', 1))
        return root

    def test_continuous_cadence_uses_app_presentations_not_system_swaps(self):
        root = self.continuous_fixture()
        result = reduce_trace(root/'trace.xml', root/'toc.xml', 'OxideBenchIOS', 'images', {'updates': 2})
        self.assertTrue(result['valid_capture'])
        self.assertEqual(result['attributed_updates'], 2)
        self.assertAlmostEqual(result['presentation_interval_ms']['p95'], 8)
        self.assertAlmostEqual(result['window_presentation_interval_ms']['p95'], 8)
        self.assertTrue(result['validity']['presentation_cadence']['valid'])
        self.assertFalse(result['validity']['presentation_cadence']['coverage'].startswith('complete'))
        self.assertIsNone(result['system_display_interval_ms_diagnostic'])

    def test_missing_or_duplicate_presentation_prevents_cadence_headline(self):
        for replacement in ['0.000000000', '102.050000000']:
            root = self.continuous_fixture()
            path = root/'trace.xml'
            path.write_text(path.read_text().replace('102.058000000', replacement))
            result = reduce_trace(path, root/'toc.xml', 'OxideBenchIOS', 'images', {'updates': 2})
            self.assertIsNone(result['presentation_interval_ms'])

    def test_window_cadence_includes_mapped_warmup_presentation(self):
        root = self.continuous_fixture()
        path = root/'trace.xml'
        warmup = '<row><event-time>22000000000</event-time><thread/><process ref="p"/><event-type>Event</event-type><string/><id/><string>CorePresented</string><string/><sentinel/><string>com.oxide.compare-core</string><string/><os-log-metadata>generation=0 time=101.990000000</os-log-metadata></row>'
        path.write_text(path.read_text().replace('</node>', warmup + '</node>', 1))
        result = reduce_trace(path, root/'toc.xml', 'OxideBenchIOS', 'images', {'updates': 2})
        self.assertEqual(result['presentation_interval_ms']['count'], 2)
        self.assertAlmostEqual(result['presentation_interval_ms']['p95'], 57.4)
        self.assertEqual(result['source_cohort_update_to_present_ms']['count'], 2)

    def test_duplicate_generation_presentation_is_ambiguous(self):
        root = self.fixture('OxideBenchIOS', presented=['102.050000000', '102.050000000'])
        result = self.reduce(root, 'OxideBenchIOS')
        self.assertEqual(result['ambiguous_presentation_generations'], [1])
        self.assertEqual(len(result['ambiguous_presentation_identities']), 1)
        self.assertIsNone(result['scheduled_update_to_present_ms'])

    def test_coalesced_uikit_transaction_does_not_block_capture_but_blocks_source_latency(self):
        root = self.fixture()
        path = root/'trace.xml'
        extra = '<row><event-time>2000500000</event-time><thread/><process ref="p"/><event-type>Event</event-type><string/><id/><string>CoreMutation</string><string/><sentinel/><string>com.oxide.compare-core</string><string/><os-log-metadata>generation=2 scheduled=101.908000000 source=102.008000000</os-log-metadata></row>'
        path.write_text(path.read_text().replace('</node>', extra + '</node>', 1))
        result = reduce_trace(path, root/'toc.xml', 'UIKitBenchIOS', 'text', {'updates': 2, 'skipped_updates': 0})
        self.assertTrue(result['valid_capture'])
        self.assertIn('mutation-submission-not-one-to-one', result['unresolved_transactions'])
        self.assertIsNone(result['source_cohort_update_to_present_ms'])
        self.assertFalse(result['validity']['source_cohort_latency']['valid'])

    def test_uikit_cadence_does_not_require_a_mutation_inside_each_transaction(self):
        root = self.fixture()
        path = root/'trace.xml'
        xml = path.read_text()
        update = '<row><start-time>3000000000</start-time><duration>121000000</duration><process ref="p"/><display/><uint32>9</uint32><uint32>8</uint32><color/><uint32/><string/></row>'
        swap = '<row><start-time>3020000000</start-time><start-time/><display/><displayed-surface-swap>8</displayed-surface-swap><uint32/><displayed-surface-swap>9</displayed-surface-swap></row>'
        xml = xml.replace('<node xpath="//table[2]">', '<node xpath="//table[2]">' + update)
        xml = xml.replace('<node xpath="//table[3]">', '<node xpath="//table[3]">' + swap)
        path.write_text(xml)
        result = self.reduce(root)
        self.assertEqual(result['presentation_interval_ms']['count'], 1)
        self.assertEqual(result['presentation_interval_ms']['p95'], 1000)
        self.assertEqual(result['attributed_updates'], 1)
        self.assertTrue(result['validity']['source_cohort_latency']['valid'])

    def test_cpu_clips_unions_per_thread_and_ignores_wrong_process(self):
        rows = (
            '<row><start-time>1500000000</start-time><thread>Main</thread><thread-state>Running</thread-state><duration>1000000000</duration><process ref="p"/></row>'
            '<row><start-time>2000000000</start-time><thread>Main</thread><thread-state>Running</thread-state><duration>1000000000</duration><process ref="p"/></row>'
            '<row><start-time>1000000000</start-time><thread>Worker</thread><thread-state>Running</thread-state><duration>500000000</duration><process ref="other"/></row>'
            '<row><start-time>20500000000</start-time><thread>Worker</thread><thread-state>Running</thread-state><duration>1000000000</duration><process ref="p"/></row>'
        )
        result = self.reduce(self.fixture(thread_rows=rows))
        self.assertTrue(result['cpu']['valid'])
        self.assertEqual(result['cpu']['running_threads'], 2)
        self.assertAlmostEqual(result['cpu']['app_process_running_ms'], 2000)
        self.assertAlmostEqual(result['cpu']['app_process_running_ms_per_mutation'], 2000)
        self.assertTrue(result['validity']['app_process_cpu']['valid'])

    def test_missing_cpu_events_and_wrong_pid_are_unavailable(self):
        wrong = '<row><start-time>1500000000</start-time><thread>Main</thread><thread-state>Running</thread-state><duration>1000000000</duration><process ref="other"/></row>'
        result = self.reduce(self.fixture(thread_rows=wrong))
        self.assertFalse(result['cpu']['valid'])
        self.assertEqual(result['cpu']['reason'], 'no target-process Running intervals')
        self.assertFalse(result['validity']['app_process_cpu']['valid'])

    def test_cpu_can_use_thread_owner_pid_only_when_process_cell_is_empty(self):
        row = '<row><start-time>1500000000</start-time><thread>Worker (pid: 42)</thread><thread-state>Running</thread-state><duration>1000000000</duration><process/></row>'
        result = self.reduce(self.fixture(thread_rows=row))
        self.assertTrue(result['cpu']['valid'])
        self.assertAlmostEqual(result['cpu']['app_process_running_ms'], 1000)

    def test_duration_uses_completion_measured_seconds(self):
        root = self.fixture()
        path = root/'trace.xml'
        path.write_text(path.read_text().replace('<event-time>21000000000</event-time>', '<event-time>61000000000</event-time>'))
        result = reduce_trace(path, root/'toc.xml', 'UIKitBenchIOS', 'text',
                              {'updates': 1, 'skipped_updates': 0, 'measured_seconds': 60})
        self.assertTrue(result['valid_capture'])

    def test_scorecard_requires_five_pairs_and_preserves_visual_failure(self):
        root = self.fixture()
        (root/'identity.json').write_text(json.dumps({'visual_evidence': {'cases': {
            'text': {'status': 'blocked', 'reason': 'different layout'},
            'images': {'status': 'passed'}}}}))
        pair = {'case': 'images', 'accepted': True, 'runs': [
            {'side': 'oxide', 'metrics': {'presentation_interval_ms': {'p95': 8}}},
            {'side': 'uikit', 'metrics': {'presentation_interval_ms': {'p95': 10}}}]}
        (root/'runs.json').write_text(json.dumps([pair] * 4))
        summarize(root)
        score = {r['case']: r for r in json.loads((root/'scorecard.json').read_text())}
        self.assertEqual(score['images']['result'], 'unavailable')
        self.assertEqual(score['text']['result'], 'visual equivalence blocked')
        (root/'runs.json').write_text(json.dumps([pair] * 5))
        summarize(root)
        score = {r['case']: r for r in json.loads((root/'scorecard.json').read_text())}
        self.assertEqual(score['images']['result'], 'Oxide lower p95 in all five pairs')

    def test_native_precision_and_thousands_separator(self):
        self.assertEqual(metadata_number('generation= 2,399 time= 0.0000', 'generation'), 2399)
        root = self.fixture()
        trace = Trace(root/'trace.xml', root/'toc.xml')
        self.assertEqual(trace.signs('UIKitBenchIOS')[1]['source'], 102)
        self.assertEqual(trace.signs('OtherApp'), [])


if __name__ == '__main__':
    unittest.main()
