//! C SQLite backend: the upstream SQLite amalgamation, compiled and linked via
//! `rusqlite` (the `bundled` feature builds the C library — no system SQLite
//! required). This is a non-Rust dependency, isolated behind the
//! [`LedgerStore`] trait exactly as `docs/05-tech-decisions.md` prescribes.

use std::sync::Mutex;

use async_trait::async_trait;
use oncora_core::{ContentHash, LedgerRecord, LedgerStore, OncoraError, Result, RunId};
use rusqlite::{Connection, params};

use crate::SCHEMA;

/// A [`LedgerStore`] backed by C SQLite.
pub struct SqliteCLedger {
    conn: Mutex<Connection>,
}

fn storage(e: impl std::fmt::Display) -> OncoraError {
    OncoraError::Storage(format!("sqlite-c: {e}"))
}

impl SqliteCLedger {
    /// Open (or create) a database file.
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path).map_err(storage)?;
        conn.execute_batch(SCHEMA).map_err(storage)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Open an ephemeral in-memory database.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(storage)?;
        conn.execute_batch(SCHEMA).map_err(storage)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

#[async_trait]
impl LedgerStore for SqliteCLedger {
    async fn append(&self, record: LedgerRecord) -> Result<i64> {
        let conn = self.conn.lock().map_err(|_| storage("lock poisoned"))?;
        conn.execute(
            "INSERT INTO ledger (run_id, seq, kind, payload, content_hash) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                record.run_id.as_str(),
                record.seq,
                record.kind,
                record.payload,
                record.content_hash.as_str()
            ],
        )
        .map_err(storage)?;
        Ok(conn.last_insert_rowid())
    }

    async fn records_for(&self, run_id: &RunId) -> Result<Vec<LedgerRecord>> {
        let conn = self.conn.lock().map_err(|_| storage("lock poisoned"))?;
        let mut stmt = conn
            .prepare(
                "SELECT run_id, seq, kind, payload, content_hash \
                 FROM ledger WHERE run_id = ?1 ORDER BY seq",
            )
            .map_err(storage)?;
        let rows = stmt
            .query_map(params![run_id.as_str()], |row| {
                Ok(LedgerRecord {
                    run_id: RunId::new(row.get::<_, String>(0)?),
                    seq: row.get(1)?,
                    kind: row.get(2)?,
                    payload: row.get(3)?,
                    content_hash: ContentHash::new(row.get::<_, String>(4)?),
                })
            })
            .map_err(storage)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(storage)?);
        }
        Ok(out)
    }

    async fn count(&self) -> Result<usize> {
        let conn = self.conn.lock().map_err(|_| storage("lock poisoned"))?;
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM ledger", [], |row| row.get(0))
            .map_err(storage)?;
        Ok(n as usize)
    }
}
