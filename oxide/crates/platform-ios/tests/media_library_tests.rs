use oxide_platform_api::media_library::AssetId;
use oxide_platform_ios::IosMediaLibraryManager;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

static FULL_IMAGE_RGBA_CALLS: AtomicUsize = AtomicUsize::new(0);

#[repr(C)]
pub struct TestOxideImageData {
    data_ptr: *const u8,
    data_len: usize,
    width: u32,
    height: u32,
    row_bytes: usize,
}

fn reset_stub_state() {
    FULL_IMAGE_RGBA_CALLS.store(0, Ordering::SeqCst);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn oxide_media_load_full_image_rgba(
    identifier_ptr: *const u8,
    identifier_len: usize,
    out_image: *mut TestOxideImageData,
) -> i32 {
    FULL_IMAGE_RGBA_CALLS.fetch_add(1, Ordering::SeqCst);
    let identifier = unsafe {
        std::str::from_utf8(std::slice::from_raw_parts(identifier_ptr, identifier_len))
            .expect("valid asset id")
    };
    assert_eq!(identifier, "display-asset");
    assert!(!out_image.is_null());

    let mut bytes = vec![1_u8, 2, 3, 4];
    let data_ptr = bytes.as_mut_ptr();
    let data_len = bytes.len();
    std::mem::forget(bytes);

    unsafe {
        (*out_image).data_ptr = data_ptr.cast_const();
        (*out_image).data_len = data_len;
        (*out_image).width = 1;
        (*out_image).height = 1;
        (*out_image).row_bytes = 4;
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn oxide_media_free_image_data(data_ptr: *const u8, data_len: usize) {
    if data_ptr.is_null() || data_len == 0 {
        return;
    }
    unsafe {
        drop(Vec::from_raw_parts(data_ptr as *mut u8, data_len, data_len));
    }
}

#[test]
fn display_image_loader_reuses_full_rgba_loader_until_cached_variant_exists() {
    reset_stub_state();

    let manager = IosMediaLibraryManager::default();
    let image = manager
        .load_display_image_bgra_data_if_available(&AssetId(String::from("display-asset")))
        .expect("display image request should succeed")
        .expect("display image should be returned");

    assert_eq!(FULL_IMAGE_RGBA_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(image.width, 1);
    assert_eq!(image.height, 1);
    assert_eq!(image.row_bytes, 4);
    assert_eq!(image.bgra, vec![1_u8, 2, 3, 4]);
}

#[test]
fn media_library_permission_status_stays_lazy_until_request() {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ios/host_services.m"),
    )
    .expect("read host_services.m");

    let oxide_status_start = source
        .find("static uint32_t oxide_media_library_permission_status(void) {")
        .expect("oxide status function");
    let oxide_status_end = source
        .find("static int32_t nametag_media_library_permission_status(void) {")
        .expect("nametag status function");
    let oxide_status_body = &source[oxide_status_start..oxide_status_end];
    assert!(
        !oxide_status_body.contains("current_photo_authorization")
            && !oxide_status_body.contains("PHPhotoLibrary"),
        "oxide media-library status must stay lazy until explicit request"
    );

    let nametag_status_start = oxide_status_end;
    let nametag_status_end = source
        .find("static void emit_location_permission_updates(void) {")
        .expect("location updates function");
    let nametag_status_body = &source[nametag_status_start..nametag_status_end];
    assert!(
        !nametag_status_body.contains("current_photo_authorization")
            && !nametag_status_body.contains("PHPhotoLibrary"),
        "nametag media-library status must stay lazy until explicit request"
    );
}

#[test]
fn nametag_bootstrap_permission_sync_skips_media_library() {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ios/host_services.m"),
    )
    .expect("read host_services.m");

    let publish_start = source
        .find("void nametag_ios_publish_permissions(void) {")
        .expect("nametag publish function");
    let publish_end = source
        .find("void nametag_ios_request_permission(int32_t domain) {")
        .expect("nametag request function");
    let publish_body = &source[publish_start..publish_end];
    assert!(
        !publish_body.contains("kNametagPermissionDomainMediaLibrary"),
        "nametag bootstrap permission sync must not probe Photos at launch"
    );
}
