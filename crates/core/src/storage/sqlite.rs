use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::Connection;

use crate::domain::{WorkspaceInfo, WorkspaceType};
use crate::workspace::config::WorkspaceEntry;

use super::{ApiHistoryEntry, ApiHistoryStorage, UiPrefsStorage, WorkspaceStorage};

pub struct SqliteStorage {
    conn: Mutex<Connection>,
}

impl SqliteStorage {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        let storage = Self {
            conn: Mutex::new(conn),
        };
        storage.initialize_schema()?;
        Ok(storage)
    }

    fn initialize_schema(&self) -> anyhow::Result<()> {
        let mut conn = self.conn.lock();
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);",
        )?;

        let version: i64 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )?;

        if version < 1 {
            let tx = conn.transaction()?;
            tx.execute_batch(SCHEMA_V1)?;
            tx.execute("INSERT INTO schema_version (version) VALUES (1)", [])?;
            tx.commit()?;
        }

        Ok(())
    }

    /// Migrate all workspaces from JSON config files into SQLite.
    /// Returns the number of entries migrated.
    pub fn migrate_from_json(&self) -> anyhow::Result<usize> {
        let entries = crate::workspace::config::load_all();
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let mut count = 0;

        for entry in &entries {
            let rows = tx.execute(
                "INSERT OR IGNORE INTO workspaces (project_root, name, description, prompt, kanban_path, branch, worktree_path, source_repo, workspace_type, group_name, display_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                rusqlite::params![
                    entry.source_repo.to_string_lossy(),
                    entry.name,
                    entry.description,
                    entry.prompt,
                    entry.kanban_path,
                    entry.branch,
                    entry.worktree_path.to_string_lossy(),
                    entry.source_repo.to_string_lossy(),
                    workspace_type_str(entry.workspace_type),
                    entry.group,
                    entry.order,
                ],
            )?;
            count += rows;
        }

        tx.commit()?;
        Ok(count)
    }
}

const SCHEMA_V1: &str = r"
CREATE TABLE IF NOT EXISTS workspaces (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    project_root    TEXT NOT NULL,
    name            TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    prompt          TEXT NOT NULL DEFAULT '',
    kanban_path     TEXT,
    branch          TEXT NOT NULL,
    worktree_path   TEXT NOT NULL UNIQUE,
    source_repo     TEXT NOT NULL,
    workspace_type  TEXT NOT NULL DEFAULT 'Worktree',
    group_name      TEXT,
    display_order   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_workspaces_source_repo ON workspaces(source_repo);

CREATE TABLE IF NOT EXISTS api_history (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    source_repo      TEXT NOT NULL,
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    request_text     TEXT NOT NULL,
    method           TEXT NOT NULL,
    url              TEXT NOT NULL,
    status           INTEGER NOT NULL,
    elapsed_ms       INTEGER NOT NULL,
    response_body    TEXT NOT NULL,
    response_headers TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_api_history_created ON api_history(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_api_history_repo ON api_history(source_repo);

CREATE VIRTUAL TABLE IF NOT EXISTS api_history_fts USING fts5(
    request_text, url, response_body,
    content='api_history', content_rowid='id'
);

CREATE TRIGGER IF NOT EXISTS api_history_ai AFTER INSERT ON api_history BEGIN
    INSERT INTO api_history_fts(rowid, request_text, url, response_body)
    VALUES (new.id, new.request_text, new.url, new.response_body);
END;

CREATE TRIGGER IF NOT EXISTS api_history_ad AFTER DELETE ON api_history BEGIN
    INSERT INTO api_history_fts(api_history_fts, rowid, request_text, url, response_body)
    VALUES ('delete', old.id, old.request_text, old.url, old.response_body);
END;

CREATE TRIGGER IF NOT EXISTS api_history_au AFTER UPDATE ON api_history BEGIN
    INSERT INTO api_history_fts(api_history_fts, rowid, request_text, url, response_body)
    VALUES ('delete', old.id, old.request_text, old.url, old.response_body);
    INSERT INTO api_history_fts(rowid, request_text, url, response_body)
    VALUES (new.id, new.request_text, new.url, new.response_body);
END;

CREATE UNIQUE INDEX IF NOT EXISTS idx_api_history_natural_key
    ON api_history(source_repo, method, url, request_text);

CREATE TABLE IF NOT EXISTS collapsed_groups (group_name TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS ui_preferences (key TEXT PRIMARY KEY, value TEXT NOT NULL);
";

fn workspace_type_str(wt: WorkspaceType) -> &'static str {
    match wt {
        WorkspaceType::Worktree => "Worktree",
        WorkspaceType::Simple => "Simple",
        WorkspaceType::Project => "Project",
    }
}

fn parse_workspace_type(s: &str) -> WorkspaceType {
    match s {
        "Simple" => WorkspaceType::Simple,
        "Project" => WorkspaceType::Project,
        _ => WorkspaceType::Worktree,
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceEntry> {
    Ok(WorkspaceEntry {
        name: row.get(0)?,
        description: row.get(1)?,
        prompt: row.get(2)?,
        kanban_path: row.get(3)?,
        branch: row.get(4)?,
        worktree_path: PathBuf::from(row.get::<_, String>(5)?),
        source_repo: PathBuf::from(row.get::<_, String>(6)?),
        workspace_type: parse_workspace_type(&row.get::<_, String>(7)?),
        group: row.get(8)?,
        order: row.get(9)?,
    })
}

fn row_to_api_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<ApiHistoryEntry> {
    Ok(ApiHistoryEntry {
        id: Some(row.get(0)?),
        source_repo: row.get(1)?,
        created_at: row.get(2)?,
        request_text: row.get(3)?,
        method: row.get(4)?,
        url: row.get(5)?,
        status: row.get::<_, u32>(6)? as u16,
        elapsed_ms: row.get::<_, i64>(7)? as u128,
        response_body: row.get(8)?,
        response_headers: row.get(9)?,
    })
}

// Implement traits on Arc<SqliteStorage> so the same instance can be shared
// across Box<dyn WorkspaceStorage>, Box<dyn ApiHistoryStorage>, Box<dyn UiPrefsStorage>

impl WorkspaceStorage for Arc<SqliteStorage> {
    fn save_workspaces(&self, git_root: &Path, workspaces: &[WorkspaceInfo]) -> anyhow::Result<()> {
        let mut conn = self.conn.lock();
        let git_root_str = git_root.to_string_lossy();

        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM workspaces WHERE source_repo = ?1",
            [&*git_root_str],
        )?;

        for ws in workspaces.iter().filter(|ws| ws.source_repo == git_root) {
            tx.execute(
                "INSERT INTO workspaces (project_root, name, description, prompt, kanban_path, branch, worktree_path, source_repo, workspace_type, group_name, display_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                rusqlite::params![
                    git_root_str,
                    ws.name,
                    ws.description,
                    ws.prompt,
                    ws.kanban_path,
                    ws.branch,
                    ws.path.to_string_lossy(),
                    ws.source_repo.to_string_lossy(),
                    workspace_type_str(ws.workspace_type),
                    ws.group,
                    ws.order,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    fn load_workspaces(&self, git_root: &Path) -> anyhow::Result<Vec<WorkspaceEntry>> {
        let conn = self.conn.lock();
        let git_root_str = git_root.to_string_lossy();
        let mut stmt = conn.prepare(
            "SELECT name, description, prompt, kanban_path, branch, worktree_path, source_repo, workspace_type, group_name, display_order FROM workspaces WHERE source_repo = ?1 ORDER BY display_order",
        )?;

        let entries: Vec<WorkspaceEntry> = stmt
            .query_map([&*git_root_str], row_to_entry)?
            .filter_map(|r| r.ok())
            .filter(|e| e.worktree_path.exists())
            .collect();

        Ok(entries)
    }

    fn load_all_workspaces(&self) -> Vec<WorkspaceEntry> {
        let conn = self.conn.lock();
        let mut stmt = match conn.prepare(
            "SELECT name, description, prompt, kanban_path, branch, worktree_path, source_repo, workspace_type, group_name, display_order FROM workspaces ORDER BY display_order",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let mut seen = std::collections::HashSet::new();
        match stmt.query_map([], row_to_entry) {
            Ok(rows) => rows
                .filter_map(|r| r.ok())
                .filter(|e| e.worktree_path.exists())
                .filter(|e| seen.insert(e.worktree_path.clone()))
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}

impl ApiHistoryStorage for Arc<SqliteStorage> {
    fn save_api_entry(&self, entry: &ApiHistoryEntry) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO api_history (source_repo, request_text, method, url, status, elapsed_ms, response_body, response_headers)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(source_repo, method, url, request_text) DO UPDATE SET
                 status = excluded.status,
                 elapsed_ms = excluded.elapsed_ms,
                 response_body = excluded.response_body,
                 response_headers = excluded.response_headers,
                 created_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            rusqlite::params![
                entry.source_repo,
                entry.request_text,
                entry.method,
                entry.url,
                entry.status as u32,
                entry.elapsed_ms as i64,
                entry.response_body,
                entry.response_headers,
            ],
        )?;
        Ok(())
    }

    fn search_api_history(
        &self,
        source_repo: &Path,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<ApiHistoryEntry>> {
        let conn = self.conn.lock();
        let repo_str = source_repo.to_string_lossy();
        let mut stmt = conn.prepare(
            "SELECT h.id, h.source_repo, h.created_at, h.request_text, h.method, h.url, h.status, h.elapsed_ms, h.response_body, h.response_headers FROM api_history h JOIN api_history_fts f ON h.id = f.rowid WHERE h.source_repo = ?1 AND api_history_fts MATCH ?2 ORDER BY h.created_at DESC LIMIT ?3",
        )?;

        let entries = stmt
            .query_map(
                rusqlite::params![&*repo_str, query, limit as i64],
                row_to_api_entry,
            )?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entries)
    }

    fn load_recent_api_history(
        &self,
        source_repo: &Path,
        limit: usize,
    ) -> anyhow::Result<Vec<ApiHistoryEntry>> {
        let conn = self.conn.lock();
        let repo_str = source_repo.to_string_lossy();
        let mut stmt = conn.prepare(
            "SELECT id, source_repo, created_at, request_text, method, url, status, elapsed_ms, response_body, response_headers FROM api_history WHERE source_repo = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;

        let entries = stmt
            .query_map(
                rusqlite::params![&*repo_str, limit as i64],
                row_to_api_entry,
            )?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entries)
    }

    fn delete_api_entry(&self, id: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM api_history WHERE id = ?1", [id])?;
        Ok(())
    }
}

impl UiPrefsStorage for Arc<SqliteStorage> {
    fn get_collapsed_groups(&self) -> anyhow::Result<HashSet<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT group_name FROM collapsed_groups")?;
        let groups = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(groups)
    }

    fn set_collapsed_groups(&self, groups: &HashSet<String>) -> anyhow::Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM collapsed_groups", [])?;
        for name in groups {
            tx.execute(
                "INSERT INTO collapsed_groups (group_name) VALUES (?1)",
                [name],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn get_preference(&self, key: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock();
        let result = conn.query_row(
            "SELECT value FROM ui_preferences WHERE key = ?1",
            [key],
            |row| row.get(0),
        );
        match result {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn set_preference(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO ui_preferences (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }
}
