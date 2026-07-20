#!/usr/bin/env bash
# check_panic_free_production_baseline.sh — production-lane panic-source gate
# (Issue #10908 Phase 3 of the #10869 panic-debt retirement epic).
#
# `scripts/check_panic_free_ratchet.sh` (Issue #8706) ratchets RAW per-module
# `.unwrap()`/`.expect()`/`panic!()`/`todo!()`/`unimplemented!()` counts,
# mixing test-only, build-time, cache-boundary, and user-input-reachable
# source in the same row — a module with 200 test-only `.unwrap()` calls and
# a NEW real one in production code both just "bump the row's count", so a
# single-digit new production panic can hide inside a large pre-existing
# test-only baseline. `scripts/panic_debt_classification.py` (Issue #10869
# Phase 0) already buckets every site into test-only / build-time-invariant /
# cache-corruption-boundary / user-input-reachable with per-file rules and
# whole-file/inline `#[cfg(test)]` closure detection — this script is the
# "successor mechanism" the epic's acceptance criteria calls for: it reuses
# that exact classification (imported, not reimplemented) and gates ONLY the
# `user-input-reachable` bucket ("production") against
# `docs/vm/PANIC_FREE_PRODUCTION_BASELINE.tsv`, whose target is 0 — any
# remaining nonzero row must be listed with a `reason` column linking the
# issue that justifies it (parser `self.expect(Token)` false positive,
# AoT codegen-string templates, `aot_throw` design divergence, the
# `vm/exec/mod.rs` SystemTime exception, doc-comment/rustdoc examples, and
# this issue's own `expr_heads.rs` compiler-invariant allow). A NEW
# production hit not already in the allowlist, or an allowlisted row whose
# count grew, fails.
#
# `docs/vm/PANIC_FREE_RATCHET_BASELINE.tsv` / `PANIC_FREE_DENY_MODULES.tsv`
# are untouched by this script and remain the test/build-time-lane
# enforcement (broad monotonic ratchet + module-level deny pragmas).
#
# Usage:
#   bash scripts/check_panic_free_production_baseline.sh
#   bash scripts/check_panic_free_production_baseline.sh --update
#     rewrite docs/vm/PANIC_FREE_PRODUCTION_BASELINE.tsv to the current
#     user-input-reachable (metric, module) counts, preserving each existing
#     row's `reason` text (new rows get a "NEEDS REASON — Issue #NNNN" stub
#     that must be filled in by hand before the row is accepted as
#     justified). Never drops a row automatically to 0 silently — a count
#     that reaches 0 is removed (nothing left to justify), matching
#     check_panic_free_ratchet.sh's `--update` UX.

set -euo pipefail

cd "$(dirname "$0")/.."

PRODUCTION_BASELINE="${PANIC_PRODUCTION_BASELINE:-docs/vm/PANIC_FREE_PRODUCTION_BASELINE.tsv}"
PRODUCTION_UPDATE=0
if [[ "${1:-}" == "--update" ]]; then
  PRODUCTION_UPDATE=1
fi

export PRODUCTION_BASELINE PRODUCTION_UPDATE

python3 - <<'PY'
from __future__ import annotations

import os
import sys
from pathlib import Path

sys.path.insert(0, "scripts")
import panic_debt_classification as classifier  # noqa: E402

USER_INPUT_REACHABLE = classifier.USER_INPUT_REACHABLE


def current_production_counts() -> dict[tuple[str, str], int]:
    files = classifier.rust_files()
    bucket_counts, _module_metric_totals = classifier.classify(files)
    counts: dict[tuple[str, str], int] = {}
    for (bucket, metric, module), n in bucket_counts.items():
        if bucket != USER_INPUT_REACHABLE:
            continue
        if metric not in ("unwrap_call", "expect_call", "panic_macro"):
            continue
        counts[(metric, module)] = n
    return counts


def parse_baseline(path: Path) -> dict[tuple[str, str], tuple[int, str]]:
    baseline: dict[tuple[str, str], tuple[int, str]] = {}
    if not path.exists():
        return baseline
    for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip() or line.startswith("metric\t"):
            continue
        fields = line.split("\t")
        if len(fields) != 4:
            raise SystemExit(f"malformed production baseline row {path}:{lineno}: {line}")
        metric, module, count, reason = fields
        baseline[(metric, module)] = (int(count), reason)
    return baseline


def write_baseline(path: Path, counts: dict[tuple[str, str], int], old: dict[tuple[str, str], tuple[int, str]]) -> None:
    lines = ["metric\tmodule\tbaseline\treason"]
    for key in sorted(counts):
        metric, module = key
        n = counts[key]
        _old_n, reason = old.get(key, (0, ""))
        if not reason:
            reason = "NEEDS REASON — Issue #10908: fill in why this production hit is justified/deferred before merging"
        lines.append(f"{metric}\t{module}\t{n}\t{reason}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    baseline_path = Path(os.environ["PRODUCTION_BASELINE"])
    counts = current_production_counts()

    if os.environ.get("PRODUCTION_UPDATE") == "1":
        old = parse_baseline(baseline_path)
        write_baseline(baseline_path, counts, old)
        all_keys = sorted(set(old) | set(counts))
        changes = []
        for key in all_keys:
            old_n = old.get(key, (0, ""))[0]
            new_n = counts.get(key, 0)
            if old_n != new_n:
                changes.append((key, old_n, new_n))
        if changes:
            print(f"Updated {len(changes)} row(s) in {baseline_path}:")
            for (metric, module), old_n, new_n in changes:
                direction = "bumped" if new_n > old_n else "tightened"
                print(f"  {metric}\t{module}: {old_n} -> {new_n} ({direction})")
            needs_reason = [k for k in counts if k not in old]
            if needs_reason:
                print(
                    "NEW row(s) need a real 'reason' column filled in by hand "
                    "(placeholder 'NEEDS REASON' written) before this can pass:",
                    file=sys.stderr,
                )
                for metric, module in needs_reason:
                    print(f"  {metric}\t{module}", file=sys.stderr)
        else:
            print("No production baseline changes needed; every row already matches its count.")
        print("Re-run without --update to confirm a green check.")
        return 0

    if not baseline_path.exists():
        print(f"missing panic-free production baseline: {baseline_path}", file=sys.stderr)
        return 2

    baseline = parse_baseline(baseline_path)
    failures = 0
    for key, actual in sorted(counts.items()):
        if actual <= 0:
            continue
        if key not in baseline:
            metric, module = key
            print(
                f"panic-free PRODUCTION baseline: NEW unallowlisted user-input-reachable "
                f"{metric} in {module}: {actual}. File an Issue, add a "
                f"docs/vm/PANIC_FREE_PRODUCTION_BASELINE.tsv row with a linked reason, "
                "or fix it (Issue #10908).",
                file=sys.stderr,
            )
            failures += 1
            continue
        allowed, reason = baseline[key]
        if actual > allowed:
            metric, module = key
            print(
                f"panic-free PRODUCTION baseline exceeded for {metric} {module}: "
                f"{actual} > {allowed} (allowlisted reason: {reason or '(none)'})",
                file=sys.stderr,
            )
            failures += 1
    for key, (allowed, _reason) in sorted(baseline.items()):
        actual = counts.get(key, 0)
        if actual < allowed:
            metric, module = key
            print(
                f"NOTE: panic-free production baseline can tighten for {metric} {module}: "
                f"{actual} < {allowed}",
                file=sys.stderr,
            )
        if allowed == 0:
            metric, module = key
            print(
                f"NOTE: panic-free production baseline row for {metric} {module} has "
                "baseline=0 — drop this row, it no longer needs an allowlist entry",
                file=sys.stderr,
            )

    if failures:
        print(f"panic-free production baseline failed with {failures} issue(s)", file=sys.stderr)
        return 1

    total = sum(n for n, _ in baseline.values())
    print(
        f"Panic-free PRODUCTION baseline (Issue #10908 Phase 3 of #10869): OK "
        f"({len(baseline)} allowlisted row(s), {total} total justified hit(s))"
    )
    return 0


raise SystemExit(main())
PY
