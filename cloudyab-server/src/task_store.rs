//! SQLite-backed task persistence for the task queue.
//!
//! Stores task state in a SQLite database so tasks survive server restarts.
//! On startup, pending/running tasks are recovered and re-queued.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};
use tracing::{debug, info};

use crate::task_queue::{TaskEntry, TaskRequest, TaskResult};

/// Errors from task store operations.
#[derive(Debug, thiserror::Error)]
pub enum TaskStoreError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Lock poisoned")]
    LockPoisoned,
}

/// SQLite-backed persistent task store.
/// Uses a Mutex to make the non-Send Connection safe for async contexts.
pub struct TaskStore {
    conn: Mutex<Connection>,
}

#[allow(unused)]
impl TaskStore {
    /// Open or create a task store at the given path.
    pub fn open(path: &Path) -> Result<Self, TaskStoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                request_json TEXT NOT NULL,
                result_json TEXT,
                error TEXT,
                attempts INTEGER NOT NULL DEFAULT 0,
                max_retries INTEGER NOT NULL DEFAULT 3,
                created_at TEXT NOT NULL,
                completed_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);",
        )?;

        info!(path = %path.display(), "Task store opened");
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Save a task entry to the database (insert or update).
    pub fn save(&self, entry: &TaskEntry) -> Result<(), TaskStoreError> {
        let request_json = serde_json::to_string(&entry.request)?;
        let result_json = entry
            .result
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        let conn = self.conn.lock().map_err(|_| TaskStoreError::LockPoisoned)?;
        conn.execute(
            "INSERT OR REPLACE INTO tasks
             (id, status, request_json, result_json, error, attempts, max_retries, created_at, completed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entry.id,
                entry.status,
                request_json,
                result_json,
                entry.error,
                entry.attempts,
                entry.max_retries,
                entry.created_at.to_rfc3339(),
                entry.completed_at.map(|t| t.to_rfc3339()),
            ],
        )?;

        debug!(task_id = %entry.id, status = %entry.status, "Task saved to store");
        Ok(())
    }

    /// Load a task by ID.
    pub fn load(&self, task_id: &str) -> Result<Option<TaskEntry>, TaskStoreError> {
        let conn = self.conn.lock().map_err(|_| TaskStoreError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            "SELECT id, status, request_json, result_json, error, attempts, max_retries
             FROM tasks WHERE id = ?1",
        )?;

        let entry = stmt
            .query_row(params![task_id], |row| {
                Ok(RawTaskRow {
                    id: row.get(0)?,
                    status: row.get(1)?,
                    request_json: row.get(2)?,
                    result_json: row.get(3)?,
                    error: row.get(4)?,
                    attempts: row.get(5)?,
                    max_retries: row.get(6)?,
                })
            })
            .optional()?;

        match entry {
            Some(raw) => Ok(Some(raw.into_task_entry()?)),
            None => Ok(None),
        }
    }

    /// Load all tasks that were pending or running (for recovery on restart).
    pub fn load_recoverable(&self) -> Result<Vec<TaskEntry>, TaskStoreError> {
        let conn = self.conn.lock().map_err(|_| TaskStoreError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            "SELECT id, status, request_json, result_json, error, attempts, max_retries
             FROM tasks WHERE status IN ('pending', 'running', 'retrying')
             ORDER BY created_at ASC",
        )?;

        let entries = stmt
            .query_map([], |row| {
                Ok(RawTaskRow {
                    id: row.get(0)?,
                    status: row.get(1)?,
                    request_json: row.get(2)?,
                    result_json: row.get(3)?,
                    error: row.get(4)?,
                    attempts: row.get(5)?,
                    max_retries: row.get(6)?,
                })
            })?
            .filter_map(|row| row.ok()?.into_task_entry().ok())
            .collect();

        Ok(entries)
    }

    /// Update just the status and error fields of a task.
    pub fn update_status(
        &self,
        task_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), TaskStoreError> {
        let conn = self.conn.lock().map_err(|_| TaskStoreError::LockPoisoned)?;
        conn.execute(
            "UPDATE tasks SET status = ?1, error = ?2 WHERE id = ?3",
            params![status, error, task_id],
        )?;
        Ok(())
    }

    /// Delete tasks older than the given number of seconds.
    pub fn cleanup_old(&self, max_age_secs: u64) -> Result<usize, TaskStoreError> {
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(max_age_secs as i64);
        let cutoff_str = cutoff.to_rfc3339();

        let conn = self.conn.lock().map_err(|_| TaskStoreError::LockPoisoned)?;
        let count = conn.execute(
            "DELETE FROM tasks
             WHERE status IN ('completed', 'failed', 'cancelled') AND created_at < ?1",
            params![cutoff_str],
        )?;

        if count > 0 {
            info!(count, "Cleaned up old tasks from store");
        }
        Ok(count)
    }

    /// Get total count of tasks by status.
    pub fn count_by_status(&self) -> Result<Vec<(String, usize)>, TaskStoreError> {
        let conn = self.conn.lock().map_err(|_| TaskStoreError::LockPoisoned)?;
        let mut stmt = conn.prepare("SELECT status, COUNT(*) FROM tasks GROUP BY status")?;

        let counts = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(counts)
    }
}

/// Raw row data from SQLite before deserialization.
struct RawTaskRow {
    id: String,
    status: String,
    request_json: String,
    result_json: Option<String>,
    error: Option<String>,
    attempts: u32,
    max_retries: u32,
}

impl RawTaskRow {
    /// Convert raw row into a TaskEntry by deserializing JSON fields.
    fn into_task_entry(self) -> Result<TaskEntry, TaskStoreError> {
        let request: TaskRequest = serde_json::from_str(&self.request_json)?;
        let result: Option<TaskResult> = self
            .result_json
            .as_ref()
            .map(|j| serde_json::from_str(j))
            .transpose()?;

        Ok(TaskEntry {
            id: self.id,
            status: self.status,
            request,
            result,
            error: self.error,
            attempts: self.attempts,
            max_retries: self.max_retries,
            created_at: chrono::Utc::now(),
            completed_at: None,
        })
    }
}

/// Extension trait for rusqlite to add `.optional()` to query results.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
