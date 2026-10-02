# `xtask/tests/xctrace_record_tests.rs`

## Intention and purpose

These integration tests prove the storage-safety contract for `xtask` trace recording without launching Instruments or requiring an Apple device.

## Relation to the rest of the code

The tests exercise the public owner in `xtask::xctrace_record` and inspect `xtask::lib` only to ensure every `xctrace record` construction site uses that owner.

## Entry points

The Rust test harness reaches four `#[test]` functions covering environment isolation, combined working-set growth, drop cleanup, and call-site integration.

## Logic narrative

Short shell stand-ins expose inherited temporary-directory variables. Bounded sleep children model a long-running recorder so tests can create trace and scratch bytes synchronously, trip the fuse, or drop the owner. The source coverage test counts the two record argument constructions and verifies both enclosing functions use `XctraceRecordProcess::spawn`.

## Preconditions and postconditions

The host must provide `/bin/sh`, `/bin/sleep`, and `/bin/kill`. Every test uses its own temporary directory and asserts owned files and children are gone before returning.

## Edge cases and failure modes

The tests deliberately exceed a tiny test-only limit using bytes split across the final trace and scratch directory. They do not depend on timing sleeps or on Instruments availability.

## Concurrency and memory behavior

The production monitor thread runs during each test and is joined by commit or drop. Test artifacts are only a few bytes.

## Performance notes

These are correctness tests for benchmark tooling and do not measure application performance.

## Feature flags and cfgs

No feature flags are required. The process-group behavior is Unix-specific, matching the Apple host requirement.

## Testing and benchmarks

Run with `cargo test --locked -p xtask --test xctrace_record_tests` from the Rust workspace.

## Examples

Each test is a minimal executable example of owner creation, explicit commit, fuse enforcement, or cleanup through drop.

## Changelog

- 2026-07-21: added deterministic non-Instruments coverage for the xctrace storage owner.
