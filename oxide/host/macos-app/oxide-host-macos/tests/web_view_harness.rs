#[cfg(all(target_os = "macos", feature = "host-testing"))]
mod harness {
    use oxide_host_macos::{
        host_harness_reset, macos_app_init,
    };
    use oxide_platform_api::media_library::{AssetData, AssetId, AssetType, ImageFormat, ImageQuality};
    use oxide_platform_api::{
        CameraConfig, CameraFrame, CameraImage, Capabilities, CaptureMode, ColorSpace, FlashMode,
        HapticPattern, LocationAccuracy, LocationEvent, LocationOptions, LocationReading,
        PermissionDomain, PermissionStatus, PhotoEvent, PhotoOptions, Platform, PlatformError,
        RecordingContainer, RecordingDestination, RecordingEvent, RecordingOptions, StandardPath,
        TorchMode,
    };
    use oxide_platform_api::network_status::NetworkStatus;
    use oxide_platform_api::web_view::WebViewEvent;
    use std::ffi::c_void;
    use std::future::Future;
    use std::path::PathBuf;
    use std::pin::Pin;
    use std::sync::mpsc;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    use std::time::{Duration, Instant};

    type CFStringRef = *const c_void;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFRunLoopDefaultMode: CFStringRef;
        fn CFRunLoopRunInMode(mode: CFStringRef, seconds: f64, return_after_source_handled: u8) -> i32;
    }

    fn block_on_ready<F: Future>(future: F) -> F::Output {
        fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(core::ptr::null(), &VTABLE)
        }
        fn wake(_: *const ()) {}
        fn wake_by_ref(_: *const ()) {}
        fn drop(_: *const ()) {}
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
        let waker = unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) };
        let mut cx = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        match Pin::new(&mut future).poll(&mut cx) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("macOS WebView test future unexpectedly pending"),
        }
    }

    fn run_loop_once() {
        unsafe {
            let _ = CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.01, 1);
        }
    }

    fn recv_web_view_event(events: &mpsc::Receiver<WebViewEvent>) -> WebViewEvent {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match events.try_recv() {
                Ok(event) => return event,
                Err(mpsc::TryRecvError::Disconnected) => panic!("web view event channel disconnected"),
                Err(mpsc::TryRecvError::Empty) => {}
            }
            assert!(Instant::now() < deadline, "timed out waiting for WebView event");
            run_loop_once();
        }
    }

    fn recv_network_status(events: &mpsc::Receiver<NetworkStatus>) -> NetworkStatus {
        events
            .recv_timeout(Duration::from_secs(2))
            .expect("receive immediate network-status callback")
    }

    fn recv_camera_frame(events: &mpsc::Receiver<CameraFrame>) -> CameraFrame {
        events
            .recv_timeout(Duration::from_secs(5))
            .expect("receive live macOS camera frame")
    }

    fn recv_photo_event(events: &mpsc::Receiver<PhotoEvent>) -> PhotoEvent {
        events
            .recv_timeout(Duration::from_secs(5))
            .expect("receive live macOS photo event")
    }

    fn recv_recording_event(events: &mpsc::Receiver<RecordingEvent>) -> RecordingEvent {
        events
            .recv_timeout(Duration::from_secs(10))
            .expect("receive live macOS recording event")
    }

    fn recv_location_update(events: &mpsc::Receiver<LocationEvent>) -> LocationReading {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            match events.try_recv() {
                Ok(LocationEvent::Update(reading)) => return reading,
                Ok(LocationEvent::Error(error)) => panic!("live macOS location update failed: {:?}", error),
                Ok(LocationEvent::EnteredRegion(_)) | Ok(LocationEvent::ExitedRegion(_)) => {}
                Err(mpsc::TryRecvError::Disconnected) => panic!("location event channel disconnected"),
                Err(mpsc::TryRecvError::Empty) => {}
            }
            assert!(Instant::now() < deadline, "timed out waiting for live macOS location update");
            run_loop_once();
        }
    }

    fn assert_valid_permission_status(status: PermissionStatus) {
        assert!(matches!(
            status,
            PermissionStatus::NotDetermined
                | PermissionStatus::Denied
                | PermissionStatus::Limited
                | PermissionStatus::Authorized
        ));
    }

    fn assert_network_status_shape(status: NetworkStatus) {
        if !status.is_connected {
            assert!(
                status.interfaces.is_empty(),
                "disconnected network status should not report active interfaces: {:?}",
                status
            );
        }
    }

    fn assert_location_reading(reading: LocationReading) {
        assert!(reading.latitude_deg.is_finite(), "latitude should be finite");
        assert!(reading.longitude_deg.is_finite(), "longitude should be finite");
        assert!(
            (-90.0..=90.0).contains(&reading.latitude_deg),
            "latitude should be in range: {}",
            reading.latitude_deg
        );
        assert!(
            (-180.0..=180.0).contains(&reading.longitude_deg),
            "longitude should be in range: {}",
            reading.longitude_deg
        );
        assert!(reading.timestamp_ms > 0, "location timestamp should be non-zero");
        assert!(
            reading.horizontal_accuracy_m.is_finite() && reading.horizontal_accuracy_m >= 0.0,
            "horizontal accuracy should be finite and non-negative"
        );
    }

    fn assert_camera_frame_shape(frame: &CameraFrame) {
        assert!(frame.size.0 > 0 && frame.size.1 > 0, "camera frame dimensions must be non-zero");
        match &frame.image {
            CameraImage::Nv12 {
                y_plane,
                uv_plane,
                stride_y,
                stride_uv,
                bit_depth,
                ..
            } => {
                assert_eq!(*bit_depth, 8, "macOS host currently publishes 8-bit NV12 frames");
                assert!(!y_plane.is_empty(), "NV12 Y plane should not be empty");
                assert!(!uv_plane.is_empty(), "NV12 UV plane should not be empty");
                assert!(*stride_y >= frame.size.0, "Y stride should cover frame width");
                assert!(*stride_uv >= frame.size.0, "UV stride should cover frame width");
            }
            CameraImage::Gpu { .. } => panic!("macOS camera host should publish Oxide-owned NV12 frames"),
        }
    }

    fn verify_basic_platform_services(platform: &dyn Platform) {
        platform.request_redraw();
        platform.set_high_refresh(true);
        platform.set_high_refresh(false);
        platform.set_idle_timer_disabled(true);
        platform.set_idle_timer_disabled(false);
        platform.ime_show();
        platform.ime_hide();
        assert!(!platform.is_simulation());

        for path in [StandardPath::Documents, StandardPath::Cache, StandardPath::Temporary] {
            let value = platform.paths().get(path);
            assert!(!value.is_empty(), "standard path should not be empty: {:?}", path);
            let path = std::path::Path::new(&value);
            assert!(path.is_absolute(), "standard path should be absolute: {value}");
            assert!(path.is_dir(), "standard path should exist as a directory: {value}");
        }

        let t0 = platform.time().monotonic_now();
        let t1 = platform.time().monotonic_now();
        assert!(t1 >= t0, "monotonic clock should not move backwards");

        let caps = platform.device_caps();
        assert!(caps.max_framerate_hz >= 60);
        assert!(caps.native_scale.is_finite() && caps.native_scale > 0.0);
        assert!(caps.supports_msaa4x);
        assert!(matches!(caps.color_space, ColorSpace::Srgb));

        let capabilities = platform.capabilities();
        assert!(capabilities.contains(Capabilities::HOVER_POINTER));
        assert!(capabilities.contains(Capabilities::BLUETOOTH));
        assert!(capabilities.contains(Capabilities::PUSH));
        assert!(
            !capabilities.contains(Capabilities::CAMERA_RECORDING)
                || capabilities.contains(Capabilities::CAMERA),
            "camera recording capability requires camera capability"
        );

        assert_eq!(platform.telephony().home_country_iso_code(), None);
        platform.haptics().play(HapticPattern::Selection);
        let cached = platform.bluetooth().cached_peripherals();
        for entry in cached {
            assert!(entry.last_seen_ms > 0);
            assert!(!entry.peripheral.id.to_le_bytes().iter().all(|byte| *byte == 0));
        }
    }

    fn verify_permission_statuses(platform: &dyn Platform) {
        let domains = [
            PermissionDomain::Notifications,
            PermissionDomain::Location,
            PermissionDomain::Camera,
            PermissionDomain::Contacts,
            PermissionDomain::Bluetooth,
            PermissionDomain::Motion,
            PermissionDomain::Microphone,
            PermissionDomain::MediaLibrary,
        ];
        for domain in domains {
            assert_valid_permission_status(platform.permissions().status(domain));
        }
        assert_eq!(
            platform.permissions().status(PermissionDomain::Motion),
            PermissionStatus::Denied,
            "macOS motion permission should stay a no-provider denied status"
        );
    }

    fn verify_network_status(platform: &dyn Platform) {
        let current = platform.network_status().current_status();
        assert_network_status_shape(current);

        let (events_tx, events_rx) = mpsc::channel();
        platform.network_status().subscribe(Box::new(move |status| {
            let _ = events_tx.send(status);
        }));
        assert_network_status_shape(recv_network_status(&events_rx));
    }

    fn assert_unsupported<T>(result: Result<T, PlatformError>, context: &str) {
        match result {
            Err(PlatformError::Unsupported(_)) => {}
            Err(error) => panic!("{context} should be explicitly unsupported, got {:?}", error),
            Ok(_) => panic!("{context} should be explicitly unsupported, got Ok"),
        }
    }

    fn assert_media_expected_error<T>(result: Result<T, PlatformError>, context: &str) {
        match result {
            Err(PlatformError::PermissionDenied("media_library")) | Err(PlatformError::NotFound(_)) => {}
            Err(error) => panic!("{context} returned unexpected error: {:?}", error),
            Ok(_) => panic!("{context} should not resolve data for a synthetic asset id"),
        }
    }

    fn assert_image_data(data: AssetData, context: &str) {
        match data {
            AssetData::Image { data, format } => {
                assert_eq!(format, ImageFormat::Jpeg, "{context} should return JPEG image data");
                assert!(!data.is_empty(), "{context} image bytes should not be empty");
            }
            AssetData::Video { file_path } => {
                panic!("{context} returned video data instead of image bytes: {file_path}");
            }
        }
    }

    fn assert_video_data(data: AssetData, context: &str) {
        match data {
            AssetData::Video { file_path } => {
                assert!(!file_path.is_empty(), "{context} video path should not be empty");
                let path = std::path::Path::new(&file_path);
                assert!(path.is_absolute(), "{context} video path should be absolute: {file_path}");
                assert!(path.exists(), "{context} video path should exist: {file_path}");
                if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                    if path.starts_with(std::env::temp_dir()) && name.starts_with("oxide-media-") {
                        let _ = std::fs::remove_file(path);
                    }
                }
            }
            AssetData::Image { .. } => panic!("{context} returned image data instead of a video path"),
        }
    }

    fn assert_not_found<T>(result: Result<T, PlatformError>, context: &str) {
        match result {
            Err(PlatformError::NotFound(_)) => {}
            Err(error) => panic!("{context} should report a missing native session, got {:?}", error),
            Ok(_) => panic!("{context} should report a missing native session, got Ok"),
        }
    }

    fn camera_smoke_config() -> CameraConfig {
        CameraConfig {
            fps: 30,
            resolution: (640, 480),
            capture: CaptureMode::Preview,
            preferred_color_space: Some(ColorSpace::Srgb),
            ..CameraConfig::default()
        }
    }

    fn verify_camera_unauthorized_start_error(platform: &dyn Platform) {
        if platform.permissions().status(PermissionDomain::Camera) == PermissionStatus::Authorized {
            return;
        }

        let result = platform.camera().start_stream(camera_smoke_config(), Box::new(|_| {}), None);
        match result {
            Err(PlatformError::PermissionDenied(_)) => {}
            Err(error) => panic!("unauthorized macOS camera start returned unexpected error: {:?}", error),
            Ok(stream) => {
                stream.stop();
                panic!("unauthorized macOS camera start should not create a stream");
            }
        }
    }

    fn verify_inactive_camera_session_errors(platform: &dyn Platform) {
        assert_not_found(
            platform
                .camera()
                .capture_photo(PhotoOptions::default(), Box::new(|_| {})),
            "macOS camera photo capture without a running session",
        );
        assert_not_found(
            platform.camera().start_recording(
                temporary_video_recording_options(),
                Box::new(|_| {}),
            ),
            "macOS camera recording without a running session",
        );
    }

    fn temporary_video_recording_options() -> RecordingOptions {
        RecordingOptions {
            destination: RecordingDestination::Temporary,
            container: RecordingContainer::Mov,
            include_audio: false,
            ..RecordingOptions::default()
        }
    }

    fn verify_live_location_if_requested(platform: &dyn Platform) {
        if std::env::var("OXIDE_MACOS_LIVE_LOCATION").ok().as_deref() != Some("1") {
            return;
        }
        assert_eq!(
            platform.permissions().status(PermissionDomain::Location),
            PermissionStatus::Authorized,
            "OXIDE_MACOS_LIVE_LOCATION=1 requires pre-authorized location permission"
        );
        assert!(
            platform.capabilities().contains(Capabilities::LOCATION),
            "live location validation requires enabled macOS location services"
        );

        let (events_tx, events_rx) = mpsc::channel();
        platform.location().subscribe(Box::new(move |event| {
            let _ = events_tx.send(event);
        }));
        platform
            .location()
            .start(LocationOptions {
                accuracy: LocationAccuracy::Precise,
                distance_filter_m: 0.0,
                allow_background_updates: false,
                precise: true,
            })
            .expect("start live macOS location updates");
        platform.location().request_once();
        let reading = recv_location_update(&events_rx);
        assert_location_reading(reading);

        let last = platform.location().last().expect("live macOS location should cache last reading");
        assert_location_reading(last);
        assert!(
            !platform.location().history().is_empty(),
            "live macOS location should cache history"
        );
        platform.location().stop();
    }

    fn verify_live_camera_if_requested(platform: &dyn Platform) {
        if std::env::var("OXIDE_MACOS_LIVE_CAMERA").ok().as_deref() != Some("1") {
            return;
        }
        assert_eq!(
            platform.permissions().status(PermissionDomain::Camera),
            PermissionStatus::Authorized,
            "OXIDE_MACOS_LIVE_CAMERA=1 requires pre-authorized camera permission"
        );
        assert!(
            platform.capabilities().contains(Capabilities::CAMERA),
            "live camera validation requires an available macOS camera"
        );

        let (frames_tx, frames_rx) = mpsc::channel();
        let stream = platform
            .camera()
            .start_stream(camera_smoke_config(), Box::new(move |frame| {
                let _ = frames_tx.send(frame);
            }), None)
            .expect("start live macOS camera stream");
        let frame = recv_camera_frame(&frames_rx);
        assert_camera_frame_shape(&frame);

        let (photos_tx, photos_rx) = mpsc::channel();
        platform
            .camera()
            .capture_photo(
                PhotoOptions { high_speed_from_preview: true, flash_mode: FlashMode::Off },
                Box::new(move |event| {
                    let _ = photos_tx.send(event);
                }),
            )
            .expect("capture live macOS photo from preview stream");
        match recv_photo_event(&photos_rx) {
            PhotoEvent::Completed(frame) => assert_camera_frame_shape(&frame),
            PhotoEvent::Failed(error) => panic!("live macOS photo capture failed: {:?}", error),
        }

        let (record_tx, record_rx) = mpsc::channel();
        let recording = platform
            .camera()
            .start_recording(
                temporary_video_recording_options(),
                Box::new(move |event| {
                    let _ = record_tx.send(event);
                }),
            )
            .expect("start live macOS camera recording");
        std::thread::sleep(Duration::from_millis(500));
        recording.stop();
        match recv_recording_event(&record_rx) {
            RecordingEvent::Completed(result) => {
                assert!(!result.path.is_empty(), "recording path should not be empty");
                assert!(!result.had_audio, "audio-disabled recording should report no audio");
                let path = std::path::Path::new(&result.path);
                assert!(path.exists(), "recording file should exist: {}", result.path);
                let _ = std::fs::remove_file(path);
            }
            RecordingEvent::Cancelled => panic!("live macOS recording unexpectedly cancelled"),
            RecordingEvent::Failed(error) => panic!("live macOS recording failed: {:?}", error),
        }

        stream.stop();
    }

    fn verify_location_motion_and_camera_no_prompt_paths(platform: &dyn Platform) {
        assert_eq!(platform.location().last(), None);
        platform
            .location()
            .set_accuracy(LocationAccuracy::Reduced)
            .expect("set reduced location accuracy without prompt");
        platform
            .location()
            .set_accuracy(LocationAccuracy::Balanced)
            .expect("set balanced location accuracy without prompt");
        verify_live_location_if_requested(platform);

        assert_eq!(platform.motion().pressure_history(), Vec::new());
        assert!(!platform.motion().is_running());
        assert_unsupported(platform.motion().start(), "macOS motion start");
        assert!(!platform.motion().is_running());
        platform.motion().stop();

        let camera = platform.camera();
        assert_unsupported(
            camera.start_native_preview(CameraConfig::default()),
            "macOS native camera preview",
        );
        assert_unsupported(camera.set_zoom_factor(2.0), "macOS camera zoom");
        camera.set_flash_mode(FlashMode::Off).expect("macOS flash off should be accepted");
        assert_unsupported(camera.set_flash_mode(FlashMode::On), "macOS camera flash on");
        camera.set_torch_mode(TorchMode::Off).expect("macOS torch off should be accepted");
        assert_unsupported(
            camera.set_torch_mode(TorchMode::On { level: 1.0 }),
            "macOS camera torch on",
        );
        verify_camera_unauthorized_start_error(platform);
        verify_inactive_camera_session_errors(platform);
        verify_live_camera_if_requested(platform);
    }

    fn verify_push_no_bundle_paths(platform: &dyn Platform) {
        platform.push().register();
        assert_eq!(platform.push().device_token(), None);
        platform.push().set_badge(1);
        platform.push().clear_badge();
        platform.push().clear_all_delivered();
    }

    fn first_media_asset(platform: &dyn Platform, asset_type: AssetType, limit: u32) -> Option<AssetId> {
        match block_on_ready(platform.media_library().query_assets(asset_type, limit, 0)) {
            Ok(assets) => {
                assert!(assets.len() <= limit as usize, "media query limit should be honored");
                for asset in assets.iter() {
                    assert_eq!(asset.asset_type, asset_type);
                    assert!(!asset.id.0.is_empty(), "media asset id should not be empty");
                    assert!(asset.width > 0 || asset.height > 0 || asset.duration_ms.is_some());
                }
                assets.into_iter().next().map(|asset| asset.id)
            }
            Err(PlatformError::PermissionDenied("media_library")) => None,
            Err(error) => panic!("media query returned unexpected error: {:?}", error),
        }
    }

    fn verify_media_library_no_prompt_paths(platform: &dyn Platform) {
        let first_image = first_media_asset(platform, AssetType::Image, 1);
        let first_video = first_media_asset(platform, AssetType::Video, 1);

        if let Some(image_id) = first_image.as_ref() {
            assert_image_data(
                block_on_ready(
                    platform
                        .media_library()
                        .request_image_data(image_id, ImageQuality::Thumbnail),
                )
                .expect("load authorized macOS media thumbnail"),
                "authorized macOS media thumbnail",
            );
        }

        let missing = AssetId(String::from("oxide-macos-missing-media-asset"));
        assert_media_expected_error(
            block_on_ready(platform.media_library().request_image_data(&missing, ImageQuality::Thumbnail)),
            "thumbnail request for missing media asset",
        );
        assert_media_expected_error(
            block_on_ready(platform.media_library().request_image_data(&missing, ImageQuality::Display)),
            "display image request for missing media asset",
        );
        match block_on_ready(platform.media_library().request_video_data(&missing)) {
            Err(PlatformError::PermissionDenied("media_library")) | Err(PlatformError::NotFound(_)) => {}
            Err(error) => panic!("video request for missing media asset returned unexpected error: {:?}", error),
            Ok(AssetData::Video { file_path }) => {
                panic!("video request for missing media asset returned path: {file_path}")
            }
            Ok(AssetData::Image { .. }) => panic!("video request returned image data"),
        }

        verify_live_media_if_requested(platform, first_image, first_video);
    }

    fn verify_live_media_if_requested(
        platform: &dyn Platform,
        first_image: Option<AssetId>,
        first_video: Option<AssetId>,
    ) {
        if std::env::var("OXIDE_MACOS_LIVE_MEDIA").ok().as_deref() != Some("1") {
            return;
        }
        assert_eq!(
            platform.permissions().status(PermissionDomain::MediaLibrary),
            PermissionStatus::Authorized,
            "OXIDE_MACOS_LIVE_MEDIA=1 requires pre-authorized Photos access"
        );

        let image_id = first_image.or_else(|| first_media_asset(platform, AssetType::Image, 16))
            .expect("OXIDE_MACOS_LIVE_MEDIA=1 requires at least one image asset");
        assert_image_data(
            block_on_ready(
                platform
                    .media_library()
                    .request_image_data(&image_id, ImageQuality::Display),
            )
            .expect("load authorized macOS display image"),
            "authorized macOS display image",
        );

        let video_id = first_video.or_else(|| first_media_asset(platform, AssetType::Video, 16))
            .expect("OXIDE_MACOS_LIVE_MEDIA=1 requires at least one video asset");
        assert_video_data(
            block_on_ready(platform.media_library().request_video_data(&video_id))
                .expect("load authorized macOS video asset"),
            "authorized macOS video asset",
        );
    }

    fn temp_html_url(name: &str, value: &str) -> (String, PathBuf) {
        let mut path = std::env::temp_dir();
        path.push(format!("oxide-macos-web-view-{}-{name}.html", std::process::id()));
        std::fs::write(
            &path,
            format!(
                r#"<!doctype html>
<html>
<body>
<main id="answer">{value}</main>
<script>
window.oxideValue = "oxide-webview:" + document.getElementById("answer").textContent;
</script>
</body>
</html>"#
            ),
        )
        .expect("write WebView test document");
        (format!("file://{}", path.display()), path)
    }

    fn assert_load_finished(events: &mpsc::Receiver<WebViewEvent>, context: &str) {
        match recv_web_view_event(events) {
            WebViewEvent::LoadFinished => {}
            WebViewEvent::LoadFailed(error) => panic!("{context} WebView load failed: {:?}", error),
        }
    }

    fn assert_web_view_script_error(
        result: Result<Option<String>, PlatformError>,
        context: &str,
    ) {
        match result {
            Err(PlatformError::Unknown(_)) => {}
            Err(error) => panic!("{context} should return a script failure, got {:?}", error),
            Ok(value) => panic!("{context} should return a script failure, got {:?}", value),
        }
    }

    fn verify_web_view_lifecycle(platform: &dyn Platform) {
        match platform.web_view_service().create_view("", Box::new(|_| {})) {
            Err(PlatformError::Invalid(_)) => {}
            Err(error) => panic!("empty WebView URL should be invalid, got {:?}", error),
            Ok(view) => {
                view.close();
                panic!("empty WebView URL should not create a view");
            }
        }

        let (events_a_tx, events_a_rx) = mpsc::channel();
        let (events_b_tx, events_b_rx) = mpsc::channel();
        let (url_a, temp_a) = temp_html_url("a", "oxide-webview-a");
        let (url_b, temp_b) = temp_html_url("b", "oxide-webview-b");

        let view_a = platform
            .web_view_service()
            .create_view(&url_a, Box::new(move |event| {
                let _ = events_a_tx.send(event);
            }))
            .expect("create first hidden macOS WebView through installed platform");
        let view_b = platform
            .web_view_service()
            .create_view(&url_b, Box::new(move |event| {
                let _ = events_b_tx.send(event);
            }))
            .expect("create second hidden macOS WebView through installed platform");

        assert_load_finished(&events_a_rx, "first hidden macOS");
        assert_load_finished(&events_b_rx, "second hidden macOS");

        let value_a = block_on_ready(view_a.execute_script("window.oxideValue"))
            .expect("execute JavaScript through first hidden macOS WebView");
        assert_eq!(value_a.as_deref(), Some("oxide-webview:oxide-webview-a"));

        let value_b = block_on_ready(view_b.execute_script("window.oxideValue"))
            .expect("execute JavaScript through second hidden macOS WebView");
        assert_eq!(value_b.as_deref(), Some("oxide-webview:oxide-webview-b"));

        let numeric = block_on_ready(view_a.execute_script("21 * 2"))
            .expect("execute numeric JavaScript through hidden macOS WebView");
        assert_eq!(numeric.as_deref(), Some("42"));

        let empty = block_on_ready(view_a.execute_script("''"))
            .expect("execute empty-string JavaScript through hidden macOS WebView");
        assert_eq!(empty.as_deref(), Some(""));

        let missing = block_on_ready(view_a.execute_script("undefined"))
            .expect("execute undefined JavaScript through hidden macOS WebView");
        assert_eq!(missing, None);

        let json = block_on_ready(view_a.execute_script("JSON.stringify({alpha:1,beta:'two'})"))
            .expect("execute JSON JavaScript through hidden macOS WebView");
        assert_eq!(json.as_deref(), Some(r#"{"alpha":1,"beta":"two"}"#));

        assert_web_view_script_error(
            block_on_ready(view_a.execute_script("throw new Error('oxide webview failure')")),
            "throwing JavaScript",
        );
        match block_on_ready(view_a.execute_script("   ")) {
            Err(PlatformError::Invalid(_)) => {}
            Err(error) => panic!("empty WebView script should be invalid, got {:?}", error),
            Ok(value) => panic!("empty WebView script should be invalid, got {:?}", value),
        }

        view_a.close();
        view_a.close();
        match block_on_ready(view_a.execute_script("window.oxideValue")) {
            Err(PlatformError::NotFound(_)) => {}
            Err(error) => panic!("closed WebView script should return not-found, got {:?}", error),
            Ok(value) => panic!("closed WebView script should return not-found, got {:?}", value),
        }

        let value_b_after_close = block_on_ready(view_b.execute_script("window.oxideValue"))
            .expect("second WebView should survive closing the first view");
        assert_eq!(value_b_after_close.as_deref(), Some("oxide-webview:oxide-webview-b"));

        view_b.close();
        std::fs::remove_file(temp_a).expect("remove first WebView test document");
        std::fs::remove_file(temp_b).expect("remove second WebView test document");
    }

    pub fn run() {
        const W: u32 = 320;
        const H: u32 = 240;
        const SCALE: f32 = 2.0;

        host_harness_reset();
        assert_eq!(macos_app_init(W, H, SCALE), 0, "macos_app_init failed");
        let platform = oxide_platform_api::current_platform_if_registered()
            .expect("macOS platform should be installed");
        verify_basic_platform_services(platform.as_ref());
        verify_permission_statuses(platform.as_ref());
        verify_network_status(platform.as_ref());
        verify_location_motion_and_camera_no_prompt_paths(platform.as_ref());
        verify_push_no_bundle_paths(platform.as_ref());
        verify_media_library_no_prompt_paths(platform.as_ref());
        verify_web_view_lifecycle(platform.as_ref());
        host_harness_reset();
    }
}

#[cfg(all(target_os = "macos", feature = "host-testing"))]
fn main() {
    harness::run();
}

#[cfg(not(all(target_os = "macos", feature = "host-testing")))]
fn main() {}
