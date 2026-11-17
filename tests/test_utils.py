import pytest

from ctd.errors import CtdError, UsageError
from ctd.utils import console
from ctd.utils.file_reader import read_text_file
from ctd.utils.hash import sha256_text
from ctd.utils.timeutil import format_local, now_utc_iso
from ctd.utils.validator import name_key, normalize_id_prefix, parse_tags, validate_name


def write(tmp_path, name, data: bytes):
    path = tmp_path / name
    path.write_bytes(data)
    return path


# ---- file_reader ----------------------------------------------------------

def test_read_removes_bom_and_normalizes_newlines(tmp_path):
    path = write(tmp_path, "a.txt", b"\xef\xbb\xbfline1\r\nline2\rline3\n")
    data = read_text_file(str(path))
    assert data.content == "line1\nline2\nline3\n"
    assert data.path == path.resolve()


def test_hash_is_computed_after_normalization(tmp_path):
    crlf = write(tmp_path, "a.txt", "あ\r\nい\r\n".encode("utf-8"))
    lf = write(tmp_path, "b.txt", "あ\nい\n".encode("utf-8"))
    assert read_text_file(str(crlf)).content_hash == read_text_file(str(lf)).content_hash
    assert read_text_file(str(lf)).content_hash == sha256_text("あ\nい\n")


def test_leading_and_trailing_whitespace_is_kept(tmp_path):
    path = write(tmp_path, "a.txt", b"\n\n  text  \n\n")
    assert read_text_file(str(path)).content == "\n\n  text  \n\n"


@pytest.mark.parametrize("raw", [b"", b"   \n\t\n", "\ufeff".encode("utf-8"), b"\r\n\r\n"])
def test_empty_or_whitespace_only_is_rejected(tmp_path, raw):
    path = write(tmp_path, "empty.txt", raw)
    with pytest.raises(CtdError, match="空"):
        read_text_file(str(path))


def test_shift_jis_is_rejected_with_guidance(tmp_path):
    path = write(tmp_path, "sjis.txt", "日本語のテキストです".encode("shift_jis"))
    with pytest.raises(CtdError) as info:
        read_text_file(str(path))
    assert "UTF-8" in info.value.message
    assert "UTF-8" in info.value.hint


def test_utf16_is_rejected(tmp_path):
    with_bom = write(tmp_path, "u16.txt", "abc".encode("utf-16"))
    without_bom = write(tmp_path, "u16le.txt", "abc".encode("utf-16-le"))
    for path in (with_bom, without_bom):
        with pytest.raises(CtdError):
            read_text_file(str(path))


def test_extension_check_is_case_insensitive_and_txt_only(tmp_path):
    assert read_text_file(str(write(tmp_path, "UPPER.TXT", b"ok"))).content == "ok"
    with pytest.raises(CtdError, match="TXT"):
        read_text_file(str(write(tmp_path, "note.md", b"ok")))


def test_missing_file_and_directory_are_rejected(tmp_path):
    with pytest.raises(CtdError, match="見つかりません"):
        read_text_file(str(tmp_path / "none.txt"))
    folder = tmp_path / "dir.txt"
    folder.mkdir()
    with pytest.raises(CtdError, match="ファイルではありません"):
        read_text_file(str(folder))


def test_quoted_path_is_accepted(tmp_path):
    path = write(tmp_path, "my memo.txt", b"ok")
    assert read_text_file(f'"{path}"').content == "ok"


# ---- validator ------------------------------------------------------------

def test_name_key_normalizes_width_case_and_spaces():
    assert name_key("  Ｐｙｔｈｏｎ ") == name_key("python")
    assert name_key("ＡＢＣ　") == "abc"
    assert name_key("ﾒﾓ") == name_key("メモ")


def test_validate_name_rejects_blank():
    with pytest.raises(CtdError):
        validate_name("   ", "タグ")
    with pytest.raises(CtdError):
        validate_name("　", "カテゴリ")
    assert validate_name("  Python ", "タグ") == "Python"


def test_parse_tags_splits_dedupes_and_keeps_first_spelling():
    assert parse_tags("Python, python ,ＰＹＴＨＯＮ、SQL，,  ") == ["Python", "SQL"]
    assert parse_tags("") == []
    assert parse_tags(" , ,") == []


def test_normalize_id_prefix():
    assert normalize_id_prefix(" ABCDEF12 ") == "abcdef12"
    with pytest.raises(UsageError, match="8文字以上"):
        normalize_id_prefix("abc123")
    with pytest.raises(UsageError, match="形式"):
        normalize_id_prefix("zzzzzzzz")
    with pytest.raises(UsageError):
        normalize_id_prefix("%%%%%%%%")


# ---- timeutil -------------------------------------------------------------

def test_now_is_utc_iso_with_z_suffix():
    text = now_utc_iso()
    assert text.endswith("Z") and "T" in text


def test_format_local_handles_utc_text():
    assert len(format_local("2026-11-15T03:00:00.000Z")) == len("2026-11-15 12:00")
    assert len(format_local("2026-11-15T03:00:00.000Z", with_seconds=True)) == len("2026-11-15 12:00:00")
    assert format_local("broken") == "broken"


# ---- console --------------------------------------------------------------

def test_display_width_counts_full_width_as_two():
    assert console.display_width("abc") == 3
    assert console.display_width("日本語") == 6
    assert console.display_width("aあ") == 3


def test_truncate_respects_display_width():
    text = console.truncate("日本語のタイトルです", 11)
    assert console.display_width(text) <= 11
    assert text.endswith("...")
    assert console.truncate("short", 10) == "short"


def test_render_table_aligns_japanese_columns():
    lines = console.render_table(["タイトル", "ID"], [["短い", "a1"], ["とても長い", "b2"]])
    # 2列目の直前までの表示幅が全行でそろっている(全角は幅2として計算)
    widths = {console.display_width(line.rsplit("  ", 1)[0]) for line in lines}
    assert widths == {10}
    assert lines[0].startswith("タイトル")


def test_render_table_shrinks_flex_column_to_total_width():
    rows = [["a1", "あ" * 60, "cat"]]
    lines = console.render_table(["ID", "タイトル", "カテゴリ"], rows, flex_col=1, total_width=50)
    assert all(console.display_width(line) <= 50 for line in lines)
    assert "..." in lines[2]


def test_confirm_prompt_text_and_defaults(monkeypatch):
    prompts = []
    answers = iter(["", "x", "y"])

    def fake_input(text=""):
        prompts.append(text)
        return next(answers)

    monkeypatch.setattr("builtins.input", fake_input)
    assert console.confirm("Are you sure?", default=False) is False  # Enterはキャンセル
    assert prompts[0] == "Are you sure? [y/N] "
    assert console.confirm("続けますか?", default=True) is True  # 不正入力は再入力させる
    assert len(prompts) == 3
