#[repr(C)]
struct Scene3dMaterialCpuLayout
{
   color: [f32; 4],
   material: u32,
   _pad: [f32; 3],
   params: [f32; 4],
}

fn field_offset<T, F>(base: &T, field: &F) -> usize
{
   let base_ptr = base as *const T as usize;
   let field_ptr = field as *const F as usize;
   field_ptr - base_ptr
}

#[test]
fn scene3d_material_shader_matches_cpu_set_bytes_layout()
{
   let cpu = Scene3dMaterialCpuLayout {
      color: [0.0; 4],
      material: 0,
      _pad: [0.0; 3],
      params: [0.0; 4],
   };

   assert_eq!(core::mem::size_of::<Scene3dMaterialCpuLayout>(), 48);
   assert_eq!(field_offset(&cpu, &cpu.material), 16);
   assert_eq!(field_offset(&cpu, &cpu.params), 32);

   let shader = include_str!("../shaders/scene3d.metal");
   let shader_lines = shader.lines().map(str::trim).collect::<Vec<_>>();
   assert!(shader_lines.contains(&"packed_float3 _pad;"));
   assert!(!shader_lines.contains(&"float3 _pad;"));
}

#[test]
fn scene3d_bloom_payload_reaches_bloom_encoder()
{
   let renderer_source = include_str!("../src/lib.rs");

   assert!(renderer_source.contains(
      "self.encode_scene3d_bloom(&cmd, &target_tex, pass.view_proj, bloom)?;"
   ));
   assert!(!renderer_source.contains("bloom.layers.iter().map(|layer| layer.strength"));
}
