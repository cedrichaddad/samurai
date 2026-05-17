use bevy::prelude::*;
use tch::{CModule, Tensor};
use crate::game::combat::{CharacterState, ActionTimer, FrameWindow, Health, Velocity, ATTACK_DURATION, PARRY_DURATION, DODGE_DURATION, ARENA_RADIUS};
use crate::game::feel::{
    DODGE_IFRAMES, DODGE_RECOVERY, DODGE_STARTUP, LIGHT_ACTIVE, LIGHT_RECOVERY, LIGHT_STARTUP,
    PARRY_PERFECT_FRAMES, PARRY_RECOVERY, PARRY_TOTAL,
};
use crate::game::fusion::{fuse_decision, BossDecisionInput, BossStyle};
use crate::game::instinct::Instinct;
use crate::game::memory::PredictedPlayerWindow;
use crate::game::player::Player;
use crate::game::boss::Boss;
use crate::game::rush::CurrentBossConfig;

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
    mut commands: Commands,
    mut boss_query: Query<(Entity, &Transform, &mut CharacterState, &mut ActionTimer, &mut FrameWindow, &Health, &mut crate::game::aggression::BossAggressionTimer, &mut PreviousPosition, &mut BossMemory, &mut crate::game::boss::BossAttackCooldown, &mut crate::game::boss::BossFeintTimer, &mut Velocity), With<Boss>>,
    mut player_query: Query<(&Transform, &CharacterState, &ActionTimer, &Health, &mut PreviousPosition), (With<Player>, Without<Boss>)>,
    boss_model: Option<Res<BossModel>>,
    player_stats: Res<crate::game::stats::PlayerStats>,
    time: Res<Time>,
    instinct: Option<Res<Instinct>>,
    prediction: Option<Res<PredictedPlayerWindow>>,
    current_cfg: Option<Res<CurrentBossConfig>>,
    db: Option<Res<crate::game::memory::BossMemoryDb>>,
) {
    if let Some(m) = boss_model {
        let model = &m.model;

    let (player_tf, player_state, player_timer, player_health, mut player_prev_pos) = if let Ok(p) = player_query.get_single_mut() {
        p
    } else {
        return;
    };

    for (boss_e, boss_tf, mut boss_state, mut boss_timer, mut boss_window, boss_health, mut aggression_timer, mut boss_prev_pos, mut boss_memory, mut boss_cooldown, mut boss_feint, mut boss_velocity) in &mut boss_query {
        // 0. Update Timers
        boss_cooldown.timer.tick(time.delta());
        boss_feint.timer.tick(time.delta());

        // --- TRAP LOGIC START ---
        if boss_feint.active {
            // TRAP TRIGGER: Player attacked while we were baiting
            if *player_state == CharacterState::Attack {
                println!("*** TRAP SPRUNG: Punishing Player! ***");
                
                // Immediate Parry (Hyper-response)
                *boss_state = CharacterState::Parry;
                boss_timer.timer = Timer::from_seconds(PARRY_DURATION, TimerMode::Once);
                boss_timer.next_state = Some(CharacterState::Idle);
                
                // Reset Trap
                boss_feint.active = false; 
                continue; // Skip inference
            } 
            // TRAP EXPIRED: Player didn't bite after bait duration (using boss_timer which was set to 0.5s)
            else if boss_timer.timer.finished() {
                 boss_feint.active = false;
                 // Return to normal AI next frame
            }
            else {
                // Continue Baiting (Wait)
                // Update physics but skip inference
                // We need to run physics below, so let's NOT continue here, but skip inference block?
                // Or just force action=0?
                // Let's force action=0 and skip inference.
            }
        }
        // --- TRAP LOGIC END ---
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
                 if boss_timer.timer.remaining_secs() > (ATTACK_DURATION * 0.3) {
                    let dir = (player_tf.translation - boss_tf.translation).normalize_or_zero();
                    // boss_tf.translation += dir * 2.0 * time.delta_secs(); 
                    boss_velocity.0 = dir * 2.0;
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
        
        // Map states to float. Block reads as a defensive state (same bucket
        // as Parry) for the policy — keeps the existing model usable without
        // retraining.
        let map_state = |s: &CharacterState| -> f32 {
            match s {
                CharacterState::Idle => 0.0,
                CharacterState::Move => 1.0,
                CharacterState::Attack => 2.0,
                CharacterState::Parry | CharacterState::Block => 3.0,
                CharacterState::Dodge => 4.0,
                CharacterState::Stunned => 5.0,
            }
        };

        let obs_vec = vec![
            dist / ARENA_RADIUS, // Normalize by arena size approx
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
        // Skip inference if trapping
        if boss_feint.active {
             // Just wait (physics will run below/above)
             // Actually physics ran above.
             // We just need to skip the rest.
             continue;
        }

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
                
                // === Decision fusion: combine TorchScript policy with vibrato
                // dossier prediction and in-match instinct, gated by reaction
                // delay and a mistake budget. Boss-style is per-stage. ===
                let style = current_cfg
                    .as_ref()
                    .and_then(|c| c.0.as_ref().map(|b| b.style))
                    .unwrap_or(BossStyle::None);
                let reaction_delay_ticks = current_cfg
                    .as_ref()
                    .and_then(|c| c.0.as_ref().map(|b| b.reaction_delay_ticks))
                    .unwrap_or(0);
                let mistake_rate = current_cfg
                    .as_ref()
                    .and_then(|c| c.0.as_ref().map(|b| b.mistake_rate))
                    .unwrap_or(0.0);
                let current_tick = db
                    .as_ref()
                    .map(|d| d.tick)
                    .unwrap_or(0);
                // Mimic uses the matched-window's first-future-action as its
                // hint when available (cross-match) and falls back to the
                // instinct match's last_action (in-match) otherwise.
                let mimic_hint = if matches!(style, BossStyle::Mimic) {
                    prediction
                        .as_ref()
                        .and_then(|p| p.futures.first().and_then(|f| f.first().copied()))
                } else {
                    None
                };
                let instinct_match = instinct.as_ref().and_then(|i| i.nearest());
                let prediction_ref = prediction.as_deref();
                let fused = fuse_decision(BossDecisionInput {
                    policy_action: action as u8,
                    style,
                    instinct: instinct_match,
                    prediction: prediction_ref,
                    current_tick,
                    reaction_delay_ticks,
                    mistake_rate,
                    mimic_action_hint: mimic_hint,
                });
                action = fused.action as i64;

                // Trap-setup safety: if Pattern-Breaker / Counter-Sage want to
                // Wait while baiting parry-happy players, prime the existing
                // feint timer so the trap-trigger snap-parry stays available.
                let player_parry_pressure = player_stats.parry_count > 4;
                if fused.reason == "counter-sage"
                    && player_parry_pressure
                    && action == 0
                    && boss_feint.timer.finished()
                    && !boss_feint.active
                {
                    boss_feint.active = true;
                    boss_feint.timer.reset();
                    boss_timer.timer = Timer::from_seconds(0.5, TimerMode::Once);
                    boss_memory.last_action = 0;
                    *boss_state = CharacterState::Idle;
                    return;
                }

                // Final safety: if we're far enough away that no reaction
                // matters, just close. This stays from the original baseline.
                if action == 0 && dist > 3.5 {
                    action = 1;
                }

                match action {
                    0 => { *boss_state = CharacterState::Idle; },
                    1 => {
                        let dist = boss_tf.translation.distance(player_tf.translation);
                        if dist > 1.5 {
                            let dir = (player_tf.translation - boss_tf.translation).normalize_or_zero();
                            boss_velocity.0 = dir * 5.0;
                            *boss_state = CharacterState::Move;
                        } else {
                            *boss_state = CharacterState::Idle;
                        }
                    },
                    2 => {
                        let dir = (boss_tf.translation - player_tf.translation).normalize_or_zero();
                        boss_velocity.0 = dir * 5.0;
                        *boss_state = CharacterState::Move;
                    },
                    3 | 4 => {
                        *boss_state = CharacterState::Move;
                    },
                    5 => {
                        *boss_state = CharacterState::Attack;
                        *boss_window = FrameWindow::new(
                            LIGHT_STARTUP,
                            LIGHT_ACTIVE,
                            LIGHT_RECOVERY,
                        );
                        boss_timer.timer =
                            Timer::from_seconds(boss_window.total_secs(), TimerMode::Once);
                        boss_timer.next_state = Some(CharacterState::Idle);
                        boss_cooldown.timer = Timer::from_seconds(
                            boss_window.total_secs() + 0.4,
                            TimerMode::Once,
                        );
                        // Roll for an unblockable telegraph. Probability comes
                        // from the boss roster (rises 0 → 0.20 across the 5
                        // stages). Telegraph runs the full attack window so
                        // the player has time to read it and dodge.
                        let unblockable_chance = current_cfg
                            .as_ref()
                            .and_then(|c| c.0.as_ref().map(|b| b.unblockable_chance))
                            .unwrap_or(0.0);
                        if unblockable_chance > 0.0 {
                            // Cheap deterministic [0,1) keyed on (tick, entity).
                            let mut x = current_tick
                                .wrapping_add(boss_e.to_bits() as u64)
                                .wrapping_mul(0x9E37_79B9_7F4A_7C15);
                            x ^= x >> 33;
                            x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
                            x ^= x >> 33;
                            let r = ((x >> 40) as f32) / ((1u64 << 24) as f32);
                            if r < unblockable_chance {
                                commands.entity(boss_e).insert(
                                    crate::game::vfx::Unblockable {
                                        remaining_s: boss_window.total_secs() + 0.05,
                                    },
                                );
                            }
                        }
                    },
                    6 => {
                        *boss_state = CharacterState::Parry;
                        *boss_window = FrameWindow::new(0, PARRY_TOTAL, PARRY_RECOVERY)
                            .with_perfect(PARRY_PERFECT_FRAMES);
                        boss_timer.timer =
                            Timer::from_seconds(PARRY_DURATION, TimerMode::Once);
                        boss_timer.next_state = Some(CharacterState::Idle);
                    },
                    7 => {
                        *boss_state = CharacterState::Dodge;
                        *boss_window = FrameWindow::new(
                            DODGE_STARTUP,
                            DODGE_IFRAMES,
                            DODGE_RECOVERY,
                        );
                        boss_timer.timer =
                            Timer::from_seconds(DODGE_DURATION, TimerMode::Once);
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
