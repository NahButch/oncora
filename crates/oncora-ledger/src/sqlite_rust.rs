//! Pure-Rust SQLite backend via `turso` (the Rust rewrite of SQLite, formerly
//! `limbo`). Same schema and same [`LedgerStore`] trait as the C backend — the
//! only difference is which engine executes the SQL. This is the runnable form
//! of the C→Rust migration spike in `docs/08-roadmap.md`.

use async_trait::async_trait;
use oncora_core::{ContentHash, LedgerRecord, LedgerStore, OncoraError, Result, RunId};
use tokio::sync::Mutex;
use turso::{Builder, Connection};

use crate::SCHEMA;

/// A [`LedgerStore`] backed by pure-Rust SQLite (`turso`).
pub struct SqliteRustLedger {
    conn: Mutex<Connection>,
}

fn storage(e: impl std::fmt::Display) -> OncoraError {
    OncoraError::Storage(format!("sqlite-rust: {e}"))
}

/// Extract a TEXT column from a turso row.
fn col_text(row: &turso::Row, i: usize) -> Result<String> {
    row.get_value(i)
        .map_err(storage)?
        .as_text()
        .map(|s| s.to_string())
        .ok_or_else(|| storage(format!("column {i} is not text")))
}

/// Extract an INTEGER column from a turso row.
fn col_int(row: &turso::Row, i: usize) -> Result<i64> {
    row.get_value(i)
        .map_err(storage)?
        .as_integer()
        .copied()
        .ok_or_else(|| storage(format!("column {i} is not an integer")))
}

impl SqliteRustLedger {
    /// Open (or create) a database file.
    pub async fn open(path: &str) -> Result<Self> {
        let db = Builder::new_local(path).build().await.map_err(storage)?;
        let conn = db.connect().map_err(storage)?;
        conn.execute(SCHEMA, ()).await.map_err(storage)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Open an ephemeral in-memory database.
    pub async fn open_in_memory() -> Result<Self> {
        Self::open(":memory:").await
    }
}

#[async_trait]
impl LedgerStore for SqliteRustLedger {
    async fn append(&self, record: LedgerRecord) -> Result<i64> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO ledger (run_id, seq, kind, payload, content_hash) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                record.run_id.0.clone(),
                record.seq,
                record.kind.clone(),
                record.payload.clone(),
                record.content_hash.0.clone(),
            ),
        )
        .await
        .map_err(storage)?;
        Ok(conn.last_insert_rowid())
    }

    async fn records_for(&self, run_id: &RunId) -> Result<Vec<LedgerRecord>> {
        let conn = self.conn.lock().await;
        let mut rows = conn
            .query(
                "SELECT run_id, seq, kind, payload, content_hash \
                 FROM ledger WHERE run_id = ?1 ORDER BY seq",
                (run_id.0.clone(),),
            )
            .await
            .map_err(storage)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            out.push(LedgerRecord {
                run_id: RunId::new(col_text(&row, 0)?),
                seq: col_int(&row, 1)?,
                kind: col_text(&row, 2)?,
                payload: col_text(&row, 3)?,
                content_hash: ContentHash::new(col_text(&row, 4)?),
            });
        }
        Ok(out)
    }

    async fn count(&self) -> Result<usize> {
        let conn = self.conn.lock().await;
        let mut rows = conn
            .query("SELECT COUNT(*) FROM ledger", ())
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or_else(|| storage("count returned no row"))?;
        Ok(col_int(&row, 0)? as usize)
    }
}
