use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxide_benchmark_spec::{ChatFixture, ChatMessage, DashboardCategories, DashboardFixture, EffectsFixture, EnduranceFixture, FeedFixture, FeedRow, GridFixture, ImageDecodeZoomFixture, InlineTextAsset, InlineTextAtlas, MutationFixture, NavigationFixture, ResizeFixture, RoleCount, ScenarioSpec, StartupCard, StartupFixture, TextFixture, TraceEvent, TraceOperation, TraceValue};
use oxide_renderer_api as gfx;
use oxide_ui_core::{elements, DrawListBuilder};

// The comparison style pack is authored in sRGB. Metal shader colors are
// linear and the sRGB attachment performs the final transfer on write.
const BACKGROUND: gfx::Color = gfx::Color::rgba(0.896269353, 0.913098652, 0.938685728, 1.0);
const SURFACE: gfx::Color = gfx::Color::rgba(1.0, 1.0, 1.0, 1.0);
const TEXT: gfx::Color = gfx::Color::rgba(0.014443844, 0.017641954, 0.025186860, 1.0);
const SECONDARY: gfx::Color = gfx::Color::rgba(0.141263291, 0.165132195, 0.219526200, 1.0);
const ACCENT: gfx::Color = gfx::Color::rgba(0.046665086, 0.155926464, 0.863157213, 1.0);
const DASHBOARD_MATERIAL: gfx::Color = gfx::Color::rgba(0.745404210, 0.760524505, 0.863157214, 1.0);
const EFFECTS_MATERIAL: gfx::Color = gfx::Color::rgba(0.723055129, 0.723055129, 0.745404210, 1.0);
const MODAL_OVERLAY: gfx::Color = gfx::Color::rgba(0.009721217, 0.011612245, 0.016807376, 1.0);
const SHADOW: gfx::Color = gfx::Color::rgba(0.014443844, 0.017641954, 0.025186860, 0.160784314);
const INACTIVE_CONTROL: gfx::Color = gfx::Color::rgba(0.456411023, 0.491020850, 0.564711506, 1.0);
const DISABLED_SLIDER_TRACK: gfx::Color = gfx::Color::rgba(0.799102738, 0.814846572, 0.822785754, 1.0);
const RESIZE_LIGHT_MATERIAL: gfx::Color = gfx::Color::rgba(0.910000000, 0.892000000, 0.865000000, 1.0);
const RESIZE_DARK_MATERIAL: gfx::Color = gfx::Color::rgba(0.220000000, 0.216000000, 0.212000000, 1.0);
const VIEWPORT_HEIGHT: f32 = 844.0;
const FEED_HEADER_HEIGHT: f32 = 52.0;
const CHAT_HEADER_HEIGHT: f32 = 52.0;
const CHAT_COMPOSER_HEIGHT: f32 = 92.0;
const MAX_VISIBLE_CHAT_ROWS: usize = 10;
const ATLAS_COLUMNS: u32 = 16;
const ATLAS_ROWS: u32 = 8;
const ATLAS_TILE_SIZE: f32 = 24.0;
const IMAGE_BASE_DOWNSAMPLE: f32 = 2.0;

#[inline]
fn point_in_rect(x: f32, y: f32, rect: gfx::RectF) -> bool
{
   x >= rect.x && x < rect.x + rect.w && y >= rect.y && y < rect.y + rect.h
}

#[cfg(feature = "comparison-geometry")]
#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonGeometryNode
{
   pub role: String,
   pub identifier: String,
   pub bounds: gfx::RectF,
   pub text_line_bounds: Vec<gfx::RectF>,
}

trait GeometrySink
{
   fn record(&mut self, role: &str, identifier: &str, bounds: gfx::RectF, text_line_bounds: &[gfx::RectF]);
   fn record_indexed(&mut self, role: &str, prefix: &str, index: usize, width: usize, bounds: gfx::RectF, text_line_bounds: &[gfx::RectF]);
   fn record_suffix(&mut self, role: &str, identifier: &str, suffix: &str, bounds: gfx::RectF, text_line_bounds: &[gfx::RectF]);
}

struct IgnoreGeometry;

impl GeometrySink for IgnoreGeometry
{
   #[inline]
   fn record(&mut self, _role: &str, _identifier: &str, _bounds: gfx::RectF, _text_line_bounds: &[gfx::RectF]) {}

   #[inline]
   fn record_indexed(&mut self, _role: &str, _prefix: &str, _index: usize, _width: usize, _bounds: gfx::RectF, _text_line_bounds: &[gfx::RectF]) {}

   #[inline]
   fn record_suffix(&mut self, _role: &str, _identifier: &str, _suffix: &str, _bounds: gfx::RectF, _text_line_bounds: &[gfx::RectF]) {}
}

#[cfg(feature = "comparison-geometry")]
#[derive(Default)]
struct CaptureGeometry
{
   nodes: Vec<ComparisonGeometryNode>,
}

#[cfg(feature = "comparison-geometry")]
impl GeometrySink for CaptureGeometry
{
   fn record(&mut self, role: &str, identifier: &str, bounds: gfx::RectF, text_line_bounds: &[gfx::RectF])
   {
      self.nodes.push(ComparisonGeometryNode {
         role: role.to_string(),
         identifier: identifier.to_string(),
         bounds,
         text_line_bounds: text_line_bounds.to_vec(),
      });
   }

   fn record_indexed(&mut self, role: &str, prefix: &str, index: usize, width: usize, bounds: gfx::RectF, text_line_bounds: &[gfx::RectF])
   {
      self.nodes.push(ComparisonGeometryNode {
         role: role.to_string(),
         identifier: format!("{prefix}{index:0width$}"),
         bounds,
         text_line_bounds: text_line_bounds.to_vec(),
      });
   }

   fn record_suffix(&mut self, role: &str, identifier: &str, suffix: &str, bounds: gfx::RectF, text_line_bounds: &[gfx::RectF])
   {
      self.nodes.push(ComparisonGeometryNode {
         role: role.to_string(),
         identifier: format!("{identifier}{suffix}"),
         bounds,
         text_line_bounds: text_line_bounds.to_vec(),
      });
   }
}

fn encode_rounded_atlas_image(builder: &mut DrawListBuilder, image: gfx::ImageHandle, dst: gfx::RectF, src: gfx::RectF, radius: f32)
{
   const SEGMENTS_PER_CORNER: usize = 16;
   const BOUNDARY_VERTICES: usize = SEGMENTS_PER_CORNER * 4;
   const VERTEX_COUNT: usize = BOUNDARY_VERTICES + 1;
   const INDEX_COUNT: usize = BOUNDARY_VERTICES * 3;
   let radius = radius.clamp(0.0, 0.5 * dst.w.min(dst.h));
   let mut vertices = [gfx::Vertex {x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: u32::MAX}; VERTEX_COUNT];
   let mut indices = [0_u16; INDEX_COUNT];
   let atlas_width = ATLAS_COLUMNS as f32 * ATLAS_TILE_SIZE;
   let atlas_height = ATLAS_ROWS as f32 * ATLAS_TILE_SIZE;
   let vertex = |x: f32, y: f32| gfx::Vertex {
      x: dst.x + x,
      y: dst.y + y,
      u: (src.x + 0.5 + x / dst.w * (src.w - 1.0)) / atlas_width,
      v: (src.y + 0.5 + y / dst.h * (src.h - 1.0)) / atlas_height,
      rgba: u32::MAX,
   };
   vertices[0] = vertex(dst.w * 0.5, dst.h * 0.5);
   let centers = [
      (radius, radius, core::f32::consts::PI),
      (dst.w - radius, radius, core::f32::consts::PI * 1.5),
      (dst.w - radius, dst.h - radius, 0.0),
      (radius, dst.h - radius, core::f32::consts::FRAC_PI_2),
   ];
   for (corner, (center_x, center_y, start)) in centers.into_iter().enumerate()
   {
      for segment in 0..SEGMENTS_PER_CORNER
      {
         let boundary = corner * SEGMENTS_PER_CORNER + segment;
         let angle = start + core::f32::consts::FRAC_PI_2 * segment as f32 / SEGMENTS_PER_CORNER as f32;
         let (sin, cos) = angle.sin_cos();
         vertices[boundary + 1] = vertex(center_x + cos * radius, center_y + sin * radius);
         let index = boundary * 3;
         indices[index] = 0;
         indices[index + 1] = (boundary + 1) as u16;
         indices[index + 2] = ((boundary + 1) % BOUNDARY_VERTICES + 1) as u16;
      }
   }
   builder.image_mesh(image, &vertices, &indices, 1.0);
}

fn encode_solid_rects(builder: &mut DrawListBuilder, rects: impl IntoIterator<Item = gfx::RectF>, color: gfx::Color)
{
   const MAX_RECTS_PER_DRAW: usize = (u16::MAX as usize + 1) / 4;

   let mut rects = rects.into_iter().filter(|rect| {
      rect.x.is_finite()
         && rect.y.is_finite()
         && rect.w.is_finite()
         && rect.h.is_finite()
         && rect.w > 0.0
         && rect.h > 0.0
   });
   loop
   {
      let mut exhausted = false;
      let spans =
      {
         let list = builder.drawlist_mut();
         let Ok(vertex_offset) = u32::try_from(list.vertices.len()) else { return };
         let Ok(index_offset) = u32::try_from(list.indices.len()) else { return };
         let mut rect_count = 0_usize;
         while rect_count < MAX_RECTS_PER_DRAW
         {
            let Some(rect) = rects.next() else
            {
               exhausted = true;
               break;
            };
            let local = (rect_count * 4) as u16;
            list.vertices.extend_from_slice(&[
               gfx::Vertex {x: rect.x, y: rect.y, u: 0.0, v: 0.0, rgba: 0},
               gfx::Vertex {x: rect.x + rect.w, y: rect.y, u: 0.0, v: 0.0, rgba: 0},
               gfx::Vertex {x: rect.x, y: rect.y + rect.h, u: 0.0, v: 0.0, rgba: 0},
               gfx::Vertex {x: rect.x + rect.w, y: rect.y + rect.h, u: 0.0, v: 0.0, rgba: 0},
            ]);
            list.indices.extend_from_slice(&[local, local + 1, local + 2, local + 2, local + 1, local + 3]);
            rect_count += 1;
         }
         let Ok(vertex_len) = u32::try_from(rect_count * 4) else { return };
         let Ok(index_len) = u32::try_from(rect_count * 6) else { return };
         (
            gfx::VertexSpan {offset: vertex_offset, len: vertex_len},
            gfx::IndexSpan {offset: index_offset, len: index_len},
         )
      };
      if spans.0.len == 0
      {
         return;
      }
      builder.solid(spans.0, spans.1, color);
      if exhausted
      {
         return;
      }
   }
}

struct ComparisonResources
{
   thumbnail_atlas: gfx::ImageHandle,
   accented_thumbnail_atlas: gfx::ImageHandle,
   source_image: gfx::ImageHandle,
   thumbnail_image: gfx::ImageHandle,
   latin_font: usize,
   arabic_font: usize,
   cjk_font: usize,
   inline_text: Option<InlineTextResources>,
}

struct InlineTextResources
{
   images: Vec<gfx::ImageHandle>,
   atlas: InlineTextAtlas,
}

impl Default for ComparisonResources
{
   fn default() -> Self
   {
      Self {
         thumbnail_atlas: gfx::ImageHandle(0),
         accented_thumbnail_atlas: gfx::ImageHandle(0),
         source_image: gfx::ImageHandle(0),
         thumbnail_image: gfx::ImageHandle(0),
         latin_font: 0,
         arabic_font: 0,
         cjk_font: 0,
         inline_text: None,
      }
   }
}

impl ComparisonResources
{
   fn set_fonts(&mut self, font_ids: [usize; 3])
   {
      self.latin_font = font_ids[0];
      self.arabic_font = font_ids[1];
      self.cjk_font = font_ids[2];
   }

   fn set_shared(&mut self, thumbnail_atlas: gfx::ImageHandle, font_ids: [usize; 3])
   {
      self.thumbnail_atlas = thumbnail_atlas;
      self.set_fonts(font_ids);
   }

   fn thumbnail_atlas(&self, accented: bool) -> gfx::ImageHandle
   {
      if accented && self.accented_thumbnail_atlas.0 != 0
      {
         self.accented_thumbnail_atlas
      }
      else
      {
         self.thumbnail_atlas
      }
   }

   fn set_inline_text(&mut self, images: Vec<gfx::ImageHandle>, atlas: InlineTextAtlas) -> Result<(), String>
   {
      if images.len() != atlas.variants.len() || images.iter().any(|image| image.0 == 0)
      {
         return Err(String::from("inline-text image handles do not match the raster variants"));
      }
      self.inline_text = Some(InlineTextResources {images, atlas});
      Ok(())
   }
}

pub enum ComparisonScene
{
   Startup(StartupScene),
   Dashboard(DashboardScene),
   Endurance(EnduranceScene),
   Idle(DashboardScene),
   Feed(FeedScene),
   Chat(ChatScene),
   Navigation(NavigationScene),
   Image(ImageScene),
   Grid(GridScene),
   Effects(EffectsScene),
   Mutation(MutationScene),
   Text(TextScene),
   Resize(ResizeScene),
}

impl ComparisonScene
{
   pub fn prepare(scenario: &ScenarioSpec, fixture: &[u8]) -> Result<Self, String>
   {
      Self::prepare_id(&scenario.id, fixture)
   }

   pub fn prepare_id(scenario_id: &str, fixture: &[u8]) -> Result<Self, String>
   {
      match scenario_id
      {
         "startup.first-screen" => serde_json::from_slice::<StartupFixture>(fixture).map(StartupScene::new).map(Self::Startup).map_err(|error| error.to_string()),
         "dashboard.mixed-static" => serde_json::from_slice::<DashboardFixture>(fixture).map(DashboardScene::new).map(Self::Dashboard).map_err(|error| error.to_string()),
         "endurance.churn" => serde_json::from_slice::<EnduranceFixture>(fixture).map(EnduranceScene::new).map(Self::Endurance).map_err(|error| error.to_string()),
         "idle.steady" => serde_json::from_slice::<DashboardFixture>(fixture).map(DashboardScene::new).map(Self::Idle).map_err(|error| error.to_string()),
         "feed.variable-scroll" => serde_json::from_slice::<FeedFixture>(fixture).map(FeedScene::new).map(Self::Feed).map_err(|error| error.to_string()),
         "chat.live-update" => serde_json::from_slice::<ChatFixture>(fixture).map(ChatScene::new).map(Self::Chat).map_err(|error| error.to_string()),
         "navigation.modal" => serde_json::from_slice::<NavigationFixture>(fixture).map(NavigationScene::new).map(Self::Navigation).map_err(|error| error.to_string()),
         "image.decode-zoom" => serde_json::from_slice::<ImageDecodeZoomFixture>(fixture).map(ImageScene::new).map(Self::Image).map_err(|error| error.to_string()),
         "grid.large-scroll" => serde_json::from_slice::<GridFixture>(fixture).map(GridScene::new).map(Self::Grid).map_err(|error| error.to_string()),
         "effects.layers" => serde_json::from_slice::<EffectsFixture>(fixture).map(EffectsScene::new).map(Self::Effects).map_err(|error| error.to_string()),
         "mutation.damage" => serde_json::from_slice::<MutationFixture>(fixture).map(MutationScene::new).map(Self::Mutation).map_err(|error| error.to_string()),
         "text.multilingual" => serde_json::from_slice::<TextFixture>(fixture).map(TextScene::new).map(Self::Text).map_err(|error| error.to_string()),
         "resize.theme" => serde_json::from_slice::<ResizeFixture>(fixture).map(ResizeScene::new).map(Self::Resize).map_err(|error| error.to_string()),
         _ => Err(format!("unsupported comparison scenario {scenario_id}")),
      }
   }

   pub fn set_resources(&mut self, thumbnail_atlas: gfx::ImageHandle, font_ids: [usize; 3])
   {
      match self
      {
         Self::Startup(scene) => scene.resources.set_shared(thumbnail_atlas, font_ids),
         Self::Dashboard(scene) => scene.resources.set_shared(thumbnail_atlas, font_ids),
         Self::Endurance(scene) => scene.set_resources(thumbnail_atlas, font_ids),
         Self::Idle(scene) => scene.resources.set_shared(thumbnail_atlas, font_ids),
         Self::Feed(scene) => scene.resources.set_shared(thumbnail_atlas, font_ids),
         Self::Chat(scene) => scene.resources.set_shared(thumbnail_atlas, font_ids),
         Self::Navigation(scene) => scene.resources.set_shared(thumbnail_atlas, font_ids),
         Self::Image(scene) => scene.resources.set_shared(thumbnail_atlas, font_ids),
         Self::Grid(scene) => scene.resources.set_shared(thumbnail_atlas, font_ids),
         Self::Effects(scene) => scene.resources.set_shared(thumbnail_atlas, font_ids),
         Self::Mutation(scene) => scene.resources.set_shared(thumbnail_atlas, font_ids),
         Self::Text(scene) => scene.resources.set_shared(thumbnail_atlas, font_ids),
         Self::Resize(scene) => scene.dashboard.resources.set_shared(thumbnail_atlas, font_ids),
      }
   }

   pub fn set_thumbnail_variant_resource(&mut self, accented_thumbnail_atlas: gfx::ImageHandle) -> Result<(), String>
   {
      if accented_thumbnail_atlas.0 == 0
      {
         return Err(String::from("accented thumbnail atlas handle is invalid"));
      }
      let Self::Grid(scene) = self else
      {
         return Err(String::from("accented thumbnail atlas is only valid for grid.large-scroll"));
      };
      scene.resources.accented_thumbnail_atlas = accented_thumbnail_atlas;
      Ok(())
   }

   pub fn set_fonts(&mut self, font_ids: [usize; 3])
   {
      match self
      {
         Self::Startup(scene) => scene.resources.set_fonts(font_ids),
         Self::Dashboard(scene) => scene.resources.set_fonts(font_ids),
         Self::Endurance(scene) => scene.set_fonts(font_ids),
         Self::Idle(scene) => scene.resources.set_fonts(font_ids),
         Self::Feed(scene) => scene.resources.set_fonts(font_ids),
         Self::Chat(scene) => scene.resources.set_fonts(font_ids),
         Self::Navigation(scene) => scene.resources.set_fonts(font_ids),
         Self::Image(scene) => scene.resources.set_fonts(font_ids),
         Self::Grid(scene) => scene.resources.set_fonts(font_ids),
         Self::Effects(scene) => scene.resources.set_fonts(font_ids),
         Self::Mutation(scene) => scene.resources.set_fonts(font_ids),
         Self::Text(scene) => scene.resources.set_fonts(font_ids),
         Self::Resize(scene) => scene.dashboard.resources.set_fonts(font_ids),
      }
   }

   pub fn set_inline_text_resources(&mut self, images: Vec<gfx::ImageHandle>, atlas: InlineTextAtlas) -> Result<(), String>
   {
      match self
      {
         Self::Startup(scene) => scene.resources.set_inline_text(images, atlas),
         Self::Dashboard(scene) => scene.resources.set_inline_text(images, atlas),
         Self::Endurance(scene) => scene.set_inline_text_resources(images, atlas),
         Self::Idle(scene) => scene.resources.set_inline_text(images, atlas),
         Self::Feed(scene) => scene.resources.set_inline_text(images, atlas),
         Self::Chat(scene) => scene.resources.set_inline_text(images, atlas),
         Self::Navigation(scene) => scene.resources.set_inline_text(images, atlas),
         Self::Image(scene) => scene.resources.set_inline_text(images, atlas),
         Self::Grid(scene) => scene.resources.set_inline_text(images, atlas),
         Self::Effects(scene) => scene.resources.set_inline_text(images, atlas),
         Self::Mutation(scene) => scene.resources.set_inline_text(images, atlas),
         Self::Text(scene) => scene.resources.set_inline_text(images, atlas),
         Self::Resize(scene) => scene.dashboard.resources.set_inline_text(images, atlas),
      }
   }

   pub fn set_image_resources(&mut self, source: gfx::ImageHandle, source_sha256: &str, thumbnail: gfx::ImageHandle, thumbnail_sha256: &str) -> Result<(), String>
   {
      let Self::Image(scene) = self else {return Err(String::from("comparison scenario does not consume standalone images"))};
      scene.set_thumbnail_resource(thumbnail, thumbnail_sha256)?;
      scene.set_source_resource(source, source_sha256)
   }

   pub fn set_image_thumbnail_resource(&mut self, thumbnail: gfx::ImageHandle, thumbnail_sha256: &str) -> Result<(), String>
   {
      let Self::Image(scene) = self else {return Err(String::from("comparison scenario does not consume standalone images"))};
      scene.set_thumbnail_resource(thumbnail, thumbnail_sha256)
   }

   pub fn set_image_source_resource(&mut self, source: gfx::ImageHandle, source_sha256: &str) -> Result<(), String>
   {
      let Self::Image(scene) = self else {return Err(String::from("comparison scenario does not consume standalone images"))};
      scene.set_source_resource(source, source_sha256)
   }

   pub fn update(&mut self, dt_ms: u32)
   {
      if let Self::Navigation(scene) = self
      {
         scene.update(dt_ms);
      }
   }

   pub fn wants_next_frame(&self) -> bool
   {
      matches!(self, Self::Navigation(scene) if scene.transition.is_some())
   }

   pub fn apply(&mut self, event: &TraceEvent) -> Result<(), String>
   {
      match self
      {
         Self::Startup(scene) => scene.apply(event),
         Self::Dashboard(scene) => scene.apply(event),
         Self::Endurance(scene) => scene.apply(event),
         Self::Idle(_) => Err(String::from("idle.steady rejects all logical events")),
         Self::Feed(scene) => scene.apply(event),
         Self::Chat(scene) => scene.apply(event),
         Self::Navigation(scene) => scene.apply(event),
         Self::Image(scene) => scene.apply(event),
         Self::Grid(scene) => scene.apply(event),
         Self::Effects(scene) => scene.apply(event),
         Self::Mutation(scene) => scene.apply(event),
         Self::Text(scene) => scene.apply(event),
         Self::Resize(scene) => scene.apply(event),
      }
   }

   pub fn input_pointer(&mut self, x: f32, y: f32, dx: f32, dy: f32, buttons: u32)
   {
      match self
      {
         Self::Feed(scene) => scene.input_pointer(x, y, dy, buttons),
         Self::Image(scene) => scene.input_pointer(x, y, dx, dy, buttons),
         _ => {}
      }
   }

   pub fn host_click(&mut self, x: f32, y: f32) -> bool
   {
      match self
      {
         Self::Feed(scene) => scene.host_click(x, y),
         Self::Chat(scene) => scene.host_click(x, y),
         Self::Navigation(scene) => scene.host_click(x, y),
         Self::Grid(scene) => scene.host_click(x, y),
         _ => false,
      }
   }

   pub fn host_pointer_down(&mut self, x: f32, y: f32) -> bool
   {
      match self
      {
         Self::Navigation(scene) => scene.host_pointer_down(x, y),
         Self::Image(scene) => scene.host_pointer_down(x, y),
         _ => false,
      }
   }

   pub fn host_pointer_move(&mut self, x: f32, y: f32, dx: f32, dy: f32) -> bool
   {
      match self
      {
         Self::Feed(scene) => scene.host_pointer_delta(dy),
         Self::Navigation(scene) => scene.host_pointer_move(x, y),
         Self::Image(scene) => scene.host_pointer_move(x, y, dx, dy),
         _ => false,
      }
   }

   pub fn host_pointer_up(&mut self) -> bool
   {
      match self
      {
         Self::Navigation(scene) => scene.host_pointer_up(),
         Self::Image(scene) => scene.host_pointer_up(),
         _ => false,
      }
   }

   pub fn host_pointer_delta(&mut self, dx: f32, dy: f32) -> bool
   {
      match self
      {
         Self::Feed(scene) => scene.host_pointer_delta(dy),
         Self::Image(scene) => scene.host_pointer_delta(dx, dy),
         _ => false,
      }
   }

   pub fn host_wheel(&mut self, delta_y_millionths: i32) -> bool
   {
      match self
      {
         Self::Grid(scene) => scene.host_wheel(delta_y_millionths),
         _ => false,
      }
   }

   pub fn host_text(&mut self, value: &str) -> bool
   {
      match self
      {
         Self::Chat(scene) => scene.host_text(value),
         _ => false,
      }
   }

   pub fn host_key(&mut self, key_code: u16, shift: bool, value: &str) -> bool
   {
      match self
      {
         Self::Chat(scene) => scene.host_key(key_code, shift, value),
         _ => false,
      }
   }

   pub fn host_chat_selection_text(&self) -> Option<&str>
   {
      let Self::Chat(scene) = self else {return None};
      scene.selection_text()
   }

   pub fn role_counts(&self) -> Vec<RoleCount>
   {
      match self
      {
         Self::Startup(scene) => scene.role_counts(),
         Self::Dashboard(scene) => scene.role_counts(),
         Self::Endurance(scene) => scene.role_counts(),
         Self::Idle(scene) => scene.role_counts(),
         Self::Feed(scene) => scene.role_counts(),
         Self::Chat(scene) => scene.role_counts(),
         Self::Navigation(scene) => scene.role_counts(),
         Self::Image(scene) => scene.role_counts(),
         Self::Grid(scene) => scene.role_counts(),
         Self::Effects(scene) => scene.role_counts(),
         Self::Mutation(scene) => scene.role_counts(),
         Self::Text(scene) => scene.role_counts(),
         Self::Resize(scene) => scene.role_counts(),
      }
   }

   pub fn checkpoint_json(&self, checkpoint_id: &str) -> Result<(Vec<u8>, Vec<u8>), String>
   {
      let (scenario_id, model) = match self
      {
         Self::Startup(scene) => ("startup.first-screen", scene.checkpoint_model()),
         Self::Dashboard(scene) => ("dashboard.mixed-static", scene.checkpoint_model()),
         Self::Endurance(scene) => ("endurance.churn", scene.checkpoint_model()),
         Self::Idle(scene) => ("idle.steady", scene.checkpoint_model()),
         Self::Feed(scene) => ("feed.variable-scroll", scene.checkpoint_model()),
         Self::Chat(scene) => ("chat.live-update", scene.checkpoint_model()),
         Self::Navigation(scene) => ("navigation.modal", scene.checkpoint_model()),
         Self::Image(scene) => ("image.decode-zoom", scene.checkpoint_model()),
         Self::Grid(scene) => ("grid.large-scroll", scene.checkpoint_model()),
         Self::Effects(scene) => ("effects.layers", scene.checkpoint_model()),
         Self::Mutation(scene) => ("mutation.damage", scene.checkpoint_model()),
         Self::Text(scene) => ("text.multilingual", scene.checkpoint_model()),
         Self::Resize(scene) => return scene.checkpoint_json(checkpoint_id),
      };
      semantic_checkpoint_json(scenario_id, checkpoint_id, self.role_counts(), model)
   }

   pub fn draw<U: elements::ImageUploader>(&mut self, viewport: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder)
   {
      self.draw_with_geometry(viewport, scale, text, uploader, builder, &mut IgnoreGeometry);
   }

   #[cfg(feature = "comparison-geometry")]
   pub fn capture_geometry<U: elements::ImageUploader>(&mut self, viewport: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder) -> Vec<ComparisonGeometryNode>
   {
      let mut geometry = CaptureGeometry::default();
      self.draw_with_geometry(viewport, scale, text, uploader, builder, &mut geometry);
      geometry.nodes
   }

   fn draw_with_geometry<U: elements::ImageUploader, G: GeometrySink>(&mut self, viewport: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder, geometry: &mut G)
   {
      match self
      {
         Self::Startup(scene) => scene.draw(viewport, scale, text, uploader, builder, geometry),
         Self::Dashboard(scene) => scene.draw(viewport, scale, text, uploader, builder, geometry, "dashboard", "dashboard", None),
         Self::Endurance(scene) => scene.draw(viewport, scale, text, uploader, builder, geometry),
         Self::Idle(scene) => scene.draw(viewport, scale, text, uploader, builder, geometry, "dashboard", "dashboard", None),
         Self::Feed(scene) => scene.draw(viewport, scale, text, uploader, builder, geometry),
         Self::Chat(scene) => scene.draw(viewport, scale, text, uploader, builder, geometry),
         Self::Navigation(scene) => scene.draw(viewport, scale, text, uploader, builder, geometry),
         Self::Image(scene) => scene.draw(viewport, scale, text, uploader, builder, geometry),
         Self::Grid(scene) => scene.draw(viewport, scale, text, uploader, builder, geometry),
         Self::Effects(scene) => scene.draw(viewport, scale, text, uploader, builder, geometry),
         Self::Mutation(scene) => scene.draw(viewport, scale, text, uploader, builder, geometry),
         Self::Text(scene) => scene.draw(viewport, scale, text, uploader, builder, geometry),
         Self::Resize(scene) => scene.draw(viewport, scale, text, uploader, builder, geometry),
      }
   }
}

fn counts(values: &[(&str, u32)]) -> Vec<RoleCount>
{
   values.iter().map(|(role, count)| RoleCount {role: (*role).to_string(), count: *count}).collect()
}

fn semantic_checkpoint_json(scenario_id: &str, checkpoint_id: &str, roles: Vec<RoleCount>, model: serde_json::Value) -> Result<(Vec<u8>, Vec<u8>), String>
{
   semantic_checkpoint_json_with_root(scenario_id, checkpoint_id, roles, model, [0, 0, 390, 844])
}

fn semantic_checkpoint_json_with_root(scenario_id: &str, checkpoint_id: &str, roles: Vec<RoleCount>, model: serde_json::Value, root_frame: [i32; 4]) -> Result<(Vec<u8>, Vec<u8>), String>
{
   let raw_tree_source = match scenario_id
   {
      "grid.large-scroll" | "effects.layers" | "mutation.damage" | "text.multilingual" | "resize.theme" => "framework-neutral-release-contract",
      _ => "runtime-semantic-tree",
   };
   let state = serde_json::json!({
      "schema_version": 2,
      "scenario_id": scenario_id,
      "checkpoint_id": checkpoint_id,
      "model": model,
      "visible_role_counts": roles,
   });
   let nodes = roles.iter().enumerate().map(|(order, role)|
   {
      let visible = role.count > 0;
      serde_json::json!({
         "role": role.role,
         "name": role.role,
         "value": role.count.to_string(),
         "state": if visible {vec!["enabled", "visible"]} else {vec!["hidden"]},
         "order": order,
         "focused": semantic_role_focused(scenario_id, &role.role, &model),
         "actions": semantic_role_actions(&role.role),
         "frame": semantic_role_frame(scenario_id, &role.role),
         "count": role.count,
         "visible": visible,
      })
   }).collect::<Vec<_>>();
   let accessibility = serde_json::json!({
      "schema_version": 2,
      "scenario_id": scenario_id,
      "checkpoint_id": checkpoint_id,
      "root_frame": root_frame,
      "raw_tree_source": raw_tree_source,
      "nodes": nodes,
   });
   Ok((
      serde_json::to_vec(&state).map_err(|error| error.to_string())?,
      serde_json::to_vec(&accessibility).map_err(|error| error.to_string())?,
   ))
}

pub struct GridScene
{
   fixture: GridFixture,
   scroll_millionths: i32,
   detail_visible: bool,
   resources: ComparisonResources,
}

impl GridScene
{
   const COLUMNS: u32 = 3;
   const ITEM_HEIGHT: u64 = 144;
   const LINE_SPACING: u64 = 8;
   const SECTION_TOP: u64 = 4;
   const SECTION_BOTTOM: u64 = 12;
   const VIEWPORT_TOP: f32 = 48.0;
   const VIEWPORT_HEIGHT: u64 = 796;

   fn new(fixture: GridFixture) -> Self
   {
      Self {fixture, scroll_millionths: 0, detail_visible: false, resources: ComparisonResources::default()}
   }

   fn visible_origin(&self) -> (u32, f32)
   {
      let rows = (self.fixture.tile_count as u64 + Self::COLUMNS as u64 - 1) / Self::COLUMNS as u64;
      let line_spacing = rows.saturating_sub(1) * Self::LINE_SPACING;
      let content_height = Self::SECTION_TOP + rows * Self::ITEM_HEIGHT + line_spacing + Self::SECTION_BOTTOM;
      let maximum = content_height.saturating_sub(Self::VIEWPORT_HEIGHT);
      let offset = maximum as f64 * self.scroll_millionths as f64 / 1_000_000.0;
      let row = (offset / (Self::ITEM_HEIGHT + Self::LINE_SPACING) as f64).floor() as u64;
      let first = (row * Self::COLUMNS as u64).min(self.fixture.tile_count.saturating_sub(1) as u64) as u32;
      let y = Self::VIEWPORT_TOP + Self::SECTION_TOP as f32 + (row * (Self::ITEM_HEIGHT + Self::LINE_SPACING)) as f32 - offset as f32;
      (first, y)
   }

   fn apply(&mut self, event: &TraceEvent) -> Result<(), String>
   {
      match event.op
      {
         TraceOperation::Wheel if event.target.as_deref() == Some("grid:collection") =>
         {
            self.scroll_millionths = self.scroll_millionths.saturating_add(event.delta_y_millionths.unwrap_or(0)).clamp(0, 1_000_000);
         }
         TraceOperation::PointerDown | TraceOperation::PointerUp => {}
         TraceOperation::Navigate if event.target.as_deref() == Some("grid:detail:07500") => self.detail_visible = true,
         TraceOperation::Navigate if event.target.as_deref() == Some("grid:collection") => self.detail_visible = false,
         _ => return Err(format!("grid rejects {:?}", event.op)),
      }
      Ok(())
   }

   fn host_wheel(&mut self, delta_y_millionths: i32) -> bool
   {
      let previous = self.scroll_millionths;
      self.scroll_millionths = self.scroll_millionths.saturating_add(delta_y_millionths).clamp(0, 1_000_000);
      self.scroll_millionths != previous
   }

   fn host_click(&mut self, x: f32, y: f32) -> bool
   {
      if self.detail_visible
      {
         if point_in_rect(x, y, gfx::RectF::new(16.0, 16.0, 72.0, 44.0))
         {
            self.detail_visible = false;
            return true;
         }
         return false;
      }
      let (first, first_y) = self.visible_origin();
      for visible in 0..18_u32
      {
         let row = visible / 3;
         let column = visible % 3;
         let tile = first + visible;
         let rect = gfx::RectF::new(12.0 + column as f32 * 124.0, first_y + row as f32 * 152.0, 116.0, 144.0);
         if tile == self.fixture.detail_tile_index && point_in_rect(x, y, rect)
         {
            self.detail_visible = true;
            return true;
         }
      }
      false
   }

   fn role_counts(&self) -> Vec<RoleCount>
   {
      if self.detail_visible
      {
         counts(&[("detail-view", 1), ("detail-thumbnail", 1), ("detail-title", 1), ("back-control", 1)])
      }
      else
      {
         counts(&[("thumbnail-grid", 1), ("grid-tile", 18), ("thumbnail", 18), ("tile-label", 18)])
      }
   }

   fn checkpoint_model(&self) -> serde_json::Value
   {
      serde_json::json!({
         "detail_tile_id": if self.detail_visible {serde_json::Value::String(format!("grid:tile:{:05}", self.fixture.detail_tile_index))} else {serde_json::Value::Null},
         "route": if self.detail_visible {"detail"} else {"grid"},
         "scroll_position_millionths": self.scroll_millionths,
         "thumbnail_count": self.fixture.thumbnail_count,
         "tile_count": self.fixture.tile_count,
      })
   }

   fn draw<U: elements::ImageUploader, G: GeometrySink>(&self, viewport: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder, geometry: &mut G)
   {
      encode_solid_rects(builder, [viewport], BACKGROUND);
      if self.detail_visible
      {
         let back = gfx::RectF::new(16.0, 16.0, 72.0, 44.0);
         let thumbnail = gfx::RectF::new(24.0, 92.0, 342.0, 342.0);
         let title = gfx::RectF::new(24.0, 452.0, 342.0, 32.0);
         let _ = encode_comparison_label("‹ Back", ACCENT, elements::Align::Left, self.resources.latin_font, 15.0, back, scale, text, uploader, builder);
         let accented = self.fixture.detail_tile_index % 256 >= 128;
         encode_rounded_atlas_image(builder, self.resources.thumbnail_atlas(accented), thumbnail, atlas_source(self.fixture.detail_tile_index % 128), 12.0);
         let title_line = encode_comparison_label(&format!("Tile {:05}", self.fixture.detail_tile_index), TEXT, elements::Align::Left, self.resources.latin_font, 20.0, title, scale, text, uploader, builder);
         geometry.record("detail-view", "grid.detail", viewport, &[]);
         geometry.record("detail-thumbnail", "grid.detail.thumbnail", thumbnail, &[]);
         geometry.record("detail-title", "grid.detail.title", title, &[title_line]);
         geometry.record("back-control", "grid.back", back, &[back]);
         return;
      }
      geometry.record("thumbnail-grid", "grid.collection", viewport, &[]);
      let heading = gfx::RectF::new(16.0, 8.0, 240.0, 32.0);
      geometry.record("heading", "grid.heading", heading, &[heading]);
      let (first, first_y) = self.visible_origin();
      builder.clip_push(gfx::RectI::new(0, 48, 390, 796));
      for visible in 0..18_u32
      {
         let row = visible / 3;
         let column = visible % 3;
         let tile = first + visible;
         let rect = gfx::RectF::new(12.0 + column as f32 * 124.0, first_y + row as f32 * 152.0, 116.0, 144.0);
         builder.rrect(rect, [12.0; 4], SURFACE);
         let thumbnail = gfx::RectF::new(rect.x + 8.0, rect.y + 8.0, 100.0, 104.0);
         let label = gfx::RectF::new(rect.x + 8.0, rect.y + 116.0, 100.0, 20.0);
         encode_rounded_atlas_image(builder, self.resources.thumbnail_atlas(tile % 256 >= 128), thumbnail, atlas_source(tile % 128), 8.0);
         let label_line = encode_comparison_label(&format!("Tile {:05}", tile), TEXT, elements::Align::Left, self.resources.latin_font, 11.0, label, scale, text, uploader, builder);
         geometry.record_indexed("grid-tile", "grid:tile:", tile as usize, 5, rect, &[]);
         geometry.record_indexed("thumbnail", "grid:thumbnail:", tile as usize, 5, thumbnail, &[]);
         geometry.record_indexed("tile-label", "grid:label:", tile as usize, 5, label, &[label_line]);
      }
      builder.clip_pop();
      encode_comparison_label("Thumbnail Grid", TEXT, elements::Align::Left, self.resources.latin_font, 20.0, heading, scale, text, uploader, builder);
   }
}

pub struct EffectsScene
{
   fixture: EffectsFixture,
   animation_progress: u32,
   dirty_generation: u32,
   resources: ComparisonResources,
}

impl EffectsScene
{
   fn new(fixture: EffectsFixture) -> Self
   {
      Self {fixture, animation_progress: 0, dirty_generation: 0, resources: ComparisonResources::default()}
   }

   fn apply(&mut self, event: &TraceEvent) -> Result<(), String>
   {
      match (event.op, event.target.as_deref(), event.value.as_ref())
      {
         (TraceOperation::ResourceArrival, Some("effects:layer-resources"), _) => {}
         (TraceOperation::Mutate, Some("effects:animation-progress-millionths"), Some(TraceValue::Integer(value))) if (0..=1_000_000).contains(value) => self.animation_progress = *value as u32,
         (TraceOperation::Mutate, Some("effects:layer:037:dirty-generation"), Some(TraceValue::Integer(value))) if *value >= 0 => self.dirty_generation = *value as u32,
         _ => return Err(format!("effects rejects {:?}", event.op)),
      }
      Ok(())
   }

   fn role_counts(&self) -> Vec<RoleCount>
   {
      counts(&[("effects-scene", 1), ("rounded-card", self.fixture.card_count), ("clip-region", self.fixture.clip_count), ("shadow", self.fixture.shadow_count), ("backdrop-region", self.fixture.backdrop_blur_count)])
   }

   fn checkpoint_model(&self) -> serde_json::Value
   {
      serde_json::json!({
         "animation_progress_millionths": self.animation_progress,
         "backdrop_blur_count": self.fixture.backdrop_blur_count,
         "card_count": self.fixture.card_count,
         "dirty_layer_generation": self.dirty_generation,
         "shadow_count": self.fixture.shadow_count,
      })
   }

   fn draw<U: elements::ImageUploader, G: GeometrySink>(&self, viewport: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder, geometry: &mut G)
   {
      encode_solid_rects(builder, [viewport], BACKGROUND);
      geometry.record("effects-scene", "effects.scene", viewport, &[]);
      let heading = gfx::RectF::new(16.0, 8.0, 280.0, 32.0);
      geometry.record("heading", "effects.heading", heading, &[heading]);
      encode_comparison_label("Layer Effects", TEXT, elements::Align::Left, self.resources.latin_font, 20.0, heading, scale, text, uploader, builder);
      let t = self.animation_progress as f32 / 1_000_000.0;
      let card_alpha = 0.65 + 0.35 * t;
      let offset_x = self.fixture.animation.translation_x_points as f32 * t;
      let offset_y = self.fixture.animation.translation_y_points as f32 * t;
      for index in 0..self.fixture.card_count
      {
         let row = index / 10;
         let column = index % 10;
         let rect = gfx::RectF::new(8.0 + column as f32 * 74.0 + offset_x, 64.0 + row as f32 * 74.0 + offset_y, 68.0, 68.0);
         geometry.record_indexed("rounded-card", "effects:card:", index as usize, 3, rect, &[]);
         if index < self.fixture.clip_count {geometry.record_indexed("clip-region", "effects:clip:", index as usize, 3, rect, &[]);}
         if index < self.fixture.shadow_count {geometry.record_indexed("shadow", "effects:shadow:", index as usize, 3, rect, &[]);}
         if [5, 17, 29, 41, 53, 65, 77, 89].contains(&index) {geometry.record_indexed("backdrop-region", "effects:backdrop:", index as usize, 3, rect, &[]);}
         builder.layer_begin(index, rect, index == self.fixture.dirty_layer_index && self.dirty_generation > 0);
         if index < self.fixture.shadow_count
         {
            builder.rrect(
               gfx::RectF::new(rect.x, rect.y + 2.0, rect.w, rect.h),
               [12.0; 4],
               gfx::Color::rgba(SHADOW.r, SHADOW.g, SHADOW.b, SHADOW.a * card_alpha),
            );
         }
         if [5, 17, 29, 41, 53, 65, 77, 89].contains(&index)
         {
            builder.backdrop(rect, self.fixture.backdrop_blur_radius as f32, EFFECTS_MATERIAL, 1.0);
         }
         builder.clip_push(gfx::RectI::new(rect.x as i32, rect.y as i32, rect.w.ceil() as i32, rect.h.ceil() as i32));
         builder.rrect(rect, [self.fixture.corner_radius as f32; 4], gfx::Color::rgba(SURFACE.r, SURFACE.g, SURFACE.b, card_alpha));
         builder.clip_pop();
         builder.layer_end();
      }
   }
}

pub struct MutationScene
{
   fixture: MutationFixture,
   mutation_class: Option<usize>,
   generation: u32,
   changed: Vec<bool>,
   resources: ComparisonResources,
}

impl MutationScene
{
   fn new(fixture: MutationFixture) -> Self
   {
      let changed = vec![false; fixture.node_count as usize];
      Self {fixture, mutation_class: None, generation: 0, changed, resources: ComparisonResources::default()}
   }

   fn apply(&mut self, event: &TraceEvent) -> Result<(), String>
   {
      let Some(target) = event.target.as_deref() else {return Err(String::from("mutation event has no target"))};
      let Some(class_id) = target.strip_prefix("mutation:").and_then(|value| value.strip_suffix(":generation")) else {return Err(String::from("mutation target is unsupported"))};
      let Some(class_index) = self.fixture.mutation_classes.iter().position(|class| class.id == class_id) else {return Err(String::from("mutation class is unsupported"))};
      let Some(TraceValue::Integer(generation)) = event.value.as_ref() else {return Err(String::from("mutation generation is absent"))};
      if event.op != TraceOperation::Mutate || !(1..=self.fixture.mutation_repetitions as i64).contains(generation)
      {
         return Err(String::from("mutation generation is outside the frozen contract"));
      }
      self.changed.fill(false);
      let class = &self.fixture.mutation_classes[class_index];
      for ordinal in 0..class.changed_node_count
      {
         let index = (self.fixture.selection_formula.seed as u64
            + *generation as u64 * self.fixture.selection_formula.increment as u64
            + ordinal as u64 * self.fixture.selection_formula.multiplier as u64)
            % self.fixture.selection_formula.modulus as u64;
         self.changed[index as usize] = true;
      }
      self.mutation_class = Some(class_index);
      self.generation = *generation as u32;
      Ok(())
   }

   fn role_counts(&self) -> Vec<RoleCount>
   {
      counts(&[("mutation-surface", 1), ("simple-node", self.fixture.node_count)])
   }

   fn checkpoint_model(&self) -> serde_json::Value
   {
      let class = self.mutation_class.map(|index| &self.fixture.mutation_classes[index]);
      serde_json::json!({
         "changed_node_count": class.map(|class| class.changed_node_count).unwrap_or(0),
         "generation": self.generation,
         "mutation_class": class.map(|class| class.id.as_str()),
         "node_count": self.fixture.node_count,
      })
   }

   fn draw<U: elements::ImageUploader, G: GeometrySink>(&self, viewport: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder, geometry: &mut G)
   {
      encode_solid_rects(builder, [viewport], BACKGROUND);
      geometry.record("mutation-surface", "mutation.surface", viewport, &[]);
      let heading = gfx::RectF::new(16.0, 8.0, 300.0, 32.0);
      geometry.record("heading", "mutation.heading", heading, &[heading]);
      encode_comparison_label("10,000-node mutation", TEXT, elements::Align::Left, self.resources.latin_font, 20.0, heading, scale, text, uploader, builder);
      let rect = |index: usize|
      {
         let row = index / 100;
         let column = index % 100;
         gfx::RectF::new(8.0 + column as f32 * 11.0 / 3.0, 52.0 + row as f32 * 22.0 / 3.0, 10.0 / 3.0, 7.0)
      };
      encode_solid_rects(builder, (0..self.changed.len()).filter(|index| !self.changed[*index]).map(|index| {let bounds = rect(index); geometry.record_indexed("simple-node", "mutation:node:", index, 5, bounds, &[]); bounds}), SURFACE);
      encode_solid_rects(builder, (0..self.changed.len()).filter(|index| self.changed[*index]).map(|index| {let bounds = rect(index); geometry.record_indexed("simple-node", "mutation:node:", index, 5, bounds, &[]); bounds}), ACCENT);
   }
}

pub struct TextScene
{
   fixture: TextFixture,
   font_atlas_cold: bool,
   replay_generation: u32,
   scale_millionths: u32,
   wrap_rotation: u32,
   resources: ComparisonResources,
}

impl TextScene
{
   fn new(fixture: TextFixture) -> Self
   {
      Self {fixture, font_atlas_cold: true, replay_generation: 0, scale_millionths: 1_000_000, wrap_rotation: 0, resources: ComparisonResources::default()}
   }

   fn apply(&mut self, event: &TraceEvent) -> Result<(), String>
   {
      match (event.op, event.target.as_deref(), event.value.as_ref())
      {
         (TraceOperation::ResourceArrival, Some("text:font-pack" | "text:inline-atlas"), _) => {}
         (TraceOperation::Mutate, Some("text:replay-generation"), Some(TraceValue::Integer(value))) if *value >= 0 =>
         {
            self.replay_generation = *value as u32;
            self.font_atlas_cold = false;
         }
         (TraceOperation::Scale, Some("text:scale-millionths"), Some(TraceValue::Integer(value))) if *value > 0 => self.scale_millionths = *value as u32,
         (TraceOperation::Mutate, Some("text:wrap-width-rotation"), Some(TraceValue::Integer(value))) if *value >= 0 => self.wrap_rotation = *value as u32,
         _ => return Err(format!("text rejects {:?}", event.op)),
      }
      Ok(())
   }

   fn role_counts(&self) -> Vec<RoleCount>
   {
      counts(&[("text-surface", 1), ("multilingual-label", 12)])
   }

   fn checkpoint_model(&self) -> serde_json::Value
   {
      serde_json::json!({
         "font_atlas_cold": self.font_atlas_cold,
         "label_count": self.fixture.label_count,
         "replay_generation": self.replay_generation,
         "scale_millionths": self.scale_millionths,
         "wrap_rotation": self.wrap_rotation,
      })
   }

   fn draw<U: elements::ImageUploader, G: GeometrySink>(&self, viewport: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder, geometry: &mut G)
   {
      encode_solid_rects(builder, [viewport], BACKGROUND);
      geometry.record("text-surface", "text.surface", viewport, &[]);
      let heading = gfx::RectF::new(16.0, 8.0, 280.0, 32.0);
      geometry.record("heading", "text.heading", heading, &[heading]);
      encode_comparison_label("Multilingual Text", TEXT, elements::Align::Left, self.resources.latin_font, 20.0, heading, scale, text, uploader, builder);
      let wrap = self.fixture.wrap_width_rotation[self.wrap_rotation as usize % self.fixture.wrap_width_rotation.len()] as f32;
      for index in 0..40_usize
      {
         let category = &self.fixture.categories[index % self.fixture.categories.len()];
         let font = font_for_text(&self.resources, &category.text);
         let row = gfx::RectF::new(16.0, 40.0 + index as f32 * 72.0, wrap.min(358.0), 72.0);
         let _ = encode_wrapped_inline_text(
            &category.text,
            TEXT,
            elements::Align::Left,
            if category.direction == "rtl" {2} else {3},
            font,
            15.0 * self.scale_millionths as f32 / 1_000_000.0,
            row,
            scale,
            &self.resources,
            text,
            uploader,
            builder,
         );
         if row.y < viewport.y + viewport.h
         {
            geometry.record_indexed("multilingual-label", "text:label:", index, 3, row, &[row]);
         }
      }
   }
}

pub struct ResizeScene
{
   fixture: ResizeFixture,
   change_index: u32,
   orientation: String,
   theme: String,
   width: u32,
   height: u32,
   dashboard: DashboardScene,
}

impl ResizeScene
{
   fn new(fixture: ResizeFixture) -> Self
   {
      let dashboard = DashboardScene::new(DashboardFixture {
         schema_version: 1,
         id: String::from("dashboard.mixed-static"),
         visible_node_count: 300,
         categories: DashboardCategories {label: 176, icon_image: 64, rounded_card: 32, control: 24, backdrop_region: 4},
         clipped_rounded_cards: 32,
         shadow_count: 32,
         backdrop_blur_count: 4,
         leaf_update_sequence: Vec::new(),
         update_10_percent_ids: Vec::new(),
      });
      Self {
         change_index: 0,
         orientation: fixture.initial_orientation.clone(),
         theme: fixture.initial_theme.clone(),
         width: fixture.initial_viewport.width,
         height: fixture.initial_viewport.height,
         fixture,
         dashboard,
      }
   }

   fn apply(&mut self, event: &TraceEvent) -> Result<(), String>
   {
      let next = self.fixture.changes.get(self.change_index as usize).ok_or_else(|| String::from("resize change sequence is exhausted"))?;
      match (event.op, event.target.as_deref(), event.value.as_ref())
      {
         (TraceOperation::Orientation, Some("resize:orientation"), Some(TraceValue::Text(value))) if value == &next.orientation => self.orientation = value.clone(),
         (TraceOperation::Resize, Some("resize:viewport"), Some(TraceValue::Text(value))) if value == &format!("{}x{}", next.width, next.height) =>
         {
            self.width = next.width;
            self.height = next.height;
         }
         (TraceOperation::Theme, Some("resize:theme"), Some(TraceValue::Text(value))) if value == &next.theme =>
         {
            self.theme = value.clone();
            self.change_index += 1;
         }
         _ => return Err(format!("resize rejects {:?}", event.op)),
      }
      Ok(())
   }

   fn role_counts(&self) -> Vec<RoleCount>
   {
      counts(&[("dashboard", 1), ("label", 176), ("icon-image", 64), ("rounded-card", 32), ("control", 24), ("backdrop-region", 4)])
   }

   fn checkpoint_model(&self) -> serde_json::Value
   {
      serde_json::json!({
         "change_index": self.change_index,
         "height": self.height,
         "orientation": self.orientation,
         "theme": self.theme,
         "visible_node_count": 300,
         "width": self.width,
      })
   }

   fn checkpoint_json(&self, checkpoint_id: &str) -> Result<(Vec<u8>, Vec<u8>), String>
   {
      let roles = self.role_counts();
      let model = self.checkpoint_model();
      let state = serde_json::json!({
         "schema_version": 2,
         "scenario_id": "resize.theme",
         "checkpoint_id": checkpoint_id,
         "model": model,
         "visible_role_counts": roles,
      });
      let landscape = self.orientation == "landscape";
      let common = if landscape {[24, 36, 796, 318]} else {[16, 48, 358, 728]};
      let backdrop = if landscape {[20, 32, 804, 280]} else {[12, 42, 366, 586]};
      let nodes = roles.iter().enumerate().map(|(order, role)| serde_json::json!({
         "role": role.role,
         "name": role.role,
         "value": role.count.to_string(),
         "state": ["enabled", "visible"],
         "order": order,
         "focused": false,
         "actions": semantic_role_actions(&role.role),
         "frame": if role.role == "dashboard" {[0, 0, self.width as i32, self.height as i32]} else if role.role == "backdrop-region" {backdrop} else {common},
         "count": role.count,
         "visible": true,
      })).collect::<Vec<_>>();
      let accessibility = serde_json::json!({
         "schema_version": 2,
         "scenario_id": "resize.theme",
         "checkpoint_id": checkpoint_id,
         "root_frame": [0, 0, self.width, self.height],
         "raw_tree_source": "framework-neutral-release-contract",
         "nodes": nodes,
      });
      Ok((serde_json::to_vec(&state).map_err(|error| error.to_string())?, serde_json::to_vec(&accessibility).map_err(|error| error.to_string())?))
   }

   fn draw<U: elements::ImageUploader, G: GeometrySink>(&self, viewport: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder, geometry: &mut G)
   {
      let material = if self.theme == "dark" {RESIZE_DARK_MATERIAL} else {RESIZE_LIGHT_MATERIAL};
      self.dashboard.draw(viewport, scale, text, uploader, builder, geometry, "dashboard", "dashboard", Some(material));
   }
}

fn semantic_role_focused(scenario_id: &str, role: &str, model: &serde_json::Value) -> bool
{
   scenario_id == "chat.live-update" && role == "message" && !model["focused_message_id"].is_null()
}

fn semantic_role_actions(role: &str) -> Vec<&'static str>
{
   match role
   {
      "primary-control" | "control" | "favorite-control" | "send-control" | "list-item" | "dismiss-control" | "back-control" => vec!["activate"],
      "feed" | "chat-thread" => vec!["scroll"],
      "composer" => vec!["set-text"],
      "image-canvas" | "image" => vec!["pan", "zoom"],
      "thumbnail-grid" => vec!["scroll"],
      "grid-tile" => vec!["activate"],
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
      ("idle.steady", "dashboard") => [0, 0, 390, 844],
      ("idle.steady", "backdrop-region") => [12, 42, 366, 586],
      ("idle.steady", _) => [16, 48, 358, 728],
      ("feed.variable-scroll", "navigation-bar") => [0, 0, 390, 52],
      ("feed.variable-scroll", _) => [0, 52, 390, 792],
      ("chat.live-update", "chat-thread") | ("chat.live-update", "message") | ("chat.live-update", "avatar") => [0, 52, 390, 700],
      ("chat.live-update", "composer") => [12, 780, 318, 48],
      ("chat.live-update", "send-control") => [338, 780, 40, 48],
      ("navigation.modal", "navigation-list") | ("navigation.modal", "list-item") => [0, 0, 390, 844],
      ("navigation.modal", "detail") | ("navigation.modal", "back-control") => [0, 0, 390, 844],
      ("navigation.modal", "modal") | ("navigation.modal", "dismiss-control") => [24, 132, 342, 580],
      ("image.decode-zoom", "image-canvas") | ("image.decode-zoom", "image") => [0, 52, 390, 740],
      ("image.decode-zoom", "zoom-control") => [16, 800, 358, 28],
      ("grid.large-scroll", "thumbnail-grid") => [0, 48, 390, 748],
      ("grid.large-scroll", "grid-tile") => [12, 60, 366, 724],
      ("grid.large-scroll", "thumbnail") => [20, 68, 350, 570],
      ("grid.large-scroll", "tile-label") => [20, 646, 350, 130],
      ("grid.large-scroll", "detail-view") => [0, 0, 390, 844],
      ("grid.large-scroll", "detail-thumbnail") => [16, 84, 358, 358],
      ("grid.large-scroll", "detail-title") => [16, 466, 358, 52],
      ("grid.large-scroll", "back-control") => [16, 24, 44, 44],
      ("effects.layers", "effects-scene") | ("mutation.damage", "mutation-surface") | ("text.multilingual", "text-surface") => [0, 0, 390, 844],
      ("effects.layers", _) => [12, 48, 366, 748],
      ("mutation.damage", _) => [8, 40, 374, 796],
      ("text.multilingual", _) => [16, 40, 358, 788],
      _ => [0, 0, 390, 844],
   }
}

fn atlas_source(index: u32) -> gfx::RectF
{
   gfx::RectF::new(
      (index % ATLAS_COLUMNS) as f32 * ATLAS_TILE_SIZE,
      (index / ATLAS_COLUMNS) as f32 * ATLAS_TILE_SIZE,
      ATLAS_TILE_SIZE,
      ATLAS_TILE_SIZE,
   )
}

fn font_for_text(resources: &ComparisonResources, value: &str) -> usize
{
   let has_latin = value.chars().any(|value| value.is_ascii_alphabetic());
   let has_cjk = value.chars().any(|value| ('\u{4e00}'..='\u{9fff}').contains(&value));
   let has_arabic = value.chars().any(|value| ('\u{0600}'..='\u{06ff}').contains(&value));
   if has_cjk && !has_latin && !has_arabic
   {
      resources.cjk_font
   }
   else if has_arabic && !has_latin && !has_cjk
   {
      resources.arabic_font
   }
   else
   {
      resources.latin_font
   }
}

fn next_inline_text_asset<'a>(value: &str, entries: &'a [InlineTextAsset]) -> Option<(usize, &'a InlineTextAsset)>
{
   entries.iter().filter_map(|entry| value.find(&entry.grapheme).map(|index| (index, entry))).min_by_key(|(index, _)| *index)
}

fn inline_text_variant(font_px: f32, scale: f32, resources: &InlineTextResources) -> Option<(gfx::ImageHandle, u32)>
{
   let target = (font_px * scale).round().max(1.0) as u32;
   resources.atlas.variants.iter().zip(&resources.images)
      .min_by_key(|(variant, _)| variant.em_pixels.abs_diff(target))
      .map(|(variant, image)| (*image, variant.em_pixels))
}

fn snap_to_physical_pixel(value: f32, scale: f32) -> f32
{
   (value * scale).round() / scale
}

fn snap_image_sampling_origin(value: f32, scale: f32, image_scale: f32) -> f32
{
   let phase = (2.0 - image_scale).clamp(0.0, 1.0) * 0.25;
   ((value * scale - phase).ceil() + phase) / scale
}

fn centered_label_rect(font_id: usize, font_px: f32, rect: gfx::RectF, scale: f32, text: &elements::TextCtx) -> gfx::RectF
{
   let _ = (font_id, text);
   let baseline = rect.y + (rect.h - font_px * 1.362) * 0.5 + font_px * 1.069;
   gfx::RectF::new(rect.x, snap_to_physical_pixel(baseline, scale), rect.w, rect.h)
}

#[derive(Clone, Copy)]
struct ComparisonTextLines
{
   bounds: [gfx::RectF; 3],
   len: usize,
}

impl ComparisonTextLines
{
   fn as_slice(&self) -> &[gfx::RectF]
   {
      &self.bounds[..self.len]
   }
}

fn encode_comparison_label<U: elements::ImageUploader>(value: &str, color: gfx::Color, align: elements::Align, font_id: usize, font_px: f32, rect: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder) -> gfx::RectF
{
   let draw_rect = centered_label_rect(font_id, font_px, rect, scale, text);
   elements::encode_label_text(value, color, align, false, font_id, font_px, draw_rect, scale, text, uploader, builder);
   draw_rect
}

fn encode_clipped_label<U: elements::ImageUploader>(value: &str, color: gfx::Color, align: elements::Align, font_id: usize, font_px: f32, rect: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder) -> gfx::RectF
{
   builder.clip_push(gfx::RectI::new(rect.x.floor() as i32, rect.y.floor() as i32, rect.w.ceil() as i32, rect.h.ceil() as i32));
   let line = encode_comparison_label(value, color, align, font_id, font_px, rect, scale, text, uploader, builder);
   builder.clip_pop();
   line
}

fn measure_comparison_text(value: &str, font_id: usize, font_px: f32, text: &mut elements::TextCtx) -> f32
{
   let Some(font) = text.fonts.font(font_id) else {return 0.0};
   text.shaper.shape(font, font_id, value, font_px).map_or(0.0, |shape| shape.width())
}

fn encode_inline_text<U: elements::ImageUploader>(value: &str, color: gfx::Color, font_id: usize, font_px: f32, rect: gfx::RectF, scale: f32, resources: &ComparisonResources, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder) -> gfx::RectF
{
   builder.clip_push(gfx::RectI::new(rect.x.floor() as i32, rect.y.floor() as i32, rect.w.ceil() as i32, rect.h.ceil() as i32));
   let line = encode_inline_text_unclipped(value, color, font_id, font_px, rect, scale, resources, text, uploader, builder);
   builder.clip_pop();
   line
}

fn encode_inline_text_unclipped<U: elements::ImageUploader>(value: &str, color: gfx::Color, font_id: usize, font_px: f32, rect: gfx::RectF, scale: f32, resources: &ComparisonResources, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder) -> gfx::RectF
{
   let draw_rect = centered_label_rect(font_id, font_px, rect, scale, text);
   let Some(inline) = resources.inline_text.as_ref() else
   {
      elements::encode_label_text(value, color, elements::Align::Left, false, font_id, font_px, draw_rect, scale, text, uploader, builder);
      return draw_rect;
   };
   if next_inline_text_asset(value, &inline.atlas.entries).is_none()
   {
      elements::encode_label_text(value, color, elements::Align::Left, false, font_id, font_px, draw_rect, scale, text, uploader, builder);
      return draw_rect;
   }
   let mut remaining = value;
   let mut x = rect.x;
   let baseline = draw_rect.y;
   let Some((image, em_pixels)) = inline_text_variant(font_px, scale, inline) else
   {
      elements::encode_label_text(value, color, elements::Align::Left, false, font_id, font_px, draw_rect, scale, text, uploader, builder);
      return draw_rect;
   };
   while let Some((index, entry)) = next_inline_text_asset(remaining, &inline.atlas.entries)
   {
      let plain = &remaining[..index];
      if !plain.is_empty()
      {
         let width = measure_comparison_text(plain, font_id, font_px, text);
         elements::encode_label_text(plain, color, elements::Align::Left, false, font_id, font_px, gfx::RectF::new(x, draw_rect.y, width, rect.h), scale, text, uploader, builder);
         x += width;
      }
      let unit = font_px / 1_000_000.0;
      let width = entry.width_millionths as f32 * unit;
      let height = entry.height_millionths as f32 * unit;
      let top = baseline + entry.top_from_baseline_millionths as f32 * unit;
      let image_x = snap_to_physical_pixel(x, scale);
      let image_y = snap_to_physical_pixel(top, scale);
      builder.image(
         image,
         gfx::RectF::new(image_x, image_y, width, height),
         gfx::RectF::new((entry.column * em_pixels) as f32, (entry.row * em_pixels) as f32, em_pixels as f32, em_pixels as f32),
         1.0,
      );
      x += entry.advance_millionths as f32 * unit;
      remaining = &remaining[index + entry.grapheme.len()..];
   }
   if !remaining.is_empty()
   {
      elements::encode_label_text(remaining, color, elements::Align::Left, false, font_id, font_px, gfx::RectF::new(x, draw_rect.y, (rect.x + rect.w - x).max(0.0), rect.h), scale, text, uploader, builder);
   }
   draw_rect
}

fn measure_inline_text(value: &str, font_id: usize, font_px: f32, resources: &ComparisonResources, text: &mut elements::TextCtx) -> f32
{
   let measure = |value: &str, text: &mut elements::TextCtx|
   {
      text.shaper.shape_with_fallback_fonts(
         &text.fonts,
         font_id,
         &[resources.arabic_font, resources.cjk_font],
         value,
         font_px,
      ).map_or(0.0, |shape| shape.width())
   };
   let Some(inline) = resources.inline_text.as_ref() else {return measure(value, text)};
   let mut remaining = value;
   let mut width = 0.0;
   while let Some((index, entry)) = next_inline_text_asset(remaining, &inline.atlas.entries)
   {
      width += measure(&remaining[..index], text);
      width += entry.advance_millionths as f32 * font_px / 1_000_000.0;
      remaining = &remaining[index + entry.grapheme.len()..];
   }
   width + measure(remaining, text)
}

fn wrap_inline_text<'a>(value: &'a str, maximum_width: f32, maximum_lines: usize, font_id: usize, font_px: f32, resources: &ComparisonResources, text: &mut elements::TextCtx) -> ([&'a str; 3], usize)
{
   let maximum_lines = maximum_lines.clamp(1, 3);
   let split_words = value.chars().any(char::is_whitespace);
   let mut lines = [""; 3];
   let mut line_count = 0;
   let mut cursor = 0;
   let mut line_start = None;
   let mut line_end = 0;
   while cursor < value.len()
   {
      let (word_start, word_end) = if split_words
      {
         let Some(relative_start) = value[cursor..].find(|character: char| !character.is_whitespace()) else {break};
         let word_start = cursor + relative_start;
         let word_end = value[word_start..].find(char::is_whitespace).map(|index| word_start + index).unwrap_or(value.len());
         (word_start, word_end)
      }
      else
      {
         let word_start = cursor;
         let word_end = value[word_start..].char_indices().nth(1).map(|(index, _)| word_start + index).unwrap_or(value.len());
         (word_start, word_end)
      };
      let start = *line_start.get_or_insert(word_start);
      if line_end > start && measure_inline_text(&value[start..word_end], font_id, font_px, resources, text) > maximum_width
      {
         lines[line_count] = &value[start..line_end];
         line_count += 1;
         if line_count == maximum_lines
         {
            return (lines, line_count);
         }
         line_start = Some(word_start);
      }
      line_end = word_end;
      cursor = word_end;
   }
   if let Some(start) = line_start
   {
      if line_count < maximum_lines
      {
         lines[line_count] = &value[start..line_end];
         line_count += 1;
      }
   }
   (lines, line_count)
}

fn encode_wrapped_inline_text<U: elements::ImageUploader>(value: &str, color: gfx::Color, align: elements::Align, maximum_lines: usize, font_id: usize, font_px: f32, rect: gfx::RectF, scale: f32, resources: &ComparisonResources, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder) -> ComparisonTextLines
{
   builder.clip_push(gfx::RectI::new(rect.x.floor() as i32, rect.y.floor() as i32, rect.w.ceil() as i32, rect.h.ceil() as i32));
   let (lines, line_count) = wrap_inline_text(value, rect.w, maximum_lines, font_id, font_px, resources, text);
   let line_height = font_px * 61.0 / 45.0;
   let center_offset = line_height * line_count.saturating_sub(1) as f32 * 0.5;
   let mut bounds = [rect, rect, rect];
   let mut len = 0;
   for (index, line) in lines[..line_count].iter().copied().enumerate()
   {
      let line_rect = gfx::RectF::new(rect.x, rect.y - center_offset + index as f32 * line_height, rect.w, rect.h);
      match align
      {
         elements::Align::Left => {encode_inline_text_unclipped(line, color, font_id, font_px, line_rect, scale, resources, text, uploader, builder);}
         _ => {encode_comparison_label(line, color, align, font_id, font_px, line_rect, scale, text, uploader, builder);}
      }
      bounds[index] = line_rect;
      len += 1;
   }
   builder.clip_pop();
   ComparisonTextLines {bounds, len}
}

pub struct StartupScene
{
   cards: Vec<StartupCard>,
   data: String,
   header_id: String,
   navigation_id: String,
   control_id: String,
   foreground_count: u32,
   background_count: u32,
   fresh_install_ready: bool,
   lifecycle_state: Option<String>,
   resources: ComparisonResources,
}

impl StartupScene
{
   fn new(fixture: StartupFixture) -> Self
   {
      Self {
         cards: fixture.cards,
         data: fixture.data,
         header_id: fixture.header_id,
         navigation_id: fixture.navigation_id,
         control_id: fixture.control_id,
         foreground_count: 0,
         background_count: 0,
         fresh_install_ready: false,
         lifecycle_state: None,
         resources: ComparisonResources::default(),
      }
   }

   fn apply(&mut self, event: &TraceEvent) -> Result<(), String>
   {
      if !matches!(event.op, TraceOperation::Foreground | TraceOperation::Background | TraceOperation::ResourceArrival)
      {
         return Err(format!("startup rejects {:?}", event.op));
      }
      match event.op
      {
         TraceOperation::Foreground => self.foreground_count = self.foreground_count.saturating_add(1),
         TraceOperation::Background => self.background_count = self.background_count.saturating_add(1),
         TraceOperation::ResourceArrival => self.fresh_install_ready = true,
         _ => {}
      }
      self.lifecycle_state = event.state_id.clone();
      Ok(())
   }

   fn checkpoint_model(&self) -> serde_json::Value
   {
      serde_json::json!({
         "foreground_count": self.foreground_count,
         "background_count": self.background_count,
         "fresh_install_ready": self.fresh_install_ready,
         "scene_visible": self.background_count < self.foreground_count,
         "lifecycle_state_id": self.lifecycle_state,
         "card_count": self.cards.len(),
      })
   }

   fn role_counts(&self) -> Vec<RoleCount>
   {
      counts(&[("header", 1), ("navigation", 1), ("card", 6), ("initial-image", 6), ("primary-control", 1)])
   }

   fn draw<U: elements::ImageUploader, G: GeometrySink>(&self, viewport: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder, geometry: &mut G)
   {
      encode_solid_rects(builder, [viewport], BACKGROUND);
      let header = gfx::RectF::new(viewport.x + 16.0, viewport.y + 20.0, 358.0, 48.0);
      let header_line = encode_comparison_label("Production Comparison", TEXT, elements::Align::Left, self.resources.latin_font, 20.0, header, scale, text, uploader, builder);
      geometry.record("header", &self.header_id, header, &[header_line]);
      let navigation = gfx::RectF::new(viewport.x + 16.0, viewport.y + 76.0, 358.0, 44.0);
      builder.rrect(navigation, [12.0; 4], SURFACE);
      let navigation_line = encode_comparison_label("First Screen", SECONDARY, elements::Align::Left, self.resources.latin_font, 15.0, navigation, scale, text, uploader, builder);
      geometry.record("navigation", &self.navigation_id, navigation, &[navigation_line]);

      builder.clip_push(gfx::RectI::new((viewport.x + 16.0) as i32, (viewport.y + 132.0) as i32, 358, 552));
      for (visible_index, card) in self.cards.iter().filter(|card| card.initially_visible).take(6).enumerate()
      {
         let column = visible_index % 2;
         let row = visible_index / 2;
         let card_w = (viewport.w - 44.0) * 0.5;
         let card_rect = gfx::RectF::new(viewport.x + 16.0 + column as f32 * (card_w + 12.0), viewport.y + 132.0 + row as f32 * 188.0, card_w, 176.0);
         builder.rrect(gfx::RectF::new(card_rect.x, card_rect.y + 2.0, card_rect.w, card_rect.h), [12.0; 4], SHADOW);
         builder.rrect(card_rect, [12.0; 4], SURFACE);
         encode_rounded_atlas_image(builder, self.resources.thumbnail_atlas, gfx::RectF::new(card_rect.x + 12.0, card_rect.y + 12.0, 48.0, 48.0), atlas_source(card.thumbnail_index), 8.0);
         let title_line = encode_clipped_label(&card.id, TEXT, elements::Align::Left, self.resources.latin_font, 15.0, gfx::RectF::new(card_rect.x + 72.0, card_rect.y + 14.0, card_rect.w - 84.0, 22.0), scale, text, uploader, builder);
         let detail_start = card.data_offset as usize;
         let detail_end = detail_start.saturating_add(48).min(self.data.len());
         let detail = self.data.get(detail_start..detail_end).unwrap_or("");
         let detail_line = encode_clipped_label(detail, SECONDARY, elements::Align::Left, self.resources.latin_font, 11.0, gfx::RectF::new(card_rect.x + 72.0, card_rect.y + 42.0, card_rect.w - 84.0, 54.0), scale, text, uploader, builder);
         geometry.record("card", &card.id, card_rect, &[title_line, detail_line]);
         geometry.record_suffix("initial-image", &card.id, ":thumbnail", gfx::RectF::new(card_rect.x + 12.0, card_rect.y + 12.0, 48.0, 48.0), &[]);
      }
      builder.clip_pop();

      let control = gfx::RectF::new(viewport.x + 16.0, viewport.y + 776.0, 358.0, 48.0);
      builder.rrect(control, [0.0; 4], ACCENT);
      let control_line = encode_comparison_label("Continue", SURFACE, elements::Align::Center, self.resources.latin_font, 15.0, gfx::RectF::new(control.x - 1.0 / 3.0, control.y + 1.0 / 3.0, control.w, control.h), scale, text, uploader, builder);
      geometry.record("primary-control", &self.control_id, control, &[control_line]);
      let _ = (&self.header_id, &self.navigation_id, &self.control_id, &self.lifecycle_state);
   }
}

pub struct DashboardScene
{
   labels: Vec<String>,
   card_count: u32,
   icon_count: u32,
   control_count: u32,
   backdrop_count: u32,
   leaf_updates: u32,
   bulk_updates: u32,
   resources: ComparisonResources,
}

impl DashboardScene
{
   fn new(fixture: DashboardFixture) -> Self
   {
      Self::new_with_counts(
         fixture.categories.label,
         fixture.categories.rounded_card,
         fixture.categories.icon_image,
         fixture.categories.control,
         fixture.categories.backdrop_region,
      )
   }

   fn new_with_counts(label_count: u32, card_count: u32, icon_count: u32, control_count: u32, backdrop_count: u32) -> Self
   {
      let mut labels = Vec::with_capacity(label_count as usize);
      for card_index in 0..card_count
      {
         let label_count = if card_index < 16 {6} else {5};
         for label_index in 0..label_count
         {
            labels.push(format!("Node {}", card_index * 6 + label_index));
         }
      }
      Self {
         labels,
         card_count,
         icon_count,
         control_count,
         backdrop_count,
         leaf_updates: 0,
         bulk_updates: 0,
         resources: ComparisonResources::default(),
      }
   }

   fn set_tab(&mut self, tab_index: u32)
   {
      for (index, label) in self.labels.iter_mut().enumerate()
      {
         let node = if index < 96 {index} else {16 * 6 + (index - 96) / 5 * 6 + (index - 96) % 5};
         *label = if tab_index == 0 {format!("Node {node}")} else {format!("Tab Node {node}")};
      }
   }

   fn apply(&mut self, event: &TraceEvent) -> Result<(), String>
   {
      if event.op != TraceOperation::Mutate
      {
         return Err(format!("dashboard rejects {:?}", event.op));
      }
      let target = event.target.as_deref().ok_or_else(|| String::from("dashboard mutation has no target"))?;
      if target == "dashboard:update-count"
      {
         self.bulk_updates = match event.value {Some(TraceValue::Integer(value)) => value.max(0) as u32, _ => 0};
      }
      else if let Some(index) = target.strip_prefix("dashboard:label:").and_then(|value| value.parse::<usize>().ok()).filter(|index| *index < self.labels.len())
      {
         self.leaf_updates = self.leaf_updates.saturating_add(1);
         self.labels[index] = format!("Updated {}", self.leaf_updates);
      }
      Ok(())
   }

   fn role_counts(&self) -> Vec<RoleCount>
   {
      counts(&[
         ("dashboard", 1),
         ("label", self.labels.len() as u32),
         ("icon-image", self.icon_count),
         ("rounded-card", self.card_count),
         ("control", self.control_count),
         ("backdrop-region", self.backdrop_count),
      ])
   }

   fn checkpoint_model(&self) -> serde_json::Value
   {
      serde_json::json!({
         "leaf_update_count": self.leaf_updates,
         "bulk_update_count": self.bulk_updates,
         "visible_node_count": 1 + self.labels.len() as u32 + self.icon_count + self.card_count + self.control_count + self.backdrop_count,
      })
   }

   fn draw<U: elements::ImageUploader, G: GeometrySink>(&self, viewport: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder, geometry: &mut G, root_role: &str, identifier_prefix: &str, backdrop_material: Option<gfx::Color>)
   {
      encode_solid_rects(builder, [viewport], BACKGROUND);
      geometry.record(root_role, identifier_prefix, viewport, &[]);
      let landscape = viewport.w > viewport.h;
      let backdrop_prefix = if identifier_prefix == "endurance" {"endurance:backdrop:"} else {"dashboard:backdrop:"};
      let card_prefix = if identifier_prefix == "endurance" {"endurance:card:"} else {"dashboard:card:"};
      let label_prefix = if identifier_prefix == "endurance" {"endurance:label:"} else {"dashboard:label:"};
      let icon_prefix = if identifier_prefix == "endurance" {"endurance:icon:"} else {"dashboard:icon:"};
      let control_prefix = if identifier_prefix == "endurance" {"endurance:control:"} else {"dashboard:control:"};
      let card_bounds = |index: usize|
      {
         let columns = if landscape {4} else {2};
         let column = index % columns;
         let row = index / columns;
         let card_w = if landscape {190.0} else {(viewport.w - 44.0) * 0.5};
         if landscape
         {
            gfx::RectF::new(viewport.x + 24.0 + column as f32 * 202.0, viewport.y + 36.0 + row as f32 * 40.0, card_w, 38.0)
         }
         else
         {
            gfx::RectF::new(viewport.x + 16.0 + column as f32 * (card_w + 12.0), viewport.y + 48.0 + row as f32 * 46.0, card_w, 38.0)
         }
      };
      for index in 0..self.backdrop_count
      {
         let backdrop = if landscape
         {
            gfx::RectF::new(viewport.x + 24.0 + (index % 2) as f32 * 398.0, viewport.y + 48.0 + (index / 2) as f32 * 144.0, 386.0, 120.0)
         }
         else
         {
            gfx::RectF::new(viewport.x + 16.0, viewport.y + 64.0 + index as f32 * 204.0, viewport.w - 32.0, 96.0)
         };
         builder.backdrop(
            backdrop,
            12.0,
            backdrop_material.unwrap_or(DASHBOARD_MATERIAL),
            if backdrop_material.is_some() {1.0} else {56.0 / 255.0},
         );
         geometry.record_indexed("backdrop-region", backdrop_prefix, index as usize, 1, backdrop, &[]);
      }
      for index in 0..self.card_count as usize
      {
         let card = card_bounds(index);
         builder.rrect(gfx::RectF::new(card.x, card.y + 2.0, card.w, card.h), [12.0; 4], SHADOW);
         builder.rrect(card, [12.0; 4], SURFACE);
         builder.image(self.resources.thumbnail_atlas, gfx::RectF::new(card.x + 6.0, card.y + 6.0, 14.0, 14.0), atlas_source((index * 2) as u32), 1.0);
         builder.image(self.resources.thumbnail_atlas, gfx::RectF::new(card.x + 24.0, card.y + 6.0, 14.0, 14.0), atlas_source((index * 2 + 1) as u32), 1.0);
         geometry.record_indexed("rounded-card", card_prefix, index, 2, card, &[]);
         geometry.record_indexed("icon-image", icon_prefix, index * 2, 3, gfx::RectF::new(card.x + 6.0, card.y + 6.0, 14.0, 14.0), &[]);
         geometry.record_indexed("icon-image", icon_prefix, index * 2 + 1, 3, gfx::RectF::new(card.x + 24.0, card.y + 6.0, 14.0, 14.0), &[]);
         let label_count = if index < 16 {6} else {5};
         let label_base = if index < 16 {index * 6} else {96 + (index - 16) * 5};
         for label_index in 0..label_count
         {
            let label = &self.labels[label_base + label_index];
            let label_rect = gfx::RectF::new(card.x + 42.0, card.y + 2.0 - 8.0 / 3.0 + label_index as f32 * 6.0, 125.0, 11.0);
            let label_line = encode_comparison_label(
               label,
               if self.bulk_updates > 0 && label_base + label_index < self.bulk_updates as usize {ACCENT} else if label_index == 0 {TEXT} else {SECONDARY},
               elements::Align::Left,
               font_for_text(&self.resources, label),
               7.0,
               label_rect,
               scale,
               text,
               uploader,
               builder,
            );
            geometry.record_indexed("label", label_prefix, label_base + label_index, 3, label_rect, &[label_line]);
         }
      }
      encode_solid_rects(builder, (0..self.control_count as usize).map(|index| {
         let card = card_bounds(index);
         let control = gfx::RectF::new(card.x + 145.0, card.y + 13.0, 18.0, 12.0);
         geometry.record_indexed("control", control_prefix, index, 2, control, &[]);
         control
      }), ACCENT);
   }
}

pub struct EnduranceScene
{
   fixture: EnduranceFixture,
   dashboard: Option<DashboardScene>,
   resources: ComparisonResources,
   heavy_screen_visible: bool,
   heavy_screen_transitions: u32,
   open_close_cycles: u32,
   active_tab_index: u32,
   tab_switches: u32,
   animation_frame_index: u32,
   animation_frames: u32,
}

impl EnduranceScene
{
   fn new(fixture: EnduranceFixture) -> Self
   {
      let resources = ComparisonResources::default();
      let dashboard = Some(Self::make_dashboard(&fixture, Self::dashboard_resources(&resources)));
      let active_tab_index = fixture.initial_tab_index;
      Self {
         fixture,
         dashboard,
         resources,
         heavy_screen_visible: true,
         heavy_screen_transitions: 0,
         open_close_cycles: 0,
         active_tab_index,
         tab_switches: 0,
         animation_frame_index: 0,
         animation_frames: 0,
      }
   }

   fn make_dashboard(fixture: &EnduranceFixture, resources: ComparisonResources) -> DashboardScene
   {
      let mut dashboard = DashboardScene::new_with_counts(
         fixture.categories.label,
         fixture.categories.rounded_card,
         fixture.categories.icon_image,
         fixture.categories.control,
         fixture.categories.backdrop_region,
      );
      dashboard.resources = resources;
      dashboard
   }

   fn dashboard_resources(resources: &ComparisonResources) -> ComparisonResources
   {
      ComparisonResources {
         thumbnail_atlas: resources.thumbnail_atlas,
         accented_thumbnail_atlas: resources.accented_thumbnail_atlas,
         source_image: resources.source_image,
         thumbnail_image: resources.thumbnail_image,
         latin_font: resources.latin_font,
         arabic_font: resources.arabic_font,
         cjk_font: resources.cjk_font,
         inline_text: None,
      }
   }

   fn set_resources(&mut self, thumbnail_atlas: gfx::ImageHandle, font_ids: [usize; 3])
   {
      self.resources.set_shared(thumbnail_atlas, font_ids);
      if let Some(dashboard) = &mut self.dashboard
      {
         dashboard.resources = Self::dashboard_resources(&self.resources);
      }
   }

   fn set_fonts(&mut self, font_ids: [usize; 3])
   {
      self.resources.set_fonts(font_ids);
      if let Some(dashboard) = &mut self.dashboard
      {
         dashboard.resources.set_fonts(font_ids);
      }
   }

   fn set_inline_text_resources(&mut self, images: Vec<gfx::ImageHandle>, atlas: InlineTextAtlas) -> Result<(), String>
   {
      self.resources.set_inline_text(images, atlas)?;
      if let Some(dashboard) = &mut self.dashboard
      {
         dashboard.resources = Self::dashboard_resources(&self.resources);
      }
      Ok(())
   }

   fn apply(&mut self, event: &TraceEvent) -> Result<(), String>
   {
      if event.op != TraceOperation::Mutate
      {
         return Err(format!("endurance rejects {:?}", event.op));
      }
      let target = event.target.as_deref().ok_or_else(|| String::from("endurance mutation has no target"))?;
      if target == self.fixture.heavy_screen_target_id
      {
         let Some(TraceValue::Boolean(visible)) = event.value else {return Err(String::from("endurance heavy-screen mutation is not boolean"))};
         if visible == self.heavy_screen_visible || self.heavy_screen_transitions >= self.fixture.heavy_screen_cycle_count * 2
         {
            return Err(String::from("endurance heavy-screen transition differs from the frozen sequence"));
         }
         self.heavy_screen_visible = visible;
         self.heavy_screen_transitions += 1;
         if visible
         {
            self.open_close_cycles += 1;
            let mut dashboard = Self::make_dashboard(&self.fixture, Self::dashboard_resources(&self.resources));
            dashboard.set_tab(self.active_tab_index);
            dashboard.bulk_updates = self.animation_frame_index % 30;
            self.dashboard = Some(dashboard);
         }
         else
         {
            self.dashboard = None;
         }
      }
      else if target == self.fixture.active_tab_target_id
      {
         let Some(TraceValue::Integer(index)) = event.value else {return Err(String::from("endurance tab mutation is not integer"))};
         let index = u32::try_from(index).map_err(|_| String::from("endurance tab index is negative"))?;
         if index >= self.fixture.tab_count || index == self.active_tab_index || self.tab_switches >= self.fixture.tab_switch_count
         {
            return Err(String::from("endurance tab transition differs from the frozen sequence"));
         }
         self.active_tab_index = index;
         self.tab_switches += 1;
         if let Some(dashboard) = &mut self.dashboard
         {
            dashboard.set_tab(index);
         }
      }
      else if target == self.fixture.animation_frame_target_id
      {
         let Some(TraceValue::Integer(frame)) = event.value else {return Err(String::from("endurance animation mutation is not integer"))};
         let frame = u32::try_from(frame).map_err(|_| String::from("endurance animation frame is negative"))?;
         if frame != self.animation_frame_index + 1 || frame > self.fixture.animation_frame_count
         {
            return Err(String::from("endurance animation frame differs from the frozen sequence"));
         }
         self.animation_frame_index = frame;
         self.animation_frames += 1;
         if let Some(dashboard) = &mut self.dashboard
         {
            dashboard.bulk_updates = frame % 30;
         }
      }
      else
      {
         return Err(format!("endurance rejects target {target}"));
      }
      Ok(())
   }

   fn role_counts(&self) -> Vec<RoleCount>
   {
      if !self.heavy_screen_visible
      {
         return counts(&[("endurance", 1), ("label", 0), ("icon-image", 0), ("rounded-card", 0), ("control", 0), ("backdrop-region", 0)]);
      }
      counts(&[
         ("endurance", 1),
         ("label", self.fixture.categories.label),
         ("icon-image", self.fixture.categories.icon_image),
         ("rounded-card", self.fixture.categories.rounded_card),
         ("control", self.fixture.categories.control),
         ("backdrop-region", self.fixture.categories.backdrop_region),
      ])
   }

   fn checkpoint_model(&self) -> serde_json::Value
   {
      serde_json::json!({
         "open_close_cycle_count": self.open_close_cycles,
         "heavy_screen_transition_count": self.heavy_screen_transitions,
         "heavy_screen_visible": self.heavy_screen_visible,
         "tab_switch_count": self.tab_switches,
         "active_tab_index": self.active_tab_index,
         "animation_frame_count": self.animation_frames,
         "animation_frame_index": self.animation_frame_index,
         "visible_node_count": if self.heavy_screen_visible {1 + self.fixture.visible_node_count} else {1},
      })
   }

   fn draw<U: elements::ImageUploader, G: GeometrySink>(&mut self, viewport: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder, geometry: &mut G)
   {
      if let Some(dashboard) = &self.dashboard
      {
         dashboard.draw(viewport, scale, text, uploader, builder, geometry, "endurance", "endurance", None);
      }
      else
      {
         encode_solid_rects(builder, [viewport], BACKGROUND);
         geometry.record("endurance", "endurance", viewport, &[]);
      }
   }
}

pub struct FeedScene
{
   rows: Vec<FeedRow>,
   offsets: Vec<f32>,
   content_height: f32,
   scroll: f32,
   trace_scroll_millionths: u32,
   favorite: Option<String>,
   prepend_count: usize,
   resources: ComparisonResources,
}

impl FeedScene
{
   fn new(fixture: FeedFixture) -> Self
   {
      let mut scene = Self {
         rows: fixture.rows,
         offsets: Vec::with_capacity(fixture.row_count as usize + fixture.prepend_rows.len() + 1),
         content_height: 0.0,
         scroll: 0.0,
         trace_scroll_millionths: 0,
         favorite: None,
         prepend_count: 0,
         resources: ComparisonResources::default(),
      };
      scene.rebuild_offsets();
      scene
   }

   fn rebuild_offsets(&mut self)
   {
      self.offsets.clear();
      self.offsets.push(0.0);
      let mut offset = 0.0;
      for row in &self.rows
      {
         offset += row.height as f32;
         self.offsets.push(offset);
      }
      self.content_height = offset;
   }

   fn prepend(&mut self)
   {
      if self.prepend_count != 0
      {
         return;
      }
      let mut prepended = Vec::with_capacity(20);
      for index in 0..20
      {
         prepended.push(FeedRow {
            id: format!("feed:prepend:{index:02}"),
            height: 76,
            text: format!("Prepended {index}"),
            thumbnail_index: index,
            favorite: false,
            direction: String::from("ltr"),
         });
      }
      prepended.append(&mut self.rows);
      self.rows = prepended;
      self.prepend_count = 20;
      self.scroll += 20.0 * 76.0;
      self.rebuild_offsets();
   }

   fn apply(&mut self, event: &TraceEvent) -> Result<(), String>
   {
      match event.op
      {
         TraceOperation::PointerDown => {}
         TraceOperation::PointerMove | TraceOperation::PointerUp =>
         {
            if let Some(y) = event.y_millionths
            {
               self.trace_scroll_millionths = (1_000_000 - y).max(0) as u32;
               self.scroll = ((1.0 - y as f32 / 1_000_000.0) * self.max_scroll()).clamp(0.0, self.max_scroll());
            }
         }
         TraceOperation::Mutate if event.target.as_deref() == Some("feed:prepend-count") => self.prepend(),
         TraceOperation::Mutate =>
         {
            self.favorite = event.target.as_deref().and_then(|target| target.strip_suffix(":favorite")).map(String::from);
         }
         _ => return Err(format!("feed rejects {:?}", event.op)),
      }
      Ok(())
   }

   fn input_pointer(&mut self, _x: f32, _y: f32, dy: f32, buttons: u32)
   {
      if buttons & 1 != 0
      {
         self.scroll = (self.scroll - dy).clamp(0.0, self.max_scroll());
      }
   }

   fn host_pointer_delta(&mut self, dy: f32) -> bool
   {
      let previous = self.scroll;
      self.input_pointer(0.0, 0.0, dy, 1);
      if self.scroll == previous
      {
         return false;
      }
      let maximum = self.max_scroll();
      self.trace_scroll_millionths = if maximum > 0.0 {(self.scroll / maximum * 1_000_000.0).round() as u32} else {0};
      true
   }

   fn host_click(&mut self, x: f32, y: f32) -> bool
   {
      for index in self.visible_range(VIEWPORT_HEIGHT)
      {
         let row = &self.rows[index];
         let card_y = FEED_HEADER_HEIGHT + self.offsets[index] - self.scroll + 4.0;
         let favorite = gfx::RectF::new(332.0, card_y + 11.0, 18.0, 18.0);
         if point_in_rect(x, y, favorite) && self.favorite.as_deref() != Some(row.id.as_str())
         {
            self.favorite = Some(row.id.clone());
            return true;
         }
      }
      false
   }

   fn max_scroll(&self) -> f32
   {
      (self.content_height - (VIEWPORT_HEIGHT - FEED_HEADER_HEIGHT)).max(0.0)
   }

   fn visible_range(&self, viewport_height: f32) -> core::ops::Range<usize>
   {
      let start = self.offsets.partition_point(|offset| *offset < self.scroll).saturating_sub(1).min(self.rows.len());
      let bottom = self.scroll + (viewport_height - FEED_HEADER_HEIGHT).max(0.0);
      let mut end = start;
      while end < self.rows.len() && self.offsets[end] <= bottom
      {
         end += 1;
      }
      start..end
   }

   fn role_counts(&self) -> Vec<RoleCount>
   {
      let visible = self.visible_range(VIEWPORT_HEIGHT).len() as u32;
      counts(&[
         ("navigation-bar", 1),
         ("feed", 1),
         ("feed-card", visible),
         ("thumbnail", visible),
         ("favorite-control", visible),
      ])
   }

   fn checkpoint_model(&self) -> serde_json::Value
   {
      serde_json::json!({
         "row_count": self.rows.len(),
         "scroll_position_millionths": self.trace_scroll_millionths,
         "favorite_id": self.favorite,
         "prepend_count": self.prepend_count,
      })
   }

   fn draw<U: elements::ImageUploader, G: GeometrySink>(&self, viewport: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder, geometry: &mut G)
   {
      encode_solid_rects(builder, [viewport], BACKGROUND);
      let navigation = gfx::RectF::new(viewport.x, viewport.y, viewport.w, 52.0);
      builder.rrect(navigation, [0.0; 4], SURFACE);
      let navigation_line = encode_comparison_label("Measured Feed", TEXT, elements::Align::Left, self.resources.latin_font, 20.0, gfx::RectF::new(viewport.x + 16.0, viewport.y + 12.0, 220.0, 28.0), scale, text, uploader, builder);
      geometry.record("navigation-bar", "feed.navigation-bar", navigation, &[navigation_line]);
      geometry.record("feed", "feed.table", gfx::RectF::new(viewport.x, viewport.y + FEED_HEADER_HEIGHT, viewport.w, viewport.h - FEED_HEADER_HEIGHT), &[]);
      builder.clip_push(gfx::RectI::new(viewport.x.floor() as i32, (viewport.y + FEED_HEADER_HEIGHT).floor() as i32, viewport.w.ceil() as i32, (viewport.h - FEED_HEADER_HEIGHT).max(0.0).ceil() as i32));
      for index in self.visible_range(viewport.h)
      {
         let row = &self.rows[index];
         let y = viewport.y + FEED_HEADER_HEIGHT + self.offsets[index] - self.scroll;
         let height = row.height as f32;
         let card = gfx::RectF::new(viewport.x + 12.0, snap_to_physical_pixel(y + 4.0, scale), viewport.w - 24.0, height - 8.0);
         builder.rrect(gfx::RectF::new(card.x, card.y + 2.0, card.w, card.h), [12.0; 4], SHADOW);
         builder.rrect(card, [12.0; 4], SURFACE);
         encode_rounded_atlas_image(builder, self.resources.thumbnail_atlas, gfx::RectF::new(card.x + 10.0, card.y + 10.0, 48.0, 48.0), atlas_source(row.thumbnail_index), 8.0);
         let title_line = gfx::RectF::new(card.x + 70.0, card.y + 8.0, card.w - 112.0, 24.0);
         let secondary_line = gfx::RectF::new(card.x + 70.0, card.y + 34.0, card.w - 112.0, 18.0);
         encode_inline_text(&row.text, TEXT, font_for_text(&self.resources, &row.text), 15.0, title_line, scale, &self.resources, text, uploader, builder);
         encode_comparison_label(&row.id, SECONDARY, elements::Align::Left, self.resources.latin_font, 11.0, secondary_line, scale, text, uploader, builder);
         let favorite = gfx::RectF::new(card.x + card.w - 34.0, card.y + 11.0, 18.0, 18.0);
         builder.rrect(
            favorite,
            [9.0; 4],
            if self.favorite.as_deref() == Some(row.id.as_str()) {ACCENT} else {INACTIVE_CONTROL},
         );
         geometry.record("feed-card", &row.id, card, &[title_line, secondary_line]);
         geometry.record_suffix("thumbnail", &row.id, ":thumbnail", gfx::RectF::new(card.x + 10.0, card.y + 10.0, 48.0, 48.0), &[]);
         geometry.record_suffix("favorite-control", &row.id, ":favorite", favorite, &[]);
      }
      builder.clip_pop();
   }
}

pub struct ChatScene
{
   messages: Vec<ChatMessage>,
   prepend_messages: Vec<ChatMessage>,
   append_templates: Vec<ChatMessage>,
   appended_messages: Vec<ChatMessage>,
   offsets: Vec<f32>,
   content_height: f32,
   scroll: f32,
   prepend_active: bool,
   composer: String,
   selection_message_id: String,
   focused_message_id: Option<String>,
   selection: Option<(usize, usize)>,
   selection_caret: Option<usize>,
   replacement_applied: bool,
   resources: ComparisonResources,
}

impl ChatScene
{
   fn new(fixture: ChatFixture) -> Self
   {
      let append_templates = fixture.messages.iter().take(8).cloned().collect();
      let mut scene = Self {
         messages: fixture.messages,
         prepend_messages: fixture.prepend_messages,
         append_templates,
         appended_messages: Vec::with_capacity(20),
         offsets: Vec::with_capacity(fixture.message_count as usize + 50 + 20 + 1),
         content_height: 0.0,
         scroll: 0.0,
         prepend_active: false,
         composer: String::with_capacity(fixture.typed_text.len() + fixture.pasted_text.len()),
         selection_message_id: fixture.selection_replacement.message_id,
         focused_message_id: None,
         selection: None,
         selection_caret: None,
         replacement_applied: false,
         resources: ComparisonResources::default(),
      };
      scene.rebuild_offsets();
      scene.scroll = scene.max_scroll();
      scene
   }

   fn rebuild_offsets(&mut self)
   {
      self.offsets.clear();
      self.offsets.push(0.0);
      let mut offset = 0.0;
      if self.prepend_active
      {
         for message in &self.prepend_messages
         {
            offset += chat_row_height(message);
            self.offsets.push(offset);
         }
      }
      for message in self.messages.iter().chain(&self.appended_messages)
      {
         offset += chat_row_height(message);
         self.offsets.push(offset);
      }
      self.content_height = offset;
   }

   fn message_count(&self) -> usize
   {
      self.messages.len() + self.appended_messages.len() + if self.prepend_active {self.prepend_messages.len()} else {0}
   }

   fn message(&self, index: usize) -> &ChatMessage
   {
      if self.prepend_active && index < self.prepend_messages.len()
      {
         &self.prepend_messages[index]
      }
      else
      {
         let base_index = index - if self.prepend_active {self.prepend_messages.len()} else {0};
         if base_index < self.messages.len() {&self.messages[base_index]} else {&self.appended_messages[base_index - self.messages.len()]}
      }
   }

   fn selection_text(&self) -> Option<&str>
   {
      self.messages.iter().chain(&self.appended_messages).find(|message| message.id == self.selection_message_id).map(|message| message.text.as_str())
   }

   fn prepend(&mut self)
   {
      if self.prepend_active
      {
         return;
      }
      let added_height = self.prepend_messages.iter().map(chat_row_height).sum::<f32>();
      self.prepend_active = true;
      self.rebuild_offsets();
      self.scroll = (self.scroll + added_height).clamp(0.0, self.max_scroll());
   }

   fn append(&mut self, id: &str)
   {
      if self.appended_messages.iter().any(|message| message.id == id)
      {
         return;
      }
      let at_bottom = self.scroll >= self.max_scroll() - 0.5;
      let index = self.appended_messages.len() as u32;
      let template = &self.append_templates[index as usize % self.append_templates.len()];
      let message = ChatMessage {
         id: String::from(id),
         sequence: 5_050 + index,
         author_index: index % 64,
         avatar_index: index % 64,
         direction: template.direction.clone(),
         text: template.text.clone(),
      };
      self.content_height += chat_row_height(&message);
      self.offsets.push(self.content_height);
      self.appended_messages.push(message);
      if at_bottom
      {
         self.scroll = self.max_scroll();
      }
   }

   fn apply(&mut self, event: &TraceEvent) -> Result<(), String>
   {
      match event.op
      {
         TraceOperation::Mutate if event.target.as_deref() == Some("chat:prepend-count") => self.prepend(),
         TraceOperation::Mutate if event.target.as_deref() == Some("chat:append") =>
         {
            let Some(TraceValue::Text(id)) = event.value.as_ref() else {return Err(String::from("chat append has no message id"))};
            self.append(id);
         }
         TraceOperation::Focus if event.target.as_deref() == Some(self.selection_message_id.as_str()) =>
         {
            let Some(TraceValue::Text(range)) = event.value.as_ref() else {return Err(String::from("chat selection has no range"))};
            let Some((start, end)) = range.split_once(':').and_then(|(start, end)| Some((start.parse::<usize>().ok()?, end.parse::<usize>().ok()?))) else {return Err(String::from("chat selection range is invalid"))};
            self.focused_message_id = event.target.clone();
            self.selection = Some((start, end));
            self.selection_caret = Some(end);
         }
         TraceOperation::CommitText if event.target.as_deref() == Some("chat:composer") =>
         {
            let Some(TraceValue::Text(value)) = event.value.as_ref() else {return Err(String::from("chat composer event has no text"))};
            self.composer.push_str(value);
         }
         TraceOperation::CommitText if event.target.as_deref() == Some(self.selection_message_id.as_str()) =>
         {
            let Some(TraceValue::Text(value)) = event.value.as_ref() else {return Err(String::from("chat replacement has no text"))};
            let Some((start, end)) = self.selection.take() else {return Err(String::from("chat replacement has no selection"))};
            let at_bottom = self.scroll >= self.max_scroll() - 0.5;
            let selection_message_id = self.selection_message_id.as_str();
            let Some(message) = self.messages.iter_mut().chain(&mut self.appended_messages).find(|message| message.id == selection_message_id) else {return Err(String::from("chat replacement target is absent"))};
            if start > end || end > message.text.len() || !message.text.is_char_boundary(start) || !message.text.is_char_boundary(end)
            {
               return Err(String::from("chat replacement range is outside UTF-8 boundaries"));
            }
            message.text.replace_range(start..end, value);
            self.selection_caret = Some(start + value.len());
            self.replacement_applied = true;
            self.rebuild_offsets();
            self.scroll = if at_bottom {self.max_scroll()} else {self.scroll.clamp(0.0, self.max_scroll())};
         }
         _ => return Err(format!("chat rejects {:?}", event.op)),
      }
      Ok(())
   }

   fn host_text(&mut self, value: &str) -> bool
   {
      if value.is_empty()
      {
         return false;
      }
      if self.focused_message_id.as_deref() == Some(self.selection_message_id.as_str())
      {
         let at_bottom = self.scroll >= self.max_scroll() - 0.5;
         let selection_message_id = self.selection_message_id.as_str();
         let Some(message) = self.messages.iter_mut().chain(&mut self.appended_messages).find(|message| message.id == selection_message_id) else {return false};
         let caret = self.selection_caret.unwrap_or(message.text.len()).min(message.text.len());
         let (start, end) = self.selection.take().unwrap_or((caret, caret));
         if start > end || end > message.text.len() || !message.text.is_char_boundary(start) || !message.text.is_char_boundary(end)
         {
            return false;
         }
         message.text.replace_range(start..end, value);
         self.selection_caret = Some(start + value.len());
         self.replacement_applied |= start != end;
         self.rebuild_offsets();
         self.scroll = if at_bottom {self.max_scroll()} else {self.scroll.clamp(0.0, self.max_scroll())};
         return true;
      }
      self.composer.push_str(value);
      true
   }

   fn host_click(&mut self, x: f32, y: f32) -> bool
   {
      let composer = gfx::RectF::new(12.0, 780.0, 318.0, 48.0);
      if point_in_rect(x, y, composer)
      {
         let changed = self.focused_message_id.take().is_some() || self.selection.take().is_some() || self.selection_caret.take().is_some();
         return changed;
      }
      for index in self.visible_range(VIEWPORT_HEIGHT)
      {
         let message = self.message(index);
         if message.id != self.selection_message_id
         {
            continue;
         }
         let row_y = CHAT_HEADER_HEIGHT + self.offsets[index] - self.scroll;
         let bubble_x = if message.direction == "rtl" {12.0} else {60.0};
         let bounds = gfx::RectF::new(bubble_x, row_y + 2.0, 286.0, chat_row_height(message) - 4.0);
         if point_in_rect(x, y, bounds)
         {
            let changed = self.focused_message_id.as_deref() != Some(self.selection_message_id.as_str()) || self.selection.is_some() || self.selection_caret.is_some();
            self.focused_message_id = Some(self.selection_message_id.clone());
            self.selection = None;
            self.selection_caret = None;
            return changed;
         }
      }
      false
   }

   fn host_key(&mut self, key_code: u16, shift: bool, value: &str) -> bool
   {
      if self.focused_message_id.as_deref() != Some(self.selection_message_id.as_str())
      {
         return self.host_text(value);
      }
      let Some(message) = self.messages.iter().chain(&self.appended_messages).find(|message| message.id == self.selection_message_id) else {return false};
      match key_code
      {
         115 =>
         {
            let changed = self.selection.is_some() || self.selection_caret != Some(0);
            self.selection = None;
            self.selection_caret = Some(0);
            changed
         }
         124 if shift =>
         {
            let start = self.selection.map_or(self.selection_caret.unwrap_or(0), |selection| selection.0);
            let end = self.selection.map_or(self.selection_caret.unwrap_or(0), |selection| selection.1);
            let next = message.text[end..].char_indices().nth(1).map_or(message.text.len(), |(offset, _)| end + offset);
            if next == end
            {
               return false;
            }
            self.selection = Some((start, next));
            self.selection_caret = Some(next);
            true
         }
         _ => self.host_text(value),
      }
   }

   fn max_scroll(&self) -> f32
   {
      (self.content_height - (VIEWPORT_HEIGHT - CHAT_HEADER_HEIGHT - CHAT_COMPOSER_HEIGHT)).max(0.0)
   }

   fn visible_range(&self, viewport_height: f32) -> core::ops::Range<usize>
   {
      let count = self.message_count();
      let start = self.offsets.partition_point(|offset| *offset <= self.scroll).saturating_sub(1).min(count);
      let bottom = self.scroll + (viewport_height - CHAT_HEADER_HEIGHT - CHAT_COMPOSER_HEIGHT).max(0.0);
      let mut end = start;
      while end < count && self.offsets[end] <= bottom && end - start < MAX_VISIBLE_CHAT_ROWS
      {
         end += 1;
      }
      start..end
   }

   fn role_counts(&self) -> Vec<RoleCount>
   {
      let visible = self.visible_range(VIEWPORT_HEIGHT).len() as u32;
      counts(&[("chat-thread", 1), ("message", visible), ("avatar", visible), ("composer", 1), ("send-control", 1)])
   }

   fn checkpoint_model(&self) -> serde_json::Value
   {
      serde_json::json!({
         "message_count": self.message_count(),
         "prepend_count": if self.prepend_active {self.prepend_messages.len()} else {0},
         "append_count": self.appended_messages.len(),
         "composer_utf8_count": self.composer.len(),
         "focused_message_id": self.focused_message_id,
         "selection_active": self.selection.is_some(),
         "replacement_applied": self.replacement_applied,
      })
   }

   fn draw<U: elements::ImageUploader, G: GeometrySink>(&self, viewport: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder, geometry: &mut G)
   {
      encode_solid_rects(builder, [viewport], BACKGROUND);
      let heading = gfx::RectF::new(viewport.x + 16.0, viewport.y + 8.0, 358.0, 36.0);
      encode_comparison_label("Live Chat", TEXT, elements::Align::Left, self.resources.latin_font, 20.0, heading, scale, text, uploader, builder);
      geometry.record("heading", "chat.heading", heading, &[heading]);
      geometry.record("chat-thread", "chat.thread", gfx::RectF::new(viewport.x, viewport.y + 52.0, viewport.w, 700.0), &[]);
      for index in self.visible_range(viewport.h)
      {
         let message = self.message(index);
         let row_y = viewport.y + CHAT_HEADER_HEIGHT + self.offsets[index] - self.scroll;
         let row_h = chat_row_height(message);
         let rtl = message.direction == "rtl";
         let avatar_x = if rtl {viewport.x + viewport.w - 48.0} else {viewport.x + 12.0};
         let bubble_x = if rtl {viewport.x + 12.0} else {viewport.x + 60.0};
         let bubble_w = 286.0;
         encode_rounded_atlas_image(builder, self.resources.thumbnail_atlas, gfx::RectF::new(avatar_x, row_y + 4.0, 36.0, 36.0), atlas_source(message.avatar_index), 18.0);
         builder.rrect(gfx::RectF::new(bubble_x, row_y + 4.0, bubble_w, row_h - 4.0), [12.0; 4], SHADOW);
         builder.rrect(gfx::RectF::new(bubble_x, row_y + 2.0, bubble_w, row_h - 4.0), [12.0; 4], SURFACE);
         let lines = encode_wrapped_inline_text(
            &message.text,
            TEXT,
            if rtl {elements::Align::Right} else {elements::Align::Left},
            2,
            font_for_text(&self.resources, &message.text),
            15.0,
            gfx::RectF::new(bubble_x + 10.0, row_y + 8.0, bubble_w - 20.0, row_h - 16.0),
            scale,
            &self.resources,
            text,
            uploader,
            builder,
         );
         geometry.record("message", &message.id, gfx::RectF::new(bubble_x, row_y + 2.0, bubble_w, row_h - 4.0), lines.as_slice());
         geometry.record_suffix("avatar", &message.id, ":avatar", gfx::RectF::new(avatar_x, row_y + 4.0, 36.0, 36.0), &[]);
      }

      let composer = gfx::RectF::new(viewport.x + 12.0, viewport.y + 780.0, 318.0, 48.0);
      builder.rrect(composer, [12.0; 4], SURFACE);
      let visible_composer = visible_text_tail(&self.composer, 80);
      let composer_line = gfx::RectF::new(composer.x + 10.0, composer.y + 8.0, composer.w - 20.0, 32.0);
      encode_inline_text(visible_composer, TEXT, font_for_text(&self.resources, visible_composer), 15.0, composer_line, scale, &self.resources, text, uploader, builder);
      let send = gfx::RectF::new(viewport.x + 338.0, viewport.y + 780.0, 40.0, 48.0);
      encode_comparison_label("Send", ACCENT, elements::Align::Center, self.resources.latin_font, 13.0, send, scale, text, uploader, builder);
      geometry.record("composer", "chat.composer", composer, &[composer_line]);
      geometry.record("send-control", "chat.send", send, &[send]);
   }
}

fn chat_row_height(message: &ChatMessage) -> f32
{
   const ROW_HEIGHTS: [f32; 10] = [58.0, 62.0, 66.0, 70.0, 74.0, 78.0, 82.0, 76.0, 70.0, 64.0];
   ROW_HEIGHTS[message.sequence as usize % ROW_HEIGHTS.len()]
}

fn visible_text_tail(value: &str, maximum_chars: usize) -> &str
{
   let start = value.char_indices().rev().nth(maximum_chars).map(|(index, character)| index + character.len_utf8()).unwrap_or(0);
   &value[start..]
}

#[derive(Clone, Copy)]
enum NavigationRoute
{
   List,
   Detail,
}

#[derive(Clone, Copy)]
struct ActiveNavigationTransition
{
   target: f32,
   elapsed_ms: u32,
   duration_ms: u32,
}

pub struct NavigationScene
{
   route: NavigationRoute,
   modal_progress: f32,
   transition: Option<ActiveNavigationTransition>,
   completed_cycles: u32,
   duration_ms: u32,
   host_drag_active: bool,
   labels: Vec<String>,
   resources: ComparisonResources,
}

impl NavigationScene
{
   fn new(fixture: NavigationFixture) -> Self
   {
      let mut labels = Vec::with_capacity(fixture.list_item_count as usize);
      for index in 0..fixture.list_item_count
      {
         labels.push(format!("Item {}", index + 1));
      }
      Self {
         route: NavigationRoute::List,
         modal_progress: 0.0,
         transition: None,
         completed_cycles: 0,
         duration_ms: fixture.transition.duration_ms,
         host_drag_active: false,
         labels,
         resources: ComparisonResources::default(),
      }
   }

   fn modal_visible(&self) -> bool
   {
      self.modal_progress > 0.0 || self.transition.is_some_and(|transition| transition.target > 0.0)
   }

   fn update(&mut self, dt_ms: u32)
   {
      let Some(mut transition) = self.transition else {return};
      transition.elapsed_ms = transition.elapsed_ms.saturating_add(dt_ms);
      let t = (transition.elapsed_ms as f32 / transition.duration_ms.max(1) as f32).clamp(0.0, 1.0);
      let eased = t * t * (3.0 - 2.0 * t);
      self.modal_progress = if transition.target > 0.0 {eased} else {1.0 - eased};
      if t >= 1.0
      {
         self.modal_progress = transition.target;
         self.transition = None;
      }
      else
      {
         self.transition = Some(transition);
      }
   }

   fn apply(&mut self, event: &TraceEvent) -> Result<(), String>
   {
      if matches!(event.op, TraceOperation::PointerDown | TraceOperation::PointerMove | TraceOperation::PointerCancel)
      {
         if event.op == TraceOperation::PointerDown
         {
            self.route = NavigationRoute::Detail;
         }
         if event.op == TraceOperation::PointerMove
         {
            self.modal_progress = event.x_millionths.unwrap_or(0) as f32 / 1_000_000.0;
         }
         if event.op == TraceOperation::PointerCancel
         {
            self.transition = Some(ActiveNavigationTransition {target: 0.0, elapsed_ms: 0, duration_ms: self.duration_ms});
         }
         return Ok(());
      }
      if event.op != TraceOperation::Navigate
      {
         return Err(format!("navigation rejects {:?}", event.op));
      }
      match event.target.as_deref()
      {
         Some("navigation:item:05") => self.route = NavigationRoute::Detail,
         Some("navigation:modal") => self.transition = Some(ActiveNavigationTransition {target: 1.0, elapsed_ms: 0, duration_ms: self.duration_ms}),
         Some("navigation:dismiss-control") => self.transition = Some(ActiveNavigationTransition {target: 0.0, elapsed_ms: 0, duration_ms: self.duration_ms}),
         Some("navigation:back-control") =>
         {
            self.route = NavigationRoute::List;
            self.modal_progress = 0.0;
            self.transition = None;
            self.completed_cycles = self.completed_cycles.saturating_add(1);
         }
         Some("navigation:cancel") =>
         {
            self.route = NavigationRoute::List;
            self.modal_progress = 0.0;
            self.transition = None;
         }
         _ => return Err(String::from("navigation target is unsupported")),
      }
      Ok(())
   }

   fn host_click(&mut self, x: f32, y: f32) -> bool
   {
      if self.modal_visible()
      {
         let modal_x = 24.0 + (1.0 - self.modal_progress) * 390.0;
         if point_in_rect(x, y, gfx::RectF::new(modal_x + 268.0, 150.0, 50.0, 28.0))
         {
            self.transition = Some(ActiveNavigationTransition {target: 0.0, elapsed_ms: 0, duration_ms: self.duration_ms});
            return true;
         }
         return false;
      }
      match self.route
      {
         NavigationRoute::List =>
         {
            let selected = gfx::RectF::new(16.0, 56.0 + 4.0 * 62.0, 358.0, 54.0);
            if point_in_rect(x, y, selected)
            {
               self.route = NavigationRoute::Detail;
               return true;
            }
         }
         NavigationRoute::Detail =>
         {
            if point_in_rect(x, y, gfx::RectF::new(16.0, 6.0, 58.0, 52.0))
            {
               self.route = NavigationRoute::List;
               self.modal_progress = 0.0;
               self.transition = None;
               self.completed_cycles = self.completed_cycles.saturating_add(1);
               return true;
            }
            if point_in_rect(x, y, gfx::RectF::new(16.0, 64.0, 358.0, 180.0))
            {
               self.transition = Some(ActiveNavigationTransition {target: 1.0, elapsed_ms: 0, duration_ms: self.duration_ms});
               return true;
            }
         }
      }
      false
   }

   fn host_pointer_down(&mut self, x: f32, y: f32) -> bool
   {
      self.host_drag_active = matches!(self.route, NavigationRoute::List)
         && x >= 390.0 * 0.9
         && (VIEWPORT_HEIGHT * 0.4..=VIEWPORT_HEIGHT * 0.6).contains(&y);
      false
   }

   fn host_pointer_move(&mut self, x: f32, _y: f32) -> bool
   {
      if !self.host_drag_active
      {
         return false;
      }
      let progress = (x / 390.0).clamp(0.0, 1.0);
      let changed = !matches!(self.route, NavigationRoute::Detail) || self.modal_progress != progress || self.transition.is_some();
      self.route = NavigationRoute::Detail;
      self.modal_progress = progress;
      self.transition = None;
      changed
   }

   fn host_pointer_up(&mut self) -> bool
   {
      if !self.host_drag_active
      {
         return false;
      }
      self.host_drag_active = false;
      let changed = !matches!(self.route, NavigationRoute::List) || self.modal_progress != 0.0 || self.transition.is_some();
      self.route = NavigationRoute::List;
      self.modal_progress = 0.0;
      self.transition = None;
      changed
   }

   fn role_counts(&self) -> Vec<RoleCount>
   {
      if self.modal_visible()
      {
         counts(&[("detail", 1), ("modal", 1), ("dismiss-control", 1), ("back-control", 1)])
      }
      else
      {
         counts(&[("navigation-list", 1), ("list-item", self.labels.len() as u32)])
      }
   }

   fn checkpoint_model(&self) -> serde_json::Value
   {
      serde_json::json!({
         "route": match self.route {NavigationRoute::List => "list", NavigationRoute::Detail => "detail"},
         "modal_visible": self.modal_visible(),
         "completed_cycles": self.completed_cycles,
      })
   }

   fn draw<U: elements::ImageUploader, G: GeometrySink>(&self, viewport: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder, geometry: &mut G)
   {
      encode_solid_rects(builder, [viewport], BACKGROUND);
      match self.route
      {
         NavigationRoute::List =>
         {
            let heading = gfx::RectF::new(viewport.x + 16.0, viewport.y + 6.0, viewport.w - 32.0, 52.0);
            encode_comparison_label("Navigation", TEXT, elements::Align::Left, self.resources.latin_font, 20.0, heading, scale, text, uploader, builder);
            geometry.record("navigation-list", "navigation.table", viewport, &[]);
            geometry.record("heading", "navigation.heading", heading, &[heading]);
            for (index, label) in self.labels.iter().enumerate()
            {
               let card = gfx::RectF::new(viewport.x + 16.0, viewport.y + 56.0 + index as f32 * 62.0, viewport.w - 32.0, 54.0);
               builder.rrect(gfx::RectF::new(card.x, card.y + 2.0, card.w, card.h), [12.0; 4], SHADOW);
               builder.rrect(card, [12.0; 4], SURFACE);
               let title = encode_comparison_label(label, TEXT, elements::Align::Left, self.resources.latin_font, 15.0, gfx::RectF::new(card.x + 10.0, card.y + 10.0 + 2.0 / 3.0, 260.0, 24.0), scale, text, uploader, builder);
               let subtitle = encode_comparison_label("Canonical navigation row", SECONDARY, elements::Align::Left, self.resources.latin_font, 11.0, gfx::RectF::new(card.x + 10.0, card.y + 30.0 + 1.0 / 3.0, 260.0, 20.0), scale, text, uploader, builder);
               geometry.record_indexed("list-item", "navigation:item:", index + 1, 2, card, &[title, subtitle]);
            }
         }
         NavigationRoute::Detail =>
         {
            let back = gfx::RectF::new(viewport.x + 16.0, viewport.y + 6.0, 58.0, 52.0);
            let back_line = encode_comparison_label("‹ Back", ACCENT, elements::Align::Left, self.resources.latin_font, 15.0, back, scale, text, uploader, builder);
            let heading = gfx::RectF::new(viewport.x + 82.0, viewport.y + 6.0, 180.0, 52.0);
            encode_comparison_label("Detail", TEXT, elements::Align::Left, self.resources.latin_font, 20.0, heading, scale, text, uploader, builder);
            let detail = gfx::RectF::new(viewport.x + 16.0, viewport.y + 64.0, viewport.w - 32.0, 180.0);
            builder.rrect(gfx::RectF::new(detail.x, detail.y + 2.0, detail.w, detail.h), [12.0; 4], SHADOW);
            builder.rrect(detail, [12.0; 4], SURFACE);
            let title = gfx::RectF::new(detail.x + 12.0, detail.y + 10.0 + 2.0 / 3.0, 280.0, 24.0);
            let subtitle = gfx::RectF::new(detail.x + 12.0, detail.y + 34.0 + 1.0 / 3.0, 310.0, 20.0);
            encode_comparison_label("Selected destination", TEXT, elements::Align::Left, self.resources.latin_font, 15.0, title, scale, text, uploader, builder);
            encode_comparison_label("Canonical navigation detail", SECONDARY, elements::Align::Left, self.resources.latin_font, 11.0, subtitle, scale, text, uploader, builder);
            geometry.record("detail", "navigation.detail", detail, &[]);
            geometry.record("back-control", "navigation.back", back, &[back_line]);
            geometry.record("heading", "navigation.detail-heading", heading, &[heading]);
            geometry.record("label", "navigation.detail-title", title, &[title]);
            geometry.record("label", "navigation.detail-subtitle", subtitle, &[subtitle]);
         }
      }
      if self.modal_visible()
      {
         encode_solid_rects(builder, [viewport], gfx::Color::rgba(MODAL_OVERLAY.r, MODAL_OVERLAY.g, MODAL_OVERLAY.b, (176.0 / 255.0) * self.modal_progress));
         let offset = (1.0 - self.modal_progress) * viewport.w;
         let modal_x = snap_to_physical_pixel(viewport.x + 24.0 + offset, scale);
         let modal = gfx::RectF::new(modal_x, viewport.y + 132.0, viewport.w - 48.0, 580.0);
         builder.rrect(gfx::RectF::new(modal.x, modal.y + 2.0, modal.w, modal.h), [16.0; 4], SHADOW);
         builder.rrect(modal, [16.0; 4], SURFACE);
         let heading = gfx::RectF::new(modal.x + 18.0, modal.y + 18.0, 220.0, 28.0);
         let first_body = gfx::RectF::new(modal.x + 18.0, modal.y + 48.0, 270.0, 20.0);
         let second_body = gfx::RectF::new(modal.x + 18.0, modal.y + 67.0, 240.0, 20.0);
         encode_comparison_label("Modal", TEXT, elements::Align::Left, self.resources.latin_font, 20.0, heading, scale, text, uploader, builder);
         encode_comparison_label("Canonical modal content", SECONDARY, elements::Align::Left, self.resources.latin_font, 11.0, first_body, scale, text, uploader, builder);
         encode_comparison_label("Transition remains apples-to-apples", SECONDARY, elements::Align::Left, self.resources.latin_font, 11.0, second_body, scale, text, uploader, builder);
         let dismiss = gfx::RectF::new(modal.x + 268.0, modal.y + 18.0, 50.0, 28.0);
         builder.rrect(dismiss, [10.0; 4], ACCENT);
         let dismiss_line = encode_comparison_label("Done", SURFACE, elements::Align::Center, self.resources.latin_font, 13.0, dismiss, scale, text, uploader, builder);
         geometry.record("modal", "navigation.modal", modal, &[]);
         geometry.record("dismiss-control", "navigation.dismiss", dismiss, &[dismiss_line]);
         geometry.record("heading", "navigation.modal-heading", heading, &[heading]);
         geometry.record("label", "navigation.modal-body-first", first_body, &[first_body]);
         geometry.record("label", "navigation.modal-body-second", second_body, &[second_body]);
      }
   }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ImageStage
{
   Thumbnail,
   BytesReady,
   Decoded,
   Uploaded,
   Visible,
}

#[derive(Clone, Copy)]
struct ActiveImageTouch
{
   id: u32,
   x: f32,
   y: f32,
}

pub struct ImageScene
{
   fixture: ImageDecodeZoomFixture,
   stage: ImageStage,
   pan_x: f32,
   pan_y: f32,
   scale: f32,
   touches: Vec<ActiveImageTouch>,
   drag_origin: Option<(u32, f32, f32, f32, f32)>,
   pinch_origin: Option<(f32, f32)>,
   host_zoom_drag_active: bool,
   resources: ComparisonResources,
}

impl ImageScene
{
   fn new(fixture: ImageDecodeZoomFixture) -> Self
   {
      Self {
         fixture,
         stage: ImageStage::Thumbnail,
         pan_x: 0.0,
         pan_y: 0.0,
         scale: 1.0,
         touches: Vec::with_capacity(2),
         drag_origin: None,
         pinch_origin: None,
         host_zoom_drag_active: false,
         resources: ComparisonResources::default(),
      }
   }

   fn set_thumbnail_resource(&mut self, thumbnail: gfx::ImageHandle, thumbnail_sha256: &str) -> Result<(), String>
   {
      if thumbnail == gfx::ImageHandle(0)
      {
         return Err(String::from("comparison thumbnail resource must use a nonzero uploaded handle"));
      }
      if thumbnail_sha256 != self.fixture.thumbnail.artifact.sha256
      {
         return Err(String::from("comparison thumbnail resource does not match the fixture SHA-256 identity"));
      }
      self.resources.thumbnail_image = thumbnail;
      Ok(())
   }

   fn set_source_resource(&mut self, source: gfx::ImageHandle, source_sha256: &str) -> Result<(), String>
   {
      if source == gfx::ImageHandle(0)
      {
         return Err(String::from("comparison source resource must use a nonzero uploaded handle"));
      }
      if source_sha256 != self.fixture.source.artifact.sha256
      {
         return Err(String::from("comparison source resource does not match the fixture SHA-256 identity"));
      }
      self.resources.source_image = source;
      Ok(())
   }

   fn apply(&mut self, event: &TraceEvent) -> Result<(), String>
   {
      match event.op
      {
         TraceOperation::ResourceArrival =>
         {
            self.stage = match (self.stage, event.target.as_deref())
            {
               (ImageStage::Thumbnail, Some("image:source-bytes")) => ImageStage::BytesReady,
               (ImageStage::BytesReady, Some("image:decoded")) => ImageStage::Decoded,
               (ImageStage::Decoded, Some("image:texture")) => ImageStage::Uploaded,
               (ImageStage::Uploaded, Some("image:presented")) => ImageStage::Visible,
               _ => return Err(String::from("image resource transition target is unsupported")),
            };
         }
         TraceOperation::PointerDown | TraceOperation::PointerMove | TraceOperation::PointerUp | TraceOperation::PointerCancel => self.apply_touch(event)?,
         _ => return Err(format!("image rejects {:?}", event.op)),
      }
      Ok(())
   }

   fn apply_touch(&mut self, event: &TraceEvent) -> Result<(), String>
   {
      let id = event.pointer.ok_or_else(|| String::from("image touch has no pointer identity"))?;
      let x = event.x_millionths.ok_or_else(|| String::from("image touch has no x coordinate"))? as f32 / 1_000_000.0;
      let y = event.y_millionths.ok_or_else(|| String::from("image touch has no y coordinate"))? as f32 / 1_000_000.0;
      match event.op
      {
         TraceOperation::PointerDown =>
         {
            if self.touches.iter().all(|touch| touch.id != id) && self.touches.len() < 2
            {
               self.touches.push(ActiveImageTouch {id, x, y});
            }
            self.restart_gesture();
         }
         TraceOperation::PointerMove =>
         {
            if let Some(touch) = self.touches.iter_mut().find(|touch| touch.id == id)
            {
               touch.x = x;
               touch.y = y;
            }
            self.update_gesture();
         }
         TraceOperation::PointerUp | TraceOperation::PointerCancel =>
         {
            if let Some(touch) = self.touches.iter_mut().find(|touch| touch.id == id)
            {
               touch.x = x;
               touch.y = y;
            }
            self.update_gesture();
            self.touches.retain(|touch| touch.id != id);
            self.restart_gesture();
         }
         _ => {}
      }
      Ok(())
   }

   fn restart_gesture(&mut self)
   {
      match self.touches.as_slice()
      {
         [touch] =>
         {
            self.drag_origin = Some((touch.id, touch.x, touch.y, self.pan_x, self.pan_y));
            self.pinch_origin = None;
         }
         [first, second] =>
         {
            self.drag_origin = None;
            self.pinch_origin = Some((touch_distance(*first, *second).max(0.000_001), self.scale));
         }
         _ =>
         {
            self.drag_origin = None;
            self.pinch_origin = None;
         }
      }
   }

   fn update_gesture(&mut self)
   {
      if let ([first, second], Some((distance, initial_scale))) = (self.touches.as_slice(), self.pinch_origin)
      {
         let maximum = self.fixture.pinch_scale_millionths as f32 / 1_000_000.0;
         self.scale = (initial_scale * touch_distance(*first, *second) / distance).clamp(1.0, maximum);
      }
      else if let ([touch], Some((id, x, y, pan_x, pan_y))) = (self.touches.as_slice(), self.drag_origin)
      {
         if touch.id == id
         {
            let maximum = self.fixture.pan_distance_millionths as f32 / 1_000_000.0;
            self.pan_x = (pan_x + touch.x - x).clamp(-maximum, maximum);
            self.pan_y = (pan_y + touch.y - y).clamp(-maximum, maximum);
         }
      }
   }

   fn input_pointer(&mut self, _x: f32, _y: f32, dx: f32, dy: f32, buttons: u32)
   {
      if buttons & 1 == 0
      {
         return;
      }
      let maximum = self.fixture.pan_distance_millionths as f32 / 1_000_000.0;
      self.pan_x = (self.pan_x + dx / 390.0).clamp(-maximum, maximum);
      self.pan_y = (self.pan_y + dy / VIEWPORT_HEIGHT).clamp(-maximum, maximum);
   }

   fn host_pointer_delta(&mut self, dx: f32, dy: f32) -> bool
   {
      let previous = (self.pan_x, self.pan_y);
      self.input_pointer(0.0, 0.0, dx, dy, 1);
      (self.pan_x, self.pan_y) != previous
   }

   fn host_pointer_down(&mut self, x: f32, y: f32) -> bool
   {
      let zoom = gfx::RectF::new(16.0, 811.0, 358.0, 6.0);
      self.host_zoom_drag_active = point_in_rect(x, y, zoom);
      false
   }

   fn host_pointer_move(&mut self, x: f32, _y: f32, dx: f32, dy: f32) -> bool
   {
      if !self.host_zoom_drag_active {return self.host_pointer_delta(dx, dy)}
      let previous = self.scale;
      let maximum = self.fixture.pinch_scale_millionths as f32 / 1_000_000.0;
      let scale = (1.0 + ((x - 16.0) / 358.0) * (maximum - 1.0)).clamp(1.0, maximum);
      self.scale = (scale * 1_000_000.0).round() / 1_000_000.0;
      self.scale != previous
   }

   fn host_pointer_up(&mut self) -> bool
   {
      self.host_zoom_drag_active = false;
      false
   }

   fn role_counts(&self) -> Vec<RoleCount>
   {
      counts(&[("image-canvas", 1), ("image", 1), ("zoom-control", 1)])
   }

   fn checkpoint_model(&self) -> serde_json::Value
   {
      serde_json::json!({
         "resource_stage": match self.stage
         {
            ImageStage::Thumbnail => "thumbnail",
            ImageStage::BytesReady => "bytes-ready",
            ImageStage::Decoded => "decoded",
            ImageStage::Uploaded => "uploaded",
            ImageStage::Visible => "visible",
         },
         "pan_x_millionths": (self.pan_x * 1_000_000.0).round() as i32,
         "pan_y_millionths": (self.pan_y * 1_000_000.0).round() as i32,
         "scale_millionths": (self.scale * 1_000_000.0).round() as i32,
         "active_pointer_count": self.touches.len(),
      })
   }

   fn draw<U: elements::ImageUploader, G: GeometrySink>(&self, viewport: gfx::RectF, scale: f32, text: &mut elements::TextCtx, uploader: &mut U, builder: &mut DrawListBuilder, geometry: &mut G)
   {
      encode_solid_rects(builder, [viewport], BACKGROUND);
      let heading = gfx::RectF::new(viewport.x + 18.0, viewport.y + 8.0, 204.0, 36.0);
      encode_comparison_label("Decode & Zoom", TEXT, elements::Align::Left, self.resources.latin_font, 20.0, heading, scale, text, uploader, builder);
      geometry.record("heading", "image.heading", heading, &[heading]);
      let canvas = gfx::RectF::new(viewport.x, viewport.y + 52.0, viewport.w, viewport.h - 104.0);
      geometry.record("image-canvas", "image.canvas", canvas, &[]);
      builder.clip_push(gfx::RectI::new(canvas.x.floor() as i32, canvas.y.floor() as i32, canvas.w.ceil() as i32, canvas.h.ceil() as i32));
      if self.stage == ImageStage::Visible
      {
         let width = self.fixture.source.width as f32 / (scale * IMAGE_BASE_DOWNSAMPLE) * self.scale;
         let height = self.fixture.source.height as f32 / (scale * IMAGE_BASE_DOWNSAMPLE) * self.scale;
         let origin_x = canvas.x + (canvas.w - width) * 0.5 + self.pan_x * canvas.w;
         let origin_y = canvas.y + (canvas.h - height) * 0.5 + self.pan_y * canvas.h;
         let destination = gfx::RectF::new(
            snap_image_sampling_origin(origin_x, scale, self.scale),
            snap_image_sampling_origin(origin_y, scale, self.scale),
            width,
            height,
         );
         builder.image(self.resources.source_image, destination, gfx::RectF::new(0.0, 0.0, self.fixture.source.width as f32, self.fixture.source.height as f32), 1.0);
         geometry.record("image", "image.source", destination, &[]);
         let border_height = 1.0 / scale;
         let top = (destination.y * scale).floor() / scale;
         let bottom = ((destination.y + destination.h) * scale).floor() / scale;
         encode_solid_rects(
            builder,
            [
               gfx::RectF::new(canvas.x, top, canvas.w, border_height),
               gfx::RectF::new(canvas.x, bottom, canvas.w, border_height),
            ],
            BACKGROUND,
         );
      }
      else
      {
         let thumbnail = gfx::RectF::new(canvas.x + 16.0, canvas.y + 16.0, 128.0, 96.0);
         builder.image(self.resources.thumbnail_image, thumbnail, gfx::RectF::new(0.0, 0.0, self.fixture.thumbnail.width as f32, self.fixture.thumbnail.height as f32), 1.0);
         geometry.record("image", "image.thumbnail", thumbnail, &[]);
      }
      builder.clip_pop();
      let track = gfx::RectF::new(viewport.x + 16.0, viewport.y + 811.0, viewport.w - 32.0, 6.0);
      builder.rrect(track, [0.0; 4], DISABLED_SLIDER_TRACK);
      geometry.record("zoom-control", "image.zoom", track, &[]);
   }
}

fn touch_distance(first: ActiveImageTouch, second: ActiveImageTouch) -> f32
{
   let dx = first.x - second.x;
   let dy = first.y - second.y;
   (dx * dx + dy * dy).sqrt()
}
