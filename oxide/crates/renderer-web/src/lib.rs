//! Oxide WebAssembly renderer backed by HTML Canvas2D.

#![forbid(unsafe_code)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use oxide_renderer_api as api;

mod solid_color;

pub mod id_mask_compositor;
pub mod neon_marker;
pub mod scene3d;

const MAX_LAYER_DIMENSION: u32 = 16_384;

#[cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]
fn saturating_texture_bytes(width: u64, height: u64, bytes_per_pixel: u64) -> u64 {
    width.saturating_mul(height).saturating_mul(bytes_per_pixel)
}

/// Per-frame counters emitted by the web renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WebRendererStats {
    pub frame_id: u64,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub draws: u32,
    pub draw_items: u32,
    pub draw_items_coalesced: u32,
    pub draw_pipeline_binds: u32,
    pub draw_bind_group_binds: u32,
    pub draw_scissor_sets: u32,
    pub solid_tris: u32,
    pub rrect_instances: u32,
    pub rrect_triangles: u32,
    pub rrect_instance_bytes: u64,
    pub image_instances: u32,
    pub image_triangles: u32,
    pub image_instance_bytes: u64,
    pub image_draws: u32,
    pub image_mesh_draws: u32,
    pub nine_slice_draws: u32,
    pub nine_slice_instances: u32,
    pub nine_slice_triangles: u32,
    pub nine_slice_instance_bytes: u64,
    pub glyph_quads: u32,
    pub sdf_glyph_quads: u32,
    pub layer_draws: u32,
    pub layer_cache_hits: u32,
    pub layer_cache_misses: u32,
    pub layer_cache_skipped_draws: u32,
    pub layer_cache_budget_bytes: u64,
    pub layer_cache_resident_bytes: u64,
    pub layer_cache_pool_bytes: u64,
    pub layer_cache_cpu_bytes: u64,
    pub layer_cache_oldest_last_used_frame: u64,
    pub layer_cache_pool_reuses: u64,
    pub layer_cache_evictions: u64,
    pub layer_cache_recreations: u64,
    pub layer_cache_purges: u64,
    pub layer_cache_last_purge_reason: u8,
    pub layer_passes: u32,
    pub scene3d_draws: u32,
    pub id_mask_draws: u32,
    pub id_mask_cache_hits: u32,
    pub id_mask_cache_misses: u32,
    pub id_mask_cache_budget_bytes: u64,
    pub id_mask_cache_resident_bytes: u64,
    pub id_mask_cache_evictions: u64,
    pub id_mask_cache_entries: u32,
    pub id_mask_cache_purges: u64,
    pub id_mask_cache_last_purge_reason: u8,
    pub backdrop_draws: u32,
    pub visual_effect_draws: u32,
    pub effect_uniform_writes: u32,
    pub effect_uniform_bytes: u64,
    pub effect_uniform_slots: u32,
    pub id_mask_uniform_writes: u32,
    pub id_mask_uniform_bytes: u64,
    pub id_mask_uniform_slots: u32,
    pub spinner_draws: u32,
    pub spinner_instances: u32,
    pub spinner_triangles: u32,
    pub spinner_instance_bytes: u64,
    pub neon_marker_instances: u32,
    pub neon_marker_triangles: u32,
    pub neon_marker_instance_bytes: u64,
    pub camera_bg_draws: u32,
    pub clip_depth_peak: u32,
    pub damage_rects: u32,
    pub render_passes: u32,
    pub clear_passes: u32,
    pub draw_passes: u32,
    pub scene3d_passes: u32,
    pub scene3d_overlay_passes: u32,
    pub id_mask_raster_passes: u32,
    pub id_mask_field_seed_passes: u32,
    pub id_mask_field_jump_passes: u32,
    pub id_mask_compositor_passes: u32,
    pub present_passes: u32,
    pub texture_copies: u32,
    pub command_buffers: u32,
    pub commands_traversed: u64,
    pub commands_copied: u64,
    pub geometry_bytes_copied: u64,
    pub chunks_reused: u64,
    pub chunks_rebuilt: u64,
    pub chunks_prepared: u64,
    pub backend_cache_hits: u64,
    pub backend_cache_misses: u64,
    pub render_encoders: u32,
    pub render_bundle_creates: u32,
    pub render_bundle_replays: u32,
    pub render_bundle_execute_calls: u32,
    pub render_bundle_draws: u32,
    pub prepared_direct_draws: u32,
    pub texture_copy_pixels: u64,
    pub texture_copy_bytes: u64,
    pub shaded_damage_pixels: u64,
    pub cache_evictions: u32,
    pub wakeups: u32,
    pub skipped_submissions: u32,
    pub actual_submissions: u32,
    pub submit_upload_alloc_count: u64,
    pub submit_upload_alloc_bytes: u64,
    pub submit_surface_alloc_count: u64,
    pub submit_surface_alloc_bytes: u64,
    pub submit_encoder_alloc_count: u64,
    pub submit_encoder_alloc_bytes: u64,
    pub submit_render_alloc_count: u64,
    pub submit_render_alloc_bytes: u64,
    pub submit_timestamp_alloc_count: u64,
    pub submit_timestamp_alloc_bytes: u64,
    pub submit_scratch_stats_alloc_count: u64,
    pub submit_scratch_stats_alloc_bytes: u64,
    pub submit_finish_queue_alloc_count: u64,
    pub submit_finish_queue_alloc_bytes: u64,
    pub submit_present_alloc_count: u64,
    pub submit_present_alloc_bytes: u64,
    pub submit_timestamp_map_alloc_count: u64,
    pub submit_timestamp_map_alloc_bytes: u64,
    pub submit_total_alloc_count: u64,
    pub submit_total_alloc_bytes: u64,
    pub submit_total_realloc_count: u64,
    pub submit_total_realloc_grow_bytes: u64,
    pub gpu_timestamp_query_supported: bool,
    pub gpu_timestamp_frame_id: u64,
    pub gpu_timestamp_passes: u32,
    pub gpu_timestamp_total_ns: u64,
    pub gpu_timestamp_clear_ns: u64,
    pub gpu_timestamp_draw_ns: u64,
    pub gpu_timestamp_scene3d_ns: u64,
    pub gpu_timestamp_scene3d_overlay_ns: u64,
    pub gpu_timestamp_id_mask_raster_ns: u64,
    pub gpu_timestamp_id_mask_field_seed_ns: u64,
    pub gpu_timestamp_id_mask_field_jump_ns: u64,
    pub gpu_timestamp_id_mask_compositor_ns: u64,
    pub gpu_timestamp_present_ns: u64,
    pub gpu_timestamp_max_pass_ns: u64,
    pub gpu_timestamp_readback_skips: u32,
    pub gpu_timestamp_readback_interval: u32,
    pub buffer_upload_bytes: u64,
    pub property_upload_bytes: u64,
    pub property_records_updated: u32,
    pub property_ring_bytes: u64,
    pub texture_upload_bytes: u64,
    pub buffer_grows: u32,
    pub texture_creates: u32,
    pub bind_group_creates: u32,
    pub pipeline_creates: u32,
    pub sampler_creates: u32,
    pub mesh3d_creates: u32,
    pub draw_buffer_grows: u32,
    pub image_texture_creates: u32,
    pub image_bind_group_creates: u32,
    pub target_texture_creates: u32,
    pub target_bind_group_creates: u32,
    pub layer_texture_creates: u32,
    pub layer_bind_group_creates: u32,
    pub scene3d_buffer_grows: u32,
    pub scene3d_bind_group_creates: u32,
    pub effect_buffer_grows: u32,
    pub effect_bind_group_creates: u32,
    pub id_mask_texture_creates: u32,
    pub id_mask_buffer_grows: u32,
    pub id_mask_bind_group_creates: u32,
    pub image_upload_temp_allocs: u32,
    pub image_upload_temp_bytes: u64,
    pub image_upload_scratch_bytes: u64,
    pub image_upload_scratch_grows: u32,
    pub cpu_scratch_bytes: u64,
    pub cpu_scratch_grows: u32,
    pub cpu_scratch_growth_bytes: u64,
    pub cpu_draw_scratch_bytes: u64,
    pub cpu_draw_scratch_grows: u32,
    pub cpu_draw_scratch_growth_bytes: u64,
    pub cpu_scene3d_scratch_bytes: u64,
    pub cpu_scene3d_scratch_grows: u32,
    pub cpu_scene3d_scratch_growth_bytes: u64,
    pub cpu_effect_scratch_bytes: u64,
    pub cpu_effect_scratch_grows: u32,
    pub cpu_effect_scratch_growth_bytes: u64,
    pub cpu_id_mask_scratch_bytes: u64,
    pub cpu_id_mask_scratch_grows: u32,
    pub cpu_id_mask_scratch_growth_bytes: u64,
    pub cpu_image_upload_scratch_bytes: u64,
    pub cpu_image_upload_scratch_grows: u32,
    pub cpu_image_upload_scratch_growth_bytes: u64,
    pub cpu_resource_table_scratch_bytes: u64,
    pub cpu_resource_table_scratch_grows: u32,
    pub cpu_resource_table_scratch_growth_bytes: u64,
    pub gpu_allocated_bytes_available: bool,
    pub gpu_logical_total_bytes: u64,
    pub gpu_allocated_total_bytes: u64,
    pub gpu_vertex_buffer_bytes: u64,
    pub gpu_index_buffer_bytes: u64,
    pub gpu_uniform_buffer_bytes: u64,
    pub gpu_persistent_asset_bytes: u64,
    pub gpu_transient_target_bytes: u64,
    pub gpu_depth_target_bytes: u64,
    pub gpu_bloom_target_bytes: u64,
    pub gpu_layer_texture_bytes: u64,
    pub gpu_id_mask_texture_bytes: u64,
    pub gpu_atlas_texture_bytes: u64,
    pub gpu_image_texture_bytes: u64,
    pub gpu_scene3d_mesh_bytes: u64,
    pub gpu_staging_buffer_bytes: u64,
    pub gpu_bind_buffer_bytes: u64,
    pub gpu_frame_ring_bytes: u64,
    pub gpu_cache_bytes: u64,
}

/// One completed per-frame WebGPU timestamp-query sample.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WebGpuTimestampSample {
    pub frame_id: u64,
    pub passes: u32,
    pub total_ns: u64,
    pub clear_ns: u64,
    pub draw_ns: u64,
    pub scene3d_ns: u64,
    pub scene3d_overlay_ns: u64,
    pub id_mask_raster_ns: u64,
    pub id_mask_field_seed_ns: u64,
    pub id_mask_field_jump_ns: u64,
    pub id_mask_compositor_ns: u64,
    pub present_ns: u64,
    pub max_pass_ns: u64,
}

/// Optional CPU timing for the app-owned phases inside one WebGPU submit.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WebGpuCpuSubmitTimingSample {
    pub upload_ms: f64,
    pub surface_ms: f64,
    pub encoder_create_ms: f64,
    pub command_encoding_ms: f64,
    pub timestamp_readback_ms: f64,
    pub scratch_stats_ms: f64,
    pub queue_submit_ms: f64,
    pub present_ms: f64,
    pub timestamp_map_ms: f64,
}

impl Default for WebRendererStats {
    fn default() -> Self {
        Self {
            frame_id: 0,
            width: 0,
            height: 0,
            scale: 1.0,
            draws: 0,
            draw_items: 0,
            draw_items_coalesced: 0,
            draw_pipeline_binds: 0,
            draw_bind_group_binds: 0,
            draw_scissor_sets: 0,
            solid_tris: 0,
            rrect_instances: 0,
            rrect_triangles: 0,
            rrect_instance_bytes: 0,
            image_instances: 0,
            image_triangles: 0,
            image_instance_bytes: 0,
            image_draws: 0,
            image_mesh_draws: 0,
            nine_slice_draws: 0,
            nine_slice_instances: 0,
            nine_slice_triangles: 0,
            nine_slice_instance_bytes: 0,
            glyph_quads: 0,
            sdf_glyph_quads: 0,
            layer_draws: 0,
            layer_cache_hits: 0,
            layer_cache_misses: 0,
            layer_cache_skipped_draws: 0,
            layer_cache_budget_bytes: 0,
            layer_cache_resident_bytes: 0,
            layer_cache_pool_bytes: 0,
            layer_cache_cpu_bytes: 0,
            layer_cache_oldest_last_used_frame: 0,
            layer_cache_pool_reuses: 0,
            layer_cache_evictions: 0,
            layer_cache_recreations: 0,
            layer_cache_purges: 0,
            layer_cache_last_purge_reason: 0,
            layer_passes: 0,
            scene3d_draws: 0,
            id_mask_draws: 0,
            id_mask_cache_hits: 0,
            id_mask_cache_misses: 0,
            id_mask_cache_budget_bytes: 0,
            id_mask_cache_resident_bytes: 0,
            id_mask_cache_evictions: 0,
            id_mask_cache_entries: 0,
            id_mask_cache_purges: 0,
            id_mask_cache_last_purge_reason: 0,
            backdrop_draws: 0,
            visual_effect_draws: 0,
            effect_uniform_writes: 0,
            effect_uniform_bytes: 0,
            effect_uniform_slots: 0,
            id_mask_uniform_writes: 0,
            id_mask_uniform_bytes: 0,
            id_mask_uniform_slots: 0,
            spinner_draws: 0,
            spinner_instances: 0,
            spinner_triangles: 0,
            spinner_instance_bytes: 0,
            neon_marker_instances: 0,
            neon_marker_triangles: 0,
            neon_marker_instance_bytes: 0,
            camera_bg_draws: 0,
            clip_depth_peak: 0,
            damage_rects: 0,
            render_passes: 0,
            clear_passes: 0,
            draw_passes: 0,
            scene3d_passes: 0,
            scene3d_overlay_passes: 0,
            id_mask_raster_passes: 0,
            id_mask_field_seed_passes: 0,
            id_mask_field_jump_passes: 0,
            id_mask_compositor_passes: 0,
            present_passes: 0,
            texture_copies: 0,
            command_buffers: 0,
            commands_traversed: 0,
            commands_copied: 0,
            geometry_bytes_copied: 0,
            chunks_reused: 0,
            chunks_rebuilt: 0,
            chunks_prepared: 0,
            backend_cache_hits: 0,
            backend_cache_misses: 0,
            render_encoders: 0,
            render_bundle_creates: 0,
            render_bundle_replays: 0,
            render_bundle_execute_calls: 0,
            render_bundle_draws: 0,
            prepared_direct_draws: 0,
            texture_copy_pixels: 0,
            texture_copy_bytes: 0,
            shaded_damage_pixels: 0,
            cache_evictions: 0,
            wakeups: 0,
            skipped_submissions: 0,
            actual_submissions: 0,
            submit_upload_alloc_count: 0,
            submit_upload_alloc_bytes: 0,
            submit_surface_alloc_count: 0,
            submit_surface_alloc_bytes: 0,
            submit_encoder_alloc_count: 0,
            submit_encoder_alloc_bytes: 0,
            submit_render_alloc_count: 0,
            submit_render_alloc_bytes: 0,
            submit_timestamp_alloc_count: 0,
            submit_timestamp_alloc_bytes: 0,
            submit_scratch_stats_alloc_count: 0,
            submit_scratch_stats_alloc_bytes: 0,
            submit_finish_queue_alloc_count: 0,
            submit_finish_queue_alloc_bytes: 0,
            submit_present_alloc_count: 0,
            submit_present_alloc_bytes: 0,
            submit_timestamp_map_alloc_count: 0,
            submit_timestamp_map_alloc_bytes: 0,
            submit_total_alloc_count: 0,
            submit_total_alloc_bytes: 0,
            submit_total_realloc_count: 0,
            submit_total_realloc_grow_bytes: 0,
            gpu_timestamp_query_supported: false,
            gpu_timestamp_frame_id: 0,
            gpu_timestamp_passes: 0,
            gpu_timestamp_total_ns: 0,
            gpu_timestamp_clear_ns: 0,
            gpu_timestamp_draw_ns: 0,
            gpu_timestamp_scene3d_ns: 0,
            gpu_timestamp_scene3d_overlay_ns: 0,
            gpu_timestamp_id_mask_raster_ns: 0,
            gpu_timestamp_id_mask_field_seed_ns: 0,
            gpu_timestamp_id_mask_field_jump_ns: 0,
            gpu_timestamp_id_mask_compositor_ns: 0,
            gpu_timestamp_present_ns: 0,
            gpu_timestamp_max_pass_ns: 0,
            gpu_timestamp_readback_skips: 0,
            gpu_timestamp_readback_interval: 0,
            buffer_upload_bytes: 0,
            property_upload_bytes: 0,
            property_records_updated: 0,
            property_ring_bytes: 0,
            texture_upload_bytes: 0,
            buffer_grows: 0,
            texture_creates: 0,
            bind_group_creates: 0,
            pipeline_creates: 0,
            sampler_creates: 0,
            mesh3d_creates: 0,
            draw_buffer_grows: 0,
            image_texture_creates: 0,
            image_bind_group_creates: 0,
            target_texture_creates: 0,
            target_bind_group_creates: 0,
            layer_texture_creates: 0,
            layer_bind_group_creates: 0,
            scene3d_buffer_grows: 0,
            scene3d_bind_group_creates: 0,
            effect_buffer_grows: 0,
            effect_bind_group_creates: 0,
            id_mask_texture_creates: 0,
            id_mask_buffer_grows: 0,
            id_mask_bind_group_creates: 0,
            image_upload_temp_allocs: 0,
            image_upload_temp_bytes: 0,
            image_upload_scratch_bytes: 0,
            image_upload_scratch_grows: 0,
            cpu_scratch_bytes: 0,
            cpu_scratch_grows: 0,
            cpu_scratch_growth_bytes: 0,
            cpu_draw_scratch_bytes: 0,
            cpu_draw_scratch_grows: 0,
            cpu_draw_scratch_growth_bytes: 0,
            cpu_scene3d_scratch_bytes: 0,
            cpu_scene3d_scratch_grows: 0,
            cpu_scene3d_scratch_growth_bytes: 0,
            cpu_effect_scratch_bytes: 0,
            cpu_effect_scratch_grows: 0,
            cpu_effect_scratch_growth_bytes: 0,
            cpu_id_mask_scratch_bytes: 0,
            cpu_id_mask_scratch_grows: 0,
            cpu_id_mask_scratch_growth_bytes: 0,
            cpu_image_upload_scratch_bytes: 0,
            cpu_image_upload_scratch_grows: 0,
            cpu_image_upload_scratch_growth_bytes: 0,
            cpu_resource_table_scratch_bytes: 0,
            cpu_resource_table_scratch_grows: 0,
            cpu_resource_table_scratch_growth_bytes: 0,
            gpu_allocated_bytes_available: false,
            gpu_logical_total_bytes: 0,
            gpu_allocated_total_bytes: 0,
            gpu_vertex_buffer_bytes: 0,
            gpu_index_buffer_bytes: 0,
            gpu_uniform_buffer_bytes: 0,
            gpu_persistent_asset_bytes: 0,
            gpu_transient_target_bytes: 0,
            gpu_depth_target_bytes: 0,
            gpu_bloom_target_bytes: 0,
            gpu_layer_texture_bytes: 0,
            gpu_id_mask_texture_bytes: 0,
            gpu_atlas_texture_bytes: 0,
            gpu_image_texture_bytes: 0,
            gpu_scene3d_mesh_bytes: 0,
            gpu_staging_buffer_bytes: 0,
            gpu_bind_buffer_bytes: 0,
            gpu_frame_ring_bytes: 0,
            gpu_cache_bytes: 0,
        }
    }
}

/// Returns a finite, positive canvas scale.
#[must_use]
pub fn sanitize_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// Converts an Oxide color to a Canvas2D CSS color string.
#[must_use]
pub fn color_to_css(color: api::Color) -> String {
    let r = color_channel(color.r);
    let g = color_channel(color.g);
    let b = color_channel(color.b);
    let a = color.a.clamp(0.0, 1.0);
    format!("rgba({r}, {g}, {b}, {a:.3})")
}

/// Converts packed Oxide RGBA vertex color to a Canvas2D CSS color string.
#[must_use]
pub fn packed_rgba_to_css(rgba: u32) -> String {
    let r = rgba & 0xFF;
    let g = (rgba >> 8) & 0xFF;
    let b = (rgba >> 16) & 0xFF;
    let a = ((rgba >> 24) & 0xFF) as f32 / 255.0;
    format!("rgba({r}, {g}, {b}, {a:.3})")
}

/// Packs an Oxide color into the same RGBA byte layout used by text vertices.
#[must_use]
pub fn color_cache_key(color: api::Color) -> u32 {
    let red = color_channel(color.r);
    let green = color_channel(color.g);
    let blue = color_channel(color.b);
    let alpha = color_channel(color.a);
    (alpha << 24) | (blue << 16) | (green << 8) | red
}

fn color_channel(channel: f32) -> u32 {
    if channel.is_finite() {
        (channel.clamp(0.0, 1.0) * 255.0).round() as u32
    } else {
        0
    }
}

#[must_use]
pub fn layer_physical_dimension(logical: f32, scale: f32) -> u32 {
    if !logical.is_finite() || logical <= 0.0 {
        return 1;
    }
    let scaled = (logical * sanitize_scale(scale)).ceil();
    if !scaled.is_finite() || scaled <= 0.0 {
        1
    } else {
        scaled.min(MAX_LAYER_DIMENSION as f32) as u32
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn logical_dimension(physical: u32, scale: f32) -> f32 {
    physical as f32 / sanitize_scale(scale)
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn copy_rows(
    width: u32,
    height: u32,
    bytes_per_pixel: usize,
    data: &[u8],
    row_bytes: usize,
) -> Option<Vec<u8>> {
    let row_width = (width as usize).checked_mul(bytes_per_pixel)?;
    let total = (height as usize).checked_mul(row_width)?;
    if row_bytes < row_width {
        return None;
    }
    let required = if height == 0 {
        0
    } else {
        row_bytes
            .checked_mul(height.saturating_sub(1) as usize)?
            .checked_add(row_width)?
    };
    if data.len() < required {
        return None;
    }
    let mut out = vec![0_u8; total];
    for y in 0..height as usize {
        let src = y.checked_mul(row_bytes)?;
        let dst = y.checked_mul(row_width)?;
        out[dst..dst + row_width].copy_from_slice(&data[src..src + row_width]);
    }
    Some(out)
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn copy_rgba_rows(width: u32, height: u32, data: &[u8], row_bytes: usize) -> Option<Vec<u8>> {
    copy_rows(width, height, 4, data, row_bytes)
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn copy_rgba_rows_into(
    out: &mut Vec<u8>,
    width: u32,
    height: u32,
    data: &[u8],
    row_bytes: usize,
) -> Option<bool> {
    copy_rows_into(out, width, height, 4, data, row_bytes)
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn copy_a8_rows_into(
    out: &mut Vec<u8>,
    width: u32,
    height: u32,
    data: &[u8],
    row_bytes: usize,
) -> Option<bool> {
    copy_rows_into(out, width, height, 1, data, row_bytes)
}

fn copy_rows_into(
    out: &mut Vec<u8>,
    width: u32,
    height: u32,
    bytes_per_pixel: usize,
    data: &[u8],
    row_bytes: usize,
) -> Option<bool> {
    let row_width = (width as usize).checked_mul(bytes_per_pixel)?;
    let total = (height as usize).checked_mul(row_width)?;
    if row_bytes < row_width {
        return None;
    }
    let required = if height == 0 {
        0
    } else {
        row_bytes
            .checked_mul(height.saturating_sub(1) as usize)?
            .checked_add(row_width)?
    };
    if data.len() < required {
        return None;
    }
    let grew = out.capacity() < total;
    out.clear();
    if grew {
        out.reserve_exact(total.saturating_sub(out.capacity()));
    }
    for y in 0..height as usize {
        let src = y.checked_mul(row_bytes)?;
        out.extend_from_slice(&data[src..src + row_width]);
    }
    Some(grew)
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn copy_a8_rows(width: u32, height: u32, data: &[u8], row_bytes: usize) -> Option<Vec<u8>> {
    copy_rows(width, height, 1, data, row_bytes)
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn a8_to_rgba(alpha: &[u8]) -> Vec<u8> {
    let mut rgba = vec![255_u8; alpha.len().saturating_mul(4)];
    for (idx, coverage) in alpha.iter().copied().enumerate() {
        let base = idx.saturating_mul(4);
        rgba[base + 3] = coverage;
    }
    rgba
}

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
enum NormalizedIndexMode {
    Local,
    Rebase { vertex_base: u32 },
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn normalized_index_mode(
    source: &[u16],
    vertex_base: u32,
    vertex_count: u32,
) -> Option<NormalizedIndexMode> {
    if source.is_empty() {
        return Some(NormalizedIndexMode::Local);
    }
    if vertex_count == 0 {
        return None;
    }
    if vertex_count <= u16::MAX as u32 {
        let local_limit = vertex_count as u16;
        if source.iter().all(|index| *index < local_limit) {
            return Some(NormalizedIndexMode::Local);
        }
    }

    let vertex_end = vertex_base.saturating_add(vertex_count);
    for index in source.iter().copied() {
        let absolute = index as u32;
        if absolute < vertex_base || absolute >= vertex_end {
            return None;
        }
    }
    Some(NormalizedIndexMode::Rebase { vertex_base })
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn resolve_index(index: u16, mode: NormalizedIndexMode) -> Option<usize> {
    match mode {
        NormalizedIndexMode::Local => Some(index as usize),
        NormalizedIndexMode::Rebase { vertex_base } => {
            let absolute = index as u32;
            absolute.checked_sub(vertex_base).map(|local| local as usize)
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[path = "wasm/image_slots.rs"]
mod image_slots;

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[path = "wasm/packed_geometry.rs"]
mod packed_geometry;

#[cfg(target_arch = "wasm32")]
mod wasm {
    #[path = "webgpu.rs"]
    mod webgpu;

    use super::{
        a8_to_rgba, color_cache_key, color_to_css, copy_a8_rows, copy_a8_rows_into,
        copy_rgba_rows, copy_rgba_rows_into,
        layer_physical_dimension, logical_dimension, normalized_index_mode, packed_rgba_to_css,
        resolve_index, sanitize_scale, saturating_texture_bytes, NormalizedIndexMode,
        WebRendererStats,
    };
    use crate::solid_color::colored_quad;
    use oxide_renderer_api as api;
    use oxide_renderer_api::Renderer;
    use std::collections::BTreeMap;
    use std::fmt::Write;
    use wasm_bindgen::{Clamped, JsCast, JsValue};
    use web_sys::{CanvasRenderingContext2d, Document, HtmlCanvasElement, ImageData};

    pub use webgpu::{BrowserRenderer, WebGpuRenderer};
    #[cfg(feature = "snapshot-tests")]
    pub use webgpu::WebIdMaskSnapshotReadback;

    #[allow(dead_code)]
    enum WebImageKind {
        Rgba,
        A8 { alpha: Vec<u8>, tinted: BTreeMap<u32, HtmlCanvasElement> },
    }

    struct WebImage {
        canvas: HtmlCanvasElement,
        width: u32,
        height: u32,
        kind: WebImageKind,
    }

    struct CachedLayer {
        canvas: HtmlCanvasElement,
        width: u32,
        height: u32,
        scale: f32,
    }

    struct LayerFrame {
        id: u32,
        rect: api::RectF,
        canvas: HtmlCanvasElement,
        parent_ctx: CanvasRenderingContext2d,
        parent_clip_depth: u32,
    }

    /// HTML canvas renderer for wasm32 browser hosts.
    pub struct WebRenderer {
        canvas: HtmlCanvasElement,
        ctx: CanvasRenderingContext2d,
        images: Vec<Option<WebImage>>,
        layers: BTreeMap<u32, CachedLayer>,
        layer_stack: Vec<LayerFrame>,
        width: u32,
        height: u32,
        scale: f32,
        frame_id: u64,
        active_token: Option<api::FrameToken>,
        stats: WebRendererStats,
        clip_depth: u32,
    }

    #[allow(dead_code)]
    impl WebRenderer {
        /// Creates a renderer from a DOM canvas id.
        pub fn from_canvas_id(id: &str) -> Result<Self, api::RenderError> {
            let document = document()?;
            let Some(element) = document.get_element_by_id(id) else {
                return Err(api::RenderError::ResourceNotFound("canvas element not found"));
            };
            let canvas = element
                .dyn_into::<HtmlCanvasElement>()
                .map_err(|_| api::RenderError::InvalidOperation("element is not a canvas"))?;
            Self::from_canvas(canvas)
        }

        /// Creates a renderer from an existing HTML canvas.
        pub fn from_canvas(canvas: HtmlCanvasElement) -> Result<Self, api::RenderError> {
            let ctx = canvas_context(&canvas)?;
            ctx.set_image_smoothing_enabled(true);
            Ok(Self {
                canvas,
                ctx,
                images: vec![None],
                layers: BTreeMap::new(),
                layer_stack: Vec::new(),
                width: 0,
                height: 0,
                scale: 1.0,
                frame_id: 0,
                active_token: None,
                stats: WebRendererStats::default(),
                clip_depth: 0,
            })
        }

        /// Returns the backing canvas element.
        #[must_use]
        pub fn canvas(&self) -> HtmlCanvasElement {
            self.canvas.clone()
        }

        /// Returns the last submitted frame counters.
        #[must_use]
        pub fn last_stats(&self) -> WebRendererStats {
            self.stats
        }

        /// Creates an RGBA8 image resource from row-strided source bytes.
        #[must_use]
        pub fn image_create_rgba8(
            &mut self,
            width: u32,
            height: u32,
            data: &[u8],
            row_bytes: usize,
        ) -> api::ImageHandle {
            match self.try_image_create_rgba8(width, height, data, row_bytes) {
                Ok(handle) => handle,
                Err(_) => api::ImageHandle(0),
            }
        }

        /// Creates an A8 glyph atlas resource from row-strided coverage bytes.
        #[must_use]
        pub fn image_create_a8(
            &mut self,
            width: u32,
            height: u32,
            data: &[u8],
            row_bytes: usize,
        ) -> api::ImageHandle {
            match self.try_image_create_a8(width, height, data, row_bytes) {
                Ok(handle) => handle,
                Err(_) => api::ImageHandle(0),
            }
        }

        /// Updates an A8 glyph atlas subrectangle.
        pub fn image_update_a8(
            &mut self,
            handle: api::ImageHandle,
            x: u32,
            y: u32,
            width: u32,
            height: u32,
            data: &[u8],
            row_bytes: usize,
        ) {
            let _ = self.try_image_update_a8(handle, x, y, width, height, data, row_bytes);
        }

        pub fn try_image_create_rgba8(
            &mut self,
            width: u32,
            height: u32,
            data: &[u8],
            row_bytes: usize,
        ) -> Result<api::ImageHandle, api::RenderError> {
            let rgba = copy_rgba_rows(width, height, data, row_bytes)
                .ok_or(api::RenderError::InvalidOperation("invalid rgba image rows"))?;
            let canvas = canvas_from_rgba(width, height, &rgba)?;
            Ok(self.push_image(WebImage { canvas, width, height, kind: WebImageKind::Rgba }))
        }

        pub fn try_image_create_a8(
            &mut self,
            width: u32,
            height: u32,
            data: &[u8],
            row_bytes: usize,
        ) -> Result<api::ImageHandle, api::RenderError> {
            let alpha = copy_a8_rows(width, height, data, row_bytes)
                .ok_or(api::RenderError::InvalidOperation("invalid a8 image rows"))?;
            let rgba = a8_to_rgba(&alpha);
            let canvas = canvas_from_rgba(width, height, &rgba)?;
            Ok(self.push_image(WebImage {
                canvas,
                width,
                height,
                kind: WebImageKind::A8 { alpha, tinted: BTreeMap::new() },
            }))
        }

        pub fn try_image_update_a8(
            &mut self,
            handle: api::ImageHandle,
            x: u32,
            y: u32,
            width: u32,
            height: u32,
            data: &[u8],
            row_bytes: usize,
        ) -> Result<(), api::RenderError> {
            let patch = copy_a8_rows(width, height, data, row_bytes)
                .ok_or(api::RenderError::InvalidOperation("invalid a8 update rows"))?;
            let Some(image) = self.image_mut(handle) else {
                return Err(api::RenderError::ResourceNotFound("image handle not found"));
            };
            let WebImageKind::A8 { alpha, tinted } = &mut image.kind else {
                return Err(api::RenderError::InvalidOperation("image is not an a8 atlas"));
            };
            if x.saturating_add(width) > image.width || y.saturating_add(height) > image.height {
                return Err(api::RenderError::InvalidOperation("a8 update outside image bounds"));
            }

            for row in 0..height as usize {
                let src = row.saturating_mul(width as usize);
                let dst = (y as usize + row)
                    .saturating_mul(image.width as usize)
                    .saturating_add(x as usize);
                alpha[dst..dst + width as usize].copy_from_slice(&patch[src..src + width as usize]);
            }
            tinted.clear();

            let rgba = a8_to_rgba(&patch);
            let image_data =
                ImageData::new_with_u8_clamped_array_and_sh(Clamped(&rgba), width, height)
                    .map_err(|err| js_error("image data", err))?;
            let ctx = canvas_context(&image.canvas)?;
            ctx.put_image_data(&image_data, x as f64, y as f64)
                .map_err(|err| js_error("put image data", err))
        }

        pub fn try_image_update_rgba8(
            &mut self,
            handle: api::ImageHandle,
            x: u32,
            y: u32,
            width: u32,
            height: u32,
            data: &[u8],
            row_bytes: usize,
        ) -> Result<(), api::RenderError> {
            let rgba = copy_rgba_rows(width, height, data, row_bytes)
                .ok_or(api::RenderError::InvalidOperation("invalid rgba update rows"))?;
            let Some(image) = self.image_mut(handle) else {
                return Err(api::RenderError::ResourceNotFound("image handle not found"));
            };
            if !matches!(&image.kind, WebImageKind::Rgba) {
                return Err(api::RenderError::InvalidOperation("image is not an rgba texture"));
            }
            if x.saturating_add(width) > image.width || y.saturating_add(height) > image.height {
                return Err(api::RenderError::InvalidOperation("rgba update outside image bounds"));
            }

            let image_data =
                ImageData::new_with_u8_clamped_array_and_sh(Clamped(&rgba), width, height)
                    .map_err(|err| js_error("image data", err))?;
            let ctx = canvas_context(&image.canvas)?;
            ctx.put_image_data(&image_data, x as f64, y as f64)
                .map_err(|err| js_error("put image data", err))
        }

        fn push_image(&mut self, image: WebImage) -> api::ImageHandle {
            let handle = api::ImageHandle(self.images.len() as u32);
            self.images.push(Some(image));
            handle
        }

        fn image(&self, handle: api::ImageHandle) -> Option<&WebImage> {
            self.images.get(handle.0 as usize).and_then(Option::as_ref)
        }

        fn image_mut(&mut self, handle: api::ImageHandle) -> Option<&mut WebImage> {
            self.images.get_mut(handle.0 as usize).and_then(Option::as_mut)
        }

        fn reset_clip_stack(&mut self) {
            while self.clip_depth > 0 {
                self.ctx.restore();
                self.clip_depth -= 1;
            }
        }

        fn push_clip(&mut self, rect: api::RectI) {
            self.ctx.save();
            self.clip_depth = self.clip_depth.saturating_add(1);
            self.stats.clip_depth_peak = self.stats.clip_depth_peak.max(self.clip_depth);
            self.ctx.begin_path();
            self.ctx.rect(rect.x as f64, rect.y as f64, rect.w.max(0) as f64, rect.h.max(0) as f64);
            self.ctx.clip();
        }

        fn pop_clip(&mut self) {
            if self.clip_depth > 0 {
                self.ctx.restore();
                self.clip_depth -= 1;
            }
        }

        fn discard_open_layers(&mut self) {
            while let Some(frame) = self.layer_stack.pop() {
                self.reset_clip_stack();
                self.ctx = frame.parent_ctx;
                self.clip_depth = frame.parent_clip_depth;
            }
        }

        fn finish_open_layers(&mut self) {
            while !self.layer_stack.is_empty() {
                let _ = self.end_layer_context();
            }
        }

        fn encode_items(
            &mut self,
            list: &api::DrawList,
            index: &mut usize,
            stop_at_layer_end: bool,
        ) {
            while *index < list.items.len() {
                match &list.items[*index] {
                    api::DrawCmd::LayerBegin { id, rect, dirty } => {
                        *index += 1;
                        self.encode_layer(list, index, *id, *rect, *dirty);
                    }
                    api::DrawCmd::LayerEnd => {
                        *index += 1;
                        if stop_at_layer_end {
                            return;
                        }
                    }
                    item => {
                        self.encode_draw_cmd(list, item);
                        *index += 1;
                    }
                }
            }
        }

        fn encode_draw_cmd(&mut self, list: &api::DrawList, item: &api::DrawCmd) {
            match item {
                api::DrawCmd::LayerBegin { .. } | api::DrawCmd::LayerEnd => {}
                api::DrawCmd::Solid { vb, ib, color } => {
                    self.draw_solid_span(list, *vb, *ib, *color)
                }
                api::DrawCmd::Image { tex, dst, src, alpha } => {
                    self.draw_image_rect(*tex, *dst, *src, *alpha)
                }
                api::DrawCmd::ImageMesh { tex, vb, ib, alpha } => {
                    self.draw_image_mesh_from_list(list, *tex, *vb, *ib, *alpha)
                }
                api::DrawCmd::GlyphRun { run } => self.draw_glyph_run_from_list(list, run),
                api::DrawCmd::RRect { rect, radii, color } => {
                    self.draw_rrect_path(*rect, *radii, *color)
                }
                api::DrawCmd::NineSlice { tex, rect, slice, alpha } => {
                    self.draw_nine_slice(*tex, *rect, *slice, *alpha)
                }
                api::DrawCmd::Backdrop { rect, sigma, tint, alpha } => {
                    self.stats.backdrop_draws = self.stats.backdrop_draws.saturating_add(1);
                    self.draw_backdrop_fallback(*rect, *sigma, *tint, *alpha)
                }
                api::DrawCmd::VisualEffect { rect, effect } => {
                    let tint = effect.tint();
                    self.stats.visual_effect_draws =
                        self.stats.visual_effect_draws.saturating_add(1);
                    self.draw_backdrop_fallback(
                        *rect,
                        effect.blur_intensity() * 72.0,
                        tint,
                        tint.a,
                    );
                }
                api::DrawCmd::CameraBg { .. } => {}
                api::DrawCmd::Spinner { center, atom, alpha } => {
                    self.stats.spinner_draws = self.stats.spinner_draws.saturating_add(1);
                    self.draw_spinner_shape(*center, *atom, *alpha)
                }
                api::DrawCmd::ClipPush { rect } => self.push_clip(*rect),
                api::DrawCmd::ClipPop => self.pop_clip(),
            }
        }

        fn encode_layer(
            &mut self,
            list: &api::DrawList,
            index: &mut usize,
            id: u32,
            rect: api::RectF,
            dirty: bool,
        ) {
            let (width, height) = layer_dimensions(rect, self.scale);
            if !dirty {
                if let Some(canvas) = self.cached_layer_canvas(id, width, height) {
                    skip_layer_body(list, index);
                    self.draw_layer_canvas(&canvas, rect);
                    return;
                }
            }

            if self.begin_layer_context(id, rect, width, height).is_ok() {
                self.encode_items(list, index, true);
                if let Some((layer_id, cached)) = self.end_layer_context() {
                    if layer_id != 0 {
                        self.layers.insert(layer_id, cached);
                    }
                }
            } else {
                self.encode_items(list, index, true);
            }
        }

        fn cached_layer_canvas(
            &self,
            id: u32,
            width: u32,
            height: u32,
        ) -> Option<HtmlCanvasElement> {
            if id == 0 {
                return None;
            }
            let layer = self.layers.get(&id)?;
            if layer.width == width
                && layer.height == height
                && (layer.scale - self.scale).abs() <= f32::EPSILON
            {
                Some(layer.canvas.clone())
            } else {
                None
            }
        }

        fn begin_layer_context(
            &mut self,
            id: u32,
            rect: api::RectF,
            width: u32,
            height: u32,
        ) -> Result<(), api::RenderError> {
            let canvas = create_canvas(width, height)?;
            let layer_ctx = canvas_context(&canvas)?;
            layer_ctx.set_image_smoothing_enabled(true);
            layer_ctx.clear_rect(0.0, 0.0, width as f64, height as f64);
            let scale = sanitize_scale(self.scale) as f64;
            layer_ctx
                .set_transform(
                    scale,
                    0.0,
                    0.0,
                    scale,
                    -(rect.x as f64) * scale,
                    -(rect.y as f64) * scale,
                )
                .map_err(|err| js_error("layer transform", err))?;

            let parent_ctx = self.ctx.clone();
            let parent_clip_depth = self.clip_depth;
            self.ctx = layer_ctx;
            self.clip_depth = 0;
            self.layer_stack.push(LayerFrame { id, rect, canvas, parent_ctx, parent_clip_depth });
            Ok(())
        }

        fn end_layer_context(&mut self) -> Option<(u32, CachedLayer)> {
            let frame = self.layer_stack.pop()?;
            self.reset_clip_stack();
            let layer_ctx = self.ctx.clone();
            self.ctx = frame.parent_ctx;
            self.clip_depth = frame.parent_clip_depth;
            drop(layer_ctx);

            self.draw_layer_canvas(&frame.canvas, frame.rect);
            let cached = CachedLayer {
                width: frame.canvas.width(),
                height: frame.canvas.height(),
                scale: self.scale,
                canvas: frame.canvas,
            };
            Some((frame.id, cached))
        }

        fn draw_layer_canvas(&mut self, canvas: &HtmlCanvasElement, rect: api::RectF) {
            if rect.w <= 0.0 || rect.h <= 0.0 {
                return;
            }
            let result = self.ctx.draw_image_with_html_canvas_element_and_dw_and_dh(
                canvas,
                rect.x as f64,
                rect.y as f64,
                rect.w as f64,
                rect.h as f64,
            );
            if result.is_ok() {
                self.stats.draws = self.stats.draws.saturating_add(1);
                self.stats.layer_draws = self.stats.layer_draws.saturating_add(1);
            }
        }

        fn draw_solid_span(
            &mut self,
            list: &api::DrawList,
            vb: api::VertexSpan,
            ib: api::IndexSpan,
            color: api::Color,
        ) {
            let Some(vertices) = vertex_slice(list, vb) else {
                return;
            };
            if vertices.iter().any(|vertex| vertex.rgba != 0) {
                if ib.len != 0 {
                    return;
                }
                let Some((rect, start, end, start_rgba, end_rgba)) =
                    colored_quad(vertices, color)
                else {
                    return;
                };
                if start_rgba == end_rgba {
                    self.ctx.set_fill_style_str(&packed_rgba_to_css(start_rgba));
                } else {
                    let gradient = self.ctx.create_linear_gradient(
                        start[0] as f64,
                        start[1] as f64,
                        end[0] as f64,
                        end[1] as f64,
                    );
                    if gradient.add_color_stop(0.0, &packed_rgba_to_css(start_rgba)).is_err()
                        || gradient.add_color_stop(1.0, &packed_rgba_to_css(end_rgba)).is_err()
                    {
                        return;
                    }
                    self.ctx.set_fill_style_canvas_gradient(&gradient);
                }
                self.ctx.fill_rect(rect.x as f64, rect.y as f64, rect.w as f64, rect.h as f64);
                self.stats.draws = self.stats.draws.saturating_add(1);
                self.stats.solid_tris = self.stats.solid_tris.saturating_add(2);
                return;
            }
            let css = color_to_css(color);
            self.ctx.set_fill_style_str(&css);
            if ib.len > 0 {
                let Some(indices) = index_slice(list, ib) else {
                    return;
                };
                let Some(mode) = normalized_index_mode(indices, vb.offset, vb.len) else {
                    return;
                };
                for tri in indices.chunks_exact(3) {
                    let Some(a) = resolve_index(tri[0], mode).and_then(|idx| vertices.get(idx))
                    else {
                        continue;
                    };
                    let Some(b) = resolve_index(tri[1], mode).and_then(|idx| vertices.get(idx))
                    else {
                        continue;
                    };
                    let Some(c) = resolve_index(tri[2], mode).and_then(|idx| vertices.get(idx))
                    else {
                        continue;
                    };
                    self.fill_triangle(*a, *b, *c);
                }
            } else if vertices.len() == 4 {
                self.fill_triangle(vertices[0], vertices[1], vertices[2]);
                self.fill_triangle(vertices[2], vertices[1], vertices[3]);
            } else {
                for tri in vertices.chunks_exact(3) {
                    self.fill_triangle(tri[0], tri[1], tri[2]);
                }
            }
        }

        fn fill_triangle(&mut self, a: api::Vertex, b: api::Vertex, c: api::Vertex) {
            self.ctx.begin_path();
            self.ctx.move_to(a.x as f64, a.y as f64);
            self.ctx.line_to(b.x as f64, b.y as f64);
            self.ctx.line_to(c.x as f64, c.y as f64);
            self.ctx.close_path();
            self.ctx.fill();
            self.stats.draws = self.stats.draws.saturating_add(1);
            self.stats.solid_tris = self.stats.solid_tris.saturating_add(1);
        }

        fn draw_image_rect(
            &mut self,
            handle: api::ImageHandle,
            dst: api::RectF,
            src: api::RectF,
            alpha: f32,
        ) {
            self.draw_image_rect_with_filter(handle, dst, src, alpha, None);
        }

        fn draw_image_rect_with_filter(
            &mut self,
            handle: api::ImageHandle,
            dst: api::RectF,
            src: api::RectF,
            alpha: f32,
            filter: Option<&str>,
        ) {
            if dst.w <= 0.0 || dst.h <= 0.0 {
                return;
            }
            let Some(image) = self.image(handle) else {
                return;
            };
            let (sx, sy, sw, sh) = source_rect(src, image.width, image.height);
            self.ctx.save();
            self.ctx.set_global_alpha(alpha.clamp(0.0, 1.0) as f64);
            if let Some(filter) = filter {
                self.ctx.set_filter(filter);
            }
            let result = self
                .ctx
                .draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                    &image.canvas,
                    sx,
                    sy,
                    sw,
                    sh,
                    dst.x as f64,
                    dst.y as f64,
                    dst.w as f64,
                    dst.h as f64,
                );
            self.ctx.restore();
            if result.is_ok() {
                self.stats.draws = self.stats.draws.saturating_add(1);
                self.stats.image_draws = self.stats.image_draws.saturating_add(1);
            }
        }

        fn draw_image_mesh_from_list(
            &mut self,
            list: &api::DrawList,
            handle: api::ImageHandle,
            vb: api::VertexSpan,
            ib: api::IndexSpan,
            alpha: f32,
        ) {
            if self.image(handle).is_none() {
                return;
            }
            let Some(vertices) = vertex_slice(list, vb) else {
                return;
            };
            let indices = index_slice(list, ib).unwrap_or(&[]);
            let mode = if indices.is_empty() {
                None
            } else {
                let Some(mode) = normalized_index_mode(indices, vb.offset, vb.len) else {
                    return;
                };
                Some(mode)
            };
            self.stats.image_mesh_draws = self.stats.image_mesh_draws.saturating_add(1);
            draw_vertex_quads(vertices, indices, mode, |quad| {
                self.draw_image_mesh_quad(handle, quad, alpha);
            });
        }

        fn draw_image_mesh_quad(
            &mut self,
            handle: api::ImageHandle,
            quad: &[api::Vertex],
            alpha: f32,
        ) {
            let dst = quad_rect(quad);
            if dst.w <= 0.0 || dst.h <= 0.0 {
                return;
            }
            let Some(image) = self.image(handle) else {
                return;
            };
            let src = uv_rect(quad, image.width, image.height);
            self.draw_image_rect(handle, dst, src, alpha);
        }

        fn draw_glyph_run_from_list(&mut self, list: &api::DrawList, run: &api::GlyphRun) {
            let Some(vertices) = vertex_slice(list, run.vb) else {
                return;
            };
            let indices = index_slice(list, run.ib).unwrap_or(&[]);
            self.draw_glyph_run_vertices(run, vertices, indices);
        }

        fn draw_glyph_run_vertices(
            &mut self,
            run: &api::GlyphRun,
            vertices: &[api::Vertex],
            indices: &[u16],
        ) {
            let mode = if indices.is_empty() { None } else { Some(NormalizedIndexMode::Local) };
            draw_vertex_quads(vertices, indices, mode, |quad| {
                self.draw_glyph_quad(run, quad);
            });
        }

        fn draw_glyph_quad(&mut self, run: &api::GlyphRun, quad: &[api::Vertex]) {
            let dst = quad_rect(quad);
            if dst.w <= 0.0 || dst.h <= 0.0 {
                return;
            }
            let Some(source_canvas) = self.tinted_or_base_canvas(run.atlas, run.color) else {
                return;
            };
            let Some(image) = self.image(run.atlas) else {
                return;
            };
            let src = uv_rect(quad, image.width, image.height);
            self.ctx.save();
            self.ctx.set_global_alpha(run.color.a.clamp(0.0, 1.0) as f64);
            let result = self
                .ctx
                .draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                    &source_canvas,
                    src.x as f64,
                    src.y as f64,
                    src.w as f64,
                    src.h as f64,
                    dst.x as f64,
                    dst.y as f64,
                    dst.w as f64,
                    dst.h as f64,
                );
            self.ctx.restore();
            if result.is_ok() {
                self.stats.draws = self.stats.draws.saturating_add(1);
                self.stats.glyph_quads = self.stats.glyph_quads.saturating_add(1);
                if run.sdf {
                    self.stats.sdf_glyph_quads = self.stats.sdf_glyph_quads.saturating_add(1);
                }
            }
        }

        fn tinted_or_base_canvas(
            &mut self,
            handle: api::ImageHandle,
            color: api::Color,
        ) -> Option<HtmlCanvasElement> {
            let key = color_cache_key(color);
            let image = self.image_mut(handle)?;
            match &mut image.kind {
                WebImageKind::Rgba => Some(image.canvas.clone()),
                WebImageKind::A8 { tinted, .. } => {
                    if let Some(canvas) = tinted.get(&key) {
                        return Some(canvas.clone());
                    }
                    let canvas =
                        tinted_canvas(&image.canvas, image.width, image.height, color).ok()?;
                    tinted.insert(key, canvas.clone());
                    Some(canvas)
                }
            }
        }

        fn draw_nine_slice(
            &mut self,
            handle: api::ImageHandle,
            rect: api::RectF,
            slice: api::Insets,
            alpha: f32,
        ) {
            let Some(image) = self.image(handle) else {
                return;
            };
            let iw = image.width as f32;
            let ih = image.height as f32;
            self.stats.nine_slice_draws = self.stats.nine_slice_draws.saturating_add(1);
            let left = slice.left.clamp(0.0, iw);
            let right = slice.right.clamp(0.0, iw - left);
            let top = slice.top.clamp(0.0, ih);
            let bottom = slice.bottom.clamp(0.0, ih - top);
            let dx = [rect.x, rect.x + left, rect.x + (rect.w - right).max(left), rect.x + rect.w];
            let dy = [rect.y, rect.y + top, rect.y + (rect.h - bottom).max(top), rect.y + rect.h];
            let sx = [0.0, left, iw - right, iw];
            let sy = [0.0, top, ih - bottom, ih];

            for row in 0..3 {
                for col in 0..3 {
                    let dst = api::RectF::new(
                        dx[col],
                        dy[row],
                        dx[col + 1] - dx[col],
                        dy[row + 1] - dy[row],
                    );
                    let src = api::RectF::new(
                        sx[col],
                        sy[row],
                        sx[col + 1] - sx[col],
                        sy[row + 1] - sy[row],
                    );
                    self.draw_image_rect(handle, dst, src, alpha);
                }
            }
        }

        fn draw_rrect_path(&mut self, rect: api::RectF, radii: [f32; 4], color: api::Color) {
            if rect.w <= 0.0 || rect.h <= 0.0 {
                return;
            }
            let css = color_to_css(color);
            self.ctx.set_fill_style_str(&css);
            rounded_rect_path(&self.ctx, rect, radii);
            self.ctx.fill();
            self.stats.draws = self.stats.draws.saturating_add(1);
        }

        fn draw_backdrop_fallback(
            &mut self,
            rect: api::RectF,
            sigma: f32,
            tint: api::Color,
            alpha: f32,
        ) {
            if rect.w <= 0.0 || rect.h <= 0.0 {
                return;
            }
            let _ = self.draw_sampled_backdrop(rect, sigma);
            self.draw_tint_rect(
                rect,
                api::Color::rgba(tint.r, tint.g, tint.b, tint.a * alpha.clamp(0.0, 1.0)),
            );
        }

        fn draw_sampled_backdrop(&mut self, rect: api::RectF, sigma: f32) -> Option<()> {
            let scale = sanitize_scale(self.scale);
            let width = layer_physical_dimension(rect.w, scale);
            let height = layer_physical_dimension(rect.h, scale);
            let source_canvas = self.current_surface_canvas();
            let origin = self.current_surface_origin();
            let sx = ((rect.x - origin.0) * scale) as f64;
            let sy = ((rect.y - origin.1) * scale) as f64;
            if sx < 0.0 || sy < 0.0 {
                return None;
            }

            let sample = create_canvas(width, height).ok()?;
            let sample_ctx = canvas_context(&sample).ok()?;
            sample_ctx
                .draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                    &source_canvas,
                    sx,
                    sy,
                    width as f64,
                    height as f64,
                    0.0,
                    0.0,
                    width as f64,
                    height as f64,
                )
                .ok()?;

            self.ctx.save();
            if sigma.is_finite() && sigma > 0.0 {
                let filter = format!("blur({:.1}px)", sigma.min(96.0));
                self.ctx.set_filter(&filter);
            }
            let result = self.ctx.draw_image_with_html_canvas_element_and_dw_and_dh(
                &sample,
                rect.x as f64,
                rect.y as f64,
                rect.w as f64,
                rect.h as f64,
            );
            self.ctx.restore();
            result.ok()?;
            self.stats.draws = self.stats.draws.saturating_add(1);
            Some(())
        }

        fn draw_tint_rect(&mut self, rect: api::RectF, tint: api::Color) {
            if tint.a <= 0.0 {
                return;
            }
            self.ctx.set_fill_style_str(&color_to_css(tint));
            self.ctx.fill_rect(rect.x as f64, rect.y as f64, rect.w as f64, rect.h as f64);
            self.stats.draws = self.stats.draws.saturating_add(1);
        }

        fn current_surface_canvas(&self) -> HtmlCanvasElement {
            self.layer_stack
                .last()
                .map(|frame| frame.canvas.clone())
                .unwrap_or_else(|| self.canvas.clone())
        }

        fn current_surface_origin(&self) -> (f32, f32) {
            self.layer_stack.last().map(|frame| (frame.rect.x, frame.rect.y)).unwrap_or((0.0, 0.0))
        }

        fn draw_spinner_shape(&mut self, center: [f32; 2], atom: f32, alpha: f32) {
            let radius = (atom * 1.5).max(1.0);
            for idx in 0..12 {
                let t = idx as f32 / 12.0;
                let angle = t * core::f32::consts::TAU;
                let a = alpha.clamp(0.0, 1.0) * (0.25 + t * 0.75);
                let color = api::Color::rgba(0.15, 0.15, 0.15, a);
                let css = color_to_css(color);
                let x = center[0] + angle.cos() * radius;
                let y = center[1] + angle.sin() * radius;
                self.ctx.begin_path();
                self.ctx.set_fill_style_str(&css);
                let _ = self.ctx.arc(
                    x as f64,
                    y as f64,
                    (atom * 0.22).max(1.0) as f64,
                    0.0,
                    core::f64::consts::TAU,
                );
                self.ctx.fill();
                self.stats.draws = self.stats.draws.saturating_add(1);
            }
        }
    }

    impl api::Renderer for WebRenderer {
        fn device_caps(&self) -> api::DeviceCaps {
            api::DeviceCaps {
                max_framerate_hz: 120,
                supports_edr: false,
                supports_msaa4x: false,
                native_scale: self.scale,
            }
        }

        fn begin_frame(
            &mut self,
            _fb: &api::FrameTarget,
            damage: Option<&api::Damage>,
        ) -> api::FrameToken {
            self.discard_open_layers();
            self.reset_clip_stack();
            self.frame_id = self.frame_id.wrapping_add(1);
            self.stats = WebRendererStats {
                frame_id: self.frame_id,
                width: self.width,
                height: self.height,
                scale: self.scale,
                damage_rects: damage.map(|d| d.rects.len() as u32).unwrap_or(0),
                ..WebRendererStats::default()
            };
            let _ = self.ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
            self.ctx.clear_rect(0.0, 0.0, self.width as f64, self.height as f64);
            let scale = sanitize_scale(self.scale) as f64;
            let _ = self.ctx.set_transform(scale, 0.0, 0.0, scale, 0.0, 0.0);
            let token = api::FrameToken(self.frame_id);
            self.active_token = Some(token);
            token
        }

        fn encode_pass(&mut self, list: &api::DrawList) {
            let mut index = 0;
            self.encode_items(list, &mut index, false);
            self.finish_open_layers();
        }

        fn submit(&mut self, token: api::FrameToken) -> Result<(), api::RenderError> {
            self.finish_open_layers();
            self.reset_clip_stack();
            if self.active_token != Some(token) {
                return Err(api::RenderError::InvalidOperation("frame token mismatch"));
            }
            self.active_token = None;
            Ok(())
        }

        fn resize(&mut self, width: u32, height: u32, scale: f32) -> Result<(), api::RenderError> {
            let scale = sanitize_scale(scale);
            self.width = width.max(1);
            self.height = height.max(1);
            self.scale = scale;
            self.canvas.set_width(self.width);
            self.canvas.set_height(self.height);
            let style = self.canvas.style();
            let css_w = format!("{}px", logical_dimension(self.width, scale).round().max(1.0));
            let css_h = format!("{}px", logical_dimension(self.height, scale).round().max(1.0));
            style
                .set_property("width", &css_w)
                .map_err(|err| js_error("canvas style width", err))?;
            style
                .set_property("height", &css_h)
                .map_err(|err| js_error("canvas style height", err))?;
            let s = scale as f64;
            self.ctx
                .set_transform(s, 0.0, 0.0, s, 0.0, 0.0)
                .map_err(|err| js_error("canvas transform", err))?;
            Ok(())
        }
    }

    impl api::RenderEncoder for WebRenderer {
        fn set_viewport(&mut self, _vp: api::RectF) {}

        fn set_clip(&mut self, scissor: api::RectI) {
            self.reset_clip_stack();
            self.push_clip(scissor);
        }

        fn draw_solid(&mut self, verts: &[api::Vertex], color: api::Color) {
            let css = color_to_css(color);
            self.ctx.set_fill_style_str(&css);
            if verts.len() == 4 {
                self.fill_triangle(verts[0], verts[1], verts[2]);
                self.fill_triangle(verts[2], verts[1], verts[3]);
            } else {
                for tri in verts.chunks_exact(3) {
                    self.fill_triangle(tri[0], tri[1], tri[2]);
                }
            }
        }

        fn draw_image(&mut self, img: api::ImageHandle, dst: api::RectF, src: api::RectF) {
            self.draw_image_rect(img, dst, src, 1.0);
        }

        fn draw_rrect(&mut self, rect: api::RectF, radii: [f32; 4], color: api::Color) {
            self.draw_rrect_path(rect, radii, color);
        }

        fn draw_nine_slice(
            &mut self,
            img: api::ImageHandle,
            rect: api::RectF,
            slice: api::Insets,
            alpha: f32,
        ) {
            self.draw_nine_slice(img, rect, slice, alpha);
        }

        fn draw_backdrop(&mut self, rect: api::RectF, sigma: f32, tint: api::Color, alpha: f32) {
            self.draw_backdrop_fallback(rect, sigma, tint, alpha);
        }

        fn draw_spinner(&mut self, center: [f32; 2], atom: f32, alpha: f32) {
            self.draw_spinner_shape(center, atom, alpha);
        }

        fn draw_glyph_run(&mut self, _run: &api::GlyphRun) {}

        fn draw_glyph_run_resolved(
            &mut self,
            run: &api::GlyphRun,
            vertices: &[api::Vertex],
            indices: &[u16],
        ) {
            self.draw_glyph_run_vertices(run, vertices, indices);
        }
    }

    struct CanvasAllocationSummary {
        alloc_count: u64,
        alloc_bytes: u64,
        dealloc_count: u64,
        dealloc_bytes: u64,
        realloc_count: u64,
        realloc_grow_bytes: u64,
        realloc_shrink_bytes: u64,
        allocating_frames: u64,
        peak_frame_alloc_bytes: u64,
    }

    /// Runs the non-default Canvas2D indexed-quad diagnostic workload on the supplied canvas.
    pub fn bench_canvas_indexed_quads(
        canvas: HtmlCanvasElement,
        samples: u32,
        frames_per_sample: u32,
        quads: u32,
    ) -> Result<String, api::RenderError> {
        let sample_count = samples.clamp(1, 30);
        let frames = frames_per_sample.clamp(1, 120);
        let quad_count = quads.clamp(1, 4096);
        let mut renderer = WebRenderer::from_canvas(canvas)?;
        renderer.resize(512, 512, 1.0)?;
        let image = {
            let pixels = canvas_checker_rgba(16, 16);
            renderer.image_create_rgba8(16, 16, &pixels, 16 * 4)
        };
        let draw_list = canvas_indexed_quad_draw_list(image, quad_count);
        let damage = api::Damage { rects: vec![api::RectI::new(0, 0, 512, 512)] };
        for _warmup in 0..4 {
            let token = renderer.begin_frame(&api::FrameTarget, Some(&damage));
            renderer.encode_pass(&draw_list);
            renderer.submit(token)?;
        }

        let mut values = Vec::with_capacity(sample_count.saturating_mul(frames) as usize);
        let mut allocations = CanvasAllocationSummary {
            alloc_count: 0,
            alloc_bytes: 0,
            dealloc_count: 0,
            dealloc_bytes: 0,
            realloc_count: 0,
            realloc_grow_bytes: 0,
            realloc_shrink_bytes: 0,
            allocating_frames: 0,
            peak_frame_alloc_bytes: 0,
        };
        for _sample in 0..sample_count {
            for _frame in 0..frames {
                let start = perf_now();
                let alloc_before = oxide_wasm_alloc_counter::snapshot();
                let token = renderer.begin_frame(&api::FrameTarget, Some(&damage));
                renderer.encode_pass(&draw_list);
                renderer.submit(token)?;
                let alloc_after = oxide_wasm_alloc_counter::snapshot();
                add_canvas_allocation_frame(&mut allocations, alloc_before, alloc_after);
                values.push((perf_now() - start).max(0.0));
            }
        }

        values.sort_by(|a, b| a.total_cmp(b));
        let total_frames = sample_count.saturating_mul(frames);
        let avg_ms = average(&values);
        let p50_ms = percentile(&values, 0.50);
        let p95_ms = percentile(&values, 0.95);
        let p99_ms = percentile(&values, 0.99);
        let peak_ms = values.last().copied().unwrap_or(0.0);
        let stats = canvas_stats_metrics(renderer.last_stats());
        let pacing = frame_pacing_metrics(&values);
        let allocations = canvas_allocation_metrics(&allocations);
        Ok(format!(
            "samples={sample_count};frames_per_sample={frames};frames={total_frames};p50_ms={p50_ms:.3};p95_ms={p95_ms:.3};p99_ms={p99_ms:.3};peak_ms={peak_ms:.3};avg_ms={avg_ms:.3};quads={quad_count};expected_image_meshes=1;expected_image_draws={quad_count}{stats}{pacing}{allocations}",
        ))
    }

    fn canvas_checker_rgba(width: u32, height: u32) -> Vec<u8> {
        let mut rgba =
            vec![0_u8; (width as usize).saturating_mul(height as usize).saturating_mul(4)];
        for y in 0..height {
            for x in 0..width {
                let idx = ((y as usize).saturating_mul(width as usize).saturating_add(x as usize))
                    .saturating_mul(4);
                let tile = ((x / 8) + (y / 8)) % 2 == 0;
                let (r, g, b) =
                    if tile { (42_u8, 122_u8, 255_u8) } else { (245_u8, 248_u8, 252_u8) };
                rgba[idx] = r;
                rgba[idx + 1] = g;
                rgba[idx + 2] = b;
                rgba[idx + 3] = 255;
            }
        }
        rgba
    }

    fn canvas_indexed_quad_draw_list(tex: api::ImageHandle, quads: u32) -> api::DrawList {
        let quad_count = quads.clamp(1, 4096) as usize;
        let mut list = api::DrawList::default();
        list.vertices.reserve(quad_count.saturating_mul(4));
        list.indices.reserve(quad_count.saturating_mul(6));
        let columns = 32_usize;
        let tile = 8.0_f32;
        for index in 0..quad_count {
            let col = index % columns;
            let row = index / columns;
            let x = (col as f32) * tile;
            let y = (row as f32) * tile;
            let base = list.vertices.len() as u16;
            list.vertices.extend_from_slice(&[
                api::Vertex { x, y, u: 0.0, v: 0.0, rgba: 0xffff_ffff },
                api::Vertex { x: x + tile, y, u: 1.0, v: 0.0, rgba: 0xffff_ffff },
                api::Vertex { x, y: y + tile, u: 0.0, v: 1.0, rgba: 0xffff_ffff },
                api::Vertex { x: x + tile, y: y + tile, u: 1.0, v: 1.0, rgba: 0xffff_ffff },
            ]);
            list.indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
        }
        list.items.push(api::DrawCmd::ImageMesh {
            tex,
            vb: api::VertexSpan { offset: 0, len: list.vertices.len() as u32 },
            ib: api::IndexSpan { offset: 0, len: list.indices.len() as u32 },
            alpha: 1.0,
        });
        list
    }

    fn add_canvas_allocation_frame(
        summary: &mut CanvasAllocationSummary,
        before: oxide_wasm_alloc_counter::AllocationSnapshot,
        after: oxide_wasm_alloc_counter::AllocationSnapshot,
    ) {
        let alloc_count = after.alloc_count.saturating_sub(before.alloc_count);
        let alloc_bytes = after.alloc_bytes.saturating_sub(before.alloc_bytes);
        let dealloc_count = after.dealloc_count.saturating_sub(before.dealloc_count);
        let dealloc_bytes = after.dealloc_bytes.saturating_sub(before.dealloc_bytes);
        let realloc_count = after.realloc_count.saturating_sub(before.realloc_count);
        let realloc_grow_bytes =
            after.realloc_grow_bytes.saturating_sub(before.realloc_grow_bytes);
        let realloc_shrink_bytes =
            after.realloc_shrink_bytes.saturating_sub(before.realloc_shrink_bytes);
        summary.alloc_count = summary.alloc_count.saturating_add(alloc_count);
        summary.alloc_bytes = summary.alloc_bytes.saturating_add(alloc_bytes);
        summary.dealloc_count = summary.dealloc_count.saturating_add(dealloc_count);
        summary.dealloc_bytes = summary.dealloc_bytes.saturating_add(dealloc_bytes);
        summary.realloc_count = summary.realloc_count.saturating_add(realloc_count);
        summary.realloc_grow_bytes =
            summary.realloc_grow_bytes.saturating_add(realloc_grow_bytes);
        summary.realloc_shrink_bytes =
            summary.realloc_shrink_bytes.saturating_add(realloc_shrink_bytes);
        let frame_alloc_bytes = alloc_bytes.saturating_add(realloc_grow_bytes);
        if alloc_count > 0 || realloc_count > 0 {
            summary.allocating_frames = summary.allocating_frames.saturating_add(1);
        }
        summary.peak_frame_alloc_bytes = summary.peak_frame_alloc_bytes.max(frame_alloc_bytes);
    }

    fn canvas_allocation_metrics(summary: &CanvasAllocationSummary) -> String {
        format!(
            ";wasm_alloc_count={};wasm_alloc_bytes={};wasm_dealloc_count={};wasm_dealloc_bytes={};wasm_realloc_count={};wasm_realloc_grow_bytes={};wasm_realloc_shrink_bytes={};wasm_allocating_frames={};wasm_peak_frame_alloc_bytes={}",
            summary.alloc_count,
            summary.alloc_bytes,
            summary.dealloc_count,
            summary.dealloc_bytes,
            summary.realloc_count,
            summary.realloc_grow_bytes,
            summary.realloc_shrink_bytes,
            summary.allocating_frames,
            summary.peak_frame_alloc_bytes,
        )
    }

    fn frame_pacing_metrics(frame_values_ms: &[f64]) -> String {
        let mut out = String::new();
        let denom = frame_values_ms.len().max(1) as f64;
        for refresh_hz in [60_u32, 120_u32] {
            let budget_ms = 1000.0 / refresh_hz as f64;
            let missed_frames =
                frame_values_ms.iter().filter(|sample| **sample > budget_ms).count();
            let hitch_frames =
                frame_values_ms.iter().filter(|sample| **sample > budget_ms * 2.0).count();
            let _ = write!(
                out,
                ";frame_budget_{refresh_hz}hz_ms={budget_ms:.6};missed_frames_{refresh_hz}hz={missed_frames};missed_frame_ratio_{refresh_hz}hz={:.6};hitch_frames_{refresh_hz}hz={hitch_frames};hitch_ratio_{refresh_hz}hz={:.6}",
                missed_frames as f64 / denom,
                hitch_frames as f64 / denom,
            );
        }
        out
    }

    fn canvas_stats_metrics(stats: WebRendererStats) -> String {
        format!(
            ";draws={};draw_items={};draw_items_coalesced={};draw_pipeline_binds={};draw_bind_group_binds={};draw_scissor_sets={};solid_tris={};rrect_instances={};rrect_triangles={};rrect_instance_bytes={};image_instances={};image_triangles={};image_instance_bytes={};image_draws={};image_mesh_draws={};nine_slice_draws={};nine_slice_instances={};nine_slice_triangles={};nine_slice_instance_bytes={};glyph_quads={};sdf_glyph_quads={};clip_depth_peak={};damage_rects={};render_passes={};clear_passes={};draw_passes={};present_passes={};texture_copies={};command_buffers={};id_mask_uniform_writes={};id_mask_uniform_bytes={};id_mask_uniform_slots={};spinner_instances={};spinner_triangles={};spinner_instance_bytes={};neon_marker_instances={};neon_marker_triangles={};neon_marker_instance_bytes={};buffer_upload_bytes={};property_upload_bytes={};property_records_updated={};property_ring_bytes={};texture_upload_bytes={};buffer_grows={};texture_creates={};bind_group_creates={};pipeline_creates={};sampler_creates={};image_texture_creates={};image_bind_group_creates={};cpu_scratch_bytes={};cpu_scratch_grows={};cpu_scratch_growth_bytes={}",
            stats.draws,
            stats.draw_items,
            stats.draw_items_coalesced,
            stats.draw_pipeline_binds,
            stats.draw_bind_group_binds,
            stats.draw_scissor_sets,
            stats.solid_tris,
            stats.rrect_instances,
            stats.rrect_triangles,
            stats.rrect_instance_bytes,
            stats.image_instances,
            stats.image_triangles,
            stats.image_instance_bytes,
            stats.image_draws,
            stats.image_mesh_draws,
            stats.nine_slice_draws,
            stats.nine_slice_instances,
            stats.nine_slice_triangles,
            stats.nine_slice_instance_bytes,
            stats.glyph_quads,
            stats.sdf_glyph_quads,
            stats.clip_depth_peak,
            stats.damage_rects,
            stats.render_passes,
            stats.clear_passes,
            stats.draw_passes,
            stats.present_passes,
            stats.texture_copies,
            stats.command_buffers,
            stats.id_mask_uniform_writes,
            stats.id_mask_uniform_bytes,
            stats.id_mask_uniform_slots,
            stats.spinner_instances,
            stats.spinner_triangles,
            stats.spinner_instance_bytes,
            stats.neon_marker_instances,
            stats.neon_marker_triangles,
            stats.neon_marker_instance_bytes,
            stats.buffer_upload_bytes,
            stats.property_upload_bytes,
            stats.property_records_updated,
            stats.property_ring_bytes,
            stats.texture_upload_bytes,
            stats.buffer_grows,
            stats.texture_creates,
            stats.bind_group_creates,
            stats.pipeline_creates,
            stats.sampler_creates,
            stats.image_texture_creates,
            stats.image_bind_group_creates,
            stats.cpu_scratch_bytes,
            stats.cpu_scratch_grows,
            stats.cpu_scratch_growth_bytes,
        )
    }

    fn average(values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        values.iter().copied().sum::<f64>() / values.len() as f64
    }

    fn percentile(sorted_values: &[f64], percentile: f64) -> f64 {
        if sorted_values.is_empty() {
            return 0.0;
        }
        let index = ((sorted_values.len().saturating_sub(1)) as f64 * percentile)
            .ceil()
            .clamp(0.0, sorted_values.len().saturating_sub(1) as f64) as usize;
        sorted_values[index]
    }

    fn perf_now() -> f64 {
        web_sys::window()
            .and_then(|window| window.performance())
            .map(|perf| perf.now())
            .unwrap_or(0.0)
    }

    fn document() -> Result<Document, api::RenderError> {
        let Some(window) = web_sys::window() else {
            return Err(api::RenderError::Unsupported("window unavailable"));
        };
        window.document().ok_or(api::RenderError::Unsupported("document unavailable"))
    }

    fn canvas_context(
        canvas: &HtmlCanvasElement,
    ) -> Result<CanvasRenderingContext2d, api::RenderError> {
        let value = canvas
            .get_context("2d")
            .map_err(|err| js_error("get canvas context", err))?
            .ok_or(api::RenderError::Unsupported("2d canvas context unavailable"))?;
        value.dyn_into::<CanvasRenderingContext2d>().map_err(|_| {
            api::RenderError::InvalidOperation("context is not CanvasRenderingContext2d")
        })
    }

    fn create_canvas(width: u32, height: u32) -> Result<HtmlCanvasElement, api::RenderError> {
        let canvas = document()?
            .create_element("canvas")
            .map_err(|err| js_error("create canvas", err))?
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| api::RenderError::InvalidOperation("created element is not a canvas"))?;
        canvas.set_width(width.max(1));
        canvas.set_height(height.max(1));
        Ok(canvas)
    }

    #[allow(dead_code)]
    fn canvas_from_rgba(
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<HtmlCanvasElement, api::RenderError> {
        let canvas = create_canvas(width, height)?;
        let ctx = canvas_context(&canvas)?;
        let image_data = ImageData::new_with_u8_clamped_array_and_sh(Clamped(rgba), width, height)
            .map_err(|err| js_error("image data", err))?;
        ctx.put_image_data(&image_data, 0.0, 0.0).map_err(|err| js_error("put image data", err))?;
        Ok(canvas)
    }

    fn tinted_canvas(
        mask: &HtmlCanvasElement,
        width: u32,
        height: u32,
        color: api::Color,
    ) -> Result<HtmlCanvasElement, api::RenderError> {
        let canvas = create_canvas(width, height)?;
        let ctx = canvas_context(&canvas)?;
        ctx.draw_image_with_html_canvas_element(mask, 0.0, 0.0)
            .map_err(|err| js_error("draw mask", err))?;
        ctx.set_global_composite_operation("source-in")
            .map_err(|err| js_error("composite source-in", err))?;
        let css = color_to_css(api::Color::rgba(color.r, color.g, color.b, 1.0));
        ctx.set_fill_style_str(&css);
        ctx.fill_rect(0.0, 0.0, width as f64, height as f64);
        ctx.set_global_composite_operation("source-over")
            .map_err(|err| js_error("composite source-over", err))?;
        Ok(canvas)
    }

    fn js_error(stage: &'static str, err: JsValue) -> api::RenderError {
        let message = err.as_string().unwrap_or_else(|| format!("{err:?}"));
        api::RenderError::Io(format!("{stage}: {message}"))
    }

    fn vertex_slice(list: &api::DrawList, span: api::VertexSpan) -> Option<&[api::Vertex]> {
        let start = span.offset as usize;
        let len = span.len as usize;
        let end = start.checked_add(len)?;
        list.vertices.get(start..end)
    }

    fn index_slice(list: &api::DrawList, span: api::IndexSpan) -> Option<&[u16]> {
        let start = span.offset as usize;
        let len = span.len as usize;
        let end = start.checked_add(len)?;
        list.indices.get(start..end)
    }

    fn layer_dimensions(rect: api::RectF, scale: f32) -> (u32, u32) {
        (layer_physical_dimension(rect.w, scale), layer_physical_dimension(rect.h, scale))
    }

    fn skip_layer_body(list: &api::DrawList, index: &mut usize) {
        let mut depth = 1_u32;
        while *index < list.items.len() && depth > 0 {
            match list.items[*index] {
                api::DrawCmd::LayerBegin { .. } => depth = depth.saturating_add(1),
                api::DrawCmd::LayerEnd => depth = depth.saturating_sub(1),
                _ => {}
            }
            *index += 1;
        }
    }

    fn source_rect(src: api::RectF, width: u32, height: u32) -> (f64, f64, f64, f64) {
        if src.w > 0.0 && src.h > 0.0 {
            (
                src.x.clamp(0.0, width as f32) as f64,
                src.y.clamp(0.0, height as f32) as f64,
                src.w.clamp(0.0, width as f32) as f64,
                src.h.clamp(0.0, height as f32) as f64,
            )
        } else {
            (0.0, 0.0, width as f64, height as f64)
        }
    }

    fn draw_vertex_quads<F>(
        vertices: &[api::Vertex],
        indices: &[u16],
        mode: Option<NormalizedIndexMode>,
        mut draw: F,
    ) where
        F: FnMut(&[api::Vertex]),
    {
        let Some(mode) = mode else {
            vertices.chunks_exact(4).for_each(draw);
            return;
        };
        for quad_indices in indices.chunks_exact(6) {
            let mut quad = [api::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0 }; 6];
            let mut valid = true;
            for (dst, index) in quad.iter_mut().zip(quad_indices.iter().copied()) {
                let Some(vertex) =
                    resolve_index(index, mode).and_then(|idx| vertices.get(idx)).copied()
                else {
                    valid = false;
                    break;
                };
                *dst = vertex;
            }
            if valid {
                draw(&quad);
            }
        }
    }

    fn quad_rect(vertices: &[api::Vertex]) -> api::RectF {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for vertex in vertices {
            min_x = min_x.min(vertex.x);
            min_y = min_y.min(vertex.y);
            max_x = max_x.max(vertex.x);
            max_y = max_y.max(vertex.y);
        }
        api::RectF::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }

    fn uv_rect(vertices: &[api::Vertex], width: u32, height: u32) -> api::RectF {
        let mut min_u = f32::INFINITY;
        let mut min_v = f32::INFINITY;
        let mut max_u = f32::NEG_INFINITY;
        let mut max_v = f32::NEG_INFINITY;
        for vertex in vertices {
            min_u = min_u.min(vertex.u);
            min_v = min_v.min(vertex.v);
            max_u = max_u.max(vertex.u);
            max_v = max_v.max(vertex.v);
        }
        api::RectF::new(
            min_u.clamp(0.0, 1.0) * width as f32,
            min_v.clamp(0.0, 1.0) * height as f32,
            (max_u - min_u).clamp(0.0, 1.0) * width as f32,
            (max_v - min_v).clamp(0.0, 1.0) * height as f32,
        )
    }

    fn rounded_rect_path(ctx: &CanvasRenderingContext2d, rect: api::RectF, radii: [f32; 4]) {
        let max_r = (rect.w.abs() * 0.5).min(rect.h.abs() * 0.5);
        let tl = radii[0].clamp(0.0, max_r);
        let tr = radii[1].clamp(0.0, max_r);
        let br = radii[2].clamp(0.0, max_r);
        let bl = radii[3].clamp(0.0, max_r);
        let x = rect.x;
        let y = rect.y;
        let r = rect.x + rect.w;
        let b = rect.y + rect.h;

        ctx.begin_path();
        ctx.move_to((x + tl) as f64, y as f64);
        ctx.line_to((r - tr) as f64, y as f64);
        ctx.quadratic_curve_to(r as f64, y as f64, r as f64, (y + tr) as f64);
        ctx.line_to(r as f64, (b - br) as f64);
        ctx.quadratic_curve_to(r as f64, b as f64, (r - br) as f64, b as f64);
        ctx.line_to((x + bl) as f64, b as f64);
        ctx.quadratic_curve_to(x as f64, b as f64, x as f64, (b - bl) as f64);
        ctx.line_to(x as f64, (y + tl) as f64);
        ctx.quadratic_curve_to(x as f64, y as f64, (x + tl) as f64, y as f64);
        ctx.close_path();
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native_stub {
    use super::{sanitize_scale, WebRendererStats};
    use oxide_renderer_api as api;

    /// Native placeholder so non-wasm workspace checks can compile the web crate.
    pub struct WebRenderer {
        width: u32,
        height: u32,
        scale: f32,
        frame_id: u64,
        stats: WebRendererStats,
    }

    impl WebRenderer {
        #[must_use]
        pub fn new_for_tests(width: u32, height: u32, scale: f32) -> Self {
            Self {
                width,
                height,
                scale: sanitize_scale(scale),
                frame_id: 0,
                stats: WebRendererStats::default(),
            }
        }

        #[must_use]
        pub fn last_stats(&self) -> WebRendererStats {
            self.stats
        }
    }

    impl api::Renderer for WebRenderer {
        fn device_caps(&self) -> api::DeviceCaps {
            api::DeviceCaps {
                max_framerate_hz: 120,
                supports_edr: false,
                supports_msaa4x: false,
                native_scale: self.scale,
            }
        }

        fn begin_frame(
            &mut self,
            _fb: &api::FrameTarget,
            damage: Option<&api::Damage>,
        ) -> api::FrameToken {
            self.frame_id = self.frame_id.wrapping_add(1);
            self.stats = WebRendererStats {
                frame_id: self.frame_id,
                width: self.width,
                height: self.height,
                scale: self.scale,
                damage_rects: damage.map(|d| d.rects.len() as u32).unwrap_or(0),
                ..WebRendererStats::default()
            };
            api::FrameToken(self.frame_id)
        }

        fn encode_pass(&mut self, list: &api::DrawList) {
            let draw_count = list
                .items
                .iter()
                .filter(|item| {
                    !matches!(
                        item,
                        api::DrawCmd::CameraBg { .. }
                            | api::DrawCmd::LayerBegin { .. }
                            | api::DrawCmd::LayerEnd
                            | api::DrawCmd::ClipPush { .. }
                            | api::DrawCmd::ClipPop
                    )
                })
                .count()
                .min(u32::MAX as usize) as u32;
            self.stats.draws = draw_count;
        }

        fn submit(&mut self, _token: api::FrameToken) -> Result<(), api::RenderError> {
            Err(api::RenderError::Unsupported("web renderer requires wasm32"))
        }

        fn resize(&mut self, width: u32, height: u32, scale: f32) -> Result<(), api::RenderError> {
            self.width = width;
            self.height = height;
            self.scale = sanitize_scale(scale);
            Ok(())
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native_stub::WebRenderer;
#[cfg(target_arch = "wasm32")]
pub use wasm::{bench_canvas_indexed_quads, BrowserRenderer, WebGpuRenderer};
#[cfg(all(target_arch = "wasm32", feature = "snapshot-tests"))]
pub use wasm::WebIdMaskSnapshotReadback;

#[cfg(test)]
mod tests {
    use super::{copy_a8_rows_into, copy_rgba_rows_into};

    #[test]
    fn a8_row_repack_preserves_only_visible_strided_bytes() {
        let mut scratch = Vec::new();
        let source = [1_u8, 2, 3, 90, 91, 4, 5, 6, 92, 93];
        assert_eq!(copy_a8_rows_into(&mut scratch, 3, 2, &source[..8], 5), Some(true));
        assert_eq!(scratch, [1, 2, 3, 4, 5, 6]);
        assert_eq!(copy_a8_rows_into(&mut scratch, 3, 2, &source, 5), Some(false));
    }

    #[test]
    fn rgba_row_repack_preserves_existing_four_channel_contract() {
        let mut scratch = Vec::new();
        let source = [
            1_u8, 2, 3, 4, 5, 6, 7, 8, 90, 91, 9, 10, 11, 12, 13, 14, 15, 16, 92, 93,
        ];
        assert_eq!(copy_rgba_rows_into(&mut scratch, 2, 2, &source, 10), Some(true));
        assert_eq!(scratch, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
    }
}
