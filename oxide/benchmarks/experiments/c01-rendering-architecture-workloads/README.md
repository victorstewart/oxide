# C01 rendering architecture proof workloads

This directory is the stable local evidence root for C01. Large suite reports, browser reports, per-pair process artifacts, packages, captures, and environment records live in the ignored `raw/` child so evidence acquisition does not recursively change the measured candidate tree. The C01 commit proof records the accepted artifact hashes and exact measured tree.

Required evidence families:

- `architecture-smoke.json`: all 132 Rust CPU/Metal architecture rows with their declared full workload sizes.
- `webgpu-architecture.json` and `webgpu-primitives.txt`: the opt-in ten-row RAF-paced WebGPU matrix with direct timestamps and backend counters.
- `unchanged-runtime-ab/`: fresh-process parent/candidate parity evidence for unchanged production-path workloads and startup/resource-layout controls.
- `visual-a.png` and `visual-b.png`: exact same-scene parent/candidate browser captures.
- `environment.json`: source identities, hardware, OS, GPU, toolchain, browser, power/thermal state, build flags, commands, cache/viewport/DPR controls, and balanced order.

Official workspace and browser baseline promotion remains deferred to C62.
