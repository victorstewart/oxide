# `oxide-apple-comparison-controller::tests::launch_tests`

## Intention and purpose

These integration tests freeze the controller-owned proof boundary for real macOS terminated-process launch evidence.

## Relation to the rest of the code

The tests exercise only the public `validate_macos_launch_evidence` entry point. They construct deterministic evidence without launching an application or Instruments process.

## Entry points list

- `terminated_warm_launch_evidence_accepts_complete_ordered_exact_pid_proof()` proves the complete happy path.
- `launch_evidence_rejects_trace_started_after_launch_request()` proves post-launch tracing cannot enter the launch cell.
- `launch_evidence_rejects_prepared_ui_before_application_launch_callback()` proves UI work cannot predate the application lifecycle callback.
- `launch_evidence_rejects_identity_or_primer_mutation()` proves generation and cache-primer identities fail closed.

## Logic narrative

A shared valid fixture binds one native-side session to a canonical plan, generation, executable, primer receipt, trace scope, and ordered milestone sequence. Each failure test mutates one contract dimension and observes public validator rejection.

## Preconditions and postconditions

Fixtures use path-safe run IDs and canonical lowercase SHA-256 strings. Passing tests guarantee deterministic validation behavior only; they do not substitute for live launch acquisition.

## Edge cases and failure modes

The suite covers a late trace, lifecycle/UI milestone inversion, malformed primer hashes, and a changed session generation. Additional live trace/export failures remain controller integration concerns.

## Concurrency and memory behavior

Tests are synchronous and operate on bounded in-memory structures.

## Performance notes

No measured application or performance-sensitive code is executed.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-apple-comparison-controller --test launch_tests`.

## Examples

The `evidence()` helper is a complete minimal public-API fixture.

## Changelog

- 2026-07-19: added launch-evidence admission coverage.
