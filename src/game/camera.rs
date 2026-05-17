//! Combat-aware camera: frame-rate-independent damping, lock-on offset, FOV
//! pulses on parry/dodge, slow-mo on kill, and trauma-driven shake.

use crate::game::boss::Boss;
use crate::game::feel::{
    CAMERA_FOLLOW_K, CAMERA_FOV_HIT_DELTA, CAMERA_FOV_HIT_TWEEN_S,
    CAMERA_FOV_PARRY_DELTA, CAMERA_FOV_TWEEN_S, KILLCAM_DURATION_S, KILLCAM_TIME_SCALE,
};
use crate::game::hitstop::{HitEvent, HitKind};
use crate::game::lockon::LockOn;
use crate::game::player::Player;
use crate::game::shake::{rotation_shake, shake_offset, Trauma};
use bevy::prelude::*;

#[derive(Component)]
pub struct MainCamera;

/// Optional FOV tween: pull toward `target_fov` over `remaining_s`, then
/// release back to `base_fov`.
#[derive(Component, Debug, Clone, Copy)]
pub struct FovTween {
    pub base_fov: f32,
    pub target_fov: f32,
    pub remaining_s: f32,
    pub total_s: f32,
}

impl Default for FovTween {
    fn default() -> Self {
        Self {
            base_fov: std::f32::consts::FRAC_PI_4, // ~45° matches Bevy default
            target_fov: std::f32::consts::FRAC_PI_4,
            remaining_s: 0.0,
            total_s: 0.0,
        }
    }
}

impl FovTween {
    pub fn pulse(&mut self, base: f32, delta_deg: f32, total_s: f32) {
        self.base_fov = base;
        self.target_fov = base + delta_deg.to_radians();
        self.remaining_s = total_s;
        self.total_s = total_s;
    }
}

/// Slow-mo manager. While `remaining_s > 0`, time runs at
/// `KILLCAM_TIME_SCALE`; eases back at the end.
#[derive(Resource, Default, Debug)]
pub struct SlowMo {
    pub remaining_s: f32,
    pub scale: f32,
}

impl SlowMo {
    pub fn trigger(&mut self, scale: f32, duration_s: f32) {
        self.remaining_s = self.remaining_s.max(duration_s);
        self.scale = scale;
    }
    pub fn active(&self) -> bool {
        self.remaining_s > 0.0
    }
}

pub fn drive_slowmo_from_kill_events_system(
    mut events: EventReader<HitEvent>,
    mut slowmo: ResMut<SlowMo>,
    mut camera_q: Query<&mut FovTween, With<MainCamera>>,
) {
    let mut killed = false;
    let mut parried = false;
    let mut connected = false;
    let mut blocked = false;
    for ev in events.read() {
        match ev.kind {
            HitKind::Killed => killed = true,
            HitKind::Parried | HitKind::PerfectParried => parried = true,
            HitKind::Connected => connected = true,
            HitKind::Blocked => blocked = true,
        }
    }
    if killed {
        slowmo.trigger(KILLCAM_TIME_SCALE, KILLCAM_DURATION_S);
    }
    // Camera FOV pulses by event class, ordered from biggest to smallest. The
    // biggest event in this batch wins so we don't stack pulses on the same
    // frame and clip the easing curve.
    if let Ok(mut tween) = camera_q.get_single_mut() {
        if parried {
            let base = tween.base_fov;
            tween.pulse(base, CAMERA_FOV_PARRY_DELTA, CAMERA_FOV_TWEEN_S);
        } else if connected {
            let base = tween.base_fov;
            tween.pulse(base, CAMERA_FOV_HIT_DELTA, CAMERA_FOV_HIT_TWEEN_S);
        } else if blocked {
            let base = tween.base_fov;
            tween.pulse(base, CAMERA_FOV_HIT_DELTA * 0.5, CAMERA_FOV_HIT_TWEEN_S);
        }
    }
}

pub fn tick_slowmo_system(real_time: Res<Time<Real>>, mut slowmo: ResMut<SlowMo>, mut time: ResMut<Time<Virtual>>) {
    let dt = real_time.delta_secs();
    if slowmo.active() {
        slowmo.remaining_s = (slowmo.remaining_s - dt).max(0.0);
        // Ease back to 1.0 in the last 25% of the duration.
        let scale = if slowmo.remaining_s < 0.25 * KILLCAM_DURATION_S {
            let t = slowmo.remaining_s / (0.25 * KILLCAM_DURATION_S).max(0.001);
            slowmo.scale + (1.0 - slowmo.scale) * (1.0 - t)
        } else {
            slowmo.scale
        };
        time.set_relative_speed(scale.clamp(0.05, 1.0));
    } else if (time.relative_speed() - 1.0).abs() > 1e-3 {
        time.set_relative_speed(1.0);
    }
}

pub fn tick_fov_tween_system(
    real_time: Res<Time<Real>>,
    mut q: Query<(&mut FovTween, &mut Projection), With<MainCamera>>,
) {
    let dt = real_time.delta_secs();
    for (mut tween, mut proj) in &mut q {
        if tween.remaining_s > 0.0 {
            tween.remaining_s = (tween.remaining_s - dt).max(0.0);
            // 0..1 progress, eases in then out (sine half-arc).
            let t = if tween.total_s > 0.0 {
                1.0 - (tween.remaining_s / tween.total_s).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let arc = (t * std::f32::consts::PI).sin();
            let fov = tween.base_fov + (tween.target_fov - tween.base_fov) * arc;
            if let Projection::Perspective(p) = proj.as_mut() {
                p.fov = fov;
            }
        } else if let Projection::Perspective(p) = proj.as_mut() {
            p.fov = tween.base_fov;
        }
    }
}

/// Frame-rate-independent camera follow with lock-on and shake. Replaces the
/// old `lerp(_, 0.1)` model.
pub fn camera_follow(
    real_time: Res<Time<Real>>,
    trauma: Res<Trauma>,
    lock_on: Res<LockOn>,
    mut camera_q: Query<&mut Transform, With<MainCamera>>,
    player_q: Query<&Transform, (With<Player>, Without<Boss>, Without<MainCamera>)>,
    boss_q: Query<&Transform, (With<Boss>, Without<Player>, Without<MainCamera>)>,
) {
    let mut camera_transform = match camera_q.get_single_mut() {
        Ok(t) => t,
        Err(_) => return,
    };
    let player_pos = match player_q.get_single() {
        Ok(t) => t.translation,
        Err(_) => return,
    };
    let boss_pos = boss_q.get_single().map(|t| t.translation).unwrap_or(player_pos);

    let dt = real_time.delta_secs().min(0.05); // clamp huge frames
    let alpha = 1.0 - (-CAMERA_FOLLOW_K * dt).exp();

    // Compute a target position. With lock-on, frame the player and boss; the
    // camera trails the player with the boss in the back of frame.
    let target_pos = if lock_on.engaged {
        let to_boss = (boss_pos - player_pos).normalize_or_zero();
        let behind = -to_boss * 5.5;
        player_pos + behind + Vec3::new(0.0, 3.5, 0.0)
    } else {
        let midpoint = (player_pos + boss_pos) * 0.5;
        midpoint + Vec3::new(0.0, 5.0, 9.5)
    };
    let look_at = if lock_on.engaged {
        boss_pos + Vec3::new(0.0, 1.0, 0.0)
    } else {
        (player_pos + boss_pos) * 0.5 + Vec3::Y
    };

    camera_transform.translation = camera_transform.translation.lerp(target_pos, alpha);

    let shake = trauma.shake();
    let shake_pos = shake_offset(trauma.time, shake);
    let shake_yaw = rotation_shake(trauma.time, shake);

    camera_transform.translation += shake_pos;
    camera_transform.look_at(look_at, Vec3::Y);
    camera_transform.rotate_local_y(shake_yaw);
}
