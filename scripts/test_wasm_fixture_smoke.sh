#!/usr/bin/env bash
# Smoke-test wasm_fixture_smoke.sh list parsing without building WASM.

set -euo pipefail
cd "$(dirname "$0")/.."

out="$(scripts/wasm_fixture_smoke.sh --list)"
grep -q $'^arithmetic_basic\tarithmetic/basic.jl\ttrue$' <<<"$out"
grep -q $'^dict_pure_minimal\tdict/test_pure_julia_dict_minimal.jl\ttrue$' <<<"$out"
grep -q $'^tuple_reverse_basic\ttuple/reverse_basic.jl\ttrue$' <<<"$out"
count="$(wc -l <<<"$out" | tr -d ' ')"
test "$count" -ge 30

help="$(scripts/wasm_fixture_smoke.sh --help)"
grep -q 'ALLOWLIST_TSV' <<<"$help"
grep -q 'scripts/wasm_fixture_smoke.sh' .github/workflows/nightly-gates.yml
