use oxide_platform_api::TouchId;
use oxide_renderer_api as gfx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupTapRegion
{
   Panel,
   Outside,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PanelPopupState
{
   open: bool,
}

impl PanelPopupState
{
   #[must_use]
   pub fn new() -> Self
   {
      Self::default()
   }

   pub fn open(&mut self)
   {
      self.open = true;
   }

   pub fn close(&mut self)
   {
      self.open = false;
   }

   pub fn toggle(&mut self)
   {
      if self.open
      {
         self.close();
      }
      else
      {
         self.open();
      }
   }

   #[must_use]
   pub fn is_open(&self) -> bool
   {
      self.open
   }

   #[must_use]
   pub fn classify_tap(&self, panel_rect: gfx::RectF, x: f32, y: f32) -> PopupTapRegion
   {
      if x >= panel_rect.x
         && x <= panel_rect.x + panel_rect.w
         && y >= panel_rect.y
         && y <= panel_rect.y + panel_rect.h
      {
         PopupTapRegion::Panel
      }
      else
      {
         PopupTapRegion::Outside
      }
   }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct WheelPickerDragState
{
   touch_id: TouchId,
   origin_y: f32,
   origin_position: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelPickerState
{
   item_count: usize,
   position: f32,
   overscroll_rows: f32,
   drag: Option<WheelPickerDragState>,
}

impl WheelPickerState
{
   #[must_use]
   pub fn new(item_count: usize, selected_index: usize) -> Self
   {
      let mut state = Self {
         item_count,
         position: 0.0,
         overscroll_rows: 0.0,
         drag: None,
      };
      state.sync_to_index(selected_index);
      state
   }

   #[must_use]
   pub fn with_overscroll_rows(mut self, overscroll_rows: f32) -> Self
   {
      self.overscroll_rows = overscroll_rows.max(0.0);
      self.position = self.clamp_position(self.position);
      self
   }

   #[must_use]
   pub fn item_count(&self) -> usize
   {
      self.item_count
   }

   #[must_use]
   pub fn position(&self) -> f32
   {
      self.position
   }

   #[must_use]
   pub fn overscroll_rows(&self) -> f32
   {
      self.overscroll_rows
   }

   #[must_use]
   pub fn drag_touch_id(&self) -> Option<TouchId>
   {
      self.drag.map(|drag| drag.touch_id)
   }

   #[must_use]
   pub fn is_dragging(&self) -> bool
   {
      self.drag.is_some()
   }

   pub fn set_item_count(&mut self, item_count: usize)
   {
      self.item_count = item_count;
      self.position = self.clamp_position(self.position);
      if item_count == 0
      {
         self.drag = None;
      }
   }

   pub fn sync_to_index(&mut self, selected_index: usize)
   {
      self.drag = None;
      self.position = self.clamp_position(selected_index as f32);
   }

   #[must_use]
   pub fn selected_index(&self) -> usize
   {
      self.snap_index()
   }

   #[must_use]
   pub fn snap_index(&self) -> usize
   {
      Self::snap_index_for(self.item_count, self.overscroll_rows, self.position)
   }

   #[must_use]
   pub fn snap_index_for(item_count: usize, overscroll_rows: f32, position: f32) -> usize
   {
      if item_count == 0
      {
         return 0;
      }
      let snapped = Self::clamp_position_for(item_count, overscroll_rows, position);
      let floor = snapped.floor();
      let fraction = snapped - floor;
      let rounded = if fraction > 0.5
      {
         floor + 1.0
      }
      else
      {
         floor
      };
      rounded.clamp(0.0, (item_count - 1) as f32) as usize
   }

   #[must_use]
   pub fn clamp_position(&self, position: f32) -> f32
   {
      Self::clamp_position_for(self.item_count, self.overscroll_rows, position)
   }

   #[must_use]
   pub fn clamp_position_for(item_count: usize, overscroll_rows: f32, position: f32) -> f32
   {
      if item_count == 0
      {
         return 0.0;
      }
      let overscroll = overscroll_rows.max(0.0);
      let max_index = (item_count - 1) as f32;
      position.clamp(-overscroll, max_index + overscroll)
   }

   pub fn begin_drag(&mut self, touch_id: TouchId, y: f32)
   {
      self.drag = Some(WheelPickerDragState {
         touch_id,
         origin_y: y,
         origin_position: self.position,
      });
   }

   pub fn update_drag(&mut self, touch_id: TouchId, y: f32, row_height: f32) -> bool
   {
      let Some(drag) = self.drag else
      {
         return false;
      };
      if drag.touch_id != touch_id
      {
         return false;
      }
      let safe_row_height = row_height.max(1.0);
      let drag_dy = y - drag.origin_y;
      self.position = self.clamp_position(drag.origin_position - drag_dy / safe_row_height);
      true
   }

   pub fn finish_drag(&mut self, touch_id: TouchId) -> Option<usize>
   {
      let Some(drag) = self.drag else
      {
         return None;
      };
      if drag.touch_id != touch_id
      {
         return None;
      }
      self.drag = None;
      let snapped_index = self.snap_index();
      self.position = snapped_index as f32;
      Some(snapped_index)
   }

   pub fn cancel_drag(&mut self, touch_id: Option<TouchId>) -> bool
   {
      let Some(active) = self.drag else
      {
         return false;
      };
      if let Some(expected) = touch_id
      {
         if active.touch_id != expected
         {
            return false;
         }
      }
      self.drag = None;
      true
   }

   #[must_use]
   pub fn index_for_linear_tap(&self, surface_rect: gfx::RectF, row_height: f32, y: f32) -> usize
   {
      if self.item_count == 0
      {
         return 0;
      }
      let safe_row_height = row_height.max(1.0);
      let center_y = surface_rect.y + surface_rect.h * 0.50;
      let next_position = (self.position + (y - center_y) / safe_row_height).round();
      next_position.clamp(0.0, (self.item_count - 1) as f32) as usize
   }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupWheelPickerState
{
   popup: PanelPopupState,
   wheel: WheelPickerState,
}

impl PopupWheelPickerState
{
   #[must_use]
   pub fn new(item_count: usize, selected_index: usize) -> Self
   {
      Self {
         popup: PanelPopupState::new(),
         wheel: WheelPickerState::new(item_count, selected_index),
      }
   }

   #[must_use]
   pub fn with_overscroll_rows(mut self, overscroll_rows: f32) -> Self
   {
      self.wheel = self.wheel.with_overscroll_rows(overscroll_rows);
      self
   }

   pub fn open(&mut self)
   {
      self.popup.open();
      self.wheel.cancel_drag(None);
   }

   pub fn close(&mut self)
   {
      self.popup.close();
      self.wheel.cancel_drag(None);
   }

   #[must_use]
   pub fn is_open(&self) -> bool
   {
      self.popup.is_open()
   }

   pub fn sync_to_index(&mut self, selected_index: usize)
   {
      self.wheel.sync_to_index(selected_index);
   }

   #[must_use]
   pub fn position(&self) -> f32
   {
      self.wheel.position()
   }

   #[must_use]
   pub fn selected_index(&self) -> usize
   {
      self.wheel.selected_index()
   }

   #[must_use]
   pub fn classify_panel_tap(
      &self,
      panel_rect: gfx::RectF,
      x: f32,
      y: f32,
   ) -> PopupTapRegion
   {
      self.popup.classify_tap(panel_rect, x, y)
   }

   pub fn begin_drag(&mut self, touch_id: TouchId, y: f32)
   {
      self.wheel.begin_drag(touch_id, y);
   }

   pub fn update_drag(&mut self, touch_id: TouchId, y: f32, row_height: f32) -> bool
   {
      self.wheel.update_drag(touch_id, y, row_height)
   }

   pub fn finish_drag(&mut self, touch_id: TouchId) -> Option<usize>
   {
      self.wheel.finish_drag(touch_id)
   }

   pub fn cancel_drag(&mut self, touch_id: Option<TouchId>) -> bool
   {
      self.wheel.cancel_drag(touch_id)
   }

   #[must_use]
   pub fn is_dragging(&self) -> bool
   {
      self.wheel.is_dragging()
   }

   #[must_use]
   pub fn drag_touch_id(&self) -> Option<TouchId>
   {
      self.wheel.drag_touch_id()
   }

   #[must_use]
   pub fn index_for_linear_tap(&self, surface_rect: gfx::RectF, row_height: f32, y: f32) -> usize
   {
      self.wheel.index_for_linear_tap(surface_rect, row_height, y)
   }
}
