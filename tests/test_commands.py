import hashlib
import json
import sqlite3
import uuid

import pytest

from ctd.commands import init as init_module
from ctd.commands import list_notes as list_module
from ctd.database import repository as repository_module
from ctd.database.repository import Repository
from ctd.utils import console


def rows(app, sql, params=()):
    return app.query(sql, params)


# ---- 基本・ヘルプ・終了コード ---------------------------------------------

def test_version_help_and_no_arguments(app_uninit):
    assert app_uninit.run("version").out.strip() == "ctd 1.0"
    helped = app_uninit.run("help")
    assert helped.code == 0
    for command in ("init", "register", "search", "list", "show", "edit", "delete", "category", "backup"):
        assert command in helped.out
    assert app_uninit.run().code == 0  # コマンド省略はヘルプ表示


def test_invalid_usage_returns_2_with_usage_text(app_uninit):
    unknown = app_uninit.run("frobnicate")
    assert unknown.code == 2 and "エラー" in unknown.err and "usage" in unknown.err
    assert app_uninit.run("register").code == 2  # 引数不足
    assert app_uninit.run("category").code == 2
    assert "list / add / rename" in app_uninit.run("category").err


def test_commands_require_init_and_create_nothing(app_uninit):
    for argv in (["list"], ["search", "abc"], ["show", "abcdef12"], ["backup"], ["category", "list"]):
        result = app_uninit.run(*argv)
        assert result.code == 1, argv
        assert "ctd init" in result.err, argv
    assert list(app_uninit.base_dir.iterdir()) == []


def test_database_missing_after_init_guides_to_init(app):
    app.db_path.unlink()
    result = app.run("list")
    assert result.code == 1 and "ctd init" in result.err


# ---- init -----------------------------------------------------------------

def test_init_creates_layout_and_is_idempotent(app_uninit):
    base = app_uninit.base_dir
    first = app_uninit.run("init")
    assert first.code == 0
    for relative in ("config/config.json", "database/ctd.db", "backup", "txt"):
        assert (base / relative).exists(), relative
    assert json.loads((base / "config/config.json").read_text(encoding="utf-8")) == {
        "database_path": "database/ctd.db", "backup_dir": "backup",
    }

    db_bytes = (base / "database/ctd.db").read_bytes()
    second = app_uninit.run("init")
    assert second.code == 0 and "既に初期化済み" in second.out
    assert (base / "database/ctd.db").read_bytes() == db_bytes


def test_init_never_overwrites_existing_config_or_database(app):
    config_path = app.base_dir / "config/config.json"
    custom = '{"database_path": "database/ctd.db", "backup_dir": "my_backups"}\n'
    config_path.write_text(custom, encoding="utf-8")
    app.register("a.txt", "既存データ")
    before = app.db_path.read_bytes()

    result = app.run("init")
    assert result.code == 0
    assert config_path.read_text(encoding="utf-8") == custom
    assert app.db_path.read_bytes() == before
    assert (app.base_dir / "my_backups").is_dir()


def test_init_resolves_relative_paths_from_base_dir(app_uninit):
    base = app_uninit.base_dir
    (base / "config").mkdir()
    (base / "config/config.json").write_text(
        '{"database_path": "data/my.db", "backup_dir": "bk"}', encoding="utf-8"
    )
    assert app_uninit.run("init").code == 0
    assert (base / "data/my.db").is_file() and (base / "bk").is_dir()
    assert app_uninit.run("list").code == 0


def test_broken_config_is_reported(app):
    (app.base_dir / "config/config.json").write_text("{ broken", encoding="utf-8")
    result = app.run("list")
    assert result.code == 1 and "設定ファイル" in result.err
    assert app.run("init").code == 1  # initも壊れた設定を上書きしない
    assert (app.base_dir / "config/config.json").read_text(encoding="utf-8") == "{ broken"


def test_init_failure_leaves_no_half_made_database(app_uninit, monkeypatch):
    def boom(conn, **kwargs):
        raise sqlite3.OperationalError("disk I/O error")

    monkeypatch.setattr(init_module, "create_schema", boom)
    result = app_uninit.run("init")
    assert result.code == 1
    assert not (app_uninit.base_dir / "database/ctd.db").exists()


# ---- register -------------------------------------------------------------

def test_register_full_flow_stores_everything(app):
    original = "SQLiteメモ\r\n2行目です\r\n".encode("utf-8")
    path = app.txt("memo.txt", b"\xef\xbb\xbf" + original)
    result = app.run("register", path, inputs=["最初のメモ", "n", "技術", "SQL, Python，sql", "説明です", "y"])
    assert result.code == 0, result.err
    assert "登録しました" in result.out

    (note_id, title, content, description, category, source, digest, created, updated), = rows(
        app,
        "SELECT n.id, n.title, n.content, n.description, c.name, n.source_path, "
        "n.content_hash, n.created_at, n.updated_at "
        "FROM notes n LEFT JOIN categories c ON c.id = n.category_id",
    )
    assert uuid.UUID(note_id).version == 4 and note_id in result.out
    assert (title, description, category) == ("最初のメモ", "説明です", "技術")
    assert content == "SQLiteメモ\n2行目です\n"  # BOM除去・LF正規化
    assert digest == hashlib.sha256(content.encode("utf-8")).hexdigest()
    assert source.endswith("memo.txt") and created.endswith("Z") and created == updated
    tags = rows(app, "SELECT t.name FROM tags t JOIN note_tags nt ON nt.tag_id = t.id ORDER BY t.name_key")
    assert [t[0] for t in tags] == ["Python", "SQL"]  # 「sql」は「SQL」と同一タグとして重複登録されない
    assert (app.base_dir / "txt/memo.txt").read_bytes() == b"\xef\xbb\xbf" + original  # 元ファイルは不変


def test_register_defaults_title_from_filename_and_allows_no_category(app):
    path = app.txt("my idea.txt", "本文")
    result = app.run("register", path, inputs=["", "", "", "", "y"])
    assert result.code == 0
    assert rows(app, "SELECT title, category_id, description FROM notes") == [("my idea", None, None)]
    assert app.scalar("SELECT COUNT(*) FROM note_tags") == 0


def test_register_cancel_leaves_database_untouched(app):
    path = app.txt("a.txt", "本文")
    declined = app.run("register", path, inputs=["t", "n", "新カテゴリ", "新タグ", "d", "n"])
    assert declined.code == 0 and "キャンセル" in declined.out
    interrupted = app.run("register", path, inputs=["t", KeyboardInterrupt])
    assert interrupted.code == 0
    eof = app.run("register", path, inputs=["t"])  # 入力が尽きる(EOF)
    assert eof.code == 0
    for table in ("notes", "categories", "tags", "note_tags", "notes_fts"):
        assert app.scalar(f"SELECT COUNT(*) FROM {table}") == 0, table


def test_register_rejects_bad_files_without_touching_db(app):
    cases = {
        "missing": (str(app.base_dir / "txt/none.txt"), "見つかりません"),
        "markdown": (app.txt("a.md", "x"), "TXT"),
        "empty": (app.txt("empty.txt", ""), "空"),
        "blank": (app.txt("blank.txt", "  \n\n"), "空"),
        "sjis": (app.txt("sjis.txt", "日本語です".encode("shift_jis")), "UTF-8"),
    }
    for name, (path, expected) in cases.items():
        result = app.run("register", path, inputs=["t", "", "", "", "y"])
        assert result.code == 1, name
        assert expected in result.err, name
    assert app.scalar("SELECT COUNT(*) FROM notes") == 0


def test_register_warns_on_duplicate_content_even_with_different_newlines(app):
    app.register("a.txt", "同じ内容\n二行目\n")
    other = app.txt("b.txt", "同じ内容\r\n二行目\r\n")  # CRLFでも同一内容として扱う

    declined = app.run("register", other, inputs=["n"])
    assert "同じ内容のメモが既に登録されています" in declined.out
    assert declined.code == 0 and app.scalar("SELECT COUNT(*) FROM notes") == 1

    accepted = app.run("register", other, inputs=["y", "B", "", "", "", "y"])
    assert accepted.code == 0 and app.scalar("SELECT COUNT(*) FROM notes") == 2


def test_register_selects_existing_category_and_reuses_by_normalized_name(app):
    app.run("category", "add", "Python")
    path = app.txt("a.txt", "one")
    assert app.run("register", path, inputs=["A", "1", "", "", "y"]).code == 0  # 番号で選択
    path2 = app.txt("b.txt", "two")
    result = app.run("register", path2, inputs=["B", "n", "ＰＹＴＨＯＮ", "", "", "y"])  # 全角・大文字で再入力
    assert result.code == 0 and "既存のカテゴリ" in result.out
    assert rows(app, "SELECT name FROM categories") == [("Python",)]
    assert app.scalar("SELECT COUNT(*) FROM notes WHERE category_id IS NOT NULL") == 2


def test_register_retries_invalid_category_choice(app):
    path = app.txt("a.txt", "x")
    result = app.run("register", path, inputs=["", "9", "abc", "n", "   ", "n", "OK", "", "", "y"])
    assert result.code == 0
    assert rows(app, "SELECT c.name FROM notes n JOIN categories c ON c.id = n.category_id") == [("OK",)]


def test_tags_are_shared_between_notes_with_first_spelling_kept(app):
    app.register("a.txt", "one", tags="Python, SQL")
    app.register("b.txt", "two", tags="python, sql, new")
    names = [r[0] for r in rows(app, "SELECT name FROM tags ORDER BY id")]
    assert names == ["Python", "SQL", "new"]


def test_register_is_atomic_when_saving_fails(app, monkeypatch):
    def boom(self, note_id, tags):
        raise sqlite3.OperationalError("disk I/O error")

    monkeypatch.setattr(Repository, "_replace_note_tags", boom)
    path = app.txt("a.txt", "x")
    result = app.run("register", path, inputs=["t", "n", "新カテゴリ", "tag", "", "y"])
    assert result.code == 1 and "データベース" in result.err
    for table in ("notes", "categories", "tags"):
        assert app.scalar(f"SELECT COUNT(*) FROM {table}") == 0, table


# ---- search / list / show --------------------------------------------------

def test_search_japanese_and_short_keywords(app):
    app.register("a.txt", "SQLiteは軽量なデータベースです。\n日本語の検索もできます。", title="データベース入門")
    app.register("b.txt", "全く別の話題です", title="別メモ")

    long_hit = app.run("search", "データベース")
    assert long_hit.code == 0 and "1件見つかりました" in long_hit.out and "データベース入門" in long_hit.out
    short_hit = app.run("search", "軽量")  # 3文字未満は通常検索
    assert "データベース入門" in short_hit.out
    assert "別メモ" not in app.run("search", "sqlite").out
    miss = app.run("search", "存在しない言葉")
    assert miss.code == 0 and "見つかりませんでした" in miss.out
    assert app.run("search", "   ").code == 2


def test_list_shows_newest_first_and_survives_restart(app):
    first = app.register("a.txt", "one", title="古いメモ")
    second = app.register("b.txt", "two", title="新しいメモ")
    # 各app.run()はDBを開き直して実行される(=再起動後もデータが保持されている)
    conn = sqlite3.connect(str(app.db_path))
    conn.execute("UPDATE notes SET created_at = '2026-01-01T00:00:00.000Z' WHERE id = ?", (first,))
    conn.execute("UPDATE notes SET created_at = '2026-06-01T00:00:00.000Z' WHERE id = ?", (second,))
    conn.commit()
    conn.close()
    result = app.run("list")
    assert result.code == 0 and "全2件" in result.out
    assert result.out.index("新しいメモ") < result.out.index("古いメモ")
    assert second[:8] in result.out and second not in result.out  # 一覧は先頭8文字


def test_list_when_empty(app):
    result = app.run("list")
    assert result.code == 0 and "登録されているメモはありません" in result.out


def _insert_many(app, n):
    conn = sqlite3.connect(str(app.db_path))
    for i in range(1, n + 1):
        conn.execute(
            "INSERT INTO notes (id, title, content, created_at, updated_at) VALUES (?, ?, 'x', ?, ?)",
            (str(uuid.uuid4()), f"memo-{i:03d}", f"2026-01-01T00:{i // 60:02d}:{i % 60:02d}.000Z",
             f"2026-01-01T00:{i // 60:02d}:{i % 60:02d}.000Z"),
        )
    conn.commit()
    conn.close()


def test_list_prints_everything_when_output_is_not_a_tty(app):
    _insert_many(app, 45)
    result = app.run("list")
    assert all(f"memo-{i:03d}" in result.out for i in range(1, 46))
    assert "[Enter" not in result.out


def test_list_paginates_20_per_page_on_a_tty(app, monkeypatch):
    _insert_many(app, 45)
    monkeypatch.setattr(console, "is_tty_output", lambda: True)
    result = app.run("list", inputs=["", "q"])  # Enterで次ページ、qで終了
    assert result.code == 0
    shown = [i for i in range(1, 46) if f"memo-{i:03d}" in result.out]
    assert shown == list(range(6, 46))  # 新しい順に20+20件。古い5件は未表示
    assert any("-- 20/45件を表示" in text for text in app.prompts)
    assert any("-- 40/45件を表示" in text for text in app.prompts)


def test_list_pagination_ends_quietly_on_eof(app, monkeypatch):
    _insert_many(app, 25)
    monkeypatch.setattr(console, "is_tty_output", lambda: True)
    result = app.run("list")  # 入力なし(EOF)
    assert result.code == 0 and "キャンセルしました" not in result.out


def test_show_displays_all_fields_and_accepts_id_prefix(app):
    note_id = app.register(
        "src.txt", "1行目\n2行目\n", title="詳細テスト", tags="a, b", desc="説明文"
    )
    result = app.run("show", note_id[:8])
    assert result.code == 0
    for expected in (note_id, "詳細テスト", "a, b", "説明文", "src.txt", "1行目\n2行目\n", "登録日時", "更新日時"):
        assert expected in result.out, expected
    assert app.run("show", note_id).code == 0  # フルUUIDも可


def test_show_error_cases(app):
    app.register("a.txt", "x")
    assert app.run("show", "abc").code == 2  # 8文字未満
    missing = app.run("show", "00000000")
    assert missing.code == 1 and "見つかりません" in missing.err


def test_show_ambiguous_prefix_lists_candidates(app):
    conn = sqlite3.connect(str(app.db_path))
    for suffix in "12":
        conn.execute(
            "INSERT INTO notes (id, title, content, created_at, updated_at) VALUES (?, ?, 'x', 'z', 'z')",
            (f"bbbbbbbb-0000-4000-8000-00000000000{suffix}", f"同じ接頭辞{suffix}"),
        )
    conn.commit()
    conn.close()
    result = app.run("show", "bbbbbbbb")
    assert result.code == 1 and "同じ接頭辞1" in result.err and "同じ接頭辞2" in result.err


# ---- edit -----------------------------------------------------------------

def test_edit_with_no_changes_does_nothing(app):
    note_id = app.register("a.txt", "本文", title="T", tags="x")
    before = app.query("SELECT * FROM notes")
    result = app.run("edit", note_id[:8], inputs=["", "", "", "", ""])
    assert result.code == 0 and "変更はありません" in result.out
    assert app.query("SELECT * FROM notes") == before


def test_edit_updates_fields_replaces_tags_and_bumps_updated_at(app, monkeypatch):
    note_id = app.register("a.txt", "本文", title="旧", tags="x, y", desc="旧説明")
    monkeypatch.setattr(repository_module, "now_utc_iso", lambda: "2030-01-01T00:00:00.000Z")
    result = app.run("edit", note_id, inputs=["新タイトル", "n", "新カテゴリ", "z", "新説明", "", "y"])
    assert result.code == 0, result.err
    (title, description, category, created, updated), = rows(
        app,
        "SELECT n.title, n.description, c.name, n.created_at, n.updated_at "
        "FROM notes n JOIN categories c ON c.id = n.category_id",
    )
    assert (title, description, category) == ("新タイトル", "新説明", "新カテゴリ")
    assert updated == "2030-01-01T00:00:00.000Z" and created != updated
    assert [r[0] for r in rows(app, "SELECT t.name FROM tags t JOIN note_tags nt ON nt.tag_id = t.id")] == ["z"]


def test_edit_can_clear_category_tags_and_description(app):
    path = app.txt("a.txt", "本文")
    app.run("register", path, inputs=["T", "n", "技術", "x", "説明", "y"])
    note_id = app.scalar("SELECT id FROM notes")
    result = app.run("edit", note_id, inputs=["", "0", "-", "-", "", "y"])
    assert result.code == 0
    assert rows(app, "SELECT category_id, description FROM notes") == [(None, None)]
    assert app.scalar("SELECT COUNT(*) FROM note_tags") == 0
    assert app.scalar("SELECT COUNT(*) FROM categories") == 1  # カテゴリ自体は消さない
    assert app.scalar("SELECT COUNT(*) FROM tags") == 1


def test_edit_replaces_content_from_new_file_and_updates_search_index(app):
    note_id = app.register("a.txt", "古い本文のテキスト")
    new_path = app.txt("b.txt", "新しい本文のテキスト")
    result = app.run("edit", note_id, inputs=["", "", "", "", new_path, "y"])
    assert result.code == 0, result.err
    (content, digest, source), = rows(app, "SELECT content, content_hash, source_path FROM notes")
    assert content == "新しい本文のテキスト"
    assert digest == hashlib.sha256(content.encode("utf-8")).hexdigest()
    assert source.endswith("b.txt")
    assert "1件" in app.run("search", "新しい本文").out
    assert "見つかりませんでした" in app.run("search", "古い本文").out


def test_edit_content_retries_after_invalid_file_and_checks_duplicates(app):
    app.register("other.txt", "他のメモの内容")
    note_id = app.register("a.txt", "編集対象の内容")
    duplicate = app.txt("dup.txt", "他のメモの内容")

    # 不正なファイル → エラー表示後に再入力 → 重複警告でno → 別ファイルを諦めてEnter(変更なし)
    result = app.run("edit", note_id, inputs=["", "", "", "", "no_such.txt", duplicate, "n", "", ])
    assert result.code == 0
    assert "見つかりません" in result.out and "同じ内容のメモが既に登録されています" in result.out
    assert "変更はありません" in result.out
    assert app.scalar("SELECT content FROM notes WHERE id = ?", (note_id,)) == "編集対象の内容"

    accepted = app.run("edit", note_id, inputs=["", "", "", "", duplicate, "y", "y"])
    assert accepted.code == 0
    assert app.scalar("SELECT content FROM notes WHERE id = ?", (note_id,)) == "他のメモの内容"


def test_edit_cancel_discards_changes(app):
    note_id = app.register("a.txt", "本文", title="元")
    declined = app.run("edit", note_id, inputs=["変更後", "", "", "", "", "n"])
    assert declined.code == 0 and "キャンセル" in declined.out
    interrupted = app.run("edit", note_id, inputs=["変更後", KeyboardInterrupt])
    assert interrupted.code == 0
    assert app.scalar("SELECT title FROM notes") == "元"


def test_edit_unknown_id(app):
    assert app.run("edit", "00000000").code == 1


# ---- delete ---------------------------------------------------------------

def test_delete_requires_explicit_yes(app):
    note_id = app.register("a.txt", "本文", category="", tags="t")
    for answer in ("n", "", "x-then-n"):
        inputs = ["maybe", "n"] if answer == "x-then-n" else [answer]
        result = app.run("delete", note_id[:8], inputs=inputs)
        assert result.code == 0 and "キャンセル" in result.out
        assert app.scalar("SELECT COUNT(*) FROM notes") == 1
    assert app.run("delete", note_id[:8], inputs=[KeyboardInterrupt]).code == 0
    assert app.scalar("SELECT COUNT(*) FROM notes") == 1


def test_delete_removes_note_links_and_index_but_keeps_categories_tags_and_source_file(app):
    path = app.txt("a.txt", "削除される本文")
    app.run("register", path, inputs=["T", "n", "技術", "x, y", "", "y"])
    note_id = app.scalar("SELECT id FROM notes")
    result = app.run("delete", note_id, inputs=["y"])
    assert result.code == 0 and "削除しました" in result.out
    assert app.scalar("SELECT COUNT(*) FROM notes") == 0
    assert app.scalar("SELECT COUNT(*) FROM note_tags") == 0
    assert app.scalar("SELECT COUNT(*) FROM notes_fts") == 0
    assert app.scalar("SELECT COUNT(*) FROM categories") == 1
    assert app.scalar("SELECT COUNT(*) FROM tags") == 2
    assert (app.base_dir / "txt/a.txt").read_text(encoding="utf-8") == "削除される本文"


def test_delete_unknown_id(app):
    result = app.run("delete", "00000000", inputs=["y"])
    assert result.code == 1 and "見つかりません" in result.err


# ---- category -------------------------------------------------------------

def test_category_list_add_rename(app):
    assert "登録されていません" in app.run("category", "list").out
    assert app.run("category", "add", "技術").code == 0
    assert app.run("category", "add", "仕事").code == 0
    listed = app.run("category", "list")
    assert "技術" in listed.out and "仕事" in listed.out and "全2件" in listed.out

    dup = app.run("category", "add", "技術")
    assert dup.code == 1 and "既に存在" in dup.err
    assert app.run("category", "add", "   ").code == 1


def test_category_names_are_unique_after_normalization(app):
    app.run("category", "add", "Python")
    for variant in ("python", "ＰＹＴＨＯＮ", " Python "):
        assert app.run("category", "add", variant).code == 1, variant
    assert app.scalar("SELECT COUNT(*) FROM categories") == 1


def test_category_rename_keeps_notes_linked(app):
    path = app.txt("a.txt", "x")
    app.run("register", path, inputs=["T", "n", "旧名", "", "", "y"])
    assert app.run("category", "rename", "旧名", "新名").code == 0
    assert "新名" in app.run("show", app.scalar("SELECT id FROM notes")).out
    assert app.run("category", "list").out.count("新名") == 1
    assert app.run("category", "rename", "新名", "新名").code == 0  # 同名への変更も無害
    assert app.run("category", "rename", "存在しない", "何か").code == 1
    app.run("category", "add", "別")
    clash = app.run("category", "rename", "新名", "別")
    assert clash.code == 1 and "既に存在" in clash.err


# ---- backup ---------------------------------------------------------------

def test_backup_creates_readable_copy_with_timestamped_name(app):
    app.register("a.txt", "バックアップされる本文", title="B")
    result = app.run("backup")
    assert result.code == 0
    files = list((app.base_dir / "backup").glob("ctd_backup_*.db"))
    assert len(files) == 1
    import re
    assert re.fullmatch(r"ctd_backup_\d{8}_\d{6}(_\d+)?\.db", files[0].name)
    copy = sqlite3.connect(str(files[0]))
    assert copy.execute("SELECT title FROM notes").fetchone()[0] == "B"
    copy.close()


def test_backup_never_overwrites_previous_backups(app):
    for _ in range(3):
        assert app.run("backup").code == 0
    assert len(list((app.base_dir / "backup").glob("ctd_backup_*.db"))) == 3
    assert list((app.base_dir / "backup").glob("*.part")) == []


def test_backup_failure_reports_error_and_keeps_data(app):
    blocker = app.base_dir / "blocker"
    blocker.write_text("file", encoding="utf-8")
    (app.base_dir / "config/config.json").write_text(
        '{"database_path": "database/ctd.db", "backup_dir": "blocker"}', encoding="utf-8"
    )
    result = app.run("backup")
    assert result.code == 1 and "バックアップ" in result.err
    assert app.run("list").code == 0


# ---- 予期しない例外 -------------------------------------------------------

def test_unexpected_exception_does_not_crash_with_traceback(app, monkeypatch):
    def boom(args, ctx):
        raise RuntimeError("boom")

    monkeypatch.setattr(list_module, "run", boom)
    result = app.run("list")
    assert result.code == 1
    assert "予期しないエラー" in result.err and "Traceback" not in result.err

    monkeypatch.setenv("CTD_DEBUG", "1")
    assert "Traceback" in app.run("list").err


def test_ctrl_c_outside_prompts_is_treated_as_cancel(app, monkeypatch):
    def interrupted(args, ctx):
        raise KeyboardInterrupt()

    monkeypatch.setattr(list_module, "run", interrupted)
    result = app.run("list")
    assert result.code == 0 and "キャンセル" in result.out
