use oxide_platform_api::{PointerDevice, TouchEvent, TouchId, TouchPhase};
use oxide_ui_core::VerticalScrollSurface;

const MS: u64 = 1_000_000;

fn touch(id: u64, phase: TouchPhase, y: f32, timestamp_ns: u64) -> TouchEvent
{
   TouchEvent {
      id: TouchId(id),
      phase,
      timestamp_ns,
      x: 195.0,
      y,
      pressure: None,
      tilt: None,
      device: PointerDevice::Finger,
   }
}

fn begin_forward_fling(surface: &mut VerticalScrollSurface, start_ns: u64)
{
   assert!(!surface.input_touch(&touch(1, TouchPhase::Start, 700.0, start_ns)));
   assert!(surface.input_touch(&touch(1, TouchPhase::Move, 650.0, start_ns + 10 * MS)));
   assert!(surface.input_touch(&touch(1, TouchPhase::Move, 600.0, start_ns + 20 * MS)));
   assert!(surface.input_touch(&touch(1, TouchPhase::End, 550.0, start_ns + 30 * MS)));
   assert!(surface.wants_next_frame());
}

fn settle_with_steps(surface: &mut VerticalScrollSurface, start_ns: u64, steps_ns: &[u64]) -> f32
{
   let mut now_ns = start_ns;
   let mut step = 0usize;
   while surface.wants_next_frame() && step < 4_096
   {
      now_ns = now_ns.saturating_add(steps_ns[step % steps_ns.len()]);
      let _ = surface.advance_to(now_ns);
      step += 1;
   }
   assert!(step < 4_096, "inertia did not settle");
   assert!(surface.is_settled());
   surface.offset()
}

fn settle_with_unchanged_extents(
   surface: &mut VerticalScrollSurface,
   start_ns: u64,
   steps_ns: &[u64],
   content_extent: f32,
   viewport_extent: f32,
) -> f32
{
   let mut now_ns = start_ns;
   let mut step = 0usize;
   while surface.wants_next_frame() && step < 4_096
   {
      now_ns = now_ns.saturating_add(steps_ns[step % steps_ns.len()]);
      let _ = surface.advance_to(now_ns);
      assert!(!surface.update_extents(content_extent, viewport_extent));
      step += 1;
   }
   assert!(step < 4_096, "inertia did not settle");
   assert!(surface.is_settled());
   surface.offset()
}

#[test]
fn drag_slop_applies_the_complete_displacement_once_crossed()
{
   let mut surface = VerticalScrollSurface::new(4_000.0, 800.0);
   let start_ns = 1_000 * MS;
   assert!(!surface.input_touch(&touch(1, TouchPhase::Start, 500.0, start_ns)));
   assert!(!surface.input_touch(&touch(1, TouchPhase::Move, 496.0, start_ns + 10 * MS)));
   assert_eq!(surface.offset(), 0.0);

   assert!(surface.input_touch(&touch(1, TouchPhase::Move, 490.0, start_ns + 20 * MS)));
   assert_eq!(surface.offset(), 10.0);
   assert!(surface.input_touch(&touch(1, TouchPhase::Move, 480.0, start_ns + 30 * MS)));
   assert_eq!(surface.offset(), 20.0);
   assert!(!surface.is_settled());
   assert!(!surface.wants_next_frame());

   assert!(!surface.input_touch(&touch(1, TouchPhase::Cancel, 480.0, start_ns + 40 * MS)));
   assert!(surface.is_settled());
}

#[test]
fn release_moves_through_inertia_and_settles()
{
   let start_ns = 2_000 * MS;
   let mut surface = VerticalScrollSurface::new(5_000.0, 800.0);
   begin_forward_fling(&mut surface, start_ns);
   assert_eq!(surface.offset(), 150.0);

   let first = surface.offset();
   assert!(surface.advance_to(start_ns + 38 * MS));
   assert!(surface.offset() > first);
   let final_offset = settle_with_steps(&mut surface, start_ns + 38 * MS, &[8_333_333]);

   assert!(final_offset > 1_000.0);
   assert!(final_offset < surface.max_offset());
   assert!(!surface.wants_next_frame());
}

#[test]
fn terminal_travel_is_callback_partition_independent()
{
   let start_ns = 3_000 * MS;
   let mut at_60_hz = VerticalScrollSurface::new(10_000.0, 800.0);
   let mut at_120_hz = VerticalScrollSurface::new(10_000.0, 800.0);
   let mut irregular = VerticalScrollSurface::new(10_000.0, 800.0);
   begin_forward_fling(&mut at_60_hz, start_ns);
   begin_forward_fling(&mut at_120_hz, start_ns);
   begin_forward_fling(&mut irregular, start_ns);

   let release_ns = start_ns + 30 * MS;
   let offset_60 = settle_with_steps(&mut at_60_hz, release_ns, &[16_666_667]);
   let offset_120 = settle_with_steps(&mut at_120_hz, release_ns, &[8_333_333]);
   let offset_irregular =
      settle_with_steps(&mut irregular, release_ns, &[5 * MS, 11 * MS, 7 * MS, 19 * MS]);

   assert!((offset_60 - offset_120).abs() <= 0.01);
   assert!((offset_60 - offset_irregular).abs() <= 0.01);
}

#[test]
fn unchanged_extent_updates_preserve_partition_independent_travel()
{
   let start_ns = 3_500 * MS;
   let content_extent = 500_000.0;
   let viewport_extent = 800.0;
   let mut at_60_hz = VerticalScrollSurface::new(content_extent, viewport_extent);
   let mut at_120_hz = VerticalScrollSurface::new(content_extent, viewport_extent);
   let mut irregular = VerticalScrollSurface::new(content_extent, viewport_extent);
   at_60_hz.set_offset(400_000.0);
   at_120_hz.set_offset(400_000.0);
   irregular.set_offset(400_000.0);
   begin_forward_fling(&mut at_60_hz, start_ns);
   begin_forward_fling(&mut at_120_hz, start_ns);
   begin_forward_fling(&mut irregular, start_ns);

   let release_ns = start_ns + 30 * MS;
   let offset_60 = settle_with_unchanged_extents(
      &mut at_60_hz,
      release_ns,
      &[16_666_667],
      content_extent,
      viewport_extent,
   );
   let offset_120 = settle_with_unchanged_extents(
      &mut at_120_hz,
      release_ns,
      &[8_333_333],
      content_extent,
      viewport_extent,
   );
   let offset_irregular = settle_with_unchanged_extents(
      &mut irregular,
      release_ns,
      &[5 * MS, 11 * MS, 7 * MS, 19 * MS],
      content_extent,
      viewport_extent,
   );

   assert!((offset_60 - offset_120).abs() <= 0.01);
   assert!((offset_60 - offset_irregular).abs() <= 0.01);
}

#[test]
fn reverse_fling_from_bottom_is_symmetric()
{
   let start_ns = 4_000 * MS;
   let mut forward = VerticalScrollSurface::new(10_000.0, 800.0);
   begin_forward_fling(&mut forward, start_ns);
   let forward_end = settle_with_steps(&mut forward, start_ns + 30 * MS, &[8_333_333]);

   let mut reverse = VerticalScrollSurface::new(10_000.0, 800.0);
   let max_offset = reverse.max_offset();
   reverse.set_offset(max_offset);
   assert!(!reverse.input_touch(&touch(2, TouchPhase::Start, 150.0, start_ns)));
   assert!(reverse.input_touch(&touch(2, TouchPhase::Move, 200.0, start_ns + 10 * MS)));
   assert!(reverse.input_touch(&touch(2, TouchPhase::Move, 250.0, start_ns + 20 * MS)));
   assert!(reverse.input_touch(&touch(2, TouchPhase::End, 300.0, start_ns + 30 * MS)));
   let reverse_end = settle_with_steps(&mut reverse, start_ns + 30 * MS, &[8_333_333]);

   let reverse_travel = max_offset - reverse_end;
   assert!((forward_end - reverse_travel).abs() <= 0.01);
}

#[test]
fn second_touch_cancels_drag_and_remaining_touch_restarts_without_a_jump()
{
   let start_ns = 5_000 * MS;
   let mut surface = VerticalScrollSurface::new(4_000.0, 800.0);
   surface.input_touch(&touch(1, TouchPhase::Start, 500.0, start_ns));
   surface.input_touch(&touch(1, TouchPhase::Move, 450.0, start_ns + 10 * MS));
   assert_eq!(surface.offset(), 50.0);

   surface.input_touch(&touch(2, TouchPhase::Start, 550.0, start_ns + 20 * MS));
   surface.input_touch(&touch(1, TouchPhase::Move, 400.0, start_ns + 30 * MS));
   assert_eq!(surface.offset(), 50.0);
   surface.input_touch(&touch(2, TouchPhase::End, 550.0, start_ns + 40 * MS));
   assert_eq!(surface.offset(), 50.0);

   assert!(surface.input_touch(&touch(1, TouchPhase::Move, 390.0, start_ns + 50 * MS)));
   assert_eq!(surface.offset(), 60.0);
   surface.input_touch(&touch(1, TouchPhase::Cancel, 390.0, start_ns + 60 * MS));
   assert!(surface.is_settled());
}

#[test]
fn cancel_never_flings_and_stationary_touch_requests_no_frames()
{
   let start_ns = 6_000 * MS;
   let mut surface = VerticalScrollSurface::new(4_000.0, 800.0);
   surface.input_touch(&touch(1, TouchPhase::Start, 500.0, start_ns));
   assert!(!surface.is_settled());
   assert!(!surface.wants_next_frame());
   surface.input_touch(&touch(1, TouchPhase::Move, 450.0, start_ns + 10 * MS));
   surface.input_touch(&touch(1, TouchPhase::Cancel, 450.0, start_ns + 20 * MS));

   assert!(surface.is_settled());
   assert!(!surface.wants_next_frame());
   assert!(!surface.advance_to(start_ns + 100 * MS));
}

#[test]
fn subthreshold_release_settles_without_a_reverse_frame()
{
   let start_ns = 6_500 * MS;
   let mut surface = VerticalScrollSurface::new(4_000.0, 800.0);
   surface.input_touch(&touch(1, TouchPhase::Start, 500.0, start_ns));
   surface.input_touch(&touch(1, TouchPhase::Move, 490.0, start_ns + 10 * MS));
   surface.input_touch(&touch(1, TouchPhase::Move, 489.9, start_ns + 1_000 * MS));
   surface.input_touch(&touch(1, TouchPhase::End, 489.8, start_ns + 1_050 * MS));
   let release_offset = surface.offset();

   assert!(surface.is_settled());
   assert!(!surface.wants_next_frame());
   assert!(!surface.advance_to(start_ns + 1_066 * MS));
   assert_eq!(surface.offset(), release_offset);
}

#[test]
fn clamped_bounds_extents_and_non_monotonic_time_stop_safely()
{
   let start_ns = 7_000 * MS;
   let mut surface = VerticalScrollSurface::new(1_200.0, 800.0);
   begin_forward_fling(&mut surface, start_ns);
   assert!(surface.advance_to(start_ns + 40 * MS));
   let advanced = surface.offset();
   assert!(!surface.advance_to(start_ns + 39 * MS));
   assert_eq!(surface.offset(), advanced);

   assert!(surface.update_extents(900.0, 800.0));
   assert_eq!(surface.offset(), 100.0);
   assert!(surface.is_settled());

   assert!(surface.update_extents(f32::NAN, f32::INFINITY));
   assert_eq!(surface.offset(), 0.0);
   assert_eq!(surface.max_offset(), 0.0);
   assert!(surface.is_settled());
}

#[test]
fn content_that_fits_the_viewport_never_starts_inertia()
{
   let start_ns = 8_000 * MS;
   let mut surface = VerticalScrollSurface::new(400.0, 800.0);
   surface.input_touch(&touch(1, TouchPhase::Start, 700.0, start_ns));
   assert!(!surface.input_touch(&touch(1, TouchPhase::Move, 500.0, start_ns + 10 * MS)));
   assert!(!surface.input_touch(&touch(1, TouchPhase::End, 300.0, start_ns + 20 * MS)));

   assert_eq!(surface.offset(), 0.0);
   assert!(surface.is_settled());
   assert!(!surface.wants_next_frame());
}
