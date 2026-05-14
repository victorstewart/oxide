use oxide_renderer_api as api;

/// Column-major 4x4 matrix matching the existing Oxide scene3d renderer contract.
pub type Mat4 = [[f32; 4]; 4];

#[must_use]
pub fn identity_mat4() -> Mat4 {
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

#[must_use]
pub fn mat4_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0_f32; 4]; 4];
    let mut col = 0;
    while col < 4 {
        let mut row = 0;
        while row < 4 {
            out[col][row] = a[0][row] * b[col][0]
                + a[1][row] * b[col][1]
                + a[2][row] * b[col][2]
                + a[3][row] * b[col][3];
            row += 1;
        }
        col += 1;
    }
    out
}

#[must_use]
pub fn mat4_mul_vec4(m: Mat4, v: [f32; 4]) -> [f32; 4] {
    [
        m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2] + m[3][0] * v[3],
        m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2] + m[3][1] * v[3],
        m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2] + m[3][2] * v[3],
        m[0][3] * v[0] + m[1][3] * v[1] + m[2][3] * v[2] + m[3][3] * v[3],
    ]
}

#[must_use]
pub fn perspective(fovy_rad: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fovy_rad * 0.5).tan();
    let nf = 1.0 / (near - far);
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, (far + near) * nf, -1.0],
        [0.0, 0.0, (2.0 * far * near) * nf, 0.0],
    ]
}

#[must_use]
pub fn scale_xyz(scale: f32) -> Mat4 {
    [[scale, 0.0, 0.0, 0.0], [0.0, scale, 0.0, 0.0], [0.0, 0.0, scale, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

#[must_use]
pub fn rotation_x(angle_rad: f32) -> Mat4 {
    let (sin, cos) = angle_rad.sin_cos();
    [[1.0, 0.0, 0.0, 0.0], [0.0, cos, sin, 0.0], [0.0, -sin, cos, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

#[must_use]
pub fn rotation_y(angle_rad: f32) -> Mat4 {
    let (sin, cos) = angle_rad.sin_cos();
    [[cos, 0.0, -sin, 0.0], [0.0, 1.0, 0.0, 0.0], [sin, 0.0, cos, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

#[must_use]
pub fn clip_space_translate(x: f32, y: f32) -> Mat4 {
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [x, y, 0.0, 1.0]]
}

#[must_use]
pub fn look_at_f64(eye: [f64; 3], center: [f64; 3], up: [f64; 3]) -> Mat4 {
    fn normalize(v: [f64; 3]) -> [f64; 3] {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if len <= f64::EPSILON {
            [0.0, 0.0, 0.0]
        } else {
            [v[0] / len, v[1] / len, v[2] / len]
        }
    }
    fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    }
    fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }
    let f = normalize([center[0] - eye[0], center[1] - eye[1], center[2] - eye[2]]);
    let s = normalize(cross(f, up));
    let u = cross(s, f);
    [
        [s[0] as f32, u[0] as f32, -f[0] as f32, 0.0],
        [s[1] as f32, u[1] as f32, -f[1] as f32, 0.0],
        [s[2] as f32, u[2] as f32, -f[2] as f32, 0.0],
        [-dot(s, eye) as f32, -dot(u, eye) as f32, dot(f, eye) as f32, 1.0],
    ]
}

#[must_use]
pub fn project_point_to_screen(
    view_proj: Mat4,
    viewport: api::RectF,
    position: [f32; 3],
) -> Option<(f32, f32)> {
    let clip = mat4_mul_vec4(view_proj, [position[0], position[1], position[2], 1.0]);
    if !clip[3].is_finite() || clip[3].abs() <= 1.0e-6 {
        return None;
    }
    let ndc_x = clip[0] / clip[3];
    let ndc_y = clip[1] / clip[3];
    if !ndc_x.is_finite() || !ndc_y.is_finite() {
        return None;
    }
    Some((
        viewport.x + (ndc_x * 0.5 + 0.5) * viewport.w,
        viewport.y + (0.5 - ndc_y * 0.5) * viewport.h,
    ))
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VertexColor3d {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshHandle3d(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshTopology {
    Triangles,
    Lines,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CullMode3d {
    None,
    Front,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode3d {
    Alpha,
    Additive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material3d {
    Flat,
    NeighborhoodFill,
    Emissive,
}

#[derive(Clone, Copy, Debug)]
pub struct MeshColor3dData<'a> {
    pub vertices: &'a [VertexColor3d],
    pub indices: &'a [u32],
    pub topology: MeshTopology,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance3d {
    pub mesh: MeshHandle3d,
    pub transform: Mat4,
    pub color: api::Color,
    pub cull: CullMode3d,
    pub depth_test: bool,
    pub depth_write: bool,
    pub color_write: bool,
    pub blend: BlendMode3d,
    pub material: Material3d,
    pub params: [f32; 4],
}

impl Instance3d {
    #[must_use]
    pub fn new(mesh: MeshHandle3d, transform: Mat4, color: api::Color) -> Self {
        Self {
            mesh,
            transform,
            color,
            cull: CullMode3d::Back,
            depth_test: true,
            depth_write: true,
            color_write: true,
            blend: BlendMode3d::Alpha,
            material: Material3d::Flat,
            params: [0.0; 4],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Pass3d<'a> {
    pub clear_color: Option<api::Color>,
    pub clear_depth: bool,
    pub view_proj: Mat4,
    pub instances: &'a [Instance3d],
    pub bloom: Option<()>,
}
