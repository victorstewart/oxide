#include <metal_stdlib>
using namespace metal;

struct NeonMarkerParams
{
   float4 viewport;
   uint marker_count;
   uint _pad0;
   uint _pad1;
   uint _pad2;
};

struct NeonMarkerInstance
{
   float2 center;
   float core_radius_px;
   float ring_radius_px;
   float ring_width_px;
   float halo_radius_px;
   float halo_sigma_px;
   float halo_alpha_max;
   float ring_alpha_max;
   packed_float4 core_color;
   packed_float4 ring_color;
   uint _tail_pad;
};

struct NeonMarkerRaster
{
   float4 position [[position]];
   float2 pos_dp;
   uint instance_id;
};

vertex NeonMarkerRaster v_neon_marker(uint vid [[vertex_id]],
                                      uint iid [[instance_id]],
                                      constant NeonMarkerParams &params [[buffer(0)]],
                                      constant NeonMarkerInstance *markers [[buffer(1)]])
{
   float2 offs[6] = {float2(-1.0, -1.0), float2(1.0, -1.0), float2(-1.0, 1.0),
                     float2(-1.0, 1.0), float2(1.0, -1.0), float2(1.0, 1.0)};
   NeonMarkerInstance marker = markers[iid];
   float radius = max(max(marker.halo_radius_px, marker.ring_radius_px + marker.ring_width_px), marker.core_radius_px);
   float2 dp = marker.center + offs[vid] * radius;
   float2 clip;
   clip.x = ((dp.x - params.viewport.x) / max(params.viewport.z, 1e-5)) * 2.0 - 1.0;
   clip.y = 1.0 - ((dp.y - params.viewport.y) / max(params.viewport.w, 1e-5)) * 2.0;
   NeonMarkerRaster out;
   out.position = float4(clip, 0.0, 1.0);
   out.pos_dp = dp;
   out.instance_id = iid;
   return out;
}

fragment float4 f_neon_marker(NeonMarkerRaster in [[stage_in]],
                              constant NeonMarkerParams &params [[buffer(0)]],
                              constant NeonMarkerInstance *markers [[buffer(1)]])
{
   if (in.instance_id >= params.marker_count) {
      return float4(0.0);
   }
   NeonMarkerInstance marker = markers[in.instance_id];
   float2 delta = in.pos_dp - marker.center;
   float distance = length(delta);
   if (distance > marker.halo_radius_px) {
      return float4(0.0);
   }

   if (distance <= marker.core_radius_px) {
      float edge = clamp(distance / max(marker.core_radius_px, 0.001), 0.0, 1.0);
      float alpha = marker.core_color.a * (1.0 - edge * 0.08);
      return float4(marker.core_color.rgb, alpha);
   }

   float ring_width = max(marker.ring_width_px, 0.001);
   float ring_alpha = clamp(1.0 - abs(distance - marker.ring_radius_px) / ring_width, 0.0, 1.0) * marker.ring_alpha_max;
   float sigma = max(marker.halo_sigma_px, 0.001);
   float halo_alpha = exp(-(distance * distance) / (2.0 * sigma * sigma)) * marker.halo_alpha_max;
   float alpha = max(ring_alpha, halo_alpha) * marker.ring_color.a;
   if (alpha <= 0.001) {
      return float4(0.0);
   }
   return float4(marker.ring_color.rgb, clamp(alpha, 0.0, 1.0));
}
