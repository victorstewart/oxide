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

#[derive(Clone, Copy, Debug)]
pub struct IdMaskCompositorPass<'a> {
    pub viewport: api::RectF,
    pub mask_width: usize,
    pub mask_height: usize,
    pub mask_scale: f32,
    pub city_ids: &'a [u8],
    pub neighborhood_ids: &'a [u8],
    pub city_styles: [IdMaskCityStyle; ID_MASK_MAX_CITY_STYLES],
    pub neighborhood_colors: [[f32; 3]; ID_MASK_MAX_NEIGHBORHOOD_COLORS],
    pub mode: IdMaskCompositorMode,
    pub glow_enabled: bool,
    pub darken_background_alpha: f32,
}

impl<'a> IdMaskCompositorPass<'a> {
    #[must_use]
    pub fn expected_len(&self) -> Option<usize> {
        self.mask_width.checked_mul(self.mask_height)
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
        Self {
            position_px,
            city_id: city_id as u32,
            neighborhood_id: neighborhood_id as u32,
        }
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

#[derive(Clone, Debug)]
pub struct IdMaskBuffer {
    width: usize,
    height: usize,
    scale: f32,
    city_ids: Vec<u8>,
    neighborhood_ids: Vec<u8>,
}

impl IdMaskBuffer {
    #[must_use]
    pub fn new(width: usize, height: usize, scale: f32) -> Option<Self> {
        let len = width.checked_mul(height)?;
        Some(Self {
            width,
            height,
            scale,
            city_ids: vec![0; len],
            neighborhood_ids: vec![0; len],
        })
    }

    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    #[must_use]
    pub fn city_ids(&self) -> &[u8] {
        &self.city_ids
    }

    #[must_use]
    pub fn neighborhood_ids(&self) -> &[u8] {
        &self.neighborhood_ids
    }

    pub fn rasterize_triangle(
        &mut self,
        a: [f32; 2],
        b: [f32; 2],
        c: [f32; 2],
        city_code: u8,
        neighborhood_code: u8,
    ) {
        let area = edge_function(a, b, c);
        if area.abs() <= 1.0e-4 {
            return;
        }
        let min_x = a[0].min(b[0]).min(c[0]).floor().max(0.0) as usize;
        let max_x = a[0]
            .max(b[0])
            .max(c[0])
            .ceil()
            .min((self.width.saturating_sub(1)) as f32) as usize;
        let min_y = a[1].min(b[1]).min(c[1]).floor().max(0.0) as usize;
        let max_y = a[1]
            .max(b[1])
            .max(c[1])
            .ceil()
            .min((self.height.saturating_sub(1)) as f32) as usize;
        if max_x < min_x || max_y < min_y {
            return;
        }
        let positive = area > 0.0;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let w0 = edge_function(b, c, p);
                let w1 = edge_function(c, a, p);
                let w2 = edge_function(a, b, p);
                let inside = if positive {
                    w0 >= -0.01 && w1 >= -0.01 && w2 >= -0.01
                } else {
                    w0 <= 0.01 && w1 <= 0.01 && w2 <= 0.01
                };
                if inside {
                    let idx = y * self.width + x;
                    self.city_ids[idx] = city_code;
                    self.neighborhood_ids[idx] = neighborhood_code;
                }
            }
        }
    }

    pub fn polish_city_masks(
        &mut self,
        city_codes: &[u8],
        smooth_radius_px: usize,
        fallback_radius_px: usize,
    ) {
        let len = self.city_ids.len();
        let mut polished_city = vec![0u8; len];
        let mut polished_neighborhood = vec![0u8; len];
        for city_code in city_codes {
            let source_mask = self
                .city_ids
                .iter()
                .map(|code| *code == *city_code)
                .collect::<Vec<_>>();
            if !source_mask.iter().any(|value| *value) {
                continue;
            }
            let opened = open_mask(&source_mask, self.width, self.height, smooth_radius_px);
            let smoothed = close_mask(&opened, self.width, self.height, smooth_radius_px);
            let fallback_neighborhood = self.first_neighborhood_for_city(*city_code);
            for (idx, included) in smoothed.iter().enumerate() {
                if !*included {
                    continue;
                }
                let neighborhood = if self.city_ids[idx] == *city_code && self.neighborhood_ids[idx] != 0 {
                    self.neighborhood_ids[idx]
                } else {
                    self.nearest_neighborhood(idx, *city_code, fallback_radius_px)
                        .or(fallback_neighborhood)
                        .unwrap_or(0)
                };
                if neighborhood != 0 && polished_city[idx] == 0 {
                    polished_city[idx] = *city_code;
                    polished_neighborhood[idx] = neighborhood;
                }
            }
        }
        self.city_ids = polished_city;
        self.neighborhood_ids = polished_neighborhood;
    }

    #[must_use]
    pub fn neighborhood_bbox(&self, neighborhood_code: u8) -> Option<api::RectF> {
        let mut min_x = self.width;
        let mut min_y = self.height;
        let mut max_x = 0usize;
        let mut max_y = 0usize;
        let mut found = false;
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = y * self.width + x;
                if self.neighborhood_ids[idx] == neighborhood_code {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                    found = true;
                }
            }
        }
        if !found {
            return None;
        }
        let inv = 1.0 / self.scale;
        Some(api::RectF::new(
            min_x as f32 * inv,
            min_y as f32 * inv,
            (max_x.saturating_sub(min_x).max(1) + 1) as f32 * inv,
            (max_y.saturating_sub(min_y).max(1) + 1) as f32 * inv,
        ))
    }

    #[must_use]
    pub fn compositor_pass<'a>(
        &'a self,
        viewport: api::RectF,
        city_styles: [IdMaskCityStyle; ID_MASK_MAX_CITY_STYLES],
        neighborhood_colors: [[f32; 3]; ID_MASK_MAX_NEIGHBORHOOD_COLORS],
        mode: IdMaskCompositorMode,
        glow_enabled: bool,
        darken_background_alpha: f32,
    ) -> IdMaskCompositorPass<'a> {
        IdMaskCompositorPass {
            viewport,
            mask_width: self.width,
            mask_height: self.height,
            mask_scale: self.scale,
            city_ids: &self.city_ids,
            neighborhood_ids: &self.neighborhood_ids,
            city_styles,
            neighborhood_colors,
            mode,
            glow_enabled,
            darken_background_alpha,
        }
    }

    fn first_neighborhood_for_city(&self, city_code: u8) -> Option<u8> {
        self.city_ids
            .iter()
            .zip(self.neighborhood_ids.iter())
            .find_map(|(city, neighborhood)| {
                if *city == city_code && *neighborhood != 0 {
                    Some(*neighborhood)
                } else {
                    None
                }
            })
    }

    fn nearest_neighborhood(&self, idx: usize, city_code: u8, max_radius: usize) -> Option<u8> {
        let x = idx % self.width;
        let y = idx / self.width;
        for radius in 1..=max_radius {
            let min_x = x.saturating_sub(radius);
            let max_x = (x + radius).min(self.width.saturating_sub(1));
            let min_y = y.saturating_sub(radius);
            let max_y = (y + radius).min(self.height.saturating_sub(1));
            for nx in min_x..=max_x {
                for ny in [min_y, max_y] {
                    let n = ny * self.width + nx;
                    if self.city_ids[n] == city_code && self.neighborhood_ids[n] != 0 {
                        return Some(self.neighborhood_ids[n]);
                    }
                }
            }
            for ny in min_y.saturating_add(1)..max_y {
                for nx in [min_x, max_x] {
                    let n = ny * self.width + nx;
                    if self.city_ids[n] == city_code && self.neighborhood_ids[n] != 0 {
                        return Some(self.neighborhood_ids[n]);
                    }
                }
            }
        }
        None
    }
}

fn edge_function(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0])
}

fn close_mask(source: &[bool], width: usize, height: usize, radius_px: usize) -> Vec<bool> {
    if radius_px == 0 || !source.iter().any(|value| *value) {
        return source.to_vec();
    }
    let radius = radius_px as f32;
    let dilated_distance = distance_from_mask(source, width, height);
    let dilated = dilated_distance
        .iter()
        .map(|distance| *distance as f32 / 3.0 <= radius)
        .collect::<Vec<_>>();
    let inverse = dilated.iter().map(|value| !*value).collect::<Vec<_>>();
    let inverse_distance = distance_from_mask(&inverse, width, height);
    inverse_distance
        .iter()
        .map(|distance| *distance as f32 / 3.0 > radius)
        .collect()
}

fn open_mask(source: &[bool], width: usize, height: usize, radius_px: usize) -> Vec<bool> {
    if radius_px == 0 || !source.iter().any(|value| *value) {
        return source.to_vec();
    }
    let radius = radius_px as f32;
    let inverse = source.iter().map(|value| !*value).collect::<Vec<_>>();
    let inverse_distance = distance_from_mask(&inverse, width, height);
    let eroded = inverse_distance
        .iter()
        .map(|distance| *distance as f32 / 3.0 > radius)
        .collect::<Vec<_>>();
    let eroded_distance = distance_from_mask(&eroded, width, height);
    eroded_distance
        .iter()
        .map(|distance| *distance as f32 / 3.0 <= radius)
        .collect()
}

fn distance_from_mask(source: &[bool], width: usize, height: usize) -> Vec<u16> {
    const INF: u16 = 30_000;
    const ORTHOGONAL: u16 = 3;
    const DIAGONAL: u16 = 4;
    let mut distance = source
        .iter()
        .map(|value| if *value { 0 } else { INF })
        .collect::<Vec<_>>();
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let mut best = distance[idx];
            if x > 0 {
                best = best.min(distance[idx - 1].saturating_add(ORTHOGONAL));
            }
            if y > 0 {
                best = best.min(distance[idx - width].saturating_add(ORTHOGONAL));
                if x > 0 {
                    best = best.min(distance[idx - width - 1].saturating_add(DIAGONAL));
                }
                if x + 1 < width {
                    best = best.min(distance[idx - width + 1].saturating_add(DIAGONAL));
                }
            }
            distance[idx] = best;
        }
    }
    for y in (0..height).rev() {
        for x in (0..width).rev() {
            let idx = y * width + x;
            let mut best = distance[idx];
            if x + 1 < width {
                best = best.min(distance[idx + 1].saturating_add(ORTHOGONAL));
            }
            if y + 1 < height {
                best = best.min(distance[idx + width].saturating_add(ORTHOGONAL));
                if x + 1 < width {
                    best = best.min(distance[idx + width + 1].saturating_add(DIAGONAL));
                }
                if x > 0 {
                    best = best.min(distance[idx + width - 1].saturating_add(DIAGONAL));
                }
            }
            distance[idx] = best;
        }
    }
    distance
}
