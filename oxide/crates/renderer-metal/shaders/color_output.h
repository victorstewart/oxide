#pragma once

// Renderer API colors and sRGB texture samples are linear.  The normal
// metallib writes those values to an sRGB attachment, which performs the
// output conversion after linear blending.  The opt-in sRGB metallib instead
// writes encoded source RGB to an unorm attachment so source-over occurs in
// the same sRGB domain as the UIKit baseline.  Alpha is always linear.
inline float oxide_linear_to_srgb(float value)
{
    float magnitude = abs(value);
    float encoded = magnitude <= 0.0031308
        ? magnitude * 12.92
        : 1.055 * pow(magnitude, 1.0 / 2.4) - 0.055;
    return copysign(encoded, value);
}

// RGBA image storage is premultiplied in linear light before filtering.
inline float4 straight_image_sample(float4 color)
{
    if (color.a > 1e-6) color.rgb /= color.a;
    else color.rgb = float3(0.0);
    return color;
}

inline float4 source_to_output(float4 color)
{
#if OXIDE_SRGB_COMPOSITING
    color.rgb = float3(
        oxide_linear_to_srgb(color.r),
        oxide_linear_to_srgb(color.g),
        oxide_linear_to_srgb(color.b)
    );
#endif
    return color;
}

// Effects sample output-domain render targets, but material/tint arithmetic
// keeps its linear-input contract before returning to the selected output space.
inline float4 output_to_source(float4 color)
{
#if OXIDE_SRGB_COMPOSITING
   float3 magnitude = abs(color.rgb);
   float3 linear = select(pow((magnitude + 0.055) / 1.055, float3(2.4)),
      magnitude / 12.92, magnitude <= float3(0.04045));
   color.rgb = copysign(linear, color.rgb);
#endif
   return color;
}
