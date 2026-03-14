//! View capture helpers for Oxide surfaces.

use oxide_renderer_api as gfx;

pub struct SurfaceCapture {
    pub viewport: gfx::RectF,
    pub device_scale: f32,
    pub draw_list: gfx::DrawList,
}

impl SurfaceCapture {
    pub fn new(viewport: gfx::RectF, device_scale: f32, draw_list: gfx::DrawList) -> Self {
        Self { viewport, device_scale, draw_list }
    }

    #[inline]
    pub fn draw_list(&self) -> &gfx::DrawList {
        &self.draw_list
    }

    #[inline]
    pub fn into_draw_list(self) -> gfx::DrawList {
        self.draw_list
    }
}
