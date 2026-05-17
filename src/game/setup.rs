//! Scene bootstrap: arena geometry, lighting, fog, and the rendering camera
//! with HDR + bloom + tonemapping.

use crate::game::combat::ARENA_RADIUS;
use bevy::core_pipeline::bloom::Bloom;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;

/// A subtle dark slate, used for both clear color and fog falloff color so
/// the arena recedes into the same value the sky uses.
pub const SKY_TINT: Color = Color::srgb(0.045, 0.055, 0.075);

pub fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // ─── Sky & background ──────────────────────────────────────────────────
    commands.insert_resource(ClearColor(SKY_TINT));

    // ─── Arena floor ───────────────────────────────────────────────────────
    // A dark, slightly cool stone disc — large enough that the play boundary
    // (radius 10) sits well inside the visible plane.
    let floor_radius = 24.0;
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(floor_radius))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.075, 0.080, 0.090),
            perceptual_roughness: 0.9,
            metallic: 0.0,
            reflectance: 0.05,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.002, 0.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));

    // A flatter, slightly warmer inner disc inside the play boundary; helps
    // the player read the safe-zone perimeter at a glance.
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(ARENA_RADIUS))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.105, 0.095, 0.085),
            perceptual_roughness: 0.85,
            metallic: 0.0,
            reflectance: 0.04,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.001, 0.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));

    // Arena boundary ring — annulus made from a torus, very thin, glowing
    // softly so the player can always see where the play area ends.
    commands.spawn((
        Mesh3d(meshes.add(Torus {
            minor_radius: 0.06,
            major_radius: ARENA_RADIUS,
        })),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.18, 0.20, 0.28),
            emissive: Color::srgb(0.12, 0.16, 0.30).to_linear() * 1.4,
            perceptual_roughness: 0.5,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.01, 0.0),
    ));

    // ─── Lights ────────────────────────────────────────────────────────────
    // Bumped from 70 → 280 lux so PBR-shaded characters with dark base colors
    // stay readable. The mood still reads "dusk" because the directional key
    // is warm and the fog/sky stays cool.
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.55, 0.60, 0.75),
        brightness: 280.0,
    });

    // Key light (warm sunset), high overhead at an angle. Bumped to 18k lux.
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(1.0, 0.93, 0.78),
            illuminance: 18_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(8.0, 12.0, 6.0)
            .looking_at(Vec3::new(0.0, 0.5, 0.0), Vec3::Y),
    ));

    // Fill light (cool dusk), opposite side, no shadows — Bevy 0.15 allows
    // at most one shadow-casting directional light by default.
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.45, 0.55, 0.85),
            illuminance: 6_500.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(-6.0, 8.0, -4.0)
            .looking_at(Vec3::new(0.0, 0.5, 0.0), Vec3::Y),
    ));

    // Warm rim/key light directly over the arena, large range, no shadows.
    // This is what gives the duelists their stage-spotlight feel.
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.92, 0.78),
            intensity: 250_000.0,
            range: 14.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 5.0, 0.0),
    ));

    // ─── Camera ────────────────────────────────────────────────────────────
    commands.spawn((
        Camera3d::default(),
        Camera {
            hdr: true,
            ..default()
        },
        // ACES is the cinematic tonemapper — handles bloom and highlights
        // without crushing shadows.
        Tonemapping::AcesFitted,
        Bloom::NATURAL,
        DistanceFog {
            color: SKY_TINT,
            falloff: FogFalloff::Linear {
                start: 18.0,
                end: 60.0,
            },
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: 55.0_f32.to_radians(),
            ..default()
        }),
        crate::game::camera::MainCamera,
        crate::game::camera::FovTween {
            base_fov: 55.0_f32.to_radians(),
            target_fov: 55.0_f32.to_radians(),
            remaining_s: 0.0,
            total_s: 0.0,
        },
        Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
