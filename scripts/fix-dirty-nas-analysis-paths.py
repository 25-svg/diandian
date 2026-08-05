#!/usr/bin/env python3
"""Fix NAS archive paths polluted by import note JSON.

Default is dry-run. Pass --apply to move files and UPDATE sqlite.

Examples:
  python scripts/fix-dirty-nas-analysis-paths.py
  python scripts/fix-dirty-nas-analysis-paths.py --apply
  python scripts/fix-dirty-nas-analysis-paths.py --db "C:/Users/.../data_v2.db" --apply
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import sqlite3
import sys
from pathlib import Path


ILLEGAL = '<>:"/\\|?*'


def sanitize_component(value: str, fallback: str) -> str:
    text = (value or "").strip()
    if not text:
        return fallback
    replaced = "".join("_" if ch in ILLEGAL else ch for ch in text).strip()
    return replaced or fallback


def looks_like_analysis_metadata(value: str) -> bool:
    text = (value or "").strip()
    if not text:
        return False
    lowered = text.lower()
    return (
        text.startswith("{")
        or "analysispurpose" in lowered
        or "masterscriptkey" in lowered
        or "competitorname" in lowered
    )


def find_db(explicit: str | None) -> Path:
    if explicit:
        path = Path(explicit)
        if not path.is_file():
            raise SystemExit(f"DB not found: {path}")
        return path
    appdata = Path(os.environ.get("APPDATA", ""))
    local = Path(os.environ.get("LOCALAPPDATA", ""))
    candidates = [
        appdata / "cn.vjoi.bilishadowreplay" / "data_v2.db",
        appdata / "cn.vjoi.bili-shadowreplay" / "data_v2.db",
        local / "cn.vjoi.bilishadowreplay" / "data_v2.db",
        local / "cn.vjoi.bili-shadowreplay" / "data_v2.db",
    ]
    for candidate in candidates:
        if candidate.is_file() and candidate.stat().st_size > 0:
            return candidate
    raise SystemExit("Could not find data_v2.db. Pass --db explicitly.")


def choose_anchor(title: str, room_id: str) -> str:
    if title.strip() and not looks_like_analysis_metadata(title):
        return title.strip()
    if room_id.strip() and not looks_like_analysis_metadata(room_id):
        return room_id.strip()
    return "其他主播"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--db", default="")
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()

    db_path = find_db(args.db or None)
    print(f"Using database: {db_path}")
    print("Mode:", "APPLY" if args.apply else "DRY-RUN")

    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row
    rows = conn.execute(
        """
        SELECT
          j.id AS job_id,
          j.video_id,
          j.nas_path,
          j.local_path,
          v.title,
          v.room_id,
          v.file,
          v.created_at
        FROM video_archive_jobs j
        JOIN videos v ON v.id = j.video_id
        WHERE lower(j.nas_path) LIKE '%analysispurpose%'
           OR lower(j.nas_path) LIKE '%masterscriptkey%'
           OR lower(j.nas_path) LIKE '%competitorname%'
           OR j.nas_path LIKE '%{%'
        ORDER BY j.id
        """
    ).fetchall()

    if not rows:
        print("No dirty nas_path rows found.")
        return 0

    fixed = 0
    for row in rows:
        old_path = Path(str(row["nas_path"]))
        file_name = old_path.name
        root = old_path.anchor
        if not root:
            print(f"Skip job {row['job_id']}: cannot determine drive root for {old_path}")
            continue
        anchor = sanitize_component(choose_anchor(row["title"] or "", row["room_id"] or ""), "其他主播")
        created = row["created_at"] or ""
        date = created[:10] if len(created) >= 10 and re.match(r"\d{4}-\d{2}-\d{2}", created[:10]) else "日期未知"
        new_dir = Path(root) / anchor / date
        new_path = new_dir / file_name

        print()
        print(f"job={row['job_id']} video={row['video_id']}")
        print(f"  from: {old_path}")
        print(f"  to:   {new_path}")

        if not args.apply:
            continue
        if not old_path.is_file():
            print("  source file missing; skipped file move (DB unchanged)")
            continue
        new_dir.mkdir(parents=True, exist_ok=True)
        if new_path.exists() and new_path.resolve() != old_path.resolve():
            stem = new_path.stem
            suffix = new_path.suffix
            index = 2
            while True:
                candidate = new_dir / f"{stem}-{index}{suffix}"
                if not candidate.exists():
                    new_path = candidate
                    break
                index += 1
            print(f"  collision -> {new_path}")
        try:
            shutil.move(str(old_path), str(new_path))
            moved = True
        except PermissionError:
            # App/ffmpeg may still hold the source open. Copy first, keep old
            # file for manual cleanup after the app is closed.
            print("  move locked; copying instead (close the app later to delete the old file)")
            shutil.copy2(str(old_path), str(new_path))
            moved = False
        conn.execute(
            "UPDATE video_archive_jobs SET nas_path = ? WHERE id = ?",
            (str(new_path), row["job_id"]),
        )
        if (row["file"] or "") == str(old_path):
            conn.execute(
                "UPDATE videos SET file = ? WHERE id = ?",
                (str(new_path), row["video_id"]),
            )
        conn.commit()
        fixed += 1
        print("  updated" + ("" if moved else " (DB points to copy; old dirty path still on disk)"))

    print()
    if args.apply:
        print(f"Done. Updated {fixed} row(s). Re-open the app and retry clips.")
    else:
        print(f"Dry-run complete ({len(rows)} dirty row(s)). Re-run with --apply to move files and update DB.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
