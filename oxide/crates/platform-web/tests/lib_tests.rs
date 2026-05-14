use oxide_platform_api::{HapticPattern, Platform};
use oxide_platform_web::{hex_decode, hex_encode, refresh_rate_hz_from_frame_deltas, WebPlatform};

#[test]
fn hex_round_trips_secret_bytes() {
    let bytes = [0_u8, 1, 2, 15, 16, 127, 128, 255];
    let encoded = hex_encode(&bytes);
    assert_eq!(encoded, "0001020f107f80ff");
    assert_eq!(hex_decode(&encoded).ok(), Some(bytes.to_vec()));
}

#[test]
fn hex_decode_rejects_invalid_input() {
    assert!(hex_decode("abc").is_err());
    assert!(hex_decode("zz").is_err());
}

#[test]
fn refresh_rate_estimator_uses_median_frame_delta() {
    assert_eq!(refresh_rate_hz_from_frame_deltas(&[]), 60);
    assert_eq!(refresh_rate_hz_from_frame_deltas(&[16.6, 16.7, 200.0, 16.8, 16.7]), 60);
    assert_eq!(refresh_rate_hz_from_frame_deltas(&[8.3, 8.4, 8.3, 8.5, 50.0]), 119);
    assert_eq!(refresh_rate_hz_from_frame_deltas(&[1.0, 1.0, 1.0]), 240);
}

#[test]
fn platform_reports_browser_caps_and_unsupported_os_services() {
    let platform = WebPlatform::new();
    let caps = platform.device_caps();
    assert_eq!(caps.max_framerate_hz, 60);
    assert_eq!(caps.native_scale, 1.0);
    assert!(platform.capabilities().is_empty());
    assert_eq!(
        platform.permissions().status(oxide_platform_api::PermissionDomain::Contacts),
        oxide_platform_api::PermissionStatus::Denied,
    );
    assert!(platform
        .camera()
        .start_stream(oxide_platform_api::CameraConfig::default(), Box::new(|_| {}), None,)
        .is_err());
}

#[test]
fn native_fallback_keeps_browser_only_services_explicit() {
    let platform = WebPlatform::new();
    let location = platform.location();
    assert!(location.start(oxide_platform_api::LocationOptions::default()).is_err());
    location.request_once();
    assert_eq!(location.last(), None);
    assert!(location.history().is_empty());
    assert!(location.region_tracker().is_none());
    assert!(location.set_accuracy(oxide_platform_api::LocationAccuracy::Precise).is_err());

    let network = platform.network_status();
    let status = network.current_status();
    assert!(!status.is_connected);
    assert!(status.interfaces.is_empty());
    network.subscribe(Box::new(|_| {}));

    platform.permissions().request(oxide_platform_api::PermissionDomain::Location);
    platform.permissions().subscribe(Box::new(|_, _| {}));
    assert_eq!(
        platform.permissions().status(oxide_platform_api::PermissionDomain::Location),
        oxide_platform_api::PermissionStatus::Denied,
    );

    assert!(platform.web_view_service().create_view("about:blank", Box::new(|_| {})).is_err());
}

#[test]
fn platform_clipboard_provider_caches_written_text() {
    let platform = WebPlatform::new();
    platform.clipboard_set("oxide-web");
    assert_eq!(platform.clipboard_get().as_deref(), Some("oxide-web"));
}

#[test]
fn haptics_service_accepts_all_patterns() {
    let platform = WebPlatform::new();
    let haptics = platform.haptics();
    haptics.play(HapticPattern::ImpactLight);
    haptics.play(HapticPattern::NotificationError);
}
