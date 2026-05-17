//! Decision fusion: combine the TorchScript policy output, the in-match
//! instinct ring, the cross-match `identify` prediction, and a boss-style
//! fairness budget into a single action.
//!
//! Action space matches the existing one in `ai.rs`:
//!   0 = Wait, 1 = Forward, 2 = Backward, 3 = Strafe L, 4 = Strafe R,
//!   5 = Attack, 6 = Parry, 7 = Dodge

use crate::game::instinct::InstinctMatch;
use crate::game::memory::PredictedPlayerWindow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BossStyle {
    None,
    Mimic,
    CounterSage,
    PatternBreaker,
    MemoryEater,
}

impl BossStyle {
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "mimic" => Self::Mimic,
            "counter_sage" | "counter-sage" => Self::CounterSage,
            "pattern_breaker" | "pattern-breaker" => Self::PatternBreaker,
            "memory_eater" | "memory-eater" => Self::MemoryEater,
            _ => Self::None,
        }
    }
}

/// Player dominant action enum, as written into the action sidecar by
/// `BossMemoryDb::push_tick`.
const PLAYER_IDLE: u8 = 0;
const PLAYER_MOVE: u8 = 1;
const PLAYER_ATTACK: u8 = 2;
const PLAYER_PARRY: u8 = 3;
const PLAYER_DODGE: u8 = 4;

pub const ACTION_WAIT: u8 = 0;
pub const ACTION_FORWARD: u8 = 1;
pub const ACTION_BACKWARD: u8 = 2;
#[allow(dead_code)]
pub const ACTION_STRAFE_L: u8 = 3;
#[allow(dead_code)]
pub const ACTION_STRAFE_R: u8 = 4;
pub const ACTION_ATTACK: u8 = 5;
pub const ACTION_PARRY: u8 = 6;
pub const ACTION_DODGE: u8 = 7;

#[derive(Clone, Debug)]
pub struct BossDecisionInput<'a> {
    pub policy_action: u8,
    pub style: BossStyle,
    pub instinct: Option<InstinctMatch>,
    pub prediction: Option<&'a PredictedPlayerWindow>,
    pub current_tick: u64,
    pub reaction_delay_ticks: u64,
    pub mistake_rate: f32,
    pub mimic_action_hint: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BossDecisionOutput {
    pub action: u8,
    pub reason: &'static str,
}

pub fn fuse_decision(input: BossDecisionInput) -> BossDecisionOutput {
    // Step 1: pick a candidate action based on style.
    let mut candidate = match input.style {
        BossStyle::None => BossDecisionOutput {
            action: input.policy_action,
            reason: "policy",
        },
        BossStyle::Mimic => mimic_action(&input),
        BossStyle::CounterSage => counter_sage_action(&input),
        BossStyle::PatternBreaker => pattern_breaker_action(&input),
        BossStyle::MemoryEater => memory_eater_action(&input),
    };

    // Step 2: enforce the reaction-delay budget. If the prediction was
    // observed too recently, fall back to the policy action.
    if let Some(pred) = input.prediction {
        if input.current_tick.saturating_sub(pred.tick_observed) < input.reaction_delay_ticks
            && candidate.reason != "policy"
        {
            candidate = BossDecisionOutput {
                action: input.policy_action,
                reason: "reaction-budget",
            };
        }
    }

    // Step 3: sometimes inject a mistake to keep the boss feeling human.
    if input.mistake_rate > 0.0 {
        let r = pseudo_random(input.current_tick);
        if r < input.mistake_rate {
            // Cycle by 1 and clamp into action space.
            candidate.action = (candidate.action + 1) % 8;
            candidate.reason = "mistake";
        }
    }

    candidate
}

fn mimic_action(input: &BossDecisionInput) -> BossDecisionOutput {
    if let Some(hint) = input.mimic_action_hint {
        let action = match hint {
            PLAYER_IDLE => ACTION_WAIT,
            PLAYER_MOVE => ACTION_FORWARD,
            PLAYER_ATTACK => ACTION_ATTACK,
            PLAYER_PARRY => ACTION_PARRY,
            PLAYER_DODGE => ACTION_DODGE,
            _ => input.policy_action,
        };
        return BossDecisionOutput {
            action,
            reason: "mimic-future",
        };
    }
    if let Some(m) = input.instinct {
        let action = match m.last_action {
            5 => ACTION_ATTACK,
            6 => ACTION_PARRY,
            7 => ACTION_DODGE,
            1 => ACTION_FORWARD,
            2 => ACTION_BACKWARD,
            _ => input.policy_action,
        };
        return BossDecisionOutput {
            action,
            reason: "mimic-instinct",
        };
    }
    BossDecisionOutput {
        action: input.policy_action,
        reason: "mimic-fallback",
    }
}

fn counter_sage_action(input: &BossDecisionInput) -> BossDecisionOutput {
    let predicted = predicted_first_action(input).unwrap_or(PLAYER_IDLE);
    let action = match predicted {
        PLAYER_PARRY => {
            // Player will parry → don't swing. Wait/feint.
            ACTION_WAIT
        }
        PLAYER_ATTACK => {
            // Player will attack → parry.
            ACTION_PARRY
        }
        PLAYER_DODGE => {
            // Player will dodge → wait, then close.
            ACTION_WAIT
        }
        PLAYER_IDLE => {
            // Player idling → press the offense.
            ACTION_ATTACK
        }
        _ => input.policy_action,
    };
    BossDecisionOutput {
        action,
        reason: "counter-sage",
    }
}

fn pattern_breaker_action(input: &BossDecisionInput) -> BossDecisionOutput {
    let Some(pred) = input.prediction else {
        return BossDecisionOutput {
            action: input.policy_action,
            reason: "pattern-breaker-no-pred",
        };
    };
    let top_habit = top_habit_player_action(&pred.habit_histogram);
    let policy_targets_top = matches!(
        (input.policy_action, top_habit),
        (ACTION_ATTACK, PLAYER_PARRY)
            | (ACTION_PARRY, PLAYER_ATTACK)
            | (ACTION_DODGE, PLAYER_ATTACK)
            | (ACTION_WAIT, PLAYER_IDLE)
    );
    if !policy_targets_top {
        return BossDecisionOutput {
            action: input.policy_action,
            reason: "pattern-breaker-keep",
        };
    }
    // Refuse to engage on the player's favored matchup. Pick the alternative.
    let alt = match top_habit {
        PLAYER_ATTACK => ACTION_BACKWARD,
        PLAYER_PARRY => ACTION_WAIT,
        PLAYER_DODGE => ACTION_FORWARD,
        PLAYER_MOVE => ACTION_WAIT,
        _ => ACTION_WAIT,
    };
    BossDecisionOutput {
        action: alt,
        reason: "pattern-breaker-deny",
    }
}

fn memory_eater_action(input: &BossDecisionInput) -> BossDecisionOutput {
    // Memory-Eater alternates between counter-sage and pattern-breaker every
    // 4 seconds (240 ticks). Layered with a higher mistake budget through the
    // BossDecisionInput, this reads as deliberate yet imperfect.
    let phase = (input.current_tick / 240) & 1;
    let chosen = if phase == 0 {
        counter_sage_action(input)
    } else {
        pattern_breaker_action(input)
    };
    BossDecisionOutput {
        action: chosen.action,
        reason: "memory-eater",
    }
}

fn predicted_first_action(input: &BossDecisionInput) -> Option<u8> {
    let pred = input.prediction?;
    pred.futures.first().and_then(|f| f.first().copied())
}

fn top_habit_player_action(hist: &[u32; 5]) -> u8 {
    let mut best_idx = 0u8;
    let mut best_count = 0u32;
    for (i, &c) in hist.iter().enumerate() {
        if c > best_count {
            best_count = c;
            best_idx = i as u8;
        }
    }
    best_idx
}

/// Cheap deterministic pseudo-random in [0,1) keyed on a tick. We don't need
/// crypto strength here, just enough variation to break repetitive losses.
fn pseudo_random(tick: u64) -> f32 {
    let mut x = tick.wrapping_mul(2_654_435_761).wrapping_add(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^= x >> 33;
    let f = ((x >> 40) as f32) / ((1u64 << 24) as f32);
    f.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::memory::PredictedPlayerWindow;

    fn pred_with_first_future_action(action: u8) -> PredictedPlayerWindow {
        let mut p = PredictedPlayerWindow::default();
        p.futures.push(vec![action; 16]);
        let mut hist = [0u32; 5];
        hist[action as usize] += 16;
        p.habit_histogram = hist;
        p.tick_observed = 0;
        p
    }

    #[test]
    fn counter_sage_parries_predicted_attack() {
        let pred = pred_with_first_future_action(PLAYER_ATTACK);
        let out = fuse_decision(BossDecisionInput {
            policy_action: ACTION_FORWARD,
            style: BossStyle::CounterSage,
            instinct: None,
            prediction: Some(&pred),
            current_tick: 1000,
            reaction_delay_ticks: 0,
            mistake_rate: 0.0,
            mimic_action_hint: None,
        });
        assert_eq!(out.action, ACTION_PARRY);
    }

    #[test]
    fn counter_sage_waits_on_predicted_parry() {
        let pred = pred_with_first_future_action(PLAYER_PARRY);
        let out = fuse_decision(BossDecisionInput {
            policy_action: ACTION_ATTACK,
            style: BossStyle::CounterSage,
            instinct: None,
            prediction: Some(&pred),
            current_tick: 1000,
            reaction_delay_ticks: 0,
            mistake_rate: 0.0,
            mimic_action_hint: None,
        });
        assert_eq!(out.action, ACTION_WAIT);
    }

    #[test]
    fn pattern_breaker_refuses_attack_when_player_loves_parrying() {
        let pred = pred_with_first_future_action(PLAYER_PARRY);
        let out = fuse_decision(BossDecisionInput {
            policy_action: ACTION_ATTACK,
            style: BossStyle::PatternBreaker,
            instinct: None,
            prediction: Some(&pred),
            current_tick: 1000,
            reaction_delay_ticks: 0,
            mistake_rate: 0.0,
            mimic_action_hint: None,
        });
        assert_ne!(out.action, ACTION_ATTACK);
    }

    #[test]
    fn reaction_budget_blocks_premature_response() {
        let mut pred = pred_with_first_future_action(PLAYER_ATTACK);
        pred.tick_observed = 1000;
        let out = fuse_decision(BossDecisionInput {
            policy_action: ACTION_FORWARD,
            style: BossStyle::CounterSage,
            instinct: None,
            prediction: Some(&pred),
            current_tick: 1005, // only 5 ticks since observation
            reaction_delay_ticks: 12,
            mistake_rate: 0.0,
            mimic_action_hint: None,
        });
        assert_eq!(out.action, ACTION_FORWARD);
        assert_eq!(out.reason, "reaction-budget");
    }

    #[test]
    fn no_style_passes_policy_through() {
        let out = fuse_decision(BossDecisionInput {
            policy_action: ACTION_ATTACK,
            style: BossStyle::None,
            instinct: None,
            prediction: None,
            current_tick: 1,
            reaction_delay_ticks: 0,
            mistake_rate: 0.0,
            mimic_action_hint: None,
        });
        assert_eq!(out.action, ACTION_ATTACK);
        assert_eq!(out.reason, "policy");
    }

    #[test]
    fn mimic_uses_future_hint_when_present() {
        let out = fuse_decision(BossDecisionInput {
            policy_action: ACTION_WAIT,
            style: BossStyle::Mimic,
            instinct: None,
            prediction: None,
            current_tick: 1,
            reaction_delay_ticks: 0,
            mistake_rate: 0.0,
            mimic_action_hint: Some(PLAYER_DODGE),
        });
        assert_eq!(out.action, ACTION_DODGE);
    }
}
