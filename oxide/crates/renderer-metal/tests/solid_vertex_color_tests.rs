fn compact(source: &str) -> String
{
   source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

#[test]
fn solid_shader_inherits_zero_and_interpolates_nonzero_vertex_color()
{
   let shader = compact(include_str!("../shaders/solid.metal"));

   assert!(shader.contains("constantSolidUniform&uni[[buffer(1)]]"));
   assert!(shader.contains("all(in.rgba==float4(0.0))?uni.color:in.rgba"));
   assert!(shader.contains("o.rgba="));
   assert!(shader.contains("returnin.rgba;"));
   assert!(!shader.contains("constantSolidUniform&uni[[buffer(0)]]"));
}
