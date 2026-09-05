# oxide-apple-comparison-controller correctness

## Intention and purpose

`correctness.rs` reduces paired Apple correctness checkpoints while preserving cryptographic provenance for state, accessibility, runtime geometry, and screenshots.

## Relation to the rest of the code

The controller supplies paired `oxide.evidence` and `native.evidence` roots. The reducer loads scenario bindings from the active plan, verifies every runtime artifact, and invokes `oxide-benchmark-spec` visual comparison using the checkpoint's observed geometry and canonical scale.

Screenshot-blocked release candidates use the distinct `capture_macos_release_candidates` control path. That path validates the Release build plus `plans/macos-release-candidate-capture.json`, launches no profiler or timing collector, requires both apps to terminate after emitting all 18 checkpoints, validates plan-bound completion receipts, and invokes atomic release promotion. It never admits a screenshot-free candidate through the normal runnable-scenario loader.

Call flow:

- atomic controller pair
  - verified side checkpoint artifacts
  - optional frozen checkpoint geometry
  - structural comparison
  - semantic-region visual reducer
  - schema-5 correctness report

## Entry points list

- `reduce_apple_correctness_evidence(...) -> anyhow::Result<AppleCorrectnessVisualReport>` reduces the Apple PR plan or selected pack.
- `reduce_generic_apple_correctness_evidence(...) -> anyhow::Result<AppleCorrectnessVisualReport>` reduces a content-addressed generic Apple campaign.
- `compare_apple_correctness_evidence(...) -> anyhow::Result<AppleCorrectnessVisualReport>` persists a reduced PR report.
- `AppleCorrectnessVisualCheckpoint` and `AppleCorrectnessVisualReport` expose the hash-bound result.

## Logic narrative

Each runtime identity must end in the exact evidence-root/scenario/checkpoint filename and match the observed bytes. Geometry is decoded through the canonical macOS profile validator. Structural acceptance requires exact state, accessibility, and AppKit/Oxide geometry bytes; promoted checkpoints additionally require exact equality with the frozen geometry artifact. Visual reduction receives those geometry bytes and their declared scale instead of a fixture layout and hardcoded scale. Schema 5 embeds the v4 semantic-region visual report: node bounds cannot become text exclusions, full-frame SSIM and local voxels evaluate every pixel, and only actual text-line bounds plus one physical pixel are omitted from the stable-interior differing-pixel count.

## Preconditions and postconditions

Inputs are complete side roots from the same controller pair and a content-addressed plan. An accepted row proves structural equality, valid observed geometry, and calibrated screenshot parity.

## Edge cases and failure modes

Missing geometry, path substitution, hash drift, invalid profile/scale/root, scenario identity drift, absent screenshots, and malformed JSON fail closed. A valid but unequal pair geometry sets structural acceptance false.

## Concurrency and memory behavior

Reduction is synchronous and lock-free. It owns checkpoint bytes while reducing one report.

## Performance notes

This path executes after untimed correctness capture and does not affect application frame measurements.

## Feature flags and cfgs

None.

## Testing and benchmarks

`tests/lib_tests.rs` builds complete side roots, proves accepted geometry metadata, and rejects an artifact path that escapes its checkpoint binding.

## Examples

Use `reduce_generic_apple_correctness_evidence` for promoted release plans because their checkpoints contain frozen runtime geometry identities.

## Changelog

- 2026-07-26: schema 5 binds the semantic-region v4 visual report and rejects the former broad text-node/rounded-edge mask admission.
- 2026-07-21: schema 3 now verifies runtime geometry and derives the visual scale from the canonical capture profile.
