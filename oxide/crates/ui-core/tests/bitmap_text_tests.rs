use font8x8::{UnicodeFonts, BASIC_FONTS};
use fontdue::{
    layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle as FontdueTextStyle},
    Font, FontSettings,
};
use oxide_renderer_api::{
    Color, GlyphRun, ImageHandle, Insets, RectF, RectI, RenderEncoder, Vertex,
};
use oxide_ui_core::bitmap_text::{
    line_height, resolve_text_with_placeholder, text_width, text_width_pixel_snapped,
    text_width_spans, BitmapTextAtlas, TextAlign, TextSpan, TextStyle,
};
use oxide_ui_core::{
    draw_text_input_options_popover, text_input_options_layout, TextInputOptionsConfig,
    TextInputOptionsPopoverStyle,
};

#[derive(Default)]
struct CollectingEncoder {
    rects: Vec<RectF>,
    glyph_runs: usize,
    rrects: usize,
    resolved_glyph_vertices: usize,
    resolved_glyph_indices: usize,
    glyph_vertices: Vec<Vertex>,
}

impl RenderEncoder for CollectingEncoder {
    fn set_viewport(&mut self, _vp: RectF) {}

    fn set_clip(&mut self, _scissor: RectI) {}

    fn draw_solid(&mut self, verts: &[Vertex], _color: Color) {
        if verts.is_empty() {
            return;
        }
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for v in verts {
            min_x = min_x.min(v.x);
            min_y = min_y.min(v.y);
            max_x = max_x.max(v.x);
            max_y = max_y.max(v.y);
        }
        self.rects.push(RectF::new(
            min_x,
            min_y,
            (max_x - min_x).max(0.0),
            (max_y - min_y).max(0.0),
        ));
    }

    fn draw_image(&mut self, _img: ImageHandle, _dst: RectF, _src: RectF) {}

    fn draw_rrect(&mut self, _rect: RectF, _radii: [f32; 4], _color: Color) {
        self.rrects += 1;
    }

    fn draw_nine_slice(&mut self, _img: ImageHandle, _rect: RectF, _slice: Insets, _alpha: f32) {}

    fn draw_backdrop(&mut self, _rect: RectF, _sigma: f32, _tint: Color, _alpha: f32) {}

    fn draw_spinner(&mut self, _center: [f32; 2], _atom: f32, _alpha: f32) {}

    fn draw_glyph_run(&mut self, _run: &GlyphRun) {
        self.glyph_runs += 1;
    }

    fn draw_glyph_run_resolved(&mut self, run: &GlyphRun, vertices: &[Vertex], indices: &[u16]) {
        self.draw_glyph_run(run);
        self.resolved_glyph_vertices += vertices.len();
        self.resolved_glyph_indices += indices.len();
        self.glyph_vertices.extend_from_slice(vertices);
    }
}

#[test]
fn bitmap_reference_uses_lsb_left_to_right_bit_order() {
    let mut encoder = CollectingEncoder::default();
    draw_bitmap_reference(
        &mut encoder,
        '\\',
        10.0,
        20.0,
        Color::rgba(1.0, 1.0, 1.0, 1.0),
    );
    assert!(!encoder.rects.is_empty(), "expected bitmap rect draws for glyph");

    let glyph = BASIC_FONTS.get('\\').expect("glyph backslash");
    let (row_index, row_bits) = glyph
        .iter()
        .copied()
        .enumerate()
        .find(|(_, bits)| {
            let lsb_left = (0..8).find(|col| ((bits >> col) & 1) == 1);
            let msb_left = (0..8).find(|col| ((bits >> (7 - col)) & 1) == 1);
            lsb_left.is_some() && lsb_left != msb_left
        })
        .expect("row with orientation-sensitive bits");
    let expected_left_col = (0..8).find(|col| ((row_bits >> col) & 1) == 1).expect("lit row");
    let row_y = 20.0 + row_index as f32;
    let mut min_x = f32::INFINITY;
    for rect in &encoder.rects {
        if (rect.y - row_y).abs() < 0.001 {
            min_x = min_x.min(rect.x);
        }
    }
    assert!(min_x.is_finite(), "expected at least one run on first row");
    assert!((min_x - (10.0 + expected_left_col as f32)).abs() < 0.001);
}

#[test]
fn small_asap_text_uses_smooth_widths() {
    let style = TextStyle::new(5.25, Color::rgba(1.0, 1.0, 1.0, 1.0));
    assert!(text_width("followers", style) < 40.0);
    assert!((line_height(style) - 6.3).abs() < 0.001);
}

#[test]
fn smooth_text_width_includes_trailing_advance() {
    let style = TextStyle::new(12.0, Color::rgba(1.0, 1.0, 1.0, 1.0));
    assert!(text_width("scope    ", style) > text_width("scope", style));
}

#[test]
fn pixel_snapped_text_width_keeps_trailing_advance() {
    let style = TextStyle::new(7.0, Color::rgba(1.0, 1.0, 1.0, 1.0));
    assert!(text_width_pixel_snapped("500    ", style) > text_width_pixel_snapped("500", style));
}

#[test]
fn text_width_spans_matches_sum_of_segment_widths() {
    let style_a = TextStyle::new(5.0, Color::rgba(1.0, 1.0, 1.0, 1.0));
    let style_b = TextStyle::new(5.0, Color::rgba(0.8, 0.8, 0.8, 1.0)).bold();
    let spans = [TextSpan::new("first ", style_a), TextSpan::new("name", style_b)];
    let expected = text_width("first ", style_a) + text_width("name", style_b);
    assert!((text_width_spans(&spans) - expected).abs() < 0.001);
}

#[test]
fn resolve_text_with_placeholder_prefers_placeholder_when_empty() {
    let text_style = TextStyle::new(5.0, Color::rgba(1.0, 1.0, 1.0, 1.0));
    let placeholder_style = TextStyle::new(5.0, Color::rgba(0.6, 0.6, 0.6, 1.0)).italic();
    let span = resolve_text_with_placeholder("", "username", text_style, placeholder_style);
    assert_eq!(span, TextSpan::new("username", placeholder_style));
}

#[test]
fn resolve_text_with_placeholder_prefers_entered_text_when_present() {
    let text_style = TextStyle::new(5.0, Color::rgba(1.0, 1.0, 1.0, 1.0)).bold();
    let placeholder_style = TextStyle::new(5.0, Color::rgba(0.6, 0.6, 0.6, 1.0));
    let span = resolve_text_with_placeholder("victor", "username", text_style, placeholder_style);
    assert_eq!(span, TextSpan::new("victor", text_style));
}

#[test]
fn atlas_text_records_resolved_glyphs_instead_of_solid_runs() {
    let mut atlas = BitmapTextAtlas::new();
    let (pixels, width, height) = atlas.image();
    assert_eq!(pixels.len(), width as usize * height as usize);
    atlas.set_handle(ImageHandle(77));

    let mut encoder = CollectingEncoder::default();
    let style = TextStyle::new(18.0, Color::rgba(1.0, 1.0, 1.0, 1.0)).bold();
    assert!(atlas.draw_text(&mut encoder, "followers", 12.0, 24.0, style, 2.0));

    assert!(encoder.rects.is_empty(), "atlas text must not emit solid alpha runs");
    assert!(encoder.glyph_runs > 0, "expected glyph run command");
    assert!(encoder.resolved_glyph_vertices > 0, "expected resolved glyph vertices");
    assert!(encoder.resolved_glyph_indices > 0, "expected resolved glyph indices");
    assert!(atlas.dirty_rect().is_some(), "new glyphs should dirty the A8 atlas");
    atlas.clear_dirty();
    assert!(atlas.dirty_rect().is_none());
}

#[test]
fn atlas_glyph_run_matches_retired_fontdue_coverage_and_geometry_exactly() {
    const OVERSAMPLE: f32 = 3.0;
    let font = Font::from_bytes(
        include_bytes!("../assets/Asap-Bold.ttf").as_slice(),
        FontSettings::default(),
    )
    .expect("Asap bold font");
    let style = TextStyle::new(10.6, Color::rgba(1.0, 1.0, 1.0, 0.96)).bold();
    let mut atlas = BitmapTextAtlas::new();
    atlas.set_handle(ImageHandle(77));

    for (index, label) in ["Cut", "Copy", "Select All", "Paste"].iter().enumerate() {
        let x = 12.0 + index as f32 * 90.0;
        let y = 24.0;
        let mut encoder = CollectingEncoder::default();
        assert!(atlas.draw_text(&mut encoder, label, x, y, style, 2.0));

        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings {
            x: (x * OVERSAMPLE).round(),
            y: (y * OVERSAMPLE).round(),
            ..LayoutSettings::default()
        });
        layout.append(&[&font], &FontdueTextStyle::new(label, style.px * OVERSAMPLE, 0));

        let (pixels, atlas_width, atlas_height) = atlas.image();
        let mut candidate_vertex = 0usize;
        for glyph in layout.glyphs() {
            let (metrics, reference) = font.rasterize_indexed(glyph.key.glyph_index, glyph.key.px);
            if metrics.width == 0 || metrics.height == 0 {
                continue;
            }
            let quad = &encoder.glyph_vertices[candidate_vertex..candidate_vertex + 4];
            candidate_vertex += 4;
            assert!((quad[0].x - glyph.x.round() / OVERSAMPLE).abs() < 0.0001);
            assert!((quad[0].y - glyph.y.round() / OVERSAMPLE).abs() < 0.0001);
            assert!((quad[1].x - quad[0].x - metrics.width as f32 / OVERSAMPLE).abs() < 0.0001);
            assert!((quad[2].y - quad[0].y - metrics.height as f32 / OVERSAMPLE).abs() < 0.0001);

            let atlas_x = (quad[0].u * atlas_width as f32).round() as usize;
            let atlas_y = (quad[0].v * atlas_height as f32).round() as usize;
            for row in 0..metrics.height {
                let atlas_offset = (atlas_y + row) * atlas_width as usize + atlas_x;
                let reference_offset = row * metrics.width;
                assert_eq!(
                    &pixels[atlas_offset..atlas_offset + metrics.width],
                    &reference[reference_offset..reference_offset + metrics.width],
                    "coverage mismatch for {label} glyph {} row {row}",
                    glyph.key.glyph_index,
                );
            }
        }
        assert_eq!(candidate_vertex, encoder.glyph_vertices.len());
    }
}

#[test]
fn text_input_options_use_one_glyph_run_per_label_and_no_alpha_run_solids() {
    let layout = text_input_options_layout(
        RectF::new(260.0, 80.0, 120.0, 44.0),
        RectF::new(0.0, 0.0, 640.0, 480.0),
        1.0,
        TextInputOptionsConfig::all(),
        10.6,
    )
    .expect("option layout");
    let style = TextInputOptionsPopoverStyle {
        background: Color::rgba(0.01, 0.01, 0.01, 0.96),
        divider: Color::rgba(1.0, 1.0, 1.0, 0.78),
        text: Color::rgba(1.0, 1.0, 1.0, 0.96),
        text_px: 10.6,
    };
    let mut atlas = BitmapTextAtlas::new();
    let mut missing = CollectingEncoder::default();
    assert!(!draw_text_input_options_popover(
        &mut missing,
        &mut atlas,
        2.0,
        layout,
        style,
    ));
    assert_eq!(missing.glyph_runs, 0);
    assert_eq!(missing.rects.len(), 0);
    assert_eq!(missing.rrects, 0);

    atlas.set_handle(ImageHandle(77));
    let mut encoder = CollectingEncoder::default();
    assert!(draw_text_input_options_popover(
        &mut encoder,
        &mut atlas,
        2.0,
        layout,
        style,
    ));
    assert_eq!(encoder.glyph_runs, 4);
    assert_eq!(encoder.rects.len(), 2, "only the two popover arrows use solid geometry");
    assert_eq!(encoder.rrects, 5, "bubble, inset, and three option dividers");
    assert!(atlas.dirty_rect().is_some());
}

#[test]
fn aligned_draw_centers_and_right_aligns_width() {
    let style = TextStyle::new(8.0, Color::rgba(1.0, 1.0, 1.0, 1.0));
    let rect = RectF::new(12.0, 20.0, 200.0, 24.0);
    let width = text_width("A", style);
    let mut atlas = BitmapTextAtlas::new();
    atlas.set_handle(ImageHandle(77));

    let mut centered = CollectingEncoder::default();
    assert!(atlas.draw_text_aligned(&mut centered, "A", rect, TextAlign::Center, style, 1.0));
    let center_min_x =
        centered.glyph_vertices.iter().map(|vertex| vertex.x).fold(f32::INFINITY, f32::min);

    let mut right = CollectingEncoder::default();
    assert!(atlas.draw_text_aligned(&mut right, "A", rect, TextAlign::Right, style, 1.0));
    let right_min_x =
        right.glyph_vertices.iter().map(|vertex| vertex.x).fold(f32::INFINITY, f32::min);
    let delta_error = (right_min_x - center_min_x - (rect.w - width) * 0.5).abs();
    assert!(
        delta_error <= 1.0 / 3.0 + 0.001,
        "center={center_min_x} right={right_min_x} width={width} error={delta_error}"
    );
}

fn draw_bitmap_reference(
    encoder: &mut dyn RenderEncoder,
    ch: char,
    x: f32,
    y: f32,
    color: Color,
) {
    let Some(bitmap) = BASIC_FONTS.get(ch) else { return };
    for (row_index, row_bits) in bitmap.iter().copied().enumerate() {
        let mut run_start: Option<usize> = None;
        for col in 0..8 {
            let on = ((row_bits >> col) & 1) == 1;
            match (run_start, on) {
                (None, true) => run_start = Some(col),
                (Some(start), false) => {
                    draw_bitmap_reference_run(
                        encoder,
                        x,
                        y + row_index as f32,
                        start,
                        col,
                        color,
                    );
                    run_start = None;
                }
                _ => {}
            }
        }
        if let Some(start) = run_start {
            draw_bitmap_reference_run(
                encoder,
                x,
                y + row_index as f32,
                start,
                8,
                color,
            );
        }
    }
}

fn draw_bitmap_reference_run(
    encoder: &mut dyn RenderEncoder,
    x: f32,
    y: f32,
    start_col: usize,
    end_col: usize,
    color: Color,
) {
    let left = x + start_col as f32;
    let width = end_col.saturating_sub(start_col) as f32;
    let vertices = [
        Vertex { x: left, y, u: 0.0, v: 0.0, rgba: u32::MAX },
        Vertex { x: left + width, y, u: 1.0, v: 0.0, rgba: u32::MAX },
        Vertex { x: left, y: y + 1.0, u: 0.0, v: 1.0, rgba: u32::MAX },
        Vertex { x: left + width, y: y + 1.0, u: 1.0, v: 1.0, rgba: u32::MAX },
    ];
    encoder.draw_solid(&vertices, color);
}
