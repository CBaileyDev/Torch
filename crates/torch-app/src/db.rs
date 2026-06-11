//! SQLite persistence: runs, their event transcripts, settings, templates.

use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub id: String,
    pub goal: String,
    pub workdir: String,
    pub preset: String,
    pub created_at: i64,
    pub status: String,
    pub refine_iterations: i64,
    pub total_turns: i64,
    pub total_output_tokens: i64,
}

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS runs (
            id TEXT PRIMARY KEY,
            goal TEXT NOT NULL,
            workdir TEXT NOT NULL,
            preset TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            status TEXT NOT NULL,
            refine_iterations INTEGER NOT NULL DEFAULT 0,
            total_turns INTEGER NOT NULL DEFAULT 0,
            total_output_tokens INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS events (
            run_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            json TEXT NOT NULL,
            PRIMARY KEY (run_id, seq)
        );
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS templates (
            name TEXT PRIMARY KEY,
            content TEXT NOT NULL
        );",
    )?;
    Ok(conn)
}

pub fn insert_run(
    conn: &Connection,
    id: &str,
    goal: &str,
    workdir: &str,
    preset: &str,
    created_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO runs (id, goal, workdir, preset, created_at, status) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'running')",
        params![id, goal, workdir, preset, created_at],
    )?;
    Ok(())
}

pub fn set_run_status(conn: &Connection, id: &str, status: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE runs SET status = ?2 WHERE id = ?1",
        params![id, status],
    )?;
    Ok(())
}

pub fn finish_run(
    conn: &Connection,
    id: &str,
    status: &str,
    refine_iterations: i64,
    total_turns: i64,
    total_output_tokens: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE runs SET status = ?2, refine_iterations = ?3, total_turns = ?4, \
         total_output_tokens = ?5 WHERE id = ?1",
        params![
            id,
            status,
            refine_iterations,
            total_turns,
            total_output_tokens
        ],
    )?;
    Ok(())
}

pub fn append_event(conn: &Connection, run_id: &str, json: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO events (run_id, seq, json) VALUES (?1, \
         (SELECT COALESCE(MAX(seq), 0) + 1 FROM events WHERE run_id = ?1), ?2)",
        params![run_id, json],
    )?;
    Ok(())
}

pub fn run_events(conn: &Connection, run_id: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT json FROM events WHERE run_id = ?1 ORDER BY seq ASC")?;
    let rows = stmt.query_map(params![run_id], |row| row.get::<_, String>(0))?;
    rows.collect()
}

pub fn list_runs(conn: &Connection) -> rusqlite::Result<Vec<RunSummary>> {
    let mut stmt = conn.prepare(
        "SELECT id, goal, workdir, preset, created_at, status, refine_iterations, \
         total_turns, total_output_tokens FROM runs ORDER BY created_at DESC LIMIT 200",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(RunSummary {
            id: row.get(0)?,
            goal: row.get(1)?,
            workdir: row.get(2)?,
            preset: row.get(3)?,
            created_at: row.get(4)?,
            status: row.get(5)?,
            refine_iterations: row.get(6)?,
            total_turns: row.get(7)?,
            total_output_tokens: row.get(8)?,
        })
    })?;
    rows.collect()
}

pub fn get_setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
}

pub fn all_settings(conn: &Connection) -> rusqlite::Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.collect()
}

pub fn save_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn all_templates(conn: &Connection) -> rusqlite::Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT name, content FROM templates")?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.collect()
}

pub fn save_template(conn: &Connection, name: &str, content: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO templates (name, content) VALUES (?1, ?2) \
         ON CONFLICT(name) DO UPDATE SET content = excluded.content",
        params![name, content],
    )?;
    Ok(())
}
