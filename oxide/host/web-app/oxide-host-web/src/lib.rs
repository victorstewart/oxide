//! Oxide WebAssembly browser host.

#![forbid(unsafe_code)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

/// Generates the default checkerboard texture used by the web demo host.
#[must_use]
pub fn generate_checker_rgba(width: u32, height: u32) -> Vec<u8> {
    let mut rgba = vec![0_u8; (width as usize).saturating_mul(height as usize).saturating_mul(4)];
    for y in 0..height {
        for x in 0..width {
            let idx = ((y as usize).saturating_mul(width as usize).saturating_add(x as usize))
                .saturating_mul(4);
            let tile = ((x / 24) + (y / 24)) % 2 == 0;
            let (r, g, b) = if tile { (42_u8, 122_u8, 255_u8) } else { (245_u8, 248_u8, 252_u8) };
            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = 255;
        }
    }
    rgba
}

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static OXIDE_WASM_ALLOCATOR: oxide_wasm_alloc_counter::CountingAllocator<std::alloc::System> =
    oxide_wasm_alloc_counter::CountingAllocator::new(std::alloc::System);

#[cfg(target_arch = "wasm32")]
mod wasm_host {
    use super::generate_checker_rgba;
    use oxide_wasm_alloc_counter::AllocationSnapshot;
    use oxide_platform_api as platform_api;
    use oxide_platform_api::Platform;
    use oxide_renderer_api as gfx;
    use oxide_renderer_api::Renderer;
    use oxide_renderer_web::{
        id_mask_compositor, neon_marker, scene3d, BrowserRenderer, WebRendererStats,
    };
    use oxide_test_scenes as scenes;
    use oxide_text as text;
    use oxide_ui_core as ui;
    use std::cell::RefCell;
    use std::fmt::Write;
    use std::rc::Rc;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::{closure::Closure, JsCast};
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{
        CompositionEvent, Event, EventTarget, HtmlCanvasElement, HtmlElement, HtmlTextAreaElement,
        InputEvent, KeyboardEvent, PointerEvent, WheelEvent,
    };

    const DEFAULT_FONT_BYTES: &[u8] =
        include_bytes!("../../../../crates/ui-core/assets/Asap-Regular.ttf");
    const WEBGPU_ID_MASK_CELLS: usize = 40;
    const WEBGPU_ID_MASK_EXTENT: f32 = 512.0;
    const WEBGPU_UPLOAD_ATLAS_SIZE: u32 = 1024;
    const WEBGPU_UPLOAD_DIRTY_SIZE: u32 = 64;
    const WEBGPU_UPLOAD_IMAGE_SIZE: u32 = 256;
    const WEBGPU_UPLOAD_SCRATCH_UPDATES: u32 = 24;
    const WEBGPU_MIXED_GLYPHS: usize = 96;
    const WEBGPU_MIXED_IMAGE_TILES: usize = 96;
    const WEBGPU_MIXED_IMAGE_COLUMNS: usize = 12;
    const WEBGPU_LAYER_EFFECT_GLYPHS: usize = 72;
    const WEBGPU_LAYER_EFFECT_IMAGE_TILES: usize = 64;
    const WEBGPU_LAYER_EFFECT_IMAGE_COLUMNS: usize = 8;
    const WEBGPU_LAYER_EFFECT_BACKDROPS: usize = 4;
    const WEBGPU_EFFECT_UNIFORM_BACKDROPS: usize = 48;
    const WEBGPU_BACKDROP_BATCH_BACKDROPS: usize = 12;
    const WEBGPU_COMMAND_FAMILY_SDF_GLYPHS: usize = 36;
    const WEBGPU_COMMAND_FAMILY_SDF_RUNS: usize = 8;
    const WEBGPU_COMMAND_FAMILY_REPEATS: usize = 64;
    const WEBGPU_COMMAND_FAMILY_COLUMNS: usize = 8;
    const WEBGPU_DRAW_STATE_CACHE_DRAWS: usize = 1024;
    const WEBGPU_DRAW_STATE_CACHE_COLUMNS: usize = 32;
    const WEBGPU_CLIP_STATE_DRAWS: usize = 512;
    const WEBGPU_CLIP_STATE_RUNS: usize = 16;
    const WEBGPU_NEON_MARKERS: usize = 64;
    const WEBGPU_NEON_MARKER_COLUMNS: usize = 8;
    const WEBGPU_DIRECT_SURFACE_DRAWS: usize = 384;
    const WEBGPU_DIRECT_SURFACE_COLUMNS: usize = 24;
    const WEBGPU_SCENE3D_STRESS_INSTANCES: usize = 96;
    const WEBGPU_TIMESTAMP_SETTLE_RAFS: u32 = 60;

    struct WebUploader {
        renderer: Rc<RefCell<BrowserRenderer>>,
    }

    impl ui::elements::ImageUploader for WebUploader {
        fn create_a8(
            &mut self,
            width: u32,
            height: u32,
            data: &[u8],
            row_bytes: usize,
        ) -> gfx::ImageHandle {
            self.renderer.borrow_mut().image_create_a8(width, height, data, row_bytes)
        }

        fn update_a8(
            &mut self,
            handle: gfx::ImageHandle,
            x: u32,
            y: u32,
            width: u32,
            height: u32,
            data: &[u8],
            row_bytes: usize,
        ) {
            self.renderer
                .borrow_mut()
                .image_update_a8(handle, x, y, width, height, data, row_bytes);
        }
    }

    struct AppState {
        canvas: HtmlCanvasElement,
        ime_textarea: HtmlTextAreaElement,
        renderer: Rc<RefCell<BrowserRenderer>>,
        router: scenes::Router<WebUploader>,
        builder: ui::DrawListBuilder,
        damage_rects: Vec<gfx::RectI>,
        coalesce_items: Vec<gfx::DrawCmd>,
        bench_resources: Option<WebGpuUploadBenchResources>,
        last_ms: u64,
        ime_focused: bool,
        ime_composing: bool,
        ime_skip_next_input: bool,
        raf: Option<Closure<dyn FnMut(f64)>>,
        raf_pending: bool,
        listeners: Vec<Closure<dyn FnMut(Event)>>,
        frame_dirty: bool,
        settle_frames_remaining: u8,
        idle_skipped_frames: u64,
        submitted_frames: u64,
        direct_capture_active: bool,
    }

    const IDLE_SETTLE_FRAMES: u8 = 2;

    impl AppState {
        fn frame_at(&mut self, timestamp_ms: f64) -> Result<(), JsValue> {
            self.frame_at_inner(timestamp_ms, None)
        }

        fn frame_at_profiled(
            &mut self,
            timestamp_ms: f64,
            allocation_stages: &mut WebGpuFrameStageAllocationSummary,
        ) -> Result<(), JsValue> {
            self.frame_at_inner(timestamp_ms, Some(allocation_stages))
        }

        fn frame_at_inner(
            &mut self,
            timestamp_ms: f64,
            mut allocation_stages: Option<&mut WebGpuFrameStageAllocationSummary>,
        ) -> Result<(), JsValue> {
            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            let (physical_w, physical_h, scale) = canvas_backing_size(&self.canvas);
            self.renderer.borrow_mut().resize(physical_w, physical_h, scale).map_err(render_err)?;
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::CanvasResize,
                stage_before,
            );

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            let now_ms = timestamp_ms.max(0.0).round() as u64;
            let dt_ms = if self.last_ms == 0 {
                16
            } else {
                now_ms.saturating_sub(self.last_ms).min(u32::MAX as u64) as u32
            };
            self.last_ms = now_ms;
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::FrameTiming,
                stage_before,
            );

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            self.builder.clear();
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::BuilderClear,
                stage_before,
            );
            let viewport = gfx::RectF::new(
                0.0,
                0.0,
                physical_w as f32 / scale.max(1.0),
                physical_h as f32 / scale.max(1.0),
            );

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            self.router.update(now_ms, dt_ms);
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::RouterUpdate,
                stage_before,
            );

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            self.router.draw(viewport, scale, &mut self.builder);
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::RouterDraw,
                stage_before,
            );

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            self.router.take_damage_into(&mut self.damage_rects);
            let mut damage = gfx::Damage { rects: core::mem::take(&mut self.damage_rects) };
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::DamageHandoff,
                stage_before,
            );

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            ui::coalesce_adjacent_draws_reuse(
                self.builder.drawlist_mut(),
                &mut self.coalesce_items,
            );
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::DrawCoalesce,
                stage_before,
            );

            let mut renderer = self.renderer.borrow_mut();
            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            let token = renderer.begin_frame(&gfx::FrameTarget, Some(&damage));
            self.damage_rects = core::mem::take(&mut damage.rects);
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::BeginFrame,
                stage_before,
            );

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            renderer.encode_pass(self.builder.drawlist());
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::EncodePass,
                stage_before,
            );

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            renderer.submit(token).map_err(render_err)?;
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::Submit,
                stage_before,
            );

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            self.submitted_frames = self.submitted_frames.saturating_add(1);
            if self.settle_frames_remaining > 0 {
                self.settle_frames_remaining -= 1;
            }
            self.frame_dirty = false;
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::PostSubmit,
                stage_before,
            );
            Ok(())
        }

        fn mark_frame_dirty(&mut self) {
            self.frame_dirty = true;
            self.settle_frames_remaining = IDLE_SETTLE_FRAMES;
        }

        fn should_request_next_frame(&self) -> bool {
            self.frame_dirty || self.settle_frames_remaining > 0 || self.router.wants_next_frame()
        }

        fn last_draw_count(&self) -> u32 {
            self.renderer.borrow().last_stats().draws
        }

        fn renderer_backend(&self) -> &'static str {
            self.renderer.borrow().backend_name()
        }
    }

    /// JavaScript-visible Oxide browser host.
    #[wasm_bindgen]
    pub struct OxideWebApp {
        state: Rc<RefCell<AppState>>,
    }

    #[wasm_bindgen]
    impl OxideWebApp {
        #[wasm_bindgen(constructor)]
        pub fn new(canvas_id: &str) -> Result<OxideWebApp, JsValue> {
            let _ = canvas_id;
            Err(render_err(gfx::RenderError::Unsupported(
                "webgpu renderer requires async browser initialization",
            )))
        }

        #[wasm_bindgen(js_name = newAsync)]
        pub async fn new_async(canvas_id: &str) -> Result<OxideWebApp, JsValue> {
            let _platform = oxide_platform_web::install_current_platform();
            let renderer =
                BrowserRenderer::from_canvas_id_webgpu(canvas_id).await.map_err(render_err)?;
            Self::new_with_renderer(renderer)
        }

        fn new_with_renderer(mut renderer: BrowserRenderer) -> Result<OxideWebApp, JsValue> {
            let canvas = renderer.canvas();
            let ime_textarea = create_ime_textarea(&canvas)?;
            let (physical_w, physical_h, scale) = canvas_backing_size(&canvas);
            renderer.resize(physical_w, physical_h, scale).map_err(render_err)?;

            let renderer = Rc::new(RefCell::new(renderer));
            let uploader = WebUploader { renderer: Rc::clone(&renderer) };
            let mut router = scenes::Router::new(uploader);
            let _font_id =
                router.text.fonts.add_font(text::Font::from_bytes(DEFAULT_FONT_BYTES.to_vec()));

            let (image_w, image_h) = (256_u32, 256_u32);
            let checker = generate_checker_rgba(image_w, image_h);
            let image = renderer.borrow_mut().image_create_rgba8(
                image_w,
                image_h,
                &checker,
                (image_w as usize).saturating_mul(4),
            );
            router.set_zoom_image(image, image_w, image_h);

            let state = Rc::new(RefCell::new(AppState {
                canvas,
                ime_textarea,
                renderer,
                router,
                builder: ui::DrawListBuilder::new(),
                damage_rects: Vec::new(),
                coalesce_items: Vec::new(),
                bench_resources: None,
                last_ms: 0,
                ime_focused: false,
                ime_composing: false,
                ime_skip_next_input: false,
                raf: None,
                raf_pending: false,
                listeners: Vec::new(),
                frame_dirty: true,
                settle_frames_remaining: IDLE_SETTLE_FRAMES,
                idle_skipped_frames: 0,
                submitted_frames: 0,
                direct_capture_active: false,
            }));
            install_event_listeners(&state)?;
            Ok(OxideWebApp { state })
        }

        pub fn start(&self) -> Result<(), JsValue> {
            if self.state.borrow().raf.is_some() {
                return Ok(());
            }
            let state_for_frame = Rc::clone(&self.state);
            let closure = Closure::wrap(Box::new(move |timestamp_ms: f64| {
                {
                    let mut state = state_for_frame.borrow_mut();
                    state.raf_pending = false;
                    let _ = state.frame_at(timestamp_ms);
                }
                if state_for_frame.borrow().should_request_next_frame() {
                    request_next_frame(&state_for_frame);
                } else {
                    let mut state = state_for_frame.borrow_mut();
                    state.idle_skipped_frames = state.idle_skipped_frames.saturating_add(1);
                }
            }) as Box<dyn FnMut(f64)>);
            self.state.borrow_mut().raf = Some(closure);
            request_next_frame(&self.state);
            Ok(())
        }

        pub fn prewarm_webgpu_bench_resources(&self) -> Result<(), JsValue> {
            self.ensure_upload_bench_resources().map(|_| ())
        }

        pub fn frame(&self) -> Result<(), JsValue> {
            self.state.borrow_mut().frame_at(perf_now())
        }

        pub fn render_webgpu_app_snapshot(&self) -> Result<String, JsValue> {
            const SNAPSHOT_TIMESTAMP_MS: f64 = 1_000.0;
            self.state.borrow_mut().frame_at(SNAPSHOT_TIMESTAMP_MS)?;
            let draws = self.state.borrow().last_draw_count();
            Ok(format!("timestamp_ms={SNAPSHOT_TIMESTAMP_MS:.3};draws={draws}"))
        }

        pub fn render_webgpu_scene3d_snapshot(
            &self,
            width: u32,
            height: u32,
        ) -> Result<String, JsValue> {
            let (renderer, physical_w, physical_h) = {
                let mut state = self.state.borrow_mut();
                state.direct_capture_active = true;
                (state.renderer.clone(), width.max(1), height.max(1))
            };
            let mut renderer = renderer.borrow_mut();
            webgpu_scene3d_frame(&mut renderer, physical_w, physical_h, 1.0)?;
            let stats = renderer.last_stats();
            Ok(format!(
                "meshes=2;instances=2;draws={};frame_id={};width={};height={}",
                stats.draws, stats.frame_id, stats.width, stats.height,
            ))
        }

        pub fn bench_frames(&self, frames: u32) -> Result<String, JsValue> {
            let frame_count = frames.clamp(1, 600);
            let start = perf_now();
            for frame in 0..frame_count {
                self.state.borrow_mut().frame_at(start + frame as f64 * 16.666_667)?;
            }
            let total_ms = (perf_now() - start).max(0.0);
            let avg_ms = total_ms / frame_count as f64;
            let stats = self.state.borrow().renderer.borrow().last_stats();
            let backend_stats = renderer_stats_metrics(stats, "");
            Ok(format!(
                "frames={frame_count};total_ms={total_ms:.3};avg_ms={avg_ms:.3}{backend_stats}",
            ))
        }

        pub async fn bench_frame_samples(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.state.borrow().renderer.clone();
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut values = Vec::with_capacity(sample_count.saturating_mul(frames) as usize);
            let mut timestamp = perf_now();
            for _warmup in 0..4 {
                self.state.borrow_mut().frame_at(timestamp)?;
                timestamp += 16.666_667;
            }

            let mut allocations = WebGpuAllocationSummary::default();
            let mut allocation_stages = WebGpuFrameStageAllocationSummary::default();
            for _sample in 0..sample_count {
                for _frame in 0..frames {
                    let start = perf_now();
                    let alloc_before = oxide_wasm_alloc_counter::snapshot();
                    self.state
                        .borrow_mut()
                        .frame_at_profiled(timestamp, &mut allocation_stages)?;
                    let alloc_after = oxide_wasm_alloc_counter::snapshot();
                    add_allocation_frame(&mut allocations, alloc_before, alloc_after);
                    values.push((perf_now() - start).max(0.0));
                    timestamp += 16.666_667;
                }
            }

            values.sort_by(|a, b| a.total_cmp(b));
            let total_frames = sample_count.saturating_mul(frames);
            let avg_ms = average(&values);
            let p50_ms = percentile(&values, 0.50);
            let p95_ms = percentile(&values, 0.95);
            let p99_ms = percentile(&values, 0.99);
            let peak_ms = values.last().copied().unwrap_or(0.0);
            let stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let pacing = frame_pacing_metrics(&values, "");
            let allocations = allocation_metrics(&allocations, "");
            let allocation_stages = frame_stage_allocation_metrics(&allocation_stages);
            let backend_stats = renderer_stats_metrics(stats, "");
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames};frames={total_frames};p50_ms={p50_ms:.3};p95_ms={p95_ms:.3};p99_ms={p99_ms:.3};peak_ms={peak_ms:.3};avg_ms={avg_ms:.3}{backend_stats}{pacing}{allocations}{allocation_stages}",
            ))
        }

        pub async fn bench_webgpu_id_mask_ab(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.state.borrow().renderer.clone();
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut current = {
                let mut renderer = renderer.borrow_mut();
                bench_webgpu_id_mask_case(&mut renderer, true, sample_count, frames)?
            };
            current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut legacy = {
                let mut renderer = renderer.borrow_mut();
                bench_webgpu_id_mask_case(&mut renderer, false, sample_count, frames)?
            };
            legacy.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let ratio = if current.p50_ms > 0.0 { legacy.p50_ms / current.p50_ms } else { 0.0 };
            let current_pacing = frame_pacing_metrics(&current.frame_values, "current");
            let legacy_pacing = frame_pacing_metrics(&legacy.frame_values, "legacy");
            let current_allocations = allocation_metrics(&current.allocations, "current");
            let legacy_allocations = allocation_metrics(&legacy.allocations, "legacy");
            let current_stats = renderer_stats_metrics(current.stats, "current");
            let legacy_stats = renderer_stats_metrics(legacy.stats, "legacy");
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames};current_p50_ms={:.3};current_p95_ms={:.3};current_p99_ms={:.3};current_peak_ms={:.3};current_avg_ms={:.3}{current_pacing}{current_allocations}{current_stats};legacy_p50_ms={:.3};legacy_p95_ms={:.3};legacy_p99_ms={:.3};legacy_peak_ms={:.3};legacy_avg_ms={:.3}{legacy_pacing}{legacy_allocations}{legacy_stats};legacy_over_current={ratio:.3};vertices={};vertex_bytes={}",
                current.p50_ms,
                current.p95_ms,
                current.p99_ms,
                current.peak_ms,
                current.avg_ms,
                legacy.p50_ms,
                legacy.p95_ms,
                legacy.p99_ms,
                legacy.peak_ms,
                legacy.avg_ms,
                current.vertices,
                current.vertex_bytes,
            ))
        }

        pub async fn bench_webgpu_upload_ab(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.ensure_upload_bench_resources()?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut glyph_current = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.glyph_frame(renderer, true)
                })
            })?;
            glyph_current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut glyph_legacy = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.glyph_frame(renderer, false)
                })
            })?;
            glyph_legacy.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut image_current = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.image_frame(renderer, true)
                })
            })?;
            image_current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut image_legacy = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.image_frame(renderer, false)
                })
            })?;
            image_legacy.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let glyph_ratio = if glyph_current.p50_ms > 0.0 {
                glyph_legacy.p50_ms / glyph_current.p50_ms
            } else {
                0.0
            };
            let image_ratio = if image_current.p50_ms > 0.0 {
                image_legacy.p50_ms / image_current.p50_ms
            } else {
                0.0
            };
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{}{}{};glyph_legacy_over_current={glyph_ratio:.3};image_legacy_over_current={image_ratio:.3};atlas_width={};atlas_height={};atlas_dirty_width={};atlas_dirty_height={};image_width={};image_height={};image_dirty_width={};image_dirty_height={}",
                sampled_case_metrics(&glyph_current, "glyph_current"),
                sampled_case_metrics(&glyph_legacy, "glyph_legacy"),
                sampled_case_metrics(&image_current, "image_current"),
                sampled_case_metrics(&image_legacy, "image_legacy"),
                WEBGPU_UPLOAD_ATLAS_SIZE,
                WEBGPU_UPLOAD_ATLAS_SIZE,
                WEBGPU_UPLOAD_DIRTY_SIZE,
                WEBGPU_UPLOAD_DIRTY_SIZE,
                WEBGPU_UPLOAD_IMAGE_SIZE,
                WEBGPU_UPLOAD_IMAGE_SIZE,
                WEBGPU_UPLOAD_DIRTY_SIZE,
                WEBGPU_UPLOAD_DIRTY_SIZE,
            ))
        }

        pub async fn bench_webgpu_upload_scratch_ab(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.ensure_upload_bench_resources()?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut current = self.with_upload_bench_resources(|renderer, resources| {
                renderer.set_image_upload_scratch_enabled_for_benchmark(true);
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.upload_scratch_frame(renderer)
                })
            })?;
            current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut legacy = self.with_upload_bench_resources(|renderer, resources| {
                renderer.set_image_upload_scratch_enabled_for_benchmark(false);
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.upload_scratch_frame(renderer)
                })
            })?;
            legacy.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            renderer
                .borrow_mut()
                .set_image_upload_scratch_enabled_for_benchmark(true);
            let ratio = if current.p50_ms > 0.0 { legacy.p50_ms / current.p50_ms } else { 0.0 };
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{};legacy_over_current={ratio:.3};updates={WEBGPU_UPLOAD_SCRATCH_UPDATES};atlas_dirty_width={WEBGPU_UPLOAD_DIRTY_SIZE};atlas_dirty_height={WEBGPU_UPLOAD_DIRTY_SIZE};image_dirty_width={WEBGPU_UPLOAD_DIRTY_SIZE};image_dirty_height={WEBGPU_UPLOAD_DIRTY_SIZE}",
                sampled_case_metrics(&current, "current"),
                sampled_case_metrics(&legacy, "legacy"),
            ))
        }

        pub async fn bench_webgpu_effect_uniform_ab(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 12);
            let renderer = self.ensure_upload_bench_resources()?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(false);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut current = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.effect_uniform_frame(renderer)
                })
            })?;
            current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_effect_uniform_batch_enabled_for_benchmark(false);
                renderer.set_backdrop_batch_enabled_for_benchmark(false);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut legacy = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.effect_uniform_frame(renderer)
                })
            })?;
            legacy.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(true);
            }
            let ratio = if current.p50_ms > 0.0 { legacy.p50_ms / current.p50_ms } else { 0.0 };
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{};legacy_over_current={ratio:.3};expected_backdrops={WEBGPU_EFFECT_UNIFORM_BACKDROPS};sigma=18.0",
                sampled_case_metrics(&current, "current"),
                sampled_case_metrics(&legacy, "legacy"),
            ))
        }

        pub async fn bench_webgpu_backdrop_batch_ab(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 12);
            let renderer = self.ensure_upload_bench_resources()?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(true);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut current = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.backdrop_batch_frame(renderer)
                })
            })?;
            current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(false);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut legacy = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.backdrop_batch_frame(renderer)
                })
            })?;
            legacy.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(true);
            }
            let ratio = if current.p50_ms > 0.0 { legacy.p50_ms / current.p50_ms } else { 0.0 };
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{};legacy_over_current={ratio:.3};expected_backdrops={WEBGPU_BACKDROP_BATCH_BACKDROPS};sigma=6.0",
                sampled_case_metrics(&current, "current"),
                sampled_case_metrics(&legacy, "legacy"),
            ))
        }

        pub async fn bench_webgpu_scene3d_ab(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.state.borrow().renderer.clone();
            let mut resources = {
                let mut renderer = renderer.borrow_mut();
                WebGpuScene3dBenchResources::new(&mut renderer)?
            };
            let mut stress_resources = {
                let mut renderer = renderer.borrow_mut();
                WebGpuScene3dStressBenchResources::new(&mut renderer)?
            };
            let mut stress_recreate = WebGpuScene3dStressRecreateResources::new();
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut reused = {
                let mut renderer = renderer.borrow_mut();
                bench_webgpu_sampled_case(&mut renderer, sample_count, frames, |renderer, _, _| {
                    resources.frame(renderer)
                })?
            };
            reused.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut recreate = {
                let mut renderer = renderer.borrow_mut();
                bench_webgpu_sampled_case(&mut renderer, sample_count, frames, |renderer, _, _| {
                    webgpu_scene3d_recreate_frame(renderer, 512, 512, 2.0)
                })?
            };
            recreate.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut stress_reused = {
                let mut renderer = renderer.borrow_mut();
                bench_webgpu_sampled_case(&mut renderer, sample_count, frames, |renderer, _, _| {
                    stress_resources.frame(renderer)
                })?
            };
            stress_reused.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut stress_recreate_summary = {
                let mut renderer = renderer.borrow_mut();
                bench_webgpu_sampled_case(&mut renderer, sample_count, frames, |renderer, _, _| {
                    stress_recreate.frame(renderer)
                })?
            };
            stress_recreate_summary.stats =
                settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let ratio = if reused.p50_ms > 0.0 { recreate.p50_ms / reused.p50_ms } else { 0.0 };
            let stress_ratio = if stress_reused.p50_ms > 0.0 {
                stress_recreate_summary.p50_ms / stress_reused.p50_ms
            } else {
                0.0
            };
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{}{}{};recreate_over_reused={ratio:.3};stress_recreate_over_reused={stress_ratio:.3};meshes=2;instances=2;stress_meshes=2;stress_instances={WEBGPU_SCENE3D_STRESS_INSTANCES}",
                sampled_case_metrics(&reused, "reused"),
                sampled_case_metrics(&recreate, "recreate"),
                sampled_case_metrics(&stress_reused, "stress_reused"),
                sampled_case_metrics(&stress_recreate_summary, "stress_recreate"),
            ))
        }

        pub async fn bench_webgpu_mixed_matrix(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.ensure_upload_bench_resources()?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_state_cache_enabled_for_benchmark(true);
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(true);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut current = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.mixed_frame(renderer)
                })
            })?;
            current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_state_cache_enabled_for_benchmark(false);
                renderer.set_effect_uniform_batch_enabled_for_benchmark(false);
                renderer.set_backdrop_batch_enabled_for_benchmark(false);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut legacy = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.mixed_frame(renderer)
                })
            })?;
            legacy.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_state_cache_enabled_for_benchmark(true);
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(true);
            }
            let ratio = if current.p50_ms > 0.0 { legacy.p50_ms / current.p50_ms } else { 0.0 };
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{};legacy_over_current={ratio:.3};glyphs={};image_tiles={WEBGPU_MIXED_IMAGE_TILES};image_width={};image_height={}",
                sampled_case_metrics(&current, "current"),
                sampled_case_metrics(&legacy, "legacy"),
                WEBGPU_MIXED_GLYPHS,
                WEBGPU_UPLOAD_IMAGE_SIZE,
                WEBGPU_UPLOAD_IMAGE_SIZE,
            ))
        }

        pub async fn bench_webgpu_layer_effects_matrix(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.ensure_upload_bench_resources()?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_state_cache_enabled_for_benchmark(true);
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(true);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut current = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.layer_effects_frame(renderer)
                })
            })?;
            current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_state_cache_enabled_for_benchmark(false);
                renderer.set_effect_uniform_batch_enabled_for_benchmark(false);
                renderer.set_backdrop_batch_enabled_for_benchmark(false);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut legacy = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.layer_effects_frame(renderer)
                })
            })?;
            legacy.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_state_cache_enabled_for_benchmark(true);
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(true);
            }
            let ratio = if current.p50_ms > 0.0 { legacy.p50_ms / current.p50_ms } else { 0.0 };
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{};legacy_over_current={ratio:.3};glyphs={};image_tiles={WEBGPU_LAYER_EFFECT_IMAGE_TILES};image_width={};image_height={};expected_layers=3;expected_damage_rects=3;expected_backdrops={WEBGPU_LAYER_EFFECT_BACKDROPS}",
                sampled_case_metrics(&current, "current"),
                sampled_case_metrics(&legacy, "legacy"),
                WEBGPU_LAYER_EFFECT_GLYPHS,
                WEBGPU_UPLOAD_IMAGE_SIZE,
                WEBGPU_UPLOAD_IMAGE_SIZE,
            ))
        }

        pub async fn bench_webgpu_command_family_matrix(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.ensure_upload_bench_resources()?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_state_cache_enabled_for_benchmark(true);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut current = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.command_family_frame(renderer)
                })
            })?;
            current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_state_cache_enabled_for_benchmark(false);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut legacy = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.command_family_frame(renderer)
                })
            })?;
            legacy.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_state_cache_enabled_for_benchmark(true);
            }
            let ratio = if current.p50_ms > 0.0 { legacy.p50_ms / current.p50_ms } else { 0.0 };
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{};legacy_over_current={ratio:.3};expected_image_meshes={WEBGPU_COMMAND_FAMILY_REPEATS};expected_nine_slices={WEBGPU_COMMAND_FAMILY_REPEATS};expected_sdf_glyphs={};expected_sdf_runs={WEBGPU_COMMAND_FAMILY_SDF_RUNS};expected_camera_bg=0;image_width={};image_height={}",
                sampled_case_metrics(&current, "current"),
                sampled_case_metrics(&legacy, "legacy"),
                WEBGPU_COMMAND_FAMILY_SDF_GLYPHS.saturating_mul(WEBGPU_COMMAND_FAMILY_SDF_RUNS),
                WEBGPU_UPLOAD_IMAGE_SIZE,
                WEBGPU_UPLOAD_IMAGE_SIZE,
            ))
        }

        pub async fn bench_webgpu_neon_marker_ab(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.ensure_upload_bench_resources()?;
            {
                renderer.borrow_mut().set_draw_state_cache_enabled_for_benchmark(true);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut current = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.neon_marker_frame(renderer)
                })
            })?;
            current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                renderer.borrow_mut().set_draw_state_cache_enabled_for_benchmark(false);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut legacy = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.neon_marker_frame(renderer)
                })
            })?;
            legacy.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                renderer.borrow_mut().set_draw_state_cache_enabled_for_benchmark(true);
            }
            let ratio = if current.p50_ms > 0.0 { legacy.p50_ms / current.p50_ms } else { 0.0 };
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{};legacy_over_current={ratio:.3};expected_markers={WEBGPU_NEON_MARKERS};expected_draw_items={}",
                sampled_case_metrics(&current, "current"),
                sampled_case_metrics(&legacy, "legacy"),
                WEBGPU_NEON_MARKERS.saturating_mul(3),
            ))
        }

        pub async fn bench_webgpu_direct_surface_ab(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.ensure_upload_bench_resources()?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_state_cache_enabled_for_benchmark(true);
                renderer.set_direct_surface_enabled_for_benchmark(true);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut current = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.direct_surface_frame(renderer)
                })
            })?;
            current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                renderer.borrow_mut().set_direct_surface_enabled_for_benchmark(false);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut legacy = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.direct_surface_frame(renderer)
                })
            })?;
            legacy.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                renderer.borrow_mut().set_direct_surface_enabled_for_benchmark(true);
            }
            let ratio = if current.p50_ms > 0.0 { legacy.p50_ms / current.p50_ms } else { 0.0 };
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{};legacy_over_current={ratio:.3};expected_draw_items={};expected_image_draws={WEBGPU_DIRECT_SURFACE_DRAWS};columns={WEBGPU_DIRECT_SURFACE_COLUMNS};image_width={};image_height={}",
                sampled_case_metrics(&current, "current"),
                sampled_case_metrics(&legacy, "legacy"),
                WEBGPU_DIRECT_SURFACE_DRAWS.saturating_add(1),
                WEBGPU_UPLOAD_IMAGE_SIZE,
                WEBGPU_UPLOAD_IMAGE_SIZE,
            ))
        }

        pub async fn bench_webgpu_draw_state_cache_ab(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.ensure_upload_bench_resources()?;
            {
                renderer.borrow_mut().set_draw_state_cache_enabled_for_benchmark(true);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut current = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.draw_state_cache_frame(renderer)
                })
            })?;
            current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                renderer.borrow_mut().set_draw_state_cache_enabled_for_benchmark(false);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut legacy = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.draw_state_cache_frame(renderer)
                })
            })?;
            legacy.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                renderer.borrow_mut().set_draw_state_cache_enabled_for_benchmark(true);
            }
            let ratio = if current.p50_ms > 0.0 { legacy.p50_ms / current.p50_ms } else { 0.0 };
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{};legacy_over_current={ratio:.3};expected_draw_items={WEBGPU_DRAW_STATE_CACHE_DRAWS};columns={WEBGPU_DRAW_STATE_CACHE_COLUMNS};image_width={};image_height={}",
                sampled_case_metrics(&current, "current"),
                sampled_case_metrics(&legacy, "legacy"),
                WEBGPU_UPLOAD_IMAGE_SIZE,
                WEBGPU_UPLOAD_IMAGE_SIZE,
            ))
        }

        pub async fn bench_webgpu_clip_state_ab(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.ensure_upload_bench_resources()?;
            {
                renderer.borrow_mut().set_draw_state_cache_enabled_for_benchmark(true);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut current = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.clip_state_frame(renderer)
                })
            })?;
            current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                renderer.borrow_mut().set_draw_state_cache_enabled_for_benchmark(false);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut legacy = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.clip_state_frame(renderer)
                })
            })?;
            legacy.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                renderer.borrow_mut().set_draw_state_cache_enabled_for_benchmark(true);
            }
            let ratio = if current.p50_ms > 0.0 { legacy.p50_ms / current.p50_ms } else { 0.0 };
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{};legacy_over_current={ratio:.3};expected_draw_items={WEBGPU_CLIP_STATE_DRAWS};expected_clip_runs={WEBGPU_CLIP_STATE_RUNS};expected_clip_depth=2;image_width={};image_height={}",
                sampled_case_metrics(&current, "current"),
                sampled_case_metrics(&legacy, "legacy"),
                WEBGPU_UPLOAD_IMAGE_SIZE,
                WEBGPU_UPLOAD_IMAGE_SIZE,
            ))
        }

        pub fn render_webgpu_id_mask_snapshot(&self) -> Result<String, JsValue> {
            let renderer = {
                let mut state = self.state.borrow_mut();
                state.direct_capture_active = true;
                state.renderer.clone()
            };
            let mut renderer = renderer.borrow_mut();
            let vertices = webgpu_id_mask_vertices(WEBGPU_ID_MASK_CELLS, WEBGPU_ID_MASK_EXTENT);
            let vertex_bytes =
                vertices.len() * core::mem::size_of::<id_mask_compositor::IdMaskRasterVertex>();
            webgpu_id_mask_frame(&mut renderer, &vertices, 1, perf_now())?;
            let draws = renderer.last_stats().draws;
            Ok(format!(
                "vertices={};vertex_bytes={vertex_bytes};revision=1;draws={draws}",
                vertices.len(),
            ))
        }

        pub fn set_scene(&self, scene_index: usize) {
            {
                let mut state = self.state.borrow_mut();
                state.router.set_scene(scene_index);
                state.mark_frame_dirty();
            }
            request_next_frame(&self.state);
        }

        #[must_use]
        pub fn last_draw_count(&self) -> u32 {
            self.state.borrow().last_draw_count()
        }

        #[must_use]
        pub fn renderer_backend(&self) -> String {
            self.state.borrow().renderer_backend().to_string()
        }
    }

    impl OxideWebApp {
        fn ensure_upload_bench_resources(&self) -> Result<Rc<RefCell<BrowserRenderer>>, JsValue> {
            let renderer = self.state.borrow().renderer.clone();
            if self.state.borrow().bench_resources.is_none() {
                let resources = {
                    let mut renderer = renderer.borrow_mut();
                    WebGpuUploadBenchResources::new(&mut renderer)?
                };
                self.state.borrow_mut().bench_resources = Some(resources);
            }
            Ok(renderer)
        }

        fn with_upload_bench_resources<T>(
            &self,
            f: impl FnOnce(&mut BrowserRenderer, &mut WebGpuUploadBenchResources) -> Result<T, JsValue>,
        ) -> Result<T, JsValue> {
            let renderer = self.ensure_upload_bench_resources()?;
            let mut state = self.state.borrow_mut();
            let Some(resources) = state.bench_resources.as_mut() else {
                return Err(JsValue::from_str("WebGPU upload benchmark resources unavailable"));
            };
            let mut renderer = renderer.borrow_mut();
            f(&mut renderer, resources)
        }
    }

    /// Returns unsupported because WebGPU renderer initialization is asynchronous in browsers.
    #[wasm_bindgen]
    pub fn start_oxide(canvas_id: &str) -> Result<OxideWebApp, JsValue> {
        OxideWebApp::new(canvas_id)
    }

    /// Starts Oxide with the required WebGPU renderer and begins the animation loop.
    #[wasm_bindgen]
    pub async fn start_oxide_async(canvas_id: &str) -> Result<OxideWebApp, JsValue> {
        let app = OxideWebApp::new_async(canvas_id).await?;
        app.start()?;
        Ok(app)
    }

    /// Runs a browser-backed platform smoke check for the static test page.
    #[wasm_bindgen]
    pub fn platform_smoke_report() -> String {
        let platform = oxide_platform_web::platform();
        let caps = platform.capabilities();
        let network = platform.network_status().current_status();
        platform.network_status().subscribe(Box::new(|_| {}));
        let web_view = platform.web_view_service().create_view("about:blank", Box::new(|_| {}));
        let web_view_status = match web_view {
            Ok(view) => {
                view.close();
                "ok"
            }
            Err(_) => "unsupported",
        };
        let location_status = permission_status_name(
            platform.permissions().status(platform_api::PermissionDomain::Location),
        );
        format!(
            "caps={};online={};location={};webview={}",
            caps.bits(),
            network.is_connected,
            location_status,
            web_view_status,
        )
    }

    /// Probes browser WebGPU availability without depending on unstable web-sys WebGPU bindings.
    #[wasm_bindgen]
    pub async fn webgpu_smoke_report() -> String {
        match webgpu_smoke_report_inner().await {
            Ok(report) => report,
            Err(error) => format!("webgpu=error;detail={}", js_detail(&error)),
        }
    }

    /// Reports browser WebGPU timestamp-query capability for GPU-stage attribution.
    #[wasm_bindgen]
    pub async fn webgpu_timing_report() -> String {
        match webgpu_timing_report_inner().await {
            Ok(report) => report,
            Err(error) => format!(
                "timestamp_query=probe-error;gpu_stage_attribution=unavailable;source=adapter.features;detail={}",
                js_detail(&error),
            ),
        }
    }

    async fn webgpu_smoke_report_inner() -> Result<String, JsValue> {
        let Some(window) = web_sys::window() else {
            return Ok(String::from("webgpu=missing-window"));
        };
        let navigator = window.navigator();
        let gpu = js_sys::Reflect::get(navigator.as_ref(), &JsValue::from_str("gpu"))?;
        if gpu.is_null() || gpu.is_undefined() {
            return Ok(String::from("webgpu=missing"));
        }

        let adapter = webgpu_call_promise(&gpu, "requestAdapter").await?;
        if adapter.is_null() || adapter.is_undefined() {
            return Ok(String::from("webgpu=adapter-none"));
        }

        let device = webgpu_call_promise(&adapter, "requestDevice").await?;
        if device.is_null() || device.is_undefined() {
            return Ok(String::from("webgpu=device-none"));
        }
        webgpu_destroy_device(&device);
        Ok(String::from("webgpu=device-ok"))
    }

    async fn webgpu_timing_report_inner() -> Result<String, JsValue> {
        let Some(window) = web_sys::window() else {
            return Ok(String::from(
                "timestamp_query=missing-window;gpu_stage_attribution=unavailable;source=adapter.features",
            ));
        };
        let navigator = window.navigator();
        let gpu = js_sys::Reflect::get(navigator.as_ref(), &JsValue::from_str("gpu"))?;
        if gpu.is_null() || gpu.is_undefined() {
            return Ok(String::from(
                "timestamp_query=webgpu-missing;gpu_stage_attribution=unavailable;source=adapter.features",
            ));
        }

        let adapter = webgpu_call_promise(&gpu, "requestAdapter").await?;
        if adapter.is_null() || adapter.is_undefined() {
            return Ok(String::from(
                "timestamp_query=adapter-none;gpu_stage_attribution=unavailable;source=adapter.features",
            ));
        }

        let supported = webgpu_adapter_feature_supported(&adapter, "timestamp-query")?;
        if supported {
            Ok(String::from(
                "timestamp_query=adapter-supported;gpu_stage_attribution=renderer-timestamp-query-enabled;source=adapter.features",
            ))
        } else {
            Ok(String::from(
                "timestamp_query=adapter-unsupported;gpu_stage_attribution=pass-family-counters-only;source=adapter.features",
            ))
        }
    }

    fn webgpu_adapter_feature_supported(adapter: &JsValue, feature: &str) -> Result<bool, JsValue> {
        let features = js_sys::Reflect::get(adapter, &JsValue::from_str("features"))?;
        if features.is_null() || features.is_undefined() {
            return Ok(false);
        }
        let has = js_sys::Reflect::get(&features, &JsValue::from_str("has"))?
            .dyn_into::<js_sys::Function>()?;
        Ok(has
            .call1(&features, &JsValue::from_str(feature))?
            .as_bool()
            .unwrap_or(false))
    }

    struct WebGpuIdMaskBenchSummary {
        p50_ms: f64,
        p95_ms: f64,
        p99_ms: f64,
        peak_ms: f64,
        avg_ms: f64,
        frame_values: Vec<f64>,
        allocations: WebGpuAllocationSummary,
        vertices: usize,
        vertex_bytes: usize,
        stats: WebRendererStats,
    }

    struct WebGpuBenchSummary {
        p50_ms: f64,
        p95_ms: f64,
        p99_ms: f64,
        peak_ms: f64,
        avg_ms: f64,
        frame_values: Vec<f64>,
        allocations: WebGpuAllocationSummary,
        stats: WebRendererStats,
    }

    #[derive(Clone, Copy, Default)]
    struct WebGpuAllocationSummary {
        alloc_count: u64,
        alloc_bytes: u64,
        dealloc_count: u64,
        dealloc_bytes: u64,
        realloc_count: u64,
        realloc_grow_bytes: u64,
        realloc_shrink_bytes: u64,
        allocating_frames: u64,
        peak_frame_alloc_bytes: u64,
    }

    #[derive(Clone, Copy, Default)]
    struct WebGpuFrameStageAllocationSummary {
        canvas_resize: WebGpuAllocationSummary,
        frame_timing: WebGpuAllocationSummary,
        builder_clear: WebGpuAllocationSummary,
        router_update: WebGpuAllocationSummary,
        router_draw: WebGpuAllocationSummary,
        damage_handoff: WebGpuAllocationSummary,
        draw_coalesce: WebGpuAllocationSummary,
        begin_frame: WebGpuAllocationSummary,
        encode_pass: WebGpuAllocationSummary,
        submit: WebGpuAllocationSummary,
        post_submit: WebGpuAllocationSummary,
    }

    #[derive(Clone, Copy)]
    enum WebGpuFrameStage {
        CanvasResize,
        FrameTiming,
        BuilderClear,
        RouterUpdate,
        RouterDraw,
        DamageHandoff,
        DrawCoalesce,
        BeginFrame,
        EncodePass,
        Submit,
        PostSubmit,
    }

    struct WebGpuUploadBenchResources {
        glyph_atlas: gfx::ImageHandle,
        image: gfx::ImageHandle,
        full_a8: Vec<u8>,
        dirty_a8: Vec<u8>,
        full_rgba: Vec<u8>,
        dirty_rgba: Vec<u8>,
        neon_markers: Vec<neon_marker::NeonMarker>,
        builder: ui::DrawListBuilder,
        mixed_damage: gfx::Damage,
        layer_effects_damage: gfx::Damage,
    }

    impl WebGpuUploadBenchResources {
        fn new(renderer: &mut BrowserRenderer) -> Result<Self, JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let full_a8 = glyph_upload_a8(WEBGPU_UPLOAD_ATLAS_SIZE);
            let dirty_a8 = glyph_upload_a8(WEBGPU_UPLOAD_DIRTY_SIZE);
            let full_rgba =
                generate_checker_rgba(WEBGPU_UPLOAD_IMAGE_SIZE, WEBGPU_UPLOAD_IMAGE_SIZE);
            let dirty_rgba =
                generate_checker_rgba(WEBGPU_UPLOAD_DIRTY_SIZE, WEBGPU_UPLOAD_DIRTY_SIZE);
            let mut neon_markers = Vec::with_capacity(WEBGPU_NEON_MARKERS);
            webgpu_fill_neon_markers(&mut neon_markers);
            let glyph_atlas = renderer.image_create_a8(
                WEBGPU_UPLOAD_ATLAS_SIZE,
                WEBGPU_UPLOAD_ATLAS_SIZE,
                &full_a8,
                WEBGPU_UPLOAD_ATLAS_SIZE as usize,
            );
            let image = renderer.image_create_rgba8(
                WEBGPU_UPLOAD_IMAGE_SIZE,
                WEBGPU_UPLOAD_IMAGE_SIZE,
                &full_rgba,
                WEBGPU_UPLOAD_IMAGE_SIZE as usize * 4,
            );
            Ok(Self {
                glyph_atlas,
                image,
                full_a8,
                dirty_a8,
                full_rgba,
                dirty_rgba,
                neon_markers,
                builder: ui::DrawListBuilder::new(),
                mixed_damage: gfx::Damage {
                    rects: vec![
                        gfx::RectI::new(0, 0, 128, 128),
                        gfx::RectI::new(64, 64, 192, 192),
                    ],
                },
                layer_effects_damage: gfx::Damage {
                    rects: vec![
                        gfx::RectI::new(0, 0, 96, 96),
                        gfx::RectI::new(96, 64, 180, 132),
                        gfx::RectI::new(30, 180, 220, 70),
                    ],
                },
            })
        }

        fn glyph_frame(
            &mut self,
            renderer: &mut BrowserRenderer,
            dirty_update: bool,
        ) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            if dirty_update {
                renderer.image_update_a8(
                    self.glyph_atlas,
                    0,
                    0,
                    WEBGPU_UPLOAD_DIRTY_SIZE,
                    WEBGPU_UPLOAD_DIRTY_SIZE,
                    &self.dirty_a8,
                    WEBGPU_UPLOAD_DIRTY_SIZE as usize,
                );
            } else {
                renderer.image_update_a8(
                    self.glyph_atlas,
                    0,
                    0,
                    WEBGPU_UPLOAD_ATLAS_SIZE,
                    WEBGPU_UPLOAD_ATLAS_SIZE,
                    &self.full_a8,
                    WEBGPU_UPLOAD_ATLAS_SIZE as usize,
                );
            }
            self.builder.clear();
            self.builder.rrect(
                gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                [12.0; 4],
                gfx::Color::rgba(0.05, 0.07, 0.10, 1.0),
            );
            if !append_glyph_grid(
                &mut self.builder,
                self.glyph_atlas,
                WEBGPU_MIXED_GLYPHS,
                18.0,
                24.0,
                18.0,
                false,
                gfx::Color::rgba(0.95, 0.98, 1.0, 1.0),
            ) {
                return Err(JsValue::from_str("failed to build glyph upload draw list"));
            }
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn image_frame(
            &mut self,
            renderer: &mut BrowserRenderer,
            dirty_update: bool,
        ) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            if dirty_update {
                renderer
                    .image_update_rgba8(
                        self.image,
                        0,
                        0,
                        WEBGPU_UPLOAD_DIRTY_SIZE,
                        WEBGPU_UPLOAD_DIRTY_SIZE,
                        &self.dirty_rgba,
                        WEBGPU_UPLOAD_DIRTY_SIZE as usize * 4,
                    )
                    .map_err(render_err)?;
            } else {
                renderer
                    .image_update_rgba8(
                        self.image,
                        0,
                        0,
                        WEBGPU_UPLOAD_IMAGE_SIZE,
                        WEBGPU_UPLOAD_IMAGE_SIZE,
                        &self.full_rgba,
                        WEBGPU_UPLOAD_IMAGE_SIZE as usize * 4,
                    )
                    .map_err(render_err)?;
            }
            self.builder.clear();
            self.builder.rrect(
                gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                [16.0; 4],
                gfx::Color::rgba(0.96, 0.97, 0.98, 1.0),
            );
            self.builder.image(
                self.image,
                gfx::RectF::new(24.0, 24.0, 208.0, 208.0),
                gfx::RectF::new(
                    0.0,
                    0.0,
                    WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                    WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                ),
                1.0,
            );
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn upload_scratch_frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            for _ in 0..WEBGPU_UPLOAD_SCRATCH_UPDATES {
                renderer.image_update_a8(
                    self.glyph_atlas,
                    0,
                    0,
                    WEBGPU_UPLOAD_DIRTY_SIZE,
                    WEBGPU_UPLOAD_DIRTY_SIZE,
                    &self.dirty_a8,
                    WEBGPU_UPLOAD_DIRTY_SIZE as usize,
                );
                renderer
                    .image_update_rgba8(
                        self.image,
                        0,
                        0,
                        WEBGPU_UPLOAD_DIRTY_SIZE,
                        WEBGPU_UPLOAD_DIRTY_SIZE,
                        &self.dirty_rgba,
                        WEBGPU_UPLOAD_DIRTY_SIZE as usize * 4,
                    )
                    .map_err(render_err)?;
            }
            self.builder.clear();
            self.builder.rrect(
                gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                [10.0; 4],
                gfx::Color::rgba(0.05, 0.06, 0.08, 1.0),
            );
            self.builder.image(
                self.image,
                gfx::RectF::new(18.0, 18.0, 88.0, 88.0),
                gfx::RectF::new(
                    0.0,
                    0.0,
                    WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                    WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                ),
                0.92,
            );
            if !append_glyph_grid(
                &mut self.builder,
                self.glyph_atlas,
                WEBGPU_COMMAND_FAMILY_SDF_GLYPHS,
                130.0,
                36.0,
                12.0,
                false,
                gfx::Color::rgba(0.94, 0.97, 1.0, 0.95),
            ) {
                return Err(JsValue::from_str("failed to build upload scratch draw list"));
            }
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn effect_uniform_frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            self.builder.clear();
            self.builder.rrect(
                gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                [0.0; 4],
                gfx::Color::rgba(0.035, 0.045, 0.065, 1.0),
            );
            self.builder.image(
                self.image,
                gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                gfx::RectF::new(
                    0.0,
                    0.0,
                    WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                    WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                ),
                0.72,
            );
            let cols = 8_usize;
            let cell_w = 256.0 / cols as f32;
            let cell_h = 25.0;
            for index in 0..WEBGPU_EFFECT_UNIFORM_BACKDROPS {
                let col = index % cols;
                let row = index / cols;
                let x = col as f32 * cell_w + 2.0;
                let y = 8.0 + row as f32 * 30.0;
                self.builder.backdrop(
                    gfx::RectF::new(x, y, cell_w - 4.0, cell_h),
                    18.0,
                    gfx::Color::rgba(0.04, 0.06, 0.09, 0.28),
                    1.0,
                );
            }
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn backdrop_batch_frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            self.builder.clear();
            self.builder.rrect(
                gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                [0.0; 4],
                gfx::Color::rgba(0.025, 0.035, 0.055, 1.0),
            );
            self.builder.image(
                self.image,
                gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                gfx::RectF::new(
                    0.0,
                    0.0,
                    WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                    WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                ),
                0.74,
            );
            for index in 0..WEBGPU_BACKDROP_BATCH_BACKDROPS {
                let col = index % 4;
                let row = index / 4;
                let x = 18.0 + col as f32 * 58.0;
                let y = 24.0 + row as f32 * 58.0;
                self.builder.backdrop(
                    gfx::RectF::new(x, y, 30.0, 30.0),
                    6.0,
                    gfx::Color::rgba(0.05, 0.07, 0.10, 0.30),
                    1.0,
                );
            }
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn mixed_frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, Some(&self.mixed_damage));
            self.builder.clear();
            self.builder.rrect(
                gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                [0.0; 4],
                gfx::Color::rgba(0.04, 0.05, 0.07, 1.0),
            );
            self.builder.layer_begin(91, gfx::RectF::new(12.0, 12.0, 232.0, 232.0), true);
            self.builder.clip_push(gfx::RectI::new(10, 10, 236, 236));
            self.builder.image(
                self.image,
                gfx::RectF::new(18.0, 18.0, 96.0, 96.0),
                gfx::RectF::new(
                    0.0,
                    0.0,
                    WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                    WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                ),
                0.92,
            );
            self.builder.rrect(
                gfx::RectF::new(126.0, 22.0, 104.0, 60.0),
                [14.0; 4],
                gfx::Color::rgba(0.10, 0.42, 0.90, 0.90),
            );
            for index in 0..WEBGPU_MIXED_IMAGE_TILES {
                let col = index % WEBGPU_MIXED_IMAGE_COLUMNS;
                let row = index / WEBGPU_MIXED_IMAGE_COLUMNS;
                let x = 18.0 + col as f32 * 18.0;
                let y = 88.0 + row as f32 * 9.0;
                self.builder.image(
                    self.image,
                    gfx::RectF::new(x, y, 14.0, 7.0),
                    gfx::RectF::new(
                        0.0,
                        0.0,
                        WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                        WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                    ),
                    0.38,
                );
            }
            let _ = append_glyph_grid(
                &mut self.builder,
                self.glyph_atlas,
                WEBGPU_MIXED_GLYPHS,
                24.0,
                124.0,
                12.0,
                false,
                gfx::Color::rgba(0.96, 0.97, 1.0, 0.96),
            );
            self.builder.backdrop(
                gfx::RectF::new(64.0, 70.0, 150.0, 116.0),
                18.0,
                gfx::Color::rgba(0.08, 0.10, 0.14, 0.35),
                1.0,
            );
            self.builder.visual_effect(
                gfx::RectF::new(96.0, 104.0, 112.0, 72.0),
                gfx::VisualEffect::DarkPopup {
                    blur_intensity: 0.42,
                    tint: gfx::Color::rgba(0.02, 0.03, 0.05, 0.55),
                },
            );
            self.builder.spinner([210.0, 210.0], 10.0, 0.70);
            self.builder.clip_pop();
            self.builder.layer_end();
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn layer_effects_frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, Some(&self.layer_effects_damage));
            self.builder.clear();
            self.builder.rrect(
                gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                [0.0; 4],
                gfx::Color::rgba(0.03, 0.04, 0.06, 1.0),
            );
            self.builder.layer_begin(201, gfx::RectF::new(10.0, 10.0, 236.0, 236.0), true);
            self.builder.clip_push(gfx::RectI::new(8, 8, 240, 240));
            self.builder.image(
                self.image,
                gfx::RectF::new(18.0, 18.0, 92.0, 92.0),
                gfx::RectF::new(
                    0.0,
                    0.0,
                    WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                    WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                ),
                0.86,
            );
            for index in 0..WEBGPU_LAYER_EFFECT_IMAGE_TILES {
                let col = index % WEBGPU_LAYER_EFFECT_IMAGE_COLUMNS;
                let row = index / WEBGPU_LAYER_EFFECT_IMAGE_COLUMNS;
                let x = 18.0 + col as f32 * 18.0;
                let y = 118.0 + row as f32 * 7.0;
                self.builder.image(
                    self.image,
                    gfx::RectF::new(x, y, 14.0, 5.0),
                    gfx::RectF::new(
                        0.0,
                        0.0,
                        WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                        WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                    ),
                    0.30,
                );
            }
            self.builder.layer_begin(202, gfx::RectF::new(118.0, 18.0, 112.0, 120.0), true);
            self.builder.rrect(
                gfx::RectF::new(118.0, 18.0, 112.0, 120.0),
                [18.0; 4],
                gfx::Color::rgba(0.10, 0.31, 0.68, 0.78),
            );
            if !append_glyph_grid(
                &mut self.builder,
                self.glyph_atlas,
                WEBGPU_LAYER_EFFECT_GLYPHS,
                126.0,
                36.0,
                10.0,
                false,
                gfx::Color::rgba(0.95, 0.97, 1.0, 0.94),
            ) {
                return Err(JsValue::from_str("failed to build layer effects glyph draw list"));
            }
            for index in 0..WEBGPU_LAYER_EFFECT_BACKDROPS {
                let x = 26.0 + index as f32 * 54.0;
                self.builder.backdrop(
                    gfx::RectF::new(x, 92.0, 24.0, 24.0),
                    3.0,
                    gfx::Color::rgba(0.03, 0.05, 0.08, 0.40),
                    1.0,
                );
            }
            self.builder.visual_effect(
                gfx::RectF::new(74.0, 124.0, 128.0, 74.0),
                gfx::VisualEffect::DarkPopup {
                    blur_intensity: 0.50,
                    tint: gfx::Color::rgba(0.95, 0.97, 1.0, 0.34),
                },
            );
            self.builder.spinner([210.0, 210.0], 9.0, 0.76);
            self.builder.layer_end();
            self.builder.layer_begin(203, gfx::RectF::new(24.0, 184.0, 112.0, 48.0), true);
            self.builder.rrect(
                gfx::RectF::new(24.0, 184.0, 112.0, 48.0),
                [12.0; 4],
                gfx::Color::rgba(0.86, 0.34, 0.12, 0.90),
            );
            self.builder.layer_end();
            self.builder.clip_pop();
            self.builder.layer_end();
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn command_family_frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            self.builder.clear();
            self.builder.rrect(
                gfx::RectF::new(12.0, 12.0, 232.0, 232.0),
                [18.0; 4],
                gfx::Color::rgba(0.04, 0.05, 0.07, 0.48),
            );
            let mesh_indices = [0_u16, 1, 2, 2, 1, 3];
            for index in 0..WEBGPU_COMMAND_FAMILY_REPEATS {
                let col = index % WEBGPU_COMMAND_FAMILY_COLUMNS;
                let row = index / WEBGPU_COMMAND_FAMILY_COLUMNS;
                let x = 18.0 + col as f32 * 30.0;
                let y = 24.0 + row as f32 * 22.0;
                let mesh_vertices = [
                    gfx::Vertex { x, y, u: 0.0, v: 0.0, rgba: u32::MAX },
                    gfx::Vertex { x: x + 13.0, y: y + 1.0, u: 1.0, v: 0.0, rgba: u32::MAX },
                    gfx::Vertex { x: x + 1.0, y: y + 10.0, u: 0.0, v: 1.0, rgba: u32::MAX },
                    gfx::Vertex { x: x + 14.0, y: y + 11.0, u: 1.0, v: 1.0, rgba: u32::MAX },
                ];
                self.builder.image_mesh(self.image, &mesh_vertices, &mesh_indices, 0.72);
                self.builder.nine_slice(
                    self.image,
                    gfx::RectF::new(x + 15.0, y, 12.0, 12.0),
                    gfx::Insets::new(4.0, 4.0, 4.0, 4.0),
                    0.60,
                );
            }
            for run in 0..WEBGPU_COMMAND_FAMILY_SDF_RUNS {
                if !append_glyph_grid(
                    &mut self.builder,
                    self.glyph_atlas,
                    WEBGPU_COMMAND_FAMILY_SDF_GLYPHS,
                    18.0 + run as f32 * 28.0,
                    204.0,
                    2.0,
                    true,
                    gfx::Color::rgba(0.90, 0.96, 1.0, 0.92),
                ) {
                    return Err(JsValue::from_str("failed to build command family SDF glyph draw list"));
                }
            }
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn neon_marker_frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            renderer
                .encode_neon_markers(&neon_marker::NeonMarkerPass {
                    viewport: gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                    markers: &self.neon_markers,
                })
                .map_err(render_err)?;
            renderer.submit(token).map_err(render_err)
        }

        fn draw_state_cache_frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            self.builder.clear();
            let src = gfx::RectF::new(
                0.0,
                0.0,
                WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                WEBGPU_UPLOAD_IMAGE_SIZE as f32,
            );
            let size = 256.0 / WEBGPU_DRAW_STATE_CACHE_COLUMNS as f32;
            for index in 0..WEBGPU_DRAW_STATE_CACHE_DRAWS {
                let col = index % WEBGPU_DRAW_STATE_CACHE_COLUMNS;
                let row = index / WEBGPU_DRAW_STATE_CACHE_COLUMNS;
                self.builder.image(
                    self.image,
                    gfx::RectF::new(col as f32 * size, row as f32 * size, size, size),
                    src,
                    0.86,
                );
            }
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn direct_surface_frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            self.builder.clear();
            self.builder.rrect(
                gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                [0.0; 4],
                gfx::Color::rgba(0.03, 0.04, 0.06, 1.0),
            );
            let src = gfx::RectF::new(
                0.0,
                0.0,
                WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                WEBGPU_UPLOAD_IMAGE_SIZE as f32,
            );
            let size = 256.0 / WEBGPU_DIRECT_SURFACE_COLUMNS as f32;
            for index in 0..WEBGPU_DIRECT_SURFACE_DRAWS {
                let col = index % WEBGPU_DIRECT_SURFACE_COLUMNS;
                let row = index / WEBGPU_DIRECT_SURFACE_COLUMNS;
                self.builder.image(
                    self.image,
                    gfx::RectF::new(col as f32 * size, row as f32 * size, size, size),
                    src,
                    0.82,
                );
            }
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn clip_state_frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            self.builder.clear();
            let src = gfx::RectF::new(
                0.0,
                0.0,
                WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                WEBGPU_UPLOAD_IMAGE_SIZE as f32,
            );
            self.builder.clip_push(gfx::RectI::new(0, 0, 256, 256));
            let per_run = WEBGPU_CLIP_STATE_DRAWS / WEBGPU_CLIP_STATE_RUNS;
            let size = 256.0 / WEBGPU_DRAW_STATE_CACHE_COLUMNS as f32;
            for run in 0..WEBGPU_CLIP_STATE_RUNS {
                let y = ((run % WEBGPU_CLIP_STATE_RUNS) as i32 * 16).min(240);
                self.builder.clip_push(gfx::RectI::new(0, y, 256, 16));
                for slot in 0..per_run {
                    let index = run * per_run + slot;
                    let col = index % WEBGPU_DRAW_STATE_CACHE_COLUMNS;
                    let row = index / WEBGPU_DRAW_STATE_CACHE_COLUMNS;
                    self.builder.image(
                        self.image,
                        gfx::RectF::new(col as f32 * size, row as f32 * size, size, size),
                        src,
                        0.86,
                    );
                }
                self.builder.clip_pop();
            }
            self.builder.clip_pop();
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }
    }

    struct WebGpuScene3dBenchResources {
        back: scene3d::MeshHandle3d,
        front: scene3d::MeshHandle3d,
    }

    impl WebGpuScene3dBenchResources {
        fn new(renderer: &mut BrowserRenderer) -> Result<Self, JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let back = webgpu_scene3d_create_back_mesh(renderer)?;
            let front = webgpu_scene3d_create_front_mesh(renderer)?;
            Ok(Self { back, front })
        }

        fn frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            webgpu_scene3d_encode_handles(renderer, self.back, self.front)?;
            renderer.submit(token).map_err(render_err)
        }
    }

    struct WebGpuScene3dStressBenchResources {
        back: scene3d::MeshHandle3d,
        front: scene3d::MeshHandle3d,
        instances: Vec<scene3d::Instance3d>,
    }

    impl WebGpuScene3dStressBenchResources {
        fn new(renderer: &mut BrowserRenderer) -> Result<Self, JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let back = webgpu_scene3d_create_back_mesh(renderer)?;
            let front = webgpu_scene3d_create_front_mesh(renderer)?;
            let mut instances = Vec::with_capacity(WEBGPU_SCENE3D_STRESS_INSTANCES);
            webgpu_scene3d_fill_stress_instances(&mut instances, back, front);
            Ok(Self { back, front, instances })
        }

        fn frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            webgpu_scene3d_fill_stress_instances(&mut self.instances, self.back, self.front);
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            webgpu_scene3d_encode_instances(renderer, &self.instances)
                .and_then(|_| renderer.submit(token).map_err(render_err))
        }
    }

    struct WebGpuScene3dStressRecreateResources {
        instances: Vec<scene3d::Instance3d>,
    }

    impl WebGpuScene3dStressRecreateResources {
        fn new() -> Self {
            Self { instances: Vec::with_capacity(WEBGPU_SCENE3D_STRESS_INSTANCES) }
        }

        fn frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            let back = webgpu_scene3d_create_back_mesh(renderer)?;
            let front = webgpu_scene3d_create_front_mesh(renderer)?;
            webgpu_scene3d_fill_stress_instances(&mut self.instances, back, front);
            let result = webgpu_scene3d_encode_instances(renderer, &self.instances)
                .and_then(|_| renderer.submit(token).map_err(render_err));
            renderer.mesh3d_release(back);
            renderer.mesh3d_release(front);
            result
        }
    }

    fn bench_webgpu_id_mask_case(
        renderer: &mut BrowserRenderer,
        stable_revision: bool,
        sample_count: u32,
        frames: u32,
    ) -> Result<WebGpuIdMaskBenchSummary, JsValue> {
        let vertices = webgpu_id_mask_vertices(WEBGPU_ID_MASK_CELLS, WEBGPU_ID_MASK_EXTENT);
        let vertex_bytes =
            vertices.len() * core::mem::size_of::<id_mask_compositor::IdMaskRasterVertex>();
        let mut timestamp = perf_now();
        for warmup in 0..4 {
            let revision = if stable_revision { 1 } else { warmup as u64 + 1 };
            webgpu_id_mask_frame(renderer, &vertices, revision, timestamp)?;
            timestamp += 16.666_667;
        }

        let mut values = Vec::with_capacity(sample_count as usize);
        let mut allocations = WebGpuAllocationSummary::default();
        for sample in 0..sample_count {
            // Browser timer resolution can round the fast retained path to zero if each frame is timed alone.
            let sample_start = perf_now();
            for frame in 0..frames {
                let seq = sample.saturating_mul(frames).saturating_add(frame) as u64;
                let revision = if stable_revision { 1 } else { seq + 64 };
                let alloc_before = oxide_wasm_alloc_counter::snapshot();
                webgpu_id_mask_frame(renderer, &vertices, revision, timestamp)?;
                let alloc_after = oxide_wasm_alloc_counter::snapshot();
                add_allocation_frame(&mut allocations, alloc_before, alloc_after);
                timestamp += 16.666_667;
            }
            values.push(((perf_now() - sample_start).max(0.0)) / frames as f64);
        }
        values.sort_by(|a, b| a.total_cmp(b));
        let stats = renderer.last_stats();

        Ok(WebGpuIdMaskBenchSummary {
            p50_ms: percentile(&values, 0.50),
            p95_ms: percentile(&values, 0.95),
            p99_ms: percentile(&values, 0.99),
            peak_ms: values.last().copied().unwrap_or(0.0),
            avg_ms: average(&values),
            frame_values: values,
            allocations,
            vertices: vertices.len(),
            vertex_bytes,
            stats,
        })
    }

    fn bench_webgpu_sampled_case<F>(
        renderer: &mut BrowserRenderer,
        sample_count: u32,
        frames: u32,
        mut frame: F,
    ) -> Result<WebGpuBenchSummary, JsValue>
    where
        F: FnMut(&mut BrowserRenderer, u64, f64) -> Result<(), JsValue>,
    {
        let mut timestamp = perf_now();
        for warmup in 0..4 {
            frame(renderer, warmup, timestamp)?;
            timestamp += 16.666_667;
        }

        let mut values = Vec::with_capacity(sample_count as usize);
        let mut allocations = WebGpuAllocationSummary::default();
        for sample in 0..sample_count {
            let sample_start = perf_now();
            for frame_index in 0..frames {
                let seq = sample.saturating_mul(frames).saturating_add(frame_index) as u64;
                let alloc_before = oxide_wasm_alloc_counter::snapshot();
                frame(renderer, seq + 64, timestamp)?;
                let alloc_after = oxide_wasm_alloc_counter::snapshot();
                add_allocation_frame(&mut allocations, alloc_before, alloc_after);
                timestamp += 16.666_667;
            }
            values.push(((perf_now() - sample_start).max(0.0)) / frames as f64);
        }
        values.sort_by(|a, b| a.total_cmp(b));
        let stats = renderer.last_stats();

        Ok(WebGpuBenchSummary {
            p50_ms: percentile(&values, 0.50),
            p95_ms: percentile(&values, 0.95),
            p99_ms: percentile(&values, 0.99),
            peak_ms: values.last().copied().unwrap_or(0.0),
            avg_ms: average(&values),
            frame_values: values,
            allocations,
            stats,
        })
    }

    fn sampled_case_metrics(summary: &WebGpuBenchSummary, prefix: &str) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            ";{prefix}_p50_ms={:.3};{prefix}_p95_ms={:.3};{prefix}_p99_ms={:.3};{prefix}_peak_ms={:.3};{prefix}_avg_ms={:.3}",
            summary.p50_ms,
            summary.p95_ms,
            summary.p99_ms,
            summary.peak_ms,
            summary.avg_ms,
        );
        out.push_str(&frame_pacing_metrics(&summary.frame_values, prefix));
        out.push_str(&allocation_metrics(&summary.allocations, prefix));
        out.push_str(&renderer_stats_metrics(summary.stats, prefix));
        out
    }

    fn glyph_upload_a8(size: u32) -> Vec<u8> {
        let mut data = vec![0_u8; (size as usize).saturating_mul(size as usize)];
        for y in 0..size {
            for x in 0..size {
                let idx = (y as usize).saturating_mul(size as usize).saturating_add(x as usize);
                let edge = x < 4 || y < 4 || x + 4 >= size || y + 4 >= size;
                data[idx] = if edge || ((x / 8 + y / 8) & 1) == 0 { 220 } else { 96 };
            }
        }
        data
    }

    fn append_glyph_grid(
        builder: &mut ui::DrawListBuilder,
        atlas: gfx::ImageHandle,
        count: usize,
        origin_x: f32,
        origin_y: f32,
        cell: f32,
        sdf: bool,
        color: gfx::Color,
    ) -> bool {
        let drawlist = builder.drawlist_mut();
        let Ok(vb_offset) = u32::try_from(drawlist.vertices.len()) else {
            return false;
        };
        let Ok(ib_offset) = u32::try_from(drawlist.indices.len()) else {
            return false;
        };
        let cols = 12_usize;
        let u1 = WEBGPU_UPLOAD_DIRTY_SIZE as f32 / WEBGPU_UPLOAD_ATLAS_SIZE as f32;
        let v1 = u1;
        for idx in 0..count {
            let Ok(base) = u16::try_from(idx.saturating_mul(4)) else {
                return false;
            };
            let col = idx % cols;
            let row = idx / cols;
            let x = origin_x + col as f32 * cell;
            let y = origin_y + row as f32 * cell;
            let right = x + cell * 0.72;
            let bottom = y + cell * 0.92;
            drawlist.vertices.extend_from_slice(&[
                gfx::Vertex { x, y, u: 0.0, v: 0.0, rgba: u32::MAX },
                gfx::Vertex { x: right, y, u: u1, v: 0.0, rgba: u32::MAX },
                gfx::Vertex { x, y: bottom, u: 0.0, v: v1, rgba: u32::MAX },
                gfx::Vertex { x: right, y: bottom, u: u1, v: v1, rgba: u32::MAX },
            ]);
            drawlist.indices.extend_from_slice(&[
                base,
                base + 1,
                base + 2,
                base + 2,
                base + 1,
                base + 3,
            ]);
        }
        let Ok(vb_len) = u32::try_from(count.saturating_mul(4)) else {
            return false;
        };
        let Ok(ib_len) = u32::try_from(count.saturating_mul(6)) else {
            return false;
        };
        drawlist.items.push(gfx::DrawCmd::GlyphRun {
            run: gfx::GlyphRun {
                atlas,
                atlas_revision: 1,
                vb: gfx::VertexSpan { offset: vb_offset, len: vb_len },
                ib: gfx::IndexSpan { offset: ib_offset, len: ib_len },
                sdf,
                color,
            },
        });
        true
    }

    fn webgpu_scene3d_frame(
        renderer: &mut BrowserRenderer,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<(), JsValue> {
        renderer.resize(width, height, scale).map_err(render_err)?;
        let back = webgpu_scene3d_create_back_mesh(renderer)?;
        let front = webgpu_scene3d_create_front_mesh(renderer)?;
        let token = renderer.begin_frame(&gfx::FrameTarget, None);
        let result = webgpu_scene3d_encode_handles(renderer, back, front)
            .and_then(|_| renderer.submit(token).map_err(render_err));
        renderer.mesh3d_release(back);
        renderer.mesh3d_release(front);
        result
    }

    fn webgpu_scene3d_recreate_frame(
        renderer: &mut BrowserRenderer,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<(), JsValue> {
        renderer.resize(width, height, scale).map_err(render_err)?;
        let token = renderer.begin_frame(&gfx::FrameTarget, None);
        let back = webgpu_scene3d_create_back_mesh(renderer)?;
        let front = webgpu_scene3d_create_front_mesh(renderer)?;
        let result = webgpu_scene3d_encode_handles(renderer, back, front)
            .and_then(|_| renderer.submit(token).map_err(render_err));
        renderer.mesh3d_release(back);
        renderer.mesh3d_release(front);
        result
    }

    fn webgpu_scene3d_create_back_mesh(
        renderer: &mut BrowserRenderer,
    ) -> Result<scene3d::MeshHandle3d, JsValue> {
        let back_vertices = [
            scene3d::VertexColor3d {
                position: [-0.76, -0.58, 0.24],
                color: [0.10, 0.36, 1.0, 1.0],
            },
            scene3d::VertexColor3d { position: [0.62, -0.50, 0.24], color: [0.16, 0.74, 1.0, 1.0] },
            scene3d::VertexColor3d {
                position: [-0.08, 0.62, 0.24],
                color: [0.10, 0.26, 0.82, 1.0],
            },
        ];
        let indices = [0_u32, 1, 2];
        renderer
            .mesh3d_create_colored(&scene3d::MeshColor3dData {
                vertices: &back_vertices,
                indices: &indices,
                topology: scene3d::MeshTopology::Triangles,
            })
            .map_err(render_err)
    }

    fn webgpu_scene3d_create_front_mesh(
        renderer: &mut BrowserRenderer,
    ) -> Result<scene3d::MeshHandle3d, JsValue> {
        let front_vertices = [
            scene3d::VertexColor3d {
                position: [-0.52, -0.34, 0.08],
                color: [1.0, 0.42, 0.14, 1.0],
            },
            scene3d::VertexColor3d { position: [0.44, -0.30, 0.08], color: [1.0, 0.76, 0.20, 1.0] },
            scene3d::VertexColor3d {
                position: [-0.06, 0.46, 0.08],
                color: [0.98, 0.24, 0.16, 1.0],
            },
        ];
        let indices = [0_u32, 1, 2];
        renderer
            .mesh3d_create_colored(&scene3d::MeshColor3dData {
                vertices: &front_vertices,
                indices: &indices,
                topology: scene3d::MeshTopology::Triangles,
            })
            .map_err(render_err)
    }

    fn webgpu_scene3d_encode_handles(
        renderer: &mut BrowserRenderer,
        back: scene3d::MeshHandle3d,
        front: scene3d::MeshHandle3d,
    ) -> Result<(), JsValue> {
        let identity = scene3d::identity_mat4();
        let mut back_instance =
            scene3d::Instance3d::new(back, identity, gfx::Color::rgba(1.0, 1.0, 1.0, 1.0));
        back_instance.cull = scene3d::CullMode3d::None;
        let mut front_instance =
            scene3d::Instance3d::new(front, identity, gfx::Color::rgba(1.0, 1.0, 1.0, 1.0));
        front_instance.cull = scene3d::CullMode3d::None;
        let instances = [back_instance, front_instance];
        let pass = scene3d::Pass3d {
            viewport: None,
            clear_color: Some(gfx::Color::rgba(0.025, 0.035, 0.055, 1.0)),
            clear_depth: true,
            view_proj: identity,
            instances: &instances,
            bloom: None,
        };
        renderer.encode_scene3d(&pass).map_err(render_err)
    }

    fn webgpu_scene3d_fill_stress_instances(
        out: &mut Vec<scene3d::Instance3d>,
        back: scene3d::MeshHandle3d,
        front: scene3d::MeshHandle3d,
    ) {
        out.clear();
        out.reserve(WEBGPU_SCENE3D_STRESS_INSTANCES);
        let scale = scene3d::scale_xyz(0.105);
        for index in 0..WEBGPU_SCENE3D_STRESS_INSTANCES {
            let col = index % 12;
            let row = index / 12;
            let x = -0.88 + col as f32 * 0.16;
            let y = -0.76 + row as f32 * 0.22;
            let translate = scene3d::clip_space_translate(x, y);
            let transform = scene3d::mat4_mul(&translate, &scale);
            let mesh = if (col + row) & 1 == 0 { back } else { front };
            let tint = if (col + row) & 1 == 0 {
                gfx::Color::rgba(0.70, 0.86, 1.0, 0.96)
            } else {
                gfx::Color::rgba(1.0, 0.82, 0.44, 0.96)
            };
            let mut instance = scene3d::Instance3d::new(mesh, transform, tint);
            instance.cull = scene3d::CullMode3d::None;
            instance.depth_write = row % 2 == 0;
            out.push(instance);
        }
    }

    fn webgpu_fill_neon_markers(out: &mut Vec<neon_marker::NeonMarker>) {
        out.clear();
        out.reserve(WEBGPU_NEON_MARKERS);
        for index in 0..WEBGPU_NEON_MARKERS {
            let col = index % WEBGPU_NEON_MARKER_COLUMNS;
            let row = index / WEBGPU_NEON_MARKER_COLUMNS;
            let x = 24.0 + col as f32 * 28.0;
            let y = 26.0 + row as f32 * 22.0;
            let hue = index as f32 / WEBGPU_NEON_MARKERS as f32;
            out.push(neon_marker::NeonMarker {
                center: [x, y],
                core_radius_px: 2.5 + (index % 3) as f32 * 0.4,
                ring_radius_px: 5.5 + (index % 4) as f32 * 0.35,
                ring_width_px: 1.5,
                halo_radius_px: 11.0 + (index % 5) as f32 * 0.5,
                halo_sigma_px: 7.0,
                core_color: gfx::Color::rgba(0.92, 0.98, 1.0, 0.96),
                ring_color: gfx::Color::rgba(0.20 + hue * 0.60, 0.70, 1.0 - hue * 0.45, 0.85),
                halo_alpha_max: 0.22,
                ring_alpha_max: 0.74,
            });
        }
    }

    fn webgpu_scene3d_encode_instances(
        renderer: &mut BrowserRenderer,
        instances: &[scene3d::Instance3d],
    ) -> Result<(), JsValue> {
        let pass = scene3d::Pass3d {
            viewport: None,
            clear_color: Some(gfx::Color::rgba(0.022, 0.028, 0.045, 1.0)),
            clear_depth: true,
            view_proj: scene3d::identity_mat4(),
            instances,
            bloom: None,
        };
        renderer.encode_scene3d(&pass).map_err(render_err)
    }

    fn webgpu_id_mask_frame(
        renderer: &mut BrowserRenderer,
        vertices: &[id_mask_compositor::IdMaskRasterVertex],
        revision: u64,
        _timestamp: f64,
    ) -> Result<(), JsValue> {
        renderer.resize(512, 512, 2.0).map_err(render_err)?;
        let pass = webgpu_id_mask_pass(vertices, revision);
        let token = renderer.begin_frame(&gfx::FrameTarget, None);
        renderer.encode_id_mask_gpu_compositor(&pass).map_err(render_err)?;
        renderer.submit(token).map_err(render_err)
    }

    fn webgpu_id_mask_vertices(
        cells: usize,
        extent: f32,
    ) -> Vec<id_mask_compositor::IdMaskRasterVertex> {
        let mut vertices = Vec::with_capacity(cells * cells * 6);
        let step = extent / cells as f32;
        let origin = 0.0;
        for y in 0..cells {
            for x in 0..cells {
                let x0 = origin + x as f32 * step;
                let y0 = origin + y as f32 * step;
                let x1 = x0 + step;
                let y1 = y0 + step;
                let city = ((x + y) & 3) as u8;
                let neighborhood = ((x * 3 + y * 5) & 31) as u8;
                vertices.push(id_mask_compositor::IdMaskRasterVertex::new(
                    [x0, y0],
                    city,
                    neighborhood,
                ));
                vertices.push(id_mask_compositor::IdMaskRasterVertex::new(
                    [x1, y0],
                    city,
                    neighborhood,
                ));
                vertices.push(id_mask_compositor::IdMaskRasterVertex::new(
                    [x0, y1],
                    city,
                    neighborhood,
                ));
                vertices.push(id_mask_compositor::IdMaskRasterVertex::new(
                    [x1, y0],
                    city,
                    neighborhood,
                ));
                vertices.push(id_mask_compositor::IdMaskRasterVertex::new(
                    [x1, y1],
                    city,
                    neighborhood,
                ));
                vertices.push(id_mask_compositor::IdMaskRasterVertex::new(
                    [x0, y1],
                    city,
                    neighborhood,
                ));
            }
        }
        vertices
    }

    fn webgpu_id_mask_pass<'a>(
        vertices: &'a [id_mask_compositor::IdMaskRasterVertex],
        revision: u64,
    ) -> id_mask_compositor::IdMaskGpuCompositorPass<'a> {
        let city_styles = [
            id_mask_compositor::IdMaskCityStyle {
                fill_rgb: [0.95, 0.26, 0.22],
                edge_rgb: [0.58, 0.10, 0.10],
                seam_rgb: [1.0, 0.78, 0.32],
            },
            id_mask_compositor::IdMaskCityStyle {
                fill_rgb: [0.15, 0.55, 0.95],
                edge_rgb: [0.05, 0.18, 0.42],
                seam_rgb: [0.70, 0.90, 1.0],
            },
            id_mask_compositor::IdMaskCityStyle {
                fill_rgb: [0.20, 0.72, 0.38],
                edge_rgb: [0.06, 0.26, 0.11],
                seam_rgb: [0.75, 1.0, 0.62],
            },
            id_mask_compositor::IdMaskCityStyle {
                fill_rgb: [0.72, 0.36, 0.92],
                edge_rgb: [0.28, 0.12, 0.44],
                seam_rgb: [0.94, 0.74, 1.0],
            },
        ];
        let mut neighborhood_colors =
            [[0.0_f32; 3]; id_mask_compositor::ID_MASK_MAX_NEIGHBORHOOD_COLORS];
        for (index, color) in neighborhood_colors.iter_mut().enumerate() {
            let t = index as f32 / id_mask_compositor::ID_MASK_MAX_NEIGHBORHOOD_COLORS as f32;
            *color = [0.20 + t * 0.55, 0.34 + (1.0 - t) * 0.38, 0.52 + (t * 0.27)];
        }

        id_mask_compositor::IdMaskGpuCompositorPass {
            raster: id_mask_compositor::IdMaskGpuRasterPass {
                viewport: gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                mask_width: 512,
                mask_height: 512,
                mask_scale: 2.0,
                vertex_revision: revision,
                vertices,
                projection: id_mask_compositor::IdMaskRasterProjection::screen_px(),
            },
            city_styles,
            neighborhood_colors,
            mode: id_mask_compositor::IdMaskCompositorMode::Beauty,
            glow_enabled: false,
            darken_background_alpha: 0.0,
            polish: id_mask_compositor::IdMaskPolishConfig::default(),
        }
    }

    async fn webgpu_call_promise(
        receiver: &JsValue,
        method: &'static str,
    ) -> Result<JsValue, JsValue> {
        let method_value = js_sys::Reflect::get(receiver, &JsValue::from_str(method))?;
        let method_fn =
            method_value.dyn_into::<js_sys::Function>().map_err(|_| JsValue::from_str(method))?;
        let promise_value = method_fn.call0(receiver)?;
        let promise = promise_value
            .dyn_into::<js_sys::Promise>()
            .map_err(|_| JsValue::from_str("WebGPU method did not return a Promise"))?;
        JsFuture::from(promise).await
    }

    fn webgpu_destroy_device(device: &JsValue) {
        let Ok(destroy_value) = js_sys::Reflect::get(device, &JsValue::from_str("destroy")) else {
            return;
        };
        let Some(destroy) = destroy_value.dyn_ref::<js_sys::Function>() else {
            return;
        };
        let _ = destroy.call0(device);
    }

    fn timestamp_stats_cover_row(
        stats: &WebRendererStats,
        after_frame_id: u64,
    ) -> bool {
        stats.gpu_timestamp_passes > 0
            && stats.gpu_timestamp_frame_id > after_frame_id
            && stats.gpu_timestamp_passes == stats.render_passes
    }

    async fn settle_renderer_timestamps(
        renderer: &Rc<RefCell<BrowserRenderer>>,
        after_frame_id: u64,
    ) -> Result<WebRendererStats, JsValue> {
        let target_frame_id = renderer.borrow().last_stats().frame_id;
        let mut stats = renderer.borrow_mut().collect_timestamp_readbacks();
        let mut pending_readbacks = renderer.borrow().pending_timestamp_readbacks();
        if !stats.gpu_timestamp_query_supported {
            return Ok(stats);
        }
        if timestamp_stats_cover_row(&stats, after_frame_id) && pending_readbacks == 0 {
            return Ok(stats);
        }
        for _ in 0..WEBGPU_TIMESTAMP_SETTLE_RAFS {
            wait_animation_frame_once().await?;
            stats = renderer.borrow_mut().collect_timestamp_readbacks();
            pending_readbacks = renderer.borrow().pending_timestamp_readbacks();
            if timestamp_stats_cover_row(&stats, after_frame_id) && pending_readbacks == 0 {
                return Ok(stats);
            }
        }
        Err(JsValue::from_str(&format!(
            "WebGPU timestamp readback did not settle for row after start frame {after_frame_id}; target frame {target_frame_id}, latest timestamp frame {} passes {}, render passes {}, pending readbacks {}",
            stats.gpu_timestamp_frame_id,
            stats.gpu_timestamp_passes,
            stats.render_passes,
            pending_readbacks,
        )))
    }

    async fn wait_animation_frame_once() -> Result<(), JsValue> {
        let promise = js_sys::Promise::new(&mut |resolve, reject| {
            let Some(window) = web_sys::window() else {
                let _ = reject.call1(&JsValue::UNDEFINED, &JsValue::from_str("window unavailable"));
                return;
            };
            let callback = Closure::once_into_js(move |_timestamp_ms: f64| {
                let _ = resolve.call0(&JsValue::UNDEFINED);
            });
            let Some(function) = callback.dyn_ref::<js_sys::Function>() else {
                let _ = reject.call1(&JsValue::UNDEFINED, &JsValue::from_str("raf callback unavailable"));
                return;
            };
            if let Err(error) = window.request_animation_frame(function) {
                let _ = reject.call1(&JsValue::UNDEFINED, &error);
            }
        });
        JsFuture::from(promise).await.map(|_| ())
    }

    fn request_next_frame(state: &Rc<RefCell<AppState>>) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let mut borrowed = state.borrow_mut();
        if borrowed.raf_pending {
            return;
        }
        let Some(raf) = borrowed.raf.as_ref() else {
            return;
        };
        if window.request_animation_frame(raf.as_ref().unchecked_ref()).is_ok() {
            borrowed.raf_pending = true;
        }
    }

    fn install_event_listeners(state: &Rc<RefCell<AppState>>) -> Result<(), JsValue> {
        let canvas = state.borrow().canvas.clone();
        let target: &EventTarget = canvas.unchecked_ref();
        install_pointer_listener(state, target, "pointerdown")?;
        install_pointer_listener(state, target, "pointermove")?;
        install_pointer_listener(state, target, "pointerup")?;
        install_pointer_listener(state, target, "pointercancel")?;
        install_wheel_listener(state, target)?;

        let Some(window) = web_sys::window() else {
            return Ok(());
        };
        let window_target: &EventTarget = window.unchecked_ref();
        install_keyboard_listener(state, window_target, "keydown")?;
        install_keyboard_listener(state, window_target, "keyup")?;
        install_frame_event_listener(state, window_target, "resize")?;
        install_frame_event_listener(state, window_target, "oxide-redraw")?;
        install_ime_listeners(state, window_target)
    }

    fn create_ime_textarea(canvas: &HtmlCanvasElement) -> Result<HtmlTextAreaElement, JsValue> {
        let document = web_sys::window()
            .and_then(|window| window.document())
            .ok_or_else(|| JsValue::from_str("document is unavailable"))?;
        let textarea = document
            .create_element("textarea")?
            .dyn_into::<HtmlTextAreaElement>()
            .map_err(|_| JsValue::from_str("created element was not a textarea"))?;
        textarea.set_attribute("aria-hidden", "true")?;
        textarea.set_attribute("autocomplete", "off")?;
        textarea.set_attribute("autocapitalize", "off")?;
        textarea.set_attribute("spellcheck", "false")?;
        let style = textarea.style();
        style.set_property("position", "fixed")?;
        style.set_property("left", "0")?;
        style.set_property("top", "0")?;
        style.set_property("width", "1px")?;
        style.set_property("height", "1px")?;
        style.set_property("opacity", "0")?;
        style.set_property("pointer-events", "none")?;
        style.set_property("z-index", "-1")?;
        let document_body =
            document.body().ok_or_else(|| JsValue::from_str("document body is unavailable"))?;
        document_body.append_child(textarea.unchecked_ref())?;
        canvas.set_attribute("tabindex", "0")?;
        Ok(textarea)
    }

    fn retain_listener(
        state: &Rc<RefCell<AppState>>,
        target: &EventTarget,
        name: &'static str,
        closure: Closure<dyn FnMut(Event)>,
    ) -> Result<(), JsValue> {
        target.add_event_listener_with_callback(name, closure.as_ref().unchecked_ref())?;
        state.borrow_mut().listeners.push(closure);
        Ok(())
    }

    fn install_pointer_listener(
        state: &Rc<RefCell<AppState>>,
        target: &EventTarget,
        name: &'static str,
    ) -> Result<(), JsValue> {
        let state_for_event = Rc::clone(state);
        let closure = Closure::wrap(Box::new(move |event: Event| {
            let Some(pointer) = event.dyn_ref::<PointerEvent>() else {
                return;
            };
            event.prevent_default();
            let phase = touch_phase_for_event(name);
            let (x, y) = event_point(
                &state_for_event.borrow().canvas,
                pointer.client_x() as f32,
                pointer.client_y() as f32,
            );
            if pointer.pointer_type() == "touch" || pointer.pointer_type() == "pen" {
                let device = if pointer.pointer_type() == "pen" {
                    platform_api::PointerDevice::Pencil
                } else {
                    platform_api::PointerDevice::Finger
                };
                let touch = platform_api::TouchEvent {
                    id: platform_api::TouchId(pointer.pointer_id().max(0) as u64),
                    phase,
                    timestamp_ns: (pointer.time_stamp().max(0.0) * 1_000_000.0) as u64,
                    x,
                    y,
                    pressure: Some(pointer.pressure()),
                    tilt: None,
                    device,
                };
                let mut state = state_for_event.borrow_mut();
                state.router.input_touch(&touch);
                state.mark_frame_dirty();
            } else {
                let mut state = state_for_event.borrow_mut();
                state.router.input_pointer(
                    x,
                    y,
                    pointer.movement_x() as f32,
                    pointer.movement_y() as f32,
                    pointer.buttons() as u32,
                );
                state.mark_frame_dirty();
            }
            request_next_frame(&state_for_event);
        }) as Box<dyn FnMut(Event)>);
        retain_listener(state, target, name, closure)
    }

    fn install_wheel_listener(
        state: &Rc<RefCell<AppState>>,
        target: &EventTarget,
    ) -> Result<(), JsValue> {
        let state_for_event = Rc::clone(state);
        let closure = Closure::wrap(Box::new(move |event: Event| {
            let Some(wheel) = event.dyn_ref::<WheelEvent>() else {
                return;
            };
            event.prevent_default();
            let (x, y) = event_point(
                &state_for_event.borrow().canvas,
                wheel.client_x() as f32,
                wheel.client_y() as f32,
            );
            let delta = wheel.delta_y() as f32;
            if wheel.ctrl_key() || wheel.meta_key() {
                let mut state = state_for_event.borrow_mut();
                state.router.input_pinch(x, y, -delta * 0.001);
                state.mark_frame_dirty();
            } else {
                let mut state = state_for_event.borrow_mut();
                state.router.input_pointer(x, y, 0.0, -delta, 0);
                state.mark_frame_dirty();
            }
            request_next_frame(&state_for_event);
        }) as Box<dyn FnMut(Event)>);
        retain_listener(state, target, "wheel", closure)
    }

    fn install_keyboard_listener(
        state: &Rc<RefCell<AppState>>,
        target: &EventTarget,
        name: &'static str,
    ) -> Result<(), JsValue> {
        let state_for_event = Rc::clone(state);
        let closure = Closure::wrap(Box::new(move |event: Event| {
            let Some(keyboard) = event.dyn_ref::<KeyboardEvent>() else {
                return;
            };
            let mut state = state_for_event.borrow_mut();
            let ime_focused = state.ime_focused;
            let handled = route_key(&mut state.router, keyboard, name == "keydown", ime_focused);
            if handled {
                event.prevent_default();
            }
            state.mark_frame_dirty();
            drop(state);
            request_next_frame(&state_for_event);
        }) as Box<dyn FnMut(Event)>);
        retain_listener(state, target, name, closure)
    }

    fn install_ime_listeners(
        state: &Rc<RefCell<AppState>>,
        window_target: &EventTarget,
    ) -> Result<(), JsValue> {
        let textarea = state.borrow().ime_textarea.clone();
        let target: &EventTarget = textarea.unchecked_ref();

        let state_for_start = Rc::clone(state);
        let start = Closure::wrap(Box::new(move |event: Event| {
            event.prevent_default();
            let mut state = state_for_start.borrow_mut();
            state.ime_composing = true;
            state.ime_skip_next_input = false;
            state.mark_frame_dirty();
            drop(state);
            request_next_frame(&state_for_start);
        }) as Box<dyn FnMut(Event)>);
        retain_listener(state, target, "compositionstart", start)?;

        let state_for_update = Rc::clone(state);
        let update = Closure::wrap(Box::new(move |event: Event| {
            let Some(composition) = event.dyn_ref::<CompositionEvent>() else {
                return;
            };
            let text = composition.data().unwrap_or_default();
            let end = text.chars().count() as u32;
            let mut state = state_for_update.borrow_mut();
            state.router.input_set_composition(0, end, &text);
            state.mark_frame_dirty();
            drop(state);
            request_next_frame(&state_for_update);
        }) as Box<dyn FnMut(Event)>);
        retain_listener(state, target, "compositionupdate", update)?;

        let state_for_end = Rc::clone(state);
        let textarea_for_end = textarea.clone();
        let end = Closure::wrap(Box::new(move |event: Event| {
            let Some(composition) = event.dyn_ref::<CompositionEvent>() else {
                return;
            };
            let text = composition.data().unwrap_or_default();
            let mut state = state_for_end.borrow_mut();
            state.ime_composing = false;
            state.ime_skip_next_input = true;
            state.router.input_set_composition(0, 0, "");
            if !text.is_empty() {
                state.router.input_commit(&text);
            }
            textarea_for_end.set_value("");
            state.mark_frame_dirty();
            drop(state);
            request_next_frame(&state_for_end);
        }) as Box<dyn FnMut(Event)>);
        retain_listener(state, target, "compositionend", end)?;

        let state_for_input = Rc::clone(state);
        let textarea_for_input = textarea.clone();
        let input = Closure::wrap(Box::new(move |event: Event| {
            let Some(input) = event.dyn_ref::<InputEvent>() else {
                return;
            };
            let mut state = state_for_input.borrow_mut();
            if state.ime_composing {
                return;
            }
            if state.ime_skip_next_input {
                state.ime_skip_next_input = false;
                textarea_for_input.set_value("");
                return;
            }
            let text = input.data().unwrap_or_else(|| textarea_for_input.value());
            if !text.is_empty() {
                state.router.input_commit(&text);
            }
            textarea_for_input.set_value("");
            state.mark_frame_dirty();
            drop(state);
            request_next_frame(&state_for_input);
        }) as Box<dyn FnMut(Event)>);
        retain_listener(state, target, "input", input)?;

        let state_for_show = Rc::clone(state);
        let textarea_for_show = textarea.clone();
        let show = Closure::wrap(Box::new(move |_event: Event| {
            let mut state = state_for_show.borrow_mut();
            state.ime_focused = true;
            textarea_for_show.set_value("");
            let element: &HtmlElement = textarea_for_show.unchecked_ref();
            let _ = element.focus();
            let (_, physical_h, scale) = canvas_backing_size(&state.canvas);
            let logical_h = physical_h as f32 / scale.max(1.0);
            state.router.input_set_ime_rect(gfx::RectF::new(
                0.0,
                logical_h * 0.62,
                0.0,
                logical_h * 0.38,
            ));
            state.mark_frame_dirty();
            drop(state);
            request_next_frame(&state_for_show);
        }) as Box<dyn FnMut(Event)>);
        retain_listener(state, window_target, "oxide-ime-show", show)?;

        let state_for_hide = Rc::clone(state);
        let textarea_for_hide = textarea;
        let hide = Closure::wrap(Box::new(move |_event: Event| {
            let mut state = state_for_hide.borrow_mut();
            state.ime_focused = false;
            state.ime_composing = false;
            state.ime_skip_next_input = false;
            textarea_for_hide.set_value("");
            let element: &HtmlElement = textarea_for_hide.unchecked_ref();
            let _ = element.blur();
            state.router.input_hide_ime();
            state.mark_frame_dirty();
            drop(state);
            request_next_frame(&state_for_hide);
        }) as Box<dyn FnMut(Event)>);
        retain_listener(state, window_target, "oxide-ime-hide", hide)
    }

    fn install_frame_event_listener(
        state: &Rc<RefCell<AppState>>,
        target: &EventTarget,
        name: &'static str,
    ) -> Result<(), JsValue> {
        let state_for_event = Rc::clone(state);
        let closure = Closure::wrap(Box::new(move |_event: Event| {
            {
                let mut state = state_for_event.borrow_mut();
                if state.direct_capture_active {
                    return;
                }
                state.mark_frame_dirty();
                let _ = state.frame_at(perf_now());
            }
            if state_for_event.borrow().should_request_next_frame() {
                request_next_frame(&state_for_event);
            }
        }) as Box<dyn FnMut(Event)>);
        retain_listener(state, target, name, closure)
    }

    fn route_key(
        router: &mut scenes::Router<WebUploader>,
        event: &KeyboardEvent,
        down: bool,
        ime_focused: bool,
    ) -> bool {
        let key = event.key();
        if ime_focused {
            if down {
                let key_event = platform_api::KeyEvent {
                    code: key_code_for_event(event),
                    chars: None,
                    repeat: event.repeat(),
                    modifiers: modifiers_for_event(event),
                };
                router.input_key(&key_event);
            }
            return false;
        }
        if down {
            if let Some(scene) = scene_index_for_key(&key) {
                router.key_scene_select(scene);
                return true;
            }
        }

        match key.as_str() {
            " " => {
                if down {
                    router.key_space_down();
                } else {
                    router.key_space_up();
                }
                true
            }
            "ArrowLeft" if down => {
                router.key_arrow_left();
                true
            }
            "ArrowRight" if down => {
                router.key_arrow_right();
                true
            }
            "ArrowUp" if down => {
                router.key_arrow_up();
                true
            }
            "ArrowDown" if down => {
                router.key_arrow_down();
                true
            }
            "r" | "R" if down => {
                router.key_zoom_reset();
                true
            }
            "o" | "O" if down => {
                router.toggle_overlay();
                true
            }
            _ => {
                if down {
                    let key_event = platform_api::KeyEvent {
                        code: key_code_for_event(event),
                        chars: chars_for_key(&key),
                        repeat: event.repeat(),
                        modifiers: modifiers_for_event(event),
                    };
                    router.input_key(&key_event);
                }
                false
            }
        }
    }

    fn scene_index_for_key(key: &str) -> Option<usize> {
        match key {
            "1" => Some(0),
            "2" => Some(1),
            "3" => Some(2),
            "4" => Some(3),
            "5" => Some(4),
            "6" => Some(5),
            "7" => Some(6),
            "8" => Some(7),
            "9" => Some(8),
            "0" => Some(9),
            _ => None,
        }
    }

    fn chars_for_key(key: &str) -> Option<String> {
        if key.chars().count() == 1 {
            Some(key.to_owned())
        } else {
            None
        }
    }

    fn key_code_for_event(event: &KeyboardEvent) -> platform_api::KeyCode {
        let key = event.key();
        match key.as_str() {
            "Escape" => platform_api::KeyCode::Escape,
            "Enter" => platform_api::KeyCode::Enter,
            "Tab" => platform_api::KeyCode::Tab,
            "Backspace" => platform_api::KeyCode::Backspace,
            " " => platform_api::KeyCode::Space,
            "ArrowUp" => platform_api::KeyCode::ArrowUp,
            "ArrowDown" => platform_api::KeyCode::ArrowDown,
            "ArrowLeft" => platform_api::KeyCode::ArrowLeft,
            "ArrowRight" => platform_api::KeyCode::ArrowRight,
            "Home" => platform_api::KeyCode::Home,
            "End" => platform_api::KeyCode::End,
            "PageUp" => platform_api::KeyCode::PageUp,
            "PageDown" => platform_api::KeyCode::PageDown,
            "Insert" => platform_api::KeyCode::Insert,
            "Delete" => platform_api::KeyCode::Delete,
            _ => single_char_key_code(&key),
        }
    }

    fn single_char_key_code(key: &str) -> platform_api::KeyCode {
        let mut chars = key.chars();
        let Some(ch) = chars.next() else {
            return platform_api::KeyCode::Unknown;
        };
        if chars.next().is_some() {
            return platform_api::KeyCode::Unknown;
        }
        if ch.is_ascii_digit() {
            platform_api::KeyCode::Digit(ch as u8 - b'0')
        } else if ch.is_ascii_alphabetic() {
            platform_api::KeyCode::Letter(ch.to_ascii_uppercase())
        } else {
            platform_api::KeyCode::Unknown
        }
    }

    fn modifiers_for_event(event: &KeyboardEvent) -> platform_api::Modifiers {
        let mut modifiers = platform_api::Modifiers::empty();
        if event.shift_key() {
            modifiers |= platform_api::Modifiers::SHIFT;
        }
        if event.ctrl_key() {
            modifiers |= platform_api::Modifiers::CONTROL;
        }
        if event.alt_key() {
            modifiers |= platform_api::Modifiers::ALT;
        }
        if event.meta_key() {
            modifiers |= platform_api::Modifiers::META;
        }
        modifiers
    }

    fn touch_phase_for_event(name: &str) -> platform_api::TouchPhase {
        match name {
            "pointerdown" => platform_api::TouchPhase::Start,
            "pointerup" => platform_api::TouchPhase::End,
            "pointercancel" => platform_api::TouchPhase::Cancel,
            _ => platform_api::TouchPhase::Move,
        }
    }

    fn event_point(canvas: &HtmlCanvasElement, client_x: f32, client_y: f32) -> (f32, f32) {
        let rect = canvas.get_bounding_client_rect();
        (client_x - rect.left() as f32, client_y - rect.top() as f32)
    }

    fn canvas_backing_size(canvas: &HtmlCanvasElement) -> (u32, u32, f32) {
        let scale = web_sys::window()
            .map(|window| window.device_pixel_ratio() as f32)
            .unwrap_or(1.0)
            .max(1.0);
        let css_w = canvas.client_width().max(1) as f32;
        let css_h = canvas.client_height().max(1) as f32;
        let width = (css_w * scale).round().max(1.0) as u32;
        let height = (css_h * scale).round().max(1.0) as u32;
        (width, height, scale)
    }

    fn render_err(err: gfx::RenderError) -> JsValue {
        JsValue::from_str(&err.to_string())
    }

    fn perf_now() -> f64 {
        web_sys::window()
            .and_then(|window| window.performance())
            .map(|perf| perf.now())
            .unwrap_or(0.0)
    }

    fn average(values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        values.iter().copied().sum::<f64>() / values.len() as f64
    }

    fn percentile(sorted_values: &[f64], percentile: f64) -> f64 {
        if sorted_values.is_empty() {
            return 0.0;
        }
        let index = ((sorted_values.len().saturating_sub(1)) as f64 * percentile)
            .ceil()
            .clamp(0.0, sorted_values.len().saturating_sub(1) as f64) as usize;
        sorted_values[index]
    }

    const WEBGPU_FRAME_STAGES: [(WebGpuFrameStage, &str); 11] = [
        (WebGpuFrameStage::CanvasResize, "canvas_resize"),
        (WebGpuFrameStage::FrameTiming, "frame_timing"),
        (WebGpuFrameStage::BuilderClear, "builder_clear"),
        (WebGpuFrameStage::RouterUpdate, "router_update"),
        (WebGpuFrameStage::RouterDraw, "router_draw"),
        (WebGpuFrameStage::DamageHandoff, "damage_handoff"),
        (WebGpuFrameStage::DrawCoalesce, "draw_coalesce"),
        (WebGpuFrameStage::BeginFrame, "begin_frame"),
        (WebGpuFrameStage::EncodePass, "encode_pass"),
        (WebGpuFrameStage::Submit, "submit"),
        (WebGpuFrameStage::PostSubmit, "post_submit"),
    ];

    fn allocation_stage_begin(
        stages: Option<&WebGpuFrameStageAllocationSummary>,
    ) -> Option<AllocationSnapshot> {
        stages.map(|_| oxide_wasm_alloc_counter::snapshot())
    }

    fn allocation_stage_end(
        stages: Option<&mut WebGpuFrameStageAllocationSummary>,
        stage: WebGpuFrameStage,
        before: Option<AllocationSnapshot>,
    ) {
        if let (Some(stages), Some(before)) = (stages, before) {
            add_allocation_frame(
                frame_stage_summary_mut(stages, stage),
                before,
                oxide_wasm_alloc_counter::snapshot(),
            );
        }
    }

    fn frame_stage_summary_mut(
        stages: &mut WebGpuFrameStageAllocationSummary,
        stage: WebGpuFrameStage,
    ) -> &mut WebGpuAllocationSummary {
        match stage {
            WebGpuFrameStage::CanvasResize => &mut stages.canvas_resize,
            WebGpuFrameStage::FrameTiming => &mut stages.frame_timing,
            WebGpuFrameStage::BuilderClear => &mut stages.builder_clear,
            WebGpuFrameStage::RouterUpdate => &mut stages.router_update,
            WebGpuFrameStage::RouterDraw => &mut stages.router_draw,
            WebGpuFrameStage::DamageHandoff => &mut stages.damage_handoff,
            WebGpuFrameStage::DrawCoalesce => &mut stages.draw_coalesce,
            WebGpuFrameStage::BeginFrame => &mut stages.begin_frame,
            WebGpuFrameStage::EncodePass => &mut stages.encode_pass,
            WebGpuFrameStage::Submit => &mut stages.submit,
            WebGpuFrameStage::PostSubmit => &mut stages.post_submit,
        }
    }

    fn frame_stage_summary(
        stages: &WebGpuFrameStageAllocationSummary,
        stage: WebGpuFrameStage,
    ) -> &WebGpuAllocationSummary {
        match stage {
            WebGpuFrameStage::CanvasResize => &stages.canvas_resize,
            WebGpuFrameStage::FrameTiming => &stages.frame_timing,
            WebGpuFrameStage::BuilderClear => &stages.builder_clear,
            WebGpuFrameStage::RouterUpdate => &stages.router_update,
            WebGpuFrameStage::RouterDraw => &stages.router_draw,
            WebGpuFrameStage::DamageHandoff => &stages.damage_handoff,
            WebGpuFrameStage::DrawCoalesce => &stages.draw_coalesce,
            WebGpuFrameStage::BeginFrame => &stages.begin_frame,
            WebGpuFrameStage::EncodePass => &stages.encode_pass,
            WebGpuFrameStage::Submit => &stages.submit,
            WebGpuFrameStage::PostSubmit => &stages.post_submit,
        }
    }

    fn add_allocation_frame(
        summary: &mut WebGpuAllocationSummary,
        before: AllocationSnapshot,
        after: AllocationSnapshot,
    ) {
        let alloc_count = after.alloc_count.saturating_sub(before.alloc_count);
        let alloc_bytes = after.alloc_bytes.saturating_sub(before.alloc_bytes);
        let dealloc_count = after.dealloc_count.saturating_sub(before.dealloc_count);
        let dealloc_bytes = after.dealloc_bytes.saturating_sub(before.dealloc_bytes);
        let realloc_count = after.realloc_count.saturating_sub(before.realloc_count);
        let realloc_grow_bytes = after.realloc_grow_bytes.saturating_sub(before.realloc_grow_bytes);
        let realloc_shrink_bytes =
            after.realloc_shrink_bytes.saturating_sub(before.realloc_shrink_bytes);
        summary.alloc_count = summary.alloc_count.saturating_add(alloc_count);
        summary.alloc_bytes = summary.alloc_bytes.saturating_add(alloc_bytes);
        summary.dealloc_count = summary.dealloc_count.saturating_add(dealloc_count);
        summary.dealloc_bytes = summary.dealloc_bytes.saturating_add(dealloc_bytes);
        summary.realloc_count = summary.realloc_count.saturating_add(realloc_count);
        summary.realloc_grow_bytes =
            summary.realloc_grow_bytes.saturating_add(realloc_grow_bytes);
        summary.realloc_shrink_bytes =
            summary.realloc_shrink_bytes.saturating_add(realloc_shrink_bytes);
        let frame_alloc_bytes = alloc_bytes.saturating_add(realloc_grow_bytes);
        if alloc_count > 0 || realloc_count > 0 {
            summary.allocating_frames = summary.allocating_frames.saturating_add(1);
            summary.peak_frame_alloc_bytes = summary.peak_frame_alloc_bytes.max(frame_alloc_bytes);
        }
    }

    fn allocation_metrics(summary: &WebGpuAllocationSummary, prefix: &str) -> String {
        let mut out = String::new();
        let key_prefix = if prefix.is_empty() { String::new() } else { format!("{prefix}_") };
        let _ = write!(
            out,
            ";{key_prefix}wasm_alloc_count={};{key_prefix}wasm_alloc_bytes={};{key_prefix}wasm_dealloc_count={};{key_prefix}wasm_dealloc_bytes={};{key_prefix}wasm_realloc_count={};{key_prefix}wasm_realloc_grow_bytes={};{key_prefix}wasm_realloc_shrink_bytes={};{key_prefix}wasm_allocating_frames={};{key_prefix}wasm_peak_frame_alloc_bytes={}",
            summary.alloc_count,
            summary.alloc_bytes,
            summary.dealloc_count,
            summary.dealloc_bytes,
            summary.realloc_count,
            summary.realloc_grow_bytes,
            summary.realloc_shrink_bytes,
            summary.allocating_frames,
            summary.peak_frame_alloc_bytes,
        );
        out
    }

    fn frame_stage_allocation_metrics(stages: &WebGpuFrameStageAllocationSummary) -> String {
        let mut out = String::new();
        for (stage, name) in WEBGPU_FRAME_STAGES {
            let summary = frame_stage_summary(stages, stage);
            let _ = write!(
                out,
                ";wasm_stage_{name}_alloc_count={};wasm_stage_{name}_alloc_bytes={};wasm_stage_{name}_realloc_count={};wasm_stage_{name}_realloc_grow_bytes={};wasm_stage_{name}_peak_frame_alloc_bytes={}",
                summary.alloc_count,
                summary.alloc_bytes,
                summary.realloc_count,
                summary.realloc_grow_bytes,
                summary.peak_frame_alloc_bytes,
            );
        }
        out
    }

    fn frame_pacing_metrics(frame_values_ms: &[f64], prefix: &str) -> String {
        let mut out = String::new();
        let key_prefix = if prefix.is_empty() { String::new() } else { format!("{prefix}_") };
        let denom = frame_values_ms.len().max(1) as f64;
        for refresh_hz in [60_u32, 120_u32] {
            let budget_ms = 1000.0 / refresh_hz as f64;
            let missed_frames =
                frame_values_ms.iter().filter(|sample| **sample > budget_ms).count();
            let hitch_frames =
                frame_values_ms.iter().filter(|sample| **sample > budget_ms * 2.0).count();
            let _ = write!(
                out,
                ";{key_prefix}frame_budget_{refresh_hz}hz_ms={budget_ms:.6};{key_prefix}missed_frames_{refresh_hz}hz={missed_frames};{key_prefix}missed_frame_ratio_{refresh_hz}hz={:.6};{key_prefix}hitch_frames_{refresh_hz}hz={hitch_frames};{key_prefix}hitch_ratio_{refresh_hz}hz={:.6}",
                missed_frames as f64 / denom,
                hitch_frames as f64 / denom,
            );
        }
        out
    }

    fn renderer_stats_metrics(stats: WebRendererStats, prefix: &str) -> String {
        let mut out = String::new();
        let key_prefix = if prefix.is_empty() { String::new() } else { format!("{prefix}_") };
        let _ = write!(
            out,
            ";{key_prefix}draws={};{key_prefix}draw_items={};{key_prefix}draw_pipeline_binds={};{key_prefix}draw_bind_group_binds={};{key_prefix}draw_scissor_sets={};{key_prefix}solid_tris={};{key_prefix}image_draws={};{key_prefix}image_mesh_draws={};{key_prefix}nine_slice_draws={};{key_prefix}glyph_quads={};{key_prefix}sdf_glyph_quads={};{key_prefix}clip_depth_peak={};{key_prefix}damage_rects={};{key_prefix}layer_draws={};{key_prefix}scene3d_draws={};{key_prefix}id_mask_draws={};{key_prefix}backdrop_draws={};{key_prefix}visual_effect_draws={};{key_prefix}effect_uniform_writes={};{key_prefix}effect_uniform_bytes={};{key_prefix}effect_uniform_slots={};{key_prefix}spinner_draws={};{key_prefix}camera_bg_draws={};{key_prefix}render_passes={};{key_prefix}clear_passes={};{key_prefix}draw_passes={};{key_prefix}scene3d_passes={};{key_prefix}scene3d_overlay_passes={};{key_prefix}id_mask_raster_passes={};{key_prefix}id_mask_field_seed_passes={};{key_prefix}id_mask_field_jump_passes={};{key_prefix}id_mask_compositor_passes={};{key_prefix}present_passes={};{key_prefix}texture_copies={};{key_prefix}command_buffers={};{key_prefix}gpu_timestamp_query_supported={};{key_prefix}gpu_timestamp_frame_id={};{key_prefix}gpu_timestamp_passes={};{key_prefix}gpu_timestamp_total_ns={};{key_prefix}gpu_timestamp_clear_ns={};{key_prefix}gpu_timestamp_draw_ns={};{key_prefix}gpu_timestamp_scene3d_ns={};{key_prefix}gpu_timestamp_scene3d_overlay_ns={};{key_prefix}gpu_timestamp_id_mask_raster_ns={};{key_prefix}gpu_timestamp_id_mask_field_seed_ns={};{key_prefix}gpu_timestamp_id_mask_field_jump_ns={};{key_prefix}gpu_timestamp_id_mask_compositor_ns={};{key_prefix}gpu_timestamp_present_ns={};{key_prefix}gpu_timestamp_max_pass_ns={};{key_prefix}gpu_timestamp_readback_skips={};{key_prefix}gpu_timestamp_readback_interval={};{key_prefix}buffer_upload_bytes={};{key_prefix}texture_upload_bytes={};{key_prefix}buffer_grows={};{key_prefix}texture_creates={};{key_prefix}bind_group_creates={};{key_prefix}pipeline_creates={};{key_prefix}sampler_creates={};{key_prefix}mesh3d_creates={};{key_prefix}draw_buffer_grows={};{key_prefix}image_texture_creates={};{key_prefix}image_bind_group_creates={};{key_prefix}target_texture_creates={};{key_prefix}target_bind_group_creates={};{key_prefix}scene3d_buffer_grows={};{key_prefix}scene3d_bind_group_creates={};{key_prefix}effect_buffer_grows={};{key_prefix}effect_bind_group_creates={};{key_prefix}id_mask_texture_creates={};{key_prefix}id_mask_buffer_grows={};{key_prefix}id_mask_bind_group_creates={};{key_prefix}image_upload_temp_allocs={};{key_prefix}image_upload_temp_bytes={};{key_prefix}image_upload_scratch_bytes={};{key_prefix}image_upload_scratch_grows={};{key_prefix}cpu_scratch_bytes={};{key_prefix}cpu_scratch_grows={};{key_prefix}cpu_scratch_growth_bytes={};{key_prefix}cpu_draw_scratch_bytes={};{key_prefix}cpu_draw_scratch_grows={};{key_prefix}cpu_draw_scratch_growth_bytes={};{key_prefix}cpu_scene3d_scratch_bytes={};{key_prefix}cpu_scene3d_scratch_grows={};{key_prefix}cpu_scene3d_scratch_growth_bytes={};{key_prefix}cpu_effect_scratch_bytes={};{key_prefix}cpu_effect_scratch_grows={};{key_prefix}cpu_effect_scratch_growth_bytes={};{key_prefix}cpu_id_mask_scratch_bytes={};{key_prefix}cpu_id_mask_scratch_grows={};{key_prefix}cpu_id_mask_scratch_growth_bytes={};{key_prefix}cpu_image_upload_scratch_bytes={};{key_prefix}cpu_image_upload_scratch_grows={};{key_prefix}cpu_image_upload_scratch_growth_bytes={};{key_prefix}cpu_resource_table_scratch_bytes={};{key_prefix}cpu_resource_table_scratch_grows={};{key_prefix}cpu_resource_table_scratch_growth_bytes={}",
            stats.draws,
            stats.draw_items,
            stats.draw_pipeline_binds,
            stats.draw_bind_group_binds,
            stats.draw_scissor_sets,
            stats.solid_tris,
            stats.image_draws,
            stats.image_mesh_draws,
            stats.nine_slice_draws,
            stats.glyph_quads,
            stats.sdf_glyph_quads,
            stats.clip_depth_peak,
            stats.damage_rects,
            stats.layer_draws,
            stats.scene3d_draws,
            stats.id_mask_draws,
            stats.backdrop_draws,
            stats.visual_effect_draws,
            stats.effect_uniform_writes,
            stats.effect_uniform_bytes,
            stats.effect_uniform_slots,
            stats.spinner_draws,
            stats.camera_bg_draws,
            stats.render_passes,
            stats.clear_passes,
            stats.draw_passes,
            stats.scene3d_passes,
            stats.scene3d_overlay_passes,
            stats.id_mask_raster_passes,
            stats.id_mask_field_seed_passes,
            stats.id_mask_field_jump_passes,
            stats.id_mask_compositor_passes,
            stats.present_passes,
            stats.texture_copies,
            stats.command_buffers,
            u32::from(stats.gpu_timestamp_query_supported),
            stats.gpu_timestamp_frame_id,
            stats.gpu_timestamp_passes,
            stats.gpu_timestamp_total_ns,
            stats.gpu_timestamp_clear_ns,
            stats.gpu_timestamp_draw_ns,
            stats.gpu_timestamp_scene3d_ns,
            stats.gpu_timestamp_scene3d_overlay_ns,
            stats.gpu_timestamp_id_mask_raster_ns,
            stats.gpu_timestamp_id_mask_field_seed_ns,
            stats.gpu_timestamp_id_mask_field_jump_ns,
            stats.gpu_timestamp_id_mask_compositor_ns,
            stats.gpu_timestamp_present_ns,
            stats.gpu_timestamp_max_pass_ns,
            stats.gpu_timestamp_readback_skips,
            stats.gpu_timestamp_readback_interval,
            stats.buffer_upload_bytes,
            stats.texture_upload_bytes,
            stats.buffer_grows,
            stats.texture_creates,
            stats.bind_group_creates,
            stats.pipeline_creates,
            stats.sampler_creates,
            stats.mesh3d_creates,
            stats.draw_buffer_grows,
            stats.image_texture_creates,
            stats.image_bind_group_creates,
            stats.target_texture_creates,
            stats.target_bind_group_creates,
            stats.scene3d_buffer_grows,
            stats.scene3d_bind_group_creates,
            stats.effect_buffer_grows,
            stats.effect_bind_group_creates,
            stats.id_mask_texture_creates,
            stats.id_mask_buffer_grows,
            stats.id_mask_bind_group_creates,
            stats.image_upload_temp_allocs,
            stats.image_upload_temp_bytes,
            stats.image_upload_scratch_bytes,
            stats.image_upload_scratch_grows,
            stats.cpu_scratch_bytes,
            stats.cpu_scratch_grows,
            stats.cpu_scratch_growth_bytes,
            stats.cpu_draw_scratch_bytes,
            stats.cpu_draw_scratch_grows,
            stats.cpu_draw_scratch_growth_bytes,
            stats.cpu_scene3d_scratch_bytes,
            stats.cpu_scene3d_scratch_grows,
            stats.cpu_scene3d_scratch_growth_bytes,
            stats.cpu_effect_scratch_bytes,
            stats.cpu_effect_scratch_grows,
            stats.cpu_effect_scratch_growth_bytes,
            stats.cpu_id_mask_scratch_bytes,
            stats.cpu_id_mask_scratch_grows,
            stats.cpu_id_mask_scratch_growth_bytes,
            stats.cpu_image_upload_scratch_bytes,
            stats.cpu_image_upload_scratch_grows,
            stats.cpu_image_upload_scratch_growth_bytes,
            stats.cpu_resource_table_scratch_bytes,
            stats.cpu_resource_table_scratch_grows,
            stats.cpu_resource_table_scratch_growth_bytes,
        );
        out
    }

    fn js_detail(value: &JsValue) -> String {
        value
            .as_string()
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| String::from("unknown"))
    }

    fn permission_status_name(status: platform_api::PermissionStatus) -> &'static str {
        match status {
            platform_api::PermissionStatus::NotDetermined => "not-determined",
            platform_api::PermissionStatus::Denied => "denied",
            platform_api::PermissionStatus::Limited => "limited",
            platform_api::PermissionStatus::Authorized => "authorized",
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_host::{
    platform_smoke_report, start_oxide, start_oxide_async, webgpu_smoke_report,
    webgpu_timing_report, OxideWebApp,
};

#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub const fn host_web_requires_wasm32() -> &'static str {
    "oxide-host-web exports browser entry points only for wasm32"
}
