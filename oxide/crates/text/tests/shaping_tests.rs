use oxide_renderer_api as api;
use oxide_text::{Atlas, Font, FontDb, TextShaper};

const LATIN_FONT: &[u8] = include_bytes!("fixtures/test_text_latin.ttf");
const CJK_FONT: &[u8] = include_bytes!("fixtures/test_text_cjk.ttf");

fn load_font(data: &[u8]) -> Font {
    Font::from_bytes(data.to_vec())
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
    assert!(indices.iter().all(|index| usize::from(*index) < verts.len()));
    let (img, _, _) = atlas.image();
    assert!(img.iter().any(|&px| px != 0));
    assert!(!verts.is_empty());
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
