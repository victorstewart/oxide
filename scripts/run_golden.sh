#!/usr/bin/env bash
set -euo pipefail

SUITE_DIR="artifacts/static"
GOLDEN_DIR="goldens/static"
MANIFEST="oxideui/Cargo.toml"

cargo build --release -p oxideui-snapshot-runner --manifest-path "$MANIFEST"

BIN="oxideui/target/release/oxideui-snapshot-runner"
rm -rf "$SUITE_DIR"
mkdir -p "$SUITE_DIR" "$GOLDEN_DIR"

: > "$SUITE_DIR/sweep.txt"

run_one() {
  local comp="$1"; shift
  local variant="$1"; shift
  local state="${1:-default}"; shift || true
  local w=800; local h=600; local scale=2.0
  local out="${SUITE_DIR}/${comp}/${variant}/${state}.png"
  local golden="${GOLDEN_DIR}/${comp}/${variant}/${state}/baseline.png"
  mkdir -p "$(dirname "$out")" "$(dirname "$golden")"
  "$BIN" --component "$comp" --variant "$variant" --state "$state" --width "$w" --height "$h" --scale "$scale" --out "$out" --golden "$golden" | tee -a "$SUITE_DIR/sweep.txt"
}

# Minimal primitives set for Phase 3
run_one progressbar determinate default
run_one progressbar determinate indeterminate
run_one spinner default
run_one imageview contain
run_one imageview_zoom default
run_one nine_slice default
run_one scene_controls default
run_one scene_text default
run_one scene_zoom default
run_one scene_collection default
run_one text_unicode default
run_one style_effects default
run_one layer_composite default
run_one button default
run_one button default pressed
run_one button default disabled
run_one toggle default
run_one toggle default on
run_one slider default
run_one slider default low
run_one slider default high
run_one collection_grid default
run_one collection_row default

echo "golden suite done -> ${SUITE_DIR}"
