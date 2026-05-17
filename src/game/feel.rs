//! Single source of truth for game-feel tuning. Every magic number lives here.
//! When something feels off, this is the file you edit.
//!
//! Units: ticks are at the 60 Hz `FixedUpdate` cadence. ms / s are wall-clock.

// ─── Phase 1: foundations ──────────────────────────────────────────────────

/// Action presses are remembered for this many ticks; press-and-act windows
/// extend that long after the press.
pub const INPUT_BUFFER_TICKS: u64 = 12; // ~200 ms

/// Hit-stop pause durations on different events. While stopped, combat / AI /
/// movement systems are gated; camera + VFX continue to tick.
pub const HIT_STOP_HIT: u64 = 5; // ~83 ms
pub const HIT_STOP_BLOCK: u64 = 3; // 50 ms — softer than a clean hit
pub const HIT_STOP_PARRY: u64 = 6; // 100 ms
pub const HIT_STOP_PERFECT_PARRY: u64 = 8; // 133 ms
pub const HIT_STOP_KILL: u64 = 10; // 167 ms

/// Camera shake trauma values per event. Visual shake = `trauma²`, so 0.4 →
/// 0.16, which is enough to feel without losing readability.
pub const TRAUMA_HIT: f32 = 0.40;
pub const TRAUMA_BLOCK: f32 = 0.20;
pub const TRAUMA_PARRY: f32 = 0.55;
pub const TRAUMA_PERFECT_PARRY: f32 = 0.70;
pub const TRAUMA_KILL: f32 = 0.85;
pub const TRAUMA_DECAY_PER_SEC: f32 = 1.5;

/// Camera follow strength. `lerp(target, 1 - exp(-k * dt))` is frame-rate
/// independent.
pub const CAMERA_FOLLOW_K: f32 = 8.0;
/// FOV adjustments on combat events.
pub const CAMERA_FOV_HIT_DELTA: f32 = -2.0;
pub const CAMERA_FOV_PARRY_DELTA: f32 = -5.0;
#[allow(dead_code)]
pub const CAMERA_FOV_DODGE_DELTA: f32 = 3.0;
pub const CAMERA_FOV_TWEEN_S: f32 = 0.4;
pub const CAMERA_FOV_HIT_TWEEN_S: f32 = 0.18;

/// Movement: exponential damping (per second). 0.92 means roughly half-life of
/// ~3 frames at 60 Hz. v2: stronger lunge for crisper attack steps.
pub const VELOCITY_DAMPING: f32 = 0.92;
pub const ROTATION_K: f32 = 12.0;
pub const ATTACK_LUNGE_FORCE: f32 = 11.0;
/// Auto-aim slerp during attack startup; higher k = harder snap. Range cap
/// keeps faraway targets from yanking the camera around.
pub const AUTO_SNAP_K: f32 = 40.0;
pub const AUTO_SNAP_RANGE: f32 = 5.5;

/// Animation blend lengths. v2: combat blends 35→25 ms — crisper transitions.
pub const COMBAT_BLEND_MS: u64 = 25;
pub const NAV_BLEND_MS: u64 = 100;
pub const STUN_BLEND_MS: u64 = 50;

// ─── Phase 2: combat depth ─────────────────────────────────────────────────

/// Frame data per action (in 60 Hz ticks). Sum equals total animation length.
pub const LIGHT_STARTUP: u64 = 6;
pub const LIGHT_ACTIVE: u64 = 4;
pub const LIGHT_RECOVERY: u64 = 14;

pub const HEAVY_STARTUP: u64 = 14;
pub const HEAVY_ACTIVE: u64 = 5;
pub const HEAVY_RECOVERY: u64 = 20;

pub const PARRY_TOTAL: u64 = 12;
pub const PARRY_PERFECT_FRAMES: u64 = 5;
pub const PARRY_RECOVERY: u64 = 8;

pub const DODGE_STARTUP: u64 = 3;
pub const DODGE_IFRAMES: u64 = 9;
pub const DODGE_RECOVERY: u64 = 6;

/// Combo: chained attacks shorten recovery and feel snappier.
pub const COMBO_CANCEL_TAIL_FRAMES: u64 = 6;
pub const COMBO_CHAIN_TIMEOUT_S: f32 = 0.6;
pub const COMBO_CHAIN_MAX: u8 = 3;

// ─── Posture ────────────────────────────────────────────────────────────────

pub const POSTURE_DECAY_PER_SEC: f32 = 25.0;
pub const POSTURE_DECAY_GRACE_S: f32 = 1.0;
pub const POSTURE_BREAK_WINDOW_S: f32 = 1.5;
pub const POSTURE_GAIN_LANDED_HIT: f32 = 8.0;
pub const POSTURE_GAIN_PARRIED_BY_OPP: f32 = 12.0;
pub const POSTURE_GAIN_PERFECT_PARRIED_BY_OPP: f32 = 18.0;
pub const POSTURE_GAIN_BLOCKED: f32 = 12.0;
pub const POSTURE_GAIN_VICTIM_HIT: f32 = 6.0;

/// Stun durations on parry. Perfect parries punish harder.
pub const PARRY_LATE_STUN_S: f32 = 0.6;
pub const PARRY_PERFECT_STUN_S: f32 = 1.2;

/// Block: chip-damage fraction taken when blocking, plus pushback magnitudes.
pub const BLOCK_CHIP_FRACTION: f32 = 0.30;
#[allow(dead_code)]
pub const BLOCK_HOLD_THRESHOLD_S: f32 = 0.18; // hold Q this long after parry to lock into block

/// Hit pushback. Victim recoils away; attacker steps in slightly.
pub const PUSHBACK_VICTIM: f32 = 6.5;
pub const PUSHBACK_ATTACKER: f32 = 1.5;
pub const PUSHBACK_BLOCK_VICTIM: f32 = 2.5;
pub const PUSHBACK_BLOCK_ATTACKER: f32 = 0.5;
pub const PUSHBACK_PARRY_ATTACKER: f32 = 3.0;

/// Execute prompt range when posture is broken.
pub const EXECUTE_RANGE: f32 = 2.6;

// ─── Phase 6: spectacle ────────────────────────────────────────────────────

pub const KILLCAM_TIME_SCALE: f32 = 0.20;
pub const KILLCAM_DURATION_S: f32 = 0.8;
pub const STAGE_CLEAR_SLOWMO_S: f32 = 0.8;
pub const PLAYER_DEATH_FADE_S: f32 = 1.5;
pub const BOSS_INTRO_S: f32 = 3.0;

// ─── Helpers ───────────────────────────────────────────────────────────────

#[allow(dead_code)]
#[inline]
pub fn ms_to_ticks(ms: u64) -> u64 {
    (ms * 60) / 1000
}

#[allow(dead_code)]
#[inline]
pub fn ticks_to_secs(ticks: u64) -> f32 {
    (ticks as f32) / 60.0
}
