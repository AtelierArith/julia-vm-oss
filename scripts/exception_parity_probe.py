#!/usr/bin/env python3
"""Exception-taxonomy parity probe (Issue #10813 Phase 0).

This probe only produces a report. The gating wrapper
`scripts/exception_parity_ratchet.sh` checks that report against the
issue-linked two-sided ratchet. The probe runs a fixed corpus of Julia
constructs under BOTH upstream `julia` and `sjulia`, in two modes per
construct:

  - bare:    the construct exactly as written (top-level program).
  - wrapped: the construct inside `try ... catch e; println("...", typeof(e));
             end`, so we can observe (a) whether the failure is catchable at
             all (process exits 0 and the sentinel prints) and (b) what
             concrete exception type reaches the `catch` variable.

This directly probes the three axes named in Issue #10813:
  1. exception TYPE parity (same construct, different exception class)
  2. raise-LAYER parity (catchable runtime throw vs. an uncatchable
     parse/lowering/compile-time abort that never reaches `catch` at all)
  3. "silent wrong result" divergence (one side raises, the other returns a
     value with no exception at all -- caught for free by the same template,
     since a NO_EXCEPTION sentinel on one side and CAUGHT:<Type> on the other
     is exactly this case)

Entries whose `code` is deliberately invalid syntax (parse-error probes) set
parse_error=True: catchability is skipped (a parse error prevents the file
from running at all, so wrapping it in try/catch cannot help, by
construction) and only the bare run is executed.

Usage:
    scripts/exception_parity_probe.py --sjulia <path-to-sjulia-binary> \
        [--julia julia] [--out docs/vm/EXCEPTION_PARITY_PROBE.tsv] \
        [--timeout 15]

Writes a TSV snapshot and a short console summary. It does not decide whether
known divergences are acceptable; `exception_parity_ratchet.py` owns that
policy.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

WRAP_TEMPLATE = """{setup}
try
    {code}
    println("EXC_PARITY_PROBE:NO_EXCEPTION")
catch __exc_parity_probe_e
    println("EXC_PARITY_PROBE:CAUGHT:", typeof(__exc_parity_probe_e))
end
"""

SENTINEL_RE = re.compile(r"EXC_PARITY_PROBE:(NO_EXCEPTION|CAUGHT:(\S+))")


@dataclass
class Case:
    id: str
    category: str
    code: str
    note: str = ""
    parse_error: bool = False
    related_issue: str = ""
    setup: str = ""


# Corpus: deliberately broad across the exception classes named in Issue
# #10813 (undefined var, MethodError, BoundsError, DivideError, InexactError,
# ArgumentError, TypeError, KeyError, DomainError, StackOverflow, parse
# errors, type-assert failures, kwarg errors) plus the concrete MWEs from the
# Evidence table (#10318/#10406/#10481/#10511/#10593/#10602/#10736/#10737).
# Identifiers are prefixed `___` / suffixed `_probe`/`_xyz` to avoid
# colliding with Base names.
CORPUS: list[Case] = [
    Case("undef_var_read", "undefined-var", "___undefined_var_probe_xyz"),
    Case("undef_var_call", "undefined-var", "___undefined_fn_probe_xyz(1, 2)"),
    Case("method_error_type_mismatch", "dispatch", 'sin("a")'),
    Case("method_error_arity", "dispatch", "((x, y) -> x + y)(1)"),
    Case(
        "method_error_noncallable",
        "dispatch",
        "___noncallable_probe_zzz = 5; ___noncallable_probe_zzz(1)",
        note="a bare numeric literal directly before '(' is multiplication in "
        "Julia (e.g. `2(3) == 6`), not a call -- use a variable to force a "
        "true non-callable-value call",
    ),
    Case("bounds_error_vector_high", "collections", "[1, 2, 3][10]"),
    Case("bounds_error_vector_negative", "collections", "[1, 2, 3][-1]"),
    Case("bounds_error_string", "strings", '"abc"[10]'),
    Case("string_index_error", "strings", '"\\u20ac"[2]', note="multi-byte codeunit index"),
    Case("divide_error_int", "numeric", "div(1, 0)"),
    Case("divide_error_rem", "numeric", "rem(1, 0)"),
    Case("inexact_error_int_conv", "numeric", "Int(1.5)"),
    Case("inexact_error_uint", "numeric", "UInt8(-1)"),
    Case("domain_error_sqrt", "numeric", "sqrt(-1.0)"),
    Case("argument_error_negative_size", "collections", "Vector{Int}(undef, -1)"),
    Case("argument_error_string_repeat", "strings", 'repeat("a", -1)'),
    Case("key_error_dict", "collections", "Dict(1 => 2)[3]"),
    Case("type_error_typeassert", "typesystem", "(1)::AbstractString"),
    Case(
        "undef_keyword_error",
        "kwargs",
        "function ___kwreq_probe(; x); return x; end; ___kwreq_probe()",
    ),
    Case(
        "method_error_unknown_kwarg",
        "kwargs",
        "function ___kwok_probe(; x=1); return x; end; ___kwok_probe(y=2)",
    ),
    Case(
        "field_error_undefined",
        "structs",
        "struct ___SProbe1; a::Int; end; getfield(___SProbe1(1), :___bogus_field_xyz)",
    ),
    Case(
        "immutable_field_assign",
        "structs",
        "struct ___SProbe2; a::Int; end; ___p_probe = ___SProbe2(1); ___p_probe.a = 2",
        related_issue="#10511 (closed)",
    ),
    Case(
        "getfield_module_bogus",
        "reflection",
        "import Base; getfield(Base, :___bogus_symbol_xyz)",
        related_issue="#10318 (closed)",
    ),
    Case(
        "memory_undef_ctor",
        "memory",
        "Memory{Int64}(undef)",
        related_issue="#10737 (closed)",
    ),
    Case("sqrt_string", "numeric", 'sqrt("a")', related_issue="#10481 (closed)"),
    Case(
        "abs2_string_silent",
        "numeric",
        'abs2("a")',
        note="expect: silent wrong result in sjulia, not an exception at all",
        related_issue="#10602 (closed)",
    ),
    Case(
        "conj_string_fallback",
        "numeric",
        'conj("a")',
        note="generic-fallback sweep regression sentinel",
        related_issue="#11522 (open)",
    ),
    Case(
        "isreal_string_fallback",
        "numeric",
        'isreal("a")',
        note="generic-fallback sweep regression sentinel",
        related_issue="#11522 (open)",
    ),
    Case(
        "flipsign_string_fallback",
        "numeric",
        'flipsign("a", -1)',
        note="generic-fallback sweep regression sentinel",
        related_issue="#11525 (open)",
    ),
    Case(
        "real_string_fallback",
        "numeric",
        'real("a")',
        note="generic-fallback sweep regression sentinel",
        related_issue="#11797 (open)",
    ),
    Case(
        "signbit_string_fallback",
        "numeric",
        'signbit("a")',
        note="generic-fallback sweep regression sentinel",
        related_issue="#11797 (open)",
    ),
    Case(
        "abs_string_fallback",
        "numeric",
        'abs("a")',
        note="generic-fallback sweep regression sentinel",
        related_issue="#11797 (open)",
    ),
    Case(
        "map_dispatch_failure",
        "hof",
        'map(sqrt, ["a", "b"])',
        note="exact MWE from Issue #10406 (closed) -- regression sentinel",
        related_issue="#10406 (closed)",
    ),
    Case(
        "parametric_ctor_nonconvertible",
        "structs",
        '___SProbeB{Float64}("abc")',
        related_issue="#10593 (closed)",
        setup="struct ___SProbeB{T}; x::T; end",
    ),
    Case(
        "regex_match_oob",
        "regex",
        'match(r"x", "abc", 10)',
        related_issue="#10736 (closed)",
    ),
    Case(
        "regex_findnext_negative",
        "regex",
        'findnext(r"\\d", "abc", 0)',
        related_issue="#10736 (closed)",
    ),
    Case(
        "substitution_string_length",
        "strings",
        'length(s"abc")',
        note="expect: spurious error in sjulia where upstream succeeds",
        related_issue="#10735 (closed)",
    ),
    Case(
        "convert_failure",
        "conversion",
        'convert(Int, "a")',
        note="same TypeError-vs-MethodError class Issue #10481 closed for "
        "sqrt(::String) -- confirms the fix did not generalize into a funnel",
    ),
    Case(
        "domain_error_log_silent",
        "numeric",
        "log(-1.0)",
        related_issue="#11559 (open)",
        note="upstream: DomainError; sjulia returns NaN silently (Issue #11559)",
    ),
    Case(
        "typed_local_reassign_no_enforcement",
        "typesystem",
        'function ___tlr_probe(); local z::Int = 1; z = "s"; return z; end; '
        "___tlr_probe()",
        note="upstream converts on typed-local reassignment (fails ->"
        " MethodError); sjulia observed to skip the convert/check entirely",
        related_issue="#11794 (open)",
    ),
    Case("iterate_noniterable", "iteration", "for ___q_probe in 5; end"),
    Case("assertion_error", "control_flow", '@assert 1 == 2 "boom"'),
    Case(
        "stack_overflow",
        "control_flow",
        "___f_rec_probe() = 1 + ___f_rec_probe(); ___f_rec_probe()",
        note="unbounded self-recursion; run under `timeout`",
    ),
    Case(
        "undef_ref_error",
        "arrays",
        "___v_probe = Vector{String}(undef, 1); ___v_probe[1]",
        related_issue="#11390 (open)",
    ),
    Case(
        "parse_error_dangling_op",
        "parse",
        "1 +\n",
        parse_error=True,
    ),
    Case(
        "parse_error_unmatched_paren",
        "parse",
        "x_probe = (1 + 2\n",
        parse_error=True,
    ),
]


def run(cmd: list[str], stdin_path: Path, timeout: int) -> tuple[int, str]:
    try:
        proc = subprocess.run(
            cmd + [str(stdin_path)],
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return proc.returncode, (proc.stdout + proc.stderr)
    except subprocess.TimeoutExpired:
        return -9, "<probe timed out>"
    except FileNotFoundError as e:
        return -127, f"<interpreter not found: {e}>"


def extract_sentinel(output: str) -> tuple[str, str]:
    """Returns (outcome, type_name). outcome in {NO_EXCEPTION, CAUGHT, ABSENT}."""
    m = SENTINEL_RE.search(output)
    if not m:
        return "ABSENT", ""
    if m.group(1) == "NO_EXCEPTION":
        return "NO_EXCEPTION", ""
    return "CAUGHT", m.group(2)


def probe_one(interp_cmd: list[str], case: Case, tmpdir: Path, timeout: int) -> dict:
    bare_path = tmpdir / f"{case.id}_bare.jl"
    bare_path.write_text(case.setup + "\n" + case.code + "\n")
    bare_exit, bare_out = run(interp_cmd, bare_path, timeout)

    def infrastructure_health(exit_code: int, outcome: str | None = None) -> str:
        if exit_code == -127:
            return "interpreter-missing"
        if exit_code == -9:
            return "timeout"
        if exit_code < 0:
            return f"signal{-exit_code}"
        if outcome in {"CAUGHT", "NO_EXCEPTION"} and exit_code != 0:
            return "sentinel-with-nonzero-exit"
        if outcome == "ABSENT" and exit_code == 0:
            return "missing-sentinel"
        return "ok"

    bare_health = infrastructure_health(bare_exit)

    if case.parse_error:
        return {
            "bare_exit": bare_exit,
            "bare_out": bare_out,
            "catchable": "n/a-parse-time",
            "exc_type": "",
            "health": bare_health,
        }

    wrapped_path = tmpdir / f"{case.id}_wrapped.jl"
    wrapped_path.write_text(WRAP_TEMPLATE.format(setup=case.setup, code=case.code))
    wrapped_exit, wrapped_out = run(interp_cmd, wrapped_path, timeout)

    outcome, exc_type = extract_sentinel(wrapped_out)
    wrapped_health = infrastructure_health(wrapped_exit, outcome)
    health = bare_health if bare_health != "ok" else wrapped_health
    if outcome == "CAUGHT":
        catchable = "yes"
    elif outcome == "NO_EXCEPTION":
        catchable = "no-exception-raised"
    else:
        # Sentinel never printed: the process died before reaching the
        # try/catch's own print statements -- a compile/lowering-time abort,
        # not a runtime throw. This is exactly the "raise layer" divergence
        # Issue #10813 claim 2 is about.
        catchable = "no-uncatchable" if wrapped_exit != 0 else "no-unknown"

    return {
        "bare_exit": bare_exit,
        "bare_out": bare_out,
        "wrapped_exit": wrapped_exit,
        "wrapped_out": wrapped_out,
        "catchable": catchable,
        "exc_type": exc_type,
        "health": health,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--sjulia", required=True, help="path to sjulia binary")
    ap.add_argument("--julia", default="julia", help="upstream julia binary/command")
    ap.add_argument(
        "--out",
        default="docs/vm/EXCEPTION_PARITY_PROBE.tsv",
        help="TSV output path",
    )
    ap.add_argument("--timeout", type=int, default=15)
    args = ap.parse_args()

    julia_cmd = [args.julia, "--startup-file=no"]
    sjulia_cmd = [args.sjulia]

    rows = []
    type_mismatch = 0
    catchable_mismatch = 0
    match_count = 0

    with tempfile.TemporaryDirectory(prefix="exc_parity_probe_") as td:
        tmpdir = Path(td)
        for case in CORPUS:
            j = probe_one(julia_cmd, case, tmpdir, args.timeout)
            s = probe_one(sjulia_cmd, case, tmpdir, args.timeout)

            if case.parse_error:
                type_match = "n/a"
                catchable_match = "n/a"
                note = "parse-time (bare only); catchability is structurally 'no' on both sides"
            else:
                type_match = "yes" if j["exc_type"] == s["exc_type"] else "no"
                catchable_match = "yes" if j["catchable"] == s["catchable"] else "no"
                if type_match == "no":
                    type_mismatch += 1
                if catchable_match == "no":
                    catchable_mismatch += 1
                if type_match == "yes" and catchable_match == "yes":
                    match_count += 1
                note = case.note

            rows.append(
                {
                    "id": case.id,
                    "category": case.category,
                    "related_issue": case.related_issue,
                    "julia_exit": j["bare_exit"],
                    "julia_catchable": j.get("catchable", ""),
                    "julia_exc_type": j.get("exc_type", ""),
                    "sjulia_exit": s["bare_exit"],
                    "sjulia_catchable": s.get("catchable", ""),
                    "sjulia_exc_type": s.get("exc_type", ""),
                    "type_match": type_match,
                    "catchable_match": catchable_match,
                    # Keep the committed TSV free of trailing whitespace while
                    # retaining a concrete final column for every row.
                    "note": note or "-",
                    "julia_health": j["health"],
                    "sjulia_health": s["health"],
                }
            )

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    cols = [
        "id",
        "category",
        "related_issue",
        "julia_exit",
        "julia_catchable",
        "julia_exc_type",
        "sjulia_exit",
        "sjulia_catchable",
        "sjulia_exc_type",
        "type_match",
        "catchable_match",
        "note",
        "julia_health",
        "sjulia_health",
    ]
    with out_path.open("w") as f:
        f.write("\t".join(cols) + "\n")
        for row in rows:
            f.write("\t".join(str(row[c]).replace("\t", " ").replace("\n", " ") for c in cols) + "\n")

    total = len(CORPUS)
    parse_cases = sum(1 for c in CORPUS if c.parse_error)
    comparable = total - parse_cases
    print(f"wrote {out_path} ({total} corpus cases, {parse_cases} parse-time-only)")
    print(f"comparable cases: {comparable}")
    print(f"  exact match (type+catchable): {match_count}")
    print(f"  type mismatch: {type_mismatch}")
    print(f"  catchable mismatch: {catchable_mismatch}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
