#include <metal_stdlib>
using namespace metal;

struct Scene3dVertex
{
   float3 position [[attribute(0)]];
};

struct Scene3dColorVertex
{
   float3 position [[attribute(0)]];
   float4 color    [[attribute(1)]];
};

struct Scene3dUniforms
{
   float4x4 mvp;
};

struct Scene3dMaterial
{
   float4 color;
   uint material;
   packed_float3 _pad;
   float4 params;
};

struct Scene3dRaster
{
   float4 position [[position]];
   float3 local_position;
   uint instance_id [[flat]];
};

struct Scene3dColorRaster
{
   float4 position [[position]];
   float4 color;
   uint instance_id [[flat]];
};

vertex Scene3dRaster v_scene3d(Scene3dVertex in_vertex [[stage_in]], uint instance_id [[instance_id]], device const Scene3dUniforms *uniforms [[buffer(1)]])
{
   Scene3dRaster raster;
   raster.position = uniforms[instance_id].mvp * float4(in_vertex.position, 1.0);
   raster.local_position = in_vertex.position;
   raster.instance_id = instance_id;
   return raster;
}

vertex Scene3dColorRaster v_scene3d_color(Scene3dColorVertex in_vertex [[stage_in]], uint instance_id [[instance_id]], device const Scene3dUniforms *uniforms [[buffer(1)]])
{
   Scene3dColorRaster raster;
   raster.position = uniforms[instance_id].mvp * float4(in_vertex.position, 1.0);
   raster.color = in_vertex.color;
   raster.instance_id = instance_id;
   return raster;
}

fragment float4 f_scene3d(Scene3dRaster raster [[stage_in]], device const Scene3dMaterial *materials [[buffer(0)]])
{
   device const Scene3dMaterial &mat = materials[raster.instance_id];
   float4 c = mat.color;
   if (mat.material == 1) {
      float radius = max(mat.params.x, 0.001);
      float2 center_position = mat.params.yz;
      float2 p = raster.local_position.xy - center_position;
      float r = length(p) / radius;
      float radial = 1.0 - smoothstep(0.0, 1.05, r);
      float2 dir = normalize(float2(-0.45, 0.89));
      float diag = dot(normalize(p + float2(0.0001)), dir) * 0.5 + 0.5;
      diag = smoothstep(0.15, 1.0, diag);
      float3 base = clamp(c.rgb, 0.0, 1.0);
      float edge_darken = clamp(mat.params.w, 0.52, 0.82);
      float3 edge_col = base * edge_darken;
      float3 center_col = min(base * 1.42 + float3(0.08), float3(1.0));
      float light = clamp(radial * 0.78 + diag * 0.16, 0.0, 1.0);
      c.rgb = mix(edge_col, center_col, light);
      c.a = mat.color.a;
   } else if (mat.material == 2) {
      c.rgb = min(c.rgb * max(mat.params.x, 1.0), float3(1.0));
   }
   return c;
}

fragment float4 f_scene3d_color(Scene3dColorRaster raster [[stage_in]], device const Scene3dMaterial *materials [[buffer(0)]])
{
   device const Scene3dMaterial &mat = materials[raster.instance_id];
   float4 c = raster.color;
   c.rgb *= mat.color.rgb;
   c.a *= mat.color.a;
   return c;
}
