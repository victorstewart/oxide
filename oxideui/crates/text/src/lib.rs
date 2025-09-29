//! `OxideUI` text system: shaping, atlas packing, and quad generation.
#![allow(clippy::all, clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::cast_precision_loss
)]

use oxideui_renderer_api as api;
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

pub struct Atlas {
    width: u32,
    height: u32,
    data: Vec<u8>, // A8 coverage
    next_x: u32,
    row_y: u32,
    row_h: u32,
    map: HashMap<GlyphKey, GlyphAtlasEntry>,
    clock: u64,
    max_size: u32,
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
            max_size: 2048,
        }
    }

    pub fn image(&self) -> (&[u8], u32, u32) {
        (&self.data, self.width, self.height)
    }

    pub fn reset(&mut self) {
        self.data.fill(0);
        self.next_x = 1;
        self.row_y = 1;
        self.row_h = 0;
        self.map.clear();
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

    fn grow_or_reset(&mut self) {
        // Try to grow atlas up to max_size; otherwise reset (evict all)
        let mut new_w = self.width;
        let mut new_h = self.height;
        if self.width < self.max_size {
            new_w = (self.width * 2).min(self.max_size);
        } else if self.height < self.max_size {
            new_h = (self.height * 2).min(self.max_size);
        } else {
            // At max; reset without growth (evict all)
        }
        self.width = new_w;
        self.height = new_h;
        self.data.clear();
        self.data.resize((self.width * self.height) as usize, 0);
        self.next_x = 1;
        self.row_y = 1;
        self.row_h = 0;
        self.map.clear();
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
        // Pen positioning using rustybuzz outputs (26.6 fixed-point advances)
        let mut pen_x: f32 = 0.0;
        let mut pen_y: f32 = 0.0;
        let scale = if device_scale > 0.0 { device_scale } else { 1.0 };
        // Snap origin to device pixel grid
        let ox = (origin_x * scale).round() / scale;
        let oy = (origin_y * scale).round() / scale;

        // Prepare swash scaler and renderer for alpha mask outlines
        let fontref = swash::FontRef::from_index(&self.font.data, 0).expect("swash font");
        let mut scx = ScaleContext::new();
        let mut scaler = scx.builder(fontref).size(self.px).hint(true).build();
        let render = Render::new(&[Source::Outline]);

        let v_start = draw_vertices.len() as u32;
        let i_start = draw_indices.len() as u32;
        let (atlas_w, atlas_h) = (atlas.width, atlas.height);

        let infos = self.glyphs.glyph_infos();
        let poss = self.glyphs.glyph_positions();
        for (info, pos) in infos.iter().zip(poss.iter()) {
            let gid_u16 = (info.glyph_id as u32) as u16;
            // Get or rasterize into atlas
            let use_sdf = (self.px * device_scale) >= 24.0;
            let key = GlyphKey {
                font: self.font_id,
                gid: gid_u16,
                px: self.px.round() as u16,
                sdf: use_sdf,
            };
            let entry = if let Some(e) = atlas.map.get_mut(&key) {
                e.last_used = atlas.clock.wrapping_add(1);
                atlas.clock = e.last_used;
                e.clone()
            } else {
                let mut img = Image::new();
                if !render.render_into(&mut scaler, gid_u16, &mut img) {
                    // Advance even if missing; skip draw
                    pen_x += pos.x_advance as f32 / 64.0;
                    pen_y += pos.y_advance as f32 / 64.0;
                    continue;
                }
                // Allocate in atlas
                let w = img.placement.width.max(0);
                let h = img.placement.height.max(0);
                if w == 0 || h == 0 {
                    pen_x += pos.x_advance as f32 / 64.0;
                    pen_y += pos.y_advance as f32 / 64.0;
                    continue;
                }
                let (aw, ah) = (w as u32, h as u32);
                let (ax, ay) = match atlas.alloc_rect(aw, ah) {
                    Some(rc) => rc,
                    None => {
                        atlas.grow_or_reset();
                        match atlas.alloc_rect(aw, ah) {
                            Some(rc2) => rc2,
                            None => {
                                // Still no space: skip glyph
                                pen_x += pos.x_advance as f32 / 64.0;
                                pen_y += pos.y_advance as f32 / 64.0;
                                continue;
                            }
                        }
                    }
                };
                // Blit into atlas: either coverage (A8) or SDF(A8)
                if use_sdf {
                    // Compute simple SDF around the binary coverage mask using local window
                    let mut sdf_row = vec![0u8; aw as usize];
                    let spread: i32 = 8;
                    for yy in 0..ah as i32 {
                        for xx in 0..aw as i32 {
                            let idx = (yy as usize) * (aw as usize) + (xx as usize);
                            let a = img.data[idx] as f32 / 255.0;
                            let inside = a > 0.5;
                            let mut min_d2 = ((spread + 1) * (spread + 1)) as i32;
                            // local search window
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

            // Compute glyph quad in device-independent coords (dp)
            let gx = ox + pen_x + (entry.l as f32);
            let gy = oy + pen_y - (entry.t as f32);
            let gw = entry.w as f32;
            let gh = entry.h as f32;

            // UVs in atlas normalized 0..1
            let u0 = (entry.u as f32) / (atlas_w as f32);
            let v0 = (entry.v as f32) / (atlas_h as f32);
            let u1 = (entry.u as f32 + entry.w as f32) / (atlas_w as f32);
            let v1 = (entry.v as f32 + entry.h as f32) / (atlas_h as f32);
            let rgba = pack_rgba(color);

            // Four vertices per glyph (indexed quad)
            let base = draw_vertices.len() as u32;
            push_v(draw_vertices, gx, gy, u0, v0, rgba); // 0
            push_v(draw_vertices, gx + gw, gy, u1, v0, rgba); // 1
            push_v(draw_vertices, gx, gy + gh, u0, v1, rgba); // 2
            push_v(draw_vertices, gx + gw, gy + gh, u1, v1, rgba); // 3
                                                                   // Indices: (0,1,2), (2,1,3)
            push_i(draw_indices, base + 0, base + 1, base + 2);
            push_i(draw_indices, base + 2, base + 1, base + 3);

            // Advance pen
            pen_x += pos.x_advance as f32 / 64.0;
            pen_y += pos.y_advance as f32 / 64.0;
        }

        let v_end = draw_vertices.len() as u32;
        let i_end = draw_indices.len() as u32;
        api::GlyphRun {
            atlas: atlas_handle,
            vb: api::VertexSpan { offset: v_start, len: v_end - v_start },
            ib: api::IndexSpan { offset: i_start, len: i_end - i_start },
            sdf: (self.px * device_scale) >= 24.0,
            color,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_pack_and_reset() {
        let mut atlas = Atlas::new(8, 8);
        let a = atlas.alloc_rect(2, 2).expect("alloc a");
        let b = atlas.alloc_rect(2, 2).expect("alloc b");
        let c = atlas.alloc_rect(2, 2).expect("alloc c");
        assert!(a.0 < b.0 || a.1 < b.1);
        assert!(c.1 > a.1);
        assert_eq!(atlas.alloc_rect(8, 8), None);
        atlas.grow_or_reset();
        let a2 = atlas.alloc_rect(2, 2).expect("alloc after grow");
        assert_eq!(a2, (1, 1));
        let (data, w, h) = atlas.image();
        assert_eq!(data.len(), (w * h) as usize);
    }
}
