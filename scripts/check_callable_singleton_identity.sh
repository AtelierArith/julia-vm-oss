#!/usr/bin/env bash
# Callable singleton identity authority and carrier coverage (Issue #11703).

set -euo pipefail

cd "$(dirname "$0")/.."

python3 - <<'PY'
import re
import sys
from collections import Counter
from pathlib import Path


def fail(message: str) -> None:
    print(f"FAIL: callable singleton identity audit: {message}", file=sys.stderr)
    raise SystemExit(1)


def read(path: str) -> str:
    source = Path(path)
    if not source.is_file():
        fail(f"required source is missing: {path}")
    return source.read_text(encoding="utf-8")


def braced_item(text: str, marker: str, label: str) -> str:
    start = text.find(marker)
    if start < 0:
        fail(f"{label} declaration is missing")
    brace = text.find("{", start)
    if brace < 0:
        fail(f"{label} declaration has no body")
    depth = 0
    for index in range(brace, len(text)):
        if text[index] == "{":
            depth += 1
        elif text[index] == "}":
            depth -= 1
            if depth == 0:
                return text[start : index + 1]
    fail(f"{label} declaration has an unterminated body")
    return ""


metadata_path = "subset_julia_vm_bytecode/src/value/metadata.rs"
metadata = read(metadata_path)
for carrier in ("FunctionValue", "ClosureValue"):
    struct = braced_item(metadata, f"pub struct {carrier}", carrier)
    if struct.count("singleton_identity: Rc<CallableSingletonIdentity>") != 1:
        fail(f"{carrier} lost its private CallableSingletonIdentity carrier")
    implementation = braced_item(metadata, f"impl {carrier}", f"{carrier} impl")
    if "pub fn singleton_identity(&self) -> &CallableSingletonIdentity" not in implementation:
        fail(f"{carrier} lost its singleton_identity authority accessor")
    if "with_candidates_and_identity" not in implementation:
        fail(f"{carrier} lost identity-preserving candidate construction")

generator_path = "subset_julia_vm_bytecode/src/value/generator.rs"
generator = read(generator_path)
generator_enum = braced_item(generator, "pub enum GeneratorCallable", "GeneratorCallable")
index_variants = set(
    re.findall(r"^\s*([A-Za-z0-9_]*FunctionIndex)\s*(?:\(|\{)", generator_enum, re.MULTILINE)
)
expected_index_variants = {
    "FunctionIndex",
    "FilteredFunctionIndex",
    "TupleSplatFunctionIndex",
}
if index_variants != expected_index_variants:
    fail(
        "GeneratorCallable function-index carrier set changed: expected {}, found {}; "
        "extend the #11703 relocation matrix and audit".format(
            sorted(expected_index_variants), sorted(index_variants)
        )
    )

matrix_path = "subset_julia_vm_vm/tests/internal/persisted_callable_replacement_9784_test.rs"
matrix = read(matrix_path)
for anchor in (
    "persisted_callable_carrier_matrix_preserves_owner_and_helper_provenance_11703",
    "Value::Function(helper_function)",
    "Value::Closure(helper_closure)",
    "GeneratorCallable::FunctionIndex",
    "GeneratorCallable::TupleSplatFunctionIndex",
    "GeneratorCallable::FilteredFunctionIndex",
    "GeneratorCallable::RuntimeValue",
    "GeneratorCallable::TupleSplatRuntimeValue",
    "GeneratorCallable::FilteredRuntimeValue",
    "singleton_identity().is_lowering_helper()",
    "singleton_identity().owner_names()",
    "deep_copy_value",
):
    if anchor not in matrix:
        fail(f"#11703 carrier matrix lost required evidence {anchor!r}")

introspection = read("subset_julia_vm_vm/src/vm/type_ops/introspection.rs")
for anchor in (
    "callable_singleton_identity_for_candidates",
    "FunctionValue::with_candidates_and_identity",
    "ClosureValue::with_candidates_and_identity",
):
    if anchor not in introspection:
        fail(f"callable construction lost identity authority {anchor!r}")

comparison = read("subset_julia_vm_vm/src/vm/type_ops/comparison.rs")
if comparison.count("singleton_identity().encoded_name()") != 2:
    fail("callable comparison must canonicalize FunctionValue and ClosureValue via identity")

deep_copy = read("subset_julia_vm_vm/src/vm/type_ops/deep_copy.rs")
if "c.singleton_identity().clone()" not in deep_copy:
    fail("closure deep copy stopped preserving CallableSingletonIdentity")

vm_mod = read("subset_julia_vm_vm/src/vm/mod.rs")
if vm_mod.count("singleton_dispatch_key()") != 2:
    fail("runtime type keys must use FunctionValue and ClosureValue singleton dispatch keys")

state = read("subset_julia_vm_vm/src/vm/state.rs")
remap = braced_item(
    state,
    "pub(super) fn remap_persisted_callable_value",
    "remap_persisted_callable_value",
)
for variant in expected_index_variants:
    if f"GeneratorCallable::{variant}" not in remap:
        fail(f"persisted generator remap lost {variant}")
if remap.count("persisted_function_value_for_index(") != 4:
    fail("generator fallback must build four identity-preserving runtime callable values")
for anchor in ("Value::Function(function)", "Value::Closure(closure)"):
    if anchor not in remap:
        fail(f"persisted callable remap lost carrier {anchor}")

reflection = read("subset_julia_vm_vm/src/vm/builtins_reflection/mod.rs")
if "fallback.is_lowering_helper && source_names.contains" not in reflection:
    fail("reflection lost helper/source collision filtering")

# Callable identity construction in production VM code is an authority boundary.
# New sites must be reviewed and added here rather than deriving identity ad hoc.
construction_pattern = re.compile(
    r"CallableSingletonIdentity::(source|from_provenance|with_owners)\s*\("
)
actual_constructions: Counter[tuple[str, str]] = Counter()
for path in sorted(Path("subset_julia_vm_vm/src").rglob("*.rs")):
    text = path.read_text(encoding="utf-8")
    for match in construction_pattern.finditer(text):
        actual_constructions[(path.as_posix(), match.group(1))] += 1
expected_constructions = Counter(
    {
        ("subset_julia_vm_vm/src/vm/repl_support.rs", "from_provenance"): 1,
        ("subset_julia_vm_vm/src/vm/type_ops/introspection.rs", "with_owners"): 2,
        ("subset_julia_vm_vm/src/vm/type_ops/introspection.rs", "from_provenance"): 1,
        ("subset_julia_vm_vm/src/vm/state.rs", "source"): 1,
        ("subset_julia_vm_vm/src/vm/builtins_reflection/mod.rs", "from_provenance"): 1,
    }
)
if actual_constructions != expected_constructions:
    missing = expected_constructions - actual_constructions
    unexpected = actual_constructions - expected_constructions
    fail(
        "CallableSingletonIdentity construction inventory drifted; missing={}, unexpected={}".format(
            sorted(missing.items()), sorted(unexpected.items())
        )
    )

print(
    "OK: callable singleton identity authority covers FunctionValue, ClosureValue, "
    "3 generator index forms, relocation fallback, comparison, deep copy, reflection, "
    "and runtime type keys (Issue #11703)."
)
PY
