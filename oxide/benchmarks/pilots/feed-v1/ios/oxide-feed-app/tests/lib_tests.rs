use std::ffi::OsString;
use std::path::Path;
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

static ENVIRONMENT_LOCK: Mutex<()> = Mutex::new(());

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
   rgba_uploads: Vec<RgbaUpload>,
}

impl RuntimeImageUploader for UploadProbe
{
   fn create_a8(&mut self, _width: u32, _height: u32, _data: &[u8], _row_bytes: usize) -> ImageHandle
   {
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
