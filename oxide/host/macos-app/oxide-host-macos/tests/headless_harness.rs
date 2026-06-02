#[cfg(all(target_os = "macos", feature = "host-testing"))]
mod harness {
    use oxide_host_macos::{
        host_harness_reset, host_harness_snapshot, macos_app_frame, macos_app_frame_with_drawable,
        macos_app_init, macos_emit_key, macos_emit_pinch, macos_emit_pointer, macos_emit_rotate,
        macos_emit_text_commit, macos_emit_text_composition, macos_emit_text_selection,
        macos_emit_touch, macos_set_key_callback, macos_set_pinch_callback,
        macos_set_pointer_callback, macos_set_rotate_callback, macos_set_text_commit_callback,
        macos_set_text_composition_callback, macos_set_text_selection_callback,
        macos_set_touch_callback,
    };
    use oxide_platform_api::{
        ConnectionEvent, ConnectionOptions, PlatformError, ProtocolOptions, QuicOptions,
        TcpOptions, TlsOptions, UdpEvent, UdpPacket,
    };
    use oxide_telemetry::TelemetryLifecycleState;
    use oxide_test_scenes::SceneKind;
    use std::future::Future;
    use std::io::{Read, Write};
    use std::net::{TcpListener, UdpSocket};
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    use std::thread;
    use std::time::{Duration, Instant};

    static TOUCH_EVENTS: AtomicUsize = AtomicUsize::new(0);
    static POINTER_EVENTS: AtomicUsize = AtomicUsize::new(0);
    static KEY_EVENTS: AtomicUsize = AtomicUsize::new(0);
    static TEXT_COMMIT_EVENTS: AtomicUsize = AtomicUsize::new(0);
    static TEXT_COMPOSE_EVENTS: AtomicUsize = AtomicUsize::new(0);
    static TEXT_SELECT_EVENTS: AtomicUsize = AtomicUsize::new(0);
    static PINCH_EVENTS: AtomicUsize = AtomicUsize::new(0);
    static ROTATE_EVENTS: AtomicUsize = AtomicUsize::new(0);

    fn test_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    extern "C" fn touch_cb(_id: u64, _phase: u32, _x: f32, _y: f32, _timestamp_ns: u64) {
        TOUCH_EVENTS.fetch_add(1, Ordering::SeqCst);
    }

    extern "C" fn pointer_cb(
        _x: f32,
        _y: f32,
        _dx: f32,
        _dy: f32,
        _buttons: u32,
        _modifiers: u32,
        _timestamp_ns: u64,
    ) {
        POINTER_EVENTS.fetch_add(1, Ordering::SeqCst);
    }

    extern "C" fn key_cb(
        _code: u32,
        _chars_ptr: *const u8,
        _chars_len: usize,
        _repeat: u8,
        _modifiers: u32,
        _timestamp_ns: u64,
    ) {
        KEY_EVENTS.fetch_add(1, Ordering::SeqCst);
    }

    extern "C" fn text_commit_cb(_text_ptr: *const u8, _text_len: usize) {
        TEXT_COMMIT_EVENTS.fetch_add(1, Ordering::SeqCst);
    }

    extern "C" fn text_compose_cb(_start: u32, _end: u32, _text_ptr: *const u8, _text_len: usize) {
        TEXT_COMPOSE_EVENTS.fetch_add(1, Ordering::SeqCst);
    }

    extern "C" fn text_select_cb(_start: u32, _end: u32) {
        TEXT_SELECT_EVENTS.fetch_add(1, Ordering::SeqCst);
    }

    extern "C" fn pinch_cb(_cx: f32, _cy: f32, _delta: f32, _timestamp_ns: u64) {
        PINCH_EVENTS.fetch_add(1, Ordering::SeqCst);
    }

    extern "C" fn rotate_cb(_cx: f32, _cy: f32, _radians: f32, _timestamp_ns: u64) {
        ROTATE_EVENTS.fetch_add(1, Ordering::SeqCst);
    }

    fn reset_event_counts() {
        TOUCH_EVENTS.store(0, Ordering::SeqCst);
        POINTER_EVENTS.store(0, Ordering::SeqCst);
        KEY_EVENTS.store(0, Ordering::SeqCst);
        TEXT_COMMIT_EVENTS.store(0, Ordering::SeqCst);
        TEXT_COMPOSE_EVENTS.store(0, Ordering::SeqCst);
        TEXT_SELECT_EVENTS.store(0, Ordering::SeqCst);
        PINCH_EVENTS.store(0, Ordering::SeqCst);
        ROTATE_EVENTS.store(0, Ordering::SeqCst);
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
            Poll::Pending => panic!("macOS host test future unexpectedly pending"),
        }
    }

    fn unique_secure_storage_key() -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("oxide-host-macos-live-secure-storage-{}-{nanos}", std::process::id())
    }

    fn spawn_loopback_http_response(
        body: &'static [u8],
        content_type: &'static str,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback HTTP listener");
        let addr = listener.local_addr().expect("read loopback listener address");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept loopback HTTP client");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).expect("read loopback HTTP request");
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).expect("write loopback HTTP header");
            stream.write_all(body).expect("write loopback HTTP body");
            stream.flush().expect("flush loopback HTTP response");
        });
        (format!("http://{addr}/oxide-host-macos-http"), handle)
    }

    fn spawn_loopback_tcp_echo() -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback TCP listener");
        let port = listener.local_addr().expect("read loopback TCP listener address").port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept loopback TCP client");
            let mut request = [0_u8; 4];
            stream.read_exact(&mut request).expect("read loopback TCP request");
            assert_eq!(&request, b"ping");
            stream.write_all(b"pong").expect("write loopback TCP response");
            stream.flush().expect("flush loopback TCP response");
        });
        (port, handle)
    }

    fn spawn_loopback_udp_echo() -> (u16, thread::JoinHandle<()>) {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("bind loopback UDP socket");
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set loopback UDP read timeout");
        let port = socket.local_addr().expect("read loopback UDP socket address").port();
        let handle = thread::spawn(move || {
            let mut request = [0_u8; 64];
            let (len, peer) = socket.recv_from(&mut request).expect("read loopback UDP packet");
            assert_eq!(&request[..len], b"ping");
            socket.send_to(b"pong", peer).expect("write loopback UDP response");
        });
        (port, handle)
    }

    fn recv_tcp_read(events: &mpsc::Receiver<ConnectionEvent>, expected: &[u8]) -> ConnectionEvent {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "timed out waiting for TCP read");
            let event = events.recv_timeout(remaining).expect("receive TCP connection event");
            match &event {
                ConnectionEvent::Read(bytes) if bytes == expected => return event,
                ConnectionEvent::Disconnected { error: Some(error) } => {
                    panic!("TCP disconnected before read: {:?}", error);
                }
                _ => {}
            }
        }
    }

    fn recv_udp_read(events: &mpsc::Receiver<UdpEvent>, expected: &[u8]) -> UdpPacket {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "timed out waiting for UDP read");
            match events.recv_timeout(remaining).expect("receive UDP socket event") {
                UdpEvent::Read(packet) if packet.data == expected => return packet,
                UdpEvent::WriteError(error) => panic!("UDP write failed before read: {:?}", error),
                _ => {}
            }
        }
    }

    #[test]
    fn headless_host_smoke() {
        let _guard = test_lock();
        const W: u32 = 1280;
        const H: u32 = 720;
        const SCALE: f32 = 2.0;

        host_harness_reset();
        assert!(
            oxide_platform_api::current_platform_if_registered().is_none(),
            "reset should clear the process-global platform"
        );

        let init = macos_app_init(W, H, SCALE);
        assert_eq!(init, 0, "macos_app_init failed");
        assert!(
            oxide_platform_api::current_platform_if_registered().is_some(),
            "macos_app_init should install the macOS platform"
        );

        let baseline = host_harness_snapshot();
        assert!(baseline.inited, "app should be initialised");
        assert_eq!(baseline.router_scene, Some(SceneKind::Controls));
        assert_eq!(baseline.draws.unwrap_or_default(), 0, "no draws before frames");
        assert_eq!(
            baseline.telemetry_lifecycle,
            Some(TelemetryLifecycleState::Foreground),
            "telemetry lifecycle should be foreground after init"
        );

        for _ in 0..5 {
            let frame = macos_app_frame(W, H, SCALE);
            assert_eq!(frame, 0, "macos_app_frame should succeed");
        }
        assert_eq!(
            macos_app_frame_with_drawable(W, H, SCALE, core::ptr::null_mut()),
            0,
            "macos_app_frame_with_drawable should preserve the headless path",
        );

        let after = host_harness_snapshot();
        assert!(after.inited, "app should remain initialised");
        let draws = after.draws.unwrap_or_default();
        let instanced = after.instanced.unwrap_or_default();
        assert!(
            draws > 0 || instanced > 0,
            "renderer should record draw commands; draws={} instanced={}",
            draws,
            instanced
        );
        assert!(after.last_ms >= baseline.last_ms, "frame clock should advance");
        assert_eq!(
            after.telemetry_lifecycle,
            Some(TelemetryLifecycleState::Foreground),
            "telemetry lifecycle should remain foreground"
        );

        host_harness_reset();
        assert!(
            oxide_platform_api::current_platform_if_registered().is_none(),
            "reset should clear the process-global platform after the smoke test"
        );
    }

    #[test]
    fn host_callbacks_forward_registered_events() {
        let _guard = test_lock();
        reset_event_counts();

        macos_set_touch_callback(Some(touch_cb));
        macos_set_pointer_callback(Some(pointer_cb));
        macos_set_key_callback(Some(key_cb));
        macos_set_text_commit_callback(Some(text_commit_cb));
        macos_set_text_composition_callback(Some(text_compose_cb));
        macos_set_text_selection_callback(Some(text_select_cb));
        macos_set_pinch_callback(Some(pinch_cb));
        macos_set_rotate_callback(Some(rotate_cb));

        macos_emit_touch(7, 0, 12.0, 13.0, 100);
        macos_emit_pointer(12.0, 13.0, 1.0, -1.0, 1, 0, 101);
        macos_emit_key(35, core::ptr::null(), 0, 0, 0, 102);
        macos_emit_text_commit(core::ptr::null(), 0);
        macos_emit_text_composition(1, 2, core::ptr::null(), 0);
        macos_emit_text_selection(1, 2);
        macos_emit_pinch(12.0, 13.0, 0.25, 103);
        macos_emit_rotate(12.0, 13.0, 0.5, 104);

        assert_eq!(TOUCH_EVENTS.load(Ordering::SeqCst), 1);
        assert_eq!(POINTER_EVENTS.load(Ordering::SeqCst), 1);
        assert_eq!(KEY_EVENTS.load(Ordering::SeqCst), 1);
        assert_eq!(TEXT_COMMIT_EVENTS.load(Ordering::SeqCst), 1);
        assert_eq!(TEXT_COMPOSE_EVENTS.load(Ordering::SeqCst), 1);
        assert_eq!(TEXT_SELECT_EVENTS.load(Ordering::SeqCst), 1);
        assert_eq!(PINCH_EVENTS.load(Ordering::SeqCst), 1);
        assert_eq!(ROTATE_EVENTS.load(Ordering::SeqCst), 1);

        macos_set_touch_callback(None);
        macos_set_pointer_callback(None);
        macos_set_key_callback(None);
        macos_set_text_commit_callback(None);
        macos_set_text_composition_callback(None);
        macos_set_text_selection_callback(None);
        macos_set_pinch_callback(None);
        macos_set_rotate_callback(None);
    }

    #[test]
    fn host_secure_storage_round_trips_live_keychain() {
        let _guard = test_lock();
        const W: u32 = 320;
        const H: u32 = 240;
        const SCALE: f32 = 2.0;

        host_harness_reset();
        assert_eq!(macos_app_init(W, H, SCALE), 0, "macos_app_init failed");
        let platform = oxide_platform_api::current_platform_if_registered()
            .expect("macOS platform should be installed");
        let storage = platform.secure_storage();
        let key = unique_secure_storage_key();
        let value = b"oxide-live-keychain-value";

        let _ = block_on_ready(storage.delete(&key));
        block_on_ready(storage.save(&key, value)).expect("save live Keychain value");
        assert_eq!(
            block_on_ready(storage.load(&key)).expect("load live Keychain value"),
            Some(value.to_vec())
        );
        block_on_ready(storage.delete(&key)).expect("delete live Keychain value");
        assert_eq!(
            block_on_ready(storage.load(&key)).expect("load deleted live Keychain value"),
            None
        );

        host_harness_reset();
    }

    #[test]
    fn host_http_get_fetches_loopback_response() {
        let _guard = test_lock();
        const W: u32 = 320;
        const H: u32 = 240;
        const SCALE: f32 = 2.0;
        const BODY: &[u8] = b"oxide macos http loopback";
        const CONTENT_TYPE: &str = "text/plain; charset=utf-8";

        host_harness_reset();
        assert_eq!(macos_app_init(W, H, SCALE), 0, "macos_app_init failed");
        let platform = oxide_platform_api::current_platform_if_registered()
            .expect("macOS platform should be installed");
        let (url, server) = spawn_loopback_http_response(BODY, CONTENT_TYPE);
        let request = oxide_platform_api::HttpRequest::get(url.clone())
            .with_timeout_ms(2_000)
            .with_max_response_bytes(1024);

        let response = platform.http().fetch(&request).expect("fetch loopback HTTP response");

        server.join().expect("loopback HTTP server should exit");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, BODY);
        assert_eq!(response.content_type.as_deref(), Some(CONTENT_TYPE));
        assert!(
            response.final_url.starts_with(&url),
            "final URL should preserve loopback URL: {}",
            response.final_url
        );

        host_harness_reset();
    }

    #[test]
    fn host_networking_tcp_connects_and_reads_loopback_response() {
        let _guard = test_lock();
        const W: u32 = 320;
        const H: u32 = 240;
        const SCALE: f32 = 2.0;

        host_harness_reset();
        assert_eq!(macos_app_init(W, H, SCALE), 0, "macos_app_init failed");
        let platform = oxide_platform_api::current_platform_if_registered()
            .expect("macOS platform should be installed");
        let (port, server) = spawn_loopback_tcp_echo();
        let (events_tx, events_rx) = mpsc::channel();
        let connection = platform
            .networking()
            .connect_tcp(
                ConnectionOptions {
                    host: String::from("127.0.0.1"),
                    port,
                    protocol: ProtocolOptions::Tcp(TcpOptions::default()),
                    tls_options: None,
                },
                Box::new(move |event| {
                    let _ = events_tx.send(event);
                }),
            )
            .expect("connect TCP through installed macOS platform");

        block_on_ready(connection.write(b"ping"))
            .expect("write TCP request through installed macOS platform");
        let event = recv_tcp_read(&events_rx, b"pong");
        assert!(matches!(event, ConnectionEvent::Read(_)));

        connection.close();
        server.join().expect("loopback TCP server should exit");
        host_harness_reset();
    }

    #[test]
    fn host_networking_tcp_keepalive_connects_and_reads_loopback_response() {
        let _guard = test_lock();
        const W: u32 = 320;
        const H: u32 = 240;
        const SCALE: f32 = 2.0;

        host_harness_reset();
        assert_eq!(macos_app_init(W, H, SCALE), 0, "macos_app_init failed");
        let platform = oxide_platform_api::current_platform_if_registered()
            .expect("macOS platform should be installed");
        let (port, server) = spawn_loopback_tcp_echo();
        let (events_tx, events_rx) = mpsc::channel();
        let mut tcp = TcpOptions::default();
        tcp.keepalive = true;
        tcp.keepalive_idle_time_secs = 30;
        let connection = platform
            .networking()
            .connect_tcp(
                ConnectionOptions {
                    host: String::from("127.0.0.1"),
                    port,
                    protocol: ProtocolOptions::Tcp(tcp),
                    tls_options: None,
                },
                Box::new(move |event| {
                    let _ = events_tx.send(event);
                }),
            )
            .expect("connect keepalive TCP through installed macOS platform");

        block_on_ready(connection.write(b"ping"))
            .expect("write keepalive TCP request through installed macOS platform");
        let event = recv_tcp_read(&events_rx, b"pong");
        assert!(matches!(event, ConnectionEvent::Read(_)));

        connection.close();
        server.join().expect("loopback keepalive TCP server should exit");
        host_harness_reset();
    }

    #[test]
    fn host_networking_udp_sends_and_reads_loopback_packet() {
        let _guard = test_lock();
        const W: u32 = 320;
        const H: u32 = 240;
        const SCALE: f32 = 2.0;

        host_harness_reset();
        assert_eq!(macos_app_init(W, H, SCALE), 0, "macos_app_init failed");
        let platform = oxide_platform_api::current_platform_if_registered()
            .expect("macOS platform should be installed");
        let (port, server) = spawn_loopback_udp_echo();
        let (events_tx, events_rx) = mpsc::channel();
        let socket = platform
            .networking()
            .bind_udp(
                0,
                Box::new(move |event| {
                    let _ = events_tx.send(event);
                }),
            )
            .expect("bind UDP through installed macOS platform");

        socket
            .send(&UdpPacket { host: String::from("127.0.0.1"), port, data: b"ping".to_vec() })
            .expect("send UDP packet through installed macOS platform");
        let packet = recv_udp_read(&events_rx, b"pong");
        assert_eq!(packet.host, "127.0.0.1");

        socket.close();
        server.join().expect("loopback UDP server should exit");
        host_harness_reset();
    }

    #[test]
    fn host_networking_rejects_unsupported_transport_options() {
        let _guard = test_lock();
        const W: u32 = 320;
        const H: u32 = 240;
        const SCALE: f32 = 2.0;

        host_harness_reset();
        assert_eq!(macos_app_init(W, H, SCALE), 0, "macos_app_init failed");
        let platform = oxide_platform_api::current_platform_if_registered()
            .expect("macOS platform should be installed");
        let tcp_options = |tcp: TcpOptions, tls_options: Option<TlsOptions>| ConnectionOptions {
            host: String::from("127.0.0.1"),
            port: 9,
            protocol: ProtocolOptions::Tcp(tcp),
            tls_options,
        };

        match platform.networking().connect_tcp(
            tcp_options(
                TcpOptions::default(),
                Some(TlsOptions { client_identity: None, pinned_public_keys: Vec::new() }),
            ),
            Box::new(|_| {}),
        ) {
            Err(PlatformError::Unsupported(_)) => {}
            Err(error) => panic!("raw TCP TLS should be explicitly unsupported: {error:?}"),
            Ok(connection) => {
                connection.close();
                panic!("raw TCP TLS should not establish a macOS host connection");
            }
        }

        let mut fast_open = TcpOptions::default();
        fast_open.fast_open = true;
        match platform.networking().connect_tcp(tcp_options(fast_open, None), Box::new(|_| {})) {
            Err(PlatformError::Unsupported(_)) => {}
            Err(error) => panic!("TCP Fast Open should be explicitly unsupported: {error:?}"),
            Ok(connection) => {
                connection.close();
                panic!("TCP Fast Open should not establish a macOS host connection");
            }
        }

        match platform.networking().connect_quic(
            ConnectionOptions {
                host: String::from("127.0.0.1"),
                port: 443,
                protocol: ProtocolOptions::Quic(QuicOptions { alpn: String::from("h3") }),
                tls_options: None,
            },
            Box::new(|_| {}),
        ) {
            Err(PlatformError::Unsupported(_)) => {}
            Err(error) => panic!("raw QUIC should be explicitly unsupported: {error:?}"),
            Ok(group) => {
                group.close();
                panic!("raw QUIC should not establish a macOS host connection group");
            }
        }

        host_harness_reset();
    }
}

#[cfg(not(all(target_os = "macos", feature = "host-testing")))]
#[test]
fn headless_host_smoke() {
    // Harness requires macOS + host-testing feature; nothing to assert here.
}
