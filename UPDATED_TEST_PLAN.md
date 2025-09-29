# Updated OxideUI Test Coverage Plan

## ✅ Completed Work

### Architectural Fix - Test Code Separation
- **Created** new `oxideui-test-scenes` crate at `/crates/test-scenes/`
- **Moved** all test scene code out of production library
- **Removed** `pub mod scenes` from `oxideui-ui-core`
- **Result**: Clean separation - test code no longer compiled into production library

### Phase 1 - Elements Extended Scene ✅
Successfully implemented test scene #11 covering:
- Badge with bounce animations (600ms custom timing)
- CountNode with animated values
- RecordButton with recording states (150ms press, 5s timeout)
- SlidingSwitch with drag gestures (3s inactivity timeout)
- ShiftingTextInput with focus/blur states

## 📋 Remaining Phases to Implement

### Phase 2: Animation Timings Test Scene
**Purpose**: Validate all configurable animation timings work correctly

**New Scene: "Animation Config"**
```rust
pub struct AnimationConfig {
    // Multiple instances of same component with different timings
    badges: Vec<(Badge, BadgeState)>, // 100ms, 450ms, 1000ms
    buttons: Vec<(Button, ButtonState)>, // 50ms, 100ms, 200ms
    toggles: Vec<(Toggle, ToggleState)>, // 100ms, 200ms, 400ms
    record_buttons: Vec<(RecordButton, RecordButtonState)>, // Various press/timeout combos
}
```

**Test Coverage**:
- Verify timing changes take effect immediately
- Test extreme values (1ms, 10000ms)
- Ensure no hardcoded timings remain
- Validate smooth animation at different speeds

### Phase 3: UI Orchestration Test Scene
**Purpose**: Test complex multi-component interactions and orchestration

**New Scene: "Orchestration"**
```rust
pub struct OrchestrationScene {
    scatter_orchestrator: ScatterOrchestrator,
    test_nodes: Vec<NodeId>,
    popup_window: PopupWindow,
    overlay: Overlay,
    overlay_state: OverlayState,
    // Test staggered animations
    stagger_items: Vec<NodeId>,
}
```

**Test Coverage**:
- ScatterOrchestrator full API:
  - `scatter_on()` / `scatter_off()`
  - `transition()` between node sets
  - `scatter_on_staggered()` with delays
  - Interaction depth tracking
- PopupWindow modal behavior
- Overlay state management
- Complex animation sequences

### Phase 4: Permissions UI Test Scene
**Purpose**: Test permission prompts and user interactions

**New Scene: "Permissions"**
```rust
pub struct PermissionsScene {
    permission_overlay: PermissionOverlayUi,
    test_states: Vec<PermissionState>,
    current_domain: PermissionDomain,
}
```

**Test Coverage**:
- All 8 permission domains (Camera, Microphone, Location, etc.)
- Request/Denied/Limited states
- Button interactions (Allow/Open Settings)
- Overlay rendering and positioning
- State transitions

### Phase 5: Integration Test Scene
**Purpose**: Test real-world UI flows and component interactions

**New Scene: "Integration"**
```rust
pub struct IntegrationScene {
    // Complex form with validation
    form_inputs: Vec<TextInput>,
    form_validation: FormValidation,

    // Media capture flow
    camera_view: UICameraView,
    record_button: RecordButton,
    capture_state: CaptureState,

    // Collection with animations
    collection: CollectionView,
    collection_items: Vec<CollectionItem>,
}
```

**Test Coverage**:
- Form validation workflow
- Camera capture flow
- Collection item animations
- State propagation between components
- Memory management under load

### Phase 6: Performance Stress Test Scene
**Purpose**: Validate performance with many animated elements

**New Scene: "Performance"**
```rust
pub struct PerformanceScene {
    // 100+ animated badges
    badges: Vec<(Badge, BadgeState)>,

    // Large collection with transitions
    large_collection: CollectionView,

    // Many simultaneous animations
    animating_nodes: Vec<NodeId>,

    // Performance metrics
    fps_counter: FpsCounter,
    memory_usage: MemoryMetrics,
}
```

**Test Coverage**:
- 100+ simultaneous animations
- Large collections (1000+ items)
- Memory leak detection
- Frame rate monitoring
- Draw call optimization

## 🔧 Implementation Details

### File Structure
```
crates/test-scenes/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Router and existing scenes
│   ├── animation_config.rs
│   ├── orchestration.rs
│   ├── permissions.rs
│   ├── integration.rs
│   └── performance.rs
```

### Platform Test App Integration
The test apps in `host/ios-app/` and `host/macos-app/` need to:
1. Add dependency on `oxideui-test-scenes`
2. Import and use `Router<U>` from test crate
3. Remove any direct references to `oxideui_ui_core::scenes`

### Testing Protocol
Each scene should:
1. Run for at least 60 seconds without crashes
2. Maintain 60 FPS (except performance stress test)
3. Have no memory leaks
4. Respond correctly to all interactions
5. Render correctly on both iOS and macOS

## 📊 Success Metrics

### Coverage Goals
- ✅ 100% of UI elements have test coverage
- ⏳ 100% of animation APIs tested
- ⏳ 100% of public methods exercised
- ⏳ All timing configurations validated
- ⏳ All interaction patterns tested

### Quality Goals
- Zero test code in production library ✅
- All tests pass on iOS simulator
- All tests pass on macOS
- No memory leaks detected
- No thread safety issues

## 🚀 Next Steps

1. **Immediate**: Update platform test apps to use new `oxideui-test-scenes` crate
2. **Phase 2**: Implement Animation Config scene (2-3 hours)
3. **Phase 3**: Implement Orchestration scene (2-3 hours)
4. **Phase 4**: Implement Permissions scene (1-2 hours)
5. **Phase 5**: Implement Integration scene (3-4 hours)
6. **Phase 6**: Implement Performance scene (2-3 hours)

**Total Estimated Time**: 10-15 hours

## Notes

- All test scenes are now properly isolated in `oxideui-test-scenes` crate
- Production library (`oxideui-ui-core`) contains zero test code
- Test scenes can evolve independently without affecting library size
- Platform apps can optionally include test scenes for debugging