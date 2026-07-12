# C00 paired renderer experiments

This directory is the stable local evidence root for C00. Large raw browser traces and per-pair process artifacts live in the ignored `raw/` child so the measured candidate tree does not recursively change when evidence records its own staged-tree identity. The C00 commit proof records hashes for every accepted artifact.

Required evidence families:

- `browser-diagnostic/`: version 6 raw page report, validated report, Markdown, Chrome trace, and capture from the cross-origin-isolated 2,000-frame diagnostic.
- `cpu-submit-ab/`: 15 fresh-process parent/candidate pairs analyzed with the shared no-material-regression policy.
- `displayed-frame-b/`: at least 10 fresh candidate Chrome sessions, each containing 2,000 RAF and GPU samples.
- `legacy-defect/`: the legacy synchronous batch report demonstrating why its batch CPU number cannot carry displayed-frame semantics.
- `environment.json`: branch, source identities, hardware, OS, toolchain, browser, display, power/thermal controls, viewport, DPR, refresh/cache state, build flags, commands, seed, and exact balanced order.

Official `benchmarks/web/latest.*` promotion is intentionally deferred to C62.
