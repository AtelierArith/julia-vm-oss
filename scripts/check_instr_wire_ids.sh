#!/usr/bin/env bash
# check_instr_wire_ids.sh
#
# Audit the stable wire-ID tables in `compile/instr_wire_ids.rs` (Issue #8628).
#
# Three checks per covered enum (BuiltinOp, BuiltinId, Intrinsic):
#
#   1. COVERAGE  — every active variant in the enum has an entry in the
#                  corresponding `*_to_wire_id()` match arm.
#   2. NO-DUP    — no two variants share the same wire ID within one table.
#   3. NO-REUSE  — wire IDs that appear in tombstone/RETIRED comments are NOT
#                  also assigned to live variants.
#
# `Instr` is intentionally excluded — full wire-ID serde for Instr is deferred
# to the Register VM migration (Issue #8448); Issue #8626 provides the safety
# net for Instr in the interim.
#
# Usage (from repo root):
#   bash scripts/check_instr_wire_ids.sh
#
# Exit code: 0 = all checks pass, 1 = one or more failures.
#
# Dependencies: python3 (stdlib only), bash 3.2+.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Post-crate-split locations (Issue #8655/#8656): the wire-ID table and the
# BuiltinOp enum live in subset_julia_vm_types; BuiltinId/Intrinsic live in
# subset_julia_vm_bytecode. The old subset_julia_vm/src paths are re-export
# shims whose files no longer contain the definitions this audit parses.
WIRE_IDS_RS="$repo_root/subset_julia_vm_types/src/ir/wire_ids.rs"
WIRE_IDS_BYTECODE_RS="$repo_root/subset_julia_vm_bytecode/src/wire_ids.rs"
BUILTINOP_SRC="$repo_root/subset_julia_vm_types/src/ir/core.rs"
BUILTINID_SRC="$repo_root/subset_julia_vm_bytecode/src/builtins.rs"
INTRINSIC_SRC="$repo_root/subset_julia_vm_bytecode/src/intrinsics.rs"

for f in "$WIRE_IDS_RS" "$WIRE_IDS_BYTECODE_RS" "$BUILTINOP_SRC" "$BUILTINID_SRC" "$INTRINSIC_SRC"; do
  if [[ ! -f "$f" ]]; then
    echo "ERROR: required file not found: $f" >&2
    exit 1
  fi
done

# ---------------------------------------------------------------------------
# Python helper — run in-process for reliability
# ---------------------------------------------------------------------------
# Note: the command substitution is assigned UNQUOTED (`RESULT=$(...)`, not
# `RESULT="$(...)"`). A command-substitution RHS of an assignment is not
# word-split, and the embedded Python f-strings contain double quotes
# (`active_counts["BuiltinOp"]`); wrapping the `$(...)` in double quotes makes
# bash 3.2 mis-parse those inner quotes ("unexpected EOF"), which silently broke
# this audit on macOS stock /bin/bash (Issue #9461).
set +e
RESULT=$(python3 - "$WIRE_IDS_RS" "$WIRE_IDS_BYTECODE_RS" "$BUILTINOP_SRC" "$BUILTINID_SRC" "$INTRINSIC_SRC" <<'PYEOF'
import sys, re

wire_ids_path, wire_ids_bytecode_path, builtinop_path, builtinid_path, intrinsic_path = sys.argv[1:]

def read(path):
    with open(path) as f:
        return f.read()

# Post-crate-split (#8655/#8656): BuiltinOp's wire table lives in
# subset_julia_vm_types, BuiltinId/Intrinsic tables in subset_julia_vm_bytecode.
wire_text_types = read(wire_ids_path)
wire_text_bytecode = read(wire_ids_bytecode_path)

# -------------------------------------------------------------------------
# Helpers
# -------------------------------------------------------------------------
def extract_enum_variants(src_text, enum_name):
    """
    Extract variant identifiers from `pub enum <enum_name> { ... }`.
    Handles simple unit variants (with or without trailing comment).
    Returns a list of variant name strings.
    """
    # Find the enum block
    m = re.search(r'pub\s+enum\s+' + re.escape(enum_name) + r'\s*\{', src_text)
    if not m:
        raise RuntimeError(f'enum {enum_name} not found')
    depth = 0
    start = m.start()
    # Scan for the matching closing brace
    body_start = src_text.index('{', start)
    pos = body_start
    brace_depth = 0
    body_chars = []
    while pos < len(src_text):
        ch = src_text[pos]
        if ch == '{':
            brace_depth += 1
        elif ch == '}':
            brace_depth -= 1
            if brace_depth == 0:
                break
        else:
            body_chars.append(ch)
        pos += 1
    body = ''.join(body_chars)
    # Extract variant names: lines that start with whitespace + identifier
    # A variant is an identifier at the start of a non-comment, non-attribute line.
    variants = []
    for raw_line in body.splitlines():
        line = raw_line.strip()
        if not line or line.startswith('//') or line.startswith('#') or line.startswith('/*'):
            continue
        # Match variant identifier (uppercase or _uppercase, then word chars)
        m2 = re.match(r'^([A-Z_][A-Za-z0-9_]*)[\s,({]?', line)
        if not m2:
            continue
        name = m2.group(1)
        # Skip if it looks like a keyword, attribute, or type annotation
        if name in ('pub', 'fn', 'let', 'use', 'const', 'type', 'where', 'impl', 'struct'):
            continue
        variants.append(name)
    return variants

def extract_to_wire_id_assignments(wire_text, enum_ns, fn_name):
    """
    From `fn <fn_name>(...) -> u32 { match v { ... } }` in wire_text,
    extract a dict {VariantName: wire_id} for all live assignments.
    Also returns (set_of_live_ids, set_of_retired_ids).
    """
    # Find the function body (pub or pub(crate) — visibility widened when the
    # tables moved to their own crates in the #8655/#8656 split)
    m = re.search(r'pub(?:\(crate\))?\s+fn\s+' + re.escape(fn_name) + r'\b[^{]*\{', wire_text)
    if not m:
        raise RuntimeError(f'function {fn_name} not found in wire_ids file')
    # Extract the block (balanced braces)
    pos = wire_text.index('{', m.start())
    brace_depth = 0
    body_chars = []
    while pos < len(wire_text):
        ch = wire_text[pos]
        if ch == '{':
            brace_depth += 1
        elif ch == '}':
            brace_depth -= 1
            if brace_depth == 0:
                break
        else:
            body_chars.append(ch)
        pos += 1
    body = ''.join(body_chars)

    # Extract retired IDs from comments in this function
    # Patterns: "// 272 is retired", "// 272 retired", "// Wire ID 272 → RETIRED"
    retired = set()
    for m2 in re.finditer(r'//[^\n]*?(\b(\d+)\b)[^\n]*(?:retire|RETIRE|RETIRED|retired)', body, re.IGNORECASE):
        # could be multiple digits; grab all
        for m3 in re.finditer(r'\b(\d+)\b', m2.group(0)):
            retired.add(int(m3.group(1)))

    # Extract live assignments. Arms may be written with any path prefix
    # depending on the crate the table lives in:
    #   `crate::builtins::BuiltinId::Sqrt => 0`   (bytecode crate)
    #   `BuiltinOp::Rand => 0`                    (types crate, local import)
    # Match on the enum's short name with an optional module path.
    enum_short = enum_ns.rsplit('::', 1)[-1]
    assignments = {}
    for m2 in re.finditer(
        r'(?:\b[a-z_][a-z0-9_]*::|crate::)*' + re.escape(enum_short)
        + r'::([A-Z_][A-Za-z0-9_]*)\s*=>\s*(\d+)',
        body
    ):
        variant = m2.group(1)
        wire_id = int(m2.group(2))
        assignments[variant] = wire_id

    live_ids = set(assignments.values())
    return assignments, live_ids, retired

errors = []

def check_enum(enum_name, src_path, enum_ns, to_fn, from_fn, wire_text):
    src_text = read(src_path)
    try:
        variants = extract_enum_variants(src_text, enum_name)
    except RuntimeError as e:
        errors.append(f'[{enum_name}] {e}')
        return

    try:
        assignments, live_ids, retired = extract_to_wire_id_assignments(wire_text, enum_ns, to_fn)
    except RuntimeError as e:
        errors.append(f'[{enum_name}] {e}')
        return

    # CHECK 1: coverage
    missing = [v for v in variants if v not in assignments]
    if missing:
        errors.append(
            f'[{enum_name}] COVERAGE: {len(missing)} variant(s) missing from {to_fn}: '
            + ', '.join(missing[:10]) + ('...' if len(missing) > 10 else '')
        )

    # CHECK 2: no duplicate wire IDs
    seen_ids = {}
    dups = []
    for variant, wid in assignments.items():
        if wid in seen_ids:
            dups.append(f'wire_id {wid}: {seen_ids[wid]} and {variant}')
        else:
            seen_ids[wid] = variant
    if dups:
        errors.append(f'[{enum_name}] NO-DUP: duplicate wire IDs: ' + '; '.join(dups))

    # CHECK 3: no retired-ID reuse
    reused = retired & live_ids
    if reused:
        errors.append(
            f'[{enum_name}] NO-REUSE: retired wire ID(s) reassigned to live variants: '
            + ', '.join(str(i) for i in sorted(reused))
        )

check_enum('BuiltinOp', builtinop_path,
           'ir::core::BuiltinOp', 'builtinop_to_wire_id', 'builtinop_from_wire_id',
           wire_text_types)
check_enum('BuiltinId', builtinid_path,
           'builtins::BuiltinId', 'builtinid_to_wire_id', 'builtinid_from_wire_id',
           wire_text_bytecode)
check_enum('Intrinsic', intrinsic_path,
           'intrinsics::Intrinsic', 'intrinsic_to_wire_id', 'intrinsic_from_wire_id',
           wire_text_bytecode)

if errors:
    print('FAIL')
    for e in errors:
        print(f'  {e}')
    sys.exit(1)
else:
    active_counts = {}
    for enum_name, src_path, enum_ns, to_fn, from_fn in [
        ('BuiltinOp', builtinop_path, 'ir::core::BuiltinOp', 'builtinop_to_wire_id', ''),
        ('BuiltinId', builtinid_path, 'builtins::BuiltinId', 'builtinid_to_wire_id', ''),
        ('Intrinsic', intrinsic_path, 'intrinsics::Intrinsic', 'intrinsic_to_wire_id', ''),
    ]:
        src_text = read(src_path)
        variants = extract_enum_variants(src_text, enum_name)
        active_counts[enum_name] = len(variants)
    print('OK: wire-ID table coverage/no-dup/no-reuse checks pass '
          f'(BuiltinOp={active_counts["BuiltinOp"]}, '
          f'BuiltinId={active_counts["BuiltinId"]}, '
          f'Intrinsic={active_counts["Intrinsic"]})')
    sys.exit(0)
PYEOF
)
EXIT=$?
set -e
echo "$RESULT"
exit $EXIT
