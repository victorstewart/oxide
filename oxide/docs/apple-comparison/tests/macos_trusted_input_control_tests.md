# macOS trusted-input control tests

`MacOSTrustedInputControlTests.swift` compiles together with the comparison campaign executor and XCUI performer. It freezes supported command grouping, the exact two-event `chat:append:16` select/replace command, rejection of arbitrary ranges/targets/replacements, absorbed events, application-stimulus separation, lifecycle/IME/multipointer rejection, exact durable foreground controller and application receipt validation, ordered mouse/key `NSEvent` evidence, full trace-range and request/controller hash binding, state-transition failure closure, session-global command sequencing, public-XCUI-only dispatch, and unconditional UI-test dismissal. Source-boundary coverage additionally proves that request canonicalization/hashing is pre-start, that descriptor-preparation time is named separately from live dispatch time, that scheduled application/controller functions contain no file/JSON/hash work, and that receipt persistence is invoked only by the post-measurement flush.

`AppKitProductionScenarioAdapterTests.testMeasuredNavigationAndComposerEventsDispatchThroughNativeAppKitControls` verifies that measured generation starts at zero, advances only after the real AppKit target/action or text callback mutates its model, and resets with the scenario.

`AppKitProductionScenarioAdapterTests.testMeasuredFeedScrollTracksOnlyUserDrivenBoundsChanges` verifies that programmatic clip-view movement does not update model state or generation, while the native wheel bounds-change path updates both, and reset restores both to zero.

`AppKitProductionScenarioAdapterTests.testMeasuredChatSelectionReplacementUsesTheNativeEditableMessage` applies the frozen prepend and append phases, proves `chat:append:16` is naturally visible without a test scroll, drives its native field editor through exact `0:6` selection and replacement, then verifies two measured generations, final model state, visible record mutation, and restoration of the frozen thread position. `AppKitProductionReferenceArchitectureTests.testEditableChatMessageFieldKeepsTheExactStaticRaster` requires byte-identical PNG output between the editable target and the established exact static text field. The full frozen checkpoint reducer remains the state/accessibility regression gate. The changed selection-replaced checkpoint requires a new headed screenshot capture before visual qualification; old pixels are not reused as evidence.

Run the focused suite with a bounded DerivedData path:

```sh
xcodebuild -project AppleComparison.xcodeproj -scheme AppKitComparison -derivedDataPath /private/tmp/oxide-xcui-tests -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO test -only-testing:AppKitComparisonUnitTests/MacOSTrustedInputControlTests
```

Remove the DerivedData directory after verification.

## Changelog

- 2026-07-21: added contract, executor, public-XCUI performer, session sequence, and dismissal coverage.
- 2026-07-21: added raw application event, full-command state transition, fail-closed unchanged-state, and AppKit native-callback generation coverage.
- 2026-07-21: added native feed wheel ingestion and programmatic-scroll suppression coverage.
- 2026-07-21: added naturally visible native editable chat selection/replacement, exact raster, and full frozen state/accessibility regression coverage; headed screenshot recapture remains required.
- 2026-07-21: added typed select/replace compilation, exact public-XCUI Home/Shift+Right/text dispatch checks, and composite raw mouse/key receipt coverage.
- 2026-07-21: added the preloaded-control-plane and deferred-receipt-persistence hot-path contamination regression test.
