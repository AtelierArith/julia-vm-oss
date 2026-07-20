#!/usr/bin/env bash
# audit_health_report.sh — run the full audit family and produce a structured
# health report.
#
# Mirrors the batch logic in .github/workflows/pr-fast.yml so local runs and
# CI see the same failures. Excludes audits that need a built sjulia binary or
# network access by default.
#
# Usage:
#   bash scripts/audit_health_report.sh
#   bash scripts/audit_health_report.sh --format json
#   bash scripts/audit_health_report.sh --format markdown
#   bash scripts/audit_health_report.sh --with-build-deps
#   bash scripts/audit_health_report.sh --jobs 8
#   bash scripts/audit_health_report.sh --fast        # ratchet-only, ~seconds
#
# Exit code: number of failed audit scripts.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 2

FORMAT="text"
WITH_BUILD_DEPS=0
JOBS=4
FAST=0
while [ $# -gt 0 ]; do
  case "$1" in
    --format)
      FORMAT="$2"
      if [[ "$FORMAT" != "text" && "$FORMAT" != "json" && "$FORMAT" != "markdown" ]]; then
        echo "ERROR: --format must be text, json, or markdown" >&2
        exit 2
      fi
      shift 2
      ;;
    --with-build-deps)
      WITH_BUILD_DEPS=1
      shift
      ;;
    --jobs)
      JOBS="$2"
      if ! [[ "$JOBS" =~ ^[1-9][0-9]*$ ]]; then
        echo "ERROR: --jobs must be a positive integer" >&2
        exit 2
      fi
      shift 2
      ;;
    --fast)
      FAST=1
      shift
      ;;
    -h|--help)
      sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "ERROR: unknown arg $1 (see --help)" >&2
      exit 2
      ;;
  esac
done

COMMIT="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
DATE="$(date +%F)"

# Audits that always run (no build, no network).
CHECK_SCRIPTS=()
while IFS= read -r script; do
  CHECK_SCRIPTS+=("$script")
done < <(find scripts -maxdepth 1 -name 'check_*.sh' | sort)

AUDIT_SCRIPTS=()
while IFS= read -r script; do
  AUDIT_SCRIPTS+=("$script")
done < <(find scripts -maxdepth 1 -name 'audit_*.sh' | sort)

# Audits requiring a built sjulia binary or julia on PATH.
BUILD_DEPS_SCRIPTS=(
  "scripts/check_metaprogramming_roundtrip.sh"
)

# Audits requiring network access.
NETWORK_SCRIPTS=(
  "scripts/check_vendored_drift.sh"
)

# Fast ratchet audits suitable for daily local monitoring (~seconds).
FAST_AUDITS=(
  "scripts/check_structural_debt_inventory.sh"
  "scripts/check_no_panic_in_tests.sh"
  "scripts/check_vmerror_classification.sh"
  "scripts/check_missing_debug.sh"
  "scripts/check_base_routing_registry.sh"
  "scripts/check_rust_semantics_ratchet.sh"
  "scripts/check_complex_interleaved_allowlist.sh"
  "scripts/check_no_expect_in_bin.sh"
  "scripts/check_workarounds_documented.sh"
  "scripts/check_workarounds_sync.sh"
  "scripts/check_instr_wire_ids.sh"
  "scripts/check_ffi_header_compiles.sh"
  "scripts/audit_compile_vm_coupling.sh"
  "scripts/check_no_public_base_stdlib_routes.sh"
  "scripts/check_no_hardcoded_var_names_in_inference.sh"
  "scripts/check_no_typevar_name_heuristic.sh"
)

is_excluded() {
  local script="$1"
  local excluded
  for excluded in "${BUILD_DEPS_SCRIPTS[@]}" "${NETWORK_SCRIPTS[@]}"; do
    if [[ "$script" == "$excluded" ]]; then
      return 0
    fi
  done
  return 1
}

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

scripts_to_run=()
if [ "$FAST" -eq 1 ]; then
  for script in "${FAST_AUDITS[@]}"; do
    if [ -f "$script" ]; then
      scripts_to_run+=("$script")
    fi
  done
else
  for script in "${CHECK_SCRIPTS[@]}" "${AUDIT_SCRIPTS[@]}"; do
    if is_excluded "$script"; then
      continue
    fi
    scripts_to_run+=("$script")
  done
  if [ "$WITH_BUILD_DEPS" -eq 1 ]; then
    for script in "${BUILD_DEPS_SCRIPTS[@]}"; do
      if [ -f "$script" ]; then
        scripts_to_run+=("$script")
      fi
    done
  fi
fi

# Run each audit in parallel with a job-slot limit.
run_one() {
  local idx="$1"
  local script="$2"
  local out="$TMPDIR/$idx.out"
  local err="$TMPDIR/$idx.err"
  local start status elapsed
  start=$(date +%s)
  if bash "$script" >"$out" 2>"$err"; then
    status="passed"
  else
    status="failed"
  fi
  elapsed=$(($(date +%s) - start))
  printf '%s\t%s\t%s\t%s\n' "$idx" "$script" "$status" "$elapsed" >> "$TMPDIR/results.tsv"
}

idx=0
running=0
for script in "${scripts_to_run[@]}"; do
  run_one "$idx" "$script" &
  running=$((running + 1))
  idx=$((idx + 1))
  if [ "$running" -ge "$JOBS" ]; then
    wait
    running=0
  fi
done
wait

failed_count=0
passed_count=0

# Text output.
print_text() {
  echo "Audit health report — $DATE @ $COMMIT"
  echo ""
  echo "Executed ${#scripts_to_run[@]} audit scripts"
  echo ""
  local has_failures=0
  while IFS=$'\t' read -r ridx script status elapsed; do
    if [ "$status" = "failed" ]; then
      has_failures=1
      failed_count=$((failed_count + 1))
      echo "FAIL  $script (${elapsed}s)"
      if [ -s "$TMPDIR/$ridx.out" ]; then
        sed 's/^/      /' "$TMPDIR/$ridx.out"
      fi
      if [ -s "$TMPDIR/$ridx.err" ]; then
        sed 's/^/      /' "$TMPDIR/$ridx.err"
      fi
    else
      passed_count=$((passed_count + 1))
    fi
  done < "$TMPDIR/results.tsv"
  if [ "$has_failures" -eq 0 ]; then
    echo "All audits passed."
  fi
  echo ""
  echo "Passed: $passed_count, Failed: $failed_count"
  echo ""
  echo "Skipped by default (use --with-build-deps to include build-deps):"
  for script in "${BUILD_DEPS_SCRIPTS[@]}"; do echo "  $script"; done
  echo ""
  echo "Skipped by default (network access required):"
  for script in "${NETWORK_SCRIPTS[@]}"; do echo "  $script"; done
}

# JSON output.
print_json() {
  echo "{"
  echo "  \"date\": \"$DATE\","
  echo "  \"commit\": \"$COMMIT\","
  echo "  \"audits\": ["
  local first=1
  while IFS=$'\t' read -r ridx script status elapsed; do
    if [ "$first" -eq 0 ]; then echo ","; fi
    first=0
    local escaped_script
    escaped_script="$(printf '%s' "$script" | sed 's/\\/\\\\/g; s/"/\\"/g')"
    echo -n "    {\"script\": \"$escaped_script\", \"status\": \"$status\", \"elapsed_seconds\": $elapsed}"
    if [ "$status" = "failed" ]; then
      failed_count=$((failed_count + 1))
    else
      passed_count=$((passed_count + 1))
    fi
  done < "$TMPDIR/results.tsv"
  echo ""
  echo "  ],"
  echo "  \"passed\": $passed_count,"
  echo "  \"failed\": $failed_count,"
  echo -n "  \"skipped_build_deps\": ["
  local first_skip=1
  for script in "${BUILD_DEPS_SCRIPTS[@]}"; do
    if [ "$first_skip" -eq 0 ]; then echo -n ", "; fi
    first_skip=0
    local escaped
    escaped="$(printf '%s' "$script" | sed 's/\\/\\\\/g; s/"/\\"/g')"
    echo -n "\"$escaped\""
  done
  echo "],"
  echo -n "  \"skipped_network\": ["
  first_skip=1
  for script in "${NETWORK_SCRIPTS[@]}"; do
    if [ "$first_skip" -eq 0 ]; then echo -n ", "; fi
    first_skip=0
    local escaped
    escaped="$(printf '%s' "$script" | sed 's/\\/\\\\/g; s/"/\\"/g')"
    echo -n "\"$escaped\""
  done
  echo "]"
  echo "}"
}

# Markdown output.
print_markdown() {
  echo "# Audit Health Report — $DATE @ $COMMIT"
  echo ""
  echo "| Script | Status | Elapsed (s) |"
  echo "|--------|--------|-------------|"
  while IFS=$'\t' read -r ridx script status elapsed; do
    local badge
    if [ "$status" = "passed" ]; then
      badge="✅ PASS"
      passed_count=$((passed_count + 1))
    else
      badge="❌ FAIL"
      failed_count=$((failed_count + 1))
    fi
    echo "| $script | $badge | $elapsed |"
  done < "$TMPDIR/results.tsv"
  echo ""
  echo "**Summary:** $passed_count passed, $failed_count failed."
  echo ""
  echo "Skipped by default:"
  for script in "${BUILD_DEPS_SCRIPTS[@]}" "${NETWORK_SCRIPTS[@]}"; do
    echo "- $script"
  done
}

case "$FORMAT" in
  text) print_text ;;
  json) print_json ;;
  markdown) print_markdown ;;
esac

exit "$failed_count"
