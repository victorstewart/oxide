# oxide-benchmark-spec::apple_campaign_plan

## Intention and purpose

`apple_campaign_plan` owns the benchmark-only macOS nightly and release acquisition contract. It generalizes scenario bindings, process packs, measurement passes, and their six occupied-time budget components without changing the frozen Apple PR schema or any production Oxide runtime path.

## Contract

Nightly binds the six PR scenarios plus `idle.steady` and `endurance.churn`. It declares one six-pair primary presentation pass, one launch pass with eight terminated/warm-system-cache and two fresh-install pairs, two isolated soak passes, and exactly two three-pair attribution acquisitions: Time Profiler and physical footprint.

Core and claim-complete release plans bind the full 13-candidate matrix. Their warm work is partitioned into `core-interaction`, `scroll-damage`, and `media-text-warm`, with separate launch, idle, and endurance packs. Release attribution declares Time Profiler, System Trace, the low-overhead `common-gpu` pass, and physical footprint. The exact-process `common-gpu` pass remains descriptive in every tier because AppKit may shift compositor work to WindowServer; claim-complete promotes only the declared claim-grade rows whose scope is symmetric.

Every primary and attribution pass also owns a typed timing overlay. Nightly freezes a five-second session reset allocation, 20-second readiness deadline, one-second setup, three-second warmup, and 12-second measured allocation; release freezes the same reset/setup values with a 30-second readiness deadline, five-second warmup, and 20-second measured allocation. `navigation.modal` replaces the continuous duration with 10 nightly or 20 release canonical cycles, while `resize.theme` freezes its release measurement at ten changes. Iteration rows retain an occupied-time bound so process deadlines and budget arithmetic remain total.

## Availability and failure behavior

Materialization binds the SHA-256 of every scenario manifest that exists and records `null` for a missing candidate. Structural validation is useful for reviewing the complete intended matrix, but `validate_runnable_apple_campaign_plan` rejects every missing binding before acquisition and lists all absent `scenarios/*.json` paths. As scenario artifacts land independently, the same preflight automatically moves them from unavailable to content-addressed without weakening the 8- or 13-scenario contract.

`extended` and `full-attribution` are valid serialized tier identities but have no default v1 budgets. They require an explicit plan and budget. Every attribution acquisition in a full-attribution plan covers all 13 scenarios, and its GPU/counter replay uses exactly one `full-attribution` pass; that ID is reserved for the audit and does not replace the `common-gpu` release pass.

## Entry points

- `materialize_default_macos_campaign_plan(spec_root, budget)` creates the canonical nightly, release-core, or claim-complete candidate.
- `validate_apple_campaign_contract(plan, budget)` checks identities, scenario/pack/pass references, exact tier structure, pass ownership, and all six budget components.
- `validate_runnable_apple_campaign_plan(spec_root, plan, budget)` additionally verifies every bound manifest hash, ID, and transitive artifact closure.
- `canonical_apple_campaign_plan_json(plan)` and `apple_campaign_plan_sha256(plan)` provide stable review and identity bytes.

## Performance and safety

All work is synchronous preflight outside measured processes. No renderer, host frame loop, app adapter, or production artifact depends on this module.

## Verification

`tests/apple_campaign_plan_tests.rs` freezes the eight- and 13-scenario matrices, pass and pack identities, budget ownership, explicit full-attribution coverage, canonical JSON, and fail-closed missing-artifact diagnostics.

## Changelog

- 2026-07-21: added generalized macOS nightly/release planning and explicit extended/full-attribution admission.
- 2026-07-21: added canonical tier timing overlays and typed duration-versus-iteration measurement semantics for every primary and attribution scenario.
