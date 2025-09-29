# OxideUI Nametag Migration Plan

> Scope: greenfield rewrite of Nametag in a new OxideUI workspace that ships the same Rust codebase into native wrappers on both iOS and Android. The legacy UIKit/Objective-C app remains untouched; all parity lives inside the OxideUI runtime plus thin platform hosts.

| Phase | Focus | Status |
| --- | --- | --- |
| P1 | Dependency & asset recon | Pending |
| P2 | OxideUI capability baseline | Pending |
| P3 | UI node API specification | Pending |
| P4 | Lifecycle, layout & navigation host design | Pending |
| P5 | Animation & interaction framework alignment | Pending |
| P6 | Media capture & rendering bridge | Pending |
| P7 | Sensor & environment services contract | Pending |
| P8 | Permissions & consent UX architecture | Pending |
| P9 | Notifications & background execution pathways | Pending |
| P10 | Networking, security & telemetry channel design | Pending |
| P11 | Storage, keychain & filesystem strategy | Pending |
| P12 | Tooling, QA & launch readiness | Pending |

## Phase P1 – Dependency & Asset Recon
- **Objective**: Produce an authoritative inventory of every third-party library, system framework, analytic SDK, feature-flag engine, and bundled asset the new cross-platform app must support.
- **Scope & Boundaries**: CocoaPods/Gradle deps, static/dynamic frameworks, JNI/NDK modules, resource bundles, proprietary forks, analytics/crash tooling, provisioning assets, fonts, icons, animation JSON. Excludes legacy-only debugging scaffolds already retired.
- **Primary Tasks**:
  1. Parse `Podfile`, `Nametag.xcodeproj`, `build.gradle`, and legacy manifests to gather dependency metadata, minimum OS versions, and transitive requirements.
  2. List proprietary frameworks or AARs (e.g., `SVGKit.framework`, Texture fork) with commit hashes, license posture, and upgrade constraints.
  3. Catalog UI-impacting assets (fonts, icons, media, animation JSON) with formats, density buckets, localization considerations (even if currently unused), and ingestion pipeline.
  4. Record analytics, crash reporting, logging, feature-flag tooling, and telemetry endpoints with initialization expectations and data residency constraints.
- **Inputs Required**: Legacy repo metadata, provisioning profiles, Play Store capability matrix, asset directories, prior system import summaries.
- **Deliverables**: Markdown appendix mapping each dependency to purpose, license, OxideUI integration posture (native, bridge, replace), platform coverage (iOS, Android, both), and owner contact.
- **Exit Criteria**: Every module referenced in `#import`, `#include`, Gradle manifests, or dynamically loaded at runtime is tagged with an OxideUI disposition, risk rating, and parity notes.
- **Risks & Mitigations**: Hidden runtime dependencies (mitigate via `otool -L`, `nm`, `jdeps` scans); stale vendored forks (flag refresh requirements); missing license coverage (engage legal review).
- **Follow-on Notes**: Output feeds P2 coverage analysis and P10 telemetry design.

## Phase P2 – OxideUI Capability Baseline
- **Objective**: Align Nametag’s requirement ledger with OxideUI’s current crates on both renderers (`renderer-metal`, `renderer-vulkan`) and platform bridges (`platform-ios`, `platform-android`) to expose capability gaps before drafting APIs.
- **Scope & Boundaries**: Rendering, layout, animation, gesture handling, camera/audio, Bluetooth/location, notifications, telemetry, storage, background execution across iOS and Android hosts.
- **Primary Tasks**:
  1. Enumerate OxideUI modules, traits, and FFI surfaces by reviewing crate sources and docs (`ui-core`, `input`, `timing`, `text`, `platform-*`, `telemetry`, `permissions`).
  2. Map Nametag functional requirements (from P1) to existing OxideUI features, noting direct coverage vs. adapters vs. net-new work for each target platform.
  3. Verify renderer parity (Metal vs. Vulkan) for required effects, including HDR/EDR, shader availability, and texture formats.
  4. Capture telemetry/logging requirements and confirm OxideUI crates can deliver equivalent analytics/crash hooks on both platforms.
  5. Highlight constraints (timing precision, GPU feature flags, background limits) and file upstream tickets where already tracked.
- **Inputs Required**: P1 ledger, OxideUI crate documentation, macOS/iOS host demos, Android host scaffolds.
- **Deliverables**: Coverage matrix listing requirement → OxideUI module/trait → disposition (`native`, `extend`, `build`) with platform column and commentary on technical debt/quick wins.
- **Exit Criteria**: All Nametag UI/system touchpoints and operational hooks have explicit OxideUI alignment status per platform and assigned owner in later phases.
- **Risks & Mitigations**: Doc drift vs. source (validate with code); macOS-centric assumptions (test against Android host); overlooked subsystems (cross-check with P1 ledger, especially telemetry/crash reporting).
- **Follow-on Notes**: Guides prioritization for specs in P3–P12 and informs Rust crate boundaries.

## Phase P3 – UI Node API Specification
- **Objective**: Define Rust-side abstractions for AsyncDisplayKit-style nodes so Nametag screens can be recreated with functional parity using OxideUI primitives across both platforms.
- **Scope & Boundaries**: Node lifecycle, diffing/state propagation, composable traits, text input behaviors, design-system tokens. Navigation routing moves to P4.
- **Primary Tasks**:
  1. Decompose custom Nametag nodes (`HorizontalShiftingTextNode`, `BadgeableButton`, etc.) into capability traits leveraging `ui-core::elements`, `layout_async`, and `orchestration` patterns.
  2. Specify OxideUI node interfaces (property binding, sizing callbacks, async data flows) and align with `platform-api::App::event` for iOS and Android hosts.
  3. Identify native interop hooks (camera overlays, sensor readouts) needing FFI shims and define safe wrappers that compile for both Objective-C and JNI bridges.
  4. Document animation hooks each node expects so P5 timelines can bind without rework.
- **Inputs Required**: Nametag node sources, OxideUI node tree design, P2 coverage matrix.
- **Deliverables**: Spec with trait signatures, state diagrams, component catalog, and bridging notes for iOS/Android hosts.
- **Exit Criteria**: Every visual node has a mapped OxideUI trait and adapter plan with unresolved questions flagged for review.
- **Risks & Mitigations**: Overfitting to legacy semantics (keep interfaces minimal); performance regressions (mark hot paths for benchmarking); duplicated platform-specific glue (centralize adapters).
- **Follow-on Notes**: Feeds P4 layout composition and P5 animation events.

## Phase P4 – Lifecycle, Layout & Navigation Host Design
- **Objective**: Design the end-to-end flow binding OxideUI scenes to native hosts on iOS (Obj-C/Swift) and Android (Kotlin/NDK), covering layout orchestration, navigation stacks, and lifecycle/event routing.
- **Scope & Boundaries**: `platform-api::App` integration, host event loops (`UIApplication`/scene delegates, Android `Activity`/`SurfaceView`), layout spec mapping, navigation transitions, modal management. Animation specifics remain in P5.
- **Primary Tasks**:
  1. Catalog layout patterns (absolute, inset, carousel, async collection) and map them to `ui-core::layout_async` and `collection` builders, noting platform differences (e.g., safe areas vs. insets).
  2. Design lifecycle glue between Rust app code and native wrappers: app launch, background/foreground, configuration changes, memory warnings, process restarts.
  3. Define navigation host responsibilities (window/surface creation, modal/popup layers, deep-link routing) and bridging to native controllers/fragments when OS-specific UI is required.
  4. Specify state persistence boundaries and error propagation paths per platform.
- **Inputs Required**: P3 spec, OxideUI host samples (`host/macos-app`, `platform-ios`, `platform-android`), Nametag navigation flows.
- **Deliverables**: Systems diagram, pseudo-code for host bridges, lifecycle state machines, layout builder catalog with platform notes.
- **Exit Criteria**: Each layout construct and navigation flow has a defined OxideUI implementation path with explicit host responsibilities for iOS and Android.
- **Risks & Mitigations**: UIKit vs. Android event ordering mismatches (prototype with both hosts); configuration change handling on Android (plan rehydration); background mode interplay with lifecycle (coordinate with P7/P9).
- **Follow-on Notes**: Must align with animation triggers (P5) and background execution (P7/P9).

## Phase P5 – Animation & Interaction Framework Alignment
- **Objective**: Provide an OxideUI animation API surface covering Core Animation/Android Renderer equivalents (wiggles, glows, chained sequences) and gesture-driven transitions across both targets.
- **Scope & Boundaries**: Declarative animation timelines, gesture recognition, interaction feedback. Video frame rendering handled in P6.
- **Primary Tasks**:
  1. Inventory animation patterns (wiggle, glow, progress label timings, chained sequences) and map them to `ui-core::anim::helpers`, `oxideui_timing` curves, and renderer overrides for Metal and Vulkan.
  2. Define missing animation descriptors or timeline combinators and ensure shader support on both renderers (e.g., additive blend, blur).
  3. Integrate gestures using `oxideui-input` (tap, pan, long-press, multi-touch) and confirm Android host event propagation matches iOS behavior.
  4. Build prototype scenes on each platform/renderer to validate parity before finalizing API.
- **Inputs Required**: Legacy animation helpers, OxideUI animation modules, renderer feature lists.
- **Deliverables**: API proposal with timing diagrams, cross-platform usage snippets, prototype findings, and renderer capability matrix.
- **Exit Criteria**: Every animation has a planned OxideUI construct with proven feasibility on Metal and Vulkan hosts.
- **Risks & Mitigations**: Renderer gaps (add shader tasks/fallbacks); differing frame pacing (align with platform timing APIs); gesture conflicts (define priority rules per platform).
- **Follow-on Notes**: Coordinates with P6 camera overlays and P8 permission prompts.

## Phase P6 – Media Capture & Rendering Bridge
- **Objective**: Architect OxideUI interfaces for camera preview, video capture, GPU rendering, and recording flows replacing AVFoundation and Android Camera pipelines.
- **Scope & Boundaries**: Camera session control, NV12/YUV texture ingestion, audio capture, encoder hooks, gallery export. Sensor fusion stays in P7.
- **Primary Tasks**:
  1. Break down Nametag camera pipeline (`MetalCamera`, `VideoRecorder`, Android equivalents) into responsibilities and latency budgets.
  2. Define how `platform-ios` and `platform-android` camera traits will be bound, including threading, buffer reuse, Metal/Vulkan texture sharing, and encoder selection (HEVC/H264/MPEG4).
  3. Document entitlements/permissions and background-mode requirements for both platforms, including Android’s manifest permissions and scoped storage.
  4. Sketch error handling and recovery flows (thermal throttling, capture interruptions, camera rebind after configuration changes) with telemetry hooks.
- **Inputs Required**: Camera source files, `oxideui_platform_api` camera traits, platform FFI surfaces, Apple/Android docs.
- **Deliverables**: Interface contract with state diagrams, threading expectations, data formats (NV12, YUV_420_888), and entitlement checklist per platform.
- **Exit Criteria**: End-to-end capture flow mapped to OxideUI surfaces with fallback strategies and metrics defined for iOS and Android.
- **Risks & Mitigations**: Hardware encoder differences (abstract via trait); memory pressure (monitor through host callbacks); audio/video sync (plan instrumentation in P12).
- **Follow-on Notes**: Depends on P7 sensors and P11 storage destinations.

## Phase P7 – Sensor & Environment Services Contract
- **Objective**: Specify OxideUI wrappers for location, Bluetooth LE, motion, altitude, and time sync services supporting Nametag radar features on both platforms.
- **Scope & Boundaries**: Foreground/background updates, BLE scanning, beaconing, time sync. Permission UI covered in P8.
- **Primary Tasks**:
  1. Model sensor interactions (`LocationHardwareProxy`, `BluetoothHardwareProxy`, `AltitudeEngine`, `ActivityTracker`, `TimeServer`) and map to `platform-api` traits plus `platform-ios`/`platform-android` callbacks.
  2. Define trait implementations handling callbacks, background execution guarantees, BLE central/peripheral coexistence, and telemetry reporting per OS constraints.
  3. Capture coexistence rules (simultaneous BLE modes, location accuracy downgrades) and throttle/backoff policies tailored to iOS and Android power models.
  4. Outline time-sync integration (UDP or alternative) and OxideUI runtime scheduling primitives.
- **Inputs Required**: Sensor code, OS capability notes, OxideUI async runtime constraints.
- **Deliverables**: Sensor blueprint with event diagrams, error enums, throttle policies, and metrics schema across platforms.
- **Exit Criteria**: All sensor workflows mapped to OxideUI traits with platform expectations for accuracy, latency, power usage, and telemetry coverage.
- **Risks & Mitigations**: BLE restore semantics (persist state snapshots per OS); background execution limits (define watchdog behaviors); platform policy changes (monitor release notes).
- **Follow-on Notes**: Feeds P8 permission gating and P10 network-triggered actions.

## Phase P8 – Permissions & Consent UX Architecture
- **Objective**: Design unified permission handling mirroring Nametag’s custom flows while leveraging `platform-api::Permissions` across iOS and Android.
- **Scope & Boundaries**: Camera/mic/location/media/contacts/Bluetooth permissions, platform-specific rationale screens, and custom consent flows. Push token storage handled in P9.
- **Primary Tasks**:
  1. Document each permission state machine (`Permissions.hpp`, `GetPermission`, Android equivalents) aligned with `platform-api` enums and callbacks.
  2. Define OxideUI permission manager APIs with request triggers, rationale messaging, callback routing, and telemetry hooks per platform.
  3. Map UI presentation requirements (icons, copy, timers) to node traits (P3), noting any platform-specific guidance (e.g., Android overlay warnings).
  4. Capture failure/retry pathways and asynchronous host coordination.
- **Inputs Required**: Permission code paths, assets, Apple/Android guidelines, P3 spec.
- **Deliverables**: Flowcharts plus API spec for permission orchestration, UI scaffolding, telemetry events, and platform nuance notes.
- **Exit Criteria**: Each permission has a codified OxideUI flow with platform divergence documented and localization hooks reserved if needed later.
- **Risks & Mitigations**: Naming differences (encapsulate in config); platform policy shifts (monitor updates); background prompts deadlocking UI (define async patterns).
- **Follow-on Notes**: Interlocks with P7 sensors and P9 notifications.

## Phase P9 – Notifications & Background Execution Pathways
- **Objective**: Plan OxideUI support for push notifications, badges, and background wake flows on APNs and FCM while matching Nametag behavior.
- **Scope & Boundaries**: Push registration, service extension handling, in-app handlers, background timers, badge mutations.
- **Primary Tasks**:
  1. Analyze NotificationCenter/APNs and Firebase workflows, token persistence, badge logic, and service extension needs; map to `platform-api::PushManager` and platform callbacks.
  2. Define Rust interfaces for registering, handling foreground/background notifications, mutating badges/bubbles, and delegating to native extensions where OS mandates.
  3. Document required background execution triggers (BLE, location, notifications) and interplay with lifecycle states per platform.
  4. Plan telemetry for notification delivery, open rates, and wake diagnostics across both ecosystems.
- **Inputs Required**: Notification source files, AppDelegate/Android lifecycle handling, OxideUI runtime design.
- **Deliverables**: Integration schematic plus API spec for notification lifecycle, background dispatchers, telemetry events, and platform-specific notes.
- **Exit Criteria**: Notification flows fully represented with extension hand-offs, credential storage paths, and logging.
- **Risks & Mitigations**: Capability variations (encapsulate extension hooks); race conditions updating shared token state (define locking strategy); service reliability differences (implement backoff/diagnostics).
- **Follow-on Notes**: Supports P10 for secure networking and P11 for storage.

## Phase P10 – Networking, Security & Telemetry Channel Design
- **Objective**: Define OxideUI networking stack requirements (TLS, reachability, background transfers, telemetry delivery) replacing Objective-C/C++ and Android Java helpers.
- **Scope & Boundaries**: TLS credential handling, reachability monitoring, background download/upload APIs, analytics/crash forwarding, CloudKit/Play Services interactions. Offline storage is P11.
- **Primary Tasks**:
  1. Break down `TLSLockbox`, `Reachability`, `ForeignEngine`, analytics emitters, Android networking layers into responsibilities.
  2. Specify OxideUI networking layer requirements using `oxideui-networking`, `platform-api` reachability traits, platform services (NWPathMonitor, ConnectivityManager), and background transfer shims.
  3. Define credential storage & keychain/Keystore access patterns, including certificate pinning, mutual TLS, and hardware-backed storage.
  4. Design telemetry pipelines: which events stay local vs. stream, batching, retry/backoff, crash log capture, privacy filters.
- **Inputs Required**: Networking code, OxideUI network capabilities, compliance constraints, telemetry schemas.
- **Deliverables**: Security design note with API contracts, credential flows, telemetry pipelines, and error taxonomy per platform.
- **Exit Criteria**: Each networking/telemetry operation has an OxideUI counterpart with documented credential storage, trust evaluation steps, and compliance notes for iOS and Android.
- **Risks & Mitigations**: Keychain/Keystore disparities (abstract providers); background transfer limits (plan BGTaskScheduler/WorkManager usage); telemetry spikes (rate limiting & batching).
- **Follow-on Notes**: Coordinates with P11 persistence and P7 sensor-triggered events.

## Phase P11 – Storage, Keychain & Filesystem Strategy
- **Objective**: Establish OxideUI abstractions for file management, secure token storage, and cached media aligned with iOS and Android expectations.
- **Scope & Boundaries**: Document/cache/temp directories, push token storage, badge counts, media export paths, telemetry queue persistence.
- **Primary Tasks**:
  1. Review legacy storage helpers, keychain/Keystore usage, media export flows, telemetry buffering, and background write requirements.
  2. Design a Rust storage facade exposing directory provisioning, backup exclusion, scoped storage strategies, temp file helpers, and background-safe IO.
  3. Specify secure storage bridges for keychain/Keystore operations, including migration strategy for tokens and secrets.
  4. Define retention policies, cleanup jobs, and quota enforcement across platforms.
- **Inputs Required**: Storage code, OS docs, OxideUI configuration modules, compliance requirements.
- **Deliverables**: Storage design spec with API list, path conventions, lifecycle notes, and migration considerations per platform.
- **Exit Criteria**: All persisted artifacts mapped to OxideUI storage APIs with thread-safety guarantees and telemetry persistence accounted for on iOS and Android.
- **Risks & Mitigations**: Secure storage disparities (abstract layer, per-platform provider); disk pressure (add quotas/purge policies); background IO restrictions (use WorkManager/BGTaskScheduler).
- **Follow-on Notes**: Feeds P12 testing and operational tooling.

## Phase P12 – Tooling, QA & Launch Readiness
- **Objective**: Plan developer tooling, automated tests, and rollout sequencing so the cross-platform Rust app can be validated quickly on both ecosystems.
- **Scope & Boundaries**: Build scripts, FFI test harnesses, GPU validation, CI pipelines, telemetry verification, documentation. Implementation beyond scaffolding remains out-of-scope here.
- **Primary Tasks**:
  1. Define test suites (unit, integration, snapshot, device, performance, soak) derived from P3–P11, including Metal/Vulkan rendering baselines, camera/audio soak tests, and telemetry replay harnesses.
  2. Outline CLI tooling/scripts (trait codegen, asset validators, entitlement/permission auditors, telemetry schema linters, Android manifest validators).
  3. Draft readiness checklist covering performance baselines, battery/thermal regression gates, permission audits, capture/storage stress tests, App Store/Play Store submission requirements.
  4. Define observability and alerting workflows (log sinks, crash symbolication, metrics dashboards) operational from day one.
- **Inputs Required**: Prior phase requirements, OxideUI CI/CD capabilities, platform submission guidelines.
- **Deliverables**: QA/tooling roadmap with milestones, automation inventory, platform device matrix, and ownership assignments.
- **Exit Criteria**: Every preceding phase has linked validation steps, tooling assignments, and acceptance thresholds captured in the readiness checklist for iOS and Android.
- **Risks & Mitigations**: Underestimated device matrix (document minimum hardware targets); tooling drift (schedule audits); CI resource gaps (budget GPU-accelerated runners for both platforms).
- **Follow-on Notes**: Serves as final sign-off gate before feature implementation sprints begin.

## Out-of-Scope (Confirmed)
- Dynamic Type, VoiceOver, localization, GDPR/CCPA consent flows, and rollback to the legacy UIKit app are intentionally excluded from this greenfield effort per product direction. Document incidental hooks only if required for future compliance reviews.
