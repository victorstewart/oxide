#include <metal_stdlib>
using namespace metal;

// Dedicated UI vertex output with instance id to avoid cross-file collisions.
struct UIVSOut { float4 position [[position]]; float2 pos_px; float2 rect_origin [[flat]]; uint iid [[flat]]; };

struct VSRectParams { float4 rect; };
struct RRectParams { float4 rect; float4 radii; float4 color; };
struct NineSliceParams { float4 rect; float2 texSize; float4 sliceLTRB; float alpha; };
struct ImageParams { float4 rect; float4 srcRect; float2 texSize; float alpha; uint texIndex; };
struct UIVPSize { float2 size; };
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

inline float2 preparedVector(float2 local, constant PreparedInstance& instance)
{
    if (instance.opacityAndPadding.y != 0.0)
    {
        return local;
    }
    return float2(
        instance.matrix.x * local.x + instance.matrix.z * local.y,
        instance.matrix.y * local.x + instance.matrix.w * local.y
    );
}

inline UIVSOut instancedRectOutput(uint vid, uint iid, float4 rect, constant UIVPSize& vp)
{
    float2 offs[6] = {
        float2(0.0, 0.0), float2(1.0, 0.0), float2(0.0, 1.0),
        float2(0.0, 1.0), float2(1.0, 0.0), float2(1.0, 1.0)
    };
    float2 dp = rect.xy + offs[vid] * rect.zw;
    float2 clip;
    clip.x = (dp.x / max(vp.size.x, 1e-5)) * 2.0 - 1.0;
    // App-space dp uses top-left origin with +Y downward.
    // Metal clip-space for this pipeline expects +Y upward, so flip Y.
    clip.y = 1.0 - (dp.y / max(vp.size.y, 1e-5)) * 2.0;
    UIVSOut o;
    o.position = float4(clip, 0.0, 1.0);
    o.pos_px = dp;
    o.rect_origin = rect.xy;
    o.iid = iid;
    return o;
}

// Instanced rect vertex: per-instance rect in dp and global viewport dp size
vertex UIVSOut v_inst_rect(uint vid [[vertex_id]], uint iid [[instance_id]],
                         const device VSRectParams* inst [[buffer(0)]],
                         constant UIVPSize& vp [[buffer(1)]])
{
    return instancedRectOutput(vid, iid, inst[iid].rect, vp);
}

vertex UIVSOut v_inst_rrect(uint vid [[vertex_id]], uint iid [[instance_id]],
                           const device RRectParams* inst [[buffer(0)]],
                           constant UIVPSize& vp [[buffer(1)]])
{
    return instancedRectOutput(vid, iid, inst[iid].rect, vp);
}

vertex UIVSOut v_inst_nine_slice(uint vid [[vertex_id]], uint iid [[instance_id]],
                                const device NineSliceParams* inst [[buffer(0)]],
                                constant UIVPSize& vp [[buffer(1)]])
{
    return instancedRectOutput(vid, iid, inst[iid].rect, vp);
}

vertex UIVSOut v_prepared_inst_rect(uint vid [[vertex_id]], uint iid [[instance_id]],
                                    const device float4* params [[buffer(0)]],
                                    constant PreparedInstance& instance [[buffer(2)]])
{
    float2 offs[6] = {
        float2(0.0, 0.0), float2(1.0, 0.0), float2(0.0, 1.0),
        float2(0.0, 1.0), float2(1.0, 0.0), float2(1.0, 1.0)
    };
    float4 rect = params[iid * 3];
    float2 extent = offs[vid] * rect.zw;
    float2 local = rect.xy + extent;
    // Preserve the flat path's translation rounding order at raster edges:
    // transform the local origin first, then add the transformed extent.
    float2 rectOrigin = preparedPosition(rect.xy, instance);
    float2 dp = rectOrigin + preparedVector(extent, instance);
    float2 clip;
    clip.x = (dp.x / max(instance.viewport.x, 1e-5)) * 2.0 - 1.0;
    clip.y = 1.0 - (dp.y / max(instance.viewport.y, 1e-5)) * 2.0;
    UIVSOut o;
    o.position = float4(clip, 0.0, 1.0);
    o.pos_px = instance.opacityAndPadding.y != 0.0 ? dp : local;
    o.rect_origin = instance.opacityAndPadding.y != 0.0 ? rectOrigin : rect.xy;
    o.iid = iid;
    return o;
}

// (legacy v_rect_ui removed; v_inst_rect is used for both single and instanced draws)

vertex UIVSOut v_fullscreen_ui(uint vid [[vertex_id]]) {
    float2 pos[3] = { float2(-1.0, -1.0), float2(3.0, -1.0), float2(-1.0, 3.0) };
    UIVSOut o;
    o.position = float4(pos[vid], 0.0, 1.0);
    o.pos_px = float2(0.0, 0.0);
    o.rect_origin = float2(0.0, 0.0);
    o.iid = 0;
    return o;
}

fragment float4 f_rrect(UIVSOut in [[stage_in]], const device RRectParams* parr [[buffer(1)]]) {
    RRectParams p = parr[in.iid];
    float2 xy = in.pos_px - in.rect_origin;
    float2 sz = p.rect.zw;
    // Outside rect -> discard
    if (xy.x < 0.0 || xy.y < 0.0 || xy.x > sz.x || xy.y > sz.y) discard_fragment();

    // Pick corner radius by quadrant. radii = {tl, tr, br, bl}
    float2 center = sz * 0.5;
    bool right = xy.x >= center.x;
    bool bottom = xy.y >= center.y;
    float r = right ? (bottom ? p.radii.z : p.radii.y) : (bottom ? p.radii.w : p.radii.x);
    float rmax = 0.5 * min(sz.x, sz.y);
    r = clamp(r, 0.0, rmax);

    // Signed-distance for rounded rect with selected corner radius.
    float2 pxy = xy - center;
    float2 b = center - float2(r, r);
    float2 q = abs(pxy) - b;
    float dist = length(max(q, float2(0.0))) + min(max(q.x, q.y), 0.0) - r;

    // Use derivative-driven AA width so the edge ramp is about one physical
    // pixel regardless of viewport scale (dp -> px).
    float aa = max(fwidth(dist), 1e-4);
    float alpha = 1.0 - smoothstep(-aa, aa, dist);
    if (alpha <= 0.0) discard_fragment();
    return float4(p.color.rgb, p.color.a * alpha);
}

fragment float4 f_prepared_rrect(UIVSOut in [[stage_in]],
                                 const device RRectParams* parr [[buffer(1)]],
                                 constant PreparedInstance& instance [[buffer(3)]])
{
    RRectParams p = parr[in.iid];
    float2 xy = in.pos_px - in.rect_origin;
    float2 sz = p.rect.zw;
    if (xy.x < 0.0 || xy.y < 0.0 || xy.x > sz.x || xy.y > sz.y) discard_fragment();
    float2 center = sz * 0.5;
    bool right = xy.x >= center.x;
    bool bottom = xy.y >= center.y;
    float radius = right ? (bottom ? p.radii.z : p.radii.y) : (bottom ? p.radii.w : p.radii.x);
    radius = clamp(radius, 0.0, 0.5 * min(sz.x, sz.y));
    float2 q = abs(xy - center) - (center - float2(radius));
    float distance = length(max(q, float2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
    float aa = max(fwidth(distance), 1e-4);
    float alpha = 1.0 - smoothstep(-aa, aa, distance);
    if (alpha <= 0.0) discard_fragment();
    return float4(p.color.rgb, p.color.a * alpha * instance.opacityAndPadding.x);
}

float mapNine(float x, float L, float R, float Wt, float Ws)
{
    if (x < L) return x; // left cap
    if (x > Wt - R) return Ws - (Wt - x); // right cap
    float xc = (x - L) / max(Wt - L - R, 1e-5);
    return L + xc * (Ws - L - R);
}

fragment float4 f_nine_slice(UIVSOut in [[stage_in]],
                             texture2d<float> img [[texture(0)]], sampler s [[sampler(0)]],
                             const device NineSliceParams* parr [[buffer(1)]])
{
    NineSliceParams p = parr[in.iid];
    float2 xy = in.pos_px - in.rect_origin;
    if (xy.x < 0.0 || xy.y < 0.0 || xy.x > p.rect.z || xy.y > p.rect.w) discard_fragment();
    float u = mapNine(xy.x, p.sliceLTRB.x, p.sliceLTRB.z, p.rect.z, p.texSize.x);
    float v = mapNine(xy.y, p.sliceLTRB.y, p.sliceLTRB.w, p.rect.w, p.texSize.y);
    float2 uv = float2(u / p.texSize.x, v / p.texSize.y);
    float4 c = img.sample(s, uv);
    c.a *= p.alpha;
    return c;
}

fragment float4 f_layer_composite_aligned(UIVSOut in [[stage_in]],
                                          texture2d<float> img [[texture(0)]],
                                          constant NineSliceParams* parr [[buffer(1)]])
{
    constexpr sampler alignedSampler(coord::pixel, address::clamp_to_edge, filter::nearest);
    NineSliceParams p = parr[in.iid];
    float2 xy = in.pos_px - in.rect_origin;
    if (xy.x < 0.0 || xy.y < 0.0 || xy.x > p.rect.z || xy.y > p.rect.w) discard_fragment();
    float2 texel = xy * (p.texSize / max(p.rect.zw, float2(1e-5)));
    return img.sample(alignedSampler, texel);
}

struct SpinnerParams { float2 center; float radius; float thickness; float phase; float alpha; };

fragment float4 f_spinner(UIVSOut in [[stage_in]], const device SpinnerParams* sarr [[buffer(1)]])
{
    SpinnerParams sp = sarr[in.iid];
    float2 d = in.pos_px - sp.center;
    float r = length(d);
    float a = atan2(d.y, d.x);
    if (a < 0.0) a += 6.28318530718;
    float a0 = sp.phase;
    float a1 = sp.phase + 4.71238898; // 270 degrees
    // Normalize to [0, 2pi)
    bool inArc = (a0 <= a1) ? (a >= a0 && a <= a1) : (a >= a0 || a <= fmod(a1, 6.28318530718));
    float aa = max(fwidth(r), 1e-4);
    float ring = 1.0 - smoothstep(sp.radius - sp.thickness*0.5 - aa, sp.radius - sp.thickness*0.5 + aa, r)
                  - (1.0 - smoothstep(sp.radius + sp.thickness*0.5 - aa, sp.radius + sp.thickness*0.5 + aa, r));
    float alpha = (inArc ? 1.0 : 0.0) * clamp(ring, 0.0, 1.0) * sp.alpha;
    if (alpha <= 0.01) discard_fragment();
    return float4(236.0 / 255.0, 240.0 / 255.0, 241.0 / 255.0, alpha);
}

struct ImageArgs { array<texture2d<float>, 128> imgs [[id(0)]]; };

fragment float4 f_image(UIVSOut in [[stage_in]],
                        sampler s [[sampler(0)]],
                        const device ImageParams* parr [[buffer(1)]],
                        constant ImageArgs& A [[buffer(2)]])
{
    ImageParams p = parr[in.iid];
    float2 xy = in.pos_px - in.rect_origin;
    if (xy.x < 0.0 || xy.y < 0.0 || xy.x > p.rect.z || xy.y > p.rect.w) discard_fragment();
    float2 uv_px = float2(p.srcRect.x, p.srcRect.y) + xy * float2(p.srcRect.z / max(p.rect.z, 1e-5),
                                                                  p.srcRect.w / max(p.rect.w, 1e-5));
    float2 uv = float2(uv_px.x / p.texSize.x, uv_px.y / p.texSize.y);
    float4 c = A.imgs[p.texIndex].sample(s, uv);
    c.a *= p.alpha;
    return c;
}

fragment float4 f_prepared_image(UIVSOut in [[stage_in]],
                                 sampler s [[sampler(0)]],
                                 const device ImageParams* parr [[buffer(1)]],
                                 constant ImageArgs& A [[buffer(2)]],
                                 constant PreparedInstance& instance [[buffer(3)]])
{
    ImageParams p = parr[in.iid];
    float2 xy = in.pos_px - in.rect_origin;
    if (xy.x < 0.0 || xy.y < 0.0 || xy.x > p.rect.z || xy.y > p.rect.w) discard_fragment();
    float2 uv_px = float2(p.srcRect.x, p.srcRect.y) + xy * float2(p.srcRect.z / max(p.rect.z, 1e-5),
                                                                  p.srcRect.w / max(p.rect.w, 1e-5));
    float2 uv = float2(uv_px.x / p.texSize.x, uv_px.y / p.texSize.y);
    float4 color = A.imgs[p.texIndex].sample(s, uv);
    color.a *= p.alpha * instance.opacityAndPadding.x;
    return color;
}

fragment float4 f_image_single(UIVSOut in [[stage_in]],
                               texture2d<float> img [[texture(0)]],
                               sampler s [[sampler(0)]],
                               const device ImageParams* parr [[buffer(1)]])
{
    ImageParams p = parr[in.iid];
    float2 xy = in.pos_px - in.rect_origin;
    if (xy.x < 0.0 || xy.y < 0.0 || xy.x > p.rect.z || xy.y > p.rect.w) discard_fragment();
    float2 uv_px = float2(p.srcRect.x, p.srcRect.y) + xy * float2(p.srcRect.z / max(p.rect.z, 1e-5),
                                                                  p.srcRect.w / max(p.rect.w, 1e-5));
    float2 uv = float2(uv_px.x / p.texSize.x, uv_px.y / p.texSize.y);
    float4 c = img.sample(s, uv);
    c.a *= p.alpha;
    return c;
}

fragment float4 f_prepared_image_single(UIVSOut in [[stage_in]],
                                        texture2d<float> img [[texture(0)]],
                                        sampler s [[sampler(0)]],
                                        const device ImageParams* parr [[buffer(1)]],
                                        constant PreparedInstance& instance [[buffer(3)]])
{
    ImageParams p = parr[in.iid];
    float2 xy = in.pos_px - in.rect_origin;
    if (xy.x < 0.0 || xy.y < 0.0 || xy.x > p.rect.z || xy.y > p.rect.w) discard_fragment();
    float2 uv_px = float2(p.srcRect.x, p.srcRect.y) + xy * float2(p.srcRect.z / max(p.rect.z, 1e-5),
                                                                  p.srcRect.w / max(p.rect.w, 1e-5));
    float2 uv = float2(uv_px.x / p.texSize.x, uv_px.y / p.texSize.y);
    float4 color = img.sample(s, uv);
    color.a *= p.alpha * instance.opacityAndPadding.x;
    return color;
}

// BackdropParams is defined in effects.metal
