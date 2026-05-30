use oxide_renderer_api as gfx;
use oxide_ui_core::{prepare_draws, sort_for_batching, DrawListBuilder, PreparedDraw};

#[test]
fn builder_records_advanced_draws() {
    let mut builder = DrawListBuilder::new();
    let tex = gfx::ImageHandle(9);
    let rect = gfx::RectF::new(4.0, 8.0, 64.0, 32.0);
    let slice = gfx::Insets::new(3.0, 4.0, 5.0, 6.0);
    builder.nine_slice(tex, rect, slice, 0.75);

    let bg_rect = gfx::RectF::new(0.0, 0.0, 100.0, 80.0);
    builder.camera_bg(bg_rect, gfx::Color::rgba(0.1, 0.2, 0.3, 0.9), 0.8, true, false, 12.0);

    let effect_rect = gfx::RectF::new(2.0, 3.0, 90.0, 70.0);
    builder.visual_effect(
        effect_rect,
        gfx::VisualEffect::DarkPopup {
            blur_intensity: 0.5,
            tint: gfx::Color::rgba(1.0, 1.0, 1.0, 0.9),
        },
    );

    builder.spinner([16.0, 24.0], 36.0, 1.0);

    let list = builder.drawlist();
    assert_eq!(list.items.len(), 4);
    match &list.items[0] {
        gfx::DrawCmd::NineSlice { tex: t, rect: r, slice: s, alpha } => {
            assert_eq!((*t, *alpha), (tex, 0.75));
            assert_eq!(*r, rect);
            assert_eq!(*s, slice);
        }
        other => panic!("expected nine-slice, found {other:?}"),
    }
    match &list.items[1] {
        gfx::DrawCmd::CameraBg { rect: r, tint, alpha, grayscale, blur, sigma } => {
            assert_eq!(*r, bg_rect);
            assert_eq!(*tint, gfx::Color::rgba(0.1, 0.2, 0.3, 0.9));
            assert_eq!((*alpha, *grayscale, *blur, *sigma), (0.8, true, false, 12.0));
        }
        other => panic!("expected camera bg, found {other:?}"),
    }
    match &list.items[2] {
        gfx::DrawCmd::VisualEffect { rect, effect } => {
            assert_eq!(*rect, effect_rect);
            assert_eq!(
                *effect,
                gfx::VisualEffect::DarkPopup {
                    blur_intensity: 0.5,
                    tint: gfx::Color::rgba(1.0, 1.0, 1.0, 0.9),
                }
            );
        }
        other => panic!("expected visual effect, found {other:?}"),
    }
    match &list.items[3] {
        gfx::DrawCmd::Spinner { center, atom, alpha } => {
            assert_eq!(*center, [16.0, 24.0]);
            assert_eq!((*atom, *alpha), (36.0, 1.0));
        }
        other => panic!("expected spinner, found {other:?}"),
    }
}

#[test]
fn builder_records_image_mesh_geometry() {
    let mut builder = DrawListBuilder::new();
    let tex = gfx::ImageHandle(42);
    let vertices = [
        gfx::Vertex { x: 1.0, y: 2.0, u: 0.0, v: 0.0, rgba: u32::MAX },
        gfx::Vertex { x: 9.0, y: 2.0, u: 1.0, v: 0.0, rgba: u32::MAX },
        gfx::Vertex { x: 1.0, y: 8.0, u: 0.0, v: 1.0, rgba: u32::MAX },
        gfx::Vertex { x: 9.0, y: 8.0, u: 1.0, v: 1.0, rgba: u32::MAX },
    ];
    let indices = [0, 1, 2, 2, 1, 3];
    builder.image_mesh(tex, &vertices, &indices, 0.8);

    let list = builder.drawlist();
    assert_eq!(list.vertices, vertices);
    assert_eq!(list.indices, indices);
    match &list.items[0] {
        gfx::DrawCmd::ImageMesh { tex: t, vb, ib, alpha } => {
            assert_eq!((*t, *alpha), (tex, 0.8));
            assert_eq!(
                (*vb, *ib),
                (gfx::VertexSpan { offset: 0, len: 4 }, gfx::IndexSpan { offset: 0, len: 6 })
            );
        }
        other => panic!("expected image mesh, found {other:?}"),
    }
}

#[test]
fn builder_indexes_unindexed_image_mesh_quad() {
    let mut builder = DrawListBuilder::new();
    let tex = gfx::ImageHandle(43);
    let vertices = [
        gfx::Vertex { x: 1.0, y: 2.0, u: 0.0, v: 0.0, rgba: u32::MAX },
        gfx::Vertex { x: 9.0, y: 2.0, u: 1.0, v: 0.0, rgba: u32::MAX },
        gfx::Vertex { x: 1.0, y: 8.0, u: 0.0, v: 1.0, rgba: u32::MAX },
        gfx::Vertex { x: 9.0, y: 8.0, u: 1.0, v: 1.0, rgba: u32::MAX },
    ];
    builder.image_mesh(tex, &vertices, &[], 0.8);

    let list = builder.drawlist();
    assert_eq!(list.vertices, vertices);
    assert_eq!(list.indices, [0, 1, 2, 2, 1, 3]);
    match &list.items[0] {
        gfx::DrawCmd::ImageMesh { tex: t, vb, ib, alpha } => {
            assert_eq!((*t, *alpha), (tex, 0.8));
            assert_eq!(
                (*vb, *ib),
                (gfx::VertexSpan { offset: 0, len: 4 }, gfx::IndexSpan { offset: 0, len: 6 })
            );
        }
        other => panic!("expected image mesh, found {other:?}"),
    }
}

#[test]
fn builder_manages_clip_stack() {
    let mut builder = DrawListBuilder::new();
    let clip_a = gfx::RectI::new(0, 0, 50, 60);
    let clip_b = gfx::RectI::new(10, 10, 20, 20);
    builder.clip_push(clip_a);
    builder.clip_push(clip_b);
    builder.clip_pop();
    builder.clip_pop();

    let list = builder.drawlist();
    assert!(matches!(list.items[0], gfx::DrawCmd::ClipPush { rect } if rect == clip_a));
    assert!(matches!(list.items[1], gfx::DrawCmd::ClipPush { rect } if rect == clip_b));
    assert!(matches!(list.items[2], gfx::DrawCmd::ClipPop));
    assert!(matches!(list.items[3], gfx::DrawCmd::ClipPop));
}

#[test]
fn builder_clear_drops_geometry_buffers() {
    let mut builder = DrawListBuilder::new();
    {
        let list = builder.drawlist_mut();
        list.vertices.push(gfx::Vertex { x: 1.0, y: 2.0, u: 0.0, v: 0.0, rgba: 0xFFFF_FFFF });
        list.indices.extend_from_slice(&[0, 1, 2]);
    }
    builder.rrect(
        gfx::RectF::new(0.0, 0.0, 10.0, 10.0),
        [2.0, 2.0, 2.0, 2.0],
        gfx::Color::rgba(0.3, 0.4, 0.5, 1.0),
    );

    builder.clear();

    let list = builder.drawlist();
    assert!(list.items.is_empty());
    assert!(list.vertices.is_empty());
    assert!(list.indices.is_empty());
}

#[test]
fn clip_intersection() {
    let mut builder = DrawListBuilder::new();
    builder.clip_push(gfx::RectI::new(0, 0, 100, 100));
    builder.solid(
        gfx::VertexSpan { offset: 0, len: 3 },
        gfx::IndexSpan { offset: 0, len: 3 },
        gfx::Color::rgba(1.0, 0.0, 0.0, 1.0),
    );
    builder.clip_push(gfx::RectI::new(50, 50, 100, 100));
    builder.solid(
        gfx::VertexSpan { offset: 0, len: 6 },
        gfx::IndexSpan { offset: 0, len: 6 },
        gfx::Color::rgba(0.0, 1.0, 0.0, 1.0),
    );
    builder.clip_pop();
    builder.solid(
        gfx::VertexSpan { offset: 0, len: 6 },
        gfx::IndexSpan { offset: 0, len: 6 },
        gfx::Color::rgba(0.0, 0.0, 1.0, 1.0),
    );

    let prepared = prepare_draws(&builder.into_inner());
    assert_eq!(prepared.len(), 3);
    let c0 = prepared[0].clip.expect("outer clip");
    assert_eq!((c0.x, c0.y, c0.w, c0.h), (0, 0, 100, 100));
    let c1 = prepared[1].clip.expect("intersected clip");
    assert_eq!((c1.x, c1.y, c1.w, c1.h), (50, 50, 50, 50));
    let c2 = prepared[2].clip.expect("restored clip");
    assert_eq!((c2.x, c2.y, c2.w, c2.h), (0, 0, 100, 100));
}

#[test]
fn stable_sort_batches() {
    let img1 = gfx::ImageHandle(1);
    let img2 = gfx::ImageHandle(2);
    let draws = vec![
        PreparedDraw {
            cmd: gfx::DrawCmd::Image {
                tex: img2,
                dst: gfx::RectF::new(0.0, 0.0, 1.0, 1.0),
                src: gfx::RectF::new(0.0, 0.0, 1.0, 1.0),
                alpha: 1.0,
            },
            clip: None,
        },
        PreparedDraw {
            cmd: gfx::DrawCmd::Solid {
                vb: gfx::VertexSpan { offset: 0, len: 3 },
                ib: gfx::IndexSpan { offset: 0, len: 3 },
                color: gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
            },
            clip: Some(gfx::RectI::new(0, 0, 10, 10)),
        },
        PreparedDraw {
            cmd: gfx::DrawCmd::Image {
                tex: img1,
                dst: gfx::RectF::new(0.0, 0.0, 1.0, 1.0),
                src: gfx::RectF::new(0.0, 0.0, 1.0, 1.0),
                alpha: 1.0,
            },
            clip: None,
        },
    ];

    let sorted = sort_for_batching(draws);

    match sorted[0].cmd {
        gfx::DrawCmd::Solid { .. } => {}
        _ => panic!("expected solid first"),
    }
    match sorted[1].cmd {
        gfx::DrawCmd::Image { tex, .. } => assert_eq!(tex.0, 1),
        _ => panic!("expected image"),
    }
    match sorted[2].cmd {
        gfx::DrawCmd::Image { tex, .. } => assert_eq!(tex.0, 2),
        _ => panic!("expected image"),
    }
}
