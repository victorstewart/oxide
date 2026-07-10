#ifndef OXIDE_PLATFORM_IOS_NETWORK_H
#define OXIDE_PLATFORM_IOS_NETWORK_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef void *NametagQuicHandle;

enum NametagIosQuicPollResult
{
   NAMETAG_IOS_QUIC_POLL_TERMINAL = -1,
   NAMETAG_IOS_QUIC_POLL_IDLE = 0,
   NAMETAG_IOS_QUIC_POLL_FRAME = 1,
};

// Returns FRAME after copying one complete frame, IDLE when no frame is queued,
// and TERMINAL for invalid arguments or a failed/closed retained session.
// Queued frames drain before TERMINAL; out_len is zero except on FRAME.
int32_t nametag_ios_quic_poll_recv(NametagQuicHandle handle,
                                   uint8_t *buffer,
                                   size_t buffer_len,
                                   size_t *out_len);

#ifdef __cplusplus
}
#endif

#endif
