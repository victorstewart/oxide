#include <metal_stdlib>
using namespace metal;

// VSOut with instance id is defined here and reused by UI effects
struct VSOut {
    float4 position [[position]];
    float2 uv;
    uint iid [[flat]];
};

vertex VSOut v_fullscreen(uint vid [[vertex_id]]) {
    float2 pos[3] = { float2(-1.0, -1.0), float2(3.0, -1.0), float2(-1.0, 3.0) };
    float2 uv[3] = { float2(0.0, 0.0), float2(2.0, 0.0), float2(0.0, 2.0) };
    VSOut o;
    o.position = float4(pos[vid], 0.0, 1.0);
    o.uv = uv[vid];
    o.iid = 0;
    return o;
}

// params: (dir_x, dir_y, sigma, radius)
fragment float4 f_blur(VSOut in [[stage_in]], texture2d<float> src [[texture(0)]], sampler s [[sampler(0)]], constant float4& params [[buffer(1)]]) {
    float2 dir = params.xy;
    float sigma = max(params.z, 0.001);
    int radius = clamp(int(round(params.w)), 2, 192);
    float2 inv_size = float2(1.0 / src.get_width(), 1.0 / src.get_height());
    float2 uv = in.uv;
    float2 step = dir * inv_size;

    // Wider separable Gaussian. The effect prepass runs at quarter resolution,
    // so each radius texel covers roughly four framebuffer pixels. Backdrop
    // effects pass a wider radius than camera blur to match UIKit's material.
    float w0 = 1.0 / (sqrt(2.0 * M_PI_F) * sigma);
    float4 c = src.sample(s, uv) * w0;
    float norm = w0;
    for (int i = 1; i <= 192; ++i) {
        if (i > radius) {
            break;
        }
        float x = float(i);
        float w = w0 * exp(-0.5 * (x / sigma) * (x / sigma));
        c += (src.sample(s, uv + step * x) + src.sample(s, uv - step * x)) * w;
        norm += 2.0 * w;
    }
    return c / max(norm, 1e-6);
}

// Precomputed paired kernels bind one normalized (offset, weight) record per
// bilinear sample pair. A separate pipeline keeps the exact Gaussian loop and
// its runtime exponentials out of paired-kernel shader occupancy.
fragment float4 f_blur_paired(VSOut in [[stage_in]], texture2d<float> src [[texture(0)]], sampler s [[sampler(0)]], constant float* packed_kernel [[buffer(1)]]) {
    float2 inv_size = float2(1.0 / src.get_width(), 1.0 / src.get_height());
    float2 step = float2(packed_kernel[0], packed_kernel[1]) * inv_size;
    float4 c = src.sample(s, in.uv) * packed_kernel[4];
    uint pair_count = uint(packed_kernel[5]);
    for (uint pair = 0; pair < pair_count; ++pair) {
        uint base = 6 + pair * 2;
        float2 offset_weight = float2(packed_kernel[base], packed_kernel[base + 1]);
        float2 delta = step * offset_weight.x;
        c += (src.sample(s, in.uv + delta) + src.sample(s, in.uv - delta)) * offset_weight.y;
    }
    return c;
}

// Downsample by 2x: sample at center of 2x2 block (simple bilinear is sufficient)
fragment float4 f_downsample(VSOut in [[stage_in]], texture2d<float> src [[texture(0)]], sampler s [[sampler(0)]])
{
    // `in.uv` is already normalized across the destination render target.
    // Sampling source at the same normalized coordinate gives stable 2x minification
    // when rendering into half/quarter-sized targets.
    return src.sample(s, in.uv);
}

// Upsample by scale factor (e.g., 2.0)
fragment float4 f_upsample(VSOut in [[stage_in]], texture2d<float> src [[texture(0)]], sampler s [[sampler(0)]], constant float& scale [[buffer(1)]])
{
   // Scale is intentionally accepted for ABI stability; UV mapping is normalized.
   // Re-scaling UV here causes out-of-range sampling and severe artifacts.
   (void)scale;
   return src.sample(s, in.uv);
}

fragment float4 f_bloom_composite(VSOut in [[stage_in]], texture2d<float> src [[texture(0)]], sampler s [[sampler(0)]], constant float& strength [[buffer(1)]])
{
   float gain = max(strength, 0.0);
   float4 c = src.sample(s, in.uv);
   return float4(c.rgb * gain, c.a * gain);
}

// Dedicated backdrop rect pipeline I/O. Keep names unique to avoid cross-file collisions.
struct BackdropParams { float4 rect; float4 tint; };
struct BackdropVPSize { float2 size; };
struct BackdropVSOut {
    float4 position [[position]];
    float2 pos_dp;
    float2 uv;
    uint iid [[flat]];
};

vertex BackdropVSOut v_backdrop(uint vid [[vertex_id]], uint iid [[instance_id]],
                                const device BackdropParams* inst [[buffer(0)]],
                                constant BackdropVPSize& vp [[buffer(1)]])
{
    float2 offs[6] = {
        float2(0.0, 0.0), float2(1.0, 0.0), float2(0.0, 1.0),
        float2(0.0, 1.0), float2(1.0, 0.0), float2(1.0, 1.0)
    };
    BackdropParams p = inst[iid];
    float2 dp = p.rect.xy + offs[vid] * p.rect.zw;
    float2 clip;
    clip.x = (dp.x / max(vp.size.x, 1e-5)) * 2.0 - 1.0;
    // App-space dp uses top-left origin with +Y downward.
    // Flip Y to match clip-space orientation.
    clip.y = 1.0 - (dp.y / max(vp.size.y, 1e-5)) * 2.0;
    BackdropVSOut o;
    o.position = float4(clip, 0.0, 1.0);
    o.pos_dp = dp;
    o.uv = dp / max(vp.size, float2(1e-5, 1e-5));
    o.iid = iid;
    return o;
}

fragment float4 f_backdrop(BackdropVSOut in [[stage_in]],
                           texture2d<float> src [[texture(0)]], sampler s [[sampler(0)]],
                           const device BackdropParams* parr [[buffer(1)]])
{
    BackdropParams p = parr[in.iid];
    float2 xy_dp = in.pos_dp;
    if (xy_dp.x < p.rect.x || xy_dp.y < p.rect.y ||
        xy_dp.x > p.rect.x + p.rect.z || xy_dp.y > p.rect.y + p.rect.w)
    {
        discard_fragment();
    }
    // Use normalized dp-space UV to avoid clip-space orientation ambiguity.
    float4 c = src.sample(s, in.uv);
    c.rgb *= float3(p.tint.x, p.tint.y, p.tint.z);
    c.a *= p.tint.w;
    return c;
}

// rect: (x, y, w, h), tint: caller material tint color.
struct VisualEffectParams { float4 rect; float4 tint; };

static inline float3 dark_popup_blur_material(float3 blurred, float3 tint)
{
    float3 darkened_backdrop = blurred * 0.10;
    return mix(darkened_backdrop, tint, 0.73);
}

fragment float4 f_visual_effect(BackdropVSOut in [[stage_in]],
                                texture2d<float> src [[texture(0)]], sampler s [[sampler(0)]],
                                const device VisualEffectParams* parr [[buffer(1)]])
{
    VisualEffectParams p = parr[in.iid];
    float2 xy_dp = in.pos_dp;
    if (xy_dp.x < p.rect.x || xy_dp.y < p.rect.y ||
        xy_dp.x > p.rect.x + p.rect.z || xy_dp.y > p.rect.y + p.rect.w)
    {
        discard_fragment();
    }

    float4 c = src.sample(s, in.uv);
    float effect_alpha = clamp(p.tint.a, 0.0, 1.0);
    float3 material = dark_popup_blur_material(c.rgb, clamp(p.tint.rgb, 0.0, 1.0));
    return float4(mix(c.rgb, material, effect_alpha), c.a);
}
