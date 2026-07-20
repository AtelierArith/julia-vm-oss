#!/usr/bin/env bash
# aot_numeric_matrix_reduced.sh — first-stage AoT numeric matrix comparator
# (Issue #9565).
#
# This reuses the upstream reduced numeric matrix TSV, but only executes the
# currently supported AoT scalar slice: Int64/Float64 arithmetic/comparison,
# and same-type Int64/Int8/Int16/Int32/Int128 arithmetic/comparison plus
# div/fld/cld/rem/mod (Issue #9687 slice 3). Unsupported cells must match
# the checked-in skiplist, so the first-stage scope grows by shrinking that
# ratchet.
#
# Slice-2 scope note (Issue #9687): the signed-integer tower slice is
# deliberately restricted to types whose Julia `string()` and `repr()` agree
# (true for Bool/signed integers/Float64, verified against upstream `julia`).
# The probe below prints `string(r)`, matching this script's own established
# convention from the initial 45-row slice (AoT codegen has no `repr()`
# builtin at all — Issue #9565's original scope note). UInt8/16/32/64/128 and
# Float32 diverge (`repr()` shows hex / an `f0` suffix that `string()` does
# not) and remain skiplisted pending an AoT `repr()` builtin.
#
# Slice-3 scope note (Issue #9687): `div`/`fld`/`cld`/`rem`/`mod` on
# non-Int64 integer widths used to be blocked because the codegen always
# routed the result through `Value::from(...)`, and the AoT runtime `Value`
# enum had no I8/I16/I128/U8/U16/U32/U64/U128 variants. Issue #10131 fixed
# this generally (Value gained the missing variants, and the div-family
# emitters only box through `Value::from(...)` when the recorded return type
# is an actually-boxed slot, keeping native results native otherwise). The
# div-family is therefore extended here to the same signed-integer tower
# already in scope for arithmetic/comparison (Int8/Int16/Int32/Int128),
# where `string(x) == repr(x)` keeps the string()-based probe faithful.
# UInt8/16/32/64/128 div-family results now compile too (Issue #10131), but
# stay skiplisted here because their `repr()` output (hex) still diverges
# from `string()` in the oracle, not because of any remaining div-family gap.

set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

source "$ROOT/scripts/cargo_target_dir.sh"
cargo_target_dir="$(resolve_cargo_target_dir "$ROOT")"
export CARGO_TARGET_DIR="$cargo_target_dir"

ORACLE="${ORACLE:-subset_julia_vm/tests/fixtures/numeric/numeric_matrix_reduced_8696.tsv}"
SKIPLIST="${SKIPLIST:-docs/vm/NUMERIC_MATRIX_AOT_REDUCED_SKIPLIST.tsv}"
JULIARS_BIN="${JULIARS_BIN:-$cargo_target_dir/release/juliars}"
export JULIARS_BIN
OUT_DIR="${OUT_DIR:-target/numeric-matrix-aot-reduced}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-600}"
EXPECTED_ROWS="${EXPECTED_ROWS:-105}"

mkdir -p "$OUT_DIR"
PROBE="$OUT_DIR/probe_aot.jl"
EXPECTED="$OUT_DIR/expected.tsv"
ACTUAL="$OUT_DIR/aot.tsv"
DIFF="$OUT_DIR/diff.tsv"
GENERATED_RS="$OUT_DIR/generated.rs"
AOT_BIN="$OUT_DIR/probe_aot_bin"
BUILD_LOG="$OUT_DIR/juliars.log"

if [[ ! -f "$ORACLE" ]]; then
  echo "ERROR: oracle TSV not found: $ORACLE" >&2
  exit 1
fi
if [[ ! -f "$SKIPLIST" ]]; then
  echo "ERROR: AoT reduced numeric matrix skiplist not found: $SKIPLIST" >&2
  exit 1
fi
if [[ ! -x "$JULIARS_BIN" ]]; then
  echo "ERROR: juliars binary not executable: $JULIARS_BIN" >&2
  echo "Build it with: cargo build --release -p subset_julia_vm --features aot --bin juliars" >&2
  exit 1
fi

julia --startup-file=no - "$ORACLE" "$PROBE" "$EXPECTED" "$EXPECTED_ROWS" "$SKIPLIST" <<'JL'
const ORACLE = ARGS[1]
const PROBE = ARGS[2]
const EXPECTED = ARGS[3]
const EXPECTED_ROWS = parse(Int, ARGS[4])
const SKIPLIST = ARGS[5]

const SCALAR_TYPES = Set(["Int64", "Float64"])
const SCALAR_OPS = Set(["+", "-", "*", "/", "==", "!=", "<", "<=", ">", ">="])
# Div-family builtins. Not Int64-specific any more (Issue #9687 slice 3):
# Issue #10131 fixed the AoT codegen so div/fld/cld/rem/mod stay correctly
# typed for every integer width, not only Int64.
const DIV_FAMILY_OPS = Set(["div", "fld", "cld", "rem", "mod"])
# Slice 2 (Issue #9687): same-type signed-integer tower arithmetic/comparison.
# Restricted to types where `string(x) == repr(x)` in upstream Julia, so the
# existing string()-based probe (see file header) stays a faithful oracle
# comparison without needing an AoT `repr()` builtin.
const WIDE_SAME_TYPES = Set(["Int8", "Int16", "Int32", "Int128"])

function key_for(fields)
    if length(fields) >= 13
        return join((fields[1], fields[2], fields[3], fields[10], fields[11]), "|")
    end
    return join((fields[1], fields[2], fields[3]), "|")
end

function expr_for(fields)
    op = fields[3]
    a = length(fields) >= 13 ? fields[12] : error("numeric matrix oracle row lacks left_expr")
    b = length(fields) >= 13 ? fields[13] : error("numeric matrix oracle row lacks right_expr")
    if op in ("+", "-", "*", "/", "==", "!=", "<", "<=", ">", ">=")
        return "($a) $op ($b)"
    end
    return "$op($a, $b)"
end

function supported(fields)
    length(fields) >= 13 || return false
    left_type, right_type, op = fields[1], fields[2], fields[3]
    left_label, right_label = fields[10], fields[11]
    left_label == "normal" && right_label == "normal" || return false
    if left_type in SCALAR_TYPES && right_type in SCALAR_TYPES && op in SCALAR_OPS
        return true
    end
    if left_type == right_type && left_type in WIDE_SAME_TYPES && op in SCALAR_OPS
        return true
    end
    # Slice 3 (Issue #9687): div-family now works for the same signed-integer
    # tower as arithmetic/comparison, plus Int64 (Issue #10131).
    if left_type == right_type && left_type == "Int64" && op in DIV_FAMILY_OPS
        return true
    end
    return left_type == right_type && left_type in WIDE_SAME_TYPES && op in DIV_FAMILY_OPS
end

function pattern_matches(pattern, value)
    return pattern == "ANY" || pattern == value
end

function skip_rule_matches(rule, fields)
    length(fields) >= 13 || return false
    return pattern_matches(rule.left_type, fields[1]) &&
        pattern_matches(rule.left_label, fields[10]) &&
        pattern_matches(rule.operator, fields[3]) &&
        pattern_matches(rule.right_type, fields[4]) &&
        pattern_matches(rule.right_label, fields[11])
end

function read_skiplist(path)
    rules = []
    for line in eachline(path)
        isempty(strip(line)) && continue
        startswith(line, "#") && continue
        startswith(line, "left_type\t") && continue
        fields = split(line, '\t')
        length(fields) >= 7 || error("malformed skiplist row in $path: $line")
        push!(rules, (
            left_type = String(fields[1]),
            left_label = String(fields[2]),
            operator = String(fields[3]),
            right_type = String(fields[4]),
            right_label = String(fields[5]),
            issue = String(fields[6]),
            expected_count = parse(Int, fields[7]),
        ))
    end
    isempty(rules) && error("empty AoT reduced numeric matrix skiplist: $path")
    return rules
end

function main()
    rows = Vector{Vector{SubString{String}}}()
    skipped = Vector{Vector{SubString{String}}}()
    total = 0
    for line in eachline(ORACLE)
        startswith(line, "left_type\t") && continue
        total += 1
        fields = split(line, '\t')
        if supported(fields)
            push!(rows, fields)
        else
            push!(skipped, fields)
        end
    end

    if length(rows) != EXPECTED_ROWS
        error("AoT reduced numeric matrix supported row count changed: expected $EXPECTED_ROWS, observed $(length(rows))")
    end

    rules = read_skiplist(SKIPLIST)
    observed_counts = fill(0, length(rules))
    for fields in skipped
        matches = Int[]
        for (idx, rule) in pairs(rules)
            skip_rule_matches(rule, fields) && push!(matches, idx)
        end
        if isempty(matches)
            error("AoT reduced numeric matrix skipped row is not covered by $SKIPLIST: $(join(fields[1:13], '\t'))")
        end
        if length(matches) > 1
            issues = join([rules[idx].issue for idx in matches], ", ")
            error("AoT reduced numeric matrix skipped row matches multiple skiplist rules ($issues): $(join(fields[1:13], '\t'))")
        end
        observed_counts[matches[1]] += 1
    end
    for (idx, rule) in pairs(rules)
        if observed_counts[idx] != rule.expected_count
            error("AoT reduced numeric matrix skiplist count changed for $(rule.issue): expected $(rule.expected_count), observed $(observed_counts[idx])")
        end
    end

    open(PROBE, "w") do io
        println(io, "# Generated by scripts/aot_numeric_matrix_reduced.sh.")
        println(io, "# Supported rows: $(length(rows)); skipped rows: $(total - length(rows)).")
        # Workaround: each row gets its own uniquely-named local (`r1`, `r2`,
        # ...) instead of reusing a single `r` across sibling top-level `let`
        # blocks. The AoT compiler unifies same-named locals from separate
        # `let` blocks to the first-seen static Rust type instead of treating
        # each `let` as an independent scope, which silently corrupts
        # typeof()/values for later blocks once the row set spans more than
        # one concrete type (Issue #10111).
        for (idx, fields) in enumerate(rows)
            key = key_for(fields)
            expr = expr_for(fields)
            var = "r$(idx)"
            println(io, "let")
            println(io, "    $var = ", expr)
            println(io, "    println(\"", escape_string(key), "\\t\", \"ok\\t\", string(typeof($var)), \"\\t\", string($var), \"\\tnone\")")
            println(io, "end")
        end
    end

    open(EXPECTED, "w") do io
        println(io, "key\tstatus\tresult_type\tresult_value\texception_type")
        for fields in rows
            println(io, join((key_for(fields), fields[6], fields[7], fields[8], fields[9]), '\t'))
        end
    end

    println("Generated AoT numeric matrix probe with $(length(rows)) supported rows and $(length(skipped)) skiplisted rows.")
end

main()
JL

if ! timeout "$TIMEOUT_SECONDS" "$JULIARS_BIN" --minimal-prelude "$PROBE" -o "$GENERATED_RS" --emit-binary "$AOT_BIN" >"$BUILD_LOG" 2>&1; then
  echo "ERROR: juliars failed for AoT numeric matrix probe" >&2
  tail -80 "$BUILD_LOG" >&2
  exit 1
fi

timeout "$TIMEOUT_SECONDS" "$AOT_BIN" > "$ACTUAL"

julia --startup-file=no - "$EXPECTED" "$ACTUAL" "$DIFF" "$ORACLE" <<'JL'
const EXPECTED = ARGS[1]
const ACTUAL = ARGS[2]
const DIFF = ARGS[3]
const ORACLE = ARGS[4]

struct Row
    status::String
    result_type::String
    result_value::String
    exception_type::String
end

function read_rows(path)
    rows = Dict{String,Row}()
    for line in eachline(path)
        startswith(line, "key\t") && continue
        f = split(line, '\t')
        length(f) == 5 || error("malformed row in $path: $line")
        rows[f[1]] = Row(f[2], f[3], f[4], f[5])
    end
    rows
end

expected = read_rows(EXPECTED)
actual = read_rows(ACTUAL)
missing = sort(collect(setdiff(keys(expected), keys(actual))))
extra = sort(collect(setdiff(keys(actual), keys(expected))))
isempty(missing) || error("missing actual rows: $(join(missing[1:min(end, 5)], ", "))")
isempty(extra) || error("extra actual rows: $(join(extra[1:min(end, 5)], ", "))")

diff_rows = String[]
for key in sort(collect(keys(expected)))
    exp = expected[key]
    act = actual[key]
    if exp != act
        push!(diff_rows, join((key, exp.status, exp.result_type, exp.result_value, exp.exception_type, act.status, act.result_type, act.result_value, act.exception_type), '\t'))
    end
end

open(DIFF, "w") do io
    println(io, "key\texpected_status\texpected_type\texpected_value\texpected_exception\tactual_status\tactual_type\tactual_value\tactual_exception")
    for row in diff_rows
        println(io, row)
    end
end

if !isempty(diff_rows)
    println(stderr, "ERROR: AoT numeric matrix reduced subset diverged in $(length(diff_rows)) rows")
    println(stderr, "  example: ", diff_rows[1])
    println(stderr, "diff: $DIFF")
    exit(1)
end

total = count(!startswith("left_type\t"), eachline(ORACLE))
println("OK: AoT numeric matrix reduced subset matched upstream oracle for $(length(expected)) rows; skipped $(total - length(expected)) rows outside the current AoT numeric scope.")
println("actual: $ACTUAL")
println("diff: $DIFF")
JL
