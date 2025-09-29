use oxideui_host_ios::oxideui_host_app_present;

#[test]
fn release_path_stub() {
    unsafe {
        assert_eq!(oxideui_host_app_present(core::ptr::null_mut(), core::ptr::null_mut()), -1);
    }
}
