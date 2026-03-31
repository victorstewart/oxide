//! Overlay and popup window management for Oxide surfaces.

use crate::{DrawListBuilder, LayoutRect, NodeId, UiSurface};
use alloc::boxed::Box;
use alloc::vec::Vec;
use oxide_renderer_api as gfx;
use oxide_timing as timing;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct OverlayHandle(pub u64);

#[derive(Clone, Copy, Debug)]
pub struct OverlayVisual {
    pub blur_sigma: f32,
    pub tint: gfx::Color,
    pub alpha: f32,
    pub z_index: i32,
}

impl Default for OverlayVisual {
    fn default() -> Self {
        Self {
            blur_sigma: 18.0,
            tint: gfx::Color::rgba(0.0, 0.0, 0.0, 1.0),
            alpha: 0.45,
            z_index: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OverlayBehavior {
    pub dismiss_on_background_tap: bool,
    pub block_underlying_inputs: bool,
    pub content_root: Option<NodeId>,
    pub focus_root: Option<NodeId>,
}

impl Default for OverlayBehavior {
    fn default() -> Self {
        Self {
            dismiss_on_background_tap: false,
            block_underlying_inputs: true,
            content_root: None,
            focus_root: None,
        }
    }
}

pub enum OverlayPointerResult {
    Consumed { handle: OverlayHandle, node: Option<(NodeId, [f32; 2])> },
    Dismissed { handle: OverlayHandle },
    Ignored,
}

struct OverlayEntry {
    id: OverlayHandle,
    surface: UiSurface,
    visual: OverlayVisual,
    behavior: OverlayBehavior,
    last_tick: u64,
}

impl OverlayEntry {
    fn new(
        id: OverlayHandle,
        surface: UiSurface,
        visual: OverlayVisual,
        behavior: OverlayBehavior,
    ) -> Self {
        Self { id, surface, visual, behavior, last_tick: 0 }
    }
}

pub struct OverlayStack {
    entries: Vec<OverlayEntry>,
    next_id: u64,
    viewport: gfx::RectF,
    device_scale: f32,
}

impl OverlayStack {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
            viewport: gfx::RectF::new(0.0, 0.0, 0.0, 0.0),
            device_scale: 1.0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn set_viewport(&mut self, viewport: gfx::RectF, device_scale: f32) {
        self.viewport = viewport;
        self.device_scale = device_scale;
        for entry in &mut self.entries {
            entry.surface.layout(viewport.w, viewport.h);
        }
    }

    pub fn push(
        &mut self,
        mut surface: UiSurface,
        visual: OverlayVisual,
        behavior: OverlayBehavior,
    ) -> OverlayHandle {
        let id = OverlayHandle(self.next_id);
        self.next_id = self.next_id.wrapping_add(1).max(1);
        surface.layout(self.viewport.w, self.viewport.h);
        let mut entry = OverlayEntry::new(id, surface, visual, behavior);
        entry.last_tick = timing::now_ms();
        self.entries.push(entry);
        self.entries.sort_by(|a, b| {
            a.visual.z_index.cmp(&b.visual.z_index).then_with(|| a.id.0.cmp(&b.id.0))
        });
        id
    }

    pub fn remove(&mut self, handle: OverlayHandle) -> Option<UiSurface> {
        if let Some(pos) = self.entries.iter().position(|entry| entry.id == handle) {
            Some(self.entries.remove(pos).surface)
        } else {
            None
        }
    }

    pub fn top_handle(&self) -> Option<OverlayHandle> {
        self.entries.last().map(|entry| entry.id)
    }

    pub fn surface_mut(&mut self, handle: OverlayHandle) -> Option<&mut UiSurface> {
        self.entries.iter_mut().find(|entry| entry.id == handle).map(|entry| &mut entry.surface)
    }

    pub fn focus_target(&self) -> Option<NodeId> {
        self.entries.last().and_then(|entry| entry.behavior.focus_root)
    }

    pub fn pointer_event(&mut self, x: f32, y: f32, buttons: u32) -> OverlayPointerResult {
        let Some(entry) = self.entries.last_mut() else { return OverlayPointerResult::Ignored };
        let hit = entry.surface.hit_test(x, y);
        let mut inside_content = hit.is_some();
        if let (Some(content_root), Some((hit_node, _))) = (entry.behavior.content_root, hit) {
            inside_content = entry.surface.tree().is_descendant(hit_node, content_root);
        }
        if buttons == 0 && entry.behavior.dismiss_on_background_tap && !inside_content {
            let handle = entry.id;
            let _ = self.entries.pop();
            return OverlayPointerResult::Dismissed { handle };
        }
        if entry.behavior.block_underlying_inputs || inside_content {
            OverlayPointerResult::Consumed { handle: entry.id, node: hit }
        } else {
            OverlayPointerResult::Ignored
        }
    }

    pub fn tick_at(&mut self, now_ms: u64) -> bool {
        let mut changed = false;
        for entry in &mut self.entries {
            if entry.surface.tick_at(now_ms) {
                changed = true;
            }
            entry.last_tick = now_ms;
        }
        changed
    }

    pub fn tick(&mut self) -> bool {
        let now = timing::now_ms();
        self.tick_at(now)
    }

    pub fn encode(&self, builder: &mut DrawListBuilder) {
        for entry in self.sorted_entries() {
            builder.backdrop(
                self.viewport,
                entry.visual.blur_sigma,
                entry.visual.tint,
                entry.visual.alpha,
            );
            entry.surface.encode(builder);
        }
    }

    pub fn capture(&self) -> gfx::DrawList {
        let mut builder = DrawListBuilder::new();
        self.encode(&mut builder);
        builder.into_inner()
    }

    fn sorted_entries(&self) -> impl Iterator<Item = &OverlayEntry> {
        let mut refs: Vec<&OverlayEntry> = self.entries.iter().collect();
        refs.sort_by(|a, b| {
            a.visual.z_index.cmp(&b.visual.z_index).then_with(|| a.id.0.cmp(&b.id.0))
        });
        refs.into_iter()
    }
}

fn layout_rect_to_rect(rect: LayoutRect) -> gfx::RectF {
    gfx::RectF::new(rect.x, rect.y, rect.w, rect.h)
}

fn rect_contains_point(rect: gfx::RectF, x: f32, y: f32) -> bool {
    x >= rect.x && y >= rect.y && x <= rect.x + rect.w && y <= rect.y + rect.h
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PopupTouchRegion {
    None,
    ContentRoot,
    Rect(gfx::RectF),
}

impl Default for PopupTouchRegion {
    fn default() -> Self {
        Self::ContentRoot
    }
}

pub type PopupApproveDismissal = Box<dyn FnMut(&mut UiSurface) -> bool>;
pub type PopupDismissal = Box<dyn FnMut(&mut UiSurface)>;
pub type PopupApproveTouch = Box<dyn FnMut(&mut UiSurface, [f32; 2]) -> bool>;

pub struct PopupCallbacks {
    pub approve_dismissal: Option<PopupApproveDismissal>,
    pub dismissal: Option<PopupDismissal>,
    pub approve_touch: Option<PopupApproveTouch>,
}

impl Default for PopupCallbacks {
    fn default() -> Self {
        Self { approve_dismissal: None, dismissal: None, approve_touch: None }
    }
}

pub struct PopupSpec {
    pub visual: OverlayVisual,
    pub behavior: OverlayBehavior,
    pub touch_region: PopupTouchRegion,
    pub callbacks: PopupCallbacks,
}

impl Default for PopupSpec {
    fn default() -> Self {
        Self {
            visual: OverlayVisual::default(),
            behavior: OverlayBehavior::default(),
            touch_region: PopupTouchRegion::default(),
            callbacks: PopupCallbacks::default(),
        }
    }
}

pub type PopupHandle = OverlayHandle;

struct PopupEntry {
    id: PopupHandle,
    surface: UiSurface,
    visual: OverlayVisual,
    behavior: OverlayBehavior,
    touch_region: PopupTouchRegion,
    touch_exception: Option<gfx::RectF>,
    callbacks: PopupCallbacks,
    last_tick: u64,
}

impl PopupEntry {
    fn new(id: PopupHandle, mut surface: UiSurface, spec: PopupSpec, viewport: gfx::RectF) -> Self {
        surface.layout(viewport.w, viewport.h);
        let mut entry = Self {
            id,
            surface,
            visual: spec.visual,
            behavior: spec.behavior,
            touch_region: spec.touch_region,
            touch_exception: None,
            callbacks: spec.callbacks,
            last_tick: timing::now_ms(),
        };
        entry.sync_touch_exception();
        entry
    }

    fn sync_touch_exception(&mut self) {
        self.touch_exception = match self.touch_region {
            PopupTouchRegion::None => None,
            PopupTouchRegion::ContentRoot => self
                .behavior
                .content_root
                .and_then(|node| self.surface.tree().layout_rect(node))
                .map(layout_rect_to_rect),
            PopupTouchRegion::Rect(rect) => Some(rect),
        };
    }

    fn can_attempt_dismissal(&self) -> bool {
        self.behavior.dismiss_on_background_tap
            || self.callbacks.approve_dismissal.is_some()
            || self.callbacks.dismissal.is_some()
            || self.callbacks.approve_touch.is_some()
    }
}

pub struct PopupManager {
    entries: Vec<PopupEntry>,
    next_id: u64,
    viewport: gfx::RectF,
    device_scale: f32,
}

impl PopupManager {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
            viewport: gfx::RectF::new(0.0, 0.0, 0.0, 0.0),
            device_scale: 1.0,
        }
    }

    pub fn set_viewport(&mut self, viewport: gfx::RectF, device_scale: f32) {
        self.viewport = viewport;
        self.device_scale = device_scale;
        for entry in &mut self.entries {
            entry.surface.layout(viewport.w, viewport.h);
            entry.sync_touch_exception();
        }
    }

    pub fn push(&mut self, surface: UiSurface, spec: PopupSpec) -> PopupHandle {
        let id = OverlayHandle(self.next_id);
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let entry = PopupEntry::new(id, surface, spec, self.viewport);
        self.entries.push(entry);
        self.entries.sort_by(|a, b| {
            a.visual.z_index.cmp(&b.visual.z_index).then_with(|| a.id.0.cmp(&b.id.0))
        });
        id
    }

    pub fn remove(&mut self, handle: PopupHandle) -> Option<UiSurface> {
        if let Some(pos) = self.entries.iter().position(|entry| entry.id == handle) {
            Some(self.entries.remove(pos).surface)
        } else {
            None
        }
    }

    pub fn key_popup(&self) -> Option<PopupHandle> {
        self.entries.last().map(|entry| entry.id)
    }

    pub fn popup_is_key_window(&self) -> bool {
        self.key_popup().is_some()
    }

    pub fn dismiss(&mut self, handle: PopupHandle) -> bool {
        let Some(pos) = self.entries.iter().position(|entry| entry.id == handle) else {
            return false;
        };

        let approved = {
            let entry = &mut self.entries[pos];
            if let Some(approve) = entry.callbacks.approve_dismissal.as_mut() {
                approve(&mut entry.surface)
            } else {
                true
            }
        };
        if !approved {
            return false;
        }

        let mut entry = self.entries.remove(pos);
        if let Some(mut dismissal) = entry.callbacks.dismissal.take() {
            dismissal(&mut entry.surface);
        }
        true
    }

    pub fn dismiss_key_popup(&mut self) -> Option<PopupHandle> {
        let handle = self.key_popup()?;
        if self.dismiss(handle) {
            Some(handle)
        } else {
            None
        }
    }

    pub fn content_size_changed(&mut self, handle: PopupHandle) -> bool {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == handle) {
            entry.surface.layout(self.viewport.w, self.viewport.h);
            entry.sync_touch_exception();
            true
        } else {
            false
        }
    }

    pub fn set_touch_region(&mut self, handle: PopupHandle, region: PopupTouchRegion) -> bool {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == handle) {
            entry.touch_region = region;
            entry.sync_touch_exception();
            true
        } else {
            false
        }
    }

    pub fn touch_region(&self, handle: PopupHandle) -> Option<PopupTouchRegion> {
        self.entries.iter().find(|entry| entry.id == handle).map(|entry| entry.touch_region)
    }

    pub fn pointer_event(&mut self, x: f32, y: f32, buttons: u32) -> OverlayPointerResult {
        let Some((handle, hit, inside_content, should_attempt_dismiss, should_consume)) = ({
            let Some(entry) = self.entries.last_mut() else { return OverlayPointerResult::Ignored };
            let hit = entry.surface.hit_test(x, y);
            let mut inside_content = hit.is_some();
            if let (Some(content_root), Some((hit_node, _))) = (entry.behavior.content_root, hit) {
                inside_content = entry.surface.tree().is_descendant(hit_node, content_root);
            }
            let touch_rejected = if let Some(approve_touch) = entry.callbacks.approve_touch.as_mut()
            {
                !approve_touch(&mut entry.surface, [x, y])
            } else {
                false
            };
            let outside_touch_exception =
                entry.touch_exception.map(|rect| !rect_contains_point(rect, x, y)).unwrap_or(false);
            let should_attempt_dismiss = entry.can_attempt_dismissal()
                && (touch_rejected
                    || outside_touch_exception
                    || (buttons == 0
                        && entry.behavior.dismiss_on_background_tap
                        && !inside_content));
            let should_consume = entry.behavior.block_underlying_inputs || inside_content;
            Some((entry.id, hit, inside_content, should_attempt_dismiss, should_consume))
        }) else {
            return OverlayPointerResult::Ignored;
        };

        if should_attempt_dismiss {
            if self.dismiss(handle) {
                return OverlayPointerResult::Dismissed { handle };
            }
            return OverlayPointerResult::Consumed {
                handle,
                node: if inside_content { hit } else { None },
            };
        }

        if should_consume {
            OverlayPointerResult::Consumed { handle, node: hit }
        } else {
            OverlayPointerResult::Ignored
        }
    }

    pub fn tick_at(&mut self, now_ms: u64) -> bool {
        let mut changed = false;
        for entry in &mut self.entries {
            if entry.surface.tick_at(now_ms) {
                changed = true;
            }
            entry.last_tick = now_ms;
        }
        changed
    }

    pub fn tick(&mut self) -> bool {
        let now = timing::now_ms();
        self.tick_at(now)
    }

    pub fn encode(&self, builder: &mut DrawListBuilder) {
        for entry in self.sorted_entries() {
            builder.backdrop(
                self.viewport,
                entry.visual.blur_sigma,
                entry.visual.tint,
                entry.visual.alpha,
            );
            entry.surface.encode(builder);
        }
    }

    pub fn focus_target(&self) -> Option<NodeId> {
        self.entries.last().and_then(|entry| entry.behavior.focus_root)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn capture(&self) -> gfx::DrawList {
        let mut builder = DrawListBuilder::new();
        self.encode(&mut builder);
        builder.into_inner()
    }

    pub fn surface_mut(&mut self, handle: PopupHandle) -> Option<&mut UiSurface> {
        self.entries.iter_mut().find(|entry| entry.id == handle).map(|entry| &mut entry.surface)
    }

    fn sorted_entries(&self) -> impl Iterator<Item = &PopupEntry> {
        let mut refs: Vec<&PopupEntry> = self.entries.iter().collect();
        refs.sort_by(|a, b| {
            a.visual.z_index.cmp(&b.visual.z_index).then_with(|| a.id.0.cmp(&b.id.0))
        });
        refs.into_iter()
    }
}
