#!/usr/bin/env python3
"""Select pr-fast fixture-test filters from changed paths.

Safety rule: when a Rust/VM-relevant path is not covered by a specific mapping,
return scope=all so CI runs the full fixture suite.
"""

from __future__ import annotations

import argparse
import fnmatch
import re
import sys
import tomllib
from pathlib import Path
from typing import NamedTuple


FIXTURE_RE = re.compile(r"^subset_julia_vm/tests/fixtures/([^/]+)/[^/].*\.jl$")


class Rule(NamedTuple):
    name: str
    paths: tuple[str, ...]
    filters: tuple[str, ...]


class Config(NamedTuple):
    smoke_filters: tuple[str, ...]
    full_fallback_patterns: tuple[str, ...]
    rules: tuple[Rule, ...]


class Selection(NamedTuple):
    scope: str
    filters: tuple[str, ...]
    reasons: tuple[str, ...]


def load_config(path: Path) -> Config:
    with path.open("rb") as f:
        raw = tomllib.load(f)
    selection = raw.get("selection", {})
    rules = tuple(
        Rule(
            name=str(rule["name"]),
            paths=tuple(str(p) for p in rule.get("paths", ())),
            filters=tuple(str(f) for f in rule.get("filters", ())),
        )
        for rule in raw.get("rules", ())
    )
    return Config(
        smoke_filters=tuple(str(f) for f in selection.get("smoke_filters", ())),
        full_fallback_patterns=tuple(
            str(p) for p in selection.get("full_fallback_patterns", ())
        ),
        rules=rules,
    )


def normalize_path(path: str) -> str:
    path = path.strip().replace("\\", "/")
    while path.startswith("./"):
        path = path[2:]
    return path


def matches_any(path: str, patterns: tuple[str, ...]) -> bool:
    return any(fnmatch.fnmatchcase(path, pattern) for pattern in patterns)


def add_unique(items: list[str], values: tuple[str, ...]) -> None:
    for value in values:
        if value not in items:
            items.append(value)


def fixture_category(path: str) -> str | None:
    match = FIXTURE_RE.match(path)
    if match is None:
        return None
    return match.group(1)


def select_for_paths(paths: list[str], config: Config) -> Selection:
    normalized = [normalize_path(path) for path in paths if normalize_path(path)]
    if not normalized:
        return Selection(scope="all", filters=(), reasons=("no-changed-files",))

    filters: list[str] = []
    reasons: list[str] = []
    saw_selected = False
    saw_relevant = False
    saw_unmapped_relevant = False

    for path in normalized:
        category = fixture_category(path)
        if category is not None:
            saw_selected = True
            saw_relevant = True
            add_unique(filters, config.smoke_filters)
            add_unique(filters, (f"{category}::",))
            reason = f"changed-fixture:{category}"
            if reason not in reasons:
                reasons.append(reason)
            continue

        matched_rule = False
        for rule in config.rules:
            if matches_any(path, rule.paths):
                saw_selected = True
                saw_relevant = True
                matched_rule = True
                add_unique(filters, config.smoke_filters)
                add_unique(filters, rule.filters)
                if rule.name not in reasons:
                    reasons.append(rule.name)

        if matched_rule:
            continue

        if matches_any(path, config.full_fallback_patterns):
            saw_relevant = True
            saw_unmapped_relevant = True

    if saw_unmapped_relevant:
        reason_values = tuple(reasons + ["unmapped-rust"])
        return Selection(scope="all", filters=(), reasons=reason_values)
    if saw_selected:
        return Selection(scope="selected", filters=tuple(filters), reasons=tuple(reasons))
    if saw_relevant:
        return Selection(scope="all", filters=(), reasons=("unmapped-rust",))
    return Selection(scope="none", filters=(), reasons=("docs-only",))


def read_changed_files(path: Path) -> list[str]:
    return path.read_text(encoding="utf-8").splitlines()


def write_github_output(path: Path, selection: Selection) -> None:
    filters = " ".join(selection.filters)
    reasons = ",".join(selection.reasons)
    with path.open("a", encoding="utf-8") as f:
        f.write(f"fixture_scope={selection.scope}\n")
        f.write(f"fixture_filters={filters}\n")
        f.write(f"fixture_reason={reasons}\n")


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--changed-files", required=True, type=Path)
    parser.add_argument(
        "--config", default=Path(".github/fixture-selection.toml"), type=Path
    )
    parser.add_argument("--github-output", type=Path)
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    config = load_config(args.config)
    selection = select_for_paths(read_changed_files(args.changed_files), config)
    if args.github_output is not None:
        write_github_output(args.github_output, selection)
    print(f"fixture_scope={selection.scope}")
    print(f"fixture_filters={' '.join(selection.filters)}")
    print(f"fixture_reason={','.join(selection.reasons)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
