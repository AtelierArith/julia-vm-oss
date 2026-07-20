#!/usr/bin/env bash
# check_cache_sensitive_fixture_lane.sh — Issue #10223 (parent #10246; escape #10092)
#
# Both-cache-mode lane for cache-sensitive fixture categories.
#
# Issue #10092 (WeakRef target surviving GC.gc()) was only reproducible with the
# persistent Base bytecode cache present: the cache-RESTORE paths rebuilt
# `struct_table` with `has_inner_constructor: false`, so `WeakRef(x)` skipped the
# outer constructor and the weak cell was never registered with the GC. The
# fixture harness runs each fixture under exactly ONE cache configuration, so a
# bug that manifests only with (or only without) the persistent caches is
# invisible to any single-mode run.
#
# This lane closes that hole for the fixture classes where cache mode is part of
# the tested semantics (GC / WeakRef / finalizer / struct-table identity): it
# runs every fixture category containing a manifest entry tagged
# `cache_sensitive = true` under BOTH cache modes and fails on divergence:
#
#   cold pass:   persistent caches removed + SUBSET_JULIA_VM_DISABLE_CACHE=1
#                SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE=1
#                SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE=1
#                (Base/prelude compiled from source in every test process)
#   prime pass:  default env; regenerates the persistent caches on disk. This is
#                also a real coverage lane (the default cache-writing path): a
#                RED prime pass fails the lane, so a regression that only breaks
#                the default-cache path cannot slip through by leaving cold and
#                cached in agreement.
#   cached pass: default env with the persistent caches present — every test
#                process RESTORES Base from the persistent cache, exercising
#                the restore paths (`build_struct_tables`,
#                `restore_compile_context_from_program`) where #10092 lived
#
# The lane fails if: the prime pass is red, OR the cold and cached passes
# disagree (a cache-transparency regression in a cache-sensitive category), OR
# all passes are red. It succeeds only when all three passes are green.
# Only tagged categories run three times, so suite wall-clock impact stays
# bounded (do NOT tag broad categories without need; the whole-suite
# counterpart is the nightly `check_cold_cached_nextest.sh` job, Issue #8719).
#
# Tagging: add `cache_sensitive = true` to a `[[tests]]` entry in the
# category's manifest.toml (declarative metadata, accepted by the harness's
# `deny_unknown_fields` manifest structs — see `TestCase` in
# subset_julia_vm/tests/fixture_tests.rs and subset_julia_vm/build.rs).
#
# Usage (from the repository root):
#   bash scripts/check_cache_sensitive_fixture_lane.sh              # tagged categories
#   bash scripts/check_cache_sensitive_fixture_lane.sh ref          # explicit categories
#
# Environment:
#   SJULIA_CACHE_LANE_CARGO_PROFILE  cargo profile for nextest (default: release;
#                                    use release-fast for faster local iteration)
#   CARGO_TARGET_DIR                 honored for persistent-cache location
#
# NOTE: do not run concurrently with another cargo build/nextest — the cold
# pass removes the shared persistent caches under the target dir.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

FIXTURES_DIR="${SJULIA_FIXTURES_DIR:-$REPO_ROOT/subset_julia_vm/tests/fixtures}"
CARGO_PROFILE="${SJULIA_CACHE_LANE_CARGO_PROFILE:-release}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"

if [[ "$CARGO_PROFILE" == "release" ]]; then
  profile_args=(--release)
else
  profile_args=(--cargo-profile "$CARGO_PROFILE")
fi

# Mirror of `sanitize_mod_name` in subset_julia_vm/build.rs: a fixture category
# directory maps to the generated nextest module `<sanitized>::`. KEEP IN SYNC
# with build.rs (the empty-filter guard below catches drift loudly).
sanitize_mod_name() {
  local name="${1//[-. ]/_}"
  case "$name" in
    abstract|type|types|struct|where|mod|module|fn|function|loop|for|while|if|\
    else|match|return|break|continue|const|static|mut|ref|self|super|crate|\
    impl|trait|enum|union|unsafe|async|await|dyn|move|pub|use|extern|let|box|\
    final|override|priv|virtual|yield|become|do|macro|typeof|unsized|try)
      printf '%s_tests\n' "$name" ;;
    *)
      printf '%s\n' "$name" ;;
  esac
}

# ---------------------------------------------------------------------------
# 1. Resolve the category set: explicit args, or manifests tagged
#    `cache_sensitive = true`.
# ---------------------------------------------------------------------------
categories=()
if [[ $# -gt 0 ]]; then
  categories=("$@")
else
  for manifest in "$FIXTURES_DIR"/*/manifest.toml; do
    [[ -f "$manifest" ]] || continue
    if grep -Eq '^[[:space:]]*cache_sensitive[[:space:]]*=[[:space:]]*true' "$manifest"; then
      categories+=("$(basename "$(dirname "$manifest")")")
    fi
  done
fi

if [[ ${#categories[@]} -eq 0 ]]; then
  echo "FAIL: no cache-sensitive fixture categories found (Issue #10223)." >&2
  echo "At least one manifest entry must be tagged 'cache_sensitive = true'" >&2
  echo "(the WeakRef/GC fixtures in tests/fixtures/ref/ are the canonical set);" >&2
  echo "an empty lane would silently stop covering the #10092 bug class." >&2
  exit 1
fi

filters=()
for category in "${categories[@]}"; do
  filters+=("$(sanitize_mod_name "$category")::")
done

echo "== cache-sensitive fixture lane (Issue #10223) =="
echo "categories: ${categories[*]}"
echo "nextest filters: ${filters[*]}"
echo "cargo profile: $CARGO_PROFILE"

# ---------------------------------------------------------------------------
# 2. Guard: every filter must select at least one test, otherwise the
#    category→module mapping above drifted from build.rs and the lane would
#    silently cover nothing.
# ---------------------------------------------------------------------------
for filter in "${filters[@]}"; do
  listed="$(cargo nextest list "${profile_args[@]}" --test fixture_tests "$filter" 2>/dev/null | grep -c "$filter")"
  if [[ "$listed" -eq 0 ]]; then
    echo "FAIL: nextest filter '$filter' matches no tests (Issue #10223)." >&2
    echo "Either the category has no registered fixtures or sanitize_mod_name" >&2
    echo "in this script drifted from subset_julia_vm/build.rs." >&2
    exit 1
  fi
  echo "filter '$filter': $listed test chunk(s)"
done

run_selection() {
  timeout 1800 cargo nextest run "${profile_args[@]}" \
    --test fixture_tests --no-fail-fast "${filters[@]}"
}

# ---------------------------------------------------------------------------
# 3. Cold pass: no persistent caches, all sjulia caches disabled.
# ---------------------------------------------------------------------------
echo
echo "== [1/3] cold pass (caches removed + SUBSET_JULIA_VM_DISABLE_* set) =="
rm -f "$CARGO_TARGET_DIR"/sjulia_base_cache_*.bin
rm -f "$CARGO_TARGET_DIR"/sjulia_prelude_program_*.bin
rm -rf "${TMPDIR:-/tmp}/subset_julia_vm_cache"
(
  unset SJULIA_BASE_CACHE SJULIA_PRELUDE_PROGRAM_CACHE
  export SUBSET_JULIA_VM_DISABLE_CACHE=1
  export SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE=1
  export SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE=1
  run_selection
)
cold_status=$?
echo "cold pass exit: $cold_status"

# ---------------------------------------------------------------------------
# 4. Prime pass: default env; regenerates the persistent caches so the cached
#    pass below actually exercises the cache-RESTORE paths.
# ---------------------------------------------------------------------------
echo
echo "== [2/3] prime pass (default env; regenerates persistent caches) =="
(
  unset SJULIA_BASE_CACHE SJULIA_PRELUDE_PROGRAM_CACHE
  unset SUBSET_JULIA_VM_DISABLE_CACHE
  unset SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE
  unset SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE
  run_selection
)
prime_status=$?
echo "prime pass exit: $prime_status"

# The prime pass runs the SAME selection under the default (cache-writing)
# configuration, so it is a real coverage lane, not just cache setup: a
# regression that breaks only the default-cache path (persistent cache present
# and being written/updated) would leave both the cold and cached passes green
# and slip through if we ignored it. Fail the lane on a red prime pass.
if [[ "$prime_status" -ne 0 ]]; then
  echo "FAIL: prime pass (default cache-writing env) is red (exit $prime_status)" >&2
  echo "for the cache-sensitive categories (Issue #10223). A regression that only" >&2
  echo "breaks the default-cache path must not slip through just because the cold" >&2
  echo "and cached passes agree — fix the default-env failure." >&2
  exit "$prime_status"
fi

if ! compgen -G "$CARGO_TARGET_DIR/sjulia_base_cache_*.bin" >/dev/null; then
  echo "FAIL: prime pass produced no persistent Base cache under $CARGO_TARGET_DIR" >&2
  echo "(Issue #10223) — the cached pass would not exercise the cache-restore" >&2
  echo "mode, so the lane cannot certify cache transparency." >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# 5. Cached pass: default env with the persistent caches present — every test
#    process restores Base from the persistent cache.
# ---------------------------------------------------------------------------
echo
echo "== [3/3] cached pass (persistent caches present; restore path) =="
(
  unset SJULIA_BASE_CACHE SJULIA_PRELUDE_PROGRAM_CACHE
  unset SUBSET_JULIA_VM_DISABLE_CACHE
  unset SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE
  unset SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE
  run_selection
)
cached_status=$?
echo "cached pass exit: $cached_status"

echo
if [[ "$cold_status" -ne "$cached_status" ]]; then
  cat >&2 <<EOF
FAIL: cache-mode divergence in cache-sensitive fixture categories (Issue #10223).
  cold pass exit (caches disabled):          $cold_status
  cached pass exit (persistent cache restore): $cached_status

The same fixture selection behaves differently depending on whether the
persistent Base/prelude caches are used. This is the #10092 bug class: a
compile-context field lost or mis-rebuilt on the cache-restore path
(pipeline_ctx.rs::build_struct_tables /
cache.rs::restore_compile_context_from_program). Fix the restore path — do not
retag or untag fixtures to hide the divergence.
EOF
  exit 1
fi

if [[ "$cold_status" -ne 0 ]]; then
  echo "FAIL: cold and cached passes both failed (exit $cold_status) — the" >&2
  echo "categories are red in BOTH cache modes (not a cache-transparency bug," >&2
  echo "but the lane cannot certify a red selection)." >&2
  exit "$cold_status"
fi

echo "OK: cache-sensitive categories are green in all three passes (cold, prime, cached) and cold/cached agree."
