#import <Foundation/Foundation.h>
#import <stdbool.h>
#import <stdint.h>
#import <stdlib.h>
#import <string.h>

// Perf-host-only fallbacks for iOS bridges that Oxide does not implement yet.
// The UIKit perf harness does not exercise these product integrations, but the
// host app still needs linkable symbols when the full iOS host target is built.

typedef struct {
  const uint8_t *waypoint_ptr;
  size_t waypoint_len;
  const uint8_t *carrier_region_ptr;
  size_t carrier_region_len;
} OxideContactsState;

typedef struct {
  const uint8_t *identifier_ptr;
  size_t identifier_len;
  const uint8_t *given_name_ptr;
  size_t given_name_len;
  const uint8_t *family_name_ptr;
  size_t family_name_len;
  const void *phones_ptr;
  size_t phones_count;
  const void *emails_ptr;
  size_t emails_count;
} OxideContact;

typedef struct {
  const uint8_t *data_ptr;
  size_t data_len;
  uint32_t width;
  uint32_t height;
  size_t row_bytes;
} OxideImageData;

#ifndef OXIDE_HOST_USE_PLATFORM_CAMERA
typedef void (*OxideCameraPhotoCallback)(const void *event);

static OxideCameraPhotoCallback g_oxide_perf_camera_photo_callback = NULL;
#endif

void nametag_host_update_permission(int32_t domain, int32_t status) {
  (void)domain;
  (void)status;
}

void nametag_ios_handle_notification_response(NSDictionary *userInfo) {
  (void)userInfo;
}

#ifndef OXIDE_HOST_USE_PLATFORM_CAMERA
void oxide_host_set_camera_photo_callback(OxideCameraPhotoCallback callback) {
  g_oxide_perf_camera_photo_callback = callback;
}
#endif

static void oxide_perf_zero_contacts_state(OxideContactsState *state) {
  if (state == NULL) {
    return;
  }
  state->waypoint_ptr = NULL;
  state->waypoint_len = 0;
  state->carrier_region_ptr = NULL;
  state->carrier_region_len = 0;
}

static void oxide_perf_zero_image(OxideImageData *image) {
  if (image == NULL) {
    return;
  }
  image->data_ptr = NULL;
  image->data_len = 0;
  image->width = 0;
  image->height = 0;
  image->row_bytes = 0;
}

#ifndef OXIDE_HOST_USE_PLATFORM_CAMERA
int32_t oxide_cam_capture_photo(uint8_t high_speed_from_preview,
                                int32_t flash_mode) {
  (void)high_speed_from_preview;
  (void)flash_mode;
  return -1;
}

int32_t oxide_cam_set_audio_session_mode(int32_t mode) {
  (void)mode;
  return 0;
}

int32_t oxide_cam_set_flash_mode(int32_t mode) {
  (void)mode;
  return 0;
}

int32_t oxide_cam_set_focus_point(float x, float y) {
  (void)x;
  (void)y;
  return 0;
}

int32_t oxide_cam_set_torch_mode(int32_t mode, float level) {
  (void)mode;
  (void)level;
  return 0;
}

int32_t oxide_cam_set_zoom_factor(float factor) {
  (void)factor;
  return 0;
}

int32_t oxide_cam_query_formats(void **out_ptr, size_t *out_count) {
  if (out_ptr != NULL) {
    *out_ptr = NULL;
  }
  if (out_count != NULL) {
    *out_count = 0;
  }
  return 0;
}

int32_t oxide_cam_query_pixfmts(void **out_ptr, size_t *out_count) {
  return oxide_cam_query_formats(out_ptr, out_count);
}

void oxide_cam_caps_free(void *ptr) { free(ptr); }
#endif

int32_t oxide_contacts_fetch(const uint8_t *waypoint_ptr, size_t waypoint_len,
                             const OxideContact **out_contacts,
                             size_t *out_count, OxideContactsState *out_state) {
  (void)waypoint_ptr;
  (void)waypoint_len;
  if (out_contacts != NULL) {
    *out_contacts = NULL;
  }
  if (out_count != NULL) {
    *out_count = 0;
  }
  oxide_perf_zero_contacts_state(out_state);
  return 0;
}

void oxide_contacts_free(const OxideContact *contacts, size_t count) {
  (void)contacts;
  (void)count;
}

int32_t oxide_contacts_get_carrier_region(const uint8_t **out_ptr,
                                          size_t *out_len) {
  if (out_ptr != NULL) {
    *out_ptr = NULL;
  }
  if (out_len != NULL) {
    *out_len = 0;
  }
  return 1;
}

int32_t oxide_media_fetch_assets(uint8_t media_type_mask, int32_t limit,
                                 uint8_t ascending, const void **out_assets,
                                 size_t *out_count) {
  (void)media_type_mask;
  (void)limit;
  (void)ascending;
  if (out_assets != NULL) {
    *out_assets = NULL;
  }
  if (out_count != NULL) {
    *out_count = 0;
  }
  return 0;
}

void oxide_media_free_assets(const void *assets, size_t count) {
  (void)assets;
  (void)count;
}

int32_t oxide_media_load_thumbnail(const uint8_t *identifier_ptr,
                                   size_t identifier_len, uint8_t size,
                                   OxideImageData *out_image) {
  (void)identifier_ptr;
  (void)identifier_len;
  (void)size;
  oxide_perf_zero_image(out_image);
  return 0;
}

int32_t oxide_media_load_thumbnail_rgba(const uint8_t *identifier_ptr,
                                        size_t identifier_len, uint8_t size,
                                        OxideImageData *out_image) {
  return oxide_media_load_thumbnail(identifier_ptr, identifier_len, size,
                                    out_image);
}

int32_t oxide_media_load_full_image(const uint8_t *identifier_ptr,
                                    size_t identifier_len,
                                    OxideImageData *out_image) {
  return oxide_media_load_thumbnail(identifier_ptr, identifier_len, 0,
                                    out_image);
}

int32_t oxide_media_load_full_image_rgba(const uint8_t *identifier_ptr,
                                         size_t identifier_len,
                                         OxideImageData *out_image) {
  return oxide_media_load_thumbnail(identifier_ptr, identifier_len, 0,
                                    out_image);
}

void oxide_media_free_image_data(const uint8_t *data_ptr, size_t data_len) {
  (void)data_ptr;
  (void)data_len;
}

int32_t oxide_media_load_video_file(const uint8_t *identifier_ptr,
                                    size_t identifier_len,
                                    const uint8_t **out_path_ptr,
                                    size_t *out_path_len) {
  (void)identifier_ptr;
  (void)identifier_len;
  if (out_path_ptr != NULL) {
    *out_path_ptr = NULL;
  }
  if (out_path_len != NULL) {
    *out_path_len = 0;
  }
  return 1;
}

void oxide_media_free_string(const uint8_t *data_ptr, size_t data_len) {
  (void)data_ptr;
  (void)data_len;
}

bool oxide_telephony_home_country_iso(char *buffer, size_t buffer_len) {
  if (buffer != NULL && buffer_len > 0) {
    buffer[0] = '\0';
  }
  return false;
}

int32_t oxide_url_can_open(const uint8_t *url_ptr, size_t url_len) {
  (void)url_ptr;
  (void)url_len;
  return 0;
}

int32_t oxide_url_open(const uint8_t *url_ptr, size_t url_len) {
  (void)url_ptr;
  (void)url_len;
  return 0;
}
