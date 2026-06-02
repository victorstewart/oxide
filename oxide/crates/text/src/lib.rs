//! `Oxide` text system: shaping, atlas packing, and quad generation.
#![allow(clippy::all, clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::cast_precision_loss
)]

use std::collections::HashMap;
use std::ops::Range;

use oxide_renderer_api as api;
use rustybuzz::{Face as RbFace, GlyphBuffer as RbGlyphs, UnicodeBuffer};
use swash::scale::{image::Image, ScaleContext};
#[allow(unused_imports)]
use swash::scale::{Render, Source};
use ttf_parser::Face as TtfFace;
use unicode_segmentation::UnicodeSegmentation;

pub struct Font {
    data: std::sync::Arc<Vec<u8>>,
}

impl Font {
    #[must_use]
    pub fn from_bytes(data: Vec<u8>) -> Self {
        Self { data: std::sync::Arc::new(data) }
    }

    #[must_use]
    pub fn supports_cluster(&self, cluster: &str) -> bool {
        let Ok(face) = TtfFace::parse(&self.data, 0) else {
            return false;
        };
        cluster.chars().all(|ch| cluster_char_supported(&face, ch))
    }
}

pub struct FontDb {
    fonts: Vec<Font>,
}

impl Default for FontDb {
    fn default() -> Self {
        Self { fonts: Vec::new() }
    }
}

impl FontDb {
    pub fn add_font(&mut self, f: Font) -> usize {
        self.fonts.push(f);
        self.fonts.len() - 1
    }
    pub fn font(&self, id: usize) -> Option<&Font> {
        self.fonts.get(id)
    }
    pub fn font_supports_cluster(&self, id: usize, cluster: &str) -> bool {
        self.font(id).map_or(false, |font| font.supports_cluster(cluster))
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct GlyphKey {
    font: usize,
    gid: u16,
    px: u16,
    sdf: bool,
}

#[derive(Clone, Debug)]
struct GlyphAtlasEntry {
    u: u16,
    v: u16,
    w: u16,
    h: u16,
    l: i16,
    t: i16,
    last_used: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasDirtyRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

pub struct Atlas {
    width: u32,
    height: u32,
    data: Vec<u8>, // A8 coverage
    next_x: u32,
    row_y: u32,
    row_h: u32,
    map: HashMap<GlyphKey, GlyphAtlasEntry>,
    clock: u64,
    dirty: Option<AtlasDirtyRect>,
    evictions: u64,
    revision: u64,
}

impl Atlas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; (width * height) as usize],
            next_x: 1,
            row_y: 1,
            row_h: 0,
            map: HashMap::new(),
            clock: 0,
            dirty: None,
            evictions: 0,
            revision: 0,
        }
    }

    pub fn image(&self) -> (&[u8], u32, u32) {
        (&self.data, self.width, self.height)
    }

    #[inline]
    pub fn dirty_rect(&self) -> Option<AtlasDirtyRect> {
        self.dirty
    }

    #[inline]
    pub fn clear_dirty(&mut self) {
        self.dirty = None;
    }

    pub fn reset(&mut self) {
        self.clear_storage();
        self.evictions = 0;
        self.revision = self.revision.wrapping_add(1);
    }

    #[inline]
    pub fn glyph_count(&self) -> usize {
        self.map.len()
    }

    #[inline]
    pub fn eviction_count(&self) -> u64 {
        self.evictions
    }

    #[inline]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn clear_storage(&mut self) {
        self.data.fill(0);
        self.next_x = 1;
        self.row_y = 1;
        self.row_h = 0;
        self.map.clear();
        self.mark_dirty(0, 0, self.width, self.height);
    }

    #[inline]
    fn can_fit_rect(&self, w: u32, h: u32) -> bool {
        w > 0 && h > 0 && w.saturating_add(2) <= self.width && h.saturating_add(2) <= self.height
    }

    fn evict_rect_for(&mut self, w: u32, h: u32, protect_after_clock: u64) -> Option<(u32, u32)> {
        if !self.can_fit_rect(w, h) {
            return None;
        }
        let key = self
            .map
            .iter()
            .filter(|(_, entry)| {
                let fits = (entry.w as u32) >= w && (entry.h as u32) >= h;
                entry.last_used <= protect_after_clock && fits
            })
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(key, _)| *key)?;
        let entry = self.map.remove(&key)?;
        self.evictions = self.evictions.wrapping_add(1);
        self.revision = self.revision.wrapping_add(1);
        for row in 0..entry.h as usize {
            let y = entry.v as usize + row;
            let x = entry.u as usize;
            let off = y.saturating_mul(self.width as usize).saturating_add(x);
            let end = off.saturating_add(entry.w as usize).min(self.data.len());
            if off < end {
                self.data[off..end].fill(0);
            }
        }
        self.mark_dirty(entry.u as u32, entry.v as u32, entry.w as u32, entry.h as u32);
        Some((entry.u as u32, entry.v as u32))
    }

    fn alloc_rect(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w == 0 || h == 0 {
            return None;
        }
        if self.next_x + w + 1 > self.width {
            self.row_y += self.row_h + 1;
            self.next_x = 1;
            self.row_h = 0;
        }
        if self.row_y + h + 1 > self.height {
            return None;
        }
        let x = self.next_x;
        let y = self.row_y;
        self.next_x += w + 1;
        self.row_h = self.row_h.max(h + 1);
        Some((x, y))
    }

    fn mark_dirty(&mut self, x: u32, y: u32, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        let x1 = x.saturating_add(w).min(self.width);
        let y1 = y.saturating_add(h).min(self.height);
        let rect = AtlasDirtyRect {
            x: x.min(self.width),
            y: y.min(self.height),
            w: x1.saturating_sub(x.min(self.width)),
            h: y1.saturating_sub(y.min(self.height)),
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
                AtlasDirtyRect {
                    x: min_x,
                    y: min_y,
                    w: max_x.saturating_sub(min_x).min(self.width.saturating_sub(min_x)),
                    h: max_y.saturating_sub(min_y).min(self.height.saturating_sub(min_y)),
                }
            }
            None => rect,
        });
    }
}

pub struct TextShaper {}

impl Default for TextShaper {
    fn default() -> Self {
        Self {}
    }
}

#[derive(Clone, Debug)]
pub struct FallbackShapeRun {
    pub font_id: usize,
    pub byte_range: Range<usize>,
    pub x_offset: f32,
    pub shape: OwnedShape,
}

#[derive(Clone, Debug)]
pub struct FallbackShape {
    pub runs: Vec<FallbackShapeRun>,
    width: f32,
    rtl: bool,
}

impl FallbackShape {
    #[inline]
    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn bake_into_with(
        &self,
        fonts: &FontDb,
        raster: &mut RasterCtx,
        atlas: &mut Atlas,
        draw_vertices: &mut Vec<api::Vertex>,
        draw_indices: &mut Vec<u16>,
        color: api::Color,
        atlas_handle: api::ImageHandle,
        origin_x: f32,
        origin_y: f32,
        device_scale: f32,
    ) -> api::GlyphRun {
        let v_start = draw_vertices.len() as u32;
        let i_start = draw_indices.len() as u32;
        let mut sdf = false;
        for run in &self.runs {
            let Some(font) = fonts.font(run.font_id) else {
                continue;
            };
            let before_v = draw_vertices.len();
            let before_i = draw_indices.len();
            let glyph_run = run.shape.bake_into_with(
                font,
                raster,
                atlas,
                draw_vertices,
                draw_indices,
                color,
                atlas_handle,
                origin_x + run.x_offset,
                origin_y,
                device_scale,
            );
            let base = glyph_run.vb.offset.saturating_sub(v_start);
            if base.saturating_add(glyph_run.vb.len) > u16::MAX as u32 {
                draw_vertices.truncate(before_v);
                draw_indices.truncate(before_i);
                break;
            }
            if base > 0 {
                let base = base as u16;
                for index in &mut draw_indices[before_i..] {
                    *index = index.saturating_add(base);
                }
            }
            sdf = glyph_run.sdf;
        }
        let v_end = draw_vertices.len() as u32;
        let i_end = draw_indices.len() as u32;
        api::GlyphRun {
            atlas: atlas_handle,
            atlas_revision: atlas.revision(),
            vb: api::VertexSpan { offset: v_start, len: v_end - v_start },
            ib: api::IndexSpan { offset: i_start, len: i_end - i_start },
            sdf,
            color,
        }
    }

    pub fn cursor_map_for_text(&self, text: &str) -> ShapedCursorMap {
        let boundaries = grapheme_byte_boundaries(text);
        let mut widths = vec![0.0_f32; boundaries.len()];
        for run in &self.runs {
            if run.byte_range.start > run.byte_range.end || run.byte_range.end > text.len() {
                continue;
            }
            let local = &text[run.byte_range.clone()];
            let local_boundaries = grapheme_byte_boundaries(local);
            let local_widths = run.shape.prefix_widths_for_boundaries(&local_boundaries);
            for (local_index, local_byte) in local_boundaries.iter().copied().enumerate() {
                let global_byte = run.byte_range.start.saturating_add(local_byte);
                let bucket = match boundaries.binary_search(&global_byte) {
                    Ok(bucket) => bucket,
                    Err(bucket) => bucket.min(boundaries.len().saturating_sub(1)),
                };
                if let Some(width) = widths.get_mut(bucket) {
                    *width = run.x_offset + local_widths.get(local_index).copied().unwrap_or(0.0);
                }
            }
        }
        let (downstream, upstream) =
            caret_positions_from_text_prefix_widths(text, &boundaries, widths, self.rtl);
        ShapedCursorMap::from_boundaries_and_affinity_widths(boundaries, downstream, upstream)
    }
}

impl TextShaper {
    pub fn shape<'a>(
        &mut self,
        font: &'a Font,
        font_id: usize,
        text: &str,
        px: f32,
    ) -> anyhow::Result<ShapeOutput<'a>> {
        let rb_face = RbFace::from_slice(&font.data, 0).ok_or_else(|| anyhow::anyhow!("face"))?;
        let mut buf = UnicodeBuffer::new();
        buf.push_str(text);
        let glyphs = rustybuzz::shape(&rb_face, &[], buf);
        Ok(ShapeOutput { font, font_id, glyphs, px, rtl: text_base_direction_is_rtl(text) })
    }

    pub fn shape_with_fallback_fonts(
        &mut self,
        fonts: &FontDb,
        primary_id: usize,
        fallback_ids: &[usize],
        text: &str,
        px: f32,
    ) -> Option<FallbackShape> {
        if text.is_empty() {
            return Some(FallbackShape {
                runs: Vec::new(),
                width: 0.0,
                rtl: text_base_direction_is_rtl(text),
            });
        }
        let boundaries = grapheme_byte_boundaries(text);
        let mut runs = Vec::with_capacity(fallback_ids.len().saturating_add(1));
        let mut run_font = primary_id;
        let mut run_start = 0usize;
        let mut pen = 0.0_f32;
        let mut first = true;

        for pair in boundaries.windows(2) {
            let cluster = &text[pair[0]..pair[1]];
            let font_id = fallback_font_for_cluster(fonts, primary_id, fallback_ids, cluster);
            if first {
                run_font = font_id;
                run_start = pair[0];
                first = false;
            } else if font_id != run_font {
                pen = self.append_fallback_shape_run(
                    fonts, run_font, text, run_start, pair[0], px, pen, &mut runs,
                )?;
                run_font = font_id;
                run_start = pair[0];
            }
        }
        let width = self.append_fallback_shape_run(
            fonts,
            run_font,
            text,
            run_start,
            text.len(),
            px,
            pen,
            &mut runs,
        )?;
        Some(FallbackShape { runs, width, rtl: text_base_direction_is_rtl(text) })
    }

    pub fn cursor_map_with_fallback_fonts(
        &mut self,
        fonts: &FontDb,
        primary_id: usize,
        fallback_ids: &[usize],
        text: &str,
        px: f32,
    ) -> Option<ShapedCursorMap> {
        Some(
            self.shape_with_fallback_fonts(fonts, primary_id, fallback_ids, text, px)?
                .cursor_map_for_text(text),
        )
    }

    fn append_fallback_shape_run(
        &mut self,
        fonts: &FontDb,
        font_id: usize,
        text: &str,
        start: usize,
        end: usize,
        px: f32,
        pen: f32,
        runs: &mut Vec<FallbackShapeRun>,
    ) -> Option<f32> {
        if start >= end {
            return Some(pen);
        }
        let font = fonts.font(font_id)?;
        let run = &text[start..end];
        let shape = self.shape(font, font_id, run, px).ok()?.to_owned_shape();
        let width = shape.width();
        runs.push(FallbackShapeRun { font_id, byte_range: start..end, x_offset: pen, shape });
        Some(pen + width)
    }
}

pub struct RasterCtx {
    scale: ScaleContext,
}

impl Default for RasterCtx {
    fn default() -> Self {
        Self { scale: ScaleContext::new() }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ShapedGlyph {
    glyph_id: u16,
    cluster: usize,
    x_advance: i32,
    y_advance: i32,
}

#[derive(Clone, Debug)]
pub struct ShapedCursorMap {
    byte_boundaries: Vec<usize>,
    widths: Vec<f32>,
    upstream_widths: Vec<f32>,
    order: CursorMapOrder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaretAffinity {
    Upstream,
    Downstream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CursorMapOrder {
    Ascending,
    Descending,
    Mixed,
}

impl ShapedCursorMap {
    #[must_use]
    pub fn from_text_and_widths(text: &str, widths: Vec<f32>) -> Self {
        Self::from_boundaries_and_widths(grapheme_byte_boundaries(text), widths)
    }

    #[must_use]
    pub fn from_boundaries_and_widths(
        mut byte_boundaries: Vec<usize>,
        mut widths: Vec<f32>,
    ) -> Self {
        if byte_boundaries.is_empty() {
            byte_boundaries.push(0);
        }
        normalize_cursor_widths(&mut widths, byte_boundaries.len());
        let order = classify_cursor_positions(&widths);
        Self { byte_boundaries, upstream_widths: widths.clone(), widths, order }
    }

    #[must_use]
    fn from_boundaries_and_affinity_widths(
        mut byte_boundaries: Vec<usize>,
        mut downstream_widths: Vec<f32>,
        mut upstream_widths: Vec<f32>,
    ) -> Self {
        if byte_boundaries.is_empty() {
            byte_boundaries.push(0);
        }
        normalize_cursor_widths(&mut downstream_widths, byte_boundaries.len());
        normalize_cursor_widths(&mut upstream_widths, byte_boundaries.len());
        let order = if cursor_affinity_widths_match(&downstream_widths, &upstream_widths) {
            classify_cursor_positions(&downstream_widths)
        } else {
            CursorMapOrder::Mixed
        };
        Self { byte_boundaries, widths: downstream_widths, upstream_widths, order }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.byte_boundaries.len().saturating_sub(1)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[must_use]
    pub fn byte_boundaries(&self) -> &[usize] {
        &self.byte_boundaries
    }

    #[must_use]
    pub fn widths(&self) -> &[f32] {
        &self.widths
    }

    #[must_use]
    pub fn byte_index(&self, cursor: usize) -> usize {
        self.byte_boundaries
            .get(cursor.min(self.len()))
            .copied()
            .unwrap_or_else(|| self.byte_boundaries.last().copied().unwrap_or(0))
    }

    #[must_use]
    pub fn byte_range(&self, range: Range<usize>) -> Range<usize> {
        self.byte_index(range.start)..self.byte_index(range.end)
    }

    #[must_use]
    pub fn width_at(&self, cursor: usize) -> f32 {
        self.width_at_with_affinity(cursor, CaretAffinity::Downstream)
    }

    #[must_use]
    pub fn width_at_with_affinity(&self, cursor: usize, affinity: CaretAffinity) -> f32 {
        let positions = match affinity {
            CaretAffinity::Upstream => &self.upstream_widths,
            CaretAffinity::Downstream => &self.widths,
        };
        positions
            .get(cursor.min(self.len()))
            .copied()
            .unwrap_or_else(|| positions.last().copied().unwrap_or(0.0))
    }

    #[must_use]
    pub fn cursor_for_x(&self, x: f32) -> usize {
        if self.is_empty() {
            return 0;
        }
        let last = self.len().min(self.widths.len().saturating_sub(1));
        let target = x.max(0.0);
        let positions = &self.widths[..=last];
        match self.order {
            CursorMapOrder::Ascending => return nearest_cursor_ascending(positions, target),
            CursorMapOrder::Descending => return nearest_cursor_descending(positions, target),
            CursorMapOrder::Mixed => {}
        }
        if !cursor_affinity_widths_match(&self.widths[..=last], &self.upstream_widths[..=last]) {
            return nearest_cursor_with_any_affinity(
                &self.widths[..=last],
                &self.upstream_widths[..=last],
                target,
            );
        }

        let mut best = 0usize;
        let mut best_dist = f32::INFINITY;
        for (index, width) in positions.iter().copied().enumerate() {
            let dist = (width - target).abs();
            if dist < best_dist {
                best = index;
                best_dist = dist;
            }
        }
        best
    }

    #[must_use]
    pub fn cursor_for_x_with_affinity(&self, x: f32, affinity: CaretAffinity) -> usize {
        if self.is_empty() {
            return 0;
        }
        let last = self.len().min(self.widths.len().saturating_sub(1));
        let target = x.max(0.0);
        let positions = match affinity {
            CaretAffinity::Upstream => &self.upstream_widths[..=last],
            CaretAffinity::Downstream => &self.widths[..=last],
        };
        nearest_cursor_for_affinity(positions, target, affinity)
    }
}

fn normalize_cursor_widths(widths: &mut Vec<f32>, len: usize) {
    if widths.len() < len {
        let last = widths.last().copied().unwrap_or(0.0);
        widths.resize(len, last);
    } else if widths.len() > len {
        widths.truncate(len);
    }
    if widths.is_empty() {
        widths.push(0.0);
    }
    if !widths[0].is_finite() {
        widths[0] = 0.0;
    }
    let mut last = widths[0];
    for width in widths.iter_mut().skip(1) {
        if width.is_finite() {
            last = *width;
        } else {
            *width = last;
        }
    }
}

fn cursor_affinity_widths_match(downstream: &[f32], upstream: &[f32]) -> bool {
    downstream.len() == upstream.len()
        && downstream
            .iter()
            .zip(upstream.iter())
            .all(|(left, right)| (*left - *right).abs() <= f32::EPSILON)
}

fn classify_cursor_positions(positions: &[f32]) -> CursorMapOrder {
    let mut ascending = true;
    let mut descending = true;
    for pair in positions.windows(2) {
        if pair[0] > pair[1] {
            ascending = false;
        }
        if pair[0] < pair[1] {
            descending = false;
        }
    }
    if ascending {
        CursorMapOrder::Ascending
    } else if descending {
        CursorMapOrder::Descending
    } else {
        CursorMapOrder::Mixed
    }
}

fn nearest_cursor_ascending(positions: &[f32], target: f32) -> usize {
    let last = positions.len().saturating_sub(1);
    let upper = positions.partition_point(|width| *width < target);
    let mut best = upper.min(last);
    if upper > 0 {
        let prior = upper - 1;
        let best_dist = (positions[best] - target).abs();
        let prior_dist = (positions[prior] - target).abs();
        if prior_dist <= best_dist {
            best = prior;
        }
    }
    best
}

fn nearest_cursor_descending(positions: &[f32], target: f32) -> usize {
    let last = positions.len().saturating_sub(1);
    let upper = positions.partition_point(|width| *width > target);
    let mut best = upper.min(last);
    if upper > 0 {
        let prior = upper - 1;
        let best_dist = (positions[best] - target).abs();
        let prior_dist = (positions[prior] - target).abs();
        if prior_dist <= best_dist {
            best = prior;
        }
    }
    best
}

fn nearest_cursor_for_affinity(positions: &[f32], target: f32, affinity: CaretAffinity) -> usize {
    let prefer_high_on_tie = matches!(affinity, CaretAffinity::Upstream);
    let mut best = 0usize;
    let mut best_dist = f32::INFINITY;
    for (index, width) in positions.iter().copied().enumerate() {
        let dist = (width - target).abs();
        if dist < best_dist
            || (dist == best_dist
                && ((prefer_high_on_tie && index > best) || (!prefer_high_on_tie && index < best)))
        {
            best = index;
            best_dist = dist;
        }
    }
    best
}

fn nearest_cursor_with_any_affinity(downstream: &[f32], upstream: &[f32], target: f32) -> usize {
    let mut best = 0usize;
    let mut best_dist = f32::INFINITY;
    for index in 0..downstream.len().min(upstream.len()) {
        for width in [downstream[index], upstream[index]] {
            let dist = (width - target).abs();
            if dist < best_dist || (dist == best_dist && index < best) {
                best = index;
                best_dist = dist;
            }
        }
    }
    best
}

#[derive(Clone, Debug)]
pub struct OwnedShape {
    font_id: usize,
    glyphs: Vec<ShapedGlyph>,
    px: f32,
    width: f32,
    rtl: bool,
}

impl OwnedShape {
    #[inline]
    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn prefix_widths_for_boundaries(&self, boundaries: &[usize]) -> Vec<f32> {
        prefix_widths_from_clusters(
            boundaries,
            self.glyphs.iter().map(|glyph| (glyph.cluster, glyph.x_advance as f32 / 64.0)),
        )
    }

    pub fn cursor_map_for_text(&self, text: &str) -> ShapedCursorMap {
        let boundaries = grapheme_byte_boundaries(text);
        let widths = self.prefix_widths_for_boundaries(&boundaries);
        let (downstream, upstream) =
            caret_positions_from_text_prefix_widths(text, &boundaries, widths, self.rtl);
        ShapedCursorMap::from_boundaries_and_affinity_widths(boundaries, downstream, upstream)
    }

    pub fn cursor_map_for_boundaries(&self, byte_boundaries: Vec<usize>) -> ShapedCursorMap {
        let widths = caret_positions_from_prefix_widths(
            self.prefix_widths_for_boundaries(&byte_boundaries),
            self.rtl,
        );
        ShapedCursorMap::from_boundaries_and_widths(byte_boundaries, widths)
    }

    pub fn bake_into_with(
        &self,
        font: &Font,
        raster: &mut RasterCtx,
        atlas: &mut Atlas,
        draw_vertices: &mut Vec<api::Vertex>,
        draw_indices: &mut Vec<u16>,
        color: api::Color,
        atlas_handle: api::ImageHandle,
        origin_x: f32,
        origin_y: f32,
        device_scale: f32,
    ) -> api::GlyphRun {
        if self.rtl && !clusters_are_descending(self.glyphs.iter().map(|glyph| glyph.cluster)) {
            let mut glyphs = self.glyphs.clone();
            glyphs.reverse();
            return bake_glyphs_into(
                font,
                self.font_id,
                self.px,
                &glyphs,
                raster,
                atlas,
                draw_vertices,
                draw_indices,
                color,
                atlas_handle,
                origin_x,
                origin_y,
                device_scale,
            );
        }
        bake_glyphs_into(
            font,
            self.font_id,
            self.px,
            &self.glyphs,
            raster,
            atlas,
            draw_vertices,
            draw_indices,
            color,
            atlas_handle,
            origin_x,
            origin_y,
            device_scale,
        )
    }
}

pub struct ShapeOutput<'a> {
    font: &'a Font,
    font_id: usize,
    glyphs: RbGlyphs,
    px: f32,
    rtl: bool,
}

impl<'a> ShapeOutput<'a> {
    #[inline]
    pub fn width(&self) -> f32 {
        let mut width = 0.0_f32;
        for p in self.glyphs.glyph_positions() {
            width += p.x_advance as f32 / 64.0;
        }
        width
    }

    pub fn prefix_widths_for_boundaries(&self, boundaries: &[usize]) -> Vec<f32> {
        let glyphs = self.logical_glyphs();
        prefix_widths_from_clusters(
            boundaries,
            glyphs.iter().map(|glyph| (glyph.cluster, glyph.x_advance as f32 / 64.0)),
        )
    }

    pub fn cursor_map_for_text(&self, text: &str) -> ShapedCursorMap {
        let boundaries = grapheme_byte_boundaries(text);
        let widths = self.prefix_widths_for_boundaries(&boundaries);
        let (downstream, upstream) =
            caret_positions_from_text_prefix_widths(text, &boundaries, widths, self.rtl);
        ShapedCursorMap::from_boundaries_and_affinity_widths(boundaries, downstream, upstream)
    }

    pub fn cursor_map_for_boundaries(&self, byte_boundaries: Vec<usize>) -> ShapedCursorMap {
        let widths = caret_positions_from_prefix_widths(
            self.prefix_widths_for_boundaries(&byte_boundaries),
            self.rtl,
        );
        ShapedCursorMap::from_boundaries_and_widths(byte_boundaries, widths)
    }

    pub fn to_owned_shape(&self) -> OwnedShape {
        let glyphs = self.logical_glyphs();
        let width = glyphs.iter().map(|glyph| glyph.x_advance as f32 / 64.0).sum();
        OwnedShape { font_id: self.font_id, glyphs, px: self.px, width, rtl: self.rtl }
    }

    fn raw_glyphs(&self) -> Vec<ShapedGlyph> {
        let infos = self.glyphs.glyph_infos();
        let poss = self.glyphs.glyph_positions();
        let mut glyphs = Vec::with_capacity(infos.len());
        for (info, pos) in infos.iter().zip(poss.iter()) {
            glyphs.push(ShapedGlyph {
                glyph_id: (info.glyph_id as u32) as u16,
                cluster: info.cluster as usize,
                x_advance: pos.x_advance,
                y_advance: pos.y_advance,
            });
        }
        glyphs
    }

    fn logical_glyphs(&self) -> Vec<ShapedGlyph> {
        let mut glyphs = self.raw_glyphs();
        if self.rtl && clusters_are_descending(glyphs.iter().map(|glyph| glyph.cluster)) {
            glyphs.reverse();
        }
        glyphs
    }

    fn visual_glyphs(&self) -> Vec<ShapedGlyph> {
        let mut glyphs = self.raw_glyphs();
        if self.rtl && !clusters_are_descending(glyphs.iter().map(|glyph| glyph.cluster)) {
            glyphs.reverse();
        }
        glyphs
    }

    pub fn bake_into(
        &self,
        atlas: &mut Atlas,
        draw_vertices: &mut Vec<api::Vertex>,
        draw_indices: &mut Vec<u16>,
        color: api::Color,
        atlas_handle: api::ImageHandle,
        origin_x: f32,
        origin_y: f32,
        device_scale: f32,
    ) -> api::GlyphRun {
        let mut raster = RasterCtx::default();
        self.bake_into_with(
            &mut raster,
            atlas,
            draw_vertices,
            draw_indices,
            color,
            atlas_handle,
            origin_x,
            origin_y,
            device_scale,
        )
    }

    pub fn bake_into_with(
        &self,
        raster: &mut RasterCtx,
        atlas: &mut Atlas,
        draw_vertices: &mut Vec<api::Vertex>,
        draw_indices: &mut Vec<u16>,
        color: api::Color,
        atlas_handle: api::ImageHandle,
        origin_x: f32,
        origin_y: f32,
        device_scale: f32,
    ) -> api::GlyphRun {
        let glyphs = self.visual_glyphs();
        bake_glyphs_into(
            self.font,
            self.font_id,
            self.px,
            &glyphs,
            raster,
            atlas,
            draw_vertices,
            draw_indices,
            color,
            atlas_handle,
            origin_x,
            origin_y,
            device_scale,
        )
    }
}

fn clusters_are_descending<I>(clusters: I) -> bool
where
    I: IntoIterator<Item = usize>,
{
    let mut prior = None;
    let mut descending = false;
    for cluster in clusters {
        if let Some(prior_cluster) = prior {
            if prior_cluster < cluster {
                return false;
            }
            if prior_cluster > cluster {
                descending = true;
            }
        }
        prior = Some(cluster);
    }
    descending
}

fn caret_positions_from_prefix_widths(mut widths: Vec<f32>, rtl: bool) -> Vec<f32> {
    if rtl {
        let total = widths.last().copied().unwrap_or(0.0);
        for width in &mut widths {
            *width = (total - *width).max(0.0);
        }
    }
    widths
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextDirection {
    Ltr,
    Rtl,
}

fn caret_positions_from_text_prefix_widths(
    text: &str,
    boundaries: &[usize],
    mut widths: Vec<f32>,
    rtl: bool,
) -> (Vec<f32>, Vec<f32>) {
    normalize_cursor_widths(&mut widths, boundaries.len());
    let directions = resolved_grapheme_directions(text, boundaries, rtl);
    if directions.len() + 1 != boundaries.len()
        || directions.windows(2).all(|pair| pair[0] == pair[1])
    {
        let positions = caret_positions_from_prefix_widths(widths, rtl);
        return (positions.clone(), positions);
    }

    let total_width = widths.last().copied().unwrap_or(0.0).max(0.0);
    let mut advances = Vec::with_capacity(directions.len());
    for index in 0..directions.len() {
        let start = widths.get(index).copied().unwrap_or(0.0);
        let end = widths.get(index + 1).copied().unwrap_or(start);
        advances.push((end - start).max(0.0));
    }

    let mut downstream = vec![f32::NAN; boundaries.len()];
    let mut upstream = vec![f32::NAN; boundaries.len()];
    let mut run_start_index = 0usize;
    let mut visual_pen = if rtl { total_width } else { 0.0 };
    while run_start_index < directions.len() {
        let run_dir = directions[run_start_index];
        let mut run_end_index = run_start_index + 1;
        while run_end_index < directions.len() && directions[run_end_index] == run_dir {
            run_end_index += 1;
        }

        let run_width: f32 = advances[run_start_index..run_end_index].iter().sum();
        let visual_start = if rtl {
            let start = (visual_pen - run_width).max(0.0);
            visual_pen = start;
            start
        } else {
            let start = visual_pen;
            visual_pen += run_width;
            start
        };

        let mut local_pen = 0.0_f32;
        for offset in 0..=run_end_index - run_start_index {
            let cursor = run_start_index + offset;
            let x = match run_dir {
                TextDirection::Ltr => visual_start + local_pen,
                TextDirection::Rtl => visual_start + run_width - local_pen,
            };
            if offset < run_end_index - run_start_index {
                downstream[cursor] = x;
                local_pen += advances[run_start_index + offset];
            }
            if offset > 0 {
                upstream[cursor] = x;
            }
        }

        run_start_index = run_end_index;
    }

    for index in 0..boundaries.len() {
        if !downstream[index].is_finite() {
            downstream[index] = if upstream[index].is_finite() { upstream[index] } else { 0.0 };
        }
        if !upstream[index].is_finite() {
            upstream[index] = downstream[index];
        }
    }

    (downstream, upstream)
}

fn resolved_grapheme_directions(text: &str, boundaries: &[usize], rtl: bool) -> Vec<TextDirection> {
    let base = if rtl { TextDirection::Rtl } else { TextDirection::Ltr };
    let mut current = base;
    let mut directions = Vec::with_capacity(boundaries.len().saturating_sub(1));
    for pair in boundaries.windows(2) {
        let cluster = &text[pair[0]..pair[1]];
        if let Some(direction) = grapheme_strong_direction(cluster) {
            current = direction;
        }
        directions.push(current);
    }
    directions
}

fn grapheme_strong_direction(cluster: &str) -> Option<TextDirection> {
    for ch in cluster.chars() {
        if is_rtl_strong(ch) {
            return Some(TextDirection::Rtl);
        }
        if is_ltr_strong(ch) {
            return Some(TextDirection::Ltr);
        }
    }
    None
}

fn fallback_font_for_cluster(
    fonts: &FontDb,
    primary_id: usize,
    fallback_ids: &[usize],
    cluster: &str,
) -> usize {
    if fonts.font_supports_cluster(primary_id, cluster) {
        return primary_id;
    }
    for fallback_id in fallback_ids {
        if fonts.font_supports_cluster(*fallback_id, cluster) {
            return *fallback_id;
        }
    }
    primary_id
}

fn cluster_char_supported(face: &TtfFace<'_>, ch: char) -> bool {
    ch.is_control()
        || ch.is_whitespace()
        || matches!(ch as u32, 0x200C..=0x200D | 0xFE00..=0xFE0F)
        || face.glyph_index(ch).is_some()
}

fn text_base_direction_is_rtl(text: &str) -> bool {
    for ch in text.chars() {
        if is_rtl_strong(ch) {
            return true;
        }
        if is_ltr_strong(ch) {
            return false;
        }
    }
    false
}

fn is_ltr_strong(ch: char) -> bool {
    ch.is_ascii_alphabetic()
        || matches!(
            ch as u32,
            0x0041..=0x005A
                | 0x0061..=0x007A
                | 0x00C0..=0x02AF
                | 0x0370..=0x03FF
                | 0x0400..=0x052F
        )
}

fn is_rtl_strong(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0590..=0x08FF
            | 0xFB1D..=0xFDFF
            | 0xFE70..=0xFEFF
            | 0x10800..=0x10FFF
            | 0x1E800..=0x1EFFF
    )
}

fn prefix_widths_from_clusters<I>(boundaries: &[usize], glyphs: I) -> Vec<f32>
where
    I: IntoIterator<Item = (usize, f32)>,
{
    let mut widths = vec![0.0_f32; boundaries.len()];
    if boundaries.len() <= 1 {
        return widths;
    }

    let last = boundaries.last().copied().unwrap_or(0);
    let mut pending: Option<(usize, f32)> = None;

    for (cluster, advance) in glyphs {
        let cluster = cluster.min(last);
        if let Some((start, width)) = pending.as_mut() {
            if cluster == *start {
                *width += advance;
                continue;
            }
            let end = if cluster > *start { cluster } else { last };
            add_prefix_width(&mut widths, boundaries, end, *width);
        }
        pending = Some((cluster, advance));
    }

    if let Some((_, width)) = pending {
        add_prefix_width(&mut widths, boundaries, last, width);
    }

    let mut pen = 0.0_f32;
    for width in &mut widths {
        pen += *width;
        *width = pen;
    }
    widths[0] = 0.0;
    widths
}

fn grapheme_byte_boundaries(text: &str) -> Vec<usize> {
    if text.is_ascii() {
        let mut boundaries = Vec::with_capacity(text.len() + 1);
        for index in 0..=text.len() {
            boundaries.push(index);
        }
        return boundaries;
    }
    let mut boundaries = Vec::new();
    for (index, _) in UnicodeSegmentation::grapheme_indices(text, true) {
        boundaries.push(index);
    }
    boundaries.push(text.len());
    boundaries
}

fn add_prefix_width(widths: &mut [f32], boundaries: &[usize], end: usize, width: f32) {
    let bucket = match boundaries.binary_search(&end) {
        Ok(bucket) => bucket.min(widths.len() - 1),
        Err(bucket) => bucket.min(widths.len() - 1),
    };
    widths[bucket] += width;
}

fn bake_glyphs_into(
    font: &Font,
    font_id: usize,
    px: f32,
    glyphs: &[ShapedGlyph],
    raster: &mut RasterCtx,
    atlas: &mut Atlas,
    draw_vertices: &mut Vec<api::Vertex>,
    draw_indices: &mut Vec<u16>,
    color: api::Color,
    atlas_handle: api::ImageHandle,
    origin_x: f32,
    origin_y: f32,
    device_scale: f32,
) -> api::GlyphRun {
    let mut pen_x: f32 = 0.0;
    let mut pen_y: f32 = 0.0;
    let scale = if device_scale > 0.0 { device_scale } else { 1.0 };
    let ox = (origin_x * scale).round() / scale;
    let oy = (origin_y * scale).round() / scale;

    let v_start = draw_vertices.len() as u32;
    let i_start = draw_indices.len() as u32;
    let use_sdf = (px * device_scale) >= 24.0;
    let mut scaler = None;
    let mut render = None;
    let protect_after_clock = atlas.clock;

    for glyph in glyphs.iter().copied() {
        let key =
            GlyphKey { font: font_id, gid: glyph.glyph_id, px: px.round() as u16, sdf: use_sdf };
        let entry = if let Some(e) = atlas.map.get_mut(&key) {
            e.last_used = atlas.clock.wrapping_add(1);
            atlas.clock = e.last_used;
            e.clone()
        } else {
            let mut img = Image::new();
            if scaler.is_none() {
                let Some(fontref) = swash::FontRef::from_index(&font.data, 0) else {
                    pen_x += glyph.x_advance as f32 / 64.0;
                    pen_y += glyph.y_advance as f32 / 64.0;
                    continue;
                };
                scaler = Some(raster.scale.builder(fontref).size(px).hint(true).build());
            }
            let Some(scaler) = scaler.as_mut() else {
                pen_x += glyph.x_advance as f32 / 64.0;
                pen_y += glyph.y_advance as f32 / 64.0;
                continue;
            };
            let render = render.get_or_insert_with(|| Render::new(&[Source::Outline]));
            if !render.render_into(scaler, glyph.glyph_id, &mut img) {
                cache_empty_glyph_entry(atlas, key, 0, 0);
                pen_x += glyph.x_advance as f32 / 64.0;
                pen_y += glyph.y_advance as f32 / 64.0;
                continue;
            }
            let w = img.placement.width.max(0);
            let h = img.placement.height.max(0);
            if w == 0 || h == 0 {
                cache_empty_glyph_entry(
                    atlas,
                    key,
                    img.placement.left as i16,
                    img.placement.top as i16,
                );
                pen_x += glyph.x_advance as f32 / 64.0;
                pen_y += glyph.y_advance as f32 / 64.0;
                continue;
            }
            let (aw, ah) = (w as u32, h as u32);
            let (ax, ay) = match atlas
                .alloc_rect(aw, ah)
                .or_else(|| atlas.evict_rect_for(aw, ah, protect_after_clock))
            {
                Some(rc) => rc,
                None => {
                    pen_x += glyph.x_advance as f32 / 64.0;
                    pen_y += glyph.y_advance as f32 / 64.0;
                    continue;
                }
            };
            if use_sdf {
                let mut sdf_row = vec![0u8; aw as usize];
                let spread: i32 = 8;
                for yy in 0..ah as i32 {
                    for xx in 0..aw as i32 {
                        let idx = (yy as usize) * (aw as usize) + (xx as usize);
                        let a = img.data[idx] as f32 / 255.0;
                        let inside = a > 0.5;
                        let mut min_d2 = ((spread + 1) * (spread + 1)) as i32;
                        for dy in -spread..=spread {
                            let y2 = yy + dy;
                            if y2 < 0 || y2 >= ah as i32 {
                                continue;
                            }
                            for dx in -spread..=spread {
                                let x2 = xx + dx;
                                if x2 < 0 || x2 >= aw as i32 {
                                    continue;
                                }
                                let j = (y2 as usize) * (aw as usize) + (x2 as usize);
                                let a2 = img.data[j] as f32 / 255.0;
                                let inside2 = a2 > 0.5;
                                if inside2 != inside {
                                    let d2 = dx * dx + dy * dy;
                                    if d2 < min_d2 {
                                        min_d2 = d2;
                                    }
                                }
                            }
                        }
                        let dist = (min_d2 as f32).sqrt();
                        let sd = if inside { dist } else { -dist };
                        let v = (0.5 + sd / (2.0 * spread as f32)).clamp(0.0, 1.0);
                        sdf_row[xx as usize] = (v * 255.0).round() as u8;
                    }
                    let dst_y = (ay as usize) + (yy as usize);
                    let dst_off = (dst_y * (atlas.width as usize)) + (ax as usize);
                    atlas.data[dst_off..dst_off + (aw as usize)].copy_from_slice(&sdf_row);
                }
            } else {
                for row in 0..ah as usize {
                    let src_off = row * (aw as usize);
                    let dst_y = (ay as usize) + row;
                    let dst_off = (dst_y * (atlas.width as usize)) + (ax as usize);
                    atlas.data[dst_off..dst_off + (aw as usize)]
                        .copy_from_slice(&img.data[src_off..src_off + (aw as usize)]);
                }
            }
            atlas.mark_dirty(ax, ay, aw, ah);
            let e = GlyphAtlasEntry {
                u: ax as u16,
                v: ay as u16,
                w: aw as u16,
                h: ah as u16,
                l: img.placement.left as i16,
                t: img.placement.top as i16,
                last_used: atlas.clock.wrapping_add(1),
            };
            atlas.clock = e.last_used;
            atlas.map.insert(key, e.clone());
            e
        };
        if entry.w == 0 || entry.h == 0 {
            pen_x += glyph.x_advance as f32 / 64.0;
            pen_y += glyph.y_advance as f32 / 64.0;
            continue;
        }

        let gx = ox + pen_x + (entry.l as f32);
        let gy = oy + pen_y - (entry.t as f32);
        let gw = entry.w as f32;
        let gh = entry.h as f32;

        let run_vertex_base = (draw_vertices.len() as u32).saturating_sub(v_start);
        if run_vertex_base.saturating_add(4) > u16::MAX as u32 {
            break;
        }

        let atlas_w = atlas.width.max(1) as f32;
        let atlas_h = atlas.height.max(1) as f32;
        let u0 = (entry.u as f32) / atlas_w;
        let v0 = (entry.v as f32) / atlas_h;
        let u1 = (entry.u as f32 + entry.w as f32) / atlas_w;
        let v1 = (entry.v as f32 + entry.h as f32) / atlas_h;
        let rgba = pack_rgba(color);

        push_v(draw_vertices, gx, gy, u0, v0, rgba);
        push_v(draw_vertices, gx + gw, gy, u1, v0, rgba);
        push_v(draw_vertices, gx, gy + gh, u0, v1, rgba);
        push_v(draw_vertices, gx + gw, gy + gh, u1, v1, rgba);
        push_i(draw_indices, run_vertex_base + 0, run_vertex_base + 1, run_vertex_base + 2);
        push_i(draw_indices, run_vertex_base + 2, run_vertex_base + 1, run_vertex_base + 3);

        pen_x += glyph.x_advance as f32 / 64.0;
        pen_y += glyph.y_advance as f32 / 64.0;
    }

    let v_end = draw_vertices.len() as u32;
    let i_end = draw_indices.len() as u32;
    api::GlyphRun {
        atlas: atlas_handle,
        atlas_revision: atlas.revision(),
        vb: api::VertexSpan { offset: v_start, len: v_end - v_start },
        ib: api::IndexSpan { offset: i_start, len: i_end - i_start },
        sdf: use_sdf,
        color,
    }
}

fn cache_empty_glyph_entry(atlas: &mut Atlas, key: GlyphKey, left: i16, top: i16) {
    let e = GlyphAtlasEntry {
        u: 0,
        v: 0,
        w: 0,
        h: 0,
        l: left,
        t: top,
        last_used: atlas.clock.wrapping_add(1),
    };
    atlas.clock = e.last_used;
    atlas.map.insert(key, e);
}

fn pack_rgba(color: api::Color) -> u32 {
    let red = (color.r.clamp(0.0, 1.0) * 255.0).round() as u32;
    let green = (color.g.clamp(0.0, 1.0) * 255.0).round() as u32;
    let blue = (color.b.clamp(0.0, 1.0) * 255.0).round() as u32;
    let alpha = (color.a.clamp(0.0, 1.0) * 255.0).round() as u32;
    (alpha << 24) | (blue << 16) | (green << 8) | red
}

fn push_v(verts: &mut Vec<api::Vertex>, x: f32, y: f32, u: f32, v: f32, rgba: u32) {
    verts.push(api::Vertex { x, y, u, v, rgba });
}

fn push_i(indices: &mut Vec<u16>, a: u32, b: u32, c: u32) {
    indices.push(a as u16);
    indices.push(b as u16);
    indices.push(c as u16);
}
