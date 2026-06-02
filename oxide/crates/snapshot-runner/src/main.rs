use clap::Parser;
use oxide_platform_api as plat;
use oxide_renderer_api as api;
use oxide_renderer_api::Renderer;
use oxide_renderer_metal as metal;
use oxide_renderer_metal::scene3d::{self, Instance3d, Mesh3dData, Pass3d, Vertex3d};
use oxide_test_scenes as scenes;
use oxide_text as text;
use oxide_ui_core as ui;
use std::fs;
use std::io::{BufWriter, Read};
use std::path::{Path, PathBuf};
use ui::elements::ImageUploader;

const DEFAULT_SNAPSHOT_FONT: &[u8] = include_bytes!("../../ui-core/assets/Asap-Regular.ttf");
const CJK_SNAPSHOT_FONT: &[u8] = include_bytes!("../../text/tests/fixtures/test_text_cjk.ttf");

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
    #[arg(long)]
    allow_mismatch: bool,
    #[arg(long, default_value_t = 0)]
    pixel_tolerance: u64,
    #[arg(long, default_value_t = 0)]
    max_error_tolerance: u8,
    #[arg(long, default_value_t = 0.0)]
    mse_tolerance: f64,
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
    if ctx.fonts.font(0).is_none() {
        let _ = ctx.fonts.add_font(text::Font::from_bytes(DEFAULT_SNAPSHOT_FONT.to_vec()));
    }
}

fn add_cjk_fallback_font(ctx: &mut ui::elements::TextCtx) -> usize {
    let cjk_id = ctx.fonts.add_font(text::Font::from_bytes(CJK_SNAPSHOT_FONT.to_vec()));
    ctx.set_fallback_fonts(&[cjk_id]);
    cjk_id
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
        'F' => [0b11111, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110],
        'I' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10101, 0b10011, 0b10001, 0b10001],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'R' => [0b11110, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
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

fn mat4_identity() -> scene3d::Mat4 {
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
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

fn is_direct_component(component: &str) -> bool {
    matches!(
        component.to_ascii_lowercase().as_str(),
        "scene3d_mixed"
            | "scene3d_bloom"
            | "scene3d_depth_stack"
            | "scene3d_viewport_clip"
            | "scene3d_material_cull"
            | "scene3d_blend_modes"
            | "id_mask_compositor"
            | "id_mask_compositor_city_ids"
            | "id_mask_compositor_neighborhood_ids"
            | "id_mask_compositor_seams"
    )
}

fn encode_scene3d_mixed(renderer: &mut metal::MetalRenderer) -> anyhow::Result<()> {
    let fill_vertices = [
        Vertex3d { position: [-0.70, -0.55, 0.10] },
        Vertex3d { position: [0.10, -0.60, 0.10] },
        Vertex3d { position: [-0.45, 0.15, 0.10] },
    ];
    let fill_indices = [0_u32, 1, 2];
    let fill = renderer.mesh3d_create(&Mesh3dData {
        vertices: &fill_vertices,
        indices: &fill_indices,
        topology: scene3d::MeshTopology::Triangles,
    })?;

    let line_vertices = [
        Vertex3d { position: [-0.85, 0.0, 0.0] },
        Vertex3d { position: [0.85, 0.0, 0.0] },
        Vertex3d { position: [0.0, -0.85, 0.0] },
        Vertex3d { position: [0.0, 0.85, 0.0] },
    ];
    let line_indices = [0_u32, 1, 2, 3];
    let lines = renderer.mesh3d_create(&Mesh3dData {
        vertices: &line_vertices,
        indices: &line_indices,
        topology: scene3d::MeshTopology::Lines,
    })?;

    let identity = mat4_identity();
    let mut line_instance =
        Instance3d::new(lines, identity, api::Color::rgba(0.98, 0.30, 0.46, 1.0));
    line_instance.cull = scene3d::CullMode3d::None;
    line_instance.depth_write = false;
    let instances =
        [Instance3d::new(fill, identity, api::Color::rgba(0.18, 0.72, 1.0, 1.0)), line_instance];
    let scene = Pass3d {
        viewport: None,
        clear_color: Some(api::Color::rgba(0.08, 0.09, 0.13, 1.0)),
        clear_depth: true,
        view_proj: identity,
        instances: &instances,
        bloom: None,
    };
    renderer.encode_scene3d(&scene)?;

    let mut overlay = api::DrawList::default();
    overlay.items.push(api::DrawCmd::RRect {
        rect: api::RectF::new(10.0, 10.0, 42.0, 20.0),
        radii: [4.0; 4],
        color: api::Color::rgba(1.0, 1.0, 1.0, 1.0),
    });
    renderer.encode_pass(&overlay);
    Ok(())
}

fn encode_scene3d_bloom(renderer: &mut metal::MetalRenderer) -> anyhow::Result<()> {
    let base_vertices = [
        Vertex3d { position: [-0.78, -0.55, 0.18] },
        Vertex3d { position: [0.52, -0.50, 0.18] },
        Vertex3d { position: [-0.12, 0.48, 0.18] },
    ];
    let tri_indices = [0_u32, 1, 2];
    let base = renderer.mesh3d_create(&Mesh3dData {
        vertices: &base_vertices,
        indices: &tri_indices,
        topology: scene3d::MeshTopology::Triangles,
    })?;

    let glow_vertices = [
        Vertex3d { position: [-0.22, -0.18, 0.05] },
        Vertex3d { position: [0.46, -0.12, 0.05] },
        Vertex3d { position: [0.12, 0.46, 0.05] },
    ];
    let glow = renderer.mesh3d_create(&Mesh3dData {
        vertices: &glow_vertices,
        indices: &tri_indices,
        topology: scene3d::MeshTopology::Triangles,
    })?;

    let identity = mat4_identity();
    let base_instance = Instance3d::new(base, identity, api::Color::rgba(0.12, 0.30, 0.82, 1.0));
    let mut glow_instance =
        Instance3d::new(glow, identity, api::Color::rgba(1.0, 0.42, 0.18, 0.92));
    glow_instance.cull = scene3d::CullMode3d::None;
    glow_instance.depth_write = false;
    glow_instance.material = scene3d::Material3d::Emissive;
    glow_instance.params = [2.7, 0.0, 0.0, 0.0];

    let instances = [base_instance, glow_instance];
    let emissive = [glow_instance];
    let bloom_layers = [
        scene3d::BloomLayer3d { sigma_px: 5.0, strength: 0.55 },
        scene3d::BloomLayer3d { sigma_px: 14.0, strength: 0.26 },
    ];
    let scene = Pass3d {
        viewport: None,
        clear_color: Some(api::Color::rgba(0.02, 0.02, 0.05, 1.0)),
        clear_depth: true,
        view_proj: identity,
        instances: &instances,
        bloom: Some(scene3d::Bloom3d {
            emissive_instances: &emissive,
            layers: &bloom_layers,
            downsample_divisor: 2,
        }),
    };
    renderer.encode_scene3d(&scene)?;

    let mut overlay = api::DrawList::default();
    overlay.items.push(api::DrawCmd::RRect {
        rect: api::RectF::new(18.0, 154.0, 54.0, 18.0),
        radii: [5.0; 4],
        color: api::Color::rgba(0.96, 0.98, 1.0, 1.0),
    });
    renderer.encode_pass(&overlay);
    Ok(())
}

fn encode_scene3d_depth_stack(renderer: &mut metal::MetalRenderer) -> anyhow::Result<()> {
    let back_vertices = [
        Vertex3d { position: [-0.72, -0.58, 0.42] },
        Vertex3d { position: [0.66, -0.50, 0.42] },
        Vertex3d { position: [-0.04, 0.62, 0.42] },
    ];
    let front_vertices = [
        Vertex3d { position: [-0.50, -0.36, 0.10] },
        Vertex3d { position: [0.42, -0.30, 0.10] },
        Vertex3d { position: [-0.08, 0.42, 0.10] },
    ];
    let tri_indices = [0_u32, 1, 2];
    let back = renderer.mesh3d_create(&Mesh3dData {
        vertices: &back_vertices,
        indices: &tri_indices,
        topology: scene3d::MeshTopology::Triangles,
    })?;
    let front = renderer.mesh3d_create(&Mesh3dData {
        vertices: &front_vertices,
        indices: &tri_indices,
        topology: scene3d::MeshTopology::Triangles,
    })?;

    let identity = mat4_identity();
    let instances = [
        Instance3d::new(back, identity, api::Color::rgba(0.15, 0.33, 0.85, 1.0)),
        Instance3d::new(front, identity, api::Color::rgba(1.0, 0.68, 0.18, 1.0)),
    ];
    let scene = Pass3d {
        viewport: None,
        clear_color: Some(api::Color::rgba(0.05, 0.06, 0.09, 1.0)),
        clear_depth: true,
        view_proj: identity,
        instances: &instances,
        bloom: None,
    };
    renderer.encode_scene3d(&scene)?;

    let mut overlay = api::DrawList::default();
    overlay.items.push(api::DrawCmd::RRect {
        rect: api::RectF::new(132.0, 16.0, 42.0, 18.0),
        radii: [4.0; 4],
        color: api::Color::rgba(0.92, 0.96, 1.0, 1.0),
    });
    renderer.encode_pass(&overlay);
    Ok(())
}

fn encode_scene3d_viewport_clip(
    renderer: &mut metal::MetalRenderer,
    w_dp: f32,
    h_dp: f32,
) -> anyhow::Result<()> {
    let wide_vertices = [
        Vertex3d { position: [-1.35, -1.05, 0.14] },
        Vertex3d { position: [1.30, -0.92, 0.14] },
        Vertex3d { position: [-0.04, 1.22, 0.14] },
    ];
    let line_vertices = [
        Vertex3d { position: [-1.0, 0.0, 0.05] },
        Vertex3d { position: [1.0, 0.0, 0.05] },
        Vertex3d { position: [0.0, -1.0, 0.05] },
        Vertex3d { position: [0.0, 1.0, 0.05] },
    ];
    let tri_indices = [0_u32, 1, 2];
    let line_indices = [0_u32, 1, 2, 3];
    let wide = renderer.mesh3d_create(&Mesh3dData {
        vertices: &wide_vertices,
        indices: &tri_indices,
        topology: scene3d::MeshTopology::Triangles,
    })?;
    let lines = renderer.mesh3d_create(&Mesh3dData {
        vertices: &line_vertices,
        indices: &line_indices,
        topology: scene3d::MeshTopology::Lines,
    })?;

    let identity = mat4_identity();
    let mut line_instance =
        Instance3d::new(lines, identity, api::Color::rgba(1.0, 0.86, 0.26, 1.0));
    line_instance.cull = scene3d::CullMode3d::None;
    line_instance.depth_write = false;
    let instances =
        [Instance3d::new(wide, identity, api::Color::rgba(0.22, 0.78, 0.52, 1.0)), line_instance];
    let viewport = api::RectF::new(w_dp * 0.23, h_dp * 0.20, w_dp * 0.54, h_dp * 0.56);
    let scene = Pass3d {
        viewport: Some(viewport),
        clear_color: Some(api::Color::rgba(0.04, 0.05, 0.08, 1.0)),
        clear_depth: true,
        view_proj: identity,
        instances: &instances,
        bloom: None,
    };
    renderer.encode_scene3d(&scene)?;

    let mut overlay = api::DrawList::default();
    let border = api::Color::rgba(0.92, 0.96, 1.0, 1.0);
    overlay.items.push(api::DrawCmd::RRect {
        rect: api::RectF::new(viewport.x - 2.0, viewport.y - 2.0, viewport.w + 4.0, 2.0),
        radii: [0.0; 4],
        color: border,
    });
    overlay.items.push(api::DrawCmd::RRect {
        rect: api::RectF::new(viewport.x - 2.0, viewport.y + viewport.h, viewport.w + 4.0, 2.0),
        radii: [0.0; 4],
        color: border,
    });
    overlay.items.push(api::DrawCmd::RRect {
        rect: api::RectF::new(viewport.x - 2.0, viewport.y - 2.0, 2.0, viewport.h + 4.0),
        radii: [0.0; 4],
        color: border,
    });
    overlay.items.push(api::DrawCmd::RRect {
        rect: api::RectF::new(viewport.x + viewport.w, viewport.y - 2.0, 2.0, viewport.h + 4.0),
        radii: [0.0; 4],
        color: border,
    });
    renderer.encode_pass(&overlay);
    Ok(())
}

fn encode_scene3d_material_cull(renderer: &mut metal::MetalRenderer) -> anyhow::Result<()> {
    let shaded_vertices = [
        Vertex3d { position: [-0.88, -0.54, 0.16] },
        Vertex3d { position: [-0.28, -0.50, 0.16] },
        Vertex3d { position: [-0.58, 0.38, 0.16] },
    ];
    let emissive_vertices = [
        Vertex3d { position: [-0.24, -0.52, 0.12] },
        Vertex3d { position: [0.28, -0.50, 0.12] },
        Vertex3d { position: [0.02, 0.40, 0.12] },
    ];
    let backface_vertices = [
        Vertex3d { position: [0.38, -0.54, 0.14] },
        Vertex3d { position: [0.88, -0.50, 0.14] },
        Vertex3d { position: [0.62, 0.36, 0.14] },
    ];
    let front_indices = [0_u32, 1, 2];
    let back_indices = [0_u32, 2, 1];
    let shaded = renderer.mesh3d_create(&Mesh3dData {
        vertices: &shaded_vertices,
        indices: &front_indices,
        topology: scene3d::MeshTopology::Triangles,
    })?;
    let emissive = renderer.mesh3d_create(&Mesh3dData {
        vertices: &emissive_vertices,
        indices: &front_indices,
        topology: scene3d::MeshTopology::Triangles,
    })?;
    let backface = renderer.mesh3d_create(&Mesh3dData {
        vertices: &backface_vertices,
        indices: &back_indices,
        topology: scene3d::MeshTopology::Triangles,
    })?;

    let identity = mat4_identity();
    let mut shaded_instance =
        Instance3d::new(shaded, identity, api::Color::rgba(0.18, 0.74, 0.48, 1.0));
    shaded_instance.material = scene3d::Material3d::NeighborhoodFill;
    shaded_instance.params = [0.64, -0.58, -0.12, 0.58];
    let mut emissive_instance =
        Instance3d::new(emissive, identity, api::Color::rgba(1.0, 0.46, 0.12, 1.0));
    emissive_instance.cull = scene3d::CullMode3d::None;
    emissive_instance.depth_test = false;
    emissive_instance.depth_write = false;
    emissive_instance.material = scene3d::Material3d::Emissive;
    emissive_instance.params = [2.2, 0.0, 0.0, 0.0];
    let mut visible_backface =
        Instance3d::new(backface, identity, api::Color::rgba(0.28, 0.55, 1.0, 1.0));
    visible_backface.cull = scene3d::CullMode3d::Front;
    visible_backface.depth_test = false;
    visible_backface.depth_write = false;
    let mut hidden_backface =
        Instance3d::new(backface, identity, api::Color::rgba(1.0, 0.0, 0.72, 1.0));
    hidden_backface.cull = scene3d::CullMode3d::Back;
    hidden_backface.depth_test = false;
    hidden_backface.depth_write = false;

    let instances = [shaded_instance, emissive_instance, visible_backface, hidden_backface];
    let scene = Pass3d {
        viewport: None,
        clear_color: Some(api::Color::rgba(0.035, 0.045, 0.07, 1.0)),
        clear_depth: true,
        view_proj: identity,
        instances: &instances,
        bloom: None,
    };
    renderer.encode_scene3d(&scene)?;

    let mut overlay = api::DrawList::default();
    for (x, color) in [
        (26.0, api::Color::rgba(0.52, 0.95, 0.76, 1.0)),
        (82.0, api::Color::rgba(1.0, 0.74, 0.35, 1.0)),
        (138.0, api::Color::rgba(0.58, 0.75, 1.0, 1.0)),
    ] {
        overlay.items.push(api::DrawCmd::RRect {
            rect: api::RectF::new(x, 158.0, 28.0, 7.0),
            radii: [3.0; 4],
            color,
        });
    }
    renderer.encode_pass(&overlay);
    Ok(())
}

fn encode_scene3d_blend_modes(renderer: &mut metal::MetalRenderer) -> anyhow::Result<()> {
    let tri_indices = [0_u32, 1, 2];
    let base_vertices = [
        Vertex3d { position: [-0.86, -0.60, 0.22] },
        Vertex3d { position: [0.42, -0.58, 0.22] },
        Vertex3d { position: [-0.22, 0.58, 0.22] },
    ];
    let alpha_vertices = [
        Vertex3d { position: [-0.36, -0.50, 0.12] },
        Vertex3d { position: [0.78, -0.40, 0.12] },
        Vertex3d { position: [0.14, 0.58, 0.12] },
    ];
    let additive_vertices = [
        Vertex3d { position: [-0.58, -0.12, 0.06] },
        Vertex3d { position: [0.62, -0.06, 0.06] },
        Vertex3d { position: [0.02, 0.72, 0.06] },
    ];
    let line_vertices = [
        Vertex3d { position: [-0.86, 0.64, 0.0] },
        Vertex3d { position: [0.86, -0.64, 0.0] },
        Vertex3d { position: [-0.72, -0.70, 0.0] },
        Vertex3d { position: [0.76, 0.58, 0.0] },
    ];
    let line_indices = [0_u32, 1, 2, 3];
    let base = renderer.mesh3d_create(&Mesh3dData {
        vertices: &base_vertices,
        indices: &tri_indices,
        topology: scene3d::MeshTopology::Triangles,
    })?;
    let alpha = renderer.mesh3d_create(&Mesh3dData {
        vertices: &alpha_vertices,
        indices: &tri_indices,
        topology: scene3d::MeshTopology::Triangles,
    })?;
    let additive = renderer.mesh3d_create(&Mesh3dData {
        vertices: &additive_vertices,
        indices: &tri_indices,
        topology: scene3d::MeshTopology::Triangles,
    })?;
    let lines = renderer.mesh3d_create(&Mesh3dData {
        vertices: &line_vertices,
        indices: &line_indices,
        topology: scene3d::MeshTopology::Lines,
    })?;

    let identity = mat4_identity();
    let mut base_instance =
        Instance3d::new(base, identity, api::Color::rgba(0.08, 0.42, 0.95, 1.0));
    base_instance.cull = scene3d::CullMode3d::None;
    let mut alpha_instance =
        Instance3d::new(alpha, identity, api::Color::rgba(0.22, 0.92, 0.72, 0.56));
    alpha_instance.cull = scene3d::CullMode3d::None;
    alpha_instance.depth_test = false;
    alpha_instance.depth_write = false;
    alpha_instance.blend = scene3d::BlendMode3d::Alpha;
    let mut additive_instance =
        Instance3d::new(additive, identity, api::Color::rgba(1.0, 0.38, 0.08, 0.72));
    additive_instance.cull = scene3d::CullMode3d::None;
    additive_instance.depth_test = false;
    additive_instance.depth_write = false;
    additive_instance.blend = scene3d::BlendMode3d::Additive;
    additive_instance.material = scene3d::Material3d::Emissive;
    additive_instance.params = [1.35, 0.0, 0.0, 0.0];
    let mut line_instance =
        Instance3d::new(lines, identity, api::Color::rgba(0.96, 0.90, 0.24, 0.82));
    line_instance.cull = scene3d::CullMode3d::None;
    line_instance.depth_test = false;
    line_instance.depth_write = false;
    line_instance.blend = scene3d::BlendMode3d::Additive;

    let instances = [base_instance, alpha_instance, additive_instance, line_instance];
    let scene = Pass3d {
        viewport: None,
        clear_color: Some(api::Color::rgba(0.035, 0.035, 0.055, 1.0)),
        clear_depth: true,
        view_proj: identity,
        instances: &instances,
        bloom: None,
    };
    renderer.encode_scene3d(&scene)?;

    let mut overlay = api::DrawList::default();
    for (x, color) in [
        (24.0, api::Color::rgba(0.20, 0.64, 1.0, 1.0)),
        (76.0, api::Color::rgba(0.34, 0.96, 0.76, 1.0)),
        (128.0, api::Color::rgba(1.0, 0.58, 0.20, 1.0)),
    ] {
        overlay.items.push(api::DrawCmd::RRect {
            rect: api::RectF::new(x, 160.0, 34.0, 8.0),
            radii: [3.0; 4],
            color,
        });
    }
    renderer.encode_pass(&overlay);
    Ok(())
}

fn id_mask_snapshot_vertices(
    w_dp: f32,
    h_dp: f32,
) -> Vec<metal::id_mask_compositor::IdMaskRasterVertex> {
    let x0 = w_dp * 0.18;
    let x1 = w_dp * 0.82;
    let y0 = h_dp * 0.20;
    let y1 = h_dp * 0.78;
    let mid_x = (x0 + x1) * 0.5;
    vec![
        metal::id_mask_compositor::IdMaskRasterVertex::new([x0, y0], 0, 1),
        metal::id_mask_compositor::IdMaskRasterVertex::new([mid_x, y0], 0, 1),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x0, y1], 0, 1),
        metal::id_mask_compositor::IdMaskRasterVertex::new([mid_x, y0], 1, 8),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x1, y0], 1, 8),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x1, y1], 1, 8),
        metal::id_mask_compositor::IdMaskRasterVertex::new([mid_x, y0], 1, 8),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x1, y1], 1, 8),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x0, y1], 2, 16),
    ]
}

fn id_mask_seam_snapshot_vertices(
    w_dp: f32,
    h_dp: f32,
) -> Vec<metal::id_mask_compositor::IdMaskRasterVertex> {
    let x0 = w_dp * 0.16;
    let x1 = w_dp * 0.84;
    let y0 = h_dp * 0.16;
    let y1 = h_dp * 0.84;
    let mid_x = (x0 + x1) * 0.5;
    let mid_y = (y0 + y1) * 0.5;
    let city = 1;
    vec![
        metal::id_mask_compositor::IdMaskRasterVertex::new([x0, y0], city, 4),
        metal::id_mask_compositor::IdMaskRasterVertex::new([mid_x, y0], city, 4),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x0, mid_y], city, 4),
        metal::id_mask_compositor::IdMaskRasterVertex::new([mid_x, y0], city, 4),
        metal::id_mask_compositor::IdMaskRasterVertex::new([mid_x, mid_y], city, 4),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x0, mid_y], city, 4),
        metal::id_mask_compositor::IdMaskRasterVertex::new([mid_x, y0], city, 8),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x1, y0], city, 8),
        metal::id_mask_compositor::IdMaskRasterVertex::new([mid_x, mid_y], city, 8),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x1, y0], city, 8),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x1, mid_y], city, 8),
        metal::id_mask_compositor::IdMaskRasterVertex::new([mid_x, mid_y], city, 8),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x0, mid_y], city, 12),
        metal::id_mask_compositor::IdMaskRasterVertex::new([mid_x, mid_y], city, 12),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x0, y1], city, 12),
        metal::id_mask_compositor::IdMaskRasterVertex::new([mid_x, mid_y], city, 12),
        metal::id_mask_compositor::IdMaskRasterVertex::new([mid_x, y1], city, 12),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x0, y1], city, 12),
        metal::id_mask_compositor::IdMaskRasterVertex::new([mid_x, mid_y], city, 16),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x1, mid_y], city, 16),
        metal::id_mask_compositor::IdMaskRasterVertex::new([mid_x, y1], city, 16),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x1, mid_y], city, 16),
        metal::id_mask_compositor::IdMaskRasterVertex::new([x1, y1], city, 16),
        metal::id_mask_compositor::IdMaskRasterVertex::new([mid_x, y1], city, 16),
    ]
}

fn encode_id_mask_snapshot(
    renderer: &mut metal::MetalRenderer,
    w_dp: f32,
    h_dp: f32,
    w: u32,
    h: u32,
    scale: f32,
    mode: metal::id_mask_compositor::IdMaskCompositorMode,
    glow_enabled: bool,
) -> anyhow::Result<()> {
    let vertices = if mode == metal::id_mask_compositor::IdMaskCompositorMode::SeamMask {
        id_mask_seam_snapshot_vertices(w_dp, h_dp)
    } else {
        id_mask_snapshot_vertices(w_dp, h_dp)
    };
    let city_styles = [
        metal::id_mask_compositor::IdMaskCityStyle {
            fill_rgb: [0.15, 0.55, 0.95],
            edge_rgb: [0.05, 0.16, 0.30],
            seam_rgb: [1.0, 1.0, 1.0],
        },
        metal::id_mask_compositor::IdMaskCityStyle {
            fill_rgb: [0.95, 0.38, 0.22],
            edge_rgb: [0.33, 0.08, 0.04],
            seam_rgb: [1.0, 0.95, 0.75],
        },
        metal::id_mask_compositor::IdMaskCityStyle {
            fill_rgb: [0.20, 0.75, 0.38],
            edge_rgb: [0.04, 0.24, 0.08],
            seam_rgb: [0.85, 1.0, 0.85],
        },
        metal::id_mask_compositor::IdMaskCityStyle::default(),
    ];
    let mut neighborhood_colors =
        [[0.0_f32; 3]; metal::id_mask_compositor::ID_MASK_MAX_NEIGHBORHOOD_COLORS];
    for (index, color) in neighborhood_colors.iter_mut().enumerate() {
        let t = index as f32 / metal::id_mask_compositor::ID_MASK_MAX_NEIGHBORHOOD_COLORS as f32;
        *color = [0.15 + t * 0.70, 0.20 + (1.0 - t) * 0.50, 0.45 + t * 0.35];
    }
    let pass = metal::id_mask_compositor::IdMaskGpuCompositorPass {
        raster: metal::id_mask_compositor::IdMaskGpuRasterPass {
            viewport: api::RectF::new(0.0, 0.0, w_dp, h_dp),
            mask_width: w as usize,
            mask_height: h as usize,
            mask_scale: scale,
            vertex_revision: 1,
            vertices: &vertices,
            projection: metal::id_mask_compositor::IdMaskRasterProjection::screen_px(),
        },
        city_styles,
        neighborhood_colors,
        mode,
        glow_enabled,
        darken_background_alpha: 0.0,
        polish: metal::id_mask_compositor::IdMaskPolishConfig::default(),
    };
    renderer.encode_id_mask_gpu_compositor(&pass)?;
    Ok(())
}

fn encode_direct_component(
    component: &str,
    renderer: &mut metal::MetalRenderer,
    w_dp: f32,
    h_dp: f32,
    w: u32,
    h: u32,
    scale: f32,
) -> anyhow::Result<bool> {
    match component.to_ascii_lowercase().as_str() {
        "scene3d_mixed" => {
            encode_scene3d_mixed(renderer)?;
            Ok(true)
        }
        "scene3d_bloom" => {
            encode_scene3d_bloom(renderer)?;
            Ok(true)
        }
        "scene3d_depth_stack" => {
            encode_scene3d_depth_stack(renderer)?;
            Ok(true)
        }
        "scene3d_viewport_clip" => {
            encode_scene3d_viewport_clip(renderer, w_dp, h_dp)?;
            Ok(true)
        }
        "scene3d_material_cull" => {
            encode_scene3d_material_cull(renderer)?;
            Ok(true)
        }
        "scene3d_blend_modes" => {
            encode_scene3d_blend_modes(renderer)?;
            Ok(true)
        }
        "id_mask_compositor" => {
            encode_id_mask_snapshot(
                renderer,
                w_dp,
                h_dp,
                w,
                h,
                scale,
                metal::id_mask_compositor::IdMaskCompositorMode::Beauty,
                true,
            )?;
            Ok(true)
        }
        "id_mask_compositor_city_ids" => {
            encode_id_mask_snapshot(
                renderer,
                w_dp,
                h_dp,
                w,
                h,
                scale,
                metal::id_mask_compositor::IdMaskCompositorMode::CityIdMask,
                false,
            )?;
            Ok(true)
        }
        "id_mask_compositor_neighborhood_ids" => {
            encode_id_mask_snapshot(
                renderer,
                w_dp,
                h_dp,
                w,
                h,
                scale,
                metal::id_mask_compositor::IdMaskCompositorMode::NeighborhoodIdMask,
                false,
            )?;
            Ok(true)
        }
        "id_mask_compositor_seams" => {
            encode_id_mask_snapshot(
                renderer,
                w_dp,
                h_dp,
                w,
                h,
                scale,
                metal::id_mask_compositor::IdMaskCompositorMode::SeamMask,
                false,
            )?;
            Ok(true)
        }
        _ => Ok(false),
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
        "camera_preview"
        | "camera_preview_legacy"
        | "camera_preview_bgra"
        | "camera_preview_blur_gray"
        | "camera_preview_tint_alpha" => {
            renderer.set_camera_texture_source(metal::CameraTextureSource::SyntheticBenchmark);
            let mode = match comp.to_ascii_lowercase().as_str() {
                "camera_preview_legacy" => metal::CameraRenderMode::Nv12Legacy,
                "camera_preview_bgra" => metal::CameraRenderMode::BgraBenchmark,
                _ => metal::CameraRenderMode::Nv12Optimized,
            };
            renderer.set_camera_render_mode(mode);
            b.rrect(vp, [0.0; 4], api::Color::rgba(0.02, 0.03, 0.04, 1.0));
            if comp.eq_ignore_ascii_case("camera_preview_blur_gray") {
                b.camera_bg(panel, api::Color::rgba(0.78, 0.92, 1.0, 1.0), 0.92, true, true, 8.0);
            } else if comp.eq_ignore_ascii_case("camera_preview_tint_alpha") {
                b.camera_bg(panel, api::Color::rgba(1.0, 0.72, 0.56, 1.0), 0.58, false, false, 0.0);
            } else {
                b.camera_bg(panel, api::Color::rgba(1.0, 1.0, 1.0, 1.0), 1.0, false, false, 0.0);
            }
        }
        "scene_controls"
        | "scene_text"
        | "scene_zoom"
        | "scene_anim_timeline"
        | "scene_collection"
        | "scene_damage"
        | "scene_input_lab"
        | "scene_nine_slice"
        | "scene_sdf_text"
        | "scene_snapshot"
        | "scene_camera"
        | "scene_elements_extended"
        | "scene_animation_config"
        | "scene_orchestration"
        | "scene_permissions"
        | "scene_integration"
        | "scene_stress" => {
            let ptr: *mut metal::MetalRenderer = renderer;
            let mut router = scenes::Router::new(Uploader { r: ptr });
            // Provide an image for zoom scene
            let (zw, zh) = (256u32, 256u32);
            let zrgba = gen_checker_rgba(zw, zh);
            let tex = renderer.image_create_rgba8(zw, zh, &zrgba, (zw as usize) * 4);
            router.set_zoom_image(tex, zw, zh);
            router.nine_slice_set_image(tex);
            // Try to load a vendored font into the router's text context
            try_load_font_into_ctx(&mut router.text);
            let idx = match comp.to_ascii_lowercase().as_str() {
                "scene_controls" => 0,
                "scene_text" => 1,
                "scene_zoom" => 2,
                "scene_anim_timeline" => 3,
                "scene_collection" => 4,
                "scene_damage" => 5,
                "scene_input_lab" => 6,
                "scene_nine_slice" => 7,
                "scene_sdf_text" => 8,
                "scene_snapshot" => 9,
                "scene_camera" => 10,
                "scene_elements_extended" => 11,
                "scene_animation_config" => 12,
                "scene_orchestration" => 13,
                "scene_permissions" => 14,
                "scene_integration" => 15,
                "scene_stress" => 16,
                _ => 0,
            };
            if comp.eq_ignore_ascii_case("scene_camera") {
                renderer.set_camera_texture_source(metal::CameraTextureSource::SyntheticBenchmark);
                renderer.set_camera_render_mode(metal::CameraRenderMode::Nv12Optimized);
            }
            router.set_scene(idx);
            router.toggle_overlay();
            if comp.eq_ignore_ascii_case("scene_damage") {
                router.damage_set_options(true, 0.75, 0.25);
                router.damage_set_stats(0.37, 2);
            }
            let dt = time_ms.unwrap_or(0);
            router.update(oxide_timing::now_ms(), dt);
            router.draw(panel, 1.0, &mut b);
            // If no font loaded (empty DB), draw a deterministic bitmap label of the scene name
            if router.text.fonts.font(0).is_none() {
                let name = match idx {
                    0 => "CONTROLS",
                    1 => "TEXT",
                    2 => "ZOOM",
                    3 => "ANIM",
                    4 => "COLLECTION",
                    5 => "DAMAGE",
                    6 => "INPUT",
                    7 => "NINE",
                    8 => "SDF",
                    9 => "SNAP",
                    10 => "CAMERA",
                    11 => "ELEMS",
                    12 => "ANIM CFG",
                    13 => "ORCH",
                    14 => "PERMS",
                    15 => "INTEG",
                    16 => "STRESS",
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
        "text_input_ime_composition" => {
            let mut txtctx = ui::elements::TextCtx::default();
            try_load_font_into_ctx(&mut txtctx);
            add_cjk_fallback_font(&mut txtctx);
            let ptr: *mut metal::MetalRenderer = renderer;
            let mut up = Uploader { r: ptr };
            let title = ui::elements::Label {
                text: "IME composition range".to_string(),
                color: api::Color::rgba(0.10, 0.12, 0.16, 1.0),
                align: ui::elements::Align::Left,
                wrap: false,
                font_id: 0,
                font_px: 15.0,
            };
            title.encode(
                api::RectF::new(panel.x + 18.0, panel.y + 20.0, panel.w - 36.0, 22.0),
                1.0,
                &mut txtctx,
                &mut up,
                &mut b,
            );
            let input = ui::elements::TextInput {
                style: ui::elements::TextInputStyle {
                    font_id: 0,
                    font_px: 28.0,
                    placeholder_font_px: 18.0,
                    composition: api::Color::rgba(0.15, 0.44, 0.92, 0.72),
                    ..ui::elements::TextInputStyle::default()
                },
                corner_radius: 8.0,
            };
            let mut st = ui::elements::TextInputState::new("Name");
            st.focus();
            st.handle_text_event(&plat::TextEvent::Commit { text: "oxide".into() });
            st.handle_text_event(&plat::TextEvent::Composition { range: 1..4, text: "漢".into() });
            input.encode(
                &st,
                api::RectF::new(panel.x + 18.0, panel.y + 76.0, panel.w - 36.0, 58.0),
                1.0,
                &mut txtctx,
                &mut up,
                &mut b,
            );
        }
        "text_input_grapheme_selection" => {
            let mut txtctx = ui::elements::TextCtx::default();
            try_load_font_into_ctx(&mut txtctx);
            let ptr: *mut metal::MetalRenderer = renderer;
            let mut up = Uploader { r: ptr };
            let title = ui::elements::Label {
                text: "Grapheme selection".to_string(),
                color: api::Color::rgba(0.10, 0.12, 0.16, 1.0),
                align: ui::elements::Align::Left,
                wrap: false,
                font_id: 0,
                font_px: 15.0,
            };
            title.encode(
                api::RectF::new(panel.x + 18.0, panel.y + 20.0, panel.w - 36.0, 22.0),
                1.0,
                &mut txtctx,
                &mut up,
                &mut b,
            );
            let input = ui::elements::TextInput {
                style: ui::elements::TextInputStyle {
                    font_id: 0,
                    font_px: 26.0,
                    placeholder_font_px: 18.0,
                    selection: api::Color::rgba(0.10, 0.46, 0.96, 0.32),
                    ..ui::elements::TextInputStyle::default()
                },
                corner_radius: 8.0,
            };
            let mut st = ui::elements::TextInputState::new("Name");
            st.set_text("e\u{301}clair oxide");
            st.focus();
            st.set_selection(0, 1);
            input.encode(
                &st,
                api::RectF::new(panel.x + 18.0, panel.y + 76.0, panel.w - 36.0, 58.0),
                1.0,
                &mut txtctx,
                &mut up,
                &mut b,
            );
        }
        "text_input_fallback_cjk" => {
            let mut txtctx = ui::elements::TextCtx::default();
            try_load_font_into_ctx(&mut txtctx);
            add_cjk_fallback_font(&mut txtctx);
            let ptr: *mut metal::MetalRenderer = renderer;
            let mut up = Uploader { r: ptr };
            let title = ui::elements::Label {
                text: "Fallback CJK cursor width".to_string(),
                color: api::Color::rgba(0.10, 0.12, 0.16, 1.0),
                align: ui::elements::Align::Left,
                wrap: false,
                font_id: 0,
                font_px: 15.0,
            };
            title.encode(
                api::RectF::new(panel.x + 18.0, panel.y + 20.0, panel.w - 36.0, 22.0),
                1.0,
                &mut txtctx,
                &mut up,
                &mut b,
            );
            let input = ui::elements::TextInput {
                style: ui::elements::TextInputStyle {
                    font_id: 0,
                    font_px: 28.0,
                    placeholder_font_px: 18.0,
                    selection: api::Color::rgba(0.10, 0.46, 0.96, 0.32),
                    ..ui::elements::TextInputStyle::default()
                },
                corner_radius: 8.0,
            };
            let mut st = ui::elements::TextInputState::new("Name");
            st.set_text("A漢B oxide");
            st.focus();
            st.set_selection(1, 2);
            input.encode(
                &st,
                api::RectF::new(panel.x + 18.0, panel.y + 76.0, panel.w - 36.0, 58.0),
                1.0,
                &mut txtctx,
                &mut up,
                &mut b,
            );
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

    let direct = is_direct_component(&component);
    let list = if direct {
        None
    } else {
        Some(render_component(
            &component,
            &args.state,
            w_dp,
            h_dp,
            &mut r,
            args.time_ms,
            args.period_ms,
        ))
    };
    let fb = &api::FrameTarget;
    let token = r.begin_frame(fb, None);
    if !encode_direct_component(&component, &mut r, w_dp, h_dp, w, h, scale)? {
        if let Some(list) = list.as_ref() {
            r.encode_pass(list);
        }
    }
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
                if !args.allow_mismatch
                    && (pixdiff > args.pixel_tolerance
                        || max_err > args.max_error_tolerance
                        || mse > args.mse_tolerance)
                {
                    anyhow::bail!(
                        "golden mismatch: pixdiff={} max_err={} mse={:.6} tolerances pixdiff={} max_err={} mse={:.6}",
                        pixdiff,
                        max_err,
                        mse,
                        args.pixel_tolerance,
                        args.max_error_tolerance,
                        args.mse_tolerance
                    );
                }
            } else {
                if !args.allow_mismatch {
                    anyhow::bail!("golden size mismatch: got {}x{}, golden {}x{}", w, h, gw, gh);
                }
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
