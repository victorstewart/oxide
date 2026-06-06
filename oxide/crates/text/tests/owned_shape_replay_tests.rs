use oxide_renderer_api as api;
use oxide_text::{Atlas, Font, FontDb, RasterCtx, TextShaper};
use oxide_wasm_alloc_counter::{snapshot, CountingAllocator};
use std::alloc::System;
use std::sync::Mutex;

#[global_allocator]
static ALLOCATOR: CountingAllocator<System> = CountingAllocator::new(System);
static TEST_LOCK: Mutex<()> = Mutex::new(());

const LATIN_FONT: &[u8] = include_bytes!("fixtures/test_text_latin.ttf");
const MACOS_HEBREW_FONT: &str = "/System/Library/Fonts/Supplemental/Arial Unicode.ttf";

fn load_font(data: &[u8]) -> Font {
    Font::from_bytes(data.to_vec())
}

fn load_macos_hebrew_font() -> Option<Font> {
    std::fs::read(MACOS_HEBREW_FONT).ok().map(Font::from_bytes)
}

#[test]
fn cached_ltr_owned_shape_replay_is_allocation_free_after_warmup() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut db = FontDb::default();
    let font_id = db.add_font(load_font(LATIN_FONT));
    let mut shaper = TextShaper::default();
    let font = db.font(font_id).expect("latin font");
    let owned = shaper
        .shape(font, font_id, "Controls Showcase", 18.0)
        .expect("shape controls label")
        .to_owned_shape();
    let mut raster = RasterCtx::default();
    let mut atlas = Atlas::new(512, 512);
    let mut vertices = Vec::with_capacity(256);
    let mut indices = Vec::with_capacity(384);

    let warm = owned.bake_into_with(
        font,
        &mut raster,
        &mut atlas,
        &mut vertices,
        &mut indices,
        api::Color::rgba(0.1, 0.1, 0.1, 1.0),
        api::ImageHandle(7),
        12.0,
        18.0,
        2.0,
    );
    assert!(warm.vb.len > 0);
    assert!(warm.ib.len > 0);

    vertices.clear();
    indices.clear();
    atlas.clear_dirty();
    let before = snapshot();
    let replay = owned.bake_into_with(
        font,
        &mut raster,
        &mut atlas,
        &mut vertices,
        &mut indices,
        api::Color::rgba(0.2, 0.3, 0.4, 1.0),
        api::ImageHandle(7),
        12.0,
        18.0,
        2.0,
    );
    let after = snapshot();

    assert_eq!(after.alloc_count - before.alloc_count, 0);
    assert_eq!(after.realloc_count - before.realloc_count, 0);
    assert_eq!(replay.vb.len as usize, vertices.len());
    assert_eq!(replay.ib.len as usize, indices.len());
    assert!(atlas.dirty_rect().is_none());
}

#[test]
fn cached_rtl_owned_shape_replay_matches_direct_visual_order() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(font) = load_macos_hebrew_font() else {
        eprintln!("skipping RTL owned-shape replay test; {MACOS_HEBREW_FONT} is unavailable");
        return;
    };
    let mut db = FontDb::default();
    let font_id = db.add_font(font);
    let mut shaper = TextShaper::default();
    let font = db.font(font_id).expect("rtl font");
    let shaped = shaper.shape(font, font_id, "שלום", 24.0).expect("shape rtl text");
    let owned = shaped.to_owned_shape();

    let mut direct_raster = RasterCtx::default();
    let mut direct_atlas = Atlas::new(512, 512);
    let mut direct_vertices = Vec::with_capacity(128);
    let mut direct_indices = Vec::with_capacity(192);
    let direct = shaped.bake_into_with(
        &mut direct_raster,
        &mut direct_atlas,
        &mut direct_vertices,
        &mut direct_indices,
        api::Color::rgba(0.1, 0.1, 0.1, 1.0),
        api::ImageHandle(11),
        0.0,
        0.0,
        2.0,
    );

    let mut owned_raster = RasterCtx::default();
    let mut owned_atlas = Atlas::new(512, 512);
    let mut owned_vertices = Vec::with_capacity(128);
    let mut owned_indices = Vec::with_capacity(192);
    let replay = owned.bake_into_with(
        font,
        &mut owned_raster,
        &mut owned_atlas,
        &mut owned_vertices,
        &mut owned_indices,
        api::Color::rgba(0.1, 0.1, 0.1, 1.0),
        api::ImageHandle(11),
        0.0,
        0.0,
        2.0,
    );

    assert_eq!(replay.vb.len, direct.vb.len);
    assert_eq!(replay.ib.len, direct.ib.len);
    assert_eq!(owned_vertices.len(), direct_vertices.len());
    assert_eq!(owned_indices, direct_indices);
    for (left, right) in owned_vertices.iter().zip(direct_vertices.iter()) {
        assert!((left.x - right.x).abs() < 0.001);
        assert!((left.y - right.y).abs() < 0.001);
        assert_eq!(left.rgba, right.rgba);
    }
}
