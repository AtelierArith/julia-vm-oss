#!/usr/bin/env bash
# check_panic_free_ratchet.sh — fast panic-source module ratchet (Issue #8706).
#
# This audit is intentionally static and fast enough for the check_*.sh batch.
# The heavier Clippy warning inventory remains available through
# scripts/panic_free_inventory.sh --run-clippy.
#
# Usage:
#   bash scripts/check_panic_free_ratchet.sh
#   bash scripts/check_panic_free_ratchet.sh --update   # rewrite
#     docs/vm/PANIC_FREE_RATCHET_BASELINE.tsv to the current per-module counts
#     (bumps AND tightens every row); prints what changed vs. the old
#     baseline. Does NOT touch docs/vm/PANIC_FREE_DENY_MODULES.tsv — a missing
#     `#![deny(...)]` pragma has no baseline to bump and must be fixed by
#     hand. Part of the one-command baseline refresh for Issue #10870; run
#     this, review the diff, then re-run without --update to confirm green.

set -euo pipefail

cd "$(dirname "$0")/.."

PANIC_RATCHET_BASELINE="${PANIC_RATCHET_BASELINE:-docs/vm/PANIC_FREE_RATCHET_BASELINE.tsv}"
PANIC_DENY_MODULES="${PANIC_DENY_MODULES:-docs/vm/PANIC_FREE_DENY_MODULES.tsv}"
PANIC_RATCHET_ROOTS="${PANIC_RATCHET_ROOTS:-subset_julia_vm/src:subset_julia_vm_compile/src:subset_julia_vm_lowering/src:subset_julia_vm_vm/src:subset_julia_vm_ffi/src:subset_julia_vm_parser/src:subset_julia_vm_runtime/src:subset_julia_vm_web/src:build.rs:subset_julia_vm/build.rs:subset_julia_vm_compile/build.rs}"
PANIC_RATCHET_UPDATE=0
if [[ "${1:-}" == "--update" ]]; then
  PANIC_RATCHET_UPDATE=1
fi

export PANIC_RATCHET_BASELINE PANIC_DENY_MODULES PANIC_RATCHET_ROOTS PANIC_RATCHET_UPDATE

python3 - <<'PY'
from __future__ import annotations

import os
import re
import sys
from collections import Counter
from pathlib import Path

PATTERNS = {
    "unwrap_call": re.compile(r"\.unwrap\s*\("),
    "expect_call": re.compile(r"\.expect\s*\("),
    "panic_macro": re.compile(r"(?<![A-Za-z0-9_])panic!\s*\("),
    "todo_macro": re.compile(r"(?<![A-Za-z0-9_])todo!\s*\("),
    "unimplemented_macro": re.compile(r"(?<![A-Za-z0-9_])unimplemented!\s*\("),
}


def rel(path: Path) -> str:
    try:
        return path.relative_to(Path.cwd()).as_posix()
    except ValueError:
        return path.as_posix()


def module_key(path: str) -> str:
    parts = path.replace("\\", "/").split("/")
    if len(parts) >= 5 and parts[0] == "subset_julia_vm_vm" and parts[1:3] == ["src", "vm"]:
        if parts[3] == "exec":
            return "/".join(parts[:5])
        return "/".join(parts[:4])
    if len(parts) >= 4 and parts[0] == "subset_julia_vm" and parts[1] == "src":
        if parts[2] == "vm" and len(parts) >= 5:
            return "/".join(parts[:5]) if parts[3] == "exec" else "/".join(parts[:4])
        return "/".join(parts[:3])
    if len(parts) >= 3 and parts[0].startswith("subset_julia_vm"):
        return "/".join(parts[:3])
    return "/".join(parts[:2])


def rust_files() -> list[Path]:
    files: list[Path] = []
    for raw in os.environ["PANIC_RATCHET_ROOTS"].split(":"):
        if not raw:
            continue
        root = Path(raw)
        if root.is_file() and root.suffix == ".rs":
            files.append(root)
        elif root.is_dir():
            files.extend(sorted(root.rglob("*.rs")))
    return sorted(set(files))


def module_for(path: Path, roots: list[Path]) -> str:
    path = path.resolve()
    cwd = Path.cwd().resolve()
    try:
        return module_key(path.relative_to(cwd).as_posix())
    except ValueError:
        pass
    for root in roots:
        resolved = root.resolve()
        if resolved.is_dir():
            try:
                path.relative_to(resolved)
                return resolved.name
            except ValueError:
                continue
    return path.parent.name


def current_counts() -> Counter[tuple[str, str]]:
    roots = [Path(raw) for raw in os.environ["PANIC_RATCHET_ROOTS"].split(":") if raw]
    counts: Counter[tuple[str, str]] = Counter()
    for path in rust_files():
        text = path.read_text(encoding="utf-8", errors="ignore")
        module = module_for(path, roots)
        for metric, pattern in PATTERNS.items():
            hits = len(pattern.findall(text))
            if hits:
                counts[(metric, module)] += hits
    return counts


def parse_baseline(path: Path) -> dict[tuple[str, str], int]:
    baseline: dict[tuple[str, str], int] = {}
    for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip() or line.startswith("metric\t"):
            continue
        fields = line.split("\t")
        if len(fields) != 3:
            raise SystemExit(f"malformed baseline row {path}:{lineno}: {line}")
        metric, module, count = fields
        baseline[(metric, module)] = int(count)
    return baseline


def resolve_deny_file(raw: str, roots: list[Path]) -> Path:
    direct = Path(raw)
    if direct.exists():
        return direct
    for root in roots:
        candidate = root.parent / raw
        if candidate.exists():
            return candidate
    return direct


def check_deny_modules(path: Path) -> int:
    roots = [Path(raw) for raw in os.environ["PANIC_RATCHET_ROOTS"].split(":") if raw]
    failures = 0
    for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip() or line.startswith("file\t"):
            continue
        fields = line.split("\t")
        if len(fields) < 4:
            print(f"malformed deny row {path}:{lineno}: {line}", file=sys.stderr)
            failures += 1
            continue
        raw_file, unwrap_required, expect_required, _reason = fields[:4]
        file_path = resolve_deny_file(raw_file, roots)
        if not file_path.exists():
            print(f"panic-free deny module is missing: {raw_file}", file=sys.stderr)
            failures += 1
            continue
        text = file_path.read_text(encoding="utf-8", errors="ignore")
        if unwrap_required == "true" and "#![deny(clippy::unwrap_used)]" not in text:
            print(f"missing unwrap_used deny: {raw_file}", file=sys.stderr)
            failures += 1
        if expect_required == "true" and "#![deny(clippy::expect_used)]" not in text:
            print(f"missing expect_used deny: {raw_file}", file=sys.stderr)
            failures += 1
    return failures


def print_current(counts: Counter[tuple[str, str]]) -> None:
    print("metric\tmodule\tbaseline")
    for metric, module in sorted(counts):
        print(f"{metric}\t{module}\t{counts[(metric, module)]}")


def write_baseline(baseline_path: Path, counts: Counter[tuple[str, str]]) -> None:
    lines = ["metric\tmodule\tbaseline"]
    for metric, module in sorted(counts):
        lines.append(f"{metric}\t{module}\t{counts[(metric, module)]}")
    baseline_path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    baseline_path = Path(os.environ["PANIC_RATCHET_BASELINE"])
    deny_path = Path(os.environ["PANIC_DENY_MODULES"])
    counts = current_counts()

    if os.environ.get("PANIC_RATCHET_PRINT_CURRENT") == "1":
        print_current(counts)
        return 0

    if os.environ.get("PANIC_RATCHET_UPDATE") == "1":
        # --update (Issue #10870): rewrite the baseline TSV to the current
        # per-(metric, module) counts — bumping over-baseline rows AND
        # tightening under-baseline rows (including dropping rows whose count
        # is now 0) in one mechanical pass. docs/vm/PANIC_FREE_DENY_MODULES.tsv
        # is untouched; a missing deny pragma has no baseline to bump.
        old_baseline: dict[tuple[str, str], int] = {}
        if baseline_path.exists():
            old_baseline = parse_baseline(baseline_path)
        write_baseline(baseline_path, counts)

        all_keys = sorted(set(old_baseline) | set(counts))
        changes = []
        for key in all_keys:
            old = old_baseline.get(key, 0)
            new = counts.get(key, 0)
            if old != new:
                changes.append((key, old, new))
        if changes:
            print(f"Updated {len(changes)} row(s) in {baseline_path}:")
            for (metric, module), old, new in changes:
                direction = "bumped" if new > old else "tightened"
                print(f"  {metric}\t{module}: {old} -> {new} ({direction})")
        else:
            print("No baseline changes needed; every row already matches its count.")

        deny_failures = check_deny_modules(deny_path) if deny_path.exists() else 0
        if deny_failures:
            print(
                f"panic-free ratchet: {deny_failures} deny-pragma issue(s) remain "
                "— --update does not fix these; edit the flagged module(s) by hand.",
                file=sys.stderr,
            )
            return 1
        print("Re-run without --update to confirm a green check.")
        return 0

    if not baseline_path.exists():
        print(f"missing panic-free ratchet baseline: {baseline_path}", file=sys.stderr)
        return 2
    if not deny_path.exists():
        print(f"missing panic-free deny module list: {deny_path}", file=sys.stderr)
        return 2

    baseline = parse_baseline(baseline_path)
    failures = 0
    for key, actual in sorted(counts.items()):
        allowed = baseline.get(key, 0)
        if actual > allowed:
            metric, module = key
            print(
                f"panic-free ratchet exceeded for {metric} {module}: {actual} > {allowed}",
                file=sys.stderr,
            )
            failures += 1
    for key, allowed in sorted(baseline.items()):
        actual = counts.get(key, 0)
        if actual < allowed:
            metric, module = key
            print(
                f"NOTE: panic-free ratchet can tighten for {metric} {module}: {actual} < {allowed}",
                file=sys.stderr,
            )

    failures += check_deny_modules(deny_path)
    if failures:
        print(f"panic-free ratchet failed with {failures} issue(s)", file=sys.stderr)
        return 1

    print("Panic-free ratchet (Issue #8706): OK")
    return 0


raise SystemExit(main())
PY
