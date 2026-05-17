//! Bevy systems gluing the player observation pipeline to `BossMemoryDb`,
//! `Instinct`, and `PredictedPlayerWindow`.

use crate::game::boss::Boss;
use crate::game::combat::{ARENA_RADIUS, CharacterState, Health};
use crate::game::instinct::Instinct;
use crate::game::memory::{BossMemoryDb, DominantAction, PredictedPlayerWindow, TickSample};
use crate::game::player::Player;
use bevy::prelude::*;

/// Sampling cadence for `identify`: approximately once per second at 60 Hz.
const IDENTIFY_TICK_INTERVAL: u64 = 60;

/// Per FixedUpdate tick: build the player TickSample, feed both the cross-match
/// `BossMemoryDb` ring and the in-match `Instinct` buffer.
pub fn ingest_player_memory_system(
    mut db: ResMut<BossMemoryDb>,
    mut instinct: ResMut<Instinct>,
    boss_query: Query<&Transform, (With<Boss>, Without<Player>)>,
    player_query: Query<(&Transform, &CharacterState, &Health), With<Player>>,
) {
    let Ok((player_tf, player_state, _health)) = player_query.get_single() else {
        return;
    };
    let Ok(boss_tf) = boss_query.get_single() else {
        return;
    };

    let dist = boss_tf.translation.distance(player_tf.translation);
    let dist_norm = (dist / ARENA_RADIUS).clamp(0.0, 1.0);

    // Angle of the player relative to the boss's forward axis.
    let forward = boss_tf.forward();
    let to_player = (player_tf.translation - boss_tf.translation).normalize_or_zero();
    let cos_angle = forward.dot(to_player).clamp(-1.0, 1.0);
    let sin_angle = forward.cross(to_player).y.clamp(-1.0, 1.0);
    let rel_angle = sin_angle.atan2(cos_angle) / std::f32::consts::PI;

    let action = match player_state {
        CharacterState::Attack => DominantAction::Attack,
        // Block collapses to Parry in the dossier — both are "defensive
        // stance" from the boss's perspective. Mimic of a blocky player will
        // therefore parry, which is fine.
        CharacterState::Parry | CharacterState::Block => DominantAction::Parry,
        CharacterState::Dodge => DominantAction::Dodge,
        CharacterState::Move => DominantAction::Move,
        _ => DominantAction::Idle,
    };

    let player_state_id = match player_state {
        CharacterState::Idle => 0.0,
        CharacterState::Move => 1.0,
        CharacterState::Attack => 2.0,
        CharacterState::Parry | CharacterState::Block => 3.0,
        CharacterState::Dodge => 4.0,
        CharacterState::Stunned => 5.0,
    } / 5.0;

    let sample = TickSample {
        dist_norm,
        rel_angle,
        player_state: player_state_id,
        action_signal: action.signal(),
    };

    db.push_tick(sample, action);
    instinct.push_tick(sample, action as u8);
}

/// Once per ~1 s, run identify against the loaded dossier and refresh the
/// `PredictedPlayerWindow` resource consumed by `boss_ai_system` /
/// `fusion::fuse_decision`.
pub fn identify_player_pattern_system(
    db: Res<BossMemoryDb>,
    mut prediction: ResMut<PredictedPlayerWindow>,
) {
    if db.tick == 0 || db.tick % IDENTIFY_TICK_INTERVAL != 0 {
        return;
    }
    let results = db.identify();
    let mut futures: Vec<Vec<u8>> = Vec::with_capacity(results.len());
    let mut hist = [0u32; 5];
    let mut total_score = 0.0f32;
    for r in &results {
        let mut future_actions: Vec<u8> = Vec::new();
        // Each future row carries 16 ticks of action data. Concatenate them.
        for row_id in r.future_start_id..=r.future_end_id {
            if let Some(actions) = db.sidecar_action_for(row_id) {
                future_actions.extend(actions.iter().copied());
                for &a in &actions {
                    if let Some(slot) = hist.get_mut(a as usize) {
                        *slot += 1;
                    }
                }
            }
        }
        if !future_actions.is_empty() {
            futures.push(future_actions);
        }
        total_score += r.score;
    }
    let avg_score = if results.is_empty() {
        0.0
    } else {
        total_score / results.len() as f32
    };

    prediction.results = results;
    prediction.futures = futures;
    prediction.habit_histogram = hist;
    prediction.tick_observed = db.tick;
    prediction.last_score = avg_score;
}
