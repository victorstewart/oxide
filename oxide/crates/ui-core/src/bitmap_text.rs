//! Deterministic text renderer for UI overlays and fallback labels.
//!
//! Preferred path: smooth CPU glyph rasterization from an embedded latin font.
//! Fallback path: deterministic `font8x8` bitmap rasterization for hosts/configs
//! where the smooth font path is unavailable.

use font8x8::{UnicodeFonts, BASIC_FONTS};
use fontdue::{
    layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle as FontdueTextStyle},
    Font, FontSettings,
};
use once_cell::sync::Lazy;
use oxide_renderer_api::{Color, RectF, RenderEncoder, Vertex};
use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;

const SMOOTH_MIN_PX: f32 = 6.0;
const SMOOTH_OVERSAMPLE: f32 = 3.0;
const BUNDLED_FONT_REGULAR_BYTES: &[u8] = include_bytes!("../assets/Asap-Regular.ttf");
const BUNDLED_FONT_BOLD_BYTES: &[u8] = include_bytes!("../assets/Asap-Bold.ttf");
const BUNDLED_FONT_ITALIC_BYTES: &[u8] = include_bytes!("../assets/Asap-Italic.ttf");
const BUNDLED_FONT_FALLBACK_BYTES: &[u8] = include_bytes!("../assets/Asap-VF.ttf");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontFace {
    Regular,
    Bold,
    Italic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RasterKey {
    face: FontFace,
    glyph_index: u16,
    px_tenths: u16,
}

impl RasterKey {
    fn new(face: FontFace, glyph_index: u16, px: f32) -> Option<Self> {
        if !px.is_finite() || px <= 0.0 {
            return None;
        }
        let px_tenths = (px * 10.0).round().clamp(1.0, u16::MAX as f32) as u16;
        Some(Self { face, glyph_index, px_tenths })
    }
}

#[derive(Debug, Clone)]
struct RasterGlyph {
    width: usize,
    height: usize,
    alpha: Vec<u8>,
}

struct SmoothTextState {
    regular: Option<Font>,
    bold: Option<Font>,
    italic: Option<Font>,
    glyphs: HashMap<RasterKey, RasterGlyph>,
}

impl SmoothTextState {
    fn new() -> Self {
        let (regular, bold, italic) = load_host_fonts();
        Self { regular, bold, italic, glyphs: HashMap::new() }
    }

    fn font_for_face(&self, face: FontFace) -> Option<&Font> {
        match face {
            FontFace::Regular => self.regular.as_ref(),
            FontFace::Bold => self.bold.as_ref().or(self.regular.as_ref()),
            FontFace::Italic => self.italic.as_ref().or(self.regular.as_ref()),
        }
    }
}

static SMOOTH_TEXT_STATE: Lazy<Mutex<SmoothTextState>> =
    Lazy::new(|| Mutex::new(SmoothTextState::new()));

fn load_host_fonts() -> (Option<Font>, Option<Font>, Option<Font>) {
    let regular = load_bundled_font(BUNDLED_FONT_REGULAR_BYTES)
        .or_else(|| load_bundled_font(BUNDLED_FONT_FALLBACK_BYTES))
        .or_else(|| load_font_from_env("OXIDE_UI_TEXT_FONT_REGULAR_PATH"))
        .or_else(|| load_font_from_env("OXIDE_UI_TEXT_FONT_PATH"))
        .or_else(load_system_font);
    let bold = load_bundled_font(BUNDLED_FONT_BOLD_BYTES)
        .or_else(|| load_font_from_env("OXIDE_UI_TEXT_FONT_BOLD_PATH"))
        .or_else(|| load_bundled_font(BUNDLED_FONT_FALLBACK_BYTES))
        .or_else(|| load_font_from_env("OXIDE_UI_TEXT_FONT_PATH"))
        .or_else(load_system_font);
    let italic = load_bundled_font(BUNDLED_FONT_ITALIC_BYTES)
        .or_else(|| load_font_from_env("OXIDE_UI_TEXT_FONT_ITALIC_PATH"))
        .or_else(|| load_bundled_font(BUNDLED_FONT_FALLBACK_BYTES))
        .or_else(|| load_font_from_env("OXIDE_UI_TEXT_FONT_PATH"))
        .or_else(load_system_font);
    (regular, bold, italic)
}

fn load_bundled_font(bytes: &[u8]) -> Option<Font> {
    Font::from_bytes(bytes, FontSettings::default()).ok()
}

fn load_font_from_env(key: &str) -> Option<Font> {
    let path = std::env::var(key)
        .ok()
        .map(|path| path.trim().to_owned())
        .filter(|path| !path.is_empty())?;
    load_font_from_path(path.as_str())
}

fn load_system_font() -> Option<Font> {
    let candidates = [
        "/System/Library/Fonts/SFNS.ttf",
        "/System/Library/Fonts/SFNSRounded.ttf",
        "/System/Library/Fonts/NewYork.ttf",
        "/System/Library/Fonts/Geneva.ttf",
        "/System/Library/Fonts/Monaco.ttf",
    ];
    for path in candidates {
        if let Some(font) = load_font_from_path(path) {
            return Some(font);
        }
    }
    None
}

fn load_font_from_path(path: &str) -> Option<Font> {
    let bytes = fs::read(path).ok()?;
    Font::from_bytes(bytes, FontSettings::default()).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub px: f32,
    pub color: Color,
    pub face: FontFace,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextSpan<'a> {
    pub text: &'a str,
    pub style: TextStyle,
    pub y_offset: f32,
}

impl<'a> TextSpan<'a> {
    #[must_use]
    pub fn new(text: &'a str, style: TextStyle) -> Self {
        Self { text, style, y_offset: 0.0 }
    }

    #[must_use]
    pub fn with_y_offset(mut self, y_offset: f32) -> Self {
        self.y_offset = y_offset;
        self
    }
}

impl TextStyle {
    #[must_use]
    pub fn new(px: f32, color: Color) -> Self {
        Self { px, color, face: FontFace::Regular }
    }

    #[must_use]
    pub fn with_face(mut self, face: FontFace) -> Self {
        self.face = face;
        self
    }

    #[must_use]
    pub fn bold(self) -> Self {
        self.with_face(FontFace::Bold)
    }

    #[must_use]
    pub fn italic(self) -> Self {
        self.with_face(FontFace::Italic)
    }

    #[must_use]
    pub fn regular(self) -> Self {
        self.with_face(FontFace::Regular)
    }
}

#[must_use]
pub fn line_height(style: TextStyle) -> f32 {
    if smooth_enabled(style) {
        (style.px * 1.25).max(12.0)
    } else {
        pixel_size(style) * 10.0
    }
}

#[must_use]
pub fn text_width(text: &str, style: TextStyle) -> f32 {
    if let Some(width) = smooth_text_width(text, style) {
        return width;
    }
    text_width_bitmap(text, style)
}

fn text_width_bitmap(text: &str, style: TextStyle) -> f32 {
    let glyph = pixel_size(style) * 8.0;
    let spacing = pixel_size(style) * 1.4;
    if text.is_empty() {
        return 0.0;
    }
    let count = text.chars().count() as f32;
    glyph * count + spacing * (count - 1.0).max(0.0)
}

pub fn draw_text(encoder: &mut dyn RenderEncoder, text: &str, x: f32, y: f32, style: TextStyle) {
    if draw_text_smooth(encoder, text, x, y, style) {
        return;
    }
    draw_text_bitmap(encoder, text, x, y, style);
}

fn draw_text_bitmap(encoder: &mut dyn RenderEncoder, text: &str, x: f32, y: f32, style: TextStyle) {
    let pixel = pixel_size(style);
    let mut cursor = x;
    let spacing = pixel * 1.4;
    for ch in text.chars() {
        if ch == '\n' {
            continue;
        }
        draw_char(encoder, ch, cursor, y, pixel, style.color);
        cursor += pixel * 8.0 + spacing;
    }
}

pub fn draw_text_aligned(
    encoder: &mut dyn RenderEncoder,
    text: &str,
    rect: RectF,
    align: TextAlign,
    style: TextStyle,
) {
    let width = text_width(text, style);
    let x = aligned_x(rect, width, align);
    draw_text(encoder, text, x.max(rect.x), rect.y, style);
}

#[must_use]
pub fn text_width_spans(spans: &[TextSpan<'_>]) -> f32 {
    spans
        .iter()
        .filter(|span| !span.text.is_empty())
        .map(|span| text_width(span.text, span.style))
        .sum()
}

pub fn draw_text_spans(encoder: &mut dyn RenderEncoder, spans: &[TextSpan<'_>], x: f32, y: f32) {
    let mut cursor = x;
    for span in spans {
        if span.text.is_empty() {
            continue;
        }
        draw_text(encoder, span.text, cursor, y + span.y_offset, span.style);
        cursor += text_width(span.text, span.style);
    }
}

pub fn draw_text_spans_aligned(
    encoder: &mut dyn RenderEncoder,
    spans: &[TextSpan<'_>],
    rect: RectF,
    align: TextAlign,
) {
    let width = text_width_spans(spans);
    let x = aligned_x(rect, width, align);
    draw_text_spans(encoder, spans, x.max(rect.x), rect.y);
}

#[must_use]
pub fn resolve_text_with_placeholder<'a>(
    text: &'a str,
    placeholder: &'a str,
    text_style: TextStyle,
    placeholder_style: TextStyle,
) -> TextSpan<'a> {
    if text.is_empty() {
        TextSpan::new(placeholder, placeholder_style)
    } else {
        TextSpan::new(text, text_style)
    }
}

#[must_use]
pub fn text_width_with_placeholder(
    text: &str,
    placeholder: &str,
    text_style: TextStyle,
    placeholder_style: TextStyle,
) -> f32 {
    let span = resolve_text_with_placeholder(text, placeholder, text_style, placeholder_style);
    text_width(span.text, span.style)
}

pub fn draw_text_with_placeholder(
    encoder: &mut dyn RenderEncoder,
    text: &str,
    placeholder: &str,
    x: f32,
    y: f32,
    text_style: TextStyle,
    placeholder_style: TextStyle,
) {
    let span = resolve_text_with_placeholder(text, placeholder, text_style, placeholder_style);
    draw_text(encoder, span.text, x, y, span.style);
}

pub fn draw_text_with_placeholder_aligned(
    encoder: &mut dyn RenderEncoder,
    text: &str,
    placeholder: &str,
    rect: RectF,
    align: TextAlign,
    text_style: TextStyle,
    placeholder_style: TextStyle,
) {
    let span = resolve_text_with_placeholder(text, placeholder, text_style, placeholder_style);
    draw_text_aligned(encoder, span.text, rect, align, span.style);
}

pub fn draw_multiline(
    encoder: &mut dyn RenderEncoder,
    lines: &[String],
    rect: RectF,
    align: TextAlign,
    style: TextStyle,
) {
    let height = line_height(style);
    let mut cursor_y = rect.y;
    for line in lines {
        if cursor_y + height > rect.y + rect.h {
            break;
        }
        draw_text_aligned(
            encoder,
            line,
            RectF::new(rect.x, cursor_y, rect.w, height),
            align,
            style,
        );
        cursor_y += height;
    }
}

fn smooth_enabled(style: TextStyle) -> bool {
    if style.px < SMOOTH_MIN_PX {
        return false;
    }
    match SMOOTH_TEXT_STATE.lock() {
        Ok(state) => state.font_for_face(style.face).is_some(),
        Err(_) => false,
    }
}

fn aligned_x(rect: RectF, width: f32, align: TextAlign) -> f32 {
    match align {
        TextAlign::Left => rect.x,
        TextAlign::Center => rect.x + (rect.w - width) * 0.5,
        TextAlign::Right => rect.x + rect.w - width,
    }
}

fn smooth_text_width(text: &str, style: TextStyle) -> Option<f32> {
    if text.is_empty() {
        return Some(0.0);
    }
    if style.px < SMOOTH_MIN_PX {
        return None;
    }
    let state = SMOOTH_TEXT_STATE.lock().ok()?;
    let font = state.font_for_face(style.face)?;
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings::default());
    layout.append(&[font], &FontdueTextStyle::new(text, style.px * SMOOTH_OVERSAMPLE, 0));
    let mut width = 0.0_f32;
    for glyph in layout.glyphs() {
        width = width.max(glyph.x + glyph.width as f32);
    }
    Some(width / SMOOTH_OVERSAMPLE)
}

fn draw_text_smooth(
    encoder: &mut dyn RenderEncoder,
    text: &str,
    x: f32,
    y: f32,
    style: TextStyle,
) -> bool {
    if text.is_empty() {
        return true;
    }
    if style.px < SMOOTH_MIN_PX {
        return false;
    }
    let mut state = match SMOOTH_TEXT_STATE.lock() {
        Ok(state) => state,
        Err(_) => return false,
    };
    if state.font_for_face(style.face).is_none() {
        return false;
    }
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    let origin_x = (x * SMOOTH_OVERSAMPLE).round();
    let origin_y = (y * SMOOTH_OVERSAMPLE).round();
    layout.reset(&LayoutSettings { x: origin_x, y: origin_y, ..LayoutSettings::default() });
    let scaled_px = style.px * SMOOTH_OVERSAMPLE;
    {
        let font = state.font_for_face(style.face).expect("smooth font for style face");
        layout.append(&[font], &FontdueTextStyle::new(text, scaled_px, 0));
    }

    for glyph in layout.glyphs() {
        let Some(key) = RasterKey::new(style.face, glyph.key.glyph_index, glyph.key.px) else {
            continue;
        };
        if !state.glyphs.contains_key(&key) {
            let (metrics, bitmap) = {
                let font = state.font_for_face(style.face).expect("smooth font for style face");
                font.rasterize_indexed(glyph.key.glyph_index, glyph.key.px)
            };
            state.glyphs.insert(
                key,
                RasterGlyph { width: metrics.width, height: metrics.height, alpha: bitmap },
            );
        }
        let Some(raster) = state.glyphs.get(&key) else {
            continue;
        };
        if raster.width == 0 || raster.height == 0 {
            continue;
        }
        draw_raster_glyph(
            encoder,
            raster,
            glyph.x.round() / SMOOTH_OVERSAMPLE,
            glyph.y.round() / SMOOTH_OVERSAMPLE,
            style.color,
            1.0 / SMOOTH_OVERSAMPLE,
        );
    }
    true
}

fn draw_raster_glyph(
    encoder: &mut dyn RenderEncoder,
    glyph: &RasterGlyph,
    x: f32,
    y: f32,
    color: Color,
    px_step: f32,
) {
    for row in 0..glyph.height {
        let row_offset = row * glyph.width;
        let mut run_start: Option<usize> = None;
        let mut run_alpha = 0_u8;
        for col in 0..glyph.width {
            let alpha = quantize_alpha(glyph.alpha[row_offset + col]);
            match (run_start, alpha) {
                (None, 0) => {}
                (None, value) => {
                    run_start = Some(col);
                    run_alpha = value;
                }
                (Some(start), 0) => {
                    draw_raster_run(encoder, x, y, row, start, col, run_alpha, color, px_step);
                    run_start = None;
                }
                (Some(start), value) if value != run_alpha => {
                    draw_raster_run(encoder, x, y, row, start, col, run_alpha, color, px_step);
                    run_start = Some(col);
                    run_alpha = value;
                }
                _ => {}
            }
        }
        if let Some(start) = run_start {
            draw_raster_run(encoder, x, y, row, start, glyph.width, run_alpha, color, px_step);
        }
    }
}

fn draw_raster_run(
    encoder: &mut dyn RenderEncoder,
    x: f32,
    y: f32,
    row: usize,
    start_col: usize,
    end_col: usize,
    alpha: u8,
    color: Color,
    px_step: f32,
) {
    if end_col <= start_col || alpha == 0 {
        return;
    }
    let left = x + start_col as f32 * px_step;
    let top = y + row as f32 * px_step;
    let width = (end_col - start_col) as f32 * px_step;
    let quantized_alpha = (alpha as f32 / 255.0) * color.a;
    if quantized_alpha <= 0.0 {
        return;
    }
    draw_rect(
        encoder,
        left,
        top,
        width,
        px_step,
        Color::rgba(color.r, color.g, color.b, quantized_alpha),
    );
}

fn quantize_alpha(alpha: u8) -> u8 {
    alpha
}

fn draw_char(encoder: &mut dyn RenderEncoder, ch: char, x: f32, y: f32, pixel: f32, color: Color) {
    let Some(bitmap) = BASIC_FONTS.get(ch) else { return };
    for (row_index, row_bits) in bitmap.iter().copied().enumerate() {
        let row_y = y + row_index as f32 * pixel;
        let mut run_start: Option<usize> = None;
        for col in 0..8 {
            // font8x8 stores row bits in low->high order for left->right columns.
            // Reading high->low mirrors glyphs horizontally.
            let on = ((row_bits >> col) & 1) == 1;
            match (run_start, on) {
                (None, true) => run_start = Some(col),
                (Some(start), false) => {
                    draw_run(encoder, x, row_y, start, col, pixel, color);
                    run_start = None;
                }
                _ => {}
            }
        }
        if let Some(start) = run_start {
            draw_run(encoder, x, row_y, start, 8, pixel, color);
        }
    }
}

fn draw_run(
    encoder: &mut dyn RenderEncoder,
    x: f32,
    y: f32,
    start_col: usize,
    end_col: usize,
    pixel: f32,
    color: Color,
) {
    if end_col <= start_col {
        return;
    }
    let width = (end_col - start_col) as f32 * pixel;
    let left = x + start_col as f32 * pixel;
    draw_rect(encoder, left, y, width, pixel, color);
}

fn draw_rect(encoder: &mut dyn RenderEncoder, x: f32, y: f32, w: f32, h: f32, color: Color) {
    let verts = [
        Vertex { x, y, u: 0.0, v: 0.0, rgba: u32::MAX },
        Vertex { x: x + w, y, u: 1.0, v: 0.0, rgba: u32::MAX },
        Vertex { x, y: y + h, u: 0.0, v: 1.0, rgba: u32::MAX },
        Vertex { x: x + w, y: y + h, u: 1.0, v: 1.0, rgba: u32::MAX },
    ];
    encoder.draw_solid(&verts, color);
}

#[must_use]
fn pixel_size(style: TextStyle) -> f32 {
    (style.px / 8.0).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_renderer_api::{GlyphRun, ImageHandle, Insets, RectI};

    #[derive(Default)]
    struct CollectingEncoder {
        rects: alloc::vec::Vec<RectF>,
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

        fn draw_rrect(&mut self, _rect: RectF, _radii: [f32; 4], _color: Color) {}

        fn draw_nine_slice(
            &mut self,
            _img: ImageHandle,
            _rect: RectF,
            _slice: Insets,
            _alpha: f32,
        ) {
        }

        fn draw_backdrop(&mut self, _rect: RectF, _sigma: f32, _tint: Color, _alpha: f32) {}

        fn draw_spinner(&mut self, _center: [f32; 2], _atom: f32, _alpha: f32) {}

        fn draw_glyph_run(&mut self, _run: &GlyphRun) {}
    }

    #[test]
    fn draw_text_uses_lsb_left_to_right_bit_order() {
        // Keep this test on the bitmap fallback path.
        let style = TextStyle::new(5.0, Color::rgba(1.0, 1.0, 1.0, 1.0));
        let mut encoder = CollectingEncoder::default();
        draw_text(&mut encoder, "\\", 10.0, 20.0, style);
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
        let expected_left_col =
            (0..8).find(|col| ((row_bits >> col) & 1) == 1).expect("row has lit pixels");
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
        let span =
            resolve_text_with_placeholder("victor", "username", text_style, placeholder_style);
        assert_eq!(span, TextSpan::new("victor", text_style));
    }

    #[test]
    fn aligned_x_centers_width() {
        let rect = RectF::new(12.0, 20.0, 200.0, 24.0);
        let centered = aligned_x(rect, 60.0, TextAlign::Center);
        assert!((centered - 82.0).abs() < 0.001);
        let right = aligned_x(rect, 60.0, TextAlign::Right);
        assert!((right - 152.0).abs() < 0.001);
    }
}
