use oxideui_permissions::PermissionState;
use oxideui_platform_api::{LocationEvent, LocationReading, PermissionDomain, PermissionStatus};
use oxideui_ui_core::scenes::Router;
use oxideui_ui_core::SensorBridgeConfig;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

struct DummyUploader;
impl oxideui_ui_core::elements::ImageUploader for DummyUploader {
    fn create_a8(
        &mut self,
        _w: u32,
        _h: u32,
        _data: &[u8],
        _row_bytes: usize,
    ) -> oxideui_renderer_api::ImageHandle {
        oxideui_renderer_api::ImageHandle(0)
    }

    fn update_a8(
        &mut self,
        _handle: oxideui_renderer_api::ImageHandle,
        _x: u32,
        _y: u32,
        _w: u32,
        _h: u32,
        _data: &[u8],
        _row_bytes: usize,
    ) {
    }
}

fn sample_reading(ts: u64) -> LocationReading {
    LocationReading {
        latitude_deg: 1.0,
        longitude_deg: 2.0,
        altitude_m: 0.0,
        horizontal_accuracy_m: 1.0,
        vertical_accuracy_m: 1.0,
        speed_mps: 0.0,
        course_deg: 0.0,
        timestamp_ms: ts,
    }
}

#[test]
fn router_exposes_sensor_snapshot() {
    let now = Arc::new(AtomicU64::new(0));
    let clock = {
        let now = Arc::clone(&now);
        Arc::new(move || now.load(Ordering::SeqCst))
    };
    let bridge = Arc::new(oxideui_permissions::SensorBridge::new_with_config(
        clock,
        SensorBridgeConfig::default(),
    ));
    bridge.update_permission(PermissionState::new(
        PermissionDomain::Location,
        PermissionStatus::Authorized,
        0,
    ));
    bridge.handle_location_event(LocationEvent::Update(sample_reading(0)));

    let uploader = DummyUploader;
    let mut router = Router::new(uploader);
    router.sensors_bind(&bridge);

    let snap = router.sensors_snapshot().expect("snapshot");
    assert_eq!(snap.location.history.len(), 1);
}
