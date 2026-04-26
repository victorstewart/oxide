//! Generic scroll state helper for clamped offset tracking.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollState {
    offset: f32,
    content_extent: f32,
    viewport_extent: f32,
}

impl ScrollState {
    #[must_use]
    pub fn new(content_extent: f32, viewport_extent: f32) -> Self {
        let mut state = Self {
            offset: 0.0,
            content_extent: sanitize_extent(content_extent),
            viewport_extent: sanitize_extent(viewport_extent),
        };
        state.offset = state.offset.clamp(0.0, state.max_offset());
        state
    }

    pub fn update_extents(&mut self, content_extent: f32, viewport_extent: f32) {
        self.content_extent = sanitize_extent(content_extent);
        self.viewport_extent = sanitize_extent(viewport_extent);
        self.offset = self.offset.clamp(0.0, self.max_offset());
    }

    pub fn set_offset(&mut self, value: f32) -> f32 {
        let normalized = if value.is_finite() { value } else { 0.0 };
        self.offset = normalized.clamp(0.0, self.max_offset());
        self.offset
    }

    pub fn scroll_by(&mut self, delta: f32) -> f32 {
        let normalized = if delta.is_finite() { delta } else { 0.0 };
        self.set_offset(self.offset + normalized)
    }

    #[must_use]
    pub fn offset(&self) -> f32 {
        self.offset
    }

    #[must_use]
    pub fn max_offset(&self) -> f32 {
        (self.content_extent - self.viewport_extent).max(0.0)
    }

    #[must_use]
    pub fn progress(&self) -> f32 {
        let max = self.max_offset();
        if max <= f32::EPSILON {
            return 0.0;
        }
        (self.offset / max).clamp(0.0, 1.0)
    }
}

#[inline]
fn sanitize_extent(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}
