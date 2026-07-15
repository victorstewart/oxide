use anyhow::{bail, Context, Result};
use oxide_harness_registry as registry;
use oxide_input::{GestureRecognizer, TouchSurfaceRecognizer};
use oxide_permissions as permissions;
use oxide_platform_api as platform;
use oxide_platform_api::Platform as _;
use oxide_platform_web as platform_web;
use oxide_renderer_api as api;
use oxide_renderer_api::Renderer;
use oxide_renderer_metal as metal;
use oxide_test_scenes as scenes;
use oxide_text as text;
use oxide_timing as timing;
use oxide_ui_core as ui;
use serde::{Deserialize, Serialize};
use std::boxed::Box;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::hint::black_box;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

pub mod paired;
mod architecture_matrix;

const DEFAULT_BASELINE_JSON: &str = "benchmarks/workspace/latest.json";
const DEFAULT_BASELINE_MARKDOWN: &str = "benchmarks/workspace/latest.md";
const DEFAULT_MARKDOWN_RENDER_BENCH_ITERS: usize = 256;
const DEFAULT_JSON_RENDER_BENCH_ITERS: usize = 256;
const DEFAULT_SAMPLE_SUMMARY_BENCH_ITERS: usize = 262_144;
const DEFAULT_CASE_FILTER_BENCH_ITERS: usize = 262_144;
const DEFAULT_COMPARE_REPORTS_BENCH_ITERS: usize = 16_384;
const DEFAULT_FRAME_PACING_METRICS_BENCH_ITERS: usize = 262_144;
const DEFAULT_DISTRIBUTION_METRICS_BENCH_ITERS: usize = 262_144;
const DEFAULT_CASE_METRIC_CONTRACT_BENCH_ITERS: usize = 16_384;
const DEFAULT_CONTRACT_COVERAGE_BENCH_ITERS: usize = 16_384;
const LINEAR_COMPARE_BASELINE_CASE_LIMIT: usize = 32;
const LATIN_FONT: &[u8] = include_bytes!("../../text/tests/fixtures/test_text_latin.ttf");
const CJK_FONT: &[u8] = include_bytes!("../../text/tests/fixtures/test_text_cjk.ttf");
const MACOS_HEBREW_FONT: &str = "/System/Library/Fonts/Supplemental/Arial Unicode.ttf";
const DAMAGE_USE_THRESH: f32 = 0.75;
const DAMAGE_PREFILTER_THRESH: f32 = 0.25;
const PERF_DEVICE_SCALE: f32 = 2.0;
const PERF_SCENE_W: u32 = 1200;
const PERF_SCENE_H: u32 = 800;
const PERF_RUNNER_FILTER_ENV: &str = "OXIDE_PERF_RUNNER_FILTER";
static PERF_CASE_FILTERS: OnceLock<Vec<String>> = OnceLock::new();

struct ScenePerfSpec {
    slug: &'static str,
    name: &'static str,
    index: usize,
}

struct JourneyPerfSpec {
    id: &'static str,
    name: &'static str,
}

struct AuthoringPerfSpec {
    id: &'static str,
    name: &'static str,
}

struct NamedPerfSpec {
    id: &'static str,
    name: &'static str,
}

struct LaunchPerfSpec {
    id: &'static str,
    name: &'static str,
}

#[derive(Clone, Copy)]
enum PrimitiveLifecycleKind {
    EmptyRoot,
    FlatRects,
    Labels,
    Cards,
    Images,
    ControlSet,
}

#[derive(Clone, Copy)]
enum PrimitiveLifecycleOp {
    Mount,
    Mutate,
    RemoveAll,
    Remount,
}

struct PrimitiveLifecycleSpec {
    id: &'static str,
    name: &'static str,
    kind: PrimitiveLifecycleKind,
    count: usize,
    op: PrimitiveLifecycleOp,
}

impl PrimitiveLifecycleOp {
    fn label(self) -> &'static str {
        match self {
            Self::Mount => "mount",
            Self::Mutate => "mutate",
            Self::RemoveAll => "remove-all",
            Self::Remount => "remount",
        }
    }
}

struct BridgePerfSpec {
    id: &'static str,
    name: &'static str,
}

fn perf_case_filters() -> &'static [String] {
    PERF_CASE_FILTERS.get_or_init(load_perf_case_filters).as_slice()
}

fn load_perf_case_filters() -> Vec<String> {
    let Some(value) = std::env::var(PERF_RUNNER_FILTER_ENV).ok() else {
        return Vec::new();
    };
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from)
        .collect::<Vec<_>>()
}

fn perf_case_allowed(case_id: &str) -> bool {
    let filters = perf_case_filters();
    filters.is_empty()
        || filters.iter().any(|filter| case_id.starts_with(filter))
}

fn perf_case_prefix_allowed(prefix: &str) -> bool {
    let filters = perf_case_filters();
    filters.is_empty()
        || filters.iter().any(|filter| filter.starts_with(prefix) || prefix.starts_with(filter))
}

fn set_env_if_unset(name: &str, value: &str) {
    if std::env::var_os(name).is_none() {
        unsafe {
            std::env::set_var(name, value);
        }
    }
}

fn env_bool(name: &str, default: bool) -> bool {
    let Some(value) = std::env::var(name).ok() else {
        return default;
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

fn env_f32(name: &str, default: f32) -> f32 {
    std::env::var(name).ok().and_then(|value| value.trim().parse::<f32>().ok()).unwrap_or(default)
}

const PERF_SCENE_SPECS: &[ScenePerfSpec] = &[
    ScenePerfSpec { slug: "controls", name: "Controls", index: 0 },
    ScenePerfSpec { slug: "text_layout", name: "Text Layout", index: 1 },
    ScenePerfSpec { slug: "zoom_image", name: "Zoom Image", index: 2 },
    ScenePerfSpec { slug: "anim_timeline", name: "Animations", index: 3 },
    ScenePerfSpec { slug: "collection", name: "Collection Stress", index: 4 },
    ScenePerfSpec { slug: "damage_lab", name: "Damage Lab", index: 5 },
    ScenePerfSpec { slug: "input_lab", name: "Input & Haptics", index: 6 },
    ScenePerfSpec { slug: "nine_slice", name: "Nine Slice", index: 7 },
    ScenePerfSpec { slug: "sdf_text", name: "SDF Text", index: 8 },
    ScenePerfSpec { slug: "snapshot", name: "Snapshot", index: 9 },
    ScenePerfSpec { slug: "camera", name: "Camera", index: 10 },
    ScenePerfSpec { slug: "elements_extended", name: "Elements Extended", index: 11 },
    ScenePerfSpec { slug: "animation_config", name: "Animation Timings", index: 12 },
    ScenePerfSpec { slug: "orchestration", name: "UI Orchestration", index: 13 },
    ScenePerfSpec { slug: "permissions", name: "Permissions", index: 14 },
    ScenePerfSpec { slug: "integration", name: "Integration", index: 15 },
    ScenePerfSpec { slug: "stress", name: "Stress Test", index: 16 },
];

const PERF_JOURNEY_SPECS: &[JourneyPerfSpec] = &[
    JourneyPerfSpec { id: "cpu.journey.input_form_submit", name: "Input Form Submit" },
    JourneyPerfSpec {
        id: "cpu.journey.text_ime_composition_cycle",
        name: "Text IME Composition Cycle",
    },
    JourneyPerfSpec { id: "cpu.journey.collection_navigation", name: "Collection Navigation" },
    JourneyPerfSpec {
        id: "cpu.journey.zoom_image_gesture_cycle",
        name: "Zoom Image Gesture Cycle",
    },
    JourneyPerfSpec {
        id: "cpu.journey.orchestration_transition_modal",
        name: "Orchestration Transition + Modal",
    },
    JourneyPerfSpec { id: "cpu.journey.feed_scroll_matrix", name: "Feed Scroll Matrix" },
    JourneyPerfSpec {
        id: "cpu.journey.thumbnail_grid_scroll_matrix",
        name: "Thumbnail Grid Scroll Matrix",
    },
    JourneyPerfSpec {
        id: "cpu.journey.chat_thread_scroll_matrix",
        name: "Chat Thread Scroll Matrix",
    },
];

const PERF_GPU_JOURNEY_SPECS: &[NamedPerfSpec] = &[NamedPerfSpec {
    id: "gpu.journey.collection_navigation.frame_pacing",
    name: "Collection Navigation Frame Pacing",
}];

const POPUP_WHEEL_PICKER_CASE_ID: &str = "cpu.authoring.popup_wheel_picker.interaction";

const PERF_AUTHORING_SPECS: &[AuthoringPerfSpec] = &[
    AuthoringPerfSpec { id: "cpu.authoring.text_fields.edit_cycle", name: "Text Fields" },
    AuthoringPerfSpec { id: POPUP_WHEEL_PICKER_CASE_ID, name: "Popup Wheel Picker" },
    AuthoringPerfSpec { id: "cpu.authoring.burst_emitter.sample", name: "Burst Emitter" },
    AuthoringPerfSpec {
        id: "cpu.authoring.surface_router.compose",
        name: "Surface Router Composition",
    },
    AuthoringPerfSpec {
        id: "cpu.authoring.surface_retained.clean_encode",
        name: "Surface Retained Clean Encode",
    },
    AuthoringPerfSpec {
        id: "cpu.authoring.surface_retained.dirty_leaf_encode",
        name: "Surface Retained Dirty Leaf Encode",
    },
    AuthoringPerfSpec {
        id: "cpu.authoring.surface_retained.text_atlas_context",
        name: "Surface Retained Text Atlas Context",
    },
    AuthoringPerfSpec {
        id: "cpu.authoring.surface_retained.cache_policy",
        name: "Surface Retained Cache Policy",
    },
    AuthoringPerfSpec {
        id: "cpu.authoring.animation.dynamic_properties_300",
        name: "Dynamic Property Animation",
    },
    AuthoringPerfSpec {
        id: "cpu.authoring.retained_snapshot.spatial_query_10000",
        name: "Retained Snapshot Spatial Query",
    },
    AuthoringPerfSpec {
        id: "cpu.authoring.surface.retained_damage_dirty_leaf_10000",
        name: "Retained Surface Exact Damage",
    },
    AuthoringPerfSpec {
        id: "cpu.authoring.drawlist_text_replay.multi_atlas",
        name: "DrawList Text Replay Multi Atlas",
    },
    AuthoringPerfSpec {
        id: "cpu.authoring.collection_key_reconcile.indexed",
        name: "Collection Key Reconcile Indexed",
    },
    AuthoringPerfSpec {
        id: "cpu.authoring.collection_key_reconcile.scan",
        name: "Collection Key Reconcile Scan",
    },
    AuthoringPerfSpec {
        id: "cpu.authoring.collection_measure_cache.bounded_churn",
        name: "Collection Measure Cache Bounded Churn",
    },
    AuthoringPerfSpec {
        id: "cpu.authoring.collection_prefix_update.incremental",
        name: "Collection Prefix Update Incremental",
    },
    AuthoringPerfSpec {
        id: "cpu.authoring.collection_prefix_update.full_scan",
        name: "Collection Prefix Update Full Scan",
    },
    AuthoringPerfSpec {
        id: "gpu.authoring.retained_snapshot.clean_mixed",
        name: "Retained Snapshot Metal Replay",
    },
    AuthoringPerfSpec {
        id: "gpu.authoring.retained_snapshot.spatial_damage_10000",
        name: "Retained Snapshot Spatial Damage",
    },
    AuthoringPerfSpec { id: "gpu.authoring.scene3d.mixed_frame", name: "Scene3D Mixed Frame" },
    AuthoringPerfSpec {
        id: "gpu.authoring.image_store.atlas_grid_1000",
        name: "Image Store Atlas Grid",
    },
];

const PERF_LAUNCH_SPECS: &[LaunchPerfSpec] = &[
    LaunchPerfSpec { id: "cpu.launch.simple_home.cold_launch", name: "Simple Home Cold Launch" },
    LaunchPerfSpec { id: "cpu.launch.heavy_home.cold_launch", name: "Heavy Home Cold Launch" },
    LaunchPerfSpec { id: "cpu.launch.detail.deep_link_launch", name: "Detail Deep Link Launch" },
    LaunchPerfSpec { id: "cpu.launch.simple_home.warm_resume", name: "Simple Home Warm Resume" },
    LaunchPerfSpec {
        id: "cpu.launch.heavy_home.foreground_after_background",
        name: "Heavy Home Foreground After Background",
    },
];

const PERF_LAYOUT_SPECS: &[NamedPerfSpec] = &[
    NamedPerfSpec {
        id: "cpu.layout.flat_grid.rotation_relayout",
        name: "Flat Grid Rotation Relayout",
    },
    NamedPerfSpec { id: "cpu.layout.deep_stack.theme_swap", name: "Deep Stack Theme Swap" },
    NamedPerfSpec { id: "cpu.layout.grid.safe_area_swap", name: "Grid Safe Area Swap" },
    NamedPerfSpec {
        id: "cpu.layout.dirty_subtree.incremental_relayout",
        name: "Dirty Subtree Incremental Relayout",
    },
    NamedPerfSpec {
        id: "cpu.layout.descendant_only.incremental_relayout",
        name: "Descendant-Only Incremental Relayout",
    },
    NamedPerfSpec { id: "cpu.layout.transform_only.reposition", name: "Transform-Only Reposition" },
    NamedPerfSpec { id: "cpu.layout.paint_only.opacity_clip", name: "Paint-Only Opacity Clip" },
    NamedPerfSpec {
        id: "cpu.layout.node_content_dirty.retained_replay",
        name: "Node Content Dirty Retained Replay",
    },
    NamedPerfSpec {
        id: "cpu.layout.non_draw_dirty.retained_reuse",
        name: "Non-Draw Dirty Retained Reuse",
    },
    NamedPerfSpec {
        id: "cpu.layout.scoped_tree_mutation.add_remove",
        name: "Scoped Tree Mutation Add Remove",
    },
];

const PERF_TEXT_INPUT_SPECS: &[NamedPerfSpec] = &[
    NamedPerfSpec {
        id: "cpu.text_input.large_editor.keystroke_burst",
        name: "Large Editor Keystroke Burst",
    },
    NamedPerfSpec { id: "cpu.text_input.large_editor.paste_10kb", name: "Large Editor Paste 10KB" },
    NamedPerfSpec {
        id: "cpu.text_input.large_editor.selection_replace",
        name: "Large Editor Selection Replace",
    },
    NamedPerfSpec {
        id: "cpu.text_input.ime.composition_commit_cycle",
        name: "IME Composition Commit Cycle",
    },
    NamedPerfSpec { id: "cpu.text_input.cursor_pick.cluster_map", name: "Cursor Pick Cluster Map" },
    NamedPerfSpec {
        id: "cpu.text_input.cursor_pick.rtl_cluster_map",
        name: "RTL Cursor Pick Cluster Map",
    },
    NamedPerfSpec {
        id: "cpu.text_input.cursor_pick.fallback_cluster_map",
        name: "Fallback Font Cursor Pick Cluster Map",
    },
    NamedPerfSpec {
        id: "cpu.text_input.cursor_pick.mixed_bidi_affinity",
        name: "Mixed Bidi Cursor Pick Affinity",
    },
];

const PERF_IMAGE_PIPELINE_SPECS: &[NamedPerfSpec] = &[
    NamedPerfSpec { id: "cpu.image_pipeline.png.decode", name: "PNG Decode" },
    NamedPerfSpec { id: "gpu.image_pipeline.png.upload", name: "PNG Upload" },
    NamedPerfSpec { id: "gpu.image_pipeline.png.first_visible", name: "PNG First Visible" },
];

const PERF_NAVIGATION_SPECS: &[NamedPerfSpec] = &[
    NamedPerfSpec { id: "cpu.navigation.button_press.response", name: "Button Press Response" },
    NamedPerfSpec { id: "cpu.navigation.slider_scrub.response", name: "Slider Scrub Response" },
    NamedPerfSpec { id: "cpu.navigation.text_focus.response", name: "Text Focus Response" },
];

const PERF_RECONCILE_SPECS: &[NamedPerfSpec] = &[
    NamedPerfSpec { id: "cpu.reconcile.single_node_mutation", name: "Single Node Mutation" },
    NamedPerfSpec { id: "cpu.reconcile.tree_mutation_1pct", name: "Tree Mutation 1Pct" },
    NamedPerfSpec { id: "cpu.reconcile.tree_mutation_10pct", name: "Tree Mutation 10Pct" },
    NamedPerfSpec { id: "cpu.reconcile.theme_swap_full", name: "Theme Swap Full" },
];

const PERF_ENDURANCE_SPECS: &[NamedPerfSpec] = &[
    NamedPerfSpec {
        id: "cpu.endurance.open_close_heavy_screen.100x",
        name: "Open Close Heavy Screen 100x",
    },
    NamedPerfSpec { id: "cpu.endurance.tab_switch_heavy.500x", name: "Tab Switch Heavy 500x" },
    NamedPerfSpec {
        id: "cpu.endurance.idle_animation.600_frames",
        name: "Idle Animation 600 Frames",
    },
];

const PERF_STRESS_SPECS: &[NamedPerfSpec] = &[
    NamedPerfSpec { id: "cpu.stress.flat_rects.10000.mount", name: "Flat Rects 10k Mount" },
    NamedPerfSpec {
        id: "cpu.stress.simultaneous_animations.300",
        name: "Simultaneous Animations 300",
    },
    NamedPerfSpec { id: "cpu.stress.ticker_100hz", name: "Ticker 100 Hz" },
];

const PERF_PRIMITIVE_LIFECYCLE_SPECS: &[PrimitiveLifecycleSpec] = &[
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.empty_root.mount",
        name: "Empty Root Mount",
        kind: PrimitiveLifecycleKind::EmptyRoot,
        count: 0,
        op: PrimitiveLifecycleOp::Mount,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.flat_rects.10.mount",
        name: "Flat Rects 10 Mount",
        kind: PrimitiveLifecycleKind::FlatRects,
        count: 10,
        op: PrimitiveLifecycleOp::Mount,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.flat_rects.100.mount",
        name: "Flat Rects 100 Mount",
        kind: PrimitiveLifecycleKind::FlatRects,
        count: 100,
        op: PrimitiveLifecycleOp::Mount,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.flat_rects.1000.mount",
        name: "Flat Rects 1000 Mount",
        kind: PrimitiveLifecycleKind::FlatRects,
        count: 1_000,
        op: PrimitiveLifecycleOp::Mount,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.flat_rects.10.mutate_fill",
        name: "Flat Rects 10 Mutate Fill",
        kind: PrimitiveLifecycleKind::FlatRects,
        count: 10,
        op: PrimitiveLifecycleOp::Mutate,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.flat_rects.100.mutate_fill",
        name: "Flat Rects 100 Mutate Fill",
        kind: PrimitiveLifecycleKind::FlatRects,
        count: 100,
        op: PrimitiveLifecycleOp::Mutate,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.flat_rects.1000.mutate_fill",
        name: "Flat Rects 1000 Mutate Fill",
        kind: PrimitiveLifecycleKind::FlatRects,
        count: 1_000,
        op: PrimitiveLifecycleOp::Mutate,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.flat_rects.100.remove_all",
        name: "Flat Rects 100 Remove All",
        kind: PrimitiveLifecycleKind::FlatRects,
        count: 100,
        op: PrimitiveLifecycleOp::RemoveAll,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.flat_rects.100.remount",
        name: "Flat Rects 100 Remount",
        kind: PrimitiveLifecycleKind::FlatRects,
        count: 100,
        op: PrimitiveLifecycleOp::Remount,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.labels.10.mount",
        name: "Labels 10 Mount",
        kind: PrimitiveLifecycleKind::Labels,
        count: 10,
        op: PrimitiveLifecycleOp::Mount,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.labels.100.mount",
        name: "Labels 100 Mount",
        kind: PrimitiveLifecycleKind::Labels,
        count: 100,
        op: PrimitiveLifecycleOp::Mount,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.labels.1000.mount",
        name: "Labels 1000 Mount",
        kind: PrimitiveLifecycleKind::Labels,
        count: 1_000,
        op: PrimitiveLifecycleOp::Mount,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.labels.10.mutate_text",
        name: "Labels 10 Mutate Text",
        kind: PrimitiveLifecycleKind::Labels,
        count: 10,
        op: PrimitiveLifecycleOp::Mutate,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.labels.100.mutate_text",
        name: "Labels 100 Mutate Text",
        kind: PrimitiveLifecycleKind::Labels,
        count: 100,
        op: PrimitiveLifecycleOp::Mutate,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.labels.1000.mutate_text",
        name: "Labels 1000 Mutate Text",
        kind: PrimitiveLifecycleKind::Labels,
        count: 1_000,
        op: PrimitiveLifecycleOp::Mutate,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.cards.10.mount",
        name: "Cards 10 Mount",
        kind: PrimitiveLifecycleKind::Cards,
        count: 10,
        op: PrimitiveLifecycleOp::Mount,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.cards.100.mount",
        name: "Cards 100 Mount",
        kind: PrimitiveLifecycleKind::Cards,
        count: 100,
        op: PrimitiveLifecycleOp::Mount,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.cards.10.mutate_palette",
        name: "Cards 10 Mutate Palette",
        kind: PrimitiveLifecycleKind::Cards,
        count: 10,
        op: PrimitiveLifecycleOp::Mutate,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.cards.100.mutate_palette",
        name: "Cards 100 Mutate Palette",
        kind: PrimitiveLifecycleKind::Cards,
        count: 100,
        op: PrimitiveLifecycleOp::Mutate,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.images.10.mount",
        name: "Images 10 Mount",
        kind: PrimitiveLifecycleKind::Images,
        count: 10,
        op: PrimitiveLifecycleOp::Mount,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.images.100.mount",
        name: "Images 100 Mount",
        kind: PrimitiveLifecycleKind::Images,
        count: 100,
        op: PrimitiveLifecycleOp::Mount,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.images.10.mutate_alpha",
        name: "Images 10 Mutate Alpha",
        kind: PrimitiveLifecycleKind::Images,
        count: 10,
        op: PrimitiveLifecycleOp::Mutate,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.images.100.mutate_alpha",
        name: "Images 100 Mutate Alpha",
        kind: PrimitiveLifecycleKind::Images,
        count: 100,
        op: PrimitiveLifecycleOp::Mutate,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.control_set.mount",
        name: "Control Set Mount",
        kind: PrimitiveLifecycleKind::ControlSet,
        count: 1,
        op: PrimitiveLifecycleOp::Mount,
    },
    PrimitiveLifecycleSpec {
        id: "cpu.primitive.control_set.mutate_state",
        name: "Control Set Mutate State",
        kind: PrimitiveLifecycleKind::ControlSet,
        count: 1,
        op: PrimitiveLifecycleOp::Mutate,
    },
];

const PERF_BRIDGE_SPECS: &[BridgePerfSpec] = &[
    BridgePerfSpec {
        id: "cpu.bridge.permission_callback_fanout",
        name: "Permission Callback Fanout",
    },
    BridgePerfSpec { id: "cpu.bridge.web_backend_surface", name: "Web Backend Surface" },
    BridgePerfSpec { id: "cpu.bridge.sensor_location_snapshot", name: "Sensor Location Snapshot" },
    BridgePerfSpec { id: "cpu.bridge.bluetooth_cache_update", name: "Bluetooth Cache Update" },
    BridgePerfSpec { id: "cpu.bridge.photo_import_thumbnail", name: "Photo Import Thumbnail" },
    BridgePerfSpec { id: "cpu.bridge.file_import_render", name: "File Import Render" },
    BridgePerfSpec { id: "cpu.bridge.share_payload_prepare", name: "Share Payload Prepare" },
    BridgePerfSpec {
        id: "cpu.bridge.local_json_transport_render",
        name: "Local JSON Transport Render",
    },
    BridgePerfSpec {
        id: "cpu.bridge.local_image_transport_render",
        name: "Local Image Transport Render",
    },
];

#[derive(Debug, Clone, Default)]
struct Cli {
    run_suite: bool,
    smoke: bool,
    compare: Option<PathBuf>,
    json_out: Option<PathBuf>,
    markdown_out: Option<PathBuf>,
    paired_run: Option<PathBuf>,
    paired_analyze: Option<PathBuf>,
    paired_json_out: Option<PathBuf>,
    paired_create_instrumentation_patch: Option<PathBuf>,
    paired_instrumentation_root: Option<PathBuf>,
    paired_instrumentation_paths: Vec<PathBuf>,
    markdown_bench_report: Option<PathBuf>,
    markdown_write_bench_report: Option<PathBuf>,
    markdown_bench_compare: Option<PathBuf>,
    markdown_bench_iters: usize,
    json_bench_report: Option<PathBuf>,
    json_string_bench_report: Option<PathBuf>,
    json_bench_iters: usize,
    sample_summary_bench: bool,
    sample_summary_bench_iters: usize,
    case_filter_bench: bool,
    case_filter_bench_iters: usize,
    compare_bench_current: Option<PathBuf>,
    compare_bench_baseline: Option<PathBuf>,
    compare_bench_iters: usize,
    frame_pacing_metrics_bench: bool,
    frame_pacing_metrics_bench_iters: usize,
    distribution_metrics_bench: bool,
    distribution_metrics_bench_iters: usize,
    case_metric_contract_bench_report: Option<PathBuf>,
    case_metric_contract_bench_iters: usize,
    contract_coverage_bench_report: Option<PathBuf>,
    contract_coverage_bench_iters: usize,
    write_baseline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerfReport {
    pub version: u32,
    pub suite: String,
    pub generated_label: Option<String>,
    pub cases: Vec<PerfCaseResult>,
    pub coverage: CoverageReport,
    #[serde(default)]
    pub contract: ContractCoverageReport,
    pub findings: Vec<AuditFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PerfCaseResult {
    pub id: String,
    pub family: String,
    pub layer: String,
    pub scenario: String,
    pub variant: String,
    pub cache_state: String,
    pub refresh_mode: String,
    pub unit: String,
    pub gated: bool,
    pub threshold_pct: f64,
    pub median: f64,
    pub p95: f64,
    pub p99: f64,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub samples: usize,
    pub ops_per_sample: u64,
    pub notes: Vec<String>,
    pub metrics: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CoverageReport {
    pub components_total: usize,
    pub components_covered: Vec<String>,
    pub animations_total: usize,
    pub animations_covered: Vec<String>,
    pub launch_total: usize,
    pub launch_covered: Vec<String>,
    pub primitive_lifecycle_total: usize,
    pub primitive_lifecycle_covered: Vec<String>,
    pub scenes_cpu_total: usize,
    pub scenes_cpu_covered: Vec<String>,
    pub scenes_gpu_total: usize,
    pub scenes_gpu_covered: Vec<String>,
    pub journeys_total: usize,
    pub journeys_covered: Vec<String>,
    pub authoring_total: usize,
    pub authoring_covered: Vec<String>,
    pub layout_total: usize,
    pub layout_covered: Vec<String>,
    pub text_input_total: usize,
    pub text_input_covered: Vec<String>,
    pub image_pipeline_total: usize,
    pub image_pipeline_covered: Vec<String>,
    pub navigation_total: usize,
    pub navigation_covered: Vec<String>,
    pub reconcile_total: usize,
    pub reconcile_covered: Vec<String>,
    pub endurance_total: usize,
    pub endurance_covered: Vec<String>,
    pub stress_total: usize,
    pub stress_covered: Vec<String>,
    pub bridges_total: usize,
    pub bridges_covered: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ContractCoverageReport {
    pub layers: Vec<ContractCoverageEntry>,
    pub battery: Vec<ContractCoverageEntry>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ContractCoverageEntry {
    pub id: String,
    pub label: String,
    pub status: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditFinding {
    pub status: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PerfComparison {
    pub matched: usize,
    pub missing_baseline: Vec<String>,
    pub regressions: Vec<PerfRegression>,
    pub improvements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerfRegression {
    pub id: String,
    pub baseline_median: f64,
    pub current_median: f64,
    pub allowed_median: f64,
    pub delta_pct: f64,
}

#[derive(Debug, Clone, Copy)]
struct SampleSummary {
    min: f64,
    max: f64,
    mean: f64,
    median: f64,
    p95: f64,
    p99: f64,
}

#[derive(Debug, Clone, Copy)]
struct DistributionMetricSummary
{
   max: f64,
   median: f64,
   p95: f64,
   p99: f64,
}

#[derive(Default)]
struct TextAtlasUploadStats {
    checksum: u64,
    creates: u64,
    updates: u64,
    dirty_upload_pixels: u64,
    full_upload_pixels: u64,
    max_update_pixels: u64,
    row_bytes: u64,
    glyph_runs: u64,
    vertices: u64,
    indices: u64,
}

#[derive(Default)]
struct TextAtlasPressureStats {
    checksum: u64,
    shape_count: u64,
    rendered_runs: u64,
    evictions: u64,
    resident_glyphs: u64,
    revision: u64,
    dirty_rects: u64,
    dirty_pixels: u64,
    max_dirty_pixels: u64,
    vertices: u64,
    indices: u64,
}

#[derive(Default)]
struct TextFallbackLabelStats {
    checksum: u64,
    glyph_runs: u64,
    vertices: u64,
    indices: u64,
    atlas_revision: u64,
}

#[derive(Default)]
struct TextPrefixWidthMapStats {
    checksum: u64,
    text_bytes: u64,
    prefix_boundaries: u64,
    width_entries: u64,
    shaped_runs: u64,
}

#[derive(Default)]
struct TextCursorMapStats {
    cursor_count: u64,
    byte_boundaries: u64,
    boundary_checksum: u64,
    affinity_splits: u64,
    width_span: f64,
}

#[derive(Default)]
struct CpuUploader {
    next: u32,
}

impl ui::elements::ImageUploader for CpuUploader {
    fn create_a8(&mut self, _w: u32, _h: u32, _data: &[u8], _row_bytes: usize) -> api::ImageHandle {
        self.next = self.next.saturating_add(1).max(1);
        api::ImageHandle(self.next)
    }

    fn update_a8(
        &mut self,
        _handle: api::ImageHandle,
        _x: u32,
        _y: u32,
        _w: u32,
        _h: u32,
        _data: &[u8],
        _row_bytes: usize,
    ) {
    }
}

#[derive(Default)]
struct CountingTextUploader {
    next: u32,
    stats: TextAtlasUploadStats,
}

impl ui::elements::ImageUploader for CountingTextUploader {
    fn create_a8(&mut self, w: u32, h: u32, _data: &[u8], row_bytes: usize) -> api::ImageHandle {
        self.next = self.next.saturating_add(1).max(1);
        self.stats.creates = self.stats.creates.saturating_add(1);
        self.stats.full_upload_pixels =
            self.stats.full_upload_pixels.saturating_add(w as u64 * h as u64);
        self.stats.row_bytes = row_bytes as u64;
        api::ImageHandle(self.next)
    }

    fn update_a8(
        &mut self,
        _handle: api::ImageHandle,
        _x: u32,
        _y: u32,
        w: u32,
        h: u32,
        _data: &[u8],
        row_bytes: usize,
    ) {
        let pixels = w as u64 * h as u64;
        self.stats.updates = self.stats.updates.saturating_add(1);
        self.stats.dirty_upload_pixels = self.stats.dirty_upload_pixels.saturating_add(pixels);
        self.stats.max_update_pixels = self.stats.max_update_pixels.max(pixels);
        self.stats.row_bytes = row_bytes as u64;
    }
}

struct MetalUploader {
    renderer: *mut metal::MetalRenderer,
}

unsafe impl Send for MetalUploader {}
unsafe impl Sync for MetalUploader {}

impl ui::elements::ImageUploader for MetalUploader {
    fn create_a8(&mut self, w: u32, h: u32, data: &[u8], row_bytes: usize) -> api::ImageHandle {
        unsafe { (*self.renderer).image_create_a8(w, h, data, row_bytes) }
    }

    fn update_a8(
        &mut self,
        handle: api::ImageHandle,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        data: &[u8],
        row_bytes: usize,
    ) {
        unsafe { (*self.renderer).image_update_a8(handle, x, y, w, h, data, row_bytes) }
    }

    fn append_a8(
        &mut self,
        handle: api::ImageHandle,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        data: &[u8],
        row_bytes: usize,
    ) {
        unsafe { (*self.renderer).image_append_a8(handle, x, y, w, h, data, row_bytes) }
    }

    fn release_a8(&mut self, handle: api::ImageHandle) {
        unsafe { (*self.renderer).image_release(handle) }
    }
}

struct GridMeasure;
struct GridRender;
struct FeedMeasure;
struct FeedRender;
struct ChatMeasure;
struct ChatRender;
struct KeyReconcileRender;

struct CollectionMeasureCacheChurnStats {
    checksum: u64,
    initial_measure_calls: u64,
    repair_measure_calls: u64,
    repair_draw_items: u64,
    content_h: u64,
}

struct CollectionMeasureCacheChurnMeasure {
    calls: u64,
}

struct CountingCollectionMeasure<M> {
    inner: M,
    calls: Arc<AtomicU64>,
    revision_queries: Arc<AtomicU64>,
}

impl<M: ui::collection::Measure> ui::collection::Measure for CountingCollectionMeasure<M> {
    fn measure(&mut self, index: usize, constraint: f32) -> f32 {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.measure(index, constraint)
    }

    fn item_key(&self, index: usize) -> ui::collection::ItemKey {
        self.inner.item_key(index)
    }

    fn item_index_for_key(&self, key: ui::collection::ItemKey) -> Option<usize> {
        self.inner.item_index_for_key(key)
    }

    fn item_revision(&self, index: usize) -> u64 {
        self.revision_queries.fetch_add(1, Ordering::Relaxed);
        self.inner.item_revision(index)
    }

    fn collection_revision(&self) -> Option<u64> {
        self.inner.collection_revision()
    }

    fn changed_item_range(&self) -> Option<core::ops::Range<usize>> {
        self.inner.changed_item_range()
    }

    fn fixed_extent(&self, constraint: f32) -> Option<f32> {
        self.inner.fixed_extent(constraint)
    }
}

impl ui::collection::Measure for GridMeasure {
    fn measure(&mut self, index: usize, constraint: f32) -> f32 {
        let wobble = (index % 7) as f32 * 2.0;
        (constraint * 0.55 + wobble).max(24.0)
    }

    fn collection_revision(&self) -> Option<u64> {
        Some(1)
    }
}

impl ui::collection::Measure for CollectionMeasureCacheChurnMeasure {
    fn measure(&mut self, index: usize, constraint: f32) -> f32 {
        self.calls = self.calls.saturating_add(1);
        let wobble = (index % 7) as f32 * 2.0;
        (constraint * 0.55 + wobble).max(24.0)
    }

    fn collection_revision(&self) -> Option<u64> {
        Some(1)
    }
}

impl ui::collection::CellRenderer for GridRender {
    fn render(
        &mut self,
        _cell_id: u32,
        index: usize,
        rect: api::RectF,
        _focused: bool,
        _hovered: bool,
        builder: &mut ui::DrawListBuilder,
    ) {
        let base = 0.9 - ((index % 5) as f32) * 0.06;
        builder.rrect(rect, [4.0; 4], api::Color::rgba(base, 0.3, 1.0 - base * 0.4, 1.0));
    }
}

impl ui::collection::Measure for FeedMeasure {
    fn measure(&mut self, index: usize, constraint: f32) -> f32 {
        let wobble = (index % 5) as f32 * 9.0;
        (constraint * 0.34 + 72.0 + wobble).max(96.0)
    }

    fn collection_revision(&self) -> Option<u64> {
        Some(1)
    }
}

impl ui::collection::CellRenderer for FeedRender {
    fn render(
        &mut self,
        _cell_id: u32,
        index: usize,
        rect: api::RectF,
        focused: bool,
        _hovered: bool,
        builder: &mut ui::DrawListBuilder,
    ) {
        let shell = api::Color::rgba(0.96, 0.97, 0.99, 1.0);
        let border = if focused {
            api::Color::rgba(0.24, 0.52, 0.96, 1.0)
        } else {
            api::Color::rgba(0.82, 0.86, 0.93, 1.0)
        };
        let accent = flat_rect_fill_color(index, 0);
        builder.rrect(rect, [18.0; 4], shell);
        builder.rrect(api::RectF::new(rect.x, rect.y, 6.0, rect.h), [18.0, 0.0, 0.0, 18.0], accent);
        builder.rrect(
            api::RectF::new(rect.x + 18.0, rect.y + 16.0, rect.w - 36.0, 14.0),
            [7.0; 4],
            border,
        );
        builder.rrect(
            api::RectF::new(rect.x + 18.0, rect.y + 38.0, rect.w - 54.0, 10.0),
            [5.0; 4],
            api::Color::rgba(0.76, 0.80, 0.88, 1.0),
        );
        builder.rrect(
            api::RectF::new(rect.x + 18.0, rect.y + rect.h - 28.0, rect.w - 92.0, 10.0),
            [5.0; 4],
            api::Color::rgba(0.84, 0.87, 0.93, 1.0),
        );
    }
}

impl ui::collection::Measure for ChatMeasure {
    fn measure(&mut self, index: usize, _constraint: f32) -> f32 {
        54.0 + (index % 4) as f32 * 10.0
    }

    fn collection_revision(&self) -> Option<u64> {
        Some(1)
    }
}

impl ui::collection::CellRenderer for ChatRender {
    fn render(
        &mut self,
        _cell_id: u32,
        index: usize,
        rect: api::RectF,
        focused: bool,
        _hovered: bool,
        builder: &mut ui::DrawListBuilder,
    ) {
        let outgoing = index % 2 == 0;
        let bubble_w = if outgoing { rect.w * 0.62 } else { rect.w * 0.70 };
        let bubble_x = if outgoing { rect.x + rect.w - bubble_w } else { rect.x };
        let bubble = api::RectF::new(bubble_x, rect.y + 6.0, bubble_w, rect.h - 12.0);
        let color = if outgoing {
            api::Color::rgba(0.24, 0.62, 0.96, 1.0)
        } else {
            api::Color::rgba(0.92, 0.94, 0.97, 1.0)
        };
        let line = if focused {
            api::Color::rgba(0.08, 0.16, 0.28, 1.0)
        } else if outgoing {
            api::Color::rgba(0.90, 0.96, 1.0, 1.0)
        } else {
            api::Color::rgba(0.28, 0.34, 0.44, 1.0)
        };
        builder.rrect(bubble, [18.0; 4], color);
        builder.rrect(
            api::RectF::new(bubble.x + 14.0, bubble.y + 12.0, bubble.w - 28.0, 9.0),
            [4.5; 4],
            line,
        );
        builder.rrect(
            api::RectF::new(bubble.x + 14.0, bubble.y + 28.0, bubble.w - 42.0, 8.0),
            [4.0; 4],
            line,
        );
    }
}

struct KeyReconcileMeasure {
    order: Vec<u64>,
    index_by_key: BTreeMap<u64, usize>,
    item_key_queries: Arc<AtomicU64>,
    key_index_queries: Arc<AtomicU64>,
    key_index_hits: Arc<AtomicU64>,
    indexed: bool,
}

impl KeyReconcileMeasure {
    fn new(
        count: usize,
        indexed: bool,
        item_key_queries: Arc<AtomicU64>,
        key_index_queries: Arc<AtomicU64>,
        key_index_hits: Arc<AtomicU64>,
    ) -> Self {
        let mut measure = Self {
            order: (0..count as u64).collect(),
            index_by_key: BTreeMap::new(),
            item_key_queries,
            key_index_queries,
            key_index_hits,
            indexed,
        };
        measure.move_key_to(200, 220);
        measure
    }

    fn move_key_to(&mut self, key: u64, target: usize) {
        let Some(source) = self.order.iter().position(|candidate| *candidate == key) else {
            self.rebuild_index();
            return;
        };
        let key = self.order.remove(source);
        self.order.insert(target.min(self.order.len()), key);
        self.rebuild_index();
    }

    fn rebuild_index(&mut self) {
        self.index_by_key.clear();
        for (index, key) in self.order.iter().enumerate() {
            self.index_by_key.insert(*key, index);
        }
    }
}

impl ui::collection::Measure for KeyReconcileMeasure {
    fn measure(&mut self, _index: usize, _constraint: f32) -> f32 {
        40.0
    }

    fn item_key(&self, index: usize) -> ui::collection::ItemKey {
        self.item_key_queries.fetch_add(1, Ordering::Relaxed);
        ui::collection::ItemKey(self.order[index])
    }

    fn item_index_for_key(&self, key: ui::collection::ItemKey) -> Option<usize> {
        self.key_index_queries.fetch_add(1, Ordering::Relaxed);
        if !self.indexed {
            return None;
        }
        let index = self.index_by_key.get(&key.0).copied();
        if index.is_some() {
            self.key_index_hits.fetch_add(1, Ordering::Relaxed);
        }
        index
    }

    fn fixed_extent(&self, _constraint: f32) -> Option<f32> {
        Some(40.0)
    }
}

impl ui::collection::CellRenderer for KeyReconcileRender {
    fn render(
        &mut self,
        _cell_id: u32,
        _index: usize,
        _rect: api::RectF,
        _focused: bool,
        _hovered: bool,
        _builder: &mut ui::DrawListBuilder,
    ) {
    }
}

struct PrefixUpdateMeasure {
    revisions: Vec<u64>,
    epoch: u64,
    changed_index: usize,
    changed_range_enabled: bool,
    measure_calls: Arc<AtomicU64>,
    revision_queries: Arc<AtomicU64>,
}

impl PrefixUpdateMeasure {
    fn new(
        count: usize,
        changed_index: usize,
        changed_range_enabled: bool,
        measure_calls: Arc<AtomicU64>,
        revision_queries: Arc<AtomicU64>,
    ) -> Self {
        Self {
            revisions: vec![0; count],
            epoch: 1,
            changed_index,
            changed_range_enabled,
            measure_calls,
            revision_queries,
        }
    }

    fn bump_revision(&mut self) {
        let current = self.revisions[self.changed_index];
        self.revisions[self.changed_index] = if current == 1 { 2 } else { 1 };
        self.epoch = self.epoch.wrapping_add(1);
    }
}

impl ui::collection::Measure for PrefixUpdateMeasure {
    fn measure(&mut self, index: usize, constraint: f32) -> f32 {
        self.measure_calls.fetch_add(1, Ordering::Relaxed);
        let wobble = (index % 7) as f32 * 2.0;
        (constraint * 0.55 + wobble).max(24.0)
    }

    fn item_revision(&self, index: usize) -> u64 {
        self.revision_queries.fetch_add(1, Ordering::Relaxed);
        self.revisions[index]
    }

    fn collection_revision(&self) -> Option<u64> {
        Some(self.epoch)
    }

    fn changed_item_range(&self) -> Option<core::ops::Range<usize>> {
        if self.changed_range_enabled {
            Some(self.changed_index..self.changed_index.saturating_add(1))
        } else {
            None
        }
    }
}

pub fn run_from_env() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    run_cli(&args)
}

pub fn run_cli(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage();
        return Ok(());
    }
    let cli = parse_cli(args)?;
    if cli.paired_create_instrumentation_patch.is_some() {
        return run_create_instrumentation_patch(cli);
    }
    if cli.paired_run.is_some() {
        return run_paired_workflow(cli);
    }
    if cli.paired_analyze.is_some() {
        return run_paired_analysis(cli);
    }
    if cli.paired_json_out.is_some() {
        bail!("--paired-json-out requires --paired-run or --paired-analyze");
    }
    if cli.sample_summary_bench {
        return run_sample_summary_bench(cli);
    }
    if cli.sample_summary_bench_iters != 0 {
        bail!("--bench-sample-summary-iters requires --bench-sample-summary");
    }
    if cli.case_filter_bench {
        return run_case_filter_bench(cli);
    }
    if cli.case_filter_bench_iters != 0 {
        bail!("--bench-case-filter-iters requires --bench-case-filter");
    }
    if cli.compare_bench_current.is_some() {
        return run_compare_reports_bench(cli);
    }
    if cli.compare_bench_iters != 0 {
        bail!("--bench-compare-iters requires --bench-compare-reports");
    }
    if cli.frame_pacing_metrics_bench {
        return run_frame_pacing_metrics_bench(cli);
    }
    if cli.frame_pacing_metrics_bench_iters != 0 {
        bail!("--bench-frame-pacing-iters requires --bench-frame-pacing-metrics");
    }
    if cli.distribution_metrics_bench {
        return run_distribution_metrics_bench(cli);
    }
    if cli.distribution_metrics_bench_iters != 0 {
        bail!("--bench-distribution-iters requires --bench-distribution-metrics");
    }
    if cli.case_metric_contract_bench_report.is_some() {
        return run_case_metric_contract_bench(cli);
    }
    if cli.case_metric_contract_bench_iters != 0 {
        bail!("--bench-case-metric-iters requires --bench-case-metric-contract");
    }
    if cli.contract_coverage_bench_report.is_some() {
        return run_contract_coverage_bench(cli);
    }
    if cli.contract_coverage_bench_iters != 0 {
        bail!("--bench-contract-iters requires --bench-contract-coverage");
    }
    if cli.markdown_write_bench_report.is_some() {
        return run_markdown_write_bench(cli);
    }
    if cli.markdown_bench_report.is_some() {
        return run_markdown_render_bench(cli);
    }
    if cli.markdown_bench_iters != 0 {
        bail!("--bench-markdown-iters requires --bench-markdown-render or --bench-markdown-write");
    }
    if cli.markdown_bench_compare.is_some() {
        bail!("--bench-markdown-compare requires --bench-markdown-render or --bench-markdown-write");
    }
    if cli.json_bench_report.is_some() {
        return run_json_render_bench(cli);
    }
    if cli.json_string_bench_report.is_some() {
        return run_json_string_render_bench(cli);
    }
    if cli.json_bench_iters != 0 {
        bail!("--bench-json-iters requires --bench-json-render or --bench-json-string-render");
    }
    if cli.run_suite {
        return run_suite(cli);
    }
    run_legacy_runner()
}

fn parse_cli(args: &[String]) -> Result<Cli> {
    let mut cli = Cli::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--run-suite" => {
                cli.run_suite = true;
            }
            "--smoke" => {
                cli.run_suite = true;
                cli.smoke = true;
            }
            "--compare" => {
                cli.run_suite = true;
                let path = it.next().context("missing value for --compare")?;
                cli.compare = Some(PathBuf::from(path));
            }
            "--json-out" => {
                cli.run_suite = true;
                let path = it.next().context("missing value for --json-out")?;
                cli.json_out = Some(PathBuf::from(path));
            }
            "--markdown-out" => {
                cli.run_suite = true;
                let path = it.next().context("missing value for --markdown-out")?;
                cli.markdown_out = Some(PathBuf::from(path));
            }
            "--paired-analyze" => {
                let path = it.next().context("missing value for --paired-analyze")?;
                cli.paired_analyze = Some(PathBuf::from(path));
            }
            "--paired-run" => {
                let path = it.next().context("missing value for --paired-run")?;
                cli.paired_run = Some(PathBuf::from(path));
            }
            "--paired-json-out" => {
                let path = it.next().context("missing value for --paired-json-out")?;
                cli.paired_json_out = Some(PathBuf::from(path));
            }
            "--paired-create-instrumentation-patch" => {
                let path = it.next().context("missing value for --paired-create-instrumentation-patch")?;
                cli.paired_create_instrumentation_patch = Some(PathBuf::from(path));
            }
            "--paired-instrumentation-root" => {
                let path = it.next().context("missing value for --paired-instrumentation-root")?;
                cli.paired_instrumentation_root = Some(PathBuf::from(path));
            }
            "--paired-instrumentation-path" => {
                let path = it.next().context("missing value for --paired-instrumentation-path")?;
                cli.paired_instrumentation_paths.push(PathBuf::from(path));
            }
            "--bench-markdown-render" => {
                let path = it.next().context("missing value for --bench-markdown-render")?;
                cli.markdown_bench_report = Some(PathBuf::from(path));
            }
            "--bench-markdown-write" => {
                let path = it.next().context("missing value for --bench-markdown-write")?;
                cli.markdown_write_bench_report = Some(PathBuf::from(path));
            }
            "--bench-markdown-compare" => {
                let path = it.next().context("missing value for --bench-markdown-compare")?;
                cli.markdown_bench_compare = Some(PathBuf::from(path));
            }
            "--bench-markdown-iters" => {
                let value = it.next().context("missing value for --bench-markdown-iters")?;
                cli.markdown_bench_iters = value
                    .parse::<usize>()
                    .with_context(|| format!("invalid --bench-markdown-iters value `{}`", value))?;
                if cli.markdown_bench_iters == 0 {
                    bail!("--bench-markdown-iters must be greater than zero");
                }
            }
            "--bench-json-render" => {
                let path = it.next().context("missing value for --bench-json-render")?;
                cli.json_bench_report = Some(PathBuf::from(path));
            }
            "--bench-json-string-render" => {
                let path = it.next().context("missing value for --bench-json-string-render")?;
                cli.json_string_bench_report = Some(PathBuf::from(path));
            }
            "--bench-json-iters" => {
                let value = it.next().context("missing value for --bench-json-iters")?;
                cli.json_bench_iters = value
                    .parse::<usize>()
                    .with_context(|| format!("invalid --bench-json-iters value `{}`", value))?;
                if cli.json_bench_iters == 0 {
                    bail!("--bench-json-iters must be greater than zero");
                }
            }
            "--bench-sample-summary" => {
                cli.sample_summary_bench = true;
            }
            "--bench-sample-summary-iters" => {
                let value = it.next().context("missing value for --bench-sample-summary-iters")?;
                cli.sample_summary_bench_iters = value
                    .parse::<usize>()
                    .with_context(|| format!("invalid --bench-sample-summary-iters value `{}`", value))?;
                if cli.sample_summary_bench_iters == 0 {
                    bail!("--bench-sample-summary-iters must be greater than zero");
                }
            }
            "--bench-case-filter" => {
                cli.case_filter_bench = true;
            }
            "--bench-case-filter-iters" => {
                let value = it.next().context("missing value for --bench-case-filter-iters")?;
                cli.case_filter_bench_iters = value
                    .parse::<usize>()
                    .with_context(|| format!("invalid --bench-case-filter-iters value `{}`", value))?;
                if cli.case_filter_bench_iters == 0 {
                    bail!("--bench-case-filter-iters must be greater than zero");
                }
            }
            "--bench-compare-reports" => {
                let current = it.next().context("missing current report for --bench-compare-reports")?;
                let baseline = it.next().context("missing baseline report for --bench-compare-reports")?;
                cli.compare_bench_current = Some(PathBuf::from(current));
                cli.compare_bench_baseline = Some(PathBuf::from(baseline));
            }
            "--bench-compare-iters" => {
                let value = it.next().context("missing value for --bench-compare-iters")?;
                cli.compare_bench_iters = value
                    .parse::<usize>()
                    .with_context(|| format!("invalid --bench-compare-iters value `{}`", value))?;
                if cli.compare_bench_iters == 0 {
                    bail!("--bench-compare-iters must be greater than zero");
                }
            }
            "--bench-frame-pacing-metrics" => {
                cli.frame_pacing_metrics_bench = true;
            }
            "--bench-frame-pacing-iters" => {
                let value = it.next().context("missing value for --bench-frame-pacing-iters")?;
                cli.frame_pacing_metrics_bench_iters = value
                    .parse::<usize>()
                    .with_context(|| format!("invalid --bench-frame-pacing-iters value `{}`", value))?;
                if cli.frame_pacing_metrics_bench_iters == 0 {
                    bail!("--bench-frame-pacing-iters must be greater than zero");
                }
            }
            "--bench-distribution-metrics" => {
                cli.distribution_metrics_bench = true;
            }
            "--bench-distribution-iters" => {
                let value = it.next().context("missing value for --bench-distribution-iters")?;
                cli.distribution_metrics_bench_iters = value
                    .parse::<usize>()
                    .with_context(|| format!("invalid --bench-distribution-iters value `{}`", value))?;
                if cli.distribution_metrics_bench_iters == 0 {
                    bail!("--bench-distribution-iters must be greater than zero");
                }
            }
            "--bench-case-metric-contract" => {
                let path = it.next().context("missing value for --bench-case-metric-contract")?;
                cli.case_metric_contract_bench_report = Some(PathBuf::from(path));
            }
            "--bench-case-metric-iters" => {
                let value = it.next().context("missing value for --bench-case-metric-iters")?;
                cli.case_metric_contract_bench_iters = value
                    .parse::<usize>()
                    .with_context(|| format!("invalid --bench-case-metric-iters value `{}`", value))?;
                if cli.case_metric_contract_bench_iters == 0 {
                    bail!("--bench-case-metric-iters must be greater than zero");
                }
            }
            "--bench-contract-coverage" => {
                let path = it.next().context("missing value for --bench-contract-coverage")?;
                cli.contract_coverage_bench_report = Some(PathBuf::from(path));
            }
            "--bench-contract-iters" => {
                let value = it.next().context("missing value for --bench-contract-iters")?;
                cli.contract_coverage_bench_iters = value
                    .parse::<usize>()
                    .with_context(|| format!("invalid --bench-contract-iters value `{}`", value))?;
                if cli.contract_coverage_bench_iters == 0 {
                    bail!("--bench-contract-iters must be greater than zero");
                }
            }
            "--write-baseline" => {
                cli.run_suite = true;
                cli.write_baseline = true;
            }
            other => {
                bail!("unknown argument `{}`", other);
            }
        }
    }
    Ok(cli)
}

fn print_usage() {
    println!("oxide-perf-runner");
    println!("  default: legacy renderer summary for sweep scripts");
    println!("  --run-suite [--smoke] [--compare PATH] [--json-out PATH] [--markdown-out PATH]");
    println!("  --write-baseline writes to benchmarks/workspace/latest.json and latest.md");
    println!("  --paired-run PLAN --paired-json-out PATH");
    println!("  --paired-analyze INPUT --paired-json-out PATH");
    println!("  --paired-create-instrumentation-patch OUT --paired-instrumentation-root ROOT --paired-instrumentation-path PATH [...]");
    println!("  --bench-markdown-render PATH [--bench-markdown-compare PATH] [--bench-markdown-iters N]");
    println!("  --bench-markdown-write PATH [--bench-markdown-compare PATH] [--bench-markdown-iters N]");
    println!("  --bench-json-render PATH [--bench-json-iters N]");
    println!("  --bench-json-string-render PATH [--bench-json-iters N]");
    println!("  --bench-sample-summary [--bench-sample-summary-iters N]");
    println!("  --bench-case-filter [--bench-case-filter-iters N]");
    println!("  --bench-compare-reports CURRENT BASELINE [--bench-compare-iters N]");
    println!("  --bench-frame-pacing-metrics [--bench-frame-pacing-iters N]");
    println!("  --bench-distribution-metrics [--bench-distribution-iters N]");
    println!("  --bench-case-metric-contract PATH [--bench-case-metric-iters N]");
    println!("  --bench-contract-coverage PATH [--bench-contract-iters N]");
}

fn run_paired_analysis(cli: Cli) -> Result<()> {
    let input_path = cli.paired_analyze.context("missing paired experiment input")?;
    let output_path = cli.paired_json_out.context("--paired-analyze requires --paired-json-out")?;
    let input_bytes = fs::read(&input_path)
        .with_context(|| format!("read paired experiment input {}", input_path.display()))?;
    let input = serde_json::from_slice::<paired::PairedExperimentInput>(&input_bytes)
        .with_context(|| format!("parse paired experiment input {}", input_path.display()))?;
    let report = paired::analyze_paired_experiment(input)?;
    let report_bytes = paired::report_json(&report)?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create paired report directory {}", parent.display()))?;
    }
    fs::write(&output_path, report_bytes)
        .with_context(|| format!("write paired experiment report {}", output_path.display()))?;
    Ok(())
}

fn run_paired_workflow(cli: Cli) -> Result<()> {
    let plan_path = cli.paired_run.context("missing paired workflow plan")?;
    let output_path = cli.paired_json_out.context("--paired-run requires --paired-json-out")?;
    let plan_bytes = fs::read(&plan_path)
        .with_context(|| format!("read paired workflow plan {}", plan_path.display()))?;
    let plan = serde_json::from_slice::<paired::PairedWorkflowPlan>(&plan_bytes)
        .with_context(|| format!("parse paired workflow plan {}", plan_path.display()))?;
    let report = paired::run_paired_workflow(plan)?;
    let report_bytes = paired::report_json(&report)?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create paired report directory {}", parent.display()))?;
    }
    fs::write(&output_path, report_bytes)
        .with_context(|| format!("write paired workflow report {}", output_path.display()))?;
    Ok(())
}

fn run_create_instrumentation_patch(cli: Cli) -> Result<()> {
    let output_path = cli
        .paired_create_instrumentation_patch
        .context("missing instrumentation patch output")?;
    let source_root = cli.paired_instrumentation_root.unwrap_or_else(|| PathBuf::from("."));
    let sha256 = paired::create_instrumentation_patch(
        &source_root,
        &cli.paired_instrumentation_paths,
        &output_path,
    )?;
    println!("{}  {}", sha256, output_path.display());
    Ok(())
}

fn load_markdown_bench_inputs(path: &Path, compare_path: Option<&PathBuf>) -> Result<(PerfReport, Option<PerfComparison>)> {
    let report = load_report(path)?;
    let comparison = if let Some(compare_path) = compare_path {
        let baseline = load_report(compare_path)?;
        Some(compare_reports(&report, &baseline))
    } else {
        None
    };
    Ok((report, comparison))
}

fn markdown_bench_iters(cli: &Cli) -> usize {
    if cli.markdown_bench_iters == 0 {
        DEFAULT_MARKDOWN_RENDER_BENCH_ITERS
    } else {
        cli.markdown_bench_iters
    }
}

fn run_markdown_render_bench(cli: Cli) -> Result<()> {
    let path = cli
        .markdown_bench_report
        .as_ref()
        .context("missing --bench-markdown-render path")?;
    let iterations = markdown_bench_iters(&cli);
    let (report, comparison) = load_markdown_bench_inputs(path, cli.markdown_bench_compare.as_ref())?;
    let matched = comparison.as_ref().map(|comparison| comparison.matched).unwrap_or(0);
    let regressions = comparison.as_ref().map(|comparison| comparison.regressions.len()).unwrap_or(0);
    let missing_baseline =
        comparison.as_ref().map(|comparison| comparison.missing_baseline.len()).unwrap_or(0);
    let start = Instant::now();
    let mut total_bytes = 0usize;
    for _ in 0..iterations {
        let body = render_markdown(black_box(&report), black_box(comparison.as_ref()));
        total_bytes = total_bytes.wrapping_add(black_box(body.len()));
        black_box(&body);
    }
    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
    let us_per_iter = elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64;
    let bytes_per_iter = total_bytes / iterations;
    println!(
        "markdown_render_bench report={} comparison={} matched={} regressions={} missing_baseline={} iterations={} elapsed_ms={:.3} us_per_iter={:.3} bytes_per_iter={} total_bytes={}",
        path.display(),
        cli.markdown_bench_compare
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| String::from("-")),
        matched,
        regressions,
        missing_baseline,
        iterations,
        elapsed_ms,
        us_per_iter,
        bytes_per_iter,
        total_bytes
    );
    Ok(())
}

fn markdown_latest_and_dated_render_bytes(report: &PerfReport, comparison: Option<&PerfComparison>) -> usize {
    let body = render_markdown(report, comparison);
    body.len().wrapping_mul(2)
}

fn run_markdown_write_bench(cli: Cli) -> Result<()> {
    let path = cli
        .markdown_write_bench_report
        .as_ref()
        .context("missing --bench-markdown-write path")?;
    let iterations = markdown_bench_iters(&cli);
    let (report, comparison) = load_markdown_bench_inputs(path, cli.markdown_bench_compare.as_ref())?;
    let matched = comparison.as_ref().map(|comparison| comparison.matched).unwrap_or(0);
    let regressions = comparison.as_ref().map(|comparison| comparison.regressions.len()).unwrap_or(0);
    let missing_baseline =
        comparison.as_ref().map(|comparison| comparison.missing_baseline.len()).unwrap_or(0);
    let start = Instant::now();
    let mut total_bytes = 0usize;
    for _ in 0..iterations {
        let bytes = markdown_latest_and_dated_render_bytes(
            black_box(&report),
            black_box(comparison.as_ref()),
        );
        total_bytes = total_bytes.wrapping_add(black_box(bytes));
    }
    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
    let us_per_iter = elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64;
    let bytes_per_iter = total_bytes / iterations;
    println!(
        "markdown_write_bench report={} comparison={} matched={} regressions={} missing_baseline={} iterations={} elapsed_ms={:.3} us_per_iter={:.3} bytes_per_iter={} total_bytes={}",
        path.display(),
        cli.markdown_bench_compare
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| String::from("-")),
        matched,
        regressions,
        missing_baseline,
        iterations,
        elapsed_ms,
        us_per_iter,
        bytes_per_iter,
        total_bytes
    );
    Ok(())
}

fn json_bench_iters(cli: &Cli) -> usize {
    if cli.json_bench_iters == 0 {
        DEFAULT_JSON_RENDER_BENCH_ITERS
    } else {
        cli.json_bench_iters
    }
}

fn run_json_render_bench(cli: Cli) -> Result<()> {
    let path = cli
        .json_bench_report
        .as_ref()
        .context("missing --bench-json-render path")?;
    let iterations = json_bench_iters(&cli);
    let report = load_report(path)?;
    let start = Instant::now();
    let mut total_bytes = 0usize;
    for _ in 0..iterations {
        let body = serialize_report_json(black_box(&report))?;
        total_bytes = total_bytes.wrapping_add(black_box(body.len()));
        black_box(&body);
    }
    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
    let us_per_iter = elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64;
    let bytes_per_iter = total_bytes / iterations;
    println!(
        "json_render_bench report={} iterations={} elapsed_ms={:.3} us_per_iter={:.3} bytes_per_iter={} total_bytes={}",
        path.display(),
        iterations,
        elapsed_ms,
        us_per_iter,
        bytes_per_iter,
        total_bytes
    );
    Ok(())
}

fn run_json_string_render_bench(cli: Cli) -> Result<()> {
    let path = cli
        .json_string_bench_report
        .as_ref()
        .context("missing --bench-json-string-render path")?;
    let iterations = json_bench_iters(&cli);
    let report = load_report(path)?;
    let start = Instant::now();
    let mut total_bytes = 0usize;
    for _ in 0..iterations {
        let body = serialize_report_json_string(black_box(&report))?;
        total_bytes = total_bytes.wrapping_add(black_box(body.len()));
        black_box(&body);
    }
    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
    let us_per_iter = elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64;
    let bytes_per_iter = total_bytes / iterations;
    println!(
        "json_string_render_bench report={} iterations={} elapsed_ms={:.3} us_per_iter={:.3} bytes_per_iter={} total_bytes={}",
        path.display(),
        iterations,
        elapsed_ms,
        us_per_iter,
        bytes_per_iter,
        total_bytes
    );
    Ok(())
}

fn sample_summary_bench_iters(cli: &Cli) -> usize
{
   if cli.sample_summary_bench_iters == 0
   {
      DEFAULT_SAMPLE_SUMMARY_BENCH_ITERS
   }
   else
   {
      cli.sample_summary_bench_iters
   }
}

fn run_sample_summary_bench(cli: Cli) -> Result<()>
{
   const SMOKE_SAMPLES: [f64; 6] = [3.5, 4.25, 2.75, 5.0, 3.875, 6.125];
   const JOURNEY_SAMPLES: [f64; 10] = [
      118.0, 121.5, 116.25, 130.0, 119.75, 124.5, 117.5, 122.25, 128.0, 120.5,
   ];
   const CPU_SAMPLES: [f64; 12] = [
      0.88, 0.91, 0.86, 0.94, 0.89, 0.97, 0.92, 0.87, 1.05, 0.90, 0.95, 0.93,
   ];
   const GPU_SAMPLES: [f64; 24] = [
      5.9, 6.2, 6.0, 6.4, 6.1, 7.8, 6.3, 6.0, 6.5, 6.2, 6.1, 8.4,
      6.3, 6.6, 6.2, 6.0, 6.4, 6.1, 7.2, 6.5, 6.3, 6.2, 6.7, 9.1,
   ];

   let groups: [&[f64]; 4] = [
      &SMOKE_SAMPLES[..],
      &JOURNEY_SAMPLES[..],
      &CPU_SAMPLES[..],
      &GPU_SAMPLES[..],
   ];
   let iterations = sample_summary_bench_iters(&cli);
   let start = Instant::now();
   let mut summary_count = 0usize;
   let mut checksum = 0u64;
   for _ in 0..iterations
   {
      for samples in &groups
      {
         let summary = summarize(black_box(*samples));
         summary_count = summary_count.wrapping_add(1);
         checksum = checksum.wrapping_add(black_box(sample_summary_bench_checksum(summary)));
         black_box(summary);
      }
   }
   let elapsed = start.elapsed();
   let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
   let us_per_iter = elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64;
   println!(
      "sample_summary_bench iterations={} groups={} elapsed_ms={:.3} us_per_iter={:.3} summaries_per_iter={} checksum={}",
      iterations,
      groups.len(),
      elapsed_ms,
      us_per_iter,
      summary_count / iterations,
      checksum
   );
   Ok(())
}

fn sample_summary_bench_checksum(summary: SampleSummary) -> u64
{
   let mut checksum = summary.min.to_bits();
   checksum = checksum.rotate_left(7) ^ summary.max.to_bits();
   checksum = checksum.rotate_left(7) ^ summary.mean.to_bits();
   checksum = checksum.rotate_left(7) ^ summary.median.to_bits();
   checksum = checksum.rotate_left(7) ^ summary.p95.to_bits();
   checksum.rotate_left(7) ^ summary.p99.to_bits()
}

fn run_case_filter_bench(cli: Cli) -> Result<()> {
    const PREFIXES: &[&str] = &[
        "cpu.system.",
        "gpu.system.",
        "cpu.component.",
        "cpu.authoring.",
        "gpu.scene.",
        "cpu.layout.",
        "cpu.reconcile.",
        "cpu.bridge.",
    ];
    const CASES: &[&str] = &[
        "cpu.system.prepare_draws.current",
        "cpu.system.text_prefix_width_map",
        "cpu.authoring.collection_key_reconcile",
        "cpu.authoring.collection_prefix_update",
        "gpu.scene.damage_lab.frame",
        "gpu.journey.collection_navigation.frame_pacing",
        "cpu.layout.dirty_subtree.incremental_relayout",
        "cpu.bridge.permission_callback_fanout",
    ];
    let iterations = if cli.case_filter_bench_iters == 0 {
        DEFAULT_CASE_FILTER_BENCH_ITERS
    } else {
        cli.case_filter_bench_iters
    };
    let start = Instant::now();
    let mut allowed = 0usize;
    let mut prefix_allowed = 0usize;
    let mut checksum = 0u64;
    for _ in 0..iterations {
        for prefix in PREFIXES {
            let prefix = black_box(*prefix);
            if black_box(perf_case_prefix_allowed(prefix)) {
                prefix_allowed = prefix_allowed.wrapping_add(1);
                checksum = checksum.wrapping_add(filter_bench_key(prefix));
            } else {
                checksum = checksum.wrapping_add(1);
            }
        }
        for case_id in CASES {
            let case_id = black_box(*case_id);
            if black_box(perf_case_allowed(case_id)) {
                allowed = allowed.wrapping_add(1);
                checksum = checksum.wrapping_add(filter_bench_key(case_id));
            } else {
                checksum = checksum.wrapping_add(1);
            }
        }
    }
    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
    let us_per_iter = elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64;
    println!(
        "case_filter_bench iterations={} elapsed_ms={:.3} us_per_iter={:.3} allowed_per_iter={} prefix_allowed_per_iter={} checksum={}",
        iterations,
        elapsed_ms,
        us_per_iter,
        allowed / iterations,
        prefix_allowed / iterations,
        checksum
    );
    Ok(())
}

fn filter_bench_key(value: &str) -> u64 {
    value.as_bytes().iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        hash.wrapping_mul(0x100000001b3) ^ u64::from(*byte)
    })
}

fn distribution_metrics_bench_iters(cli: &Cli) -> usize
{
   if cli.distribution_metrics_bench_iters == 0
   {
      DEFAULT_DISTRIBUTION_METRICS_BENCH_ITERS
   }
   else
   {
      cli.distribution_metrics_bench_iters
   }
}

fn distribution_metrics_bench_samples(base: f64) -> Vec<f64>
{
   let mut samples = Vec::with_capacity(24);
   for index in 0..24usize
   {
      let sample = base
         + (index % 7) as f64 * 0.137
         + if index % 11 == 0 { 1.75 } else { 0.0 };
      samples.push(sample);
   }
   samples
}

fn run_distribution_metrics_bench(cli: Cli) -> Result<()>
{
   let iterations = distribution_metrics_bench_iters(&cli);
   let frame_samples = distribution_metrics_bench_samples(6.2);
   let event_samples = distribution_metrics_bench_samples(7.4);
   let gpu_samples = distribution_metrics_bench_samples(1.1);
   let start = Instant::now();
   let mut metric_count = 0usize;
   let mut checksum = 0u64;
   for _ in 0..iterations
   {
      let mut metrics = BTreeMap::new();
      insert_distribution_metrics(&mut metrics, "frame_ms", black_box(frame_samples.as_slice()));
      insert_distribution_metrics(&mut metrics, "event_to_visible_ms", black_box(event_samples.as_slice()));
      insert_distribution_metrics(&mut metrics, "gpu_ms", black_box(gpu_samples.as_slice()));
      metric_count = metric_count.wrapping_add(black_box(metrics.len()));
      checksum = checksum.wrapping_add(black_box(frame_pacing_metrics_bench_checksum(&metrics)));
      black_box(&metrics);
   }
   let elapsed = start.elapsed();
   let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
   let us_per_iter = elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64;
   println!(
      "distribution_metrics_bench iterations={} samples_per_distribution={} distributions_per_iter=3 elapsed_ms={:.3} us_per_iter={:.3} metrics_per_iter={} checksum={}",
      iterations,
      frame_samples.len(),
      elapsed_ms,
      us_per_iter,
      metric_count / iterations,
      checksum
   );
   Ok(())
}

fn case_metric_contract_bench_iters(cli: &Cli) -> usize
{
   if cli.case_metric_contract_bench_iters == 0
   {
      DEFAULT_CASE_METRIC_CONTRACT_BENCH_ITERS
   }
   else
   {
      cli.case_metric_contract_bench_iters
   }
}

fn run_case_metric_contract_bench(cli: Cli) -> Result<()>
{
   let path = cli
      .case_metric_contract_bench_report
      .as_ref()
      .context("missing --bench-case-metric-contract path")?;
   let iterations = case_metric_contract_bench_iters(&cli);
   let report = load_report(path)?;
   let frame_required = report.cases.iter().filter(|case| case_requires_frame_metrics(case)).count();
   let gpu_required = report.cases.iter().filter(|case| case_requires_gpu_metrics(case)).count();
   let shape_checksum = case_metric_contract_bench_checksum(&report.cases);
   let start = Instant::now();
   let mut checksum = 0u64;
   for _ in 0..iterations
   {
      assert_case_metric_contract(black_box(&report.cases))?;
      checksum = checksum.wrapping_add(black_box(shape_checksum));
   }
   let elapsed = start.elapsed();
   let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
   let us_per_iter = elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64;
   println!(
      "case_metric_contract_bench report={} iterations={} cases={} frame_required={} gpu_required={} elapsed_ms={:.3} us_per_iter={:.3} checksum={}",
      path.display(),
      iterations,
      report.cases.len(),
      frame_required,
      gpu_required,
      elapsed_ms,
      us_per_iter,
      checksum
   );
   Ok(())
}

fn case_metric_contract_bench_checksum(cases: &[PerfCaseResult]) -> u64
{
   let mut checksum = 0u64;
   for case in cases
   {
      if case_requires_frame_metrics(case)
      {
         checksum = checksum.wrapping_add(filter_bench_key(&case.id));
         checksum = checksum.wrapping_add(1);
      }
      if case_requires_gpu_metrics(case)
      {
         checksum = checksum.wrapping_add(filter_bench_key(&case.id));
         checksum = checksum.wrapping_add(2);
      }
   }
   checksum
}

fn contract_coverage_bench_iters(cli: &Cli) -> usize
{
   if cli.contract_coverage_bench_iters == 0
   {
      DEFAULT_CONTRACT_COVERAGE_BENCH_ITERS
   }
   else
   {
      cli.contract_coverage_bench_iters
   }
}

fn run_contract_coverage_bench(cli: Cli) -> Result<()>
{
   let path = cli
      .contract_coverage_bench_report
      .as_ref()
      .context("missing --bench-contract-coverage path")?;
   let iterations = contract_coverage_bench_iters(&cli);
   let report = load_report(path)?;
   let layer_count = report.contract.layers.len();
   let battery_count = report.contract.battery.len();
   let note_count = contract_coverage_note_count(&report.contract);
   let shape_checksum = contract_coverage_bench_checksum(&report.contract);
   let start = Instant::now();
   let mut checksum = 0u64;
   for _ in 0..iterations
   {
      assert_contract_coverage(black_box(&report.contract))?;
      checksum = checksum.wrapping_add(black_box(shape_checksum));
   }
   let elapsed = start.elapsed();
   let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
   let us_per_iter = elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64;
   println!(
      "contract_coverage_bench report={} iterations={} layers={} battery={} notes={} elapsed_ms={:.3} us_per_iter={:.3} checksum={}",
      path.display(),
      iterations,
      layer_count,
      battery_count,
      note_count,
      elapsed_ms,
      us_per_iter,
      checksum
   );
   Ok(())
}

fn contract_coverage_note_count(contract: &ContractCoverageReport) -> usize
{
   contract
      .layers
      .iter()
      .chain(contract.battery.iter())
      .map(|entry| entry.notes.len())
      .sum()
}

fn contract_coverage_bench_checksum(contract: &ContractCoverageReport) -> u64
{
   let mut checksum = 0u64;
   for entry in contract.layers.iter().chain(contract.battery.iter())
   {
      checksum = checksum.wrapping_add(filter_bench_key(&entry.id));
      checksum = checksum.wrapping_add(filter_bench_key(&entry.status));
      for note in &entry.notes
      {
         checksum = checksum.wrapping_add(filter_bench_key(note));
      }
   }
   checksum
}

fn frame_pacing_metrics_bench_iters(cli: &Cli) -> usize
{
   if cli.frame_pacing_metrics_bench_iters == 0
   {
      DEFAULT_FRAME_PACING_METRICS_BENCH_ITERS
   }
   else
   {
      cli.frame_pacing_metrics_bench_iters
   }
}

fn frame_pacing_metrics_bench_samples() -> Vec<f64>
{
   let mut samples = Vec::with_capacity(1024);
   for index in 0..1024usize
   {
      let sample = if index % 97 == 0
      {
         24.0
      }
      else if index % 13 == 0
      {
         12.0
      }
      else if index % 5 == 0
      {
         8.8
      }
      else
      {
         6.2
      };
      samples.push(sample);
   }
   samples
}

fn run_frame_pacing_metrics_bench(cli: Cli) -> Result<()>
{
   let iterations = frame_pacing_metrics_bench_iters(&cli);
   let samples = frame_pacing_metrics_bench_samples();
   let start = Instant::now();
   let mut metric_count = 0usize;
   let mut checksum = 0u64;
   for _ in 0..iterations
   {
      let mut metrics = BTreeMap::new();
      insert_frame_pacing_metrics(&mut metrics, black_box(samples.as_slice()));
      metric_count = metric_count.wrapping_add(black_box(metrics.len()));
      checksum = checksum.wrapping_add(black_box(frame_pacing_metrics_bench_checksum(&metrics)));
      black_box(&metrics);
   }
   let elapsed = start.elapsed();
   let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
   let us_per_iter = elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64;
   println!(
      "frame_pacing_metrics_bench iterations={} samples={} elapsed_ms={:.3} us_per_iter={:.3} metrics_per_iter={} checksum={}",
      iterations,
      samples.len(),
      elapsed_ms,
      us_per_iter,
      metric_count / iterations,
      checksum
   );
   Ok(())
}

fn frame_pacing_metrics_bench_checksum(metrics: &BTreeMap<String, f64>) -> u64
{
   let mut checksum = 0u64;
   for (name, value) in metrics
   {
      checksum = checksum.wrapping_add(filter_bench_key(name));
      checksum = checksum.wrapping_add(value.to_bits());
   }
   checksum
}

fn run_compare_reports_bench(cli: Cli) -> Result<()> {
    let current_path = cli
        .compare_bench_current
        .as_ref()
        .context("missing --bench-compare-reports current path")?;
    let baseline_path = cli
        .compare_bench_baseline
        .as_ref()
        .context("missing --bench-compare-reports baseline path")?;
    let iterations = if cli.compare_bench_iters == 0 {
        DEFAULT_COMPARE_REPORTS_BENCH_ITERS
    } else {
        cli.compare_bench_iters
    };
    let current = load_report(current_path)?;
    let baseline = load_report(baseline_path)?;
    let start = Instant::now();
    let mut matched = 0usize;
    let mut regressions = 0usize;
    let mut missing_baseline = 0usize;
    let mut improvements = 0usize;
    let mut checksum = 0u64;
    for _ in 0..iterations {
        let comparison = compare_reports(black_box(&current), black_box(&baseline));
        matched = matched.wrapping_add(black_box(comparison.matched));
        regressions = regressions.wrapping_add(black_box(comparison.regressions.len()));
        missing_baseline = missing_baseline.wrapping_add(black_box(comparison.missing_baseline.len()));
        improvements = improvements.wrapping_add(black_box(comparison.improvements.len()));
        checksum = checksum.wrapping_add(black_box(comparison_bench_checksum(&comparison)));
        black_box(&comparison);
    }
    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
    let us_per_iter = elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64;
    println!(
        "compare_reports_bench current={} baseline={} iterations={} elapsed_ms={:.3} us_per_iter={:.3} matched={} regressions={} missing_baseline={} improvements={} checksum={}",
        current_path.display(),
        baseline_path.display(),
        iterations,
        elapsed_ms,
        us_per_iter,
        matched / iterations,
        regressions / iterations,
        missing_baseline / iterations,
        improvements / iterations,
        checksum
    );
    Ok(())
}

fn comparison_bench_checksum(comparison: &PerfComparison) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    hash = comparison_bench_hash_usize(hash, comparison.matched);
    for id in &comparison.missing_baseline {
        hash = comparison_bench_hash_str(hash, id);
    }
    for regression in &comparison.regressions {
        hash = comparison_bench_hash_str(hash, &regression.id);
        hash = comparison_bench_hash_u64(hash, regression.baseline_median.to_bits());
        hash = comparison_bench_hash_u64(hash, regression.current_median.to_bits());
        hash = comparison_bench_hash_u64(hash, regression.allowed_median.to_bits());
        hash = comparison_bench_hash_u64(hash, regression.delta_pct.to_bits());
    }
    for id in &comparison.improvements {
        hash = comparison_bench_hash_str(hash, id);
    }
    hash
}

fn comparison_bench_hash_str(mut hash: u64, value: &str) -> u64 {
    for byte in value.as_bytes() {
        hash = comparison_bench_hash_u64(hash, u64::from(*byte));
    }
    comparison_bench_hash_u64(hash, u64::from(b'\n'))
}

fn comparison_bench_hash_usize(hash: u64, value: usize) -> u64 {
    comparison_bench_hash_u64(hash, value as u64)
}

fn comparison_bench_hash_u64(hash: u64, value: u64) -> u64 {
    (hash ^ value).wrapping_mul(0x100000001b3)
}

fn run_suite(cli: Cli) -> Result<()> {
    let report = collect_suite(cli.smoke)?;
    if perf_case_filters().is_empty() {
        assert_full_coverage(&report.coverage)?;
        assert_contract_coverage(&report.contract)?;
        assert_case_metric_contract(&report.cases)?;
    }

    let comparison = if let Some(path) = cli.compare.as_ref() {
        let baseline = load_report(path)?;
        Some(compare_reports(&report, &baseline))
    } else {
        None
    };

    let json_out = if cli.write_baseline {
        Some(cli.json_out.unwrap_or_else(|| PathBuf::from(DEFAULT_BASELINE_JSON)))
    } else {
        cli.json_out
    };
    let markdown_out = if cli.write_baseline {
        Some(cli.markdown_out.unwrap_or_else(|| PathBuf::from(DEFAULT_BASELINE_MARKDOWN)))
    } else {
        cli.markdown_out
    };

    if let Some(path) = json_out.as_ref() {
        write_report_json(path, &report)?;
    }
    if let Some(path) = markdown_out.as_ref() {
        write_markdown_outputs(path, &report, comparison.as_ref())?;
    }

    print_summary(&report, comparison.as_ref());

    if let Some(comp) = comparison.as_ref() {
        if !comp.missing_baseline.is_empty() || !comp.regressions.is_empty() {
            bail!("performance comparison failed; inspect the generated report and update the committed baseline only with review");
        }
    }

    Ok(())
}

pub fn collect_suite_report(smoke: bool) -> Result<PerfReport> {
    collect_suite(smoke)
}

pub fn collect_suite_json(smoke: bool) -> Result<String> {
    let report = collect_suite(smoke)?;
    serde_json::to_string_pretty(&report).context("serializing perf report json")
}

pub fn render_report_markdown(report: &PerfReport, comparison: Option<&PerfComparison>) -> String {
    render_markdown(report, comparison)
}

fn collect_suite(smoke: bool) -> Result<PerfReport> {
    let mut cases = Vec::new();
    let mut covered_components = BTreeSet::new();
    let mut covered_animations = BTreeSet::new();
    let mut covered_launch = BTreeSet::new();
    let mut covered_primitive_lifecycle = BTreeSet::new();
    let mut covered_cpu_scenes = BTreeSet::new();
    let mut covered_gpu_scenes = BTreeSet::new();
    let mut covered_journeys = BTreeSet::new();
    let mut covered_authoring = BTreeSet::new();
    let mut covered_layout = BTreeSet::new();
    let mut covered_text_input = BTreeSet::new();
    let mut covered_image_pipeline = BTreeSet::new();
    let mut covered_navigation = BTreeSet::new();
    let mut covered_reconcile = BTreeSet::new();
    let mut covered_endurance = BTreeSet::new();
    let mut covered_stress = BTreeSet::new();
    let mut covered_bridges = BTreeSet::new();

    if perf_case_prefix_allowed("cpu.system.") || perf_case_prefix_allowed("gpu.system.") {
        push_system_cases(&mut cases, smoke)?;
    }
    if perf_case_prefix_allowed("cpu.architecture.")
        || perf_case_prefix_allowed("gpu.architecture.")
        || perf_case_prefix_allowed("cpu.authoring.image_view_grid.")
        || perf_case_prefix_allowed("gpu.authoring.image_view_grid.")
    {
        architecture_matrix::push_architecture_matrix_cases(&mut cases, smoke)?;
    }
    if perf_case_prefix_allowed("cpu.component.") {
        push_component_cases(&mut cases, smoke, &mut covered_components);
    }
    if perf_case_prefix_allowed("cpu.animation.") {
        push_animation_cases(&mut cases, smoke, &mut covered_animations);
    }
    if perf_case_prefix_allowed("gpu.animation.") {
        push_gpu_animation_cases(&mut cases, smoke)?;
    }
    if perf_case_prefix_allowed("cpu.launch.") {
        push_launch_cases(&mut cases, smoke, &mut covered_launch)?;
    }
    if perf_case_prefix_allowed("cpu.primitive.") {
        push_primitive_lifecycle_cases(&mut cases, smoke, &mut covered_primitive_lifecycle)?;
    }
    if perf_case_prefix_allowed("cpu.scene.") {
        push_cpu_scene_cases(&mut cases, smoke, &mut covered_cpu_scenes)?;
    }
    if perf_case_prefix_allowed("gpu.scene.") {
        push_gpu_scene_cases(&mut cases, smoke, &mut covered_gpu_scenes)?;
    }
    if perf_case_prefix_allowed("gpu.journey.") {
        push_gpu_journey_cases(&mut cases, smoke)?;
    }
    if perf_case_prefix_allowed("cpu.journey.") {
        push_journey_cases(&mut cases, smoke, &mut covered_journeys)?;
    }
    if perf_case_prefix_allowed("cpu.authoring.") || perf_case_prefix_allowed("gpu.authoring.") {
        push_authoring_cases(&mut cases, smoke, &mut covered_authoring)?;
    }
    if perf_case_prefix_allowed("cpu.layout.") {
        push_layout_cases(&mut cases, smoke, &mut covered_layout)?;
    }
    if perf_case_prefix_allowed("cpu.text_input.") {
        push_text_input_cases(&mut cases, smoke, &mut covered_text_input)?;
    }
    if perf_case_prefix_allowed("cpu.image_pipeline.")
        || perf_case_prefix_allowed("gpu.image_pipeline.")
    {
        push_image_pipeline_cases(&mut cases, smoke, &mut covered_image_pipeline)?;
    }
    if perf_case_prefix_allowed("cpu.navigation.") {
        push_navigation_cases(&mut cases, smoke, &mut covered_navigation)?;
    }
    if perf_case_prefix_allowed("cpu.reconcile.") {
        push_reconcile_cases(&mut cases, smoke, &mut covered_reconcile)?;
    }
    if perf_case_prefix_allowed("cpu.endurance.") {
        push_endurance_cases(&mut cases, smoke, &mut covered_endurance)?;
    }
    if perf_case_prefix_allowed("cpu.stress.") {
        push_stress_cases(&mut cases, smoke, &mut covered_stress)?;
    }
    if perf_case_prefix_allowed("cpu.bridge.") {
        push_bridge_cases(&mut cases, smoke, &mut covered_bridges)?;
    }

    let coverage = CoverageReport {
        components_total: registry::components().len(),
        components_covered: covered_components.into_iter().collect(),
        animations_total: registry::animations().len(),
        animations_covered: covered_animations.into_iter().collect(),
        launch_total: PERF_LAUNCH_SPECS.len(),
        launch_covered: covered_launch.into_iter().collect(),
        primitive_lifecycle_total: PERF_PRIMITIVE_LIFECYCLE_SPECS.len(),
        primitive_lifecycle_covered: covered_primitive_lifecycle.into_iter().collect(),
        scenes_cpu_total: PERF_SCENE_SPECS.len(),
        scenes_cpu_covered: covered_cpu_scenes.into_iter().collect(),
        scenes_gpu_total: PERF_SCENE_SPECS.len(),
        scenes_gpu_covered: covered_gpu_scenes.into_iter().collect(),
        journeys_total: PERF_JOURNEY_SPECS.len(),
        journeys_covered: covered_journeys.into_iter().collect(),
        authoring_total: PERF_AUTHORING_SPECS.len(),
        authoring_covered: covered_authoring.into_iter().collect(),
        layout_total: PERF_LAYOUT_SPECS.len(),
        layout_covered: covered_layout.into_iter().collect(),
        text_input_total: PERF_TEXT_INPUT_SPECS.len(),
        text_input_covered: covered_text_input.into_iter().collect(),
        image_pipeline_total: PERF_IMAGE_PIPELINE_SPECS.len(),
        image_pipeline_covered: covered_image_pipeline.into_iter().collect(),
        navigation_total: PERF_NAVIGATION_SPECS.len(),
        navigation_covered: covered_navigation.into_iter().collect(),
        reconcile_total: PERF_RECONCILE_SPECS.len(),
        reconcile_covered: covered_reconcile.into_iter().collect(),
        endurance_total: PERF_ENDURANCE_SPECS.len(),
        endurance_covered: covered_endurance.into_iter().collect(),
        stress_total: PERF_STRESS_SPECS.len(),
        stress_covered: covered_stress.into_iter().collect(),
        bridges_total: PERF_BRIDGE_SPECS.len(),
        bridges_covered: covered_bridges.into_iter().collect(),
    };

    let contract = build_oxide_contract_coverage(&cases);

    let findings = vec![
      AuditFinding {
         status: String::from("fixed"),
         summary: String::from(
            "DrawListBuilder::clear now clears retained vertex and index storage, eliminating stale geometry accumulation when builders are reused across frames.",
         ),
      },
      AuditFinding {
         status: String::from("fixed"),
         summary: String::from(
            "ui-core::prepare_draws now keeps cumulative clip intersections on the stack instead of rebuilding the full stack on every ClipPop.",
         ),
      },
      AuditFinding {
         status: String::from("fixed"),
         summary: String::from(
            "ui-core::coalesce_adjacent_draws now uses a single linear compaction pass instead of Vec::remove-based quadratic merging.",
         ),
      },
      AuditFinding {
         status: String::from("fixed"),
         summary: String::from(
            "oxide-input::TouchSurfaceRecognizer now keeps common active touches inline and derives the active one/two-touch frame with a deterministic scan instead of hashing, allocating, and sorting on every raw touch event.",
         ),
      },
      AuditFinding {
         status: String::from("fixed"),
         summary: String::from(
            "oxide-input::GestureRecognizer now keeps common active tracks inline and writes event-only and feedback outputs through monomorphized sinks, removing hashing and the second vector path from common touch handling.",
         ),
      },
      AuditFinding {
         status: String::from("fixed"),
         summary: String::from(
            "oxide-timing timers now use atomic id generation and drain due callbacks before execution, reducing scheduler overhead while avoiding callback execution under the timer map lock.",
         ),
      },
      AuditFinding {
         status: String::from("fixed"),
         summary: String::from(
            "oxide-timing animation start now uses atomic reduce-motion state and a consistent RUNNING_PROP-to-ANIMS lock order, reducing property-animation replacement overhead.",
         ),
      },
      AuditFinding {
         status: String::from("fixed"),
         summary: String::from(
            "oxide-ui-core label encoding now avoids non-wrapped internal label clones, skips disabled diagnostic string formatting on the hot path, and preallocates the common wrapped-line buffers.",
         ),
      },
      AuditFinding {
         status: String::from("fixed"),
         summary: String::from(
            "oxide-ui-core ASCII wrapped labels now shape once for break decisions on cache misses; the slower legacy wrapped-label audit row was retired after same-workload A/B proof.",
         ),
      },
      AuditFinding {
         status: String::from("fixed"),
         summary: String::from(
            "CollectionView variable measurement caches are now bounded and prune cold key/constraint/revision entries under large churn while preserving prefix-repair A/B coverage.",
         ),
      },
      AuditFinding {
         status: String::from("fixed"),
         summary: String::from(
            "renderer-metal now batches consecutive rounded rectangles through the instanced shader path, reducing per-rect command encoding and parameter binding.",
         ),
      },
      AuditFinding {
         status: String::from("fixed"),
         summary: String::from(
            "renderer-metal now reuses retained scratch buffers across the remaining small batch encode paths instead of allocating per-frame vectors for each batch group.",
         ),
      },
      AuditFinding {
         status: String::from("fixed"),
         summary: String::from(
            "renderer-metal damage prefiltering now borrows source geometry backing storage, preserving small-damage culling without cloning full vertex and index arrays.",
         ),
      },
      AuditFinding {
         status: String::from("candidate"),
         summary: String::from(
            "The macOS glyph indirect-command-buffer path is now default-disabled because Metal validation exposed CPU access to private ICB storage and an invalid ICB pipeline configuration; restoring it with a truly valid text ICB path remains a high-value GPU follow-up.",
         ),
      },
   ];

    Ok(PerfReport {
        version: 1,
        suite: if smoke { String::from("smoke") } else { String::from("full") },
        generated_label: std::env::var("PERF_REPORT_DATE").ok(),
        cases,
        coverage,
        contract,
        findings,
    })
}

fn build_oxide_contract_coverage(cases: &[PerfCaseResult]) -> ContractCoverageReport {
    let has = |prefix: &str| cases.iter().any(|case| case.id.starts_with(prefix));
    let has_case = |needle: &str| cases.iter().any(|case| case.id == needle);
    let layers = vec![
        contract_entry(
            "engine",
            "Engine Microbenchmarks",
            if has("cpu.system.")
                && has("cpu.component.")
                && has("cpu.animation.")
                && has("cpu.primitive.")
                && has("cpu.authoring.")
            {
                "implemented"
            } else {
                "partial"
            },
            vec![String::from(
                "Engine coverage currently spans system hot paths, primitive views, animations, primitive lifecycle slices, and author-facing APIs.",
            )],
        ),
        contract_entry(
            "flow",
            "Representative Screen Flows",
            "partial",
            vec![String::from(
                "Flow coverage now spans offscreen launch/lifecycle, router scenes, explicit CPU user journeys, and a macOS Metal collection-navigation journey frame-pacing row, but physical-device refresh-mode batteries remain incomplete.",
            )],
        ),
        contract_entry(
            "os-bridge",
            "OS-Bridge Benchmarks",
            if has("cpu.bridge.") { "implemented" } else { "missing" },
            vec![String::from(
                "Bridge coverage currently measures only app-owned wrapper overhead, not system-owned surface cost as a renderer win.",
            )],
        ),
    ];
    let has_all = |needles: &[&str]| needles.iter().all(|needle| has_case(needle));
    let battery = vec![
        contract_battery_entry(
            "launch-lifecycle",
            "Launch & Lifecycle",
            has_all(&[
                "cpu.launch.simple_home.cold_launch",
                "cpu.launch.heavy_home.cold_launch",
                "cpu.launch.detail.deep_link_launch",
                "cpu.launch.simple_home.warm_resume",
                "cpu.launch.heavy_home.foreground_after_background",
            ]),
            "Offscreen bootstrap now includes simple-home and heavy-home cold launch, route-driven detail launch, warm resume, and foreground-after-background lifecycle workloads.",
            "No dedicated cold launch, warm resume, deep-link launch, or foreground-after-background battery is wired into oxide-perf-runner yet.",
        ),
        contract_battery_entry(
            "primitive-lifecycle",
            "Primitive Mount / Update / Destroy",
            has_all(&[
                "cpu.primitive.empty_root.mount",
                "cpu.primitive.control_set.mount",
                "cpu.primitive.control_set.mutate_state",
                "cpu.primitive.flat_rects.100.remove_all",
                "cpu.primitive.flat_rects.100.remount",
            ]),
            "Flat rects, labels, cards, images, an empty-root slice, a shared control-set slice, and retained-tree remove-all/remount slices are all covered.",
            "Flat rects, labels, cards, and images cover mount plus mutate, but the empty-root, shared control-set, and retained-tree remove-all/remount slices are still incomplete.",
        ),
        contract_battery_entry(
            "layout-invalidation",
            "Layout & Invalidation",
            has_all(&[
                "cpu.layout.flat_grid.rotation_relayout",
                "cpu.layout.deep_stack.theme_swap",
                "cpu.layout.grid.safe_area_swap",
                "cpu.layout.dirty_subtree.incremental_relayout",
                "cpu.layout.descendant_only.incremental_relayout",
                "cpu.layout.transform_only.reposition",
                "cpu.layout.paint_only.opacity_clip",
                "cpu.layout.node_content_dirty.retained_replay",
                "cpu.layout.non_draw_dirty.retained_reuse",
                "cpu.layout.scoped_tree_mutation.add_remove",
            ]),
            "Flat-grid rotation, deep-stack theme swap, safe-area inset relayout, dirty-subtree relayout, descendant-only relayout, transform-only reposition, paint-only opacity/clip, node content-dirty retained-replay, non-draw dirty retained-reuse, and scoped tree add/remove batteries are all implemented.",
            "Dedicated relayout batteries now exist, but not every required flat/deep/grid invalidation slice is present yet.",
        ),
        contract_battery_entry(
            "text-input",
            "Text & Text Input",
            has_all(&[
                "cpu.system.text_atlas_pressure",
                "cpu.system.text_atlas_dirty_rect_upload",
                "cpu.system.text_fallback_label_encode",
                "cpu.system.wrapped_label_cached_encode",
                "cpu.text_input.large_editor.keystroke_burst",
                "cpu.text_input.large_editor.paste_10kb",
                "cpu.text_input.large_editor.selection_replace",
                "cpu.text_input.ime.composition_commit_cycle",
                "cpu.text_input.cursor_pick.cluster_map",
                "cpu.text_input.cursor_pick.rtl_cluster_map",
                "cpu.text_input.cursor_pick.fallback_cluster_map",
                "cpu.text_input.cursor_pick.mixed_bidi_affinity",
                "cpu.journey.text_ime_composition_cycle",
            ]),
            "Large-editor keystroke, paste, selection-replace, IME composition, LTR/RTL/fallback-font/mixed-bidi cursor-pick cluster-map, fallback-font label encoding, wrapped-label cache-miss fitting, atlas eviction pressure, and dirty-rect atlas upload workloads now complement the text-field and routed input-form coverage.",
            "Text fields, wrapped labels, and the input-form journey are covered, but the full large-editor, IME composition, atlas eviction, and dirty-rect upload battery is still incomplete.",
        ),
        contract_battery_entry(
            "image-pipeline",
            "Image Pipeline",
            has_all(&[
                "cpu.image_pipeline.png.decode",
                "gpu.image_pipeline.png.upload",
                "gpu.image_pipeline.png.first_visible",
            ]),
            "The committed image battery now splits PNG decode, Metal texture upload, and first-visible presentation into separate persisted workloads.",
            "Image view and zoom workloads exist, but decode, upload, and first-visible phases are not yet split into separate benchmark metrics.",
        ),
        contract_battery_entry(
            "lists-grids-chat",
            "Lists, Grids, & Chat",
            has_all(&[
                "cpu.journey.feed_scroll_matrix",
                "cpu.journey.thumbnail_grid_scroll_matrix",
                "cpu.journey.chat_thread_scroll_matrix",
            ]),
            "Feed, thumbnail-grid, and chat-thread scroll matrices now exist alongside the collection encode and navigation slices.",
            "Collection encode and collection-navigation journey coverage exist, but the full feed/grid/chat scroll matrices are still incomplete.",
        ),
        contract_battery_entry(
            "navigation-input",
            "Navigation & Input Latency",
            has_all(&[
                "cpu.navigation.button_press.response",
                "cpu.navigation.slider_scrub.response",
                "cpu.navigation.text_focus.response",
            ]),
            "Direct button-press, slider-scrub, and text-focus response batteries now complement the higher-level journey cases.",
            "Navigation, orchestration, and zoom journeys exist, but direct event-to-first-response latency batteries are still missing.",
        ),
        contract_entry(
            "animation-effects",
            "Animation & Visual Effects",
            if has_case("gpu.animation.effects.refresh_matrix") { "partial" } else { "missing" },
            if has_case("gpu.animation.effects.refresh_matrix") {
                vec![String::from(
                    "A dedicated macOS Metal animation/effects refresh-matrix row now persists direct GPU, missed-frame, and hitch distributions; physical-device refresh-mode coverage remains pending.",
                )]
            } else {
                vec![String::from(
                    "Representative animations exist, but the dedicated hitch-ratio and refresh-mode matrix is absent.",
                )]
            },
        ),
        contract_battery_entry(
            "state-reconcile",
            "State Mutation & Reconciliation",
            has_all(&[
                "cpu.reconcile.single_node_mutation",
                "cpu.reconcile.tree_mutation_1pct",
                "cpu.reconcile.tree_mutation_10pct",
                "cpu.reconcile.theme_swap_full",
            ]),
            "Single-node, 1 percent, 10 percent, and full-theme tree mutation batteries now expose diff/apply cost directly.",
            "Primitive mutations and surface-router composition exist, but there is no dedicated diff/apply battery for 1 percent, 10 percent, or full-theme tree mutation yet.",
        ),
        contract_battery_entry(
            "os-bridge",
            "OS Bridge Overhead",
            has_all(&[
                "cpu.bridge.permission_callback_fanout",
                "cpu.bridge.sensor_location_snapshot",
                "cpu.bridge.bluetooth_cache_update",
                "cpu.bridge.photo_import_thumbnail",
                "cpu.bridge.file_import_render",
                "cpu.bridge.share_payload_prepare",
                "cpu.bridge.local_json_transport_render",
                "cpu.bridge.local_image_transport_render",
            ]),
            "Permission, sensor, photo import, file import, share payload, and localhost transport/render bridge workloads are all covered without claiming system-owned UI as a renderer win.",
            "Permission, location, and Bluetooth wrappers are covered, but photo import, file import, share sheet, and transport/decode/render bridge batteries remain missing.",
        ),
        contract_battery_entry(
            "endurance-thermal",
            "Endurance, Memory, & Thermal Drift",
            has_all(&[
                "cpu.endurance.open_close_heavy_screen.100x",
                "cpu.endurance.tab_switch_heavy.500x",
                "cpu.endurance.idle_animation.600_frames",
            ]),
            "Open/close, tab-switch, and idle-animation endurance loops are now part of the committed Oxide battery.",
            "There is still not a complete long-run open/close, tab-switch, and idle-animation endurance battery in the current Oxide suite.",
        ),
        contract_battery_entry(
            "stress-pathological",
            "Stress & Pathological Regressions",
            has_all(&[
                "cpu.stress.flat_rects.10000.mount",
                "cpu.stress.simultaneous_animations.300",
                "cpu.stress.ticker_100hz",
            ]),
            "Dedicated 10k-node, 300-animation, and 100 Hz ticker traps now complement the router stress scene.",
            "The router stress scene exists, but the explicit 10k-node, 300-animation, and 100 Hz ticker traps are still incomplete.",
        ),
    ];
    ContractCoverageReport {
        layers,
        battery,
        notes: vec![
            String::from(
                "The Oxide report is intentionally explicit about missing contract families so the battery does not over-claim comprehensiveness.",
            ),
            String::from(
                "Current Oxide coverage now spans engine hot paths, launch/lifecycle, representative scenes, and bridge slices; the biggest remaining gaps are hitch-oriented flow metrics and real-device refresh-mode coverage.",
            ),
        ],
    }
}

fn contract_entry(
    id: &str,
    label: &str,
    status: &str,
    notes: Vec<String>,
) -> ContractCoverageEntry {
    ContractCoverageEntry {
        id: id.to_string(),
        label: label.to_string(),
        status: status.to_string(),
        notes,
    }
}

fn contract_battery_entry(
    id: &str,
    label: &str,
    implemented: bool,
    implemented_note: &str,
    partial_note: &str,
) -> ContractCoverageEntry {
    contract_entry(
        id,
        label,
        if implemented { "implemented" } else { "partial" },
        vec![String::from(if implemented { implemented_note } else { partial_note })],
    )
}

fn push_system_cases(cases: &mut Vec<PerfCaseResult>, smoke: bool) -> Result<()> {
    let prepare_template = build_prepare_drawlist();
    let coalesce_template = build_coalesce_items();
    let gesture_events = build_gesture_events();
    let touch_surface_events = build_touch_surface_events();

    let prepare_loops = if smoke { 64 } else { 256 };
    let coalesce_loops = if smoke { 32 } else { 128 };
    let gesture_loops = if smoke { 96 } else { 384 };
    let timer_loops = if smoke { 32 } else { 128 };
    let text_loops = if smoke { 8 } else { 24 };

    if perf_case_allowed("cpu.system.prepare_draws.current") {
        cases.push(measure_cpu_case(
            "cpu.system.prepare_draws.current",
            "system",
            smoke,
            true,
            0.12,
            prepare_loops,
            vec![String::from("Current ui-core clip lowering path.")],
            move || ui::prepare_draws(&prepare_template).len() as u64,
        ));
    }

    if perf_case_allowed("cpu.system.coalesce_adjacent_draws.current") {
        let mut coalesce_scratch = Vec::new();
        cases.push(measure_cpu_case(
            "cpu.system.coalesce_adjacent_draws.current",
            "system",
            smoke,
            true,
            0.12,
            coalesce_loops,
            vec![String::from("Current linear ui-core merge path with retained host scratch.")],
            move || {
                let mut list =
                    api::DrawList { items: coalesce_template.clone(), ..api::DrawList::default() };
                ui::coalesce_adjacent_draws_reuse(&mut list, &mut coalesce_scratch);
                list.items.len() as u64
            },
        ));
    }

    if perf_case_allowed("cpu.system.gesture_sequence") {
        cases.push(measure_cpu_case(
            "cpu.system.gesture_sequence",
            "system",
            smoke,
            true,
            0.12,
            gesture_loops,
            vec![String::from("Tap, long-press, pan, and double-tap sequence.")],
            move || run_gesture_sequence(&gesture_events),
        ));
    }

    if perf_case_allowed("cpu.system.touch_surface_sequence") {
        cases.push(measure_cpu_case(
            "cpu.system.touch_surface_sequence",
            "system",
            smoke,
            true,
            0.12,
            gesture_loops,
            vec![String::from("Raw one-touch pan plus two-touch pinch surface sequence.")],
            move || run_touch_surface_sequence(&touch_surface_events),
        ));
    }

    if perf_case_allowed("cpu.system.timer_schedule_advance") {
        cases.push(measure_cpu_case(
            "cpu.system.timer_schedule_advance",
            "system",
            smoke,
            true,
            0.12,
            timer_loops,
            vec![String::from("Schedule and advance a compact batch of due timers.")],
            run_timer_schedule_advance,
        ));
    }

    if perf_case_allowed("cpu.system.anim_start_replace") {
        cases.push(measure_cpu_case(
            "cpu.system.anim_start_replace",
            "system",
            smoke,
            true,
            0.12,
            timer_loops,
            vec![String::from("Start and replace compact property animation batches.")],
            run_anim_start_replace,
        ));
    }

    if perf_case_allowed("cpu.system.text_shape_bake") {
        cases.push(measure_cpu_case(
            "cpu.system.text_shape_bake",
            "system",
            smoke,
            true,
            0.12,
            text_loops,
            vec![String::from("Mixed Latin+CJK shaping and atlas bake.")],
            move || run_text_shape_bake(),
        ));
    }

    if perf_case_allowed("cpu.system.text_prefix_width_map") {
        cases.push(text_prefix_width_map_case(smoke, text_loops));
    }

    if perf_case_allowed("cpu.system.text_fallback_label_encode") {
        cases.push(text_fallback_label_encode_case(smoke, text_loops));
    }

    if perf_case_allowed("cpu.system.text_atlas_pressure") {
        cases.push(text_atlas_pressure_case(smoke, text_loops));
    }

    if perf_case_allowed("cpu.system.text_atlas_dirty_rect_upload") {
        cases.push(text_atlas_dirty_rect_upload_case(smoke, text_loops));
    }

    if perf_case_allowed("cpu.system.wrapped_label_cached_encode") {
        cases.push(wrapped_label_cached_encode_case(smoke, text_loops));
    }

    if perf_case_allowed("cpu.system.picker_text_cached_encode") {
        cases.push(picker_text_cached_encode_case(smoke, text_loops));
    }

    if perf_case_allowed("gpu.system.id_mask_compositor.current") {
        cases.push(gpu_system_id_mask_compositor_case(smoke)?);
    }

    Ok(())
}

fn id_mask_perf_vertices(
    cells: usize,
    extent: f32,
) -> Vec<metal::id_mask_compositor::IdMaskRasterVertex> {
    let mut vertices = Vec::with_capacity(cells * cells * 6);
    let step = extent / cells as f32;
    let origin = -extent * 0.5;
    for y in 0..cells {
        for x in 0..cells {
            let x0 = origin + x as f32 * step;
            let y0 = origin + y as f32 * step;
            let x1 = x0 + step;
            let y1 = y0 + step;
            let city = ((x + y) & 3) as u8;
            let neighborhood = ((x * 3 + y * 5) & 31) as u8;
            vertices.push(metal::id_mask_compositor::IdMaskRasterVertex::new(
                [x0, y0],
                city,
                neighborhood,
            ));
            vertices.push(metal::id_mask_compositor::IdMaskRasterVertex::new(
                [x1, y0],
                city,
                neighborhood,
            ));
            vertices.push(metal::id_mask_compositor::IdMaskRasterVertex::new(
                [x0, y1],
                city,
                neighborhood,
            ));
            vertices.push(metal::id_mask_compositor::IdMaskRasterVertex::new(
                [x1, y0],
                city,
                neighborhood,
            ));
            vertices.push(metal::id_mask_compositor::IdMaskRasterVertex::new(
                [x1, y1],
                city,
                neighborhood,
            ));
            vertices.push(metal::id_mask_compositor::IdMaskRasterVertex::new(
                [x0, y1],
                city,
                neighborhood,
            ));
        }
    }
    vertices
}

fn id_mask_perf_pass<'a>(
    vertices: &'a [metal::id_mask_compositor::IdMaskRasterVertex],
    chunks: &'a [metal::id_mask_compositor::IdMaskRasterChunk],
    revision: u64,
) -> metal::id_mask_compositor::IdMaskGpuCompositorPass<'a> {
    let city_styles = [
        metal::id_mask_compositor::IdMaskCityStyle {
            fill_rgb: [0.95, 0.26, 0.22],
            edge_rgb: [0.58, 0.10, 0.10],
            seam_rgb: [1.0, 0.78, 0.32],
        },
        metal::id_mask_compositor::IdMaskCityStyle {
            fill_rgb: [0.15, 0.55, 0.95],
            edge_rgb: [0.05, 0.18, 0.42],
            seam_rgb: [0.70, 0.90, 1.0],
        },
        metal::id_mask_compositor::IdMaskCityStyle {
            fill_rgb: [0.20, 0.72, 0.38],
            edge_rgb: [0.06, 0.26, 0.11],
            seam_rgb: [0.75, 1.0, 0.62],
        },
        metal::id_mask_compositor::IdMaskCityStyle {
            fill_rgb: [0.72, 0.36, 0.92],
            edge_rgb: [0.28, 0.12, 0.44],
            seam_rgb: [0.94, 0.74, 1.0],
        },
    ];
    let mut neighborhood_colors =
        [[0.0_f32; 3]; metal::id_mask_compositor::ID_MASK_MAX_NEIGHBORHOOD_COLORS];
    for (index, color) in neighborhood_colors.iter_mut().enumerate() {
        let t = index as f32 / metal::id_mask_compositor::ID_MASK_MAX_NEIGHBORHOOD_COLORS as f32;
        *color = [0.20 + t * 0.55, 0.34 + (1.0 - t) * 0.38, 0.52 + (t * 0.27)];
    }

    metal::id_mask_compositor::IdMaskGpuCompositorPass {
        raster: metal::id_mask_compositor::IdMaskGpuRasterPass {
            viewport: api::RectF::new(0.0, 0.0, 256.0, 256.0),
            mask_width: 512,
            mask_height: 512,
            mask_scale: 2.0,
            vertex_revision: revision,
            vertices,
            chunks,
            projection: metal::id_mask_compositor::IdMaskRasterProjection::screen_px(),
        },
        city_styles,
        neighborhood_colors,
        mode: metal::id_mask_compositor::IdMaskCompositorMode::Beauty,
        glow_enabled: false,
        darken_background_alpha: 0.0,
        polish: metal::id_mask_compositor::IdMaskPolishConfig::default(),
    }
}

fn last_metal_stats_after_submit(
    renderer: &metal::MetalRenderer,
    frame_id: u64,
) -> metal::PerfStats {
    let mut stats = renderer.last_stats();
    for _ in 0..10 {
        if stats.gpu_frame_id == frame_id {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
        stats = renderer.last_stats();
    }
    stats
}

fn gpu_system_id_mask_compositor_case(smoke: bool) -> Result<PerfCaseResult> {
    const ID: &str = "gpu.system.id_mask_compositor.current";

    let mut renderer =
        Box::new(metal::MetalRenderer::new_default().context("creating Metal renderer")?);
    renderer.resize(512, 512, 2.0).context("resizing Metal renderer")?;
    let vertices = id_mask_perf_vertices(if smoke { 40 } else { 72 }, 256.0);
    let vertex_bytes =
        vertices.len() * core::mem::size_of::<metal::id_mask_compositor::IdMaskRasterVertex>();
    let warmups = 4;
    let frames = if smoke { 8 } else { 24 };
    let mut frame_samples = Vec::with_capacity(frames);
    let mut encode_samples = Vec::with_capacity(frames);
    let mut gpu_samples = Vec::with_capacity(frames);
    let mut draws_sum = 0.0;
    let mut skipped_sum = 0.0;

    for index in 0..(warmups + frames) {
        let revision = 1;
        let chunks = [metal::id_mask_compositor::IdMaskRasterChunk {
            content_hash: revision,
            first_vertex: 0,
            vertex_count: vertices.len(),
        }];
        let pass = id_mask_perf_pass(&vertices, &chunks, revision);
        let frame_t0 = Instant::now();
        let token = renderer.begin_frame(&api::FrameTarget, None);
        let frame_id = token.0;
        renderer
            .encode_id_mask_gpu_compositor(&pass)
            .with_context(|| format!("encoding {}", ID))?;
        renderer.submit(token).with_context(|| format!("submitting {}", ID))?;
        let frame_ms = frame_t0.elapsed().as_secs_f64() * 1000.0;
        let stats = last_metal_stats_after_submit(&renderer, frame_id);
        if index >= warmups {
            frame_samples.push(frame_ms);
            encode_samples.push(stats.encode_ms);
            gpu_samples.push(stats.gpu_ms);
            draws_sum += stats.draws as f64;
            skipped_sum += stats.frame_backpressure_skipped as f64;
        }
    }

    let summary = summarize(&frame_samples);
    let (layer, scenario, variant, cache_state, refresh_mode) =
        perf_case_contract_metadata(ID, "system");
    let mut metrics = BTreeMap::new();
    metrics.insert(String::from("encode_ms_median"), summarize(&encode_samples).median);
    metrics.insert(String::from("draws_avg"), draws_sum / frames as f64);
    metrics.insert(String::from("frame_backpressure_skips"), skipped_sum);
    insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
    insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
    insert_frame_pacing_metrics(&mut metrics, &frame_samples);
    metrics.insert(String::from("vertex_count"), vertices.len() as f64);
    metrics.insert(String::from("vertex_bytes"), vertex_bytes as f64);
    metrics.insert(String::from("revision_changes_per_frame"), 0.0);

    Ok(PerfCaseResult {
        id: String::from(ID),
        family: String::from("system"),
        layer: String::from(layer),
        scenario: String::from(scenario),
        variant: String::from(variant),
        cache_state: String::from(cache_state),
        refresh_mode: String::from(refresh_mode),
        unit: String::from("ms/frame"),
        gated: true,
        threshold_pct: 0.20,
        median: summary.median,
        p95: summary.p95,
        p99: summary.p99,
        min: summary.min,
        max: summary.max,
        mean: summary.mean,
        samples: frame_samples.len(),
        ops_per_sample: 1,
        notes: vec![String::from(
            "Current macOS Metal id-mask compositor path with stable vertex_revision, proving cached raster vertex uploads.",
        )],
        metrics,
    })
}

fn push_component_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) {
    for spec in registry::components() {
        covered.insert(spec.name.to_string());
        let case = match spec.id {
            registry::ComponentId::Label => component_label_case(smoke),
            registry::ComponentId::ProgressBar => component_progress_case(smoke),
            registry::ComponentId::Spinner => component_spinner_case(smoke),
            registry::ComponentId::Button => component_button_case(smoke),
            registry::ComponentId::Toggle => component_toggle_case(smoke),
            registry::ComponentId::Slider => component_slider_case(smoke),
            registry::ComponentId::ImageView => component_image_case(smoke),
            registry::ComponentId::NineSliceImage => component_nine_slice_case(smoke),
            registry::ComponentId::CollectionView => component_collection_case(smoke),
        };
        cases.push(case);
    }
}

fn push_animation_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) {
    for spec in registry::animations() {
        covered.insert(spec.name.to_string());
        let case = match spec.id {
            registry::AnimationId::SpinnerSpin => animation_spinner_case(smoke),
            registry::AnimationId::ProgressIndeterminate => animation_progress_case(smoke),
            registry::AnimationId::ButtonPressScale => animation_button_case(smoke),
            registry::AnimationId::ToggleThumbSpring => animation_toggle_case(smoke),
            registry::AnimationId::SliderThumbMove => animation_slider_case(smoke),
            registry::AnimationId::ImageZoomPan => animation_image_zoom_case(smoke),
            registry::AnimationId::AnimTimelineBars => animation_timeline_case(smoke),
        };
        cases.push(case);
    }
}

fn push_gpu_animation_cases(cases: &mut Vec<PerfCaseResult>, smoke: bool) -> Result<()> {
    if perf_case_allowed("gpu.animation.effects.refresh_matrix") {
        cases.push(gpu_animation_effects_refresh_matrix_case(smoke)?);
    }
    Ok(())
}

fn push_launch_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for spec in PERF_LAUNCH_SPECS {
        if !perf_case_allowed(spec.id) {
            continue;
        }
        covered.insert(spec.name.to_string());
        let case = match spec.id {
            "cpu.launch.simple_home.cold_launch" => launch_simple_home_cold_case(smoke),
            "cpu.launch.heavy_home.cold_launch" => launch_heavy_home_cold_case(smoke),
            "cpu.launch.detail.deep_link_launch" => launch_detail_deep_link_case(smoke),
            "cpu.launch.simple_home.warm_resume" => launch_simple_home_warm_resume_case(smoke),
            "cpu.launch.heavy_home.foreground_after_background" => {
                launch_heavy_home_foreground_case(smoke)
            }
            other => bail!("unknown launch perf case `{}`", other),
        };
        cases.push(case?);
    }
    Ok(())
}

fn push_primitive_lifecycle_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for spec in PERF_PRIMITIVE_LIFECYCLE_SPECS {
        if !perf_case_allowed(spec.id) {
            continue;
        }
        covered.insert(spec.name.to_string());
        let case = match spec.kind {
            PrimitiveLifecycleKind::EmptyRoot => primitive_empty_root_case(spec, smoke)?,
            PrimitiveLifecycleKind::FlatRects => primitive_flat_rects_case(spec, smoke)?,
            PrimitiveLifecycleKind::Labels => primitive_labels_case(spec, smoke)?,
            PrimitiveLifecycleKind::Cards => primitive_cards_case(spec, smoke)?,
            PrimitiveLifecycleKind::Images => primitive_images_case(spec, smoke)?,
            PrimitiveLifecycleKind::ControlSet => primitive_control_set_case(spec, smoke)?,
        };
        cases.push(case);
    }
    Ok(())
}

fn push_cpu_scene_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for spec in PERF_SCENE_SPECS {
        let case_id = format!("cpu.scene.{}.frame", spec.slug);
        if !perf_case_allowed(&case_id) {
            continue;
        }
        covered.insert(spec.name.to_string());
        cases.push(cpu_scene_case(spec, smoke)?);
    }
    Ok(())
}

fn push_gpu_scene_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for spec in PERF_SCENE_SPECS {
        let case_id = format!("gpu.scene.{}.frame", spec.slug);
        if !perf_case_allowed(&case_id) {
            continue;
        }
        covered.insert(spec.name.to_string());
        cases.push(gpu_scene_case(spec, smoke)?);
    }
    Ok(())
}

fn push_journey_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for spec in PERF_JOURNEY_SPECS {
        if !perf_case_allowed(spec.id) {
            continue;
        }
        covered.insert(spec.name.to_string());
        let case = match spec.id {
            "cpu.journey.input_form_submit" => journey_input_form_case(smoke),
            "cpu.journey.text_ime_composition_cycle" => journey_text_ime_composition_case(smoke),
            "cpu.journey.collection_navigation" => journey_collection_navigation_case(smoke),
            "cpu.journey.zoom_image_gesture_cycle" => journey_zoom_image_case(smoke),
            "cpu.journey.orchestration_transition_modal" => journey_orchestration_case(smoke),
            "cpu.journey.feed_scroll_matrix" => journey_feed_scroll_case(smoke),
            "cpu.journey.thumbnail_grid_scroll_matrix" => journey_thumbnail_grid_scroll_case(smoke),
            "cpu.journey.chat_thread_scroll_matrix" => journey_chat_thread_scroll_case(smoke),
            other => bail!("unknown journey perf case `{}`", other),
        };
        cases.push(case?);
    }
    Ok(())
}

fn push_gpu_journey_cases(cases: &mut Vec<PerfCaseResult>, smoke: bool) -> Result<()> {
    for spec in PERF_GPU_JOURNEY_SPECS {
        if !perf_case_allowed(spec.id) {
            continue;
        }
        let case = match spec.id {
            "gpu.journey.collection_navigation.frame_pacing" => {
                gpu_journey_collection_navigation_frame_pacing_case(smoke)?
            }
            other => bail!("unknown gpu journey perf case `{}`", other),
        };
        cases.push(case);
    }
    Ok(())
}

fn push_authoring_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for spec in PERF_AUTHORING_SPECS {
        if !perf_case_allowed(spec.id) {
            continue;
        }
        covered.insert(spec.name.to_string());
        let case = match spec.id {
            "cpu.authoring.text_fields.edit_cycle" => authoring_text_fields_case(smoke),
            POPUP_WHEEL_PICKER_CASE_ID => authoring_popup_wheel_picker_case(smoke),
            "cpu.authoring.burst_emitter.sample" => authoring_burst_emitter_case(smoke),
            "cpu.authoring.surface_router.compose" => authoring_surface_router_case(smoke),
            "cpu.authoring.surface_retained.clean_encode" => {
                authoring_surface_retained_clean_encode_case(smoke)
            }
            "cpu.authoring.surface_retained.dirty_leaf_encode" => {
                authoring_surface_retained_dirty_leaf_encode_case(smoke)
            }
            "cpu.authoring.surface_retained.text_atlas_context" => {
                authoring_surface_retained_text_atlas_context_case(smoke)
            }
            "cpu.authoring.surface_retained.cache_policy" => {
                authoring_surface_retained_cache_policy_case(smoke)
            }
            "cpu.authoring.animation.dynamic_properties_300" => {
                architecture_matrix::authoring_dynamic_property_surface_case(smoke)
            }
            "cpu.authoring.retained_snapshot.spatial_query_10000" => {
                architecture_matrix::retained_spatial_query_case(spec.id, smoke)
            }
            "cpu.authoring.surface.retained_damage_dirty_leaf_10000" => {
                architecture_matrix::retained_surface_dirty_case(spec.id, "authoring", smoke)
            }
            "cpu.authoring.drawlist_text_replay.multi_atlas" => {
                authoring_drawlist_text_replay_multi_atlas_case(smoke)
            }
            "cpu.authoring.collection_key_reconcile.indexed" => {
                authoring_collection_key_reconcile_case(smoke, true)
            }
            "cpu.authoring.collection_key_reconcile.scan" => {
                authoring_collection_key_reconcile_case(smoke, false)
            }
            "cpu.authoring.collection_measure_cache.bounded_churn" => {
                authoring_collection_measure_cache_bounded_churn_case(smoke)
            }
            "cpu.authoring.collection_prefix_update.incremental" => {
                authoring_collection_prefix_update_case(smoke, true)
            }
            "cpu.authoring.collection_prefix_update.full_scan" => {
                authoring_collection_prefix_update_case(smoke, false)
            }
            "gpu.authoring.retained_snapshot.clean_mixed" => {
                architecture_matrix::metal_prepared_chunk_case(spec.id, smoke, false)?
            }
            "gpu.authoring.retained_snapshot.spatial_damage_10000" => {
                architecture_matrix::metal_spatial_damage_case(spec.id, smoke, false)?
            }
            "gpu.authoring.scene3d.mixed_frame" => authoring_scene3d_mixed_frame_case(smoke)?,
            "gpu.authoring.image_store.atlas_grid_1000" => {
                architecture_matrix::metal_image_store_case(spec.id, smoke, 1_000, true)?
            }
            other => bail!("unknown authoring perf case `{}`", other),
        };
        cases.push(case);
    }
    Ok(())
}

fn push_layout_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for spec in PERF_LAYOUT_SPECS {
        if !perf_case_allowed(spec.id) {
            continue;
        }
        covered.insert(spec.name.to_string());
        let case = match spec.id {
            "cpu.layout.flat_grid.rotation_relayout" => layout_flat_grid_rotation_case(smoke),
            "cpu.layout.deep_stack.theme_swap" => layout_deep_stack_theme_swap_case(smoke),
            "cpu.layout.grid.safe_area_swap" => layout_grid_safe_area_case(smoke),
            "cpu.layout.dirty_subtree.incremental_relayout" => {
                layout_dirty_subtree_incremental_case(smoke)
            }
            "cpu.layout.descendant_only.incremental_relayout" => {
                layout_descendant_only_incremental_case(smoke)
            }
            "cpu.layout.transform_only.reposition" => layout_transform_only_reposition_case(smoke),
            "cpu.layout.paint_only.opacity_clip" => layout_paint_only_opacity_clip_case(smoke),
            "cpu.layout.node_content_dirty.retained_replay" => {
                layout_node_content_dirty_retained_replay_case(smoke)
            }
            "cpu.layout.non_draw_dirty.retained_reuse" => {
                layout_non_draw_dirty_retained_reuse_case(smoke)
            }
            "cpu.layout.scoped_tree_mutation.add_remove" => {
                layout_scoped_tree_mutation_add_remove_case(smoke)
            }
            other => bail!("unknown layout perf case `{}`", other),
        };
        cases.push(case);
    }
    Ok(())
}

fn push_text_input_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for spec in PERF_TEXT_INPUT_SPECS {
        if !perf_case_allowed(spec.id) {
            continue;
        }
        covered.insert(spec.name.to_string());
        let case = match spec.id {
            "cpu.text_input.large_editor.keystroke_burst" => {
                text_input_large_editor_keystroke_case(smoke)
            }
            "cpu.text_input.large_editor.paste_10kb" => text_input_large_editor_paste_case(smoke),
            "cpu.text_input.large_editor.selection_replace" => {
                text_input_large_editor_selection_case(smoke)
            }
            "cpu.text_input.ime.composition_commit_cycle" => text_input_ime_composition_case(smoke),
            "cpu.text_input.cursor_pick.cluster_map" => {
                text_input_cursor_pick_cluster_map_case(smoke)
            }
            "cpu.text_input.cursor_pick.rtl_cluster_map" => {
                text_input_cursor_pick_rtl_cluster_map_case(smoke)
            }
            "cpu.text_input.cursor_pick.fallback_cluster_map" => {
                text_input_cursor_pick_fallback_cluster_map_case(smoke)
            }
            "cpu.text_input.cursor_pick.mixed_bidi_affinity" => {
                text_input_cursor_pick_mixed_bidi_affinity_case(smoke)
            }
            other => bail!("unknown text-input perf case `{}`", other),
        };
        cases.push(case);
    }
    Ok(())
}

fn push_image_pipeline_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for spec in PERF_IMAGE_PIPELINE_SPECS {
        if !perf_case_allowed(spec.id) {
            continue;
        }
        covered.insert(spec.name.to_string());
        let case = match spec.id {
            "cpu.image_pipeline.png.decode" => image_pipeline_png_decode_case(smoke)?,
            "gpu.image_pipeline.png.upload" => image_pipeline_png_upload_case(smoke)?,
            "gpu.image_pipeline.png.first_visible" => image_pipeline_png_first_visible_case(smoke)?,
            other => bail!("unknown image-pipeline perf case `{}`", other),
        };
        cases.push(case);
    }
    Ok(())
}

fn push_endurance_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for spec in PERF_ENDURANCE_SPECS {
        if !perf_case_allowed(spec.id) {
            continue;
        }
        covered.insert(spec.name.to_string());
        let case = match spec.id {
            "cpu.endurance.open_close_heavy_screen.100x" => endurance_open_close_case(smoke),
            "cpu.endurance.tab_switch_heavy.500x" => endurance_tab_switch_case(smoke),
            "cpu.endurance.idle_animation.600_frames" => endurance_idle_animation_case(smoke),
            other => bail!("unknown endurance perf case `{}`", other),
        };
        cases.push(case);
    }
    Ok(())
}

fn push_navigation_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for spec in PERF_NAVIGATION_SPECS {
        if !perf_case_allowed(spec.id) {
            continue;
        }
        covered.insert(spec.name.to_string());
        let case = match spec.id {
            "cpu.navigation.button_press.response" => navigation_button_press_case(smoke),
            "cpu.navigation.slider_scrub.response" => navigation_slider_scrub_case(smoke),
            "cpu.navigation.text_focus.response" => navigation_text_focus_case(smoke),
            other => bail!("unknown navigation perf case `{}`", other),
        };
        cases.push(case);
    }
    Ok(())
}

fn push_reconcile_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for spec in PERF_RECONCILE_SPECS {
        if !perf_case_allowed(spec.id) {
            continue;
        }
        covered.insert(spec.name.to_string());
        let case = match spec.id {
            "cpu.reconcile.single_node_mutation" => reconcile_single_node_case(smoke)?,
            "cpu.reconcile.tree_mutation_1pct" => reconcile_tree_mutation_case(smoke, 10)?,
            "cpu.reconcile.tree_mutation_10pct" => reconcile_tree_mutation_case(smoke, 100)?,
            "cpu.reconcile.theme_swap_full" => reconcile_theme_swap_case(smoke)?,
            other => bail!("unknown reconcile perf case `{}`", other),
        };
        cases.push(case);
    }
    Ok(())
}

fn push_stress_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for spec in PERF_STRESS_SPECS {
        if !perf_case_allowed(spec.id) {
            continue;
        }
        covered.insert(spec.name.to_string());
        let case = match spec.id {
            "cpu.stress.flat_rects.10000.mount" => stress_flat_rects_mount_case(smoke)?,
            "cpu.stress.simultaneous_animations.300" => stress_simultaneous_animations_case(smoke),
            "cpu.stress.ticker_100hz" => stress_ticker_case(smoke),
            other => bail!("unknown stress perf case `{}`", other),
        };
        cases.push(case);
    }
    Ok(())
}

fn push_bridge_cases(
    cases: &mut Vec<PerfCaseResult>,
    smoke: bool,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    for spec in PERF_BRIDGE_SPECS {
        if !perf_case_allowed(spec.id) {
            continue;
        }
        covered.insert(spec.name.to_string());
        let case = match spec.id {
            "cpu.bridge.permission_callback_fanout" => bridge_permission_callback_case(smoke),
            "cpu.bridge.web_backend_surface" => bridge_web_backend_surface_case(smoke),
            "cpu.bridge.sensor_location_snapshot" => bridge_sensor_location_case(smoke),
            "cpu.bridge.bluetooth_cache_update" => bridge_bluetooth_cache_case(smoke),
            "cpu.bridge.photo_import_thumbnail" => bridge_photo_import_thumbnail_case(smoke)?,
            "cpu.bridge.file_import_render" => bridge_file_import_render_case(smoke),
            "cpu.bridge.share_payload_prepare" => bridge_share_payload_prepare_case(smoke),
            "cpu.bridge.local_json_transport_render" => {
                bridge_local_json_transport_render_case(smoke)
            }
            "cpu.bridge.local_image_transport_render" => {
                bridge_local_image_transport_render_case(smoke)?
            }
            other => bail!("unknown bridge perf case `{}`", other),
        };
        cases.push(case);
    }
    Ok(())
}

fn launch_scene_frame(scene_index: usize) -> u64 {
    let viewport = api::RectF::new(
        0.0,
        0.0,
        PERF_SCENE_W as f32 / PERF_DEVICE_SCALE,
        PERF_SCENE_H as f32 / PERF_DEVICE_SCALE,
    );
    let mut router = prepare_cpu_router();
    let mut builder = ui::DrawListBuilder::new();
    let mut now = 0u64;
    router.set_scene(scene_index);
    advance_cpu_router_frame(&mut router, &mut builder, viewport, &mut now)
}

fn launch_scene_resume(scene_index: usize) -> u64 {
    let viewport = api::RectF::new(
        0.0,
        0.0,
        PERF_SCENE_W as f32 / PERF_DEVICE_SCALE,
        PERF_SCENE_H as f32 / PERF_DEVICE_SCALE,
    );
    let mut router = prepare_cpu_router();
    let mut builder = ui::DrawListBuilder::new();
    let mut now = 0u64;
    router.set_scene(scene_index);
    let mut checksum = advance_cpu_router_frame(&mut router, &mut builder, viewport, &mut now);
    now = now.saturating_add(250);
    checksum = checksum.wrapping_add(advance_cpu_router_frame(
        &mut router,
        &mut builder,
        viewport,
        &mut now,
    ));
    checksum
}

fn launch_simple_home_cold_case(smoke: bool) -> Result<PerfCaseResult> {
    measure_journey_case(
        "cpu.launch.simple_home.cold_launch",
        smoke,
        0.15,
        vec![String::from(
            "Offscreen Oxide bootstrap into the Controls scene and first presented frame.",
        )],
        || launch_scene_frame(0),
    )
}

fn launch_heavy_home_cold_case(smoke: bool) -> Result<PerfCaseResult> {
    measure_journey_case(
        "cpu.launch.heavy_home.cold_launch",
        smoke,
        0.15,
        vec![String::from(
            "Offscreen Oxide bootstrap into the Collection Stress scene and first presented frame.",
        )],
        || launch_scene_frame(4),
    )
}

fn launch_detail_deep_link_case(smoke: bool) -> Result<PerfCaseResult> {
    measure_journey_case(
        "cpu.launch.detail.deep_link_launch",
        smoke,
        0.15,
        vec![String::from(
            "Route-driven offscreen Oxide bootstrap into the Integration scene as the current detail-launch proxy.",
        )],
        || launch_scene_frame(15),
    )
}

fn launch_simple_home_warm_resume_case(smoke: bool) -> Result<PerfCaseResult> {
    measure_journey_case(
        "cpu.launch.simple_home.warm_resume",
        smoke,
        0.15,
        vec![String::from(
            "Warm resume proxy over the Controls scene after one prior Oxide frame has already been prepared.",
        )],
        || launch_scene_resume(0),
    )
}

fn launch_heavy_home_foreground_case(smoke: bool) -> Result<PerfCaseResult> {
    measure_journey_case(
        "cpu.launch.heavy_home.foreground_after_background",
        smoke,
        0.15,
        vec![String::from(
            "Foreground-after-background proxy over the Collection Stress scene after one prior Oxide frame has already been prepared.",
        )],
        || launch_scene_resume(4),
    )
}

fn primitive_lifecycle_iterations(kind: PrimitiveLifecycleKind, count: usize, smoke: bool) -> u64 {
    match (kind, smoke, count) {
        (PrimitiveLifecycleKind::EmptyRoot, true, _) => 24,
        (PrimitiveLifecycleKind::EmptyRoot, false, _) => 96,
        (PrimitiveLifecycleKind::FlatRects, true, 10) => 8,
        (PrimitiveLifecycleKind::FlatRects, true, 100) => 4,
        (PrimitiveLifecycleKind::FlatRects, true, _) => 2,
        (PrimitiveLifecycleKind::FlatRects, false, 10) => 24,
        (PrimitiveLifecycleKind::FlatRects, false, 100) => 12,
        (PrimitiveLifecycleKind::FlatRects, false, _) => 6,
        (PrimitiveLifecycleKind::Labels, true, 10) => 4,
        (PrimitiveLifecycleKind::Labels, true, 100) => 2,
        (PrimitiveLifecycleKind::Labels, true, _) => 1,
        (PrimitiveLifecycleKind::Labels, false, 10) => 12,
        (PrimitiveLifecycleKind::Labels, false, 100) => 6,
        (PrimitiveLifecycleKind::Labels, false, _) => 2,
        (PrimitiveLifecycleKind::Cards, true, 10) => 6,
        (PrimitiveLifecycleKind::Cards, true, _) => 3,
        (PrimitiveLifecycleKind::Cards, false, 10) => 18,
        (PrimitiveLifecycleKind::Cards, false, _) => 8,
        (PrimitiveLifecycleKind::Images, true, 10) => 6,
        (PrimitiveLifecycleKind::Images, true, _) => 3,
        (PrimitiveLifecycleKind::Images, false, 10) => 18,
        (PrimitiveLifecycleKind::Images, false, _) => 8,
        (PrimitiveLifecycleKind::ControlSet, true, _) => 10,
        (PrimitiveLifecycleKind::ControlSet, false, _) => 32,
    }
}

fn primitive_empty_root_case(spec: &PrimitiveLifecycleSpec, smoke: bool) -> Result<PerfCaseResult> {
    let loops = primitive_lifecycle_iterations(spec.kind, spec.count, smoke);
    let notes =
        vec![String::from("Retained-tree empty-root mount slice for control-free surface setup.")];
    Ok(measure_cpu_case(
        spec.id,
        "primitive-lifecycle",
        smoke,
        true,
        0.10,
        loops,
        notes,
        move || {
            let mut builder = ui::DrawListBuilder::new();
            let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(420.0));
            surface.layout(420.0, 760.0);
            surface.encode(&mut builder);
            let dl = builder.drawlist();
            (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
        },
    ))
}

fn primitive_flat_rects_case(spec: &PrimitiveLifecycleSpec, smoke: bool) -> Result<PerfCaseResult> {
    let loops = primitive_lifecycle_iterations(spec.kind, spec.count, smoke);
    let count = spec.count;
    let count_f = count as f32;
    let viewport_w: f32 = if count <= 100 { 360.0 } else { 420.0 };
    let viewport_h: f32 = if count <= 100 { 420.0 } else { 760.0 };
    let notes = vec![format!(
        "Retained-tree flat-rect grid {} case at {} visible nodes.",
        spec.op.label(),
        spec.count
    )];
    match spec.op {
        PrimitiveLifecycleOp::Mount => {
            let mut builder = ui::DrawListBuilder::new();
            Ok(measure_cpu_case(
                spec.id,
                "primitive-lifecycle",
                smoke,
                true,
                0.18,
                loops,
                notes,
                move || {
                    builder.clear();
                    let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(viewport_w));
                    populate_flat_rect_surface(&mut surface, count, 0);
                    surface.layout(viewport_w, viewport_h.max((count_f / 10.0).ceil() * 28.0));
                    surface.encode(&mut builder);
                    let dl = builder.drawlist();
                    (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
                },
            ))
        }
        PrimitiveLifecycleOp::Mutate => {
            let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(viewport_w));
            let nodes = populate_flat_rect_surface(&mut surface, count, 0);
            surface.layout(viewport_w, viewport_h.max((count_f / 10.0).ceil() * 28.0));
            let mut builder = ui::DrawListBuilder::new();
            let mut palette_phase = 0usize;
            Ok(measure_cpu_case(
                spec.id,
                "primitive-lifecycle",
                smoke,
                true,
                0.18,
                loops,
                notes,
                move || {
                    palette_phase = palette_phase.wrapping_add(1);
                    for (index, node) in nodes.cells.iter().copied().enumerate() {
                        let Some(style) = surface.tree_mut().style_mut(node) else { continue };
                        style.background = flat_rect_fill_color(index, palette_phase);
                        style.opacity = 0.72 + ((index + palette_phase) % 5) as f32 * 0.05;
                    }
                    builder.clear();
                    surface.encode(&mut builder);
                    let dl = builder.drawlist();
                    (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
                },
            ))
        }
        PrimitiveLifecycleOp::RemoveAll => {
            let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(viewport_w));
            let mut nodes = populate_flat_rect_surface(&mut surface, count, 0);
            surface.layout(viewport_w, viewport_h.max((count_f / 10.0).ceil() * 28.0));
            let mut builder = ui::DrawListBuilder::new();
            Ok(measure_cpu_case(
                spec.id,
                "primitive-lifecycle",
                smoke,
                true,
                0.18,
                loops,
                notes,
                move || {
                    for row in nodes.rows.drain(..) {
                        surface.tree_mut().remove_node(row);
                    }
                    builder.clear();
                    surface.encode(&mut builder);
                    let dl = builder.drawlist();
                    let encoded = (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64;
                    nodes = populate_flat_rect_surface(&mut surface, count, 0);
                    surface.layout(viewport_w, viewport_h.max((count_f / 10.0).ceil() * 28.0));
                    encoded
                },
            ))
        }
        PrimitiveLifecycleOp::Remount => {
            let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(viewport_w));
            let mut nodes = FlatRectSurfaceNodes::default();
            let mut builder = ui::DrawListBuilder::new();
            Ok(measure_cpu_case(
                spec.id,
                "primitive-lifecycle",
                smoke,
                true,
                0.18,
                loops,
                notes,
                move || {
                    let replenished = populate_flat_rect_surface(&mut surface, count, 0);
                    nodes.rows = replenished.rows;
                    nodes.cells = replenished.cells;
                    surface.layout(viewport_w, viewport_h.max((count_f / 10.0).ceil() * 28.0));
                    builder.clear();
                    surface.encode(&mut builder);
                    let dl = builder.drawlist();
                    let encoded = (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64;
                    for row in nodes.rows.drain(..) {
                        surface.tree_mut().remove_node(row);
                    }
                    encoded
                },
            ))
        }
    }
}

fn primitive_labels_case(spec: &PrimitiveLifecycleSpec, smoke: bool) -> Result<PerfCaseResult> {
    let loops = primitive_lifecycle_iterations(spec.kind, spec.count, smoke);
    let rects = primitive_label_rects(spec.count);
    let notes = vec![format!(
        "Multiline retained-label {} case at {} visible nodes.",
        spec.op.label(),
        spec.count
    )];
    match spec.op {
        PrimitiveLifecycleOp::Mount => {
            let mut builder = ui::DrawListBuilder::new();
            let mut txt = perf_text_ctx();
            let mut uploader = CpuUploader::default();
            Ok(measure_cpu_case(
                spec.id,
                "primitive-lifecycle",
                smoke,
                true,
                0.20,
                loops,
                notes,
                move || {
                    builder.clear();
                    let labels = build_lifecycle_labels(spec.count, 0);
                    for (label, rect) in labels.iter().zip(rects.iter().copied()) {
                        label.encode(rect, 2.0, &mut txt, &mut uploader, &mut builder);
                    }
                    let dl = builder.drawlist();
                    (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
                },
            ))
        }
        PrimitiveLifecycleOp::Mutate => {
            let mut builder = ui::DrawListBuilder::new();
            let mut txt = perf_text_ctx();
            let mut uploader = CpuUploader::default();
            let mut labels = build_lifecycle_labels(spec.count, 0);
            let mut phase = 0usize;
            Ok(measure_cpu_case(
                spec.id,
                "primitive-lifecycle",
                smoke,
                true,
                0.20,
                loops,
                notes,
                move || {
                    phase = phase.wrapping_add(1);
                    mutate_lifecycle_labels(&mut labels, phase);
                    builder.clear();
                    for (label, rect) in labels.iter().zip(rects.iter().copied()) {
                        label.encode(rect, 2.0, &mut txt, &mut uploader, &mut builder);
                    }
                    let dl = builder.drawlist();
                    (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
                },
            ))
        }
        PrimitiveLifecycleOp::RemoveAll | PrimitiveLifecycleOp::Remount => {
            bail!("unsupported label primitive lifecycle op `{}`", spec.id)
        }
    }
}

fn primitive_cards_case(spec: &PrimitiveLifecycleSpec, smoke: bool) -> Result<PerfCaseResult> {
    let loops = primitive_lifecycle_iterations(spec.kind, spec.count, smoke);
    let rects = primitive_card_rects(spec.count);
    let notes = vec![format!(
        "Rounded shadow-card {} case at {} visible nodes.",
        spec.op.label(),
        spec.count
    )];
    match spec.op {
        PrimitiveLifecycleOp::Mount => {
            let mut builder = ui::DrawListBuilder::new();
            Ok(measure_cpu_case(
                spec.id,
                "primitive-lifecycle",
                smoke,
                true,
                0.18,
                loops,
                notes,
                move || {
                    builder.clear();
                    encode_lifecycle_cards(&mut builder, &rects, 0);
                    let dl = builder.drawlist();
                    (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
                },
            ))
        }
        PrimitiveLifecycleOp::Mutate => {
            let mut builder = ui::DrawListBuilder::new();
            let mut phase = 0usize;
            Ok(measure_cpu_case(
                spec.id,
                "primitive-lifecycle",
                smoke,
                true,
                0.18,
                loops,
                notes,
                move || {
                    phase = phase.wrapping_add(1);
                    builder.clear();
                    encode_lifecycle_cards(&mut builder, &rects, phase);
                    let dl = builder.drawlist();
                    (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
                },
            ))
        }
        PrimitiveLifecycleOp::RemoveAll | PrimitiveLifecycleOp::Remount => {
            bail!("unsupported card primitive lifecycle op `{}`", spec.id)
        }
    }
}

fn primitive_images_case(spec: &PrimitiveLifecycleSpec, smoke: bool) -> Result<PerfCaseResult> {
    let loops = primitive_lifecycle_iterations(spec.kind, spec.count, smoke);
    let rects = primitive_image_rects(spec.count);
    let notes = vec![format!(
        "Bitmap image-view {} case at {} visible nodes.",
        spec.op.label(),
        spec.count
    )];
    match spec.op {
        PrimitiveLifecycleOp::Mount => {
            let mut builder = ui::DrawListBuilder::new();
            Ok(measure_cpu_case(
                spec.id,
                "primitive-lifecycle",
                smoke,
                true,
                0.18,
                loops,
                notes,
                move || {
                    builder.clear();
                    let images = build_lifecycle_images(spec.count, 0);
                    for (image, rect) in images.iter().zip(rects.iter().copied()) {
                        image.encode(rect, None, &mut builder);
                    }
                    let dl = builder.drawlist();
                    (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
                },
            ))
        }
        PrimitiveLifecycleOp::Mutate => {
            let mut builder = ui::DrawListBuilder::new();
            let mut images = build_lifecycle_images(spec.count, 0);
            let mut phase = 0usize;
            Ok(measure_cpu_case(
                spec.id,
                "primitive-lifecycle",
                smoke,
                true,
                0.18,
                loops,
                notes,
                move || {
                    phase = phase.wrapping_add(1);
                    mutate_lifecycle_images(&mut images, phase);
                    builder.clear();
                    for (image, rect) in images.iter().zip(rects.iter().copied()) {
                        image.encode(rect, None, &mut builder);
                    }
                    let dl = builder.drawlist();
                    (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
                },
            ))
        }
        PrimitiveLifecycleOp::RemoveAll | PrimitiveLifecycleOp::Remount => {
            bail!("unsupported image primitive lifecycle op `{}`", spec.id)
        }
    }
}

fn primitive_control_set_case(
    spec: &PrimitiveLifecycleSpec,
    smoke: bool,
) -> Result<PerfCaseResult> {
    let loops = primitive_lifecycle_iterations(spec.kind, spec.count, smoke);
    let notes = vec![String::from(
        "Representative control-set slice using the shared controls showcase workload.",
    )];
    let viewport = api::RectF::new(0.0, 0.0, 420.0, 240.0);
    match spec.op {
        PrimitiveLifecycleOp::Mount => Ok(measure_cpu_case(
            spec.id,
            "primitive-lifecycle",
            smoke,
            true,
            0.14,
            loops,
            notes,
            move || {
                let mut builder = ui::DrawListBuilder::new();
                let mut txt = perf_text_ctx();
                let mut uploader = CpuUploader::default();
                let mut controls = scenes::Controls::default();
                controls.draw(viewport, 2.0, &mut txt, &mut uploader, &mut builder);
                let dl = builder.drawlist();
                (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
            },
        )),
        PrimitiveLifecycleOp::Mutate => {
            let mut builder = ui::DrawListBuilder::new();
            let mut txt = perf_text_ctx();
            let mut uploader = CpuUploader::default();
            let mut controls = scenes::Controls::default();
            let mut button_pressed = false;
            Ok(measure_cpu_case(
                spec.id,
                "primitive-lifecycle",
                smoke,
                true,
                0.14,
                loops,
                notes,
                move || {
                    controls.update(16);
                    controls.key_arrow_right();
                    button_pressed = !button_pressed;
                    if button_pressed {
                        controls.key_space_down();
                    } else {
                        let _ = controls.key_space_up();
                    }
                    builder.clear();
                    controls.draw(viewport, 2.0, &mut txt, &mut uploader, &mut builder);
                    let dl = builder.drawlist();
                    (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
                },
            ))
        }
        PrimitiveLifecycleOp::RemoveAll | PrimitiveLifecycleOp::Remount => {
            bail!("unsupported control-set primitive lifecycle op `{}`", spec.id)
        }
    }
}

fn flat_rect_surface_root_style(width: f32) -> ui::NodeStyle {
    ui::NodeStyle {
        axis: ui::Axis::Column,
        size: ui::Size2D { w: ui::Dim::Px(width), h: ui::Dim::Auto },
        padding: ui::Edges { left: 8.0, top: 8.0, right: 8.0, bottom: 8.0 },
        gap: 6.0,
        background: api::Color::rgba(0.0, 0.0, 0.0, 0.0),
        clip: true,
        ..ui::NodeStyle::default()
    }
}

fn flat_rect_row_style() -> ui::NodeStyle {
    ui::NodeStyle {
        axis: ui::Axis::Row,
        size: ui::Size2D { w: ui::Dim::Auto, h: ui::Dim::Px(18.0) },
        gap: 6.0,
        background: api::Color::rgba(0.0, 0.0, 0.0, 0.0),
        ..ui::NodeStyle::default()
    }
}

fn flat_rect_cell_style(index: usize, palette_phase: usize) -> ui::NodeStyle {
    ui::NodeStyle {
        size: ui::Size2D { w: ui::Dim::Px(28.0), h: ui::Dim::Px(18.0) },
        background: flat_rect_fill_color(index, palette_phase),
        corner_radii: [4.0, 4.0, 4.0, 4.0],
        opacity: 0.90,
        ..ui::NodeStyle::default()
    }
}

#[derive(Default)]
struct FlatRectSurfaceNodes {
    rows: Vec<ui::NodeId>,
    cells: Vec<ui::NodeId>,
}

fn populate_flat_rect_surface(
    surface: &mut ui::UiSurface,
    count: usize,
    palette_phase: usize,
) -> FlatRectSurfaceNodes {
    let root = surface.root();
    let cols = if count <= 10 { 5 } else { 10 };
    let rows = count.div_ceil(cols);
    let mut nodes =
        FlatRectSurfaceNodes { rows: Vec::with_capacity(rows), cells: Vec::with_capacity(count) };
    for row in 0..rows {
        let row_node = surface.tree_mut().add_node(root, flat_rect_row_style());
        nodes.rows.push(row_node);
        let start = row * cols;
        let end = (start + cols).min(count);
        for index in start..end {
            let node =
                surface.tree_mut().add_node(row_node, flat_rect_cell_style(index, palette_phase));
            nodes.cells.push(node);
        }
    }
    nodes
}

fn flat_rect_fill_color(index: usize, palette_phase: usize) -> api::Color {
    let slot = (index + palette_phase) % 6;
    match slot {
        0 => api::Color::rgba(0.18, 0.48, 0.96, 1.0),
        1 => api::Color::rgba(0.96, 0.38, 0.24, 1.0),
        2 => api::Color::rgba(0.22, 0.72, 0.42, 1.0),
        3 => api::Color::rgba(0.96, 0.74, 0.18, 1.0),
        4 => api::Color::rgba(0.58, 0.38, 0.96, 1.0),
        _ => api::Color::rgba(0.16, 0.68, 0.86, 1.0),
    }
}

fn primitive_grid_rects(
    count: usize,
    columns: usize,
    cell_w: f32,
    cell_h: f32,
    spacing: f32,
) -> Vec<api::RectF> {
    let cols = columns.max(1);
    let mut rects = Vec::with_capacity(count);
    for index in 0..count {
        let row = index / cols;
        let col = index % cols;
        rects.push(api::RectF::new(
            col as f32 * (cell_w + spacing),
            row as f32 * (cell_h + spacing),
            cell_w,
            cell_h,
        ));
    }
    rects
}

fn primitive_label_rects(count: usize) -> Vec<api::RectF> {
    let columns = if count <= 10 {
        2
    } else if count <= 100 {
        4
    } else {
        5
    };
    primitive_grid_rects(count, columns, 92.0, 34.0, 8.0)
}

fn build_lifecycle_labels(count: usize, phase: usize) -> Vec<ui::elements::Label> {
    let mut labels = Vec::with_capacity(count);
    for index in 0..count {
        labels.push(ui::elements::Label {
            text: lifecycle_label_text(index, phase),
            color: lifecycle_label_color(index, phase),
            align: ui::elements::Align::Left,
            wrap: true,
            font_id: 0,
            font_px: 13.0,
        });
    }
    labels
}

fn mutate_lifecycle_labels(labels: &mut [ui::elements::Label], phase: usize) {
    for (index, label) in labels.iter_mut().enumerate() {
        label.text = lifecycle_label_text(index, phase);
        label.color = lifecycle_label_color(index, phase);
    }
}

fn lifecycle_label_text(index: usize, phase: usize) -> String {
    if (index + phase) % 3 == 0 {
        format!("Oxide {} status {}", index % 97, phase % 11)
    } else {
        format!("Pilot {} ready", (index + phase) % 257)
    }
}

fn lifecycle_label_color(index: usize, phase: usize) -> api::Color {
    match (index + phase) % 4 {
        0 => api::Color::rgba(0.10, 0.12, 0.18, 1.0),
        1 => api::Color::rgba(0.18, 0.30, 0.58, 1.0),
        2 => api::Color::rgba(0.62, 0.22, 0.20, 1.0),
        _ => api::Color::rgba(0.14, 0.44, 0.32, 1.0),
    }
}

fn primitive_card_rects(count: usize) -> Vec<api::RectF> {
    let columns = if count <= 10 { 2 } else { 5 };
    primitive_grid_rects(count, columns, 76.0, 52.0, 12.0)
}

fn encode_lifecycle_cards(builder: &mut ui::DrawListBuilder, rects: &[api::RectF], phase: usize) {
    for (index, rect) in rects.iter().copied().enumerate() {
        let shadow_rect = api::RectF::new(rect.x, rect.y + 4.0, rect.w, rect.h);
        builder.rrect(
            shadow_rect,
            [12.0, 12.0, 12.0, 12.0],
            api::Color::rgba(0.0, 0.0, 0.0, 0.10 + ((index + phase) % 3) as f32 * 0.02),
        );
        builder.rrect(rect, [12.0, 12.0, 12.0, 12.0], lifecycle_card_border(index, phase));
        builder.rrect(
            api::RectF::new(rect.x + 1.5, rect.y + 1.5, rect.w - 3.0, rect.h - 3.0),
            [10.5, 10.5, 10.5, 10.5],
            lifecycle_card_fill(index, phase),
        );
    }
}

fn lifecycle_card_border(index: usize, phase: usize) -> api::Color {
    match (index + phase) % 4 {
        0 => api::Color::rgba(0.90, 0.92, 0.96, 1.0),
        1 => api::Color::rgba(0.78, 0.84, 0.94, 1.0),
        2 => api::Color::rgba(0.90, 0.82, 0.78, 1.0),
        _ => api::Color::rgba(0.82, 0.90, 0.86, 1.0),
    }
}

fn lifecycle_card_fill(index: usize, phase: usize) -> api::Color {
    match (index + phase) % 5 {
        0 => api::Color::rgba(0.96, 0.97, 1.0, 1.0),
        1 => api::Color::rgba(0.92, 0.96, 1.0, 1.0),
        2 => api::Color::rgba(1.0, 0.95, 0.92, 1.0),
        3 => api::Color::rgba(0.94, 1.0, 0.95, 1.0),
        _ => api::Color::rgba(0.97, 0.94, 1.0, 1.0),
    }
}

fn primitive_image_rects(count: usize) -> Vec<api::RectF> {
    let columns = if count <= 10 { 2 } else { 5 };
    primitive_grid_rects(count, columns, 84.0, 64.0, 10.0)
}

fn build_lifecycle_images(count: usize, phase: usize) -> Vec<ui::elements::ImageView> {
    let mut images = Vec::with_capacity(count);
    for index in 0..count {
        images.push(lifecycle_image_view(index, phase));
    }
    images
}

fn mutate_lifecycle_images(images: &mut [ui::elements::ImageView], phase: usize) {
    for (index, image) in images.iter_mut().enumerate() {
        *image = lifecycle_image_view(index, phase);
    }
}

fn lifecycle_image_view(index: usize, phase: usize) -> ui::elements::ImageView {
    let even = (index + phase) % 2 == 0;
    ui::elements::ImageView {
        image: api::ImageHandle(200 + (index % 7) as u32),
        natural_w: 256,
        natural_h: 256,
        fit: if even { ui::elements::ImageFit::Contain } else { ui::elements::ImageFit::Cover },
        alpha: if even { 1.0 } else { 0.62 },
    }
}

fn component_label_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 12 } else { 32 };
    let mut builder = ui::DrawListBuilder::new();
    let mut txt = perf_text_ctx();
    let mut uploader = CpuUploader::default();
    let label = ui::elements::Label {
        text: String::from("Oxide perf audit label wrapping path for hot layout measurement."),
        color: api::Color::rgba(0.1, 0.1, 0.1, 1.0),
        align: ui::elements::Align::Left,
        wrap: true,
        font_id: 0,
        font_px: 16.0,
    };
    let rect = api::RectF::new(0.0, 0.0, 320.0, 80.0);
    measure_cpu_case(
        "cpu.component.label.encode",
        "component",
        smoke,
        true,
        0.12,
        loops,
        vec![String::from("Representative wrapped label encode.")],
        move || {
            builder.clear();
            label.encode(rect, 2.0, &mut txt, &mut uploader, &mut builder);
            let dl = builder.drawlist();
            (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
        },
    )
}

fn component_progress_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 48 } else { 160 };
    let mut builder = ui::DrawListBuilder::new();
    let progress =
        ui::elements::ProgressBar { value: Some(0.61), ..ui::elements::ProgressBar::default() };
    let rect = api::RectF::new(0.0, 0.0, 260.0, 16.0);
    measure_cpu_case(
        "cpu.component.progress_bar.encode",
        "component",
        smoke,
        true,
        0.10,
        loops,
        vec![String::from("Representative determinate progress encode.")],
        move || {
            builder.clear();
            progress.encode(rect, 0.0, &mut builder);
            builder.drawlist().items.len() as u64
        },
    )
}

fn component_spinner_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 48 } else { 160 };
    let mut builder = ui::DrawListBuilder::new();
    let spinner = ui::elements::Spinner::default();
    let rect = api::RectF::new(0.0, 0.0, 32.0, 32.0);
    measure_cpu_case(
        "cpu.component.spinner.encode",
        "component",
        smoke,
        true,
        0.10,
        loops,
        vec![String::from("Representative spinner encode.")],
        move || {
            builder.clear();
            spinner.encode(rect, &mut builder);
            builder.drawlist().items.len() as u64
        },
    )
}

fn component_button_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 12 } else { 32 };
    let mut builder = ui::DrawListBuilder::new();
    let mut txt = perf_text_ctx();
    let mut uploader = CpuUploader::default();
    let button =
        ui::elements::Button { text: String::from("Measure"), ..ui::elements::Button::default() };
    let state = ui::elements::ButtonState::default();
    let rect = api::RectF::new(0.0, 0.0, 140.0, 40.0);
    measure_cpu_case(
        "cpu.component.button.encode",
        "component",
        smoke,
        true,
        0.12,
        loops,
        vec![String::from("Representative enabled button encode.")],
        move || {
            builder.clear();
            button.encode(rect, 2.0, &mut txt, &mut uploader, &state, &mut builder);
            let dl = builder.drawlist();
            (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
        },
    )
}

fn component_toggle_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 48 } else { 160 };
    let mut builder = ui::DrawListBuilder::new();
    let toggle = ui::elements::Toggle::default();
    let mut state = ui::elements::ToggleState::default();
    state.set_on(true);
    state.step(16);
    let rect = api::RectF::new(0.0, 0.0, 48.0, 24.0);
    measure_cpu_case(
        "cpu.component.toggle.encode",
        "component",
        smoke,
        true,
        0.10,
        loops,
        vec![String::from("Representative toggle encode.")],
        move || {
            builder.clear();
            toggle.encode(rect, &state, &mut builder);
            builder.drawlist().items.len() as u64
        },
    )
}

fn component_slider_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 48 } else { 160 };
    let mut builder = ui::DrawListBuilder::new();
    let slider = ui::elements::Slider::default();
    let mut state = ui::elements::SliderState::default();
    state.value = 0.68;
    let rect = api::RectF::new(0.0, 0.0, 260.0, 16.0);
    measure_cpu_case(
        "cpu.component.slider.encode",
        "component",
        smoke,
        true,
        0.10,
        loops,
        vec![String::from("Representative slider encode.")],
        move || {
            builder.clear();
            slider.encode(rect, &state, &mut builder);
            builder.drawlist().items.len() as u64
        },
    )
}

fn component_image_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 48 } else { 160 };
    let mut builder = ui::DrawListBuilder::new();
    let image = ui::elements::ImageView {
        image: api::ImageHandle(11),
        natural_w: 512,
        natural_h: 512,
        fit: ui::elements::ImageFit::Contain,
        alpha: 1.0,
    };
    let rect = api::RectF::new(0.0, 0.0, 220.0, 180.0);
    measure_cpu_case(
        "cpu.component.image_view.encode",
        "component",
        smoke,
        true,
        0.10,
        loops,
        vec![String::from("Representative contain image encode.")],
        move || {
            builder.clear();
            image.encode(rect, None, &mut builder);
            builder.drawlist().items.len() as u64
        },
    )
}

fn component_nine_slice_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 48 } else { 160 };
    let mut builder = ui::DrawListBuilder::new();
    let image = ui::elements::NineSliceImage {
        tex: api::ImageHandle(12),
        slice: api::Insets::new(12.0, 12.0, 12.0, 12.0),
        alpha: 1.0,
    };
    let rect = api::RectF::new(0.0, 0.0, 240.0, 120.0);
    measure_cpu_case(
        "cpu.component.nine_slice_image.encode",
        "component",
        smoke,
        true,
        0.10,
        loops,
        vec![String::from("Representative nine-slice encode.")],
        move || {
            builder.clear();
            image.encode(rect, &mut builder);
            builder.drawlist().items.len() as u64
        },
    )
}

fn component_collection_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 10 } else { 24 };
    let mut builder = ui::DrawListBuilder::new();
    let mut view =
        ui::collection::CollectionView::new(ui::collection::CollectionMode::VerticalGrid {
            col_width: 96.0,
            spacing: 8.0,
        });
    view.set_count(240);
    let mut measure = GridMeasure;
    let mut renderer = GridRender;
    let viewport = api::RectF::new(0.0, 0.0, 360.0, 240.0);
    let mut scroll = 0.0f32;
    measure_cpu_case(
        "cpu.component.collection_view.encode",
        "component",
        smoke,
        true,
        0.12,
        loops,
        vec![String::from("Representative virtualized collection layout+render.")],
        move || {
            builder.clear();
            scroll = (scroll + 31.0).min(1_200.0);
            view.set_scroll(scroll);
            let metrics =
                view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
            let draws = builder.drawlist().items.len() as u64;
            draws + metrics.content_h.round() as u64
        },
    )
}

fn animation_spinner_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 48 } else { 160 };
    let mut builder = ui::DrawListBuilder::new();
    let spinner = ui::elements::Spinner::default();
    let rect = api::RectF::new(0.0, 0.0, 32.0, 32.0);
    measure_cpu_case(
        "cpu.animation.spinner_spin",
        "animation",
        smoke,
        true,
        0.10,
        loops,
        vec![String::from("Registry animation sample for SpinnerSpin.")],
        move || {
            builder.clear();
            spinner.encode(rect, &mut builder);
            builder.drawlist().items.len() as u64
        },
    )
}

fn animation_progress_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 48 } else { 160 };
    let mut builder = ui::DrawListBuilder::new();
    let progress =
        ui::elements::ProgressBar { value: None, ..ui::elements::ProgressBar::default() };
    let rect = api::RectF::new(0.0, 0.0, 280.0, 16.0);
    let mut phase = 0.0f32;
    measure_cpu_case(
        "cpu.animation.progress_indeterminate",
        "animation",
        smoke,
        true,
        0.10,
        loops,
        vec![String::from("Registry animation sample for ProgressIndeterminate.")],
        move || {
            builder.clear();
            phase = (phase + 0.0275).fract();
            progress.encode(rect, phase, &mut builder);
            builder.drawlist().items.len() as u64
        },
    )
}

fn animation_button_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 12 } else { 32 };
    let mut builder = ui::DrawListBuilder::new();
    let mut txt = perf_text_ctx();
    let mut uploader = CpuUploader::default();
    let button =
        ui::elements::Button { text: String::from("Tap"), ..ui::elements::Button::default() };
    let mut state = ui::elements::ButtonState::default();
    state.on_pointer_down();
    let rect = api::RectF::new(0.0, 0.0, 120.0, 40.0);
    measure_cpu_case(
        "cpu.animation.button_press_scale",
        "animation",
        smoke,
        true,
        0.12,
        loops,
        vec![String::from("Registry animation sample for ButtonPressScale.")],
        move || {
            builder.clear();
            button.encode(rect, 2.0, &mut txt, &mut uploader, &state, &mut builder);
            let dl = builder.drawlist();
            (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
        },
    )
}

fn animation_toggle_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 48 } else { 160 };
    let mut builder = ui::DrawListBuilder::new();
    let toggle = ui::elements::Toggle::default();
    let mut state = ui::elements::ToggleState::default();
    state.set_on(true);
    let rect = api::RectF::new(0.0, 0.0, 48.0, 24.0);
    measure_cpu_case(
        "cpu.animation.toggle_thumb_spring",
        "animation",
        smoke,
        true,
        0.10,
        loops,
        vec![String::from("Registry animation sample for ToggleThumbSpring.")],
        move || {
            builder.clear();
            state.step(16);
            toggle.encode(rect, &state, &mut builder);
            builder.drawlist().items.len() as u64
        },
    )
}

fn animation_slider_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 48 } else { 160 };
    let mut builder = ui::DrawListBuilder::new();
    let slider = ui::elements::Slider::default();
    let mut state = ui::elements::SliderState::default();
    let rect = api::RectF::new(0.0, 0.0, 240.0, 16.0);
    let mut step = 0u32;
    measure_cpu_case(
        "cpu.animation.slider_thumb_move",
        "animation",
        smoke,
        true,
        0.10,
        loops,
        vec![String::from("Registry animation sample for SliderThumbMove.")],
        move || {
            builder.clear();
            step = step.wrapping_add(1);
            state.value = ((step % 100) as f32) / 100.0;
            slider.encode(rect, &state, &mut builder);
            builder.drawlist().items.len() as u64
        },
    )
}

fn animation_image_zoom_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 48 } else { 160 };
    let mut builder = ui::DrawListBuilder::new();
    let image = ui::elements::ImageView {
        image: api::ImageHandle(21),
        natural_w: 512,
        natural_h: 512,
        fit: ui::elements::ImageFit::Contain,
        alpha: 1.0,
    };
    let rect = api::RectF::new(0.0, 0.0, 260.0, 200.0);
    let mut zoom = ui::elements::ImageZoomState::default();
    measure_cpu_case(
        "cpu.animation.image_zoom_pan",
        "animation",
        smoke,
        true,
        0.10,
        loops,
        vec![String::from("Registry animation sample for ImageZoomPan.")],
        move || {
            builder.clear();
            zoom.pinch(0.01, [rect.w * 0.5, rect.h * 0.5]);
            zoom.pan(1.5, -0.75);
            image.encode(rect, Some(&zoom), &mut builder);
            builder.drawlist().items.len() as u64
        },
    )
}

fn animation_timeline_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 8 } else { 20 };
    let mut builder = ui::DrawListBuilder::new();
    let mut txt = perf_text_ctx();
    let mut uploader = CpuUploader::default();
    let mut timeline = scenes::AnimTimeline::default();
    let rect = api::RectF::new(0.0, 0.0, 420.0, 220.0);
    let mut time_ms = 0u32;
    measure_cpu_case(
        "cpu.animation.anim_timeline_bars",
        "animation",
        smoke,
        true,
        0.12,
        loops,
        vec![String::from("Registry animation sample for AnimTimelineBars.")],
        move || {
            builder.clear();
            time_ms = time_ms.wrapping_add(16);
            timeline.update(time_ms);
            timeline.draw(rect, 2.0, &mut txt, &mut uploader, &mut builder);
            let dl = builder.drawlist();
            (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
        },
    )
}

fn cpu_scene_case(spec: &ScenePerfSpec, smoke: bool) -> Result<PerfCaseResult> {
    let case_id = format!("cpu.scene.{}.frame", spec.slug);
    let loops = if smoke { 4 } else { 8 };
    let mut router = prepare_cpu_router();
    router.set_scene(spec.index);
    let mut builder = ui::DrawListBuilder::new();
    let vp = perf_viewport();
    let mut now = timing::now_ms();
    Ok(measure_cpu_case(
        &case_id,
        "scene-cpu",
        smoke,
        true,
        0.15,
        loops,
        vec![format!("Router update + draw + coalesce for {}", spec.name)],
        move || advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now),
    ))
}

fn gpu_scene_case(spec: &ScenePerfSpec, smoke: bool) -> Result<PerfCaseResult> {
    set_env_if_unset("OXIDE_ENABLE_DAMAGE", "1");
    let damage_enabled = env_bool("OXIDE_PERF_DAMAGE_ENABLED", true);
    let damage_use_thresh = env_f32("OXIDE_PERF_DAMAGE_USE_THRESH", DAMAGE_USE_THRESH);
    let damage_prefilter_thresh =
        env_f32("OXIDE_PERF_DAMAGE_PREFILTER_THRESH", DAMAGE_PREFILTER_THRESH);

    let config = metal::MetalRendererConfig::visible_host();
    let frame_resource_depth = config.frame_resource_depth as u32;
    let mut renderer = Box::new(
        metal::MetalRenderer::new_with_config(config)
            .context("creating visible-host Metal renderer")?,
    );
    let w = PERF_SCENE_W;
    let h = PERF_SCENE_H;
    let scale = PERF_DEVICE_SCALE;
    renderer.resize(w, h, scale).context("resizing Metal renderer")?;
    renderer.set_damage_options(damage_enabled, damage_use_thresh, damage_prefilter_thresh);

    let ptr: *mut metal::MetalRenderer = &mut *renderer;
    let checker = gen_checker_rgba(512, 512);
    let tex = unsafe { (*ptr).image_create_rgba8(512, 512, &checker, 512 * 4) };
    let mut router = prepare_gpu_router(ptr, tex);
    router.set_scene(spec.index);

    let mut builder = ui::DrawListBuilder::new();
    let vp = api::RectF::new(0.0, 0.0, (w as f32) / scale, (h as f32) / scale);
    let warmups = if smoke { 4 } else { 8 };
    let frames = if smoke { 12 } else { 48 };
    let mut frame_samples = Vec::with_capacity(frames);
    let mut draw_samples = Vec::with_capacity(frames);
    let mut encode_samples = Vec::with_capacity(frames);
    let mut gpu_samples = Vec::with_capacity(frames);
    let mut draws_sum = 0.0f64;
    let mut instanced_sum = 0.0f64;
    let mut culled_sum = 0.0f64;
    let mut damage_sum = 0.0f64;
    let mut frame_ring_bytes_peak = 0_u64;
    let mut resource_grows_sum = 0_u64;
    let mut skipped_sum = 0_u64;

    for index in 0..(warmups + frames) {
        builder.clear();
        let frame_t0 = Instant::now();
        let now = timing::now_ms();
        router.update(now, 16);
        let draw_t0 = Instant::now();
        router.draw(vp, scale, &mut builder);
        ui::coalesce_adjacent_draws(builder.drawlist_mut());
        let draw_ms = draw_t0.elapsed().as_secs_f64() * 1000.0;
        let damage = api::Damage { rects: router.take_damage() };
        let token = if damage_enabled {
            renderer.begin_frame(&api::FrameTarget, Some(&damage))
        } else {
            renderer.begin_frame(&api::FrameTarget, None)
        };
        let frame_id = token.0;
        renderer.encode_pass(builder.drawlist());
        renderer
            .submit(token)
            .with_context(|| format!("submitting Metal frame for gpu.scene.{}.frame", spec.slug))?;
        let frame_ms = frame_t0.elapsed().as_secs_f64() * 1000.0;
        let stats = last_metal_stats_after_submit(&renderer, frame_id);
        frame_ring_bytes_peak = frame_ring_bytes_peak.max(stats.memory.frame_ring_buffer_bytes);
        if index >= warmups {
            frame_samples.push(frame_ms);
            draw_samples.push(draw_ms);
            encode_samples.push(stats.encode_ms);
            gpu_samples.push(stats.gpu_ms);
            draws_sum += stats.draws as f64;
            instanced_sum += stats.instanced as f64;
            culled_sum += stats.culled as f64;
            damage_sum += stats.damage_pct as f64 * 100.0;
            resource_grows_sum = resource_grows_sum.saturating_add(stats.resource_grows as u64);
            skipped_sum = skipped_sum.saturating_add(stats.frame_backpressure_skipped as u64);
        }
    }

    let summary = summarize(&frame_samples);
    let mut metrics = BTreeMap::new();
    metrics.insert(String::from("draw_ms_median"), summarize(&draw_samples).median);
    metrics.insert(String::from("encode_ms_median"), summarize(&encode_samples).median);
    insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
    insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
    insert_frame_pacing_metrics(&mut metrics, &frame_samples);
    metrics.insert(String::from("draws_avg"), draws_sum / frames as f64);
    metrics.insert(String::from("instanced_avg"), instanced_sum / frames as f64);
    metrics.insert(String::from("culled_avg"), culled_sum / frames as f64);
    metrics.insert(String::from("damage_pct_avg"), damage_sum / frames as f64);
    metrics.insert(String::from("frame_resource_depth"), frame_resource_depth as f64);
    metrics.insert(String::from("frame_ring_buffer_bytes_peak"), frame_ring_bytes_peak as f64);
    metrics.insert(String::from("resource_grows_total"), resource_grows_sum as f64);
    metrics.insert(String::from("frame_backpressure_skips"), skipped_sum as f64);
    metrics.insert(String::from("damage_enabled"), if damage_enabled { 1.0 } else { 0.0 });
    metrics.insert(String::from("damage_use_thresh"), damage_use_thresh as f64);
    metrics.insert(String::from("damage_prefilter_thresh"), damage_prefilter_thresh as f64);

    Ok(PerfCaseResult {
        id: format!("gpu.scene.{}.frame", spec.slug),
        family: String::from("scene-gpu"),
        layer: String::from("flow"),
        scenario: String::from("screen-flow"),
        variant: String::from("oxide"),
        cache_state: String::from("warm"),
        refresh_mode: String::from("offscreen"),
        unit: String::from("ms/frame"),
        gated: true,
        threshold_pct: gpu_scene_threshold(spec),
        median: summary.median,
        p95: summary.p95,
        p99: summary.p99,
        min: summary.min,
        max: summary.max,
        mean: summary.mean,
        samples: frame_samples.len(),
        ops_per_sample: 1,
        notes: vec![format!("Metal scene frame for {}", spec.name)],
        metrics,
    })
}

fn gpu_animation_effects_refresh_matrix_case(smoke: bool) -> Result<PerfCaseResult> {
    let spec = ScenePerfSpec { slug: "anim_timeline", name: "Animation Effects", index: 3 };
    let mut case = gpu_scene_case(&spec, smoke)?;
    case.id = String::from("gpu.animation.effects.refresh_matrix");
    case.family = String::from("animation-effects");
    case.layer = String::from("flow");
    case.scenario = String::from("animation-effects");
    case.variant = String::from("oxide-metal");
    case.refresh_mode = String::from("60hz-and-120hz-budget");
    case.threshold_pct = 0.35;
    case.notes = vec![String::from(
        "Dedicated Metal animation/effects frame-pacing row with direct GPU distribution plus 60 Hz and 120 Hz missed-frame and hitch metrics.",
    )];
    case.metrics.insert(String::from("refresh_matrix_rows"), 2.0);
    Ok(case)
}

fn gpu_journey_collection_navigation_frame_pacing_case(smoke: bool) -> Result<PerfCaseResult> {
    set_env_if_unset("OXIDE_ENABLE_DAMAGE", "1");
    let damage_enabled = env_bool("OXIDE_PERF_DAMAGE_ENABLED", true);
    let damage_use_thresh = env_f32("OXIDE_PERF_DAMAGE_USE_THRESH", DAMAGE_USE_THRESH);
    let damage_prefilter_thresh =
        env_f32("OXIDE_PERF_DAMAGE_PREFILTER_THRESH", DAMAGE_PREFILTER_THRESH);

    let config = metal::MetalRendererConfig::visible_host();
    let frame_resource_depth = config.frame_resource_depth as u32;
    let mut renderer = Box::new(
        metal::MetalRenderer::new_with_config(config)
            .context("creating visible-host Metal renderer")?,
    );
    let w = PERF_SCENE_W;
    let h = PERF_SCENE_H;
    let scale = PERF_DEVICE_SCALE;
    renderer.resize(w, h, scale).context("resizing Metal renderer")?;
    renderer.set_damage_options(damage_enabled, damage_use_thresh, damage_prefilter_thresh);

    let ptr: *mut metal::MetalRenderer = &mut *renderer;
    let checker = gen_checker_rgba(512, 512);
    let tex = unsafe { (*ptr).image_create_rgba8(512, 512, &checker, 512 * 4) };
    let mut router = prepare_gpu_router(ptr, tex);
    router.set_scene(4);

    let mut builder = ui::DrawListBuilder::new();
    let vp = api::RectF::new(0.0, 0.0, (w as f32) / scale, (h as f32) / scale);
    let warmups = if smoke { 4 } else { 8 };
    let frames = if smoke { 12 } else { 48 };
    let mut now = timing::now_ms();
    let mut frame_samples = Vec::with_capacity(frames);
    let mut event_samples = Vec::with_capacity(frames);
    let mut draw_samples = Vec::with_capacity(frames);
    let mut encode_samples = Vec::with_capacity(frames);
    let mut gpu_samples = Vec::with_capacity(frames);
    let mut draws_sum = 0.0f64;
    let mut damage_rects_sum = 0.0f64;
    let mut skipped_sum = 0.0f64;
    let mut navigation_events = 0.0f64;
    let mut frame_ring_bytes_peak = 0_u64;
    let mut resource_grows_sum = 0_u64;

    for index in 0..(warmups + frames) {
        let frame_t0 = Instant::now();
        match index % 4 {
            0 => router.key_arrow_right(),
            1 => router.key_arrow_down(),
            2 => router.key_arrow_down(),
            _ => router.key_arrow_left(),
        }
        builder.clear();
        now = now.saturating_add(16);
        router.update(now, 16);
        let draw_t0 = Instant::now();
        router.draw(vp, scale, &mut builder);
        ui::coalesce_adjacent_draws(builder.drawlist_mut());
        let draw_ms = draw_t0.elapsed().as_secs_f64() * 1000.0;
        let damage_rects = router.take_damage();
        let damage_rect_count = damage_rects.len();
        let damage = api::Damage { rects: damage_rects };
        let token = if damage_enabled {
            renderer.begin_frame(&api::FrameTarget, Some(&damage))
        } else {
            renderer.begin_frame(&api::FrameTarget, None)
        };
        let frame_id = token.0;
        renderer.encode_pass(builder.drawlist());
        renderer
            .submit(token)
            .context("submitting Metal frame for gpu.journey.collection_navigation.frame_pacing")?;
        let event_ms = frame_t0.elapsed().as_secs_f64() * 1000.0;
        let stats = last_metal_stats_after_submit(&renderer, frame_id);
        frame_ring_bytes_peak = frame_ring_bytes_peak.max(stats.memory.frame_ring_buffer_bytes);
        if index >= warmups {
            frame_samples.push(event_ms);
            event_samples.push(event_ms);
            draw_samples.push(draw_ms);
            encode_samples.push(stats.encode_ms);
            gpu_samples.push(stats.gpu_ms);
            draws_sum += stats.draws as f64;
            damage_rects_sum += damage_rect_count as f64;
            skipped_sum += stats.frame_backpressure_skipped as f64;
            resource_grows_sum = resource_grows_sum.saturating_add(stats.resource_grows as u64);
            navigation_events += 1.0;
        }
    }

    let summary = summarize(&frame_samples);
    let (layer, scenario, _, cache_state, _) =
        perf_case_contract_metadata("gpu.journey.collection_navigation.frame_pacing", "journey");
    let mut metrics = BTreeMap::new();
    metrics.insert(String::from("draw_ms_median"), summarize(&draw_samples).median);
    metrics.insert(String::from("encode_ms_median"), summarize(&encode_samples).median);
    insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
    insert_distribution_metrics(&mut metrics, "event_to_visible_ms", &event_samples);
    insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
    insert_frame_pacing_metrics(&mut metrics, &frame_samples);
    metrics.insert(String::from("draws_avg"), draws_sum / frames as f64);
    metrics.insert(String::from("damage_rects_avg"), damage_rects_sum / frames as f64);
    metrics.insert(String::from("frame_backpressure_skips"), skipped_sum);
    metrics.insert(String::from("frame_resource_depth"), frame_resource_depth as f64);
    metrics.insert(String::from("frame_ring_buffer_bytes_peak"), frame_ring_bytes_peak as f64);
    metrics.insert(String::from("resource_grows_total"), resource_grows_sum as f64);
    metrics.insert(String::from("navigation_events"), navigation_events);
    metrics.insert(String::from("damage_enabled"), if damage_enabled { 1.0 } else { 0.0 });
    metrics.insert(String::from("damage_use_thresh"), damage_use_thresh as f64);
    metrics.insert(String::from("damage_prefilter_thresh"), damage_prefilter_thresh as f64);

    Ok(PerfCaseResult {
        id: String::from("gpu.journey.collection_navigation.frame_pacing"),
        family: String::from("journey-gpu"),
        layer: String::from(layer),
        scenario: String::from(scenario),
        variant: String::from("oxide-metal"),
        cache_state: String::from(cache_state),
        refresh_mode: String::from("60hz-and-120hz-budget"),
        unit: String::from("ms/frame"),
        gated: true,
        threshold_pct: 0.45,
        median: summary.median,
        p95: summary.p95,
        p99: summary.p99,
        min: summary.min,
        max: summary.max,
        mean: summary.mean,
        samples: frame_samples.len(),
        ops_per_sample: 1,
        notes: vec![String::from(
            "Metal collection-navigation journey row with event-to-visible, direct GPU, missed-frame, and hitch distributions.",
        )],
        metrics,
    })
}

fn journey_input_form_case(smoke: bool) -> Result<PerfCaseResult> {
    measure_journey_case(
        "cpu.journey.input_form_submit",
        smoke,
        0.15,
        vec![String::from(
            "Input & Haptics scene boot, text entry, picker movement, keyboard submit, and composed redraw.",
        )],
        move || {
            let mut router = prepare_cpu_router();
            router.set_scene(6);
            let mut builder = ui::DrawListBuilder::new();
            let vp = perf_viewport();
            let mut now = timing::now_ms();
            let work = advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now);

            router.input_pointer(72.0, 108.0, 0.0, 0.0, 1);
            router.input_pointer(72.0, 108.0, 0.0, 0.0, 0);
            router.input_set_selection(0, 64);
            router.input_set_composition(0, 5, "Pilot");
            router.input_commit("Pilot One");

            router.input_pointer(72.0, 180.0, 0.0, 0.0, 1);
            router.input_pointer(72.0, 180.0, 0.0, 0.0, 0);
            router.input_set_selection(0, 64);
            router.input_commit("Orbit123");

            router.input_pointer(456.0, 120.0, 0.0, 24.0, 1);
            router.input_pointer(456.0, 120.0, 0.0, 0.0, 0);
            router.input_key(&platform::KeyEvent {
                code: platform::KeyCode::Enter,
                chars: None,
                repeat: false,
                modifiers: platform::Modifiers::empty(),
            });
            router.input_hide_ime();

            work + advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now)
        },
    )
}

fn journey_text_ime_composition_case(smoke: bool) -> Result<PerfCaseResult> {
    measure_journey_case(
        "cpu.journey.text_ime_composition_cycle",
        smoke,
        0.15,
        vec![String::from(
            "Input scene IME composition, marked-text update, commit, selection sync, keyboard hide, and composed redraw.",
        )],
        move || {
            let mut router = prepare_cpu_router();
            router.set_scene(6);
            let mut builder = ui::DrawListBuilder::new();
            let vp = perf_viewport();
            let mut now = timing::now_ms();
            let work = advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now);

            router.input_pointer(72.0, 108.0, 0.0, 0.0, 1);
            router.input_pointer(72.0, 108.0, 0.0, 0.0, 0);
            router.input_set_selection(0, 0);
            router.input_set_composition(0, 0, "に");
            router.input_set_composition(0, 0, "日本");
            router.input_commit("日本語");
            router.input_set_selection(0, 3);
            router.input_set_composition(0, 3, "かな");
            router.input_commit("かな入力");
            router.input_hide_ime();

            work + advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now)
        },
    )
}

fn journey_collection_navigation_case(smoke: bool) -> Result<PerfCaseResult> {
    measure_journey_case(
        "cpu.journey.collection_navigation",
        smoke,
        0.15,
        vec![String::from(
            "Collection Stress scene boot plus focus navigation across a virtualized grid.",
        )],
        move || {
            let mut router = prepare_cpu_router();
            router.set_scene(4);
            let mut builder = ui::DrawListBuilder::new();
            let vp = perf_viewport();
            let mut now = timing::now_ms();
            let mut work = advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now);

            router.key_arrow_right();
            work += advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now);
            router.key_arrow_down();
            work += advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now);
            router.key_arrow_down();
            work += advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now);
            router.key_arrow_left();
            work += advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now);

            work
        },
    )
}

fn journey_zoom_image_case(smoke: bool) -> Result<PerfCaseResult> {
    measure_journey_case(
        "cpu.journey.zoom_image_gesture_cycle",
        smoke,
        0.15,
        vec![String::from("Zoom Image scene boot plus pinch, pan, and double-tap reset gestures.")],
        move || {
            let mut router = prepare_cpu_router();
            router.set_scene(2);
            let mut builder = ui::DrawListBuilder::new();
            let vp = perf_viewport();
            let mut now = timing::now_ms();
            let mut work = advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now);

            router.input_pinch(vp.w * 0.5, vp.h * 0.5, 1.12);
            work += advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now);
            router.input_pointer(vp.w * 0.5, vp.h * 0.5, 18.0, -12.0, 1);
            router.input_pointer(vp.w * 0.5 + 18.0, vp.h * 0.5 - 12.0, 0.0, 0.0, 0);
            work += advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now);
            router.input_double_tap();
            work + advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now)
        },
    )
}

fn journey_orchestration_case(smoke: bool) -> Result<PerfCaseResult> {
    measure_journey_case(
        "cpu.journey.orchestration_transition_modal",
        smoke,
        0.15,
        vec![String::from(
            "Orchestration scene boot, transition trigger, animated frames, dismissable modal, and overlay dismissal.",
        )],
        move || {
            let mut router = prepare_cpu_router();
            router.set_scene(13);
            let mut builder = ui::DrawListBuilder::new();
            let vp = perf_viewport();
            let mut now = timing::now_ms();
            let mut work = advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now);

            router.input_pointer(390.0, 170.0, 0.0, 0.0, 1);
            router.input_pointer(390.0, 170.0, 0.0, 0.0, 0);
            for _ in 0..4 {
                work += advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now);
            }

            router.input_pointer(390.0, 220.0, 0.0, 0.0, 1);
            router.input_pointer(390.0, 220.0, 0.0, 0.0, 0);
            work += advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now);
            router.input_pointer(36.0, 36.0, 0.0, 0.0, 1);
            router.input_pointer(36.0, 36.0, 0.0, 0.0, 0);

            work + advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now)
        },
    )
}

fn authoring_text_fields_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 8 } else { 24 };
    let username_policy = authoring_username_policy();
    let bio_policy = authoring_bio_policy();
    let password_policy = authoring_password_policy();
    measure_cpu_case(
        "cpu.authoring.text_fields.edit_cycle",
        "authoring",
        smoke,
        true,
        0.12,
        loops,
        vec![String::from(
            "Public text-field state engines covering policy normalization, caret edits, fail states, and secure remasking.",
        )],
        move || {
            let mut editable = ui::EditableText::new(username_policy.clone());
            editable.set("Pilot");
            editable.append(".$");
            editable.apply_commit("One");
            let _ = editable.pop_last();

            let mut shifting =
                ui::HorizontalShiftingText::new(bio_policy.clone(), 32.0, 1_200)
                    .with_text("Victor");
            shifting.focus();
            shifting.move_caret_left();
            shifting.move_caret_left();
            shifting.apply_commit(" X");
            shifting.fail_with_message(
                "checking",
                ui::FieldFailRestoreMode::RestoreValue,
            );
            shifting.advance(ui::HorizontalShiftingText::fail_duration_ms());
            shifting.blur();

            let mut secure = ui::SecureText::new(ui::EditableText::new(password_policy.clone()));
            secure.focus();
            secure.apply_commit("Secret42");
            secure.advance(1_200);
            secure.fail_with_message("invalid", ui::FieldFailRestoreMode::Clear);
            secure.advance(ui::HorizontalShiftingText::fail_duration_ms());
            secure.blur();

            editable.value().len() as u64
                + shifting.value().len() as u64
                + shifting.caret_index() as u64
                + secure.value().len() as u64
                + secure.display_text().len() as u64
                + secure.caret_index() as u64
        },
    )
}

fn authoring_popup_wheel_picker_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 24 } else { 72 };
    let panel_rect = api::RectF::new(24.0, 36.0, 180.0, 120.0);
    measure_cpu_case(
        POPUP_WHEEL_PICKER_CASE_ID,
        "authoring",
        smoke,
        true,
        0.10,
        loops,
        vec![String::from(
            "Popup panel classification plus legacy picker drag/commit lifecycle through the public picker state API.",
        )],
        move || {
            let mut popup = ui::PanelPopupState::new();
            popup.open();
            let inside = matches!(
                popup.classify_tap(panel_rect, 80.0, 80.0),
                ui::PopupTapRegion::Panel
            ) as u64;
            let outside = matches!(
                popup.classify_tap(panel_rect, 4.0, 4.0),
                ui::PopupTapRegion::Outside
            ) as u64;

            let mut picker = ui::PopupPickerState::new(7, 2);
            picker.open();
            let begun = picker.begin_drag(0, platform::TouchId(11), 100.0) as u64;
            let updated = picker.update_drag(0, platform::TouchId(11), 42.0, 18.0) as u64;
            let drag_commit = picker.finish_drag(0, platform::TouchId(11));
            let dragged_index = drag_commit
                .map(|commit| commit.selected_index() as u64)
                .unwrap_or_default();
            let commit_haptic = drag_commit
                .map(|commit| {
                    matches!(
                        commit.haptic_pattern(),
                        platform::HapticPattern::ImpactMedium
                    ) as u64
                })
                .unwrap_or_default();
            let _ = picker.sync_to_index(0, 3);
            let selected = picker.selected_index(0).unwrap_or_default() as u64;
            picker.close();

            inside + outside + begun + updated + dragged_index + commit_haptic + selected
        },
    )
}

fn authoring_burst_emitter_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 12 } else { 40 };
    let emitter = authoring_burst_emitter();
    measure_cpu_case(
        "cpu.authoring.burst_emitter.sample",
        "authoring",
        smoke,
        true,
        0.10,
        loops,
        vec![String::from(
            "Deterministic CAEmitter-style particle sampling through the public burst-emitter API.",
        )],
        move || {
            let capacity = emitter.emitted_particle_capacity() as u64;
            let spawned = emitter.spawned_particle_count(3_100) as u64;
            let particles = emitter.particles(3_100, [40.0, 60.0], 32.0);
            let first_particle = emitter
                .particle(0, 2_040, [120.0, 90.0], 32.0)
                .map(|particle| particle.rect.w.round().max(0.0) as u64 + particle.index as u64)
                .unwrap_or_default();

            capacity
                + spawned
                + particles.len() as u64
                + first_particle
                + emitter.visible_end_ms().saturating_sub(emitter.started_ms())
        },
    )
}

fn authoring_surface_router_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 8 } else { 32 };
    let viewport = api::RectF::new(0.0, 0.0, 240.0, 240.0);
    let current_reused = Arc::new(AtomicU64::new(0));
    let current_rebuilt = Arc::new(AtomicU64::new(0));
    let overlay_reused = Arc::new(AtomicU64::new(0));
    let overlay_rebuilt = Arc::new(AtomicU64::new(0));
    let popup_reused = Arc::new(AtomicU64::new(0));
    let popup_rebuilt = Arc::new(AtomicU64::new(0));
    let current_reused_for_run = Arc::clone(&current_reused);
    let current_rebuilt_for_run = Arc::clone(&current_rebuilt);
    let overlay_reused_for_run = Arc::clone(&overlay_reused);
    let overlay_rebuilt_for_run = Arc::clone(&overlay_rebuilt);
    let popup_reused_for_run = Arc::clone(&popup_reused);
    let popup_rebuilt_for_run = Arc::clone(&popup_rebuilt);
    let mut case = measure_cpu_case(
        "cpu.authoring.surface_router.compose",
        "authoring",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from(
            "Surface composition, overlay/popup wiring, scatter transition, encode, capture, and pointer routing through the public surface APIs.",
        )],
        move || {
            let (base_surface, outgoing_node) =
                authoring_surface(240.0, api::Color::rgba(0.12, 0.16, 0.24, 1.0));
            let (next_surface, incoming_node) =
                authoring_surface(240.0, api::Color::rgba(0.22, 0.28, 0.40, 1.0));
            let mut router = ui::SurfaceRouter::new(base_surface);
            let next_index = router.push(next_surface);
            router.set_viewport(viewport, 1.0);

            let (overlay_surface, overlay_content) =
                authoring_surface(240.0, api::Color::rgba(0.82, 0.22, 0.28, 0.92));
            router.overlays_mut().push(
                overlay_surface,
                ui::OverlayVisual::default(),
                ui::OverlayBehavior {
                    dismiss_on_background_tap: true,
                    block_underlying_inputs: true,
                    content_root: Some(overlay_content),
                    focus_root: Some(overlay_content),
                },
            );

            let (popup_surface, popup_content) =
                authoring_surface(180.0, api::Color::rgba(0.24, 0.60, 0.92, 0.96));
            let popup_approve_dismissal = Arc::new(AtomicU64::new(0));
            let popup_dismissals = Arc::new(AtomicU64::new(0));
            let popup_approve_touches = Arc::new(AtomicU64::new(0));
            let popup_handle = router.popups_mut().push(
                popup_surface,
                ui::PopupSpec {
                    visual: ui::OverlayVisual { z_index: 8, ..ui::OverlayVisual::default() },
                    behavior: ui::OverlayBehavior {
                        dismiss_on_background_tap: false,
                        block_underlying_inputs: true,
                        content_root: Some(popup_content),
                        focus_root: Some(popup_content),
                    },
                    touch_region: ui::PopupTouchRegion::ContentRoot,
                    callbacks: ui::PopupCallbacks {
                        approve_dismissal: Some(Box::new({
                            let popup_approve_dismissal = Arc::clone(&popup_approve_dismissal);
                            move |_| {
                                popup_approve_dismissal.fetch_add(1, Ordering::Relaxed) > 0
                            }
                        })),
                        dismissal: Some(Box::new({
                            let popup_dismissals = Arc::clone(&popup_dismissals);
                            move |_| {
                                popup_dismissals.fetch_add(1, Ordering::Relaxed);
                            }
                        })),
                        approve_touch: Some(Box::new({
                            let popup_approve_touches = Arc::clone(&popup_approve_touches);
                            move |_, point| {
                                popup_approve_touches.fetch_add(1, Ordering::Relaxed);
                                point[0] < 220.0 && point[1] < 220.0
                            }
                        })),
                    },
                },
            );
            let popup_key_handle = router.popups().key_popup().map(|handle| handle.0).unwrap_or_default();
            let popup_key_window = if router.popups().popup_is_key_window() { 1 } else { 0 };
            {
                let popups = router.popups_mut();
                if let Some(surface) = popups.surface_mut(popup_handle) {
                    if let Some(style) = surface.tree_mut().style_mut(popup_content) {
                        style.size = ui::Size2D { w: ui::Dim::Px(180.0), h: ui::Dim::Px(180.0) };
                    }
                }
                let _ = popups.content_size_changed(popup_handle);
                let _ = popups.set_touch_region(
                    popup_handle,
                    ui::PopupTouchRegion::Rect(api::RectF::new(0.0, 0.0, 180.0, 180.0)),
                );
            }

            router.transition_to(
                next_index,
                &[ui::ScatterSpec::new(outgoing_node, [0.0, -24.0]).duration(90)],
                &[ui::ScatterSpec::new(incoming_node, [0.0, 24.0]).duration(90)],
            );
            router.tick_all_at(320);

            let mut builder = ui::DrawListBuilder::new();
            router.encode_with_overlays(viewport, 1.0, &mut builder);
            builder.clear();
            router.encode_with_overlays(viewport, 1.0, &mut builder);
            let retained = router.retained_composition_stats();
            current_reused_for_run.fetch_add(retained.current_reused as u64, Ordering::Relaxed);
            current_rebuilt_for_run.fetch_add(retained.current_rebuilt as u64, Ordering::Relaxed);
            overlay_reused_for_run.fetch_add(retained.overlay_reused as u64, Ordering::Relaxed);
            overlay_rebuilt_for_run.fetch_add(retained.overlay_rebuilt as u64, Ordering::Relaxed);
            popup_reused_for_run.fetch_add(retained.popup_reused as u64, Ordering::Relaxed);
            popup_rebuilt_for_run.fetch_add(retained.popup_rebuilt as u64, Ordering::Relaxed);
            let capture = router.capture(viewport, 1.0);
            let mut hits = 0u64;
            router.pointer_event(20.0, 20.0, 1, |_, _| hits = hits.saturating_add(1));
            router.pointer_event(220.0, 220.0, 1, |_, _| hits = hits.saturating_add(1));
            router.pointer_event(220.0, 220.0, 0, |_, _| hits = hits.saturating_add(1));
            popup_approve_dismissal.store(1, Ordering::Relaxed);
            router.pointer_event(220.0, 220.0, 1, |_, _| hits = hits.saturating_add(1));

            let (dismiss_popup_surface, dismiss_popup_content) =
                authoring_surface(140.0, api::Color::rgba(0.30, 0.78, 0.44, 0.96));
            router.popups_mut().push(
                dismiss_popup_surface,
                ui::PopupSpec {
                    visual: ui::OverlayVisual { z_index: 9, ..ui::OverlayVisual::default() },
                    behavior: ui::OverlayBehavior {
                        dismiss_on_background_tap: false,
                        block_underlying_inputs: true,
                        content_root: Some(dismiss_popup_content),
                        focus_root: Some(dismiss_popup_content),
                    },
                    callbacks: ui::PopupCallbacks {
                        approve_dismissal: None,
                        dismissal: Some(Box::new({
                            let popup_dismissals = Arc::clone(&popup_dismissals);
                            move |_| {
                                popup_dismissals.fetch_add(1, Ordering::Relaxed);
                            }
                        })),
                        approve_touch: None,
                    },
                    ..ui::PopupSpec::default()
                },
            );
            let manual_dismissed = router.popups_mut().dismiss_key_popup().map(|handle| handle.0).unwrap_or_default();

            builder.drawlist().items.len() as u64
                + capture.draw_list.items.len() as u64
                + router.current_index() as u64
                + router.overlays().top_handle().map(|handle| handle.0).unwrap_or_default()
                + if router.popups().is_empty() { 0 } else { 1 }
                + popup_key_handle
                + popup_key_window
                + popup_dismissals.load(Ordering::Relaxed)
                + popup_approve_touches.load(Ordering::Relaxed)
                + manual_dismissed
                + hits
        },
    );
    case.metrics.insert(
        String::from("router_current_reused_total"),
        current_reused.load(Ordering::Relaxed) as f64,
    );
    case.metrics.insert(
        String::from("router_current_rebuilt_total"),
        current_rebuilt.load(Ordering::Relaxed) as f64,
    );
    case.metrics.insert(
        String::from("router_overlay_reused_total"),
        overlay_reused.load(Ordering::Relaxed) as f64,
    );
    case.metrics.insert(
        String::from("router_overlay_rebuilt_total"),
        overlay_rebuilt.load(Ordering::Relaxed) as f64,
    );
    case.metrics.insert(
        String::from("router_popup_reused_total"),
        popup_reused.load(Ordering::Relaxed) as f64,
    );
    case.metrics.insert(
        String::from("router_popup_rebuilt_total"),
        popup_rebuilt.load(Ordering::Relaxed) as f64,
    );
    case
}

fn authoring_collection_key_reconcile_case(smoke: bool, indexed: bool) -> PerfCaseResult {
    let loops = if smoke { 64 } else { 256 };
    let item_key_queries = Arc::new(AtomicU64::new(0));
    let key_index_queries = Arc::new(AtomicU64::new(0));
    let key_index_hits = Arc::new(AtomicU64::new(0));
    let mut measure = KeyReconcileMeasure::new(
        256,
        indexed,
        Arc::clone(&item_key_queries),
        Arc::clone(&key_index_queries),
        Arc::clone(&key_index_hits),
    );
    let mut collection =
        ui::collection::CollectionView::new(ui::collection::CollectionMode::VerticalGrid {
            col_width: 80.0,
            spacing: 4.0,
        });
    collection.set_count(256);
    let mut render = KeyReconcileRender;
    let mut builder = ui::DrawListBuilder::new();
    let viewport = api::RectF::new(0.0, 0.0, 100.0, 180.0);
    let id = if indexed {
        "cpu.authoring.collection_key_reconcile.indexed"
    } else {
        "cpu.authoring.collection_key_reconcile.scan"
    };
    let note = if indexed {
        "Keyed CollectionView focus reconciliation using Measure::item_index_for_key after a far reorder."
    } else {
        "Keyed CollectionView focus reconciliation using the legacy item_key scan fallback after a far reorder."
    };
    let mut case = measure_cpu_case(
        id,
        "authoring",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from(note)],
        || {
            builder.clear();
            collection.focus_set_key(Some(200), Some(ui::collection::ItemKey(200)));
            collection.layout_and_render(viewport, &mut measure, &mut render, &mut builder);
            collection.focus().unwrap_or_default() as u64
                + builder.drawlist().items.len() as u64
                + builder.drawlist().vertices.len() as u64
        },
    );
    let item_key_total = item_key_queries.load(Ordering::Relaxed);
    let index_query_total = key_index_queries.load(Ordering::Relaxed);
    let index_hit_total = key_index_hits.load(Ordering::Relaxed);
    case.metrics.insert(String::from("collection_item_key_queries_total"), item_key_total as f64);
    case.metrics
        .insert(String::from("collection_key_index_queries_total"), index_query_total as f64);
    case.metrics.insert(String::from("collection_key_index_hits_total"), index_hit_total as f64);
    case.metrics.insert(
        String::from("collection_item_key_queries_per_lookup"),
        item_key_total as f64 / index_query_total.max(1) as f64,
    );
    case.metrics
        .insert(String::from("collection_key_index_enabled"), if indexed { 1.0 } else { 0.0 });
    case.metrics.insert(String::from("collection_count"), 256.0);
    case.metrics.insert(String::from("collection_reconciled_index"), 220.0);
    case
}

fn authoring_collection_measure_cache_bounded_churn_case(smoke: bool) -> PerfCaseResult {
    let loops = 1;
    let count = 20_000usize;
    let mut op_total = 0u64;
    let mut initial_total = 0u64;
    let mut repair_total = 0u64;
    let mut repair_draw_total = 0u64;
    let mut content_h_total = 0u64;
    let mut case = measure_cpu_case(
        "cpu.authoring.collection_measure_cache.bounded_churn",
        "authoring",
        smoke,
        true,
        0.25,
        loops,
        vec![String::from(
            "Large variable CollectionView grid proves bounded measurement-cache repair after cold key churn.",
        )],
        || {
            let stats = run_collection_measure_cache_bounded_churn(count);
            op_total = op_total.saturating_add(1);
            initial_total = initial_total.saturating_add(stats.initial_measure_calls);
            repair_total = repair_total.saturating_add(stats.repair_measure_calls);
            repair_draw_total = repair_draw_total.saturating_add(stats.repair_draw_items);
            content_h_total = content_h_total.saturating_add(stats.content_h);
            stats.checksum
        },
    );
    let measured_ops = op_total.max(1);
    let initial_per_op = initial_total as f64 / measured_ops as f64;
    let repair_per_op = repair_total as f64 / measured_ops as f64;
    case.metrics.insert(String::from("collection_count"), count as f64);
    case.metrics
        .insert(String::from("collection_measure_cache_churn_ops_total"), measured_ops as f64);
    case.metrics.insert(String::from("collection_initial_measure_calls_per_op"), initial_per_op);
    case.metrics.insert(String::from("collection_repair_measure_calls_per_op"), repair_per_op);
    case.metrics.insert(
        String::from("collection_repair_to_initial_measure_ratio"),
        repair_per_op / initial_per_op.max(1.0),
    );
    case.metrics.insert(
        String::from("collection_repair_draw_items_per_op"),
        repair_draw_total as f64 / measured_ops as f64,
    );
    case.metrics.insert(
        String::from("collection_content_h_per_op"),
        content_h_total as f64 / measured_ops as f64,
    );
    case
}

fn run_collection_measure_cache_bounded_churn(count: usize) -> CollectionMeasureCacheChurnStats {
    let mut collection =
        ui::collection::CollectionView::new(ui::collection::CollectionMode::VerticalGrid {
            col_width: 50.0,
            spacing: 4.0,
        });
    collection.set_count(count);
    let mut measure = CollectionMeasureCacheChurnMeasure { calls: 0 };
    let mut render = GridRender;
    let mut builder = ui::DrawListBuilder::new();
    let viewport = api::RectF::new(0.0, 0.0, 120.0, 140.0);
    let initial = collection.layout_and_render(viewport, &mut measure, &mut render, &mut builder);
    let initial_measure_calls = measure.calls;
    measure.calls = 0;
    builder.clear();
    collection.set_scroll(42_000.0);
    let repaired = collection.layout_and_render(viewport, &mut measure, &mut render, &mut builder);
    let repair_measure_calls = measure.calls;
    let repair_draw_items = builder.drawlist().items.len() as u64;
    let content_h = repaired.content_h.max(initial.content_h).round().max(0.0) as u64;
    CollectionMeasureCacheChurnStats {
        checksum: initial_measure_calls
            .wrapping_add(repair_measure_calls)
            .wrapping_add(repair_draw_items)
            .wrapping_add(content_h),
        initial_measure_calls,
        repair_measure_calls,
        repair_draw_items,
        content_h,
    }
}

fn authoring_collection_prefix_update_case(smoke: bool, incremental: bool) -> PerfCaseResult {
    let loops = if smoke { 16 } else { 64 };
    let count = 4_096usize;
    let changed_index = 4_000usize;
    let measure_calls = Arc::new(AtomicU64::new(0));
    let revision_queries = Arc::new(AtomicU64::new(0));
    let ops = Arc::new(AtomicU64::new(0));
    let mut measure = PrefixUpdateMeasure::new(
        count,
        changed_index,
        incremental,
        Arc::clone(&measure_calls),
        Arc::clone(&revision_queries),
    );
    let mut collection =
        ui::collection::CollectionView::new(ui::collection::CollectionMode::VerticalGrid {
            col_width: 104.0,
            spacing: 8.0,
        });
    collection.set_count(count);
    let mut render = KeyReconcileRender;
    let mut builder = ui::DrawListBuilder::new();
    let viewport = api::RectF::new(0.0, 0.0, 360.0, 640.0);
    collection.layout_and_render(viewport, &mut measure, &mut render, &mut builder);
    measure_calls.store(0, Ordering::Relaxed);
    revision_queries.store(0, Ordering::Relaxed);
    let id = if incremental {
        "cpu.authoring.collection_prefix_update.incremental"
    } else {
        "cpu.authoring.collection_prefix_update.full_scan"
    };
    let note = if incremental {
        "Variable CollectionView prefix repair with Measure::changed_item_range after one tail item revision."
    } else {
        "Variable CollectionView prefix repair through the full item-revision scan fallback after one tail item revision."
    };
    let mut case = measure_cpu_case(
        id,
        "authoring",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from(note)],
        || {
            builder.clear();
            measure.bump_revision();
            ops.fetch_add(1, Ordering::Relaxed);
            let metrics =
                collection.layout_and_render(viewport, &mut measure, &mut render, &mut builder);
            builder.drawlist().items.len() as u64
                + builder.drawlist().vertices.len() as u64
                + metrics.content_h.round().max(0.0) as u64
        },
    );
    let op_total = ops.load(Ordering::Relaxed).max(1);
    let revision_query_total = revision_queries.load(Ordering::Relaxed);
    case.metrics.insert(String::from("collection_count"), count as f64);
    case.metrics.insert(String::from("collection_changed_index"), changed_index as f64);
    case.metrics.insert(
        String::from("collection_changed_range_enabled"),
        if incremental { 1.0 } else { 0.0 },
    );
    case.metrics.insert(String::from("collection_prefix_update_ops_total"), op_total as f64);
    case.metrics.insert(
        String::from("collection_measure_calls_total"),
        measure_calls.load(Ordering::Relaxed) as f64,
    );
    case.metrics.insert(
        String::from("collection_item_revision_queries_total"),
        revision_query_total as f64,
    );
    case.metrics.insert(
        String::from("collection_item_revision_queries_per_op"),
        revision_query_total as f64 / op_total as f64,
    );
    case
}

fn authoring_surface_retained_clean_encode_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 16 } else { 64 };
    let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(420.0));
    populate_flat_rect_surface(&mut surface, 1_000, 0);
    surface.layout(420.0, 760.0);
    let warm = surface.render_snapshot_retained(
        api::RenderChunkId(100),
        &[],
        Vec::new(),
        api::Damage { rects: Vec::new() },
    ).unwrap();
    let mut cached_draws = 0_u64;
    let mut cached_vertices = 0_u64;
    let mut cached_indices = 0_u64;
    warm.snapshot.visit_instances(|instance| {
        cached_draws = cached_draws.saturating_add(instance.chunk.draw_list().items.len() as u64);
        cached_vertices = cached_vertices.saturating_add(instance.chunk.draw_list().vertices.len() as u64);
        cached_indices = cached_indices.saturating_add(instance.chunk.draw_list().indices.len() as u64);
    });
    let retained_bytes = warm.stats.retained_bytes.saturating_add(warm.stats.retained_sequence_bytes);
    let mut reused = 0u64;
    let mut rebuilt = 0u64;
    let mut command_bytes_copied = 0_u64;
    let mut vertex_bytes_copied = 0_u64;
    let mut index_bytes_copied = 0_u64;
    let mut case = measure_cpu_case(
        "cpu.authoring.surface_retained.clean_encode",
        "authoring",
        smoke,
        true,
        0.16,
        loops,
        vec![String::from(
            "Clean retained UiSurface snapshot over a 1000-node flat-rect tree; expected path reuses immutable per-node chunks without copying commands or geometry.",
        )],
        || {
            let rendered = surface.render_snapshot_retained(
                api::RenderChunkId(100),
                &[],
                Vec::new(),
                api::Damage { rects: Vec::new() },
            ).unwrap();
            match rendered.stats.status {
                ui::RetainedDrawStatus::Reused => {
                    reused = reused.saturating_add(1);
                }
                ui::RetainedDrawStatus::Rebuilt => {
                    rebuilt = rebuilt.saturating_add(1);
                }
            }
            command_bytes_copied = command_bytes_copied.saturating_add(rendered.stats.command_bytes_copied);
            vertex_bytes_copied = vertex_bytes_copied.saturating_add(rendered.stats.vertex_bytes_copied);
            index_bytes_copied = index_bytes_copied.saturating_add(rendered.stats.index_bytes_copied);
            rendered.snapshot.instance_count()
        },
    );
    let total = reused.saturating_add(rebuilt).max(1);
    case.metrics.insert(String::from("retained_reused_ops"), reused as f64);
    case.metrics.insert(String::from("retained_rebuilt_ops"), rebuilt as f64);
    case.metrics.insert(String::from("retained_reuse_ratio"), reused as f64 / total as f64);
    case.metrics.insert(String::from("draw_items"), cached_draws as f64);
    case.metrics.insert(String::from("vertex_count"), cached_vertices as f64);
    case.metrics.insert(String::from("index_count"), cached_indices as f64);
    case.metrics.insert(String::from("command_bytes_copied"), command_bytes_copied as f64);
    case.metrics.insert(String::from("vertex_bytes_copied"), vertex_bytes_copied as f64);
    case.metrics.insert(String::from("index_bytes_copied"), index_bytes_copied as f64);
    case.metrics.insert(String::from("command_bytes_copied_per_op"), command_bytes_copied as f64 / total as f64);
    case.metrics.insert(String::from("vertex_bytes_copied_per_op"), vertex_bytes_copied as f64 / total as f64);
    case.metrics.insert(String::from("index_bytes_copied_per_op"), index_bytes_copied as f64 / total as f64);
    case.metrics.insert(String::from("retained_chunk_bytes"), retained_bytes as f64);
    case.metrics.insert(String::from("flat_fallback_uses"), 0.0);
    case
}

fn authoring_surface_retained_cache_policy_case(smoke: bool) -> PerfCaseResult
{
   let loops = if smoke { 16 } else { 64 };
   let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(420.0));
   populate_flat_rect_surface(&mut surface, 1_000, 0);
   surface.layout(420.0, 760.0);
   let policy = ui::RetainedCachePolicy {
      cpu_budget_bytes: 1024 * 1024,
      prepared_gpu_budget_bytes: 2 * 1024 * 1024,
      ..ui::RetainedCachePolicy::default()
   };
   surface.set_retained_cache_policy(policy);
   let _ = surface.render_snapshot_retained(
      api::RenderChunkId(109),
      &[],
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).unwrap();
   let warm_stats = surface.retained_node_stats();
   let mut case = measure_cpu_case(
      "cpu.authoring.surface_retained.cache_policy",
      "authoring",
      smoke,
      true,
      0.16,
      loops,
      vec![String::from(
         "Author-configured UiSurface retained CPU/GPU byte budgets on a reusable 1,000-node tree; setting an unchanged policy preserves the hot snapshot path.",
      )],
      || {
         surface.set_retained_cache_policy(policy);
         surface.render_snapshot_retained(
            api::RenderChunkId(109),
            &[],
            Vec::new(),
            api::Damage { rects: Vec::new() },
         ).unwrap().snapshot.instance_count()
      },
   );
   let _ = surface.render_snapshot_retained(
      api::RenderChunkId(109),
      &[],
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).unwrap();
   let probe_stats = surface.retained_node_stats();
   case.metrics.insert(String::from("cpu_budget_bytes"), policy.cpu_budget_bytes as f64);
   case.metrics.insert(String::from("prepared_gpu_budget_bytes"), policy.prepared_gpu_budget_bytes as f64);
   case.metrics.insert(String::from("retained_chunk_bytes"), warm_stats.retained_chunk_bytes as f64);
   case.metrics.insert(String::from("retained_sequence_bytes"), warm_stats.retained_sequence_bytes as f64);
   case.metrics.insert(String::from("cache_hits"), probe_stats.cache_hits as f64);
   case.metrics.insert(String::from("cache_misses"), probe_stats.cache_misses as f64);
   case.metrics.insert(String::from("cache_complete"), f64::from(probe_stats.cache_complete));
   case
}

fn authoring_surface_retained_dirty_leaf_encode_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 8 } else { 32 };
    let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(420.0));
    let nodes = populate_flat_rect_surface(&mut surface, 1_000, 0);
    surface.layout(420.0, 760.0);
    let warm = surface.render_snapshot_retained(
        api::RenderChunkId(101),
        &[],
        Vec::new(),
        api::Damage { rects: Vec::new() },
    ).unwrap();
    let mut cached_draws = 0_u64;
    let mut cached_vertices = 0_u64;
    let mut cached_indices = 0_u64;
    warm.snapshot.visit_instances(|instance| {
        cached_draws = cached_draws.saturating_add(instance.chunk.draw_list().items.len() as u64);
        cached_vertices = cached_vertices.saturating_add(instance.chunk.draw_list().vertices.len() as u64);
        cached_indices = cached_indices.saturating_add(instance.chunk.draw_list().indices.len() as u64);
    });
    let mut step = 0usize;
    let mut ops = 0u64;
    let mut reused_nodes = 0u64;
    let mut rebuilt_nodes = 0u64;
    let mut command_bytes_copied = 0_u64;
    let mut vertex_bytes_copied = 0_u64;
    let mut index_bytes_copied = 0_u64;
    let mut retained_bytes = 0_u64;
    let mut case = measure_cpu_case(
        "cpu.authoring.surface_retained.dirty_leaf_encode",
        "authoring",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Dirty-leaf retained UiSurface snapshot over a 1000-node flat-rect tree; expected path replaces one node chunk without duplicating geometry through ancestors.",
        )],
        || {
            let target = nodes.cells[step % nodes.cells.len()];
            step = step.wrapping_add(1);
            surface.edit_style(target, |style| {
                let phase = (step % 31) as f32 / 31.0;
                style.background = api::Color::rgba(0.92, 0.18 + phase * 0.42, 0.22, 1.0);
            });
            surface.layout(420.0, 760.0);
            let rendered = surface.render_snapshot_retained(
                api::RenderChunkId(101),
                &[],
                Vec::new(),
                api::Damage { rects: Vec::new() },
            ).unwrap();
            reused_nodes = reused_nodes.saturating_add(rendered.stats.chunks_reused);
            rebuilt_nodes = rebuilt_nodes.saturating_add(rendered.stats.chunks_rebuilt);
            command_bytes_copied = command_bytes_copied.saturating_add(rendered.stats.command_bytes_copied);
            vertex_bytes_copied = vertex_bytes_copied.saturating_add(rendered.stats.vertex_bytes_copied);
            index_bytes_copied = index_bytes_copied.saturating_add(rendered.stats.index_bytes_copied);
            retained_bytes = retained_bytes.max(rendered.stats.retained_bytes);
            ops = ops.saturating_add(1);
            rendered.snapshot.instance_count()
        },
    );
    let total_nodes = reused_nodes.saturating_add(rebuilt_nodes).max(1);
    let total_ops = ops.max(1);
    case.metrics.insert(
        String::from("retained_reused_nodes_per_op"),
        reused_nodes as f64 / total_ops as f64,
    );
    case.metrics.insert(
        String::from("retained_rebuilt_nodes_per_op"),
        rebuilt_nodes as f64 / total_ops as f64,
    );
    case.metrics.insert(
        String::from("retained_node_reuse_ratio"),
        reused_nodes as f64 / total_nodes as f64,
    );
    case.metrics.insert(String::from("dirty_nodes"), 1.0);
    case.metrics.insert(String::from("tracked_nodes"), nodes.cells.len() as f64);
    case.metrics.insert(String::from("draw_items"), cached_draws as f64);
    case.metrics.insert(String::from("vertex_count"), cached_vertices as f64);
    case.metrics.insert(String::from("index_count"), cached_indices as f64);
    case.metrics.insert(String::from("command_bytes_copied"), command_bytes_copied as f64);
    case.metrics.insert(String::from("vertex_bytes_copied"), vertex_bytes_copied as f64);
    case.metrics.insert(String::from("index_bytes_copied"), index_bytes_copied as f64);
    case.metrics.insert(String::from("command_bytes_copied_per_op"), command_bytes_copied as f64 / total_ops as f64);
    case.metrics.insert(String::from("vertex_bytes_copied_per_op"), vertex_bytes_copied as f64 / total_ops as f64);
    case.metrics.insert(String::from("index_bytes_copied_per_op"), index_bytes_copied as f64 / total_ops as f64);
    case.metrics.insert(String::from("retained_chunk_bytes"), retained_bytes as f64);
    case.metrics.insert(String::from("flat_fallback_uses"), 0.0);
    case
}

fn authoring_surface_retained_text_atlas_context_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 16 } else { 64 };
    let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(420.0));
    populate_flat_rect_surface(&mut surface, 1_000, 0);
    surface.layout(420.0, 760.0);
    let mut text = perf_text_ctx();
    let mut text_uploader = CpuUploader::default();
    let mut text_builder = ui::DrawListBuilder::new();
    let seed_label = ui::elements::Label {
        text: String::from("Retained text atlas context"),
        color: api::Color::rgba(0.12, 0.16, 0.20, 1.0),
        align: ui::elements::Align::Left,
        wrap: false,
        font_id: 0,
        font_px: 14.0,
    };
    seed_label.encode(
        api::RectF::new(0.0, 0.0, 260.0, 32.0),
        2.0,
        &mut text,
        &mut text_uploader,
        &mut text_builder,
    );
    let text_atlas_ready = text.retained_text_atlas_revision().is_some();
    let mut warm = ui::DrawListBuilder::new();
    let _ = surface.encode_retained_with_text_ctx(&mut warm, &text);
    let cached_draws = warm.drawlist().items.len() as u64;
    let cached_vertices = warm.drawlist().vertices.len() as u64;
    let cached_indices = warm.drawlist().indices.len() as u64;
    let mut builder = ui::DrawListBuilder::new();
    let mut reused = 0u64;
    let mut rebuilt = 0u64;
    let mut case = measure_cpu_case(
        "cpu.authoring.surface_retained.text_atlas_context",
        "authoring",
        smoke,
        true,
        0.16,
        loops,
        vec![String::from(
            "Clean retained UiSurface encode through the explicit text-atlas revision context path; expected path replays cached surface draws while validating current atlas revisions.",
        )],
        || {
            builder.clear();
            match surface.encode_retained_with_text_ctx(&mut builder, &text) {
                ui::RetainedDrawStatus::Reused => {
                    reused = reused.saturating_add(1);
                }
                ui::RetainedDrawStatus::Rebuilt => {
                    rebuilt = rebuilt.saturating_add(1);
                }
            }
            let dl = builder.drawlist();
            (dl.items.len() as u64)
                .saturating_add(dl.vertices.len() as u64)
                .saturating_add(dl.indices.len() as u64)
        },
    );
    let total = reused.saturating_add(rebuilt).max(1);
    case.metrics.insert(String::from("retained_reused_ops"), reused as f64);
    case.metrics.insert(String::from("retained_rebuilt_ops"), rebuilt as f64);
    case.metrics.insert(String::from("retained_reuse_ratio"), reused as f64 / total as f64);
    case.metrics
        .insert(String::from("text_atlases_checked"), if text_atlas_ready { 1.0 } else { 0.0 });
    case.metrics.insert(String::from("draw_items"), cached_draws as f64);
    case.metrics.insert(String::from("vertex_count"), cached_vertices as f64);
    case.metrics.insert(String::from("index_count"), cached_indices as f64);
    case
}

fn authoring_drawlist_text_replay_multi_atlas_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 64 } else { 256 };
    let cached = authoring_text_replay_drawlist();
    let atlases = [(api::ImageHandle(4), 3), (api::ImageHandle(9), 7)];
    let mut builder = ui::DrawListBuilder::new();
    let mut accepted = 0u64;
    let mut case = measure_cpu_case(
        "cpu.authoring.drawlist_text_replay.multi_atlas",
        "authoring",
        smoke,
        true,
        0.12,
        loops,
        vec![String::from(
            "Public cached draw-list replay with explicit text-atlas revision checks for a multi-atlas glyph drawlist.",
        )],
        || {
            builder.clear();
            if builder.append_retained_drawlist_with_text_atlas_revisions(&cached, &atlases) {
                accepted = accepted.saturating_add(1);
            }
            let dl = builder.drawlist();
            (dl.items.len() as u64)
                .saturating_add(dl.vertices.len() as u64)
                .saturating_add(dl.indices.len() as u64)
        },
    );
    case.metrics.insert(String::from("text_atlases_checked"), atlases.len() as f64);
    case.metrics.insert(String::from("glyph_runs_replayed"), cached.items.len() as f64);
    case.metrics.insert(String::from("accepted_replays"), accepted as f64);
    case
}

fn authoring_text_replay_drawlist() -> api::DrawList {
    let mut cached = api::DrawList::default();
    cached.vertices.extend_from_slice(&[
        api::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: u32::MAX },
        api::Vertex { x: 1.0, y: 0.0, u: 1.0, v: 0.0, rgba: u32::MAX },
        api::Vertex { x: 0.0, y: 1.0, u: 0.0, v: 1.0, rgba: u32::MAX },
        api::Vertex { x: 1.0, y: 1.0, u: 1.0, v: 1.0, rgba: u32::MAX },
    ]);
    cached.indices.extend_from_slice(&[0, 1, 2, 2, 1, 3]);
    cached.items.push(api::DrawCmd::GlyphRun {
        run: api::GlyphRun {
            atlas: api::ImageHandle(4),
            atlas_revision: 3,
            vb: api::VertexSpan { offset: 0, len: 4 },
            ib: api::IndexSpan { offset: 0, len: 6 },
            sdf: false,
            color: api::Color::rgba(1.0, 1.0, 1.0, 1.0),
        },
    });
    cached.items.push(api::DrawCmd::GlyphRun {
        run: api::GlyphRun {
            atlas: api::ImageHandle(9),
            atlas_revision: 7,
            vb: api::VertexSpan { offset: 0, len: 4 },
            ib: api::IndexSpan { offset: 0, len: 6 },
            sdf: true,
            color: api::Color::rgba(0.8, 0.9, 1.0, 1.0),
        },
    });
    cached
}

fn authoring_scene3d_identity() -> metal::scene3d::Mat4 {
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

fn authoring_scene3d_mixed_frame_case(smoke: bool) -> Result<PerfCaseResult> {
    let mut renderer =
        Box::new(metal::MetalRenderer::new_default().context("creating Metal renderer")?);
    renderer.resize(512, 512, 2.0).context("resizing Metal renderer")?;

    let fill_vertices = [
        metal::scene3d::Vertex3d { position: [-0.72, -0.60, 0.08] },
        metal::scene3d::Vertex3d { position: [0.18, -0.62, 0.08] },
        metal::scene3d::Vertex3d { position: [-0.40, 0.22, 0.08] },
    ];
    let fill_indices = [0_u32, 1, 2];
    let fill = renderer
        .mesh3d_create(&metal::scene3d::Mesh3dData {
            vertices: &fill_vertices,
            indices: &fill_indices,
            topology: metal::scene3d::MeshTopology::Triangles,
        })
        .context("creating scene3d fill mesh")?;

    let line_vertices = [
        metal::scene3d::Vertex3d { position: [-0.84, 0.0, 0.0] },
        metal::scene3d::Vertex3d { position: [0.84, 0.0, 0.0] },
        metal::scene3d::Vertex3d { position: [0.0, -0.84, 0.0] },
        metal::scene3d::Vertex3d { position: [0.0, 0.84, 0.0] },
    ];
    let line_indices = [0_u32, 1, 2, 3];
    let lines = renderer
        .mesh3d_create(&metal::scene3d::Mesh3dData {
            vertices: &line_vertices,
            indices: &line_indices,
            topology: metal::scene3d::MeshTopology::Lines,
        })
        .context("creating scene3d line mesh")?;

    let identity = authoring_scene3d_identity();
    let mut line_instance =
        metal::scene3d::Instance3d::new(lines, identity, api::Color::rgba(0.98, 0.30, 0.46, 1.0));
    line_instance.cull = metal::scene3d::CullMode3d::None;
    line_instance.depth_write = false;
    let instances = [
        metal::scene3d::Instance3d::new(fill, identity, api::Color::rgba(0.18, 0.72, 1.0, 1.0)),
        line_instance,
    ];
    let scene = metal::scene3d::Pass3d {
        viewport: None,
        clear_color: Some(api::Color::rgba(0.08, 0.09, 0.13, 1.0)),
        clear_depth: true,
        view_proj: identity,
        instances: &instances,
        bloom: None,
    };

    let mut overlay = api::DrawList::default();
    overlay.items.push(api::DrawCmd::RRect {
        rect: api::RectF::new(16.0, 16.0, 96.0, 28.0),
        radii: [8.0; 4],
        color: api::Color::rgba(0.92, 0.94, 0.97, 1.0),
    });

    let warmups = if smoke { 2 } else { 4 };
    let frames = if smoke { 6 } else { 12 };
    let mut frame_samples = Vec::with_capacity(frames);
    let mut encode_samples = Vec::with_capacity(frames);
    let mut gpu_samples = Vec::with_capacity(frames);
    let mut draws_sum = 0.0;

    for index in 0..(warmups + frames) {
        let frame_t0 = Instant::now();
        let token = renderer.begin_frame(&api::FrameTarget, None);
        let frame_id = token.0;
        renderer
            .encode_scene3d(&scene)
            .with_context(|| "encoding authoring scene3d mixed frame")?;
        renderer.encode_pass(&overlay);
        renderer.submit(token).with_context(|| "submitting authoring scene3d mixed frame")?;
        let frame_ms = frame_t0.elapsed().as_secs_f64() * 1000.0;
        let stats = last_metal_stats_after_submit(&renderer, frame_id);
        if index >= warmups {
            frame_samples.push(frame_ms);
            encode_samples.push(stats.encode_ms);
            gpu_samples.push(stats.gpu_ms);
            draws_sum += stats.draws as f64;
        }
    }

    renderer.mesh3d_release(fill);
    renderer.mesh3d_release(lines);

    let summary = summarize(&frame_samples);
    let (layer, scenario, variant, cache_state, refresh_mode) =
        perf_case_contract_metadata("gpu.authoring.scene3d.mixed_frame", "authoring");
    let mut metrics = BTreeMap::new();
    metrics.insert(String::from("draws_avg"), draws_sum / frames as f64);
    metrics.insert(String::from("encode_ms_median"), summarize(&encode_samples).median);
    insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
    insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
    insert_frame_pacing_metrics(&mut metrics, &frame_samples);
    metrics
        .insert(String::from("mesh_vertices"), (fill_vertices.len() + line_vertices.len()) as f64);
    metrics.insert(String::from("mesh_indices"), (fill_indices.len() + line_indices.len()) as f64);

    Ok(PerfCaseResult {
        id: String::from("gpu.authoring.scene3d.mixed_frame"),
        family: String::from("authoring"),
        layer: String::from(layer),
        scenario: String::from(scenario),
        variant: String::from(variant),
        cache_state: String::from(cache_state),
        refresh_mode: String::from(refresh_mode),
        unit: String::from("ms/frame"),
        gated: true,
        threshold_pct: 0.20,
        median: summary.median,
        p95: summary.p95,
        p99: summary.p99,
        min: summary.min,
        max: summary.max,
        mean: summary.mean,
        samples: frame_samples.len(),
        ops_per_sample: 1,
        notes: vec![String::from(
            "Persistent scene3d mesh handles rendered ahead of a 2D Oxide drawlist in the same frame to validate mixed 2D/3D authoring overhead on the Metal backend.",
        )],
        metrics,
    })
}

fn authoring_username_policy() -> ui::TextFieldPolicy {
    ui::TextFieldPolicy::new(ui::elements::CharFilter::Custom(Arc::new(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '$')
    })))
    .with_max_length(Some(15))
    .with_lowercase(true)
    .with_first_token_only_on_set(true)
}

fn authoring_bio_policy() -> ui::TextFieldPolicy {
    ui::TextFieldPolicy::new(ui::elements::CharFilter::AlphanumericPlus(String::from("@-&(),.' +")))
        .with_max_length(Some(30))
}

fn authoring_password_policy() -> ui::TextFieldPolicy {
    ui::TextFieldPolicy::new(ui::elements::CharFilter::None)
        .with_max_length(Some(30))
        .with_lowercase(true)
}

fn authoring_burst_emitter() -> ui::BurstEmitter {
    ui::BurstEmitter::new(
        ui::BurstEmitterConfig {
            active_duration_s: 1.1,
            emitter_size_scale: [1.5, 1.5],
            emitter_depth: 15.0,
            emitter_shape: ui::BurstEmitterShape::Sphere,
            cell: ui::BurstEmitterCellConfig {
                birth_rate: 25.0,
                lifetime_s: 1.0,
                velocity_points_per_s: 300.0,
                scale: 0.10,
                emission_range_rad: std::f32::consts::PI * 2.0,
                emission_longitude_rad: 0.0,
            },
        },
        2_000,
        77,
    )
}

fn authoring_surface(size: f32, color: api::Color) -> (ui::UiSurface, ui::NodeId) {
    let mut surface = ui::UiSurface::new(ui::NodeStyle {
        size: ui::Size2D { w: ui::Dim::Px(size), h: ui::Dim::Px(size) },
        background: color,
        ..ui::NodeStyle::default()
    });
    let root = surface.root();
    let content = surface.tree_mut().add_node(
        root,
        ui::NodeStyle {
            size: ui::Size2D { w: ui::Dim::Px(size * 0.5), h: ui::Dim::Px(size * 0.5) },
            ..ui::NodeStyle::default()
        },
    );
    surface.layout(size, size);
    (surface, content)
}

fn layout_case_iterations(smoke: bool) -> u64 {
    if smoke {
        4
    } else {
        16
    }
}

fn text_input_iterations(smoke: bool) -> u64 {
    if smoke {
        2
    } else {
        8
    }
}

fn endurance_iterations(smoke: bool) -> u64 {
    if smoke {
        1
    } else {
        2
    }
}

fn stress_iterations(smoke: bool) -> u64 {
    if smoke {
        1
    } else {
        2
    }
}

fn layout_flat_grid_rotation_case(smoke: bool) -> PerfCaseResult {
    let loops = layout_case_iterations(smoke);
    let mut case = measure_cpu_case(
        "cpu.layout.flat_grid.rotation_relayout",
        "layout",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Retained-tree flat grid relayout under alternating portrait and landscape widths.",
        )],
        move || {
            let mut builder = ui::DrawListBuilder::new();
            let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(360.0));
            populate_flat_rect_surface(&mut surface, 240, 0);
            surface.layout(360.0, 760.0);
            builder.clear();
            surface.encode(&mut builder);
            let portrait = builder.drawlist().items.len() as u64;
            surface.layout(640.0, 420.0);
            builder.clear();
            surface.encode(&mut builder);
            portrait + builder.drawlist().items.len() as u64
        },
    );
    case.metrics.insert(String::from("dirty_nodes"), 240.0);
    case.metrics.insert(String::from("layout_passes"), 2.0);
    case
}

fn layout_deep_stack_theme_swap_case(smoke: bool) -> PerfCaseResult {
    let loops = layout_case_iterations(smoke);
    let mut case = measure_cpu_case(
        "cpu.layout.deep_stack.theme_swap",
        "layout",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Deeply nested container stack relayout while alternating theme colors and spacing.",
        )],
        move || {
            let mut surface = ui::UiSurface::new(ui::NodeStyle {
                axis: ui::Axis::Column,
                size: ui::Size2D { w: ui::Dim::Px(360.0), h: ui::Dim::Auto },
                padding: ui::Edges { left: 12.0, top: 12.0, right: 12.0, bottom: 12.0 },
                gap: 6.0,
                background: api::Color::rgba(0.98, 0.99, 1.0, 1.0),
                ..ui::NodeStyle::default()
            });
            let mut parent = surface.root();
            let mut nodes = Vec::with_capacity(30);
            for depth in 0..30 {
                let width = (340.0 - depth as f32 * 6.0).max(108.0);
                let node = surface.tree_mut().add_node(
                    parent,
                    ui::NodeStyle {
                        axis: ui::Axis::Column,
                        size: ui::Size2D { w: ui::Dim::Px(width), h: ui::Dim::Auto },
                        padding: ui::Edges { left: 6.0, top: 6.0, right: 6.0, bottom: 6.0 },
                        gap: 4.0,
                        background: flat_rect_fill_color(depth, 0),
                        ..ui::NodeStyle::default()
                    },
                );
                let child = surface.tree_mut().add_node(
                    node,
                    ui::NodeStyle {
                        size: ui::Size2D { w: ui::Dim::Px(width - 18.0), h: ui::Dim::Px(18.0) },
                        background: api::Color::rgba(1.0, 1.0, 1.0, 0.72),
                        ..ui::NodeStyle::default()
                    },
                );
                nodes.push(node);
                nodes.push(child);
                parent = node;
            }
            surface.layout(360.0, 780.0);
            for (index, node) in nodes.iter().copied().enumerate() {
                if let Some(style) = surface.tree_mut().style_mut(node) {
                    style.background = flat_rect_fill_color(index, 2);
                    style.gap = 6.0 + (index % 3) as f32;
                }
            }
            let mut builder = ui::DrawListBuilder::new();
            builder.clear();
            surface.layout(420.0, 820.0);
            surface.encode(&mut builder);
            builder.drawlist().items.len() as u64
        },
    );
    case.metrics.insert(String::from("dirty_nodes"), 60.0);
    case.metrics.insert(String::from("layout_passes"), 2.0);
    case
}

fn layout_grid_safe_area_case(smoke: bool) -> PerfCaseResult {
    let loops = layout_case_iterations(smoke);
    let mut case = measure_cpu_case(
        "cpu.layout.grid.safe_area_swap",
        "layout",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Retained grid relayout while alternating root insets to mimic safe-area changes.",
        )],
        move || {
            let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(420.0));
            let root = surface.root();
            populate_flat_rect_surface(&mut surface, 180, 0);
            surface.layout(420.0, 760.0);
            if let Some(style) = surface.tree_mut().style_mut(root) {
                style.padding = ui::Edges { left: 32.0, top: 44.0, right: 24.0, bottom: 28.0 };
            }
            let mut builder = ui::DrawListBuilder::new();
            builder.clear();
            surface.layout(420.0, 760.0);
            surface.encode(&mut builder);
            let first = builder.drawlist().items.len() as u64;
            if let Some(style) = surface.tree_mut().style_mut(root) {
                style.padding = ui::Edges { left: 8.0, top: 8.0, right: 8.0, bottom: 8.0 };
            }
            builder.clear();
            surface.layout(420.0, 760.0);
            surface.encode(&mut builder);
            first + builder.drawlist().items.len() as u64
        },
    );
    case.metrics.insert(String::from("dirty_nodes"), 180.0);
    case.metrics.insert(String::from("layout_passes"), 3.0);
    case
}

fn layout_dirty_subtree_incremental_case(smoke: bool) -> PerfCaseResult {
    let loops = layout_case_iterations(smoke);
    let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(420.0));
    let nodes = populate_flat_rect_surface(&mut surface, 1_000, 0);
    let cold = surface.layout(420.0, 760.0);
    let mut step = 0usize;
    let mut ops = 0u64;
    let mut visited_nodes = 0u64;
    let mut skipped_subtrees = 0u64;
    let mut layout_updates = 0u64;
    let mut measured_children = 0u64;
    let mut case = measure_cpu_case(
        "cpu.layout.dirty_subtree.incremental_relayout",
        "layout",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "One-cell layout mutation over a 1000-node retained surface; clean sibling rows should skip through per-node layout dirtiness.",
        )],
        || {
            let target = nodes.cells[step % nodes.cells.len()];
            let width = 29.0 + (step % 7) as f32;
            step = step.wrapping_add(1);
            let _ = surface.edit_style(target, |style| {
                style.size.w = ui::Dim::Px(width);
            });
            let stats = surface.layout(420.0, 760.0);
            ops = ops.saturating_add(1);
            visited_nodes = visited_nodes.saturating_add(stats.visited_nodes as u64);
            skipped_subtrees = skipped_subtrees.saturating_add(stats.skipped_subtrees as u64);
            layout_updates = layout_updates.saturating_add(stats.layout_updates as u64);
            measured_children = measured_children.saturating_add(stats.measured_children as u64);
            stats.visited_nodes as u64
                + stats.skipped_subtrees as u64
                + stats.layout_updates as u64
                + stats.measured_children as u64
        },
    );
    let total_ops = ops.max(1) as f64;
    case.metrics.insert(String::from("dirty_nodes"), 1.0);
    case.metrics.insert(String::from("layout_passes"), 1.0);
    case.metrics.insert(String::from("layout_ops_sampled"), ops as f64);
    case.metrics.insert(String::from("cold_visited_nodes"), cold.visited_nodes as f64);
    case.metrics.insert(String::from("cold_measured_children"), cold.measured_children as f64);
    case.metrics
        .insert(String::from("layout_visited_nodes_per_op"), visited_nodes as f64 / total_ops);
    case.metrics.insert(
        String::from("layout_skipped_subtrees_per_op"),
        skipped_subtrees as f64 / total_ops,
    );
    case.metrics.insert(String::from("layout_updates_per_op"), layout_updates as f64 / total_ops);
    case.metrics.insert(
        String::from("layout_measured_children_per_op"),
        measured_children as f64 / total_ops,
    );
    case
}

fn descendant_only_layout_surface() -> (ui::UiSurface, ui::NodeId) {
    let mut surface = ui::UiSurface::new(ui::NodeStyle {
        axis: ui::Axis::Row,
        size: ui::Size2D { w: ui::Dim::Px(420.0), h: ui::Dim::Px(160.0) },
        gap: 4.0,
        ..ui::NodeStyle::default()
    });
    let root = surface.root();
    for column in 0..64 {
        let branch = surface.tree_mut().add_node(
            root,
            ui::NodeStyle {
                axis: ui::Axis::Column,
                size: ui::Size2D { w: ui::Dim::Px(5.0), h: ui::Dim::Px(140.0) },
                background: flat_rect_fill_color(column, 0),
                ..ui::NodeStyle::default()
            },
        );
        let _ = surface.tree_mut().add_node(
            branch,
            ui::NodeStyle {
                size: ui::Size2D { w: ui::Dim::Px(3.0), h: ui::Dim::Px(40.0) },
                background: flat_rect_fill_color(column, 2),
                ..ui::NodeStyle::default()
            },
        );
    }
    (surface, ui::NodeId(66))
}

fn scoped_tree_mutation_surface() -> (ui::UiSurface, ui::NodeId) {
    let mut surface = ui::UiSurface::new(ui::NodeStyle {
        axis: ui::Axis::Row,
        size: ui::Size2D { w: ui::Dim::Px(420.0), h: ui::Dim::Px(160.0) },
        gap: 4.0,
        ..ui::NodeStyle::default()
    });
    let root = surface.root();
    let mut target = root;
    for column in 0..64 {
        let branch = surface
            .add_node(
                root,
                ui::NodeStyle {
                    axis: ui::Axis::Column,
                    size: ui::Size2D { w: ui::Dim::Px(5.0), h: ui::Dim::Px(140.0) },
                    background: flat_rect_fill_color(column, 0),
                    ..ui::NodeStyle::default()
                },
            )
            .unwrap_or(root);
        let _ = surface.add_node(
            branch,
            ui::NodeStyle {
                size: ui::Size2D { w: ui::Dim::Px(3.0), h: ui::Dim::Px(40.0) },
                background: flat_rect_fill_color(column, 2),
                ..ui::NodeStyle::default()
            },
        );
        if column == 47 {
            target = branch;
        }
    }
    (surface, target)
}

fn layout_descendant_only_incremental_case(smoke: bool) -> PerfCaseResult {
    let loops = layout_case_iterations(smoke);
    let (mut surface, target) = descendant_only_layout_surface();
    let cold = surface.layout(420.0, 160.0);
    let mut step = 0usize;
    let mut ops = 0u64;
    let mut visited_nodes = 0u64;
    let mut skipped_subtrees = 0u64;
    let mut layout_updates = 0u64;
    let mut measured_children = 0u64;
    let mut case = measure_cpu_case(
        "cpu.layout.descendant_only.incremental_relayout",
        "layout",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Fixed-size child padding mutation over a wide retained row; parent geometry remains stable so clean siblings should skip without parent child-measure scans.",
        )],
        || {
            let pad = if step & 1 == 0 { 1.0 } else { 2.0 };
            step = step.wrapping_add(1);
            let _ = surface.edit_style(target, |style| {
                style.padding = ui::Edges { left: pad, top: 0.0, right: 0.0, bottom: 0.0 };
            });
            let stats = surface.layout(420.0, 160.0);
            ops = ops.saturating_add(1);
            visited_nodes = visited_nodes.saturating_add(stats.visited_nodes as u64);
            skipped_subtrees = skipped_subtrees.saturating_add(stats.skipped_subtrees as u64);
            layout_updates = layout_updates.saturating_add(stats.layout_updates as u64);
            measured_children = measured_children.saturating_add(stats.measured_children as u64);
            stats.visited_nodes as u64
                + stats.skipped_subtrees as u64
                + stats.layout_updates as u64
                + stats.measured_children as u64
        },
    );
    let total_ops = ops.max(1) as f64;
    case.metrics.insert(String::from("dirty_nodes"), 1.0);
    case.metrics.insert(String::from("layout_passes"), 1.0);
    case.metrics.insert(String::from("layout_ops_sampled"), ops as f64);
    case.metrics.insert(String::from("cold_visited_nodes"), cold.visited_nodes as f64);
    case.metrics.insert(String::from("cold_measured_children"), cold.measured_children as f64);
    case.metrics
        .insert(String::from("layout_visited_nodes_per_op"), visited_nodes as f64 / total_ops);
    case.metrics.insert(
        String::from("layout_skipped_subtrees_per_op"),
        skipped_subtrees as f64 / total_ops,
    );
    case.metrics.insert(String::from("layout_updates_per_op"), layout_updates as f64 / total_ops);
    case.metrics.insert(
        String::from("layout_measured_children_per_op"),
        measured_children as f64 / total_ops,
    );
    case
}

fn layout_transform_only_reposition_case(smoke: bool) -> PerfCaseResult {
    let loops = layout_case_iterations(smoke);
    let (mut surface, target) = descendant_only_layout_surface();
    let cold = surface.layout(420.0, 160.0);
    let mut builder = ui::DrawListBuilder::new();
    let _ = surface.encode_retained(&mut builder);
    let mut step = 0usize;
    let mut ops = 0u64;
    let mut visited_nodes = 0u64;
    let mut skipped_subtrees = 0u64;
    let mut layout_updates = 0u64;
    let mut measured_children = 0u64;
    let mut reused_nodes = 0u64;
    let mut rebuilt_nodes = 0u64;
    let mut case = measure_cpu_case(
        "cpu.layout.transform_only.reposition",
        "layout",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Transform-only child reposition over a retained row; logical layout should remain clean while draw and hit-test state update.",
        )],
        || {
            let tx = if step & 1 == 0 { 3.0 } else { 9.0 };
            let ty = if step & 1 == 0 { 2.0 } else { 5.0 };
            step = step.wrapping_add(1);
            let _ = surface.edit_style(target, |style| {
                style.transform = platform::Transform2D { tx, ty, sx: 1.0, sy: 1.0, rot_rad: 0.0 };
            });
            let stats = surface.layout(420.0, 160.0);
            builder.clear();
            let _ = surface.encode_retained(&mut builder);
            let retained = surface.retained_node_stats();
            ops = ops.saturating_add(1);
            visited_nodes = visited_nodes.saturating_add(stats.visited_nodes as u64);
            skipped_subtrees = skipped_subtrees.saturating_add(stats.skipped_subtrees as u64);
            layout_updates = layout_updates.saturating_add(stats.layout_updates as u64);
            measured_children = measured_children.saturating_add(stats.measured_children as u64);
            reused_nodes = reused_nodes.saturating_add(retained.reused_nodes as u64);
            rebuilt_nodes = rebuilt_nodes.saturating_add(retained.rebuilt_nodes as u64);
            builder.drawlist().items.len() as u64
                + stats.visited_nodes as u64
                + stats.measured_children as u64
                + retained.reused_nodes as u64
                + retained.rebuilt_nodes as u64
        },
    );
    let total_ops = ops.max(1) as f64;
    case.metrics.insert(String::from("dirty_nodes"), 1.0);
    case.metrics.insert(String::from("layout_passes"), 0.0);
    case.metrics.insert(String::from("layout_ops_sampled"), ops as f64);
    case.metrics.insert(String::from("cold_visited_nodes"), cold.visited_nodes as f64);
    case.metrics.insert(String::from("cold_measured_children"), cold.measured_children as f64);
    case.metrics
        .insert(String::from("layout_visited_nodes_per_op"), visited_nodes as f64 / total_ops);
    case.metrics.insert(
        String::from("layout_skipped_subtrees_per_op"),
        skipped_subtrees as f64 / total_ops,
    );
    case.metrics.insert(String::from("layout_updates_per_op"), layout_updates as f64 / total_ops);
    case.metrics.insert(
        String::from("layout_measured_children_per_op"),
        measured_children as f64 / total_ops,
    );
    case.metrics
        .insert(String::from("retained_reused_nodes_per_op"), reused_nodes as f64 / total_ops);
    case.metrics
        .insert(String::from("retained_rebuilt_nodes_per_op"), rebuilt_nodes as f64 / total_ops);
    case
}

fn layout_paint_only_opacity_clip_case(smoke: bool) -> PerfCaseResult {
    let loops = layout_case_iterations(smoke);
    let (mut surface, target) = descendant_only_layout_surface();
    let cold = surface.layout(420.0, 160.0);
    let mut builder = ui::DrawListBuilder::new();
    let _ = surface.encode_retained(&mut builder);
    let mut step = 0usize;
    let mut ops = 0u64;
    let mut opacity_ops = 0u64;
    let mut clip_ops = 0u64;
    let mut visited_nodes = 0u64;
    let mut skipped_subtrees = 0u64;
    let mut layout_updates = 0u64;
    let mut measured_children = 0u64;
    let mut reused_nodes = 0u64;
    let mut rebuilt_nodes = 0u64;
    let mut case = measure_cpu_case(
        "cpu.layout.paint_only.opacity_clip",
        "layout",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Opacity and clip edits over a retained row; layout should stay clean while affected ancestors rebuild and clean sibling subtrees replay.",
        )],
        || {
            let phase = step;
            step = step.wrapping_add(1);
            if phase & 1 == 0 {
                let opacity = if phase & 2 == 0 { 0.56 } else { 0.84 };
                let _ = surface.edit_style(target, |style| {
                    style.opacity = opacity;
                });
                opacity_ops = opacity_ops.saturating_add(1);
            } else {
                let clip = phase & 2 == 0;
                let _ = surface.edit_style(target, |style| {
                    style.clip = clip;
                });
                clip_ops = clip_ops.saturating_add(1);
            }
            let stats = surface.layout(420.0, 160.0);
            builder.clear();
            let _ = surface.encode_retained(&mut builder);
            let retained = surface.retained_node_stats();
            ops = ops.saturating_add(1);
            visited_nodes = visited_nodes.saturating_add(stats.visited_nodes as u64);
            skipped_subtrees = skipped_subtrees.saturating_add(stats.skipped_subtrees as u64);
            layout_updates = layout_updates.saturating_add(stats.layout_updates as u64);
            measured_children = measured_children.saturating_add(stats.measured_children as u64);
            reused_nodes = reused_nodes.saturating_add(retained.reused_nodes as u64);
            rebuilt_nodes = rebuilt_nodes.saturating_add(retained.rebuilt_nodes as u64);
            builder.drawlist().items.len() as u64
                + stats.visited_nodes as u64
                + stats.measured_children as u64
                + retained.reused_nodes as u64
                + retained.rebuilt_nodes as u64
        },
    );
    let total_ops = ops.max(1) as f64;
    case.metrics.insert(String::from("dirty_nodes"), 1.0);
    case.metrics.insert(String::from("layout_passes"), 0.0);
    case.metrics.insert(String::from("layout_ops_sampled"), ops as f64);
    case.metrics.insert(String::from("opacity_ops"), opacity_ops as f64);
    case.metrics.insert(String::from("clip_ops"), clip_ops as f64);
    case.metrics.insert(String::from("cold_visited_nodes"), cold.visited_nodes as f64);
    case.metrics.insert(String::from("cold_measured_children"), cold.measured_children as f64);
    case.metrics
        .insert(String::from("layout_visited_nodes_per_op"), visited_nodes as f64 / total_ops);
    case.metrics.insert(
        String::from("layout_skipped_subtrees_per_op"),
        skipped_subtrees as f64 / total_ops,
    );
    case.metrics.insert(String::from("layout_updates_per_op"), layout_updates as f64 / total_ops);
    case.metrics.insert(
        String::from("layout_measured_children_per_op"),
        measured_children as f64 / total_ops,
    );
    case.metrics
        .insert(String::from("retained_reused_nodes_per_op"), reused_nodes as f64 / total_ops);
    case.metrics
        .insert(String::from("retained_rebuilt_nodes_per_op"), rebuilt_nodes as f64 / total_ops);
    case
}

fn layout_node_content_dirty_retained_replay_case(smoke: bool) -> PerfCaseResult {
    let loops = layout_case_iterations(smoke);
    let (mut surface, target) = descendant_only_layout_surface();
    let cold = surface.layout(420.0, 160.0);
    let mut builder = ui::DrawListBuilder::new();
    let _ = surface.encode_retained(&mut builder);
    let mut step = 0usize;
    let mut ops = 0u64;
    let mut text_ops = 0u64;
    let mut image_ops = 0u64;
    let mut camera_ops = 0u64;
    let mut visited_nodes = 0u64;
    let mut skipped_subtrees = 0u64;
    let mut layout_updates = 0u64;
    let mut measured_children = 0u64;
    let mut reused_nodes = 0u64;
    let mut rebuilt_nodes = 0u64;
    let mut case = measure_cpu_case(
        "cpu.layout.node_content_dirty.retained_replay",
        "layout",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Node-scoped text/image/camera content dirtying over a retained row; layout should stay clean while clean sibling subtrees replay.",
        )],
        || {
            let class = match step % 3 {
                0 => {
                    text_ops = text_ops.saturating_add(1);
                    ui::DirtyClass::Text
                }
                1 => {
                    image_ops = image_ops.saturating_add(1);
                    ui::DirtyClass::ImageContent
                }
                _ => {
                    camera_ops = camera_ops.saturating_add(1);
                    ui::DirtyClass::CameraFrame
                }
            };
            step = step.wrapping_add(1);
            let _ = surface.mark_node_dirty(target, class);
            let stats = surface.layout(420.0, 160.0);
            builder.clear();
            let _ = surface.encode_retained(&mut builder);
            let retained = surface.retained_node_stats();
            ops = ops.saturating_add(1);
            visited_nodes = visited_nodes.saturating_add(stats.visited_nodes as u64);
            skipped_subtrees = skipped_subtrees.saturating_add(stats.skipped_subtrees as u64);
            layout_updates = layout_updates.saturating_add(stats.layout_updates as u64);
            measured_children = measured_children.saturating_add(stats.measured_children as u64);
            reused_nodes = reused_nodes.saturating_add(retained.reused_nodes as u64);
            rebuilt_nodes = rebuilt_nodes.saturating_add(retained.rebuilt_nodes as u64);
            builder.drawlist().items.len() as u64
                + stats.visited_nodes as u64
                + stats.measured_children as u64
                + retained.reused_nodes as u64
                + retained.rebuilt_nodes as u64
        },
    );
    let total_ops = ops.max(1) as f64;
    case.metrics.insert(String::from("dirty_nodes"), 1.0);
    case.metrics.insert(String::from("layout_passes"), 0.0);
    case.metrics.insert(String::from("layout_ops_sampled"), ops as f64);
    case.metrics.insert(String::from("text_dirty_ops"), text_ops as f64);
    case.metrics.insert(String::from("image_dirty_ops"), image_ops as f64);
    case.metrics.insert(String::from("camera_dirty_ops"), camera_ops as f64);
    case.metrics.insert(String::from("cold_visited_nodes"), cold.visited_nodes as f64);
    case.metrics.insert(String::from("cold_measured_children"), cold.measured_children as f64);
    case.metrics
        .insert(String::from("layout_visited_nodes_per_op"), visited_nodes as f64 / total_ops);
    case.metrics.insert(
        String::from("layout_skipped_subtrees_per_op"),
        skipped_subtrees as f64 / total_ops,
    );
    case.metrics.insert(String::from("layout_updates_per_op"), layout_updates as f64 / total_ops);
    case.metrics.insert(
        String::from("layout_measured_children_per_op"),
        measured_children as f64 / total_ops,
    );
    case.metrics
        .insert(String::from("retained_reused_nodes_per_op"), reused_nodes as f64 / total_ops);
    case.metrics
        .insert(String::from("retained_rebuilt_nodes_per_op"), rebuilt_nodes as f64 / total_ops);
    case
}

fn layout_non_draw_dirty_retained_reuse_case(smoke: bool) -> PerfCaseResult {
    let loops = layout_case_iterations(smoke);
    let (mut surface, target) = descendant_only_layout_surface();
    let cold = surface.layout(420.0, 160.0);
    let mut builder = ui::DrawListBuilder::new();
    let _ = surface.encode_retained(&mut builder);
    let mut step = 0usize;
    let mut ops = 0u64;
    let mut accessibility_ops = 0u64;
    let mut hit_test_ops = 0u64;
    let mut retained_reused_ops = 0u64;
    let mut retained_rebuilt_ops = 0u64;
    let mut visited_nodes = 0u64;
    let mut skipped_subtrees = 0u64;
    let mut layout_updates = 0u64;
    let mut measured_children = 0u64;
    let mut reused_nodes = 0u64;
    let mut rebuilt_nodes = 0u64;
    let mut case = measure_cpu_case(
        "cpu.layout.non_draw_dirty.retained_reuse",
        "layout",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Node-scoped accessibility and hit-test dirtying over a retained row; layout and draw caches should stay reusable.",
        )],
        || {
            let class = if step & 1 == 0 {
                accessibility_ops = accessibility_ops.saturating_add(1);
                ui::DirtyClass::Accessibility
            } else {
                hit_test_ops = hit_test_ops.saturating_add(1);
                ui::DirtyClass::HitTest
            };
            step = step.wrapping_add(1);
            let _ = surface.mark_node_dirty(target, class);
            let stats = surface.layout(420.0, 160.0);
            builder.clear();
            let status = surface.encode_retained(&mut builder);
            match status {
                ui::RetainedDrawStatus::Reused => {
                    retained_reused_ops = retained_reused_ops.saturating_add(1);
                }
                ui::RetainedDrawStatus::Rebuilt => {
                    retained_rebuilt_ops = retained_rebuilt_ops.saturating_add(1);
                }
            }
            let retained = surface.retained_node_stats();
            ops = ops.saturating_add(1);
            visited_nodes = visited_nodes.saturating_add(stats.visited_nodes as u64);
            skipped_subtrees = skipped_subtrees.saturating_add(stats.skipped_subtrees as u64);
            layout_updates = layout_updates.saturating_add(stats.layout_updates as u64);
            measured_children = measured_children.saturating_add(stats.measured_children as u64);
            reused_nodes = reused_nodes.saturating_add(retained.reused_nodes as u64);
            rebuilt_nodes = rebuilt_nodes.saturating_add(retained.rebuilt_nodes as u64);
            builder.drawlist().items.len() as u64
                + stats.visited_nodes as u64
                + stats.measured_children as u64
                + retained.reused_nodes as u64
                + retained.rebuilt_nodes as u64
        },
    );
    let total_ops = ops.max(1) as f64;
    case.metrics.insert(String::from("dirty_nodes"), 1.0);
    case.metrics.insert(String::from("layout_passes"), 0.0);
    case.metrics.insert(String::from("layout_ops_sampled"), ops as f64);
    case.metrics.insert(String::from("accessibility_dirty_ops"), accessibility_ops as f64);
    case.metrics.insert(String::from("hit_test_dirty_ops"), hit_test_ops as f64);
    case.metrics.insert(String::from("retained_reused_ops"), retained_reused_ops as f64);
    case.metrics.insert(String::from("retained_rebuilt_ops"), retained_rebuilt_ops as f64);
    case.metrics.insert(String::from("cold_visited_nodes"), cold.visited_nodes as f64);
    case.metrics.insert(String::from("cold_measured_children"), cold.measured_children as f64);
    case.metrics
        .insert(String::from("layout_visited_nodes_per_op"), visited_nodes as f64 / total_ops);
    case.metrics.insert(
        String::from("layout_skipped_subtrees_per_op"),
        skipped_subtrees as f64 / total_ops,
    );
    case.metrics.insert(String::from("layout_updates_per_op"), layout_updates as f64 / total_ops);
    case.metrics.insert(
        String::from("layout_measured_children_per_op"),
        measured_children as f64 / total_ops,
    );
    case.metrics
        .insert(String::from("retained_reused_nodes_per_op"), reused_nodes as f64 / total_ops);
    case.metrics
        .insert(String::from("retained_rebuilt_nodes_per_op"), rebuilt_nodes as f64 / total_ops);
    case
}

fn layout_scoped_tree_mutation_add_remove_case(smoke: bool) -> PerfCaseResult {
    let loops = layout_case_iterations(smoke);
    let (mut surface, target) = scoped_tree_mutation_surface();
    let cold = surface.layout(420.0, 160.0);
    let mut builder = ui::DrawListBuilder::new();
    let _ = surface.encode_retained(&mut builder);
    let mut inserted: Option<ui::NodeId> = None;
    let mut step = 0usize;
    let mut ops = 0u64;
    let mut add_ops = 0u64;
    let mut remove_ops = 0u64;
    let mut visited_nodes = 0u64;
    let mut skipped_subtrees = 0u64;
    let mut layout_updates = 0u64;
    let mut measured_children = 0u64;
    let mut reused_nodes = 0u64;
    let mut rebuilt_nodes = 0u64;
    let mut case = measure_cpu_case(
        "cpu.layout.scoped_tree_mutation.add_remove",
        "layout",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Scoped add/remove child mutation inside one retained branch; clean sibling branches should skip layout and replay retained draws.",
        )],
        || {
            if let Some(node) = inserted.take() {
                let _ = surface.remove_node(node);
                remove_ops = remove_ops.saturating_add(1);
            } else {
                inserted = surface.add_node(
                    target,
                    ui::NodeStyle {
                        size: ui::Size2D { w: ui::Dim::Px(3.0), h: ui::Dim::Px(24.0) },
                        background: flat_rect_fill_color(step, 3),
                        ..ui::NodeStyle::default()
                    },
                );
                add_ops = add_ops.saturating_add(1);
            }
            step = step.wrapping_add(1);
            let stats = surface.layout(420.0, 160.0);
            builder.clear();
            let _ = surface.encode_retained(&mut builder);
            let retained = surface.retained_node_stats();
            ops = ops.saturating_add(1);
            visited_nodes = visited_nodes.saturating_add(stats.visited_nodes as u64);
            skipped_subtrees = skipped_subtrees.saturating_add(stats.skipped_subtrees as u64);
            layout_updates = layout_updates.saturating_add(stats.layout_updates as u64);
            measured_children = measured_children.saturating_add(stats.measured_children as u64);
            reused_nodes = reused_nodes.saturating_add(retained.reused_nodes as u64);
            rebuilt_nodes = rebuilt_nodes.saturating_add(retained.rebuilt_nodes as u64);
            builder.drawlist().items.len() as u64
                + stats.visited_nodes as u64
                + stats.skipped_subtrees as u64
                + retained.reused_nodes as u64
                + retained.rebuilt_nodes as u64
        },
    );
    let total_ops = ops.max(1) as f64;
    case.metrics.insert(String::from("dirty_nodes"), 1.0);
    case.metrics.insert(String::from("layout_passes"), 1.0);
    case.metrics.insert(String::from("layout_ops_sampled"), ops as f64);
    case.metrics.insert(String::from("scoped_add_ops"), add_ops as f64);
    case.metrics.insert(String::from("scoped_remove_ops"), remove_ops as f64);
    case.metrics.insert(String::from("cold_visited_nodes"), cold.visited_nodes as f64);
    case.metrics.insert(String::from("cold_measured_children"), cold.measured_children as f64);
    case.metrics
        .insert(String::from("layout_visited_nodes_per_op"), visited_nodes as f64 / total_ops);
    case.metrics.insert(
        String::from("layout_skipped_subtrees_per_op"),
        skipped_subtrees as f64 / total_ops,
    );
    case.metrics.insert(String::from("layout_updates_per_op"), layout_updates as f64 / total_ops);
    case.metrics.insert(
        String::from("layout_measured_children_per_op"),
        measured_children as f64 / total_ops,
    );
    case.metrics
        .insert(String::from("retained_reused_nodes_per_op"), reused_nodes as f64 / total_ops);
    case.metrics
        .insert(String::from("retained_rebuilt_nodes_per_op"), rebuilt_nodes as f64 / total_ops);
    case
}

fn large_editor_seed_text(lines: usize) -> String {
    let mut text = String::new();
    for line in 0..lines {
        let _ = std::fmt::Write::write_fmt(
            &mut text,
            format_args!(
                "Orbit {} telemetry line {} retains enough prose to force multiline wrapping.\n",
                line % 17,
                line
            ),
        );
    }
    text
}

fn text_input_large_editor_keystroke_case(smoke: bool) -> PerfCaseResult {
    let loops = text_input_iterations(smoke);
    let seed = large_editor_seed_text(96);
    let mut case = measure_cpu_case(
        "cpu.text_input.large_editor.keystroke_burst",
        "text-input",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Large-editor typing burst using the public text-input state engine on a multiline document.",
        )],
        move || {
            let mut state = ui::elements::TextInputState::new("Editor");
            state.focus();
            state.set_text(seed.clone());
            state.move_cursor_to_end();
            for chunk in 0..32 {
                let text = if chunk % 4 == 0 { "\npatch" } else { " patch" };
                state.handle_text_event(&platform::TextEvent::Commit { text: text.to_string() });
            }
            state.tick(16);
            state.handle_key(&platform::KeyEvent {
                code: platform::KeyCode::Backspace,
                chars: None,
                repeat: false,
                modifiers: platform::Modifiers::empty(),
            });
            state.text().len() as u64 + state.ime_rect().is_some() as u64
        },
    );
    case.metrics.insert(String::from("dirty_nodes"), 1.0);
    case
}

fn text_input_large_editor_paste_case(smoke: bool) -> PerfCaseResult {
    let loops = text_input_iterations(smoke);
    let seed = large_editor_seed_text(64);
    let paste = "paste-block ".repeat(860);
    let mut case = measure_cpu_case(
        "cpu.text_input.large_editor.paste_10kb",
        "text-input",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Large-editor 10 KB paste path through clipboard-backed public text-input APIs.",
        )],
        move || {
            let mut state = ui::elements::TextInputState::new("Editor");
            state.focus();
            state.set_text(seed.clone());
            state.set_selection(48, 128);
            let _ = platform::clipboard::write_string(&paste);
            let pasted = state.paste_from_clipboard() as u64;
            state.tick(16);
            pasted + state.text().len() as u64
        },
    );
    case.metrics.insert(String::from("dirty_nodes"), 1.0);
    case
}

fn text_input_large_editor_selection_case(smoke: bool) -> PerfCaseResult {
    let loops = text_input_iterations(smoke);
    let seed = large_editor_seed_text(128);
    let mut case = measure_cpu_case(
        "cpu.text_input.large_editor.selection_replace",
        "text-input",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Large-editor selection, cut, replace, and submit flow through the public text-input APIs.",
        )],
        move || {
            let mut state = ui::elements::TextInputState::new("Editor");
            state.focus();
            state.set_text(seed.clone());
            state.set_selection(120, 260);
            let copied = state.copy_selection_to_clipboard() as u64;
            let cut = state.cut_selection_to_clipboard() as u64;
            state.handle_text_event(&platform::TextEvent::Commit {
                text: String::from("[selection replaced]"),
            });
            state.handle_key(&platform::KeyEvent {
                code: platform::KeyCode::Enter,
                chars: None,
                repeat: false,
                modifiers: platform::Modifiers::empty(),
            });
            copied + cut + state.take_submit() as u64 + state.text().len() as u64
        },
    );
    case.metrics.insert(String::from("dirty_nodes"), 1.0);
    case
}

fn text_input_ime_composition_case(smoke: bool) -> PerfCaseResult {
    let loops = text_input_iterations(smoke);
    let seed = large_editor_seed_text(32);
    let base_cursor = seed.chars().count() as u32;
    let mut case = measure_cpu_case(
        "cpu.text_input.ime.composition_commit_cycle",
        "text-input",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Focused text-input IME marked-text updates, Unicode commit, selection sync, cancellation, and keyboard geometry events.",
        )],
        move || {
            let mut state = ui::elements::TextInputState::new("Message");
            state.focus();
            state.set_text(seed.clone());
            state.move_cursor_to_end();
            state.handle_text_event(&platform::TextEvent::IMEShown(api::RectF::new(
                0.0, 520.0, 390.0, 324.0,
            )));
            state.handle_text_event(&platform::TextEvent::Composition {
                range: base_cursor..base_cursor,
                text: String::from("に"),
            });
            state.handle_text_event(&platform::TextEvent::Composition {
                range: base_cursor..base_cursor,
                text: String::from("日本"),
            });
            state.handle_text_event(&platform::TextEvent::Commit {
                text: String::from("日本語"),
            });
            state.handle_text_event(&platform::TextEvent::SelectionChanged {
                range: base_cursor..base_cursor + 3,
            });
            state.handle_text_event(&platform::TextEvent::Composition {
                range: base_cursor..base_cursor + 3,
                text: String::from("かな"),
            });
            state.handle_text_event(&platform::TextEvent::IMEHidden);
            state.handle_text_event(&platform::TextEvent::Composition {
                range: base_cursor..base_cursor,
                text: String::from("한"),
            });
            state.handle_text_event(&platform::TextEvent::Commit {
                text: String::from("한글"),
            });
            state.tick(16);
            state.text().len() as u64 + state.ime_rect().is_some() as u64
        },
    );
    case.metrics.insert(String::from("dirty_nodes"), 1.0);
    case.metrics.insert(String::from("composition_updates_per_op"), 4.0);
    case.metrics.insert(String::from("selection_sync_events_per_op"), 1.0);
    case.metrics.insert(String::from("ime_geometry_events_per_op"), 2.0);
    case
}

fn text_cursor_map_stats(map: &text::ShapedCursorMap) -> TextCursorMapStats {
    let mut boundary_checksum = 0xcbf2_9ce4_8422_2325_u64;
    let mut affinity_splits = 0_u64;
    let mut min_width = f32::MAX;
    let mut max_width = f32::MIN;
    for cursor in 0..=map.len() {
        boundary_checksum =
            boundary_checksum.wrapping_mul(0x1000_0000_01b3) ^ (map.byte_index(cursor) as u64);
        let downstream = map.width_at_with_affinity(cursor, text::CaretAffinity::Downstream);
        let upstream = map.width_at_with_affinity(cursor, text::CaretAffinity::Upstream);
        if (downstream - upstream).abs() > 0.001 {
            affinity_splits = affinity_splits.saturating_add(1);
        }
        min_width = min_width.min(downstream).min(upstream);
        max_width = max_width.max(downstream).max(upstream);
    }
    TextCursorMapStats {
        cursor_count: map.len() as u64,
        byte_boundaries: map.len().saturating_add(1) as u64,
        boundary_checksum,
        affinity_splits,
        width_span: (max_width - min_width).max(0.0) as f64,
    }
}

fn shaped_cursor_map_stats(
    fonts: &text::FontDb,
    font_id: usize,
    text_value: &str,
    font_px: f32,
) -> TextCursorMapStats {
    let Some(font) = fonts.font(font_id) else {
        return TextCursorMapStats::default();
    };
    let mut shaper = text::TextShaper::default();
    shaper
        .shape(font, font_id, text_value, font_px)
        .ok()
        .map(|shape| text_cursor_map_stats(&shape.cursor_map_for_text(text_value)))
        .unwrap_or_default()
}

fn fallback_cursor_map_stats(
    fonts: &text::FontDb,
    primary_id: usize,
    fallback_ids: &[usize],
    text_value: &str,
    font_px: f32,
) -> (TextCursorMapStats, u64) {
    let mut shaper = text::TextShaper::default();
    let stats = shaper
        .cursor_map_with_fallback_fonts(fonts, primary_id, fallback_ids, text_value, font_px)
        .map(|map| text_cursor_map_stats(&map))
        .unwrap_or_default();
    let mut shaper = text::TextShaper::default();
    let runs = shaper
        .shape_with_fallback_fonts(fonts, primary_id, fallback_ids, text_value, font_px)
        .map(|shape| shape.runs.len() as u64)
        .unwrap_or(0);
    (stats, runs)
}

fn insert_text_cursor_map_metrics(
    case: &mut PerfCaseResult,
    stats: TextCursorMapStats,
    prefix: &str,
) {
    case.metrics.insert(format!("{prefix}_cursor_count"), stats.cursor_count as f64);
    case.metrics.insert(format!("{prefix}_byte_boundaries"), stats.byte_boundaries as f64);
    case.metrics.insert(format!("{prefix}_boundary_checksum"), stats.boundary_checksum as f64);
    case.metrics.insert(format!("{prefix}_affinity_splits"), stats.affinity_splits as f64);
    case.metrics.insert(format!("{prefix}_width_span"), stats.width_span);
}

fn text_input_cursor_pick_cluster_map_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 24 } else { 192 };
    let mut text_ctx = perf_text_ctx();
    let style = ui::elements::TextInputStyle {
        font_id: 0,
        font_px: 16.0,
        ..ui::elements::TextInputStyle::default()
    };
    let seed = format!(
        "{} {}",
        large_editor_seed_text(24),
        "Cafe\u{301} 👨‍👩‍👧‍👦 日本語 Hangul 한글 cursor pressure."
    );
    let map_stats = shaped_cursor_map_stats(&text_ctx.fonts, style.font_id, &seed, style.font_px);
    let mut state = ui::elements::TextInputState::new("Editor");
    state.focus();
    state.set_text(seed.clone());
    let positions = [
        style.padding.left,
        style.padding.left + 24.0,
        style.padding.left + 96.0,
        style.padding.left + 188.0,
        style.padding.left + 320.0,
        style.padding.left + 520.0,
    ];
    let mut cursor_sum = 0u64;
    let mut step = 0usize;
    let mut case = measure_cpu_case(
        "cpu.text_input.cursor_pick.cluster_map",
        "text-input",
        smoke,
        true,
        0.16,
        loops,
        vec![String::from(
            "Hot pointer-to-cursor mapping over a long Unicode text-input line using a shaped cluster prefix map.",
        )],
        || {
            let x = positions[step % positions.len()];
            step = step.wrapping_add(1);
            state.handle_pointer([black_box(x), 0.0], &style, &mut text_ctx);
            let cursor = state.cursor_index() as u64;
            cursor_sum = cursor_sum.wrapping_add(cursor);
            cursor
        },
    );
    case.metrics.insert(String::from("cursor_pick_positions"), positions.len() as f64);
    case.metrics.insert(String::from("text_bytes"), seed.len() as f64);
    case.metrics.insert(String::from("cursor_checksum"), cursor_sum as f64);
    insert_text_cursor_map_metrics(&mut case, map_stats, "cursor_map");
    case
}

fn text_input_cursor_pick_rtl_cluster_map_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 24 } else { 192 };
    let mut text_ctx = perf_text_ctx();
    let font_id = load_rtl_perf_font(&mut text_ctx);
    let style = ui::elements::TextInputStyle {
        font_id,
        font_px: 16.0,
        ..ui::elements::TextInputStyle::default()
    };
    let seed = "אבגדה וזחטי כלמנס עפצקר שת אבגדה וזחטי כלמנס עפצקר שת ".repeat(24);
    let map_stats = shaped_cursor_map_stats(&text_ctx.fonts, font_id, &seed, style.font_px);
    let mut state = ui::elements::TextInputState::new("Editor");
    state.focus();
    state.set_text(seed.clone());
    let positions = [
        style.padding.left - 8.0,
        style.padding.left + 24.0,
        style.padding.left + 96.0,
        style.padding.left + 188.0,
        style.padding.left + 320.0,
        style.padding.left + 520.0,
    ];
    let mut cursor_sum = 0u64;
    let mut step = 0usize;
    let mut case = measure_cpu_case(
        "cpu.text_input.cursor_pick.rtl_cluster_map",
        "text-input",
        smoke,
        true,
        0.16,
        loops,
        vec![String::from(
            "Hot pointer-to-cursor mapping over a long pure RTL text-input line using a descending shaped cluster map.",
        )],
        || {
            let x = positions[step % positions.len()];
            step = step.wrapping_add(1);
            state.handle_pointer([black_box(x), 0.0], &style, &mut text_ctx);
            let cursor = state.cursor_index() as u64;
            cursor_sum = cursor_sum.wrapping_add(cursor);
            cursor
        },
    );
    case.metrics.insert(String::from("cursor_pick_positions"), positions.len() as f64);
    case.metrics.insert(String::from("text_bytes"), seed.len() as f64);
    case.metrics.insert(String::from("rtl_cursor_checksum"), cursor_sum as f64);
    insert_text_cursor_map_metrics(&mut case, map_stats, "rtl_cursor_map");
    case
}

fn text_input_cursor_pick_fallback_cluster_map_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 24 } else { 192 };
    let mut text_ctx = perf_text_ctx();
    text_ctx.set_fallback_fonts(&[1]);
    let style = ui::elements::TextInputStyle {
        font_id: 0,
        font_px: 16.0,
        ..ui::elements::TextInputStyle::default()
    };
    let seed = "ABÉ 漢 AB漢 ÉB漢 ".repeat(96);
    let (map_stats, fallback_runs) =
        fallback_cursor_map_stats(&text_ctx.fonts, 0, &[1], &seed, style.font_px);
    let mut state = ui::elements::TextInputState::new("Editor");
    state.focus();
    state.set_text(seed.clone());
    let positions = [
        style.padding.left,
        style.padding.left + 18.0,
        style.padding.left + 48.0,
        style.padding.left + 96.0,
        style.padding.left + 188.0,
        style.padding.left + 320.0,
    ];
    let mut cursor_sum = 0u64;
    let mut step = 0usize;
    let mut case = measure_cpu_case(
        "cpu.text_input.cursor_pick.fallback_cluster_map",
        "text-input",
        smoke,
        true,
        0.16,
        loops,
        vec![String::from(
            "Hot pointer-to-cursor mapping over mixed Latin/CJK text using configured fallback-font shaped cursor widths.",
        )],
        || {
            let x = positions[step % positions.len()];
            step = step.wrapping_add(1);
            state.handle_pointer([black_box(x), 0.0], &style, &mut text_ctx);
            let cursor = state.cursor_index() as u64;
            cursor_sum = cursor_sum.wrapping_add(cursor);
            cursor
        },
    );
    case.metrics.insert(String::from("cursor_pick_positions"), positions.len() as f64);
    case.metrics.insert(String::from("fallback_fonts"), 1.0);
    case.metrics.insert(String::from("fallback_shape_runs"), fallback_runs as f64);
    case.metrics.insert(String::from("text_bytes"), seed.len() as f64);
    case.metrics.insert(String::from("fallback_cursor_checksum"), cursor_sum as f64);
    insert_text_cursor_map_metrics(&mut case, map_stats, "fallback_cursor_map");
    case
}

fn text_input_cursor_pick_mixed_bidi_affinity_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 24 } else { 192 };
    let mut text_ctx = perf_text_ctx();
    let font_id = load_rtl_perf_font(&mut text_ctx);
    let style = ui::elements::TextInputStyle {
        font_id,
        font_px: 16.0,
        ..ui::elements::TextInputStyle::default()
    };
    let sample = if font_id == 0 { "AABB" } else { "AאבB" };
    let seed = format!("{} ", sample).repeat(192);
    let mut shaper = text::TextShaper::default();
    let map = text_ctx
        .fonts
        .font(font_id)
        .and_then(|font| shaper.shape(font, font_id, sample, style.font_px).ok())
        .map(|shape| shape.cursor_map_for_text(sample));
    let positions = if let Some(map) = map.as_ref() {
        [
            style.padding.left + map.width_at_with_affinity(1, text::CaretAffinity::Downstream),
            style.padding.left + map.width_at(2),
            style.padding.left + map.width_at_with_affinity(3, text::CaretAffinity::Upstream),
            style.padding.left + map.width_at_with_affinity(3, text::CaretAffinity::Downstream),
            style.padding.left + map.width_at(sample.chars().count()) + 24.0,
            style.padding.left + 180.0,
        ]
    } else {
        [
            style.padding.left,
            style.padding.left + 24.0,
            style.padding.left + 48.0,
            style.padding.left + 72.0,
            style.padding.left + 96.0,
            style.padding.left + 180.0,
        ]
    };
    let map_stats = map.as_ref().map(text_cursor_map_stats).unwrap_or_default();
    let mut state = ui::elements::TextInputState::new("Editor");
    state.focus();
    state.set_text(seed.clone());
    let mut cursor_sum = 0u64;
    let mut step = 0usize;
    let mut case = measure_cpu_case(
        "cpu.text_input.cursor_pick.mixed_bidi_affinity",
        "text-input",
        smoke,
        true,
        0.16,
        loops,
        vec![String::from(
            "Hot pointer-to-cursor mapping over mixed LTR/RTL text with affinity-aware boundary positions.",
        )],
        || {
            let x = positions[step % positions.len()];
            step = step.wrapping_add(1);
            state.handle_pointer([black_box(x), 0.0], &style, &mut text_ctx);
            let cursor = state.cursor_index() as u64;
            cursor_sum = cursor_sum.wrapping_add(cursor);
            cursor
        },
    );
    case.metrics.insert(String::from("cursor_pick_positions"), positions.len() as f64);
    case.metrics.insert(String::from("mixed_bidi_boundary_positions"), 2.0);
    case.metrics.insert(String::from("rtl_font_loaded"), if font_id == 0 { 0.0 } else { 1.0 });
    case.metrics.insert(String::from("text_bytes"), seed.len() as f64);
    case.metrics.insert(String::from("mixed_bidi_cursor_checksum"), cursor_sum as f64);
    insert_text_cursor_map_metrics(&mut case, map_stats, "mixed_bidi_cursor_map");
    case
}

fn image_pipeline_iterations(smoke: bool) -> u64 {
    if smoke {
        4
    } else {
        12
    }
}

fn checker_png_bytes(w: u32, h: u32) -> Option<Vec<u8>> {
    let rgba = gen_checker_rgba(w, h);
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(&rgba).ok()?;
    drop(writer);
    Some(bytes)
}

fn decode_png_rgba(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let src = &buf[..info.buffer_size()];
    match info.color_type {
        png::ColorType::Rgba => Some((info.width, info.height, src.to_vec())),
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity((info.width as usize) * (info.height as usize) * 4);
            for chunk in src.chunks_exact(3) {
                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            Some((info.width, info.height, rgba))
        }
        _ => None,
    }
}

fn image_pipeline_png_decode_case(smoke: bool) -> Result<PerfCaseResult> {
    let loops = image_pipeline_iterations(smoke);
    let png_bytes =
        checker_png_bytes(128, 128).with_context(|| "encoding generated checker PNG payload")?;
    let png_len = png_bytes.len() as f64;
    let (_, _, rgba) =
        decode_png_rgba(&png_bytes).with_context(|| "decoding generated checker PNG payload")?;
    let mut case = measure_cpu_case(
        "cpu.image_pipeline.png.decode",
        "image-pipeline",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from(
            "Dedicated PNG decode phase over the shared checker payload before any renderer upload work.",
        )],
        move || {
            let Some((width, height, decoded)) = decode_png_rgba(&png_bytes) else { return 0 };
            width as u64 + height as u64 + decoded.len() as u64
        },
    );
    case.metrics.insert(String::from("encoded_bytes"), png_len);
    case.metrics.insert(String::from("texture_bytes"), rgba.len() as f64);
    Ok(case)
}

fn image_pipeline_png_upload_case(smoke: bool) -> Result<PerfCaseResult> {
    let png_bytes =
        checker_png_bytes(128, 128).with_context(|| "encoding generated checker PNG payload")?;
    let (w, h, rgba) =
        decode_png_rgba(&png_bytes).with_context(|| "decoding generated checker PNG payload")?;
    let mut renderer =
        Box::new(metal::MetalRenderer::new_default().context("creating Metal renderer")?);
    renderer.resize(512, 512, 2.0).context("resizing Metal renderer")?;

    let warmups = if smoke { 2 } else { 4 };
    let sample_count = if smoke { 6 } else { 10 };
    let ops_per_sample = image_pipeline_iterations(smoke);
    let target_sample_us = if smoke { 300.0 } else { 2_500.0 };

    for _ in 0..warmups {
        for _ in 0..ops_per_sample {
            let handle = renderer.image_create_rgba8(w, h, &rgba, (w as usize) * 4);
            renderer.image_release(handle);
        }
    }

    let mut samples = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        let t0 = Instant::now();
        let mut total_ops = 0u64;
        loop {
            for _ in 0..ops_per_sample {
                let handle = renderer.image_create_rgba8(w, h, &rgba, (w as usize) * 4);
                renderer.image_release(handle);
            }
            total_ops = total_ops.saturating_add(ops_per_sample);
            if t0.elapsed().as_secs_f64() * 1_000_000.0 >= target_sample_us {
                break;
            }
        }
        samples.push(t0.elapsed().as_secs_f64() * 1_000_000.0 / total_ops.max(1) as f64);
    }

    let summary = summarize(&samples);
    let (layer, scenario, variant, cache_state, refresh_mode) =
        perf_case_contract_metadata("gpu.image_pipeline.png.upload", "image-pipeline");
    let mut metrics = BTreeMap::new();
    metrics.insert(String::from("encoded_bytes"), png_bytes.len() as f64);
    metrics.insert(String::from("texture_bytes"), rgba.len() as f64);

    Ok(PerfCaseResult {
        id: String::from("gpu.image_pipeline.png.upload"),
        family: String::from("image-pipeline"),
        layer: String::from(layer),
        scenario: String::from(scenario),
        variant: String::from(variant),
        cache_state: String::from(cache_state),
        refresh_mode: String::from(refresh_mode),
        unit: String::from("us/upload"),
        gated: true,
        threshold_pct: 0.20,
        median: summary.median,
        p95: summary.p95,
        p99: summary.p99,
        min: summary.min,
        max: summary.max,
        mean: summary.mean,
        samples: samples.len(),
        ops_per_sample,
        notes: vec![String::from(
            "Dedicated Metal texture-upload phase over the same decoded checker payload used by the decode and first-visible image cases.",
        )],
        metrics,
    })
}

fn image_pipeline_png_first_visible_case(smoke: bool) -> Result<PerfCaseResult> {
    let png_bytes =
        checker_png_bytes(128, 128).with_context(|| "encoding generated checker PNG payload")?;
    let (w, h, rgba) =
        decode_png_rgba(&png_bytes).with_context(|| "decoding generated checker PNG payload")?;
    let mut renderer =
        Box::new(metal::MetalRenderer::new_default().context("creating Metal renderer")?);
    renderer.resize(640, 480, 2.0).context("resizing Metal renderer")?;
    let rect = api::RectF::new(40.0, 32.0, 240.0, 180.0);

    let warmups = if smoke { 1 } else { 2 };
    let sample_count = if smoke { 6 } else { 10 };
    for _ in 0..warmups {
        let handle = renderer.image_create_rgba8(w, h, &rgba, (w as usize) * 4);
        let mut builder = ui::DrawListBuilder::new();
        let image = ui::elements::ImageView {
            image: handle,
            natural_w: w,
            natural_h: h,
            fit: ui::elements::ImageFit::Contain,
            alpha: 1.0,
        };
        image.encode(rect, None, &mut builder);
        let token =
            renderer.begin_frame(&api::FrameTarget, Some(&api::Damage { rects: Vec::new() }));
        renderer.encode_pass(builder.drawlist());
        renderer.submit(token).context("submitting image first-visible warmup frame")?;
        renderer.image_release(handle);
    }

    let mut frame_samples = Vec::with_capacity(sample_count);
    let mut encode_samples = Vec::with_capacity(sample_count);
    let mut gpu_samples = Vec::with_capacity(sample_count);
    let mut draws_sum = 0.0f64;
    for _ in 0..sample_count {
        let handle = renderer.image_create_rgba8(w, h, &rgba, (w as usize) * 4);
        let mut builder = ui::DrawListBuilder::new();
        let image = ui::elements::ImageView {
            image: handle,
            natural_w: w,
            natural_h: h,
            fit: ui::elements::ImageFit::Contain,
            alpha: 1.0,
        };
        image.encode(rect, None, &mut builder);
        let frame_t0 = Instant::now();
        let token =
            renderer.begin_frame(&api::FrameTarget, Some(&api::Damage { rects: Vec::new() }));
        let frame_id = token.0;
        renderer.encode_pass(builder.drawlist());
        renderer.submit(token).context("submitting image first-visible frame")?;
        let frame_ms = frame_t0.elapsed().as_secs_f64() * 1000.0;
        let stats = last_metal_stats_after_submit(&renderer, frame_id);
        renderer.image_release(handle);
        frame_samples.push(frame_ms);
        encode_samples.push(stats.encode_ms);
        gpu_samples.push(stats.gpu_ms);
        draws_sum += stats.draws as f64;
    }

    let summary = summarize(&frame_samples);
    let (layer, scenario, variant, cache_state, refresh_mode) =
        perf_case_contract_metadata("gpu.image_pipeline.png.first_visible", "image-pipeline");
    let mut metrics = BTreeMap::new();
    metrics.insert(String::from("encoded_bytes"), png_bytes.len() as f64);
    metrics.insert(String::from("texture_bytes"), rgba.len() as f64);
    metrics.insert(String::from("draw_calls"), draws_sum / frame_samples.len().max(1) as f64);
    metrics.insert(String::from("encode_ms_median"), summarize(&encode_samples).median);
    insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
    insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
    insert_frame_pacing_metrics(&mut metrics, &frame_samples);

    Ok(PerfCaseResult {
        id: String::from("gpu.image_pipeline.png.first_visible"),
        family: String::from("image-pipeline"),
        layer: String::from(layer),
        scenario: String::from(scenario),
        variant: String::from(variant),
        cache_state: String::from(cache_state),
        refresh_mode: String::from(refresh_mode),
        unit: String::from("ms/frame"),
        gated: true,
        threshold_pct: 0.20,
        median: summary.median,
        p95: summary.p95,
        p99: summary.p99,
        min: summary.min,
        max: summary.max,
        mean: summary.mean,
        samples: frame_samples.len(),
        ops_per_sample: 1,
        notes: vec![String::from(
            "Dedicated first-visible image phase: upload the shared PNG payload, encode one retained image, and submit its first Metal frame.",
        )],
        metrics,
    })
}

fn navigation_button_press_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 24 } else { 64 };
    let mut builder = ui::DrawListBuilder::new();
    let mut txt = perf_text_ctx();
    let mut uploader = CpuUploader::default();
    let button =
        ui::elements::Button { text: String::from("Tap"), ..ui::elements::Button::default() };
    let mut state = ui::elements::ButtonState::default();
    let rect = api::RectF::new(0.0, 0.0, 140.0, 40.0);
    let mut phase = 0usize;
    let mut case = measure_cpu_case(
        "cpu.navigation.button_press.response",
        "navigation",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Direct input-latency slice for a pressed button transitioning to its first visible response.",
        )],
        move || {
            phase = phase.wrapping_add(1);
            if phase % 2 == 0 {
                state.on_pointer_down();
            } else {
                let _ = state.on_pointer_up();
            }
            builder.clear();
            button.encode(rect, 2.0, &mut txt, &mut uploader, &state, &mut builder);
            let dl = builder.drawlist();
            (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
        },
    );
    case.metrics.insert(String::from("dirty_nodes"), 1.0);
    case
}

fn navigation_slider_scrub_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 24 } else { 64 };
    let mut builder = ui::DrawListBuilder::new();
    let slider = ui::elements::Slider::default();
    let mut state = ui::elements::SliderState::default();
    let rect = api::RectF::new(0.0, 0.0, 240.0, 16.0);
    let mut step = 0usize;
    let mut case = measure_cpu_case(
        "cpu.navigation.slider_scrub.response",
        "navigation",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Direct input-latency slice for a slider scrub updating thumb position and fill immediately.",
        )],
        move || {
            step = step.wrapping_add(1);
            state.value = ((step % 11) as f32) / 10.0;
            builder.clear();
            slider.encode(rect, &state, &mut builder);
            builder.drawlist().items.len() as u64
        },
    );
    case.metrics.insert(String::from("dirty_nodes"), 1.0);
    case
}

fn navigation_text_focus_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 12 } else { 32 };
    let mut case = measure_cpu_case(
        "cpu.navigation.text_focus.response",
        "navigation",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Direct input-latency slice for focusing the Input & Haptics editor and presenting its next visible frame.",
        )],
        move || {
            let mut router = prepare_cpu_router();
            router.set_scene(6);
            let mut builder = ui::DrawListBuilder::new();
            let vp = perf_viewport();
            let mut now = timing::now_ms();
            let bootstrap = advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now);
            router.input_pointer(72.0, 108.0, 0.0, 0.0, 1);
            router.input_pointer(72.0, 108.0, 0.0, 0.0, 0);
            bootstrap + advance_cpu_router_frame(&mut router, &mut builder, vp, &mut now)
        },
    );
    case.metrics.insert(String::from("dirty_nodes"), 1.0);
    case
}

fn reconcile_single_node_case(smoke: bool) -> Result<PerfCaseResult> {
    reconcile_tree_mutation_case(smoke, 1)
}

fn reconcile_tree_mutation_case(smoke: bool, dirty_nodes: usize) -> Result<PerfCaseResult> {
    let loops = if smoke { 8 } else { 24 };
    let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(420.0));
    let nodes = populate_flat_rect_surface(&mut surface, 1_000, 0);
    surface.layout(420.0, 760.0);
    let mut builder = ui::DrawListBuilder::new();
    let mut phase = 0usize;
    let id = match dirty_nodes {
        1 => "cpu.reconcile.single_node_mutation",
        10 => "cpu.reconcile.tree_mutation_1pct",
        100 => "cpu.reconcile.tree_mutation_10pct",
        _ => bail!("unsupported reconcile dirty-node count `{}`", dirty_nodes),
    };
    let label = match dirty_nodes {
        1 => "single-node mutation",
        10 => "1 percent tree mutation",
        100 => "10 percent tree mutation",
        _ => unreachable!(),
    };
    let mut case = measure_cpu_case(
        id,
        "reconcile",
        smoke,
        true,
        0.18,
        loops,
        vec![format!("Diff/apply slice for a {} over a retained 1000-node flat-rect tree.", label)],
        move || {
            phase = phase.wrapping_add(1);
            for index in 0..dirty_nodes {
                let node = nodes.cells[index];
                let Some(style) = surface.tree_mut().style_mut(node) else { continue };
                style.background = flat_rect_fill_color(index, phase);
                style.opacity = 0.72 + ((index + phase) % 5) as f32 * 0.05;
            }
            builder.clear();
            surface.encode(&mut builder);
            let dl = builder.drawlist();
            (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
        },
    );
    case.metrics.insert(String::from("dirty_nodes"), dirty_nodes as f64);
    case.metrics.insert(String::from("layout_passes"), 1.0);
    Ok(case)
}

fn reconcile_theme_swap_case(smoke: bool) -> Result<PerfCaseResult> {
    let loops = if smoke { 8 } else { 24 };
    let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(420.0));
    let root = surface.root();
    let nodes = populate_flat_rect_surface(&mut surface, 1_000, 0);
    let node_count = nodes.cells.len();
    surface.layout(420.0, 760.0);
    let mut builder = ui::DrawListBuilder::new();
    let mut phase = 0usize;
    let mut case = measure_cpu_case(
        "cpu.reconcile.theme_swap_full",
        "reconcile",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Full retained-tree theme swap touching every node plus root spacing to expose diff/apply and relayout cost directly.",
        )],
        move || {
            phase = phase.wrapping_add(1);
            if let Some(style) = surface.tree_mut().style_mut(root) {
                style.padding = if phase % 2 == 0 {
                    ui::Edges { left: 16.0, top: 16.0, right: 16.0, bottom: 16.0 }
                } else {
                    ui::Edges { left: 8.0, top: 8.0, right: 8.0, bottom: 8.0 }
                };
                style.gap = if phase % 2 == 0 { 8.0 } else { 6.0 };
            }
            for (index, node) in nodes.cells.iter().copied().enumerate() {
                let Some(style) = surface.tree_mut().style_mut(node) else { continue };
                style.background = flat_rect_fill_color(index, phase);
                style.opacity = 0.68 + ((index + phase) % 4) as f32 * 0.06;
            }
            builder.clear();
            surface.layout(420.0, 760.0);
            surface.encode(&mut builder);
            let dl = builder.drawlist();
            (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
        },
    );
    case.metrics.insert(String::from("dirty_nodes"), node_count as f64);
    case.metrics.insert(String::from("layout_passes"), 2.0);
    Ok(case)
}

fn collection_flow_case<M, R>(
    id: &str,
    name: &str,
    smoke: bool,
    count: usize,
    mode: ui::collection::CollectionMode,
    measure: M,
    mut render: R,
) -> Result<PerfCaseResult>
where
    M: ui::collection::Measure,
    R: ui::collection::CellRenderer,
{
    let measure_calls = Arc::new(AtomicU64::new(0));
    let revision_queries = Arc::new(AtomicU64::new(0));
    let measure_calls_for_run = Arc::clone(&measure_calls);
    let revision_queries_for_run = Arc::clone(&revision_queries);
    let has_collection_revision = measure.collection_revision().is_some();
    let mut measure = CountingCollectionMeasure {
        inner: measure,
        calls: measure_calls_for_run,
        revision_queries: revision_queries_for_run,
    };
    let mut case = measure_journey_case(
        id,
        smoke,
        0.18,
        vec![format!(
            "{} using collection virtualization across slow scroll, medium scroll, hard fling, reverse, and focus movement.",
            name
        )],
        move || {
            let viewport = api::RectF::new(0.0, 0.0, 360.0, 640.0);
            let mut builder = ui::DrawListBuilder::new();
            let mut collection = ui::collection::CollectionView::new(mode.clone());
            collection.set_count(count);
            collection.focus_set(Some(0));

            let mut work = 0u64;
            for (index, scroll) in [0.0, 180.0, 620.0, 1_420.0, 940.0, 220.0].iter().enumerate() {
                builder.clear();
                collection.set_scroll(*scroll);
                if index % 2 == 0 {
                    collection.focus_move_down();
                    collection.focus_move_right();
                } else {
                    collection.focus_move_up();
                    collection.focus_move_left();
                }
                let metrics =
                    collection.layout_and_render(viewport, &mut measure, &mut render, &mut builder);
                work = work
                    .saturating_add(builder.drawlist().items.len() as u64)
                    .saturating_add(metrics.content_h.round().max(0.0) as u64)
                    .saturating_add(collection.focus().unwrap_or_default() as u64);
            }
            work
        },
    )?;
    case.metrics.insert(
        String::from("collection_measure_calls_total"),
        measure_calls.load(Ordering::Relaxed) as f64,
    );
    case.metrics.insert(
        String::from("collection_item_revision_queries_total"),
        revision_queries.load(Ordering::Relaxed) as f64,
    );
    case.metrics.insert(
        String::from("collection_revision_hint"),
        if has_collection_revision { 1.0 } else { 0.0 },
    );
    Ok(case)
}

fn journey_feed_scroll_case(smoke: bool) -> Result<PerfCaseResult> {
    collection_flow_case(
        "cpu.journey.feed_scroll_matrix",
        "Feed scroll matrix",
        smoke,
        1_000,
        ui::collection::CollectionMode::VerticalGrid { col_width: 320.0, spacing: 14.0 },
        FeedMeasure,
        FeedRender,
    )
}

fn journey_thumbnail_grid_scroll_case(smoke: bool) -> Result<PerfCaseResult> {
    collection_flow_case(
        "cpu.journey.thumbnail_grid_scroll_matrix",
        "Thumbnail grid scroll matrix",
        smoke,
        3_000,
        ui::collection::CollectionMode::VerticalGrid { col_width: 104.0, spacing: 8.0 },
        GridMeasure,
        GridRender,
    )
}

fn journey_chat_thread_scroll_case(smoke: bool) -> Result<PerfCaseResult> {
    collection_flow_case(
        "cpu.journey.chat_thread_scroll_matrix",
        "Chat thread scroll matrix",
        smoke,
        2_000,
        ui::collection::CollectionMode::VerticalGrid { col_width: 320.0, spacing: 10.0 },
        ChatMeasure,
        ChatRender,
    )
}

fn endurance_open_close_case(smoke: bool) -> PerfCaseResult {
    let loops = endurance_iterations(smoke);
    let mut case = measure_cpu_case(
        "cpu.endurance.open_close_heavy_screen.100x",
        "endurance",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Repeated heavy-screen open/close loop across Oxide router scenes to trap retained-state ratchets.",
        )],
        move || {
            let mut router = prepare_cpu_router();
            let mut builder = ui::DrawListBuilder::new();
            let viewport = perf_viewport();
            let mut now = timing::now_ms();
            let mut work = 0u64;
            for index in 0..100 {
                router.set_scene(if index % 2 == 0 { 15 } else { 13 });
                work = work.saturating_add(advance_cpu_router_frame(
                    &mut router,
                    &mut builder,
                    viewport,
                    &mut now,
                ));
            }
            work
        },
    );
    case.metrics.insert(String::from("layout_passes"), 100.0);
    case
}

fn endurance_tab_switch_case(smoke: bool) -> PerfCaseResult {
    let loops = endurance_iterations(smoke);
    let mut case = measure_cpu_case(
        "cpu.endurance.tab_switch_heavy.500x",
        "endurance",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Heavy tab-switch loop across three representative scenes to catch drift in repeated scene swaps.",
        )],
        move || {
            let mut router = prepare_cpu_router();
            let mut builder = ui::DrawListBuilder::new();
            let viewport = perf_viewport();
            let mut now = timing::now_ms();
            let mut work = 0u64;
            for index in 0..500 {
                router.set_scene(match index % 3 {
                    0 => 4,
                    1 => 13,
                    _ => 15,
                });
                work = work.saturating_add(advance_cpu_router_frame(
                    &mut router,
                    &mut builder,
                    viewport,
                    &mut now,
                ));
            }
            work
        },
    );
    case.metrics.insert(String::from("layout_passes"), 500.0);
    case
}

fn endurance_idle_animation_case(smoke: bool) -> PerfCaseResult {
    let loops = endurance_iterations(smoke);
    let mut case = measure_cpu_case(
        "cpu.endurance.idle_animation.600_frames",
        "endurance",
        smoke,
        true,
        0.18,
        loops,
        vec![String::from(
            "Long-running animation tick loop for drift detection across a 600-frame idle window.",
        )],
        move || {
            let mut router = prepare_cpu_router();
            router.set_scene(3);
            let mut builder = ui::DrawListBuilder::new();
            let viewport = perf_viewport();
            let mut now = timing::now_ms();
            let mut work = 0u64;
            for _ in 0..600 {
                work = work.saturating_add(advance_cpu_router_frame(
                    &mut router,
                    &mut builder,
                    viewport,
                    &mut now,
                ));
            }
            work
        },
    );
    case.metrics.insert(String::from("layout_passes"), 600.0);
    case
}

fn stress_flat_rects_mount_case(smoke: bool) -> Result<PerfCaseResult> {
    let loops = stress_iterations(smoke);
    let count = 10_000usize;
    let count_f = count as f32;
    let mut case = measure_cpu_case(
        "cpu.stress.flat_rects.10000.mount",
        "stress",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from("Pathological retained-tree mount of ten thousand flat rectangles.")],
        move || {
            let mut builder = ui::DrawListBuilder::new();
            let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(420.0));
            populate_flat_rect_surface(&mut surface, count, 0);
            surface.layout(420.0, (count_f / 10.0).ceil() * 28.0);
            surface.encode(&mut builder);
            let dl = builder.drawlist();
            (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
        },
    );
    case.metrics.insert(String::from("dirty_nodes"), 10_000.0);
    case.metrics.insert(String::from("layout_passes"), 1.0);
    Ok(case)
}

fn stress_simultaneous_animations_case(smoke: bool) -> PerfCaseResult {
    let loops = stress_iterations(smoke);
    let mut case = measure_cpu_case(
        "cpu.stress.simultaneous_animations.300",
        "stress",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from(
            "Three hundred simultaneous animated bars sharing one encode pass to trap bulk animation regressions.",
        )],
        move || {
            let mut builder = ui::DrawListBuilder::new();
            builder.clear();
            for index in 0..300 {
                let phase = (index % 23) as f32 / 23.0;
                let x = (index % 20) as f32 * 18.0;
                let y = (index / 20) as f32 * 22.0;
                let h = 10.0 + phase * 38.0;
                builder.rrect(
                    api::RectF::new(x, y + (48.0 - h), 12.0, h),
                    [6.0; 4],
                    flat_rect_fill_color(index, index / 7),
                );
            }
            let dl = builder.drawlist();
            (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
        },
    );
    case.metrics.insert(String::from("draw_calls"), 300.0);
    case
}

fn stress_ticker_case(smoke: bool) -> PerfCaseResult {
    let loops = stress_iterations(smoke);
    let mut case = measure_cpu_case(
        "cpu.stress.ticker_100hz",
        "stress",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from(
            "Hundred-hertz ticker update loop on the stress scene to trap high-frequency mutation regressions.",
        )],
        move || {
            let mut router = prepare_cpu_router();
            router.set_scene(16);
            let mut builder = ui::DrawListBuilder::new();
            let viewport = perf_viewport();
            let mut now = timing::now_ms();
            let mut work = 0u64;
            for _ in 0..100 {
                work = work.saturating_add(advance_cpu_router_frame(
                    &mut router,
                    &mut builder,
                    viewport,
                    &mut now,
                ));
            }
            work
        },
    );
    case.metrics.insert(String::from("layout_passes"), 100.0);
    case
}

type PermissionCallback =
    Box<dyn Fn(platform::PermissionDomain, platform::PermissionStatus) + Send>;

struct PerfPermissionsStub {
    statuses: Mutex<BTreeMap<platform::PermissionDomain, platform::PermissionStatus>>,
    subscribers: Mutex<Vec<PermissionCallback>>,
}

impl PerfPermissionsStub {
    fn new(domain: platform::PermissionDomain, status: platform::PermissionStatus) -> Self {
        let mut statuses = BTreeMap::new();
        statuses.insert(domain, status);
        Self { statuses: Mutex::new(statuses), subscribers: Mutex::new(Vec::new()) }
    }

    fn notify(&self, domain: platform::PermissionDomain, status: platform::PermissionStatus) {
        lock_unpoison(&self.statuses).insert(domain, status);
        for callback in lock_unpoison(&self.subscribers).iter() {
            callback(domain, status);
        }
    }
}

impl platform::Permissions for PerfPermissionsStub {
    fn request(&self, _domain: platform::PermissionDomain) {}

    fn status(&self, domain: platform::PermissionDomain) -> platform::PermissionStatus {
        lock_unpoison(&self.statuses)
            .get(&domain)
            .copied()
            .unwrap_or(platform::PermissionStatus::NotDetermined)
    }

    fn subscribe(
        &self,
        f: Box<dyn Fn(platform::PermissionDomain, platform::PermissionStatus) + Send>,
    ) {
        lock_unpoison(&self.subscribers).push(f);
    }
}

fn lock_unpoison<'a, T>(mutex: &'a Mutex<T>) -> MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn bridge_permission_callback_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 32 } else { 128 };
    let permission_domain = platform::PermissionDomain::Camera;
    let permissions = Arc::new(PerfPermissionsStub::new(
        permission_domain,
        platform::PermissionStatus::Authorized,
    ));
    let manager = permissions::PermissionManager::new(permissions.clone(), Arc::new(|| 77));
    let callback_sum = Arc::new(AtomicU64::new(0));
    let mut subscriptions = Vec::new();

    for offset in [1u64, 3, 5] {
        let callback_sum = Arc::clone(&callback_sum);
        let subscription = manager.subscribe(permission_domain, move |state| {
            callback_sum.fetch_add(state.status as u64 + offset, Ordering::Relaxed);
        });
        subscriptions.push(subscription);
    }
    black_box(subscriptions.len());

    let mut toggle = false;
    measure_cpu_case(
        "cpu.bridge.permission_callback_fanout",
        "bridge",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from(
            "App-owned permission wrapper overhead: cached status update plus three callback listeners.",
        )],
        move || {
            toggle = !toggle;
            let status = if toggle {
                platform::PermissionStatus::Authorized
            } else {
                platform::PermissionStatus::Limited
            };
            permissions.notify(permission_domain, status);
            callback_sum.load(Ordering::Relaxed) + manager.status(permission_domain) as u64
        },
    )
}

fn bridge_web_backend_surface_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 32 } else { 128 };
    let web_platform = platform_web::WebPlatform::new();
    let mut tick = 0u64;
    measure_cpu_case(
        "cpu.bridge.web_backend_surface",
        "bridge",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from(
            "Web backend bridge surface on the native fallback path: capabilities, device caps, clipboard cache, network status, permission callbacks, location fallback, haptics, and iframe WebView unsupported boundary.",
        )],
        move || {
            tick = tick.wrapping_add(1);
            web_platform.clipboard_set("oxide-web-backend");
            web_platform.network_status().subscribe(Box::new(|_| {}));
            web_platform.permissions().subscribe(Box::new(|_, _| {}));
            web_platform.permissions().request(platform::PermissionDomain::Location);
            web_platform.haptics().play(platform::HapticPattern::Selection);
            web_platform.location().request_once();

            let caps = web_platform.capabilities().bits();
            let device = web_platform.device_caps();
            let network = web_platform.network_status().current_status();
            let permission = web_platform.permissions().status(platform::PermissionDomain::Location) as u64;
            let clipboard_len = web_platform
                .clipboard_get()
                .map(|value| value.len() as u64)
                .unwrap_or_default();
            let location_start = match web_platform.location().start(platform::LocationOptions::default()) {
                Ok(()) => 1,
                Err(platform::PlatformError::Unsupported(_)) => 2,
                Err(_) => 3,
            };
            let web_view_status = match web_platform.web_view_service().create_view("about:blank", Box::new(|_| {})) {
                Ok(view) => {
                    view.close();
                    1
                }
                Err(platform::PlatformError::Unsupported(_)) => 2,
                Err(_) => 3,
            };

            tick
                .wrapping_add(caps)
                .wrapping_add(device.max_framerate_hz as u64)
                .wrapping_add(device.native_scale.to_bits() as u64)
                .wrapping_add(u64::from(network.is_connected))
                .wrapping_add(permission)
                .wrapping_add(clipboard_len)
                .wrapping_add(web_platform.location().history().len() as u64)
                .wrapping_add(location_start)
                .wrapping_add(web_view_status)
        },
    )
}

fn bridge_sensor_location_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 32 } else { 96 };
    let bridge = permissions::SensorBridge::with_config(permissions::sensors::SensorBridgeConfig {
        location_history_max: 12,
        location_max_age_ms: 30_000,
        motion_history_max: 8,
        bluetooth_max_age_ms: 60_000,
        push_history_max: 4,
        bluetooth_cache_max: 16,
    });
    bridge.update_permission(permissions::PermissionState::new(
        platform::PermissionDomain::Location,
        platform::PermissionStatus::Authorized,
        0,
    ));
    let mut tick = 0u64;
    measure_cpu_case(
        "cpu.bridge.sensor_location_snapshot",
        "bridge",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from(
            "Location bridge overhead: permission-gated sensor ingest plus snapshot materialization.",
        )],
        move || {
            tick = tick.saturating_add(17);
            bridge.handle_location_event(platform::LocationEvent::Update(platform::LocationReading {
                latitude_deg: 37.7749 + tick as f64 * 0.000001,
                longitude_deg: -122.4194 - tick as f64 * 0.000001,
                altitude_m: 14.0,
                horizontal_accuracy_m: 4.0,
                vertical_accuracy_m: 7.0,
                speed_mps: 1.8,
                course_deg: 92.0,
                timestamp_ms: tick,
            }));
            let snapshot = bridge.snapshot();
            snapshot.location.history.len() as u64
                + snapshot.location.last.map(|reading| reading.timestamp_ms).unwrap_or_default()
        },
    )
}

fn bridge_bluetooth_cache_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 24 } else { 64 };
    let bridge = permissions::SensorBridge::with_config(permissions::sensors::SensorBridgeConfig {
        location_history_max: 8,
        location_max_age_ms: 15_000,
        motion_history_max: 8,
        bluetooth_max_age_ms: 120_000,
        push_history_max: 4,
        bluetooth_cache_max: 24,
    });
    bridge.update_permission(permissions::PermissionState::new(
        platform::PermissionDomain::Bluetooth,
        platform::PermissionStatus::Authorized,
        0,
    ));
    bridge.handle_bluetooth_event(platform::BluetoothEvent::StateChanged { powered_on: true });
    let service = platform::BleUuid([7u8; 16]);
    let mut discovery_index = 0u64;
    measure_cpu_case(
        "cpu.bridge.bluetooth_cache_update",
        "bridge",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from(
            "Bluetooth bridge overhead: discovery cache update and snapshot emission without renderer work.",
        )],
        move || {
            discovery_index = discovery_index.saturating_add(1);
            let peripheral_id = 10_000 + (discovery_index % 12) as u128;
            bridge.handle_bluetooth_event(platform::BluetoothEvent::Discovered(
                platform::PeripheralInfo {
                    id: peripheral_id,
                    name: Some(format!("Bench {}", peripheral_id)),
                    rssi_dbm: -44,
                    advertisement: platform::AdvertisementData {
                        services: vec![service],
                        manufacturer_data: Some(vec![1, 2, 3, discovery_index as u8]),
                        connectable: true,
                    },
                },
            ));
            let snapshot = bridge.bluetooth_snapshot();
            snapshot.devices.len() as u64 + if snapshot.powered_on { 1 } else { 0 }
        },
    )
}

fn bridge_file_fixture(rows: usize) -> String {
    let mut out = String::new();
    for row in 0..rows {
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!(
                "ITEM-{row:03}|Orbit {}|Priority {}|Owner {}\n",
                row % 9,
                row % 3,
                row % 5
            ),
        );
    }
    out
}

fn bridge_json_fixture(rows: usize) -> String {
    let mut out = String::from("[");
    for row in 0..rows {
        if row > 0 {
            out.push(',');
        }
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!(
                "{{\"title\":\"Feed {row}\",\"accent\":{},\"count\":{}}}",
                row % 6,
                40 + row
            ),
        );
    }
    out.push(']');
    out
}

fn bridge_photo_import_thumbnail_case(smoke: bool) -> Result<PerfCaseResult> {
    let loops = if smoke { 4 } else { 12 };
    let png_bytes =
        checker_png_bytes(128, 128).with_context(|| "encoding bridge photo-import PNG payload")?;
    let png_len = png_bytes.len() as f64;
    let (_w, _h, rgba) =
        decode_png_rgba(&png_bytes).with_context(|| "decoding bridge photo-import PNG payload")?;
    let rects = primitive_image_rects(10);
    let mut case = measure_cpu_case(
        "cpu.bridge.photo_import_thumbnail",
        "bridge",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from(
            "App-owned photo-import bridge path from imported bytes to the first rendered thumbnail strip, excluding the system picker surface.",
        )],
        move || {
            let Some((width, height, _)) = decode_png_rgba(&png_bytes) else { return 0 };
            let mut builder = ui::DrawListBuilder::new();
            for (index, rect) in rects.iter().copied().enumerate() {
                let image = ui::elements::ImageView {
                    image: api::ImageHandle(700 + index as u32),
                    natural_w: width,
                    natural_h: height,
                    fit: ui::elements::ImageFit::Cover,
                    alpha: 1.0,
                };
                image.encode(rect, None, &mut builder);
            }
            let dl = builder.drawlist();
            width as u64
                + height as u64
                + (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
        },
    );
    case.metrics.insert(String::from("encoded_bytes"), png_len);
    case.metrics.insert(String::from("texture_bytes"), rgba.len() as f64);
    Ok(case)
}

fn bridge_file_import_render_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 4 } else { 10 };
    let file_text = bridge_file_fixture(32);
    let mut case = measure_cpu_case(
        "cpu.bridge.file_import_render",
        "bridge",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from(
            "App-owned file-import path from imported text bytes to the first rendered snapshot rows, excluding any system document picker UI.",
        )],
        move || {
            let mut builder = ui::DrawListBuilder::new();
            let mut txt = perf_text_ctx();
            let mut uploader = CpuUploader::default();
            let mut checksum = 0u64;
            for (index, line) in file_text.lines().enumerate() {
                let label = ui::elements::Label {
                    text: line.to_string(),
                    color: api::Color::rgba(0.12, 0.14, 0.18, 1.0),
                    align: ui::elements::Align::Left,
                    wrap: false,
                    font_id: 0,
                    font_px: 14.0,
                };
                let rect = api::RectF::new(0.0, index as f32 * 18.0, 360.0, 16.0);
                label.encode(rect, 2.0, &mut txt, &mut uploader, &mut builder);
                checksum = checksum.wrapping_add(line.len() as u64);
            }
            let dl = builder.drawlist();
            checksum + (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
        },
    );
    case.metrics.insert(String::from("dirty_nodes"), 32.0);
    case
}

fn bridge_share_payload_prepare_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 8 } else { 24 };
    let selected = vec![
        String::from("Orbit telemetry card"),
        String::from("Damage report snapshot"),
        String::from("Field note export"),
    ];
    let mut case = measure_cpu_case(
        "cpu.bridge.share_payload_prepare",
        "bridge",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from(
            "App-owned share bridge path from selected records to a prepared share payload summary, excluding the system share sheet presentation.",
        )],
        move || {
            let mut payload = String::new();
            for (index, item) in selected.iter().enumerate() {
                let _ = std::fmt::Write::write_fmt(
                    &mut payload,
                    format_args!("{}. {}\n", index + 1, item),
                );
            }
            let label = ui::elements::Label {
                text: payload.clone(),
                color: api::Color::rgba(0.16, 0.18, 0.24, 1.0),
                align: ui::elements::Align::Left,
                wrap: true,
                font_id: 0,
                font_px: 16.0,
            };
            let mut builder = ui::DrawListBuilder::new();
            let mut txt = perf_text_ctx();
            let mut uploader = CpuUploader::default();
            label.encode(api::RectF::new(0.0, 0.0, 320.0, 110.0), 2.0, &mut txt, &mut uploader, &mut builder);
            payload.len() as u64 + builder.drawlist().items.len() as u64
        },
    );
    case.metrics.insert(String::from("encoded_bytes"), 74.0);
    case
}

fn bridge_local_json_transport_render_case(smoke: bool) -> PerfCaseResult {
    let loops = if smoke { 4 } else { 10 };
    let json = bridge_json_fixture(48);
    let json_len = json.len() as f64;
    let mut case = measure_cpu_case(
        "cpu.bridge.local_json_transport_render",
        "bridge",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from(
            "App-owned localhost JSON transport path from bytes-ready through decode into the first rendered feed skeleton, excluding network stack and system UI.",
        )],
        move || {
            let Ok(rows) = serde_json::from_str::<Vec<serde_json::Value>>(&json) else { return 0 };
            let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(360.0));
            populate_flat_rect_surface(&mut surface, rows.len(), 1);
            surface.layout(360.0, 720.0);
            let mut builder = ui::DrawListBuilder::new();
            surface.encode(&mut builder);
            let dl = builder.drawlist();
            rows.len() as u64 + (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
        },
    );
    case.metrics.insert(String::from("encoded_bytes"), json_len);
    case.metrics.insert(String::from("dirty_nodes"), 48.0);
    case
}

fn bridge_local_image_transport_render_case(smoke: bool) -> Result<PerfCaseResult> {
    let loops = if smoke { 4 } else { 12 };
    let png_bytes = checker_png_bytes(256, 256)
        .with_context(|| "encoding bridge image-transport PNG payload")?;
    let png_len = png_bytes.len() as f64;
    let (_w, _h, rgba) = decode_png_rgba(&png_bytes)
        .with_context(|| "decoding bridge image-transport PNG payload")?;
    let mut case = measure_cpu_case(
        "cpu.bridge.local_image_transport_render",
        "bridge",
        smoke,
        true,
        0.20,
        loops,
        vec![String::from(
            "App-owned localhost image transport path from bytes-ready through decode into the first rendered hero image, excluding network stack and system UI.",
        )],
        move || {
            let Some((width, height, _)) = decode_png_rgba(&png_bytes) else { return 0 };
            let image = ui::elements::ImageView {
                image: api::ImageHandle(880),
                natural_w: width,
                natural_h: height,
                fit: ui::elements::ImageFit::Contain,
                alpha: 1.0,
            };
            let mut builder = ui::DrawListBuilder::new();
            image.encode(api::RectF::new(12.0, 12.0, 320.0, 240.0), None, &mut builder);
            let dl = builder.drawlist();
            width as u64
                + height as u64
                + (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64
        },
    );
    case.metrics.insert(String::from("encoded_bytes"), png_len);
    case.metrics.insert(String::from("texture_bytes"), rgba.len() as f64);
    Ok(case)
}

fn perf_text_ctx() -> ui::elements::TextCtx {
    let mut txt = ui::elements::TextCtx::default();
    load_perf_fonts(&mut txt);
    txt
}

fn load_perf_fonts(txt: &mut ui::elements::TextCtx) {
    if txt.fonts.font(0).is_none() {
        let _ = txt.fonts.add_font(text::Font::from_bytes(LATIN_FONT.to_vec()));
    }
    if txt.fonts.font(1).is_none() {
        let _ = txt.fonts.add_font(text::Font::from_bytes(CJK_FONT.to_vec()));
    }
}

fn load_rtl_perf_font(txt: &mut ui::elements::TextCtx) -> usize {
    if let Ok(bytes) = fs::read(MACOS_HEBREW_FONT) {
        return txt.fonts.add_font(text::Font::from_bytes(bytes));
    }
    0
}

fn perf_viewport() -> api::RectF {
    api::RectF::new(
        0.0,
        0.0,
        (PERF_SCENE_W as f32) / PERF_DEVICE_SCALE,
        (PERF_SCENE_H as f32) / PERF_DEVICE_SCALE,
    )
}

fn prepare_cpu_router() -> scenes::Router<CpuUploader> {
    let mut router = scenes::Router::new(CpuUploader::default());
    load_perf_fonts(&mut router.text);
    router.set_zoom_image(api::ImageHandle(101), 512, 512);
    router.nine_slice_set_image(api::ImageHandle(102));
    router
}

fn prepare_gpu_router(
    renderer: *mut metal::MetalRenderer,
    image: api::ImageHandle,
) -> scenes::Router<MetalUploader> {
    let mut router = scenes::Router::new(MetalUploader { renderer });
    load_perf_fonts(&mut router.text);
    router.set_zoom_image(image, 512, 512);
    router.nine_slice_set_image(image);
    router
}

fn advance_cpu_router_frame<U: ui::elements::ImageUploader>(
    router: &mut scenes::Router<U>,
    builder: &mut ui::DrawListBuilder,
    viewport: api::RectF,
    now: &mut u64,
) -> u64 {
    builder.clear();
    *now = now.saturating_add(16);
    router.update(*now, 16);
    router.draw(viewport, PERF_DEVICE_SCALE, builder);
    ui::coalesce_adjacent_draws(builder.drawlist_mut());
    let dl = builder.drawlist();
    let damage = router.take_damage().len() as u64;
    (dl.items.len() + dl.vertices.len() + dl.indices.len()) as u64 + damage
}

fn gpu_scene_threshold(spec: &ScenePerfSpec) -> f64 {
    match spec.slug {
        "anim_timeline" | "collection" | "input_lab" | "integration" => 0.45,
        _ => 0.35,
    }
}

fn perf_case_contract_metadata(
    id: &str,
    family: &str,
) -> (&'static str, &'static str, &'static str, &'static str, &'static str) {
    if id.contains(".launch.") || family == "launch" {
        let cache_state = if id.contains(".cold_launch") || id.contains(".deep_link_launch") {
            "cold"
        } else {
            "warm"
        };
        return ("flow", "launch-lifecycle", "oxide", cache_state, "offscreen");
    }
    if id.contains(".scene.")
        || family == "scene-cpu"
        || family == "scene-gpu"
        || family == "journey"
    {
        return ("flow", "screen-flow", "oxide", "warm", "offscreen");
    }
    if id.contains(".bridge.") {
        return ("bridge", "os-bridge", "oxide", "warm", "offscreen");
    }
    if family == "system" {
        return ("engine", "system", "oxide", "warm", "offscreen");
    }
    if family == "component" {
        return ("engine", "component", "oxide", "warm", "offscreen");
    }
    if family == "primitive-lifecycle" {
        return ("engine", "primitive-lifecycle", "oxide", "warm", "offscreen");
    }
    if family == "animation" {
        return ("engine", "animation", "oxide", "warm", "offscreen");
    }
    if family == "authoring" {
        return ("engine", "authoring", "oxide", "warm", "offscreen");
    }
    if family == "layout" {
        return ("engine", "layout-invalidation", "oxide", "warm", "offscreen");
    }
    if family == "text-input" {
        return ("engine", "text-input", "oxide", "warm", "offscreen");
    }
    if family == "image-pipeline" {
        let cache_state = if id.contains(".decode") { "cold" } else { "warm" };
        return ("engine", "image-pipeline", "oxide", cache_state, "offscreen");
    }
    if family == "navigation" {
        return ("flow", "navigation-input", "oxide", "warm", "offscreen");
    }
    if family == "reconcile" {
        return ("engine", "state-reconcile", "oxide", "warm", "offscreen");
    }
    if family == "endurance" {
        return ("flow", "endurance-thermal", "oxide", "warm", "offscreen");
    }
    if family == "stress" {
        return ("engine", "stress-pathological", "oxide", "warm", "offscreen");
    }
    if family == "architecture" {
        return ("engine", "rendering-architecture", "oxide", "warm", "offscreen");
    }
    if family == "audit-baseline" {
        return ("engine", "audit-baseline", "legacy-baseline", "warm", "offscreen");
    }
    ("engine", "uncategorized", "oxide", "warm", "offscreen")
}

fn measure_cpu_case<F>(
    id: &str,
    family: &str,
    smoke: bool,
    gated: bool,
    threshold_pct: f64,
    ops_per_sample: u64,
    notes: Vec<String>,
    mut run_once: F,
) -> PerfCaseResult
where
    F: FnMut() -> u64,
{
    let warmups = if smoke { 2 } else { 4 };
    let sample_count = if smoke { 6 } else { 12 };
    let target_sample_us = if smoke { 1_000.0 } else { 8_000.0 };

    for _ in 0..warmups {
        let mut checksum = 0u64;
        for _ in 0..ops_per_sample {
            checksum = checksum.wrapping_add(black_box(run_once()));
        }
        black_box(checksum);
    }

    let mut samples = Vec::with_capacity(sample_count);
    let mut checksum = 0u64;
    for _ in 0..sample_count {
        let t0 = Instant::now();
        let mut local = 0u64;
        let mut total_ops = 0u64;
        loop {
            for _ in 0..ops_per_sample {
                local = local.wrapping_add(black_box(run_once()));
            }
            total_ops = total_ops.saturating_add(ops_per_sample);
            if t0.elapsed().as_secs_f64() * 1_000_000.0 >= target_sample_us {
                break;
            }
        }
        let elapsed = t0.elapsed().as_secs_f64() * 1_000_000.0 / total_ops.max(1) as f64;
        samples.push(elapsed);
        checksum = checksum.wrapping_add(local);
    }
    black_box(checksum);

    let summary = summarize(&samples);
    let (layer, scenario, variant, cache_state, refresh_mode) =
        perf_case_contract_metadata(id, family);
    PerfCaseResult {
        id: id.to_string(),
        family: family.to_string(),
        layer: String::from(layer),
        scenario: String::from(scenario),
        variant: String::from(variant),
        cache_state: String::from(cache_state),
        refresh_mode: String::from(refresh_mode),
        unit: String::from("us/op"),
        gated,
        threshold_pct,
        median: summary.median,
        p95: summary.p95,
        p99: summary.p99,
        min: summary.min,
        max: summary.max,
        mean: summary.mean,
        samples: samples.len(),
        ops_per_sample,
        notes,
        metrics: BTreeMap::new(),
    }
}

fn measure_journey_case<F>(
    id: &str,
    smoke: bool,
    threshold_pct: f64,
    notes: Vec<String>,
    mut run_once: F,
) -> Result<PerfCaseResult>
where
    F: FnMut() -> u64,
{
    let warmups = if smoke { 1 } else { 2 };
    let sample_count = if smoke { 6 } else { 10 };

    for _ in 0..warmups {
        black_box(run_once());
    }

    let mut samples = Vec::with_capacity(sample_count);
    let mut checksum = 0u64;
    for _ in 0..sample_count {
        let t0 = Instant::now();
        checksum = checksum.wrapping_add(black_box(run_once()));
        samples.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
    }
    black_box(checksum);

    let summary = summarize(&samples);
    let (layer, scenario, variant, cache_state, refresh_mode) =
        perf_case_contract_metadata(id, "journey");
    Ok(PerfCaseResult {
        id: id.to_string(),
        family: String::from("journey"),
        layer: String::from(layer),
        scenario: String::from(scenario),
        variant: String::from(variant),
        cache_state: String::from(cache_state),
        refresh_mode: String::from(refresh_mode),
        unit: String::from("us/journey"),
        gated: true,
        threshold_pct,
        median: summary.median,
        p95: summary.p95,
        p99: summary.p99,
        min: summary.min,
        max: summary.max,
        mean: summary.mean,
        samples: samples.len(),
        ops_per_sample: 1,
        notes,
        metrics: BTreeMap::new(),
    })
}

fn summarize(samples: &[f64]) -> SampleSummary
{
   assert!(!samples.is_empty(), "perf suites must record at least one sample");
   const STACK_SAMPLE_CAP: usize = 32;
   let sum = samples.iter().copied().sum::<f64>();
   if samples.len() <= STACK_SAMPLE_CAP
   {
      let mut stack = [0.0_f64; STACK_SAMPLE_CAP];
      stack[..samples.len()].copy_from_slice(samples);
      let sorted = &mut stack[..samples.len()];
      sorted.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
      return summarize_sorted_samples(sorted, sum, samples.len());
   }

   let mut sorted = samples.to_vec();
   sorted.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
   summarize_sorted_samples(&sorted, sum, samples.len())
}

fn summarize_sorted_samples(sorted: &[f64], sum: f64, sample_count: usize) -> SampleSummary
{
   match sorted.len()
   {
      6 => {
         let p95_weight = 5.0 * 0.95 - 4.0;
         let p99_weight = 5.0 * 0.99 - 4.0;
         return SampleSummary {
            min: sorted[0],
            max: sorted[5],
            mean: sum / sample_count as f64,
            median: 0.5 * sorted[2] + 0.5 * sorted[3],
            p95: (1.0 - p95_weight) * sorted[4] + p95_weight * sorted[5],
            p99: (1.0 - p99_weight) * sorted[4] + p99_weight * sorted[5],
         };
      }
      10 => {
         let p95_weight = 9.0 * 0.95 - 8.0;
         let p99_weight = 9.0 * 0.99 - 8.0;
         return SampleSummary {
            min: sorted[0],
            max: sorted[9],
            mean: sum / sample_count as f64,
            median: 0.5 * sorted[4] + 0.5 * sorted[5],
            p95: (1.0 - p95_weight) * sorted[8] + p95_weight * sorted[9],
            p99: (1.0 - p99_weight) * sorted[8] + p99_weight * sorted[9],
         };
      }
      12 => {
         let p95_weight = 11.0 * 0.95 - 10.0;
         let p99_weight = 11.0 * 0.99 - 10.0;
         return SampleSummary {
            min: sorted[0],
            max: sorted[11],
            mean: sum / sample_count as f64,
            median: 0.5 * sorted[5] + 0.5 * sorted[6],
            p95: (1.0 - p95_weight) * sorted[10] + p95_weight * sorted[11],
            p99: (1.0 - p99_weight) * sorted[10] + p99_weight * sorted[11],
         };
      }
      24 => {
         let p95_weight = 23.0 * 0.95 - 21.0;
         let p99_weight = 23.0 * 0.99 - 22.0;
         return SampleSummary {
            min: sorted[0],
            max: sorted[23],
            mean: sum / sample_count as f64,
            median: 0.5 * sorted[11] + 0.5 * sorted[12],
            p95: (1.0 - p95_weight) * sorted[21] + p95_weight * sorted[22],
            p99: (1.0 - p99_weight) * sorted[22] + p99_weight * sorted[23],
         };
      }
      _ => {}
   }

   SampleSummary {
      min: *sorted.first().unwrap_or(&0.0),
      max: *sorted.last().unwrap_or(&0.0),
      mean: sum / sample_count as f64,
      median: quantile(sorted, 0.5),
      p95: quantile(sorted, 0.95),
      p99: quantile(sorted, 0.99),
   }
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let n = sorted.len() as f64;
    let idx = ((n - 1.0) * q).clamp(0.0, n - 1.0);
    let lo = idx.floor() as usize;
    let hi = idx.ceil() as usize;
    if lo == hi {
        return sorted[lo];
    }
    let weight = idx - lo as f64;
    (1.0 - weight) * sorted[lo] + weight * sorted[hi]
}

fn summarize_distribution_metrics(samples: &[f64]) -> DistributionMetricSummary
{
   const STACK_SAMPLE_CAP: usize = 32;
   if samples.len() <= STACK_SAMPLE_CAP
   {
      let mut stack = [0.0_f64; STACK_SAMPLE_CAP];
      stack[..samples.len()].copy_from_slice(samples);
      let sorted = &mut stack[..samples.len()];
      sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
      return summarize_sorted_distribution_metrics(sorted);
   }

   let mut sorted = samples.to_vec();
   sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
   summarize_sorted_distribution_metrics(&sorted)
}

fn summarize_sorted_distribution_metrics(sorted: &[f64]) -> DistributionMetricSummary
{
   if sorted.len() == 24
   {
      let p95_weight = 23.0 * 0.95 - 21.0;
      let p99_weight = 23.0 * 0.99 - 22.0;
      return DistributionMetricSummary {
         max: sorted[23],
         median: 0.5 * sorted[11] + 0.5 * sorted[12],
         p95: (1.0 - p95_weight) * sorted[21] + p95_weight * sorted[22],
         p99: (1.0 - p99_weight) * sorted[22] + p99_weight * sorted[23],
      };
   }

   DistributionMetricSummary {
      max: *sorted.last().unwrap_or(&0.0),
      median: quantile(sorted, 0.5),
      p95: quantile(sorted, 0.95),
      p99: quantile(sorted, 0.99),
   }
}

fn insert_distribution_metrics(metrics: &mut BTreeMap<String, f64>, prefix: &str, samples: &[f64]) {
    if samples.is_empty() {
        return;
    }
    let summary = summarize_distribution_metrics(samples);
    match prefix {
        "frame_ms" => insert_distribution_metric_values(
            metrics,
            "frame_ms_p50",
            "frame_ms_p95",
            "frame_ms_p99",
            "frame_ms_peak",
            summary,
        ),
        "gpu_ms" => insert_distribution_metric_values(
            metrics,
            "gpu_ms_p50",
            "gpu_ms_p95",
            "gpu_ms_p99",
            "gpu_ms_peak",
            summary,
        ),
        "event_to_visible_ms" => insert_distribution_metric_values(
            metrics,
            "event_to_visible_ms_p50",
            "event_to_visible_ms_p95",
            "event_to_visible_ms_p99",
            "event_to_visible_ms_peak",
            summary,
        ),
        _ => {
            metrics.insert(format!("{}_p50", prefix), summary.median);
            metrics.insert(format!("{}_p95", prefix), summary.p95);
            metrics.insert(format!("{}_p99", prefix), summary.p99);
            metrics.insert(format!("{}_peak", prefix), summary.max);
        }
    }
}

fn insert_distribution_metric_values(
    metrics: &mut BTreeMap<String, f64>,
    p50_key: &str,
    p95_key: &str,
    p99_key: &str,
    peak_key: &str,
    summary: DistributionMetricSummary,
) {
    metrics.insert(String::from(p50_key), summary.median);
    metrics.insert(String::from(p95_key), summary.p95);
    metrics.insert(String::from(p99_key), summary.p99);
    metrics.insert(String::from(peak_key), summary.max);
}

fn insert_frame_pacing_metrics(metrics: &mut BTreeMap<String, f64>, frame_samples_ms: &[f64]) {
    insert_frame_pacing_metrics_for_refresh(metrics, frame_samples_ms, 60);
    insert_frame_pacing_metrics_for_refresh(metrics, frame_samples_ms, 120);
}

fn insert_frame_pacing_metrics_for_refresh(
    metrics: &mut BTreeMap<String, f64>,
    frame_samples_ms: &[f64],
    refresh_hz: u32,
) {
    if frame_samples_ms.is_empty() {
        return;
    }
    let budget_ms = 1000.0 / refresh_hz as f64;
    let missed_frames = frame_samples_ms.iter().filter(|sample| **sample > budget_ms).count();
    let hitch_frames = frame_samples_ms.iter().filter(|sample| **sample > budget_ms * 2.0).count();
    let denom = frame_samples_ms.len() as f64;
    match refresh_hz {
        60 => insert_frame_pacing_metric_values(
            metrics,
            "frame_budget_60hz_ms",
            "missed_frames_60hz",
            "missed_frame_ratio_60hz",
            "hitch_frames_60hz",
            "hitch_ratio_60hz",
            budget_ms,
            missed_frames,
            hitch_frames,
            denom,
        ),
        120 => insert_frame_pacing_metric_values(
            metrics,
            "frame_budget_120hz_ms",
            "missed_frames_120hz",
            "missed_frame_ratio_120hz",
            "hitch_frames_120hz",
            "hitch_ratio_120hz",
            budget_ms,
            missed_frames,
            hitch_frames,
            denom,
        ),
        _ => {
            let label = format!("{}hz", refresh_hz);
            metrics.insert(format!("frame_budget_{}_ms", label), budget_ms);
            metrics.insert(format!("missed_frames_{}", label), missed_frames as f64);
            metrics.insert(format!("missed_frame_ratio_{}", label), missed_frames as f64 / denom);
            metrics.insert(format!("hitch_frames_{}", label), hitch_frames as f64);
            metrics.insert(format!("hitch_ratio_{}", label), hitch_frames as f64 / denom);
        }
    }
}

fn insert_frame_pacing_metric_values(
    metrics: &mut BTreeMap<String, f64>,
    budget_key: &str,
    missed_key: &str,
    missed_ratio_key: &str,
    hitch_key: &str,
    hitch_ratio_key: &str,
    budget_ms: f64,
    missed_frames: usize,
    hitch_frames: usize,
    denom: f64,
) {
    metrics.insert(String::from(budget_key), budget_ms);
    metrics.insert(String::from(missed_key), missed_frames as f64);
    metrics.insert(String::from(missed_ratio_key), missed_frames as f64 / denom);
    metrics.insert(String::from(hitch_key), hitch_frames as f64);
    metrics.insert(String::from(hitch_ratio_key), hitch_frames as f64 / denom);
}

fn regression_allowed_median(current: &PerfCaseResult, base: &PerfCaseResult) -> f64 {
    let mut allowed = base.median * (1.0 + current.threshold_pct);
    if base.median > 0.0 {
        let baseline_variance = base.p95 / base.median;
        if baseline_variance >= 1.30 {
            allowed = allowed.max(base.p95 * 1.10);
        }
    }
    allowed
}

pub fn compare_reports(current: &PerfReport, baseline: &PerfReport) -> PerfComparison {
   let missing_capacity = current.cases.len();
   let matched_capacity = current.cases.len().min(baseline.cases.len());
   if current.cases.len() == baseline.cases.len() {
      if let Some(comparison) = try_compare_reports_same_case_order(current, matched_capacity, baseline) {
         return comparison;
      }
   }
   if baseline.cases.len() <= LINEAR_COMPARE_BASELINE_CASE_LIMIT {
      return compare_reports_with_lookup(current, missing_capacity, matched_capacity, |id| {
         baseline.cases.iter().find(|case| case.id.as_str() == id)
      });
   }

   let baseline_map =
      baseline.cases.iter().map(|case| (case.id.as_str(), case)).collect::<BTreeMap<_, _>>();

   compare_reports_with_lookup(current, missing_capacity, matched_capacity, |id| baseline_map.get(id).copied())
}

fn try_compare_reports_same_case_order(current: &PerfReport, matched_capacity: usize, baseline: &PerfReport) -> Option<PerfComparison> {
    if current.cases.len() != baseline.cases.len() {
        return None;
    }
    let mut comparison = PerfComparison {
        matched: 0,
        missing_baseline: Vec::new(),
        regressions: Vec::new(),
        improvements: Vec::with_capacity(matched_capacity),
    };
    for (case, base) in current.cases.iter().zip(&baseline.cases) {
        if case.id != base.id {
            return None;
        }
        if !case.gated {
            continue;
        }
        comparison.matched += 1;
        if case.median == base.median {
            continue;
        }
        let allowed = regression_allowed_median(case, base);
        if case.median > allowed {
            comparison.regressions.push(PerfRegression {
                id: case.id.clone(),
                baseline_median: base.median,
                current_median: case.median,
                allowed_median: allowed,
                delta_pct: if base.median > 0.0 {
                    ((case.median - base.median) / base.median) * 100.0
                } else {
                    0.0
                },
            });
        } else if case.median < base.median {
            comparison.improvements.push(case.id.clone());
        }
    }
    Some(comparison)
}

fn compare_reports_with_lookup<'a, F>(current: &PerfReport, missing_capacity: usize, matched_capacity: usize, mut baseline_case: F) -> PerfComparison
where
    F: FnMut(&str) -> Option<&'a PerfCaseResult>,
{
    let mut comparison = PerfComparison {
        matched: 0,
        missing_baseline: Vec::with_capacity(missing_capacity),
        regressions: Vec::new(),
        improvements: Vec::with_capacity(matched_capacity),
    };
    for case in &current.cases {
        if !case.gated {
            continue;
        }
        let Some(base) = baseline_case(case.id.as_str()) else {
            comparison.missing_baseline.push(case.id.clone());
            continue;
        };
        comparison.matched += 1;
        if case.median < base.median {
            comparison.improvements.push(case.id.clone());
            continue;
        }
        let allowed = regression_allowed_median(case, base);
        if case.median > allowed {
            comparison.regressions.push(PerfRegression {
                id: case.id.clone(),
                baseline_median: base.median,
                current_median: case.median,
                allowed_median: allowed,
                delta_pct: if base.median > 0.0 {
                    ((case.median - base.median) / base.median) * 100.0
                } else {
                    0.0
                },
            });
        } else if case.median < base.median {
            comparison.improvements.push(case.id.clone());
        }
    }
    comparison
}

pub fn assert_full_coverage(coverage: &CoverageReport) -> Result<()> {
    let checks = [
        (
            coverage.components_total,
            coverage.components_covered.len(),
            "component perf coverage is incomplete",
        ),
        (
            coverage.animations_total,
            coverage.animations_covered.len(),
            "animation perf coverage is incomplete",
        ),
        (
            coverage.launch_total,
            coverage.launch_covered.len(),
            "launch perf coverage is incomplete",
        ),
        (
            coverage.primitive_lifecycle_total,
            coverage.primitive_lifecycle_covered.len(),
            "primitive lifecycle perf coverage is incomplete",
        ),
        (
            coverage.scenes_cpu_total,
            coverage.scenes_cpu_covered.len(),
            "cpu scene perf coverage is incomplete",
        ),
        (
            coverage.scenes_gpu_total,
            coverage.scenes_gpu_covered.len(),
            "gpu scene perf coverage is incomplete",
        ),
        (
            coverage.journeys_total,
            coverage.journeys_covered.len(),
            "user journey perf coverage is incomplete",
        ),
        (
            coverage.authoring_total,
            coverage.authoring_covered.len(),
            "authoring perf coverage is incomplete",
        ),
        (
            coverage.layout_total,
            coverage.layout_covered.len(),
            "layout perf coverage is incomplete",
        ),
        (
            coverage.text_input_total,
            coverage.text_input_covered.len(),
            "text-input perf coverage is incomplete",
        ),
        (
            coverage.image_pipeline_total,
            coverage.image_pipeline_covered.len(),
            "image-pipeline perf coverage is incomplete",
        ),
        (
            coverage.navigation_total,
            coverage.navigation_covered.len(),
            "navigation perf coverage is incomplete",
        ),
        (
            coverage.reconcile_total,
            coverage.reconcile_covered.len(),
            "reconcile perf coverage is incomplete",
        ),
        (
            coverage.endurance_total,
            coverage.endurance_covered.len(),
            "endurance perf coverage is incomplete",
        ),
        (
            coverage.stress_total,
            coverage.stress_covered.len(),
            "stress perf coverage is incomplete",
        ),
        (
            coverage.bridges_total,
            coverage.bridges_covered.len(),
            "bridge perf coverage is incomplete",
        ),
    ];

    for (total, covered, message) in checks {
        if total != covered {
            bail!(message);
        }
    }

    Ok(())
}

pub fn assert_contract_coverage(contract: &ContractCoverageReport) -> Result<()> {
   for entry in contract.layers.iter().chain(contract.battery.iter()) {
      match entry.status.as_str() {
         "implemented" | "partial" | "missing" | "separate" => {}
         _ => bail!("unknown contract coverage status `{}` for `{}`", entry.status, entry.id),
      }
      if entry.status == "implemented" {
         for note in &entry.notes {
            if note_contains_unresolved_gap(note) {
               bail!(
                  "implemented contract row `{}` contains unresolved-gap note: {}",
                  entry.id,
                  note
               );
            }
         }
      }
   }
   Ok(())
}

fn note_contains_unresolved_gap(note: &str) -> bool {
   let bytes = note.as_bytes();
   for index in 0..bytes.len() {
      match bytes[index] | 0x20 {
         b'm' => {
            if ascii_prefix_ignore_case(&bytes[index..], b"missing") {
               return true;
            }
         }
         b'i' => {
            if ascii_prefix_ignore_case(&bytes[index..], b"incomplete") {
               return true;
            }
         }
         b'n' => {
            if ascii_prefix_ignore_case(&bytes[index..], b"not yet")
               || ascii_prefix_ignore_case(&bytes[index..], b"no dedicated")
            {
               return true;
            }
         }
         b's' => {
            if ascii_prefix_ignore_case(&bytes[index..], b"still not") {
               return true;
            }
         }
         _ => {}
      }
   }
   false
}

fn ascii_prefix_ignore_case(value: &[u8], prefix: &[u8]) -> bool
{
   if value.len() < prefix.len()
   {
      return false;
   }
   for index in 1..prefix.len()
   {
      if (value[index] | 0x20) != prefix[index]
      {
         return false;
      }
   }
   true
}

pub fn assert_case_metric_contract(cases: &[PerfCaseResult]) -> Result<()> {
    let mut missing = Vec::new();
    for case in cases {
        if case_requires_frame_metrics(case) {
            push_missing_frame_metrics(case, &mut missing);
        }
        if case_requires_gpu_metrics(case) {
            push_missing_gpu_metrics(case, &mut missing);
        }
    }
    if !missing.is_empty() {
        bail!("case metric contract is incomplete: {}", missing.join("; "));
    }
    Ok(())
}

fn case_requires_frame_metrics(case: &PerfCaseResult) -> bool {
    case.unit == "ms/frame" || case.metrics.contains_key("frame_ms_p50")
}

fn case_requires_gpu_metrics(case: &PerfCaseResult) -> bool {
    (case.id.starts_with("gpu.") && case.unit == "ms/frame")
        || case.metrics.contains_key("gpu_ms_p50")
}

const REQUIRED_FRAME_METRIC_KEYS: &[&str] = &[
    "frame_ms_p50",
    "frame_ms_p95",
    "frame_ms_p99",
    "frame_ms_peak",
    "frame_budget_60hz_ms",
    "missed_frames_60hz",
    "missed_frame_ratio_60hz",
    "hitch_frames_60hz",
    "hitch_ratio_60hz",
    "frame_budget_120hz_ms",
    "missed_frames_120hz",
    "missed_frame_ratio_120hz",
    "hitch_frames_120hz",
    "hitch_ratio_120hz",
];

const REQUIRED_GPU_METRIC_KEYS: &[&str] = &[
    "gpu_ms_p50",
    "gpu_ms_p95",
    "gpu_ms_p99",
    "gpu_ms_peak",
];

fn push_missing_frame_metrics(case: &PerfCaseResult, missing: &mut Vec<String>) {
    for key in REQUIRED_FRAME_METRIC_KEYS {
        push_missing_metric(case, key, missing);
    }
}

fn push_missing_gpu_metrics(case: &PerfCaseResult, missing: &mut Vec<String>) {
    for key in REQUIRED_GPU_METRIC_KEYS {
        push_missing_metric(case, key, missing);
    }
}

fn push_missing_metric(case: &PerfCaseResult, key: &str, missing: &mut Vec<String>) {
    if !case.metrics.contains_key(key) {
        missing.push(format!("{} missing `{}`", case.id, key));
    }
}

fn load_report(path: &Path) -> Result<PerfReport> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

fn write_report_json(path: &Path, report: &PerfReport) -> Result<()> {
    ensure_parent(path)?;
    let body = serialize_report_json(report)?;
    fs::write(path, body).with_context(|| format!("writing {}", path.display()))
}

fn serialize_report_json(report: &PerfReport) -> Result<Vec<u8>> {
    let mut body = Vec::with_capacity(report_json_capacity_hint(report));
    serde_json::to_writer_pretty(&mut body, report)?;
    Ok(body)
}

fn serialize_report_json_string(report: &PerfReport) -> Result<String> {
    serde_json::to_string_pretty(report).context("serializing perf report json")
}

fn report_json_capacity_hint(report: &PerfReport) -> usize {
    4096usize
        .saturating_add(report.cases.len().saturating_mul(930))
        .saturating_add(report.findings.len().saturating_mul(384))
}

fn write_markdown_outputs(
    latest_path: &Path,
    report: &PerfReport,
    comparison: Option<&PerfComparison>,
) -> Result<()> {
    ensure_parent(latest_path)?;
    let body = render_markdown(report, comparison);
    fs::write(latest_path, body.as_bytes())
        .with_context(|| format!("writing {}", latest_path.display()))?;
    let Some(date_label) = report.generated_label.as_ref() else {
        return Ok(());
    };
    let Some(parent) = latest_path.parent() else {
        return Ok(());
    };
    let dated = parent.join(format!("{}.md", date_label));
    ensure_parent(&dated)?;
    fs::write(&dated, body.as_bytes()).with_context(|| format!("writing {}", dated.display()))
}

fn render_markdown(report: &PerfReport, comparison: Option<&PerfComparison>) -> String {
    let mut out = String::new();
    out.push_str("# Oxide Performance Report\n\n");
    let _ = std::fmt::Write::write_fmt(&mut out, format_args!("- Suite: `{}`\n", report.suite));
    if let Some(label) = report.generated_label.as_ref() {
        let _ = std::fmt::Write::write_fmt(&mut out, format_args!("- Label: `{}`\n", label));
    }
    let _ = std::fmt::Write::write_fmt(
        &mut out,
        format_args!(
            "- Coverage: {}/{} components, {}/{} animations, {}/{} launch cases, {}/{} primitive lifecycle cases, {}/{} CPU scenes, {}/{} GPU scenes, {}/{} journeys, {}/{} authoring APIs, {}/{} layout cases, {}/{} text-input cases, {}/{} image pipeline cases, {}/{} navigation cases, {}/{} reconcile cases, {}/{} endurance cases, {}/{} stress cases, {}/{} bridge paths\n",
            report.coverage.components_covered.len(),
            report.coverage.components_total,
            report.coverage.animations_covered.len(),
            report.coverage.animations_total,
            report.coverage.launch_covered.len(),
            report.coverage.launch_total,
            report.coverage.primitive_lifecycle_covered.len(),
            report.coverage.primitive_lifecycle_total,
            report.coverage.scenes_cpu_covered.len(),
            report.coverage.scenes_cpu_total,
            report.coverage.scenes_gpu_covered.len(),
            report.coverage.scenes_gpu_total,
            report.coverage.journeys_covered.len(),
            report.coverage.journeys_total,
            report.coverage.authoring_covered.len(),
            report.coverage.authoring_total,
            report.coverage.layout_covered.len(),
            report.coverage.layout_total,
            report.coverage.text_input_covered.len(),
            report.coverage.text_input_total,
            report.coverage.image_pipeline_covered.len(),
            report.coverage.image_pipeline_total,
            report.coverage.navigation_covered.len(),
            report.coverage.navigation_total,
            report.coverage.reconcile_covered.len(),
            report.coverage.reconcile_total,
            report.coverage.endurance_covered.len(),
            report.coverage.endurance_total,
            report.coverage.stress_covered.len(),
            report.coverage.stress_total,
            report.coverage.bridges_covered.len(),
            report.coverage.bridges_total
        )
    );
    if let Some(comp) = comparison {
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!(
                "- Comparison: `{}` matched, `{}` regressions, `{}` missing baseline cases\n",
                comp.matched,
                comp.regressions.len(),
                comp.missing_baseline.len()
            )
        );
    }
    out.push('\n');

    out.push_str("## Contract Coverage\n\n");
    out.push_str("| Section | Status | Notes |\n");
    out.push_str("| --- | --- | --- |\n");
    for entry in &report.contract.layers {
        write_contract_coverage_row(&mut out, entry);
    }
    for entry in &report.contract.battery {
        write_contract_coverage_row(&mut out, entry);
    }
    if !report.contract.notes.is_empty() {
        out.push_str("\n");
        for note in &report.contract.notes {
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("- {}\n", note));
        }
        out.push('\n');
    }

    out.push_str("## Results\n\n");
    out.push_str("| Case | Layer | Scenario | Variant | Cache | Refresh | P50 | P95 | P99 | Peak | Unit | Gate | Key Metrics |\n");
    out.push_str(
        "| --- | --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | --- | --- | --- |\n",
    );
    for case in &report.cases {
        let gate = if case.gated { "regression-gated" } else { "audit-only" };
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!(
                "| `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} | ",
                case.id,
                case.layer,
                case.scenario,
                case.variant,
                case.cache_state,
                case.refresh_mode,
                case.median,
                case.p95,
                case.p99,
                case.max,
                case.unit,
                gate
            )
        );
        write_case_metrics_summary(&mut out, &case.metrics);
        out.push_str(" |\n");
    }

    let speedups = compute_audit_speedups(report);
    if !speedups.is_empty() {
        out.push_str("\n## A/B Audit\n\n");
        for (label, value) in speedups {
            out.push_str(&format!(
                "- {}: {:.2}x faster than the retained legacy path\n",
                label, value
            ));
        }
    }

    if let Some(comp) = comparison {
        out.push_str("\n## Regression Check\n\n");
        if comp.regressions.is_empty() && comp.missing_baseline.is_empty() {
            out.push_str("- No gated regressions detected.\n");
        }
        for reg in &comp.regressions {
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!(
                    "- Regression: `{}` median {:.3} vs baseline {:.3} (allowed {:.3}, delta {:+.2}%)\n",
                    reg.id,
                    reg.current_median,
                    reg.baseline_median,
                    reg.allowed_median,
                    reg.delta_pct
                )
            );
        }
        for missing in &comp.missing_baseline {
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!("- Missing baseline case: `{}`\n", missing)
            );
        }
    }

    out.push_str("\n## Findings\n\n");
    for finding in &report.findings {
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!("- [{}] {}\n", finding.status, finding.summary)
        );
    }

    out.push_str("\n## Baseline Workflow\n\n");
    out.push_str("- Update the committed baseline only with review: `PERF_REPORT_DATE=$(date +%F) cargo run --release --locked -j$(sysctl -n hw.ncpu) -p oxide-perf-runner -- --run-suite --write-baseline`\n");
    let _ = std::fmt::Write::write_fmt(
        &mut out,
        format_args!("- Latest JSON baseline: `{}`\n", DEFAULT_BASELINE_JSON)
    );
    let _ = std::fmt::Write::write_fmt(
        &mut out,
        format_args!("- Latest Markdown baseline: `{}`\n", DEFAULT_BASELINE_MARKDOWN)
    );
    out
}

fn write_contract_coverage_row(out: &mut String, entry: &ContractCoverageEntry) {
    let _ = std::fmt::Write::write_fmt(
        out,
        format_args!("| `{}` | `{}` | ", entry.label, entry.status)
    );
    for (index, note) in entry.notes.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push_str(note);
    }
    out.push_str(" |\n");
}

fn write_case_metrics_summary(out: &mut String, metrics: &BTreeMap<String, f64>) {
    if metrics.is_empty() {
        out.push_str("`-`");
        return;
    }
    let priority = [
        "gpu_ms_p50",
        "gpu_ms_p95",
        "gpu_ms_p99",
        "gpu_ms_peak",
        "hitch_ms_per_s",
        "missed_frames",
        "missed_frames_per_s",
        "missed_frame_ratio_120hz",
        "hitch_ratio_120hz",
        "missed_frame_ratio_60hz",
        "hitch_ratio_60hz",
        "frame_interval_ms_p50",
        "frame_interval_ms_p95",
        "frame_ms_p50",
        "frame_ms_p95",
        "frame_backpressure_skips",
    ];
    let mut emitted = 0usize;
    let mut emitted_priority = [false; 16];
    out.push('`');
    for (index, name) in priority.iter().enumerate() {
        if let Some(value) = metrics.get(*name) {
            push_case_metric_summary_part(out, &mut emitted, name, *value);
            emitted_priority[index] = true;
            if emitted == 6 {
                break;
            }
        }
    }
    for (name, value) in metrics {
        if emitted == 6 {
            break;
        }
        if case_metric_priority_index(name.as_str())
            .map(|index| emitted_priority[index])
            .unwrap_or(false)
        {
            continue;
        }
        push_case_metric_summary_part(out, &mut emitted, name, *value);
    }
    out.push('`');
}

fn case_metric_priority_index(name: &str) -> Option<usize> {
    match name {
        "gpu_ms_p50" => Some(0),
        "gpu_ms_p95" => Some(1),
        "gpu_ms_p99" => Some(2),
        "gpu_ms_peak" => Some(3),
        "hitch_ms_per_s" => Some(4),
        "missed_frames" => Some(5),
        "missed_frames_per_s" => Some(6),
        "missed_frame_ratio_120hz" => Some(7),
        "hitch_ratio_120hz" => Some(8),
        "missed_frame_ratio_60hz" => Some(9),
        "hitch_ratio_60hz" => Some(10),
        "frame_interval_ms_p50" => Some(11),
        "frame_interval_ms_p95" => Some(12),
        "frame_ms_p50" => Some(13),
        "frame_ms_p95" => Some(14),
        "frame_backpressure_skips" => Some(15),
        _ => None,
    }
}

fn push_case_metric_summary_part(out: &mut String, emitted: &mut usize, name: &str, value: f64) {
    if *emitted > 0 {
        out.push_str("; ");
    }
    let _ = std::fmt::Write::write_fmt(out, format_args!("{}={:.3}", name, value));
    *emitted += 1;
}

fn compute_audit_speedups(_report: &PerfReport) -> Vec<(String, f64)> {
    Vec::new()
}

fn print_summary(report: &PerfReport, comparison: Option<&PerfComparison>) {
    println!(
        "suite={} cases={} components={}/{} animations={}/{} launch={}/{} primitive_lifecycle={}/{} scenes_cpu={}/{} scenes_gpu={}/{} journeys={}/{} authoring={}/{} layout={}/{} text_input={}/{} image_pipeline={}/{} navigation={}/{} reconcile={}/{} endurance={}/{} stress={}/{} bridges={}/{}",
        report.suite,
        report.cases.len(),
        report.coverage.components_covered.len(),
        report.coverage.components_total,
        report.coverage.animations_covered.len(),
        report.coverage.animations_total,
        report.coverage.launch_covered.len(),
        report.coverage.launch_total,
        report.coverage.primitive_lifecycle_covered.len(),
        report.coverage.primitive_lifecycle_total,
        report.coverage.scenes_cpu_covered.len(),
        report.coverage.scenes_cpu_total,
        report.coverage.scenes_gpu_covered.len(),
        report.coverage.scenes_gpu_total,
        report.coverage.journeys_covered.len(),
        report.coverage.journeys_total,
        report.coverage.authoring_covered.len(),
        report.coverage.authoring_total,
        report.coverage.layout_covered.len(),
        report.coverage.layout_total,
        report.coverage.text_input_covered.len(),
        report.coverage.text_input_total,
        report.coverage.image_pipeline_covered.len(),
        report.coverage.image_pipeline_total,
        report.coverage.navigation_covered.len(),
        report.coverage.navigation_total,
        report.coverage.reconcile_covered.len(),
        report.coverage.reconcile_total,
        report.coverage.endurance_covered.len(),
        report.coverage.endurance_total,
        report.coverage.stress_covered.len(),
        report.coverage.stress_total,
        report.coverage.bridges_covered.len(),
        report.coverage.bridges_total
    );
    for case in &report.cases {
        println!(
            "case={} layer={} scenario={} variant={} cache={} refresh={} median={:.3} p95={:.3} p99={:.3} unit={}",
            case.id,
            case.layer,
            case.scenario,
            case.variant,
            case.cache_state,
            case.refresh_mode,
            case.median,
            case.p95,
            case.p99,
            case.unit
        );
    }
    if let Some(comp) = comparison {
        println!(
            "comparison matched={} regressions={} missing_baseline={}",
            comp.matched,
            comp.regressions.len(),
            comp.missing_baseline.len()
        );
    }
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    Ok(())
}

fn build_prepare_drawlist() -> api::DrawList {
    let mut builder = ui::DrawListBuilder::new();
    let color = api::Color::rgba(0.2, 0.4, 0.9, 1.0);
    for group in 0..48 {
        for depth in 0..8 {
            builder.clip_push(api::RectI::new(
                depth * 3,
                depth * 3,
                220 - depth * 4,
                180 - depth * 4,
            ));
        }
        for draw in 0..6 {
            let x = (group * 7 + draw * 9) as f32;
            let y = (group * 5 + draw * 3) as f32;
            builder.rrect(api::RectF::new(x, y, 24.0, 12.0), [3.0; 4], color);
        }
        for _ in 0..8 {
            builder.clip_pop();
        }
    }
    builder.into_inner()
}

fn build_coalesce_items() -> Vec<api::DrawCmd> {
    let mut items = Vec::new();
    let white = api::Color::rgba(1.0, 1.0, 1.0, 1.0);
    let accent = api::Color::rgba(0.9, 0.3, 0.2, 1.0);
    let mut vb = 0u32;
    let mut ib = 0u32;
    for index in 0..768u32 {
        let color = if index % 17 == 0 { accent } else { white };
        items.push(api::DrawCmd::Solid {
            vb: api::VertexSpan { offset: vb, len: 6 },
            ib: api::IndexSpan { offset: ib, len: 6 },
            color,
        });
        vb += 6;
        ib += 6;
    }
    items
}

fn build_gesture_events() -> Vec<(platform::TouchEvent, u64)> {
    vec![
        (touch(platform::TouchId(1), platform::TouchPhase::Start, 0.0, 0.0), 0),
        (touch(platform::TouchId(1), platform::TouchPhase::Move, 0.0, 0.0), 500),
        (touch(platform::TouchId(1), platform::TouchPhase::Move, 20.0, 0.0), 516),
        (touch(platform::TouchId(1), platform::TouchPhase::End, 24.0, 0.0), 532),
        (touch(platform::TouchId(2), platform::TouchPhase::Start, 10.0, 10.0), 700),
        (touch(platform::TouchId(2), platform::TouchPhase::End, 10.0, 10.0), 760),
        (touch(platform::TouchId(3), platform::TouchPhase::Start, 10.0, 10.0), 860),
        (touch(platform::TouchId(3), platform::TouchPhase::End, 10.0, 10.0), 920),
    ]
}

fn build_touch_surface_events() -> Vec<platform::TouchEvent> {
    vec![
        touch(platform::TouchId(1), platform::TouchPhase::Start, 100.0, 200.0),
        touch(platform::TouchId(1), platform::TouchPhase::Move, 112.0, 210.0),
        touch(platform::TouchId(1), platform::TouchPhase::End, 112.0, 210.0),
        touch(platform::TouchId(2), platform::TouchPhase::Start, 120.0, 220.0),
        touch(platform::TouchId(3), platform::TouchPhase::Start, 220.0, 420.0),
        touch(platform::TouchId(2), platform::TouchPhase::Move, 110.0, 210.0),
        touch(platform::TouchId(3), platform::TouchPhase::Move, 240.0, 430.0),
        touch(platform::TouchId(2), platform::TouchPhase::End, 110.0, 210.0),
        touch(platform::TouchId(3), platform::TouchPhase::End, 240.0, 430.0),
    ]
}

fn run_gesture_sequence(events: &[(platform::TouchEvent, u64)]) -> u64 {
    let mut recognizer = GestureRecognizer::with_defaults();
    let mut count = 0u64;
    for (event, at) in events {
        count = count.wrapping_add(recognizer.on_touch(event, *at).len() as u64);
    }
    count
}

fn run_touch_surface_sequence(events: &[platform::TouchEvent]) -> u64 {
    let mut recognizer = TouchSurfaceRecognizer::new();
    let mut count = 0u64;
    for event in events {
        count = count.wrapping_add(recognizer.on_touch(event).len() as u64);
    }
    count
}

fn run_timer_schedule_advance() -> u64 {
    timing::testing::reset();
    let start = timing::now_ms();
    let fired = Arc::new(AtomicU64::new(0));
    for index in 0..32u64 {
        let fired = fired.clone();
        timing::schedule_after(index % 8, move || {
            fired.fetch_add(1, Ordering::Relaxed);
        });
    }
    timing::advance_timers(start.saturating_add(8));
    fired.load(Ordering::Relaxed)
}

fn run_anim_start_replace() -> u64 {
    timing::testing::reset();
    let mut checksum = 0u64;
    for index in 0..32u64 {
        let desc = perf_anim_desc(index + 1, platform::AnimProp::Opacity);
        checksum = checksum.wrapping_add(timing::anim::start(&desc));
    }
    timing::anim::step(timing::now_ms().saturating_add(1_000));
    checksum.wrapping_add(timing::testing::active_anims() as u64)
}

fn perf_anim_desc(id: u64, prop: platform::AnimProp) -> platform::AnimDesc {
    platform::AnimDesc {
        id,
        prop,
        from: platform::AnimValue::F32(0.0),
        to: platform::AnimValue::F32(1.0),
        curve: platform::AnimCurve::Ease {
            ease: platform::Ease { kind: platform::EaseKind::QuadInOut },
        },
        duration_ms: 250,
        delay_ms: 0,
        repeat: platform::Repeat::Once,
    }
}

fn run_text_shape_bake() -> u64 {
    let mut db = text::FontDb::default();
    let latin_id = db.add_font(text::Font::from_bytes(LATIN_FONT.to_vec()));
    let cjk_id = db.add_font(text::Font::from_bytes(CJK_FONT.to_vec()));
    let mut shaper = text::TextShaper::default();
    let latin = db.font(latin_id).expect("latin font");
    let cjk = db.font(cjk_id).expect("cjk font");
    let shaped_latin =
        shaper.shape(latin, latin_id, "Oxide perf audit", 18.0).expect("latin shape");
    let shaped_cjk = shaper.shape(cjk, cjk_id, "漢字", 18.0).expect("cjk shape");
    let mut atlas = text::Atlas::new(256, 256);
    let mut verts = Vec::new();
    let mut indices = Vec::new();
    let run_latin = shaped_latin.bake_into(
        &mut atlas,
        &mut verts,
        &mut indices,
        api::Color::rgba(0.2, 0.2, 0.2, 1.0),
        api::ImageHandle(1),
        0.0,
        0.0,
        1.0,
    );
    let run_cjk = shaped_cjk.bake_into(
        &mut atlas,
        &mut verts,
        &mut indices,
        api::Color::rgba(0.3, 0.4, 0.7, 1.0),
        api::ImageHandle(1),
        0.0,
        20.0,
        1.0,
    );
    (run_latin.vb.len + run_cjk.vb.len + indices.len() as u32) as u64
}

fn text_prefix_width_map_case(smoke: bool, text_loops: u64) -> PerfCaseResult {
    let mut case = measure_cpu_case(
        "cpu.system.text_prefix_width_map",
        "system",
        smoke,
        true,
        0.12,
        text_loops,
        vec![String::from(
            "Single shaped-run cursor prefix width map over a long unwrapped text input line.",
        )],
        move || run_text_prefix_width_map_stats().checksum,
    );
    let stats = run_text_prefix_width_map_stats();
    case.metrics.insert(String::from("text_bytes"), stats.text_bytes as f64);
    case.metrics.insert(String::from("prefix_boundaries"), stats.prefix_boundaries as f64);
    case.metrics.insert(String::from("width_entries"), stats.width_entries as f64);
    case.metrics.insert(String::from("shaped_runs"), stats.shaped_runs as f64);
    case
}

fn run_text_prefix_width_map_stats() -> TextPrefixWidthMapStats {
    let mut db = text::FontDb::default();
    let latin_id = db.add_font(text::Font::from_bytes(LATIN_FONT.to_vec()));
    let mut shaper = text::TextShaper::default();
    let Some(latin) = db.font(latin_id) else {
        return TextPrefixWidthMapStats::default();
    };
    let text_value =
        "Orbit telemetry cache line carries enough unwrapped text for cursor map pressure. "
            .repeat(12);
    let mut boundaries = Vec::with_capacity(text_value.len() + 1);
    for index in 0..=text_value.len() {
        boundaries.push(index);
    }
    let Ok(shaped) = shaper.shape(latin, latin_id, &text_value, 16.0) else {
        return TextPrefixWidthMapStats::default();
    };
    let widths = shaped.prefix_widths_for_boundaries(&boundaries);
    let last = widths.last().copied().map_or(0.0, |width| width);
    TextPrefixWidthMapStats {
        checksum: widths.len() as u64 + (last.max(0.0) * 64.0) as u64,
        text_bytes: text_value.len() as u64,
        prefix_boundaries: boundaries.len() as u64,
        width_entries: widths.len() as u64,
        shaped_runs: 1,
    }
}

fn text_fallback_label_encode_case(smoke: bool, text_loops: u64) -> PerfCaseResult {
    let mut case = measure_cpu_case(
        "cpu.system.text_fallback_label_encode",
        "system",
        smoke,
        true,
        0.12,
        text_loops,
        vec![String::from(
            "Visible label encoding for mixed Latin/CJK text using configured fallback-font shaped runs.",
        )],
        move || run_text_fallback_label_encode_stats().checksum,
    );
    let stats = run_text_fallback_label_encode_stats();
    case.metrics.insert(String::from("fallback_fonts"), 1.0);
    case.metrics.insert(String::from("fallback_label_glyph_runs"), stats.glyph_runs as f64);
    case.metrics.insert(String::from("fallback_label_vertices"), stats.vertices as f64);
    case.metrics.insert(String::from("fallback_label_indices"), stats.indices as f64);
    case.metrics.insert(String::from("atlas_revision"), stats.atlas_revision as f64);
    case
}

fn run_text_fallback_label_encode_stats() -> TextFallbackLabelStats {
    let mut text_ctx = perf_text_ctx();
    text_ctx.set_fallback_fonts(&[1]);
    let mut uploader = CpuUploader::default();
    let mut builder = ui::DrawListBuilder::new();
    let text_value = "ABÉ 漢 AB漢 ÉB漢 ".repeat(16);

    ui::elements::encode_label_text(
        &text_value,
        api::Color::rgba(0.1, 0.1, 0.1, 1.0),
        ui::elements::Align::Left,
        false,
        0,
        18.0,
        api::RectF::new(0.0, 0.0, 720.0, 28.0),
        2.0,
        &mut text_ctx,
        &mut uploader,
        &mut builder,
    );

    let dl = builder.drawlist();
    let glyph_runs =
        dl.items.iter().filter(|cmd| matches!(cmd, api::DrawCmd::GlyphRun { .. })).count() as u64;
    let vertices = dl.vertices.len() as u64;
    let indices = dl.indices.len() as u64;
    let atlas_revision = text_ctx.atlas_revision();
    TextFallbackLabelStats {
        checksum: glyph_runs
            .wrapping_add(vertices)
            .wrapping_add(indices)
            .wrapping_add(atlas_revision),
        glyph_runs,
        vertices,
        indices,
        atlas_revision,
    }
}

fn text_atlas_pressure_case(smoke: bool, text_loops: u64) -> PerfCaseResult {
    let mut case = measure_cpu_case(
        "cpu.system.text_atlas_pressure",
        "system",
        smoke,
        true,
        0.12,
        text_loops,
        vec![String::from("Glyph atlas packing under constrained capacity and LRU slot eviction.")],
        move || run_text_atlas_pressure_stats().checksum,
    );
    let stats = run_text_atlas_pressure_stats();
    case.metrics.insert(String::from("atlas_shape_count"), stats.shape_count as f64);
    case.metrics.insert(String::from("atlas_rendered_glyph_runs"), stats.rendered_runs as f64);
    case.metrics.insert(String::from("atlas_evictions"), stats.evictions as f64);
    case.metrics.insert(String::from("atlas_resident_glyphs"), stats.resident_glyphs as f64);
    case.metrics.insert(String::from("atlas_revision"), stats.revision as f64);
    case.metrics.insert(String::from("atlas_dirty_rects"), stats.dirty_rects as f64);
    case.metrics.insert(String::from("atlas_dirty_pixels"), stats.dirty_pixels as f64);
    case.metrics.insert(String::from("atlas_max_dirty_pixels"), stats.max_dirty_pixels as f64);
    case.metrics.insert(String::from("atlas_pressure_vertices"), stats.vertices as f64);
    case.metrics.insert(String::from("atlas_pressure_indices"), stats.indices as f64);
    case
}

fn run_text_atlas_pressure_stats() -> TextAtlasPressureStats {
    let mut db = text::FontDb::default();
    let latin_id = db.add_font(text::Font::from_bytes(LATIN_FONT.to_vec()));
    let mut shaper = text::TextShaper::default();
    let Some(latin) = db.font(latin_id) else {
        return TextAtlasPressureStats::default();
    };
    let shapes: Vec<_> = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
        .chars()
        .filter_map(|ch| {
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            shaper.shape(latin, latin_id, s, 22.0).ok().map(|shape| shape.to_owned_shape())
        })
        .collect();
    let mut atlas = text::Atlas::new(24, 24);
    let mut raster = text::RasterCtx::default();
    let mut verts = Vec::new();
    let mut indices = Vec::new();
    let mut checksum = 0u64;
    let mut rendered_runs = 0u64;
    let mut dirty_rects = 0u64;
    let mut dirty_pixels = 0u64;
    let mut max_dirty_pixels = 0u64;

    for (index, shape) in shapes.iter().enumerate() {
        atlas.clear_dirty();
        let run = shape.bake_into_with(
            latin,
            &mut raster,
            &mut atlas,
            &mut verts,
            &mut indices,
            api::Color::rgba(0.2, 0.2, 0.2, 1.0),
            api::ImageHandle(1),
            0.0,
            index as f32,
            1.0,
        );
        checksum = checksum
            .wrapping_add(run.vb.len as u64)
            .wrapping_add(run.ib.len as u64)
            .wrapping_add(atlas.eviction_count());
        if run.vb.len > 0 {
            rendered_runs = rendered_runs.saturating_add(1);
        }
        if let Some(rect) = atlas.dirty_rect() {
            let pixels = u64::from(rect.w).saturating_mul(u64::from(rect.h));
            dirty_rects = dirty_rects.saturating_add(1);
            dirty_pixels = dirty_pixels.saturating_add(pixels);
            max_dirty_pixels = max_dirty_pixels.max(pixels);
        }
    }

    let evictions = atlas.eviction_count();
    let resident_glyphs = atlas.glyph_count() as u64;
    let revision = atlas.revision();
    let vertices = verts.len() as u64;
    let index_count = indices.len() as u64;
    TextAtlasPressureStats {
        checksum: checksum
            .wrapping_add(resident_glyphs)
            .wrapping_add(index_count)
            .wrapping_add(evictions)
            .wrapping_add(dirty_pixels),
        shape_count: shapes.len() as u64,
        rendered_runs,
        evictions,
        resident_glyphs,
        revision,
        dirty_rects,
        dirty_pixels,
        max_dirty_pixels,
        vertices,
        indices: index_count,
    }
}

fn text_atlas_dirty_rect_upload_case(smoke: bool, text_loops: u64) -> PerfCaseResult {
    let mut case = measure_cpu_case(
        "cpu.system.text_atlas_dirty_rect_upload",
        "system",
        smoke,
        true,
        0.12,
        text_loops,
        vec![String::from(
            "TextCtx atlas publication through one full A8 create followed by incremental dirty-rect uploads for new glyphs.",
        )],
        move || run_text_atlas_dirty_rect_upload_stats().checksum,
    );
    let stats = run_text_atlas_dirty_rect_upload_stats();
    case.metrics.insert(String::from("atlas_create_calls"), stats.creates as f64);
    case.metrics.insert(String::from("atlas_update_calls"), stats.updates as f64);
    case.metrics.insert(String::from("dirty_upload_pixels"), stats.dirty_upload_pixels as f64);
    case.metrics.insert(String::from("full_upload_pixels"), stats.full_upload_pixels as f64);
    case.metrics.insert(String::from("max_dirty_update_pixels"), stats.max_update_pixels as f64);
    case.metrics.insert(String::from("atlas_row_bytes"), stats.row_bytes as f64);
    let ratio = if stats.full_upload_pixels == 0 {
        0.0
    } else {
        stats.dirty_upload_pixels as f64 / stats.full_upload_pixels as f64
    };
    case.metrics.insert(String::from("dirty_to_full_upload_ratio"), ratio);
    case
}

fn run_text_atlas_dirty_rect_upload_stats() -> TextAtlasUploadStats {
    let mut text_ctx = perf_text_ctx();
    let mut uploader = CountingTextUploader::default();
    let mut builder = ui::DrawListBuilder::new();
    let texts = ["A", "B", "É", "C"];
    let mut checksum = 0u64;

    for (index, text_value) in texts.iter().enumerate() {
        ui::elements::encode_label_text(
            text_value,
            api::Color::rgba(0.1, 0.1, 0.1, 1.0),
            ui::elements::Align::Left,
            false,
            0,
            18.0,
            api::RectF::new(0.0, index as f32 * 24.0, 360.0, 28.0),
            2.0,
            &mut text_ctx,
            &mut uploader,
            &mut builder,
        );
        let dl = builder.drawlist();
        checksum = checksum
            .wrapping_add(dl.items.len() as u64)
            .wrapping_add(dl.vertices.len() as u64)
            .wrapping_add(dl.indices.len() as u64)
            .wrapping_add(text_ctx.atlas_revision());
        builder.clear();
    }

    uploader.stats.checksum = checksum
        .wrapping_add(text_ctx.atlas.glyph_count() as u64)
        .wrapping_add(uploader.stats.dirty_upload_pixels)
        .wrapping_add(uploader.stats.full_upload_pixels);
    uploader.stats
}

const WRAPPED_LABEL_VARIANTS: usize = 4_096;

struct WrappedLabelEncodeBench {
    texts: Vec<Arc<str>>,
    text_index: usize,
    rect: api::RectF,
    text_ctx: ui::elements::TextCtx,
    uploader: CountingTextUploader,
    builder: ui::DrawListBuilder,
}

impl WrappedLabelEncodeBench {
    fn new() -> Self {
        let stem = "Orbit telemetry cache labels wrap across narrow rows while preserving glyph atlas updates and final line output";
        let mut texts = Vec::with_capacity(WRAPPED_LABEL_VARIANTS);
        for index in 0..WRAPPED_LABEL_VARIANTS {
            texts.push(Arc::<str>::from(format!(
                "{stem} sample {:04} with stable ASCII words for wrapped label cache miss pressure",
                index
            )));
        }
        Self {
            texts,
            text_index: 0,
            rect: api::RectF::new(0.0, 0.0, 164.0, 220.0),
            text_ctx: perf_text_ctx(),
            uploader: CountingTextUploader::default(),
            builder: ui::DrawListBuilder::new(),
        }
    }

    fn encode_cached_once(&mut self) -> u64 {
        let Self { texts, text_index, rect, text_ctx, uploader, builder, .. } = self;
        let index = *text_index % texts.len();
        *text_index = text_index.saturating_add(1);
        let text_value = Arc::clone(&texts[index]);
        builder.clear();
        ui::elements::encode_label_text(
            text_value.as_ref(),
            api::Color::rgba(0.1, 0.1, 0.1, 1.0),
            ui::elements::Align::Left,
            true,
            0,
            13.0,
            *rect,
            PERF_DEVICE_SCALE,
            text_ctx,
            uploader,
            builder,
        );
        let dl = builder.drawlist();
        dl.items.len() as u64
            + dl.vertices.len() as u64
            + dl.indices.len() as u64
            + text_ctx.atlas_revision()
    }
}

fn wrapped_label_cached_encode_case(smoke: bool, text_loops: u64) -> PerfCaseResult {
    let mut bench = WrappedLabelEncodeBench::new();
    let mut case = measure_cpu_case(
        "cpu.system.wrapped_label_cached_encode",
        "system",
        smoke,
        true,
        0.12,
        text_loops,
        vec![String::from(
            "Current TextCtx ASCII wrapped-label cache-miss path using one shaped run for break decisions and final-line shaping only.",
        )],
        move || bench.encode_cached_once(),
    );
    let stats = run_wrapped_label_cached_encode_stats();
    case.metrics.insert(String::from("wrapped_label_variants"), WRAPPED_LABEL_VARIANTS as f64);
    case.metrics.insert(String::from("atlas_create_calls"), stats.creates as f64);
    case.metrics.insert(String::from("atlas_update_calls"), stats.updates as f64);
    case.metrics.insert(String::from("dirty_upload_pixels"), stats.dirty_upload_pixels as f64);
    case.metrics.insert(String::from("full_upload_pixels"), stats.full_upload_pixels as f64);
    case.metrics.insert(String::from("max_dirty_update_pixels"), stats.max_update_pixels as f64);
    case.metrics.insert(String::from("wrapped_label_glyph_runs"), stats.glyph_runs as f64);
    case.metrics.insert(String::from("wrapped_label_vertices"), stats.vertices as f64);
    case.metrics.insert(String::from("wrapped_label_indices"), stats.indices as f64);
    let ratio = if stats.full_upload_pixels == 0 {
        0.0
    } else {
        stats.dirty_upload_pixels as f64 / stats.full_upload_pixels as f64
    };
    case.metrics.insert(String::from("dirty_to_full_upload_ratio"), ratio);
    case
}

fn run_wrapped_label_cached_encode_stats() -> TextAtlasUploadStats {
    let mut bench = WrappedLabelEncodeBench::new();
    let mut checksum = 0u64;
    for _ in 0..32 {
        checksum = checksum.wrapping_add(bench.encode_cached_once());
    }
    let dl = bench.builder.drawlist();
    let glyph_runs =
        dl.items.iter().filter(|cmd| matches!(cmd, api::DrawCmd::GlyphRun { .. })).count() as u64;
    let mut stats = bench.uploader.stats;
    stats.glyph_runs = glyph_runs;
    stats.vertices = dl.vertices.len() as u64;
    stats.indices = dl.indices.len() as u64;
    stats.checksum =
        checksum.wrapping_add(stats.dirty_upload_pixels).wrapping_add(stats.full_upload_pixels);
    stats
}

struct PickerTextEncodeBench {
    picker: ui::elements::PickerState,
    style: ui::elements::PickerStyle,
    rect: api::RectF,
    text_ctx: ui::elements::TextCtx,
    uploader: CountingTextUploader,
    builder: ui::DrawListBuilder,
}

impl PickerTextEncodeBench {
    fn new() -> Self {
        let items: Vec<String> = [
            "Alpine", "Brass", "Cerulean", "Dawn", "Emerald", "Frost", "Graphite", "Harbor",
            "Ivory",
        ]
        .iter()
        .map(|value| (*value).to_string())
        .collect();
        Self {
            picker: ui::elements::PickerState::new(items),
            style: ui::elements::PickerStyle { font_id: 0, ..ui::elements::PickerStyle::default() },
            rect: api::RectF::new(0.0, 0.0, 240.0, 180.0),
            text_ctx: perf_text_ctx(),
            uploader: CountingTextUploader::default(),
            builder: ui::DrawListBuilder::new(),
        }
    }

    fn encode_cached_once(&mut self) -> u64 {
        self.builder.clear();
        self.picker.encode(
            &self.style,
            self.rect,
            PERF_DEVICE_SCALE,
            &mut self.text_ctx,
            &mut self.uploader,
            &mut self.builder,
        );
        let dl = self.builder.drawlist();
        dl.items.len() as u64
            + dl.vertices.len() as u64
            + dl.indices.len() as u64
            + self.text_ctx.atlas_revision()
    }
}

fn picker_text_cached_encode_case(smoke: bool, text_loops: u64) -> PerfCaseResult {
    let mut bench = PickerTextEncodeBench::new();
    let mut case = measure_cpu_case(
        "cpu.system.picker_text_cached_encode",
        "system",
        smoke,
        true,
        0.12,
        text_loops,
        vec![String::from(
            "Visible picker label encoding through TextCtx cached shaped runs and dirty atlas publication.",
        )],
        move || bench.encode_cached_once(),
    );
    let stats = run_picker_text_cached_encode_stats();
    case.metrics.insert(String::from("atlas_create_calls"), stats.creates as f64);
    case.metrics.insert(String::from("atlas_update_calls"), stats.updates as f64);
    case.metrics.insert(String::from("dirty_upload_pixels"), stats.dirty_upload_pixels as f64);
    case.metrics.insert(String::from("full_upload_pixels"), stats.full_upload_pixels as f64);
    case.metrics.insert(String::from("max_dirty_update_pixels"), stats.max_update_pixels as f64);
    case.metrics.insert(String::from("atlas_row_bytes"), stats.row_bytes as f64);
    case.metrics.insert(String::from("picker_glyph_runs"), stats.glyph_runs as f64);
    case.metrics.insert(String::from("picker_vertices"), stats.vertices as f64);
    case.metrics.insert(String::from("picker_indices"), stats.indices as f64);
    let ratio = if stats.full_upload_pixels == 0 {
        0.0
    } else {
        stats.dirty_upload_pixels as f64 / stats.full_upload_pixels as f64
    };
    case.metrics.insert(String::from("dirty_to_full_upload_ratio"), ratio);
    case
}

fn run_picker_text_cached_encode_stats() -> TextAtlasUploadStats {
    let mut bench = PickerTextEncodeBench::new();
    let cold_checksum = bench.encode_cached_once();
    let warm_checksum = bench.encode_cached_once();
    let dl = bench.builder.drawlist();
    let glyph_runs =
        dl.items.iter().filter(|cmd| matches!(cmd, api::DrawCmd::GlyphRun { .. })).count() as u64;
    let mut stats = bench.uploader.stats;
    stats.glyph_runs = glyph_runs;
    stats.vertices = dl.vertices.len() as u64;
    stats.indices = dl.indices.len() as u64;
    stats.checksum = cold_checksum
        .wrapping_add(warm_checksum)
        .wrapping_add(stats.dirty_upload_pixels)
        .wrapping_add(stats.full_upload_pixels);
    stats
}

fn touch(
    id: platform::TouchId,
    phase: platform::TouchPhase,
    x: f32,
    y: f32,
) -> platform::TouchEvent {
    platform::TouchEvent {
        id,
        phase,
        timestamp_ns: 0,
        x,
        y,
        pressure: None,
        tilt: None,
        device: platform::PointerDevice::Finger,
    }
}

fn gen_checker_rgba(w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let c = if ((x / 16) + (y / 16)) % 2 == 0 { 220 } else { 180 };
            out[i] = c;
            out[i + 1] = c;
            out[i + 2] = c;
            out[i + 3] = 255;
        }
    }
    out
}

fn run_legacy_runner() -> Result<()> {
    std::env::set_var("OXIDE_ENABLE_DAMAGE", "1");

    let mut renderer = metal::MetalRenderer::new_default().context("creating Metal renderer")?;
    let (w, h, scale) = (1200u32, 800u32, 2.0f32);
    renderer.resize(w, h, scale).context("resizing Metal renderer")?;
    let mut boxed = Box::new(renderer);
    let ptr: *mut metal::MetalRenderer = &mut *boxed;
    let uploader = MetalUploader { renderer: ptr };
    let mut router = scenes::Router::new(uploader);
    load_perf_fonts(&mut router.text);
    let checker = gen_checker_rgba(512, 512);
    let tex = unsafe { (*ptr).image_create_rgba8(512, 512, &checker, 512 * 4) };
    router.set_zoom_image(tex, 512, 512);
    let vp = api::RectF::new(0.0, 0.0, (w as f32) / scale, (h as f32) / scale);
    run_legacy_scene(&mut boxed, &mut router, 0, 120, vp, scale)?;
    run_legacy_scene(&mut boxed, &mut router, 3, 120, vp, scale)?;
    run_legacy_scene(&mut boxed, &mut router, 4, 120, vp, scale)?;
    run_legacy_scene(&mut boxed, &mut router, 2, 120, vp, scale)?;
    let stats = boxed.last_stats();
    println!(
        "enc_ms={:.2} draws={} inst={} culled={} dmg%={:.0}",
        stats.encode_ms,
        stats.draws,
        stats.instanced,
        stats.culled,
        (stats.damage_pct * 100.0).round()
    );
    Ok(())
}

fn run_legacy_scene(
    renderer: &mut metal::MetalRenderer,
    router: &mut scenes::Router<MetalUploader>,
    scene_index: usize,
    frames: usize,
    vp: api::RectF,
    scale: f32,
) -> Result<()> {
    let mut builder = ui::DrawListBuilder::new();
    router.set_scene(scene_index);
    for _ in 0..frames {
        builder.clear();
        let now = timing::now_ms();
        router.update(now, 16);
        router.draw(vp, scale, &mut builder);
        ui::coalesce_adjacent_draws(builder.drawlist_mut());
        let damage = api::Damage { rects: router.take_damage() };
        let token = renderer.begin_frame(&api::FrameTarget, Some(&damage));
        renderer.encode_pass(builder.drawlist());
        renderer.submit(token).context("submitting legacy Metal frame")?;
    }
    Ok(())
}
