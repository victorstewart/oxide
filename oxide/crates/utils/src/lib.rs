//! `Oxide` utilities: canvas math, pixel snapping, and helpers.
#![allow(clippy::all, clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::cast_precision_loss
)]

use oxide_renderer_api as gfx;

/// How to fit the design canvas inside the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitPolicy {
    /// Uniform scale to fit within bounds; letterbox as needed.
    Contain,
    /// Uniform scale to cover bounds; content may bleed/clipped.
    Cover,
}

/// Canonical canvas transform and metrics derived from §26.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasMetrics {
    pub design_w: f32,
    pub design_h: f32,
    /// Scale applied to design space to get view space.
    pub scale: f32,
    /// Offset in view points where the design canvas is placed.
    pub offset_x: f32,
    pub offset_y: f32,
    /// Device pixel scale (`UIScreen` scale on iOS).
    pub device_scale: f32,
}

impl CanvasMetrics {
    /// Convert a position in design points to view points.
    #[inline]
    #[must_use]
    pub fn design_to_view(&self, x: f32, y: f32) -> (f32, f32) {
        (self.offset_x + x * self.scale, self.offset_y + y * self.scale)
    }

    /// Convert a position in view points to design points.
    #[inline]
    #[must_use]
    pub fn view_to_design(&self, vx: f32, vy: f32) -> (f32, f32) {
        if self.scale <= 0.0 {
            (0.0, 0.0)
        } else {
            ((vx - self.offset_x) / self.scale, (vy - self.offset_y) / self.scale)
        }
    }

    /// Compute a view-rect in points for the canonical canvas placement.
    #[inline]
    #[must_use]
    pub fn view_rect(&self) -> gfx::RectF {
        if self.scale <= 0.0 {
            gfx::RectF::new(self.offset_x, self.offset_y, 0.0, 0.0)
        } else {
            gfx::RectF::new(
                self.offset_x,
                self.offset_y,
                self.design_w * self.scale,
                self.design_h * self.scale,
            )
        }
    }
}

/// Compute canonical canvas transform based on §26.
///
/// When either the view or design dimensions are non-positive, a zero-scale
/// metrics record anchored at the origin is returned so callers can branch
/// without triggering division-by-zero.
#[must_use]
pub fn compute_canvas(
    view_w: f32,
    view_h: f32,
    design_w: f32,
    design_h: f32,
    device_scale: f32,
    fit: FitPolicy,
) -> CanvasMetrics {
    if view_w <= 0.0 || view_h <= 0.0 || design_w <= 0.0 || design_h <= 0.0 {
        return CanvasMetrics {
            design_w,
            design_h,
            scale: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
            device_scale,
        };
    }
    let sx = view_w / design_w;
    let sy = view_h / design_h;
    let s = match fit {
        FitPolicy::Contain => sx.min(sy),
        FitPolicy::Cover => sx.max(sy),
    };
    let ox = ((view_w - design_w * s) * 0.5).floor();
    let oy = ((view_h - design_h * s) * 0.5).floor();
    CanvasMetrics { design_w, design_h, scale: s, offset_x: ox, offset_y: oy, device_scale }
}

/// Pixel snapping for hairlines and glyph origins after transform:
/// `snap(v)` = round(v * `device_scale`) / `device_scale`
#[inline]
#[must_use]
pub fn snap_scalar(v: f32, device_scale: f32) -> f32 {
    if device_scale <= 0.0 {
        return v;
    }
    (v * device_scale).round() / device_scale
}

/// Snap a point in view space to device pixels.
#[inline]
#[must_use]
pub fn snap_point(x: f32, y: f32, device_scale: f32) -> (f32, f32) {
    (snap_scalar(x, device_scale), snap_scalar(y, device_scale))
}

/// Do not snap filled interior vertices; only edges/glyph origins. This helper
/// snaps a rect's edges; callers decide when to use it.
#[must_use]
pub fn snap_rect_edges(mut r: gfx::RectF, device_scale: f32) -> gfx::RectF {
    let x2 = r.x + r.w;
    let y2 = r.y + r.h;
    let x1s = snap_scalar(r.x, device_scale);
    let y1s = snap_scalar(r.y, device_scale);
    let x2s = snap_scalar(x2, device_scale);
    let y2s = snap_scalar(y2, device_scale);
    r.x = x1s;
    r.y = y1s;
    r.w = x2s - x1s;
    r.h = y2s - y1s;
    r
}

/// Stroke widths are specified in physical pixels; convert each frame:
/// `stroke_pt` = 1 / (`device_scale` * `canvas_scale`)
#[inline]
#[must_use]
pub fn stroke_width_points(device_scale: f32, canvas_scale: f32) -> f32 {
    if device_scale <= 0.0 || canvas_scale <= 0.0 {
        return 0.0;
    }
    1.0 / (device_scale * canvas_scale)
}

/// Clamp helper for floats.
#[inline]
#[must_use]
pub fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    v.max(lo).min(hi)
}

pub mod prelude {
    pub use super::{
        clamp, compute_canvas, snap_point, snap_rect_edges, snap_scalar, stroke_width_points,
        CanvasMetrics, FitPolicy,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_identity() {
        let m = compute_canvas(390.0, 844.0, 390.0, 844.0, 3.0, FitPolicy::Contain);
        assert!((m.scale - 1.0).abs() < 1e-6);
        assert_eq!(m.offset_x, 0.0);
        assert_eq!(m.offset_y, 0.0);
        assert_eq!(m.device_scale, 3.0);
        let vr = m.view_rect();
        assert_eq!(vr.w, 390.0);
        assert_eq!(vr.h, 844.0);
    }

    #[test]
    fn canvas_letterbox() {
        // Square view, wide design
        let m = compute_canvas(100.0, 100.0, 200.0, 100.0, 2.0, FitPolicy::Contain);
        assert!((m.scale - 0.5).abs() < 1e-6);
        assert_eq!(m.offset_x, 0.0);
        assert_eq!(m.offset_y, 25.0);
        let (vx, vy) = m.design_to_view(100.0, 50.0);
        assert_eq!((vx, vy), (50.0, 50.0));
        let (dx, dy) = m.view_to_design(vx, vy);
        assert!((dx - 100.0).abs() < 1e-6 && (dy - 50.0).abs() < 1e-6);
    }

    #[test]
    fn canvas_cover() {
        // Cover should scale to fill and bleed
        let m = compute_canvas(100.0, 50.0, 30.0, 30.0, 2.0, FitPolicy::Cover);
        assert!(m.scale > 0.0);
        // Since view is wider than tall relative to design, cover matches width
        assert!((m.design_w * m.scale - 100.0).abs() < 1e-3);
    }

    #[test]
    fn stroke_width_zero_safe() {
        assert_eq!(stroke_width_points(0.0, 1.0), 0.0);
        assert_eq!(stroke_width_points(2.0, 0.0), 0.0);
    }

    #[test]
    fn snapping_scalar_point_rect() {
        let ds = 3.0;
        let v = 10.3333;
        let s = snap_scalar(v, ds);
        assert!((s - (31.0 / 3.0)).abs() < 1e-6);

        let (x, y) = snap_point(1.2, 2.7, ds);
        assert!((x - (1.2 * ds).round() / ds).abs() < 1e-6);
        assert!((y - (2.7 * ds).round() / ds).abs() < 1e-6);

        let r = gfx::RectF::new(0.3, 0.6, 2.2, 3.7);
        let sr = snap_rect_edges(r, ds);
        // Edges are snapped; widths adjusted accordingly.
        assert!((sr.x - snap_scalar(0.3, ds)).abs() < 1e-6);
        assert!((sr.y - snap_scalar(0.6, ds)).abs() < 1e-6);
        let x2 = snap_scalar(0.3 + 2.2, ds);
        let y2 = snap_scalar(0.6 + 3.7, ds);
        assert!((sr.w - (x2 - sr.x)).abs() < 1e-6);
        assert!((sr.h - (y2 - sr.y)).abs() < 1e-6);
    }

    #[test]
    fn stroke_width() {
        let w = stroke_width_points(3.0, 0.5);
        assert!((w - (1.0 / 1.5)).abs() < 1e-6);
    }
}
