#[repr(C)]
struct Scene3dMaterialCpuLayout {
    color: [f32; 4],
    material: u32,
    _pad: [f32; 3],
    params: [f32; 4],
}

fn field_offset<T, F>(base: &T, field: &F) -> usize {
    let base_ptr = base as *const T as usize;
    let field_ptr = field as *const F as usize;
    field_ptr - base_ptr
}

#[test]
fn scene3d_material_shader_matches_cpu_set_bytes_layout() {
    let cpu =
        Scene3dMaterialCpuLayout { color: [0.0; 4], material: 0, _pad: [0.0; 3], params: [0.0; 4] };

    assert_eq!(core::mem::size_of::<Scene3dMaterialCpuLayout>(), 48);
    assert_eq!(field_offset(&cpu, &cpu.material), 16);
    assert_eq!(field_offset(&cpu, &cpu.params), 32);

    let shader = include_str!("../shaders/scene3d.metal");
    let shader_lines = shader.lines().map(str::trim).collect::<Vec<_>>();
    assert!(shader_lines.contains(&"packed_float3 _pad;"));
    assert!(!shader_lines.contains(&"float3 _pad;"));
}

#[test]
fn scene3d_bloom_payload_reaches_bloom_encoder() {
    let renderer_source = include_str!("../src/lib.rs");

    assert!(renderer_source
        .contains("self.encode_scene3d_bloom(&cmd, &target_tex, pass.view_proj, bloom)?;"));
    assert!(!renderer_source.contains("bloom.layers.iter().map(|layer| layer.strength"));
}

#[test]
fn scene3d_matrix_helpers_are_column_major_renderer_math() {
    let projection = oxide_renderer_metal::scene3d::ortho(-1.0, 1.0, -1.0, 1.0, -1.0, 1.0);
    let transform = oxide_renderer_metal::scene3d::translate(0.25, -0.5, 0.0);
    let mvp = oxide_renderer_metal::scene3d::mat4_mul(&projection, &transform);
    let projected = oxide_renderer_metal::scene3d::mat4_mul_vec4(mvp, [0.0, 0.0, 0.0, 1.0]);

    assert!((projected[0] - 0.25).abs() < 1.0e-6);
    assert!((projected[1] + 0.5).abs() < 1.0e-6);
    assert!((projected[3] - 1.0).abs() < 1.0e-6);
}

#[test]
fn id_mask_compositor_is_renderer_owned_shader_path() {
    let renderer_source = include_str!("../src/lib.rs");
    let id_mask_gpu_source = include_str!("../src/id_mask_gpu.rs");
    let shader_source = include_str!("../shaders/id_mask_compositor.metal");

    assert!(renderer_source.contains("pub mod id_mask_compositor;"));
    assert!(id_mask_gpu_source.contains("pub fn encode_id_mask_gpu_compositor"));
    assert!(!renderer_source.contains("pub fn encode_id_mask_compositor("));
    assert!(!renderer_source.contains("upload_r8_mask_texture"));
    assert!(renderer_source.contains("pso_id_mask_compositor"));
    assert!(shader_source.contains("texture2d<uint, access::read> city_tex"));
    assert!(shader_source.contains("nearest_seam_distance"));
    assert!(shader_source.contains("nearest_city"));
}
