# feed-v1 run record schema

Both app bundles atomically persist one JSON record for every successful
fresh-process gesture at `Documents/oxide-feed-v1-<nonce>.json`. The file is
closed before the app posts `com.oxide.feed-v1.complete.<nonce>`. A failure
closes the strict failure record defined below before posting
`com.oxide.feed-v1.failed.<nonce>`.

All numeric coordinates are finite JSON numbers. Point values are logical iOS
points. Pixel rectangles are integer physical pixels with an upper-left origin
and half-open edges. `visible_components` contains every component whose
frozen content rectangle intersects the captured viewport, sorted by
`row_index` and then the frozen component-kind order.

```json
{
  "schema": "oxide.feed-v1.run",
  "schema_revision": 1,
  "fixture": {
    "schema": "oxide.feed-v1.fixture",
    "revision": 1,
    "canonical_sha256": "a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473",
    "canonical_byte_count": 717745
  },
  "run": {
    "nonce": "primary-s00-p00-o0-uikit-idiomatic-forward-<uuid>",
    "phase": "primary",
    "session_index": 0,
    "pair_index": 0,
    "order_index": 0,
    "treatment": "uikit-idiomatic",
    "start_state": "top",
    "direction": "forward"
  },
  "canvas": {
    "host_width_points": 440,
    "host_height_points": 956,
    "surface_origin_x_points": 25,
    "surface_origin_y_points": 56,
    "surface_width_points": 390,
    "surface_height_points": 844,
    "scale": 3
  },
  "geometry": {
    "row_count": 2000,
    "manifest_component_count": 12000,
    "content_extent_points": 237460.0,
    "maximum_content_offset_points": 236616.0,
    "captured_content_offset_points": 0.0,
    "viewport_clip_px": { "x": 0, "y": 0, "width": 1170, "height": 2532 },
    "visible_components": [
      {
        "id": "feed-v1-row-0000/row",
        "kind": "row",
        "row_index": 0,
        "content_rect_px": { "x": 0, "y": 0, "width": 1170, "height": 276 },
        "viewport_clip_px": { "x": 0, "y": 0, "width": 1170, "height": 276 }
      }
    ]
  },
  "environment": {
    "before": {
      "thermal_state": "nominal",
      "low_power_mode": false,
      "maximum_frames_per_second": 120,
      "configured_frame_rate": { "minimum": 120.0, "maximum": 120.0, "preferred": 120.0 }
    },
    "after": {
      "thermal_state": "nominal",
      "low_power_mode": false,
      "maximum_frames_per_second": 120,
      "configured_frame_rate": { "minimum": 120.0, "maximum": 120.0, "preferred": 120.0 }
    },
    "thermal_state_change_count": 0,
    "low_power_mode_change_count": 0
  },
  "gesture": {
    "start_offset_points": 0.0,
    "end_offset_points": 2000.0,
    "signed_travel_points": 2000.0,
    "travel_distance_points": 2000.0,
    "duration_seconds": 1.0,
    "inertia_observed": true,
    "settled": true
  },
  "display_link": {
    "clock": "CADisplayLink.timestamp/targetTimestamp",
    "callback_only": true,
    "samples": [
      { "timestamp_seconds": 100.0, "target_timestamp_seconds": 100.0083333333 },
      { "timestamp_seconds": 100.0083333333, "target_timestamp_seconds": 100.0166666667 }
    ]
  },
  "status": "complete",
  "failure": null
}
```

A failure cannot truthfully provide observations that were never reached. It
therefore uses a separate, non-measurable schema and never fills run fields with
sentinels:

```json
{
  "schema": "oxide.feed-v1.failure",
  "schema_revision": 1,
  "fixture": {
    "schema": "oxide.feed-v1.fixture",
    "revision": 1,
    "canonical_sha256": "a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473",
    "canonical_byte_count": 717745
  },
  "nonce": "primary-s00-p00-o0-uikit-idiomatic-forward-<uuid>",
  "treatment": "uikit-idiomatic",
  "stage": "launch|initial-admission|gesture|persistence",
  "message": "precise failure"
}
```

`nonce` and `treatment` are JSON `null` only when launch input parsing failed
before either value was available. The reducer admits no measurement whenever
it discovers a failure record.

A usable completion nonce is exactly 1 through 128 UTF-8 bytes and every byte
must be an ASCII digit, uppercase or lowercase letter, or hyphen. Both app
targets apply this same predicate before constructing a result path or Darwin
notification name; an invalid raw nonce authorizes neither persistence nor a
notification.

The controller supplies `OXIDE_FEED_V1_PHASE`,
`OXIDE_FEED_V1_SESSION_INDEX`, `OXIDE_FEED_V1_PAIR_INDEX`, and
`OXIDE_FEED_V1_ORDER_INDEX` in addition to the three frozen workload inputs.
Those four values describe sampling order only and cannot change app behavior.
`phase` is `smoke` or `primary`; the three indices are non-negative decimal
integers. A full run contains exactly one primary block: three sessions by
three pairs by three treatments by two directions, yielding nine paired
clusters and 54 primary timing runs. No continuation or second primary block is
admitted.

`gesture.duration_seconds` ends at the treatment's actual settled-state
transition. A following display-link callback is retained as closure evidence
but does not extend the gesture duration. The final app bundles must enable
native iPhone frame durations, and reducer admission requires at least 95% of
observed `target_timestamp_seconds - timestamp_seconds` periods to be within
`7.5 .. 9.2 ms`.
Both app targets retain at most 1,024 callback samples. Exceeding that common
capacity produces a failure record instead of silently truncating one
treatment's evidence.

`gesture.travel_distance_points` must be at least `524 pt`, the frozen direct
drag span less the `16 pt` travel tolerance. This prevents a delivered but
degenerate gesture from being admitted. It does not prove a fling:
`gesture.inertia_observed` is independently required and is true only after the
treatment reports actual entry into inertial motion. UIKit records
`scrollViewWillBeginDecelerating`; Oxide records the production collection
surface's inertial-state transition. Drag-only movement is rejected.

The six unreplicated smoke flings do not assert cross-treatment travel
equivalence. In the full population, each candidate primary run is paired with
idiomatic UIKit by session, pair index, and direction. Each treatment/direction
group must keep its nine-pair median relative travel delta within `5%` and its
exact two-sided median interval within `[-10%, +10%]`. The interval uses
one-based ranks 2 and 8 and reports `96.09375%` achieved binomial coverage for
the requested 95 percent level.

Environment transition observers are installed before readiness. Their
monotonic counts are snapshotted immediately before the initial environment
endpoint query that admits readiness, then atomically closed and differenced at
the settled run's finish boundary before record persistence and notification.
Both persisted counts must be zero, even when `before` and `after` happen to
contain the same thermal and power values.

For a bottom/reverse record, `captured_content_offset_points` and
`start_offset_points` are the exact frozen maximum. Content rectangles remain
in full-feed coordinates; each `viewport_clip_px` is their intersection with
the captured viewport translated so viewport `(0, 0)` is the crop origin.

The reducer rejects unknown fields in both schemas. It rejects missing fields,
non-finite numbers, duplicate nonces or component IDs, unsorted components, a
direction inconsistent with its start state, any fixture/canvas identity
mismatch, and any complete record with fewer than two display-link samples.
The six-run smoke command applies these gates plus screenshot, visual, device,
travel, environment, and cleanup admission, but writes no report and computes
no timing classification. Only the exact 60-record evidence population—six
smoke diagnostics plus the 54-run primary block—can produce `latest.json` and
`latest.md`; their timing rows come only from the 54 primary records.

## Smoke screenshot attachment

The controller attaches exactly one PNG named `feed-v1-<nonce>.png` for each of
the six smoke treatment/direction tuples. Primary tuples produce only their
app-owned run records. There is no controller capture JSON, second image, or
repeat gate. Because one full run uses the same immutable manifest-hashed app
bundles for its smoke and primary populations, the six smoke visuals authorize
or block the 54 primary timing rows.

Every smoke PNG must decode as the complete portrait `1320 x 2868` physical-
pixel XCUIScreen capture. The reducer rejects an already-cropped surface image,
then extracts physical pixels `(75, 168) + 1170 x 2532` and represents that crop
as RGBA8. PNG ancillary bytes and the black host surround cannot fail or rescue
a surface comparison.

Xcode 26 decorates each suggested name with a terminal `_0_<UUID>` during
attachment export. The reducer accepts only that exact UUID-shaped decoration
or the undecorated name, removes the decoration, and then requires the exported
XCTest manifest to contain exactly one detail for
`FeedV1ControllerTests/testFeedV1PhysicalDevicePilot()` and map the six
canonical nonce-derived PNG names to six distinct canonical files. Canonical
path aliases and, on the Unix device-runner host, hard-link file-identity aliases
are rejected along with fallback lookup, unrelated attachments, or an
unmanifested authority record. Both smoke and full modes therefore retain
exactly six attachments. Each successful app record must use the exact
`oxide-feed-v1-<nonce>.json` basename beneath the retrieved UIKit documents tree
for either UIKit treatment or the retrieved Oxide documents tree for Oxide.

## Evidence and cleanup records

Exactly one `oxide.feed-v1.evidence-manifest` revision 3 may appear, at
`raw/evidence-manifest.json`. It contains the fixture hash; the exact named Git
ref, clean `HEAD` commit, and `HEAD` tree; a strict embedded
`oxide.feed-v1.build-provenance` revision 1; a sorted source-path-to-SHA-256 map;
deterministic hashes of each signed app bundle, the compiled
`FeedV1Controller-Runner.app` and its executable, the embedded
`FeedV1Controller.xctest` and its executable, the exact standalone reducer
executable, and both font hashes. Build provenance freezes the exact CoreDevice
and hardware UDID, model, OS version/build, 120 Hz requirement, Xcode and iPhoneOS
SDK versions/builds, Rust compiler/host and Cargo version, resolved Release build
settings hash, Cargo lock and resolved production metadata hashes, and the
Authority/team/CDHash identity of both apps plus both controller products.
`project.yml` is the Xcode source definition;
the externally generated `FeedV1Pilot.xcodeproj` is neither checked in nor
admitted as source evidence. The report identifies both the frozen visual-gate
specification and the actual `reducer/src/lib.rs` source hash from this manifest.

Exactly one `oxide.feed-v1.cleanup` revision 3 may appear, at
`raw/cleanup.json`, and is written only after process/app cleanup and external
build-root removal. It contains `test_succeeded`,
`verified_attachment_count`, `apps_uninstalled`, `controller_uninstalled`,
`uikit_process_absent`, `oxide_process_absent`, `controller_process_absent`,
`prelaunch_fuses_admitted`, the exact structured `resource_limits`, `source_snapshot_preserved`,
`external_build_removed`, `result_bundle_removed`,
`reducer_binary_absent_from_result_root`, `external_build_bytes`,
`result_bundle_bytes`, `raw_evidence_bytes`, `retained_file_count`,
`largest_retained_file_bytes`, and
`runtime_seconds`.
`raw_evidence_bytes` is the allocated size of `raw/` immediately before the
cleanup proof and reports are written; it is not presented as final-package
size. Publication requires Xcode success, the exact six-PNG smoke population,
and every cleanup boolean to be true. The frozen caps are 4 GiB for the external
build root, 512 MiB for the result bundle and retained evidence, 512 retained
files, and 128 MiB for any retained file. The reducer independently proves that no
reducer executable exists inside the result root and rejects an actual retained
`.xcresult` or `tools` directory. It also totals every retained input file
instead of trusting the claimed raw size. After reduction, the runner removes
the external temporary reducer and exits unsuccessfully if that final cleanup
fails; the report does not claim that a post-reduction action has already
happened. The runner separately measures the completed package after both
reports are written. More than 512 MiB at either boundary, more than 20 minutes
of device-test runtime, a missing record, or an unknown field blocks
publication.

The controller atomically writes exactly one
`oxide.feed-v1.controller-runtime` revision 1 record named
`oxide-feed-v1-controller-runtime.json` in its app container. The runner
retrieves it beneath `raw/controller-documents`. It contains `mode`,
`total_runtime_seconds`, and separate
`uikit_idiomatic_runtime_seconds`, `uikit_optimized_runtime_seconds`, and
`oxide_runtime_seconds`. Full mode resets the treatment accumulators after the
smoke prefix, so the three treatment limits describe the primary population.
The reducer independently requires total runtime at most 20 minutes and every
treatment at most 10 minutes; Xcode test success remains separately mandatory.
After export, the runner also rechecks that the named Git ref, commit, tree, and
clean worktree still match the pre-build snapshot and persists that result in
the cleanup proof.

The report requires matching `device-before.json` and `device-after.json`
records for one booted physical arm64 iPhone plus unlocked lock-state records
before build and immediately before the test. It emits only marketing model,
product type, OS version/build, and CPU; device identifiers remain raw evidence
and are not copied into the publication summary.

A complete revision-4 full report contains a deterministic
`travel_equivalence` array and matching compact Markdown table for the four
treatment/direction comparisons. Each result persists its pair count, median
relative delta, exact interval method, requested and achieved coverage,
one-based rank bounds, numeric bounds, both frozen margins, and pass/fail
decision. The two pacing comparisons persist the same interval metadata.
Historical revision-3 bootstrap reports remain historical evidence and are not
rewritten as revision 4. The revision-4 deterministic `runs` array and matching
dense Markdown table contain the 54 primary gestures. Each row preserves the
run identity, duration, inertia/travel evidence, both environment-transition
counts, callback count/cadence,
p50/p95/p99/peak, missed/expected counts and ratio, hitch excess, and
target-period admission ratio. Blocked and smoke evaluations expose no timing
rows. Exact report output paths are excluded from discovery so a second full
reduction is byte-identical to the first.
