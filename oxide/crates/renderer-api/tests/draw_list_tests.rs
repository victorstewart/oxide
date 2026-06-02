use oxide_renderer_api::{
    Color, Damage, DrawCmd, DrawList, GlyphRun, ImageHandle, IndexSpan, RectF, RectI, Vertex,
    VertexSpan,
};

fn validate_draw_list(list: &DrawList) -> Result<(), &'static str> {
    let mut layer_depth = 0i32;
    let mut clip_depth = 0i32;
    for cmd in &list.items {
        match cmd {
            DrawCmd::LayerBegin { .. } => layer_depth += 1,
            DrawCmd::LayerEnd => layer_depth -= 1,
            DrawCmd::ClipPush { .. } => clip_depth += 1,
            DrawCmd::ClipPop => clip_depth -= 1,
            _ => {}
        }
        if layer_depth < 0 {
            return Err("layer underflow");
        }
        if clip_depth < 0 {
            return Err("clip underflow");
        }
    }
    if layer_depth != 0 {
        return Err("unbalanced layer stack");
    }
    if clip_depth != 0 {
        return Err("unbalanced clip stack");
    }
    Ok(())
}

#[test]
fn draw_list_api_has_no_native_preview_or_app_specific_commands() {
    let source = include_str!("../src/lib.rs");
    assert!(
        !source.contains("NativeCameraPreview") && !source.contains("draw_native_camera_preview"),
        "renderer-api must keep visible camera preview in Oxide-owned renderer commands"
    );
    assert!(
        !source.contains("TopomapGlobe")
            && !source.contains("TopomapGlobeWebApp")
            && !source.contains("topomap_globe")
            && !source.contains("topomap_app_")
            && !source.contains("draw_topomap_globe")
            && !source.contains("DrawCmd::TopomapGlobe"),
        "renderer-api must not expose app-specific Topomap globe commands"
    );
}

#[test]
fn balanced_layers_and_clips_validate() {
    let mut list = DrawList::default();
    list.items.push(DrawCmd::LayerBegin {
        id: 1,
        rect: RectF::new(0.0, 0.0, 50.0, 50.0),
        dirty: false,
    });
    list.items.push(DrawCmd::ClipPush { rect: RectI::new(0, 0, 50, 50) });
    list.items.push(DrawCmd::Solid {
        vb: VertexSpan { offset: 0, len: 4 },
        ib: IndexSpan { offset: 0, len: 6 },
        color: Color::rgba(1.0, 0.0, 0.0, 1.0),
    });
    list.items.push(DrawCmd::GlyphRun {
        run: GlyphRun {
            atlas: ImageHandle(7),
            atlas_revision: 11,
            vb: VertexSpan { offset: 10, len: 12 },
            ib: IndexSpan { offset: 20, len: 18 },
            sdf: false,
            color: Color::rgba(0.1, 0.2, 0.3, 1.0),
        },
    });
    list.items.push(DrawCmd::ClipPop);
    list.items.push(DrawCmd::LayerEnd);

    assert!(validate_draw_list(&list).is_ok());
}

#[test]
fn draw_list_detects_stale_text_atlas_revision() {
    let mut list = DrawList::default();
    list.items.push(DrawCmd::GlyphRun {
        run: GlyphRun {
            atlas: ImageHandle(7),
            atlas_revision: 2,
            vb: VertexSpan { offset: 0, len: 4 },
            ib: IndexSpan { offset: 0, len: 6 },
            sdf: false,
            color: Color::rgba(0.1, 0.2, 0.3, 1.0),
        },
    });

    assert!(list.text_atlas_revision_compatible(ImageHandle(7), 2));
    assert!(!list.text_atlas_revision_compatible(ImageHandle(7), 3));
    assert!(!list.text_atlas_revision_compatible(ImageHandle(8), 99));
    assert!(list.text_atlas_revisions_compatible(&[(ImageHandle(7), 2)]));
    assert!(!list.text_atlas_revisions_compatible(&[]));
}

#[test]
fn detects_unbalanced_layer_stack() {
    let mut list = DrawList::default();
    list.items.push(DrawCmd::LayerEnd);
    assert_eq!(validate_draw_list(&list), Err("layer underflow"));

    let mut list2 = DrawList::default();
    list2.items.push(DrawCmd::LayerBegin {
        id: 1,
        rect: RectF::new(0.0, 0.0, 1.0, 1.0),
        dirty: true,
    });
    assert_eq!(validate_draw_list(&list2), Err("unbalanced layer stack"));
}

#[test]
fn damage_rects_round_trip() {
    let rects = vec![RectI::new(0, 0, 100, 50), RectI::new(10, 10, 20, 20)];
    let damage = Damage { rects: rects.clone() };
    assert_eq!(damage.rects, rects);
}

#[test]
fn vertex_storage_is_mutable() {
    let mut list = DrawList::default();
    list.vertices.push(Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0xFFFF_0000 });
    list.indices.extend([0, 1, 2]);
    assert_eq!(list.vertices.len(), 1);
    assert_eq!(list.indices, vec![0, 1, 2]);
}
