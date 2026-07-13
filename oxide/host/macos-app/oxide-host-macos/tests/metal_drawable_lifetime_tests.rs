#[test]
fn draw_rect_prepares_frame_before_acquiring_drawable() {
    let source = include_str!("../src/macos/app.m");
    let draw_rect = source.find("- (void)drawRect:").expect("drawRect");
    let tail = &source[draw_rect..];
    let prepare = tail.find("macos_app_prepare_frame").expect("prepare frame call");
    let acquire = tail.find("nextDrawable").expect("drawable acquisition");
    let submit =
        tail.find("macos_app_submit_prepared_frame_with_drawable").expect("prepared submit call");
    assert!(
        prepare < acquire && acquire < submit,
        "macOS host must build the frame before acquiring CAMetalDrawable and submit immediately after acquisition"
    );
}

#[test]
fn draw_rect_uses_timeout_capable_drawable_acquisition() {
    let source = include_str!("../src/macos/app.m");
    assert!(
        source.contains("layer.maximumDrawableCount = 3;")
            && source.contains("layer.allowsNextDrawableTimeout = YES;"),
        "macOS CAMetalLayer must allow nextDrawable timeout so drawable pressure skips instead of blocking indefinitely"
    );

    let draw_rect = source.find("- (void)drawRect:").expect("drawRect");
    let tail = &source[draw_rect..];
    let nil_drawable = tail.find("if (!drawable)").expect("nil drawable branch");
    let cancel = tail.find("macos_app_cancel_prepared_frame").expect("cancel prepared frame");
    let submit =
        tail.find("macos_app_submit_prepared_frame_with_drawable").expect("prepared submit call");
    assert!(
        nil_drawable < cancel && cancel < submit,
        "macOS host must cancel the prepared frame when drawable acquisition times out"
    );
}

#[test]
fn canceled_prepared_frame_retains_damage_for_retry() {
    let source = include_str!("../src/lib.rs");
    assert!(
        source.contains("fn retain_pending_damage_for_retry"),
        "macOS prepared-frame retry must have an explicit pending-damage retention helper"
    );

    let prepare = source.find("pub extern \"C\" fn macos_app_prepare_frame").expect("prepare");
    let submit = source
        .find("pub extern \"C\" fn macos_app_submit_prepared_frame_with_drawable")
        .expect("submit");
    let prepare_body = &source[prepare..submit];
    assert!(
        prepare_body.contains(
            "retain_pending_damage_for_retry(&mut app.pending_damage_rects, &mut damage_rects)"
        ),
        "newly prepared damage must be merged with damage retained from a skipped frame"
    );

    let cancel =
        source.find("pub extern \"C\" fn macos_app_cancel_prepared_frame").expect("cancel");
    let next = source[cancel..].find("fn macos_app_frame_inner").expect("frame inner") + cancel;
    let cancel_body = &source[cancel..next];
    assert!(
        cancel_body.contains("mark_frame_dirty(&mut app)") && !cancel_body.contains(".clear()"),
        "drawable-timeout cancellation must request a retry without dropping pending damage"
    );

    let submit_body = &source[submit..cancel];
    assert!(
        submit_body.contains("core::mem::take(&mut app.pending_damage_rects)")
            && submit_body.contains(
                "retain_pending_damage_for_retry(&mut app.pending_damage_rects, &mut damage_rects)"
            ),
        "submit failure must restore pending damage for the next drawable-backed frame"
    );
}

#[test]
fn native_frame_coalescing_reuses_app_storage() {
    let source = include_str!("../src/lib.rs");
    assert!(source.contains("coalesce_items: Vec<gfx_api::DrawCmd>"));
    assert!(source.contains("coalesce_adjacent_draws_reuse(dl, &mut app.coalesce_items)"));
    assert!(!source.contains("oxide_ui_core::coalesce_adjacent_draws(dl)"));
}

#[test]
fn native_damage_handoff_reuses_router_and_submit_storage() {
    let source = include_str!("../src/lib.rs");
    assert!(source.contains("damage_rects: Vec<gfx_api::RectI>"));
    assert!(source.contains("router.take_damage_into(&mut damage_rects)"));
    assert!(source.contains("damage_rects = damage_obj.rects"));
    assert!(source.contains("app.damage_rects = damage_rects"));
    assert!(!source.contains("router.take_damage()"));
}
