#!/usr/bin/env python3
"""Fail-loud source edits for audit negative-control injectors (Issue #11274)."""

from __future__ import annotations

import re
from pathlib import Path


def replace_literal_exactly_once(
    path: Path, needle: str, replacement: str, *, label: str
) -> None:
    source = path.read_text(encoding="utf-8")
    count = source.count(needle)
    if count != 1:
        raise AssertionError(f"{label}: expected exactly one literal match, found {count}")
    path.write_text(source.replace(needle, replacement, 1), encoding="utf-8")


def replace_regex_exactly_once(
    path: Path,
    pattern,
    replacement,
    *,
    label: str,
    flags: int = 0,
) -> None:
    source = path.read_text(encoding="utf-8")
    compiled = re.compile(pattern, flags) if isinstance(pattern, str) else pattern
    matches = list(compiled.finditer(source))
    if len(matches) != 1:
        raise AssertionError(
            f"{label}: expected exactly one regex match, found {len(matches)}"
        )
    path.write_text(compiled.sub(replacement, source, count=1), encoding="utf-8")


def _selftest() -> None:
    path = Path("/tmp/sjulia-audit-selftest-edit-selftest.txt")
    try:
        path.write_text("fn one(x: i32) {}\n", encoding="utf-8")
        replace_literal_exactly_once(path, "i32", "i64", label="literal")
        replace_regex_exactly_once(
            path,
            r"fn\s+(?P<name>\w+)\([^)]*\)",
            lambda match: f"fn {match.group('name')}(value: i64)",
            label="regex",
        )
        assert path.read_text(encoding="utf-8") == "fn one(value: i64) {}\n"

        try:
            replace_literal_exactly_once(path, "missing", "x", label="zero")
        except AssertionError as error:
            assert "found 0" in str(error)
        else:
            raise AssertionError("zero-match literal edit did not fail")

        path.write_text("same same", encoding="utf-8")
        try:
            replace_regex_exactly_once(path, r"same", "other", label="multiple")
        except AssertionError as error:
            assert "found 2" in str(error)
        else:
            raise AssertionError("multiple-match regex edit did not fail")
    finally:
        if path.exists():
            path.unlink()


if __name__ == "__main__":
    _selftest()
    print("audit_selftest_edit selftest: OK")
