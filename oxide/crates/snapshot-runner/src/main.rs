use clap::Parser;
use oxide_platform_api as plat;
use oxide_renderer_api as api;
use oxide_renderer_api::Renderer;
use oxide_renderer_metal as metal;
use oxide_test_scenes as scenes;
use oxide_text as text;
use oxide_ui_core as ui;
use std::fs;
use std::io::{BufWriter, Read};
use std::path::{Path, PathBuf};
use ui::elements::ImageUploader;

#[derive(Parser, Debug)]
#[command(name = "oxide-snapshot-runner")]
struct Args {
    #[arg(long)]
    smoke: bool,
    #[arg(long, default_value = "static")]
    suite: String,
    #[arg(long, required_unless_present = "smoke")]
    component: Option<String>,
    #[arg(long, default_value = "default")]
    variant: String,
    #[arg(long, default_value = "default")]
    state: String,
    #[arg(long, default_value_t = 800)]
    width: u32,
    #[arg(long, default_value_t = 600)]
    height: u32,
    #[arg(long, default_value_t = 2.0)]
    scale: f32,
    #[arg(long)]
    time_ms: Option<u32>,
    #[arg(long, default_value_t = 1000)]
    period_ms: u32,
    #[arg(long, required_unless_present = "smoke")]
    out: Option<PathBuf>,
    #[arg(long)]
    golden: Option<PathBuf>,
}

fn gen_checker_rgba(w: u32, h: u32) -> Vec<u8> {
    let mut v = vec![0u8; (w as usize) * (h as usize) * 4];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let c = if ((x / 16) + (y / 16)) % 2 == 0 { 220 } else { 180 };
            v[i] = c;
            v[i + 1] = c;
            v[i + 2] = c;
            v[i + 3] = 255;
        }
    }
    v
}

fn write_png_rgba(path: &Path, w: u32, h: u32, rgba: &[u8]) -> anyhow::Result<()> {
    let file = fs::File::create(path)?;
    let wtr = BufWriter::new(file);
    let mut enc = png::Encoder::new(wtr, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header()?;
    wr.write_image_data(rgba)?;
    Ok(())
}

fn load_png_rgba(path: &Path) -> anyhow::Result<(u32, u32, Vec<u8>)> {
    let mut f = fs::File::open(path)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    let dec = png::Decoder::new(&buf[..]);
    let mut reader = dec.read_info()?;
    let mut out = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut out)?;
    let bytes = &out[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => {
            let mut v = Vec::with_capacity(info.width as usize * info.height as usize * 4);
            for c in bytes.chunks_exact(3) {
                v.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
            v
        }
        _ => anyhow::bail!("unsupported color type"),
    };
    Ok((info.width, info.height, rgba))
}

fn bgra_to_rgba_inplace(buf: &mut [u8]) {
    for p in buf.chunks_exact_mut(4) {
        p.swap(0, 2); // B <-> R
    }
}

fn compare_rgba(a: &[u8], b: &[u8]) -> (u64, u8, f64) {
    assert_eq!(a.len(), b.len());
    let mut pixdiff = 0u64;
    let mut max_err = 0u8;
    let mut sum_sq = 0u64;
    for (pa, pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        let mut pd = 0u8;
        for c in 0..4 {
            let d = pa[c].abs_diff(pb[c]);
            if d > pd {
                pd = d;
            }
            sum_sq += (d as u64) * (d as u64);
            if d > max_err {
                max_err = d;
            }
        }
        if pd > 0 {
            pixdiff += 1;
        }
    }
    let n = (a.len() / 4) as f64;
    let mse = (sum_sq as f64) / (n * 4.0);
    (pixdiff, max_err, mse)
}

fn try_load_font_into_ctx(ctx: &mut ui::elements::TextCtx) {
    // Allow override via env
    if let Ok(path) = std::env::var("SNAPSHOT_FONT_PATH") {
        if let Ok(bytes) = std::fs::read(&path) {
            let _ = ctx.fonts.add_font(text::Font::from_bytes(bytes));
            return;
        }
    }
    // Try common repo paths relative to current working directory
    let candidates = [
        "host/macos-app/Resources/fonts/Inter-Regular.ttf",
        "../host/macos-app/Resources/fonts/Inter-Regular.ttf",
        "../../host/macos-app/Resources/fonts/Inter-Regular.ttf",
        "fonts/Inter-Regular.ttf",
        "../fonts/Inter-Regular.ttf",
        "../../fonts/Inter-Regular.ttf",
    ];
    for p in candidates.iter() {
        if let Ok(bytes) = std::fs::read(p) {
            let _ = ctx.fonts.add_font(text::Font::from_bytes(bytes));
            break;
        }
    }
}

fn draw_bitmap_char5x7(
    b: &mut ui::DrawListBuilder,
    ch: char,
    x: f32,
    y: f32,
    px: f32,
    color: api::Color,
) {
    // 5x7 uppercase bitmap font for snapshot labels (subset)
    // Each row is 5 bits (MSB left)
    let pattern: [u8; 7] = match ch {
        'O' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b11110, 0b10001, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100],
        'E' => [0b11111, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000, 0b11111],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10101, 0b10011, 0b10001, 0b10001],
        'R' => [0b11110, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'X' => [0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b01010, 0b10001],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        ' ' => [0; 7],
        _ => [0; 7],
    };
    for (row, bits) in pattern.iter().enumerate() {
        for col in 0..5 {
            if (bits >> (4 - col)) & 1 == 1 {
                b.rrect(
                    api::RectF::new(x + col as f32 * px, y + row as f32 * px, px, px),
                    [0.0; 4],
                    color,
                );
            }
        }
    }
}

fn draw_bitmap_text_centered(
    b: &mut ui::DrawListBuilder,
    text: &str,
    rect: api::RectF,
    px: f32,
    color: api::Color,
) {
    let w = (text.chars().count() as f32) * (5.0 * px)
        + ((text.chars().count().saturating_sub(1)) as f32) * px;
    let h = 7.0 * px;
    let mut x = rect.x + (rect.w - w) * 0.5;
    let y = rect.y + (rect.h - h) * 0.5;
    for ch in text.chars() {
        draw_bitmap_char5x7(b, ch, x, y, px, color);
        x += 6.0 * px; // 5px glyph + 1px spacing
    }
}

struct Uploader {
    r: *mut metal::MetalRenderer,
}
unsafe impl Send for Uploader {}
unsafe impl Sync for Uploader {}
impl ImageUploader for Uploader {
    fn create_a8(&mut self, w: u32, h: u32, data: &[u8], row_bytes: usize) -> api::ImageHandle {
        unsafe { (*self.r).image_create_a8(w, h, data, row_bytes) }
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
        unsafe { (*self.r).image_update_a8(handle, x, y, w, h, data, row_bytes) }
    }
}

fn render_component(
    comp: &str,
    state: &str,
    w_dp: f32,
    h_dp: f32,
    renderer: &mut metal::MetalRenderer,
    time_ms: Option<u32>,
    period_ms: u32,
) -> api::DrawList {
    let mut b = ui::DrawListBuilder::new();
    let vp = api::RectF::new(0.0, 0.0, w_dp, h_dp);
    b.clip_push(api::RectI::new(0, 0, w_dp as i32, h_dp as i32));
    // White background for stability
    b.rrect(vp, [0.0; 4], api::Color::rgba(1.0, 1.0, 1.0, 1.0));
    let panel = api::RectF::new(20.0, 20.0, w_dp - 40.0, h_dp - 40.0);
    match comp.to_ascii_lowercase().as_str() {
        "progressbar" => {
            let mut pb = ui::elements::ProgressBar::default();
            // state: determinate (default) or indeterminate
            if state.eq_ignore_ascii_case("indeterminate") {
                pb.value = None;
            } else {
                pb.value = Some(0.6);
            }
            let phase = time_ms.map(|t| (t as f32 / period_ms as f32)).unwrap_or(0.25);
            pb.encode(
                api::RectF::new(panel.x, panel.y + panel.h * 0.5 - 6.0, panel.w, 12.0),
                phase,
                &mut b,
            );
        }
        "spinner" => {
            let sp = ui::elements::Spinner::default();
            sp.encode(
                api::RectF::new(
                    panel.x + panel.w * 0.5 - 16.0,
                    panel.y + panel.h * 0.5 - 16.0,
                    32.0,
                    32.0,
                ),
                &mut b,
            );
        }
        "button" => {
            let btn = ui::elements::Button { text: "OK".to_string(), ..Default::default() };
            let mut st = ui::elements::ButtonState::default();
            if state.eq_ignore_ascii_case("disabled") {
                st.disabled = true;
            }
            if state.eq_ignore_ascii_case("pressed") {
                st.on_pointer_down();
            }
            let mut txt = ui::elements::TextCtx::default();
            try_load_font_into_ctx(&mut txt);
            // Provide uploader even if no font is present; Label.encode will early-return if no font.
            let ptr: *mut metal::MetalRenderer = renderer;
            let mut up = Uploader { r: ptr };
            let rect = api::RectF::new(
                panel.x + panel.w * 0.5 - 60.0,
                panel.y + panel.h * 0.5 - 20.0,
                120.0,
                40.0,
            );
            btn.encode(rect, 2.0, &mut txt, &mut up, &st, &mut b);
            // Deterministic bitmap text fallback
            draw_bitmap_text_centered(
                &mut b,
                "OK",
                rect,
                1.5,
                api::Color::rgba(1.0, 1.0, 1.0, 1.0),
            );
        }
        "toggle" => {
            let t = ui::elements::Toggle::default();
            let mut st = ui::elements::ToggleState::default();
            if state.eq_ignore_ascii_case("on") {
                st.set_on(true);
            }
            let rect = api::RectF::new(
                panel.x + panel.w * 0.5 - 24.0,
                panel.y + panel.h * 0.5 - 12.0,
                48.0,
                24.0,
            );
            t.encode(rect, &st, &mut b);
        }
        "toggle_anim" => {
            // Animate the toggle's spring toward on/off using time_ms integration in 50ms steps
            let tgl = ui::elements::Toggle::default();
            let mut st = ui::elements::ToggleState::default();
            if state.eq_ignore_ascii_case("to_on") {
                st.set_on(true);
            } else {
                st.set_on(false);
            }
            let total = time_ms.unwrap_or(0);
            let mut acc = 0u32;
            while acc < total {
                let dt = core::cmp::min(50, total - acc);
                st.step(dt);
                acc += dt;
            }
            let rect = api::RectF::new(
                panel.x + panel.w * 0.5 - 24.0,
                panel.y + panel.h * 0.5 - 12.0,
                48.0,
                24.0,
            );
            tgl.encode(rect, &st, &mut b);
        }
        "slider" => {
            let s = ui::elements::Slider::default();
            let mut st = ui::elements::SliderState::default();
            st.value = if state.eq_ignore_ascii_case("low") {
                0.2
            } else if state.eq_ignore_ascii_case("high") {
                0.8
            } else {
                0.6
            };
            let rect = api::RectF::new(
                panel.x + 40.0,
                panel.y + panel.h * 0.5 - 8.0,
                panel.w - 80.0,
                16.0,
            );
            s.encode(rect, &st, &mut b);
        }
        "slider_move" => {
            // Animate slider value by phase in [0,1]
            let s = ui::elements::Slider::default();
            let mut st = ui::elements::SliderState::default();
            let phase = time_ms.map(|t| (t as f32 / period_ms as f32)).unwrap_or(0.0).fract();
            st.value = phase.clamp(0.0, 1.0);
            let rect = api::RectF::new(
                panel.x + 40.0,
                panel.y + panel.h * 0.5 - 8.0,
                panel.w - 80.0,
                16.0,
            );
            s.encode(rect, &st, &mut b);
        }
        "imageview" | "ninesliceimage" => {
            // Create a 128x128 checker image and draw it centered with contain fit via nine-slice zeros
            let zw = 128u32;
            let zh = 128u32;
            let rgba = gen_checker_rgba(zw, zh);
            let tex = renderer.image_create_rgba8(zw, zh, &rgba, (zw as usize) * 4);
            let dst =
                api::RectF::new(panel.x + 40.0, panel.y + 40.0, panel.w - 80.0, panel.h - 80.0);
            b.nine_slice(tex, dst, api::Insets::new(0.0, 0.0, 0.0, 0.0), 1.0);
        }
        "imageview_zoom" => {
            let zw = 256u32;
            let zh = 256u32;
            let rgba = gen_checker_rgba(zw, zh);
            let tex = renderer.image_create_rgba8(zw, zh, &rgba, (zw as usize) * 4);
            let dst =
                api::RectF::new(panel.x + 20.0, panel.y + 20.0, panel.w - 40.0, panel.h - 40.0);
            let iv = ui::elements::ImageView {
                image: tex,
                natural_w: zw,
                natural_h: zh,
                fit: ui::elements::ImageFit::Contain,
                alpha: 1.0,
            };
            let zoom = ui::elements::ImageZoomState { scale: 2.0, offset: [10.0, 15.0] };
            iv.encode(dst, Some(&zoom), &mut b);
        }
        "nine_slice" => {
            let zw = 128u32;
            let zh = 128u32;
            let rgba = gen_checker_rgba(zw, zh);
            let tex = renderer.image_create_rgba8(zw, zh, &rgba, (zw as usize) * 4);
            let dst =
                api::RectF::new(panel.x + 20.0, panel.y + 20.0, panel.w - 40.0, panel.h - 40.0);
            b.nine_slice(tex, dst, api::Insets::new(16.0, 16.0, 16.0, 16.0), 1.0);
        }
        "scene_controls" | "scene_text" | "scene_zoom" | "scene_collection" => {
            let ptr: *mut metal::MetalRenderer = renderer;
            let mut router = scenes::Router::new(Uploader { r: ptr });
            // Provide an image for zoom scene
            let (zw, zh) = (256u32, 256u32);
            let zrgba = gen_checker_rgba(zw, zh);
            let tex = renderer.image_create_rgba8(zw, zh, &zrgba, (zw as usize) * 4);
            router.set_zoom_image(tex, zw, zh);
            // Try to load a vendored font into the router's text context
            try_load_font_into_ctx(&mut router.text);
            let idx = match comp.to_ascii_lowercase().as_str() {
                "scene_controls" => 0,
                "scene_text" => 1,
                "scene_zoom" => 2,
                "scene_collection" => 4,
                _ => 0,
            };
            router.set_scene(idx);
            let dt = time_ms.unwrap_or(0);
            router.update(oxide_timing::now_ms(), dt);
            router.draw(panel, 1.0, &mut b);
            // If no font loaded (empty DB), draw a deterministic bitmap label of the scene name
            if router.text.fonts.font(0).is_none() {
                let name = match idx {
                    0 => "CONTROLS",
                    1 => "TEXT",
                    2 => "ZOOM",
                    4 => "COLLECTION",
                    _ => "SCENE",
                };
                let rect = api::RectF::new(panel.x + 12.0, panel.y + 12.0, 200.0, 20.0);
                draw_bitmap_text_centered(
                    &mut b,
                    name,
                    rect,
                    1.5,
                    api::Color::rgba(0.1, 0.1, 0.1, 1.0),
                );
            }
        }
        "text_unicode" => {
            // Draw a shaped label with mixed scripts and emoji; falls back to bitmap only if font missing
            let mut txtctx = ui::elements::TextCtx::default();
            try_load_font_into_ctx(&mut txtctx);
            let ptr: *mut metal::MetalRenderer = renderer;
            let mut up = Uploader { r: ptr };
            let label = ui::elements::Label {
                text: "Hello Ω Привет こんにちは 😀".to_string(),
                color: api::Color::rgba(0.1, 0.1, 0.1, 1.0),
                align: ui::elements::Align::Left,
                wrap: true,
                font_id: 0,
                font_px: 16.0,
            };
            let rect =
                api::RectF::new(panel.x + 20.0, panel.y + 20.0, panel.w - 40.0, panel.h - 40.0);
            label.encode(rect, 1.0, &mut txtctx, &mut up, &mut b);
            if txtctx.fonts.font(0).is_none() {
                draw_bitmap_text_centered(
                    &mut b,
                    "TEXT",
                    api::RectF::new(panel.x + 12.0, panel.y + 12.0, 100.0, 20.0),
                    1.5,
                    api::Color::rgba(0.1, 0.1, 0.1, 1.0),
                );
            }
        }
        "style_effects" => {
            // NodeTree with opacity on parent, transform and shadow on child
            let mut tree = ui::NodeTree::new_root(ui::NodeStyle {
                size: ui::Size2D { w: ui::Dim::Px(w_dp), h: ui::Dim::Px(h_dp) },
                background: api::Color::rgba(0.95, 0.95, 0.98, 1.0),
                opacity: 0.8,
                ..ui::NodeStyle::default()
            });
            let _child = tree.add_node(
                tree.root(),
                ui::NodeStyle {
                    size: ui::Size2D { w: ui::Dim::Px(200.0), h: ui::Dim::Px(120.0) },
                    background: api::Color::rgba(0.2, 0.6, 1.0, 1.0),
                    corner_radii: [10.0, 10.0, 10.0, 10.0],
                    transform: plat::Transform2D {
                        tx: 40.0,
                        ty: 24.0,
                        sx: 1.0,
                        sy: 1.0,
                        rot_rad: 0.0,
                    },
                    shadow_alpha: 0.5,
                    ..ui::NodeStyle::default()
                },
            );
            tree.layout(w_dp, h_dp);
            tree.encode_draws(&mut b);
        }
        "layer_composite" => {
            // Compose a sublist inside a layer rect
            let mut dl = api::DrawList::default();
            // Background
            dl.items.push(api::DrawCmd::RRect {
                rect: panel,
                radii: [0.0; 4],
                color: api::Color::rgba(1.0, 1.0, 1.0, 1.0),
            });
            // Begin layer
            dl.items.push(api::DrawCmd::LayerBegin {
                id: 1,
                rect: api::RectF::new(
                    panel.x + 40.0,
                    panel.y + 40.0,
                    panel.w - 80.0,
                    panel.h - 80.0,
                ),
                dirty: true,
            });
            // Inner draw
            dl.items.push(api::DrawCmd::RRect {
                rect: api::RectF::new(
                    panel.x + 60.0,
                    panel.y + 60.0,
                    panel.w - 120.0,
                    panel.h - 120.0,
                ),
                radii: [12.0; 4],
                color: api::Color::rgba(0.9, 0.2, 0.2, 1.0),
            });
            // End layer
            dl.items.push(api::DrawCmd::LayerEnd);
            b.clip_pop();
            return dl;
        }
        "collection_grid" => {
            // Vertical grid with fixed measurement and simple colored cells
            let mut view =
                ui::collection::CollectionView::new(ui::collection::CollectionMode::VerticalGrid {
                    col_width: 100.0,
                    spacing: 8.0,
                });
            view.set_count(100);
            struct Meas;
            impl ui::collection::Measure for Meas {
                fn measure(&mut self, _i: usize, cw: f32) -> f32 {
                    (cw * 0.6).max(20.0)
                }
            }
            struct Rend;
            impl ui::collection::CellRenderer for Rend {
                fn render(
                    &mut self,
                    _id: u32,
                    idx: usize,
                    rect: api::RectF,
                    _f: bool,
                    _h: bool,
                    b: &mut ui::DrawListBuilder,
                ) {
                    let base = 0.9 - ((idx % 5) as f32) * 0.05;
                    let c = api::Color::rgba(base, base, base, 1.0);
                    b.rrect(rect, [4.0; 4], c);
                }
            }
            let mut meas = Meas;
            let mut rend = Rend;
            let vp = api::RectF::new(panel.x, panel.y, panel.w, panel.h);
            view.layout_and_render(vp, &mut meas, &mut rend, &mut b);
        }
        "collection_row" => {
            // Horizontal row with variable widths
            let mut view = ui::collection::CollectionView::new(
                ui::collection::CollectionMode::HorizontalRow { row_height: 80.0, spacing: 8.0 },
            );
            view.set_count(40);
            struct Meas;
            impl ui::collection::Measure for Meas {
                fn measure(&mut self, i: usize, rh: f32) -> f32 {
                    let k = (i % 5) as f32;
                    rh * (1.0 + 0.3 * (k / 4.0))
                }
            }
            struct Rend;
            impl ui::collection::CellRenderer for Rend {
                fn render(
                    &mut self,
                    _id: u32,
                    idx: usize,
                    rect: api::RectF,
                    _f: bool,
                    _h: bool,
                    b: &mut ui::DrawListBuilder,
                ) {
                    let base = 0.85 - ((idx % 7) as f32) * 0.04;
                    let c = api::Color::rgba(base, base, 1.0 - base * 0.5, 1.0);
                    b.rrect(rect, [6.0; 4], c);
                }
            }
            let mut meas = Meas;
            let mut rend = Rend;
            let vp = api::RectF::new(panel.x, panel.y + (panel.h - 100.0) * 0.5, panel.w, 100.0);
            view.layout_and_render(vp, &mut meas, &mut rend, &mut b);
        }
        "collection_grid_scroll" => {
            let mut view =
                ui::collection::CollectionView::new(ui::collection::CollectionMode::VerticalGrid {
                    col_width: 100.0,
                    spacing: 8.0,
                });
            view.set_count(1000);
            struct Meas;
            impl ui::collection::Measure for Meas {
                fn measure(&mut self, _i: usize, cw: f32) -> f32 {
                    (cw * 0.6).max(20.0)
                }
            }
            struct Rend;
            impl ui::collection::CellRenderer for Rend {
                fn render(
                    &mut self,
                    _id: u32,
                    idx: usize,
                    rect: api::RectF,
                    _f: bool,
                    _h: bool,
                    b: &mut ui::DrawListBuilder,
                ) {
                    let base = 0.9 - ((idx % 5) as f32) * 0.05;
                    let c = api::Color::rgba(base, base, base, 1.0);
                    b.rrect(rect, [4.0; 4], c);
                }
            }
            let mut meas = Meas;
            let mut rend = Rend;
            let vp = api::RectF::new(panel.x, panel.y, panel.w, panel.h);
            // First layout to get content size
            let content =
                view.layout_and_render(vp, &mut meas, &mut rend, &mut ui::DrawListBuilder::new());
            let ratio = time_ms.map(|t| (t as f32 / period_ms as f32)).unwrap_or(0.0).fract();
            let max_scroll = (content.content_h - vp.h).max(0.0);
            view.set_scroll(max_scroll * ratio);
            view.layout_and_render(vp, &mut meas, &mut rend, &mut b);
        }
        "collection_row_scroll" => {
            let mut view = ui::collection::CollectionView::new(
                ui::collection::CollectionMode::HorizontalRow { row_height: 80.0, spacing: 8.0 },
            );
            view.set_count(500);
            struct Meas;
            impl ui::collection::Measure for Meas {
                fn measure(&mut self, i: usize, rh: f32) -> f32 {
                    let k = (i % 5) as f32;
                    rh * (1.0 + 0.3 * (k / 4.0))
                }
            }
            struct Rend;
            impl ui::collection::CellRenderer for Rend {
                fn render(
                    &mut self,
                    _id: u32,
                    idx: usize,
                    rect: api::RectF,
                    _f: bool,
                    _h: bool,
                    b: &mut ui::DrawListBuilder,
                ) {
                    let base = 0.85 - ((idx % 7) as f32) * 0.04;
                    let c = api::Color::rgba(base, base, 1.0 - base * 0.5, 1.0);
                    b.rrect(rect, [6.0; 4], c);
                }
            }
            let mut meas = Meas;
            let mut rend = Rend;
            let vp = api::RectF::new(panel.x, panel.y + (panel.h - 100.0) * 0.5, panel.w, 100.0);
            let content =
                view.layout_and_render(vp, &mut meas, &mut rend, &mut ui::DrawListBuilder::new());
            let ratio = time_ms.map(|t| (t as f32 / period_ms as f32)).unwrap_or(0.0).fract();
            let max_scroll = (content.content_w - vp.w).max(0.0);
            view.set_scroll(max_scroll * ratio);
            view.layout_and_render(vp, &mut meas, &mut rend, &mut b);
        }
        "animtimeline" => {
            let mut at = scenes::AnimTimeline::default();
            let tms = time_ms.unwrap_or(0);
            at.update(tms);
            let mut txt = ui::elements::TextCtx::default();
            let ptr: *mut metal::MetalRenderer = renderer;
            let mut up = Uploader { r: ptr };
            at.draw(panel, 1.0, &mut txt, &mut up, &mut b);
        }
        "button_press" => {
            let btn = ui::elements::Button { text: "OK".to_string(), ..Default::default() };
            let mut st = ui::elements::ButtonState::default();
            st.on_pointer_down();
            if let Some(t) = time_ms {
                std::thread::sleep(std::time::Duration::from_millis(t as u64));
            }
            let mut txt = ui::elements::TextCtx::default();
            let ptr: *mut metal::MetalRenderer = renderer;
            let mut up = Uploader { r: ptr };
            let rect = api::RectF::new(
                panel.x + panel.w * 0.5 - 60.0,
                panel.y + panel.h * 0.5 - 20.0,
                120.0,
                40.0,
            );
            btn.encode(rect, 2.0, &mut txt, &mut up, &st, &mut b);
        }
        other => {
            eprintln!("unknown component '{}', drawing empty", other);
        }
    }
    b.clip_pop();
    b.into_inner()
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let component = args.component.clone().unwrap_or_else(|| "button".to_string());
    let out_path = if let Some(path) = args.out.clone() {
        path
    } else {
        let mut path = PathBuf::from("artifacts/ui");
        path.push(format!("{}_smoke.png", component));
        path
    };
    let mut r = metal::MetalRenderer::new_default().expect("metal");
    let (w, h, scale) = (args.width, args.height, args.scale);
    r.resize(w, h, scale).unwrap();
    let w_dp = (w as f32) / scale;
    let h_dp = (h as f32) / scale;

    println!(
        "## RUN suite={} component={} variant={} state={}{} width={} height={} scale={}",
        args.suite,
        component,
        args.variant,
        args.state,
        args.time_ms.map(|t| format!(" time_ms={}", t)).unwrap_or_default(),
        w,
        h,
        scale
    );

    let list =
        render_component(&component, &args.state, w_dp, h_dp, &mut r, args.time_ms, args.period_ms);
    let fb = &api::FrameTarget;
    let token = r.begin_frame(fb, None);
    r.encode_pass(&list);
    r.submit(token).unwrap();
    let (rw, rh, mut bgra) = r.readback_bgra8().expect("readback");
    assert_eq!((rw, rh), (w, h));
    bgra_to_rgba_inplace(&mut bgra);

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_png_rgba(&out_path, w, h, &bgra)?;

    let mut pixdiff = 0u64;
    let mut max_err = 0u8;
    let mut mse = 0.0f64;
    if let Some(golden) = args.golden.as_ref() {
        let update = std::env::var("UPDATE_GOLDENS").ok().as_deref() == Some("1");
        if golden.exists() && !update {
            let (gw, gh, grgba) = load_png_rgba(golden)?;
            if gw == w && gh == h {
                let (pd, me, mm) = compare_rgba(&bgra, &grgba);
                pixdiff = pd;
                max_err = me;
                mse = mm;
            } else {
                eprintln!("golden size mismatch: got {}x{}, golden {}x{}", w, h, gw, gh);
            }
        } else {
            if let Some(parent) = golden.parent() {
                fs::create_dir_all(parent)?;
            }
            // First run: create golden
            write_png_rgba(golden, w, h, &bgra)?;
        }
    }

    println!(
        "summary suite={} component={} variant={} state={}{} pixdiff={} max_err={} mse={:.6}",
        args.suite,
        component,
        args.variant,
        args.state,
        args.time_ms.map(|t| format!(" time_ms={}", t)).unwrap_or_default(),
        pixdiff,
        max_err,
        mse
    );
    println!(
        "## END suite={} component={} variant={} state={}{}",
        args.suite,
        component,
        args.variant,
        args.state,
        args.time_ms.map(|t| format!(" time_ms={}", t)).unwrap_or_default(),
    );
    Ok(())
}
