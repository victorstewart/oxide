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
runner retrieves each app's `Documents` directory. Each of the six publication
smoke treatment/direction tuples attaches two immediate normalized full-screen
PNGs named `feed-v1-<nonce>-admission.png` and
`feed-v1-<nonce>-repeat.png`. Both independently pass the frozen visual gate,
and their normalized surface crops must be byte-identical. The 54 primary
tuples produce app records and prove an attachment-free XCTest export. The
retained publication evidence therefore contains exactly 12 smoke PNGs, not
the `.xcresult` or reducer executable.

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
pilot bundle IDs and controller runner, terminates and proves absence of the
exact `FeedV1UIKit`, `FeedV1Oxide`, and `FeedV1Controller-Runner` processes, and
captures the same physical-device identity before and after the run.

Before the controller launch, the runner admits a build root no larger than
`4,294,967,296` bytes and a retained tree no larger than `536,870,912` bytes,
`512` files, or `134,217,728` bytes per file. The not-yet-created `.xcresult`
has its own `536,870,912`-byte cap before attachment export. The same build,
retained-byte, file-count, and per-file limits are checked again after their
respective phases and recorded in the strict cleanup proof.

Device signing follows the workspace convention: set
`OXIDE_IOS_DEVELOPMENT_TEAM` to the 10-character team identifier. The runner
passes that team, `Automatic` signing, and `Apple Development` explicitly to
both Xcode phases; it does not bake a developer identity into the project. It
rejects a translated host process, any non-arm64 resolved architecture, a
foreign signing team, or a non-development authority before device launch.

The default `smoke` mode strictly verifies the six frozen publication
treatment/direction tuples and their 12 captures, but produces no timing
classification, cross-treatment travel claim, or publication report. Passing
`full` first runs and admits that exact smoke population. Only then does a
separate controller phase run one 54-launch primary block: nine paired clusters,
each containing three treatments in both directions and no screenshots. The
full population is therefore exactly 60 launches. Only its replicated primary
population may admit travel equivalence and write `latest.json` and `latest.md`.
Those outputs persist all four travel-equivalence decisions. Canonical JSON
contains the 54 primary timing rows; compact Markdown contains aggregate
summaries only. The six smoke run records remain admission evidence. A smoke
blocker stops before the primary phase without replacement.

Cleanup proof records actual Xcode test success and the verified 12-attachment
smoke count. The primary phase separately proves zero attachments. A merely
present result bundle cannot authorize a report. Publication writes
`oxide-feed-v1-controller-runtime-smoke.json`; full mode additionally writes
`oxide-feed-v1-controller-runtime-primary.json`. The reducer sums the two
proofs' runtimes before enforcing the 20-minute total and each 10-minute
treatment limit. `raw/runner.json`
records the requested mode, completed or blocked status, first failure, smoke
admission result, and phase outcomes so a blocked primary cannot erase valid
smoke admission. The runner independently fails an overlong Xcode phase and
rechecks the clean named ref/commit/tree after evidence export.

Controller-runtime schema revision 2 requires `session_environments` on every
publication runtime proof. Smoke records use an empty array. The primary proof
uses exactly three ordered session entries for
indices 0, 1, and 2, sampled before and after each session's 18 launches. Each
endpoint must report nominal thermal state, Low Power Mode off, maximum refresh
exactly 120 Hz, and configured minimum, maximum, and preferred refresh exactly
120 Hz; both transition counts must be zero.

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
