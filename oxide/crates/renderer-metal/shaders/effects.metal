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

// params: (dir_x, dir_y, sigma, pad)
fragment float4 f_blur(VSOut in [[stage_in]], texture2d<float> src [[texture(0)]], sampler s [[sampler(0)]], constant float4& params [[buffer(1)]]) {
    float2 dir = params.xy;
    float sigma = max(params.z, 0.001);
    float2 inv_size = float2(1.0 / src.get_width(), 1.0 / src.get_height());
    float2 uv = in.uv;

    // 5-tap Gaussian weights based on sigma
    float w0 = 1.0 / (sqrt(2.0 * M_PI_F) * sigma);
    float e1 = exp(-0.5 * (1.0 / sigma) * (1.0 / sigma));
    float e2 = exp(-0.5 * (2.0 / sigma) * (2.0 / sigma));
    float w1 = w0 * e1;
    float w2 = w0 * e2;
    float norm = w0 + 2.0 * (w1 + w2);
    w0 /= norm; w1 /= norm; w2 /= norm;

    float2 step = dir * inv_size;
    float4 c = src.sample(s, uv) * w0;
    c += (src.sample(s, uv + step * 1.0) + src.sample(s, uv - step * 1.0)) * w1;
    c += (src.sample(s, uv + step * 2.0) + src.sample(s, uv - step * 2.0)) * w2;
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

// Dedicated backdrop rect pipeline I/O. Keep names unique to avoid cross-file collisions.
struct BackdropRectParams { float4 rect; };
struct BackdropVPSize { float2 size; };
struct BackdropVSOut {
    float4 position [[position]];
    float2 pos_dp;
    float2 uv;
    uint iid [[flat]];
};

vertex BackdropVSOut v_backdrop(uint vid [[vertex_id]], uint iid [[instance_id]],
                                const device BackdropRectParams* inst [[buffer(0)]],
                                constant BackdropVPSize& vp [[buffer(1)]])
{
    float2 offs[6] = {
        float2(0.0, 0.0), float2(1.0, 0.0), float2(0.0, 1.0),
        float2(0.0, 1.0), float2(1.0, 0.0), float2(1.0, 1.0)
    };
    BackdropRectParams p = inst[iid];
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

// rect: (x, y, w, h) in dp space, tint: (r, g, b, alpha)
struct BackdropParams { float4 rect; float4 tint; };

fragment float4 f_backdrop(BackdropVSOut in [[stage_in]],
                           texture2d<float> src [[texture(0)]], sampler s [[sampler(0)]],
                           constant BackdropParams* parr [[buffer(1)]])
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
