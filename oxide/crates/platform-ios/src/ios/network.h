#ifndef OXIDE_PLATFORM_IOS_NETWORK_H
#define OXIDE_PLATFORM_IOS_NETWORK_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

struct OxideQuicConfig
{
   uint32_t idle_timeout_ms;
   uint16_t max_datagram_size;
   uint16_t keepalive_interval_secs;
   bool allow_fallback;
   const char *alpn;
   bool force_tcp_tls;
};

struct OxideQuicRetryPolicy
{
   uint32_t max_attempts;
   uint64_t initial_backoff_ms;
   uint64_t max_backoff_ms;
};

struct OxideTlsTrustAnchor
{
   const uint8_t *data;
   size_t len;
};

struct OxideQuicTlsConfig
{
   const uint8_t *identity_der;
   size_t identity_der_len;
   const uint8_t *private_key_pkcs8;
   size_t private_key_pkcs8_len;
   const struct OxideTlsTrustAnchor *trust_anchors;
   size_t trust_anchor_count;
   bool enforce_hostname;
};

struct OxideQuicMetrics
{
   uint64_t handshake_ms;
   uint64_t resume_ms;
   uint64_t payload_bytes;
   uint64_t total_bytes;
   uint32_t attempts;
   bool fallback_used;
};

struct OxideReachabilityStatus
{
   bool reachable;
   bool expensive;
   uint8_t path_kind;
};

typedef void *OxideQuicHandle;
typedef void *OxideReachabilityHandle;
typedef void (*OxideReachabilityCallback)(uint32_t status,
                                          uint32_t interface_kind,
                                          uint8_t expensive);

enum OxideIosQuicPollResult
{
   OXIDE_IOS_QUIC_POLL_TERMINAL = -1,
   OXIDE_IOS_QUIC_POLL_IDLE = 0,
   OXIDE_IOS_QUIC_POLL_FRAME = 1,
};

OxideQuicHandle oxide_ios_quic_connect(
    const char *endpoint,
    const struct OxideQuicConfig *config,
    const struct OxideQuicRetryPolicy *retry,
    const struct OxideQuicTlsConfig *tls);
bool oxide_ios_quic_metrics(OxideQuicHandle handle,
                            struct OxideQuicMetrics *out_metrics);
bool oxide_ios_quic_wait_ready(OxideQuicHandle handle, uint64_t timeout_ms);
void oxide_ios_quic_close(OxideQuicHandle handle);
bool oxide_ios_quic_send(OxideQuicHandle handle,
                         const uint8_t *data,
                         size_t len,
                         uint64_t timeout_ms);
bool oxide_ios_quic_recv(OxideQuicHandle handle,
                         uint8_t *buffer,
                         size_t buffer_len,
                         size_t *out_len,
                         uint64_t timeout_ms);

// Returns FRAME after copying one complete frame, IDLE when no frame is queued,
// and TERMINAL for invalid arguments or a failed/closed retained session.
// Queued frames drain before TERMINAL; out_len is zero except on FRAME.
int32_t oxide_ios_quic_poll_recv(OxideQuicHandle handle,
                                 uint8_t *buffer,
                                 size_t buffer_len,
                                 size_t *out_len);

OxideReachabilityHandle oxide_ios_reachability_start(void);
bool oxide_ios_reachability_poll(
    OxideReachabilityHandle handle,
    struct OxideReachabilityStatus *out_status);
void oxide_ios_reachability_close(OxideReachabilityHandle handle);

void oxide_host_net_set_reachability_callback(OxideReachabilityCallback callback);
int32_t oxide_host_net_start_reachability(void);
void oxide_host_net_stop_reachability(void);

#ifdef __cplusplus
}
#endif

#endif
