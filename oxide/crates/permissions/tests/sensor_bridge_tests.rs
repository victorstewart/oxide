use oxide_permissions::SensorBridge;
use oxide_platform_api::{
    BluetoothEvent, LocationEvent, LocationReading, MotionSample, PermissionDomain,
    PermissionStatus, PushNotification, PushPresentation, PushToken,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

fn test_clock_pair() -> (Arc<AtomicU64>, Arc<dyn Fn() -> u64 + Send + Sync>) {
    let now = Arc::new(AtomicU64::new(0));
    let clock_now = Arc::clone(&now);
    let clock = Arc::new(move || clock_now.load(Ordering::SeqCst));
    (now, clock)
}

fn sample_location(ts: u64) -> LocationReading {
    LocationReading {
        latitude_deg: 10.0,
        longitude_deg: 20.0,
        altitude_m: 5.0,
        horizontal_accuracy_m: 1.0,
        vertical_accuracy_m: 1.5,
        speed_mps: 0.1,
        course_deg: 90.0,
        timestamp_ms: ts,
    }
}

fn sample_motion(ts: u64) -> MotionSample {
    MotionSample { pressure_pa: Some(101_325.0), relative_altitude_m: Some(1.2), timestamp_ms: ts }
}

fn permit(bridge: &SensorBridge, domain: PermissionDomain, status: PermissionStatus) {
    bridge.update_permission(oxide_permissions::PermissionState::new(domain, status, 0));
}

#[test]
fn permission_status_tracks_every_domain_slot() {
   let (_, clock) = test_clock_pair();
   let bridge = SensorBridge::with_clock(clock);
   let cases = [
      (PermissionDomain::Notifications, PermissionStatus::NotDetermined),
      (PermissionDomain::Location, PermissionStatus::Authorized),
      (PermissionDomain::Camera, PermissionStatus::Denied),
      (PermissionDomain::Contacts, PermissionStatus::Limited),
      (PermissionDomain::Bluetooth, PermissionStatus::Authorized),
      (PermissionDomain::Motion, PermissionStatus::Limited),
      (PermissionDomain::Microphone, PermissionStatus::Denied),
      (PermissionDomain::MediaLibrary, PermissionStatus::Authorized),
   ];

   for (domain, status) in cases {
      permit(&bridge, domain, status);
   }

   for (domain, status) in cases {
      assert_eq!(bridge.permission_status(domain), Some(status));
   }
}

#[test]
fn location_history_prunes_by_length_and_age() {
    let (now, clock) = test_clock_pair();
    let config = oxide_permissions::sensors::SensorBridgeConfig {
        location_history_max: 3,
        location_max_age_ms: 30,
        motion_history_max: 2,
        bluetooth_max_age_ms: 1_000,
        push_history_max: 4,
        bluetooth_cache_max: 8,
    };
    let bridge = SensorBridge::new_with_config(clock, config);
    permit(&bridge, PermissionDomain::Location, PermissionStatus::Authorized);

    for i in 0..4 {
        now.store(i * 20, Ordering::SeqCst);
        bridge.handle_location_event(LocationEvent::Update(sample_location(i * 20)));
    }

    let history = bridge.location_history();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].timestamp_ms, 40);
    assert_eq!(history[1].timestamp_ms, 60);
}

#[test]
fn location_events_ignored_without_permission() {
    let (_, clock) = test_clock_pair();
    let bridge = SensorBridge::with_clock(clock);
    bridge.handle_location_event(LocationEvent::Update(sample_location(0)));
    assert!(bridge.location_history().is_empty());
}

#[test]
fn motion_history_clears_on_revocation() {
    let (now, clock) = test_clock_pair();
    let config = oxide_permissions::sensors::SensorBridgeConfig {
        motion_history_max: 4,
        ..Default::default()
    };
    let bridge = SensorBridge::new_with_config(clock, config);
    permit(&bridge, PermissionDomain::Motion, PermissionStatus::Authorized);

    now.store(10, Ordering::SeqCst);
    bridge.handle_motion_sample(sample_motion(10));
    assert_eq!(bridge.motion_history().len(), 1);

    permit(&bridge, PermissionDomain::Motion, PermissionStatus::Denied);
    assert!(bridge.motion_history().is_empty());
    assert!(bridge.last_motion().is_none());
}

#[test]
fn bluetooth_prunes_by_age_limit() {
    let (now, clock) = test_clock_pair();
    let config = oxide_permissions::sensors::SensorBridgeConfig {
        bluetooth_max_age_ms: 100,
        ..Default::default()
    };
    let bridge = SensorBridge::new_with_config(clock, config);
    permit(&bridge, PermissionDomain::Bluetooth, PermissionStatus::Authorized);

    let info = oxide_platform_api::PeripheralInfo {
        id: 1,
        name: Some("dev".into()),
        rssi_dbm: -40,
        advertisement: oxide_platform_api::AdvertisementData {
            services: Vec::new(),
            manufacturer_data: None,
            connectable: true,
        },
    };
    now.store(0, Ordering::SeqCst);
    bridge.handle_bluetooth_event(BluetoothEvent::Discovered(info.clone()));
    now.store(200, Ordering::SeqCst);
    bridge.prune_bluetooth();

    let snapshot = bridge.bluetooth_snapshot();
    assert!(snapshot.devices.is_empty());
}

#[test]
fn bluetooth_discovery_snapshot_preserves_payload_fields() {
   let (now, clock) = test_clock_pair();
   let bridge = SensorBridge::with_clock(clock);
   permit(&bridge, PermissionDomain::Bluetooth, PermissionStatus::Authorized);

   let service = oxide_platform_api::BleUuid([9; 16]);
   let info = oxide_platform_api::PeripheralInfo {
      id: 88,
      name: Some("bench-device".into()),
      rssi_dbm: -51,
      advertisement: oxide_platform_api::AdvertisementData {
         services: vec![service],
         manufacturer_data: Some(vec![1, 2, 3, 4]),
         connectable: true,
      },
   };
   now.store(42, Ordering::SeqCst);
   bridge.handle_bluetooth_event(BluetoothEvent::Discovered(info));

   let snapshot = bridge.bluetooth_snapshot();
   assert_eq!(snapshot.devices.len(), 1);
   let entry = &snapshot.devices[0];
   assert_eq!(entry.last_seen_ms, 42);
   assert_eq!(entry.peripheral.id, 88);
   assert_eq!(entry.peripheral.name.as_deref(), Some("bench-device"));
   assert_eq!(entry.peripheral.rssi_dbm, -51);
   assert_eq!(entry.peripheral.advertisement.services.as_slice(), &[service]);
   assert_eq!(
      entry.peripheral.advertisement.manufacturer_data.as_deref(),
      Some(&[1, 2, 3, 4][..])
   );
   assert!(entry.peripheral.advertisement.connectable);
}

#[test]
fn push_notifications_tracked_when_authorized() {
    let (_, clock) = test_clock_pair();
    let bridge = SensorBridge::with_clock(clock);
    permit(&bridge, PermissionDomain::Notifications, PermissionStatus::Authorized);

    bridge.set_push_token(Some(PushToken {
        provider: oxide_platform_api::PushProvider::Apns,
        value: "abc".into(),
    }));
    let mut notification = PushNotification {
        user_info: Default::default(),
        badge: Some(1),
        sound: None,
        presentation: PushPresentation::Foreground,
    };
    bridge.handle_push_notification(notification.clone());
    notification.badge = Some(2);
    bridge.handle_push_notification(notification.clone());

    assert_eq!(bridge.push_notifications().len(), 2);
    assert_eq!(bridge.push_token().unwrap().value, "abc");

    permit(&bridge, PermissionDomain::Notifications, PermissionStatus::Denied);
    assert!(bridge.push_notifications().is_empty());
    assert!(bridge.push_token().is_none());
}
