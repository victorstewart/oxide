# oxide-benchmark-spec::validate

## Intention and purpose

`validate` rejects budget drift, missing time components, misleading shard arithmetic, weakened scenario fairness, and malformed deterministic traces before comparison acquisition begins.

## Relation to the rest of the code

- Consumes `schema::BudgetSpec` values loaded by `fixtures`.
- Supplies the hard gate used by `xtask compare-ui validate` and `plan`.

Call flow:

- `validate_default_budget_set`
  - cardinality and identity table
  - `validate_budget`
  - filename, platform, tier, total, critical-wall, and aggregate checks

## Entry points list

- `validate_budget(budget: &BudgetSpec) -> anyhow::Result<()>` validates one budget's schema and arithmetic invariants.
- `validate_default_budget_set(budgets: &[(PathBuf, BudgetSpec)]) -> anyhow::Result<()>` validates the complete normative set.
- `validate_scenario(scenario: &ScenarioSpec) -> anyhow::Result<()>` validates one scenario's identities, phases, metrics, parity checkpoints, and fairness surface.
- Asset-manifest validation verifies a positive inline-text grid, nonempty uniquely sized raster variants, unique variant roles, exact `columns * em_pixels` by `rows * em_pixels` dimensions, verified member identities, unique nonempty graphemes, positive metrics, and unique in-grid cells.
- `validate_font_pack_manifest(spec_root: &Path, manifest: &FontPackManifest, expected_id: &str) -> anyhow::Result<()>` verifies content hashes, provenance, licenses, and the exact ordered variable-axis contract against each font's `fvar` table.
- `validate_trace(events: &[TraceEvent]) -> anyhow::Result<()>` validates ordered integer timestamps, normalized coordinates, and operation-specific required data.
- Crate-internal artifact, asset-manifest, and font-pack validators are shared with release-candidate capture admission so the capture path checks the same transitive bytes as runnable scenarios.

## Logic narrative

Single-budget validation recomputes the component sum, ceiling reserve, and hard total, then checks shard values do not exceed campaign aggregate time. Set validation requires exactly nine unique IDs and compares each against the frozen v1 platform, tier, hard-total, critical-wall, aggregate, and filename contract.

Font validation hashes and parses each declared font before inspecting variation metadata. It requires a nonempty `fvar` table, exact manifest/fvar axis cardinality and record order, unique printable four-byte ASCII tags, and an integer-millionths coordinate inside the corresponding axis range. Validation happens before acquisition or materialization; it does not enter text shaping or a measured frame.

Inline-text validation binds one logical cell map to every verified raster variant. It rejects zero grid dimensions, missing or duplicate roles, duplicate em sizes, dimensions that do not exactly match the declared grid, duplicate graphemes or cells, out-of-grid cells, and nonpositive advance or size. This makes system fallback, undeclared scaling, and inconsistent variant geometry contract errors rather than adapter choices.

## Preconditions and postconditions

Paths identify the files from which values were loaded. Success proves exact v1 headline arithmetic, not scenario coverage, device readiness, calibration, or acquisition validity.

## Edge cases and failure modes

Every mismatch returns an error naming the budget or font role and failed invariant. Duplicate IDs cannot hide a missing fixture because cardinality and seen-set checks are both required. Malformed font bytes, static fonts, incomplete axis lists, reordered or duplicate tags, non-printable tags, out-of-range coordinates, and malformed inline-text atlas geometry fail closed.

## Concurrency and memory behavior

Validation borrows inputs and allocates small ordered maps/sets bounded to the fixture or font-axis count. Font bytes are read once for hashing and `fvar` parsing. No global state or synchronization is used.

## Performance notes

Budget validation is `O(n log n)` for nine fixtures. Font validation is linear in font bytes for hashing plus axis count for metadata checks and has no relevance to measured application performance.

## Feature flags and cfgs

No features or target cfgs apply.

## Testing and benchmarks

Integration tests alter component and reserve values and require fail-closed errors, while committed fixtures exercise all expected identities. Scenario tests accept the pinned Noto axes and inline-text variants and reject incomplete, reordered, duplicate, malformed, out-of-range, dimension-mismatched, duplicate-cell, and out-of-grid declarations.

## Examples

Call `validate_default_budget_set(&load_default_budgets(root)?)` before selecting a platform and tier.

## Changelog

- 2026-07-21: exposed transitive artifact validators within the crate for release-candidate capture admission without adding public API.
- 2026-07-18: validated the content-addressed inline-text raster variants, exact grid-derived dimensions, unique em sizes/roles/graphemes/cells, and in-grid metrics.
- 2026-07-18: validated complete ordered integer-millionths coordinates against each hashed variable font's `fvar` table.
- 2026-07-17: froze v1 component arithmetic and all nine default plan totals, including disjoint nightly web worker accounting.
- 2026-07-17: added fail-closed scenario/fairness and deterministic trace validation.
