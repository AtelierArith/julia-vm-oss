#!/usr/bin/env python3
"""Select whether changed repository paths require the AoT gate."""

from __future__ import annotations

import argparse
import fnmatch
import sys
from pathlib import Path
from typing import NamedTuple, Optional, Sequence, Tuple


class Selection(NamedTuple):
    run_aot: bool
    matched_patterns: Tuple[str, ...]


def normalize_path(path: str) -> str:
    normalized = path.strip().replace("\\", "/")
    while normalized.startswith("./"):
        normalized = normalized[2:]
    return normalized


def load_patterns(path: Path) -> Tuple[str, ...]:
    patterns = []
    for line_number, raw_line in enumerate(
        path.read_text(encoding="utf-8").splitlines(), start=1
    ):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("/") or line.startswith("./") or "\\" in line:
            raise ValueError(
                f"{path}:{line_number}: patterns must be normalized repository paths"
            )
        if line in patterns:
            raise ValueError(f"{path}:{line_number}: duplicate pattern: {line}")
        patterns.append(line)
    if not patterns:
        raise ValueError(f"{path}: no AoT gate path patterns found")
    return tuple(patterns)


def select_for_paths(paths: Sequence[str], patterns: Sequence[str]) -> Selection:
    matched = []
    for raw_path in paths:
        path = normalize_path(raw_path)
        if not path:
            continue
        for pattern in patterns:
            if fnmatch.fnmatchcase(path, pattern) and pattern not in matched:
                matched.append(pattern)
    return Selection(run_aot=bool(matched), matched_patterns=tuple(matched))


def read_changed_files(path: Path) -> Tuple[str, ...]:
    return tuple(path.read_text(encoding="utf-8").splitlines())


def write_github_output(path: Path, selection: Selection) -> None:
    reason = ",".join(selection.matched_patterns) or "no-aot-relevant-paths"
    with path.open("a", encoding="utf-8") as output:
        output.write(f"aot={'true' if selection.run_aot else 'false'}\n")
        output.write(f"aot_reason={reason}\n")


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--changed-files", required=True, type=Path)
    parser.add_argument(
        "--config", default=Path(".github/aot-gate-paths.txt"), type=Path
    )
    parser.add_argument("--github-output", type=Path)
    return parser.parse_args(argv)


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    try:
        patterns = load_patterns(args.config)
        selection = select_for_paths(read_changed_files(args.changed_files), patterns)
    except (OSError, ValueError) as error:
        print(f"select_aot_gate.py: {error}", file=sys.stderr)
        return 2

    if args.github_output is not None:
        write_github_output(args.github_output, selection)
    reason = ",".join(selection.matched_patterns) or "no-aot-relevant-paths"
    print(f"aot={'true' if selection.run_aot else 'false'}")
    print(f"aot_reason={reason}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
