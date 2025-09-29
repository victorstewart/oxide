# OxideUI Test Coverage Audit Report

## Executive Summary
After comprehensive analysis of the OxideUI library and headless test apps, I've identified significant gaps in test coverage for UI elements and animations. Many critical components added during our refactoring are not tested.

## Current Test Scenes (11 total)
Located in `/crates/ui-core/src/scenes.rs`:

1. **Controls** - Tests basic controls
   - ✅ Button (with ButtonState)
   - ✅ Toggle (with ToggleState)
   - ✅ Slider (with SliderState)
   - ✅ ProgressBar
   - ✅ Spinner
   - ✅ Label

2. **TextLayout** - Tests text rendering
   - ✅ Label with different alignments
   - ✅ Multi-language text (English, Arabic, Japanese, emoji)
   - ✅ Text wrapping

3. **ZoomImage** - Tests image interactions
   - ✅ ImageView
   - ✅ ImageZoomState (pinch, pan, double-tap)

4. **AnimTimeline** - Tests animations
   - ✅ Basic animation sequencing
   - ✅ Scatter animations (uses scatter helper)
   - ✅ Reduce motion support

5. **CollectionStress** - Tests collection view
   - ✅ CollectionView
   - ✅ Cell transitions (shrink_grow)
   - ✅ Focus navigation
   - ✅ Performance with many cells

6. **DamageLab** - Tests damage rect optimization
   - ✅ Partial redraw testing

7. **InputLab** - Tests input handling
   - ✅ TextInput (username/password fields)
   - ✅ TextInputState
   - ✅ Overlay animations (shrink_grow scale)
   - ✅ Picker (role selection)
   - ✅ IME handling
   - ✅ Keyboard events
   - ✅ Validation states

8. **NineSlice** - Tests nine-slice images
   - ✅ NineSliceImage

9. **SdfText** - Tests SDF text rendering
   - ✅ SDF font rendering

10. **Snapshot** - Tests render-to-texture
    - ✅ Readback functionality

11. **Camera** - Tests camera preview
    - ✅ UICameraView
    - ✅ Permission handling

## Missing Test Coverage

### 🔴 CRITICAL - Elements Not Tested At All

1. **Badge** (BadgeStyle, BadgeState)
   - bounce_duration_ms animation timing
   - Badge bounce animation
   - Count display

2. **CountNode**
   - Count value updates
   - Text rendering with count

3. **RecordButton** (RecordButtonStyle, RecordButtonState)
   - recording_timeout_ms timing
   - press_animation_ms timing
   - Recording state management
   - Pulse animations during recording
   - Press/release animations

4. **SlidingSwitch** (SlidingSwitchStyle, SlidingSwitchState)
   - inactive_timeout_ms timing
   - Sliding animations
   - Active/inactive state transitions
   - Auto-deactivation on timeout

5. **ShiftingTextInput** (ShiftingTextInputStyle, ShiftingTextInputState)
   - Shifting animations
   - Text input with shifting behavior
   - Validation states

6. **PopupWindow** (PopupStyle)
   - Popup display/dismiss
   - Modal behavior
   - Content rendering

7. **OverlayState** (OverlayStyle)
   - While Overlay animations are tested in InputLab, the standalone Overlay component isn't fully tested

### ⚠️ IMPORTANT - Incomplete Coverage

1. **Animation Timings**
   - New per-component animation timings not tested:
     - BadgeStyle::bounce_duration_ms
     - RecordButtonStyle::recording_timeout_ms
     - RecordButtonStyle::press_animation_ms
     - SlidingSwitchStyle::inactive_timeout_ms
     - ToggleStyle::animation_ms
     - ButtonStyle::press_animation_ms

2. **ScatterOrchestrator**
   - Only basic scatter animations tested
   - Missing: transition(), scatter_on_staggered()
   - Missing: interaction depth tracking

3. **Permissions UI**
   - PermissionOverlayUi not tested
   - Permission prompts and interactions

4. **Design System**
   - GeometricScale validation
   - ScreenScale validation
   - Invalid input handling

## Features Added in This Session Not Tested

From our refactoring work:

1. **Exponential Backoff** (networking)
   - Not UI, but should have integration tests

2. **URL Scheme Security**
   - Security validation logic needs testing

3. **Telemetry Rate Limiting**
   - Rate limiting behavior
   - Batch processing

4. **Animation Config Refactor**
   - All per-component animation timings
   - Dynamic timing configuration

## Implementation Plan

### Phase 1: Add Missing Element Tests (High Priority)

Create new test scene: **"Elements Extended"**
```rust
pub struct ElementsExtended {
    badge: Badge,
    badge_state: BadgeState,
    count_node: CountNode,
    record_button: RecordButton,
    record_button_state: RecordButtonState,
    sliding_switch: SlidingSwitch,
    sliding_switch_state: SlidingSwitchState,
    shifting_input: ShiftingTextInput,
    shifting_input_state: ShiftingTextInputState,
}
```

### Phase 2: Add Animation Timing Tests

Create new test scene: **"Animation Timings"**
- Test all configurable animation durations
- Validate timing changes take effect
- Test with different timing configurations

### Phase 3: Add Orchestration Tests

Create new test scene: **"UI Orchestration"**
```rust
pub struct UiOrchestration {
    scatter_orchestrator: ScatterOrchestrator,
    test_nodes: Vec<NodeId>,
    popup_window: PopupWindow,
    overlay: Overlay,
    overlay_state: OverlayState,
}
```

### Phase 4: Add Permissions UI Test

Enhance existing Camera scene or create new **"Permissions"** scene:
- Test PermissionOverlayUi
- Test all permission domains
- Test request/denied states
- Test button interactions

### Phase 5: Integration Tests

Create new test scene: **"Integration"**
- Test interactions between components
- Test state management across components
- Test complex UI flows

## File Changes Required

1. **crates/ui-core/src/scenes.rs**
   - Add 4-5 new test scenes
   - Add ~800-1000 lines of test code
   - Update SceneRouter with new scenes

2. **host/ios/OxideUI/TestViewController.swift** (if needed)
   - Update scene selection UI
   - Add new scene navigation

3. **host/macos/OxideUI/TestViewController.swift** (if needed)
   - Update scene selection UI
   - Add new scene navigation

## Validation Criteria

Each test scene must verify:
1. ✅ Visual rendering correctness
2. ✅ Animation timing accuracy
3. ✅ User interaction handling
4. ✅ State management
5. ✅ Performance characteristics
6. ✅ Memory management (no leaks)
7. ✅ Thread safety

## Priority Order

1. 🔴 **Critical**: Badge, RecordButton, SlidingSwitch (core UI elements)
2. 🔴 **Critical**: Animation timing tests (verify refactor works)
3. ⚠️ **Important**: CountNode, ShiftingTextInput
4. ⚠️ **Important**: ScatterOrchestrator full coverage
5. 🟡 **Nice to have**: PopupWindow, PermissionOverlayUi

## Estimated Effort

- **Total New Test Code**: ~1500-2000 lines
- **New Test Scenes**: 4-5 scenes
- **Development Time**: 4-6 hours
- **Testing Time**: 2-3 hours

## Next Steps

1. Implement ElementsExtended scene first (highest impact)
2. Add animation timing validation scene
3. Implement orchestration tests
4. Run full test suite on iOS/macOS targets
5. Validate all animations work with Metal renderer
6. Performance profiling with all tests running

## Conclusion

The headless test apps provide good coverage for basic UI elements but are missing tests for:
- 7 complete UI components
- All new animation timing configurations
- Complex orchestration patterns
- Permission UI flows

This gap represents a significant testing deficit that should be addressed before production deployment.