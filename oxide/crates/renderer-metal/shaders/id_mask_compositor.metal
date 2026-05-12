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
  float4 city_fill_colors[4];
  float4 city_edge_colors[4];
  float4 city_seam_colors[4];
  float4 neighborhood_colors[32];
};

struct IdMaskRasterParams
{
  float2 mask_size;
  float2 _pad0;
};

struct IdMaskRasterVertexIn
{
  float2 position_px;
  uint city_id;
  uint neighborhood_id;
};

struct IdMaskRasterOut
{
  float4 position [[position]];
  uint city_id [[flat]];
  uint neighborhood_id [[flat]];
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

vertex IdMaskRasterOut v_id_mask_raster(uint vid [[vertex_id]],
                                        const device IdMaskRasterVertexIn *vertices [[buffer(0)]],
                                        constant IdMaskRasterParams &params [[buffer(1)]])
{
  IdMaskRasterVertexIn vtx = vertices[vid];
  float2 mask_size = max(params.mask_size, float2(1.0));
  float2 normalized = vtx.position_px / mask_size;
  IdMaskRasterOut out;
  out.position = float4(normalized.x * 2.0 - 1.0, 1.0 - normalized.y * 2.0, 0.0, 1.0);
  out.city_id = vtx.city_id;
  out.neighborhood_id = vtx.neighborhood_id;
  return out;
}

fragment IdMaskRasterTargets f_id_mask_raster(IdMaskRasterOut in [[stage_in]])
{
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

inline int2 direction_sample_offset(int direction_index, int radius)
{
  const float2 dirs[24] = {
      float2(1.0, 0.0), float2(0.966, 0.259), float2(0.866, 0.5),
      float2(0.707, 0.707), float2(0.5, 0.866), float2(0.259, 0.966),
      float2(0.0, 1.0), float2(-0.259, 0.966), float2(-0.5, 0.866),
      float2(-0.707, 0.707), float2(-0.866, 0.5), float2(-0.966, 0.259),
      float2(-1.0, 0.0), float2(-0.966, -0.259), float2(-0.866, -0.5),
      float2(-0.707, -0.707), float2(-0.5, -0.866), float2(-0.259, -0.966),
      float2(0.0, -1.0), float2(0.259, -0.966), float2(0.5, -0.866),
      float2(0.707, -0.707), float2(0.866, -0.5), float2(0.966, -0.259)};
  return int2(round(dirs[direction_index] * float(radius)));
}

inline uint conservative_polished_city(texture2d<uint, access::read> city_tex,
                                       int2 p,
                                       uint2 size,
                                       int max_radius)
{
  uint direct = read_mask(city_tex, p, size);
  if (direct != 0 || max_radius <= 0) {
    return direct;
  }
  uint counts[4] = {0u, 0u, 0u, 0u};
  uint sample_count = 0u;
  int radius_squared = max_radius * max_radius;
  for (int oy = -max_radius; oy <= max_radius; ++oy) {
    for (int ox = -max_radius; ox <= max_radius; ++ox) {
      if (ox * ox + oy * oy > radius_squared) {
        continue;
      }
      ++sample_count;
      uint city = min(read_mask(city_tex, p + int2(ox, oy), size), 3u);
      if (city != 0u) {
        ++counts[city];
      }
    }
  }
  uint best_city = 0u;
  uint best_count = 0u;
  for (uint city = 1u; city <= 3u; ++city) {
    if (counts[city] > best_count) {
      best_city = city;
      best_count = counts[city];
    }
  }
  float coverage = sample_count == 0u ? 0.0 : float(best_count) / float(sample_count);
  return coverage >= 0.68 ? best_city : 0u;
}

inline bool is_internal_seam_pixel(texture2d<uint, access::read> city_tex,
                                   texture2d<uint, access::read> neighborhood_tex,
                                   int2 p,
                                   uint2 size)
{
  uint city = read_mask(city_tex, p, size);
  uint neighborhood = read_mask(neighborhood_tex, p, size);
  if (city == 0 || neighborhood == 0) {
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
        if (other != 0 && other != neighborhood) {
          return true;
        }
      }
    }
  }
  return false;
}

inline uint nearest_city_id(texture2d<uint, access::read> city_tex,
                            int2 p,
                            uint2 size,
                            int max_radius)
{
  uint direct = read_mask(city_tex, p, size);
  if (direct != 0) {
    return direct;
  }
  for (int r = 1; r <= max_radius; r += 2) {
    for (int i = 0; i < 24; ++i) {
      int2 q = p + direction_sample_offset(i, r);
      uint city = read_mask(city_tex, q, size);
      if (city != 0) {
        return city;
      }
    }
  }
  return 0;
}

inline float nearest_city_distance(texture2d<uint, access::read> city_tex,
                                   int2 p,
                                   uint2 size,
                                   int max_radius)
{
  if (read_mask(city_tex, p, size) != 0) {
    return 0.0;
  }
  for (int r = 1; r <= max_radius; r += 2) {
    for (int i = 0; i < 24; ++i) {
      int2 q = p + direction_sample_offset(i, r);
      if (read_mask(city_tex, q, size) != 0) {
        return float(r);
      }
    }
  }
  return float(max_radius + 1);
}

inline uint nearest_neighborhood_for_city(texture2d<uint, access::read> city_tex,
                                          texture2d<uint, access::read> neighborhood_tex,
                                          int2 p,
                                          uint2 size,
                                          uint city,
                                          int max_radius)
{
  uint direct_city = read_mask(city_tex, p, size);
  uint direct_neighborhood = read_mask(neighborhood_tex, p, size);
  if (direct_city == city && direct_neighborhood != 0) {
    return direct_neighborhood;
  }
  for (int r = 1; r <= max_radius; ++r) {
    for (int i = 0; i < 24; ++i) {
      int2 q = p + direction_sample_offset(i, r);
      if (read_mask(city_tex, q, size) == city) {
        uint neighborhood = read_mask(neighborhood_tex, q, size);
        if (neighborhood != 0) {
          return neighborhood;
        }
      }
    }
  }
  return 0;
}

inline float nearest_seam_distance(texture2d<uint, access::read> city_tex,
                                   texture2d<uint, access::read> neighborhood_tex,
                                   int2 p,
                                   uint2 size,
                                   int max_radius)
{
  if (is_internal_seam_pixel(city_tex, neighborhood_tex, p, size)) {
    return 0.0;
  }
  for (int r = 1; r <= max_radius; ++r) {
    for (int i = 0; i < 24; ++i) {
      int2 q = p + direction_sample_offset(i, r);
      if (is_internal_seam_pixel(city_tex, neighborhood_tex, q, size)) {
        return float(r);
      }
    }
  }
  return float(max_radius + 1);
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

fragment float4 f_id_mask_compositor(IdMaskCompositorRaster in [[stage_in]],
                                     texture2d<uint, access::read> city_tex [[texture(0)]],
                                     texture2d<uint, access::read> neighborhood_tex [[texture(1)]],
                                     constant IdMaskCompositorParams &params [[buffer(0)]])
{
  uint2 size = uint2(uint(max(params.mask_size.x, 1.0)), uint(max(params.mask_size.y, 1.0)));
  int2 p = int2(clamp(in.pos_mask, float2(0.0), params.mask_size - float2(1.0)));
  int polish_radius = int(ceil(params.polish_radius_px * params.mask_scale));
  int fallback_radius = int(ceil(params.fallback_radius_px * params.mask_scale));
  uint city = conservative_polished_city(city_tex, p, size, polish_radius);
  uint neighborhood = nearest_neighborhood_for_city(city_tex,
                                                    neighborhood_tex,
                                                    p,
                                                    size,
                                                    city,
                                                    fallback_radius);
  uint city_index = min(city, 3u);
  uint neighborhood_index = min(neighborhood, 31u);

  if (params.mode == 2u) {
    return city == 0 ? float4(0.0, 0.0, 0.0, 1.0) : float4(params.city_edge_colors[city_index].rgb, 1.0);
  }
  if (params.mode == 3u) {
    return neighborhood == 0 ? float4(0.0, 0.0, 0.0, 1.0) : float4(params.neighborhood_colors[neighborhood_index].rgb, 1.0);
  }

  float seam_distance = nearest_seam_distance(city_tex, neighborhood_tex, p, size,
                                              int(ceil(5.0 * params.mask_scale)));
  if (params.mode == 1u) {
    float core = gaussian_alpha(seam_distance, params.mask_scale, 0.42, 1.0, 2.1);
    return core > 0.04 && city != 0 ? float4(1.0, 1.0, 1.0, 1.0)
                                    : float4(0.0, 0.0, 0.0, 1.0);
  }

  if (city == 0) {
    if (params.glow_enabled == 0u) {
      return float4(0.0, 0.0, 0.0, clamp(params.darken_background_alpha, 0.0, 1.0));
    }
    uint halo_city = nearest_city_id(city_tex, p, size, int(ceil(18.0 * params.mask_scale)));
    if (halo_city == 0u) {
      return float4(0.0, 0.0, 0.0, clamp(params.darken_background_alpha, 0.0, 1.0));
    }
    float halo_distance = nearest_city_distance(city_tex, p, size, int(ceil(18.0 * params.mask_scale)));
    float alpha = max(gaussian_alpha(halo_distance, params.mask_scale, 16.0, 0.04, 3.2),
                      gaussian_alpha(halo_distance, params.mask_scale, 8.5, 0.15, 3.2));
    if (alpha <= 0.002) {
      return float4(0.0, 0.0, 0.0, clamp(params.darken_background_alpha, 0.0, 1.0));
    }
    return float4(params.city_edge_colors[min(halo_city, 3u)].rgb, alpha);
  }

  float2 normalized = in.pos_mask / max(params.mask_size, float2(1.0));
  float top_left_light = clamp((1.0 - normalized.x) * 0.55 + (1.0 - normalized.y) * 0.45, 0.0, 1.0);
  float light = 0.92 + 0.08 * top_left_light;
  float3 fill = min(params.neighborhood_colors[neighborhood_index].rgb * light, float3(1.0));

  if (params.glow_enabled != 0u) {
    float seam_halo = gaussian_alpha(seam_distance, params.mask_scale, 1.10, 0.22, 2.5);
    float seam_core = gaussian_alpha(seam_distance, params.mask_scale, 0.27, 0.82, 1.7);
    float seam_alpha = max(seam_halo, seam_core);
    if (seam_alpha > 0.002) {
      float3 seam = params.city_seam_colors[city_index].rgb;
      fill = mix(fill, seam, clamp(seam_alpha, 0.0, 1.0));
    }
  }

  return float4(fill, 0.96);
}
