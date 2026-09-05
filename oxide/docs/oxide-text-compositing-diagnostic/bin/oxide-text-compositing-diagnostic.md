# `oxide-text-compositing-diagnostic` binary

## Intention and purpose

The binary provides the narrow command-line boundary for running the offscreen title experiment.

## Relation to the rest of the code

It parses the pinned font path and output directory, calls `oxide_text_compositing_diagnostic::run`, and prints only the completed CoreText report path. Production-atlas preparation, both offscreen helpers, the companion `metal-report.json`, and all validation remain library-owned.

## Entry points

- `main()`: validates two positional arguments, executes the diagnostic, and returns a nonzero process status on failure.

## Logic narrative

Argument errors use status 2. Runtime or report-contract failures use status 1. A successful invocation prints `report.json` only after both reports and every CoreText/A8/Metal artifact identity have been checked.

## Preconditions and postconditions

The font and output paths must satisfy the library contract. Success guarantees the printed report exists and passed validation.

## Edge cases and failure modes

Missing or extra arguments, helper failures, and validation failures emit concise stderr messages.

## Concurrency and memory behavior

The binary performs one sequential library call.

## Performance notes

The binary is diagnostic-only and unmeasured.

## Feature flags and cfgs

No feature flags.

## Testing and benchmarks

Library integration tests exercise the command's complete underlying workflow.

## Examples

See the crate documentation for the full invocation.

## Changelog

- 2026-07-19: added the bounded command-line entry point.
- 2026-07-19: extended success to require the exact production-atlas Metal stage report.
