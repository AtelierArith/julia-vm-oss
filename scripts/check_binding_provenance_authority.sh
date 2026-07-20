#!/usr/bin/env bash
# check_binding_provenance_authority.sh — typed binding provenance and
# owner-qualified declared-global runtime-key audit (Issue #11317).

set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

python3 - <<'PY'
import csv
import pathlib
import re
import sys

CORE = pathlib.Path("subset_julia_vm_types/src/ir/core.rs")
COMPILE_ROOT = pathlib.Path("subset_julia_vm_compile/src")
INVENTORY = pathlib.Path("docs/vm/BINDING_PROVENANCE_CONSUMERS.tsv")

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


def compact(text):
    return re.sub(r"\s+", "", text)


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


def function_source(path, owner):
    source = path.read_text(encoding="utf-8")
    masked = strip_comments_and_literals(source)
    match = re.search(r"\bfn\s+" + re.escape(owner) + r"(?:\s*<[^>{}]*>)?\s*\(", masked)
    if match is None:
        fail("provenance consumer owner '{}' is missing from {}".format(owner, path))
        return ""
    opening = masked.find("{", match.end())
    if opening < 0:
        fail("provenance consumer owner '{}::{}' has no body".format(path, owner))
        return ""
    body = braced_region(masked, opening)
    if not body:
        fail("provenance consumer owner '{}::{}' has an unclosed body".format(path, owner))
    return body


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


def require_tokens(label, body, tokens):
    packed = compact(body)
    for token in tokens:
        if compact(token) not in packed:
            fail("{} lost required provenance evidence '{}'".format(label, token))


for path in (CORE, COMPILE_ROOT, INVENTORY):
    if not path.exists():
        fail("binding provenance audit target is missing: {}".format(path))

if errors:
    for error in errors:
        print("ERROR: {} (Issue #11317)".format(error), file=sys.stderr)
    sys.exit(1)

core_masked = strip_comments_and_literals(CORE.read_text(encoding="utf-8"))
enum_match = re.search(r"\bpub\s+enum\s+LocalDeclKind\s*\{", core_masked)
if enum_match is None:
    fail("LocalDeclKind enum is missing")
else:
    enum_body = braced_region(core_masked, core_masked.find("{", enum_match.start()))
    variants = re.findall(r"(?m)^\s*([A-Z][A-Za-z0-9_]*)\s*(?:,|\{|\()", enum_body)
    if variants != ["Explicit", "CompilerEnclosing"]:
        fail(
            "LocalDeclKind variants changed from Explicit/CompilerEnclosing; "
            "classify the new provenance in every inventoried consumer"
        )

with INVENTORY.open(newline="", encoding="utf-8") as inventory_file:
    rows = list(csv.DictReader(inventory_file, delimiter="\t"))

expected_header = ["path", "owner", "classification", "authority", "issue"]
if rows and list(rows[0]) != expected_header:
    fail("binding provenance consumer inventory header/schema drifted")
if not rows:
    fail("binding provenance consumer inventory has no rows")

seen = set()
for row in rows:
    key = (row["path"], row["owner"])
    if key in seen:
        fail("duplicate binding provenance consumer '{}::{}'".format(*key))
        continue
    seen.add(key)
    if not row["classification"] or not row["authority"] or row["issue"] != "#11317":
        fail("incomplete binding provenance inventory row '{}::{}'".format(*key))
        continue
    path = pathlib.Path(row["path"])
    if not path.is_file():
        fail("binding provenance consumer file is missing: {}".format(path))
        continue
    body = function_source(path, row["owner"])
    require_tokens(
        "{}::{}".format(path, row["owner"]),
        body,
        (
            "Stmt::LocalDecl {",
            "kind",
            "LocalDeclKind::Explicit",
            "LocalDeclKind::CompilerEnclosing",
        ),
    )

# Any semantic pattern that consumes LocalDecl fields is a provenance consumer.
# Scan every workspace production `src/` tree and recognize match arms, `if let` /
# `let ... else`, and `matches!` forms, including an unqualified `LocalDecl`
# imported through `use Stmt::*`. The TSV is the authority: a newly discovered
# owner must be registered there, but adding a valid row needs no second edit to
# this script.
observed_consumers = set()
roots = sorted(path for path in pathlib.Path(".").glob("*/src") if path.is_dir())
local_decl_pattern = re.compile(
    r"(?:(?:Stmt|Self)::)?\bLocalDecl\s*\{([^{}]{0,300})\}"
)
for root in roots:
    for path in root.rglob("*.rs"):
        masked = strip_comments_and_literals(path.read_text(encoding="utf-8"))
        regions = function_regions(masked)
        for match in local_decl_pattern.finditer(masked):
            fields = compact(match.group(1))
            following = compact(masked[match.end() : match.end() + 80])
            preceding = compact(masked[max(0, match.start() - 160) : match.start()])
            is_pattern = (
                following.startswith("=>")
                or re.search(r"(?:if|while)?let(?:Stmt::|Self::)?$", preceding)
                or "matches!(" in preceding
            )
            if not is_pattern:
                # Struct construction creates provenance rather than consuming it.
                continue
            has_var = re.search(r"(?:^|,)var(?:[:,]|$)", fields)
            has_kind = re.search(r"(?:^|,)kind(?:[:,]|$)", fields)
            if not has_var and not has_kind:
                continue
            if has_var and not has_kind:
                fail(
                    "{}::{} consumes LocalDecl.var without LocalDecl.kind".format(
                        path, owner_at(regions, match.start())
                    )
                )
            observed_consumers.add((str(path), owner_at(regions, match.start())))

if observed_consumers != seen:
    fail(
        "unreviewed LocalDecl semantic consumer set: inventory {}, discovered {}".format(
            sorted(seen), sorted(observed_consumers)
        )
    )

# A whole-variant `LocalDecl { .. }` arm is allowed only for a reviewed
# non-semantic visitor. Ratchet every current occurrence by file and semantic
# function owner so a new ignored-provenance path cannot enter unnoticed,
# including outside the split compiler/VM/lowering crates.
expected_ignored_patterns = {
    ("subset_julia_vm/src/aot/call_graph.rs", "collect_calls_in_stmt"): 1,
    ("subset_julia_vm/src/macro_runtime.rs", "collect_called_functions_stmt"): 1,
    ("subset_julia_vm/src/macro_runtime.rs", "collect_referenced_modules_stmt"): 1,
    ("subset_julia_vm/src/repl/converters.rs", "collect_potential_rebindings"): 1,
    ("subset_julia_vm_compile/src/compile/abstract_interp/engine/mod.rs", "cfg_authoritative_straightline_stmt_supported"): 1,
    ("subset_julia_vm_compile/src/compile/cache.rs", "stmt_contains_function_def"): 1,
    ("subset_julia_vm_compile/src/compile/collect.rs", "collect_stmt_functions_with_new_authority"): 1,
    ("subset_julia_vm_compile/src/compile/collect.rs", "collect_module_body_let_functions"): 1,
    ("subset_julia_vm_compile/src/compile/complex_sroa.rs", "rewrite_stmt"): 1,
    ("subset_julia_vm_compile/src/compile/constants.rs", "stmt_contains_direct_throw"): 1,
    ("subset_julia_vm_compile/src/compile/expr/collection.rs", "stmt_has_nontransparent_filter_call"): 1,
    ("subset_julia_vm_compile/src/compile/ipo/call_graph.rs", "extract_calls_from_stmt"): 1,
    ("subset_julia_vm_compile/src/compile/pipeline_ctx.rs", "scan_stmt_opaque_runtime_eval"): 1,
    ("subset_julia_vm_compile/src/compile/pipeline_ctx.rs", "collect_let_scope_function_captures"): 1,
    ("subset_julia_vm_compile/src/compile/pipeline_ctx.rs", "collect_hard_scope_function_captures"): 1,
    ("subset_julia_vm_compile/src/compile/pipeline_ctx.rs", "collect_runtime_inner_constructor_structs_in_stmt"): 1,
    ("subset_julia_vm_compile/src/compile/ssa_ir/build.rs", "convert_stmt"): 1,
    ("subset_julia_vm_compile/src/compile/ssa_ir/scan.rs", "stmt_reads"): 1,
    ("subset_julia_vm_compile/src/compile/ssa_ir/scan.rs", "stmt_write_names"): 1,
    ("subset_julia_vm_compile/src/compile/ssa_ir/scan.rs", "collect_global_decls_stmt"): 1,
    ("subset_julia_vm_compile/src/compile/stmt.rs", "compile_stmt"): 1,
    ("subset_julia_vm_lowering/src/lowering/closure_box.rs", "collect_inline_funcs_stmt"): 1,
    ("subset_julia_vm_lowering/src/lowering/closure_box.rs", "stmt_unsafe_for_box"): 1,
    ("subset_julia_vm_lowering/src/lowering/closure_box.rs", "collect_function_refs_stmt"): 1,
    ("subset_julia_vm_lowering/src/lowering/closure_box.rs", "rewrite_reads_stmt"): 1,
    ("subset_julia_vm_types/src/ir/core.rs", "visit_definition_orders_mut"): 1,
    ("subset_julia_vm_types/src/ir/free_vars.rs", "collect_referenced_names_stmt"): 1,
    ("subset_julia_vm_vm/src/vm/builtins_reflection/mod.rs", "collect_call_arities_from_stmt"): 1,
    ("subset_julia_vm_vm/src/vm/specialize/helpers.rs", "stmt_variant_name"): 1,
}
ignored_pattern = re.compile(r"(?:Stmt|Self)::LocalDecl\s*\{\s*\.\.\s*\}")
observed_ignored_patterns = {}
for root in roots:
    for path in root.rglob("*.rs"):
        masked = strip_comments_and_literals(path.read_text(encoding="utf-8"))
        regions = function_regions(masked)
        for match in ignored_pattern.finditer(masked):
            key = (str(path), owner_at(regions, match.start()))
            observed_ignored_patterns[key] = observed_ignored_patterns.get(key, 0) + 1
if observed_ignored_patterns != expected_ignored_patterns:
    fail(
        "ignored LocalDecl provenance pattern inventory drifted; "
        "classify any new semantic consumer exhaustively: expected {}, found {}".format(
            sorted(expected_ignored_patterns.items()),
            sorted(observed_ignored_patterns.items()),
        )
    )

core_compiler = pathlib.Path("subset_julia_vm_compile/src/compile/core_compiler.rs")
runtime_name = function_source(core_compiler, "declared_global_runtime_name")
require_tokens(
    "declared_global_runtime_name",
    runtime_name,
    (
        "self.current_module_path",
        "Some(module_path) if !name.contains",
        "format!(",
    ),
)
load_authority = function_source(core_compiler, "emit_load_declared_global")
expected_load_authority = compact(
    "{ self.emit(Instr::LoadGlobalAny(self.declared_global_runtime_name(name),)); }"
)
if compact(load_authority) != expected_load_authority:
    fail("emit_load_declared_global body drifted from the exact key-authority expression")
store_authority = function_source(core_compiler, "emit_store_declared_global")
expected_store_authority = compact(
    "{ self.emit(Instr::StoreGlobalAny(self.declared_global_runtime_name(name),)); }"
)
if compact(store_authority) != expected_store_authority:
    fail("emit_store_declared_global body drifted from the exact key-authority expression")

declared_global_calls = {
    ("subset_julia_vm_compile/src/compile/expr/mod.rs", "load_local"): (
        "self.declared_globals.contains(name)",
        "emit_load_declared_global",
        "{self.emit_load_declared_global(name);returnOk(());}",
    ),
    ("subset_julia_vm_compile/src/compile/expr/mod.rs", "store_local"): (
        "self.declared_globals.contains(name)",
        "emit_store_declared_global",
        "{self.emit_store_declared_global(name);return;}",
    ),
    ("subset_julia_vm_compile/src/compile/stmt.rs", "store_module_alias_runtime_value"): (
        "self.declared_globals.contains(name)",
        "emit_store_declared_global",
        "{self.emit_store_declared_global(name);return;}",
    ),
    ("subset_julia_vm_compile/src/compile/stmt.rs", "compile_stmt"): (
        "self.declared_globals.contains(&func.name)",
        "emit_store_declared_global",
        "{self.emit_store_declared_global(&func.name);}",
    ),
}
for (path_text, owner), (condition, authority, expected_branch) in declared_global_calls.items():
    path = pathlib.Path(path_text)
    body = function_source(path, owner)
    require_tokens(
        "{}::{}".format(path, owner),
        body,
        (condition, "self.{}".format(authority)),
    )
    condition_match = re.search(re.escape(compact(condition)), compact(body))
    if condition_match is None:
        continue
    packed = compact(body)
    opening = packed.find("{", condition_match.end())
    branch = braced_region(packed, opening) if opening >= 0 else ""
    if branch != expected_branch:
        fail(
            "{}::{} declared-global branch drifted from its exact {} authority path".format(
                path, owner, authority
            )
        )

# Keep the reviewed caller inventory exact. A new declared-global path must be
# added above instead of silently emitting another frame-zero key.
compile_source = "\n".join(
    strip_comments_and_literals(path.read_text(encoding="utf-8"))
    for path in COMPILE_ROOT.rglob("*.rs")
)
load_calls = len(re.findall(r"\bself\.emit_load_declared_global\s*\(", compile_source))
store_calls = len(re.findall(r"\bself\.emit_store_declared_global\s*\(", compile_source))
if load_calls != 1 or store_calls != 3:
    fail(
        "declared-global authority caller inventory drifted: "
        "expected load=1/store=3, found load={}/store={}".format(load_calls, store_calls)
    )

# Any new declaration-state query is a potential new global-key emission path
# and must be reviewed into the authority/caller inventory above.
expected_declared_global_queries = {
    # Explicit lexical-owner activation must exclude a same-scope `global`
    # declaration before any opcode is routed (Issues #11569/#11317).
    "subset_julia_vm_compile/src/compile/core_compiler.rs": 3,
    "subset_julia_vm_compile/src/compile/expr/builtin.rs": 1,
    # Comprehension-assignment owner discovery removes explicit globals before
    # entering the hard lexical child scope (Issues #11569/#11317).
    "subset_julia_vm_compile/src/compile/expr/collection.rs": 1,
    "subset_julia_vm_compile/src/compile/expr/infer/julia_type.rs": 1,
    "subset_julia_vm_compile/src/compile/expr/infer/mod.rs": 1,
    "subset_julia_vm_compile/src/compile/expr/mod.rs": 4,
    "subset_julia_vm_compile/src/compile/pipeline_ctx.rs": 1,
    "subset_julia_vm_compile/src/compile/stmt.rs": 6,
}
observed_declared_global_queries = {}
for path in COMPILE_ROOT.rglob("*.rs"):
    count = len(
        re.findall(
            r"\b(?:self\.)?declared_globals\.contains\s*\(",
            strip_comments_and_literals(path.read_text(encoding="utf-8")),
        )
    )
    if count:
        observed_declared_global_queries[str(path)] = count
if observed_declared_global_queries != expected_declared_global_queries:
    fail(
        "declared-global query inventory drifted; review any new runtime-key path: "
        "expected {}, found {}".format(
            sorted(expected_declared_global_queries.items()),
            sorted(observed_declared_global_queries.items()),
        )
    )

# Inventory every raw frame-zero global opcode by semantic function owner and
# normalized key expression. This makes helper indirection and alternate string
# construction spellings fail closed: changing, adding, or moving an emitter
# requires explicit review here, even when the number of opcodes is unchanged.
expected_raw_global_emitters = {
    ("subset_julia_vm_compile/src/compile/cache.rs", "live_append_gate_rejects_function_bearing_opcodes_9199", "LoadGlobalAny", '"x".to_string()'): 1,
    # This is a read-only bytecode pattern in the live-append eligibility gate,
    # not a new key emitter. It rejects cached callers whose raw global load
    # names a nominal binding introduced by the current delta (Issue #11651).
    ("subset_julia_vm_compile/src/compile/cache.rs", "repl_relocatable_delta_compile", "LoadGlobalAny", "name"): 1,
    ("subset_julia_vm_compile/src/compile/core_compiler.rs", "emit_load_declared_global", "LoadGlobalAny", "self.declared_global_runtime_name(name),"): 1,
    ("subset_julia_vm_compile/src/compile/core_compiler.rs", "emit_store_declared_global", "StoreGlobalAny", "self.declared_global_runtime_name(name),"): 1,
    ("subset_julia_vm_compile/src/compile/stmt.rs", "store_module_alias_runtime_value", "StoreGlobalAny", 'format!("{module_path}.{name}")'): 1,
    ("subset_julia_vm_compile/src/compile/module_alias.rs", "emit_load_imported_binding", "LoadGlobalAny", "flag"): 1,
    ("subset_julia_vm_compile/src/compile/module_alias.rs", "emit_load_imported_binding", "LoadGlobalAny", "whole_module_flag"): 1,
    ("subset_julia_vm_compile/src/compile/module_alias.rs", "emit_load_imported_binding", "LoadGlobalAny", "self.runtime_imported_binding_source_storage(name),"): 3,
    ("subset_julia_vm_compile/src/compile/module_alias.rs", "emit_load_imported_binding", "LoadGlobalAny", "self.runtime_imported_binding_renamed_flag(name),"): 1,
    ("subset_julia_vm_compile/src/compile/module_alias.rs", "emit_load_imported_binding", "LoadGlobalAny", "self.runtime_module_alias_binding_name(name),"): 1,
    ("subset_julia_vm_compile/src/compile/module_alias.rs", "emit_runtime_imported_binding_state", "StoreGlobalAny", "self.runtime_module_alias_binding_name(name),"): 1,
    ("subset_julia_vm_compile/src/compile/module_alias.rs", "emit_runtime_imported_binding_state", "StoreGlobalAny", "self.runtime_imported_binding_source_storage(name),"): 2,
    ("subset_julia_vm_compile/src/compile/module_alias.rs", "emit_runtime_imported_binding_state", "StoreGlobalAny", "self.runtime_imported_binding_renamed_flag(name),"): 1,
    ("subset_julia_vm_compile/src/compile/module_alias.rs", "emit_runtime_imported_binding_state", "StoreGlobalAny", "self.runtime_imported_binding_whole_module_flag(name),"): 1,
    ("subset_julia_vm_compile/src/compile/module_alias.rs", "emit_runtime_imported_binding_state", "StoreGlobalAny", "self.runtime_module_alias_ambiguity_flag(name),"): 1,
    # compile_expr: two pre-existing qualified loads plus the Issue #11426
    # conflict-winning module value binding read (an ignored conflicting
    # import keeps the source-earlier module global authoritative).
    ("subset_julia_vm_compile/src/compile/expr/mod.rs", "compile_expr", "LoadGlobalAny", "qualified"): 3,
    ("subset_julia_vm_compile/src/compile/expr/mod.rs", "load_local", "LoadGlobalAny", "load_name"): 2,
    ("subset_julia_vm_compile/src/compile/expr/mod.rs", "store_local", "StoreGlobalAny", "store_name"): 2,
    ("subset_julia_vm_compile/src/compile/expr/call/module_call.rs", "compile_module_global_value_call", "LoadGlobalAny", "qualified_name"): 1,
    ("subset_julia_vm_compile/src/compile/expr/call/module_call.rs", "compile_resolved_module_function_ref", "LoadGlobalAny", 'format!( "{}.{}", resolved_module, function )'): 2,
    ("subset_julia_vm_compile/src/compile/expr/call/mod.rs", "try_compile_imported_module_global_call", "LoadGlobalAny", "qualified"): 1,
    ("subset_julia_vm_compile/src/compile/expr/call/module_value_call.rs", "emit_module_value", "LoadGlobalAny", "module_name.to_string()"): 1,
    ("subset_julia_vm_compile/src/compile/expr/call/dynamic.rs", "compile_function_variable_call_with_kwargs", "LoadGlobalAny", 'format!( "{}.{}", module_path, var_name )'): 1,
    # Issue #11426: parametric application on a module value binding that
    # shadowed an ignored conflicting import loads the qualified module
    # global as the runtime apply_type base.
    ("subset_julia_vm_compile/src/compile/expr/call/dynamic.rs", "compile_module_value_datatype_parametric_call", "LoadGlobalAny", "qualified_base.to_string()"): 1,
}
observed_raw_global_emitters = {}
raw_global_pattern = re.compile(r"Instr::(LoadGlobalAny|StoreGlobalAny)\s*\(")
for path in COMPILE_ROOT.rglob("*.rs"):
    source = path.read_text(encoding="utf-8")
    masked = strip_comments_and_literals(source)
    regions = function_regions(masked)
    for match in raw_global_pattern.finditer(masked):
        opening = masked.find("(", match.start())
        depth = 0
        closing = None
        for index in range(opening, len(masked)):
            if masked[index] == "(":
                depth += 1
            elif masked[index] == ")":
                depth -= 1
                if depth == 0:
                    closing = index
                    break
        if closing is None:
            fail("{} has an unclosed {} opcode".format(path, match.group(1)))
            continue
        argument = " ".join(source[opening + 1 : closing].split())
        key = (str(path), owner_at(regions, match.start()), match.group(1), argument)
        observed_raw_global_emitters[key] = observed_raw_global_emitters.get(key, 0) + 1
if observed_raw_global_emitters != expected_raw_global_emitters:
    fail(
        "raw global opcode owner/key inventory drifted; expected {}, found {}".format(
            sorted(expected_raw_global_emitters.items()),
            sorted(observed_raw_global_emitters.items()),
        )
    )

if errors:
    for error in errors:
        print("ERROR: {} (Issue #11317)".format(error), file=sys.stderr)
    sys.exit(1)

print(
    "OK: {} LocalDecl provenance consumers are exhaustive and all declared-global "
    "runtime loads/stores use the owner-qualified authority (Issue #11317).".format(
        len(seen)
    )
)
PY
