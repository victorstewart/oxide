//! Overlay and popup window management for OxideUI surfaces.

use crate::{DrawListBuilder, NodeId, UiSurface};
use alloc::vec::Vec;
use core::cmp::Ordering;
use oxideui_renderer_api as gfx;
use oxideui_timing as timing;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct OverlayHandle(pub u64);

#[derive(Clone, Copy, Debug)]
pub struct OverlayVisual
{
   pub blur_sigma: f32,
   pub tint: gfx::Color,
   pub alpha: f32,
   pub z_index: i32,
}

impl Default for OverlayVisual
{
   fn default() -> Self
   {
      Self { blur_sigma: 18.0, tint: gfx::Color::rgba(0.0, 0.0, 0.0, 1.0), alpha: 0.45, z_index: 0 }
   }
}

#[derive(Clone, Copy, Debug)]
pub struct OverlayBehavior
{
   pub dismiss_on_background_tap: bool,
   pub block_underlying_inputs: bool,
   pub content_root: Option<NodeId>,
   pub focus_root: Option<NodeId>,
}

impl Default for OverlayBehavior
{
   fn default() -> Self
   {
      Self
      {
         dismiss_on_background_tap: false,
         block_underlying_inputs: true,
         content_root: None,
         focus_root: None,
      }
   }
}

pub enum OverlayPointerResult
{
   Consumed { handle: OverlayHandle, node: Option<(NodeId, [f32; 2])> },
   Dismissed { handle: OverlayHandle },
   Ignored,
}

struct OverlayEntry
{
   id: OverlayHandle,
   surface: UiSurface,
   visual: OverlayVisual,
   behavior: OverlayBehavior,
   last_tick: u64,
}

impl OverlayEntry
{
   fn new(id: OverlayHandle, surface: UiSurface, visual: OverlayVisual, behavior: OverlayBehavior) -> Self
   {
      Self { id, surface, visual, behavior, last_tick: 0 }
   }
}

pub struct OverlayStack
{
   entries: Vec<OverlayEntry>,
   next_id: u64,
   viewport: gfx::RectF,
   device_scale: f32,
}

impl OverlayStack
{
   pub fn new() -> Self
   {
      Self
      {
         entries: Vec::new(),
         next_id: 1,
         viewport: gfx::RectF::new(0.0, 0.0, 0.0, 0.0),
         device_scale: 1.0,
      }
   }

   pub fn is_empty(&self) -> bool
   {
      self.entries.is_empty()
   }

   pub fn set_viewport(&mut self, viewport: gfx::RectF, device_scale: f32)
   {
      self.viewport = viewport;
      self.device_scale = device_scale;
      for entry in &mut self.entries
      {
         entry.surface.layout(viewport.w, viewport.h);
      }
   }

   pub fn push(&mut self, mut surface: UiSurface, visual: OverlayVisual, behavior: OverlayBehavior) -> OverlayHandle
   {
      let id = OverlayHandle(self.next_id);
      self.next_id = self.next_id.wrapping_add(1).max(1);
      surface.layout(self.viewport.w, self.viewport.h);
      let mut entry = OverlayEntry::new(id, surface, visual, behavior);
      entry.last_tick = timing::now_ms();
      self.entries.push(entry);
      self.entries.sort_by(|a, b| a.visual.z_index.cmp(&b.visual.z_index));
      id
   }

   pub fn remove(&mut self, handle: OverlayHandle) -> Option<UiSurface>
   {
      if let Some(pos) = self.entries.iter().position(|entry| entry.id == handle)
      {
         Some(self.entries.remove(pos).surface)
      }
      else
      {
         None
      }
   }

   pub fn top_handle(&self) -> Option<OverlayHandle>
   {
      self.entries.last().map(|entry| entry.id)
   }

   pub fn surface_mut(&mut self, handle: OverlayHandle) -> Option<&mut UiSurface>
   {
      self.entries.iter_mut().find(|entry| entry.id == handle).map(|entry| &mut entry.surface)
   }

   pub fn focus_target(&self) -> Option<NodeId>
   {
      self.entries.last().and_then(|entry| entry.behavior.focus_root)
   }

   pub fn pointer_event(&mut self, x: f32, y: f32, buttons: u32) -> OverlayPointerResult
   {
      let Some(entry) = self.entries.last_mut() else { return OverlayPointerResult::Ignored };
      let hit = entry.surface.hit_test(x, y);
      let mut inside_content = hit.is_some();
      if let (Some(content_root), Some((hit_node, _))) = (entry.behavior.content_root, hit)
      {
         inside_content = entry.surface.tree().is_descendant(hit_node, content_root);
      }
      if buttons == 0 && entry.behavior.dismiss_on_background_tap && !inside_content
      {
         let handle = entry.id;
         let _ = self.entries.pop();
         return OverlayPointerResult::Dismissed { handle };
      }
      if entry.behavior.block_underlying_inputs || inside_content
      {
         OverlayPointerResult::Consumed { handle: entry.id, node: hit }
      }
      else
      {
         OverlayPointerResult::Ignored
      }
   }

   pub fn tick_at(&mut self, now_ms: u64) -> bool
   {
      let mut changed = false;
      for entry in &mut self.entries
      {
         if entry.surface.tick_at(now_ms)
         {
            changed = true;
         }
         entry.last_tick = now_ms;
      }
      changed
   }

  pub fn tick(&mut self) -> bool
   {
      let now = timing::now_ms();
      self.tick_at(now)
   }

   pub fn encode(&self, builder: &mut DrawListBuilder)
   {
      for entry in self.sorted_entries()
      {
         builder.backdrop(
            self.viewport,
            entry.visual.blur_sigma,
            entry.visual.tint,
            entry.visual.alpha,
         );
         entry.surface.encode(builder);
      }
   }

   pub fn capture(&self) -> gfx::DrawList
   {
      let mut builder = DrawListBuilder::new();
      self.encode(&mut builder);
      builder.into_inner()
   }

   fn sorted_entries(&self) -> impl Iterator<Item = &OverlayEntry>
   {
      let mut refs: Vec<&OverlayEntry> = self.entries.iter().collect();
      refs.sort_by(|a, b| match a.visual.z_index.cmp(&b.visual.z_index)
      {
         Ordering::Equal => a.id.0.cmp(&b.id.0),
         other => other,
      });
      refs.into_iter()
   }
}

#[derive(Clone, Copy, Debug)]
pub struct PopupSpec
{
   pub visual: OverlayVisual,
   pub behavior: OverlayBehavior,
}

impl Default for PopupSpec
{
   fn default() -> Self
   {
      Self { visual: OverlayVisual::default(), behavior: OverlayBehavior::default() }
   }
}

pub type PopupHandle = OverlayHandle;

pub struct PopupManager
{
   stack: OverlayStack,
}

impl PopupManager
{
   pub fn new() -> Self
   {
      Self { stack: OverlayStack::new() }
   }

   pub fn set_viewport(&mut self, viewport: gfx::RectF, device_scale: f32)
   {
      self.stack.set_viewport(viewport, device_scale);
   }

   pub fn push(&mut self, surface: UiSurface, spec: PopupSpec) -> PopupHandle
   {
      self.stack.push(surface, spec.visual, spec.behavior)
   }

   pub fn remove(&mut self, handle: PopupHandle) -> Option<UiSurface>
   {
      self.stack.remove(handle)
   }

   pub fn pointer_event(&mut self, x: f32, y: f32, buttons: u32) -> OverlayPointerResult
   {
      self.stack.pointer_event(x, y, buttons)
   }

   pub fn tick_at(&mut self, now_ms: u64) -> bool
   {
      self.stack.tick_at(now_ms)
   }

   pub fn tick(&mut self) -> bool
   {
      self.stack.tick()
   }

   pub fn encode(&self, builder: &mut DrawListBuilder)
   {
      self.stack.encode(builder);
   }

   pub fn focus_target(&self) -> Option<NodeId>
   {
      self.stack.focus_target()
   }

   pub fn is_empty(&self) -> bool
   {
      self.stack.is_empty()
   }

   pub fn capture(&self) -> gfx::DrawList
   {
      self.stack.capture()
   }

   pub fn surface_mut(&mut self, handle: PopupHandle) -> Option<&mut UiSurface>
   {
      self.stack.surface_mut(handle)
   }
}
