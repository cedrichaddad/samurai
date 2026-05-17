//! Lock-on system. While engaged, the player auto-rotates toward the locked
//! target each tick and movement keys become strafing relative to the target.
//!
//! Key: T toggles. The camera reads `LockOn.engaged` to choose its framing.

use crate::game::boss::Boss;
use crate::game::player::Player;
use bevy::prelude::*;

#[derive(Resource, Default, Debug)]
pub struct LockOn {
    pub engaged: bool,
    pub target: Option<Entity>,
}

pub fn toggle_lock_on_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut lock: ResMut<LockOn>,
    boss_q: Query<Entity, With<Boss>>,
) {
    // Souls-likes convention: click the right stick (R3) to toggle lock-on.
    // Keyboard equivalent is `T`.
    let toggle_pressed = keyboard.just_pressed(KeyCode::KeyT)
        || gamepads
            .iter()
            .any(|g| g.just_pressed(GamepadButton::RightThumb));
    if toggle_pressed {
        if lock.engaged {
            lock.engaged = false;
            lock.target = None;
        } else if let Ok(e) = boss_q.get_single() {
            lock.engaged = true;
            lock.target = Some(e);
        }
    }
    // Drop a stale target.
    if let Some(t) = lock.target {
        if boss_q.get(t).is_err() {
            lock.engaged = false;
            lock.target = None;
        }
    }
}

/// Rotate the player toward the locked target each tick while engaged.
pub fn face_target_system(
    real_time: Res<Time<Real>>,
    lock: Res<LockOn>,
    boss_q: Query<&Transform, (With<Boss>, Without<Player>)>,
    mut player_q: Query<&mut Transform, With<Player>>,
) {
    if !lock.engaged {
        return;
    }
    let Some(target) = lock.target else {
        return;
    };
    let Ok(boss_tf) = boss_q.get(target) else { return };
    let Ok(mut tf) = player_q.get_single_mut() else { return };
    let to_target = (boss_tf.translation - tf.translation).normalize_or_zero();
    if to_target.length_squared() < 1e-4 {
        return;
    }
    let target_rot = Quat::from_rotation_arc(Vec3::Z, to_target);
    let dt = real_time.delta_secs().min(0.05);
    let alpha = 1.0 - (-18.0 * dt).exp();
    tf.rotation = tf.rotation.slerp(target_rot, alpha);
}
