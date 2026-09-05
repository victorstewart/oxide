# oxide-benchmark-spec release_promotion

## Intention and purpose

`release_promotion.rs` atomically converts the five blocked macOS release candidates into runnable, content-addressed scenario manifests only after real paired AppKit/Oxide correctness evidence passes structural and calibrated visual gates.

## Relation to the rest of the code

The input is one comparison-controller correctness pair directory containing `native.evidence` and `oxide.evidence`, produced by the separate hash-bound `macos-release-candidate-capture` plan. Every checkpoint is loaded through its runtime `evidence.json` identities. Accepted screenshots and runtime geometry are copied into the promoted specification tree, then qualification plans and 1x/2x workload overlays are generated from those manifests.

Call flow:

- comparison applications emit checkpoint evidence
  - controller pair directory retains both evidence roots
  - `promote_release_candidates`
  - promoted screenshot and geometry artifacts
  - runnable release manifests and qualification plan

## Entry points list

- `oxide_benchmark_spec::promote_release_candidates(spec_root: &Path, evidence_root: &Path, output_root: &Path, canonical_scale: u32) -> anyhow::Result<ReleasePromotionReport>` validates and atomically publishes all five candidates.
- `oxide_benchmark_spec::ReleasePromotionReport` records promoted scenario and qualification identities.
- `oxide_benchmark_spec::ReleasePromotionQualificationReport` binds the generated execution plan, qualification plan, and scale overlays.
- `oxide_benchmark_spec::RELEASE_CANDIDATE_IDS` freezes the all-or-nothing candidate set.

## Logic narrative

Promotion validates each candidate and referenced frozen input. For every checkpoint it verifies the runtime evidence paths and hashes, exact state/accessibility equality against both sides and the frozen contract, valid live AppKit view-tree and Oxide draw-list geometry, matching roots/profile/scale/tolerances, and calibrated full-frame visual acceptance. It then content-addresses AppKit's accepted PNG and a paired geometry envelope containing both source-specific runtime snapshots. All files are staged beside a copied specification tree and renamed into place only after every promoted manifest validates.

## Preconditions and postconditions

The evidence root must be a complete controller pair directory. The output must not exist and must be outside both input roots. Success produces all five manifests, their checkpoint screenshots and geometries, 26 scale overlays, and the qualification plans in one atomic tree.

## Edge cases and failure modes

Missing or path-unbound runtime artifacts, hash drift, noncanonical validation labels, structural mismatch, geometry/profile/scale drift, rejected visuals, incomplete candidates, unsafe paths, and an existing output fail before publication.

## Concurrency and memory behavior

Promotion is synchronous and single-threaded. It owns bounded checkpoint bytes and writes through a process-specific staging directory before one rename.

## Performance notes

This is offline Stage-1 qualification work. It performs no production or measured-frame work.

## Feature flags and cfgs

None.

## Testing and benchmarks

`tests/release_promotion_tests.rs` covers missing evidence, visual rejection, capture-profile rejection, and complete atomic promotion with content-addressed checkpoint geometry.

## Examples

Pass a completed correctness pair directory as `--evidence-root`; its immediate children must be `native.evidence` and `oxide.evidence`.

## Changelog

- 2026-07-21: replaced manually renamed evidence inputs with hash-bound runtime checkpoints and promoted paired source-specific runtime geometry.
