#!/usr/bin/env python3
"""Paired offscreen background driver; it never claims native presentation pacing."""
import argparse
import json
import os
from pathlib import Path

import run_macos


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--mode", choices=("all", "timing", "accounting", "capture"), default="all")
    parser.add_argument("--cases", default="")
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--warmup", type=float, default=6)
    parser.add_argument("--duration", type=float, default=12)
    args = parser.parse_args(argv)
    if args.repeats != 3:
        parser.error("background protocol fixes --repeats at 3")
    if args.warmup != 6 or args.duration != 12:
        parser.error("background protocol fixes --warmup at 6 and --duration at 12 virtual seconds")
    binary, output = args.binary.resolve(), args.output.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error("--binary must name an executable oxide-macos-comparison")
    if output.exists() and any(output.iterdir()):
        parser.error("--output must be new or empty; existing artifacts are preserved")
    cases = run_macos.parse_cases(args.cases)
    root = Path(__file__).resolve().parents[3]
    output.mkdir(parents=True, exist_ok=True)
    paths = {name: output / name for name in ("immediate", "retained")}
    for name, path in paths.items():
        path.mkdir()
        run_macos.write_json(path / "identity.json", run_macos.source_identity(root, binary, "offscreen-replay", name))
    plan = {"schema": 1, "execution": "offscreen-replay", "virtual_protocol": {"warmup_seconds": 6, "measured_seconds": 12},
            "repeats": 3, "cases": cases, "mode": args.mode,
            "order": {"repeat_0": ["immediate", "retained"], "repeat_1": ["retained", "immediate"], "repeat_2": ["immediate", "retained"]},
            "outputs": {name: str(path / "summary.json") for name, path in paths.items()}}
    run_macos.write_json(output / "plan.json", plan)
    progress = {name: [] for name in paths}

    def record(path_name, case, repeat, mode):
        item = run_macos.run_one(binary, root, paths[path_name], case, repeat, mode, 6, 12,
                                 offscreen=True, text_path=path_name)
        progress[path_name].append(item)
        run_macos.write_json(paths[path_name] / "progress.json", {"runs": progress[path_name]})
        run_macos.write_json(output / "progress.json", {"paths": progress})
        print(json.dumps({"path": path_name, **item}, sort_keys=True), flush=True)

    if args.mode in ("all", "timing"):
        for case in cases:
            for repeat in range(3):
                order = ("immediate", "retained") if repeat % 2 == 0 else ("retained", "immediate")
                for path_name in order:
                    record(path_name, case, repeat, "timing")
    if args.mode in ("all", "accounting"):
        for case in cases:
            for path_name in ("immediate", "retained"):
                record(path_name, case, 0, "accounting")
    if args.mode in ("all", "capture"):
        for case in cases:
            for path_name in ("immediate", "retained"):
                record(path_name, case, 0, "capture")
    summaries = {}
    for name, values in progress.items():
        summary = run_macos.summarize_runs(values, "offscreen-replay")
        summary["text_path"] = name
        summary["capture_protocol"] = {"stages": run_macos.CAPTURE_STAGES, "checkpoints": run_macos.CAPTURE_CHECKPOINTS}
        run_macos.write_json(paths[name] / "summary.json", summary)
        summaries[name] = summary
    run_macos.write_json(output / "summary.json", {"schema": 1,
        "scope": "paired offscreen replay only; path groups and their distributions are retained separately, with no pooled performance claim",
        "plan": plan, "paths": summaries})


if __name__ == "__main__":
    main()
