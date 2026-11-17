"""日時の扱い。DB内部はUTC(ISO 8601, 末尾Z)、表示はローカル時刻。"""

from __future__ import annotations

from datetime import datetime, timezone


def now_utc_iso() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def format_local(iso_text: str, with_seconds: bool = False) -> str:
    try:
        parsed = datetime.fromisoformat(iso_text.replace("Z", "+00:00"))
    except ValueError:
        return iso_text
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    local = parsed.astimezone()
    return local.strftime("%Y-%m-%d %H:%M:%S" if with_seconds else "%Y-%m-%d %H:%M")


def backup_timestamp(now: datetime | None = None) -> str:
    """バックアップ名用のローカル時刻(例: 20261115_120000)。"""
    return (now or datetime.now()).strftime("%Y%m%d_%H%M%S")
