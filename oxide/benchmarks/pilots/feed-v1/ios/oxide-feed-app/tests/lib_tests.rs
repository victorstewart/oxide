use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use oxide_feed_v1_app::contract::{self, StartState};
use oxide_feed_v1_app::{FeedV1App, FeedV1Phase};
use oxide_platform_api::{
   App,
   AppEvent,
   FrameContext,
   FrameDemand,
   HapticPattern,
   Haptics,
   InputEvent,
   Lifecycle,
   RendererStats,
   Timers,
   TouchEvent,
   TouchId,
   TouchPhase,
   UpdateContext,
};
use oxide_renderer_api::{
   Color,
   DrawCmd,
   ImageHandle,
   ImageSampling,
   RectF,
   RuntimeImageUploader,
};
use serde_json::Value;

static ENVIRONMENT_LOCK: Mutex<()> = Mutex::new(());
static TEMPORARY_HOME_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
struct RgbaUpload
{
   width: u32,
   height: u32,
   row_bytes: usize,
   sampling: ImageSampling,
   bytes: Vec<u8>,
}

#[derive(Default)]
struct UploadProbe
{
   next_handle: u32,
   a8_creates: usize,
   a8_updates: usize,
   rgba_uploads: Vec<RgbaUpload>,
}

impl RuntimeImageUploader for UploadProbe
{
   fn create_a8(&mut self, _width: u32, _height: u32, _data: &[u8], _row_bytes: usize) -> ImageHandle
   {
      self.a8_creates = self.a8_creates.saturating_add(1);
      self.handle()
   }

   fn try_create_rgba8_sampled(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize, sampling: ImageSampling) -> Option<ImageHandle>
   {
      self.rgba_uploads.push(RgbaUpload {
         width,
         height,
         row_bytes,
         sampling,
         bytes: data.to_vec(),
      });
      Some(self.handle())
   }

   fn update_a8(&mut self, _handle: ImageHandle, _x: u32, _y: u32, _width: u32, _height: u32, _data: &[u8], _row_bytes: usize)
   {
      self.a8_updates = self.a8_updates.saturating_add(1);
   }
}

impl UploadProbe
{
   fn handle(&mut self) -> ImageHandle
   {
      self.next_handle = self.next_handle.saturating_add(1);
      ImageHandle(self.next_handle)
   }
}

struct NoHaptics;

impl Haptics for NoHaptics
{
   fn play(&self, _pattern: HapticPattern)
   {
   }
}

struct EnvironmentScope
{
   prior: Vec<(&'static str, Option<OsString>)>,
}

impl EnvironmentScope
{
   fn new(state: StartState, nonce: &str, home: Option<&Path>) -> Self
   {
      let values = [
         (contract::TREATMENT_ENVIRONMENT_KEY, OsString::from(contract::OXIDE_VARIANT_VALUE)),
         (contract::START_STATE_ENVIRONMENT_KEY, OsString::from(state.as_str())),
         (contract::COMPLETION_NONCE_ENVIRONMENT_KEY, OsString::from(nonce)),
         ("OXIDE_FEED_V1_PHASE", OsString::from("smoke")),
         ("OXIDE_FEED_V1_SESSION_INDEX", OsString::from("0")),
         ("OXIDE_FEED_V1_PAIR_INDEX", OsString::from("0")),
         ("OXIDE_FEED_V1_ORDER_INDEX", OsString::from("0")),
      ];
      let mut prior = Vec::with_capacity(values.len() + usize::from(home.is_some()));
      for (name, value) in values
      {
         prior.push((name, std::env::var_os(name)));
         std::env::set_var(name, value);
      }
      if let Some(home) = home
      {
         prior.push(("HOME", std::env::var_os("HOME")));
         std::env::set_var("HOME", home);
      }
      Self { prior }
   }
}

impl Drop for EnvironmentScope
{
   fn drop(&mut self)
   {
      for (name, value) in self.prior.drain(..).rev()
      {
         if let Some(value) = value
         {
            std::env::set_var(name, value);
         }
         else
         {
            std::env::remove_var(name);
         }
      }
   }
}

struct TemporaryHome
{
   path: PathBuf,
}

impl TemporaryHome
{
   fn new() -> Self
   {
      let sequence = TEMPORARY_HOME_COUNTER.fetch_add(1, Ordering::Relaxed);
      let path = std::env::temp_dir().join(format!(
         "oxide-feed-v1-app-tests-{}-{sequence}",
         std::process::id(),
      ));
      assert!(fs::create_dir(&path).is_ok(), "could not create isolated test HOME");
      Self { path }
   }

   fn path(&self) -> &Path
   {
      &self.path
   }
}

impl Drop for TemporaryHome
{
   fn drop(&mut self)
   {
      let _ = fs::remove_dir_all(&self.path);
   }
}

#[test]
fn ordinary_builds_are_rlib_only_and_the_device_build_explicitly_requests_staticlib()
{
   let manifest = include_str!("../Cargo.toml");
   assert!(manifest.contains("crate-type = [\"rlib\"]"));
   assert!(!manifest.contains("\"staticlib\""));

   let project = include_str!("../../device-pilot/project.yml");
   assert!(project.contains(
      "cargo rustc --locked --profile feed-v1-device --target aarch64-apple-ios --manifest-path \"${MANIFEST}\" --lib --crate-type staticlib",
   ));
   assert!(!project.contains(
      "cargo build --locked --release --target aarch64-apple-ios --manifest-path \"${MANIFEST}\"",
   ));
}

#[test]
fn feed_crates_share_the_root_workspace_without_entering_default_builds()
{
   let workspace = include_str!("../../../../../../Cargo.toml");
   let lines: Vec<&str> = workspace.lines().map(str::trim).collect();
   let workspace_array = |heading: &str|
   {
      let start = lines.iter().position(|line| *line == heading)
         .expect("root workspace array heading") + 1;
      let end = start + lines[start..].iter().position(|line| *line == "]")
         .expect("root workspace array closing bracket");
      &lines[start..end]
   };
   let members = workspace_array("members = [");
   let default_members = workspace_array("default-members = [");
   for member in [
      "benchmarks/pilots/feed-v1/ios/oxide-feed-app",
      "benchmarks/pilots/feed-v1/reducer",
   ]
   {
      let entry = format!("\"{member}\",");
      assert!(members.contains(&entry.as_str()));
      assert!(!default_members.contains(&entry.as_str()));
   }
   assert!(workspace.contains(
      "[profile.feed-v1-device.package.\"oxide-host-ios\"]\nopt-level = 3",
   ));
   assert!(workspace.contains(
      "[profile.feed-v1-reducer]\ninherits = \"release\"\npanic = \"unwind\"",
   ));

   let app_manifest = include_str!("../Cargo.toml");
   let reducer_manifest = include_str!("../../../reducer/Cargo.toml");
   for manifest in [app_manifest, reducer_manifest]
   {
      assert!(!manifest.contains("\n[workspace]"));
      assert!(!manifest.contains("\n[profile."));
   }

   let app_root = Path::new(env!("CARGO_MANIFEST_DIR"));
   assert!(!app_root.join("Cargo.lock").exists());
   assert!(!app_root.join("../../reducer/Cargo.lock").exists());

   let runner = include_str!("../../device-pilot/run-device.sh");
   assert!(runner.contains("cargo build --locked --profile feed-v1-reducer"));
   assert!(runner.contains("$REDUCER_TARGET/feed-v1-reducer/oxide-feed-v1-reducer"));
}

#[test]
fn complete_fixture_identity_is_a_host_preflight_not_runtime_startup_work()
{
   let app = include_str!("../src/lib.rs");
   assert!(!app.contains("canonical_fixture_identity(&fixture)"));

   let runner = include_str!("../../device-pilot/run-device.sh");
   assert!(runner.contains("FeedV1ContractCheckMain.swift"));
   assert!(runner.contains("rust_recipe_reproduces_the_complete_swift_canonical_identity"));
   assert!(runner.contains("--locked"));
}

#[test]
fn frozen_app_starts_at_each_exact_contract_offset()
{
   let _lock = lock_environment();
   let _top_environment = EnvironmentScope::new(StartState::Top, "test-top-offset", None);
   let top = FeedV1App::from_environment();
   assert_eq!(top.status().content_offset_points, 0.0);
   drop(_top_environment);

   let _bottom_environment = EnvironmentScope::new(StartState::Bottom, "test-bottom-offset", None);
   let bottom = FeedV1App::from_environment();
   assert_eq!(
      bottom.status().content_offset_points,
      contract::MAXIMUM_CONTENT_OFFSET_POINTS as f32,
   );
}

#[test]
fn measurement_begins_at_the_first_drag_offset_change()
{
   let _lock = lock_environment();
   let _environment = EnvironmentScope::new(StartState::Top, "test-drag-boundary", None);
   let mut app = FeedV1App::from_environment();
   let mut uploader = UploadProbe::default();
   admit_ready(&mut app, &mut uploader);

   dispatch(&mut app, AppEvent::Input(InputEvent::Touch(touch(TouchPhase::Start, 1_000_000_000, 700.0))));
   assert_eq!(app.status().phase, FeedV1Phase::TouchPending);
   assert_eq!(app.status().gesture_start_timestamp_ns, 0);

   dispatch(&mut app, AppEvent::Input(InputEvent::Touch(touch(TouchPhase::Move, 1_010_000_000, 697.0))));
   assert_eq!(app.status().phase, FeedV1Phase::TouchPending);
   assert_eq!(app.status().content_offset_points, 0.0);

   dispatch(&mut app, AppEvent::Input(InputEvent::Touch(touch(TouchPhase::Move, 1_020_000_000, 693.0))));
   let status = app.status();
   assert_eq!(status.phase, FeedV1Phase::Gesture);
   assert_eq!(status.gesture_start_timestamp_ns, 1_020_000_000);
   assert_eq!(status.callback_sample_count, 0);
   assert!(status.content_offset_points > 0.0);
}

#[test]
fn a_touch_that_never_drags_returns_to_ready()
{
   let _lock = lock_environment();
   let _environment = EnvironmentScope::new(StartState::Top, "test-tap-ready", None);
   let mut app = FeedV1App::from_environment();
   let mut uploader = UploadProbe::default();
   admit_ready(&mut app, &mut uploader);

   dispatch(&mut app, AppEvent::Input(InputEvent::Touch(touch(TouchPhase::Start, 1_000_000_000, 700.0))));
   dispatch(&mut app, AppEvent::Input(InputEvent::Touch(touch(TouchPhase::End, 1_050_000_000, 700.0))));
   assert_eq!(app.status().phase, FeedV1Phase::Ready);
   assert_eq!(app.status().gesture_start_timestamp_ns, 0);
   assert_eq!(app.status().content_offset_points, 0.0);
}

#[test]
fn checker_uploads_use_only_frozen_nearest_sampled_source_bytes()
{
   let _lock = lock_environment();
   let _environment = EnvironmentScope::new(StartState::Top, "test-checker-uploads", None);
   let mut app = FeedV1App::from_environment();
   let mut uploader = UploadProbe::default();
   assert_eq!(App::prepare_frame(&mut app, frame(1, 1_000_000_000), &mut uploader), FrameDemand::NextVsync);
   assert!(!uploader.rgba_uploads.is_empty());
   for upload in &uploader.rgba_uploads
   {
      assert_eq!(upload.width, contract::CHECKER_SIDE_PIXELS as u32);
      assert_eq!(upload.height, contract::CHECKER_SIDE_PIXELS as u32);
      assert_eq!(upload.row_bytes, contract::CHECKER_SIDE_PIXELS * 4);
      assert_eq!(upload.sampling, ImageSampling::Nearest);
      assert_eq!(upload.bytes.len(), contract::CHECKER_RGBA_BYTE_COUNT);
      assert!(frozen_checker_payload(&upload.bytes));
   }
}

#[test]
fn text_atlas_publication_is_coalesced_once_per_frame()
{
   let _lock = lock_environment();
   let _environment = EnvironmentScope::new(StartState::Top, "test-text-frame-publication", None);
   let mut app = FeedV1App::from_environment();
   let mut uploader = UploadProbe::default();

   assert_eq!(App::prepare_frame(&mut app, frame(1, 1_000_000_000), &mut uploader), FrameDemand::NextVsync);
   assert_eq!(uploader.a8_creates, 1);
   assert_eq!(uploader.a8_updates, 0, "cold glyphs must publish with the single atlas creation");

   assert_eq!(App::prepare_frame(&mut app, frame(2, 1_008_333_333), &mut uploader), FrameDemand::Idle);
   assert_eq!(uploader.a8_creates, 1);
   assert_eq!(uploader.a8_updates, 0, "an unchanged warm frame must not republish glyph pixels");
}

#[test]
fn runtime_text_adapter_preserves_append_and_release_operations()
{
   let source = include_str!("../src/lib.rs");
   let start = source
      .find("impl ImageUploader for RuntimeTextUploader<'_>")
      .expect("runtime text adapter implementation");
   let tail = &source[start..];
   let end = tail.find("\n}\n\nstruct FeedRenderer").expect("runtime text adapter boundary");
   let implementation = &tail[..end];

   assert!(implementation.contains("self.uploader.append_a8("));
   assert!(implementation.contains("self.uploader.release_a8(handle);"));
}

#[test]
fn rendered_frame_colors_round_trip_the_frozen_srgb_bytes()
{
   let _lock = lock_environment();
   let _environment = EnvironmentScope::new(StartState::Top, "test-frame-colors", None);
   let mut app = FeedV1App::from_environment();
   let mut uploader = UploadProbe::default();
   assert_eq!(App::prepare_frame(&mut app, frame(1, 1_000_000_000), &mut uploader), FrameDemand::NextVsync);
   let Some(prepared) = App::prepared_frame(&app) else
   {
      panic!("feed app did not expose its prepared frame");
   };
   let mut colors = Vec::new();
   for command in &prepared.draw_list.items
   {
      match command
      {
         DrawCmd::RRect { color, .. } => colors.push(srgb_bytes(*color)),
         DrawCmd::GlyphRun { run } => colors.push(srgb_bytes(run.color)),
         _ => {}
      }
   }
   for expected in [
      contract::BACKGROUND,
      contract::TITLE_COLOR,
      contract::CAPTION_COLOR,
      contract::METADATA_COLOR,
      contract::SEPARATOR_COLOR,
      contract::SHADOW_COLOR,
   ]
   {
      assert!(colors.contains(&[expected.red, expected.green, expected.blue, expected.alpha]));
   }
}

#[test]
fn ready_frame_exposes_frozen_damage_clip_and_image_geometry()
{
   let _lock = lock_environment();
   let _environment = EnvironmentScope::new(StartState::Top, "test-ready-geometry", None);
   let mut app = FeedV1App::from_environment();
   let mut uploader = UploadProbe::default();
   assert_eq!(App::prepare_frame(&mut app, frame(1, 1_000_000_000), &mut uploader), FrameDemand::NextVsync);
   let Some(prepared) = App::prepared_frame(&app) else
   {
      panic!("feed app did not expose its prepared frame");
   };
   assert_eq!(prepared.damage.len(), 1);
   assert_eq!(
      prepared.damage[0],
      oxide_renderer_api::RectI::new(
         0,
         0,
         contract::HOST_WIDTH_POINTS as i32,
         contract::HOST_HEIGHT_POINTS as i32,
      ),
   );
   assert!(matches!(
      prepared.draw_list.items.first(),
      Some(DrawCmd::RRect { rect, color, .. })
         if *rect == RectF::new(
            contract::SURFACE_ORIGIN_X_POINTS as f32,
            contract::SURFACE_ORIGIN_Y_POINTS as f32,
            contract::SURFACE_WIDTH_POINTS as f32,
            contract::SURFACE_HEIGHT_POINTS as f32,
         ) && srgb_bytes(*color) == [
            contract::BACKGROUND.red,
            contract::BACKGROUND.green,
            contract::BACKGROUND.blue,
            contract::BACKGROUND.alpha,
         ]
   ));
   assert!(prepared.draw_list.items.iter().any(|command|
   {
      matches!(
         command,
         DrawCmd::ClipPush { rect }
            if *rect == oxide_renderer_api::RectI::new(
               contract::SURFACE_ORIGIN_X_POINTS as i32,
               contract::SURFACE_ORIGIN_Y_POINTS as i32,
               contract::SURFACE_WIDTH_POINTS as i32,
               contract::SURFACE_HEIGHT_POINTS as i32,
            )
      )
   }));
   let Some(DrawCmd::ImageMesh { vb, .. }) = prepared
      .draw_list
      .items
      .iter()
      .find(|command| matches!(command, DrawCmd::ImageMesh { .. }))
   else
   {
      panic!("ready frame has no checker image mesh");
   };
   let vertices = &prepared.draw_list.vertices[vb.offset as usize..(vb.offset + vb.len) as usize];
   let bounds = vertex_bounds(vertices);
   assert_eq!(
      bounds,
      RectF::new(
         (contract::SURFACE_ORIGIN_X_POINTS + contract::ROW_LEADING_POINTS) as f32,
         (contract::SURFACE_ORIGIN_Y_POINTS + contract::ROW_TOP_POINTS) as f32,
         contract::IMAGE_SIDE_POINTS as f32,
         contract::IMAGE_SIDE_POINTS as f32,
      ),
   );
}

#[test]
fn failed_submissions_retry_the_immutable_frame_under_the_latest_frame_id()
{
   let _lock = lock_environment();
   let _environment = EnvironmentScope::new(StartState::Top, "test-ready-retry", None);
   let mut app = FeedV1App::from_environment();
   let mut uploader = UploadProbe::default();
   assert_eq!(App::prepare_frame(&mut app, frame(1, 1_000_000_000), &mut uploader), FrameDemand::NextVsync);
   assert_eq!(App::prepare_frame(&mut app, frame(2, 1_008_333_333), &mut uploader), FrameDemand::Idle);
   assert_eq!(app.status().phase, FeedV1Phase::AwaitingReadySubmit);
   assert_eq!(app.status().pending_submit_frame_id, Some(2));
   let before = prepared_draw_list(&app);

   assert_eq!(App::prepare_frame(&mut app, frame(3, 1_016_666_666), &mut uploader), FrameDemand::Idle);
   assert_eq!(app.status().pending_submit_frame_id, Some(3));
   assert_eq!(prepared_draw_list(&app), before);
   dispatch(&mut app, AppEvent::RendererStats(renderer_stats(3)));
   assert_eq!(app.status().phase, FeedV1Phase::Ready);
}

#[test]
fn settlement_waits_for_a_closing_frame_and_retries_its_exact_submit()
{
   let _lock = lock_environment();
   let _environment = EnvironmentScope::new(StartState::Top, "test-settlement-submit", None);
   let mut app = FeedV1App::from_environment();
   let mut uploader = UploadProbe::default();
   admit_ready(&mut app, &mut uploader);
   begin_short_settled_drag(&mut app);

   assert_eq!(App::prepare_frame(&mut app, frame(3, 1_400_000_000), &mut uploader), FrameDemand::NextVsync);
   assert_eq!(app.status().phase, FeedV1Phase::SettledFramePrepared);
   assert_eq!(app.status().callback_sample_count, 1);
   assert_eq!(App::prepare_frame(&mut app, frame(4, 1_408_333_333), &mut uploader), FrameDemand::Idle);
   assert_eq!(app.status().phase, FeedV1Phase::AwaitingCompletionSubmit);
   assert_eq!(app.status().callback_sample_count, 2);
   assert_eq!(app.status().pending_submit_frame_id, Some(4));
   let closing_frame = prepared_draw_list(&app);

   assert_eq!(App::prepare_frame(&mut app, frame(5, 1_416_666_666), &mut uploader), FrameDemand::Idle);
   assert_eq!(app.status().pending_submit_frame_id, Some(5));
   assert_eq!(prepared_draw_list(&app), closing_frame);
}

#[test]
fn completed_inertial_run_requires_zero_environment_transition_deltas()
{
   let _lock = lock_environment();
   let home = TemporaryHome::new();
   let nonce = "test-zero-environment-transitions";
   let _environment = EnvironmentScope::new(StartState::Top, nonce, Some(home.path()));
   let mut app = FeedV1App::from_environment();
   let mut uploader = UploadProbe::default();
   admit_ready(&mut app, &mut uploader);
   begin_inertial_drag(&mut app);

   let completion_frame_id = prepare_until_completion_submit(&mut app, &mut uploader);
   dispatch(&mut app, AppEvent::RendererStats(renderer_stats(completion_frame_id)));
   assert_eq!(app.status().phase, FeedV1Phase::Finished);

   let record = persisted_record(home.path(), nonce);
   assert_eq!(record["gesture"]["inertia_observed"], true);
   assert_eq!(record["environment"]["thermal_state_change_count"], 0);
   assert_eq!(record["environment"]["low_power_mode_change_count"], 0);
}

#[test]
fn noninertial_drag_cannot_emit_a_complete_record()
{
   let _lock = lock_environment();
   let home = TemporaryHome::new();
   let nonce = "test-noninertial-rejected";
   let _environment = EnvironmentScope::new(StartState::Top, nonce, Some(home.path()));
   let mut app = FeedV1App::from_environment();
   let mut uploader = UploadProbe::default();
   admit_ready(&mut app, &mut uploader);
   begin_short_settled_drag(&mut app);

   assert_eq!(App::prepare_frame(&mut app, frame(3, 1_400_000_000), &mut uploader), FrameDemand::NextVsync);
   assert_eq!(App::prepare_frame(&mut app, frame(4, 1_408_333_333), &mut uploader), FrameDemand::Idle);
   dispatch(&mut app, AppEvent::RendererStats(renderer_stats(4)));
   assert_eq!(app.status().phase, FeedV1Phase::Finished);
   assert_eq!(failure_stage(home.path(), nonce), "gesture");
}

#[test]
fn render_failures_are_labeled_by_the_phase_that_observed_them()
{
   let _lock = lock_environment();
   let home = TemporaryHome::new();
   let _initial_environment = EnvironmentScope::new(
      StartState::Top,
      "test-initial-render-failure",
      Some(home.path()),
   );
   let mut initial = FeedV1App::from_environment();
   let mut uploader = UploadProbe::default();
   assert_eq!(
      App::prepare_frame(&mut initial, mismatched_frame(1, 1_000_000_000), &mut uploader),
      FrameDemand::Idle,
   );
   assert_eq!(initial.status().phase, FeedV1Phase::Finished);
   assert_eq!(failure_stage(home.path(), "test-initial-render-failure"), "initial-admission");
   drop(_initial_environment);

   let _gesture_environment = EnvironmentScope::new(
      StartState::Top,
      "test-gesture-render-failure",
      Some(home.path()),
   );
   let mut gesture = FeedV1App::from_environment();
   let mut uploader = UploadProbe::default();
   admit_ready(&mut gesture, &mut uploader);
   begin_short_settled_drag(&mut gesture);
   assert_eq!(
      App::prepare_frame(&mut gesture, mismatched_frame(3, 1_400_000_000), &mut uploader),
      FrameDemand::NextVsync,
   );
   assert_eq!(
      App::prepare_frame(&mut gesture, frame(4, 1_408_333_333), &mut uploader),
      FrameDemand::Idle,
   );
   dispatch(&mut gesture, AppEvent::RendererStats(renderer_stats(4)));
   assert_eq!(gesture.status().phase, FeedV1Phase::Finished);
   assert_eq!(failure_stage(home.path(), "test-gesture-render-failure"), "gesture");
}

#[test]
fn lifecycle_exit_is_stage_correct_while_waiting_for_submit_or_drag()
{
   let _lock = lock_environment();
   let home = TemporaryHome::new();
   let _ready_environment = EnvironmentScope::new(
      StartState::Top,
      "test-ready-lifecycle-exit",
      Some(home.path()),
   );
   let mut ready = FeedV1App::from_environment();
   let mut uploader = UploadProbe::default();
   assert_eq!(App::prepare_frame(&mut ready, frame(1, 1_000_000_000), &mut uploader), FrameDemand::NextVsync);
   assert_eq!(App::prepare_frame(&mut ready, frame(2, 1_008_333_333), &mut uploader), FrameDemand::Idle);
   dispatch(&mut ready, AppEvent::Lifecycle(Lifecycle::WillTerminate));
   assert_eq!(failure_stage(home.path(), "test-ready-lifecycle-exit"), "initial-admission");
   drop(_ready_environment);

   let _drag_environment = EnvironmentScope::new(
      StartState::Top,
      "test-drag-lifecycle-exit",
      Some(home.path()),
   );
   let mut drag = FeedV1App::from_environment();
   let mut uploader = UploadProbe::default();
   admit_ready(&mut drag, &mut uploader);
   dispatch(&mut drag, AppEvent::Input(InputEvent::Touch(touch(TouchPhase::Start, 1_000_000_000, 700.0))));
   dispatch(&mut drag, AppEvent::Lifecycle(Lifecycle::DidEnterBackground));
   assert_eq!(failure_stage(home.path(), "test-drag-lifecycle-exit"), "gesture");
}

fn lock_environment() -> MutexGuard<'static, ()>
{
   ENVIRONMENT_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn admit_ready(app: &mut FeedV1App, uploader: &mut UploadProbe)
{
   assert_eq!(App::prepare_frame(app, frame(1, 1_000_000_000), uploader), FrameDemand::NextVsync);
   assert_eq!(App::prepare_frame(app, frame(2, 1_008_333_333), uploader), FrameDemand::Idle);
   assert_eq!(app.status().pending_submit_frame_id, Some(2));
   dispatch(app, AppEvent::RendererStats(renderer_stats(2)));
   assert_eq!(app.status().phase, FeedV1Phase::Ready);
   assert!(!app.status().inertia_observed);
}

fn begin_short_settled_drag(app: &mut FeedV1App)
{
   dispatch(app, AppEvent::Input(InputEvent::Touch(touch(TouchPhase::Start, 1_100_000_000, 700.0))));
   dispatch(app, AppEvent::Input(InputEvent::Touch(touch(TouchPhase::Move, 1_110_000_000, 690.0))));
   dispatch(app, AppEvent::Input(InputEvent::Touch(touch(TouchPhase::End, 1_300_000_000, 690.0))));
   assert_eq!(app.status().phase, FeedV1Phase::Gesture);
   assert!(!app.status().inertia_observed);
}

fn begin_inertial_drag(app: &mut FeedV1App)
{
   dispatch(app, AppEvent::Input(InputEvent::Touch(touch(TouchPhase::Start, 1_100_000_000, 700.0))));
   dispatch(app, AppEvent::Input(InputEvent::Touch(touch(TouchPhase::Move, 1_110_000_000, 690.0))));
   dispatch(app, AppEvent::Input(InputEvent::Touch(touch(TouchPhase::Move, 1_120_000_000, 680.0))));
   dispatch(app, AppEvent::Input(InputEvent::Touch(touch(TouchPhase::End, 1_130_000_000, 670.0))));
   assert_eq!(app.status().phase, FeedV1Phase::Gesture);
   assert!(app.status().inertia_observed);
}

fn prepare_until_completion_submit(app: &mut FeedV1App, uploader: &mut UploadProbe) -> u64
{
   let mut frame_id = 3_u64;
   let mut timestamp_ns = 1_140_000_000_u64;
   for _ in 0..768
   {
      let demand = App::prepare_frame(app, frame(frame_id, timestamp_ns), uploader);
      if app.status().phase == FeedV1Phase::AwaitingCompletionSubmit
      {
         assert_eq!(demand, FrameDemand::Idle);
         return frame_id;
      }
      assert_eq!(demand, FrameDemand::NextVsync);
      frame_id = frame_id.saturating_add(1);
      timestamp_ns = timestamp_ns.saturating_add(8_333_333);
   }
   panic!("inertial feed gesture did not reach its completion submit boundary");
}

fn dispatch(app: &mut FeedV1App, event: AppEvent)
{
   let mut context = UpdateContext {
      post_task: Box::new(|_| {}),
      timers: Timers,
      haptics: Box::new(NoHaptics),
   };
   App::event(app, event, &mut context);
}

fn renderer_stats(frame_id: u64) -> RendererStats
{
   RendererStats {
      frame_id,
      encode_ms: 0.0,
      damage_pct: 100.0,
      damage_rects: 1,
      draws: 1,
      sample_count: 1,
      hdr: false,
   }
}

fn frame(frame_id: u64, timestamp_ns: u64) -> FrameContext
{
   FrameContext {
      frame_id,
      timestamp_ns,
      target_timestamp_ns: timestamp_ns.saturating_add(8_333_333),
      dt_ns: 8_333_333,
      viewport: RectF::new(
         0.0,
         0.0,
         contract::HOST_WIDTH_POINTS as f32,
         contract::HOST_HEIGHT_POINTS as f32,
      ),
      scale: contract::SURFACE_SCALE as f32,
   }
}

fn mismatched_frame(frame_id: u64, timestamp_ns: u64) -> FrameContext
{
   let mut frame = frame(frame_id, timestamp_ns);
   frame.viewport.w -= 1.0;
   frame
}

fn touch(phase: TouchPhase, timestamp_ns: u64, y: f32) -> TouchEvent
{
   TouchEvent {
      id: TouchId(1),
      phase,
      timestamp_ns,
      x: contract::SURFACE_ORIGIN_X_POINTS as f32 + 20.0,
      y,
      pressure: None,
      tilt: None,
      device: oxide_platform_api::PointerDevice::Finger,
   }
}

fn frozen_checker_payload(bytes: &[u8]) -> bool
{
   let mut expected = [0; contract::CHECKER_RGBA_BYTE_COUNT];
   (0..contract::CHECKER_VARIANT_COUNT).any(|variant|
   {
      contract::checker_rgba_bytes(variant, &mut expected) && bytes == expected
   })
}

fn srgb_bytes(color: Color) -> [u8; 4]
{
   [
      linear_channel_to_srgb_byte(color.r),
      linear_channel_to_srgb_byte(color.g),
      linear_channel_to_srgb_byte(color.b),
      (color.a * 255.0).round() as u8,
   ]
}

fn linear_channel_to_srgb_byte(linear: f32) -> u8
{
   let encoded = if linear <= 0.003_130_8
   {
      linear * 12.92
   }
   else
   {
      1.055 * linear.powf(1.0 / 2.4) - 0.055
   };
   (encoded * 255.0).round() as u8
}

fn vertex_bounds(vertices: &[oxide_renderer_api::Vertex]) -> RectF
{
   let Some(first) = vertices.first() else
   {
      return RectF::new(0.0, 0.0, 0.0, 0.0);
   };
   let mut left = first.x;
   let mut top = first.y;
   let mut right = first.x;
   let mut bottom = first.y;
   for vertex in &vertices[1..]
   {
      left = left.min(vertex.x);
      top = top.min(vertex.y);
      right = right.max(vertex.x);
      bottom = bottom.max(vertex.y);
   }
   RectF::new(left, top, right - left, bottom - top)
}

fn prepared_draw_list(app: &FeedV1App) -> oxide_renderer_api::DrawList
{
   let Some(prepared) = App::prepared_frame(app) else
   {
      panic!("feed app did not expose its prepared frame");
   };
   prepared.draw_list.clone()
}

fn failure_stage(home: &Path, nonce: &str) -> String
{
   persisted_record(home, nonce)["stage"].as_str().unwrap_or("").to_owned()
}

fn persisted_record(home: &Path, nonce: &str) -> Value
{
   let path = home
      .join(contract::RESULT_DIRECTORY_NAME)
      .join(format!("{}{}{}", contract::RESULT_FILE_PREFIX, nonce, contract::RESULT_FILE_SUFFIX));
   let bytes = match fs::read(&path)
   {
      Ok(bytes) => bytes,
      Err(error) => panic!("could not read {}: {error}", path.display()),
   };
   let value: Value = match serde_json::from_slice(&bytes)
   {
      Ok(value) => value,
      Err(error) => panic!("could not parse {}: {error}", path.display()),
   };
   value
}
