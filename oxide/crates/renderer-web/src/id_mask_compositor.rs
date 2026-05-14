use oxide_renderer_api as api;

pub const ID_MASK_MAX_CITY_STYLES: usize = 4;
pub const ID_MASK_MAX_NEIGHBORHOOD_COLORS: usize = 32;

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

impl Default for IdMaskCityStyle {
    fn default() -> Self {
        Self { fill_rgb: [1.0, 1.0, 1.0], edge_rgb: [1.0, 1.0, 1.0], seam_rgb: [1.0, 1.0, 1.0] }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IdMaskRasterVertex {
    pub position_px: [f32; 2],
    pub city_id: u32,
    pub neighborhood_id: u32,
}

impl IdMaskRasterVertex {
    #[must_use]
    pub fn new(position_px: [f32; 2], city_id: u8, neighborhood_id: u8) -> Self {
        Self { position_px, city_id: city_id as u32, neighborhood_id: neighborhood_id as u32 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IdMaskPolishConfig {
    pub smooth_radius_px: f32,
    pub fallback_radius_px: f32,
}

impl Default for IdMaskPolishConfig {
    fn default() -> Self {
        Self { smooth_radius_px: 0.70, fallback_radius_px: 2.0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IdMaskGpuRasterPass<'a> {
    pub viewport: api::RectF,
    pub mask_width: usize,
    pub mask_height: usize,
    pub mask_scale: f32,
    pub vertices: &'a [IdMaskRasterVertex],
}

impl<'a> IdMaskGpuRasterPass<'a> {
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    #[must_use]
    pub fn valid_triangle_vertex_count(&self) -> bool {
        !self.vertices.is_empty() && self.vertices.len() % 3 == 0
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
