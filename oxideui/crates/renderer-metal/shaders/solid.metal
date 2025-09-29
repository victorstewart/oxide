#include <metal_stdlib>
using namespace metal;

struct SolidVSIn { float2 pos [[attribute(0)]]; float2 uv [[attribute(1)]]; ushort4 rgba [[attribute(2)]]; };
struct SolidVSOut { float4 position [[position]]; };
struct SolidUniform { float4 color; };

vertex SolidVSOut v_solid(SolidVSIn in [[stage_in]]) {
    SolidVSOut o;
    o.position = float4(in.pos, 0.0, 1.0);
    return o;
}

fragment float4 f_solid(SolidVSOut in [[stage_in]], constant SolidUniform& uni [[buffer(1)]]) {
    (void)in;
    return uni.color;
}
