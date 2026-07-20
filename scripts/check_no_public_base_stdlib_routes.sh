#!/usr/bin/env bash
# check_no_public_base_stdlib_routes.sh
#
# Issue #8278: stdlib root modules must not leak through public Base.<stdlib>
# compiler routes. Real Base submodules such as Base.Iterators and
# Base.Broadcast are deliberately excluded from this denylist.
#
# Usage:
#   bash scripts/check_no_public_base_stdlib_routes.sh

set -euo pipefail

DENIED_BASE_STDLIB_MODULES="
Base64
Dates
InteractiveUtils
LinearAlgebra
Printf
Random
Statistics
Test
"

MODULE_CALL_FILE="subset_julia_vm_compile/src/compile/expr/call/module_call.rs"
BASE_FUNCTIONS_FILE="subset_julia_vm_compile/src/compile/base_functions.rs"
CORE_COMPILER_FILE="subset_julia_vm_compile/src/compile/core_compiler.rs"
PIPELINE_CTX_FILE="subset_julia_vm_compile/src/compile/pipeline_ctx.rs"

ERRORS=0

fail() {
  echo "ERROR: $*" >&2
  ERRORS=$((ERRORS + 1))
}

extract_rust_fn() {
  local file="$1"
  local signature="$2"
  awk -v sig="$signature" '
    index($0, sig) { in_fn = 1 }
    in_fn {
      print
      line = $0
      opens = gsub(/\{/, "{", line)
      line = $0
      closes = gsub(/\}/, "}", line)
      depth += opens - closes
      if (opens > 0) seen_open = 1
      if (seen_open && depth == 0) exit
    }
  ' "$file"
}

base_submodule_body="$(extract_rust_fn "$BASE_FUNCTIONS_FILE" "pub(super) fn is_base_submodule_function")"
canonical_module_path_body="$(extract_rust_fn "$CORE_COMPILER_FILE" "pub(super) fn canonical_module_path")"
resolve_using_body="$(extract_rust_fn "$PIPELINE_CTX_FILE" "fn resolve_using_module_name")"
validate_using_body="$(extract_rust_fn "$PIPELINE_CTX_FILE" "fn validate_using_import(")"

if ! printf '%s\n' "$canonical_module_path_body" | grep -q "!super::constants::is_stdlib_module(base_submodule)"; then
  fail "canonical_module_path must not canonicalize Base.<stdlib> to a stdlib root module."
fi

if ! printf '%s\n' "$resolve_using_body" | grep -q "!super::constants::is_stdlib_module(base_submodule)"; then
  fail "resolve_using_module_name must not resolve using Base.<stdlib> to a stdlib root module."
fi

if ! printf '%s\n' "$validate_using_body" | grep -q "UndefVarError: \`{base_submodule}\` not defined in \`Base\`"; then
  fail "validate_using_import must keep using Base.<stdlib> as an explicit Base UndefVarError."
fi

if ! printf '%s\n' "$validate_using_body" | grep -q "!super::constants::is_stdlib_module(base_submodule)"; then
  fail "validate_using_import must allow real Base submodules without allowing stdlib roots."
fi

while IFS= read -r module; do
  [[ -n "$module" ]] || continue

  if printf '%s\n' "$base_submodule_body" |
      grep -Eq "\"$module\"[[:space:]]*=>[[:space:]]*(true|matches!|Some|\\{)"; then
    fail "is_base_submodule_function exposes Base.$module as a public Base submodule route."
  fi

  if grep -RIn "\"Base\\.$module" subset_julia_vm_compile/src/compile \
      | grep -v "Issue #8278" >/dev/null; then
    fail "compile code contains a direct Base.$module string route; use the root stdlib or a private bridge."
    grep -RIn "\"Base\\.$module" subset_julia_vm_compile/src/compile >&2 || true
  fi

  if grep -In "submodule == \"$module\"" "$MODULE_CALL_FILE" >/dev/null; then
    fail "module-call special casing branches on Base.$module; stdlib roots must stay outside Base."
  fi
done <<< "$DENIED_BASE_STDLIB_MODULES"

if [[ "$ERRORS" -ne 0 ]]; then
  echo "FAILED: public Base.<stdlib> route audit failed (Issue #8278)." >&2
  exit 1
fi

echo "OK: no public Base.<stdlib> stdlib escape routes found (Issue #8278)."
