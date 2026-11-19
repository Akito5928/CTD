//! SQLite database: schema, inspection and helpers.

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::path::Path;

pub const SCHEMA_VERSION: i64 = 1;

const SCHEMA_SQL: &str = "
CREATE TABLE files (
    id INTEGER PRIMARY KEY,
    relative_path TEXT NOT NULL UNIQUE,
    file_size INTEGER NOT NULL,
    modified_time_ns INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    content TEXT NOT NULL
);
CREATE VIRTUAL TABLE files_fts USING fts5(
    relative_path, content,
    content='files', content_rowid='id',
    tokenize='trigram'
);
CREATE TRIGGER files_ai AFTER INSERT ON files BEGIN
    INSERT INTO files_fts(rowid, relative_path, content)
    VALUES (new.id, new.relative_path, new.content);
END;
CREATE TRIGGER files_ad AFTER DELETE ON files BEGIN
    INSERT INTO files_fts(files_fts, rowid, relative_path, content)
    VALUES ('delete', old.id, old.relative_path, old.content);
END;
CREATE TRIGGER files_au AFTER UPDATE OF relative_path, content ON files BEGIN
    INSERT INTO files_fts(files_fts, rowid, relative_path, content)
    VALUES ('delete', old.id, old.relative_path, old.content);
    INSERT INTO files_fts(rowid, relative_path, content)
    VALUES (new.id, new.relative_path, new.content);
END;
PRAGMA user_version = 1;
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexState {
    Missing,
    Invalid(String),
    Unsupported(i64),
    Ready,
}

impl IndexState {
    pub fn label(&self) -> &'static str {
        match self {
            IndexState::Missing => "Missing",
            IndexState::Invalid(_) => "Invalid",
            IndexState::Unsupported(_) => "Unsupported version",
            IndexState::Ready => "Ready",
        }
    }
}

pub fn create_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA_SQL)
}

/// Create a brand-new database file with the v1 schema.
pub fn create_new(path: &Path) -> Result<()> {
    let conn = Connection::open(path)
        .with_context(|| format!("cannot create database {}", path.display()))?;
    create_schema(&conn).context("cannot initialize the database schema")?;
    Ok(())
}

fn open_rw(path: &Path) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
}

/// Open an existing database for reading and writing.
pub fn open(path: &Path) -> Result<Connection> {
    let conn = open_rw(path).with_context(|| format!("cannot open {}", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch("PRAGMA cache_size = -65536; PRAGMA temp_store = MEMORY;")?;
    Ok(conn)
}

/// Examine the index database without modifying it.
pub fn inspect(path: &Path) -> IndexState {
    match path.try_exists() {
        Ok(true) => {}
        Ok(false) => return IndexState::Missing,
        Err(e) => return IndexState::Invalid(e.to_string()),
    }
    let conn = match open_rw(path).or_else(|_| {
        Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
    }) {
        Ok(c) => c,
        Err(e) => return IndexState::Invalid(e.to_string()),
    };
    let version: i64 = match conn.query_row("PRAGMA user_version", [], |r| r.get(0)) {
        Ok(v) => v,
        Err(e) => return IndexState::Invalid(e.to_string()),
    };
    if version > SCHEMA_VERSION {
        return IndexState::Unsupported(version);
    }
    if version != SCHEMA_VERSION {
        return IndexState::Invalid(format!("unexpected schema version {version}"));
    }
    let tables: rusqlite::Result<i64> = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('files','files_fts')",
        [],
        |r| r.get(0),
    );
    match tables {
        Ok(2) => {}
        Ok(_) => return IndexState::Invalid("required tables are missing".to_string()),
        Err(e) => return IndexState::Invalid(e.to_string()),
    }
    if let Err(e) = conn.query_row("SELECT count(*) FROM files", [], |r| r.get::<_, i64>(0)) {
        return IndexState::Invalid(e.to_string());
    }
    IndexState::Ready
}

pub fn count_files(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT count(*) FROM files", [], |r| r.get(0))?)
}

#[derive(Debug, Clone)]
pub struct IndexedMeta {
    pub size: i64,
    pub mtime_ns: i64,
    pub hash: String,
}

pub fn load_meta(conn: &Connection) -> Result<HashMap<String, IndexedMeta>> {
    let mut stmt =
        conn.prepare("SELECT relative_path, file_size, modified_time_ns, content_hash FROM files")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            IndexedMeta {
                size: r.get(1)?,
                mtime_ns: r.get(2)?,
                hash: r.get(3)?,
            },
        ))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (p, m) = row?;
        map.insert(p, m);
    }
    Ok(map)
}
