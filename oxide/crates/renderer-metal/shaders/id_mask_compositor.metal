#include <metal_stdlib>
using namespace metal;

struct IdMaskCompositorParams
{
  float4 viewport;
  float2 mask_size;
  float mask_scale;
  float darken_background_alpha;
  uint mode;
  uint glow_enabled;
  float polish_radius_px;
  float fallback_radius_px;
  float4 exterior_halo;
  float4 city_fill_colors[4];
  float4 city_edge_colors[4];
  float4 city_seam_colors[4];
  float4 neighborhood_colors[32];
};

struct IdMaskRasterParams
{
  float2 mask_size;
  float use_world_position;
  float visible_hemisphere;
  float4x4 world_to_clip;
  float4x4 model_to_world;
  float4 camera_eye_front_min;
  float4 normal_scale;
};

struct IdMaskRasterVertexIn
{
  // Matches the packed Rust/WebGPU byte layout exactly:
  // f32x2 position, f32x4 world position, u32 city, u32 neighborhood.
  // Plain float vectors would add Metal-side alignment padding and corrupt
  // native map masks.
  packed_float2 position_px;
  packed_float4 position_world;
  uint city_id;
  uint neighborhood_id;
};

struct IdMaskRasterOut
{
  float4 position [[position]];
  uint city_id [[flat]];
  uint neighborhood_id [[flat]];
  float frontness;
  float visible_hemisphere;
  float visible_front_min;
};

struct IdMaskRasterTargets
{
  uint4 city [[color(0)]];
  uint4 neighborhood [[color(1)]];
};

struct IdMaskCompositorRaster
{
  float4 position [[position]];
  float2 pos_dp;
  float2 pos_mask;
};

struct IdMaskFieldParams
{
  float2 mask_size;
  float jump;
  float _pad0;
};

struct IdMaskFieldRaster
{
  float4 position [[position]];
};

struct IdMaskWideFieldTargets
{
  float4 city [[color(0)]];
  float4 seam [[color(1)]];
};

constant uint ID_MASK_PACKED_INVALID = 0xffffu;

vertex IdMaskRasterOut v_id_mask_raster(uint vid [[vertex_id]],
                                        const device IdMaskRasterVertexIn *vertices [[buffer(0)]],
                                        constant IdMaskRasterParams &params [[buffer(1)]])
{
  IdMaskRasterVertexIn vtx = vertices[vid];
  IdMaskRasterOut out;
  out.frontness = 1.0;
  out.visible_hemisphere = params.visible_hemisphere;
  out.visible_front_min = params.camera_eye_front_min.w;
  if (params.use_world_position > 0.5)
  {
    float4 position_world = float4(vtx.position_world);
    out.position = params.world_to_clip * position_world;
    if (params.visible_hemisphere > 0.5)
    {
      float3 normal = normalize((params.model_to_world * position_world).xyz * params.normal_scale.xyz);
      out.frontness = dot(normal, normalize(params.camera_eye_front_min.xyz));
    }
  }
  else
  {
    float2 mask_size = max(params.mask_size, float2(1.0));
    float2 normalized = float2(vtx.position_px) / mask_size;
    out.position = float4(normalized.x * 2.0 - 1.0, 1.0 - normalized.y * 2.0, 0.0, 1.0);
  }
  out.city_id = vtx.city_id;
  out.neighborhood_id = vtx.neighborhood_id;
  return out;
}

fragment IdMaskRasterTargets f_id_mask_raster(IdMaskRasterOut in [[stage_in]])
{
  if (in.visible_hemisphere > 0.5 && in.frontness < in.visible_front_min) {
    discard_fragment();
  }
  IdMaskRasterTargets out;
  out.city = uint4(in.city_id, 0u, 0u, 1u);
  out.neighborhood = uint4(in.neighborhood_id, 0u, 0u, 1u);
  return out;
}

vertex IdMaskCompositorRaster v_id_mask_compositor(uint vid [[vertex_id]],
                                                   constant IdMaskCompositorParams &params [[buffer(0)]])
{
  float2 offs[6] = {float2(0.0, 0.0), float2(1.0, 0.0), float2(0.0, 1.0),
                    float2(0.0, 1.0), float2(1.0, 0.0), float2(1.0, 1.0)};
  float2 local = offs[vid] * params.viewport.zw;
  float2 dp = params.viewport.xy + local;
  float2 clip;
  clip.x = ((dp.x - params.viewport.x) / max(params.viewport.z, 1e-5)) * 2.0 - 1.0;
  clip.y = 1.0 - ((dp.y - params.viewport.y) / max(params.viewport.w, 1e-5)) * 2.0;
  IdMaskCompositorRaster out;
  out.position = float4(clip, 0.0, 1.0);
  out.pos_dp = dp;
  out.pos_mask = local * params.mask_scale;
  return out;
}

inline uint read_mask(texture2d<uint, access::read> tex, int2 p, uint2 size)
{
  if (p.x < 0 || p.y < 0 || p.x >= int(size.x) || p.y >= int(size.y)) {
    return 0;
  }
  return tex.read(uint2(p)).r;
}

inline float4 read_field(texture2d<float, access::read> tex, int2 p, uint2 size)
{
  if (p.x < 0 || p.y < 0 || p.x >= int(size.x) || p.y >= int(size.y)) {
    return float4(-1.0, -1.0, 0.0, 0.0);
  }
  return tex.read(uint2(p));
}

inline uint4 read_packed_field(texture2d<uint, access::read> tex, int2 p, uint2 size)
{
  if (p.x < 0 || p.y < 0 || p.x >= int(size.x) || p.y >= int(size.y)) {
    return uint4(ID_MASK_PACKED_INVALID);
  }
  return tex.read(uint2(p));
}

inline bool packed_seed_valid(uint2 seed)
{
  return seed.x != ID_MASK_PACKED_INVALID && seed.y != ID_MASK_PACKED_INVALID;
}

inline float4 unpack_seed_coordinate(uint2 seed)
{
  if (!packed_seed_valid(seed)) {
    return float4(-1.0, -1.0, 0.0, 0.0);
  }
  return float4(float2(seed), 1.0, 0.0);
}

inline bool field_valid(float4 field)
{
  return field.x >= -0.5 && field.y >= -0.5 && field.z >= 0.5;
}

inline float field_distance(float4 field, int2 p)
{
  if (!field_valid(field)) {
    return 1000000.0;
  }
  return length(field.xy - float2(p));
}

inline uint field_city(float4 field)
{
  return uint(round(clamp(field.z, 0.0, 255.0)));
}

inline uint field_neighborhood(float4 field)
{
  return uint(round(clamp(field.w, 0.0, 255.0)));
}

inline uint resolved_field_city(float4 field,
                                texture2d<uint, access::read> city_tex,
                                uint2 size,
                                bool recover_ids)
{
  if (!field_valid(field)) {
    return 0u;
  }
  return recover_ids ? read_mask(city_tex, int2(field.xy), size) : field_city(field);
}

inline uint resolved_field_neighborhood(float4 field,
                                        texture2d<uint, access::read> neighborhood_tex,
                                        uint2 size,
                                        bool recover_ids)
{
  if (!field_valid(field)) {
    return 0u;
  }
  return recover_ids ? read_mask(neighborhood_tex, int2(field.xy), size)
                     : field_neighborhood(field);
}

vertex IdMaskFieldRaster v_id_mask_field(uint vid [[vertex_id]])
{
  const float2 pos[6] = {
    float2(-1.0, -1.0), float2(1.0, -1.0), float2(-1.0, 1.0),
    float2(-1.0, 1.0), float2(1.0, -1.0), float2(1.0, 1.0)};
  IdMaskFieldRaster out;
  out.position = float4(pos[vid], 0.0, 1.0);
  return out;
}

inline uint2 field_size(constant IdMaskFieldParams &params)
{
  return uint2(uint(max(params.mask_size.x, 1.0)), uint(max(params.mask_size.y, 1.0)));
}

inline int2 field_pixel(float4 pos, uint2 size)
{
  return int2(clamp(pos.xy, float2(0.0), float2(size) - float2(1.0)));
}

inline float seed_distance2(float4 seed, int2 p)
{
  if (!field_valid(seed)) {
    return 1.0e30;
  }
  float2 delta = seed.xy - float2(p);
  return dot(delta, delta);
}

inline bool is_seam_seed(texture2d<uint, access::read> city_tex,
                         texture2d<uint, access::read> neighborhood_tex,
                         int2 p,
                         uint2 size)
{
  uint city = read_mask(city_tex, p, size);
  uint neighborhood = read_mask(neighborhood_tex, p, size);
  if (city == 0u || neighborhood == 0u) {
    return false;
  }
  for (int oy = -1; oy <= 1; ++oy) {
    for (int ox = -1; ox <= 1; ++ox) {
      if (ox == 0 && oy == 0) {
        continue;
      }
      int2 q = p + int2(ox, oy);
      if (read_mask(city_tex, q, size) == city) {
        uint other = read_mask(neighborhood_tex, q, size);
        if (other != 0u && other != neighborhood) {
          return true;
        }
      }
    }
  }
  return false;
}

fragment uint4 f_id_mask_field_seed(IdMaskFieldRaster in [[stage_in]],
                                    texture2d<uint, access::read> city_tex [[texture(0)]],
                                    texture2d<uint, access::read> neighborhood_tex [[texture(1)]],
                                    constant IdMaskFieldParams &params [[buffer(0)]])
{
  uint2 size = field_size(params);
  int2 p = field_pixel(in.position, size);
  uint city = read_mask(city_tex, p, size);
  uint2 invalid = uint2(ID_MASK_PACKED_INVALID);
  uint2 coordinate = uint2(p);
  return uint4(city == 0u ? invalid : coordinate,
               is_seam_seed(city_tex, neighborhood_tex, p, size) ? coordinate : invalid);
}

fragment IdMaskWideFieldTargets f_id_mask_field_seed_wide(
    IdMaskFieldRaster in [[stage_in]],
    texture2d<uint, access::read> city_tex [[texture(0)]],
    texture2d<uint, access::read> neighborhood_tex [[texture(1)]],
    constant IdMaskFieldParams &params [[buffer(0)]])
{
  uint2 size = field_size(params);
  int2 p = field_pixel(in.position, size);
  uint city = read_mask(city_tex, p, size);
  uint neighborhood = read_mask(neighborhood_tex, p, size);
  IdMaskWideFieldTargets out;
  out.city = city == 0u ? float4(-1.0, -1.0, 0.0, 0.0)
                        : float4(float2(p), float(city), float(neighborhood));
  out.seam = is_seam_seed(city_tex, neighborhood_tex, p, size)
      ? float4(float2(p), float(city), 1.0)
      : float4(-1.0, -1.0, 0.0, 0.0);
  return out;
}

// The beauty compositor needs nearest-city and nearest-seam distances for
// crisp edges and glow. Do that once with jump flooding; never put a radius
// search back into f_id_mask_compositor, or high-res native/Web masks regress.
inline float4 best_jump_seed(texture2d<float, access::read> src,
                             int2 p,
                             uint2 size,
                             int jump)
{
  float4 best = read_field(src, p, size);
  float best_distance = seed_distance2(best, p);
  for (int oy = -1; oy <= 1; ++oy) {
    for (int ox = -1; ox <= 1; ++ox) {
      if (ox == 0 && oy == 0) {
        continue;
      }
      float4 candidate = read_field(src, p + int2(ox * jump, oy * jump), size);
      float distance = seed_distance2(candidate, p);
      if (distance < best_distance) {
        best = candidate;
        best_distance = distance;
      }
    }
  }
  return best;
}

inline float packed_seed_distance2(uint2 seed, int2 p)
{
  if (!packed_seed_valid(seed)) {
    return 1.0e30;
  }
  float2 delta = float2(seed) - float2(p);
  return dot(delta, delta);
}

inline uint4 best_jump_packed(texture2d<uint, access::read> src,
                              int2 p,
                              uint2 size,
                              int jump)
{
  uint4 best = read_packed_field(src, p, size);
  float city_distance = packed_seed_distance2(best.xy, p);
  float seam_distance = packed_seed_distance2(best.zw, p);
  for (int oy = -1; oy <= 1; ++oy) {
    for (int ox = -1; ox <= 1; ++ox) {
      if (ox == 0 && oy == 0) {
        continue;
      }
      uint4 candidate = read_packed_field(src, p + int2(ox * jump, oy * jump), size);
      float candidate_city_distance = packed_seed_distance2(candidate.xy, p);
      if (candidate_city_distance < city_distance) {
        best.xy = candidate.xy;
        city_distance = candidate_city_distance;
      }
      float candidate_seam_distance = packed_seed_distance2(candidate.zw, p);
      if (candidate_seam_distance < seam_distance) {
        best.zw = candidate.zw;
        seam_distance = candidate_seam_distance;
      }
    }
  }
  return best;
}

fragment uint4 f_id_mask_field_jump(IdMaskFieldRaster in [[stage_in]],
                                    texture2d<uint, access::read> src [[texture(0)]],
                                    constant IdMaskFieldParams &params [[buffer(0)]])
{
  uint2 size = field_size(params);
  int2 p = field_pixel(in.position, size);
  return best_jump_packed(src, p, size, max(int(round(params.jump)), 1));
}

fragment IdMaskWideFieldTargets f_id_mask_field_jump_wide(
    IdMaskFieldRaster in [[stage_in]],
    texture2d<float, access::read> city_src [[texture(0)]],
    texture2d<float, access::read> seam_src [[texture(1)]],
    constant IdMaskFieldParams &params [[buffer(0)]])
{
  uint2 size = field_size(params);
  int2 p = field_pixel(in.position, size);
  int jump = max(int(round(params.jump)), 1);
  IdMaskWideFieldTargets out;
  out.city = best_jump_seed(city_src, p, size, jump);
  out.seam = best_jump_seed(seam_src, p, size, jump);
  return out;
}

inline float gaussian_alpha(float distance_mask_px,
                            float mask_scale,
                            float sigma_px,
                            float max_alpha,
                            float cutoff_sigma)
{
  float distance_px = distance_mask_px / max(mask_scale, 1.0);
  if (distance_px > sigma_px * cutoff_sigma) {
    return 0.0;
  }
  float sigma = max(sigma_px, 0.001);
  return clamp(max_alpha * exp(-(distance_px * distance_px) / (2.0 * sigma * sigma)), 0.0, 1.0);
}

inline float4 composite_id_mask(IdMaskCompositorRaster in,
                                texture2d<uint, access::read> city_tex,
                                texture2d<uint, access::read> neighborhood_tex,
                                constant IdMaskCompositorParams &params,
                                float4 nearest_city_field,
                                float4 seam_field,
                                bool recover_ids)
{
  uint2 size = uint2(uint(max(params.mask_size.x, 1.0)), uint(max(params.mask_size.y, 1.0)));
  int2 p = int2(clamp(in.pos_mask, float2(0.0), params.mask_size - float2(1.0)));
  int polish_radius = int(ceil(params.polish_radius_px * params.mask_scale));
  int fallback_radius = int(ceil(params.fallback_radius_px * params.mask_scale));
  uint city_direct = read_mask(city_tex, p, size);
  float city_distance = field_distance(nearest_city_field, p);
  uint nearest_city = city_direct != 0u
      ? city_direct
      : resolved_field_city(nearest_city_field, city_tex, size, recover_ids);
  uint city = city_direct != 0u ? city_direct
                                : (city_distance <= float(polish_radius) ? nearest_city : 0u);
  uint neighborhood_direct = read_mask(neighborhood_tex, p, size);
  uint neighborhood = (city_direct == city && neighborhood_direct != 0u)
      ? neighborhood_direct
      : ((city_distance <= float(fallback_radius) && nearest_city == city)
             ? resolved_field_neighborhood(nearest_city_field, neighborhood_tex, size, recover_ids)
             : 0u);
  uint city_index = min(city, 3u);
  uint neighborhood_index = min(neighborhood, 31u);

  if (params.mode == 2u) {
    return city == 0 ? float4(0.0, 0.0, 0.0, 1.0) : float4(params.city_edge_colors[city_index].rgb, 1.0);
  }
  if (params.mode == 3u) {
    return neighborhood == 0 ? float4(0.0, 0.0, 0.0, 1.0) : float4(params.neighborhood_colors[neighborhood_index].rgb, 1.0);
  }

  if (params.mode == 1u) {
    if (city == 0u || !field_valid(seam_field)) {
      return float4(0.0, 0.0, 0.0, 1.0);
    }
    float seam_distance = field_distance(seam_field, p);
    float core = gaussian_alpha(seam_distance, params.mask_scale, 0.42, 1.0, 2.1);
    if (core <= 0.04 || resolved_field_city(seam_field, city_tex, size, recover_ids) != city) {
      return float4(0.0, 0.0, 0.0, 1.0);
    }
    return float4(1.0, 1.0, 1.0, 1.0);
  }

  if (city == 0) {
    if (params.glow_enabled == 0u) {
      return float4(0.0, 0.0, 0.0, clamp(params.darken_background_alpha, 0.0, 1.0));
    }
    uint halo_city = nearest_city;
    if (!field_valid(nearest_city_field) || halo_city == 0u) {
      return float4(0.0, 0.0, 0.0, clamp(params.darken_background_alpha, 0.0, 1.0));
    }
    float halo_distance = city_distance;
    float alpha = max(
        gaussian_alpha(halo_distance, params.mask_scale, params.exterior_halo.x, params.exterior_halo.y, 3.2),
        gaussian_alpha(halo_distance, params.mask_scale, params.exterior_halo.z, params.exterior_halo.w, 3.2));
    if (alpha <= 0.002) {
      return float4(0.0, 0.0, 0.0, clamp(params.darken_background_alpha, 0.0, 1.0));
    }
    return float4(params.city_edge_colors[min(halo_city, 3u)].rgb, alpha);
  }

  float2 normalized = in.pos_mask / max(params.mask_size, float2(1.0));
  float top_left_light = clamp((1.0 - normalized.x) * 0.55 + (1.0 - normalized.y) * 0.45, 0.0, 1.0);
  float light = 0.92 + 0.08 * top_left_light;
  float3 fill = min(params.neighborhood_colors[neighborhood_index].rgb * light, float3(1.0));

  if (params.glow_enabled != 0u && field_valid(seam_field)) {
    float seam_distance = field_distance(seam_field, p);
    float seam_halo = gaussian_alpha(seam_distance, params.mask_scale, 1.10, 0.22, 2.5);
    float seam_core = gaussian_alpha(seam_distance, params.mask_scale, 0.27, 0.82, 1.7);
    float seam_alpha = max(seam_halo, seam_core);
    if (seam_alpha > 0.002
        && resolved_field_city(seam_field, city_tex, size, recover_ids) == city) {
      float3 seam = params.city_seam_colors[city_index].rgb;
      fill = mix(fill, seam, clamp(seam_alpha, 0.0, 1.0));
    }
  }

  return float4(fill, 0.96);
}

fragment float4 f_id_mask_compositor(IdMaskCompositorRaster in [[stage_in]],
                                     texture2d<uint, access::read> city_tex [[texture(0)]],
                                     texture2d<uint, access::read> neighborhood_tex [[texture(1)]],
                                     texture2d<uint, access::read> field_tex [[texture(2)]],
                                     constant IdMaskCompositorParams &params [[buffer(0)]])
{
  uint2 size = uint2(uint(max(params.mask_size.x, 1.0)), uint(max(params.mask_size.y, 1.0)));
  int2 p = int2(clamp(in.pos_mask, float2(0.0), params.mask_size - float2(1.0)));
  uint4 packed = read_packed_field(field_tex, p, size);
  return composite_id_mask(in, city_tex, neighborhood_tex, params,
                           unpack_seed_coordinate(packed.xy),
                           unpack_seed_coordinate(packed.zw), true);
}

fragment float4 f_id_mask_compositor_wide(
    IdMaskCompositorRaster in [[stage_in]],
    texture2d<uint, access::read> city_tex [[texture(0)]],
    texture2d<uint, access::read> neighborhood_tex [[texture(1)]],
    texture2d<float, access::read> city_field_tex [[texture(2)]],
    texture2d<float, access::read> seam_field_tex [[texture(3)]],
    constant IdMaskCompositorParams &params [[buffer(0)]])
{
  uint2 size = uint2(uint(max(params.mask_size.x, 1.0)), uint(max(params.mask_size.y, 1.0)));
  int2 p = int2(clamp(in.pos_mask, float2(0.0), params.mask_size - float2(1.0)));
  return composite_id_mask(in, city_tex, neighborhood_tex, params,
                           read_field(city_field_tex, p, size),
                           read_field(seam_field_tex, p, size), false);
}
