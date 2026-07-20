#!/usr/bin/env python3
"""Shared reporting helpers for iOS E2E sample sweeps.

The public status vocabulary is intentionally small:

* sample_pass: the sample ran and no sample-level failure was detected
* sample_fail: the app ran the sample and the sample itself failed
* infra_failure: the harness could not produce a trustworthy sample verdict
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path

SAMPLE_PASS = "sample_pass"
SAMPLE_FAIL = "sample_fail"
INFRA_FAILURE = "infra_failure"
STATUSES = (SAMPLE_PASS, SAMPLE_FAIL, INFRA_FAILURE)

LEGACY_STATUS_MAP = {
    "PASS": SAMPLE_PASS,
    "DONE": SAMPLE_PASS,
    "FAIL": SAMPLE_FAIL,
    "UNKNOWN": INFRA_FAILURE,
    "ERROR": INFRA_FAILURE,
}


@dataclass(frozen=True)
class ReportRow:
    sample_id: str
    status: str
    detail: str
    attempts: int = 1


@dataclass(frozen=True)
class ReportSummary:
    sample_pass: int
    sample_fail: int
    infra_failure: int

    @property
    def sample_total(self) -> int:
        return self.sample_pass + self.sample_fail

    @property
    def total(self) -> int:
        return self.sample_total + self.infra_failure

    @property
    def sample_rate(self) -> float:
        if self.sample_total == 0:
            return 0.0
        return round(100.0 * self.sample_pass / self.sample_total, 2)

    @property
    def infra_rate(self) -> float:
        if self.total == 0:
            return 0.0
        return round(100.0 * self.infra_failure / self.total, 2)


def normalize_status(status: str) -> str:
    status = status.strip()
    if status in STATUSES:
        return status
    if status in LEGACY_STATUS_MAP:
        return LEGACY_STATUS_MAP[status]
    raise ValueError(f"unknown iOS E2E status: {status}")


def should_retry(status: str, *, attempt: int, max_attempts: int) -> bool:
    return normalize_status(status) == INFRA_FAILURE and attempt < max_attempts


def summarize(rows: list[ReportRow]) -> ReportSummary:
    counts = {status: 0 for status in STATUSES}
    for row in rows:
        counts[normalize_status(row.status)] += 1
    return ReportSummary(
        sample_pass=counts[SAMPLE_PASS],
        sample_fail=counts[SAMPLE_FAIL],
        infra_failure=counts[INFRA_FAILURE],
    )


def parse_report(path: str | Path) -> ReportSummary:
    rows: list[ReportRow] = []
    with Path(path).open(encoding="utf-8") as fh:
        for raw in fh:
            line = raw.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t", 3)
            if len(parts) < 3:
                continue
            status, sample_id, detail = parts[:3]
            attempts = int(parts[3]) if len(parts) == 4 and parts[3].isdigit() else 1
            rows.append(
                ReportRow(
                    sample_id=sample_id,
                    status=normalize_status(status),
                    detail=detail,
                    attempts=attempts,
                )
            )
    return summarize(rows)


def write_report(path: str | Path, rows: list[ReportRow]) -> ReportSummary:
    summary = summarize(rows)
    with Path(path).open("w", encoding="utf-8") as fh:
        fh.write("# status\tsample_id\tdetail\tattempts\n")
        for row in rows:
            fh.write(
                f"{normalize_status(row.status)}\t{row.sample_id}\t{row.detail}\t{row.attempts}\n"
            )
        fh.write(
            f"# summary\tsample_pass={summary.sample_pass}\t"
            f"sample_fail={summary.sample_fail}\t"
            f"infra_failure={summary.infra_failure}\t"
            f"sample_rate={summary.sample_rate:.2f}\t"
            f"infra_rate={summary.infra_rate:.2f}\n"
        )
    return summary


def format_summary(summary: ReportSummary) -> str:
    return (
        f"sample_pass={summary.sample_pass}\n"
        f"sample_fail={summary.sample_fail}\n"
        f"infra_failure={summary.infra_failure}\n"
        f"sample_total={summary.sample_total}\n"
        f"total={summary.total}\n"
        f"sample_rate={summary.sample_rate:.2f}\n"
        f"infra_rate={summary.infra_rate:.2f}\n"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description="Summarize iOS E2E 3-value reports.")
    parser.add_argument("--summary", metavar="REPORT", help="Print key=value summary for report.txt.")
    args = parser.parse_args()

    if args.summary:
        print(format_summary(parse_report(args.summary)), end="")


if __name__ == "__main__":
    main()
