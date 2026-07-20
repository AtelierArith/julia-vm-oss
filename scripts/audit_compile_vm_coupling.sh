#!/usr/bin/env bash
# audit_compile_vm_coupling.sh
#
# Issue #8449: ratchet the existing compile <-> VM coupling while the runtime
# type-system facade and backend boundaries are split out incrementally.
#
# Usage:
#   bash scripts/audit_compile_vm_coupling.sh

set -euo pipefail

if [[ ! -d subset_julia_vm_compile/src/compile || ! -d subset_julia_vm_vm/src/vm ]]; then
  echo "ERROR: subset_julia_vm_compile/src/compile or subset_julia_vm_vm/src/vm not found. Run from the repository root." >&2
  exit 1
fi

if [[ ! -f subset_julia_vm/src/runtime_types.rs ]]; then
  echo "ERROR: subset_julia_vm/src/runtime_types.rs is missing; Issue #8449 requires a runtime type facade." >&2
  exit 1
fi

if ! grep -Eq 'pub\(crate\) mod runtime_types;' subset_julia_vm/src/lib.rs; then
  echo "ERROR: subset_julia_vm/src/lib.rs must declare pub(crate) mod runtime_types;" >&2
  exit 1
fi

python3 - <<'PY'
from pathlib import Path
import re
import sys

CHECKS = [
    {
        "name": "compile_to_vm",
        "root": Path("subset_julia_vm_compile/src/compile"),
        # Issue #8837: compiler bytecode generation now imports the staging
        # bytecode facade (`crate::bytecode`) rather than interpreter modules.
        # Keep direct compile -> vm references at zero until the facade moves
        # into its own crate.
        "limit": 0,
        "pattern": re.compile(
            r"\b(?:crate::vm|super::super::vm|super::vm)::"
            r"|use\s+crate::vm\b|use\s+crate::vm::"
        ),
        "message": "compile currently emits stack-VM bytecode directly; do not add new direct VM references.",
    },
    {
        "name": "vm_to_compile",
        "root": Path("subset_julia_vm_vm/src/vm"),
        "limit": 0,
        "test_limit": 0,
        "pattern": re.compile(
            r"\b(?:crate::compile|super::super::compile|super::compile)::"
            r"|use\s+crate::compile\b|use\s+crate::compile::"
        ),
        "message": "VM runtime code must not grow new dependencies on compiler internals.",
    },
]


def brace_delta(line: str) -> int:
    return line.count("{") - line.count("}")


def rust_lines(root: Path):
    for path in sorted(root.rglob("*.rs")):
        text = path.read_text(encoding="utf-8", errors="ignore")
        depth = 0
        pending_cfg_test = False
        test_module_depths = []
        for line_no, line in enumerate(text.splitlines(), 1):
            stripped = line.strip()
            in_test_context = path.name == "tests.rs" or bool(test_module_depths)
            if stripped.startswith("//") or stripped.startswith("*"):
                continue
            yield path, line_no, line, in_test_context

            if stripped == "#[cfg(test)]":
                pending_cfg_test = True
            elif pending_cfg_test and stripped.startswith("#["):
                pass
            elif pending_cfg_test and re.search(r"\bmod\b", stripped) and "{" in stripped:
                test_module_depths.append(depth + brace_delta(line))
                pending_cfg_test = False
            elif pending_cfg_test and stripped:
                pending_cfg_test = False

            depth += brace_delta(line)
            while test_module_depths and depth < test_module_depths[-1]:
                test_module_depths.pop()


failed = False
for check in CHECKS:
    matches = [
        (path, line_no, line.strip(), in_test_context)
        for path, line_no, line, in_test_context in rust_lines(check["root"])
        if check["pattern"].search(line)
    ]
    if "test_limit" in check:
        runtime_matches = [m for m in matches if not m[3]]
        test_matches = [m for m in matches if m[3]]
        count = len(runtime_matches)
        test_count = len(test_matches)
    else:
        runtime_matches = matches
        test_matches = []
        count = len(matches)
        test_count = 0
    limit = check["limit"]
    print(f"{check['name']}: {count} references (baseline limit {limit})")
    if count > limit:
        failed = True
        print(f"ERROR: {check['message']}", file=sys.stderr)
        print(f"ERROR: {check['name']} increased from {limit} to {count}.", file=sys.stderr)
        for path, line_no, line, _ in runtime_matches:
            print(f"  {path}:{line_no}: {line}", file=sys.stderr)
    elif count < limit:
        print(
            f"NOTE: {check['name']} is below its baseline; lower the limit from {limit} to {count}.",
            file=sys.stderr,
        )
    if "test_limit" in check:
        test_limit = check["test_limit"]
        test_name = f"{check['name']}_tests"
        print(f"{test_name}: {test_count} references (baseline limit {test_limit})")
        if test_count > test_limit:
            failed = True
            print(
                f"ERROR: {test_name} increased from {test_limit} to {test_count}.",
                file=sys.stderr,
            )
            for path, line_no, line, _ in test_matches:
                print(f"  {path}:{line_no}: {line}", file=sys.stderr)
        elif test_count < test_limit:
            print(
                f"NOTE: {test_name} is below its baseline; lower the limit from {test_limit} to {test_count}.",
                file=sys.stderr,
            )

if failed:
    sys.exit(1)

print("OK: compile/VM coupling did not increase (Issue #8449).")
PY
