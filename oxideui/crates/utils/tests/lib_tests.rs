use oxideui_renderer_api::RectF;
use oxideui_utils::{
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

    assert_eq!(snap_scalar(3.14, -1.0), 3.14);
    assert_eq!(snap_point(0.5, 0.5, 0.0), (0.5, 0.5));
}

#[test]
fn clamp_and_stroke_width_behaviour() {
    assert_eq!(clamp(-1.0, 0.0, 1.0), 0.0);
    assert_eq!(clamp(2.0, -1.0, 1.0), 1.0);
    assert_eq!(clamp(0.5, -1.0, 1.0), 0.5);

    assert_eq!(stroke_width_points(0.0, 5.0), 0.0);
    assert_eq!(stroke_width_points(4.0, 0.0), 0.0);
    let width = stroke_width_points(2.0, 0.5);
    assert!((width - 1.0).abs() < 1e-6);
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
