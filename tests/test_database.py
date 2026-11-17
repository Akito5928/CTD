import sqlite3
import uuid

import pytest

from ctd.database import backup as backup_module
from ctd.database import repository as repository_module
from ctd.database.backup import create_backup
from ctd.database.connection import connect
from ctd.database.repository import Repository
from ctd.database.schema import SCHEMA_VERSION, create_schema, verify_schema
from ctd.errors import CtdError, UsageError


def make_repo(path, fts=True):
    conn = connect(path, must_exist=False)
    create_schema(conn, fts=fts)
    return Repository(conn)


@pytest.fixture
def repo(tmp_path):
    r = make_repo(tmp_path / "t.db")
    yield r
    r.close()


@pytest.fixture
def repo_nofts(tmp_path):
    r = make_repo(tmp_path / "nofts.db", fts=False)
    yield r
    r.close()


def add(repo, title="title", content="body text", description=None, category=None,
        tags=(), content_hash=None):
    return repo.create_note(
        title=title, content=content, description=description, category_name=category,
        tags=list(tags), source_path=None, content_hash=content_hash or uuid.uuid4().hex,
    )


def count(repo, table):
    return repo.conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]


# ---- スキーマ -------------------------------------------------------------

def test_schema_has_required_tables_indexes_and_version(repo):
    names = {r[0] for r in repo.conn.execute("SELECT name FROM sqlite_master WHERE type='table'")}
    assert {"notes", "categories", "tags", "note_tags", "notes_fts"} <= names
    indexes = {r[0] for r in repo.conn.execute("SELECT name FROM sqlite_master WHERE type='index'")}
    assert {"idx_notes_category_id", "idx_notes_created_at",
            "idx_notes_content_hash", "idx_note_tags_tag_id"} <= indexes
    assert repo.conn.execute("PRAGMA user_version").fetchone()[0] == SCHEMA_VERSION == 1
    assert repo.conn.execute("PRAGMA foreign_keys").fetchone()[0] == 1
    assert repo.fts_enabled is True


def test_schema_without_fts_still_works(repo_nofts):
    assert repo_nofts.fts_enabled is False
    add(repo_nofts, title="日本語のメモ", content="SQLiteの全文検索")
    assert repo_nofts.count_search("全文検索") == 1  # 3文字以上でもLIKEで検索できる


def test_verify_schema_rejects_uninitialized_and_newer_versions(tmp_path):
    empty = connect(tmp_path / "empty.db", must_exist=False)
    with pytest.raises(CtdError, match="初期化されていません"):
        verify_schema(empty)
    empty.execute("PRAGMA user_version = 99")
    with pytest.raises(CtdError, match="新しいバージョン"):
        verify_schema(empty)
    empty.close()


def test_garbage_file_is_reported_as_error(tmp_path):
    path = tmp_path / "garbage.db"
    path.write_bytes(b"this is not a sqlite database" * 100)
    with pytest.raises(CtdError):
        verify_schema(connect(path))


def test_connect_requires_existing_db_and_guides_to_init(tmp_path):
    with pytest.raises(CtdError) as info:
        connect(tmp_path / "missing.db")
    assert "ctd init" in info.value.hint


# ---- メモ・ID -------------------------------------------------------------

def test_note_roundtrip_with_uuid4_and_utc_timestamps(repo):
    note_id = add(repo, title="T", content="C\n", description="D", category="技術", tags=["A", "b"])
    assert uuid.UUID(note_id).version == 4
    note = repo.get_note(note_id)
    assert (note.title, note.content, note.description, note.category) == ("T", "C\n", "D", "技術")
    assert note.tags == ["A", "b"]
    assert note.created_at.endswith("Z") and note.updated_at.endswith("Z")


def test_resolve_id_by_prefix(repo):
    note_id = add(repo)
    assert repo.resolve_id(note_id) == note_id
    assert repo.resolve_id(note_id[:8]) == note_id
    assert repo.resolve_id(note_id[:8].upper()) == note_id
    with pytest.raises(UsageError):
        repo.resolve_id(note_id[:7])
    with pytest.raises(CtdError, match="見つかりません"):
        repo.resolve_id("00000000")


def test_resolve_id_ambiguous_lists_candidates(repo):
    for suffix in ("1", "2"):
        repo.conn.execute(
            "INSERT INTO notes (id, title, content, created_at, updated_at) VALUES (?, ?, 'x', 'z', 'z')",
            (f"aaaaaaaa-0000-4000-8000-00000000000{suffix}", f"候補{suffix}"),
        )
    with pytest.raises(CtdError) as info:
        repo.resolve_id("aaaaaaaa")
    assert "候補1" in info.value.message and "候補2" in info.value.message
    assert repo.resolve_id("aaaaaaaa-0000-4000-8000-000000000002").endswith("2")


def test_find_by_hash_with_exclude(repo):
    a = add(repo, title="A", content_hash="same")
    b = add(repo, title="B", content_hash="same")
    assert {r["id"] for r in repo.find_by_hash("same")} == {a, b}
    assert [r["id"] for r in repo.find_by_hash("same", exclude_id=a)] == [b]
    assert repo.find_by_hash("other") == []


# ---- 検索 -----------------------------------------------------------------

def test_search_japanese_with_fts_and_like_fallback(repo):
    add(repo, title="SQLite入門", content="SQLiteは軽量なデータベースです。日本語検索も使えます。")
    assert repo.count_search("データベース") == 1  # 3文字以上: FTS5 trigram
    assert repo.count_search("日本語検索") == 1
    assert repo.count_search("軽量") == 1  # 3文字未満: LIKE
    assert repo.count_search("DB") == 0
    assert repo.count_search("sqlite") == 1  # 大文字小文字を区別しない
    assert repo.count_search("存在しない言葉") == 0


def test_search_covers_title_content_and_description(repo):
    add(repo, title="タイトルだけ語", content="x", description=None)
    add(repo, title="y", content="本文だけ語", description=None)
    add(repo, title="z", content="x", description="説明だけ語")
    for word in ("タイトルだけ語", "本文だけ語", "説明だけ語"):
        assert repo.count_search(word) == 1, word
    for partial in ("タイトル", "本文だ", "説明だ"):
        assert repo.count_search(partial) == 1, partial


def test_search_handles_special_characters(repo):
    add(repo, title="a", content='He said "hello" to 100% of us_all')
    assert repo.count_search('"hello"') == 1  # FTS: 引用符
    assert repo.count_search("100%") == 1
    assert repo.count_search("%") == 1  # LIKE: ワイルドカードは文字として扱う
    assert repo.count_search("_") == 1
    assert repo.count_search("us_all") == 1
    assert repo.count_search("\\") == 0


def test_fts_stays_in_sync_on_update_and_delete(repo):
    note_id = add(repo, title="旧タイトル", content="古い本文のテキスト")
    assert repo.count_search("古い本文") == 1
    repo.update_note(note_id, title="新タイトル", content="新しい本文のテキスト", description=None,
                     category_name=None, tags=[], source_path=None, content_hash="h2")
    assert repo.count_search("古い本文") == 0
    assert repo.count_search("新しい本文") == 1
    repo.delete_note(note_id)
    assert repo.count_search("新しい本文") == 0
    assert count(repo, "notes_fts") == 0


def test_fts_trigger_fires_for_single_column_updates(repo):
    # update_note以外の経路(1列だけのUPDATE)でも検索インデックスが同期される
    note_id = add(repo, title="元タイトル語", content="元の本文テキスト")
    repo.conn.execute("UPDATE notes SET content = ? WHERE id = ?", ("差し替え本文テキスト", note_id))
    assert (repo.count_search("元の本文"), repo.count_search("差し替え本文")) == (0, 1)
    repo.conn.execute("UPDATE notes SET title = ? WHERE id = ?", ("新タイトル語", note_id))
    assert (repo.count_search("元タイトル"), repo.count_search("新タイトル")) == (0, 1)
    repo.conn.execute("UPDATE notes SET description = ? WHERE id = ?", ("追加説明テキスト", note_id))
    assert repo.count_search("追加説明") == 1


def test_search_results_are_newest_first_with_id_tiebreak(repo, monkeypatch):
    stamps = iter(["2026-01-01T00:00:00.000Z", "2026-01-01T00:00:00.000Z", "2026-02-01T00:00:00.000Z"])
    monkeypatch.setattr(repository_module, "now_utc_iso", lambda: next(stamps))
    first = add(repo, title="古い1", content="共通ワード")
    second = add(repo, title="古い2", content="共通ワード")
    newest = add(repo, title="新しい", content="共通ワード")
    expected = [newest] + sorted([first, second])
    assert [r["id"] for r in repo.search_notes("共通ワード", None)] == expected
    assert [r["id"] for r in repo.list_notes(None)] == expected
    assert [r["id"] for r in repo.list_notes(2, 1)] == expected[1:3]


# ---- カテゴリ・タグ -------------------------------------------------------

def test_category_names_are_normalized_but_keep_first_spelling(repo):
    first = repo.ensure_category("Python")
    assert repo.ensure_category("ｐｙｔｈｏｎ ") == first
    assert [c.name for c in repo.list_categories()] == ["Python"]
    with pytest.raises(CtdError, match="既に存在"):
        repo.add_category("PYTHON")


def test_rename_category_rules(repo):
    note_id = add(repo, category="旧名")
    repo.add_category("別名")
    repo.rename_category("旧名", "新名")
    assert repo.get_note(note_id).category == "新名"  # 紐付けは維持
    repo.rename_category("新名", "ＳＥＩＭＥＩ")  # 表記だけの変更は許可
    assert repo.find_category("seimei")["name"] == "ＳＥＩＭＥＩ"
    with pytest.raises(CtdError, match="既に存在"):
        repo.rename_category("ＳＥＩＭＥＩ", "別名")
    with pytest.raises(CtdError, match="存在しません"):
        repo.rename_category("無い", "何か")


def test_tags_are_reused_by_normalized_key(repo):
    add(repo, tags=["Python", "SQL"])
    add(repo, tags=["python", "ｓｑｌ", "new"])
    names = [r[0] for r in repo.conn.execute("SELECT name FROM tags ORDER BY id")]
    assert names == ["Python", "SQL", "new"]


def test_deleting_note_keeps_categories_and_tags_but_removes_links(repo):
    note_id = add(repo, category="技術", tags=["a", "b"])
    repo.delete_note(note_id)
    assert count(repo, "notes") == 0
    assert count(repo, "note_tags") == 0
    assert count(repo, "categories") == 1
    assert count(repo, "tags") == 2


def test_foreign_keys_are_enforced(repo):
    note_id = add(repo, category="技術")
    with pytest.raises(sqlite3.IntegrityError):
        repo.conn.execute("DELETE FROM categories")  # ON DELETE RESTRICT
    with pytest.raises(sqlite3.IntegrityError):
        repo.conn.execute(
            "INSERT INTO notes (id, title, content, category_id, created_at, updated_at) "
            "VALUES ('x', 't', 'c', 999, 'z', 'z')"
        )
    assert repo.get_note(note_id).category == "技術"


def test_transaction_rolls_back_everything_on_failure(repo):
    # 新規カテゴリ作成 → メモ挿入のあとでタグ名が不正で失敗 → 全て元に戻る
    with pytest.raises(CtdError):
        add(repo, category="新カテゴリ", tags=["ok", "   "])
    assert (count(repo, "notes"), count(repo, "categories"), count(repo, "tags")) == (0, 0, 0)
    assert count(repo, "notes_fts") == 0
    assert repo.conn.in_transaction is False


# ---- バックアップ ---------------------------------------------------------

def test_backup_is_a_valid_copy_and_never_overwrites(repo, tmp_path, monkeypatch):
    add(repo, title="バックアップ対象", content="本文")
    monkeypatch.setattr(backup_module, "backup_timestamp", lambda now=None: "20261115_120000")
    folder = tmp_path / "bk"
    first = create_backup(repo.conn, folder)
    second = create_backup(repo.conn, folder)
    assert first.name == "ctd_backup_20261115_120000.db"
    assert second.name == "ctd_backup_20261115_120000_1.db"
    assert sorted(p.name for p in folder.iterdir()) == [first.name, second.name]  # .part が残らない
    copy = sqlite3.connect(str(first))
    assert copy.execute("SELECT title FROM notes").fetchone()[0] == "バックアップ対象"
    assert copy.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
    copy.close()


def test_backup_failure_is_reported(repo, tmp_path):
    blocker = tmp_path / "not_a_folder"
    blocker.write_text("file")
    with pytest.raises(CtdError, match="バックアップ"):
        create_backup(repo.conn, blocker)
