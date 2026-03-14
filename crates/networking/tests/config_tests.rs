use oxide_networking::{ConnectionConfig, ConnectionConfigError, Endpoint};

#[test]
fn config_roundtrip_serialization()
{
   let config = ConnectionConfig {
      application_id: "oxide.test".to_owned(),
      endpoint: Endpoint { host: "edge.oxide.dev".to_owned(), port: 4433 },
      alpn: vec!["hq-interop".to_owned(), "hq-29".to_owned()],
      idle_timeout_ms: 30_000,
      max_datagram_size: 1_200,
      allow_local_fallback: false,
   };

   let encoded = config.encode().expect("encode");
   let decoded = ConnectionConfig::decode(&encoded).expect("decode");
   assert_eq!(decoded, config);
}

#[test]
fn config_decode_rejects_truncated_payload()
{
   let config = ConnectionConfig {
      application_id: "oxide.test".to_owned(),
      endpoint: Endpoint { host: "edge.oxide.dev".to_owned(), port: 4433 },
      alpn: vec!["hq-interop".to_owned()],
      idle_timeout_ms: 10_000,
      max_datagram_size: 1_200,
      allow_local_fallback: true,
   };

   let mut encoded = config.encode().expect("encode");
   encoded.truncate(encoded.len().saturating_sub(2));
   let err = ConnectionConfig::decode(&encoded).expect_err("decode err");
   assert_eq!(err, ConnectionConfigError::UnexpectedEof);
}
