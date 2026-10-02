use oxide_renderer_api::Color;

fn assert_close(actual: f32, expected: f32)
{
   assert!((actual - expected).abs() < 0.000_001, "expected {expected}, got {actual}");
}

#[test]
fn from_srgba_converts_standard_srgb_and_preserves_linear_alpha()
{
   let color = Color::from_srgba(0.04045, 0.5, 1.0, 1.25);

   assert_close(color.r, 0.04045 / 12.92);
   assert_close(color.g, 0.214_041_14);
   assert_close(color.b, 1.0);
   assert_eq!(color.a, 1.25);
}

#[test]
fn from_srgba_extends_finite_rgb_without_clamping()
{
   let color = Color::from_srgba(1.25, -0.5, -0.04045, -0.25);

   assert_close(color.r, ((1.25_f32 + 0.055) / 1.055).powf(2.4));
   assert_close(color.g, -0.214_041_14);
   assert_close(color.b, -(0.04045 / 12.92));
   assert_eq!(color.a, -0.25);
}

#[test]
fn from_srgba_preserves_non_finite_components()
{
   let color = Color::from_srgba(f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::NAN);

   assert!(color.r.is_nan());
   assert_eq!(color.g, f32::INFINITY);
   assert_eq!(color.b, f32::NEG_INFINITY);
   assert!(color.a.is_nan());
}

#[test]
fn from_srgba8_normalizes_srgb_and_linear_alpha()
{
   let color = Color::from_srgba8(0, 128, 255, 64);

   assert_eq!(color.r, 0.0);
   assert_close(color.g, 0.215_860_53);
   assert_eq!(color.b, 1.0);
   assert_close(color.a, 64.0 / 255.0);
}

#[test]
fn rgba_remains_linear_and_pack_rgba8_is_unchanged()
{
   let color = Color::rgba(0.25, 0.5, 0.75, 1.0);

   assert_eq!(color, Color { r: 0.25, g: 0.5, b: 0.75, a: 1.0 });
   assert_eq!(color.pack_rgba8(), 0xFFBF_8040);
}
