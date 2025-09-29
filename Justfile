set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

scheme := "oxideui-perf-runner"
manifest := "oxideui/Cargo.toml"
use_vals := "0.6,0.7,0.75,0.8,0.9"
pref_vals := "0.15,0.25,0.33"

default: sweep

build:
    cargo build --release -p {{scheme}} --manifest-path {{manifest}}

sweep:
    ./scripts/sweep_local.sh "{{use_vals}}" "{{pref_vals}}" "{{manifest}}"

aggregate:
    cargo build --release --manifest-path tools/sweep_agg/Cargo.toml
    ./tools/sweep_agg/target/release/sweep_agg --input sweep.txt --csv sweep.csv --json sweep.json

golden:
    ./scripts/run_golden.sh

anim:
    ./scripts/run_anim.sh

aggregate-anim:
    cargo build --release --manifest-path tools/anim_agg/Cargo.toml
    ./tools/anim_agg/target/release/anim_agg --input artifacts/anim/sweep.txt --csv artifacts/anim/sweep.csv --json artifacts/anim/summary.json

test:
    cd oxideui && cargo test -p oxideui-ui-core -p oxideui-timing -p oxideui-platform-ios

test-camera:
    cd oxideui && cargo test -p oxideui-platform-ios
