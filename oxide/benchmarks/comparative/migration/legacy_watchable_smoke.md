# Legacy Apple watchable-smoke migration baseline

On the physical iPhone18,2, the existing device harness completed one build and a 14-row watchable smoke in approximately 12 minutes 20 seconds including build time. It produced 14 process-scoped Metal System Trace bundles and 138 rendered-frame PNGs.

The current legacy source surface contains 197 XCTest methods: 185 in `OxideHostPerfTests.swift` and 12 launch methods. The smoke sampled nine UIKit rows and five Oxide rows across component, camera, navigation, animation, and journey families. All families passed the harness watchability gate, but no family proof was run.

The optional `Metal GPU Counters` profile was rejected by the device/toolchain on Oxide rows; the harness correctly retained in-app Metal timing and process-scoped Metal System Trace. One legacy custom-NV12 camera XCTest failure was retained with its usable diagnostics rather than retried away.

This is a migration baseline, not an official Oxide/UIKit comparison. It does not establish the required 80% physical-device-minute reduction or 5× unique-risk detection density; those require the committed disposition map, detection matrix, and packed PR campaign.
