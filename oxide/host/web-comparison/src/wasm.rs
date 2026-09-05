use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Promise, Uint8Array};
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;
use wasm_bindgen::{closure::Closure, JsCast};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{EventTarget, HtmlCanvasElement, KeyboardEvent, PointerEvent, Response};

use oxide_benchmark_spec::{AssetManifest, FontPackManifest, ImageDecodeZoomFixture, ScenarioSpec, TraceEvent, TraceOperation, TraceValue};
use oxide_renderer_api as gfx;
use oxide_renderer_api::Renderer;
use oxide_renderer_web::{BrowserRenderer, WebRendererStats};
use oxide_test_scenes as scenes;
use oxide_text as text;
use oxide_ui_core as ui;

const CSS_WIDTH: u32 = 390;
const CSS_HEIGHT: u32 = 844;
const DEVICE_SCALE: f32 = 3.0;
const PHYSICAL_WIDTH: u32 = CSS_WIDTH * 3;
const PHYSICAL_HEIGHT: u32 = CSS_HEIGHT * 3;
const TIMESTAMP_SETTLE_RAFS: u32 = 60;
const RESOURCE_SETTLE_RAFS: u32 = 600;
const SCENARIOS: [&str; 6] = [
   "startup.first-screen",
   "dashboard.mixed-static",
   "feed.variable-scroll",
   "chat.live-update",
   "navigation.modal",
   "image.decode-zoom",
];

const NEUTRAL_ASSET_MANIFEST: &[u8] = include_bytes!("../../../benchmarks/comparative/specs/v1/assets/neutral-v1.json");
const THUMBNAIL_ATLAS: &[u8] = include_bytes!("../../../benchmarks/comparative/specs/v1/assets/neutral-thumbnail-atlas-v1.png");
const INLINE_TEXT_ATLAS_128: &[u8] = include_bytes!("../../../benchmarks/comparative/specs/v1/assets/inline-text-atlas-v1.png");
const INLINE_TEXT_ATLAS_45: &[u8] = include_bytes!("../../../benchmarks/comparative/specs/v1/assets/inline-text-atlas-v1-45px.png");
const INLINE_TEXT_ATLAS_39: &[u8] = include_bytes!("../../../benchmarks/comparative/specs/v1/assets/inline-text-atlas-v1-39px.png");
const IMAGE_THUMBNAIL: &[u8] = include_bytes!("../../../benchmarks/comparative/specs/v1/assets/image-decode-zoom-thumbnail-v1.png");
const FONT_MANIFEST: &[u8] = include_bytes!("../../../benchmarks/comparative/specs/v1/font-packs/oxide-bench-fonts-v1.json");
const LATIN_FONT: &[u8] = include_bytes!("../../../benchmarks/comparative/specs/v1/font-packs/oxide-bench-fonts-v1/NotoSans-VF.ttf");
const ARABIC_FONT: &[u8] = include_bytes!("../../../benchmarks/comparative/specs/v1/font-packs/oxide-bench-fonts-v1/NotoSansArabic-VF.ttf");
const CJK_FONT: &[u8] = include_bytes!("../../../benchmarks/comparative/specs/v1/font-packs/oxide-bench-fonts-v1/NotoSansSC-VF.ttf");

struct WebUploader
{
   renderer: Rc<RefCell<BrowserRenderer>>,
}

impl ui::elements::ImageUploader for WebUploader
{
   fn create_a8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> gfx::ImageHandle
   {
      self.renderer.borrow_mut().image_create_a8(width, height, data, row_bytes)
   }

   fn update_a8(&mut self, handle: gfx::ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize)
   {
      self.renderer.borrow_mut().image_update_a8(handle, x, y, width, height, data, row_bytes);
   }

   fn append_a8(&mut self, handle: gfx::ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize)
   {
      self.renderer.borrow_mut().image_append_a8(handle, x, y, width, height, data, row_bytes);
   }

   fn release_a8(&mut self, handle: gfx::ImageHandle)
   {
      self.renderer.borrow_mut().image_release(handle);
   }
}

#[derive(Default)]
struct ScenarioResources
{
   images: Vec<gfx::ImageHandle>,
}

struct PointerListener
{
   name: &'static str,
   callback: Closure<dyn FnMut(PointerEvent)>,
}

struct KeyboardListener
{
   callback: Closure<dyn FnMut(KeyboardEvent)>,
}

struct Runtime
{
   canvas: HtmlCanvasElement,
   renderer: Rc<RefCell<BrowserRenderer>>,
   router: scenes::Router<WebUploader>,
   builder: ui::DrawListBuilder,
   coalesced: Vec<gfx::DrawCmd>,
   damage_rects: Vec<gfx::RectI>,
   resources: ScenarioResources,
   scenario_id: Option<String>,
   fixture_id: Option<String>,
   seed: u64,
   generation: u32,
   ready: bool,
   started: bool,
   last_timestamp_ms: f64,
   last_pointer: Option<(f32, f32)>,
   last_error: Option<String>,
   rapid_events: Vec<TraceEvent>,
   listeners: Vec<PointerListener>,
   keyboard_listener: Option<KeyboardListener>,
   image_source_loading: bool,
   image_resource_timing: Option<Value>,
}

impl Runtime
{
   fn clear_scenario(&mut self)
   {
      self.router.text.trim_memory_with_uploader(&mut self.router.uploader);
      {
         let mut renderer = self.renderer.borrow_mut();
         for handle in self.resources.images.drain(..)
         {
            renderer.image_release(handle);
         }
      }
      self.router = scenes::Router::new(WebUploader {renderer: Rc::clone(&self.renderer)});
      self.builder.clear();
      self.coalesced.clear();
      self.damage_rects.clear();
      self.scenario_id = None;
      self.fixture_id = None;
      self.ready = false;
      self.last_pointer = None;
      self.last_error = None;
      self.rapid_events.clear();
      self.image_source_loading = false;
      self.image_resource_timing = None;
   }

   fn reset(&mut self, scenario_id: &str, seed: u64, generation: u32) -> Result<(), JsValue>
   {
      let package = embedded_scenario(scenario_id)?;
      let fixture_value: Value = serde_json::from_slice(package.fixture).map_err(|error| js_error(format!("parsing fixture {scenario_id}: {error}")))?;
      let fixture_id = fixture_value.get("id").and_then(Value::as_str).ok_or_else(|| js_error(format!("fixture {scenario_id} has no id")))?.to_string();
      let fonts = load_fonts()?;
      let decoded = decode_resources(scenario_id)?;

      self.clear_scenario();
      let mut resources = upload_resources(&self.renderer, &decoded)?;
      let mut router = scenes::Router::new(WebUploader {renderer: Rc::clone(&self.renderer)});
      if let Err(error) = router.prepare_comparison_scenario(&package.scenario, package.fixture)
      {
         release_resources(&self.renderer, &mut resources);
         return Err(js_error(error));
      }
      let [latin, arabic, cjk] = fonts;
      let font_ids = [router.text.fonts.add_font(latin), router.text.fonts.add_font(arabic), router.text.fonts.add_font(cjk)];
      router.text.set_fallback_fonts(&font_ids);
      if let Err(error) = configure_resources(&mut router, scenario_id, font_ids, &resources, decoded.inline_text_atlas)
      {
         release_resources(&self.renderer, &mut resources);
         return Err(js_error(error));
      }

      self.router = router;
      self.resources = resources;
      self.scenario_id = Some(scenario_id.to_string());
      self.fixture_id = Some(fixture_id);
      self.seed = seed;
      self.generation = generation;
      self.ready = false;
      self.last_timestamp_ms = 0.0;
      self.last_error = None;
      self.rapid_events = rapid_events(scenario_id)?;
      self.render_at(performance_now())
   }

   fn render_at(&mut self, timestamp_ms: f64) -> Result<(), JsValue>
   {
      if self.scenario_id.is_none()
      {
         return Err(js_error("no comparison scenario is prepared"));
      }
      let timestamp_ms = if timestamp_ms.is_finite() {timestamp_ms.max(0.0)} else {0.0};
      let dt_ms = if self.last_timestamp_ms == 0.0
      {
         0
      }
      else
      {
         (timestamp_ms - self.last_timestamp_ms).clamp(0.0, f64::from(u32::MAX)) as u32
      };
      self.last_timestamp_ms = timestamp_ms;
      self.builder.clear();
      self.router.update(timestamp_ms.floor() as u64, dt_ms);
      self.router.draw(gfx::RectF::new(0.0, 0.0, CSS_WIDTH as f32, CSS_HEIGHT as f32), DEVICE_SCALE, &mut self.builder);
      self.router.take_damage_into(&mut self.damage_rects);
      ui::coalesce_adjacent_draws_reuse(self.builder.drawlist_mut(), &mut self.coalesced);

      let mut damage = gfx::Damage {rects: core::mem::take(&mut self.damage_rects)};
      let result = {
         let mut renderer = self.renderer.borrow_mut();
         renderer.set_animation_time_ms(timestamp_ms);
         let token = renderer.begin_frame(&gfx::FrameTarget, Some(&damage));
         renderer.encode_pass(self.builder.drawlist());
         renderer.submit(token).map_err(render_error)
      };
      self.damage_rects = core::mem::take(&mut damage.rects);
      result?;
      self.canvas.set_attribute("data-visual-generation", &self.generation.to_string())?;
      Ok(())
   }

   fn pointer_event(&mut self, event: &PointerEvent)
   {
      if self.scenario_id.is_none()
      {
         return;
      }
      event.prevent_default();
      let rect = self.canvas.get_bounding_client_rect();
      let x = event.client_x() as f32 - rect.left() as f32;
      let y = event.client_y() as f32 - rect.top() as f32;
      let (dx, dy) = self.last_pointer.map_or((0.0, 0.0), |(last_x, last_y)| (x - last_x, y - last_y));
      let buttons = event.buttons() as u32;
      self.router.input_pointer(x, y, dx, dy, buttons);
      if event.type_() == "pointerup"
      {
         if let Err(error) = self.apply_trusted_pointer_action(x, y)
         {
            self.last_error = Some(error);
            return;
         }
      }
      self.last_pointer = if buttons == 0 {None} else {Some((x, y))};
      self.generation = self.generation.saturating_add(1);
      if let Err(error) = self.render_at(performance_now())
      {
         self.last_error = error.as_string().or_else(|| Some(String::from("pointer frame failed")));
      }
   }

   fn apply_trusted_pointer_action(&mut self, x: f32, y: f32) -> Result<(), String>
   {
      let scenario_id = self.scenario_id.as_deref().ok_or_else(|| String::from("no comparison scenario is prepared"))?;
      let event = if scenario_id == "startup.first-screen" && y >= 776.0
      {
         Some(trace_event(TraceOperation::ResourceArrival, Some("fresh-install"), None, Some("startup:fresh-install-ready"), None, None))
      }
      else if scenario_id == "dashboard.mixed-static" && (161.0..=179.0).contains(&x) && (61.0..=73.0).contains(&y)
      {
         Some(trace_event(TraceOperation::Mutate, Some("dashboard:label:003"), Some(TraceValue::Integer(1)), Some("dashboard:leaf-update:00"), None, None))
      }
      else if scenario_id == "feed.variable-scroll"
      {
         let y_millionths = (y.clamp(0.0, CSS_HEIGHT as f32) / CSS_HEIGHT as f32 * 1_000_000.0).round() as i32;
         Some(trace_event(TraceOperation::PointerUp, None, None, Some("feed:trusted-scroll"), Some(500_000), Some(y_millionths)))
      }
      else if scenario_id == "navigation.modal" && (452.0..=523.0).contains(&y)
      {
         Some(trace_event(TraceOperation::Navigate, Some("navigation:item:05"), None, Some("navigation:cycle:0:detail"), None, None))
      }
      else
      {
         None
      };
      if let Some(event) = event
      {
         self.router.apply_comparison_event(&event)?;
      }
      Ok(())
   }

   fn keyboard_event(&mut self, event: &KeyboardEvent)
   {
      if self.scenario_id.as_deref() != Some("chat.live-update") || event.type_() != "keydown" || event.key() != "x"
      {
         return;
      }
      event.prevent_default();
      let trace = trace_event(TraceOperation::CommitText, Some("chat:composer"), Some(TraceValue::Text(String::from("x"))), Some("chat:trusted-character"), None, None);
      if let Err(error) = self.router.apply_comparison_event(&trace)
      {
         self.last_error = Some(error);
         return;
      }
      self.generation = self.generation.saturating_add(1);
      if let Err(error) = self.render_at(performance_now())
      {
         self.last_error = error.as_string().or_else(|| Some(String::from("keyboard frame failed")));
      }
   }

   fn snapshot_value(&mut self) -> Result<Value, JsValue>
   {
      let scenario_id = self.scenario_id.as_deref().ok_or_else(|| js_error("no comparison scenario is prepared"))?;
      let (state, _) = self.router.comparison_checkpoint_json("snapshot").map_err(js_error)?;
      let state: Value = serde_json::from_slice(&state).map_err(|error| js_error(format!("parsing snapshot state: {error}")))?;
      let stats = self.renderer.borrow_mut().collect_timestamp_readbacks();
      let mut logical_counters = renderer_counters(stats);
      logical_counters["image_resource_timing"] = self.image_resource_timing.clone().unwrap_or(Value::Null);
      Ok(json!({
         "scene": {
            "scenario_id": scenario_id,
            "fixture_id": self.fixture_id,
         },
         "state": state,
         "target_geometry": {
            "x": 0,
            "y": 0,
            "width": CSS_WIDTH,
            "height": CSS_HEIGHT,
            "scale": DEVICE_SCALE,
            "physical_width": PHYSICAL_WIDTH,
            "physical_height": PHYSICAL_HEIGHT,
         },
         "logical_counters": logical_counters,
         "gpu_pass_timestamps": timestamp_diagnostics(stats),
         "seed": self.seed,
         "generation": self.generation,
         "ready": self.ready,
         "error": self.last_error,
      }))
   }

   fn checkpoint_value(&self, checkpoint_id: &str) -> Result<Value, JsValue>
   {
      let (state, accessibility) = self.router.comparison_checkpoint_json(checkpoint_id).map_err(js_error)?;
      let state: Value = serde_json::from_slice(&state).map_err(|error| js_error(format!("parsing checkpoint state: {error}")))?;
      let accessibility: Value = serde_json::from_slice(&accessibility).map_err(|error| js_error(format!("parsing checkpoint accessibility: {error}")))?;
      let raw_accessibility = accessibility.get("nodes").cloned().unwrap_or_else(|| json!([]));
      Ok(json!({
         "state": state,
         "accessibility": accessibility,
         "raw_accessibility": raw_accessibility,
      }))
   }
}

struct EmbeddedScenario
{
   scenario: ScenarioSpec,
   fixture: &'static [u8],
}

struct DecodedImage
{
   width: u32,
   height: u32,
   rgba: Vec<u8>,
}

struct DecodedResources
{
   images: Vec<DecodedImage>,
   inline_text_atlas: Option<oxide_benchmark_spec::InlineTextAtlas>,
}

fn trace_event(op: TraceOperation, target: Option<&str>, value: Option<TraceValue>, state_id: Option<&str>, x_millionths: Option<i32>, y_millionths: Option<i32>) -> TraceEvent
{
   TraceEvent {
      at_us: 0,
      op,
      pointer: matches!(op, TraceOperation::PointerDown | TraceOperation::PointerMove | TraceOperation::PointerUp | TraceOperation::PointerCancel).then_some(1),
      x_millionths,
      y_millionths,
      delta_x_millionths: None,
      delta_y_millionths: None,
      target: target.map(String::from),
      value,
      state_id: state_id.map(String::from),
   }
}

fn rapid_events(scenario_id: &str) -> Result<Vec<TraceEvent>, JsValue>
{
   let mut events = Vec::new();
   match scenario_id
   {
      "startup.first-screen" =>
      {
         append_trace_until(&mut events, include_bytes!("../../../benchmarks/comparative/specs/v1/traces/startup-terminated-warm-cache.json"), u64::MAX)?;
         append_trace_until(&mut events, include_bytes!("../../../benchmarks/comparative/specs/v1/traces/startup-fresh-install-first-launch.json"), u64::MAX)?;
      }
      "dashboard.mixed-static" => append_trace_until(&mut events, include_bytes!("../../../benchmarks/comparative/specs/v1/traces/dashboard-leaf-updates.json"), 1_250_000)?,
      "feed.variable-scroll" =>
      {
         append_trace_until(&mut events, include_bytes!("../../../benchmarks/comparative/specs/v1/traces/feed-forward-fling.json"), u64::MAX)?;
         append_trace_until(&mut events, include_bytes!("../../../benchmarks/comparative/specs/v1/traces/feed-reverse-fling.json"), u64::MAX)?;
         append_trace_until(&mut events, include_bytes!("../../../benchmarks/comparative/specs/v1/traces/feed-favorite-one.json"), u64::MAX)?;
      }
      "chat.live-update" =>
      {
         append_trace_until(&mut events, include_bytes!("../../../benchmarks/comparative/specs/v1/traces/chat-prepend-50.json"), u64::MAX)?;
         append_trace_until(&mut events, include_bytes!("../../../benchmarks/comparative/specs/v1/traces/chat-append-10hz.json"), 1_900_000)?;
      }
      "navigation.modal" => append_trace_until(&mut events, include_bytes!("../../../benchmarks/comparative/specs/v1/traces/navigation-canonical-cycles.json"), 625_000)?,
      "image.decode-zoom" => append_trace_until(&mut events, include_bytes!("../../../benchmarks/comparative/specs/v1/traces/image-pan.json"), 1_000_000)?,
      _ => return Err(js_error(format!("unsupported comparison scenario {scenario_id}"))),
   }
   Ok(events)
}

fn rapid_checkpoint_matches(scenario_id: &str, checkpoint_id: &str) -> bool
{
   matches!(
      (scenario_id, checkpoint_id),
      ("startup.first-screen", "fresh-install-ready")
         | ("dashboard.mixed-static", "leaf-updated")
         | ("feed.variable-scroll", "favorite-applied")
         | ("chat.live-update", "append-settled")
         | ("navigation.modal", "modal-100")
         | ("image.decode-zoom", "first-visible" | "pan-mid")
   )
}

fn append_trace_until(output: &mut Vec<TraceEvent>, bytes: &[u8], at_us: u64) -> Result<(), JsValue>
{
   let events: Vec<TraceEvent> = serde_json::from_slice(bytes).map_err(|error| js_error(format!("parsing rapid comparison trace: {error}")))?;
   output.extend(events.into_iter().filter(|event| event.at_us <= at_us));
   Ok(())
}

async fn load_image_source(state: &Rc<RefCell<Runtime>>) -> Result<(), JsValue>
{
   let fetch_start_ms = performance_now();
   let window = web_sys::window().ok_or_else(|| js_error("window is unavailable"))?;
   let response = JsFuture::from(window.fetch_with_str("/specs/assets/image-decode-zoom-source-v1.png")).await?.dyn_into::<Response>().map_err(|_| js_error("image source fetch did not return a response"))?;
   if !response.ok()
   {
      return Err(js_error(format!("image source request failed {}", response.status())));
   }
   let buffer = JsFuture::from(response.array_buffer()?).await?;
   let bytes = Uint8Array::new(&buffer).to_vec();
   let fetch_end_ms = performance_now();
   let decode_start_ms = fetch_end_ms;
   let decoded = decode_png_rgba(&bytes)?;
   let decode_end_ms = performance_now();
   let upload_start_ms = decode_end_ms;

   let mut runtime = state.borrow_mut();
   let handle = runtime.renderer.borrow_mut().image_create_rgba8(decoded.width, decoded.height, &decoded.rgba, decoded.width as usize * 4);
   if handle == gfx::ImageHandle(0)
   {
      return Err(js_error("WebGPU image source upload returned an invalid handle"));
   }
   let fixture: ImageDecodeZoomFixture = serde_json::from_slice(include_bytes!("../../../benchmarks/comparative/specs/v1/fixtures/image.decode-zoom.json")).map_err(|error| js_error(format!("parsing image fixture: {error}")))?;
   if let Err(error) = runtime.router.set_comparison_image_source_resource(handle, &fixture.source.artifact.sha256)
   {
      runtime.renderer.borrow_mut().image_release(handle);
      return Err(js_error(error));
   }
   runtime.resources.images.push(handle);
   for event in [
      trace_event(TraceOperation::ResourceArrival, Some("image:source-bytes"), None, Some("image:bytes-ready"), None, None),
      trace_event(TraceOperation::ResourceArrival, Some("image:decoded"), None, Some("image:decoded"), None, None),
      trace_event(TraceOperation::ResourceArrival, Some("image:texture"), None, Some("image:uploaded"), None, None),
      trace_event(TraceOperation::ResourceArrival, Some("image:presented"), None, Some("image:visible"), None, None),
   ]
   {
      runtime.router.apply_comparison_event(&event).map_err(js_error)?;
   }
   let upload_end_ms = performance_now();
   runtime.image_resource_timing = Some(json!({
      "source": "oxide-web-comparison-runtime",
      "encoded_bytes": bytes.len(),
      "decoded_rgba_bytes": decoded.rgba.len(),
      "fetch_ms": fetch_end_ms - fetch_start_ms,
      "decode_ms": decode_end_ms - decode_start_ms,
      "upload_ms": upload_end_ms - upload_start_ms,
   }));
   runtime.generation = runtime.generation.saturating_add(1);
   runtime.ready = false;
   runtime.render_at(performance_now())?;
   runtime.image_source_loading = false;
   Ok(())
}

fn embedded_scenario(id: &str) -> Result<EmbeddedScenario, JsValue>
{
   let (scenario, fixture) = match id
   {
      "startup.first-screen" => (include_bytes!("../../../benchmarks/comparative/specs/v1/scenarios/startup.first-screen.json").as_slice(), include_bytes!("../../../benchmarks/comparative/specs/v1/fixtures/startup.first-screen.json").as_slice()),
      "dashboard.mixed-static" => (include_bytes!("../../../benchmarks/comparative/specs/v1/scenarios/dashboard.mixed-static.json").as_slice(), include_bytes!("../../../benchmarks/comparative/specs/v1/fixtures/dashboard.mixed-static.json").as_slice()),
      "feed.variable-scroll" => (include_bytes!("../../../benchmarks/comparative/specs/v1/scenarios/feed.variable-scroll.json").as_slice(), include_bytes!("../../../benchmarks/comparative/specs/v1/fixtures/feed.variable-scroll.json").as_slice()),
      "chat.live-update" => (include_bytes!("../../../benchmarks/comparative/specs/v1/scenarios/chat.live-update.json").as_slice(), include_bytes!("../../../benchmarks/comparative/specs/v1/fixtures/chat.live-update.json").as_slice()),
      "navigation.modal" => (include_bytes!("../../../benchmarks/comparative/specs/v1/scenarios/navigation.modal.json").as_slice(), include_bytes!("../../../benchmarks/comparative/specs/v1/fixtures/navigation.modal.json").as_slice()),
      "image.decode-zoom" => (include_bytes!("../../../benchmarks/comparative/specs/v1/scenarios/image.decode-zoom.json").as_slice(), include_bytes!("../../../benchmarks/comparative/specs/v1/fixtures/image.decode-zoom.json").as_slice()),
      _ => return Err(js_error(format!("unsupported comparison scenario {id}"))),
   };
   let scenario = serde_json::from_slice(scenario).map_err(|error| js_error(format!("parsing scenario {id}: {error}")))?;
   Ok(EmbeddedScenario {scenario, fixture})
}

fn load_fonts() -> Result<[text::Font; 3], JsValue>
{
   let manifest: FontPackManifest = serde_json::from_slice(FONT_MANIFEST).map_err(|error| js_error(format!("parsing comparison font manifest: {error}")))?;
   let load = |role: &str, bytes: &[u8]| -> Result<text::Font, JsValue>
   {
      let _ = manifest.fonts.iter().find(|font| font.role == role).ok_or_else(|| js_error(format!("comparison font manifest has no {role} font")))?;
      Ok(text::Font::from_bytes(bytes.to_vec()))
   };
   Ok([load("latin", LATIN_FONT)?, load("arabic", ARABIC_FONT)?, load("cjk-simplified", CJK_FONT)?])
}

fn decode_resources(scenario_id: &str) -> Result<DecodedResources, JsValue>
{
   if scenario_id == "image.decode-zoom"
   {
      return Ok(DecodedResources {
         images: vec![decode_png_rgba(IMAGE_THUMBNAIL)?],
         inline_text_atlas: None,
      });
   }
   let manifest: AssetManifest = serde_json::from_slice(NEUTRAL_ASSET_MANIFEST).map_err(|error| js_error(format!("parsing neutral asset manifest: {error}")))?;
   let inline_text_atlas = manifest.inline_text_atlas.ok_or_else(|| js_error("neutral asset manifest has no inline-text atlas"))?;
   Ok(DecodedResources {
      images: vec![
         decode_png_rgba(THUMBNAIL_ATLAS)?,
         decode_png_rgba(INLINE_TEXT_ATLAS_128)?,
         decode_png_rgba(INLINE_TEXT_ATLAS_45)?,
         decode_png_rgba(INLINE_TEXT_ATLAS_39)?,
      ],
      inline_text_atlas: Some(inline_text_atlas),
   })
}

fn upload_resources(renderer: &Rc<RefCell<BrowserRenderer>>, decoded: &DecodedResources) -> Result<ScenarioResources, JsValue>
{
   let mut resources = ScenarioResources {images: Vec::with_capacity(decoded.images.len())};
   let mut renderer = renderer.borrow_mut();
   for image in &decoded.images
   {
      let handle = renderer.image_create_rgba8(image.width, image.height, &image.rgba, image.width as usize * 4);
      if handle == gfx::ImageHandle(0)
      {
         for handle in resources.images.drain(..)
         {
            renderer.image_release(handle);
         }
         return Err(js_error("WebGPU image upload returned an invalid handle"));
      }
      resources.images.push(handle);
   }
   Ok(resources)
}

fn release_resources(renderer: &Rc<RefCell<BrowserRenderer>>, resources: &mut ScenarioResources)
{
   let mut renderer = renderer.borrow_mut();
   for handle in resources.images.drain(..)
   {
      renderer.image_release(handle);
   }
}

fn configure_resources(router: &mut scenes::Router<WebUploader>, scenario_id: &str, fonts: [usize; 3], resources: &ScenarioResources, inline_text_atlas: Option<oxide_benchmark_spec::InlineTextAtlas>) -> Result<(), String>
{
   if scenario_id == "image.decode-zoom"
   {
      let thumbnail = resources.images.first().copied().ok_or_else(|| String::from("image scenario thumbnail is absent"))?;
      let fixture: ImageDecodeZoomFixture = serde_json::from_slice(include_bytes!("../../../benchmarks/comparative/specs/v1/fixtures/image.decode-zoom.json")).map_err(|error| error.to_string())?;
      router.set_comparison_fonts(fonts)?;
      return router.set_comparison_image_thumbnail_resource(thumbnail, &fixture.thumbnail.artifact.sha256);
   }
   let atlas = resources.images.first().copied().ok_or_else(|| String::from("comparison thumbnail atlas is absent"))?;
   let inline_images = resources.images.get(1..).ok_or_else(|| String::from("comparison inline-text images are absent"))?.to_vec();
   router.set_comparison_resources(atlas, fonts)?;
   router.set_comparison_inline_text_resources(inline_images, inline_text_atlas.ok_or_else(|| String::from("comparison inline-text contract is absent"))?)
}

fn decode_png_rgba(bytes: &[u8]) -> Result<DecodedImage, JsValue>
{
   let mut decoder = png::Decoder::new(bytes);
   decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
   let mut reader = decoder.read_info().map_err(|error| js_error(format!("reading embedded PNG header: {error}")))?;
   let mut buffer = vec![0; reader.output_buffer_size()];
   let info = reader.next_frame(&mut buffer).map_err(|error| js_error(format!("decoding embedded PNG: {error}")))?;
   let bytes = &buffer[..info.buffer_size()];
   let rgba = match info.color_type
   {
      png::ColorType::Rgba => bytes.to_vec(),
      png::ColorType::Rgb => bytes.chunks_exact(3).flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255]).collect(),
      png::ColorType::Grayscale => bytes.iter().flat_map(|gray| [*gray, *gray, *gray, 255]).collect(),
      png::ColorType::GrayscaleAlpha => bytes.chunks_exact(2).flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]]).collect(),
      png::ColorType::Indexed => return Err(js_error("embedded PNG remained indexed after expansion")),
   };
   Ok(DecodedImage {width: info.width, height: info.height, rgba})
}

fn renderer_counters(stats: WebRendererStats) -> Value
{
   let memory = wasm_bindgen::memory().unchecked_into::<js_sys::WebAssembly::Memory>();
   let wasm_committed_bytes = Uint8Array::new(&memory.buffer()).byte_length() as u64;
   json!({
      "frame_id": stats.frame_id,
      "draws": stats.draws,
      "draw_items": stats.draw_items,
      "draw_items_coalesced": stats.draw_items_coalesced,
      "render_passes": stats.render_passes,
      "command_buffers": stats.command_buffers,
      "actual_submissions": stats.actual_submissions,
      "skipped_submissions": stats.skipped_submissions,
      "buffer_upload_bytes": stats.buffer_upload_bytes,
      "texture_upload_bytes": stats.texture_upload_bytes,
      "texture_creates": stats.texture_creates,
      "pipeline_creates": stats.pipeline_creates,
      "bind_group_creates": stats.bind_group_creates,
      "glyph_quads": stats.glyph_quads,
      "image_draws": stats.image_draws,
      "rrect_instances": stats.rrect_instances,
      "shaded_damage_pixels": stats.shaded_damage_pixels,
      "gpu_logical_total_bytes": stats.gpu_logical_total_bytes,
      "gpu_allocated_total_bytes": stats.gpu_allocated_total_bytes,
      "wasm_committed_bytes": wasm_committed_bytes,
      "wasm_committed_pages": wasm_committed_bytes / 65_536,
   })
}

fn timestamp_diagnostics(stats: WebRendererStats) -> Value
{
   json!({
      "source": "webgpu-timestamp-query",
      "supported": stats.gpu_timestamp_query_supported,
      "frame_id": stats.gpu_timestamp_frame_id,
      "passes": stats.gpu_timestamp_passes,
      "total_ns": stats.gpu_timestamp_total_ns,
      "draw_ns": stats.gpu_timestamp_draw_ns,
      "clear_ns": stats.gpu_timestamp_clear_ns,
      "backdrop_copy_ns": stats.gpu_timestamp_backdrop_copy_ns,
      "present_ns": stats.gpu_timestamp_present_ns,
      "max_pass_ns": stats.gpu_timestamp_max_pass_ns,
      "readback_skips": stats.gpu_timestamp_readback_skips,
      "readback_interval": stats.gpu_timestamp_readback_interval,
   })
}

fn capabilities_value(stats: WebRendererStats) -> Value
{
   json!({
      "implementation_id": "oxide.production",
      "renderer": "webgpu",
      "scenarios": SCENARIOS,
      "trusted_input_required": true,
      "semantic_accessibility": true,
      "target_geometry": {
         "width": CSS_WIDTH,
         "height": CSS_HEIGHT,
         "scale": DEVICE_SCALE,
         "physical_width": PHYSICAL_WIDTH,
         "physical_height": PHYSICAL_HEIGHT,
      },
      "gpu_pass_timestamps": {
         "source": "webgpu-timestamp-query",
         "supported": stats.gpu_timestamp_query_supported,
      },
   })
}

fn install_pointer_listeners(state: &Rc<RefCell<Runtime>>) -> Result<Vec<PointerListener>, JsValue>
{
   let canvas = state.borrow().canvas.clone();
   let target: &EventTarget = canvas.unchecked_ref();
   let mut listeners: Vec<PointerListener> = Vec::with_capacity(4);
   for name in ["pointerdown", "pointermove", "pointerup", "pointercancel"]
   {
      let state_for_event = Rc::clone(state);
      let callback = Closure::wrap(Box::new(move |event: PointerEvent|
      {
         let start_image_load = {
            let mut runtime = state_for_event.borrow_mut();
            runtime.pointer_event(&event);
            let start = name == "pointerup" && runtime.scenario_id.as_deref() == Some("image.decode-zoom") && !runtime.image_source_loading;
            if start
            {
               runtime.image_source_loading = true;
            }
            start
         };
         if start_image_load
         {
            let state_for_load = Rc::clone(&state_for_event);
            spawn_local(async move {
               if let Err(error) = load_image_source(&state_for_load).await
               {
                  let mut runtime = state_for_load.borrow_mut();
                  runtime.image_source_loading = false;
                  runtime.last_error = error.as_string().or_else(|| Some(String::from("image source load failed")));
               }
            });
         }
      }) as Box<dyn FnMut(PointerEvent)>);
      if let Err(error) = target.add_event_listener_with_callback(name, callback.as_ref().unchecked_ref())
      {
         for listener in listeners.drain(..)
         {
            let _ = target.remove_event_listener_with_callback(listener.name, listener.callback.as_ref().unchecked_ref());
         }
         return Err(error);
      }
      listeners.push(PointerListener {name, callback});
   }
   Ok(listeners)
}

fn install_keyboard_listener(state: &Rc<RefCell<Runtime>>) -> Result<KeyboardListener, JsValue>
{
   let canvas = state.borrow().canvas.clone();
   let target: &EventTarget = canvas.unchecked_ref();
   let state_for_event = Rc::clone(state);
   let callback = Closure::wrap(Box::new(move |event: KeyboardEvent| state_for_event.borrow_mut().keyboard_event(&event)) as Box<dyn FnMut(KeyboardEvent)>);
   target.add_event_listener_with_callback("keydown", callback.as_ref().unchecked_ref())?;
   Ok(KeyboardListener {callback})
}

fn remove_pointer_listeners(runtime: &mut Runtime)
{
   let target: &EventTarget = runtime.canvas.unchecked_ref();
   for listener in runtime.listeners.drain(..)
   {
      let _ = target.remove_event_listener_with_callback(listener.name, listener.callback.as_ref().unchecked_ref());
   }
   if let Some(listener) = runtime.keyboard_listener.take()
   {
      let _ = target.remove_event_listener_with_callback("keydown", listener.callback.as_ref().unchecked_ref());
   }
   runtime.started = false;
}

async fn wait_until_ready(state: &Rc<RefCell<Runtime>>) -> Result<(), JsValue>
{
   let (renderer, target_frame_id) = {
      let state = state.borrow();
      if state.scenario_id.is_none()
      {
         return Err(js_error("no comparison scenario is prepared"));
      }
      let renderer = Rc::clone(&state.renderer);
      let target_frame_id = renderer.borrow().last_stats().frame_id;
      (renderer, target_frame_id)
   };
   let completed = renderer.borrow().queue_completion_flag_for_benchmark();
   for _ in 0..TIMESTAMP_SETTLE_RAFS
   {
      wait_animation_frame().await?;
      let stats = renderer.borrow_mut().collect_timestamp_readbacks();
      let timestamps_ready = !stats.gpu_timestamp_query_supported || (stats.gpu_timestamp_frame_id >= target_frame_id && stats.gpu_timestamp_passes > 0 && renderer.borrow().pending_timestamp_readbacks() == 0);
      if completed.load(std::sync::atomic::Ordering::Acquire) && timestamps_ready
      {
         state.borrow_mut().ready = true;
         return Ok(());
      }
   }
   Err(js_error(format!("WebGPU frame {target_frame_id} did not settle within {TIMESTAMP_SETTLE_RAFS} animation frames")))
}

async fn wait_until_resources_ready(state: &Rc<RefCell<Runtime>>) -> Result<(), JsValue>
{
   for _ in 0..RESOURCE_SETTLE_RAFS
   {
      let state_ref = state.borrow();
      if let Some(error) = state_ref.last_error.as_ref()
      {
         return Err(js_error(error));
      }
      if !state_ref.image_source_loading
      {
         return Ok(());
      }
      drop(state_ref);
      wait_animation_frame().await?;
   }
   Err(js_error(format!("image source did not settle within {RESOURCE_SETTLE_RAFS} animation frames")))
}

async fn wait_animation_frame() -> Result<(), JsValue>
{
   let promise = Promise::new(&mut |resolve, reject| {
      let Some(window) = web_sys::window() else
      {
         let _ = reject.call1(&JsValue::UNDEFINED, &js_error("window is unavailable"));
         return;
      };
      let callback = Closure::once_into_js(move |_timestamp_ms: f64| {
         let _ = resolve.call0(&JsValue::UNDEFINED);
      });
      let Some(function) = callback.dyn_ref::<js_sys::Function>() else
      {
         let _ = reject.call1(&JsValue::UNDEFINED, &js_error("animation-frame callback is unavailable"));
         return;
      };
      if let Err(error) = window.request_animation_frame(function)
      {
         let _ = reject.call1(&JsValue::UNDEFINED, &error);
      }
   });
   JsFuture::from(promise).await.map(|_| ())
}

fn canvas_by_id(id: &str) -> Result<HtmlCanvasElement, JsValue>
{
   let document = web_sys::window().and_then(|window| window.document()).ok_or_else(|| js_error("document is unavailable"))?;
   document.get_element_by_id(id).ok_or_else(|| js_error(format!("canvas id {id} was not found")))?.dyn_into::<HtmlCanvasElement>().map_err(|_| js_error(format!("element {id} is not a canvas")))
}

fn performance_now() -> f64
{
   web_sys::window().and_then(|window| window.performance()).map_or(0.0, |performance| performance.now())
}

fn render_error(error: gfx::RenderError) -> JsValue
{
   js_error(error.to_string())
}

fn js_error(message: impl AsRef<str>) -> JsValue
{
   JsValue::from_str(message.as_ref())
}

fn json_string(value: &Value) -> Result<String, JsValue>
{
   serde_json::to_string(value).map_err(|error| js_error(format!("serializing comparison response: {error}")))
}

#[wasm_bindgen]
pub struct OxideComparisonApp
{
   state: Rc<RefCell<Runtime>>,
}

#[wasm_bindgen]
impl OxideComparisonApp
{
   #[wasm_bindgen(js_name = newAsync)]
   pub async fn new_async(canvas_id: &str) -> Result<OxideComparisonApp, JsValue>
   {
      let canvas = canvas_by_id(canvas_id)?;
      canvas.set_width(PHYSICAL_WIDTH);
      canvas.set_height(PHYSICAL_HEIGHT);
      let mut renderer = BrowserRenderer::from_canvas_webgpu(canvas.clone()).await.map_err(render_error)?;
      renderer.resize(PHYSICAL_WIDTH, PHYSICAL_HEIGHT, DEVICE_SCALE).map_err(render_error)?;
      renderer.set_timestamp_readback_interval_for_benchmark(1);
      let renderer = Rc::new(RefCell::new(renderer));
      let router = scenes::Router::new(WebUploader {renderer: Rc::clone(&renderer)});
      Ok(OxideComparisonApp {
         state: Rc::new(RefCell::new(Runtime {
            canvas,
            renderer,
            router,
            builder: ui::DrawListBuilder::new(),
            coalesced: Vec::new(),
            damage_rects: Vec::new(),
            resources: ScenarioResources::default(),
            scenario_id: None,
            fixture_id: None,
            seed: 0,
            generation: 0,
            ready: false,
            started: false,
            last_timestamp_ms: 0.0,
            last_pointer: None,
            last_error: None,
            rapid_events: Vec::new(),
            listeners: Vec::new(),
            keyboard_listener: None,
            image_source_loading: false,
            image_resource_timing: None,
         })),
      })
   }

   pub fn capabilities(&self) -> Result<String, JsValue>
   {
      let stats = self.state.borrow().renderer.borrow().last_stats();
      json_string(&capabilities_value(stats))
   }

   pub fn reset(&self, scenario_id: &str, seed: u64, generation: u32) -> Result<(), JsValue>
   {
      self.state.borrow_mut().reset(scenario_id, seed, generation)
   }

   pub async fn advance(&self, checkpoint_id: &str) -> Result<(), JsValue>
   {
      let (scenario_id, load_source) = {
         let mut runtime = self.state.borrow_mut();
         let scenario_id = runtime.scenario_id.clone().ok_or_else(|| js_error("no comparison scenario is prepared"))?;
         if !rapid_checkpoint_matches(&scenario_id, checkpoint_id)
         {
            return Err(js_error(format!("unsupported rapid checkpoint {scenario_id}/{checkpoint_id}")));
         }
         let load_source = scenario_id == "image.decode-zoom" && runtime.image_resource_timing.is_none();
         if load_source
         {
            runtime.image_source_loading = true;
         }
         (scenario_id, load_source)
      };

      if load_source
      {
         if let Err(error) = load_image_source(&self.state).await
         {
            self.state.borrow_mut().image_source_loading = false;
            return Err(error);
         }
      }
      if checkpoint_id == "first-visible"
      {
         return Ok(());
      }

      let mut runtime = self.state.borrow_mut();
      {
         let Runtime {router, rapid_events, ..} = &mut *runtime;
         for event in rapid_events.iter()
         {
            router.apply_comparison_event(event).map_err(js_error)?;
         }
      }
      runtime.generation = runtime.generation.saturating_add(1);
      runtime.ready = false;
      let timestamp_ms = if scenario_id == "navigation.modal" {runtime.last_timestamp_ms + 300.0} else {performance_now()};
      runtime.render_at(timestamp_ms)
   }

   pub async fn ready(&self) -> Result<(), JsValue>
   {
      wait_until_resources_ready(&self.state).await?;
      wait_until_ready(&self.state).await
   }

   pub fn snapshot(&self) -> Result<String, JsValue>
   {
      let value = self.state.borrow_mut().snapshot_value()?;
      json_string(&value)
   }

   pub fn checkpoint(&self, checkpoint_id: &str) -> Result<String, JsValue>
   {
      let value = self.state.borrow().checkpoint_value(checkpoint_id)?;
      json_string(&value)
   }

   pub fn teardown(&self)
   {
      let mut state = self.state.borrow_mut();
      remove_pointer_listeners(&mut state);
      state.clear_scenario();
      state.canvas.set_width(PHYSICAL_WIDTH);
      state.canvas.set_height(PHYSICAL_HEIGHT);
   }

   pub fn start(&self) -> Result<(), JsValue>
   {
      if self.state.borrow().started
      {
         return Ok(());
      }
      let listeners = install_pointer_listeners(&self.state)?;
      let keyboard_listener = match install_keyboard_listener(&self.state)
      {
         Ok(listener) => listener,
         Err(error) =>
         {
            let mut state = self.state.borrow_mut();
            state.listeners = listeners;
            remove_pointer_listeners(&mut state);
            return Err(error);
         }
      };
      let mut state = self.state.borrow_mut();
      state.listeners = listeners;
      state.keyboard_listener = Some(keyboard_listener);
      state.started = true;
      Ok(())
   }
}
