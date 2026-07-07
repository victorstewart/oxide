use oxide_renderer_api::{
    Color, GlyphRun, ImageHandle, Insets, RectF, RectI, RenderEncoder, Vertex, VisualEffect,
};
use oxide_starscape_background::{
    color_from_srgb_u8, StarscapeAtmosphereConfig, StarscapeAtmosphereMode,
    StarscapeAtmosphereOrigin, StarscapeBackground, StarscapeBackgroundConfig, StarscapeStarConfig,
};

#[derive(Default)]
struct RecordingEncoder {
    solids: Vec<(RectF, Color)>,
    rrects: Vec<(RectF, Color)>,
}

impl RenderEncoder for RecordingEncoder {
    fn set_viewport(&mut self, _vp: RectF) {}
    fn set_clip(&mut self, _scissor: RectI) {}

    fn draw_solid(&mut self, verts: &[Vertex], color: Color) {
        self.solids.push((bounds_for_vertices(verts), color));
    }

    fn draw_image(&mut self, _img: ImageHandle, _dst: RectF, _src: RectF) {}

    fn draw_rrect(&mut self, rect: RectF, _radii: [f32; 4], color: Color) {
        self.rrects.push((rect, color));
    }

    fn draw_nine_slice(&mut self, _img: ImageHandle, _rect: RectF, _slice: Insets, _alpha: f32) {}

    fn draw_backdrop(&mut self, _rect: RectF, _sigma: f32, _tint: Color, _alpha: f32) {}

    fn draw_visual_effect(&mut self, _rect: RectF, _effect: VisualEffect) {}

    fn draw_spinner(&mut self, _center: [f32; 2], _atom: f32, _alpha: f32) {}

    fn draw_glyph_run(&mut self, _run: &GlyphRun) {}
}

fn bounds_for_vertices(verts: &[Vertex]) -> RectF {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for vertex in verts {
        min_x = min_x.min(vertex.x);
        min_y = min_y.min(vertex.y);
        max_x = max_x.max(vertex.x);
        max_y = max_y.max(vertex.y);
    }
    RectF::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

fn draw_with_atmosphere(
    atmosphere: Option<StarscapeAtmosphereConfig>,
    rect: RectF,
) -> RecordingEncoder {
    let pink = color_from_srgb_u8(255, 57, 117, 1.0);
    let mut stars = StarscapeStarConfig::nametag(pink);
    stars.density = 0.0;
    let config = StarscapeBackgroundConfig::new(Color::rgba(0.0, 0.0, 0.0, 1.0), stars, atmosphere);
    let background = StarscapeBackground::new(12345);
    let mut encoder = RecordingEncoder::default();
    background.draw(&mut encoder, rect, &config);
    encoder
}

#[test]
fn simple_atmosphere_alpha_is_strongest_at_origin_edge_and_fades() {
    let config = StarscapeAtmosphereConfig::nametag_top_simple(
        color_from_srgb_u8(255, 57, 117, 1.0),
        color_from_srgb_u8(35, 37, 44, 1.0),
    );
    let encoder = draw_with_atmosphere(Some(config), RectF::new(0.0, 0.0, 100.0, 200.0));
    let strips = &encoder.solids[1..];
    assert!(strips.len() > 8);
    assert!(strips[0].1.a > strips[strips.len() - 1].1.a);
    for pair in strips.windows(2) {
        assert!(
            pair[0].1.a >= pair[1].1.a,
            "alpha increased from {} to {}",
            pair[0].1.a,
            pair[1].1.a
        );
    }
    assert!(strips[strips.len() - 1].1.a <= 0.0025);
}

#[test]
fn simple_atmosphere_strips_are_contiguous_without_overlap() {
    let mut config = StarscapeAtmosphereConfig::nametag_top_simple(
        color_from_srgb_u8(255, 57, 117, 1.0),
        color_from_srgb_u8(35, 37, 44, 1.0),
    );
    config.rows = 32;
    let encoder = draw_with_atmosphere(Some(config), RectF::new(0.0, 0.0, 100.0, 240.0));
    let strips = &encoder.solids[1..];
    assert!(strips.len() > 8);
    for pair in strips.windows(2) {
        let previous_bottom = pair[0].0.y + pair[0].0.h;
        assert!(
            (previous_bottom - pair[1].0.y).abs() <= 0.001,
            "strip boundary overlapped or gapped: previous bottom {}, next top {}",
            previous_bottom,
            pair[1].0.y
        );
    }
}

#[test]
fn bottom_origin_is_top_origin_vertical_mirror() {
    let mut top = StarscapeAtmosphereConfig::nametag_top_simple(
        color_from_srgb_u8(255, 57, 117, 1.0),
        color_from_srgb_u8(35, 37, 44, 1.0),
    );
    top.rows = 24;
    let mut bottom = top;
    bottom.origin = StarscapeAtmosphereOrigin::Bottom;
    let rect = RectF::new(7.0, 11.0, 160.0, 240.0);
    let top_encoder = draw_with_atmosphere(Some(top), rect);
    let bottom_encoder = draw_with_atmosphere(Some(bottom), rect);
    let top_strips = &top_encoder.solids[1..];
    let bottom_strips = &bottom_encoder.solids[1..];
    assert_eq!(top_strips.len(), bottom_strips.len());
    let row_h = rect.h * top.coverage_fraction / top.rows as f32;
    for (top_strip, bottom_strip) in top_strips.iter().zip(bottom_strips.iter()) {
        let mirrored_y = rect.y + rect.h - (top_strip.0.y - rect.y) - row_h;
        assert!((mirrored_y - bottom_strip.0.y).abs() <= 1.0);
        assert!((top_strip.1.a - bottom_strip.1.a).abs() <= f32::EPSILON);
    }
}

#[test]
fn disabling_atmosphere_leaves_star_and_dust_draws_unchanged() {
    let background = StarscapeBackground::new(12345);
    let pink = color_from_srgb_u8(255, 57, 117, 1.0);
    let evening = color_from_srgb_u8(35, 37, 44, 1.0);
    let base = StarscapeBackgroundConfig::new(
        Color::rgba(0.0, 0.0, 0.0, 1.0),
        StarscapeStarConfig::nametag(pink),
        None,
    );
    let with_atmosphere = StarscapeBackgroundConfig::new(
        base.base_color,
        base.stars,
        Some(StarscapeAtmosphereConfig::nametag_top_simple(pink, evening)),
    );
    let rect = RectF::new(0.0, 0.0, 320.0, 480.0);
    let mut no_atmosphere = RecordingEncoder::default();
    let mut atmosphere = RecordingEncoder::default();
    background.draw(&mut no_atmosphere, rect, &base);
    background.draw(&mut atmosphere, rect, &with_atmosphere);
    assert_eq!(no_atmosphere.rrects, atmosphere.rrects);
}

#[test]
fn nametag_presets_keep_exact_srgb_color_inputs() {
    let pink = color_from_srgb_u8(255, 57, 117, 1.0);
    let evening = color_from_srgb_u8(35, 37, 44, 1.0);
    let simple = StarscapeAtmosphereConfig::nametag_top_simple(pink, evening);
    let complex = StarscapeAtmosphereConfig::nametag_top_complex(pink, evening);
    assert_eq!(simple.pink, Color::rgba(255.0 / 255.0, 57.0 / 255.0, 117.0 / 255.0, 1.0));
    assert_eq!(simple.evening, Color::rgba(35.0 / 255.0, 37.0 / 255.0, 44.0 / 255.0, 1.0));
    assert_eq!(complex.origin, StarscapeAtmosphereOrigin::Top);
    assert_eq!(complex.mode, StarscapeAtmosphereMode::ComplexSoftMesh);
    assert_eq!(complex.rows, 96);
    assert_eq!(complex.pink, simple.pink);
    assert_eq!(complex.evening, simple.evening);
}
