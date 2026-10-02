#[cfg(feature = "native-measurement")]
use std::time::Instant;

use oxide_renderer_metal as metal;

#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(feature = "native-measurement"), allow(dead_code))]
pub struct NativeFrameMetrics
{
   pub prepare_ms: f64,
   pub preparation_texture_upload_bytes: u64,
   pub encode_submit_ms: f64,
   pub renderer_perf: metal::PerfStats,
}

impl Default for NativeFrameMetrics
{
   fn default() -> Self
   {
      Self {prepare_ms: 0.0, preparation_texture_upload_bytes: 0, encode_submit_ms: 0.0, renderer_perf: metal::PerfStats::default()}
   }
}

#[derive(Default)]
pub(crate) struct NativeMeasurement
{
   pub(crate) metrics: NativeFrameMetrics,
   pub(crate) accounting: bool,
}

pub(crate) struct NativeFrameTimer
{
   #[cfg(feature = "native-measurement")]
   started: Instant,
   #[cfg(feature = "native-measurement")]
   encode_started: Option<Instant>,
   #[cfg(feature = "native-measurement")]
   preparation_texture_upload_bytes_before: u64,
   #[cfg(feature = "native-measurement")]
   collect_preparation_texture_upload_bytes: bool,
}

impl NativeFrameTimer
{
   pub(crate) fn start(renderer: &metal::MetalRenderer, accounting: bool) -> Option<Self>
   {
      #[cfg(feature = "native-measurement")]
      {Some(Self {
         started: Instant::now(), encode_started: None,
         preparation_texture_upload_bytes_before: if accounting {renderer.last_stats().texture_upload_bytes} else {0},
         collect_preparation_texture_upload_bytes: accounting,
      })}
      #[cfg(not(feature = "native-measurement"))]
      {let _ = (renderer, accounting); None}
   }

   pub(crate) fn prepared(&mut self, metrics: &mut NativeFrameMetrics, renderer: &metal::MetalRenderer)
   {
      #[cfg(feature = "native-measurement")]
      {
         metrics.prepare_ms = self.started.elapsed().as_secs_f64() * 1_000.0;
         metrics.preparation_texture_upload_bytes = if self.collect_preparation_texture_upload_bytes
         {
            renderer.last_stats().texture_upload_bytes
               .saturating_sub(self.preparation_texture_upload_bytes_before)
         }
         else {0};
         self.encode_started = Some(Instant::now());
      }
      #[cfg(not(feature = "native-measurement"))]
      {let _ = (metrics, renderer);}
   }

   pub(crate) fn submitted(self, metrics: &mut NativeFrameMetrics, renderer: &metal::MetalRenderer)
   {
      #[cfg(feature = "native-measurement")]
      {
         if let Some(started) = self.encode_started
         {
            metrics.encode_submit_ms = started.elapsed().as_secs_f64() * 1_000.0;
         }
         metrics.renderer_perf = renderer.last_stats();
      }
      #[cfg(not(feature = "native-measurement"))]
      {let _ = (metrics, renderer);}
   }
}
