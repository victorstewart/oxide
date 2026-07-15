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
    use futures_util::future::join_all;
    use oxide_image_store as images;
    use oxide_platform_api as platform_api;
    use oxide_platform_api::Platform;
    use oxide_renderer_api as gfx;
    use oxide_renderer_api::Renderer;
    use oxide_renderer_web::{
        id_mask_compositor, neon_marker, scene3d, BrowserRenderer, WebGpuCpuSubmitTimingSample,
        WebGpuTimestampSample, WebIdMaskSnapshotReadback, WebRendererStats,
    };
    use oxide_test_scenes as scenes;
    use oxide_text as text;
    use oxide_ui_core as ui;
    use oxide_wasm_alloc_counter::AllocationSnapshot;
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::fmt::Write;
    use std::rc::Rc;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::{closure::Closure, JsCast};
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{
        CompositionEvent, Event, EventTarget, HtmlCanvasElement, HtmlElement, HtmlTextAreaElement,
        InputEvent, KeyboardEvent, MutationObserver, MutationObserverInit, PointerEvent,
        ResizeObserver, ResizeObserverEntry, WheelEvent,
    };

    const DEFAULT_FONT_BYTES: &[u8] =
        include_bytes!("../../../../crates/ui-core/assets/Asap-Regular.ttf");
    const C46_LATIN_FONT_BYTES: &[u8] =
        include_bytes!("../../../../crates/text/tests/fixtures/test_text_latin.ttf");
    const C46_CJK_FONT_BYTES: &[u8] =
        include_bytes!("../../../../crates/text/tests/fixtures/test_text_cjk.ttf");
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
    const WEBGPU_CLEAN_LAYER_GLYPHS: usize = 96;
    const WEBGPU_CLEAN_LAYER_IMAGE_TILES: usize = 144;
    const WEBGPU_CLEAN_LAYER_IMAGE_COLUMNS: usize = 16;
    const WEBGPU_EFFECT_UNIFORM_BACKDROPS: usize = 48;
    const WEBGPU_BACKDROP_BATCH_BACKDROPS: usize = 12;
    const WEBGPU_COMMAND_FAMILY_SDF_GLYPHS: usize = 36;
    const WEBGPU_COMMAND_FAMILY_SDF_RUNS: usize = 8;
    const WEBGPU_COMMAND_FAMILY_REPEATS: usize = 64;
    const WEBGPU_COMMAND_FAMILY_COLUMNS: usize = 8;
    const WEBGPU_GLYPH_RUN_RUNS: usize = 64;
    const WEBGPU_GLYPH_RUN_GLYPHS_PER_RUN: usize = 8;
    const WEBGPU_GLYPH_RUN_SDF_RUNS: usize = 32;
    const WEBGPU_GEOMETRY_QUADS: usize = 10_000;
    const WEBGPU_GEOMETRY_LARGE_VERTICES: usize = 70_002;
    const WEBGPU_DRAW_STATE_CACHE_DRAWS: usize = 1024;
    const WEBGPU_DRAW_STATE_CACHE_COLUMNS: usize = 32;
    const WEBGPU_DRAW_ITEM_COALESCE_EXPECTED_ITEMS: usize = 1;
    const WEBGPU_NEON_MARKERS: usize = 64;
    const WEBGPU_ARCHITECTURE_NEON_MARKERS: usize = 1_024;
    const WEBGPU_NEON_MARKER_COLUMNS: usize = 8;
    const WEBGPU_DIRECT_SURFACE_DRAWS: usize = 384;
    const WEBGPU_DIRECT_SURFACE_COLUMNS: usize = 24;
    const WEBGPU_SCENE3D_STRESS_INSTANCES: usize = 96;
    const WEBGPU_TIMESTAMP_SETTLE_RAFS: u32 = 60;
    const WEBGPU_PREPARED_CHUNKS: usize = 256;
    const WEBGPU_PREPARED_DRAW_COUNTS: [usize; 4] = [8, 16, 32, 64];
    const WEBGPU_DYNAMIC_PROPERTY_NODES: usize = 300;
    const WEBGPU_LOCAL_LAYER_CARDS: usize = 100;
    const WEBGPU_LOCAL_LAYER_COLUMNS: usize = 10;
    const WEBGPU_LOCAL_LAYER_WIDTH: f32 = 72.0;
    const WEBGPU_LOCAL_LAYER_HEIGHT: f32 = 40.0;
    const WEBGPU_LOCAL_LAYER_GAP: f32 = 8.0;
    const WEBGPU_LOCAL_LAYER_CLOCK_WARMUP_DRAWS: usize = 64;
    const WEBGPU_LOCAL_LAYER_CLOCK_WARMUP_FRAMES: usize = 12;
    const WEBGPU_LOCAL_LAYER_GPU_POSTROLL_FRAMES: usize = 1;

    struct WebUploader {
        renderer: Rc<RefCell<BrowserRenderer>>,
    }

    struct BorrowedWebUploader<'a> {
        renderer: &'a mut BrowserRenderer,
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

        fn append_a8(
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
                .image_append_a8(handle, x, y, width, height, data, row_bytes);
        }

        fn release_a8(&mut self, handle: gfx::ImageHandle) {
            let _ = self.renderer.borrow_mut().image_release(handle);
        }
    }

    impl ui::elements::ImageUploader for BorrowedWebUploader<'_> {
        fn create_a8(
            &mut self,
            width: u32,
            height: u32,
            data: &[u8],
            row_bytes: usize,
        ) -> gfx::ImageHandle {
            self.renderer.image_create_a8(width, height, data, row_bytes)
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
            self.renderer.image_update_a8(handle, x, y, width, height, data, row_bytes);
        }

        fn release_a8(&mut self, handle: gfx::ImageHandle) {
            let _ = self.renderer.image_release(handle);
        }
    }

    #[derive(Clone, Copy)]
    struct CanvasMetrics {
        physical_width: u32,
        physical_height: u32,
        css_width: f32,
        css_height: f32,
        scale: f32,
        left: f32,
        top: f32,
    }

    struct AppState {
        canvas: HtmlCanvasElement,
        canvas_metrics: CanvasMetrics,
        canvas_metrics_dirty: bool,
        ime_textarea: HtmlTextAreaElement,
        renderer: Rc<RefCell<BrowserRenderer>>,
        router: scenes::Router<WebUploader>,
        builder: ui::DrawListBuilder,
        damage_rects: Vec<gfx::RectI>,
        coalesce_items: Vec<gfx::DrawCmd>,
        bench_resources: Option<WebGpuUploadBenchResources>,
        geometry_bench_resources: Option<WebGpuGeometryBenchResources>,
        glyph_matrix_resources: Option<WebGpuGlyphMatrixResources>,
        last_timestamp_ms: f64,
        frame_time_remainder_ms: f64,
        ime_focused: bool,
        ime_composing: bool,
        ime_skip_next_input: bool,
        raf: Option<Closure<dyn FnMut(f64)>>,
        raf_pending: bool,
        pointer_anticipation: bool,
        resize_observer: Option<ResizeObserver>,
        resize_observer_callback: Option<Closure<dyn FnMut(js_sys::Array)>>,
        mutation_observer: Option<MutationObserver>,
        mutation_observer_callback: Option<Closure<dyn FnMut(js_sys::Array)>>,
        listeners: Vec<Closure<dyn FnMut(Event)>>,
        frame_dirty: bool,
        raf_callbacks: u64,
        raf_requests: u64,
        invalidations: u64,
        canvas_metric_reads: u64,
        idle_skipped_frames: u64,
        submitted_frames: u64,
        direct_capture_active: bool,
        raf_gpu_start_frame_id: u64,
        timestamp_samples: Vec<WebGpuTimestampSample>,
    }

    impl AppState {
        fn refresh_canvas_metrics(&mut self) -> Result<(), JsValue> {
            if !self.canvas_metrics_dirty {
                return Ok(());
            }
            let metrics = measure_canvas_metrics(&self.canvas);
            self.canvas_metric_reads = self.canvas_metric_reads.saturating_add(1);
            self.renderer
                .borrow_mut()
                .resize(metrics.physical_width, metrics.physical_height, metrics.scale)
                .map_err(render_err)?;
            self.canvas_metrics = metrics;
            self.canvas_metrics_dirty = false;
            Ok(())
        }

        fn frame_at(&mut self, timestamp_ms: f64) -> Result<(), JsValue> {
            self.frame_at_inner(timestamp_ms, None, None)
        }

        fn frame_at_profiled(
            &mut self,
            timestamp_ms: f64,
            allocation_stages: &mut WebGpuFrameStageAllocationSummary,
        ) -> Result<(), JsValue> {
            self.frame_at_inner(timestamp_ms, Some(allocation_stages), None)
        }

        fn frame_at_inner(
            &mut self,
            timestamp_ms: f64,
            mut allocation_stages: Option<&mut WebGpuFrameStageAllocationSummary>,
            mut timing_sample: Option<&mut WebGpuFrameStageTimingSample>,
        ) -> Result<(), JsValue> {
            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            let timing_before = timing_stage_begin(timing_sample.as_deref());
            self.refresh_canvas_metrics()?;
            let metrics = self.canvas_metrics;
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::CanvasResize,
                stage_before,
            );
            timing_stage_end(timing_sample.as_deref_mut(), WebGpuFrameStage::CanvasResize, timing_before);

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            let timing_before = timing_stage_begin(timing_sample.as_deref());
            let timestamp_ms = if timestamp_ms.is_finite() { timestamp_ms.max(0.0) } else { 0.0 };
            let now_ms = timestamp_ms.floor() as u64;
            let dt_ms = if self.last_timestamp_ms == 0.0 {
                16
            } else {
                let elapsed_ms = (timestamp_ms - self.last_timestamp_ms).max(0.0);
                let accumulated_ms = elapsed_ms + self.frame_time_remainder_ms;
                let whole_ms = accumulated_ms.floor().min(u32::MAX as f64);
                self.frame_time_remainder_ms = accumulated_ms - whole_ms;
                whole_ms as u32
            };
            self.last_timestamp_ms = timestamp_ms;
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::FrameTiming,
                stage_before,
            );
            timing_stage_end(timing_sample.as_deref_mut(), WebGpuFrameStage::FrameTiming, timing_before);

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            let timing_before = timing_stage_begin(timing_sample.as_deref());
            self.builder.clear();
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::BuilderClear,
                stage_before,
            );
            timing_stage_end(timing_sample.as_deref_mut(), WebGpuFrameStage::BuilderClear, timing_before);
            let viewport = gfx::RectF::new(
                0.0,
                0.0,
                metrics.physical_width as f32 / metrics.scale.max(1.0),
                metrics.physical_height as f32 / metrics.scale.max(1.0),
            );

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            let timing_before = timing_stage_begin(timing_sample.as_deref());
            self.router.update(now_ms, dt_ms);
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::RouterUpdate,
                stage_before,
            );
            timing_stage_end(timing_sample.as_deref_mut(), WebGpuFrameStage::RouterUpdate, timing_before);

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            let timing_before = timing_stage_begin(timing_sample.as_deref());
            self.router.draw(viewport, metrics.scale, &mut self.builder);
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::RouterDraw,
                stage_before,
            );
            timing_stage_end(timing_sample.as_deref_mut(), WebGpuFrameStage::RouterDraw, timing_before);

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            let timing_before = timing_stage_begin(timing_sample.as_deref());
            self.router.take_damage_into(&mut self.damage_rects);
            let mut damage = gfx::Damage { rects: core::mem::take(&mut self.damage_rects) };
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::DamageHandoff,
                stage_before,
            );
            timing_stage_end(timing_sample.as_deref_mut(), WebGpuFrameStage::DamageHandoff, timing_before);

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            let timing_before = timing_stage_begin(timing_sample.as_deref());
            ui::coalesce_adjacent_draws_reuse(
                self.builder.drawlist_mut(),
                &mut self.coalesce_items,
            );
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::DrawCoalesce,
                stage_before,
            );
            timing_stage_end(timing_sample.as_deref_mut(), WebGpuFrameStage::DrawCoalesce, timing_before);

            let mut renderer = self.renderer.borrow_mut();
            renderer.set_animation_time_ms(timestamp_ms);
            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            let timing_before = timing_stage_begin(timing_sample.as_deref());
            let token = renderer.begin_frame(&gfx::FrameTarget, Some(&damage));
            self.damage_rects = core::mem::take(&mut damage.rects);
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::BeginFrame,
                stage_before,
            );
            timing_stage_end(timing_sample.as_deref_mut(), WebGpuFrameStage::BeginFrame, timing_before);

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            let timing_before = timing_stage_begin(timing_sample.as_deref());
            renderer.encode_pass(self.builder.drawlist());
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::EncodePass,
                stage_before,
            );
            timing_stage_end(timing_sample.as_deref_mut(), WebGpuFrameStage::EncodePass, timing_before);

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            let timing_before = timing_stage_begin(timing_sample.as_deref());
            renderer.set_cpu_submit_timing_enabled_for_benchmark(timing_sample.is_some());
            let submit_result = renderer.submit(token);
            let cpu_submit_timing = renderer.last_cpu_submit_timing();
            renderer.set_cpu_submit_timing_enabled_for_benchmark(false);
            submit_result.map_err(render_err)?;
            if let Some(timing_sample) = timing_sample.as_deref_mut() {
                timing_sample.cpu_submit = cpu_submit_timing;
            }
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::Submit,
                stage_before,
            );
            timing_stage_end(timing_sample.as_deref_mut(), WebGpuFrameStage::Submit, timing_before);

            let stage_before = allocation_stage_begin(allocation_stages.as_deref());
            let timing_before = timing_stage_begin(timing_sample.as_deref());
            self.submitted_frames = self.submitted_frames.saturating_add(1);
            self.frame_dirty = false;
            allocation_stage_end(
                allocation_stages.as_deref_mut(),
                WebGpuFrameStage::PostSubmit,
                stage_before,
            );
            timing_stage_end(timing_sample.as_deref_mut(), WebGpuFrameStage::PostSubmit, timing_before);
            Ok(())
        }

        fn mark_frame_dirty(&mut self) {
            self.frame_dirty = true;
            self.invalidations = self.invalidations.saturating_add(1);
        }

        fn mark_canvas_metrics_dirty(&mut self) {
            self.canvas_metrics_dirty = true;
            self.mark_frame_dirty();
        }

        fn should_request_next_frame(&self) -> bool {
            self.frame_dirty || self.canvas_metrics_dirty || self.router.wants_next_frame()
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
            let canvas = canvas_by_id(canvas_id)?;
            let metrics = measure_canvas_metrics(&canvas);
            canvas.set_width(metrics.physical_width);
            canvas.set_height(metrics.physical_height);
            let renderer = BrowserRenderer::from_canvas_webgpu(canvas).await.map_err(render_err)?;
            Self::new_with_renderer(renderer, metrics)
        }

        fn new_with_renderer(
            mut renderer: BrowserRenderer,
            metrics: CanvasMetrics,
        ) -> Result<OxideWebApp, JsValue> {
            let canvas = renderer.canvas();
            let ime_textarea = create_ime_textarea(&canvas)?;
            renderer
                .resize(metrics.physical_width, metrics.physical_height, metrics.scale)
                .map_err(render_err)?;

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
                canvas_metrics: metrics,
                canvas_metrics_dirty: false,
                ime_textarea,
                renderer,
                router,
                builder: ui::DrawListBuilder::new(),
                damage_rects: Vec::new(),
                coalesce_items: Vec::new(),
                bench_resources: None,
                geometry_bench_resources: None,
                glyph_matrix_resources: None,
                last_timestamp_ms: 0.0,
                frame_time_remainder_ms: 0.0,
                ime_focused: false,
                ime_composing: false,
                ime_skip_next_input: false,
                raf: None,
                raf_pending: false,
                pointer_anticipation: false,
                resize_observer: None,
                resize_observer_callback: None,
                mutation_observer: None,
                mutation_observer_callback: None,
                listeners: Vec::new(),
                frame_dirty: true,
                raf_callbacks: 0,
                raf_requests: 0,
                invalidations: 1,
                canvas_metric_reads: 1,
                idle_skipped_frames: 0,
                submitted_frames: 0,
                direct_capture_active: false,
                raf_gpu_start_frame_id: 0,
                timestamp_samples: Vec::new(),
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
                    state.raf_callbacks = state.raf_callbacks.saturating_add(1);
                    if state.should_request_next_frame() {
                        let _ = state.frame_at(timestamp_ms);
                    } else {
                        state.idle_skipped_frames = state.idle_skipped_frames.saturating_add(1);
                    }
                }
                let request_anticipation = {
                    let mut state = state_for_frame.borrow_mut();
                    let request = state.pointer_anticipation;
                    state.pointer_anticipation = false;
                    request
                };
                if request_anticipation || state_for_frame.borrow().should_request_next_frame() {
                    request_next_frame(&state_for_frame);
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

        pub fn frame_at_timestamp_unprofiled(&self, timestamp_ms: f64) -> Result<(), JsValue> {
            self.state.borrow_mut().frame_at(timestamp_ms)
        }

        pub fn frame_at_timestamp_profiled(&self, timestamp_ms: f64) -> Result<String, JsValue> {
            let mut timing = WebGpuFrameStageTimingSample::default();
            let frame_start = perf_now();
            self.state
                .borrow_mut()
                .frame_at_inner(timestamp_ms, None, Some(&mut timing))?;
            timing.total_ms = (perf_now() - frame_start).max(0.0);
            Ok(frame_stage_timing_metrics(&timing))
        }

        pub fn begin_raf_gpu_timestamp_capture(&self) {
            let renderer = self.state.borrow().renderer.clone();
            let mut renderer = renderer.borrow_mut();
            renderer.collect_timestamp_readbacks();
            renderer.clear_completed_timestamp_samples();
            renderer.set_timestamp_readback_interval_for_benchmark(1);
            let frame_id = renderer.last_stats().frame_id;
            drop(renderer);
            let mut state = self.state.borrow_mut();
            state.raf_gpu_start_frame_id = frame_id;
            state.timestamp_samples.clear();
            if state.timestamp_samples.capacity() < 2_048 {
                state.timestamp_samples.reserve(2_048);
            }
        }

        pub async fn finish_raf_gpu_timestamp_capture(&self) -> Result<String, JsValue> {
            let renderer = self.state.borrow().renderer.clone();
            let start_frame_id = self.state.borrow().raf_gpu_start_frame_id;
            let settle = settle_renderer_timestamps_diagnostic(&renderer, start_frame_id).await?;
            let mut state = self.state.borrow_mut();
            {
                let mut renderer = renderer.borrow_mut();
                renderer.drain_completed_timestamp_samples_into(&mut state.timestamp_samples);
                renderer.set_timestamp_readback_interval_for_benchmark(8);
            }
            state.timestamp_samples.retain(|sample| sample.frame_id > start_frame_id);
            Ok(timestamp_samples_json(&state.timestamp_samples, settle))
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

        pub fn render_webgpu_glyph_snapshot(&self) -> Result<String, JsValue> {
            self.state.borrow_mut().direct_capture_active = true;
            let renderer = self.ensure_upload_bench_resources()?;
            self.with_upload_bench_resources(|renderer, resources| {
                resources.glyph_frame(renderer, true)?;
                resources.glyph_run_frame(renderer)
            })?;
            let stats = renderer.borrow().last_stats();
            Ok(format!(
                "glyph_quads={};sdf_glyph_quads={};frame_id={};width={};height={}",
                stats.glyph_quads,
                stats.sdf_glyph_quads,
                stats.frame_id,
                stats.width,
                stats.height,
            ))
        }

        pub fn render_webgpu_rrect_snapshot(
            &self,
            width: u32,
            height: u32,
            dpr: f32,
        ) -> Result<String, JsValue> {
            self.state.borrow_mut().direct_capture_active = true;
            let renderer = self.ensure_upload_bench_resources()?;
            self.with_upload_bench_resources(|renderer, resources| {
                resources.rrect_capture_frame(renderer, width, height, dpr)
            })?;
            let stats = renderer.borrow().last_stats();
            Ok(format!(
                "rrect_instances={};rrect_triangles={};solid_tris={};frame_id={};width={};height={};dpr={:.1}",
                stats.rrect_instances,
                stats.rrect_triangles,
                stats.solid_tris,
                stats.frame_id,
                stats.width,
                stats.height,
                dpr,
            ))
        }

        pub fn render_webgpu_image_snapshot(
            &self,
            width: u32,
            height: u32,
            dpr: f32,
        ) -> Result<String, JsValue> {
            self.state.borrow_mut().direct_capture_active = true;
            let renderer = self.ensure_upload_bench_resources()?;
            self.with_upload_bench_resources(|renderer, resources| {
                resources.image_capture_frame(renderer, width, height, dpr)
            })?;
            let stats = renderer.borrow().last_stats();
            Ok(format!(
                "image_instances={};image_triangles={};image_instance_bytes={};draws={};binds={};frame_id={};width={};height={};dpr={:.1}",
                stats.image_instances,
                stats.image_triangles,
                stats.image_instance_bytes,
                stats.draws,
                stats.draw_bind_group_binds,
                stats.frame_id,
                stats.width,
                stats.height,
                dpr,
            ))
        }

        pub fn render_webgpu_nine_slice_snapshot(
            &self,
            width: u32,
            height: u32,
            dpr: f32,
        ) -> Result<String, JsValue> {
            self.state.borrow_mut().direct_capture_active = true;
            let renderer = self.ensure_upload_bench_resources()?;
            self.with_upload_bench_resources(|renderer, resources| {
                resources.nine_slice_capture_frame(renderer, width, height, dpr)
            })?;
            let stats = renderer.borrow().last_stats();
            Ok(format!(
                "nine_slice_instances={};nine_slice_triangles={};nine_slice_instance_bytes={};draws={};binds={};frame_id={};width={};height={};dpr={:.1}",
                stats.nine_slice_instances,
                stats.nine_slice_triangles,
                stats.nine_slice_instance_bytes,
                stats.draws,
                stats.draw_bind_group_binds,
                stats.frame_id,
                stats.width,
                stats.height,
                dpr,
            ))
        }

        pub fn render_webgpu_spinner_snapshot(
            &self,
            phase_ms: f64,
            reference: bool,
            width: u32,
            height: u32,
            dpr: f32,
        ) -> Result<String, JsValue> {
            self.state.borrow_mut().direct_capture_active = true;
            let renderer = self.ensure_upload_bench_resources()?;
            self.with_upload_bench_resources(|renderer, resources| {
                resources.spinner_capture_frame(
                    renderer,
                    phase_ms,
                    reference,
                    width,
                    height,
                    dpr,
                )
            })?;
            let stats = renderer.borrow().last_stats();
            Ok(format!(
                "reference={};phase_ms={:.3};spinner_instances={};spinner_triangles={};spinner_instance_bytes={};rrect_instances={};rrect_triangles={};rrect_instance_bytes={};draws={};frame_id={};width={};height={};dpr={:.1}",
                u32::from(reference),
                phase_ms,
                stats.spinner_instances,
                stats.spinner_triangles,
                stats.spinner_instance_bytes,
                stats.rrect_instances,
                stats.rrect_triangles,
                stats.rrect_instance_bytes,
                stats.draws,
                stats.frame_id,
                stats.width,
                stats.height,
                dpr,
            ))
        }

        pub fn render_webgpu_neon_marker_snapshot(
            &self,
            width: u32,
            height: u32,
            dpr: f32,
        ) -> Result<String, JsValue> {
            self.state.borrow_mut().direct_capture_active = true;
            let renderer = self.ensure_upload_bench_resources()?;
            self.with_upload_bench_resources(|renderer, resources| {
                resources.neon_marker_capture_frame(renderer, width, height, dpr)
            })?;
            let stats = renderer.borrow().last_stats();
            Ok(format!(
                "neon_marker_instances={};neon_marker_triangles={};neon_marker_instance_bytes={};rrect_instances={};solid_tris={};draws={};frame_id={};width={};height={};dpr={:.1}",
                stats.neon_marker_instances,
                stats.neon_marker_triangles,
                stats.neon_marker_instance_bytes,
                stats.rrect_instances,
                stats.solid_tris,
                stats.draws,
                stats.frame_id,
                stats.width,
                stats.height,
                dpr,
            ))
        }

        pub fn render_webgpu_prepared_snapshot(
            &self,
            flat_control: bool,
        ) -> Result<String, JsValue> {
            self.state.borrow_mut().direct_capture_active = true;
            let renderer = self.ensure_upload_bench_resources()?;
            let (image, atlas) = {
                let state = self.state.borrow();
                let resources = state
                    .bench_resources
                    .as_ref()
                    .ok_or_else(|| JsValue::from_str("missing WebGPU prepared resources"))?;
                (resources.image, resources.glyph_atlas)
            };
            let snapshot = webgpu_prepared_snapshot(image, atlas, 1)?;
            let mut flat = gfx::DrawList::default();
            {
                let mut renderer = renderer.borrow_mut();
                renderer.resize(1_200, 800, 1.0).map_err(render_err)?;
                renderer.set_prepared_bundle_min_draws_for_benchmark(8);
            }
            webgpu_prepared_frame(&renderer, &snapshot, flat_control, &mut flat)?;
            let stats = renderer.borrow().last_stats();
            Ok(format!(
                "flat_control={};draws={};bundle_replays={};prepared_direct_draws={};frame_id={};width={};height={}",
                u32::from(flat_control),
                stats.draws,
                stats.render_bundle_replays,
                stats.prepared_direct_draws,
                stats.frame_id,
                stats.width,
                stats.height,
            ))
        }

        pub fn render_webgpu_local_layers_c30(&self) -> Result<String, JsValue>
        {
            self.state.borrow_mut().direct_capture_active = true;
            let renderer = self.ensure_upload_bench_resources()?;
            let (dirty, clean) = webgpu_local_layer_edge_snapshots()?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.resize(512, 512, 2.0).map_err(render_err)?;
            }
            webgpu_local_layer_frame(&renderer, &dirty)?;
            webgpu_local_layer_frame(&renderer, &clean)?;
            let stats = renderer.borrow().last_stats();
            Ok(format!(
                "layers={};hits={};misses={};passes={};layer_bytes={};frame_id={};width={};height={}",
                stats.layer_draws,
                stats.layer_cache_hits,
                stats.layer_cache_misses,
                stats.layer_passes,
                stats.gpu_layer_texture_bytes,
                stats.frame_id,
                stats.width,
                stats.height,
            ))
        }

        pub fn render_webgpu_dynamic_property_snapshot(
            &self,
            phase: u32,
            full_affine: bool,
            flat_control: bool,
        ) -> Result<String, JsValue> {
            self.state.borrow_mut().direct_capture_active = true;
            let renderer = self.ensure_upload_bench_resources()?;
            let snapshot = {
                let mut state = self.state.borrow_mut();
                let resources = state
                    .bench_resources
                    .as_mut()
                    .ok_or_else(|| JsValue::from_str("missing WebGPU dynamic property resources"))?;
                resources.dynamic_property_snapshot(phase as usize, full_affine)?
            };
            let mut flat = gfx::DrawList::default();
            renderer.borrow_mut().resize(1_200, 800, 1.0).map_err(render_err)?;
            webgpu_prepared_frame(&renderer, &snapshot, flat_control, &mut flat)?;
            let stats = renderer.borrow().last_stats();
            Ok(format!(
                "phase={};full_affine={};flat_control={};draws={};property_upload_bytes={};property_records_updated={};geometry_upload_bytes={};commands_copied={};frame_id={};width={};height={}",
                phase & 1,
                u32::from(full_affine),
                u32::from(flat_control),
                stats.draws,
                stats.property_upload_bytes,
                stats.property_records_updated,
                stats.buffer_upload_bytes,
                stats.commands_copied,
                stats.frame_id,
                stats.width,
                stats.height,
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

        pub async fn bench_cpu_submit_samples(
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
            for _warmup in 0..512 {
                self.state.borrow_mut().frame_at(timestamp)?;
                timestamp += 16.666_667;
            }

            let mut allocations = WebGpuAllocationSummary::default();
            let mut allocation_stages = WebGpuFrameStageAllocationSummary::default();
            let mut submit_allocations = WebGpuSubmitAllocationSummary::default();
            for _sample in 0..sample_count {
                for _frame in 0..frames {
                    let start = perf_now();
                    let alloc_before = oxide_wasm_alloc_counter::snapshot();
                    self.state.borrow_mut().frame_at_profiled(timestamp, &mut allocation_stages)?;
                    let alloc_after = oxide_wasm_alloc_counter::snapshot();
                    add_allocation_frame(&mut allocations, alloc_before, alloc_after);
                    add_submit_allocation_frame(
                        &mut submit_allocations,
                        renderer.borrow().last_stats(),
                    );
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
            let allocations = allocation_metrics(&allocations, "");
            let allocation_stages = frame_stage_allocation_metrics(&allocation_stages);
            let submit_allocations = submit_allocation_metrics(&submit_allocations, "");
            let backend_stats = renderer_stats_metrics(stats, "");
            Ok(format!(
                "warmup_frames=512;samples={sample_count};frames_per_sample={frames};frames={total_frames};cpu_submit_p50_ms={p50_ms:.3};cpu_submit_p95_ms={p95_ms:.3};cpu_submit_p99_ms={p99_ms:.3};cpu_submit_peak_ms={peak_ms:.3};cpu_submit_avg_ms={avg_ms:.3}{backend_stats}{allocations}{allocation_stages}{submit_allocations}",
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
            let current_allocations = allocation_metrics(&current.allocations, "current");
            let legacy_allocations = allocation_metrics(&legacy.allocations, "legacy");
            let current_submit_allocations =
                submit_allocation_metrics(&current.submit_allocations, "current");
            let legacy_submit_allocations =
                submit_allocation_metrics(&legacy.submit_allocations, "legacy");
            let current_stats = renderer_stats_metrics(current.stats, "current");
            let legacy_stats = renderer_stats_metrics(legacy.stats, "legacy");
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames};current_warmup_ms={:.3};current_p50_ms={:.3};current_p95_ms={:.3};current_p99_ms={:.3};current_peak_ms={:.3};current_avg_ms={:.3}{current_allocations}{current_submit_allocations}{current_stats};legacy_warmup_ms={:.3};legacy_p50_ms={:.3};legacy_p95_ms={:.3};legacy_p99_ms={:.3};legacy_peak_ms={:.3};legacy_avg_ms={:.3}{legacy_allocations}{legacy_submit_allocations}{legacy_stats};legacy_over_current={ratio:.3};vertices={};vertex_bytes={}",
                current.warmup_ms,
                current.p50_ms,
                current.p95_ms,
                current.p99_ms,
                current.peak_ms,
                current.avg_ms,
                legacy.warmup_ms,
                legacy.p50_ms,
                legacy.p95_ms,
                legacy.p99_ms,
                legacy.peak_ms,
                legacy.avg_ms,
                current.vertices,
                current.vertex_bytes,
            ))
        }

        pub async fn bench_webgpu_id_mask_current(
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
            let current_allocations = allocation_metrics(&current.allocations, "current");
            let current_submit_allocations =
                submit_allocation_metrics(&current.submit_allocations, "current");
            let current_stats = renderer_stats_metrics(current.stats, "current");
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames};current_warmup_ms={:.3};current_p50_ms={:.3};current_p95_ms={:.3};current_p99_ms={:.3};current_peak_ms={:.3};current_avg_ms={:.3}{current_allocations}{current_submit_allocations}{current_stats};vertices={};vertex_bytes={}",
                current.warmup_ms,
                current.p50_ms,
                current.p95_ms,
                current.p99_ms,
                current.peak_ms,
                current.avg_ms,
                current.vertices,
                current.vertex_bytes,
            ))
        }

        pub async fn bench_webgpu_id_mask_cache_c33(&self) -> Result<String, JsValue> {
            let renderer = self.state.borrow().renderer.clone();
            let vertices = webgpu_id_mask_vertices(WEBGPU_ID_MASK_CELLS, WEBGPU_ID_MASK_EXTENT);
            let mut changed_vertices = vertices.clone();
            changed_vertices[0].position_px[0] += 0.25;
            let default_budget = renderer.borrow().id_mask_cache_budget_bytes();
            let one_entry_budget = renderer
                .borrow()
                .id_mask_target_bytes_per_pixel()
                .saturating_mul(512 * 512);
            let one_entry_timing = measure_webgpu_id_mask_multi_cache(
                &renderer,
                &vertices,
                one_entry_budget,
                4,
                16,
                "one_entry_multi",
            )
            .await?;
            let lru_timing = measure_webgpu_id_mask_multi_cache(
                &renderer,
                &vertices,
                default_budget,
                4,
                16,
                "lru_multi",
            )
            .await?;

            let static_stats = {
                let mut renderer = renderer.borrow_mut();
                renderer.purge_id_mask_field_cache();
                webgpu_id_mask_frame(&mut renderer, &vertices, 1, 0.0)?;
                webgpu_id_mask_frame(&mut renderer, &vertices, 1, 0.0)?;
                renderer.last_stats()
            };
            let style_stats = {
                let mut renderer = renderer.borrow_mut();
                webgpu_id_mask_configured_frame(&mut renderer, &vertices, 1, |pass| {
                    pass.city_styles[0].fill_rgb = [0.31, 0.71, 0.19];
                })?;
                renderer.last_stats()
            };
            let viewport_stats = {
                let mut renderer = renderer.borrow_mut();
                webgpu_id_mask_configured_frame(&mut renderer, &vertices, 1, |pass| {
                    pass.raster.viewport = gfx::RectF::new(8.0, 6.0, 248.0, 246.0);
                })?;
                renderer.last_stats()
            };
            let projection_stats = {
                let mut renderer = renderer.borrow_mut();
                webgpu_id_mask_configured_frame(&mut renderer, &vertices, 1, |pass| {
                    pass.raster.projection.world_to_clip[3][0] = 0.125;
                })?;
                renderer.last_stats()
            };
            let content_stats = {
                let mut renderer = renderer.borrow_mut();
                webgpu_id_mask_frame(&mut renderer, &changed_vertices, 2, 0.0)?;
                renderer.last_stats()
            };
            let one_entry_stats = {
                let mut renderer = renderer.borrow_mut();
                renderer.purge_id_mask_field_cache();
                renderer.set_id_mask_cache_budget_bytes(one_entry_budget);
                webgpu_id_mask_two_map_frame(&mut renderer, &vertices)?;
                webgpu_id_mask_two_map_frame(&mut renderer, &vertices)?;
                renderer.last_stats()
            };
            let lru_stats = {
                let mut renderer = renderer.borrow_mut();
                renderer.purge_id_mask_field_cache();
                renderer.set_id_mask_cache_budget_bytes(default_budget);
                webgpu_id_mask_two_map_frame(&mut renderer, &vertices)?;
                webgpu_id_mask_two_map_frame(&mut renderer, &vertices)?;
                renderer.last_stats()
            };
            let memory_pressure = {
                let mut renderer = renderer.borrow_mut();
                renderer.purge_id_mask_field_cache_for_memory_pressure();
                renderer.last_stats()
            };
            let reentry = {
                let mut renderer = renderer.borrow_mut();
                webgpu_id_mask_frame(&mut renderer, &vertices, 1, 0.0)?;
                renderer.last_stats()
            };
            let device_loss = {
                let mut renderer = renderer.borrow_mut();
                renderer.purge_id_mask_field_cache_for_device_loss_for_benchmark();
                renderer.last_stats()
            };

            Ok(format!(
                "{};{};{};{};{};{};{};{};{};memory_pressure_resident_bytes={};memory_pressure_reason={};reentry_misses={};reentry_entries={};device_loss_resident_bytes={};device_loss_reason={}",
                one_entry_timing,
                lru_timing,
                id_mask_cache_case_metrics("static", static_stats),
                id_mask_cache_case_metrics("style", style_stats),
                id_mask_cache_case_metrics("viewport", viewport_stats),
                id_mask_cache_case_metrics("projection", projection_stats),
                id_mask_cache_case_metrics("content", content_stats),
                id_mask_cache_case_metrics("one_entry_multi", one_entry_stats),
                id_mask_cache_case_metrics("lru_multi", lru_stats),
                memory_pressure.id_mask_cache_resident_bytes,
                memory_pressure.id_mask_cache_last_purge_reason,
                reentry.id_mask_cache_misses,
                reentry.id_mask_cache_entries,
                device_loss.id_mask_cache_resident_bytes,
                device_loss.id_mask_cache_last_purge_reason,
            ))
        }

        pub async fn bench_webgpu_upload_current(
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
            glyph_current.stats =
                settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut image_current = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.image_frame(renderer, true)
                })
            })?;
            image_current.stats =
                settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{};atlas_width={};atlas_height={};atlas_dirty_width={};atlas_dirty_height={};image_width={};image_height={};image_dirty_width={};image_dirty_height={}",
                sampled_case_metrics(&glyph_current, "glyph_current"),
                sampled_case_metrics(&image_current, "image_current"),
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

        pub async fn bench_webgpu_atlas_c15(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.ensure_upload_bench_resources()?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut cold = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.glyph_cold_create_frame(renderer)
                })
            })?;
            cold.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut full = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.glyph_frame(renderer, false)
                })
            })?;
            full.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut dirty = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.glyph_frame(renderer, true)
                })
            })?;
            dirty.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames};cold_warmup_ms={:.3};full_warmup_ms={:.3};dirty_warmup_ms={:.3}{}{}{};atlas_width={};atlas_height={};atlas_row_bytes={};dirty_width={};dirty_height={};dirty_row_bytes={}",
                cold.warmup_ms,
                full.warmup_ms,
                dirty.warmup_ms,
                sampled_case_metrics(&cold, "cold"),
                sampled_case_metrics(&full, "full"),
                sampled_case_metrics(&dirty, "dirty"),
                WEBGPU_UPLOAD_ATLAS_SIZE,
                WEBGPU_UPLOAD_ATLAS_SIZE,
                WEBGPU_UPLOAD_ATLAS_SIZE + 3,
                WEBGPU_UPLOAD_DIRTY_SIZE,
                WEBGPU_UPLOAD_DIRTY_SIZE,
                WEBGPU_UPLOAD_DIRTY_SIZE + 3,
            ))
        }

        pub async fn bench_webgpu_geometry_c16(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.ensure_geometry_bench_resources()?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut glyphs = self.with_geometry_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    WebGpuGeometryBenchResources::frame(renderer, &resources.glyphs)
                })
            })?;
            glyphs.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut images = self.with_geometry_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    WebGpuGeometryBenchResources::frame(renderer, &resources.images)
                })
            })?;
            images.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut large_mesh = self.with_geometry_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    WebGpuGeometryBenchResources::frame(renderer, &resources.large_mesh)
                })
            })?;
            large_mesh.stats =
                settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames};glyphs_warmup_ms={:.3};images_warmup_ms={:.3};large_mesh_warmup_ms={:.3}{}{}{};glyph_quads={WEBGPU_GEOMETRY_QUADS};image_quads={WEBGPU_GEOMETRY_QUADS};large_vertices={WEBGPU_GEOMETRY_LARGE_VERTICES}",
                glyphs.warmup_ms,
                images.warmup_ms,
                large_mesh.warmup_ms,
                sampled_case_metrics(&glyphs, "glyphs"),
                sampled_case_metrics(&images, "images"),
                sampled_case_metrics(&large_mesh, "large_mesh"),
            ))
        }

        pub async fn bench_webgpu_targets_c19(&self, repeats: u32) -> Result<String, JsValue> {
            let repeat_count = repeats.clamp(1, 100);
            let renderer = self.state.borrow().renderer.clone();
            renderer.borrow_mut().set_timestamp_readback_interval_for_benchmark(1);
            renderer.borrow_mut().set_memory_stats_interval_for_benchmark(1);
            let (back, front) = {
                let mut renderer = renderer.borrow_mut();
                (
                    webgpu_scene3d_create_back_mesh(&mut renderer)?,
                    webgpu_scene3d_create_front_mesh(&mut renderer)?,
                )
            };
            let mut builder = ui::DrawListBuilder::new();
            c19_resize(&renderer, 512, 512)?;
            c19_direct_frame(&renderer, &mut builder)?;
            wait_renderer_queue_idle(&renderer).await?;
            renderer.borrow_mut().prewarm_auxiliary_targets(true, false);
            c19_backdrop_frame(&renderer, &mut builder)?;
            wait_renderer_queue_idle(&renderer).await?;
            c19_resize(&renderer, 514, 514)?;
            renderer.borrow_mut().prewarm_auxiliary_targets(false, true);
            c19_scene3d_frame(&renderer, back, front)?;
            wait_renderer_queue_idle(&renderer).await?;

            let mut resize_direct_ms = Vec::with_capacity(repeat_count as usize);
            let mut direct_first_ms = Vec::with_capacity(repeat_count as usize);
            let mut direct_ready_ms = Vec::with_capacity(repeat_count as usize);
            let mut direct_complete_ms = Vec::with_capacity(repeat_count as usize);
            let mut direct_gpu_ms = Vec::with_capacity(repeat_count as usize);
            let mut backdrop_prewarm_ms = Vec::with_capacity(repeat_count as usize);
            let mut backdrop_first_ms = Vec::with_capacity(repeat_count as usize);
            let mut backdrop_ready_ms = Vec::with_capacity(repeat_count as usize);
            let mut backdrop_complete_ms = Vec::with_capacity(repeat_count as usize);
            let mut backdrop_gpu_ms = Vec::with_capacity(repeat_count as usize);
            let mut resize_scene3d_ms = Vec::with_capacity(repeat_count as usize);
            let mut scene3d_prewarm_ms = Vec::with_capacity(repeat_count as usize);
            let mut scene3d_first_ms = Vec::with_capacity(repeat_count as usize);
            let mut scene3d_ready_ms = Vec::with_capacity(repeat_count as usize);
            let mut scene3d_complete_ms = Vec::with_capacity(repeat_count as usize);
            let mut scene3d_gpu_ms = Vec::with_capacity(repeat_count as usize);
            let mut direct_stats = WebRendererStats::default();
            let mut backdrop_stats = WebRendererStats::default();
            let mut scene3d_stats = WebRendererStats::default();
            let mut resize_direct_target_creates = 0_u64;
            let mut resize_scene3d_target_creates = 0_u64;
            let mut backdrop_prewarm_target_creates = 0_u64;
            let mut scene3d_prewarm_target_creates = 0_u64;
            for _ in 0..repeat_count {
                let before = renderer.borrow().last_stats().target_texture_creates;
                let start = perf_now();
                c19_resize(&renderer, 512, 512)?;
                resize_direct_ms.push((perf_now() - start).max(0.0));
                let after = renderer.borrow().last_stats().target_texture_creates;
                resize_direct_target_creates = resize_direct_target_creates
                    .saturating_add(u64::from(after.saturating_sub(before)));

                let start = perf_now();
                c19_direct_frame(&renderer, &mut builder)?;
                let direct_first = (perf_now() - start).max(0.0);
                direct_first_ms.push(direct_first);
                direct_ready_ms.push(resize_direct_ms.last().copied().unwrap_or(0.0) + direct_first);
                wait_renderer_queue_idle(&renderer).await?;
                direct_complete_ms.push((perf_now() - start).max(0.0));
                direct_stats = renderer.borrow_mut().collect_timestamp_readbacks();
                direct_gpu_ms.push(direct_stats.gpu_timestamp_total_ns as f64 / 1_000_000.0);

                let before = renderer.borrow().last_stats().target_texture_creates;
                let prewarm_start = perf_now();
                renderer.borrow_mut().prewarm_auxiliary_targets(true, false);
                let backdrop_prewarm = (perf_now() - prewarm_start).max(0.0);
                backdrop_prewarm_ms.push(backdrop_prewarm);
                let after = renderer.borrow().last_stats().target_texture_creates;
                backdrop_prewarm_target_creates = backdrop_prewarm_target_creates
                    .saturating_add(u64::from(after.saturating_sub(before)));
                let start = perf_now();
                c19_backdrop_frame(&renderer, &mut builder)?;
                let backdrop_first = (perf_now() - start).max(0.0);
                backdrop_first_ms.push(backdrop_first);
                backdrop_ready_ms.push(backdrop_prewarm + backdrop_first);
                wait_renderer_queue_idle(&renderer).await?;
                backdrop_complete_ms.push((perf_now() - start).max(0.0));
                backdrop_stats = renderer.borrow_mut().collect_timestamp_readbacks();
                backdrop_gpu_ms.push(backdrop_stats.gpu_timestamp_total_ns as f64 / 1_000_000.0);

                let before = backdrop_stats.target_texture_creates;
                let start = perf_now();
                c19_resize(&renderer, 514, 514)?;
                resize_scene3d_ms.push((perf_now() - start).max(0.0));
                let after = renderer.borrow().last_stats().target_texture_creates;
                resize_scene3d_target_creates = resize_scene3d_target_creates
                    .saturating_add(u64::from(after.saturating_sub(before)));

                let before = renderer.borrow().last_stats().target_texture_creates;
                let prewarm_start = perf_now();
                renderer.borrow_mut().prewarm_auxiliary_targets(false, true);
                let scene3d_prewarm = (perf_now() - prewarm_start).max(0.0);
                scene3d_prewarm_ms.push(scene3d_prewarm);
                let after = renderer.borrow().last_stats().target_texture_creates;
                scene3d_prewarm_target_creates = scene3d_prewarm_target_creates
                    .saturating_add(u64::from(after.saturating_sub(before)));
                let start = perf_now();
                c19_scene3d_frame(&renderer, back, front)?;
                let scene3d_first = (perf_now() - start).max(0.0);
                scene3d_first_ms.push(scene3d_first);
                scene3d_ready_ms.push(
                    resize_scene3d_ms.last().copied().unwrap_or(0.0)
                        + scene3d_prewarm
                        + scene3d_first,
                );
                wait_renderer_queue_idle(&renderer).await?;
                scene3d_complete_ms.push((perf_now() - start).max(0.0));
                scene3d_stats = renderer.borrow_mut().collect_timestamp_readbacks();
                scene3d_gpu_ms.push(scene3d_stats.gpu_timestamp_total_ns as f64 / 1_000_000.0);
            }
            {
                let mut renderer = renderer.borrow_mut();
                renderer.mesh3d_release(back);
                renderer.mesh3d_release(front);
                renderer.set_timestamp_readback_interval_for_benchmark(8);
                renderer.set_memory_stats_interval_for_benchmark(60);
            }

            let mut out = String::new();
            let _ = write!(out, "repeats={repeat_count}");
            write_c19_samples(&mut out, "resize_direct_ms", &resize_direct_ms);
            write_c19_samples(&mut out, "direct_first_ms", &direct_first_ms);
            write_c19_samples(&mut out, "direct_ready_ms", &direct_ready_ms);
            write_c19_samples(&mut out, "direct_complete_ms", &direct_complete_ms);
            write_c19_samples(&mut out, "direct_gpu_ms", &direct_gpu_ms);
            write_c19_samples(&mut out, "backdrop_prewarm_ms", &backdrop_prewarm_ms);
            write_c19_samples(&mut out, "backdrop_first_ms", &backdrop_first_ms);
            write_c19_samples(&mut out, "backdrop_ready_ms", &backdrop_ready_ms);
            write_c19_samples(&mut out, "backdrop_complete_ms", &backdrop_complete_ms);
            write_c19_samples(&mut out, "backdrop_gpu_ms", &backdrop_gpu_ms);
            write_c19_samples(&mut out, "resize_scene3d_ms", &resize_scene3d_ms);
            write_c19_samples(&mut out, "scene3d_prewarm_ms", &scene3d_prewarm_ms);
            write_c19_samples(&mut out, "scene3d_first_ms", &scene3d_first_ms);
            write_c19_samples(&mut out, "scene3d_ready_ms", &scene3d_ready_ms);
            write_c19_samples(&mut out, "scene3d_complete_ms", &scene3d_complete_ms);
            write_c19_samples(&mut out, "scene3d_gpu_ms", &scene3d_gpu_ms);
            let _ = write!(
                out,
                ";resize_direct_target_creates={resize_direct_target_creates};resize_scene3d_target_creates={resize_scene3d_target_creates};backdrop_prewarm_target_creates={backdrop_prewarm_target_creates};scene3d_prewarm_target_creates={scene3d_prewarm_target_creates};direct_target_texture_creates={};direct_target_bind_group_creates={};direct_transient_target_bytes={};direct_depth_target_bytes={};direct_buffer_upload_bytes={};backdrop_target_texture_creates={};backdrop_target_bind_group_creates={};backdrop_transient_target_bytes={};backdrop_depth_target_bytes={};scene3d_target_texture_creates={};scene3d_target_bind_group_creates={};scene3d_transient_target_bytes={};scene3d_depth_target_bytes={}",
                direct_stats.target_texture_creates,
                direct_stats.target_bind_group_creates,
                direct_stats.gpu_transient_target_bytes,
                direct_stats.gpu_depth_target_bytes,
                direct_stats.buffer_upload_bytes,
                backdrop_stats.target_texture_creates,
                backdrop_stats.target_bind_group_creates,
                backdrop_stats.gpu_transient_target_bytes,
                backdrop_stats.gpu_depth_target_bytes,
                scene3d_stats.target_texture_creates,
                scene3d_stats.target_bind_group_creates,
                scene3d_stats.gpu_transient_target_bytes,
                scene3d_stats.gpu_depth_target_bytes,
            );
            Ok(out)
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
            renderer.borrow_mut().set_image_upload_scratch_enabled_for_benchmark(true);
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
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(true);
            }
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{};expected_backdrops={WEBGPU_EFFECT_UNIFORM_BACKDROPS};sigma=18.0",
                sampled_case_metrics(&current, "current"),
            ))
        }

        pub async fn bench_webgpu_backdrop_batch_current(
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
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{};expected_backdrops={WEBGPU_BACKDROP_BATCH_BACKDROPS};sigma=6.0",
                sampled_case_metrics(&current, "current"),
            ))
        }

        pub async fn bench_webgpu_backdrop_region_matrix(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.ensure_upload_bench_resources()?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(true);
                renderer.set_timestamp_readback_interval_for_benchmark(1);
                renderer.set_backdrop_copy_timestamp_fences_enabled_for_benchmark(true);
            }
            let separated = self.sample_backdrop_region_case(
                &renderer,
                BackdropRegionCase::Separated48,
                sample_count,
                frames,
            ).await?;
            let coalescible = self.sample_backdrop_region_case(
                &renderer,
                BackdropRegionCase::Coalescible12,
                sample_count,
                frames,
            ).await?;
            let fullscreen = self.sample_backdrop_region_case(
                &renderer,
                BackdropRegionCase::Fullscreen,
                sample_count,
                frames,
            ).await?;
            let edges = self.sample_backdrop_region_case(
                &renderer,
                BackdropRegionCase::EdgesAndCorners,
                sample_count,
                frames,
            ).await?;
            let nested = self.sample_backdrop_region_case(
                &renderer,
                BackdropRegionCase::NestedLayers,
                sample_count,
                frames,
            ).await?;
            let mixed = self.sample_backdrop_region_case(
                &renderer,
                BackdropRegionCase::MixedSigma,
                sample_count,
                frames,
            ).await?;
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{}{}{}{}{};separated_backdrops=48;coalescible_backdrops=12;fullscreen_backdrops=1;edges_backdrops=4;nested_backdrops=3;mixed_backdrops=6",
                sampled_case_metrics(&separated, "separated"),
                sampled_case_metrics(&coalescible, "coalescible"),
                sampled_case_metrics(&fullscreen, "fullscreen"),
                sampled_case_metrics(&edges, "edges"),
                sampled_case_metrics(&nested, "nested"),
                sampled_case_metrics(&mixed, "mixed"),
            ))
        }

        pub async fn bench_webgpu_backdrop_region_case(
            &self,
            case: u32,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let case = BackdropRegionCase::from_u32(case)
                .ok_or_else(|| JsValue::from_str("unknown WebGPU backdrop-region case"))?;
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.ensure_upload_bench_resources()?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(true);
                renderer.set_timestamp_readback_interval_for_benchmark(1);
                renderer.set_backdrop_copy_timestamp_fences_enabled_for_benchmark(true);
            }
            let summary = self.sample_backdrop_region_case(
                &renderer,
                case,
                sample_count,
                frames,
            ).await?;
            Ok(format!(
                "case={};samples={sample_count};frames_per_sample={frames}{}",
                case.name(),
                sampled_case_metrics(&summary, "current"),
            ))
        }

        pub async fn bench_webgpu_backdrop_region_gpu_population(
            &self,
            case: u32,
            measured_frames: u32,
        ) -> Result<String, JsValue> {
            let case = BackdropRegionCase::from_u32(case)
                .ok_or_else(|| JsValue::from_str("unknown WebGPU backdrop-region case"))?;
            let measured_frames = measured_frames.clamp(1, 4_096);
            let renderer = self.ensure_upload_bench_resources()?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(true);
                renderer.set_timestamp_readback_interval_for_benchmark(1);
                renderer.set_backdrop_copy_timestamp_fences_enabled_for_benchmark(true);
            }
            for _ in 0..4 {
                self.with_upload_bench_resources(|renderer, resources| {
                    resources.backdrop_region_frame(renderer, case)
                })?;
            }
            wait_renderer_queue_idle(&renderer).await?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.collect_timestamp_readbacks();
                renderer.clear_completed_timestamp_samples();
            }

            let start_frame_id = renderer.borrow().last_stats().frame_id;
            let start_ms = perf_now();
            let mut cpu_submit_ms = Vec::with_capacity(measured_frames as usize);
            let mut gpu_samples = Vec::with_capacity(measured_frames as usize);
            let mut drained = Vec::with_capacity(32);
            for frame in 0..measured_frames {
                let frame_start = perf_now();
                self.with_upload_bench_resources(|renderer, resources| {
                    resources.backdrop_region_frame(renderer, case)
                })?;
                cpu_submit_ms.push((perf_now() - frame_start).max(0.0));
                if (frame + 1) % 32 == 0 || frame + 1 == measured_frames {
                    wait_renderer_queue_idle(&renderer).await?;
                    let mut renderer = renderer.borrow_mut();
                    renderer.collect_timestamp_readbacks();
                    renderer.drain_completed_timestamp_samples_into(&mut drained);
                    gpu_samples.extend_from_slice(&drained);
                }
            }
            if gpu_samples.len() != measured_frames as usize {
                return Err(JsValue::from_str(&format!(
                    "WebGPU backdrop-region GPU population expected {measured_frames} samples, observed {}",
                    gpu_samples.len(),
                )));
            }
            let stats = renderer.borrow().last_stats();
            let settle = TimestampSettleDiagnostics {
                stats,
                elapsed_ms: (perf_now() - start_ms).max(0.0),
                raf_waits: 0,
                pending_initial: 0,
                pending_final: renderer.borrow().pending_timestamp_readbacks(),
            };
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_timestamp_readback_interval_for_benchmark(8);
                renderer.set_backdrop_copy_timestamp_fences_enabled_for_benchmark(false);
            }
            let mut cpu_json = String::from("[");
            for (index, value) in cpu_submit_ms.iter().enumerate() {
                if index != 0 {
                    cpu_json.push(',');
                }
                let _ = write!(cpu_json, "{value:.6}");
            }
            cpu_json.push(']');
            Ok(format!(
                "{{\"case\":\"{}\",\"warmup_frames\":4,\"measured_frames\":{measured_frames},\"start_frame_id\":{start_frame_id},\"cpu_submit_ms\":{cpu_json},\"gpu_timestamp\":{},\"texture_copies\":{},\"texture_copy_pixels\":{},\"texture_copy_bytes\":{},\"render_passes\":{}}}",
                case.name(),
                timestamp_samples_json(&gpu_samples, settle),
                stats.texture_copies,
                stats.texture_copy_pixels,
                stats.texture_copy_bytes,
                stats.render_passes,
            ))
        }

        pub fn render_webgpu_backdrop_region_case(&self, case: u32) -> Result<String, JsValue>
        {
            let case = BackdropRegionCase::from_u32(case)
                .ok_or_else(|| JsValue::from_str("unknown WebGPU backdrop-region case"))?;
            let renderer = self.ensure_upload_bench_resources()?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(true);
            }
            self.with_upload_bench_resources(|renderer, resources| {
                resources.backdrop_region_frame(renderer, case)
            })?;
            Ok(format!(
                "case={};{}",
                case.name(),
                renderer_stats_metrics(renderer.borrow().last_stats(), "current"),
            ))
        }

        pub async fn bench_webgpu_scene3d_ab(
            &self,
            samples: u32,
            frames_per_sample: u32,
            stress_instances: u32,
            stress_mode: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let stress_instances = if stress_instances == 0 {
                WEBGPU_SCENE3D_STRESS_INSTANCES
            } else {
                stress_instances.clamp(1, 10_000) as usize
            };
            let stress_mode = WebGpuScene3dStressMode::from_u32(stress_mode)
                .ok_or_else(|| JsValue::from_str("unknown WebGPU Scene3D stress mode"))?;
            let renderer = self.state.borrow().renderer.clone();
            let mut resources = {
                let mut renderer = renderer.borrow_mut();
                WebGpuScene3dBenchResources::new(&mut renderer)?
            };
            let mut stress_resources = {
                let mut renderer = renderer.borrow_mut();
                WebGpuScene3dStressBenchResources::new(
                    &mut renderer,
                    stress_instances,
                    stress_mode,
                )?
            };
            let mut stress_recreate =
                WebGpuScene3dStressRecreateResources::new(stress_instances, stress_mode);
            renderer.borrow_mut().clear_completed_timestamp_samples();
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut reused = {
                let mut renderer = renderer.borrow_mut();
                bench_webgpu_sampled_case(&mut renderer, sample_count, frames, |renderer, _, _| {
                    resources.frame(renderer)
                })?
            };
            reused.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let reused_gpu = drain_scene3d_gpu_distribution(&mut renderer.borrow_mut());
            renderer.borrow_mut().clear_completed_timestamp_samples();
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut recreate = {
                let mut renderer = renderer.borrow_mut();
                bench_webgpu_sampled_case(&mut renderer, sample_count, frames, |renderer, _, _| {
                    webgpu_scene3d_recreate_frame(renderer, 512, 512, 2.0)
                })?
            };
            recreate.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let recreate_gpu = drain_scene3d_gpu_distribution(&mut renderer.borrow_mut());
            renderer.borrow_mut().clear_completed_timestamp_samples();
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut stress_reused = {
                let mut renderer = renderer.borrow_mut();
                bench_webgpu_sampled_case(&mut renderer, sample_count, frames, |renderer, _, _| {
                    stress_resources.frame(renderer)
                })?
            };
            stress_reused.stats =
                settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let stress_reused_gpu =
                drain_scene3d_gpu_distribution(&mut renderer.borrow_mut());
            renderer.borrow_mut().clear_completed_timestamp_samples();
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut stress_recreate_summary = {
                let mut renderer = renderer.borrow_mut();
                bench_webgpu_sampled_case(&mut renderer, sample_count, frames, |renderer, _, _| {
                    stress_recreate.frame(renderer)
                })?
            };
            stress_recreate_summary.stats =
                settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let stress_recreate_gpu =
                drain_scene3d_gpu_distribution(&mut renderer.borrow_mut());
            let ratio = if reused.p50_ms > 0.0 { recreate.p50_ms / reused.p50_ms } else { 0.0 };
            let stress_ratio = if stress_reused.p50_ms > 0.0 {
                stress_recreate_summary.p50_ms / stress_reused.p50_ms
            } else {
                0.0
            };
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{}{}{}{}{}{}{};recreate_over_reused={ratio:.3};stress_recreate_over_reused={stress_ratio:.3};meshes=2;instances=2;stress_meshes=2;stress_instances={stress_instances};stress_mode={}",
                sampled_case_metrics(&reused, "reused"),
                scene3d_gpu_distribution_metrics(reused_gpu, "reused"),
                sampled_case_metrics(&recreate, "recreate"),
                scene3d_gpu_distribution_metrics(recreate_gpu, "recreate"),
                sampled_case_metrics(&stress_reused, "stress_reused"),
                scene3d_gpu_distribution_metrics(stress_reused_gpu, "stress_reused"),
                sampled_case_metrics(&stress_recreate_summary, "stress_recreate"),
                scene3d_gpu_distribution_metrics(stress_recreate_gpu, "stress_recreate"),
                stress_mode.name(),
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
                renderer.set_draw_item_coalescing_enabled_for_benchmark(false);
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
                renderer.set_draw_state_cache_enabled_for_benchmark(true);
                renderer.set_draw_item_coalescing_enabled_for_benchmark(true);
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(true);
            }
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{};glyphs={};image_tiles={WEBGPU_MIXED_IMAGE_TILES};image_width={};image_height={}",
                sampled_case_metrics(&current, "current"),
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
                renderer.set_draw_item_coalescing_enabled_for_benchmark(false);
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
                renderer.set_draw_state_cache_enabled_for_benchmark(true);
                renderer.set_draw_item_coalescing_enabled_for_benchmark(true);
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(true);
            }
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{};glyphs={};image_tiles={WEBGPU_LAYER_EFFECT_IMAGE_TILES};image_width={};image_height={};expected_layers=3;expected_damage_rects=3;expected_backdrops={WEBGPU_LAYER_EFFECT_BACKDROPS}",
                sampled_case_metrics(&current, "current"),
                WEBGPU_LAYER_EFFECT_GLYPHS,
                WEBGPU_UPLOAD_IMAGE_SIZE,
                WEBGPU_UPLOAD_IMAGE_SIZE,
            ))
        }

        pub async fn bench_webgpu_clean_layer_ab(
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
                renderer.set_draw_item_coalescing_enabled_for_benchmark(false);
                renderer.set_effect_uniform_batch_enabled_for_benchmark(true);
                renderer.set_backdrop_batch_enabled_for_benchmark(true);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut clean = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.clean_layer_frame(renderer, false)
                })
            })?;
            clean.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                renderer.borrow_mut().set_draw_item_coalescing_enabled_for_benchmark(true);
            }
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{};glyphs={};image_tiles={WEBGPU_CLEAN_LAYER_IMAGE_TILES};image_width={};image_height={};expected_layers=1;expected_clean_hits=1",
                sampled_case_metrics(&clean, "clean"),
                WEBGPU_CLEAN_LAYER_GLYPHS,
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
                renderer.set_draw_item_coalescing_enabled_for_benchmark(false);
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
                renderer.set_draw_state_cache_enabled_for_benchmark(true);
                renderer.set_draw_item_coalescing_enabled_for_benchmark(true);
            }
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{};expected_image_meshes={WEBGPU_COMMAND_FAMILY_REPEATS};expected_nine_slices={WEBGPU_COMMAND_FAMILY_REPEATS};expected_sdf_glyphs={};expected_sdf_runs={WEBGPU_COMMAND_FAMILY_SDF_RUNS};expected_camera_bg=0;image_width={};image_height={}",
                sampled_case_metrics(&current, "current"),
                WEBGPU_COMMAND_FAMILY_SDF_GLYPHS.saturating_mul(WEBGPU_COMMAND_FAMILY_SDF_RUNS),
                WEBGPU_UPLOAD_IMAGE_SIZE,
                WEBGPU_UPLOAD_IMAGE_SIZE,
            ))
        }

        pub async fn bench_webgpu_glyph_run_current(
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
                renderer.set_draw_item_coalescing_enabled_for_benchmark(false);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut current = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.glyph_run_frame(renderer)
                })
            })?;
            current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_state_cache_enabled_for_benchmark(true);
                renderer.set_draw_item_coalescing_enabled_for_benchmark(true);
            }
            let expected_glyph_quads =
                WEBGPU_GLYPH_RUN_RUNS.saturating_mul(WEBGPU_GLYPH_RUN_GLYPHS_PER_RUN);
            let expected_draw_items = 3;
            let expected_glyph_instance_bytes = expected_glyph_quads.saturating_mul(36);
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{};expected_glyph_runs={WEBGPU_GLYPH_RUN_RUNS};expected_glyphs_per_run={WEBGPU_GLYPH_RUN_GLYPHS_PER_RUN};expected_glyph_quads={expected_glyph_quads};expected_glyph_instance_bytes={expected_glyph_instance_bytes};expected_sdf_runs={WEBGPU_GLYPH_RUN_SDF_RUNS};expected_sdf_glyph_quads={};expected_draw_items={expected_draw_items};atlas_width={WEBGPU_UPLOAD_ATLAS_SIZE};atlas_height={WEBGPU_UPLOAD_ATLAS_SIZE}",
                sampled_case_metrics(&current, "current"),
                WEBGPU_GLYPH_RUN_SDF_RUNS.saturating_mul(WEBGPU_GLYPH_RUN_GLYPHS_PER_RUN),
            ))
        }

        pub async fn bench_webgpu_glyph_language_matrix_current(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let renderer = self.ensure_glyph_matrix_resources()?;
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut current = self.with_glyph_matrix_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.frame(renderer)
                })
            })?;
            current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            let (atlas_pages, glyph_quads, bitmap_runs, sdf_runs, source_draw_items) = {
                let state = self.state.borrow();
                let Some(resources) = state.glyph_matrix_resources.as_ref() else {
                    return Err(JsValue::from_str("WebGPU glyph matrix resources unavailable"));
                };
                (
                    resources.atlas_pages,
                    resources.glyph_quads,
                    resources.bitmap_runs,
                    resources.sdf_runs,
                    resources.drawlist.items.len(),
                )
            };
            let expected_glyph_instance_bytes = glyph_quads.saturating_mul(36);
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{};labels=1000;languages=latin,rtl,cjk,emoji;atlas_pages={atlas_pages};bitmap_runs={bitmap_runs};sdf_runs={sdf_runs};source_draw_items={source_draw_items};expected_glyph_quads={glyph_quads};expected_glyph_instance_bytes={expected_glyph_instance_bytes}",
                sampled_case_metrics(&current, "current"),
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
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_state_cache_enabled_for_benchmark(true);
                renderer.set_draw_item_coalescing_enabled_for_benchmark(false);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut current = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.neon_marker_frame(renderer)
                })
            })?;
            current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_state_cache_enabled_for_benchmark(true);
                renderer.set_draw_item_coalescing_enabled_for_benchmark(true);
            }
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{};expected_markers={WEBGPU_NEON_MARKERS};expected_draw_items={}",
                sampled_case_metrics(&current, "current"),
                WEBGPU_NEON_MARKERS,
            ))
        }

        pub async fn bench_webgpu_architecture_primitives(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            self.bench_webgpu_architecture_matrix(
                samples,
                frames_per_sample,
                2.0,
                WebGpuArchitectureMatrixKind::Full,
            ).await
        }

        pub async fn bench_webgpu_rrect_architecture(
            &self,
            samples: u32,
            frames_per_sample: u32,
            dpr: f32,
        ) -> Result<String, JsValue> {
            self.bench_webgpu_architecture_matrix(
                samples,
                frames_per_sample,
                dpr,
                WebGpuArchitectureMatrixKind::RRect,
            ).await
        }

        pub async fn bench_webgpu_image_architecture(
            &self,
            samples: u32,
            frames_per_sample: u32,
            dpr: f32,
        ) -> Result<String, JsValue> {
            self.bench_webgpu_architecture_matrix(
                samples,
                frames_per_sample,
                dpr,
                WebGpuArchitectureMatrixKind::Image,
            ).await
        }

        pub async fn bench_webgpu_image_store(
            &self,
            requested_count: u32,
            standalone: bool,
        ) -> Result<String, JsValue> {
            let count = requested_count.clamp(1, 10_000) as usize;
            let encoded: Vec<_> = (0..count)
                .map(|seed| c60_web_icon_png(seed as u64, 64))
                .collect::<Result<_, _>>()?;
            let usage = if standalone {
                images::ImageUsage::Standalone
            } else {
                images::ImageUsage::Static
            };
            let mut store = images::ImageStore::new(images::ImageStoreConfig {
                decoded_budget_bytes: 64 * 1024 * 1024,
                gpu_budget_bytes: 64 * 1024 * 1024,
                atlas_width: 512,
                atlas_height: 512,
                max_atlas_image_dimension: 128,
                gutter: 2,
            })
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
            let setup_started = perf_now();
            let ids: Vec<_> = (0..count)
                .map(|index| {
                    store.request(images::ImageRequest {
                        variant: images::ImageVariant {
                            source: index as u64 + 1,
                            revision: 1,
                            display_width: 28,
                            display_height: 28,
                        },
                        encoded: encoded[index].clone(),
                        usage,
                    })
                })
                .collect();
            let decode_started = perf_now();
            loop {
                let jobs: Vec<_> = (0..32).filter_map(|_| store.take_decode_job()).collect();
                if jobs.is_empty() {
                    break;
                }
                for completion in join_all(
                    jobs.into_iter().map(images::decode_image_at_display_size_browser),
                )
                .await
                {
                    store.complete_decode(completion);
                }
            }
            let decode_wall_ms = (perf_now() - decode_started).max(0.0);
            let renderer = self.state.borrow().renderer.clone();
            let upload_started = perf_now();
            let uploaded = store.upload_ready(&mut *renderer.borrow_mut());
            let upload_wall_ms = (perf_now() - upload_started).max(0.0);
            let mut builder = ui::DrawListBuilder::new();
            for (index, image) in ids.iter().filter_map(|id| store.resolve(*id)).enumerate() {
                builder.image(
                    image.texture,
                    gfx::RectF::new(
                        (index % 43) as f32 * 28.0,
                        (index / 43) as f32 * 28.0,
                        28.0,
                        28.0,
                    ),
                    image.source,
                    1.0,
                );
            }
            let setup_ms = (perf_now() - setup_started).max(0.0);
            let timestamp_start_frame_id = renderer.borrow().last_stats().frame_id;
            {
                let mut renderer = renderer.borrow_mut();
                let token = renderer.begin_frame(&gfx::FrameTarget, None);
                renderer.encode_pass(builder.drawlist());
                renderer.submit(token).map_err(render_err)?;
            }
            wait_renderer_queue_idle(&renderer).await?;
            wait_animation_frame_once().await?;
            let first_displayed_ms = (perf_now() - setup_started).max(0.0);
            let mut submit_ms = Vec::with_capacity(20);
            for _ in 0..20 {
                let started = perf_now();
                let mut renderer = renderer.borrow_mut();
                let token = renderer.begin_frame(&gfx::FrameTarget, None);
                renderer.encode_pass(builder.drawlist());
                renderer.submit(token).map_err(render_err)?;
                drop(renderer);
                submit_ms.push((perf_now() - started).max(0.0));
            }
            submit_ms.sort_by(f64::total_cmp);
            let backend = settle_renderer_timestamps(&renderer, timestamp_start_frame_id).await?;
            let stats = store.stats();
            let publication_ms = stats.request_to_first_publication_ns as f64
                / stats.first_publication_count.max(1) as f64
                / 1_000_000.0;
            for id in ids {
                store.release(id, &mut *renderer.borrow_mut());
            }
            store.purge_for_memory_warning(&mut *renderer.borrow_mut());
            Ok(format!(
                "count={count};variant={};setup_ms={setup_ms:.6};request_to_first_displayed_frame_ms={first_displayed_ms:.6};store_request_to_first_publication_ms_avg={publication_ms:.6};submit_p50_ms={:.6};submit_p95_ms={:.6};submit_p99_ms={:.6};submit_peak_ms={:.6};decoded_output_bytes={};decoded_peak_bytes={};decode_time_ms={:.6};decode_wall_ms={decode_wall_ms:.6};upload_wall_ms={upload_wall_ms:.6};upload_bytes={};page_clear_bytes={};gpu_resident_bytes={};gpu_peak_bytes={};texture_creates={};atlas_pages={};atlas_slots={};standalone_images={};first_publications={};uploaded={uploaded};draws={};bind_group_binds={};gpu_timestamp_total_ns={}",
                if standalone { "standalone" } else { "atlas" },
                percentile(&submit_ms, 0.50),
                percentile(&submit_ms, 0.95),
                percentile(&submit_ms, 0.99),
                submit_ms.last().copied().unwrap_or(0.0),
                stats.decoded_output_bytes,
                stats.decoded_peak_bytes,
                stats.decode_time_ns as f64 / 1_000_000.0,
                stats.upload_bytes,
                stats.atlas_page_clear_bytes,
                stats.gpu_resident_bytes,
                stats.gpu_peak_bytes,
                stats.texture_creates,
                stats.atlas_pages,
                stats.atlas_slots,
                stats.standalone_images,
                stats.first_publication_count,
                backend.draws,
                backend.draw_bind_group_binds,
                backend.gpu_timestamp_total_ns,
            ))
        }

        pub async fn bench_webgpu_nine_slice_architecture(
            &self,
            samples: u32,
            frames_per_sample: u32,
            dpr: f32,
        ) -> Result<String, JsValue> {
            self.bench_webgpu_architecture_matrix(
                samples,
                frames_per_sample,
                dpr,
                WebGpuArchitectureMatrixKind::NineSlice,
            ).await
        }

        pub async fn bench_webgpu_spinner_architecture(
            &self,
            samples: u32,
            frames_per_sample: u32,
            dpr: f32,
        ) -> Result<String, JsValue> {
            self.bench_webgpu_architecture_matrix(
                samples,
                frames_per_sample,
                dpr,
                WebGpuArchitectureMatrixKind::Spinner,
            ).await
        }

        pub async fn bench_webgpu_neon_marker_architecture(
            &self,
            samples: u32,
            frames_per_sample: u32,
            dpr: f32,
        ) -> Result<String, JsValue> {
            self.bench_webgpu_architecture_matrix(
                samples,
                frames_per_sample,
                dpr,
                WebGpuArchitectureMatrixKind::NeonMarker,
            ).await
        }

        async fn bench_webgpu_architecture_matrix(
            &self,
            samples: u32,
            frames_per_sample: u32,
            dpr: f32,
            kind: WebGpuArchitectureMatrixKind,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let dpr = dpr.clamp(1.0, 3.0);
            let variants: &[(&'static str, &'static str, usize)] = match kind {
                WebGpuArchitectureMatrixKind::RRect => &[
                    ("rrect_1", "rrect", 1),
                    ("rrect_64", "rrect", 64),
                    ("rrect_1024", "rrect", 1_024),
                    ("rrect_pathological_64", "rrect_pathological", 64),
                ],
                WebGpuArchitectureMatrixKind::Image => &[
                    ("image_100", "image", 100),
                    ("image_1000", "image", 1_000),
                    ("image_mixed_100", "image_mixed", 100),
                    ("image_mixed_1000", "image_mixed", 1_000),
                ],
                WebGpuArchitectureMatrixKind::NineSlice => &[
                    ("nine_slice_1", "nine_slice", 1),
                    ("nine_slice_64", "nine_slice", 64),
                    ("nine_slice_512", "nine_slice", 512),
                    ("nine_slice_1024", "nine_slice", 1_024),
                ],
                WebGpuArchitectureMatrixKind::Spinner => &[
                    ("spinner_1", "spinner", 1),
                    ("spinner_64", "spinner", 64),
                    ("spinner_512", "spinner", 512),
                    ("spinner_1024", "spinner", 1_024),
                ],
                WebGpuArchitectureMatrixKind::NeonMarker => &[
                    ("neon_64", "neon", 64),
                    ("neon_1024", "neon", 1_024),
                ],
                WebGpuArchitectureMatrixKind::Full => &[
                    ("rrect_1", "rrect", 1),
                    ("rrect_64", "rrect", 64),
                    ("rrect_1024", "rrect", 1_024),
                    ("spinner_1", "spinner", 1),
                    ("spinner_64", "spinner", 64),
                    ("spinner_512", "spinner", 512),
                    ("neon_64", "neon", 64),
                    ("neon_1024", "neon", 1_024),
                    ("nine_slice_64", "nine_slice", 64),
                    ("nine_slice_512", "nine_slice", 512),
                ],
            };
            let renderer = self.ensure_upload_bench_resources()?;
            renderer.borrow_mut().set_timestamp_readback_interval_for_benchmark(1);
            let mut report = String::new();
            for &(name, kind, count) in variants {
                for _ in 0..4 {
                    self.with_upload_bench_resources(|renderer, resources| {
                        resources.architecture_primitive_frame(renderer, kind, count, dpr)
                    })?;
                    wait_renderer_queue_idle(&renderer).await?;
                    renderer.borrow_mut().collect_timestamp_readbacks();
                }
                let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
                renderer.borrow_mut().clear_completed_timestamp_samples();
                let mut values = Vec::with_capacity(sample_count as usize);
                let mut timestamp_samples = Vec::with_capacity(sample_count.saturating_mul(frames) as usize);
                let mut drained_samples = Vec::with_capacity(1);
                let mut allocations = WebGpuAllocationSummary::default();
                let mut submit_allocations = WebGpuSubmitAllocationSummary::default();
                for _ in 0..sample_count {
                    let mut cpu_ms = 0.0;
                    for _ in 0..frames {
                        let alloc_before = oxide_wasm_alloc_counter::snapshot();
                        let start_ms = perf_now();
                        self.with_upload_bench_resources(|renderer, resources| {
                            resources.architecture_primitive_frame(renderer, kind, count, dpr)
                        })?;
                        cpu_ms += (perf_now() - start_ms).max(0.0);
                        let alloc_after = oxide_wasm_alloc_counter::snapshot();
                        add_allocation_frame(&mut allocations, alloc_before, alloc_after);
                        add_submit_allocation_frame(&mut submit_allocations, renderer.borrow().last_stats());
                        wait_renderer_queue_idle(&renderer).await?;
                        let mut renderer = renderer.borrow_mut();
                        renderer.collect_timestamp_readbacks();
                        renderer.drain_completed_timestamp_samples_into(&mut drained_samples);
                        timestamp_samples.extend_from_slice(&drained_samples);
                    }
                    values.push(cpu_ms / frames as f64);
                }
                values.sort_by(|a, b| a.total_cmp(b));
                let expected_gpu_samples = sample_count.saturating_mul(frames) as usize;
                if timestamp_samples.len() != expected_gpu_samples {
                    return Err(JsValue::from_str(&format!(
                        "architecture primitive {name} expected {expected_gpu_samples} GPU samples, observed {}",
                        timestamp_samples.len(),
                    )));
                }
                let mut gpu_values = timestamp_samples
                    .iter()
                    .map(|sample| sample.total_ns as f64 / 1_000_000.0)
                    .collect::<Vec<_>>();
                gpu_values.sort_by(|a, b| a.total_cmp(b));
                let summary = WebGpuBenchSummary {
                    warmup_ms: 0.0,
                    p50_ms: percentile(&values, 0.50),
                    p95_ms: percentile(&values, 0.95),
                    p99_ms: percentile(&values, 0.99),
                    peak_ms: values.last().copied().unwrap_or(0.0),
                    avg_ms: average(&values),
                    allocations,
                    submit_allocations,
                    stats: settle_renderer_timestamps(&renderer, timestamp_after_frame).await?,
                };
                if !report.is_empty() {
                    report.push('\n');
                }
                let _ = write!(
                    report,
                    "case=web.architecture.primitive.{name};samples={sample_count};frames_per_sample={frames};primitive_count={count};dpr={dpr:.1};current_gpu_samples={};current_gpu_p50_ms={:.6};current_gpu_p95_ms={:.6};current_gpu_p99_ms={:.6};current_gpu_peak_ms={:.6}{}",
                    gpu_values.len(),
                    percentile(&gpu_values, 0.50),
                    percentile(&gpu_values, 0.95),
                    percentile(&gpu_values, 0.99),
                    gpu_values.last().copied().unwrap_or(0.0),
                    sampled_case_metrics(&summary, "current"),
                );
            }
            renderer.borrow_mut().set_timestamp_readback_interval_for_benchmark(8);
            Ok(report)
        }

        pub async fn bench_webgpu_prepared_chunks(
            &self,
            samples: u32,
            frames_per_sample: u32,
            dirty: bool,
            flat_control: bool,
            bundle_threshold: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 40);
            let renderer = self.ensure_upload_bench_resources()?;
            let (image, atlas) = {
                let state = self.state.borrow();
                let resources = state
                    .bench_resources
                    .as_ref()
                    .ok_or_else(|| JsValue::from_str("missing WebGPU prepared resources"))?;
                (resources.image, resources.glyph_atlas)
            };
            let snapshot_a = webgpu_prepared_snapshot(image, atlas, 1)?;
            let snapshot_b = webgpu_prepared_snapshot(image, atlas, if dirty { 2 } else { 1 })?;
            let mut flat = gfx::DrawList::default();
            {
                let mut renderer = renderer.borrow_mut();
                renderer.resize(1_200, 800, 1.0).map_err(render_err)?;
                renderer.set_timestamp_readback_interval_for_benchmark(1);
                renderer.set_cpu_submit_timing_enabled_for_benchmark(true);
                renderer.set_prepared_bundle_min_draws_for_benchmark(bundle_threshold as usize);
            }
            let mut warmup_values = Vec::with_capacity(8);
            for warmup in 0..8 {
                let snapshot = if warmup & 1 == 0 { &snapshot_a } else { &snapshot_b };
                let warmup_start = perf_now();
                webgpu_prepared_frame(&renderer, snapshot, flat_control, &mut flat)?;
                warmup_values.push((perf_now() - warmup_start).max(0.0));
            }
            wait_renderer_queue_idle(&renderer).await?;
            renderer.borrow_mut().collect_timestamp_readbacks();
            renderer.borrow_mut().clear_completed_timestamp_samples();
            let mut frame_values = Vec::with_capacity(sample_count as usize);
            let mut active_frame_values = Vec::with_capacity(sample_count as usize);
            let mut queue_wait_values = Vec::with_capacity(sample_count as usize);
            let mut encode_values = Vec::with_capacity(sample_count as usize);
            let mut command_encode_values = Vec::with_capacity(sample_count as usize);
            let mut gpu_samples = Vec::with_capacity(sample_count.saturating_mul(frames) as usize);
            let mut drained = Vec::with_capacity(1);
            let mut hits = 0_u64;
            let mut misses = 0_u64;
            let mut traversed = 0_u64;
            let mut copied = 0_u64;
            let mut geometry = 0_u64;
            let mut uploads = 0_u64;
            let mut bundles = 0_u64;
            let mut bundle_execute_calls = 0_u64;
            let mut bundle_draws = 0_u64;
            let mut direct_draws = 0_u64;
            for sample in 0..sample_count {
                let sample_start = perf_now();
                let mut encode_ms = 0.0;
                let mut command_encode_ms = 0.0;
                for frame in 0..frames {
                    let sequence = sample.saturating_mul(frames).saturating_add(frame);
                    let snapshot = if sequence & 1 == 0 { &snapshot_a } else { &snapshot_b };
                    let encoded = webgpu_prepared_frame(&renderer, snapshot, flat_control, &mut flat)?;
                    encode_ms += encoded;
                    let stats = renderer.borrow().last_stats();
                    let timing = renderer.borrow().last_cpu_submit_timing();
                    command_encode_ms += timing.command_encoding_ms;
                    hits = hits.saturating_add(stats.backend_cache_hits);
                    misses = misses.saturating_add(stats.backend_cache_misses);
                    traversed = traversed.saturating_add(stats.commands_traversed);
                    copied = copied.saturating_add(stats.commands_copied);
                    geometry = geometry.saturating_add(stats.geometry_bytes_copied);
                    uploads = uploads.saturating_add(stats.buffer_upload_bytes);
                    bundles = bundles.saturating_add(u64::from(stats.render_bundle_replays));
                    bundle_execute_calls = bundle_execute_calls.saturating_add(u64::from(stats.render_bundle_execute_calls));
                    bundle_draws = bundle_draws.saturating_add(u64::from(stats.render_bundle_draws));
                    direct_draws = direct_draws.saturating_add(u64::from(stats.prepared_direct_draws));
                }
                let active_end = perf_now();
                let queue_wait_start = active_end;
                wait_renderer_queue_idle(&renderer).await?;
                let queue_wait_end = perf_now();
                let mut borrowed = renderer.borrow_mut();
                borrowed.collect_timestamp_readbacks();
                borrowed.drain_completed_timestamp_samples_into(&mut drained);
                drop(borrowed);
                gpu_samples.extend_from_slice(&drained);
                frame_values.push((perf_now() - sample_start).max(0.0) / frames as f64);
                active_frame_values.push((active_end - sample_start).max(0.0) / frames as f64);
                queue_wait_values.push((queue_wait_end - queue_wait_start).max(0.0) / frames as f64);
                encode_values.push(encode_ms / frames as f64);
                command_encode_values.push(command_encode_ms / frames as f64);
            }
            frame_values.sort_by(|a, b| a.total_cmp(b));
            active_frame_values.sort_by(|a, b| a.total_cmp(b));
            queue_wait_values.sort_by(|a, b| a.total_cmp(b));
            encode_values.sort_by(|a, b| a.total_cmp(b));
            command_encode_values.sort_by(|a, b| a.total_cmp(b));
            let mut gpu_values = gpu_samples.iter()
                .map(|sample| sample.total_ns as f64 / 1_000_000.0)
                .collect::<Vec<_>>();
            gpu_values.sort_by(|a, b| a.total_cmp(b));
            let measured_frames = u64::from(sample_count).saturating_mul(u64::from(frames)).max(1);
            let stats = renderer.borrow().last_stats();
            let cache_bytes = renderer.borrow().prepared_cache_resident_bytes();
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_cpu_submit_timing_enabled_for_benchmark(false);
                renderer.set_timestamp_readback_interval_for_benchmark(8);
            }
            let mut report = format!(
                "samples={sample_count};frames_per_sample={frames};dirty={};flat_control={};bundle_threshold={};frame_p50_ms={:.6};frame_p95_ms={:.6};frame_p99_ms={:.6};frame_peak_ms={:.6};active_frame_p50_ms={:.6};active_frame_p95_ms={:.6};active_frame_p99_ms={:.6};active_frame_peak_ms={:.6};queue_wait_p50_ms={:.6};queue_wait_p95_ms={:.6};queue_wait_p99_ms={:.6};queue_wait_peak_ms={:.6};encode_p50_ms={:.6};encode_p95_ms={:.6};encode_p99_ms={:.6};encode_peak_ms={:.6};command_encode_p50_ms={:.6};command_encode_p95_ms={:.6};command_encode_p99_ms={:.6};command_encode_peak_ms={:.6};gpu_samples={};gpu_p50_ms={:.6};gpu_p95_ms={:.6};gpu_p99_ms={:.6};gpu_peak_ms={:.6};cache_hits_avg={:.6};cache_misses_avg={:.6};commands_traversed_avg={:.6};commands_copied_avg={:.6};geometry_bytes_copied_avg={:.6};buffer_upload_bytes_avg={:.6};bundle_replays_avg={:.6};bundle_execute_calls_avg={:.6};bundle_draws_avg={:.6};prepared_direct_draws_avg={:.6};prepared_cache_bytes={};last_bundle_creates={};last_buffer_grows={};last_cache_evictions={};last_draws={}",
                u32::from(dirty),
                u32::from(flat_control),
                bundle_threshold.max(1),
                percentile(&frame_values, 0.50),
                percentile(&frame_values, 0.95),
                percentile(&frame_values, 0.99),
                frame_values.last().copied().unwrap_or(0.0),
                percentile(&active_frame_values, 0.50),
                percentile(&active_frame_values, 0.95),
                percentile(&active_frame_values, 0.99),
                active_frame_values.last().copied().unwrap_or(0.0),
                percentile(&queue_wait_values, 0.50),
                percentile(&queue_wait_values, 0.95),
                percentile(&queue_wait_values, 0.99),
                queue_wait_values.last().copied().unwrap_or(0.0),
                percentile(&encode_values, 0.50),
                percentile(&encode_values, 0.95),
                percentile(&encode_values, 0.99),
                encode_values.last().copied().unwrap_or(0.0),
                percentile(&command_encode_values, 0.50),
                percentile(&command_encode_values, 0.95),
                percentile(&command_encode_values, 0.99),
                command_encode_values.last().copied().unwrap_or(0.0),
                gpu_values.len(),
                percentile(&gpu_values, 0.50),
                percentile(&gpu_values, 0.95),
                percentile(&gpu_values, 0.99),
                gpu_values.last().copied().unwrap_or(0.0),
                hits as f64 / measured_frames as f64,
                misses as f64 / measured_frames as f64,
                traversed as f64 / measured_frames as f64,
                copied as f64 / measured_frames as f64,
                geometry as f64 / measured_frames as f64,
                uploads as f64 / measured_frames as f64,
                bundles as f64 / measured_frames as f64,
                bundle_execute_calls as f64 / measured_frames as f64,
                bundle_draws as f64 / measured_frames as f64,
                direct_draws as f64 / measured_frames as f64,
                cache_bytes,
                stats.render_bundle_creates,
                stats.buffer_grows,
                stats.cache_evictions,
                stats.draws,
            );
            write_c19_samples(&mut report, "frame_samples_ms", &frame_values);
            write_c19_samples(&mut report, "active_frame_samples_ms", &active_frame_values);
            write_c19_samples(&mut report, "queue_wait_samples_ms", &queue_wait_values);
            write_c19_samples(&mut report, "encode_samples_ms", &encode_values);
            write_c19_samples(
                &mut report,
                "command_encode_samples_ms",
                &command_encode_values,
            );
            write_c19_samples(&mut report, "gpu_samples_ms", &gpu_values);
            write_c19_samples(&mut report, "warmup_samples_ms", &warmup_values);
            Ok(report)
        }

        pub async fn bench_webgpu_local_layers_c30(
            &self,
            width: u32,
            height: u32,
            samples: u32,
            frames_per_sample: u32,
            one_dirty: bool,
        ) -> Result<String, JsValue>
        {
            let width = width.clamp(1, 4_096);
            let height = height.clamp(1, 4_096);
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 160);
            let renderer = self.ensure_upload_bench_resources()?;
            let (populate, clean, dirty) = webgpu_local_layer_card_snapshots()?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.resize(width, height, 2.0).map_err(render_err)?;
                renderer.set_memory_stats_interval_for_benchmark(1);
                renderer.set_cpu_submit_timing_enabled_for_benchmark(true);
            }
            webgpu_local_layer_frame(&renderer, &populate)?;
            wait_renderer_queue_idle(&renderer).await?;
            let clock_warmup = webgpu_local_layer_clock_warmup();
            for _ in 0..WEBGPU_LOCAL_LAYER_CLOCK_WARMUP_FRAMES
            {
                webgpu_local_layer_clock_warmup_frame(&renderer, &clock_warmup)?;
            }
            wait_renderer_queue_idle(&renderer).await?;
            let mut warmup_values = Vec::with_capacity(4);
            for _ in 0..4
            {
                let start = perf_now();
                webgpu_local_layer_frame(&renderer, if one_dirty { &dirty } else { &clean })?;
                warmup_values.push((perf_now() - start).max(0.0));
            }
            wait_renderer_queue_idle(&renderer).await?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.collect_timestamp_readbacks();
                renderer.clear_completed_timestamp_samples();
                renderer.set_timestamp_readback_interval_for_benchmark(1);
            }

            let snapshot = if one_dirty { &dirty } else { &clean };
            let measured_frames = u64::from(sample_count).saturating_mul(u64::from(frames)).max(1);
            let mut cpu_values = Vec::with_capacity(sample_count as usize);
            let mut gpu_samples = Vec::with_capacity(measured_frames as usize);
            let mut drained = Vec::with_capacity(32);
            let mut hits = 0_u64;
            let mut misses = 0_u64;
            let mut skipped = 0_u64;
            let mut layer_passes = 0_u64;
            let mut render_passes = 0_u64;
            let mut clear_passes = 0_u64;
            let mut draw_passes = 0_u64;
            let mut traversed = 0_u64;
            let mut copied = 0_u64;
            let mut geometry = 0_u64;
            let mut uploads = 0_u64;
            let mut texture_creates = 0_u64;
            let mut layer_texture_bytes = 0_u64;
            for sample in 0..sample_count
            {
                let mut cpu_ms = 0.0;
                for frame in 0..frames
                {
                    let start = perf_now();
                    webgpu_local_layer_frame(&renderer, snapshot)?;
                    cpu_ms += (perf_now() - start).max(0.0);
                    let stats = renderer.borrow().last_stats();
                    hits = hits.saturating_add(u64::from(stats.layer_cache_hits));
                    misses = misses.saturating_add(u64::from(stats.layer_cache_misses));
                    skipped = skipped.saturating_add(u64::from(stats.layer_cache_skipped_draws));
                    layer_passes = layer_passes.saturating_add(u64::from(stats.layer_passes));
                    render_passes = render_passes.saturating_add(u64::from(stats.render_passes));
                    clear_passes = clear_passes.saturating_add(u64::from(stats.clear_passes));
                    draw_passes = draw_passes.saturating_add(u64::from(stats.draw_passes));
                    traversed = traversed.saturating_add(stats.commands_traversed);
                    copied = copied.saturating_add(stats.commands_copied);
                    geometry = geometry.saturating_add(stats.geometry_bytes_copied);
                    uploads = uploads.saturating_add(stats.buffer_upload_bytes);
                    texture_creates = texture_creates.saturating_add(u64::from(stats.layer_texture_creates));
                    layer_texture_bytes = layer_texture_bytes.max(stats.gpu_layer_texture_bytes);
                    if (frame + 1) % 32 == 0
                    {
                        wait_renderer_queue_idle(&renderer).await?;
                        let mut renderer = renderer.borrow_mut();
                        renderer.collect_timestamp_readbacks();
                        renderer.drain_completed_timestamp_samples_into(&mut drained);
                        gpu_samples.extend_from_slice(&drained);
                    }
                }
                let postroll_frame_id = if sample + 1 == sample_count
                {
                    webgpu_local_layer_frame(&renderer, snapshot)?;
                    Some(renderer.borrow().last_stats().frame_id)
                }
                else
                {
                    None
                };
                wait_renderer_queue_idle(&renderer).await?;
                let mut renderer = renderer.borrow_mut();
                renderer.collect_timestamp_readbacks();
                renderer.drain_completed_timestamp_samples_into(&mut drained);
                if let Some(postroll_frame_id) = postroll_frame_id
                {
                    drained.retain(|sample| sample.frame_id != postroll_frame_id);
                }
                gpu_samples.extend_from_slice(&drained);
                cpu_values.push(cpu_ms / frames as f64);
            }
            if gpu_samples.len() != measured_frames as usize
            {
                return Err(JsValue::from_str(&format!(
                    "C30 expected {measured_frames} GPU samples, observed {}",
                    gpu_samples.len(),
                )));
            }
            let mut gpu_values = gpu_samples.iter()
                .map(|sample| sample.total_ns as f64 / 1_000_000.0)
                .collect::<Vec<_>>();
            cpu_values.sort_by(|a, b| a.total_cmp(b));
            gpu_values.sort_by(|a, b| a.total_cmp(b));
            let local_layer_bytes = (WEBGPU_LOCAL_LAYER_WIDTH as u64 * 2)
                .saturating_mul(WEBGPU_LOCAL_LAYER_HEIGHT as u64 * 2)
                .saturating_mul(4)
                .saturating_mul(WEBGPU_LOCAL_LAYER_CARDS as u64);
            let full_canvas_layer_bytes = u64::from(width)
                .saturating_mul(u64::from(height))
                .saturating_mul(4)
                .saturating_mul(WEBGPU_LOCAL_LAYER_CARDS as u64);
            let target_pixels = layer_texture_bytes
                .checked_div((WEBGPU_LOCAL_LAYER_CARDS as u64).saturating_mul(4))
                .unwrap_or(0);
            let mut report = format!(
                "width={width};height={height};dpr=2;cards={WEBGPU_LOCAL_LAYER_CARDS};one_dirty={};samples={sample_count};frames_per_sample={frames};gpu_clock_warmup_draws={WEBGPU_LOCAL_LAYER_CLOCK_WARMUP_DRAWS};gpu_clock_warmup_frames={WEBGPU_LOCAL_LAYER_CLOCK_WARMUP_FRAMES};gpu_postroll_frames={WEBGPU_LOCAL_LAYER_GPU_POSTROLL_FRAMES};cpu_p50_ms={:.6};cpu_p95_ms={:.6};cpu_p99_ms={:.6};cpu_peak_ms={:.6};gpu_sample_count={};gpu_p50_ms={:.6};gpu_p95_ms={:.6};gpu_p99_ms={:.6};gpu_peak_ms={:.6};layer_texture_bytes={layer_texture_bytes};expected_local_layer_bytes={local_layer_bytes};full_canvas_layer_bytes={full_canvas_layer_bytes};layer_target_pixels={target_pixels};layer_clear_pixels_avg={:.6};layer_shaded_target_pixels_avg={:.6};layer_cache_hits_avg={:.6};layer_cache_misses_avg={:.6};layer_cache_skipped_draws_avg={:.6};layer_passes_avg={:.6};render_passes_avg={:.6};clear_passes_avg={:.6};draw_passes_avg={:.6};commands_traversed_avg={:.6};commands_copied_avg={:.6};geometry_bytes_copied_avg={:.6};buffer_upload_bytes_avg={:.6};layer_texture_creates_avg={:.6}",
                u32::from(one_dirty),
                percentile(&cpu_values, 0.50),
                percentile(&cpu_values, 0.95),
                percentile(&cpu_values, 0.99),
                cpu_values.last().copied().unwrap_or(0.0),
                gpu_values.len(),
                percentile(&gpu_values, 0.50),
                percentile(&gpu_values, 0.95),
                percentile(&gpu_values, 0.99),
                gpu_values.last().copied().unwrap_or(0.0),
                target_pixels as f64 * layer_passes as f64 / measured_frames as f64,
                target_pixels as f64 * layer_passes as f64 / measured_frames as f64,
                hits as f64 / measured_frames as f64,
                misses as f64 / measured_frames as f64,
                skipped as f64 / measured_frames as f64,
                layer_passes as f64 / measured_frames as f64,
                render_passes as f64 / measured_frames as f64,
                clear_passes as f64 / measured_frames as f64,
                draw_passes as f64 / measured_frames as f64,
                traversed as f64 / measured_frames as f64,
                copied as f64 / measured_frames as f64,
                geometry as f64 / measured_frames as f64,
                uploads as f64 / measured_frames as f64,
                texture_creates as f64 / measured_frames as f64,
            );
            write_c19_samples(&mut report, "cpu_samples_ms", &cpu_values);
            write_c19_samples(&mut report, "gpu_samples_ms", &gpu_values);
            write_c19_samples(&mut report, "warmup_samples_ms", &warmup_values);
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_timestamp_readback_interval_for_benchmark(8);
                renderer.set_cpu_submit_timing_enabled_for_benchmark(false);
                renderer.set_memory_stats_interval_for_benchmark(60);
            }
            Ok(report)
        }

        pub async fn bench_webgpu_local_layer_guardrails_c30(&self) -> Result<String, JsValue>
        {
            let renderer = self.ensure_upload_bench_resources()?;
            let (populate, clean, dirty) = webgpu_local_layer_card_snapshots()?;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.resize(640, 480, 2.0).map_err(render_err)?;
                renderer.set_memory_stats_interval_for_benchmark(1);
                renderer.purge_prepared_chunks();
            }

            webgpu_local_layer_frame(&renderer, &populate)?;
            webgpu_local_layer_frame(&renderer, &clean)?;
            let clean_stats = renderer.borrow().last_stats();
            webgpu_local_layer_frame(&renderer, &dirty)?;
            let dirty_stats = renderer.borrow().last_stats();

            renderer.borrow_mut().resize(800, 600, 2.0).map_err(render_err)?;
            webgpu_local_layer_frame(&renderer, &clean)?;
            let resize_stats = renderer.borrow().last_stats();

            renderer.borrow_mut().resize(800, 600, 1.0).map_err(render_err)?;
            webgpu_local_layer_frame(&renderer, &clean)?;
            let scale_stats = renderer.borrow().last_stats();

            renderer.borrow_mut().resize(640, 480, 2.0).map_err(render_err)?;
            webgpu_local_layer_frame(&renderer, &populate)?;
            renderer.borrow_mut().purge_prepared_chunks();
            webgpu_local_layer_frame(&renderer, &clean)?;
            let purge_stats = renderer.borrow().last_stats();

            renderer.borrow_mut().advance_prepared_device_generation_for_benchmark();
            webgpu_local_layer_frame(&renderer, &clean)?;
            let device_stats = renderer.borrow().last_stats();

            let pixels_a = [255_u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
            let pixels_b = [0_u8, 255, 255, 255, 255, 0, 255, 255, 255, 255, 0, 255, 32, 64, 96, 255];
            let image = renderer.borrow_mut().image_create_rgba8(2, 2, &pixels_a, 8);
            let resource = webgpu_local_layer_resource_snapshot(image, 1)?;
            webgpu_local_layer_frame(&renderer, &resource)?;
            webgpu_local_layer_frame(&renderer, &resource)?;
            let resource_clean_stats = renderer.borrow().last_stats();
            renderer.borrow_mut().image_update_rgba8(image, 0, 0, 2, 2, &pixels_b, 8)
                .map_err(render_err)?;
            webgpu_local_layer_frame(&renderer, &resource)?;
            let resource_update_stats = renderer.borrow().last_stats();
            let released = renderer.borrow_mut().image_release(image);
            let recreated = renderer.borrow_mut().image_create_rgba8(2, 2, &pixels_a, 8);
            let recreated_resource = webgpu_local_layer_resource_snapshot(recreated, 2)?;
            webgpu_local_layer_frame(&renderer, &recreated_resource)?;
            let resource_recreate_stats = renderer.borrow().last_stats();
            let generation_changed = recreated != image;
            let _ = renderer.borrow_mut().image_release(recreated);

            let (edge_dirty, edge_clean) = webgpu_local_layer_edge_snapshots()?;
            webgpu_local_layer_frame(&renderer, &edge_dirty)?;
            webgpu_local_layer_frame(&renderer, &edge_clean)?;
            let edge_stats = renderer.borrow().last_stats();
            wait_renderer_queue_idle(&renderer).await?;
            renderer.borrow_mut().set_memory_stats_interval_for_benchmark(60);

            Ok(format!(
                "clean_hits={};clean_misses={};clean_traversed={};dirty_hits={};dirty_misses={};dirty_layer_passes={};dirty_texture_creates={};resize_hits={};resize_misses={};resize_texture_creates={};resize_layer_bytes={};scale_hits={};scale_misses={};scale_texture_creates={};purge_hits={};purge_misses={};purge_texture_creates={};device_hits={};device_misses={};device_texture_creates={};resource_clean_hits={};resource_clean_misses={};resource_update_hits={};resource_update_misses={};resource_update_clear_passes={};resource_update_texture_creates={};resource_recreate_hits={};resource_recreate_misses={};resource_recreate_texture_creates={};resource_released={};resource_generation_changed={};edge_hits={};edge_misses={};edge_traversed={};edge_layer_passes={}",
                clean_stats.layer_cache_hits,
                clean_stats.layer_cache_misses,
                clean_stats.commands_traversed,
                dirty_stats.layer_cache_hits,
                dirty_stats.layer_cache_misses,
                dirty_stats.layer_passes,
                dirty_stats.layer_texture_creates,
                resize_stats.layer_cache_hits,
                resize_stats.layer_cache_misses,
                resize_stats.layer_texture_creates,
                resize_stats.gpu_layer_texture_bytes,
                scale_stats.layer_cache_hits,
                scale_stats.layer_cache_misses,
                scale_stats.layer_texture_creates,
                purge_stats.layer_cache_hits,
                purge_stats.layer_cache_misses,
                purge_stats.layer_texture_creates,
                device_stats.layer_cache_hits,
                device_stats.layer_cache_misses,
                device_stats.layer_texture_creates,
                resource_clean_stats.layer_cache_hits,
                resource_clean_stats.layer_cache_misses,
                resource_update_stats.layer_cache_hits,
                resource_update_stats.layer_cache_misses,
                resource_update_stats.clear_passes,
                resource_update_stats.layer_texture_creates,
                resource_recreate_stats.layer_cache_hits,
                resource_recreate_stats.layer_cache_misses,
                resource_recreate_stats.layer_texture_creates,
                u32::from(released),
                u32::from(generation_changed),
                edge_stats.layer_cache_hits,
                edge_stats.layer_cache_misses,
                edge_stats.commands_traversed,
                edge_stats.layer_passes,
            ))
        }

        pub async fn bench_webgpu_layer_cache_c31(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue>
        {
            let samples = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 160);
            let renderer = self.ensure_upload_bench_resources()?;
            let mut snapshots = Vec::with_capacity(4);
            for phase in 0..4_u32
            {
                snapshots.push(webgpu_local_layer_card_snapshots_with_id_base(
                    31_000 + phase * WEBGPU_LOCAL_LAYER_CARDS as u32,
                )?.0);
            }
            let budget = 6 * 1024 * 1024_u64;
            {
                let mut renderer = renderer.borrow_mut();
                renderer.resize(1_920, 1_080, 2.0).map_err(render_err)?;
                renderer.set_memory_stats_interval_for_benchmark(1);
                renderer.set_layer_cache_budget_bytes(budget);
                renderer.purge_layer_cache();
            }
            webgpu_local_layer_frame(&renderer, &snapshots[0])?;
            wait_renderer_queue_idle(&renderer).await?;

            let mut frame_values = Vec::with_capacity((samples * frames) as usize);
            let mut texture_creates = 0_u64;
            let mut resident_peak = 0_u64;
            let mut pool_peak = 0_u64;
            let mut cpu_peak = 0_u64;
            let mut evictions_peak = 0_u64;
            let mut reuses_peak = 0_u64;
            let mut recreations_peak = 0_u64;
            let mut budget_violations = 0_u64;
            for frame in 0..samples.saturating_mul(frames)
            {
                let snapshot = &snapshots[(frame as usize + 1) % snapshots.len()];
                let started = perf_now();
                webgpu_local_layer_frame(&renderer, snapshot)?;
                frame_values.push((perf_now() - started).max(0.0));
                let stats = renderer.borrow().last_stats();
                texture_creates = texture_creates.saturating_add(u64::from(stats.layer_texture_creates));
                resident_peak = resident_peak.max(stats.layer_cache_resident_bytes);
                pool_peak = pool_peak.max(stats.layer_cache_pool_bytes);
                cpu_peak = cpu_peak.max(stats.layer_cache_cpu_bytes);
                evictions_peak = evictions_peak.max(stats.layer_cache_evictions);
                reuses_peak = reuses_peak.max(stats.layer_cache_pool_reuses);
                recreations_peak = recreations_peak.max(stats.layer_cache_recreations);
                if stats.layer_cache_resident_bytes.saturating_add(stats.layer_cache_pool_bytes)
                    > stats.layer_cache_budget_bytes
                {
                    budget_violations = budget_violations.saturating_add(1);
                }
            }
            wait_renderer_queue_idle(&renderer).await?;
            frame_values.sort_by(|a, b| a.total_cmp(b));

            renderer.borrow_mut().purge_layer_cache_for_memory_pressure();
            let memory_warning = renderer.borrow().last_stats();
            webgpu_local_layer_frame(&renderer, &snapshots[0])?;
            let reentry = renderer.borrow().last_stats();
            renderer.borrow_mut().purge_layer_cache_for_device_loss_for_benchmark();
            let device_loss = renderer.borrow().last_stats();
            renderer.borrow_mut().set_memory_stats_interval_for_benchmark(60);

            let mut report = format!(
                "samples={samples};frames_per_sample={frames};layers={WEBGPU_LOCAL_LAYER_CARDS};budget_bytes={budget};frame_p50_ms={:.6};frame_p95_ms={:.6};frame_p99_ms={:.6};frame_peak_ms={:.6};layer_texture_creates={texture_creates};resident_bytes_peak={resident_peak};pool_bytes_peak={pool_peak};cpu_bytes_peak={cpu_peak};evictions={evictions_peak};pool_reuses={reuses_peak};recreations={recreations_peak};budget_violations={budget_violations};memory_warning_resident_bytes={};memory_warning_pool_bytes={};memory_warning_purge_reason={};reentry_texture_creates={};device_loss_resident_bytes={};device_loss_pool_bytes={};device_loss_purge_reason={}",
                percentile(&frame_values, 0.50),
                percentile(&frame_values, 0.95),
                percentile(&frame_values, 0.99),
                frame_values.last().copied().unwrap_or(0.0),
                memory_warning.layer_cache_resident_bytes,
                memory_warning.layer_cache_pool_bytes,
                memory_warning.layer_cache_last_purge_reason,
                reentry.layer_texture_creates,
                device_loss.layer_cache_resident_bytes,
                device_loss.layer_cache_pool_bytes,
                device_loss.layer_cache_last_purge_reason,
            );
            write_c19_samples(&mut report, "frame_samples_ms", &frame_values);
            Ok(report)
        }

        pub async fn bench_webgpu_dynamic_properties(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 40);
            let renderer = self.ensure_upload_bench_resources()?;
            let (snapshot_a, snapshot_b) = {
                let mut state = self.state.borrow_mut();
                let resources = state
                    .bench_resources
                    .as_mut()
                    .ok_or_else(|| JsValue::from_str("missing WebGPU dynamic property resources"))?;
                (
                    resources.dynamic_property_snapshot(0, false)?,
                    resources.dynamic_property_snapshot(1, false)?,
                )
            };
            {
                let mut renderer = renderer.borrow_mut();
                renderer.resize(1_200, 800, 1.0).map_err(render_err)?;
                renderer.set_timestamp_readback_interval_for_benchmark(1);
                renderer.set_cpu_submit_timing_enabled_for_benchmark(true);
            }
            let mut flat = gfx::DrawList::default();
            let mut warmup_values = Vec::with_capacity(8);
            for warmup in 0..8 {
                let snapshot = if warmup & 1 == 0 { &snapshot_a } else { &snapshot_b };
                let warmup_start = perf_now();
                webgpu_prepared_frame(&renderer, snapshot, false, &mut flat)?;
                warmup_values.push((perf_now() - warmup_start).max(0.0));
            }
            wait_renderer_queue_idle(&renderer).await?;
            renderer.borrow_mut().collect_timestamp_readbacks();
            renderer.borrow_mut().clear_completed_timestamp_samples();
            let mut frame_values = Vec::with_capacity(sample_count as usize);
            let mut active_frame_values = Vec::with_capacity(sample_count as usize);
            let mut queue_wait_values = Vec::with_capacity(sample_count as usize);
            let mut encode_values = Vec::with_capacity(sample_count as usize);
            let mut command_encode_values = Vec::with_capacity(sample_count as usize);
            let mut event_to_submit_values = Vec::with_capacity(sample_count as usize);
            let mut gpu_samples = Vec::with_capacity(sample_count.saturating_mul(frames) as usize);
            let mut drained = Vec::with_capacity(1);
            let mut hits = 0_u64;
            let mut misses = 0_u64;
            let mut traversed = 0_u64;
            let mut copied = 0_u64;
            let mut geometry = 0_u64;
            let mut geometry_uploads = 0_u64;
            let mut property_uploads = 0_u64;
            let mut property_records = 0_u64;
            let mut property_ring_bytes = 0_u64;
            for sample in 0..sample_count {
                let sample_start = perf_now();
                let mut encode_ms = 0.0;
                let mut command_encode_ms = 0.0;
                let mut event_to_submit_ms = 0.0;
                for frame in 0..frames {
                    let sequence = sample.saturating_mul(frames).saturating_add(frame);
                    let snapshot = if sequence & 1 == 0 { &snapshot_a } else { &snapshot_b };
                    let event_start = perf_now();
                    encode_ms += webgpu_prepared_frame(&renderer, snapshot, false, &mut flat)?;
                    event_to_submit_ms += (perf_now() - event_start).max(0.0);
                    let stats = renderer.borrow().last_stats();
                    let timing = renderer.borrow().last_cpu_submit_timing();
                    command_encode_ms += timing.command_encoding_ms;
                    hits = hits.saturating_add(stats.backend_cache_hits);
                    misses = misses.saturating_add(stats.backend_cache_misses);
                    traversed = traversed.saturating_add(stats.commands_traversed);
                    copied = copied.saturating_add(stats.commands_copied);
                    geometry = geometry.saturating_add(stats.geometry_bytes_copied);
                    geometry_uploads = geometry_uploads.saturating_add(stats.buffer_upload_bytes);
                    property_uploads = property_uploads.saturating_add(stats.property_upload_bytes);
                    property_records = property_records.saturating_add(u64::from(stats.property_records_updated));
                    property_ring_bytes = property_ring_bytes.max(stats.property_ring_bytes);
                }
                let active_end = perf_now();
                let queue_wait_start = active_end;
                wait_renderer_queue_idle(&renderer).await?;
                let queue_wait_end = perf_now();
                let mut borrowed = renderer.borrow_mut();
                borrowed.collect_timestamp_readbacks();
                borrowed.drain_completed_timestamp_samples_into(&mut drained);
                drop(borrowed);
                gpu_samples.extend_from_slice(&drained);
                frame_values.push((perf_now() - sample_start).max(0.0) / frames as f64);
                active_frame_values.push((active_end - sample_start).max(0.0) / frames as f64);
                queue_wait_values.push((queue_wait_end - queue_wait_start).max(0.0) / frames as f64);
                encode_values.push(encode_ms / frames as f64);
                command_encode_values.push(command_encode_ms / frames as f64);
                event_to_submit_values.push(event_to_submit_ms / frames as f64);
            }
            frame_values.sort_by(|a, b| a.total_cmp(b));
            active_frame_values.sort_by(|a, b| a.total_cmp(b));
            queue_wait_values.sort_by(|a, b| a.total_cmp(b));
            encode_values.sort_by(|a, b| a.total_cmp(b));
            command_encode_values.sort_by(|a, b| a.total_cmp(b));
            event_to_submit_values.sort_by(|a, b| a.total_cmp(b));
            let mut gpu_values = gpu_samples.iter()
                .map(|sample| sample.total_ns as f64 / 1_000_000.0)
                .collect::<Vec<_>>();
            gpu_values.sort_by(|a, b| a.total_cmp(b));
            let measured_frames = u64::from(sample_count).saturating_mul(u64::from(frames)).max(1);
            let stats = renderer.borrow().last_stats();
            {
                let mut renderer = renderer.borrow_mut();
                renderer.set_cpu_submit_timing_enabled_for_benchmark(false);
                renderer.set_timestamp_readback_interval_for_benchmark(8);
            }
            let mut report = format!(
                "samples={sample_count};frames_per_sample={frames};animated_nodes={WEBGPU_DYNAMIC_PROPERTY_NODES};text_nodes=200;image_nodes=100;frame_p50_ms={:.6};frame_p95_ms={:.6};frame_p99_ms={:.6};frame_peak_ms={:.6};active_frame_p50_ms={:.6};active_frame_p95_ms={:.6};active_frame_p99_ms={:.6};active_frame_peak_ms={:.6};queue_wait_p50_ms={:.6};queue_wait_p95_ms={:.6};queue_wait_p99_ms={:.6};queue_wait_peak_ms={:.6};encode_p50_ms={:.6};encode_p95_ms={:.6};encode_p99_ms={:.6};encode_peak_ms={:.6};command_encode_p50_ms={:.6};command_encode_p95_ms={:.6};command_encode_p99_ms={:.6};command_encode_peak_ms={:.6};event_to_submit_p50_ms={:.6};event_to_submit_p95_ms={:.6};event_to_submit_p99_ms={:.6};event_to_submit_peak_ms={:.6};gpu_samples={};gpu_p50_ms={:.6};gpu_p95_ms={:.6};gpu_p99_ms={:.6};gpu_peak_ms={:.6};cache_hits_avg={:.6};cache_misses_avg={:.6};commands_traversed_avg={:.6};commands_copied_avg={:.6};geometry_bytes_copied_avg={:.6};geometry_upload_bytes_avg={:.6};property_upload_bytes_avg={:.6};property_records_updated_avg={:.6};property_ring_bytes={};last_draws={}",
                percentile(&frame_values, 0.50),
                percentile(&frame_values, 0.95),
                percentile(&frame_values, 0.99),
                frame_values.last().copied().unwrap_or(0.0),
                percentile(&active_frame_values, 0.50),
                percentile(&active_frame_values, 0.95),
                percentile(&active_frame_values, 0.99),
                active_frame_values.last().copied().unwrap_or(0.0),
                percentile(&queue_wait_values, 0.50),
                percentile(&queue_wait_values, 0.95),
                percentile(&queue_wait_values, 0.99),
                queue_wait_values.last().copied().unwrap_or(0.0),
                percentile(&encode_values, 0.50),
                percentile(&encode_values, 0.95),
                percentile(&encode_values, 0.99),
                encode_values.last().copied().unwrap_or(0.0),
                percentile(&command_encode_values, 0.50),
                percentile(&command_encode_values, 0.95),
                percentile(&command_encode_values, 0.99),
                command_encode_values.last().copied().unwrap_or(0.0),
                percentile(&event_to_submit_values, 0.50),
                percentile(&event_to_submit_values, 0.95),
                percentile(&event_to_submit_values, 0.99),
                event_to_submit_values.last().copied().unwrap_or(0.0),
                gpu_values.len(),
                percentile(&gpu_values, 0.50),
                percentile(&gpu_values, 0.95),
                percentile(&gpu_values, 0.99),
                gpu_values.last().copied().unwrap_or(0.0),
                hits as f64 / measured_frames as f64,
                misses as f64 / measured_frames as f64,
                traversed as f64 / measured_frames as f64,
                copied as f64 / measured_frames as f64,
                geometry as f64 / measured_frames as f64,
                geometry_uploads as f64 / measured_frames as f64,
                property_uploads as f64 / measured_frames as f64,
                property_records as f64 / measured_frames as f64,
                property_ring_bytes,
                stats.draws,
            );
            write_c19_samples(&mut report, "frame_samples_ms", &frame_values);
            write_c19_samples(&mut report, "active_frame_samples_ms", &active_frame_values);
            write_c19_samples(&mut report, "queue_wait_samples_ms", &queue_wait_values);
            write_c19_samples(&mut report, "encode_samples_ms", &encode_values);
            write_c19_samples(&mut report, "command_encode_samples_ms", &command_encode_values);
            write_c19_samples(&mut report, "event_to_submit_samples_ms", &event_to_submit_values);
            write_c19_samples(&mut report, "gpu_samples_ms", &gpu_values);
            write_c19_samples(&mut report, "warmup_samples_ms", &warmup_values);
            Ok(report)
        }

        pub async fn bench_webgpu_prepared_guardrails(&self) -> Result<String, JsValue> {
            let renderer = self.ensure_upload_bench_resources()?;
            let (image, atlas) = {
                let state = self.state.borrow();
                let resources = state
                    .bench_resources
                    .as_ref()
                    .ok_or_else(|| JsValue::from_str("missing WebGPU prepared resources"))?;
                (resources.image, resources.glyph_atlas)
            };
            let tiny = webgpu_prepared_single_snapshot(image, atlas, 0, 1)?;
            let bundled = webgpu_prepared_single_snapshot(image, atlas, 3, 1)?;
            let dirty = webgpu_prepared_single_snapshot(image, atlas, 3, 2)?;
            let aggregate = webgpu_prepared_snapshot(image, atlas, 1)?;
            let structural = webgpu_prepared_structural_snapshot(image, atlas)?;
            let segmented = webgpu_prepared_segmented_snapshot(image, atlas)?;
            let effect = webgpu_prepared_effect_guard_snapshot()?;
            let layer = webgpu_prepared_layer_guard_snapshot()?;
            let mut dynamic_instance = bundled
                .instance(0)
                .ok_or_else(|| JsValue::from_str("missing dynamic guard instance"))?;
            dynamic_instance.origin = [1.0, 0.0];
            let dynamic = gfx::RenderSnapshot::new(
                vec![dynamic_instance],
                Vec::new(),
                gfx::Damage { rects: Vec::new() },
            ).map_err(|error| JsValue::from_str(&format!("dynamic prepared guard: {error}")))?;
            let mut flat = gfx::DrawList::default();
            {
                let mut renderer = renderer.borrow_mut();
                renderer.resize(1_200, 800, 1.0).map_err(render_err)?;
                renderer.set_prepared_cache_budget_bytes(32 * 1024 * 1024);
                renderer.set_prepared_bundle_min_draws_for_benchmark(8);
                renderer.purge_prepared_chunks();
            }

            webgpu_prepared_frame(&renderer, &aggregate, false, &mut flat)?;
            webgpu_prepared_frame(&renderer, &aggregate, false, &mut flat)?;
            webgpu_prepared_frame(&renderer, &structural, false, &mut flat)?;
            let structural_stats = renderer.borrow().last_stats();
            renderer.borrow_mut().purge_prepared_chunks();

            webgpu_prepared_frame(&renderer, &segmented, false, &mut flat)?;
            let segmented_stats = renderer.borrow().last_stats();

            webgpu_prepared_frame(&renderer, &tiny, false, &mut flat)?;
            let tiny_stats = renderer.borrow().last_stats();
            webgpu_prepared_frame(&renderer, &bundled, false, &mut flat)?;
            let bundled_cold = renderer.borrow().last_stats();
            webgpu_prepared_frame(&renderer, &bundled, false, &mut flat)?;
            let bundled_clean = renderer.borrow().last_stats();
            webgpu_prepared_frame(&renderer, &dirty, false, &mut flat)?;
            let dirty_stats = renderer.borrow().last_stats();
            webgpu_prepared_frame(&renderer, &dynamic, false, &mut flat)?;
            let dynamic_stats = renderer.borrow().last_stats();
            webgpu_prepared_frame(&renderer, &effect, false, &mut flat)?;
            let effect_stats = renderer.borrow().last_stats();
            webgpu_prepared_frame(&renderer, &layer, false, &mut flat)?;
            let layer_stats = renderer.borrow().last_stats();

            renderer.borrow_mut().advance_prepared_device_generation_for_benchmark();
            webgpu_prepared_frame(&renderer, &bundled, false, &mut flat)?;
            let device_stats = renderer.borrow().last_stats();
            renderer.borrow_mut().resize(1_199, 800, 1.0).map_err(render_err)?;
            webgpu_prepared_frame(&renderer, &bundled, false, &mut flat)?;
            let resize_stats = renderer.borrow().last_stats();
            renderer.borrow_mut().resize(1_200, 800, 1.0).map_err(render_err)?;

            let pixels_a = [255_u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
            let pixels_b = [0_u8, 255, 255, 255, 255, 0, 255, 255, 255, 255, 0, 255, 32, 64, 96, 255];
            let resource_image = renderer.borrow_mut().image_create_rgba8(2, 2, &pixels_a, 8);
            let resource_snapshot = webgpu_prepared_resource_snapshot(resource_image, 25_900, 1)?;
            webgpu_prepared_frame(&renderer, &resource_snapshot, false, &mut flat)?;
            webgpu_prepared_frame(&renderer, &resource_snapshot, false, &mut flat)?;
            let resource_clean = renderer.borrow().last_stats();
            renderer.borrow_mut().image_update_rgba8(
                resource_image,
                0,
                0,
                2,
                2,
                &pixels_b,
                8,
            ).map_err(render_err)?;
            webgpu_prepared_frame(&renderer, &resource_snapshot, false, &mut flat)?;
            let resource_update = renderer.borrow().last_stats();
            let released = renderer.borrow_mut().image_release(resource_image);
            let recreated_image = renderer.borrow_mut().image_create_rgba8(2, 2, &pixels_a, 8);
            let recreated_snapshot = webgpu_prepared_resource_snapshot(recreated_image, 25_900, 2)?;
            webgpu_prepared_frame(&renderer, &recreated_snapshot, false, &mut flat)?;
            let resource_recreate = renderer.borrow().last_stats();
            let generation_changed = recreated_image != resource_image;
            let _ = renderer.borrow_mut().image_release(recreated_image);

            renderer.borrow_mut().set_prepared_cache_budget_bytes(0);
            webgpu_prepared_frame(&renderer, &bundled, false, &mut flat)?;
            let budget_stats = renderer.borrow().last_stats();
            let budget_bytes = renderer.borrow().prepared_cache_resident_bytes();
            renderer.borrow_mut().set_prepared_cache_budget_bytes(32 * 1024 * 1024);
            wait_renderer_queue_idle(&renderer).await?;
            renderer.borrow_mut().set_prepared_bundle_min_draws_for_benchmark(8);

            Ok(format!(
                "structural_hits={};structural_misses={};structural_traversed={};structural_bundle_creates={};structural_bundle_replays={};structural_bundle_execute_calls={};structural_bundle_draws={};structural_direct_draws={};segmented_bundle_creates={};segmented_bundle_replays={};segmented_direct_draws={};tiny_misses={};tiny_bundle_creates={};tiny_bundle_replays={};tiny_direct_draws={};cold_misses={};cold_bundle_creates={};clean_hits={};clean_misses={};clean_upload_bytes={};clean_bundle_replays={};dirty_hits={};dirty_misses={};dirty_traversed={};dynamic_hits={};dynamic_misses={};dynamic_commands_copied={};dynamic_bundle_replays={};effect_commands_copied={};effect_bundle_replays={};layer_commands_copied={};layer_bundle_replays={};device_misses={};resize_misses={};resource_clean_hits={};resource_update_hits={};resource_update_misses={};resource_update_evictions={};resource_recreate_misses={};resource_recreate_evictions={};resource_released={};resource_generation_changed={};budget_cache_bytes={};budget_commands_copied={};budget_upload_bytes={};budget_buffer_grows={};budget_bundle_replays={}",
                structural_stats.backend_cache_hits,
                structural_stats.backend_cache_misses,
                structural_stats.commands_traversed,
                structural_stats.render_bundle_creates,
                structural_stats.render_bundle_replays,
                structural_stats.render_bundle_execute_calls,
                structural_stats.render_bundle_draws,
                structural_stats.prepared_direct_draws,
                segmented_stats.render_bundle_creates,
                segmented_stats.render_bundle_replays,
                segmented_stats.prepared_direct_draws,
                tiny_stats.backend_cache_misses,
                tiny_stats.render_bundle_creates,
                tiny_stats.render_bundle_replays,
                tiny_stats.prepared_direct_draws,
                bundled_cold.backend_cache_misses,
                bundled_cold.render_bundle_creates,
                bundled_clean.backend_cache_hits,
                bundled_clean.backend_cache_misses,
                bundled_clean.buffer_upload_bytes,
                bundled_clean.render_bundle_replays,
                dirty_stats.backend_cache_hits,
                dirty_stats.backend_cache_misses,
                dirty_stats.commands_traversed,
                dynamic_stats.backend_cache_hits,
                dynamic_stats.backend_cache_misses,
                dynamic_stats.commands_copied,
                dynamic_stats.render_bundle_replays,
                effect_stats.commands_copied,
                effect_stats.render_bundle_replays,
                layer_stats.commands_copied,
                layer_stats.render_bundle_replays,
                device_stats.backend_cache_misses,
                resize_stats.backend_cache_misses,
                resource_clean.backend_cache_hits,
                resource_update.backend_cache_hits,
                resource_update.backend_cache_misses,
                resource_update.cache_evictions,
                resource_recreate.backend_cache_misses,
                resource_recreate.cache_evictions,
                u32::from(released),
                u32::from(generation_changed),
                budget_bytes,
                budget_stats.commands_copied,
                budget_stats.buffer_upload_bytes,
                budget_stats.buffer_grows,
                budget_stats.render_bundle_replays,
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
                renderer.set_draw_item_coalescing_enabled_for_benchmark(false);
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
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_item_coalescing_enabled_for_benchmark(true);
                renderer.set_direct_surface_enabled_for_benchmark(true);
            }
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{};expected_draw_items={};expected_image_draws={WEBGPU_DIRECT_SURFACE_DRAWS};columns={WEBGPU_DIRECT_SURFACE_COLUMNS};image_width={};image_height={}",
                sampled_case_metrics(&current, "current"),
                WEBGPU_DIRECT_SURFACE_DRAWS.saturating_add(1),
                WEBGPU_UPLOAD_IMAGE_SIZE,
                WEBGPU_UPLOAD_IMAGE_SIZE,
            ))
        }

        pub async fn bench_webgpu_draw_item_coalescing_ab(
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
                renderer.set_draw_item_coalescing_enabled_for_benchmark(true);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut current = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.draw_state_cache_frame(renderer)
                })
            })?;
            current.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                renderer.borrow_mut().set_draw_item_coalescing_enabled_for_benchmark(false);
            }
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut legacy = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.draw_state_cache_frame(renderer)
                })
            })?;
            legacy.stats = settle_renderer_timestamps(&renderer, timestamp_after_frame).await?;
            {
                renderer.borrow_mut().set_draw_item_coalescing_enabled_for_benchmark(true);
            }
            let ratio = if current.p50_ms > 0.0 { legacy.p50_ms / current.p50_ms } else { 0.0 };
            Ok(format!(
                "samples={sample_count};frames_per_sample={frames}{}{};legacy_over_current={ratio:.3};expected_source_draw_items={WEBGPU_DRAW_STATE_CACHE_DRAWS};expected_current_draw_items={WEBGPU_DRAW_ITEM_COALESCE_EXPECTED_ITEMS};columns={WEBGPU_DRAW_STATE_CACHE_COLUMNS};image_width={};image_height={}",
                sampled_case_metrics(&current, "current"),
                sampled_case_metrics(&legacy, "legacy"),
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
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_state_cache_enabled_for_benchmark(true);
                renderer.set_draw_item_coalescing_enabled_for_benchmark(false);
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
                let mut renderer = renderer.borrow_mut();
                renderer.set_draw_state_cache_enabled_for_benchmark(true);
                renderer.set_draw_item_coalescing_enabled_for_benchmark(true);
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

        pub async fn read_webgpu_asymmetric_id_mask_fields(&self) -> Result<String, JsValue>
        {
            let renderer = {
                let mut state = self.state.borrow_mut();
                state.direct_capture_active = true;
                state.renderer.clone()
            };
            {
                let mut renderer = renderer.borrow_mut();
                renderer.purge_id_mask_field_cache();
                webgpu_asymmetric_id_mask_frame(&mut renderer)?;
                webgpu_asymmetric_id_mask_frame(&mut renderer)?;
                renderer.begin_id_mask_snapshot_readback().map_err(render_err)?;
            }
            let readback = wait_webgpu_id_mask_snapshot(&renderer).await?;
            let stats = renderer.borrow().last_stats();
            Ok(id_mask_snapshot_json(&readback, stats))
        }

        pub async fn read_webgpu_id_mask_field_matrix(&self) -> Result<String, JsValue>
        {
            let renderer = {
                let mut state = self.state.borrow_mut();
                state.direct_capture_active = true;
                state.renderer.clone()
            };
            let mut json = String::from("{\"cases\":[");
            for (case_index, (width, height)) in [
                (256_usize, 256_usize),
                (512, 512),
                (1024, 1024),
                (2048, 2048),
                (257, 509),
                (2048, 257),
                (511, 1024),
            ].into_iter().enumerate()
            {
                let seed_x = width * 3 / 7;
                let seed_y = height * 5 / 11;
                {
                    let mut renderer = renderer.borrow_mut();
                    renderer.purge_id_mask_field_cache();
                    webgpu_single_seed_id_mask_frame(
                        &mut renderer,
                        width,
                        height,
                        seed_x,
                        seed_y,
                        case_index as u64 + 1,
                    )?;
                    renderer.begin_id_mask_snapshot_readback().map_err(render_err)?;
                }
                let readback = wait_webgpu_id_mask_snapshot(&renderer).await?;
                let pixel_count = width * height;
                if readback.width != width
                    || readback.height != height
                    || readback.city.len() != pixel_count
                    || readback.neighborhood.len() != pixel_count
                    || readback.city_field.len() != pixel_count
                    || readback.seam_field.len() != pixel_count
                {
                    return Err(JsValue::from_str("WebGPU ID-mask matrix readback shape mismatch"));
                }
                let seed_index = seed_y * width + seed_x;
                let mut city_mismatches = 0_usize;
                let mut neighborhood_mismatches = 0_usize;
                let mut city_field_mismatches = 0_usize;
                let mut seam_field_mismatches = 0_usize;
                for index in 0..pixel_count
                {
                    let expected_city = if index == seed_index { 2 } else { 0 };
                    let expected_neighborhood = if index == seed_index { 17 } else { 0 };
                    city_mismatches += usize::from(readback.city[index] != expected_city);
                    neighborhood_mismatches +=
                        usize::from(readback.neighborhood[index] != expected_neighborhood);
                    city_field_mismatches += usize::from(
                        readback.city_field[index]
                            != [seed_x as f32, seed_y as f32, 2.0, 17.0],
                    );
                    seam_field_mismatches += usize::from(
                        readback.seam_field[index] != [-1.0, -1.0, 0.0, 0.0],
                    );
                }
                if case_index != 0
                {
                    json.push(',');
                }
                let _ = write!(
                    json,
                    "{{\"width\":{width},\"height\":{height},\"seed_x\":{seed_x},\"seed_y\":{seed_y},\"packed_fields\":{},\"field_logical_bytes\":{},\"wide_field_logical_bytes\":{},\"city_mismatches\":{city_mismatches},\"neighborhood_mismatches\":{neighborhood_mismatches},\"city_field_mismatches\":{city_field_mismatches},\"seam_field_mismatches\":{seam_field_mismatches}}}",
                    readback.packed_fields,
                    readback.field_logical_bytes,
                    readback.wide_field_logical_bytes,
                );
            }
            json.push_str("]}");
            Ok(json)
        }

        pub fn set_scene(&self, scene_index: usize) {
            {
                let mut state = self.state.borrow_mut();
                state.router.set_scene(scene_index);
                state.mark_frame_dirty();
            }
            request_next_frame(&self.state);
        }

        pub fn reset_web_scheduler_metrics(&self) {
            let mut state = self.state.borrow_mut();
            state.raf_callbacks = 0;
            state.raf_requests = 0;
            state.invalidations = 0;
            state.canvas_metric_reads = 0;
            state.idle_skipped_frames = 0;
            state.submitted_frames = 0;
        }

        #[must_use]
        pub fn web_scheduler_metrics(&self) -> String {
            let state = self.state.borrow();
            let metrics = state.canvas_metrics;
            format!(
                "raf_callbacks={};raf_requests={};invalidations={};canvas_metric_reads={};idle_skipped_frames={};submitted_frames={};raf_pending={};last_timestamp_ms={:.6};frame_time_remainder_ms={:.6};physical_width={};physical_height={};css_width={:.3};css_height={:.3};scale={:.3};left={:.3};top={:.3}",
                state.raf_callbacks,
                state.raf_requests,
                state.invalidations,
                state.canvas_metric_reads,
                state.idle_skipped_frames,
                state.submitted_frames,
                u8::from(state.raf_pending),
                state.last_timestamp_ms,
                state.frame_time_remainder_ms,
                metrics.physical_width,
                metrics.physical_height,
                metrics.css_width,
                metrics.css_height,
                metrics.scale,
                metrics.left,
                metrics.top,
            )
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

        async fn sample_backdrop_region_case(
            &self,
            renderer: &Rc<RefCell<BrowserRenderer>>,
            case: BackdropRegionCase,
            sample_count: u32,
            frames: u32,
        ) -> Result<WebGpuBenchSummary, JsValue> {
            let timestamp_after_frame = renderer.borrow().last_stats().frame_id;
            let mut summary = self.with_upload_bench_resources(|renderer, resources| {
                bench_webgpu_sampled_case(renderer, sample_count, frames, |renderer, _, _| {
                    resources.backdrop_region_frame(renderer, case)
                })
            })?;
            summary.stats = settle_renderer_timestamps(renderer, timestamp_after_frame).await?;
            Ok(summary)
        }

        fn ensure_geometry_bench_resources(&self) -> Result<Rc<RefCell<BrowserRenderer>>, JsValue> {
            let renderer = self.ensure_upload_bench_resources()?;
            if self.state.borrow().geometry_bench_resources.is_none() {
                let (glyph_atlas, image) = {
                    let state = self.state.borrow();
                    let Some(resources) = state.bench_resources.as_ref() else {
                        return Err(JsValue::from_str("WebGPU upload benchmark resources unavailable"));
                    };
                    (resources.glyph_atlas, resources.image)
                };
                let resources = WebGpuGeometryBenchResources::new(glyph_atlas, image)?;
                self.state.borrow_mut().geometry_bench_resources = Some(resources);
            }
            Ok(renderer)
        }

        fn with_geometry_bench_resources<T>(
            &self,
            f: impl FnOnce(&mut BrowserRenderer, &WebGpuGeometryBenchResources) -> Result<T, JsValue>,
        ) -> Result<T, JsValue> {
            let renderer = self.ensure_geometry_bench_resources()?;
            let state = self.state.borrow();
            let Some(resources) = state.geometry_bench_resources.as_ref() else {
                return Err(JsValue::from_str("WebGPU geometry benchmark resources unavailable"));
            };
            let mut renderer = renderer.borrow_mut();
            f(&mut renderer, resources)
        }

        fn ensure_glyph_matrix_resources(&self) -> Result<Rc<RefCell<BrowserRenderer>>, JsValue> {
            let renderer = self.state.borrow().renderer.clone();
            if self.state.borrow().glyph_matrix_resources.is_none() {
                let resources = {
                    let mut renderer = renderer.borrow_mut();
                    WebGpuGlyphMatrixResources::new(&mut renderer)?
                };
                self.state.borrow_mut().glyph_matrix_resources = Some(resources);
            }
            Ok(renderer)
        }

        fn with_glyph_matrix_resources<T>(
            &self,
            f: impl FnOnce(&mut BrowserRenderer, &WebGpuGlyphMatrixResources) -> Result<T, JsValue>,
        ) -> Result<T, JsValue> {
            let renderer = self.ensure_glyph_matrix_resources()?;
            let state = self.state.borrow();
            let Some(resources) = state.glyph_matrix_resources.as_ref() else {
                return Err(JsValue::from_str("WebGPU glyph matrix resources unavailable"));
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

    /// Runs the non-default Canvas2D indexed-quad diagnostic workload.
    #[wasm_bindgen]
    pub fn bench_canvas_indexed_quads(
        samples: u32,
        frames_per_sample: u32,
        quads: u32,
    ) -> Result<String, JsValue> {
        let canvas = create_hidden_canvas(512, 512)?;
        let report = oxide_renderer_web::bench_canvas_indexed_quads(
            canvas.clone(),
            samples,
            frames_per_sample,
            quads,
        )
        .map_err(render_err);
        remove_hidden_canvas(&canvas);
        report
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
        Ok(has.call1(&features, &JsValue::from_str(feature))?.as_bool().unwrap_or(false))
    }

    struct WebGpuIdMaskBenchSummary {
        warmup_ms: f64,
        p50_ms: f64,
        p95_ms: f64,
        p99_ms: f64,
        peak_ms: f64,
        avg_ms: f64,
        allocations: WebGpuAllocationSummary,
        submit_allocations: WebGpuSubmitAllocationSummary,
        vertices: usize,
        vertex_bytes: usize,
        stats: WebRendererStats,
    }

    struct WebGpuBenchSummary {
        warmup_ms: f64,
        p50_ms: f64,
        p95_ms: f64,
        p99_ms: f64,
        peak_ms: f64,
        avg_ms: f64,
        allocations: WebGpuAllocationSummary,
        submit_allocations: WebGpuSubmitAllocationSummary,
        stats: WebRendererStats,
    }

    #[derive(Clone, Copy, Default)]
    struct Scene3dGpuDistribution {
        samples: usize,
        p50_ns: f64,
        p95_ns: f64,
        p99_ns: f64,
        peak_ns: f64,
    }

    fn drain_scene3d_gpu_distribution(
        renderer: &mut BrowserRenderer,
    ) -> Scene3dGpuDistribution {
        let mut completed = Vec::new();
        renderer.drain_completed_timestamp_samples_into(&mut completed);
        let mut values = completed
            .into_iter()
            .map(|sample| sample.scene3d_ns.saturating_add(sample.scene3d_overlay_ns) as f64)
            .filter(|value| *value > 0.0)
            .collect::<Vec<_>>();
        values.sort_by(f64::total_cmp);
        Scene3dGpuDistribution {
            samples: values.len(),
            p50_ns: percentile(&values, 0.50),
            p95_ns: percentile(&values, 0.95),
            p99_ns: percentile(&values, 0.99),
            peak_ns: values.last().copied().unwrap_or(0.0),
        }
    }

    fn scene3d_gpu_distribution_metrics(
        distribution: Scene3dGpuDistribution,
        prefix: &str,
    ) -> String {
        format!(
            ";{prefix}_gpu_samples={};{prefix}_gpu_p50_ns={:.0};{prefix}_gpu_p95_ns={:.0};{prefix}_gpu_p99_ns={:.0};{prefix}_gpu_peak_ns={:.0}",
            distribution.samples,
            distribution.p50_ns,
            distribution.p95_ns,
            distribution.p99_ns,
            distribution.peak_ns,
        )
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
    struct WebGpuSubmitAllocationSummary {
        upload_alloc_count: u64,
        upload_alloc_bytes: u64,
        surface_alloc_count: u64,
        surface_alloc_bytes: u64,
        encoder_alloc_count: u64,
        encoder_alloc_bytes: u64,
        render_alloc_count: u64,
        render_alloc_bytes: u64,
        timestamp_alloc_count: u64,
        timestamp_alloc_bytes: u64,
        scratch_stats_alloc_count: u64,
        scratch_stats_alloc_bytes: u64,
        finish_queue_alloc_count: u64,
        finish_queue_alloc_bytes: u64,
        present_alloc_count: u64,
        present_alloc_bytes: u64,
        timestamp_map_alloc_count: u64,
        timestamp_map_alloc_bytes: u64,
        total_alloc_count: u64,
        total_alloc_bytes: u64,
        total_realloc_count: u64,
        total_realloc_grow_bytes: u64,
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

    #[derive(Clone, Copy, Default)]
    struct WebGpuFrameStageTimingSample {
        values_ms: [f64; WEBGPU_FRAME_STAGES.len()],
        total_ms: f64,
        cpu_submit: WebGpuCpuSubmitTimingSample,
    }

    #[derive(Clone, Copy)]
    #[repr(usize)]
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

    #[derive(Clone, Copy)]
    enum WebGpuArchitectureMatrixKind
    {
        Full,
        RRect,
        Image,
        NineSlice,
        Spinner,
        NeonMarker,
    }

    struct WebGpuGeometryBenchResources {
        glyphs: gfx::DrawList,
        images: gfx::DrawList,
        large_mesh: gfx::DrawList,
    }

    impl WebGpuGeometryBenchResources {
        fn new(glyph_atlas: gfx::ImageHandle, image: gfx::ImageHandle) -> Result<Self, JsValue> {
            Ok(Self {
                glyphs: webgpu_geometry_glyphs(glyph_atlas)?,
                images: webgpu_geometry_images(image),
                large_mesh: webgpu_geometry_large_mesh(),
            })
        }

        fn frame(renderer: &mut BrowserRenderer, list: &gfx::DrawList) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            renderer.encode_pass(list);
            renderer.submit(token).map_err(render_err)
        }
    }

    struct WebGpuGlyphMatrixResources {
        drawlist: gfx::DrawList,
        atlas_pages: usize,
        glyph_quads: usize,
        bitmap_runs: usize,
        sdf_runs: usize,
    }

    impl WebGpuGlyphMatrixResources {
        fn new(renderer: &mut BrowserRenderer) -> Result<Self, JsValue> {
            let mut text = ui::elements::TextCtx::default();
            let latin_id = text.fonts.add_font(text::Font::from_bytes(C46_LATIN_FONT_BYTES.to_vec()));
            let cjk_id = text.fonts.add_font(text::Font::from_bytes(C46_CJK_FONT_BYTES.to_vec()));
            text.atlas = text::PagedAtlas::new(128, 128, text::DEFAULT_GLYPH_ATLAS_PAGE_COUNT);
            text.set_fallback_fonts(&[cjk_id]);
            let mut supported = Vec::new();
            for scalar in 0x21_u32..0x3000 {
                let Some(ch) = char::from_u32(scalar) else { continue };
                if !ch.is_whitespace() && text.fonts.font_supports_cluster(latin_id, &ch.to_string()) {
                    supported.push(ch);
                }
            }
            if supported.is_empty() {
                return Err(JsValue::from_str("C46 glyph matrix requires supported Latin glyphs"));
            }
            let labels = (0..1_000_usize).map(|index| {
                let unique = supported[index % supported.len()];
                match index % 11 {
                    0 => format!("Wrapped Latin {unique} words {index:04} across a narrow row"),
                    1 => format!("RTL مرحبا {unique} {index:04}"),
                    2 => format!("CJK 漢字 {unique} {index:04}"),
                    3 => format!("Emoji 😀 {unique} {index:04}"),
                    _ => format!("Glyph {unique} size variant {index:04}"),
                }
            }).collect::<Vec<_>>();
            let palette = [
                gfx::Color::rgba(0.10, 0.12, 0.16, 1.0),
                gfx::Color::rgba(0.80, 0.20, 0.12, 0.90),
                gfx::Color::rgba(0.12, 0.45, 0.85, 0.75),
                gfx::Color::rgba(0.20, 0.70, 0.35, 1.0),
            ];
            let mut builder = ui::DrawListBuilder::new();
            text.begin_frame();
            for (index, label) in labels.iter().enumerate() {
                let font_px = 16.0 + (index % 20) as f32;
                let wrap = index % 11 == 0;
                ui::elements::encode_label_text(
                    label,
                    palette[index % palette.len()],
                    ui::elements::Align::Left,
                    wrap,
                    latin_id,
                    font_px,
                    gfx::RectF::new(
                        (index % 5) as f32 * 238.0,
                        (index % 40) as f32 * 20.0,
                        if wrap { 180.0 } else { 232.0 },
                        if wrap { 48.0 } else { 40.0 },
                    ),
                    1.0,
                    &mut text,
                    &mut BorrowedWebUploader { renderer },
                    &mut builder,
                );
            }
            text.finish_frame(&mut BorrowedWebUploader { renderer }, &mut builder);
            ui::coalesce_adjacent_draws(builder.drawlist_mut());
            let mut atlas_pages = HashSet::new();
            let mut glyph_quads = 0_usize;
            let mut bitmap_runs = 0_usize;
            let mut sdf_runs = 0_usize;
            for item in &builder.drawlist().items {
                let gfx::DrawCmd::GlyphRun { run } = item else { continue };
                atlas_pages.insert(run.atlas);
                glyph_quads = glyph_quads.saturating_add(run.ib.len as usize / 6);
                if run.sdf {
                    sdf_runs = sdf_runs.saturating_add(1);
                } else {
                    bitmap_runs = bitmap_runs.saturating_add(1);
                }
            }
            if atlas_pages.len() < 2 || bitmap_runs == 0 || sdf_runs == 0 {
                return Err(JsValue::from_str("C46 glyph matrix requires multiple pages and bitmap/SDF text"));
            }
            Ok(Self {
                drawlist: builder.drawlist().clone(),
                atlas_pages: atlas_pages.len(),
                glyph_quads,
                bitmap_runs,
                sdf_runs,
            })
        }

        fn frame(&self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(1_200, 800, 1.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            renderer.encode_pass(&self.drawlist);
            renderer.submit(token).map_err(render_err)
        }
    }

    #[derive(Clone, Copy)]
    enum BackdropRegionCase
    {
        Separated48,
        Coalescible12,
        Fullscreen,
        EdgesAndCorners,
        NestedLayers,
        MixedSigma,
    }

    impl BackdropRegionCase
    {
        const fn from_u32(value: u32) -> Option<Self>
        {
            match value
            {
                0 => Some(Self::Separated48),
                1 => Some(Self::Coalescible12),
                2 => Some(Self::Fullscreen),
                3 => Some(Self::EdgesAndCorners),
                4 => Some(Self::NestedLayers),
                5 => Some(Self::MixedSigma),
                _ => None,
            }
        }

        const fn name(self) -> &'static str
        {
            match self
            {
                Self::Separated48 => "separated-48",
                Self::Coalescible12 => "coalescible-12",
                Self::Fullscreen => "fullscreen",
                Self::EdgesAndCorners => "edges-corners",
                Self::NestedLayers => "nested-layers",
                Self::MixedSigma => "mixed-sigma",
            }
        }
    }

    struct WebGpuUploadBenchResources {
        glyph_atlas: gfx::ImageHandle,
        image: gfx::ImageHandle,
        image_alt: gfx::ImageHandle,
        full_a8: Vec<u8>,
        dirty_a8: Vec<u8>,
        full_a8_row_bytes: usize,
        dirty_a8_row_bytes: usize,
        full_rgba: Vec<u8>,
        dirty_rgba: Vec<u8>,
        neon_markers: Vec<neon_marker::NeonMarker>,
        architecture_neon_markers: Vec<neon_marker::NeonMarker>,
        builder: ui::DrawListBuilder,
        mixed_damage: gfx::Damage,
        layer_effects_damage: gfx::Damage,
        dynamic_property_snapshots: Option<[gfx::RenderSnapshot; 2]>,
        dynamic_property_affine_snapshots: Option<[gfx::RenderSnapshot; 2]>,
    }

    impl WebGpuUploadBenchResources {
        fn new(renderer: &mut BrowserRenderer) -> Result<Self, JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let full_a8_row_bytes = WEBGPU_UPLOAD_ATLAS_SIZE as usize + 3;
            let dirty_a8_row_bytes = WEBGPU_UPLOAD_DIRTY_SIZE as usize + 3;
            let full_a8 = glyph_upload_a8(WEBGPU_UPLOAD_ATLAS_SIZE, full_a8_row_bytes);
            let dirty_a8 = glyph_upload_a8(WEBGPU_UPLOAD_DIRTY_SIZE, dirty_a8_row_bytes);
            let full_rgba =
                generate_checker_rgba(WEBGPU_UPLOAD_IMAGE_SIZE, WEBGPU_UPLOAD_IMAGE_SIZE);
            let dirty_rgba =
                generate_checker_rgba(WEBGPU_UPLOAD_DIRTY_SIZE, WEBGPU_UPLOAD_DIRTY_SIZE);
            let mut neon_markers = Vec::with_capacity(WEBGPU_NEON_MARKERS);
            webgpu_fill_neon_markers(&mut neon_markers, WEBGPU_NEON_MARKERS, WEBGPU_NEON_MARKER_COLUMNS, 24.0, 26.0, 28.0, 22.0);
            let mut architecture_neon_markers = Vec::with_capacity(WEBGPU_ARCHITECTURE_NEON_MARKERS);
            webgpu_fill_neon_markers(&mut architecture_neon_markers, WEBGPU_ARCHITECTURE_NEON_MARKERS, 32, 4.0, 4.0, 7.75, 7.75);
            let glyph_atlas = renderer.image_create_a8(
                WEBGPU_UPLOAD_ATLAS_SIZE,
                WEBGPU_UPLOAD_ATLAS_SIZE,
                &full_a8,
                full_a8_row_bytes,
            );
            let image = renderer.image_create_rgba8(
                WEBGPU_UPLOAD_IMAGE_SIZE,
                WEBGPU_UPLOAD_IMAGE_SIZE,
                &full_rgba,
                WEBGPU_UPLOAD_IMAGE_SIZE as usize * 4,
            );
            let mut alternate_rgba = full_rgba.clone();
            for pixel in alternate_rgba.chunks_exact_mut(4) {
                pixel.swap(0, 2);
                pixel[1] = 255_u8.saturating_sub(pixel[1]);
            }
            let image_alt = renderer.image_create_rgba8(
                WEBGPU_UPLOAD_IMAGE_SIZE,
                WEBGPU_UPLOAD_IMAGE_SIZE,
                &alternate_rgba,
                WEBGPU_UPLOAD_IMAGE_SIZE as usize * 4,
            );
            Ok(Self {
                glyph_atlas,
                image,
                image_alt,
                full_a8,
                dirty_a8,
                full_a8_row_bytes,
                dirty_a8_row_bytes,
                full_rgba,
                dirty_rgba,
                neon_markers,
                architecture_neon_markers,
                builder: ui::DrawListBuilder::new(),
                mixed_damage: gfx::Damage {
                    rects: vec![gfx::RectI::new(0, 0, 128, 128), gfx::RectI::new(64, 64, 192, 192)],
                },
                layer_effects_damage: gfx::Damage {
                    rects: vec![
                        gfx::RectI::new(0, 0, 96, 96),
                        gfx::RectI::new(96, 64, 180, 132),
                        gfx::RectI::new(30, 180, 220, 70),
                    ],
                },
                dynamic_property_snapshots: None,
                dynamic_property_affine_snapshots: None,
            })
        }

        fn dynamic_property_snapshot(&mut self, phase: usize, full_affine: bool) -> Result<gfx::RenderSnapshot, JsValue> {
            let snapshots = if full_affine {
                &mut self.dynamic_property_affine_snapshots
            } else {
                &mut self.dynamic_property_snapshots
            };
            if snapshots.is_none() {
                let instances = webgpu_dynamic_property_instances(self.image, self.glyph_atlas)?;
                *snapshots = Some([
                    webgpu_dynamic_property_snapshot(&instances, 0, full_affine)?,
                    webgpu_dynamic_property_snapshot(&instances, 1, full_affine)?,
                ]);
            }
            snapshots.as_ref()
                .and_then(|snapshots| snapshots.get(phase & 1))
                .cloned()
                .ok_or_else(|| JsValue::from_str("WebGPU dynamic property snapshot unavailable"))
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
                    self.dirty_a8_row_bytes,
                );
            } else {
                renderer.image_update_a8(
                    self.glyph_atlas,
                    0,
                    0,
                    WEBGPU_UPLOAD_ATLAS_SIZE,
                    WEBGPU_UPLOAD_ATLAS_SIZE,
                    &self.full_a8,
                    self.full_a8_row_bytes,
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

        fn glyph_cold_create_frame(
            &mut self,
            renderer: &mut BrowserRenderer,
        ) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            let atlas = renderer.image_create_a8(
                WEBGPU_UPLOAD_ATLAS_SIZE,
                WEBGPU_UPLOAD_ATLAS_SIZE,
                &self.full_a8,
                self.full_a8_row_bytes,
            );
            if atlas.0 == 0 {
                return Err(JsValue::from_str("failed to create cold C15 glyph atlas"));
            }
            self.builder.clear();
            if !append_glyph_grid(
                &mut self.builder,
                atlas,
                WEBGPU_MIXED_GLYPHS,
                18.0,
                24.0,
                18.0,
                true,
                gfx::Color::rgba(0.95, 0.98, 1.0, 1.0),
            ) {
                return Err(JsValue::from_str("failed to build cold C15 glyph draw list"));
            }
            renderer.encode_pass(self.builder.drawlist());
            let result = renderer.submit(token).map_err(render_err);
            renderer.image_release(atlas);
            result
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
                    self.dirty_a8_row_bytes,
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

        fn backdrop_region_frame(
            &mut self,
            renderer: &mut BrowserRenderer,
            case: BackdropRegionCase,
        ) -> Result<(), JsValue>
        {
            match case
            {
                BackdropRegionCase::Separated48 => self.effect_uniform_frame(renderer),
                BackdropRegionCase::Coalescible12 => self.backdrop_batch_frame(renderer),
                BackdropRegionCase::Fullscreen => self.backdrop_fullscreen_frame(renderer),
                BackdropRegionCase::EdgesAndCorners => self.backdrop_edges_frame(renderer),
                BackdropRegionCase::NestedLayers => self.backdrop_nested_frame(renderer),
                BackdropRegionCase::MixedSigma => self.backdrop_mixed_sigma_frame(renderer),
            }
        }

        fn backdrop_frame_begin(&mut self, renderer: &mut BrowserRenderer) -> Result<gfx::FrameToken, JsValue>
        {
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
            Ok(token)
        }

        fn backdrop_frame_end(
            &mut self,
            renderer: &mut BrowserRenderer,
            token: gfx::FrameToken,
        ) -> Result<(), JsValue>
        {
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn backdrop_fullscreen_frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue>
        {
            let token = self.backdrop_frame_begin(renderer)?;
            self.builder.backdrop(
                gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                24.0,
                gfx::Color::rgba(0.05, 0.08, 0.12, 0.28),
                1.0,
            );
            self.backdrop_frame_end(renderer, token)
        }

        fn backdrop_edges_frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue>
        {
            let token = self.backdrop_frame_begin(renderer)?;
            for (x, y) in [(-12.0, -8.0), (218.0, -8.0), (-12.0, 216.0), (218.0, 216.0)]
            {
                self.builder.backdrop(
                    gfx::RectF::new(x, y, 50.0, 48.0),
                    12.0,
                    gfx::Color::rgba(0.08, 0.10, 0.14, 0.34),
                    1.0,
                );
            }
            self.backdrop_frame_end(renderer, token)
        }

        fn backdrop_nested_frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue>
        {
            let token = self.backdrop_frame_begin(renderer)?;
            self.builder.layer_begin(49_001, gfx::RectF::new(20.0, 20.0, 216.0, 216.0), true);
            self.builder.rrect(
                gfx::RectF::new(20.0, 20.0, 216.0, 216.0),
                [14.0; 4],
                gfx::Color::rgba(0.08, 0.11, 0.16, 0.92),
            );
            self.builder.layer_begin(49_002, gfx::RectF::new(44.0, 44.0, 168.0, 168.0), true);
            self.builder.image(
                self.image_alt,
                gfx::RectF::new(44.0, 44.0, 168.0, 168.0),
                gfx::RectF::new(
                    0.0,
                    0.0,
                    WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                    WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                ),
                0.88,
            );
            self.builder.backdrop(
                gfx::RectF::new(58.0, 58.0, 54.0, 52.0),
                8.0,
                gfx::Color::rgba(0.08, 0.10, 0.16, 0.30),
                1.0,
            );
            self.builder.backdrop(
                gfx::RectF::new(144.0, 142.0, 54.0, 52.0),
                14.0,
                gfx::Color::rgba(0.10, 0.08, 0.16, 0.34),
                1.0,
            );
            self.builder.layer_end();
            self.builder.backdrop(
                gfx::RectF::new(92.0, 174.0, 72.0, 42.0),
                10.0,
                gfx::Color::rgba(0.06, 0.12, 0.14, 0.30),
                1.0,
            );
            self.builder.layer_end();
            self.backdrop_frame_end(renderer, token)
        }

        fn backdrop_mixed_sigma_frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue>
        {
            let token = self.backdrop_frame_begin(renderer)?;
            for (index, sigma) in [0.0, 2.0, 6.0, 12.0, 24.0, 48.0].into_iter().enumerate()
            {
                let col = index % 3;
                let row = index / 3;
                self.builder.backdrop(
                    gfx::RectF::new(18.0 + col as f32 * 80.0, 34.0 + row as f32 * 104.0, 48.0, 48.0),
                    sigma,
                    gfx::Color::rgba(0.06, 0.09, 0.14, 0.32),
                    1.0,
                );
            }
            self.backdrop_frame_end(renderer, token)
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

        fn clean_layer_frame(
            &mut self,
            renderer: &mut BrowserRenderer,
            dirty: bool,
        ) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            self.builder.clear();
            self.builder.rrect(
                gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                [0.0; 4],
                gfx::Color::rgba(0.035, 0.043, 0.055, 1.0),
            );
            self.builder.layer_begin(301, gfx::RectF::new(18.0, 18.0, 220.0, 220.0), dirty);
            self.builder.clip_push(gfx::RectI::new(18, 18, 220, 220));
            self.builder.rrect(
                gfx::RectF::new(20.0, 20.0, 216.0, 216.0),
                [18.0; 4],
                gfx::Color::rgba(0.08, 0.16, 0.22, 0.92),
            );
            for index in 0..WEBGPU_CLEAN_LAYER_IMAGE_TILES {
                let col = index % WEBGPU_CLEAN_LAYER_IMAGE_COLUMNS;
                let row = index / WEBGPU_CLEAN_LAYER_IMAGE_COLUMNS;
                let x = 24.0 + col as f32 * 13.0;
                let y = 82.0 + row as f32 * 11.0;
                self.builder.image(
                    self.image,
                    gfx::RectF::new(x, y, 10.0, 8.0),
                    gfx::RectF::new(
                        0.0,
                        0.0,
                        WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                        WEBGPU_UPLOAD_IMAGE_SIZE as f32,
                    ),
                    0.42,
                );
            }
            if !append_glyph_grid(
                &mut self.builder,
                self.glyph_atlas,
                WEBGPU_CLEAN_LAYER_GLYPHS,
                30.0,
                38.0,
                10.0,
                false,
                gfx::Color::rgba(0.94, 0.98, 1.0, 0.95),
            ) {
                return Err(JsValue::from_str("failed to build clean layer glyph draw list"));
            }
            self.builder.rrect(
                gfx::RectF::new(50.0, 194.0, 164.0, 20.0),
                [8.0; 4],
                gfx::Color::rgba(0.88, 0.43, 0.14, 0.82),
            );
            self.builder.clip_pop();
            self.builder.layer_end();
            self.builder.rrect(
                gfx::RectF::new(22.0, 22.0, 212.0, 212.0),
                [18.0; 4],
                gfx::Color::rgba(0.02, 0.04, 0.05, 0.10),
            );
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
                    return Err(JsValue::from_str(
                        "failed to build command family SDF glyph draw list",
                    ));
                }
            }
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn glyph_run_frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            self.builder.clear();
            self.builder.rrect(
                gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                [0.0; 4],
                gfx::Color::rgba(0.035, 0.045, 0.060, 1.0),
            );
            let columns = 8_usize;
            let cell = 7.0_f32;
            for run in 0..WEBGPU_GLYPH_RUN_RUNS {
                let col = run % columns;
                let row = run / columns;
                let sdf = run >= WEBGPU_GLYPH_RUN_RUNS.saturating_sub(WEBGPU_GLYPH_RUN_SDF_RUNS);
                let x = 10.0 + col as f32 * 30.0;
                let y = 12.0 + row as f32 * 29.0;
                let color = if sdf {
                    gfx::Color::rgba(0.76, 0.93, 1.0, 0.94)
                } else {
                    gfx::Color::rgba(0.95, 0.97, 1.0, 0.96)
                };
                if !append_glyph_grid(
                    &mut self.builder,
                    self.glyph_atlas,
                    WEBGPU_GLYPH_RUN_GLYPHS_PER_RUN,
                    x,
                    y,
                    cell,
                    sdf,
                    color,
                ) {
                    return Err(JsValue::from_str("failed to build glyph-run draw list"));
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
                    markers: &self.neon_markers[..WEBGPU_NEON_MARKERS],
                })
                .map_err(render_err)?;
            renderer.submit(token).map_err(render_err)
        }

        fn architecture_primitive_frame(
            &mut self,
            renderer: &mut BrowserRenderer,
            kind: &str,
            count: usize,
            dpr: f32,
        ) -> Result<(), JsValue> {
            let physical = (256.0 * dpr).round() as u32;
            renderer.resize(physical, physical, dpr).map_err(render_err)?;
            renderer.set_animation_time_ms(0.0);
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            if kind == "neon" {
                for markers in self.architecture_neon_markers[..count]
                    .chunks(neon_marker::NEON_MARKER_MAX_INSTANCES)
                {
                    renderer
                        .encode_neon_markers(&neon_marker::NeonMarkerPass {
                            viewport: gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                            markers,
                        })
                        .map_err(render_err)?;
                }
            } else {
                self.builder.clear();
                if kind == "image" || kind == "image_mixed"
                {
                    self.builder.clip_push(gfx::RectI::new(1, 1, 254, 254));
                    for index in 0..count
                    {
                        let x = (index % 32) as f32 * 8.0 + 0.25;
                        let y = (index / 32) as f32 * 8.0 + 0.5;
                        let src = match index % 4
                        {
                            0 => gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                            1 => gfx::RectF::new(32.0, 0.0, 192.0, 256.0),
                            2 => gfx::RectF::new(0.0, 40.0, 256.0, 176.0),
                            _ => gfx::RectF::new(48.0, 24.0, 160.0, 208.0),
                        };
                        let image = if kind == "image_mixed" && (index / 8) & 1 != 0
                        {
                            self.image_alt
                        }
                        else
                        {
                            self.image
                        };
                        self.builder.image(
                            image,
                            gfx::RectF::new(x, y, 7.5, 7.25),
                            src,
                            0.55 + (index % 4) as f32 * 0.15,
                        );
                    }
                    self.builder.clip_pop();
                }
                else if kind == "nine_slice"
                {
                    self.builder.clip_push(gfx::RectI::new(1, 1, 254, 254));
                    for index in 0..count
                    {
                        let x = (index % 32) as f32 * 8.0 + 0.25;
                        let y = (index / 32) as f32 * 8.0 + 0.5;
                        let (width, height, slice) = match index % 4
                        {
                            0 => (7.5, 7.25, gfx::Insets::new(2.0, 2.0, 2.0, 2.0)),
                            1 => (3.25, 5.5, gfx::Insets::new(4.0, 3.0, 5.0, 4.0)),
                            2 => (9.25, 4.75, gfx::Insets::new(0.5, 6.25, 3.75, 7.5)),
                            _ => (6.5, 8.25, gfx::Insets::new(300.0, -2.0, 300.0, 260.0)),
                        };
                        self.builder.nine_slice(
                            self.image,
                            gfx::RectF::new(x, y, width, height),
                            slice,
                            0.55 + (index % 4) as f32 * 0.15,
                        );
                    }
                    self.builder.clip_pop();
                }
                else
                {
                    for index in 0..count {
                        let x = (index % 32) as f32 * 8.0;
                        let y = (index / 32) as f32 * 8.0;
                        if kind == "rrect" {
                            self.builder.rrect(
                                gfx::RectF::new(x, y, 7.0, 7.0),
                                [2.0; 4],
                                gfx::Color::rgba(0.18, 0.62, 0.94, 0.92),
                            );
                        } else if kind == "rrect_pathological" {
                            let radii = match index % 8 {
                                0 => [-4.0, 0.0, 2.0, 20.0],
                                1 => [20.0, 2.0, 0.0, -4.0],
                                2 => [0.0; 4],
                                3 => [0.25, 1.0, 3.5, 12.0],
                                4 => [3.5; 4],
                                5 => [64.0; 4],
                                6 => [1.0, 2.0, 3.0, 4.0],
                                _ => [6.75, 0.5, 6.75, 0.5],
                            };
                            let width = if index & 1 == 0 { 7.0 } else { 3.0 };
                            let height = if index & 2 == 0 { 7.0 } else { 5.0 };
                            self.builder.rrect(
                                gfx::RectF::new(x, y, width, height),
                                radii,
                                gfx::Color::rgba(0.18, 0.62, 0.94, 0.92),
                            );
                        } else if kind == "spinner" {
                            self.builder.spinner([x + 3.5, y + 3.5], 3.0 + (index % 7) as f32 * 0.05, 1.0);
                        } else {
                            self.builder.nine_slice(
                                self.image,
                                gfx::RectF::new(x, y, 7.0, 7.0),
                                gfx::Insets::new(2.0, 2.0, 2.0, 2.0),
                                0.92,
                            );
                        }
                    }
                }
                renderer.encode_pass(self.builder.drawlist());
            }
            renderer.submit(token).map_err(render_err)?;
            let stats = renderer.last_stats();
            if stats.render_passes == 0 {
                return Err(JsValue::from_str(&format!(
                    "architecture primitive {kind}/{count} produced zero render passes: draws={} items={} command_buffers={} frame_id={} timestamp_interval={}",
                    stats.draws,
                    stats.draw_items,
                    stats.command_buffers,
                    stats.frame_id,
                    stats.gpu_timestamp_readback_interval,
                )));
            }
            Ok(())
        }

        fn rrect_capture_frame(
            &mut self,
            renderer: &mut BrowserRenderer,
            width: u32,
            height: u32,
            dpr: f32,
        ) -> Result<(), JsValue> {
            let dpr = dpr.clamp(1.0, 3.0);
            let physical_width = (width.max(256) as f32 * dpr).round() as u32;
            let physical_height = (height.max(256) as f32 * dpr).round() as u32;
            renderer.resize(physical_width, physical_height, dpr).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            self.builder.clear();
            for index in 0..64 {
                let column = index % 8;
                let row = index / 8;
                let radii = match index % 8 {
                    0 => [-4.0, 0.0, 8.0, 40.0],
                    1 => [40.0, 8.0, 0.0, -4.0],
                    2 => [0.0; 4],
                    3 => [0.25, 3.0, 12.0, 40.0],
                    4 => [12.0; 4],
                    5 => [64.0; 4],
                    6 => [2.0, 6.0, 12.0, 18.0],
                    _ => [22.0, 0.5, 22.0, 0.5],
                };
                let width = if index & 1 == 0 { 25.0 } else { 15.0 };
                let height = if index & 2 == 0 { 25.0 } else { 19.0 };
                self.builder.rrect(
                    gfx::RectF::new(column as f32 * 32.0 + 3.0, row as f32 * 32.0 + 3.0, width, height),
                    radii,
                    gfx::Color::rgba(0.18, 0.62, 0.94, 0.92),
                );
            }
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn image_capture_frame(
            &mut self,
            renderer: &mut BrowserRenderer,
            width: u32,
            height: u32,
            dpr: f32,
        ) -> Result<(), JsValue> {
            let dpr = dpr.clamp(1.0, 3.0);
            let physical_width = (width.max(256) as f32 * dpr).round() as u32;
            let physical_height = (height.max(256) as f32 * dpr).round() as u32;
            renderer.resize(physical_width, physical_height, dpr).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            self.builder.clear();
            self.builder.clip_push(gfx::RectI::new(4, 4, 248, 248));
            for index in 0..64
            {
                let column = index % 8;
                let row = index / 8;
                let src = match index % 4
                {
                    0 => gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                    1 => gfx::RectF::new(32.0, 0.0, 192.0, 256.0),
                    2 => gfx::RectF::new(0.0, 40.0, 256.0, 176.0),
                    _ => gfx::RectF::new(48.0, 24.0, 160.0, 208.0),
                };
                self.builder.image(
                    if (index / 4) & 1 == 0 { self.image } else { self.image_alt },
                    gfx::RectF::new(
                        column as f32 * 32.0 + 2.25,
                        row as f32 * 32.0 + 2.5,
                        if index & 1 == 0 { 29.5 } else { 23.25 },
                        if index & 2 == 0 { 28.75 } else { 24.5 },
                    ),
                    src,
                    0.55 + (index % 4) as f32 * 0.15,
                );
            }
            self.builder.clip_pop();
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn nine_slice_capture_frame(
            &mut self,
            renderer: &mut BrowserRenderer,
            width: u32,
            height: u32,
            dpr: f32,
        ) -> Result<(), JsValue> {
            let dpr = dpr.clamp(1.0, 3.0);
            let physical_width = (width.max(256) as f32 * dpr).round() as u32;
            let physical_height = (height.max(256) as f32 * dpr).round() as u32;
            renderer.resize(physical_width, physical_height, dpr).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            self.builder.clear();
            self.builder.clip_push(gfx::RectI::new(4, 4, 248, 248));
            for index in 0..64
            {
                let column = index % 8;
                let row = index / 8;
                let (width, height, slice) = match index % 8
                {
                    0 => (29.5, 28.75, gfx::Insets::new(4.0, 5.0, 6.0, 7.0)),
                    1 => (5.25, 9.5, gfx::Insets::new(8.0, 7.0, 9.0, 6.0)),
                    2 => (17.75, 4.25, gfx::Insets::new(0.5, 6.25, 3.75, 7.5)),
                    3 => (3.25, 3.75, gfx::Insets::new(300.0, 300.0, 300.0, 300.0)),
                    4 => (23.5, 19.25, gfx::Insets::new(-4.0, -2.0, 12.0, 14.0)),
                    5 => (7.5, 21.75, gfx::Insets::new(128.0, 1.0, 128.0, 1.0)),
                    6 => (31.25, 6.5, gfx::Insets::new(1.0, 128.0, 1.0, 128.0)),
                    _ => (11.5, 15.25, gfx::Insets::new(64.5, 32.25, 191.5, 223.75)),
                };
                self.builder.nine_slice(
                    if row & 1 == 0 { self.image } else { self.image_alt },
                    gfx::RectF::new(
                        column as f32 * 32.0 + 2.25,
                        row as f32 * 32.0 + 2.5,
                        width,
                        height,
                    ),
                    slice,
                    0.55 + (index % 4) as f32 * 0.15,
                );
            }
            self.builder.clip_pop();
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn spinner_capture_frame(
            &mut self,
            renderer: &mut BrowserRenderer,
            phase_ms: f64,
            reference: bool,
            width: u32,
            height: u32,
            dpr: f32,
        ) -> Result<(), JsValue> {
            let dpr = dpr.clamp(1.0, 3.0);
            let physical_width = (width.max(256) as f32 * dpr).round() as u32;
            let physical_height = (height.max(256) as f32 * dpr).round() as u32;
            renderer.resize(physical_width, physical_height, dpr).map_err(render_err)?;
            renderer.set_animation_time_ms(phase_ms);
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            self.builder.clear();
            self.builder.clip_push(gfx::RectI::new(4, 4, 248, 248));
            let phase = (phase_ms.rem_euclid(1_000.0) / 1_000.0) as f32;
            for index in 0..64
            {
                let center = [
                    (index % 8) as f32 * 32.0 + 16.0,
                    (index / 8) as f32 * 32.0 + 16.0,
                ];
                let atom = 5.0 + (index % 4) as f32 * 0.75;
                let alpha = 0.55 + (index % 4) as f32 * 0.15;
                if reference
                {
                    let radius = (atom * 1.5).max(1.0);
                    for atom_index in 0..12
                    {
                        let progress = (atom_index as f32 / 12.0 + phase).fract();
                        let angle = atom_index as f32 / 12.0 * core::f32::consts::TAU;
                        let x = center[0] + angle.cos() * radius;
                        let y = center[1] + angle.sin() * radius;
                        let dot_radius = atom * 0.12;
                        self.builder.rrect(
                            gfx::RectF::new(
                                x - dot_radius,
                                y - dot_radius,
                                dot_radius * 2.0,
                                dot_radius * 2.0,
                            ),
                            [dot_radius; 4],
                            gfx::Color::rgba(
                                0.15,
                                0.15,
                                0.15,
                                alpha * (0.25 + progress * 0.75),
                            ),
                        );
                    }
                }
                else
                {
                    self.builder.spinner(center, atom, alpha);
                }
            }
            self.builder.clip_pop();
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
        }

        fn neon_marker_capture_frame(
            &mut self,
            renderer: &mut BrowserRenderer,
            width: u32,
            height: u32,
            dpr: f32,
        ) -> Result<(), JsValue> {
            let dpr = dpr.clamp(1.0, 3.0);
            let physical_width = (width.max(256) as f32 * dpr).round() as u32;
            let physical_height = (height.max(256) as f32 * dpr).round() as u32;
            renderer.resize(physical_width, physical_height, dpr).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            renderer
                .encode_neon_markers(&neon_marker::NeonMarkerPass {
                    viewport: gfx::RectF::new(0.0, 0.0, width.max(256) as f32, height.max(256) as f32),
                    markers: &self.neon_markers[..WEBGPU_NEON_MARKERS],
                })
                .map_err(render_err)?;
            renderer.submit(token).map_err(render_err)
        }

        fn draw_state_cache_frame(
            &mut self,
            renderer: &mut BrowserRenderer,
        ) -> Result<(), JsValue> {
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

    #[derive(Clone, Copy)]
    enum WebGpuScene3dStressMode {
        Compatible,
        Mixed,
        Transparent,
        Subviewport,
    }

    impl WebGpuScene3dStressMode {
        fn from_u32(value: u32) -> Option<Self> {
            match value {
                0 => Some(Self::Compatible),
                1 => Some(Self::Mixed),
                2 => Some(Self::Transparent),
                3 => Some(Self::Subviewport),
                _ => None,
            }
        }

        fn name(self) -> &'static str {
            match self {
                Self::Compatible => "compatible",
                Self::Mixed => "mixed",
                Self::Transparent => "transparent",
                Self::Subviewport => "subviewport",
            }
        }

        fn viewport(self) -> Option<gfx::RectF> {
            matches!(self, Self::Subviewport)
                .then(|| gfx::RectF::new(56.0, 40.0, 144.0, 168.0))
        }
    }

    struct WebGpuScene3dStressBenchResources {
        back: scene3d::MeshHandle3d,
        front: scene3d::MeshHandle3d,
        instances: Vec<scene3d::Instance3d>,
        instance_count: usize,
        mode: WebGpuScene3dStressMode,
    }

    impl WebGpuScene3dStressBenchResources {
        fn new(
            renderer: &mut BrowserRenderer,
            instance_count: usize,
            mode: WebGpuScene3dStressMode,
        ) -> Result<Self, JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let back = webgpu_scene3d_create_back_mesh(renderer)?;
            let front = webgpu_scene3d_create_front_mesh(renderer)?;
            let mut instances = Vec::with_capacity(instance_count);
            webgpu_scene3d_fill_stress_instances(
                &mut instances,
                back,
                front,
                instance_count,
                mode,
            );
            Ok(Self { back, front, instances, instance_count, mode })
        }

        fn frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            webgpu_scene3d_fill_stress_instances(
                &mut self.instances,
                self.back,
                self.front,
                self.instance_count,
                self.mode,
            );
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            webgpu_scene3d_encode_instances_in_viewport(
                renderer,
                &self.instances,
                self.mode.viewport(),
            )
                .and_then(|_| renderer.submit(token).map_err(render_err))
        }
    }

    struct WebGpuScene3dStressRecreateResources {
        instances: Vec<scene3d::Instance3d>,
        instance_count: usize,
        mode: WebGpuScene3dStressMode,
    }

    impl WebGpuScene3dStressRecreateResources {
        fn new(instance_count: usize, mode: WebGpuScene3dStressMode) -> Self {
            Self { instances: Vec::with_capacity(instance_count), instance_count, mode }
        }

        fn frame(&mut self, renderer: &mut BrowserRenderer) -> Result<(), JsValue> {
            renderer.resize(512, 512, 2.0).map_err(render_err)?;
            let token = renderer.begin_frame(&gfx::FrameTarget, None);
            let back = webgpu_scene3d_create_back_mesh(renderer)?;
            let front = webgpu_scene3d_create_front_mesh(renderer)?;
            webgpu_scene3d_fill_stress_instances(
                &mut self.instances,
                back,
                front,
                self.instance_count,
                self.mode,
            );
            let result = webgpu_scene3d_encode_instances_in_viewport(
                renderer,
                &self.instances,
                self.mode.viewport(),
            )
                .and_then(|_| renderer.submit(token).map_err(render_err));
            renderer.mesh3d_release(back);
            renderer.mesh3d_release(front);
            result
        }
    }

    fn webgpu_local_layer_card_snapshots() -> Result<(gfx::RenderSnapshot, gfx::RenderSnapshot, gfx::RenderSnapshot), JsValue>
    {
        webgpu_local_layer_card_snapshots_with_id_base(30_000)
    }

    fn webgpu_local_layer_card_snapshots_with_id_base(id_base: u32) -> Result<(gfx::RenderSnapshot, gfx::RenderSnapshot, gfx::RenderSnapshot), JsValue>
    {
        let mut instances = Vec::with_capacity(WEBGPU_LOCAL_LAYER_CARDS);
        for index in 0..WEBGPU_LOCAL_LAYER_CARDS
        {
            let column = index % WEBGPU_LOCAL_LAYER_COLUMNS;
            let row = index / WEBGPU_LOCAL_LAYER_COLUMNS;
            let x = WEBGPU_LOCAL_LAYER_GAP
                + column as f32 * (WEBGPU_LOCAL_LAYER_WIDTH + WEBGPU_LOCAL_LAYER_GAP);
            let y = WEBGPU_LOCAL_LAYER_GAP
                + row as f32 * (WEBGPU_LOCAL_LAYER_HEIGHT + WEBGPU_LOCAL_LAYER_GAP);
            let rect = gfx::RectF::new(x, y, WEBGPU_LOCAL_LAYER_WIDTH, WEBGPU_LOCAL_LAYER_HEIGHT);
            let list = gfx::DrawList {
                items: vec![
                    gfx::DrawCmd::ClipPush {
                        rect: gfx::RectI::new(x as i32, y as i32, WEBGPU_LOCAL_LAYER_WIDTH as i32, WEBGPU_LOCAL_LAYER_HEIGHT as i32),
                    },
                    gfx::DrawCmd::RRect {
                        rect,
                        radii: [8.0; 4],
                        color: gfx::Color::rgba(0.05, 0.10 + row as f32 * 0.012, 0.18, 0.94),
                    },
                    gfx::DrawCmd::RRect {
                        rect: gfx::RectF::new(x + 6.0, y + 7.0, 44.0, 7.0),
                        radii: [3.5; 4],
                        color: gfx::Color::rgba(0.18, 0.50, 0.96, 0.86),
                    },
                    gfx::DrawCmd::RRect {
                        rect: gfx::RectF::new(x + 6.0, y + 22.0, 58.0, 10.0),
                        radii: [5.0; 4],
                        color: gfx::Color::rgba(0.90, 0.42, 0.12, 0.72),
                    },
                    gfx::DrawCmd::ClipPop,
                ],
                vertices: Vec::new(),
                indices: Vec::new(),
            };
            let chunk = gfx::RenderChunk::new(
                gfx::RenderChunkId(u64::from(id_base) + index as u64),
                gfx::RenderChunkRevisions {
                    structural: 1,
                    geometry: 1,
                    ..gfx::RenderChunkRevisions::default()
                },
                list,
                gfx::ChunkIndexMode::Local,
                &[],
            ).map_err(|error| JsValue::from_str(&format!("WebGPU C30 card chunk: {error}")))?;
            let mut instance = gfx::RenderChunkInstance::new(chunk, [0.0, 0.0]);
            instance.layer = Some(gfx::RenderLayerInstance {
                id: id_base + index as u32,
                rect,
                dirty: false,
            });
            instances.push(instance);
        }
        let mut populate_instances = instances.clone();
        for instance in &mut populate_instances
        {
            if let Some(layer) = instance.layer.as_mut()
            {
                layer.dirty = true;
            }
        }
        let mut dirty_instances = instances.clone();
        if let Some(layer) = dirty_instances.first_mut().and_then(|instance| instance.layer.as_mut())
        {
            layer.dirty = true;
        }
        let snapshot = |instances| gfx::RenderSnapshot::new(
            instances,
            Vec::new(),
            gfx::Damage { rects: Vec::new() },
        ).map_err(|error| JsValue::from_str(&format!("WebGPU C30 card snapshot: {error}")));
        Ok((
            snapshot(populate_instances)?,
            snapshot(instances)?,
            snapshot(dirty_instances)?,
        ))
    }

    fn webgpu_local_layer_clock_warmup() -> gfx::DrawList
    {
        let mut items = Vec::with_capacity(WEBGPU_LOCAL_LAYER_CLOCK_WARMUP_DRAWS);
        for index in 0..WEBGPU_LOCAL_LAYER_CLOCK_WARMUP_DRAWS
        {
            let phase = index as f32 / WEBGPU_LOCAL_LAYER_CLOCK_WARMUP_DRAWS as f32;
            items.push(gfx::DrawCmd::RRect {
                rect: gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
                radii: [16.0 + phase * 8.0; 4],
                color: gfx::Color::rgba(0.08 + phase * 0.12, 0.16, 0.32 - phase * 0.12, 1.0),
            });
        }
        gfx::DrawList { items, vertices: Vec::new(), indices: Vec::new() }
    }

    fn webgpu_local_layer_clock_warmup_frame(renderer: &Rc<RefCell<BrowserRenderer>>, list: &gfx::DrawList) -> Result<(), JsValue>
    {
        let mut renderer = renderer.borrow_mut();
        let token = renderer.begin_frame(&gfx::FrameTarget, None);
        renderer.encode_pass(list);
        renderer.submit(token).map_err(render_err)
    }

    fn webgpu_local_layer_edge_snapshots() -> Result<(gfx::RenderSnapshot, gfx::RenderSnapshot), JsValue>
    {
        let outer = gfx::RectF::new(-12.25, 14.5, 218.5, 204.25);
        let list = gfx::DrawList {
            items: vec![
                gfx::DrawCmd::ClipPush { rect: gfx::RectI::new(-8, 18, 206, 190) },
                gfx::DrawCmd::RRect {
                    rect: outer,
                    radii: [19.5, 8.25, 24.75, 13.5],
                    color: gfx::Color::rgba(0.04, 0.08, 0.15, 0.96),
                },
                gfx::DrawCmd::RRect {
                    rect: gfx::RectF::new(-2.5, 25.25, 194.75, 164.5),
                    radii: [14.25; 4],
                    color: gfx::Color::rgba(0.08, 0.30, 0.70, 0.84),
                },
                gfx::DrawCmd::LayerBegin {
                    id: 30_901,
                    rect: gfx::RectF::new(34.75, 44.5, 96.25, 72.5),
                    dirty: true,
                },
                gfx::DrawCmd::RRect {
                    rect: gfx::RectF::new(34.75, 44.5, 96.25, 72.5),
                    radii: [17.25; 4],
                    color: gfx::Color::rgba(0.88, 0.28, 0.08, 0.82),
                },
                gfx::DrawCmd::RRect {
                    rect: gfx::RectF::new(45.25, 55.75, 74.5, 49.25),
                    radii: [11.5; 4],
                    color: gfx::Color::rgba(0.98, 0.78, 0.22, 0.76),
                },
                gfx::DrawCmd::LayerEnd,
                gfx::DrawCmd::Backdrop {
                    rect: gfx::RectF::new(118.25, 128.5, 70.5, 48.25),
                    sigma: 6.0,
                    tint: gfx::Color::rgba(0.02, 0.04, 0.10, 0.38),
                    alpha: 1.0,
                },
                gfx::DrawCmd::RRect {
                    rect: gfx::RectF::new(122.5, 133.25, 62.0, 38.75),
                    radii: [12.25; 4],
                    color: gfx::Color::rgba(0.92, 0.96, 1.0, 0.28),
                },
                gfx::DrawCmd::RRect {
                    rect: gfx::RectF::new(4.25, 196.0, 46.5, 8.25),
                    radii: [4.0; 4],
                    color: gfx::Color::rgba(0.98, 0.99, 1.0, 1.0),
                },
                gfx::DrawCmd::ClipPop,
            ],
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        let chunk = gfx::RenderChunk::new(
            gfx::RenderChunkId(30_900),
            gfx::RenderChunkRevisions {
                structural: 1,
                geometry: 1,
                ..gfx::RenderChunkRevisions::default()
            },
            list,
            gfx::ChunkIndexMode::Local,
            &[],
        ).map_err(|error| JsValue::from_str(&format!("WebGPU C30 edge chunk: {error}")))?;
        let make = |dirty| {
            let mut instance = gfx::RenderChunkInstance::new(chunk.clone(), [0.375, -0.25]);
            instance.layer = Some(gfx::RenderLayerInstance { id: 30_900, rect: outer, dirty });
            gfx::RenderSnapshot::new(
                vec![instance],
                Vec::new(),
                gfx::Damage { rects: Vec::new() },
            ).map_err(|error| JsValue::from_str(&format!("WebGPU C30 edge snapshot: {error}")))
        };
        Ok((make(true)?, make(false)?))
    }

    fn webgpu_local_layer_resource_snapshot(image: gfx::ImageHandle, generation: u64) -> Result<gfx::RenderSnapshot, JsValue>
    {
        let rect = gfx::RectF::new(8.0, 8.0, 32.0, 32.0);
        let list = gfx::DrawList {
            items: vec![gfx::DrawCmd::Image {
                tex: image,
                dst: rect,
                src: gfx::RectF::new(0.0, 0.0, 2.0, 2.0),
                alpha: 1.0,
            }],
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        let dependencies = [gfx::RenderResourceDependency { image, generation }];
        let chunk = gfx::RenderChunk::new(
            gfx::RenderChunkId(30_950),
            gfx::RenderChunkRevisions {
                resource: generation,
                ..gfx::RenderChunkRevisions::default()
            },
            list,
            gfx::ChunkIndexMode::Local,
            &dependencies,
        ).map_err(|error| JsValue::from_str(&format!("WebGPU C30 resource chunk: {error}")))?;
        let mut instance = gfx::RenderChunkInstance::new(chunk, [0.0, 0.0]);
        instance.layer = Some(gfx::RenderLayerInstance { id: 30_950, rect, dirty: false });
        gfx::RenderSnapshot::new(
            vec![instance],
            Vec::new(),
            gfx::Damage { rects: Vec::new() },
        ).map_err(|error| JsValue::from_str(&format!("WebGPU C30 resource snapshot: {error}")))
    }

    fn webgpu_local_layer_frame(renderer: &Rc<RefCell<BrowserRenderer>>, snapshot: &gfx::RenderSnapshot) -> Result<(), JsValue>
    {
        let mut renderer = renderer.borrow_mut();
        let token = renderer.begin_frame(&gfx::FrameTarget, Some(snapshot.damage()));
        renderer.encode_snapshot(snapshot)
            .map_err(|error| JsValue::from_str(&format!("encoding WebGPU C30 snapshot: {error}")))?;
        renderer.submit(token).map_err(render_err)
    }

    fn webgpu_prepared_snapshot(
        image: gfx::ImageHandle,
        atlas: gfx::ImageHandle,
        dirty_revision: u64,
    ) -> Result<gfx::RenderSnapshot, JsValue> {
        let mut instances = Vec::with_capacity(WEBGPU_PREPARED_CHUNKS);
        for chunk_index in 0..WEBGPU_PREPARED_CHUNKS {
            let geometry = if chunk_index == 0 { dirty_revision } else { 1 };
            let chunk = webgpu_prepared_chunk(image, atlas, chunk_index, geometry)?;
            instances.push(gfx::RenderChunkInstance::new(chunk, [0.0, 0.0]));
        }
        gfx::RenderSnapshot::new(
            instances,
            Vec::new(),
            gfx::Damage { rects: Vec::new() },
        ).map_err(|error| JsValue::from_str(&format!("WebGPU prepared snapshot: {error}")))
    }

    fn webgpu_dynamic_property_instances(
        image: gfx::ImageHandle,
        atlas: gfx::ImageHandle,
    ) -> Result<Vec<gfx::RenderChunkInstance>, JsValue> {
        let image_chunk = gfx::RenderChunk::new(
            gfx::RenderChunkId(26_000),
            gfx::RenderChunkRevisions { resource: 1, ..gfx::RenderChunkRevisions::default() },
            gfx::DrawList {
                items: vec![gfx::DrawCmd::Image {
                    tex: image,
                    dst: gfx::RectF::new(0.0, 0.0, 28.0, 28.0),
                    src: gfx::RectF::new(0.0, 0.0, 2.0, 2.0),
                    alpha: 1.0,
                }],
                vertices: Vec::new(),
                indices: Vec::new(),
            },
            gfx::ChunkIndexMode::Local,
            &[gfx::RenderResourceDependency { image, generation: 1 }],
        ).map_err(|error| JsValue::from_str(&format!("WebGPU dynamic image chunk: {error}")))?;
        let text_chunk = gfx::RenderChunk::new(
            gfx::RenderChunkId(26_001),
            gfx::RenderChunkRevisions { resource: 1, ..gfx::RenderChunkRevisions::default() },
            gfx::DrawList {
                items: vec![gfx::DrawCmd::GlyphRun { run: gfx::GlyphRun {
                    atlas,
                    atlas_revision: 1,
                    vb: gfx::VertexSpan { offset: 0, len: 4 },
                    ib: gfx::IndexSpan { offset: 0, len: 6 },
                    sdf: false,
                    color: gfx::Color::rgba(0.92, 0.94, 1.0, 1.0),
                }}],
                vertices: vec![
                    gfx::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0 },
                    gfx::Vertex { x: 28.0, y: 0.0, u: 1.0, v: 0.0, rgba: 0 },
                    gfx::Vertex { x: 28.0, y: 28.0, u: 1.0, v: 1.0, rgba: 0 },
                    gfx::Vertex { x: 0.0, y: 28.0, u: 0.0, v: 1.0, rgba: 0 },
                ],
                indices: vec![0, 1, 2, 0, 2, 3],
            },
            gfx::ChunkIndexMode::Local,
            &[gfx::RenderResourceDependency { image: atlas, generation: 1 }],
        ).map_err(|error| JsValue::from_str(&format!("WebGPU dynamic text chunk: {error}")))?;
        Ok((0..WEBGPU_DYNAMIC_PROPERTY_NODES).map(|index| {
            let chunk = if index < 200 { text_chunk.clone() } else { image_chunk.clone() };
            let mut instance = gfx::RenderChunkInstance::new(chunk, [
                (index % 20) as f32 * 40.0 + 24.0,
                (index / 20) as f32 * 40.0 + 24.0,
            ]);
            instance.property_slots = vec![
                gfx::RenderPropertySlotId((index * 2 + 1) as u32),
                gfx::RenderPropertySlotId((index * 2 + 2) as u32),
            ].into();
            instance
        }).collect())
    }

    fn webgpu_dynamic_property_snapshot(
        instances: &[gfx::RenderChunkInstance],
        phase: u64,
        full_affine: bool,
    ) -> Result<gfx::RenderSnapshot, JsValue> {
        let mut properties = Vec::with_capacity(instances.len().saturating_mul(2));
        for index in 0..instances.len() {
            let transform = if full_affine {
                let angle = if phase & 1 == 0 { -0.035 } else { 0.035 };
                let scale = if phase & 1 == 0 { 0.96 } else { 1.04 };
                let (sin, cos) = f32::sin_cos(angle + (index % 7) as f32 * 0.001);
                [
                    cos * scale,
                    sin * scale,
                    -sin * scale,
                    cos * scale,
                    if phase & 1 == 0 { -1.25 } else { 1.25 },
                    if phase & 1 == 0 { 0.75 } else { -0.75 },
                ]
            } else {
                [
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                    if phase & 1 == 0 { -1.25 } else { 1.25 },
                    if phase & 1 == 0 { 0.75 } else { -0.75 },
                ]
            };
            properties.push(gfx::RenderPropertySlot {
                id: gfx::RenderPropertySlotId((index * 2 + 1) as u32),
                revision: phase.saturating_add(1),
                value: gfx::RenderPropertyValue::Transform(transform),
            });
            properties.push(gfx::RenderPropertySlot {
                id: gfx::RenderPropertySlotId((index * 2 + 2) as u32),
                revision: phase.saturating_add(1),
                value: gfx::RenderPropertyValue::Opacity(if phase & 1 == 0 { 0.72 } else { 0.96 }),
            });
        }
        gfx::RenderSnapshot::new(
            instances.to_vec(),
            properties,
            gfx::Damage { rects: Vec::new() },
        ).map_err(|error| JsValue::from_str(&format!("WebGPU dynamic property snapshot: {error}")))
    }

    fn webgpu_prepared_single_snapshot(
        image: gfx::ImageHandle,
        atlas: gfx::ImageHandle,
        chunk_index: usize,
        geometry_revision: u64,
    ) -> Result<gfx::RenderSnapshot, JsValue> {
        let chunk = webgpu_prepared_chunk(image, atlas, chunk_index, geometry_revision)?;
        gfx::RenderSnapshot::new(
            vec![gfx::RenderChunkInstance::new(chunk, [0.0, 0.0])],
            Vec::new(),
            gfx::Damage { rects: Vec::new() },
        ).map_err(|error| JsValue::from_str(&format!("WebGPU prepared snapshot: {error}")))
    }

    fn webgpu_prepared_structural_snapshot(
        image: gfx::ImageHandle,
        atlas: gfx::ImageHandle,
    ) -> Result<gfx::RenderSnapshot, JsValue> {
        let mut instances = Vec::with_capacity(WEBGPU_PREPARED_CHUNKS);
        for chunk_index in 0..WEBGPU_PREPARED_CHUNKS {
            let chunk = if chunk_index == 0 {
                webgpu_prepared_chunk_with_shape(image, atlas, chunk_index, 1, 1, 9)?
            } else {
                webgpu_prepared_chunk(image, atlas, chunk_index, 1)?
            };
            instances.push(gfx::RenderChunkInstance::new(chunk, [0.0, 0.0]));
        }
        gfx::RenderSnapshot::new(
            instances,
            Vec::new(),
            gfx::Damage { rects: Vec::new() },
        ).map_err(|error| JsValue::from_str(&format!("WebGPU prepared structural snapshot: {error}")))
    }

    fn webgpu_prepared_chunk(
        image: gfx::ImageHandle,
        atlas: gfx::ImageHandle,
        chunk_index: usize,
        geometry_revision: u64,
    ) -> Result<gfx::RenderChunk, JsValue> {
        webgpu_prepared_chunk_with_shape(
            image,
            atlas,
            chunk_index,
            geometry_revision,
            0,
            WEBGPU_PREPARED_DRAW_COUNTS[chunk_index & 3],
        )
    }

    fn webgpu_prepared_chunk_with_shape(
        image: gfx::ImageHandle,
        atlas: gfx::ImageHandle,
        chunk_index: usize,
        geometry_revision: u64,
        structural_revision: u64,
        draw_count: usize,
    ) -> Result<gfx::RenderChunk, JsValue> {
        let column = chunk_index % 16;
        let row = chunk_index / 16;
        let base_x = column as f32 * 72.0 + 8.0;
        let base_y = row as f32 * 46.0 + 8.0;
        let mut list = gfx::DrawList::default();
        for draw_index in 0..draw_count {
            let x = base_x + draw_index as f32 * 0.75;
            match draw_index & 3 {
                0 => list.items.push(gfx::DrawCmd::Image {
                    tex: image,
                    dst: gfx::RectF::new(x, base_y, 0.625, 28.0),
                    src: gfx::RectF::new(0.0, 0.0, 2.0, 2.0),
                    alpha: 1.0,
                }),
                1 | 2 => {
                    let vertex_offset = list.vertices.len() as u32;
                    let index_offset = list.indices.len() as u32;
                    list.vertices.extend_from_slice(&[
                        gfx::Vertex { x, y: base_y, u: 0.0, v: 0.0, rgba: 0 },
                        gfx::Vertex { x: x + 0.625, y: base_y, u: 1.0, v: 0.0, rgba: 0 },
                        gfx::Vertex { x: x + 0.625, y: base_y + 28.0, u: 1.0, v: 1.0, rgba: 0 },
                        gfx::Vertex { x, y: base_y + 28.0, u: 0.0, v: 1.0, rgba: 0 },
                    ]);
                    list.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
                    list.items.push(gfx::DrawCmd::GlyphRun { run: gfx::GlyphRun {
                        atlas,
                        atlas_revision: 1,
                        vb: gfx::VertexSpan { offset: vertex_offset, len: 4 },
                        ib: gfx::IndexSpan { offset: index_offset, len: 6 },
                        sdf: draw_index & 3 == 2,
                        color: gfx::Color::rgba(0.9, 0.9, 1.0, 1.0),
                    }});
                }
                _ => {
                    let vertex_offset = list.vertices.len() as u32;
                    let index_offset = list.indices.len() as u32;
                    list.vertices.extend_from_slice(&[
                        gfx::Vertex { x, y: base_y + 28.0, u: 0.0, v: 0.0, rgba: 0 },
                        gfx::Vertex { x: x + 0.3125, y: base_y, u: 0.0, v: 0.0, rgba: 0 },
                        gfx::Vertex { x: x + 0.625, y: base_y + 28.0, u: 0.0, v: 0.0, rgba: 0 },
                    ]);
                    list.indices.extend_from_slice(&[0, 1, 2]);
                    list.items.push(gfx::DrawCmd::Solid {
                        vb: gfx::VertexSpan { offset: vertex_offset, len: 3 },
                        ib: gfx::IndexSpan { offset: index_offset, len: 3 },
                        color: gfx::Color::rgba(0.9, 0.55, 0.1, 1.0),
                    });
                }
            }
        }
        let dependencies = [
            gfx::RenderResourceDependency { image, generation: 1 },
            gfx::RenderResourceDependency { image: atlas, generation: 1 },
        ];
        gfx::RenderChunk::new(
            gfx::RenderChunkId(25_000 + chunk_index as u64),
            gfx::RenderChunkRevisions {
                structural: structural_revision,
                geometry: geometry_revision,
                resource: 1,
                ..gfx::RenderChunkRevisions::default()
            },
            list,
            gfx::ChunkIndexMode::Local,
            &dependencies,
        ).map_err(|error| JsValue::from_str(&format!("WebGPU prepared chunk: {error}")))
    }

    fn webgpu_prepared_resource_snapshot(
        image: gfx::ImageHandle,
        id: u64,
        resource_revision: u64,
    ) -> Result<gfx::RenderSnapshot, JsValue> {
        let mut list = gfx::DrawList::default();
        list.items.push(gfx::DrawCmd::Image {
            tex: image,
            dst: gfx::RectF::new(8.0, 8.0, 32.0, 32.0),
            src: gfx::RectF::new(0.0, 0.0, 2.0, 2.0),
            alpha: 1.0,
        });
        let dependency = [gfx::RenderResourceDependency { image, generation: resource_revision }];
        let chunk = gfx::RenderChunk::new(
            gfx::RenderChunkId(id),
            gfx::RenderChunkRevisions {
                resource: resource_revision,
                ..gfx::RenderChunkRevisions::default()
            },
            list,
            gfx::ChunkIndexMode::Local,
            &dependency,
        ).map_err(|error| JsValue::from_str(&format!("WebGPU prepared resource chunk: {error}")))?;
        gfx::RenderSnapshot::new(
            vec![gfx::RenderChunkInstance::new(chunk, [0.0, 0.0])],
            Vec::new(),
            gfx::Damage { rects: Vec::new() },
        ).map_err(|error| JsValue::from_str(&format!("WebGPU prepared resource snapshot: {error}")))
    }

    fn webgpu_prepared_segmented_snapshot(
        image: gfx::ImageHandle,
        atlas: gfx::ImageHandle,
    ) -> Result<gfx::RenderSnapshot, JsValue> {
        let source = webgpu_prepared_chunk(image, atlas, 3, 1)?;
        let mut list = source.draw_list().clone();
        list.items.insert(8, gfx::DrawCmd::ClipPush {
            rect: gfx::RectI::new(150, 0, 80, 800),
        });
        list.items.insert(17, gfx::DrawCmd::ClipPop);
        let chunk = gfx::RenderChunk::new(
            gfx::RenderChunkId(25_950),
            gfx::RenderChunkRevisions::default(),
            list,
            gfx::ChunkIndexMode::Local,
            source.resource_dependencies(),
        ).map_err(|error| JsValue::from_str(&format!("WebGPU segmented prepared chunk: {error}")))?;
        gfx::RenderSnapshot::new(
            vec![gfx::RenderChunkInstance::new(chunk, [0.0, 0.0])],
            Vec::new(),
            gfx::Damage { rects: Vec::new() },
        ).map_err(|error| JsValue::from_str(&format!("WebGPU segmented prepared snapshot: {error}")))
    }

    fn webgpu_prepared_effect_guard_snapshot() -> Result<gfx::RenderSnapshot, JsValue> {
        let mut list = gfx::DrawList::default();
        list.items.push(gfx::DrawCmd::RRect {
            rect: gfx::RectF::new(16.0, 16.0, 96.0, 48.0),
            radii: [8.0; 4],
            color: gfx::Color::rgba(0.2, 0.4, 0.8, 1.0),
        });
        list.items.push(gfx::DrawCmd::Backdrop {
            rect: gfx::RectF::new(24.0, 24.0, 64.0, 32.0),
            sigma: 6.0,
            tint: gfx::Color::rgba(0.1, 0.2, 0.3, 0.5),
            alpha: 1.0,
        });
        webgpu_prepared_guard_snapshot(25_960, list)
    }

    fn webgpu_prepared_layer_guard_snapshot() -> Result<gfx::RenderSnapshot, JsValue> {
        let mut list = gfx::DrawList::default();
        list.items.push(gfx::DrawCmd::LayerBegin {
            id: 25_961,
            rect: gfx::RectF::new(16.0, 16.0, 96.0, 48.0),
            dirty: true,
        });
        list.items.push(gfx::DrawCmd::RRect {
            rect: gfx::RectF::new(16.0, 16.0, 96.0, 48.0),
            radii: [8.0; 4],
            color: gfx::Color::rgba(0.2, 0.8, 0.4, 1.0),
        });
        list.items.push(gfx::DrawCmd::LayerEnd);
        webgpu_prepared_guard_snapshot(25_961, list)
    }

    fn webgpu_prepared_guard_snapshot(
        id: u64,
        list: gfx::DrawList,
    ) -> Result<gfx::RenderSnapshot, JsValue> {
        let chunk = gfx::RenderChunk::new(
            gfx::RenderChunkId(id),
            gfx::RenderChunkRevisions::default(),
            list,
            gfx::ChunkIndexMode::Local,
            &[],
        ).map_err(|error| JsValue::from_str(&format!("WebGPU prepared guard chunk: {error}")))?;
        gfx::RenderSnapshot::new(
            vec![gfx::RenderChunkInstance::new(chunk, [0.0, 0.0])],
            Vec::new(),
            gfx::Damage { rects: Vec::new() },
        ).map_err(|error| JsValue::from_str(&format!("WebGPU prepared guard snapshot: {error}")))
    }

    fn webgpu_prepared_frame(
        renderer: &Rc<RefCell<BrowserRenderer>>,
        snapshot: &gfx::RenderSnapshot,
        flat_control: bool,
        flat: &mut gfx::DrawList,
    ) -> Result<f64, JsValue> {
        let mut renderer = renderer.borrow_mut();
        let token = renderer.begin_frame(&gfx::FrameTarget, Some(snapshot.damage()));
        let encode_start = perf_now();
        if flat_control {
            flat.items.clear();
            flat.vertices.clear();
            flat.indices.clear();
            snapshot.flatten_into(flat)
                .map_err(|error| JsValue::from_str(&format!("flattening WebGPU snapshot: {error}")))?;
            renderer.encode_pass(flat);
        } else {
            renderer.encode_snapshot(snapshot)
                .map_err(|error| JsValue::from_str(&format!("encoding WebGPU snapshot: {error}")))?;
        }
        let encode_ms = (perf_now() - encode_start).max(0.0);
        renderer.submit(token).map_err(render_err)?;
        Ok(encode_ms)
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
        let mut warmup_values = [0.0_f64; 4];
        for warmup in 0..4 {
            let revision = if stable_revision { 1 } else { warmup as u64 + 1 };
            let warmup_start = perf_now();
            webgpu_id_mask_frame(renderer, &vertices, revision, timestamp)?;
            warmup_values[warmup as usize] = (perf_now() - warmup_start).max(0.0);
            timestamp += 16.666_667;
        }

        let mut values = Vec::with_capacity(sample_count as usize);
        let mut allocations = WebGpuAllocationSummary::default();
        let mut submit_allocations = WebGpuSubmitAllocationSummary::default();
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
                add_submit_allocation_frame(&mut submit_allocations, renderer.last_stats());
                timestamp += 16.666_667;
            }
            values.push(((perf_now() - sample_start).max(0.0)) / frames as f64);
        }
        values.sort_by(|a, b| a.total_cmp(b));
        let stats = renderer.last_stats();

        Ok(WebGpuIdMaskBenchSummary {
            warmup_ms: average(&warmup_values),
            p50_ms: percentile(&values, 0.50),
            p95_ms: percentile(&values, 0.95),
            p99_ms: percentile(&values, 0.99),
            peak_ms: values.last().copied().unwrap_or(0.0),
            avg_ms: average(&values),
            allocations,
            submit_allocations,
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
        let warmup_start = perf_now();
        for warmup in 0..4 {
            frame(renderer, warmup, timestamp)?;
            timestamp += 16.666_667;
        }
        let warmup_ms = ((perf_now() - warmup_start).max(0.0)) / 4.0;

        let mut values = Vec::with_capacity(sample_count as usize);
        let mut allocations = WebGpuAllocationSummary::default();
        let mut submit_allocations = WebGpuSubmitAllocationSummary::default();
        for sample in 0..sample_count {
            let sample_start = perf_now();
            for frame_index in 0..frames {
                let seq = sample.saturating_mul(frames).saturating_add(frame_index) as u64;
                let alloc_before = oxide_wasm_alloc_counter::snapshot();
                frame(renderer, seq + 64, timestamp)?;
                let alloc_after = oxide_wasm_alloc_counter::snapshot();
                add_allocation_frame(&mut allocations, alloc_before, alloc_after);
                add_submit_allocation_frame(&mut submit_allocations, renderer.last_stats());
                timestamp += 16.666_667;
            }
            values.push(((perf_now() - sample_start).max(0.0)) / frames as f64);
        }
        values.sort_by(|a, b| a.total_cmp(b));
        let stats = renderer.last_stats();

        Ok(WebGpuBenchSummary {
            warmup_ms,
            p50_ms: percentile(&values, 0.50),
            p95_ms: percentile(&values, 0.95),
            p99_ms: percentile(&values, 0.99),
            peak_ms: values.last().copied().unwrap_or(0.0),
            avg_ms: average(&values),
            allocations,
            submit_allocations,
            stats,
        })
    }

    fn c19_resize(
        renderer: &Rc<RefCell<BrowserRenderer>>,
        width: u32,
        height: u32,
    ) -> Result<(), JsValue> {
        renderer.borrow_mut().resize(width, height, 2.0).map_err(render_err)
    }

    fn c19_direct_frame(
        renderer: &Rc<RefCell<BrowserRenderer>>,
        builder: &mut ui::DrawListBuilder,
    ) -> Result<(), JsValue> {
        let mut renderer = renderer.borrow_mut();
        let token = renderer.begin_frame(&gfx::FrameTarget, None);
        builder.clear();
        builder.rrect(
            gfx::RectF::new(16.0, 16.0, 220.0, 220.0),
            [18.0; 4],
            gfx::Color::rgba(0.08, 0.16, 0.28, 1.0),
        );
        renderer.encode_pass(builder.drawlist());
        renderer.submit(token).map_err(render_err)
    }

    fn c19_backdrop_frame(
        renderer: &Rc<RefCell<BrowserRenderer>>,
        builder: &mut ui::DrawListBuilder,
    ) -> Result<(), JsValue> {
        let mut renderer = renderer.borrow_mut();
        let token = renderer.begin_frame(&gfx::FrameTarget, None);
        builder.clear();
        builder.rrect(
            gfx::RectF::new(0.0, 0.0, 256.0, 256.0),
            [0.0; 4],
            gfx::Color::rgba(0.04, 0.10, 0.18, 1.0),
        );
        builder.backdrop(
            gfx::RectF::new(32.0, 32.0, 192.0, 192.0),
            12.0,
            gfx::Color::rgba(0.10, 0.16, 0.24, 0.35),
            1.0,
        );
        renderer.encode_pass(builder.drawlist());
        renderer.submit(token).map_err(render_err)
    }

    fn c19_scene3d_frame(
        renderer: &Rc<RefCell<BrowserRenderer>>,
        back: scene3d::MeshHandle3d,
        front: scene3d::MeshHandle3d,
    ) -> Result<(), JsValue> {
        let mut renderer = renderer.borrow_mut();
        let token = renderer.begin_frame(&gfx::FrameTarget, None);
        webgpu_scene3d_encode_handles(&mut renderer, back, front)?;
        renderer.submit(token).map_err(render_err)
    }

    fn write_c19_samples(out: &mut String, name: &str, samples: &[f64]) {
        let _ = write!(out, ";{name}=");
        for (index, sample) in samples.iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            let _ = write!(out, "{sample:.6}");
        }
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
        out.push_str(&allocation_metrics(&summary.allocations, prefix));
        out.push_str(&submit_allocation_metrics(&summary.submit_allocations, prefix));
        out.push_str(&renderer_stats_metrics(summary.stats, prefix));
        out
    }

    fn glyph_upload_a8(size: u32, row_bytes: usize) -> Vec<u8> {
        let mut data = vec![0_u8; row_bytes.saturating_mul(size as usize)];
        for y in 0..size {
            for x in 0..size {
                let idx = (y as usize).saturating_mul(row_bytes).saturating_add(x as usize);
                let edge = x < 4 || y < 4 || x + 4 >= size || y + 4 >= size;
                data[idx] = if edge || ((x / 8 + y / 8) & 1) == 0 { 220 } else { 96 };
            }
        }
        data
    }

    fn webgpu_geometry_glyphs(atlas: gfx::ImageHandle) -> Result<gfx::DrawList, JsValue> {
        let mut builder = ui::DrawListBuilder::new();
        if !append_glyph_grid(
            &mut builder,
            atlas,
            WEBGPU_GEOMETRY_QUADS,
            1.0,
            1.0,
            2.0,
            false,
            gfx::Color::rgba(0.13, 0.71, 0.94, 0.88),
        ) {
            return Err(JsValue::from_str("failed to build C16 glyph geometry"));
        }
        Ok(builder.drawlist().clone())
    }

    fn webgpu_geometry_images(image: gfx::ImageHandle) -> gfx::DrawList {
        let mut list = gfx::DrawList {
            items: Vec::with_capacity(WEBGPU_GEOMETRY_QUADS),
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        for index in 0..WEBGPU_GEOMETRY_QUADS {
            let column = index % 100;
            let row = index / 100;
            list.items.push(gfx::DrawCmd::Image {
                tex: image,
                dst: gfx::RectF::new(column as f32 * 2.56, row as f32 * 2.56, 2.4, 2.4),
                src: gfx::RectF::new(0.0, 0.0, WEBGPU_UPLOAD_IMAGE_SIZE as f32, WEBGPU_UPLOAD_IMAGE_SIZE as f32),
                alpha: 0.82,
            });
        }
        list
    }

    fn webgpu_geometry_large_mesh() -> gfx::DrawList {
        let mut vertices = Vec::with_capacity(WEBGPU_GEOMETRY_LARGE_VERTICES);
        for triangle in 0..WEBGPU_GEOMETRY_LARGE_VERTICES / 3 {
            let column = triangle % 154;
            let row = triangle / 154;
            let x = column as f32 * (256.0 / 154.0);
            let y = row as f32 * (256.0 / 152.0);
            let rgba = 0xD040_80FF_u32.wrapping_add((triangle as u32 & 0x1F) << 8);
            vertices.extend_from_slice(&[
                gfx::Vertex { x, y, u: 0.0, v: 0.0, rgba },
                gfx::Vertex { x: x + 1.5, y, u: 1.0, v: 0.0, rgba },
                gfx::Vertex { x: x + 0.5, y: y + 1.5, u: 0.5, v: 1.0, rgba },
            ]);
        }
        gfx::DrawList {
            items: vec![gfx::DrawCmd::Solid {
                vb: gfx::VertexSpan { offset: 0, len: WEBGPU_GEOMETRY_LARGE_VERTICES as u32 },
                ib: gfx::IndexSpan { offset: 0, len: 0 },
                color: gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
            }],
            vertices,
            indices: Vec::new(),
        }
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
        instance_count: usize,
        mode: WebGpuScene3dStressMode,
    ) {
        out.clear();
        out.reserve(instance_count);
        let columns = (instance_count as f32).sqrt().ceil().max(1.0) as usize;
        let rows = instance_count.saturating_add(columns - 1) / columns;
        let step_x = 1.82 / columns as f32;
        let step_y = 1.64 / rows.max(1) as f32;
        let scale = scene3d::scale_xyz(step_x.min(step_y) * 0.72);
        for index in 0..instance_count {
            let col = index % columns;
            let row = index / columns;
            let x = -0.91 + (col as f32 + 0.5) * step_x;
            let y = -0.82 + (row as f32 + 0.5) * step_y;
            let translate = scene3d::clip_space_translate(x, y);
            let transform = scene3d::mat4_mul(&translate, &scale);
            let group = index.saturating_mul(12) / instance_count.max(1);
            let use_front = match mode {
                WebGpuScene3dStressMode::Compatible
                | WebGpuScene3dStressMode::Subviewport => index >= instance_count / 2,
                WebGpuScene3dStressMode::Mixed => group & 1 != 0,
                WebGpuScene3dStressMode::Transparent => false,
            };
            let mesh = if use_front { front } else { back };
            let alpha = if matches!(mode, WebGpuScene3dStressMode::Transparent) {
                0.52
            } else {
                1.0
            };
            let tint = if use_front {
                gfx::Color::rgba(1.0, 0.82, 0.44, alpha)
            } else {
                gfx::Color::rgba(0.70, 0.86, 1.0, alpha)
            };
            let mut instance = scene3d::Instance3d::new(mesh, transform, tint);
            match mode {
                WebGpuScene3dStressMode::Compatible
                | WebGpuScene3dStressMode::Subviewport => {
                    instance.cull = scene3d::CullMode3d::None;
                }
                WebGpuScene3dStressMode::Transparent => {
                    instance.cull = scene3d::CullMode3d::None;
                    instance.depth_write = false;
                }
                WebGpuScene3dStressMode::Mixed => {
                    instance.cull = match group % 3 {
                        0 => scene3d::CullMode3d::None,
                        1 => scene3d::CullMode3d::Front,
                        _ => scene3d::CullMode3d::Back,
                    };
                    instance.depth_test = group & 2 == 0;
                    instance.depth_write = group & 4 == 0;
                    instance.blend = if group & 3 == 3 {
                        scene3d::BlendMode3d::Additive
                    } else {
                        scene3d::BlendMode3d::Alpha
                    };
                    instance.material = match group % 3 {
                        0 => scene3d::Material3d::Flat,
                        1 => scene3d::Material3d::NeighborhoodFill,
                        _ => scene3d::Material3d::Emissive,
                    };
                    instance.params = [group as f32, 0.25, 0.5, 1.0];
                }
            }
            out.push(instance);
        }
    }

    fn webgpu_fill_neon_markers(
        out: &mut Vec<neon_marker::NeonMarker>,
        count: usize,
        columns: usize,
        origin_x: f32,
        origin_y: f32,
        step_x: f32,
        step_y: f32,
    ) {
        out.clear();
        out.reserve(count);
        for index in 0..count {
            let col = index % columns;
            let row = index / columns;
            let x = origin_x + col as f32 * step_x;
            let y = origin_y + row as f32 * step_y;
            let hue = index as f32 / count as f32;
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

    fn webgpu_scene3d_encode_instances_in_viewport(
        renderer: &mut BrowserRenderer,
        instances: &[scene3d::Instance3d],
        viewport: Option<gfx::RectF>,
    ) -> Result<(), JsValue> {
        let pass = scene3d::Pass3d {
            viewport,
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
        let chunks = [id_mask_compositor::IdMaskRasterChunk {
            content_hash: revision,
            first_vertex: 0,
            vertex_count: vertices.len(),
        }];
        let pass = webgpu_id_mask_pass(vertices, &chunks, revision);
        let token = renderer.begin_frame(&gfx::FrameTarget, None);
        renderer.encode_id_mask_gpu_compositor(&pass).map_err(render_err)?;
        renderer.submit(token).map_err(render_err)
    }

    fn webgpu_id_mask_configured_frame<F>(
        renderer: &mut BrowserRenderer,
        vertices: &[id_mask_compositor::IdMaskRasterVertex],
        revision: u64,
        configure: F,
    ) -> Result<(), JsValue>
    where
        F: FnOnce(&mut id_mask_compositor::IdMaskGpuCompositorPass<'_>),
    {
        renderer.resize(512, 512, 2.0).map_err(render_err)?;
        let chunks = [id_mask_compositor::IdMaskRasterChunk {
            content_hash: revision,
            first_vertex: 0,
            vertex_count: vertices.len(),
        }];
        let mut pass = webgpu_id_mask_pass(vertices, &chunks, revision);
        configure(&mut pass);
        let token = renderer.begin_frame(&gfx::FrameTarget, None);
        renderer.encode_id_mask_gpu_compositor(&pass).map_err(render_err)?;
        renderer.submit(token).map_err(render_err)
    }

    fn webgpu_id_mask_two_map_frame(
        renderer: &mut BrowserRenderer,
        vertices: &[id_mask_compositor::IdMaskRasterVertex],
    ) -> Result<(), JsValue> {
        renderer.resize(512, 512, 2.0).map_err(render_err)?;
        let chunks_a = [id_mask_compositor::IdMaskRasterChunk {
            content_hash: 0xC33_A,
            first_vertex: 0,
            vertex_count: vertices.len(),
        }];
        let chunks_b = [id_mask_compositor::IdMaskRasterChunk {
            content_hash: 0xC33_B,
            first_vertex: 0,
            vertex_count: vertices.len(),
        }];
        let pass_a = webgpu_id_mask_pass(vertices, &chunks_a, 0xC33_A);
        let mut pass_b = webgpu_id_mask_pass(vertices, &chunks_b, 0xC33_B);
        pass_b.raster.projection.world_to_clip[3][0] = 0.125;
        let token = renderer.begin_frame(&gfx::FrameTarget, None);
        renderer.encode_id_mask_gpu_compositor(&pass_a).map_err(render_err)?;
        renderer.encode_id_mask_gpu_compositor(&pass_b).map_err(render_err)?;
        renderer.submit(token).map_err(render_err)
    }

    fn id_mask_cache_case_metrics(name: &str, stats: WebRendererStats) -> String {
        format!(
            "{name}_hits={};{name}_misses={};{name}_raster={};{name}_seed={};{name}_jump={};{name}_compositor={};{name}_render_passes={};{name}_texture_creates={};{name}_resident_bytes={};{name}_budget_bytes={};{name}_entries={};{name}_evictions={};{name}_uniform_bytes={};{name}_uniform_slots={}",
            stats.id_mask_cache_hits,
            stats.id_mask_cache_misses,
            stats.id_mask_raster_passes,
            stats.id_mask_field_seed_passes,
            stats.id_mask_field_jump_passes,
            stats.id_mask_compositor_passes,
            stats.render_passes,
            stats.id_mask_texture_creates,
            stats.id_mask_cache_resident_bytes,
            stats.id_mask_cache_budget_bytes,
            stats.id_mask_cache_entries,
            stats.id_mask_cache_evictions,
            stats.id_mask_uniform_bytes,
            stats.id_mask_uniform_slots,
        )
    }

    async fn measure_webgpu_id_mask_multi_cache(
        renderer: &Rc<RefCell<BrowserRenderer>>,
        vertices: &[id_mask_compositor::IdMaskRasterVertex],
        budget_bytes: u64,
        samples: u32,
        frames: u32,
        name: &str,
    ) -> Result<String, JsValue> {
        {
            let mut renderer = renderer.borrow_mut();
            renderer.purge_id_mask_field_cache();
            renderer.set_id_mask_cache_budget_bytes(budget_bytes);
            renderer.set_timestamp_readback_interval_for_benchmark(1);
        }
        for _ in 0..4 {
            webgpu_id_mask_two_map_frame(&mut renderer.borrow_mut(), vertices)?;
        }
        wait_renderer_queue_idle(renderer).await?;
        {
            let mut renderer = renderer.borrow_mut();
            renderer.collect_timestamp_readbacks();
            renderer.clear_completed_timestamp_samples();
        }

        let mut cpu_values = Vec::with_capacity(samples as usize);
        let mut gpu_samples = Vec::with_capacity(samples.saturating_mul(frames) as usize);
        let mut drained = Vec::with_capacity(frames as usize);
        for _ in 0..samples {
            let start = perf_now();
            for _ in 0..frames {
                webgpu_id_mask_two_map_frame(&mut renderer.borrow_mut(), vertices)?;
            }
            cpu_values.push((perf_now() - start).max(0.0) / frames as f64);
            wait_renderer_queue_idle(renderer).await?;
            let mut renderer = renderer.borrow_mut();
            renderer.collect_timestamp_readbacks();
            renderer.drain_completed_timestamp_samples_into(&mut drained);
            gpu_samples.extend_from_slice(&drained);
        }
        let expected = samples.saturating_mul(frames) as usize;
        if gpu_samples.len() != expected {
            return Err(JsValue::from_str(&format!(
                "{name} expected {expected} GPU samples, observed {}",
                gpu_samples.len(),
            )));
        }
        cpu_values.sort_by(|a, b| a.total_cmp(b));
        let sorted_stage = |value: fn(&WebGpuTimestampSample) -> u64| {
            let mut values = gpu_samples
                .iter()
                .map(|sample| value(sample) as f64 / 1_000_000.0)
                .collect::<Vec<_>>();
            values.sort_by(|a, b| a.total_cmp(b));
            values
        };
        let gpu = sorted_stage(|sample| sample.total_ns);
        let raster = sorted_stage(|sample| sample.id_mask_raster_ns);
        let seed = sorted_stage(|sample| sample.id_mask_field_seed_ns);
        let jump = sorted_stage(|sample| sample.id_mask_field_jump_ns);
        let compositor = sorted_stage(|sample| sample.id_mask_compositor_ns);
        let stats = renderer.borrow().last_stats();
        renderer
            .borrow_mut()
            .set_timestamp_readback_interval_for_benchmark(8);
        Ok(format!(
            "{name}_timing_samples={expected};{name}_cpu_p50_ms={:.6};{name}_cpu_p95_ms={:.6};{name}_cpu_p99_ms={:.6};{name}_cpu_peak_ms={:.6};{name}_gpu_p50_ms={:.6};{name}_gpu_p95_ms={:.6};{name}_gpu_p99_ms={:.6};{name}_gpu_peak_ms={:.6};{name}_gpu_raster_p50_ms={:.6};{name}_gpu_seed_p50_ms={:.6};{name}_gpu_jump_p50_ms={:.6};{name}_gpu_compositor_p50_ms={:.6};{name}_passes={};{name}_hits={};{name}_misses={};{name}_resident_bytes={};{name}_budget_bytes={}",
            percentile(&cpu_values, 0.50),
            percentile(&cpu_values, 0.95),
            percentile(&cpu_values, 0.99),
            cpu_values.last().copied().unwrap_or(0.0),
            percentile(&gpu, 0.50),
            percentile(&gpu, 0.95),
            percentile(&gpu, 0.99),
            gpu.last().copied().unwrap_or(0.0),
            percentile(&raster, 0.50),
            percentile(&seed, 0.50),
            percentile(&jump, 0.50),
            percentile(&compositor, 0.50),
            stats.render_passes,
            stats.id_mask_cache_hits,
            stats.id_mask_cache_misses,
            stats.id_mask_cache_resident_bytes,
            stats.id_mask_cache_budget_bytes,
        ))
    }

    fn webgpu_asymmetric_id_mask_frame(renderer: &mut BrowserRenderer) -> Result<(), JsValue>
    {
        const WIDTH: usize = 17;
        const HEIGHT: usize = 11;
        let mut vertices = Vec::with_capacity(24);
        for (x, y, city, neighborhood) in [
            (0_usize, 5_usize, 1_u8, 3_u8),
            (1, 5, 1, 7),
            (5, 1, 2, 11),
            (13, 8, 3, 19),
        ]
        {
            let x0 = x as f32;
            let y0 = y as f32;
            let x1 = x0 + 1.0;
            let y1 = y0 + 1.0;
            let vertex = |position| {
                id_mask_compositor::IdMaskRasterVertex::new(position, city, neighborhood)
            };
            vertices.extend_from_slice(&[
                vertex([x0, y0]),
                vertex([x1, y0]),
                vertex([x0, y1]),
                vertex([x1, y0]),
                vertex([x1, y1]),
                vertex([x0, y1]),
            ]);
        }
        let chunks = [id_mask_compositor::IdMaskRasterChunk {
            content_hash: 0xC03,
            first_vertex: 0,
            vertex_count: vertices.len(),
        }];
        let mut pass = webgpu_id_mask_pass(&vertices, &chunks, 1);
        pass.raster.viewport = gfx::RectF::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32);
        pass.raster.mask_width = WIDTH;
        pass.raster.mask_height = HEIGHT;
        pass.raster.mask_scale = 1.0;
        let distractor_vertex = |position| {
            id_mask_compositor::IdMaskRasterVertex::new(position, 4, 31)
        };
        let distractor_vertices = [
            distractor_vertex([0.0, 0.0]),
            distractor_vertex([WIDTH as f32, 0.0]),
            distractor_vertex([0.0, HEIGHT as f32]),
            distractor_vertex([WIDTH as f32, 0.0]),
            distractor_vertex([WIDTH as f32, HEIGHT as f32]),
            distractor_vertex([0.0, HEIGHT as f32]),
        ];
        let distractor_chunks = [id_mask_compositor::IdMaskRasterChunk {
            content_hash: 0xC04,
            first_vertex: 0,
            vertex_count: distractor_vertices.len(),
        }];
        let mut distractor = webgpu_id_mask_pass(&distractor_vertices, &distractor_chunks, 0);
        distractor.raster.viewport = pass.raster.viewport;
        distractor.raster.mask_width = WIDTH;
        distractor.raster.mask_height = HEIGHT;
        distractor.raster.mask_scale = 1.0;
        renderer.resize(WIDTH as u32, HEIGHT as u32, 1.0).map_err(render_err)?;
        let token = renderer.begin_frame(&gfx::FrameTarget, None);
        renderer.encode_id_mask_gpu_compositor(&distractor).map_err(render_err)?;
        renderer.encode_id_mask_gpu_compositor(&pass).map_err(render_err)?;
        renderer.submit(token).map_err(render_err)
    }

    fn webgpu_single_seed_id_mask_frame(
        renderer: &mut BrowserRenderer,
        width: usize,
        height: usize,
        seed_x: usize,
        seed_y: usize,
        revision: u64,
    ) -> Result<(), JsValue>
    {
        let x0 = seed_x as f32;
        let y0 = seed_y as f32;
        let x1 = x0 + 1.0;
        let y1 = y0 + 1.0;
        let vertex = |position| {
            id_mask_compositor::IdMaskRasterVertex::new(position, 2, 17)
        };
        let vertices = [
            vertex([x0, y0]),
            vertex([x1, y0]),
            vertex([x0, y1]),
            vertex([x1, y0]),
            vertex([x1, y1]),
            vertex([x0, y1]),
        ];
        let chunks = [id_mask_compositor::IdMaskRasterChunk {
            content_hash: ((width as u64) << 32) | height as u64,
            first_vertex: 0,
            vertex_count: vertices.len(),
        }];
        let mut pass = webgpu_id_mask_pass(&vertices, &chunks, revision);
        pass.raster.viewport = gfx::RectF::new(0.0, 0.0, width as f32, height as f32);
        pass.raster.mask_width = width;
        pass.raster.mask_height = height;
        pass.raster.mask_scale = 1.0;
        renderer.resize(width as u32, height as u32, 1.0).map_err(render_err)?;
        let token = renderer.begin_frame(&gfx::FrameTarget, None);
        renderer.encode_id_mask_gpu_compositor(&pass).map_err(render_err)?;
        renderer.submit(token).map_err(render_err)
    }

    async fn wait_webgpu_id_mask_snapshot(
        renderer: &Rc<RefCell<BrowserRenderer>>,
    ) -> Result<WebIdMaskSnapshotReadback, JsValue>
    {
        for _ in 0..WEBGPU_TIMESTAMP_SETTLE_RAFS
        {
            wait_animation_frame_once().await?;
            let readback = renderer.borrow_mut().collect_id_mask_snapshot_readback();
            if let Some(readback) = readback
            {
                return readback.map_err(render_err);
            }
        }
        Err(JsValue::from_str("WebGPU ID-mask readback did not settle"))
    }

    fn id_mask_snapshot_json(readback: &WebIdMaskSnapshotReadback, stats: WebRendererStats) -> String
    {
        let mut json = String::with_capacity(
            readback.city.len().saturating_mul(48),
        );
        let _ = write!(json, "{{\"width\":{},\"height\":{},\"city\":[", readback.width, readback.height);
        for (index, value) in readback.city.iter().enumerate()
        {
            if index != 0
            {
                json.push(',');
            }
            let _ = write!(json, "{value}");
        }
        json.push_str("],\"neighborhood\":[");
        for (index, value) in readback.neighborhood.iter().enumerate()
        {
            if index != 0
            {
                json.push(',');
            }
            let _ = write!(json, "{value}");
        }
        json.push_str("],\"city_field\":[");
        write_field_json(&mut json, &readback.city_field);
        json.push_str("],\"seam_field\":[");
        write_field_json(&mut json, &readback.seam_field);
        let _ = write!(
            json,
            "],\"packed_fields\":{},\"field_logical_bytes\":{},\"wide_field_logical_bytes\":{},\"encoded_id_mask_draws\":{},\"uniform_writes\":{},\"uniform_bytes\":{},\"uniform_slots\":{},\"cache_hits\":{},\"cache_misses\":{},\"raster_passes\":{},\"seed_passes\":{},\"jump_passes\":{},\"compositor_passes\":{}}}",
            readback.packed_fields,
            readback.field_logical_bytes,
            readback.wide_field_logical_bytes,
            stats.id_mask_draws,
            stats.id_mask_uniform_writes,
            stats.id_mask_uniform_bytes,
            stats.id_mask_uniform_slots,
            stats.id_mask_cache_hits,
            stats.id_mask_cache_misses,
            stats.id_mask_raster_passes,
            stats.id_mask_field_seed_passes,
            stats.id_mask_field_jump_passes,
            stats.id_mask_compositor_passes,
        );
        json
    }

    fn write_field_json(json: &mut String, field: &[[f32; 4]])
    {
        for (index, pixel) in field.iter().enumerate()
        {
            if index != 0
            {
                json.push(',');
            }
            let _ = write!(
                json,
                "[{:.1},{:.1},{:.1},{:.1}]",
                pixel[0], pixel[1], pixel[2], pixel[3],
            );
        }
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
        chunks: &'a [id_mask_compositor::IdMaskRasterChunk],
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
                chunks,
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

    fn timestamp_stats_cover_row(stats: &WebRendererStats, after_frame_id: u64) -> bool {
        stats.gpu_timestamp_passes > 0
            && stats.gpu_timestamp_frame_id > after_frame_id
            && stats.gpu_timestamp_passes == stats.render_passes
    }

    #[derive(Clone, Copy)]
    struct TimestampSettleDiagnostics {
        stats: WebRendererStats,
        elapsed_ms: f64,
        raf_waits: u32,
        pending_initial: u32,
        pending_final: u32,
    }

    async fn settle_renderer_timestamps(
        renderer: &Rc<RefCell<BrowserRenderer>>,
        after_frame_id: u64,
    ) -> Result<WebRendererStats, JsValue> {
        settle_renderer_timestamps_diagnostic(renderer, after_frame_id)
            .await
            .map(|settle| settle.stats)
    }

    async fn settle_renderer_timestamps_diagnostic(
        renderer: &Rc<RefCell<BrowserRenderer>>,
        after_frame_id: u64,
    ) -> Result<TimestampSettleDiagnostics, JsValue> {
        let start_ms = perf_now();
        let target_frame_id = renderer.borrow().last_stats().frame_id;
        let mut stats = renderer.borrow_mut().collect_timestamp_readbacks();
        let mut pending_readbacks = renderer.borrow().pending_timestamp_readbacks();
        let pending_initial = pending_readbacks;
        if !stats.gpu_timestamp_query_supported {
            return Ok(TimestampSettleDiagnostics {
                stats,
                elapsed_ms: (perf_now() - start_ms).max(0.0),
                raf_waits: 0,
                pending_initial,
                pending_final: pending_readbacks,
            });
        }
        if timestamp_stats_cover_row(&stats, after_frame_id) && pending_readbacks == 0 {
            return Ok(TimestampSettleDiagnostics {
                stats,
                elapsed_ms: (perf_now() - start_ms).max(0.0),
                raf_waits: 0,
                pending_initial,
                pending_final: pending_readbacks,
            });
        }
        for raf_waits in 1..=WEBGPU_TIMESTAMP_SETTLE_RAFS {
            wait_animation_frame_once().await?;
            stats = renderer.borrow_mut().collect_timestamp_readbacks();
            pending_readbacks = renderer.borrow().pending_timestamp_readbacks();
            if timestamp_stats_cover_row(&stats, after_frame_id) && pending_readbacks == 0 {
                return Ok(TimestampSettleDiagnostics {
                    stats,
                    elapsed_ms: (perf_now() - start_ms).max(0.0),
                    raf_waits,
                    pending_initial,
                    pending_final: pending_readbacks,
                });
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
                let _ = reject
                    .call1(&JsValue::UNDEFINED, &JsValue::from_str("raf callback unavailable"));
                return;
            };
            if let Err(error) = window.request_animation_frame(function) {
                let _ = reject.call1(&JsValue::UNDEFINED, &error);
            }
        });
        JsFuture::from(promise).await.map(|_| ())
    }

    async fn wait_renderer_queue_idle(
        renderer: &Rc<RefCell<BrowserRenderer>>,
    ) -> Result<(), JsValue> {
        let completed = renderer.borrow().queue_completion_flag_for_benchmark();
        for _ in 0..WEBGPU_TIMESTAMP_SETTLE_RAFS {
            if completed.load(Ordering::Acquire) {
                wait_animation_frame_once().await?;
                return Ok(());
            }
            wait_animation_frame_once().await?;
        }
        Err(JsValue::from_str("WebGPU architecture benchmark queue did not drain"))
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
            borrowed.raf_requests = borrowed.raf_requests.saturating_add(1);
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
        install_frame_event_listener(state, window_target, "resize", true, false)?;
        install_frame_event_listener(state, window_target, "scroll", true, true)?;
        install_frame_event_listener(state, window_target, "oxide-redraw", false, false)?;
        install_canvas_observers(state)?;
        install_ime_listeners(state, window_target)
    }

    fn install_canvas_observers(state: &Rc<RefCell<AppState>>) -> Result<(), JsValue> {
        let canvas = state.borrow().canvas.clone();
        let state_for_resize = Rc::clone(state);
        let resize_callback = Closure::wrap(Box::new(move |entries: js_sys::Array| {
            let Some(entry) = entries.get(0).dyn_into::<ResizeObserverEntry>().ok() else {
                return;
            };
            let rect = entry.content_rect();
            let css_width = rect.width().max(1.0) as f32;
            let css_height = rect.height().max(1.0) as f32;
            let changed = {
                let state = state_for_resize.borrow();
                (state.canvas_metrics.css_width - css_width).abs() > f32::EPSILON
                    || (state.canvas_metrics.css_height - css_height).abs() > f32::EPSILON
            };
            if changed {
                state_for_resize.borrow_mut().mark_canvas_metrics_dirty();
                request_next_frame(&state_for_resize);
            }
        }) as Box<dyn FnMut(js_sys::Array)>);
        let resize_observer = ResizeObserver::new(resize_callback.as_ref().unchecked_ref())?;
        resize_observer.observe(canvas.unchecked_ref());

        let document = web_sys::window()
            .and_then(|window| window.document())
            .ok_or_else(|| JsValue::from_str("document is unavailable"))?;
        let document_root = document
            .document_element()
            .ok_or_else(|| JsValue::from_str("document root is unavailable"))?;
        let state_for_mutation = Rc::clone(state);
        let mutation_callback = Closure::wrap(Box::new(move |_records: js_sys::Array| {
            state_for_mutation.borrow_mut().mark_canvas_metrics_dirty();
            request_next_frame(&state_for_mutation);
        }) as Box<dyn FnMut(js_sys::Array)>);
        let mutation_observer = MutationObserver::new(mutation_callback.as_ref().unchecked_ref())?;
        let options = MutationObserverInit::new();
        let attribute_filter = js_sys::Array::new();
        attribute_filter.push(&JsValue::from_str("class"));
        attribute_filter.push(&JsValue::from_str("style"));
        options.set_attributes(true);
        options.set_attribute_filter(attribute_filter.as_ref());
        options.set_child_list(true);
        options.set_subtree(true);
        mutation_observer.observe_with_options(document_root.unchecked_ref(), &options)?;

        let mut state = state.borrow_mut();
        state.resize_observer = Some(resize_observer);
        state.resize_observer_callback = Some(resize_callback);
        state.mutation_observer = Some(mutation_observer);
        state.mutation_observer_callback = Some(mutation_callback);
        Ok(())
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

    fn create_hidden_canvas(width: u32, height: u32) -> Result<HtmlCanvasElement, JsValue> {
        let document = web_sys::window()
            .and_then(|window| window.document())
            .ok_or_else(|| JsValue::from_str("document is unavailable"))?;
        let canvas = document
            .create_element("canvas")?
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| JsValue::from_str("created element was not a canvas"))?;
        canvas.set_width(width.max(1));
        canvas.set_height(height.max(1));
        let style = canvas.style();
        style.set_property("position", "fixed")?;
        style.set_property("left", "-10000px")?;
        style.set_property("top", "-10000px")?;
        style.set_property("width", "1px")?;
        style.set_property("height", "1px")?;
        style.set_property("opacity", "0")?;
        style.set_property("pointer-events", "none")?;
        if let Some(body) = document.body() {
            let _ = body.append_child(canvas.unchecked_ref());
        }
        Ok(canvas)
    }

    fn remove_hidden_canvas(canvas: &HtmlCanvasElement) {
        if let Some(parent) = canvas.parent_node() {
            let _ = parent.remove_child(canvas.unchecked_ref());
        }
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
            {
                let mut state = state_for_event.borrow_mut();
                state.mark_frame_dirty();
                if name == "pointermove" {
                    state.pointer_anticipation = true;
                }
            }
            let phase = touch_phase_for_event(name);
            let (x, y) = {
                let mut state = state_for_event.borrow_mut();
                let _ = state.refresh_canvas_metrics();
                event_point(
                    state.canvas_metrics,
                    pointer.client_x() as f32,
                    pointer.client_y() as f32,
                )
            };
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
            } else {
                let mut state = state_for_event.borrow_mut();
                state.router.input_pointer(
                    x,
                    y,
                    pointer.movement_x() as f32,
                    pointer.movement_y() as f32,
                    pointer.buttons() as u32,
                );
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
            state_for_event.borrow_mut().mark_frame_dirty();
            let (x, y) = {
                let mut state = state_for_event.borrow_mut();
                let _ = state.refresh_canvas_metrics();
                event_point(
                    state.canvas_metrics,
                    wheel.client_x() as f32,
                    wheel.client_y() as f32,
                )
            };
            let delta = wheel.delta_y() as f32;
            if wheel.ctrl_key() || wheel.meta_key() {
                let mut state = state_for_event.borrow_mut();
                state.router.input_pinch(x, y, -delta * 0.001);
            } else {
                let mut state = state_for_event.borrow_mut();
                state.router.input_pointer(x, y, 0.0, -delta, 0);
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
            let down = name == "keydown";
            let handled = route_key(&mut state.router, keyboard, down, ime_focused);
            if handled {
                event.prevent_default();
            }
            let needs_frame = handled || ime_focused && down;
            if needs_frame {
                state.mark_frame_dirty();
            }
            drop(state);
            if needs_frame {
                request_next_frame(&state_for_event);
            }
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
            let metrics = state.canvas_metrics;
            let logical_h = metrics.physical_height as f32 / metrics.scale.max(1.0);
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
        canvas_metrics_dirty: bool,
        capture: bool,
    ) -> Result<(), JsValue> {
        let state_for_event = Rc::clone(state);
        let closure = Closure::wrap(Box::new(move |_event: Event| {
            {
                let mut state = state_for_event.borrow_mut();
                if state.direct_capture_active {
                    return;
                }
                if canvas_metrics_dirty {
                    state.mark_canvas_metrics_dirty();
                } else {
                    state.mark_frame_dirty();
                }
            }
            request_next_frame(&state_for_event);
        }) as Box<dyn FnMut(Event)>);
        target.add_event_listener_with_callback_and_bool(
            name,
            closure.as_ref().unchecked_ref(),
            capture,
        )?;
        state.borrow_mut().listeners.push(closure);
        Ok(())
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

    fn event_point(metrics: CanvasMetrics, client_x: f32, client_y: f32) -> (f32, f32) {
        (client_x - metrics.left, client_y - metrics.top)
    }

    fn canvas_by_id(id: &str) -> Result<HtmlCanvasElement, JsValue> {
        let document = web_sys::window()
            .and_then(|window| window.document())
            .ok_or_else(|| JsValue::from_str("document is unavailable"))?;
        document
            .get_element_by_id(id)
            .ok_or_else(|| JsValue::from_str("canvas id was not found"))?
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| JsValue::from_str("element is not a canvas"))
    }

    fn measure_canvas_metrics(canvas: &HtmlCanvasElement) -> CanvasMetrics {
        let rect = canvas.get_bounding_client_rect();
        let scale = web_sys::window()
            .map(|window| window.device_pixel_ratio() as f32)
            .unwrap_or(1.0)
            .max(1.0);
        let css_width = rect.width().max(1.0) as f32;
        let css_height = rect.height().max(1.0) as f32;
        CanvasMetrics {
            physical_width: (css_width * scale).round().max(1.0) as u32,
            physical_height: (css_height * scale).round().max(1.0) as u32,
            css_width,
            css_height,
            scale,
            left: rect.left() as f32,
            top: rect.top() as f32,
        }
    }

    fn c60_web_icon_png(seed: u64, size: u32) -> Result<Arc<[u8]>, JsValue> {
        let low = seed as u8;
        let high = (seed >> 8) as u8;
        let mut pixels = Vec::with_capacity(size as usize * size as usize * 4);
        for y in 0..size {
            for x in 0..size {
                let checker = (((x / 8) ^ (y / 8)) & 1) as u8;
                pixels.extend_from_slice(&[
                    low.wrapping_mul(17).wrapping_add((x as u8).wrapping_mul(3)),
                    high.wrapping_mul(29).wrapping_add((y as u8).wrapping_mul(5)),
                    72_u8
                        .wrapping_add(checker.wrapping_mul(108))
                        .wrapping_add(low.wrapping_mul(7)),
                    255,
                ]);
            }
        }
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, size, size);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|error| JsValue::from_str(&format!("encoding C60 PNG header: {error}")))?;
            writer
                .write_image_data(&pixels)
                .map_err(|error| JsValue::from_str(&format!("encoding C60 PNG pixels: {error}")))?;
        }
        Ok(encoded.into())
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

    fn timing_stage_begin(stages: Option<&WebGpuFrameStageTimingSample>) -> Option<f64> {
        stages.map(|_| perf_now())
    }

    fn timing_stage_end(
        stages: Option<&mut WebGpuFrameStageTimingSample>,
        stage: WebGpuFrameStage,
        before_ms: Option<f64>,
    ) {
        if let (Some(stages), Some(before_ms)) = (stages, before_ms) {
            stages.values_ms[stage as usize] = (perf_now() - before_ms).max(0.0);
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
        summary.realloc_grow_bytes = summary.realloc_grow_bytes.saturating_add(realloc_grow_bytes);
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

    fn add_submit_allocation_frame(
        summary: &mut WebGpuSubmitAllocationSummary,
        stats: WebRendererStats,
    ) {
        summary.upload_alloc_count =
            summary.upload_alloc_count.saturating_add(stats.submit_upload_alloc_count);
        summary.upload_alloc_bytes =
            summary.upload_alloc_bytes.saturating_add(stats.submit_upload_alloc_bytes);
        summary.surface_alloc_count =
            summary.surface_alloc_count.saturating_add(stats.submit_surface_alloc_count);
        summary.surface_alloc_bytes =
            summary.surface_alloc_bytes.saturating_add(stats.submit_surface_alloc_bytes);
        summary.encoder_alloc_count =
            summary.encoder_alloc_count.saturating_add(stats.submit_encoder_alloc_count);
        summary.encoder_alloc_bytes =
            summary.encoder_alloc_bytes.saturating_add(stats.submit_encoder_alloc_bytes);
        summary.render_alloc_count =
            summary.render_alloc_count.saturating_add(stats.submit_render_alloc_count);
        summary.render_alloc_bytes =
            summary.render_alloc_bytes.saturating_add(stats.submit_render_alloc_bytes);
        summary.timestamp_alloc_count =
            summary.timestamp_alloc_count.saturating_add(stats.submit_timestamp_alloc_count);
        summary.timestamp_alloc_bytes =
            summary.timestamp_alloc_bytes.saturating_add(stats.submit_timestamp_alloc_bytes);
        summary.scratch_stats_alloc_count = summary
            .scratch_stats_alloc_count
            .saturating_add(stats.submit_scratch_stats_alloc_count);
        summary.scratch_stats_alloc_bytes = summary
            .scratch_stats_alloc_bytes
            .saturating_add(stats.submit_scratch_stats_alloc_bytes);
        summary.finish_queue_alloc_count =
            summary.finish_queue_alloc_count.saturating_add(stats.submit_finish_queue_alloc_count);
        summary.finish_queue_alloc_bytes =
            summary.finish_queue_alloc_bytes.saturating_add(stats.submit_finish_queue_alloc_bytes);
        summary.present_alloc_count =
            summary.present_alloc_count.saturating_add(stats.submit_present_alloc_count);
        summary.present_alloc_bytes =
            summary.present_alloc_bytes.saturating_add(stats.submit_present_alloc_bytes);
        summary.timestamp_map_alloc_count = summary
            .timestamp_map_alloc_count
            .saturating_add(stats.submit_timestamp_map_alloc_count);
        summary.timestamp_map_alloc_bytes = summary
            .timestamp_map_alloc_bytes
            .saturating_add(stats.submit_timestamp_map_alloc_bytes);
        summary.total_alloc_count =
            summary.total_alloc_count.saturating_add(stats.submit_total_alloc_count);
        summary.total_alloc_bytes =
            summary.total_alloc_bytes.saturating_add(stats.submit_total_alloc_bytes);
        summary.total_realloc_count =
            summary.total_realloc_count.saturating_add(stats.submit_total_realloc_count);
        summary.total_realloc_grow_bytes =
            summary.total_realloc_grow_bytes.saturating_add(stats.submit_total_realloc_grow_bytes);
    }

    fn submit_allocation_metrics(summary: &WebGpuSubmitAllocationSummary, prefix: &str) -> String {
        let mut out = String::new();
        let key_prefix = if prefix.is_empty() { String::new() } else { format!("{prefix}_") };
        let _ = write!(
            out,
            ";{key_prefix}submit_upload_alloc_count={};{key_prefix}submit_upload_alloc_bytes={};{key_prefix}submit_surface_alloc_count={};{key_prefix}submit_surface_alloc_bytes={};{key_prefix}submit_encoder_alloc_count={};{key_prefix}submit_encoder_alloc_bytes={};{key_prefix}submit_render_alloc_count={};{key_prefix}submit_render_alloc_bytes={};{key_prefix}submit_timestamp_alloc_count={};{key_prefix}submit_timestamp_alloc_bytes={};{key_prefix}submit_scratch_stats_alloc_count={};{key_prefix}submit_scratch_stats_alloc_bytes={};{key_prefix}submit_finish_queue_alloc_count={};{key_prefix}submit_finish_queue_alloc_bytes={};{key_prefix}submit_present_alloc_count={};{key_prefix}submit_present_alloc_bytes={};{key_prefix}submit_timestamp_map_alloc_count={};{key_prefix}submit_timestamp_map_alloc_bytes={};{key_prefix}submit_total_alloc_count={};{key_prefix}submit_total_alloc_bytes={};{key_prefix}submit_total_realloc_count={};{key_prefix}submit_total_realloc_grow_bytes={}",
            summary.upload_alloc_count,
            summary.upload_alloc_bytes,
            summary.surface_alloc_count,
            summary.surface_alloc_bytes,
            summary.encoder_alloc_count,
            summary.encoder_alloc_bytes,
            summary.render_alloc_count,
            summary.render_alloc_bytes,
            summary.timestamp_alloc_count,
            summary.timestamp_alloc_bytes,
            summary.scratch_stats_alloc_count,
            summary.scratch_stats_alloc_bytes,
            summary.finish_queue_alloc_count,
            summary.finish_queue_alloc_bytes,
            summary.present_alloc_count,
            summary.present_alloc_bytes,
            summary.timestamp_map_alloc_count,
            summary.timestamp_map_alloc_bytes,
            summary.total_alloc_count,
            summary.total_alloc_bytes,
            summary.total_realloc_count,
            summary.total_realloc_grow_bytes,
        );
        out
    }

    fn frame_stage_timing_metrics(sample: &WebGpuFrameStageTimingSample) -> String {
        let mut out = format!("total_ms={:.6}", sample.total_ms);
        for (stage, name) in WEBGPU_FRAME_STAGES {
            let _ = write!(out, ";{name}_ms={:.6}", sample.values_ms[stage as usize]);
        }
        let event_update_ms = sample.values_ms[WebGpuFrameStage::FrameTiming as usize]
            + sample.values_ms[WebGpuFrameStage::RouterUpdate as usize];
        let draw_extraction_ms = sample.values_ms[WebGpuFrameStage::BuilderClear as usize]
            + sample.values_ms[WebGpuFrameStage::RouterDraw as usize]
            + sample.values_ms[WebGpuFrameStage::DamageHandoff as usize];
        let command_encoding_ms = sample.cpu_submit.surface_ms
            + sample.cpu_submit.encoder_create_ms
            + sample.cpu_submit.command_encoding_ms
            + sample.cpu_submit.timestamp_readback_ms
            + sample.cpu_submit.scratch_stats_ms;
        let post_submit_ms = sample.values_ms[WebGpuFrameStage::PostSubmit as usize]
            + sample.cpu_submit.present_ms
            + sample.cpu_submit.timestamp_map_ms;
        let _ = write!(
            out,
            ";event_update_ms={event_update_ms:.6};layout_ms=0;text_prepare_ms=0;draw_extraction_ms={draw_extraction_ms:.6};coalescing_ms={:.6};backend_lowering_ms={:.6};upload_ms={:.6};command_encoding_ms={command_encoding_ms:.6};queue_submit_ms={:.6};post_submit_contract_ms={post_submit_ms:.6}",
            sample.values_ms[WebGpuFrameStage::DrawCoalesce as usize],
            sample.values_ms[WebGpuFrameStage::EncodePass as usize],
            sample.cpu_submit.upload_ms,
            sample.cpu_submit.queue_submit_ms,
        );
        out
    }

    fn timestamp_samples_json(
        samples: &[WebGpuTimestampSample],
        settle: TimestampSettleDiagnostics,
    ) -> String {
        let stats = settle.stats;
        let mut out = format!(
            "{{\"supported\":{},\"readback_skips\":{},\"queue_drain_ms\":{:.6},\"queue_drain_raf_waits\":{},\"queue_pending_initial\":{},\"queue_pending_final\":{},\"samples\":[",
            stats.gpu_timestamp_query_supported,
            stats.gpu_timestamp_readback_skips,
            settle.elapsed_ms,
            settle.raf_waits,
            settle.pending_initial,
            settle.pending_final,
        );
        for (index, sample) in samples.iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"frame_id\":{},\"passes\":{},\"total_ns\":{},\"backdrop_copy_ns\":{},\"clear_ns\":{},\"draw_ns\":{},\"scene3d_ns\":{},\"scene3d_overlay_ns\":{},\"id_mask_raster_ns\":{},\"id_mask_field_seed_ns\":{},\"id_mask_field_jump_ns\":{},\"id_mask_compositor_ns\":{},\"present_ns\":{},\"max_pass_ns\":{}}}",
                sample.frame_id,
                sample.passes,
                sample.total_ns,
                sample.backdrop_copy_ns,
                sample.clear_ns,
                sample.draw_ns,
                sample.scene3d_ns,
                sample.scene3d_overlay_ns,
                sample.id_mask_raster_ns,
                sample.id_mask_field_seed_ns,
                sample.id_mask_field_jump_ns,
                sample.id_mask_compositor_ns,
                sample.present_ns,
                sample.max_pass_ns,
            );
        }
        out.push_str("]}");
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

    fn renderer_stats_metrics(stats: WebRendererStats, prefix: &str) -> String {
        let mut out = String::new();
        let key_prefix = if prefix.is_empty() { String::new() } else { format!("{prefix}_") };
        let _ = write!(
            out,
            ";{key_prefix}draws={};{key_prefix}draw_items={};{key_prefix}draw_items_coalesced={};{key_prefix}draw_pipeline_binds={};{key_prefix}draw_bind_group_binds={};{key_prefix}draw_scissor_sets={};{key_prefix}solid_tris={};{key_prefix}rrect_instances={};{key_prefix}rrect_triangles={};{key_prefix}rrect_instance_bytes={};{key_prefix}image_instances={};{key_prefix}image_triangles={};{key_prefix}image_instance_bytes={};{key_prefix}image_draws={};{key_prefix}image_mesh_draws={};{key_prefix}nine_slice_draws={};{key_prefix}nine_slice_instances={};{key_prefix}nine_slice_triangles={};{key_prefix}nine_slice_instance_bytes={};{key_prefix}glyph_quads={};{key_prefix}sdf_glyph_quads={};{key_prefix}glyph_instances={};{key_prefix}glyph_triangles={};{key_prefix}glyph_instance_bytes={};{key_prefix}glyph_instance_buffer_binds={};{key_prefix}clip_depth_peak={};{key_prefix}damage_rects={};{key_prefix}layer_draws={};{key_prefix}layer_cache_hits={};{key_prefix}layer_cache_misses={};{key_prefix}layer_cache_skipped_draws={};{key_prefix}layer_passes={};{key_prefix}scene3d_draws={};{key_prefix}scene3d_instances={};{key_prefix}scene3d_instance_bytes={};{key_prefix}scene3d_pipeline_binds={};{key_prefix}scene3d_bind_group_binds={};{key_prefix}scene3d_mesh_buffer_binds={};{key_prefix}scene3d_viewport_sets={};{key_prefix}id_mask_draws={};{key_prefix}backdrop_draws={};{key_prefix}visual_effect_draws={};{key_prefix}effect_uniform_writes={};{key_prefix}effect_uniform_bytes={};{key_prefix}effect_uniform_slots={};{key_prefix}id_mask_uniform_writes={};{key_prefix}id_mask_uniform_bytes={};{key_prefix}id_mask_uniform_slots={};{key_prefix}spinner_draws={};{key_prefix}spinner_instances={};{key_prefix}spinner_triangles={};{key_prefix}spinner_instance_bytes={};{key_prefix}neon_marker_instances={};{key_prefix}neon_marker_triangles={};{key_prefix}neon_marker_instance_bytes={};{key_prefix}camera_bg_draws={};{key_prefix}render_passes={};{key_prefix}clear_passes={};{key_prefix}draw_passes={};{key_prefix}scene3d_passes={};{key_prefix}scene3d_overlay_passes={};{key_prefix}id_mask_raster_passes={};{key_prefix}id_mask_field_seed_passes={};{key_prefix}id_mask_field_jump_passes={};{key_prefix}id_mask_compositor_passes={};{key_prefix}present_passes={};{key_prefix}texture_copies={};{key_prefix}command_buffers={};{key_prefix}gpu_timestamp_query_supported={};{key_prefix}gpu_timestamp_frame_id={};{key_prefix}gpu_timestamp_passes={};{key_prefix}gpu_timestamp_total_ns={};{key_prefix}gpu_timestamp_backdrop_copy_ns={};{key_prefix}gpu_timestamp_clear_ns={};{key_prefix}gpu_timestamp_draw_ns={};{key_prefix}gpu_timestamp_scene3d_ns={};{key_prefix}gpu_timestamp_scene3d_overlay_ns={};{key_prefix}gpu_timestamp_id_mask_raster_ns={};{key_prefix}gpu_timestamp_id_mask_field_seed_ns={};{key_prefix}gpu_timestamp_id_mask_field_jump_ns={};{key_prefix}gpu_timestamp_id_mask_compositor_ns={};{key_prefix}gpu_timestamp_present_ns={};{key_prefix}gpu_timestamp_max_pass_ns={};{key_prefix}gpu_timestamp_readback_skips={};{key_prefix}gpu_timestamp_readback_interval={};{key_prefix}buffer_upload_bytes={};{key_prefix}texture_upload_bytes={};{key_prefix}buffer_grows={};{key_prefix}texture_creates={};{key_prefix}bind_group_creates={};{key_prefix}pipeline_creates={};{key_prefix}sampler_creates={};{key_prefix}mesh3d_creates={};{key_prefix}draw_buffer_grows={};{key_prefix}image_texture_creates={};{key_prefix}image_bind_group_creates={};{key_prefix}target_texture_creates={};{key_prefix}target_bind_group_creates={};{key_prefix}layer_texture_creates={};{key_prefix}layer_bind_group_creates={};{key_prefix}scene3d_buffer_grows={};{key_prefix}scene3d_bind_group_creates={};{key_prefix}effect_buffer_grows={};{key_prefix}effect_bind_group_creates={};{key_prefix}id_mask_texture_creates={};{key_prefix}id_mask_buffer_grows={};{key_prefix}id_mask_bind_group_creates={};{key_prefix}image_upload_temp_allocs={};{key_prefix}image_upload_temp_bytes={};{key_prefix}image_upload_scratch_bytes={};{key_prefix}image_upload_scratch_grows={};{key_prefix}cpu_scratch_bytes={};{key_prefix}cpu_scratch_grows={};{key_prefix}cpu_scratch_growth_bytes={};{key_prefix}cpu_draw_scratch_bytes={};{key_prefix}cpu_draw_scratch_grows={};{key_prefix}cpu_draw_scratch_growth_bytes={};{key_prefix}cpu_scene3d_scratch_bytes={};{key_prefix}cpu_scene3d_scratch_grows={};{key_prefix}cpu_scene3d_scratch_growth_bytes={};{key_prefix}cpu_effect_scratch_bytes={};{key_prefix}cpu_effect_scratch_grows={};{key_prefix}cpu_effect_scratch_growth_bytes={};{key_prefix}cpu_id_mask_scratch_bytes={};{key_prefix}cpu_id_mask_scratch_grows={};{key_prefix}cpu_id_mask_scratch_growth_bytes={};{key_prefix}cpu_image_upload_scratch_bytes={};{key_prefix}cpu_image_upload_scratch_grows={};{key_prefix}cpu_image_upload_scratch_growth_bytes={};{key_prefix}cpu_resource_table_scratch_bytes={};{key_prefix}cpu_resource_table_scratch_grows={};{key_prefix}cpu_resource_table_scratch_growth_bytes={}",
            stats.draws,
            stats.draw_items,
            stats.draw_items_coalesced,
            stats.draw_pipeline_binds,
            stats.draw_bind_group_binds,
            stats.draw_scissor_sets,
            stats.solid_tris,
            stats.rrect_instances,
            stats.rrect_triangles,
            stats.rrect_instance_bytes,
            stats.image_instances,
            stats.image_triangles,
            stats.image_instance_bytes,
            stats.image_draws,
            stats.image_mesh_draws,
            stats.nine_slice_draws,
            stats.nine_slice_instances,
            stats.nine_slice_triangles,
            stats.nine_slice_instance_bytes,
            stats.glyph_quads,
            stats.sdf_glyph_quads,
            stats.glyph_instances,
            stats.glyph_triangles,
            stats.glyph_instance_bytes,
            stats.glyph_instance_buffer_binds,
            stats.clip_depth_peak,
            stats.damage_rects,
            stats.layer_draws,
            stats.layer_cache_hits,
            stats.layer_cache_misses,
            stats.layer_cache_skipped_draws,
            stats.layer_passes,
            stats.scene3d_draws,
            stats.scene3d_instances,
            stats.scene3d_instance_bytes,
            stats.scene3d_pipeline_binds,
            stats.scene3d_bind_group_binds,
            stats.scene3d_mesh_buffer_binds,
            stats.scene3d_viewport_sets,
            stats.id_mask_draws,
            stats.backdrop_draws,
            stats.visual_effect_draws,
            stats.effect_uniform_writes,
            stats.effect_uniform_bytes,
            stats.effect_uniform_slots,
            stats.id_mask_uniform_writes,
            stats.id_mask_uniform_bytes,
            stats.id_mask_uniform_slots,
            stats.spinner_draws,
            stats.spinner_instances,
            stats.spinner_triangles,
            stats.spinner_instance_bytes,
            stats.neon_marker_instances,
            stats.neon_marker_triangles,
            stats.neon_marker_instance_bytes,
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
            stats.gpu_timestamp_backdrop_copy_ns,
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
            stats.layer_texture_creates,
            stats.layer_bind_group_creates,
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
        let _ = write!(
            out,
            ";{key_prefix}effect_graph_effects={};{key_prefix}effect_graph_captures={};{key_prefix}effect_graph_pyramids={};{key_prefix}effect_graph_pyramid_reuses={};{key_prefix}effect_graph_plan_reuses={};{key_prefix}effect_graph_capture_passes={};{key_prefix}effect_graph_downsample_passes={};{key_prefix}effect_graph_blur_horizontal_passes={};{key_prefix}effect_graph_blur_vertical_passes={};{key_prefix}effect_graph_composite_passes={};{key_prefix}effect_graph_max_lifetime_commands={};{key_prefix}effect_graph_resources={};{key_prefix}effect_graph_alias_slots={};{key_prefix}effect_graph_logical_bytes={};{key_prefix}effect_graph_physical_bytes={};{key_prefix}effect_graph_aliased_bytes={}",
            stats.effect_graph_effects,
            stats.effect_graph_captures,
            stats.effect_graph_pyramids,
            stats.effect_graph_pyramid_reuses,
            stats.effect_graph_plan_reuses,
            stats.effect_graph_capture_passes,
            stats.effect_graph_downsample_passes,
            stats.effect_graph_blur_horizontal_passes,
            stats.effect_graph_blur_vertical_passes,
            stats.effect_graph_composite_passes,
            stats.effect_graph_max_lifetime_commands,
            stats.effect_graph_resources,
            stats.effect_graph_alias_slots,
            stats.effect_graph_logical_bytes,
            stats.effect_graph_physical_bytes,
            stats.effect_graph_aliased_bytes,
        );
        let _ = write!(
            out,
            ";{key_prefix}commands_traversed={};{key_prefix}commands_copied={};{key_prefix}geometry_bytes_copied={};{key_prefix}chunks_reused={};{key_prefix}chunks_rebuilt={};{key_prefix}chunks_prepared={};{key_prefix}backend_cache_hits={};{key_prefix}backend_cache_misses={};{key_prefix}render_encoders={};{key_prefix}render_bundle_creates={};{key_prefix}render_bundle_replays={};{key_prefix}render_bundle_execute_calls={};{key_prefix}render_bundle_draws={};{key_prefix}prepared_direct_draws={};{key_prefix}property_upload_bytes={};{key_prefix}property_records_updated={};{key_prefix}property_ring_bytes={};{key_prefix}texture_copy_pixels={};{key_prefix}texture_copy_bytes={};{key_prefix}shaded_damage_pixels={};{key_prefix}cache_evictions={};{key_prefix}wakeups={};{key_prefix}skipped_submissions={};{key_prefix}actual_submissions={};{key_prefix}gpu_allocated_bytes_available={};{key_prefix}gpu_logical_total_bytes={};{key_prefix}gpu_allocated_total_bytes={};{key_prefix}gpu_vertex_buffer_bytes={};{key_prefix}gpu_index_buffer_bytes={};{key_prefix}gpu_uniform_buffer_bytes={};{key_prefix}gpu_persistent_asset_bytes={};{key_prefix}gpu_transient_target_bytes={};{key_prefix}gpu_depth_target_bytes={};{key_prefix}gpu_bloom_target_bytes={};{key_prefix}gpu_layer_texture_bytes={};{key_prefix}gpu_id_mask_texture_bytes={};{key_prefix}gpu_atlas_texture_bytes={};{key_prefix}gpu_image_texture_bytes={};{key_prefix}gpu_scene3d_mesh_bytes={};{key_prefix}gpu_staging_buffer_bytes={};{key_prefix}gpu_bind_buffer_bytes={};{key_prefix}gpu_frame_ring_bytes={};{key_prefix}gpu_cache_bytes={}",
            stats.commands_traversed,
            stats.commands_copied,
            stats.geometry_bytes_copied,
            stats.chunks_reused,
            stats.chunks_rebuilt,
            stats.chunks_prepared,
            stats.backend_cache_hits,
            stats.backend_cache_misses,
            stats.render_encoders,
            stats.render_bundle_creates,
            stats.render_bundle_replays,
            stats.render_bundle_execute_calls,
            stats.render_bundle_draws,
            stats.prepared_direct_draws,
            stats.property_upload_bytes,
            stats.property_records_updated,
            stats.property_ring_bytes,
            stats.texture_copy_pixels,
            stats.texture_copy_bytes,
            stats.shaded_damage_pixels,
            stats.cache_evictions,
            stats.wakeups,
            stats.skipped_submissions,
            stats.actual_submissions,
            u32::from(stats.gpu_allocated_bytes_available),
            stats.gpu_logical_total_bytes,
            stats.gpu_allocated_total_bytes,
            stats.gpu_vertex_buffer_bytes,
            stats.gpu_index_buffer_bytes,
            stats.gpu_uniform_buffer_bytes,
            stats.gpu_persistent_asset_bytes,
            stats.gpu_transient_target_bytes,
            stats.gpu_depth_target_bytes,
            stats.gpu_bloom_target_bytes,
            stats.gpu_layer_texture_bytes,
            stats.gpu_id_mask_texture_bytes,
            stats.gpu_atlas_texture_bytes,
            stats.gpu_image_texture_bytes,
            stats.gpu_scene3d_mesh_bytes,
            stats.gpu_staging_buffer_bytes,
            stats.gpu_bind_buffer_bytes,
            stats.gpu_frame_ring_bytes,
            stats.gpu_cache_bytes,
        );
        let _ = write!(
            out,
            ";{key_prefix}id_mask_cache_hits={};{key_prefix}id_mask_cache_misses={};{key_prefix}id_mask_cache_budget_bytes={};{key_prefix}id_mask_cache_resident_bytes={};{key_prefix}id_mask_cache_evictions={};{key_prefix}id_mask_cache_entries={};{key_prefix}id_mask_cache_purges={};{key_prefix}id_mask_cache_last_purge_reason={}",
            stats.id_mask_cache_hits,
            stats.id_mask_cache_misses,
            stats.id_mask_cache_budget_bytes,
            stats.id_mask_cache_resident_bytes,
            stats.id_mask_cache_evictions,
            stats.id_mask_cache_entries,
            stats.id_mask_cache_purges,
            stats.id_mask_cache_last_purge_reason,
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
    bench_canvas_indexed_quads, platform_smoke_report, start_oxide, start_oxide_async,
    webgpu_smoke_report, webgpu_timing_report, OxideWebApp,
};

#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub const fn host_web_requires_wasm32() -> &'static str {
    "oxide-host-web exports browser entry points only for wasm32"
}
