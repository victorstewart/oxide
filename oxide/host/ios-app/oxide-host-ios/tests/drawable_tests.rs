use oxide_host_ios::oxide_host_app_frame_with_drawable;

#[test]
fn frame_with_drawable_stub() {
    assert_eq!(oxide_host_app_frame_with_drawable(1, 1, 1.0, core::ptr::null_mut()), -1);
}
