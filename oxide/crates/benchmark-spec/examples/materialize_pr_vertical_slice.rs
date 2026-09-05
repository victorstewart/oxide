use anyhow::{ensure, Context, Result};
use base64::Engine;
use oxide_benchmark_spec::{
   canonical_apple_pr_plan_json, canonical_scenario_json, validate_font_pack_manifest,
   ApplePrPlanSpec, ArtifactIdentity, AssetFile, AssetManifest, ChatFixture, ChatMessage,
   ChatSelectionReplacement, DashboardCategories, EnduranceFixture, FairnessContract,
   FontPackIdentity, FontPackManifest, ImageDecodeZoomFixture, ImageFileFixture,
   ParityCheckpoint, RoleCount, ScenarioPhase, ScenarioSpec, SceneContract, StartupCard,
   StartupFixture, TraceEvent, TraceOperation, TraceValue,
};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

const WIDTH: u32 = 1_170;
const HEIGHT: u32 = 2_532;
const FONT_PACK_PATH: &str = "font-packs/oxide-bench-fonts-v1.json";
const APPLE_PR_PLAN_PATH: &str = "plans/apple-pr.json";
const INLINE_TEXT_ATLAS_PATH: &str = "assets/inline-text-atlas-v1.png";
const INLINE_TEXT_ATLAS_45_PATH: &str = "assets/inline-text-atlas-v1-45px.png";
const INLINE_TEXT_ATLAS_39_PATH: &str = "assets/inline-text-atlas-v1-39px.png";
const INLINE_TEXT_ATLAS_SOURCE_PATH: &str = "assets/source/inline-text-atlas-v1.png.b64";
const INLINE_TEXT_LICENSE_PATH: &str = "assets/source/OFL-noto-emoji.txt";

fn main() -> Result<()>
{
   let root = parse_root()?;
   ensure!(root.is_dir(), "materialization root does not exist: {}", root.display());
   ensure!(root.join(FONT_PACK_PATH).is_file(), "pinned font-pack manifest is missing under {}", root.display());

   let shared = materialize_shared(&root)?;
   let dashboard = materialize_dashboard(&root, &shared)?;
   let endurance = materialize_endurance(&root, &shared)?;
   let feed = materialize_feed(&root, &shared)?;
   let navigation = materialize_navigation(&root, &shared)?;
   let startup = materialize_startup(&root, &shared)?;
   let chat = materialize_chat(&root, &shared)?;
   let image = materialize_image(&root, &shared)?;
   refresh_apple_pr_plan(&root)?;

   println!("materialized {}", startup.path);
   println!("materialized {}", dashboard.path);
   println!("materialized {}", endurance.path);
   println!("materialized {}", feed.path);
   println!("materialized {}", chat.path);
   println!("materialized {}", navigation.path);
   println!("materialized {}", image.path);
   println!("materialized {}", APPLE_PR_PLAN_PATH);
   Ok(())
}

fn parse_root() -> Result<PathBuf>
{
   let mut args = std::env::args().skip(1);
   ensure!(args.next().as_deref() == Some("--root"), "usage: materialize_pr_vertical_slice --root PATH");
   let root = args.next().context("--root requires a path")?;
   ensure!(args.next().is_none(), "unexpected materializer arguments");
   fs::canonicalize(&root).with_context(|| format!("canonicalizing materialization root {}", root))
}

struct SharedArtifacts
{
   assets: ArtifactIdentity,
   font_pack: FontPackIdentity,
   style: ArtifactIdentity,
}

fn materialize_shared(root: &Path) -> Result<SharedArtifacts>
{
   let font_pack_bytes = fs::read(root.join(FONT_PACK_PATH)).with_context(|| format!("reading pinned font-pack manifest {}", root.join(FONT_PACK_PATH).display()))?;
   let font_pack_manifest = serde_json::from_slice::<FontPackManifest>(&font_pack_bytes).context("parsing pinned font-pack manifest")?;
   validate_font_pack_manifest(root, &font_pack_manifest, "oxide-bench-fonts-v1").context("validating pinned font-pack axes and artifacts")?;
   let atlas_path = "assets/neutral-thumbnail-atlas-v1.png";
   write_atlas_png(&root.join(atlas_path))?;
   let atlas = identity(root, atlas_path)?;
   write_base64_asset(root, INLINE_TEXT_ATLAS_SOURCE_PATH, INLINE_TEXT_ATLAS_PATH)?;
   write_inline_text_raster_variant(&root.join(INLINE_TEXT_ATLAS_PATH), &root.join(INLINE_TEXT_ATLAS_45_PATH), 45)?;
   write_inline_text_raster_variant(&root.join(INLINE_TEXT_ATLAS_PATH), &root.join(INLINE_TEXT_ATLAS_39_PATH), 39)?;
   let inline_text_atlas = identity(root, INLINE_TEXT_ATLAS_PATH)?;
   let inline_text_atlas_45 = identity(root, INLINE_TEXT_ATLAS_45_PATH)?;
   let inline_text_atlas_39 = identity(root, INLINE_TEXT_ATLAS_39_PATH)?;
   let inline_text_license = identity(root, INLINE_TEXT_LICENSE_PATH)?;
   let assets = write_json(root, "assets/neutral-v1.json", &json!({
      "schema_version": 1,
      "id": "neutral-assets-v1",
      "thumbnail_atlas": atlas,
      "artifacts": [
         {
            "role": "thumbnail-atlas",
            "artifact": atlas,
            "media_type": "image/png",
            "color_space": "srgb"
         },
         {
            "role": "inline-text-atlas-128",
            "artifact": inline_text_atlas,
            "media_type": "image/png",
            "color_space": "srgb"
         },
         {
            "role": "inline-text-atlas-45",
            "artifact": inline_text_atlas_45,
            "media_type": "image/png",
            "color_space": "srgb"
         },
         {
            "role": "inline-text-atlas-39",
            "artifact": inline_text_atlas_39,
            "media_type": "image/png",
            "color_space": "srgb"
         }
      ],
      "inline_text_atlas": {
         "columns": 5,
         "rows": 2,
         "source_repository": "https://github.com/googlefonts/noto-emoji",
         "source_commit": "8998f5dd683424a73e2314a8c1f1e359c19e8742",
         "license": inline_text_license,
         "variants": [
            {"artifact_role": "inline-text-atlas-128", "pixel_width": 640, "pixel_height": 256, "em_pixels": 128},
            {"artifact_role": "inline-text-atlas-45", "pixel_width": 225, "pixel_height": 90, "em_pixels": 45},
            {"artifact_role": "inline-text-atlas-39", "pixel_width": 195, "pixel_height": 78, "em_pixels": 39}
         ],
         "entries": [
            inline_text_entry("●", 0),
            inline_text_entry("◆", 1),
            inline_text_entry("★", 2),
            inline_text_entry("☺", 3),
            inline_text_entry("👩🏽‍💻", 4),
            inline_text_entry("🌍", 5),
            inline_text_entry("✨", 6),
            inline_text_entry("👨‍👩‍👧‍👦", 7),
            inline_text_entry("🇺🇳", 8),
            inline_text_entry("🇯🇵", 9)
         ]
      },
      "tile_width": 24,
      "tile_height": 24,
      "columns": 16,
      "rows": 8,
      "tile_count": 128,
      "sampling": "linear-clamp",
      "color_space": "srgb"
   }))?;
   let style = write_json(root, "styles/neutral-v1.json", &json!({
      "schema_version": 1,
      "id": "neutral-v1",
      "logical_viewport": {"width": 390, "height": 844, "scale": 3},
      "color_space": "srgb",
      "background": "#f3f5f8ff",
      "surface": "#ffffffff",
      "text": "#20242cff",
      "secondary_text": "#697181ff",
      "accent": "#3d6eefff",
      "muted_surface": "#eef1f6ff",
      "rtl_surface": "#eaeef7ff",
      "modal_overlay": "#191c23b0",
      "backdrop_tint": "#e1e5ee38",
      "corner_curve": "circular",
      "corner_radius": 12,
      "shadow": {"offset_y": 2, "blur_radius": 0, "opacity": 0.16},
      "backdrop_blur_radius": 12,
      "font_size_body": 15,
      "font_size_heading": 20,
      "text_pixel_policy": "normalized-srgb8-exact-static-full-frame"
   }))?;
   let font_pack_artifact = identity(root, FONT_PACK_PATH)?;
   Ok(SharedArtifacts {
      assets,
      font_pack: FontPackIdentity {
         id: String::from("oxide-bench-fonts-v1"),
         manifest: String::from(FONT_PACK_PATH),
         sha256: font_pack_artifact.sha256,
      },
      style,
   })
}

struct MaterializedScenario
{
   path: String,
}

fn refresh_apple_pr_plan(root: &Path) -> Result<()>
{
   let path = root.join(APPLE_PR_PLAN_PATH);
   let bytes = fs::read(&path).with_context(|| format!("reading Apple PR plan {}", path.display()))?;
   let mut plan = serde_json::from_slice::<ApplePrPlanSpec>(&bytes).with_context(|| format!("parsing Apple PR plan {}", path.display()))?;
   for scenario in &mut plan.scenarios
   {
      scenario.artifact = identity(root, &scenario.artifact.path)?;
   }
   let bytes = canonical_apple_pr_plan_json(&plan).context("serializing canonical Apple PR plan")?;
   fs::write(&path, bytes).with_context(|| format!("writing Apple PR plan {}", path.display()))
}

fn materialize_startup(root: &Path, shared: &SharedArtifacts) -> Result<MaterializedScenario>
{
   let fixture = StartupFixture {
      schema_version: 1,
      id: String::from("startup.first-screen"),
      data_size_bytes: 24 * 1_024,
      data: (0..24 * 1_024).map(|index| (b'A' + (index % 26) as u8) as char).collect(),
      card_count: 24,
      cards: (0..24).map(|index| StartupCard {
         id: format!("startup:card:{index:02}"),
         data_offset: index * 1_024,
         data_length: 1_024,
         thumbnail_index: index % 6,
         initially_visible: index < 6,
      }).collect(),
      initial_image_indices: (0..6).collect(),
      header_id: String::from("startup:header"),
      navigation_id: String::from("startup:navigation"),
      control_id: String::from("startup:primary-control"),
   };
   let fixture = write_json(root, "fixtures/startup.first-screen.json", &fixture)?;
   let layout = write_json(root, "layout/startup.first-screen.json", &startup_layout())?;
   let terminated = write_trace(root, "traces/startup-terminated-warm-cache.json", vec![lifecycle_event(0, TraceOperation::Foreground, "terminated-warm-cache", "startup:launch-requested")])?;
   let fresh = write_trace(root, "traces/startup-fresh-install-first-launch.json", vec![lifecycle_event(0, TraceOperation::ResourceArrival, "fresh-install", "startup:fresh-install-ready")])?;
   let resume = write_trace(root, "traces/startup-warm-resume.json", vec![
      lifecycle_event(0, TraceOperation::Background, "warm-resume", "startup:backgrounded"),
      lifecycle_event(1, TraceOperation::Foreground, "warm-resume", "startup:resume-requested"),
   ])?;
   let counts = startup_counts();
   let checkpoints = vec![
      checkpoint(root, "startup.first-screen", "terminated-ready", "terminated-warm-cache", Some(0), &counts, Golden::Startup(0))?,
      checkpoint(root, "startup.first-screen", "fresh-install-ready", "fresh-install-first-launch", Some(0), &counts, Golden::Startup(0))?,
      checkpoint(root, "startup.first-screen", "warm-resume-ready", "warm-resume", Some(1), &counts, Golden::Startup(0))?,
   ];
   let scenario = ScenarioSpec {
      schema_version: 1,
      id: String::from("startup.first-screen"),
      fixture,
      assets: shared.assets.clone(),
      font_pack: shared.font_pack.clone(),
      viewport_class: String::from("phone-portrait"),
      scene: SceneContract {
         roles: vec!["header", "navigation", "card", "initial-image", "primary-control"].into_iter().map(String::from).collect(),
         style_tokens: shared.style.clone(),
         layout_assertions: layout,
      },
      phases: vec![
         phase("setup", false, None, None),
         phase("terminated-warm-cache", true, None, Some(terminated)),
         phase("fresh-install-first-launch", true, None, Some(fresh)),
         phase("warm-resume", true, None, Some(resume)),
         phase("teardown", false, None, None),
      ],
      primary_metric: String::from("controller_launch_request_to_first_attributed_present_proxy_ms"),
      required_metrics: required_metrics("controller_launch_request_to_first_attributed_present_proxy_ms"),
      optional_metrics: vec![String::from("startup.ready_probe_ms"), String::from("memory.peak_resident_bytes")],
      parity_checkpoints: checkpoints,
      fairness_contract: fairness("ltr", counts),
   };
   write_scenario(root, &scenario)
}

fn materialize_chat(root: &Path, shared: &SharedArtifacts) -> Result<MaterializedScenario>
{
   let fixture = ChatFixture {
      schema_version: 1,
      id: String::from("chat.live-update"),
      message_count: 5_000,
      avatar_count: 64,
      messages: (0..5_000).map(|index| chat_message(index, false)).collect(),
      prepend_messages: (0..50).map(|index| chat_message(index, true)).collect(),
      append_rate_hz: 10,
      typed_text: "abcdefghijklmnopqrstuvwxyz".chars().cycle().take(100).collect(),
      pasted_text: "0123456789abcdef".repeat(640),
      selection_replacement: ChatSelectionReplacement {
         message_id: String::from("chat:append:16"),
         start_utf8: 0,
         end_utf8: 6,
         replacement: String::from("Oxide"),
      },
   };
   let pasted_text = fixture.pasted_text.clone();
   let typed_text = fixture.typed_text.clone();
   let fixture = write_json(root, "fixtures/chat.live-update.json", &fixture)?;
   let layout = write_json(root, "layout/chat.live-update.json", &chat_layout())?;
   let prepend = write_trace(root, "traces/chat-prepend-50.json", vec![mutate_event(0, "chat:prepend-count", TraceValue::Integer(50), "chat:prepended-50")])?;
   let append = write_trace(root, "traces/chat-append-10hz.json", (0..20).map(|index| mutate_event(index * 100_000, "chat:append", TraceValue::Text(format!("chat:append:{index:02}")), &format!("chat:appended:{index:02}"))).collect())?;
   let typing = write_trace(root, "traces/chat-type-100.json", typed_text.chars().enumerate().map(|(index, value)| text_event(index as u64 * 15_000, TraceOperation::CommitText, "chat:composer", value.to_string(), &format!("chat:typed:{:03}", index + 1))).collect())?;
   let paste = write_trace(root, "traces/chat-paste-10kib.json", vec![text_event(0, TraceOperation::CommitText, "chat:composer", pasted_text, "chat:pasted-10kib")])?;
   let replace = write_trace(root, "traces/chat-select-replace.json", vec![
      text_event(0, TraceOperation::Focus, "chat:append:16", String::from("0:6"), "chat:selection-ready"),
      text_event(500_000, TraceOperation::CommitText, "chat:append:16", String::from("Oxide"), "chat:selection-replaced"),
   ])?;
   let counts = chat_counts();
   let checkpoints = vec![
      checkpoint(root, "chat.live-update", "initial", "setup", None, &counts, Golden::Chat(0))?,
      checkpoint(root, "chat.live-update", "prepended-50", "prepend-50", Some(0), &counts, Golden::Chat(1))?,
      checkpoint(root, "chat.live-update", "append-settled", "append-10hz", Some(1_900_000), &counts, Golden::Chat(2))?,
      checkpoint(root, "chat.live-update", "typed-100", "type-100", Some(1_485_000), &counts, Golden::Chat(3))?,
      checkpoint(root, "chat.live-update", "pasted-10kib", "paste-10kib", Some(0), &counts, Golden::Chat(4))?,
      checkpoint(root, "chat.live-update", "selection-replaced", "select-replace", Some(500_000), &counts, Golden::Chat(5))?,
   ];
   let scenario = ScenarioSpec {
      schema_version: 1,
      id: String::from("chat.live-update"),
      fixture,
      assets: shared.assets.clone(),
      font_pack: shared.font_pack.clone(),
      viewport_class: String::from("phone-portrait"),
      scene: SceneContract {
         roles: vec!["chat-thread", "message", "avatar", "composer", "send-control"].into_iter().map(String::from).collect(),
         style_tokens: shared.style.clone(),
         layout_assertions: layout,
      },
      phases: vec![
         phase("setup", false, None, None),
         phase("prewarm", false, Some(2_000), None),
         phase("prepend-50", true, Some(500), Some(prepend)),
         phase("append-10hz", true, Some(2_000), Some(append)),
         phase("type-100", true, Some(1_500), Some(typing)),
         phase("paste-10kib", true, Some(1_000), Some(paste)),
         phase("select-replace", true, Some(1_000), Some(replace)),
         phase("teardown", false, None, None),
      ],
      primary_metric: String::from("keystroke_to_attributed_presentation_ms"),
      required_metrics: required_metrics("keystroke_to_attributed_presentation_ms"),
      optional_metrics: vec![String::from("text.shape_ms"), String::from("memory.retained_slope_bytes_per_min")],
      parity_checkpoints: checkpoints,
      fairness_contract: fairness("mixed-ltr-rtl", counts),
   };
   write_scenario(root, &scenario)
}

fn materialize_image(root: &Path, shared: &SharedArtifacts) -> Result<MaterializedScenario>
{
   let source_path = "assets/image-decode-zoom-source-v1.png";
   let thumbnail_path = "assets/image-decode-zoom-thumbnail-v1.png";
   write_deterministic_image_png(&root.join(source_path), 4_096, 3_072)?;
   write_deterministic_image_png(&root.join(thumbnail_path), 384, 288)?;
   let source = ImageFileFixture {artifact: identity(root, source_path)?, width: 4_096, height: 3_072, format: String::from("png"), color_space: String::from("srgb")};
   let thumbnail = ImageFileFixture {artifact: identity(root, thumbnail_path)?, width: 384, height: 288, format: String::from("png"), color_space: String::from("srgb")};
   let assets = write_json(root, "assets/image-decode-zoom-v1.json", &AssetManifest {
      schema_version: 1,
      id: String::from("image-decode-zoom-assets-v1"),
      artifacts: vec![
         AssetFile {role: String::from("source-image"), artifact: source.artifact.clone(), media_type: String::from("image/png"), color_space: String::from("srgb")},
         AssetFile {role: String::from("thumbnail"), artifact: thumbnail.artifact.clone(), media_type: String::from("image/png"), color_space: String::from("srgb")},
      ],
      inline_text_atlas: None,
   })?;
   let fixture = write_json(root, "fixtures/image.decode-zoom.json", &ImageDecodeZoomFixture {
      schema_version: 1,
      id: String::from("image.decode-zoom"),
      source,
      thumbnail,
      pan_distance_millionths: 450_000,
      pinch_scale_millionths: 2_000_000,
   })?;
   let layout = write_json(root, "layout/image.decode-zoom.json", &image_layout())?;
   let bytes_ready = write_trace(root, "traces/image-bytes-ready.json", vec![lifecycle_event(0, TraceOperation::ResourceArrival, "image:source-bytes", "image:bytes-ready")])?;
   let decode = write_trace(root, "traces/image-decode.json", vec![lifecycle_event(0, TraceOperation::ResourceArrival, "image:decoded", "image:decode-complete")])?;
   let upload = write_trace(root, "traces/image-upload.json", vec![lifecycle_event(0, TraceOperation::ResourceArrival, "image:texture", "image:upload-complete")])?;
   let visible = write_trace(root, "traces/image-first-visible.json", vec![lifecycle_event(0, TraceOperation::ResourceArrival, "image:presented", "image:first-visible")])?;
   let pan = write_trace(root, "traces/image-pan.json", image_pan_trace())?;
   let pinch = write_trace(root, "traces/image-pinch.json", image_pinch_trace())?;
   let counts = image_counts();
   let checkpoints = vec![
      checkpoint(root, "image.decode-zoom", "thumbnail", "setup", None, &counts, Golden::Image(0))?,
      checkpoint(root, "image.decode-zoom", "first-visible", "first-visible", Some(0), &counts, Golden::Image(1))?,
      checkpoint(root, "image.decode-zoom", "pan-mid", "pan", Some(1_000_000), &counts, Golden::Image(2))?,
      checkpoint(root, "image.decode-zoom", "pinch-mid", "pinch", Some(1_000_000), &counts, Golden::Image(3))?,
   ];
   let scenario = ScenarioSpec {
      schema_version: 1,
      id: String::from("image.decode-zoom"),
      fixture,
      assets,
      font_pack: shared.font_pack.clone(),
      viewport_class: String::from("phone-portrait"),
      scene: SceneContract {
         roles: vec!["image-canvas", "image", "zoom-control"].into_iter().map(String::from).collect(),
         style_tokens: shared.style.clone(),
         layout_assertions: layout,
      },
      phases: vec![
         phase("setup", false, None, None),
         phase("prewarm", false, Some(2_000), None),
         phase("bytes-ready", true, Some(250), Some(bytes_ready)),
         phase("decode", true, Some(750), Some(decode)),
         phase("upload", true, Some(500), Some(upload)),
         phase("first-visible", true, Some(500), Some(visible)),
         phase("pan", true, Some(2_000), Some(pan)),
         phase("pinch", true, Some(2_000), Some(pinch)),
         phase("teardown", false, None, None),
      ],
      primary_metric: String::from("bytes_ready_to_first_visible_ms"),
      required_metrics: required_metrics("bytes_ready_to_first_visible_ms"),
      optional_metrics: vec![String::from("image.decode_ms"), String::from("texture.upload_ms"), String::from("texture.resident_bytes")],
      parity_checkpoints: checkpoints,
      fairness_contract: fairness("ltr", counts),
   };
   write_scenario(root, &scenario)
}

fn materialize_dashboard(root: &Path, shared: &SharedArtifacts) -> Result<MaterializedScenario>
{
   let fixture = write_json(root, "fixtures/dashboard.mixed-static.json", &json!({
      "schema_version": 1,
      "id": "dashboard.mixed-static",
      "visible_node_count": 300,
      "categories": {"label": 176, "icon_image": 64, "rounded_card": 32, "control": 24, "backdrop_region": 4},
      "clipped_rounded_cards": 32,
      "shadow_count": 32,
      "backdrop_blur_count": 4,
      "leaf_update_sequence": (0..20).map(|index| format!("dashboard:label:{:03}", index * 7 + 3)).collect::<Vec<_>>(),
      "update_10_percent_ids": (0..30).map(|index| format!("dashboard:node:{:03}", index * 9 % 300)).collect::<Vec<_>>()
   }))?;
   let layout = write_json(root, "layout/dashboard.mixed-static.json", &dashboard_layout())?;
   let leaf_trace = write_trace(root, "traces/dashboard-leaf-updates.json", dashboard_leaf_trace())?;
   let ten_percent_trace = write_trace(root, "traces/dashboard-update-10-percent.json", vec![mutate_event(0, "dashboard:update-count", TraceValue::Integer(30), "dashboard:updated-30")])?;
   let counts = dashboard_counts();
   let checkpoints = vec![
      checkpoint(root, "dashboard.mixed-static", "mounted", "first-mount", None, &counts, Golden::Dashboard(0))?,
      checkpoint(root, "dashboard.mixed-static", "idle", "clean-idle", None, &counts, Golden::Dashboard(0))?,
      checkpoint(root, "dashboard.mixed-static", "leaf-updated", "leaf-updates", Some(1_250_000), &counts, Golden::Dashboard(1))?,
      checkpoint(root, "dashboard.mixed-static", "updated-10-percent", "update-10-percent", Some(2_500_000), &counts, Golden::Dashboard(2))?,
   ];
   let scenario = ScenarioSpec {
      schema_version: 1,
      id: String::from("dashboard.mixed-static"),
      fixture,
      assets: shared.assets.clone(),
      font_pack: shared.font_pack.clone(),
      viewport_class: String::from("phone-portrait"),
      scene: SceneContract {
         roles: vec!["dashboard", "label", "icon-image", "rounded-card", "control", "backdrop-region"].into_iter().map(String::from).collect(),
         style_tokens: shared.style.clone(),
         layout_assertions: layout,
      },
      phases: vec![
         phase("setup", false, None, None),
         phase("prewarm", false, Some(2_000), None),
         phase("first-mount", true, Some(500), None),
         phase("clean-idle", true, Some(500), None),
         phase("leaf-updates", true, Some(2_500), Some(leaf_trace)),
         phase("update-10-percent", true, Some(2_500), Some(ten_percent_trace)),
         phase("teardown", false, None, None),
      ],
      primary_metric: String::from("leaf_update_to_attributed_presentation_ms"),
      required_metrics: required_metrics("leaf_update_to_attributed_presentation_ms"),
      optional_metrics: vec![String::from("gpu.device_scope_active_ms"), String::from("memory.declared_resource_bytes")],
      parity_checkpoints: checkpoints,
      fairness_contract: fairness("ltr", counts),
   };
   write_scenario(root, &scenario)
}

fn materialize_endurance(root: &Path, shared: &SharedArtifacts) -> Result<MaterializedScenario>
{
   let fixture = EnduranceFixture {
      schema_version: 1,
      id: String::from("endurance.churn"),
      visible_node_count: 300,
      categories: DashboardCategories {label: 176, icon_image: 64, rounded_card: 32, control: 24, backdrop_region: 4},
      clipped_rounded_cards: 32,
      shadow_count: 32,
      backdrop_blur_count: 4,
      heavy_screen_cycle_count: 100,
      tab_switch_count: 500,
      animation_frame_count: 600,
      tab_count: 2,
      initial_tab_index: 0,
      heavy_screen_target_id: String::from("endurance:heavy-screen-visible"),
      active_tab_target_id: String::from("endurance:active-tab"),
      animation_frame_target_id: String::from("endurance:animation-frame"),
   };
   let fixture = write_json(root, "fixtures/endurance.churn.json", &fixture)?;
   let layout = identity(root, "layout/dashboard.mixed-static.json")?;
   let open_close = write_trace(root, "traces/endurance-open-close-heavy-screen-100.json", endurance_open_close_trace())?;
   let tab_switches = write_trace(root, "traces/endurance-tab-switch-heavy-500.json", endurance_tab_switch_trace())?;
   let animation = write_trace(root, "traces/endurance-idle-animation-600.json", endurance_animation_trace())?;
   let counts = endurance_counts();
   let checkpoints = vec![
      checkpoint_reusing_screenshot(root, "endurance.churn", "heavy-screen-recovered", "open-close-heavy-screen", Some(119_400_000), &counts, "checkpoints/dashboard.mixed-static/idle/screenshot.png")?,
      checkpoint_reusing_screenshot(root, "endurance.churn", "tab-restored", "tab-switch-heavy", Some(164_670_000), &counts, "checkpoints/dashboard.mixed-static/idle/screenshot.png")?,
      checkpoint_reusing_screenshot(root, "endurance.churn", "animation-settled", "idle-animation", Some(9_983_333), &counts, "checkpoints/dashboard.mixed-static/idle/screenshot.png")?,
      checkpoint_reusing_screenshot(root, "endurance.churn", "recovered", "recovery", Some(5_000_000), &counts, "checkpoints/dashboard.mixed-static/idle/screenshot.png")?,
   ];
   let scenario = ScenarioSpec {
      schema_version: 1,
      id: String::from("endurance.churn"),
      fixture,
      assets: shared.assets.clone(),
      font_pack: shared.font_pack.clone(),
      viewport_class: String::from("phone-portrait"),
      scene: SceneContract {
         roles: vec!["endurance", "label", "icon-image", "rounded-card", "control", "backdrop-region"].into_iter().map(String::from).collect(),
         style_tokens: shared.style.clone(),
         layout_assertions: layout,
      },
      phases: vec![
         phase("setup", false, None, None),
         phase("prewarm", false, Some(2_000), None),
         phase("open-close-heavy-screen", true, Some(120_000), Some(open_close)),
         phase("tab-switch-heavy", true, Some(165_000), Some(tab_switches)),
         phase("idle-animation", true, Some(10_000), Some(animation)),
         phase("recovery", true, Some(5_000), None),
         phase("teardown", false, None, None),
      ],
      primary_metric: String::from("memory.retained_slope_bytes_per_min"),
      required_metrics: endurance_required_metrics(),
      optional_metrics: vec![String::from("memory.peak_resident_bytes"), String::from("memory.declared_resource_bytes")],
      parity_checkpoints: checkpoints,
      fairness_contract: fairness("ltr", counts),
   };
   write_scenario(root, &scenario)
}

fn materialize_feed(root: &Path, shared: &SharedArtifacts) -> Result<MaterializedScenario>
{
   let texts = [
      "A measured interface should still feel human.",
      "مرحبا بالعالم — تحديث ثابت",
      "你好，世界 — 固定内容",
      "Frame pacing matters more than a mean.",
      "اختبار التمرير متعدد اللغات",
      "可重复的滚动和图像更新",
      "Pinned assets; identical work; honest pixels.",
      "Status icons: ● ◆ ★ ☺",
   ];
   let rows = (0..2_000).map(|index| json!({
      "id": format!("feed:item:{:04}", index),
      "height": 68 + (index * 17 % 53),
      "text": texts[index % texts.len()],
      "thumbnail_index": index % 128,
      "favorite": false,
      "direction": if index % 8 == 1 || index % 8 == 4 {"rtl"} else {"ltr"}
   })).collect::<Vec<_>>();
   let fixture = write_json(root, "fixtures/feed.variable-scroll.json", &json!({
      "schema_version": 1,
      "id": "feed.variable-scroll",
      "row_count": 2000,
      "thumbnail_count": 128,
      "rows": rows,
      "prepend_rows": (0..20).map(|index| format!("feed:prepend:{:02}", index)).collect::<Vec<_>>()
   }))?;
   let layout = write_json(root, "layout/feed.variable-scroll.json", &feed_layout())?;
   let forward = write_trace(root, "traces/feed-forward-fling.json", fling_trace(false))?;
   let reverse = write_trace(root, "traces/feed-reverse-fling.json", fling_trace(true))?;
   let favorite = write_trace(root, "traces/feed-favorite-one.json", vec![mutate_event(0, "feed:item:0300:favorite", TraceValue::Boolean(true), "feed:item:0300:favorited")])?;
   let prepend = write_trace(root, "traces/feed-prepend-20.json", vec![mutate_event(0, "feed:prepend-count", TraceValue::Integer(20), "feed:prepended-20")])?;
   let initial = feed_counts(9, 9);
   let forward_visible = feed_counts(10, 10);
   let checkpoints = vec![
      checkpoint(root, "feed.variable-scroll", "initial", "setup", None, &initial, Golden::Feed(0))?,
      checkpoint(root, "feed.variable-scroll", "mid-forward", "forward-fling", Some(1_000_000), &forward_visible, Golden::Feed(1))?,
      checkpoint(root, "feed.variable-scroll", "mid-reverse", "reverse-fling", Some(1_000_000), &initial, Golden::Feed(2))?,
      checkpoint(root, "feed.variable-scroll", "favorite-applied", "favorite-one", Some(500_000), &initial, Golden::Feed(3))?,
      checkpoint(root, "feed.variable-scroll", "prepended-20", "prepend-20", Some(500_000), &initial, Golden::Feed(4))?,
      checkpoint(root, "feed.variable-scroll", "settled", "settle", Some(1_000_000), &initial, Golden::Feed(4))?,
   ];
   let scenario = ScenarioSpec {
      schema_version: 1,
      id: String::from("feed.variable-scroll"),
      fixture,
      assets: shared.assets.clone(),
      font_pack: shared.font_pack.clone(),
      viewport_class: String::from("phone-portrait"),
      scene: SceneContract {
         roles: vec!["navigation-bar", "feed", "feed-card", "thumbnail", "favorite-control"].into_iter().map(String::from).collect(),
         style_tokens: shared.style.clone(),
         layout_assertions: layout,
      },
      phases: vec![
         phase("setup", false, None, None),
         phase("prewarm", false, Some(2_000), None),
         phase("forward-fling", true, Some(2_000), Some(forward)),
         phase("reverse-fling", true, Some(2_000), Some(reverse)),
         phase("favorite-one", true, Some(500), Some(favorite)),
         phase("prepend-20", true, Some(500), Some(prepend)),
         phase("settle", true, Some(1_000), None),
         phase("teardown", false, None, None),
      ],
      primary_metric: String::from("frame.missed_display_opportunities_per_1000"),
      required_metrics: required_metrics("frame.missed_display_opportunities_per_1000"),
      optional_metrics: vec![String::from("gpu.device_scope_active_ms"), String::from("memory.allocated_bytes_per_s")],
      parity_checkpoints: checkpoints,
      fairness_contract: fairness("mixed-ltr-rtl", initial),
   };
   write_scenario(root, &scenario)
}

fn materialize_navigation(root: &Path, shared: &SharedArtifacts) -> Result<MaterializedScenario>
{
   let fixture = write_json(root, "fixtures/navigation.modal.json", &json!({
      "schema_version": 1,
      "id": "navigation.modal",
      "list_item_count": 12,
      "cycle_count": 4,
      "transition": {"duration_ms": 300, "curve": "ease-in-out"},
      "interactive_cancel_fraction": 0.5,
      "selected_item_id": "navigation:item:05"
   }))?;
   let layout = write_json(root, "layout/navigation.modal.json", &navigation_layout())?;
   let canonical = write_trace(root, "traces/navigation-canonical-cycles.json", navigation_cycle_trace())?;
   let cancel = write_trace(root, "traces/navigation-interactive-cancel.json", navigation_cancel_trace())?;
   let list_counts = navigation_list_counts();
   let modal_counts = navigation_modal_counts();
   let checkpoints = vec![
      checkpoint(root, "navigation.modal", "list-initial", "setup", None, &list_counts, Golden::Navigation(0))?,
      checkpoint(root, "navigation.modal", "modal-0", "canonical-cycles", Some(325_000), &modal_counts, Golden::Navigation(1))?,
      checkpoint(root, "navigation.modal", "modal-25", "canonical-cycles", Some(400_000), &modal_counts, Golden::Navigation(2))?,
      checkpoint(root, "navigation.modal", "modal-50", "canonical-cycles", Some(475_000), &modal_counts, Golden::Navigation(3))?,
      checkpoint(root, "navigation.modal", "modal-75", "canonical-cycles", Some(550_000), &modal_counts, Golden::Navigation(4))?,
      checkpoint(root, "navigation.modal", "modal-100", "canonical-cycles", Some(625_000), &modal_counts, Golden::Navigation(5))?,
      checkpoint(root, "navigation.modal", "list-restored", "canonical-cycles", Some(5_000_000), &list_counts, Golden::Navigation(0))?,
      checkpoint(root, "navigation.modal", "cancel-mid", "interactive-cancel", Some(500_000), &modal_counts, Golden::Navigation(3))?,
      checkpoint(root, "navigation.modal", "cancel-restored", "interactive-cancel", Some(1_000_000), &list_counts, Golden::Navigation(0))?,
   ];
   let scenario = ScenarioSpec {
      schema_version: 1,
      id: String::from("navigation.modal"),
      fixture,
      assets: shared.assets.clone(),
      font_pack: shared.font_pack.clone(),
      viewport_class: String::from("phone-portrait"),
      scene: SceneContract {
         roles: vec!["navigation-list", "list-item", "detail", "modal", "dismiss-control", "back-control"].into_iter().map(String::from).collect(),
         style_tokens: shared.style.clone(),
         layout_assertions: layout,
      },
      phases: vec![
         phase("setup", false, None, None),
         phase("prewarm", false, Some(2_000), None),
         phase("canonical-cycles", true, Some(5_000), Some(canonical)),
         phase("interactive-cancel", true, Some(1_000), Some(cancel)),
         phase("teardown", false, None, None),
      ],
      primary_metric: String::from("modal_open_to_attributed_presentation_ms"),
      required_metrics: required_metrics("modal_open_to_attributed_presentation_ms"),
      optional_metrics: vec![String::from("gpu.device_scope_active_ms"), String::from("transition.cancel_to_restored_ms")],
      parity_checkpoints: checkpoints,
      fairness_contract: fairness("ltr", list_counts),
   };
   write_scenario(root, &scenario)
}

fn dashboard_layout() -> Value
{
   json!({
      "schema_version": 1,
      "coordinate_space": "logical-points",
      "root": {"x": 0, "y": 0, "width": 390, "height": 844},
      "grid": {"columns": 2, "gap": 12, "inset": 16, "top": 48, "card_height": 38, "row_stride": 46},
      "backdrop_regions": [[16, 64, 358, 96], [16, 268, 358, 96], [16, 472, 358, 96], [16, 676, 358, 96]],
      "text_comparison": {"pixel_mask": true, "baseline_tolerance": 0.5, "ink_bounds_tolerance": 0.5},
      "text_masks": [[16, 18, 250, 22], [58, 48, 316, 728]]
   })
}

fn feed_layout() -> Value
{
   json!({
      "schema_version": 1,
      "coordinate_space": "logical-points",
      "root": {"x": 0, "y": 0, "width": 390, "height": 844},
      "navigation_bar_height": 52,
      "row_inset": 12,
      "row_gap": 8,
      "thumbnail": {"width": 48, "height": 48, "corner_radius": 8},
      "anchor_preservation": "first-fully-visible-row-and-offset",
      "text_comparison": {"pixel_mask": true, "baseline_tolerance": 0.5, "ink_bounds_tolerance": 0.5},
      "text_masks": [[16, 18, 250, 22], [82, 52, 284, 792]]
   })
}

fn navigation_layout() -> Value
{
   json!({
      "schema_version": 1,
      "coordinate_space": "logical-points",
      "root": {"x": 0, "y": 0, "width": 390, "height": 844},
      "list": {"inset": 16, "row_height": 54, "row_gap": 8},
      "modal": {"x": 24, "y": 132, "width": 342, "height": 580, "corner_radius": 16},
      "transition": {"duration_ms": 300, "curve": "ease-in-out", "checkpoints": [0, 0.25, 0.5, 0.75, 1.0]},
      "interactive_cancel_fraction": 0.5,
      "text_masks": [[16, 18, 358, 22], [26, 56, 338, 744], [24, 132, 366, 580]]
   })
}

fn startup_layout() -> Value
{
   json!({
      "schema_version": 1,
      "coordinate_space": "logical-points",
      "root": {"x": 0, "y": 0, "width": 390, "height": 844},
      "header": {"x": 16, "y": 20, "width": 358, "height": 48},
      "navigation": {"x": 16, "y": 76, "width": 358, "height": 44},
      "cards": {"visible": 6, "columns": 2, "gap": 12, "inset": 16, "height": 176},
      "primary_control": {"x": 16, "y": 776, "width": 358, "height": 48},
      "text_masks": [[24, 30, 180, 22], [24, 88, 180, 20], [16, 132, 358, 176], [16, 320, 358, 176], [16, 508, 358, 176], [80, 790, 230, 20]]
   })
}

fn chat_layout() -> Value
{
   json!({
      "schema_version": 1,
      "coordinate_space": "logical-points",
      "root": {"x": 0, "y": 0, "width": 390, "height": 844},
      "thread": {"inset": 12, "top": 52, "bottom": 92, "visible_messages": 10},
      "avatar": {"width": 36, "height": 36, "corner_radius": 18},
      "message": {"maximum_width": 286, "vertical_gap": 8, "corner_radius": 12},
      "composer": {"x": 12, "y": 780, "width": 318, "height": 48},
      "send_control": {"x": 338, "y": 780, "width": 40, "height": 48},
      "text_masks": [[16, 18, 250, 22], [12, 52, 366, 700], [24, 790, 294, 20], [342, 790, 32, 20]]
   })
}

fn image_layout() -> Value
{
   json!({
      "schema_version": 1,
      "coordinate_space": "logical-points",
      "root": {"x": 0, "y": 0, "width": 390, "height": 844},
      "canvas": {"x": 0, "y": 52, "width": 390, "height": 740, "clip": true},
      "thumbnail": {"x": 16, "y": 68, "width": 96, "height": 72},
      "zoom_control": {"x": 16, "y": 800, "width": 358, "height": 28},
      "source_aspect_ratio": "4:3",
      "pan_distance_millionths": 450000,
      "pinch_scale_millionths": 2000000,
      "text_masks": [[18, 18, 250, 22], [28, 805, 334, 18]]
   })
}

fn required_metrics(primary_metric: &str) -> Vec<String>
{
   [
      "frame.present_ms",
      "first.interactive_ms",
      "input.event_to_visible_response_ms",
      "frame.missed_display_opportunities_per_1000",
      "cpu.main_thread_ms",
      "memory.resident_bytes",
      "oxide.dirty_node_count",
      "oxide.layout_pass_count",
      "oxide.draw_call_count",
      "oxide.encoded_bytes",
      "oxide.texture_bytes",
   ].into_iter().filter(|metric| *metric != primary_metric).map(String::from).collect()
}

fn endurance_required_metrics() -> Vec<String>
{
   let mut metrics = required_metrics("memory.retained_slope_bytes_per_min");
   metrics.push(String::from("memory.resident_drift_bytes"));
   metrics.push(String::from("memory.recovery_ms"));
   metrics
}

fn phase(id: &str, measured: bool, duration_ms: Option<u64>, trace: Option<ArtifactIdentity>) -> ScenarioPhase
{
   ScenarioPhase {id: String::from(id), measured, duration_ms, trace}
}

fn fairness(direction: &str, counts: Vec<RoleCount>) -> FairnessContract
{
   FairnessContract {
      locale: String::from("en_US_POSIX"),
      timezone: String::from("UTC"),
      direction: String::from(direction),
      logical_viewport_width: 390,
      logical_viewport_height: 844,
      expected_visible_role_counts: counts,
      schedule_tolerance_us: 1_000,
      coordinate_tolerance_microunits: 1_000,
      elapsed_time_driven: true,
   }
}

fn dashboard_counts() -> Vec<RoleCount>
{
   role_counts(&[("dashboard", 1), ("label", 176), ("icon-image", 64), ("rounded-card", 32), ("control", 24), ("backdrop-region", 4)])
}

fn endurance_counts() -> Vec<RoleCount>
{
   role_counts(&[("endurance", 1), ("label", 176), ("icon-image", 64), ("rounded-card", 32), ("control", 24), ("backdrop-region", 4)])
}

fn feed_counts(cards: u32, thumbnails: u32) -> Vec<RoleCount>
{
   role_counts(&[("navigation-bar", 1), ("feed", 1), ("feed-card", cards), ("thumbnail", thumbnails), ("favorite-control", cards)])
}

fn navigation_list_counts() -> Vec<RoleCount>
{
   role_counts(&[("navigation-list", 1), ("list-item", 12)])
}

fn navigation_modal_counts() -> Vec<RoleCount>
{
   role_counts(&[("detail", 1), ("modal", 1), ("dismiss-control", 1), ("back-control", 1)])
}

fn startup_counts() -> Vec<RoleCount>
{
   role_counts(&[("header", 1), ("navigation", 1), ("card", 6), ("initial-image", 6), ("primary-control", 1)])
}

fn chat_counts() -> Vec<RoleCount>
{
   role_counts(&[("chat-thread", 1), ("message", 10), ("avatar", 10), ("composer", 1), ("send-control", 1)])
}

fn image_counts() -> Vec<RoleCount>
{
   role_counts(&[("image-canvas", 1), ("image", 1), ("zoom-control", 1)])
}

fn role_counts(values: &[(&str, u32)]) -> Vec<RoleCount>
{
   values.iter().map(|(role, count)| RoleCount {role: String::from(*role), count: *count}).collect()
}

fn dashboard_leaf_trace() -> Vec<TraceEvent>
{
   (0..20).map(|index| mutate_event(index * 125_000, &format!("dashboard:label:{:03}", index * 7 + 3), TraceValue::Integer(index as i64 + 1), &format!("dashboard:leaf-update:{:02}", index))).collect()
}

fn endurance_open_close_trace() -> Vec<TraceEvent>
{
   let mut events = Vec::with_capacity(200);
   for cycle in 0..100_u64
   {
      let base = cycle * 1_200_000;
      events.push(mutate_event(base, "endurance:heavy-screen-visible", TraceValue::Boolean(false), &format!("endurance:heavy-screen:closed:{:03}", cycle + 1)));
      events.push(mutate_event(base + 600_000, "endurance:heavy-screen-visible", TraceValue::Boolean(true), &format!("endurance:heavy-screen:open:{:03}", cycle + 1)));
   }
   events
}

fn endurance_tab_switch_trace() -> Vec<TraceEvent>
{
   (0..500_u64).map(|index| {
      let tab = (index + 1) % 2;
      mutate_event(index * 330_000, "endurance:active-tab", TraceValue::Integer(tab as i64), &format!("endurance:tab:{tab}:switch:{:03}", index + 1))
   }).collect()
}

fn endurance_animation_trace() -> Vec<TraceEvent>
{
   (0..600_u64).map(|index| {
      let frame = index + 1;
      mutate_event(index * 50_000 / 3, "endurance:animation-frame", TraceValue::Integer(frame as i64), &format!("endurance:animation-frame:{frame:03}"))
   }).collect()
}

fn chat_message(index: u32, prepend: bool) -> ChatMessage
{
   let texts = [
      ("ltr", "Stable frames keep conversation feeling immediate."),
      ("rtl", "مرحبا بالعالم — رسالة ثابتة"),
      ("ltr", "你好，世界 — 固定消息"),
      ("ltr", "Emoji remain grapheme clusters: 👩🏽‍💻 🌍 ✨"),
      ("rtl", "اختبار كتابة واتجاه من اليمين إلى اليسار"),
      ("ltr", "可重复的文本、头像和布局更新"),
      ("ltr", "Measured work stays identical across implementations."),
      ("ltr", "Family status: 👨‍👩‍👧‍👦; flags: 🇺🇳 🇯🇵"),
   ];
   let (direction, text) = texts[index as usize % texts.len()];
   let prefix = if prepend {"chat:prepend"} else {"chat:message"};
   ChatMessage {
      id: format!("{prefix}:{index:04}"),
      sequence: if prepend {index} else {index + 50},
      author_index: index % 64,
      avatar_index: index % 64,
      direction: String::from(direction),
      text: String::from(text),
   }
}

fn image_pan_trace() -> Vec<TraceEvent>
{
   (0..=8).map(|step| {
      let at_us = step * 250_000;
      let x = 725_000 - step as i32 * 56_250;
      let op = if step == 0 {TraceOperation::PointerDown} else if step == 8 {TraceOperation::PointerUp} else {TraceOperation::PointerMove};
      pointer_event(at_us, op, 1, x, 500_000)
   }).collect()
}

fn image_pinch_trace() -> Vec<TraceEvent>
{
   let mut events = Vec::new();
   for step in 0..=8_u64
   {
      let at_us = step * 250_000;
      let distance = step as i32 * 31_250;
      let op = if step == 0 {TraceOperation::PointerDown} else if step == 8 {TraceOperation::PointerUp} else {TraceOperation::PointerMove};
      events.push(pointer_event(at_us, op, 1, 375_000 - distance, 500_000));
      events.push(pointer_event(at_us, op, 2, 625_000 + distance, 500_000));
   }
   events
}

fn fling_trace(reverse: bool) -> Vec<TraceEvent>
{
   let (start, end) = if reverse {(250_000, 850_000)} else {(850_000, 250_000)};
   vec![
      pointer_event(0, TraceOperation::PointerDown, 1, 500_000, start),
      pointer_event(80_000, TraceOperation::PointerMove, 1, 500_000, (start + end) / 2),
      pointer_event(160_000, TraceOperation::PointerUp, 1, 500_000, end),
   ]
}

fn navigation_cycle_trace() -> Vec<TraceEvent>
{
   let mut events = Vec::new();
   for cycle in 0..4_u64
   {
      let base = cycle * 1_200_000;
      events.push(navigate_event(base, "navigation:item:05", &format!("navigation:cycle:{cycle}:detail")));
      events.push(navigate_event(base + 325_000, "navigation:modal", &format!("navigation:cycle:{cycle}:modal")));
      events.push(navigate_event(base + 650_000, "navigation:dismiss-control", &format!("navigation:cycle:{cycle}:detail-restored")));
      events.push(navigate_event(base + 950_000, "navigation:back-control", &format!("navigation:cycle:{cycle}:list-restored")));
   }
   events
}

fn navigation_cancel_trace() -> Vec<TraceEvent>
{
   vec![
      pointer_event(0, TraceOperation::PointerDown, 1, 950_000, 500_000),
      pointer_event(250_000, TraceOperation::PointerMove, 1, 750_000, 500_000),
      pointer_event(500_000, TraceOperation::PointerMove, 1, 500_000, 500_000),
      pointer_event(750_000, TraceOperation::PointerCancel, 1, 500_000, 500_000),
      navigate_event(1_000_000, "navigation:cancel", "navigation:list-restored"),
   ]
}

fn mutate_event(at_us: u64, target: &str, value: TraceValue, state_id: &str) -> TraceEvent
{
   TraceEvent {
      at_us,
      op: TraceOperation::Mutate,
      pointer: None,
      x_millionths: None,
      y_millionths: None,
      delta_x_millionths: None,
      delta_y_millionths: None,
      target: Some(String::from(target)),
      value: Some(value),
      state_id: Some(String::from(state_id)),
   }
}

fn pointer_event(at_us: u64, op: TraceOperation, pointer: u32, x_millionths: i32, y_millionths: i32) -> TraceEvent
{
   TraceEvent {
      at_us,
      op,
      pointer: Some(pointer),
      x_millionths: Some(x_millionths),
      y_millionths: Some(y_millionths),
      delta_x_millionths: None,
      delta_y_millionths: None,
      target: None,
      value: None,
      state_id: None,
   }
}

fn navigate_event(at_us: u64, target: &str, state_id: &str) -> TraceEvent
{
   TraceEvent {
      at_us,
      op: TraceOperation::Navigate,
      pointer: None,
      x_millionths: None,
      y_millionths: None,
      delta_x_millionths: None,
      delta_y_millionths: None,
      target: Some(String::from(target)),
      value: None,
      state_id: Some(String::from(state_id)),
   }
}

fn lifecycle_event(at_us: u64, op: TraceOperation, target: &str, state_id: &str) -> TraceEvent
{
   TraceEvent {
      at_us,
      op,
      pointer: None,
      x_millionths: None,
      y_millionths: None,
      delta_x_millionths: None,
      delta_y_millionths: None,
      target: Some(String::from(target)),
      value: None,
      state_id: Some(String::from(state_id)),
   }
}

fn text_event(at_us: u64, op: TraceOperation, target: &str, value: String, state_id: &str) -> TraceEvent
{
   TraceEvent {
      at_us,
      op,
      pointer: None,
      x_millionths: None,
      y_millionths: None,
      delta_x_millionths: None,
      delta_y_millionths: None,
      target: Some(String::from(target)),
      value: Some(TraceValue::Text(value)),
      state_id: Some(String::from(state_id)),
   }
}

fn checkpoint(root: &Path, scenario_id: &str, id: &str, phase_id: &str, at_us: Option<u64>, counts: &[RoleCount], golden: Golden) -> Result<ParityCheckpoint>
{
   let prefix = format!("checkpoints/{scenario_id}/{id}");
   let model = checkpoint_model(scenario_id, id)?;
   let state = write_json(root, &format!("{prefix}/state.json"), &json!({
      "schema_version": 2,
      "scenario_id": scenario_id,
      "checkpoint_id": id,
      "model": &model,
      "visible_role_counts": counts
   }))?;
   let accessibility = write_json(root, &format!("{prefix}/accessibility.json"), &json!({
      "schema_version": 2,
      "scenario_id": scenario_id,
      "checkpoint_id": id,
      "root_frame": [0, 0, 390, 844],
      "raw_tree_source": "runtime-semantic-tree",
      "nodes": semantic_accessibility_nodes(scenario_id, &model, counts)
   }))?;
   let screenshot_path = format!("{prefix}/screenshot.png");
   write_golden_png(root, &root.join(&screenshot_path), golden)?;
   let screenshot = identity(root, &screenshot_path)?;
   Ok(ParityCheckpoint {
      id: String::from(id),
      phase_id: String::from(phase_id),
      at_us,
      state,
      accessibility,
      geometry: None,
      screenshot,
      recapture_status: None,
      expected_visible_role_counts: counts.to_vec(),
   })
}

fn checkpoint_reusing_screenshot(root: &Path, scenario_id: &str, id: &str, phase_id: &str, at_us: Option<u64>, counts: &[RoleCount], screenshot_path: &str) -> Result<ParityCheckpoint>
{
   let prefix = format!("checkpoints/{scenario_id}/{id}");
   let model = checkpoint_model(scenario_id, id)?;
   let state = write_json(root, &format!("{prefix}/state.json"), &json!({
      "schema_version": 2,
      "scenario_id": scenario_id,
      "checkpoint_id": id,
      "model": &model,
      "visible_role_counts": counts
   }))?;
   let accessibility = write_json(root, &format!("{prefix}/accessibility.json"), &json!({
      "schema_version": 2,
      "scenario_id": scenario_id,
      "checkpoint_id": id,
      "root_frame": [0, 0, 390, 844],
      "raw_tree_source": "runtime-semantic-tree",
      "nodes": semantic_accessibility_nodes(scenario_id, &model, counts)
   }))?;
   let screenshot = identity(root, screenshot_path)?;
   Ok(ParityCheckpoint {
      id: String::from(id),
      phase_id: String::from(phase_id),
      at_us,
      state,
      accessibility,
      geometry: None,
      screenshot,
      recapture_status: None,
      expected_visible_role_counts: counts.to_vec(),
   })
}

fn checkpoint_model(scenario_id: &str, checkpoint_id: &str) -> Result<Value>
{
   let model = match (scenario_id, checkpoint_id)
   {
      ("startup.first-screen", "terminated-ready") => json!({"foreground_count": 1, "background_count": 0, "fresh_install_ready": false, "scene_visible": true, "lifecycle_state_id": "startup:launch-requested", "card_count": 24}),
      ("startup.first-screen", "fresh-install-ready") => json!({"foreground_count": 1, "background_count": 0, "fresh_install_ready": true, "scene_visible": true, "lifecycle_state_id": "startup:fresh-install-ready", "card_count": 24}),
      ("startup.first-screen", "warm-resume-ready") => json!({"foreground_count": 2, "background_count": 1, "fresh_install_ready": true, "scene_visible": true, "lifecycle_state_id": "startup:resume-requested", "card_count": 24}),
      ("dashboard.mixed-static", "mounted" | "idle") => json!({"leaf_update_count": 0, "bulk_update_count": 0, "visible_node_count": 301}),
      ("dashboard.mixed-static", "leaf-updated") => json!({"leaf_update_count": 11, "bulk_update_count": 0, "visible_node_count": 301}),
      ("dashboard.mixed-static", "updated-10-percent") => json!({"leaf_update_count": 20, "bulk_update_count": 30, "visible_node_count": 301}),
      ("endurance.churn", "heavy-screen-recovered") => json!({"open_close_cycle_count": 100, "heavy_screen_transition_count": 200, "heavy_screen_visible": true, "tab_switch_count": 0, "active_tab_index": 0, "animation_frame_count": 0, "animation_frame_index": 0, "visible_node_count": 301}),
      ("endurance.churn", "tab-restored") => json!({"open_close_cycle_count": 100, "heavy_screen_transition_count": 200, "heavy_screen_visible": true, "tab_switch_count": 500, "active_tab_index": 0, "animation_frame_count": 0, "animation_frame_index": 0, "visible_node_count": 301}),
      ("endurance.churn", "animation-settled" | "recovered") => json!({"open_close_cycle_count": 100, "heavy_screen_transition_count": 200, "heavy_screen_visible": true, "tab_switch_count": 500, "active_tab_index": 0, "animation_frame_count": 600, "animation_frame_index": 600, "visible_node_count": 301}),
      ("feed.variable-scroll", "initial") => json!({"row_count": 2_000, "scroll_position_millionths": 0, "favorite_id": null, "prepend_count": 0}),
      ("feed.variable-scroll", "mid-forward") => json!({"row_count": 2_000, "scroll_position_millionths": 750_000, "favorite_id": null, "prepend_count": 0}),
      ("feed.variable-scroll", "mid-reverse") => json!({"row_count": 2_000, "scroll_position_millionths": 150_000, "favorite_id": null, "prepend_count": 0}),
      ("feed.variable-scroll", "favorite-applied") => json!({"row_count": 2_000, "scroll_position_millionths": 150_000, "favorite_id": "feed:item:0300", "prepend_count": 0}),
      ("feed.variable-scroll", "prepended-20" | "settled") => json!({"row_count": 2_020, "scroll_position_millionths": 150_000, "favorite_id": "feed:item:0300", "prepend_count": 20}),
      ("chat.live-update", "initial") => json!({"message_count": 5_000, "prepend_count": 0, "append_count": 0, "composer_utf8_count": 0, "focused_message_id": null, "selection_active": false, "replacement_applied": false}),
      ("chat.live-update", "prepended-50") => json!({"message_count": 5_050, "prepend_count": 50, "append_count": 0, "composer_utf8_count": 0, "focused_message_id": null, "selection_active": false, "replacement_applied": false}),
      ("chat.live-update", "append-settled") => json!({"message_count": 5_070, "prepend_count": 50, "append_count": 20, "composer_utf8_count": 0, "focused_message_id": null, "selection_active": false, "replacement_applied": false}),
      ("chat.live-update", "typed-100") => json!({"message_count": 5_070, "prepend_count": 50, "append_count": 20, "composer_utf8_count": 100, "focused_message_id": null, "selection_active": false, "replacement_applied": false}),
      ("chat.live-update", "pasted-10kib") => json!({"message_count": 5_070, "prepend_count": 50, "append_count": 20, "composer_utf8_count": 10_340, "focused_message_id": null, "selection_active": false, "replacement_applied": false}),
      ("chat.live-update", "selection-replaced") => json!({"message_count": 5_070, "prepend_count": 50, "append_count": 20, "composer_utf8_count": 10_340, "focused_message_id": "chat:append:16", "selection_active": false, "replacement_applied": true}),
      ("navigation.modal", "list-initial") => json!({"route": "list", "modal_visible": false, "completed_cycles": 0}),
      ("navigation.modal", "modal-0" | "modal-25" | "modal-50" | "modal-75" | "modal-100") => json!({"route": "detail", "modal_visible": true, "completed_cycles": 0}),
      ("navigation.modal", "list-restored") => json!({"route": "list", "modal_visible": false, "completed_cycles": 4}),
      ("navigation.modal", "cancel-mid") => json!({"route": "detail", "modal_visible": true, "completed_cycles": 4}),
      ("navigation.modal", "cancel-restored") => json!({"route": "list", "modal_visible": false, "completed_cycles": 4}),
      ("image.decode-zoom", "thumbnail") => json!({"resource_stage": "thumbnail", "pan_x_millionths": 0, "pan_y_millionths": 0, "scale_millionths": 1_000_000, "active_pointer_count": 0}),
      ("image.decode-zoom", "first-visible") => json!({"resource_stage": "visible", "pan_x_millionths": 0, "pan_y_millionths": 0, "scale_millionths": 1_000_000, "active_pointer_count": 0}),
      ("image.decode-zoom", "pan-mid") => json!({"resource_stage": "visible", "pan_x_millionths": -225_000, "pan_y_millionths": 0, "scale_millionths": 1_000_000, "active_pointer_count": 1}),
      ("image.decode-zoom", "pinch-mid") => json!({"resource_stage": "visible", "pan_x_millionths": -450_000, "pan_y_millionths": 0, "scale_millionths": 2_000_000, "active_pointer_count": 2}),
      _ => anyhow::bail!("no canonical checkpoint model for {scenario_id}:{checkpoint_id}"),
   };
   Ok(model)
}

fn semantic_accessibility_nodes(scenario_id: &str, model: &Value, counts: &[RoleCount]) -> Vec<Value>
{
   counts.iter().enumerate().map(|(order, role)|
   {
      let visible = role.count > 0;
      json!({
         "role": role.role,
         "name": role.role,
         "value": role.count.to_string(),
         "state": if visible {vec!["enabled", "visible"]} else {vec!["hidden"]},
         "order": order,
         "focused": scenario_id == "chat.live-update" && role.role == "message" && !model["focused_message_id"].is_null(),
         "actions": semantic_role_actions(&role.role),
         "frame": semantic_role_frame(scenario_id, &role.role),
         "count": role.count,
         "visible": visible,
      })
   }).collect()
}

fn semantic_role_actions(role: &str) -> Vec<&'static str>
{
   match role
   {
      "primary-control" | "control" | "favorite-control" | "send-control" | "list-item" | "dismiss-control" | "back-control" => vec!["activate"],
      "feed" | "chat-thread" => vec!["scroll"],
      "composer" => vec!["set-text"],
      "image-canvas" | "image" => vec!["pan", "zoom"],
      "zoom-control" => vec!["increment", "decrement"],
      _ => Vec::new(),
   }
}

fn semantic_role_frame(scenario_id: &str, role: &str) -> [i32; 4]
{
   match (scenario_id, role)
   {
      ("startup.first-screen", "header") => [16, 20, 358, 48],
      ("startup.first-screen", "navigation") => [16, 76, 358, 44],
      ("startup.first-screen", "card") | ("startup.first-screen", "initial-image") => [16, 132, 358, 552],
      ("startup.first-screen", "primary-control") => [16, 776, 358, 48],
      ("dashboard.mixed-static", "dashboard") | ("endurance.churn", "endurance") => [0, 0, 390, 844],
      ("dashboard.mixed-static", "backdrop-region") | ("endurance.churn", "backdrop-region") => [12, 42, 366, 586],
      ("dashboard.mixed-static", _) | ("endurance.churn", _) => [16, 48, 358, 728],
      ("feed.variable-scroll", "navigation-bar") => [0, 0, 390, 52],
      ("feed.variable-scroll", _) => [0, 52, 390, 792],
      ("chat.live-update", "chat-thread") | ("chat.live-update", "message") | ("chat.live-update", "avatar") => [0, 52, 390, 700],
      ("chat.live-update", "composer") => [12, 780, 318, 48],
      ("chat.live-update", "send-control") => [338, 780, 40, 48],
      ("navigation.modal", "navigation-list") | ("navigation.modal", "list-item") | ("navigation.modal", "detail") | ("navigation.modal", "back-control") => [0, 0, 390, 844],
      ("navigation.modal", "modal") | ("navigation.modal", "dismiss-control") => [24, 132, 342, 580],
      ("image.decode-zoom", "image-canvas") | ("image.decode-zoom", "image") => [0, 52, 390, 740],
      ("image.decode-zoom", "zoom-control") => [16, 800, 358, 28],
      _ => [0, 0, 390, 844],
   }
}

fn write_trace(root: &Path, path: &str, events: Vec<TraceEvent>) -> Result<ArtifactIdentity>
{
   write_json(root, path, &events)
}

fn write_scenario(root: &Path, scenario: &ScenarioSpec) -> Result<MaterializedScenario>
{
   let path = format!("scenarios/{}.json", scenario.id);
   write_bytes(&root.join(&path), &canonical_scenario_json(scenario)?)?;
   Ok(MaterializedScenario {path})
}

fn write_json<T: Serialize + ?Sized>(root: &Path, path: &str, value: &T) -> Result<ArtifactIdentity>
{
   let mut bytes = serde_json::to_vec_pretty(value).context("serializing materialized JSON artifact")?;
   bytes.push(b'\n');
   write_bytes(&root.join(path), &bytes)?;
   identity(root, path)
}

fn identity(root: &Path, path: &str) -> Result<ArtifactIdentity>
{
   let bytes = fs::read(root.join(path)).with_context(|| format!("reading materialized artifact {path}"))?;
   Ok(ArtifactIdentity {path: String::from(path), sha256: format!("{:x}", Sha256::digest(bytes))})
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()>
{
   let parent = path.parent().context("materialized artifact has no parent")?;
   fs::create_dir_all(parent).with_context(|| format!("creating artifact directory {}", parent.display()))?;
   fs::write(path, bytes).with_context(|| format!("writing materialized artifact {}", path.display()))
}

#[derive(Clone, Copy)]
enum Golden
{
   Startup(u8),
   Dashboard(u8),
   Feed(u8),
   Chat(u8),
   Navigation(u8),
   Image(u8),
}

struct Canvas
{
   pixels: Vec<u8>,
}

struct RasterImage
{
   width: u32,
   height: u32,
   pixels: Vec<u8>,
}

const BACKGROUND: [u8; 4] = [243, 245, 248, 255];
const SURFACE: [u8; 4] = [255, 255, 255, 255];
const TEXT: [u8; 4] = [32, 36, 44, 255];
const SECONDARY: [u8; 4] = [105, 113, 129, 255];
const ACCENT: [u8; 4] = [61, 110, 239, 255];
const MUTED: [u8; 4] = [238, 241, 246, 255];
const RTL_SURFACE: [u8; 4] = [234, 238, 247, 255];

fn px(value: i32) -> i32
{
   value * 3
}

impl Canvas
{
   fn new(color: [u8; 4]) -> Self
   {
      let mut pixels = vec![0; WIDTH as usize * HEIGHT as usize * 4];
      for pixel in pixels.chunks_exact_mut(4)
      {
         pixel.copy_from_slice(&color);
      }
      Self {pixels}
   }

   fn blend_pixel(&mut self, x: i32, y: i32, color: [u8; 4])
   {
      if x < 0 || y < 0 || x >= WIDTH as i32 || y >= HEIGHT as i32
      {
         return;
      }
      let offset = ((y as u32 * WIDTH + x as u32) * 4) as usize;
      let alpha = u32::from(color[3]);
      let inverse = 255 - alpha;
      for channel in 0..3
      {
         self.pixels[offset + channel] = ((u32::from(color[channel]) * alpha + u32::from(self.pixels[offset + channel]) * inverse + 127) / 255) as u8;
      }
      self.pixels[offset + 3] = 255;
   }

   fn rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: [u8; 4])
   {
      let left = x.max(0);
      let top = y.max(0);
      let right = x.saturating_add(width).min(WIDTH as i32);
      let bottom = y.saturating_add(height).min(HEIGHT as i32);
      for row in top..bottom
      {
         for column in left..right
         {
            self.blend_pixel(column, row, color);
         }
      }
   }

   fn rounded_rect(&mut self, x: i32, y: i32, width: i32, height: i32, radius: i32, color: [u8; 4])
   {
      let left = x.max(0);
      let top = y.max(0);
      let right = x.saturating_add(width).min(WIDTH as i32);
      let bottom = y.saturating_add(height).min(HEIGHT as i32);
      for row in top..bottom
      {
         for column in left..right
         {
            if rounded_contains(column - x, row - y, width, height, radius)
            {
               self.blend_pixel(column, row, color);
            }
         }
      }
   }

   fn shadowed_rect(&mut self, x: i32, y: i32, width: i32, height: i32, radius: i32, color: [u8; 4])
   {
      self.rounded_rect(x, y + px(2), width, height, radius, [32, 36, 44, 41]);
      self.rounded_rect(x, y, width, height, radius, color);
   }

   fn image(&mut self, image: &RasterImage, source: [u32; 4], destination: [i32; 4], clip: [i32; 4], radius: i32)
   {
      let [x, y, width, height] = destination;
      let [clip_x, clip_y, clip_width, clip_height] = clip;
      let left = x.max(clip_x).max(0);
      let top = y.max(clip_y).max(0);
      let right = x.saturating_add(width).min(clip_x.saturating_add(clip_width)).min(WIDTH as i32);
      let bottom = y.saturating_add(height).min(clip_y.saturating_add(clip_height)).min(HEIGHT as i32);
      if width <= 0 || height <= 0 || left >= right || top >= bottom
      {
         return;
      }
      for row in top..bottom
      {
         for column in left..right
         {
            if radius > 0 && !rounded_contains(column - x, row - y, width, height, radius)
            {
               continue;
            }
            let local_x = column - x;
            let local_y = row - y;
            let (source_x, weight_x) = source_coordinate(local_x, width, source[2]);
            let (source_y, weight_y) = source_coordinate(local_y, height, source[3]);
            let x0 = source[0] + source_x;
            let y0 = source[1] + source_y;
            let x1 = (x0 + 1).min(source[0] + source[2] - 1);
            let y1 = (y0 + 1).min(source[1] + source[3] - 1);
            let mut color = [0, 0, 0, 255];
            for channel in 0..4
            {
               let top_value = lerp_channel(image.channel(x0, y0, channel), image.channel(x1, y0, channel), weight_x);
               let bottom_value = lerp_channel(image.channel(x0, y1, channel), image.channel(x1, y1, channel), weight_x);
               color[channel] = lerp_channel(top_value, bottom_value, weight_y);
            }
            self.blend_pixel(column, row, color);
         }
      }
   }

   fn box_blur(&mut self, x: i32, y: i32, width: i32, height: i32, radius: usize)
   {
      let left = x.max(0) as usize;
      let top = y.max(0) as usize;
      let right = x.saturating_add(width).min(WIDTH as i32).max(0) as usize;
      let bottom = y.saturating_add(height).min(HEIGHT as i32).max(0) as usize;
      if left >= right || top >= bottom
      {
         return;
      }
      let region_width = right - left;
      let region_height = bottom - top;
      let mut horizontal = vec![0_u8; region_width * region_height * 3];
      for row in 0..region_height
      {
         for channel in 0..3
         {
            let mut prefix = vec![0_u32; region_width + 1];
            for column in 0..region_width
            {
               let source = (((top + row) * WIDTH as usize + left + column) * 4) + channel;
               prefix[column + 1] = prefix[column] + u32::from(self.pixels[source]);
            }
            for column in 0..region_width
            {
               let start = column.saturating_sub(radius);
               let end = (column + radius + 1).min(region_width);
               horizontal[(row * region_width + column) * 3 + channel] = ((prefix[end] - prefix[start]) / (end - start) as u32) as u8;
            }
         }
      }
      for column in 0..region_width
      {
         for channel in 0..3
         {
            let mut prefix = vec![0_u32; region_height + 1];
            for row in 0..region_height
            {
               prefix[row + 1] = prefix[row] + u32::from(horizontal[(row * region_width + column) * 3 + channel]);
            }
            for row in 0..region_height
            {
               let start = row.saturating_sub(radius);
               let end = (row + radius + 1).min(region_height);
               let destination = (((top + row) * WIDTH as usize + left + column) * 4) + channel;
               self.pixels[destination] = ((prefix[end] - prefix[start]) / (end - start) as u32) as u8;
            }
         }
      }
   }
}

impl RasterImage
{
   fn channel(&self, x: u32, y: u32, channel: usize) -> u8
   {
      self.pixels[((y * self.width + x) * 4) as usize + channel]
   }
}

fn rounded_contains(x: i32, y: i32, width: i32, height: i32, radius: i32) -> bool
{
   if x < 0 || y < 0 || x >= width || y >= height
   {
      return false;
   }
   let radius = radius.min((width - 1) / 2).min((height - 1) / 2).max(0);
   let center_x = x.clamp(radius, width - radius - 1);
   let center_y = y.clamp(radius, height - radius - 1);
   let distance_x = x - center_x;
   let distance_y = y - center_y;
   distance_x * distance_x + distance_y * distance_y <= radius * radius
}

fn source_coordinate(destination: i32, destination_size: i32, source_size: u32) -> (u32, u32)
{
   let numerator = (2_i64 * i64::from(destination) + 1) * i64::from(source_size) * 65_536;
   let fixed = (numerator / (2 * i64::from(destination_size)) - 32_768).clamp(0, i64::from(source_size.saturating_sub(1)) * 65_536);
   ((fixed >> 16) as u32, (fixed & 65_535) as u32)
}

fn lerp_channel(left: u8, right: u8, weight: u32) -> u8
{
   ((u32::from(left) * (65_536 - weight) + u32::from(right) * weight + 32_768) >> 16) as u8
}

fn inline_text_entry(grapheme: &str, index: u32) -> Value
{
   json!({
      "grapheme": grapheme,
      "column": index % 5,
      "row": index / 5,
      "advance_millionths": 1_000_000,
      "top_from_baseline_millionths": -800_000,
      "width_millionths": 1_000_000,
      "height_millionths": 1_000_000
   })
}

fn write_base64_asset(root: &Path, source_path: &str, destination_path: &str) -> Result<()>
{
   let source = fs::read_to_string(root.join(source_path)).with_context(|| format!("reading base64 asset source {}", root.join(source_path).display()))?;
   let bytes = base64::engine::general_purpose::STANDARD.decode(source.trim()).with_context(|| format!("decoding base64 asset source {source_path}"))?;
   fs::write(root.join(destination_path), bytes).with_context(|| format!("writing decoded asset {}", root.join(destination_path).display()))
}

fn write_inline_text_raster_variant(source_path: &Path, destination_path: &Path, em_pixels: u32) -> Result<()>
{
   let source = read_png(source_path)?;
   ensure!(source.width == 640 && source.height == 256, "inline-text source atlas dimensions changed");
   let width = 5 * em_pixels;
   let height = 2 * em_pixels;
   let mut pixels = vec![0_u8; width as usize * height as usize * 4];
   for cell in 0..10_u32
   {
      for destination_y in 0..em_pixels
      {
         for destination_x in 0..em_pixels
         {
            let mut alpha_weight = 0_u64;
            let mut alpha_sum = 0_u64;
            let mut premultiplied = [0_u64; 3];
            let source_y_start = destination_y * 128 / em_pixels;
            let source_y_end = ((destination_y + 1) * 128 + em_pixels - 1) / em_pixels;
            let source_x_start = destination_x * 128 / em_pixels;
            let source_x_end = ((destination_x + 1) * 128 + em_pixels - 1) / em_pixels;
            for source_y in source_y_start..source_y_end
            {
               let overlap_y = interval_overlap(destination_y * 128, (destination_y + 1) * 128, source_y * em_pixels, (source_y + 1) * em_pixels);
               for source_x in source_x_start..source_x_end
               {
                  let overlap_x = interval_overlap(destination_x * 128, (destination_x + 1) * 128, source_x * em_pixels, (source_x + 1) * em_pixels);
                  let weight = u64::from(overlap_x) * u64::from(overlap_y);
                  let source_offset = ((((cell / 5 * 128 + source_y) * source.width) + cell % 5 * 128 + source_x) * 4) as usize;
                  let alpha = u64::from(source.pixels[source_offset + 3]);
                  alpha_weight += weight;
                  alpha_sum += alpha * weight;
                  for channel in 0..3
                  {
                     premultiplied[channel] += u64::from(source.pixels[source_offset + channel]) * alpha * weight;
                  }
               }
            }
            let destination_offset = ((((cell / 5 * em_pixels + destination_y) * width) + cell % 5 * em_pixels + destination_x) * 4) as usize;
            pixels[destination_offset + 3] = ((alpha_sum + alpha_weight / 2) / alpha_weight) as u8;
            if alpha_sum > 0
            {
               for channel in 0..3
               {
                  pixels[destination_offset + channel] = ((premultiplied[channel] + alpha_sum / 2) / alpha_sum) as u8;
               }
            }
         }
      }
   }
   write_srgb_png(destination_path, width, height, &pixels)
}

fn interval_overlap(left_a: u32, right_a: u32, left_b: u32, right_b: u32) -> u32
{
   right_a.min(right_b).saturating_sub(left_a.max(left_b))
}

fn write_atlas_png(path: &Path) -> Result<()>
{
   let width = 16 * 24;
   let height = 8 * 24;
   let mut pixels = vec![0; width as usize * height as usize * 4];
   for tile in 0..128_u32
   {
      let color = [
         (41 + tile * 37) as u8,
         (97 + tile * 53) as u8,
         (173 + tile * 29) as u8,
         255,
      ];
      for y in tile / 16 * 24..tile / 16 * 24 + 24
      {
         for x in tile % 16 * 24..tile % 16 * 24 + 24
         {
            let offset = ((y * width + x) * 4) as usize;
            pixels[offset..offset + 4].copy_from_slice(&color);
         }
      }
   }
   write_png(path, width, height, &pixels)
}

fn write_golden_png(root: &Path, path: &Path, golden: Golden) -> Result<()>
{
   let atlas = read_png(&root.join("assets/neutral-thumbnail-atlas-v1.png"))?;
   let mut canvas = Canvas::new(BACKGROUND);
   match golden
   {
      Golden::Startup(state) => draw_startup(&mut canvas, &atlas, state),
      Golden::Dashboard(state) => draw_dashboard(&mut canvas, &atlas, state),
      Golden::Feed(state) => draw_feed(&mut canvas, &atlas, state),
      Golden::Chat(state) => draw_chat(&mut canvas, &atlas, state),
      Golden::Navigation(state) => draw_navigation(&mut canvas, state),
      Golden::Image(state) =>
      {
         let image_path = if state == 0 {"assets/image-decode-zoom-thumbnail-v1.png"} else {"assets/image-decode-zoom-source-v1.png"};
         draw_image(&mut canvas, &read_png(&root.join(image_path))?, state);
      }
   }
   write_srgb_png(path, WIDTH, HEIGHT, &canvas.pixels)
}

fn draw_startup(canvas: &mut Canvas, atlas: &RasterImage, _state: u8)
{
   canvas.shadowed_rect(px(16), px(20), px(358), px(48), px(12), SURFACE);
   text_bar(canvas, 24, 33, 176, 8, TEXT);
   canvas.shadowed_rect(px(16), px(76), px(358), px(44), px(12), SURFACE);
   text_bar(canvas, 24, 92, 132, 6, SECONDARY);
   for index in 0..6_i32
   {
      let column = index % 2;
      let row = index / 2;
      let x = 16 + column * 185;
      let y = 132 + row * 188;
      canvas.shadowed_rect(px(x), px(y), px(173), px(176), px(12), SURFACE);
      draw_tile(canvas, atlas, index as u32, [px(x + 12), px(y + 12), px(48), px(48)], px(8));
      text_bar(canvas, x + 64, y + 18, 92, 7, TEXT);
      text_bar(canvas, x + 64, y + 34, 76, 5, SECONDARY);
      text_bar(canvas, x + 12, y + 78, 146, 6, TEXT);
      text_bar(canvas, x + 12, y + 98, 136, 5, SECONDARY);
      text_bar(canvas, x + 12, y + 114, 118, 5, SECONDARY);
   }
   canvas.shadowed_rect(px(16), px(776), px(358), px(48), px(12), ACCENT);
   text_bar(canvas, 130, 796, 130, 7, SURFACE);
}

fn draw_dashboard(canvas: &mut Canvas, atlas: &RasterImage, state: u8)
{
   text_bar(canvas, 16, 22, 200, 8, TEXT);
   for index in 0..32_i32
   {
      let column = index % 2;
      let row = index / 2;
      let x = 16 + column * 185;
      let y = 48 + row * 46;
      canvas.shadowed_rect(px(x), px(y), px(173), px(38), px(12), SURFACE);
      draw_tile(canvas, atlas, (index * 2) as u32, [px(x + 6), px(y + 12), px(14), px(14)], px(4));
      draw_tile(canvas, atlas, (index * 2 + 1) as u32, [px(x + 24), px(y + 12), px(14), px(14)], px(4));
      for label in 0..6
      {
         let change = if state > 0 && (index * 6 + label) % 7 == 3 {i32::from(state) * 3} else {0};
         text_bar(canvas, x + 42, y + 3 + label * 5, 72 + (label % 3) * 10 + change, 2, if label == 0 {TEXT} else {SECONDARY});
      }
      if index < 24
      {
         canvas.rounded_rect(px(x + 145), px(y + 13), px(18), px(12), px(6), ACCENT);
      }
   }
   for y in [64, 268, 472, 676]
   {
      canvas.box_blur(px(16), px(y), px(358), px(96), px(12) as usize);
      canvas.rounded_rect(px(16), px(y), px(358), px(96), px(12), [225, 229, 238, 56]);
   }
}

fn draw_feed(canvas: &mut Canvas, atlas: &RasterImage, state: u8)
{
   canvas.rect(0, 0, WIDTH as i32, px(52), SURFACE);
   text_bar(canvas, 16, 22, 142, 8, TEXT);
   let scroll = match state
   {
      0 => 0,
      1 => 750,
      _ => 150,
   };
   let mut index = 0_i32;
   let mut row_top = 52 - scroll;
   while row_top + feed_row_height(index) <= 52
   {
      row_top += feed_row_height(index);
      index += 1;
   }
   while row_top < 844
   {
      let height = feed_row_height(index);
      canvas.shadowed_rect(px(12), px(row_top + 4), px(366), px(height - 8), px(12), SURFACE);
      draw_tile(canvas, atlas, index as u32 % 128, [px(22), px(row_top + 14), px(48), px(48)], px(8));
      text_bar(canvas, 82, row_top + 17, 210, 7, TEXT);
      text_bar(canvas, 82, row_top + 34, 250, 5, SECONDARY);
      text_bar(canvas, 82, row_top + 49, 190, 5, SECONDARY);
      let favorite = state >= 3 && index == 731;
      canvas.rounded_rect(px(344), px(row_top + 15), px(18), px(18), px(9), if favorite {ACCENT} else {[180, 186, 198, 255]});
      row_top += height;
      index += 1;
   }
}

fn draw_chat(canvas: &mut Canvas, atlas: &RasterImage, state: u8)
{
   canvas.rect(0, 0, WIDTH as i32, px(52), SURFACE);
   text_bar(canvas, 16, 22, 150, 8, TEXT);
   let base = 4_990 + if state >= 1 {50} else {0} + if state >= 2 {20} else {0};
   for visible in 0..10_i32
   {
      let message = base + visible;
      let y = 58 + visible * 68;
      let rtl = message % 8 == 1 || message % 8 == 4;
      let avatar_x = if rtl {342} else {12};
      let bubble_x = if rtl {48} else {56};
      let bubble_width = 286 - (message % 4) * 16;
      draw_tile(canvas, atlas, message as u32 % 64, [px(avatar_x), px(y + 10), px(36), px(36)], px(18));
      canvas.shadowed_rect(px(bubble_x), px(y), px(bubble_width), px(60), px(12), if rtl {RTL_SURFACE} else {SURFACE});
      text_bar(canvas, bubble_x + 10, y + 12, bubble_width - 36, 6, TEXT);
      text_bar(canvas, bubble_x + 10, y + 29, bubble_width - 22, 5, SECONDARY);
      text_bar(canvas, bubble_x + 10, y + 43, bubble_width - 58, 5, SECONDARY);
   }
   canvas.rect(0, px(752), WIDTH as i32, px(92), SURFACE);
   canvas.rounded_rect(px(12), px(780), px(318), px(48), px(12), MUTED);
   let composer_width = match state {3 => 250, 4 | 5 => 294, _ => 80};
   text_bar(canvas, 24, 800, composer_width, 6, SECONDARY);
   canvas.rounded_rect(px(338), px(780), px(40), px(48), px(12), ACCENT);
   text_bar(canvas, 345, 801, 26, 6, SURFACE);
}

fn draw_navigation(canvas: &mut Canvas, state: u8)
{
   text_bar(canvas, 16, 22, 150, 8, TEXT);
   for index in 0..12_i32
   {
      let y = 56 + index * 62;
      canvas.shadowed_rect(px(16), px(y), px(358), px(54), px(12), SURFACE);
      text_bar(canvas, 26, y + 14, 220, 7, TEXT);
      text_bar(canvas, 26, y + 33, 140, 5, SECONDARY);
   }
   if state > 0
   {
      canvas.rect(0, 0, WIDTH as i32, HEIGHT as i32, BACKGROUND);
      text_bar(canvas, 16, 22, 48, 7, ACCENT);
      text_bar(canvas, 82, 22, 180, 8, TEXT);
      canvas.shadowed_rect(px(16), px(64), px(358), px(180), px(12), SURFACE);
      text_bar(canvas, 28, 84, 280, 7, TEXT);
      text_bar(canvas, 28, 108, 310, 5, SECONDARY);
      let quarter = i32::from(state - 1);
      let overlay_alpha = (176 * quarter / 4) as u8;
      canvas.rect(0, 0, WIDTH as i32, HEIGHT as i32, [25, 28, 35, overlay_alpha]);
      let modal_x = 24 + (4 - quarter) * 390 / 4;
      canvas.shadowed_rect(px(modal_x), px(132), px(342), px(580), px(16), SURFACE);
      text_bar(canvas, modal_x + 18, 156, 190, 8, TEXT);
      text_bar(canvas, modal_x + 18, 186, 270, 6, SECONDARY);
      text_bar(canvas, modal_x + 18, 205, 240, 6, SECONDARY);
      canvas.rounded_rect(px(modal_x + 268), px(150), px(50), px(28), px(10), ACCENT);
      text_bar(canvas, modal_x + 279, 161, 28, 6, SURFACE);
   }
}

fn draw_image(canvas: &mut Canvas, image: &RasterImage, state: u8)
{
   canvas.rect(0, 0, WIDTH as i32, px(52), SURFACE);
   text_bar(canvas, 18, 22, 204, 8, TEXT);
   canvas.rect(0, px(52), WIDTH as i32, px(740), MUTED);
   let destination = match state
   {
      0 => [px(16), px(68), px(96), px(72)],
      1 => [0, 827, WIDTH as i32, 878],
      2 => [-263, 827, WIDTH as i32, 878],
      _ => [-1_112, 389, 2_340, 1_755],
   };
   canvas.image(image, [0, 0, image.width, image.height], destination, [0, px(52), WIDTH as i32, px(740)], if state == 0 {px(8)} else {0});
   canvas.rounded_rect(px(16), px(800), px(358), px(28), px(12), SURFACE);
   text_bar(canvas, 28, 810, 270, 6, SECONDARY);
}

fn feed_row_height(index: i32) -> i32
{
   68 + index * 17 % 53
}

fn draw_tile(canvas: &mut Canvas, atlas: &RasterImage, tile: u32, destination: [i32; 4], radius: i32)
{
   let tile = tile % 128;
   canvas.image(atlas, [(tile % 16) * 24, (tile / 16) * 24, 24, 24], destination, [0, 0, WIDTH as i32, HEIGHT as i32], radius);
}

fn text_bar(canvas: &mut Canvas, x: i32, y: i32, width: i32, height: i32, color: [u8; 4])
{
   canvas.rounded_rect(px(x), px(y), px(width), px(height), px(height) / 2, color);
}

fn read_png(path: &Path) -> Result<RasterImage>
{
   let file = fs::File::open(path).with_context(|| format!("opening pinned PNG {}", path.display()))?;
   let mut decoder = png::Decoder::new(file);
   decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
   let mut reader = decoder.read_info().with_context(|| format!("reading pinned PNG header {}", path.display()))?;
   let mut buffer = vec![0; reader.output_buffer_size()];
   let output = reader.next_frame(&mut buffer).with_context(|| format!("decoding pinned PNG {}", path.display()))?;
   let bytes = &buffer[..output.buffer_size()];
   let mut pixels = Vec::with_capacity(output.width as usize * output.height as usize * 4);
   match output.color_type
   {
      png::ColorType::Rgba => pixels.extend_from_slice(bytes),
      png::ColorType::Rgb =>
      {
         for pixel in bytes.chunks_exact(3)
         {
            pixels.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
         }
      }
      png::ColorType::Grayscale =>
      {
         for value in bytes
         {
            pixels.extend_from_slice(&[*value, *value, *value, 255]);
         }
      }
      png::ColorType::GrayscaleAlpha =>
      {
         for pixel in bytes.chunks_exact(2)
         {
            pixels.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
         }
      }
      png::ColorType::Indexed => anyhow::bail!("pinned PNG remained indexed after expansion: {}", path.display()),
   }
   Ok(RasterImage {width: output.width, height: output.height, pixels})
}

fn write_deterministic_image_png(path: &Path, width: u32, height: u32) -> Result<()>
{
   let mut pixels = vec![0; width as usize * height as usize * 3];
   for y in 0..height
   {
      for x in 0..width
      {
         let tile_x = x / 64;
         let tile_y = y / 64;
         let offset = ((y * width + x) * 3) as usize;
         pixels[offset] = (tile_x * 19 + tile_y * 7 + x % 16) as u8;
         pixels[offset + 1] = (tile_x * 5 + tile_y * 23 + y % 16) as u8;
         pixels[offset + 2] = (tile_x * 11 + tile_y * 13 + (x + y) % 16) as u8;
      }
   }
   write_rgb_png(path, width, height, &pixels)
}

fn write_rgb_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<()>
{
   ensure!(pixels.len() == width as usize * height as usize * 3, "RGB PNG byte length differs from declared dimensions");
   let parent = path.parent().context("PNG artifact has no parent")?;
   fs::create_dir_all(parent).with_context(|| format!("creating PNG directory {}", parent.display()))?;
   let file = fs::File::create(path).with_context(|| format!("creating PNG artifact {}", path.display()))?;
   let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
   encoder.set_color(png::ColorType::Rgb);
   encoder.set_depth(png::BitDepth::Eight);
   encoder.set_compression(png::Compression::Best);
   encoder.set_filter(png::FilterType::Paeth);
   let mut writer = encoder.write_header().context("writing RGB PNG header")?;
   writer.write_image_data(pixels).context("writing RGB PNG pixels")?;
   writer.finish().context("finishing RGB PNG artifact")?;
   Ok(())
}

fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<()>
{
   ensure!(pixels.len() == width as usize * height as usize * 4, "PNG byte length differs from declared dimensions");
   let parent = path.parent().context("PNG artifact has no parent")?;
   fs::create_dir_all(parent).with_context(|| format!("creating PNG directory {}", parent.display()))?;
   let file = fs::File::create(path).with_context(|| format!("creating PNG artifact {}", path.display()))?;
   let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
   encoder.set_color(png::ColorType::Rgba);
   encoder.set_depth(png::BitDepth::Eight);
   let mut writer = encoder.write_header().context("writing PNG header")?;
   writer.write_image_data(pixels).context("writing PNG pixels")?;
   writer.finish().context("finishing PNG artifact")?;
   Ok(())
}

fn write_srgb_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<()>
{
   ensure!(pixels.len() == width as usize * height as usize * 4, "sRGB PNG byte length differs from declared dimensions");
   let parent = path.parent().context("sRGB PNG artifact has no parent")?;
   fs::create_dir_all(parent).with_context(|| format!("creating sRGB PNG directory {}", parent.display()))?;
   let file = fs::File::create(path).with_context(|| format!("creating sRGB PNG artifact {}", path.display()))?;
   let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
   encoder.set_color(png::ColorType::Rgba);
   encoder.set_depth(png::BitDepth::Eight);
   encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
   encoder.set_compression(png::Compression::Best);
   encoder.set_filter(png::FilterType::Paeth);
   let mut writer = encoder.write_header().context("writing sRGB PNG header")?;
   writer.write_image_data(pixels).context("writing sRGB PNG pixels")?;
   writer.finish().context("finishing sRGB PNG artifact")?;
   Ok(())
}
