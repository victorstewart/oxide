use oxide_renderer_api as api;

pub const ID_MASK_MAX_CITY_STYLES: usize = 4;
pub const ID_MASK_MAX_NEIGHBORHOOD_COLORS: usize = 32;

pub const SEMANTIC_MASK_MAX_REGION_STYLES: usize = ID_MASK_MAX_CITY_STYLES;
pub const SEMANTIC_MASK_MAX_SUBREGION_COLORS: usize = ID_MASK_MAX_NEIGHBORHOOD_COLORS;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdMaskCompositorMode {
    Beauty = 0,
    SeamMask = 1,
    CityIdMask = 2,
    NeighborhoodIdMask = 3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IdMaskCityStyle {
    pub fill_rgb: [f32; 3],
    pub edge_rgb: [f32; 3],
    pub seam_rgb: [f32; 3],
}

pub type SemanticMaskRegionStyle = IdMaskCityStyle;

impl Default for IdMaskCityStyle {
    fn default() -> Self {
        Self { fill_rgb: [1.0, 1.0, 1.0], edge_rgb: [1.0, 1.0, 1.0], seam_rgb: [1.0, 1.0, 1.0] }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IdMaskRasterVertex {
    pub position_px: [f32; 2],
    pub position_world: [f32; 4],
    pub city_id: u32,
    pub neighborhood_id: u32,
}

impl IdMaskRasterVertex {
    #[must_use]
    pub fn new(position_px: [f32; 2], city_id: u8, neighborhood_id: u8) -> Self {
        Self {
            position_px,
            position_world: [0.0, 0.0, 0.0, 1.0],
            city_id: city_id as u32,
            neighborhood_id: neighborhood_id as u32,
        }
    }

    #[must_use]
    pub fn new_world(position_world: [f32; 3], city_id: u8, neighborhood_id: u8) -> Self {
        Self {
            position_px: [0.0, 0.0],
            position_world: [position_world[0], position_world[1], position_world[2], 1.0],
            city_id: city_id as u32,
            neighborhood_id: neighborhood_id as u32,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IdMaskRasterProjection {
    pub world_to_clip: [[f32; 4]; 4],
    pub model_to_world: [[f32; 4]; 4],
    pub camera_eye_unit: [f32; 3],
    pub normal_scale: [f32; 3],
    pub visible_front_min: f32,
    pub use_world_position: bool,
    pub visible_hemisphere: bool,
}

impl IdMaskRasterProjection {
    #[must_use]
    pub fn screen_px() -> Self {
        Self {
            world_to_clip: identity(),
            model_to_world: identity(),
            camera_eye_unit: [0.0, 0.0, 1.0],
            normal_scale: [1.0, 1.0, 1.0],
            visible_front_min: -1.0,
            use_world_position: false,
            visible_hemisphere: false,
        }
    }

    #[must_use]
    pub fn world_3d(world_to_clip: [[f32; 4]; 4]) -> Self {
        Self {
            world_to_clip,
            model_to_world: identity(),
            camera_eye_unit: [0.0, 0.0, 1.0],
            normal_scale: [1.0, 1.0, 1.0],
            visible_front_min: -1.0,
            use_world_position: true,
            visible_hemisphere: false,
        }
    }

    #[must_use]
    pub fn world_3d_visible_hemisphere(
        world_to_clip: [[f32; 4]; 4],
        model_to_world: [[f32; 4]; 4],
        camera_eye_unit: [f32; 3],
        normal_scale: [f32; 3],
        visible_front_min: f32,
    ) -> Self {
        Self {
            world_to_clip,
            model_to_world,
            camera_eye_unit,
            normal_scale,
            visible_front_min,
            use_world_position: true,
            visible_hemisphere: true,
        }
    }
}

fn identity() -> [[f32; 4]; 4] {
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IdMaskPolishConfig {
    pub smooth_radius_px: f32,
    pub fallback_radius_px: f32,
    /// Optional exterior city halo. Product code should own these visual values;
    /// Metal only consumes them through the JFA field compositor.
    pub exterior_halo_inner_sigma_px: f32,
    pub exterior_halo_inner_alpha: f32,
    pub exterior_halo_outer_sigma_px: f32,
    pub exterior_halo_outer_alpha: f32,
}

impl Default for IdMaskPolishConfig {
    fn default() -> Self {
        Self {
            smooth_radius_px: 0.70,
            fallback_radius_px: 2.0,
            exterior_halo_inner_sigma_px: 8.5,
            exterior_halo_inner_alpha: 0.15,
            exterior_halo_outer_sigma_px: 16.0,
            exterior_halo_outer_alpha: 0.04,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IdMaskRasterChunk {
    pub content_hash: u64,
    pub first_vertex: usize,
    pub vertex_count: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct IdMaskGpuRasterPass<'a> {
    pub viewport: api::RectF,
    pub mask_width: usize,
    pub mask_height: usize,
    pub mask_scale: f32,
    pub vertex_revision: u64,
    pub vertices: &'a [IdMaskRasterVertex],
    pub chunks: &'a [IdMaskRasterChunk],
    pub projection: IdMaskRasterProjection,
}

impl<'a> IdMaskGpuRasterPass<'a> {
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    #[must_use]
    pub fn valid_triangle_vertex_count(&self) -> bool {
        if self.vertices.is_empty() || self.chunks.is_empty() {
            return false;
        }
        let mut total = 0usize;
        for chunk in self.chunks {
            let end = chunk.first_vertex.saturating_add(chunk.vertex_count);
            if chunk.vertex_count == 0 || chunk.vertex_count % 3 != 0 || end > self.vertices.len() {
                return false;
            }
            total = total.saturating_add(chunk.vertex_count);
        }
        total == self.vertices.len() && total % 3 == 0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IdMaskGpuCompositorPass<'a> {
    pub raster: IdMaskGpuRasterPass<'a>,
    pub city_styles: [IdMaskCityStyle; ID_MASK_MAX_CITY_STYLES],
    pub neighborhood_colors: [[f32; 3]; ID_MASK_MAX_NEIGHBORHOOD_COLORS],
    pub mode: IdMaskCompositorMode,
    pub glow_enabled: bool,
    pub darken_background_alpha: f32,
    pub polish: IdMaskPolishConfig,
}
