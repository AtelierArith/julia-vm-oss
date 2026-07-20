#!/usr/bin/env bash
# Mechanically archive old dated sections from docs/vm/STATUS.md and DONE.md.
#
# Keeps the newest dated sections in the live files and moves older whole
# sections verbatim into docs/vm/archive/<NAME>-<YEAR>.md until each live file is
# below --max-lines. Use --check in CI/nightly to fail when the policy drifts.

set -euo pipefail

max_lines=3000
check_only=0
dry_run=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --max-lines)
            max_lines="$2"
            shift 2
            ;;
        --check)
            check_only=1
            shift
            ;;
        --dry-run)
            dry_run=1
            shift
            ;;
        *)
            echo "usage: $0 [--max-lines N] [--check] [--dry-run]" >&2
            exit 2
            ;;
    esac
done

python3 - "$max_lines" "$check_only" "$dry_run" <<'PY'
from __future__ import annotations

import re
import sys
from pathlib import Path

max_lines = int(sys.argv[1])
check_only = sys.argv[2] == "1"
dry_run = sys.argv[3] == "1"

ROOT = Path.cwd()
DOCS = ROOT / "docs" / "vm"
ARCHIVE = DOCS / "archive"
SECTION_RE = re.compile(r"^## 最新対応 \(((\d{4})-\d{2}-\d{2})\)\n", re.MULTILINE)


def line_count(text: str) -> int:
    return text.count("\n") + (0 if text.endswith("\n") or not text else 1)


def split_sections(text: str):
    matches = list(SECTION_RE.finditer(text))
    if not matches:
        return text, []
    preamble = text[: matches[0].start()]
    sections = []
    for idx, match in enumerate(matches):
        end = matches[idx + 1].start() if idx + 1 < len(matches) else len(text)
        sections.append((match.group(2), match.group(1), text[match.start() : end]))
    return preamble, sections


def archive_path_for(live_path: Path, year: str) -> Path:
    return ARCHIVE / f"{live_path.stem}-{year}.md"


def existing_archive_dates(text: str) -> set[str]:
    return {m.group(1) for m in SECTION_RE.finditer(text)}


def process(live_path: Path) -> tuple[int, int, list[Path]]:
    text = live_path.read_text(encoding="utf-8")
    before_lines = line_count(text)
    preamble, sections = split_sections(text)
    if before_lines <= max_lines:
        return before_lines, before_lines, []
    if check_only:
        print(f"ERROR: {live_path} has {before_lines} lines (max {max_lines})")
        return before_lines, before_lines, []

    kept = list(sections)
    moved = []
    while line_count(preamble + "".join(section for _, _, section in kept)) > max_lines and len(kept) > 1:
        moved.append(kept.pop())

    if not moved:
        print(f"ERROR: cannot archive {live_path} below {max_lines} without removing every dated section")
        return before_lines, before_lines, []

    if dry_run:
        after = line_count(preamble + "".join(section for _, _, section in kept))
        print(f"DRY-RUN: {live_path}: {before_lines} -> {after} lines, moving {len(moved)} sections")
        return before_lines, after, []

    live_path.write_text(preamble + "".join(section for _, _, section in kept), encoding="utf-8")

    touched = []
    by_year: dict[str, list[tuple[str, str]]] = {}
    for year, date, section in moved:
        by_year.setdefault(year, []).append((date, section))

    ARCHIVE.mkdir(parents=True, exist_ok=True)
    for year, moved_sections in by_year.items():
        archive = archive_path_for(live_path, year)
        if archive.exists():
            archive_text = archive.read_text(encoding="utf-8")
        else:
            title = "現状分析" if live_path.stem == "STATUS" else "実装済み一覧"
            archive_text = (
                f"# {title}アーカイブ ({year} 年・部分)\n\n"
                f"> `{live_path}` からの機械的アーカイブ (Issue #6341)。\n\n"
                "---\n\n"
            )
        archived_dates = existing_archive_dates(archive_text)
        fresh_sections = [
            (date, section)
            for date, section in moved_sections
            if date not in archived_dates
        ]
        if not fresh_sections:
            continue
        archive_preamble, archived_sections = split_sections(archive_text)
        merged_sections = fresh_sections + [
            (date, section) for _, date, section in archived_sections
        ]
        # Live STATUS/DONE sections are newest-to-oldest. Re-sort the complete
        # archive after every batch so the same chronology holds across batch
        # boundaries; Python's stable sort preserves same-day body order.
        merged_sections.sort(key=lambda item: item[0], reverse=True)
        archive_text = archive_preamble + "".join(section for _, section in merged_sections)
        archive.write_text(archive_text, encoding="utf-8")
        touched.append(archive)

    after_lines = line_count(live_path.read_text(encoding="utf-8"))
    print(f"Archived {live_path}: {before_lines} -> {after_lines} lines")
    return before_lines, after_lines, touched


def main() -> int:
    failures = 0
    touched: set[Path] = set()
    for name in ("STATUS.md", "DONE.md"):
        before, after, archives = process(DOCS / name)
        touched.update(archives)
        if after > max_lines:
            failures += 1
    if touched:
        for path in sorted(touched):
            print(f"Updated archive: {path}")
    return 1 if failures else 0


sys.exit(main())
PY
