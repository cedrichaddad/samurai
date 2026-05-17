use bevy::prelude::*;
use crate::game::combat::{
    ActionTimer, CharacterState, FrameWindow, Velocity, DODGE_DURATION, PARRY_DURATION,
};
use crate::game::feel::{
    ATTACK_LUNGE_FORCE, AUTO_SNAP_K, AUTO_SNAP_RANGE, COMBAT_BLEND_MS, COMBO_CANCEL_TAIL_FRAMES,
    COMBO_CHAIN_MAX, COMBO_CHAIN_TIMEOUT_S, DODGE_IFRAMES, DODGE_RECOVERY, DODGE_STARTUP,
    EXECUTE_RANGE, HEAVY_ACTIVE, HEAVY_RECOVERY, HEAVY_STARTUP, LIGHT_ACTIVE, LIGHT_RECOVERY,
    LIGHT_STARTUP, NAV_BLEND_MS, PARRY_PERFECT_FRAMES, PARRY_RECOVERY, PARRY_TOTAL, ROTATION_K,
    STUN_BLEND_MS,
};
use crate::game::hitstop::{HitEvent, HitKind};
use crate::game::input::{BufferedAction, InputBuffer};
use crate::game::lockon::LockOn;
use crate::game::posture::Posture;

// --- RESOURCES ---
#[derive(Resource)]
pub struct PlayerAssets {
    pub scene: Handle<Scene>,
    pub anim_idle: Handle<AnimationClip>,
    pub anim_run: Handle<AnimationClip>,
    pub anim_attack: Handle<AnimationClip>,
    pub anim_parry: Handle<AnimationClip>,
    pub anim_dodge: Handle<AnimationClip>,
    pub anim_hit: Handle<AnimationClip>,
}

pub fn load_player_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(PlayerAssets {
        scene: asset_server.load("player.glb#Scene0"),
        anim_idle: asset_server.load("player.glb#Animation0"),
        anim_attack: asset_server.load("player.glb#Animation1"),
        anim_parry: asset_server.load("player.glb#Animation2"),
        anim_dodge: asset_server.load("player.glb#Animation3"),
        anim_run: asset_server.load("player.glb#Animation4"),
        anim_hit: asset_server.load("player.glb#Animation5"),
    });
}

// --- COMPONENTS ---
#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct Speed(pub f32);

/// Combo state on the player. Chains light attacks (and a heavy as the
/// finisher) together with shorter recovery on each step.
#[derive(Component, Default, Debug)]
pub struct ComboState {
    pub step: u8,
    pub last_step_time: f32,
}

#[derive(Component)]
pub struct PlayerAnimationIndices {
    pub idle: AnimationNodeIndex,
    pub run: AnimationNodeIndex,
    pub attack: AnimationNodeIndex,
    pub parry: AnimationNodeIndex,
    pub dodge: AnimationNodeIndex,
    pub hit: AnimationNodeIndex,
}

// --- SPAWNING ---
pub fn spawn_player(
    mut commands: Commands,
    assets: Res<PlayerAssets>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut graph = AnimationGraph::new();
    let idle = graph.add_clip(assets.anim_idle.clone(), 1.0, graph.root);
    let run = graph.add_clip(assets.anim_run.clone(), 1.0, graph.root);
    let attack = graph.add_clip(assets.anim_attack.clone(), 1.0, graph.root);
    let parry = graph.add_clip(assets.anim_parry.clone(), 1.0, graph.root);
    let dodge = graph.add_clip(assets.anim_dodge.clone(), 1.0, graph.root);
    let hit = graph.add_clip(assets.anim_hit.clone(), 1.0, graph.root);

    let graph_handle = graphs.add(graph);

    // Primitive body — a capsule placeholder. Guaranteed visible regardless of
    // GLB load state. The GLB scene spawns *on top of* this and replaces the
    // visible character when it lands; until then the player can always tell
    // where they are.
    let body_mesh = meshes.add(Capsule3d::new(0.35, 1.0));
    let body_color = Color::srgb(0.42, 0.55, 0.95);
    let body_mat = materials.add(StandardMaterial {
        base_color: body_color,
        emissive: body_color.to_linear() * 0.85,
        metallic: 0.30,
        perceptual_roughness: 0.40,
        cull_mode: None,
        ..default()
    });

    commands
        .spawn((
            Player,
            Speed(5.0),
            SceneRoot(assets.scene.clone()),
            Transform::from_xyz(-2.0, 0.0, 0.0),
            PlayerAnimationIndices { idle, run, attack, parry, dodge, hit },
            crate::game::combat::CharacterState::Idle,
            crate::game::combat::ActionTimer::default(),
            crate::game::combat::FrameWindow::default(),
            crate::game::combat::Health::default(),
            crate::game::combat::Hitbox::default(),
            crate::game::combat::Hurtbox::default(),
            crate::game::combat::Velocity::default(),
            crate::game::posture::Posture::new(100.0),
            ComboState::default(),
            crate::game::vfx::DodgeDustOnce::default(),
        ))
        .insert(crate::game::ai::PreviousPosition::default())
        .insert(AnimationGraphHandle(graph_handle))
        .insert(AnimationTransitions::default())
        .with_children(|c| {
            c.spawn((
                Mesh3d(body_mesh),
                MeshMaterial3d(body_mat),
                Transform::from_xyz(0.0, 0.85, 0.0),
            ));
        });
}

// --- ANIMATION CONTROLLER ---
pub fn player_animation_controller(
    player_query: Query<
        (Entity, &CharacterState, &PlayerAnimationIndices),
        (With<Player>, Changed<CharacterState>),
    >,
    children_query: Query<&Children>,
    mut animation_players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    for (entity, state, indices) in player_query.iter() {
        for descendant in children_query.iter_descendants(entity) {
            if let Ok((mut player, mut transitions)) = animation_players.get_mut(descendant) {
                let (index, should_repeat, blend_ms) = match state {
                    CharacterState::Idle => (indices.idle, true, NAV_BLEND_MS),
                    CharacterState::Move => (indices.run, true, NAV_BLEND_MS),
                    CharacterState::Attack => (indices.attack, false, COMBAT_BLEND_MS),
                    CharacterState::Parry | CharacterState::Block => {
                        (indices.parry, false, COMBAT_BLEND_MS)
                    }
                    CharacterState::Dodge => (indices.dodge, false, COMBAT_BLEND_MS),
                    CharacterState::Stunned => (indices.hit, false, STUN_BLEND_MS),
                };
                transitions.play(&mut player, index, std::time::Duration::from_millis(blend_ms));
                if should_repeat {
                    player.animation_mut(index).unwrap().repeat();
                }
                break;
            }
        }
    }
}

pub fn link_animation_components(
    mut commands: Commands,
    root_query: Query<(Entity, &AnimationGraphHandle), With<Player>>,
    children_query: Query<&Children>,
    player_query: Query<Entity, With<AnimationPlayer>>,
) {
    for (root_entity, graph_handle) in &root_query {
        for descendant in children_query.iter_descendants(root_entity) {
            if player_query.contains(descendant) {
                commands.entity(descendant).insert((
                    graph_handle.clone(),
                    AnimationTransitions::default(),
                ));
                commands.entity(root_entity).remove::<AnimationGraphHandle>();
                println!("Animation system linked for Player!");
                break;
            }
        }
    }
}

pub fn player_movement(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    time: Res<Time>,
    lock_on: Res<LockOn>,
    mut player_query: Query<
        (&mut Transform, &Speed, &mut CharacterState, &mut Velocity),
        With<Player>,
    >,
    camera_query: Query<
        &Transform,
        (With<crate::game::camera::MainCamera>, Without<Player>),
    >,
) {
    let Ok(camera_transform) = camera_query.get_single() else {
        return;
    };

    for (mut transform, speed, mut state, mut velocity) in &mut player_query {
        // Movement only allowed in Idle/Move. Block / Parry / Attack / Dodge
        // / Stunned all root the player.
        if *state != CharacterState::Idle && *state != CharacterState::Move {
            continue;
        }

        let mut direction = Vec3::ZERO;
        let forward = camera_transform.forward();
        let right = camera_transform.right();
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

        // Gamepad left stick — camera-relative, with a small radial deadzone.
        // Magnitude is preserved so analog leans give analog speed.
        for gamepad in &gamepads {
            let stick = gamepad.left_stick();
            let mag = stick.length();
            if mag < 0.18 {
                continue;
            }
            // Re-map [0.18, 1.0] → [0.0, 1.0] so deadzone doesn't truncate top speed.
            let rescaled = ((mag - 0.18) / 0.82).min(1.0);
            let unit = stick / mag;
            direction += forward_xz * unit.y * rescaled + right_xz * unit.x * rescaled;
        }

        if direction.length_squared() > 0.0 {
            let magnitude = direction.length().min(1.0);
            direction = direction.normalize();
            velocity.0 = direction * speed.0 * magnitude;

            // Smooth-rotate. When lock-on is engaged, lockon::face_target_system
            // overrides this rotation each tick anyway.
            if !lock_on.engaged {
                let target_rotation = Quat::from_rotation_arc(Vec3::Z, direction);
                let dt = time.delta_secs().min(0.05);
                let alpha = 1.0 - (-ROTATION_K * dt).exp();
                transform.rotation = transform.rotation.slerp(target_rotation, alpha);
            }

            *state = CharacterState::Move;
        } else {
            *state = CharacterState::Idle;
        }
    }
}

/// Resolve player input into actions, with universal cancel rules so the
/// game reads as fluid:
/// - Parry / Dodge can fire from any non-Idle/Move state EXCEPT during the
///   active hit window of an attack (committed frames).
/// - Light/Heavy attacks can fire from Idle/Move OR from the cancel tail of
///   the prior attack's recovery (combo chain).
/// - Attack inputs from Parry recovery / Dodge recovery / Block also fire.
/// All cancels consume from the input buffer so no presses are lost.
pub fn player_combat(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut buffer: ResMut<InputBuffer>,
    mut query: Query<
        (
            &mut CharacterState,
            &mut ActionTimer,
            &mut FrameWindow,
            &mut ComboState,
            &mut Velocity,
            &Transform,
        ),
        With<Player>,
    >,
    boss_q: Query<
        &Transform,
        (
            With<crate::game::boss::Boss>,
            Without<Player>,
        ),
    >,
    camera_q: Query<
        &Transform,
        (
            With<crate::game::camera::MainCamera>,
            Without<Player>,
            Without<crate::game::boss::Boss>,
        ),
    >,
) {
    let now = time.elapsed_secs();
    for (mut state, mut timer, mut window, mut combo, mut velocity, tf) in &mut query {
        if now - combo.last_step_time > COMBO_CHAIN_TIMEOUT_S {
            combo.step = 0;
        }

        // Classify cancel windows.
        let in_attack_active =
            matches!(*state, CharacterState::Attack) && window.in_active();
        let in_attack_recovery_tail = matches!(*state, CharacterState::Attack)
            && window.in_recovery()
            && window.remaining() <= COMBO_CANCEL_TAIL_FRAMES;
        let in_attack_startup =
            matches!(*state, CharacterState::Attack) && window.in_startup();
        let in_parry_recovery =
            matches!(*state, CharacterState::Parry) && window.in_recovery();
        let in_dodge_recovery =
            matches!(*state, CharacterState::Dodge) && window.in_recovery();
        let in_block = matches!(*state, CharacterState::Block);

        let neutral = matches!(*state, CharacterState::Idle | CharacterState::Move);

        // Defensive cancels (Parry / Dodge) — allowed from anywhere except
        // committed active frames of an attack and from Stunned.
        let can_cancel_defensive = neutral
            || in_attack_startup
            || in_attack_recovery_tail
            || in_parry_recovery
            || in_dodge_recovery
            || in_block;

        // Attack inputs — only reset combo from non-Attack states.
        let can_cancel_attack = neutral
            || in_attack_recovery_tail
            || in_parry_recovery
            || in_dodge_recovery
            || in_block;

        // Forbid acting during active attack frames or while stunned.
        let active_or_stunned = in_attack_active || matches!(*state, CharacterState::Stunned);
        let _ = active_or_stunned; // for clarity; covered by `can_cancel_*`.

        let mut consumed = false;

        // Light attack (combo capable).
        if can_cancel_attack
            && buffer
                .consume(|a| a == BufferedAction::LightAttack)
                .is_some()
        {
            let next_step = if matches!(*state, CharacterState::Attack) {
                combo.step.saturating_add(1).min(COMBO_CHAIN_MAX)
            } else {
                1
            };
            let recovery = match next_step {
                1 => LIGHT_RECOVERY,
                2 => LIGHT_RECOVERY.saturating_sub(4),
                _ => LIGHT_RECOVERY.saturating_sub(6),
            };
            *state = CharacterState::Attack;
            *window = FrameWindow::new(LIGHT_STARTUP, LIGHT_ACTIVE, recovery);
            timer.timer = Timer::from_seconds(window.total_secs(), TimerMode::Once);
            timer.next_state = Some(CharacterState::Idle);
            combo.step = next_step;
            combo.last_step_time = now;
            apply_attack_lunge(&mut velocity, tf, boss_q.get_single().ok(), 1.0);
            consumed = true;
        }

        // Heavy attack (combo finisher).
        if !consumed
            && can_cancel_attack
            && buffer
                .consume(|a| a == BufferedAction::HeavyAttack)
                .is_some()
        {
            *state = CharacterState::Attack;
            *window = FrameWindow::new(HEAVY_STARTUP, HEAVY_ACTIVE, HEAVY_RECOVERY);
            timer.timer = Timer::from_seconds(window.total_secs(), TimerMode::Once);
            timer.next_state = Some(CharacterState::Idle);
            combo.step = COMBO_CHAIN_MAX;
            combo.last_step_time = now;
            apply_attack_lunge(&mut velocity, tf, boss_q.get_single().ok(), 1.4);
            consumed = true;
        }

        // Parry — defensive cancel of anything except the committed attack
        // active window.
        if !consumed
            && can_cancel_defensive
            && buffer.consume(|a| a == BufferedAction::Parry).is_some()
        {
            *state = CharacterState::Parry;
            *window = FrameWindow::new(0, PARRY_TOTAL, PARRY_RECOVERY)
                .with_perfect(PARRY_PERFECT_FRAMES);
            timer.timer = Timer::from_seconds(PARRY_DURATION, TimerMode::Once);
            timer.next_state = Some(CharacterState::Idle);
            combo.step = 0;
            consumed = true;
        }

        // Dodge — same cancel rules as parry. Direction comes from WASD at
        // the moment of dodge (camera-relative); the boss is the fallback
        // reference axis when no key is held.
        if !consumed
            && can_cancel_defensive
            && buffer.consume(|a| a == BufferedAction::Dodge).is_some()
        {
            *state = CharacterState::Dodge;
            *window = FrameWindow::new(DODGE_STARTUP, DODGE_IFRAMES, DODGE_RECOVERY);
            timer.timer = Timer::from_seconds(DODGE_DURATION, TimerMode::Once);
            timer.next_state = Some(CharacterState::Idle);
            combo.step = 0;
            velocity.0 = resolve_dodge_velocity(&keyboard, &gamepads, tf, &boss_q, &camera_q);
        }
        // Suppress unused variable warning when no constants apply at runtime.
        let _ = EXECUTE_RANGE;
    }
}

/// Pick a dodge direction from the held WASD keys (camera-relative). With no
/// keys held, dodge away from the boss. The forward dodge ("step-in") has a
/// shorter push so the player doesn't sail past the boss; lateral and back
/// dodges are full strength.
fn resolve_dodge_velocity(
    keyboard: &ButtonInput<KeyCode>,
    gamepads: &Query<&Gamepad>,
    player_tf: &Transform,
    boss_q: &Query<
        &Transform,
        (
            With<crate::game::boss::Boss>,
            Without<Player>,
        ),
    >,
    camera_q: &Query<
        &Transform,
        (
            With<crate::game::camera::MainCamera>,
            Without<Player>,
            Without<crate::game::boss::Boss>,
        ),
    >,
) -> Vec3 {
    let (forward_xz, right_xz) = match camera_q.get_single() {
        Ok(cam_tf) => {
            let f = cam_tf.forward();
            let r = cam_tf.right();
            (
                Vec3::new(f.x, 0.0, f.z).normalize_or_zero(),
                Vec3::new(r.x, 0.0, r.z).normalize_or_zero(),
            )
        }
        Err(_) => {
            let f = player_tf.forward();
            let r = player_tf.right();
            (
                Vec3::new(f.x, 0.0, f.z).normalize_or_zero(),
                Vec3::new(r.x, 0.0, r.z).normalize_or_zero(),
            )
        }
    };

    let mut dir = Vec3::ZERO;
    let mut forward_request = false;
    if keyboard.pressed(KeyCode::KeyW) {
        dir += forward_xz;
        forward_request = true;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        dir -= forward_xz;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        dir -= right_xz;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        dir += right_xz;
    }
    // Gamepad left stick contributes to dodge direction with the same
    // semantics as movement. If the stick has any meaningful tilt, treat
    // it as the directional intent.
    for gamepad in gamepads {
        let stick = gamepad.left_stick();
        if stick.length() < 0.20 {
            continue;
        }
        dir += forward_xz * stick.y + right_xz * stick.x;
        if stick.y > 0.30 {
            forward_request = true;
        }
    }
    let push = if forward_request { 7.5 } else { 10.0 };
    if dir.length_squared() > 0.001 {
        return dir.normalize() * push;
    }
    // No input: dodge straight back, away from the boss.
    if let Ok(boss_tf) = boss_q.get_single() {
        let away = (player_tf.translation - boss_tf.translation)
            .with_y(0.0)
            .normalize_or_zero();
        return away * 10.0;
    }
    let back = -player_tf.forward();
    Vec3::new(back.x, 0.0, back.z) * 10.0
}

fn apply_attack_lunge(
    velocity: &mut Velocity,
    tf: &Transform,
    boss_tf: Option<&Transform>,
    scale: f32,
) {
    let forward = tf.forward();
    let forward_xz = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    if let Some(b) = boss_tf {
        let dir = (b.translation - tf.translation)
            .with_y(0.0)
            .normalize_or_zero();
        velocity.0 = (dir * 0.6 + forward_xz * 0.4) * (ATTACK_LUNGE_FORCE * scale);
    } else {
        velocity.0 += forward_xz * (ATTACK_LUNGE_FORCE * scale);
    }
}

/// Hold-to-block: when a parry's active window expires AND the player still
/// holds the parry button, lock into Block. Release to leave Block. Block
/// can be held via keyboard Q or gamepad L1.
pub fn maintain_block_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut q: Query<(&mut CharacterState, &FrameWindow, &mut ActionTimer), With<Player>>,
) {
    let parry_held = keyboard.pressed(KeyCode::KeyQ)
        || gamepads
            .iter()
            .any(|g| g.pressed(GamepadButton::LeftTrigger));
    for (mut state, win, mut timer) in &mut q {
        match *state {
            CharacterState::Parry => {
                if win.elapsed >= win.startup + win.active && parry_held {
                    *state = CharacterState::Block;
                    timer.timer.pause();
                }
            }
            CharacterState::Block => {
                if !parry_held {
                    *state = CharacterState::Idle;
                }
            }
            _ => {}
        }
    }
}

/// During attack startup, slerp toward the locked-on target (or the nearest
/// boss within range). Eliminates the "I committed but missed by 5 degrees"
/// problem without making the player feel guided.
pub fn auto_snap_attack_aim_system(
    real_time: Res<Time<Real>>,
    lock: Res<LockOn>,
    mut player_q: Query<(&mut Transform, &CharacterState, &FrameWindow), With<Player>>,
    boss_q: Query<&Transform, (With<crate::game::boss::Boss>, Without<Player>)>,
) {
    let Ok((mut tf, state, win)) = player_q.get_single_mut() else {
        return;
    };
    let in_startup = matches!(*state, CharacterState::Attack) && win.in_startup();
    if !in_startup {
        return;
    }
    let target_pos = if let Some(target) = lock.target.filter(|_| lock.engaged) {
        boss_q.get(target).ok().map(|t| t.translation)
    } else {
        boss_q
            .iter()
            .min_by(|a, b| {
                let da = a.translation.distance_squared(tf.translation);
                let db = b.translation.distance_squared(tf.translation);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|t| t.translation)
    };
    let Some(target_pos) = target_pos else { return };
    if tf.translation.distance(target_pos) > AUTO_SNAP_RANGE {
        return;
    }
    let to_target = (target_pos - tf.translation)
        .with_y(0.0)
        .normalize_or_zero();
    if to_target.length_squared() < 1e-4 {
        return;
    }
    let target_rot = Quat::from_rotation_arc(Vec3::Z, to_target);
    let dt = real_time.delta_secs().min(0.05);
    let alpha = 1.0 - (-AUTO_SNAP_K * dt).exp();
    tf.rotation = tf.rotation.slerp(target_rot, alpha);
}

/// Posture-break execute: if the boss is broken and the player is in range,
/// consuming a buffered E press drops the boss to 0 HP and emits a Killed
/// event. The kill-cam slow-mo and FOV punch then fire via the existing
/// HitEvent pipeline.
pub fn try_execute_broken_boss_system(
    mut commands: Commands,
    mut buffer: ResMut<InputBuffer>,
    mut hit_events: EventWriter<HitEvent>,
    player_q: Query<(Entity, &Transform), With<Player>>,
    mut boss_q: Query<
        (Entity, &Transform, &Posture, &mut crate::game::combat::Health),
        With<crate::game::boss::Boss>,
    >,
) {
    let Ok((player_e, player_tf)) = player_q.get_single() else {
        return;
    };
    let Ok((boss_e, boss_tf, posture, mut hp)) = boss_q.get_single_mut() else {
        return;
    };
    if !posture.is_broken() || hp.current <= 0.0 {
        return;
    }
    if player_tf.translation.distance(boss_tf.translation) > EXECUTE_RANGE {
        return;
    }
    if buffer.consume(|a| a == BufferedAction::Execute).is_none() {
        return;
    }
    let damage = hp.current.max(1.0);
    hp.current = 0.0;
    hit_events.send(HitEvent {
        kind: HitKind::Killed,
        attacker: player_e,
        victim: boss_e,
        contact_point: boss_tf.translation + Vec3::new(0.0, 1.0, 0.0),
        damage_dealt: damage,
    });
    commands.entity(boss_e).despawn();
}
