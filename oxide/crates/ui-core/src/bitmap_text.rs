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
use oxide_renderer_api::{
    Color, GlyphRun, ImageHandle, IndexSpan, RectF, RenderEncoder, Vertex, VertexSpan,
};
use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;

const SMOOTH_MIN_PX: f32 = 4.0;
const SMOOTH_OVERSAMPLE: f32 = 3.0;
const SMOOTH_TEXT_WIDTH_CACHE_MAX: usize = 4096;
const BITMAP_TEXT_ATLAS_WIDTH: u32 = 1024;
const BITMAP_TEXT_ATLAS_HEIGHT: u32 = 1024;
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextWidthKey {
    face: FontFace,
    px_tenths: u16,
    pixel_snapped: bool,
    text: String,
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

impl TextWidthKey {
    fn new(text: &str, style: TextStyle, pixel_snapped: bool) -> Option<Self> {
        if !style.px.is_finite() || style.px <= 0.0 {
            return None;
        }
        let px_tenths = (style.px * 10.0).round().clamp(1.0, u16::MAX as f32) as u16;
        Some(Self {
            face: style.face,
            px_tenths,
            pixel_snapped,
            text: text.to_owned(),
        })
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
static SMOOTH_TEXT_WIDTHS: Lazy<Mutex<HashMap<TextWidthKey, f32>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BitmapTextAtlasDirtyRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BitmapTextAtlasEntry {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

pub struct BitmapTextAtlas {
    data: Vec<u8>,
    dirty: Option<BitmapTextAtlasDirtyRect>,
    handle: Option<ImageHandle>,
    revision: u64,
    next_x: u32,
    row_y: u32,
    row_h: u32,
    glyphs: HashMap<RasterKey, BitmapTextAtlasEntry>,
    scratch_vertices: Vec<Vertex>,
    scratch_indices: Vec<u16>,
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

impl Default for BitmapTextAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl BitmapTextAtlas {
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: vec![0; BITMAP_TEXT_ATLAS_WIDTH as usize * BITMAP_TEXT_ATLAS_HEIGHT as usize],
            dirty: Some(BitmapTextAtlasDirtyRect {
                x: 0,
                y: 0,
                w: BITMAP_TEXT_ATLAS_WIDTH,
                h: BITMAP_TEXT_ATLAS_HEIGHT,
            }),
            handle: None,
            revision: 0,
            next_x: 1,
            row_y: 1,
            row_h: 0,
            glyphs: HashMap::new(),
            scratch_vertices: Vec::with_capacity(256),
            scratch_indices: Vec::with_capacity(384),
        }
    }

    #[must_use]
    pub fn image(&self) -> (&[u8], u32, u32) {
        (&self.data, BITMAP_TEXT_ATLAS_WIDTH, BITMAP_TEXT_ATLAS_HEIGHT)
    }

    #[must_use]
    pub fn dirty_rect(&self) -> Option<BitmapTextAtlasDirtyRect> {
        self.dirty
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = None;
    }

    #[must_use]
    pub fn handle(&self) -> Option<ImageHandle> {
        self.handle
    }

    pub fn set_handle(&mut self, handle: ImageHandle) {
        self.handle = Some(handle);
    }

    #[must_use]
    pub fn atlas_revision(&self) -> u64 {
        self.revision
    }

    pub fn draw_text(
        &mut self,
        encoder: &mut dyn RenderEncoder,
        text: &str,
        x: f32,
        y: f32,
        style: TextStyle,
        _device_scale: f32,
    ) -> bool {
        if text.is_empty() || style.px <= 0.0 || style.color.a <= 0.0 {
            return true;
        }
        if style.px < SMOOTH_MIN_PX {
            return false;
        }
        let Some(atlas_handle) = self.handle else {
            return false;
        };
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

        self.scratch_vertices.clear();
        self.scratch_indices.clear();
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
            let Some(entry) = self.ensure_glyph_entry(key, raster) else {
                return false;
            };
            if !self.push_glyph_quad(
                entry,
                glyph.x.round() / SMOOTH_OVERSAMPLE,
                glyph.y.round() / SMOOTH_OVERSAMPLE,
                raster.width as f32 / SMOOTH_OVERSAMPLE,
                raster.height as f32 / SMOOTH_OVERSAMPLE,
            ) {
                break;
            }
        }
        let run = GlyphRun {
            atlas: atlas_handle,
            atlas_revision: self.revision,
            vb: VertexSpan { offset: 0, len: self.scratch_vertices.len() as u32 },
            ib: IndexSpan { offset: 0, len: self.scratch_indices.len() as u32 },
            sdf: false,
            color: style.color,
        };
        if run.vb.len > 0 && run.ib.len > 0 {
            encoder.draw_glyph_run_resolved(&run, &self.scratch_vertices, &self.scratch_indices);
        }
        true
    }

    pub fn draw_text_or_fallback(
        &mut self,
        encoder: &mut dyn RenderEncoder,
        text: &str,
        x: f32,
        y: f32,
        style: TextStyle,
        device_scale: f32,
    ) {
        if !self.draw_text(encoder, text, x, y, style, device_scale) {
            draw_text(encoder, text, x, y, style);
        }
    }

    pub fn draw_text_aligned_or_fallback(
        &mut self,
        encoder: &mut dyn RenderEncoder,
        text: &str,
        rect: RectF,
        align: TextAlign,
        style: TextStyle,
        device_scale: f32,
    ) {
        let x = aligned_x(rect, text_width(text, style), align).max(rect.x);
        self.draw_text_or_fallback(encoder, text, x, rect.y, style, device_scale);
    }

    pub fn draw_text_spans_or_fallback(
        &mut self,
        encoder: &mut dyn RenderEncoder,
        spans: &[TextSpan<'_>],
        x: f32,
        y: f32,
        device_scale: f32,
    ) {
        let mut cursor = x;
        for span in spans {
            if span.text.is_empty() {
                continue;
            }
            self.draw_text_or_fallback(
                encoder,
                span.text,
                cursor,
                y + span.y_offset,
                span.style,
                device_scale,
            );
            cursor += text_width(span.text, span.style);
        }
    }

    pub fn draw_text_spans_aligned_or_fallback(
        &mut self,
        encoder: &mut dyn RenderEncoder,
        spans: &[TextSpan<'_>],
        rect: RectF,
        align: TextAlign,
        device_scale: f32,
    ) {
        let x = aligned_x(rect, text_width_spans(spans), align).max(rect.x);
        self.draw_text_spans_or_fallback(encoder, spans, x, rect.y, device_scale);
    }

    pub fn draw_multiline_or_fallback(
        &mut self,
        encoder: &mut dyn RenderEncoder,
        lines: &[String],
        rect: RectF,
        align: TextAlign,
        style: TextStyle,
        device_scale: f32,
    ) {
        let height = line_height(style);
        let mut cursor_y = rect.y;
        for line in lines {
            if cursor_y + height > rect.y + rect.h {
                break;
            }
            self.draw_text_aligned_or_fallback(
                encoder,
                line,
                RectF::new(rect.x, cursor_y, rect.w, height),
                align,
                style,
                device_scale,
            );
            cursor_y += height;
        }
    }

    fn ensure_glyph_entry(
        &mut self,
        key: RasterKey,
        raster: &RasterGlyph,
    ) -> Option<BitmapTextAtlasEntry> {
        if let Some(entry) = self.glyphs.get(&key).copied() {
            return Some(entry);
        }
        let w = u32::try_from(raster.width).ok()?;
        let h = u32::try_from(raster.height).ok()?;
        let (x, y) = self.alloc_rect(w, h)?;
        for row in 0..raster.height {
            let src = row.saturating_mul(raster.width);
            let dst = (y as usize)
                .saturating_add(row)
                .saturating_mul(BITMAP_TEXT_ATLAS_WIDTH as usize)
                .saturating_add(x as usize);
            let end = dst.saturating_add(raster.width).min(self.data.len());
            if end > dst {
                self.data[dst..end].copy_from_slice(&raster.alpha[src..src + end - dst]);
            }
        }
        self.mark_dirty(x, y, w, h);
        self.revision = self.revision.wrapping_add(1);
        let entry = BitmapTextAtlasEntry { x, y, w, h };
        self.glyphs.insert(key, entry);
        Some(entry)
    }

    fn alloc_rect(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w == 0 || h == 0 {
            return None;
        }
        if self.next_x.saturating_add(w).saturating_add(1) > BITMAP_TEXT_ATLAS_WIDTH {
            self.row_y = self.row_y.saturating_add(self.row_h).saturating_add(1);
            self.next_x = 1;
            self.row_h = 0;
        }
        if self.row_y.saturating_add(h).saturating_add(1) > BITMAP_TEXT_ATLAS_HEIGHT {
            return None;
        }
        let x = self.next_x;
        let y = self.row_y;
        self.next_x = self.next_x.saturating_add(w).saturating_add(1);
        self.row_h = self.row_h.max(h.saturating_add(1));
        Some((x, y))
    }

    fn mark_dirty(&mut self, x: u32, y: u32, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        let x0 = x.min(BITMAP_TEXT_ATLAS_WIDTH);
        let y0 = y.min(BITMAP_TEXT_ATLAS_HEIGHT);
        let x1 = x.saturating_add(w).min(BITMAP_TEXT_ATLAS_WIDTH);
        let y1 = y.saturating_add(h).min(BITMAP_TEXT_ATLAS_HEIGHT);
        let rect = BitmapTextAtlasDirtyRect {
            x: x0,
            y: y0,
            w: x1.saturating_sub(x0),
            h: y1.saturating_sub(y0),
        };
        if rect.w == 0 || rect.h == 0 {
            return;
        }
        self.dirty = Some(match self.dirty {
            Some(old) => {
                let min_x = old.x.min(rect.x);
                let min_y = old.y.min(rect.y);
                let max_x = old.x.saturating_add(old.w).max(rect.x.saturating_add(rect.w));
                let max_y = old.y.saturating_add(old.h).max(rect.y.saturating_add(rect.h));
                BitmapTextAtlasDirtyRect {
                    x: min_x,
                    y: min_y,
                    w: max_x
                        .saturating_sub(min_x)
                        .min(BITMAP_TEXT_ATLAS_WIDTH.saturating_sub(min_x)),
                    h: max_y
                        .saturating_sub(min_y)
                        .min(BITMAP_TEXT_ATLAS_HEIGHT.saturating_sub(min_y)),
                }
            }
            None => rect,
        });
    }

    fn push_glyph_quad(
        &mut self,
        entry: BitmapTextAtlasEntry,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> bool {
        let base = self.scratch_vertices.len();
        if base.saturating_add(4) > u16::MAX as usize {
            return false;
        }
        let base = base as u16;
        let atlas_w = BITMAP_TEXT_ATLAS_WIDTH as f32;
        let atlas_h = BITMAP_TEXT_ATLAS_HEIGHT as f32;
        let u0 = entry.x as f32 / atlas_w;
        let v0 = entry.y as f32 / atlas_h;
        let u1 = entry.x.saturating_add(entry.w) as f32 / atlas_w;
        let v1 = entry.y.saturating_add(entry.h) as f32 / atlas_h;
        self.scratch_vertices.extend_from_slice(&[
            Vertex { x, y, u: u0, v: v0, rgba: u32::MAX },
            Vertex { x: x + w, y, u: u1, v: v0, rgba: u32::MAX },
            Vertex { x, y: y + h, u: u0, v: v1, rgba: u32::MAX },
            Vertex { x: x + w, y: y + h, u: u1, v: v1, rgba: u32::MAX },
        ]);
        self.scratch_indices.extend_from_slice(&[
            base,
            base + 1,
            base + 2,
            base + 2,
            base + 1,
            base + 3,
        ]);
        true
    }
}

#[must_use]
pub fn line_height(style: TextStyle) -> f32 {
    if smooth_enabled(style) {
        (style.px * 1.20).max(1.0)
    } else {
        pixel_size(style) * 10.0
    }
}

#[must_use]
pub fn text_width(text: &str, style: TextStyle) -> f32 {
    if let Some(width) = smooth_text_width(text, style, false) {
        return width;
    }
    text_width_bitmap(text, style)
}

#[must_use]
pub fn text_width_pixel_snapped(text: &str, style: TextStyle) -> f32 {
    if let Some(width) = smooth_text_width(text, style, true) {
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

fn smooth_text_width(text: &str, style: TextStyle, pixel_snapped: bool) -> Option<f32> {
    if text.is_empty() {
        return Some(0.0);
    }
    if style.px < SMOOTH_MIN_PX {
        return None;
    }
    let key = TextWidthKey::new(text, style, pixel_snapped)?;
    if let Ok(cache) = SMOOTH_TEXT_WIDTHS.lock() {
        if let Some(width) = cache.get(&key) {
            return Some(*width);
        }
    }
    let state = SMOOTH_TEXT_STATE.lock().ok()?;
    let font = state.font_for_face(style.face)?;
    let scaled_px = style.px * SMOOTH_OVERSAMPLE;
    let mut width = 0.0_f32;
    for ch in text.chars() {
        if ch == '\n' {
            continue;
        }
        let glyph_index = font.lookup_glyph_index(ch);
        let metrics = font.metrics_indexed(glyph_index, scaled_px);
        width += if pixel_snapped { metrics.advance_width.ceil() } else { metrics.advance_width };
    }
    let width = width / SMOOTH_OVERSAMPLE;
    if let Ok(mut cache) = SMOOTH_TEXT_WIDTHS.lock() {
        if cache.len() >= SMOOTH_TEXT_WIDTH_CACHE_MAX {
            cache.clear();
        }
        cache.insert(key, width);
    }
    Some(width)
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
