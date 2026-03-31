use alloc::vec::Vec;
use core::f32::consts::TAU;
use oxide_renderer_api::RectF;

/// Subset of emitter source shapes needed by Oxide's CAEmitter-style burst simulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BurstEmitterShape {
    Sphere,
}

/// Per-particle settings that mirror the legacy CAEmitterCell fields used by Nametag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BurstEmitterCellConfig {
    pub birth_rate: f32,
    pub lifetime_s: f32,
    pub velocity_points_per_s: f32,
    pub scale: f32,
    pub emission_range_rad: f32,
    pub emission_longitude_rad: f32,
}

impl BurstEmitterCellConfig {
    #[must_use]
    pub fn sanitized(self) -> Self {
        Self {
            birth_rate: self.birth_rate.max(0.0),
            lifetime_s: self.lifetime_s.max(0.0),
            velocity_points_per_s: self.velocity_points_per_s.max(0.0),
            scale: self.scale.max(0.0),
            emission_range_rad: self.emission_range_rad.max(0.0),
            emission_longitude_rad: self.emission_longitude_rad,
        }
    }
}

/// Layer-level settings that mirror the CAEmitterLayer fields used by the legacy app.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BurstEmitterConfig {
    pub active_duration_s: f32,
    pub emitter_size_scale: [f32; 2],
    pub emitter_depth: f32,
    pub emitter_shape: BurstEmitterShape,
    pub cell: BurstEmitterCellConfig,
}

impl BurstEmitterConfig {
    #[must_use]
    pub fn sanitized(self) -> Self {
        Self {
            active_duration_s: self.active_duration_s.max(0.0),
            emitter_size_scale: [
                self.emitter_size_scale[0].max(0.0),
                self.emitter_size_scale[1].max(0.0),
            ],
            emitter_depth: self.emitter_depth.max(0.0),
            emitter_shape: self.emitter_shape,
            cell: self.cell.sanitized(),
        }
    }

    #[must_use]
    pub fn emitter_size(self, base_side: f32) -> [f32; 2] {
        let safe_base_side = base_side.max(0.0);
        [safe_base_side * self.emitter_size_scale[0], safe_base_side * self.emitter_size_scale[1]]
    }

    #[must_use]
    pub fn visible_duration_s(self) -> f32 {
        self.active_duration_s + self.cell.lifetime_s
    }
}

/// One sampled particle instance for a CAEmitter-style burst at a specific time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BurstEmitterParticle {
    pub index: usize,
    pub spawn_time_s: f32,
    pub age_s: f32,
    pub source_offset: [f32; 3],
    pub emission_angle_rad: f32,
    pub rect: RectF,
}

/// Deterministic particle sampler for legacy CAEmitter-style image bursts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BurstEmitter {
    config: BurstEmitterConfig,
    started_ms: u64,
    seed: u64,
}

impl BurstEmitter {
    #[must_use]
    pub fn new(config: BurstEmitterConfig, started_ms: u64, seed: u64) -> Self {
        Self { config: config.sanitized(), started_ms, seed }
    }

    #[must_use]
    pub fn config(self) -> BurstEmitterConfig {
        self.config
    }

    #[must_use]
    pub fn started_ms(self) -> u64 {
        self.started_ms
    }

    #[must_use]
    pub fn seed(self) -> u64 {
        self.seed
    }

    #[must_use]
    pub fn emission_end_ms(self) -> u64 {
        self.started_ms
            .saturating_add((self.config.active_duration_s * 1000.0).round().max(0.0) as u64)
    }

    #[must_use]
    pub fn visible_end_ms(self) -> u64 {
        self.started_ms
            .saturating_add((self.config.visible_duration_s() * 1000.0).round().max(0.0) as u64)
    }

    #[must_use]
    pub fn emitted_particle_capacity(self) -> usize {
        if self.config.active_duration_s <= 0.0 || self.config.cell.birth_rate <= 0.0 {
            return 0;
        }
        (self.config.active_duration_s * self.config.cell.birth_rate).ceil().max(0.0) as usize
    }

    #[must_use]
    pub fn spawned_particle_count(self, now_ms: u64) -> usize {
        let capacity = self.emitted_particle_capacity();
        if capacity == 0 {
            return 0;
        }

        let elapsed_s = self.elapsed_seconds(now_ms).clamp(0.0, self.config.active_duration_s);
        let seeded_particles = (elapsed_s * self.config.cell.birth_rate - 0.5).floor() + 1.0;
        seeded_particles.clamp(0.0, capacity as f32) as usize
    }

    #[must_use]
    pub fn particles(
        self,
        now_ms: u64,
        emitter_center: [f32; 2],
        base_side: f32,
    ) -> Vec<BurstEmitterParticle> {
        let capacity = self.spawned_particle_count(now_ms);
        let mut particles = Vec::with_capacity(capacity);

        for index in 0..capacity {
            if let Some(particle) = self.particle(index, now_ms, emitter_center, base_side) {
                particles.push(particle);
            }
        }

        particles
    }

    #[must_use]
    pub fn particle(
        self,
        index: usize,
        now_ms: u64,
        emitter_center: [f32; 2],
        base_side: f32,
    ) -> Option<BurstEmitterParticle> {
        if self.config.cell.lifetime_s <= 0.0 {
            return None;
        }
        let spawn_time_s = self.spawn_time_s(index)?;
        let spawn_time_ms = self.spawn_time_ms(index)?;
        let elapsed_ms = now_ms.saturating_sub(self.started_ms);
        if elapsed_ms < spawn_time_ms {
            return None;
        }
        let age_ms = elapsed_ms - spawn_time_ms;
        let lifetime_ms = (self.config.cell.lifetime_s * 1000.0).round().max(0.0) as u64;
        if age_ms >= lifetime_ms {
            return None;
        }
        let age_s = age_ms as f32 / 1000.0;

        let source_offset = self.sample_source_offset(index, base_side);
        let emission_angle_rad = self.sample_emission_angle(index);
        let displacement = self.config.cell.velocity_points_per_s * age_s;
        let center_x =
            emitter_center[0] + source_offset[0] + emission_angle_rad.cos() * displacement;
        let center_y =
            emitter_center[1] + source_offset[1] + emission_angle_rad.sin() * displacement;
        let side = base_side.max(0.0) * self.config.cell.scale;
        let rect = RectF::new(center_x - side * 0.50, center_y - side * 0.50, side, side);

        Some(BurstEmitterParticle {
            index,
            spawn_time_s,
            age_s,
            source_offset,
            emission_angle_rad,
            rect,
        })
    }

    fn elapsed_seconds(self, now_ms: u64) -> f32 {
        now_ms.saturating_sub(self.started_ms) as f32 / 1000.0
    }

    fn spawn_time_s(self, index: usize) -> Option<f32> {
        if self.config.cell.birth_rate <= 0.0 {
            return None;
        }

        let spawn_time_s = (index as f32 + 0.5) / self.config.cell.birth_rate;
        if spawn_time_s > self.config.active_duration_s {
            return None;
        }
        Some(spawn_time_s)
    }

    #[must_use]
    pub fn spawn_time_s_for_index(self, index: usize) -> Option<f32> {
        self.spawn_time_s(index)
    }

    fn spawn_time_ms(self, index: usize) -> Option<u64> {
        self.spawn_time_s(index).map(|spawn_time_s| (spawn_time_s * 1000.0).round().max(0.0) as u64)
    }

    fn sample_source_offset(self, index: usize, base_side: f32) -> [f32; 3] {
        let emitter_size = self.config.emitter_size(base_side);
        match self.config.emitter_shape {
            BurstEmitterShape::Sphere => {
                let seed = self.particle_seed(index as u64, 0x5A17);
                sample_ellipsoid_point(seed, emitter_size, self.config.emitter_depth)
            }
        }
    }

    fn sample_emission_angle(self, index: usize) -> f32 {
        let spread = self.config.cell.emission_range_rad * 0.50;
        let unit = unit_interval(self.particle_seed(index as u64, 0x91D4));
        self.config.cell.emission_longitude_rad + (unit * 2.0 - 1.0) * spread
    }

    fn particle_seed(self, index: u64, salt: u64) -> u64 {
        splitmix64(
            self.seed
                ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ salt.wrapping_mul(0xD1B5_4A32_D192_ED03),
        )
    }
}

fn sample_ellipsoid_point(seed: u64, size: [f32; 2], depth: f32) -> [f32; 3] {
    let radius = unit_interval(seed ^ 0xA5A5_5A5A_A5A5_5A5A).powf(1.0 / 3.0);
    let cos_theta = unit_interval(seed ^ 0xC3C3_3C3C_C3C3_3C3C) * 2.0 - 1.0;
    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
    let phi = unit_interval(seed ^ 0xF0F0_0F0F_F0F0_0F0F) * TAU;

    [
        radius * sin_theta * phi.cos() * size[0].max(0.0) * 0.50,
        radius * sin_theta * phi.sin() * size[1].max(0.0) * 0.50,
        radius * cos_theta * depth.max(0.0) * 0.50,
    ]
}

fn unit_interval(seed: u64) -> f32 {
    let bits = (splitmix64(seed) >> 40) as u32;
    bits as f32 / 16_777_215.0
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}
