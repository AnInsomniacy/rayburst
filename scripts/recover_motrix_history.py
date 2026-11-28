#!/usr/bin/env python3
"""Merge download history from Motrix Next into Rayburst on macOS."""

import argparse
from contextlib import closing
from pathlib import Path
import sqlite3
import sys
import tempfile


COLUMNS = (
    "gid, name, uri, dir, total_length, status, task_type, "
    "added_at, created_at, completed_at, meta"
)
MISSING_HISTORY = """
    FROM legacy.download_history AS old
    WHERE NOT EXISTS (
        SELECT 1 FROM main.download_history AS current WHERE current.gid = old.gid
    )
"""


def recover(source, target, apply):
    if source.samefile(target):
        raise ValueError("The source and destination must be different databases.")

    # The system SQLite on macOS can require write access to recreate WAL
    # sidecars after an app closes. Only main.download_history receives writes.
    with closing(sqlite3.connect(target.as_uri() + "?mode=rw", uri=True)) as db:
        if not apply:
            db.execute("PRAGMA query_only = ON")
        db.execute("ATTACH DATABASE ? AS legacy", (source.as_uri() + "?mode=rw",))
        db.execute(f"SELECT {COLUMNS} FROM legacy.download_history LIMIT 0")
        db.execute(f"SELECT {COLUMNS} FROM main.download_history LIMIT 0")
        available = db.execute("SELECT count(*) " + MISSING_HISTORY).fetchone()[0]
        if not apply or not available:
            return available, None

        backup_dir = Path(tempfile.mkdtemp(prefix="history-backup-", dir=target.parent))
        backup = backup_dir / target.name
        with closing(sqlite3.connect(backup)) as snapshot:
            db.backup(snapshot)
        print(f"Rayburst history backup: {backup}")

        with db:
            inserted = db.execute(
                f"INSERT INTO main.download_history ({COLUMNS}) "
                f"SELECT {COLUMNS} {MISSING_HISTORY} ON CONFLICT(gid) DO NOTHING"
            ).rowcount
        return inserted, backup


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true", help="Back up Rayburst and import missing history.")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("This helper uses macOS application data paths.")

    support = Path.home() / "Library" / "Application Support"
    source = support / "com.motrix.next" / "history.db"
    target = support / "dev.aninsomniacy.rayburst" / "history.db"
    print(f"Motrix Next history: {source}")
    print(f"Rayburst history:    {target}")

    try:
        count, _ = recover(source, target, args.apply)
    except (OSError, sqlite3.Error, ValueError) as error:
        parser.exit(1, f"History recovery failed: {error}\n")

    if args.apply:
        print(f"Imported {count} history entries.")
    else:
        print(f"{count} history entries can be imported.")
        if count:
            print("Quit both apps, then run again with --apply to import them.")


if __name__ == "__main__":
    main()
