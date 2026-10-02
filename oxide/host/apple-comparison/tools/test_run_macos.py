#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import unittest


SPEC = importlib.util.spec_from_file_location("run_macos", Path(__file__).with_name("run_macos.py"))
run_macos = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(run_macos)


def frame(identifier, elapsed, presented, gpu=0):
    return {"id": identifier, "measured": True, "elapsed": elapsed, "source_time": 10 + elapsed,
            "scheduled_time": 9.99 + elapsed, "presented_time": presented, "acquire_ms": 1,
            "draw_ms": 2, "prepare_ms": 3, "encode_submit_ms": 4, "gpu_frame_id": gpu,
            "gpu_ms": 5 if gpu else 0, "result": 0}


def receipt(case, frames):
    return {"schema": 1, "case": case, "mode": "timing", "frames": frames,
            "samples": [{"time": 1.5, "foreground": True, "visible": True, "screen_hz": 120}], "errors": [],
            "external_inputs": 0, "cpu_start_us": 100, "cpu_end_us": 300, "measurement_start": 1,
            "measurement_end": 3}


class RunMacosTests(unittest.TestCase):
    def test_deduplicates_latest_completed_gpu_and_excludes_zero_presentation(self):
        reduced = run_macos.reduce_receipt(receipt("shapes", [frame(7, 0, 10.01, 7), frame(8, .1, 0, 7), frame(9, .2, 10.21, 8)]))
        self.assertEqual(reduced["gpu"]["unique_completed_frames"], 2)
        self.assertEqual(reduced["presentation"]["zero_timestamp_excluded"], 1)
        self.assertAlmostEqual(reduced["presentation"]["coverage"], 2 / 3)
        self.assertFalse(reduced["valid"])

    def test_visual_pickers_excludes_idle_stage_cadence(self):
        frames = [frame(1, 1.9, 11.9), frame(2, 2.1, 12.1), frame(3, 3.0, 13.0), frame(4, 4.1, 14.1)]
        reduced = run_macos.reduce_receipt(receipt("visual-pickers", frames))
        cadence = reduced["metrics"]["actual_cadence_ms"]
        self.assertEqual(cadence["count"], 1)
        self.assertAlmostEqual(cadence["p50"], 900.0)

    def test_all_window_samples_and_nonnegative_source_latency_are_required(self):
        value = receipt("shapes", [frame(1, 0, 9.9)])
        value["samples"].append({"time": 1.8, "foreground": False, "visible": True})
        reduced = run_macos.reduce_receipt(value)
        self.assertFalse(reduced["valid"])
        self.assertFalse(reduced["validity"]["foreground_and_visible"])
        self.assertFalse(reduced["validity"]["nonnegative_source_to_present"])

    def test_invalid_phase_and_noncontiguous_gpu_filter_exclude_warm_leading_ids(self):
        value = receipt("shapes", [frame(10, 0, 10.01, 4), frame(12, .1, 10.11, 10)])
        value["measurement_end"] = value["measurement_start"]
        reduced = run_macos.reduce_receipt(value)
        self.assertFalse(reduced["validity"]["measurement_phase_valid"])
        self.assertEqual(reduced["gpu"]["unique_completed_frames"], 1)

    def test_display_asleep_measured_sample_rejects_new_receipt(self):
        value = receipt("shapes", [frame(1, 0, 10.01)])
        value["samples"][0]["display_awake"] = False
        reduced = run_macos.reduce_receipt(value)
        self.assertFalse(reduced["valid"])
        self.assertFalse(reduced["validity"]["display_awake"])

    def test_legacy_receipt_keeps_existing_behavior_with_unverified_awake_scope(self):
        reduced = run_macos.reduce_receipt(receipt("shapes", [frame(1, 0, 10.01)]))
        self.assertTrue(reduced["validity"]["display_awake"])
        self.assertIn("unverified", reduced["validity"]["display_awake_scope"])

    def test_keep_awake_command_is_native_macos_only(self):
        self.assertIsNone(run_macos.keep_awake_command(42, system="Linux"))
        command = run_macos.keep_awake_command(42, system="Darwin")
        if command:
            self.assertEqual(command[-4:], ["-d", "-i", "-w", "42"])
        else:
            self.assertEqual(command, [])

    def test_offscreen_replay_is_valid_without_presentation_or_visible_samples(self):
        value = receipt("visual-composition", [frame(index, index / 120, 0, index) for index in range(1, 7)])
        value["execution"] = "offscreen-replay"
        value["samples"] = []
        reduced = run_macos.reduce_receipt(value)
        self.assertTrue(reduced["valid"])
        self.assertFalse(reduced["presentation"]["available"])
        self.assertNotIn("source_to_present_ms", reduced["metrics"])
        self.assertNotIn("actual_cadence_ms", reduced["metrics"])
        self.assertAlmostEqual(reduced["cpu_us_per_measured_frame"], 200 / 6)

    def test_offscreen_replay_rejects_skipped_submissions_and_wrong_population(self):
        value = receipt("visual-composition", [frame(index, index / 120, 0, index) for index in range(1, 7)])
        value["execution"] = "offscreen-replay"
        value["samples"] = []
        value["frames"][2]["skipped_submissions"] = 1
        reduced = run_macos.reduce_receipt(value)
        self.assertFalse(reduced["valid"])
        self.assertFalse(reduced["validity"]["no_errors"])
        value["frames"].pop()
        reduced = run_macos.reduce_receipt(value)
        self.assertFalse(reduced["validity"]["frames_measured"])

    def test_offscreen_caffeinate_does_not_request_display_wake(self):
        command = run_macos.keep_awake_command(42, system="Darwin", offscreen=True)
        if command:
            self.assertEqual(command[-3:], ["-i", "-w", "42"])
            self.assertNotIn("-d", command)
        else:
            self.assertEqual(command, [])

    def test_percentile_includes_p99(self):
        values = run_macos.distribution([1, 2, 3, 4])
        self.assertEqual(values["p50"], 2.5)
        self.assertGreater(values["p99"], values["p95"])


if __name__ == "__main__":
    unittest.main()
