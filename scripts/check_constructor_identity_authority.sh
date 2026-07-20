#!/usr/bin/env bash
# check_constructor_identity_authority.sh — constructor identity authority audit
# (Issue #11043).
#
# `MethodSig` deliberately projects away Julia's implicit constructor-self
# argument. The serialized `MethodTable::constructor_self_families` map is the
# sole authority for inner-vs-outer identity; a per-signature boolean would
# split that authority and can disagree after deduplication or cache replay.
#
# This audit also anchors the two reconstruction paths reviewed in #11043:
# type-stability analysis must build signatures through the canonical
# `MethodSig::from_julia_projections` constructor, and MethodSig/MethodTable
# cache replay must retain `core_signature` plus `constructor_self_families`.

set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

python3 - <<'PY'
import pathlib
import re
import sys

METHOD_TABLE = pathlib.Path("subset_julia_vm_bytecode/src/method_table.rs")
CONSTRUCTORS = pathlib.Path("subset_julia_vm_compile/src/compile/expr/call/constructors.rs")
ANALYZER = pathlib.Path("subset_julia_vm_compile/src/compile/type_stability/analyzer.rs")
CACHE = pathlib.Path("subset_julia_vm_compile/src/compile/cache.rs")

paths = (METHOD_TABLE, CONSTRUCTORS, ANALYZER, CACHE)
missing = [str(path) for path in paths if not path.is_file()]
if missing:
    print(
        "ERROR: constructor identity audit target(s) missing: " + ", ".join(missing)
        + ". Update scripts/check_constructor_identity_authority.sh if code moved "
        + "(Issue #11043).",
        file=sys.stderr,
    )
    sys.exit(1)


def strip_comments_and_literals(src):
    """Preserve newlines/length while masking Rust comments and literals."""
    out = list(src)
    i = 0
    state = "normal"
    block_depth = 0
    while i < len(src):
        c = src[i]
        nxt = src[i + 1] if i + 1 < len(src) else ""
        if state == "normal":
            if c == "/" and nxt == "/":
                out[i] = out[i + 1] = " "
                i += 2
                state = "line"
                continue
            if c == "/" and nxt == "*":
                out[i] = out[i + 1] = " "
                i += 2
                state = "block"
                block_depth = 1
                continue
            if c == '"':
                out[i] = " "
                i += 1
                state = "string"
                continue
            if c == "'" and ((nxt == "\\") or (i + 2 < len(src) and src[i + 2] == "'")):
                out[i] = " "
                i += 1
                state = "char"
                continue
            i += 1
            continue
        if state == "line":
            if c == "\n":
                state = "normal"
            else:
                out[i] = " "
            i += 1
            continue
        if state == "block":
            if c == "/" and nxt == "*":
                out[i] = out[i + 1] = " "
                block_depth += 1
                i += 2
                continue
            if c == "*" and nxt == "/":
                out[i] = out[i + 1] = " "
                block_depth -= 1
                i += 2
                if block_depth == 0:
                    state = "normal"
                continue
            if c != "\n":
                out[i] = " "
            i += 1
            continue
        if state in ("string", "char"):
            terminal = '"' if state == "string" else "'"
            if c == "\\" and nxt:
                out[i] = " "
                if nxt != "\n":
                    out[i + 1] = " "
                i += 2
                continue
            if c == terminal:
                out[i] = " "
                state = "normal"
            elif c != "\n":
                out[i] = " "
            i += 1
    return "".join(out)


def braced_item(src, pattern, label):
    masked = strip_comments_and_literals(src)
    match = re.search(pattern, masked, re.MULTILINE)
    if match is None:
        fail(f"anchor `{label}` not found; update the audit if it moved")
    opening = masked.find("{", match.end())
    if opening < 0:
        fail(f"opening brace for `{label}` not found")
    depth = 0
    for index in range(opening, len(masked)):
        if masked[index] == "{":
            depth += 1
        elif masked[index] == "}":
            depth -= 1
            if depth == 0:
                return masked[match.start() : index + 1]
    fail(f"closing brace for `{label}` not found")


errors = []


def fail(message):
    errors.append(message)
    return ""


method_src = METHOD_TABLE.read_text(encoding="utf-8")
constructors_src = CONSTRUCTORS.read_text(encoding="utf-8")
analyzer_src = ANALYZER.read_text(encoding="utf-8")
cache_src = CACHE.read_text(encoding="utf-8")

method_sig = braced_item(method_src, r"\bpub\s+struct\s+MethodSig\b\s*", "MethodSig")
method_wire = braced_item(method_src, r"\bstruct\s+MethodSigWire\b\s*", "MethodSigWire")
method_table = braced_item(method_src, r"\bpub\s+struct\s+MethodTable\b\s*", "MethodTable")
serialize_impl = braced_item(
    method_src,
    r"\bimpl\s+Serialize\s+for\s+MethodSig\b\s*",
    "impl Serialize for MethodSig",
)
deserialize_impl = braced_item(
    method_src,
    r"\bimpl\s*<[^>]+>\s*Deserialize\s*<[^>]+>\s+for\s+MethodSig\b\s*",
    "impl Deserialize for MethodSig",
)
inner_query = braced_item(
    method_src,
    r"\bpub\s+fn\s+is_inner_constructor\s*\([^)]*\)\s*->\s*bool\s*",
    "MethodTable::is_inner_constructor",
)
selector = braced_item(
    constructors_src,
    r"\bfn\s+single_dynamic_parametric_outer_constructor_method\s*\(",
    "single_dynamic_parametric_outer_constructor_method",
)
analyzer_inner = braced_item(
    analyzer_src,
    r"\bfn\s+add_inner_constructor_method_sigs\s*\(",
    "add_inner_constructor_method_sigs",
)

for label, block in (("MethodSig", method_sig), ("MethodSigWire", method_wire)):
    if re.search(r"\bis_inner_constructor\b", block):
        fail(
            f"forbidden side boolean in {label}: constructor identity belongs only "
            "to MethodTable::constructor_self_families (Issue #11043)"
        )
    bool_fields = re.findall(r"\b(?:pub\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*:\s*bool\b", block)
    unexpected_bool_fields = [name for name in bool_fields if name != "is_base_extension"]
    if unexpected_bool_fields:
        fail(
            f"forbidden constructor-origin side boolean(s) in {label}: "
            + ", ".join(unexpected_bool_fields)
            + "; only is_base_extension is an allowed bool field (Issue #11043)"
        )

# A field read has no call parentheses. Calls on MethodTable are allowed because
# they consult the authoritative map. Scan all production crate sources so the
# stale boolean cannot simply move to another consumer.
field_read = re.compile(r"\.\s*is_inner_constructor\b(?!\s*\()")
for root in pathlib.Path(".").glob("subset_julia_vm*/src"):
    for path in root.rglob("*.rs"):
        masked = strip_comments_and_literals(path.read_text(encoding="utf-8"))
        match = field_read.search(masked)
        if match:
            line = masked.count("\n", 0, match.start()) + 1
            fail(
                f"forbidden MethodSig-style is_inner_constructor field read at "
                f"{path}:{line}; query the owning MethodTable instead (Issue #11043)"
            )

if not re.search(
    r"#\s*\[\s*serde\s*\(\s*default\s*\)\s*\]\s*"
    r"constructor_self_families\s*:\s*BTreeMap\s*<\s*usize\s*,\s*ConstructorSelfFamily\s*>",
    method_table,
):
    fail(
        "MethodTable must serialize defaulted constructor_self_families as "
        "BTreeMap<usize, ConstructorSelfFamily> (Issue #11043)"
    )

compact_inner_query = re.sub(r"\s+", "", inner_query)
authoritative_query_bodies = (
    "self.constructor_self_families.contains_key(&global_index)",
    "self.constructor_self_families.get(&global_index).is_some()",
)
if not any(
    compact_inner_query.endswith("{" + body + "}") for body in authoritative_query_bodies
):
    fail(
        "MethodTable::is_inner_constructor must return only the direct "
        "constructor_self_families membership query (Issue #11043)"
    )

selector_receiver = re.search(
    r"for\s*\(\s*table_name\s*,\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)\s*"
    r"in\s+self\s*\.\s*method_tables",
    selector,
)
if selector_receiver is None:
    fail(
        "dynamic parametric outer selector no longer exposes its method-table "
        "receiver; update the audit if iteration moved (Issue #11043)"
    )
else:
    receiver = re.escape(selector_receiver.group(1))
    leading_authority = re.compile(
        rf"\bif\s+{receiver}\s*\.\s*is_inner_constructor\s*\(\s*"
        r"method\s*\.\s*global_index\s*\)\s*\|\|"
    )
    if not leading_authority.search(selector):
        fail(
            "dynamic parametric outer selection must use the owning MethodTable "
            "inner-constructor query as the leading rejection disjunct (Issue #11043)"
        )

if not re.search(
    r"#\s*\[\s*derive\s*\([^]]*\bSerialize\b[^]]*\bDeserialize\b[^]]*\)\s*\]"
    r"\s*pub\s+struct\s+MethodTable\b",
    strip_comments_and_literals(method_src),
):
    fail(
        "MethodTable must derive Serialize and Deserialize so the constructor "
        "family authority crosses cache boundaries (Issue #11043)"
    )

compact_analyzer_inner = re.sub(r"\s+", "", analyzer_inner)
if compact_analyzer_inner.count("MethodSig::from_julia_projections(") != 1:
    fail(
        "type-stability inner-constructor reconstruction must use canonical "
        "MethodSig::from_julia_projections (Issue #11043)"
    )
if not re.search(
    r"letsig=MethodSig::from_julia_projections\(.*?\);"
    r"engine\.add_initial_method\(struct_def\.name\.clone\(\),sig\);",
    compact_analyzer_inner,
):
    fail(
        "type-stability inner-constructor reconstruction no longer seeds the "
        "shared inference method table (Issue #11043)"
    )

wire_requirements = (
    (r"let\s+core_signature\s*=\s*self\s*\.\s*core_signature\s*\(\s*\)",
     "MethodSig serialization must source core_signature canonically"),
    (r"core_signature\s*:\s*wire\s*\.\s*core_signature",
     "MethodSig deserialization must restore the canonical core_signature"),
)
for block, (pattern, message) in zip(
    (serialize_impl, deserialize_impl), wire_requirements
):
    if not re.search(pattern, block):
        fail(f"{message} (Issue #11043)")

cache_test = braced_item(
    cache_src,
    r"\bfn\s+base_constructor_self_family_survives_cache_round_trip_10962\s*\(",
    "base_constructor_self_family_survives_cache_round_trip_10962",
)
if not re.search(
    r"#\s*\[\s*test\s*\]\s*fn\s+"
    r"base_constructor_self_family_survives_cache_round_trip_10962\s*\(",
    strip_comments_and_literals(cache_src),
):
    fail(
        "Base-cache constructor-family round-trip function must remain a #[test] "
        "(Issue #11043)"
    )
for token in (
    "serialize_base_cache",
    "deserialize_base_cache",
    "is_explicit_parametric_inner_constructor",
):
    if token not in cache_test:
        fail(
            "real Base-cache constructor-family round-trip test lost required "
            f"`{token}` coverage (Issue #11043)"
        )
if not re.search(
    r"assert_eq\s*!\s*\(\s*fresh_origins\s*,\s*restored_origins\s*,", cache_test
):
    fail(
        "Base-cache constructor-family round-trip test must assert the complete "
        "fresh/restored origin maps are equal (Issue #11043)"
    )

if errors:
    for error in errors:
        print(f"ERROR: {error}", file=sys.stderr)
    print(
        f"FAILED: {len(errors)} constructor identity authority violation(s) found.",
        file=sys.stderr,
    )
    sys.exit(1)

print(
    "OK: constructor identity has one table-owned authority; analyzer and cache "
    "reconstruction retain canonical signatures (Issue #11043)."
)
PY
