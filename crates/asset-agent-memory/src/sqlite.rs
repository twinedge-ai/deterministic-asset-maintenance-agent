use crate::schema::SQLITE_SCHEMA;
use asset_agent_core::checklist::DEFAULT_CHECKLIST_ITEMS;
use rusqlite::{params, Connection};
use std::{
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("failed to create database directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("case not found: {0}")]
    CaseNotFound(String),
    #[error("invalid feedback: {0}")]
    InvalidFeedback(String),
}

pub type MemoryResult<T> = Result<T, MemoryError>;

pub struct MemoryStore {
    pub(crate) conn: Connection,
}

impl MemoryStore {
    pub fn open(path: impl AsRef<Path>) -> MemoryResult<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| MemoryError::CreateDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.initialize()?;
        Ok(store)
    }

    pub fn open_in_memory() -> MemoryResult<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.initialize()?;
        Ok(store)
    }

    fn initialize(&self) -> MemoryResult<()> {
        self.conn.pragma_update(None, "journal_mode", "WAL")?;
        self.conn.pragma_update(None, "synchronous", "NORMAL")?;
        self.conn.execute_batch(SQLITE_SCHEMA)?;
        self.seed_checklist()?;
        self.seed_default_threshold()?;
        Ok(())
    }

    fn seed_checklist(&self) -> MemoryResult<()> {
        for item in DEFAULT_CHECKLIST_ITEMS {
            self.conn.execute(
                "INSERT INTO checklist_stats(item_id, item_text, fault_type, successes, attempts)
                 VALUES(?1, ?2, 'low_npsh_cavitation', 0, 0)
                 ON CONFLICT(item_id) DO UPDATE SET item_text = excluded.item_text",
                params![item.item_id, item.item_text],
            )?;
        }
        Ok(())
    }

    fn seed_default_threshold(&self) -> MemoryResult<()> {
        self.conn.execute(
            "INSERT INTO pump_thresholds(
               asset_id, threshold_name, default_value, current_value, proposed_value,
               evidence_count, status
             )
             VALUES('hp_pump_1', 'npsh_margin_high_risk_m', 0.20, 0.20, NULL, 0, 'collecting_evidence')
             ON CONFLICT(asset_id, threshold_name) DO NOTHING",
            [],
        )?;
        Ok(())
    }
}
