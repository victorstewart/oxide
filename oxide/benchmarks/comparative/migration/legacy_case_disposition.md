# Legacy Apple method disposition review

The committed [legacy disposition manifest](../legacy_case_disposition.json) maps the live 197-method source inventory exactly once. `cargo xtask compare-ui validate` extracts both Swift files, compares qualified `Class.testMethod` identities, validates replacement targets and risk ownership, and requires canonical JSON.

| Disposition | Methods |
| --- | ---: |
| `comparative_phase` | 37 |
| `native_ceiling_only` | 37 |
| `internal_microbench` | 43 |
| `fast_correctness` | 27 |
| `specialized_suite` | 13 |
| `remove_duplicate` | 40 |
| **Total** | **197** |

Important review decisions:

- Composite class/method identity is mandatory because camera method names repeat across the two XCTest classes.
- Optimized internal and platform-bridge wrappers are duplicate acquisition surfaces, not native-ceiling candidates.
- Native-ceiling candidates are limited to the 37 product-shaped primary counterparts and still require independent comparator acceptance.
- Camera/media remains an opt-in specialized suite.
- Permission, sensor, Bluetooth, photo/file/share, and local-transport bridge primaries become unmeasured integration correctness; their optimized wrappers are removed duplicates.
- Active `IdleAnimation600Frames` contributes to endurance migration only and cannot establish settled `idle.steady` behavior.
- Warm resume and foreground-after-background are phases of `startup.first-screen`; deep-link navigation belongs to the navigation journey.

The manifest deliberately records seven pre-existing v1 gaps rather than crediting weak legacy evidence: idle scheduling, payload delivery, multilingual shaping, visual fidelity, accessibility semantics, offscreen-effect attribution, and retained-resource slope. These gaps must be closed by canonical scenarios, correctness fingerprints, and the fault/detection matrix before the migration’s coverage and density acceptance gates can pass.
