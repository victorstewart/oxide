//! Basic UI elements: Label, ProgressBar, Spinner
#![allow(clippy::module_name_repetitions)]

use crate::{text_boundary, text_fields::text_floating_placeholder_tick, DrawListBuilder};
use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Range;
use oxide_platform_api::{
    clipboard, AutoCapitalization, KeyCode, KeyEvent, KeyboardAppearance, Modifiers, ReturnKeyType,
    TextContentType, TextEvent, TextInputConfig,
};
use oxide_renderer_api as gfx;
use oxide_text as text;
use oxide_timing as timing;
use std::collections::HashMap;

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn watch_event_log_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        env_flag("OXIDE_PERF_WATCH_LOG_EVENTS")
            || env_flag("OXIDE_PERF_WATCH_MODE")
            || env_flag("OXIDE_RUST_LOG")
    })
}

fn watch_text_fragment(text: &str) -> String {
    const LIMIT: usize = 40;
    let mut fragment = String::new();
    let mut count = 0usize;
    for ch in text.chars() {
        if count == LIMIT {
            fragment.push_str("...");
            break;
        }
        fragment.push(ch);
        count += 1;
    }
    fragment
}

fn watch_text_event(event: &str, font_id: usize, text: &str, detail: &str) {
    if watch_event_log_enabled() {
        std::eprintln!(
            "oxide.watch: text event={} font_id={} text=\"{}\" {}",
            event,
            font_id,
            watch_text_fragment(text),
            detail
        );
    }
}

#[inline]
fn watch_text_event_lazy<F>(event: &str, font_id: usize, text: &str, detail: F)
where
    F: FnOnce() -> String,
{
    if watch_event_log_enabled() {
        std::eprintln!(
            "oxide.watch: text event={} font_id={} text=\"{}\" {}",
            event,
            font_id,
            watch_text_fragment(text),
            detail()
        );
    }
}

// ----- Text integration -----

pub trait ImageUploader {
    fn create_a8(&mut self, w: u32, h: u32, data: &[u8], row_bytes: usize) -> gfx::ImageHandle;
    fn update_a8(
        &mut self,
        handle: gfx::ImageHandle,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        data: &[u8],
        row_bytes: usize,
    );

    fn append_a8(
        &mut self,
        handle: gfx::ImageHandle,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        data: &[u8],
        row_bytes: usize,
    ) {
        self.update_a8(handle, x, y, w, h, data, row_bytes);
    }

    fn release_a8(&mut self, _handle: gfx::ImageHandle) {}
}

const LABEL_LAYOUT_CACHE_CAP: usize = 2_048;
const LABEL_LAYOUT_CACHE_PRUNE_TARGET: usize = LABEL_LAYOUT_CACHE_CAP / 2;
const TEXT_PREFIX_CACHE_CAP: usize = 512;
const TEXT_PREFIX_CACHE_PRUNE_TARGET: usize = TEXT_PREFIX_CACHE_CAP / 2;
const WRAP_WIDTH_CONFIRM_EPS: f32 = 8.0;

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct LabelLayoutStyleKey {
    font_id: usize,
    font_px_bits: u32,
    wrap: bool,
    max_w_bits: u32,
}

struct CachedLabelRun {
    font_id: usize,
    x_offset: f32,
    shape: text::OwnedShape,
}

enum CachedLabelShape {
    Single(CachedLabelRun),
    Fallback(text::FallbackShape),
}

struct CachedLabelLine {
    width: f32,
    shape: CachedLabelShape,
}

struct CachedLabelLayout {
    lines: alloc::vec::Vec<CachedLabelLine>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextFrameStats {
    pub visible_labels: u64,
    pub shaping_calls: u64,
    pub rasterizations: u64,
    pub layout_cache_hits: u64,
    pub layout_cache_misses: u64,
    pub glyph_cache_hits: u64,
    pub glyph_cache_misses: u64,
    pub atlas_upload_calls: u64,
    pub atlas_upload_pixels: u64,
    pub atlas_upload_bytes: u64,
    pub atlas_evictions: u64,
    pub invalidated_runs: u64,
}

#[derive(Clone, Copy, Default)]
struct TextCounterSnapshot {
    shaping_calls: u64,
    rasterizations: u64,
    layout_cache_hits: u64,
    layout_cache_misses: u64,
    glyph_cache_hits: u64,
    glyph_cache_misses: u64,
    atlas_upload_calls: u64,
    atlas_upload_pixels: u64,
    atlas_upload_bytes: u64,
    atlas_evictions: u64,
    invalidated_runs: u64,
}

#[derive(Default)]
struct TextProfiler {
    frame_counters: TextCounterSnapshot,
    last_frame_stats: TextFrameStats,
    shaping_calls: u64,
    layout_cache_hits: u64,
    layout_cache_misses: u64,
    atlas_upload_calls: u64,
    atlas_upload_pixels: u64,
    atlas_upload_bytes: u64,
    invalidated_runs: u64,
}

#[derive(Default)]
struct TextFrameState {
    glyph_runs: Vec<gfx::GlyphRun>,
    page_generations: Vec<(u32, u64)>,
    retired_pages: Vec<(gfx::ImageHandle, u32)>,
    profiler: Option<TextProfiler>,
}

#[derive(Clone, Copy)]
struct TextGpuPage {
    id: u32,
    generation: u64,
    handle: gfx::ImageHandle,
    pristine: bool,
}

struct CachedLabelLayoutEntry {
    layout: Arc<CachedLabelLayout>,
    last_used: u64,
}

struct CachedTextPrefixMetrics {
    map: text::ShapedCursorMap,
}

struct CachedTextPrefixEntry {
    metrics: Arc<CachedTextPrefixMetrics>,
    last_used: u64,
}

fn cache_f32_bits(value: f32) -> u32 {
    if value == 0.0 {
        0
    } else {
        value.to_bits()
    }
}

impl LabelLayoutStyleKey {
    fn new(font_id: usize, font_px: f32, wrap: bool, max_w: f32) -> Self {
        Self {
            font_id,
            font_px_bits: cache_f32_bits(font_px),
            wrap,
            max_w_bits: if wrap { cache_f32_bits(max_w) } else { 0 },
        }
    }
}

fn cached_label_line_from_owned_shape(font_id: usize, shape: text::OwnedShape) -> CachedLabelLine {
    CachedLabelLine {
        width: shape.width(),
        shape: CachedLabelShape::Single(CachedLabelRun { font_id, x_offset: 0.0, shape }),
    }
}

fn ascii_wrap_word_ranges(text_value: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::with_capacity(16);
    let mut offset = 0usize;
    for segment in text_value.split_inclusive(' ') {
        let trailing_spaces = segment.as_bytes().iter().rev().take_while(|b| **b == b' ').count();
        let word_len = segment.len().saturating_sub(trailing_spaces);
        if word_len > 0 {
            ranges.push((offset, offset + word_len));
        }
        offset = offset.saturating_add(segment.len());
    }
    ranges
}

pub struct TextCtx {
    pub fonts: text::FontDb,
    pub shaper: text::TextShaper,
    pub raster: text::RasterCtx,
    pub atlas: text::PagedAtlas,
    pub atlas_handle: Option<gfx::ImageHandle>,
    gpu_pages: Vec<TextGpuPage>,
    retained_atlas_revisions: Vec<(gfx::ImageHandle, u64)>,
    label_layouts:
        HashMap<LabelLayoutStyleKey, HashMap<alloc::string::String, CachedLabelLayoutEntry>>,
    label_layout_len: usize,
    text_prefixes:
        HashMap<LabelLayoutStyleKey, HashMap<alloc::string::String, CachedTextPrefixEntry>>,
    text_prefix_len: usize,
    label_layout_clock: u64,
    fallback_fonts: Vec<usize>,
    frame_active: bool,
    frame: Box<TextFrameState>,
}

impl Default for TextCtx {
    fn default() -> Self {
        Self {
            fonts: text::FontDb::default(),
            shaper: text::TextShaper::default(),
            raster: text::RasterCtx::default(),
            atlas: text::PagedAtlas::new(1024, 1024, text::DEFAULT_GLYPH_ATLAS_PAGE_COUNT),
            atlas_handle: None,
            gpu_pages: Vec::with_capacity(text::DEFAULT_GLYPH_ATLAS_PAGE_COUNT),
            retained_atlas_revisions:
                Vec::with_capacity(text::DEFAULT_GLYPH_ATLAS_PAGE_COUNT),
            label_layouts: HashMap::new(),
            label_layout_len: 0,
            text_prefixes: HashMap::new(),
            text_prefix_len: 0,
            label_layout_clock: 0,
            fallback_fonts: Vec::new(),
            frame_active: false,
            frame: Box::new(TextFrameState {
                glyph_runs: Vec::with_capacity(8),
                page_generations: Vec::with_capacity(text::DEFAULT_GLYPH_ATLAS_PAGE_COUNT),
                retired_pages: Vec::with_capacity(text::DEFAULT_GLYPH_ATLAS_PAGE_COUNT),
                profiler: None,
            }),
        }
    }
}

impl TextCtx {
    fn counter_snapshot(&self) -> TextCounterSnapshot {
        let Some(profiler) = self.frame.profiler.as_ref() else {
            return TextCounterSnapshot::default();
        };
        TextCounterSnapshot {
            shaping_calls: profiler.shaping_calls,
            rasterizations: self.atlas.rasterization_count(),
            layout_cache_hits: profiler.layout_cache_hits,
            layout_cache_misses: profiler.layout_cache_misses,
            glyph_cache_hits: self.atlas.glyph_cache_hits(),
            glyph_cache_misses: self.atlas.glyph_cache_misses(),
            atlas_upload_calls: profiler.atlas_upload_calls,
            atlas_upload_pixels: profiler.atlas_upload_pixels,
            atlas_upload_bytes: profiler.atlas_upload_bytes,
            atlas_evictions: self.atlas.eviction_count(),
            invalidated_runs: profiler.invalidated_runs,
        }
    }

    pub fn begin_frame(&mut self) {
        if self.frame_active {
            self.atlas.end_frame();
        }
        if self.frame.profiler.is_some() {
            let counters = self.counter_snapshot();
            if let Some(profiler) = self.frame.profiler.as_mut() {
                profiler.frame_counters = counters;
                profiler.last_frame_stats = TextFrameStats::default();
            }
        }
        self.frame_active = true;
        self.frame.page_generations.clear();
        for page in &self.gpu_pages {
            self.frame.page_generations.push((page.id, page.generation));
        }
        self.atlas.begin_frame();
    }

    pub fn finish_frame<U: ImageUploader>(
        &mut self,
        up: &mut U,
        builder: &mut DrawListBuilder,
    ) -> TextFrameStats {
        if !self.frame_active {
            return self.last_frame_stats();
        }
        self.publish_gpu_pages(up, true);
        self.patch_builder_atlas_pages(builder);
        self.atlas.end_frame();
        self.frame_active = false;
        if self.frame.profiler.is_none() {
            return TextFrameStats::default();
        }
        let end = self.counter_snapshot();
        let Some(profiler) = self.frame.profiler.as_mut() else {
            return TextFrameStats::default();
        };
        let start = profiler.frame_counters;
        let layout_cache_hits = end.layout_cache_hits.saturating_sub(start.layout_cache_hits);
        let layout_cache_misses = end.layout_cache_misses.saturating_sub(start.layout_cache_misses);
        profiler.last_frame_stats = TextFrameStats {
            visible_labels: layout_cache_hits.saturating_add(layout_cache_misses),
            shaping_calls: end.shaping_calls.saturating_sub(start.shaping_calls),
            rasterizations: end.rasterizations.saturating_sub(start.rasterizations),
            layout_cache_hits,
            layout_cache_misses,
            glyph_cache_hits: end.glyph_cache_hits.saturating_sub(start.glyph_cache_hits),
            glyph_cache_misses: end.glyph_cache_misses.saturating_sub(start.glyph_cache_misses),
            atlas_upload_calls: end.atlas_upload_calls.saturating_sub(start.atlas_upload_calls),
            atlas_upload_pixels: end
                .atlas_upload_pixels
                .saturating_sub(start.atlas_upload_pixels),
            atlas_upload_bytes: end.atlas_upload_bytes.saturating_sub(start.atlas_upload_bytes),
            atlas_evictions: end.atlas_evictions.saturating_sub(start.atlas_evictions),
            invalidated_runs: end.invalidated_runs.saturating_sub(start.invalidated_runs),
        };
        profiler.last_frame_stats
    }

    fn patch_builder_atlas_pages(&mut self, builder: &mut DrawListBuilder) {
        let count_invalidations = self.frame_active;
        let mut invalidated_runs = 0_u64;
        for item in &mut builder.drawlist_mut().items {
            let gfx::DrawCmd::GlyphRun { run } = item else {
                continue;
            };
            let (page_id, retired) = if run.atlas.0 == 0 {
                (run.atlas_revision as u32, false)
            } else if let Some((_, page_id)) = self
                .frame
                .retired_pages
                .iter()
                .find(|(handle, _)| *handle == run.atlas)
            {
                (*page_id, true)
            } else {
                continue;
            };
            let Some(page) = self.gpu_pages.iter().find(|page| page.id == page_id) else {
                continue;
            };
            if count_invalidations
                && (retired
                    || self.frame.page_generations.iter().any(|(id, generation)| {
                        *id == page.id && *generation != page.generation
                    }))
            {
                invalidated_runs = invalidated_runs.wrapping_add(1);
            }
            run.atlas = page.handle;
            run.atlas_revision = 1;
        }
        if let Some(profiler) = self.frame.profiler.as_mut() {
            profiler.invalidated_runs = profiler.invalidated_runs.wrapping_add(invalidated_runs);
        }
    }

    #[inline]
    pub fn last_frame_stats(&self) -> TextFrameStats {
        self.frame.profiler.as_ref().map_or(TextFrameStats::default(), |profiler| {
            profiler.last_frame_stats
        })
    }

    pub fn set_frame_stats_enabled(&mut self, enabled: bool) {
        self.atlas.set_counters_enabled(enabled);
        if enabled {
            self.frame.profiler.get_or_insert_with(TextProfiler::default);
        } else {
            self.frame.profiler = None;
        }
    }

    #[inline]
    pub fn atlas_revision(&self) -> u64 {
        self.retained_atlas_revisions
            .first()
            .map_or(self.atlas.revision(), |(_, generation)| *generation)
    }

    #[inline]
    pub fn retained_text_atlas_revision(&self) -> Option<(gfx::ImageHandle, u64)> {
        if self.atlas.has_dirty_pages() {
            return None;
        }
        self.retained_atlas_revisions
            .first()
            .copied()
            .or_else(|| self.atlas_handle.map(|handle| (handle, self.atlas.revision())))
    }

    #[inline]
    pub fn retained_text_atlas_revisions(&self) -> Option<&[(gfx::ImageHandle, u64)]> {
        if self.atlas.has_dirty_pages()
            || self.retained_atlas_revisions.len() != self.atlas.page_count()
        {
            return None;
        }
        Some(&self.retained_atlas_revisions)
    }

    pub fn ensure_gpu<U: ImageUploader>(&mut self, up: &mut U) -> gfx::ImageHandle {
        self.publish_gpu_pages(up, !self.frame_active);
        self.atlas_handle.unwrap_or(gfx::ImageHandle(0))
    }

    #[inline]
    fn flush_after_encoding<U: ImageUploader>(
        &mut self,
        up: &mut U,
        builder: &mut DrawListBuilder,
    ) {
        if self.frame_active {
            return;
        }
        if self.atlas.has_dirty_pages() {
            let _ = self.ensure_gpu(up);
        }
        self.patch_builder_atlas_pages(builder);
    }

    fn record_atlas_uploads(&mut self, calls: u64, pixels: u64) {
        if let Some(profiler) = self.frame.profiler.as_mut() {
            profiler.atlas_upload_calls = profiler.atlas_upload_calls.saturating_add(calls);
            profiler.atlas_upload_pixels = profiler.atlas_upload_pixels.saturating_add(pixels);
            profiler.atlas_upload_bytes = profiler.atlas_upload_bytes.saturating_add(pixels);
        }
    }

    fn publish_gpu_pages<U: ImageUploader>(&mut self, up: &mut U, flush_dirty: bool) {
        let mut upload_calls = 0_u64;
        let mut upload_pixels = 0_u64;
        self.frame.retired_pages.clear();
        let mut gpu_index = 0;
        while gpu_index < self.gpu_pages.len() {
            let gpu = self.gpu_pages[gpu_index];
            let current = (0..self.atlas.page_count()).any(|index| {
                self.atlas
                    .page_image(index)
                    .is_some_and(|(id, generation, ..)| id == gpu.id && generation == gpu.generation)
            });
            if current {
                gpu_index += 1;
            } else {
                self.frame.retired_pages.push((gpu.handle, gpu.id));
                up.release_a8(gpu.handle);
                self.gpu_pages.remove(gpu_index);
            }
        }

        for index in 0..self.atlas.page_count() {
            let Some((id, generation, data, width, height, _dirty)) = self.atlas.page_image(index)
            else {
                continue;
            };
            if let Some(gpu_position) = self
                .gpu_pages
                .iter()
                .position(|gpu| gpu.id == id && gpu.generation == generation)
            {
                let gpu = self.gpu_pages[gpu_position];
                if flush_dirty {
                    let dirty_count = if gpu.pristine {
                        usize::from(_dirty.is_some())
                    } else {
                        self.atlas.page_dirty_rect_count(index)
                    };
                    for dirty_index in 0..dirty_count {
                        let rect = if gpu.pristine {
                            _dirty
                        } else {
                            self.atlas.page_dirty_rect(index, dirty_index)
                        };
                        let Some(rect) = rect else { continue; };
                        let offset = rect.y as usize * width as usize + rect.x as usize;
                        up.append_a8(
                            gpu.handle,
                            rect.x,
                            rect.y,
                            rect.w,
                            rect.h,
                            &data[offset.min(data.len())..],
                            width as usize,
                        );
                        let pixels = u64::from(rect.w).saturating_mul(u64::from(rect.h));
                        upload_calls = upload_calls.saturating_add(1);
                        upload_pixels = upload_pixels.saturating_add(pixels);
                    }
                    if dirty_count != 0 {
                        self.gpu_pages[gpu_position].pristine = false;
                    }
                    self.atlas.clear_page_dirty(id);
                }
                continue;
            }
            let handle = up.create_a8(width, height, data, width as usize);
            self.gpu_pages.push(TextGpuPage {
                id,
                generation,
                handle,
                pristine: _dirty.is_none(),
            });
            upload_calls = upload_calls.saturating_add(1);
            upload_pixels = upload_pixels
                .saturating_add(u64::from(width).saturating_mul(u64::from(height)));
            self.atlas.clear_page_dirty(id);
        }
        self.record_atlas_uploads(upload_calls, upload_pixels);
        self.gpu_pages.sort_unstable_by_key(|page| page.id);
        self.atlas_handle = self.gpu_pages.first().map(|page| page.handle);
        self.retained_atlas_revisions.clear();
        self.retained_atlas_revisions.extend(
            self.gpu_pages.iter().map(|page| (page.handle, 1)),
        );
    }

    pub fn trim_memory(&mut self) {
        self.frame_active = false;
        self.atlas.end_frame();
        self.atlas.reset();
        self.atlas_handle = None;
        self.gpu_pages.clear();
        self.retained_atlas_revisions.clear();
        self.frame.retired_pages.clear();
        self.label_layouts.clear();
        self.label_layout_len = 0;
        self.text_prefixes.clear();
        self.text_prefix_len = 0;
        self.label_layout_clock = 0;
        if let Some(profiler) = self.frame.profiler.as_mut() {
            profiler.last_frame_stats = TextFrameStats::default();
        }
    }

    pub fn trim_memory_with_uploader<U: ImageUploader>(&mut self, up: &mut U) {
        for page in self.gpu_pages.iter().copied() {
            up.release_a8(page.handle);
        }
        self.trim_memory();
    }

    pub fn handle_device_loss(&mut self) {
        self.trim_memory();
    }

    pub fn set_fallback_fonts(&mut self, font_ids: &[usize]) {
        if self.fallback_fonts.as_slice() == font_ids {
            return;
        }
        self.fallback_fonts.clear();
        self.fallback_fonts.extend_from_slice(font_ids);
        self.label_layouts.clear();
        self.label_layout_len = 0;
        self.text_prefixes.clear();
        self.text_prefix_len = 0;
    }

    fn cached_label_layout<const COUNT_STATS: bool>(
        &mut self,
        text_value: &str,
        font_id: usize,
        font_px: f32,
        wrap: bool,
        max_w: f32,
    ) -> Option<Arc<CachedLabelLayout>> {
        let key = LabelLayoutStyleKey::new(font_id, font_px, wrap, max_w);
        self.label_layout_clock = self.label_layout_clock.wrapping_add(1);
        if let Some(entries) = self.label_layouts.get_mut(&key) {
            if let Some(entry) = entries.get_mut(text_value) {
                entry.last_used = self.label_layout_clock;
                if COUNT_STATS {
                    if let Some(profiler) = self.frame.profiler.as_mut() {
                        profiler.layout_cache_hits = profiler.layout_cache_hits.wrapping_add(1);
                    }
                }
                return Some(entry.layout.clone());
            }
        }
        if COUNT_STATS {
            if let Some(profiler) = self.frame.profiler.as_mut() {
                profiler.layout_cache_misses = profiler.layout_cache_misses.wrapping_add(1);
            }
        }
        let layout = Arc::new(self.build_label_layout(
            text_value,
            font_id,
            font_px,
            wrap,
            max_w,
            COUNT_STATS,
        )?);
        if self.label_layout_len >= LABEL_LAYOUT_CACHE_CAP {
            self.evict_cold_label_layouts();
        }
        let entries = self.label_layouts.entry(key).or_insert_with(HashMap::new);
        let prior = entries.insert(
            alloc::string::String::from(text_value),
            CachedLabelLayoutEntry { layout: layout.clone(), last_used: self.label_layout_clock },
        );
        if prior.is_none() {
            self.label_layout_len = self.label_layout_len.saturating_add(1);
        }
        Some(layout)
    }

    fn cached_prefix_metrics(
        &mut self,
        text_value: &str,
        font_id: usize,
        font_px: f32,
    ) -> Option<Arc<CachedTextPrefixMetrics>> {
        let key = LabelLayoutStyleKey::new(font_id, font_px, false, 0.0);
        self.label_layout_clock = self.label_layout_clock.wrapping_add(1);
        if let Some(entries) = self.text_prefixes.get_mut(&key) {
            if let Some(entry) = entries.get_mut(text_value) {
                entry.last_used = self.label_layout_clock;
                return Some(entry.metrics.clone());
            }
        }
        let metrics = Arc::new(self.build_prefix_metrics(text_value, font_id, font_px)?);
        if self.text_prefix_len >= TEXT_PREFIX_CACHE_CAP {
            self.evict_cold_prefix_metrics();
        }
        let entries = self.text_prefixes.entry(key).or_insert_with(HashMap::new);
        let prior = entries.insert(
            alloc::string::String::from(text_value),
            CachedTextPrefixEntry { metrics: metrics.clone(), last_used: self.label_layout_clock },
        );
        if prior.is_none() {
            self.text_prefix_len = self.text_prefix_len.saturating_add(1);
        }
        Some(metrics)
    }

    fn build_prefix_metrics(
        &mut self,
        text_value: &str,
        font_id: usize,
        font_px: f32,
    ) -> Option<CachedTextPrefixMetrics> {
        if !self.fallback_fonts.is_empty() {
            let map = self.shaper.cursor_map_with_fallback_fonts(
                &self.fonts,
                font_id,
                &self.fallback_fonts,
                text_value,
                font_px,
            )?;
            return Some(CachedTextPrefixMetrics { map });
        }
        let key = LabelLayoutStyleKey::new(font_id, font_px, false, 0.0);
        if let Some(entries) = self.label_layouts.get_mut(&key) {
            if let Some(entry) = entries.get_mut(text_value) {
                entry.last_used = self.label_layout_clock;
                if let Some(line) = entry.layout.lines.first() {
                    if let CachedLabelShape::Single(run) = &line.shape {
                        let map = run.shape.cursor_map_for_text(text_value);
                        return Some(CachedTextPrefixMetrics { map });
                    }
                }
            }
        }
        let font = self.fonts.font(font_id)?;
        let shape = self.shaper.shape(font, font_id, text_value, font_px).ok()?.to_owned_shape();
        let map = shape.cursor_map_for_text(text_value);
        self.cache_unwrapped_label_shape(key, font_id, text_value, shape);
        Some(CachedTextPrefixMetrics { map })
    }

    fn cache_unwrapped_label_shape(
        &mut self,
        key: LabelLayoutStyleKey,
        font_id: usize,
        text_value: &str,
        shape: text::OwnedShape,
    ) {
        if self.label_layout_len >= LABEL_LAYOUT_CACHE_CAP {
            self.evict_cold_label_layouts();
        }
        let entries = self.label_layouts.entry(key).or_insert_with(HashMap::new);
        let width = shape.width();
        let prior = entries.insert(
            alloc::string::String::from(text_value),
            CachedLabelLayoutEntry {
                layout: Arc::new(CachedLabelLayout {
                    lines: alloc::vec![CachedLabelLine {
                        width,
                        shape: CachedLabelShape::Single(CachedLabelRun {
                            font_id,
                            x_offset: 0.0,
                            shape,
                        }),
                    }],
                }),
                last_used: self.label_layout_clock,
            },
        );
        if prior.is_none() {
            self.label_layout_len = self.label_layout_len.saturating_add(1);
        }
    }

    fn evict_cold_label_layouts(&mut self) {
        if self.label_layout_len < LABEL_LAYOUT_CACHE_CAP {
            return;
        }
        let prune_before =
            self.label_layout_clock.saturating_sub(LABEL_LAYOUT_CACHE_PRUNE_TARGET as u64);
        let mut removed = 0usize;
        self.label_layouts.retain(|_, entries| {
            let before = entries.len();
            entries.retain(|_, entry| entry.last_used >= prune_before);
            removed = removed.saturating_add(before.saturating_sub(entries.len()));
            !entries.is_empty()
        });
        self.label_layout_len = self.label_layout_len.saturating_sub(removed);
        if self.label_layout_len < LABEL_LAYOUT_CACHE_CAP {
            return;
        }
        let Some((old_style_key, old_text_key)) = self
            .label_layouts
            .iter()
            .flat_map(|(style_key, entries)| {
                entries.iter().map(move |(text_key, entry)| (*style_key, text_key, entry.last_used))
            })
            .min_by_key(|(_, _, last_used)| *last_used)
            .map(|(style_key, text_key, _)| (style_key, text_key.clone()))
        else {
            return;
        };
        let remove_style = if let Some(entries) = self.label_layouts.get_mut(&old_style_key) {
            if entries.remove(&old_text_key).is_some() {
                self.label_layout_len = self.label_layout_len.saturating_sub(1);
            }
            entries.is_empty()
        } else {
            false
        };
        if remove_style {
            self.label_layouts.remove(&old_style_key);
        }
    }

    fn evict_cold_prefix_metrics(&mut self) {
        if self.text_prefix_len < TEXT_PREFIX_CACHE_CAP {
            return;
        }
        let prune_before =
            self.label_layout_clock.saturating_sub(TEXT_PREFIX_CACHE_PRUNE_TARGET as u64);
        let mut removed = 0usize;
        self.text_prefixes.retain(|_, entries| {
            let before = entries.len();
            entries.retain(|_, entry| entry.last_used >= prune_before);
            removed = removed.saturating_add(before.saturating_sub(entries.len()));
            !entries.is_empty()
        });
        self.text_prefix_len = self.text_prefix_len.saturating_sub(removed);
        if self.text_prefix_len < TEXT_PREFIX_CACHE_CAP {
            return;
        }
        let Some((old_style_key, old_text_key)) = self
            .text_prefixes
            .iter()
            .flat_map(|(style_key, entries)| {
                entries.iter().map(move |(text_key, entry)| (*style_key, text_key, entry.last_used))
            })
            .min_by_key(|(_, _, last_used)| *last_used)
            .map(|(style_key, text_key, _)| (style_key, text_key.clone()))
        else {
            return;
        };
        let remove_style = if let Some(entries) = self.text_prefixes.get_mut(&old_style_key) {
            if entries.remove(&old_text_key).is_some() {
                self.text_prefix_len = self.text_prefix_len.saturating_sub(1);
            }
            entries.is_empty()
        } else {
            false
        };
        if remove_style {
            self.text_prefixes.remove(&old_style_key);
        }
    }

    fn build_label_layout(
        &mut self,
        text_value: &str,
        font_id: usize,
        font_px: f32,
        wrap: bool,
        max_w: f32,
        count_shapes: bool,
    ) -> Option<CachedLabelLayout> {
        if !wrap {
            return Some(CachedLabelLayout {
                lines: alloc::vec![self.build_label_line(
                    text_value,
                    font_id,
                    font_px,
                    count_shapes,
                )?],
            });
        }
        if self.fallback_fonts.is_empty() && text_value.is_ascii() {
            if let Some(layout) = self.build_primary_ascii_wrapped_label_layout(
                text_value,
                font_id,
                font_px,
                max_w,
                count_shapes,
            )
            {
                return Some(layout);
            }
        }

        let mut lines: alloc::vec::Vec<CachedLabelLine> = alloc::vec::Vec::with_capacity(4);
        let mut cur = alloc::string::String::with_capacity(text_value.len().min(128));
        let mut cur_line: Option<CachedLabelLine> = None;
        let mut pending_spaces = 0usize;
        for w in text_value.split_inclusive(' ') {
            let trailing_spaces = w.as_bytes().iter().rev().take_while(|b| **b == b' ').count();
            let word = &w[..w.len() - trailing_spaces];
            if word.is_empty() {
                pending_spaces = pending_spaces.saturating_add(trailing_spaces);
                continue;
            }
            let prior_len = cur.len();
            for _ in 0..pending_spaces {
                cur.push(' ');
            }
            cur.push_str(word);
            pending_spaces = trailing_spaces;
            let Some(line) = self.build_label_line(&cur, font_id, font_px, count_shapes) else {
                cur.truncate(prior_len);
                continue;
            };
            if line.width > max_w && prior_len > 0 {
                cur.truncate(prior_len);
                if let Some(line) = cur_line.take() {
                    lines.push(line);
                }
                cur.clear();
                cur.push_str(word);
                let Some(line) = self.build_label_line(&cur, font_id, font_px, count_shapes) else {
                    cur.clear();
                    pending_spaces = 0;
                    continue;
                };
                cur_line = Some(line);
            } else {
                cur_line = Some(line);
            }
        }
        if !cur.is_empty() {
            if let Some(line) = cur_line.take() {
                lines.push(line);
            }
        }
        Some(CachedLabelLayout { lines })
    }

    fn build_primary_ascii_wrapped_label_layout(
        &mut self,
        text_value: &str,
        font_id: usize,
        font_px: f32,
        max_w: f32,
        count_shapes: bool,
    ) -> Option<CachedLabelLayout> {
        if text_value.is_empty() {
            return Some(CachedLabelLayout { lines: Vec::new() });
        }
        let font = self.fonts.font(font_id)?;
        let whole_shape =
            self.shaper.shape(font, font_id, text_value, font_px).ok()?.to_owned_shape();
        self.record_shape_calls(count_shapes, 1);
        let words = ascii_wrap_word_ranges(text_value);
        if words.is_empty() {
            return Some(CachedLabelLayout { lines: Vec::new() });
        }

        let mut boundaries = Vec::with_capacity(words.len().saturating_mul(2).saturating_add(1));
        boundaries.push(0);
        for (start, end) in words.iter().copied() {
            if boundaries.last().copied() != Some(start) {
                boundaries.push(start);
            }
            if boundaries.last().copied() != Some(end) {
                boundaries.push(end);
            }
        }
        let widths = whole_shape.prefix_widths_for_boundaries(&boundaries);
        let width_at = |byte: usize| -> f32 {
            match boundaries.binary_search(&byte) {
                Ok(index) => widths.get(index).copied().unwrap_or(0.0),
                Err(_) => 0.0,
            }
        };

        let mut lines: Vec<CachedLabelLine> = Vec::with_capacity(4);
        let mut line_start: Option<usize> = None;
        let mut line_end = 0usize;
        for (word_start, word_end) in words.iter().copied() {
            let start = line_start.unwrap_or(0);
            let candidate_width = (width_at(word_end) - width_at(start)).max(0.0);
            let mut should_wrap = line_end > start && candidate_width > max_w;
            if !should_wrap
                && line_end > start
                && max_w.is_finite()
                && candidate_width + WRAP_WIDTH_CONFIRM_EPS >= max_w
            {
                if let Some(line) =
                    self.build_label_line(
                        &text_value[start..word_end],
                        font_id,
                        font_px,
                        count_shapes,
                    )
                {
                    should_wrap = line.width > max_w;
                }
            }
            if should_wrap {
                if let Some(line) =
                    self.build_label_line(
                        &text_value[start..line_end],
                        font_id,
                        font_px,
                        count_shapes,
                    )
                {
                    lines.push(line);
                }
                line_start = Some(word_start);
                line_end = word_end;
            } else {
                line_start = Some(start);
                line_end = word_end;
            }
        }

        if let Some(start) = line_start {
            if start < line_end {
                if lines.is_empty() && start == 0 && line_end == text_value.len() {
                    lines.push(cached_label_line_from_owned_shape(font_id, whole_shape));
                } else if let Some(line) =
                    self.build_label_line(
                        &text_value[start..line_end],
                        font_id,
                        font_px,
                        count_shapes,
                    )
                {
                    lines.push(line);
                }
            }
        }
        Some(CachedLabelLayout { lines })
    }

    fn build_label_line(
        &mut self,
        text_value: &str,
        font_id: usize,
        font_px: f32,
        count_shapes: bool,
    ) -> Option<CachedLabelLine> {
        if !self.fallback_fonts.is_empty() {
            let fallback = self.shaper.shape_with_fallback_fonts(
                &self.fonts,
                font_id,
                &self.fallback_fonts,
                text_value,
                font_px,
            )?;
            self.record_shape_calls(count_shapes, fallback.shape_run_count() as u64);
            let width = fallback.width();
            return Some(CachedLabelLine { width, shape: CachedLabelShape::Fallback(fallback) });
        }
        let font = self.fonts.font(font_id)?;
        let shape = self.shaper.shape(font, font_id, text_value, font_px).ok()?.to_owned_shape();
        self.record_shape_calls(count_shapes, 1);
        Some(cached_label_line_from_owned_shape(font_id, shape))
    }

    #[inline]
    fn record_shape_calls(&mut self, enabled: bool, calls: u64) {
        if enabled {
            if let Some(profiler) = self.frame.profiler.as_mut() {
                profiler.shaping_calls = profiler.shaping_calls.wrapping_add(calls);
            }
        }
    }
}

// ----- Label -----

#[derive(Clone, Copy, Debug)]
pub enum Align {
    Left,
    Center,
    Right,
}

pub struct Label {
    pub text: alloc::string::String,
    pub color: gfx::Color,
    pub align: Align,
    pub wrap: bool,
    pub font_id: usize,
    pub font_px: f32,
}

impl Default for Label {
    fn default() -> Self {
        Self {
            text: alloc::string::String::new(),
            color: gfx::Color::rgba(0.1, 0.1, 0.1, 1.0),
            align: Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 14.0,
        }
    }
}

fn bake_cached_label_line<const COUNT_STATS: bool>(
    line: &CachedLabelLine,
    color: gfx::Color,
    origin_x: f32,
    origin_y: f32,
    device_scale: f32,
    txt: &mut TextCtx,
    b: &mut DrawListBuilder,
) -> (u32, u32) {
    let vertex_start = b.drawlist().vertices.len() as u32;
    let index_start = b.drawlist().indices.len() as u32;
    let mut glyph_runs = core::mem::take(&mut txt.frame.glyph_runs);
    glyph_runs.clear();
    match &line.shape {
        CachedLabelShape::Single(run) => {
            let Some(font) = txt.fonts.font(run.font_id) else {
                txt.frame.glyph_runs = glyph_runs;
                return (0, 0);
            };
            let dl = b.drawlist_mut();
            if COUNT_STATS {
                run.shape.bake_paged_counted_into_with(
                    font,
                    &mut txt.raster,
                    &mut txt.atlas,
                    &mut dl.vertices,
                    &mut dl.indices,
                    &mut glyph_runs,
                    color,
                    origin_x + run.x_offset,
                    origin_y,
                    device_scale,
                );
            } else {
                run.shape.bake_paged_into_with(
                    font,
                    &mut txt.raster,
                    &mut txt.atlas,
                    &mut dl.vertices,
                    &mut dl.indices,
                    &mut glyph_runs,
                    color,
                    origin_x + run.x_offset,
                    origin_y,
                    device_scale,
                );
            }
        }
        CachedLabelShape::Fallback(shape) => {
            let dl = b.drawlist_mut();
            if COUNT_STATS {
                shape.bake_paged_counted_into_with(
                    &txt.fonts,
                    &mut txt.raster,
                    &mut txt.atlas,
                    &mut dl.vertices,
                    &mut dl.indices,
                    &mut glyph_runs,
                    color,
                    origin_x,
                    origin_y,
                    device_scale,
                );
            } else {
                shape.bake_paged_into_with(
                    &txt.fonts,
                    &mut txt.raster,
                    &mut txt.atlas,
                    &mut dl.vertices,
                    &mut dl.indices,
                    &mut glyph_runs,
                    color,
                    origin_x,
                    origin_y,
                    device_scale,
                );
            }
        }
    }
    for run in glyph_runs.iter().copied() {
        b.glyph_run_provisional(run);
    }
    glyph_runs.clear();
    txt.frame.glyph_runs = glyph_runs;
    let vertex_count = (b.drawlist().vertices.len() as u32).saturating_sub(vertex_start);
    let index_count = (b.drawlist().indices.len() as u32).saturating_sub(index_start);
    (vertex_count, index_count)
}

#[inline]
fn encode_label_cached<const COUNT_STATS: bool, U: ImageUploader>(
    text_value: &str,
    color: gfx::Color,
    align: Align,
    wrap: bool,
    font_id: usize,
    font_px: f32,
    rect: gfx::RectF,
    device_scale: f32,
    txt: &mut TextCtx,
    up: &mut U,
    b: &mut DrawListBuilder,
) {
    if txt.fonts.font(font_id).is_none() {
        watch_text_event_lazy("label.skip_missing_font", font_id, text_value, || {
            format!("rect={:.1}x{:.1} font_px={:.1}", rect.w, rect.h, font_px)
        });
        return;
    }
    let max_w = if wrap { rect.w.max(0.0) } else { f32::INFINITY };
    let Some(layout) = txt.cached_label_layout::<COUNT_STATS>(text_value, font_id, font_px, wrap, max_w) else {
        watch_text_event("label.shape_error", font_id, text_value, "cache_build_failed");
        return;
    };
    watch_text_event_lazy("label.begin", font_id, text_value, || {
        format!(
            "rect={:.1}x{:.1} font_px={:.1} atlas_handle={}",
            rect.w,
            rect.h,
            font_px,
            txt.atlas_handle.is_some()
        )
    });

    let scale = if device_scale > 0.0 { device_scale } else { 1.0 };
    let ox = (rect.x * scale).round() / scale;
    let mut oy = (rect.y * scale).round() / scale;
    let line_h = (font_px * 1.25).ceil();
    watch_text_event_lazy("label.lines", font_id, text_value, || {
        format!("count={} wrap={}", layout.lines.len(), wrap)
    });

    for line in layout.lines.iter() {
        let dx = match align {
            Align::Left => 0.0,
            Align::Center => (rect.w - line.width) * 0.5,
            Align::Right => rect.w - line.width,
        };
        let (verts, indices) =
            bake_cached_label_line::<COUNT_STATS>(line, color, ox + dx, oy, scale, txt, b);
        watch_text_event_lazy("label.glyph_run", font_id, text_value, || {
            format!(
                "width={:.1} verts={} indices={} atlas_handle={}",
                line.width,
                verts,
                indices,
                txt.atlas_handle.is_some()
            )
        });
        oy += line_h;
    }

    txt.flush_after_encoding(up, b);
}

#[inline]
fn encode_label_unwrapped<U: ImageUploader>(
    text_value: &str,
    color: gfx::Color,
    align: Align,
    font_id: usize,
    font_px: f32,
    rect: gfx::RectF,
    device_scale: f32,
    txt: &mut TextCtx,
    up: &mut U,
    b: &mut DrawListBuilder,
) {
    encode_label_cached::<false, U>(
        text_value,
        color,
        align,
        false,
        font_id,
        font_px,
        rect,
        device_scale,
        txt,
        up,
        b,
    );
}

#[inline]
pub fn encode_label_text<U: ImageUploader>(
    text_value: &str,
    color: gfx::Color,
    align: Align,
    wrap: bool,
    font_id: usize,
    font_px: f32,
    rect: gfx::RectF,
    device_scale: f32,
    txt: &mut TextCtx,
    up: &mut U,
    b: &mut DrawListBuilder,
) {
    encode_label_cached::<false, U>(
        text_value,
        color,
        align,
        wrap,
        font_id,
        font_px,
        rect,
        device_scale,
        txt,
        up,
        b,
    );
}

#[doc(hidden)]
#[cold]
#[inline(never)]
pub fn encode_label_text_profiled<U: ImageUploader>(
    text_value: &str,
    color: gfx::Color,
    align: Align,
    wrap: bool,
    font_id: usize,
    font_px: f32,
    rect: gfx::RectF,
    device_scale: f32,
    txt: &mut TextCtx,
    up: &mut U,
    b: &mut DrawListBuilder,
) {
    encode_label_cached::<true, U>(
        text_value,
        color,
        align,
        wrap,
        font_id,
        font_px,
        rect,
        device_scale,
        txt,
        up,
        b,
    );
}

impl Label {
    pub fn encode<U: ImageUploader>(
        &self,
        rect: gfx::RectF,
        device_scale: f32,
        txt: &mut TextCtx,
        up: &mut U,
        b: &mut DrawListBuilder,
    ) {
        encode_label_cached::<false, U>(
            &self.text,
            self.color,
            self.align,
            self.wrap,
            self.font_id,
            self.font_px,
            rect,
            device_scale,
            txt,
            up,
            b,
        );
    }
}

// ----- ProgressBar -----

pub struct ProgressBar {
    pub value: Option<f32>, // None -> indeterminate
    pub track: gfx::Color,
    pub fill: gfx::Color,
    pub corner: f32,
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self {
            value: Some(0.0),
            track: gfx::Color::rgba(0.85, 0.85, 0.85, 1.0),
            fill: gfx::Color::rgba(0.2, 0.5, 1.0, 1.0),
            corner: 4.0,
        }
    }
}

impl ProgressBar {
    pub fn encode(&self, rect: gfx::RectF, phase: f32, b: &mut DrawListBuilder) {
        // Track
        b.rrect(rect, [self.corner; 4], self.track);
        // Draw determinate or indeterminate fill
        match self.value {
            Some(mut v) => {
                v = v.clamp(0.0, 1.0);
                let w = rect.w * v;
                if w > 0.5 {
                    b.rrect(
                        gfx::RectF::new(rect.x, rect.y, w, rect.h),
                        [self.corner; 4],
                        self.fill,
                    );
                }
            }
            None => {
                // Indeterminate: moving segment ~30% width looping with phase 0..1
                let seg = (rect.w * 0.3).max(8.0);
                let t = (phase.fract() + 1.0).fract();
                let x = rect.x + (rect.w - seg) * t;
                let mut c = self.fill;
                c.a *= 0.9;
                b.rrect(gfx::RectF::new(x, rect.y, seg, rect.h), [self.corner; 4], c);
            }
        }
    }
}

// ----- Camera background -----

pub struct UICameraView {
    pub tint: gfx::Color,
    pub alpha: f32,
    pub grayscale: bool,
    pub blur: bool,
    pub sigma: f32,
}

impl Default for UICameraView {
    fn default() -> Self {
        Self {
            tint: gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
            alpha: 1.0,
            grayscale: false,
            blur: true,
            sigma: 6.0,
        }
    }
}

impl UICameraView {
    pub fn encode(&self, rect: gfx::RectF, b: &mut DrawListBuilder) {
        b.camera_bg(rect, self.tint, self.alpha, self.grayscale, self.blur, self.sigma);
    }
}

// ----- Spinner -----

/// Legacy iOS spinner contract backed by `UIActivityIndicatorViewStyleLarge`.
///
/// The shared API owns sizing and animation defaults so callers no longer
/// pass renderer-specific stroke or phase data.
pub struct Spinner {
    pub alpha: f32,
}

impl Default for Spinner {
    fn default() -> Self {
        Self { alpha: 1.0 }
    }
}

impl Spinner {
    pub fn encode(&self, rect: gfx::RectF, b: &mut DrawListBuilder) {
        self.encode_at(
            [rect.x + rect.w * 0.5, rect.y + rect.h * 0.5],
            rect.w.min(rect.h).max(1.0),
            b,
        );
    }

    pub fn encode_at(&self, center: [f32; 2], atom: f32, b: &mut DrawListBuilder) {
        debug_assert!(center[0].is_finite() && center[1].is_finite());
        debug_assert!(atom.is_finite() && atom > 0.0);
        let alpha = self.alpha.clamp(0.0, 1.0);
        b.spinner(center, atom.max(1.0), alpha);
    }

    pub fn draw(&self, rect: gfx::RectF, encoder: &mut dyn gfx::RenderEncoder) {
        self.draw_at(
            [rect.x + rect.w * 0.5, rect.y + rect.h * 0.5],
            rect.w.min(rect.h).max(1.0),
            encoder,
        );
    }

    pub fn draw_at(&self, center: [f32; 2], atom: f32, encoder: &mut dyn gfx::RenderEncoder) {
        debug_assert!(center[0].is_finite() && center[1].is_finite());
        debug_assert!(atom.is_finite() && atom > 0.0);
        encoder.draw_spinner(center, atom.max(1.0), self.alpha.clamp(0.0, 1.0));
    }
}

// ----- helpers -----

// =============================
// Button, Toggle, Slider (UI II)
// =============================

// ----- Button -----

#[derive(Clone, Copy, Debug)]
pub struct ButtonStyle {
    pub corner: f32,
    pub pad_x: f32,
    pub pad_y: f32,
    pub color: gfx::Color,
    pub color_pressed: gfx::Color,
    pub color_disabled: gfx::Color,
    pub text_px: f32,
    pub text_color: gfx::Color,
    pub press_animation_ms: u32,
}

impl Default for ButtonStyle {
    fn default() -> Self {
        Self {
            corner: 6.0,
            pad_x: 10.0,
            pad_y: 6.0,
            color: gfx::Color::rgba(0.20, 0.55, 1.0, 1.0),
            color_pressed: gfx::Color::rgba(0.18, 0.50, 0.95, 1.0),
            color_disabled: gfx::Color::rgba(0.70, 0.75, 0.80, 1.0),
            text_px: 14.0,
            text_color: gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
            press_animation_ms: 100,
        }
    }
}

pub struct Button {
    pub text: alloc::string::String,
    pub style: ButtonStyle,
}

impl Default for Button {
    fn default() -> Self {
        Self { text: alloc::string::String::from("Button"), style: ButtonStyle::default() }
    }
}

pub struct ButtonState {
    pub disabled: bool,
    pressed: bool,
    anim_from: f32,
    anim_to: f32,
    anim_start_ms: u64,
    anim_dur_ms: u32,
}

impl Default for ButtonState {
    fn default() -> Self {
        Self {
            disabled: false,
            pressed: false,
            anim_from: 1.0,
            anim_to: 1.0,
            anim_start_ms: timing::now_ms(),
            anim_dur_ms: 0,
        }
    }
}

impl ButtonState {
    #[inline]
    pub fn is_pressed(&self) -> bool {
        self.pressed
    }

    fn current_scale(&self, now: u64) -> f32 {
        let t_ms = now.saturating_sub(self.anim_start_ms) as u32;
        if self.anim_dur_ms == 0 {
            return self.anim_to;
        }
        let k = (t_ms as f32 / self.anim_dur_ms as f32).clamp(0.0, 1.0);
        self.anim_from + (self.anim_to - self.anim_from) * k
    }

    pub fn on_pointer_down(&mut self) {
        if self.disabled {
            return;
        }
        self.pressed = true;
        self.anim_from = self.current_scale(timing::now_ms());
        self.anim_to = 0.98;
        self.anim_start_ms = timing::now_ms();
        self.anim_dur_ms = 80;
    }

    pub fn on_pointer_cancel(&mut self) {
        if self.disabled {
            return;
        }
        self.pressed = false;
        self.anim_from = self.current_scale(timing::now_ms());
        self.anim_to = 1.0;
        self.anim_start_ms = timing::now_ms();
        self.anim_dur_ms = 120;
    }

    /// Returns true if this was a tap (released while pressed)
    pub fn on_pointer_up(&mut self) -> bool {
        if self.disabled {
            return false;
        }
        let was_pressed = self.pressed;
        self.pressed = false;
        self.anim_from = self.current_scale(timing::now_ms());
        self.anim_to = 1.0;
        self.anim_start_ms = timing::now_ms();
        self.anim_dur_ms = 120;
        was_pressed
    }
}

impl Button {
    pub fn encode<U: ImageUploader>(
        &self,
        rect: gfx::RectF,
        device_scale: f32,
        txt: &mut TextCtx,
        up: &mut U,
        state: &ButtonState,
        b: &mut DrawListBuilder,
    ) {
        // Determine scale from animation
        let s = state.current_scale(timing::now_ms()).clamp(0.9, 1.0);
        let color = if state.disabled {
            self.style.color_disabled
        } else if state.pressed {
            self.style.color_pressed
        } else {
            self.style.color
        };
        // Scale about center
        let cx = rect.x + rect.w * 0.5;
        let cy = rect.y + rect.h * 0.5;
        let w = rect.w * s;
        let h = rect.h * s;
        let r = gfx::RectF::new(cx - w * 0.5, cy - h * 0.5, w, h);
        b.rrect(r, [self.style.corner; 4], color);

        // Label centered
        if !self.text.is_empty() {
            encode_label_unwrapped(
                &self.text,
                self.style.text_color,
                Align::Center,
                0,
                self.style.text_px,
                r,
                device_scale,
                txt,
                up,
                b,
            );
        }
    }
}

// ----- Toggle -----

#[derive(Clone, Copy, Debug)]
pub struct ToggleStyle {
    pub corner: f32,
    pub track_on: gfx::Color,
    pub track_off: gfx::Color,
    pub thumb: gfx::Color,
    pub pad: f32,
    pub animation_ms: u32,
}

impl Default for ToggleStyle {
    fn default() -> Self {
        Self {
            corner: 12.0,
            track_on: gfx::Color::rgba(0.20, 0.65, 0.25, 1.0),
            track_off: gfx::Color::rgba(0.75, 0.78, 0.82, 1.0),
            thumb: gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
            pad: 2.0,
            animation_ms: 200,
        }
    }
}

pub struct Toggle {
    pub style: ToggleStyle,
}

impl Default for Toggle {
    fn default() -> Self {
        Self { style: ToggleStyle::default() }
    }
}

pub struct ToggleState {
    pub on: bool,
    dragging: bool,
    drag_start_x: f32,
    anim_t: f32,      // 0..1 visual position
    anim_target: f32, // 0..1
    anim_v: f32,      // velocity for simple spring
}

impl Default for ToggleState {
    fn default() -> Self {
        Self {
            on: false,
            dragging: false,
            drag_start_x: 0.0,
            anim_t: 0.0,
            anim_target: 0.0,
            anim_v: 0.0,
        }
    }
}

impl ToggleState {
    pub fn set_on(&mut self, on: bool) {
        self.on = on;
        self.anim_target = if on { 1.0 } else { 0.0 };
    }
    pub fn on_tap(&mut self) -> bool {
        self.on = !self.on;
        self.anim_target = if self.on { 1.0 } else { 0.0 };
        true
    }
    pub fn begin_drag(&mut self, x: f32) {
        self.dragging = true;
        self.drag_start_x = x;
    }
    pub fn drag_to(&mut self, x: f32, rect: gfx::RectF) {
        if self.dragging {
            let t = ((x - rect.x) / rect.w).clamp(0.0, 1.0);
            self.anim_target = t;
            self.on = t >= 0.5;
        }
    }
    pub fn end_drag(&mut self) {
        self.dragging = false;
        self.anim_target = if self.on { 1.0 } else { 0.0 };
    }

    pub fn step(&mut self, dt_ms: u32) {
        // Critically damped spring toward anim_target
        let target = self.anim_target;
        let k: f32 = 20.0;
        let c = 2.0 * (k).sqrt();
        let dt = (dt_ms as f32 / 1000.0).min(0.05);
        let a = k * (target - self.anim_t) - c * self.anim_v;
        self.anim_v += a * dt;
        self.anim_t += self.anim_v * dt;
        // Clamp
        if (self.anim_t - target).abs() < 0.001 && self.anim_v.abs() < 0.001 {
            self.anim_t = target;
            self.anim_v = 0.0;
        }
    }
}

impl Toggle {
    pub fn encode(&self, rect: gfx::RectF, state: &ToggleState, b: &mut DrawListBuilder) {
        // Track color interpolated
        let t = state.anim_t.clamp(0.0, 1.0);
        let lerp = |a: gfx::Color, b_: gfx::Color, k: f32| {
            gfx::Color::rgba(
                a.r + (b_.r - a.r) * k,
                a.g + (b_.g - a.g) * k,
                a.b + (b_.b - a.b) * k,
                a.a + (b_.a - a.a) * k,
            )
        };
        let track_c = lerp(self.style.track_off, self.style.track_on, t);
        b.rrect(rect, [self.style.corner; 4], track_c);
        // Thumb position
        let r = rect;
        let thumb_r = (r.h * 0.5 - self.style.pad).max(2.0);
        let x0 = r.x + self.style.pad + thumb_r;
        let x1 = r.x + r.w - self.style.pad - thumb_r;
        let cx = x0 + (x1 - x0) * t;
        let cy = r.y + r.h * 0.5;
        // Draw thumb as a rounded rect approximated by rrect with equal radii
        let d = thumb_r;
        let thumb = gfx::RectF::new(cx - d, cy - d, d * 2.0, d * 2.0);
        b.rrect(thumb, [d; 4], self.style.thumb);
    }
}

// ----- Slider -----

#[derive(Clone, Copy, Debug)]
pub struct SliderStyle {
    pub corner: f32,
    pub track: gfx::Color,
    pub fill: gfx::Color,
    pub thumb: gfx::Color,
    pub pad: f32,
}

impl Default for SliderStyle {
    fn default() -> Self {
        Self {
            corner: 3.0,
            track: gfx::Color::rgba(0.80, 0.82, 0.86, 1.0),
            fill: gfx::Color::rgba(0.25, 0.55, 1.0, 1.0),
            thumb: gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
            pad: 4.0,
        }
    }
}

pub struct Slider {
    pub style: SliderStyle,
    pub step: Option<f32>,
}

impl Default for Slider {
    fn default() -> Self {
        Self { style: SliderStyle::default(), step: None }
    }
}

pub struct SliderState {
    pub value: f32, // 0..1
    dragging: bool,
    drag_start_x: f32,
    value_at_start: f32,
}

impl Default for SliderState {
    fn default() -> Self {
        Self { value: 0.0, dragging: false, drag_start_x: 0.0, value_at_start: 0.0 }
    }
}

impl SliderState {
    pub fn set(&mut self, v: f32, step: Option<f32>) -> bool {
        let nv = apply_step(v.clamp(0.0, 1.0), step);
        let changed = (nv - self.value).abs() > 1e-6;
        self.value = nv;
        changed
    }

    pub fn begin_drag(&mut self, x: f32) {
        self.dragging = true;
        self.drag_start_x = x;
        self.value_at_start = self.value;
    }
    pub fn drag_to(&mut self, x: f32, rect: gfx::RectF, step: Option<f32>) -> bool {
        if !self.dragging {
            return false;
        }
        let t = ((x - rect.x) / rect.w).clamp(0.0, 1.0);
        self.set(t, step)
    }
    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    pub fn arrow_left(&mut self, step: Option<f32>) -> bool {
        let s = step.unwrap_or(0.01);
        self.set(self.value - s, step)
    }
    pub fn arrow_right(&mut self, step: Option<f32>) -> bool {
        let s = step.unwrap_or(0.01);
        self.set(self.value + s, step)
    }
}

impl Slider {
    pub fn encode(&self, rect: gfx::RectF, state: &SliderState, b: &mut DrawListBuilder) {
        // Track
        b.rrect(rect, [self.style.corner; 4], self.style.track);
        // Fill
        let w = (rect.w * state.value.clamp(0.0, 1.0)).max(0.0);
        if w > 0.5 {
            b.rrect(
                gfx::RectF::new(rect.x, rect.y, w, rect.h),
                [self.style.corner; 4],
                self.style.fill,
            );
        }
        // Thumb
        let r = rect;
        let thumb_r = (r.h * 0.5 - self.style.pad).max(2.0);
        let cx = r.x + r.w * state.value;
        let cy = r.y + r.h * 0.5;
        let d = thumb_r;
        let thumb = gfx::RectF::new(cx - d, cy - d, d * 2.0, d * 2.0);
        b.rrect(thumb, [d; 4], self.style.thumb);
    }
}

fn apply_step(v: f32, step: Option<f32>) -> f32 {
    if let Some(s) = step {
        (v / s).round() * s
    } else {
        v
    }
}

// =============================
// ImageView (+ Zoom) and Nine-Slice
// =============================

#[derive(Clone, Copy, Debug)]
pub enum ImageFit {
    Contain,
    Cover,
    Stretch,
}

pub struct ImageView {
    pub image: gfx::ImageHandle,
    pub natural_w: u32,
    pub natural_h: u32,
    pub fit: ImageFit,
    pub alpha: f32,
}

impl Default for ImageView {
    fn default() -> Self {
        Self {
            image: gfx::ImageHandle(0),
            natural_w: 1,
            natural_h: 1,
            fit: ImageFit::Contain,
            alpha: 1.0,
        }
    }
}

pub struct ImageZoomState {
    pub scale: f32,       // additional zoom multiplier
    pub offset: [f32; 2], // pan in pixels in destination space
}

impl Default for ImageZoomState {
    fn default() -> Self {
        Self { scale: 1.0, offset: [0.0, 0.0] }
    }
}

impl ImageZoomState {
    pub fn reset(&mut self) {
        self.scale = 1.0;
        self.offset = [0.0, 0.0];
    }
    pub fn pinch(&mut self, delta: f32, center: [f32; 2]) {
        let s = (self.scale * (1.0 + delta)).clamp(0.5, 8.0);
        self.scale = s;
        let _ = center;
    }
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.offset[0] += dx;
        self.offset[1] += dy;
    }
    pub fn double_tap_zoom_out(&mut self) {
        self.reset();
    }
}

impl ImageView {
    pub fn encode(&self, rect: gfx::RectF, zoom: Option<&ImageZoomState>, b: &mut DrawListBuilder) {
        let alpha = self.alpha.clamp(0.0, 1.0);
        if self.image.0 == 0
            || !rect_finite_positive(rect)
            || !alpha.is_finite()
            || alpha <= 0.0
        {
            return;
        }
        let iw = self.natural_w.max(1) as f32;
        let ih = self.natural_h.max(1) as f32;
        if zoom.is_none() {
            let view_cross = rect.w * ih;
            let image_cross = rect.h * iw;
            if !view_cross.is_finite() || !image_cross.is_finite() {
                return;
            }
            match self.fit {
                ImageFit::Contain => {
                    let (dw, dh) = if view_cross >= image_cross {
                        (rect.h * iw / ih, rect.h)
                    } else {
                        (rect.w, rect.w * ih / iw)
                    };
                    let dst = gfx::RectF::new(
                        rect.x + (rect.w - dw) * 0.5,
                        rect.y + (rect.h - dh) * 0.5,
                        dw,
                        dh,
                    );
                    b.image_prevalidated(
                        self.image,
                        dst,
                        gfx::RectF::new(0.0, 0.0, iw, ih),
                        alpha,
                    );
                }
                ImageFit::Cover => {
                    let src = if view_cross >= image_cross {
                        let src_h = rect.h * iw / rect.w;
                        gfx::RectF::new(0.0, (ih - src_h) * 0.5, iw, src_h)
                    } else {
                        let src_w = rect.w * ih / rect.h;
                        gfx::RectF::new((iw - src_w) * 0.5, 0.0, src_w, ih)
                    };
                    b.image_prevalidated(self.image, rect, src, alpha);
                }
                ImageFit::Stretch => {
                    b.image_prevalidated(
                        self.image,
                        rect,
                        gfx::RectF::new(0.0, 0.0, iw, ih),
                        alpha,
                    );
                }
            }
            return;
        }
        let sx = rect.w / iw;
        let sy = rect.h / ih;
        let base = match self.fit {
            ImageFit::Contain => sx.min(sy),
            ImageFit::Cover => sx.max(sy),
            ImageFit::Stretch => 1.0,
        };
        let Some(zoom) = zoom else {
            return;
        };
        if zoom.offset == [0.0, 0.0]
            && (zoom.scale == 1.0 || matches!(self.fit, ImageFit::Stretch))
        {
            self.encode(rect, None, b);
            return;
        }
        let scale = base * zoom.scale;
        let dw = if matches!(self.fit, ImageFit::Stretch) { rect.w } else { iw * scale };
        let dh = if matches!(self.fit, ImageFit::Stretch) { rect.h } else { ih * scale };
        let mut dx = rect.x + (rect.w - dw) * 0.5;
        let mut dy = rect.y + (rect.h - dh) * 0.5;
        dx += zoom.offset[0];
        dy += zoom.offset[1];
        let Some((dst, src)) = cropped_image_mapping(rect, gfx::RectF::new(dx, dy, dw, dh), iw, ih)
        else {
            return;
        };
        b.image_prevalidated(self.image, dst, src, alpha);
    }
}

fn cropped_image_mapping(bounds: gfx::RectF, fitted: gfx::RectF, image_w: f32, image_h: f32) -> Option<(gfx::RectF, gfx::RectF)> {
    if !rect_finite_positive(bounds)
        || !rect_finite_positive(fitted)
        || !image_w.is_finite()
        || !image_h.is_finite()
        || image_w <= 0.0
        || image_h <= 0.0
    {
        return None;
    }
    let fitted_right = fitted.x + fitted.w;
    let fitted_bottom = fitted.y + fitted.h;
    let bounds_right = bounds.x + bounds.w;
    let bounds_bottom = bounds.y + bounds.h;
    if !fitted_right.is_finite()
        || !fitted_bottom.is_finite()
        || !bounds_right.is_finite()
        || !bounds_bottom.is_finite()
    {
        return None;
    }
    let left = fitted.x.max(bounds.x);
    let top = fitted.y.max(bounds.y);
    let right = fitted_right.min(bounds_right);
    let bottom = fitted_bottom.min(bounds_bottom);
    if right <= left || bottom <= top
    {
        return None;
    }
    let src_left = ((left - fitted.x) * image_w / fitted.w).clamp(0.0, image_w);
    let src_top = ((top - fitted.y) * image_h / fitted.h).clamp(0.0, image_h);
    let src_right = ((right - fitted.x) * image_w / fitted.w).clamp(0.0, image_w);
    let src_bottom = ((bottom - fitted.y) * image_h / fitted.h).clamp(0.0, image_h);
    let dst = gfx::RectF::new(left, top, right - left, bottom - top);
    let src = gfx::RectF::new(src_left, src_top, src_right - src_left, src_bottom - src_top);
    (rect_finite_positive(src) && rect_finite_positive(dst)).then_some((dst, src))
}

#[inline]
fn rect_finite_positive(rect: gfx::RectF) -> bool {
    rect.x.is_finite()
        && rect.y.is_finite()
        && rect.w.is_finite()
        && rect.h.is_finite()
        && rect.w > 0.0
        && rect.h > 0.0
}

pub struct NineSliceImage {
    pub tex: gfx::ImageHandle,
    pub slice: gfx::Insets,
    pub alpha: f32,
}

impl NineSliceImage {
    pub fn encode(&self, rect: gfx::RectF, b: &mut DrawListBuilder) {
        b.nine_slice(self.tex, rect, self.slice, self.alpha);
    }
}

// ----- Text Input, Overlay, Picker -----

#[derive(Clone, Copy, Debug)]
pub struct TextInputStyle {
    pub padding: gfx::Insets,
    pub font_id: usize,
    pub font_px: f32,
    pub placeholder_font_px: f32,
    pub placeholder_offset: f32,
    pub background: gfx::Color,
    pub background_focus: gfx::Color,
    pub background_invalid: gfx::Color,
    pub border: gfx::Color,
    pub border_focus: gfx::Color,
    pub border_invalid: gfx::Color,
    pub text: gfx::Color,
    pub placeholder: gfx::Color,
    pub caret: gfx::Color,
    pub selection: gfx::Color,
    pub composition: gfx::Color,
}

impl Default for TextInputStyle {
    fn default() -> Self {
        Self {
            padding: gfx::Insets::new(10.0, 12.0, 10.0, 12.0),
            font_id: 0,
            font_px: 16.0,
            placeholder_font_px: 14.0,
            placeholder_offset: 12.0,
            background: gfx::Color::rgba(0.98, 0.99, 1.0, 1.0),
            background_focus: gfx::Color::rgba(0.94, 0.97, 1.0, 1.0),
            background_invalid: gfx::Color::rgba(1.0, 0.96, 0.96, 1.0),
            border: gfx::Color::rgba(0.82, 0.85, 0.89, 1.0),
            border_focus: gfx::Color::rgba(0.35, 0.55, 1.0, 1.0),
            border_invalid: gfx::Color::rgba(0.90, 0.30, 0.30, 1.0),
            text: gfx::Color::rgba(0.14, 0.14, 0.18, 1.0),
            placeholder: gfx::Color::rgba(0.48, 0.51, 0.56, 1.0),
            caret: gfx::Color::rgba(0.22, 0.36, 0.82, 1.0),
            selection: gfx::Color::rgba(0.25, 0.52, 0.95, 0.20),
            composition: gfx::Color::rgba(0.25, 0.52, 0.95, 0.38),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextValidation {
    Pending,
    Valid,
    Invalid,
}

#[derive(Clone, Debug)]
struct CompositionRange {
    range: Range<usize>,
    text: String,
}

#[derive(Clone)]
enum TextFilter {
    Any,
    Digits,
    Alphanumeric,
    Custom(Arc<dyn Fn(char) -> bool + Send + Sync>),
}

impl TextFilter {
    fn allows(&self, ch: char) -> bool {
        match self {
            Self::Any => true,
            Self::Digits => ch.is_ascii_digit(),
            Self::Alphanumeric => ch.is_alphanumeric(),
            Self::Custom(filter) => filter(ch),
        }
    }
}

impl Default for TextFilter {
    fn default() -> Self {
        Self::Any
    }
}

#[derive(Clone, Debug)]
pub struct AccessoryButton {
    pub id: u32,
    pub title: String,
}

#[derive(Clone, Copy, Debug)]
pub struct OtpConfig {
    length: usize,
    gap: f32,
    placeholder: char,
}

pub struct TextInputState {
    text: String,
    text_is_ascii: bool,
    placeholder: String,
    secure: bool,
    focused: bool,
    cursor: usize,
    selection: Option<Range<usize>>,
    composition: Option<CompositionRange>,
    validator: Option<Box<dyn Fn(&str) -> bool + Send + Sync>>,
    validation: TextValidation,
    placeholder_t: f32,
    caret_timer: u32,
    caret_on: bool,
    keyboard: TextInputConfig,
    max_len_chars: Option<usize>,
    filter: TextFilter,
    otp: Option<OtpConfig>,
    accessories: Vec<AccessoryButton>,
    submit: bool,
    ime_rect: Option<gfx::RectF>,
}

impl TextInputState {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            text: String::new(),
            text_is_ascii: true,
            placeholder: placeholder.into(),
            secure: false,
            focused: false,
            cursor: 0,
            selection: None,
            composition: None,
            validator: None,
            validation: TextValidation::Pending,
            placeholder_t: 0.0,
            caret_timer: 0,
            caret_on: true,
            keyboard: TextInputConfig::default(),
            max_len_chars: None,
            filter: TextFilter::default(),
            otp: None,
            accessories: Vec::new(),
            submit: false,
            ime_rect: None,
        }
    }

    pub fn with_secure(placeholder: impl Into<String>, secure: bool) -> Self {
        let mut s = Self::new(placeholder);
        s.secure = secure;
        s.keyboard.autocorrect = false;
        s.keyboard.autocapitalization = AutoCapitalization::None;
        s.keyboard.content_type =
            if secure { TextContentType::Password } else { TextContentType::Plain };
        s
    }

    pub fn set_validator<F>(&mut self, validator: F)
    where
        F: Fn(&str) -> bool + Send + Sync + 'static,
    {
        self.validator = Some(Box::new(validator));
        self.revalidate();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, value: impl Into<String>) {
        self.text = value.into();
        self.text_is_ascii = self.text.is_ascii();
        self.selection = None;
        self.apply_constraints();
        self.revalidate();
        self.reset_caret();
    }

    pub fn secure(&self) -> bool {
        self.secure
    }

    pub fn set_secure(&mut self, secure: bool) {
        self.secure = secure;
        if secure {
            self.keyboard.autocorrect = false;
            self.keyboard.autocapitalization = AutoCapitalization::None;
            self.keyboard.content_type = TextContentType::Password;
        } else if self.keyboard.content_type == TextContentType::Password {
            self.keyboard.content_type = TextContentType::Plain;
        }
    }

    pub fn keyboard_config(&self) -> &TextInputConfig {
        &self.keyboard
    }

    pub fn set_keyboard_config(&mut self, config: TextInputConfig) {
        self.keyboard = config;
    }

    pub fn set_autocorrect(&mut self, enabled: bool) {
        self.keyboard.autocorrect = enabled;
    }

    pub fn set_autocapitalization(&mut self, mode: AutoCapitalization) {
        self.keyboard.autocapitalization = mode;
    }

    pub fn set_keyboard_appearance(&mut self, appearance: KeyboardAppearance) {
        self.keyboard.keyboard = appearance;
    }

    pub fn set_return_key(&mut self, key: ReturnKeyType) {
        self.keyboard.return_key = key;
    }

    pub fn set_content_type(&mut self, ty: TextContentType) {
        self.keyboard.content_type = ty;
    }

    pub fn max_length(&self) -> Option<usize> {
        self.max_len_chars
    }

    pub fn set_max_length(&mut self, len: Option<usize>) {
        self.max_len_chars = len;
        self.apply_constraints();
        self.revalidate();
        self.reset_caret();
    }

    pub fn set_filter_digits(&mut self) {
        self.filter = TextFilter::Digits;
        self.apply_constraints();
        self.revalidate();
        self.reset_caret();
    }

    pub fn set_filter_alphanumeric(&mut self) {
        self.filter = TextFilter::Alphanumeric;
        self.apply_constraints();
        self.revalidate();
        self.reset_caret();
    }

    pub fn set_filter_custom<F>(&mut self, filter: F)
    where
        F: Fn(char) -> bool + Send + Sync + 'static,
    {
        self.filter = TextFilter::Custom(Arc::new(filter));
        self.apply_constraints();
        self.revalidate();
        self.reset_caret();
    }

    pub fn clear_filter(&mut self) {
        self.filter = TextFilter::Any;
        self.apply_constraints();
        self.revalidate();
        self.reset_caret();
    }

    pub fn configure_one_time_code(&mut self, length: usize) {
        self.secure = false;
        self.keyboard.autocorrect = false;
        self.keyboard.autocapitalization = AutoCapitalization::None;
        self.keyboard.content_type = TextContentType::OneTimeCode;
        self.max_len_chars = Some(length);
        self.filter = TextFilter::Digits;
        self.otp = Some(OtpConfig { length, gap: 12.0, placeholder: '–' });
        self.apply_constraints();
        self.revalidate();
        self.reset_caret();
    }

    pub fn clear_one_time_code(&mut self) {
        self.otp = None;
        if self.keyboard.content_type == TextContentType::OneTimeCode {
            self.keyboard.content_type = TextContentType::Plain;
        }
        self.apply_constraints();
        self.revalidate();
        self.reset_caret();
    }

    pub fn otp_config(&self) -> Option<OtpConfig> {
        self.otp
    }

    pub fn add_accessory_button(&mut self, title: impl Into<String>) -> u32 {
        let id = self.accessories.len() as u32;
        self.accessories.push(AccessoryButton { id, title: title.into() });
        id
    }

    pub fn clear_accessory_buttons(&mut self) {
        self.accessories.clear();
    }

    pub fn accessory_buttons(&self) -> &[AccessoryButton] {
        &self.accessories
    }

    pub fn validation(&self) -> TextValidation {
        self.validation
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    pub fn cursor_index(&self) -> usize {
        self.cursor
    }

    pub fn focus(&mut self) {
        if !self.focused {
            self.focused = true;
            self.reset_caret();
        }
    }

    pub fn blur(&mut self) {
        if self.focused {
            self.focused = false;
            self.selection = None;
            self.composition = None;
        }
    }

    pub fn set_selection(&mut self, start: usize, end: usize) {
        if start >= end {
            self.selection = None;
            self.cursor = self.clamp_cursor_index(end);
        } else {
            let s = self.clamp_cursor_index(start);
            let e = self.clamp_cursor_index(end);
            self.selection = Some(s..e);
            self.cursor = e;
        }
        self.reset_caret();
    }

    pub fn move_cursor_to_end(&mut self) {
        self.cursor = self.text_cursor_len();
        self.selection = None;
        self.reset_caret();
    }

    pub fn copy_selection_to_clipboard(&self) -> bool {
        if let Some(sel) = &self.selection {
            if sel.start < sel.end {
                let range = self.cursor_range_to_byte(sel.start..sel.end);
                if let Some(slice) = self.text.get(range) {
                    return clipboard::write_string(slice);
                }
            }
        }
        false
    }

    pub fn cut_selection_to_clipboard(&mut self) -> bool {
        if let Some(sel) = self.selection.clone() {
            if sel.start < sel.end {
                let range = self.cursor_range_to_byte(sel.start..sel.end);
                let success =
                    self.text.get(range).map_or(false, |slice| clipboard::write_string(slice));
                self.erase(sel.clone());
                self.cursor = sel.start.min(self.text_cursor_len());
                self.reset_caret();
                return success;
            }
        }
        false
    }

    pub fn paste_from_clipboard(&mut self) -> bool {
        if let Some(data) = clipboard::read_string() {
            if data.is_empty() {
                return false;
            }
            let before = self.text_cursor_len();
            let _ = self.insert(&data);
            return self.text_cursor_len() != before;
        }
        false
    }

    pub fn tick(&mut self, dt_ms: u32) {
        self.placeholder_t = text_floating_placeholder_tick(
            self.placeholder_t,
            self.focused,
            self.text.is_empty(),
            dt_ms,
        );
        if self.focused && self.selection.is_none() {
            self.caret_timer = self.caret_timer.saturating_add(dt_ms);
            if self.caret_timer >= 520 {
                self.caret_timer = 0;
                self.caret_on = !self.caret_on;
            }
        } else {
            self.caret_timer = 0;
            self.caret_on = true;
        }
    }

    pub fn take_submit(&mut self) -> bool {
        if self.submit {
            self.submit = false;
            true
        } else {
            false
        }
    }

    pub fn ime_rect(&self) -> Option<gfx::RectF> {
        self.ime_rect
    }

    pub fn handle_pointer(
        &mut self,
        local: [f32; 2],
        style: &TextInputStyle,
        text_ctx: &mut TextCtx,
    ) {
        self.focus();
        self.selection = None;
        self.cursor = self.pick_cursor(local[0], style, text_ctx);
        self.reset_caret();
    }

    pub fn handle_key(&mut self, key: &KeyEvent) {
        if !self.focused {
            return;
        }
        match key.code {
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete_forward(),
            KeyCode::ArrowLeft => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                self.selection = None;
                self.reset_caret();
            }
            KeyCode::ArrowRight => {
                let end = self.text_cursor_len();
                if self.cursor < end {
                    self.cursor += 1;
                }
                self.selection = None;
                self.reset_caret();
            }
            KeyCode::Enter => {
                self.submit = true;
            }
            KeyCode::Letter('A') if key.modifiers.contains(Modifiers::META) => {
                let len = self.text_cursor_len();
                if len > 0 {
                    self.selection = Some(0..len);
                }
            }
            _ => {}
        }
    }

    pub fn handle_text_event(&mut self, event: &TextEvent) {
        match event {
            TextEvent::Commit { text } => {
                if !self.focused {
                    return;
                }
                if text == "\n" {
                    self.submit = true;
                } else {
                    if let Some(comp) = self.composition.take() {
                        self.selection = None;
                        self.erase(comp.range.clone());
                        self.cursor = comp.range.start.min(self.text_cursor_len());
                    }
                    let _ = self.insert(text);
                }
            }
            TextEvent::Composition { range, text } => {
                if text.is_empty() {
                    self.composition = None;
                } else {
                    let start = self.clamp_cursor_index(range.start as usize);
                    let end = self.clamp_cursor_index(range.end as usize);
                    let (start, end) = if start <= end { (start, end) } else { (end, start) };
                    self.composition =
                        Some(CompositionRange { range: start..end, text: text.clone() });
                }
            }
            TextEvent::SelectionChanged { range } => {
                let start = self.clamp_cursor_index(range.start as usize);
                let end = self.clamp_cursor_index(range.end as usize);
                if start >= end {
                    self.selection = None;
                    self.cursor = end;
                } else {
                    self.selection = Some(start..end);
                    self.cursor = end;
                }
                self.reset_caret();
            }
            TextEvent::IMEShown(rect) => {
                self.ime_rect = Some(*rect);
            }
            TextEvent::IMEHidden => {
                self.ime_rect = None;
                self.composition = None;
            }
        }
    }

    fn backspace(&mut self) {
        if let Some(range) = self.selection.take() {
            self.erase(range.clone());
            self.cursor = range.start;
        } else if self.cursor > 0 {
            let start = self.cursor - 1;
            self.erase(start..self.cursor);
            self.cursor = start;
        }
        self.reset_caret();
    }

    fn delete_forward(&mut self) {
        if let Some(range) = self.selection.take() {
            self.erase(range.clone());
            self.cursor = range.start;
        } else {
            let len = self.text_cursor_len();
            if self.cursor < len {
                self.erase(self.cursor..self.cursor + 1);
            }
        }
        self.reset_caret();
    }

    fn insert(&mut self, value: &str) -> bool {
        if !self.focused {
            return false;
        }
        if let Some(range) = self.selection.take() {
            self.erase(range.clone());
            self.cursor = range.start;
        }
        let sanitized = self.sanitize_input(value);
        if sanitized.is_empty() {
            self.reset_caret();
            return false;
        }
        let inserted = sanitized.as_ref();
        let inserted_is_ascii = inserted.is_ascii();
        let inserted_len =
            if inserted_is_ascii { inserted.len() } else { text_cursor_len(inserted) };
        let byte = self.cursor_range_to_byte(self.cursor..self.cursor).start;
        self.text.insert_str(byte, inserted);
        self.text_is_ascii &= inserted_is_ascii;
        self.cursor += inserted_len;
        if !self.accepts_unconstrained_text() {
            self.apply_constraints();
        }
        self.revalidate();
        self.reset_caret();
        true
    }

    fn erase(&mut self, range: Range<usize>) {
        let bytes = self.cursor_range_to_byte(range);
        if bytes.start < bytes.end {
            self.text.drain(bytes);
            if self.accepts_unconstrained_text()
                && self.selection.is_none()
                && self.composition.is_none()
            {
                self.revalidate();
                return;
            }
            self.apply_constraints();
            self.revalidate();
        }
    }

    fn revalidate(&mut self) {
        self.validation = if let Some(v) = &self.validator {
            if self.text.is_empty() {
                TextValidation::Pending
            } else if v(&self.text) {
                TextValidation::Valid
            } else {
                TextValidation::Invalid
            }
        } else if self.text.is_empty() {
            TextValidation::Pending
        } else {
            TextValidation::Valid
        };
    }

    fn reset_caret(&mut self) {
        self.caret_timer = 0;
        self.caret_on = true;
    }

    #[inline]
    fn accepts_unconstrained_text(&self) -> bool {
        matches!(&self.filter, TextFilter::Any) && self.max_len_chars.is_none()
    }

    fn sanitize_input<'a>(&self, value: &'a str) -> Cow<'a, str> {
        if value.is_empty() {
            return Cow::Borrowed(value);
        }
        if self.accepts_unconstrained_text() {
            return Cow::Borrowed(value);
        }
        let mut out = String::new();
        let current_len = self.max_len_chars.map(|_| self.text_cursor_len()).unwrap_or(0);
        let mut added = 0usize;
        let boundaries = text_boundary::cluster_boundaries(value);
        for pair in boundaries.windows(2) {
            let cluster = &value[pair[0]..pair[1]];
            if !cluster.chars().all(|ch| self.filter.allows(ch)) {
                continue;
            }
            if let Some(max) = self.max_len_chars {
                if current_len + added >= max {
                    break;
                }
            }
            out.push_str(cluster);
            added += 1;
        }
        Cow::Owned(out)
    }

    fn apply_constraints(&mut self) {
        let len = if self.accepts_unconstrained_text() {
            self.text_cursor_len()
        } else {
            let mut filtered = String::new();
            let mut count = 0usize;
            let boundaries = text_boundary::cluster_boundaries(&self.text);
            for pair in boundaries.windows(2) {
                let cluster = &self.text[pair[0]..pair[1]];
                if !cluster.chars().all(|ch| self.filter.allows(ch)) {
                    continue;
                }
                if let Some(max) = self.max_len_chars {
                    if count >= max {
                        break;
                    }
                }
                filtered.push_str(cluster);
                count += 1;
            }
            if filtered != self.text {
                self.text = filtered;
                self.text_is_ascii = self.text.is_ascii();
            }
            self.text_cursor_len()
        };
        self.cursor = self.cursor.min(len);
        if let Some(sel) = &mut self.selection {
            let start = sel.start.min(len);
            let end = sel.end.min(len);
            if start >= end {
                self.selection = None;
            } else {
                sel.start = start;
                sel.end = end;
            }
        }
        if let Some(comp) = &mut self.composition {
            comp.range.start = comp.range.start.min(len);
            comp.range.end = comp.range.end.min(len);
            if comp.range.start > comp.range.end || comp.text.is_empty() {
                self.composition = None;
            }
        }
    }

    fn pick_cursor(&self, x: f32, style: &TextInputStyle, text_ctx: &mut TextCtx) -> usize {
        let display = self.display_text();
        let len = text_cursor_len(&display);
        let Some(metrics) = text_ctx.cached_prefix_metrics(&display, style.font_id, style.font_px)
        else {
            return 0;
        };
        metrics.map.cursor_for_x(x - style.padding.left).min(len)
    }

    fn display_text(&self) -> String {
        if self.otp.is_some() {
            return self.text.clone();
        }
        if !self.secure {
            if let Some(comp) = &self.composition {
                let bytes = cursor_range_to_byte(&self.text, comp.range.clone());
                let mut display = String::with_capacity(
                    self.text.len().saturating_sub(bytes.end.saturating_sub(bytes.start))
                        + comp.text.len(),
                );
                display.push_str(&self.text[..bytes.start]);
                display.push_str(&comp.text);
                display.push_str(&self.text[bytes.end..]);
                display
            } else {
                self.text.clone()
            }
        } else {
            core::iter::repeat('•').take(self.text_cursor_len()).collect()
        }
    }

    #[inline]
    fn text_cursor_len(&self) -> usize {
        if self.text_is_ascii {
            self.text.len()
        } else {
            text_cursor_len(&self.text)
        }
    }

    #[inline]
    fn clamp_cursor_index(&self, idx: usize) -> usize {
        idx.min(self.text_cursor_len())
    }

    #[inline]
    fn cursor_range_to_byte(&self, range: Range<usize>) -> Range<usize> {
        if self.text_is_ascii {
            return range.start.min(self.text.len())..range.end.min(self.text.len());
        }
        cursor_range_to_byte(&self.text, range)
    }
}

pub struct TextInput {
    pub style: TextInputStyle,
    pub corner_radius: f32,
}

impl Default for TextInput {
    fn default() -> Self {
        Self { style: TextInputStyle::default(), corner_radius: 10.0 }
    }
}

impl TextInput {
    #[allow(clippy::too_many_arguments)]
    pub fn encode<U: ImageUploader>(
        &self,
        state: &TextInputState,
        rect: gfx::RectF,
        device_scale: f32,
        text_ctx: &mut TextCtx,
        uploader: &mut U,
        builder: &mut DrawListBuilder,
    ) {
        let style = &self.style;
        let bg = match state.validation {
            TextValidation::Invalid => style.background_invalid,
            _ => {
                if state.focused {
                    style.background_focus
                } else {
                    style.background
                }
            }
        };
        let border = match state.validation {
            TextValidation::Invalid => style.border_invalid,
            _ => {
                if state.focused {
                    style.border_focus
                } else {
                    style.border
                }
            }
        };
        builder.rrect(rect, [self.corner_radius; 4], border);
        let inner = gfx::RectF::new(rect.x + 1.5, rect.y + 1.5, rect.w - 3.0, rect.h - 3.0);
        builder.rrect(inner, [self.corner_radius - 1.5; 4], bg);

        let content = gfx::RectF::new(
            inner.x + style.padding.left,
            inner.y + style.padding.top,
            inner.w - style.padding.left - style.padding.right,
            inner.h - style.padding.top - style.padding.bottom,
        );

        let display = state.display_text();

        if let Some(cfg) = state.otp_config() {
            self.encode_otp(
                state,
                style,
                cfg,
                content,
                device_scale,
                text_ctx,
                uploader,
                builder,
                &display,
            );
            text_ctx.flush_after_encoding(uploader, builder);
            return;
        }

        let prefix_metrics = text_ctx.cached_prefix_metrics(&display, style.font_id, style.font_px);

        if let Some(sel) = &state.selection {
            if sel.start < sel.end {
                let sx = content.x
                    + prefix_metrics
                        .as_ref()
                        .map_or(0.0, |metrics| metrics.map.width_at(sel.start));
                let ex = content.x
                    + prefix_metrics
                        .as_ref()
                        .map_or(sx - content.x, |metrics| metrics.map.width_at(sel.end));
                let highlight =
                    gfx::RectF::new(sx, content.y - 2.0, (ex - sx).max(1.0), style.font_px + 6.0);
                builder.rrect(highlight, [4.0; 4], style.selection);
            }
        }

        if let Some(comp) = &state.composition {
            let marked_len = text_cursor_len(&comp.text);
            let display_end = comp.range.start.saturating_add(marked_len).max(comp.range.end);
            if comp.range.start < display_end {
                let sx = content.x
                    + prefix_metrics
                        .as_ref()
                        .map_or(0.0, |metrics| metrics.map.width_at(comp.range.start));
                let ex = content.x
                    + prefix_metrics
                        .as_ref()
                        .map_or(sx - content.x, |metrics| metrics.map.width_at(display_end));
                let underline =
                    gfx::RectF::new(sx, content.y + style.font_px + 2.0, (ex - sx).max(1.0), 2.0);
                builder.rrect(underline, [1.0; 4], style.composition);
            }
        }

        let Some(layout) = text_ctx.cached_label_layout::<false>(
            &display,
            style.font_id,
            style.font_px,
            false,
            f32::INFINITY,
        ) else {
            return;
        };
        let Some(line) = layout.lines.first() else {
            return;
        };
        let _ = bake_cached_label_line::<false>(
            line,
            style.text,
            content.x,
            content.y,
            device_scale,
            text_ctx,
            builder,
        );

        if state.focused && state.caret_on {
            let caret_w =
                prefix_metrics.as_ref().map_or(0.0, |metrics| metrics.map.width_at(state.cursor));
            let caret_rect =
                gfx::RectF::new(content.x + caret_w, content.y - 1.0, 1.5, style.font_px + 4.0);
            builder.rrect(caret_rect, [0.8; 4], style.caret);
        }

        if state.text.is_empty() || state.placeholder_t > 0.01 {
            let px =
                style.font_px + (style.placeholder_font_px - style.font_px) * state.placeholder_t;
            let line_h = (px * 1.25).ceil();
            let inline_y = content.y + (content.h - line_h) * 0.50;
            let floating_y = content.y - style.placeholder_offset;
            let y = inline_y + (floating_y - inline_y) * state.placeholder_t;
            let ph_rect = gfx::RectF::new(content.x, y, content.w, line_h + 2.0);
            encode_label_unwrapped(
                &state.placeholder,
                style.placeholder,
                Align::Center,
                style.font_id,
                px,
                ph_rect,
                device_scale,
                text_ctx,
                uploader,
                builder,
            );
        }

        text_ctx.flush_after_encoding(uploader, builder);
    }

    #[allow(clippy::too_many_arguments)]
    fn encode_otp<U: ImageUploader>(
        &self,
        state: &TextInputState,
        style: &TextInputStyle,
        cfg: OtpConfig,
        content: gfx::RectF,
        device_scale: f32,
        text_ctx: &mut TextCtx,
        _uploader: &mut U,
        builder: &mut DrawListBuilder,
        display: &str,
    ) {
        if cfg.length == 0 {
            return;
        }
        let length = cfg.length;
        let chars: Vec<char> = display.chars().collect();
        let total_gap = cfg.gap * (length.saturating_sub(1) as f32);
        let slot_w = ((content.w - total_gap).max(0.0)) / (length as f32);
        let slot_h = content.h;
        let mut x = content.x;
        for idx in 0..length {
            let slot_rect = gfx::RectF::new(x, content.y, slot_w, slot_h);
            let inner = gfx::RectF::new(
                slot_rect.x + 2.0,
                slot_rect.y + 4.0,
                slot_rect.w - 4.0,
                slot_rect.h - 8.0,
            );
            let focus = state.focused && state.cursor.min(length) == idx;
            let border_color = if focus { style.border_focus } else { style.border };
            builder.rrect(
                slot_rect,
                [6.0; 4],
                gfx::Color::rgba(border_color.r, border_color.g, border_color.b, 0.18),
            );

            if let Some(font) = text_ctx.fonts.font(style.font_id) {
                let glyph_char = chars
                    .get(idx)
                    .copied()
                    .filter(|c| !c.is_whitespace())
                    .unwrap_or(cfg.placeholder);
                let glyph = glyph_char.to_string();
                if let Ok(shape) = text_ctx.shaper.shape(font, style.font_id, &glyph, style.font_px)
                {
                    let width = shape.width();
                    let text_x = inner.x + (inner.w - width).max(0.0) * 0.5;
                    let text_y = inner.y + (inner.h - style.font_px).max(0.0) * 0.5;
                    let color =
                        if chars.get(idx).is_some() { style.text } else { style.placeholder };
                    let mut runs = core::mem::take(&mut text_ctx.frame.glyph_runs);
                    runs.clear();
                    let dl = builder.drawlist_mut();
                    shape.bake_paged_into_with(
                        &mut text_ctx.raster,
                        &mut text_ctx.atlas,
                        &mut dl.vertices,
                        &mut dl.indices,
                        &mut runs,
                        color,
                        text_x,
                        text_y,
                        device_scale,
                    );
                    for run in runs.iter().copied() {
                        builder.glyph_run_provisional(run);
                    }
                    runs.clear();
                    text_ctx.frame.glyph_runs = runs;
                }
            }

            x += slot_w + cfg.gap;
        }

        if state.focused && state.caret_on {
            let caret_index = state.cursor.min(length);
            let caret_base = if caret_index == length {
                content.x + (length.saturating_sub(1) as f32) * (slot_w + cfg.gap) + slot_w
            } else {
                content.x + (caret_index as f32) * (slot_w + cfg.gap)
            };
            let caret_rect = gfx::RectF::new(caret_base - 0.75, content.y + 4.0, 1.5, slot_h - 8.0);
            builder.rrect(caret_rect, [0.8; 4], style.caret);
        }
    }
}

#[inline]
fn cursor_range_to_byte(s: &str, range: Range<usize>) -> Range<usize> {
    text_boundary::cluster_range_to_byte(s, range)
}

#[inline]
fn text_cursor_len(s: &str) -> usize {
    text_boundary::cluster_count(s)
}

/// Fullscreen modal-overlay blur parameters after resolving the current viewport and animation state.
#[derive(Clone, Copy, Debug)]
pub struct OverlayStyle {
    pub tint: gfx::Color,
    pub alpha: f32,
    pub blur_sigma: f32,
}

impl Default for OverlayStyle {
    fn default() -> Self {
        Self { tint: gfx::Color::rgba(0.0, 0.0, 0.0, 1.0), alpha: 0.90, blur_sigma: 18.0 }
    }
}

/// Resolved fullscreen backdrop draw for a modal overlay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackdropSpec {
    pub rect: gfx::RectF,
    pub sigma: f32,
    pub tint: gfx::Color,
    pub alpha: f32,
}

pub struct OverlayState {
    visible: bool,
    t: f32,
    vel: f32,
}

impl OverlayState {
    pub fn new() -> Self {
        Self { visible: false, t: 0.0, vel: 0.0 }
    }

    pub fn is_visible(&self) -> bool {
        self.visible || self.t > 0.01
    }

    pub fn open(&mut self) {
        self.visible = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
    }

    pub fn toggle(&mut self) {
        if self.visible {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn tick(&mut self, dt_ms: u32) {
        let target = if self.visible { 1.0 } else { 0.0 };
        let dt = dt_ms as f32 / 16.0;
        let diff = target - self.t;
        self.vel += diff * 0.12 * dt;
        self.vel *= 0.82_f32.powf(dt);
        self.t = (self.t + self.vel).clamp(0.0, 1.0);
        if !self.visible && self.t < 0.01 {
            self.t = 0.0;
            self.vel = 0.0;
        }
    }

    pub fn progress(&self) -> f32 {
        self.t
    }
}

pub struct Overlay {
    pub style: OverlayStyle,
}

impl Default for Overlay {
    fn default() -> Self {
        Self { style: OverlayStyle::default() }
    }
}

impl Overlay {
    /// Resolves the fullscreen backdrop draw that matches the legacy iOS modal overlay contract.
    #[must_use]
    pub fn backdrop_spec(
        &self,
        rect: gfx::RectF,
        progress: f32,
        device_scale: f32,
    ) -> Option<BackdropSpec> {
        let alpha = self.style.alpha * progress.clamp(0.0, 1.0);
        if alpha <= f32::EPSILON {
            return None;
        }
        Some(BackdropSpec {
            rect,
            sigma: self.style.blur_sigma * device_scale,
            tint: self.style.tint,
            alpha,
        })
    }

    pub fn encode(
        &self,
        state: &OverlayState,
        viewport: gfx::RectF,
        device_scale: f32,
        builder: &mut DrawListBuilder,
    ) -> bool {
        if !state.is_visible() {
            return false;
        }
        let Some(backdrop) = self.backdrop_spec(viewport, state.progress(), device_scale) else {
            return false;
        };
        builder.backdrop(backdrop.rect, backdrop.sigma, backdrop.tint, backdrop.alpha);
        true
    }
}

/// Shared popup chrome parameters for the legacy iOS blur-card treatment.
#[derive(Clone, Copy, Debug)]
pub struct PopupStyle {
    pub blur_tint: gfx::Color,
    pub panel_backdrop_alpha: f32,
    pub panel_backdrop_sigma: f32,
    pub corner_radius_scale: f32,
    pub border_width_points: f32,
    pub shell_color: gfx::Color,
    pub inner_fill_color: gfx::Color,
}

impl Default for PopupStyle {
    fn default() -> Self {
        Self {
            blur_tint: gfx::Color::rgba(0.0, 0.0, 0.0, 1.0),
            panel_backdrop_alpha: 0.50,
            panel_backdrop_sigma: 16.0,
            corner_radius_scale: 0.09,
            border_width_points: 1.0,
            shell_color: gfx::Color::rgba(236.0 / 255.0, 240.0 / 255.0, 241.0 / 255.0, 1.0),
            inner_fill_color: gfx::Color::rgba(0.0, 0.0, 0.0, 0.18),
        }
    }
}

/// Resolved geometry and blur state for a popup panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupChrome {
    pub panel_backdrop: BackdropSpec,
    pub panel_rect: gfx::RectF,
    pub panel_radius: f32,
    pub border_width: f32,
    pub panel_inner_rect: gfx::RectF,
    pub panel_inner_radius: f32,
}

pub struct PopupWindow {
    pub style: PopupStyle,
}

impl Default for PopupWindow {
    fn default() -> Self {
        Self { style: PopupStyle::default() }
    }
}

impl PopupWindow {
    /// Resolves the legacy popup blur-card chrome for a panel rect at the current device scale.
    #[must_use]
    pub fn chrome(&self, rect: gfx::RectF, device_scale: f32) -> PopupChrome {
        let border_width = (self.style.border_width_points * device_scale).max(1.0);
        let panel_radius = rect.w * self.style.corner_radius_scale;
        let panel_inner_rect = gfx::RectF::new(
            rect.x + border_width,
            rect.y + border_width,
            (rect.w - border_width * 2.0).max(0.0),
            (rect.h - border_width * 2.0).max(0.0),
        );
        let panel_inner_radius = (panel_radius - border_width).max(0.0);
        PopupChrome {
            panel_backdrop: BackdropSpec {
                rect,
                sigma: self.style.panel_backdrop_sigma * device_scale,
                tint: self.style.blur_tint,
                alpha: self.style.panel_backdrop_alpha,
            },
            panel_rect: rect,
            panel_radius,
            border_width,
            panel_inner_rect,
            panel_inner_radius,
        }
    }

    pub fn encode(&self, rect: gfx::RectF, device_scale: f32, builder: &mut DrawListBuilder) {
        let chrome = self.chrome(rect, device_scale);
        builder.backdrop(
            chrome.panel_backdrop.rect,
            chrome.panel_backdrop.sigma,
            chrome.panel_backdrop.tint,
            chrome.panel_backdrop.alpha,
        );
        builder.rrect(chrome.panel_rect, [chrome.panel_radius; 4], self.style.shell_color);
        if chrome.panel_inner_rect.w > 0.0 && chrome.panel_inner_rect.h > 0.0 {
            builder.rrect(
                chrome.panel_inner_rect,
                [chrome.panel_inner_radius; 4],
                self.style.inner_fill_color,
            );
        }
    }
}

#[derive(Clone, Debug)]
struct PickerColumn {
    items: Vec<String>,
    offset: f32,
    velocity: f32,
    selection: usize,
}

impl PickerColumn {
    fn new(items: Vec<String>) -> Self {
        let mut column = Self { items, offset: 0.0, velocity: 0.0, selection: 0 };
        column.set_selection(0);
        column
    }

    fn set_items(&mut self, items: Vec<String>) {
        self.items = items;
        self.set_selection(self.selection);
    }

    fn set_selection(&mut self, selection: usize) {
        if self.items.is_empty() {
            self.offset = 0.0;
            self.velocity = 0.0;
            self.selection = 0;
            return;
        }
        self.selection = selection.min(self.items.len() - 1);
        self.offset = self.selection as f32;
        self.velocity = 0.0;
    }

    fn selection_label(&self) -> Option<&str> {
        self.items.get(self.selection).map(String::as_str)
    }

    fn position(&self) -> f32 {
        self.offset
    }

    fn scroll(&mut self, delta: f32) {
        if self.items.is_empty() {
            return;
        }
        self.offset = clamp_picker_position(self.items.len(), self.offset - delta);
        self.selection = snap_picker_position(self.offset);
    }

    fn fling(&mut self, velocity: f32) {
        self.velocity = velocity;
    }

    fn tick(&mut self, dt_ms: u32) {
        if self.items.is_empty() {
            return;
        }
        let dt = dt_ms as f32 / 1000.0;
        self.offset += self.velocity * dt;
        self.velocity *= 0.9_f32.powf(dt_ms as f32 / 16.0);
        if self.velocity.abs() < 0.02 {
            self.velocity = 0.0;
        }
        self.offset = clamp_picker_position(self.items.len(), self.offset);
        if self.velocity.abs() < 0.02 {
            self.offset = self.offset.floor() + if self.offset.fract() > 0.5 { 1.0 } else { 0.0 };
        }
        self.selection = snap_picker_position(self.offset);
    }
}

#[inline]
fn clamp_picker_position(item_count: usize, position: f32) -> f32 {
    if item_count == 0 {
        0.0
    } else {
        position.clamp(0.0, (item_count - 1) as f32)
    }
}

#[inline]
fn snap_picker_position(position: f32) -> usize {
    let floor = position.floor();
    let fraction = position - floor;
    if fraction > 0.5 {
        floor as usize + 1
    } else {
        floor as usize
    }
}

#[derive(Clone, Debug)]
pub struct PickerState {
    columns: Vec<PickerColumn>,
}

impl PickerState {
    pub fn new(items: Vec<String>) -> Self {
        Self::from_columns(vec![items])
    }

    pub fn from_columns(columns: Vec<Vec<String>>) -> Self {
        Self { columns: columns.into_iter().map(PickerColumn::new).collect() }
    }

    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    pub fn set_columns(&mut self, columns: Vec<Vec<String>>) {
        self.columns = columns.into_iter().map(PickerColumn::new).collect();
    }

    pub fn set_items(&mut self, items: Vec<String>) {
        if let Some(column) = self.columns.get_mut(0) {
            column.set_items(items);
        } else {
            self.columns.push(PickerColumn::new(items));
        }
    }

    pub fn set_column_items(&mut self, column_index: usize, items: Vec<String>) -> bool {
        let Some(column) = self.columns.get_mut(column_index) else {
            return false;
        };
        column.set_items(items);
        true
    }

    pub fn set_column_selection(&mut self, column_index: usize, selection: usize) -> bool {
        let Some(column) = self.columns.get_mut(column_index) else {
            return false;
        };
        column.set_selection(selection);
        true
    }

    pub fn selection(&self) -> usize {
        self.column_selection(0).unwrap_or(0)
    }

    pub fn column_selection(&self, column_index: usize) -> Option<usize> {
        self.columns.get(column_index).map(|column| column.selection)
    }

    pub fn selection_label(&self) -> Option<&str> {
        self.column_selection_label(0)
    }

    pub fn column_selection_label(&self, column_index: usize) -> Option<&str> {
        self.columns.get(column_index).and_then(PickerColumn::selection_label)
    }

    pub fn column_position(&self, column_index: usize) -> Option<f32> {
        self.columns.get(column_index).map(PickerColumn::position)
    }

    pub fn scroll(&mut self, delta: f32) {
        self.scroll_column(0, delta);
    }

    pub fn scroll_column(&mut self, column_index: usize, delta: f32) -> bool {
        let Some(column) = self.columns.get_mut(column_index) else {
            return false;
        };
        column.scroll(delta);
        true
    }

    pub fn fling(&mut self, velocity: f32) {
        self.fling_column(0, velocity);
    }

    pub fn fling_column(&mut self, column_index: usize, velocity: f32) -> bool {
        let Some(column) = self.columns.get_mut(column_index) else {
            return false;
        };
        column.fling(velocity);
        true
    }

    pub fn tick(&mut self, dt_ms: u32) {
        for column in &mut self.columns {
            column.tick(dt_ms);
        }
    }

    pub fn encode<U: ImageUploader>(
        &self,
        style: &PickerStyle,
        rect: gfx::RectF,
        device_scale: f32,
        text_ctx: &mut TextCtx,
        uploader: &mut U,
        builder: &mut DrawListBuilder,
    ) {
        if self.columns.is_empty() {
            return;
        }
        let highlight = style.center_band_rect(rect);
        builder.rrect(highlight, [style.center_band_radius(rect); 4], style.highlight);

        if text_ctx.fonts.font(style.font_id).is_none() {
            return;
        }
        builder.clip_push(gfx::RectI::new(
            rect.x.floor() as i32,
            rect.y.floor() as i32,
            rect.w.ceil() as i32,
            rect.h.ceil() as i32,
        ));

        for (column_index, column) in self.columns.iter().enumerate() {
            for (item_index, label) in column.items.iter().enumerate() {
                let Some(item_rect) = style.item_rect(
                    rect,
                    self.columns.len(),
                    column_index,
                    column.position(),
                    item_index,
                ) else {
                    continue;
                };
                if item_rect.y + item_rect.h <= rect.y || item_rect.y >= rect.y + rect.h {
                    continue;
                }
                let Some(layout) = text_ctx.cached_label_layout::<false>(
                    label,
                    style.font_id,
                    style.font_px,
                    false,
                    f32::INFINITY,
                ) else {
                    continue;
                };
                let Some(line) = layout.lines.first() else {
                    continue;
                };
                let text_x = item_rect.x + (item_rect.w - line.width) * 0.50;
                let text_y =
                    item_rect.y + (item_rect.h - style.font_px) * 0.50 + style.baseline_shift;
                let _ = bake_cached_label_line::<false>(
                    line,
                    style.text_color,
                    text_x,
                    text_y,
                    device_scale,
                    text_ctx,
                    builder,
                );
            }
        }

        builder.clip_pop();
        text_ctx.flush_after_encoding(uploader, builder);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickerStyle {
    pub font_id: usize,
    pub font_px: f32,
    pub highlight: gfx::Color,
    pub text_color: gfx::Color,
    pub center_band_corner_ratio: f32,
    pub baseline_shift: f32,
}

impl PickerStyle {
    #[must_use]
    pub fn visible_rows(&self) -> usize {
        3
    }

    #[must_use]
    pub fn row_height(&self, rect: gfx::RectF) -> f32 {
        (rect.h / self.visible_rows() as f32).max(1.0)
    }

    #[must_use]
    pub fn center_band_rect(&self, rect: gfx::RectF) -> gfx::RectF {
        let row_height = self.row_height(rect);
        gfx::RectF::new(rect.x, rect.y + (rect.h - row_height) * 0.50, rect.w, row_height)
    }

    #[must_use]
    pub fn center_band_radius(&self, rect: gfx::RectF) -> f32 {
        self.row_height(rect) * self.center_band_corner_ratio
    }

    #[must_use]
    pub fn column_rect(
        &self,
        rect: gfx::RectF,
        column_count: usize,
        column_index: usize,
    ) -> Option<gfx::RectF> {
        if column_count == 0 || column_index >= column_count {
            return None;
        }
        let column_width = rect.w / column_count as f32;
        Some(gfx::RectF::new(
            rect.x + column_width * column_index as f32,
            rect.y,
            column_width,
            rect.h,
        ))
    }

    #[must_use]
    pub fn item_rect(
        &self,
        rect: gfx::RectF,
        column_count: usize,
        column_index: usize,
        column_position: f32,
        item_index: usize,
    ) -> Option<gfx::RectF> {
        let column_rect = self.column_rect(rect, column_count, column_index)?;
        let row_height = self.row_height(rect);
        let center_mid_y = rect.y + rect.h * 0.50;
        let row_mid_y = center_mid_y + (item_index as f32 - column_position) * row_height;
        Some(gfx::RectF::new(
            column_rect.x,
            row_mid_y - row_height * 0.50,
            column_rect.w,
            row_height,
        ))
    }
}

impl Default for PickerStyle {
    fn default() -> Self {
        Self {
            font_id: 0,
            font_px: 17.5,
            highlight: gfx::Color::rgba(0.82, 0.91, 1.0, 0.45),
            text_color: gfx::Color::rgba(0.12, 0.24, 0.60, 1.0),
            center_band_corner_ratio: 0.25,
            baseline_shift: 4.0,
        }
    }
}
// ----- Badge -----

const LEGACY_BADGE_EDGE_RATIO: f32 = 0.25;
const LEGACY_BADGE_X_OVERLAP_RATIO: f32 = 0.50;
const LEGACY_BADGE_IMAGE_SRC: gfx::RectF = gfx::RectF::new(0.0, 0.0, 1.0, 1.0);

#[derive(Clone, Copy, Debug)]
pub struct BadgeStyle {
    /// Legacy fallback color used when the badge image handle is unavailable.
    pub color: gfx::Color,
    /// Legacy 0.45s bounce timing from `Badge.m`.
    pub bounce_duration_ms: u32,
}

impl Default for BadgeStyle {
    fn default() -> Self {
        Self {
            color: gfx::Color::rgba(231.0 / 255.0, 76.0 / 255.0, 60.0 / 255.0, 1.0),
            bounce_duration_ms: 450,
        }
    }
}

/// Image-backed badge overlay that matches the old iOS `BadgeableButton` contract.
pub struct Badge {
    /// Full badge texture handle. When absent, the legacy red fallback circle is drawn instead.
    pub image: gfx::ImageHandle,
    pub style: BadgeStyle,
}

impl Default for Badge {
    fn default() -> Self {
        Self { image: gfx::ImageHandle(0), style: BadgeStyle::default() }
    }
}

pub struct BadgeState {
    bounce_anim_start_ms: u64,
    bounce_anim_duration_ms: u32,
    bounce_from_scale: f32,
    bounce_to_scale: f32,
}

impl Default for BadgeState {
    fn default() -> Self {
        Self {
            bounce_anim_start_ms: 0,
            bounce_anim_duration_ms: 0,
            bounce_from_scale: 1.0,
            bounce_to_scale: 1.0,
        }
    }
}

impl BadgeState {
    /// Trigger bounce animation (3.0x scale with bounce easing)
    pub fn bounce(&mut self, style: &BadgeStyle) {
        self.bounce_from_scale = self.current_scale();
        self.bounce_to_scale = 3.0;
        self.bounce_anim_start_ms = timing::now_ms();
        self.bounce_anim_duration_ms = style.bounce_duration_ms;
    }

    fn current_scale(&self) -> f32 {
        if self.bounce_anim_duration_ms == 0 {
            return 1.0;
        }
        let elapsed = timing::now_ms().saturating_sub(self.bounce_anim_start_ms) as u32;
        if elapsed >= self.bounce_anim_duration_ms {
            return 1.0; // Animation complete, return to normal
        }
        let t = elapsed as f32 / self.bounce_anim_duration_ms as f32;
        // Bounce out easing with overshoot then settle
        let scale = if t < 0.5 {
            // First half: scale up to 3.0x with elastic
            let local = t * 2.0;
            self.bounce_from_scale
                + (self.bounce_to_scale - self.bounce_from_scale) * ease_out_back(local)
        } else {
            // Second half: settle back to 1.0 with bounce
            let local = (t - 0.5) * 2.0;
            self.bounce_to_scale + (1.0 - self.bounce_to_scale) * ease_out_bounce(local)
        };
        scale.max(0.1)
    }
}

impl Badge {
    /// Resolve the unscaled legacy badge rect from the host icon bounds.
    #[must_use]
    pub fn rect(&self, icon_rect: gfx::RectF) -> gfx::RectF {
        let side = icon_rect.w * LEGACY_BADGE_EDGE_RATIO;
        gfx::RectF::new(
            icon_rect.x + icon_rect.w - side * LEGACY_BADGE_X_OVERLAP_RATIO,
            icon_rect.y,
            side,
            side,
        )
    }

    #[must_use]
    fn scaled_rect(&self, icon_rect: gfx::RectF, scale: f32) -> gfx::RectF {
        let badge_rect = self.rect(icon_rect);
        let cx = badge_rect.x + badge_rect.w * 0.5;
        let cy = badge_rect.y + badge_rect.h * 0.5;
        let w = badge_rect.w * scale;
        let h = badge_rect.h * scale;
        gfx::RectF::new(cx - w * 0.5, cy - h * 0.5, w, h)
    }

    /// Draw the badge over the host icon using the legacy top-right overlay placement.
    pub fn encode(&self, icon_rect: gfx::RectF, state: &BadgeState, b: &mut DrawListBuilder) {
        let badge_rect = self.scaled_rect(icon_rect, state.current_scale());
        if self.image.0 != 0 {
            b.image(self.image, badge_rect, LEGACY_BADGE_IMAGE_SRC, 1.0);
            return;
        }

        let radius = badge_rect.w.min(badge_rect.h) * 0.50;
        b.rrect(badge_rect, [radius; 4], self.style.color);
    }
}

fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    let t2 = t - 1.0;
    1.0 + c3 * t2.powi(3) + c1 * t2.powi(2)
}

fn ease_out_bounce(t: f32) -> f32 {
    const N1: f32 = 7.5625;
    const D1: f32 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        let t2 = t - 1.5 / D1;
        N1 * t2 * t2 + 0.75
    } else if t < 2.5 / D1 {
        let t2 = t - 2.25 / D1;
        N1 * t2 * t2 + 0.9375
    } else {
        let t2 = t - 2.625 / D1;
        N1 * t2 * t2 + 0.984375
    }
}

// ----- CountNode -----

pub struct CountNode {
    pub count: u64,
    pub label: alloc::string::String,
    pub count_font_px: f32,
    pub label_font_px: f32,
    pub count_color: gfx::Color,
    pub label_color: gfx::Color,
}

impl Default for CountNode {
    fn default() -> Self {
        Self {
            count: 0,
            label: alloc::string::String::new(),
            count_font_px: 18.0,
            label_font_px: 7.0,
            count_color: gfx::Color::rgba(0.92, 0.94, 0.95, 1.0), // ColorMaster::base
            label_color: gfx::Color::rgba(0.92, 0.94, 0.95, 1.0),
        }
    }
}

impl CountNode {
    /// Format large numbers with K/M suffixes
    fn format_count(count: u64) -> alloc::string::String {
        if count < 1000 {
            alloc::format!("{}", count)
        } else if count < 1_000_000 {
            alloc::format!("{:.1}K", count as f64 / 1000.0)
        } else {
            alloc::format!("{:.1}M", count as f64 / 1_000_000.0)
        }
    }

    pub fn encode<U: ImageUploader>(
        &self,
        rect: gfx::RectF,
        device_scale: f32,
        txt: &mut TextCtx,
        up: &mut U,
        b: &mut DrawListBuilder,
    ) {
        let count_text = Self::format_count(self.count);

        // Vertical stack: count on top, label below
        let count_height = self.count_font_px * 1.25;
        let label_height = self.label_font_px * 1.25;
        let total_height = count_height + label_height;
        let start_y = rect.y + (rect.h - total_height) * 0.5;

        let count_rect = gfx::RectF::new(rect.x, start_y, rect.w, count_height);
        encode_label_unwrapped(
            &count_text,
            self.count_color,
            Align::Center,
            0,
            self.count_font_px,
            count_rect,
            device_scale,
            txt,
            up,
            b,
        );

        // Draw label
        if !self.label.is_empty() {
            let label_rect = gfx::RectF::new(rect.x, start_y + count_height, rect.w, label_height);
            encode_label_unwrapped(
                &self.label,
                self.label_color,
                Align::Center,
                0,
                self.label_font_px,
                label_rect,
                device_scale,
                txt,
                up,
                b,
            );
        }
    }
}

// ----- RecordButton -----

#[derive(Clone, Copy, Debug)]
pub struct RecordButtonStyle {
    pub border_width: f32,
    pub border_color: gfx::Color,
    pub fill_color: gfx::Color,
    pub progress_color: gfx::Color,
    pub progress_track_color: gfx::Color,
    pub recording_timeout_ms: u32,
    pub press_animation_ms: u32,
}

impl Default for RecordButtonStyle {
    fn default() -> Self {
        Self {
            border_width: 1.5,
            border_color: gfx::Color::rgba(0.92, 0.94, 0.95, 1.0), // ColorMaster::base
            fill_color: gfx::Color::rgba(0.0, 0.0, 0.0, 0.0),      // Transparent
            progress_color: gfx::Color::rgba(0.96, 0.14, 0.35, 1.0), // Nametag red
            progress_track_color: gfx::Color::rgba(0.85, 0.85, 0.85, 0.3),
            recording_timeout_ms: 9000,
            press_animation_ms: 100,
        }
    }
}

pub struct RecordButton {
    pub style: RecordButtonStyle,
}

impl Default for RecordButton {
    fn default() -> Self {
        Self { style: RecordButtonStyle::default() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RecordButtonMode {
    Idle,
    Recording,
}

pub struct RecordButtonState {
    pub mode: RecordButtonMode,
    pressed: bool,
    recording_start_ms: u64,
    recording_duration_ms: u32,
    press_anim_start_ms: u64,
    press_anim_from: f32,
    press_anim_to: f32,
}

impl Default for RecordButtonState {
    fn default() -> Self {
        Self {
            mode: RecordButtonMode::Idle,
            pressed: false,
            recording_start_ms: 0,
            recording_duration_ms: 0, // Will be set from style when starting recording
            press_anim_start_ms: 0,
            press_anim_from: 1.0,
            press_anim_to: 1.0,
        }
    }
}

impl RecordButtonState {
    pub fn is_recording(&self) -> bool {
        self.mode == RecordButtonMode::Recording
    }

    pub fn on_pointer_down(&mut self, style: &RecordButtonStyle) {
        self.pressed = true;
        self.press_anim_from = self.current_scale(style);
        self.press_anim_to = 0.80;
        self.press_anim_start_ms = timing::now_ms();
    }

    pub fn on_pointer_up(&mut self, style: &RecordButtonStyle) -> bool {
        let was_pressed = self.pressed;
        self.pressed = false;
        self.press_anim_from = self.current_scale(style);
        self.press_anim_to = 1.0;
        self.press_anim_start_ms = timing::now_ms();
        was_pressed
    }

    pub fn start_recording(&mut self, style: &RecordButtonStyle) {
        self.mode = RecordButtonMode::Recording;
        self.recording_start_ms = timing::now_ms();
        self.recording_duration_ms = style.recording_timeout_ms;
    }

    pub fn stop_recording(&mut self) {
        self.mode = RecordButtonMode::Idle;
        self.recording_start_ms = 0;
    }

    fn current_scale(&self, style: &RecordButtonStyle) -> f32 {
        let elapsed = timing::now_ms().saturating_sub(self.press_anim_start_ms);
        let duration = style.press_animation_ms as u64;
        if elapsed > duration {
            return self.press_anim_to;
        }
        let t = elapsed as f32 / duration as f32;
        self.press_anim_from + (self.press_anim_to - self.press_anim_from) * t
    }

    fn recording_progress(&self) -> f32 {
        if self.mode != RecordButtonMode::Recording {
            return 0.0;
        }
        let elapsed = timing::now_ms().saturating_sub(self.recording_start_ms);
        (elapsed as f32 / self.recording_duration_ms as f32).clamp(0.0, 1.0)
    }

    pub fn is_timeout(&self) -> bool {
        self.recording_progress() >= 1.0
    }
}

impl RecordButton {
    pub fn encode(&self, rect: gfx::RectF, state: &RecordButtonState, b: &mut DrawListBuilder) {
        let scale = state.current_scale(&self.style);
        let cx = rect.x + rect.w * 0.5;
        let cy = rect.y + rect.h * 0.5;
        let radius = rect.w.min(rect.h) * 0.5 * scale;

        // Draw outer circle (border)
        let outer_rect = gfx::RectF::new(cx - radius, cy - radius, radius * 2.0, radius * 2.0);
        b.rrect(outer_rect, [radius; 4], self.style.border_color);

        // Draw inner circle (fill)
        let inner_radius = radius - self.style.border_width;
        if inner_radius > 0.0 {
            let inner_rect = gfx::RectF::new(
                cx - inner_radius,
                cy - inner_radius,
                inner_radius * 2.0,
                inner_radius * 2.0,
            );
            b.rrect(inner_rect, [inner_radius; 4], self.style.fill_color);
        }

        // Draw progress indicator when recording
        if state.mode == RecordButtonMode::Recording {
            let progress = state.recording_progress();
            let progress_bar_rect = gfx::RectF::new(rect.x, rect.y + rect.h + 8.0, rect.w, 4.0);

            // Track
            b.rrect(progress_bar_rect, [2.0; 4], self.style.progress_track_color);

            // Progress
            let progress_width = progress_bar_rect.w * progress;
            if progress_width > 0.5 {
                let progress_rect = gfx::RectF::new(
                    progress_bar_rect.x,
                    progress_bar_rect.y,
                    progress_width,
                    progress_bar_rect.h,
                );
                b.rrect(progress_rect, [2.0; 4], self.style.progress_color);
            }
        }
    }
}

// ----- ShiftingTextInput -----

/// Character filter for text input validation
#[derive(Clone)]
pub enum CharFilter {
    /// Allow all characters
    None,
    /// Only allow characters that pass the predicate
    Custom(Arc<dyn Fn(char) -> bool + Send + Sync>),
    /// Only letters
    Alphabetic,
    /// Only letters and hyphen
    AlphabeticHyphen,
    /// Only digits
    Digits,
    /// Letters, numbers, underscore
    Alphanumeric,
    /// Alphanumeric plus specific chars
    AlphanumericPlus(String),
}

impl core::fmt::Debug for CharFilter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CharFilter::None => write!(f, "CharFilter::None"),
            CharFilter::Custom(_) => write!(f, "CharFilter::Custom(<fn>)"),
            CharFilter::Alphabetic => write!(f, "CharFilter::Alphabetic"),
            CharFilter::AlphabeticHyphen => write!(f, "CharFilter::AlphabeticHyphen"),
            CharFilter::Digits => write!(f, "CharFilter::Digits"),
            CharFilter::Alphanumeric => write!(f, "CharFilter::Alphanumeric"),
            CharFilter::AlphanumericPlus(s) => write!(f, "CharFilter::AlphanumericPlus({:?})", s),
        }
    }
}

impl CharFilter {
    pub fn allows(&self, ch: char) -> bool {
        match self {
            CharFilter::None => true,
            CharFilter::Custom(f) => f(ch),
            CharFilter::Alphabetic => ch.is_alphabetic(),
            CharFilter::AlphabeticHyphen => ch.is_alphabetic() || ch == '-',
            CharFilter::Digits => ch.is_ascii_digit(),
            CharFilter::Alphanumeric => ch.is_alphanumeric(),
            CharFilter::AlphanumericPlus(chars) => ch.is_alphanumeric() || chars.contains(ch),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ShiftingTextInputStyle {
    pub font_px: f32,
    pub prompt_font_px: f32,
    pub text_color: gfx::Color,
    pub placeholder_color: gfx::Color,
    pub prompt_color: gfx::Color,
    pub background: gfx::Color,
    pub background_focus: gfx::Color,
    pub background_invalid: gfx::Color,
    pub corner: f32,
    pub padding: gfx::Insets,
}

impl Default for ShiftingTextInputStyle {
    fn default() -> Self {
        Self {
            font_px: 17.0,
            prompt_font_px: 10.0,
            text_color: gfx::Color::rgba(0.92, 0.94, 0.95, 1.0),
            placeholder_color: gfx::Color::rgba(0.92, 0.94, 0.95, 0.5),
            prompt_color: gfx::Color::rgba(0.92, 0.94, 0.95, 0.5),
            background: gfx::Color::rgba(0.95, 0.95, 0.95, 0.1),
            background_focus: gfx::Color::rgba(0.95, 0.95, 0.95, 0.15),
            background_invalid: gfx::Color::rgba(0.96, 0.14, 0.35, 0.2),
            corner: 8.0,
            padding: gfx::Insets { left: 12.0, top: 8.0, right: 12.0, bottom: 8.0 },
        }
    }
}

pub struct ShiftingTextInput {
    pub placeholder: String,
    pub prompt: Option<String>,
    pub max_length: Option<usize>,
    pub filter: CharFilter,
    pub style: ShiftingTextInputStyle,
}

impl Default for ShiftingTextInput {
    fn default() -> Self {
        Self {
            placeholder: String::from("text"),
            prompt: None,
            max_length: None,
            filter: CharFilter::None,
            style: ShiftingTextInputStyle::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShiftingTextValidation {
    Valid,
    Invalid,
}

pub struct ShiftingTextInputState {
    pub text: String,
    pub focused: bool,
    pub validation: ShiftingTextValidation,
    prompt_anim_t: f32, // 0.0 = centered placeholder, 1.0 = shifted prompt above
    shake_anim_start_ms: u64,
    shake_anim_cycles: u32,
}

impl Default for ShiftingTextInputState {
    fn default() -> Self {
        Self {
            text: String::new(),
            focused: false,
            validation: ShiftingTextValidation::Valid,
            prompt_anim_t: 0.0,
            shake_anim_start_ms: 0,
            shake_anim_cycles: 0,
        }
    }
}

impl ShiftingTextInputState {
    pub fn set_text(&mut self, text: String, filter: &CharFilter, max_length: Option<usize>) {
        // Filter invalid characters
        let filtered: String = text.chars().filter(|&ch| filter.allows(ch)).collect();

        // Apply max length
        if let Some(max) = max_length {
            self.text = filtered.chars().take(max).collect();
        } else {
            self.text = filtered;
        }
    }

    pub fn on_focus(&mut self) {
        self.focused = true;
    }

    pub fn on_blur(&mut self) {
        self.focused = false;
    }

    /// Trigger fail mode with shake animation
    pub fn fail(&mut self) {
        self.validation = ShiftingTextValidation::Invalid;
        self.shake_anim_start_ms = timing::now_ms();
        self.shake_anim_cycles = 6;
    }

    pub fn clear_fail(&mut self) {
        self.validation = ShiftingTextValidation::Valid;
        self.shake_anim_cycles = 0;
    }

    fn shake_offset(&self) -> f32 {
        if self.shake_anim_cycles == 0 {
            return 0.0;
        }
        let elapsed = timing::now_ms().saturating_sub(self.shake_anim_start_ms);
        let cycle_ms = 35;
        let total_duration = cycle_ms * self.shake_anim_cycles as u64 * 2;
        if elapsed >= total_duration {
            return 0.0;
        }
        let t = (elapsed % (cycle_ms * 2)) as f32 / (cycle_ms * 2) as f32;
        let amplitude = 2.0;
        if t < 0.5 {
            amplitude * (t * 4.0 - 1.0)
        } else {
            amplitude * (3.0 - t * 4.0)
        }
    }

    pub fn tick(&mut self) {
        // Update prompt animation
        let target = if self.focused || !self.text.is_empty() { 1.0 } else { 0.0 };
        let delta = target - self.prompt_anim_t;
        if delta.abs() > 0.01 {
            self.prompt_anim_t += delta * 0.15; // Smooth transition
        } else {
            self.prompt_anim_t = target;
        }
    }
}

impl ShiftingTextInput {
    #[allow(clippy::too_many_arguments)]
    pub fn encode<U: ImageUploader>(
        &self,
        state: &ShiftingTextInputState,
        rect: gfx::RectF,
        device_scale: f32,
        text_ctx: &mut TextCtx,
        uploader: &mut U,
        builder: &mut DrawListBuilder,
    ) {
        let style = &self.style;

        // Apply shake offset
        let shake_x = state.shake_offset();
        let adjusted_rect = gfx::RectF::new(rect.x + shake_x, rect.y, rect.w, rect.h);

        // Background
        let bg = match state.validation {
            ShiftingTextValidation::Invalid => style.background_invalid,
            _ => {
                if state.focused {
                    style.background_focus
                } else {
                    style.background
                }
            }
        };
        builder.rrect(adjusted_rect, [style.corner; 4], bg);

        // Determine prompt position
        let prompt_height = style.prompt_font_px * 1.25;
        let prompt_offset = state.prompt_anim_t * prompt_height;

        // Draw prompt above if animating up
        if let Some(prompt_text) = &self.prompt {
            if state.prompt_anim_t > 0.01 {
                let prompt_color = gfx::Color {
                    r: style.prompt_color.r,
                    g: style.prompt_color.g,
                    b: style.prompt_color.b,
                    a: style.prompt_color.a * state.prompt_anim_t,
                };
                let prompt_rect = gfx::RectF::new(
                    adjusted_rect.x + style.padding.left,
                    adjusted_rect.y + style.padding.top,
                    adjusted_rect.w - style.padding.left - style.padding.right,
                    prompt_height,
                );
                encode_label_unwrapped(
                    prompt_text,
                    prompt_color,
                    Align::Center,
                    0,
                    style.prompt_font_px,
                    prompt_rect,
                    device_scale,
                    text_ctx,
                    uploader,
                    builder,
                );
            }
        }

        // Draw placeholder or text
        let text_y = adjusted_rect.y + style.padding.top + prompt_offset;
        let text_rect = gfx::RectF::new(
            adjusted_rect.x + style.padding.left,
            text_y,
            adjusted_rect.w - style.padding.left - style.padding.right,
            adjusted_rect.h - style.padding.top - style.padding.bottom - prompt_offset,
        );

        if state.text.is_empty() {
            encode_label_unwrapped(
                &self.placeholder,
                style.placeholder_color,
                Align::Center,
                0,
                style.font_px,
                text_rect,
                device_scale,
                text_ctx,
                uploader,
                builder,
            );
        } else {
            encode_label_unwrapped(
                &state.text,
                style.text_color,
                Align::Center,
                0,
                style.font_px,
                text_rect,
                device_scale,
                text_ctx,
                uploader,
                builder,
            );
        }
    }
}

// ----- SlidingSwitch -----

#[derive(Clone, Copy, Debug)]
pub struct SlidingSwitchStyle {
    pub background_color: gfx::Color,
    pub knob_color: gfx::Color,
    pub corner: f32,
    pub inactive_timeout_ms: u64,
}

impl Default for SlidingSwitchStyle {
    fn default() -> Self {
        Self {
            background_color: gfx::Color::rgba(0.96, 0.14, 0.35, 1.0), // Nametag red
            knob_color: gfx::Color::rgba(0.96, 0.14, 0.35, 1.0),
            corner: 0.0, // Will be set to height/2
            inactive_timeout_ms: 2000,
        }
    }
}

pub struct SlidingSwitch {
    pub style: SlidingSwitchStyle,
}

impl Default for SlidingSwitch {
    fn default() -> Self {
        Self { style: SlidingSwitchStyle::default() }
    }
}

const LEGACY_SLIDING_SWITCH_LONG_PRESS_MS: u64 = 300;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SlidingSwitchMode {
    Idle,
    Pressing,
    Dragging,
    Triggered,
}

pub struct SlidingSwitchState {
    pub mode: SlidingSwitchMode,
    knob_offset_x: f32,
    inactive_timer_ms: u64,
    last_interaction_ms: u64,
    press_started_ms: u64,
    inactive_armed: bool,
}

impl Default for SlidingSwitchState {
    fn default() -> Self {
        Self {
            mode: SlidingSwitchMode::Idle,
            knob_offset_x: 0.0,
            inactive_timer_ms: 0, // Will be set from style when needed
            last_interaction_ms: 0,
            press_started_ms: 0,
            inactive_armed: false,
        }
    }
}

#[inline]
fn sliding_switch_point_in_rect(point: [f32; 2], rect: gfx::RectF) -> bool {
    point[0] >= rect.x
        && point[0] <= rect.x + rect.w
        && point[1] >= rect.y
        && point[1] <= rect.y + rect.h
}

#[inline]
fn sliding_switch_knob_rect(bounds: gfx::RectF, knob_offset_x: f32) -> gfx::RectF {
    gfx::RectF::new(bounds.x + knob_offset_x, bounds.y, bounds.h, bounds.h)
}

impl SlidingSwitchState {
    /// Start the inactive timer
    pub fn start(&mut self, style: &SlidingSwitchStyle) {
        self.last_interaction_ms = timing::now_ms();
        self.inactive_timer_ms = style.inactive_timeout_ms;
        self.inactive_armed = self.inactive_timer_ms > 0;
    }

    /// Emit the one-shot inactive event after the legacy timeout elapses.
    pub fn take_inactive(&mut self) -> bool {
        if !self.inactive_armed {
            return false;
        }
        let elapsed = timing::now_ms().saturating_sub(self.last_interaction_ms);
        if elapsed < self.inactive_timer_ms {
            return false;
        }
        self.inactive_armed = false;
        true
    }

    /// Begin the legacy knob press. Motion is ignored until the long-press gate elapses.
    pub fn begin_drag(&mut self, point: [f32; 2], bounds: gfx::RectF) -> bool {
        if self.mode != SlidingSwitchMode::Idle {
            return false;
        }
        let knob_rect = sliding_switch_knob_rect(bounds, self.knob_offset_x);
        if !sliding_switch_point_in_rect(point, knob_rect) {
            return false;
        }
        let now = timing::now_ms();
        self.mode = SlidingSwitchMode::Pressing;
        self.press_started_ms = now;
        self.last_interaction_ms = now;
        true
    }

    /// Update the legacy press/drag state. Returns true when the control reaches the trigger edge.
    pub fn drag_to(&mut self, point: [f32; 2], bounds: gfx::RectF) -> bool {
        if !matches!(self.mode, SlidingSwitchMode::Pressing | SlidingSwitchMode::Dragging) {
            return false;
        }
        if !sliding_switch_point_in_rect(point, bounds) {
            self.reset();
            return false;
        }
        let now = timing::now_ms();
        self.last_interaction_ms = now;
        if self.mode == SlidingSwitchMode::Pressing
            && now.saturating_sub(self.press_started_ms) < LEGACY_SLIDING_SWITCH_LONG_PRESS_MS
        {
            return false;
        }
        self.mode = SlidingSwitchMode::Dragging;
        let max_offset = (bounds.w - bounds.h).max(0.0);
        debug_assert!(bounds.w >= bounds.h, "SlidingSwitch requires width >= height");
        if max_offset <= 0.0 {
            self.knob_offset_x = 0.0;
            return false;
        }
        let local_x = (point[0] - bounds.x).clamp(0.0, max_offset);
        self.knob_offset_x = local_x;
        if self.knob_offset_x >= max_offset {
            self.mode = SlidingSwitchMode::Triggered;
            return true;
        }
        false
    }

    /// End drag gesture
    pub fn end_drag(&mut self) {
        if matches!(self.mode, SlidingSwitchMode::Pressing | SlidingSwitchMode::Dragging) {
            self.reset();
        }
    }

    /// Reset to idle
    pub fn reset(&mut self) {
        self.mode = SlidingSwitchMode::Idle;
        self.knob_offset_x = 0.0;
        self.press_started_ms = 0;
    }

    /// Get current slide progress (0.0 to 1.0)
    pub fn progress(&self, bounds: gfx::RectF) -> f32 {
        let max_offset = bounds.w - bounds.h;
        if max_offset <= 0.0 {
            return 0.0;
        }
        (self.knob_offset_x / max_offset).clamp(0.0, 1.0)
    }
}

impl SlidingSwitch {
    pub fn encode(&self, rect: gfx::RectF, state: &SlidingSwitchState, b: &mut DrawListBuilder) {
        let corner = if self.style.corner > 0.0 {
            self.style.corner
        } else {
            rect.h * 0.5 // Default: circular ends
        };

        // Background (fades in based on progress)
        let progress = state.progress(rect);
        let mut bg_color = self.style.background_color;
        bg_color.a *= progress;
        b.rrect(rect, [corner; 4], bg_color);

        // Knob
        let knob_size = rect.h;
        let knob_x = rect.x + state.knob_offset_x;
        let knob_rect = gfx::RectF::new(knob_x, rect.y, knob_size, knob_size);
        b.rrect(knob_rect, [knob_size * 0.5; 4], self.style.knob_color);
    }
}
