use oxide_text::{Font, FontDb, TextShaper};

#[test]
fn fontdb_adds_and_shaper_errors_on_invalid_font() {
    let mut db = FontDb::default();
    let id = db.add_font(Font::from_bytes(vec![0u8; 32]));
    assert!(db.font(id).is_some());

    let mut shaper = TextShaper::default();
    match shaper.shape(db.font(id).unwrap(), id, "test", 12.0) {
        Ok(_) => panic!("expected shaping to fail"),
        Err(err) => assert!(err.to_string().contains("face")),
    }
}
