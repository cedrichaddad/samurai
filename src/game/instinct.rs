//! In-match "instinct" buffer — an in-memory ring of the most recent player
//! tick samples plus brute-force cosine NN.
//!
//! This is intentionally NOT vibrato. Identify only scans the on-disk base
//! artifact, so it can't see this match's writes until stage-end. For
//! sub-second reactions we keep our own ring buffer in the boss and search it
//! with a tight loop. ~1000 × 64 = 64k dot products per query, well under 100
//! microseconds.

use crate::game::memory::{TickSample, DIM, FLOATS_PER_TICK, TICKS_PER_VECTOR};
use bevy::prelude::*;
use std::collections::VecDeque;

pub const INSTINCT_CAPACITY: usize = 1024;

/// One stored instinct sample: a 64-d L2-normalized vector with the dominant
/// action that led the matching player to that frame.
#[derive(Clone)]
struct InstinctRow {
    vec: [f32; DIM],
    last_action: u8,
}

/// Boss-owned ring buffer of recent player play patterns. Each row encodes a
/// 16-tick rolling window. Reads happen on the boss decision tick; writes
/// happen alongside `BossMemoryDb::push_tick`.
#[derive(Resource)]
pub struct Instinct {
    rolling: VecDeque<TickSample>,
    rows: VecDeque<InstinctRow>,
    tick: u64,
}

impl Default for Instinct {
    fn default() -> Self {
        Self::new()
    }
}

impl Instinct {
    pub fn new() -> Self {
        Self {
            rolling: VecDeque::with_capacity(TICKS_PER_VECTOR),
            rows: VecDeque::with_capacity(INSTINCT_CAPACITY),
            tick: 0,
        }
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Mirrors [`BossMemoryDb::push_tick`] cadence: every 4 ticks once the
    /// rolling window is full, commit a normalized 64-d snapshot.
    pub fn push_tick(&mut self, sample: TickSample, last_action: u8) -> bool {
        self.tick = self.tick.saturating_add(1);
        if self.rolling.len() == TICKS_PER_VECTOR {
            self.rolling.pop_front();
        }
        self.rolling.push_back(sample);

        if self.rolling.len() < TICKS_PER_VECTOR {
            return false;
        }
        if self.tick % crate::game::memory::INGEST_STRIDE != 0 {
            return false;
        }

        let mut vec = [0.0f32; DIM];
        for (i, ts) in self.rolling.iter().enumerate() {
            let off = i * FLOATS_PER_TICK;
            let f = ts.floats();
            vec[off..off + FLOATS_PER_TICK].copy_from_slice(&f);
        }
        let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > f32::EPSILON {
            for x in &mut vec {
                *x /= norm;
            }
        }
        if self.rows.len() == INSTINCT_CAPACITY {
            self.rows.pop_front();
        }
        self.rows.push_back(InstinctRow { vec, last_action });
        true
    }

    /// Most-similar past row by cosine over the rolling window. Returns `None`
    /// until the rolling window is full and at least 2 rows exist (so we can
    /// skip the trivial self-match at the tail).
    pub fn nearest(&self) -> Option<InstinctMatch> {
        if self.rolling.len() < TICKS_PER_VECTOR || self.rows.len() < 2 {
            return None;
        }
        let mut query = [0.0f32; DIM];
        for (i, ts) in self.rolling.iter().enumerate() {
            let off = i * FLOATS_PER_TICK;
            let f = ts.floats();
            query[off..off + FLOATS_PER_TICK].copy_from_slice(&f);
        }
        let norm = query.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > f32::EPSILON {
            for x in &mut query {
                *x /= norm;
            }
        }
        // Skip the most recent row (the trivial self-match).
        let cutoff = self.rows.len() - 1;
        let mut best: Option<InstinctMatch> = None;
        for (idx, row) in self.rows.iter().take(cutoff).enumerate() {
            let mut s = 0.0f32;
            for k in 0..DIM {
                s += query[k] * row.vec[k];
            }
            if best.as_ref().map(|m| s > m.score).unwrap_or(true) {
                best = Some(InstinctMatch {
                    score: s,
                    last_action: row.last_action,
                    row_idx: idx,
                });
            }
        }
        best
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InstinctMatch {
    pub score: f32,
    pub last_action: u8,
    #[allow(dead_code)]
    pub row_idx: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::memory::DominantAction;

    fn ts(phase: f32, action: DominantAction) -> TickSample {
        TickSample {
            dist_norm: (phase.sin() + 1.0) * 0.5,
            rel_angle: phase.cos(),
            player_state: 0.0,
            action_signal: action.signal(),
        }
    }

    #[test]
    fn nearest_recovers_repeated_pattern() {
        let mut inst = Instinct::new();
        // Pattern A: 16 ticks of "attack-shaped" samples.
        for i in 0..16 {
            inst.push_tick(ts(i as f32 * 0.1, DominantAction::Attack), 5);
        }
        // 32 different ticks separating.
        for i in 0..32 {
            inst.push_tick(
                ts(20.0 + i as f32 * 0.3, DominantAction::Idle),
                0,
            );
        }
        // Re-emit Pattern A so the nearest match should be the first window.
        for i in 0..16 {
            inst.push_tick(ts(i as f32 * 0.1, DominantAction::Attack), 5);
        }
        let m = inst.nearest().unwrap();
        // The matched row's last_action must be the Attack-flavored one (5).
        assert_eq!(m.last_action, 5);
        assert!(m.score > 0.5);
    }

    #[test]
    fn capacity_enforced() {
        let mut inst = Instinct::new();
        // Push enough to exceed capacity by 100.
        for i in 0..(INSTINCT_CAPACITY + 100) * 4 {
            inst.push_tick(ts(i as f32 * 0.01, DominantAction::Move), 1);
        }
        assert_eq!(inst.len(), INSTINCT_CAPACITY);
    }

    #[test]
    fn nearest_none_until_filled_window() {
        let mut inst = Instinct::new();
        // First 15 ticks fill the rolling window but never commit.
        for i in 1..15 {
            inst.push_tick(ts(i as f32, DominantAction::Idle), 0);
        }
        assert!(inst.nearest().is_none());
    }
}
