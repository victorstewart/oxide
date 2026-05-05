use anyhow::{bail, Context, Result};
use oxide_harness_registry as registry;
use oxide_input::{GestureRecognizer, TouchSurfaceRecognizer};
use oxide_permissions as permissions;
use oxide_platform_api as platform;
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
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

const DEFAULT_BASELINE_JSON: &str = "benchmarks/workspace/latest.json";
const DEFAULT_BASELINE_MARKDOWN: &str = "benchmarks/workspace/latest.md";
const LATIN_FONT: &[u8] = include_bytes!("../../text/tests/fixtures/test_text_latin.ttf");
const CJK_FONT: &[u8] = include_bytes!("../../text/tests/fixtures/test_text_cjk.ttf");
const DAMAGE_USE_THRESH: f32 = 0.75;
const DAMAGE_PREFILTER_THRESH: f32 = 0.25;
const PERF_DEVICE_SCALE: f32 = 2.0;
const PERF_SCENE_W: u32 = 1200;
const PERF_SCENE_H: u32 = 800;
const PERF_RUNNER_FILTER_ENV: &str = "OXIDE_PERF_RUNNER_FILTER";

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

fn perf_case_filters() -> Vec<String> {
    std::env::var(PERF_RUNNER_FILTER_ENV)
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn perf_case_allowed(case_id: &str) -> bool {
    let filters = perf_case_filters();
    filters.is_empty()
        || filters.iter().any(|filter| case_id == filter || case_id.starts_with(filter))
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
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<f32>().ok())
        .unwrap_or(default)
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

const POPUP_WHEEL_PICKER_CASE_ID: &str = "cpu.authoring.popup_wheel_picker.interaction";

const PERF_AUTHORING_SPECS: &[AuthoringPerfSpec] = &[
    AuthoringPerfSpec { id: "cpu.authoring.text_fields.edit_cycle", name: "Text Fields" },
    AuthoringPerfSpec { id: POPUP_WHEEL_PICKER_CASE_ID, name: "Popup Wheel Picker" },
    AuthoringPerfSpec { id: "cpu.authoring.burst_emitter.sample", name: "Burst Emitter" },
    AuthoringPerfSpec {
        id: "cpu.authoring.surface_router.compose",
        name: "Surface Router Composition",
    },
    AuthoringPerfSpec { id: "gpu.authoring.scene3d.mixed_frame", name: "Scene3D Mixed Frame" },
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
}

struct GridMeasure;
struct GridRender;
struct FeedMeasure;
struct FeedRender;
struct ChatMeasure;
struct ChatRender;

impl ui::collection::Measure for GridMeasure {
    fn measure(&mut self, index: usize, constraint: f32) -> f32 {
        let wobble = (index % 7) as f32 * 2.0;
        (constraint * 0.55 + wobble).max(24.0)
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
}

fn run_suite(cli: Cli) -> Result<()> {
    let report = collect_suite(cli.smoke)?;
    if perf_case_filters().is_empty() {
        assert_full_coverage(&report.coverage)?;
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
        write_markdown(path, &report, comparison.as_ref())?;
        write_dated_markdown(path, &report, comparison.as_ref())?;
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

    if perf_case_prefix_allowed("cpu.system.") {
        push_system_cases(&mut cases, smoke);
    }
    if perf_case_prefix_allowed("cpu.component.") {
        push_component_cases(&mut cases, smoke, &mut covered_components);
    }
    if perf_case_prefix_allowed("cpu.animation.") {
        push_animation_cases(&mut cases, smoke, &mut covered_animations);
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
      AuditFinding {
         status: String::from("candidate"),
         summary: String::from(
            "Label wrapping still re-shapes tentative strings per word and clones intermediate Strings, which is likely the next CPU hotspot for text-heavy wrapped layouts.",
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
            if has("cpu.scene.") && has("gpu.scene.") && has("cpu.journey.") {
                "implemented"
            } else {
                "partial"
            },
            vec![String::from(
                "Flow coverage now spans offscreen launch/lifecycle, router scenes, and explicit user journeys, but hitch and device refresh-mode batteries are still incomplete.",
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
    let has_case = |needle: &str| cases.iter().any(|case| case.id == needle);
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
            ]),
            "Flat-grid rotation, deep-stack theme swap, and safe-area inset relayout batteries are all implemented.",
            "Dedicated relayout batteries now exist, but not every required flat/deep/grid invalidation slice is present yet.",
        ),
        contract_battery_entry(
            "text-input",
            "Text & Text Input",
            has_all(&[
                "cpu.text_input.large_editor.keystroke_burst",
                "cpu.text_input.large_editor.paste_10kb",
                "cpu.text_input.large_editor.selection_replace",
            ]),
            "Large-editor keystroke, paste, and selection-replace workloads now complement the existing text-field and input-form coverage.",
            "Text fields, wrapped labels, and the input-form journey are covered, but the full large-editor typing, paste, and selection battery is still incomplete.",
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
            "partial",
            vec![String::from(
                "Representative animations exist, but there is no dedicated hitch-ratio or refresh-mode matrix yet for 60 Hz versus native refresh.",
            )],
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

fn push_system_cases(cases: &mut Vec<PerfCaseResult>, smoke: bool) {
    let prepare_template = build_prepare_drawlist();
    let prepare_template_legacy = build_prepare_drawlist();
    let coalesce_template = build_coalesce_items();
    let coalesce_template_legacy = coalesce_template.clone();
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

    if perf_case_allowed("cpu.system.prepare_draws.legacy") {
        cases.push(measure_cpu_case(
            "cpu.system.prepare_draws.legacy",
            "audit-baseline",
            smoke,
            false,
            0.0,
            prepare_loops,
            vec![String::from("Legacy ClipPop recompute path kept for A/B audit context.")],
            move || legacy_prepare_draws(&prepare_template_legacy).len() as u64,
        ));
    }

    if perf_case_allowed("cpu.system.coalesce_adjacent_draws.current") {
        cases.push(measure_cpu_case(
            "cpu.system.coalesce_adjacent_draws.current",
            "system",
            smoke,
            true,
            0.12,
            coalesce_loops,
            vec![String::from("Current linear ui-core merge path.")],
            move || {
                let mut list =
                    api::DrawList { items: coalesce_template.clone(), ..api::DrawList::default() };
                ui::coalesce_adjacent_draws(&mut list);
                list.items.len() as u64
            },
        ));
    }

    if perf_case_allowed("cpu.system.coalesce_adjacent_draws.legacy") {
        cases.push(measure_cpu_case(
            "cpu.system.coalesce_adjacent_draws.legacy",
            "audit-baseline",
            smoke,
            false,
            0.0,
            coalesce_loops,
            vec![String::from("Legacy Vec::remove merge path kept for A/B audit context.")],
            move || {
                let mut list = api::DrawList {
                    items: coalesce_template_legacy.clone(),
                    ..api::DrawList::default()
                };
                legacy_coalesce_adjacent_draws(&mut list);
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
            "gpu.authoring.scene3d.mixed_frame" => authoring_scene3d_mixed_frame_case(smoke)?,
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

    let mut renderer =
        Box::new(metal::MetalRenderer::new_default().context("creating Metal renderer")?);
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
    let mut draws_sum = 0.0f64;
    let mut instanced_sum = 0.0f64;
    let mut culled_sum = 0.0f64;
    let mut damage_sum = 0.0f64;

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
        renderer.encode_pass(builder.drawlist());
        renderer
            .submit(token)
            .with_context(|| format!("submitting Metal frame for gpu.scene.{}.frame", spec.slug))?;
        let stats = renderer.last_stats();
        if index >= warmups {
            frame_samples.push(frame_t0.elapsed().as_secs_f64() * 1000.0);
            draw_samples.push(draw_ms);
            encode_samples.push(stats.encode_ms);
            draws_sum += stats.draws as f64;
            instanced_sum += stats.instanced as f64;
            culled_sum += stats.culled as f64;
            damage_sum += stats.damage_pct as f64 * 100.0;
        }
    }

    let summary = summarize(&frame_samples);
    let mut metrics = BTreeMap::new();
    metrics.insert(String::from("draw_ms_median"), summarize(&draw_samples).median);
    metrics.insert(String::from("encode_ms_median"), summarize(&encode_samples).median);
    metrics.insert(String::from("draws_avg"), draws_sum / frames as f64);
    metrics.insert(String::from("instanced_avg"), instanced_sum / frames as f64);
    metrics.insert(String::from("culled_avg"), culled_sum / frames as f64);
    metrics.insert(String::from("damage_pct_avg"), damage_sum / frames as f64);
    metrics.insert(
        String::from("damage_enabled"),
        if damage_enabled { 1.0 } else { 0.0 },
    );
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
    measure_cpu_case(
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
    )
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
    let mut draws_sum = 0.0;

    for index in 0..(warmups + frames) {
        let frame_t0 = Instant::now();
        let token = renderer.begin_frame(&api::FrameTarget, None);
        renderer
            .encode_scene3d(&scene)
            .with_context(|| "encoding authoring scene3d mixed frame")?;
        renderer.encode_pass(&overlay);
        renderer.submit(token).with_context(|| "submitting authoring scene3d mixed frame")?;
        let stats = renderer.last_stats();
        if index >= warmups {
            frame_samples.push(frame_t0.elapsed().as_secs_f64() * 1000.0);
            encode_samples.push(stats.encode_ms);
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
        renderer.encode_pass(builder.drawlist());
        renderer.submit(token).context("submitting image first-visible frame")?;
        let stats = renderer.last_stats();
        renderer.image_release(handle);
        frame_samples.push(frame_t0.elapsed().as_secs_f64() * 1000.0);
        encode_samples.push(stats.encode_ms);
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
    mut measure: M,
    mut render: R,
) -> Result<PerfCaseResult>
where
    M: ui::collection::Measure,
    R: ui::collection::CellRenderer,
{
    measure_journey_case(
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
    )
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

fn summarize(samples: &[f64]) -> SampleSummary {
    assert!(!samples.is_empty(), "perf suites must record at least one sample");
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sum = samples.iter().copied().sum::<f64>();
    SampleSummary {
        min: *sorted.first().unwrap_or(&0.0),
        max: *sorted.last().unwrap_or(&0.0),
        mean: sum / samples.len() as f64,
        median: quantile(&sorted, 0.5),
        p95: quantile(&sorted, 0.95),
        p99: quantile(&sorted, 0.99),
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
    let baseline_map =
        baseline.cases.iter().map(|case| (case.id.as_str(), case)).collect::<BTreeMap<_, _>>();

    let mut comparison = PerfComparison::default();
    for case in &current.cases {
        if !case.gated {
            continue;
        }
        let Some(base) = baseline_map.get(case.id.as_str()) else {
            comparison.missing_baseline.push(case.id.clone());
            continue;
        };
        comparison.matched += 1;
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

fn load_report(path: &Path) -> Result<PerfReport> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

fn write_report_json(path: &Path, report: &PerfReport) -> Result<()> {
    ensure_parent(path)?;
    let body = serde_json::to_vec_pretty(report)?;
    fs::write(path, body).with_context(|| format!("writing {}", path.display()))
}

fn write_markdown(
    path: &Path,
    report: &PerfReport,
    comparison: Option<&PerfComparison>,
) -> Result<()> {
    ensure_parent(path)?;
    let body = render_markdown(report, comparison);
    fs::write(path, body).with_context(|| format!("writing {}", path.display()))
}

fn write_dated_markdown(
    latest_path: &Path,
    report: &PerfReport,
    comparison: Option<&PerfComparison>,
) -> Result<()> {
    let Some(date_label) = report.generated_label.as_ref() else {
        return Ok(());
    };
    let Some(parent) = latest_path.parent() else {
        return Ok(());
    };
    let dated = parent.join(format!("{}.md", date_label));
    ensure_parent(&dated)?;
    let body = render_markdown(report, comparison);
    fs::write(&dated, body).with_context(|| format!("writing {}", dated.display()))
}

fn render_markdown(report: &PerfReport, comparison: Option<&PerfComparison>) -> String {
    let mut out = String::new();
    out.push_str("# Oxide Performance Report\n\n");
    out.push_str(&format!("- Suite: `{}`\n", report.suite));
    if let Some(label) = report.generated_label.as_ref() {
        out.push_str(&format!("- Label: `{}`\n", label));
    }
    out.push_str(&format!(
        "- Coverage: {}/{} components, {}/{} animations, {}/{} launch cases, {}/{} primitive lifecycle cases, {}/{} CPU scenes, {}/{} GPU scenes, {}/{} journeys, {}/{} authoring APIs, {}/{} image pipeline cases, {}/{} navigation cases, {}/{} reconcile cases, {}/{} bridge paths\n",
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
        report.coverage.image_pipeline_covered.len(),
        report.coverage.image_pipeline_total,
        report.coverage.navigation_covered.len(),
        report.coverage.navigation_total,
        report.coverage.reconcile_covered.len(),
        report.coverage.reconcile_total,
        report.coverage.bridges_covered.len(),
        report.coverage.bridges_total
    ));
    if let Some(comp) = comparison {
        out.push_str(&format!(
            "- Comparison: `{}` matched, `{}` regressions, `{}` missing baseline cases\n",
            comp.matched,
            comp.regressions.len(),
            comp.missing_baseline.len()
        ));
    }
    out.push('\n');

    out.push_str("## Contract Coverage\n\n");
    out.push_str("| Section | Status | Notes |\n");
    out.push_str("| --- | --- | --- |\n");
    for entry in &report.contract.layers {
        out.push_str(&format!(
            "| `{}` | `{}` | {} |\n",
            entry.label,
            entry.status,
            entry.notes.join(" ")
        ));
    }
    for entry in &report.contract.battery {
        out.push_str(&format!(
            "| `{}` | `{}` | {} |\n",
            entry.label,
            entry.status,
            entry.notes.join(" ")
        ));
    }
    if !report.contract.notes.is_empty() {
        out.push_str("\n");
        for note in &report.contract.notes {
            out.push_str(&format!("- {}\n", note));
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
        out.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} | {} |\n",
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
            gate,
            render_case_metrics_summary(&case.metrics)
        ));
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
            out.push_str(&format!(
            "- Regression: `{}` median {:.3} vs baseline {:.3} (allowed {:.3}, delta {:+.2}%)\n",
            reg.id, reg.current_median, reg.baseline_median, reg.allowed_median, reg.delta_pct
         ));
        }
        for missing in &comp.missing_baseline {
            out.push_str(&format!("- Missing baseline case: `{}`\n", missing));
        }
    }

    out.push_str("\n## Findings\n\n");
    for finding in &report.findings {
        out.push_str(&format!("- [{}] {}\n", finding.status, finding.summary));
    }

    out.push_str("\n## Baseline Workflow\n\n");
    out.push_str("- Update the committed baseline only with review: `PERF_REPORT_DATE=$(date +%F) cargo run --release -j$(sysctl -n hw.ncpu) -p oxide-perf-runner -- --run-suite --write-baseline`\n");
    out.push_str(&format!("- Latest JSON baseline: `{}`\n", DEFAULT_BASELINE_JSON));
    out.push_str(&format!("- Latest Markdown baseline: `{}`\n", DEFAULT_BASELINE_MARKDOWN));
    out
}

fn render_case_metrics_summary(metrics: &BTreeMap<String, f64>) -> String {
    if metrics.is_empty() {
        return String::from("`-`");
    }
    let parts = metrics
        .iter()
        .take(4)
        .map(|(name, value)| format!("{}={:.3}", name, value))
        .collect::<Vec<_>>();
    format!("`{}`", parts.join("; "))
}

fn compute_audit_speedups(report: &PerfReport) -> Vec<(String, f64)> {
    let map = report.cases.iter().map(|case| (case.id.as_str(), case)).collect::<BTreeMap<_, _>>();
    let mut out = Vec::new();
    if let (Some(current), Some(legacy)) =
        (map.get("cpu.system.prepare_draws.current"), map.get("cpu.system.prepare_draws.legacy"))
    {
        if current.median > 0.0 {
            out.push((String::from("prepare_draws"), legacy.median / current.median));
        }
    }
    if let (Some(current), Some(legacy)) = (
        map.get("cpu.system.coalesce_adjacent_draws.current"),
        map.get("cpu.system.coalesce_adjacent_draws.legacy"),
    ) {
        if current.median > 0.0 {
            out.push((String::from("coalesce_adjacent_draws"), legacy.median / current.median));
        }
    }
    out
}

fn print_summary(report: &PerfReport, comparison: Option<&PerfComparison>) {
    println!(
        "suite={} cases={} components={}/{} animations={}/{} launch={}/{} primitive_lifecycle={}/{} scenes_cpu={}/{} scenes_gpu={}/{} journeys={}/{} authoring={}/{} image_pipeline={}/{} navigation={}/{} reconcile={}/{} bridges={}/{}",
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
        report.coverage.image_pipeline_covered.len(),
        report.coverage.image_pipeline_total,
        report.coverage.navigation_covered.len(),
        report.coverage.navigation_total,
        report.coverage.reconcile_covered.len(),
        report.coverage.reconcile_total,
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

fn touch(
    id: platform::TouchId,
    phase: platform::TouchPhase,
    x: f32,
    y: f32,
) -> platform::TouchEvent {
    platform::TouchEvent {
        id,
        phase,
        x,
        y,
        pressure: None,
        tilt: None,
        device: platform::PointerDevice::Finger,
    }
}

fn legacy_prepare_draws(list: &api::DrawList) -> Vec<ui::PreparedDraw> {
    use api::DrawCmd as C;
    let mut out = Vec::with_capacity(list.items.len());
    let mut stack = Vec::new();
    let mut current: Option<api::RectI> = None;
    for item in &list.items {
        match *item {
            C::ClipPush { rect } => {
                current = Some(if let Some(cur) = current {
                    intersect_rect(cur, rect).unwrap_or(api::RectI { x: 0, y: 0, w: 0, h: 0 })
                } else {
                    rect
                });
                stack.push(rect);
            }
            C::ClipPop => {
                let _ = stack.pop();
                current = if stack.is_empty() {
                    None
                } else {
                    let mut it = stack.iter();
                    let mut acc = *it.next().expect("stack non-empty");
                    for rect in it {
                        if let Some(next) = intersect_rect(acc, *rect) {
                            acc = next;
                        } else {
                            acc = api::RectI { x: 0, y: 0, w: 0, h: 0 };
                            break;
                        }
                    }
                    Some(acc)
                };
            }
            _ => {
                out.push(ui::PreparedDraw {
                    cmd: item.clone(),
                    clip: current.filter(|rect| rect.w > 0 && rect.h > 0),
                });
            }
        }
    }
    out
}

fn legacy_coalesce_adjacent_draws(list: &mut api::DrawList) {
    use api::DrawCmd as C;

    #[inline]
    fn contiguous(a_off: u32, a_len: u32, b_off: u32) -> bool {
        a_off.saturating_add(a_len) == b_off
    }

    #[inline]
    fn mergeable_nonindexed_solid(vb: api::VertexSpan) -> bool {
        vb.len >= 3 && vb.len % 3 == 0
    }

    let mut i = 0usize;
    while i + 1 < list.items.len() {
        let can_merge = match (&list.items[i], &list.items[i + 1]) {
            (C::GlyphRun { .. }, C::GlyphRun { .. }) => false,
            (C::Solid { vb: av, ib: ai, color: ac }, C::Solid { vb: bv, ib: bi, color: bc }) => {
                if ac != bc
                    || !contiguous(av.offset, av.len, bv.offset)
                    || !contiguous(ai.offset, ai.len, bi.offset)
                {
                    false
                } else if ai.len == 0 && bi.len == 0 {
                    mergeable_nonindexed_solid(*av) && mergeable_nonindexed_solid(*bv)
                } else {
                    ai.len > 0 && bi.len > 0
                }
            }
            _ => false,
        };
        if can_merge {
            let (left_slice, right_slice) = list.items.split_at_mut(i + 1);
            let left = &mut left_slice[i];
            let right = &mut right_slice[0];
            if let (C::Solid { vb: av, ib: ai, .. }, C::Solid { vb: bv, ib: bi, .. }) =
                (left, right)
            {
                av.len += bv.len;
                ai.len += bi.len;
            }
            list.items.remove(i + 1);
        } else {
            i += 1;
        }
    }
}

fn intersect_rect(a: api::RectI, b: api::RectI) -> Option<api::RectI> {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.w).min(b.x + b.w);
    let y2 = (a.y + a.h).min(b.y + b.h);
    let w = x2 - x1;
    let h = y2 - y1;
    if w > 0 && h > 0 {
        Some(api::RectI { x: x1, y: y1, w, h })
    } else {
        None
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
