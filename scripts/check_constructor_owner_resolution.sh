#!/usr/bin/env bash
# check_constructor_owner_resolution.sh — owner-preserving constructor lookup
# audit (Issues #11172, #11369, #11371, #11713, #11716, #11720, #11733, and
# #11737, and #11684).
#
# Constructor declarations carry owner-qualified identities, but a call-site
# fallback that strips the owner can probe a same-leaf table from Base or a
# sibling module. Keep every remaining compatibility projection explicit and
# pin the runtime DataType safeguards that make qualified and unique-bare
# lookup distinct. Phase #10992 owns deleting the inventoried legacy rows.

set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

python3 - <<'PY'
import csv
import pathlib
import re
import sys

CONSTRUCTORS = pathlib.Path("subset_julia_vm_compile/src/compile/expr/call/constructors.rs")
CALL_ROUTING = pathlib.Path("subset_julia_vm_compile/src/compile/expr/call/mod.rs")
MODULE_CALLS = pathlib.Path("subset_julia_vm_compile/src/compile/expr/call/module_call.rs")
DYNAMIC_CALLS = pathlib.Path("subset_julia_vm_compile/src/compile/expr/call/dynamic.rs")
COLLECTION_EXPRS = pathlib.Path("subset_julia_vm_compile/src/compile/expr/collection.rs")
CORE_COMPILER = pathlib.Path("subset_julia_vm_compile/src/compile/core_compiler.rs")
COMPILE_CONTEXT = pathlib.Path("subset_julia_vm_compile/src/compile/context.rs")
PIPELINE = pathlib.Path("subset_julia_vm_compile/src/compile/pipeline_ctx.rs")
COLLECT = pathlib.Path("subset_julia_vm_compile/src/compile/collect.rs")
CACHE = pathlib.Path("subset_julia_vm_compile/src/compile/cache.rs")
REPL_SESSION = pathlib.Path("subset_julia_vm/src/repl/session.rs")
LOADER = pathlib.Path("subset_julia_vm/src/loader.rs")
IR_CORE = pathlib.Path("subset_julia_vm_types/src/ir/core.rs")
LOWERING_CALLS = pathlib.Path("subset_julia_vm_lowering/src/lowering/expr/call.rs")
FREE_VARS = pathlib.Path("subset_julia_vm_types/src/ir/free_vars.rs")
RUNTIME = pathlib.Path("subset_julia_vm_vm/src/vm/exec/call_function_variable.rs")
INVENTORY = pathlib.Path("docs/vm/CONSTRUCTOR_OWNER_FALLBACK_INVENTORY.tsv")

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


def braced_function(source, name):
    masked = strip_comments_and_literals(source)
    match = re.search(r"\bfn\s+" + re.escape(name) + r"(?:\s*<[^>{}]*>)?\s*\(", masked)
    if match is None:
        fail(f"function owner '{name}' is missing; update the audit if it moved")
        return ""
    opening = masked.find("{", match.end())
    if opening < 0:
        fail(f"function owner '{name}' has no opening brace")
        return ""
    depth = 0
    for index in range(opening, len(masked)):
        if masked[index] == "{":
            depth += 1
        elif masked[index] == "}":
            depth -= 1
            if depth == 0:
                return masked[match.start() : index + 1]
    fail(f"function owner '{name}' has no closing brace")
    return ""


def braced_function_raw(source, name):
    masked = strip_comments_and_literals(source)
    match = re.search(r"\bfn\s+" + re.escape(name) + r"(?:\s*<[^>{}]*>)?\s*\(", masked)
    if match is None:
        fail(f"function owner '{name}' is missing; update the audit if it moved")
        return ""
    opening = masked.find("{", match.end())
    if opening < 0:
        fail(f"function owner '{name}' has no opening brace")
        return ""
    depth = 0
    for index in range(opening, len(masked)):
        if masked[index] == "{":
            depth += 1
        elif masked[index] == "}":
            depth -= 1
            if depth == 0:
                return source[match.start() : index + 1]
    fail(f"function owner '{name}' has no closing brace")
    return ""


def compact(text):
    return re.sub(r"\s+", "", text)


def require_all(owner, block, fragments):
    body = compact(block)
    for fragment in fragments:
        if compact(fragment) not in body:
            fail(f"{owner} lost required owner-resolution evidence '{fragment}'")


def require_before(owner, block, first, second):
    body = compact(block)
    first_pos = body.find(compact(first))
    second_pos = body.find(compact(second))
    if first_pos < 0 or second_pos < 0 or first_pos >= second_pos:
        fail(f"{owner} must keep '{first}' before '{second}'")


def require_ordered(owner, block, fragments):
    body = compact(block)
    cursor = 0
    for fragment in fragments:
        position = body.find(compact(fragment), cursor)
        if position < 0:
            fail(
                f"{owner} lost ordered owner-resolution evidence '{fragment}'"
            )
            return
        cursor = position + len(compact(fragment))


def require_count(owner, block, fragment, expected):
    actual = compact(block).count(compact(fragment))
    if actual != expected:
        fail(f"{owner} has {actual} occurrences of '{fragment}'; expected {expected}")


def require_optional_trailing_comma_call_count(owner, block, call, expected):
    actual = len(re.findall(re.escape(compact(call)) + r",?\)", compact(block)))
    if actual != expected:
        fail(f"{owner} has {actual} calls to '{call}'; expected {expected}")


def scanner_selftest():
    synthetic = '''
fn owner(x: &str) {
    // short_constructor_name(x)
    let inert = "short_constructor_name(x)";
    if x == "{" { nested(); }
    short_constructor_name(x);
}
'''
    block = braced_function(synthetic, "owner")
    if len(re.findall(r"\bshort_constructor_name\s*\(", block)) != 1:
        fail("scanner self-test did not ignore comment/string decoys")


scanner_selftest()

for path in (
    CONSTRUCTORS,
    CALL_ROUTING,
    MODULE_CALLS,
    DYNAMIC_CALLS,
    COLLECTION_EXPRS,
    CORE_COMPILER,
    COMPILE_CONTEXT,
    PIPELINE,
    COLLECT,
    CACHE,
    LOADER,
    IR_CORE,
    LOWERING_CALLS,
    FREE_VARS,
    RUNTIME,
    INVENTORY,
):
    if not path.is_file():
        fail(f"constructor owner audit target missing: {path}")

if errors:
    for error in errors:
        print(
            f"ERROR: {error} (Issues #11172/#11369/#11371/#11713/#11716)",
            file=sys.stderr,
        )
    sys.exit(1)

constructor_source = CONSTRUCTORS.read_text(encoding="utf-8")
call_routing_source = CALL_ROUTING.read_text(encoding="utf-8")
module_call_source = MODULE_CALLS.read_text(encoding="utf-8")
dynamic_call_source = DYNAMIC_CALLS.read_text(encoding="utf-8")
collection_expr_source = COLLECTION_EXPRS.read_text(encoding="utf-8")
core_compiler_source = CORE_COMPILER.read_text(encoding="utf-8")
compile_context_source = COMPILE_CONTEXT.read_text(encoding="utf-8")
pipeline_source = PIPELINE.read_text(encoding="utf-8")
collect_source = COLLECT.read_text(encoding="utf-8")
cache_source = CACHE.read_text(encoding="utf-8")
repl_session_source = REPL_SESSION.read_text(encoding="utf-8")
loader_source = LOADER.read_text(encoding="utf-8")
ir_core_source = IR_CORE.read_text(encoding="utf-8")
lowering_call_source = LOWERING_CALLS.read_text(encoding="utf-8")
free_vars_source = FREE_VARS.read_text(encoding="utf-8")
runtime_source = RUNTIME.read_text(encoding="utf-8")
constructor_masked = strip_comments_and_literals(constructor_source)

with INVENTORY.open(newline="", encoding="utf-8") as inventory_file:
    rows = list(csv.DictReader(inventory_file, delimiter="\t"))

expected_header = ["owner", "expected_calls", "classification", "authority", "issue"]
if rows and list(rows[0]) != expected_header:
    fail("constructor fallback inventory header/schema drifted")
if not rows:
    fail("constructor fallback inventory has no owner rows")

seen_owners = set()
expected_total = 0
for row in rows:
    owner = row["owner"]
    if owner in seen_owners:
        fail(f"duplicate constructor fallback inventory owner '{owner}'")
        continue
    seen_owners.add(owner)
    try:
        expected_calls = int(row["expected_calls"])
    except ValueError:
        fail(f"invalid expected_calls for '{owner}': {row['expected_calls']!r}")
        continue
    if expected_calls < 0 or not row["classification"] or not row["authority"] or not row["issue"]:
        fail(f"incomplete constructor fallback inventory row for '{owner}'")
        continue
    block = braced_function(constructor_source, owner)
    actual_calls = len(re.findall(r"\bshort_constructor_name\s*\(", block))
    if actual_calls != expected_calls:
        fail(
            f"{owner} has {actual_calls} short_constructor_name calls; "
            f"inventory allows {expected_calls}"
        )
    expected_total += expected_calls

all_calls = len(re.findall(r"\bshort_constructor_name\s*\(", constructor_masked))
if all_calls != expected_total + 1:
    fail(
        f"constructors.rs has {all_calls - 1} short_constructor_name call sites; "
        f"inventory owns {expected_total}"
    )

expected_owners = {
    "static_parametric_constructor_method",
    "parametric_constructor_method_table_names",
    "try_compile_struct_table_constructor_call",
    "try_compile_inferred_parametric_constructor_call",
}
if seen_owners != expected_owners:
    fail(
        "constructor fallback owner set drifted: expected "
        + ", ".join(sorted(expected_owners))
        + "; found "
        + ", ".join(sorted(seen_owners))
    )

require_all(
    "static_parametric_constructor_method",
    braced_function(constructor_source, "static_parametric_constructor_method"),
    (
        "resolve_parametric_struct_name(base_name)",
        "table.is_explicit_parametric_inner_constructor(method.global_index)",
        "constructor_method_owner_matches(",
        "resolved_base_name.as_deref()",
    ),
)
require_all(
    "parametric_constructor_method_table_names",
    braced_function(constructor_source, "parametric_constructor_method_table_names"),
    (
        "resolve_parametric_struct_name(base_name)",
        "resolved.contains() && self.method_tables.contains_key(resolved)",
        "base_name.contains() && self.method_tables.contains_key(base_name)",
    ),
)
require_all(
    "try_compile_struct_table_constructor_call",
    braced_function(constructor_source, "try_compile_struct_table_constructor_call"),
    (
        "resolve_struct_info_scoped(function)",
        "function.contains() && struct_info.has_inner_constructor",
        "compile_runtime_datatype_value_call(",
        "function.to_string()",
    ),
)
require_all(
    "try_compile_inferred_parametric_constructor_call",
    braced_function(constructor_source, "try_compile_inferred_parametric_constructor_call"),
    (
        "resolve_parametric_struct_name(function)",
        "parametric_structs.get(function).or_else(|| self.shared_ctx.parametric_structs.get(&resolved_name))",
        "!parametric.def.inner_constructors.is_empty()",
    ),
)

# Owner resolution must happen before splat/public-container early returns and
# remain exact through qualified module calls and parametric DataType emission.
require_all(
    "analyze_expr_free_vars",
    braced_function(free_vars_source, "analyze_expr_free_vars"),
    (
        "parse_parametric_call(function)",
        "let callee_binding =",
        "outer_scope_vars.contains(callee_binding.as_str())",
        "free_vars.insert(callee_binding)",
    ),
)
require_all(
    "owned_constructor_name_in_scope",
    braced_function(constructor_source, "owned_constructor_name_in_scope"),
    (
        "parse_parametric_call(function)",
        "locals.contains_key(&constructor_base)",
        "captured_vars.contains(&constructor_base)",
        "constructor_base.contains()",
        "struct_table.contains_key(&constructor_base)",
        "parametric_structs.contains_key(&constructor_base)",
        "current_module_path.as_deref()",
        "struct_table.contains_key(&qualified_base)",
        "parametric_structs.contains_key(&qualified_base)",
    ),
)
require_all(
    "try_compile_lexical_datatype_parametric_call",
    braced_function(constructor_source, "try_compile_lexical_datatype_parametric_call"),
    (
        "parse_parametric_call(function)",
        "locals.contains_key(&base_name)",
        "captured_vars.contains(&base_name)",
        "compile_lexical_datatype_parametric_call(",
    ),
)
require_all(
    "compile_call",
    braced_function(call_routing_source, "compile_call"),
    (
        "try_compile_lexical_datatype_parametric_call(",
        "if has_splat && !self.locals.contains_key(function) && !self.captured_vars.contains(function)",
        "if let Some(qualified_constructor) = self.owned_constructor_name_in_scope(function)",
        "compile_runtime_datatype_value_call(",
        "let current_module_constructor = self.owned_constructor_name_in_scope(function)",
        "else if let Some(result) = self.try_compile_public_collection_constructor_call(",
    ),
)
require_before(
    "compile_call",
    braced_function(call_routing_source, "compile_call"),
    "try_compile_lexical_datatype_parametric_call(",
    "return self.compile_splat_call(",
)
require_ordered(
    "compile_call",
    braced_function(call_routing_source, "compile_call"),
    (
        "runtime_nominal_binding_name(function)",
        "compile_runtime_datatype_value_call(",
        "return self.compile_splat_call(",
    ),
)
require_before(
    "compile_call",
    braced_function(call_routing_source, "compile_call"),
    "owned_constructor_name_in_scope(function)",
    "return self.compile_splat_call(",
)
require_all(
    "compile_lexical_datatype_parametric_call",
    braced_function(dynamic_call_source, "compile_lexical_datatype_parametric_call"),
    (
        "load_local(base_name)",
        "compile_datatype_parametric_apply_tail(",
    ),
)
# Issue #11426: the module-value variant loads the qualified module global
# (a value binding that shadowed an ignored conflicting import) as the
# apply_type base, then shares the same evaluation-order tail.
require_all(
    "compile_module_value_datatype_parametric_call",
    braced_function(dynamic_call_source, "compile_module_value_datatype_parametric_call"),
    (
        "Instr::LoadGlobalAny(qualified_base.to_string())",
        "compile_datatype_parametric_apply_tail(",
    ),
)
require_all(
    "compile_datatype_parametric_apply_tail",
    braced_function(dynamic_call_source, "compile_datatype_parametric_apply_tail"),
    (
        "Instr::ApplyTypeDynamic(type_args.len())",
        "Instr::StoreAny(callee_temp.clone())",
        "for arg in args",
        "Instr::LoadAny(callee_temp)",
        "Instr::CallFunctionVariableWithKwargsSplat(",
    ),
)
require_before(
    "compile_datatype_parametric_apply_tail",
    braced_function(dynamic_call_source, "compile_datatype_parametric_apply_tail"),
    "Instr::StoreAny(callee_temp.clone())",
    "for arg in args",
)
require_before(
    "compile_datatype_parametric_apply_tail",
    braced_function(dynamic_call_source, "compile_datatype_parametric_apply_tail"),
    "for arg in args",
    "Instr::LoadAny(callee_temp)",
)
require_all(
    "type_is_base_origin",
    braced_function(dynamic_call_source, "type_is_base_origin"),
    (
        "type_name.starts_with",
        "parametric_structs.get(leaf)",
        "base_parametric_structs.get(leaf)",
        "definition.def.is_base_origin",
    ),
)
require_all(
    "runtime_nominal_binding_name",
    braced_function(dynamic_call_source, "runtime_nominal_binding_name"),
    (
        "runtime_nominal_callable_names",
        "current_input_runtime_nominal_names",
        "if !is_current_input",
        "self.current_module_path.as_deref()?",
        'let qualified = format!',
        ".then_some(qualified)",
        "if self.type_is_base_origin(type_name)",
        "lexical_qualified",
        "lexical_qualified.filter(|qualified|",
        "runtime_nominal_callable_names.contains(qualified)",
        "!self.shared_ctx.struct_table.contains_key(qualified)",
        "!self.shared_ctx.parametric_structs.contains_key(qualified)",
    ),
)
require_before(
    "runtime_nominal_binding_name",
    braced_function(dynamic_call_source, "runtime_nominal_binding_name"),
    "runtime_nominal_callable_names.contains(qualified)",
    "runtime_nominal_callable_names.contains(type_name)",
)
require_ordered(
    "try_compile_enum_call",
    braced_function(call_routing_source, "try_compile_enum_call"),
    (
        "runtime_nominal_binding_name(function)",
        "Instr::ProbeRuntimeBinding(runtime_binding.clone())",
        "self.compile_expr(&args[0])",
        "Instr::ConstructEnum(enum_name)",
    ),
)
require_all(
    "collect_module_body_binding_names",
    braced_function(collect_source, "collect_module_body_binding_names"),
    (
        "Stmt::EnumDef",
        "RuntimeNominalDef::Enum(enum_def)",
        "enum_def.members.iter().map(|member| member.name.clone())",
    ),
)
require_all(
    "compile_main",
    pipeline_source,
    (
        "for module in &program.modules",
        "pre_optimization_runtime_nominal_names",
        "current_input_runtime_nominal_names",
        "self.pre_optimization_runtime_nominal_names.clone()",
    ),
)
require_all(
    "REPLSession current-input nominal provenance",
    repl_session_source,
    (
        "let current_input_type_names = repl_support::current_type_names(&program)",
        "&current_input_type_names",
    ),
)
require_all(
    "package type chronology provenance",
    loader_source + ir_core_source + pipeline_source,
    (
        "pub is_package_origin: bool",
        "module_value.mark_as_package_origin()",
        "module.is_base_origin || module.is_package_origin",
        "inherited_module_roots",
    ),
)
require_all(
    "current-source nominal origin boundary",
    ir_core_source + pipeline_source,
    (
        "pub is_base_origin: bool",
        "self.is_base_origin = true",
        "inherited_module_roots",
    ),
)
require_all(
    "current-main runtime nominal origin boundary",
    braced_function(pipeline_source, "compile_core_program_internal"),
    (
        ".position(is_base_user_main_boundary)",
        "collect_runtime_nominal_names_in_statements(",
        "current_main_stmts",
    ),
)
require_all(
    "inherited-module runtime nominal origin boundary",
    braced_function(collect_source, "collect_module_runtime_nominal_names"),
    (
        "module.is_base_origin || module.is_package_origin",
    ),
)
require_all(
    "current-input nominal provenance",
    cache_source + repl_session_source + pipeline_source,
    (
        "fn repl_current_type_names",
        "current_input_type_names: Option<&HashSet<String>>",
        "Some(current_input_type_names)",
        "repl_support::current_type_names(&program)",
        "&current_input_type_names",
        "self.current_input_type_names.as_ref()",
    ),
)
require_all(
    "compile_call",
    braced_function(call_routing_source, "compile_call"),
    (
        "call_span.start != call_span.end",
        "type_position.is_before(call_span.definition_order, call_span.start)",
    ),
)
require_all(
    "compile_dynamic_parametric_struct",
    braced_function(dynamic_call_source, "compile_dynamic_parametric_struct"),
    (
        "runtime_nominal_binding_name(&qualified_base_name)",
        "Instr::ProbeRuntimeBinding(runtime_binding)",
        "for type_arg in type_args",
        "Instr::StoreAny(type_arg_temp.clone())",
        "for arg in args",
        "Instr::LoadAny(type_arg_temp)",
        "Instr::NewDynamicParametricStruct(",
    ),
)
require_before(
    "compile_dynamic_parametric_struct",
    braced_function(dynamic_call_source, "compile_dynamic_parametric_struct"),
    "Instr::ProbeRuntimeBinding(runtime_binding)",
    "for type_arg in type_args",
)
require_ordered(
    "compile_dynamic_parametric_constructor_method_call",
    braced_function(dynamic_call_source, "compile_dynamic_parametric_constructor_method_call"),
    (
        "runtime_nominal_binding_name(&qualified_base_name)",
        "Instr::ProbeRuntimeBinding(runtime_binding)",
        "for type_arg in type_args",
        "for arg in args",
    ),
)
require_ordered(
    "try_compile_splat_parametric_constructor_call",
    braced_function(constructor_source, "try_compile_splat_parametric_constructor_call"),
    (
        ".or_else(|| self.runtime_nominal_binding_name(&base_name))",
        "runtime_nominal_binding_name(&resolved_base_name)",
        "Instr::ProbeRuntimeBinding(runtime_binding)",
        "for type_arg in &type_args",
        "for arg in args",
    ),
)
require_before(
    "compile_dynamic_parametric_struct",
    braced_function(dynamic_call_source, "compile_dynamic_parametric_struct"),
    "Instr::StoreAny(type_arg_temp.clone())",
    "for arg in args",
)
require_all(
    "compile_resolved_module_call",
    braced_function(module_call_source, "compile_resolved_module_call"),
    (
        "let qualified_constructor_base =",
        "struct_table.contains_key(&qualified_constructor_base)",
        "parametric_structs.contains_key(&qualified_constructor_base)",
        "module_owns_constructor && (has_splat || !kwargs.is_empty()",
        "compile_runtime_datatype_value_call(",
        "method_tables.get(&qualified_function).is_some_and(",
        "(!exact_constructor_matches)",
    ),
)
require_all(
    "split_base_parametric_call_target",
    braced_function_raw(lowering_call_source, "split_base_parametric_call_target"),
    (
        "let head = &name[..brace]",
        "head.rsplit_once('.')",
        'module != "Base"',
        'format!("{}{}", leaf, &name[brace..])',
    ),
)
require_all(
    "resolve_call_target",
    braced_function_raw(lowering_call_source, "resolve_call_target"),
    (
        "NodeKind::ParametrizedTypeExpression",
        "split_base_parametric_call_target(&name)",
        "ResolvedCallTarget::ModuleCall { module, function }",
        "ResolvedCallTarget::DirectCall { name }",
    ),
)
require_all(
    "resolve_parametric_struct_name",
    braced_function_raw(core_compiler_source, "resolve_parametric_struct_name"),
    (
        "name.contains('.') && self.shared_ctx.parametric_structs.contains_key(name)",
        'name.strip_prefix("Base.")',
    ),
)
require_before(
    "resolve_parametric_struct_name",
    braced_function_raw(core_compiler_source, "resolve_parametric_struct_name"),
    'if let Some(unqualified) = name.strip_prefix("Base.")',
    "name.contains('.') && self.shared_ctx.parametric_structs.contains_key(name)",
)
require_all(
    "emit_type_expr_value_for_array_alloc",
    braced_function_raw(collection_expr_source, "emit_type_expr_value_for_array_alloc"),
    (
        'base.strip_prefix("Base.")',
        "self.shared_ctx.base_parametric_structs.contains_key(name)",
        "if explicit_base_owner",
        "base.clone()",
        "resolve_parametric_struct_name(base)",
    ),
)
require_count(
    "build_struct_tables",
    braced_function_raw(pipeline_source, "build_struct_tables"),
    "module_path.is_none() && stored_def.is_base_origin",
    2,
)
require_count(
    "build_struct_tables",
    braced_function_raw(pipeline_source, "build_struct_tables"),
    "base_parametric_structs.insert(",
    2,
)
require_all(
    "restore_compile_context_from_program",
    braced_function_raw(cache_source, "restore_compile_context_from_program"),
    (
        "let mut base_parametric_structs = HashMap::new()",
        "if def.is_base_origin",
        "base_parametric_structs.insert(",
        "def.name.clone()",
    ),
)
require_all(
    "resolve_instantiation_with_type_expr",
    braced_function_raw(compile_context_source, "resolve_instantiation_with_type_expr"),
    (
        'let explicit_base_name = base_name.strip_prefix("Base.")',
        "let explicit_base_def =",
        "base.to_string()",
        "let parametric_def = explicit_base_def",
        "self.substitute_field_type(",
        "def.is_base_origin",
    ),
)
require_all(
    "substitute_field_type",
    braced_function_raw(compile_context_source, "substitute_field_type"),
    (
        "base_origin_owner: bool",
        "base_origin_owner && !base.contains('.')",
        'format!("Base.{}", base)',
        "self.base_parametric_structs.contains_key(base)",
        "owned_base.is_some() || self.parametric_structs.contains_key(resolved_base)",
        "resolve_instantiation_with_type_expr(resolved_base, &resolved_params)",
    ),
)
require_all(
    "try_compile_explicit_public_dict_constructor",
    braced_function_raw(call_routing_source, "try_compile_explicit_public_dict_constructor"),
    ('resolve_instantiation_with_type_expr("Base.Dict", &type_args)',),
)
require_all(
    "try_compile_explicit_public_set_constructor",
    braced_function_raw(call_routing_source, "try_compile_explicit_public_set_constructor"),
    ('resolve_instantiation_with_type_expr("Base.Set", &[type_arg])',),
)
require_ordered(
    "compile_call",
    braced_function_raw(call_routing_source, "compile_call"),
    (
        "type_definition_positions",
        "type_position.is_before(call_span.definition_order, call_span.start)",
        "Instr::ThrowUndefVarError",
        "let has_splat",
    ),
)
require_all(
    "compile_call",
    braced_function(call_routing_source, "compile_call"),
    (
        "resolve_visible_type_object_name(function)",
        "let static_root_is_later =",
        "type_definition_positions",
        "!position.is_before(call_span.definition_order, call_span.start)",
        "!self.shared_ctx.struct_table.contains_key(&static_binding)",
        "struct_table.contains_key(&static_binding)",
        "parametric_structs.contains_key(&static_binding)",
        "|| static_root_is_later",
    ),
)
require_ordered(
    "compile_resolved_module_call",
    braced_function_raw(module_call_source, "compile_resolved_module_call"),
    (
        "type_definition_positions",
        "type_position.is_before(call_span.definition_order, call_span.start)",
        "Instr::ThrowUndefVarError(constructor_base)",
        "Special handling for Base module",
    ),
)
require_ordered(
    "compile_runtime_datatype_value_call",
    braced_function_raw(module_call_source, "compile_runtime_datatype_value_call"),
    (
        "runtime_nominal_binding_name(&base_name)",
        "Instr::ProbeRuntimeBinding(runtime_binding)",
        "for type_arg in &type_args",
        "runtime_nominal_binding_name(&type_name)",
        'self.new_temp("runtime_constructor_callee")',
        "Instr::StoreAny(callee_temp.clone())",
    ),
)
require_before(
    "compile_runtime_datatype_value_call",
    braced_function(module_call_source, "compile_runtime_datatype_value_call"),
    "Instr::StoreAny(callee_temp.clone())",
    "for arg in args",
)
require_before(
    "compile_runtime_datatype_value_call",
    braced_function(module_call_source, "compile_runtime_datatype_value_call"),
    "for arg in args",
    "Instr::LoadAny(callee_temp)",
)
require_all(
    "compile_runtime_datatype_value_call",
    braced_function_raw(module_call_source, "compile_runtime_datatype_value_call"),
    (
        "parse_parametric_call(&type_name)",
        "resolve_instantiation_with_type_expr(&base_name, &type_args)",
        "runtime_nominal_binding_name(&base_name)",
        "compile_type_expr_as_value(type_arg)",
        "Instr::ConstructParametricType(base_name, type_args.len())",
        "runtime_nominal_binding_name(&type_name)",
        "Instr::ProbeRuntimeBinding(runtime_binding)",
        "Instr::PushDataType(type_name)",
        'self.new_temp("runtime_constructor_callee")',
        "Instr::StoreAny(callee_temp.clone())",
        "Instr::LoadAny(callee_temp)",
    ),
)
require_optional_trailing_comma_call_count(
    "execute_call_function_variable",
    braced_function(runtime_source, "execute_call_function_variable"),
    "try_construct_default_datatype(&func_name, &expanded_args",
    4,
)

# Runtime DataType/apply-type calls carry display strings until FunctionId lands,
# so pin the three safeguards that keep owner loss bounded and deterministic.
require_all(
    "collect_function_variable_candidates_into",
    braced_function(runtime_source, "collect_function_variable_candidates_into"),
    (
        "let exact_indices = self.get_function_indices_by_name(func_name)",
        "if func_name.contains() && !exact_indices.is_empty()",
        "return;",
    ),
)
require_all(
    "constructor_type_heads_match",
    braced_function(runtime_source, "constructor_type_heads_match"),
    (
        "if left.contains() || right.contains()",
        "left == right",
    ),
)
require_all(
    "function_is_inner_constructor_for_datatype",
    braced_function(runtime_source, "function_is_inner_constructor_for_datatype"),
    (
        "resolve_runtime_parametric_def(expected)",
        "extract_base_type(&canonical) == expected",
        "!def.inner_constructors.is_empty()",
    ),
)
require_all(
    "try_construct_default_datatype",
    braced_function(runtime_source, "try_construct_default_datatype"),
    (
        "find(|(_, def)| def.name == type_name)",
        "match (matches.next(), matches.next())",
        "(Some((type_id, def)), None)",
    ),
)
require_all(
    "execute_call_function_variable",
    braced_function(runtime_source, "execute_call_function_variable"),
    (
        "if candidates.is_empty()",
        "if let Value::DataType(_) = &func_val",
        "self.raise(VmError::MethodError(format!(",
        "arg_type_names.join(",
    ),
)

if errors:
    for error in errors:
        print(f"ERROR: {error} (Issues #11172/#11369/#11371)", file=sys.stderr)
    sys.exit(1)

print(
    "OK: constructor call-site fallbacks are inventoried and runtime DataType "
    "lookup preserves exact/unique owner guards and callee-first evaluation "
    "(Issues #11172, #11369, #11371, #11684, #11713, #11716, #11720, and #11733)."
)
PY
