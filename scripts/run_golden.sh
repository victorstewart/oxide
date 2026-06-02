#!/usr/bin/env bash
set -euo pipefail

SUITE_DIR="artifacts/static"
GOLDEN_DIR="goldens/static"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "${SCRIPT_DIR}/../oxide" && pwd)"

(cd "$WORKSPACE_DIR" && cargo build --release -p oxide-snapshot-runner)

BIN="$WORKSPACE_DIR/target/release/oxide-snapshot-runner"
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

run_one_size() {
  local comp="$1"; shift
  local variant="$1"; shift
  local state="$1"; shift
  local w="$1"; shift
  local h="$1"; shift
  local scale="$1"; shift
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
run_one scene_damage default
run_one scene_anim_timeline default
run_one scene_input_lab default
run_one scene_nine_slice default
run_one scene_sdf_text default
run_one scene_snapshot default
run_one scene_camera default
run_one scene_elements_extended default
run_one scene_animation_config default
run_one scene_orchestration default
run_one scene_permissions default
run_one scene_integration default
run_one scene_stress default
run_one text_unicode default
run_one text_input_ime_composition default
run_one text_input_grapheme_selection default
run_one text_input_fallback_cjk default
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
run_one_size scene3d_mixed wide default 320 192 1
run_one_size scene3d_bloom wide default 320 192 1
run_one_size scene3d_depth_stack wide default 320 192 1
run_one_size scene3d_viewport_clip wide default 320 192 1
run_one_size scene3d_material_cull wide default 320 192 1
run_one_size scene3d_blend_modes wide default 320 192 1
run_one_size camera_preview wide default 384 216 1
run_one_size camera_preview_legacy wide default 384 216 1
run_one_size camera_preview_bgra wide default 384 216 1
run_one_size camera_preview_blur_gray wide default 384 216 1
run_one_size camera_preview_tint_alpha wide default 384 216 1
run_one_size id_mask_compositor wide default 320 192 1
run_one_size id_mask_compositor_city_ids wide default 320 192 1
run_one_size id_mask_compositor_neighborhood_ids wide default 320 192 1
run_one_size id_mask_compositor_seams wide default 320 192 1

echo "golden suite done -> ${SUITE_DIR}"
