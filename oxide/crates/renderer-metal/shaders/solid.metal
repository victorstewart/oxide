#include <metal_stdlib>
using namespace metal;

struct SolidVSIn { float2 pos [[attribute(0)]]; float2 uv [[attribute(1)]]; float4 rgba [[attribute(2)]]; };
struct SolidVSOut { float4 position [[position]]; float4 rgba; };
struct SolidUniform { float4 color; };
struct PreparedInstance { float4 matrix; float2 translation; float2 viewport; float4 opacityAndPadding; };

inline float2 preparedPosition(float2 local, constant PreparedInstance& instance)
{
    if (instance.opacityAndPadding.y != 0.0)
    {
        return local + instance.translation;
    }
    return float2(
        instance.matrix.x * local.x + instance.matrix.z * local.y + instance.translation.x,
        instance.matrix.y * local.x + instance.matrix.w * local.y + instance.translation.y
    );
}

vertex SolidVSOut v_solid(SolidVSIn in [[stage_in]], constant SolidUniform& uni [[buffer(1)]]) {
    SolidVSOut o;
    o.position = float4(in.pos, 0.0, 1.0);
    o.rgba = all(in.rgba == float4(0.0)) ? uni.color : in.rgba;
    return o;
}

vertex SolidVSOut v_prepared_solid(SolidVSIn in [[stage_in]], constant SolidUniform& uni [[buffer(1)]], constant PreparedInstance& instance [[buffer(2)]])
{
    SolidVSOut o;
    float2 dp = preparedPosition(in.pos, instance);
    float2 clip;
    clip.x = (dp.x / max(instance.viewport.x, 1.0)) * 2.0 - 1.0;
    clip.y = 1.0 - (dp.y / max(instance.viewport.y, 1.0)) * 2.0;
    o.position = float4(clip, 0.0, 1.0);
    o.rgba = all(in.rgba == float4(0.0)) ? uni.color : in.rgba;
    o.rgba.a *= instance.opacityAndPadding.x;
    return o;
}

fragment float4 f_solid(SolidVSOut in [[stage_in]]) {
    return in.rgba;
}
