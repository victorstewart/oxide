struct Viewport {
   size_origin: vec4<f32>,
   matrix: vec4<f32>,
   translation_opacity: vec4<f32>,
};

struct Effect {
   texel_radius: vec4<f32>,
};

@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(1) @binding(0) var source_tex: texture_2d<f32>;
@group(1) @binding(1) var source_sampler: sampler;
@group(2) @binding(0) var<uniform> effect: Effect;

struct VertexIn {
   @location(0) pos: vec2<f32>,
   @location(1) uv: vec2<f32>,
   @location(2) color: vec4<f32>,
};

struct VertexOut {
   @builtin(position) pos: vec4<f32>,
   @location(0) uv: vec2<f32>,
   @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
   let size = max(viewport.size_origin.xy, vec2<f32>(1.0, 1.0));
   let origin = viewport.size_origin.zw;
   let transformed = vec2<f32>(
      viewport.matrix.x * input.pos.x + viewport.matrix.z * input.pos.y + viewport.translation_opacity.x,
      viewport.matrix.y * input.pos.x + viewport.matrix.w * input.pos.y + viewport.translation_opacity.y,
   );
   let local = (transformed - origin) / size;
   var out: VertexOut;
   out.pos = vec4<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0, 0.0, 1.0);
   out.uv = input.uv;
   out.color = vec4<f32>(input.color.rgb, input.color.a * viewport.translation_opacity.z);
   return out;
}
struct GlyphInstanceIn {
   @location(0) rect: vec4<f32>,
   @location(1) uv_rect: vec4<f32>,
   @location(2) color: vec4<f32>,
};

@vertex
fn vs_glyph_instance(
   @builtin(vertex_index) vertex_index: u32,
   input: GlyphInstanceIn,
) -> VertexOut {
   let unit = array<vec2<f32>, 4>(
      vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0),
      vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 1.0),
   )[vertex_index];
   let dp = input.rect.xy + unit * input.rect.zw;
   let transformed = vec2<f32>(
      viewport.matrix.x * dp.x + viewport.matrix.z * dp.y + viewport.translation_opacity.x,
      viewport.matrix.y * dp.x + viewport.matrix.w * dp.y + viewport.translation_opacity.y,
   );
   let size = max(viewport.size_origin.xy, vec2<f32>(1.0, 1.0));
   let local = (transformed - viewport.size_origin.zw) / size;
   var out: VertexOut;
   out.pos = vec4<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0, 0.0, 1.0);
   out.uv = mix(input.uv_rect.xy, input.uv_rect.zw, unit);
   out.color = vec4<f32>(input.color.rgb, input.color.a * viewport.translation_opacity.z);
   return out;
}

struct ImageInstanceIn {
   @location(0) rect: vec4<f32>,
   @location(1) uv_rect: vec4<f32>,
   @location(2) alpha: f32,
   @location(3) unit: vec2<f32>,
};

@vertex
fn vs_image_instance(input: ImageInstanceIn) -> VertexOut {
   let dp = input.rect.xy + input.unit * input.rect.zw;
   let transformed = vec2<f32>(
      viewport.matrix.x * dp.x + viewport.matrix.z * dp.y + viewport.translation_opacity.x,
      viewport.matrix.y * dp.x + viewport.matrix.w * dp.y + viewport.translation_opacity.y,
   );
   let size = max(viewport.size_origin.xy, vec2<f32>(1.0, 1.0));
   let local = (transformed - viewport.size_origin.zw) / size;
   var out: VertexOut;
   out.pos = vec4<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0, 0.0, 1.0);
   out.uv = mix(input.uv_rect.xy, input.uv_rect.zw, input.unit);
   out.color = vec4<f32>(1.0, 1.0, 1.0, input.alpha * viewport.translation_opacity.z);
   return out;
}

struct NineSliceInstanceIn {
   @location(0) rect: vec4<f32>,
   @location(1) image_size: vec2<f32>,
   @location(2) slice: vec4<f32>,
   @location(3) alpha: f32,
   @location(4) grid: vec4<u32>,
};

@vertex
fn vs_nine_slice_instance(input: NineSliceInstanceIn) -> VertexOut {
   let x2 = max(input.rect.z - input.slice.z, input.slice.x);
   let y2 = max(input.rect.w - input.slice.w, input.slice.y);
   let dx = array<f32, 4>(0.0, input.slice.x, x2, max(input.rect.z, x2));
   let dy = array<f32, 4>(0.0, input.slice.y, y2, max(input.rect.w, y2));
   let sx = array<f32, 4>(0.0, input.slice.x, input.image_size.x - input.slice.z, input.image_size.x);
   let sy = array<f32, 4>(0.0, input.slice.y, input.image_size.y - input.slice.w, input.image_size.y);
   let corner = vec2<f32>(f32(input.grid.z), f32(input.grid.w));
   let col = input.grid.x;
   let row = input.grid.y;
   let dp = input.rect.xy + vec2<f32>(
      mix(dx[col], dx[col + 1u], corner.x),
      mix(dy[row], dy[row + 1u], corner.y),
   );
   let source_valid = sx[col + 1u] > sx[col] && sy[row + 1u] > sy[row];
   let source_px = vec2<f32>(
      mix(sx[col], sx[col + 1u], corner.x),
      mix(sy[row], sy[row + 1u], corner.y),
   );
   let transformed = vec2<f32>(
      viewport.matrix.x * dp.x + viewport.matrix.z * dp.y + viewport.translation_opacity.x,
      viewport.matrix.y * dp.x + viewport.matrix.w * dp.y + viewport.translation_opacity.y,
   );
   let size = max(viewport.size_origin.xy, vec2<f32>(1.0, 1.0));
   let local = (transformed - viewport.size_origin.zw) / size;
   var out: VertexOut;
   out.pos = vec4<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0, 0.0, 1.0);
   out.uv = select(corner, source_px / input.image_size, source_valid);
   out.color = vec4<f32>(1.0, 1.0, 1.0, input.alpha * viewport.translation_opacity.z);
   return out;
}

struct RRectIn {
   @location(0) rect: vec4<f32>,
   @location(1) radii: vec4<f32>,
   @location(2) color: vec4<f32>,
};

struct RRectOut {
   @builtin(position) pos: vec4<f32>,
   @location(0) local_px: vec2<f32>,
   @location(1) @interpolate(flat) rect_size: vec2<f32>,
   @location(2) @interpolate(flat) radii: vec4<f32>,
   @location(3) @interpolate(flat) color: vec4<f32>,
};

struct SpinnerInstanceIn {
   @location(0) center: vec2<f32>,
   @location(1) atom: f32,
   @location(2) alpha: f32,
   @location(3) color: vec4<f32>,
};

@vertex
fn vs_spinner_instance(
   @builtin(vertex_index) vertex_index: u32,
   input: SpinnerInstanceIn,
) -> RRectOut {
   let corners = array<vec2<f32>, 6>(
      vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
      vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
   );
   let directions = array<vec2<f32>, 12>(
      vec2<f32>(1.0, 0.0), vec2<f32>(0.8660253882, 0.5),
      vec2<f32>(0.5, 0.8660253882), vec2<f32>(0.0, 1.0),
      vec2<f32>(-0.5, 0.8660253882), vec2<f32>(-0.8660253882, 0.5),
      vec2<f32>(-1.0, 0.0), vec2<f32>(-0.8660253882, -0.5),
      vec2<f32>(-0.5, -0.8660253882), vec2<f32>(0.0, -1.0),
      vec2<f32>(0.5, -0.8660253882), vec2<f32>(0.8660253882, -0.5),
   );
   let atom_index = vertex_index / 6u;
   let unit = corners[vertex_index % 6u];
   let dot_radius = input.atom * 0.12;
   let rect_size = vec2<f32>(dot_radius * 2.0);
   let dot_center = input.center + directions[atom_index] * max(input.atom * 1.5, 1.0);
   let local_px = unit * rect_size;
   let dp = dot_center - vec2<f32>(dot_radius) + local_px;
   let transformed = vec2<f32>(
      viewport.matrix.x * dp.x + viewport.matrix.z * dp.y + viewport.translation_opacity.x,
      viewport.matrix.y * dp.x + viewport.matrix.w * dp.y + viewport.translation_opacity.y,
   );
   let size = max(viewport.size_origin.xy, vec2<f32>(1.0, 1.0));
   let local = (transformed - viewport.size_origin.zw) / size;
   let progress = fract(f32(atom_index) / 12.0 + viewport.translation_opacity.w);
   let dot_alpha = round(clamp(input.alpha, 0.0, 1.0)
      * (0.25 + progress * 0.75) * 255.0) / 255.0;
   var out: RRectOut;
   out.pos = vec4<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0, 0.0, 1.0);
   out.local_px = local_px;
   out.rect_size = rect_size;
   out.radii = vec4<f32>(dot_radius);
   out.color = vec4<f32>(input.color.rgb, dot_alpha * viewport.translation_opacity.z);
   return out;
}

struct NeonMarkerIn {
   @location(0) center: vec2<f32>,
   @location(1) shape: vec4<f32>,
   @location(2) alpha: vec3<f32>,
   @location(3) core_color: vec4<f32>,
   @location(4) ring_color: vec4<f32>,
   @location(5) marker_viewport: vec4<f32>,
};

struct NeonMarkerOut {
   @builtin(position) pos: vec4<f32>,
   @location(0) pos_dp: vec2<f32>,
   @location(1) @interpolate(flat) center: vec2<f32>,
   @location(2) @interpolate(flat) shape: vec4<f32>,
   @location(3) @interpolate(flat) alpha: vec3<f32>,
   @location(4) @interpolate(flat) core_color: vec4<f32>,
   @location(5) @interpolate(flat) ring_color: vec4<f32>,
};

@vertex
fn vs_neon_marker_instance(
   @builtin(vertex_index) vertex_index: u32,
   input: NeonMarkerIn,
) -> NeonMarkerOut {
   let corners = array<vec2<f32>, 6>(
      vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
      vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
   );
   let radius = max(max(input.shape.w, input.shape.y + input.shape.z), input.shape.x);
   let dp = input.center + corners[vertex_index] * radius;
   let viewport_size = max(input.marker_viewport.zw, vec2<f32>(0.00001));
   let local = (dp - input.marker_viewport.xy) / viewport_size;
   var out: NeonMarkerOut;
   out.pos = vec4<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0, 0.0, 1.0);
   out.pos_dp = dp;
   out.center = input.center;
   out.shape = input.shape;
   out.alpha = input.alpha;
   out.core_color = input.core_color;
   out.ring_color = input.ring_color;
   return out;
}

@fragment
fn fs_neon_marker(input: NeonMarkerOut) -> @location(0) vec4<f32> {
   let distance = length(input.pos_dp - input.center);
   if (distance > input.shape.w) {
      return vec4<f32>(0.0);
   }
   if (distance <= input.shape.x) {
      let edge = clamp(distance / max(input.shape.x, 0.001), 0.0, 1.0);
      let core_alpha = input.core_color.a * (1.0 - edge * 0.08);
      return vec4<f32>(input.core_color.rgb, core_alpha);
   }
   let ring_width = max(input.shape.z, 0.001);
   let ring_alpha = clamp(1.0 - abs(distance - input.shape.y) / ring_width, 0.0, 1.0)
      * input.alpha.z;
   let sigma = max(input.alpha.x, 0.001);
   let halo_alpha = exp(-(distance * distance) / (2.0 * sigma * sigma)) * input.alpha.y;
   let marker_alpha = max(ring_alpha, halo_alpha) * input.ring_color.a;
   if (marker_alpha <= 0.001) {
      return vec4<f32>(0.0);
   }
   return vec4<f32>(input.ring_color.rgb, clamp(marker_alpha, 0.0, 1.0));
}

@vertex
fn vs_rrect(@builtin(vertex_index) vertex_index: u32, input: RRectIn) -> RRectOut {
   let unit = array<vec2<f32>, 6>(
      vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
      vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
   )[vertex_index];
   let local_px = unit * input.rect.zw;
   let dp = input.rect.xy + local_px;
   let transformed = vec2<f32>(
      viewport.matrix.x * dp.x + viewport.matrix.z * dp.y + viewport.translation_opacity.x,
      viewport.matrix.y * dp.x + viewport.matrix.w * dp.y + viewport.translation_opacity.y,
   );
   let size = max(viewport.size_origin.xy, vec2<f32>(1.0, 1.0));
   let local = (transformed - viewport.size_origin.zw) / size;
   var out: RRectOut;
   out.pos = vec4<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0, 0.0, 1.0);
   out.local_px = local_px;
   out.rect_size = input.rect.zw;
   out.radii = input.radii;
   out.color = vec4<f32>(input.color.rgb, input.color.a * viewport.translation_opacity.z);
   return out;
}

@fragment
fn fs_rrect(input: RRectOut) -> @location(0) vec4<f32> {
   let center = input.rect_size * 0.5;
   let right = input.local_px.x >= center.x;
   let bottom = input.local_px.y >= center.y;
   let top_radius = select(input.radii.x, input.radii.y, right);
   let bottom_radius = select(input.radii.w, input.radii.z, right);
   let radius = clamp(select(top_radius, bottom_radius, bottom), 0.0, min(center.x, center.y));
   let q = abs(input.local_px - center) - (center - vec2<f32>(radius));
   let distance = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
   let aa = max(fwidth(distance), 0.0001);
   let coverage = 1.0 - smoothstep(-aa, aa, distance);
   if coverage <= 0.0 {
      discard;
   }
   return vec4<f32>(input.color.rgb, input.color.a * coverage);
}

@fragment
fn fs_solid(input: VertexOut) -> @location(0) vec4<f32> {
   return input.color;
}

@fragment
fn fs_rgba(input: VertexOut) -> @location(0) vec4<f32> {
   return textureSample(source_tex, source_sampler, input.uv) * input.color;
}

@fragment
fn fs_a8(input: VertexOut) -> @location(0) vec4<f32> {
   let coverage = textureSample(source_tex, source_sampler, input.uv).r;
   return vec4<f32>(input.color.rgb, input.color.a * coverage);
}

@fragment
fn fs_sdf(input: VertexOut) -> @location(0) vec4<f32> {
   let distance = textureSample(source_tex, source_sampler, input.uv).r;
   let width = max(fwidth(distance), 0.001);
   let coverage = smoothstep(0.5 - width, 0.5 + width, distance);
   return vec4<f32>(input.color.rgb, input.color.a * coverage);
}

@fragment
fn fs_backdrop(input: VertexOut) -> @location(0) vec4<f32> {
   let texel = effect.texel_radius.xy;
   let radius = max(effect.texel_radius.z, 0.0);
   let step = texel * max(radius * 0.35, 1.0);
   var color = textureSample(source_tex, source_sampler, input.uv) * 0.227027;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>( step.x, 0.0)) * 0.1945946;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>(-step.x, 0.0)) * 0.1945946;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>(0.0,  step.y)) * 0.1216216;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>(0.0, -step.y)) * 0.1216216;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>( step.x,  step.y)) * 0.035135;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>(-step.x,  step.y)) * 0.035135;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>( step.x, -step.y)) * 0.035135;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>(-step.x, -step.y)) * 0.035135;
   let tint = input.color;
   return vec4<f32>(mix(color.rgb, tint.rgb, tint.a), max(color.a, tint.a));
}
