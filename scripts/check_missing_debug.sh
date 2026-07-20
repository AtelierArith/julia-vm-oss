#!/usr/bin/env bash
# check_missing_debug.sh
#
# Scan compile/ and aot/ for public structs/enums that are missing #[derive(Debug)].
# Types used in test assertions must derive Debug so that assert!(matches!())
# can include {:?} in failure messages (Issue #3096, #3108).
#
# Usage:
#   bash scripts/check_missing_debug.sh
#
# Exit code:
#   0 — no unexpected violations found
#   1 — one or more public types are missing Debug beyond the known exceptions
#
# Note: This script complements the workspace-level `missing_debug_implementations = "warn"`
# lint in Cargo.toml. The Cargo lint catches all missing Debug implementations at compile
# time; this script provides a fast pre-commit check without requiring a full build.
#
# KNOWN EXCEPTIONS — complex engine types that cannot trivially derive Debug because
# they hold non-Debug fields (e.g. closures, raw pointers, cranelift codegen state).
# When adding a new entry here, explain WHY Debug cannot be derived.
KNOWN_EXCEPTIONS=(
    # compile/abstract_interp/engine/mod.rs — holds non-Debug inference state (Arc, callbacks)
    "compile/abstract_interp/engine/mod.rs"
    # compile/context.rs — SharedCompileContext holds Arc<Mutex<...>> non-Debug state
    "compile/context.rs"
    # compile/ipo/worklist.rs — IPOInferenceEngine<'a> holds references to compiler internals
    "compile/ipo/worklist.rs"
    # compile/type_stability/analyzer.rs — TypeStabilityAnalyzer holds complex state
    "compile/type_stability/analyzer.rs"
    # aot/analyze/ir_converter/mod.rs — IrConverter<'a> holds references to non-Debug types
    "aot/analyze/ir_converter/mod.rs"
    # aot/codegen/cranelift/mod.rs — CraneliftCodeGenerator wraps cranelift types without Debug
    "aot/codegen/cranelift/mod.rs"
    # aot/inference/engine/mod.rs — TypeInferenceEngine holds complex inference state
    "aot/inference/engine/mod.rs"
    # compile/tfuncs/registry.rs — TransferRule has a manual `impl Debug` that
    # renders the `fn`-pointer field as "<fn>" instead of a raw address; the
    # audit's regex only recognises `#[derive(Debug)]`, so the explicit impl
    # is reported as a false positive without this exception.
    "compile/tfuncs/registry.rs"
)

set -euo pipefail

VIOLATIONS=()

is_exception() {
    local file="$1"
    for exc in "${KNOWN_EXCEPTIONS[@]}"; do
        if [[ "$file" == *"$exc"* ]]; then
            return 0
        fi
    done
    return 1
}

# Walk every .rs file in compile/ and aot/
while IFS= read -r srcfile; do
    if is_exception "$srcfile"; then
        continue
    fi
    # Use Python to detect pub struct/enum lines without Debug in preceding 10 lines.
    # Use a `while read` loop instead of `mapfile` so this script works on macOS's
    # default /bin/bash (3.2), which predates the bash 4 `mapfile` builtin (Issue #3766).
    file_violations=()
    while IFS= read -r line; do
        file_violations+=("$line")
    done < <(python3 - "$srcfile" <<'PYEOF'
import sys
import re

with open(sys.argv[1]) as fh:
    lines = fh.readlines()

derive_re = re.compile(r'#\[derive\([^)]*\bDebug\b')
# Match pub struct/enum, pub(crate) struct/enum, pub(super) struct/enum
pub_re = re.compile(r'^\s*pub(\(\w+\))?\s+(struct|enum)\s+\w')
manual_debug_re = re.compile(r'\bimpl(?:<[^>]+>)?\s+(?:std::fmt::)?Debug\s+for\s+(\w+)')

manual_debug_types = {m.group(1) for text in [''.join(lines)] for m in manual_debug_re.finditer(text)}

for i, line in enumerate(lines):
    match = pub_re.search(line)
    if match:
        type_name = re.sub(r'^\s*pub(\(\w+\))?\s+(struct|enum)\s+', '', line).split()[0].split('<')[0]
        # Search the preceding 10 lines for a #[derive(...Debug...)]
        window = lines[max(0, i - 10):i]
        if not any(derive_re.search(w) for w in window) and type_name not in manual_debug_types:
            print(f"{sys.argv[1]}:{i + 1}: {line.rstrip()}")
PYEOF
    )
    # Guard the splat: bash 3.2 with `set -u` errors on empty `${arr[@]}`.
    VIOLATIONS+=("${file_violations[@]+"${file_violations[@]}"}")
done < <(find subset_julia_vm_compile/src/compile subset_julia_vm/src/aot -name "*.rs" -type f 2>/dev/null | sort)

if [[ ${#VIOLATIONS[@]} -eq 0 ]]; then
    echo "check_missing_debug: OK — all public types in compile/ and aot/ derive Debug"
    echo "  (${#KNOWN_EXCEPTIONS[@]} known exceptions excluded — see KNOWN_EXCEPTIONS in this script)"
    exit 0
fi

echo "check_missing_debug: FAIL — found ${#VIOLATIONS[@]} public type(s) missing #[derive(Debug)]:"
echo ""
for v in "${VIOLATIONS[@]}"; do
    echo "  $v"
done
echo ""
echo "Fix: add #[derive(Debug)] (or a manual impl Debug) to each type above."
echo "If the type cannot derive Debug (e.g. it holds closures or raw pointers),"
echo "add the file to KNOWN_EXCEPTIONS in this script with an explanation."
echo "See CLAUDE.md 'Rust Test Assertion Style' for the convention."
exit 1
