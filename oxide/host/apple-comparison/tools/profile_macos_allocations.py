#!/usr/bin/env python3
"""Process-scoped full malloc history fallback for offscreen replay profiles."""
import argparse
from collections import Counter
import gzip
import json
import os
from pathlib import Path
import re
import subprocess
import time

import run_macos


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--cases", default="")
    parser.add_argument("--seconds", type=float, default=2, help="measured replay wall seconds; shorten when full histories exceed dump limits")
    args = parser.parse_args()
    if not 0 < args.seconds <= 60:
        parser.error("--seconds must be positive and at most 60")
    output, binary = args.output.resolve(), args.binary.resolve()
    if output.exists() and any(output.iterdir()):
        parser.error("output must be new or empty")
    output.mkdir(parents=True, exist_ok=True)
    root = Path(__file__).resolve().parents[3]
    run_macos.write_json(output / "identity.json", run_macos.source_identity(root, binary, "offscreen-allocation-history", "immediate"))
    progress = []
    for case in run_macos.parse_cases(args.cases):
        receipt = output / (case + ".json")
        environment = {key: value for key, value in os.environ.items()
                       if not key.startswith("OXIDE_MAC_") and not key.startswith("MallocStackLogging")}
        environment.update({"MallocStackLoggingNoCompact": "1", "OXIDE_MAC_OFFSCREEN": "1",
                            "OXIDE_MAC_PROFILE_SECONDS": str(args.seconds), "OXIDE_MAC_PROFILE_HOLD_SECONDS": "60",
                            "OXIDE_MAC_CASE": case, "OXIDE_MAC_OUTPUT": str(receipt)})
        history = output / (case + "-history.txt")
        completed = None
        history_code = None
        with (output / (case + ".log")).open("w") as log:
            process = subprocess.Popen([str(binary)], env=environment, stdout=log, stderr=log)
            guard_command = run_macos.keep_awake_command(process.pid, offscreen=True)
            guard = subprocess.Popen(guard_command) if guard_command else None
            try:
                deadline = time.monotonic() + 25
                while not receipt.exists() and process.poll() is None and time.monotonic() < deadline:
                    time.sleep(.05)
                if receipt.exists():
                    completed = json.loads(receipt.read_text())
                    with history.open("w") as stream:
                        result = subprocess.run(["/usr/bin/malloc_history", str(process.pid), "-allEvents"],
                                                stdout=stream, stderr=log, timeout=55)
                    history_code = result.returncode
            except subprocess.TimeoutExpired:
                history_code = -1
            finally:
                # The receipt is already final. Only terminate the diagnostic hold.
                run_macos.stop_process(process)
                run_macos.stop_process(guard)
        counts, sizes = Counter(), Counter()
        free_events = 0
        if history.exists():
            with history.open() as source, gzip.open(str(history) + ".gz", "wt") as archive:
                for line in source:
                    archive.write(line)
                    free_events += line.startswith("FREE ")
                    match = re.match(r"^ALLOC .*?\[size=(\d+)\]:\s*(.*)", line)
                    if match:
                        size, stack = int(match.group(1)), match.group(2).strip()
                        counts[stack] += 1
                        sizes[stack] += size
            history.unlink()
            if history_code == 0:
                for temporary in Path("/private/tmp").glob("stack-logs.%d.*" % process.pid):
                    if temporary.is_file():
                        temporary.unlink()
        rows = [{"stack": stack, "allocations": count, "bytes": sizes[stack]} for stack, count in counts.most_common()]
        run_macos.write_json(output / (case + "-stacks.json"), {
            "scope": "all process malloc history from launch through final hold, including freed allocations; distinguish init, warmup/replay, driver and collector stacks",
            "allocation_events": sum(counts.values()), "allocated_bytes": sum(sizes.values()),
            "free_events": free_events, "stacks": rows})
        row = {"case": case, "requested_wall_seconds": args.seconds, "history_exit_code": history_code,
               "profile_completed": completed is not None and not completed.get("errors") and completed.get("measured_frames", 0) > 0,
               "allocation_events": sum(counts.values()), "free_events": free_events,
               "terminated_diagnostic_hold": True}
        progress.append(row)
        run_macos.write_json(output / "progress.json", {"runs": progress})
        print(json.dumps(row), flush=True)
        if not row["profile_completed"] or history_code != 0 or not rows:
            raise SystemExit(1)


if __name__ == "__main__":
    main()
