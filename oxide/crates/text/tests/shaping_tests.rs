use oxide_renderer_api as api;
use oxide_text::{Atlas, CaretAffinity, Font, FontDb, PagedAtlas, RasterCtx, TextShaper};

const LATIN_FONT: &[u8] = include_bytes!("fixtures/test_text_latin.ttf");
const CJK_FONT: &[u8] = include_bytes!("fixtures/test_text_cjk.ttf");
const MACOS_HEBREW_FONT: &str = "/System/Library/Fonts/Supplemental/Arial Unicode.ttf";

fn load_font(data: &[u8]) -> Font {
    Font::from_bytes(data.to_vec())
}

fn load_macos_hebrew_font() -> Option<Font> {
    std::fs::read(MACOS_HEBREW_FONT).ok().map(Font::from_bytes)
}

fn bake_paged_text(
    shaper: &mut TextShaper,
    font: &Font,
    font_id: usize,
    value: &str,
    px: f32,
    raster: &mut RasterCtx,
    atlas: &mut PagedAtlas,
) -> Vec<api::GlyphRun> {
    let shaped = shaper.shape(font, font_id, value, px).expect("shape paged text");
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut runs = Vec::new();
    shaped.bake_paged_into_with(
        raster,
        atlas,
        &mut vertices,
        &mut indices,
        &mut runs,
        api::Color::rgba(0.2, 0.4, 0.8, 1.0),
        0.0,
        24.0,
        1.0,
    );
    runs
}

#[test]
fn paged_atlas_recycles_only_the_unpinned_page_generation() {
    let mut db = FontDb::default();
    let font_id = db.add_font(load_font(LATIN_FONT));
    let font = db.font(font_id).expect("latin font");
    let mut shaper = TextShaper::default();
    let mut raster = RasterCtx::default();
    let mut atlas = PagedAtlas::new(24, 24, 2);
    let mut glyph_pages = Vec::new();

    atlas.begin_frame();
    for ch in 'A'..='Z' {
        let runs = bake_paged_text(
            &mut shaper,
            font,
            font_id,
            &ch.to_string(),
            18.0,
            &mut raster,
            &mut atlas,
        );
        if let Some(run) = runs.first() {
            glyph_pages.push((ch, run.atlas_revision as u32));
        }
        if atlas.page_count() == 2
            && glyph_pages.iter().map(|(_, page)| *page).collect::<std::collections::HashSet<_>>().len() == 2
        {
            break;
        }
    }
    atlas.end_frame();

    let first_id = atlas.page_image(0).expect("first page").0;
    let second = atlas.page_image(1).expect("second page");
    let second_id = second.0;
    let second_generation = second.1;
    let pinned_char = glyph_pages
        .iter()
        .find(|(_, page)| *page == second_id)
        .map(|(ch, _)| *ch)
        .expect("glyph on second page");

    atlas.begin_frame();
    let _ = bake_paged_text(
        &mut shaper,
        font,
        font_id,
        &pinned_char.to_string(),
        18.0,
        &mut raster,
        &mut atlas,
    );
    for px in 19..=31 {
        for ch in 'a'..='z' {
            let _ = bake_paged_text(
                &mut shaper,
                font,
                font_id,
                &ch.to_string(),
                px as f32,
                &mut raster,
                &mut atlas,
            );
            if atlas.eviction_count() > 0 {
                break;
            }
        }
        if atlas.eviction_count() > 0 {
            break;
        }
    }
    atlas.end_frame();

    let first = atlas.page_image(0).expect("recycled first page");
    let second = atlas.page_image(1).expect("pinned second page");
    assert_eq!(first.0, first_id);
    assert!(first.1 > 1);
    assert_eq!((second.0, second.1), (second_id, second_generation));
    assert_eq!(atlas.page_count(), 2);
    assert_eq!(atlas.stats().resident_bytes, atlas.byte_budget());
    assert!(atlas.stats().fragmentation_bytes > 0);
    assert!(atlas.stats().slot_generation_high_water > 0);
    assert_eq!(atlas.eviction_count(), 1);
}

#[test]
fn paged_atlas_reset_purges_extra_pages_and_advances_the_survivor() {
    let mut db = FontDb::default();
    let font_id = db.add_font(load_font(LATIN_FONT));
    let font = db.font(font_id).expect("latin font");
    let mut shaper = TextShaper::default();
    let mut raster = RasterCtx::default();
    let mut atlas = PagedAtlas::new(24, 24, 3);

    atlas.begin_frame();
    for ch in 'A'..='Z' {
        let _ = bake_paged_text(
            &mut shaper,
            font,
            font_id,
            &ch.to_string(),
            16.0,
            &mut raster,
            &mut atlas,
        );
        if atlas.page_count() > 1 {
            break;
        }
    }
    atlas.end_frame();
    assert!(atlas.page_count() > 1);
    let first = atlas.page_image(0).expect("first page before purge");
    let first_id = first.0;
    let first_generation = first.1;

    atlas.reset();

    let first = atlas.page_image(0).expect("first page after purge");
    assert_eq!(atlas.page_count(), 1);
    assert_eq!(first.0, first_id);
    assert!(first.1 > first_generation);
    assert!(first.5.is_some());
    assert!(atlas.stats().resident_bytes <= atlas.byte_budget());
}

#[test]
fn latin_text_shapes_into_atlas() {
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let mut shaper = TextShaper::default();
    let font = db.font(latin_id).expect("latin font");
    let shaped = shaper.shape(font, latin_id, "ABÉ", 32.0).expect("shape latin");
    assert!(shaped.width() > 0.0);

    let mut atlas = Atlas::new(128, 128);
    let mut verts = Vec::new();
    let mut indices = Vec::new();
    let run = shaped.bake_into(
        &mut atlas,
        &mut verts,
        &mut indices,
        api::Color::rgba(0.7, 0.2, 0.1, 1.0),
        api::ImageHandle(1),
        0.0,
        0.0,
        1.0,
    );

    assert_eq!(run.vb.len as usize, verts.len());
    assert_eq!(run.ib.len as usize, indices.len());
    assert_eq!(run.atlas_revision, atlas.revision());
    assert!(indices.iter().all(|index| usize::from(*index) < verts.len()));
    let (img, _, _) = atlas.image();
    assert!(img.iter().any(|&px| px != 0));
    assert!(!verts.is_empty());
}

#[test]
fn shaped_prefix_widths_match_ascii_prefix_shapes() {
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let mut shaper = TextShaper::default();
    let font = db.font(latin_id).expect("latin font");
    let text = "Atlas";
    let shaped = shaper.shape(font, latin_id, text, 22.0).expect("shape text");
    let widths = shaped.prefix_widths_for_boundaries(&[0, 1, 2, 3, 4, 5]);

    assert_eq!(widths.len(), 6);
    assert_eq!(widths[0], 0.0);
    for index in 1..=text.len() {
        let prefix = &text[..index];
        let expected = shaper.shape(font, latin_id, prefix, 22.0).expect("shape prefix").width();
        assert!((widths[index] - expected).abs() < 0.001);
    }
    assert!((widths[5] - shaped.width()).abs() < 0.001);
}

#[test]
fn owned_shape_prefix_widths_match_shaped_output() {
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let mut shaper = TextShaper::default();
    let font = db.font(latin_id).expect("latin font");
    let text = "Cafe\u{301} flow";
    let boundaries = [0, "Cafe\u{301}".len(), "Cafe\u{301} ".len(), text.len()];
    let shaped = shaper.shape(font, latin_id, text, 22.0).expect("shape text");
    let direct = shaped.prefix_widths_for_boundaries(&boundaries);
    let owned = shaped.to_owned_shape().prefix_widths_for_boundaries(&boundaries);

    assert_eq!(direct.len(), owned.len());
    for (left, right) in direct.iter().zip(owned.iter()) {
        assert!((*left - *right).abs() < 0.001);
    }
    assert!((owned.last().copied().unwrap_or(0.0) - shaped.width()).abs() < 0.001);
}

#[test]
fn shaped_prefix_widths_follow_combining_grapheme_boundaries() {
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let mut shaper = TextShaper::default();
    let font = db.font(latin_id).expect("latin font");
    let text = "e\u{301}x";
    let shaped = shaper.shape(font, latin_id, text, 22.0).expect("shape combining text");
    let first_boundary = "e\u{301}".len();
    let widths = shaped.prefix_widths_for_boundaries(&[0, first_boundary, text.len()]);

    assert_eq!(widths.len(), 3);
    assert_eq!(widths[0], 0.0);
    assert!(widths[1] > 0.0);
    assert!(widths[2] >= widths[1]);
    assert!((widths[2] - shaped.width()).abs() < 0.001);
}

#[test]
fn shaped_cursor_map_tracks_combining_grapheme_boundaries() {
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let mut shaper = TextShaper::default();
    let font = db.font(latin_id).expect("latin font");
    let text = "e\u{301}x";
    let shaped = shaper.shape(font, latin_id, text, 22.0).expect("shape combining text");
    let map = shaped.cursor_map_for_text(text);

    assert_eq!(map.len(), 2);
    assert_eq!(map.byte_index(0), 0);
    assert_eq!(map.byte_index(1), "e\u{301}".len());
    assert_eq!(map.byte_index(2), text.len());
    assert_eq!(map.byte_range(0..1), 0.."e\u{301}".len());
    assert_eq!(map.width_at(0), 0.0);
    assert!(map.width_at(1) > 0.0);
    assert!((map.width_at(2) - shaped.width()).abs() < 0.001);
    assert_eq!(map.cursor_for_x(-20.0), 0);
    assert_eq!(map.cursor_for_x(map.width_at(1) + 0.001), 1);
}

#[test]
fn owned_cursor_map_keeps_zwj_cluster_as_one_cursor_step() {
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let mut shaper = TextShaper::default();
    let font = db.font(latin_id).expect("latin font");
    let family = "👨‍👩‍👧‍👦";
    let text = format!("a{family}b");
    let shaped = shaper.shape(font, latin_id, &text, 22.0).expect("shape zwj text");
    let map = shaped.to_owned_shape().cursor_map_for_text(&text);

    assert_eq!(map.len(), 3);
    assert_eq!(map.byte_index(1), "a".len());
    assert_eq!(map.byte_index(2), "a".len() + family.len());
    assert_eq!(map.byte_index(3), text.len());
    assert_eq!(map.byte_range(1..2), "a".len().."a".len() + family.len());
    assert!(map.widths().windows(2).all(|pair| pair[1] >= pair[0]));
}

#[test]
fn shaped_cursor_map_tracks_rtl_visual_order() {
    let Some(font) = load_macos_hebrew_font() else {
        eprintln!("skipping RTL cursor-map test; {MACOS_HEBREW_FONT} is unavailable");
        return;
    };
    let mut db = FontDb::default();
    let font_id = db.add_font(font);
    let mut shaper = TextShaper::default();
    let font = db.font(font_id).expect("hebrew font");
    let text = "אבגדה";
    let shaped = shaper.shape(font, font_id, text, 22.0).expect("shape rtl text");
    let map = shaped.cursor_map_for_text(text);

    assert_eq!(map.len(), 5);
    assert_eq!(map.byte_index(0), 0);
    assert_eq!(map.byte_index(5), text.len());
    assert!(map.width_at(0) > map.width_at(5), "widths={:?}", map.widths());
    assert!(map.widths().windows(2).all(|pair| pair[0] >= pair[1]));
    assert_eq!(map.cursor_for_x(-12.0), 5);
    assert_eq!(map.cursor_for_x(map.width_at(0) + 12.0), 0);
}

#[test]
fn shaped_cursor_map_exposes_mixed_bidi_affinity_positions() {
    let Some(font) = load_macos_hebrew_font() else {
        eprintln!("skipping mixed-bidi cursor-map test; {MACOS_HEBREW_FONT} is unavailable");
        return;
    };
    let mut db = FontDb::default();
    let font_id = db.add_font(font);
    let mut shaper = TextShaper::default();
    let font = db.font(font_id).expect("hebrew font");
    let text = "AאבB";
    let shaped = shaper.shape(font, font_id, text, 22.0).expect("shape mixed text");
    let map = shaped.cursor_map_for_text(text);

    assert_eq!(map.len(), 4);
    let rtl_left = map.width_at_with_affinity(1, CaretAffinity::Upstream);
    let rtl_right = map.width_at_with_affinity(1, CaretAffinity::Downstream);
    assert!(rtl_right > rtl_left, "widths={:?}", map.widths());
    assert!((map.width_at_with_affinity(3, CaretAffinity::Upstream) - rtl_left).abs() < 0.001);
    assert!((map.width_at_with_affinity(3, CaretAffinity::Downstream) - rtl_right).abs() < 0.001);
    assert!(map.width_at(2) > rtl_left && map.width_at(2) < rtl_right);
    assert_eq!(map.cursor_for_x_with_affinity(rtl_right, CaretAffinity::Downstream), 1);
    assert_eq!(map.cursor_for_x_with_affinity(rtl_left, CaretAffinity::Upstream), 3);
    assert_eq!(map.cursor_for_x(map.width_at(2)), 2);
}

#[test]
fn fallback_cursor_map_uses_font_that_covers_each_grapheme() {
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let cjk_id = db.add_font(load_font(CJK_FONT));
    let mut shaper = TextShaper::default();
    let latin = db.font(latin_id).expect("latin font");
    let cjk = db.font(cjk_id).expect("cjk font");
    let text = "A漢B";
    let a_width = shaper.shape(latin, latin_id, "A", 22.0).expect("shape latin A").width();
    let cjk_width = shaper.shape(cjk, cjk_id, "漢", 22.0).expect("shape cjk").width();
    let b_width = shaper.shape(latin, latin_id, "B", 22.0).expect("shape latin B").width();
    let map = shaper
        .cursor_map_with_fallback_fonts(&db, latin_id, &[cjk_id], text, 22.0)
        .expect("fallback cursor map");

    assert_eq!(map.len(), 3);
    assert_eq!(map.byte_index(1), "A".len());
    assert_eq!(map.byte_index(2), "A漢".len());
    assert!((map.width_at(1) - a_width).abs() < 0.001);
    assert!((map.width_at(2) - (a_width + cjk_width)).abs() < 0.001);
    assert!((map.width_at(3) - (a_width + cjk_width + b_width)).abs() < 0.001);
    assert_eq!(map.cursor_for_x(a_width + cjk_width + 0.001), 2);
}

#[test]
fn fallback_shape_runs_use_font_that_covers_each_grapheme() {
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let cjk_id = db.add_font(load_font(CJK_FONT));
    let mut shaper = TextShaper::default();
    let latin = db.font(latin_id).expect("latin font");
    let cjk = db.font(cjk_id).expect("cjk font");
    let text = "A漢B";
    let expected = shaper.shape(latin, latin_id, "A", 22.0).expect("shape latin A").width()
        + shaper.shape(cjk, cjk_id, "漢", 22.0).expect("shape cjk").width()
        + shaper.shape(latin, latin_id, "B", 22.0).expect("shape latin B").width();
    let shaped = shaper
        .shape_with_fallback_fonts(&db, latin_id, &[cjk_id], text, 22.0)
        .expect("fallback shape");

    assert_eq!(shaped.runs.len(), 3);
    assert_eq!(shaped.runs[0].font_id, latin_id);
    assert_eq!(shaped.runs[0].byte_range, 0.."A".len());
    assert_eq!(shaped.runs[1].font_id, cjk_id);
    assert_eq!(shaped.runs[1].byte_range, "A".len().."A漢".len());
    assert_eq!(shaped.runs[2].font_id, latin_id);
    assert_eq!(shaped.runs[2].byte_range, "A漢".len()..text.len());
    assert!((shaped.runs[1].x_offset - shaped.runs[0].shape.width()).abs() < 0.001);
    assert!((shaped.width() - expected).abs() < 0.001);
}

#[test]
fn fallback_cursor_map_preserves_mixed_bidi_affinity_positions() {
    let Some(hebrew) = load_macos_hebrew_font() else {
        eprintln!(
            "skipping fallback mixed-bidi cursor-map test; {MACOS_HEBREW_FONT} is unavailable"
        );
        return;
    };
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let hebrew_id = db.add_font(hebrew);
    let mut shaper = TextShaper::default();
    let text = "AאבB";
    let shaped = shaper
        .shape_with_fallback_fonts(&db, latin_id, &[hebrew_id], text, 22.0)
        .expect("fallback mixed-bidi shape");
    let map = shaped.cursor_map_for_text(text);

    assert_eq!(shaped.runs.len(), 3);
    assert_eq!(shaped.runs[0].font_id, latin_id);
    assert_eq!(shaped.runs[1].font_id, hebrew_id);
    assert_eq!(shaped.runs[2].font_id, latin_id);
    let rtl_left = map.width_at_with_affinity(1, CaretAffinity::Upstream);
    let rtl_right = map.width_at_with_affinity(1, CaretAffinity::Downstream);
    assert!(rtl_right > rtl_left, "widths={:?}", map.widths());
    assert_eq!(map.cursor_for_x_with_affinity(rtl_right, CaretAffinity::Downstream), 1);
    assert_eq!(map.cursor_for_x_with_affinity(rtl_left, CaretAffinity::Upstream), 3);
}

#[test]
fn atlas_reset_preserves_image_contract() {
    let mut atlas = Atlas::new(8, 8);
    let (initial, w, h) = atlas.image();
    assert_eq!(w, 8);
    assert_eq!(h, 8);
    assert_eq!(initial.len(), 64);
    assert!(initial.iter().all(|px| *px == 0));

    atlas.reset();
    let (reset, rw, rh) = atlas.image();
    assert_eq!((rw, rh), (8, 8));
    assert_eq!(reset.len(), 64);
    assert_eq!(atlas.revision(), 1);
    assert_eq!(atlas.eviction_count(), 0);
}

#[test]
fn atlas_dirty_rect_tracks_new_glyph_pixels_only() {
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let mut shaper = TextShaper::default();
    let font = db.font(latin_id).expect("latin font");
    let shaped = shaper.shape(font, latin_id, "AA", 24.0).expect("shape latin");
    let mut atlas = Atlas::new(128, 128);
    let mut verts = Vec::new();
    let mut indices = Vec::new();

    shaped.bake_into(
        &mut atlas,
        &mut verts,
        &mut indices,
        api::Color::rgba(0.7, 0.2, 0.1, 1.0),
        api::ImageHandle(1),
        0.0,
        0.0,
        1.0,
    );
    let dirty = atlas.dirty_rect().expect("new glyphs dirty atlas");
    assert!(dirty.w < 128);
    assert!(dirty.h < 128);

    atlas.clear_dirty();
    shaped.bake_into(
        &mut atlas,
        &mut verts,
        &mut indices,
        api::Color::rgba(0.7, 0.2, 0.1, 1.0),
        api::ImageHandle(1),
        32.0,
        0.0,
        1.0,
    );
    assert_eq!(atlas.dirty_rect(), None);
}

#[test]
fn atlas_eviction_clears_full_reused_slot_for_dirty_upload() {
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let mut shaper = TextShaper::default();
    let font = db.font(latin_id).expect("latin font");
    let wide_px = 22.0;
    let small_px = 12.0;

    let wide_shape = shaper.shape(font, latin_id, "W", wide_px).expect("shape wide glyph");
    let mut wide_probe = Atlas::new(128, 128);
    let mut wide_vertices = Vec::new();
    let mut wide_indices = Vec::new();
    wide_shape.bake_into(
        &mut wide_probe,
        &mut wide_vertices,
        &mut wide_indices,
        api::Color::rgba(0.7, 0.2, 0.1, 1.0),
        api::ImageHandle(1),
        0.0,
        0.0,
        1.0,
    );
    let wide_dirty = wide_probe.dirty_rect().expect("wide glyph dirty rect");

    let small_shape = shaper.shape(font, latin_id, "W", small_px).expect("shape small glyph");
    let mut small_probe = Atlas::new(128, 128);
    let mut small_vertices = Vec::new();
    let mut small_indices = Vec::new();
    small_shape.bake_into(
        &mut small_probe,
        &mut small_vertices,
        &mut small_indices,
        api::Color::rgba(0.7, 0.2, 0.1, 1.0),
        api::ImageHandle(1),
        0.0,
        0.0,
        1.0,
    );
    let small_dirty = small_probe.dirty_rect().expect("small glyph dirty rect");
    assert!(
        wide_dirty.w > small_dirty.w || wide_dirty.h > small_dirty.h,
        "test requires the eviction replacement glyph to be smaller, wide={wide_dirty:?} small={small_dirty:?}",
    );

    let mut atlas = Atlas::new(wide_dirty.w + 2, wide_dirty.h + 2);
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    shaper.shape(font, latin_id, "W", wide_px).expect("shape constrained wide glyph").bake_into(
        &mut atlas,
        &mut vertices,
        &mut indices,
        api::Color::rgba(0.7, 0.2, 0.1, 1.0),
        api::ImageHandle(1),
        0.0,
        0.0,
        1.0,
    );
    let old_slot = atlas.dirty_rect().expect("old slot dirty rect");
    assert_eq!((old_slot.w, old_slot.h), (wide_dirty.w, wide_dirty.h));

    atlas.clear_dirty();
    shaper.shape(font, latin_id, "W", small_px).expect("shape constrained small glyph").bake_into(
        &mut atlas,
        &mut vertices,
        &mut indices,
        api::Color::rgba(0.1, 0.8, 0.2, 1.0),
        api::ImageHandle(1),
        0.0,
        0.0,
        1.0,
    );

    assert_eq!(atlas.eviction_count(), 1);
    let dirty = atlas.dirty_rect().expect("evicted slot dirty rect");
    assert_eq!((dirty.x, dirty.y), (old_slot.x, old_slot.y));
    assert!(dirty.w >= old_slot.w);
    assert!(dirty.h >= old_slot.h);
    let (image, width, _) = atlas.image();
    for y in old_slot.y..old_slot.y + old_slot.h {
        for x in old_slot.x..old_slot.x + old_slot.w {
            let inside_new = x < old_slot.x + small_dirty.w && y < old_slot.y + small_dirty.h;
            if !inside_new {
                let offset = y as usize * width as usize + x as usize;
                assert_eq!(image[offset], 0, "stale pixel after slot eviction at {x},{y}");
            }
        }
    }
}

#[test]
fn atlas_pressure_evicts_and_rebakes_current_run() {
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let mut shaper = TextShaper::default();
    let font = db.font(latin_id).expect("latin font");
    let mut atlas = Atlas::new(24, 24);
    let mut verts = Vec::new();
    let mut indices = Vec::new();

    for ch in "ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars() {
        let shaped =
            shaper.shape(font, latin_id, &ch.to_string(), 22.0).expect("shape pressure glyph");
        let before_evictions = atlas.eviction_count();
        atlas.clear_dirty();
        let v_start = verts.len();
        let i_start = indices.len();
        let run = shaped.bake_into(
            &mut atlas,
            &mut verts,
            &mut indices,
            api::Color::rgba(0.2, 0.3, 0.4, 1.0),
            api::ImageHandle(4),
            0.0,
            0.0,
            1.0,
        );

        if atlas.eviction_count() > before_evictions {
            assert_eq!(run.vb.offset as usize, v_start);
            assert_eq!(run.ib.offset as usize, i_start);
            assert_eq!(run.vb.len as usize, verts.len().saturating_sub(v_start));
            assert_eq!(run.ib.len as usize, indices.len().saturating_sub(i_start));
            assert_eq!(run.atlas_revision, atlas.revision());
            assert!(run.vb.len > 0, "current glyph must render after a stale slot eviction");
            assert!(atlas.dirty_rect().is_some(), "overwritten glyph slot must be uploaded");
            return;
        }
    }

    panic!("expected atlas pressure to evict a stale glyph slot");
}

#[test]
fn atlas_pressure_does_not_evict_glyphs_used_earlier_in_same_run() {
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let mut shaper = TextShaper::default();
    let font = db.font(latin_id).expect("latin font");
    let shaped = shaper.shape(font, latin_id, "ABCDE", 22.0).expect("shape pressure text");
    let mut atlas = Atlas::new(24, 24);
    let mut verts = Vec::new();
    let mut indices = Vec::new();

    let run = shaped.bake_into(
        &mut atlas,
        &mut verts,
        &mut indices,
        api::Color::rgba(0.2, 0.3, 0.4, 1.0),
        api::ImageHandle(6),
        0.0,
        0.0,
        1.0,
    );

    assert_eq!(atlas.eviction_count(), 0);
    assert_eq!(run.atlas_revision, atlas.revision());
    assert!(run.vb.len > 0, "at least one glyph should fit before pressure skips later glyphs");
}

#[test]
fn frame_pin_protects_preexisting_visible_glyphs_from_later_runs() {
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let mut shaper = TextShaper::default();
    let font = db.font(latin_id).expect("latin font");
    let wide = shaper.shape(font, latin_id, "W", 22.0).expect("shape wide glyph");
    let small = shaper.shape(font, latin_id, "W", 12.0).expect("shape small glyph");
    let mut probe = Atlas::new(128, 128);
    let mut probe_vertices = Vec::new();
    let mut probe_indices = Vec::new();
    wide.bake_into(
        &mut probe,
        &mut probe_vertices,
        &mut probe_indices,
        api::Color::rgba(0.2, 0.3, 0.4, 1.0),
        api::ImageHandle(7),
        0.0,
        0.0,
        1.0,
    );
    let slot = probe.dirty_rect().expect("wide glyph slot");
    let mut atlas = Atlas::new(slot.w + 2, slot.h + 2);
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let visible = wide.bake_into(
        &mut atlas,
        &mut vertices,
        &mut indices,
        api::Color::rgba(0.2, 0.3, 0.4, 1.0),
        api::ImageHandle(7),
        0.0,
        0.0,
        1.0,
    );

    atlas.begin_frame();
    let blocked = small.bake_into(
        &mut atlas,
        &mut vertices,
        &mut indices,
        api::Color::rgba(0.2, 0.3, 0.4, 1.0),
        api::ImageHandle(7),
        0.0,
        0.0,
        1.0,
    );
    assert!(visible.vb.len > 0);
    assert_eq!(blocked.vb.len, 0);
    assert_eq!(atlas.eviction_count(), 0);

    atlas.end_frame();
    let admitted = small.bake_into(
        &mut atlas,
        &mut vertices,
        &mut indices,
        api::Color::rgba(0.2, 0.3, 0.4, 1.0),
        api::ImageHandle(7),
        0.0,
        0.0,
        1.0,
    );
    assert!(admitted.vb.len > 0);
    assert_eq!(atlas.eviction_count(), 1);
}

#[test]
fn atlas_too_small_for_glyph_skips_without_eviction_loop() {
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let mut shaper = TextShaper::default();
    let font = db.font(latin_id).expect("latin font");
    let shaped = shaper.shape(font, latin_id, "A", 64.0).expect("shape oversize text");
    let mut atlas = Atlas::new(8, 8);
    let mut verts = Vec::new();
    let mut indices = Vec::new();

    let run = shaped.bake_into(
        &mut atlas,
        &mut verts,
        &mut indices,
        api::Color::rgba(0.9, 0.9, 0.9, 1.0),
        api::ImageHandle(5),
        0.0,
        0.0,
        1.0,
    );

    assert_eq!(atlas.eviction_count(), 0);
    assert_eq!(run.vb.len, 0);
    assert_eq!(run.ib.len, 0);
    assert!(verts.is_empty());
    assert!(indices.is_empty());
}

#[test]
fn cjk_text_shapes_into_atlas() {
    let mut db = FontDb::default();
    let cjk_id = db.add_font(load_font(CJK_FONT));
    let mut shaper = TextShaper::default();
    let font = db.font(cjk_id).expect("cjk font");
    let shaped = shaper.shape(font, cjk_id, "漢", 48.0).expect("shape cjk");
    assert!(shaped.width() > 0.0);

    let mut atlas = Atlas::new(256, 256);
    let mut verts = Vec::new();
    let mut indices = Vec::new();
    let run = shaped.bake_into(
        &mut atlas,
        &mut verts,
        &mut indices,
        api::Color::rgba(0.1, 0.6, 0.3, 1.0),
        api::ImageHandle(2),
        4.0,
        -2.0,
        1.0,
    );

    assert_eq!(run.vb.len as usize, verts.len());
    assert!(!verts.is_empty());
    let (img, _, _) = atlas.image();
    assert!(img.iter().any(|&px| px != 0));
}

#[test]
fn missing_glyph_is_skipped() {
    let mut db = FontDb::default();
    let latin_id = db.add_font(load_font(LATIN_FONT));
    let font = db.font(latin_id).expect("latin font");
    let mut shaper = TextShaper::default();
    let shaped = shaper.shape(font, latin_id, "A B", 28.0).expect("shape with space");

    let mut atlas = Atlas::new(64, 64);
    let mut verts = Vec::new();
    let mut indices = Vec::new();
    shaped.bake_into(
        &mut atlas,
        &mut verts,
        &mut indices,
        api::Color::rgba(0.9, 0.9, 0.9, 1.0),
        api::ImageHandle(3),
        0.0,
        0.0,
        1.0,
    );

    // Two visible glyphs (A and B) should produce eight vertices
    assert_eq!(verts.len(), 8);
    assert_eq!(indices.len(), 12);
}
