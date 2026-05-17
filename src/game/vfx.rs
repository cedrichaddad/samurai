//! Visual effects: slash trails (sword tip ribbon), parry sparks, dust kicks,
//! damage / parry color flashes, posture-break kanji overlay, and unblockable
//! red telegraph.
//!
//! All effects are mesh-based ECS entities — no extra plugin dependency. They
//! auto-despawn on lifetime expiry.

use crate::game::boss::Boss;
use crate::game::combat::{CharacterState, FrameWindow};
use crate::game::hitstop::{HitEvent, HitKind};
use crate::game::player::Player;
use bevy::prelude::*;

// ─── Generic short-lived particle ──────────────────────────────────────────

#[derive(Component)]
pub struct Particle {
    pub lifetime: f32,
    pub age: f32,
    pub initial_scale: Vec3,
    pub fade_to_zero: bool,
    pub velocity: Vec3,
    pub gravity: f32,
}

pub fn tick_particles_system(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Particle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (e, mut p, mut tf) in &mut q {
        p.age += dt;
        if p.age >= p.lifetime {
            commands.entity(e).despawn();
            continue;
        }
        let t = (p.age / p.lifetime).clamp(0.0, 1.0);
        if p.fade_to_zero {
            let s = 1.0 - t;
            tf.scale = p.initial_scale * s;
        }
        p.velocity.y -= p.gravity * dt;
        tf.translation += p.velocity * dt;
    }
}

// ─── Parry sparks ──────────────────────────────────────────────────────────

pub fn spawn_parry_sparks_on_event_system(
    mut commands: Commands,
    mut events: EventReader<HitEvent>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut mesh_handle: Option<Handle<Mesh>> = None;
    for ev in events.read() {
        let (color, count, speed, scale, lifetime) = match ev.kind {
            HitKind::PerfectParried => (Color::srgb(1.0, 0.95, 0.6), 14, 4.8, 1.25, 0.32),
            HitKind::Parried => (Color::srgb(0.85, 0.85, 0.85), 12, 3.0, 1.00, 0.30),
            HitKind::Blocked => (Color::srgb(0.55, 0.65, 0.85), 7, 2.0, 0.85, 0.22),
            _ => continue,
        };
        if mesh_handle.is_none() {
            mesh_handle = Some(meshes.add(Cuboid::new(0.05, 0.05, 0.05)));
        }
        let mesh = mesh_handle.clone().unwrap();
        let material = materials.add(StandardMaterial {
            base_color: color,
            emissive: color.to_linear() * 4.0,
            unlit: true,
            ..default()
        });
        for i in 0..count {
            let angle = (i as f32) * std::f32::consts::TAU / (count as f32);
            let dir = Vec3::new(angle.cos(), 0.6, angle.sin()).normalize();
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(ev.contact_point)
                    .with_scale(Vec3::splat(scale)),
                Particle {
                    lifetime,
                    age: 0.0,
                    initial_scale: Vec3::splat(scale),
                    fade_to_zero: true,
                    velocity: dir * speed,
                    gravity: 8.0,
                },
            ));
        }
    }
}

// ─── Dust kicks on dodge ───────────────────────────────────────────────────

#[derive(Component, Default)]
pub struct DodgeDustOnce(pub bool);

pub fn spawn_dust_on_dodge_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<
        (&Transform, &CharacterState, &FrameWindow, &mut DodgeDustOnce),
        Or<(With<Player>, With<Boss>)>,
    >,
) {
    let mut mesh_handle: Option<Handle<Mesh>> = None;
    for (tf, state, win, mut once) in &mut q {
        if *state == CharacterState::Dodge && win.elapsed <= 1 {
            if once.0 {
                continue;
            }
            once.0 = true;
            if mesh_handle.is_none() {
                mesh_handle = Some(meshes.add(Cuboid::new(0.10, 0.05, 0.10)));
            }
            let mesh = mesh_handle.clone().unwrap();
            let material = materials.add(StandardMaterial {
                base_color: Color::srgba(0.65, 0.55, 0.45, 0.95),
                unlit: true,
                ..default()
            });
            for i in 0..8 {
                let angle = (i as f32) * std::f32::consts::TAU / 8.0;
                let dir = Vec3::new(angle.cos(), 0.2, angle.sin()).normalize();
                commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_translation(tf.translation + Vec3::Y * 0.1),
                    Particle {
                        lifetime: 0.22,
                        age: 0.0,
                        initial_scale: Vec3::ONE,
                        fade_to_zero: true,
                        velocity: dir * 2.0,
                        gravity: 4.0,
                    },
                ));
            }
        } else if *state != CharacterState::Dodge {
            once.0 = false;
        }
    }
}

// ─── Slash trail (sword-tip ribbon, simple emitter) ───────────────────────

pub fn emit_slash_trail_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q: Query<
        (
            &GlobalTransform,
            &CharacterState,
            &FrameWindow,
            Has<Player>,
            Option<&Unblockable>,
        ),
        Or<(With<Player>, With<Boss>)>,
    >,
) {
    let mut light_mesh: Option<Handle<Mesh>> = None;
    let mut heavy_mesh: Option<Handle<Mesh>> = None;
    for (gt, state, win, is_player, unblock) in &q {
        if *state != CharacterState::Attack || !win.in_active() {
            continue;
        }
        // Heavy attacks have startup >= 14 ticks; light attacks have <= 6.
        // Use this to pick a thicker, hotter trail for heavy strikes. The
        // unblockable telegraph also bumps the heavy treatment for the boss.
        let is_heavy = win.startup >= 12 || unblock.is_some();

        let tip_offset = Vec3::new(0.0, 1.5, 1.4);
        let tip_world = gt.translation() + gt.rotation() * tip_offset;

        let (mesh_slot, scale, emissive_mul, lifetime) = if is_heavy {
            (&mut heavy_mesh, 1.55, 5.0, 0.22)
        } else {
            (&mut light_mesh, 1.00, 3.0, 0.18)
        };
        if mesh_slot.is_none() {
            *mesh_slot = Some(meshes.add(Sphere::new(if is_heavy { 0.10 } else { 0.06 })));
        }

        // Color: player white-blue light, player white-amber heavy; boss
        // crimson light, hot-orange heavy / unblockable.
        let color = match (is_player, is_heavy) {
            (true, false) => Color::srgb(0.95, 0.97, 1.0),
            (true, true) => Color::srgb(1.0, 0.95, 0.55),
            (false, false) => Color::srgb(1.0, 0.4, 0.4),
            (false, true) => Color::srgb(1.0, 0.5, 0.15),
        };
        let material = materials.add(StandardMaterial {
            base_color: color,
            emissive: color.to_linear() * emissive_mul,
            unlit: true,
            ..default()
        });
        commands.spawn((
            Mesh3d(mesh_slot.clone().unwrap()),
            MeshMaterial3d(material),
            Transform::from_translation(tip_world).with_scale(Vec3::splat(scale)),
            Particle {
                lifetime,
                age: 0.0,
                initial_scale: Vec3::splat(scale),
                fade_to_zero: true,
                velocity: Vec3::ZERO,
                gravity: 0.0,
            },
        ));
    }
}

// ─── Stunned glow: persistent yellow emissive pulse while Stunned ─────────

#[derive(Component, Default)]
pub struct StunnedGlow;

pub fn drive_stunned_glow_system(
    mut commands: Commands,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    children_query: Query<&Children>,
    matq: Query<&MeshMaterial3d<StandardMaterial>>,
    q: Query<(Entity, &CharacterState), Or<(With<Player>, With<Boss>)>>,
) {
    let phase = (time.elapsed_secs() * 9.0).sin() * 0.5 + 0.5;
    let stun_color = Color::srgb(1.0, 0.85, 0.25).to_linear() * (1.5 + 1.5 * phase);
    for (e, state) in &q {
        let stunned = matches!(state, CharacterState::Stunned);
        if !stunned {
            commands.entity(e).remove::<StunnedGlow>();
            continue;
        }
        commands.entity(e).insert(StunnedGlow);
        // Apply emissive to all child mesh materials.
        let mut stack = vec![e];
        while let Some(cur) = stack.pop() {
            if let Ok(children) = children_query.get(cur) {
                for &c in children {
                    stack.push(c);
                }
            }
            if let Ok(mat_handle) = matq.get(cur) {
                if let Some(mat) = materials.get_mut(&mat_handle.0) {
                    mat.emissive = stun_color;
                }
            }
        }
    }
}

// ─── Damage / parry color flash on materials ──────────────────────────────

#[derive(Component)]
pub struct MaterialFlash {
    pub remaining_s: f32,
    pub total_s: f32,
    pub flash_color: Color,
}

pub fn spawn_material_flash_on_event_system(
    mut commands: Commands,
    mut events: EventReader<HitEvent>,
) {
    for ev in events.read() {
        let (color, total) = match ev.kind {
            HitKind::Connected => (Color::srgb(1.0, 0.25, 0.25), 0.08),
            HitKind::Killed => (Color::srgb(1.0, 0.0, 0.0), 0.12),
            HitKind::Blocked => (Color::srgb(0.55, 0.65, 0.85), 0.05),
            HitKind::Parried => (Color::srgb(0.8, 0.8, 0.8), 0.06),
            HitKind::PerfectParried => (Color::srgb(1.0, 1.0, 0.7), 0.10),
        };
        if let Some(mut e) = commands.get_entity(ev.victim) {
            e.insert(MaterialFlash {
                remaining_s: total,
                total_s: total,
                flash_color: color,
            });
        }
    }
}

pub fn tick_material_flash_system(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    children_query: Query<&Children>,
    matq: Query<&MeshMaterial3d<StandardMaterial>>,
    mut q: Query<(Entity, &mut MaterialFlash)>,
) {
    let dt = time.delta_secs();
    for (e, mut flash) in &mut q {
        flash.remaining_s = (flash.remaining_s - dt).max(0.0);
        let intensity = if flash.total_s > 0.0 {
            (flash.remaining_s / flash.total_s).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // Apply emissive to all child mesh materials.
        let mut stack = vec![e];
        while let Some(cur) = stack.pop() {
            if let Ok(children) = children_query.get(cur) {
                for &c in children {
                    stack.push(c);
                }
            }
            if let Ok(mat_handle) = matq.get(cur) {
                if let Some(mat) = materials.get_mut(&mat_handle.0) {
                    let lin = flash.flash_color.to_linear() * (4.0 * intensity);
                    mat.emissive = lin;
                }
            }
        }
        if flash.remaining_s == 0.0 {
            commands.entity(e).remove::<MaterialFlash>();
        }
    }
}

// ─── Unblockable red telegraph ─────────────────────────────────────────────

#[derive(Component)]
pub struct Unblockable {
    pub remaining_s: f32,
}

pub fn pulse_unblockable_outline_system(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    children_query: Query<&Children>,
    matq: Query<&MeshMaterial3d<StandardMaterial>>,
    mut q: Query<(Entity, &mut Unblockable)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    let phase = (time.elapsed_secs() * 14.0).sin() * 0.5 + 0.5;
    for (e, mut u) in &mut q {
        u.remaining_s = (u.remaining_s - dt).max(0.0);
        let pulse_color = Color::srgb(1.0, 0.1 + 0.7 * phase, 0.1);
        let mut stack = vec![e];
        while let Some(cur) = stack.pop() {
            if let Ok(children) = children_query.get(cur) {
                for &c in children {
                    stack.push(c);
                }
            }
            if let Ok(mat_handle) = matq.get(cur) {
                if let Some(mat) = materials.get_mut(&mat_handle.0) {
                    mat.emissive = pulse_color.to_linear() * (2.0 + 2.0 * phase);
                }
            }
        }
        if u.remaining_s == 0.0 {
            commands.entity(e).remove::<Unblockable>();
        }
    }
}
