# oxide-benchmark-spec

## Intention and purpose

`oxide-benchmark-spec` owns framework-neutral, versioned comparison-lab contracts. The current slice freezes machine-readable tier budgets, the Apple PR acquisition expansion, generalized macOS nightly/release candidate plans, deterministic scenario and trace inputs, the Section 8 comparison plan/session/raw-observation schema, strict pre-acquisition ComparatorAcceptance admission, the normalized PNG visual parity gate, and the exact legacy Apple migration disposition before platform adapters or acquisition can consume device time.

## Relation to the rest of the code

- `xtask compare-ui validate` loads and validates committed fixtures through this crate.
- `xtask compare-ui plan` selects validated platform/tier budget shards and explains their occupied-time ceiling.
- `xtask compare-ui` can feed correctness checkpoint PNGs, layout JSON, and host text geometry into the deterministic visual reducer before timing evidence is eligible.
- Rust, Swift, and TypeScript adapters consume the scenario, trace, metric, plan, session, and raw-observation schemas; parity and executable-plan validation remain incremental work.

Call flow:

- `load_default_budgets`
  - JSON fixture parse
  - `validate_default_budget_set`
  - `validate_budget`
- `xtask compare-ui plan`
  - platform/tier selection
  - budget explanation
- comparison acquisition
  - `ComparisonPlan`
  - `ControllerChunk` and `ScenarioPack`
  - `ComparisonSession` and typed raw JSONL rows

## Entry points list

- `oxide_benchmark_spec::load_default_budgets(workspace_root: &Path) -> anyhow::Result<Vec<(PathBuf, BudgetSpec)>>` loads the nine benchmark-spec v1 budget fixtures.
- `oxide_benchmark_spec::canonical_budget_json(budget: &BudgetSpec) -> anyhow::Result<Vec<u8>>` emits canonical pretty JSON with a trailing newline.
- `oxide_benchmark_spec::validate_budget(budget: &BudgetSpec) -> anyhow::Result<()>` validates component, reserve, and total arithmetic.
- `oxide_benchmark_spec::validate_default_budget_set(budgets: &[(PathBuf, BudgetSpec)]) -> anyhow::Result<()>` freezes the required ids, platforms, tiers, totals, and shard wall/aggregate values.
- `oxide_benchmark_spec::validate_scenario(scenario: &ScenarioSpec) -> anyhow::Result<()>` validates frozen scenario and fairness invariants.
- `oxide_benchmark_spec::validate_font_pack_manifest(...)` verifies every pinned font and license byte plus its source provenance.
- `oxide_benchmark_spec::FontVariationAxis { tag: String, value_millionths: i64 }` exposes ordered deterministic variable-font coordinates to Rust and host adapters without floating-point manifest values.
- `oxide_benchmark_spec::{InlineTextAtlas, InlineTextAtlasVariant, InlineTextAsset}` exposes the pinned multi-density image-run contract for graphemes not covered by the committed text fonts.
- `oxide_benchmark_spec::validate_trace(events: &[TraceEvent]) -> anyhow::Result<()>` validates ordered, integer trace inputs.
- `oxide_benchmark_spec::validate_comparison_plan(plan: &ComparisonPlan) -> anyhow::Result<()>` validates identities and every scenario/pack/chunk/cell/metric/family cross-reference before acquisition.
- `oxide_benchmark_spec::admit_comparator_acceptance(...)` admits only an exact, canonical, fully evidenced and signed `accepted` reference audit for the expected identity, retained scenarios, and primary cells.
- `oxide_benchmark_spec::load_appkit_macos_native_production_audit(...)` loads the canonical truthful `rejected` audit for the current custom-canvas AppKit target.
- `oxide_benchmark_spec::validate_apple_pr_acquisition(...)` validates the exact one-build, four-acquisition Apple PR expansion against its tier budget.
- `oxide_benchmark_spec::materialize_default_macos_campaign_plan(...)` creates the canonical eight-scenario nightly or 13-scenario release candidate while preserving explicit missing bindings.
- `oxide_benchmark_spec::materialize_macos_full_attribution_plan(...)` creates the separate 13-scenario Allocations, VM Tracker, Metal System Trace, and available GPU-counter replay matrix; see [macos_attribution](macos_attribution.md).
- `oxide_benchmark_spec::validate_apple_campaign_contract(...)` validates higher-tier scenario, pack, pass, and budget-component ownership; `validate_runnable_apple_campaign_plan(...)` additionally requires every content-addressed scenario artifact closure.
- `oxide_benchmark_spec::canonical_instrumentation_calibration_input_json(...)` and `reduce_instrumentation_calibration(...)` own canonical trace-on/off external-sensor admission; see [instrumentation_calibration](instrumentation_calibration.md).
- `oxide_benchmark_spec::validate_legacy_case_disposition(...)` requires exact one-to-one mapping of the reviewed XCTest inventory and preserves required risks.
- `oxide_benchmark_spec::validate_pr_vertical_slice(...)` validates the exact dashboard/feed/navigation product slice and its complete artifact closure.
- `oxide_benchmark_spec::validate_apple_pr_scenario_set(...)` validates all six canonical Apple PR manifests against the workload matrix.
- `oxide_benchmark_spec::compare_calibrated_static_pngs(...) -> anyhow::Result<CalibratedStaticVisualParityReport>` is the cross-framework static admission gate: full-frame SSIM, a 0.5% stable-interior ratio cap at 2/255 tolerance, and an unmasked localized 16×16 voxel cap.
- `oxide_benchmark_spec::reduce_normalized_png_visual_parity(...) -> anyhow::Result<VisualParityReport>` retains masked SSIM, Delta-E, and text-geometry diagnostics.
- `oxide_benchmark_spec::compare_calibrated_static_pngs(...) -> anyhow::Result<CalibratedStaticVisualParityReport>` owns v5 cross-framework admission with full-frame SSIM, narrow text-line stable-interior accounting, and true 16×16 voxel averaging.
- `oxide_benchmark_spec::compare_exact_static_pngs(...) -> anyhow::Result<ExactStaticVisualParityReport>` remains the exact RGB diagnostic and same-renderer determinism gate.
- `oxide_benchmark_spec::decode_macos_correctness_geometry(...)` validates the observed checkpoint root against the frozen macOS capture profile and supplies the scale used by correctness reduction and release promotion.
- `oxide_benchmark_spec::promote_release_candidates(...)` consumes one real controller correctness pair, content-addresses matching runtime geometry and accepted AppKit screenshots, and atomically emits all five runnable release scenarios plus qualification plans.
- `oxide_benchmark_spec::load_release_candidate_for_capture(...)` validates one screenshot-blocked candidate and its transitive artifact closure for correctness capture without constructing a partial or runnable `ScenarioSpec`.
- `oxide_benchmark_spec::{VisualThresholds, CalibratedStaticVisualThresholds, VisualParityReport, CalibratedStaticVisualParityReport, NonTextVisualMetrics, RasterVisualMetrics, TextGeometryEvidence, TextLineGeometry, TextValidationStatus, LogicalRect, PhysicalRect}` expose the reducer's deterministic input and report contract for `xtask` and host evidence importers.
- `oxide_benchmark_spec::{StartupFixture, StartupCard, DashboardFixture, DashboardCategories, FeedFixture, FeedRow, ChatFixture, ChatMessage, ChatSelectionReplacement, NavigationFixture, NavigationTransition, ImageDecodeZoomFixture, ImageFileFixture}` are the exported typed six-scenario PR fixture contracts.
- `oxide_benchmark_spec::{GridFixture, EffectsFixture, MutationFixture, TextFixture, ResizeFixture}` and their nested value types are the strict exported five-scenario release fixture contracts.
- `oxide_benchmark_spec::load_pr_vertical_scenarios(...)` loads the three canonical manifest filenames in frozen pack order.
- `oxide_benchmark_spec::load_apple_pr_scenarios(...)` loads the six canonical Apple PR filenames in tier order once their artifact closures exist.
- Re-exported comparison types describe Section 8 plans, identities, cells, packs, chunks, metric/decision contracts, sessions, and typed raw observations.
- Re-exported schema types describe platforms, tiers, budgets, scenarios, phase-bound parity checkpoints, traces, and schema version.

## Logic narrative

The fixture loader opens the exact v1 filenames in stable order and deserializes each object. Validation recomputes every component sum, the ceiling 20-percent reserve, and the hard total. Set validation then rejects missing, duplicate, renamed, unexpected, or contract-divergent fixtures. Comparison serde types preserve Section 8 field order and encode u64 timestamps, durations, indices, generations, byte counts, and counters as decimal strings so reviewed artifacts are portable across Rust and JavaScript readers. The exact static comparator normalizes supported PNG channel layouts to opaque sRGB8, proves exact physical dimensions from the logical root and canonical scale, and requires byte-exact RGB equality. The v5 calibrated reducer derives narrow text regions only from runtime text lines, evaluates every pixel with full-frame SSIM and true local voxel averages, and applies the differing-pixel cap to the stable interior. The legacy diagnostic reducer separately rasterizes fixture text masks and emits non-text SSIM, pixel-difference, CIEDE2000, and text-geometry attribution.

## Preconditions and postconditions

- `workspace_root` is the Cargo workspace containing `benchmarks/comparative`.
- Budget files are canonical UTF-8 JSON using schema version one.
- A successful default-set validation proves all nine required files and exact Section 15 headline totals are present.

## Edge cases and failure modes

Missing files, malformed JSON, unknown schema versions, changed components, incorrect reserve rounding, incorrect totals, duplicate IDs, filename/ID mismatches, and wrong shard aggregate/critical-wall values fail closed.

## Concurrency and memory behavior

Loading is synchronous and bounded to nine small files. Returned values own their paths and schema data; no global state or locks are used.

## Performance notes

Budget validation runs before acquisition and is negligible compared with builds or device occupancy. Stable ordered files favor reviewability and reproducibility over parallel I/O.

## Feature flags and cfgs

The crate has no features or target-specific behavior.

## Testing and benchmarks

`tests/budget_tests.rs` covers canonical round trips, exact PR arithmetic, nightly web sharding, and component/reserve corruption. `tests/acquisition_tests.rs` freezes the exact Apple PR expansion. `tests/apple_campaign_plan_tests.rs` freezes generalized macOS nightly/release acquisition structure, missing-artifact preflight, and explicit full-attribution coverage. `tests/instrumentation_calibration_tests.rs` freezes canonical calibration input plus statistical and CPU-margin admission. `tests/scenario_tests.rs` covers canonical scenario shape, trace validation, exact committed font axes and inline-text grid/variant geometry, and malformed-contract rejection. `tests/pr_fixtures_tests.rs` freezes all six typed PR workloads; `tests/release_fixtures_tests.rs` strictly decodes all five release workloads and rejects unknown work; `tests/release_candidate_tests.rs` proves capture-only admission and metadata/hash failure closure. Remaining suites cover scenario closure, comparison schemas, admission, migration, and visual gates.

## Examples

Load the budgets from the Cargo workspace root, call `validate_default_budget_set`, then select the objects whose `platform` and `tier` match the requested plan.

## Changelog

- 2026-07-26: exported v5 true-voxel equivalence admission and froze the rapid thresholds at `0.9795` SSIM, `3%` stable-interior difference, channel tolerance `2`, and voxel delta `45`.
- 2026-07-26: exported v4 semantic-region visual admission with full-frame SSIM, narrow text-line regions, stable-interior differing-pixel limits, and unmasked voxel rejection.
- 2026-07-21: added runtime macOS geometry validation and per-checkpoint geometry binding for release promotion.
- 2026-07-21: added the typed isolated macOS full-attribution matrix and exact derived budget.
- 2026-07-21: added explicit screenshot-blocked release-candidate capture admission without weakening runnable scenario validation.
- 2026-07-21: exported strict typed fixture contracts for all five frozen release candidates.
- 2026-07-21: added generalized macOS nightly/release candidate plans with fail-closed artifact availability and explicit extended/full-attribution contracts.
- 2026-07-19: added strict ComparatorAcceptance v1 admission and the truthful rejected current AppKit audit.
- 2026-07-18: exported the content-addressed inline-text raster variants, logical grid, and frozen image-run metrics.
- 2026-07-18: exported ordered integer-millionths variable-font axes and validated them against hashed `fvar` metadata.
- 2026-07-18: added exact full-frame static Oxide↔UIKit comparison; thresholded and masked reducer results cannot accept a cross-framework static checkpoint.
- 2026-07-18: added the deterministic normalized PNG visual parity reducer and fail-closed text-validation report.
- 2026-07-18: exported typed dashboard, feed, and navigation fixture contracts and exact PR validation.
- 2026-07-18: exported typed startup, chat, and image contracts and promoted the complete six-scenario Apple PR artifact set.
- 2026-07-17: added the canonical Apple PR acquisition expansion and validation.
- 2026-07-17: made style/layout and phase-bound parity evidence content-addressed in the scenario schema.
- 2026-07-17: added the exact legacy Apple method-disposition and required-risk validation contract.
- 2026-07-17: added Section 8 comparison plan/session/metric/decision types and typed raw observation rows.
- 2026-07-17: created benchmark-spec v1 with the nine normative default budget fixtures and fail-closed arithmetic validation.
- 2026-07-17: added the framework-neutral scenario, fairness, and typed integer trace contract.
