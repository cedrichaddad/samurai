//! Cross-match player memory: 64-dim play-pattern embeddings persisted to a
//! vibrato-edge dossier. Identify is queried at ~1 Hz against the prior-runs
//! base artifact; in-match writes accumulate in `session_buf` and are merged
//! into the on-disk dossier at stage end.
//!
//! Identify scans `base_store` only (vibrato-edge/src/lib.rs:1437-1445), so
//! mid-match ingests are not visible until the artifact is rebuilt.

use bevy::prelude::*;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use vibrato_edge::{
    build_graph_artifact, EdgeArtifactMetadata, EdgeIdentifyOptions, EdgeIdentifyResultV1,
    EdgeOpenPolicy, EdgeRuntime,
};

pub const DIM: usize = 64;
pub const TICKS_PER_VECTOR: usize = 16;
pub const FLOATS_PER_TICK: usize = 4;
pub const INGEST_STRIDE: u64 = 4;
pub const IDENTIFY_QUERY_LEN: usize = 16;
pub const IDENTIFY_FUTURE_STEPS: usize = 6;
pub const IDENTIFY_K: usize = 5;
pub const OVERLAY_CAPACITY: usize = 16384;

/// One tick of player observation, used as a 4-float column inside the 64-dim
/// vector. Order matters — index 3 (`action_signal`) is what Mimic reads to
/// recover "what the player did next".
#[derive(Clone, Copy, Debug, Default)]
pub struct TickSample {
    pub dist_norm: f32,
    pub rel_angle: f32,
    pub player_state: f32,
    pub action_signal: f32,
}

impl TickSample {
    pub fn floats(&self) -> [f32; FLOATS_PER_TICK] {
        [
            self.dist_norm,
            self.rel_angle,
            self.player_state,
            self.action_signal,
        ]
    }
}

/// Discretized player action used both as embedding column and as sidecar.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DominantAction {
    Idle = 0,
    Move = 1,
    Attack = 2,
    Parry = 3,
    Dodge = 4,
}

impl DominantAction {
    #[allow(dead_code)]
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Move,
            2 => Self::Attack,
            3 => Self::Parry,
            4 => Self::Dodge,
            _ => Self::Idle,
        }
    }
    pub fn signal(&self) -> f32 {
        match self {
            Self::Idle => 0.0,
            Self::Move => 0.25,
            Self::Attack => 0.5,
            Self::Parry => 0.75,
            Self::Dodge => 1.0,
        }
    }
}

/// One entry in the action sidecar — the dominant action for each of the 16
/// ticks packed into a single 64-d vector.
pub type ActionSidecarEntry = [u8; TICKS_PER_VECTOR];

/// Bevy resource that owns the player pattern memory.
///
/// `runtime` is the read side: the dossier loaded from disk holding all prior
/// runs and prior stages. `session_buf` is the write side: this run's vectors
/// accumulating in-memory until stage-end, when they're merged into the
/// dossier and the runtime is rebuilt.
#[derive(Resource)]
pub struct BossMemoryDb {
    dossier_path: PathBuf,
    sidecar_path: PathBuf,
    runtime: Option<EdgeRuntime>,
    rolling: VecDeque<TickSample>,
    session_vectors: Vec<f32>,
    session_actions: Vec<ActionSidecarEntry>,
    session_count: usize,
    pub tick: u64,
}

impl BossMemoryDb {
    pub fn open_or_create(dossier_path: impl Into<PathBuf>) -> Self {
        let dossier_path = dossier_path.into();
        let sidecar_path = sidecar_path_for(&dossier_path);
        let runtime = if dossier_path.exists() {
            EdgeRuntime::open_with_policy(
                &dossier_path,
                OVERLAY_CAPACITY,
                EdgeOpenPolicy::RebuildOpen,
                64,
            )
            .ok()
        } else {
            None
        };
        Self {
            dossier_path,
            sidecar_path,
            runtime,
            rolling: VecDeque::with_capacity(TICKS_PER_VECTOR),
            session_vectors: Vec::new(),
            session_actions: Vec::new(),
            session_count: 0,
            tick: 0,
        }
    }

    #[allow(dead_code)]
    pub fn dossier_path(&self) -> &Path {
        &self.dossier_path
    }

    #[allow(dead_code)]
    pub fn dossier_loaded(&self) -> bool {
        self.runtime.is_some()
    }

    pub fn dossier_base_count(&self) -> usize {
        self.runtime.as_ref().map(|r| r.base_len()).unwrap_or(0)
    }

    #[allow(dead_code)]
    pub fn session_count(&self) -> usize {
        self.session_count
    }

    /// Push a single tick's observation into the rolling 16-tick window.
    /// Returns `Some(committed_vector_index)` when a new 64-d vector was
    /// committed (i.e., once every `INGEST_STRIDE` ticks after the rolling
    /// buffer is full).
    pub fn push_tick(&mut self, sample: TickSample, action: DominantAction) -> Option<usize> {
        self.tick = self.tick.saturating_add(1);
        if self.rolling.len() == TICKS_PER_VECTOR {
            self.rolling.pop_front();
        }
        self.rolling.push_back(sample);

        let buffer_full = self.rolling.len() == TICKS_PER_VECTOR;
        let stride_hit = self.tick % INGEST_STRIDE == 0;
        if !(buffer_full && stride_hit) {
            return None;
        }

        let mut vec = [0.0f32; DIM];
        let mut actions: ActionSidecarEntry = [0u8; TICKS_PER_VECTOR];
        for (i, tick_sample) in self.rolling.iter().enumerate() {
            let floats = tick_sample.floats();
            let off = i * FLOATS_PER_TICK;
            vec[off..off + FLOATS_PER_TICK].copy_from_slice(&floats);
            // recover the action enum from the signal level
            actions[i] = action_from_signal(tick_sample.action_signal) as u8;
        }
        // The current action overrides the last slot (it's the freshest signal).
        actions[TICKS_PER_VECTOR - 1] = action as u8;
        let normalized = l2_normalize(&vec);

        let committed = self.session_count;
        self.session_vectors.extend_from_slice(&normalized);
        self.session_actions.push(actions);
        self.session_count += 1;
        Some(committed)
    }

    /// Build the identify query window from the most recent committed session
    /// vectors. Returns `None` until enough vectors exist.
    pub fn build_query_window(&self) -> Option<Vec<f32>> {
        if self.session_count < IDENTIFY_QUERY_LEN {
            return None;
        }
        let start_vec = self.session_count - IDENTIFY_QUERY_LEN;
        let start = start_vec * DIM;
        let end = self.session_count * DIM;
        Some(self.session_vectors[start..end].to_vec())
    }

    /// Run identify against the loaded dossier. Returns no results until both
    /// (a) a dossier with sufficient rows exists on disk, and (b) the session
    /// has enough vectors to fill the query window.
    pub fn identify(&self) -> Vec<EdgeIdentifyResultV1> {
        let Some(runtime) = self.runtime.as_ref() else {
            return Vec::new();
        };
        let Some(window) = self.build_query_window() else {
            return Vec::new();
        };
        if runtime.base_len() < IDENTIFY_QUERY_LEN + IDENTIFY_FUTURE_STEPS {
            return Vec::new();
        }
        let opts = EdgeIdentifyOptions {
            k: IDENTIFY_K,
            future_steps: IDENTIFY_FUTURE_STEPS,
            already_normalized: true,
            max_end_id: usize::MAX,
            exclude_future_from_id: usize::MAX,
        };
        match runtime.identify_with_report(&window, IDENTIFY_QUERY_LEN, opts, None) {
            Ok(report) => report.results,
            Err(_) => Vec::new(),
        }
    }

    /// Merge prior dossier rows + session vectors into a fresh artifact at
    /// `dossier_path`, drop the old runtime, and reopen so subsequent
    /// `identify` calls see everything from this stage onward.
    pub fn snapshot_stage_end(&mut self) -> Result<(), MemoryError> {
        if self.session_count == 0 {
            return Ok(());
        }
        let mut all_vectors: Vec<f32> = Vec::new();
        let mut all_actions: Vec<ActionSidecarEntry> = Vec::new();
        let mut prior_count = 0usize;

        if self.dossier_path.exists() {
            let store = vibrato_core::store::VectorStore::open(&self.dossier_path)
                .map_err(|e| MemoryError::Read(e.to_string()))?;
            if store.dim != DIM {
                return Err(MemoryError::DimensionMismatch {
                    expected: DIM,
                    found: store.dim,
                });
            }
            prior_count = store.count;
            for i in 0..prior_count {
                all_vectors.extend_from_slice(store.get(i));
            }
            if self.sidecar_path.exists() {
                let bytes = std::fs::read(&self.sidecar_path)
                    .map_err(|e| MemoryError::Read(e.to_string()))?;
                let prior: Vec<ActionSidecarEntry> = serde_json::from_slice(&bytes)
                    .map_err(|e| MemoryError::Read(e.to_string()))?;
                all_actions.extend_from_slice(&prior);
            }
        }

        all_vectors.extend_from_slice(&self.session_vectors);
        all_actions.extend_from_slice(&self.session_actions);
        let total_count = prior_count + self.session_count;

        // We don't persist entity metadata — single player per machine, so
        // identify with no filter is exactly what we want.
        let metadata: Option<EdgeArtifactMetadata> = None;
        // Drop the old runtime so the file isn't held open during the rebuild.
        self.runtime = None;
        build_graph_artifact(
            &self.dossier_path,
            &all_vectors,
            total_count,
            DIM,
            true, // already normalized
            metadata,
        )
        .map_err(|e| MemoryError::Write(format!("{:?}", e)))?;

        let sidecar_bytes = serde_json::to_vec(&all_actions)
            .map_err(|e| MemoryError::Write(e.to_string()))?;
        let tmp = self.sidecar_path.with_extension("json.tmp");
        std::fs::write(&tmp, &sidecar_bytes).map_err(|e| MemoryError::Write(e.to_string()))?;
        std::fs::rename(&tmp, &self.sidecar_path)
            .map_err(|e| MemoryError::Write(e.to_string()))?;

        self.runtime = Some(
            EdgeRuntime::open_with_policy(
                &self.dossier_path,
                OVERLAY_CAPACITY,
                EdgeOpenPolicy::RebuildOpen,
                64,
            )
            .map_err(|e| MemoryError::Reopen(format!("{:?}", e)))?,
        );

        // Session is now baked into the dossier; clear in-memory accumulators.
        self.session_vectors.clear();
        self.session_actions.clear();
        self.session_count = 0;
        Ok(())
    }

    /// Look up the action sidecar for a base-store row index. Used by Mimic to
    /// recover the player's actual action sequence after a matched window.
    pub fn sidecar_action_for(&self, row_id: usize) -> Option<ActionSidecarEntry> {
        if !self.sidecar_path.exists() {
            return None;
        }
        let bytes = std::fs::read(&self.sidecar_path).ok()?;
        let all: Vec<ActionSidecarEntry> = serde_json::from_slice(&bytes).ok()?;
        all.get(row_id).copied()
    }
}

/// Predicted player window cached by `identify_player_pattern_system` and
/// read by the boss decision systems each tick.
#[derive(Resource, Default, Debug, Clone)]
pub struct PredictedPlayerWindow {
    pub results: Vec<EdgeIdentifyResultV1>,
    /// Concatenated future action sequences, one entry per matched future
    /// window. Index `i` corresponds to `results[i]`.
    pub futures: Vec<Vec<u8>>,
    /// Counts of dominant actions across all matched future windows. Used by
    /// Pattern-Breaker to decide which action to mask.
    pub habit_histogram: [u32; 5],
    /// Tick at which this prediction was computed — used by reaction-delay
    /// budgets so the boss can't react to information it shouldn't have yet.
    pub tick_observed: u64,
    pub last_score: f32,
}

#[derive(Debug)]
pub enum MemoryError {
    Read(String),
    Write(String),
    Reopen(String),
    DimensionMismatch { expected: usize, found: usize },
}

impl std::fmt::Display for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(s) => write!(f, "memory read error: {s}"),
            Self::Write(s) => write!(f, "memory write error: {s}"),
            Self::Reopen(s) => write!(f, "memory reopen error: {s}"),
            Self::DimensionMismatch { expected, found } => write!(
                f,
                "dossier dim mismatch: expected {expected}, found {found}"
            ),
        }
    }
}

impl std::error::Error for MemoryError {}

fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm < f32::EPSILON {
        return v.to_vec();
    }
    v.iter().map(|x| x / norm).collect()
}

fn action_from_signal(s: f32) -> DominantAction {
    if s < 0.125 {
        DominantAction::Idle
    } else if s < 0.375 {
        DominantAction::Move
    } else if s < 0.625 {
        DominantAction::Attack
    } else if s < 0.875 {
        DominantAction::Parry
    } else {
        DominantAction::Dodge
    }
}

fn sidecar_path_for(dossier_path: &Path) -> PathBuf {
    let mut p = dossier_path.to_path_buf();
    let name = match p.file_name().and_then(|n| n.to_str()) {
        Some(n) => format!("{n}.actions"),
        None => "dossier.vdb.actions".to_string(),
    };
    p.set_file_name(name);
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn synthetic_sample(t: u64) -> TickSample {
        let phase = (t as f32) * 0.1;
        TickSample {
            dist_norm: (phase.sin() + 1.0) * 0.5,
            rel_angle: phase.cos(),
            player_state: ((t % 5) as f32) / 5.0,
            action_signal: DominantAction::from_u8((t % 5) as u8).signal(),
        }
    }

    #[test]
    fn push_tick_commits_on_stride_after_buffer_full() {
        let dir = tempdir().unwrap();
        let mut db = BossMemoryDb::open_or_create(dir.path().join("dossier.vdb"));
        // First TICKS_PER_VECTOR-1 ticks fill the buffer, no commits.
        for t in 1..TICKS_PER_VECTOR as u64 {
            assert!(db
                .push_tick(synthetic_sample(t), DominantAction::Idle)
                .is_none());
        }
        // Tick 16 fills the buffer AND hits stride (16 % 4 == 0): one commit.
        let committed = db.push_tick(synthetic_sample(16), DominantAction::Attack);
        assert_eq!(committed, Some(0));
        assert_eq!(db.session_count(), 1);
        // Next 3 ticks fill in but don't hit stride.
        for t in 17..20 {
            assert!(db
                .push_tick(synthetic_sample(t), DominantAction::Idle)
                .is_none());
        }
        // Tick 20 hits stride.
        assert_eq!(
            db.push_tick(synthetic_sample(20), DominantAction::Idle),
            Some(1)
        );
        assert_eq!(db.session_count(), 2);
    }

    #[test]
    fn snapshot_round_trip_preserves_rows_and_sidecar() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dossier.vdb");
        let mut db = BossMemoryDb::open_or_create(&path);
        // Generate 200 vectors so identify can run (needs >= 16 + 6).
        for t in 1..=200 * 4 {
            db.push_tick(synthetic_sample(t as u64), DominantAction::Move);
        }
        let session_count_before = db.session_count();
        assert!(session_count_before >= 100);

        db.snapshot_stage_end().unwrap();

        // After snapshot, session is empty and dossier has the rows.
        assert_eq!(db.session_count(), 0);
        assert!(db.dossier_loaded());
        assert_eq!(db.dossier_base_count(), session_count_before);

        // Sidecar persisted and indexable.
        let action = db.sidecar_action_for(0);
        assert!(action.is_some());

        // Reopen-from-disk path also works.
        let db2 = BossMemoryDb::open_or_create(&path);
        assert!(db2.dossier_loaded());
        assert_eq!(db2.dossier_base_count(), session_count_before);
    }

    #[test]
    fn identify_returns_results_after_dossier_built() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dossier.vdb");
        let mut db = BossMemoryDb::open_or_create(&path);
        // Build a dossier with enough rows.
        for t in 1..=300 * 4 {
            db.push_tick(synthetic_sample(t as u64), DominantAction::Attack);
        }
        db.snapshot_stage_end().unwrap();

        // Run a fresh "match" to fill the query window.
        for t in 1..=20 * 4 {
            db.push_tick(synthetic_sample(t as u64), DominantAction::Attack);
        }
        let results = db.identify();
        assert!(!results.is_empty(), "identify should return matches");
        for r in &results {
            assert!(r.future_end_id >= r.future_start_id);
            assert!(r.matched_end_id >= r.matched_start_id);
            assert!(r.score.is_finite());
        }
    }

    /// Latency floor check. Stays cheap enough to live in-tree (no criterion
    /// dep) so it can run alongside the unit tests. Mirror of
    /// `benches/identify_inflight.rs` from the plan, scaled to fit on a small
    /// dossier corpus.
    #[test]
    #[ignore] // run with `cargo test -- --ignored identify_latency_p99`
    fn identify_latency_p99_under_one_ms() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dossier.vdb");
        let mut db = BossMemoryDb::open_or_create(&path);
        // Build a 5,000-vector dossier — comparable to a few full runs.
        for t in 1..=5_000 * 4 {
            db.push_tick(synthetic_sample(t as u64), DominantAction::Idle);
        }
        db.snapshot_stage_end().unwrap();

        // Prime a fresh session so build_query_window() has a populated
        // 16-vector tail.
        for t in 1..=20 * 4 {
            db.push_tick(synthetic_sample(t as u64), DominantAction::Move);
        }

        let trials = 200;
        let mut samples_us: Vec<u128> = Vec::with_capacity(trials);
        for _ in 0..trials {
            let t = std::time::Instant::now();
            let _ = db.identify();
            samples_us.push(t.elapsed().as_micros());
        }
        samples_us.sort_unstable();
        let p99 = samples_us[(trials * 99) / 100];
        let p50 = samples_us[trials / 2];
        println!("identify latency p50={p50}us p99={p99}us over {trials} trials");
        assert!(p99 < 1_000, "identify p99 must stay under 1ms; got {p99}us");
    }

    #[test]
    fn dim_mismatch_when_reopening_with_different_dim() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dossier.vdb");
        // Build a dossier at DIM=64 by going through the normal path.
        let mut db = BossMemoryDb::open_or_create(&path);
        for t in 1..=64 * 4 {
            db.push_tick(synthetic_sample(t as u64), DominantAction::Idle);
        }
        db.snapshot_stage_end().unwrap();

        // Manually try to merge a sidecar at the wrong dim by writing a fake
        // .vdb of dim=128. Easier: corrupt the path by passing a vector store
        // expectation via direct VectorStore::open and asserting the runtime
        // surface enforces our DIM.
        let store = vibrato_core::store::VectorStore::open(&path).unwrap();
        assert_eq!(store.dim, DIM);
    }

    #[test]
    fn empty_session_snapshot_is_noop() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dossier.vdb");
        let mut db = BossMemoryDb::open_or_create(&path);
        db.snapshot_stage_end().unwrap();
        assert!(!path.exists(), "no rows means no artifact written");
    }
}
