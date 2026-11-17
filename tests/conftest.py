"""共通フィクスチャ。テストは必ず一時ディレクトリのDBだけを使い、実際のユーザーデータには触れない。"""

from __future__ import annotations

import sqlite3

import pytest

from ctd import config as config_module
from ctd.main import main


class Result:
    def __init__(self, code: int, out: str, err: str):
        self.code = code
        self.out = out
        self.err = err


class App:
    """一時ディレクトリを基準ディレクトリにしてctdを実行するテスト用ハーネス。"""

    def __init__(self, base_dir, monkeypatch, capsys):
        self.base_dir = base_dir
        self._monkeypatch = monkeypatch
        self._capsys = capsys
        self.prompts: list[str] = []  # input()に渡されたプロンプト文言(画面に出るはずの文言)

    def run(self, *argv: str, inputs=()) -> Result:
        """inputs: 対話入力の列。例外クラス/インスタンスを入れるとその例外をinput()で発生させる。
        入力が尽きた場合はEOFErrorになる(=キャンセル扱い)。"""
        feed = iter(inputs)
        self.prompts = []

        def fake_input(prompt=""):
            self.prompts.append(prompt)
            try:
                value = next(feed)
            except StopIteration:
                raise EOFError()
            if isinstance(value, BaseException) or (isinstance(value, type) and issubclass(value, BaseException)):
                raise value
            return value

        self._monkeypatch.setattr("builtins.input", fake_input)
        code = main(list(argv), base_dir=self.base_dir)
        captured = self._capsys.readouterr()
        return Result(code, captured.out, captured.err)

    @property
    def db_path(self):
        return config_module.load_config(self.base_dir).database_path

    def query(self, sql: str, params=()):
        conn = sqlite3.connect(str(self.db_path))
        try:
            return conn.execute(sql, params).fetchall()
        finally:
            conn.close()

    def scalar(self, sql: str, params=()):
        return self.query(sql, params)[0][0]

    def txt(self, name: str, content: str | bytes, encoding: str = "utf-8") -> str:
        folder = self.base_dir / "txt"
        folder.mkdir(parents=True, exist_ok=True)
        path = folder / name
        path.write_bytes(content if isinstance(content, bytes) else content.encode(encoding))
        return str(path)

    def register(self, name: str, content: str, *, title="", category="", tags="", desc="") -> str:
        """登録フローを実行し、登録されたIDを返す。categoryは""(未設定)のみ対応。"""
        path = self.txt(name, content)
        result = self.run("register", path, inputs=[title, category, tags, desc, "y"])
        assert result.code == 0, result.err
        return self.scalar("SELECT id FROM notes ORDER BY created_at DESC, rowid DESC LIMIT 1")


@pytest.fixture(autouse=True)
def _never_touch_real_data(monkeypatch):
    def boom():
        raise RuntimeError("テストが実際のユーザーデータの保存場所を参照しました")

    monkeypatch.setattr(config_module, "get_default_base_dir", boom)


@pytest.fixture
def app_uninit(tmp_path, monkeypatch, capsys):
    return App(tmp_path, monkeypatch, capsys)


@pytest.fixture
def app(app_uninit):
    result = app_uninit.run("init")
    assert result.code == 0, result.err
    return app_uninit
