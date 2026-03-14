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

#[cfg(test)]
mod tests {
    use super::ScrollState;

    #[test]
    fn scroll_state_clamps_to_bounds() {
        let mut state = ScrollState::new(800.0, 300.0);
        assert_eq!(state.max_offset(), 500.0);

        let offset = state.scroll_by(200.0);
        assert_eq!(offset, 200.0);

        let offset = state.scroll_by(700.0);
        assert_eq!(offset, 500.0);

        let offset = state.scroll_by(-900.0);
        assert_eq!(offset, 0.0);
    }

    #[test]
    fn scroll_state_progress_tracks_extent() {
        let mut state = ScrollState::new(900.0, 300.0);
        state.set_offset(300.0);
        assert!((state.progress() - 0.5).abs() < 0.0001);

        state.update_extents(200.0, 400.0);
        assert_eq!(state.max_offset(), 0.0);
        assert_eq!(state.offset(), 0.0);
        assert_eq!(state.progress(), 0.0);
    }

    #[test]
    fn scroll_state_rejects_non_finite_values() {
        let mut state = ScrollState::new(f32::NAN, f32::INFINITY);
        assert_eq!(state.max_offset(), 0.0);
        assert_eq!(state.offset(), 0.0);

        state.update_extents(500.0, 100.0);
        state.set_offset(f32::NAN);
        assert_eq!(state.offset(), 0.0);
        state.scroll_by(f32::INFINITY);
        assert_eq!(state.offset(), 0.0);
    }
}
