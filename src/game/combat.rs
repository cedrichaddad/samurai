use bevy::prelude::*;

use crate::game::feel::{
    BLOCK_CHIP_FRACTION, PARRY_LATE_STUN_S, PARRY_PERFECT_STUN_S, POSTURE_GAIN_BLOCKED,
    POSTURE_GAIN_LANDED_HIT, POSTURE_GAIN_PARRIED_BY_OPP, POSTURE_GAIN_PERFECT_PARRIED_BY_OPP,
    POSTURE_GAIN_VICTIM_HIT, PUSHBACK_ATTACKER, PUSHBACK_BLOCK_ATTACKER, PUSHBACK_BLOCK_VICTIM,
    PUSHBACK_PARRY_ATTACKER, PUSHBACK_VICTIM,
};
use crate::game::hitstop::{HitEvent, HitKind};
use crate::game::posture::Posture;

pub const ATTACK_DURATION: f32 = 0.5;
pub const PARRY_DURATION: f32 = 0.2;
pub const DODGE_DURATION: f32 = 0.3;
pub const STUN_DURATION: f32 = 0.3;
pub const ARENA_RADIUS: f32 = 10.0;

#[derive(Component, Default, Debug)]
pub struct Velocity(pub Vec3);

pub fn apply_velocity(time: Res<Time>, mut query: Query<(&mut Transform, &mut Velocity)>) {
    let dt = time.delta_secs();
    let damping = (-crate::game::feel::VELOCITY_DAMPING * 60.0 * dt).exp();
    for (mut transform, mut velocity) in &mut query {
        if velocity.0.length_squared() > 0.0001 {
            transform.translation += velocity.0 * dt;
            velocity.0 *= damping;
            if velocity.0.length_squared() < 0.0001 {
                velocity.0 = Vec3::ZERO;
            }
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CharacterState {
    #[default]
    Idle,
    Move,
    Attack,
    Parry,
    /// Held defensive stance. Entered by holding Q past the parry window.
    /// Reduces incoming damage by `BLOCK_CHIP_FRACTION` but feeds posture.
    Block,
    Dodge,
    Stunned,
}

#[derive(Component, Default)]
pub struct ActionTimer {
    pub timer: Timer,
    pub next_state: Option<CharacterState>,
}

/// Frame-window data for combat actions. `elapsed` ticks every fixed update
/// while the entity is in a non-Idle/Move state; the active window is the
/// only time hitboxes connect, and i-frames live inside the active window of
/// Dodge.
#[derive(Component, Debug, Clone, Copy)]
pub struct FrameWindow {
    pub startup: u64,
    pub active: u64,
    pub recovery: u64,
    pub elapsed: u64,
    /// Set by parry; the first `perfect` frames of `active` count as a
    /// perfect parry. Zero on non-parry actions.
    pub perfect: u64,
}

impl Default for FrameWindow {
    fn default() -> Self {
        Self::new(0, 0, 0)
    }
}

impl FrameWindow {
    pub fn new(startup: u64, active: u64, recovery: u64) -> Self {
        Self {
            startup,
            active,
            recovery,
            elapsed: 0,
            perfect: 0,
        }
    }
    pub fn with_perfect(mut self, perfect: u64) -> Self {
        self.perfect = perfect;
        self
    }
    pub fn total(&self) -> u64 {
        self.startup + self.active + self.recovery
    }
    pub fn total_secs(&self) -> f32 {
        self.total() as f32 / 60.0
    }
    pub fn remaining(&self) -> u64 {
        self.total().saturating_sub(self.elapsed)
    }
    pub fn in_startup(&self) -> bool {
        self.elapsed < self.startup
    }
    pub fn in_active(&self) -> bool {
        self.elapsed >= self.startup && self.elapsed < self.startup + self.active
    }
    pub fn in_perfect(&self) -> bool {
        self.elapsed >= self.startup && self.elapsed < self.startup + self.perfect
    }
    pub fn in_recovery(&self) -> bool {
        self.elapsed >= self.startup + self.active
    }
}

pub fn tick_frame_windows_system(mut q: Query<(&CharacterState, &mut FrameWindow)>) {
    for (state, mut win) in &mut q {
        let combat_active = matches!(
            state,
            CharacterState::Attack | CharacterState::Parry | CharacterState::Dodge
        );
        if combat_active {
            if win.elapsed < win.total() {
                win.elapsed += 1;
            }
        } else {
            win.elapsed = 0;
        }
    }
}

pub fn update_action_timers(
    time: Res<Time>,
    mut query: Query<(Entity, &mut CharacterState, &mut ActionTimer)>,
) {
    for (_entity, mut state, mut action_timer) in &mut query {
        // Block has no timed expiration — it's a held state.
        if matches!(*state, CharacterState::Block) {
            continue;
        }
        action_timer.timer.tick(time.delta());
        if action_timer.timer.finished() {
            if let Some(next) = action_timer.next_state {
                *state = next;
            } else {
                *state = CharacterState::Idle;
            }
            action_timer.timer.reset();
            action_timer.timer.pause();
        }
    }
}

#[derive(Component)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            current: 100.0,
            max: 100.0,
        }
    }
}

#[derive(Component)]
pub struct Hitbox {
    pub radius: f32,
    pub offset: Vec3,
    pub active: bool,
    pub has_hit: bool,
    pub damage: f32,
}

impl Default for Hitbox {
    fn default() -> Self {
        Self {
            radius: 1.0,
            offset: Vec3::new(0.0, 1.0, 1.0),
            active: false,
            has_hit: false,
            damage: 10.0,
        }
    }
}

#[derive(Component)]
pub struct Hurtbox {
    pub radius: f32,
    pub offset: Vec3,
}

impl Default for Hurtbox {
    fn default() -> Self {
        Self {
            radius: 0.5,
            offset: Vec3::new(0.0, 1.0, 0.0),
        }
    }
}

/// Single-query collision pass using `iter_combinations_mut`. Each pair is
/// checked both ways (a→b and b→a) so any combatant can attack any other.
///
/// This replaces the previous two-query design (attacker_query + victim_query
/// both with `Option<&mut Posture>`) which conflicted at startup whenever the
/// same entity matched both queries — i.e., always, for our two-character
/// arenas.
pub fn combat_collision(
    mut commands: Commands,
    mut hit_events: EventWriter<HitEvent>,
    mut combatants: Query<(
        Entity,
        &GlobalTransform,
        &mut Hitbox,
        &Hurtbox,
        &mut Health,
        &CharacterState,
        &FrameWindow,
        &mut Velocity,
        &mut Posture,
        Option<&crate::game::vfx::Unblockable>,
    )>,
) {
    let mut combos = combatants.iter_combinations_mut();
    while let Some([a, b]) = combos.fetch_next() {
        let (
            a_e, a_gt, mut a_hit, a_hurt, mut a_hp, a_state, a_win, mut a_vel, mut a_post, a_unblock,
        ) = a;
        let (
            b_e, b_gt, mut b_hit, b_hurt, mut b_hp, b_state, b_win, mut b_vel, mut b_post, b_unblock,
        ) = b;

        try_resolve_pair_hit(
            &mut commands,
            &mut hit_events,
            a_e,
            &a_gt,
            &mut *a_hit,
            a_state,
            a_win,
            a_unblock.is_some(),
            &mut *a_vel,
            &mut *a_post,
            b_e,
            &b_gt,
            &b_hurt,
            &mut *b_hp,
            b_state,
            b_win,
            &mut *b_vel,
            &mut *b_post,
        );
        try_resolve_pair_hit(
            &mut commands,
            &mut hit_events,
            b_e,
            &b_gt,
            &mut *b_hit,
            b_state,
            b_win,
            b_unblock.is_some(),
            &mut *b_vel,
            &mut *b_post,
            a_e,
            &a_gt,
            &a_hurt,
            &mut *a_hp,
            a_state,
            a_win,
            &mut *a_vel,
            &mut *a_post,
        );
    }

    // After collision pairs, also reset any inactive attacker hitboxes to keep
    // them in sync with the FrameWindow.
    for (_, _, mut hit, _, _, state, win, _, _, _) in &mut combatants {
        let live = matches!(state, CharacterState::Attack) && win.in_active();
        if !live {
            hit.active = false;
            if !matches!(state, CharacterState::Attack) {
                hit.has_hit = false;
            }
        }
    }
}

fn try_resolve_pair_hit(
    commands: &mut Commands,
    hit_events: &mut EventWriter<HitEvent>,
    attacker_e: Entity,
    attacker_gt: &GlobalTransform,
    attacker_hit: &mut Hitbox,
    attacker_state: &CharacterState,
    attacker_win: &FrameWindow,
    attacker_unblockable: bool,
    attacker_vel: &mut Velocity,
    attacker_post: &mut Posture,
    victim_e: Entity,
    victim_gt: &GlobalTransform,
    victim_hurt: &Hurtbox,
    victim_hp: &mut Health,
    victim_state: &CharacterState,
    victim_win: &FrameWindow,
    victim_vel: &mut Velocity,
    victim_post: &mut Posture,
) {
    if !matches!(attacker_state, CharacterState::Attack) {
        return;
    }
    if !attacker_win.in_active() {
        return;
    }
    if attacker_hit.has_hit {
        return;
    }
    let hitbox_pos = attacker_gt.translation() + attacker_gt.rotation() * attacker_hit.offset;
    let hurtbox_pos = victim_gt.translation() + victim_gt.rotation() * victim_hurt.offset;
    let dist = hitbox_pos.distance(hurtbox_pos);
    if dist >= attacker_hit.radius + victim_hurt.radius {
        return;
    }

    // Dodge i-frames live in the *active* slice of dodge.
    if matches!(victim_state, CharacterState::Dodge) && victim_win.in_active() {
        return;
    }

    let push_dir = (hurtbox_pos - hitbox_pos)
        .with_y(0.0)
        .normalize_or_zero();

    // Parry branch — bypassed entirely by Unblockable attacks. Player must
    // dodge to avoid an unblockable.
    if matches!(victim_state, CharacterState::Parry) && !attacker_unblockable {
        let perfect = victim_win.in_perfect();
        let kind = if perfect {
            HitKind::PerfectParried
        } else {
            HitKind::Parried
        };
        hit_events.send(HitEvent {
            kind,
            attacker: attacker_e,
            victim: victim_e,
            contact_point: hitbox_pos,
            damage_dealt: 0.0,
        });
        attacker_hit.has_hit = true;
        attacker_hit.active = false;

        let stun_s = if perfect {
            PARRY_PERFECT_STUN_S
        } else {
            PARRY_LATE_STUN_S
        };
        let mut ac = commands.entity(attacker_e);
        ac.insert(CharacterState::Stunned);
        ac.insert(ActionTimer {
            timer: Timer::from_seconds(stun_s, TimerMode::Once),
            next_state: Some(CharacterState::Idle),
        });
        // Push parried attacker back; defender holds ground.
        attacker_vel.0 -= push_dir * PUSHBACK_PARRY_ATTACKER;
        // Posture: parried attacker eats posture. Perfect = more.
        let attacker_gain = if perfect {
            POSTURE_GAIN_PERFECT_PARRIED_BY_OPP
        } else {
            POSTURE_GAIN_PARRIED_BY_OPP
        };
        attacker_post.add(attacker_gain, 0.0);
        return;
    }

    // Block branch — chip damage + posture, smaller pushback than full hit.
    // Unblockable attacks also bypass block.
    if matches!(victim_state, CharacterState::Block) && !attacker_unblockable {
        let chip = attacker_hit.damage * BLOCK_CHIP_FRACTION;
        victim_hp.current -= chip;
        let killed = victim_hp.current <= 0.0;
        hit_events.send(HitEvent {
            kind: if killed {
                HitKind::Killed
            } else {
                HitKind::Blocked
            },
            attacker: attacker_e,
            victim: victim_e,
            contact_point: hitbox_pos,
            damage_dealt: chip,
        });
        attacker_hit.has_hit = true;
        attacker_hit.active = false;
        victim_vel.0 += push_dir * PUSHBACK_BLOCK_VICTIM;
        attacker_vel.0 += push_dir * PUSHBACK_BLOCK_ATTACKER;
        victim_post.add(POSTURE_GAIN_BLOCKED, 0.0);
        if killed {
            commands.entity(victim_e).despawn();
        }
        return;
    }

    // Connected hit. Unblockables deal extra damage so they're a real threat
    // for the player to dodge, not just a parry-blocker.
    let damage = if attacker_unblockable {
        attacker_hit.damage * 1.5
    } else {
        attacker_hit.damage
    };
    victim_hp.current -= damage;
    let killed = victim_hp.current <= 0.0;
    hit_events.send(HitEvent {
        kind: if killed {
            HitKind::Killed
        } else {
            HitKind::Connected
        },
        attacker: attacker_e,
        victim: victim_e,
        contact_point: hitbox_pos,
        damage_dealt: damage,
    });

    if matches!(victim_state, CharacterState::Idle | CharacterState::Move) {
        if let Some(mut vc) = commands.get_entity(victim_e) {
            vc.insert(CharacterState::Stunned);
            vc.insert(ActionTimer {
                timer: Timer::from_seconds(STUN_DURATION, TimerMode::Once),
                next_state: Some(CharacterState::Idle),
            });
        }
    }
    // Pushback: victim flies back, attacker steps in.
    victim_vel.0 += push_dir * PUSHBACK_VICTIM;
    attacker_vel.0 += push_dir * PUSHBACK_ATTACKER;
    // Posture: connected hits load both sides — attacker grows confidence,
    // victim eats stagger pressure.
    attacker_post.add(POSTURE_GAIN_LANDED_HIT, 0.0);
    victim_post.add(POSTURE_GAIN_VICTIM_HIT, 0.0);

    attacker_hit.has_hit = true;
    attacker_hit.active = false;

    if killed {
        commands.entity(victim_e).despawn();
    }
}

pub fn prevent_character_overlap(
    mut characters: Query<(&Transform, &mut Velocity, &Hitbox), With<CharacterState>>,
) {
    let mut combinations = characters.iter_combinations_mut();
    while let Some([(t1, mut v1, _h1), (t2, mut v2, _h2)]) = combinations.fetch_next() {
        let body_radius = 0.8;
        let min_dist = body_radius + body_radius;
        let dir = t1.translation - t2.translation;
        let dist = dir.length();
        if dist < min_dist && dist > 0.001 {
            let overlap = min_dist - dist;
            let push_strength = 50.0;
            let push = dir.normalize() * push_strength * overlap;
            v1.0 += push;
            v2.0 -= push;
        }
    }
}
