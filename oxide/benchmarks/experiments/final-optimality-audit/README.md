# Final C00-C62 rendering optimality audit

Status: accepted final audit. Runtime and report inputs are unchanged from C62; this record adds the required skeptical current-tree disposition for every numbered work item.

## Method and decision rule

Starting SHA: `219f08e50ff3f8ded046263da3f9a36f83826066`.

Audited C62 tree: `a89fc5b74443bba2aef3db7c6a7b34d46561c7bd`.

Each row was reviewed against its implementation, paired proof, rejected alternatives, and final downstream ownership. A row is retained only when the current tree still has one necessary owner, bounded lifetime/storage, no avoidable steady-state work, and the simplest design consistent with exact output and the measured winner. `Accepted` means no material corrective implementation is justified. `Rejected` means the experiment remains absent from production. The complete original A/B distributions, confidence intervals, tails, counters, memory, visual proof, and alternatives are indexed in [C62](../c62-end-to-end-proof/README.md).

## Ordered current-tree dispositions

| Item | Commit(s) | Final disposition | Current-tree optimality result |
| --- | --- | --- | --- |
| C00 | `398e8745` | Accepted | The paired runner remains the common statistical foundation; no duplicate timing authority was introduced. |
| C01 | `5d82c68c` | Accepted | The architecture workload matrix remains representative and is extended, not shadowed, by later cases. |
| C02 | `5b287545` | Accepted | Accounting is explicit, saturating, bounded, and sampled off the hot path where collection cost matters. |
| C03 | `0578ade6` | Accepted | CPU/Metal/Web parity goldens remain the exactness oracle; no tolerance was weakened. |
| C04 | `787ae4e3` | Accepted | Per-pass ID-mask uniform slices remain required despite later field caching and packing. |
| C05 | `3d17fbcc` | Accepted | Single layer-body ownership remains the invariant generalized by C29; later caching does not duplicate it. |
| C06 | `a37bba29` | Accepted | Immutable image argument ranges remain completion-safe and necessary under the later image store. |
| C07 | `8cf5d1d9` | Accepted | The explicit 72-byte neon ABI remains compact, legal, and shared by direct and ring paths. |
| C08 | `79809012` | Accepted | Auxiliary encoders still use the selected frame slot; no derived-slot fallback survives. |
| C09 | `0216337f` | Accepted | Neon instances stream once through the completion-safe ring with bounded growth. |
| C10 | `7e00b142` | Accepted | Target validity remains necessary for partial loads and composes correctly with C51 direct presentation. |
| C11 | `bb29bb35` | Accepted | Native coalescing reuses caller storage with zero warm allocation; no in-place variant proved better. |
| C12 | `caea1348` | Accepted | Damage storage retains capacity with exact contents and no warm allocation. |
| C13 | `d1bf626f` | Accepted | Semantically invisible commands are deleted at the earliest safe UI boundary. |
| C14 | `60460cb9` | Accepted | Cropped images use one bounded image draw and avoid redundant nine-slice geometry and overdraw. |
| C15 | `6d434f85` | Accepted | The Web glyph atlas remains R8 end to end with correct row repacking and fourfold byte reduction. |
| C16 | `fddcae1d` | Accepted | Generic Web geometry uses the compact direct upload representation without a duplicate expansion path. |
| C17 | `466fe5cd` | Accepted | Effect targets are created only from the declared plan; C50 now owns reuse without restoring eager targets. |
| C18 | `160c78a8` | Accepted | Visible/offscreen ring depths match measured in-flight needs and remain completion protected. |
| C19 | `d681c6c9` | Accepted | Web targets remain lazy by declared use with explicit prewarm, bounded teardown, and no hidden eager set. |
| C20 | `57a702c5` | Accepted | Browser invalidations converge on one RAF authority with zero idle callbacks. |
| C21 | `cb783e77` | Accepted | Immutable versioned chunks remain the minimal retained unit and canonical exact flat fallback input. |
| C22 | `0094fba8` | Accepted | Node-local chunks and persistent sequences rebuild only dirty ownership; C23 bounds their cost. |
| C23 | `88104a51` | Accepted | Retained caching has hard byte budgets, LRU ownership, churn resistance, and exact fallback. |
| C24 | `a24b541f` | Accepted | Metal prepared chunks persist backend work and fall back only when snapshot semantics cannot be represented exactly. |
| C25 | `b633c4df` | Accepted | Web prepared chunks and aggregate bundles avoid replay peaks while preserving exact order. |
| C26 | `262ed9c7` | Accepted | Dense generation-checked property slots isolate animation mutation from immutable geometry. |
| C27 | `e7599fa3` | Accepted | Spatial metadata is prepared once and queried without revisiting unrelated geometry. |
| C28 | `38812b21` | Accepted | Exact retained damage propagates through scheduling and backend queries, including bounded full fallback. |
| C29 | `2ef28637` | Accepted | Metal prepared layers have one generation-correct owner, exact invalidation, and clean-hit body elimination. |
| C30 | `efa7f0ee` | Accepted | Web layers use local coordinates and local-sized textures; no full-surface residency path survives. |
| C31 | `332c0d6e` | Accepted | Layer caches share hard budgets, compatible pools, aging, eviction, purge, and exact uncached fallback. |
| C32 | `39cfa6d0` | Accepted | Metal ID-mask fields are cached by complete content identity with bounded target ownership. |
| C33 | `55b8287c` | Accepted | The two-entry Web ID-mask LRU is the measured minimum that handles alternating maps. |
| C34 | `8873c6e8` | Accepted | Metal jump fields retain the compact exact packed representation and wide fallback. |
| C35 | `80ff180b` | Accepted | Web jump fields retain the compact exact packed representation and compatible compositor contract. |
| C36 | `13c08463` | Accepted | Metal ID-mask work uses bounded completion-safe target sets instead of per-frame/eight-set churn. |
| C37 | `94856958` | Accepted | Web RRects use compact analytic instances; the result correctly claims CPU/upload, not universal GPU, wins. |
| C38 | `1d8c9e48` | Accepted | Web images stream compact instances without changing draw/bind ordering or image semantics. |
| C39 | `133e9bbf` | Accepted | Nine-slices instance dense runs; the singleton tradeoff remains disclosed and does not justify split machinery. |
| C40 | `b7732e3` | Accepted | Spinners are one compact procedural instance each with bounded DPR3 visual error. |
| C41 | `5f03fa33` | Accepted | Neon markers use semantically correct instances; the bounded GPU-tail cost remains preferable to expanded geometry. |
| C42 | `ec8c4dec` | Accepted | Metal analytic families share persistent rings and order-safe runs with zero warm growth. |
| C43 | `2c6090dc` | Accepted | Text is prepared and published once per frame; the small warm CPU tradeoff avoids repeated atlas work. |
| C44 | `1b7e5ea7` | Accepted | Page-local atlas generations invalidate only dependent text chunks and remain bounded. |
| C45 | `a011c3d6` | Accepted | Metal glyphs use compact instances; the disclosed synthetic GPU tradeoff is outweighed by total-frame and CPU wins. |
| C46 | `a3625b40` | Accepted | Web glyphs use compact ordered instances in both immediate and prepared paths. |
| C47 | `45871f47` | Accepted | Exact EDT and bounded fallback caches remove warm allocation; brute force remains test-oracle only. |
| C48 | `f4b631ef` | Accepted | Bitmap text is consolidated into the shared atlas/GlyphRun owner; the duplicate renderer is deleted. |
| C49 | `a01f24d2` | Accepted | Backdrop copies are clipped to exact sampled regions and safely coalesced by epoch. |
| C50 | `ad56907c` | Accepted | One reusable effect graph owns capture, pyramids, alias lifetimes, and bounded Web plan caching. |
| C51 | `def6d456` | Accepted | Metal renders directly to the drawable only when legal; exact offscreen persistence remains for partial/effect paths. |
| C52 | `8a35c073` | Accepted | Wide Gaussian taps are paired under an explicit measured quality threshold with exact narrow fallback. |
| C53 | `5576b3ca` | Rejected | Adaptive Web retained damage still loses on the measured workload; no production path or toggle survives. |
| C54 | `8f1135f` | Accepted | iOS display ticks suspend at true idle and wake from Rust-owned demand without missed work. |
| C55 | `bf0ffb0` | Accepted | macOS display polling likewise suspends at idle with one demand-driven scheduling owner. |
| C56 | `25e5cfd` | Accepted | Web Scene3D compacts and instances only compatible order-safe runs with generation-checked resources. |
| C57 | `ddf6b0a` | Accepted | Metal Scene3D uses compact rings, correct physical viewports, and order-safe instancing. |
| C58 | `0f7f139` | Accepted | Bloom is integrated into the shared render graph with two physical intermediates and preserved overlays. |
| C59 | `07090dc3` | Accepted | Shared immutable images plus full mips win; the measured slower/higher-memory staged Private policy remains absent. |
| C60 | `abb7f533`, `caabe92` | Accepted | Generation-checked async decode and bounded cross-backend stores page eligible static images and preserve standalone fallbacks. |
| C61 | `7a237c66`, `c14823d`, `d169b21` | Accepted | Current-tree browser/device proof, resumable staged promotion, and frozen report contracts remain the single publication input. |
| C62 | `d14e66b1` | Accepted | Report promotion and aggregate proof remain documentation-only and exactly identify the measured tree. |

## Skeptical findings

- No material corrective runtime change is justified. Later work composes with or generalizes earlier invariants instead of leaving duplicate hot paths.
- The flat encoders that remain beside prepared paths are required exact fallbacks for unsupported snapshot semantics, malformed input containment, device loss, or bounded-cache misses. Deleting them would weaken correctness.
- All reviewed runtime caches have hard byte/count bounds, generation or revision invalidation, explicit purge/device-loss handling where applicable, and an exact uncached path.
- C53 remains a genuine rejected experiment: its implementation is not compiled into the production path and there is no obsolete runtime toggle to remove.
- Native test builds warn that `saturating_texture_bytes` and `GenerationSlots::values` are unused. Both are used by the wasm WebGPU production implementation, so deleting or test-gating them would be incorrect churn.
- Because the audit changes neither runtime nor report inputs, rerunning or repromoting C62 would replace reviewed measurements without testing a changed candidate and is intentionally not done.

## Verification

- Combined locked eight-package all-target gate: passed, including 114 `oxide-perf-runner` report tests and 135 `xtask` tests.
- Focused locked `oxide-renderer-api`, `oxide-ui-core`, `oxide-text`, `oxide-input`, and `oxide-renderer-web` suites: passed.
- Locked Metal `snapshot-tests`: passed, including 32 exact pixel snapshots.
- Existing physical-iPhone proof was re-audited and remains applicable because neither runtime nor report inputs changed: C61/C62 passed 38 UIKit and 23 Oxide cases from one signed build on `iPhone18,2` at native refresh, with 75 process-scoped Metal trace bundles. The user explicitly stopped a duplicate final-audit rerun after its unchanged build installed and UIKit watchable metrics completed; `/tmp/oxide-final-audit/device` is diagnostic-only and was not promoted.
- `cargo run --locked -p xtask -- experiments check`: 191 decided entries, 89 accepted, 102 rejected, zero undecided.
- Promoted workspace, WebGPU, Oxide-device, and UIKit-device report hashes exactly match the C62 publication.
- `git diff --check` passed before the audit commit.

## Decision

Accept the current implementation of C00-C52 and C54-C62 as the simplest measured design under the final architecture. Keep C53 rejected and absent from production. No corrective runtime commit or C62 repromotion is warranted because the audit found no material improvement and changed no runtime or report input.
