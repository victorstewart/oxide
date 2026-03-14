#include <metal_stdlib>
using namespace metal;

// Standalone instanced-rect vertex for camera (self-contained for build.rs per-file compile)
struct CamVSOut { float4 position [[position]]; float2 pos_dp; uint iid [[flat]]; };
struct CamVPSize { float2 size; };
vertex CamVSOut v_inst_rect_cam(uint vid [[vertex_id]], uint iid [[instance_id]],
                                const device float4* inst [[buffer(0)]],
                                constant CamVPSize& vp [[buffer(1)]])
{
    float2 offs[6] = {
        float2(0.0, 0.0), float2(1.0, 0.0), float2(0.0, 1.0),
        float2(0.0, 1.0), float2(1.0, 0.0), float2(1.0, 1.0)
    };
    float4 r = inst[iid]; // x,y,w,h in dp
    float2 dp = r.xy + offs[vid] * r.zw;
    float2 clip;
    clip.x = (dp.x / max(vp.size.x, 1e-5)) * 2.0 - 1.0;
    // App-space dp uses top-left origin with +Y downward.
    // Flip Y to match clip-space orientation.
    clip.y = 1.0 - (dp.y / max(vp.size.y, 1e-5)) * 2.0;
    CamVSOut o;
    o.position = float4(clip, 0.0, 1.0);
    o.pos_dp = dp;
    o.iid = iid;
    return o;
}

struct CamParams {
    float4 rect;      // x,y,w,h in dp (matches v_inst_rect_cam input space)
    float4 tint;      // r,g,b,a multiplier
    float2 uv_scale;  // scale to apply to normalized dest UV (aspect fill)
    float2 uv_bias;   // bias to center after scaling
    float  grayscale; // 1.0 -> use luma only, 0.0 -> full color
    float  matrix;    // 0=709,1=601,2=2020
    float  videoRange; // 0 full, 1 video
    float  bitDepth;   // 8 or 10 (others treated as 8)
    float  pad;
};

// NV12 (Y: R8Unorm, UV: RG8Unorm) to sRGB
inline float3 yuv_to_rgb_matrix(float y, float u, float v, int m)
{
    float r, g, b;
    if (m == 1) {
        // BT.601
        r = y + 1.402 * v;
        g = y - 0.344136 * u - 0.714136 * v;
        b = y + 1.772 * u;
    } else if (m == 2) {
        // BT.2020 (non-constant luminance approx)
        r = y + 1.4746 * v;
        g = y - 0.164553 * u - 0.571353 * v;
        b = y + 1.8814 * u;
    } else {
        // BT.709
        r = y + 1.5748 * v;
        g = y - 0.1873 * u - 0.4681 * v;
        b = y + 1.8556 * u;
    }
    return float3(r, g, b);
}

fragment float4 f_camera_nv12(
    CamVSOut in [[stage_in]],
    texture2d<float> yTex [[texture(0)]],
    texture2d<float> uvTex [[texture(1)]],
    sampler s [[sampler(0)]],
    constant CamParams* parr [[buffer(1)]])
{
    CamParams p = parr[in.iid];
    float2 xy = in.pos_dp;
    // Discard outside rect to limit work
    if (xy.x < p.rect.x || xy.y < p.rect.y || xy.x > (p.rect.x + p.rect.z) || xy.y > (p.rect.y + p.rect.w))
    {
        discard_fragment();
    }
    // Normalized coords within destination rect
    float2 d = (xy - p.rect.xy) / max(p.rect.zw, float2(1e-5));
    float2 uv_norm = d * p.uv_scale + p.uv_bias;
    // Sample NV12/P010 planes (normalized)
    float y_s = yTex.sample(s, uv_norm).r;
    float2 uv_s = uvTex.sample(s, uv_norm).rg;
    // Normalize for full/video range and center chroma
    float y, u, v;
    bool video = (p.videoRange > 0.5);
    bool is10 = (p.bitDepth > 9.5);
    if (video) {
        float yMin = is10 ? (64.0/1023.0) : (16.0/255.0);
        float yMax = is10 ? (940.0/1023.0) : (235.0/255.0);
        float cMax = is10 ? (960.0/1023.0) : (240.0/255.0);
        float cAmp = max(cMax - 0.5, 1e-5);
        y = clamp((y_s - yMin) / max(yMax - yMin, 1e-5), 0.0, 1.0);
        float2 uvz = uv_s - float2(0.5, 0.5);
        float scale = 0.5 / cAmp;
        u = uvz.x * scale;
        v = uvz.y * scale;
    } else {
        y = y_s;
        float2 uvz = uv_s - float2(0.5, 0.5);
        u = uvz.x * 2.0;
        v = uvz.y * 2.0;
    }
    int m = (int)floor(clamp(p.matrix + 0.5, 0.0, 2.0));
    float3 rgb = yuv_to_rgb_matrix(y, u, v, m);
    // Grayscale path: use luma only (plus tint)
    float3 base = mix(rgb, float3(y, y, y), clamp(p.grayscale, 0.0, 1.0));
    float3 mod = base * p.tint.rgb;
    return float4(mod, p.tint.a);
}
