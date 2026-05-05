use oxide_renderer_api as api;

/// Column-major 4x4 matrix matching the Metal shader contract.
pub type Mat4 = [[f32; 4]; 4];

/// One position-only vertex for the current scene3d path.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex3d {
    pub position: [f32; 3],
}

/// One position + color vertex for scene3d meshes that carry their own color field.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VertexColor3d {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

/// Stable renderer-owned handle for a GPU-resident 3D mesh.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshHandle3d(pub u32);

/// Indexed topology supported by the current scene3d encoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshTopology {
    Triangles,
    Lines,
}

/// Back-face policy for a single scene3d instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CullMode3d {
    None,
    Front,
    Back,
}

/// Blend policy for a single scene3d instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode3d {
    Alpha,
    Additive,
}

/// Fragment material used by the scene3d shader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material3d {
    Flat,
    NeighborhoodFill,
    Emissive,
}

/// CPU-side mesh upload payload copied into Metal buffers once.
#[derive(Clone, Copy, Debug)]
pub struct Mesh3dData<'a> {
    pub vertices: &'a [Vertex3d],
    pub indices: &'a [u32],
    pub topology: MeshTopology,
}

/// CPU-side colored mesh upload payload copied into Metal buffers once.
#[derive(Clone, Copy, Debug)]
pub struct MeshColor3dData<'a> {
    pub vertices: &'a [VertexColor3d],
    pub indices: &'a [u32],
    pub topology: MeshTopology,
}

/// One draw instance inside a `Pass3d`.
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

/// One separable blur/composite layer for scene3d emissive bloom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BloomLayer3d {
    pub sigma_px: f32,
    pub strength: f32,
}

/// Optional emissive layer rendered by scene3d into an offscreen bloom stack.
#[derive(Clone, Copy, Debug)]
pub struct Bloom3d<'a> {
    pub emissive_instances: &'a [Instance3d],
    pub layers: &'a [BloomLayer3d],
    pub downsample_divisor: u32,
}

/// One 3D pass encoded into the current frame before 2D content.
#[derive(Clone, Copy, Debug)]
pub struct Pass3d<'a> {
    pub clear_color: Option<api::Color>,
    pub clear_depth: bool,
    pub view_proj: Mat4,
    pub instances: &'a [Instance3d],
    pub bloom: Option<Bloom3d<'a>>,
}
