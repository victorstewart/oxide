# oxide-benchmark-spec::tests::release_candidate_tests

## Intention and purpose

These tests prove the screenshot-capture loader accepts exactly the five frozen release candidates and fails closed when their admission metadata or hash-bound inputs drift.

## Relation to the rest of the code

The tests exercise only the public `load_release_candidate_for_capture` API. They complement promotion tests, which require paired runtime evidence, and runtime tests, which exercise Metal preparation through the capture-only FFI.

## Entry points list

The test binary exports no public API. Cargo invokes one all-candidate acceptance case and two mutation-driven rejection cases.

## Logic narrative

The acceptance case loads every frozen identity, checks its content hash and nonempty capture contracts, and confirms no runnable manifest exists. Rejection tests copy the specification tree to isolated temporary directories, rewrite one canonical candidate field at a time, introduce a second newline, alter a declared fixture hash, and alter the fixture bytes. Each mutation must produce the corresponding admission or hash error.

## Preconditions and postconditions

The committed version-one specification tree must exist. Tests never alter it; mutation occurs only in temporary copies.

## Edge cases and failure modes

The cases cover unknown identity, identity mismatch, status mismatch, owning-pass mismatch, destination mismatch, noncanonical bytes, declared hash mismatch, and observed artifact mismatch.

## Concurrency and memory behavior

Each mutation test owns its temporary tree. No mutable files or process-global state are shared between tests.

## Performance notes

Tree copies and hashing are intentionally offline correctness work and are not performance benchmarks.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-benchmark-spec --test release_candidate_tests` with a bounded external target directory.

## Examples

The `rewrite` helper demonstrates how a structurally valid, canonical mutation is produced so the test reaches the intended validation boundary rather than failing JSON parsing first.

## Changelog

- 2026-07-21: added all-candidate admission and fail-closed metadata/hash mutation coverage.
