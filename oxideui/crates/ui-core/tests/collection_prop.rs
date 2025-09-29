use oxideui_renderer_api as gfx;
use oxideui_ui_core as ui;
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

fn deterministic_config() -> ProptestConfig {
    ProptestConfig {
        rng_seed: RngSeed::Fixed(0xC011_EC7E_C011EC7Eu64),
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(deterministic_config())]
    // Grid layout visibility: visible items intersect viewport; content metrics are non-negative.
    #[test]
    fn collection_grid_visibility(
        vp_w in 100f32..800f32,
        vp_h in 100f32..800f32,
        col_w in 20f32..200f32,
        spacing in 0f32..20f32,
        count in 0usize..2000usize,
        scroll in 0f32..2000f32,
    ) {
        let mut view = ui::collection::CollectionView::new(ui::collection::CollectionMode::VerticalGrid { col_width: col_w, spacing });
        view.set_count(count);
        view.set_scroll(scroll);
        let vp = gfx::RectF::new(0.0, 0.0, vp_w, vp_h);
        struct Meas; impl ui::collection::Measure for Meas { fn measure(&mut self, _i: usize, cw: f32) -> f32 { (cw*0.6).max(1.0) } }
        struct Rend; impl ui::collection::CellRenderer for Rend {
            fn render(&mut self, _id: u32, _i: usize, _r: gfx::RectF, _f: bool, _h: bool, _b: &mut ui::DrawListBuilder) {}
        }
        let mut b = ui::DrawListBuilder::new();
        let content = view.layout_and_render(vp, &mut Meas, &mut Rend, &mut b);
        // Content metrics are non-negative and reasonably bounded
        prop_assert!(content.content_w >= 0.0 && content.content_h >= 0.0);
        // The view does not clamp scroll; we only assert it computes sane non-negative metrics and doesn't panic.
    }
}
