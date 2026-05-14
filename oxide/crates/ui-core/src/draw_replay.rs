//! Draw-list replay helpers for CPU-side composition paths.
//!
//! These helpers are renderer-agnostic and can be shared by host/app crates
//! that need to replay an `oxide_renderer_api::DrawList` through a
//! `RenderEncoder`.

use alloc::vec::Vec;
use oxide_renderer_api::{DrawCmd, DrawList, RectF, RectI, RenderEncoder, Vertex, VertexSpan};

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
            DrawCmd::GlyphRun { run } => {
                let vertices = slice_vertices(list, run.vb).unwrap_or(&[]);
                let indices = slice_indices(list, run.ib).unwrap_or(&[]);
                encoder.draw_glyph_run_resolved(run, vertices, indices);
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
