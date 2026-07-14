#include <metal_stdlib>
using namespace metal;

struct TextVSIn { float2 pos [[attribute(0)]]; float2 uv [[attribute(1)]]; float4 rgba [[attribute(2)]]; };
struct TextVSOut { float4 position [[position]]; float2 uv; };
struct TextVPSize { float2 size; };
struct TextUniform { float4 color; };
struct PreparedInstance { float4 matrix; float2 translation; float2 viewport; float4 opacityAndPadding; };
struct GlyphGpuInstance { packed_float4 dst; packed_float4 uv; packed_float4 color; };
struct GlyphVSOut { float4 position [[position]]; float2 uv; float4 color; };

inline float2 preparedPosition(float2 local, constant PreparedInstance& instance);

constant float2 glyphCorners[4] = {
    float2(0.0, 0.0), float2(1.0, 0.0),
    float2(0.0, 1.0), float2(1.0, 1.0),
};

inline GlyphVSOut glyphVertex(uint vertexId, uint instanceId, device const GlyphGpuInstance* instances, float2 viewport, constant PreparedInstance* prepared)
{
    GlyphGpuInstance instance = instances[instanceId];
    float4 dst = float4(instance.dst);
    float4 uv = float4(instance.uv);
    float2 corner = glyphCorners[vertexId];
    float2 dp = dst.xy + corner * dst.zw;
    if (prepared != nullptr)
    {
        dp = preparedPosition(dp, *prepared);
    }
    float2 clip;
    clip.x = (dp.x / max(viewport.x, 1.0)) * 2.0 - 1.0;
    clip.y = 1.0 - (dp.y / max(viewport.y, 1.0)) * 2.0;
    GlyphVSOut out;
    out.position = float4(clip, 0.0, 1.0);
    out.uv = mix(uv.xy, uv.zw, corner);
    out.color = float4(instance.color);
    return out;
}

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

vertex TextVSOut v_text(TextVSIn in [[stage_in]], constant TextVPSize& vp [[buffer(1)]]) {
    TextVSOut o;
    // Interpret input position as device-independent pixels (dp) and map to clip space using vp size
    float2 clip;
    clip.x = (in.pos.x / max(vp.size.x, 1.0)) * 2.0 - 1.0;
    // App-space dp uses top-left origin with +Y downward.
    // Flip Y to match clip-space orientation.
    clip.y = 1.0 - (in.pos.y / max(vp.size.y, 1.0)) * 2.0;
    o.position = float4(clip, 0.0, 1.0);
    o.uv = in.uv;
    return o;
}

vertex TextVSOut v_prepared_text(TextVSIn in [[stage_in]], constant PreparedInstance& instance [[buffer(2)]])
{
    TextVSOut o;
    float2 dp = preparedPosition(in.pos, instance);
    float2 clip;
    clip.x = (dp.x / max(instance.viewport.x, 1.0)) * 2.0 - 1.0;
    clip.y = 1.0 - (dp.y / max(instance.viewport.y, 1.0)) * 2.0;
    o.position = float4(clip, 0.0, 1.0);
    o.uv = in.uv;
    return o;
}

vertex GlyphVSOut v_glyph(uint vertexId [[vertex_id]], uint instanceId [[instance_id]], device const GlyphGpuInstance* instances [[buffer(0)]], constant TextVPSize& vp [[buffer(1)]])
{
    return glyphVertex(vertexId, instanceId, instances, vp.size, nullptr);
}

vertex GlyphVSOut v_prepared_glyph(uint vertexId [[vertex_id]], uint instanceId [[instance_id]], device const GlyphGpuInstance* instances [[buffer(0)]], constant PreparedInstance& prepared [[buffer(2)]])
{
    return glyphVertex(vertexId, instanceId, instances, prepared.viewport, &prepared);
}

fragment float4 f_glyph(GlyphVSOut in [[stage_in]], texture2d<float> atlas [[texture(0)]], sampler s [[sampler(0)]])
{
    float alpha = atlas.sample(s, in.uv).r;
    return float4(in.color.rgb, in.color.a * alpha);
}

fragment float4 f_prepared_glyph(GlyphVSOut in [[stage_in]], texture2d<float> atlas [[texture(0)]], sampler s [[sampler(0)]], constant PreparedInstance& instance [[buffer(3)]])
{
    float alpha = atlas.sample(s, in.uv).r;
    return float4(in.color.rgb, in.color.a * alpha * instance.opacityAndPadding.x);
}

fragment float4 f_glyph_sdf(GlyphVSOut in [[stage_in]], texture2d<float> atlas [[texture(0)]], sampler s [[sampler(0)]])
{
    float distance = atlas.sample(s, in.uv).r;
    float alpha = smoothstep(0.38, 0.62, distance);
    return float4(in.color.rgb, in.color.a * alpha);
}

fragment float4 f_prepared_glyph_sdf(GlyphVSOut in [[stage_in]], texture2d<float> atlas [[texture(0)]], sampler s [[sampler(0)]], constant PreparedInstance& instance [[buffer(3)]])
{
    float distance = atlas.sample(s, in.uv).r;
    float alpha = smoothstep(0.38, 0.62, distance);
    return float4(in.color.rgb, in.color.a * alpha * instance.opacityAndPadding.x);
}

fragment float4 f_text(TextVSOut in [[stage_in]], texture2d<float> atlas [[texture(0)]], sampler s [[sampler(0)]], constant TextUniform& uni [[buffer(0)]]) {
    // Sample alpha from the atlas (assume single-channel in .a or .r; using .r here)
    float a = atlas.sample(s, in.uv).r;
    return float4(uni.color.rgb, uni.color.a * a);
}

fragment float4 f_prepared_text(TextVSOut in [[stage_in]], texture2d<float> atlas [[texture(0)]], sampler s [[sampler(0)]], constant TextUniform& uni [[buffer(0)]], constant PreparedInstance& instance [[buffer(3)]])
{
    float alpha = atlas.sample(s, in.uv).r;
    return float4(uni.color.rgb, uni.color.a * alpha * instance.opacityAndPadding.x);
}

fragment float4 f_image_mesh(TextVSOut in [[stage_in]], texture2d<float> img [[texture(0)]], sampler s [[sampler(0)]], constant TextUniform& uni [[buffer(0)]]) {
    float4 c = img.sample(s, in.uv);
    c.rgb *= uni.color.rgb;
    c.a *= uni.color.a;
    return c;
}

fragment float4 f_prepared_image_mesh(TextVSOut in [[stage_in]], texture2d<float> img [[texture(0)]], sampler s [[sampler(0)]], constant TextUniform& uni [[buffer(0)]], constant PreparedInstance& instance [[buffer(3)]])
{
    float4 c = img.sample(s, in.uv);
    c.rgb *= uni.color.rgb;
    c.a *= uni.color.a * instance.opacityAndPadding.x;
    return c;
}

// SDF variant: treat atlas.r as signed-distance remapped to [0,1] with 0.5 as edge
fragment float4 f_text_sdf(TextVSOut in [[stage_in]], texture2d<float> atlas [[texture(0)]], sampler s [[sampler(0)]], constant TextUniform& uni [[buffer(0)]])
{
    float sd = atlas.sample(s, in.uv).r; // in [0,1], 0.5 at edge
    // Fixed smoothing width; could be adapted using derivatives
    float w = 0.12;
    float alpha = smoothstep(0.5 - w, 0.5 + w, sd);
    return float4(uni.color.rgb, uni.color.a * alpha);
}

fragment float4 f_prepared_text_sdf(TextVSOut in [[stage_in]], texture2d<float> atlas [[texture(0)]], sampler s [[sampler(0)]], constant TextUniform& uni [[buffer(0)]], constant PreparedInstance& instance [[buffer(3)]])
{
    float sd = atlas.sample(s, in.uv).r;
    float alpha = smoothstep(0.38, 0.62, sd);
    return float4(uni.color.rgb, uni.color.a * alpha * instance.opacityAndPadding.x);
}
