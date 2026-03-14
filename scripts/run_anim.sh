#!/usr/bin/env bash
set -euo pipefail

SUITE_DIR="artifacts/anim"
GOLDEN_DIR="goldens/anim"
MANIFEST="oxide/Cargo.toml"

cargo build --release -p oxide-snapshot-runner --manifest-path "$MANIFEST"

BIN="oxide/target/release/oxide-snapshot-runner"
rm -rf "$SUITE_DIR"
mkdir -p "$SUITE_DIR" "$GOLDEN_DIR"

: > "$SUITE_DIR/sweep.txt"

run_one_t() {
  local comp="$1"; shift
  local variant="$1"; shift
  local state="$1"; shift
  local tms="$1"; shift
  local w=800; local h=600; local scale=2.0; local period=1000
  local out="${SUITE_DIR}/${comp}/${variant}/${state}/t${tms}.png"
  local golden="${GOLDEN_DIR}/${comp}/${variant}/${state}/t${tms}/baseline.png"
  mkdir -p "$(dirname "$out")" "$(dirname "$golden")"
  "$BIN" --suite anim --component "$comp" --variant "$variant" --state "$state" --time-ms "$tms" --period-ms "$period" --width "$w" --height "$h" --scale "$scale" --out "$out" --golden "$golden" | tee -a "$SUITE_DIR/sweep.txt"
}

times=(0 33 100 250 500 1000)

# Spinner animation frames
for t in "${times[@]}"; do
  run_one_t spinner default default "$t"
done

# Progress bar indeterminate frames
for t in "${times[@]}"; do
  run_one_t progressbar determinate indeterminate "$t"
done

# AnimTimeline bars
for t in "${times[@]}"; do
  run_one_t animtimeline default default "$t"
done

# Button press scale frames (skip 33ms to avoid OS sleep jitter)
times_btn=(0 100 250 500 1000)
for t in "${times_btn[@]}"; do
  run_one_t button_press default default "$t"
done

# Toggle spring to on
for t in "${times[@]}"; do
  run_one_t toggle_anim default to_on "$t"
done

# Slider move by phase
for t in "${times[@]}"; do
  run_one_t slider_move default default "$t"
done

# Collection scroll (grid and row)
for t in "${times[@]}"; do
  run_one_t collection_grid_scroll default default "$t"
  run_one_t collection_row_scroll default default "$t"
done

echo "anim suite done -> ${SUITE_DIR}"
