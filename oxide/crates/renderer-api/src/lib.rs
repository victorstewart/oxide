//! `Oxide` Renderer API
#![forbid(unsafe_code)]
#![allow(clippy::all, clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use core::fmt;

// Opaque frame target used by Renderer implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct FrameToken(pub u64);

/// Optional per-frame damage regions in device-independent pixels (dp).
/// When provided, renderers may restrict work to these rectangles.
#[derive(Debug, Clone)]
pub struct Damage {
    pub rects: alloc::vec::Vec<RectI>,
}

// Geometry and basic graphics types
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}
impl Insets {
    #[inline]
    #[must_use]
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl RectF {
    #[inline]
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RectI {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
impl RectI {
    #[inline]
    #[must_use]
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}
impl Color {
    #[inline]
    #[must_use]
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VisualEffect {
    UIKitDark,
    /// One draw command that collapses old Nametag iOS `PopupBlurBase` chrome.
    ///
    /// `blur_intensity` is the only blur-strength control exposed to callers:
    /// `0.0` disables blur and `1.0` requests the strongest reusable popup
    /// blur material. Renderer backends own the sigma, radius, downsample, and
    /// pass-planning details.
    ///
    /// `tint` is the caller-controlled material color. Its RGB channels are
    /// the overlay color; its alpha is the material opacity, not another blur
    /// strength control.
    DarkPopup {
        blur_intensity: f32,
        tint: Color,
    },
}

impl VisualEffect {
    #[inline]
    #[must_use]
    pub fn blur_intensity(self) -> f32 {
        match self {
            Self::UIKitDark => 1.0,
            Self::DarkPopup { blur_intensity, .. } => {
                if blur_intensity.is_finite() {
                    blur_intensity.clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }
        }
    }

    #[inline]
    #[must_use]
    pub fn tint(self) -> Color {
        match self {
            Self::UIKitDark => Color::rgba(0.0, 0.0, 0.0, 0.90),
            Self::DarkPopup { tint, .. } => tint,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub u: f32,
    pub v: f32,
    pub rgba: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VertexSpan {
    pub offset: u32,
    pub len: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexSpan {
    pub offset: u32,
    pub len: u32,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphRun {
    pub atlas: ImageHandle,
    pub atlas_revision: u64,
    pub vb: VertexSpan,
    pub ib: IndexSpan,
    pub sdf: bool,
    pub color: Color,
}

// Render errors
#[derive(Debug, Clone)]
pub enum RenderError {
    DeviceLost,
    OutOfMemory,
    InvalidOperation(&'static str),
    ResourceNotFound(&'static str),
    Unsupported(&'static str),
    ShaderCompile(String),
    Io(String),
}
impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceLost => write!(f, "device lost"),
            Self::OutOfMemory => write!(f, "out of memory"),
            Self::InvalidOperation(s) => write!(f, "invalid operation: {s}"),
            Self::ResourceNotFound(s) => write!(f, "resource not found: {s}"),
            Self::Unsupported(s) => write!(f, "unsupported: {s}"),
            Self::ShaderCompile(s) => write!(f, "shader compile error: {s}"),
            Self::Io(s) => write!(f, "io error: {s}"),
        }
    }
}
impl std::error::Error for RenderError {}

// Draw list and encoder API (crate-agnostic)
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DrawList {
    pub items: alloc::vec::Vec<DrawCmd>,
    // Optional backing arrays for span-based draws. When present, spans
    // reference these arrays; encoders may fall back to immediate paths.
    pub vertices: alloc::vec::Vec<Vertex>,
    pub indices: alloc::vec::Vec<u16>,
}

impl DrawList {
    #[inline]
    #[must_use]
    pub fn text_atlas_revision_compatible(&self, atlas: ImageHandle, revision: u64) -> bool {
        self.text_atlas_revisions_compatible(&[(atlas, revision)])
    }

    #[must_use]
    pub fn text_atlas_revisions_compatible(&self, atlases: &[(ImageHandle, u64)]) -> bool {
        self.items.iter().all(|cmd| match cmd {
            DrawCmd::GlyphRun { run } => atlases
                .iter()
                .any(|(atlas, revision)| run.atlas == *atlas && run.atlas_revision == *revision),
            _ => true,
        })
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCmd {
    // Layered rendering: render nested items into an offscreen texture, then composite.
    // Nested items appear between LayerBegin and LayerEnd and are not drawn directly to target.
    LayerBegin { id: u32, rect: RectF, dirty: bool },
    LayerEnd,
    Solid { vb: VertexSpan, ib: IndexSpan, color: Color },
    Image { tex: ImageHandle, dst: RectF, src: RectF, alpha: f32 },
    ImageMesh { tex: ImageHandle, vb: VertexSpan, ib: IndexSpan, alpha: f32 },
    GlyphRun { run: GlyphRun },
    RRect { rect: RectF, radii: [f32; 4], color: Color },
    NineSlice { tex: ImageHandle, rect: RectF, slice: Insets, alpha: f32 },
    Backdrop { rect: RectF, sigma: f32, tint: Color, alpha: f32 },
    VisualEffect { rect: RectF, effect: VisualEffect },
    // Platform camera background (iOS Metal: NV12 import). Renderer interprets this
    // as a request to composite the latest camera frame behind UI.
    // When unsupported on a platform, it is a no-op.
    CameraBg { rect: RectF, tint: Color, alpha: f32, grayscale: bool, blur: bool, sigma: f32 },
    Spinner { center: [f32; 2], atom: f32, alpha: f32 },
    ClipPush { rect: RectI },
    ClipPop,
}

pub trait RenderEncoder {
    fn set_viewport(&mut self, vp: RectF);
    fn set_clip(&mut self, scissor: RectI);
    fn draw_solid(&mut self, verts: &[Vertex], color: Color);
    fn draw_image(&mut self, img: ImageHandle, dst: RectF, src: RectF);
    fn draw_image_mesh(
        &mut self,
        img: ImageHandle,
        vertices: &[Vertex],
        _indices: &[u16],
        _alpha: f32,
    ) {
        let Some(bounds) = vertex_bounds(vertices) else {
            return;
        };
        self.draw_image(img, bounds, RectF::new(0.0, 0.0, 0.0, 0.0));
    }
    fn draw_rrect(&mut self, rect: RectF, radii: [f32; 4], color: Color);
    fn draw_nine_slice(&mut self, img: ImageHandle, rect: RectF, slice: Insets, alpha: f32);
    fn draw_backdrop(&mut self, rect: RectF, sigma: f32, tint: Color, alpha: f32);
    fn draw_visual_effect(&mut self, rect: RectF, effect: VisualEffect) {
        match effect {
            VisualEffect::UIKitDark | VisualEffect::DarkPopup { .. } => {
                let tint = effect.tint();
                const FALLBACK_MAX_BLUR_SIGMA: f32 = 72.0;
                self.draw_backdrop(
                    rect,
                    effect.blur_intensity() * FALLBACK_MAX_BLUR_SIGMA,
                    Color::rgba(tint.r, tint.g, tint.b, 1.0),
                    tint.a,
                );
            }
        }
    }
    fn draw_camera_bg(
        &mut self,
        _rect: RectF,
        _tint: Color,
        _alpha: f32,
        _grayscale: bool,
        _blur: bool,
        _sigma: f32,
    ) {
    }
    fn draw_spinner(&mut self, center: [f32; 2], atom: f32, alpha: f32);
    fn draw_glyph_run(&mut self, run: &GlyphRun);
    fn draw_glyph_run_resolved(&mut self, run: &GlyphRun, _vertices: &[Vertex], _indices: &[u16]) {
        self.draw_glyph_run(run);
    }
}

#[inline]
#[must_use]
fn vertex_bounds(vertices: &[Vertex]) -> Option<RectF> {
    let first = vertices.first()?;
    let mut x0 = first.x;
    let mut y0 = first.y;
    let mut x1 = first.x;
    let mut y1 = first.y;
    for vertex in &vertices[1..] {
        x0 = x0.min(vertex.x);
        y0 = y0.min(vertex.y);
        x1 = x1.max(vertex.x);
        y1 = y1.max(vertex.y);
    }
    let w = x1 - x0;
    let h = y1 - y0;
    (w > 0.0 && h > 0.0).then_some(RectF::new(x0, y0, w, h))
}

pub trait Renderer {
    fn device_caps(&self) -> DeviceCaps;
    fn begin_frame(&mut self, fb: &FrameTarget, damage: Option<&Damage>) -> FrameToken;
    fn encode_pass(&mut self, list: &DrawList);
    fn submit(&mut self, token: FrameToken) -> Result<(), RenderError>;
    fn resize(&mut self, w: u32, h: u32, scale: f32) -> Result<(), RenderError>;
}

// Exposed here to avoid circular deps (platform-api needs it for App::draw).
pub struct RenderContext {
    pub frame_id: u64,
    pub encoder: alloc::boxed::Box<dyn RenderEncoder>,
}

// Minimal device caps subset duplicated here for renderer consumption.
// Full DeviceCaps is declared in platform-api; we expose a mirror to avoid a hard dep.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceCaps {
    pub max_framerate_hz: u32,
    pub supports_edr: bool,
    pub supports_msaa4x: bool,
    pub native_scale: f32,
}

extern crate alloc;
