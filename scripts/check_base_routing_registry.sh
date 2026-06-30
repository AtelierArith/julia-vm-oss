#!/usr/bin/env bash
# check_base_routing_registry.sh
#
# Ensure public Base builtin fallback routing stays classified in the central
# registry instead of drifting back into ad hoc string match arms.
#
# Usage:
#   bash scripts/check_base_routing_registry.sh

set -euo pipefail

file="subset_julia_vm/src/compile/base_functions.rs"

if ! grep -q "BASE_FUNCTION_ROUTES" "$file"; then
  echo "ERROR: BASE_FUNCTION_ROUTES registry not found in $file" >&2
  exit 1
fi

if grep -Eq 'route\([^,]+,[^,]+,[^,]+,[[:space:]]*""[[:space:]]*\)' "$file"; then
  echo "ERROR: BASE_FUNCTION_ROUTES contains an empty upstream_ref" >&2
  exit 1
fi

base_fn_body="$(awk '
  /pub\(super\) fn base_function_to_builtin_op/ { in_fn = 1 }
  in_fn { print }
  in_fn && /^}/ { exit }
' "$file")"

if ! printf '%s\n' "$base_fn_body" | grep -q "base_function_route(name)"; then
  echo "ERROR: base_function_to_builtin_op must route through BASE_FUNCTION_ROUTES" >&2
  exit 1
fi

if printf '%s\n' "$base_fn_body" | grep -Eq '"[^"]+"[[:space:]]*=>[[:space:]]*Some\(BuiltinOp::'; then
  echo "ERROR: direct string arms in base_function_to_builtin_op bypass the routing registry" >&2
  exit 1
fi

dispatch_first_body="$(awk '
  /pub\(super\) fn is_method_dispatch_first_base_function/ { in_fn = 1 }
  in_fn { print }
  in_fn && /^}/ { exit }
' "$file")"

if ! printf '%s\n' "$dispatch_first_body" | grep -q "base_function_route(name)"; then
  echo "ERROR: is_method_dispatch_first_base_function must route through BASE_FUNCTION_ROUTES" >&2
  exit 1
fi

if printf '%s\n' "$dispatch_first_body" | grep -q "matches!("; then
  echo "ERROR: dispatch-first names must be classified in BASE_FUNCTION_ROUTES, not a matches! list" >&2
  exit 1
fi

string_file="subset_julia_vm/src/compile/expr/builtin_string.rs"
STRING_ROUTES_FILE="$(mktemp)"
STRING_EXEMPTIONS_FILE="$(mktemp)"
STRING_DIRECT_FILE="$(mktemp)"
ROUTE_NAMES_FILE="$(mktemp)"
DOC_ROUTE_NAMES_FILE="$(mktemp)"
ROUTE_MISSING_DOC_FILE="$(mktemp)"
ROUTE_MISSING_CODE_FILE="$(mktemp)"
trap 'rm -f "$STRING_ROUTES_FILE" "$STRING_EXEMPTIONS_FILE" "$STRING_DIRECT_FILE" "$ROUTE_NAMES_FILE" "$DOC_ROUTE_NAMES_FILE" "$ROUTE_MISSING_DOC_FILE" "$ROUTE_MISSING_CODE_FILE"' EXIT

awk '
  /^[[:space:]]{12}"[^"]+"[[:space:]]*=>[[:space:]]*\{/ {
    line = $0
    sub(/^[[:space:]]*"/, "", line)
    sub(/".*/, "", line)
    current = line
  }
  /self\.emit\(Instr::CallBuiltin/ && current != "" {
    print current
  }
' "$string_file" | sort -u > "$STRING_DIRECT_FILE"

awk '
  /BASE_FUNCTION_ROUTES/ { in_routes = 1 }
  in_routes { print }
  in_routes && /^[[:space:]]*];/ { exit }
' "$file" > "$STRING_ROUTES_FILE"

cat > "$STRING_EXEMPTIONS_FILE" <<'EOF'
_regex_replace
_substring_retag
EOF

while IFS= read -r name; do
  [[ -n "$name" ]] || continue
  if ! grep -Fq "$name" docs/vm/BUILTIN_REMOVAL.md && \
     ! grep -Fq "$name" docs/vm/BUILTIN_OWNERSHIP.md && \
     ! grep -Fq "$name" docs/vm/UNIMPLEMENTED.md; then
    echo "ERROR: compile_builtin_string exemption '$name' is not documented." >&2
    echo "       Document the internal boundary in docs/vm before exempting it." >&2
    exit 1
  fi
done < "$STRING_EXEMPTIONS_FILE"

while IFS= read -r name; do
  [[ -n "$name" ]] || continue
  if ! grep -q "\"$name\"" "$STRING_ROUTES_FILE" && ! grep -qx "$name" "$STRING_EXEMPTIONS_FILE"; then
    echo "ERROR: compile_builtin_string direct builtin '$name' is not classified." >&2
    echo "       Add it to BASE_FUNCTION_ROUTES or the documented string fallback exemption list." >&2
    exit 1
  fi
done < "$STRING_DIRECT_FILE"

doc_file="docs/vm/PUBLIC_FALLBACKS.md"
if [[ ! -f "$doc_file" ]]; then
  echo "ERROR: public fallback route inventory doc is missing: $doc_file" >&2
  exit 1
fi

awk '
  /BASE_FUNCTION_ROUTES/ { in_routes = 1 }
  in_routes && /^[[:space:]]*];/ { exit }
  in_routes && /^[[:space:]]*(route|marker)\(/ { in_entry = 1 }
  in_entry && /"/ {
    line = $0
    sub(/^[^"]*"/, "", line)
    sub(/".*/, "", line)
    print line
    in_entry = 0
  }
' "$file" | sort > "$ROUTE_NAMES_FILE"

awk '
  /^## Inventory/ { in_inventory = 1; next }
  /^## Current/ { exit }
  in_inventory && /^\| `/ {
    line = $0
    sub(/^\| `/, "", line)
    sub(/` .*/, "", line)
    print line
  }
' "$doc_file" | sort > "$DOC_ROUTE_NAMES_FILE"

comm -23 "$ROUTE_NAMES_FILE" "$DOC_ROUTE_NAMES_FILE" > "$ROUTE_MISSING_DOC_FILE"
comm -13 "$ROUTE_NAMES_FILE" "$DOC_ROUTE_NAMES_FILE" > "$ROUTE_MISSING_CODE_FILE"

if [[ -s "$ROUTE_MISSING_DOC_FILE" ]]; then
  echo "ERROR: BASE_FUNCTION_ROUTES entries missing from $doc_file:" >&2
  cat "$ROUTE_MISSING_DOC_FILE" >&2
  exit 1
fi

if [[ -s "$ROUTE_MISSING_CODE_FILE" ]]; then
  echo "ERROR: $doc_file documents routes missing from BASE_FUNCTION_ROUTES:" >&2
  cat "$ROUTE_MISSING_CODE_FILE" >&2
  exit 1
fi

echo "OK: public Base fallback routing is centralized in BASE_FUNCTION_ROUTES."
