use oxide_renderer_api as gfx;
use oxide_ui_core::DrawListBuilder;

#[test]
fn builder_records_advanced_draws() {
    let mut builder = DrawListBuilder::new();
    let tex = gfx::ImageHandle(9);
    let rect = gfx::RectF::new(4.0, 8.0, 64.0, 32.0);
    let slice = gfx::Insets::new(3.0, 4.0, 5.0, 6.0);
    builder.nine_slice(tex, rect, slice, 0.75);

    let bg_rect = gfx::RectF::new(0.0, 0.0, 100.0, 80.0);
    builder.camera_bg(bg_rect, gfx::Color::rgba(0.1, 0.2, 0.3, 0.9), 0.8, true, false, 12.0);

    builder.spinner([16.0, 24.0], 18.0, 2.0, 0.5, 1.0);

    let list = builder.drawlist();
    assert_eq!(list.items.len(), 3);
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
        gfx::DrawCmd::Spinner { center, radius, thickness, phase, alpha } => {
            assert_eq!(*center, [16.0, 24.0]);
            assert_eq!((*radius, *thickness, *phase, *alpha), (18.0, 2.0, 0.5, 1.0));
        }
        other => panic!("expected spinner, found {other:?}"),
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
