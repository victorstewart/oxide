fn source_between<'a>(source: &'a str, start_marker: &str, end_marker: &str) -> &'a str {
    let start = source.rfind(start_marker).expect(start_marker);
    let end = source[start..].find(end_marker).expect(end_marker) + start;
    &source[start..end]
}

#[test]
fn forced_tcp_tls_retries_stay_on_tls_parameters() {
    let source = include_str!("../src/ios/network.m");
    let body = source_between(
        source,
        "- (void)handleFailure:(nw_error_t)error fallback:(BOOL)attemptedFallback",
        "- (void)scheduleRetryWithParameters:(nw_parameters_t)parameters",
    );
    let forced_branch = body.find("BOOL forceTcpTls").expect("forced TCP/TLS retry branch");
    let generic_retry = body
        .find("[self scheduleRetryWithParameters:_quicParameters fallback:NO];")
        .expect("generic QUIC retry");

    assert!(
        forced_branch < generic_retry,
        "forced TCP/TLS retry must be checked before the generic QUIC retry"
    );
    assert!(body.contains("self.quicConfig.force_tcp_tls"));
    assert!(body.contains("env_truthy(\"NAMETAG_NETWORK_FORCE_TCP_TLS\")"));
    assert!(body.contains("forceTcpTls && _tlsParameters != NULL && canRetry && connectFailed"));
    assert!(body.contains("[self scheduleRetryWithParameters:_tlsParameters fallback:YES];"));
}

#[test]
fn network_bridge_ignores_stale_connection_state_events() {
    let source = include_str!("../src/ios/network.m");
    let body = source_between(
        source,
        "_connection = connection;",
        "nw_connection_start(_connection);",
    );

    assert!(body.contains("NSUInteger attemptNumber = self.attempt;"));
    assert!(body.contains("strongSelf->_connection != connection"));
    assert!(body.contains("return;"));
}

#[test]
fn network_bridge_skips_stale_delayed_retry_after_ready() {
    let source = include_str!("../src/ios/network.m");
    let body = source_between(
        source,
        "- (void)scheduleRetryWithParameters:(nw_parameters_t)parameters",
        "- (BOOL)waitForReady:(uint64_t)timeoutMs",
    );

    assert!(body.contains("NSUInteger expectedAttempt = self.attempt;"));
    assert!(body.contains("strongSelf.attempt != expectedAttempt || strongSelf.ready"));
    assert!(body.contains("Nametag network retry skipped"));
}

#[test]
fn network_bridge_ready_signal_spans_retries() {
    let source = include_str!("../src/ios/network.m");
    let body = source_between(
        source,
        "- (void)startAttemptWithParameters:(nw_parameters_t)parameters",
        "- (void)handleReady",
    );

    assert!(
        !body.contains("self.readySignal = dispatch_semaphore_create(0);"),
        "send waiters must survive QUIC-to-TCP fallback attempts"
    );
    assert!(source.contains("Nametag network wait ready timeout"));
}

#[test]
fn network_bridge_receives_stream_frames_not_messages() {
    let source = include_str!("../src/ios/network.m");

    assert!(
        source.contains("nw_connection_receive(\n       _connection, 1, 65536"),
        "stream transports must use byte receive"
    );
    assert!(
        !source.contains("nw_connection_receive_message("),
        "message receive reports No message available on STREAM"
    );
    assert!(source.contains("kNametagMaxFrameBytes"));
    assert!(source.contains("frameLength < 16"));
    assert!(source.contains("[strongSelf drainIncomingBytes];"));
    assert!(source.contains("Nametag network receive frame"));
}
