#!/usr/bin/env python3
"""Append a nightly row per benchmark to benchmarks/results/perf_gate.tsv.

Usage:
    python3 benchmarks/scripts/append_perf_gate_tsv.py \
        benchmarks/baselines/multi_bench_nightly_thresholds.json \
        target/criterion \
        <date>          \\   # YYYY-MM-DD
        <commit>            # short git SHA

Writes/appends to benchmarks/results/perf_gate.tsv.
Columns: date, commit, criterion_id, mean_s, baseline_s, ratio, status
status = PASS | FAIL | MISSING (estimates.json not found — bench did not run)

Exit 0 always: this step is append-only; threshold enforcement is done by
check_criterion_thresholds.py in the preceding step.
"""

from __future__ import annotations

import json
import pathlib
import sys


def main() -> int:
    if len(sys.argv) < 5:
        print(
            f"Usage: {sys.argv[0]} <baseline.json> <criterion_root> <date> <commit>",
            file=sys.stderr,
        )
        return 2

    baseline_path = pathlib.Path(sys.argv[1])
    criterion_root = pathlib.Path(sys.argv[2])
    date_str = sys.argv[3]
    commit_str = sys.argv[4]

    tsv_path = pathlib.Path("benchmarks/results/perf_gate.tsv")
    tsv_path.parent.mkdir(parents=True, exist_ok=True)

    header = "date\tcommit\tcriterion_id\tmean_s\tbaseline_s\tratio\tstatus\n"
    if not tsv_path.exists():
        tsv_path.write_text(header, encoding="utf-8")

    baselines = json.loads(baseline_path.read_text(encoding="utf-8"))["benchmarks"]
    rows_written = 0

    with tsv_path.open("a", encoding="utf-8") as f:
        for entry in baselines:
            cid = entry["criterion_id"]
            baseline_s = float(entry["baseline_seconds"])
            estimates_path = criterion_root / cid / "new" / "estimates.json"

            if not estimates_path.exists():
                f.write(
                    f"{date_str}\t{commit_str}\t{cid}\tN/A\t{baseline_s:.6f}\tN/A\tMISSING\n"
                )
                rows_written += 1
                print(f"  MISSING {cid} (estimates.json not found)")
                continue

            data = json.loads(estimates_path.read_text(encoding="utf-8"))
            mean_s = float(data["mean"]["point_estimate"]) / 1_000_000_000.0
            ratio = mean_s / baseline_s if baseline_s > 0 else float("inf")
            max_ratio = float(entry["max_regression_ratio"])
            status = "PASS" if mean_s <= baseline_s * (1.0 + max_ratio) else "FAIL"
            f.write(
                f"{date_str}\t{commit_str}\t{cid}\t{mean_s:.6f}\t{baseline_s:.6f}\t{ratio:.4f}\t{status}\n"
            )
            rows_written += 1
            print(f"  {status} {cid}: {mean_s:.6f}s (baseline {baseline_s:.6f}s, ratio {ratio:.4f})")

    print(f"Wrote {rows_written} TSV rows to {tsv_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
