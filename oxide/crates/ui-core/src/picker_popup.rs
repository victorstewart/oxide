use oxide_platform_api::{HapticPattern, TouchId};
use oxide_renderer_api as gfx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupTapRegion {
    Panel,
    Outside,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PanelPopupState {
    open: bool,
}

impl PanelPopupState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open();
        }
    }

    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    #[must_use]
    pub fn classify_tap(&self, panel_rect: gfx::RectF, x: f32, y: f32) -> PopupTapRegion {
        if x >= panel_rect.x
            && x <= panel_rect.x + panel_rect.w
            && y >= panel_rect.y
            && y <= panel_rect.y + panel_rect.h
        {
            PopupTapRegion::Panel
        } else {
            PopupTapRegion::Outside
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PickerDragState {
    touch_id: TouchId,
    origin_y: f32,
    origin_position: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickerColumnState {
    item_count: usize,
    position: f32,
    drag: Option<PickerDragState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickerColumnCommit {
    selected_index: usize,
}

impl PickerColumnCommit {
    const fn new(selected_index: usize) -> Self {
        Self { selected_index }
    }

    #[must_use]
    pub const fn selected_index(self) -> usize {
        self.selected_index
    }

    #[must_use]
    pub const fn haptic_pattern(self) -> HapticPattern {
        HapticPattern::ImpactMedium
    }
}

impl PickerColumnState {
    #[must_use]
    pub fn new(item_count: usize, selected_index: usize) -> Self {
        let mut state = Self { item_count, position: 0.0, drag: None };
        state.sync_to_index(selected_index);
        state
    }

    #[must_use]
    pub fn item_count(&self) -> usize {
        self.item_count
    }

    #[must_use]
    pub fn position(&self) -> f32 {
        self.position
    }

    #[must_use]
    pub fn drag_touch_id(&self) -> Option<TouchId> {
        self.drag.map(|drag| drag.touch_id)
    }

    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn set_item_count(&mut self, item_count: usize) {
        self.item_count = item_count;
        self.position = self.clamp_position(self.position);
        if item_count == 0 {
            self.drag = None;
        }
    }

    pub fn sync_to_index(&mut self, selected_index: usize) {
        self.drag = None;
        self.position = self.clamp_position(selected_index as f32);
    }

    #[must_use]
    pub fn selected_index(&self) -> usize {
        self.snap_index()
    }

    #[must_use]
    pub fn snap_index(&self) -> usize {
        Self::snap_index_for(self.item_count, self.position)
    }

    #[must_use]
    pub fn snap_index_for(item_count: usize, position: f32) -> usize {
        if item_count == 0 {
            return 0;
        }
        let snapped = Self::clamp_position_for(item_count, position);
        let floor = snapped.floor();
        let fraction = snapped - floor;
        let rounded = if fraction > 0.5 { floor + 1.0 } else { floor };
        rounded.clamp(0.0, (item_count - 1) as f32) as usize
    }

    #[must_use]
    pub fn clamp_position(&self, position: f32) -> f32 {
        Self::clamp_position_for(self.item_count, position)
    }

    #[must_use]
    pub fn clamp_position_for(item_count: usize, position: f32) -> f32 {
        if item_count == 0 {
            return 0.0;
        }
        let max_index = (item_count - 1) as f32;
        position.clamp(0.0, max_index)
    }

    pub fn begin_drag(&mut self, touch_id: TouchId, y: f32) {
        if self.item_count == 0 {
            self.drag = None;
            return;
        }
        self.drag = Some(PickerDragState { touch_id, origin_y: y, origin_position: self.position });
    }

    pub fn update_drag(&mut self, touch_id: TouchId, y: f32, row_height: f32) -> bool {
        let Some(drag) = self.drag else {
            return false;
        };
        if drag.touch_id != touch_id {
            return false;
        }
        let safe_row_height = row_height.max(1.0);
        let drag_dy = y - drag.origin_y;
        self.position = self.clamp_position(drag.origin_position - drag_dy / safe_row_height);
        true
    }

    pub fn finish_drag(&mut self, touch_id: TouchId) -> Option<PickerColumnCommit> {
        let Some(drag) = self.drag else {
            return None;
        };
        if drag.touch_id != touch_id {
            return None;
        }
        self.drag = None;
        let snapped_index = self.snap_index();
        self.position = snapped_index as f32;
        Some(PickerColumnCommit::new(snapped_index))
    }

    pub fn cancel_drag(&mut self, touch_id: Option<TouchId>) -> bool {
        let Some(active) = self.drag else {
            return false;
        };
        if let Some(expected) = touch_id {
            if active.touch_id != expected {
                return false;
            }
        }
        self.drag = None;
        true
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PopupPickerState {
    popup: PanelPopupState,
    columns: Vec<PickerColumnState>,
}

impl PopupPickerState {
    #[must_use]
    pub fn new(item_count: usize, selected_index: usize) -> Self {
        Self::from_columns(vec![item_count], vec![selected_index])
    }

    #[must_use]
    pub fn from_columns(column_item_counts: Vec<usize>, selected_indices: Vec<usize>) -> Self {
        let columns = column_item_counts
            .into_iter()
            .enumerate()
            .map(|(column_index, item_count)| {
                PickerColumnState::new(
                    item_count,
                    selected_indices.get(column_index).copied().unwrap_or(0),
                )
            })
            .collect();
        Self { popup: PanelPopupState::new(), columns }
    }

    #[must_use]
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    pub fn open(&mut self) {
        self.popup.open();
        self.cancel_all_drags();
    }

    pub fn close(&mut self) {
        self.popup.close();
        self.cancel_all_drags();
    }

    #[must_use]
    pub fn is_open(&self) -> bool {
        self.popup.is_open()
    }

    pub fn set_column_item_count(&mut self, column_index: usize, item_count: usize) -> bool {
        let Some(column) = self.columns.get_mut(column_index) else {
            return false;
        };
        column.set_item_count(item_count);
        true
    }

    #[must_use]
    pub fn position(&self, column_index: usize) -> Option<f32> {
        self.columns.get(column_index).map(PickerColumnState::position)
    }

    #[must_use]
    pub fn selected_index(&self, column_index: usize) -> Option<usize> {
        self.columns.get(column_index).map(PickerColumnState::selected_index)
    }

    pub fn sync_to_index(&mut self, column_index: usize, selected_index: usize) -> bool {
        let Some(column) = self.columns.get_mut(column_index) else {
            return false;
        };
        column.sync_to_index(selected_index);
        true
    }

    pub fn sync_to_indices(&mut self, selected_indices: &[usize]) {
        for (column_index, column) in self.columns.iter_mut().enumerate() {
            column.sync_to_index(selected_indices.get(column_index).copied().unwrap_or(0));
        }
    }

    #[must_use]
    pub fn classify_panel_tap(&self, panel_rect: gfx::RectF, x: f32, y: f32) -> PopupTapRegion {
        self.popup.classify_tap(panel_rect, x, y)
    }

    pub fn begin_drag(&mut self, column_index: usize, touch_id: TouchId, y: f32) -> bool {
        let Some(column) = self.columns.get_mut(column_index) else {
            return false;
        };
        column.begin_drag(touch_id, y);
        true
    }

    pub fn update_drag(
        &mut self,
        column_index: usize,
        touch_id: TouchId,
        y: f32,
        row_height: f32,
    ) -> bool {
        let Some(column) = self.columns.get_mut(column_index) else {
            return false;
        };
        column.update_drag(touch_id, y, row_height)
    }

    pub fn finish_drag(
        &mut self,
        column_index: usize,
        touch_id: TouchId,
    ) -> Option<PickerColumnCommit> {
        self.columns.get_mut(column_index)?.finish_drag(touch_id)
    }

    pub fn cancel_drag(&mut self, column_index: usize, touch_id: Option<TouchId>) -> bool {
        let Some(column) = self.columns.get_mut(column_index) else {
            return false;
        };
        column.cancel_drag(touch_id)
    }

    #[must_use]
    pub fn is_dragging(&self, column_index: usize) -> bool {
        self.columns.get(column_index).is_some_and(PickerColumnState::is_dragging)
    }

    #[must_use]
    pub fn drag_touch_id(&self, column_index: usize) -> Option<TouchId> {
        self.columns.get(column_index).and_then(PickerColumnState::drag_touch_id)
    }
    fn cancel_all_drags(&mut self) {
        for column in &mut self.columns {
            column.cancel_drag(None);
        }
    }
}
