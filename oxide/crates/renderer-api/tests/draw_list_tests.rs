use oxide_renderer_api::{
    Color, Damage, DrawCmd, DrawList, GlyphRun, ImageHandle, IndexSpan, Insets, RectF, RectI,
    RenderEncoder, Vertex, VertexSpan, VisualEffect,
};

const EXPECTED_DRAW_CMD_TAXONOMY: &[&str] = &[
   "LayerBegin",
   "LayerEnd",
   "Solid",
   "Image",
   "ImageMesh",
   "GlyphRun",
   "RRect",
   "NineSlice",
   "Backdrop",
   "VisualEffect",
   "CameraBg",
   "Spinner",
   "ClipPush",
   "ClipPop",
];

const EXPECTED_REPRESENTATIVE_DRAW_STREAM_CAPTURE: &[&str] = &[
   "vertices=8 indices=12",
   "00 ClipPush rect=0,0,120,90",
   "01 LayerBegin id=42 rect=10.000,11.000,80.000,40.000 dirty=true",
   "02 Solid vb=0:4 ib=0:6 color=0.100/0.200/0.300/0.400",
   "03 Image tex=11 dst=12.000,13.000,30.000,20.000 src=0.000,0.000,0.500,0.500 alpha=0.500",
   "04 ImageMesh tex=12 vb=4:4 ib=6:6 alpha=0.600",
   "05 GlyphRun atlas=13 revision=99 vb=4:4 ib=6:6 sdf=true color=0.800/0.700/0.600/0.500",
   "06 RRect rect=15.000,16.000,24.000,18.000 radii=1.000/2.000/3.000/4.000 color=0.200/0.300/0.400/0.900",
   "07 NineSlice tex=14 rect=20.000,21.000,50.000,25.000 slice=2.000/3.000/4.000/5.000 alpha=0.700",
   "08 Backdrop rect=0.000,0.000,120.000,90.000 sigma=8.000 tint=0.100/0.200/0.300/0.400 alpha=0.800",
   "09 VisualEffect rect=4.000,5.000,60.000,30.000 effect=DarkPopup blur=0.350 tint=0.400/0.500/0.600/0.700",
   "10 CameraBg rect=0.000,0.000,120.000,90.000 tint=0.900/0.800/0.700/0.600 alpha=0.900 grayscale=true blur=true sigma=11.000",
   "11 Spinner center=30.000/31.000 atom=12.000 alpha=1.000",
   "12 LayerEnd",
   "13 ClipPop",
];

const EXPECTED_REPRESENTATIVE_DRAW_STREAM_REPLAY: &[&str] = &[
   "set_clip rect=0,0,120,90",
   "draw_solid vertices=4 color=0.100/0.200/0.300/0.400",
   "draw_image tex=11 dst=12.000,13.000,30.000,20.000 src=0.000,0.000,0.500,0.500",
   "draw_image_mesh tex=12 vertices=4 indices=6 alpha=0.600",
   "draw_glyph_run_resolved atlas=13 revision=99 vertices=4 indices=6 sdf=true color=0.800/0.700/0.600/0.500",
   "draw_rrect rect=15.000,16.000,24.000,18.000 radii=1.000/2.000/3.000/4.000 color=0.200/0.300/0.400/0.900",
   "draw_nine_slice tex=14 rect=20.000,21.000,50.000,25.000 slice=2.000/3.000/4.000/5.000 alpha=0.700",
   "draw_backdrop rect=0.000,0.000,120.000,90.000 sigma=8.000 tint=0.100/0.200/0.300/0.400 alpha=0.800",
   "draw_visual_effect rect=4.000,5.000,60.000,30.000 effect=DarkPopup blur=0.350 tint=0.400/0.500/0.600/0.700",
   "draw_camera_bg rect=0.000,0.000,120.000,90.000 tint=0.900/0.800/0.700/0.600 alpha=0.900 grayscale=true blur=true sigma=11.000",
   "draw_spinner center=30.000/31.000 atom=12.000 alpha=1.000",
   "set_clip rect=0,0,0,0",
];

fn sample_rect() -> RectF
{
   RectF::new(1.0, 2.0, 3.0, 4.0)
}

fn sample_color() -> Color
{
   Color::rgba(0.1, 0.2, 0.3, 0.4)
}

fn sample_vertex_span() -> VertexSpan
{
   VertexSpan { offset: 5, len: 6 }
}

fn sample_index_span() -> IndexSpan
{
   IndexSpan { offset: 7, len: 8 }
}

fn sample_glyph_run() -> GlyphRun
{
   GlyphRun {
      atlas: ImageHandle(9),
      atlas_revision: 10,
      vb: sample_vertex_span(),
      ib: sample_index_span(),
      sdf: true,
      color: sample_color(),
   }
}

fn representative_draw_cmds() -> [DrawCmd; 14]
{
   [
      DrawCmd::LayerBegin { id: 1, rect: sample_rect(), dirty: true },
      DrawCmd::LayerEnd,
      DrawCmd::Solid { vb: sample_vertex_span(), ib: sample_index_span(), color: sample_color() },
      DrawCmd::Image {
         tex: ImageHandle(2),
         dst: sample_rect(),
         src: RectF::new(0.0, 0.0, 1.0, 1.0),
         alpha: 0.5,
      },
      DrawCmd::ImageMesh {
         tex: ImageHandle(3),
         vb: sample_vertex_span(),
         ib: sample_index_span(),
         alpha: 0.6,
      },
      DrawCmd::GlyphRun { run: sample_glyph_run() },
      DrawCmd::RRect { rect: sample_rect(), radii: [1.0, 2.0, 3.0, 4.0], color: sample_color() },
      DrawCmd::NineSlice {
         tex: ImageHandle(4),
         rect: sample_rect(),
         slice: Insets::new(1.0, 2.0, 3.0, 4.0),
         alpha: 0.7,
      },
      DrawCmd::Backdrop { rect: sample_rect(), sigma: 8.0, tint: sample_color(), alpha: 0.8 },
      DrawCmd::VisualEffect { rect: sample_rect(), effect: VisualEffect::UIKitDark },
      DrawCmd::CameraBg {
         rect: sample_rect(),
         tint: sample_color(),
         alpha: 0.9,
         grayscale: true,
         blur: true,
         sigma: 11.0,
      },
      DrawCmd::Spinner { center: [12.0, 13.0], atom: 14.0, alpha: 1.0 },
      DrawCmd::ClipPush { rect: RectI::new(0, 1, 2, 3) },
      DrawCmd::ClipPop,
   ]
}

fn representative_draw_stream() -> DrawList
{
   let mut list = DrawList::default();
   list.vertices.extend([
      Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0xFF00_0000 },
      Vertex { x: 10.0, y: 0.0, u: 1.0, v: 0.0, rgba: 0xFF00_0000 },
      Vertex { x: 0.0, y: 10.0, u: 0.0, v: 1.0, rgba: 0xFF00_0000 },
      Vertex { x: 10.0, y: 10.0, u: 1.0, v: 1.0, rgba: 0xFF00_0000 },
      Vertex { x: 20.0, y: 20.0, u: 0.0, v: 0.0, rgba: 0xFFFF_FFFF },
      Vertex { x: 30.0, y: 20.0, u: 1.0, v: 0.0, rgba: 0xFFFF_FFFF },
      Vertex { x: 20.0, y: 30.0, u: 0.0, v: 1.0, rgba: 0xFFFF_FFFF },
      Vertex { x: 30.0, y: 30.0, u: 1.0, v: 1.0, rgba: 0xFFFF_FFFF },
   ]);
   list.indices.extend([0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7]);
   list.items.extend([
      DrawCmd::ClipPush { rect: RectI::new(0, 0, 120, 90) },
      DrawCmd::LayerBegin { id: 42, rect: RectF::new(10.0, 11.0, 80.0, 40.0), dirty: true },
      DrawCmd::Solid {
         vb: VertexSpan { offset: 0, len: 4 },
         ib: IndexSpan { offset: 0, len: 6 },
         color: Color::rgba(0.1, 0.2, 0.3, 0.4),
      },
      DrawCmd::Image {
         tex: ImageHandle(11),
         dst: RectF::new(12.0, 13.0, 30.0, 20.0),
         src: RectF::new(0.0, 0.0, 0.5, 0.5),
         alpha: 0.5,
      },
      DrawCmd::ImageMesh {
         tex: ImageHandle(12),
         vb: VertexSpan { offset: 4, len: 4 },
         ib: IndexSpan { offset: 6, len: 6 },
         alpha: 0.6,
      },
      DrawCmd::GlyphRun {
         run: GlyphRun {
            atlas: ImageHandle(13),
            atlas_revision: 99,
            vb: VertexSpan { offset: 4, len: 4 },
            ib: IndexSpan { offset: 6, len: 6 },
            sdf: true,
            color: Color::rgba(0.8, 0.7, 0.6, 0.5),
         },
      },
      DrawCmd::RRect {
         rect: RectF::new(15.0, 16.0, 24.0, 18.0),
         radii: [1.0, 2.0, 3.0, 4.0],
         color: Color::rgba(0.2, 0.3, 0.4, 0.9),
      },
      DrawCmd::NineSlice {
         tex: ImageHandle(14),
         rect: RectF::new(20.0, 21.0, 50.0, 25.0),
         slice: Insets::new(2.0, 3.0, 4.0, 5.0),
         alpha: 0.7,
      },
      DrawCmd::Backdrop {
         rect: RectF::new(0.0, 0.0, 120.0, 90.0),
         sigma: 8.0,
         tint: Color::rgba(0.1, 0.2, 0.3, 0.4),
         alpha: 0.8,
      },
      DrawCmd::VisualEffect {
         rect: RectF::new(4.0, 5.0, 60.0, 30.0),
         effect: VisualEffect::DarkPopup {
            blur_intensity: 0.35,
            tint: Color::rgba(0.4, 0.5, 0.6, 0.7),
         },
      },
      DrawCmd::CameraBg {
         rect: RectF::new(0.0, 0.0, 120.0, 90.0),
         tint: Color::rgba(0.9, 0.8, 0.7, 0.6),
         alpha: 0.9,
         grayscale: true,
         blur: true,
         sigma: 11.0,
      },
      DrawCmd::Spinner { center: [30.0, 31.0], atom: 12.0, alpha: 1.0 },
      DrawCmd::LayerEnd,
      DrawCmd::ClipPop,
   ]);
   list
}

fn draw_cmd_taxonomy_name(cmd: &DrawCmd) -> &'static str
{
   match cmd {
      DrawCmd::LayerBegin { .. } => "LayerBegin",
      DrawCmd::LayerEnd => "LayerEnd",
      DrawCmd::Solid { .. } => "Solid",
      DrawCmd::Image { .. } => "Image",
      DrawCmd::ImageMesh { .. } => "ImageMesh",
      DrawCmd::GlyphRun { .. } => "GlyphRun",
      DrawCmd::RRect { .. } => "RRect",
      DrawCmd::NineSlice { .. } => "NineSlice",
      DrawCmd::Backdrop { .. } => "Backdrop",
      DrawCmd::VisualEffect { .. } => "VisualEffect",
      DrawCmd::CameraBg { .. } => "CameraBg",
      DrawCmd::Spinner { .. } => "Spinner",
      DrawCmd::ClipPush { .. } => "ClipPush",
      DrawCmd::ClipPop => "ClipPop",
   }
}

fn capture_draw_stream_signature(list: &DrawList) -> Vec<String>
{
   let mut rows = Vec::with_capacity(list.items.len() + 1);
   rows.push(format!("vertices={} indices={}", list.vertices.len(), list.indices.len()));
   for (index, cmd) in list.items.iter().enumerate() {
      rows.push(format!("{index:02} {}", capture_draw_cmd_signature(cmd)));
   }
   rows
}

fn capture_draw_cmd_signature(cmd: &DrawCmd) -> String
{
   match cmd {
      DrawCmd::LayerBegin { id, rect, dirty } => {
         format!("LayerBegin id={id} rect={} dirty={dirty}", format_rect_f(*rect))
      }
      DrawCmd::LayerEnd => String::from("LayerEnd"),
      DrawCmd::Solid { vb, ib, color } => {
         format!("Solid vb={} ib={} color={}", format_vertex_span(*vb), format_index_span(*ib), format_color(*color))
      }
      DrawCmd::Image { tex, dst, src, alpha } => {
         format!("Image tex={} dst={} src={} alpha={alpha:.3}", tex.0, format_rect_f(*dst), format_rect_f(*src))
      }
      DrawCmd::ImageMesh { tex, vb, ib, alpha } => {
         format!("ImageMesh tex={} vb={} ib={} alpha={alpha:.3}", tex.0, format_vertex_span(*vb), format_index_span(*ib))
      }
      DrawCmd::GlyphRun { run } => {
         format!(
            "GlyphRun atlas={} revision={} vb={} ib={} sdf={} color={}",
            run.atlas.0,
            run.atlas_revision,
            format_vertex_span(run.vb),
            format_index_span(run.ib),
            run.sdf,
            format_color(run.color)
         )
      }
      DrawCmd::RRect { rect, radii, color } => {
         format!("RRect rect={} radii={} color={}", format_rect_f(*rect), format_radii(*radii), format_color(*color))
      }
      DrawCmd::NineSlice { tex, rect, slice, alpha } => {
         format!("NineSlice tex={} rect={} slice={} alpha={alpha:.3}", tex.0, format_rect_f(*rect), format_insets(*slice))
      }
      DrawCmd::Backdrop { rect, sigma, tint, alpha } => {
         format!("Backdrop rect={} sigma={sigma:.3} tint={} alpha={alpha:.3}", format_rect_f(*rect), format_color(*tint))
      }
      DrawCmd::VisualEffect { rect, effect } => {
         format!("VisualEffect rect={} effect={}", format_rect_f(*rect), format_visual_effect(*effect))
      }
      DrawCmd::CameraBg { rect, tint, alpha, grayscale, blur, sigma } => {
         format!(
            "CameraBg rect={} tint={} alpha={alpha:.3} grayscale={grayscale} blur={blur} sigma={sigma:.3}",
            format_rect_f(*rect),
            format_color(*tint)
         )
      }
      DrawCmd::Spinner { center, atom, alpha } => {
         format!("Spinner center={:.3}/{:.3} atom={atom:.3} alpha={alpha:.3}", center[0], center[1])
      }
      DrawCmd::ClipPush { rect } => format!("ClipPush rect={}", format_rect_i(*rect)),
      DrawCmd::ClipPop => String::from("ClipPop"),
   }
}

fn replay_representative_draw_stream(list: &DrawList, encoder: &mut dyn RenderEncoder)
{
   let fallback = RectI::new(0, 0, 0, 0);
   let mut clip_stack = Vec::new();
   for cmd in &list.items {
      match cmd {
         DrawCmd::ClipPush { rect } => {
            clip_stack.push(*rect);
            encoder.set_clip(*rect);
         }
         DrawCmd::ClipPop => {
            let _ = clip_stack.pop();
            encoder.set_clip(*clip_stack.last().unwrap_or(&fallback));
         }
         DrawCmd::LayerBegin { .. } | DrawCmd::LayerEnd => {}
         DrawCmd::Solid { vb, color, .. } => {
            if let Some(vertices) = vertex_slice(list, *vb) {
               encoder.draw_solid(vertices, *color);
            }
         }
         DrawCmd::Image { tex, dst, src, .. } => encoder.draw_image(*tex, *dst, *src),
         DrawCmd::ImageMesh { tex, vb, ib, alpha } => {
            if let (Some(vertices), Some(indices)) = (vertex_slice(list, *vb), index_slice(list, *ib)) {
               encoder.draw_image_mesh(*tex, vertices, indices, *alpha);
            }
         }
         DrawCmd::GlyphRun { run } => {
            if let (Some(vertices), Some(indices)) = (vertex_slice(list, run.vb), index_slice(list, run.ib)) {
               encoder.draw_glyph_run_resolved(run, vertices, indices);
            }
         }
         DrawCmd::RRect { rect, radii, color } => encoder.draw_rrect(*rect, *radii, *color),
         DrawCmd::NineSlice { tex, rect, slice, alpha } => {
            encoder.draw_nine_slice(*tex, *rect, *slice, *alpha);
         }
         DrawCmd::Backdrop { rect, sigma, tint, alpha } => {
            encoder.draw_backdrop(*rect, *sigma, *tint, *alpha);
         }
         DrawCmd::VisualEffect { rect, effect } => encoder.draw_visual_effect(*rect, *effect),
         DrawCmd::CameraBg { rect, tint, alpha, grayscale, blur, sigma } => {
            encoder.draw_camera_bg(*rect, *tint, *alpha, *grayscale, *blur, *sigma);
         }
         DrawCmd::Spinner { center, atom, alpha } => encoder.draw_spinner(*center, *atom, *alpha),
      }
   }
}

fn vertex_slice(list: &DrawList, span: VertexSpan) -> Option<&[Vertex]>
{
   let start = span.offset as usize;
   let end = start.checked_add(span.len as usize)?;
   list.vertices.get(start..end)
}

fn index_slice(list: &DrawList, span: IndexSpan) -> Option<&[u16]>
{
   let start = span.offset as usize;
   let end = start.checked_add(span.len as usize)?;
   list.indices.get(start..end)
}

fn format_rect_f(rect: RectF) -> String
{
   format!("{:.3},{:.3},{:.3},{:.3}", rect.x, rect.y, rect.w, rect.h)
}

fn format_rect_i(rect: RectI) -> String
{
   format!("{},{},{},{}", rect.x, rect.y, rect.w, rect.h)
}

fn format_color(color: Color) -> String
{
   format!("{:.3}/{:.3}/{:.3}/{:.3}", color.r, color.g, color.b, color.a)
}

fn format_vertex_span(span: VertexSpan) -> String
{
   format!("{}:{}", span.offset, span.len)
}

fn format_index_span(span: IndexSpan) -> String
{
   format!("{}:{}", span.offset, span.len)
}

fn format_radii(radii: [f32; 4]) -> String
{
   format!("{:.3}/{:.3}/{:.3}/{:.3}", radii[0], radii[1], radii[2], radii[3])
}

fn format_insets(insets: Insets) -> String
{
   format!("{:.3}/{:.3}/{:.3}/{:.3}", insets.left, insets.top, insets.right, insets.bottom)
}

fn format_visual_effect(effect: VisualEffect) -> String
{
   match effect {
      VisualEffect::UIKitDark => String::from("UIKitDark"),
      VisualEffect::DarkPopup { blur_intensity, tint } => {
         format!("DarkPopup blur={blur_intensity:.3} tint={}", format_color(tint))
      }
   }
}

fn expected_strings(rows: &[&str]) -> Vec<String>
{
   rows.iter().map(|row| String::from(*row)).collect()
}

#[derive(Default)]
struct TraceEncoder
{
   rows: Vec<String>,
}

impl TraceEncoder
{
   fn push(&mut self, row: String)
   {
      self.rows.push(row);
   }
}

impl RenderEncoder for TraceEncoder
{
   fn set_viewport(&mut self, vp: RectF)
   {
      self.push(format!("set_viewport rect={}", format_rect_f(vp)));
   }

   fn set_clip(&mut self, scissor: RectI)
   {
      self.push(format!("set_clip rect={}", format_rect_i(scissor)));
   }

   fn draw_solid(&mut self, verts: &[Vertex], color: Color)
   {
      self.push(format!("draw_solid vertices={} color={}", verts.len(), format_color(color)));
   }

   fn draw_image(&mut self, img: ImageHandle, dst: RectF, src: RectF)
   {
      self.push(format!(
         "draw_image tex={} dst={} src={}",
         img.0,
         format_rect_f(dst),
         format_rect_f(src)
      ));
   }

   fn draw_image_mesh(&mut self, img: ImageHandle, vertices: &[Vertex], indices: &[u16], alpha: f32)
   {
      self.push(format!(
         "draw_image_mesh tex={} vertices={} indices={} alpha={alpha:.3}",
         img.0,
         vertices.len(),
         indices.len()
      ));
   }

   fn draw_rrect(&mut self, rect: RectF, radii: [f32; 4], color: Color)
   {
      self.push(format!(
         "draw_rrect rect={} radii={} color={}",
         format_rect_f(rect),
         format_radii(radii),
         format_color(color)
      ));
   }

   fn draw_nine_slice(&mut self, img: ImageHandle, rect: RectF, slice: Insets, alpha: f32)
   {
      self.push(format!(
         "draw_nine_slice tex={} rect={} slice={} alpha={alpha:.3}",
         img.0,
         format_rect_f(rect),
         format_insets(slice)
      ));
   }

   fn draw_backdrop(&mut self, rect: RectF, sigma: f32, tint: Color, alpha: f32)
   {
      self.push(format!(
         "draw_backdrop rect={} sigma={sigma:.3} tint={} alpha={alpha:.3}",
         format_rect_f(rect),
         format_color(tint)
      ));
   }

   fn draw_visual_effect(&mut self, rect: RectF, effect: VisualEffect)
   {
      self.push(format!(
         "draw_visual_effect rect={} effect={}",
         format_rect_f(rect),
         format_visual_effect(effect)
      ));
   }

   fn draw_camera_bg(
      &mut self,
      rect: RectF,
      tint: Color,
      alpha: f32,
      grayscale: bool,
      blur: bool,
      sigma: f32,
   )
   {
      self.push(format!(
         "draw_camera_bg rect={} tint={} alpha={alpha:.3} grayscale={grayscale} blur={blur} sigma={sigma:.3}",
         format_rect_f(rect),
         format_color(tint)
      ));
   }

   fn draw_spinner(&mut self, center: [f32; 2], atom: f32, alpha: f32)
   {
      self.push(format!(
         "draw_spinner center={:.3}/{:.3} atom={atom:.3} alpha={alpha:.3}",
         center[0],
         center[1]
      ));
   }

   fn draw_glyph_run(&mut self, run: &GlyphRun)
   {
      self.push(format!(
         "draw_glyph_run atlas={} revision={} sdf={} color={}",
         run.atlas.0,
         run.atlas_revision,
         run.sdf,
         format_color(run.color)
      ));
   }

   fn draw_glyph_run_resolved(&mut self, run: &GlyphRun, vertices: &[Vertex], indices: &[u16])
   {
      self.push(format!(
         "draw_glyph_run_resolved atlas={} revision={} vertices={} indices={} sdf={} color={}",
         run.atlas.0,
         run.atlas_revision,
         vertices.len(),
         indices.len(),
         run.sdf,
         format_color(run.color)
      ));
   }
}

fn draw_cmd_source_taxonomy() -> Vec<&'static str>
{
   let source = include_str!("../src/lib.rs");
   let header = "pub enum DrawCmd {";
   let start = source
      .find(header)
      .unwrap_or_else(|| panic!("DrawCmd enum declaration is missing"))
      + header.len();
   let body = &source[start..];
   let mut variants = Vec::new();
   let mut depth = 0i32;
   let mut item_start = 0usize;
   let mut index = 0usize;
   while index < body.len() {
      let rest = &body[index..];
      if rest.starts_with("//") {
         index += rest.find('\n').map_or(rest.len(), |line_len| line_len + 1);
         continue;
      }
      let ch = rest.chars().next().unwrap_or_else(|| panic!("invalid DrawCmd source slice"));
      match ch {
         '{' | '(' | '[' => depth += 1,
         '}' if depth == 0 => break,
         '}' | ')' | ']' => depth -= 1,
         ',' if depth == 0 => {
            if let Some(name) = draw_cmd_source_variant_name(&body[item_start..index]) {
               variants.push(name);
            }
            item_start = index + ch.len_utf8();
         }
         _ => {}
      }
      index += ch.len_utf8();
   }
   variants
}

fn draw_cmd_source_variant_name(item: &'static str) -> Option<&'static str>
{
   for line in item.lines() {
      let trimmed = line.trim();
      if trimmed.is_empty() || trimmed.starts_with("//") {
         continue;
      }
      let end = trimmed
         .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
         .unwrap_or(trimmed.len());
      if end > 0 {
         return Some(&trimmed[..end]);
      }
   }
   None
}

fn validate_draw_list(list: &DrawList) -> Result<(), &'static str> {
    let mut layer_depth = 0i32;
    let mut clip_depth = 0i32;
    for cmd in &list.items {
        match cmd {
            DrawCmd::LayerBegin { .. } => layer_depth += 1,
            DrawCmd::LayerEnd => layer_depth -= 1,
            DrawCmd::ClipPush { .. } => clip_depth += 1,
            DrawCmd::ClipPop => clip_depth -= 1,
            _ => {}
        }
        if layer_depth < 0 {
            return Err("layer underflow");
        }
        if clip_depth < 0 {
            return Err("clip underflow");
        }
    }
    if layer_depth != 0 {
        return Err("unbalanced layer stack");
    }
    if clip_depth != 0 {
        return Err("unbalanced clip stack");
    }
    Ok(())
}

#[test]
fn draw_cmd_taxonomy_is_frozen()
{
   let representatives = representative_draw_cmds();
   assert_eq!(representatives.len(), EXPECTED_DRAW_CMD_TAXONOMY.len());
   for (cmd, expected_name) in representatives.iter().zip(EXPECTED_DRAW_CMD_TAXONOMY) {
      assert_eq!(draw_cmd_taxonomy_name(cmd), *expected_name);
   }
   assert_eq!(draw_cmd_source_taxonomy().as_slice(), EXPECTED_DRAW_CMD_TAXONOMY);
}

#[test]
fn representative_draw_stream_capture_signature_is_frozen()
{
   let list = representative_draw_stream();
   assert_eq!(
      capture_draw_stream_signature(&list),
      expected_strings(EXPECTED_REPRESENTATIVE_DRAW_STREAM_CAPTURE)
   );

   let mut encoder = TraceEncoder::default();
   replay_representative_draw_stream(&list, &mut encoder);
   assert_eq!(encoder.rows, expected_strings(EXPECTED_REPRESENTATIVE_DRAW_STREAM_REPLAY));
}

#[test]
fn draw_list_api_has_no_native_preview_or_app_specific_commands() {
    let source = include_str!("../src/lib.rs");
    assert!(
        !source.contains("NativeCameraPreview") && !source.contains("draw_native_camera_preview"),
        "renderer-api must keep visible camera preview in Oxide-owned renderer commands"
    );
    assert!(
        !source.contains("TopomapGlobe")
            && !source.contains("TopomapGlobeWebApp")
            && !source.contains("topomap_globe")
            && !source.contains("topomap_app_")
            && !source.contains("draw_topomap_globe")
            && !source.contains("DrawCmd::TopomapGlobe"),
        "renderer-api must not expose app-specific Topomap globe commands"
    );
}

#[test]
fn balanced_layers_and_clips_validate() {
    let mut list = DrawList::default();
    list.items.push(DrawCmd::LayerBegin {
        id: 1,
        rect: RectF::new(0.0, 0.0, 50.0, 50.0),
        dirty: false,
    });
    list.items.push(DrawCmd::ClipPush { rect: RectI::new(0, 0, 50, 50) });
    list.items.push(DrawCmd::Solid {
        vb: VertexSpan { offset: 0, len: 4 },
        ib: IndexSpan { offset: 0, len: 6 },
        color: Color::rgba(1.0, 0.0, 0.0, 1.0),
    });
    list.items.push(DrawCmd::GlyphRun {
        run: GlyphRun {
            atlas: ImageHandle(7),
            atlas_revision: 11,
            vb: VertexSpan { offset: 10, len: 12 },
            ib: IndexSpan { offset: 20, len: 18 },
            sdf: false,
            color: Color::rgba(0.1, 0.2, 0.3, 1.0),
        },
    });
    list.items.push(DrawCmd::ClipPop);
    list.items.push(DrawCmd::LayerEnd);

    assert!(validate_draw_list(&list).is_ok());
}

#[test]
fn draw_list_detects_stale_text_atlas_revision() {
    let mut list = DrawList::default();
    list.items.push(DrawCmd::GlyphRun {
        run: GlyphRun {
            atlas: ImageHandle(7),
            atlas_revision: 2,
            vb: VertexSpan { offset: 0, len: 4 },
            ib: IndexSpan { offset: 0, len: 6 },
            sdf: false,
            color: Color::rgba(0.1, 0.2, 0.3, 1.0),
        },
    });

    assert!(list.text_atlas_revision_compatible(ImageHandle(7), 2));
    assert!(!list.text_atlas_revision_compatible(ImageHandle(7), 3));
    assert!(!list.text_atlas_revision_compatible(ImageHandle(8), 99));
    assert!(list.text_atlas_revisions_compatible(&[(ImageHandle(7), 2)]));
    assert!(!list.text_atlas_revisions_compatible(&[]));
}

#[test]
fn detects_unbalanced_layer_stack() {
    let mut list = DrawList::default();
    list.items.push(DrawCmd::LayerEnd);
    assert_eq!(validate_draw_list(&list), Err("layer underflow"));

    let mut list2 = DrawList::default();
    list2.items.push(DrawCmd::LayerBegin {
        id: 1,
        rect: RectF::new(0.0, 0.0, 1.0, 1.0),
        dirty: true,
    });
    assert_eq!(validate_draw_list(&list2), Err("unbalanced layer stack"));
}

#[test]
fn damage_rects_round_trip() {
    let rects = vec![RectI::new(0, 0, 100, 50), RectI::new(10, 10, 20, 20)];
    let damage = Damage { rects: rects.clone() };
    assert_eq!(damage.rects, rects);
}

#[test]
fn vertex_storage_is_mutable() {
    let mut list = DrawList::default();
    list.vertices.push(Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0xFFFF_0000 });
    list.indices.extend([0, 1, 2]);
    assert_eq!(list.vertices.len(), 1);
    assert_eq!(list.indices, vec![0, 1, 2]);
}
