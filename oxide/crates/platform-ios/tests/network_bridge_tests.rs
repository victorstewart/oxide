fn source_between<'a>(source: &'a str, start_marker: &str, end_marker: &str) -> &'a str
{
   let start = source.rfind(start_marker).expect(start_marker);
   let end = source[start..].find(end_marker).expect(end_marker) + start;
   &source[start..end]
}

#[test]
fn forced_tcp_tls_retries_stay_on_tls_parameters()
{
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
   assert!(body.contains("forceTcpTls && _tlsParameters != NULL && canRetry"));
   assert!(body.contains("[self scheduleRetryWithParameters:_tlsParameters fallback:YES];"));
}
