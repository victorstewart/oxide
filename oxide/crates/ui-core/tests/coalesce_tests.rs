use oxide_renderer_api::{Color, DrawCmd, DrawList, GlyphRun, ImageHandle, IndexSpan, VertexSpan};
use oxide_ui_core::{coalesce_adjacent_draws, coalesce_adjacent_draws_reuse};

fn solid(vb_off: u32, vb_len: u32, ib_off: u32, ib_len: u32) -> DrawCmd {
    DrawCmd::Solid {
        vb: VertexSpan { offset: vb_off, len: vb_len },
        ib: IndexSpan { offset: ib_off, len: ib_len },
        color: Color::rgba(1.0, 1.0, 1.0, 1.0),
    }
}

fn glyph(vb_off: u32, vb_len: u32, ib_off: u32, ib_len: u32) -> DrawCmd {
    DrawCmd::GlyphRun {
        run: GlyphRun {
            atlas: ImageHandle(7),
            atlas_revision: 0,
            vb: VertexSpan { offset: vb_off, len: vb_len },
            ib: IndexSpan { offset: ib_off, len: ib_len },
            sdf: false,
            color: Color::rgba(1.0, 1.0, 1.0, 1.0),
        },
    }
}

#[test]
fn coalesce_keeps_nonindexed_quad_solids_separate() {
    let mut list =
        DrawList { items: vec![solid(0, 4, 0, 0), solid(4, 4, 0, 0)], ..DrawList::default() };
    coalesce_adjacent_draws(&mut list);
    assert_eq!(list.items.len(), 2, "quad strips must not be merged");
}

#[test]
fn coalesce_merges_indexed_solids() {
    let mut list =
        DrawList { items: vec![solid(10, 8, 20, 12), solid(18, 6, 32, 9)], ..DrawList::default() };
    coalesce_adjacent_draws(&mut list);
    assert_eq!(list.items.len(), 1);
    match &list.items[0] {
        DrawCmd::Solid { vb, ib, .. } => {
            assert_eq!(vb.offset, 10);
            assert_eq!(vb.len, 14);
            assert_eq!(ib.offset, 20);
            assert_eq!(ib.len, 21);
        }
        _ => panic!("expected merged solid"),
    }
}

#[test]
fn coalesce_reuse_merges_indexed_solids_with_caller_storage() {
    let mut list =
        DrawList { items: vec![solid(10, 8, 20, 12), solid(18, 6, 32, 9)], ..DrawList::default() };
    let mut scratch = Vec::with_capacity(8);
    coalesce_adjacent_draws_reuse(&mut list, &mut scratch);
    assert_eq!(list.items.len(), 1);
    match &list.items[0] {
        DrawCmd::Solid { vb, ib, .. } => {
            assert_eq!(vb.offset, 10);
            assert_eq!(vb.len, 14);
            assert_eq!(ib.offset, 20);
            assert_eq!(ib.len, 21);
        }
        _ => panic!("expected merged solid"),
    }

    list.items.clear();
    list.items.push(solid(0, 6, 0, 0));
    list.items.push(solid(6, 3, 0, 0));
    coalesce_adjacent_draws_reuse(&mut list, &mut scratch);
    assert_eq!(list.items.len(), 1);
}

#[test]
fn coalesce_merges_nonindexed_triangle_lists() {
    let mut list =
        DrawList { items: vec![solid(0, 6, 0, 0), solid(6, 3, 0, 0)], ..DrawList::default() };
    coalesce_adjacent_draws(&mut list);
    assert_eq!(list.items.len(), 1);
    match &list.items[0] {
        DrawCmd::Solid { vb, ib, .. } => {
            assert_eq!(vb.offset, 0);
            assert_eq!(vb.len, 9);
            assert_eq!(ib.offset, 0);
            assert_eq!(ib.len, 0);
        }
        _ => panic!("expected merged solid"),
    }
}

#[test]
fn coalesce_keeps_glyph_runs_separate_for_local_index_safety() {
    let mut list =
        DrawList { items: vec![glyph(0, 8, 0, 12), glyph(8, 8, 12, 12)], ..DrawList::default() };
    coalesce_adjacent_draws(&mut list);
    assert_eq!(list.items.len(), 2);
}
