#!/usr/bin/env python3
"""Two-sided ratchet for the Issue #10813 exception-parity corpus."""

from __future__ import annotations

import argparse
import csv
import re
import sys
from dataclasses import dataclass
from pathlib import Path


REQUIRED_REPORT_COLUMNS = {
    "id",
    "julia_catchable",
    "sjulia_catchable",
    "type_match",
    "catchable_match",
    "julia_health",
    "sjulia_health",
}
REQUIRED_ALLOWLIST_COLUMNS = {"id", "issue", "class", "reason"}
ISSUE_RE = re.compile(r"#[1-9][0-9]*$")
CATCHABILITY_TOKENS = {
    "yes",
    "no-exception-raised",
    "no-uncatchable",
    "n/a-parse-time",
}


@dataclass
class RatchetResult:
    errors: list[str]
    comparable_count: int
    divergence_count: int
    allowlist_count: int


def _read_tsv(path: Path, required: set[str]) -> tuple[list[dict[str, str]], list[str]]:
    errors: list[str] = []
    try:
        handle = path.open(encoding="utf-8", newline="")
    except OSError as exc:
        return [], [f"cannot read {path}: {exc}"]
    with handle:
        reader = csv.DictReader(
            (line for line in handle if line.strip() and not line.startswith("#")),
            delimiter="\t",
        )
        columns = set(reader.fieldnames or [])
        missing = sorted(required - columns)
        if missing:
            return [], [f"{path}: missing required column(s): {', '.join(missing)}"]
        rows = list(reader)
    return rows, errors


def check_ratchet(
    report_path: Path,
    allowlist_path: Path,
    minimum_cases: int = 0,
    case_baseline_path: Path | None = None,
) -> RatchetResult:
    report, errors = _read_tsv(report_path, REQUIRED_REPORT_COLUMNS)
    allowlist, allowlist_errors = _read_tsv(allowlist_path, REQUIRED_ALLOWLIST_COLUMNS)
    errors.extend(allowlist_errors)
    if errors:
        return RatchetResult(errors, 0, 0, 0)

    report_by_id: dict[str, dict[str, str]] = {}
    for row in report:
        case_id = row["id"].strip()
        if not case_id:
            errors.append(f"{report_path}: report row has an empty id")
        elif case_id in report_by_id:
            errors.append(f"{report_path}: duplicate report id: {case_id}")
        else:
            report_by_id[case_id] = row
        for column in ("type_match", "catchable_match"):
            value = row[column]
            if value not in {"yes", "no", "n/a"}:
                errors.append(
                    f"{report_path}: {case_id}: invalid {column} token {value!r}; "
                    "expected yes, no, or n/a"
                )
        if (row["type_match"] == "n/a") != (row["catchable_match"] == "n/a"):
            errors.append(
                f"{report_path}: {case_id}: type_match/catchable_match must use "
                "n/a together"
            )
        for column in ("julia_catchable", "sjulia_catchable"):
            value = row[column]
            if value not in CATCHABILITY_TOKENS:
                errors.append(
                    f"{report_path}: {case_id}: invalid {column} token {value!r}"
                )
        for column in ("julia_health", "sjulia_health"):
            value = row[column]
            if value != "ok":
                errors.append(
                    f"{report_path}: {case_id}: infrastructure failure in "
                    f"{column}: {value}"
                )
    if len(report_by_id) < minimum_cases:
        errors.append(
            f"{report_path}: corpus shrank to {len(report_by_id)} case(s), below the "
            f"ratcheted floor {minimum_cases} (Issue #10813/#11148)"
        )
    if case_baseline_path is not None:
        baseline_rows, baseline_errors = _read_tsv(case_baseline_path, {"id"})
        errors.extend(baseline_errors)
        baseline_ids = {row["id"].strip() for row in baseline_rows}
        current_ids = set(report_by_id)
        if baseline_ids != current_ids:
            removed = ", ".join(sorted(baseline_ids - current_ids)) or "none"
            added = ", ".join(sorted(current_ids - baseline_ids)) or "none"
            errors.append(
                "corpus case identity drift: "
                f"removed=[{removed}], added=[{added}]; refresh the committed report "
                "explicitly when intentionally changing the corpus (Issue #10813/#11148)"
            )

    allowlist_by_id: dict[str, dict[str, str]] = {}
    for row in allowlist:
        case_id = row["id"].strip()
        issue = row["issue"].strip()
        if not case_id:
            errors.append(f"{allowlist_path}: allowlist row has an empty id")
            continue
        if case_id in allowlist_by_id:
            errors.append(f"{allowlist_path}: duplicate allowlist id: {case_id}")
            continue
        if not ISSUE_RE.fullmatch(issue):
            errors.append(
                f"{allowlist_path}: {case_id}: issue must be #<number>, got {issue!r}"
            )
        if not row["class"].strip() or not row["reason"].strip():
            errors.append(
                f"{allowlist_path}: {case_id}: class and reason must be non-empty"
            )
        allowlist_by_id[case_id] = row

    divergences = {
        case_id
        for case_id, row in report_by_id.items()
        if row["type_match"] == "no" or row["catchable_match"] == "no"
    }

    def divergence_class(row: dict[str, str]) -> str:
        julia_catchable = row["julia_catchable"]
        sjulia_catchable = row["sjulia_catchable"]
        if julia_catchable == "yes" and sjulia_catchable == "no-exception-raised":
            return "silent-error"
        if julia_catchable == "no-exception-raised" and sjulia_catchable == "yes":
            return "spurious-error"
        if julia_catchable == "yes" and sjulia_catchable.startswith("no-"):
            return "raise-layer"
        if row["catchable_match"] == "no":
            return "catchability"
        return "type"
    comparable = sum(
        1
        for row in report_by_id.values()
        if row["type_match"] != "n/a" and row["catchable_match"] != "n/a"
    )

    for case_id in sorted(divergences - allowlist_by_id.keys()):
        errors.append(
            f"NEW unallowlisted exception-parity divergence: {case_id}; file a bug "
            "Issue before adding an issue-linked allowlist row (Issue #10813/#11148)"
        )
    for case_id in sorted(allowlist_by_id.keys() - divergences):
        errors.append(
            f"STALE allowlist entry: {case_id} no longer diverges; remove the row so "
            "the exception-parity ratchet shrinks (Issue #10813/#11148)"
        )
    for case_id in sorted(divergences & allowlist_by_id.keys()):
        observed = divergence_class(report_by_id[case_id])
        expected = allowlist_by_id[case_id]["class"].strip()
        if observed != expected:
            errors.append(
                f"{case_id}: divergence class changed {expected} -> {observed}; "
                "a known gap worsened or changed shape (Issue #10813/#11148)"
            )
    for case_id in sorted(allowlist_by_id.keys() - report_by_id.keys()):
        errors.append(
            f"{allowlist_path}: allowlist id is absent from the corpus report: {case_id}"
        )

    return RatchetResult(errors, comparable, len(divergences), len(allowlist_by_id))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", required=True, type=Path)
    parser.add_argument("--allowlist", required=True, type=Path)
    parser.add_argument("--minimum-cases", type=int, default=46)
    parser.add_argument("--case-baseline", type=Path)
    args = parser.parse_args()

    result = check_ratchet(
        args.report,
        args.allowlist,
        args.minimum_cases,
        args.case_baseline,
    )
    if result.errors:
        print("FAIL: exception-parity ratchet violations:", file=sys.stderr)
        for error in result.errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    print(
        "OK: exception-parity ratchet intact "
        f"({result.comparable_count} comparable cases, "
        f"{result.divergence_count} issue-linked divergence(s), two-sided ratchet)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
