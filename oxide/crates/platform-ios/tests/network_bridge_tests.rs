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
        "- (void)handleFailure:(nw_error_t)error",
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
    assert!(!source.contains("OXIDE_NETWORK_FORCE_TCP_TLS"));
    assert!(body.contains("forceTcpTls && _tlsParameters != NULL && canRetry && connectFailed"));
    assert!(body.contains("[self scheduleRetryWithParameters:_tlsParameters fallback:YES];"));
}

#[test]
fn network_bridge_public_abi_is_oxide_owned_and_complete()
{
   let header = include_str!("../src/ios/network.h");
   let source = include_str!("../src/ios/network.m");

   for declaration in [
      "struct OxideQuicConfig",
      "struct OxideQuicRetryPolicy",
      "struct OxideTlsTrustAnchor",
      "struct OxideQuicTlsConfig",
      "struct OxideQuicMetrics",
      "struct OxideReachabilityStatus",
      "oxide_ios_quic_connect(",
      "oxide_ios_quic_metrics(",
      "oxide_ios_quic_wait_ready(",
      "oxide_ios_quic_send(",
      "oxide_ios_quic_recv(",
      "oxide_ios_quic_poll_recv(",
      "oxide_ios_quic_close(",
      "oxide_ios_reachability_start(",
      "oxide_ios_reachability_poll(",
      "oxide_ios_reachability_close(",
      "oxide_host_net_set_reachability_callback(",
      "oxide_host_net_start_reachability(",
      "oxide_host_net_stop_reachability(",
   ]
   {
      assert!(header.contains(declaration), "missing public ABI declaration: {declaration}");
   }
   assert!(!header.contains("Nametag"));
   assert!(!header.contains("nametag"));
   assert!(!source.contains("Nametag"));
   assert!(!source.contains("nametag"));
}

#[test]
fn network_bridge_requires_caller_owned_connection_policy()
{
   let source = include_str!("../src/ios/network.m");
   let body = source_between(
      source,
      "OxideQuicHandle oxide_ios_quic_connect(",
      "bool oxide_ios_quic_metrics(",
   );

   assert!(body.contains("retry->max_attempts == 0"));
   assert!(!body.contains("(struct OxideQuicConfig){"));
   assert!(!body.contains("(struct OxideQuicRetryPolicy){"));
   assert!(!source.contains("kOxideDefaultPort"));
   assert!(!source.contains("MAX(self.retryPolicy.max_attempts, 1)"));
}

#[test]
fn network_bridge_strictly_parses_explicit_ports()
{
   let source = include_str!("../src/ios/network.m");
   let body = source_between(
      source,
      "static BOOL parse_endpoint(const char *endpoint, NSString **host,",
      "OxideQuicHandle oxide_ios_quic_connect(",
   );

   assert!(body.contains("if (portPart.length == 0)"), "missing ports must fail");
   assert!(body.contains("if (parsed == 0)"), "port zero must fail");
   assert!(body.contains("if (parsed > UINT16_MAX)"), "ports above 65535 must fail");
   assert!(body.contains("if (digit < '0' || digit > '9')"), "suffix junk must fail");
   assert!(body.contains("[mutable hasPrefix:@\"[\"]"), "bracketed IPv6 must be parsed");
   assert!(body.contains("[hostPart containsString:@\":\"]"), "unbracketed IPv6 must fail");
   assert!(!body.contains("integerValue"));
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
fn network_bridge_real_state_handler_treats_invalid_as_terminal() {
    let source = include_str!("../src/ios/network.m");
    let body = source_between(
        source,
        "nw_connection_set_state_changed_handler(",
        "nw_connection_start(_connection);",
    );
    let invalid = body
        .find("case nw_connection_state_invalid:")
        .expect("invalid connection state arm");
    let failed = body
        .find("case nw_connection_state_failed:")
        .expect("failed connection state arm");
    let terminal = body[invalid..]
        .find("terminal:YES")
        .expect("terminal invalid-state handling")
        + invalid;

    assert!(body.contains("strongSelf.state = state;"));
    assert!(invalid < failed && failed < terminal);
}

#[test]
fn network_bridge_ignores_stale_connection_receive_events() {
    let source = include_str!("../src/ios/network.m");
    let body = source_between(
        source,
        "- (void)startReceiveLoop",
        "- (void)drainIncomingBytes",
    );

    assert!(body.contains("nw_connection_t connection = _connection;"));
    assert!(body.contains("nw_connection_receive(\n       connection, 1, 65536"));
    assert!(body.contains("strongSelf->_connection != connection"));
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
    assert!(body.contains("Oxide network retry skipped"));
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
    assert!(
        !body.contains("self.receiveSignal = dispatch_semaphore_create(0);"),
        "receive pollers must never race a semaphore replacement"
    );
    assert!(source.contains("Oxide network wait ready timeout"));
}

#[test]
fn network_bridge_receives_stream_frames_not_messages() {
    let source = include_str!("../src/ios/network.m");

    assert!(
        source.contains("nw_connection_receive(\n       connection, 1, 65536"),
        "stream transports must use byte receive"
    );
    assert!(
        !source.contains("nw_connection_receive_message("),
        "message receive reports No message available on STREAM"
    );
    assert!(source.contains("kOxideMaxFrameBytes"));
    assert!(source.contains("frameLength < 16"));
    assert!(source.contains("[strongSelf drainIncomingBytes];"));
    assert!(source.contains("Oxide network receive frame"));
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
    assert!(source.contains("self.state == nw_connection_state_preparing"));
    assert!(send_body.contains("[self waitForWritableConnection:overallDeadlineNs]"));
    assert!(send_body.contains("![self isWritableOnQueue]"));
    assert!(!send_body.contains("[self waitForReady:timeoutMs]"));
}

#[test]
fn network_bridge_consumes_one_receive_permit_before_each_frame_pop() {
    let source = include_str!("../src/ios/network.m");
    let body = source_between(
        source,
        "- (NSData *)popReceived:(uint64_t)timeoutMs",
        "- (void)startReceiveLoop",
    );
    let wait = body
        .find("dispatch_semaphore_wait(_receiveSignal, deadline)")
        .expect("receive permit wait");
    let pop = body
        .find("after = strongSelf.receiveBuffer.firstObject;")
        .expect("receive queue pop");

    assert!(wait < pop, "a receive permit must be consumed before queue removal");
    assert_eq!(body.matches("receiveBuffer.firstObject").count(), 1);
    assert!(body.contains("strongSelf.queuedReceiveBytes -= after.length;"));
}

#[test]
fn network_bridge_exposes_nonblocking_tri_state_receive_contract() {
    let header = include_str!("../src/ios/network.h");
    let source = include_str!("../src/ios/network.m");
    let build = include_str!("../build.rs");
    let body = source_between(
        source,
        "int32_t oxide_ios_quic_poll_recv(OxideQuicHandle handle,",
        "OxideReachabilityHandle oxide_ios_reachability_start(void)",
    );

    assert!(header.contains("OXIDE_IOS_QUIC_POLL_TERMINAL = -1"));
    assert!(header.contains("OXIDE_IOS_QUIC_POLL_IDLE = 0"));
    assert!(header.contains("OXIDE_IOS_QUIC_POLL_FRAME = 1"));
    assert!(header.contains("int32_t oxide_ios_quic_poll_recv("));
    assert!(build.contains("cargo:rerun-if-changed=src/ios/network.h"));
    assert!(body.contains("*out_len = 0;"));
    assert!(body.contains("NSData *payload = [connection popReceived:0];"));
    assert!(body.contains("[connection copyClosedState]"));
    assert!(body.contains("[connection close];"));
    assert!(body.contains("*out_len = payload.length;"));
    assert!(body.contains("return OXIDE_IOS_QUIC_POLL_FRAME;"));
}

#[test]
fn network_bridge_marks_only_terminal_connection_events_closed() {
    let source = include_str!("../src/ios/network.m");
    let state_body = source_between(
        source,
        "switch (state)",
        "nw_connection_start(_connection);",
    );
    let failure_body = source_between(
        source,
        "- (void)handleFailure:(nw_error_t)error",
        "- (void)scheduleRetryWithParameters:(nw_parameters_t)parameters",
    );
    let receive_body = source_between(
        source,
        "- (void)startReceiveLoop",
        "- (void)drainIncomingBytes",
    );
    let close_body = source_between(
        source,
        "- (void)close\n{",
        "- (BOOL)copyClosedState",
    );
    let final_retry = failure_body
        .find("[self scheduleRetryWithParameters:_quicParameters fallback:NO];")
        .expect("final retry branch");
    let terminal_close = failure_body.find("[self close];").expect("terminal close");

    assert!(state_body.contains("terminal:YES"));
    assert!(state_body.contains("terminal:NO"));
    assert!(final_retry < terminal_close, "retry paths must precede terminal closure");
    assert!(failure_body.contains("if (terminalEvent)"));
    assert!(failure_body.contains("self.ready = NO;"));
    assert_eq!(receive_body.matches("[strongSelf close];").count(), 2);
    assert!(close_body.contains("self.ready = NO;"));
    assert!(close_body.contains("self.closed = YES;"));
}

#[test]
fn network_bridge_bounds_receive_queue_by_frames_and_bytes() {
    let source = include_str!("../src/ios/network.m");
    let body = source_between(
        source,
        "- (void)drainIncomingBytes",
        "- (BOOL)copyMetrics:(struct OxideQuicMetrics *)outMetrics",
    );
    let capacity_check = body
        .find("self.receiveBuffer.count >= kOxideMaxQueuedReceiveFrames")
        .expect("frame capacity check");
    let frame_copy = body
        .find("[self.incomingBytes subdataWithRange:NSMakeRange(0, frameLength)]")
        .expect("frame copy");
    let queue_add = body
        .find("[self.receiveBuffer addObject:frame];")
        .expect("receive queue append");

    assert!(source.contains("kOxideMaxQueuedReceiveFrames = 64"));
    assert!(source.contains("kOxideMaxQueuedReceiveBytes = 32 * 1024 * 1024"));
    assert!(capacity_check < frame_copy && frame_copy < queue_add);
    assert!(body.contains("kOxideMaxQueuedReceiveBytes - self.queuedReceiveBytes"));
    assert!(body.contains("Oxide network receive queue overflow"));
    assert!(body.contains("[self close];"));
    assert!(body.contains("self.queuedReceiveBytes += frame.length;"));
}

#[test]
fn network_bridge_closes_terminally_on_invalid_frame_length() {
    let source = include_str!("../src/ios/network.m");
    let drain_body = source_between(
        source,
        "- (void)drainIncomingBytes",
        "- (BOOL)copyMetrics:(struct OxideQuicMetrics *)outMetrics",
    );
    let invalid_body = source_between(
        drain_body,
        "if (frameLength < 16 || frameLength > kOxideMaxFrameBytes)",
        "if (self.incomingBytes.length < frameLength)",
    );
    let close_body = source_between(
        source,
        "- (void)closeOnQueue",
        "- (BOOL)copyClosedState",
    );

    assert!(invalid_body.contains("[self.incomingBytes setLength:0];"));
    assert!(invalid_body.contains("[self close];"));
    assert!(close_body.contains("self.closed = YES;"));
    assert!(close_body.contains("dispatch_semaphore_signal(_receiveSignal);"));
}

#[test]
fn network_bridge_uses_one_monotonic_deadline_for_send() {
    let source = include_str!("../src/ios/network.m");
    let wait_body = source_between(
        source,
        "- (BOOL)waitForWritableConnection:(uint64_t)deadlineNs",
        "- (BOOL)sendBytes:(const uint8_t *)data",
    );
    let send_body = source_between(
        source,
        "- (BOOL)sendBytes:(const uint8_t *)data",
        "- (NSData *)popReceived:(uint64_t)timeoutMs",
    );

    assert!(source.contains("clock_gettime_nsec_np(CLOCK_MONOTONIC)"));
    assert!(wait_body.contains("monotonic_remaining_ns(deadlineNs)"));
    assert!(!wait_body.contains("NSDate"));
    assert!(send_body.contains("uint64_t overallDeadlineNs ="));
    assert!(send_body.contains("[self waitForWritableConnection:overallDeadlineNs]"));
    assert!(send_body.contains("monotonic_remaining_ns(overallDeadlineNs)"));
}

#[test]
fn network_bridge_serializes_rust_facing_session_state() {
    let source = include_str!("../src/ios/network.m");
    let ready_body = source_between(
        source,
        "- (BOOL)waitForReady:(uint64_t)timeoutMs",
        "- (BOOL)isWritableOnQueue",
    );
    let writable_body = source_between(
        source,
        "- (BOOL)waitForWritableConnection:(uint64_t)deadlineNs",
        "- (BOOL)sendBytes:(const uint8_t *)data",
    );
    let close_body = source_between(
        source,
        "- (void)close\n{",
        "- (BOOL)copyClosedState",
    );
    let closed_body = source_between(
        source,
        "- (BOOL)copyClosedState",
        "@end",
    );

    assert!(source.contains("dispatch_queue_set_specific("));
    assert!(source.contains("static void quic_sync(dispatch_block_t block)"));
    assert!(source.contains("@property(nonatomic, strong, readonly) dispatch_queue_t queue;"));
    assert!(source
        .contains("@property(nonatomic, strong, readonly) dispatch_semaphore_t readySignal;"));
    assert!(source
        .contains("@property(nonatomic, strong, readonly) dispatch_semaphore_t receiveSignal;"));
    assert!(ready_body.contains("quic_sync(^{"));
    assert!(writable_body.contains("quic_sync(^{"));
    assert!(close_body.contains("quic_sync(^{"));
    assert!(close_body.contains("[self closeOnQueue];"));
    assert!(closed_body.contains("quic_sync(^{"));
}

#[test]
fn network_bridge_serializes_send_start_timeout_and_completion() {
    let source = include_str!("../src/ios/network.m");
    let body = source_between(
        source,
        "- (BOOL)sendBytes:(const uint8_t *)data",
        "- (NSData *)popReceived:(uint64_t)timeoutMs",
    );
    let begin_gate = body
        .find("monotonic_remaining_ns(overallDeadlineNs) == 0")
        .expect("send start deadline gate");
    let state_gate = body
        .find("![self isWritableOnQueue]")
        .expect("send start state gate");
    let send = body
        .find("nw_connection_send(")
        .expect("serialized Network.framework send");
    let completion_deadline = body[send..]
        .find("monotonic_remaining_ns(overallDeadlineNs) == 0")
        .expect("completion deadline arbitration")
        + send;

    assert!(body.contains("__block enum OxideSendOutcome outcome"));
    assert!(!body.contains("__block BOOL ok"));
    assert!(begin_gate < state_gate && state_gate < send);
    assert!(body[..send].contains("quic_sync(^{"));
    assert!(body.contains("sendConnection = self->_connection;"));
    assert!(completion_deadline > send);
    assert!(body.matches("outcome = OxideSendOutcomeTimedOut;").count() >= 2);
    assert!(body.matches("self->_connection == sendConnection").count() >= 2);
    assert!(body.matches("[self closeOnQueue];").count() >= 2);
    assert!(body.contains("return outcome == OxideSendOutcomeSucceeded;"));
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
        "- (void)handleFailure:(nw_error_t)error",
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
