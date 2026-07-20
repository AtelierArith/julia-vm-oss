#!/usr/bin/env python3
"""Check Criterion estimates against stored benchmark thresholds.

Usage:
    python3 benchmarks/scripts/check_criterion_thresholds.py \
        benchmarks/baselines/vm_calc_pi_thresholds.json target/criterion

Criterion writes each benchmark estimate to:
    target/criterion/<criterion_id>/new/estimates.json

The estimate point value is in nanoseconds. Baselines are stored in seconds.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


def load_json(path: Path) -> object:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def mean_seconds(criterion_root: Path, criterion_id: str) -> float:
    estimates_path = criterion_root / criterion_id / "new" / "estimates.json"
    data = load_json(estimates_path)
    try:
        nanos = data["mean"]["point_estimate"]
    except (KeyError, TypeError) as exc:
        raise ValueError(f"{estimates_path} does not look like Criterion estimates.json") from exc
    return float(nanos) / 1_000_000_000.0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("baseline", type=Path)
    parser.add_argument("criterion_root", type=Path)
    args = parser.parse_args()

    baseline = load_json(args.baseline)
    benchmarks = baseline.get("benchmarks") if isinstance(baseline, dict) else None
    if not isinstance(benchmarks, list):
        print(f"ERROR: {args.baseline} must contain a benchmarks array", file=sys.stderr)
        return 2

    failures: list[str] = []
    for entry in benchmarks:
        criterion_id = entry["criterion_id"]
        baseline_seconds = float(entry["baseline_seconds"])
        max_regression_ratio = float(entry["max_regression_ratio"])
        allowed_seconds = baseline_seconds * (1.0 + max_regression_ratio)
        actual_seconds = mean_seconds(args.criterion_root, criterion_id)
        print(
            f"{criterion_id}: actual={actual_seconds:.6f}s "
            f"baseline={baseline_seconds:.6f}s allowed={allowed_seconds:.6f}s"
        )
        if actual_seconds > allowed_seconds:
            failures.append(
                f"{criterion_id} {actual_seconds:.6f}s exceeds allowed {allowed_seconds:.6f}s"
            )

    if failures:
        print("ERROR: benchmark threshold failure(s):", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
