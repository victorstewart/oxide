#include <metal_stdlib>
using namespace metal;

// VSOut with instance id is defined here and reused by UI effects
struct VSOut { float4 position [[position]]; uint iid; };

vertex VSOut v_fullscreen(uint vid [[vertex_id]]) {
    float2 pos[3] = { float2(-1.0, -1.0), float2(3.0, -1.0), float2(-1.0, 3.0) };
    VSOut o; o.position = float4(pos[vid], 0.0, 1.0); o.iid = 0; return o;
}

// params: (dir_x, dir_y, sigma, pad)
fragment float4 f_blur(VSOut in [[stage_in]], texture2d<float> src [[texture(0)]], sampler s [[sampler(0)]], constant float4& params [[buffer(1)]]) {
    float2 dir = params.xy;
    float sigma = max(params.z, 0.001);
    float2 inv_size = float2(1.0 / src.get_width(), 1.0 / src.get_height());
    float2 uv = in.position.xy * inv_size;

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
    float2 inv_src = float2(1.0 / src.get_width(), 1.0 / src.get_height());
    float2 center = (in.position.xy * 2.0 + float2(1.0, 1.0)) * inv_src;
    return src.sample(s, center);
}

// Upsample by scale factor (e.g., 2.0)
fragment float4 f_upsample(VSOut in [[stage_in]], texture2d<float> src [[texture(0)]], sampler s [[sampler(0)]], constant float& scale [[buffer(1)]])
{
    float2 inv_src = float2(1.0 / src.get_width(), 1.0 / src.get_height());
    float2 uv = (in.position.xy / max(scale, 0.0001)) * inv_src;
    return src.sample(s, uv);
}

// rect: (x, y, w, h), tint: (r, g, b, alpha)
struct BackdropParams { float4 rect; float4 tint; };

fragment float4 f_backdrop(VSOut in [[stage_in]],
                           texture2d<float> src [[texture(0)]], sampler s [[sampler(0)]],
                           constant BackdropParams* parr [[buffer(1)]])
{
    BackdropParams p = parr[in.iid];
    float2 xy = in.position.xy;
    // Discard outside the rect; rely on full-screen triangle otherwise.
    if (xy.x < p.rect.x || xy.y < p.rect.y || xy.x > p.rect.x + p.rect.z || xy.y > p.rect.y + p.rect.w)
    {
        discard_fragment();
    }
    float2 inv_size = float2(1.0 / src.get_width(), 1.0 / src.get_height());
    float2 uv = xy * inv_size;
    float4 c = src.sample(s, uv);
    c.rgb *= float3(p.tint.x, p.tint.y, p.tint.z);
    c.a *= p.tint.w;
    return c;
}
