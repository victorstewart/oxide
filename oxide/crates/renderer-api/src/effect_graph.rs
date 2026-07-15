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
pub struct EffectGraphCapture
{
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
   Downsample,
   BlurHorizontal,
   BlurVertical,
   Composite,
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
   pub downsample_passes: u32,
   pub blur_horizontal_passes: u32,
   pub blur_vertical_passes: u32,
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
         let EffectGraphEventKind::Effect {
            source,
            destination,
            output,
            pyramid,
         } = event.kind else
         {
            active_capture = None;
            continue;
         };
         if source.is_empty() || destination.is_empty() || output.is_empty()
         {
            continue;
         }
         let compatible = active_capture.is_some_and(|index| {
            let capture = self.captures[index];
            let output_overlap = source.intersects(capture.output)
               && self.effects[capture.effect_start as usize..].iter()
                  .any(|effect| source.intersects(effect.output));
            capture.target == event.target
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
            self.find_or_add_pyramid(capture_index, event.command, output, pyramid)
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
   ) -> usize
   {
      if let Some(index) = self.pyramids.iter().position(|pyramid| {
         pyramid.capture == capture as u32 && pyramid.spec == spec
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
         let capture = self.captures[capture_index];
         let resource = self.push_resource(
            EffectGraphResourceKind::Capture,
            capture.target,
            capture.destination,
            capture.first_command,
            capture.last_command,
         );
         self.captures[capture_index].resource = resource;
         for pyramid_index in 0..self.pyramids.len()
         {
            let pyramid = self.pyramids[pyramid_index];
            if pyramid.capture != capture_index as u32
            {
               continue;
            }
            let mut level_region = pyramid.region;
            let mut final_resource = u32::MAX;
            let resource_start = self.resources.len() as u32;
            for level in 1..=pyramid.spec.downsample_levels.max(1)
            {
               level_region = downsampled(level_region);
               final_resource = self.push_resource(
                  EffectGraphResourceKind::PyramidLevel {
                     pyramid: pyramid_index as u32,
                     level,
                  },
                  capture.target,
                  level_region,
                  capture.first_command,
                  pyramid.last_command,
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
      let alias_slot = (target.storage == EffectGraphStorage::Transient).then(|| {
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
      });
      let slot = &mut self.alias_slots[alias_slot];
      slot.last_use = last_use;
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
      for (capture_index, capture) in self.captures.iter().enumerate()
      {
         self.passes.push(EffectGraphPass {
            reason: EffectGraphPassReason::Capture,
            target: capture.target,
            region: capture.destination,
            first_command: capture.first_command,
            last_command: capture.last_command,
            read_resource: None,
            write_resource: Some(capture.resource),
            load: EffectGraphLoadAction::DontCare,
            store: EffectGraphStoreAction::Store,
         });
         for pyramid in self.pyramids.iter()
            .filter(|pyramid| pyramid.capture == capture_index as u32)
         {
            let mut read_resource = capture.resource;
            for level in 1..=pyramid.spec.downsample_levels.max(1)
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
            if pyramid.spec.blur_passes > 0
            {
               let scratch = pyramid.scratch_resource.unwrap_or(pyramid.resource);
               self.passes.push(EffectGraphPass {
                  reason: EffectGraphPassReason::BlurHorizontal,
                  target: capture.target,
                  region: pyramid.region,
                  first_command: pyramid.first_command,
                  last_command: pyramid.last_command,
                  read_resource: Some(pyramid.resource),
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
         }
         for effect in self.effects.iter().filter(|effect| {
            effect.capture == capture_index as u32
         })
         {
            let read_resource = effect.pyramid
               .map_or(capture.resource, |pyramid| self.pyramids[pyramid as usize].resource);
            self.passes.push(EffectGraphPass {
               reason: EffectGraphPassReason::Composite,
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
      }
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
      let mut downsample_passes = 0_u32;
      let mut blur_horizontal_passes = 0_u32;
      let mut blur_vertical_passes = 0_u32;
      let mut composite_passes = 0_u32;
      for pass in &self.passes
      {
         match pass.reason
         {
            EffectGraphPassReason::Capture => capture_passes += 1,
            EffectGraphPassReason::Downsample => downsample_passes += 1,
            EffectGraphPassReason::BlurHorizontal => blur_horizontal_passes += 1,
            EffectGraphPassReason::BlurVertical => blur_vertical_passes += 1,
            EffectGraphPassReason::Composite => composite_passes += 1,
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
         downsample_passes,
         blur_horizontal_passes,
         blur_vertical_passes,
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
