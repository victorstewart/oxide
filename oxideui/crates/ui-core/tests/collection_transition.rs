use oxideui_renderer_api as gfx;
use oxideui_ui_core::{collection, DrawListBuilder};

struct DummyMeasure;

impl collection::Measure for DummyMeasure {
    fn measure(&mut self, _index: usize, constraint: f32) -> f32 {
        constraint.min(48.0)
    }
}

struct CaptureRenderer {
    rects: Vec<gfx::RectF>,
}

impl collection::CellRenderer for CaptureRenderer {
    fn render(
        &mut self,
        _cell_id: u32,
        _index: usize,
        rect: gfx::RectF,
        _focused: bool,
        _hovered: bool,
        _b: &mut DrawListBuilder,
    ) {
        self.rects.push(rect);
    }
}

#[test]
fn shrink_grow_transition_scales_on_entry() {
    let mut view = collection::CollectionView::new(collection::CollectionMode::VerticalGrid {
        col_width: 60.0,
        spacing: 6.0,
    });
    view.set_transition(Some(collection::CellTransition::shrink_grow(360, 0.82, 1.08)));
    view.set_count(1);
    let mut measure = DummyMeasure;
    let mut renderer = CaptureRenderer { rects: Vec::new() };
    let viewport = gfx::RectF::new(0.0, 0.0, 120.0, 120.0);
    let mut builder = DrawListBuilder::new();
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert_eq!(renderer.rects.len(), 1);
    let rect = renderer.rects[0];
    assert!(rect.w < 60.0 + f32::EPSILON);
    assert!(rect.h < 60.0 + f32::EPSILON);
}
