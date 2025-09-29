#!/usr/bin/env bash
set -euo pipefail

USE_LIST="${1:-0.6,0.7,0.75,0.8,0.9}"
PREF_LIST="${2:-0.15,0.25,0.33}"
MANIFEST="${3:-oxideui/Cargo.toml}"

cargo build --release -p oxideui-perf-runner --manifest-path "${MANIFEST}"

# Resolve target dir relative to manifest
MANI_DIR="$(cd "$(dirname "${MANIFEST}")" && pwd)"
BIN="${MANI_DIR}/target/release/oxideui-perf-runner"

: > sweep.txt

IFS=',' read -r -a USES <<< "${USE_LIST}"
IFS=',' read -r -a PREFS <<< "${PREF_LIST}"

for u in "${USES[@]}"; do
  for p in "${PREFS[@]}"; do
    echo "## RUN use=${u} prefilter=${p}" | tee -a sweep.txt
    OXIDEUI_ENABLE_DAMAGE=1 \
    OXIDEUI_DAMAGE_USE_THRESH="${u}" \
    OXIDEUI_DAMAGE_PREFILTER_THRESH="${p}" \
    "${BIN}" 2>&1 | tee -a sweep.txt
    echo "## END use=${u} prefilter=${p}" | tee -a sweep.txt
  done
done

echo "sweep.txt written"

