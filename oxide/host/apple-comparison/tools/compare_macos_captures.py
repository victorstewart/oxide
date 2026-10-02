#!/usr/bin/env python3
"""Verify full-resolution Metal captures and build three-checkpoint galleries."""
import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image, ImageDraw

from run_macos import CASES, CAPTURE_STAGES


def inspect(root):
    cases = {}
    for case in CASES:
        receipt_path = root / "runs" / case / "capture-00" / "receipt.json"
        if not receipt_path.exists():
            cases[case] = {"valid": False, "error": "missing capture receipt"}
            continue
        receipt = json.loads(receipt_path.read_text())
        files = [Path(path) for path in receipt.get("capture_files", [])]
        images = []
        records = []
        for path in files:
            with Image.open(path) as image:
                pixels = image.convert("RGBA")
                records.append({"path": str(path), "size": list(image.size),
                                "pixel_sha256": hashlib.sha256(pixels.tobytes()).hexdigest()})
                images.append(pixels)
        correct_sizes = len(images) == 7 and all(image.size == (1170, 2532) for image in images)
        repeated = []
        if correct_sizes:
            for later, earlier in ((3, 0), (4, 1), (5, 2), (6, 0)):
                repeated.append({"earlier": earlier, "later": later,
                                 "equal": records[earlier]["pixel_sha256"] == records[later]["pixel_sha256"]})
        cases[case] = {"valid": correct_sizes and not receipt.get("errors") and all(row["equal"] for row in repeated),
                       "execution": receipt.get("execution", "visible-native"),
                       "dimensions_valid": correct_sizes, "restoration": repeated, "files": records,
                       "errors": receipt.get("errors", [])}
    return cases


def galleries(cases, output):
    paths = []
    for group in range(4):
        canvas = Image.new("RGB", (780, 4 * 595), "#e0e4e8")
        draw = ImageDraw.Draw(canvas)
        for row, case in enumerate(CASES[group * 4:group * 4 + 4]):
            draw.text((8, row * 595 + 8), case, fill="#111820")
            for column, record in enumerate(cases[case].get("files", [])[:3]):
                with Image.open(record["path"]) as image:
                    canvas.paste(image.convert("RGB").resize((260, 563), Image.Resampling.LANCZOS),
                                 (column * 260, row * 595 + 28))
        path = output / ("gallery-%d.png" % (group + 1))
        canvas.save(path)
        paths.append(str(path))
    return paths


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--captures", type=Path, required=True)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    cases = inspect(args.captures.resolve())
    if args.baseline:
        baseline = inspect(args.baseline.resolve())
        for case, row in cases.items():
            before, after = baseline[case].get("files", []), row.get("files", [])
            row["baseline_pixels_equal"] = len(before) == len(after) == 7 and all(
                a["pixel_sha256"] == b["pixel_sha256"] for a, b in zip(before, after))
            row["valid"] = row["valid"] and baseline[case]["valid"] and row["baseline_pixels_equal"]
    result = {"schema": 1, "scope": "full Metal target pixels; per-case execution identifies offscreen or visible path; readback excluded from timing",
              "stages": CAPTURE_STAGES, "all_valid": all(row["valid"] for row in cases.values()),
              "cases": cases, "galleries": galleries(cases, args.output)}
    (args.output / "visual-verification.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"all_valid": result["all_valid"], "valid_cases": sum(row["valid"] for row in cases.values()),
                      "report": str(args.output / "visual-verification.json")}), flush=True)
    raise SystemExit(0 if result["all_valid"] else 1)


if __name__ == "__main__":
    main()
