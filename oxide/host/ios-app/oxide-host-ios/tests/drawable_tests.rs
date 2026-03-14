use oxide_host_ios::oxide_host_app_present;

#[test]
fn release_path_stub() {
    assert_eq!(oxide_host_app_present(core::ptr::null_mut(), core::ptr::null_mut()), -1);
}
