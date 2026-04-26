use core::f32::consts::PI;
use oxide_renderer_api::RectF;
use oxide_utils::{
    clamp, compute_canvas, snap_point, snap_rect_edges, snap_scalar, stroke_width_points,
    CanvasMetrics, FitPolicy,
};
use proptest::prelude::*;

#[test]
fn compute_canvas_degenerate_inputs() {
    let metrics = compute_canvas(0.0, 200.0, 400.0, 300.0, 2.0, FitPolicy::Contain);
    assert_eq!(metrics.scale, 0.0);
    assert_eq!(metrics.offset_x, 0.0);
    assert_eq!(metrics.offset_y, 0.0);
    assert_eq!(metrics.view_rect().w, 0.0);
    assert_eq!(metrics.view_rect().h, 0.0);

    let zero_design = compute_canvas(100.0, 100.0, 0.0, -10.0, 1.0, FitPolicy::Cover);
    assert_eq!(zero_design.scale, 0.0);
    assert_eq!(zero_design.view_to_design(50.0, 50.0), (0.0, 0.0));
}

#[test]
fn compute_canvas_identity_letterbox_and_cover() {
    let identity = compute_canvas(390.0, 844.0, 390.0, 844.0, 3.0, FitPolicy::Contain);
    assert!((identity.scale - 1.0).abs() < 1e-6);
    assert_eq!(identity.offset_x, 0.0);
    assert_eq!(identity.offset_y, 0.0);
    assert_eq!(identity.device_scale, 3.0);
    let identity_rect = identity.view_rect();
    assert_eq!(identity_rect.w, 390.0);
    assert_eq!(identity_rect.h, 844.0);

    let letterbox = compute_canvas(100.0, 100.0, 200.0, 100.0, 2.0, FitPolicy::Contain);
    assert!((letterbox.scale - 0.5).abs() < 1e-6);
    assert_eq!(letterbox.offset_x, 0.0);
    assert_eq!(letterbox.offset_y, 25.0);
    let (vx, vy) = letterbox.design_to_view(100.0, 50.0);
    assert_eq!((vx, vy), (50.0, 50.0));
    let (dx, dy) = letterbox.view_to_design(vx, vy);
    assert!((dx - 100.0).abs() < 1e-6 && (dy - 50.0).abs() < 1e-6);

    let cover = compute_canvas(100.0, 50.0, 30.0, 30.0, 2.0, FitPolicy::Cover);
    assert!(cover.scale > 0.0);
    assert!((cover.design_w * cover.scale - 100.0).abs() < 1e-3);
}

#[test]
fn snapping_respects_device_scale() {
    let device_scale = 3.0;
    let raw = 12.4;
    let snapped = snap_scalar(raw, device_scale);
    assert!((snapped * device_scale).round() == snapped * device_scale);

    let (px, py) = snap_point(1.1, -2.2, device_scale);
    assert!(((px * device_scale).round() - px * device_scale).abs() < 1e-6);
    assert!(((py * device_scale).round() - py * device_scale).abs() < 1e-6);

    let rect = RectF::new(0.25, 1.5, 10.2, 6.7);
    let snapped_rect = snap_rect_edges(rect, device_scale);
    let right = snapped_rect.x + snapped_rect.w;
    let bottom = snapped_rect.y + snapped_rect.h;
    for edge in [snapped_rect.x, snapped_rect.y, right, bottom] {
        assert!(((edge * device_scale).round() - edge * device_scale).abs() < 1e-6);
    }

    assert_eq!(snap_scalar(PI, -1.0), PI);
    assert_eq!(snap_point(0.5, 0.5, 0.0), (0.5, 0.5));

    let ds = 3.0;
    let scalar = snap_scalar(10.3333, ds);
    assert!((scalar - (31.0 / 3.0)).abs() < 1e-6);

    let rect = RectF::new(0.3, 0.6, 2.2, 3.7);
    let snapped = snap_rect_edges(rect, ds);
    assert!((snapped.x - snap_scalar(0.3, ds)).abs() < 1e-6);
    assert!((snapped.y - snap_scalar(0.6, ds)).abs() < 1e-6);
    let x2 = snap_scalar(0.3 + 2.2, ds);
    let y2 = snap_scalar(0.6 + 3.7, ds);
    assert!((snapped.w - (x2 - snapped.x)).abs() < 1e-6);
    assert!((snapped.h - (y2 - snapped.y)).abs() < 1e-6);
}

#[test]
fn clamp_and_stroke_width_behaviour() {
    assert_eq!(clamp(-1.0, 0.0, 1.0), 0.0);
    assert_eq!(clamp(2.0, -1.0, 1.0), 1.0);
    assert_eq!(clamp(0.5, -1.0, 1.0), 0.5);

    assert_eq!(stroke_width_points(0.0, 5.0), 0.0);
    assert_eq!(stroke_width_points(0.0, 1.0), 0.0);
    assert_eq!(stroke_width_points(2.0, 0.0), 0.0);
    assert_eq!(stroke_width_points(4.0, 0.0), 0.0);
    let width = stroke_width_points(2.0, 0.5);
    assert!((width - 1.0).abs() < 1e-6);
    let scaled = stroke_width_points(3.0, 0.5);
    assert!((scaled - (1.0 / 1.5)).abs() < 1e-6);
}

fn approx_eq(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-4
}

fn canvas_ok(metrics: &CanvasMetrics, _view_w: f32, _view_h: f32) {
    assert!(metrics.scale.is_finite());
    assert!(metrics.offset_x.is_finite());
    assert!(metrics.offset_y.is_finite());
    let rect = metrics.view_rect();
    assert!(rect.w >= 0.0 && rect.h >= 0.0);
    assert!(rect.x.is_finite() && rect.y.is_finite());
}

proptest! {
   #[test]
   fn compute_canvas_scales(view_w in 1f32..2000f32,
                            view_h in 1f32..2000f32,
                            design_w in 1f32..2000f32,
                            design_h in 1f32..2000f32,
                            device_scale in 0.5f32..5.0f32) {
      let contain = compute_canvas(view_w, view_h, design_w, design_h, device_scale, FitPolicy::Contain);
      canvas_ok(&contain, view_w, view_h);
      let cover = compute_canvas(view_w, view_h, design_w, design_h, device_scale, FitPolicy::Cover);
      canvas_ok(&cover, view_w, view_h);

      let sx = view_w / design_w;
      let sy = view_h / design_h;
      assert!(approx_eq(contain.scale, sx.min(sy)));
      assert!(approx_eq(cover.scale, sx.max(sy)));
   }
}
