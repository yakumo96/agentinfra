"""The one original-content store (Headroom's CCR, unified).

The proxy writes originals here when it compresses; `context_retrieve` reads
from the same table; there is one retention policy. The file is 0600.
"""
from __future__ import annotations

import hashlib
import os
import sqlite3
import time
from pathlib import Path


class OriginalStore:
    def __init__(self, path: str | Path, retention_s: int):
        self.path = Path(path)
        self.retention_s = retention_s
        new = not self.path.exists()
        if new:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            fd = os.open(self.path, os.O_CREAT | os.O_WRONLY, 0o600)
            os.close(fd)
        os.chmod(self.path, 0o600)
        self.db = sqlite3.connect(self.path, isolation_level=None)
        self.db.execute("PRAGMA journal_mode=WAL")
        self.db.execute(
            "CREATE TABLE IF NOT EXISTS originals ("
            " digest TEXT PRIMARY KEY, content TEXT NOT NULL, created_at REAL NOT NULL, last_read_at REAL)"
        )

    @staticmethod
    def digest(content: str) -> str:
        return hashlib.sha256(content.encode("utf-8")).hexdigest()

    def put(self, content: str, now: float | None = None) -> str:
        d = self.digest(content)
        self.db.execute(
            "INSERT INTO originals (digest, content, created_at) VALUES (?, ?, ?) "
            "ON CONFLICT (digest) DO UPDATE SET created_at = excluded.created_at",
            (d, content, now if now is not None else time.time()),
        )
        return d

    def get(self, digest: str, now: float | None = None) -> str | None:
        now = now if now is not None else time.time()
        row = self.db.execute("SELECT content, created_at FROM originals WHERE digest = ?", (digest,)).fetchone()
        if row is None or row[1] + self.retention_s < now:
            return None
        self.db.execute("UPDATE originals SET last_read_at = ? WHERE digest = ?", (now, digest))
        return row[0]

    def expire(self, now: float | None = None) -> int:
        now = now if now is not None else time.time()
        cur = self.db.execute("DELETE FROM originals WHERE created_at + ? < ?", (self.retention_s, now))
        return cur.rowcount

    def close(self) -> None:
        self.db.close()
