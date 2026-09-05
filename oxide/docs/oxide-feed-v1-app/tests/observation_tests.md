# oxide-feed-v1-app `tests/observation_tests.rs`

## Intention and purpose

This integration suite freezes the exact JSON evidence boundary for successful and failed feed-v1 runs. It verifies required keys and nested shapes, actual endpoint component coordinates/clips, mandatory environment transition counts, and nonce safety through only the public observation/contract APIs.

## Relation to the rest of the code

- Exercises serializers and nonce validation documented in [`observation.md`](../observation.md).
- Builds geometry through the public frozen fixture documented in [`contract.md`](../contract.md).
- Mirrors the strict reducer's expected record surface without depending on reducer implementation details.
- Complements app-level terminal-file coverage in [`lib_tests.md`](lib_tests.md).

Call graph:

- test fixture -> `FeedFixture` visible range/component rectangles -> `RunRecord`
- `RunRecord` -> `write_record_json` -> `serde_json` parse -> exact-key/geometry assertions
- hostile nonce/failure values -> public observation validators/writers

## Entry points list

- `complete_record_matches_the_authoritative_nested_schema` checks all top-level sections, fixture/run identity, complete status, `gesture.inertia_observed`, callback cardinality, and nonzero transition fields.
- `bottom_components_keep_content_coordinates_and_translate_their_clips` proves bottom geometry stays in content space while clips become viewport-relative.
- `failure_record_uses_the_strict_non_measurable_schema` requires the separate flat failure shape and absence of run/display-link data.
- `launch_failure_can_truthfully_omit_unparsed_inputs` permits null nonce/treatment only in a launch failure record.
- `nonce_validation_prevents_path_or_notification_injection` covers valid controller forms and rejects traversal, underscore, period, slash, whitespace, and newline spellings outside the shared ASCII alphanumeric-or-hyphen grammar.

## Logic narrative

Test records borrow deterministic callback arrays and component vectors constructed from `FeedFixture`. They serialize into bytes with the production streaming writer, parse only for ergonomic assertions, and require exact object cardinalities so an accidental extra or missing field fails. Both top and bottom endpoints use all visible component kinds in stable order.

The success record intentionally uses `inertia_observed=true` and nonzero thermal/low-power change counts to prove fields are serialized from record data rather than inferred from travel or hard-coded zero. App integration separately proves zero remains explicit for a no-transition native completion.

## Preconditions and postconditions

- Test records use a valid nonce, exact fixture, positive 120 Hz endpoint states, monotonic timestamps, and deterministic visible components.
- Every complete success object has exactly the approved top-level keys and exact fixture identity.
- Geometry rectangles and clips have positive area and remain inside the frozen 1,170-by-2,532-pixel viewport.
- Failure objects cannot contain success timing or run sections.

## Edge cases and failure modes

- Bottom content Y coordinates are deliberately much larger than viewport clip Y, catching accidental coordinate-space collapse.
- Null launch fields are tested independently from invalid nonce persistence, which remains rejected.
- Exact-key assertions catch permissive schema drift even when JSON remains syntactically valid.
- String access uses defensive fallbacks only to produce clear assertion failures, never to accept missing data.

## Concurrency and memory behavior

All values are test-local; no environment variables, files, notifications, threads, or locks are used. JSON and component vectors allocate only in test code. Production serializers borrow inputs and stream output.

## Performance notes

The suite serializes one small visible-component population per case. It contains no exhaustive 2,000-row reconstruction or repeated benchmark loop; those concerns belong to contract tests and the device pilot respectively.

## Feature flags and cfgs

No feature flags or platform cfg branches. Tests call the pure serializers, so their result is identical on native and iOS targets.

## Testing and benchmarks

```sh
cd oxide
cargo test --locked --offline -p oxide-feed-v1-app --test observation_tests
```

This validates schema logic only; physical runs validate durable files and notifications.

## Examples

```sh
cd oxide
cargo test -p oxide-feed-v1-app --test observation_tests complete_record_matches
```

## Changelog

- 2026-08-07: Switched focused commands to the shared root workspace graph.
- 2026-08-06: Added explicit underscore and period rejection to keep Rust nonce admission symmetric with UIKit.
- 2026-08-06: Added required direct-inertia and nonzero thermal/low-power transition-field coverage.
- 2026-08-06: Moved exact success/failure schema, geometry, and nonce coverage into a mapped public-API integration suite.
