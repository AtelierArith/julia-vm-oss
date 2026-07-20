#!/usr/bin/env bash
# check_type_application_matrix.sh — type-application opcode/route coverage audit
# (Issue #10556).
#
# Prevents the static-vs-dynamic type-application drift class behind
# #10554 / #10587 / #10586 / #10422: static `ConstructParametricType*` and dynamic
# `ApplyTypeDynamic*` construction diverged silently because no single fixture
# exercised every type-application opcode/route through both success and negative
# (bound / arity / non-type-base / concrete-base) cases.
#
# This audit fails when a type-application opcode in the `Instr` enum is NOT
# represented in the parity matrix, so a newly added opcode/route cannot ship
# without a matrix case that pins its behavior against upstream Julia.
#
# Two mandatory source-only checks (no build required, so this runs in the
# pr-fast `scripts/check_*.sh` batch and in the audit negative self-test sandbox):
#   1. Every type-application opcode discovered in
#      subset_julia_vm_bytecode/src/instr.rs is declared `opcode-covered:` in the
#      matrix fixture, and every declared opcode still exists in the enum.
# One best-effort check when a built sjulia binary is available (locally / in the
# fixture-tests CI job):
#   2. Each declared opcode is actually EMITTED by the fixture's compiled user
#      code (via --dump-bytecode), so a declaration cannot claim coverage the
#      fixture does not exercise.
# The fixture asserts only cases where sjulia already matches upstream (so it
# stays upstream-parity-comparable); currently-diverging cases are documented in
# docs/vm/TYPE_APPLICATION_MATRIX_SKIPLIST.tsv, whose format this audit validates.
#
# Usage: bash scripts/check_type_application_matrix.sh
# Exit: 0 = every type-application opcode is represented (and, when a binary is
#           present, emitted) by the matrix; 1 otherwise.

set -euo pipefail
cd "$(dirname "$0")/.."

INSTR="${INSTR:-subset_julia_vm_bytecode/src/instr.rs}"
FIXTURE="${FIXTURE:-subset_julia_vm/tests/fixtures/types/type_application_matrix_10556.jl}"
MANIFEST="${MANIFEST:-subset_julia_vm/tests/fixtures/types/manifest.toml}"
SKIPLIST="${SKIPLIST:-docs/vm/TYPE_APPLICATION_MATRIX_SKIPLIST.tsv}"

fail=0

for path in "$INSTR" "$FIXTURE"; do
  if [ ! -f "$path" ]; then
    echo "ERROR: required file not found: $path (Issue #10556)." >&2
    exit 1
  fi
done

# --- discover type-application opcodes from the Instr enum body ---------------
# Variant names inside `pub enum Instr { ... }` whose identifier contains
# `ApplyType` or `ParametricType`, plus `PushDataType`. A new opcode following
# this family's naming is discovered automatically.
instr_opcodes="$(
  awk '/^pub enum Instr \{/{f=1;next} f&&/^\}/{f=0} f&&/^    [A-Za-z_]/{print}' "$INSTR" \
    | sed -E 's/^[[:space:]]+//; s/[^A-Za-z0-9_].*$//' \
    | grep -E 'ApplyType|ParametricType|^PushDataType$' \
    | sort -u
)"

if [ -z "$instr_opcodes" ]; then
  echo "ERROR: discovered zero type-application opcodes in $INSTR — the enum shape" >&2
  echo "       or discovery pattern changed; the audit can no longer guard its" >&2
  echo "       invariant (Issue #10556)." >&2
  exit 1
fi

# --- declared coverage from the matrix fixture -------------------------------
declared_opcodes="$(
  grep -oE 'opcode-covered:[[:space:]]*[A-Za-z0-9_]+' "$FIXTURE" \
    | sed -E 's/opcode-covered:[[:space:]]*//' \
    | sort -u
)"

# opcodes in the enum but not declared by the matrix -> unrepresented route
missing="$(comm -23 <(printf '%s\n' "$instr_opcodes") <(printf '%s\n' "$declared_opcodes"))"
if [ -n "$missing" ]; then
  echo "ERROR: type-application opcode(s) present in $INSTR but not represented in the matrix (Issue #10556):" >&2
  printf '%s\n' "$missing" | sed 's/^/  /' >&2
  echo "Add a case to $FIXTURE that exercises the opcode against upstream julia," >&2
  echo "then declare it with an 'opcode-covered:' line in that fixture's header." >&2
  fail=1
fi

# declared opcodes that no longer exist in the enum -> stale coverage claim
stale="$(comm -13 <(printf '%s\n' "$instr_opcodes") <(printf '%s\n' "$declared_opcodes"))"
if [ -n "$stale" ]; then
  echo "ERROR: matrix declares opcode-covered for name(s) absent from $INSTR (Issue #10556):" >&2
  printf '%s\n' "$stale" | sed 's/^/  /' >&2
  echo "Remove or rename the stale 'opcode-covered:' declaration in $FIXTURE." >&2
  fail=1
fi

# --- fixture hygiene (best-effort; skipped if manifest absent, e.g. sandbox) --
if [ -f "$MANIFEST" ]; then
  fixture_base="$(basename "$FIXTURE")"
  if ! grep -qF "$fixture_base" "$MANIFEST"; then
    echo "ERROR: $fixture_base is not registered in $MANIFEST (Issue #10556)." >&2
    fail=1
  fi
fi
if [ "$(tail -n 1 "$FIXTURE" | tr -d '[:space:]')" != "true" ]; then
  echo "ERROR: $FIXTURE must end with a bare 'true' line (fixture convention, Issue #10556)." >&2
  fail=1
fi

# --- known-drift skiplist hygiene (best-effort; skipped if file absent) -------
# Each documented divergence row must carry a numeric tracking issue and a unique
# id, so a diverging case cannot be parked without a live issue to unpark it.
if [ -f "$SKIPLIST" ]; then
  EXPECTED_SKIPLIST_HEADER=$'id\tissue\troute\texpr\texpected_julia\treason'
  skip_header="$(sed -n '1p' "$SKIPLIST")"
  if [ "$skip_header" != "$EXPECTED_SKIPLIST_HEADER" ]; then
    echo "ERROR: $SKIPLIST has an unexpected header (Issue #10556)." >&2
    echo "  expected: id<TAB>issue<TAB>route<TAB>expr<TAB>expected_julia<TAB>reason" >&2
    fail=1
  fi
  if ! awk -F '\t' '
    NR == 1 { next }
    /^[[:space:]]*$/ { next }
    NF < 6 {
      printf "ERROR: malformed skiplist row %d: expected 6 tab-separated columns (Issue #10556)\n", NR > "/dev/stderr"
      errors += 1; next
    }
    $2 !~ /^[0-9]+$/ {
      printf "ERROR: skiplist id %s has a non-numeric issue %s (Issue #10556)\n", $1, $2 > "/dev/stderr"
      errors += 1
    }
    seen[$1]++ {
      printf "ERROR: duplicate skiplist id %s (Issue #10556)\n", $1 > "/dev/stderr"
      errors += 1
    }
    END { exit errors ? 1 : 0 }
  ' "$SKIPLIST"; then
    fail=1
  fi
fi

# --- best-effort emission check (needs a built sjulia binary) ----------------
SJULIA_BIN="${SJULIA_BIN:-}"
if [ -z "$SJULIA_BIN" ]; then
  for cand in ./target/release/sjulia ./target/release-fast/sjulia ./target/dev-fast/sjulia; do
    if [ -x "$cand" ]; then SJULIA_BIN="$cand"; break; fi
  done
fi

if [ -n "$SJULIA_BIN" ] && [ -x "$SJULIA_BIN" ]; then
  emitted="$(
    "$SJULIA_BIN" --dump-bytecode "$FIXTURE" 2>/dev/null \
      | grep -oE 'ApplyTypeDynamicSplat|ApplyTypeDynamic|ConstructParametricTypeSplat|ConstructParametricType|PushDataType' \
      | sort -u
  )"
  not_emitted="$(comm -23 <(printf '%s\n' "$declared_opcodes") <(printf '%s\n' "$emitted"))"
  if [ -n "$not_emitted" ]; then
    echo "ERROR: opcode(s) declared 'opcode-covered:' but NOT emitted by the compiled matrix fixture (Issue #10556):" >&2
    printf '%s\n' "$not_emitted" | sed 's/^/  /' >&2
    echo "The declaration claims coverage the fixture does not exercise; add a case that emits it." >&2
    fail=1
  else
    echo "OK: emission check — all declared opcodes are emitted by the compiled fixture ($SJULIA_BIN)."
  fi
else
  echo "NOTE: no sjulia binary found; skipping the emission check (static coverage cross-check still enforced)."
fi

if [ "$fail" -ne 0 ]; then
  echo "FAIL: type-application matrix coverage audit failed (Issue #10556)." >&2
  exit 1
fi

opcode_count="$(printf '%s\n' "$instr_opcodes" | grep -c .)"
echo "OK: type-application matrix represents all $opcode_count type-application opcode(s) (Issue #10556)."
