"""設定ファイルと基準ディレクトリの管理。

基準ディレクトリは ctd.exe の配置ディレクトリ。
開発時(非EXE実行)はプロジェクトのルートディレクトリを基準とする。
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from pathlib import Path

from .errors import CtdError

CONFIG_RELATIVE_PATH = Path("config") / "config.json"
TXT_DIR_NAME = "txt"
DEFAULT_SETTINGS = {
    "database_path": "database/ctd.db",
    "backup_dir": "backup",
}


@dataclass(frozen=True)
class Config:
    base_dir: Path
    database_path: Path
    backup_dir: Path


def get_default_base_dir() -> Path:
    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve().parent
    # src/ctd/config.py -> プロジェクトルート
    return Path(__file__).resolve().parents[2]


def config_file_path(base_dir: Path) -> Path:
    return base_dir / CONFIG_RELATIVE_PATH


def _resolve(base_dir: Path, value: str) -> Path:
    path = Path(value).expanduser()
    return path if path.is_absolute() else base_dir / path


def write_default_config(base_dir: Path) -> bool:
    """設定ファイルが無ければ作成する。既存の場合は何もしない(Falseを返す)。"""
    path = config_file_path(base_dir)
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with open(path, "x", encoding="utf-8") as f:  # "x": 既存ファイルは上書きしない
            json.dump(DEFAULT_SETTINGS, f, indent=2, ensure_ascii=False)
            f.write("\n")
    except FileExistsError:
        return False
    return True


def load_config(base_dir: Path) -> Config:
    path = config_file_path(base_dir)
    if not path.is_file():
        raise CtdError(
            "設定ファイルが見つかりません。",
            hint="先に `ctd init` を実行して初期化してください。",
        )
    try:
        data = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, ValueError) as e:
        raise CtdError(f"設定ファイルを読み込めません: {path} ({e})")
    if not isinstance(data, dict):
        raise CtdError(f"設定ファイルの形式が正しくありません(JSONオブジェクトが必要です): {path}")

    values: dict[str, str] = {}
    for key, default in DEFAULT_SETTINGS.items():
        value = data.get(key, default)
        if not isinstance(value, str) or not value.strip():
            raise CtdError(f"設定ファイルの「{key}」が正しくありません: {path}")
        values[key] = value
    return Config(
        base_dir=base_dir,
        database_path=_resolve(base_dir, values["database_path"]),
        backup_dir=_resolve(base_dir, values["backup_dir"]),
    )
