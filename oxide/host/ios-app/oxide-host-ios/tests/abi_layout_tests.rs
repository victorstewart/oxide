use oxide_host_ios::OxideHostStats;
use std::mem::{align_of, size_of};

fn assert_layout<T>(name: &str, expected_size: usize, expected_align: usize)
{
   assert_eq!(size_of::<T>(), expected_size, "{name} ABI size changed");
   assert_eq!(align_of::<T>(), expected_align, "{name} ABI alignment changed");
}

fn source_between<'a>(source: &'a str, start: &str, end: &str) -> &'a str
{
   source
      .split(start)
      .nth(1)
      .unwrap_or_else(|| panic!("missing source marker `{}`", start))
      .split(end)
      .next()
      .unwrap_or_else(|| panic!("missing source end marker `{}`", end))
}

#[test]
fn ios_host_camera_typedefs_keep_abi_static_asserts()
{
   let app = include_str!("../src/ios/app.m");
   assert!(app.contains("_Static_assert(sizeof(OxCameraFrame) == 72"));
   assert!(app.contains("_Static_assert(_Alignof(OxCameraFrame) == 8"));
   assert!(app.contains("_Static_assert(sizeof(OxCameraAudio) == 32"));
   assert!(app.contains("_Static_assert(_Alignof(OxCameraAudio) == 8"));
   assert!(app.contains("_Static_assert(sizeof(OxCameraRecordEvent) == 64"));
   assert!(app.contains("_Static_assert(_Alignof(OxCameraRecordEvent) == 8"));
}

#[test]
fn ios_host_stats_layout_is_frozen()
{
   assert_layout::<OxideHostStats>("OxideHostStats", 512, 8);

   let app = include_str!("../src/ios/app.m");
   assert!(app.contains("_Static_assert(sizeof(oxide_host_stats_t) == 512"));
   assert!(app.contains("_Static_assert(_Alignof(oxide_host_stats_t) == 8"));
   assert!(app.contains("_Static_assert(sizeof(oxide_host_camera_tick_perf_t) == 48"));
   assert!(app.contains("_Static_assert(_Alignof(oxide_host_camera_tick_perf_t) == 8"));
   assert!(app.contains("_Static_assert(sizeof(oxide_host_app_debug_perf_t) == 60"));
   assert!(app.contains("_Static_assert(_Alignof(oxide_host_app_debug_perf_t) == 4"));

   let swift = include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/../App/PerfShared/OxideUIKitBenchmarkRuntime.swift"
   ));
   let stats = source_between(swift, "private struct OxideHostStats", "private struct OxideHostCameraTickPerf");
   assert!(stats.contains("var hostIdleSkippedFrames: UInt64 = 0"));
   assert!(stats.contains("var hostSubmittedFrames: UInt64 = 0"));
   assert!(stats.contains("var hostFrameDirty: UInt8 = 0"));
   assert!(stats.contains("var hostSettleFramesRemaining: UInt8 = 0"));
}

#[test]
fn ios_host_private_camera_snapshots_keep_abi_static_asserts()
{
   let host = include_str!("../src/lib.rs");
   assert!(host.contains("core::mem::size_of::<OxideCamPerfSnapshot>()"));
   assert!(host.contains("core::mem::align_of::<OxideCamPerfSnapshot>()"));
   assert!(host.contains("core::mem::size_of::<OxideCamContractSnapshot>()"));
   assert!(host.contains("core::mem::align_of::<OxideCamContractSnapshot>()"));

   let camera = include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/../../../crates/platform-ios/src/ios/camera.m"
   ));
   assert!(camera.contains("_Static_assert(sizeof(struct OxideCamPerfSnapshot) == 208"));
   assert!(camera.contains("_Static_assert(_Alignof(struct OxideCamPerfSnapshot) == 8"));
   assert!(camera.contains("_Static_assert(sizeof(struct OxideCamContractSnapshot) == 20"));
   assert!(camera.contains("_Static_assert(_Alignof(struct OxideCamContractSnapshot) == 4"));
}
