use oxide_renderer_api as api;

/// Column-major 4x4 matrix matching the Metal shader contract.
pub type Mat4 = [[f32; 4]; 4];

/// One position-only vertex for the current scene3d path.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex3d {
    pub position: [f32; 3],
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

/// CPU-side mesh upload payload copied into Metal buffers once.
#[derive(Clone, Copy, Debug)]
pub struct Mesh3dData<'a> {
    pub vertices: &'a [Vertex3d],
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
        }
    }
}

/// One 3D pass encoded into the current frame before 2D content.
#[derive(Clone, Copy, Debug)]
pub struct Pass3d<'a> {
    pub clear_color: Option<api::Color>,
    pub clear_depth: bool,
    pub view_proj: Mat4,
    pub instances: &'a [Instance3d],
}
