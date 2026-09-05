# Trusted-input tests

## Purpose

`trusted_input_tests.rs` freezes the Rust comparison controller's macOS trusted-input capability boundary.

## Coverage

The focused tests prove that one trace can compile clicks, wheel input, composer text, the exact two-event `chat:append:16` select/replace operation, a matching key pair, and a duration-preserving single-pointer drag while keeping application stimuli separate. They prove arbitrary selection ranges, targets, replacements, and adjacency fail closed alongside pinch/multipointer input, pointer cancellation, IME composition, and lifecycle transitions without controller receipts. Favorite mutation is classified as a user click while other mutations remain application stimuli. Repository-backed cases freeze that the current grid detail/back trace becomes exactly two clicks and the current chat select/replace trace becomes one typed command, while pinch and interactive cancellation retain their declared capability gaps.

Module-level receipt tests additionally prove canonical request/controller/application triplets produce a campaign-relative deterministic manifest, missing and extra artifacts fail, broken hash links fail, mismatched raw event ranges/families fail, non-advancing generations fail, and cross-process timestamp misordering fails. The valid fixture advances generation by two, freezing that multi-event commands are not restricted to an exact `+1` transition. Iteration-overlay coverage proves preflight expands the exact session command stream and preserves scenario-global event indexes. Repository-backed preflight proves the complete current chat scenario includes the typed select/replace command. Analyzer-bridge coverage proves deletion, byte tampering, and extra raw receipt files cannot survive publication-time hash closure.

## Running

From the Rust workspace root, use a bounded external target directory:

```sh
CARGO_TARGET_DIR=/private/tmp/oxide-trusted-input-target cargo test --locked -p oxide-apple-comparison-controller --test trusted_input_tests
```

Remove that target after the focused verification run.

## Changelog

- 2026-07-21: introduced focused supported-command and unsupported-capability coverage.
- 2026-07-21: added positive manifest coverage and negative receipt completeness, linkage, generation, and temporal-order coverage.
- 2026-07-21: added frozen select/replace compilation, unsafe-range rejection, repository-backed preflight, and raw range/family receipt rejection.
