use bevy::prelude::*;

pub const ATTACK_DURATION: f32 = 0.5;
pub const PARRY_DURATION: f32 = 0.2;
pub const DODGE_DURATION: f32 = 0.3;
pub const STUN_DURATION: f32 = 0.3;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CharacterState {
    #[default]
    Idle,
    Move,
    Attack,
    Parry,
    Dodge,
    #[allow(dead_code)]
    Stunned,
}

#[derive(Component, Default)]
pub struct ActionTimer {
    pub timer: Timer,
    pub next_state: Option<CharacterState>,
}

pub fn update_action_timers(
    time: Res<Time>,
    mut query: Query<(Entity, &mut CharacterState, &mut ActionTimer)>,
) {
    for (_entity, mut state, mut action_timer) in &mut query {
        action_timer.timer.tick(time.delta());
        if action_timer.timer.finished() {
            if let Some(next) = action_timer.next_state {
                *state = next;
            } else {
                *state = CharacterState::Idle;
            }
            // Disable timer by pausing or resetting? 
            // Better to remove component or just have a flag?
            // For now, we assume if timer finished, we transition.
            // But we don't want to transition every frame after finish.
            // So we might need to reset timer or something.
            // Actually, let's just use the state transition to reset logic if needed.
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
        Self { current: 100.0, max: 100.0 }
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
        Self { radius: 1.0, offset: Vec3::new(0.0, 1.0, 1.0), active: false, has_hit: false, damage: 10.0 }
    }
}

#[derive(Component)]
pub struct Hurtbox {
    pub radius: f32,
    pub offset: Vec3,
}

impl Default for Hurtbox {
    fn default() -> Self {
        Self { radius: 0.5, offset: Vec3::new(0.0, 1.0, 0.0) }
    }
}

pub fn combat_collision(
    mut commands: Commands,
    mut attacker_query: Query<(Entity, &GlobalTransform, &mut Hitbox, &CharacterState)>,
    mut victim_query: Query<(Entity, &GlobalTransform, &Hurtbox, &mut Health, &CharacterState)>,
) {
    for (attacker_entity, attacker_tf, mut hitbox, attacker_state) in &mut attacker_query {
        // Only active if attacking
        if *attacker_state == CharacterState::Attack {
            if !hitbox.has_hit {
                hitbox.active = true;
            } else {
                hitbox.active = false;
            }
        } else {
            hitbox.active = false;
            hitbox.has_hit = false; // Reset for next attack
            continue;
        }

        if !hitbox.active { continue; }

        let hitbox_pos = attacker_tf.translation() + attacker_tf.rotation() * hitbox.offset;

        for (victim_entity, victim_tf, hurtbox, mut health, victim_state) in &mut victim_query {
            if attacker_entity == victim_entity { continue; } // Don't hit self

            let hurtbox_pos = victim_tf.translation() + victim_tf.rotation() * hurtbox.offset;
            let distance = hitbox_pos.distance(hurtbox_pos);

            if distance < (hitbox.radius + hurtbox.radius) {
                // Collision!
                
                // Check for Parry/Dodge
                match *victim_state {
                    CharacterState::Dodge => {
                        // Miss due to dodge
                        continue;
                    }
                    CharacterState::Parry => {
                        println!("Parried!");
                        hitbox.has_hit = true; // Mark as hit to avoid multi-proc
                        hitbox.active = false;
                        
                        // Stun the attacker
                        let mut attacker_commands = commands.entity(attacker_entity);
                        attacker_commands.insert(CharacterState::Stunned);
                        // Also need to set a timer for the stun duration?
                        // ActionTimer is on the entity.
                        attacker_commands.insert(ActionTimer {
                            timer: Timer::from_seconds(1.0, TimerMode::Once),
                            next_state: Some(CharacterState::Idle),
                        });
                        continue;
                    }
                    _ => {} // Handle other states or do nothing
                }

                // Apply damage
                health.current -= hitbox.damage;
                println!("Hit! Health: {}", health.current);
                
                // Hit Stun Logic
                // If victim is Idle or Move, stun them.
                if *victim_state == CharacterState::Idle || *victim_state == CharacterState::Move {
                    if let Some(mut victim_commands) = commands.get_entity(victim_entity) {
                        victim_commands.insert(CharacterState::Stunned);
                        victim_commands.insert(ActionTimer {
                            timer: Timer::from_seconds(STUN_DURATION, TimerMode::Once),
                            next_state: Some(CharacterState::Idle),
                        });
                    }
                }

                hitbox.has_hit = true;
                hitbox.active = false;
                
                if health.current <= 0.0 {
                    println!("Entity {:?} died!", victim_entity);
                    commands.entity(victim_entity).despawn();
                }
            }
        }
    }
}

pub fn prevent_character_overlap(
    mut characters: Query<(&mut Transform, &crate::game::combat::Hitbox), With<crate::game::combat::CharacterState>>,
) {
    let mut combinations = characters.iter_combinations_mut();
    while let Some([(mut t1, _h1), (mut t2, _h2)]) = combinations.fetch_next() {
        // Use a fixed body radius for physics (separate from hitbox sometimes, but hitbox radius 1.0 is fine)
        let body_radius = 0.8; 
        let min_dist = body_radius + body_radius;
        
        let dir = t1.translation - t2.translation;
        let dist = dir.length();
        
        if dist < min_dist && dist > 0.001 {
            let overlap = min_dist - dist;
            let push = dir.normalize() * (overlap / 2.0);
            
            // Push both apart
            t1.translation += push;
            t2.translation -= push;
        }
    }
}
