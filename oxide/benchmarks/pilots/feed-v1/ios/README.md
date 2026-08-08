# feed-v1 iOS treatments

This directory contains the benchmark-only frozen `feed-v1` fixture, its two
UIKit treatments, the Oxide app crate, and the physical-device pilot. The
top-level Swift files are reusable treatment sources rather than an application
shell; `device-pilot` owns the app delegates, windows, controller, and reducer
orchestration. None of these files belong in a production host target.

## Frozen fixture

`FeedV1Contract.materialize()` returns a fixture that retains only the
deterministic recipe's 2,001-entry height prefix and the frozen canonical
identity. Before building either device app, the authoritative runner executes
independent Swift and Rust host checks that derive all 2,000 rows and reject any
output whose complete canonical SHA-256 is not:

```text
a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473
```

The canonical stream is exactly `717745` bytes. Integers are unsigned 32-bit
little-endian values, strings are a 32-bit little-endian byte length followed
by UTF-8, and checker payloads are raw RGBA8 bytes in declared variant order.
`canonicalBytesForAudit()` reproduces that stream after audit admission. Device
runtime construction does not derive offscreen strings or hash the canonical
stream; each treatment generates row content only when its collection requests
that row.

The hash covers every generated ID and string, all 2,000 integer row heights,
the complete 2,001-entry prefix table, the final content extent, every checker
RGBA byte, font references and hashes, colors, geometry, placement, and launch
transport names. The compact source recipe is the live fixture; neither UIKit
treatment retains a 2,000-row string table and no such dump is checked in.
`FeedV1Fixture.row(at:)` materializes one bounded row only when a data source
needs that visible cell, matching Oxide's recipe-owned, on-demand row content.

Each row has six deterministic manifest IDs: `/row`, `/image`, `/title`,
`/caption`, `/metadata`, and `/separator` beneath its stable row ID.
`componentRectPhysicalPixels(rowIndex:kind:)` supplies their exact content-space
physical-pixel bounds, and `viewportClipPhysicalPixels` freezes the shared
viewport-space clip rectangle. `visibleComponentRecords(contentOffsetPoints:)`
emits the controller schema shape
`{id,kind,row_index,content_rect_px:{x,y,width,height},viewport_clip_px:{x,y,width,height}}`
for a frozen or observed offset.

The host canvas is exactly `440 x 956 pt` at `3x`. XCTest supplies one full
`1320 x 2868` physical-pixel screen PNG for each smoke tuple. The reducer then
crops the `390 x 844 pt` feed surface at the frozen integer origin `(25, 56)`,
yielding `(75, 168) + 1170 x 2532` physical pixels. Already-cropped PNG input is
rejected. The feed surface has zero safe-area inset. Both app targets require
full-screen portrait presentation and hide the status bar. The home-indicator
region lies below the frozen crop, whose vertical extent is `y = 56 .. 900 pt`,
so it is neither benchmark content nor a reason to fork the production host.

Rows are contiguous, with no collection inset or inter-row spacing. Heights are
`92`, `110`, `128`, or `146 pt`; `prefix[0] = 0` and
`prefix[i + 1] = prefix[i] + row[i].height`. The resulting content extent is
`237460 pt`, and the bottom start offset is `236616 pt`. Cell geometry, the
one-physical-pixel separator, exact colors, Asap face names and file hashes,
hard-offset image shadow, explicit caption newlines, and the procedural `12 x
12` straight-RGBA checker bytes are defined in `FeedV1Contract.swift`.

The required font resources are the unmodified files at:

- `oxide/crates/ui-core/assets/Asap-Regular.ttf`
- `oxide/crates/ui-core/assets/Asap-Bold.ttf`

The benchmark target must copy those files into its resource bundle under the
same basenames. Runtime admission validates their byte counts and SHA-256
hashes before registering the faces with Core Text.

## Treatments

- `FeedV1IdiomaticUIKitView` uses `UICollectionViewFlowLayout`, ordinary
  `UILabel`/`UIImageView` composition, standard cell reuse, and UIKit-owned
  scroll/deceleration.
- `FeedV1OptimizedUIKitView` uses the same collection, subviews, resources,
  clipping, and scroll behavior, but precomputes all item geometry, common exact
  visible attribute ranges, and the four cell-frame shapes. It does not replace
  cells with a painted canvas.

Both paths disable collection prefetching so a fresh process warms only the
initially visible resources. Checker images are generated and cached lazily on
first naturally visible use. Their shared fixture retains only prefix geometry
and identity; row strings are generated as cells are requested. Neither path
logs or appends samples during frame callbacks.

`FeedV1RootFactory.make(variant:resourceBundle:)` creates exactly one root and
one feed surface. The controller chooses `.idiomatic` or `.optimized` once per
fresh process, assigns a `FeedV1UIKitObservationSink`, and calls `mount(at:)`
with `.top` or `.bottom`. The sink receives direct scroll-state and display-link
timestamp/target-timestamp callbacks; it must preallocate any sample storage.
Admission must reject a non-nil `hostContractErrorDescription`, a non-nil
surface `renderingErrorDescription`, or content extent/offset outside the
one-physical-pixel protocol tolerance.

Shared process inputs are `OXIDE_FEED_V1_TREATMENT`,
`OXIDE_FEED_V1_START_STATE`, and `OXIDE_FEED_V1_COMPLETION_NONCE`. The common
result transport is `Documents/oxide-feed-v1-<nonce>.json`. Readiness is Darwin
notification `com.oxide.feed-v1.ready.<nonce>`. Completion is emitted only after
the file is atomically closed, using Darwin notification
`com.oxide.feed-v1.complete.<nonce>`; failures use
`com.oxide.feed-v1.failed.<nonce>`. Persistence and notification emission stay
in the separate symmetric controller/runtime integration.

Completion nonces are exactly 1 through 128 ASCII alphanumeric-or-hyphen bytes.
The shared validator runs before deriving a filename or notification name;
launch-failure handling neither persists nor posts when the raw nonce is
invalid. Both treatment recorders retain at most 1,024 display-link callback
samples, which exceeds the six-second 120 Hz measurement bound without an
asymmetric buffer.

## Checks

Run the deterministic fixture check on the host:

```sh
swiftc -parse-as-library \
  -DFEED_V1_CONTRACT_CHECK_MAIN \
  FeedV1Contract.swift FeedV1ContractCheckMain.swift \
  -o /tmp/feed-v1-contract-check
/tmp/feed-v1-contract-check
```

Type-check the complete arm64 iOS source set without generating a project:

```sh
sdk_path=$(xcrun --sdk iphoneos --show-sdk-path)
xcrun swiftc -typecheck -parse-as-library \
  -target arm64-apple-ios18.0 -sdk "$sdk_path" *.swift
```

The real visual and interaction proof still requires the protocol's physical
iPhone build, exact full-screen captures, and XCTest OS-level forward/reverse
flings. Simulator output is not comparison evidence.
