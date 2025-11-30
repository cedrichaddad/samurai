use bevy::prelude::*;
use crate::game::combat::{CharacterState, ActionTimer};

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct Speed(pub f32);

pub fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Player,
        Speed(5.0),
        crate::game::combat::CharacterState::default(),
        crate::game::combat::ActionTimer::default(),
        crate::game::combat::Health::default(),
        crate::game::combat::Hitbox::default(),
        crate::game::combat::Hurtbox::default(),
        Mesh3d(meshes.add(Capsule3d::new(0.5, 1.8))),
        MeshMaterial3d(materials.add(Color::srgb(0.0, 0.0, 1.0))),
        Transform::from_xyz(-2.0, 1.0, 0.0),
    ))
    .insert(crate::game::ai::PreviousPosition::default());
}

pub fn player_movement(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut player_query: Query<(&mut Transform, &Speed, &mut CharacterState), With<Player>>,
    camera_query: Query<&Transform, (With<crate::game::camera::MainCamera>, Without<Player>)>,
) {
    let camera_transform = if let Ok(t) = camera_query.get_single() {
        t
    } else {
        return;
    };

    for (mut transform, speed, mut state) in &mut player_query {
        // Only allow movement in Idle or Move
        if *state != CharacterState::Idle && *state != CharacterState::Move {
            continue;
        }

        let mut direction = Vec3::ZERO;
        let forward = camera_transform.forward();
        let right = camera_transform.right();

        // Project onto XZ plane to avoid flying/digging
        let forward_xz = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
        let right_xz = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();

        if keyboard_input.pressed(KeyCode::KeyW) {
            direction += forward_xz;
        }
        if keyboard_input.pressed(KeyCode::KeyS) {
            direction -= forward_xz;
        }
        if keyboard_input.pressed(KeyCode::KeyA) {
            direction -= right_xz;
        }
        if keyboard_input.pressed(KeyCode::KeyD) {
            direction += right_xz;
        }

        if direction.length_squared() > 0.0 {
            direction = direction.normalize();
            transform.translation += direction * speed.0 * time.delta_secs();
            
            // Also rotate player to face movement direction
            let target_rotation = Quat::from_rotation_arc(Vec3::Z, direction);
            // Smooth rotation could be added here
            transform.rotation = target_rotation; // Snap for now

            *state = CharacterState::Move;
        } else {
            *state = CharacterState::Idle;
        }
    }
}

pub fn player_combat(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut CharacterState, &mut ActionTimer), With<Player>>,
) {
    for (mut state, mut timer) in &mut query {
        if *state != CharacterState::Idle && *state != CharacterState::Move {
            continue;
        }

        if keyboard_input.just_pressed(KeyCode::Space) {
            // Simple cooldown check: only attack if Idle or Move (which is already checked above)
            // But we want to prevent spamming immediately after.
            // The ActionTimer handles the duration of the attack.
            // So we can't attack WHILE attacking.
            // But we can attack immediately after.
            // Let's add a small delay?
            // Actually, the ActionTimer transitions to Idle.
            // If we want a cooldown, we need another timer or state.
            // For now, let's just rely on the animation duration (ActionTimer).
            *state = CharacterState::Attack;
            timer.timer = Timer::from_seconds(0.5, TimerMode::Once);
            timer.next_state = Some(CharacterState::Idle);
        } else if keyboard_input.just_pressed(KeyCode::KeyQ) { // Parry
            *state = CharacterState::Parry;
            timer.timer = Timer::from_seconds(0.2, TimerMode::Once);
            timer.next_state = Some(CharacterState::Idle);
        } else if keyboard_input.just_pressed(KeyCode::ShiftLeft) { // Dodge
            *state = CharacterState::Dodge;
            timer.timer = Timer::from_seconds(0.3, TimerMode::Once);
            timer.next_state = Some(CharacterState::Idle);
        }
    }
}
