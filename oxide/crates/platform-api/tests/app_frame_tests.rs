use oxide_platform_api::{
   App, AppEvent, FrameContext, FrameDemand, InitContext, PreparedFrame, UpdateContext,
};
use oxide_renderer_api::{DrawCmd, DrawList, ImageHandle, RectF, RectI, RuntimeImageUploader};

struct TestUploader;

impl RuntimeImageUploader for TestUploader
{
   fn create_a8(&mut self, _width: u32, _height: u32, _data: &[u8], _row_bytes: usize) -> ImageHandle
   {
      ImageHandle(1)
   }

   fn update_a8(
      &mut self,
      _handle: ImageHandle,
      _x: u32,
      _y: u32,
      _width: u32,
      _height: u32,
      _data: &[u8],
      _row_bytes: usize,
   )
   {
   }

   fn try_create_rgba8(
      &mut self,
      _width: u32,
      _height: u32,
      _data: &[u8],
      _row_bytes: usize,
   ) -> Option<ImageHandle>
   {
      Some(ImageHandle(2))
   }
}

struct PreparedApp
{
   draw_list: DrawList,
   damage: Vec<RectI>,
   observed: Option<FrameContext>,
}

impl PreparedApp
{
   fn new() -> Self
   {
      Self { draw_list: DrawList::default(), damage: Vec::new(), observed: None }
   }
}

impl App for PreparedApp
{
   fn init(&mut self, _ctx: &mut InitContext)
   {
   }

   fn event(&mut self, _event: AppEvent, _ctx: &mut UpdateContext)
   {
   }

   fn prepare_frame(
      &mut self,
      context: FrameContext,
      uploader: &mut dyn RuntimeImageUploader,
   ) -> FrameDemand
   {
      assert_eq!(
         uploader.try_create_rgba8(1, 1, &[0, 0, 0, 0], 4),
         Some(ImageHandle(2)),
      );
      self.observed = Some(context);
      self.draw_list.items.clear();
      self.draw_list.items.push(DrawCmd::ClipPush { rect: RectI::new(0, 0, 390, 844) });
      self.damage.clear();
      self.damage.push(RectI::new(0, 20, 390, 80));
      FrameDemand::NextVsync
   }

   fn prepared_frame(&self) -> Option<PreparedFrame<'_>>
   {
      Some(PreparedFrame { draw_list: &self.draw_list, damage: &self.damage })
   }
}

struct CompatibilityApp;

impl App for CompatibilityApp
{
   fn init(&mut self, _ctx: &mut InitContext)
   {
   }

   fn event(&mut self, _event: AppEvent, _ctx: &mut UpdateContext)
   {
   }
}

#[test]
fn prepared_app_retains_frame_until_the_host_submits_it()
{
   let context = FrameContext {
      frame_id: 7,
      timestamp_ns: 1_000_000_000,
      target_timestamp_ns: 1_008_333_333,
      dt_ns: 8_333_333,
      viewport: RectF::new(0.0, 0.0, 390.0, 844.0),
      scale: 3.0,
   };
   let mut app = PreparedApp::new();
   let mut uploader = TestUploader;

   assert_eq!(app.prepare_frame(context, &mut uploader), FrameDemand::NextVsync);
   let frame = app.prepared_frame().expect("prepared frame");

   assert_eq!(app.observed, Some(context));
   assert_eq!(frame.draw_list.items.len(), 1);
   assert_eq!(frame.damage, &[RectI::new(0, 20, 390, 80)]);
}

#[test]
fn compatibility_app_does_not_claim_a_prepared_frame()
{
   let mut app = CompatibilityApp;
   let context = FrameContext {
      frame_id: 1,
      timestamp_ns: 0,
      target_timestamp_ns: 0,
      dt_ns: 0,
      viewport: RectF::new(0.0, 0.0, 1.0, 1.0),
      scale: 1.0,
   };
   let mut uploader = TestUploader;

   assert_eq!(app.prepare_frame(context, &mut uploader), FrameDemand::Idle);
   assert!(app.prepared_frame().is_none());
}
