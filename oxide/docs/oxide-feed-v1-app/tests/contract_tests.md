# oxide-feed-v1-app `tests/contract_tests.rs`

## Intention and purpose

This integration suite proves the Rust fixture is an independent, byte-exact reconstruction of the authoritative Swift feed-v1 contract. It also freezes every deterministic row/checker decision and validates that baseline placement constants come from the actual embedded Asap font tables.

## Relation to the rest of the code

- Calls only the public API documented in [`contract.md`](../contract.md).
- Reads the same embedded Asap font files used by `src/lib.rs` and hashed by the pilot evidence manifest.
- Protects the identity and geometry assumptions consumed by app tests, observation serialization, UIKit treatments, and the reducer.

Call graph:

- canonical test -> `canonical_fixture_identity(FeedFixture::new())` -> production canonical streamer -> exact byte count + SHA-256
- prefix test -> every row -> prefix and endpoint invariants
- font test -> embedded TTF tables -> frozen Asap units -> UIKit-compatible baseline helpers

## Entry points list

- `rust_recipe_reproduces_the_complete_swift_canonical_identity` invokes the production streamer and checks its exact 717,745-byte count, frozen SHA-256, and `is_expected` admission result.
- `prefix_and_rows_are_deterministic_at_every_index` reconstructs both fixture instances and checks all 2,000 row heights/prefix entries.
- `checker_recipe_is_bounded_deterministic_and_opaque` checks all 64 variants, repeatability, opaque alpha, and out-of-range rejection.
- `checker_payload_is_exactly_the_frozen_source_size` freezes the 12-by-12 RGBA8 storage contract.
- `uikit_label_box_tops_use_the_embedded_asap_vertical_metrics` parses real TTF metrics and checks title/caption/metadata baseline math at device-pixel precision.

## Logic narrative

The identity test deliberately calls the production `canonical_fixture_identity` function rather than retaining a test-only encoder. That makes runtime admission and the Swift parity assertion depend on one canonical order and removes a duplicate implementation that could drift. Separate focused tests localize failures to row/prefix, checker, payload-size, or typography causes rather than reporting only one digest mismatch.

The font helper walks the TTF table directory, locates `head` and `hhea`, and reads big-endian units-per-em, ascender, and descender. It does not trust constants copied beside the test.

## Preconditions and postconditions

- Font bytes are the repository assets used by the app, not test fixtures or platform-installed fonts.
- Canonical serialization order and integer width/endianness match the Swift fixture contract.
- A passing digest implies both exact byte count and exact content for every represented field.
- All checker output buffers begin with hostile fill bytes so the test proves complete overwrite.

## Edge cases and failure modes

- Missing/malformed TTF table bounds return `None` and fail the typography assertion without unsafe reads.
- Checker variant 64 is explicitly rejected and leaves no claim of a fallback image.
- All row indices are checked, preventing a sparse sample from hiding a deterministic mismatch.
- Device-pixel snapping catches subpoint baseline drift relevant at 3x scale.

## Concurrency and memory behavior

Tests share only immutable font bytes and constants. The identity call owns bounded production scratch and no complete byte vector. Checker buffers are fixed stack arrays. No environment state, filesystem writes, threads, locks, or device resources are used.

## Performance notes

The complete identity test intentionally performs one bounded 2,000-row pass; there are no repeated soak loops. The app performs the same bounded pass once at startup, outside measured frame and gesture work.

## Feature flags and cfgs

No feature flags or platform cfg branches.

## Testing and benchmarks

```sh
cd oxide
cargo test --locked --offline -p oxide-feed-v1-app --test contract_tests
```

These are deterministic contract tests, not performance measurements.

## Examples

```sh
cd oxide
cargo test -p oxide-feed-v1-app --test contract_tests rust_recipe_reproduces
```

## Changelog

- 2026-08-07: Switched focused commands to the shared root workspace graph.
- 2026-08-06: Reused the production canonical streamer and deleted the duplicate test-only byte builder.
- 2026-08-06: Moved complete identity, row, checker, payload, and real-font coverage out of source into a mapped integration suite.
