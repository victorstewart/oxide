use oxide_ui_core as ui;
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

fn row_config() -> ProptestConfig {
    ProptestConfig { rng_seed: RngSeed::Fixed(0x5EED0A55AA11u64), ..ProptestConfig::default() }
}

fn column_config() -> ProptestConfig {
    ProptestConfig { rng_seed: RngSeed::Fixed(0x5EEDC0110001u64), ..ProptestConfig::default() }
}

proptest! {
    #![proptest_config(row_config())]
    // Row layout: fixed + flex distribution stays within content width, non-negative sizes, and gap respected.
    #[test]
    fn row_layout_flex_distribution(
        root_w in 200f32..1200f32,
        root_h in 100f32..600f32,
        pad in 0f32..20f32,
        gap in 0f32..20f32,
        a_w in 0f32..200f32,
        b_w in 0f32..200f32,
        flex in 0f32..3f32,
    ) {
        let mut tree = ui::NodeTree::new_root(ui::NodeStyle {
            axis: ui::Axis::Row,
            size: ui::Size2D { w: ui::Dim::Px(root_w), h: ui::Dim::Px(root_h) },
            padding: ui::Edges { left: pad, top: pad, right: pad, bottom: pad },
            gap,
            ..ui::NodeStyle::default()
        });
        let a = tree.add_node(tree.root(), ui::NodeStyle { size: ui::Size2D { w: ui::Dim::Px(a_w), h: ui::Dim::Px(50.0) }, ..ui::NodeStyle::default() });
        let b = tree.add_node(tree.root(), ui::NodeStyle { size: ui::Size2D { w: ui::Dim::Px(b_w), h: ui::Dim::Px(50.0) }, ..ui::NodeStyle::default() });
        let c = tree.add_node(tree.root(), ui::NodeStyle { size: ui::Size2D { w: ui::Dim::Auto, h: ui::Dim::Px(50.0) }, flex_grow: flex, ..ui::NodeStyle::default() });
        tree.layout(root_w, root_h);

        // Use hit testing to verify ordering and positions without accessing internals
        let y = (pad + 5.0).min(root_h - 1.0);
        let content_w = (root_w - pad*2.0).max(0.0);
        // Ensure we have space to probe B
        prop_assume!(content_w >= a_w + gap + 2.0);
        // Hit inside A near its left
        if a_w >= 2.0 {
            let xa = (pad + 1.0).min(root_w - 1.0);
            let hit_a = tree.hit_test(xa, y).map(|h| h.0);
            prop_assert_eq!(hit_a, Some(a));
        }
        // Hit inside B just after A + gap
        if b_w >= 2.0 {
            let xb = (pad + a_w + gap + 1.0).min(root_w - 1.0);
            let hit_b = tree.hit_test(xb, y).map(|h| h.0);
            prop_assert_eq!(hit_b, Some(b));
        }
        // If there is leftover and flex>0, near the right edge should be C
        let gaps = if 3 > 1 { gap * 2.0 } else { 0.0 };
        let fixed = (a_w + b_w).min(content_w).max(0.0);
        let leftover = (content_w - fixed - gaps).max(0.0);
        if flex > 0.0 && leftover > 3.0 {
            let xc = (root_w - pad - 1.0).max(1.0);
            let hit_c = tree.hit_test(xc, y).map(|h| h.0);
            prop_assert_eq!(hit_c, Some(c));
        }
    }
}

proptest! {
    #![proptest_config(column_config())]
    // Column layout: similar checks vertically.
    #[test]
    fn column_layout_flex_distribution(
        root_w in 200f32..800f32,
        root_h in 200f32..1200f32,
        pad in 0f32..20f32,
        gap in 0f32..20f32,
        a_h in 0f32..200f32,
        b_h in 0f32..200f32,
        flex in 0f32..3f32,
    ) {
        let mut tree = ui::NodeTree::new_root(ui::NodeStyle {
            axis: ui::Axis::Column,
            size: ui::Size2D { w: ui::Dim::Px(root_w), h: ui::Dim::Px(root_h) },
            padding: ui::Edges { left: pad, top: pad, right: pad, bottom: pad },
            gap,
            ..ui::NodeStyle::default()
        });
        let a = tree.add_node(tree.root(), ui::NodeStyle { size: ui::Size2D { w: ui::Dim::Px(50.0), h: ui::Dim::Px(a_h) }, ..ui::NodeStyle::default() });
        let b = tree.add_node(tree.root(), ui::NodeStyle { size: ui::Size2D { w: ui::Dim::Px(50.0), h: ui::Dim::Px(b_h) }, ..ui::NodeStyle::default() });
        let c = tree.add_node(tree.root(), ui::NodeStyle { size: ui::Size2D { w: ui::Dim::Px(50.0), h: ui::Dim::Auto }, flex_grow: flex, ..ui::NodeStyle::default() });
        tree.layout(root_w, root_h);

        let x = (pad + 5.0).min(root_w - 1.0);
        let content_h = (root_h - pad*2.0).max(0.0);
        prop_assume!(content_h >= a_h + gap + 2.0);
        // Hit inside A near its top
        if a_h >= 2.0 {
            let ya = (pad + 1.0).min(root_h - 1.0);
            let hit_a = tree.hit_test(x, ya).map(|h| h.0);
            prop_assert_eq!(hit_a, Some(a));
        }
        // Hit inside B just after A + gap
        if b_h >= 2.0 {
            let yb = (pad + a_h + gap + 1.0).min(root_h - 1.0);
            let hit_b = tree.hit_test(x, yb).map(|h| h.0);
            prop_assert_eq!(hit_b, Some(b));
        }
        // If leftover and flex>0, near the bottom edge should be C
        let gaps = if 3 > 1 { gap * 2.0 } else { 0.0 };
        let fixed = (a_h + b_h).min(content_h).max(0.0);
        let leftover = (content_h - fixed - gaps).max(0.0);
        if flex > 0.0 && leftover > 3.0 {
            let yc = (root_h - pad - 1.0).max(1.0);
            let hit_c = tree.hit_test(x, yc).map(|h| h.0);
            prop_assert_eq!(hit_c, Some(c));
        }
    }
}
