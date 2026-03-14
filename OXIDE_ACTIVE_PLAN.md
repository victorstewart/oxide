# Oxide Active Work Plan

_Last updated: 2025-11-24_

This file replaces the legacy plan and audit documents. It tracks every open thread that still requires work to get the workspace back to a clean, testable, production-ready state.

## Status Legend

- [ ] Todo — not started
- [~] In progress
- [x] Complete
- [!] Blocked (notes explain why)

## 1. Build & Test Stabilisation

- [x] Restore `cargo test --workspace --all-targets --all-features` to green  
  - Tests now compile against `oxide_test_scenes`; perf- and snapshot-runner crates import the new router crate, and the networking/telemetry regressions were updated to match the current implementations.

- [x] Fix xtask test harness compilation  
  - Added `tempfile` as a dev-dependency so the CLI fixture tests build again.

- [!] Harden `cargo run -p xtask -- test-all`  
  - Clippy is installed and invoked, but the workspace currently fails on dozens of pre-existing lint violations (e.g., `clone_on_copy`, `collapsible_if`, `approx_constant`, host dead-code). Either land the fixes or decide on a scoped allowlist before the gate can be considered green.  
  - Once clippy passes, re-run the command to verify the remaining stages (`cargo hack`, perf-runner, snapshot-runner, XCUI smoke) complete; today the clippy step aborts early, so the later stages are still untested.

- [ ] Record a fresh green run in `docs/testing.md`  
  - Capture the exact commands, flags, and any prerequisite environment variables once the gate succeeds.  
  - Trim or update references to removed scripts/plans.

## 2. Scene & Coverage Integration

- [ ] Wire the new test scenes into automated flows  
  - Ensure `ElementsExtended`, `AnimationConfig`, `Orchestration`, `Permissions`, `Integration`, and `StressTest` are reachable from the iOS/macOS hosts and covered by snapshot/perf runners.  
  - Add unit and smoke tests that assert the timing configurability (badge, button, toggle, record button, sliding switch) behaves correctly with non-default values.

- [ ] Expand UITest coverage  
  - Extend `OxideHostUITests` (and any macOS automation harness) to navigate through the new scenes, execute the critical interactions, and capture screenshots/metrics.  
  - Reinstate the screenshot → golden diff checks once the additional scenes are automated.

- [ ] Re-evaluate telemetry and automation metrics  
  - Verify that telemetry rate limiting, scatter orchestrator cleanup, and QUIC backoff behave as expected under load; add tests or benchmarks where gaps remain.  
  - Update or recreate the removed automation coverage matrix after the above work lands.

## 3. Documentation Refresh

- [ ] Produce concise replacements for the archived docs  
  - Write focused notes for: test coverage snapshot (after new automation is in place) and UI automation operating guide (commands, destinations, skip flags).  
  - Keep all “current state” documents in sync with the build/test reality—link them from the README or this plan once published.

- [ ] Review remaining Markdown guides (`SHIFTINGTEXTINPUT_USAGE.md`, `DESIGN_SYSTEM_PROPER_USAGE.md`, `docs/testing.md`, etc.) for accuracy and update as features evolve.

## 4. Release Readiness Checklist

- [ ] After the above tasks are complete, run a full dry-run release: build both hosts, execute `xtask test-all`, and validate the sample apps on device/simulator.  
- [ ] Document the release procedure (toolchain versions, gating commands, artifact locations) so the audit history no longer depends on removed “Phase” summaries.

---

Owners and target dates can be added per team preference; keep this single source of truth updated as work progresses.
