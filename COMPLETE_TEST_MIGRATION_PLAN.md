# Complete Test Code Migration & Coverage Plan

## 🚨 Current State: Test Code Locations

### ✅ Already Migrated
- **2,500+ lines** moved from `crates/ui-core/src/scenes.rs` → `crates/test-scenes/`
- Production library (`oxideui-ui-core`) no longer exports test scenes

### ❌ Still Using Old References
1. **iOS Host App** (`host/ios-app/oxideui-host-ios/src/lib.rs`)
   - 15+ references to `ui::scenes::Router`
   - Hardcoded `SceneKind` enum values
   - Direct dependency on ui-core for test scenes

2. **macOS Host App** (`host/macos-app/oxideui-host-macos/src/lib.rs`)
   - References to `ui::scenes::Router`
   - Needs migration to test-scenes crate

## 📋 Migration Tasks

### Task 1: Update Host App Dependencies
**Files to modify:**
- `/host/ios-app/oxideui-host-ios/Cargo.toml`
- `/host/macos-app/oxideui-host-macos/Cargo.toml`

**Changes:**
```toml
[dependencies]
# Add new dependency
oxideui-test-scenes = { path = "../../../crates/test-scenes" }
# Keep existing ui-core for production use
oxideui-ui-core = { path = "../../../crates/ui-core" }
```

### Task 2: Update iOS Host Imports
**File:** `/host/ios-app/oxideui-host-ios/src/lib.rs`

**Changes needed:**
```rust
// OLD
use oxideui_ui_core as ui;
// ... later ...
router: Option<ui::scenes::Router<MtlUploader>>,

// NEW
use oxideui_ui_core as ui;
use oxideui_test_scenes as test_scenes;
// ... later ...
router: Option<test_scenes::Router<MtlUploader>>,
```

**All occurrences to update:**
- Line 1074: `ui::scenes::Router` → `test_scenes::Router`
- Line 1227: `ui::scenes::Router` → `test_scenes::Router`
- Line 1275: `ui::scenes::Router::new` → `test_scenes::Router::new`
- Line 1431: `ui::scenes::CameraMetrics` → `test_scenes::CameraMetrics`
- Line 2051: `ui::scenes::Router::` → `test_scenes::Router::`
- Line 2056: `ui::scenes::Router::` → `test_scenes::Router::`
- Line 2181-2193: `ui::scenes::SceneKind` → `test_scenes::SceneKind`

### Task 3: Update macOS Host Imports
**File:** `/host/macos-app/oxideui-host-macos/src/lib.rs`

**Changes needed:**
```rust
// Add import
use oxideui_test_scenes as test_scenes;

// Update references
router: Option<test_scenes::Router<MtlUploader>>,
let mut router = test_scenes::Router::new(uploader);
```

### Task 4: Verify No Other Test Code in Production
**Check these locations:**
- `crates/*/src/` - Should contain NO test scenes, only production code
- `crates/*/tests/` - Unit tests are OK here
- `crates/*/examples/` - Example code is OK here

## 📊 Test Coverage Implementation Plan

### Phase 1: Infrastructure Migration ✅
- [x] Create `oxideui-test-scenes` crate
- [x] Move test code out of production
- [ ] Update iOS host app references
- [ ] Update macOS host app references
- [ ] Verify compilation

### Phase 2: Animation Timings Scene
**Location:** `crates/test-scenes/src/animation_config.rs`

```rust
pub struct AnimationConfigScene {
    // Test different timing configurations
    badges: Vec<(Badge, BadgeState, u32)>, // Various bounce_duration_ms
    buttons: Vec<(Button, ButtonState, u32)>, // Various press_animation_ms
    toggles: Vec<(Toggle, ToggleState, u32)>, // Various animation_ms

    // Runtime timing changes
    timing_slider: Slider,
    current_timing_ms: u32,
}
```

**Tests:**
- Dynamic timing updates
- Extreme values (1ms - 10000ms)
- Simultaneous different timings
- Smooth animation curves

### Phase 3: UI Orchestration Scene
**Location:** `crates/test-scenes/src/orchestration.rs`

```rust
pub struct OrchestrationScene {
    scatter_orchestrator: ScatterOrchestrator,

    // Test all scatter methods
    scatter_nodes: Vec<NodeId>,
    transition_old: Vec<NodeId>,
    transition_new: Vec<NodeId>,
    stagger_nodes: Vec<NodeId>,

    // Modal/Overlay testing
    popup_window: PopupWindow,
    overlay: Overlay,
    overlay_state: OverlayState,

    // Interaction depth
    nested_animations: Vec<AnimationLayer>,
}
```

**Tests:**
- `scatter_on()`, `scatter_off()`, `transition()`
- `scatter_on_staggered()` with various delays
- Interaction blocking during animations
- Nested animation management
- PopupWindow modal behavior

### Phase 4: Permissions UI Scene
**Location:** `crates/test-scenes/src/permissions.rs`

```rust
pub struct PermissionsScene {
    permission_overlay: PermissionOverlayUi,

    // Cycle through all domains
    test_domains: [PermissionDomain; 8],
    current_domain_index: usize,

    // Test all states
    test_states: BTreeMap<PermissionDomain, PermissionStatus>,

    // Interaction testing
    last_button_press: Option<PermissionDomain>,
}
```

**Tests:**
- All 8 permission domains
- NotDetermined → Authorized flow
- NotDetermined → Denied flow
- Limited state handling
- Button interactions
- Proper text/messaging

### Phase 5: Integration Scene
**Location:** `crates/test-scenes/src/integration.rs`

```rust
pub struct IntegrationScene {
    // Complex form workflow
    form: ComplexForm {
        text_inputs: Vec<TextInput>,
        validation_states: Vec<ValidationState>,
        submit_button: Button,
    },

    // Media capture flow
    capture_flow: CaptureFlow {
        camera_view: UICameraView,
        record_button: RecordButton,
        preview_state: PreviewState,
        share_button: Button,
    },

    // Collection with live updates
    live_collection: LiveCollection {
        view: CollectionView,
        items: Vec<DynamicItem>,
        update_timer: Timer,
    },
}
```

**Tests:**
- Multi-step form validation
- Camera → Record → Preview → Share flow
- Dynamic collection updates
- State synchronization
- Error recovery

### Phase 6: Performance Stress Scene
**Location:** `crates/test-scenes/src/performance.rs`

```rust
pub struct PerformanceScene {
    // Mass animation test
    animated_badges: Vec<(Badge, BadgeState)>, // 100+

    // Large collections
    huge_collection: CollectionView, // 1000+ items

    // Simultaneous effects
    scatter_nodes: Vec<NodeId>, // 50+ nodes
    active_animations: Vec<AnimationHandle>,

    // Metrics
    fps_counter: FpsCounter,
    frame_times: RingBuffer<f32>,
    memory_tracker: MemoryTracker,
    draw_call_counter: DrawCallCounter,
}
```

**Tests:**
- 100+ simultaneous badge animations
- 1000+ collection items with scrolling
- 50+ scatter animations
- Memory leak detection
- FPS stability (should maintain 60fps)
- Draw call batching efficiency

## 🔧 File Structure After Migration

```
oxideui/
├── crates/
│   ├── ui-core/              # Production library (NO test scenes)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── elements.rs
│   │       └── ...
│   │
│   └── test-scenes/           # ALL test scene code
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs         # Router + existing scenes
│           ├── animation_config.rs
│           ├── orchestration.rs
│           ├── permissions.rs
│           ├── integration.rs
│           └── performance.rs
│
└── host/
    ├── ios-app/
    │   └── oxideui-host-ios/
    │       ├── Cargo.toml     # Add test-scenes dependency
    │       └── src/lib.rs     # Update imports
    │
    └── macos-app/
        └── oxideui-host-macos/
            ├── Cargo.toml     # Add test-scenes dependency
            └── src/lib.rs     # Update imports
```

## 📈 Success Metrics

### Architecture Goals
- ✅ Zero test code in production crates
- ⏳ All host apps use test-scenes crate
- ⏳ Clean separation of concerns
- ⏳ Test scenes can be excluded from release builds

### Coverage Goals
- ✅ 100% of UI elements tested
- ⏳ 100% of animation APIs tested
- ⏳ 100% of orchestration patterns tested
- ⏳ All permission flows tested
- ⏳ Performance validated under stress

## 🚀 Implementation Order

### Immediate (Must do first)
1. Update iOS host app to use `oxideui-test-scenes`
2. Update macOS host app to use `oxideui-test-scenes`
3. Verify both apps compile and run

### Phase Implementation (10-15 hours total)
1. **Animation Config** (2-3 hours) - Test all timing systems
2. **Orchestration** (2-3 hours) - ScatterOrchestrator + modals
3. **Permissions** (1-2 hours) - All permission UI flows
4. **Integration** (3-4 hours) - Real-world workflows
5. **Performance** (2-3 hours) - Stress testing

## 🎯 Definition of Done

### For Migration
- [ ] iOS app compiles with test-scenes crate
- [ ] macOS app compiles with test-scenes crate
- [ ] All test scenes work in both apps
- [ ] No references to `ui::scenes` in host apps
- [ ] Production library has zero test code

### For Each Test Scene
- [ ] Scene implemented in test-scenes crate
- [ ] All target elements/APIs covered
- [ ] Runs on iOS without crashes
- [ ] Runs on macOS without crashes
- [ ] Maintains 60 FPS (except stress test)
- [ ] No memory leaks
- [ ] Interactive elements respond correctly

## Notes

- **IMPORTANT**: The `oxideui-test-scenes` crate should be marked as `[dev-dependencies]` for any production apps, so it's excluded from release builds
- Host apps are currently test/development apps, so direct dependency is OK
- Consider adding a feature flag to completely exclude test scenes from builds