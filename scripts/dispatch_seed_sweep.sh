#!/usr/bin/env bash
# dispatch_seed_sweep.sh — seed-variation determinism sweep (Issue #8659,
# parent #8641; #5966 precedent).
#
# std `HashMap`/`HashSet` get a fresh random hash seed in every process, so a
# dispatch decision that leaks iteration order produces output that differs
# BETWEEN PROCESSES, not between repeated runs inside one process (which is
# exactly why the #5966 class of bug passes targeted tests and only dies in
# the full suite). This sweep runs each fixture in N separate `sjulia`
# processes and fails when any fixture's combined stdout+stderr+exit-code
# hash differs across runs.
#
# Usage:
#   bash scripts/dispatch_seed_sweep.sh [--runs N] [--all] [category ...]
#
#   --runs N     processes per fixture (default 3)
#   --all        sweep every fixture category (nightly scope)
#   category...  fixture category dirs under subset_julia_vm/tests/fixtures/
#                (default: the dispatch-heavy set below)
#
# Requires a release binary: cargo build --release -p subset_julia_vm --bin
# sjulia --features repl (or set SJULIA_BIN). Fixtures are independent
# programs, so per-process Base compilation is served by the persistent
# on-disk Base cache after the first run.
#
# This is a developer/nightly harness (needs a built binary + minutes of
# runtime), so it is intentionally NOT named `check_*.sh` (see
# docs/vm/CODE_AUDITS.md "Rename out of the audit perimeter"); the
# nightly-gates workflow invokes it on a schedule.

set -euo pipefail
cd "$(dirname "$0")/.."

SJULIA_BIN="${SJULIA_BIN:-target/release/sjulia}"
if [ ! -x "$SJULIA_BIN" ]; then
  echo "ERROR: $SJULIA_BIN not found. Build it first:"
  echo "  cargo build --release -p subset_julia_vm --bin sjulia --features repl"
  exit 2
fi

RUNS=3
ALL=0
CATEGORIES=()
while [ $# -gt 0 ]; do
  case "$1" in
    --runs)
      RUNS="$2"
      shift 2
      ;;
    --all)
      ALL=1
      shift
      ;;
    *)
      CATEGORIES+=("$1")
      shift
      ;;
  esac
done

FIXTURE_ROOT=subset_julia_vm/tests/fixtures
if [ "$ALL" -eq 1 ]; then
  CATEGORIES=()
  while IFS= read -r dir; do
    CATEGORIES+=("$(basename "$dir")")
  done < <(find "$FIXTURE_ROOT" -mindepth 1 -maxdepth 1 -type d | sort)
elif [ ${#CATEGORIES[@]} -eq 0 ]; then
  # Dispatch-heavy default: the categories most likely to surface an
  # iteration-order leak in method selection / tie-breaks / promotion.
  # `complex` is included because Issue #10775 (concrete `Complex{Float64}`/
  # `Complex{Float32}` methods nondeterministically matching a
  # `Complex{Int64}` argument, e.g. `abs2(Complex(2,3))`) was a
  # process-seed-dependent dispatch bug in exactly this category, latent
  # enough to pass single-run fixture checks.
  CATEGORIES=(dispatch dispatch_parity promotion operators closures complex)
fi

# Declared per-fixture output normalization (Issue #11474). Some fixtures
# intentionally print nondeterministic values that vary between processes for
# reasons unrelated to hash seeds (e.g. `@time`'s elapsed seconds). Registered
# fixtures get a NARROW normalization applied before hashing; exit status and
# all other output stay covered. Registry: docs/vm/DISPATCH_SEED_SWEEP_NORMALIZATION.tsv
# (fixture-path <TAB> normalization-kind). Stale entries (missing fixture or
# unknown kind) FAIL the sweep so the list cannot rot.
NORMALIZATION_TSV=docs/vm/DISPATCH_SEED_SWEEP_NORMALIZATION.tsv
declare -A NORMALIZE_KIND=()
if [ -f "$NORMALIZATION_TSV" ]; then
  while IFS=$'\t' read -r nfix nkind; do
    case "$nfix" in ''|'#'*) continue ;; esac
    if [ ! -f "$nfix" ]; then
      echo "FAIL: $NORMALIZATION_TSV lists missing fixture: $nfix (stale entry)"
      exit 1
    fi
    case "$nkind" in
      elapsed-time) ;;
      *)
        echo "FAIL: $NORMALIZATION_TSV has unknown normalization kind '$nkind' for $nfix"
        exit 1
        ;;
    esac
    NORMALIZE_KIND["$nfix"]="$nkind"
  done < "$NORMALIZATION_TSV"
fi

normalize_output() {
  # Apply the fixture's registered normalization (if any) to stdout+stderr
  # before hashing. `elapsed-time` masks sjulia `@time`'s "  <float> seconds"
  # line (base/timing.jl), including scientific notation, and nothing else.
  local file="$1"
  case "${NORMALIZE_KIND[$file]:-}" in
    elapsed-time)
      sed -E 's/^(  )[0-9]+(\.[0-9]+)?(e-?[0-9]+)? seconds$/\1<ELAPSED> seconds/'
      ;;
    *)
      cat
      ;;
  esac
}

hash_run() {
  # Combined stdout+stderr+exit-code hash of one fixture run in a fresh
  # process (fresh HashMap seed).
  local file="$1"
  local out rc
  out=$("$SJULIA_BIN" "$file" 2>&1) && rc=0 || rc=$?
  printf '%s\nexit:%s\n' "$out" "$rc" | normalize_output "$file" | sha256sum | cut -d' ' -f1
}

total=0
mismatched=0
for category in "${CATEGORIES[@]+"${CATEGORIES[@]}"}"; do
  dir="$FIXTURE_ROOT/$category"
  if [ ! -d "$dir" ]; then
    echo "WARN: no such fixture category: $category"
    continue
  fi
  while IFS= read -r fixture; do
    total=$((total + 1))
    first=$(hash_run "$fixture")
    i=1
    while [ "$i" -lt "$RUNS" ]; do
      h=$(hash_run "$fixture")
      if [ "$h" != "$first" ]; then
        echo "SEED-VARIANT OUTPUT: $fixture (run $((i + 1)) differs from run 1)"
        mismatched=$((mismatched + 1))
        break
      fi
      i=$((i + 1))
    done
  done < <(find "$dir" -name '*.jl' | sort)
done

echo "Swept $total fixture(s) x $RUNS process(es); $mismatched with seed-variant output."
if [ "$mismatched" -gt 0 ]; then
  echo "FAIL: dispatch output depends on per-process hash seed (Issue #8641 policy)."
  exit 1
fi
echo "OK: all outputs identical across processes."
