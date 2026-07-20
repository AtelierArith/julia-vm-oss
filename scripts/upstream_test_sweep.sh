#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/upstream_test_sweep.sh [file-or-stem ...]

Run selected upstream julia/test files with upstream Julia and sjulia, then emit
a TSV summary. Stems such as "int" resolve to "$JULIA_TEST_ROOT/int.jl".

Environment:
  JULIA_TEST_ROOT   upstream julia/test directory (default: julia/test)
  JULIA_BIN         upstream Julia executable (default: julia)
  SJULIA_BIN        sjulia executable (default: ./target/release/sjulia)
  OUT_DIR           log/output directory (default: target/upstream-test-sweep)
  TIMEOUT_SECONDS   per-run timeout (default: 300)
  ALLOWLIST         TSV mapping file stems to known classifications/issues
                    (default: docs/vm/UPSTREAM_TEST_SWEEP_ALLOWLIST.tsv)
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

JULIA_TEST_ROOT="${JULIA_TEST_ROOT:-julia/test}"
JULIA_BIN="${JULIA_BIN:-julia}"
SJULIA_BIN="${SJULIA_BIN:-./target/release/sjulia}"
OUT_DIR="${OUT_DIR:-target/upstream-test-sweep}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-300}"
ALLOWLIST="${ALLOWLIST:-docs/vm/UPSTREAM_TEST_SWEEP_ALLOWLIST.tsv}"

if [[ $# -gt 0 ]]; then
  TARGETS=("$@")
else
  TARGETS=(int operators bool char rational dict sets)
fi

mkdir -p "$OUT_DIR"

resolve_target() {
  local target="$1"
  if [[ "$target" == */* || "$target" == *.jl ]]; then
    printf '%s\n' "$target"
  else
    printf '%s/%s.jl\n' "$JULIA_TEST_ROOT" "$target"
  fi
}

display_path() {
  local file="$1"
  local root="${JULIA_TEST_ROOT%/}"
  if [[ "$file" == "$root"/* ]]; then
    printf 'julia/test/%s\n' "${file#"$root"/}"
  else
    printf '%s\n' "$file"
  fi
}

quote_julia_string() {
  # Escape backslashes and double quotes for a Julia string literal.
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

run_one() {
  local runner="$1"
  local executable="$2"
  local file="$3"
  local log="$4"
  local quoted
  quoted="$(quote_julia_string "$file")"
  set +e
  if [[ "$runner" == "julia" ]]; then
    timeout "$TIMEOUT_SECONDS" "$executable" --startup-file=no \
      -e "using Test; include(\"$quoted\")" >"$log" 2>&1
  else
    timeout "$TIMEOUT_SECONDS" "$executable" \
      -e "using Test; include(\"$quoted\")" >"$log" 2>&1
  fi
  local status=$?
  set -e
  if [[ $status -eq 124 ]]; then
    printf '%s\n' "timeout"
  elif [[ $status -eq 0 ]]; then
    printf '%s\n' "pass"
  else
    printf '%s\n' "error"
  fi
}

first_error_line() {
  local log="$1"
  local line
  line="$(rg -m 1 'Pipeline error|Compilation error|Runtime error|ERROR:|Test Failed|Parse failed|UnsupportedFeature' "$log" 2>/dev/null || true)"
  if [[ -z "$line" ]]; then
    line="$(head -n 1 "$log" 2>/dev/null || true)"
  fi
  local root="${JULIA_TEST_ROOT%/}/"
  line="${line//$root/julia/test/}"
  printf '%s' "$line" | tr '\t' ' ' | tr -d '\r'
}

count_lines() {
  local pattern="$1"
  local log="$2"
  rg -c "$pattern" "$log" 2>/dev/null || printf '0\n'
}

allowlist_column() {
  local stem="$1"
  local column="$2"
  if [[ ! -f "$ALLOWLIST" ]]; then
    return 0
  fi
  awk -F'\t' -v file="$stem" -v column="$column" '
    NR > 1 && $1 == file && $column != "" && !seen[$column]++ {
      if (out != "") {
        out = out ","
      }
      out = out $column
    }
    END {
      printf "%s", out
    }
  ' "$ALLOWLIST"
}

printf 'file\tpath\tupstream_status\tsjulia_status\tupstream_testsets\tsjulia_testsets\tclassification\tissue\tfirst_error\n'

for target in "${TARGETS[@]}"; do
  file="$(resolve_target "$target")"
  path="$(display_path "$file")"
  stem="$(basename "$file" .jl)"
  upstream_log="$OUT_DIR/${stem}.julia.log"
  sjulia_log="$OUT_DIR/${stem}.sjulia.log"

  if [[ ! -f "$file" ]]; then
    printf '%s\t%s\tmissing\tmissing\t0\t0\tscope-out\t\t%s\n' \
      "$stem" "$path" "target file not found"
    continue
  fi

  upstream_status="$(run_one "julia" "$JULIA_BIN" "$file" "$upstream_log")"
  sjulia_status="$(run_one "sjulia" "$SJULIA_BIN" "$file" "$sjulia_log")"
  upstream_testsets="$(count_lines 'Test Summary:' "$upstream_log")"
  sjulia_testsets="$(count_lines '^Test Set:' "$sjulia_log")"

  classification=""
  issue=""
  if [[ "$sjulia_status" != "pass" ]]; then
    classification="$(allowlist_column "$stem" 2)"
    issue="$(allowlist_column "$stem" 3)"
    if [[ -z "$classification" ]]; then
      classification="unclassified"
    fi
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$stem" "$path" "$upstream_status" "$sjulia_status" \
    "$upstream_testsets" "$sjulia_testsets" "$classification" "$issue" \
    "$(first_error_line "$sjulia_log")"
done
