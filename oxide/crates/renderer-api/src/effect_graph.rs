use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectGraphRegion
{
   pub x: u32,
   pub y: u32,
   pub w: u32,
   pub h: u32,
}

impl EffectGraphRegion
{
   #[inline]
   pub const fn new(x: u32, y: u32, w: u32, h: u32) -> Self
   {
      Self { x, y, w, h }
   }

   #[inline]
   pub const fn is_empty(self) -> bool
   {
      self.w == 0 || self.h == 0
   }

   pub fn intersects(self, other: Self) -> bool
   {
      let x1 = self.x.saturating_add(self.w);
      let y1 = self.y.saturating_add(self.h);
      let other_x1 = other.x.saturating_add(other.w);
      let other_y1 = other.y.saturating_add(other.h);
      self.x < other_x1 && x1 > other.x && self.y < other_y1 && y1 > other.y
   }

   pub fn contains(self, other: Self) -> bool
   {
      let x1 = self.x.saturating_add(self.w);
      let y1 = self.y.saturating_add(self.h);
      let other_x1 = other.x.saturating_add(other.w);
      let other_y1 = other.y.saturating_add(other.h);
      self.x <= other.x && self.y <= other.y && x1 >= other_x1 && y1 >= other_y1
   }

   pub fn union(self, other: Self) -> Self
   {
      if self.is_empty()
      {
         return other;
      }
      if other.is_empty()
      {
         return self;
      }
      let x = self.x.min(other.x);
      let y = self.y.min(other.y);
      let x1 = self.x.saturating_add(self.w)
         .max(other.x.saturating_add(other.w));
      let y1 = self.y.saturating_add(self.h)
         .max(other.y.saturating_add(other.h));
      Self::new(x, y, x1.saturating_sub(x), y1.saturating_sub(y))
   }

   pub const fn pixels(self) -> u64
   {
      (self.w as u64).saturating_mul(self.h as u64)
   }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EffectGraphStorage
{
   Transient,
   Persistent,
   Memoryless,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectGraphTarget
{
   pub id: u64,
   pub format: u32,
   pub sample_count: u8,
   pub bytes_per_pixel: u8,
   pub storage: EffectGraphStorage,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectGraphPyramidSpec
{
   pub sigma_bits: u32,
   pub quality: u8,
   pub downsample_levels: u8,
   pub blur_passes: u8,
   pub materialized: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectGraphEventKind
{
   Write,
   Extract {
      region: EffectGraphRegion,
      downsampled: bool,
   },
   Filter {
      region: EffectGraphRegion,
      output: EffectGraphRegion,
      pyramid: EffectGraphPyramidSpec,
   },
   Effect {
      source: EffectGraphRegion,
      destination: EffectGraphRegion,
      output: EffectGraphRegion,
      pyramid: EffectGraphPyramidSpec,
   },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectGraphEvent
{
   pub command: u32,
   pub target: EffectGraphTarget,
   pub kind: EffectGraphEventKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectGraphEffect
{
   pub command: u32,
   pub capture: u32,
   pub pyramid: Option<u32>,
   pub source: EffectGraphRegion,
   pub destination: EffectGraphRegion,
   pub output: EffectGraphRegion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectGraphCaptureKind
{
   Snapshot,
   Extraction,
   ExtractionDownsample,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectGraphCapture
{
   pub kind: EffectGraphCaptureKind,
   pub target: EffectGraphTarget,
   pub first_command: u32,
   pub last_command: u32,
   pub effect_start: u32,
   pub effect_count: u32,
   pub source: EffectGraphRegion,
   pub destination: EffectGraphRegion,
   pub output: EffectGraphRegion,
   pub resource: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectGraphPyramid
{
   pub capture: u32,
   pub spec: EffectGraphPyramidSpec,
   pub first_command: u32,
   pub last_command: u32,
   pub effect_count: u32,
   pub region: EffectGraphRegion,
   pub resource_start: u32,
   pub resource_count: u32,
   pub resource: u32,
   pub scratch_resource: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectGraphResourceKind
{
   Capture,
   PyramidLevel { pyramid: u32, level: u8 },
   PyramidScratch { pyramid: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectGraphLoadAction
{
   DontCare,
   Clear,
   Load,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectGraphStoreAction
{
   DontCare,
   Store,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectGraphPassReason
{
   Capture,
   Extract,
   ExtractDownsample,
   Downsample,
   BlurHorizontal,
   BlurVertical,
   Composite,
   UpsampleComposite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectGraphPass
{
   pub reason: EffectGraphPassReason,
   pub target: EffectGraphTarget,
   pub region: EffectGraphRegion,
   pub first_command: u32,
   pub last_command: u32,
   pub read_resource: Option<u32>,
   pub write_resource: Option<u32>,
   pub load: EffectGraphLoadAction,
   pub store: EffectGraphStoreAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectGraphResource
{
   pub kind: EffectGraphResourceKind,
   pub target: EffectGraphTarget,
   pub region: EffectGraphRegion,
   pub first_use: u32,
   pub last_use: u32,
   pub alias_slot: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectGraphStats
{
   pub effects: u32,
   pub captures: u32,
   pub pyramids: u32,
   pub pyramid_reuses: u32,
   pub capture_passes: u32,
   pub extract_passes: u32,
   pub downsample_passes: u32,
   pub blur_horizontal_passes: u32,
   pub blur_vertical_passes: u32,
   pub upsample_passes: u32,
   pub composite_passes: u32,
   pub max_lifetime_commands: u32,
   pub resources: u32,
   pub alias_slots: u32,
   pub logical_bytes: u64,
   pub physical_bytes: u64,
   pub aliased_bytes: u64,
}

#[derive(Clone, Copy, Debug)]
struct AliasSlot
{
   target: EffectGraphTarget,
   width: u32,
   height: u32,
   last_use: u32,
}

#[derive(Default)]
pub struct EffectGraphPlan
{
   effects: Vec<EffectGraphEffect>,
   captures: Vec<EffectGraphCapture>,
   pyramids: Vec<EffectGraphPyramid>,
   resources: Vec<EffectGraphResource>,
   passes: Vec<EffectGraphPass>,
   alias_slots: Vec<AliasSlot>,
   stats: EffectGraphStats,
}

impl EffectGraphPlan
{
   pub fn build(&mut self, events: &[EffectGraphEvent])
   {
      self.effects.clear();
      self.captures.clear();
      self.pyramids.clear();
      self.resources.clear();
      self.passes.clear();
      self.alias_slots.clear();
      let mut active_capture: Option<usize> = None;
      for event in events
      {
         let (source, destination, output, pyramid, extracted) = match event.kind
         {
            EffectGraphEventKind::Write =>
            {
               active_capture = None;
               continue;
            }
            EffectGraphEventKind::Extract { region, downsampled } =>
            {
               active_capture = None;
               if region.is_empty()
               {
                  continue;
               }
               let index = self.captures.len();
               self.captures.push(EffectGraphCapture {
                  kind: if downsampled {
                     EffectGraphCaptureKind::ExtractionDownsample
                  } else {
                     EffectGraphCaptureKind::Extraction
                  },
                  target: event.target,
                  first_command: event.command,
                  last_command: event.command,
                  effect_start: self.effects.len() as u32,
                  effect_count: 0,
                  source: region,
                  destination: region,
                  output: EffectGraphRegion::default(),
                  resource: u32::MAX,
               });
               active_capture = Some(index);
               continue;
            }
            EffectGraphEventKind::Filter { region, output, pyramid } =>
            {
               let Some(index) = active_capture.filter(|index| {
                  let capture = self.captures[*index];
                  capture.target == event.target
                     && capture.kind != EffectGraphCaptureKind::Snapshot
                     && capture.destination.contains(region)
               }) else
               {
                  active_capture = None;
                  continue;
               };
               active_capture = Some(index);
               (region, region, output, pyramid, true)
            }
            EffectGraphEventKind::Effect {
               source,
               destination,
               output,
               pyramid,
            } => (source, destination, output, pyramid, false),
         };
         if source.is_empty() || destination.is_empty() || output.is_empty()
         {
            continue;
         }
         let compatible = extracted || active_capture.is_some_and(|index| {
            let capture = self.captures[index];
            let output_overlap = source.intersects(capture.output)
               && self.effects[capture.effect_start as usize..].iter()
                  .any(|effect| source.intersects(effect.output));
            capture.kind == EffectGraphCaptureKind::Snapshot
               && capture.target == event.target
               && mapping_delta(capture.source, capture.destination)
                  == mapping_delta(source, destination)
               && !output_overlap
         });
         let capture_index = if compatible
         {
            active_capture.unwrap()
         }
         else
         {
            let index = self.captures.len();
            self.captures.push(EffectGraphCapture {
               kind: EffectGraphCaptureKind::Snapshot,
               target: event.target,
               first_command: event.command,
               last_command: event.command,
               effect_start: self.effects.len() as u32,
               effect_count: 0,
               source,
               destination,
               output,
               resource: u32::MAX,
            });
            active_capture = Some(index);
            index
         };
         let pyramid_index = pyramid.materialized.then(|| {
            self.find_or_add_pyramid(
               capture_index,
               event.command,
               if extracted { destination } else { output },
               pyramid,
               extracted,
            )
         });
         self.effects.push(EffectGraphEffect {
            command: event.command,
            capture: capture_index as u32,
            pyramid: pyramid_index.map(|index| index as u32),
            source,
            destination,
            output,
         });
         let capture = &mut self.captures[capture_index];
         capture.last_command = event.command;
         capture.effect_count = capture.effect_count.saturating_add(1);
         capture.source = capture.source.union(source);
         capture.destination = capture.destination.union(destination);
         capture.output = capture.output.union(output);
      }
      self.assign_resources();
      self.assign_passes();
      self.stats = self.collect_stats();
   }

   fn find_or_add_pyramid(
      &mut self,
      capture: usize,
      command: u32,
      region: EffectGraphRegion,
      spec: EffectGraphPyramidSpec,
      contiguous_only: bool,
   ) -> usize
   {
      if let Some(index) = self.pyramids.iter().position(|pyramid| {
         pyramid.capture == capture as u32
            && pyramid.spec == spec
            && (!contiguous_only || pyramid.last_command.saturating_add(1) == command)
      })
      {
         let pyramid = &mut self.pyramids[index];
         pyramid.last_command = command;
         pyramid.effect_count = pyramid.effect_count.saturating_add(1);
         pyramid.region = pyramid.region.union(region);
         return index;
      }
      let index = self.pyramids.len();
      self.pyramids.push(EffectGraphPyramid {
         capture: capture as u32,
         spec,
         first_command: command,
         last_command: command,
         effect_count: 1,
         region,
         resource_start: u32::MAX,
         resource_count: 0,
         resource: u32::MAX,
         scratch_resource: None,
      });
      index
   }

   fn assign_resources(&mut self)
   {
      for capture_index in 0..self.captures.len()
      {
         let mut capture = self.captures[capture_index];
         let resource = self.push_resource(
            EffectGraphResourceKind::Capture,
            capture.target,
            capture.destination,
            capture.first_command,
            capture.last_command,
         );
         self.captures[capture_index].resource = resource;
         capture.resource = resource;
         for pyramid_index in 0..self.pyramids.len()
         {
            let pyramid = self.pyramids[pyramid_index];
            if pyramid.capture != capture_index as u32
            {
               continue;
            }
            let mut level_region = pyramid.region;
            let resource_start = self.resources.len() as u32;
            let extracted = capture.kind != EffectGraphCaptureKind::Snapshot;
            let first_use = if extracted {
               pyramid.first_command
            } else {
               capture.first_command
            };
            let mut final_resource = capture.resource;
            for level in 1..=pyramid.spec.downsample_levels
            {
               level_region = downsampled(level_region);
               final_resource = self.push_resource(
                  EffectGraphResourceKind::PyramidLevel {
                     pyramid: pyramid_index as u32,
                     level,
                  },
                  capture.target,
                  level_region,
                  first_use,
                  pyramid.last_command,
               );
            }
            if pyramid.spec.downsample_levels == 0 && pyramid.spec.blur_passes > 0
            {
               let alias_slot = (extracted
                  && capture.effect_count == 1
                  && pyramid.effect_count == 1
                  && pyramid.spec.blur_passes > 1)
                  .then_some(self.resources[capture.resource as usize].alias_slot);
               final_resource = self.push_resource_with_alias(
                  EffectGraphResourceKind::PyramidLevel {
                     pyramid: pyramid_index as u32,
                     level: 0,
                  },
                  capture.target,
                  level_region,
                  first_use,
                  pyramid.last_command,
                  alias_slot,
               );
            }
            let scratch_resource = if pyramid.spec.blur_passes > 1
            {
               Some(self.push_resource(
                  EffectGraphResourceKind::PyramidScratch {
                     pyramid: pyramid_index as u32,
                  },
                  capture.target,
                  level_region,
                  pyramid.first_command,
                  pyramid.last_command,
               ))
            }
            else
            {
               None
            };
            self.pyramids[pyramid_index].resource_start = resource_start;
            self.pyramids[pyramid_index].resource_count = self.resources.len() as u32
               - resource_start;
            self.pyramids[pyramid_index].resource = final_resource;
            self.pyramids[pyramid_index].scratch_resource = scratch_resource;
         }
      }
   }

   fn push_resource(
      &mut self,
      kind: EffectGraphResourceKind,
      target: EffectGraphTarget,
      region: EffectGraphRegion,
      first_use: u32,
      last_use: u32,
   ) -> u32
   {
      self.push_resource_with_alias(kind, target, region, first_use, last_use, None)
   }

   fn push_resource_with_alias(
      &mut self,
      kind: EffectGraphResourceKind,
      target: EffectGraphTarget,
      region: EffectGraphRegion,
      first_use: u32,
      last_use: u32,
      forced_alias_slot: Option<u32>,
   ) -> u32
   {
      let alias_slot = if let Some(alias_slot) = forced_alias_slot
      {
         alias_slot as usize
      }
      else
      {
         (target.storage == EffectGraphStorage::Transient).then(|| {
            self.alias_slots.iter().enumerate().filter(|(_, slot)| {
               slot.target.format == target.format
                  && slot.target.sample_count == target.sample_count
                  && slot.target.bytes_per_pixel == target.bytes_per_pixel
                  && slot.target.storage == target.storage
                  && slot.last_use < first_use
            }).min_by_key(|(_, slot)| {
               let current = u64::from(slot.width).saturating_mul(u64::from(slot.height));
               let grown = u64::from(slot.width.max(region.w))
                  .saturating_mul(u64::from(slot.height.max(region.h)));
               grown.saturating_sub(current)
            }).map(|(index, _)| index)
         }).flatten().unwrap_or_else(|| {
            self.alias_slots.push(AliasSlot {
               target,
               width: region.w,
               height: region.h,
               last_use,
            });
            self.alias_slots.len() - 1
         })
      };
      let slot = &mut self.alias_slots[alias_slot];
      slot.last_use = slot.last_use.max(last_use);
      slot.width = slot.width.max(region.w);
      slot.height = slot.height.max(region.h);
      let resource = self.resources.len() as u32;
      self.resources.push(EffectGraphResource {
         kind,
         target,
         region,
         first_use,
         last_use,
         alias_slot: alias_slot as u32,
      });
      resource
   }

   fn assign_passes(&mut self)
   {
      for capture_index in 0..self.captures.len()
      {
         let capture = self.captures[capture_index];
         self.passes.push(EffectGraphPass {
            reason: match capture.kind {
               EffectGraphCaptureKind::Snapshot => EffectGraphPassReason::Capture,
               EffectGraphCaptureKind::Extraction => EffectGraphPassReason::Extract,
               EffectGraphCaptureKind::ExtractionDownsample => {
                  EffectGraphPassReason::ExtractDownsample
               }
            },
            target: capture.target,
            region: capture.destination,
            first_command: capture.first_command,
            last_command: capture.last_command,
            read_resource: None,
            write_resource: Some(capture.resource),
            load: if capture.kind == EffectGraphCaptureKind::Snapshot {
               EffectGraphLoadAction::DontCare
            } else {
               EffectGraphLoadAction::Clear
            },
            store: EffectGraphStoreAction::Store,
         });
         if capture.kind == EffectGraphCaptureKind::Snapshot
         {
            for pyramid_index in 0..self.pyramids.len()
            {
               let pyramid = self.pyramids[pyramid_index];
               if pyramid.capture == capture_index as u32
               {
                  self.push_pyramid_passes(capture, pyramid);
               }
            }
            for effect_index in 0..self.effects.len()
            {
               let effect = self.effects[effect_index];
               if effect.capture == capture_index as u32
               {
                  self.push_composite_pass(capture, effect);
               }
            }
         }
         else
         {
            let mut emitted_pyramid = None;
            for effect_index in 0..self.effects.len()
            {
               let effect = self.effects[effect_index];
               if effect.capture != capture_index as u32
               {
                  continue;
               }
               if effect.pyramid != emitted_pyramid
               {
                  if let Some(pyramid) = effect.pyramid
                  {
                     self.push_pyramid_passes(capture, self.pyramids[pyramid as usize]);
                  }
                  emitted_pyramid = effect.pyramid;
               }
               self.push_composite_pass(capture, effect);
            }
         }
      }
   }

   fn push_pyramid_passes(
      &mut self,
      capture: EffectGraphCapture,
      pyramid: EffectGraphPyramid,
   )
   {
      let mut read_resource = capture.resource;
      for level in 1..=pyramid.spec.downsample_levels
      {
         let write_resource = pyramid.resource_start + u32::from(level) - 1;
         self.passes.push(EffectGraphPass {
            reason: EffectGraphPassReason::Downsample,
            target: capture.target,
            region: pyramid.region,
            first_command: pyramid.first_command,
            last_command: pyramid.last_command,
            read_resource: Some(read_resource),
            write_resource: Some(write_resource),
            load: EffectGraphLoadAction::DontCare,
            store: EffectGraphStoreAction::Store,
         });
         read_resource = write_resource;
      }
      if pyramid.spec.blur_passes == 0
      {
         return;
      }
      let scratch = pyramid.scratch_resource.unwrap_or(pyramid.resource);
      self.passes.push(EffectGraphPass {
         reason: EffectGraphPassReason::BlurHorizontal,
         target: capture.target,
         region: pyramid.region,
         first_command: pyramid.first_command,
         last_command: pyramid.last_command,
         read_resource: Some(read_resource),
         write_resource: Some(scratch),
         load: EffectGraphLoadAction::DontCare,
         store: EffectGraphStoreAction::Store,
      });
      if pyramid.spec.blur_passes > 1
      {
         self.passes.push(EffectGraphPass {
            reason: EffectGraphPassReason::BlurVertical,
            target: capture.target,
            region: pyramid.region,
            first_command: pyramid.first_command,
            last_command: pyramid.last_command,
            read_resource: Some(scratch),
            write_resource: Some(pyramid.resource),
            load: EffectGraphLoadAction::DontCare,
            store: EffectGraphStoreAction::Store,
         });
      }
   }

   fn push_composite_pass(
      &mut self,
      capture: EffectGraphCapture,
      effect: EffectGraphEffect,
   )
   {
      let read_resource = effect.pyramid
         .map_or(capture.resource, |pyramid| self.pyramids[pyramid as usize].resource);
      self.passes.push(EffectGraphPass {
         reason: if capture.kind == EffectGraphCaptureKind::ExtractionDownsample {
            EffectGraphPassReason::UpsampleComposite
         } else {
            EffectGraphPassReason::Composite
         },
         target: capture.target,
         region: effect.output,
         first_command: effect.command,
         last_command: effect.command,
         read_resource: Some(read_resource),
         write_resource: None,
         load: EffectGraphLoadAction::Load,
         store: EffectGraphStoreAction::Store,
      });
   }

   pub fn effects(&self) -> &[EffectGraphEffect]
   {
      &self.effects
   }

   pub fn captures(&self) -> &[EffectGraphCapture]
   {
      &self.captures
   }

   pub fn pyramids(&self) -> &[EffectGraphPyramid]
   {
      &self.pyramids
   }

   pub fn resources(&self) -> &[EffectGraphResource]
   {
      &self.resources
   }

   pub fn passes(&self) -> &[EffectGraphPass]
   {
      &self.passes
   }

   pub fn alias_slot_count(&self) -> usize
   {
      self.alias_slots.len()
   }

   pub fn scratch_capacity_bytes(&self) -> usize
   {
      self.effects.capacity().saturating_mul(core::mem::size_of::<EffectGraphEffect>())
         .saturating_add(
            self.captures.capacity().saturating_mul(core::mem::size_of::<EffectGraphCapture>()),
         )
         .saturating_add(
            self.pyramids.capacity().saturating_mul(core::mem::size_of::<EffectGraphPyramid>()),
         )
         .saturating_add(
            self.resources.capacity().saturating_mul(core::mem::size_of::<EffectGraphResource>()),
         )
         .saturating_add(
            self.passes.capacity().saturating_mul(core::mem::size_of::<EffectGraphPass>()),
         )
         .saturating_add(
            self.alias_slots.capacity().saturating_mul(core::mem::size_of::<AliasSlot>()),
         )
   }

   fn collect_stats(&self) -> EffectGraphStats
   {
      let logical_bytes = self.resources.iter().fold(0_u64, |bytes, resource| {
         bytes.saturating_add(resource.region.pixels().saturating_mul(
            u64::from(resource.target.bytes_per_pixel),
         ))
      });
      let physical_bytes = self.alias_slots.iter().fold(0_u64, |bytes, slot| {
         bytes.saturating_add(
            u64::from(slot.width)
               .saturating_mul(u64::from(slot.height))
               .saturating_mul(u64::from(slot.target.bytes_per_pixel)),
         )
      });
      let mut capture_passes = 0_u32;
      let mut extract_passes = 0_u32;
      let mut downsample_passes = 0_u32;
      let mut blur_horizontal_passes = 0_u32;
      let mut blur_vertical_passes = 0_u32;
      let mut upsample_passes = 0_u32;
      let mut composite_passes = 0_u32;
      for pass in &self.passes
      {
         match pass.reason
         {
            EffectGraphPassReason::Capture => capture_passes += 1,
            EffectGraphPassReason::Extract => extract_passes += 1,
            EffectGraphPassReason::ExtractDownsample => {
               extract_passes += 1;
               downsample_passes += 1;
            }
            EffectGraphPassReason::Downsample => downsample_passes += 1,
            EffectGraphPassReason::BlurHorizontal => blur_horizontal_passes += 1,
            EffectGraphPassReason::BlurVertical => blur_vertical_passes += 1,
            EffectGraphPassReason::Composite => composite_passes += 1,
            EffectGraphPassReason::UpsampleComposite => {
               upsample_passes += 1;
               composite_passes += 1;
            }
         }
      }
      let max_lifetime_commands = self.resources.iter().map(|resource| {
         resource.last_use.saturating_sub(resource.first_use).saturating_add(1)
      }).max().unwrap_or(0);
      EffectGraphStats {
         effects: self.effects.len() as u32,
         captures: self.captures.len() as u32,
         pyramids: self.pyramids.len() as u32,
         pyramid_reuses: self.effects.iter().filter(|effect| {
            effect.pyramid.is_some_and(|pyramid| {
               self.pyramids[pyramid as usize].first_command != effect.command
            })
         }).count() as u32,
         capture_passes,
         extract_passes,
         downsample_passes,
         blur_horizontal_passes,
         blur_vertical_passes,
         upsample_passes,
         composite_passes,
         max_lifetime_commands,
         resources: self.resources.len() as u32,
         alias_slots: self.alias_slots.len() as u32,
         logical_bytes,
         physical_bytes,
         aliased_bytes: logical_bytes.saturating_sub(physical_bytes),
      }
   }

   pub const fn stats(&self) -> EffectGraphStats
   {
      self.stats
   }

}

fn downsampled(region: EffectGraphRegion) -> EffectGraphRegion
{
   EffectGraphRegion::new(
      region.x / 2,
      region.y / 2,
      region.w.saturating_add(1) / 2,
      region.h.saturating_add(1) / 2,
   )
}

fn mapping_delta(source: EffectGraphRegion, destination: EffectGraphRegion) -> (i64, i64)
{
   (
      i64::from(destination.x) - i64::from(source.x),
      i64::from(destination.y) - i64::from(source.y),
   )
}

#[cfg(test)]
mod tests
{
   use super::*;

   fn target(id: u64) -> EffectGraphTarget
   {
      EffectGraphTarget {
         id,
         format: 1,
         sample_count: 1,
         bytes_per_pixel: 4,
         storage: EffectGraphStorage::Transient,
      }
   }

   fn effect(command: u32, x: u32, sigma: f32) -> EffectGraphEvent
   {
      let region = EffectGraphRegion::new(x, 0, 20, 20);
      EffectGraphEvent {
         command,
         target: target(0),
         kind: EffectGraphEventKind::Effect {
            source: region,
            destination: region,
            output: region,
            pyramid: EffectGraphPyramidSpec {
               sigma_bits: sigma.to_bits(),
               quality: 1,
               downsample_levels: 2,
               blur_passes: 2,
               materialized: true,
            },
         },
      }
   }

   fn extracted_filter(command: u32, sigma: f32) -> EffectGraphEvent
   {
      EffectGraphEvent {
         command,
         target: target(0),
         kind: EffectGraphEventKind::Filter {
            region: EffectGraphRegion::new(0, 0, 100, 100),
            output: EffectGraphRegion::new(0, 0, 200, 200),
            pyramid: EffectGraphPyramidSpec {
               sigma_bits: sigma.to_bits(),
               quality: 1,
               downsample_levels: 0,
               blur_passes: 2,
               materialized: true,
            },
         },
      }
   }

   #[test]
   fn terminal_extracted_filter_aliases_output_back_to_source()
   {
      let mut plan = EffectGraphPlan::default();
      plan.build(&[
         EffectGraphEvent {
            command: 0,
            target: target(0),
            kind: EffectGraphEventKind::Extract {
               region: EffectGraphRegion::new(0, 0, 100, 100),
               downsampled: true,
            },
         },
         extracted_filter(1, 8.0),
      ]);
      assert_eq!(plan.captures()[0].kind, EffectGraphCaptureKind::ExtractionDownsample);
      assert_eq!(plan.resources().len(), 3);
      assert_eq!(plan.alias_slot_count(), 2);
      assert_eq!(
         plan.passes().iter().map(|pass| pass.reason).collect::<Vec<_>>(),
         vec![
            EffectGraphPassReason::ExtractDownsample,
            EffectGraphPassReason::BlurHorizontal,
            EffectGraphPassReason::BlurVertical,
            EffectGraphPassReason::UpsampleComposite,
         ],
      );
      let stats = plan.stats();
      assert_eq!((stats.extract_passes, stats.downsample_passes), (1, 1));
      assert_eq!((stats.blur_horizontal_passes, stats.blur_vertical_passes), (1, 1));
      assert_eq!((stats.upsample_passes, stats.composite_passes), (1, 1));
      assert_eq!((stats.resources, stats.alias_slots), (3, 2));
      assert_eq!((stats.logical_bytes, stats.physical_bytes, stats.aliased_bytes), (120_000, 80_000, 40_000));
   }

   #[test]
   fn extracted_filter_must_stay_inside_the_extracted_region()
   {
      let mut plan = EffectGraphPlan::default();
      let mut outside = extracted_filter(1, 8.0);
      outside.kind = EffectGraphEventKind::Filter {
         region: EffectGraphRegion::new(50, 0, 75, 100),
         output: EffectGraphRegion::new(0, 0, 200, 200),
         pyramid: EffectGraphPyramidSpec {
            sigma_bits: 8.0_f32.to_bits(),
            quality: 1,
            downsample_levels: 0,
            blur_passes: 2,
            materialized: true,
         },
      };
      plan.build(&[
         EffectGraphEvent {
            command: 0,
            target: target(0),
            kind: EffectGraphEventKind::Extract {
               region: EffectGraphRegion::new(0, 0, 100, 100),
               downsampled: true,
            },
         },
         outside,
      ]);
      assert!(plan.effects().is_empty());
      assert_eq!(plan.passes().len(), 1);
   }

   #[test]
   fn extracted_multi_filter_graph_preserves_source_and_aliases_layer_intermediates()
   {
      let mut plan = EffectGraphPlan::default();
      plan.build(&[
         EffectGraphEvent {
            command: 0,
            target: target(0),
            kind: EffectGraphEventKind::Extract {
               region: EffectGraphRegion::new(0, 0, 100, 100),
               downsampled: true,
            },
         },
         extracted_filter(1, 3.0),
         extracted_filter(2, 8.0),
         extracted_filter(3, 16.0),
      ]);
      assert_eq!((plan.effects().len(), plan.pyramids().len()), (3, 3));
      assert_eq!(plan.resources().len(), 7);
      assert_eq!(plan.alias_slot_count(), 3);
      assert_eq!(plan.passes().len(), 10);
      let stats = plan.stats();
      assert_eq!((stats.extract_passes, stats.downsample_passes), (1, 1));
      assert_eq!((stats.blur_horizontal_passes, stats.blur_vertical_passes), (3, 3));
      assert_eq!((stats.upsample_passes, stats.composite_passes), (3, 3));
      assert_eq!((stats.resources, stats.alias_slots), (7, 3));
      assert!(stats.aliased_bytes > 0);
   }

   #[test]
   fn adjacent_disjoint_effects_share_capture_and_matching_pyramid()
   {
      let mut plan = EffectGraphPlan::default();
      plan.build(&[effect(1, 0, 12.0), effect(2, 40, 12.0)]);
      assert_eq!(plan.captures().len(), 1);
      assert_eq!(plan.captures()[0].effect_count, 2);
      assert_eq!(plan.pyramids().len(), 1);
      assert_eq!(plan.pyramids()[0].effect_count, 2);
      assert_eq!(plan.resources().len(), 4);
      assert_eq!(plan.passes().len(), 7);
      assert_eq!(plan.alias_slot_count(), 4);
   }

   #[test]
   fn intervening_write_overlap_and_target_change_split_epochs()
   {
      let mut events = vec![effect(1, 0, 12.0), effect(2, 10, 12.0)];
      events.push(EffectGraphEvent {
         command: 3,
         target: target(0),
         kind: EffectGraphEventKind::Write,
      });
      events.push(effect(4, 60, 12.0));
      let mut other = effect(5, 90, 12.0);
      other.target = target(8);
      events.push(other);
      let mut plan = EffectGraphPlan::default();
      plan.build(&events);
      assert_eq!(plan.captures().len(), 4);
      assert_eq!(plan.effects().iter().map(|effect| effect.capture).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
   }

   #[test]
   fn mixed_sigma_shares_capture_but_not_pyramid_and_later_epochs_alias()
   {
      let events = [
         effect(1, 0, 8.0),
         effect(2, 40, 16.0),
         EffectGraphEvent { command: 3, target: target(0), kind: EffectGraphEventKind::Write },
         effect(4, 80, 8.0),
      ];
      let mut plan = EffectGraphPlan::default();
      plan.build(&events);
      assert_eq!(plan.captures().len(), 2);
      assert_eq!(plan.pyramids().len(), 3);
      assert_eq!(plan.resources().len(), 11);
      assert!(plan.alias_slot_count() < plan.resources().len());
      assert!(plan.stats().aliased_bytes > 0);
   }

   #[test]
   fn persistent_resources_do_not_alias_across_lifetimes()
   {
      let mut first = effect(1, 0, 12.0);
      first.target.storage = EffectGraphStorage::Persistent;
      let mut second = effect(3, 40, 12.0);
      second.target.storage = EffectGraphStorage::Persistent;
      let mut plan = EffectGraphPlan::default();
      plan.build(&[
         first,
         EffectGraphEvent {
            command: 2,
            target: target(0),
            kind: EffectGraphEventKind::Write,
         },
         second,
      ]);
      assert_eq!(plan.resources().len(), plan.alias_slot_count());
      assert_eq!(plan.stats().aliased_bytes, 0);
   }

   #[test]
   fn later_larger_transient_resources_grow_existing_alias_slots()
   {
      let mut later = effect(3, 40, 12.0);
      let region = EffectGraphRegion::new(40, 0, 80, 80);
      later.kind = EffectGraphEventKind::Effect {
         source: region,
         destination: region,
         output: region,
         pyramid: EffectGraphPyramidSpec {
            sigma_bits: 12.0_f32.to_bits(),
            quality: 1,
            downsample_levels: 2,
            blur_passes: 2,
            materialized: true,
         },
      };
      let mut plan = EffectGraphPlan::default();
      plan.build(&[
         effect(1, 0, 12.0),
         EffectGraphEvent {
            command: 2,
            target: target(0),
            kind: EffectGraphEventKind::Write,
         },
         later,
      ]);
      assert_eq!(plan.resources().len(), 8);
      assert_eq!(plan.alias_slot_count(), 4);
      assert!(plan.stats().aliased_bytes > 0);
   }
}
