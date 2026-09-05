# Apple comparison preview smoke

- Status: diagnostic-only physical-host correctness smoke
- Device: unlocked USB iPhone18,2, iOS 26.5.1 (`00008150-001529C434F8401C`)
- Apps: signed Release `com.oxide.comparison.uikitbenchios` and `com.oxide.comparison.oxidebenchios`, Team `6GQ7T2VDQ5`
- Viewport: exact centered 390x844 logical points at 3x (1170x2532 backing)
- Font pack: `33140f1da80a9de06824427773c4fe0f890ab05d6f6431eb45d07522fb2ca84d`

| Scenario | UIKit roles | Oxide roles | Frozen fixture |
| --- | --- | --- | --- |
| `dashboard.mixed-static` | match | match | `b3f1a54053b7b9de284164bba630f3091c73d2a10de7e2b0bea10ee87d954579` |
| `feed.variable-scroll` | match | match | `e96e75c68268cc188fee5ab0c5ae84b6be23fea17c05636a8769976c705d2e04` |
| `navigation.modal` | match | match | `0490ef34ec47dfc1d56377283596ec7034af71bd62e005163bad5fecb027730b` |

Each app launch loaded the bundled scenario, recursively validated its content-addressed artifact graph, registered the pinned font pack, laid out the fixed viewport, wrote a durable Documents diagnostic, and remained running. Oxide used the normal Rust router, draw list, `MetalView`, `CAMetalLayer`, and Metal renderer, with one immutable upload of the pinned atlas. The host pulled each diagnostic through CoreDevice and compared both implementations' observed role counts with the frozen fairness contract.

This is not benchmark evidence. Preview mode bypasses the controller and telemetry ring. Screenshots, exact state/accessibility checkpoints, animation checkpoints, input traces, reset/teardown, and presentation attribution remain unproven.
