//! Persistent player dossier metadata sidecar (the `.vdb` itself is owned by
//! `BossMemoryDb`). This file carries the schema version and monotonic tick
//! counter that survives across runs.

use crate::game::memory::DIM;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

pub const DOSSIER_SCHEMA_VERSION: u32 = 1;
pub const DOSSIER_FILENAME: &str = "dossier.vdb";
pub const DOSSIER_META_FILENAME: &str = "dossier.meta.json";
pub const DOSSIER_DIR: &str = ".samurai";

#[derive(Resource, Debug, Clone)]
pub struct Dossier {
    #[allow(dead_code)]
    pub root: PathBuf,
    pub vdb_path: PathBuf,
    pub meta_path: PathBuf,
    pub meta: DossierMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DossierMeta {
    pub schema_version: u32,
    pub dim: u32,
    pub monotonic_tick: u64,
    pub entity_id: u64,
}

impl Default for DossierMeta {
    fn default() -> Self {
        Self {
            schema_version: DOSSIER_SCHEMA_VERSION,
            dim: DIM as u32,
            monotonic_tick: 0,
            entity_id: 0,
        }
    }
}

impl Dossier {
    /// Resolve `~/.samurai/`, ensure the directory exists, load or create the
    /// meta sidecar, archive on schema mismatch.
    pub fn open_or_create() -> Result<Self, DossierError> {
        let root = dossier_root()?;
        std::fs::create_dir_all(&root).map_err(|e| DossierError::Io(e.to_string()))?;

        let vdb_path = root.join(DOSSIER_FILENAME);
        let meta_path = root.join(DOSSIER_META_FILENAME);

        let entity_id = stable_user_entity_id();
        let mut meta = if meta_path.exists() {
            let bytes = std::fs::read(&meta_path).map_err(|e| DossierError::Io(e.to_string()))?;
            serde_json::from_slice::<DossierMeta>(&bytes)
                .unwrap_or_else(|_| DossierMeta::default())
        } else {
            DossierMeta::default()
        };

        // Schema migration: archive incompatible files and start fresh.
        if meta.schema_version != DOSSIER_SCHEMA_VERSION || meta.dim != DIM as u32 {
            archive_existing(&vdb_path, meta.schema_version);
            archive_existing(&meta_path, meta.schema_version);
            meta = DossierMeta::default();
        }
        if meta.entity_id == 0 {
            meta.entity_id = entity_id;
        }

        let dossier = Self {
            root,
            vdb_path,
            meta_path,
            meta,
        };
        dossier.persist_meta()?;
        Ok(dossier)
    }

    pub fn persist_meta(&self) -> Result<(), DossierError> {
        let bytes = serde_json::to_vec_pretty(&self.meta)
            .map_err(|e| DossierError::Io(e.to_string()))?;
        let tmp = self.meta_path.with_extension("json.tmp");
        std::fs::write(&tmp, &bytes).map_err(|e| DossierError::Io(e.to_string()))?;
        std::fs::rename(&tmp, &self.meta_path).map_err(|e| DossierError::Io(e.to_string()))?;
        Ok(())
    }

    pub fn advance_tick(&mut self, by: u64) -> Result<(), DossierError> {
        self.meta.monotonic_tick = self.meta.monotonic_tick.saturating_add(by);
        self.persist_meta()
    }
}

#[derive(Debug)]
pub enum DossierError {
    Io(String),
    NoHome,
}

impl std::fmt::Display for DossierError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(s) => write!(f, "dossier io: {s}"),
            Self::NoHome => write!(f, "could not resolve home directory"),
        }
    }
}

impl std::error::Error for DossierError {}

fn dossier_root() -> Result<PathBuf, DossierError> {
    let home = dirs::home_dir().ok_or(DossierError::NoHome)?;
    Ok(home.join(DOSSIER_DIR))
}

fn stable_user_entity_id() -> u64 {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    let mut h = DefaultHasher::new();
    user.hash(&mut h);
    h.finish()
}

fn archive_existing(path: &Path, prior_version: u32) {
    if !path.exists() {
        return;
    }
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Some(parent) = path.parent() {
        let archived = parent.join(format!("{name}.v{prior_version}.bak"));
        let _ = std::fs::rename(path, archived);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Mutex;

    // The HOME env var is process-global, so tests that mutate it must be
    // serialized.
    static HOME_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn meta_roundtrip() {
        let _g = HOME_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        unsafe { env::set_var("HOME", dir.path()); }
        let dossier = Dossier::open_or_create().unwrap();
        assert!(dossier.meta_path.exists());
        assert_eq!(dossier.meta.schema_version, DOSSIER_SCHEMA_VERSION);
        assert_eq!(dossier.meta.dim, DIM as u32);
        assert_ne!(dossier.meta.entity_id, 0);
    }

    #[test]
    fn schema_mismatch_archives_old_dossier() {
        let _g = HOME_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        unsafe { env::set_var("HOME", dir.path()); }

        let root = dir.path().join(DOSSIER_DIR);
        std::fs::create_dir_all(&root).unwrap();
        let vdb_path = root.join(DOSSIER_FILENAME);
        std::fs::write(&vdb_path, b"fake vdb").unwrap();
        let stale_meta = DossierMeta {
            schema_version: 0,
            dim: 32,
            monotonic_tick: 999,
            entity_id: 42,
        };
        std::fs::write(
            root.join(DOSSIER_META_FILENAME),
            serde_json::to_vec(&stale_meta).unwrap(),
        )
        .unwrap();

        let dossier = Dossier::open_or_create().unwrap();
        assert_eq!(dossier.meta.schema_version, DOSSIER_SCHEMA_VERSION);
        assert_eq!(dossier.meta.dim, DIM as u32);
        assert!(!vdb_path.exists());
        assert!(root.join(format!("{DOSSIER_FILENAME}.v0.bak")).exists());
    }
}
