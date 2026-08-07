use super::*;

struct NoopHaptics;

impl oxide_platform_api::Haptics for NoopHaptics
{
   fn play(&self, _pattern: oxide_platform_api::HapticPattern)
   {
   }
}

struct EventApp;

impl App for EventApp
{
   fn init(&mut self, _context: &mut oxide_platform_api::InitContext)
   {
   }

   fn event(&mut self, _event: AppEvent, _context: &mut UpdateContext)
   {
   }
}

#[test]
fn legacy_encoder_borrow_and_draw_storage_survive_frame_reuse()
{
   let mut encoder = LegacyDrawEncoder::new();
   encoder.begin_frame();
   let vertices = [
      gfx_api::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: u32::MAX },
      gfx_api::Vertex { x: 10.0, y: 0.0, u: 1.0, v: 0.0, rgba: u32::MAX },
      gfx_api::Vertex { x: 0.0, y: 10.0, u: 0.0, v: 1.0, rgba: u32::MAX },
      gfx_api::Vertex { x: 10.0, y: 10.0, u: 1.0, v: 1.0, rgba: u32::MAX },
   ];
   {
      let context = gfx_api::RenderContext { frame_id: 1, encoder: &mut encoder };
      context.encoder.draw_solid(&vertices, gfx_api::Color::rgba(1.0, 0.0, 0.0, 1.0));
   }
   encoder.finish_frame();

   assert_eq!(encoder.draw_list().items.len(), 1);
   assert_eq!(encoder.draw_list().vertices.len(), 4);
   let capacity = encoder.draw_list().vertices.capacity();
   encoder.begin_frame();
   assert_eq!(encoder.draw_list().vertices.capacity(), capacity);
}

#[test]
fn injected_cancel_and_submit_failure_retry_without_success_spin()
{
   let mut app = AppState::default();
   app.pending_wake_generation = FRAME_WAKE_GENERATION.load(Ordering::Acquire);
   app.presented_wake_generation = app.pending_wake_generation;
   assert!(!injected_frame_should_render(
      app.presented_wake_generation,
      FrameDemand::Idle,
   ));

   let surface = PreparedSurface::new(1320, 2868, 3.0);
   app.prepared_frame = true;
   app.prepared_surface = Some(surface);
   request_injected_frame_retry(&mut app);
   let retry_generation = app.prepared_retry_generation.expect("retry generation");
   assert!(app.prepared_frame);
   assert!(injected_frame_should_render(
      app.presented_wake_generation,
      FrameDemand::Idle,
   ));
   assert!(reuse_injected_prepared_frame(
      &mut app,
      surface,
      retry_generation,
   ));
   assert!(app.prepared_frame);
   assert_eq!(app.pending_wake_generation, retry_generation);
   assert_eq!(app.prepared_retry_generation, None);

   app.prepared_retry_generation = Some(retry_generation);
   assert!(!reuse_injected_prepared_frame(
      &mut app,
      PreparedSurface::new(1170, 2532, 3.0),
      retry_generation,
   ));
   assert!(!app.prepared_frame);
   assert_eq!(app.prepared_surface, None);
   assert_eq!(app.prepared_retry_generation, None);

   app.prepared_frame = true;
   app.prepared_surface = Some(surface);
   request_injected_frame_retry(&mut app);
   let superseded_retry = app.prepared_retry_generation.expect("superseded retry");
   assert!(!reuse_injected_prepared_frame(
      &mut app,
      surface,
      superseded_retry.wrapping_add(1),
   ));
   assert!(!app.prepared_frame);

   app.pending_wake_generation = FRAME_WAKE_GENERATION.load(Ordering::Acquire);
   app.presented_wake_generation = app.pending_wake_generation;
   assert!(!injected_frame_should_render(
      app.presented_wake_generation,
      FrameDemand::Idle,
   ));
}

#[test]
fn injected_event_invalidates_any_unsubmitted_prepared_frame()
{
   let update = UpdateContext {
      post_task: Box::new(|_| {}),
      timers: oxide_platform_api::Timers::default(),
      haptics: Box::new(NoopHaptics),
   };
   let mut app = AppState::default();
   app.injected = Some(InjectedApp::new(Box::new(EventApp), update));
   app.prepared_frame = true;
   app.prepared_surface = Some(PreparedSurface::new(1320, 2868, 3.0));
   app.prepared_retry_generation = Some(7);
   app.pending_damage_rects.push(gfx_api::RectI::new(0, 0, 10, 10));

   assert!(dispatch_injected_event_without_wake(
      &mut app,
      AppEvent::Lifecycle(Lifecycle::WillEnterForeground),
   ));
   assert!(!app.prepared_frame);
   assert_eq!(app.prepared_surface, None);
   assert_eq!(app.prepared_retry_generation, None);
   assert!(app.pending_damage_rects.is_empty());
}

#[test]
fn renderer_stats_observation_does_not_invalidate_frame_state()
{
   let update = UpdateContext {
      post_task: Box::new(|_| {}),
      timers: oxide_platform_api::Timers::default(),
      haptics: Box::new(NoopHaptics),
   };
   let mut app = AppState::default();
   app.injected = Some(InjectedApp::new(Box::new(EventApp), update));
   app.prepared_frame = true;
   app.prepared_surface = Some(PreparedSurface::new(1320, 2868, 3.0));
   app.prepared_retry_generation = Some(7);
   app.pending_damage_rects.push(gfx_api::RectI::new(0, 0, 10, 10));
   let stats = RendererStats {
      frame_id: 8,
      encode_ms: 0.25,
      damage_pct: 5.0,
      damage_rects: 1,
      draws: 2,
      sample_count: 1,
      hdr: false,
   };

   assert!(observe_injected_renderer_stats(&mut app, stats));
   assert!(app.prepared_frame);
   assert_eq!(app.prepared_surface, Some(PreparedSurface::new(1320, 2868, 3.0)));
   assert_eq!(app.prepared_retry_generation, Some(7));
   assert_eq!(app.pending_damage_rects, [gfx_api::RectI::new(0, 0, 10, 10)]);
}
