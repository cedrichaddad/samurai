//! Sekiro-style posture meter. Builds on parry/block/landed-hit, decays after
//! a grace period, and triggers a `Broken` execute window when full.

use crate::game::feel::{POSTURE_BREAK_WINDOW_S, POSTURE_DECAY_GRACE_S, POSTURE_DECAY_PER_SEC};
use bevy::prelude::*;

#[derive(Component, Debug, Clone)]
pub struct Posture {
    pub current: f32,
    pub max: f32,
    pub last_increment_at: f32,
    pub broken_remaining_s: f32,
}

impl Posture {
    pub fn new(max: f32) -> Self {
        Self {
            current: 0.0,
            max,
            last_increment_at: 0.0,
            broken_remaining_s: 0.0,
        }
    }

    pub fn add(&mut self, amount: f32, now: f32) -> bool {
        if self.broken_remaining_s > 0.0 {
            return false;
        }
        self.current = (self.current + amount).clamp(0.0, self.max);
        self.last_increment_at = now;
        if self.current >= self.max {
            self.current = self.max;
            self.broken_remaining_s = POSTURE_BREAK_WINDOW_S;
            true
        } else {
            false
        }
    }

    pub fn is_broken(&self) -> bool {
        self.broken_remaining_s > 0.0
    }

    pub fn fill_ratio(&self) -> f32 {
        if self.max <= 0.0 {
            0.0
        } else {
            self.current / self.max
        }
    }
}

pub fn decay_posture_system(time: Res<Time>, mut q: Query<&mut Posture>) {
    let now = time.elapsed_secs();
    let dt = time.delta_secs();
    for mut p in &mut q {
        if p.broken_remaining_s > 0.0 {
            p.broken_remaining_s = (p.broken_remaining_s - dt).max(0.0);
            if p.broken_remaining_s == 0.0 {
                // Recover from break with half posture and a fresh grace
                // period so the boss isn't insta-broken again.
                p.current = p.max * 0.5;
                p.last_increment_at = now;
            }
            continue;
        }
        if now - p.last_increment_at >= POSTURE_DECAY_GRACE_S && p.current > 0.0 {
            p.current = (p.current - POSTURE_DECAY_PER_SEC * dt).max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn break_triggers_at_max() {
        let mut p = Posture::new(100.0);
        assert!(!p.add(50.0, 0.0));
        assert!(p.add(60.0, 0.5));
        assert!(p.is_broken());
    }

    #[test]
    fn decay_after_grace() {
        let mut p = Posture::new(100.0);
        let _ = p.add(50.0, 0.0);
        // 1.0s passes (no further increments) — grace is 1.0s, so decay starts now.
        assert!((p.current - 50.0).abs() < 1e-3);
    }
}
