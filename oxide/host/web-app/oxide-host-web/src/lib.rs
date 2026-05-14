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
mod wasm_host {
    use super::generate_checker_rgba;
    use oxide_platform_api as platform_api;
    use oxide_platform_api::Platform;
    use oxide_renderer_api as gfx;
    use oxide_renderer_api::Renderer;
    use oxide_renderer_web::BrowserRenderer;
    use oxide_test_scenes as scenes;
    use oxide_text as text;
    use oxide_ui_core as ui;
    use std::cell::RefCell;
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
        last_ms: u64,
        ime_focused: bool,
        ime_composing: bool,
        ime_skip_next_input: bool,
        raf: Option<Closure<dyn FnMut(f64)>>,
        listeners: Vec<Closure<dyn FnMut(Event)>>,
    }

    impl AppState {
        fn frame_at(&mut self, timestamp_ms: f64) -> Result<(), JsValue> {
            let (physical_w, physical_h, scale) = canvas_backing_size(&self.canvas);
            self.renderer.borrow_mut().resize(physical_w, physical_h, scale).map_err(render_err)?;

            let now_ms = timestamp_ms.max(0.0).round() as u64;
            let dt_ms = if self.last_ms == 0 {
                16
            } else {
                now_ms.saturating_sub(self.last_ms).min(u32::MAX as u64) as u32
            };
            self.last_ms = now_ms;

            self.builder.clear();
            let viewport = gfx::RectF::new(
                0.0,
                0.0,
                physical_w as f32 / scale.max(1.0),
                physical_h as f32 / scale.max(1.0),
            );
            self.router.update(now_ms, dt_ms);
            self.router.draw(viewport, scale, &mut self.builder);
            let damage = gfx::Damage { rects: self.router.take_damage() };
            ui::coalesce_adjacent_draws(self.builder.drawlist_mut());

            let mut renderer = self.renderer.borrow_mut();
            let token = renderer.begin_frame(&gfx::FrameTarget, Some(&damage));
            renderer.encode_pass(self.builder.drawlist());
            renderer.submit(token).map_err(render_err)
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
                last_ms: 0,
                ime_focused: false,
                ime_composing: false,
                ime_skip_next_input: false,
                raf: None,
                listeners: Vec::new(),
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
                    let _ = state_for_frame.borrow_mut().frame_at(timestamp_ms);
                }
                request_next_frame(&state_for_frame);
            }) as Box<dyn FnMut(f64)>);
            self.state.borrow_mut().raf = Some(closure);
            request_next_frame(&self.state);
            Ok(())
        }

        pub fn frame(&self) -> Result<(), JsValue> {
            self.state.borrow_mut().frame_at(perf_now())
        }

        pub fn bench_frames(&self, frames: u32) -> Result<String, JsValue> {
            let frame_count = frames.clamp(1, 600);
            let start = perf_now();
            for frame in 0..frame_count {
                self.state.borrow_mut().frame_at(start + frame as f64 * 16.666_667)?;
            }
            let total_ms = (perf_now() - start).max(0.0);
            let avg_ms = total_ms / frame_count as f64;
            let draws = self.state.borrow().last_draw_count();
            Ok(format!(
                "frames={frame_count};total_ms={total_ms:.3};avg_ms={avg_ms:.3};draws={draws}",
            ))
        }

        pub fn bench_frame_samples(
            &self,
            samples: u32,
            frames_per_sample: u32,
        ) -> Result<String, JsValue> {
            let sample_count = samples.clamp(1, 30);
            let frames = frames_per_sample.clamp(1, 120);
            let mut values = Vec::with_capacity(sample_count as usize);
            let mut timestamp = perf_now();

            for _sample in 0..sample_count {
                let start = perf_now();
                for _frame in 0..frames {
                    self.state.borrow_mut().frame_at(timestamp)?;
                    timestamp += 16.666_667;
                }
                values.push((perf_now() - start).max(0.0) / frames as f64);
            }

            values.sort_by(|a, b| a.total_cmp(b));
            let total_frames = sample_count.saturating_mul(frames);
            let avg_ms = average(&values);
            let p50_ms = percentile(&values, 0.50);
            let p95_ms = percentile(&values, 0.95);
            let p99_ms = percentile(&values, 0.99);
            let peak_ms = values.last().copied().unwrap_or(0.0);
            let draws = self.state.borrow().last_draw_count();
            Ok(format!(
            "samples={sample_count};frames_per_sample={frames};frames={total_frames};p50_ms={p50_ms:.3};p95_ms={p95_ms:.3};p99_ms={p99_ms:.3};peak_ms={peak_ms:.3};avg_ms={avg_ms:.3};draws={draws}",
         ))
        }

        pub fn set_scene(&self, scene_index: usize) {
            self.state.borrow_mut().router.set_scene(scene_index);
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

    fn request_next_frame(state: &Rc<RefCell<AppState>>) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let borrowed = state.borrow();
        let Some(raf) = borrowed.raf.as_ref() else {
            return;
        };
        let _ = window.request_animation_frame(raf.as_ref().unchecked_ref());
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
                    x,
                    y,
                    pressure: Some(pointer.pressure()),
                    tilt: None,
                    device,
                };
                state_for_event.borrow_mut().router.input_touch(&touch);
            } else {
                state_for_event.borrow_mut().router.input_pointer(
                    x,
                    y,
                    pointer.movement_x() as f32,
                    pointer.movement_y() as f32,
                    pointer.buttons() as u32,
                );
            }
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
                state_for_event.borrow_mut().router.input_pinch(x, y, -delta * 0.001);
            } else {
                state_for_event.borrow_mut().router.input_pointer(x, y, 0.0, -delta, 0);
            }
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
        }) as Box<dyn FnMut(Event)>);
        retain_listener(state, target, "compositionstart", start)?;

        let state_for_update = Rc::clone(state);
        let update = Closure::wrap(Box::new(move |event: Event| {
            let Some(composition) = event.dyn_ref::<CompositionEvent>() else {
                return;
            };
            let text = composition.data().unwrap_or_default();
            let end = text.chars().count() as u32;
            state_for_update.borrow_mut().router.input_set_composition(0, end, &text);
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
            let _ = state_for_event.borrow_mut().frame_at(perf_now());
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
    platform_smoke_report, start_oxide, start_oxide_async, webgpu_smoke_report, OxideWebApp,
};

#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub const fn host_web_requires_wasm32() -> &'static str {
    "oxide-host-web exports browser entry points only for wasm32"
}
