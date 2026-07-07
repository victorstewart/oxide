//! Reusable renderer-agnostic starscape backgrounds.
#![forbid(unsafe_code)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use oxide_renderer_api::{Color, RectF, RenderEncoder, Vertex};

pub const STARSCAPE_BACKGROUND_DEFAULT_DENSITY: f32 = 1.25;
pub const STARSCAPE_BACKGROUND_DEFAULT_PINK_MIX: f32 = 1.5;
pub const STARSCAPE_BACKGROUND_MAX_DENSITY: f32 = 4.0;
pub const STARSCAPE_BACKGROUND_BASE_STAR_COUNT_MIN: usize = 22;
pub const STARSCAPE_BACKGROUND_BASE_STAR_COUNT_SPREAD: usize = 16;
pub const STARSCAPE_BACKGROUND_BASE_PINK_SHARE: f32 = 0.22;
pub const STARSCAPE_BACKGROUND_DUST_COUNT: usize = 220;

const STARSCAPE_BACKGROUND_DUST_CLOUD_COUNT: usize = 6;
const STARSCAPE_BACKGROUND_DUST_CLUSTER_SHARE: f32 = 0.82;
const STARSCAPE_BACKGROUND_TAU: f32 = 6.283_185_5;
const STARSCAPE_ATMOSPHERE_MIN_ROWS: usize = 8;
const STARSCAPE_ATMOSPHERE_MAX_ROWS: usize = 128;
const STARSCAPE_ATMOSPHERE_ELLIPSE_RINGS: usize = 7;
const STARSCAPE_ATMOSPHERE_ELLIPSE_SEGMENTS: usize = 36;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StarscapeBackgroundConfig {
    pub base_color: Color,
    pub stars: StarscapeStarConfig,
    pub atmosphere: Option<StarscapeAtmosphereConfig>,
}

impl StarscapeBackgroundConfig {
    #[must_use]
    pub fn new(
        base_color: Color,
        stars: StarscapeStarConfig,
        atmosphere: Option<StarscapeAtmosphereConfig>,
    ) -> Self {
        Self { base_color, stars, atmosphere }
    }
}

impl Default for StarscapeBackgroundConfig {
    fn default() -> Self {
        Self {
            base_color: Color::rgba(0.0, 0.0, 0.0, 1.0),
            stars: StarscapeStarConfig::default(),
            atmosphere: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StarscapeStarConfig {
    pub density: f32,
    pub pink_mix: f32,
    pub pink: Color,
    pub white: Color,
    pub dust: Color,
}

impl StarscapeStarConfig {
    #[must_use]
    pub fn nametag(pink: Color) -> Self {
        Self {
            density: STARSCAPE_BACKGROUND_DEFAULT_DENSITY,
            pink_mix: STARSCAPE_BACKGROUND_DEFAULT_PINK_MIX,
            pink,
            white: Color::rgba(0.94, 0.97, 1.0, 1.0),
            dust: pink,
        }
    }
}

impl Default for StarscapeStarConfig {
    fn default() -> Self {
        Self {
            density: STARSCAPE_BACKGROUND_DEFAULT_DENSITY,
            pink_mix: 0.0,
            pink: Color::rgba(1.0, 0.22, 0.46, 1.0),
            white: Color::rgba(0.94, 0.97, 1.0, 1.0),
            dust: Color::rgba(1.0, 0.22, 0.46, 1.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StarscapeAtmosphereConfig {
    pub origin: StarscapeAtmosphereOrigin,
    pub mode: StarscapeAtmosphereMode,
    pub pink: Color,
    pub evening: Color,
    pub max_alpha: f32,
    pub coverage_fraction: f32,
    pub falloff_power: f32,
    pub rows: usize,
    pub seed: u32,
}

impl StarscapeAtmosphereConfig {
    #[must_use]
    pub fn nametag_top_simple(pink: Color, evening: Color) -> Self {
        Self {
            origin: StarscapeAtmosphereOrigin::Top,
            mode: StarscapeAtmosphereMode::SimpleVertical,
            pink,
            evening,
            max_alpha: 0.14,
            coverage_fraction: 0.46,
            falloff_power: 2.1,
            rows: 64,
            seed: 0x4E54_4153,
        }
    }

    #[must_use]
    pub fn nametag_top_complex(pink: Color, evening: Color) -> Self {
        Self {
            origin: StarscapeAtmosphereOrigin::Top,
            mode: StarscapeAtmosphereMode::ComplexSoftMesh,
            pink,
            evening,
            max_alpha: 0.16,
            coverage_fraction: 0.54,
            falloff_power: 2.15,
            rows: 80,
            seed: 0x4E54_4158,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StarscapeAtmosphereOrigin {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StarscapeAtmosphereMode {
    SimpleVertical,
    SimpleDiagonal,
    ComplexSoftMesh,
}

pub struct StarscapeBackground {
    base_count: usize,
    stars: Vec<StarscapeBackgroundStar>,
    dust: Vec<StarscapeBackgroundDust>,
}

impl StarscapeBackground {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut rng = StarscapeBackgroundRng::new(seed);
        let base_count = STARSCAPE_BACKGROUND_BASE_STAR_COUNT_MIN
            + rng.next_usize(STARSCAPE_BACKGROUND_BASE_STAR_COUNT_SPREAD + 1);
        let max_count = ((base_count as f32) * STARSCAPE_BACKGROUND_MAX_DENSITY).ceil() as usize;
        let mut stars = Vec::with_capacity(max_count);
        for _ in 0..max_count {
            let large = rng.next_f32() > 0.72;
            stars.push(StarscapeBackgroundStar {
                x: rng.range_f32(0.0, 1.0),
                y: rng.range_f32(0.0, 1.0),
                radius: if large {
                    rng.range_f32(0.0032, 0.0051)
                } else {
                    rng.range_f32(0.0014, 0.0034)
                },
                alpha: rng.range_f32(0.70, 0.96),
                pink_rank: rng.next_f32(),
            });
        }

        let mut clouds = Vec::with_capacity(STARSCAPE_BACKGROUND_DUST_CLOUD_COUNT);
        for _ in 0..STARSCAPE_BACKGROUND_DUST_CLOUD_COUNT {
            clouds.push(StarscapeBackgroundDustCloud {
                x: rng.range_f32(-0.08, 1.08),
                y: rng.range_f32(-0.08, 1.08),
                radius: rng.range_f32(0.14, 0.32),
                width_scale: rng.range_f32(0.80, 1.80),
                height_scale: rng.range_f32(0.65, 1.45),
            });
        }
        let mut dust = Vec::with_capacity(STARSCAPE_BACKGROUND_DUST_COUNT);
        for _ in 0..STARSCAPE_BACKGROUND_DUST_COUNT {
            let clustered = rng.next_f32() < STARSCAPE_BACKGROUND_DUST_CLUSTER_SHARE;
            let (x, y) = if clustered {
                let cloud = clouds[rng.next_usize(clouds.len())];
                let angle = rng.range_f32(0.0, STARSCAPE_BACKGROUND_TAU);
                let distance = rng.next_f32().sqrt() * cloud.radius;
                (
                    cloud.x + angle.cos() * distance * cloud.width_scale,
                    cloud.y + angle.sin() * distance * cloud.height_scale,
                )
            } else {
                (rng.range_f32(-0.12, 1.12), rng.range_f32(-0.12, 1.12))
            };
            let haze = rng.next_f32() > 0.78;
            dust.push(StarscapeBackgroundDust {
                x,
                y,
                radius: if haze {
                    rng.range_f32(0.012, 0.030)
                } else {
                    rng.range_f32(0.003, 0.012)
                },
                alpha: if haze { rng.range_f32(0.004, 0.009) } else { rng.range_f32(0.006, 0.018) },
            });
        }

        Self { base_count, stars, dust }
    }

    #[must_use]
    pub fn count_for_density(&self, density: f32) -> usize {
        ((self.base_count as f32) * density).round().max(0.0) as usize
    }

    #[must_use]
    pub fn stars(&self) -> &[StarscapeBackgroundStar] {
        &self.stars
    }

    #[must_use]
    pub fn dust(&self) -> &[StarscapeBackgroundDust] {
        &self.dust
    }

    pub fn draw(
        &self,
        encoder: &mut dyn RenderEncoder,
        rect: RectF,
        config: &StarscapeBackgroundConfig,
    ) {
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return;
        }
        encoder.set_viewport(rect);
        encoder.draw_solid(&quad(rect.x, rect.y, rect.w, rect.h), config.base_color);
        if let Some(atmosphere) = config.atmosphere {
            draw_atmosphere(encoder, rect, atmosphere);
        }
        self.draw_dust(encoder, rect, config.stars);
        self.draw_stars(encoder, rect, config.stars);
    }

    fn draw_dust(&self, encoder: &mut dyn RenderEncoder, rect: RectF, config: StarscapeStarConfig) {
        let min_axis = rect.w.min(rect.h).max(1.0);
        let alpha_scale =
            (config.pink_mix / STARSCAPE_BACKGROUND_DEFAULT_PINK_MIX).clamp(0.0, 1.35);
        for particle in &self.dust {
            let x = rect.x + rect.w * particle.x;
            let y = rect.y + rect.h * particle.y;
            let radius = (min_axis * particle.radius).clamp(0.85, 18.0);
            let alpha = (particle.alpha * alpha_scale).clamp(0.0, 0.025);
            encoder.draw_rrect(
                RectF::new(x - radius, y - radius, radius * 2.0, radius * 2.0),
                [radius; 4],
                Color::rgba(config.dust.r, config.dust.g, config.dust.b, config.dust.a * alpha),
            );
        }
    }

    fn draw_stars(
        &self,
        encoder: &mut dyn RenderEncoder,
        rect: RectF,
        config: StarscapeStarConfig,
    ) {
        let min_axis = rect.w.min(rect.h).max(1.0);
        let star_count = self
            .count_for_density(config.density.clamp(0.0, STARSCAPE_BACKGROUND_MAX_DENSITY))
            .min(self.stars.len());
        let pink_share = (STARSCAPE_BACKGROUND_BASE_PINK_SHARE * config.pink_mix).clamp(0.0, 1.0);
        for star in self.stars.iter().take(star_count) {
            let x = rect.x + rect.w * star.x;
            let y = rect.y + rect.h * star.y;
            let radius = (min_axis * star.radius).clamp(0.85, 3.8);
            let source = if star.pink_rank < pink_share { config.pink } else { config.white };
            let color = Color::rgba(source.r, source.g, source.b, source.a * star.alpha);
            encoder.draw_rrect(
                RectF::new(x - radius, y - radius, radius * 2.0, radius * 2.0),
                [radius; 4],
                color,
            );
            if radius > 1.4 {
                let arm = radius * 2.2;
                let thin = (radius * 0.34).max(0.6);
                let arm_color =
                    Color::rgba(source.r, source.g, source.b, source.a * star.alpha * 0.42);
                encoder.draw_rrect(
                    RectF::new(x - arm, y - thin * 0.5, arm * 2.0, thin),
                    [thin * 0.5; 4],
                    arm_color,
                );
                encoder.draw_rrect(
                    RectF::new(x - thin * 0.5, y - arm, thin, arm * 2.0),
                    [thin * 0.5; 4],
                    arm_color,
                );
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StarscapeBackgroundStar {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub alpha: f32,
    pub pink_rank: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StarscapeBackgroundDust {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub alpha: f32,
}

#[derive(Clone, Copy)]
struct StarscapeBackgroundDustCloud {
    x: f32,
    y: f32,
    radius: f32,
    width_scale: f32,
    height_scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AtmosphereStrip {
    rect: RectF,
    color: Color,
    t: f32,
}

#[derive(Debug, Clone, Copy)]
struct AtmosphereLobe {
    x: f32,
    y: f32,
    rx: f32,
    ry: f32,
    alpha: f32,
}

fn draw_atmosphere(
    encoder: &mut dyn RenderEncoder,
    rect: RectF,
    config: StarscapeAtmosphereConfig,
) {
    if config.max_alpha <= 0.0 || config.coverage_fraction <= 0.0 {
        return;
    }
    for strip in atmosphere_strips(rect, config) {
        encoder
            .draw_solid(&quad(strip.rect.x, strip.rect.y, strip.rect.w, strip.rect.h), strip.color);
    }
    if config.mode == StarscapeAtmosphereMode::ComplexSoftMesh {
        draw_soft_lobes(encoder, rect, config);
    }
}

fn atmosphere_strips(rect: RectF, config: StarscapeAtmosphereConfig) -> Vec<AtmosphereStrip> {
    let rows = config.rows.clamp(STARSCAPE_ATMOSPHERE_MIN_ROWS, STARSCAPE_ATMOSPHERE_MAX_ROWS);
    let coverage_h = rect.h * config.coverage_fraction.clamp(0.0, 1.0);
    if rect.w <= 0.0 || rect.h <= 0.0 || coverage_h <= 0.0 {
        return Vec::new();
    }
    let mut strips = Vec::with_capacity(rows);
    for row in 0..rows {
        let t0 = row as f32 / rows as f32;
        let t1 = (row + 1) as f32 / rows as f32;
        let tm = (t0 + t1) * 0.5;
        let eased = smoothstep(tm);
        let fade = 1.0 - eased;
        let alpha = config.max_alpha.clamp(0.0, 1.0) * fade.powf(config.falloff_power.max(0.01));
        if alpha <= 0.001 {
            continue;
        }
        let mut color = mix_color(config.pink, config.evening, eased);
        color.a *= alpha;
        let y = match config.origin {
            StarscapeAtmosphereOrigin::Top => rect.y + t0 * coverage_h,
            StarscapeAtmosphereOrigin::Bottom => rect.y + rect.h - t1 * coverage_h,
        };
        strips.push(AtmosphereStrip {
            rect: RectF::new(rect.x, y, rect.w, ((t1 - t0) * coverage_h).ceil() + 1.0),
            color,
            t: tm,
        });
    }
    strips
}

fn draw_soft_lobes(
    encoder: &mut dyn RenderEncoder,
    rect: RectF,
    config: StarscapeAtmosphereConfig,
) {
    let color = mix_color(config.pink, config.evening, 0.34);
    for lobe in atmosphere_lobes(config.seed) {
        let center = [
            rect.x + rect.w * lobe.x,
            match config.origin {
                StarscapeAtmosphereOrigin::Top => rect.y + rect.h * lobe.y,
                StarscapeAtmosphereOrigin::Bottom => rect.y + rect.h * (1.0 - lobe.y),
            },
        ];
        let rx = rect.w * lobe.rx;
        let ry = rect.h * lobe.ry;
        let alpha = config.max_alpha.clamp(0.0, 1.0) * lobe.alpha;
        draw_soft_ellipse(encoder, center, rx, ry, color, alpha);
    }
}

fn atmosphere_lobes(seed: u32) -> [AtmosphereLobe; 3] {
    let mut rng = StarscapeBackgroundRng::new(u64::from(seed));
    [
        jitter_lobe(
            AtmosphereLobe { x: 0.18, y: -0.08, rx: 0.34, ry: 0.22, alpha: 0.28 },
            &mut rng,
        ),
        jitter_lobe(
            AtmosphereLobe { x: 0.62, y: -0.04, rx: 0.44, ry: 0.18, alpha: 0.18 },
            &mut rng,
        ),
        jitter_lobe(AtmosphereLobe { x: 0.90, y: 0.06, rx: 0.30, ry: 0.20, alpha: 0.10 }, &mut rng),
    ]
}

fn jitter_lobe(mut lobe: AtmosphereLobe, rng: &mut StarscapeBackgroundRng) -> AtmosphereLobe {
    lobe.x += rng.range_f32(-0.018, 0.018);
    lobe.y += rng.range_f32(-0.012, 0.012);
    lobe.rx *= rng.range_f32(0.94, 1.06);
    lobe.ry *= rng.range_f32(0.94, 1.06);
    lobe
}

fn draw_soft_ellipse(
    encoder: &mut dyn RenderEncoder,
    center: [f32; 2],
    rx: f32,
    ry: f32,
    color: Color,
    alpha: f32,
) {
    if rx <= 0.0 || ry <= 0.0 || alpha <= 0.0 {
        return;
    }
    for ring in (1..=STARSCAPE_ATMOSPHERE_ELLIPSE_RINGS).rev() {
        let t = ring as f32 / STARSCAPE_ATMOSPHERE_ELLIPSE_RINGS as f32;
        let ring_alpha = alpha * (1.0 - t).powf(1.7) * 0.18;
        if ring_alpha <= 0.0005 {
            continue;
        }
        let mut verts = Vec::with_capacity(STARSCAPE_ATMOSPHERE_ELLIPSE_SEGMENTS * 3);
        let ring_rx = rx * t;
        let ring_ry = ry * t;
        for index in 0..STARSCAPE_ATMOSPHERE_ELLIPSE_SEGMENTS {
            let a0 = STARSCAPE_BACKGROUND_TAU * index as f32
                / STARSCAPE_ATMOSPHERE_ELLIPSE_SEGMENTS as f32;
            let a1 = STARSCAPE_BACKGROUND_TAU * (index + 1) as f32
                / STARSCAPE_ATMOSPHERE_ELLIPSE_SEGMENTS as f32;
            verts.extend_from_slice(&[
                vertex(center[0], center[1]),
                vertex(center[0] + a0.cos() * ring_rx, center[1] + a0.sin() * ring_ry),
                vertex(center[0] + a1.cos() * ring_rx, center[1] + a1.sin() * ring_ry),
            ]);
        }
        encoder.draw_solid(&verts, Color::rgba(color.r, color.g, color.b, color.a * ring_alpha));
    }
}

#[inline]
#[must_use]
pub fn color_from_srgb_u8(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::rgba(f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, a)
}

#[inline]
#[must_use]
pub fn generated_background_seed(load_timestamp: u64, salt: &str) -> u64 {
    let mut seed = 0xCBF2_9CE4_8422_2325_u64 ^ load_timestamp.rotate_left(17);
    for byte in salt.as_bytes() {
        seed ^= u64::from(*byte);
        seed = seed.wrapping_mul(0x0000_0100_0000_01B3);
    }
    seed
}

#[inline]
#[must_use]
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
#[must_use]
fn mix_color(left: Color, right: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::rgba(
        left.r + (right.r - left.r) * t,
        left.g + (right.g - left.g) * t,
        left.b + (right.b - left.b) * t,
        left.a + (right.a - left.a) * t,
    )
}

#[inline]
#[must_use]
fn vertex(x: f32, y: f32) -> Vertex {
    Vertex { x, y, u: 0.0, v: 0.0, rgba: 0 }
}

#[inline]
#[must_use]
fn quad(x: f32, y: f32, w: f32, h: f32) -> [Vertex; 6] {
    [
        vertex(x, y),
        vertex(x + w, y),
        vertex(x + w, y + h),
        vertex(x, y),
        vertex(x + w, y + h),
        vertex(x, y + h),
    ]
}

struct StarscapeBackgroundRng {
    state: u64,
}

impl StarscapeBackgroundRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(0xBF58_476D_1CE4_E5B9)
                .max(1),
        }
    }

    fn next_u32(&mut self) -> u32 {
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.state = value;
        ((value >> 32) as u32) ^ (value as u32)
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u32() as f32) / ((u32::MAX as f32) + 1.0)
    }

    fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_f32()
    }

    fn next_usize(&mut self, limit: usize) -> usize {
        if limit == 0 {
            return 0;
        }
        (self.next_u32() as usize) % limit
    }
}
