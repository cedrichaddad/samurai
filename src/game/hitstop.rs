//! Hit-stop: brief world-pauses on combat events to convey impact weight.
//! Combat / AI / movement systems gate on this; camera and VFX keep ticking.

use crate::game::feel::{
    HIT_STOP_BLOCK, HIT_STOP_HIT, HIT_STOP_KILL, HIT_STOP_PARRY, HIT_STOP_PERFECT_PARRY,
};
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitKind {
    Connected,
    Blocked,
    Parried,
    PerfectParried,
    Killed,
}

#[derive(Event, Clone, Debug)]
pub struct HitEvent {
    pub kind: HitKind,
    #[allow(dead_code)]
    pub attacker: Entity,
    pub victim: Entity,
    pub contact_point: Vec3,
    #[allow(dead_code)]
    pub damage_dealt: f32,
}

#[derive(Resource, Default, Debug)]
pub struct HitStop {
    pub remaining: u64,
}

impl HitStop {
    pub fn active(&self) -> bool {
        self.remaining > 0
    }
    pub fn add(&mut self, ticks: u64) {
        self.remaining = self.remaining.max(ticks);
    }
}

pub fn no_hit_stop(stop: Res<HitStop>) -> bool {
    !stop.active()
}

/// Decrement the hit-stop counter each tick. Always runs (independent of the
/// hit-stop gate so it can clear).
pub fn tick_hit_stop_system(mut stop: ResMut<HitStop>) {
    if stop.remaining > 0 {
        stop.remaining -= 1;
    }
}

/// Translate hit events into hit-stop frames. Other systems (audio, shake,
/// VFX) read the same events and react in parallel.
pub fn apply_hit_stop_from_events_system(
    mut events: EventReader<HitEvent>,
    mut stop: ResMut<HitStop>,
) {
    for ev in events.read() {
        let frames = match ev.kind {
            HitKind::Connected => HIT_STOP_HIT,
            HitKind::Blocked => HIT_STOP_BLOCK,
            HitKind::Parried => HIT_STOP_PARRY,
            HitKind::PerfectParried => HIT_STOP_PERFECT_PARRY,
            HitKind::Killed => HIT_STOP_KILL,
        };
        stop.add(frames);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_keeps_max() {
        let mut s = HitStop::default();
        s.add(5);
        s.add(3);
        assert_eq!(s.remaining, 5);
        s.add(8);
        assert_eq!(s.remaining, 8);
    }
}
