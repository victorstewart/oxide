# `oxide-image-sampling-diagnostic` binary

## Intention and purpose

The binary is the narrow command-line entry point for persisting the macOS image sampling report without launching either benchmark application.

## Relation to the rest of the code

It validates two positional arguments, delegates all work to `oxide_image_sampling_diagnostic::run`, prints the resulting report path, and maps invalid usage or probe failures to nonzero exit status.

## Entry points

- `main()`: accepts `<source.png> <output-directory>` and runs the comparison-only diagnostic.

## Logic narrative

Argument validation precedes all filesystem or GPU work. A valid invocation delegates to the library, prints one path on success, and otherwise emits one diagnostic error to stderr.

## Preconditions and postconditions

Exactly two arguments are required. Successful execution means the library validated and persisted the complete report; visual differences do not make execution fail.

## Edge cases and failure modes

Missing or excess arguments exit with status 2. Decode, filesystem, Apple-toolchain, Metal, or report-contract failures exit with status 1.

## Concurrency and memory behavior

The binary adds no concurrency or material allocation beyond the library operation.

## Performance notes

The binary is outside every production path and makes no performance claim.

## Feature flags and cfgs

None.

## Testing and benchmarks

Library integration tests cover the deterministic inputs; the complete command is documented in the library page.

## Examples

```text
cargo run --locked -p oxide-image-sampling-diagnostic -- source.png /private/tmp/oxide-image-sampling
```

## Changelog

- 2026-07-19: added the standalone diagnostic entry point.
