#!/usr/bin/env bash
# probe_base_api_parity.sh
#
# Proactively probe a curated set of Julia Base function call shapes
# under both ./target/release/sjulia and upstream julia, then report
# every divergence side-by-side. Meant to be RUN BY A HUMAN to discover
# new bugs — not a CI gate.
#
# Rationale: sjulia ships many Base function wrappers that were
# implemented for the common 1-arg shape and silently diverge or
# reject other shapes (Issues #4759 #4761 #4764 #4768 #4770 #4772
# #4774 #4777 #4780 / etc. were all the same family). The existing
# `scripts/fixture_julia_parity.sh` validates *existing* fixtures
# under both engines but doesn't discover NEW divergences. This script
# does that discovery: hold a list of probe one-liners, run each
# under both engines, diff the output.
#
# NAMING: deliberately NOT named `check_*.sh` so it does NOT trip the
# `Verify all check_*.sh scripts are referenced in this workflow and
# docs` audit (same convention as `scripts/fixture_julia_parity.sh`).
# This is a developer probe, not a CI gate.
#
# SCOPE: each probe must be a self-contained one-line `println(expr)`
# call whose upstream output is short and stable across Julia patch
# versions. Probes that print float reprs whose digit count varies
# across versions should be avoided.
#
# Usage:
#   bash scripts/probe_base_api_parity.sh                 # run all probes
#   bash scripts/probe_base_api_parity.sh --verbose       # also print matches
#
# Requirements:
#   - julia on PATH
#   - ./target/release/sjulia already built (cargo build --release
#     --bin sjulia --features repl)
#
# Exit code: 0 if zero divergences, 1 otherwise. (You'll usually run
# this with a `|| true` since divergences are the whole point.)

set -uo pipefail

VERBOSE=0
for arg in "$@"; do
    case "$arg" in
        --verbose|-v) VERBOSE=1 ;;
        *)
            echo "unknown argument: $arg" >&2
            exit 2
            ;;
    esac
done

SJULIA="./target/release/sjulia"
if [[ ! -x "$SJULIA" ]]; then
    echo "ERROR: $SJULIA not built. Run:" >&2
    echo "  cargo build --release --bin sjulia --features repl" >&2
    exit 2
fi
if ! command -v julia >/dev/null 2>&1; then
    echo "ERROR: julia not on PATH" >&2
    exit 2
fi

# Each probe is a single line of Julia source. The expected output is
# whatever upstream julia produces — recomputed every run, so probes
# stay valid even as upstream evolves.
PROBES=(
    # Symbol constructor variants
    'println(Symbol("col_", 1))'
    'println(Symbol(:a, "_", :b))'
    'println(Symbol("a", "b", "c", "d"))'
    # repr/string of containers with structured elements
    'println(repr([Pair(1, 2), Pair(3, 4)]))'
    'println(string(filter(iseven, 1:6)))'
    'println(string(sort([3, 1, 2])))'
    'println(string(Dict("a" => 1)))'
    # Tuple display edge cases
    'println(string((1,)))'
    'println(string((1, "hi", '"'"'c'"'"')))'
    'println(string(()))'
    # NamedTuple show form
    'println(string((x = 1, y = "hi", z = '"'"'c'"'"')))'
    # Range display
    'println(string(1:0.5:3))'
    'println(string(0.0:0.5:2.0))'
    # Whole-number Float64 range — narrowing-to-Int64 regression
    # guard (Issue #4760).
    'println(repr(0.0:1.0:5.0))'
    'println(typeof(first(0.0:1.0:5.0)))'
    'println(repr(LinRange(0.0, 1.0, 5)))'
    # Comprehension shapes
    'println(typeof([i+j for i in 1:2, j in 1:3]))'
    'println(size([i+j for i in 1:2, j in 1:3]))'
    'println(string([i+j for i in 1:2, j in 1:3]))'
    # User struct display
    'struct P; x; end; println(repr(P(7)))'
    'struct Q; s::String; end; println(string(Q("hi")))'
    # Float type-preservation
    'println(repr(Float32(1.5)))'
    'println(string(-0.0))'
    'println(repr(typemin(Int64)))'
    # Float-width parity matrix (Issue #4806, prevention for #4802/#4804/#4807):
    # all three widths must use scientific notation at their respective
    # threshold. Cells are written so they ONLY depend on the format
    # path, not on the underlying rounding (avoid magnitudes that
    # require f16 precision tricks).
    'println(string(Float64(1.5e-10)))'
    'println(string(Float64(1.5e20)))'
    'println(string(Float64(1.0e6)))'
    'println(string(Float32(1.5e-10)))'
    'println(string(Float32(1.5e20)))'
    'println(string(Float32(1.0e6)))'
    'println(string(Float16(1.5e3)))'
    # Char / String show-form
    'println(repr('"'"'\n'"'"'))'
    'println(repr("a\nb"))'
    # Vector/Matrix construction
    'println(string([1, 2, 3]))'
    'println(string([1 2; 3 4]))'
    'println(string(Int64[]))'
    # Dict/Set basics (sort to make output order-independent — Set
    # iteration order is implementation-defined and differs across
    # hash table implementations)
    'println(string(sort(collect(Set([1, 2, 3])))))'
    # Higher-order function returns
    'println(string(map(x -> x*2, 1:3)))'
    # Reverse / iteration
    'println(string(reverse([1, 2, 3])))'

    # ---- Narrow integer dispatch matrix (Issue #4791) ----
    # The recurring #4785/#4787/#4789 family: VM ops with arms for
    # I64 only, silently wrong / crashing on narrower widths.
    # abs(typemin) — two's-complement wrap
    'println(abs(typemin(Int8)))'
    'println(abs(typemin(Int16)))'
    'println(abs(typemin(Int32)))'
    'println(abs(typemin(Int64)))'
    'println(abs(typemin(Int128)))'
    # Unary minus typemin (same wrap)
    'println(-typemin(Int8))'
    'println(-typemin(Int16))'
    'println(-typemin(Int32))'
    # count_zeros respects width
    'println(count_zeros(UInt8(0xf0)))'
    'println(count_zeros(UInt16(0xff00)))'
    'println(count_zeros(UInt32(0xffff_0000)))'
    'println(count_zeros(Int8(127)))'
    # leading_zeros / leading_ones respect width
    'println(leading_zeros(UInt8(0x01)))'
    'println(leading_zeros(UInt16(0x0001)))'
    'println(leading_zeros(UInt32(0x0000_0001)))'
    'println(leading_ones(UInt8(0xf0)))'
    'println(leading_ones(UInt16(0xff00)))'
    # bswap preserves element type and bit width
    'println(bswap(UInt16(0x1234)))'
    'println(bswap(UInt32(0x12345678)))'
    'println(bswap(UInt8(0x12)))'
    'println(typeof(bswap(UInt16(0x1234))))'
    'println(typeof(bswap(UInt32(0x12345678))))'
    # bitreverse preserves element type
    'println(typeof(bitreverse(UInt8(0x01))))'
    'println(typeof(bitreverse(UInt16(0x0001))))'
    # signbit / iseven / isodd on narrow integers
    'println(signbit(Int8(-1)))'
    'println(iseven(Int8(2)))'
    'println(isodd(UInt8(3)))'
)

divergences=0
total=${#PROBES[@]}

# Use a single temp file we rewrite for each probe.
TMP_PROBE="$(mktemp -t sjulia-parity-probe.XXXXXX).jl"
trap 'rm -f "$TMP_PROBE"' EXIT

for probe in "${PROBES[@]}"; do
    printf '%s\n' "$probe" >"$TMP_PROBE"

    sj_out="$("$SJULIA" "$TMP_PROBE" 2>&1 || true)"
    jl_out="$(julia "$TMP_PROBE" 2>&1 || true)"

    if [[ "$sj_out" == "$jl_out" ]]; then
        if [[ "$VERBOSE" == "1" ]]; then
            printf '  MATCH:  %s\n' "$probe"
        fi
    else
        divergences=$((divergences + 1))
        printf '\nDIVERGE: %s\n' "$probe"
        printf '  sjulia: %s\n' "${sj_out//$'\n'/$'\n'          }"
        printf '  julia : %s\n' "${jl_out//$'\n'/$'\n'          }"
    fi
done

echo
echo "----"
echo "Total probes: $total"
echo "Divergences:  $divergences"

if [[ "$divergences" -gt 0 ]]; then
    exit 1
fi
exit 0
