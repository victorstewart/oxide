#include <metal_stdlib>
using namespace metal;

// Dedicated UI vertex output with instance id to avoid cross-file collisions
struct UIVSOut { float4 position [[position]]; uint iid; };

struct VSRectParams { float4 rect; };
struct UIVPSize { float2 size; };

// Instanced rect vertex: per-instance rect in dp and global viewport dp size
vertex UIVSOut v_inst_rect(uint vid [[vertex_id]], uint iid [[instance_id]],
                         const device VSRectParams* inst [[buffer(0)]],
                         constant UIVPSize& vp [[buffer(1)]])
{
    float2 offs[6] = {
        float2(0.0, 0.0), float2(1.0, 0.0), float2(0.0, 1.0),
        float2(0.0, 1.0), float2(1.0, 0.0), float2(1.0, 1.0)
    };
    VSRectParams p = inst[iid];
    float2 dp = p.rect.xy + offs[vid] * p.rect.zw;
    float2 clip;
    clip.x = (dp.x / max(vp.size.x, 1e-5)) * 2.0 - 1.0;
    clip.y = (dp.y / max(vp.size.y, 1e-5)) * 2.0 - 1.0;
    UIVSOut o; o.position = float4(clip, 0.0, 1.0); o.iid = iid; return o;
}

// (legacy v_rect_ui removed; v_inst_rect is used for both single and instanced draws)

vertex UIVSOut v_fullscreen_ui(uint vid [[vertex_id]]) {
    float2 pos[3] = { float2(-1.0, -1.0), float2(3.0, -1.0), float2(-1.0, 3.0) };
    UIVSOut o; o.position = float4(pos[vid], 0.0, 1.0); o.iid = 0; return o;
}

struct RRectParams { float4 rect; float4 radii; float4 color; };

fragment float4 f_rrect(UIVSOut in [[stage_in]], constant RRectParams* parr [[buffer(1)]]) {
    RRectParams p = parr[in.iid];
    float2 xy = in.position.xy - p.rect.xy;
    float2 sz = p.rect.zw;
    // Outside rect -> discard
    if (xy.x < 0.0 || xy.y < 0.0 || xy.x > sz.x || xy.y > sz.y) discard_fragment();

    // Distances to each corner radius
    float2 tl = float2(p.radii.x, p.radii.x);
    float2 tr = float2(p.radii.y, p.radii.y);
    float2 br = float2(p.radii.z, p.radii.z);
    float2 bl = float2(p.radii.w, p.radii.w);

    float alpha = 1.0;
    if (xy.x < tl.x && xy.y < tl.y) {
        float2 d = xy - tl; alpha = clamp(1.0 - length(d) / max(tl.x, 1e-5), 0.0, 1.0);
    } else if (xy.x > sz.x - tr.x && xy.y < tr.y) {
        float2 d = (xy - float2(sz.x - tr.x, tr.y)); alpha = clamp(1.0 - length(d) / max(tr.x, 1e-5), 0.0, 1.0);
    } else if (xy.x > sz.x - br.x && xy.y > sz.y - br.y) {
        float2 d = (xy - (sz - br)); alpha = clamp(1.0 - length(d) / max(br.x, 1e-5), 0.0, 1.0);
    } else if (xy.x < bl.x && xy.y > sz.y - bl.y) {
        float2 d = (xy - float2(bl.x, sz.y - bl.y)); alpha = clamp(1.0 - length(d) / max(bl.x, 1e-5), 0.0, 1.0);
    }
    return float4(p.color.rgb, p.color.a * alpha);
}

struct NineSliceParams { float4 rect; float2 texSize; float4 sliceLTRB; float alpha; };

float mapNine(float x, float L, float R, float Wt, float Ws)
{
    if (x < L) return x; // left cap
    if (x > Wt - R) return Ws - (Wt - x); // right cap
    float xc = (x - L) / max(Wt - L - R, 1e-5);
    return L + xc * (Ws - L - R);
}

fragment float4 f_nine_slice(UIVSOut in [[stage_in]],
                             texture2d<float> img [[texture(0)]], sampler s [[sampler(0)]],
                             constant NineSliceParams* parr [[buffer(1)]])
{
    NineSliceParams p = parr[in.iid];
    float2 xy = in.position.xy - p.rect.xy;
    if (xy.x < 0.0 || xy.y < 0.0 || xy.x > p.rect.z || xy.y > p.rect.w) discard_fragment();
    float u = mapNine(xy.x, p.sliceLTRB.x, p.sliceLTRB.z, p.rect.z, p.texSize.x);
    float v = mapNine(xy.y, p.sliceLTRB.y, p.sliceLTRB.w, p.rect.w, p.texSize.y);
    float2 uv = float2(u / p.texSize.x, v / p.texSize.y);
    float4 c = img.sample(s, uv);
    c.a *= p.alpha;
    return c;
}

struct SpinnerParams { float2 center; float radius; float thickness; float phase; float alpha; };

fragment float4 f_spinner(UIVSOut in [[stage_in]], constant SpinnerParams* sarr [[buffer(1)]])
{
    SpinnerParams sp = sarr[in.iid];
    float2 d = in.position.xy - sp.center;
    float r = length(d);
    float a = atan2(d.y, d.x);
    if (a < 0.0) a += 6.28318530718;
    float a0 = sp.phase;
    float a1 = sp.phase + 4.71238898; // 270 degrees
    // Normalize to [0, 2pi)
    bool inArc = (a0 <= a1) ? (a >= a0 && a <= a1) : (a >= a0 || a <= fmod(a1, 6.28318530718));
    float ring = 1.0 - smoothstep(sp.radius - sp.thickness*0.5 - 1.0, sp.radius - sp.thickness*0.5, r)
                  - (1.0 - smoothstep(sp.radius + sp.thickness*0.5, sp.radius + sp.thickness*0.5 + 1.0, r));
    float alpha = (inArc ? 1.0 : 0.0) * clamp(ring, 0.0, 1.0) * sp.alpha;
    if (alpha <= 0.01) discard_fragment();
    return float4(0.0, 0.0, 0.0, alpha);
}

struct ImageParams { float4 rect; float4 srcRect; float2 texSize; float alpha; uint texIndex; };
struct ImageArgs { array<texture2d<float>, 128> imgs [[id(0)]]; };

fragment float4 f_image(UIVSOut in [[stage_in]],
                        sampler s [[sampler(0)]],
                        constant ImageParams* parr [[buffer(1)]],
                        constant ImageArgs& A [[buffer(2)]])
{
    ImageParams p = parr[in.iid];
    float2 xy = in.position.xy - p.rect.xy;
    if (xy.x < 0.0 || xy.y < 0.0 || xy.x > p.rect.z || xy.y > p.rect.w) discard_fragment();
    float2 uv_px = float2(p.srcRect.x, p.srcRect.y) + xy * float2(p.srcRect.z / max(p.rect.z, 1e-5),
                                                                  p.srcRect.w / max(p.rect.w, 1e-5));
    float2 uv = float2(uv_px.x / p.texSize.x, uv_px.y / p.texSize.y);
    float4 c = A.imgs[p.texIndex].sample(s, uv);
    c.a *= p.alpha;
    return c;
}

// BackdropParams is defined in effects.metal
