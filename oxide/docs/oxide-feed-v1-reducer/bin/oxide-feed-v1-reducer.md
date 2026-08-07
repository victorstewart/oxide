# oxide-feed-v1-reducer `src/main.rs`

## Intention and purpose

The binary is a thin command dispatcher for the feed-v1 reducer library. It keeps device orchestration in `run-device.sh` and confines this process to deterministic evidence validation, hashing, reduction, and report writes.

## Relation to the rest of the code

- Dispatches directly into `oxide_feed_v1_reducer` library entry points.
- Is built into an external temporary directory by the physical-device runner.
- Is hashed into the evidence manifest, used for one reduction, and removed before a successful retained package is admitted.

## Entry points list

- `main() -> ExitCode` calls the argument dispatcher, returns success only for an admitted operation, and prints one prefixed error before returning failure.
- `run() -> Result<(), String>` parses exactly one command and dispatches into the library.
- `reduce <run-root> <latest.json> <latest.md>` admits six smoke diagnostics plus one 54-run primary block and publishes four travel-equivalence decisions plus the 54 primary timing rows.
- `manifest <source-root> <repository-root> <UIKit.app> <Oxide.app> <FeedV1Controller-Runner.app> <FeedV1Controller.xctest> <output.json>` creates the clean-Git/source/app/controller/reducer/font evidence manifest.
- `verify-attachments <root>` validates the frozen six-file export from the exact controller XCTest.
- `verify-smoke <run-root>` admits the six smoke tuples without producing a report.

## Logic narrative

`main` calls `run`, maps success to exit code zero, and prints one prefixed error to standard error for any rejected command or library failure. `run` parses exactly one subcommand, requires every positional path/count, and rejects all trailing arguments before dispatch.

## Preconditions and postconditions

- Commands receive filesystem paths explicitly; there is no implicit current-result directory.
- `verify-attachments` has no count knob; six files are part of the frozen protocol.
- Success means the selected library operation completed. Failure always returns a nonzero exit code.

## Edge cases and failure modes

- Missing or extra arguments return a precise parse error.
- Unknown subcommands return the complete one-line usage contract.
- All schema, evidence, and output failures are propagated without being converted into a partial success.

## Concurrency and memory behavior

The dispatcher starts no threads and retains only the parsed arguments. Memory and I/O ownership belong to the selected library operation.

## Performance notes

The CLI adds no sampling loop, app launch, or benchmark instrumentation. Its work is outside the measured UIKit/Oxide intervals.

## Feature flags and cfgs

No feature flags or platform cfg branches.

## Testing and benchmarks

Library and integration coverage runs with:

```sh
cargo test --locked --manifest-path oxide/benchmarks/pilots/feed-v1/reducer/Cargo.toml
```

The physical-device runner exercises all four subcommands across its smoke and full modes. Reducer tests cover exact attachment authority, path/file-identity alias rejection, and report-schema/travel-decision output.

## Examples

```sh
oxide-feed-v1-reducer verify-smoke /tmp/feed-v1-smoke
```

## Changelog

- 2026-08-06: Bound attachment verification to the exact six-file controller export and documented revision-3 travel-equivalence output.
- 2026-08-06: Added strict `reduce`, `manifest`, `verify-attachments`, and `verify-smoke` dispatch.
