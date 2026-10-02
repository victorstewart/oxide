# Benchmarks

This directory contains maintained benchmark contracts. Benchmark implementations,
fixtures, assets, and regression goldens remain with their source owners under
`oxide/crates/`, `oxide/host/`, and `oxide/tests/`. Generated benchmark evidence stays
outside Git.

Performance reports, captures, traces, energy exports, and temporary comparisons belong
under `oxide/artifacts/` (or an explicitly chosen location outside the repository). The
Rust runner defaults to `oxide/artifacts/perf-runner/`; the iOS device runner defaults to
`oxide/artifacts/oxide-device/`. These paths are ignored by Git. Keep a result with the
run that produced it; only promote source contracts, workloads, fixtures, and goldens
after review.

The reusable runners remain source:

- the 16-case workload and Apple runners;
- the macOS runner and capture support;
- iOS/device, trace, and energy tooling; and
- browser capture tooling.

See `CONTRACT.md` for the workload and result contract.
