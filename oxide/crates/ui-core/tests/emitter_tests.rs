use oxide_ui_core::{BurstEmitter, BurstEmitterCellConfig, BurstEmitterConfig, BurstEmitterShape};

fn legacy_ghost_emitter() -> BurstEmitter {
    BurstEmitter::new(
        BurstEmitterConfig {
            active_duration_s: 1.1,
            emitter_size_scale: [1.5, 1.5],
            emitter_depth: 15.0,
            emitter_shape: BurstEmitterShape::Sphere,
            cell: BurstEmitterCellConfig {
                birth_rate: 25.0,
                lifetime_s: 1.0,
                velocity_points_per_s: 300.0,
                scale: 0.10,
                emission_range_rad: std::f32::consts::PI * 2.0,
                emission_longitude_rad: 0.0,
            },
        },
        2_000,
        77,
    )
}

#[test]
fn burst_emitter_matches_legacy_ghost_capacity_and_tail_window() {
    let emitter = legacy_ghost_emitter();

    assert_eq!(emitter.emitted_particle_capacity(), 28);
    assert_eq!(emitter.spawned_particle_count(2_000), 0);
    assert!(emitter.spawned_particle_count(2_020) >= 1);
    assert_eq!(emitter.spawned_particle_count(emitter.emission_end_ms()), 28);

    assert!(!emitter.particles(4_099, [120.0, 90.0], 32.0).is_empty());
    assert!(emitter.particles(emitter.visible_end_ms(), [120.0, 90.0], 32.0).is_empty());
}

#[test]
fn burst_emitter_particles_stay_inside_legacy_spherical_source_volume_at_birth() {
    let emitter = legacy_ghost_emitter();
    let emitter_size = emitter.config().emitter_size(32.0);

    for index in 0..emitter.emitted_particle_capacity() {
        let spawn_ms = emitter.started_ms()
            + (emitter.spawn_time_s_for_index(index).expect("spawn time") * 1000.0).round() as u64;
        let particle =
            emitter.particle(index, spawn_ms + 1, [0.0, 0.0], 32.0).expect("particle at birth");

        assert!(particle.source_offset[0].abs() <= emitter_size[0] * 0.50 + 0.001);
        assert!(particle.source_offset[1].abs() <= emitter_size[1] * 0.50 + 0.001);
        assert!(particle.source_offset[2].abs() <= emitter.config().emitter_depth * 0.50 + 0.001);
        assert!((particle.rect.w - 3.2).abs() <= 0.001);
        assert!((particle.rect.h - 3.2).abs() <= 0.001);
    }
}

#[test]
fn burst_emitter_uses_full_range_legacy_emission_angles() {
    let emitter = legacy_ghost_emitter();
    let particles = emitter.particles(3_100, [40.0, 60.0], 32.0);
    let min_angle =
        particles.iter().map(|particle| particle.emission_angle_rad).fold(f32::INFINITY, f32::min);
    let max_angle = particles
        .iter()
        .map(|particle| particle.emission_angle_rad)
        .fold(f32::NEG_INFINITY, f32::max);

    assert!(!particles.is_empty());
    assert!(min_angle >= -std::f32::consts::PI - 0.001);
    assert!(max_angle <= std::f32::consts::PI + 0.001);
    assert!(min_angle < -0.5);
    assert!(max_angle > 0.5);
}
