#!/usr/bin/env bash
# Issue #4766 (prevention for the StructRef-leak fix family
# #4761 / #4763 / #4725 / #4727 / #4729 / #4735 / #4755 / #5038).
#
# Every VM display entry point that pops a runtime `Value` from the stack and
# feeds it to one of the heap-less stringify helpers
#
#     vm::formatting::format_value_print(..)
#     vm::formatting::format_value(..)        (a.k.a. the `format_value(` re-export)
#     vm::formatting::value_to_string(..)
#
# for USER-VISIBLE output MUST first resolve any `Value::StructRef(idx)` against
# `self.struct_heap` — otherwise heap-allocated structs (`Pair(1, 2)`, user
# structs) leak the Rust `Debug` repr `StructRef(heap_idx=N)` into the output.
# The canonical resolution is
#
#     let resolved = crate::vm::formatting::resolve_struct_refs_for_format(
#         &val, &self.struct_heap);
#     let s = crate::vm::formatting::format_value_print(&resolved);
#
# (or an inline `if let Value::StructRef(idx) = .. { self.struct_heap.get(idx) }`
# top-level deref). The format helpers are deliberately heap-less, so the
# resolution can only happen at the call site.
#
# This audit greps the VM display-entry-point files for the three sink helpers
# and FAILS if a call's argument is not one of the known-safe forms:
#
#   - `&resolved` / `resolved`        — the canonical heap-resolved local
#   - wrapped directly in `resolve_struct_refs_for_format(` (same or next line)
#   - a `&Value::` constructed literal (already a concrete, non-StructRef value)
#
# Any other argument shape must carry an `// AUDIT(#NNNN)` allowlist comment in
# the preceding few lines explaining why no StructRef can reach the call (e.g.
# an error-message path, or a loop over an already-resolved collection).
#
# Adding a NEW display entry point? Either resolve via
# `resolve_struct_refs_for_format` before the format call, or add an
# `// AUDIT(#NNNN)` comment justifying why it is leak-safe — AND add a cell to
# the matrix fixtures (io_no_rust_debug_leak_4757 / _iobuf_4766 /
# _container_struct_matrix_4774 / _symbol_4766). See docs/vm/CODE_AUDITS.md.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# VM display entry-point files: every Rust source under subset_julia_vm/src/vm
# that pops a Value and stringifies it for user-visible output. The FFI layer
# (subset_julia_vm/src/ffi/) is intentionally excluded — its `format_value`
# has no `struct_heap` access and renders a bare StructRef as the benign
# "<struct ref>" placeholder, not the Rust Debug `StructRef(heap_idx=N)` repr.
TARGETS="
subset_julia_vm/src/vm/builtins_io.rs
subset_julia_vm/src/vm/builtins_strings.rs
subset_julia_vm/src/vm/builtins_macro/mod.rs
subset_julia_vm/src/vm/exec/string_ops.rs
subset_julia_vm/src/vm/exec/print.rs
subset_julia_vm/src/vm/exec/error_handling.rs
"

errors=0

# Defensive: ensure the canonical resolver helper still exists. If someone
# renames/removes it the audit would silently pass (no calls to anchor on),
# so anchor on its definition.
if ! rg -q "fn resolve_struct_refs_for_format" "$ROOT/subset_julia_vm/src/vm/formatting/mod.rs"; then
    echo "ERROR: resolve_struct_refs_for_format is gone from vm/formatting/mod.rs."
    echo "       The StructRef heap-resolution contract (Issue #4766) relies on it;"
    echo "       update this audit if the helper was renamed."
    errors=$((errors + 1))
fi

for rel in $TARGETS; do
    f="$ROOT/$rel"
    if [[ ! -f "$f" ]]; then
        echo "ERROR: audit target $rel no longer exists — update check_format_value_resolves_structref.sh (Issue #4766)."
        errors=$((errors + 1))
        continue
    fi

    # awk pass: walk the file line by line, remembering how many lines ago we
    # last saw an `// AUDIT(#` allowlist comment, and flag any sink call whose
    # argument is not a known-safe form.
    flagged=$(awk -v fname="$rel" '
        # A previous sink-call line ended with a bare "(" (multi-line call):
        # the argument is on THIS line. If it is the resolver, the pending
        # call is safe; otherwise fall through and re-evaluate this line
        # normally (it may itself be a flaggable sink).
        pending {
            pending = 0
            if ($0 ~ /resolve_struct_refs_for_format\(/) { resolved_pending = 1 }
        }

        # Track the most recent AUDIT allowlist comment (audit_age counts the
        # lines since it was seen).
        /\/\/[[:space:]]*AUDIT\(#/ { audit_age = 0; next }
        { audit_age++ }

        # Recognize loops over a heap-resolved collection:
        #   `for VAR in resolved_values`  /  `for VAR in &resolved_values`
        #   `for VAR in print_values`     (a slice of resolved_values)
        #   `for VAR in resolved`
        # Any `format_value*(VAR)` inside such a loop operates on an
        # already-resolved element, so remember VAR as a resolved binding.
        /for[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]+in[[:space:]]+&?(resolved_values|print_values|resolved)([^A-Za-z0-9_]|$)/ {
            loopvar = $0
            sub(/.*for[[:space:]]+/, "", loopvar)
            sub(/[[:space:]]+in.*/, "", loopvar)
            resolved_loopvar = loopvar
        }

        # A sink call is any line invoking one of the three heap-less helpers.
        # The helper may be path-qualified (crate::vm::formatting:: /
        # super::super::formatting::) or bare (format_value(, value_to_string().
        /format_value_print\(|value_to_string\(|format_value\(/ {
            line = $0

            # If the previous line was a multi-line sink whose argument (this
            # line) is the resolver, that pending call is already cleared; do
            # not also evaluate this resolver line as a fresh sink.
            if (resolved_pending) { resolved_pending = 0; next }

            # Strip the path qualifier so we can inspect just the call + arg.
            call = line
            sub(/.*(format_value_print|value_to_string|format_value)\(/, "", call)

            # Multi-line call: the open paren has nothing after it, so the
            # argument is on the following line — defer judgment to it.
            if (call ~ /^[[:space:]]*$/) { pending = 1; next }

            # Safe form 1: the canonical heap-resolved local `resolved`.
            if (call ~ /^&?resolved[,)\.]/) next
            if (call ~ /^&?resolved$/) next

            # Safe form 1b: the argument is the loop variable of a `for VAR in
            # resolved_values / print_values / resolved` loop seen above, i.e.
            # an element of an already-resolved collection.
            if (resolved_loopvar != "") {
                argname = call
                sub(/^&/, "", argname)
                sub(/[,)\.].*/, "", argname)
                if (argname == resolved_loopvar) next
            }

            # Safe form 2: the argument is itself the resolver call inline.
            if (call ~ /resolve_struct_refs_for_format\(/) next

            # Safe form 3: a `&Value::` constructed literal (concrete value,
            # never a bare StructRef placeholder).
            if (call ~ /^&Value::/) next

            # Safe form 4: an AUDIT allowlist comment within the last 8 lines.
            if (audit_age <= 8) next

            # Otherwise: a raw popped Value fed to a heap-less helper without
            # resolution and without justification — flag it.
            printf "%s:%d:%s\n", fname, FNR, $0
        }
    ' "$f" || true)

    if [[ -n "$flagged" ]]; then
        echo "ERROR: unresolved StructRef display sink(s) in $rel (Issue #4766):"
        printf '%s\n' "$flagged" | sed 's/^/    /'
        echo "    -> precede the call with resolve_struct_refs_for_format(&val, &self.struct_heap),"
        echo "       or add an // AUDIT(#NNNN) comment justifying why no StructRef can reach it."
        errors=$((errors + 1))
    fi
done

if [[ "$errors" -gt 0 ]]; then
    echo ""
    echo "FAILED: StructRef heap-resolution audit failed (Issue #4766)."
    echo "Heap-allocated structs (Pair, user structs) would leak as StructRef(heap_idx=N)."
    exit 1
fi

echo "OK: every VM display entry point resolves StructRef before formatting (Issue #4766)."
