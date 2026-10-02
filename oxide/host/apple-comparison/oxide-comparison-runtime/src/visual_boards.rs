use super::*;

pub(super) fn is_case(name: &str) -> bool
{
   matches!(name, "visual-controls" | "visual-editing" | "visual-typography" | "visual-composition" | "visual-layout" | "visual-pickers" | "visual-opacity" | "visual-images" | "visual-geometry" | "visual-editing-edges")
}

// Keep mutable draw lists until the shared text frame has published every atlas.
pub(super) struct VisualChunk
{
   pub builder: ui::DrawListBuilder,
   pub id: u64,
   pub revision: u64,
   pub origin: [f32; 2],
   pub properties: Vec<gfx::RenderPropertySlot>,
   pub slots: Vec<gfx::RenderPropertySlotId>,
   pub clips: Vec<gfx::RenderDynamicClip>,
   pub layer: Option<gfx::RenderLayerInstance>,
   pub resources: Vec<gfx::RenderResourceDependency>,
}

impl VisualChunk
{
   pub fn new(builder: ui::DrawListBuilder, id: u64, revision: u64) -> Self
   {
      Self {builder, id, revision, origin: [0.0; 2], properties: Vec::new(), slots: Vec::new(), clips: Vec::new(), layer: None, resources: Vec::new()}
   }

   fn seal(&self) -> Result<gfx::RenderChunkInstance, ()>
   {
      let chunk = gfx::RenderChunk::new(gfx::RenderChunkId(self.id), gfx::RenderChunkRevisions {structural: self.revision, ..Default::default()}, self.builder.drawlist().clone(), gfx::ChunkIndexMode::Local, &self.resources).map_err(visual_error)?;
      let mut instance = gfx::RenderChunkInstance::new(chunk, self.origin);
      instance.property_slots = self.slots.clone().into();
      instance.dynamic_clips = self.clips.clone().into();
      instance.layer = self.layer;
      Ok(instance)
   }
}

pub(super) struct VisualBoards
{
   revision: u64,
   name: String,
   heading: Label,
   root_chunk: Option<CachedRootChunk>,
   controls: super::visual_controls::VisualControls,
   structure: super::visual_structure::VisualStructure,
   extended: super::visual_extended::VisualExtended,
   editing_edges: super::visual_editing_edges::VisualEditingEdges,
}

impl VisualBoards
{
   pub(super) fn new(name: &str, text: &mut TextCtx, renderer: &mut metal::MetalRenderer) -> Result<Self, ()>
   {
      let font_id = text.fonts.add_font(text::Font::from_bytes(FONT_BYTES.to_vec()));
      let bold_id = text.fonts.add_font(text::Font::from_bytes_with_variations(FONT_BYTES.to_vec(), &[text::FontVariation {tag: *b"wght", value: 700.0}]));
      let mut images = Vec::new();
      for bytes in IMAGE_BYTES
      {
         let (pixels, width, height) = decode_png(bytes)?;
         let handle = renderer.image_create_rgba8_immutable(width, height, &pixels, width as usize * 4, true);
         if handle.0 == 0 {return Err(());}
         images.push((handle, width, height));
      }
      let fixture: serde_json::Value = serde_json::from_str(include_str!("../../fixtures/visual.json")).map_err(visual_error)?;
      let heading = Label {text: fixture["boards"][name]["title"].as_str().ok_or(())?.into(), font_id, font_px: 18.0,
         color: rgba([0.12, 0.16, 0.2, 1.0]), align: ui::elements::Align::Left, wrap: false};
      Ok(Self {revision: 0, name: name.into(), heading, root_chunk: None, controls: super::visual_controls::VisualControls::new(font_id, bold_id),
         structure: super::visual_structure::VisualStructure::new(font_id, images),
         extended: super::visual_extended::VisualExtended::new(font_id, renderer)?,
         editing_edges: super::visual_editing_edges::VisualEditingEdges::new(font_id)})
   }

   pub(super) fn draw(&mut self, stage: usize, text: &mut TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder) -> Result<gfx::RenderSnapshot, ()>
   {
      self.draw_at(stage, None, text, renderer, builder)
   }

   pub(super) fn draw_timed(&mut self, stage: usize, elapsed_ms: u64, stage_elapsed_ms: u64, text: &mut TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder) -> Result<gfx::RenderSnapshot, ()>
   {
      self.draw_at(stage, Some((elapsed_ms, stage_elapsed_ms)), text, renderer, builder)
   }

   pub(super) fn next_wakeup(&self, stage: usize, stage_elapsed_ms: u64) -> Option<u64>
   {
      match (self.name.as_str(), stage)
      {
         // The production indeterminate ProgressBar remains visible in every
         // controls state, so its elapsed-time phase requires a real redraw.
         ("visual-controls", _) => return Some(0),
         // ToggleState and PickerState are production springs/inertial state.
         // Their activity is real until the next stage boundary.
         ("visual-pickers", 1) if stage_elapsed_ms < 2_000 => return Some(0),
         _ => return None,
      }
   }

   fn draw_at(&mut self, stage: usize, timed_elapsed_ms: Option<(u64, u64)>, text: &mut TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder) -> Result<gfx::RenderSnapshot, ()>
   {
      builder.clear();
      builder.rrect(gfx::RectF::new(0.0, 0.0, 390.0, 844.0), [0.0; 4], rgba([1.0; 4]));
      text.begin_frame_at_scale(3.0);
      self.heading.encode(gfx::RectF::new(20.0, 12.0, 350.0, 32.0), 3.0, text, &mut MtlUploader {renderer}, builder);
      if let Some((elapsed_ms, stage_elapsed_ms)) = timed_elapsed_ms
      {
         self.controls.draw_timed(&self.name, stage, elapsed_ms, stage_elapsed_ms, text, renderer, builder);
      }
      else {self.controls.draw(&self.name, stage, text, renderer, builder);}
      self.structure.draw(&self.name, stage, text, renderer, builder);
      self.extended.draw(&self.name, stage, text, renderer, builder);
      if self.name == "visual-editing-edges" {self.editing_edges.draw(stage, text, renderer, builder);}
      {
         let mut builders = vec![&mut *builder];
         builders.extend(self.controls.chunks.iter_mut().map(|chunk| &mut chunk.builder));
         builders.extend(self.structure.chunks.iter_mut().map(|chunk| &mut chunk.builder));
         builders.extend(self.extended.chunks.iter_mut().map(|chunk| &mut chunk.builder));
         text.finish_frame_many(&mut MtlUploader {renderer}, &mut builders);
      }
      let resources = self.extended.resources(&self.name, stage, renderer);
      let base = if let Some(root) = self.root_chunk.as_ref().filter(|root| root.matches(builder.drawlist(), &resources))
      {
         root.chunk.clone()
      }
      else
      {
         self.revision += 1;
         let root = gfx::RenderChunk::new(gfx::RenderChunkId(500), gfx::RenderChunkRevisions {structural: self.revision, ..Default::default()}, builder.drawlist().clone(), gfx::ChunkIndexMode::Local, &resources).map_err(visual_error)?;
         self.root_chunk = Some(CachedRootChunk {chunk: root.clone(), resources});
         root
      };
      let mut instances = vec![gfx::RenderChunkInstance::new(base, [0.0; 2])];
      let mut properties = Vec::new();
      for chunk in self.controls.chunks.iter().chain(self.structure.chunks.iter()).chain(self.extended.chunks.iter())
      {
         instances.push(chunk.seal()?);
         properties.extend_from_slice(&chunk.properties);
      }
      gfx::RenderSnapshot::new(instances, properties, gfx::Damage {rects: Vec::new()}).map_err(visual_error)
   }
}

struct CachedRootChunk
{
   chunk: gfx::RenderChunk,
   resources: Vec<gfx::RenderResourceDependency>,
}

impl CachedRootChunk
{
   fn matches(&self, list: &gfx::DrawList, resources: &[gfx::RenderResourceDependency]) -> bool
   {
      self.chunk.draw_list() == list && self.resources == resources
   }
}

fn visual_error(error: impl std::fmt::Debug)
{
   eprintln!("Oxide visual benchmark: {error:?}");
}

#[cfg(test)]
mod tests
{
   use super::*;

   #[test]
   fn cached_root_requires_the_same_draw_list_and_image_generation()
   {
      let image = gfx::ImageHandle(41);
      let resources = vec![gfx::RenderResourceDependency {image, generation: 7}];
      let list = gfx::DrawList {items: vec![gfx::DrawCmd::Image {tex: image,
         dst: gfx::RectF::new(0.0, 0.0, 10.0, 10.0), src: gfx::RectF::new(0.0, 0.0, 10.0, 10.0), alpha: 1.0}], vertices: Vec::new(), indices: Vec::new()};
      let chunk = gfx::RenderChunk::new(gfx::RenderChunkId(500), gfx::RenderChunkRevisions::default(), list.clone(), gfx::ChunkIndexMode::Local, &resources).unwrap();
      let cached = CachedRootChunk {chunk, resources: resources.clone()};

      assert!(cached.matches(&list, &resources));
      assert!(!cached.matches(&list, &[gfx::RenderResourceDependency {image, generation: 8}]));

      let mut changed_list = list.clone();
      changed_list.items.clear();
      assert!(!cached.matches(&changed_list, &resources));
   }
}
