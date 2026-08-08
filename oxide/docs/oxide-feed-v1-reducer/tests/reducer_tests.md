# oxide-feed-v1-reducer `tests/reducer_tests.rs`

## Intention and purpose

Protect the feed-v1 publication boundary with deterministic host-side regressions. The suite exercises reducer kernels and a complete synthetic smoke evidence package without launching UIKit, Oxide, Xcode, or a device.

## Relation to the rest of the code

- Calls public testable kernels from `benchmarks/pilots/feed-v1/reducer/src/lib.rs`.
- Reconstructs strict run, device, evidence, cleanup, and full-screen PNG inputs in temporary directories.
- Complements, but does not replace, the signed physical-iPhone smoke and full runs.

## Entry points list

- `strict_success_schema_round_trips_and_rejects_unknown_fields` admits the exact success schema and rejects missing/foreign fields.
- `strict_failure_schema_round_trips_and_rejects_unknown_fields` does the same for non-measurable app failures.
- `travel_equivalence_decision_honors_both_inclusive_frozen_boundaries` covers exact admission boundaries plus isolated median and confidence-interval violations through the production decision kernel.
- `nearest_rank_quantile_uses_one_based_ceiling_rank` rejects empty, non-finite,
  zero-rank, and non-finite-quantile inputs while distinguishing nearest-rank
  p50 from interpolated p50 on an even population.
- `frozen_recipe_matches_contract_extent_and_component_states` checks both endpoint component manifests against the reducer-owned 2,000-row recipe.
- `visual_gate_rejects_one_corrupt_48_pixel_tile` proves localized corruption cannot hide behind whole-image averaging.
- `exact_nine_cluster_interval_freezes_ranks_and_coverage` requires the
  production interval to select ranks 2 and 8, report 96.09375 percent achieved
  coverage, and reject any population other than the frozen nine clusters.
- `frozen_order_maps_each_treatment_and_both_directions_share_it` checks the smoke/primary rotation contract.
- `deadline_formula_uses_previous_callback_target_period` protects aligned missed-callback accounting.
- `callback_admission_rejects_nonfinite_and_terminal_invalid_targets` rejects NaN, a bad final target, and an observed 60 Hz sequence outside `7.5 .. 9.2 ms`.
- `reducer_hard_blocks_failure_and_malformed_controlled_records` proves controlled-name failure/malformed JSON cannot be ignored.
- `reducer_emits_only_admitted_nonsecret_device_identity` admits matching physical-device evidence while excluding device IDs from the report.
- `attachment_export_verifier_requires_one_complete_manifest` rejects incomplete, unreferenced, or canonical-path-aliased XCTest export trees.
- `attachment_export_rejects_hard_link_file_identity_aliases` rejects two capture names backed by the same Unix file identity.
- `evidence_manifest_excludes_stale_reports_results_and_build_outputs` proves only protocol/iOS/reducer source enters `source_files`, while stale latest reports, evidence JSON, targets, raw results, and XCTest result bundles cannot contaminate it; it also proves the clean named Git commit/tree, strict device/toolchain/dependency/signing provenance, and compiled controller hashes are present.
- `six_valid_smoke_tuples_pass_smoke_but_not_full_publication` builds the complete synthetic six-tuple package, proves smoke admission, proves no report was written, and proves full reduction remains blocked until the complete 60-record evidence population is present.
- `smoke_admits_named_output_placeholders_as_evidence` proves smoke verification cannot hide an evidence record whose path happens to equal its internal no-write output placeholder.
- `smoke_scans_build_and_target_named_directories` proves evidence discovery exempts neither conventional output-directory name.
- `smoke_rejects_symlink_roots_entries_and_nonregular_files` proves Unix evidence discovery rejects root aliases, nested symlinks, and socket entries instead of silently omitting them.
- `smoke_rejects_cleanup_and_artifact_provenance_mutations` rejects false Xcode/attachment/source-snapshot/result-root/process/fuse cleanup proof, controller runtime beyond the frozen caps, app-record container swapping, screenshot attachment substitution, and duplicate authority records.
- `full_reduction_is_byte_identical_when_repeated` builds six smoke diagnostics plus the one 54-run primary block, gives one cluster many more raw callbacks than the other eight, and makes the optimized comparator intentionally faster so the fixed-order terminal decision must preserve a mixed `non-inferior`/`slower` pair. It verifies cluster aggregates, all 54 raw-sample rows, structured policy/cleanup, the sorted exact raw-file inventory, compact Markdown without raw rows, the canonical JSON path/hash, revision-5 exact interval metadata, and byte-identical repeat reduction.
- `full_reduction_rejects_systematic_primary_travel_mismatch` proves a repeatable six-percent Oxide workload mismatch blocks both direction comparisons even though each confidence interval remains inside the wider ten-percent bound.

## Logic narrative

Tests construct only the minimum fixture needed for the contract under review. The end-to-end helper writes strict run JSON for the selected population, one deterministic `1320 x 2868` PNG for each of the six smoke tuples, physical-device/lock evidence, the source/app evidence manifest, and a successful cleanup proof. Focused hostile cases then mutate one invariant at a time, including replacement with an already-cropped `1170 x 2532` image.

## Preconditions and postconditions

- Temporary roots are unique and remain outside the repository.
- A passing suite means deterministic reducer logic accepts the valid synthetic smoke population and rejects every represented hostile condition.
- Passing does not claim real device rendering, gesture delivery, or timing evidence.

## Edge cases and failure modes

- The suite includes non-finite callback data and a terminal invalid target so validation cannot stop after only the intervals it consumes.
- Neither a syntactically controlled filename nor the smoke verifier's internal output-placeholder names can bypass malformed/failure record blocking.
- The smoke regression forbids accidental publication from a cheap six-run check.
- The strict run regression rejects all three treatments below the 524-point travel floor, a transient environment notification despite equal endpoints, and drag-only motion without inertial entry.

## Concurrency and memory behavior

Tests are independent and use unique temporary directories. Each integration test removes its synthetic evidence root; callers keep Cargo output external to the repository.

## Performance notes

- Population admission precedes expensive image/adversarial work, keeping malformed or incomplete test cases cheap.
- The two full-image synthetic tests exercise streamed hostile mutations, shared RGB accumulation, and exact identity rows over the frozen 3 MP surface.
- The exact-interval test contains no RNG, resample budget, or randomized soak
  loop.
- The full synthetic population contains exactly one 54-run primary block and nine paired clusters; no second block is generated.
- The full report regression hashes one retained manifest independently, counts
  all 60 app records and six screenshots in the sorted inventory, and verifies
  Markdown contains neither the raw-sample field nor a per-run table.

## Feature flags and cfgs

No feature flags or device cfg branches.

## Testing and benchmarks

```sh
cd oxide
cargo test --locked -p oxide-feed-v1-reducer
```

Expected integration result: every focused integration case passes.

## Changelog

- 2026-08-07: Added hostile cleanup mutations for prelaunch, build, result-bundle, evidence-file, and exact process-absence fuses.
- 2026-08-07: Added strict revision-3 evidence provenance for the frozen phone, toolchains, dependency graph, resolved settings, and all signed products.
- 2026-08-07: Added mixed-comparator terminal-decision, raw-sample retention,
  structured-policy, cleanup-proof, evidence-inventory, and canonical-JSON hash
  coverage while requiring compact aggregate-only Markdown.
- 2026-08-07: Added direct nearest-rank boundary coverage and an unequal-size
  raw-callback population proving treatment p50/p95 are cluster medians rather
  than pooled callback quantiles.
- 2026-08-07: replaced bootstrap determinism with exact rank, achieved-coverage,
  population-cardinality, revision-4 serialization, and retired-field checks.
- 2026-08-07: Switched focused commands to the shared root workspace graph.
- 2026-08-06: Reduced synthetic smoke evidence to six full-screen PNGs, added pre-cropped-input rejection, and limited full-report assertions to the 54 primary rows.
- 2026-08-06: Added cleanup, provenance, transition/inertia, full-population per-run, and byte-identical rerun regressions.
- 2026-08-06: Added source-manifest contamination coverage for stale reports, results, evidence, targets, and XCTest artifacts.
- 2026-08-06: Added strict schema, fixture, visual, callback, device, attachment, cleanup, and synthetic smoke/full-boundary coverage.
