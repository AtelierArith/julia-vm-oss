#!/usr/bin/env bash
# check_struct_registry_first_match.sh — exact-or-Any constructor identity:
# no order-derived first-winner selection over hash-backed struct registries
# (Issue #11436).
#
# INVARIANT (Issue #11434 root cause): constructor return identity is either
# EXACT (owning module plus complete type parameters, i.e. an exact-key
# lookup) or it stays `Any` for runtime dispatch to resolve. A same-base /
# prefix / suffix / short-name scan over a HASH-backED registry
# (`struct_table` / `parametric_structs` / `base_parametric_structs`) that
# consumes the FIRST match (`.find`, `.find_map`, `.position`, `.next`) makes
# the selected type depend on HashMap seed order: it can pass every targeted
# test and pick an unrelated same-base instantiation in the full suite.
#
# Scans over these registries are allowed only when the consuming site is
# classified in the inventory below as one of:
#   unique-guarded        — the scan accepts a result only when it is the SOLE
#                           match (a second `.next()` must return None);
#   exact-key-equivalent  — the predicate is name identity modulo formatting
#                           (e.g. whitespace-compacted equality), so at most
#                           one entry can ever match;
#   enumeration           — the results feed diagnostics/candidate lists and
#                           never establish type identity.
# A new scan site fails this audit until it is reviewed and classified here,
# or (better) rewritten as ordered exact-key probes (`registry.get(name)`).
# See docs/vm/CODE_AUDITS.md ("struct_registry_first_match").

set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

python3 - <<'PY'
import pathlib
import re
import sys

ROOTS = [
    pathlib.Path("subset_julia_vm_compile/src"),
    pathlib.Path("subset_julia_vm_vm/src"),
    pathlib.Path("subset_julia_vm_bytecode/src"),
    pathlib.Path("subset_julia_vm_types/src"),
]

CONTAINERS = ("struct_table", "parametric_structs", "base_parametric_structs")
CONSUMERS = (".find(", ".find_map(", ".position(", ".next()")

# (path, owner fn, container, consumer) -> occurrence count, with the reviewed
# classification recorded beside each entry. Counts must match exactly. The
# recorded consumer is the first consumer token inside the scan chain; a
# `.next()` may be a textual hit from `rsplit('.').next()` inside the scan's
# own predicate — the classification below covers the SELECTION semantics of
# the whole site.
EXPECTED = {
    # unique-guarded: collision domain accepts a sole qualified owner
    # (`owners.next()` pair); ambiguity falls back to the bare name.
    ("subset_julia_vm_compile/src/compile/context.rs", "canonical_parametric_base_name", "parametric_structs", ".next()"): 1,
    # unique-guarded: `take(2)` + `len() == 1` keeps only a sole
    # exact-or-short-name match.
    ("subset_julia_vm_compile/src/compile/expr/call/module_call.rs", "compile_module_call_via_method_table", "struct_table", ".next()"): 1,
    # exact-key-equivalent: whitespace-compacted name identity for the Dict
    # return-type sharpening lane.
    ("subset_julia_vm_compile/src/compile/pipeline_ctx.rs", "build_method_tables", "struct_table", ".find("): 1,
}

errors = []


def fail(message):
    errors.append(message)


def strip_comments_and_literals(source):
    out = list(source)
    index = 0
    state = "normal"
    block_depth = 0
    while index < len(source):
        char = source[index]
        following = source[index + 1] if index + 1 < len(source) else ""
        if state == "normal":
            if char == "/" and following == "/":
                out[index] = out[index + 1] = " "
                index += 2
                state = "line"
                continue
            if char == "/" and following == "*":
                out[index] = out[index + 1] = " "
                index += 2
                state = "block"
                block_depth = 1
                continue
            if char == '"':
                out[index] = " "
                index += 1
                state = "string"
                continue
            if char == "'" and (
                following == "\\"
                or (index + 2 < len(source) and source[index + 2] == "'")
            ):
                out[index] = " "
                index += 1
                state = "char"
                continue
            index += 1
            continue
        if state == "line":
            if char == "\n":
                state = "normal"
            else:
                out[index] = " "
            index += 1
            continue
        if state == "block":
            if char == "/" and following == "*":
                out[index] = out[index + 1] = " "
                block_depth += 1
                index += 2
                continue
            if char == "*" and following == "/":
                out[index] = out[index + 1] = " "
                block_depth -= 1
                index += 2
                if block_depth == 0:
                    state = "normal"
                continue
            if char != "\n":
                out[index] = " "
            index += 1
            continue
        terminal = '"' if state == "string" else "'"
        if char == "\\" and following:
            out[index] = " "
            if following != "\n":
                out[index + 1] = " "
            index += 2
            continue
        if char == terminal:
            out[index] = " "
            state = "normal"
        elif char != "\n":
            out[index] = " "
        index += 1
    return "".join(out)


def braced_region(masked, opening):
    depth = 0
    for index in range(opening, len(masked)):
        if masked[index] == "{":
            depth += 1
        elif masked[index] == "}":
            depth -= 1
            if depth == 0:
                return masked[opening : index + 1]
    return ""


def function_regions(masked):
    regions = []
    pattern = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)(?:\s*<[^>{}]*>)?\s*\(")
    for match in pattern.finditer(masked):
        opening = masked.find("{", match.end())
        if opening < 0:
            continue
        body = braced_region(masked, opening)
        if body:
            regions.append((opening, opening + len(body), match.group(1)))
    return regions


def owner_at(regions, index):
    containing = [region for region in regions if region[0] <= index < region[1]]
    if not containing:
        return "<module>"
    return min(containing, key=lambda region: region[1] - region[0])[2]


WINDOW = 420
observed = {}
container_pattern = re.compile(r"\b(" + "|".join(CONTAINERS) + r")\b")
for root in ROOTS:
    for path in sorted(root.rglob("*.rs")):
        source = path.read_text(encoding="utf-8")
        masked = strip_comments_and_literals(source)
        regions = function_regions(masked)
        for match in container_pattern.finditer(masked):
            window = masked[match.end() : match.end() + WINDOW]
            # Only a scan of the registry itself: `.iter()` must hang directly
            # off the container mention (whitespace only in between). A chain
            # that first redirects through `.get(...)`/a field is not a
            # registry scan.
            iter_match = re.match(r"\s*\.iter\(\)", window)
            if iter_match is None:
                continue
            iter_pos = iter_match.start() + window[iter_match.start():].find(".iter()")
            # Stop at the first statement terminator after the .iter().
            chain = window[iter_pos:]
            terminator = chain.find(";")
            if terminator >= 0:
                chain = chain[:terminator]
            consumer = next((c for c in CONSUMERS if c in chain), None)
            if consumer is None:
                continue
            key = (
                str(path),
                owner_at(regions, match.start()),
                match.group(1),
                consumer,
            )
            observed[key] = observed.get(key, 0) + 1

if observed != EXPECTED:
    fail(
        "hash-backed struct registry first-winner scan inventory drifted;\n"
        "  expected {}\n  found    {}\n"
        "A new `.iter()` + find/find_map/position/next chain over "
        "struct_table/parametric_structs establishes order-derived type "
        "identity (Issue #11434/#11436). Rewrite it as ordered exact-key "
        "probes (`registry.get(name)`), guard it to accept only a SOLE "
        "match, or classify it in scripts/check_struct_registry_first_match.sh "
        "after review.".format(sorted(EXPECTED.items()), sorted(observed.items()))
    )

if errors:
    for message in errors:
        print("ERROR: {}".format(message), file=sys.stderr)
    sys.exit(1)

print(
    "OK: {} reviewed first-winner scans over hash-backed struct registries; "
    "constructor identity stays exact-or-Any (Issues #11434/#11436).".format(
        sum(EXPECTED.values())
    )
)
PY
