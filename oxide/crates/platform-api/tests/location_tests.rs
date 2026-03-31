use oxide_platform_api::{
    GeoHash, GeoRegion, LocationAccuracy, LocationEvent, LocationOptions, LocationReading,
    MotionSample, PlatformError,
};

#[test]
fn location_options_default_balanced() {
    let opts = LocationOptions::default();
    assert_eq!(opts.accuracy, LocationAccuracy::Balanced);
    assert_eq!(opts.distance_filter_m, 0.0);
    assert!(!opts.allow_background_updates);
    assert!(!opts.precise);
}

#[test]
fn location_accuracy_supports_low_power_mode() {
    assert_eq!(LocationAccuracy::LowPower, LocationAccuracy::LowPower);
}

#[test]
fn location_event_update_clones_reading() {
    let reading = LocationReading {
        latitude_deg: 1.0,
        longitude_deg: 2.0,
        altitude_m: 3.0,
        horizontal_accuracy_m: 4.0,
        vertical_accuracy_m: 5.0,
        speed_mps: 6.0,
        course_deg: 7.0,
        timestamp_ms: 8,
    };
    match LocationEvent::Update(reading) {
        LocationEvent::Update(r) => {
            assert_eq!(r.latitude_deg, 1.0);
            assert_eq!(r.timestamp_ms, 8);
        }
        _ => panic!("unexpected variant"),
    }
}

#[test]
fn motion_sample_supports_optional_fields() {
    let sample =
        MotionSample { pressure_pa: None, relative_altitude_m: Some(1.25), timestamp_ms: 42 };
    assert!(sample.pressure_pa.is_none());
    assert_eq!(sample.relative_altitude_m, Some(1.25));
    assert_eq!(sample.timestamp_ms, 42);
}

#[test]
fn location_event_error_wraps_platform_error() {
    let err = PlatformError::Unknown("unit-test".to_string());
    match LocationEvent::Error(err.clone()) {
        LocationEvent::Error(e) => {
            assert_eq!(format!("{}", e), "unknown: unit-test");
        }
        _ => panic!("unexpected variant"),
    }
}

#[test]
fn location_event_region_variants() {
    let region = GeoRegion { hash: GeoHash(42), center: (37.0, -122.0), radius_m: 50.0 };
    match LocationEvent::EnteredRegion(region) {
        LocationEvent::EnteredRegion(r) => {
            assert_eq!(r.hash.0, 42);
            assert!((r.center.0 - 37.0).abs() < f64::EPSILON);
        }
        _ => panic!("expected entered region"),
    }
    match LocationEvent::ExitedRegion(region) {
        LocationEvent::ExitedRegion(r) => assert_eq!(r.radius_m, 50.0),
        _ => panic!("expected exited region"),
    }
}
