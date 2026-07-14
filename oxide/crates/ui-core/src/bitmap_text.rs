//! Deterministic atlas text for UI overlays and fallback labels.
//!
//! Rendering is explicitly context-owned: callers upload one A8 atlas and text
//! emits `GlyphRun` commands. The old per-alpha-run solid renderer is retained
//! only by tests as a reference.

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
        Some(Self { face: style.face, px_tenths, pixel_snapped, text: text.to_owned() })
    }
}

struct SmoothTextState {
    regular: Option<Font>,
    bold: Option<Font>,
    italic: Option<Font>,
}

impl SmoothTextState {
    fn new() -> Self {
        let (regular, bold, italic) = load_host_fonts();
        Self { regular, bold, italic }
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
    smooth: SmoothTextState,
    layout: Layout,
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
            smooth: SmoothTextState::new(),
            layout: Layout::new(CoordinateSystem::PositiveYDown),
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
        if self.smooth.font_for_face(style.face).is_none() {
            return false;
        }
        let origin_x = (x * SMOOTH_OVERSAMPLE).round();
        let origin_y = (y * SMOOTH_OVERSAMPLE).round();
        self.layout.reset(
            &LayoutSettings { x: origin_x, y: origin_y, ..LayoutSettings::default() },
        );
        let scaled_px = style.px * SMOOTH_OVERSAMPLE;
        {
            let font =
                self.smooth.font_for_face(style.face).expect("smooth font for style face");
            self.layout.append(&[font], &FontdueTextStyle::new(text, scaled_px, 0));
        }

        self.scratch_vertices.clear();
        self.scratch_indices.clear();
        for index in 0..self.layout.glyphs().len() {
            let (glyph_index, glyph_px, glyph_x, glyph_y) = {
                let glyph = &self.layout.glyphs()[index];
                (glyph.key.glyph_index, glyph.key.px, glyph.x, glyph.y)
            };
            let Some(key) = RasterKey::new(style.face, glyph_index, glyph_px) else {
                continue;
            };
            let entry = if let Some(entry) = self.glyphs.get(&key).copied() {
                entry
            } else {
                let (metrics, bitmap) = {
                    let font = self
                        .smooth
                        .font_for_face(style.face)
                        .expect("smooth font for style face");
                    font.rasterize_indexed(glyph_index, glyph_px)
                };
                if metrics.width == 0 || metrics.height == 0 {
                    self.glyphs.insert(key, BitmapTextAtlasEntry { x: 0, y: 0, w: 0, h: 0 });
                    continue;
                }
                let Some(entry) = ensure_glyph_entry(
                    &mut self.data,
                    &mut self.dirty,
                    &mut self.revision,
                    &mut self.next_x,
                    &mut self.row_y,
                    &mut self.row_h,
                    &mut self.glyphs,
                    key,
                    metrics.width,
                    metrics.height,
                    &bitmap,
                ) else {
                    return false;
                };
                entry
            };
            if entry.w == 0 || entry.h == 0 {
                continue;
            }
            if !self.push_glyph_quad(
                entry,
                glyph_x.round() / SMOOTH_OVERSAMPLE,
                glyph_y.round() / SMOOTH_OVERSAMPLE,
                entry.w as f32 / SMOOTH_OVERSAMPLE,
                entry.h as f32 / SMOOTH_OVERSAMPLE,
            ) {
                return false;
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

    pub fn draw_text_aligned(
        &mut self,
        encoder: &mut dyn RenderEncoder,
        text: &str,
        rect: RectF,
        align: TextAlign,
        style: TextStyle,
        device_scale: f32,
    ) -> bool {
        let x = aligned_x(rect, self.measure_text(text, style), align).max(rect.x);
        self.draw_text(encoder, text, x, rect.y, style, device_scale)
    }

    pub fn draw_text_spans(
        &mut self,
        encoder: &mut dyn RenderEncoder,
        spans: &[TextSpan<'_>],
        x: f32,
        y: f32,
        device_scale: f32,
    ) -> bool {
        let mut cursor = x;
        for span in spans {
            if span.text.is_empty() {
                continue;
            }
            if !self.draw_text(
                encoder,
                span.text,
                cursor,
                y + span.y_offset,
                span.style,
                device_scale,
            ) {
                return false;
            }
            cursor += self.measure_text(span.text, span.style);
        }
        true
    }

    pub fn draw_text_spans_aligned(
        &mut self,
        encoder: &mut dyn RenderEncoder,
        spans: &[TextSpan<'_>],
        rect: RectF,
        align: TextAlign,
        device_scale: f32,
    ) -> bool {
        let width = spans
            .iter()
            .filter(|span| !span.text.is_empty())
            .map(|span| self.measure_text(span.text, span.style))
            .sum();
        let x = aligned_x(rect, width, align).max(rect.x);
        self.draw_text_spans(encoder, spans, x, rect.y, device_scale)
    }

    pub fn draw_multiline(
        &mut self,
        encoder: &mut dyn RenderEncoder,
        lines: &[String],
        rect: RectF,
        align: TextAlign,
        style: TextStyle,
        device_scale: f32,
    ) -> bool {
        let height = self.line_height(style);
        let mut cursor_y = rect.y;
        for line in lines {
            if cursor_y + height > rect.y + rect.h {
                break;
            }
            if !self.draw_text_aligned(
                encoder,
                line,
                RectF::new(rect.x, cursor_y, rect.w, height),
                align,
                style,
                device_scale,
            ) {
                return false;
            }
            cursor_y += height;
        }
        true
    }

    #[must_use]
    pub fn line_height(&self, style: TextStyle) -> f32 {
        if style.px >= SMOOTH_MIN_PX && self.smooth.font_for_face(style.face).is_some() {
            (style.px * 1.20).max(1.0)
        } else {
            pixel_size(style) * 10.0
        }
    }

    fn measure_text(&self, text: &str, style: TextStyle) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        if style.px < SMOOTH_MIN_PX {
            return text_width_bitmap(text, style);
        }
        let Some(font) = self.smooth.font_for_face(style.face) else {
            return text_width_bitmap(text, style);
        };
        let scaled_px = style.px * SMOOTH_OVERSAMPLE;
        text.chars()
            .filter(|ch| *ch != '\n')
            .map(|ch| {
                let glyph_index = font.lookup_glyph_index(ch);
                font.metrics_indexed(glyph_index, scaled_px).advance_width
            })
            .sum::<f32>()
            / SMOOTH_OVERSAMPLE
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

fn ensure_glyph_entry(
    data: &mut [u8],
    dirty: &mut Option<BitmapTextAtlasDirtyRect>,
    revision: &mut u64,
    next_x: &mut u32,
    row_y: &mut u32,
    row_h: &mut u32,
    glyphs: &mut HashMap<RasterKey, BitmapTextAtlasEntry>,
    key: RasterKey,
    width: usize,
    height: usize,
    alpha: &[u8],
) -> Option<BitmapTextAtlasEntry> {
    if let Some(entry) = glyphs.get(&key).copied() {
        return Some(entry);
    }
    let w = u32::try_from(width).ok()?;
    let h = u32::try_from(height).ok()?;
    let (x, y) = alloc_rect(next_x, row_y, row_h, w, h)?;
    for row in 0..height {
        let src = row.saturating_mul(width);
        let dst = (y as usize)
            .saturating_add(row)
            .saturating_mul(BITMAP_TEXT_ATLAS_WIDTH as usize)
            .saturating_add(x as usize);
        let end = dst.saturating_add(width).min(data.len());
        if end > dst {
            data[dst..end].copy_from_slice(&alpha[src..src + end - dst]);
        }
    }
    mark_dirty(dirty, x, y, w, h);
    *revision = revision.wrapping_add(1);
    let entry = BitmapTextAtlasEntry { x, y, w, h };
    glyphs.insert(key, entry);
    Some(entry)
}

fn alloc_rect(
    next_x: &mut u32,
    row_y: &mut u32,
    row_h: &mut u32,
    w: u32,
    h: u32,
) -> Option<(u32, u32)> {
    if w == 0 || h == 0 {
        return None;
    }
    if next_x.saturating_add(w).saturating_add(1) > BITMAP_TEXT_ATLAS_WIDTH {
        *row_y = row_y.saturating_add(*row_h).saturating_add(1);
        *next_x = 1;
        *row_h = 0;
    }
    if row_y.saturating_add(h).saturating_add(1) > BITMAP_TEXT_ATLAS_HEIGHT {
        return None;
    }
    let x = *next_x;
    let y = *row_y;
    *next_x = next_x.saturating_add(w).saturating_add(1);
    *row_h = (*row_h).max(h.saturating_add(1));
    Some((x, y))
}

fn mark_dirty(
    dirty: &mut Option<BitmapTextAtlasDirtyRect>,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) {
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
    *dirty = Some(match *dirty {
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

#[must_use]
pub fn text_width_spans(spans: &[TextSpan<'_>]) -> f32 {
    spans
        .iter()
        .filter(|span| !span.text.is_empty())
        .map(|span| text_width(span.text, span.style))
        .sum()
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

#[must_use]
fn pixel_size(style: TextStyle) -> f32 {
    (style.px / 8.0).max(1.0)
}
