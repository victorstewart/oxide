# Apple comparison suite

The optional Apple comparison suite runs the same sixteen named workloads through Oxide
and UIKit. It is a benchmark harness, not a production host or a renderer policy.

## Workloads

The shared registry contains these cases:

- Core: `shapes`, `text`, `local`, `images`, `animation`, and `scroll`.
- Visual boards: `visual-controls`, `visual-editing`, `visual-typography`,
  `visual-composition`, `visual-layout`, `visual-pickers`, `visual-opacity`,
  `visual-images`, `visual-geometry`, and `visual-editing-edges`.

The Rust runtime owns the same list as the native macOS runner. `fixtures/core.json`
and `fixtures/visual.json`, the bundled Noto font, and the PNG assets are shared by
the iOS adapters and the runtime. Each core case uses the 390 × 844 point, 3× fixture
timeline; visual boards use checkpoints 0, 1, and 2.

## Current-run admission

A physical comparison requires explicit current-run visual evidence. The manifest is
never read from a dated directory in this repository. `--visual-evidence PATH` names a
JSON manifest with the current `source_sha256`, per-case checkpoint hashes/statuses,
and, when available, `device_binaries.oxide` and `device_binaries.uikit` executable
SHA-256 values.

The command validates source, checkpoint, and available executable identities before
recording. Outputs must be a new directory outside Git:

```sh
cargo xtask ios compare-core \
  --visual-evidence /absolute/current-run/visual-evidence.json \
  --output /absolute/current-run/comparison
```

Pass `--case NAME` one or more times for a focused subset. With no `--case`, the
command records all sixteen cases. It builds or accepts matching Release iPhone app
bundles, requires the physical ProMotion iPhone, and retains identities, raw traces,
per-run metrics, and the generated scorecard under the output directory.

The energy sweep takes a separate `--qualification PATH` current-run, human-reviewed
bundle. It retains the established `normal_and_stall_probes_valid: true` and
`energy_captures_in_quartet: 4` predicate before recording. Qualification receipts and
identity checks remain external inputs; they do not turn simulator captures, callback
intervals, or native-macOS timings into iPhone presentation evidence.

The reusable `--pilot-renewal` qualification command can start with no history. It
records normal/stall presentation probes and an energy quartet, then requires review
of repeatability and control variation. Optional `--pilot-history` and `--pilot-resume`
inputs preserve validated recordings within the cumulative attempt bound; no specific
historical campaign or exact count of old attempts is required.

## Native macOS diagnostics

`macos-runner` and `tools/run_macos.py` use the same sixteen fixtures for bounded
native-Mac receipts, captures, and process-scoped profiling. Foreground runs include
drawable/presentation behavior. Offscreen runs isolate renderer throughput and do not
claim display cadence or iPhone comparison results. Capture readback is outside the
timed population.

## Project contents

`CoreComparison` builds both adapters from the shared project. `CoreHostBridge.m`
links the Rust static library for the Oxide app; the UIKit adapter uses the same
fixtures directly. `scripts/guard-cargo-target.sh` protects the Xcode Cargo target,
and the visual capture/color scripts validate the simulator checkpoint review path.
