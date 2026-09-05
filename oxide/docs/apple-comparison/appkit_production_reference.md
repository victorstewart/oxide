# AppKit production reference foundation

`host/apple-comparison/AppKit-macOS/AppKitProductionReferenceFoundation.swift`, `AppKitProductionVisualComponents.swift`, `AppKitProductionReferenceScenes.swift`, and `AppKitProductionReleaseScenes.swift` define the isolated full-stack AppKit implementation selected by the macOS comparison target. They are benchmark-only application sources and do not enter an Oxide production crate or shipping host.

## Intention and ownership

The production reference must exercise the framework mechanisms an experienced AppKit application would normally own. Each frozen PR/endurance scenario therefore has a distinct scene root and real component hierarchy:

- startup and dashboard own `NSCollectionView` collections with registered reusable `NSCollectionViewItem` classes;
- feed, chat, and navigation own view-based `NSTableView` lists inside explicit `NSClipView` and `NSScrollView` chains;
- chat owns an editable `NSTextView`, selection, committed text insertion, and a real send control;
- image owns an `NSImageView` document inside a scroll view plus a native zoom control.

Controls that repeat with content live inside their reusable item or row class. The scene root does not install hidden or decorative controls to satisfy an audit. `AppKitProductionSceneHost` owns exactly one installed hierarchy and removes the previous root on replacement.

Measured trace dispatch distinguishes application/resource updates from user input. Navigation list, modal, dismiss, and back actions use `NSApplication.sendAction` against the real control target/action pair. The navigation root owns a horizontal `NSPanGestureRecognizer`: a leftward drag that begins on the visible list advances the frozen modal progress through its native gesture callback, and mouse-up restores the list. This is the AppKit endpoint for the public-XCUI interactive-cancel override, where XCUI's duration-preserving mouse click-drag-release represents the trace's unavailable pointer-cancel primitive. The recognizer is attached to the existing root instead of a hidden hit-test overlay, so it does not alter static hierarchy, geometry, or raster output. Composer commits make the owned `NSTextView` first responder and enter through its `NSTextInputClient.insertText` path. The framework callback preserves the trace event's deterministic state boundary without calling the scenario model directly from the measured adapter entry point. A user-input trace without a faithful native route fails before producing a sample; it never falls back to model mutation.

The dashboard is the first scene mapped to the frozen visual contract. Its model owns 32 card records rather than one record per label. A native flow layout places `173x40` reusable items in two columns; each item paints the 38-point rounded surface and two-point shadow inside its own view while retaining two real `NSImageView` leaves, five or six real `NSTextField` leaves, and a real `NSButton` for each of the first 24 controls. Four separately owned AppKit views render the frozen backdrop regions. The target uses the verified seven-point font bytes, one-third-point text placement, low-filter atlas sampling, and exact palette values without a full-scene drawing canvas.

The nightly `idle.steady` workload reuses that complete dashboard fixture, native hierarchy, semantic geometry, and presentation path under a distinct scenario identity. It rejects logical events and measures sixty seconds of the unchanged production reference instead of substituting a reduced idle-only scene.

The nightly `endurance.churn` workload has a distinct AppKit root/model identity while reusing the same production dashboard card item and collection controller. Closing clears the collection and hides all heavy-screen views; reopening reloads all 32 cards, tab switches replace all 176 label values, and animation updates the same deterministic accent cycle as Oxide. The 100th reopen, 500th switch, and 600th frame restore the dashboard-derived checkpoint presentation.

The five release candidates now have distinct model and native scene-root adapters. Grid uses a native reusable `NSCollectionView` and an AppKit detail route; effects owns 100 clipped layer-backed AppKit cards; mutation owns a native view with 10,000 named retained sublayers; multilingual text uses a virtualized `NSTableView`; resize/theme reuses the dashboard collection components while applying the frozen portrait/landscape geometry. Correctness capture reads each active root's post-quiescence bounds and emits canonical profile geometry alongside its PNG. Their scenario manifests remain intentionally non-loadable until those real paired geometry/PNG artifacts are promoted and hash-bound.

The feed maps its frozen `390x844` viewport into a direct 52-point surface header and a 792-point native scroll region. Its `NSTableView` remains the virtualization and reuse owner. Each reusable `NSTableCellView` paints only its local rounded card and zero-blur shadow while retaining a rounded atlas-backed `NSImageView`, two CoreText-backed `NSTextField` leaves, and a borderless circular `NSButton`. Row records bind the pinned language-specific 15-point font, Latin 11-point identifier font, thumbnail identity, height, and favorite state. Inline symbol and emoji pieces are predecoded from the pinned atlas, retained by the text field, and drawn at their declared baseline without runtime font fallback. Initial installation and explicit reset tile the table once so variable-height geometry is deterministic. Measured favorite changes rebind one visible row through an O(1) identity index, pointer movement changes only the clip origin, and the fixed 20-row prepend uses `insertRows` without a full reload or full-record comparison pass.

Dedicated `NSCollectionViewDataSource`/`NSCollectionViewDelegate` and `NSTableViewDataSource`/`NSTableViewDelegate` owners bind stable record identities into those reusable components. Same-count dashboard mutations compare card records and rebind changed visible items in place without a collection reload; changed offscreen records receive current data when AppKit later materializes them. Chat prepend, append, and selected-message replacement use native incremental row insertion or one-row rebinding while preserving the visible prepend anchor. Navigation installs its immutable twelve-row dataset once. Repeated controls dispatch through real target-action routes owned by the corresponding scene controller. Feed favorite, navigation, composer input, chat send, startup, and image zoom actions update their retained models before presenting the affected native component. Actual mouse tracking on the `image.zoom` `NSSlider` maps its full track deterministically from 1x to the fixture's 2x maximum, commits the model scale, updates the retained image presentation, and advances measured generation through the existing target/action.

The image decode event performs immediate ImageIO decoding with `kCGImageSourceShouldCacheImmediately`. The subsequent upload event performs the frozen 2x downsample, creates a retained `2048x1536` sRGB `MTLTexture`, and copies all 12 MiB of pixel data into it before the event completes. The first-visible event only promotes the already prepared `CGImage` pair into the image canvas; it cannot absorb decode, downsample, or texture-allocation work. Focused counters distinguish upload preparation from any fallback presentation preparation and reject phase drift.

AppKit owns measured rendering cadence. Trace handling marks affected views for layout or display, then returns to the normal run loop; neither the trace entry point nor the display-link callback invokes `layoutSubtreeIfNeeded` or `displayIfNeeded`. Explicit synchronous layout/display remains confined to preparation, reset, quiescence, and correctness capture, which campaign orchestration places outside measured intervals. Native action closures are installed once during scene preparation rather than rebuilt after each logical update.

## Reuse and cleanup contract

`AppKitReusableCollectionViewItem` and `AppKitReusableTableCellView` record binding, preparation, and cleanup counts. Reuse clears represented objects, targets, actions, delegates, text, image content, selection, slider state, and layer animations. This contract is allocation-free while traversing the already-owned view hierarchy and prevents stale behavior or retained scene content from contaminating later samples.

Scenario teardown clears the window responder, detaches the scene hierarchy, releases the model and canonical capture surface, and then performs at least six complete default-mode main-run-loop turns outside measured work. Heap attribution proved that AppKit can release the app-owned scene immediately while deferring KVO, Auto Layout, Core Animation, and reusable-view cleanup until later turns. The minimum drain therefore cannot be conditional on a weak leak sentinel. Weak references to the app-owned model, scene, and capture surface remain the fail-closed ownership check; framework-owned reusable-view caches are instead bounded by the physical-footprint qualifier.

The foundation exposes native accessibility evidence with the actual `NSView` object identity, concrete AppKit class, hierarchy depth, identifier, role, label, and element state. Correctness geometry separately walks the installed live view hierarchy and records root-relative bounds for every positive-area view, classifying text views explicitly and retaining accessibility identifiers where present. This evidence is distinct from the normalized cross-framework accessibility fingerprint.

## Admission state

This implementation is not yet the accepted reference. The production target physically excludes the render-only diagnostic and selects the native hierarchy. Navigation target/action, interactive cancellation, composer text input, and image zoom now have native AppKit mutation callbacks. Feed fling/favorite, image pan/pinch, and release-candidate grid interactions still require complete trusted-input routes before their measured traces can be admitted. The navigation cancel path also remains end-to-end pending until the separately owned public-XCUI compiler/controller override is qualified against this native endpoint. Feed and dashboard carry their frozen component geometry, but both remain subject to exact screenshot evidence; startup, chat, navigation, and image still require exact component mapping. Exact checkpoint acceptance, native OS-input evidence, incremental update coverage for every scene, scaling/profile evidence, and the independent audit are still required.

An independent reviewer with current production AppKit experience must sign the canonical source, dependency, build, scaling, bounded-profile, parity, artifact, and release evidence before the controller can enter measured acquisition.

## Verification

`AppKitProductionReferenceArchitectureTests.swift` verifies the exact twelve-scene class mapping, including the five release roots, plus dashboard/endurance grid ownership, document-view chains, live data-source/delegate ownership, native controls, cleanup behavior, one-scene host lifecycle, and native accessibility identity. `AppKitProductionScenarioAdapterTests.swift` also replays all loadable endurance events and compares every resulting state/accessibility checkpoint with the committed contract.

```text
xcodegen generate --use-cache --spec host/apple-comparison/project.yml
xcodebuild -project host/apple-comparison/AppleComparison.xcodeproj -scheme AppKitComparison -configuration Debug -destination 'platform=macOS,arch=arm64' test
```

These checks prove the architecture foundation only. Exact checkpoint parity and comparator admission remain separate hard gates.

## Changelog

- 2026-07-26: retained pinned inline symbol/emoji raster pieces in production-reference text fields and drew them at the declared baseline.
- 2026-07-21: added the root-owned horizontal AppKit pan endpoint for the public-XCUI navigation interactive-cancel override and verified native mouse tracking through the existing image zoom slider target/action.
- 2026-07-21: bound correctness geometry to each active AppKit root and its live view/text rectangles under the shared 3x capture profile.
- 2026-07-21: Removed synchronous layout/display forcing from production measured event and display-link paths, installed native action bindings once, routed navigation and composer input through AppKit, and made unsupported measured user traces fail closed.
- 2026-07-21: added distinct native AppKit model/root/adapter implementations for all five frozen release candidates; canonical screenshot binding remains pending.
- 2026-07-21: added the distinct AppKit `endurance.churn` root/model/adapter with exact 100/500/600 event recovery.
- 2026-07-21: Added the event-free `idle.steady` nightly workload as an exact full-dashboard alias with its own checkpoint identity.
- 2026-07-20: Replaced the generic feed placeholder with the frozen direct viewport, native reusable card hierarchy, pinned assets/fonts/colors, circular target-action control, incremental same-count rebinding, and layout-before-scroll ordering.
- 2026-07-21: Replaced full feed/chat/navigation event resynchronization with indexed native row mutations, bound chat send to retained model mutation, and moved full-image downsample and Metal texture population into the declared upload event.
- 2026-07-20: Added a benchmark-only six-turn AppKit retirement drain after every scenario teardown, retaining fail-closed app-owned weak-resource checks while leaving framework-owned reuse caches to physical-footprint qualification.
- 2026-07-19: Rebound same-count dashboard mutations into visible items without collection reloads; offscreen items materialize from current records.
- 2026-07-19: Replaced the generic dashboard placeholder with the frozen 32-card native collection, component-local exact drawing, real label/image/button leaves, four backdrop owners, and item-granular mutation updates.
- 2026-07-19: Added the six real AppKit scene roots, reusable collection/table foundations and controllers, native target-action, owned text view, native accessibility evidence, and architecture tests; retained rejected admission until functional and visual completion.
