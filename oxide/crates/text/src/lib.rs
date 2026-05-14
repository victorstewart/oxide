//! `Oxide` text system: shaping, atlas packing, and quad generation.
#![allow(clippy::all, clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::cast_precision_loss
)]

use oxide_renderer_api as api;
use rustybuzz::{Face as RbFace, GlyphBuffer as RbGlyphs, UnicodeBuffer};
use std::collections::HashMap;
use swash::scale::{image::Image, ScaleContext};
#[allow(unused_imports)]
use swash::scale::{Render, Source};

pub struct Font {
    data: std::sync::Arc<Vec<u8>>,
}

impl Font {
    #[must_use]
    pub fn from_bytes(data: Vec<u8>) -> Self {
        Self { data: std::sync::Arc::new(data) }
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
        self.data.fill(0);
        self.next_x = 1;
        self.row_y = 1;
        self.row_h = 0;
        self.map.clear();
        self.mark_dirty(0, 0, self.width, self.height);
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
        Ok(ShapeOutput { font, font_id, glyphs, px })
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
    x_advance: i32,
    y_advance: i32,
}

#[derive(Clone, Debug)]
pub struct OwnedShape {
    font_id: usize,
    glyphs: Vec<ShapedGlyph>,
    px: f32,
    width: f32,
}

impl OwnedShape {
    #[inline]
    pub fn width(&self) -> f32 {
        self.width
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
        bake_glyphs_into(
            font,
            self.font_id,
            self.px,
            self.glyphs.iter().copied(),
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

    pub fn to_owned_shape(&self) -> OwnedShape {
        let infos = self.glyphs.glyph_infos();
        let poss = self.glyphs.glyph_positions();
        let mut glyphs = Vec::with_capacity(infos.len());
        let mut width = 0.0_f32;
        for (info, pos) in infos.iter().zip(poss.iter()) {
            width += pos.x_advance as f32 / 64.0;
            glyphs.push(ShapedGlyph {
                glyph_id: (info.glyph_id as u32) as u16,
                x_advance: pos.x_advance,
                y_advance: pos.y_advance,
            });
        }
        OwnedShape { font_id: self.font_id, glyphs, px: self.px, width }
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
        let infos = self.glyphs.glyph_infos();
        let poss = self.glyphs.glyph_positions();
        let glyphs = infos.iter().zip(poss.iter()).map(|(info, pos)| ShapedGlyph {
            glyph_id: (info.glyph_id as u32) as u16,
            x_advance: pos.x_advance,
            y_advance: pos.y_advance,
        });
        bake_glyphs_into(
            self.font,
            self.font_id,
            self.px,
            glyphs,
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

fn bake_glyphs_into<I>(
    font: &Font,
    font_id: usize,
    px: f32,
    glyphs: I,
    raster: &mut RasterCtx,
    atlas: &mut Atlas,
    draw_vertices: &mut Vec<api::Vertex>,
    draw_indices: &mut Vec<u16>,
    color: api::Color,
    atlas_handle: api::ImageHandle,
    origin_x: f32,
    origin_y: f32,
    device_scale: f32,
) -> api::GlyphRun
where
    I: IntoIterator<Item = ShapedGlyph>,
{
    let mut pen_x: f32 = 0.0;
    let mut pen_y: f32 = 0.0;
    let scale = if device_scale > 0.0 { device_scale } else { 1.0 };
    let ox = (origin_x * scale).round() / scale;
    let oy = (origin_y * scale).round() / scale;

    let v_start = draw_vertices.len() as u32;
    let i_start = draw_indices.len() as u32;
    let use_sdf = (px * device_scale) >= 24.0;
    let mut scaler = None;
    let render = Render::new(&[Source::Outline]);

    for glyph in glyphs {
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
            if !render.render_into(scaler, glyph.glyph_id, &mut img) {
                pen_x += glyph.x_advance as f32 / 64.0;
                pen_y += glyph.y_advance as f32 / 64.0;
                continue;
            }
            let w = img.placement.width.max(0);
            let h = img.placement.height.max(0);
            if w == 0 || h == 0 {
                pen_x += glyph.x_advance as f32 / 64.0;
                pen_y += glyph.y_advance as f32 / 64.0;
                continue;
            }
            let (aw, ah) = (w as u32, h as u32);
            let (ax, ay) = match atlas.alloc_rect(aw, ah) {
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
        vb: api::VertexSpan { offset: v_start, len: v_end - v_start },
        ib: api::IndexSpan { offset: i_start, len: i_end - i_start },
        sdf: use_sdf,
        color,
    }
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
