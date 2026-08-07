//! Production-path Oxide treatment for the frozen feed-v1 device pilot.

#![deny(unsafe_op_in_unsafe_fn, rust_2018_idioms)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::large_enum_variant)]
#![warn(missing_docs)]

/// Frozen workload identity and deterministic row recipe.
pub mod contract;
/// Strict run-record schema and nonce-scoped observation transport.
pub mod observation;

use std::string::String;

use oxide_platform_api::{
   FrameContext,
   TouchEvent,
   TouchPhase,
};
use oxide_renderer_api as gfx;
use oxide_text as text;
use oxide_ui_core as ui;
use ui::collection::{CellRenderer, CollectionMode, CollectionView, ContentMetrics, Measure};
use ui::elements::{Align, ImageUploader, TextCtx};

use contract::{FeedFixture, Rgba8, RowRecipe, StartState};
use observation::{
   CallbackSample,
   DeviceState,
   EnvironmentTransitionCounts,
   ObservedComponent,
   ObservedRect,
   MAX_CALLBACK_SAMPLES,
   MAX_VISIBLE_COMPONENTS,
};

const SETTLE_DEADLINE_NS: u64 = 6_000_000_000;
const ROUNDED_BOUNDARY_POINTS: usize = 68;
const ROUNDED_VERTEX_COUNT: usize = ROUNDED_BOUNDARY_POINTS + 1;
const ROUNDED_INDEX_COUNT: usize = ROUNDED_BOUNDARY_POINTS * 3;
const FAILURE_STAGE_LAUNCH: &str = "launch";
const FAILURE_STAGE_INITIAL_ADMISSION: &str = "initial-admission";
const FAILURE_STAGE_GESTURE: &str = "gesture";
const EMPTY_VERTEX: gfx::Vertex = gfx::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0 };
const QUARTER_CIRCLE: [(f32, f32); 17] = [
   (1.000_000_0, 0.000_000_0),
   (0.995_184_7, 0.098_017_1),
   (0.980_785_3, 0.195_090_3),
   (0.956_940_4, 0.290_284_7),
   (0.923_879_5, 0.382_683_4),
   (0.881_921_3, 0.471_396_7),
   (0.831_469_6, 0.555_570_2),
   (0.773_010_4, 0.634_393_3),
   (0.707_106_8, 0.707_106_8),
   (0.634_393_3, 0.773_010_4),
   (0.555_570_2, 0.831_469_6),
   (0.471_396_7, 0.881_921_3),
   (0.382_683_4, 0.923_879_5),
   (0.290_284_7, 0.956_940_4),
   (0.195_090_3, 0.980_785_3),
   (0.098_017_1, 0.995_184_7),
   (0.000_000_0, 1.000_000_0),
];

static REGULAR_FONT_BYTES: &[u8] =
   include_bytes!("../../../../../../crates/ui-core/assets/Asap-Regular.ttf");
static BOLD_FONT_BYTES: &[u8] =
   include_bytes!("../../../../../../crates/ui-core/assets/Asap-Bold.ttf");

/// Read-only lifecycle phase of the injected feed treatment.
///
/// The phase is diagnostic state for admission and tests. Hosts must not use it
/// as a control surface; frame demand and app events remain the control contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedV1Phase
{
   /// The first visible frame is being composed.
   MountCold,
   /// The naturally warmed admission frame is being composed.
   MountWarm,
   /// The warm frame is waiting for its successful-submit acknowledgement.
   AwaitingReadySubmit,
   /// Visual admission completed and the app is waiting for the frozen gesture.
   Ready,
   /// A primary touch exists but has not crossed Rust-owned drag slop.
   TouchPending,
   /// The raw-touch drag or Rust-owned inertia is active.
   Gesture,
   /// Settlement was observed and one closing callback is still required.
   SettledFramePrepared,
   /// The closing frame is waiting for its successful-submit acknowledgement.
   AwaitingCompletionSubmit,
   /// The run completed or failed and no more frames are requested.
   Finished,
}

type AppPhase = FeedV1Phase;

/// Minimal diagnostic snapshot of app-owned benchmark state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FeedV1Status
{
   /// Current lifecycle phase.
   pub phase: FeedV1Phase,
   /// Current Rust-owned vertical content offset in points.
   pub content_offset_points: f32,
   /// Raw-touch timestamp that crossed drag slop, or zero before measurement.
   pub gesture_start_timestamp_ns: u64,
   /// Number of bounded display-link samples retained for the active gesture.
   pub callback_sample_count: usize,
   /// Whether the measured gesture entered Rust-owned inertial motion.
   pub inertia_observed: bool,
   /// Exact frame awaiting a submit acknowledgement, when one exists.
   pub pending_submit_frame_id: Option<u64>,
}


struct RunConfig
{
   state: StartState,
   treatment: Option<String>,
   nonce: String,
   phase: String,
   session_index: u32,
   pair_index: u32,
   order_index: u32,
   startup_failure: Option<String>,
}

impl RunConfig
{
   fn from_environment() -> Self
   {
      let treatment = std::env::var(contract::TREATMENT_ENVIRONMENT_KEY).ok();
      let start = std::env::var(contract::START_STATE_ENVIRONMENT_KEY).ok();
      let nonce = std::env::var(contract::COMPLETION_NONCE_ENVIRONMENT_KEY).unwrap_or_default();
      let phase = std::env::var("OXIDE_FEED_V1_PHASE").unwrap_or_default();
      let session = parse_nonnegative_environment("OXIDE_FEED_V1_SESSION_INDEX");
      let pair = parse_nonnegative_environment("OXIDE_FEED_V1_PAIR_INDEX");
      let order = parse_nonnegative_environment("OXIDE_FEED_V1_ORDER_INDEX");
      let state = start.as_deref().and_then(StartState::parse).unwrap_or(StartState::Top);
      let mut failure = None;
      if treatment.as_deref() != Some(contract::OXIDE_VARIANT_VALUE)
      {
         failure = Some(String::from("OXIDE_FEED_V1_TREATMENT must be oxide"));
      }
      else if start.as_deref().and_then(StartState::parse).is_none()
      {
         failure = Some(String::from("OXIDE_FEED_V1_START_STATE must be top or bottom"));
      }
      else if !observation::nonce_is_valid(&nonce)
      {
         failure = Some(String::from("OXIDE_FEED_V1_COMPLETION_NONCE is invalid"));
      }
      else if !matches!(phase.as_str(), "smoke" | "primary")
      {
         failure = Some(String::from("OXIDE_FEED_V1_PHASE must be smoke or primary"));
      }
      else if session.is_none() || pair.is_none() || order.is_none()
      {
         failure = Some(String::from("feed-v1 sampling indices must be non-negative integers"));
      }
      Self {
         state,
         treatment,
         nonce,
         phase,
         session_index: session.unwrap_or(0),
         pair_index: pair.unwrap_or(0),
         order_index: order.unwrap_or(0),
         startup_failure: failure,
      }
   }

}

fn parse_nonnegative_environment(name: &str) -> Option<u32>
{
   std::env::var(name).ok()?.parse().ok()
}

#[derive(Clone, Copy)]
struct FeedColors
{
   background: gfx::Color,
   title: gfx::Color,
   caption: gfx::Color,
   metadata: gfx::Color,
   separator: gfx::Color,
   shadow: gfx::Color,
}

impl FeedColors
{
   fn frozen() -> Self
   {
      Self {
         background: renderer_color(contract::BACKGROUND),
         title: renderer_color(contract::TITLE_COLOR),
         caption: renderer_color(contract::CAPTION_COLOR),
         metadata: renderer_color(contract::METADATA_COLOR),
         separator: renderer_color(contract::SEPARATOR_COLOR),
         shadow: renderer_color(contract::SHADOW_COLOR),
      }
   }
}

/// Production-path Oxide app for the frozen physical-device feed pilot.
pub struct FeedV1App
{
   config: RunConfig,
   fixture: FeedFixture,
   collection: CollectionView,
   scroll: ui::VerticalScrollSurface,
   builder: ui::DrawListBuilder,
   text: TextCtx,
   regular_font_id: usize,
   bold_font_id: usize,
   title_scratch: String,
   metadata_scratch: String,
   checker_source: [u8; contract::CHECKER_RGBA_BYTE_COUNT],
   checker_handles: [Option<gfx::ImageHandle>; contract::CHECKER_VARIANT_COUNT],
   colors: FeedColors,
   damage: [gfx::RectI; 1],
   app_phase: AppPhase,
   failure: Option<String>,
   failure_stage: &'static str,
   frame_components: [ObservedComponent; MAX_VISIBLE_COMPONENTS],
   frame_component_count: usize,
   captured_components: [ObservedComponent; MAX_VISIBLE_COMPONENTS],
   captured_component_count: usize,
   captured_content_offset_points: f32,
   pending_ready_frame_id: u64,
   pending_completion_frame_id: u64,
   gesture_start_offset_points: f32,
   gesture_start_timestamp_ns: u64,
   gesture_end_timestamp_ns: u64,
   settled_timestamp_ns: u64,
   settle_deadline_ns: u64,
   callback_samples: [CallbackSample; MAX_CALLBACK_SAMPLES],
   callback_count: usize,
   inertia_observed: bool,
   device_before: DeviceState,
   device_after: DeviceState,
   environment_transition_baseline: EnvironmentTransitionCounts,
   thermal_state_change_count: u32,
   low_power_mode_change_count: u32,
}

impl FeedV1App
{
   /// Constructs one run from the controller-owned `OXIDE_FEED_V1_*` inputs.
   pub fn from_environment() -> Self
   {
      Self::new(RunConfig::from_environment())
   }

   /// Returns diagnostic state without changing frame demand or app behavior.
   pub fn status(&self) -> FeedV1Status
   {
      let pending_submit_frame_id = match self.app_phase
      {
         AppPhase::AwaitingReadySubmit => Some(self.pending_ready_frame_id),
         AppPhase::AwaitingCompletionSubmit => Some(self.pending_completion_frame_id),
         _ => None,
      };
      FeedV1Status {
         phase: self.app_phase,
         content_offset_points: self.scroll.offset(),
         gesture_start_timestamp_ns: self.gesture_start_timestamp_ns,
         callback_sample_count: self.callback_count,
         inertia_observed: self.inertia_observed,
         pending_submit_frame_id,
      }
   }

   fn new(config: RunConfig) -> Self
   {
      let fixture = FeedFixture::new();
      let mut collection = CollectionView::new(CollectionMode::VerticalGrid {
         col_width: contract::SURFACE_WIDTH_POINTS as f32,
         spacing: 0.0,
      });
      collection.set_count(contract::ROW_COUNT);
      let mut scroll = ui::VerticalScrollSurface::new(
         fixture.content_extent_points() as f32,
         contract::SURFACE_HEIGHT_POINTS as f32,
      );
      scroll.set_offset(config.state.offset_points() as f32);
      let mut text = TextCtx::default();
      let regular_font_id = text.fonts.add_font(text::Font::from_bytes(REGULAR_FONT_BYTES.to_vec()));
      let bold_font_id = text.fonts.add_font(text::Font::from_bytes(BOLD_FONT_BYTES.to_vec()));
      let failure_stage = if config.startup_failure.is_some()
      {
         FAILURE_STAGE_LAUNCH
      }
      else
      {
         FAILURE_STAGE_INITIAL_ADMISSION
      };
      let failure = config.startup_failure.clone().or_else(||
      {
         if fixture.content_extent_points() != contract::CONTENT_EXTENT_POINTS
            || fixture.maximum_content_offset_points() != contract::MAXIMUM_CONTENT_OFFSET_POINTS
         {
            Some(String::from("feed-v1 prefix geometry does not match the frozen extent"))
         }
         else if !contract::canonical_fixture_identity(&fixture)
            .is_some_and(contract::CanonicalFixtureIdentity::is_expected)
         {
            Some(String::from("feed-v1 canonical fixture identity does not match the frozen Swift contract"))
         }
         else if REGULAR_FONT_BYTES.len() != contract::REGULAR_FONT_BYTE_COUNT as usize
            || BOLD_FONT_BYTES.len() != contract::BOLD_FONT_BYTE_COUNT as usize
         {
            Some(String::from("feed-v1 embedded font byte counts do not match the fixture"))
         }
         else
         {
            None
         }
      });
      let initial_offset = config_state_offset(&config);
      Self {
         config,
         fixture,
         collection,
         scroll,
         builder: ui::DrawListBuilder::new(),
         text,
         regular_font_id,
         bold_font_id,
         title_scratch: String::with_capacity(48),
         metadata_scratch: String::with_capacity(32),
         checker_source: [0; contract::CHECKER_RGBA_BYTE_COUNT],
         checker_handles: [None; contract::CHECKER_VARIANT_COUNT],
         colors: FeedColors::frozen(),
         damage: [gfx::RectI::new(
            0,
            0,
            contract::HOST_WIDTH_POINTS as i32,
            contract::HOST_HEIGHT_POINTS as i32,
         )],
         app_phase: AppPhase::MountCold,
         failure,
         failure_stage,
         frame_components: [ObservedComponent::default(); MAX_VISIBLE_COMPONENTS],
         frame_component_count: 0,
         captured_components: [ObservedComponent::default(); MAX_VISIBLE_COMPONENTS],
         captured_component_count: 0,
         captured_content_offset_points: initial_offset,
         pending_ready_frame_id: 0,
         pending_completion_frame_id: 0,
         gesture_start_offset_points: initial_offset,
         gesture_start_timestamp_ns: 0,
         gesture_end_timestamp_ns: 0,
         settled_timestamp_ns: 0,
         settle_deadline_ns: 0,
         callback_samples: [CallbackSample::default(); MAX_CALLBACK_SAMPLES],
         callback_count: 0,
         inertia_observed: false,
         device_before: DeviceState::default(),
         device_after: DeviceState::default(),
         environment_transition_baseline: EnvironmentTransitionCounts::default(),
         thermal_state_change_count: 0,
         low_power_mode_change_count: 0,
      }
   }

   fn fail(&mut self, stage: &'static str, message: impl Into<String>)
   {
      if self.failure.is_none()
      {
         self.failure_stage = stage;
         self.failure = Some(message.into());
      }
   }

   fn handle_touch(&mut self, event: &TouchEvent)
   {
      if matches!(self.app_phase, AppPhase::Finished | AppPhase::MountCold | AppPhase::MountWarm)
      {
         return;
      }
      if self.app_phase == AppPhase::Ready
      {
         if event.phase != TouchPhase::Start || !touch_in_surface(event)
         {
            return;
         }
         self.scroll.input_touch(event);
         self.app_phase = AppPhase::TouchPending;
         return;
      }
      if self.app_phase == AppPhase::TouchPending
      {
         let offset_before = self.scroll.offset();
         let changed = self.scroll.input_touch(event);
         if event.phase == TouchPhase::Move && changed
         {
            self.app_phase = AppPhase::Gesture;
            self.gesture_start_offset_points = offset_before;
            self.gesture_start_timestamp_ns = event.timestamp_ns;
            self.gesture_end_timestamp_ns = 0;
            self.settled_timestamp_ns = 0;
            self.settle_deadline_ns = event.timestamp_ns.saturating_add(SETTLE_DEADLINE_NS);
            self.callback_count = 0;
         }
         else if matches!(event.phase, TouchPhase::End | TouchPhase::Cancel)
         {
            self.app_phase = AppPhase::Ready;
         }
         return;
      }
      if self.app_phase != AppPhase::Gesture
      {
         return;
      }
      self.scroll.input_touch(event);
      if self.scroll.wants_next_frame()
      {
         self.inertia_observed = true;
      }
      match event.phase
      {
         TouchPhase::End => self.gesture_end_timestamp_ns = event.timestamp_ns,
         TouchPhase::Cancel =>
         {
            self.gesture_end_timestamp_ns = event.timestamp_ns;
            self.fail(FAILURE_STAGE_GESTURE, "feed-v1 gesture was cancelled");
         }
         TouchPhase::Start | TouchPhase::Move => {}
      }
   }

   fn push_callback_sample(&mut self, sample: CallbackSample)
   {
      if self.callback_count < self.callback_samples.len()
      {
         self.callback_samples[self.callback_count] = sample;
         self.callback_count += 1;
      }
      else
      {
         self.fail(FAILURE_STAGE_GESTURE, "feed-v1 display-link sample capacity exceeded");
      }
   }

   fn render_frame(&mut self, frame: FrameContext, uploader: &mut dyn gfx::RuntimeImageUploader)
   {
      self.builder.clear();
      let viewport = gfx::RectF::new(
         contract::SURFACE_ORIGIN_X_POINTS as f32,
         contract::SURFACE_ORIGIN_Y_POINTS as f32,
         contract::SURFACE_WIDTH_POINTS as f32,
         contract::SURFACE_HEIGHT_POINTS as f32,
      );
      self.builder.rrect(viewport, [0.0; 4], self.colors.background);
      self.builder.clip_push(gfx::RectI::new(
         contract::SURFACE_ORIGIN_X_POINTS as i32,
         contract::SURFACE_ORIGIN_Y_POINTS as i32,
         contract::SURFACE_WIDTH_POINTS as i32,
         contract::SURFACE_HEIGHT_POINTS as i32,
      ));
      self.collection.set_scroll(self.scroll.offset());
      let mut measure = FeedMeasure { fixture: &self.fixture };
      self.frame_component_count = 0;
      let scroll_offset_points = self.scroll.offset();
      let mut renderer = FeedRenderer {
         text: &mut self.text,
         regular_font_id: self.regular_font_id,
         bold_font_id: self.bold_font_id,
         title_scratch: &mut self.title_scratch,
         metadata_scratch: &mut self.metadata_scratch,
         checker_source: &mut self.checker_source,
         checker_handles: &mut self.checker_handles,
         colors: self.colors,
         scale: frame.scale,
         scroll_offset_points,
         observe_components: self.app_phase == AppPhase::MountWarm,
         observed_components: &mut self.frame_components,
         observed_component_count: &mut self.frame_component_count,
         uploader,
         failure: None,
      };
      let metrics = self.collection.layout_and_render(
         viewport,
         &mut measure,
         &mut renderer,
         &mut self.builder,
      );
      let renderer_failure = renderer.failure;
      drop(renderer);
      self.builder.clip_pop();
      let render_failure_stage = self.render_failure_stage();
      if self.app_phase == AppPhase::MountCold
      {
         self.scroll.update_extents(metrics.content_h, viewport.h);
         if !metrics_match_fixture(metrics, &self.fixture)
         {
            self.fail(
               render_failure_stage,
               "CollectionView content metrics differ from feed-v1",
            );
         }
      }
      if let Some(failure) = renderer_failure
      {
         self.fail(render_failure_stage, failure);
      }
      if self.app_phase == AppPhase::MountWarm && self.failure.is_none()
      {
         if !observed_components_match_fixture(
            &self.fixture,
            self.config.state,
            scroll_offset_points,
            &self.frame_components[..self.frame_component_count],
         )
         {
            self.fail(
               FAILURE_STAGE_INITIAL_ADMISSION,
               "rendered ready-frame geometry differs from the frozen feed-v1 fixture",
            );
         }
         else
         {
            self.captured_content_offset_points = scroll_offset_points;
            self.captured_component_count = self.frame_component_count;
            self.captured_components[..self.frame_component_count]
               .copy_from_slice(&self.frame_components[..self.frame_component_count]);
         }
      }
   }

   fn render_failure_stage(&self) -> &'static str
   {
      if matches!(
         self.app_phase,
         AppPhase::TouchPending
            | AppPhase::Gesture
            | AppPhase::SettledFramePrepared
            | AppPhase::AwaitingCompletionSubmit,
      )
      {
         FAILURE_STAGE_GESTURE
      }
      else
      {
         FAILURE_STAGE_INITIAL_ADMISSION
      }
   }

   fn advance_gesture_frame(&mut self, frame: FrameContext)
   {
      self.push_callback_sample(CallbackSample {
         timestamp_ns: frame.timestamp_ns,
         target_timestamp_ns: frame.target_timestamp_ns,
      });
      self.scroll.advance_to(frame.timestamp_ns);
      if frame.timestamp_ns >= self.settle_deadline_ns && !self.scroll.is_settled()
      {
         self.fail(
            FAILURE_STAGE_GESTURE,
            "feed-v1 scroll did not settle within six seconds",
         );
      }
      if self.failure.is_some()
      {
         self.settled_timestamp_ns = frame.timestamp_ns;
         self.app_phase = AppPhase::SettledFramePrepared;
         return;
      }
      if self.gesture_end_timestamp_ns != 0 && self.scroll.is_settled()
      {
         self.settled_timestamp_ns = frame.timestamp_ns;
         self.app_phase = AppPhase::SettledFramePrepared;
      }
   }

}

struct FeedMeasure<'a>
{
   fixture: &'a FeedFixture,
}

impl Measure for FeedMeasure<'_>
{
   fn measure(&mut self, index: usize, _constraint: f32) -> f32
   {
      self.fixture.row_height_points(index).unwrap_or(1) as f32
   }

   fn collection_revision(&self) -> Option<u64>
   {
      Some(1)
   }
}

struct RuntimeTextUploader<'a>
{
   uploader: &'a mut dyn gfx::RuntimeImageUploader,
}

impl ImageUploader for RuntimeTextUploader<'_>
{
   fn create_a8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> gfx::ImageHandle
   {
      self.uploader.create_a8(width, height, data, row_bytes)
   }

   fn update_a8(&mut self, handle: gfx::ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize)
   {
      self.uploader.update_a8(handle, x, y, width, height, data, row_bytes);
   }
}

struct FeedRenderer<'a>
{
   text: &'a mut TextCtx,
   regular_font_id: usize,
   bold_font_id: usize,
   title_scratch: &'a mut String,
   metadata_scratch: &'a mut String,
   checker_source: &'a mut [u8; contract::CHECKER_RGBA_BYTE_COUNT],
   checker_handles: &'a mut [Option<gfx::ImageHandle>; contract::CHECKER_VARIANT_COUNT],
   colors: FeedColors,
   scale: f32,
   scroll_offset_points: f32,
   observe_components: bool,
   observed_components: &'a mut [ObservedComponent; MAX_VISIBLE_COMPONENTS],
   observed_component_count: &'a mut usize,
   uploader: &'a mut dyn gfx::RuntimeImageUploader,
   failure: Option<&'static str>,
}

impl CellRenderer for FeedRenderer<'_>
{
   fn render(&mut self, _cell_id: u32, index: usize, rect: gfx::RectF, _focused: bool, _hovered: bool, builder: &mut ui::DrawListBuilder)
   {
      let Some(row) = RowRecipe::at(index) else
      {
         self.failure = Some("CollectionView requested an out-of-range feed-v1 row");
         return;
      };
      let text_x = rect.x
         + (contract::ROW_LEADING_POINTS
            + contract::IMAGE_SIDE_POINTS
            + contract::IMAGE_TEXT_GAP_POINTS) as f32;
      let text_width = (contract::SURFACE_WIDTH_POINTS
         - contract::ROW_LEADING_POINTS
         - contract::IMAGE_SIDE_POINTS
         - contract::IMAGE_TEXT_GAP_POINTS
         - contract::ROW_TRAILING_POINTS) as f32;
      let image = gfx::RectF::new(
         rect.x + contract::ROW_LEADING_POINTS as f32,
         rect.y + contract::ROW_TOP_POINTS as f32,
         contract::IMAGE_SIDE_POINTS as f32,
         contract::IMAGE_SIDE_POINTS as f32,
      );
      let shadow = gfx::RectF::new(
         image.x + contract::SHADOW_OFFSET_X_POINTS as f32,
         image.y + contract::SHADOW_OFFSET_Y_POINTS as f32,
         image.w,
         image.h,
      );
      let caption_height =
         row.caption_line_count() as f32 * contract::CAPTION_LINE_HEIGHT_POINTS as f32;
      let metadata_y = rect.y
         + contract::CAPTION_TOP_POINTS as f32
         + caption_height
         + contract::METADATA_GAP_POINTS as f32;
      let separator_height =
         contract::SEPARATOR_PHYSICAL_PIXELS as f32 / contract::SURFACE_SCALE as f32;
      if self.observe_components
      {
         for (kind_index, component_rect) in [
            (0, rect),
            (1, image),
            (
               2,
               gfx::RectF::new(
                  text_x,
                  rect.y + contract::ROW_TOP_POINTS as f32,
                  text_width,
                  contract::TITLE_HEIGHT_POINTS as f32,
               ),
            ),
            (
               3,
               gfx::RectF::new(
                  text_x,
                  rect.y + contract::CAPTION_TOP_POINTS as f32,
                  text_width,
                  caption_height,
               ),
            ),
            (
               4,
               gfx::RectF::new(
                  text_x,
                  metadata_y,
                  text_width,
                  contract::METADATA_HEIGHT_POINTS as f32,
               ),
            ),
            (
               5,
               gfx::RectF::new(
                  rect.x,
                  rect.y + rect.h - separator_height,
                  rect.w,
                  separator_height,
               ),
            ),
         ]
         {
            self.observe_component(index, kind_index, component_rect);
         }
      }
      if self.failure.is_some()
      {
         return;
      }
      builder.rrect(
         shadow,
         [contract::IMAGE_CORNER_RADIUS_POINTS as f32; 4],
         self.colors.shadow,
      );
      let Some(handle) = self.ensure_checker(row.checker_variant()) else
      {
         self.failure = Some("iOS renderer does not support the required nearest RGBA8 upload");
         return;
      };
      encode_rounded_checker(builder, handle, image);

      row.write_title(self.title_scratch);
      let mut text_uploader = RuntimeTextUploader { uploader: self.uploader };
      // Oxide's label origin is a glyph baseline. UIKit's frozen label frames
      // are top-aligned boxes, so plain labels add UIFont's Asap ascender and
      // the fixed-height caption additionally splits its paragraph leading.
      ui::elements::encode_label_text(
         self.title_scratch,
         self.colors.title,
         Align::Left,
         false,
         self.bold_font_id,
         contract::TITLE_FONT_POINTS as f32,
         gfx::RectF::new(
            text_x,
            rect.y + contract::ROW_TOP_POINTS as f32
               + contract::font_baseline_from_top(contract::TITLE_FONT_POINTS),
            text_width,
            contract::TITLE_HEIGHT_POINTS as f32,
         ),
         self.scale,
         self.text,
         &mut text_uploader,
         builder,
      );
      for line in 0..row.caption_line_count()
      {
         let Some(caption) = row.caption_line(line) else
         {
            continue;
         };
         ui::elements::encode_label_text(
            caption,
            self.colors.caption,
            Align::Left,
            false,
            self.regular_font_id,
            contract::CAPTION_FONT_POINTS as f32,
            gfx::RectF::new(
               text_x,
               rect.y
                  + contract::CAPTION_TOP_POINTS as f32
                  + line as f32 * contract::CAPTION_LINE_HEIGHT_POINTS as f32
                  + contract::caption_baseline_from_line_top(),
               text_width,
               contract::CAPTION_LINE_HEIGHT_POINTS as f32,
            ),
            self.scale,
            self.text,
            &mut text_uploader,
            builder,
         );
      }
      row.write_metadata(self.metadata_scratch);
      ui::elements::encode_label_text(
         self.metadata_scratch,
         self.colors.metadata,
         Align::Left,
         false,
         self.regular_font_id,
         contract::METADATA_FONT_POINTS as f32,
         gfx::RectF::new(
            text_x,
            metadata_y + contract::font_baseline_from_top(contract::METADATA_FONT_POINTS),
            text_width,
            contract::METADATA_HEIGHT_POINTS as f32,
         ),
         self.scale,
         self.text,
         &mut text_uploader,
         builder,
      );
      builder.rrect(
         gfx::RectF::new(rect.x, rect.y + rect.h - separator_height, rect.w, separator_height),
         [0.0; 4],
         self.colors.separator,
      );
   }
}

impl FeedRenderer<'_>
{
   fn observe_component(&mut self, row_index: usize, kind_index: usize, rect: gfx::RectF)
   {
      if *self.observed_component_count >= self.observed_components.len()
      {
         self.failure = Some("feed-v1 ready-frame component capacity exceeded");
         return;
      }
      self.observed_components[*self.observed_component_count] = ObservedComponent {
         row_index,
         kind_index,
         content_rect_points: ObservedRect {
            x: rect.x - contract::SURFACE_ORIGIN_X_POINTS as f32,
            y: rect.y - contract::SURFACE_ORIGIN_Y_POINTS as f32 + self.scroll_offset_points,
            width: rect.w,
            height: rect.h,
         },
      };
      *self.observed_component_count += 1;
   }

   fn ensure_checker(&mut self, variant: usize) -> Option<gfx::ImageHandle>
   {
      if let Some(handle) = self.checker_handles.get(variant).copied().flatten()
      {
         return Some(handle);
      }
      if !contract::checker_rgba_bytes(variant, self.checker_source)
      {
         return None;
      }
      let handle = self.uploader.try_create_rgba8_sampled(
         contract::CHECKER_SIDE_PIXELS as u32,
         contract::CHECKER_SIDE_PIXELS as u32,
         self.checker_source,
         contract::CHECKER_SIDE_PIXELS * 4,
         gfx::ImageSampling::Nearest,
      )?;
      if let Some(slot) = self.checker_handles.get_mut(variant)
      {
         *slot = Some(handle);
      }
      Some(handle)
   }
}

fn encode_rounded_checker(builder: &mut ui::DrawListBuilder, handle: gfx::ImageHandle, rect: gfx::RectF)
{
   let mut vertices = [EMPTY_VERTEX; ROUNDED_VERTEX_COUNT];
   let mut indices = [0_u16; ROUNDED_INDEX_COUNT];
   vertices[0] = image_vertex(rect, rect.w * 0.5, rect.h * 0.5);
   let radius = contract::IMAGE_CORNER_RADIUS_POINTS as f32;
   let corners = [
      (radius, radius, 0_u8),
      (rect.w - radius, radius, 1_u8),
      (rect.w - radius, rect.h - radius, 2_u8),
      (radius, rect.h - radius, 3_u8),
   ];
   let mut boundary = 0_usize;
   for (center_x, center_y, corner) in corners
   {
      for (cosine, sine) in QUARTER_CIRCLE
      {
         let (x, y) = match corner
         {
            0 => (center_x - cosine * radius, center_y - sine * radius),
            1 => (center_x + sine * radius, center_y - cosine * radius),
            2 => (center_x + cosine * radius, center_y + sine * radius),
            _ => (center_x - sine * radius, center_y + cosine * radius),
         };
         vertices[boundary + 1] = image_vertex(rect, x, y);
         boundary += 1;
      }
   }
   for triangle in 0..ROUNDED_BOUNDARY_POINTS
   {
      let offset = triangle * 3;
      indices[offset] = 0;
      indices[offset + 1] = triangle as u16 + 1;
      indices[offset + 2] = ((triangle + 1) % ROUNDED_BOUNDARY_POINTS) as u16 + 1;
   }
   builder.image_mesh(handle, &vertices, &indices, 1.0);
}

fn image_vertex(rect: gfx::RectF, x: f32, y: f32) -> gfx::Vertex
{
   gfx::Vertex {
      x: rect.x + x,
      y: rect.y + y,
      u: x / rect.w,
      v: y / rect.h,
      rgba: 0,
   }
}

fn renderer_color(color: Rgba8) -> gfx::Color
{
   gfx::Color::rgba(
      srgb_channel_to_linear(color.red),
      srgb_channel_to_linear(color.green),
      srgb_channel_to_linear(color.blue),
      color.alpha as f32 / 255.0,
   )
}

fn srgb_channel_to_linear(channel: u8) -> f32
{
   let encoded = channel as f32 / 255.0;
   if encoded <= 0.040_45
   {
      encoded / 12.92
   }
   else
   {
      ((encoded + 0.055) / 1.055).powf(2.4)
   }
}

fn touch_in_surface(event: &TouchEvent) -> bool
{
   let left = contract::SURFACE_ORIGIN_X_POINTS as f32;
   let top = contract::SURFACE_ORIGIN_Y_POINTS as f32;
   event.x >= left
      && event.x <= left + contract::SURFACE_WIDTH_POINTS as f32
      && event.y >= top
      && event.y <= top + contract::SURFACE_HEIGHT_POINTS as f32
}

fn metrics_match_fixture(metrics: ContentMetrics, fixture: &FeedFixture) -> bool
{
   metrics.content_w == contract::SURFACE_WIDTH_POINTS as f32
      && metrics.content_h == fixture.content_extent_points() as f32
}

fn observed_components_match_fixture(fixture: &FeedFixture, state: StartState, captured_offset_points: f32, observed: &[ObservedComponent]) -> bool
{
   let scale = contract::SURFACE_SCALE as f32;
   if !captured_offset_points.is_finite()
      || ((captured_offset_points - state.offset_points() as f32) * scale).abs() > 1.0
   {
      return false;
   }
   let visible_rows = fixture.visible_row_range(state.offset_points());
   if observed.len() != visible_rows.clone().count() * contract::COMPONENT_KINDS.len()
   {
      return false;
   }
   let mut observed_index = 0;
   for row_index in visible_rows
   {
      for (kind_index, kind) in contract::COMPONENT_KINDS.iter().copied().enumerate()
      {
         let Some(component) = observed.get(observed_index) else
         {
            return false;
         };
         let Some(expected) = fixture.component_rect_physical_pixels(row_index, kind) else
         {
            return false;
         };
         if component.row_index != row_index
            || component.kind_index != kind_index
            || !component_rect_within_one_physical_pixel(component.content_rect_points, expected)
         {
            return false;
         }
         observed_index += 1;
      }
   }
   true
}

fn component_rect_within_one_physical_pixel(actual: ObservedRect, expected: contract::PhysicalRect) -> bool
{
   let scale = contract::SURFACE_SCALE as f32;
   let actual_right = (actual.x + actual.width) * scale;
   let actual_bottom = (actual.y + actual.height) * scale;
   let expected_right = (expected.x + expected.width) as f32;
   let expected_bottom = (expected.y + expected.height) as f32;
   actual.x.is_finite()
      && actual.y.is_finite()
      && actual.width.is_finite()
      && actual.height.is_finite()
      && actual_right.is_finite()
      && actual_bottom.is_finite()
      && (actual.x * scale - expected.x as f32).abs() <= 1.0
      && (actual.y * scale - expected.y as f32).abs() <= 1.0
      && (actual_right - expected_right).abs() <= 1.0
      && (actual_bottom - expected_bottom).abs() <= 1.0
}

fn config_state_offset(config: &RunConfig) -> f32
{
   config.state.offset_points() as f32
}
