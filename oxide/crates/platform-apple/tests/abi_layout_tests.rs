use oxide_platform_apple::camera::{CamFormat, CamPixFmt};
use oxide_platform_apple::{
   AppleBleScanConfig, AppleBleScanInfo, AppleCamAudio, AppleCamFrame, AppleCamPhotoEvent,
   AppleCamRecordEvent, AppleHttpEvent, AppleHttpHeader, AppleLocationConfig, AppleLocationSample,
   AppleMediaAsset, AppleMediaImageData, AppleMotionSample,
};
use std::mem::{align_of, size_of};

fn assert_layout<T>(name: &str, expected_size: usize, expected_align: usize)
{
   assert_eq!(size_of::<T>(), expected_size, "{name} ABI size changed");
   assert_eq!(align_of::<T>(), expected_align, "{name} ABI alignment changed");
}

#[test]
fn shared_apple_ffi_layouts_are_frozen()
{
   assert_layout::<AppleHttpHeader>("AppleHttpHeader", 32, 8);
   assert_layout::<AppleHttpEvent>("AppleHttpEvent", 72, 8);
   assert_layout::<AppleBleScanConfig>("AppleBleScanConfig", 24, 8);
   assert_layout::<AppleBleScanInfo>("AppleBleScanInfo", 80, 8);
   assert_layout::<AppleCamFrame>("AppleCamFrame", 72, 8);
   assert_layout::<AppleCamAudio>("AppleCamAudio", 32, 8);
   assert_layout::<AppleCamRecordEvent>("AppleCamRecordEvent", 64, 8);
   assert_layout::<AppleCamPhotoEvent>("AppleCamPhotoEvent", 104, 8);
   assert_layout::<CamFormat>("CamFormat", 20, 4);
   assert_layout::<CamPixFmt>("CamPixFmt", 12, 4);
   assert_layout::<AppleLocationSample>("AppleLocationSample", 64, 8);
   assert_layout::<AppleLocationConfig>("AppleLocationConfig", 24, 8);
   assert_layout::<AppleMotionSample>("AppleMotionSample", 32, 8);
   assert_layout::<AppleMediaAsset>("AppleMediaAsset", 56, 8);
   assert_layout::<AppleMediaImageData>("AppleMediaImageData", 32, 8);
}

#[test]
fn shared_apple_objc_bridges_keep_abi_static_asserts()
{
   let http = include_str!("../src/apple/http.m");
   assert!(http.contains("_Static_assert(sizeof(struct OxideHttpHeader) == 32"));
   assert!(http.contains("_Static_assert(_Alignof(struct OxideHttpHeader) == 8"));
   assert!(http.contains("_Static_assert(sizeof(struct OxideHttpEvent) == 72"));
   assert!(http.contains("_Static_assert(_Alignof(struct OxideHttpEvent) == 8"));

   let bluetooth = include_str!("../src/apple/bluetooth.m");
   assert!(bluetooth.contains("_Static_assert(sizeof(struct OxideBleScanConfig) == 24"));
   assert!(bluetooth.contains("_Static_assert(_Alignof(struct OxideBleScanConfig) == 8"));
   assert!(bluetooth.contains("_Static_assert(sizeof(struct OxideBleScanInfo) == 80"));
   assert!(bluetooth.contains("_Static_assert(_Alignof(struct OxideBleScanInfo) == 8"));
}
