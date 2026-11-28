from contextlib import closing
from pathlib import Path
import sqlite3
from tempfile import TemporaryDirectory
import unittest

from recover_motrix_history import recover


SCHEMA = """
CREATE TABLE download_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    gid TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    uri TEXT,
    dir TEXT,
    total_length INTEGER,
    status TEXT NOT NULL,
    task_type TEXT,
    added_at DATETIME,
    created_at DATETIME,
    completed_at DATETIME,
    meta TEXT
);
"""


class RecoverMotrixHistoryTest(unittest.TestCase):
    def setUp(self):
        directory = TemporaryDirectory(prefix="history test #")
        self.addCleanup(directory.cleanup)
        root = Path(directory.name)
        self.source = root / "old history.db"
        self.target = root / "new history.db"
        for path in (self.source, self.target):
            with closing(sqlite3.connect(path)) as db:
                db.executescript(SCHEMA)

    def rows(self, path):
        with closing(sqlite3.connect(path)) as db:
            return db.execute(
                "SELECT gid, name, meta FROM download_history ORDER BY gid"
            ).fetchall()

    def test_preview_merge_backup_and_repeat(self):
        with closing(sqlite3.connect(self.source)) as old:
            old.execute("PRAGMA journal_mode=WAL")
            old.executemany(
                "INSERT INTO download_history (gid, name, status, meta) VALUES (?, ?, 'complete', ?)",
                [("shared", "Old name", None), ("missing", "Old download", '{"files": []}')],
            )
            old.commit()
            with closing(sqlite3.connect(self.target)) as current:
                current.execute("PRAGMA journal_mode=WAL")
                current.execute(
                    "INSERT INTO download_history (gid, name, status) VALUES ('shared', 'New name', 'complete')"
                )
                current.commit()

            original_source = self.rows(self.source)
            original_target = self.rows(self.target)
            self.assertEqual(recover(self.source, self.target, False), (1, None))
            self.assertEqual(self.rows(self.target), original_target)
            self.assertEqual(list(self.target.parent.glob("history-backup-*")), [])

            count, backup = recover(self.source, self.target, True)
            self.assertEqual(count, 1)
            self.assertEqual(self.rows(backup), original_target)
            self.assertEqual(
                self.rows(self.target),
                [("missing", "Old download", '{"files": []}'), *original_target],
            )
            self.assertEqual(self.rows(self.source), original_source)
            self.assertEqual(recover(self.source, self.target, True), (0, None))

    def test_incompatible_source_leaves_target_unchanged(self):
        with closing(sqlite3.connect(self.source)) as old:
            old.executescript("DROP TABLE download_history; CREATE TABLE download_history (gid TEXT);")
        before = self.target.read_bytes()
        with self.assertRaises(sqlite3.Error):
            recover(self.source, self.target, True)
        self.assertEqual(self.target.read_bytes(), before)
        self.assertEqual(list(self.target.parent.glob("history-backup-*")), [])


if __name__ == "__main__":
    unittest.main()
