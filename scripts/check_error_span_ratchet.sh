#!/usr/bin/env bash
# check_error_span_ratchet.sh — static span-less VM error path ratchet (Issue #8712).
#
# This is a fast inventory gate for paths that can construct or export runtime
# errors without an attached source span. It is intentionally a ratchet: counts
# may go down as constructors become span-requiring, but new span-less paths
# must either be eliminated or added to the baseline with a reason.

set -euo pipefail

cd "$(dirname "$0")/.."

ERROR_SPAN_BASELINE="${ERROR_SPAN_BASELINE:-docs/vm/ERROR_SPAN_RATCHET_BASELINE.tsv}"
ERROR_SPAN_ROOTS="${ERROR_SPAN_ROOTS:-subset_julia_vm/src:subset_julia_vm_ffi/src}"

export ERROR_SPAN_BASELINE ERROR_SPAN_ROOTS

python3 - <<'PY'
from __future__ import annotations

import os
import re
import sys
from collections import Counter
from pathlib import Path

PATTERNS = {
    "direct_vmerror_err": re.compile(r"(?<![A-Za-z0-9_])(?:return\s+)?Err\s*\(\s*VmError::"),
    "vmerror_map_err": re.compile(r"\.map_err\s*\([^;\n]*VmError::"),
    "spanned_from_error_constructor": re.compile(r"pub\s+fn\s+from_error\s*\(\s*error:\s*VmError"),
    "spanned_from_vmerror_impl": re.compile(r"impl\s+From\s*<\s*VmError\s*>\s+for\s+SpannedVmError"),
    "ffi_empty_span": re.compile(r"CSpan::empty\s*\("),
    "ffi_runtime_without_span": re.compile(r"CError::runtime\s*\("),
    "ffi_compile_without_span": re.compile(r"CError::compile\s*\("),
}

REASONS = {
    "direct_vmerror_err": "direct Err(VmError) bypasses span attachment unless caller recovers instruction span",
    "vmerror_map_err": "map_err constructs VmError before any span-bearing builder",
    "spanned_from_error_constructor": "constructor permits SpannedVmError without span",
    "spanned_from_vmerror_impl": "From<VmError> permits implicit SpannedVmError without span",
    "ffi_empty_span": "FFI exports an empty CSpan for some error kinds",
    "ffi_runtime_without_span": "FFI runtime error conversion lacks a span parameter",
    "ffi_compile_without_span": "FFI compile error conversion lacks a span parameter",
}


def module_key(path: Path) -> str:
    rel = path.resolve().relative_to(Path.cwd().resolve()).as_posix()
    parts = rel.split("/")
    if len(parts) >= 3 and parts[0] == "subset_julia_vm" and parts[1] == "src":
        if parts[2] == "vm" and len(parts) >= 4:
            return "/".join(parts[:4])
        if parts[2] == "bin" and len(parts) >= 4:
            return "/".join(parts[:4])
        return "/".join(parts[:3])
    if len(parts) >= 3 and parts[0] == "subset_julia_vm_ffi" and parts[1] == "src":
        return "/".join(parts[:3])
    return "/".join(parts[:2])


def rust_files() -> list[Path]:
    files: list[Path] = []
    for raw in os.environ["ERROR_SPAN_ROOTS"].split(":"):
        if not raw:
            continue
        root = Path(raw)
        if root.is_file() and root.suffix == ".rs":
            files.append(root)
        elif root.is_dir():
            files.extend(sorted(root.rglob("*.rs")))
    return sorted(set(files))


def strip_line_comment(line: str) -> str:
    if "//" not in line:
        return line
    return line.split("//", 1)[0]


def current_counts() -> Counter[tuple[str, str]]:
    counts: Counter[tuple[str, str]] = Counter()
    for path in rust_files():
        text = "\n".join(strip_line_comment(line) for line in path.read_text(
            encoding="utf-8", errors="ignore"
        ).splitlines())
        module = module_key(path)
        for metric, pattern in PATTERNS.items():
            hits = len(pattern.findall(text))
            if hits:
                counts[(metric, module)] += hits
    return counts


def parse_baseline(path: Path) -> dict[tuple[str, str], tuple[int, str]]:
    baseline: dict[tuple[str, str], tuple[int, str]] = {}
    for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip() or line.startswith("metric\t"):
            continue
        fields = line.split("\t")
        if len(fields) != 4:
            raise SystemExit(f"malformed baseline row {path}:{lineno}: {line}")
        metric, module, count, reason = fields
        baseline[(metric, module)] = (int(count), reason)
    return baseline


def print_current(counts: Counter[tuple[str, str]]) -> None:
    print("metric\tmodule\tbaseline\treason")
    for metric, module in sorted(counts):
        print(f"{metric}\t{module}\t{counts[(metric, module)]}\t{REASONS[metric]}")


def main() -> int:
    baseline_path = Path(os.environ["ERROR_SPAN_BASELINE"])
    counts = current_counts()

    if os.environ.get("ERROR_SPAN_RATCHET_PRINT_CURRENT") == "1":
        print_current(counts)
        return 0

    if not baseline_path.exists():
        print(f"missing error-span baseline: {baseline_path}", file=sys.stderr)
        return 2

    baseline = parse_baseline(baseline_path)
    failures = 0
    for key, actual in sorted(counts.items()):
        allowed, _reason = baseline.get(key, (0, ""))
        if actual > allowed:
            metric, module = key
            print(
                f"error-span ratchet exceeded for {metric} {module}: {actual} > {allowed}",
                file=sys.stderr,
            )
            failures += 1
    for key, (allowed, _reason) in sorted(baseline.items()):
        actual = counts.get(key, 0)
        if actual < allowed:
            metric, module = key
            print(
                f"NOTE: error-span ratchet can tighten for {metric} {module}: {actual} < {allowed}",
                file=sys.stderr,
            )

    if failures:
        print(f"error-span ratchet failed with {failures} issue(s)", file=sys.stderr)
        return 1

    print("Error-span ratchet (Issue #8712): OK")
    return 0


raise SystemExit(main())
PY
