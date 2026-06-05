use super::*;

#[repr(C)]
#[derive(Clone, Copy)]
struct CompositorGpuParams {
    viewport: [f32; 4],
    mask_size: [f32; 2],
    mask_scale: f32,
    darken_background_alpha: f32,
    mode: u32,
    glow_enabled: u32,
    polish_radius_px: f32,
    fallback_radius_px: f32,
    exterior_halo: [f32; 4],
    city_fill_colors: [[f32; 4]; id_mask_compositor::ID_MASK_MAX_CITY_STYLES],
    city_edge_colors: [[f32; 4]; id_mask_compositor::ID_MASK_MAX_CITY_STYLES],
    city_seam_colors: [[f32; 4]; id_mask_compositor::ID_MASK_MAX_CITY_STYLES],
    neighborhood_colors: [[f32; 4]; id_mask_compositor::ID_MASK_MAX_NEIGHBORHOOD_COLORS],
}

#[derive(Clone)]
pub(super) struct RenderTargets {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) city: Texture,
    pub(super) neighborhood: Texture,
    pub(super) city_field_a: Texture,
    pub(super) city_field_b: Texture,
    pub(super) seam_field_a: Texture,
    pub(super) seam_field_b: Texture,
}

#[inline]
fn configure_clear_store_attachments(
    rpd: &RenderPassDescriptorRef,
    first: &TextureRef,
    second: &TextureRef,
) {
    for (index, texture) in [(0_u64, first), (1_u64, second)] {
        let ca = rpd.color_attachments().object_at(index).unwrap();
        ca.set_texture(Some(texture));
        ca.set_load_action(MTLLoadAction::Clear);
        ca.set_clear_color(MTLClearColor { red: 0.0, green: 0.0, blue: 0.0, alpha: 0.0 });
        ca.set_store_action(MTLStoreAction::Store);
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RasterGpuParams {
    mask_size: [f32; 2],
    use_world_position: f32,
    visible_hemisphere: f32,
    world_to_clip: [[f32; 4]; 4],
    model_to_world: [[f32; 4]; 4],
    camera_eye_front_min: [f32; 4],
    normal_scale: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FieldGpuParams {
    mask_size: [f32; 2],
    jump: f32,
    _pad: f32,
}

pub(super) fn build_compositor_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "id_mask_compositor.vertex", "v_id_mask_compositor")?;
    let f = pipeline_function(lib, "id_mask_compositor.fragment", "f_id_mask_compositor")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(1);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    configure_source_alpha_blend(ca);
    pipeline_state(device, "pso.id_mask_compositor.create", &desc)
}

pub(super) fn build_raster_pso(
    device: &Device,
    lib: &Library,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "id_mask_raster.vertex", "v_id_mask_raster")?;
    let f = pipeline_function(lib, "id_mask_raster.fragment", "f_id_mask_raster")?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(1);
    for index in 0..2 {
        let ca = desc.color_attachments().object_at(index).unwrap();
        ca.set_pixel_format(MTLPixelFormat::R8Uint);
        ca.set_blending_enabled(false);
    }
    pipeline_state(device, "pso.id_mask_raster.create", &desc)
}

pub(super) fn build_field_seed_pso(
    device: &Device,
    lib: &Library,
) -> Result<RenderPipelineState, MetalInitError> {
    build_field_pso(device, lib, "f_id_mask_field_seed", "pso.id_mask_field_seed.create")
}

pub(super) fn build_field_jump_pso(
    device: &Device,
    lib: &Library,
) -> Result<RenderPipelineState, MetalInitError> {
    build_field_pso(device, lib, "f_id_mask_field_jump", "pso.id_mask_field_jump.create")
}

fn build_field_pso(
    device: &Device,
    lib: &Library,
    fragment_name: &str,
    label: &str,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "id_mask_field.vertex", "v_id_mask_field")?;
    let f = pipeline_function(lib, "id_mask_field.fragment", fragment_name)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(1);
    for index in 0..2 {
        let ca = desc.color_attachments().object_at(index).unwrap();
        ca.set_pixel_format(MTLPixelFormat::RGBA32Float);
        ca.set_blending_enabled(false);
    }
    pipeline_state(device, label, &desc)
}

impl MetalRenderer {
    fn new_r8_mask_render_texture(
        &self,
        width: usize,
        height: usize,
    ) -> Result<Texture, api::RenderError> {
        if width == 0 || height == 0 {
            return Err(api::RenderError::InvalidOperation("id-mask render target has zero size"));
        }
        let desc = TextureDescriptor::new();
        desc.set_pixel_format(MTLPixelFormat::R8Uint);
        desc.set_texture_type(MTLTextureType::D2);
        desc.set_width(width as u64);
        desc.set_height(height as u64);
        desc.set_storage_mode(MTLStorageMode::Private);
        desc.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        Ok(self.device.new_texture(&desc))
    }

    fn ensure_id_mask_render_targets(
        &mut self,
        slot: usize,
        width: usize,
        height: usize,
    ) -> Result<RenderTargets, api::RenderError> {
        let needs_new = match &self.id_mask_targets[slot] {
            Some(targets) => targets.width != width || targets.height != height,
            None => true,
        };
        if needs_new {
            let city = self.new_r8_mask_render_texture(width, height)?;
            let neighborhood = self.new_r8_mask_render_texture(width, height)?;
            let city_field_a = self.new_rgba32_float_render_texture(width, height)?;
            let city_field_b = self.new_rgba32_float_render_texture(width, height)?;
            let seam_field_a = self.new_rgba32_float_render_texture(width, height)?;
            let seam_field_b = self.new_rgba32_float_render_texture(width, height)?;
            self.id_mask_targets[slot] = Some(RenderTargets {
                width,
                height,
                city,
                neighborhood,
                city_field_a,
                city_field_b,
                seam_field_a,
                seam_field_b,
            });
        }
        let Some(targets) = &self.id_mask_targets[slot] else {
            return Err(api::RenderError::OutOfMemory);
        };
        Ok(targets.clone())
    }

    fn id_mask_vertex_cache_index(
        &mut self,
        content_hash: u64,
        vertices: &[id_mask_compositor::IdMaskRasterVertex],
    ) -> Result<usize, api::RenderError> {
        let byte_len = vertices
            .len()
            .checked_mul(core::mem::size_of::<id_mask_compositor::IdMaskRasterVertex>())
            .ok_or(api::RenderError::InvalidOperation("id-mask raster vertex data overflow"))?;
        let key = IdMaskVertexUploadKey { content_hash, byte_len };
        if let Some(index) = self.id_mask_vertex_caches.iter().position(|cache| cache.key == key) {
            return Ok(index);
        }

        let buffer = self
            .device
            .new_buffer(byte_len.max(1) as u64, MTLResourceOptions::StorageModeShared);
        let vertex_ptr = buffer.contents().cast::<u8>();
        if vertex_ptr.is_null() {
            return Err(api::RenderError::OutOfMemory);
        }
        unsafe {
            core::ptr::copy_nonoverlapping(
                vertices.as_ptr() as *const u8,
                vertex_ptr,
                byte_len,
            );
        }
        self.id_mask_vertex_caches.push(IdMaskVertexUploadCache { key, buffer });
        Ok(self.id_mask_vertex_caches.len() - 1)
    }

    fn new_rgba32_float_render_texture(
        &self,
        width: usize,
        height: usize,
    ) -> Result<Texture, api::RenderError> {
        if width == 0 || height == 0 {
            return Err(api::RenderError::InvalidOperation("id-mask field target has zero size"));
        }
        let desc = TextureDescriptor::new();
        desc.set_pixel_format(MTLPixelFormat::RGBA32Float);
        desc.set_texture_type(MTLTextureType::D2);
        desc.set_width(width as u64);
        desc.set_height(height as u64);
        desc.set_storage_mode(MTLStorageMode::Private);
        desc.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        Ok(self.device.new_texture(&desc))
    }

    fn encode_id_mask_compositor_textures(
        &mut self,
        viewport: api::RectF,
        mask_width: usize,
        mask_height: usize,
        mask_scale: f32,
        city_tex: &TextureRef,
        neighborhood_tex: &TextureRef,
        city_field_tex: &TextureRef,
        seam_field_tex: &TextureRef,
        city_styles: &[id_mask_compositor::IdMaskCityStyle;
             id_mask_compositor::ID_MASK_MAX_CITY_STYLES],
        neighborhood_colors_src: &[[f32; 3]; id_mask_compositor::ID_MASK_MAX_NEIGHBORHOOD_COLORS],
        mode: id_mask_compositor::IdMaskCompositorMode,
        glow_enabled: bool,
        darken_background_alpha: f32,
        polish: id_mask_compositor::IdMaskPolishConfig,
    ) -> Result<(), api::RenderError> {
        let Some(target_tex) = self.target_tex.as_ref().map(Texture::to_owned) else {
            return Err(api::RenderError::InvalidOperation(
                "id-mask compositor target texture unavailable",
            ));
        };

        let mut city_fill_colors = [[0.0_f32; 4]; id_mask_compositor::ID_MASK_MAX_CITY_STYLES];
        let mut city_edge_colors = [[0.0_f32; 4]; id_mask_compositor::ID_MASK_MAX_CITY_STYLES];
        let mut city_seam_colors = [[0.0_f32; 4]; id_mask_compositor::ID_MASK_MAX_CITY_STYLES];
        for (idx, style) in city_styles.iter().enumerate() {
            city_fill_colors[idx] = [style.fill_rgb[0], style.fill_rgb[1], style.fill_rgb[2], 1.0];
            city_edge_colors[idx] = [style.edge_rgb[0], style.edge_rgb[1], style.edge_rgb[2], 1.0];
            city_seam_colors[idx] = [style.seam_rgb[0], style.seam_rgb[1], style.seam_rgb[2], 1.0];
        }
        let mut neighborhood_colors =
            [[0.0_f32; 4]; id_mask_compositor::ID_MASK_MAX_NEIGHBORHOOD_COLORS];
        for (idx, rgb) in neighborhood_colors_src.iter().enumerate() {
            neighborhood_colors[idx] = [rgb[0], rgb[1], rgb[2], 1.0];
        }
        let params = CompositorGpuParams {
            viewport: [viewport.x, viewport.y, viewport.w, viewport.h],
            mask_size: [mask_width as f32, mask_height as f32],
            mask_scale: mask_scale.max(1.0),
            darken_background_alpha: darken_background_alpha.clamp(0.0, 1.0),
            mode: mode as u32,
            glow_enabled: glow_enabled as u32,
            polish_radius_px: polish.smooth_radius_px.max(0.0),
            fallback_radius_px: polish.fallback_radius_px.max(0.0),
            exterior_halo: [
                polish.exterior_halo_inner_sigma_px.max(0.0),
                polish.exterior_halo_inner_alpha.max(0.0),
                polish.exterior_halo_outer_sigma_px.max(0.0),
                polish.exterior_halo_outer_alpha.max(0.0),
            ],
            city_fill_colors,
            city_edge_colors,
            city_seam_colors,
            neighborhood_colors,
        };

        let slot = (self.frame_id % FRAME_RING_SIZE as u64) as usize;
        let cmd = self.ensure_frame_command_buffer(slot);
        let rpd = RenderPassDescriptor::new();
        let ca0 = rpd.color_attachments().object_at(0).unwrap();
        configure_frame_color_attachment(ca0, &target_tex, self.frame_color_initialized);

        let enc = cmd.new_render_command_encoder(&rpd);
        // The compositor shader builds a local full-quad for mask sampling.
        // Hardware viewport/scissor maps that quad into the requested widget
        // rect so embedded map renderers do not leak fullscreen pixels or shade
        // outside the visible surface.
        set_viewport_and_scissor_dp(&enc, self, viewport);
        enc.set_render_pipeline_state(&self.pso_id_mask_compositor);
        enc.set_vertex_bytes(
            0,
            core::mem::size_of_val(&params) as u64,
            (&params as *const CompositorGpuParams).cast(),
        );
        enc.set_fragment_bytes(
            0,
            core::mem::size_of_val(&params) as u64,
            (&params as *const CompositorGpuParams).cast(),
        );
        enc.set_fragment_texture(0, Some(city_tex));
        enc.set_fragment_texture(1, Some(neighborhood_tex));
        enc.set_fragment_texture(2, Some(city_field_tex));
        enc.set_fragment_texture(3, Some(seam_field_tex));
        enc.draw_primitives(MTLPrimitiveType::Triangle, 0, 6);
        enc.end_encoding();

        self.acc_draws = self.acc_draws.saturating_add(1);
        self.frame_color_initialized = true;
        if let Some(t0) = self.frame_encode_started_at {
            self.last_stats.encode_ms = t0.elapsed().as_secs_f64() * 1000.0;
        }
        self.last_stats.draws = self.acc_draws;
        Ok(())
    }

    pub fn encode_id_mask_gpu_compositor(
        &mut self,
        pass: &id_mask_compositor::IdMaskGpuCompositorPass<'_>,
    ) -> Result<(), api::RenderError> {
        if self.frame_backpressure_skipped {
            return Ok(());
        }
        if self.sample_count != 1 {
            return Err(api::RenderError::Unsupported(
                "id-mask GPU compositor currently requires MetalRenderer sample_count == 1",
            ));
        }
        // The compositor deliberately supports interleaving with 2D passes.
        // High-resolution JFA/ID-mask polish may appear at a draw-list position
        // inside app UI, so this pass must load the current frame when earlier
        // 2D content has already initialized it.
        if pass.raster.mask_width == 0 || pass.raster.mask_height == 0 {
            return Err(api::RenderError::InvalidOperation(
                "id-mask GPU raster has zero dimensions",
            ));
        }
        if !pass.raster.valid_triangle_vertex_count() {
            return Err(api::RenderError::InvalidOperation(
                "id-mask GPU raster vertices must be non-empty triangles",
            ));
        }

        self.ensure_target();
        let slot = self.current_frame_slot();
        let targets = self.ensure_id_mask_render_targets(
            slot,
            pass.raster.mask_width,
            pass.raster.mask_height,
        )?;
        for chunk in pass.raster.chunks {
            let end = chunk.first_vertex.saturating_add(chunk.vertex_count);
            let Some(vertices) = pass.raster.vertices.get(chunk.first_vertex..end) else {
                return Err(api::RenderError::InvalidOperation(
                    "id-mask GPU raster chunk range is outside vertex data",
                ));
            };
            self.id_mask_vertex_cache_index(chunk.content_hash, vertices)?;
        }
        let params = RasterGpuParams {
            mask_size: [pass.raster.mask_width as f32, pass.raster.mask_height as f32],
            use_world_position: if pass.raster.projection.use_world_position { 1.0 } else { 0.0 },
            visible_hemisphere: if pass.raster.projection.visible_hemisphere { 1.0 } else { 0.0 },
            world_to_clip: pass.raster.projection.world_to_clip,
            model_to_world: pass.raster.projection.model_to_world,
            camera_eye_front_min: [
                pass.raster.projection.camera_eye_unit[0],
                pass.raster.projection.camera_eye_unit[1],
                pass.raster.projection.camera_eye_unit[2],
                pass.raster.projection.visible_front_min,
            ],
            normal_scale: [
                pass.raster.projection.normal_scale[0],
                pass.raster.projection.normal_scale[1],
                pass.raster.projection.normal_scale[2],
                0.0,
            ],
        };

        let cmd = self.ensure_frame_command_buffer(slot);
        let rpd = RenderPassDescriptor::new();
        configure_clear_store_attachments(&rpd, &targets.city, &targets.neighborhood);
        let enc = cmd.new_render_command_encoder(&rpd);
        enc.set_render_pipeline_state(&self.pso_id_mask_raster);
        enc.set_vertex_bytes(
            1,
            core::mem::size_of_val(&params) as u64,
            (&params as *const RasterGpuParams).cast(),
        );
        for chunk in pass.raster.chunks {
            let end = chunk.first_vertex.saturating_add(chunk.vertex_count);
            let Some(vertices) = pass.raster.vertices.get(chunk.first_vertex..end) else {
                continue;
            };
            let cache_index = self.id_mask_vertex_cache_index(chunk.content_hash, vertices)?;
            let Some(cache) = self.id_mask_vertex_caches.get(cache_index) else {
                continue;
            };
            enc.set_vertex_buffer(0, Some(&cache.buffer), 0);
            enc.draw_primitives(MTLPrimitiveType::Triangle, 0, chunk.vertex_count as u64);
        }
        enc.end_encoding();

        let field_params = FieldGpuParams {
            mask_size: [pass.raster.mask_width as f32, pass.raster.mask_height as f32],
            jump: 0.0,
            _pad: 0.0,
        };
        let rpd = RenderPassDescriptor::new();
        configure_clear_store_attachments(&rpd, &targets.city_field_a, &targets.seam_field_a);
        let enc = cmd.new_render_command_encoder(&rpd);
        enc.set_render_pipeline_state(&self.pso_id_mask_field_seed);
        enc.set_fragment_bytes(
            0,
            core::mem::size_of_val(&field_params) as u64,
            (&field_params as *const FieldGpuParams).cast(),
        );
        enc.set_fragment_texture(0, Some(&targets.city));
        enc.set_fragment_texture(1, Some(&targets.neighborhood));
        enc.draw_primitives(MTLPrimitiveType::Triangle, 0, 6);
        enc.end_encoding();

        // Keep nearest-city and seam lookup work in these ping-pong field
        // passes. The final beauty compositor is per-visible-pixel, so doing
        // radius searches there was the source of the WebGPU/Metal perf cliff.
        let mut src_is_a = true;
        let mut jump = pass.raster.mask_width.max(pass.raster.mask_height).next_power_of_two() / 2;
        while jump >= 1 {
            let params = FieldGpuParams {
                mask_size: [pass.raster.mask_width as f32, pass.raster.mask_height as f32],
                jump: jump as f32,
                _pad: 0.0,
            };
            let (src_city, src_seam, dst_city, dst_seam) = if src_is_a {
                (
                    &targets.city_field_a,
                    &targets.seam_field_a,
                    &targets.city_field_b,
                    &targets.seam_field_b,
                )
            } else {
                (
                    &targets.city_field_b,
                    &targets.seam_field_b,
                    &targets.city_field_a,
                    &targets.seam_field_a,
                )
            };
            let rpd = RenderPassDescriptor::new();
            configure_clear_store_attachments(&rpd, dst_city, dst_seam);
            let enc = cmd.new_render_command_encoder(&rpd);
            enc.set_render_pipeline_state(&self.pso_id_mask_field_jump);
            enc.set_fragment_bytes(
                0,
                core::mem::size_of_val(&params) as u64,
                (&params as *const FieldGpuParams).cast(),
            );
            enc.set_fragment_texture(0, Some(src_city));
            enc.set_fragment_texture(1, Some(src_seam));
            enc.draw_primitives(MTLPrimitiveType::Triangle, 0, 6);
            enc.end_encoding();
            src_is_a = !src_is_a;
            jump /= 2;
        }
        let (city_field_tex, seam_field_tex) = if src_is_a {
            (&targets.city_field_a, &targets.seam_field_a)
        } else {
            (&targets.city_field_b, &targets.seam_field_b)
        };

        self.encode_id_mask_compositor_textures(
            pass.raster.viewport,
            pass.raster.mask_width,
            pass.raster.mask_height,
            pass.raster.mask_scale,
            &targets.city,
            &targets.neighborhood,
            city_field_tex,
            seam_field_tex,
            &pass.city_styles,
            &pass.neighborhood_colors,
            pass.mode,
            pass.glow_enabled,
            pass.darken_background_alpha,
            pass.polish,
        )
    }
}
