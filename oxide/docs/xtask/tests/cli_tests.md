# xtask tests::cli_tests

## Intention and purpose

This integration suite verifies command dispatch and filesystem/tool adapters at the public `xtask::run_cli` boundary.

## Relation to the rest of the code

- Exercises experiment-manifest, comparison-budget, exact static comparison, normalized visual-reduction, and shader-bundling commands.
- Uses temporary directories and a stub `xcrun` where external compilation behavior is not the subject.
- Protects fail-closed behavior for unfinished comparison acquisition commands and incomplete Apple transport finalization invocations.

Call flow:

- argument vector
  - `run_cli`
  - command parser
  - validator/planner or test stub

## Entry points list

The file exports no library entry points. Rust's test harness reaches each `#[test]` function.

## Logic narrative

Tests construct explicit CLI vectors and assert successful validation/planning or expected errors. Comparison tests prove `validate` and `plan` execute real benchmark-spec validation, including the canonical Apple PR four-acquisition expansion, while campaign acquisition, coverage explanation, and other unfinished commands return errors instead of successful no-ops. A dry-run combined with either `--correctness-only` or `--resume` is rejected because live acquisition controls must not mutate the canonical dry-run expansion. Visual tests generate tiny opaque PNGs, layout masks, and ordered text geometry to prove exact full-frame acceptance and one-channel rejection, accepted diagnostic atomic replacement, newline-stable JSON, persisted rejected evidence before failure, diagnostic-only zero exit, and required canonical arguments.

The Apple-correctness command is pinned to the controller's public reducer API, and the former `xtask::apple_comparison` module is checked not to retain a duplicate implementation.

## Preconditions and postconditions

The workspace contains committed comparison budget and Apple PR acquisition fixtures. Unix shader tests can set executable permissions on a temporary `xcrun` stub.

## Edge cases and failure modes

Coverage includes empty/unknown legacy usage, custom experiment manifests, missing comparison implementation, incomplete coverage planning, accepted/pending/rejected visual reports, absent shader directories, and stubbed shader compilation.

## Concurrency and memory behavior

Temporary roots isolate filesystem work. The PATH-mutating shader helper assumes those particular tests are not concurrently invoking external Apple tools.

## Performance notes

Tests avoid device or browser acquisition and run only host control-plane logic.

## Feature flags and cfgs

Unix-only permission setup is guarded by `cfg(unix)`.

## Testing and benchmarks

Run `cargo test --locked -p xtask --test cli_tests` from the `oxide` workspace.

The promoted macOS qualification dry-run test requires the generated plan to expand to exactly 52 isolated side sessions, proving its plan-owned budget reaches the controller without a separate budget artifact.

## Examples

The comparison tests invoke `compare-ui plan --platform ios --tier pr --explain-budget`, the two-shard nightly web budget plan, `compare-ui compare-static-exact --oxide ... --uikit ...`, and the diagnostic `compare-ui reduce-visual` path.

## Changelog

- 2026-07-26: added regression coverage for the promoted 52-session macOS qualification dry-run and generated-budget admission.
- 2026-07-21: Added fail-closed CLI coverage for explicit macOS density plan/driver/output acquisition semantics.

- 2026-07-21: added fail-closed argument coverage for `compare-ui materialize-macos-analyzer`.
- 2026-07-19: covered fail-closed rejection of correctness-only scope on the deterministic dry-run path.
- 2026-07-19: covered fail-closed rejection of resume on the deterministic dry-run path.
- 2026-07-18: covered exact static comparison acceptance, one-channel rejection, atomic evidence persistence, and diagnostic exit behavior.
- 2026-07-18: covered accepted atomic visual reports, fail-closed pending/rejected persistence, diagnostic exit behavior, and required reducer arguments.
- 2026-07-17: added real comparison budget validation/planning and fail-closed unfinished-command coverage.
