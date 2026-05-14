use super::*;

#[repr(C)]
#[derive(Clone, Copy)]
struct MarkerGpuParams {
    viewport: [f32; 4],
    marker_count: u32,
    _pad: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MarkerGpuInstance {
    center: [f32; 2],
    core_radius_px: f32,
    ring_radius_px: f32,
    ring_width_px: f32,
    halo_radius_px: f32,
    halo_sigma_px: f32,
    halo_alpha_max: f32,
    ring_alpha_max: f32,
    core_color: [f32; 4],
    ring_color: [f32; 4],
}

pub(super) fn build_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "neon_marker.vertex", "v_neon_marker")?;
    let f = pipeline_function(lib, "neon_marker.fragment", "f_neon_marker")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    configure_source_alpha_blend(ca);
    pipeline_state(device, "pso.neon_marker.create", &desc)
}

impl MetalRenderer {
    pub fn encode_neon_markers(
        &mut self,
        pass: &neon_marker::NeonMarkerPass<'_>,
    ) -> Result<(), api::RenderError> {
        if self.sample_count != 1 {
            return Err(api::RenderError::Unsupported(
                "neon marker compositor currently requires MetalRenderer sample_count == 1",
            ));
        }
        if self.frame_2d_encoded {
            return Err(api::RenderError::InvalidOperation(
                "encode_neon_markers must run before encode_pass within a frame",
            ));
        }
        let marker_count = pass.clamped_len();
        if marker_count == 0 {
            return Ok(());
        }

        self.ensure_target();
        let slot = (self.frame_id % FRAME_RING_SIZE as u64) as usize;
        let cmd = self.ensure_frame_command_buffer(slot);
        let Some(target_tex) = self.target_tex.as_ref().map(Texture::to_owned) else {
            return Err(api::RenderError::InvalidOperation(
                "neon marker target texture unavailable",
            ));
        };

        let params = MarkerGpuParams {
            viewport: [pass.viewport.x, pass.viewport.y, pass.viewport.w, pass.viewport.h],
            marker_count: marker_count as u32,
            _pad: [0, 0, 0],
        };
        let mut markers = [MarkerGpuInstance {
            center: [0.0, 0.0],
            core_radius_px: 0.0,
            ring_radius_px: 0.0,
            ring_width_px: 0.0,
            halo_radius_px: 0.0,
            halo_sigma_px: 1.0,
            halo_alpha_max: 0.0,
            ring_alpha_max: 0.0,
            core_color: [0.0, 0.0, 0.0, 0.0],
            ring_color: [0.0, 0.0, 0.0, 0.0],
        }; neon_marker::NEON_MARKER_MAX_INSTANCES];
        for (dst, marker) in markers.iter_mut().zip(pass.markers.iter()).take(marker_count) {
            *dst = MarkerGpuInstance {
                center: marker.center,
                core_radius_px: marker.core_radius_px.max(0.0),
                ring_radius_px: marker.ring_radius_px.max(0.0),
                ring_width_px: marker.ring_width_px.max(0.001),
                halo_radius_px: marker.halo_radius_px.max(0.0),
                halo_sigma_px: marker.halo_sigma_px.max(0.001),
                halo_alpha_max: marker.halo_alpha_max.clamp(0.0, 1.0),
                ring_alpha_max: marker.ring_alpha_max.clamp(0.0, 1.0),
                core_color: [
                    marker.core_color.r.clamp(0.0, 1.0),
                    marker.core_color.g.clamp(0.0, 1.0),
                    marker.core_color.b.clamp(0.0, 1.0),
                    marker.core_color.a.clamp(0.0, 1.0),
                ],
                ring_color: [
                    marker.ring_color.r.clamp(0.0, 1.0),
                    marker.ring_color.g.clamp(0.0, 1.0),
                    marker.ring_color.b.clamp(0.0, 1.0),
                    marker.ring_color.a.clamp(0.0, 1.0),
                ],
            };
        }

        let rpd = RenderPassDescriptor::new();
        let ca0 = rpd.color_attachments().object_at(0).unwrap();
        ca0.set_texture(Some(&target_tex));
        ca0.set_store_action(MTLStoreAction::Store);
        if self.frame_color_initialized {
            ca0.set_load_action(MTLLoadAction::Load);
        } else {
            ca0.set_load_action(MTLLoadAction::Clear);
            ca0.set_clear_color(MTLClearColor { red: 0.0, green: 0.0, blue: 0.0, alpha: 1.0 });
        }

        let enc = cmd.new_render_command_encoder(&rpd);
        enc.set_render_pipeline_state(&self.pso_neon_marker);
        enc.set_vertex_bytes(
            0,
            core::mem::size_of_val(&params) as u64,
            (&params as *const MarkerGpuParams).cast(),
        );
        enc.set_vertex_bytes(
            1,
            (core::mem::size_of::<MarkerGpuInstance>() * marker_count) as u64,
            markers.as_ptr().cast(),
        );
        enc.set_fragment_bytes(
            0,
            core::mem::size_of_val(&params) as u64,
            (&params as *const MarkerGpuParams).cast(),
        );
        enc.set_fragment_bytes(
            1,
            (core::mem::size_of::<MarkerGpuInstance>() * marker_count) as u64,
            markers.as_ptr().cast(),
        );
        enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, marker_count as u64);
        enc.end_encoding();

        self.acc_draws = self.acc_draws.saturating_add(marker_count as u32);
        self.frame_color_initialized = true;
        if let Some(t0) = self.frame_encode_started_at {
            self.last_stats.encode_ms = t0.elapsed().as_secs_f64() * 1000.0;
        }
        self.last_stats.draws = self.acc_draws;
        Ok(())
    }
}
