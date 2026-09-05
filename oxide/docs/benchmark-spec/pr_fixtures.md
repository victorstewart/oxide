# oxide-benchmark-spec::pr_fixtures

## Intention and purpose

`pr_fixtures` gives all six PR comparison adapters one shared typed interpretation of their materialized JSON. The types prevent adapters from independently guessing field names or silently ignoring new fixture work, while the crate-private validators freeze the exact PR workload rather than accepting headline counts alone.

## Relation to the rest of the code

The data path is:

- materialized JSON below `benchmarks/comparative/specs/v1/fixtures`;
- `pr_scenarios::validate_pr_vertical_slice` first proves the fixture path and SHA-256 through `validate_scenario_artifacts`;
- `pr_scenarios` deserializes that verified byte stream into the matching typed fixture;
- the exact fixture validator rejects workload drift before an Oxide or comparator adapter prepares a scene;
- test-scenes and platform adapters may consume the re-exported fixture types after validation.

## Entry points list

- `oxide_benchmark_spec::DashboardCategories` exposes the five frozen visible dashboard category counts.
- `oxide_benchmark_spec::DashboardFixture` exposes the dashboard identity, 300-node composition, effect counts, and deterministic mutation identities.
- `oxide_benchmark_spec::FeedRow` exposes one deterministic variable-height feed row.
- `oxide_benchmark_spec::FeedFixture` exposes the 2,000 rows, 128-thumbnail domain, and 20 prepend identities.
- `oxide_benchmark_spec::NavigationTransition` exposes the duration and curve of the product transition.
- `oxide_benchmark_spec::NavigationFixture` exposes the 12-item list, four PR cycles, modal transition, interactive-cancel fraction, and selected item.
- `oxide_benchmark_spec::StartupCard` exposes one card's stable identity, 1 KiB data slice, thumbnail, and initial visibility.
- `oxide_benchmark_spec::StartupFixture` exposes the 24-card first-screen fixture, exact 24 KiB data payload, six initial images, header, navigation, and control.
- `oxide_benchmark_spec::ChatMessage` exposes stable message, sequence, author/avatar, direction, and multilingual text fields.
- `oxide_benchmark_spec::ChatSelectionReplacement` exposes the frozen UTF-8 selection and replacement action.
- `oxide_benchmark_spec::ChatFixture` exposes 5,000 messages, 64 avatars, 50 prepends, 10 Hz append rate, 100-character typing, 10 KiB paste, and selection replacement.
- `oxide_benchmark_spec::ImageFileFixture` exposes one content-addressed PNG's exact dimensions, format, and color space.
- `oxide_benchmark_spec::ImageDecodeZoomFixture` exposes the 4096x3072 source, 384x288 thumbnail, normalized pan distance, and 2x pinch scale.
- `oxide_benchmark_spec::EnduranceFixture` exposes the dashboard-derived heavy screen plus exact 100-cycle, 500-switch, and 600-frame nightly churn limits and target identities.

All fields are public because adapters need direct read-only access after deserialization. The types implement Serde serialization and deserialization; unknown JSON fields are rejected so a schema expansion cannot bypass review.

## Logic narrative

Dashboard validation first freezes version and identity, then verifies the category counts and their checked sum of 300. It independently freezes 32 clipped cards, 32 shadows, four backdrop blurs, the ordered 20-leaf update sequence, and the ordered 30-node ten-percent mutation sequence.

Feed validation freezes the declared and materialized row counts, the 128-thumbnail domain, and all 2,000 rows. Each row is checked against its deterministic index-derived ID, height, multilingual text, thumbnail, initial favorite state, and LTR/RTL direction. The 20 prepend IDs are also checked in order.

Navigation validation freezes 12 list items, four canonical PR cycles, the 300 ms ease-in-out transition, the exact 0.5 interactive-cancel point, and item 05 as the selected route.

Startup validation checks every byte of the deterministic 24 KiB payload, all 24 contiguous 1 KiB card spans, exactly six initially visible cards/images, and the header/navigation/control identities. Chat validation checks all 5,000 base messages and 50 prepends against the deterministic multilingual text, direction, sequence, author, and avatar formulas; it also freezes the 10 Hz append contract, 100-character input, 10 KiB paste, and replacement range. Image validation verifies both artifact hashes, PNG signatures and IHDR dimensions, sRGB/PNG identity, and the normalized pan/pinch values.

## Preconditions and postconditions

The generic artifact validator must prove the fixture is a content-addressed file beneath the canonical spec root before the typed validator reads it. Success proves that the verified bytes encode the exact first vertical-slice fixture contract. It does not prove that an adapter renders the declared work or that two adapters produce matching pixels; parity checkpoints own those later gates.

## Edge cases and failure modes

Unsupported versions, wrong IDs, unknown JSON fields, integer type/range errors, category arithmetic overflow, missing or extra rows, altered payload bytes, overlapping card spans, reordered deterministic IDs, altered multilingual text, out-of-sequence thumbnails/avatars, wrong input sizes, initially favorited rows, direction changes, transition drift, invalid PNG headers/dimensions/hashes, and gesture-contract drift fail validation. JSON cannot encode non-finite navigation fractions, and the exact contract accepts only the binary-exact value `0.5`.

## Concurrency and memory behavior

Fixture values are owned, `Send` and `Sync` when their standard collection members are, and contain no locks or shared mutable state. Deserialization allocates the strings and vectors once during planning or scene preparation. The 2,000 feed rows are intentionally materialized because every adapter must consume the same complete fixture.

## Performance notes

Exact validation is linear in materialized fixture size: 24 KiB startup data, `O(300)` dashboard identities, `O(2,000)` feed rows, `O(5,050)` chat messages, constant navigation work, and two image-file reads. It runs before measured phases and performs no renderer or frame-loop work. Deterministic index formulas avoid a second stored expectation table while preserving complete byte-level workload intent.

## Feature flags and cfgs

No feature flag or target cfg changes these schemas.

## Testing and benchmarks

`tests/pr_fixtures_tests.rs` deserializes all committed files through the public types, proves unknown fields fail, and changes feed/chat semantics while updating the declared SHA-256 to prove exact validators reject drift beyond content addressing. `tests/pr_scenario_tests.rs` proves both the three-scenario vertical slice and complete six-scenario PR set pass artifact and workload validation. No runtime performance case is required because validation runs outside acquisition windows and changes no renderer behavior.

## Examples

```rust
let fixture = serde_json::from_slice::<oxide_benchmark_spec::FeedFixture>(&bytes)?;
assert_eq!(fixture.rows.len(), fixture.row_count as usize);
```

## Changelog

- 2026-07-21: added the typed, unknown-field-denying `endurance.churn` fixture contract.
- 2026-07-18: added typed, unknown-field-denying dashboard, feed, and navigation fixture contracts with exact PR workload validation.
- 2026-07-18: added typed startup, chat, and image fixture contracts, including byte-exact payload and PNG validation.
