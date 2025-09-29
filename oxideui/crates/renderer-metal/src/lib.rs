//! OxideUI Metal renderer (metal-rs backend)
#![allow(clippy::all, clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::unnecessary_cast,
    clippy::borrow_as_ptr,
    clippy::items_after_statements,
    clippy::useless_ptr_null_checks,
    clippy::bool_to_int_with_if,
    clippy::nonminimal_bool,
    clippy::too_many_lines,
    clippy::explicit_iter_loop,
    clippy::unnecessary_get_then_check,
    clippy::map_unwrap_or,
    clippy::ref_as_ptr,
    clippy::match_same_arms,
    clippy::implicit_clone,
    clippy::semicolon_if_nothing_returned,
    clippy::unnecessary_min_or_max,
    clippy::too_many_arguments,
    clippy::missing_safety_doc,
    clippy::uninlined_format_args,
    clippy::manual_let_else,
    clippy::ptr_as_ptr,
    clippy::needless_borrow,
    clippy::unnecessary_wraps,
    clippy::must_use_candidate,
    clippy::similar_names,
    unused_variables
)]

use block::ConcreteBlock;
use core::ptr::NonNull;
use metal::foreign_types::ForeignType;
use metal::foreign_types::ForeignTypeRef;
use metal::{self, *};
use oxideui_renderer_api as api;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use thiserror::Error;

#[cfg(target_os = "ios")]
extern "C" {
    fn oxideui_host_release_drawable(drawable: *mut core::ffi::c_void);
    fn oxideui_host_ios_log(ptr: *const core::ffi::c_char, len: usize);
}

#[cfg(not(target_os = "ios"))]
unsafe fn oxideui_host_release_drawable(_drawable: *mut core::ffi::c_void) {}

#[inline(always)]
fn ios_log_enabled() -> bool {
    std::env::var("OXIDEUI_RUST_LOG")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[inline(always)]
fn ios_log(msg: &str) {
    #[cfg(target_os = "ios")]
    unsafe {
        if ios_log_enabled() {
            oxideui_host_ios_log(msg.as_ptr() as *const core::ffi::c_char, msg.len());
        }
    }
}

#[derive(Debug, Error)]
pub enum MetalInitError {
    #[error("no metal device available")]
    NoDevice,
    #[error("failed to create command queue")]
    NoQueue,
    #[error("failed to compile shader library: {0}")]
    Library(String),
    #[error("pipeline state error")]
    Pipeline,
}

const SHADERS_SRC: &str = concat!(
    include_str!("../shaders/solid.metal"),
    "\n",
    include_str!("../shaders/effects.metal"),
    "\n",
    include_str!("../shaders/ui.metal"),
    "\n",
    include_str!("../shaders/text.metal"),
    "\n",
    include_str!("../shaders/camera.metal"),
);

#[allow(dead_code)]
pub struct MetalRenderer {
    device: Device,
    queue: CommandQueue,
    pso_solid: RenderPipelineState,
    pso_image: RenderPipelineState,
    pso_blur: RenderPipelineState,
    pso_downsample: RenderPipelineState,
    pso_upsample: RenderPipelineState,
    pso_backdrop: RenderPipelineState,
    pso_rrect: RenderPipelineState,
    pso_nine_slice: RenderPipelineState,
    pso_spinner: RenderPipelineState,
    pso_text: RenderPipelineState,
    pso_text_sdf: RenderPipelineState,
    pso_camera: RenderPipelineState,
    // Argument buffer for image textures
    img_arg: Option<ArgumentEncoder>,
    img_arg_buf: Option<Buffer>,
    sampler: Option<SamplerState>,
    color_format: MTLPixelFormat,
    frame_id: u64,
    frames: [PerFrame; 3],
    vb: Ring,
    ib: Ring,
    ub: Ring,
    target_w: u32,
    target_h: u32,
    target_scale: f32,
    target_tex: Option<Texture>,
    prepass_tex: Option<Texture>,
    blur_tmp_tex: Option<Texture>,
    half_tex: Option<Texture>,
    quarter_tex: Option<Texture>,
    quarter_tmp_tex: Option<Texture>,
    images: HashMap<u32, Texture>,
    next_image_id: u32,
    layers: HashMap<u32, LayerEntry>,
    last_stats: PerfStats,
    acc_draws: u32,
    acc_instanced: u32,
    acc_icb_cmds: u32,
    // Damage rendering flag and per-frame scissor (dp) if provided
    damage_enabled: bool,
    frame_scissor_dp: Option<api::RectI>,
    frame_damage_rects: u32,
    frame_damage_pct: f32,
    frame_damage_px: u64,
    acc_culled: u32,
    damage_use_thresh: f32,
    damage_prefilter_thresh: f32,
    main_shaded_px: u64,
    prepass_shaded_px: u64,
    scissor_changes: u32,
    // Camera blur cache + scheduling
    cam_blur_tex: Option<Texture>,
    cam_last_update: Option<std::time::Instant>,
    cam_update_period: std::time::Duration,
    // Adaptive/pause state
    cam_paused: bool,
    cam_pause_frames: u32,
    // Camera props and transitions
    last_cam_w: i32,
    last_cam_h: i32,
    last_cam_bd: i32,
    last_cam_mx: i32,
    last_cam_vr: i32,
    last_cam_cs: i32,
    cam_xfade_prev_tex: Option<Texture>,
    cam_xfade_t0: Option<std::time::Instant>,
    cam_xfade_ms: u32,
    cam_blur_fade_t0: Option<std::time::Instant>,
}

impl MetalRenderer {
    pub fn new_default() -> Result<Self, MetalInitError> {
        let device = Device::system_default().ok_or(MetalInitError::NoDevice)?;
        let queue = device.new_command_queue();
        let compile_opts = CompileOptions::new();
        // Target explicit Metal Shading Language version for cross-macOS consistency
        // Highest available in metal-rs 0.32.0 (MSL 3.2 not yet exposed)
        compile_opts.set_language_version(MTLLanguageVersion::V3_0);
        let library = device
            .new_library_with_source(SHADERS_SRC, &compile_opts)
            .map_err(|e| MetalInitError::Library(format!("{}", e)))?;
        let color_format = MTLPixelFormat::BGRA8Unorm_sRGB;
        let pso_solid = build_solid_pso(&device, &library, color_format)?;
        let pso_image = build_image_pso(&device, &library, color_format)?;
        let pso_blur = build_blur_pso(&device, &library, color_format)?;
        let pso_downsample = build_downsample_pso(&device, &library, color_format)?;
        let pso_upsample = build_upsample_pso(&device, &library, color_format)?;
        let pso_backdrop = build_backdrop_pso(&device, &library, color_format)?;
        let pso_rrect = build_rrect_pso(&device, &library, color_format)?;
        let pso_nine = build_nine_slice_pso(&device, &library, color_format)?;
        let pso_spin = build_spinner_pso(&device, &library, color_format)?;
        let pso_text = build_text_pso(&device, &library, color_format)?;
        let pso_text_sdf = build_text_sdf_pso(&device, &library, color_format)?;
        let pso_camera = build_camera_pso(&device, &library, color_format)?;
        // Prepare argument encoder for image textures
        let f_image_fn =
            library.get_function("f_image", None).map_err(|_| MetalInitError::Pipeline)?;
        let img_arg = Some(f_image_fn.new_argument_encoder(2));
        let img_ab_len = img_arg.as_ref().unwrap().encoded_length();
        let img_arg_buf =
            Some(device.new_buffer(img_ab_len, MTLResourceOptions::StorageModeShared));
        img_arg.as_ref().unwrap().set_argument_buffer(img_arg_buf.as_ref().unwrap(), 0);
        let sampler = build_sampler(&device);
        let opts =
            MTLResourceOptions::CPUCacheModeWriteCombined | MTLResourceOptions::StorageModeShared;
        let vb = Ring::new(&device, 64 * 1024, opts);
        let ib = Ring::new(&device, 32 * 1024, opts);
        let ub = Ring::new(&device, 32 * 1024, opts);
        let damage_enabled = std::env::var("OXIDEUI_ENABLE_DAMAGE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let damage_use_thresh = std::env::var("OXIDEUI_DAMAGE_USE_THRESH")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.70);
        let damage_prefilter_thresh = std::env::var("OXIDEUI_DAMAGE_PREFILTER_THRESH")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.25);
        Ok(Self {
            device,
            queue,
            pso_solid,
            pso_image,
            pso_blur,
            pso_downsample,
            pso_upsample,
            pso_backdrop,
            pso_rrect,
            pso_nine_slice: pso_nine,
            pso_spinner: pso_spin,
            pso_text,
            pso_text_sdf,
            pso_camera,
            img_arg,
            img_arg_buf,
            sampler,
            color_format,
            frame_id: 0,
            frames: [PerFrame::new(), PerFrame::new(), PerFrame::new()],
            vb,
            ib,
            ub,
            target_w: 0,
            target_h: 0,
            target_scale: 1.0,
            target_tex: None,
            prepass_tex: None,
            blur_tmp_tex: None,
            half_tex: None,
            quarter_tex: None,
            quarter_tmp_tex: None,
            images: HashMap::new(),
            next_image_id: 1,
            layers: HashMap::new(),
            last_stats: PerfStats::default(),
            acc_draws: 0,
            acc_instanced: 0,
            acc_icb_cmds: 0,
            damage_enabled,
            frame_scissor_dp: None,
            frame_damage_rects: 0,
            frame_damage_pct: 0.0,
            frame_damage_px: 0,
            acc_culled: 0,
            damage_use_thresh,
            damage_prefilter_thresh,
            main_shaded_px: 0,
            prepass_shaded_px: 0,
            scissor_changes: 0,
            cam_blur_tex: None,
            cam_last_update: None,
            cam_update_period: std::time::Duration::from_millis(83), // ~12 fps
            cam_paused: false,
            cam_pause_frames: 0,
            last_cam_w: 0,
            last_cam_h: 0,
            last_cam_bd: 8,
            last_cam_mx: 0,
            last_cam_vr: 0,
            last_cam_cs: 0,
            cam_xfade_prev_tex: None,
            cam_xfade_t0: None,
            cam_xfade_ms: 120,
            cam_blur_fade_t0: None,
        })
    }

    fn ensure_target(&mut self) {
        if self.target_w == 0 || self.target_h == 0 {
            return;
        }
        let need_new = match &self.target_tex {
            Some(tex) => {
                tex.width() as u32 != self.target_w || tex.height() as u32 != self.target_h
            }
            None => true,
        };
        if need_new {
            let desc = TextureDescriptor::new();
            desc.set_pixel_format(self.color_format);
            desc.set_texture_type(MTLTextureType::D2);
            desc.set_width(self.target_w as u64);
            desc.set_height(self.target_h as u64);
            desc.set_storage_mode(MTLStorageMode::Private);
            desc.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self.target_tex = Some(self.device.new_texture(&desc));
        }
    }

    fn ensure_effect_targets(&mut self) {
        if self.target_w == 0 || self.target_h == 0 {
            return;
        }
        let need_src = match &self.prepass_tex {
            Some(tex) => {
                tex.width() as u32 != self.target_w || tex.height() as u32 != self.target_h
            }
            None => true,
        };
        let need_tmp = match &self.blur_tmp_tex {
            Some(tex) => {
                tex.width() as u32 != self.target_w || tex.height() as u32 != self.target_h
            }
            None => true,
        };
        if need_src {
            let d = TextureDescriptor::new();
            d.set_pixel_format(self.color_format);
            d.set_texture_type(MTLTextureType::D2);
            d.set_width(self.target_w as u64);
            d.set_height(self.target_h as u64);
            d.set_storage_mode(MTLStorageMode::Private);
            d.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self.prepass_tex = Some(self.device.new_texture(&d));
        }
        if need_tmp {
            let d = TextureDescriptor::new();
            d.set_pixel_format(self.color_format);
            d.set_texture_type(MTLTextureType::D2);
            d.set_width(self.target_w as u64);
            d.set_height(self.target_h as u64);
            d.set_storage_mode(MTLStorageMode::Private);
            d.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self.blur_tmp_tex = Some(self.device.new_texture(&d));
        }

        // Downsample chain targets (half, quarter) + quarter ping-pong
        let (hw, hh) = (((self.target_w / 2).max(1)) as u64, ((self.target_h / 2).max(1)) as u64);
        let (qw, qh) = (((self.target_w / 4).max(1)) as u64, ((self.target_h / 4).max(1)) as u64);
        let need_half = match &self.half_tex {
            Some(tex) => tex.width() != hw || tex.height() != hh,
            None => true,
        };
        let need_quarter = match &self.quarter_tex {
            Some(tex) => tex.width() != qw || tex.height() != qh,
            None => true,
        };
        let need_quarter_tmp = match &self.quarter_tmp_tex {
            Some(tex) => tex.width() != qw || tex.height() != qh,
            None => true,
        };
        if need_half {
            let d = TextureDescriptor::new();
            d.set_pixel_format(self.color_format);
            d.set_texture_type(MTLTextureType::D2);
            d.set_width(hw);
            d.set_height(hh);
            d.set_storage_mode(MTLStorageMode::Private);
            d.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self.half_tex = Some(self.device.new_texture(&d));
        }
        if need_quarter {
            let d = TextureDescriptor::new();
            d.set_pixel_format(self.color_format);
            d.set_texture_type(MTLTextureType::D2);
            d.set_width(qw);
            d.set_height(qh);
            d.set_storage_mode(MTLStorageMode::Private);
            d.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self.quarter_tex = Some(self.device.new_texture(&d));
        }
        if need_quarter_tmp {
            let d = TextureDescriptor::new();
            d.set_pixel_format(self.color_format);
            d.set_texture_type(MTLTextureType::D2);
            d.set_width(qw);
            d.set_height(qh);
            d.set_storage_mode(MTLStorageMode::Private);
            d.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self.quarter_tmp_tex = Some(self.device.new_texture(&d));
        }
    }

    fn get_image_tex(&self, h: api::ImageHandle) -> Option<&Texture> {
        self.images.get(&h.0)
    }

    pub fn image_create_a8(
        &mut self,
        w: u32,
        h: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> api::ImageHandle {
        let desc = TextureDescriptor::new();
        desc.set_pixel_format(MTLPixelFormat::R8Unorm);
        desc.set_texture_type(MTLTextureType::D2);
        desc.set_width(w as u64);
        desc.set_height(h as u64);
        desc.set_storage_mode(MTLStorageMode::Shared);
        desc.set_usage(MTLTextureUsage::ShaderRead);
        let tex = self.device.new_texture(&desc);
        let region = MTLRegion {
            origin: MTLOrigin { x: 0, y: 0, z: 0 },
            size: MTLSize { width: w as u64, height: h as u64, depth: 1 },
        };
        let bpr = if row_bytes == 0 { w as usize } else { row_bytes } as u64;
        tex.replace_region(region, 0, data.as_ptr() as *const _, bpr);
        let id = self.next_image_id;
        self.next_image_id = self.next_image_id.wrapping_add(1).max(1);
        self.images.insert(id, tex);
        api::ImageHandle(id)
    }

    pub fn image_update_a8(
        &mut self,
        handle: api::ImageHandle,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        data: &[u8],
        row_bytes: usize,
    ) {
        if let Some(tex) = self.images.get(&handle.0) {
            let region = MTLRegion {
                origin: MTLOrigin { x: x as u64, y: y as u64, z: 0 },
                size: MTLSize { width: w as u64, height: h as u64, depth: 1 },
            };
            let bpr = if row_bytes == 0 { w as usize } else { row_bytes } as u64;
            tex.replace_region(region, 0, data.as_ptr() as *const _, bpr);
        }
    }

    pub fn image_create_rgba8(
        &mut self,
        w: u32,
        h: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> api::ImageHandle {
        let desc = TextureDescriptor::new();
        desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm_sRGB);
        desc.set_texture_type(MTLTextureType::D2);
        desc.set_width(w as u64);
        desc.set_height(h as u64);
        desc.set_storage_mode(MTLStorageMode::Shared);
        desc.set_usage(MTLTextureUsage::ShaderRead);
        let tex = self.device.new_texture(&desc);
        let region = MTLRegion {
            origin: MTLOrigin { x: 0, y: 0, z: 0 },
            size: MTLSize { width: w as u64, height: h as u64, depth: 1 },
        };
        let bpr = if row_bytes == 0 { (w as usize) * 4 } else { row_bytes } as u64;
        tex.replace_region(region, 0, data.as_ptr() as *const _, bpr);
        let id = self.next_image_id;
        self.next_image_id = self.next_image_id.wrapping_add(1).max(1);
        self.images.insert(id, tex);
        api::ImageHandle(id)
    }

    pub fn image_update_rgba8(
        &mut self,
        handle: api::ImageHandle,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        data: &[u8],
        row_bytes: usize,
    ) {
        if let Some(tex) = self.images.get(&handle.0) {
            let region = MTLRegion {
                origin: MTLOrigin { x: x as u64, y: y as u64, z: 0 },
                size: MTLSize { width: w as u64, height: h as u64, depth: 1 },
            };
            let bpr = if row_bytes == 0 { (w as usize) * 4 } else { row_bytes } as u64;
            tex.replace_region(region, 0, data.as_ptr() as *const _, bpr);
        }
    }

    pub fn image_release(&mut self, handle: api::ImageHandle) {
        let _ = self.images.remove(&handle.0);
    }

    pub unsafe fn blit_to_texture_and_present_drawable(
        &mut self,
        dst_tex_ptr: *mut core::ffi::c_void,
        drawable_ptr: *mut core::ffi::c_void,
    ) -> Result<(), api::RenderError> {
        ios_log(&format!(
            "metal: blit+present begin dst={:p} drawable={:p}",
            dst_tex_ptr, drawable_ptr
        ));
        let src = match &self.target_tex {
            Some(t) => t,
            None => return Err(api::RenderError::InvalidOperation("no target texture")),
        };
        let dst = unsafe { TextureRef::from_ptr(dst_tex_ptr as *mut MTLTexture) };
        let raw_drawable = drawable_ptr as *mut MTLDrawable;
        let drawable = unsafe { Drawable::from_ptr(raw_drawable) };
        let cmd = self.queue.new_command_buffer();
        let blit = cmd.new_blit_command_encoder();
        let origin = MTLOrigin { x: 0, y: 0, z: 0 };
        let size = MTLSize { width: src.width(), height: src.height(), depth: 1 };
        blit.copy_from_texture(src, 0, 0, origin, size, dst, 0, 0, origin);
        blit.end_encoding();
        ios_log("metal: calling present_drawable");
        cmd.present_drawable(&drawable);
        let slot = (self.frame_id % 3) as usize;
        if let Some(pf) = self.frames.get_mut(slot) {
            let pf_ptr: *mut PerFrame = pf;
            let release_ptr = drawable_ptr;
            let block = ConcreteBlock::new(move |_cb: &CommandBufferRef| unsafe {
                (*pf_ptr).completed();
                oxideui_host_release_drawable(release_ptr);
            })
            .copy();
            cmd.add_completed_handler(&block);
        } else {
            oxideui_host_release_drawable(drawable_ptr);
        }
        ios_log("metal: committing command buffer");
        cmd.commit();
        core::mem::forget(drawable);
        ios_log("metal: blit+present end");
        Ok(())
    }

    pub fn readback_bgra8(&mut self) -> Option<(u32, u32, alloc::vec::Vec<u8>)> {
        let tex = self.target_tex.as_ref()?;
        let w = tex.width() as u32;
        let h = tex.height() as u32;
        let row_bytes = (w as usize) * 4;
        let buf_bytes = row_bytes * (h as usize);
        let opts =
            MTLResourceOptions::CPUCacheModeDefaultCache | MTLResourceOptions::StorageModeShared;
        let buf = self.device.new_buffer(buf_bytes as u64, opts);
        let cmd = self.queue.new_command_buffer();
        let blit = cmd.new_blit_command_encoder();
        let origin = MTLOrigin { x: 0, y: 0, z: 0 };
        let size = MTLSize { width: w as u64, height: h as u64, depth: 1 };
        blit.copy_from_texture_to_buffer(
            tex,
            0,
            0,
            origin,
            size,
            &buf,
            0,
            row_bytes as u64,
            (row_bytes * (h as usize)) as u64,
            MTLBlitOption::empty(),
        );
        blit.end_encoding();
        cmd.commit();
        cmd.wait_until_completed();
        let ptr = buf.contents();
        if ptr.is_null() {
            return None;
        }
        let out = unsafe { core::slice::from_raw_parts(ptr as *const u8, buf_bytes) };
        Some((w, h, out.to_vec()))
    }
}

// Build a filtered copy of a DrawList that keeps only items whose bounding
// rect (in dp) intersects the provided dp scissor. Vertices/indices are
// copied by reference (cloned arrays), spans remain valid.
fn filter_drawlist_by_dp_scissor(list: &api::DrawList, sc: api::RectI) -> api::DrawList {
    fn rect_intersects(r: &api::RectF, sc: &api::RectI) -> bool {
        let rx0 = r.x;
        let ry0 = r.y;
        let rx1 = r.x + r.w;
        let ry1 = r.y + r.h;
        let sx0 = sc.x as f32;
        let sy0 = sc.y as f32;
        let sx1 = (sc.x + sc.w) as f32;
        let sy1 = (sc.y + sc.h) as f32;
        rx1 > sx0 && rx0 < sx1 && ry1 > sy0 && ry0 < sy1
    }
    let mut out = api::DrawList {
        items: alloc::vec::Vec::new(),
        vertices: list.vertices.clone(),
        indices: list.indices.clone(),
    };
    let mut i = 0usize;
    while i < list.items.len() {
        match &list.items[i] {
            api::DrawCmd::RRect { rect, .. } => {
                if rect_intersects(rect, &sc) {
                    out.items.push(list.items[i].clone());
                }
                i += 1;
            }
            api::DrawCmd::CameraBg { rect, .. } => {
                if rect_intersects(rect, &sc) {
                    out.items.push(list.items[i].clone());
                }
                i += 1;
            }
            api::DrawCmd::NineSlice { rect, .. } => {
                if rect_intersects(rect, &sc) {
                    out.items.push(list.items[i].clone());
                }
                i += 1;
            }
            api::DrawCmd::Image { dst, .. } => {
                if rect_intersects(dst, &sc) {
                    out.items.push(list.items[i].clone());
                }
                i += 1;
            }
            api::DrawCmd::Spinner { center, radius, thickness, .. } => {
                let mm = radius + thickness;
                let rect =
                    api::RectF { x: center[0] - mm, y: center[1] - mm, w: mm * 2.0, h: mm * 2.0 };
                if rect_intersects(&rect, &sc) {
                    out.items.push(list.items[i].clone());
                }
                i += 1;
            }
            api::DrawCmd::Backdrop { rect, .. } => {
                if rect_intersects(rect, &sc) {
                    out.items.push(list.items[i].clone());
                }
                i += 1;
            }
            api::DrawCmd::GlyphRun { run } => {
                // Compute bounding box from vertices
                let v_count = run.vb.len as usize;
                if v_count == 0 {
                    i += 1;
                    continue;
                }
                let srcv =
                    &list.vertices[(run.vb.offset as usize)..(run.vb.offset as usize + v_count)];
                let mut minx = f32::INFINITY;
                let mut miny = f32::INFINITY;
                let mut maxx = f32::NEG_INFINITY;
                let mut maxy = f32::NEG_INFINITY;
                for v in srcv.iter() {
                    minx = minx.min(v.x);
                    miny = miny.min(v.y);
                    maxx = maxx.max(v.x);
                    maxy = maxy.max(v.y);
                }
                let rect = api::RectF {
                    x: minx,
                    y: miny,
                    w: (maxx - minx).max(0.0),
                    h: (maxy - miny).max(0.0),
                };
                if rect_intersects(&rect, &sc) {
                    out.items.push(list.items[i].clone());
                }
                i += 1;
            }
            api::DrawCmd::LayerBegin { rect, .. } => {
                // If layer rect doesn't intersect, skip until matching LayerEnd
                let mut depth = 1usize;
                let mut j = i + 1;
                if rect_intersects(rect, &sc) {
                    out.items.push(list.items[i].clone());
                    while j < list.items.len() && depth > 0 {
                        match &list.items[j] {
                            api::DrawCmd::LayerBegin { .. } => {
                                depth += 1;
                                out.items.push(list.items[j].clone());
                            }
                            api::DrawCmd::LayerEnd => {
                                depth -= 1;
                                out.items.push(list.items[j].clone());
                            }
                            _ => out.items.push(list.items[j].clone()),
                        }
                        j += 1;
                    }
                } else {
                    while j < list.items.len() && depth > 0 {
                        match &list.items[j] {
                            api::DrawCmd::LayerBegin { .. } => depth += 1,
                            api::DrawCmd::LayerEnd => depth -= 1,
                            _ => {}
                        }
                        j += 1;
                    }
                }
                i = j;
            }
            api::DrawCmd::LayerEnd => {
                out.items.push(list.items[i].clone());
                i += 1;
            }
            api::DrawCmd::Solid { .. } => {
                out.items.push(list.items[i].clone());
                i += 1;
            }
            api::DrawCmd::ClipPush { .. } | api::DrawCmd::ClipPop => {
                out.items.push(list.items[i].clone());
                i += 1;
            }
        }
    }
    out
}

impl api::Renderer for MetalRenderer {
    fn device_caps(&self) -> api::DeviceCaps {
        api::DeviceCaps {
            max_framerate_hz: 120,
            supports_edr: false,
            supports_msaa4x: false,
            native_scale: 1.0,
        }
    }

    fn begin_frame(
        &mut self,
        _fb: &api::FrameTarget,
        damage: Option<&api::Damage>,
    ) -> api::FrameToken {
        self.frame_id = self.frame_id.wrapping_add(1);
        let slot = (self.frame_id % 3) as usize;
        self.frames[slot].reset();
        self.acc_draws = 0;
        self.acc_instanced = 0;
        self.acc_icb_cmds = 0;
        self.acc_culled = 0;
        // Defer command buffer creation to encode_pass
        self.frames[slot].cmd = None;
        // Reset per-frame accumulators
        self.scissor_changes = 0;
        self.prepass_shaded_px = 0;
        self.main_shaded_px = 0;
        // Capture frame-level scissor in dp when enabled
        if self.damage_enabled {
            if let Some(d) = damage {
                self.frame_damage_rects = d.rects.len() as u32;
                // Union of provided rects (dp)
                let mut it = d.rects.iter();
                if let Some(first) = it.next() {
                    let mut x0 = first.x;
                    let mut y0 = first.y;
                    let mut x1 = first.x + first.w;
                    let mut y1 = first.y + first.h;
                    for r in it {
                        x0 = x0.min(r.x);
                        y0 = y0.min(r.y);
                        x1 = x1.max(r.x + r.w);
                        y1 = y1.max(r.y + r.h);
                    }
                    let w = (x1 - x0).max(0);
                    let h = (y1 - y0).max(0);
                    if w > 0 && h > 0 {
                        self.frame_scissor_dp = Some(api::RectI { x: x0, y: y0, w, h });
                    } else {
                        self.frame_scissor_dp = None;
                    }
                } else {
                    self.frame_scissor_dp = None;
                }
            } else {
                self.frame_scissor_dp = None;
                self.frame_damage_rects = 0;
            }
        } else {
            self.frame_scissor_dp = None;
            self.frame_damage_rects = 0;
        }
        // Compute damage coverage metrics
        if let Some(dp) = self.frame_scissor_dp {
            let vp_w_dp = (self.target_w as f32) / self.target_scale.max(1.0);
            let vp_h_dp = (self.target_h as f32) / self.target_scale.max(1.0);
            let vp_area_dp = (vp_w_dp.max(1.0)) * (vp_h_dp.max(1.0));
            let dmg_area_dp = (dp.w.max(0) as f32) * (dp.h.max(0) as f32);
            self.frame_damage_pct =
                if vp_area_dp > 0.0 { (dmg_area_dp / vp_area_dp).clamp(0.0, 1.0) } else { 0.0 };
            // Convert to px and clamp to framebuffer bounds
            let s = self.target_scale.max(1.0);
            let x = (dp.x as f32 * s).floor() as i32;
            let y = (dp.y as f32 * s).floor() as i32;
            let w = (dp.w as f32 * s).ceil() as i32;
            let h = (dp.h as f32 * s).ceil() as i32;
            let tx = 0;
            let ty = 0;
            let tw = self.target_w as i32;
            let th = self.target_h as i32;
            let x1 = x.clamp(tx, tx + tw);
            let y1 = y.clamp(ty, ty + th);
            let x2 = (x + w).clamp(tx, tx + tw);
            let y2 = (y + h).clamp(ty, ty + th);
            let rw = (x2 - x1).max(0) as u64;
            let rh = (y2 - y1).max(0) as u64;
            self.frame_damage_px = rw.saturating_mul(rh);
        } else {
            self.frame_damage_pct = 0.0;
            self.frame_damage_px = 0;
        }
        api::FrameToken(self.frame_id)
    }

    fn encode_pass(&mut self, list: &api::DrawList) {
        let cpu_t0 = std::time::Instant::now();
        if self.target_tex.is_none() {
            return;
        }
        let slot = (self.frame_id % 3) as usize;
        // Create command buffer for this frame now
        let cmd = self.queue.new_command_buffer().to_owned();
        self.frames[slot].cmd = Some(cmd.to_owned());

        // Adaptive policy: compute camera coverage and environment (iOS thermal/LPM),
        // then tune blur update period and optionally pause camera when hot with tiny coverage.
        let vp_w_dp = (self.target_w as f32) / self.target_scale.max(1.0);
        let vp_h_dp = (self.target_h as f32) / self.target_scale.max(1.0);
        let vp_area_dp = (vp_w_dp.max(1.0)) * (vp_h_dp.max(1.0));
        let mut cam_area: f32 = 0.0;
        for it in &list.items {
            if let api::DrawCmd::CameraBg { rect, .. } = it {
                let a = (rect.w.max(0.0) * rect.h.max(0.0)).min(vp_area_dp);
                cam_area += a;
            }
        }
        let cam_coverage =
            if vp_area_dp > 0.0 { (cam_area / vp_area_dp).clamp(0.0, 1.0) } else { 0.0 };
        #[cfg(target_os = "ios")]
        let (lpm, therm) = unsafe {
            extern "C" {
                fn oxideui_host_power_lowpower() -> ::libc::c_int;
                fn oxideui_host_thermal_state() -> ::libc::c_int;
            }
            (oxideui_host_power_lowpower() != 0, oxideui_host_thermal_state())
        };
        #[cfg(not(target_os = "ios"))]
        let (lpm, therm) = (false, 0);
        // Tune blur update period
        let mut period_ms: u64 = 83; // ~12 fps
        if lpm || therm >= 2 {
            period_ms = 120;
        } else if therm == 1 {
            period_ms = 100;
        }
        if cam_coverage < 0.15 {
            period_ms = period_ms.max(110);
        }
        if self.cam_update_period != std::time::Duration::from_millis(period_ms) {
            self.cam_update_period = std::time::Duration::from_millis(period_ms);
        }
        // Pause/resume capture when very hot and tiny coverage to save power
        #[cfg(target_os = "ios")]
        unsafe {
            extern "C" {
                fn oxideui_cam_stop();
                fn oxideui_cam_start_default();
            }
            if (lpm || therm >= 2) && cam_coverage < 0.05 {
                self.cam_pause_frames = self.cam_pause_frames.saturating_add(1);
                if self.cam_pause_frames > 30 && !self.cam_paused {
                    oxideui_cam_stop();
                    self.cam_paused = true;
                }
            } else {
                self.cam_pause_frames = 0;
                if self.cam_paused && cam_coverage > 0.10 {
                    oxideui_cam_start_default();
                    self.cam_paused = false;
                }
            }
        }

        // Camera blur prepass: if any CameraBg requests blur, update a cached blurred camera
        let need_cam_blur =
            list.items.iter().any(|c| matches!(c, api::DrawCmd::CameraBg { blur: true, .. }));
        #[cfg(target_os = "ios")]
        let mut blur_ms_out: f64 = 0.0;
        #[cfg(not(target_os = "ios"))]
        let blur_ms_out: f64 = 0.0;
        #[cfg(target_os = "ios")]
        let mut blur_updated: u32 = 0;
        #[cfg(not(target_os = "ios"))]
        let blur_updated: u32 = 0;
        if need_cam_blur {
            let do_update = match self.cam_last_update {
                None => true,
                Some(t) => t.elapsed() >= self.cam_update_period,
            };
            if do_update {
                let blur_t0 = std::time::Instant::now();
                #[cfg(target_os = "ios")]
                {
                    extern "C" {
                        fn oxideui_cam_get_latest(
                            y_tex: *mut *mut core::ffi::c_void,
                            uv_tex: *mut *mut core::ffi::c_void,
                            w: *mut i32,
                            h: *mut i32,
                        ) -> ::libc::c_int;
                    }
                    let mut y_ptr: *mut core::ffi::c_void = core::ptr::null_mut();
                    let mut uv_ptr: *mut core::ffi::c_void = core::ptr::null_mut();
                    let mut cw: i32 = 0;
                    let mut ch: i32 = 0;
                    extern "C" {
                        fn oxideui_cam_get_latest_ex(
                            y_tex: *mut *mut core::ffi::c_void,
                            uv_tex: *mut *mut core::ffi::c_void,
                            w: *mut i32,
                            h: *mut i32,
                            bitdepth: *mut i32,
                            matrix: *mut i32,
                            video_range: *mut i32,
                            colorspace: *mut i32,
                        ) -> ::libc::c_int;
                    }
                    let mut bd: i32 = 8;
                    let mut mx: i32 = 0;
                    let mut vr: i32 = 0;
                    let mut cs: i32 = 0;
                    let ok = unsafe {
                        oxideui_cam_get_latest_ex(
                            &mut y_ptr,
                            &mut uv_ptr,
                            &mut cw,
                            &mut ch,
                            &mut bd,
                            &mut mx,
                            &mut vr,
                            &mut cs,
                        )
                    };
                    if ok != 0 && !y_ptr.is_null() && !uv_ptr.is_null() && cw > 0 && ch > 0 {
                        let now = std::time::Instant::now();
                        let changed = self.last_cam_w != cw
                            || self.last_cam_h != ch
                            || self.last_cam_bd != bd
                            || self.last_cam_mx != mx
                            || self.last_cam_vr != vr
                            || self.last_cam_cs != cs;
                        if changed {
                            if let Some(tex) = &self.cam_blur_tex {
                                self.cam_xfade_prev_tex = Some(tex.to_owned());
                                self.cam_xfade_t0 = Some(now);
                            }
                            self.last_cam_w = cw;
                            self.last_cam_h = ch;
                            self.last_cam_bd = bd;
                            self.last_cam_mx = mx;
                            self.last_cam_vr = vr;
                            self.last_cam_cs = cs;
                        }
                        if self.cam_blur_tex.is_none() {
                            self.cam_blur_fade_t0 = Some(now);
                        }
                        let y_tex = unsafe { Texture::from_ptr(y_ptr as *mut MTLTexture) };
                        let uv_tex = unsafe { Texture::from_ptr(uv_ptr as *mut MTLTexture) };
                        // 1) Render NV12 to prepass texture
                        self.ensure_effect_targets();
                        if let Some(src) = &self.prepass_tex {
                            let rpd0 = RenderPassDescriptor::new();
                            let ca = rpd0.color_attachments().object_at(0).unwrap();
                            ca.set_texture(Some(src));
                            ca.set_load_action(MTLLoadAction::Clear);
                            ca.set_clear_color(MTLClearColor {
                                red: 0.0,
                                green: 0.0,
                                blue: 0.0,
                                alpha: 1.0,
                            });
                            ca.set_store_action(MTLStoreAction::Store);
                            let enc0 = cmd.new_render_command_encoder(&rpd0);
                            enc0.set_render_pipeline_state(&self.pso_camera);
                            if let Some(sam) = &self.sampler {
                                enc0.set_fragment_sampler_state(0, Some(sam));
                            }
                            enc0.set_fragment_texture(0, Some(&y_tex));
                            enc0.set_fragment_texture(1, Some(&uv_tex));
                            // VP dp and rect dp
                            let vp_dp: [f32; 2] = [
                                (self.target_w as f32) / self.target_scale.max(1.0),
                                (self.target_h as f32) / self.target_scale.max(1.0),
                            ];
                            let rect_dp: [f32; 4] = [0.0, 0.0, vp_dp[0], vp_dp[1]];
                            enc0.set_vertex_bytes(
                                1,
                                core::mem::size_of_val(&vp_dp) as u64,
                                vp_dp.as_ptr() as *const _,
                            );
                            enc0.set_vertex_bytes(
                                0,
                                core::mem::size_of_val(&rect_dp) as u64,
                                rect_dp.as_ptr() as *const _,
                            );
                            // Aspect fill params
                            let ar_dest = if vp_dp[1] > 0.0 { vp_dp[0] / vp_dp[1] } else { 1.0 };
                            let ar_cam = (cw as f32) / (ch as f32);
                            let (mut sx, mut sy) = (1.0f32, 1.0f32);
                            let (mut bx, mut by) = (0.0f32, 0.0f32);
                            if ar_cam > ar_dest {
                                sx = ar_dest / ar_cam;
                                bx = (1.0 - sx) * 0.5;
                            } else if ar_cam < ar_dest {
                                sy = ar_cam / ar_dest;
                                by = (1.0 - sy) * 0.5;
                            }
                            let rect_px =
                                [0.0f32, 0.0f32, self.target_w as f32, self.target_h as f32];
                            let fbuf: [f32; 16] = [
                                rect_px[0], rect_px[1], rect_px[2], rect_px[3], 1.0, 1.0, 1.0, 1.0,
                                sx, sy, bx, by, 0.0, // grayscale off in prepass
                                mx as f32, vr as f32, bd as f32,
                            ];
                            enc0.set_fragment_bytes(
                                1,
                                core::mem::size_of_val(&fbuf) as u64,
                                fbuf.as_ptr() as *const _,
                            );
                            enc0.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                            enc0.end_encoding();
                        }
                        // 2) Downsample and blur
                        self.ensure_effect_targets();
                        if let (Some(pre), Some(half)) = (&self.prepass_tex, &self.half_tex) {
                            let rpd = RenderPassDescriptor::new();
                            let ca = rpd.color_attachments().object_at(0).unwrap();
                            ca.set_texture(Some(half));
                            ca.set_load_action(MTLLoadAction::DontCare);
                            ca.set_store_action(MTLStoreAction::Store);
                            let enc = cmd.new_render_command_encoder(&rpd);
                            enc.set_render_pipeline_state(&self.pso_downsample);
                            if let Some(sam) = &self.sampler {
                                enc.set_fragment_sampler_state(0, Some(sam));
                            }
                            enc.set_fragment_texture(0, Some(pre));
                            let vp_dp: [f32; 2] = [
                                (self.target_w as f32) / self.target_scale.max(1.0),
                                (self.target_h as f32) / self.target_scale.max(1.0),
                            ];
                            let rect_dp: [f32; 4] = [0.0, 0.0, vp_dp[0], vp_dp[1]];
                            enc.set_vertex_bytes(
                                1,
                                core::mem::size_of_val(&vp_dp) as u64,
                                vp_dp.as_ptr() as *const _,
                            );
                            enc.set_vertex_bytes(
                                0,
                                core::mem::size_of_val(&rect_dp) as u64,
                                rect_dp.as_ptr() as *const _,
                            );
                            enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                            enc.end_encoding();
                        }
                        if let (Some(half), Some(q)) = (&self.half_tex, &self.quarter_tex) {
                            let rpd = RenderPassDescriptor::new();
                            let ca = rpd.color_attachments().object_at(0).unwrap();
                            ca.set_texture(Some(q));
                            ca.set_load_action(MTLLoadAction::DontCare);
                            ca.set_store_action(MTLStoreAction::Store);
                            let enc = cmd.new_render_command_encoder(&rpd);
                            enc.set_render_pipeline_state(&self.pso_downsample);
                            if let Some(sam) = &self.sampler {
                                enc.set_fragment_sampler_state(0, Some(sam));
                            }
                            enc.set_fragment_texture(0, Some(half));
                            let vp_dp: [f32; 2] = [
                                (self.target_w as f32) / self.target_scale.max(1.0),
                                (self.target_h as f32) / self.target_scale.max(1.0),
                            ];
                            let rect_dp: [f32; 4] = [0.0, 0.0, vp_dp[0], vp_dp[1]];
                            enc.set_vertex_bytes(
                                1,
                                core::mem::size_of_val(&vp_dp) as u64,
                                vp_dp.as_ptr() as *const _,
                            );
                            enc.set_vertex_bytes(
                                0,
                                core::mem::size_of_val(&rect_dp) as u64,
                                rect_dp.as_ptr() as *const _,
                            );
                            enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                            enc.end_encoding();
                        }
                        if let (Some(q), Some(qtmp)) = (&self.quarter_tex, &self.quarter_tmp_tex) {
                            let vp_dp: [f32; 2] = [
                                (self.target_w as f32) / self.target_scale.max(1.0),
                                (self.target_h as f32) / self.target_scale.max(1.0),
                            ];
                            let rect_dp: [f32; 4] = [0.0, 0.0, vp_dp[0], vp_dp[1]];
                            // Horizontal blur
                            let rpd = RenderPassDescriptor::new();
                            let ca = rpd.color_attachments().object_at(0).unwrap();
                            ca.set_texture(Some(qtmp));
                            ca.set_load_action(MTLLoadAction::DontCare);
                            ca.set_store_action(MTLStoreAction::Store);
                            let enc = cmd.new_render_command_encoder(&rpd);
                            enc.set_render_pipeline_state(&self.pso_blur);
                            if let Some(sam) = &self.sampler {
                                enc.set_fragment_sampler_state(0, Some(sam));
                            }
                            enc.set_fragment_texture(0, Some(q));
                            let params_h: [f32; 4] = [1.0, 0.0, 6.0, 0.0];
                            enc.set_vertex_bytes(
                                1,
                                core::mem::size_of_val(&vp_dp) as u64,
                                vp_dp.as_ptr() as *const _,
                            );
                            enc.set_vertex_bytes(
                                0,
                                core::mem::size_of_val(&rect_dp) as u64,
                                rect_dp.as_ptr() as *const _,
                            );
                            enc.set_fragment_bytes(
                                1,
                                core::mem::size_of_val(&params_h) as u64,
                                params_h.as_ptr() as *const _,
                            );
                            enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                            enc.end_encoding();
                            // Vertical blur back to q
                            let rpd2 = RenderPassDescriptor::new();
                            let ca2 = rpd2.color_attachments().object_at(0).unwrap();
                            ca2.set_texture(Some(q));
                            ca2.set_load_action(MTLLoadAction::DontCare);
                            ca2.set_store_action(MTLStoreAction::Store);
                            let enc2 = cmd.new_render_command_encoder(&rpd2);
                            enc2.set_render_pipeline_state(&self.pso_blur);
                            if let Some(sam) = &self.sampler {
                                enc2.set_fragment_sampler_state(0, Some(sam));
                            }
                            enc2.set_fragment_texture(0, Some(qtmp));
                            let params_v: [f32; 4] = [0.0, 1.0, 6.0, 0.0];
                            enc2.set_vertex_bytes(
                                1,
                                core::mem::size_of_val(&vp_dp) as u64,
                                vp_dp.as_ptr() as *const _,
                            );
                            enc2.set_vertex_bytes(
                                0,
                                core::mem::size_of_val(&rect_dp) as u64,
                                rect_dp.as_ptr() as *const _,
                            );
                            enc2.set_fragment_bytes(
                                1,
                                core::mem::size_of_val(&params_v) as u64,
                                params_v.as_ptr() as *const _,
                            );
                            enc2.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                            enc2.end_encoding();
                            self.cam_blur_tex = Some(q.to_owned());
                        }
                        self.cam_last_update = Some(std::time::Instant::now());
                        blur_ms_out = blur_t0.elapsed().as_secs_f64() * 1000.0;
                        blur_updated = 1;
                    }
                }
            }
        }

        // Pre-render cacheable layers into textures
        {
            let mut i = 0usize;
            while i < list.items.len() {
                if let api::DrawCmd::LayerBegin { id, rect, dirty } = &list.items[i] {
                    // find end
                    let mut depth = 1usize;
                    let mut j = i + 1;
                    let mut unsupported = false;
                    while j < list.items.len() && depth > 0 {
                        match &list.items[j] {
                            api::DrawCmd::LayerBegin { .. } => depth += 1,
                            api::DrawCmd::LayerEnd => depth -= 1,
                            api::DrawCmd::Solid { .. } | api::DrawCmd::Backdrop { .. } => {
                                unsupported = true
                            }
                            _ => {}
                        }
                        j += 1;
                    }
                    let end = j - 1;
                    if !unsupported {
                        // Build offset sublist like in encode_draws
                        let ox = rect.x;
                        let oy = rect.y;
                        let mut sub = api::DrawList {
                            items: alloc::vec::Vec::new(),
                            vertices: alloc::vec::Vec::new(),
                            indices: alloc::vec::Vec::new(),
                        };
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        for k in i + 1..end {
                            match &list.items[k] {
                                api::DrawCmd::ClipPush { rect: r0 } => {
                                    let mut rr = *r0;
                                    rr.x -= ox as i32;
                                    rr.y -= oy as i32;
                                    sub.items.push(api::DrawCmd::ClipPush { rect: rr });
                                }
                                api::DrawCmd::ClipPop => sub.items.push(api::DrawCmd::ClipPop),
                                api::DrawCmd::RRect { rect: r0, radii, color } => {
                                    let adj = api::RectF::new(r0.x - ox, r0.y - oy, r0.w, r0.h);
                                    sub.items.push(api::DrawCmd::RRect {
                                        rect: adj,
                                        radii: *radii,
                                        color: *color,
                                    });
                                }
                                api::DrawCmd::NineSlice { tex, rect: r0, slice, alpha } => {
                                    let adj = api::RectF::new(r0.x - ox, r0.y - oy, r0.w, r0.h);
                                    sub.items.push(api::DrawCmd::NineSlice {
                                        tex: *tex,
                                        rect: adj,
                                        slice: *slice,
                                        alpha: *alpha,
                                    });
                                }
                                api::DrawCmd::Image { tex, dst, src, alpha } => {
                                    let adj = api::RectF::new(dst.x - ox, dst.y - oy, dst.w, dst.h);
                                    sub.items.push(api::DrawCmd::Image {
                                        tex: *tex,
                                        dst: adj,
                                        src: *src,
                                        alpha: *alpha,
                                    });
                                }
                                api::DrawCmd::Spinner {
                                    center,
                                    radius,
                                    thickness,
                                    phase,
                                    alpha,
                                } => {
                                    let adj = [center[0] - ox, center[1] - oy];
                                    sub.items.push(api::DrawCmd::Spinner {
                                        center: adj,
                                        radius: *radius,
                                        thickness: *thickness,
                                        phase: *phase,
                                        alpha: *alpha,
                                    });
                                }
                                api::DrawCmd::GlyphRun { run } => {
                                    let v_count = run.vb.len as usize;
                                    let i_count = run.ib.len as usize;
                                    let new_v_off = sub.vertices.len() as u32;
                                    let srcv = &list.vertices[(run.vb.offset as usize)
                                        ..(run.vb.offset as usize + v_count)];
                                    for v in srcv.iter() {
                                        let mut vv = *v;
                                        vv.x -= ox;
                                        vv.y -= oy;
                                        sub.vertices.push(vv);
                                    }
                                    let srci = &list.indices[(run.ib.offset as usize)
                                        ..(run.ib.offset as usize + i_count)];
                                    let base = run.vb.offset;
                                    for idx in srci.iter() {
                                        let rebased = (*idx as u32)
                                            .wrapping_sub(base)
                                            .wrapping_add(new_v_off);
                                        sub.indices.push(rebased as u16);
                                    }
                                    sub.items.push(api::DrawCmd::GlyphRun {
                                        run: api::GlyphRun {
                                            atlas: run.atlas,
                                            vb: api::VertexSpan {
                                                offset: new_v_off,
                                                len: v_count as u32,
                                            },
                                            ib: api::IndexSpan {
                                                offset: (sub.indices.len() as u32)
                                                    .wrapping_sub(i_count as u32),
                                                len: i_count as u32,
                                            },
                                            sdf: run.sdf,
                                            color: run.color,
                                        },
                                    });
                                }
                                _ => {}
                            }
                        }
                        // Hash: use number of items and vertex count
                        use std::hash::Hash;
                        (sub.items.len() as u64).hash(&mut hasher);
                        (sub.vertices.len() as u64).hash(&mut hasher);
                        let hash = hasher.finish();
                        let w_px = (rect.w * self.target_scale.max(1.0)).ceil() as u32;
                        let h_px = (rect.h * self.target_scale.max(1.0)).ceil() as u32;
                        let need = *dirty
                            || !self.layers.get(id).is_some()
                            || self
                                .layers
                                .get(id)
                                .map(|e| e.w != w_px || e.h != h_px || e.hash != hash)
                                .unwrap_or(true);
                        if need {
                            let d = TextureDescriptor::new();
                            d.set_pixel_format(self.color_format);
                            d.set_texture_type(MTLTextureType::D2);
                            d.set_width(w_px as u64);
                            d.set_height(h_px as u64);
                            d.set_storage_mode(MTLStorageMode::Private);
                            d.set_usage(
                                MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead,
                            );
                            let tex = self.device.new_texture(&d);
                            let rpdl = RenderPassDescriptor::new();
                            let ca_l = rpdl.color_attachments().object_at(0).unwrap();
                            ca_l.set_texture(Some(&tex));
                            ca_l.set_load_action(MTLLoadAction::Clear);
                            ca_l.set_clear_color(MTLClearColor {
                                red: 0.0,
                                green: 0.0,
                                blue: 0.0,
                                alpha: 0.0,
                            });
                            ca_l.set_store_action(MTLStoreAction::Store);
                            let encl = cmd.new_render_command_encoder(&rpdl);
                            let mut pf_l = PerFrame::new();
                            // Temporarily change viewport values
                            let old_w = self.target_w;
                            let old_h = self.target_h;
                            let old_scale = self.target_scale;
                            self.target_w = w_px;
                            self.target_h = h_px;
                            self.target_scale = 1.0;
                            encode_draws(&encl, &mut pf_l, self, &sub, false, None);
                            self.target_w = old_w;
                            self.target_h = old_h;
                            self.target_scale = old_scale;
                            encl.end_encoding();
                            self.layers.insert(*id, LayerEntry { tex, w: w_px, h: h_px, hash });
                        }
                    }
                    i = end + 1;
                    continue;
                }
                i += 1;
            }
        }

        // Effects prepass: if there is any Backdrop, render a prepass and blur it.
        let has_backdrop = list.items.iter().any(|c| matches!(c, api::DrawCmd::Backdrop { .. }));
        if has_backdrop {
            self.ensure_effect_targets();
            // 1) Prepass: render up to the first Backdrop into prepass_tex
            let rpd0 = RenderPassDescriptor::new();
            let ca_pre = rpd0.color_attachments().object_at(0).unwrap();
            if let Some(src) = &self.prepass_tex {
                ca_pre.set_texture(Some(src));
            }
            ca_pre.set_load_action(MTLLoadAction::Clear);
            ca_pre.set_clear_color(MTLClearColor { red: 1.0, green: 1.0, blue: 1.0, alpha: 1.0 });
            ca_pre.set_store_action(MTLStoreAction::Store);
            let enc0 = cmd.new_render_command_encoder(&rpd0);
            // Move out per-frame to avoid double-borrow
            let mut pf0 = core::mem::take(&mut self.frames[slot]);
            // Compute prepass scissor: union of Backdrop rects (expanded) intersect frame scissor if enabled
            let mut prepass_scissor_dp: Option<api::RectI> = None;
            {
                let mut sigma = 6.0f32;
                let s = self.target_scale.max(1.0);
                let mut x0 = self.target_w as i32;
                let mut y0 = self.target_h as i32;
                let mut x1 = 0i32;
                let mut y1 = 0i32;
                let mut found_any = false;
                for c in &list.items {
                    if let api::DrawCmd::Backdrop { rect, sigma: sg, .. } = c {
                        if *sg > sigma {
                            sigma = *sg;
                        }
                        let margin = (3.0 * *sg).ceil();
                        let rx0 = (rect.x - margin).floor() as i32;
                        let ry0 = (rect.y - margin).floor() as i32;
                        let rx1 = (rect.x + rect.w + margin).ceil() as i32;
                        let ry1 = (rect.y + rect.h + margin).ceil() as i32;
                        x0 = x0.min(rx0);
                        y0 = y0.min(ry0);
                        x1 = x1.max(rx1);
                        y1 = y1.max(ry1);
                        found_any = true;
                    }
                }
                if found_any {
                    // Clamp to framebuffer dp bounds
                    let x0c = x0.clamp(0, (self.target_w as f32 / s) as i32);
                    let y0c = y0.clamp(0, (self.target_h as f32 / s) as i32);
                    let x1c = x1.clamp(0, (self.target_w as f32 / s) as i32);
                    let y1c = y1.clamp(0, (self.target_h as f32 / s) as i32);
                    let rx = x0c.max(0);
                    let ry = y0c.max(0);
                    let rw = (x1c - x0c).max(0);
                    let rh = (y1c - y0c).max(0);
                    let mut rect = api::RectI { x: rx, y: ry, w: rw, h: rh };
                    // Intersect with frame damage scissor if enabled
                    if self.damage_enabled {
                        if let Some(g) = self.frame_scissor_dp {
                            // intersect dp
                            let ix0 = rect.x.max(g.x);
                            let iy0 = rect.y.max(g.y);
                            let ix1 = (rect.x + rect.w).min(g.x + g.w);
                            let iy1 = (rect.y + rect.h).min(g.y + g.h);
                            let iw = (ix1 - ix0).max(0);
                            let ih = (iy1 - iy0).max(0);
                            rect = api::RectI { x: ix0, y: iy0, w: iw, h: ih };
                        }
                    }
                    if rect.w > 0 && rect.h > 0 {
                        prepass_scissor_dp = Some(rect);
                    }
                }
            }
            // Heuristics: drop prepass scissor when damage coverage is large
            let dmg_thresh: f32 = self.damage_use_thresh;
            if prepass_scissor_dp.is_some() && self.frame_damage_pct >= dmg_thresh {
                prepass_scissor_dp = None;
            }
            // Optional pre-filtering by prepass scissor only when damage is small
            let filtered_prepass;
            let list_pre_ref = if let Some(sc_dp) = prepass_scissor_dp {
                if self.frame_damage_pct <= self.damage_prefilter_thresh {
                    filtered_prepass = filter_drawlist_by_dp_scissor(list, sc_dp);
                    if filtered_prepass.items.len() < list.items.len() {
                        self.acc_culled = self.acc_culled.saturating_add(
                            (list.items.len() - filtered_prepass.items.len()) as u32,
                        );
                    }
                    &filtered_prepass
                } else {
                    list
                }
            } else {
                list
            };
            encode_draws(&enc0, &mut pf0, self, list_pre_ref, true, prepass_scissor_dp);
            self.frames[slot] = pf0;
            enc0.end_encoding();

            // Determine blur kernel and union scissor in pixel coords for all Backdrop rects
            let mut sigma = 6.0f32;
            let mut u_x0: i32 = self.target_w as i32;
            let mut u_y0: i32 = self.target_h as i32;
            let mut u_x1: i32 = 0;
            let mut u_y1: i32 = 0;
            let scale = self.target_scale.max(1.0);
            let mut found_any = false;
            for c in &list.items {
                if let api::DrawCmd::Backdrop { rect, sigma: s, .. } = c {
                    if *s > sigma {
                        sigma = *s;
                    }
                    // Expand by ~3*sigma kernel radius, convert to px then clamp
                    let margin = (3.0 * *s).ceil();
                    let x0 = ((rect.x - margin) * scale).floor() as i32;
                    let y0 = ((rect.y - margin) * scale).floor() as i32;
                    let x1 = ((rect.x + rect.w + margin) * scale).ceil() as i32;
                    let y1 = ((rect.y + rect.h + margin) * scale).ceil() as i32;
                    u_x0 = u_x0.min(x0);
                    u_y0 = u_y0.min(y0);
                    u_x1 = u_x1.max(x1);
                    u_y1 = u_y1.max(y1);
                    found_any = true;
                }
            }
            if !found_any {
                sigma = 6.0;
                u_x0 = 0;
                u_y0 = 0;
                u_x1 = self.target_w as i32;
                u_y1 = self.target_h as i32;
            }
            // Clamp to framebuffer bounds and ensure non-negative width/height
            let x0c = u_x0.clamp(0, self.target_w as i32);
            let y0c = u_y0.clamp(0, self.target_h as i32);
            let x1c = u_x1.clamp(0, self.target_w as i32);
            let y1c = u_y1.clamp(0, self.target_h as i32);
            let sc_x = x0c.max(0) as u64;
            let sc_y = y0c.max(0) as u64;
            let sc_w = (x1c - x0c).max(0) as u64;
            let sc_h = (y1c - y0c).max(0) as u64;

            // 2) Downsample: prepass_tex -> half_tex -> quarter_tex
            let sc_half = MTLScissorRect {
                x: sc_x / 2,
                y: sc_y / 2,
                width: (sc_w / 2).max(0),
                height: (sc_h / 2).max(0),
            };
            let sc_quarter = MTLScissorRect {
                x: sc_x / 4,
                y: sc_y / 4,
                width: (sc_w / 4).max(0),
                height: (sc_h / 4).max(0),
            };

            // prepass -> half
            let rpd_ds1 = RenderPassDescriptor::new();
            let ca_ds1 = rpd_ds1.color_attachments().object_at(0).unwrap();
            if let Some(dst) = &self.half_tex {
                ca_ds1.set_texture(Some(dst));
            }
            ca_ds1.set_load_action(MTLLoadAction::DontCare);
            ca_ds1.set_store_action(MTLStoreAction::Store);
            let enc_ds1 = cmd.new_render_command_encoder(&rpd_ds1);
            enc_ds1.set_render_pipeline_state(&self.pso_downsample);
            if let Some(sam) = &self.sampler {
                enc_ds1.set_fragment_sampler_state(0, Some(sam));
            }
            if let Some(src) = &self.prepass_tex {
                enc_ds1.set_fragment_texture(0, Some(src));
            }
            enc_ds1.set_scissor_rect(sc_half);
            enc_ds1.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
            self.prepass_shaded_px =
                self.prepass_shaded_px.saturating_add(sc_half.width.saturating_mul(sc_half.height));
            enc_ds1.end_encoding();

            // half -> quarter
            let rpd_ds2 = RenderPassDescriptor::new();
            let ca_ds2 = rpd_ds2.color_attachments().object_at(0).unwrap();
            if let Some(dst) = &self.quarter_tex {
                ca_ds2.set_texture(Some(dst));
            }
            ca_ds2.set_load_action(MTLLoadAction::DontCare);
            ca_ds2.set_store_action(MTLStoreAction::Store);
            let enc_ds2 = cmd.new_render_command_encoder(&rpd_ds2);
            enc_ds2.set_render_pipeline_state(&self.pso_downsample);
            if let Some(sam) = &self.sampler {
                enc_ds2.set_fragment_sampler_state(0, Some(sam));
            }
            if let Some(src) = &self.half_tex {
                enc_ds2.set_fragment_texture(0, Some(src));
            }
            enc_ds2.set_scissor_rect(sc_quarter);
            enc_ds2.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
            self.prepass_shaded_px = self
                .prepass_shaded_px
                .saturating_add(sc_quarter.width.saturating_mul(sc_quarter.height));
            enc_ds2.end_encoding();

            // 3) Blur H at quarter: quarter -> quarter_tmp
            let rpd1 = RenderPassDescriptor::new();
            let ca_blur_h = rpd1.color_attachments().object_at(0).unwrap();
            if let Some(tmp) = &self.quarter_tmp_tex {
                ca_blur_h.set_texture(Some(tmp));
            }
            ca_blur_h.set_load_action(MTLLoadAction::DontCare);
            ca_blur_h.set_store_action(MTLStoreAction::Store);
            let enc1 = cmd.new_render_command_encoder(&rpd1);
            enc1.set_render_pipeline_state(&self.pso_blur);
            if let Some(sam) = &self.sampler {
                enc1.set_fragment_sampler_state(0, Some(sam));
            }
            if let Some(src) = &self.quarter_tex {
                enc1.set_fragment_texture(0, Some(src));
            }
            enc1.set_scissor_rect(sc_quarter);
            let params_h: [f32; 4] = [1.0, 0.0, sigma / 4.0, 0.0];
            enc1.set_fragment_bytes(
                1,
                core::mem::size_of_val(&params_h) as u64,
                params_h.as_ptr() as *const _,
            );
            enc1.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
            self.prepass_shaded_px = self
                .prepass_shaded_px
                .saturating_add(sc_quarter.width.saturating_mul(sc_quarter.height));
            enc1.end_encoding();

            // 4) Blur V at quarter: quarter_tmp -> quarter
            let rpd2 = RenderPassDescriptor::new();
            let ca_blur_v = rpd2.color_attachments().object_at(0).unwrap();
            if let Some(dst) = &self.quarter_tex {
                ca_blur_v.set_texture(Some(dst));
            }
            ca_blur_v.set_load_action(MTLLoadAction::DontCare);
            ca_blur_v.set_store_action(MTLStoreAction::Store);
            let enc2 = cmd.new_render_command_encoder(&rpd2);
            enc2.set_render_pipeline_state(&self.pso_blur);
            if let Some(sam) = &self.sampler {
                enc2.set_fragment_sampler_state(0, Some(sam));
            }
            if let Some(tmp) = &self.quarter_tmp_tex {
                enc2.set_fragment_texture(0, Some(tmp));
            }
            enc2.set_scissor_rect(sc_quarter);
            let params_v: [f32; 4] = [0.0, 1.0, sigma / 4.0, 0.0];
            enc2.set_fragment_bytes(
                1,
                core::mem::size_of_val(&params_v) as u64,
                params_v.as_ptr() as *const _,
            );
            enc2.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
            self.prepass_shaded_px = self
                .prepass_shaded_px
                .saturating_add(sc_quarter.width.saturating_mul(sc_quarter.height));
            enc2.end_encoding();

            // 5) Upsample quarter -> half (scale 2)
            let rpd_us1 = RenderPassDescriptor::new();
            let ca_us1 = rpd_us1.color_attachments().object_at(0).unwrap();
            if let Some(dst) = &self.half_tex {
                ca_us1.set_texture(Some(dst));
            }
            ca_us1.set_load_action(MTLLoadAction::DontCare);
            ca_us1.set_store_action(MTLStoreAction::Store);
            let enc_us1 = cmd.new_render_command_encoder(&rpd_us1);
            enc_us1.set_render_pipeline_state(&self.pso_upsample);
            if let Some(sam) = &self.sampler {
                enc_us1.set_fragment_sampler_state(0, Some(sam));
            }
            if let Some(src) = &self.quarter_tex {
                enc_us1.set_fragment_texture(0, Some(src));
            }
            let scale2: f32 = 2.0;
            enc_us1.set_fragment_bytes(
                1,
                core::mem::size_of_val(&scale2) as u64,
                &scale2 as *const _ as *const _,
            );
            enc_us1.set_scissor_rect(sc_half);
            enc_us1.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
            self.prepass_shaded_px =
                self.prepass_shaded_px.saturating_add(sc_half.width.saturating_mul(sc_half.height));
            enc_us1.end_encoding();

            // 6) Upsample half -> prepass (scale 2)
            let rpd_us2 = RenderPassDescriptor::new();
            let ca_us2 = rpd_us2.color_attachments().object_at(0).unwrap();
            if let Some(dst) = &self.prepass_tex {
                ca_us2.set_texture(Some(dst));
            }
            ca_us2.set_load_action(MTLLoadAction::DontCare);
            ca_us2.set_store_action(MTLStoreAction::Store);
            let enc_us2 = cmd.new_render_command_encoder(&rpd_us2);
            enc_us2.set_render_pipeline_state(&self.pso_upsample);
            if let Some(sam) = &self.sampler {
                enc_us2.set_fragment_sampler_state(0, Some(sam));
            }
            if let Some(src) = &self.half_tex {
                enc_us2.set_fragment_texture(0, Some(src));
            }
            enc_us2.set_fragment_bytes(
                1,
                core::mem::size_of_val(&scale2) as u64,
                &scale2 as *const _ as *const _,
            );
            enc_us2.set_scissor_rect(MTLScissorRect {
                x: sc_x,
                y: sc_y,
                width: sc_w,
                height: sc_h,
            });
            enc_us2.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
            self.prepass_shaded_px =
                self.prepass_shaded_px.saturating_add(sc_w.saturating_mul(sc_h));
            enc_us2.end_encoding();
        }

        let rpd = RenderPassDescriptor::new();
        let ca0 = rpd.color_attachments().object_at(0).unwrap();
        if let Some(dst) = &self.target_tex {
            ca0.set_texture(Some(dst));
        }
        // Heuristic: use Load (damage) only when enabled and coverage < threshold
        let dmg_thresh: f32 = self.damage_use_thresh;
        let use_damage = self.damage_enabled
            && self.frame_scissor_dp.is_some()
            && self.frame_damage_pct < dmg_thresh;
        if use_damage {
            ca0.set_load_action(MTLLoadAction::Load);
        } else {
            ca0.set_load_action(MTLLoadAction::Clear);
        }
        ca0.set_clear_color(MTLClearColor { red: 1.0, green: 1.0, blue: 1.0, alpha: 1.0 });
        ca0.set_store_action(MTLStoreAction::Store);
        let enc = cmd.new_render_command_encoder(&rpd);
        // Move out per-frame to avoid double-borrow on &mut self
        let mut pf = core::mem::take(&mut self.frames[slot]);
        // Optional pre-filtering by frame scissor to reduce CPU work (small damage only)
        let list_main_storage;
        let list_main_ref: &api::DrawList = if use_damage {
            if let Some(sc) = self.frame_scissor_dp {
                if self.frame_damage_pct <= self.damage_prefilter_thresh {
                    list_main_storage = filter_drawlist_by_dp_scissor(list, sc);
                    if list_main_storage.items.len() < list.items.len() {
                        self.acc_culled = self.acc_culled.saturating_add(
                            (list.items.len() - list_main_storage.items.len()) as u32,
                        );
                    }
                    &list_main_storage
                } else {
                    list
                }
            } else {
                list
            }
        } else {
            list
        };
        encode_draws(
            &enc,
            &mut pf,
            self,
            list_main_ref,
            false,
            if use_damage { self.frame_scissor_dp } else { None },
        );
        self.frames[slot] = pf;
        enc.end_encoding();

        // Snapshot last stats
        self.last_stats.vb_bytes = self.frames[slot].vb_used as u64;
        self.last_stats.ub_bytes = self.frames[slot].ub_used as u64;
        self.last_stats.ib_bytes = self.frames[slot].ib_used as u64;
        self.last_stats.draws = self.acc_draws;
        self.last_stats.instanced = self.acc_instanced;
        self.last_stats.icb_cmds = self.acc_icb_cmds;
        self.last_stats.encode_ms = cpu_t0.elapsed().as_secs_f64() * 1000.0;
        self.last_stats.damage_px = self.frame_damage_px;
        self.last_stats.damage_pct = self.frame_damage_pct;
        self.last_stats.damage_rects = self.frame_damage_rects;
        self.last_stats.culled = self.acc_culled;
        // Adaptive stats
        self.last_stats.blur_ms = blur_ms_out;
        self.last_stats.blur_updates = blur_updated;
        self.last_stats.blur_period_ms =
            (self.cam_update_period.as_millis() as u64).min(u64::from(u32::MAX)) as u32;
        self.last_stats.cam_coverage_pct = cam_coverage;
        self.last_stats.cam_paused = if self.cam_paused { 1 } else { 0 };
        self.last_stats.thermal = therm as u8;
        self.last_stats.low_power = if lpm { 1 } else { 0 };
        self.last_stats.cam_width = self.last_cam_w.max(0) as u32;
        self.last_stats.cam_height = self.last_cam_h.max(0) as u32;
        self.last_stats.cam_bit_depth = self.last_cam_bd.max(0) as u8;
        self.last_stats.cam_matrix = self.last_cam_mx.max(0) as u8;
        self.last_stats.cam_video_range = self.last_cam_vr.max(0) as u8;
        self.last_stats.cam_color_space = self.last_cam_cs.max(0) as u8;
    }

    fn submit(&mut self, _token: api::FrameToken) -> Result<(), api::RenderError> {
        let slot = (self.frame_id % 3) as usize;
        if let Some(cmd) = self.frames[slot].cmd.take() {
            let pf_ptr: *mut PerFrame = &mut self.frames[slot];
            let block = ConcreteBlock::new(move |_cb: &CommandBufferRef| unsafe {
                (*pf_ptr).completed();
            })
            .copy();
            cmd.add_completed_handler(&block);
            cmd.commit();
        }
        Ok(())
    }

    fn resize(&mut self, w: u32, h: u32, scale: f32) -> Result<(), api::RenderError> {
        self.target_w = w.max(1);
        self.target_h = h.max(1);
        self.target_scale = if scale > 0.0 { scale } else { 1.0 };
        self.ensure_target();
        Ok(())
    }
}

fn encode_draws(
    enc: &RenderCommandEncoderRef,
    pf: &mut PerFrame,
    r: &mut MetalRenderer,
    list: &api::DrawList,
    prepass: bool,
    global_scissor_dp: Option<api::RectI>,
) {
    // Scissor state
    let mut stack: alloc::vec::Vec<api::RectI> = alloc::vec::Vec::new();
    let mut current: Option<api::RectI> = None;
    let mut last_applied: Option<api::RectI> = None;

    let vp_dp: [f32; 2] = [
        (r.target_w as f32) / r.target_scale.max(1.0),
        (r.target_h as f32) / r.target_scale.max(1.0),
    ];

    let mut i: usize = 0;
    while i < list.items.len() {
        match &list.items[i] {
            api::DrawCmd::CameraBg { rect, tint, alpha, grayscale, blur, sigma } => {
                // Only supported on iOS Metal; no-op elsewhere
                #[cfg(target_os = "ios")]
                {
                    if *blur {
                        if let Some(src) = &r.cam_blur_tex {
                            let a = (tint.a * *alpha).clamp(0.0, 1.0);
                            let s = r.target_scale.max(1.0);
                            let vbuf: [f32; 4] = [rect.x, rect.y, rect.w, rect.h];
                            let base_fb: [f32; 8] = [
                                rect.x * s,
                                rect.y * s,
                                rect.w * s,
                                rect.h * s,
                                tint.r,
                                tint.g,
                                tint.b,
                                a,
                            ];
                            let mut fade_prev = 0.0f32;
                            let mut fade_cur = 1.0f32;
                            if let Some(t0) = r.cam_xfade_t0 {
                                let dt = t0.elapsed().as_millis() as u32;
                                let ms = r.cam_xfade_ms.max(1);
                                let f = (dt as f32 / ms as f32).clamp(0.0, 1.0);
                                fade_prev = 1.0 - f;
                                fade_cur = f;
                            } else if let Some(t0) = r.cam_blur_fade_t0 {
                                let dt = t0.elapsed().as_millis() as u32;
                                let ms = r.cam_xfade_ms.max(1);
                                let f = (dt as f32 / ms as f32).clamp(0.0, 1.0);
                                fade_prev = 0.0;
                                fade_cur = f;
                                // Draw raw NV12 base with (1 - f)
                                if fade_cur < 1.0 {
                                    extern "C" {
                                        fn oxideui_cam_get_latest_ex(
                                            y_tex: *mut *mut core::ffi::c_void,
                                            uv_tex: *mut *mut core::ffi::c_void,
                                            w: *mut i32,
                                            h: *mut i32,
                                            bitdepth: *mut i32,
                                            matrix: *mut i32,
                                            video_range: *mut i32,
                                            colorspace: *mut i32,
                                        ) -> ::libc::c_int;
                                    }
                                    let mut y_ptr: *mut core::ffi::c_void = core::ptr::null_mut();
                                    let mut uv_ptr: *mut core::ffi::c_void = core::ptr::null_mut();
                                    let mut cw: i32 = 0;
                                    let mut ch: i32 = 0;
                                    let mut bd: i32 = 8;
                                    let mut mx: i32 = 0;
                                    let mut vr: i32 = 0;
                                    let mut cs: i32 = 0;
                                    let ok2 = unsafe {
                                        oxideui_cam_get_latest_ex(
                                            &mut y_ptr,
                                            &mut uv_ptr,
                                            &mut cw,
                                            &mut ch,
                                            &mut bd,
                                            &mut mx,
                                            &mut vr,
                                            &mut cs,
                                        )
                                    };
                                    if ok2 != 0 && !y_ptr.is_null() && !uv_ptr.is_null() {
                                        let y_tex =
                                            unsafe { Texture::from_ptr(y_ptr as *mut MTLTexture) };
                                        let uv_tex =
                                            unsafe { Texture::from_ptr(uv_ptr as *mut MTLTexture) };
                                        enc.set_render_pipeline_state(&r.pso_camera);
                                        if let Some(sam) = &r.sampler {
                                            enc.set_fragment_sampler_state(0, Some(sam));
                                        }
                                        enc.set_fragment_texture(0, Some(&y_tex));
                                        enc.set_fragment_texture(1, Some(&uv_tex));
                                        enc.set_vertex_bytes(
                                            1,
                                            core::mem::size_of_val(&vp_dp) as u64,
                                            vp_dp.as_ptr() as *const _,
                                        );
                                        enc.set_vertex_bytes(
                                            0,
                                            core::mem::size_of_val(&vbuf) as u64,
                                            vbuf.as_ptr() as *const _,
                                        );
                                        // Aspect-fill params
                                        let ar_dest =
                                            if rect.h > 0.0 { rect.w / rect.h } else { 1.0 };
                                        let ar_cam =
                                            if ch > 0 { (cw as f32) / (ch as f32) } else { 1.0 };
                                        let (mut sx, mut sy) = (1.0f32, 1.0f32);
                                        let (mut bx, mut by) = (0.0f32, 0.0f32);
                                        if ar_cam > ar_dest {
                                            sx = ar_dest / ar_cam;
                                            bx = (1.0 - sx) * 0.5;
                                        } else if ar_cam < ar_dest {
                                            sy = ar_cam / ar_dest;
                                            by = (1.0 - sy) * 0.5;
                                        }
                                        let rect_px =
                                            [rect.x * s, rect.y * s, rect.w * s, rect.h * s];
                                        let fb_cam: [f32; 16] = [
                                            rect_px[0],
                                            rect_px[1],
                                            rect_px[2],
                                            rect_px[3],
                                            tint.r,
                                            tint.g,
                                            tint.b,
                                            a * (1.0 - fade_cur),
                                            sx,
                                            sy,
                                            bx,
                                            by,
                                            if *grayscale { 1.0 } else { 0.0 },
                                            mx as f32,
                                            vr as f32,
                                            bd as f32,
                                        ];
                                        enc.set_fragment_bytes(
                                            1,
                                            core::mem::size_of_val(&fb_cam) as u64,
                                            fb_cam.as_ptr() as *const _,
                                        );
                                        enc.draw_primitives_instanced(
                                            MTLPrimitiveType::Triangle,
                                            0,
                                            6,
                                            1,
                                        );
                                        r.acc_instanced += 1;
                                    }
                                }
                            }
                            enc.set_render_pipeline_state(&r.pso_backdrop);
                            if let Some(sam) = &r.sampler {
                                enc.set_fragment_sampler_state(0, Some(sam));
                            }
                            enc.set_vertex_bytes(
                                1,
                                core::mem::size_of_val(&vp_dp) as u64,
                                vp_dp.as_ptr() as *const _,
                            );
                            enc.set_vertex_bytes(
                                0,
                                core::mem::size_of_val(&vbuf) as u64,
                                vbuf.as_ptr() as *const _,
                            );
                            // Draw previous blurred
                            if fade_prev > 0.0 {
                                if let Some(prev) = &r.cam_xfade_prev_tex {
                                    enc.set_fragment_texture(0, Some(prev));
                                    let mut fb = base_fb;
                                    fb[7] = a * fade_prev;
                                    enc.set_fragment_bytes(
                                        1,
                                        core::mem::size_of_val(&fb) as u64,
                                        fb.as_ptr() as *const _,
                                    );
                                    enc.draw_primitives_instanced(
                                        MTLPrimitiveType::Triangle,
                                        0,
                                        6,
                                        1,
                                    );
                                    r.acc_instanced += 1;
                                }
                            }
                            // Draw current blurred
                            enc.set_fragment_texture(0, Some(src));
                            let mut fb = base_fb;
                            fb[7] = a * fade_cur;
                            enc.set_fragment_bytes(
                                1,
                                core::mem::size_of_val(&fb) as u64,
                                fb.as_ptr() as *const _,
                            );
                            enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                            r.acc_instanced += 1;
                        }
                    } else {
                        extern "C" {
                            fn oxideui_cam_get_latest_ex(
                                y_tex: *mut *mut core::ffi::c_void,
                                uv_tex: *mut *mut core::ffi::c_void,
                                w: *mut i32,
                                h: *mut i32,
                                bitdepth: *mut i32,
                                matrix: *mut i32,
                                video_range: *mut i32,
                                colorspace: *mut i32,
                            ) -> ::libc::c_int;
                        }
                        let mut y_ptr: *mut core::ffi::c_void = core::ptr::null_mut();
                        let mut uv_ptr: *mut core::ffi::c_void = core::ptr::null_mut();
                        let mut cw: i32 = 0;
                        let mut ch: i32 = 0;
                        let mut bd: i32 = 8;
                        let mut mx: i32 = 0;
                        let mut vr: i32 = 0;
                        let mut cs: i32 = 0;
                        let ok = unsafe {
                            oxideui_cam_get_latest_ex(
                                &mut y_ptr,
                                &mut uv_ptr,
                                &mut cw,
                                &mut ch,
                                &mut bd,
                                &mut mx,
                                &mut vr,
                                &mut cs,
                            )
                        };
                        if ok != 0 && !y_ptr.is_null() && !uv_ptr.is_null() && cw > 0 && ch > 0 {
                            let y_tex = unsafe { Texture::from_ptr(y_ptr as *mut MTLTexture) };
                            let uv_tex = unsafe { Texture::from_ptr(uv_ptr as *mut MTLTexture) };
                            enc.set_render_pipeline_state(&r.pso_camera);
                            if let Some(sam) = &r.sampler {
                                enc.set_fragment_sampler_state(0, Some(sam));
                            }
                            enc.set_fragment_texture(0, Some(&y_tex));
                            enc.set_fragment_texture(1, Some(&uv_tex));
                            let s = r.target_scale.max(1.0);
                            let rect_px = [rect.x * s, rect.y * s, rect.w * s, rect.h * s];
                            let ar_dest = if rect.h > 0.0 { rect.w / rect.h } else { 1.0 };
                            let ar_cam = (cw as f32) / (ch as f32);
                            let (mut sx, mut sy) = (1.0f32, 1.0f32);
                            let (mut bx, mut by) = (0.0f32, 0.0f32);
                            if ar_cam > ar_dest {
                                sx = ar_dest / ar_cam;
                                bx = (1.0 - sx) * 0.5;
                            } else if ar_cam < ar_dest {
                                sy = ar_cam / ar_dest;
                                by = (1.0 - sy) * 0.5;
                            }
                            let a = (tint.a * *alpha).clamp(0.0, 1.0);
                            let fbuf_cam: [f32; 16] = [
                                rect_px[0],
                                rect_px[1],
                                rect_px[2],
                                rect_px[3],
                                tint.r,
                                tint.g,
                                tint.b,
                                a,
                                sx,
                                sy,
                                bx,
                                by,
                                if *grayscale { 1.0 } else { 0.0 },
                                mx as f32,
                                vr as f32,
                                bd as f32,
                            ];
                            let vbuf_cam: [f32; 4] = [rect.x, rect.y, rect.w, rect.h];
                            enc.set_vertex_bytes(
                                1,
                                core::mem::size_of_val(&vp_dp) as u64,
                                vp_dp.as_ptr() as *const _,
                            );
                            enc.set_vertex_bytes(
                                0,
                                core::mem::size_of_val(&vbuf_cam) as u64,
                                vbuf_cam.as_ptr() as *const _,
                            );
                            enc.set_fragment_bytes(
                                1,
                                core::mem::size_of_val(&fbuf_cam) as u64,
                                fbuf_cam.as_ptr() as *const _,
                            );
                            enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, 1);
                            r.acc_draws = r.acc_draws.saturating_add(1);
                        }
                    }
                }
                // Non-iOS or failure path: do nothing
                i += 1;
                continue;
            }
            api::DrawCmd::LayerBegin { id, rect, dirty } => {
                // Find matching LayerEnd and collect sublist
                let mut depth = 1usize;
                let mut j = i + 1;
                while j < list.items.len() && depth > 0 {
                    match &list.items[j] {
                        api::DrawCmd::LayerBegin { .. } => depth += 1,
                        api::DrawCmd::LayerEnd => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                let end = j - 1; // points to LayerEnd
                                 // If in prepass, render sublist inline (no caching)
                if prepass {
                    // Encode sublist directly
                    let sub = api::DrawList {
                        items: list.items[i + 1..end].to_vec(),
                        vertices: list.vertices.clone(),
                        indices: list.indices.clone(),
                    };
                    encode_draws(enc, pf, r, &sub, true, global_scissor_dp);
                    i = end + 1;
                    continue;
                }
                // Determine if sublist contains unsupported commands (Solid)
                let mut unsupported = false;
                for k in i + 1..end {
                    if matches!(list.items[k], api::DrawCmd::Solid { .. }) {
                        unsupported = true;
                        break;
                    }
                }
                if unsupported {
                    // Fallback to inline encode
                    let sub = api::DrawList {
                        items: list.items[i + 1..end].to_vec(),
                        vertices: list.vertices.clone(),
                        indices: list.indices.clone(),
                    };
                    encode_draws(enc, pf, r, &sub, false, global_scissor_dp);
                    i = end + 1;
                    continue;
                }
                // Build offset sublist in local coordinates (dp) and compute hash
                let ox = rect.x;
                let oy = rect.y;
                let mut sub = api::DrawList {
                    items: alloc::vec::Vec::new(),
                    vertices: alloc::vec::Vec::new(),
                    indices: alloc::vec::Vec::new(),
                };
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                for k in i + 1..end {
                    match &list.items[k] {
                        api::DrawCmd::ClipPush { rect: r0 } => {
                            let mut rr = *r0;
                            rr.x -= ox as i32;
                            rr.y -= oy as i32;
                            sub.items.push(api::DrawCmd::ClipPush { rect: rr });
                            rr.x.hash(&mut hasher);
                            rr.y.hash(&mut hasher);
                            rr.w.hash(&mut hasher);
                            rr.h.hash(&mut hasher);
                        }
                        api::DrawCmd::CameraBg {
                            rect: r0,
                            tint,
                            alpha,
                            grayscale,
                            blur,
                            sigma,
                        } => {
                            let adj = api::RectF::new(r0.x - ox, r0.y - oy, r0.w, r0.h);
                            sub.items.push(api::DrawCmd::CameraBg {
                                rect: adj,
                                tint: *tint,
                                alpha: *alpha,
                                grayscale: *grayscale,
                                blur: *blur,
                                sigma: *sigma,
                            });
                            ((adj.x.to_bits() ^ adj.y.to_bits()) as u64).hash(&mut hasher);
                        }
                        api::DrawCmd::ClipPop => sub.items.push(api::DrawCmd::ClipPop),
                        api::DrawCmd::RRect { rect: r0, radii, color } => {
                            let adj = api::RectF::new(r0.x - ox, r0.y - oy, r0.w, r0.h);
                            sub.items.push(api::DrawCmd::RRect {
                                rect: adj,
                                radii: *radii,
                                color: *color,
                            });
                            ((adj.x.to_bits() ^ adj.y.to_bits()) as u64).hash(&mut hasher);
                        }
                        api::DrawCmd::NineSlice { tex, rect: r0, slice, alpha } => {
                            let adj = api::RectF::new(r0.x - ox, r0.y - oy, r0.w, r0.h);
                            sub.items.push(api::DrawCmd::NineSlice {
                                tex: *tex,
                                rect: adj,
                                slice: *slice,
                                alpha: *alpha,
                            });
                            tex.0.hash(&mut hasher);
                        }
                        api::DrawCmd::Image { tex, dst, src, alpha } => {
                            let adj = api::RectF::new(dst.x - ox, dst.y - oy, dst.w, dst.h);
                            sub.items.push(api::DrawCmd::Image {
                                tex: *tex,
                                dst: adj,
                                src: *src,
                                alpha: *alpha,
                            });
                            tex.0.hash(&mut hasher);
                        }
                        api::DrawCmd::Spinner { center, radius, thickness, phase, alpha } => {
                            let adj = [center[0] - ox, center[1] - oy];
                            sub.items.push(api::DrawCmd::Spinner {
                                center: adj,
                                radius: *radius,
                                thickness: *thickness,
                                phase: *phase,
                                alpha: *alpha,
                            });
                        }
                        api::DrawCmd::Backdrop { rect: r0, sigma, tint, alpha } => {
                            let adj = api::RectF::new(r0.x - ox, r0.y - oy, r0.w, r0.h);
                            sub.items.push(api::DrawCmd::Backdrop {
                                rect: adj,
                                sigma: *sigma,
                                tint: *tint,
                                alpha: *alpha,
                            });
                        }
                        api::DrawCmd::GlyphRun { run } => {
                            // Copy referenced vertices/indices with rebase
                            let v_count = run.vb.len as usize;
                            let i_count = run.ib.len as usize;
                            let new_v_off = sub.vertices.len() as u32;
                            // Copy and offset verts
                            let srcv = &list.vertices
                                [(run.vb.offset as usize)..(run.vb.offset as usize + v_count)];
                            for v in srcv.iter() {
                                let mut vv = *v;
                                vv.x -= ox;
                                vv.y -= oy;
                                sub.vertices.push(vv);
                            }
                            // Copy and rebase indices
                            let srci = &list.indices
                                [(run.ib.offset as usize)..(run.ib.offset as usize + i_count)];
                            let base = run.vb.offset;
                            for idx in srci.iter() {
                                let rebased =
                                    (*idx as u32).wrapping_sub(base).wrapping_add(new_v_off);
                                sub.indices.push(rebased as u16);
                            }
                            sub.items.push(api::DrawCmd::GlyphRun {
                                run: api::GlyphRun {
                                    atlas: run.atlas,
                                    vb: api::VertexSpan { offset: new_v_off, len: v_count as u32 },
                                    ib: api::IndexSpan {
                                        offset: (sub.indices.len() as u32)
                                            .wrapping_sub(i_count as u32),
                                        len: i_count as u32,
                                    },
                                    sdf: run.sdf,
                                    color: run.color,
                                },
                            });
                        }
                        _ => {}
                    }
                }
                let hash = hasher.finish();
                // Ensure layer texture exists (px)
                let w_px = (rect.w * r.target_scale.max(1.0)).ceil() as u32;
                let h_px = (rect.h * r.target_scale.max(1.0)).ceil() as u32;
                let do_render = *dirty
                    || !r.layers.get(id).is_some()
                    || r.layers
                        .get(id)
                        .map(|e| e.w != w_px || e.h != h_px || e.hash != hash)
                        .unwrap_or(true);
                if do_render {
                    // Create/resize layer texture
                    let d = TextureDescriptor::new();
                    d.set_pixel_format(r.color_format);
                    d.set_texture_type(MTLTextureType::D2);
                    d.set_width(w_px as u64);
                    d.set_height(h_px as u64);
                    d.set_storage_mode(MTLStorageMode::Private);
                    d.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
                    let tex = r.device.new_texture(&d);
                    // Layer rendering handled in encode_pass pre-scan
                    r.layers.insert(*id, LayerEntry { tex, w: w_px, h: h_px, hash });
                }
                // Composite the cached layer via nine-slice (no slicing)
                if let Some(layer) = r.layers.get(id) {
                    enc.set_render_pipeline_state(&r.pso_nine_slice);
                    if let Some(sam) = &r.sampler {
                        enc.set_fragment_sampler_state(0, Some(sam));
                    }
                    enc.set_fragment_texture(0, Some(&layer.tex));
                    // Vertex params: rect dp + vp dp
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    let vparams: [f32; 6] = [rect.x, rect.y, rect.w, rect.h, vp_dp[0], vp_dp[1]];
                    enc.set_vertex_bytes(
                        0,
                        core::mem::size_of_val(&vparams) as u64,
                        vparams.as_ptr() as *const _,
                    );
                    // Fragment params: rect px + tex size + zero slices + alpha=1
                    let params: [f32; 12] = [
                        rect.x * r.target_scale.max(1.0),
                        rect.y * r.target_scale.max(1.0),
                        rect.w * r.target_scale.max(1.0),
                        rect.h * r.target_scale.max(1.0),
                        layer.w as f32,
                        layer.h as f32,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        1.0,
                        0.0,
                    ];
                    enc.set_fragment_bytes(
                        1,
                        core::mem::size_of_val(&params) as u64,
                        params.as_ptr() as *const _,
                    );
                    enc.draw_primitives(MTLPrimitiveType::Triangle, 0, 6);
                    r.acc_draws += 1;
                }
                i = end + 1;
                continue;
            }
            api::DrawCmd::LayerEnd => {
                i += 1;
                continue;
            }
            api::DrawCmd::ClipPush { rect } => {
                let next = if let Some(cur) = current {
                    let x1 = cur.x.max(rect.x);
                    let y1 = cur.y.max(rect.y);
                    let x2 = (cur.x + cur.w).min(rect.x + rect.w);
                    let y2 = (cur.y + cur.h).min(rect.y + rect.h);
                    if x2 > x1 && y2 > y1 {
                        api::RectI { x: x1, y: y1, w: x2 - x1, h: y2 - y1 }
                    } else {
                        api::RectI { x: 0, y: 0, w: 0, h: 0 }
                    }
                } else {
                    *rect
                };
                stack.push(*rect);
                current = Some(next);
                // Intersect with global scissor (dp)
                let effective: Option<api::RectI> = match (current, global_scissor_dp) {
                    (Some(c), Some(g)) => {
                        let x1 = c.x.max(g.x);
                        let y1 = c.y.max(g.y);
                        let x2 = (c.x + c.w).min(g.x + g.w);
                        let y2 = (c.y + c.h).min(g.y + g.h);
                        if x2 > x1 && y2 > y1 {
                            Some(api::RectI { x: x1, y: y1, w: x2 - x1, h: y2 - y1 })
                        } else {
                            Some(api::RectI { x: 0, y: 0, w: 0, h: 0 })
                        }
                    }
                    (Some(c), None) => Some(c),
                    (None, Some(g)) => Some(g),
                    (None, None) => None,
                };
                if last_applied != effective {
                    let scale = r.target_scale.max(1.0);
                    let (x, y, w, h) = match effective {
                        Some(rc) => {
                            let x = (rc.x as f32 * scale).floor() as i32;
                            let y = (rc.y as f32 * scale).floor() as i32;
                            let w = (rc.w as f32 * scale).ceil() as i32;
                            let h = (rc.h as f32 * scale).ceil() as i32;
                            (x, y, w, h)
                        }
                        None => (0, 0, r.target_w as i32, r.target_h as i32),
                    };
                    let tx = 0;
                    let ty = 0;
                    let tw = r.target_w as i32;
                    let th = r.target_h as i32;
                    let x1 = x.clamp(tx, tx + tw);
                    let y1 = y.clamp(ty, ty + th);
                    let x2 = (x + w).clamp(tx, tx + tw);
                    let y2 = (y + h).clamp(ty, ty + th);
                    let xr = x1.max(0) as u64;
                    let yr = y1.max(0) as u64;
                    let wr = (x2 - x1).max(0) as u64;
                    let hr = (y2 - y1).max(0) as u64;
                    enc.set_scissor_rect(MTLScissorRect { x: xr, y: yr, width: wr, height: hr });
                    last_applied = effective;
                    r.scissor_changes = r.scissor_changes.saturating_add(1);
                }
                i += 1;
                continue;
            }
            api::DrawCmd::ClipPop => {
                let _ = stack.pop();
                current = if stack.is_empty() {
                    None
                } else {
                    let mut it = stack.iter();
                    let mut acc = *it.next().unwrap();
                    for rct in it {
                        let x1 = acc.x.max(rct.x);
                        let y1 = acc.y.max(rct.y);
                        let x2 = (acc.x + acc.w).min(rct.x + rct.w);
                        let y2 = (acc.y + acc.h).min(rct.y + rct.h);
                        if x2 > x1 && y2 > y1 {
                            acc = api::RectI { x: x1, y: y1, w: x2 - x1, h: y2 - y1 };
                        } else {
                            acc = api::RectI { x: 0, y: 0, w: 0, h: 0 };
                            break;
                        }
                    }
                    Some(acc)
                };
                // Intersect with global scissor (dp)
                let effective: Option<api::RectI> = match (current, global_scissor_dp) {
                    (Some(c), Some(g)) => {
                        let x1 = c.x.max(g.x);
                        let y1 = c.y.max(g.y);
                        let x2 = (c.x + c.w).min(g.x + g.w);
                        let y2 = (c.y + c.h).min(g.y + g.h);
                        if x2 > x1 && y2 > y1 {
                            Some(api::RectI { x: x1, y: y1, w: x2 - x1, h: y2 - y1 })
                        } else {
                            Some(api::RectI { x: 0, y: 0, w: 0, h: 0 })
                        }
                    }
                    (Some(c), None) => Some(c),
                    (None, Some(g)) => Some(g),
                    (None, None) => None,
                };
                if last_applied != effective {
                    let scale = r.target_scale.max(1.0);
                    let (x, y, w, h) = match effective {
                        Some(rc) => {
                            let x = (rc.x as f32 * scale).floor() as i32;
                            let y = (rc.y as f32 * scale).floor() as i32;
                            let w = (rc.w as f32 * scale).ceil() as i32;
                            let h = (rc.h as f32 * scale).ceil() as i32;
                            (x, y, w, h)
                        }
                        None => (0, 0, r.target_w as i32, r.target_h as i32),
                    };
                    let tx = 0;
                    let ty = 0;
                    let tw = r.target_w as i32;
                    let th = r.target_h as i32;
                    let x1 = x.clamp(tx, tx + tw);
                    let y1 = y.clamp(ty, ty + th);
                    let x2 = (x + w).clamp(tx, tx + tw);
                    let y2 = (y + h).clamp(ty, ty + th);
                    let xr = x1.max(0) as u64;
                    let yr = y1.max(0) as u64;
                    let wr = (x2 - x1).max(0) as u64;
                    let hr = (y2 - y1).max(0) as u64;
                    enc.set_scissor_rect(MTLScissorRect { x: xr, y: yr, width: wr, height: hr });
                    last_applied = effective;
                    r.scissor_changes = r.scissor_changes.saturating_add(1);
                }
                i += 1;
                continue;
            }
            api::DrawCmd::Solid { vb, ib, color } => {
                enc.set_render_pipeline_state(&r.pso_solid);
                let v_count = vb.len as usize;
                let v_bytes = v_count * core::mem::size_of::<api::Vertex>();
                let slot = (r.frame_id % 3) as usize;
                r.vb.ensure_capacity(&r.device, slot, pf.vb_used + v_bytes);
                let dst = unsafe {
                    core::slice::from_raw_parts_mut(
                        r.vb.contents_ptr(slot).as_ptr().add(pf.vb_used),
                        v_bytes,
                    )
                };
                let src_slice =
                    &list.vertices[(vb.offset as usize)..(vb.offset as usize + v_count)];
                let src_bytes: &[u8] = unsafe {
                    core::slice::from_raw_parts(src_slice.as_ptr() as *const u8, v_bytes)
                };
                dst.copy_from_slice(src_bytes);
                let vb_off = pf.vb_used as u64;
                pf.vb_used += v_bytes;
                let rgba = [color.r, color.g, color.b, color.a];
                let ub_off = pf.ub_used as u64;
                let u_bytes = core::mem::size_of_val(&rgba);
                r.ub.ensure_capacity(&r.device, slot, pf.ub_used + u_bytes);
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        rgba.as_ptr() as *const u8,
                        r.ub.contents_ptr(slot).as_ptr().add(pf.ub_used),
                        u_bytes,
                    );
                }
                pf.ub_used += u_bytes;
                enc.set_vertex_buffer(0, Some(&r.vb.bufs[slot]), vb_off);
                enc.set_fragment_buffer(1, Some(&r.ub.bufs[slot]), ub_off);
                let idx_count = ib.len as usize;
                if idx_count > 0 {
                    // Upload indices and draw indexed
                    let i_bytes = idx_count * core::mem::size_of::<u16>();
                    r.ib.ensure_capacity(&r.device, slot, pf.ib_used + i_bytes);
                    let idst = unsafe {
                        core::slice::from_raw_parts_mut(
                            r.ib.contents_ptr(slot).as_ptr().add(pf.ib_used),
                            i_bytes,
                        )
                    };
                    let isrc_slice =
                        &list.indices[(ib.offset as usize)..(ib.offset as usize + idx_count)];
                    let isrc_bytes: &[u8] = unsafe {
                        core::slice::from_raw_parts(isrc_slice.as_ptr() as *const u8, i_bytes)
                    };
                    idst.copy_from_slice(isrc_bytes);
                    let ib_off = pf.ib_used as u64;
                    pf.ib_used += i_bytes;
                    enc.draw_indexed_primitives(
                        MTLPrimitiveType::Triangle,
                        idx_count as u64,
                        MTLIndexType::UInt16,
                        &r.ib.bufs[slot],
                        ib_off,
                    );
                    r.acc_draws += 1;
                } else {
                    enc.draw_primitives(MTLPrimitiveType::Triangle, 0, v_count as u64);
                    r.acc_draws += 1;
                }
                i += 1;
            }
            api::DrawCmd::RRect { rect, radii, color } => {
                enc.set_render_pipeline_state(&r.pso_rrect);
                // Batch consecutive RRects with same scissor as instanced draw
                let mut count = 0usize;
                let mut vbuf: alloc::vec::Vec<f32> = alloc::vec::Vec::new(); // rect dp (x,y,w,h)
                let mut fbuf: alloc::vec::Vec<f32> = alloc::vec::Vec::new(); // params px (rect,radii,color)
                let scale = r.target_scale.max(1.0);
                let mut j = i;
                while j < list.items.len() {
                    if let api::DrawCmd::RRect { rect, radii, color } = &list.items[j] {
                        vbuf.extend_from_slice(&[rect.x, rect.y, rect.w, rect.h]);
                        fbuf.extend_from_slice(&[
                            rect.x * scale,
                            rect.y * scale,
                            rect.w * scale,
                            rect.h * scale,
                            radii[0] * scale,
                            radii[1] * scale,
                            radii[2] * scale,
                            radii[3] * scale,
                            color.r,
                            color.g,
                            color.b,
                            color.a,
                        ]);
                        count += 1;
                        j += 1;
                    } else {
                        break;
                    }
                }
                // Set vp size and arrays
                enc.set_vertex_bytes(
                    1,
                    core::mem::size_of_val(&vp_dp) as u64,
                    vp_dp.as_ptr() as *const _,
                );
                enc.set_vertex_bytes(
                    0,
                    (vbuf.len() * core::mem::size_of::<f32>()) as u64,
                    vbuf.as_ptr() as *const _,
                );
                enc.set_fragment_bytes(
                    1,
                    (fbuf.len() * core::mem::size_of::<f32>()) as u64,
                    fbuf.as_ptr() as *const _,
                );
                enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, count as u64);
                r.acc_instanced += count as u32;
                i = j;
                continue;
            }
            api::DrawCmd::NineSlice { tex, rect, slice, alpha } => {
                if let Some(img) = r.get_image_tex(*tex) {
                    enc.set_render_pipeline_state(&r.pso_nine_slice);
                    if let Some(sam) = &r.sampler {
                        enc.set_fragment_sampler_state(0, Some(sam));
                    }
                    enc.set_fragment_texture(0, Some(img));
                    // Batch consecutive NineSlice with same texture
                    let mut count = 0usize;
                    let mut vbuf: alloc::vec::Vec<f32> = alloc::vec::Vec::new();
                    let mut fbuf: alloc::vec::Vec<f32> = alloc::vec::Vec::new();
                    let s = r.target_scale.max(1.0);
                    let tex_w = img.width() as f32;
                    let tex_h = img.height() as f32;
                    let mut j = i;
                    while j < list.items.len() {
                        if let api::DrawCmd::NineSlice { tex: t2, rect, slice, alpha } =
                            &list.items[j]
                        {
                            if *t2 != *tex {
                                break;
                            }
                            vbuf.extend_from_slice(&[rect.x, rect.y, rect.w, rect.h]);
                            fbuf.extend_from_slice(&[
                                rect.x * s,
                                rect.y * s,
                                rect.w * s,
                                rect.h * s,
                                tex_w,
                                tex_h,
                                slice.left,
                                slice.top,
                                slice.right,
                                slice.bottom,
                                (*alpha).clamp(0.0, 1.0),
                                0.0,
                            ]);
                            count += 1;
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    enc.set_vertex_bytes(
                        0,
                        (vbuf.len() * core::mem::size_of::<f32>()) as u64,
                        vbuf.as_ptr() as *const _,
                    );
                    enc.set_fragment_bytes(
                        1,
                        (fbuf.len() * core::mem::size_of::<f32>()) as u64,
                        fbuf.as_ptr() as *const _,
                    );
                    enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, count as u64);
                    r.acc_instanced += count as u32;
                    i = j;
                    continue;
                }
                i += 1;
            }
            api::DrawCmd::Image { .. } => {
                enc.set_render_pipeline_state(&r.pso_image);
                if let Some(sam) = &r.sampler {
                    enc.set_fragment_sampler_state(0, Some(sam));
                }
                // Bind argument buffer for image textures
                if let Some(buf) = &r.img_arg_buf {
                    enc.set_fragment_buffer(2, Some(buf), 0);
                }
                // Batch consecutive Images regardless of texture using argument buffer
                let mut count = 0usize;
                let mut vbuf: alloc::vec::Vec<f32> = alloc::vec::Vec::new();
                let mut fbytes: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
                let mut tex_map: std::collections::HashMap<u32, u32> =
                    std::collections::HashMap::new();
                let mut next_slot: u32 = 0;
                let mut j = i;
                while j < list.items.len() {
                    if let api::DrawCmd::Image { tex, dst, src, alpha } = &list.items[j] {
                        // Map texture handle to slot
                        let slot_idx = if let Some(s) = tex_map.get(&tex.0) {
                            *s
                        } else {
                            let s = next_slot;
                            next_slot += 1;
                            // Set texture in argument encoder
                            if let (Some(encdr), Some(buf)) =
                                (r.img_arg.as_ref(), r.img_arg_buf.as_ref())
                            {
                                // Rebind the buffer to ensure encoder targets it
                                encdr.set_argument_buffer(buf, 0);
                                if let Some(tex_obj) = r.get_image_tex(*tex) {
                                    encdr.set_texture(s as u64, tex_obj);
                                }
                            }
                            tex_map.insert(tex.0, s);
                            s
                        };
                        // Vertex params
                        vbuf.extend_from_slice(&[dst.x, dst.y, dst.w, dst.h]);
                        // Fragment params (ImageParams): rect(dp) src(px) texSize(px) alpha + texIndex
                        let (tw, th) = if let Some(tref) = r.get_image_tex(*tex) {
                            (tref.width() as f32, tref.height() as f32)
                        } else {
                            (1.0f32, 1.0f32)
                        };
                        let arr: [f32; 11] = [
                            dst.x,
                            dst.y,
                            dst.w,
                            dst.h,
                            src.x,
                            src.y,
                            src.w,
                            src.h,
                            tw,
                            th,
                            (*alpha).clamp(0.0, 1.0),
                        ];
                        let p_bytes: &[u8] = unsafe {
                            core::slice::from_raw_parts(
                                arr.as_ptr() as *const u8,
                                core::mem::size_of_val(&arr),
                            )
                        };
                        fbytes.extend_from_slice(p_bytes);
                        let tex_index_u32 = slot_idx as u32;
                        let t_bytes: &[u8] = unsafe {
                            core::slice::from_raw_parts(
                                (&tex_index_u32 as *const u32) as *const u8,
                                core::mem::size_of::<u32>(),
                            )
                        };
                        fbytes.extend_from_slice(t_bytes);
                        count += 1;
                        j += 1;
                    } else {
                        break;
                    }
                }
                // Set vp
                enc.set_vertex_bytes(
                    1,
                    core::mem::size_of_val(&vp_dp) as u64,
                    vp_dp.as_ptr() as *const _,
                );
                // Upload per-instance rects
                enc.set_vertex_bytes(
                    0,
                    (vbuf.len() * core::mem::size_of::<f32>()) as u64,
                    vbuf.as_ptr() as *const _,
                );
                // Upload per-instance ImageParams
                enc.set_fragment_bytes(1, fbytes.len() as u64, fbytes.as_ptr() as *const _);
                enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, count as u64);
                r.acc_instanced += count as u32;
                i = j;
                continue;
            }
            api::DrawCmd::GlyphRun { .. } => {
                // Group consecutive GlyphRun with same atlas and sdf flag, and record into ICB
                let mut count = 0usize;
                let mut group_atlas = None;
                let mut group_sdf = false;
                let slot = (r.frame_id % 3) as usize;
                // Pre-scan to determine group and upload VB/UB/IB, collecting offsets
                struct GR {
                    vb_off: u64,
                    ib_off: u64,
                    idx_count: u64,
                    ub_off: u64,
                }
                let mut group: alloc::vec::Vec<GR> = alloc::vec::Vec::new();
                let mut j = i;
                while j < list.items.len() {
                    if let api::DrawCmd::GlyphRun { run } = &list.items[j] {
                        if group_atlas.is_none() {
                            group_atlas = Some(run.atlas);
                            group_sdf = run.sdf;
                        } else if group_atlas != Some(run.atlas) || group_sdf != run.sdf {
                            break;
                        }

                        // Upload VB
                        let v_count = run.vb.len as usize;
                        let v_bytes = v_count * core::mem::size_of::<api::Vertex>();
                        r.vb.ensure_capacity(&r.device, slot, pf.vb_used + v_bytes);
                        let dst = unsafe {
                            core::slice::from_raw_parts_mut(
                                r.vb.contents_ptr(slot).as_ptr().add(pf.vb_used),
                                v_bytes,
                            )
                        };
                        let src_slice = &list.vertices
                            [(run.vb.offset as usize)..(run.vb.offset as usize + v_count)];
                        let src_bytes: &[u8] = unsafe {
                            core::slice::from_raw_parts(src_slice.as_ptr() as *const u8, v_bytes)
                        };
                        dst.copy_from_slice(src_bytes);
                        let vb_off = pf.vb_used as u64;
                        pf.vb_used += v_bytes;
                        // Upload color UB
                        let rgba = [run.color.r, run.color.g, run.color.b, run.color.a];
                        let ub_off = pf.ub_used as u64;
                        let u_bytes = core::mem::size_of_val(&rgba);
                        r.ub.ensure_capacity(&r.device, slot, pf.ub_used + u_bytes);
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                rgba.as_ptr() as *const u8,
                                r.ub.contents_ptr(slot).as_ptr().add(pf.ub_used),
                                u_bytes,
                            );
                        }
                        pf.ub_used += u_bytes;
                        // Upload IB
                        let idx_count = run.ib.len as usize;
                        let mut ib_off = 0u64;
                        if idx_count > 0 {
                            let i_bytes = idx_count * core::mem::size_of::<u16>();
                            r.ib.ensure_capacity(&r.device, slot, pf.ib_used + i_bytes);
                            let idst = unsafe {
                                core::slice::from_raw_parts_mut(
                                    r.ib.contents_ptr(slot).as_ptr().add(pf.ib_used),
                                    i_bytes,
                                )
                            };
                            let isrc_slice = &list.indices
                                [(run.ib.offset as usize)..(run.ib.offset as usize + idx_count)];
                            let isrc_bytes: &[u8] = unsafe {
                                core::slice::from_raw_parts(
                                    isrc_slice.as_ptr() as *const u8,
                                    i_bytes,
                                )
                            };
                            idst.copy_from_slice(isrc_bytes);
                            ib_off = pf.ib_used as u64;
                            pf.ib_used += i_bytes;
                        }
                        group.push(GR { vb_off, ib_off, idx_count: idx_count as u64, ub_off });
                        count += 1;
                        j += 1;
                    } else {
                        break;
                    }
                }
                // Bind atlas + sampler and vp
                if let Some(atlas) = group_atlas.and_then(|h| r.get_image_tex(h)) {
                    if group_sdf {
                        enc.set_render_pipeline_state(&r.pso_text_sdf);
                    } else {
                        enc.set_render_pipeline_state(&r.pso_text);
                    }
                    if let Some(sam) = &r.sampler {
                        enc.set_fragment_sampler_state(0, Some(sam));
                    }
                    enc.set_fragment_texture(0, Some(atlas));
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );

                    // Create ICB and record commands
                    let desc = IndirectCommandBufferDescriptor::new();
                    desc.set_command_types(MTLIndirectCommandType::DrawIndexed);
                    desc.set_inherit_pipeline_state(false);
                    desc.set_max_vertex_buffer_bind_count(1);
                    desc.set_max_fragment_buffer_bind_count(2);
                    let icb = r.device.new_indirect_command_buffer_with_descriptor(
                        &desc,
                        count as u64,
                        MTLResourceOptions::StorageModePrivate,
                    );
                    for (ci, gr) in group.iter().enumerate() {
                        let cmd_i = icb.indirect_render_command_at_index(ci as u64);
                        if group_sdf {
                            cmd_i.set_render_pipeline_state(&r.pso_text_sdf);
                        } else {
                            cmd_i.set_render_pipeline_state(&r.pso_text);
                        }
                        cmd_i.set_vertex_buffer(0, Some(&r.vb.bufs[slot]), gr.vb_off);
                        cmd_i.set_fragment_buffer(1, Some(&r.ub.bufs[slot]), gr.ub_off);
                        if gr.idx_count > 0 {
                            cmd_i.draw_indexed_primitives(
                                MTLPrimitiveType::Triangle,
                                gr.idx_count,
                                MTLIndexType::UInt16,
                                &r.ib.bufs[slot],
                                gr.ib_off,
                                1,
                                0,
                                0,
                            );
                        }
                    }
                    enc.execute_commands_in_buffer(
                        &icb,
                        NSRange { location: 0, length: count as u64 },
                    );
                    r.acc_icb_cmds += count as u32;
                }
                i = j;
                continue;
            }
            api::DrawCmd::Spinner { center, radius, thickness, phase, alpha } => {
                enc.set_render_pipeline_state(&r.pso_spinner);
                // Batch consecutive spinners
                let mut count = 0usize;
                let mut vbuf: alloc::vec::Vec<f32> = alloc::vec::Vec::new();
                let mut fbuf: alloc::vec::Vec<f32> = alloc::vec::Vec::new();
                let s = r.target_scale.max(1.0);
                let mut j = i;
                while j < list.items.len() {
                    if let api::DrawCmd::Spinner { center, radius, thickness, phase, alpha } =
                        &list.items[j]
                    {
                        let mm = *radius + *thickness;
                        vbuf.extend_from_slice(&[
                            center[0] - mm,
                            center[1] - mm,
                            mm * 2.0,
                            mm * 2.0,
                        ]);
                        fbuf.extend_from_slice(&[
                            center[0] * s,
                            center[1] * s,
                            *radius * s,
                            *thickness * s,
                            *phase,
                            *alpha,
                            0.0,
                            0.0,
                        ]);
                        count += 1;
                        j += 1;
                    } else {
                        break;
                    }
                }
                enc.set_vertex_bytes(
                    1,
                    core::mem::size_of_val(&vp_dp) as u64,
                    vp_dp.as_ptr() as *const _,
                );
                enc.set_vertex_bytes(
                    0,
                    (vbuf.len() * core::mem::size_of::<f32>()) as u64,
                    vbuf.as_ptr() as *const _,
                );
                enc.set_fragment_bytes(
                    1,
                    (fbuf.len() * core::mem::size_of::<f32>()) as u64,
                    fbuf.as_ptr() as *const _,
                );
                enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, count as u64);
                r.acc_instanced += count as u32;
                i = j;
                continue;
            }
            api::DrawCmd::Backdrop { rect, tint, alpha, .. } => {
                if prepass {
                    // Stop prepass at the first backdrop; draw nothing for it here.
                    break;
                }
                if let Some(src) = &r.prepass_tex {
                    enc.set_render_pipeline_state(&r.pso_backdrop);
                    if let Some(sam) = &r.sampler {
                        enc.set_fragment_sampler_state(0, Some(sam));
                    }
                    enc.set_fragment_texture(0, Some(src));
                    // Batch consecutive backdrops
                    let mut count = 0usize;
                    let mut vbuf: alloc::vec::Vec<f32> = alloc::vec::Vec::new();
                    let mut fbuf: alloc::vec::Vec<f32> = alloc::vec::Vec::new(); // rect px + tint
                    let s = r.target_scale.max(1.0);
                    let mut j = i;
                    while j < list.items.len() {
                        if let api::DrawCmd::Backdrop { rect, tint, alpha, .. } = &list.items[j] {
                            vbuf.extend_from_slice(&[rect.x, rect.y, rect.w, rect.h]);
                            let a = (tint.a * *alpha).clamp(0.0, 1.0);
                            fbuf.extend_from_slice(&[
                                rect.x * s,
                                rect.y * s,
                                rect.w * s,
                                rect.h * s,
                                tint.r,
                                tint.g,
                                tint.b,
                                a,
                            ]);
                            count += 1;
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    enc.set_vertex_bytes(
                        1,
                        core::mem::size_of_val(&vp_dp) as u64,
                        vp_dp.as_ptr() as *const _,
                    );
                    enc.set_vertex_bytes(
                        0,
                        (vbuf.len() * core::mem::size_of::<f32>()) as u64,
                        vbuf.as_ptr() as *const _,
                    );
                    enc.set_fragment_bytes(
                        1,
                        (fbuf.len() * core::mem::size_of::<f32>()) as u64,
                        fbuf.as_ptr() as *const _,
                    );
                    enc.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, count as u64);
                    r.acc_instanced += count as u32;
                    i = j;
                    continue;
                }
                i += 1;
            } // ClipPush/ClipPop handled above
        }
        // Default progress
        // Note: continue branches have updated i accordingly
        if i < list.items.len() { /* fallthrough increment happens in each arm */ }
    }
}

fn build_solid_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = lib.get_function("v_solid", None).map_err(|_| MetalInitError::Pipeline)?;
    let f = lib.get_function("f_solid", None).map_err(|_| MetalInitError::Pipeline)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let vdesc = VertexDescriptor::new();
    let attrs = vdesc.attributes();
    attrs.object_at(0).unwrap().set_format(MTLVertexFormat::Float2);
    attrs.object_at(0).unwrap().set_offset(0);
    attrs.object_at(0).unwrap().set_buffer_index(0);
    attrs.object_at(1).unwrap().set_format(MTLVertexFormat::Float2);
    attrs.object_at(1).unwrap().set_offset(8);
    attrs.object_at(1).unwrap().set_buffer_index(0);
    attrs.object_at(2).unwrap().set_format(MTLVertexFormat::UChar4Normalized);
    attrs.object_at(2).unwrap().set_offset(16);
    attrs.object_at(2).unwrap().set_buffer_index(0);
    let layouts = vdesc.layouts();
    layouts.object_at(0).unwrap().set_stride(20);
    layouts.object_at(0).unwrap().set_step_function(MTLVertexStepFunction::PerVertex);
    desc.set_vertex_descriptor(Some(&vdesc));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(false);
    device.new_render_pipeline_state(&desc).map_err(|_| MetalInitError::Pipeline)
}

fn build_blur_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = lib.get_function("v_fullscreen", None).map_err(|_| MetalInitError::Pipeline)?;
    let f = lib.get_function("f_blur", None).map_err(|_| MetalInitError::Pipeline)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(false);
    device.new_render_pipeline_state(&desc).map_err(|_| MetalInitError::Pipeline)
}

fn build_downsample_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = lib.get_function("v_fullscreen", None).map_err(|_| MetalInitError::Pipeline)?;
    let f = lib.get_function("f_downsample", None).map_err(|_| MetalInitError::Pipeline)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(false);
    device.new_render_pipeline_state(&desc).map_err(|_| MetalInitError::Pipeline)
}

fn build_upsample_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = lib.get_function("v_fullscreen", None).map_err(|_| MetalInitError::Pipeline)?;
    let f = lib.get_function("f_upsample", None).map_err(|_| MetalInitError::Pipeline)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(false);
    device.new_render_pipeline_state(&desc).map_err(|_| MetalInitError::Pipeline)
}

fn build_backdrop_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = lib.get_function("v_inst_rect", None).map_err(|_| MetalInitError::Pipeline)?;
    let f = lib.get_function("f_backdrop", None).map_err(|_| MetalInitError::Pipeline)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    device.new_render_pipeline_state(&desc).map_err(|_| MetalInitError::Pipeline)
}

fn build_image_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = lib.get_function("v_inst_rect", None).map_err(|_| MetalInitError::Pipeline)?;
    let f = lib.get_function("f_image", None).map_err(|_| MetalInitError::Pipeline)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    device.new_render_pipeline_state(&desc).map_err(|_| MetalInitError::Pipeline)
}

fn build_rrect_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = lib.get_function("v_inst_rect", None).map_err(|_| MetalInitError::Pipeline)?;
    let f = lib.get_function("f_rrect", None).map_err(|_| MetalInitError::Pipeline)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(false);
    device.new_render_pipeline_state(&desc).map_err(|_| MetalInitError::Pipeline)
}

fn build_nine_slice_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = lib.get_function("v_inst_rect", None).map_err(|_| MetalInitError::Pipeline)?;
    let f = lib.get_function("f_nine_slice", None).map_err(|_| MetalInitError::Pipeline)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    device.new_render_pipeline_state(&desc).map_err(|_| MetalInitError::Pipeline)
}

fn build_spinner_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = lib.get_function("v_inst_rect", None).map_err(|_| MetalInitError::Pipeline)?;
    let f = lib.get_function("f_spinner", None).map_err(|_| MetalInitError::Pipeline)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    device.new_render_pipeline_state(&desc).map_err(|_| MetalInitError::Pipeline)
}

fn build_text_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = lib.get_function("v_text", None).map_err(|_| MetalInitError::Pipeline)?;
    let f = lib.get_function("f_text", None).map_err(|_| MetalInitError::Pipeline)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let vdesc = VertexDescriptor::new();
    let attrs = vdesc.attributes();
    attrs.object_at(0).unwrap().set_format(MTLVertexFormat::Float2);
    attrs.object_at(0).unwrap().set_offset(0);
    attrs.object_at(0).unwrap().set_buffer_index(0);
    attrs.object_at(1).unwrap().set_format(MTLVertexFormat::Float2);
    attrs.object_at(1).unwrap().set_offset(8);
    attrs.object_at(1).unwrap().set_buffer_index(0);
    attrs.object_at(2).unwrap().set_format(MTLVertexFormat::UChar4Normalized);
    attrs.object_at(2).unwrap().set_offset(16);
    attrs.object_at(2).unwrap().set_buffer_index(0);
    let layouts = vdesc.layouts();
    layouts.object_at(0).unwrap().set_stride(20);
    layouts.object_at(0).unwrap().set_step_function(MTLVertexStepFunction::PerVertex);
    desc.set_vertex_descriptor(Some(&vdesc));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    device.new_render_pipeline_state(&desc).map_err(|_| MetalInitError::Pipeline)
}

fn build_text_sdf_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = lib.get_function("v_text", None).map_err(|_| MetalInitError::Pipeline)?;
    let f = lib.get_function("f_text_sdf", None).map_err(|_| MetalInitError::Pipeline)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let vdesc = VertexDescriptor::new();
    let attrs = vdesc.attributes();
    attrs.object_at(0).unwrap().set_format(MTLVertexFormat::Float2);
    attrs.object_at(0).unwrap().set_offset(0);
    attrs.object_at(0).unwrap().set_buffer_index(0);
    attrs.object_at(1).unwrap().set_format(MTLVertexFormat::Float2);
    attrs.object_at(1).unwrap().set_offset(8);
    attrs.object_at(1).unwrap().set_buffer_index(0);
    attrs.object_at(2).unwrap().set_format(MTLVertexFormat::UChar4Normalized);
    attrs.object_at(2).unwrap().set_offset(16);
    attrs.object_at(2).unwrap().set_buffer_index(0);
    let layouts = vdesc.layouts();
    layouts.object_at(0).unwrap().set_stride(20);
    layouts.object_at(0).unwrap().set_step_function(MTLVertexStepFunction::PerVertex);
    desc.set_vertex_descriptor(Some(&vdesc));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(true);
    ca.set_rgb_blend_operation(MTLBlendOperation::Add);
    ca.set_alpha_blend_operation(MTLBlendOperation::Add);
    ca.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_source_alpha_blend_factor(MTLBlendFactor::SourceAlpha);
    ca.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    ca.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    device.new_render_pipeline_state(&desc).map_err(|_| MetalInitError::Pipeline)
}

fn build_camera_pso(
    device: &Device,
    lib: &Library,
    fmt: MTLPixelFormat,
) -> Result<RenderPipelineState, MetalInitError> {
    let v = lib.get_function("v_inst_rect_cam", None).map_err(|_| MetalInitError::Pipeline)?;
    let f = lib.get_function("f_camera_nv12", None).map_err(|_| MetalInitError::Pipeline)?;
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(&v));
    desc.set_fragment_function(Some(&f));
    let ca = desc.color_attachments().object_at(0).unwrap();
    ca.set_pixel_format(fmt);
    ca.set_blending_enabled(false);
    device.new_render_pipeline_state(&desc).map_err(|_| MetalInitError::Pipeline)
}

fn build_sampler(device: &Device) -> Option<SamplerState> {
    let desc = SamplerDescriptor::new();
    desc.set_min_filter(MTLSamplerMinMagFilter::Linear);
    desc.set_mag_filter(MTLSamplerMinMagFilter::Linear);
    // Clamp-to-edge on S/T
    desc.set_address_mode_s(MTLSamplerAddressMode::ClampToEdge);
    desc.set_address_mode_t(MTLSamplerAddressMode::ClampToEdge);
    Some(device.new_sampler(&desc))
}

#[derive(Default)]
struct PerFrame {
    cmd: Option<CommandBuffer>,
    vb_used: usize,
    ib_used: usize,
    ub_used: usize,
}

impl PerFrame {
    const fn new() -> Self {
        Self { cmd: None, vb_used: 0, ib_used: 0, ub_used: 0 }
    }
    fn reset(&mut self) {
        self.vb_used = 0;
        self.ib_used = 0;
        self.ub_used = 0;
    }
    fn completed(&mut self) {
        self.reset();
    }
}

struct Ring {
    bufs: [Buffer; 3],
    cap: [usize; 3],
    opts: MTLResourceOptions,
}

impl Ring {
    fn new(device: &Device, initial: usize, opts: MTLResourceOptions) -> Self {
        let b0 = device.new_buffer(initial as u64, opts);
        let b1 = device.new_buffer(initial as u64, opts);
        let b2 = device.new_buffer(initial as u64, opts);
        Self { bufs: [b0, b1, b2], cap: [initial, initial, initial], opts }
    }
    fn ensure_capacity(&mut self, device: &Device, slot: usize, needed: usize) {
        if needed <= self.cap[slot] {
            return;
        }
        let mut new_cap = self.cap[slot] + self.cap[slot] / 2;
        if new_cap < needed {
            new_cap = needed;
        }
        self.bufs[slot] = device.new_buffer(new_cap as u64, self.opts);
        self.cap[slot] = new_cap;
    }
    fn contents_ptr(&self, slot: usize) -> NonNull<u8> {
        let p = self.bufs[slot].contents();
        NonNull::new(p as *mut u8).expect("non-null")
    }
}

extern crate alloc;

#[derive(Debug)]
struct LayerEntry {
    tex: Texture,
    w: u32,
    h: u32,
    hash: u64,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PerfStats {
    pub vb_bytes: u64,
    pub ib_bytes: u64,
    pub ub_bytes: u64,
    pub draws: u32,
    pub instanced: u32,
    pub icb_cmds: u32,
    pub encode_ms: f64,
    pub damage_px: u64,
    pub damage_pct: f32,
    pub damage_rects: u32,
    pub culled: u32,
    // Phase 7 instrumentation
    pub blur_ms: f64,          // time spent updating blurred camera this frame
    pub blur_updates: u32,     // 1 if blurred camera updated this frame, else 0
    pub blur_period_ms: u32,   // current target blur update period
    pub cam_coverage_pct: f32, // fraction of viewport covered by CameraBg
    pub cam_paused: u8,        // 1 if camera paused by adaptive policy
    pub thermal: u8,           // iOS thermal state 0..3 (0 if not iOS)
    pub low_power: u8,         // 1 if Low Power Mode enabled (0 if not iOS)
    pub cam_width: u32,
    pub cam_height: u32,
    pub cam_bit_depth: u8,
    pub cam_matrix: u8,
    pub cam_video_range: u8,
    pub cam_color_space: u8,
}

impl MetalRenderer {
    pub fn last_stats(&self) -> PerfStats {
        self.last_stats
    }

    pub fn set_damage_options(&mut self, enabled: bool, use_thresh: f32, prefilter: f32) {
        self.damage_enabled = enabled;
        self.damage_use_thresh = use_thresh.clamp(0.0, 1.0);
        self.damage_prefilter_thresh = prefilter.clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "macos", all(target_os = "ios", not(target_abi = "sim"))))]
    #[test]
    fn ring_resizes_buffers() {
        let Some(device) = Device::system_default() else { return };
        let mut ring = Ring::new(&device, 128, MTLResourceOptions::StorageModeShared);
        let initial = ring.cap[0];
        ring.ensure_capacity(&device, 0, initial * 4);
        assert!(ring.cap[0] >= initial * 4);
        assert!(!ring.contents_ptr(0).as_ptr().is_null());
    }

    #[cfg(any(target_os = "macos", all(target_os = "ios", not(target_abi = "sim"))))]
    #[test]
    fn renderer_initial_stats_zero() {
        match MetalRenderer::new_default() {
            Ok(renderer) => {
                let stats = renderer.last_stats();
                assert_eq!(stats.draws, 0);
                assert_eq!(stats.damage_rects, 0);
            }
            Err(MetalInitError::NoDevice) => {}
            Err(e) => panic!("unexpected init error: {e}"),
        }
    }
}
