# oxide-benchmark-spec::fixtures

## Intention and purpose

`fixtures` provides stable loading and canonical serialization for committed benchmark-spec v1 budget, scenario, plan, font, migration, and comparator-audit JSON.

## Relation to the rest of the code

- Reads `benchmarks/comparative/specs/v1/budgets` beneath the workspace root.
- Returns `schema::BudgetSpec` values to validators and `xtask`.

Call flow:

- workspace root
  - fixed filename list
  - filesystem read
  - Serde JSON parse
  - validator/planner

## Entry points list

- `load_default_budgets(workspace_root: &Path) -> anyhow::Result<Vec<(PathBuf, BudgetSpec)>>` loads all required fixtures in stable order.
- `canonical_budget_json(budget: &BudgetSpec) -> anyhow::Result<Vec<u8>>` serializes one fixture canonically.
- `load_scenario(workspace_root: &Path, name: &str) -> anyhow::Result<(PathBuf, ScenarioSpec)>` loads one explicitly named runnable scenario.
- `canonical_scenario_json(scenario: &ScenarioSpec) -> anyhow::Result<Vec<u8>>` serializes one scenario canonically.
- `load_appkit_macos_native_production_audit(workspace_root: &Path)` loads the committed truthful rejected AppKit ComparatorAcceptance artifact for inspection; strict admission remains a separate explicit call.
- `BUDGET_RELATIVE_ROOT` exposes the repository-relative fixture root.
- `SCENARIO_RELATIVE_ROOT` exposes the runnable-scenario root without treating schema exemplars as campaign selections.

## Logic narrative

The fixed filename list is deliberate: directory contents cannot silently add an undeclared plan or omit a required plan. Each file is read and parsed with its path attached to any error. Canonical serialization uses schema field order and adds one final newline.

## Preconditions and postconditions

The caller provides the Cargo workspace root. Successful loading returns exactly one owned path/value tuple per fixed filename; semantic validation remains a separate explicit call.

## Edge cases and failure modes

Missing, unreadable, or malformed files return contextual errors. Extra directory files are ignored until the schema explicitly adds them, while default-set validation detects missing expected identities.

## Concurrency and memory behavior

Loading is synchronous, read-only, and bounded. File contents and deserialized objects are released normally when their owners drop them.

## Performance notes

Nine small JSON reads occur only during control-plane validation/planning, never inside a benchmark session.

## Feature flags and cfgs

No feature or target-specific behavior exists.

## Testing and benchmarks

Canonical round-trip tests compare every committed file byte-for-byte with reserialization.

## Examples

`load_default_budgets(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").as_path())` loads fixtures in crate integration tests.

## Changelog

- 2026-07-19: added the fixed AppKit macOS native.production comparator-audit loader.
- 2026-07-17: added fixed-order v1 budget loading and canonical JSON output.
- 2026-07-17: added explicit scenario loading and canonical scenario JSON output.
