# oxide-benchmark-spec::schema

## Intention and purpose

`schema` defines the serialized benchmark-spec v1 budget contract and its typed platform/tier identities.

## Relation to the rest of the code

- `fixtures` deserializes `BudgetSpec` values.
- `validate` enforces their arithmetic and frozen identities.
- `xtask` selects budgets using `Platform` and `Tier`.

Call flow:

- JSON budget
  - `BudgetSpec`
  - computed component/reserve/total helpers
  - validator and planner

## Entry points list

- `oxide_benchmark_spec::schema::Platform` identifies Apple or web budget families.
- `oxide_benchmark_spec::schema::Tier` identifies PR, nightly, release-core, claim-complete, extended, or full-attribution plans. Extended tiers require explicit plan and budget artifacts because benchmark-spec v1 defines no defaults.
- `oxide_benchmark_spec::schema::BudgetSpec` stores six component caps, declared arithmetic, and shard/campaign wall-time contracts.
- `BudgetSpec::computed_pre_reserve_seconds(&self) -> Option<u64>`, `computed_reserve_seconds(&self) -> Option<u64>`, and `computed_hard_total_seconds(&self) -> Option<u64>` recompute the normative arithmetic and return `None` on overflow.

## Logic narrative

Component seconds sum without hidden terms. Reserve is integer ceiling of 20 percent, computed as `(pre_reserve + 4) / 5`. Hard total is pre-reserve plus reserve. Separate shard hard-total, campaign critical-wall, and aggregate fields prevent parallel web workers from being reported as fewer consumed machine-minutes.

## Preconditions and postconditions

All values use seconds and nonnegative `u64`. A schema value becomes authoritative only after `validate_budget` and default-set validation pass.

## Edge cases and failure modes

Integer reserve arithmetic remains exact for nonnegative seconds. Every addition is checked, so adversarial or corrupted `u64` extremes fail validation instead of wrapping or panicking.

## Concurrency and memory behavior

Types own their string identity and contain only copyable enums and integer values. They are immutable unless explicitly borrowed mutably by a caller.

## Performance notes

Arithmetic is constant time and executes outside measurement windows.

## Feature flags and cfgs

No features or target cfgs alter serialization.

## Testing and benchmarks

Budget tests freeze serialized enum spellings indirectly through canonical fixture round trips and exact arithmetic assertions.

## Examples

`BudgetSpec::computed_hard_total_seconds()` returns 906 for the Apple PR fixture.

## Changelog

- 2026-07-21: added explicit extended and full-attribution tier identities without inventing default budgets.
- 2026-07-17: introduced platform, tier, and exact occupied-time budget schema types.
