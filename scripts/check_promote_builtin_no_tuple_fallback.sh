#!/usr/bin/env bash
# check_promote_builtin_no_tuple_fallback.sh
#
# Issue #9896 - keep `BuiltinId::Promote` off the old silent fallback shape:
# after Julia method dispatch misses, the builtin must fail loudly instead of
# returning the original arguments as `Value::Tuple`.
#
# Usage: run from the repository root
#   bash scripts/check_promote_builtin_no_tuple_fallback.sh
#
# Exit code: 0 = OK, 1 = a promote builtin handler can construct a tuple
# fallback or no longer has the expected dispatch/miss shape.

set -euo pipefail

python3 - <<'PY'
from pathlib import Path
import sys

SOURCES = [
    Path("subset_julia_vm_vm/src/vm/builtins_exec.rs"),
    Path("subset_julia_vm_vm/src/vm/builtins_types_conversion.rs"),
]


def scrub_comments_and_literals(src: str) -> str:
    out = []
    i = 0
    n = len(src)
    state = "code"
    while i < n:
        c = src[i]
        nxt = src[i + 1] if i + 1 < n else ""
        if state == "code":
            if c == "/" and nxt == "/":
                out.append("  ")
                i += 2
                state = "line_comment"
                continue
            if c == "/" and nxt == "*":
                out.append("  ")
                i += 2
                state = "block_comment"
                continue
            if c == '"':
                out.append(" ")
                i += 1
                state = "string"
                continue
            if c == "'":
                out.append(" ")
                i += 1
                state = "char"
                continue
            out.append(c)
            i += 1
            continue
        if state == "line_comment":
            out.append("\n" if c == "\n" else " ")
            state = "code" if c == "\n" else state
            i += 1
            continue
        if state == "block_comment":
            if c == "*" and nxt == "/":
                out.append("  ")
                i += 2
                state = "code"
            else:
                out.append("\n" if c == "\n" else " ")
                i += 1
            continue
        if state == "string":
            if c == "\\":
                out.append("  ")
                i += 2
                continue
            out.append("\n" if c == "\n" else " ")
            state = "code" if c == '"' else state
            i += 1
            continue
        if state == "char":
            if c == "\\":
                out.append("  ")
                i += 2
                continue
            out.append("\n" if c == "\n" else " ")
            state = "code" if c == "'" else state
            i += 1
            continue
    return "".join(out)


def extract_promote_blocks(src: str):
    marker = "BuiltinId::Promote"
    blocks = []
    pos = 0
    while True:
        start = src.find(marker, pos)
        if start == -1:
            return blocks
        brace = src.find("{", start)
        if brace == -1:
            raise ValueError("found BuiltinId::Promote without opening brace")
        depth = 0
        i = brace
        state = "code"
        while i < len(src):
            c = src[i]
            nxt = src[i + 1] if i + 1 < len(src) else ""
            if state == "code":
                if c == "/" and nxt == "/":
                    i += 2
                    state = "line_comment"
                    continue
                if c == "/" and nxt == "*":
                    i += 2
                    state = "block_comment"
                    continue
                if c == '"':
                    i += 1
                    state = "string"
                    continue
                if c == "'":
                    i += 1
                    state = "char"
                    continue
                if c == "{":
                    depth += 1
                elif c == "}":
                    depth -= 1
                    if depth == 0:
                        blocks.append((start, src[start : i + 1]))
                        pos = i + 1
                        break
                i += 1
                continue
            if state == "line_comment":
                state = "code" if c == "\n" else state
                i += 1
                continue
            if state == "block_comment":
                if c == "*" and nxt == "/":
                    i += 2
                    state = "code"
                else:
                    i += 1
                continue
            if state == "string":
                i += 2 if c == "\\" else 1
                state = "code" if c == '"' else state
                continue
            if state == "char":
                i += 2 if c == "\\" else 1
                state = "code" if c == "'" else state
                continue
        else:
            raise ValueError("unterminated BuiltinId::Promote block")


failures = []
checked = 0
for path in SOURCES:
    if not path.exists():
        failures.append(f"ERROR: {path} not found. Run this script from the repository root.")
        continue
    src = path.read_text()
    try:
        blocks = extract_promote_blocks(src)
    except ValueError as exc:
        failures.append(f"ERROR: {path}: {exc}")
        continue
    if not blocks:
        failures.append(f"ERROR: {path} has no BuiltinId::Promote handler to audit.")
        continue
    for start, block in blocks:
        checked += 1
        line = src.count("\n", 0, start) + 1
        code = scrub_comments_and_literals(block)
        if "find_best_method_index" not in block or '"promote"' not in block or '"Base.promote"' not in block:
            failures.append(
                f"ERROR: {path}:{line}: BuiltinId::Promote no longer dispatches through Julia promote/Base.promote methods."
            )
        if "VmError::MethodError" not in code:
            failures.append(
                f"ERROR: {path}:{line}: BuiltinId::Promote miss path must fail loudly with VmError::MethodError (Issue #9896)."
            )
        if "Value::Tuple" in code:
            failures.append(
                f"ERROR: {path}:{line}: Promote builtin fallback must not construct Value::Tuple after method lookup misses (Issue #9896)."
            )

if failures:
    print("\n".join(failures))
    sys.exit(1)

print(f"OK: audited {checked} BuiltinId::Promote handler(s); method misses fail loudly and no Value::Tuple fallback is present (Issue #9896).")
PY
