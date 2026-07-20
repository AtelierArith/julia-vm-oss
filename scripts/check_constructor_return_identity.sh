#!/usr/bin/env bash
# check_constructor_return_identity.sh -- exact-or-Any constructor result audit
# (Issue #11436, prevention for bugs #11434/#11468/#11469).

set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

python3 - <<'PY'
import csv
import pathlib
import re
import sys

CONTEXT = pathlib.Path("subset_julia_vm_compile/src/compile/context.rs")
REGISTRY = pathlib.Path("subset_julia_vm_compile/src/compile/tfuncs/registry.rs")
STRUCT_REGISTRY = pathlib.Path("subset_julia_vm_bytecode/src/struct_registry.rs")
COMPLEX = pathlib.Path("subset_julia_vm_compile/src/compile/tfuncs/complex_ops.rs")
VALUE_INFERENCE = pathlib.Path(
    "subset_julia_vm_compile/src/compile/expr/infer/expr_tfuncs.rs"
)
JULIA_INFERENCE = pathlib.Path(
    "subset_julia_vm_compile/src/compile/expr/infer/julia_type.rs"
)
EXPR_INFERENCE = pathlib.Path("subset_julia_vm_compile/src/compile/expr/infer/mod.rs")
CORE_COMPILER = pathlib.Path("subset_julia_vm_compile/src/compile/core_compiler.rs")
BUILTIN_ARRAY = pathlib.Path("subset_julia_vm_compile/src/compile/expr/builtin_array.rs")
CALL_COMPILER = pathlib.Path("subset_julia_vm_compile/src/compile/expr/call/mod.rs")
DISPATCH = pathlib.Path("subset_julia_vm_compile/src/compile/expr/call/dispatch.rs")
INVENTORY = pathlib.Path("docs/vm/CONSTRUCTOR_RETURN_IDENTITY_INVENTORY.tsv")
FIXTURE = pathlib.Path(
    "subset_julia_vm/tests/fixtures/dispatch/constructor_return_exact_or_any_11436.jl"
)
MANIFEST = pathlib.Path("subset_julia_vm/tests/fixtures/dispatch/manifest.toml")

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


def braced_function(source, name, raw=False):
    masked = strip_comments_and_literals(source)
    match = re.search(r"\bfn\s+" + re.escape(name) + r"(?:\s*<[^>{}]*>)?\s*\(", masked)
    if match is None:
        fail("constructor return owner '{}' is missing".format(name))
        return ""
    opening = masked.find("{", match.end())
    if opening < 0:
        fail("constructor return owner '{}' has no opening brace".format(name))
        return ""
    depth = 0
    for index in range(opening, len(masked)):
        if masked[index] == "{":
            depth += 1
        elif masked[index] == "}":
            depth -= 1
            if depth == 0:
                selected = source if raw else masked
                return selected[match.start() : index + 1]
    fail("constructor return owner '{}' has no closing brace".format(name))
    return ""


def compact(text):
    return re.sub(r"\s+", "", text)


def require(owner, block, fragment):
    if compact(fragment) not in compact(block):
        fail("{} lost exact-or-Any evidence '{}'".format(owner, fragment))


def forbid(owner, block, patterns):
    for label, pattern in patterns:
        if re.search(pattern, block):
            fail("{} contains forbidden family first-winner selector '{}'".format(owner, label))


for path in (
    CONTEXT,
    REGISTRY,
    STRUCT_REGISTRY,
    COMPLEX,
    VALUE_INFERENCE,
    JULIA_INFERENCE,
    EXPR_INFERENCE,
    CORE_COMPILER,
    BUILTIN_ARRAY,
    CALL_COMPILER,
    DISPATCH,
    INVENTORY,
    FIXTURE,
    MANIFEST,
):
    if not path.is_file():
        fail("constructor return identity audit target missing: {}".format(path))

if errors:
    for error in errors:
        print("ERROR: {} (Issue #11436)".format(error), file=sys.stderr)
    sys.exit(1)

context = CONTEXT.read_text(encoding="utf-8")
registry = REGISTRY.read_text(encoding="utf-8")
struct_registry = STRUCT_REGISTRY.read_text(encoding="utf-8")
complex_source = COMPLEX.read_text(encoding="utf-8")
value_inference = VALUE_INFERENCE.read_text(encoding="utf-8")
julia_inference = JULIA_INFERENCE.read_text(encoding="utf-8")
expr_inference = EXPR_INFERENCE.read_text(encoding="utf-8")
core_compiler = CORE_COMPILER.read_text(encoding="utf-8")
builtin_array = BUILTIN_ARRAY.read_text(encoding="utf-8")
call_compiler = CALL_COMPILER.read_text(encoding="utf-8")
dispatch = DISPATCH.read_text(encoding="utf-8")

retired_identifiers = ("instantiation_of", "base_struct_type_id")
for identifier in retired_identifiers:
    for path, source in (
        (REGISTRY, registry),
        (COMPLEX, complex_source),
        (VALUE_INFERENCE, value_inference),
    ):
        if re.search(r"\b{}\b".format(identifier), strip_comments_and_literals(source)):
            fail("retired constructor family selector '{}' remains in {}".format(identifier, path))

family_first_winner = (
    ("iter().find", r"\.iter\s*\(\s*\)(?:\s|\.)*\.find\s*\("),
    ("iter().find_map", r"\.iter\s*\(\s*\)(?:\s|\.)*\.find_map\s*\("),
    ("values().next", r"\.values\s*\(\s*\)(?:\s|\.)*\.next\s*\("),
    ("values()[0]", r"\.values\s*\(\s*\)\s*\[\s*0\s*\]"),
    ("same-base predicate", r"\bis_struct_of_base\s*\("),
)

exact_lookup = braced_function(context, "get_struct_type_id")
require("get_struct_type_id", exact_lookup, "self.struct_table.resolve(name)")
require("get_struct_type_id", exact_lookup, ".map(|(_, info)| info.type_id)")
forbid("get_struct_type_id", exact_lookup, family_first_winner)

registry_resolve = braced_function(struct_registry, "resolve")
require("StructRegistry::resolve", registry_resolve, "owner_module_path(name)")
require(
    "StructRegistry::resolve",
    registry_resolve,
    "return self.resolve_in_owner(owner_path, name)",
)

exact_return = braced_function(value_inference, "exact_constructor_return_type")
require(
    "exact_constructor_return_type",
    exact_return,
    "type_id.map(ValueType::Struct).unwrap_or(ValueType::Any)",
)

complete_params = braced_function(value_inference, "constructor_type_args_are_complete")
require(
    "constructor_type_args_are_complete",
    complete_params,
    "!type_args.is_empty() && type_args.iter().all(JuliaType::is_concrete)",
)

for owner in (
    "infer_value_parametric_struct_ctor",
    "infer_value_rational_ctor",
    "infer_value_instantiated_ctor",
):
    block = braced_function(value_inference, owner)
    require(owner, block, "exact_constructor_return_type(")
    forbid(owner, block, family_first_winner)

parametric_value = braced_function(value_inference, "infer_value_parametric_struct_ctor")
require(
    "infer_value_parametric_struct_ctor",
    parametric_value,
    "if !constructor_type_args_are_complete(&type_args) { return ValueType::Any; }",
)

instantiated_value = braced_function(value_inference, "infer_value_instantiated_ctor")
require(
    "infer_value_instantiated_ctor",
    instantiated_value,
    "constructor_type_expr_is_complete(arg, is_active_type_param)",
)
require(
    "infer_value_instantiated_ctor",
    instantiated_value,
    "if type_exprs.is_empty()",
)

complex_block = braced_function(complex_source, "tfunc_complex_contextual")
require("tfunc_complex_contextual", complex_block, "LatticeType::Top")
if "struct_type_id" in complex_block:
    fail("tfunc_complex_contextual must not invent a concrete identity for unknown arguments")
forbid("tfunc_complex_contextual", complex_block, family_first_winner)

complex_julia = braced_function(value_inference, "infer_julia_complex_call")
require("infer_julia_complex_call", complex_julia, "_ => JuliaType::Any")

julia_type = braced_function(julia_inference, "infer_julia_type")
if compact(julia_type).count(
    compact("constructor_type_args_are_complete(&type_args)")
) != 2:
    fail("infer_julia_type must guard both bare and module-qualified parametric constructor returns")
if compact(julia_type).count(
    compact("self.shared_ctx.infer_type_args(&resolved_name, &arg_types)")
) != 2:
    fail("infer_julia_type must infer both parametric constructor returns from the resolved owner")

expr_type = braced_function(expr_inference, "infer_expr_type")
require("infer_expr_type", expr_type, "get_struct_type_id(&qualified_struct_name)")
require(
    "infer_expr_type",
    expr_type,
    "expr_tfuncs::infer_value_parametric_struct_ctor("
    "&resolved_name, &mut inst, &arg_types,)",
)
module_lookup = re.search(
    r"let\s+qualified_struct_name\s*=.*?get_struct_type_id\s*\(\s*&qualified_struct_name\s*\)(.*?return\s+ValueType::Struct)",
    expr_type,
    re.DOTALL,
)
if module_lookup is None or ".or_else" in module_lookup.group(1):
    fail("infer_expr_type module-qualified constructor lookup must use only the exact owner")

core_conversion = braced_function(core_compiler, "julia_type_to_value_type_with_ctx")
require(
    "julia_type_to_value_type_with_ctx",
    core_conversion,
    "if let Some(info) = self.resolve_struct_info_scoped(name)",
)

compile_call = braced_function(call_compiler, "compile_call")
require(
    "compile_call",
    compile_call,
    "if let Some(qualified_constructor) = current_module_constructor",
)
require(
    "compile_call",
    compile_call,
    "if current_module_constructor.is_none()",
)
require(
    "compile_call",
    compile_call,
    "let visible_using_constructor = current_module_constructor.is_none()",
)
require(
    "compile_call",
    compile_call,
    "compile_runtime_datatype_value_call(qualified_constructor, args, kwargs, splat_mask, kwargs_splat_mask,)",
)
require("julia_type_to_value_type_with_ctx", core_conversion, "ValueType::Any")
forbid(
    "julia_type_to_value_type_with_ctx",
    core_conversion,
    (
        ("struct-table iteration", r"struct_table\s*\.\s*iter\s*\("),
        ("struct-table borrowed iteration", r"&\s*self\.shared_ctx\.struct_table"),
        ("bare-family fallback", r"resolve_struct_info_scoped\s*\(\s*base_name\s*\)"),
    ),
)

typed_array_eltype = braced_function(
    builtin_array, "heap_julia_type_array_element_type_resolved", raw=True
)
require(
    "heap_julia_type_array_element_type_resolved",
    typed_array_eltype,
    "if let Some(info) = self.resolve_struct_info_scoped(name)",
)
require(
    "heap_julia_type_array_element_type_resolved",
    typed_array_eltype,
    "get_struct_name(info.type_id)",
)
if re.search(r"resolve_struct_info_scoped\s*\(\s*base_name\s*\)", typed_array_eltype):
    fail(
        "heap_julia_type_array_element_type_resolved must not replace a complete "
        "typed-array element identity with its bare family"
    )

generic_dispatch = braced_function(dispatch, "compile_generic_dispatch_call")
require(
    "compile_generic_dispatch_call",
    generic_dispatch,
    "try_struct_field_count_default_ctor_fallback(method_table_name, args)",
)
require(
    "compile_generic_dispatch_call",
    generic_dispatch,
    "struct_table.get(method_table_name)",
)
forbid(
    "compile_generic_dispatch_call",
    generic_dispatch,
    (
        ("struct-table iteration", r"struct_table\s*\.\s*iter\s*\("),
        ("struct-table values", r"struct_table\s*\.\s*values\s*\("),
        ("same-base predicate", r"\bis_struct_of_base\s*\("),
        ("retired instantiation selector", r"\binstantiation_of\s*\("),
        (
            "bare-owner default fallback",
            r"try_struct_field_count_default_ctor_fallback\s*\(\s*function\s*,",
        ),
    ),
)

default_fallback = braced_function(dispatch, "try_struct_field_count_default_ctor_fallback")
require(
    "try_struct_field_count_default_ctor_fallback",
    default_fallback,
    "resolve_scoped(function, self.current_module_path.as_deref(), false)",
)
forbid(
    "try_struct_field_count_default_ctor_fallback",
    default_fallback,
    (
        ("struct-table iteration", r"struct_table\s*\.\s*iter\s*\("),
        ("short-name candidate scan", r"\bshort_matches\b"),
    ),
)

with INVENTORY.open(newline="", encoding="utf-8") as inventory_file:
    rows = list(csv.DictReader(inventory_file, delimiter="\t"))

expected_header = ["path", "owner", "classification", "authority", "issue"]
if rows and list(rows[0]) != expected_header:
    fail("constructor return identity inventory header/schema drifted")

expected_owners = {
    "get_struct_type_id",
    "StructRegistry::resolve",
    "StructIdLookup",
    "constructor_type_args_are_complete",
    "tfunc_complex_contextual",
    "infer_julia_complex_call",
    "infer_value_parametric_struct_ctor",
    "infer_value_rational_ctor",
    "infer_value_instantiated_ctor",
    "infer_julia_type",
    "infer_expr_type",
    "julia_type_to_value_type_with_ctx",
    "heap_julia_type_array_element_type_resolved",
    "compile_call",
    "compile_generic_dispatch_call",
    "try_struct_field_count_default_ctor_fallback",
}
actual_owners = [row["owner"] for row in rows]
if set(actual_owners) != expected_owners or len(actual_owners) != len(expected_owners):
    fail("constructor return identity inventory owner set drifted")
for row in rows:
    if "#11436" not in row["issue"].split("/"):
        fail("inventory owner '{}' is not linked to #11436".format(row["owner"]))

manifest = MANIFEST.read_text(encoding="utf-8")
if manifest.count('name = "dispatch_constructor_return_exact_or_any_11436"') != 1:
    fail("#11436 fixture manifest registration is missing or duplicated")
if manifest.count('file = "constructor_return_exact_or_any_11436.jl"') != 1:
    fail("#11436 fixture path registration is missing or duplicated")

if errors:
    for error in errors:
        print("ERROR: {} (Issue #11436)".format(error), file=sys.stderr)
    sys.exit(1)

print(
    "OK: constructor returns use exact complete identity or remain Any; "
    "owner and parameter misses remain dynamic (Issue #11436)"
)
PY
