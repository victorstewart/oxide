#![cfg(all(
   feature = "snapshot-tests",
   any(target_os = "macos", all(target_os = "ios", not(target_abi = "sim")))
))]

use oxide_renderer_api::{self as api, Renderer};
use oxide_renderer_metal::{MetalInitError, MetalRenderer, MetalRendererConfig, SdrCompositing};

fn srgb_to_linear(value: f32) -> f32
{
   if value <= 0.04045 {value / 12.92}
   else {((value + 0.055) / 1.055).powf(2.4)}
}

fn linear_to_srgb(value: f32) -> f32
{
   if value <= 0.0031308 {value * 12.92}
   else {1.055 * value.powf(1.0 / 2.4) - 0.055}
}

fn rgba8(value: [f32; 3]) -> [u8; 3]
{
   value.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn pixel(bgra: &[u8], width: u32, x: u32, y: u32) -> [u8; 4]
{
   let offset = ((y * width + x) * 4) as usize;
   [bgra[offset + 2], bgra[offset + 1], bgra[offset], bgra[offset + 3]]
}

fn render(mode: SdrCompositing, items: Vec<api::DrawCmd>) -> [u8; 4]
{
   let mut renderer = MetalRenderer::new_with_config_and_sdr_compositing(
      MetalRendererConfig::default(), mode,
   ).expect("create renderer");
   renderer.resize(16, 16, 1.0).expect("resize");
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_pass(&api::DrawList {items, ..api::DrawList::default()});
   renderer.submit(token).expect("submit");
   let (_, _, bytes) = renderer.readback_bgra8().expect("readback");
   pixel(&bytes, 16, 8, 8)
}

fn rect(color: api::Color) -> api::DrawCmd
{
   api::DrawCmd::RRect {
      rect: api::RectF::new(0.0, 0.0, 16.0, 16.0),
      radii: [0.0; 4],
      color,
   }
}

#[test]
fn sdr_compositing_round_trips_opaque_srgb_input_in_both_modes()
{
   let encoded = [0.23, 0.57, 0.91];
   let color = api::Color::from_srgba(encoded[0], encoded[1], encoded[2], 1.0);
   let expected = rgba8(encoded);
   for mode in [SdrCompositing::Linear, SdrCompositing::SrgbSourceOver]
   {
      let actual = render(mode, vec![rect(color)]);
      for channel in 0 .. 3
      {
         assert!(actual[channel].abs_diff(expected[channel]) <= 1, "{mode:?} channel {channel}: {actual:?}");
      }
      assert_eq!(actual[3], 255);
   }
}

#[test]
fn sdr_compositing_selects_encoded_source_over_without_changing_linear_default()
{
   let backdrop = [0.16, 0.42, 0.73];
   let source = [0.82, 0.19, 0.31];
   let alpha = 0.5;
   let items = vec![
      rect(api::Color::from_srgba(backdrop[0], backdrop[1], backdrop[2], 1.0)),
      rect(api::Color::from_srgba(source[0], source[1], source[2], alpha)),
      rect(api::Color::from_srgba(source[0], source[1], source[2], alpha)),
   ];
   let encoded = render(SdrCompositing::SrgbSourceOver, items.clone());
   let linear = render(SdrCompositing::Linear, items);
   let gamma_expected = rgba8([0, 1, 2].map(|index| {
      let once = source[index] * alpha + backdrop[index] * (1.0 - alpha);
      source[index] * alpha + once * (1.0 - alpha)
   }));
   let linear_expected = rgba8([0, 1, 2].map(|index| {
      linear_to_srgb(
         srgb_to_linear(source[index]) * alpha
            + (srgb_to_linear(source[index]) * alpha
               + srgb_to_linear(backdrop[index]) * (1.0 - alpha)) * (1.0 - alpha),
      )
   }));
   for channel in 0 .. 3
   {
      assert!(encoded[channel].abs_diff(gamma_expected[channel]) <= 1, "encoded channel {channel}: {encoded:?}, expected {gamma_expected:?}");
      assert!(linear[channel].abs_diff(linear_expected[channel]) <= 1, "linear channel {channel}: {linear:?}, expected {linear_expected:?}");
   }
   assert_ne!(encoded[..3], linear[..3]);
   assert_eq!((encoded[3], linear[3]), (255, 255));
}

#[test]
fn sdr_compositing_prepared_snapshot_uses_the_selected_output_domain()
{
   let encoded = [0.27, 0.61, 0.88];
   let color = api::Color::from_srgba(encoded[0], encoded[1], encoded[2], 1.0);
   let chunk = api::RenderChunk::new(
      api::RenderChunkId(0x5352_4742),
      api::RenderChunkRevisions::default(),
      api::DrawList {items: vec![rect(color)], ..api::DrawList::default()},
      api::ChunkIndexMode::Local,
      &[],
   ).expect("chunk");
   let snapshot = api::RenderSnapshot::new(
      vec![api::RenderChunkInstance::new(chunk, [0.0, 0.0])],
      Vec::new(),
      api::Damage {rects: Vec::new()},
   ).expect("snapshot");
   for mode in [SdrCompositing::Linear, SdrCompositing::SrgbSourceOver]
   {
      let mut renderer = MetalRenderer::new_with_config_and_sdr_compositing(MetalRendererConfig::default(), mode).expect("renderer");
      renderer.resize(16, 16, 1.0).expect("resize");
      let token = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_snapshot(&snapshot).expect("encode snapshot");
      renderer.submit(token).expect("submit snapshot");
      let (_, _, bytes) = renderer.readback_bgra8().expect("readback");
      let actual = pixel(&bytes, 16, 8, 8);
      for channel in 0 .. 3
      {
         assert!(actual[channel].abs_diff(rgba8(encoded)[channel]) <= 1, "{mode:?} prepared channel {channel}: {actual:?}");
      }
   }
}

#[test]
fn sdr_compositing_rejects_hdr()
{
   let config = MetalRendererConfig {wants_hdr: true, ..MetalRendererConfig::default()};
   assert!(matches!(
      MetalRenderer::new_with_config_and_sdr_compositing(config, SdrCompositing::SrgbSourceOver),
      Err(MetalInitError::Configuration(_)),
   ));
}

fn layered_snapshot() -> api::RenderSnapshot
{
   let chunk = api::RenderChunk::new(
      api::RenderChunkId(0x4c41_5945),
      api::RenderChunkRevisions {structural: 1, geometry: 1, ..api::RenderChunkRevisions::default()},
      api::DrawList {
         items: vec![
            api::DrawCmd::RRect {
               rect: api::RectF::new(2.0, 2.0, 28.0, 28.0),
               radii: [4.0; 4],
               color: api::Color::from_srgba(0.88, 0.18, 0.30, 0.65),
            },
            api::DrawCmd::RRect {
               rect: api::RectF::new(14.0, 12.0, 28.0, 28.0),
               radii: [5.0; 4],
               color: api::Color::from_srgba(0.12, 0.68, 0.92, 0.70),
            },
         ],
         ..api::DrawList::default()
      },
      api::ChunkIndexMode::Local,
      &[],
   ).expect("layer chunk");
   let mut instance = api::RenderChunkInstance::new(chunk, [8.0, 8.0]);
   instance.layer = Some(api::RenderLayerInstance {
      id: 0x4c41_5945,
      rect: api::RectF::new(0.0, 0.0, 48.0, 48.0),
      dirty: false,
   });
   api::RenderSnapshot::new(vec![instance], Vec::new(), api::Damage {rects: Vec::new()}).expect("layer snapshot")
}

#[test]
fn sdr_compositing_cached_layer_passes_through_premultiplied_output_domain()
{
   for mode in [SdrCompositing::Linear, SdrCompositing::SrgbSourceOver]
   {
      let snapshot = layered_snapshot();
      let mut cached = MetalRenderer::new_with_config_and_sdr_compositing(MetalRendererConfig::default(), mode).expect("cached renderer");
      cached.resize(64, 64, 1.0).expect("cached resize");
      let first = cached.begin_frame(&api::FrameTarget, None);
      cached.encode_snapshot(&snapshot).expect("encode cold layer");
      cached.submit(first).expect("submit cold layer");
      let _ = cached.readback_bgra8().expect("cold readback");
      let clean = cached.begin_frame(&api::FrameTarget, None);
      cached.encode_snapshot(&snapshot).expect("encode clean layer");
      cached.submit(clean).expect("submit clean layer");
      let clean_stats = cached.last_stats();
      let (_, _, clean_pixels) = cached.readback_bgra8().expect("clean readback");
      assert_eq!(clean_stats.layer_cache_hits, 1, "{mode:?}");
      assert_eq!(clean_stats.layer_offscreen_draws, 0, "{mode:?}");

      let mut flat = api::DrawList::default();
      snapshot.flatten_into(&mut flat).expect("flatten layer");
      let mut reference = MetalRenderer::new_with_config_and_sdr_compositing(MetalRendererConfig::default(), mode).expect("reference renderer");
      reference.resize(64, 64, 1.0).expect("reference resize");
      let frame = reference.begin_frame(&api::FrameTarget, None);
      reference.encode_pass(&flat);
      reference.submit(frame).expect("submit flat");
      let (_, _, flat_pixels) = reference.readback_bgra8().expect("flat readback");
      assert_eq!(clean_pixels, flat_pixels, "{mode:?} cached layer changed output-domain premultiplication");
   }
}

#[test]
fn sdr_compositing_encodes_an_srgb_image_before_opacity_blending()
{
   let source = [0.74, 0.26, 0.48];
   let backdrop = [0.18, 0.44, 0.68];
   let alpha = 0.5;
   let image_bytes = [
      (source[0] * 255.0) as u8, (source[1] * 255.0) as u8, (source[2] * 255.0) as u8, 255,
   ];
   for mode in [SdrCompositing::Linear, SdrCompositing::SrgbSourceOver]
   {
      let mut renderer = MetalRenderer::new_with_config_and_sdr_compositing(MetalRendererConfig::default(), mode).expect("renderer");
      renderer.resize(16, 16, 1.0).expect("resize");
      let image = renderer.image_create_rgba8(1, 1, &image_bytes, 4);
      let list = api::DrawList {items: vec![
         rect(api::Color::from_srgba(backdrop[0], backdrop[1], backdrop[2], 1.0)),
         api::DrawCmd::Image {tex: image, dst: api::RectF::new(0.0, 0.0, 16.0, 16.0), src: api::RectF::new(0.0, 0.0, 1.0, 1.0), alpha},
      ], ..api::DrawList::default()};
      let token = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_pass(&list);
      renderer.submit(token).expect("submit");
      let (_, _, bytes) = renderer.readback_bgra8().expect("readback");
      let actual = pixel(&bytes, 16, 8, 8);
      let expected = rgba8([0, 1, 2].map(|index| match mode {
         SdrCompositing::SrgbSourceOver => source[index] * alpha + backdrop[index] * (1.0 - alpha),
         SdrCompositing::Linear => linear_to_srgb(srgb_to_linear(source[index]) * alpha + srgb_to_linear(backdrop[index]) * (1.0 - alpha)),
      }));
      for channel in 0 .. 3
      {
         assert!(actual[channel].abs_diff(expected[channel]) <= 2, "{mode:?} image channel {channel}: {actual:?}, expected {expected:?}");
      }
   }
}
