use oxide_networking::{
    HandshakeResponse, OutboundPacket, PacketKind, QuicSessionConfig, QuicSessionManager,
    SessionPhase, TimeSyncSample,
};

fn manager_with_clock(clock: std::sync::Arc<dyn Fn() -> u64 + Send + Sync>) -> QuicSessionManager {
    let config = QuicSessionConfig {
        handshake_timeout_ms: 50,
        max_retries: 3,
        timesync_interval_ms: 100,
        timesync_window: 4,
        initial_retry_ms: 30,
        max_retry_ms: 120,
        ..QuicSessionConfig::default()
    };
    QuicSessionManager::new_with_clock(clock, config)
}

fn test_clock_pair(
) -> (std::sync::Arc<std::sync::atomic::AtomicU64>, std::sync::Arc<dyn Fn() -> u64 + Send + Sync>) {
    let now = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let now_clone = std::sync::Arc::clone(&now);
    let clock = std::sync::Arc::new(move || now_clone.load(std::sync::atomic::Ordering::SeqCst));
    (now, clock)
}

fn extract_packet(packets: &[OutboundPacket], kind: PacketKind) -> Option<&OutboundPacket> {
    packets.iter().find(|pkt| pkt.kind == kind)
}

#[test]
fn handshake_retries_and_succeeds() {
    let (now, clock) = test_clock_pair();
    let mut manager = manager_with_clock(clock);

    manager.tick(0);
    let mut outbound = manager.drain_outbound();
    assert!(extract_packet(&outbound, PacketKind::HandshakeInit).is_some());

    // advance but within timeout - no new packet
    manager.tick(20);
    outbound = manager.drain_outbound();
    assert!(outbound.is_empty());

    // trigger retry
    now.store(55, std::sync::atomic::Ordering::SeqCst);
    manager.tick(55);
    outbound = manager.drain_outbound();
    assert!(extract_packet(&outbound, PacketKind::HandshakeRetry).is_some());

    // respond with success
    manager.on_handshake_response(HandshakeResponse { accepted: true, session_id: Some(42) }, 60);
    assert_eq!(manager.metrics().phase, SessionPhase::Established { session_id: 42 });

    // time-sync request scheduled
    manager.tick(160);
    outbound = manager.drain_outbound();
    assert!(extract_packet(&outbound, PacketKind::TimeSyncProbe).is_some());
}

#[test]
fn handshake_eventually_fails() {
    let (_, clock) = test_clock_pair();
    let mut manager = manager_with_clock(clock);

    manager.tick(0);
    manager.drain_outbound();

    for step in [60u64, 120, 240] {
        manager.tick(step);
        manager.drain_outbound();
    }

    assert!(matches!(manager.metrics().phase, SessionPhase::Failed { .. }));
}

#[test]
fn time_sync_records_offsets() {
    let (_, clock) = test_clock_pair();
    let mut manager = manager_with_clock(clock);

    manager.tick(0);
    manager.drain_outbound();
    manager.on_handshake_response(HandshakeResponse { accepted: true, session_id: Some(7) }, 10);

    manager.tick(120);
    let packets = manager.drain_outbound();
    let probe = extract_packet(&packets, PacketKind::TimeSyncProbe).expect("probe");

    let sample = TimeSyncSample {
        client_send_ms: probe.timestamp_ms,
        server_recv_ms: probe.timestamp_ms + 5,
        server_send_ms: probe.timestamp_ms + 8,
        client_recv_ms: 200,
    };
    manager.on_time_sync_response(sample);

    let metrics = manager.metrics();
    assert!(metrics.time_sync.offset_ms.is_some());
    assert!(metrics.time_sync.rtt_ms.is_some());
    assert_eq!(metrics.time_sync.sample_count, 1);
}
