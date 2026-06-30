#!/usr/bin/env bash
# check_metaprogramming_roundtrip.sh
#
# Issue #7720: metaprogramming roundtrip gate. Runs a focused Julia
# program under upstream julia and sjulia, then compares Test.jl pass/fail
# summaries. The corpus covers the currently-supported slice of:
#   - Meta.parse source stringification
#   - Meta.parse -> eval
#   - macro-returned Meta.parse Expr -> lowering IR -> run
#
# Requirements:
#   - julia on PATH
#   - ./target/release/sjulia already built with `--features repl`

set -euo pipefail

if [[ $# -ne 0 ]]; then
    echo "Usage: bash scripts/check_metaprogramming_roundtrip.sh" >&2
    exit 2
fi

if ! command -v julia >/dev/null 2>&1; then
    echo "ERROR: 'julia' is not on PATH." >&2
    exit 2
fi

sjulia_bin="./target/release/sjulia"
if [[ ! -x "$sjulia_bin" ]]; then
    echo "ERROR: sjulia binary not built. Run:" >&2
    echo "  cargo build --release --bin sjulia --features repl" >&2
    exit 2
fi

tmp_dir=$(mktemp -d)
sjulia_out="$tmp_dir/sjulia.out"
julia_out="$tmp_dir/julia.out"
roundtrip_fixture="$tmp_dir/metaprogramming_roundtrip_7720.jl"
trap 'rm -rf "$tmp_dir"' EXIT

cat > "$roundtrip_fixture" <<'JULIA'
using Test

macro emit_roundtrip_7720(src)
    return Meta.parse(src)
end

kw_roundtrip_7720(; a=0, b=0) = a + b

@testset "Meta.parse source printing roundtrip" begin
    @test string(Meta.parse("1 + 2")) == "1 + 2"
    @test string(Meta.parse("(1, 2, 3)")) == "(1, 2, 3)"
    @test string(Meta.parse("[1, 2, 3]")) == "[1, 2, 3]"
    @test string(Meta.parse("\"roundtrip\"")) == "roundtrip"
end

@testset "Meta.parse eval roundtrip" begin
    @test eval(Meta.parse("1 + 2")) == 3
    @test eval(Meta.parse("(1, 2, 3)")) == (1, 2, 3)
    @test eval(Meta.parse("[1, 2, 3]")) == [1, 2, 3]
    @test eval(Meta.parse("begin 1; 2; end")) == 2
    @test eval(Meta.parse("if true; 10; else; 20; end")) == 10
    @test eval(Meta.parse("if false; 10; elseif true; 20; else; 30; end")) == 20
    @test eval(Meta.parse("let x = 2; x + 3 end")) == 5
    @test eval(Meta.parse("kw_roundtrip_7720(a=2, b=3)")) == 5
end

@testset "Macro returned Meta.parse lowering roundtrip" begin
    @test (@emit_roundtrip_7720 "1 + 2") == 3
    @test (@emit_roundtrip_7720 "(1, 2, 3)") == (1, 2, 3)
    @test (@emit_roundtrip_7720 "[1, 2, 3]") == [1, 2, 3]
    @test (@emit_roundtrip_7720 "begin 1; 2; end") == 2
    @test (@emit_roundtrip_7720 "if true; 10; else; 20; end") == 10
    @test (@emit_roundtrip_7720 "if false; 10; elseif true; 20; else; 30; end") == 20
    @test (@emit_roundtrip_7720 "let x = 2; x + 3 end") == 5
    @test (@emit_roundtrip_7720 "kw_roundtrip_7720(a=2, b=3)") == 5
end

true
JULIA

extract_summaries() {
    awk 'match($0, /([0-9]+) passed, ([0-9]+) failed/, m) { print m[1] " " m[2] }' "$1" 2>/dev/null \
        || grep -oE '[0-9]+ passed, [0-9]+ failed' "$1" \
           | awk '{print $1 " " $3}' \
        || true
}

extract_upstream_summaries() {
    awk '
        /Pass.*Total/ { in_table = 1; next }
        in_table && /^[[:space:]]*$/ { in_table = 0; next }
        in_table {
            n = split($0, fields, /[[:space:]]+/)
            last = ""; prev = ""
            for (i = 1; i <= n; i++) {
                if (fields[i] ~ /^[0-9]+$/) { prev = last; last = fields[i] }
            }
            if (last != "" && prev != "") {
                failed = last - prev
                print prev " " failed
            }
        }
    ' "$1"
}

if ! timeout 120 "$sjulia_bin" "$roundtrip_fixture" > "$sjulia_out" 2>&1; then
    echo "ERROR: sjulia metaprogramming roundtrip gate failed" >&2
    tail -40 "$sjulia_out" >&2
    exit 1
fi

if ! timeout 120 julia "$roundtrip_fixture" > "$julia_out" 2>&1; then
    echo "ERROR: upstream julia metaprogramming roundtrip gate failed" >&2
    tail -40 "$julia_out" >&2
    exit 1
fi

sjulia_summary=$(extract_summaries "$sjulia_out")
julia_summary=$(extract_summaries "$julia_out")
if [[ -z "$julia_summary" ]]; then
    julia_summary=$(extract_upstream_summaries "$julia_out")
fi

if [[ -z "$sjulia_summary" || -z "$julia_summary" ]]; then
    echo "ERROR: could not extract Test.jl summaries" >&2
    echo "--- sjulia output ---" >&2
    tail -40 "$sjulia_out" >&2
    echo "--- julia output ---" >&2
    tail -40 "$julia_out" >&2
    exit 1
fi

if [[ "$sjulia_summary" != "$julia_summary" ]]; then
    echo "MISMATCH: metaprogramming roundtrip summaries differ." >&2
    echo "sjulia (PASSED FAILED per testset):" >&2
    echo "$sjulia_summary" >&2
    echo "julia (PASSED FAILED per testset):" >&2
    echo "$julia_summary" >&2
    exit 1
fi

echo "OK: metaprogramming roundtrip gate matches upstream Julia (Issue #7720)."
echo "$sjulia_summary" | awk '{print "  " $1 " passed, " $2 " failed"}'
