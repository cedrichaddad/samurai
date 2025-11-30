use bevy::prelude::*;
use tch::{CModule, Tensor};
use crate::game::combat::{CharacterState, ActionTimer, Health};
use crate::game::player::Player;
use crate::game::boss::Boss;

#[derive(Resource)]
pub struct BossModel {
    model: CModule,
}

#[derive(Component, Default)]
pub struct PreviousPosition(pub Vec3);

#[derive(Component, Default)]
pub struct BossMemory {
    pub last_action: i64,
    pub history: Vec<f32>, // Flattened history buffer
}

pub fn load_boss_model(mut commands: Commands) {
    // Load the TorchScript model
    // Path needs to be correct.
    let model_path = "samurai_model.pt";
    match CModule::load(model_path) {
        Ok(model) => {
            commands.insert_resource(BossModel { model });
            println!("Boss model loaded successfully.");
        },
        Err(e) => {
            eprintln!("Failed to load boss model: {:?}", e);
        }
    }
}

pub fn boss_ai_system(
    mut boss_query: Query<(&mut Transform, &mut CharacterState, &mut ActionTimer, &Health, &mut crate::game::aggression::BossAggressionTimer, &mut PreviousPosition, &mut BossMemory, &mut crate::game::boss::BossAttackCooldown), With<Boss>>,
    mut player_query: Query<(&Transform, &CharacterState, &ActionTimer, &Health, &mut PreviousPosition), (With<Player>, Without<Boss>)>,
    boss_model: Option<Res<BossModel>>,
    player_stats: Res<crate::game::stats::PlayerStats>,
    time: Res<Time>,
) {
    if let Some(m) = boss_model {
        let model = &m.model;

    let (player_tf, player_state, player_timer, player_health, mut player_prev_pos) = if let Ok(p) = player_query.get_single_mut() {
        p
    } else {
        return;
    };

    for (mut boss_tf, mut boss_state, mut boss_timer, boss_health, mut aggression_timer, mut boss_prev_pos, mut boss_memory, mut boss_cooldown) in &mut boss_query {
        // 0. Update Cooldown
        boss_cooldown.timer.tick(time.delta());
        // Calculate Velocity (before early return?)
        // Actually, we should update prev_pos every frame regardless of state?
        // But this system runs every frame.
        // Let's calculate velocity based on current and prev.
        let delta = time.delta_secs();
        let boss_vel = if delta > 0.0 { (boss_tf.translation - boss_prev_pos.0) / delta } else { Vec3::ZERO };
        let player_vel = if delta > 0.0 { (player_tf.translation - player_prev_pos.0) / delta } else { Vec3::ZERO };
        let rel_vel = player_vel - boss_vel;
        
        // Update prev pos for next frame
        boss_prev_pos.0 = boss_tf.translation;
        player_prev_pos.0 = player_tf.translation;

        // 1. ALWAYS Handle Physics (Lunge)
        if *boss_state == CharacterState::Attack {
            let dist_to_player = boss_tf.translation.distance(player_tf.translation);
            
            // ONLY Lunge if we are not already hugging the player (Dist > 1.2)
            if dist_to_player > 1.2 { 
                 if boss_timer.timer.remaining_secs() > (crate::game::combat::ATTACK_DURATION * 0.3) {
                    let dir = (player_tf.translation - boss_tf.translation).normalize_or_zero();
                    boss_tf.translation += dir * 2.0 * time.delta_secs(); 
                }
            }
        }

        // Only act if Idle or Move
        if *boss_state != CharacterState::Idle && *boss_state != CharacterState::Move {
            continue;
        }

        aggression_timer.timer.tick(time.delta());

        // Construct Observation
        // 0: Distance to opponent
        // 1: Angle to opponent (relative to facing)
        // 2: Self Health (0-1)
        // 3: Opponent Health (0-1)
        // 4: Self State (One-hot or Enum)
        // 5: Opponent State
        // 6: Self Action Timer (normalized)
        // 7: Opponent Action Timer (normalized)
        
        let dist = boss_tf.translation.distance(player_tf.translation);
        
        // Real Angle Calculation
        let forward = boss_tf.forward(); // Vec3 (Z-forward)
        let to_player = (player_tf.translation - boss_tf.translation).normalize_or_zero();
        let cos_angle = forward.dot(to_player);
        let sin_angle = forward.cross(to_player).y;
        
        // Map states to float
        let map_state = |s: &CharacterState| -> f32 {
            match s {
                CharacterState::Idle => 0.0,
                CharacterState::Move => 1.0,
                CharacterState::Attack => 2.0,
                CharacterState::Parry => 3.0,
                CharacterState::Dodge => 4.0,
                CharacterState::Stunned => 5.0,
            }
        };

        let obs_vec = vec![
            dist / 10.0, // Normalize by arena size approx
            cos_angle,
            sin_angle,
            rel_vel.x / 10.0,
            rel_vel.z / 10.0,
            boss_health.current / boss_health.max,
            player_health.current / player_health.max,
            map_state(&boss_state) / 5.0,
            map_state(player_state) / 5.0,
            boss_timer.timer.remaining_secs(), // Not normalized perfectly but okay
            player_timer.timer.remaining_secs(),
            boss_memory.last_action as f32 / 7.0,
            boss_cooldown.timer.remaining_secs(), // <--- NEW FEATURE (Index 12)
        ];
        
        // Frame Stacking Logic
        // If history is empty, fill it with current obs * 4
        if boss_memory.history.is_empty() {
            for _ in 0..4 {
                boss_memory.history.extend_from_slice(&obs_vec);
            }
        } else {
            // Remove oldest frame (first 13 elements)
            boss_memory.history.drain(0..13);
            // Add new frame
            boss_memory.history.extend_from_slice(&obs_vec);
        }
        
        // Create tensor from history (should be 13 * 4 = 52 floats)
        let obs_tensor = Tensor::from_slice(&boss_memory.history).unsqueeze(0); // [1, 52]
        
        // Inference
        let action_tensor = model.forward_ts(&[obs_tensor]);
        
        match action_tensor {
            Ok(output) => {
                let mut action = output.int64_value(&[]);
                boss_memory.last_action = action;
                // println!("Boss Action: {}, Dist: {:.2}, State: {:?}", action, dist, boss_state);
                
                // Map Action to Game Logic
                // 0: Wait
                // 1: Move Forward
                // 2: Move Backward
                // 3: Strafe Left
                // 4: Strafe Right
                // 5: Attack
                // 6: Parry
                // 7: Dodge
                
                // Adaptive Override (Heuristic)
                // If player parries a lot (>3 in last 10s), feint (Wait instead of Attack)
                if action == 5 && player_stats.parry_count > 3 {
                    println!("Boss adapts: Feinting due to high parry count!");
                    action = 0; // Force Wait
                    boss_memory.last_action = 0; // Update memory so the AI knows what actually happened
                    *boss_state = CharacterState::Idle; // Feint
                    return;
                }
                
                // If player dodges a lot, maybe wait to catch them?
                if action == 5 && player_stats.dodge_count > 3 {
                     // Delay attack?
                }

                // Heuristic: Force approach if too far
                if dist > 3.0 {
                    action = 1; // Move Forward
                }

                // Heuristic: Force attack if close and aggression timer finished
                // if dist < 2.5 && aggression_timer.timer.finished() {
                //     println!("Boss Aggression: Forcing Attack!");
                //     action = 5; // Attack
                //     aggression_timer.timer.reset();
                // }

                match action {
                    0 => { *boss_state = CharacterState::Idle; },
                    1 => { 
                        // Move Forward
                        let dir = (player_tf.translation - boss_tf.translation).normalize_or_zero();
                        boss_tf.translation += dir * 5.0 * time.delta_secs();
                        *boss_state = CharacterState::Move;
                    },
                    2 => {
                        // Move Backward
                        let dir = (boss_tf.translation - player_tf.translation).normalize_or_zero();
                        boss_tf.translation += dir * 5.0 * time.delta_secs();
                        *boss_state = CharacterState::Move;
                    },
                    3 | 4 => {
                        // Strafe (simplified to just move random side or just wait for now)
                        *boss_state = CharacterState::Move;
                    },
                    5 => {
                        *boss_state = CharacterState::Attack;
                        boss_timer.timer = Timer::from_seconds(crate::game::combat::ATTACK_DURATION, TimerMode::Once);
                        boss_timer.next_state = Some(CharacterState::Idle);
                        // Set Cooldown: Animation + 0.5s Recovery
                        boss_cooldown.timer = Timer::from_seconds(crate::game::combat::ATTACK_DURATION + 0.5, TimerMode::Once);
                    },
                    6 => {
                        *boss_state = CharacterState::Parry;
                        boss_timer.timer = Timer::from_seconds(crate::game::combat::PARRY_DURATION, TimerMode::Once);
                        boss_timer.next_state = Some(CharacterState::Idle);
                    },
                    7 => {
                        *boss_state = CharacterState::Dodge;
                        boss_timer.timer = Timer::from_seconds(crate::game::combat::DODGE_DURATION, TimerMode::Once);
                        boss_timer.next_state = Some(CharacterState::Idle);
                    },
                    _ => {}
                }
            },
            Err(_e) => {
                // eprintln!("Inference error: {:?}", e);
            }
        }
    }
    }
}
