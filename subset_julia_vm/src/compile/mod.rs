//! Compiler for ir/core::Program with multiple dispatch support.
//!
//! This module compiles the new Core IR (from tree-sitter lowering) to bytecode,
//! supporting type-annotated function parameters and multiple dispatch.
//!
//! # Module Organization
//!
//! - `base_functions.rs`: Base function classification and builtin operation mapping
//! - `base_merge.rs`: Base program merging logic
//! - `collect.rs`: Collection and resolution helpers for the compilation driver
//! - `constants.rs`: Math constants and helper functions
//! - `context.rs`: Compilation context
//! - `core_compiler.rs`: CoreCompiler struct, LoopContext, FinallyContext, type predicates
//! - `free_vars.rs`: Free variable analysis for closure capture detection
//! - `inference.rs`: Type inference
//! - `types.rs`: Type definitions and error handling
//! - `utils.rs`: Binary op conversion, literal evaluation, and other utilities
//! - `stmt.rs`: Statement compilation
//! - `expr/`: Expression compilation

pub mod abstract_interp;
mod base_functions;
mod base_merge;
pub mod bridge;
pub mod cache;
pub mod cfg;
mod collect;
pub mod const_prop;
mod constants;
mod context;
mod core_compiler;
pub mod diagnostics;
pub mod effects;
pub(crate) mod embedded_cache;
mod expr;
mod free_vars;
pub mod infer_metrics;
mod inference;
pub mod inference_trace;
pub mod ipo;
mod ir_inline;
mod ir_opt;
pub mod lattice;
mod method_table;
mod narrowing;
pub(crate) mod peephole;
mod pipeline_ctx;
pub mod precompile;
pub(crate) mod profile;
pub mod promotion;
mod sorted_serde;
mod stmt;
pub mod tfuncs;
mod type_helpers;
pub mod type_stability;
mod types;
pub mod union_split;
mod utils;

#[cfg(test)]
pub(crate) mod test_helpers;

use crate::compile::abstract_interp::engine::{CachedReturn, InferenceCacheKey};
use base_functions::{
    base_function_to_builtin_op, extract_module_path_from_expr, is_base_function,
    is_base_submodule_function, is_method_dispatch_first_base_function, is_random_function,
    is_reducible_nary_operator,
};
use base_merge::merge_with_precompiled_base;
pub use cache::{compile_with_cache, compile_with_cache_with_globals};
pub(crate) use constants::float_special_constant_value;
// Issue #8192: the runtime arg-type specializer (`vm::specialize`) resolves its
// typed binary-op instructions through the same shared table as the main
// compiler, so the two codegen paths can no longer drift apart.
pub use constants::needs_specialization;
use constants::{
    get_base_exported_constant_value, get_math_constant_value, is_euler_name, is_math_constant,
    is_pi_name, is_stdlib_module, needs_reflection_registration,
};
use context::SharedCompileContext;
pub use context::{infer_parametric_type_args, EnumInfo, StructInfo};
use core_compiler::{
    is_float_type, is_integer_type, is_numeric_type, is_singleton_type,
    static_assignment_types_compatible, CoreCompiler, FinallyContext, LoopContext,
    ResolvedUsingImport,
};
pub(crate) use expr::binary::typed_scalar_binary_instr;
pub(crate) use free_vars::{
    analyze_free_variables, collect_block_local_bindings, collect_referenced_names,
};
pub(crate) use inference::{build_shared_inference_engine, build_shared_inference_engine_owned};
use inference::{
    collect_global_types_for_inference, collect_local_binding_names_for_capture,
    collect_local_types_with_mixed_tracking, widen_non_const_globals_for_binding_inference,
};
pub(crate) use method_table::{MethodSig, MethodTable};
pub(crate) use pipeline_ctx::compile_core_program_internal;
use type_helpers::{
    check_type_satisfies_bound, is_builtin_type_name, julia_type_to_value_type,
    julia_type_to_value_type_with_table,
};
pub use types::{err, CResult, CompileError, InstantiationKey, ParametricStructDef};
use types::{parse_parametric_call, parse_type_args_recursive};

use collect::{
    collect_block_functions, collect_from_module, collect_module_functions, collect_module_info,
    collect_module_structs, collect_module_usings, collect_module_usings_recursive,
    collect_stmt_functions, collect_struct_literal_types, qualify_type_for_module,
    resolve_abstract_type, resolve_type_alias,
};
pub(super) use utils::{binary_op_to_function_name, function_name_to_binary_op};
use utils::{eval_literal_default, infer_default_type, is_required_kwarg, relocate_jumps};

use crate::ir::core::{
    Block, BuiltinOp, Expr, Function, KwParam, Program, Stmt, UsingImport,
    BASE_USER_MAIN_BOUNDARY_META,
};
use crate::types::{JuliaType, StructHierarchy, TypeExpr, TypeParam};
use crate::vm::slot::{build_slot_info, slotize_code};
use crate::vm::value::{ArrayElementType, Value};
use crate::vm::{
    AbstractTypeDefInfo, CompiledProgram, FunctionInfo, Instr, KwParamInfo, PrimitiveTypeDefInfo,
    RuntimeCompileContext, ShowMethodEntry, SpecializableFunction, StructDefInfo, ValueType,
};
use std::collections::{HashMap, HashSet};

// MergedProgram and merge_with_precompiled_base are now in base_merge.rs module
// Helper functions are now in base_functions.rs module

/// Representative reflection metadata derived from the leading `Stmt::Meta`
/// markers retained at the top of a function body. Mirrors the public
/// `Method` / `Core.CodeInfo` fields that user code inspects via `methods`,
/// `code_lowered`, and `code_typed`.
#[derive(Debug, Default, Clone, Copy)]
struct FunctionReflectionMeta {
    /// 0 = default, 1 = `@inline` / `@propagate_inbounds`, 2 = `@noinline`
    /// (mirrors `CodeInfo.inlining`, Issues #4977/#4980).
    inlining: u8,
    /// 0 = default, 1 = `Base.@constprop :aggressive`, 2 = `Base.@constprop
    /// :none` (mirrors `Method.constprop` / `CodeInfo.constprop`,
    /// Issues #4978/#4981).
    constprop: u8,
    /// `@nospecialize` bitmask over explicit positional parameters
    /// (mirrors `Method.nospecialize`, Issue #4984).
    nospecialize: i32,
    /// `Base.@propagate_inbounds` (mirrors `CodeInfo.propagate_inbounds`,
    /// Issue #4979).
    propagate_inbounds: bool,
    /// `Base.@nospecializeinfer` (mirrors `CodeInfo.nospecializeinfer`,
    /// Issue #4979).
    nospecializeinfer: bool,
    /// `Base.@assume_effects` purity bitmask (mirrors `CodeInfo.purity`,
    /// Issue #4983).
    purity: u16,
    /// Internal marker retained for `@generated` compatibility fallback bodies
    /// so VM dispatch can apply the staged Expr tuple cache (Issue #5936).
    is_generated: bool,
}

/// Map a single `Base.@assume_effects` setting name (with or without a leading
/// `:`) to its `encode_effects_override` bitmask, matching upstream Julia 1.12.
///
/// Composite settings expand to the OR of their constituent bits exactly as the
/// upstream macro does (`:foldable == 1163`, `:removable == 14`,
/// `:total == 1263`).
fn assume_effects_purity_bits(setting: &str) -> u16 {
    match setting.trim_start_matches(':') {
        "consistent" => 1,
        "effect_free" => 2,
        "nothrow" => 4,
        "terminates_globally" => 8,
        "terminates_locally" => 16,
        "notaskstate" => 32,
        "inaccessiblememonly" => 64,
        "noub" => 128,
        // consistent | effect_free | terminates_globally | noub | nortcall
        "foldable" => 1163,
        // effect_free | nothrow | terminates_globally
        "removable" => 14,
        // foldable | nothrow | notaskstate | inaccessiblememonly
        "total" => 1263,
        _ => 0,
    }
}

/// Recursively collect the length value parameters of an `NTuple{LEN, ELEM}`
/// type name, descending into nested `NTuple` element types so that patterns
/// like `NTuple{N, NTuple{M, T}}` mark both `N` and `M` as value parameters
/// (Issue #4842). A length argument is recorded only when it is a where-clause
/// type parameter of the enclosing function.
fn collect_ntuple_value_params(
    type_name: &str,
    func_type_param_names: &HashSet<&str>,
    val_type_params: &mut HashSet<String>,
) {
    if !(type_name.starts_with("NTuple{") && type_name.ends_with("}")) {
        return;
    }
    let inner = &type_name[7..type_name.len() - 1];
    // Split on the first top-level comma so a nested `NTuple{M,T}` stays intact.
    let mut depth = 0usize;
    let mut split_at = None;
    for (i, ch) in inner.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                split_at = Some(i);
                break;
            }
            _ => {}
        }
    }
    let Some(split_at) = split_at else {
        return;
    };
    let len_arg = inner[..split_at].trim();
    let elem_arg = inner[split_at + 1..].trim();
    if func_type_param_names.contains(len_arg) {
        val_type_params.insert(len_arg.to_string());
    }
    collect_ntuple_value_params(elem_arg, func_type_param_names, val_type_params);
}

/// Collect rank value parameters from `Array{T,N}`-style signatures. The second
/// array type argument is a value parameter, so function bodies must read it
/// like `Val{N}`/`NTuple{N,T}` rather than as a DataType (Issue #6210).
fn collect_array_rank_value_params(
    type_name: &str,
    func_type_param_names: &HashSet<&str>,
    val_type_params: &mut HashSet<String>,
) {
    let base = type_name
        .find('{')
        .map_or(type_name, |brace_idx| &type_name[..brace_idx]);
    let base = base.rsplit('.').next().unwrap_or(base);
    if !matches!(base, "Array" | "AbstractArray") {
        return;
    }

    let params = crate::vm::util::parse_parametric_params(type_name);
    let Some(rank_arg) = params.get(1).map(|arg| arg.trim()) else {
        return;
    };
    if func_type_param_names.contains(rank_arg) {
        val_type_params.insert(rank_arg.to_string());
    }
}

/// Scan the leading `Stmt::Meta` markers retained at the top of a function
/// body and derive upstream-compatible representative reflection metadata.
fn function_reflection_meta(func: &Function) -> FunctionReflectionMeta {
    let mut meta = FunctionReflectionMeta::default();
    for stmt in &func.body.stmts {
        let Stmt::Meta { annotation, .. } = stmt else {
            // Meta markers are inserted at the very top of the body; stop at
            // the first non-meta statement to avoid scanning the whole body.
            break;
        };
        match annotation.name.as_str() {
            "generated" => meta.is_generated = true,
            "inline" => meta.inlining = 1,
            "propagate_inbounds" => {
                meta.inlining = 1;
                meta.propagate_inbounds = true;
            }
            "noinline" => meta.inlining = 2,
            "nospecializeinfer" => meta.nospecializeinfer = true,
            "constprop" => {
                let setting = annotation.args.first().map(String::as_str);
                match setting {
                    Some(s) if s.contains("aggressive") => meta.constprop = 1,
                    Some(s) if s.contains("none") => meta.constprop = 2,
                    _ => {}
                }
            }
            "assume_effects" => {
                for arg in &annotation.args {
                    meta.purity |= assume_effects_purity_bits(arg);
                }
            }
            // Statement-position `@nospecialize a b` sets the bit for each named
            // explicit positional parameter; `@specialize` (no args) clears the
            // accumulated mask (Issue #4984).
            "nospecialize" => {
                for name in &annotation.args {
                    if let Some(pos) = func.params.iter().position(|p| p.name == *name) {
                        if pos < i32::BITS as usize {
                            meta.nospecialize |= 1i32 << pos;
                        }
                    }
                }
            }
            "specialize" => {
                if annotation.args.is_empty() {
                    meta.nospecialize = 0;
                } else {
                    for name in &annotation.args {
                        if let Some(pos) = func.params.iter().position(|p| p.name == *name) {
                            if pos < i32::BITS as usize {
                                meta.nospecialize &= !(1i32 << pos);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    meta
}

fn is_base_user_main_boundary(stmt: &Stmt) -> bool {
    matches!(
        stmt,
        Stmt::Meta { annotation, .. } if annotation.name == BASE_USER_MAIN_BOUNDARY_META
    )
}

fn build_struct_hierarchy_from_context(shared_ctx: &SharedCompileContext) -> StructHierarchy {
    let mut hierarchy = StructHierarchy::new();

    for def in &shared_ctx.struct_defs {
        hierarchy.insert(&def.name, def.parent_type.clone(), Vec::new());
    }

    for (name, ps) in &shared_ctx.parametric_structs {
        let type_params = ps
            .def
            .type_params
            .iter()
            .map(|param| param.name.clone())
            .collect();
        hierarchy.insert_if_absent(name, ps.def.parent_type.clone(), type_params);
    }

    for at in &shared_ctx.abstract_types {
        hierarchy.insert_if_absent(&at.name, at.parent.clone(), at.type_params.clone());
    }

    hierarchy
}

fn collect_assigned_binding_names(stmts: &[Stmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { var, .. } | Stmt::AddAssign { var, .. } => {
                out.insert(var.clone());
            }
            Stmt::DestructuringAssign { targets, .. } => {
                out.extend(targets.iter().cloned());
            }
            Stmt::Block(block)
            | Stmt::Timed { body: block, .. }
            | Stmt::TestSet { body: block, .. } => {
                collect_assigned_binding_names(&block.stmts, out);
            }
            _ => {}
        }
    }
}

fn collect_testset_local_binding_names_for_capture(stmts: &[Stmt], names: &mut HashSet<String>) {
    collect_testset_local_binding_names_from_stmts(stmts, names);
}

fn collect_testset_local_binding_names_from_stmts(stmts: &[Stmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Expr { expr, .. } => collect_testset_local_binding_names_from_expr(expr, out),
            Stmt::TestSet { body, .. } => {
                collect_testset_scope_assigned_binding_names(&body.stmts, out);
            }
            // A variable bound to an empty-binding `let` block — e.g.
            // `#result# = let … end`, how `@time` / `@elapsed` capture their
            // body's value — is a transparent capture scope (the `let` introduces
            // no bindings of its own). Collect the body's binding names so a
            // closure defined inside it is recognized as capturing them; without
            // this a `@time`-block-local capture fails to compile with "Undefined
            // variable" (Issue #6288).
            Stmt::Assign {
                value: Expr::LetBlock { bindings, body, .. },
                ..
            } if bindings.is_empty() => {
                collect_testset_scope_assigned_binding_names(&body.stmts, out);
            }
            Stmt::Block(block) | Stmt::Timed { body: block, .. } => {
                collect_testset_local_binding_names_from_stmts(&block.stmts, out);
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_testset_local_binding_names_from_stmts(&then_branch.stmts, out);
                if let Some(block) = else_branch {
                    collect_testset_local_binding_names_from_stmts(&block.stmts, out);
                }
            }
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                collect_testset_local_binding_names_from_stmts(&try_block.stmts, out);
                for block in [catch_block, else_block, finally_block]
                    .into_iter()
                    .flatten()
                {
                    collect_testset_local_binding_names_from_stmts(&block.stmts, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_testset_local_binding_names_from_expr(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::LetBlock { body, .. } => {
            if block_opens_testset_scope(body) {
                // Issue #6261: lambda capture pre-analysis needs the names
                // available inside a macro-expanded @testset, but #6256 must
                // not leak their concrete types into the outer pre-scan map.
                collect_testset_scope_assigned_binding_names(&body.stmts, out);
            } else {
                collect_testset_local_binding_names_from_stmts(&body.stmts, out);
            }
        }
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_testset_local_binding_names_from_expr(arg, out);
            }
            for (_, value) in kwargs {
                collect_testset_local_binding_names_from_expr(value, out);
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_testset_local_binding_names_from_expr(left, out);
            collect_testset_local_binding_names_from_expr(right, out);
        }
        Expr::UnaryOp { operand, .. } => {
            collect_testset_local_binding_names_from_expr(operand, out);
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_testset_local_binding_names_from_expr(condition, out);
            collect_testset_local_binding_names_from_expr(then_expr, out);
            collect_testset_local_binding_names_from_expr(else_expr, out);
        }
        Expr::TupleLiteral { elements, .. } | Expr::ArrayLiteral { elements, .. } => {
            for elem in elements {
                collect_testset_local_binding_names_from_expr(elem, out);
            }
        }
        _ => {}
    }
}

fn collect_testset_scope_assigned_binding_names(stmts: &[Stmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { var, value, .. } | Stmt::AddAssign { var, value, .. } => {
                out.insert(var.clone());
                // Descend into the value too: `@time` / `@elapsed` bury their
                // body's bindings inside `#result# = let … end` (an empty-binding
                // `let` block as an assignment value), so a closure inside that
                // body is only recognized as capturing them if we look there as
                // well (Issue #6288).
                collect_testset_scope_assigned_binding_names_from_expr(value, out);
            }
            Stmt::DestructuringAssign { targets, .. } => {
                out.extend(targets.iter().cloned());
            }
            Stmt::Expr { expr, .. } => {
                collect_testset_scope_assigned_binding_names_from_expr(expr, out);
            }
            Stmt::Block(block)
            | Stmt::Timed { body: block, .. }
            | Stmt::TestSet { body: block, .. }
            | Stmt::For { body: block, .. }
            | Stmt::ForEach { body: block, .. }
            | Stmt::ForEachTuple { body: block, .. }
            | Stmt::While { body: block, .. } => {
                collect_testset_scope_assigned_binding_names(&block.stmts, out);
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_testset_scope_assigned_binding_names(&then_branch.stmts, out);
                if let Some(block) = else_branch {
                    collect_testset_scope_assigned_binding_names(&block.stmts, out);
                }
            }
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                collect_testset_scope_assigned_binding_names(&try_block.stmts, out);
                for block in [catch_block, else_block, finally_block]
                    .into_iter()
                    .flatten()
                {
                    collect_testset_scope_assigned_binding_names(&block.stmts, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_testset_scope_assigned_binding_names_from_expr(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::LetBlock { body, .. } => {
            collect_testset_scope_assigned_binding_names(&body.stmts, out);
        }
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_testset_scope_assigned_binding_names_from_expr(arg, out);
            }
            for (_, value) in kwargs {
                collect_testset_scope_assigned_binding_names_from_expr(value, out);
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_testset_scope_assigned_binding_names_from_expr(left, out);
            collect_testset_scope_assigned_binding_names_from_expr(right, out);
        }
        Expr::UnaryOp { operand, .. } => {
            collect_testset_scope_assigned_binding_names_from_expr(operand, out);
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_testset_scope_assigned_binding_names_from_expr(condition, out);
            collect_testset_scope_assigned_binding_names_from_expr(then_expr, out);
            collect_testset_scope_assigned_binding_names_from_expr(else_expr, out);
        }
        Expr::TupleLiteral { elements, .. } | Expr::ArrayLiteral { elements, .. } => {
            for elem in elements {
                collect_testset_scope_assigned_binding_names_from_expr(elem, out);
            }
        }
        _ => {}
    }
}

fn block_opens_testset_scope(block: &Block) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        Stmt::Expr { expr, .. } => expr_opens_testset_scope(expr),
        _ => false,
    })
}

fn expr_opens_testset_scope(expr: &Expr) -> bool {
    match expr {
        Expr::Builtin {
            name: BuiltinOp::TestSetBegin,
            ..
        } => true,
        Expr::Call { function, .. } => function == "_testset_begin!",
        Expr::LetBlock { body, .. } => block_opens_testset_scope(body),
        _ => false,
    }
}

fn type_parameter_return_snapshot(func: &Function) -> Option<JuliaType> {
    let returned_name = directly_returned_var(func)?;

    if !func
        .type_params
        .iter()
        .any(|type_param| type_param.name == *returned_name)
    {
        return None;
    }

    let appears_as_type_param = func.params.iter().any(|param| {
        param
            .type_annotation
            .as_ref()
            .is_some_and(|ty| julia_type_contains_direct_typevar(ty, returned_name))
    });

    appears_as_type_param
        .then(|| JuliaType::TypeOf(Box::new(JuliaType::TypeVar(returned_name.clone(), None))))
}

fn collect_callable_typeof_aliases(
    stmts: &[Stmt],
    all_functions: &[(&Function, Option<String>)],
) -> HashMap<String, String> {
    let callable_names: HashSet<String> = all_functions
        .iter()
        .map(|(func, _)| func.name.clone())
        .collect();
    let mut callable_bindings: HashMap<String, String> = callable_names
        .iter()
        .map(|name| (name.clone(), name.clone()))
        .collect();
    let mut typeof_aliases = HashMap::new();

    for stmt in stmts {
        let Stmt::Assign { var, value, .. } = stmt else {
            continue;
        };

        if let Some(callable_name) = typeof_callable_target(value, &callable_bindings) {
            typeof_aliases.insert(var.clone(), callable_name);
            continue;
        }

        if let Some(callable_name) = callable_binding_target(value, &callable_bindings) {
            callable_bindings.insert(var.clone(), callable_name);
        }
    }

    typeof_aliases
}

fn callable_binding_target(
    expr: &Expr,
    callable_bindings: &HashMap<String, String>,
) -> Option<String> {
    match expr {
        Expr::FunctionRef { name, .. } => callable_bindings.get(name).cloned(),
        Expr::Var(name, _) => callable_bindings.get(name).cloned(),
        _ => None,
    }
}

fn typeof_callable_target(
    expr: &Expr,
    callable_bindings: &HashMap<String, String>,
) -> Option<String> {
    match expr {
        Expr::Builtin {
            name: BuiltinOp::TypeOf,
            args,
            ..
        } => args
            .first()
            .and_then(|arg| callable_binding_target(arg, callable_bindings)),
        Expr::Call { function, args, .. } if function == "typeof" => args
            .first()
            .and_then(|arg| callable_binding_target(arg, callable_bindings)),
        _ => None,
    }
}

fn add_callable_typeof_method_table_aliases(
    func_name: &str,
    callable_typeof_aliases: &HashMap<String, String>,
    table_names: &mut Vec<String>,
) {
    let Some(type_alias) = func_name.strip_prefix("__callable_") else {
        return;
    };
    let Some(callable_name) = callable_typeof_aliases.get(type_alias) else {
        return;
    };

    // Issue #4309: Julia lets methods added to `typeof(f)` participate in calls
    // to the function object `f`. sjulia lowers `(::Alias)(...)` as
    // `__callable_Alias`, so static `Alias = typeof(f)` bindings need an iOS-safe
    // compile-time bridge into `f`'s method table.
    if !table_names.contains(callable_name) {
        table_names.push(callable_name.clone());
    }

    let typeof_table_name = format!("__callable_typeof({})", callable_name);
    if !table_names.contains(&typeof_table_name) {
        table_names.push(typeof_table_name);
    }
}

fn has_abstract_numeric_param(params: &[(String, JuliaType)]) -> bool {
    params.iter().any(|(_, ty)| ty.is_abstract_numeric())
}

fn is_concrete_numeric_return_type(ty: &ValueType) -> bool {
    is_numeric_type(ty) || matches!(ty, ValueType::BigInt | ValueType::BigFloat)
}

fn direct_parameter_return_snapshot(func: &Function) -> Option<JuliaType> {
    let returned_name = directly_returned_var(func)?;
    let returned_param_type = func
        .params
        .iter()
        .find(|param| param.name == *returned_name)?
        .type_annotation
        .clone()?;

    julia_type_needs_return_snapshot(&returned_param_type).then_some(returned_param_type)
}

/// An optional keyword argument with no type annotation, e.g. `f(; n = 0)`.
///
/// Such a kwarg accepts *any* value at runtime regardless of its default's type,
/// so the no-JIT VM's single compiled body must type it as `Any` — using the
/// default literal's type would reject a differently-typed caller value
/// (`g(; n = 0); g(n = 1.5)` → `ReturnI64: expected integer`, Issue #5425). This
/// generalizes the `nothing`-default case (Issue #5416), which was the original
/// motivating bug, to ANY typed default.
fn is_unannotated_optional_kwparam(kw: &KwParam) -> bool {
    !kw.is_varargs && kw.type_annotation.is_none() && !is_required_kwarg(&kw.default)
}

/// True when the method body is exactly `return kw` / `kw` for an unannotated
/// optional keyword argument `kw` (`g(; n = 0) = n`).
///
/// The returned value can be any runtime type, so the *compiled body* must emit
/// `ReturnAny` (not a typed return) and *callers passing that kwarg* must keep
/// the result `Any` (Issue #5425; generalizes the `nothing`-default case of
/// Issue #5416). This deliberately does NOT widen `FunctionInfo.return_type`
/// itself: reflection (`Base.infer_return_type`) uses that field as a precise
/// snapshot for the omitted-kwarg signature (e.g. `Int64` for `g`), and the
/// `kwargs::default_expression_order_4297` fixture depends on it.
pub(in crate::compile) fn directly_returns_unannotated_optional_kwparam(func: &Function) -> bool {
    let Some(returned_name) = directly_returned_var(func) else {
        return false;
    };

    func.kwparams
        .iter()
        .any(|kw| kw.name == *returned_name && is_unannotated_optional_kwparam(kw))
}

/// True when the method's return value is *derived from* an unannotated optional
/// keyword argument through a computation rather than returned directly
/// (`g2(; n = 0) = n + 1`, `g2(; n = 0); m = n + 1; return m; end`).
///
/// This generalizes `directly_returns_unannotated_optional_kwparam` (Issue #5425,
/// which only covered `return kw`) to the follow-up case where the kwarg flows
/// into a binary op / call / local binding before being returned (Issue #5466).
/// The kwarg slot is already widened to `Any` (`is_unannotated_optional_kwparam`),
/// so the kwarg value loads dynamically; the remaining hazard is that the
/// function's *inferred* return type stays concrete (e.g. `n + 1` -> `Int64`
/// because the inference engine binds `n` to its default's type), so the compiled
/// body emits a typed `ReturnI64` that rejects a differently-typed result. As with
/// #5425 this widens the *body* / *dispatch* (`MethodSig.return_type`) /
/// *call-site* (v2) return-type channels to `Any` but deliberately leaves
/// `FunctionInfo.return_type` precise so reflection stays accurate.
///
/// Detection is conservative: it tracks data flow from the kwparam(s) through
/// local assignments (a one-or-more-step taint) and only fires when an actual
/// return-value expression references a tainted name. A function that merely
/// *mentions* a kwarg without returning a value derived from it
/// (`function f(; n = 0); println(n); return 5; end`) is NOT widened.
pub(in crate::compile) fn returns_value_derived_from_unannotated_optional_kwparam(
    func: &Function,
) -> bool {
    // The directly-returned case is handled by its own predicate (it also feeds
    // the `Nothing`-default snapshot widening); skip it here to keep the two
    // detection paths disjoint and their call sites independently auditable.
    if directly_returns_unannotated_optional_kwparam(func) {
        return false;
    }

    let mut tainted: HashSet<String> = func
        .kwparams
        .iter()
        .filter(|kw| is_unannotated_optional_kwparam(kw))
        .map(|kw| kw.name.clone())
        .collect();
    if tainted.is_empty() {
        return false;
    }

    // Propagate taint through local assignments to a fixpoint: a local bound to
    // an expression that references a tainted name becomes tainted itself, so
    // `m = n + 1; return m` is recognized.
    loop {
        let mut changed = false;
        propagate_taint_in_block(&func.body, &mut tainted, &mut changed);
        if !changed {
            break;
        }
    }

    let mut derived = false;
    collect_return_value_derivation(&func.body, &tainted, &mut derived);
    derived
}

/// True when the method's return value is an unannotated optional kwarg — either
/// returned directly (`g(; n = 0) = n`, Issue #5425) or derived from it through a
/// computation (`g2(; n = 0) = n + 1`, Issue #5466). The compiled body, dispatch
/// (`MethodSig.return_type`) and call-site (v2) return-type channels must all be
/// `Any` in either case; `FunctionInfo.return_type` stays precise for reflection.
pub(in crate::compile) fn returns_unannotated_optional_kwparam_value(func: &Function) -> bool {
    directly_returns_unannotated_optional_kwparam(func)
        || returns_value_derived_from_unannotated_optional_kwparam(func)
}

fn returns_untyped_param_power_value(func: &Function) -> bool {
    let untyped_params: HashSet<String> = func
        .params
        .iter()
        .filter(|param| param.type_annotation.is_none() && !param.is_varargs)
        .map(|param| param.name.clone())
        .collect();
    if untyped_params.is_empty() {
        return false;
    }

    let mut found = false;
    collect_return_value_power_from_names(&func.body, &untyped_params, &mut found);
    found
}

fn collect_return_value_power_from_names(block: &Block, names: &HashSet<String>, found: &mut bool) {
    let len = block.stmts.len();
    for (idx, stmt) in block.stmts.iter().enumerate() {
        match stmt {
            Stmt::Return {
                value: Some(expr), ..
            } => {
                *found = *found || expr_has_power_operand_referencing_any(expr, names);
            }
            Stmt::Expr { expr, .. } if idx + 1 == len => {
                *found = *found || expr_has_power_operand_referencing_any(expr, names);
            }
            Stmt::Block(inner) => collect_return_value_power_from_names(inner, names, found),
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_return_value_power_from_names(then_branch, names, found);
                if let Some(else_branch) = else_branch {
                    collect_return_value_power_from_names(else_branch, names, found);
                }
            }
            Stmt::For { body, .. }
            | Stmt::ForEach { body, .. }
            | Stmt::ForEachTuple { body, .. }
            | Stmt::While { body, .. }
            | Stmt::Timed { body, .. }
            | Stmt::TestSet { body, .. } => {
                collect_return_value_power_from_names(body, names, found)
            }
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                collect_return_value_power_from_names(try_block, names, found);
                for block in [catch_block, else_block, finally_block]
                    .into_iter()
                    .flatten()
                {
                    collect_return_value_power_from_names(block, names, found);
                }
            }
            _ => {}
        }
    }
}

fn expr_has_power_operand_referencing_any(expr: &Expr, names: &HashSet<String>) -> bool {
    match expr {
        Expr::BinaryOp {
            op, left, right, ..
        } => {
            (*op == crate::ir::core::BinaryOp::Pow)
                && (expr_references_any(left, names) || expr_references_any(right, names))
                || expr_has_power_operand_referencing_any(left, names)
                || expr_has_power_operand_referencing_any(right, names)
        }
        Expr::UnaryOp { operand, .. } => expr_has_power_operand_referencing_any(operand, names),
        Expr::Call {
            function,
            args,
            kwargs,
            ..
        } => {
            (function == "^" && args.iter().any(|arg| expr_references_any(arg, names)))
                || args
                    .iter()
                    .any(|arg| expr_has_power_operand_referencing_any(arg, names))
                || kwargs
                    .iter()
                    .any(|(_, value)| expr_has_power_operand_referencing_any(value, names))
        }
        Expr::ModuleCall {
            function,
            args,
            kwargs,
            ..
        } => {
            (function == "^" && args.iter().any(|arg| expr_references_any(arg, names)))
                || args
                    .iter()
                    .any(|arg| expr_has_power_operand_referencing_any(arg, names))
                || kwargs
                    .iter()
                    .any(|(_, value)| expr_has_power_operand_referencing_any(value, names))
        }
        Expr::Builtin { args, .. } => args
            .iter()
            .any(|arg| expr_has_power_operand_referencing_any(arg, names)),
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => elements
            .iter()
            .any(|expr| expr_has_power_operand_referencing_any(expr, names)),
        Expr::Index { array, indices, .. } => {
            expr_has_power_operand_referencing_any(array, names)
                || indices
                    .iter()
                    .any(|idx| expr_has_power_operand_referencing_any(idx, names))
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            expr_has_power_operand_referencing_any(condition, names)
                || expr_has_power_operand_referencing_any(then_expr, names)
                || expr_has_power_operand_referencing_any(else_expr, names)
        }
        Expr::LetBlock { bindings, body, .. } => {
            bindings
                .iter()
                .any(|(_, value)| expr_has_power_operand_referencing_any(value, names))
                || block_has_power_operand_referencing_any(body, names)
        }
        _ => false,
    }
}

fn block_has_power_operand_referencing_any(block: &Block, names: &HashSet<String>) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        Stmt::Assign { value, .. } | Stmt::AddAssign { value, .. } => {
            expr_has_power_operand_referencing_any(value, names)
        }
        Stmt::Return {
            value: Some(expr), ..
        }
        | Stmt::Expr { expr, .. } => expr_has_power_operand_referencing_any(expr, names),
        _ => false,
    })
}

/// One pass of taint propagation over a block's assignment statements.
fn propagate_taint_in_block(block: &Block, tainted: &mut HashSet<String>, changed: &mut bool) {
    for stmt in &block.stmts {
        propagate_taint_in_stmt(stmt, tainted, changed);
    }
}

fn propagate_taint_in_stmt(stmt: &Stmt, tainted: &mut HashSet<String>, changed: &mut bool) {
    match stmt {
        Stmt::Assign { var, value, .. } | Stmt::AddAssign { var, value, .. } => {
            let derives = !tainted.contains(var) && expr_references_any(value, tainted);
            *changed = (derives && tainted.insert(var.clone())) || *changed;
        }
        Stmt::Block(block) => propagate_taint_in_block(block, tainted, changed),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            propagate_taint_in_block(then_branch, tainted, changed);
            if let Some(else_branch) = else_branch {
                propagate_taint_in_block(else_branch, tainted, changed);
            }
        }
        Stmt::For { body, .. }
        | Stmt::ForEach { body, .. }
        | Stmt::ForEachTuple { body, .. }
        | Stmt::While { body, .. }
        | Stmt::Timed { body, .. }
        | Stmt::TestSet { body, .. } => propagate_taint_in_block(body, tainted, changed),
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            propagate_taint_in_block(try_block, tainted, changed);
            for block in [catch_block, else_block, finally_block]
                .into_iter()
                .flatten()
            {
                propagate_taint_in_block(block, tainted, changed);
            }
        }
        _ => {}
    }
}

/// Walk a block looking for return-value expressions (explicit `return e` and the
/// block's trailing implicit-return expression) that reference a tainted name.
fn collect_return_value_derivation(block: &Block, tainted: &HashSet<String>, derived: &mut bool) {
    let len = block.stmts.len();
    for (idx, stmt) in block.stmts.iter().enumerate() {
        match stmt {
            Stmt::Return {
                value: Some(expr), ..
            } => {
                *derived = *derived || expr_references_any(expr, tainted);
            }
            // The last statement of a block is its implicit return value in Julia.
            Stmt::Expr { expr, .. } if idx + 1 == len => {
                *derived = *derived || expr_references_any(expr, tainted);
            }
            Stmt::Block(inner) => collect_return_value_derivation(inner, tainted, derived),
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_return_value_derivation(then_branch, tainted, derived);
                if let Some(else_branch) = else_branch {
                    collect_return_value_derivation(else_branch, tainted, derived);
                }
            }
            Stmt::For { body, .. }
            | Stmt::ForEach { body, .. }
            | Stmt::ForEachTuple { body, .. }
            | Stmt::While { body, .. }
            | Stmt::Timed { body, .. }
            | Stmt::TestSet { body, .. } => collect_return_value_derivation(body, tainted, derived),
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                collect_return_value_derivation(try_block, tainted, derived);
                for block in [catch_block, else_block, finally_block]
                    .into_iter()
                    .flatten()
                {
                    collect_return_value_derivation(block, tainted, derived);
                }
            }
            _ => {}
        }
    }
}

/// True when `expr` references any variable name in `names`. Walks the full Core
/// IR `Expr` tree (so binary ops, calls, indexing, comprehensions, ternaries,
/// field access, etc. are all covered). Nested function literals are not
/// descended into — their bodies have their own scope.
fn expr_references_any(expr: &Expr, names: &HashSet<String>) -> bool {
    match expr {
        Expr::Var(name, _) => names.contains(name),
        Expr::Literal(_, _)
        | Expr::TypedEmptyArray { .. }
        | Expr::SliceAll { .. }
        | Expr::FunctionRef { .. } => false,
        Expr::BinaryOp { left, right, .. } => {
            expr_references_any(left, names) || expr_references_any(right, names)
        }
        Expr::UnaryOp { operand, .. } => expr_references_any(operand, names),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            args.iter().any(|a| expr_references_any(a, names))
                || kwargs.iter().any(|(_, v)| expr_references_any(v, names))
        }
        Expr::Builtin { args, .. } => args.iter().any(|a| expr_references_any(a, names)),
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            elements.iter().any(|e| expr_references_any(e, names))
        }
        Expr::Index { array, indices, .. } => {
            expr_references_any(array, names)
                || indices.iter().any(|i| expr_references_any(i, names))
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            expr_references_any(start, names)
                || expr_references_any(stop, names)
                || step.as_ref().is_some_and(|s| expr_references_any(s, names))
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            expr_references_any(body, names)
                || expr_references_any(iter, names)
                || filter
                    .as_ref()
                    .is_some_and(|f| expr_references_any(f, names))
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            expr_references_any(body, names)
                || iterations
                    .iter()
                    .any(|(_, it)| expr_references_any(it, names))
                || filter
                    .as_ref()
                    .is_some_and(|f| expr_references_any(f, names))
        }
        Expr::FieldAccess { object, .. } => expr_references_any(object, names),
        Expr::NamedTupleLiteral { fields, .. } => {
            fields.iter().any(|(_, v)| expr_references_any(v, names))
        }
        Expr::Pair { key, value, .. } => {
            expr_references_any(key, names) || expr_references_any(value, names)
        }
        Expr::DictLiteral { pairs, .. } => pairs
            .iter()
            .any(|(k, v)| expr_references_any(k, names) || expr_references_any(v, names)),
        Expr::LetBlock { bindings, body, .. } => {
            bindings.iter().any(|(_, v)| expr_references_any(v, names))
                || block_references_any(body, names)
        }
        Expr::StringConcat { parts, .. } => parts.iter().any(|p| expr_references_any(p, names)),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            expr_references_any(condition, names)
                || expr_references_any(then_expr, names)
                || expr_references_any(else_expr, names)
        }
        // Any remaining variants (e.g. `new(...)`, lambdas) conservatively report
        // no reference so the widening never fires spuriously for them; the cases
        // above cover every shape exercised by the `return-derived-from-kwparam`
        // fixtures and ordinary arithmetic/call computations.
        _ => false,
    }
}

/// True when any expression inside `block` references a name in `names`.
fn block_references_any(block: &Block, names: &HashSet<String>) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        Stmt::Assign { value, .. } | Stmt::AddAssign { value, .. } => {
            expr_references_any(value, names)
        }
        Stmt::Return {
            value: Some(expr), ..
        }
        | Stmt::Expr { expr, .. } => expr_references_any(expr, names),
        _ => false,
    })
}

/// Returns the name of the `where`-bound type parameter a method body directly
/// returns, for the shape `g(...) where {..., R, ...} = R`. Used by reflection
/// inference to bind `R` from the concrete call signature (Issue #4845).
///
/// Only fires when the returned variable is a declared `where` type parameter
/// *and* is not also an ordinary value parameter name (so it cannot collide
/// with the existing `direct_parameter_return_snapshot` path).
fn direct_return_type_param(func: &Function) -> Option<String> {
    let returned_name = directly_returned_var(func)?;
    if !func
        .type_params
        .iter()
        .any(|type_param| type_param.name == *returned_name)
    {
        return None;
    }
    if func.params.iter().any(|param| param.name == *returned_name) {
        return None;
    }
    Some(returned_name.clone())
}

fn directly_returned_var(func: &Function) -> Option<&String> {
    let returned_name = match func.body.stmts.as_slice() {
        [Stmt::Return {
            value: Some(Expr::Var(name, _)),
            ..
        }]
        | [Stmt::Expr {
            expr: Expr::Var(name, _),
            ..
        }] => name,
        _ => return None,
    };

    Some(returned_name)
}

fn julia_type_needs_return_snapshot(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::VectorOf(_)
        | JuliaType::MatrixOf(_)
        | JuliaType::TupleOf(_)
        | JuliaType::Union(_)
        | JuliaType::UnionAll { .. }
        | JuliaType::TypeOf(_) => true,
        JuliaType::Struct(name) => name.contains('{'),
        _ => false,
    }
}

fn julia_type_contains_direct_typevar(ty: &JuliaType, name: &str) -> bool {
    match ty {
        JuliaType::TypeVar(type_name, _) => type_name == name,
        JuliaType::TypeOf(inner) | JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) => {
            julia_type_contains_direct_typevar(inner, name)
        }
        JuliaType::TupleOf(types) | JuliaType::Union(types) => types
            .iter()
            .any(|ty| julia_type_contains_direct_typevar(ty, name)),
        JuliaType::UnionAll { body, .. } => julia_type_contains_direct_typevar(body, name),
        _ => false,
    }
}

/// Collect the names of `where`-clause type variables (restricted to
/// `where_names`) that are syntactically referenced by a constructor
/// parameter's declared type. A type variable referenced this way can be
/// recovered at runtime by argument inference, so `new{...}` may materialize it
/// directly from the constructor frame (Issue #5059).
fn collect_referenced_type_var_names(
    ty: &JuliaType,
    where_names: &HashSet<&str>,
    out: &mut HashSet<String>,
) {
    match ty {
        // An unknown bare name (`x::T`) is parsed as either a TypeVar or a
        // Struct; treat it as a referenced type var when it names a where param.
        JuliaType::TypeVar(name, _) | JuliaType::Struct(name)
            if where_names.contains(name.as_str()) =>
        {
            out.insert(name.clone());
        }
        JuliaType::TypeOf(inner) | JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) => {
            collect_referenced_type_var_names(inner, where_names, out);
        }
        JuliaType::TupleOf(types) | JuliaType::Union(types) => {
            for inner in types {
                collect_referenced_type_var_names(inner, where_names, out);
            }
        }
        JuliaType::UnionAll { body, .. } => {
            collect_referenced_type_var_names(body, where_names, out);
        }
        _ => {}
    }
}

/// Optional cache inputs for compilation.
/// Groups the precompiled Base bytecode, method tables, and closure captures
/// that can be reused across compilations (Issue #2933).
#[derive(Debug, Default)]
pub(crate) struct CompilerCacheInput<'a> {
    pub precompiled_base: Option<&'a CompiledProgram>,
    pub method_tables: Option<&'a HashMap<String, MethodTable>>,
    pub closure_captures: Option<&'a HashMap<String, std::collections::HashSet<String>>>,
    pub inference_results: Option<&'a [(InferenceCacheKey, CachedReturn)]>,
}

/// Recursively collect `@enum` definitions from a block into `enum_types`
/// (Issue #5139). A later definition overwrites an earlier one of the same
/// name, matching upstream's "last definition wins" for redefinitions.
fn collect_enum_types(block: &Block, enum_types: &mut HashMap<String, EnumInfo>) {
    collect_enum_types_with_module_path(block, enum_types, None);
}

fn collect_enum_types_with_module_path(
    block: &Block,
    enum_types: &mut HashMap<String, EnumInfo>,
    module_path: Option<&str>,
) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::EnumDef { enum_def, .. } => {
                let info = EnumInfo {
                    base_type: enum_def.base_type.clone(),
                    members: enum_def
                        .members
                        .iter()
                        .map(|m| (m.name.clone(), m.value))
                        .collect(),
                };
                enum_types.insert(enum_def.name.clone(), info.clone());
                if let Some(module_path) = module_path {
                    enum_types.insert(format!("{}.{}", module_path, enum_def.name), info);
                }
            }
            // Enum defs may be nested in plain blocks (e.g. `begin ... end`).
            Stmt::Block(inner) => {
                collect_enum_types_with_module_path(inner, enum_types, module_path)
            }
            _ => {}
        }
    }
}

/// Collect `@enum` definitions defined inside a module (and its submodules).
fn collect_enum_types_in_module(
    module: &crate::ir::core::Module,
    enum_types: &mut HashMap<String, EnumInfo>,
) {
    collect_enum_types_in_module_inner(module, enum_types, "");
}

fn collect_enum_types_in_module_inner(
    module: &crate::ir::core::Module,
    enum_types: &mut HashMap<String, EnumInfo>,
    prefix: &str,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{}.{}", prefix, module.name)
    };
    collect_enum_types_with_module_path(&module.body, enum_types, Some(&module_path));
    for sub in &module.submodules {
        collect_enum_types_in_module_inner(sub, enum_types, &module_path);
    }
}

/// Collect user-declared primitive types defined inside modules (and submodules)
/// so they are visible to the compiler/runtime alongside top-level ones (Issue #5058).
fn collect_module_primitive_types(
    modules: &[crate::ir::core::Module],
    out: &mut Vec<PrimitiveTypeDefInfo>,
) {
    collect_module_primitive_types_inner(modules, out, "");
}

fn collect_module_primitive_types_inner(
    modules: &[crate::ir::core::Module],
    out: &mut Vec<PrimitiveTypeDefInfo>,
    prefix: &str,
) {
    for module in modules {
        let module_path = if prefix.is_empty() {
            module.name.clone()
        } else {
            format!("{}.{}", prefix, module.name)
        };
        for pt in &module.primitive_types {
            out.push(PrimitiveTypeDefInfo {
                name: pt.name.clone(),
                parent: pt.parent.clone(),
                bits: pt.bits,
            });
            out.push(PrimitiveTypeDefInfo {
                name: format!("{}.{}", module_path, pt.name),
                parent: pt.parent.clone(),
                bits: pt.bits,
            });
        }
        collect_module_primitive_types_inner(&module.submodules, out, &module_path);
    }
}

/// Recursively collect abstract type definitions declared inside modules /
/// bundled packages (Issues #7263 / #7265).
///
/// Top-level `abstract type` declarations land in `program.abstract_types`, but
/// the ones declared *inside a `module` body* (e.g. `Distribution`,
/// `VariateForm`, `ValueSupport` in the bundled Distributions package) live only
/// on `Module.abstract_types`. Without this collection they never reach the
/// compiler's abstract-type registry, so a method parameter annotated with a
/// module-local abstract type (`f(d::Distribution)`) is left as a concrete
/// `Struct("Distribution")` annotation that no value can satisfy — the typed
/// method silently loses dispatch to the untyped generic it extends. Collecting
/// them here mirrors `collect_module_structs` / `collect_module_primitive_types`
/// and feeds both the struct hierarchy and `resolve_abstract_type`.
///
/// Module abstract types keep their *bare* name (`Distribution`) for
/// within-module annotations and parent links, and also gain a qualified
/// `ModuleName.Distribution` entry so runtime module-binding reflection can
/// prove ownership without treating every bare name as visible in every module.
fn collect_module_abstract_types(
    modules: &[crate::ir::core::Module],
    out: &mut Vec<crate::ir::core::AbstractTypeDef>,
) {
    collect_module_abstract_types_inner(modules, out, "");
}

fn collect_module_abstract_types_inner(
    modules: &[crate::ir::core::Module],
    out: &mut Vec<crate::ir::core::AbstractTypeDef>,
    prefix: &str,
) {
    for module in modules {
        let module_path = if prefix.is_empty() {
            module.name.clone()
        } else {
            format!("{}.{}", prefix, module.name)
        };
        for at in &module.abstract_types {
            out.push(at.clone());
            let mut qualified = at.clone();
            qualified.name = format!("{}.{}", module_path, at.name);
            out.push(qualified);
        }
        collect_module_abstract_types_inner(&module.submodules, out, &module_path);
    }
}

/// Compile a Core IR Program into bytecode with multiple dispatch support.
pub fn compile_core_program(program: &Program) -> CResult<CompiledProgram> {
    compile_core_program_with_globals(program, &HashMap::new(), &HashMap::new())
}

pub fn compile_core_program_with_globals(
    program: &Program,
    global_types: &HashMap<String, ValueType>,
    global_struct_names: &HashMap<String, String>,
) -> CResult<CompiledProgram> {
    let output = compile_core_program_internal(
        program,
        global_types,
        global_struct_names,
        CompilerCacheInput::default(),
    )?;
    Ok(output.compiled)
}

// LoopContext, FinallyContext, CoreCompiler struct, impl, and type predicates
// are now in core_compiler.rs module
// Collection helpers are now in collect.rs module
// Utility functions are now in utils.rs module
