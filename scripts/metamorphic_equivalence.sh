#!/usr/bin/env bash
# metamorphic_equivalence.sh — metamorphic / differential equivalence-lane harness
# (Issue #10465; parent analysis #10452).
#
# Many recurrent sjulia bugs are DIVERGENCES between execution lanes that should
# be semantically identical: direct vs first-class/HOF call (#10187/#10250), Main
# vs module scope, fresh vs cache, generic vs optimized, VM vs AoT. A fixture that
# asserts ONE concrete path cannot see these. This harness expresses the invariant
#
#     normalize(run(program, lane A)) == normalize(run(transform(program), lane B))
#
# for semantics-preserving transforms, comparing result value, result type, and
# exception class across lanes of the SAME sjulia binary (this is sjulia-vs-sjulia
# differential testing, NOT sjulia-vs-upstream parity — that is the existing
# fixture parity gate).
#
# LANES IMPLEMENTED (5 of 5 in #10465):
#   direct_callable  f(x) vs Base.f(x) vs (g=f; g(x)) vs map(f,[x])[1]
#                    corpus: tests/equivalence/direct_callable.tsv
#   module_wrap      run source at Main top-level vs inside a generated unique
#                    `module ... end`; corpus: tests/equivalence/module_wrap/*.jl
#   fresh_cache      run source with all persistent caches disabled vs after
#                    priming an isolated persistent-cache target directory;
#                    corpus: tests/equivalence/fresh_cache/*.jl
#   generic_optimized
#                    run source with the legacy/generic SSA pipeline disabled
#                    vs the default optimized SSA pipeline;
#                    corpus: tests/equivalence/generic_optimized/*.jl
#   vm_aot           run documented AoT-acceptance fixtures through the VM and
#                    minimal-prelude generated AoT binary;
#                    corpus: tests/equivalence/vm_aot.tsv
#
# NORMALIZERS (documented, lane-induced noise only — never semantic values):
#   * source location suffixes `at line N:M` / `at line N` and `@ file:N`
#     (differ because each lane has different source text);
#   * the generated unique module name prefix (module_wrap lane only);
#   * `Stacktrace:` frames (the call path differs by lane, e.g. map wraps the
#     error frame; the error class/message body is kept and compared).
#
# ALLOWLISTS (Issue-linked, two-sided — cannot grow silently):
#   docs/vm/EQUIVALENCE_KNOWN_DIVERGENCES.tsv     registered lane divergences
#   docs/vm/EQUIVALENCE_MODULE_WRAP_EXCLUSIONS.tsv wrap-unsafe patterns (doc only)
# A registered divergence that later AGREES fails as STALE. A new un-registered
# divergence fails the gate; file a `bug` Issue (Discovery Rule) before registering.
#
# INTEGRATION: bounded (curated corpus, not the fixture Cartesian product).
# Guarded premerge selects it automatically for semantic-pipeline paths;
# force it for other paths with:
#   bash scripts/premerge_gate.sh --metamorphic
#
# NAMING: deliberately NOT `check_*.sh` / `audit_*.sh` — it needs a built release
# sjulia binary (no source-only sandbox self-test), so it carries its OWN negative
# self-test via `--selftest` instead of the check_audit_negative_selftest.sh
# framework (same convention as premerge_gate.sh / fixture_julia_parity.sh).
#
# Usage (from the repository root):
#   bash scripts/metamorphic_equivalence.sh                 # all lanes, curated corpus
#   bash scripts/metamorphic_equivalence.sh --lane direct_callable
#   bash scripts/metamorphic_equivalence.sh --lane module_wrap
#   bash scripts/metamorphic_equivalence.sh --lane fresh_cache
#   bash scripts/metamorphic_equivalence.sh --lane generic_optimized
#   bash scripts/metamorphic_equivalence.sh --lane vm_aot
#   bash scripts/metamorphic_equivalence.sh --list          # list cases, run nothing
#   bash scripts/metamorphic_equivalence.sh --selftest      # negative + positive self-tests
#
# Environment:
#   CARGO_TARGET_DIR Cargo output root (default <repo>/target)
#   SJULIA_BIN   path to the release sjulia (default
#                $CARGO_TARGET_DIR/release/sjulia; built automatically if missing)
#   JULIARS_BIN  path to the release AoT compiler driver (default
#                $CARGO_TARGET_DIR/release/juliars; built automatically for
#                --lane vm_aot)
#   SJULIA_METAMORPHIC_CASE_TIMEOUT  maximum seconds for each VM/AoT program
#                execution (default 120; compilation keeps its 1800s bound)
#
# Exit codes: 0 = all lanes equivalent (registered divergences ok);
#             1 = an un-registered divergence or a stale allowlist row;
#             2 = infrastructure failure (missing binary/corpus, build failed).
#
# bash 3.2 compatible (macOS stock /bin/bash); no associative arrays / mapfile.

# Intentionally NO `set -e`: lanes are EXPECTED to error (exception-class cases),
# and their non-zero exits are inspected by hand.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT" || exit 2

source "$REPO_ROOT/scripts/cargo_target_dir.sh"
cargo_target_dir="$(resolve_cargo_target_dir "$REPO_ROOT")"
export CARGO_TARGET_DIR="$cargo_target_dir"
SJULIA_BIN="${SJULIA_BIN:-$cargo_target_dir/release/sjulia}"
JULIARS_BIN="${JULIARS_BIN:-$cargo_target_dir/release/juliars}"
export SJULIA_BIN JULIARS_BIN
CORPUS_DIR="$REPO_ROOT/tests/equivalence"
DC_MANIFEST="$CORPUS_DIR/direct_callable.tsv"
MW_DIR="$CORPUS_DIR/module_wrap"
FC_DIR="$CORPUS_DIR/fresh_cache"
GO_DIR="$CORPUS_DIR/generic_optimized"
VA_MANIFEST="$CORPUS_DIR/vm_aot.tsv"
KNOWN_DIVERGENCES="$REPO_ROOT/docs/vm/EQUIVALENCE_KNOWN_DIVERGENCES.tsv"
CASE_TIMEOUT="${SJULIA_METAMORPHIC_CASE_TIMEOUT:-120}"
EXECUTION_FAILURE_MARKER="__SJULIA_METAMORPHIC_EXECUTION_FAILURE__"

RUN_DC=1
RUN_MW=1
RUN_FC=1
RUN_GO=1
RUN_VA=1
LIST_ONLY=0
SELFTEST=0

# --- counters ---------------------------------------------------------------
n_pairs=0        # non-reference lanes compared
n_agree=0        # matched the reference
n_known=0        # diverged but registered (Issue-linked)
n_fail=0         # diverged, NOT registered  -> gate failure
n_stale=0        # registered but now agrees -> gate failure
infra_fail=0

say()  { printf '[metamorphic] %s\n' "$*"; }
fail() { printf '[metamorphic] FAIL: %s\n' "$*" >&2; }

usage() { sed -n '2,60p' "$0" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
  case "$1" in
    --lane)
      shift
      [ $# -gt 0 ] || { fail "--lane requires an argument (direct_callable|module_wrap|fresh_cache|generic_optimized|vm_aot)"; exit 2; }
      case "$1" in
        direct_callable)    RUN_DC=1; RUN_MW=0; RUN_FC=0; RUN_GO=0; RUN_VA=0 ;;
        module_wrap)        RUN_DC=0; RUN_MW=1; RUN_FC=0; RUN_GO=0; RUN_VA=0 ;;
        fresh_cache)        RUN_DC=0; RUN_MW=0; RUN_FC=1; RUN_GO=0; RUN_VA=0 ;;
        generic_optimized)  RUN_DC=0; RUN_MW=0; RUN_FC=0; RUN_GO=1; RUN_VA=0 ;;
        vm_aot)             RUN_DC=0; RUN_MW=0; RUN_FC=0; RUN_GO=0; RUN_VA=1 ;;
        *) fail "unknown lane: $1 (want direct_callable|module_wrap|fresh_cache|generic_optimized|vm_aot)"; exit 2 ;;
      esac
      ;;
    --list)     LIST_ONLY=1 ;;
    --selftest) SELFTEST=1 ;;
    -h|--help)  usage; exit 0 ;;
    *) fail "unknown option: $1 (see --help)"; exit 2 ;;
  esac
  shift
done

# --- sjulia binary ----------------------------------------------------------
ensure_binary() {
  if [ -x "$SJULIA_BIN" ]; then
    return 0
  fi
  say "sjulia binary not found at $SJULIA_BIN — building it ..."
  if ! cargo build --release -p subset_julia_vm --bin sjulia --features repl; then
    fail "could not build sjulia (cargo build --release -p subset_julia_vm --bin sjulia --features repl)."
    return 1
  fi
  [ -x "$SJULIA_BIN" ] || { fail "build finished but $SJULIA_BIN is still not executable."; return 1; }
}

ensure_aot_binary() {
  if [ -x "$JULIARS_BIN" ]; then
    return 0
  fi
  say "juliars binary not found at $JULIARS_BIN — building it ..."
  if ! cargo build --release -p subset_julia_vm --features aot --bin juliars; then
    fail "could not build juliars (cargo build --release -p subset_julia_vm --features aot --bin juliars)."
    return 1
  fi
  [ -x "$JULIARS_BIN" ] || { fail "build finished but $JULIARS_BIN is still not executable."; return 1; }
}

TMP_DIR=""
# shellcheck disable=SC2329  # invoked indirectly via the EXIT trap
cleanup() { [ -n "$TMP_DIR" ] && rm -rf "$TMP_DIR" 2>/dev/null; return 0; }
trap cleanup EXIT

# normalize <module-token> — filter stdin into a canonical observation.
# <module-token> is the generated module name to strip (module_wrap lane), or
# empty for lanes with no module. Only lane-induced noise is removed.
normalize() {
  local modtok="$1"
  # 1. drop stacktrace frames (call-path dependent), keep the primary error line.
  #    A frame is `Stacktrace:` or an INDENTED ` [N] func(...)` line — the leading
  #    whitespace is required so a bare Julia vector literal printed at column 0
  #    (e.g. `[1]`, a real observation value) is NOT mistaken for a frame and
  #    dropped (that would hide a `[1]`-vs-`[2]` divergence).
  # 2. strip source-location suffixes/prefixes (each lane has different source).
  grep -vE '^[[:space:]]*Stacktrace:|^[[:space:]]+\[[0-9]+\][[:space:]]' \
    | sed -E \
        -e 's/ at line [0-9]+:[0-9]+//g' \
        -e 's/ at line [0-9]+//g' \
        -e 's/@ [^ ]+:[0-9]+//g' \
    | {
        if [ -n "$modtok" ]; then
          sed -E "s/(Main\.)?${modtok}\.//g"
        else
          cat
        fi
      } \
    | sed -E 's/[[:space:]]+$//'
}

# run_observed <module-token> <command...> — run one bounded lane and return its
# normalized combined observation. An execution-failure marker makes the
# comparator fail closed even when both lanes time out or terminate alike.
run_observed() {
  local modtok="$1" raw rc
  shift
  raw="$(timeout "$CASE_TIMEOUT" "$@" 2>&1)"
  rc=$?
  printf '%s\n' "$raw" | normalize "$modtok"
  if [ "$rc" -eq 124 ] || [ "$rc" -eq 137 ]; then
    printf '%s reason=timeout command=%s seconds=%s\n' "$EXECUTION_FAILURE_MARKER" "$1" "$CASE_TIMEOUT"
  elif [ "$rc" -gt 128 ]; then
    printf '%s reason=signal command=%s exit=%s\n' "$EXECUTION_FAILURE_MARKER" "$1" "$rc"
  fi
}

run_source() {
  local src="$1" modtok="$2"
  run_observed "$modtok" "$SJULIA_BIN" "$src"
}

# --- direct_callable lane ---------------------------------------------------
# emit_lane_program <lane> <preamble> <call> <arg> -> Julia program on stdout
emit_lane_program() {
  local lane="$1" pre="$2" call="$3" arg="$4"
  [ "$pre" = "-" ] && pre=""
  [ -n "$pre" ] && printf '%s\n' "$pre"
  case "$lane" in
    direct) printf '__mm_v = (%s)(%s)\n' "$call" "$arg" ;;
    base)   printf '__mm_v = Base.%s(%s)\n' "$call" "$arg" ;;
    bind)   printf '__mm_g = (%s)\n__mm_v = __mm_g(%s)\n' "$call" "$arg" ;;
    hof)    printf '__mm_v = map((%s), [%s])[1]\n' "$call" "$arg" ;;
  esac
  printf 'println(__mm_v)\nprintln(typeof(__mm_v))\n'
}

# lookup_known <lane_group> <case> <lane> -> prints "issue<TAB>reason" or empty
lookup_known() {
  local group="$1" case="$2" lane="$3"
  [ -f "$KNOWN_DIVERGENCES" ] || return 0
  awk -F'\t' -v g="$group" -v c="$case" -v l="$lane" '
    /^[[:space:]]*#/ { next }
    NF < 5 { next }
    $1 == g && $2 == c && $3 == l { print $4 "\t" $5; exit }
  ' "$KNOWN_DIVERGENCES"
}

# compare_pair reference-obs candidate-obs -> 0 equal, 1 differ
obs_equal() {
  case "$1\n$2" in
    *"$EXECUTION_FAILURE_MARKER"*)
      fail "lane execution timed out or was terminated; failed executions are never equivalent"
      infra_fail=1
      return 1
      ;;
  esac
  [ "$1" = "$2" ]
}

run_direct_callable() {
  [ -f "$DC_MANIFEST" ] || { fail "direct_callable corpus missing: $DC_MANIFEST"; infra_fail=1; return; }
  say "== lane: direct_callable ($DC_MANIFEST) =="
  local name pre call arg lanes
  while IFS=$'\t' read -r name pre call arg lanes; do
    case "$name" in ''|'#'*) continue ;; esac
    [ "$name" = "name" ] && continue
    if [ "$LIST_ONLY" -eq 1 ]; then
      printf '  direct_callable :: %-22s lanes=%s\n' "$name" "$lanes"
      continue
    fi

    # split lanes on commas (bash 3.2)
    local lane_list ref_lane ref_obs cand_obs lane known issue reason
    lane_list="$(printf '%s' "$lanes" | tr ',' ' ')"
    ref_lane=""
    for lane in $lane_list; do
      local src="$TMP_DIR/dc_${name}_${lane}.jl"
      emit_lane_program "$lane" "$pre" "$call" "$arg" > "$src"
      local obs; obs="$(run_source "$src" '')"
      if [ -z "$ref_lane" ]; then
        ref_lane="$lane"; ref_obs="$obs"
        continue
      fi
      n_pairs=$((n_pairs + 1))
      cand_obs="$obs"
      known="$(lookup_known direct_callable "$name" "$lane")"
      issue=""; reason=""
      if [ -n "$known" ]; then
        issue="$(printf '%s' "$known" | cut -f1)"
        reason="$(printf '%s' "$known" | cut -f2)"
      fi
      if obs_equal "$ref_obs" "$cand_obs"; then
        if [ -n "$known" ]; then
          n_stale=$((n_stale + 1))
          fail "STALE allowlist: direct_callable/$name lane '$lane' now AGREES with '$ref_lane' (Issue #$issue). Remove the row from $(basename "$KNOWN_DIVERGENCES") and close the Issue."
        else
          n_agree=$((n_agree + 1))
        fi
      else
        if [ -n "$known" ]; then
          n_known=$((n_known + 1))
          say "  KNOWN divergence direct_callable/$name: '$lane' != '$ref_lane' (Issue #$issue: $reason)"
        else
          n_fail=$((n_fail + 1))
          fail "divergence direct_callable/$name: lane '$lane' != reference '$ref_lane'"
          printf '    [%s] %s\n' "$ref_lane" "$(printf '%s' "$ref_obs" | tr '\n' '|')" >&2
          printf '    [%s] %s\n' "$lane" "$(printf '%s' "$cand_obs" | tr '\n' '|')" >&2
          fail "    if this is a genuine regression, file a bug Issue (MWE + julia-vs-sjulia table) then register it in $(basename "$KNOWN_DIVERGENCES")."
        fi
      fi
    done
  done < "$DC_MANIFEST"
}

# --- module_wrap lane -------------------------------------------------------
# sanitize_module_name <case> -> a valid unique Julia module identifier
sanitize_module_name() {
  local base="$1"
  base="$(printf '%s' "$base" | tr -c 'A-Za-z0-9' '_')"
  printf 'MetamorphWrap_%s_%s' "$base" "$$"
}

run_module_wrap() {
  [ -d "$MW_DIR" ] || { fail "module_wrap corpus missing: $MW_DIR"; infra_fail=1; return; }
  say "== lane: module_wrap ($MW_DIR) =="
  local f found=0
  for f in "$MW_DIR"/*.jl; do
    [ -f "$f" ] || continue
    found=1
    local case; case="$(basename "$f" .jl)"
    if [ "$LIST_ONLY" -eq 1 ]; then
      printf '  module_wrap     :: %s\n' "$case"
      continue
    fi
    local modtok; modtok="$(sanitize_module_name "$case")"

    # main lane: run the source verbatim.
    local main_obs; main_obs="$(run_source "$f" '')"

    # module lane: wrap the whole body in a generated unique module.
    local wrapped="$TMP_DIR/mw_${case}_module.jl"
    {
      printf 'module %s\n' "$modtok"
      cat "$f"
      printf '\nend\n'
    } > "$wrapped"
    local mod_obs; mod_obs="$(run_source "$wrapped" "$modtok")"

    n_pairs=$((n_pairs + 1))
    local known issue reason
    known="$(lookup_known module_wrap "$case" module)"
    issue=""; reason=""
    if [ -n "$known" ]; then
      issue="$(printf '%s' "$known" | cut -f1)"
      reason="$(printf '%s' "$known" | cut -f2)"
    fi
    if obs_equal "$main_obs" "$mod_obs"; then
      if [ -n "$known" ]; then
        n_stale=$((n_stale + 1))
        fail "STALE allowlist: module_wrap/$case now AGREES (Issue #$issue). Remove the row from $(basename "$KNOWN_DIVERGENCES") and close the Issue."
      else
        n_agree=$((n_agree + 1))
      fi
    else
      if [ -n "$known" ]; then
        n_known=$((n_known + 1))
        say "  KNOWN divergence module_wrap/$case (Issue #$issue: $reason)"
      else
        n_fail=$((n_fail + 1))
        fail "divergence module_wrap/$case: 'main' != 'module'"
        printf '    [main]   %s\n' "$(printf '%s' "$main_obs" | tr '\n' '|')" >&2
        printf '    [module] %s\n' "$(printf '%s' "$mod_obs" | tr '\n' '|')" >&2
        fail "    if the fixture is not wrap-safe, remove it from $MW_DIR and document the pattern in docs/vm/EQUIVALENCE_MODULE_WRAP_EXCLUSIONS.tsv; if it is a real bug, file it."
      fi
    fi
  done
  [ "$found" -eq 1 ] || { fail "no *.jl cases in $MW_DIR — the module_wrap lane would silently cover nothing."; infra_fail=1; }
}

# --- fresh_cache lane -------------------------------------------------------
# Run each case once with persistent caches disabled (fresh lane), then prime and
# re-run it with an isolated persistent Base/prelude/cache directory (cached
# lane). This is a semantic observation comparison, not a timing comparison.
run_fresh_cache() {
  [ -d "$FC_DIR" ] || { fail "fresh_cache corpus missing: $FC_DIR"; infra_fail=1; return; }
  say "== lane: fresh_cache ($FC_DIR) =="
  local f found=0
  for f in "$FC_DIR"/*.jl; do
    [ -f "$f" ] || continue
    found=1
    local case; case="$(basename "$f" .jl)"
    if [ "$LIST_ONLY" -eq 1 ]; then
      printf '  fresh_cache     :: %s\n' "$case"
      continue
    fi

    local cold_target cached_target pkg_cache
    cold_target="$TMP_DIR/fc_${case}_cold_target"
    cached_target="$TMP_DIR/fc_${case}_cached_target"
    pkg_cache="$TMP_DIR/fc_${case}_pkg_cache"
    mkdir -p "$cold_target" "$cached_target" "$pkg_cache"

    local cold_obs
    cold_obs="$(run_observed '' \
      env \
        CARGO_TARGET_DIR="$cold_target" \
        SUBSETJULIA_CACHE_DIR="$pkg_cache/cold" \
        SUBSET_JULIA_VM_DISABLE_CACHE=1 \
        SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE=1 \
        SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE=1 \
        SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELOAD_CACHE=1 \
        "$SJULIA_BIN" "$f")"

    # Prime the isolated persistent target directory, then observe the restored
    # cache lane from a second process.
    if ! timeout "$CASE_TIMEOUT" env \
      CARGO_TARGET_DIR="$cached_target" \
      SUBSETJULIA_CACHE_DIR="$pkg_cache/cached" \
      "$SJULIA_BIN" "$f" >/dev/null 2>&1; then
      fail "fresh_cache/$case: priming run failed before cached-lane observation"
      infra_fail=1
      continue
    fi
    local cached_obs
    cached_obs="$(run_observed '' \
      env \
        CARGO_TARGET_DIR="$cached_target" \
        SUBSETJULIA_CACHE_DIR="$pkg_cache/cached" \
        "$SJULIA_BIN" "$f")"

    n_pairs=$((n_pairs + 1))
    local known issue reason
    known="$(lookup_known fresh_cache "$case" cached)"
    issue=""; reason=""
    if [ -n "$known" ]; then
      issue="$(printf '%s' "$known" | cut -f1)"
      reason="$(printf '%s' "$known" | cut -f2)"
    fi
    if obs_equal "$cold_obs" "$cached_obs"; then
      if [ -n "$known" ]; then
        n_stale=$((n_stale + 1))
        fail "STALE allowlist: fresh_cache/$case cached lane now AGREES with fresh (Issue #$issue). Remove the row from $(basename "$KNOWN_DIVERGENCES") and close the Issue."
      else
        n_agree=$((n_agree + 1))
      fi
    else
      if [ -n "$known" ]; then
        n_known=$((n_known + 1))
        say "  KNOWN divergence fresh_cache/$case (Issue #$issue: $reason)"
      else
        n_fail=$((n_fail + 1))
        fail "divergence fresh_cache/$case: 'cached' != reference 'fresh'"
        printf '    [fresh]  %s\n' "$(printf '%s' "$cold_obs" | tr '\n' '|')" >&2
        printf '    [cached] %s\n' "$(printf '%s' "$cached_obs" | tr '\n' '|')" >&2
        fail "    if this is a genuine cache-mode regression, file a bug Issue (MWE + julia-vs-sjulia table) then register it in $(basename "$KNOWN_DIVERGENCES")."
      fi
    fi
  done
  [ "$found" -eq 1 ] || { fail "no *.jl cases in $FC_DIR — the fresh_cache lane would silently cover nothing."; infra_fail=1; }
}

# --- generic_optimized lane -------------------------------------------------
# Compare the legacy/generic SSA path (`SJULIA_SSA_PIPELINE=0`) with the default
# optimized SSA pipeline. This catches optimizer-induced semantic drift while
# staying inside one sjulia binary.
run_generic_optimized() {
  [ -d "$GO_DIR" ] || { fail "generic_optimized corpus missing: $GO_DIR"; infra_fail=1; return; }
  say "== lane: generic_optimized ($GO_DIR) =="
  local f found=0
  for f in "$GO_DIR"/*.jl; do
    [ -f "$f" ] || continue
    found=1
    local case; case="$(basename "$f" .jl)"
    if [ "$LIST_ONLY" -eq 1 ]; then
      printf '  generic_optimized :: %s\n' "$case"
      continue
    fi

    local generic_obs optimized_obs
    generic_obs="$(run_observed '' env SJULIA_SSA_PIPELINE=0 "$SJULIA_BIN" "$f")"
    optimized_obs="$(run_source "$f" '')"

    n_pairs=$((n_pairs + 1))
    local known issue reason
    known="$(lookup_known generic_optimized "$case" optimized)"
    issue=""; reason=""
    if [ -n "$known" ]; then
      issue="$(printf '%s' "$known" | cut -f1)"
      reason="$(printf '%s' "$known" | cut -f2)"
    fi
    if obs_equal "$generic_obs" "$optimized_obs"; then
      if [ -n "$known" ]; then
        n_stale=$((n_stale + 1))
        fail "STALE allowlist: generic_optimized/$case optimized lane now AGREES with generic (Issue #$issue). Remove the row from $(basename "$KNOWN_DIVERGENCES") and close the Issue."
      else
        n_agree=$((n_agree + 1))
      fi
    else
      if [ -n "$known" ]; then
        n_known=$((n_known + 1))
        say "  KNOWN divergence generic_optimized/$case (Issue #$issue: $reason)"
      else
        n_fail=$((n_fail + 1))
        fail "divergence generic_optimized/$case: 'optimized' != reference 'generic'"
        printf '    [generic]   %s\n' "$(printf '%s' "$generic_obs" | tr '\n' '|')" >&2
        printf '    [optimized] %s\n' "$(printf '%s' "$optimized_obs" | tr '\n' '|')" >&2
        fail "    if this is a genuine optimization-pipeline regression, file a bug Issue (MWE + julia-vs-sjulia table) then register it in $(basename "$KNOWN_DIVERGENCES")."
      fi
    fi
  done
  [ "$found" -eq 1 ] || { fail "no *.jl cases in $GO_DIR — the generic_optimized lane would silently cover nothing."; infra_fail=1; }
}

# --- vm_aot lane ------------------------------------------------------------
# Compare sjulia VM stdout/stderr with a minimal-prelude generated AoT binary
# for the documented AoT acceptance scope. The corpus is a TSV manifest so it
# can point at the canonical acceptance fixtures without copying their source
# bodies. Full-Base AoT remains wider than the acceptance scope and currently
# gates on unsupported BigInt constructor lowering (Issue #6975).
run_vm_aot() {
  [ -f "$VA_MANIFEST" ] || { fail "vm_aot corpus missing: $VA_MANIFEST"; infra_fail=1; return; }
  if [ "$LIST_ONLY" -ne 1 ]; then
    ensure_aot_binary || { infra_fail=1; return; }
  fi
  say "== lane: vm_aot ($VA_MANIFEST) =="
  local name rel_fixture found=0
  while IFS=$'\t' read -r name rel_fixture; do
    case "$name" in ''|'#'*) continue ;; esac
    [ "$name" = "name" ] && continue
    found=1
    if [ "$LIST_ONLY" -eq 1 ]; then
      printf '  vm_aot          :: %-22s fixture=%s\n' "$name" "$rel_fixture"
      continue
    fi

    local fixture="$REPO_ROOT/$rel_fixture"
    if [ ! -f "$fixture" ]; then
      fail "vm_aot/$name fixture missing: $rel_fixture"
      infra_fail=1
      continue
    fi

    local vm_obs aot_obs generated_rs aot_bin compile_out
    generated_rs="$TMP_DIR/va_${name}.rs"
    aot_bin="$TMP_DIR/va_${name}_bin"
    compile_out="$TMP_DIR/va_${name}_juliars.out"

    if ! timeout 1800 "$JULIARS_BIN" "$fixture" --minimal-prelude -o "$generated_rs" --emit-binary "$aot_bin" >"$compile_out" 2>&1; then
      fail "vm_aot/$name: juliars failed"
      sed -n '1,80p' "$compile_out" >&2
      infra_fail=1
      continue
    fi

    vm_obs="$(run_source "$fixture" '')"
    aot_obs="$(run_observed '' "$aot_bin")"

    n_pairs=$((n_pairs + 1))
    local known issue reason
    known="$(lookup_known vm_aot "$name" aot)"
    issue=""; reason=""
    if [ -n "$known" ]; then
      issue="$(printf '%s' "$known" | cut -f1)"
      reason="$(printf '%s' "$known" | cut -f2)"
    fi
    if obs_equal "$vm_obs" "$aot_obs"; then
      if [ -n "$known" ]; then
        n_stale=$((n_stale + 1))
        fail "STALE allowlist: vm_aot/$name AoT lane now AGREES with VM (Issue #$issue). Remove the row from $(basename "$KNOWN_DIVERGENCES") and close the Issue."
      else
        n_agree=$((n_agree + 1))
      fi
    else
      if [ -n "$known" ]; then
        n_known=$((n_known + 1))
        say "  KNOWN divergence vm_aot/$name (Issue #$issue: $reason)"
      else
        n_fail=$((n_fail + 1))
        fail "divergence vm_aot/$name: 'aot' != reference 'vm'"
        printf '    [vm]  %s\n' "$(printf '%s' "$vm_obs" | tr '\n' '|')" >&2
        printf '    [aot] %s\n' "$(printf '%s' "$aot_obs" | tr '\n' '|')" >&2
        fail "    if this is a genuine VM/AoT semantic regression, file a bug Issue (MWE + julia-vs-sjulia table) then register it in $(basename "$KNOWN_DIVERGENCES")."
      fi
    fi
  done < "$VA_MANIFEST"
  [ "$found" -eq 1 ] || { fail "no cases in $VA_MANIFEST — the vm_aot lane would silently cover nothing."; infra_fail=1; }
}

# --- negative self-test -----------------------------------------------------
# Proves each comparator (value / type / exception) fires on a SEEDED divergence,
# and that a positive control (identical lanes) does NOT. Independent of the
# corpus and allowlist: it drives normalize()/obs_equal directly.
selftest() {
  say "== negative self-test (seeded divergences) =="
  local st_fail=0

  # helper: run two inline programs, echo "DIVERGE" or "AGREE"
  st_cmp() {
    local a="$1" b="$2"
    local fa="$TMP_DIR/st_a.jl" fb="$TMP_DIR/st_b.jl"
    printf '%s\n' "$a" > "$fa"
    printf '%s\n' "$b" > "$fb"
    local oa ob; oa="$(run_source "$fa" '')"; ob="$(run_source "$fb" '')"
    if obs_equal "$oa" "$ob"; then printf 'AGREE'; else printf 'DIVERGE'; fi
  }

  local r

  # 1. VALUE divergence: 5 vs 6.
  r="$(st_cmp 'x = 5
println(x)
println(typeof(x))' 'x = 5 + 1
println(x)
println(typeof(x))')"
  if [ "$r" = "DIVERGE" ]; then say "  PASS value comparator caught 5 vs 6"; else fail "  value comparator did NOT catch 5 vs 6"; st_fail=1; fi

  # 2. TYPE divergence: same printed value (1), different type (Int8 vs Int64).
  r="$(st_cmp 'x = Int8(1)
println(x)
println(typeof(x))' 'x = 1
println(x)
println(typeof(x))')"
  if [ "$r" = "DIVERGE" ]; then say "  PASS type comparator caught Int8(1) vs 1"; else fail "  type comparator did NOT catch Int8 vs Int64 (value prints identically)"; st_fail=1; fi

  # 3. EXCEPTION divergence: value vs raised error.
  r="$(st_cmp 'x = sqrt(4)
println(x)
println(typeof(x))' 'x = sqrt(-1)
println(x)
println(typeof(x))')"
  if [ "$r" = "DIVERGE" ]; then say "  PASS exception comparator caught value vs DomainError"; else fail "  exception comparator did NOT catch value vs raised error"; st_fail=1; fi

  # 4. POSITIVE control: identical programs must AGREE (no false positive).
  r="$(st_cmp 'x = 40 + 2
println(x)
println(typeof(x))' 'x = 42
println(x)
println(typeof(x))')"
  if [ "$r" = "AGREE" ]; then say "  PASS positive control (identical observations agree)"; else fail "  positive control FALSELY reported a divergence"; st_fail=1; fi

  # 5. NORMALIZER control: same value/type, different source line count -> the
  #    `at line N:M` and stacktrace noise must be normalized so an ERROR still
  #    compares equal across differing source. sqrt(-1) at line 1 vs line 3.
  r="$(st_cmp 'x = sqrt(-1)
println(x)' '# pad
# pad
x = sqrt(-1)
println(x)')"
  if [ "$r" = "AGREE" ]; then say "  PASS normalizer folds source-location noise for equal errors"; else fail "  normalizer left source-location noise -> equal errors reported as divergent"; st_fail=1; fi

  # 6. OVER-NORMALIZATION guard: a bare single-element vector `[1]` vs `[2]` must
  #    still DIVERGE — the stacktrace-frame filter must not eat a column-0 vector
  #    literal that happens to look like a frame index.
  r="$(st_cmp 'println([1])
println(typeof([1]))' 'println([2])
println(typeof([2]))')"
  if [ "$r" = "DIVERGE" ]; then say "  PASS value comparator caught [1] vs [2] (frame filter not over-eager)"; else fail "  frame filter over-normalized: [1] vs [2] was NOT caught (a printed vector value was dropped)"; st_fail=1; fi

  # 7. TIMEOUT guard: a hung lane must produce the fail-closed marker instead
  #    of blocking guarded premerge indefinitely or comparing two hangs equal.
  local old_timeout timed_obs
  old_timeout="$CASE_TIMEOUT"
  CASE_TIMEOUT=1
  timed_obs="$(run_observed '' sh -c 'sleep 2')"
  CASE_TIMEOUT="$old_timeout"
  case "$timed_obs" in
    *"$EXECUTION_FAILURE_MARKER"*reason=timeout*) say "  PASS timeout guard bounded a hung lane" ;;
    *) fail "  timeout guard did not mark a hung lane"; st_fail=1 ;;
  esac

  if [ "$st_fail" -eq 0 ]; then
    say "self-test: all comparators fire on seeded divergences and the controls are clean."
    return 0
  fi
  fail "self-test FAILED — the harness is not reliably detecting divergences."
  return 1
}

# --- main -------------------------------------------------------------------
if [ "$LIST_ONLY" -eq 1 ]; then
  say "curated corpus cases:"
  [ "$RUN_DC" -eq 1 ] && run_direct_callable
  [ "$RUN_MW" -eq 1 ] && run_module_wrap
  [ "$RUN_FC" -eq 1 ] && run_fresh_cache
  [ "$RUN_GO" -eq 1 ] && run_generic_optimized
  [ "$RUN_VA" -eq 1 ] && run_vm_aot
  exit 0
fi

ensure_binary || exit 2
TMP_DIR="$(mktemp -d)"

if [ "$SELFTEST" -eq 1 ]; then
  selftest; exit $?
fi

[ "$RUN_DC" -eq 1 ] && run_direct_callable
[ "$RUN_MW" -eq 1 ] && run_module_wrap
[ "$RUN_FC" -eq 1 ] && run_fresh_cache
[ "$RUN_GO" -eq 1 ] && run_generic_optimized
[ "$RUN_VA" -eq 1 ] && run_vm_aot

echo
say "summary: $n_pairs lane-pairs compared | $n_agree agree | $n_known known-divergence(registered) | $n_fail unregistered-divergence | $n_stale stale-allowlist"

if [ "$infra_fail" -ne 0 ]; then
  fail "infrastructure failure (missing corpus/binary) — see messages above."
  exit 2
fi
if [ "$n_fail" -ne 0 ] || [ "$n_stale" -ne 0 ]; then
  fail "$n_fail unregistered divergence(s), $n_stale stale allowlist row(s). Gate RED."
  exit 1
fi
say "OK: all lanes equivalent (registered divergences tracked with linked Issues)."
exit 0
