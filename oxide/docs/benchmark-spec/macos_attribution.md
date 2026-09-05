# oxide-benchmark-spec: macos_attribution

## Intention and purpose

This module freezes the macOS full-attribution acquisition matrix without placing profiler policy in production Oxide. It expands Allocations, VM Tracker, Metal System Trace, and every explicitly available GPU-counter configuration into distinct replays over all 13 release scenarios.

## Relation to the rest of the code

- `materialize_macos_full_attribution_plan` creates the typed benchmark-only schedule.
- `validate_macos_full_attribution_plan` rejects missing scenarios, merged invasive collectors, implicit GPU-counter support, and budget drift.
- `oxide-apple-comparison-controller::MacOsDeepAttributionCollector` consumes one replay at a time.

Call flow:

- toolchain/device capability audit
  - explicit `MacOsGpuCounterConfiguration` rows
  - available rows receive one selector
  - unavailable rows retain one reason and schedule no replay
- plan materialization
  - three mandatory isolated replays
  - one isolated replay per available GPU configuration
  - exact 13-scenario order and release pair count
  - derived occupied time, 20-percent reserve, and 24-hour ceiling

## Entry points list

- `materialize_macos_full_attribution_plan(gpu_counter_configurations: Vec<MacOsGpuCounterConfiguration>) -> Result<MacOsFullAttributionPlan>` creates the complete schedule and exact budget.
- `validate_macos_full_attribution_plan(plan: &MacOsFullAttributionPlan) -> Result<()>` validates identity, coverage, isolation, capability, timing, and arithmetic.
- `canonical_macos_full_attribution_plan_json(plan: &MacOsFullAttributionPlan) -> Result<Vec<u8>>` emits canonical reviewed JSON.
- `MacOsAttributionCollectorKind`, `MacOsAttributionAvailability`, `MacOsAttributionTraceSelector`, `MacOsGpuCounterConfiguration`, `MacOsAttributionReplaySpec`, `MacOsAttributionBudget`, and `MacOsFullAttributionPlan` are the public typed contract.

## Logic narrative

The constructor always inserts separate Allocations, VM Tracker, and Metal System Trace replays. It adds a GPU-counter replay only when its configuration is explicitly `available`; device/toolchain unavailability stays durable in the plan rather than being mistaken for a zero metric. Every replay receives 12 pairs, both sides, five seconds of reset, and all 13 scenarios with one-second setup, five-second warmup, and 20-second measurement. Budget arithmetic is derived from those exact terms.

## Preconditions and postconditions

GPU configuration IDs are unique lowercase path-safe identifiers. Available configurations name exactly one xctrace template or instrument selector. Unavailable configurations have no selector and have a nonempty reason. Success guarantees no replay combines profilers and all available configurations are scheduled exactly once.

## Edge cases and failure modes

Duplicate IDs, missing selectors, selectors attached to unavailable counters, missing mandatory replays, changed scenario order, a shortened pair count, combined template/instrument selectors, claim-bearing evidence roles, and totals above 24 hours fail closed.

## Concurrency and memory behavior

Plan construction is synchronous and bounded by the small preregistered capability list. Values own their strings; no global state or locks are used.

## Performance notes

This module schedules measurement but performs no measurement work. Every profiler runs separately because uncalibrated combinations would change the workload being attributed.

## Feature flags and cfgs

None.

## Testing and benchmarks

`tests/macos_attribution_tests.rs` covers exact 13-scenario expansion, available/unavailable GPU configurations, canonical JSON, isolation, and corruption rejection.

## Examples

Pass the audited GPU configuration inventory to `materialize_macos_full_attribution_plan`, persist the canonical JSON, and execute each returned replay independently.

## Changelog

- 2026-07-21: added the explicit full-attribution matrix and budget contract.
