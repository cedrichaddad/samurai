use bevy::prelude::*;

#[derive(Component)]
pub struct BossAggressionTimer {
    pub timer: Timer,
}

impl Default for BossAggressionTimer {
    fn default() -> Self {
        Self {
            // Force an attack attempt every 3 seconds if nothing else happens
            timer: Timer::from_seconds(3.0, TimerMode::Repeating),
        }
    }
}
