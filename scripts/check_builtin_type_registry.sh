#!/usr/bin/env bash
# check_builtin_type_registry.sh — canonical builtin type-name authority audit
# (Issue #10954).
#
# Exact builtin spellings and nominal JuliaType projections belong only to the
# types-crate registry. This audit fingerprints the complete registry contract,
# keeps representative category entries visible, and pins the semantic
# parser/compiler/reflection consumers to their checked projections.

set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

python3 - <<'PY'
import hashlib
import pathlib
import re
import sys

REGISTRY = pathlib.Path("subset_julia_vm_types/src/types/builtin_type_registry.rs")
PARSER = pathlib.Path("subset_julia_vm_types/src/types/julia_type/parsing.rs")
HELPERS = pathlib.Path("subset_julia_vm_compile/src/compile/type_helpers.rs")
EXPR = pathlib.Path("subset_julia_vm_compile/src/compile/expr/mod.rs")
REFLECTION = pathlib.Path("subset_julia_vm_vm/src/vm/builtins_reflection/mod.rs")
SEMANTIC_TESTS = pathlib.Path(
    "subset_julia_vm/tests/regression_dispatch_inference_tests.rs"
)
REFLECTION_FIXTURE = pathlib.Path(
    "subset_julia_vm/tests/fixtures/regex/regex_split_10176.jl"
)

paths = (
    REGISTRY,
    PARSER,
    HELPERS,
    EXPR,
    REFLECTION,
    SEMANTIC_TESTS,
    REFLECTION_FIXTURE,
)
missing = [str(path) for path in paths if not path.is_file()]
if missing:
    print(
        "ERROR: builtin type registry audit target(s) missing: " + ", ".join(missing)
        + ". Update scripts/check_builtin_type_registry.sh if code moved "
        + "(Issue #10954).",
        file=sys.stderr,
    )
    sys.exit(1)

errors = []


def fail(message):
    errors.append(message)


registry_src = REGISTRY.read_text(encoding="utf-8")
registry_start = registry_src.find("static BUILTIN_TYPE_SPECS: &[BuiltinTypeSpec] = &[")
registry_end = registry_src.find("\n];", registry_start)
if registry_start < 0 or registry_end < 0:
    fail("cannot isolate BUILTIN_TYPE_SPECS registry block")
    registry_block = ""
else:
    registry_block = registry_src[registry_start:registry_end]
entry_pattern = re.compile(
    r'builtin_type!\(\s*"([^"]+)"\s*,\s*(.*?)\s*,\s*'
    r'([A-Z_]+(?:\s*\|\s*[A-Z_]+)*)(?:,\s*(Core|Base))?\s*\),',
    re.DOTALL,
)
entries = [
    (
        match.group(1),
        re.sub(r"\s+", " ", match.group(2)).strip(),
        re.sub(r"\s+", "", match.group(3)),
        match.group(4),
    )
    for match in entry_pattern.finditer(registry_block)
]
entry_count = registry_block.count("builtin_type!(")

if len(entries) != entry_count:
    fail(
        "parsed {} of {} builtin registry entries; the audit must fail closed"
        .format(len(entries), entry_count)
    )

# This fingerprint is deliberately independent of the Rust registry. Any exact
# name, JuliaType projection, order, or consumer-set change must update this
# reviewed baseline rather than silently changing every derived test oracle.
registry_contract = "\n".join(
    "|".join((*entry[:3], entry[3] or "-")) for entry in entries
).encode("utf-8")
actual_contract_sha256 = hashlib.sha256(registry_contract).hexdigest()
# 93 -> 112 (Issue #11410): pin the complete Core/Base reflection authority,
# including the Core exception hierarchy added to the canonical registry.
expected_entry_count = 112
expected_contract_sha256 = (
    "529998f7ffd4cd003e3ef06632058dfc83f669dd51c6f25ee808ff62fa579575"
)
if len(entries) != expected_entry_count or actual_contract_sha256 != expected_contract_sha256:
    fail(
        "canonical builtin registry contract drifted: expected {} entries / {}, "
        "found {} / {}"
        .format(
            expected_entry_count,
            expected_contract_sha256,
            len(entries),
            actual_contract_sha256,
        )
    )

flag_bits = {
    "PARSER": 1,
    "COMPILER": 2,
    "REFLECTION": 4,
    "PARSER_COMPILER": 3,
    "COMPILER_REFLECTION": 6,
    "ALL": 7,
}


def consumers(expression):
    bits = 0
    for token in (part.strip() for part in expression.split("|")):
        if token not in flag_bits:
            fail("unknown builtin registry consumer flag '{}'".format(token))
            continue
        bits |= flag_bits[token]
    return bits


by_name = {}
for name, projection, flags, authority in entries:
    if name in by_name:
        fail("duplicate canonical builtin type name '{}'".format(name))
    bits = consumers(flags)
    if bool(bits & 4) != bool(authority):
        fail(
            "canonical '{}' must declare Core/Base authority exactly when "
            "reflection-visible".format(name)
        )
    by_name[name] = (projection, bits, authority)

if not by_name:
    fail("canonical builtin type registry has no entries")


def require(name, projection, required_bits, exact_bits=None, exact_authority=None):
    actual = by_name.get(name)
    if actual is None:
        fail("canonical builtin registry is missing representative '{}'".format(name))
        return
    actual_projection, actual_bits, actual_authority = actual
    if actual_projection != projection:
        fail(
            "canonical '{}' projection drifted: expected {}, found {}"
            .format(name, projection, actual_projection)
        )
    labels = ((1, "parser"), (2, "compiler"), (4, "reflection"))
    for bit, label in labels:
        if required_bits & bit and not actual_bits & bit:
            fail("canonical {} entry is missing {} projection".format(name, label))
    if exact_bits is not None and actual_bits != exact_bits:
        fail(
            "canonical '{}' consumer set drifted: expected {}, found {}"
            .format(name, exact_bits, actual_bits)
        )
    if exact_authority is not None and actual_authority != exact_authority:
        fail(
            "canonical '{}' authority drifted: expected {}, found {}"
            .format(name, exact_authority, actual_authority)
        )


# Concrete, abstract, parametric-family, runtime-only, and display-tag coverage.
require("Int64", "Direct(JuliaType::Int64)", 7, 7, "Core")
require("Number", "Direct(JuliaType::Number)", 7, 7, "Core")
require("Vector", 'Nominal("Vector")', 7, 7, "Base")
require("MemoryRef", 'Nominal("MemoryRef")', 2, 2)
require("Base.Generator", "Direct(JuliaType::Generator)", 1, 1)
require("ComplexF64", 'Nominal("Complex{Float64}")', 7, 7, "Base")

# Regression sentinel from #10953: deleting it from any projection must fail.
require("SubString", 'Nominal("SubString")', 7, 7, "Base")

def without_rust_comments(source):
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.DOTALL)
    return re.sub(r"//.*$", "", source, flags=re.MULTILINE)


consumer_checks = (
    (
        PARSER,
        "builtin_type_for_parser(name)",
        "JuliaType::from_name lost its canonical parser projection",
    ),
    (
        HELPERS,
        "builtin_type_for_compiler(name).is_some()",
        "compiler builtin predicate lost its canonical compiler projection",
    ),
    (
        EXPR,
        "crate::types::builtin_type_for_compiler(name)",
        "compiler Expr::Var lost canonical type-object emission",
    ),
    (
        REFLECTION,
        "builtin_type_binding_authority(field_name)",
        "module isdefined lost its canonical reflection projection",
    ),
)
for path, needle, reason in consumer_checks:
    count = without_rust_comments(path.read_text(encoding="utf-8")).count(needle)
    if count != 1:
        fail("{}: expected exactly one '{}', found {}".format(reason, needle, count))

# Compile/runtime tests make the source-level delegation checks observable. The
# parser coverage calls JuliaType::from_name for every fingerprinted parser row;
# the compiler test inspects Expr::Var bytecode; the fixture executes isdefined.
semantic_checks = (
    (
        REGISTRY,
        "fn parser_consumer_projects_all_registry_rows_issue_10954()",
        "parser registry semantic coverage is missing",
    ),
    (
        SEMANTIC_TESTS,
        "fn builtin_type_expr_var_emits_registry_projection_issue_10954()",
        "compiler Expr::Var registry semantic coverage is missing",
    ),
    (
        REFLECTION_FIXTURE,
        "@test isdefined(Base, :SubString)",
        "module isdefined registry semantic coverage is missing",
    ),
)
for path, needle, reason in semantic_checks:
    count = path.read_text(encoding="utf-8").count(needle)
    if count != 1:
        fail("{}: expected exactly one '{}', found {}".format(reason, needle, count))

# The old compiler authority must not regrow beside the canonical registry.
helper_src = HELPERS.read_text(encoding="utf-8")
start = helper_src.find("pub(super) fn is_builtin_type_name")
end = helper_src.find("/// Get the abstract type ancestors", start)
if start < 0 or end < 0:
    fail("cannot isolate compiler is_builtin_type_name authority wrapper")
else:
    helper_body = helper_src[start:end]
    duplicated = sorted(name for name in by_name if '"{}"'.format(name) in helper_body)
    if duplicated:
        fail(
            "compiler builtin predicate reintroduced exact-name entries: "
            + ", ".join(duplicated)
        )

if errors:
    for error in errors:
        print("ERROR: " + error, file=sys.stderr)
    print(
        "FAIL: canonical builtin type registry/consumer drift (Issue #10954)",
        file=sys.stderr,
    )
    sys.exit(1)

print(
    "OK: {} unique builtin names project through parser/compiler/reflection "
    "from one canonical registry (Issue #10954).".format(len(by_name))
)
PY
