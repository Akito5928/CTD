"""DBアクセスを一元管理するリポジトリ。SQLは必ずパラメータバインディングを使う。"""

from __future__ import annotations

import sqlite3
import uuid
from contextlib import contextmanager
from dataclasses import dataclass, field
from typing import Callable, Iterator

from ..errors import CtdError
from ..utils.timeutil import now_utc_iso
from ..utils.validator import display_name, name_key, normalize_id_prefix, validate_name

FTS_MIN_LENGTH = 3  # trigramで検索できる最小文字数。未満は通常検索(LIKE)を使う
_LIKE_ESCAPE = "\\"


@dataclass
class Note:
    id: str
    title: str
    content: str
    description: str | None
    category_id: int | None
    category: str | None
    source_path: str | None
    content_hash: str | None
    created_at: str
    updated_at: str
    tags: list[str] = field(default_factory=list)


@dataclass
class CategoryInfo:
    id: int
    name: str
    created_at: str
    note_count: int


class Repository:
    def __init__(self, conn: sqlite3.Connection):
        self.conn = conn
        self.fts_enabled = (
            conn.execute(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'notes_fts'"
            ).fetchone()
            is not None
        )

    # ---- 接続・トランザクション ---------------------------------------------

    def __enter__(self) -> "Repository":
        return self

    def __exit__(self, *exc_info) -> None:
        self.close()

    def close(self) -> None:
        self.conn.close()

    @contextmanager
    def transaction(self) -> Iterator[None]:
        """途中で例外・中断があれば全てロールバックする。"""
        self.conn.execute("BEGIN")
        try:
            yield
        except BaseException:
            if self.conn.in_transaction:
                self.conn.execute("ROLLBACK")
            raise
        else:
            self.conn.execute("COMMIT")

    # ---- カテゴリ -----------------------------------------------------------

    def find_category(self, name: str) -> sqlite3.Row | None:
        return self.conn.execute(
            "SELECT id, name FROM categories WHERE name_key = ?", (name_key(name),)
        ).fetchone()

    def ensure_category(self, name: str) -> int:
        """正規化キーが同じカテゴリがあればそのIDを、無ければ新規作成してIDを返す。"""
        display = validate_name(name, "カテゴリ")
        row = self.find_category(display)
        if row is not None:
            return row["id"]
        cur = self.conn.execute(
            "INSERT INTO categories (name, name_key, created_at) VALUES (?, ?, ?)",
            (display, name_key(display), now_utc_iso()),
        )
        return int(cur.lastrowid)

    def list_categories(self) -> list[CategoryInfo]:
        rows = self.conn.execute(
            """
            SELECT c.id, c.name, c.created_at, COUNT(n.id) AS note_count
            FROM categories c
            LEFT JOIN notes n ON n.category_id = c.id
            GROUP BY c.id
            ORDER BY c.name_key, c.id
            """
        ).fetchall()
        return [CategoryInfo(r["id"], r["name"], r["created_at"], r["note_count"]) for r in rows]

    def add_category(self, name: str) -> str:
        display = validate_name(name, "カテゴリ")
        existing = self.find_category(display)
        if existing is not None:
            raise CtdError(f"カテゴリ「{existing['name']}」は既に存在します。")
        with self.transaction():
            self.ensure_category(display)
        return display

    def rename_category(self, old: str, new: str) -> str:
        new_display = validate_name(new, "カテゴリ")
        target = self.find_category(old)
        if target is None:
            raise CtdError(f"カテゴリが存在しません: {old.strip()}")
        clash = self.find_category(new_display)
        if clash is not None and clash["id"] != target["id"]:
            raise CtdError(f"カテゴリ「{clash['name']}」は既に存在します。")
        with self.transaction():
            self.conn.execute(
                "UPDATE categories SET name = ?, name_key = ? WHERE id = ?",
                (new_display, name_key(new_display), target["id"]),
            )
        return new_display

    # ---- タグ ---------------------------------------------------------------

    def ensure_tag(self, name: str) -> int:
        display = validate_name(name, "タグ")
        row = self.conn.execute(
            "SELECT id FROM tags WHERE name_key = ?", (name_key(display),)
        ).fetchone()
        if row is not None:
            return row["id"]
        cur = self.conn.execute(
            "INSERT INTO tags (name, name_key) VALUES (?, ?)", (display, name_key(display))
        )
        return int(cur.lastrowid)

    def _replace_note_tags(self, note_id: str, tags: list[str]) -> None:
        self.conn.execute("DELETE FROM note_tags WHERE note_id = ?", (note_id,))
        for tag in tags:
            self.conn.execute(
                "INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?, ?)",
                (note_id, self.ensure_tag(tag)),
            )

    # ---- メモ: 作成・更新・削除 ---------------------------------------------

    def create_note(
        self,
        *,
        title: str,
        content: str,
        description: str | None,
        category_name: str | None,
        tags: list[str],
        source_path: str | None,
        content_hash: str,
    ) -> str:
        note_id = str(uuid.uuid4())
        now = now_utc_iso()
        with self.transaction():
            category_id = self.ensure_category(category_name) if category_name else None
            self.conn.execute(
                """
                INSERT INTO notes
                    (id, title, content, description, category_id, source_path,
                     content_hash, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (note_id, title, content, description, category_id, source_path,
                 content_hash, now, now),
            )
            self._replace_note_tags(note_id, tags)
        return note_id

    def update_note(
        self,
        note_id: str,
        *,
        title: str,
        content: str,
        description: str | None,
        category_name: str | None,
        tags: list[str],
        source_path: str | None,
        content_hash: str | None,
    ) -> None:
        with self.transaction():
            category_id = self.ensure_category(category_name) if category_name else None
            cur = self.conn.execute(
                """
                UPDATE notes
                SET title = ?, content = ?, description = ?, category_id = ?,
                    source_path = ?, content_hash = ?, updated_at = ?
                WHERE id = ?
                """,
                (title, content, description, category_id, source_path,
                 content_hash, now_utc_iso(), note_id),
            )
            if cur.rowcount == 0:
                raise CtdError(f"該当するメモが見つかりません: {note_id}")
            self._replace_note_tags(note_id, tags)

    def delete_note(self, note_id: str) -> None:
        """メモとタグの紐付けを削除する。カテゴリ・タグ自体は削除しない。"""
        with self.transaction():
            cur = self.conn.execute("DELETE FROM notes WHERE id = ?", (note_id,))
            if cur.rowcount == 0:
                raise CtdError(f"該当するメモが見つかりません: {note_id}")

    # ---- メモ: 取得 ---------------------------------------------------------

    def resolve_id(self, id_text: str) -> str:
        """UUIDの前方一致(8文字以上)で完全なIDを特定する。"""
        prefix = normalize_id_prefix(id_text)
        rows = self.conn.execute(
            "SELECT id, title FROM notes WHERE id LIKE ? ORDER BY id LIMIT 11",
            (prefix + "%",),
        ).fetchall()
        if not rows:
            raise CtdError(f"該当するメモが見つかりません: {id_text.strip()}")
        if len(rows) > 1:
            lines = "\n".join(f"  {r['id']}  {r['title']}" for r in rows[:10])
            more = "\n  ..." if len(rows) > 10 else ""
            raise CtdError(
                f"IDが一意に特定できません。候補:\n{lines}{more}",
                hint="IDをもう少し長く指定してください。",
            )
        return rows[0]["id"]

    def get_note(self, note_id: str) -> Note:
        row = self.conn.execute(
            """
            SELECT n.*, c.name AS category_name
            FROM notes n LEFT JOIN categories c ON c.id = n.category_id
            WHERE n.id = ?
            """,
            (note_id,),
        ).fetchone()
        if row is None:
            raise CtdError(f"該当するメモが見つかりません: {note_id}")
        tag_rows = self.conn.execute(
            """
            SELECT t.name FROM tags t JOIN note_tags nt ON nt.tag_id = t.id
            WHERE nt.note_id = ? ORDER BY t.name_key
            """,
            (note_id,),
        ).fetchall()
        return Note(
            id=row["id"], title=row["title"], content=row["content"],
            description=row["description"], category_id=row["category_id"],
            category=row["category_name"], source_path=row["source_path"],
            content_hash=row["content_hash"], created_at=row["created_at"],
            updated_at=row["updated_at"], tags=[r["name"] for r in tag_rows],
        )

    def find_by_hash(self, content_hash: str, exclude_id: str | None = None) -> list[sqlite3.Row]:
        if exclude_id is None:
            return self.conn.execute(
                "SELECT id, title FROM notes WHERE content_hash = ? ORDER BY created_at, id",
                (content_hash,),
            ).fetchall()
        return self.conn.execute(
            "SELECT id, title FROM notes WHERE content_hash = ? AND id != ? ORDER BY created_at, id",
            (content_hash, exclude_id),
        ).fetchall()

    # ---- メモ: 一覧・検索 ---------------------------------------------------

    _SUMMARY_SQL = """
        SELECT n.id, n.title, c.name AS category, n.created_at
        FROM notes n LEFT JOIN categories c ON c.id = n.category_id
    """
    # 登録日時の降順。同一日時はIDで順序を安定させる。
    _ORDER_SQL = " ORDER BY n.created_at DESC, n.id ASC LIMIT ? OFFSET ?"

    def count_notes(self) -> int:
        return int(self.conn.execute("SELECT COUNT(*) FROM notes").fetchone()[0])

    def list_notes(self, limit: int | None, offset: int = 0) -> list[sqlite3.Row]:
        return self.conn.execute(
            self._SUMMARY_SQL + self._ORDER_SQL,
            (-1 if limit is None else limit, offset),
        ).fetchall()

    def _search_attempts(self, keyword: str) -> list[tuple[str, list[str]]]:
        """検索条件の候補を優先順に返す。FTS5(3文字以上)→通常検索(LIKE)。"""
        attempts: list[tuple[str, list[str]]] = []
        if self.fts_enabled and len(keyword) >= FTS_MIN_LENGTH:
            phrase = '"' + keyword.replace('"', '""') + '"'
            attempts.append(
                ("n.id IN (SELECT note_id FROM notes_fts WHERE notes_fts MATCH ?)", [phrase])
            )
        escaped = (
            keyword.replace(_LIKE_ESCAPE, _LIKE_ESCAPE * 2)
            .replace("%", _LIKE_ESCAPE + "%")
            .replace("_", _LIKE_ESCAPE + "_")
        )
        pattern = f"%{escaped}%"
        like = (
            f"(n.title LIKE ? ESCAPE '{_LIKE_ESCAPE}' "
            f"OR n.content LIKE ? ESCAPE '{_LIKE_ESCAPE}' "
            f"OR IFNULL(n.description, '') LIKE ? ESCAPE '{_LIKE_ESCAPE}')"
        )
        attempts.append((like, [pattern, pattern, pattern]))
        return attempts

    def _run_search(self, keyword: str, run: Callable[[str, list[str]], list]) -> list:
        attempts = self._search_attempts(keyword)
        for index, (clause, params) in enumerate(attempts):
            try:
                return run(clause, params)
            except sqlite3.OperationalError:
                if index == len(attempts) - 1:
                    raise
        return []  # pragma: no cover

    def count_search(self, keyword: str) -> int:
        def run(clause: str, params: list[str]) -> list:
            row = self.conn.execute(
                f"SELECT COUNT(*) FROM notes n WHERE {clause}", params
            ).fetchone()
            return [int(row[0])]

        return self._run_search(keyword, run)[0]

    def search_notes(self, keyword: str, limit: int | None, offset: int = 0) -> list[sqlite3.Row]:
        def run(clause: str, params: list[str]) -> list:
            return self.conn.execute(
                self._SUMMARY_SQL + f" WHERE {clause}" + self._ORDER_SQL,
                [*params, -1 if limit is None else limit, offset],
            ).fetchall()

        return self._run_search(keyword, run)
