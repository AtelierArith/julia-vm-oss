#!/usr/bin/env bash
# check_math_router_exact_or_any.sh — the exact-or-`Any` rule for inference
# producers and math-builtin routers (Issue #11486, prevention for #11468 /
# #11481).
#
# INVARIANT: static (compile-time-bound) code may emit a concrete-type
# instruction only for a *proven* type. An argument-blind inference producer
# must return `Top`/`Any`, never a fabricated `Concrete`; and the math-builtin
# routers must route a statically-`Any` (not-proven-primitive) operand through
# runtime dispatch (`CallTypedDispatchOrBuiltin`), never the real-only
# `SqrtF64` / bare `CallBuiltin(Sqrt)` fast path. #11436 established this rule
# for constructor returns; #11468/#11481 showed it was never applied to the
# `complex` tfunc producer or the triplicated `sqrt` routers.
#
# This audit pins the shapes that fix those two bugs so they cannot silently
# regress:
#   1. `tfunc_complex_contextual` is argument-blind and returns `Top` only — no
#      `LatticeType::Concrete`, no `extract_complex_element_type`, no registry
#      scan that would fabricate `Complex{Float64}`.
#   2. `infer_julia_complex_call` keeps the `=> JuliaType::Any` fallback for a
#      non-`Concrete` inference result (never an unconditional `Complex{...}`).
#   3. Each of the three `sqrt` routers routes the not-proven-primitive operand
#      through `CallTypedDispatchOrBuiltin` for `BuiltinId::Sqrt`.
# See docs/vm/CODE_AUDITS.md ("math_router_exact_or_any").

set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

python3 - <<'PY'
import pathlib
import re
import sys

ROOT = pathlib.Path(".")
COMPLEX_TFUNC = ROOT / "subset_julia_vm_compile/src/compile/tfuncs/complex_ops.rs"
INFER = ROOT / "subset_julia_vm_compile/src/compile/expr/infer/expr_tfuncs.rs"
BUILTIN_MATH = ROOT / "subset_julia_vm_compile/src/compile/expr/builtin_math.rs"
HANDLER_MATH = ROOT / "subset_julia_vm_compile/src/compile/expr/call/handlers/math.rs"
BUILTIN = ROOT / "subset_julia_vm_compile/src/compile/expr/builtin.rs"

errors = []


def fail(message):
    errors.append(message)


def strip_comments_and_literals(source):
    out = list(source)
    index = 0
    state = "normal"
    block_depth = 0
    while index < len(source):
        char = source[index]
        following = source[index + 1] if index + 1 < len(source) else ""
        if state == "normal":
            if char == "/" and following == "/":
                out[index] = out[index + 1] = " "
                index += 2
                state = "line"
                continue
            if char == "/" and following == "*":
                out[index] = out[index + 1] = " "
                index += 2
                state = "block"
                block_depth = 1
                continue
            if char == '"':
                out[index] = " "
                index += 1
                state = "string"
                continue
            if char == "'" and (
                following == "\\"
                or (index + 2 < len(source) and source[index + 2] == "'")
            ):
                out[index] = " "
                index += 1
                state = "char"
                continue
            index += 1
            continue
        if state == "line":
            if char == "\n":
                state = "normal"
            else:
                out[index] = " "
            index += 1
            continue
        if state == "block":
            if char == "/" and following == "*":
                out[index] = out[index + 1] = " "
                block_depth += 1
                index += 2
                continue
            if char == "*" and following == "/":
                out[index] = out[index + 1] = " "
                block_depth -= 1
                index += 2
                if block_depth == 0:
                    state = "normal"
                continue
            if char != "\n":
                out[index] = " "
            index += 1
            continue
        terminal = '"' if state == "string" else "'"
        if char == "\\" and following:
            out[index] = " "
            if following != "\n":
                out[index + 1] = " "
            index += 2
            continue
        if char == terminal:
            out[index] = " "
            state = "normal"
        elif char != "\n":
            out[index] = " "
        index += 1
    return "".join(out)


def braced_body(masked, opening):
    depth = 0
    for index in range(opening, len(masked)):
        if masked[index] == "{":
            depth += 1
        elif masked[index] == "}":
            depth -= 1
            if depth == 0:
                return masked[opening : index + 1]
    return ""


def function_body(path, name):
    """Return the masked source of `fn name(...) { ... }`, or None."""
    if not path.is_file():
        fail("audit target missing: {} (Issue #11486)".format(path))
        return None
    masked = strip_comments_and_literals(path.read_text(encoding="utf-8"))
    match = re.search(r"\bfn\s+" + re.escape(name) + r"\s*(?:<[^>{}]*>)?\s*\(", masked)
    if match is None:
        fail("function '{}' not found in {} (Issue #11486)".format(name, path))
        return None
    opening = masked.find("{", match.end())
    if opening < 0:
        return None
    return braced_body(masked, opening)


def compact(text):
    return re.sub(r"\s+", "", text)


# 1. tfunc_complex_contextual: argument-blind, returns Top only.
body = function_body(COMPLEX_TFUNC, "tfunc_complex_contextual")
if body is not None:
    if "LatticeType::Top" not in compact(body):
        fail(
            "tfunc_complex_contextual must return LatticeType::Top for an "
            "argument-blind inference; it no longer does (Issue #11486/#11468)."
        )
    for forbidden in ("LatticeType::Concrete", "extract_complex_element_type"):
        if compact(forbidden) in compact(body):
            fail(
                "tfunc_complex_contextual fabricates a concrete type via '{}'; "
                "an argument-blind producer must stay Top (Issue #11486/#11468)."
                .format(forbidden)
            )

# 2. infer_julia_complex_call keeps the =>JuliaType::Any fallback.
body = function_body(INFER, "infer_julia_complex_call")
if body is not None and "JuliaType::Any" not in compact(body):
    fail(
        "infer_julia_complex_call dropped its non-Concrete => JuliaType::Any "
        "fallback; unknown complex(x) must stay dynamic (Issue #11486/#11468)."
    )

# 3. The three sqrt routers each route not-proven-primitive through
#    CallTypedDispatchOrBuiltin for Sqrt.
sqrt_routers = [
    (BUILTIN_MATH, "compile_builtin_math"),
    (HANDLER_MATH, "compile_sqrt"),
    (BUILTIN, "compile_builtin"),
]
for path, fn in sqrt_routers:
    body = function_body(path, fn)
    if body is None:
        continue
    packed = compact(body)
    if "CallTypedDispatchOrBuiltin" not in packed:
        fail(
            "sqrt router {}::{} no longer routes the not-proven-primitive "
            "operand through CallTypedDispatchOrBuiltin; a statically-Any "
            "Complex expression would misroute to SqrtF64 (Issue #11486/#11481)."
            .format(path, fn)
        )

if errors:
    for message in errors:
        print("ERROR: {}".format(message), file=sys.stderr)
    sys.exit(1)

print(
    "OK: argument-blind complex inference stays Top/Any and the sqrt routers "
    "keep runtime dispatch for not-proven-primitive operands "
    "(exact-or-Any, Issues #11486/#11468/#11481)."
)
PY
