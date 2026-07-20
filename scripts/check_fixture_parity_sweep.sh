#!/usr/bin/env bash
# check_fixture_parity_sweep.sh — Issue #10246 (escapes: #10237; siblings #10223/#10201/#10213)
#
# Upstream-parity sweep over registered fixture categories.
#
# Issue #10237 found 13 fixtures that are green in the sjulia harness but RED
# under upstream julia 1.12.6 — the fixtures assert sjulia's wrong behavior
# (fixture drift), and nothing in the harness or CI compares fixtures against
# upstream by default. This gate closes that hole: it runs every registered
# fixture of the selected categories through scripts/fixture_julia_parity.sh
# (sjulia vs `julia --startup-file=no`, comparing Test.jl pass/fail summaries,
# or the wrapped final value for legacy bare-boolean fixtures) and ratchets the
# known divergences through an explicit allowlist:
#
#   docs/vm/FIXTURE_PARITY_SWEEP_ALLOWLIST.tsv
#     columns: file<TAB>classification<TAB>issue<TAB>reason
#     (file is the category-prefixed manifest path, e.g. strings/replace_basic.jl)
#
# TWO-SIDED RATCHET (same shape as docs/vm/TESTSET_FAILURE_ALLOWLIST.tsv):
#   * a swept fixture that DIVERGES and is NOT allowlisted fails the gate
#     (no NEW drifted fixture can land silently);
#   * an allowlisted fixture in a swept category that NO LONGER diverges fails
#     as a stale entry (the allowlist monotonically shrinks as drift is fixed).
#
# Skipped (reported, never judged): manifest entries with `skip = true`,
# `skip_julia_test = true` (intentional sjulia extensions, e.g. Issue #302),
# entries with a per-test `env` table (their sjulia semantics depend on harness
# env the sweep does not replicate), and fixtures grandfathered in
# docs/vm/TESTSET_FAILURE_ALLOWLIST.tsv (already tracked as broken).
#
# Usage (from the repository root; sjulia binary + upstream julia required):
#   bash scripts/check_fixture_parity_sweep.sh ref strings          # scoped (local)
#   bash scripts/check_fixture_parity_sweep.sh --jobs 8 --all       # full sweep (nightly)
#
# Options / environment:
#   --jobs N     parallel fixture runs (default: 4)
#   --strict     forwarded to fixture_julia_parity.sh (PARITY_TARGET julia
#                version mismatch becomes a hard error, Issue #8644/#8667)
#   --all        sweep every category directory with a manifest.toml
#   SJULIA_BIN   sjulia binary (default: $CARGO_TARGET_DIR/release/sjulia)
#   OUT_DIR      log/result directory (default: target/fixture-parity-sweep)
#   ALLOWLIST    override allowlist path (for self-tests)
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

source "$REPO_ROOT/scripts/cargo_target_dir.sh"
cargo_target_dir="$(resolve_cargo_target_dir "$REPO_ROOT")"
export CARGO_TARGET_DIR="$cargo_target_dir"

FIXTURES_DIR="${SJULIA_FIXTURES_DIR:-$REPO_ROOT/subset_julia_vm/tests/fixtures}"
ALLOWLIST="${ALLOWLIST:-$REPO_ROOT/docs/vm/FIXTURE_PARITY_SWEEP_ALLOWLIST.tsv}"
TESTSET_ALLOWLIST="$REPO_ROOT/docs/vm/TESTSET_FAILURE_ALLOWLIST.tsv"
OUT_DIR="${OUT_DIR:-$REPO_ROOT/target/fixture-parity-sweep}"
SJULIA_BIN="${SJULIA_BIN:-$cargo_target_dir/release/sjulia}"
export SJULIA_BIN

JOBS=4
STRICT_FLAG=""
ALL=0
categories=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --jobs)
      JOBS="${2:?--jobs requires a value}"
      shift 2
      ;;
    --strict)
      STRICT_FLAG="--strict"
      shift
      ;;
    --all)
      ALL=1
      shift
      ;;
    -h|--help)
      sed -n '2,45p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      categories+=("$1")
      shift
      ;;
  esac
done

if [[ "$ALL" -eq 1 ]]; then
  categories=()
  for manifest in "$FIXTURES_DIR"/*/manifest.toml; do
    [[ -f "$manifest" ]] || continue
    categories+=("$(basename "$(dirname "$manifest")")")
  done
fi

if [[ ${#categories[@]} -eq 0 ]]; then
  echo "Usage: bash scripts/check_fixture_parity_sweep.sh [--jobs N] [--strict] (--all | <category> ...)" >&2
  exit 2
fi

if [[ ! -x "$SJULIA_BIN" ]]; then
  echo "FAIL: sjulia binary not built ($SJULIA_BIN). Run:" >&2
  echo "  cargo build --release -p subset_julia_vm --bin sjulia --features repl" >&2
  echo "  (or set SJULIA_BIN to an existing sjulia binary)" >&2
  exit 2
fi

RESULTS_DIR="$OUT_DIR/results"
rm -rf "$RESULTS_DIR"
mkdir -p "$RESULTS_DIR"

# allowlisted_files: first column of the sweep allowlist (comments stripped).
allowlisted_files=""
if [[ -f "$ALLOWLIST" ]]; then
  allowlisted_files="$(awk -F'\t' '!/^[[:space:]]*(#|$)/ && $1 != "file" { print $1 }' "$ALLOWLIST")"
fi

# testset_grandfathered: fixtures already tracked broken-in-harness (#9360).
testset_grandfathered=""
if [[ -f "$TESTSET_ALLOWLIST" ]]; then
  testset_grandfathered="$(awk -F'\t' '!/^[[:space:]]*(#|$)/ { print $1 }' "$TESTSET_ALLOWLIST")"
fi

in_list() {
  local needle="$1" list="$2"
  [[ -n "$list" ]] && grep -Fxq "$needle" <<<"$list"
}

# Fixtures that `using`/`import` a package sjulia bundles but a vanilla
# upstream `julia --startup-file=no` cannot load (MacroTools, AbstractAlgebra,
# Plots, …) are not upstream-comparable in this environment. Skip anything
# importing a module outside upstream's Base/stdlib set.
UPSTREAM_STDLIB_MODULES=" Base Core Test Random LinearAlgebra Printf Dates Statistics InteractiveUtils Serialization SHA Logging Unicode Markdown REPL Pkg TOML UUIDs Base64 CRC32c DelimitedFiles Distributed Downloads FileWatching Future GMP LazyArtifacts LibCURL LibGit2 Libdl Mmap NetworkOptions Profile Sockets SparseArrays SuiteSparse Tar SharedArrays "

uses_non_stdlib_package() {
  local fixture_path="$1"
  local module
  while IFS= read -r module; do
    [[ -n "$module" ]] || continue
    if [[ "$UPSTREAM_STDLIB_MODULES" != *" $module "* ]]; then
      return 0
    fi
  done < <(awk '
    /^[[:space:]]*(using|import)[[:space:]]/ {
      line = $0
      sub(/^[[:space:]]*(using|import)[[:space:]]+/, "", line)
      sub(/:.*$/, "", line)      # `using Foo: bar` -> Foo
      n = split(line, mods, /,/)
      for (i = 1; i <= n; i++) {
        m = mods[i]
        gsub(/[[:space:]]/, "", m)
        sub(/\..*$/, "", m)      # `import Base.show` -> Base
        if (m != "") print m
      }
    }
  ' "$fixture_path")
  return 1
}

# ---------------------------------------------------------------------------
# 1. Collect sweep candidates from the category manifests.
#    Emits: file<TAB>skip<TAB>skip_julia_test<TAB>has_env per [[tests]] entry.
# ---------------------------------------------------------------------------
manifest_entries() {
  awk '
    function flush() {
      if (file != "") {
        print file "\t" skip "\t" skipjulia "\t" hasenv
      }
      file = ""; skip = 0; skipjulia = 0; hasenv = 0
    }
    /^\[\[tests\]\]/ { flush(); inblock = 1; next }
    inblock && /^file[[:space:]]*=/ {
      f = $0
      sub(/^[^=]*=[[:space:]]*"/, "", f)
      sub(/".*$/, "", f)
      file = f
      next
    }
    inblock && /^skip[[:space:]]*=[[:space:]]*true/ { skip = 1; next }
    inblock && /^skip_julia_test[[:space:]]*=[[:space:]]*true/ { skipjulia = 1; next }
    inblock && (/^env[[:space:]]*=/ || /^\[tests\.env\]/) { hasenv = 1; next }
    END { flush() }
  ' "$1"
}

candidates=()
skipped_report=()
for category in "${categories[@]}"; do
  manifest="$FIXTURES_DIR/$category/manifest.toml"
  if [[ ! -f "$manifest" ]]; then
    echo "FAIL: no manifest.toml for category '$category' ($manifest)" >&2
    exit 2
  fi
  while IFS=$'\t' read -r file skip skipjulia hasenv; do
    [[ -n "$file" ]] || continue
    # Category manifests may already carry a path with a directory component.
    if [[ "$file" == */* ]]; then
      rel="$file"
    else
      rel="$category/$file"
    fi
    if [[ "$skip" == 1 ]]; then
      skipped_report+=("$rel	skip=true")
    elif [[ "$skipjulia" == 1 ]]; then
      skipped_report+=("$rel	skip_julia_test=true")
    elif [[ "$hasenv" == 1 ]]; then
      skipped_report+=("$rel	per-test env (harness-only semantics)")
    elif in_list "$rel" "$testset_grandfathered"; then
      skipped_report+=("$rel	TESTSET_FAILURE_ALLOWLIST (already tracked broken)")
    elif [[ -f "$FIXTURES_DIR/$rel" ]] && uses_non_stdlib_package "$FIXTURES_DIR/$rel"; then
      skipped_report+=("$rel	uses a bundled non-stdlib package (not loadable by vanilla upstream julia)")
    else
      candidates+=("$rel")
    fi
  done < <(manifest_entries "$manifest")
done

echo "== fixture upstream-parity sweep (Issue #10246 / #10237) =="
echo "categories: ${categories[*]}"
echo "candidates: ${#candidates[@]}  skipped: ${#skipped_report[@]}  jobs: $JOBS"

if [[ ${#candidates[@]} -eq 0 ]]; then
  echo "FAIL: selected categories register no sweepable fixtures." >&2
  exit 2
fi

# ---------------------------------------------------------------------------
# 2. Run fixture_julia_parity.sh per candidate (parallel via xargs -P).
# ---------------------------------------------------------------------------
run_one() {
  local rel="$1"
  local slug="${rel//\//__}"
  local log="$RESULTS_DIR/$slug.log"
  # --red-green: divergence = a fixture that is red under one interpreter and
  # green under the other (or whose wrapped final value differs). Per-testset
  # pass-count comparison is NOT sweep-safe while sjulia's outer-@testset
  # summary does not aggregate nested counts (Issue #10338).
  if bash "$REPO_ROOT/scripts/fixture_julia_parity.sh" --red-green ${STRICT_FLAG:+"$STRICT_FLAG"} \
    "$FIXTURES_DIR_REL/$rel" >"$log" 2>&1; then
    printf 'ok\n' >"$RESULTS_DIR/$slug.status"
  else
    printf 'diverged\n' >"$RESULTS_DIR/$slug.status"
  fi
}
export -f run_one
export RESULTS_DIR REPO_ROOT STRICT_FLAG
# fixture_julia_parity.sh resolves the manifest next to the fixture; pass a
# repo-relative path so its output stays readable.
FIXTURES_DIR_REL="subset_julia_vm/tests/fixtures"
export FIXTURES_DIR_REL

printf '%s\n' "${candidates[@]}" | xargs -P "$JOBS" -I{} bash -c 'run_one "$1"' _ {}

# ---------------------------------------------------------------------------
# 3. Aggregate; retry divergences ONCE sequentially (a heavily parallel first
#    pass can time out large fixtures — observed on
#    types/types_agg_predicates_9671.jl at --jobs 12 — and a flaky timeout must
#    not masquerade as upstream drift), then apply the two-sided ratchet.
# ---------------------------------------------------------------------------
diverged=()
for rel in "${candidates[@]}"; do
  slug="${rel//\//__}"
  status="$(cat "$RESULTS_DIR/$slug.status" 2>/dev/null || echo missing)"
  if [[ "$status" != "ok" ]]; then
    echo "retrying sequentially (parallel-pass divergence): $rel"
    run_one "$rel"
    status="$(cat "$RESULTS_DIR/$slug.status" 2>/dev/null || echo missing)"
  fi
  if [[ "$status" != "ok" ]]; then
    diverged+=("$rel")
  fi
done

new_divergences=()
known_divergences=()
for rel in "${diverged[@]+"${diverged[@]}"}"; do
  if in_list "$rel" "$allowlisted_files"; then
    known_divergences+=("$rel")
  else
    new_divergences+=("$rel")
  fi
done

# Stale entries: allowlisted files whose category was swept in this run but
# which did NOT diverge (fixed drift, renamed, or deleted fixture).
stale_entries=()
if [[ -n "$allowlisted_files" ]]; then
  while IFS= read -r listed; do
    [[ -n "$listed" ]] || continue
    listed_category="${listed%%/*}"
    swept=0
    for category in "${categories[@]}"; do
      [[ "$category" == "$listed_category" ]] && swept=1 && break
    done
    [[ "$swept" -eq 1 ]] || continue
    was_candidate=0
    for rel in "${candidates[@]}"; do
      [[ "$rel" == "$listed" ]] && was_candidate=1 && break
    done
    if [[ "$was_candidate" -eq 0 ]]; then
      # Skipped entries are exempt; a vanished file is stale.
      if printf '%s\n' "${skipped_report[@]+"${skipped_report[@]}"}" | grep -q "^$listed	"; then
        continue
      fi
      stale_entries+=("$listed (not registered in the swept manifest — renamed or deleted?)")
      continue
    fi
    still_diverges=0
    for rel in "${diverged[@]+"${diverged[@]}"}"; do
      [[ "$rel" == "$listed" ]] && still_diverges=1 && break
    done
    if [[ "$still_diverges" -eq 0 ]]; then
      stale_entries+=("$listed (no longer diverges — remove the allowlist row)")
    fi
  done <<<"$allowlisted_files"
fi

echo
echo "== sweep report =="
echo "swept:              ${#candidates[@]}"
echo "green (parity ok):  $(( ${#candidates[@]} - ${#diverged[@]} ))"
echo "known divergences:  ${#known_divergences[@]} (allowlisted)"
echo "NEW divergences:    ${#new_divergences[@]}"
echo "stale allowlist:    ${#stale_entries[@]}"
if [[ ${#skipped_report[@]} -gt 0 ]]; then
  echo "skipped (${#skipped_report[@]}):"
  printf '  %s\n' "${skipped_report[@]}"
fi
if [[ ${#known_divergences[@]} -gt 0 ]]; then
  echo "known divergences (tracked in $(basename "$ALLOWLIST")):"
  printf '  %s\n' "${known_divergences[@]}"
fi

failed=0
if [[ ${#new_divergences[@]} -gt 0 ]]; then
  failed=1
  echo >&2
  echo "FAIL: ${#new_divergences[@]} fixture(s) diverge from upstream julia and are NOT allowlisted (Issue #10246):" >&2
  for rel in "${new_divergences[@]}"; do
    slug="${rel//\//__}"
    echo "  $rel" >&2
    sed 's/^/    | /' "$RESULTS_DIR/$slug.log" 2>/dev/null | tail -8 >&2
  done
  echo "Triage each one (Issue #10237 classes): sjulia-bug (fixture asserts wrong VM" >&2
  echo "behavior — file/link the bug Issue) or bad-fixture (fix the assertion)." >&2
  echo "A deliberately deferred divergence needs a row in $ALLOWLIST with its Issue." >&2
fi
if [[ ${#stale_entries[@]} -gt 0 ]]; then
  failed=1
  echo >&2
  echo "FAIL: ${#stale_entries[@]} stale allowlist entr(ies) in $ALLOWLIST (two-sided ratchet):" >&2
  printf '  %s\n' "${stale_entries[@]}" >&2
fi

if [[ "$failed" -ne 0 ]]; then
  exit 1
fi
echo "OK: all swept fixtures match upstream julia (allowlisted divergences excepted)."
