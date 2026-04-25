#include <metal_stdlib>
using namespace metal;

struct Scene3dVertex
{
   float3 position [[attribute(0)]];
};

struct Scene3dUniforms
{
   float4x4 mvp;
};

struct Scene3dColor
{
   float4 color;
};

struct Scene3dRaster
{
   float4 position [[position]];
};

vertex Scene3dRaster v_scene3d(Scene3dVertex in_vertex [[stage_in]], constant Scene3dUniforms &uniforms [[buffer(1)]])
{
   Scene3dRaster raster;
   raster.position = uniforms.mvp * float4(in_vertex.position, 1.0);
   return raster;
}

fragment float4 f_scene3d(Scene3dRaster raster [[stage_in]], constant Scene3dColor &color [[buffer(0)]])
{
   return color.color;
}
