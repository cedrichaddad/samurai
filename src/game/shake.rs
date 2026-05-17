//! Trauma-based camera shake (Squirrel Eiserloh's GDC model). Trauma is a
//! [0,1] scalar that decays each second; visual shake intensity = trauma², so
//! it ramps in and out smoothly.

use crate::game::feel::{
    TRAUMA_BLOCK, TRAUMA_DECAY_PER_SEC, TRAUMA_HIT, TRAUMA_KILL, TRAUMA_PARRY,
    TRAUMA_PERFECT_PARRY,
};
use crate::game::hitstop::{HitEvent, HitKind};
use bevy::prelude::*;

#[derive(Resource, Default, Debug)]
pub struct Trauma {
    /// Persistent trauma from events.
    pub level: f32,
    /// Phase counter for sampling our cheap pseudo-noise functions.
    pub time: f32,
}

impl Trauma {
    pub fn add(&mut self, amount: f32) {
        self.level = (self.level + amount).clamp(0.0, 1.0);
    }
    pub fn shake(&self) -> f32 {
        self.level * self.level
    }
}

pub fn ingest_hit_events_into_trauma_system(
    mut events: EventReader<HitEvent>,
    mut trauma: ResMut<Trauma>,
) {
    for ev in events.read() {
        let amount = match ev.kind {
            HitKind::Connected => TRAUMA_HIT,
            HitKind::Blocked => TRAUMA_BLOCK,
            HitKind::Parried => TRAUMA_PARRY,
            HitKind::PerfectParried => TRAUMA_PERFECT_PARRY,
            HitKind::Killed => TRAUMA_KILL,
        };
        trauma.add(amount);
    }
}

pub fn decay_trauma_system(time: Res<Time>, mut trauma: ResMut<Trauma>) {
    let dt = time.delta_secs();
    trauma.time += dt;
    trauma.level = (trauma.level - TRAUMA_DECAY_PER_SEC * dt).max(0.0);
}

/// Cheap deterministic shake offsets — three independent perlin-ish
/// oscillations. Avoids pulling in a noise crate.
pub fn shake_offset(time: f32, level: f32) -> Vec3 {
    if level <= 0.0 {
        return Vec3::ZERO;
    }
    let amp = 0.45 * level; // metres
    let f1 = (time * 53.0).sin() * (time * 17.0).cos();
    let f2 = (time * 37.0 + 1.3).sin() * (time * 23.0 + 0.4).cos();
    let f3 = (time * 47.0 + 2.1).sin() * (time * 19.0 + 1.8).cos();
    Vec3::new(f1, f2, f3) * amp
}

pub fn rotation_shake(time: f32, level: f32) -> f32 {
    if level <= 0.0 {
        return 0.0;
    }
    let amp = 0.04 * level; // radians
    ((time * 41.0).sin() * (time * 27.0 + 0.7).cos()) * amp
}
