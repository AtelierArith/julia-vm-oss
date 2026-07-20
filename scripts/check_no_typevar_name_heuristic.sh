#!/usr/bin/env bash
# check_no_typevar_name_heuristic.sh - ban name-shape TypeVar inference.
#
# TypeVar-ness must come from a `where` / type-parameter environment, not from
# an identifier spelling such as `^[A-Z][0-9]*$` (Issue #9563).
set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

python3 - <<'PY'
from pathlib import Path
import re
import sys

ROOT = Path(".")
SOURCES = [
    ROOT / "subset_julia_vm_types/src/types/julia_type/parsing.rs",
    ROOT / "subset_julia_vm_types/src/types/julia_type/comparison.rs",
    ROOT / "subset_julia_vm_types/src/inference_core/type_core.rs",
]

patterns = [
    (
        re.compile(r"\bfn\s+is_type_variable_name\s*\("),
        "type-variable name heuristic function",
    ),
    (
        re.compile(r"\bis_type_variable_name\s*\("),
        "type-variable name heuristic call",
    ),
    (
        re.compile(r"\bis_ascii_uppercase\s*\(\)"),
        "uppercase identifier-shape TypeVar test",
    ),
    (
        re.compile(r"\bis_ascii_digit\s*\(\)"),
        "digit suffix TypeVar test",
    ),
]

failures = []
for path in SOURCES:
    text = path.read_text()
    for lineno, line in enumerate(text.splitlines(), 1):
        for pattern, reason in patterns:
            if pattern.search(line):
                failures.append((path, lineno, reason, line.strip()))

if failures:
    print("FAIL: type-variable name heuristic remains (Issue #9563).", file=sys.stderr)
    print(
        "      TypeVar resolution must use explicit where/type-parameter scope, not identifier spelling.",
        file=sys.stderr,
    )
    for path, lineno, reason, line in failures:
        print(f"      {path}:{lineno}: {reason}: {line}", file=sys.stderr)
    sys.exit(1)

print("OK: no name-shape TypeVar heuristic found.")
PY
