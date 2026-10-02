use oxide_renderer_api::{Color, DrawCmd, RectF};
use oxide_ui_core::elements::ProgressBar;
use oxide_ui_core::DrawListBuilder;

#[test]
fn fractional_progress_retains_subpixel_fill_and_configured_colors()
{
   let track = Color::from_srgba(0.85, 0.85, 0.85, 1.0);
   let fill = Color::from_srgba(0.2, 0.5, 1.0, 1.0);
   let bar = ProgressBar {value: Some(0.001), track, fill, corner: 2.0};
   let mut builder = DrawListBuilder::new();
   bar.encode(RectF::new(12.0, 24.0, 82.0, 4.0), 0.0, &mut builder);
   let [DrawCmd::RRect {rect: track_rect, color: actual_track, ..}, DrawCmd::RRect {rect, color, radii}] = builder.drawlist().items.as_slice() else {panic!("expected track and positive fill");};
   assert_eq!(*track_rect, RectF::new(12.0, 24.0, 82.0, 4.0));
   assert_eq!(*actual_track, track);
   assert_eq!(*color, fill);
   assert!((rect.w - 0.082).abs() < 0.0001);
   assert_eq!((rect.x, rect.y, rect.h), (12.0, 24.0, 4.0));
   assert!(radii.iter().all(|radius| *radius <= rect.w * 0.5));
}

#[test]
fn indeterminate_progress_stays_inside_a_narrow_track()
{
   for phase in [-0.25, 0.0, 0.5, 1.0]
   {
      let mut builder = DrawListBuilder::new();
      ProgressBar {value: None, ..ProgressBar::default()}.encode(RectF::new(12.0, 24.0, 4.0, 4.0), phase, &mut builder);
      for item in &builder.drawlist().items
      {
         let DrawCmd::RRect {rect, ..} = item else {panic!("expected rounded rect");};
         assert!(rect.x >= 12.0 && rect.x + rect.w <= 16.0);
      }
   }
}

#[test]
fn zero_and_clamped_progress_respect_track_bounds()
{
   for (value, expected_items) in [(-1.0, 1), (0.0, 1), (1.0, 2), (2.0, 2)]
   {
      let mut builder = DrawListBuilder::new();
      let bounds = RectF::new(12.0, 24.0, 82.0, 4.0);
      ProgressBar {value: Some(value), ..ProgressBar::default()}.encode(bounds, 0.0, &mut builder);
      assert_eq!(builder.drawlist().items.len(), expected_items);
      if expected_items == 2
      {
         let DrawCmd::RRect {rect, ..} = builder.drawlist().items[1] else {panic!("expected fill");};
         assert_eq!(rect, bounds);
      }
   }
   let mut builder = DrawListBuilder::new();
   ProgressBar::default().encode(RectF::new(12.0, 24.0, 0.0, 4.0), 0.0, &mut builder);
   assert!(builder.drawlist().items.is_empty());
}
