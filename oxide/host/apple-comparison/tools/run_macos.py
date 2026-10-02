#!/usr/bin/env python3
"""Bounded native-macOS receipt runner and diagnostic reducer.

This deliberately writes only standalone macOS artifacts.  It does not update
the iOS comparison scorecard or claim an Apple-device comparison.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time


CASES = (
    "shapes", "text", "local", "images", "animation", "scroll",
    "visual-controls", "visual-editing", "visual-typography", "visual-composition",
    "visual-layout", "visual-pickers", "visual-opacity", "visual-images",
    "visual-geometry", "visual-editing-edges",
)
CAPTURE_STAGES = (0, 1, 2, 0, 1, 2, 0)
CAPTURE_CHECKPOINTS = (0, 10, 19.9, 0, 10, 19.9, 0)
CONTINUOUS = {"shapes", "images", "animation", "scroll", "visual-controls"}


def expected_offscreen_frames(case):
    if case in ("text", "local"):
        return 120
    if case in CONTINUOUS:
        return 1440
    if case == "visual-pickers":
        return 484
    return 6


def percentile(values, point):
    values = sorted(float(value) for value in values if value is not None and math.isfinite(float(value)))
    if not values:
        return None
    index = (len(values) - 1) * point
    low, high = math.floor(index), math.ceil(index)
    return values[low] + (values[high] - values[low]) * (index - low)


def distribution(values):
    values = [value for value in values if value is not None and math.isfinite(float(value))]
    if not values:
        return {"count": 0, "p50": None, "p95": None, "p99": None, "max": None}
    return {"count": len(values), "p50": percentile(values, .5), "p95": percentile(values, .95),
            "p99": percentile(values, .99), "max": max(values)}


def sha256_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def command_text(root, args):
    result = subprocess.run(args, cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    return result.stdout.strip() if result.returncode == 0 else None


def keep_awake_command(pid, system=None, offscreen=False):
    if (system or platform.system()) != "Darwin":
        return None
    caffeinate = shutil.which("caffeinate")
    if not caffeinate:
        return []
    return [caffeinate, "-i", "-w", str(pid)] if offscreen else [caffeinate, "-d", "-i", "-w", str(pid)]


def stop_process(process):
    if process is None or process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=2)


def source_identity(root, binary, execution="native", text_path=None):
    status = command_text(root, ["git", "status", "--short"])
    diff = subprocess.run(["git", "diff", "--no-ext-diff", "--binary"], cwd=root, stdout=subprocess.PIPE,
                          stderr=subprocess.DEVNULL).stdout
    return {
        "binary": str(binary), "binary_sha256": sha256_file(binary),
        "git_head": command_text(root, ["git", "rev-parse", "HEAD"]),
        "git_status": status.splitlines() if status is not None else None,
        "source_diff_sha256": hashlib.sha256(diff).hexdigest(),
        "execution": execution, "text_path": text_path, "display_link": os.environ.get("OXIDE_MAC_DISPLAY_LINK", "ca"),
        "keep_awake_scope": "temporary `caffeinate -i -w <runner-pid>` per offscreen child; no display wake" if execution == "offscreen-replay" else "temporary `caffeinate -d -i -w <runner-pid>` per native macOS child; terminated after that child; no persistent power-policy change or synthetic input",
        "platform": {"system": platform.system(), "release": platform.release(), "machine": platform.machine(),
                     "python": platform.python_version(), "tools": {
                         name: command_text(root, arguments)
                         for name, arguments in (("sw_vers", ["sw_vers"]),
                                                 ("xcodebuild", ["xcodebuild", "-version"]),
                                                 ("xcrun", ["xcrun", "--version"]),
                                                 ("cargo", ["cargo", "--version"]),
                                                 ("pmset", ["pmset", "-g", "custom"])) if shutil.which(name)
                     }},
    }


def read_receipt(path):
    try:
        receipt = json.loads(path.read_text())
    except (OSError, ValueError) as error:
        return None, "receipt unreadable: %s" % error
    if receipt.get("schema") != 1:
        return None, "receipt schema must be 1"
    if not isinstance(receipt.get("frames"), list) or not isinstance(receipt.get("samples"), list):
        return None, "receipt frames and samples must be arrays"
    return receipt, None


def measured_frames(receipt):
    return [frame for frame in receipt.get("frames", []) if frame.get("measured")]


def usable_for_cadence(case, frame):
    if case == "visual-pickers":
        elapsed = frame.get("elapsed")
        return elapsed is not None and 2 <= float(elapsed) % 6 < 4
    return case in CONTINUOUS


def frame_presented(frame):
    value = frame.get("presented_time")
    return value is not None and float(value) > 0 and math.isfinite(float(value))


def reduce_receipt(receipt):
    case = receipt.get("case")
    offscreen = receipt.get("execution") == "offscreen-replay"
    frames = measured_frames(receipt)
    errors = list(receipt.get("errors", []))
    source_present, scheduled_present = [], []
    acquire, draw, cpu_total, prepare, encode_submit = [], [], [], [], []
    gpu_candidates, presentation_times = [], []
    zero_presented = no_presented = 0
    for frame in frames:
        for field, target in (("acquire_ms", acquire), ("draw_ms", draw), ("prepare_ms", prepare),
                              ("encode_submit_ms", encode_submit)):
            if frame.get(field) is not None:
                target.append(frame[field])
        if frame.get("acquire_ms") is not None and frame.get("draw_ms") is not None:
            cpu_total.append(float(frame["acquire_ms"]) + float(frame["draw_ms"]))
        if frame.get("result", 0) != 0:
            errors.append("frame %s returned %s" % (frame.get("id"), frame.get("result")))
        if offscreen and frame.get("skipped_submissions", 0):
            errors.append("frame %s skipped %s submissions" % (frame.get("id"), frame.get("skipped_submissions")))
        gpu_id, gpu_ms = frame.get("gpu_frame_id", 0), frame.get("gpu_ms")
        if gpu_id and gpu_ms is not None and float(gpu_ms) > 0:
            gpu_candidates.append((gpu_id, float(gpu_ms)))
        if offscreen or not frame_presented(frame):
            if offscreen:
                continue
            if frame.get("presented_time") == 0:
                zero_presented += 1
            else:
                no_presented += 1
            continue
        presented = float(frame["presented_time"])
        if frame.get("source_time") is not None:
            source_present.append((presented - float(frame["source_time"])) * 1000.0)
        if frame.get("scheduled_time") is not None:
            scheduled_present.append((presented - float(frame["scheduled_time"])) * 1000.0)
        if usable_for_cadence(case, frame):
            cycle = int(float(frame.get("elapsed", 0)) // 6) if case == "visual-pickers" else None
            presentation_times.append((cycle, presented))
    frame_ids = [frame.get("id") for frame in frames]
    numeric_ids = all(isinstance(value, int) for value in frame_ids)
    contiguous_ids = numeric_ids and frame_ids and set(frame_ids) == set(range(min(frame_ids), max(frame_ids) + 1))
    if contiguous_ids:
        gpu_candidates = [(identifier, milliseconds) for identifier, milliseconds in gpu_candidates if identifier in set(frame_ids)]
        gpu_filter_scope = "completed frame id matched a contiguous measured submission-id range"
    elif numeric_ids and frame_ids:
        gpu_candidates = [(identifier, milliseconds) for identifier, milliseconds in gpu_candidates if identifier >= min(frame_ids)]
        gpu_filter_scope = "measured collection samples after the first measured source id; id continuity was unavailable"
    else:
        gpu_filter_scope = "measured collection samples; source frame ids were unavailable"
    gpu_by_frame = {}
    for identifier, milliseconds in gpu_candidates:
        gpu_by_frame.setdefault(str(identifier), milliseconds)
    cadence = []
    for (left_cycle, left), (right_cycle, right) in zip(presentation_times, presentation_times[1:]):
        if right > left and left_cycle == right_cycle:
            cadence.append((right - left) * 1000.0)
    cpu_start, cpu_end = receipt.get("cpu_start_us"), receipt.get("cpu_end_us")
    start, end = receipt.get("measurement_start"), receipt.get("measurement_end")
    phase_valid = None not in (start, end) and float(end) > float(start)
    samples = [sample for sample in receipt.get("samples", [])
               if phase_valid and sample.get("time") is not None and float(start) <= float(sample["time"]) <= float(end)]
    sample_ok = bool(samples) and all(sample.get("foreground") and sample.get("visible") for sample in samples)
    awake_values = [sample.get("display_awake") for sample in samples if "display_awake" in sample]
    display_awake_ok = not any(value is False for value in awake_values)
    display_awake_scope = "verified by every measured HostSample" if samples and len(awake_values) == len(samples) else "unverified: no measured display-awake samples or legacy receipt lacks display_awake"
    cpu_per_second = None
    if None not in (cpu_start, cpu_end, start, end) and phase_valid:
        cpu_per_second = (float(cpu_end) - float(cpu_start)) / (float(end) - float(start))
    presentation_coverage = sum(frame_presented(frame) for frame in frames) / len(frames) if frames else 0.0
    negative_source_latency = any(value < 0 for value in source_present)
    expected_frames = expected_offscreen_frames(case) if offscreen else None
    offscreen_valid = phase_valid and len(frames) == expected_frames and not errors
    valid = offscreen_valid if offscreen else phase_valid and sample_ok and display_awake_ok and not errors and receipt.get("external_inputs", 0) == 0 and not negative_source_latency and presentation_coverage >= .95
    metrics = {"cpu_draw_ms": distribution(draw), "cpu_total_ms": distribution(cpu_total), "acquire_ms": distribution(acquire),
               "prepare_ms": distribution(prepare), "encode_submit_ms": distribution(encode_submit), "gpu_ms": distribution(gpu_by_frame.values())}
    if not offscreen:
        metrics.update({"source_to_present_ms": distribution(source_present),
                        "scheduled_to_present_ms": distribution(scheduled_present), "actual_cadence_ms": distribution(cadence)})
    else:
        metrics.pop("acquire_ms")
    return {
        "case": case, "mode": receipt.get("mode"), "execution": receipt.get("execution", "native"), "frames_measured": len(frames),
        "valid": valid, "validity": {"measurement_phase_valid": phase_valid, "foreground_and_visible": None if offscreen else sample_ok,
                     "window_samples": len(samples), "no_errors": not errors,
                     "display_awake": None if offscreen else display_awake_ok, "display_awake_scope": "not collected for offscreen replay" if offscreen else display_awake_scope,
                     "no_external_inputs": None if offscreen else receipt.get("external_inputs", 0) == 0,
                     "nonnegative_source_to_present": None if offscreen else not negative_source_latency,
                     "presentation_coverage_at_least_95_percent": None if offscreen else presentation_coverage >= .95,
                     "frames_measured": len(frames) == expected_frames if offscreen else None,
                     "expected_measured_frames": expected_frames},
        "cpu_us_per_second": None if offscreen else cpu_per_second,
        "cpu_us_per_measured_frame": (float(cpu_end) - float(cpu_start)) / len(frames) if offscreen and None not in (cpu_start, cpu_end) and frames else None,
        "metrics": metrics,
        "presentation": {"available": not offscreen, "coverage": None if offscreen else presentation_coverage, "attributed_frames": None if offscreen else sum(frame_presented(frame) for frame in frames),
                         "zero_timestamp_excluded": zero_presented, "missing_callback": no_presented,
                         "cadence_scope": "not collected for offscreen replay" if offscreen else "actual nonzero presentation callbacks, continuous workloads only" if case != "visual-pickers"
                         else "actual nonzero presentation callbacks within visual-pickers active stage [2s,4s) of each 6s cycle; cross-cycle gaps excluded"},
        "gpu": {"unique_completed_frames": len(gpu_by_frame), "coverage": len(gpu_by_frame) / len(frames) if frames else 0.0,
                "scope": "deduplicated latest-completed command-buffer samples; not per-submitted-frame coverage; " + gpu_filter_scope},
        "errors": errors,
    }


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def parse_cases(text):
    cases = CASES if not text else tuple(item.strip() for item in text.split(",") if item.strip())
    unknown = [case for case in cases if case not in CASES]
    if unknown or not cases:
        raise ValueError("--cases must contain only: " + ", ".join(CASES))
    return cases


def run_one(binary, root, output, case, repeat, mode, warmup, duration, accounting=False, offscreen=False, text_path="immediate"):
    label = "%s-%02d" % (mode, repeat)
    run_dir = output / "runs" / case / label
    run_dir.mkdir(parents=True, exist_ok=False)
    receipt_path = run_dir / "receipt.json"
    env = os.environ.copy()
    env.update({"OXIDE_MAC_CASE": case, "OXIDE_MAC_MODE": mode,
                "OXIDE_MAC_OUTPUT": str(receipt_path.resolve()), "OXIDE_MAC_WARMUP": str(warmup),
                "OXIDE_MAC_DURATION": str(duration), "OXIDE_MAC_TEXT_PATH": text_path})
    if offscreen:
        env["OXIDE_MAC_OFFSCREEN"] = "1"
    if mode == "accounting":
        env["OXIDE_MAC_ACCOUNTING"] = "1"
    if mode == "capture":
        capture_dir = run_dir / "captures"
        capture_dir.mkdir()
        env.update({"OXIDE_MAC_CAPTURE_DIR": str(capture_dir.resolve()),
                    "OXIDE_MAC_CAPTURE_STAGES": ",".join(map(str, CAPTURE_STAGES)),
                    "OXIDE_MAC_CAPTURE_CHECKPOINTS": ",".join(map(str, CAPTURE_CHECKPOINTS)),
                    "OXIDE_MAC_CAPTURE_RESTORE": "1"})
    timeout = 30 if mode == "capture" else warmup + duration + 30
    started = time.time()
    process = None
    keep_awake = None
    keep_awake_error = None
    launch_error = None
    timed_out = False
    stdout = stderr = ""
    try:
        process = subprocess.Popen([str(binary)], cwd=root, env=env, text=True, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE)
        command = keep_awake_command(process.pid, offscreen=offscreen)
        if command == []:
            keep_awake_error = "cannot establish temporary macOS keep-awake guard: caffeinate is unavailable"
        elif command:
            keep_awake = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
            time.sleep(.05)
            if keep_awake.poll() is not None and (keep_awake.returncode != 0 or process.poll() is None):
                detail = keep_awake.stderr.read().strip()
                keep_awake_error = "cannot establish temporary macOS keep-awake guard" + (": " + detail if detail else "")
        if keep_awake_error:
            stop_process(process)
        else:
            stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        stop_process(process)
    except OSError as error:
        launch_error = "runner or keep-awake launch failed: %s" % error
    finally:
        stop_process(keep_awake)
        if process is not None and process.poll() is None:
            stop_process(process)
    def output_text(value):
        return value.decode("utf-8", "replace") if isinstance(value, bytes) else value or ""
    if process is not None and not stdout and not stderr:
        stdout, stderr = process.communicate()
    (run_dir / "stdout.log").write_text(output_text(stdout))
    (run_dir / "stderr.log").write_text(output_text(stderr))
    receipt, error = read_receipt(receipt_path) if receipt_path.exists() else (None, "receipt was not written")
    if keep_awake_error:
        error = keep_awake_error if error is None else error + "; " + keep_awake_error
    if launch_error:
        error = launch_error if error is None else error + "; " + launch_error
    if receipt and (receipt.get("case") != case or receipt.get("mode") != mode):
        error = "receipt case/mode does not match launched run"
    expected_execution = "offscreen-replay" if offscreen else None
    if receipt and expected_execution and receipt.get("execution") != expected_execution:
        error = "receipt execution does not match offscreen replay" if error is None else error + "; receipt execution does not match offscreen replay"
    if receipt and offscreen and receipt.get("text_path") != text_path:
        error = "receipt text path does not match launched run" if error is None else error + "; receipt text path does not match launched run"
    if process is None:
        error = "runner failed to launch" if error is None else error + "; runner failed to launch"
    elif not timed_out and process.returncode != 0:
        error = "process exited with %s" % process.returncode if error is None else error + "; process exited with %s" % process.returncode
    finished = time.time()
    progress = {"case": case, "mode": mode, "execution": "offscreen-replay" if offscreen else "native", "text_path": text_path,
                "repeat": repeat, "started_unix_seconds": started,
                "finished_unix_seconds": finished, "elapsed_seconds": finished - started,
                "timeout_seconds": timeout, "timed_out": timed_out,
                "exit_code": None if timed_out or process is None else process.returncode, "receipt_error": error,
                "keep_awake": {"established": keep_awake is not None and keep_awake_error is None,
                               "cleaned_up": keep_awake is None or keep_awake.poll() is not None,
                               "scope": "temporary system-awake guard without display wake for offscreen child" if offscreen else "temporary native macOS child guard" if platform.system() == "Darwin" else "not applicable"}}
    if receipt and error is None:
        progress["reduced"] = reduce_receipt(receipt)
        progress["reduced"]["valid"] = progress["reduced"]["valid"] and process.returncode == 0
        progress["reduced"]["validity"]["exit_code_zero"] = process.returncode == 0
        if process.returncode != 0:
            progress["reduced"]["errors"].append("process exited with %s" % process.returncode)
    if mode == "capture":
        progress["capture_protocol"] = {"stages": CAPTURE_STAGES, "checkpoints": CAPTURE_CHECKPOINTS,
                                        "restoration_stage": 0}
        files = receipt.get("capture_files", []) if receipt else []
        progress["capture_validity"] = {"exactly_seven_files": isinstance(files, list) and len(files) == 7,
                                        "dimensions": "must be checked from saved PNG files"}
    write_json(run_dir / "progress.json", progress)
    return progress


def comparison(current, baseline):
    def flattened(summary):
        return {(case, mode, name): value for case, row in summary.get("cases", {}).items()
                for mode, group in row.items() if mode in ("timing", "accounting")
                for name, value in group.get("aggregate", {}).items()}
    before, after = flattened(baseline), flattened(current)
    rows = {}
    for key in sorted(set(before) & set(after)):
        old, new = before[key], after[key]
        if old is not None and new is not None:
            rows["%s.%s.%s" % key] = {"baseline": old, "current": new, "delta": new - old,
                                    "relative": (new - old) / old if old else None}
    return rows


def summarize_runs(progress, execution):
    grouped = {"timing": {}, "accounting": {}}
    for item in progress:
        reduced = item.get("reduced")
        if reduced and item["mode"] in grouped:
            grouped[item["mode"]].setdefault(item["case"], []).append(reduced)
    summary_cases = {}
    stages = ("cpu_draw_ms", "cpu_total_ms", "acquire_ms", "prepare_ms", "encode_submit_ms", "gpu_ms")
    native = ("source_to_present_ms", "scheduled_to_present_ms", "actual_cadence_ms")
    for mode, case_runs in grouped.items():
        for case, runs in case_runs.items():
            valid_runs = [run for run in runs if run["valid"]]
            names = stages + (() if execution == "offscreen-replay" else native)
            aggregate = {name: distribution([run["metrics"][name]["p50"] for run in valid_runs if name in run["metrics"]])["p50"] for name in names}
            primary = "cpu_us_per_measured_frame" if execution == "offscreen-replay" else "cpu_us_per_second"
            aggregate[primary] = distribution([run[primary] for run in valid_runs])["p50"]
            summary_cases.setdefault(case, {})[mode] = {"runs": runs, "valid_runs": len(valid_runs),
                "invalid_runs": [run for run in runs if not run["valid"]], "aggregate": aggregate}
    return {"schema": 1,
            "scope": "offscreen replay execution; no native presentation, foreground, screen, or wall-pacing claim" if execution == "offscreen-replay" else "native macOS diagnostic measurements; not official iOS comparison data",
            "execution": execution, "cases": summary_cases, "runs": progress}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--cases", default="")
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--warmup", type=float, default=6)
    parser.add_argument("--duration", type=float, default=12)
    parser.add_argument("--capture", action="store_true")
    parser.add_argument("--accounting", action="store_true")
    parser.add_argument("--offscreen", action="store_true")
    parser.add_argument("--text-path", choices=("immediate", "retained"), default="immediate")
    parser.add_argument("--baseline", type=Path)
    args = parser.parse_args(argv)
    if args.repeats < 1 or args.warmup < 0 or args.duration <= 0:
        parser.error("--repeats must be positive, --warmup nonnegative, and --duration positive")
    binary, output = args.binary.resolve(), args.output.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error("--binary must name an executable oxide-macos-comparison")
    if output.exists() and any(output.iterdir()):
        parser.error("--output must be new or empty; existing artifacts are preserved")
    cases = parse_cases(args.cases)
    root = Path(__file__).resolve().parents[3]
    output.mkdir(parents=True, exist_ok=True)
    execution = "offscreen-replay" if args.offscreen else "native"
    write_json(output / "identity.json", source_identity(root, binary, execution, args.text_path))
    modes = ["capture"] if args.capture else ["accounting"] if args.accounting else ["timing"]
    progress = []
    for case in cases:
        for mode in modes:
            count = 1 if mode == "capture" else args.repeats
            for repeat in range(count):
                progress.append(run_one(binary, root, output, case, repeat, mode, args.warmup, args.duration,
                                        args.accounting, args.offscreen, args.text_path))
                write_json(output / "progress.json", {"runs": progress})
                print(json.dumps(progress[-1], sort_keys=True), flush=True)
    summary = summarize_runs(progress, execution)
    summary["text_path"] = args.text_path
    summary["capture_protocol"] = {"stages": CAPTURE_STAGES, "checkpoints": CAPTURE_CHECKPOINTS}
    if args.baseline:
        baseline_path = args.baseline / "summary.json" if args.baseline.is_dir() else args.baseline
        summary["comparison"] = comparison(summary, json.loads(baseline_path.read_text()))
    write_json(output / "summary.json", summary)


if __name__ == "__main__":
    main()
