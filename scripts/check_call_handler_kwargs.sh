#!/usr/bin/env bash
# check_call_handler_kwargs.sh — Issue #3324
#
# Verify that inline kwparam loops in vm/exec/ do not increase beyond baseline.
# New call handlers should use bind_kwargs_defaults() or bind_kwargs_with_map()
# instead of inline loops over func.kwparams.
#
# The shared helpers in call.rs (bind_kwargs_defaults, bind_kwargs_with_map)
# correctly handle kwargs varargs (empty Pairs), which inline loops often miss.
#
# Baseline: 10 known inline loops (excluding call.rs which defines the helpers).
# When migrating an inline loop to use the shared helper, decrease this count.
set -euo pipefail

BASELINE=10

count=0
while IFS= read -r line; do
  file=$(echo "$line" | cut -d: -f1)
  base=$(basename "$file")
  # call.rs contains bind_kwargs_defaults / bind_kwargs_with_map definitions
  if [ "$base" = "call.rs" ]; then
    continue
  fi
  count=$((count + 1))
done < <(grep -rn "for kwparam in &func\.kwparams" subset_julia_vm_vm/src/vm/exec/ || true)

if [ "$count" -gt "$BASELINE" ]; then
  echo "ERROR: Found $count inline kwparam loops (baseline: $BASELINE). New call handlers must use bind_kwargs_defaults() (Issue #3324)" >&2
  echo "Run: grep -rn 'for kwparam in &func.kwparams' subset_julia_vm_vm/src/vm/exec/ to see all occurrences" >&2
  exit 1
fi

if [ "$count" -lt "$BASELINE" ]; then
  echo "NOTE: Inline kwparam loops decreased to $count (baseline: $BASELINE). Update BASELINE in this script." >&2
fi

echo "OK: Inline kwparam loop count ($count) within baseline ($BASELINE) (Issue #3324)"
