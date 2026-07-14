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
    fields: FieldTextures,
}

#[derive(Clone)]
enum FieldTextures {
    Packed { a: Texture, b: Texture },
    Wide {
        city_a: Texture,
        city_b: Texture,
        seam_a: Texture,
        seam_b: Texture,
    },
}

#[derive(Clone, Copy)]
enum FieldPair<'a> {
    Packed(&'a TextureRef),
    Wide { city: &'a TextureRef, seam: &'a TextureRef },
}

impl RenderTargets {
    fn field_pair(&self, use_a: bool) -> FieldPair<'_> {
        match &self.fields {
            FieldTextures::Packed { a, b } => FieldPair::Packed(if use_a { a } else { b }),
            FieldTextures::Wide { city_a, city_b, seam_a, seam_b } => FieldPair::Wide {
                city: if use_a { city_a } else { city_b },
                seam: if use_a { seam_a } else { seam_b },
            },
        }
    }

    fn final_fields(&self) -> FieldPair<'_> {
        let mut src_is_a = true;
        let mut jump = self.width.max(self.height).next_power_of_two() / 2;
        while jump >= 1 {
            src_is_a = !src_is_a;
            jump /= 2;
        }
        self.field_pair(src_is_a)
    }

    pub(super) fn field_texture_refs(&self) -> [Option<&TextureRef>; 4] {
        match &self.fields {
            FieldTextures::Packed { a, b } => [Some(a), Some(b), None, None],
            FieldTextures::Wide { city_a, city_b, seam_a, seam_b } => {
                [Some(city_a), Some(city_b), Some(seam_a), Some(seam_b)]
            }
        }
    }

    #[cfg(feature = "snapshot-tests")]
    fn packed_fields(&self) -> bool {
        matches!(self.fields, FieldTextures::Packed { .. })
    }
}

#[inline]
const fn packed_field_coordinates_fit(width: usize, height: usize) -> bool {
    width <= u16::MAX as usize && height <= u16::MAX as usize
}

const _: () = {
    assert!(packed_field_coordinates_fit(1, 1));
    assert!(packed_field_coordinates_fit(u16::MAX as usize, u16::MAX as usize));
    assert!(!packed_field_coordinates_fit(u16::MAX as usize + 1, 1));
    assert!(!packed_field_coordinates_fit(1, u16::MAX as usize + 1));
};

#[derive(Clone, Copy, PartialEq, Eq)]
struct IdMaskProjectionKey {
    world_to_clip: [[u32; 4]; 4],
    model_to_world: [[u32; 4]; 4],
    camera_eye_unit: [u32; 3],
    normal_scale: [u32; 3],
    visible_front_min: u32,
    use_world_position: bool,
    visible_hemisphere: bool,
}

impl From<id_mask_compositor::IdMaskRasterProjection> for IdMaskProjectionKey {
    fn from(projection: id_mask_compositor::IdMaskRasterProjection) -> Self {
        Self {
            world_to_clip: projection.world_to_clip.map(|row| row.map(f32::to_bits)),
            model_to_world: projection.model_to_world.map(|row| row.map(f32::to_bits)),
            camera_eye_unit: projection.camera_eye_unit.map(f32::to_bits),
            normal_scale: projection.normal_scale.map(f32::to_bits),
            visible_front_min: projection.visible_front_min.to_bits(),
            use_world_position: projection.use_world_position,
            visible_hemisphere: projection.visible_hemisphere,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct IdMaskChunkKey {
    content_hash: u64,
    first_vertex: usize,
    vertex_count: usize,
}

impl From<&id_mask_compositor::IdMaskRasterChunk> for IdMaskChunkKey {
    fn from(chunk: &id_mask_compositor::IdMaskRasterChunk) -> Self {
        Self {
            content_hash: chunk.content_hash,
            first_vertex: chunk.first_vertex,
            vertex_count: chunk.vertex_count,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct IdMaskFieldCacheKey {
    mask_width: usize,
    mask_height: usize,
    mask_scale: u32,
    vertex_revision: u64,
    vertex_count: usize,
    projection: IdMaskProjectionKey,
}

impl IdMaskFieldCacheKey {
    fn new(raster: &id_mask_compositor::IdMaskGpuRasterPass<'_>) -> Self {
        Self {
            mask_width: raster.mask_width,
            mask_height: raster.mask_height,
            mask_scale: raster.mask_scale.to_bits(),
            vertex_revision: raster.vertex_revision,
            vertex_count: raster.vertices.len(),
            projection: raster.projection.into(),
        }
    }
}

pub(super) struct IdMaskFieldCacheEntry {
    key: IdMaskFieldCacheKey,
    chunks: Vec<IdMaskChunkKey>,
    pub(super) targets: RenderTargets,
    pub(super) bytes: u64,
    pub(super) last_used_frame: u64,
    pub(super) serial: u64,
}

#[derive(Clone, Copy)]
pub(super) struct IdMaskInFlightGeneration {
    pub(super) serial: u64,
    pub(super) bytes: u64,
}

impl IdMaskFieldCacheEntry {
    fn matches(&self, key: IdMaskFieldCacheKey, chunks: &[id_mask_compositor::IdMaskRasterChunk]) -> bool {
        self.key == key
            && self.chunks.len() == chunks.len()
            && self.chunks.iter().zip(chunks).all(|(cached, current)| {
                *cached == IdMaskChunkKey::from(current)
            })
    }
}

#[cfg(feature = "snapshot-tests")]
#[derive(Clone, Debug, PartialEq)]
pub struct IdMaskSnapshotReadback
{
   pub width: usize,
   pub height: usize,
   pub city: Vec<u8>,
   pub neighborhood: Vec<u8>,
   pub city_field: Vec<[f32; 4]>,
   pub seam_field: Vec<[f32; 4]>,
   pub packed_fields: bool,
   pub field_logical_bytes: u64,
   pub wide_field_logical_bytes: u64,
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

#[inline]
fn configure_clear_store_fields(rpd: &RenderPassDescriptorRef, fields: FieldPair<'_>) {
    match fields {
        FieldPair::Packed(field) => {
            let ca = rpd.color_attachments().object_at(0).unwrap();
            ca.set_texture(Some(field));
            ca.set_load_action(MTLLoadAction::Clear);
            ca.set_clear_color(MTLClearColor {
                red: u16::MAX as f64,
                green: u16::MAX as f64,
                blue: u16::MAX as f64,
                alpha: u16::MAX as f64,
            });
            ca.set_store_action(MTLStoreAction::Store);
        }
        FieldPair::Wide { city, seam } => configure_clear_store_attachments(rpd, city, seam),
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
    build_compositor_variant_pso(
        device,
        lib,
        fmt,
        "f_id_mask_compositor",
        "pso.id_mask_compositor.create",
    )
}

pub(super) fn build_compositor_wide_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    build_compositor_variant_pso(
        device,
        lib,
        fmt,
        "f_id_mask_compositor_wide",
        "pso.id_mask_compositor_wide.create",
    )
}

fn build_compositor_variant_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
    fragment_name: &str,
    label: &str,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "id_mask_compositor.vertex", "v_id_mask_compositor")?;
    let f = pipeline_function(lib, "id_mask_compositor.fragment", fragment_name)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(1);
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    configure_source_alpha_blend(ca);
    pipeline_state(device, label, &desc)
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
    build_field_pso(
        device,
        lib,
        "f_id_mask_field_seed",
        "pso.id_mask_field_seed.create",
        MTLPixelFormat::RGBA16Uint,
        1,
    )
}

pub(super) fn build_field_jump_pso(
    device: &Device,
    lib: &Library,
) -> Result<RenderPipelineState, MetalInitError> {
    build_field_pso(
        device,
        lib,
        "f_id_mask_field_jump",
        "pso.id_mask_field_jump.create",
        MTLPixelFormat::RGBA16Uint,
        1,
    )
}

pub(super) fn build_field_seed_wide_pso(
    device: &Device,
    lib: &Library,
) -> Result<RenderPipelineState, MetalInitError> {
    build_field_pso(
        device,
        lib,
        "f_id_mask_field_seed_wide",
        "pso.id_mask_field_seed_wide.create",
        MTLPixelFormat::RGBA32Float,
        2,
    )
}

pub(super) fn build_field_jump_wide_pso(
    device: &Device,
    lib: &Library,
) -> Result<RenderPipelineState, MetalInitError> {
    build_field_pso(
        device,
        lib,
        "f_id_mask_field_jump_wide",
        "pso.id_mask_field_jump_wide.create",
        MTLPixelFormat::RGBA32Float,
        2,
    )
}

fn build_field_pso(
    device: &Device,
    lib: &Library,
    fragment_name: &str,
    label: &str,
    format: MTLPixelFormat,
    attachment_count: u64,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = pipeline_function(lib, "id_mask_field.vertex", "v_id_mask_field")?;
    let f = pipeline_function(lib, "id_mask_field.fragment", fragment_name)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    desc.set_sample_count(1);
    for index in 0..attachment_count {
        let ca = desc.color_attachments().object_at(index).unwrap();
        ca.set_pixel_format(format);
        ca.set_blending_enabled(false);
    }
    pipeline_state(device, label, &desc)
}

impl MetalRenderer {
    #[cfg(feature = "snapshot-tests")]
    pub fn readback_id_mask_snapshot(&self) -> Option<IdMaskSnapshotReadback>
    {
        let targets = self.id_mask_snapshot_target.as_ref()?;
        let (_, _, city) = self.readback_texture_bytes(&targets.city, 1)?;
        let (_, _, neighborhood) = self.readback_texture_bytes(&targets.neighborhood, 1)?;
        let (city_field, seam_field) = match targets.final_fields() {
            FieldPair::Packed(field) => {
                let (_, _, bytes) = self.readback_texture_bytes(field, 8)?;
                decode_rgba16_uint_fields(
                    &bytes,
                    &city,
                    &neighborhood,
                    targets.width,
                    targets.height,
                )
            }
            FieldPair::Wide { city, seam } => {
                let (_, _, city) = self.readback_texture_bytes(city, 16)?;
                let (_, _, seam) = self.readback_texture_bytes(seam, 16)?;
                (decode_rgba32_float(&city), decode_rgba32_float(&seam))
            }
        };
        let pixels = targets.width.saturating_mul(targets.height) as u64;
        Some(IdMaskSnapshotReadback {
            width: targets.width,
            height: targets.height,
            city,
            neighborhood,
            city_field,
            seam_field,
            packed_fields: targets.packed_fields(),
            field_logical_bytes: pixels.saturating_mul(if targets.packed_fields() { 16 } else { 64 }),
            wide_field_logical_bytes: pixels.saturating_mul(64),
        })
    }

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

    fn new_id_mask_render_targets(
        &mut self,
        width: usize,
        height: usize,
        reusable: Option<RenderTargets>,
    ) -> Result<RenderTargets, api::RenderError> {
        if let Some(targets) = reusable.filter(|targets| {
            targets.width == width && targets.height == height
        }) {
            return Ok(targets);
        }
        let fields = if packed_field_coordinates_fit(width, height) {
            FieldTextures::Packed {
                a: self.new_id_mask_field_texture(width, height, MTLPixelFormat::RGBA16Uint)?,
                b: self.new_id_mask_field_texture(width, height, MTLPixelFormat::RGBA16Uint)?,
            }
        } else {
            FieldTextures::Wide {
                city_a: self.new_id_mask_field_texture(width, height, MTLPixelFormat::RGBA32Float)?,
                city_b: self.new_id_mask_field_texture(width, height, MTLPixelFormat::RGBA32Float)?,
                seam_a: self.new_id_mask_field_texture(width, height, MTLPixelFormat::RGBA32Float)?,
                seam_b: self.new_id_mask_field_texture(width, height, MTLPixelFormat::RGBA32Float)?,
            }
        };
        let field_texture_count = if matches!(&fields, FieldTextures::Packed { .. }) { 2 } else { 4 };
        let targets = RenderTargets {
            width,
            height,
            city: self.new_r8_mask_render_texture(width, height)?,
            neighborhood: self.new_r8_mask_render_texture(width, height)?,
            fields,
        };
        self.acc_id_mask_target_creates = self.acc_id_mask_target_creates.saturating_add(1);
        self.acc_resource_creates = self.acc_resource_creates.saturating_add(2 + field_texture_count);
        Ok(targets)
    }

    fn id_mask_render_targets_bytes(targets: &RenderTargets) -> u64 {
        [Some(targets.city.as_ref()), Some(targets.neighborhood.as_ref())]
        .into_iter()
        .chain(targets.field_texture_refs())
        .flatten()
        .fold(0_u64, |total, texture| {
            total.saturating_add(Self::texture_allocated_bytes(texture))
        })
    }

    fn id_mask_render_targets_required_bytes(&self, width: usize, height: usize) -> u64 {
        let r8 = TextureDescriptor::new();
        r8.set_pixel_format(MTLPixelFormat::R8Uint);
        r8.set_texture_type(MTLTextureType::D2);
        r8.set_width(width as u64);
        r8.set_height(height as u64);
        r8.set_storage_mode(MTLStorageMode::Private);
        r8.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        let field = TextureDescriptor::new();
        let packed = packed_field_coordinates_fit(width, height);
        field.set_pixel_format(if packed {
            MTLPixelFormat::RGBA16Uint
        } else {
            MTLPixelFormat::RGBA32Float
        });
        field.set_texture_type(MTLTextureType::D2);
        field.set_width(width as u64);
        field.set_height(height as u64);
        field.set_storage_mode(MTLStorageMode::Private);
        field.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        (self.device.heap_texture_size_and_align(&r8).size as u64)
            .saturating_mul(2)
            .saturating_add(
                (self.device.heap_texture_size_and_align(&field).size as u64)
                    .saturating_mul(if packed { 2 } else { 4 }),
            )
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
            self.acc_chunks_reused = self.acc_chunks_reused.saturating_add(1);
            self.acc_backend_cache_hits = self.acc_backend_cache_hits.saturating_add(1);
            return Ok(index);
        }
        self.acc_chunks_rebuilt = self.acc_chunks_rebuilt.saturating_add(1);
        self.acc_backend_cache_misses = self.acc_backend_cache_misses.saturating_add(1);

        let buffer =
            self.device.new_buffer(byte_len.max(1) as u64, MTLResourceOptions::StorageModeShared);
        let vertex_ptr = buffer.contents().cast::<u8>();
        if vertex_ptr.is_null() {
            return Err(api::RenderError::OutOfMemory);
        }
        unsafe {
            core::ptr::copy_nonoverlapping(vertices.as_ptr() as *const u8, vertex_ptr, byte_len);
        }
        self.id_mask_vertex_caches.push(IdMaskVertexUploadCache { key, buffer });
        self.acc_resource_creates = self.acc_resource_creates.saturating_add(1);
        Ok(self.id_mask_vertex_caches.len() - 1)
    }

    fn new_id_mask_field_texture(
        &self,
        width: usize,
        height: usize,
        format: MTLPixelFormat,
    ) -> Result<Texture, api::RenderError> {
        if width == 0 || height == 0 {
            return Err(api::RenderError::InvalidOperation("id-mask field target has zero size"));
        }
        let desc = TextureDescriptor::new();
        desc.set_pixel_format(format);
        desc.set_texture_type(MTLTextureType::D2);
        desc.set_width(width as u64);
        desc.set_height(height as u64);
        desc.set_storage_mode(MTLStorageMode::Private);
        desc.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        Ok(self.device.new_texture(&desc))
    }

    fn id_mask_field_cache_hit(
        &mut self,
        key: IdMaskFieldCacheKey,
        chunks: &[id_mask_compositor::IdMaskRasterChunk],
    ) -> Option<RenderTargets> {
        let index = self
            .id_mask_field_cache
            .iter()
            .position(|entry| entry.matches(key, chunks))?;
        let entry = &mut self.id_mask_field_cache[index];
        entry.last_used_frame = self.frame_id;
        let serial = entry.serial;
        let bytes = entry.bytes;
        let targets = entry.targets.clone();
        if !self.id_mask_frame_cache_serials.contains(&serial) {
            self.id_mask_frame_cache_serials.push(serial);
        }
        self.acc_id_mask_cache_hits = self.acc_id_mask_cache_hits.saturating_add(1);
        self.acc_backend_cache_hits = self.acc_backend_cache_hits.saturating_add(1);
        self.retain_id_mask_in_flight_generation(serial, bytes);
        Some(targets)
    }

    fn retain_id_mask_field_cache_entry(
        &mut self,
        key: IdMaskFieldCacheKey,
        chunks: &[id_mask_compositor::IdMaskRasterChunk],
        targets: &RenderTargets,
    ) -> bool {
        let bytes = Self::id_mask_render_targets_bytes(targets);
        while self.id_mask_cache_resident_bytes.saturating_add(bytes)
            > self.id_mask_cache_budget_bytes
        {
            if self.evict_oldest_id_mask_cache_entry().is_none() {
                return false;
            }
        }
        if bytes > self.id_mask_cache_budget_bytes
            || self.id_mask_field_cache.len() >= ID_MASK_CACHE_MAX_ENTRIES
        {
            return false;
        }
        let serial = self.next_id_mask_generation_serial();
        self.id_mask_cache_resident_bytes =
            self.id_mask_cache_resident_bytes.saturating_add(bytes);
        self.id_mask_field_cache.push(IdMaskFieldCacheEntry {
            key,
            chunks: chunks.iter().map(IdMaskChunkKey::from).collect(),
            targets: targets.clone(),
            bytes,
            last_used_frame: self.frame_id,
            serial,
        });
        self.id_mask_frame_cache_serials.push(serial);
        self.retain_id_mask_in_flight_generation(serial, bytes);
        true
    }

    fn encode_id_mask_compositor_textures(
        &mut self,
        viewport: api::RectF,
        mask_width: usize,
        mask_height: usize,
        mask_scale: f32,
        city_tex: &TextureRef,
        neighborhood_tex: &TextureRef,
        fields: FieldPair<'_>,
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

        let slot = self.current_frame_slot();
        let cmd = self.ensure_frame_command_buffer(slot);
        let rpd = RenderPassDescriptor::new();
        let ca0 = rpd.color_attachments().object_at(0).unwrap();
        configure_frame_color_attachment(
            ca0,
            &target_tex,
            self.frame_color_initialized && self.persistent_target_valid,
        );

        self.acc_render_passes = self.acc_render_passes.saturating_add(1);
        self.acc_id_mask_compositor_passes =
            self.acc_id_mask_compositor_passes.saturating_add(1);
        let enc = cmd.new_render_command_encoder(&rpd);
        // The compositor shader builds a local full-quad for mask sampling.
        // Hardware viewport/scissor maps that quad into the requested widget
        // rect so embedded map renderers do not leak fullscreen pixels or shade
        // outside the visible surface.
        set_viewport_and_scissor_dp(&enc, self, viewport);
        enc.set_render_pipeline_state(match fields {
            FieldPair::Packed(_) => &self.pso_id_mask_compositor,
            FieldPair::Wide { .. } => &self.pso_id_mask_compositor_wide,
        });
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
        match fields {
            FieldPair::Packed(field) => enc.set_fragment_texture(2, Some(field)),
            FieldPair::Wide { city, seam } => {
                enc.set_fragment_texture(2, Some(city));
                enc.set_fragment_texture(3, Some(seam));
            }
        }
        enc.draw_primitives(MTLPrimitiveType::Triangle, 0, 6);
        enc.end_encoding();

        self.acc_draws = self.acc_draws.saturating_add(1);
        self.frame_color_initialized = true;
        self.persistent_target_valid = true;
        if let Some(t0) = self.frame_encode_started_at {
            self.last_stats.encode_ms = t0.elapsed().as_secs_f64() * 1000.0;
        }
        self.last_stats.draws = self.acc_draws;
        self.apply_id_mask_cache_stats();
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
        let cache_key = IdMaskFieldCacheKey::new(&pass.raster);
        if let Some(targets) = self.id_mask_field_cache_hit(cache_key, pass.raster.chunks) {
            #[cfg(feature = "snapshot-tests")]
            {
                self.id_mask_snapshot_target = Some(targets.clone());
            }
            return self.encode_id_mask_compositor_textures(
                pass.raster.viewport,
                pass.raster.mask_width,
                pass.raster.mask_height,
                pass.raster.mask_scale,
                &targets.city,
                &targets.neighborhood,
                targets.final_fields(),
                &pass.city_styles,
                &pass.neighborhood_colors,
                pass.mode,
                pass.glow_enabled,
                pass.darken_background_alpha,
                pass.polish,
            );
        }
        self.acc_id_mask_cache_misses = self.acc_id_mask_cache_misses.saturating_add(1);
        self.acc_backend_cache_misses = self.acc_backend_cache_misses.saturating_add(1);
        let required_bytes = self.id_mask_render_targets_required_bytes(
            pass.raster.mask_width,
            pass.raster.mask_height,
        );
        let admission = self.prepare_id_mask_cache_admission(
            required_bytes,
            pass.raster.mask_width,
            pass.raster.mask_height,
        );
        let cacheable = admission.is_some();
        let reusable = admission.flatten();
        #[cfg(feature = "snapshot-tests")]
        let reusable = if !cacheable
            && self.frame_in_flight.load(Ordering::Acquire) == 0
            && self.id_mask_in_flight_generations[self.current_frame_slot()].is_empty()
        {
            reusable.or_else(|| {
                self.id_mask_snapshot_target.take().filter(|targets| {
                    targets.width == pass.raster.mask_width
                        && targets.height == pass.raster.mask_height
                })
            })
        }
        else
        {
            reusable
        };
        let targets = self.new_id_mask_render_targets(
            pass.raster.mask_width,
            pass.raster.mask_height,
            reusable,
        )?;
        #[cfg(feature = "snapshot-tests")]
        {
            self.id_mask_snapshot_target = Some(targets.clone());
        }
        if !cacheable {
            let serial = self.next_id_mask_generation_serial();
            let bytes = Self::id_mask_render_targets_bytes(&targets);
            self.retain_id_mask_in_flight_generation(serial, bytes);
        }
        for chunk in pass.raster.chunks {
            self.acc_chunks_prepared = self.acc_chunks_prepared.saturating_add(1);
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
        self.acc_render_passes = self.acc_render_passes.saturating_add(1);
        self.acc_id_mask_raster_passes = self.acc_id_mask_raster_passes.saturating_add(1);
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
        configure_clear_store_fields(&rpd, targets.field_pair(true));
        self.acc_render_passes = self.acc_render_passes.saturating_add(1);
        self.acc_id_mask_field_seed_passes =
            self.acc_id_mask_field_seed_passes.saturating_add(1);
        let enc = cmd.new_render_command_encoder(&rpd);
        enc.set_render_pipeline_state(match &targets.fields {
            FieldTextures::Packed { .. } => &self.pso_id_mask_field_seed,
            FieldTextures::Wide { .. } => &self.pso_id_mask_field_seed_wide,
        });
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
            let source = targets.field_pair(src_is_a);
            let destination = targets.field_pair(!src_is_a);
            let rpd = RenderPassDescriptor::new();
            configure_clear_store_fields(&rpd, destination);
            self.acc_render_passes = self.acc_render_passes.saturating_add(1);
            self.acc_id_mask_field_jump_passes =
                self.acc_id_mask_field_jump_passes.saturating_add(1);
            let enc = cmd.new_render_command_encoder(&rpd);
            enc.set_render_pipeline_state(match source {
                FieldPair::Packed(_) => &self.pso_id_mask_field_jump,
                FieldPair::Wide { .. } => &self.pso_id_mask_field_jump_wide,
            });
            enc.set_fragment_bytes(
                0,
                core::mem::size_of_val(&params) as u64,
                (&params as *const FieldGpuParams).cast(),
            );
            match source {
                FieldPair::Packed(field) => enc.set_fragment_texture(0, Some(field)),
                FieldPair::Wide { city, seam } => {
                    enc.set_fragment_texture(0, Some(city));
                    enc.set_fragment_texture(1, Some(seam));
                }
            }
            enc.draw_primitives(MTLPrimitiveType::Triangle, 0, 6);
            enc.end_encoding();
            src_is_a = !src_is_a;
            jump /= 2;
        }
        if cacheable {
            if !self.retain_id_mask_field_cache_entry(cache_key, pass.raster.chunks, &targets) {
                let serial = self.next_id_mask_generation_serial();
                let bytes = Self::id_mask_render_targets_bytes(&targets);
                self.retain_id_mask_in_flight_generation(serial, bytes);
            }
        }
        self.encode_id_mask_compositor_textures(
            pass.raster.viewport,
            pass.raster.mask_width,
            pass.raster.mask_height,
            pass.raster.mask_scale,
            &targets.city,
            &targets.neighborhood,
            targets.final_fields(),
            &pass.city_styles,
            &pass.neighborhood_colors,
            pass.mode,
            pass.glow_enabled,
            pass.darken_background_alpha,
            pass.polish,
        )
    }
}

#[cfg(feature = "snapshot-tests")]
fn decode_rgba32_float(bytes: &[u8]) -> Vec<[f32; 4]>
{
   bytes
      .chunks_exact(16)
      .map(|pixel| {
         [
            f32::from_ne_bytes(pixel[0..4].try_into().unwrap()),
            f32::from_ne_bytes(pixel[4..8].try_into().unwrap()),
            f32::from_ne_bytes(pixel[8..12].try_into().unwrap()),
            f32::from_ne_bytes(pixel[12..16].try_into().unwrap()),
         ]
      })
      .collect()
}

#[cfg(feature = "snapshot-tests")]
fn decode_rgba16_uint_fields(
   bytes: &[u8],
   city: &[u8],
   neighborhood: &[u8],
   width: usize,
   height: usize,
) -> (Vec<[f32; 4]>, Vec<[f32; 4]>)
{
   debug_assert_eq!(bytes.len(), width.saturating_mul(height).saturating_mul(8));
   let mut city_field = Vec::with_capacity(width.saturating_mul(height));
   let mut seam_field = Vec::with_capacity(width.saturating_mul(height));
   for pixel in bytes.chunks_exact(8)
   {
      let component = |offset| u16::from_ne_bytes(pixel[offset..offset + 2].try_into().unwrap());
      let city_seed = [component(0), component(2)];
      let seam_seed = [component(4), component(6)];
      city_field.push(decode_packed_seed(city_seed, city, neighborhood, width, true));
      seam_field.push(decode_packed_seed(seam_seed, city, neighborhood, width, false));
   }
   (city_field, seam_field)
}

#[cfg(feature = "snapshot-tests")]
fn decode_packed_seed(
   seed: [u16; 2],
   city: &[u8],
   neighborhood: &[u8],
   width: usize,
   include_neighborhood: bool,
) -> [f32; 4]
{
   if seed[0] == u16::MAX || seed[1] == u16::MAX
   {
      return [-1.0, -1.0, 0.0, 0.0];
   }
   let index = seed[1] as usize * width + seed[0] as usize;
   [
      seed[0] as f32,
      seed[1] as f32,
      city[index] as f32,
      if include_neighborhood { neighborhood[index] as f32 } else { 1.0 },
   ]
}
