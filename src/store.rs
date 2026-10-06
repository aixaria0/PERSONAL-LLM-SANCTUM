use crate::domain::{ChatResponse, MemoryItem, RunView};
use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::{
    collections::HashSet,
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

impl Store {
    pub fn open(path: &str) -> Result<Self> {
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create database directory: {}", parent.display())
            })?;
        }
        let conn =
            Connection::open(path).with_context(|| format!("failed to open database: {path}"))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS memories (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               content TEXT NOT NULL,
               tags TEXT NOT NULL DEFAULT '',
               created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS runs (
               run_id TEXT PRIMARY KEY,
               input TEXT NOT NULL,
               intent TEXT NOT NULL,
               path TEXT NOT NULL,
               models_json TEXT NOT NULL,
               evidence_json TEXT NOT NULL,
               critique TEXT,
               verification TEXT,
               final_answer TEXT NOT NULL,
               elapsed_ms INTEGER NOT NULL,
               created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn add_memory(&self, content: &str, tags: &str) -> Result<i64> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        conn.execute(
            "INSERT INTO memories(content, tags) VALUES (?1, ?2)",
            params![content, tags],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn search_memory(&self, query: &str, limit: usize) -> Result<Vec<MemoryItem>> {
        let tokens: HashSet<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() >= 3)
            .map(|s| s.to_lowercase())
            .collect();

        let conn = self.conn.lock().expect("database mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, content, tags, created_at FROM memories ORDER BY id DESC LIMIT 250",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(MemoryItem {
                id: row.get(0)?,
                content: row.get(1)?,
                tags: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut scored = Vec::new();
        for item in rows.flatten() {
            let haystack = format!("{} {}", item.content, item.tags).to_lowercase();
            let score = if tokens.is_empty() {
                0
            } else {
                tokens
                    .iter()
                    .filter(|t| haystack.contains(t.as_str()))
                    .count()
            };
            if score > 0 || tokens.is_empty() {
                scored.push((score, item));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.id.cmp(&a.1.id)));
        Ok(scored
            .into_iter()
            .take(limit)
            .map(|(_, item)| item)
            .collect())
    }

    pub fn save_run(&self, input: &str, response: &ChatResponse) -> Result<()> {
        let models: Vec<_> = response.evidence.iter().map(|e| e.model.clone()).collect();
        let models_json = serde_json::to_string(&models)?;
        let evidence_json = serde_json::to_string(&response.evidence)?;
        let conn = self.conn.lock().expect("database mutex poisoned");
        conn.execute(
            "INSERT INTO runs(run_id,input,intent,path,models_json,evidence_json,critique,verification,final_answer,elapsed_ms)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                response.run_id.to_string(),
                input,
                format!("{:?}", response.intent).to_lowercase(),
                format!("{:?}", response.path).to_lowercase(),
                models_json,
                evidence_json,
                response.critique,
                response.verification,
                response.final_answer,
                response.elapsed_ms as i64,
            ],
        )?;
        Ok(())
    }

    pub fn get_run(&self, run_id: &str) -> Result<Option<RunView>> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT run_id,input,intent,path,models_json,evidence_json,critique,verification,final_answer,elapsed_ms,created_at
             FROM runs WHERE run_id = ?1"
        )?;
        let mut rows = stmt.query(params![run_id])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(RunView {
                run_id: row.get(0)?,
                input: row.get(1)?,
                intent: row.get(2)?,
                path: row.get(3)?,
                models_json: row.get(4)?,
                evidence_json: row.get(5)?,
                critique: row.get(6)?,
                verification: row.get(7)?,
                final_answer: row.get(8)?,
                elapsed_ms: row.get(9)?,
                created_at: row.get(10)?,
            }));
        }
        Ok(None)
    }
}
