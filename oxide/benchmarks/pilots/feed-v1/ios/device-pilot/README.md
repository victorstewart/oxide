# feed-v1 physical-device pilot

This benchmark-only device pilot builds exactly two applications and one
UI-test controller from `project.yml`:

- `FeedV1UIKit` chooses the idiomatic or optimized collection treatment once at
  process launch;
- `FeedV1Oxide` links the production injected-app host through
  `liboxide_feed_v1_app.a`; and
- `FeedV1Controller` launches fresh processes and performs only the frozen
  absolute-coordinate XCTest fling.

There is one visible feed screen in each app. App readiness and completion use
nonce-scoped Darwin notifications. Raw run JSON stays app-owned until the host
runner retrieves each app's `Documents` directory. Each of the six smoke
treatment/direction tuples attaches one full-screen PNG named
`feed-v1-<nonce>.png`; there is no capture JSON or repeat image. The 54 primary
tuples produce app records but no screenshots. The retained evidence package
contains exactly those six smoke PNGs, not the `.xcresult` or reducer
executable.

`FeedV1Pilot.xcodeproj` is generated output and is not checked in. To generate
the arm64-only project outside the source tree for inspection:

```sh
pilot_root=oxide/benchmarks/pilots/feed-v1/ios/device-pilot
pilot_build=$(mktemp -d /tmp/oxide-feed-v1-xcodegen.XXXXXX)
xcodegen generate --spec "$pilot_root/project.yml" \
  --project "$pilot_build" --project-root "$pilot_root"
```

`run-device.sh <CoreDevice-ID> <result-root> [smoke|full]` accepts only
CoreDevice `1DEDF2A3-EC8E-5FCC-A437-8BD3A6F3D659`, requires it to resolve to
hardware UDID `00008150-001529C434F8401C`, and creates an external build root,
generates the project there from `project.yml`, then performs the bounded build,
test, artifact retrieval, reduction, and temporary-product cleanup. Omitting the
mode selects `smoke`. Official output requires a
physical 120 Hz iPhone; the project does not declare a Simulator platform or
architecture. The result root must be a new directory outside the Git worktree.
Before creating it or touching the phone, the runner requires a clean named Git
branch and captures `HEAD` and `HEAD^{tree}`. It then runs the Swift contract
executable and the exact Rust canonical identity test against that frozen source
before generating or building either device app. This keeps full 2,000-row
derivation out of measured app startup while preserving independent byte-exact
proof. Before installation, the runner
admits both built products only when each executable is exactly arm64, each
processed plist is iPhoneOS-only and arm64-required, and both freeze
full-screen portrait presentation with the status bar hidden. It also requires
the UIKit bundle to contain byte-identical copies of both frozen Asap font
files. It separately resolves and verifies the processed controller runner
bundle ID `com.oxide.feed-v1.controller.xctrunner`, the embedded xctest ID, and
exact arm64 executables, then hashes both controller products into the evidence
manifest. The manifest also embeds the device model and OS build, the 120 Hz
contract, Xcode/SDK/Rust versions, a hash of resolved Release build settings,
the root Cargo lock and resolved production metadata hashes, and the actual
Authority/team/CDHash identities of all four signed products. Supporting raw
tool output remains beneath `raw/provenance/`. The home indicator is outside the frozen comparison crop. The runner
refuses a locked phone, removes and verifies stale installations of the two
pilot bundle IDs and controller runner, proves no exact controller process
survives, and captures the same physical-device identity before and after the
run.

Device signing follows the workspace convention: set
`OXIDE_IOS_DEVELOPMENT_TEAM` to the 10-character team identifier. The runner
passes that team, `Automatic` signing, and `Apple Development` explicitly to
both Xcode phases; it does not bake a developer identity into the project. It
rejects a translated host process, any non-arm64 resolved architecture, a
foreign signing team, or a non-development authority before device launch.

The default `smoke` mode strictly verifies the six frozen treatment/direction
tuples and produces no timing classification, cross-treatment travel claim, or
publication report. Passing `full` runs those six diagnostics plus exactly one
54-run primary block: nine paired clusters, each containing three treatments in
both directions. Only that replicated population may admit travel equivalence
and write `latest.json` and `latest.md`. Their travel-equivalence array/table
persists all four admission decisions, and their run table contains the 54
primary timing rows; the six smoke run records remain diagnostic evidence.
Cleanup proof records actual Xcode test success and the exact six
verified smoke PNGs in either mode; a merely present result bundle cannot
authorize a report. The controller also writes one app-container runtime proof.
The reducer enforces the 20-minute total and each 10-minute per-treatment limit
from that record, while the runner independently fails an overlong Xcode phase.
After evidence export, the runner rechecks the clean named ref/commit/tree before
reduction.

The pre-reduction cleanup proof states only facts already observed, including
that the reducer executable is absent from the retained result root. The runner
keeps that executable in a separate temporary directory while reducing, removes
it afterward, and fails the overall run if the final removal cannot be proved.
The publication report therefore does not attest a cleanup action that occurs
after it is written.

The app record contract is frozen in `run-record-schema.md`. Display-link data
is callback pacing only. No presentation or photon-latency claim is produced.
Both apps validate the same 1-through-128-byte ASCII
alphanumeric-or-hyphen nonce grammar and preallocate the same 1,024 callback
samples; invalid identity or capacity exhaustion fails the run.
Every run must prove at least `524 pt` of travel and actual entry into inertial
motion. Thermal and Low Power Mode notification counts are baselined at ready
admission and must remain zero through completion.
