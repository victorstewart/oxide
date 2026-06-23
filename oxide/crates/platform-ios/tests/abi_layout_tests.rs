use oxide_platform_ios::{
   OxideCamAudio, OxideCamFrame, OxideCamPhotoEvent, OxideCamRecordEvent, OxideContact,
   OxideContactEmail, OxideContactPhone, OxideContactsState, OxideLocationConfig,
   OxideLocationSample, OxideMotionSample,
};
use std::mem::{align_of, size_of};

fn assert_layout<T>(name: &str, expected_size: usize, expected_align: usize)
{
   assert_eq!(size_of::<T>(), expected_size, "{name} ABI size changed");
   assert_eq!(align_of::<T>(), expected_align, "{name} ABI alignment changed");
}

#[test]
fn ios_platform_ffi_layouts_are_frozen()
{
   assert_layout::<OxideCamFrame>("OxideCamFrame", 72, 8);
   assert_layout::<OxideCamAudio>("OxideCamAudio", 32, 8);
   assert_layout::<OxideCamRecordEvent>("OxideCamRecordEvent", 64, 8);
   assert_layout::<OxideCamPhotoEvent>("OxideCamPhotoEvent", 104, 8);
   assert_layout::<OxideLocationSample>("OxideLocationSample", 64, 8);
   assert_layout::<OxideLocationConfig>("OxideLocationConfig", 24, 8);
   assert_layout::<OxideMotionSample>("OxideMotionSample", 32, 8);
   assert_layout::<OxideContactsState>("OxideContactsState", 32, 8);
   assert_layout::<OxideContact>("OxideContact", 80, 8);
   assert_layout::<OxideContactPhone>("OxideContactPhone", 48, 8);
   assert_layout::<OxideContactEmail>("OxideContactEmail", 24, 8);
}

#[test]
fn ios_objc_bridges_keep_abi_static_asserts()
{
   let location = include_str!("../src/ios/location.m");
   assert!(location.contains("_Static_assert(sizeof(OxideLocationSample) == 64"));
   assert!(location.contains("_Static_assert(_Alignof(OxideLocationSample) == 8"));
   assert!(location.contains("_Static_assert(sizeof(OxideLocationConfig) == 24"));
   assert!(location.contains("_Static_assert(_Alignof(OxideLocationConfig) == 8"));

   let motion = include_str!("../src/ios/motion.m");
   assert!(motion.contains("_Static_assert(sizeof(OxideMotionSample) == 32"));
   assert!(motion.contains("_Static_assert(_Alignof(OxideMotionSample) == 8"));

   let camera = include_str!("../src/ios/camera.m");
   assert!(camera.contains("_Static_assert(sizeof(struct OxideCamFrame) == 72"));
   assert!(camera.contains("_Static_assert(_Alignof(struct OxideCamFrame) == 8"));
   assert!(camera.contains("_Static_assert(sizeof(struct OxideCamAudio) == 32"));
   assert!(camera.contains("_Static_assert(_Alignof(struct OxideCamAudio) == 8"));
   assert!(camera.contains("_Static_assert(sizeof(struct OxideCamRecordEvent) == 64"));
   assert!(camera.contains("_Static_assert(_Alignof(struct OxideCamRecordEvent) == 8"));
   assert!(camera.contains("_Static_assert(sizeof(struct OxideCamPhotoEvent) == 104"));
   assert!(camera.contains("_Static_assert(_Alignof(struct OxideCamPhotoEvent) == 8"));
   assert!(camera.contains("_Static_assert(sizeof(struct OxideCamPerfSnapshot) == 208"));
   assert!(camera.contains("_Static_assert(_Alignof(struct OxideCamPerfSnapshot) == 8"));
   assert!(camera.contains("_Static_assert(sizeof(struct OxideCamContractSnapshot) == 20"));
   assert!(camera.contains("_Static_assert(_Alignof(struct OxideCamContractSnapshot) == 4"));
}
