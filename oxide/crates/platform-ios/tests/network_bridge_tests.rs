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
    let body =
        source_between(source, "_connection = connection;", "nw_connection_start(_connection);");

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

#[test]
fn network_bridge_enables_ticket_resumption_on_public_security_options() {
    let source = include_str!("../src/ios/network.m");
    let body = source_between(
        source,
        "static void configure_sec_options(sec_protocol_options_t sec_options",
        "  SecIdentityRef identity_ref = copy_identity(tls);",
    );

    assert!(body.contains("sec_protocol_options_set_tls_tickets_enabled(sec_options, true);"));
    assert!(body.contains("sec_protocol_options_set_tls_resumption_enabled(sec_options, true);"));
    assert!(body.contains("sec_protocol_options_set_min_tls_protocol_version("));
    assert!(body.contains("sec_options, tls_protocol_version_TLSv13);"));
    assert!(body.contains("sec_protocol_options_set_max_tls_protocol_version("));
    assert!(
        !source.contains("sec_protocol_options_set_tls_early_data_enabled"),
        "early-data enablement must not call private SDK exports"
    );
}

#[test]
fn network_bridge_configures_tcp_tls13_fast_open_and_early_writes() {
    let source = include_str!("../src/ios/network.m");
    let tcp_body = source_between(
        source,
        "_tlsParameters = nw_parameters_create_secure_tcp",
        "  if (_tlsParameters != NULL)",
    );
    let send_body = source_between(
        source,
        "- (BOOL)sendBytes:(const uint8_t *)data",
        "- (NSData *)popReceived:(uint64_t)timeoutMs",
    );

    assert!(
        source.contains("configure_sec_options(sec_options, tls, endpoint.UTF8String, alpn, NO);")
    );
    assert!(tcp_body
        .contains("configure_sec_options(sec_options, tls, endpoint.UTF8String, alpn, YES);"));
    assert!(tcp_body.contains("nw_tcp_options_set_enable_fast_open(tcp_options, true);"));
    assert!(source.contains("nw_parameters_set_fast_open_enabled(_tlsParameters, true);"));
    assert!(source.contains("self.ready || (self.currentFallback && _connection != NULL)"));
    assert!(send_body.contains("[self waitForWritableConnection:timeoutMs]"));
    assert!(!send_body.contains("[self waitForReady:timeoutMs]"));
}

#[test]
fn network_bridge_applies_rust_quic_transport_fields() {
    let source = include_str!("../src/ios/network.m");
    let body = source_between(
        source,
        "_quicParameters = nw_parameters_create_quic",
        "  if (_quicParameters == NULL)",
    );

    assert!(
        source.contains("uint16_t keepaliveIntervalSecs = _quicConfig.keepalive_interval_secs;")
    );
    assert!(body.contains("nw_quic_set_idle_timeout(quic_options, idleTimeoutMs);"));
    assert!(body.contains("nw_quic_set_max_udp_payload_size(quic_options, maxUdpPayloadSize);"));
}

#[test]
fn network_bridge_configures_client_keepalive_for_quic_and_tcp_tls() {
    let source = include_str!("../src/ios/network.m");
    let tcp_body = source_between(
        source,
        "_tlsParameters = nw_parameters_create_secure_tcp",
        "  if (_tlsParameters != NULL)",
    );
    let ready_body = source_between(
        source,
        "- (void)handleReady",
        "- (void)handleFailure:(nw_error_t)error fallback:(BOOL)attemptedFallback",
    );

    assert!(tcp_body.contains("nw_tcp_options_set_enable_keepalive(tcp_options, true);"));
    assert!(tcp_body
        .contains("nw_tcp_options_set_keepalive_idle_time(tcp_options, keepaliveIntervalSecs);"));
    assert!(tcp_body
        .contains("nw_tcp_options_set_keepalive_interval(tcp_options, keepaliveIntervalSecs);"));
    assert!(ready_body.contains("[self configureReadyKeepalive];"));
    assert!(
        ready_body.contains("self.currentFallback || self.quicConfig.keepalive_interval_secs == 0")
    );
    assert!(
        ready_body.contains("nw_connection_copy_protocol_metadata(_connection, quicDefinition);")
    );
    assert!(ready_body.contains("nw_quic_set_keepalive_interval(quicMetadata,"));
}

#[test]
fn network_bridge_leaves_path_migration_and_ticket_cache_unpinned() {
    let source = include_str!("../src/ios/network.m");

    assert!(!source.contains("nw_parameters_require_interface("));
    assert!(!source.contains("nw_parameters_set_required_interface_type("));
    assert!(!source.contains("nw_parameters_prohibit_interface("));
    assert!(!source.contains("nw_parameters_prohibit_interface_type("));
    assert!(!source.contains("nw_parameters_set_local_endpoint("));
    assert!(!source.contains("nw_privacy_context_create("));
    assert!(!source.contains("nw_parameters_set_privacy_context("));
}
