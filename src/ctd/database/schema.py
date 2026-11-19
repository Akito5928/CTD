"""スキーマ定義。notes / categories / tags / note_tags と全文検索(FTS5 trigram)。"""

from __future__ import annotations

import sqlite3

from ..errors import CtdError

SCHEMA_VERSION = 1

_TABLES_AND_INDEXES = [
    """
    CREATE TABLE categories (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        name       TEXT NOT NULL,
        name_key   TEXT NOT NULL UNIQUE,
        created_at TEXT NOT NULL
    )
    """,
    """
    CREATE TABLE tags (
        id       INTEGER PRIMARY KEY AUTOINCREMENT,
        name     TEXT NOT NULL,
        name_key TEXT NOT NULL UNIQUE
    )
    """,
    """
    CREATE TABLE notes (
        id           TEXT PRIMARY KEY,
        title        TEXT NOT NULL,
        content      TEXT NOT NULL,
        description  TEXT,
        category_id  INTEGER REFERENCES categories(id) ON DELETE RESTRICT,
        source_path  TEXT,
        content_hash TEXT,
        created_at   TEXT NOT NULL,
        updated_at   TEXT NOT NULL
    )
    """,
    """
    CREATE TABLE note_tags (
        note_id TEXT    NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
        tag_id  INTEGER NOT NULL REFERENCES tags(id),
        PRIMARY KEY (note_id, tag_id)
    )
    """,
    "CREATE INDEX idx_notes_category_id  ON notes(category_id)",
    "CREATE INDEX idx_notes_created_at   ON notes(created_at)",
    "CREATE INDEX idx_notes_content_hash ON notes(content_hash)",
    "CREATE INDEX idx_note_tags_tag_id   ON note_tags(tag_id)",
]

# notesから独立したFTS5テーブル。note_idはUUID(UNINDEXED)で保持する。
_FTS_TABLE = """
    CREATE VIRTUAL TABLE notes_fts USING fts5(
        note_id UNINDEXED,
        title,
        content,
        description,
        tokenize = 'trigram'
    )
"""

_FTS_TRIGGERS = [
    """
    CREATE TRIGGER notes_fts_ai AFTER INSERT ON notes BEGIN
        INSERT INTO notes_fts (note_id, title, content, description)
        VALUES (new.id, new.title, new.content, IFNULL(new.description, ''));
    END
    """,
    """
    CREATE TRIGGER notes_fts_ad AFTER DELETE ON notes BEGIN
        DELETE FROM notes_fts WHERE note_id = old.id;
    END
    """,
    """
    CREATE TRIGGER notes_fts_au AFTER UPDATE OF title, content, description ON notes BEGIN
        DELETE FROM notes_fts WHERE note_id = old.id;
        INSERT INTO notes_fts (note_id, title, content, description)
        VALUES (new.id, new.title, new.content, IFNULL(new.description, ''));
    END
    """,
]


def create_schema(conn: sqlite3.Connection, *, fts: bool = True) -> bool:
    """スキーマを作成する。FTS5(trigram)が使えた場合Trueを返す。

    FTS5/trigramが使えない環境では全文検索テーブルを作らず、検索は通常検索(LIKE)で動作する。
    """
    conn.execute("BEGIN")
    try:
        for statement in _TABLES_AND_INDEXES:
            conn.execute(statement)
        fts_enabled = False
        if fts:
            try:
                conn.execute(_FTS_TABLE)
                fts_enabled = True
            except sqlite3.OperationalError:
                fts_enabled = False
        if fts_enabled:
            for statement in _FTS_TRIGGERS:
                conn.execute(statement)
        conn.execute(f"PRAGMA user_version = {SCHEMA_VERSION}")
        conn.execute("COMMIT")
    except BaseException:
        if conn.in_transaction:
            conn.execute("ROLLBACK")
        raise
    return fts_enabled


def verify_schema(conn: sqlite3.Connection) -> None:
    """DBのスキーマバージョンを確認する。"""
    try:
        version = conn.execute("PRAGMA user_version").fetchone()[0]
    except sqlite3.DatabaseError as e:
        raise CtdError(f"データベースファイルを開けません(破損している可能性があります): {e}")
    if version == 0:
        raise CtdError(
            "データベースが初期化されていません。",
            hint="`ctd init` で初期化するか、設定ファイルのdatabase_pathを確認してください。",
        )
    if version > SCHEMA_VERSION:
        raise CtdError(
            f"このデータベースは新しいバージョンのctdで作成されています(スキーマ{version})。",
            hint="ctdを最新版に更新してください。",
        )
