#!/usr/bin/env python3
"""Collect process-scoped offscreen Instruments profiles for the comparison suite."""
import argparse
import json
import os
from pathlib import Path
import subprocess

import run_macos


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cases", default="")
    parser.add_argument("--kind", choices=("cpu", "allocations", "metal"), default="cpu")
    parser.add_argument("--continuous-text", action="store_true")
    args = parser.parse_args()
    cases = run_macos.parse_cases(args.cases)
    if args.continuous_text and any(case not in ("text", "local") for case in cases):
        parser.error("continuous-text is a separate text/local stress profile")
    binary, output = args.binary.resolve(), args.output.resolve()
    if output.exists() and any(output.iterdir()):
        parser.error("output must be new or empty")
    output.mkdir(parents=True, exist_ok=True)
    root = Path(__file__).resolve().parents[3]
    run_macos.write_json(output / "identity.json", run_macos.source_identity(root, binary, "offscreen-profile", "immediate"))
    template = {"cpu": "Time Profiler", "allocations": "Allocations", "metal": "Metal System Trace"}[args.kind]
    seconds = 12 if args.kind == "cpu" else 2
    progress = []
    for case in cases:
        receipt_path = output / (case + ".json")
        command = ["xcrun", "xctrace", "record", "--template", template, "--time-limit", str(seconds + 15) + "s",
                   "--output", str(output / (case + ".trace")), "--no-prompt"]
        values = {"OXIDE_MAC_OFFSCREEN": "1", "OXIDE_MAC_PROFILE_SECONDS": str(seconds),
                  "OXIDE_MAC_CASE": case, "OXIDE_MAC_OUTPUT": str(receipt_path)}
        if args.continuous_text:
            values["OXIDE_MAC_PROFILE_CONTINUOUS"] = "1"
        for key, value in values.items():
            command.extend(["--env", key + "=" + value])
        command.extend(["--launch", "--", str(binary)])
        environment = os.environ.copy()
        for key in list(environment):
            if key.startswith("OXIDE_MAC_"):
                environment.pop(key)
        guard = None
        with (output / (case + ".log")).open("w") as log:
            process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, env=environment)
            awake = run_macos.keep_awake_command(process.pid, offscreen=True)
            if awake:
                guard = subprocess.Popen(awake, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            try:
                code = process.wait(timeout=90)
            except subprocess.TimeoutExpired:
                run_macos.stop_process(process)
                code = None
            finally:
                run_macos.stop_process(guard)
        receipt = json.loads(receipt_path.read_text()) if receipt_path.exists() else None
        row = {"case": case, "template": template, "exit_code": code, "receipt": str(receipt_path),
               "valid_execution": code == 0 and receipt is not None and not receipt.get("errors")
                   and receipt.get("measured_frames", 0) > 0,
               "timeline": None if receipt is None else receipt.get("timeline"),
               "frames": None if receipt is None else receipt.get("measured_frames")}
        progress.append(row)
        run_macos.write_json(output / "progress.json", {"runs": progress})
        print(json.dumps(row), flush=True)
        if not row["valid_execution"]:
            break
    raise SystemExit(0 if len(progress) == len(cases) and all(row["valid_execution"] for row in progress) else 1)


if __name__ == "__main__":
    main()
