import tempfile
import unittest
from pathlib import Path

from reduce_power_trace import phase_windows, reduce_power_trace, validate_receipt


class PowerTraceTests(unittest.TestCase):
    def fixture(self, rows, references=''):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        path = Path(tmp.name) / 'system-power.xml'
        path.write_text(
            '<trace-query-result><node xpath="//table[1]"><schema name="SystemPowerLevel">'
            '<col><engineering-type>start-time</engineering-type></col>'
            '<col><engineering-type>duration</engineering-type></col>'
            '<col><engineering-type>percent-per-hour</engineering-type></col>'
            '</schema>' + rows + '</node>' + references + '</trace-query-result>')
        return path

    def row(self, start, duration, rate):
        return (f'<row><start-time>{start}</start-time><duration>{duration}</duration>'
                f'<percent-per-hour>{rate}</percent-per-hour></row>')

    def signpost_fixture(self, events):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        rows = ''.join(
            f'<row><event-time>{event[0]}</event-time><thread/><process ref="p"/>'
            f'<event-type>{event[2]}</event-type><string/><id/><string>{event[1]}</string>'
            '<string/><sentinel/><string>com.oxide.compare-core</string><string/>'
            f'<string>{"source=" + str(event[3]) if len(event) > 3 else ""}</string>'
        '</row>' for event in events)
        (root / 'trace.xml').write_text(
            '<trace-query-result><node xpath="//table[1]">' + rows + '</node>'
            '<process id="p" fmt="OxideBenchIOS (42)"><pid>42</pid></process></trace-query-result>')
        (root / 'toc.xml').write_text(
            '<trace-toc><run><data><table schema="os-signpost"/></data></run></trace-toc>')
        return root

    def phase_events(self, adjustment_ns=0):
        time = 1_000_000_000
        events = []
        for name, seconds in (('CoreWarmup', 30), ('CoreControlBefore', 30),
                              ('CoreRun', 120), ('CoreDrain', 10), ('CoreControlAfter', 30)):
            events.append((time, name, 'Begin'))
            time += seconds * 1_000_000_000 + adjustment_ns
            events.append((time, name, 'End'))
        return events

    def receipt(self):
        time = 100.0
        boundaries = []
        for name, seconds in (('CoreWarmup', 30), ('CoreControlBefore', 30),
                              ('CoreRun', 120), ('CoreDrain', 10), ('CoreControlAfter', 30)):
            boundaries.append({'phase': name, 'event': 'begin', 'time': time,
                               'charging_or_plugged': False, 'low_power_mode': False})
            time += seconds
            boundaries.append({'phase': name, 'event': 'end', 'time': time,
                               'charging_or_plugged': False, 'low_power_mode': False})
        return {'status': 'diagnostic-complete', 'mode': 'energy', 'error': '', 'run_id': 'run-1',
                'charging_or_plugged_at_end': False, 'low_power_mode_at_end': False,
                'phase_boundaries': boundaries}

    def test_current_and_historical_energy_completion_receipts(self):
        for status in ('energy-complete', 'diagnostic-complete'):
            receipt = self.receipt()
            receipt['status'] = status
            self.assertTrue(validate_receipt(receipt, 'run-1'))
        for status, mode in (('failed', 'energy'), ('waiting', 'energy'),
                             ('energy-complete', 'diagnostic')):
            receipt = self.receipt()
            receipt.update(status=status, mode=mode)
            with self.assertRaisesRegex(ValueError, 'successful energy run'):
                validate_receipt(receipt, 'run-1')

    def test_integrates_native_rate_and_deduplicates_exact_rows(self):
        xml = self.fixture(self.row(0, 1_000_000_000, 36) + self.row(0, 1_000_000_000, 36)
                           + self.row(1_000_000_000, 1_000_000_000, 72))
        result = reduce_power_trace(xml, [{'name': 'active', 'start_ns': 0, 'stop_ns': 2_000_000_000}])
        window = result['windows'][0]
        self.assertEqual(result['exact_duplicate_rows'], 1)
        self.assertEqual(result['unique_rows'], 2)
        self.assertEqual(result['energy_units'], 'battery-percentage-points')
        self.assertAlmostEqual(window['battery_percentage_points'], .03)
        self.assertTrue(window['coverage_complete'])

    def test_resolves_exported_references(self):
        xml = self.fixture(
            '<row><start-time ref="s"/><duration ref="d"/><percent-per-hour ref="r"/></row>',
            '<start-time id="s">0</start-time><duration id="d">1000000000</duration>'
            '<percent-per-hour id="r">36</percent-per-hour>')
        result = reduce_power_trace(xml, [{'name': 'active', 'start_ns': 0, 'stop_ns': 1_000_000_000}])
        self.assertAlmostEqual(result['windows'][0]['battery_percentage_points'], .01)

    def test_missing_reference_is_rejected(self):
        xml = self.fixture('<row><start-time ref="missing"/><duration>1000000000</duration>'
                           '<percent-per-hour>36</percent-per-hour></row>')
        with self.assertRaisesRegex(ValueError, 'unresolved Instruments reference'):
            reduce_power_trace(xml, [{'name': 'active', 'start_ns': 0, 'stop_ns': 1_000_000_000}])

    def test_conflicting_overlap_is_rejected(self):
        xml = self.fixture(self.row(0, 2_000_000_000, 36) + self.row(1_000_000_000, 1_000_000_000, 72))
        with self.assertRaisesRegex(ValueError, 'overlap'):
            reduce_power_trace(xml, [{'name': 'active', 'start_ns': 0, 'stop_ns': 2_000_000_000}])

    def test_nanosecond_boundary_rounding_is_accepted_without_double_counting(self):
        xml = self.fixture(self.row(0, 1_000_000_001, 36) + self.row(1_000_000_000, 1_000_000_000, 72))
        result = reduce_power_trace(xml, [{'name': 'active', 'start_ns': 0, 'stop_ns': 2_000_000_000}])
        self.assertAlmostEqual(result['windows'][0]['battery_percentage_points'], .03)

    def test_gap_in_any_explicit_window_is_reported(self):
        xml = self.fixture(self.row(0, 1_000_000_000, 36) + self.row(2_000_000_000, 1_000_000_000, 36))
        result = reduce_power_trace(xml, [{'name': 'active', 'start_ns': 0, 'stop_ns': 3_000_000_000}])
        self.assertFalse(result['valid_power_windows'])
        self.assertFalse(result['windows'][0]['coverage_complete'])
        self.assertEqual(result['windows'][0]['uncovered_ranges'], [
            {'start_ns': 1_000_000_000, 'stop_ns': 2_000_000_000}])

    def test_positive_intervals_and_explicit_windows_are_required(self):
        xml = self.fixture(self.row(0, 0, 36))
        with self.assertRaisesRegex(ValueError, 'positive'):
            reduce_power_trace(xml, [{'name': 'active', 'start_ns': 0, 'stop_ns': 1}])
        xml = self.fixture(self.row(0, 1, 36))
        with self.assertRaisesRegex(ValueError, 'explicit'):
            reduce_power_trace(xml, [])
        with self.assertRaisesRegex(ValueError, 'unique names'):
            reduce_power_trace(xml, [{'name': '', 'start_ns': 0, 'stop_ns': 1}])

    def test_tiny_export_gaps_are_quantified_without_filling(self):
        xml = self.fixture(self.row(0, 1_000_000_000, 36) + self.row(1_000_000_042, 999_999_958, 36))
        result = reduce_power_trace(xml, [{'name': 'active', 'start_ns': 0, 'stop_ns': 2_000_000_000}])
        window = result['windows'][0]
        self.assertTrue(result['valid_power_windows'])
        self.assertFalse(result['exact_power_window_coverage'])
        self.assertFalse(window['coverage_complete'])
        self.assertTrue(window['usable_coverage'])
        self.assertEqual(window['uncovered_ns'], 42)
        self.assertLess(window['coverage_fraction'], 1)

    def test_material_gap_is_not_a_valid_power_window(self):
        xml = self.fixture(self.row(0, 1_000_000_000, 36) + self.row(1_000_002_000, 999_998_000, 36))
        result = reduce_power_trace(xml, [{'name': 'active', 'start_ns': 0, 'stop_ns': 2_000_000_000}])
        self.assertFalse(result['valid_power_windows'])
        self.assertFalse(result['windows'][0]['usable_coverage'])

    def test_phase_windows_accept_nontruncated_duration_within_tolerance(self):
        root = self.signpost_fixture(self.phase_events(1_000_000_000))
        windows = phase_windows(root / 'trace.xml', root / 'toc.xml', 'OxideBenchIOS', 42)
        self.assertEqual([window['name'] for window in windows], [
            'CoreWarmup', 'CoreControlBefore', 'CoreRun', 'CoreDrain', 'CoreControlAfter'])
        self.assertAlmostEqual(windows[0]['duration_deviation_seconds'], 1)

    def test_phase_windows_reject_duplicate_disordered_and_wrong_duration(self):
        events = self.phase_events()
        duplicate = events[:1] + [events[0]] + events[1:]
        root = self.signpost_fixture(duplicate)
        with self.assertRaisesRegex(ValueError, 'duplicate phase marker'):
            phase_windows(root / 'trace.xml', root / 'toc.xml', 'OxideBenchIOS', 42)
        disordered = self.phase_events()
        disordered[1], disordered[2] = disordered[2], disordered[1]
        root = self.signpost_fixture(disordered)
        with self.assertRaisesRegex(ValueError, 'disordered'):
            phase_windows(root / 'trace.xml', root / 'toc.xml', 'OxideBenchIOS', 42)
        root = self.signpost_fixture(self.phase_events(-11_000_000))
        with self.assertRaisesRegex(ValueError, 'CoreWarmup duration.*shorter'):
            phase_windows(root / 'trace.xml', root / 'toc.xml', 'OxideBenchIOS', 42)

    def test_receipt_requires_successful_unplugged_ordered_energy_phases(self):
        receipt = self.receipt()
        self.assertTrue(validate_receipt(receipt, 'run-1'))
        receipt['phase_boundaries'][3]['low_power_mode'] = True
        with self.assertRaisesRegex(ValueError, 'low-power'):
            validate_receipt(receipt, 'run-1')

    def test_missing_phase_markers_require_calibration(self):
        root = self.signpost_fixture(self.phase_events()[5:])
        with self.assertRaisesRegex(ValueError, 'calibrated receipt'):
            phase_windows(root / 'trace.xml', root / 'toc.xml', 'OxideBenchIOS', 42)

    def test_partial_phase_markers_calibrate_complete_receipt_windows(self):
        offset = 5_000_000_000
        receipt = self.receipt()
        events = [(int(280 * 1_000_000_000 + offset), 'CoreRun', 'End', 280.0),
                  (int(280 * 1_000_000_000 + offset + 20), 'CoreDrain', 'Begin', 280.0)]
        root = self.signpost_fixture(events)
        windows = phase_windows(root / 'trace.xml', root / 'toc.xml', 'OxideBenchIOS', 42, receipt)
        self.assertEqual(windows[0]['window_source'], 'receipt-calibrated-to-trace')
        self.assertEqual(windows[0]['observed_marker_count'], 2)
        self.assertEqual(windows[0]['missing_marker_count'], 8)
        self.assertEqual(windows[0]['offset_spread_ns'], 20)
        self.assertEqual(windows[0]['start_ns'], 105_000_000_020)

    def test_partial_marker_calibration_rejects_bad_offset_spread(self):
        receipt = self.receipt()
        events = [(285_000_000_000, 'CoreRun', 'End', 280.0),
                  (285_200_000_000, 'CoreDrain', 'Begin', 280.0)]
        root = self.signpost_fixture(events)
        with self.assertRaisesRegex(ValueError, 'offset spread'):
            phase_windows(root / 'trace.xml', root / 'toc.xml', 'OxideBenchIOS', 42, receipt)

    def test_partial_marker_calibration_rejects_duplicate_marker_identity(self):
        receipt = self.receipt()
        events = [(285_000_000_000, 'CoreRun', 'End', 280.0),
                  (285_000_000_020, 'CoreRun', 'End', 280.0)]
        root = self.signpost_fixture(events)
        with self.assertRaisesRegex(ValueError, 'duplicate phase marker'):
            phase_windows(root / 'trace.xml', root / 'toc.xml', 'OxideBenchIOS', 42, receipt)


if __name__ == '__main__':
    unittest.main()
