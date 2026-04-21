use oxide_platform_api::media_library::AssetId;
use oxide_platform_ios::IosMediaLibraryManager;
use std::sync::atomic::{AtomicUsize, Ordering};

static FULL_IMAGE_RGBA_CALLS: AtomicUsize = AtomicUsize::new(0);
static IF_AVAILABLE_CALLS: AtomicUsize = AtomicUsize::new(0);

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
    IF_AVAILABLE_CALLS.store(0, Ordering::SeqCst);
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
pub unsafe extern "C" fn oxide_media_load_full_image_rgba_if_available(
    _identifier_ptr: *const u8,
    _identifier_len: usize,
    _out_image: *mut TestOxideImageData,
) -> i32 {
    IF_AVAILABLE_CALLS.fetch_add(1, Ordering::SeqCst);
    -77
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
    assert_eq!(IF_AVAILABLE_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(image.width, 1);
    assert_eq!(image.height, 1);
    assert_eq!(image.row_bytes, 4);
    assert_eq!(image.bgra, vec![1_u8, 2, 3, 4]);
}
