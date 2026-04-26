use oxide_ui_core::design_system::{GeometricScale, ScreenScale};

#[test]
fn screen_scale_proportional() {
   let scale = ScreenScale::new(320.0, 414.0);
   let font_size = scale.scale(18.0);
   assert!((font_size - 13.91).abs() < 0.1);
}

#[test]
fn geometric_scale_progression_all_and_clamping() {
   let scale = GeometricScale::new(10.0, 1.5, 5);
   assert!((scale.at(0) - 10.0).abs() < 0.01);
   assert!((scale.at(1) - 15.0).abs() < 0.01);
   assert!((scale.at(2) - 22.5).abs() < 0.01);
   assert!((scale.at(3) - 33.75).abs() < 0.01);

   let doubled = GeometricScale::new(10.0, 2.0, 4);
   let sizes = doubled.all();
   assert_eq!(sizes.len(), 4);
   assert!((sizes[0] - 10.0).abs() < 0.01);
   assert!((sizes[1] - 20.0).abs() < 0.01);
   assert!((sizes[2] - 40.0).abs() < 0.01);
   assert!((sizes[3] - 80.0).abs() < 0.01);

   let clamped = GeometricScale::new(10.0, 2.0, 3);
   assert_eq!(clamped.at(5), clamped.at(2));
}

#[test]
#[should_panic(expected = "screen_width must be positive")]
fn screen_scale_invalid_width() {
   ScreenScale::new(0.0, 414.0);
}

#[test]
#[should_panic(expected = "reference_width must be positive")]
fn screen_scale_invalid_reference() {
   ScreenScale::new(320.0, 0.0);
}

#[test]
#[should_panic(expected = "base must be positive")]
fn geometric_scale_invalid_base() {
   GeometricScale::new(0.0, 1.5, 5);
}

#[test]
#[should_panic(expected = "ratio must be positive")]
fn geometric_scale_invalid_ratio() {
   GeometricScale::new(10.0, -1.5, 5);
}

#[test]
#[should_panic(expected = "count must be at least 1")]
fn geometric_scale_invalid_count() {
   GeometricScale::new(10.0, 1.5, 0);
}
