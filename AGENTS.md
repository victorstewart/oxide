# AGENTS.md — Repo rules (Rust)

## Activation
This is a Rust workspace. Load exactly this profile:
- Read `~/.codex/rules/rust/GPT5_RULES.md` and follow it verbatim.
- Do not load any other language profile.

## Local overrides (optional, narrow)
- Use only when a task explicitly opts into a subfolder-specific behavior.
- Document any local override at the top of the PR description.

## Formatting
- **NEVER run `cargo fmt` or `cargo clippy` in this repository.** Our manual style cannot be expressed via rustfmt; automated formatting will churn every file.
- Mirror the canonical snippet below for indentation (3 spaces), brace placement (Allman), and import grouping/wrapping.

```rust
use databento::dbn;
use databento::{HistoricalClient, Symbols};
use dbn::{Dataset, HasRType, SType, Schema};
use std::sync::Arc;
use thingbuf::ThingBuf;
use time::{Duration, OffsetDateTime, Time};

use super::frontier::{frontier_merge, spawn_batched, Ring, WorkerEvt};
use super::handlers::{batches, ContinuousRolled, CorpRow, HasTimestamp, ProcessMessage};
use super::helpers::{ring_pair, symbols_len};
use super::limits::{DEF_HIST_CONN_SEM, HIST_CONN_SEM, HIST_WM_GROUP_SIZE, LIVE_CONN_SEM, LIVE_WM_GROUP_SIZE, PayloadBatchSpec, WORKER_RING_CAP};
use super::workers::{hist_worker, live_worker};
use crate::types::Span;

pub type Sender<P> = Arc<ThingBuf<P>>;
pub type Receiver<P> = Arc<ThingBuf<P>>;

#[allow(dead_code)]
pub trait RxExt
{
   fn is_finished(&self) -> bool;
}

#[allow(dead_code)]
impl<T> RxExt for Receiver<T>
{
   #[inline]
   fn is_finished(&self) -> bool
   {
      Arc::strong_count(self) == 1 && self.is_empty()
   }
}

#[derive(Clone)]
pub struct Databento
{
   pub key: String,
   pub historical: HistoricalClient,
}
```

- Functions and impl blocks use one-line signatures, braces on their own lines, and 3-space indentation inside the block.
- Imports are grouped and wrapped exactly as shown (std → external crates → crate-local).
- Manual edits only—review diffs carefully to preserve legibility.

## Tooling
- Manual formatting fixes must maintain the style canon above; no automated formatter may be used.
- Deprecation warnings are blockers: whenever a deprecated API is observed in touched code or command output, replace it with the upstream-recommended supported API in the same change.

## Performance Requirements
- Every new Oxide feature, component, animation, or renderer-facing hot path must land with a corresponding Rust perf case in `oxide/crates/perf-runner` and refreshed persisted results in `oxide/benchmarks/workspace/latest.json` plus `oxide/benchmarks/workspace/latest.md`.
- Every new user-facing scene, workflow, or interaction path must also land with either a scene-level perf case or an explicit user-journey perf case in `oxide/crates/perf-runner`. If the change is interactive or visible to the user, prefer a journey case that exercises the actual use path instead of only a low-level encode path.
- Every new public Oxide author-facing API surface, state controller, composition primitive, or other code path that an app author writes against must land with a corresponding `authoring` perf case in `oxide/crates/perf-runner`, and the committed workspace baseline must be refreshed if that report shape changes.
- The performance contract is layered, not ad hoc: every important area must be represented as either an engine microbenchmark, a representative screen flow, or an OS-bridge benchmark.
- Keep the contract Oxide/UIKit-native. Do not mention legacy or reference apps in the persisted perf policy, benchmark contract, or committed reports.
- Treat the battery as a contract, not a grab bag. Across Oxide and UIKit, the suite must grow toward these workload groups: launch/lifecycle, primitive creation/update/destroy, layout/invalidation, text/input, image pipeline, list/grid/chat, navigation/input latency, animation/effects, state mutation/reconciliation, OS bridge overhead, endurance/thermal drift, and stress/pathological regressions.
- Every new Oxide UI element, animation, scene flow, or bridge benchmark must also land with apples-to-apples UIKit parity coverage in `oxide/host/ios-app/App/OxideHostPerfTests`.
- The default committed UIKit device baseline is a compact representative signal battery, not the exhaustive case matrix. Dense near-duplicate count/style permutations should be tiered into explicit touched-case or full-contract runs, while the default battery keeps the highest-signal representative rows for each important family.
- UIKit parity must maintain two baselines for the same workload family: an idiomatic UIKit implementation and a hand-optimized UIKit implementation. The first answers “what does normal UIKit look like?” and the second answers “is Oxide still hard to beat after UIKit is tuned?”
- Apples-to-apples parity is mandatory: the Oxide and UIKit cases must share the same scene spec for strings, fonts, colors, corner radii, shadows, image bytes, animation curves, durations, scroll physics, geometry, and visible effects. A faster run is invalid if it silently draws less or lowers quality.
- The measured phase vocabulary must stay symmetric across Oxide and UIKit. Use the same phase names whenever applicable: `app.launch`, `screen.mount`, `layout`, `text.measure`, `diff.apply`, `image.decode`, `texture.upload`, `draw.encode`, `frame.present`, `first.interactive`, `transition`, `scroll`, and `native.bridge`.
- Report latency distributions, not just means. Persist p50, p95, p99, and peak figures for every benchmark variant, and prefer hitch ratio, first frame, first interactive, and event-to-response latency over headline FPS.
- On user-visible scroll and animation flows, hitch ratio, missed frames, first frame, first interactive, and input-event-to-visible-response are the headline metrics. Average FPS is never enough on its own.
- Every persisted report row should move toward the same contract surface: test id, device, refresh mode, variant/style, cache state, latency distribution, hitch/missed-frame metrics when relevant, launch/first-frame/first-interactive metrics when relevant, CPU/main-thread cost, memory, logical writes, and any available direct GPU or energy metrics.
- Oxide-owned reports must also persist the internal counters that explain wins when available: dirty-node count, layout passes, draw-call count, encoded bytes, texture bytes, and similar renderer/runtime counters. These are explanatory diagnostics, not substitutes for cross-framework latency numbers.
- Keep cold, warm, and hot variants separate. Do not mix cache states in one benchmark number.
- Separate app-owned UI from system-owned UI. Keyboard, picker sheets, map/web/video surfaces, and permission alerts count as bridge overhead, not renderer wins.
- Camera preview architecture is a hard contract. Oxide may use iOS only for the lowest practical frame-acquisition hook and the unavoidable presentable surface shell. After frames are acquired, Oxide must own all visible preview rendering, composition, pacing, and presentation logic itself. `AVCaptureVideoPreviewLayer` or any other system-managed visible-preview transport is allowed only as a benchmark-only diagnostic experiment to isolate costs; it is never an acceptable product path or release candidate.
- Official camera-preview baselines must keep the buckets separate. The default committed UIKit device battery should include the parked microscope pure-custom NV12 live preview case and the matching `AVCaptureVideoPreviewLayer` baseline on the same unchanged build/device. The shipping-oriented actual app-host comparison remains a separate bucket and should be reported as partial or blocked until the UI-test runner path is stable.
- Hybrid camera visible-preview-layer cases are diagnostic-only. They may be run explicitly by `--case` for investigation, but they must never be included in the default committed UIKit device battery or in user-facing Oxide/UIKit summary tables.
- For camera preview performance work, do not jump from a coarse gap straight to another transport or renderer rewrite. First instrument the real pure-custom path at fine-grained code-block resolution, rerun on the physical iPhone, and use that attribution to choose the next optimization. The required attribution should isolate at least: low-level sample delivery / frame acquisition, texture bridge / publication, renderer-side frame fetch, command-buffer creation, encoder creation, bind, draw, end-encoding, present, commit, submission polling, and app-host tick overhead. Function-level flamegraphs alone are not sufficient when a finer app-owned block breakdown is feasible.
- The persisted reports must be explicit about contract status. If a workload family, cache-state variant, style baseline, or device class is still missing, mark it as partial or missing instead of implying comprehensive coverage.
- CI must run the Oxide workspace perf comparison on every PR and merge to `main`. Official Oxide/UIKit comparison numbers are device-only: rerun the touched Oxide device suite on the attached physical iPhone and intentionally refresh `oxide/benchmarks/oxide-device/latest.json` plus `oxide/benchmarks/oxide-device/latest.md`, and rerun the touched UIKit cases on the same phone and intentionally refresh `oxide/benchmarks/uikit-device/latest.json` plus `oxide/benchmarks/uikit-device/latest.md` in the reviewed change before merge. Any accepted regression requires an explicit baseline update in the reviewed change.
- Simulator UIKit perf runs are debug-only and must never be used as committed baselines, official comparisons, or user-facing summary numbers.
- When presenting Oxide vs UIKit summary tables, use only `oxide/benchmarks/oxide-device/*` and `oxide/benchmarks/uikit-device/*`. Do not mix desktop `workspace` numbers with physical-device UIKit numbers in an official comparison.
- Every UIKit parity case must remain runnable on physical iPhone hardware with process-scoped Metal System Trace attached only to the launched OxideHost app process. The committed `oxide/benchmarks/uikit-device/latest.json` plus `oxide/benchmarks/uikit-device/latest.md` files are the compact representative device baseline; touched cases outside that default set must still be rerun explicitly and recorded in the reviewed change when they are part of the change under review. Manual per-case Power Profiler `.trace` or raw exported `.atrc` imports should be added later when direct energy is collected for that same workload.
- The device UIKit report must always include direct GPU time and direct GPU counters when exposed by the device/toolchain. Direct energy measurements from the current Apple-supported Power Profiler workflow remain required once the corresponding manual per-case traces have been collected; until then, mark energy as manual-pending and do not substitute a proxy. Do not rely on any legacy or unsupported all-process `xctrace` capture path for either GPU or power.
- Any UIKit perf regression review is incomplete unless the device-authoritative GPU report has been rerun for the touched cases and the on-disk device baseline was intentionally updated in the reviewed change. When manual device energy traces are available for those same cases, rerun and persist the device energy metrics in the same review.
- Representative scroll and animation flows must be evaluated on real ProMotion hardware at native refresh in the official device harness. If UIKit can use `UIScrollView` hitch metrics directly but Oxide cannot, compute the symmetric hitch ratio from frame deadlines instead of comparing unlike metrics.
- Native-only is the official device contract for Oxide/UIKit parity. Any separate 60 Hz study is opt-in diagnostic work, not part of the default committed battery, and must not slow the official baseline path.
- When Android UI parity is added, extend the same policy: matching Android view-system perf tests plus persisted baseline reports are required alongside the Oxide and UIKit cases.
