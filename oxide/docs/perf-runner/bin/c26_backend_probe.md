# oxide-perf-runner::bin::c26_backend_probe

## Intention and purpose

This standalone probe measures the C26 retained dynamic-property backend workload without changing production renderer behavior.

## Relation to the rest of the code

It constructs renderer-api image/text chunks, drives renderer-metal, and emits evidence consumed by the rendering architecture workflow.

Call flow: deterministic chunks -> retained snapshot -> Metal frame submission -> printed metrics.

## Entry points list

- `main()` creates the probe workload and reports observed results.

## Logic narrative

The image chunk uses the square-image contract with explicit zero radii. Dynamic transforms and opacity change instance metadata while immutable geometry remains retained.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

Metal must be available and GPU timing must complete. The image handle and atlas remain live for the probe. No new unsafe contract is introduced.

## Edge cases and failure modes

Renderer creation, resource creation, submission, or timing failure terminates the probe instead of emitting partial success.

## Concurrency and memory behavior

The process owns one renderer and completion-protected resources. Workload vectors are built before measurement.

## Performance notes

Explicit zero radii preserve the original square-image workload and make the new inline command field visible in source identity without changing measured pixels.

## Feature flags and cfgs

The binary is a macOS Metal evidence target.

## Testing and benchmarks

The ordinary architecture matrix and focused renderer tests cover its shared contracts; execute this binary only during an intentional C26 evidence run.

## Examples

Run the Cargo binary target from the workspace on a Metal-capable macOS host.

## Changelog

- 2026-07-18: made the square-image radius payload explicit after the renderer-api contract extension.
