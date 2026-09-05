# Apple comparison Phase 0 — 2026-07-17

Status: **partial**. The fail-closed per-app-container fallback is proven on the physical iPhone; the UI-test controller body remains blocked by XCTest automation initialization.

## Observed result

| Property | Result |
| --- | --- |
| Device | iPhone18,2, iOS 26.5.1 (23F81), USB, unlocked |
| Build | one successful Release `build-for-testing`; both apps plus generated runner |
| UI-test source surface | exactly 2 public methods; 0 scenario-named methods |
| Signing | all three executables use Team `6GQ7T2VDQ5` |
| Preferred App Group | unavailable; wildcard profiles contain no App Groups entitlement |
| Frozen fallback | `per-app-container` |
| Durable side artifacts and ACKs | passed for Oxide and UIKit |
| Cross-side chain | native predecessor equals raw pulled Oxide SHA-256 |
| Host recovery/checkpoint | passed; synchronized checkpoint SHA-256 `45ff1b9e…f39a56` |
| Stale generation | rejected; no checkpoint created |
| Forced runner termination | app artifact and durable ACK remained pullable afterward |
| Telemetry channel | app containers only; no accessibility, console, or `.xcresult` telemetry |

The executable and embedded-profile inspection showed the same Team on both apps and `ComparisonControllerUITests-Runner`. All three profiles expose only wildcard application identity `6GQ7T2VDQ5.*`; therefore App Group mode was rejected before acquisition and unsupported entitlements were removed from the fallback target graph.

The real fallback pair produced:

- Oxide artifact `fb4dca8…ddb395`, ACK `6975c0f6…c95f8`.
- UIKit artifact `987784db…de3b3`, ACK `47dd21fa…b660c`.
- A native predecessor equal to the exact Oxide artifact hash.

## Remaining Phase-0 gate

Four controller attempts launched the signed runner but failed before any test body executed: three timed out enabling automation mode and one ended when local authentication was canceled. The latest retry used a newly produced but byte-identical base `.xctestrun` (`fdd82c5c…37b6e`), an unlocked USB device, and the frozen per-app environment; XCTest still timed out after 62.070 seconds while enabling automation. This is retained as an exogenous host-path blocker; it is not counted as transport success. The direct installed-binary fallback and host pull/finalizer path are proven, including persistence while that runner was forcibly torn down.

This evidence is transport-only and contains no framework performance result or claim.
