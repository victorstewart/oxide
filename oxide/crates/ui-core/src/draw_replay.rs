//! Draw-list replay helpers for CPU-side composition paths.
//!
//! These helpers are renderer-agnostic and can be shared by host/app crates
//! that need to replay an `oxide_renderer_api::DrawList` through a
//! `RenderEncoder`.

use alloc::vec::Vec;
use oxide_renderer_api::{
   DrawCmd, DrawList, RectF, RectI, RenderChunk, RenderEncoder, Vertex, VertexSpan,
};

/// Converts a floating viewport rectangle into an integer clip rectangle.
#[must_use]
pub fn viewport_clip(rect: RectF) -> RectI {
    RectI::new(
        rect.x.floor() as i32,
        rect.y.floor() as i32,
        rect.w.ceil() as i32,
        rect.h.ceil() as i32,
    )
}

/// Replays draw commands while applying a fixed origin translation.
///
/// The caller supplies a `fallback_clip` that is restored whenever the clip
/// stack unwinds to empty.
pub fn replay_drawlist(
    list: &DrawList,
    encoder: &mut dyn RenderEncoder,
    fallback_clip: RectI,
    origin: [f32; 2],
) {
    replay_drawlist_impl(list, encoder, fallback_clip, origin, false);
}

/// Replays an immutable render chunk whose command spans and local indices were
/// validated once when the chunk was created.
pub fn replay_render_chunk(chunk: &RenderChunk, encoder: &mut dyn RenderEncoder, fallback_clip: RectI, origin: [f32; 2])
{
   replay_drawlist_impl(chunk.draw_list(), encoder, fallback_clip, origin, true);
}

fn replay_drawlist_impl(list: &DrawList, encoder: &mut dyn RenderEncoder, fallback_clip: RectI, origin: [f32; 2], canonical_indices: bool)
{
    let offset_x = origin[0];
    let offset_y = origin[1];
    let offset_ix = offset_x.round() as i32;
    let offset_iy = offset_y.round() as i32;
    let translated_fallback = translate_clip(fallback_clip, offset_ix, offset_iy);
    encoder.set_clip(translated_fallback);

    let mut clip_stack = Vec::new();
    for cmd in &list.items {
        match cmd {
            DrawCmd::LayerBegin { .. } | DrawCmd::LayerEnd => {}
            DrawCmd::Solid { vb, color, .. } => {
                let Some(slice) = slice_vertices(list, *vb) else {
                    continue;
                };
                let translated = translate_vertices(slice, offset_x, offset_y);
                encoder.draw_solid(&translated, *color);
            }
            DrawCmd::Image { tex, dst, src, .. } => {
                encoder.draw_image(*tex, translate_rect(*dst, offset_x, offset_y), *src);
            }
            DrawCmd::ImageMesh { tex, vb, ib, alpha } => {
                let Some(vertices) = slice_vertices(list, *vb) else {
                    continue;
                };
                let translated = translate_vertices(vertices, offset_x, offset_y);
                let indices = slice_indices(list, *ib).unwrap_or(&[]);
                if canonical_indices {
                    encoder.draw_image_mesh(*tex, &translated, indices, *alpha);
                } else {
                    let Some(normalized_indices) =
                        normalize_indices_for_vertex_span(indices, vb.offset, vb.len)
                    else {
                        continue;
                    };
                    encoder.draw_image_mesh(*tex, &translated, &normalized_indices, *alpha);
                }
            }
            DrawCmd::GlyphRun { run } => {
                let vertices = slice_vertices(list, run.vb).unwrap_or(&[]);
                let translated = translate_vertices(vertices, offset_x, offset_y);
                let indices = slice_indices(list, run.ib).unwrap_or(&[]);
                encoder.draw_glyph_run_resolved(run, &translated, indices);
            }
            DrawCmd::RRect { rect, radii, color } => {
                encoder.draw_rrect(translate_rect(*rect, offset_x, offset_y), *radii, *color);
            }
            DrawCmd::NineSlice { tex, rect, slice, alpha } => encoder.draw_nine_slice(
                *tex,
                translate_rect(*rect, offset_x, offset_y),
                *slice,
                *alpha,
            ),
            DrawCmd::Backdrop { rect, sigma, tint, alpha } => {
                encoder.draw_backdrop(
                    translate_rect(*rect, offset_x, offset_y),
                    *sigma,
                    *tint,
                    *alpha,
                );
            }
            DrawCmd::VisualEffect { rect, effect } => {
                encoder.draw_visual_effect(translate_rect(*rect, offset_x, offset_y), *effect);
            }
            DrawCmd::Spinner { center, atom, alpha } => {
                encoder.draw_spinner([center[0] + offset_x, center[1] + offset_y], *atom, *alpha)
            }
            DrawCmd::CameraBg { rect, sigma, tint, alpha, grayscale, blur } => encoder
                .draw_camera_bg(
                    translate_rect(*rect, offset_x, offset_y),
                    *tint,
                    *alpha,
                    *grayscale,
                    *blur,
                    *sigma,
                ),
            DrawCmd::ClipPush { rect } => {
                let translated = translate_clip(*rect, offset_ix, offset_iy);
                clip_stack.push(translated);
                encoder.set_clip(translated);
            }
            DrawCmd::ClipPop => {
                clip_stack.pop();
                let restored = clip_stack.last().copied().unwrap_or(translated_fallback);
                encoder.set_clip(restored);
            }
        }
    }
    if !clip_stack.is_empty() {
        encoder.set_clip(translated_fallback);
    }
}

fn slice_vertices(list: &DrawList, span: VertexSpan) -> Option<&[Vertex]> {
    let start = span.offset as usize;
    let len = span.len as usize;
    let end = start.checked_add(len)?;
    list.vertices.get(start..end)
}

fn slice_indices(list: &DrawList, span: oxide_renderer_api::IndexSpan) -> Option<&[u16]> {
    let start = span.offset as usize;
    let len = span.len as usize;
    let end = start.checked_add(len)?;
    list.indices.get(start..end)
}

fn normalize_indices_for_vertex_span(
    indices: &[u16],
    vertex_base: u32,
    vertex_count: u32,
) -> Option<Vec<u16>> {
    if indices.is_empty() {
        return Some(Vec::new());
    }
    if vertex_count == 0 {
        return None;
    }
    if vertex_count <= u16::MAX as u32 {
        let local_limit = vertex_count as u16;
        if indices.iter().all(|index| *index < local_limit) {
            return Some(indices.to_vec());
        }
    }

    let vertex_end = vertex_base.saturating_add(vertex_count);
    let mut normalized = Vec::with_capacity(indices.len());
    for index in indices.iter().copied() {
        let absolute = index as u32;
        if absolute < vertex_base || absolute >= vertex_end {
            return None;
        }
        normalized.push((absolute - vertex_base) as u16);
    }
    Some(normalized)
}

fn translate_rect(rect: RectF, dx: f32, dy: f32) -> RectF {
    RectF::new(rect.x + dx, rect.y + dy, rect.w, rect.h)
}

fn translate_clip(rect: RectI, dx: i32, dy: i32) -> RectI {
    RectI::new(rect.x + dx, rect.y + dy, rect.w, rect.h)
}

fn translate_vertices(vertices: &[Vertex], dx: f32, dy: f32) -> Vec<Vertex> {
    let mut translated = Vec::with_capacity(vertices.len());
    for vertex in vertices {
        translated.push(Vertex {
            x: vertex.x + dx,
            y: vertex.y + dy,
            u: vertex.u,
            v: vertex.v,
            rgba: vertex.rgba,
        });
    }
    translated
}
