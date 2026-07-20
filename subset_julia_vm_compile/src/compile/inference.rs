//! Type inference engine for the compiler.
//!
//! This module provides functions to infer types of expressions, function return types,
//! and parameter types from usage patterns.
//!
//! # Architecture
//!
//! The compiler builds a shared lattice-based abstract interpretation engine
//! (`build_shared_inference_engine`) with support for:
//!
//! - Loop variable type inference from iterators
//! - Conditional type narrowing (isa checks, === nothing)
//! - Union type inference
//! - Transfer functions for built-in operations
//! - HOF (Higher-Order Function) inference via interprocedural analysis
//!
//! # Usage
//!
//! The main compiler uses `build_shared_inference_engine` to create a single engine
//! before the function compilation loop, then calls `engine.infer_function()` for each
//! function. This provides more accurate type information, especially for:
//!
//! - Loop variables in `for` loops
//! - Type narrowing in conditional branches
//! - Union types from multiple return paths
//!
//! # Design Principles
//!
//! - **Abstract Interpretation**: Simulate program execution using abstract types
//! - **Fixed-Point Iteration**: Iterate until type information stabilizes
//! - **Type Lattice**: Organize types in a hierarchy (Bottom → Concrete → Union → Top)
//! - **Transfer Functions**: Built-in functions have known return types
//!
//! See `docs/vm/TYPE_INFERENCE.md` for user-facing documentation.

use std::collections::{HashMap, HashSet};

use crate::ir::core::{Block, BuiltinOp, Expr, Function, Literal, Stmt};
use crate::runtime_types::{ArrayElementType, ValueType};
pub(crate) use subset_julia_vm_bytecode::{
    infer_simple_function_return_type_for_value_args, promote_numeric_value_types,
};

use super::is_pi_name;
use crate::compile::context::StructRegistry;

/// Collect local variable types from assignments for type inference.
/// When a variable is assigned different types:
/// - Same type: keep type
/// - F64 + I64 mix: use Any for true dynamic typing (Julia semantics)
/// - Struct + Any: keep Struct (more specific)
/// - Two numeric types: widen via promotion (`promote_numeric_value_types`)
///   then fall back to the previous "use new" behavior so existing flows
///   (e.g. `total = 0.0; total += int_val` after promotion) keep working.
/// - Two incompatible non-numeric types: widen to `Any` so the slot uses
///   dynamic typing (Issue #3535) — previously this returned the new type,
///   which could mis-compile earlier or other-branch assignments.
fn widen_type(old: &Option<ValueType>, new: ValueType) -> ValueType {
    match (old, &new) {
        // No previous type → just adopt the new one
        (None, _) => new,
        // Same type → keep
        (Some(o), n) if o == n => new,
        // F64 + I64 mix: use Any for dynamic typing
        // This enables Julia-compatible behavior where die = 7.0 then die = 6
        // correctly changes the type at runtime instead of widening to F64
        (Some(ValueType::F64), ValueType::I64) | (Some(ValueType::I64), ValueType::F64) => {
            ValueType::Any
        }
        // Once Any (from F64+I64 mix), stay Any for subsequent numeric assignments
        // This ensures the variable remains dynamically typed throughout the function
        (Some(ValueType::Any), ValueType::I64) | (Some(ValueType::Any), ValueType::F64) => {
            ValueType::Any
        }
        // If old is Struct and new is Any, keep Struct (more specific)
        // This preserves struct types from REPL session when inject_globals creates Literal::Struct
        (Some(ValueType::Struct(id)), ValueType::Any) => ValueType::Struct(*id),
        // Both sides numeric (and not the F64/I64 case handled above): keep the
        // legacy behavior of preferring the new type. This preserves promotion
        // semantics for sequences like `s = 0; s = s + 1.5` and avoids
        // regressing existing fixture/lib tests that depend on this widening.
        (Some(o), n) if is_numeric_value_type(o) && is_numeric_value_type(n) => new,
        // Issue #3535: Two different non-numeric concrete types — widen to Any
        // so the slot uses dynamic typing. Examples: Int64 + String, Struct + Nothing.
        _ => ValueType::Any,
    }
}

/// Join two maps of local types branch-locally.
///
/// For each variable present in either side, take the widened (joined) type.
/// Variables that are present in only one branch are widened against the
/// pre-branch type (passed as `base`) — if they were not bound before the
/// branch they get `Any` to model the possibility of being unassigned along
/// the other branch.
///
/// `mixed_type_vars` is updated for variables that widen to `Any` because of
/// incompatible non-numeric branch types — those need dynamic-slot codegen
/// (Issue #3535).
fn join_branch_locals(
    base: &HashMap<String, ValueType>,
    a: &HashMap<String, ValueType>,
    b: &HashMap<String, ValueType>,
    mixed_type_vars: &mut HashSet<String>,
) -> HashMap<String, ValueType> {
    let mut out = base.clone();
    let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
    keys.sort();
    keys.dedup();
    for k in keys {
        let av = a.get(k);
        let bv = b.get(k);
        let joined = match (av, bv) {
            (Some(at), Some(bt)) => {
                if at == bt {
                    at.clone()
                } else {
                    widen_type(&Some(at.clone()), bt.clone())
                }
            }
            // Variable assigned only in branch `a` — join with the pre-branch type
            // (which may be None, meaning it wasn't defined yet).
            (Some(at), None) => {
                let base_ty = base.get(k).cloned();
                if base_ty.as_ref() == Some(at) {
                    at.clone()
                } else if base_ty.is_some() {
                    widen_type(&base_ty, at.clone())
                } else {
                    // Variable only conditionally assigned — be conservative.
                    ValueType::Any
                }
            }
            (None, Some(bt)) => {
                let base_ty = base.get(k).cloned();
                if base_ty.as_ref() == Some(bt) {
                    bt.clone()
                } else if base_ty.is_some() {
                    widen_type(&base_ty, bt.clone())
                } else {
                    ValueType::Any
                }
            }
            (None, None) => continue,
        };
        // Issue #3535/#3536: When the join produced Any from two CONCRETE
        // (non-Any) branch types and at least one was non-numeric, mark the
        // variable for dynamic-slot codegen so the slot stays Any through
        // every assignment instead of latching onto the first concrete type
        // a branch produces. Skip the case where one branch was already Any
        // (typically from a Call returning Any) — the existing logic already
        // handles that.
        if joined == ValueType::Any {
            if let (Some(at), Some(bt)) = (av, bv) {
                if *at != ValueType::Any && *bt != ValueType::Any && at != bt {
                    let any_non_numeric = !is_numeric_value_type(at) || !is_numeric_value_type(bt);
                    if any_non_numeric {
                        mixed_type_vars.insert(k.clone());
                    }
                }
            }
        }
        out.insert(k.clone(), joined);
    }
    out
}

fn is_numeric_value_type(vt: &ValueType) -> bool {
    matches!(
        vt,
        ValueType::I8
            | ValueType::I16
            | ValueType::I32
            | ValueType::I64
            | ValueType::I128
            | ValueType::BigInt
            | ValueType::U8
            | ValueType::U16
            | ValueType::U32
            | ValueType::U64
            | ValueType::U128
            | ValueType::F16
            | ValueType::F32
            | ValueType::F64
            | ValueType::BigFloat
            | ValueType::Bool
    )
}

/// Collect local variable types and track variables with mixed F64+I64 types.
/// These variables should use dynamic typing (StoreAny/LoadAny) to allow type changes at runtime.
/// Collect local variable types with widening and track mixed-type variables.
/// Variables with mixed F64+I64 types are tracked in mixed_type_vars for dynamic typing.
///
/// Issue #5922 (pre-scan shrink): this pass previously took a `use_widening`
/// flag with a non-widening "exact types" mode for main/REPL blocks, but every
/// caller had long since moved to the widening mode, so the flag and its dead
/// branches were removed. Widening is now unconditional.
/// Build a lattice [`TypeEnv`] from the pre-scan's current `locals` map so the
/// shared inference engine can type `For`/`ForEach` endpoint/iterable
/// expressions against the locals already discovered earlier in the same scan
/// (Issue #6602). Each local's [`ValueType`] is lowered to its [`LatticeType`]
/// via the canonical bridge (`value_type_to_lattice_with_struct_table`), the
/// same lowering the engine itself uses for parameter / global seeding, so the
/// two paths cannot drift apart.
fn type_env_from_locals(
    locals: &HashMap<String, ValueType>,
    struct_table: &StructRegistry,
) -> crate::compile::abstract_interp::TypeEnv {
    let mut env = crate::compile::abstract_interp::TypeEnv::new();
    for (name, ty) in locals {
        let lattice =
            crate::runtime_types::bridge::value_type_to_lattice_with_struct_table(ty, struct_table);
        env.set(name, lattice);
    }
    env
}

/// Slot `ValueType` for a literal RHS that the shared lattice authority
/// (`local_authority::literal_assignment_value_type`) defers (returns `None`
/// for): array / module / regex / enum / struct / quoted-AST / kwarg-marker
/// literals. The lattice widens these to `Top` -> `Any`, a
/// codegen-specialization hazard for array-literal locals, so the pre-scan types
/// them directly here — struct-table-aware for `Struct` literals (e.g. `im` ->
/// `Complex{Float64}`). Mirrors the historical `infer_value_type(_with_structs)`
/// literal arms this slice deletes (Issue #6601, final pre-scan retirement).
fn literal_rhs_value_type(lit: &Literal, struct_table: &StructRegistry) -> ValueType {
    match lit {
        Literal::Array(_, _) => ValueType::ArrayOf(ArrayElementType::F64, None),
        Literal::ArrayI64(_, _) => ValueType::ArrayOf(ArrayElementType::I64, None),
        Literal::ArrayBool(_, _) => ValueType::ArrayOf(ArrayElementType::Bool, None),
        Literal::Module(_) => ValueType::Module,
        Literal::Regex { .. } => ValueType::Regex,
        Literal::Enum { .. } => ValueType::Enum,
        Literal::Symbol(_) => ValueType::Symbol,
        // Struct literals (e.g. `im`) resolve via the struct table, matching the
        // legacy `infer_value_type_with_structs` arm.
        Literal::Struct(struct_name, _) => {
            if let Some(struct_info) = struct_table.get(struct_name) {
                return ValueType::Struct(struct_info.type_id);
            }
            if let Some(brace_idx) = struct_name.find('{') {
                let base_name = &struct_name[..brace_idx];
                let prefix = format!("{}{{", base_name);
                for (name, struct_info) in struct_table {
                    if name.starts_with(&prefix) || name == struct_name {
                        return ValueType::Struct(struct_info.type_id);
                    }
                }
            }
            ValueType::Any
        }
        // Quoted-AST / kwarg-marker / (scalars already handled upstream): dynamic.
        _ => ValueType::Any,
    }
}

/// Issue #6601: single seam for computing the slot `ValueType` of an
/// `Assign` RHS during the function-body pre-scan. Every non-literal class now
/// routes through the shared abstract-interpretation engine
/// (`assign_rhs_value_type_via_engine` -> `infer_expr_result` ->
/// `bridge::lattice_to_value_type`). Literal RHSs never reach this seam — the
/// driver types them precisely via `literal_rhs_value_type`.
fn assign_rhs_value_type(
    value: &Expr,
    locals: &HashMap<String, ValueType>,
    struct_table: &StructRegistry,
    global_types: &HashMap<String, ValueType>,
    engine: &mut Option<crate::compile::abstract_interp::InferenceEngine>,
) -> ValueType {
    match value {
        // Bare pi/π keeps the legacy F64 special-case the empty-table engine
        // lacks. Legacy resolves a `Var` from `locals`/`global_types` first and
        // only falls through to the `is_pi_name` → F64 case when the name is
        // bound in neither, so this guard mirrors that exact precedence (a name
        // shadowed by a local/global routes through the engine below).
        Expr::Var(name, _)
            if is_pi_name(name)
                && !locals.contains_key(name.as_str())
                && !global_types.contains_key(name.as_str()) =>
        {
            ValueType::F64
        }
        // Var otherwise: proven engine-equivalent — resolved locals/globals
        // agree (see `prescan_engine_equiv_var_resolved_local_issue_6601`), and
        // an unbound non-pi name yields `Any` on both paths.
        Expr::Var(..) => {
            assign_rhs_value_type_via_engine(value, locals, struct_table, global_types, engine)
        }
        // FunctionRef: legacy returns `ValueType::Function` unconditionally
        // (`infer_value_type_with_structs` has a bare `Expr::FunctionRef { .. }
        // => ValueType::Function` arm independent of name/locals/globals). The
        // engine would instead widen to `Any` (the bridge maps
        // `ConcreteType::Function -> ValueType::Any`), so reproduce the legacy
        // result with a scoped shim here rather than changing the global bridge
        // (which is shared by every caller). Pinned by
        // `prescan_funcref_matches_legacy_through_seam_issue_6601`.
        Expr::FunctionRef { .. } => ValueType::Function,
        // Range: proven fully engine-equivalent across the #6601 discovery corpus
        // (int / var / float / mixed endpoints). Both paths produce
        // `ValueType::Range` *unconditionally* — legacy's `infer_value_type` has a
        // bare `Expr::Range { .. } => ValueType::Range` arm, and the engine always
        // returns `ConcreteType::Range { .. }` (even its heterogeneous fallback),
        // which the bridge maps to `ValueType::Range` regardless of element type.
        // Pinned by `prescan_engine_equiv_migrated_issue_6601`.
        Expr::Range { .. } => {
            assign_rhs_value_type_via_engine(value, locals, struct_table, global_types, engine)
        }
        // UnaryOp: proven engine-equivalent across the #6601 corpus once the
        // shared engine was fixed to match upstream Julia / the legacy pre-scan:
        // `!` (Not) always yields `Bool` (`tfunc_not`), and `-` (Neg) preserves
        // the concrete operand type, including `Complex{T}` (`tfunc_sub`). Both
        // changes live in the shared `compile/tfuncs/arithmetic.rs` (no local
        // special-case), so MAIN compilation sees the corrected typing too.
        // Pinned by `prescan_engine_equiv_unaryop_issue_6601` and
        // `prescan_engine_equiv_migrated_issue_6601`.
        Expr::UnaryOp { .. } => {
            assign_rhs_value_type_via_engine(value, locals, struct_table, global_types, engine)
        }
        // TupleLiteral: proven engine-equivalent across the #6601 corpus once the
        // shared engine was fixed to type ANY tuple literal as `Tuple`. Upstream
        // Julia: `typeof((1, "x", [])) == Tuple{...}` — a tuple literal is always
        // a `Tuple` regardless of element types, matching the legacy pre-scan's
        // unconditional `ValueType::Tuple`. The engine previously collapsed to
        // `Top`/`Any` when an element was non-concrete; the fix lives in the
        // shared `compile/abstract_interp/engine/mod.rs` `TupleLiteral` arm (no
        // local special-case), so MAIN compilation sees the corrected typing too.
        // Pinned by `prescan_engine_equiv_tuple_issue_6601` and
        // `prescan_engine_equiv_migrated_issue_6601`.
        Expr::TupleLiteral { .. } => {
            assign_rhs_value_type_via_engine(value, locals, struct_table, global_types, engine)
        }
        // FieldAccess: proven engine-equivalent across the #6601 corpus once the
        // shared engine learned the `Expr` builtin's fixed field types
        // (`head::Symbol`, `args::Vector{Any}`) — the struct-field path can't see
        // them because `Expr` isn't in the user struct table. Struct / unknown-field
        // / non-struct / array / Any object cases already agreed. The engine fix
        // lives in the shared `compile/abstract_interp/engine/mod.rs` FieldAccess
        // arm (no local special-case), so MAIN compilation sees the corrected
        // typing too; legacy `infer_value_type_with_structs` was aligned to type
        // `Expr.args` as `ArrayOf(Any)` (upstream `Vector{Any}`) to match. Pinned
        // by `prescan_engine_equiv_fieldaccess_issue_6601` and
        // `prescan_engine_equiv_migrated_issue_6601`.
        Expr::FieldAccess { .. } => {
            assign_rhs_value_type_via_engine(value, locals, struct_table, global_types, engine)
        }
        // Index: migrated to the shared engine as an *engine-better* class. The
        // engine types `arr[i]` precisely as the element type (legacy returned
        // `Any`), `s[i]` as `Char`, and `s[1:2]` as `Str` (after this slice's
        // shared-engine `getindex`-tfunc String->Char fix and the Index-arm
        // String-slice->String fix; both in `compile/tfuncs/array_ops.rs` /
        // `compile/abstract_interp/engine/mod.rs`, so MAIN compilation benefits
        // too). Verified upstream-correct by `prescan_engine_value_index_issue_6601`
        // and the `type_inference/prescan_index_6601.jl` fixture; filtered out of
        // the legacy-comparison divergence map via `is_migrated_assign_rhs_class`.
        Expr::Index { .. } => {
            assign_rhs_value_type_via_engine(value, locals, struct_table, global_types, engine)
        }
        // BinaryOp: migrated to the shared engine as an *engine-better* class. The
        // engine types `i^i`->I64 (new `tfunc_pow`), `s*s`->Str (string concat in
        // `tfunc_mul`), and Complex ops yield the canonical `ComplexF64` (the
        // bridge canonicalizes `Struct{Complex{Float64}}`->ComplexF64) where legacy
        // returned `F64`/`Struct(100)`. The `tfunc_pow`/`tfunc_mul` fixes live in
        // the shared `compile/tfuncs/arithmetic.rs` (no local special-case), so
        // MAIN compilation benefits too. Verified by
        // `prescan_engine_value_binaryop_issue_6601` + the
        // `type_inference/prescan_binaryop_6601.jl` fixture; filtered out of the
        // legacy-comparison divergence map via `is_migrated_assign_rhs_class`.
        Expr::BinaryOp { .. } => {
            assign_rhs_value_type_via_engine(value, locals, struct_table, global_types, engine)
        }
        // Call: migrated to the shared engine as an *engine-better* class. The
        // engine types `exp(z)`->ComplexF64 (the `tfunc_sqrt` family now preserves
        // `Complex`), `abs(Complex)`->F64, and `zeros(n)`->`ArrayOf(F64)` where the
        // legacy pre-scan was imprecise (`ComplexF64`/`Array`). The `tfunc_sqrt` fix
        // lives in the shared `compile/tfuncs/intrinsics.rs` (no local special-case),
        // so MAIN compilation benefits too. Verified by
        // `prescan_engine_value_call_issue_6601` + the
        // `type_inference/prescan_call_6601.jl` fixture; filtered out of the
        // legacy-comparison divergence map via `is_migrated_assign_rhs_class`.
        Expr::Call { .. } => {
            assign_rhs_value_type_via_engine(value, locals, struct_table, global_types, engine)
        }
        // Pre-scan retirement (Issue #6601, final slice): every non-literal
        // `Assign`-RHS class — including the non-corpus variants (array/dict
        // literals, comprehensions, ModuleCall, Builtin, ternaries, string-concat,
        // `new`, etc.) — now routes through the shared abstract-interpretation
        // engine. (Literal RHSs are typed precisely by the driver via
        // `literal_rhs_value_type` and never reach this seam.) The pre-scan no
        // longer has a second, legacy inference path: the double inference #5922
        // set out to remove is gone.
        _ => assign_rhs_value_type_via_engine(value, locals, struct_table, global_types, engine),
    }
}

/// Compute the engine-path `ValueType` for an `Assign` RHS — the migration
/// target. Mirrors the #6602 loop-var seam exactly (empty-table engine).
fn assign_rhs_value_type_via_engine(
    value: &Expr,
    locals: &HashMap<String, ValueType>,
    struct_table: &StructRegistry,
    global_types: &HashMap<String, ValueType>,
    engine: &mut Option<crate::compile::abstract_interp::InferenceEngine>,
) -> ValueType {
    let env = type_env_from_locals(locals, struct_table);
    let eng = loop_inference_engine(engine, struct_table, global_types);
    let lattice = eng.infer_expr_result(value, &env).ty;
    crate::runtime_types::bridge::lattice_to_value_type(&lattice)
}

/// Function-body / inner-constructor / `main` slot-typing pre-scan.
///
/// Issue #6601: this is the SOLE remaining pre-scan consumer after #6602/#6603.
/// It computes the whole-body widened slot types (forward references) and
/// `mixed_type_vars` that codegen reads *before* the first `Store` is emitted.
/// The exact load-bearing slot-typing contract, the engine-equivalence hazards
/// blocking a drop-in migration, and the 2-pass / lazy-slot retirement design
/// are documented in `docs/vm/TYPE_INFERENCE_COMPLETE.md`
/// ("Function-Body Slot-Typing Pre-Scan") and pinned by the
/// `prescan_*_issue_6601` characterization tests below.
pub fn collect_local_types_with_mixed_tracking(
    stmts: &[Stmt],
    locals: &mut HashMap<String, ValueType>,
    protected: &HashSet<String>,
    struct_table: &StructRegistry,
    global_types: &HashMap<String, ValueType>,
    mixed_type_vars: &mut HashSet<String>,
) {
    // Issue #6602 (pre-scan retirement 2/3): the `For` endpoint / `ForEach`
    // iterable loop-variable typing is routed through the shared lattice-based
    // abstract-interpretation engine's own expression inference
    // (`infer_expr_result`) + the same lattice element-type helpers
    // (`range_element_type` / `loop_analysis::element_type`) the engine uses
    // internally, then bridged back via `bridge::lattice_to_value_type` — the
    // engine-injection seam (mirroring `inference.rs`'s ForEach lattice insert).
    //
    // The engine is seeded with the struct table + globals but **no** function
    // table, matching the capability scope of the legacy
    // `infer_value_type_with_structs` pre-scan it replaces for this consumer
    // (that routine likewise could not resolve user calls). It is built lazily
    // (only when the first loop statement is reached) and threaded through the
    // recursion, so loop-free function bodies pay nothing and a body with loops
    // constructs the engine exactly once.
    let mut engine: Option<crate::compile::abstract_interp::InferenceEngine> = None;
    collect_local_types_with_mixed_tracking_impl(
        stmts,
        locals,
        protected,
        struct_table,
        global_types,
        mixed_type_vars,
        &mut engine,
    );
}

/// Lazily build (once) and return the shared inference engine used for
/// `For`/`ForEach` loop-variable typing (Issue #6602).
fn loop_inference_engine<'e>(
    engine: &'e mut Option<crate::compile::abstract_interp::InferenceEngine>,
    struct_table: &StructRegistry,
    global_types: &HashMap<String, ValueType>,
) -> &'e mut crate::compile::abstract_interp::InferenceEngine {
    engine.get_or_insert_with(|| build_shared_inference_engine_empty(struct_table, global_types))
}

/// Recursive worker for [`collect_local_types_with_mixed_tracking`].
///
/// Carries the lazily-built shared inference `engine` (Issue #6602) so the
/// `For`/`ForEach` loop-variable typing can use engine injection without
/// rebuilding the engine at every nested loop. All other statement classes are
/// unchanged from the legacy pre-scan.
#[allow(clippy::too_many_arguments)]
fn collect_local_types_with_mixed_tracking_impl(
    stmts: &[Stmt],
    locals: &mut HashMap<String, ValueType>,
    protected: &HashSet<String>,
    struct_table: &StructRegistry,
    global_types: &HashMap<String, ValueType>,
    mixed_type_vars: &mut HashSet<String>,
    engine: &mut Option<crate::compile::abstract_interp::InferenceEngine>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { var, value, .. } => {
                // Skip updating protected variables (function parameters)
                // This prevents overwriting parameter types inferred from assignments
                if protected.contains(var) {
                    continue;
                }

                // Issue #5922 / #6601 (pre-scan retirement): for *literal*
                // right-hand sides, route the local's type through the single
                // shared inference authority
                // (`abstract_interp::local_authority`). The authority reuses the
                // exact `Literal -> LatticeType` mapping the
                // abstract-interpretation engine uses (`InferenceEngine::infer_literal`),
                // bridged back to a `ValueType`, so the two inference paths
                // cannot drift apart for this class. It returns `None` for the
                // literal variants the lattice cannot represent faithfully
                // (array / struct / module / regex / enum / quoted-AST /
                // required-kwarg-marker literals); those are typed precisely (and
                // struct-table-aware) by `literal_rhs_value_type`, NOT widened to
                // `Any` by the engine.
                let ty = match value {
                    Expr::Literal(lit, _) => {
                        crate::compile::abstract_interp::local_authority::literal_assignment_value_type(
                            lit,
                        )
                        .unwrap_or_else(|| literal_rhs_value_type(lit, struct_table))
                    }
                    // Every non-literal expression class routes through the #6601
                    // seam, which now types them via the shared
                    // abstract-interpretation engine.
                    _ => assign_rhs_value_type(value, locals, struct_table, global_types, engine),
                };
                // Check if this is a DIRECT literal assignment (not a compound assignment)
                // Only direct literal assignments can cause mixed_type_vars (dynamic typing).
                // Compound assignments like `x *= y` use type promotion, not dynamic typing.
                let is_direct_literal = matches!(
                    value,
                    Expr::Literal(
                        Literal::Int(_)
                            | Literal::Float(_)
                            | Literal::Float32(_)
                            | Literal::Float16(_),
                        _
                    )
                );

                // Use widening to handle control flow where a variable might have
                // different types in different branches (e.g., die = floor(x) vs die = 6).
                let old_ty = locals.get(var).cloned();
                let widened = widen_type(&old_ty, ty.clone());
                // Track variables that were widened to Any due to F64+I64 mix
                // BUT only for direct literal assignments, not compound assignments.
                // This allows Julia semantics: `die = 7.0; die = 6` preserves Int64 at runtime,
                // while `result = 1; result *= 2.0` uses type promotion to Float64.
                if widened == ValueType::Any && old_ty.is_some() && is_direct_literal {
                    if let Some(ref old) = old_ty {
                        if (*old == ValueType::F64 && ty == ValueType::I64)
                            || (*old == ValueType::I64 && ty == ValueType::F64)
                        {
                            mixed_type_vars.insert(var.clone());
                        }
                    }
                }
                // Issue #3535: Variables whose collection widened to Any because
                // of incompatible (non-numeric) reassignment need dynamic typing
                // through the entire function so each assignment compiles
                // against a dynamic slot rather than the first concrete type.
                // Only triggers when BOTH old and new are concrete non-Any types;
                // a new Any (typically from a Call returning Any) is fine because
                // the existing `_ => ty` rule will pick up `ty=Any` and the slot
                // remains compatible.
                if widened == ValueType::Any {
                    if let Some(ref old) = old_ty {
                        let old_is_num = is_numeric_value_type(old);
                        let new_is_num = is_numeric_value_type(&ty);
                        // Both old and new are concrete non-Any, incompatible, and
                        // not both numeric.
                        let both_concrete_incompatible = *old != ValueType::Any
                            && ty != ValueType::Any
                            && *old != ty
                            && !(old_is_num && new_is_num);
                        // Issue #7350 (B5): the previous type is a concrete
                        // non-numeric type (e.g. `Nothing` from `acc = nothing`) and
                        // the reassignment widened to `Any` — typically a
                        // heterogeneous ternary/branch whose arms join to `Any`.
                        // This still needs a dynamic slot: without it the earlier
                        // concrete assignment narrows the slot (e.g. to `Nothing`),
                        // and a guard like `acc === nothing` reassigned inside a loop
                        // const-folds unsoundly (the loop body is compiled once with
                        // the pre-loop type), silently dropping the back-edge value.
                        let concrete_old_widened_to_any =
                            *old != ValueType::Any && ty == ValueType::Any && !old_is_num;
                        if both_concrete_incompatible || concrete_old_widened_to_any {
                            mixed_type_vars.insert(var.clone());
                        }
                    }
                }
                locals.insert(var.clone(), widened);
            }
            Stmt::DestructuringAssign { targets, value, .. } => {
                let _ = assign_rhs_value_type(value, locals, struct_table, global_types, engine);
                for target in targets {
                    if !protected.contains(target) {
                        locals.insert(target.clone(), ValueType::Any);
                    }
                }
            }
            Stmt::For {
                var,
                start,
                end,
                step,
                body,
                ..
            } => {
                // Loop variable type follows range element promotion (Issue #3518).
                // Falls back to I64 for non-numeric / unknown endpoints to preserve
                // pre-existing behavior for `1:n` ranges.
                //
                // Issue #6602: route the endpoint typing through the shared engine
                // (`infer_expr_result`) + the engine's own `range_element_type`, the
                // exact path the engine uses in its `Stmt::For` handling, instead of
                // the legacy `infer_value_type_with_structs` + a parallel
                // promote-and-fall-back-to-`I64` helper. The result is bridged back
                // to a `ValueType` via `lattice_to_value_type` (engine injection).
                // Issue #10984 / #10903: `var` is a fresh binding for this
                // loop's lifetime, not a reassignment of a same-named outer
                // local. Save the outer type now (if one is already tracked)
                // so it can be restored after the body scan below, instead of
                // leaking the loop-element type into the pre-scan's view of
                // every statement that follows the loop (which fed a stale
                // `ReturnI64`/`StoreI64` slot-type decision into codegen even
                // after the runtime-value fix in `CoreCompiler::shadow_local_enter`
                // / `shadow_local_exit`). A `var` with no prior tracked type
                // (first use) is intentionally left leaking forward, matching
                // pre-existing behavior for that case.
                let shadow_outer_ty = (!protected.contains(var))
                    .then(|| locals.get(var).cloned())
                    .flatten();
                if !protected.contains(var) {
                    let env = type_env_from_locals(locals, struct_table);
                    let eng = loop_inference_engine(engine, struct_table, global_types);
                    let start_ty = eng.infer_expr_result(start, &env).ty;
                    let end_ty = eng.infer_expr_result(end, &env).ty;
                    let step_ty = step.as_ref().map(|s| eng.infer_expr_result(s, &env).ty);
                    let elem_lattice = eng.range_element_type(&start_ty, &end_ty, step_ty.as_ref());
                    let elem_ty =
                        crate::runtime_types::bridge::lattice_to_value_type(&elem_lattice);
                    locals.insert(var.clone(), elem_ty);
                }
                collect_local_types_with_mixed_tracking_impl(
                    &body.stmts,
                    locals,
                    protected,
                    struct_table,
                    global_types,
                    mixed_type_vars,
                    engine,
                );
                if let Some(outer_ty) = shadow_outer_ty {
                    locals.insert(var.clone(), outer_ty);
                }
            }
            Stmt::ForEach {
                var,
                iterable,
                body,
                ..
            } => {
                // Infer element type from the iterable.
                //
                // Issue #6602: route the iterable typing through the shared engine
                // (`infer_expr_result`) directly to a `LatticeType`, then feed the
                // SAME `loop_analysis::element_type` the engine uses in its own
                // `Stmt::ForEach` handling, and bridge back via
                // `lattice_to_value_type` (engine injection). This drops the legacy
                // `infer_value_type_with_structs` first step (which produced a coarse
                // `ValueType` that was then re-lifted via `value_type_to_lattice`).
                //
                // Issue #10984 / #10903: see the matching comment in `Stmt::For`
                // above — save/restore the outer type around the loop-body
                // scan so a pre-existing same-named local's type is not
                // leaked into the pre-scan's view of the rest of the function.
                let shadow_outer_ty = (!protected.contains(var))
                    .then(|| locals.get(var).cloned())
                    .flatten();
                if !protected.contains(var) {
                    let env = type_env_from_locals(locals, struct_table);
                    let eng = loop_inference_engine(engine, struct_table, global_types);
                    let iterable_lattice = eng.infer_expr_result(iterable, &env).ty;
                    let elem_lattice = crate::compile::abstract_interp::loop_analysis::element_type(
                        &iterable_lattice,
                    );
                    let elem_type =
                        crate::runtime_types::bridge::lattice_to_value_type(&elem_lattice);
                    locals.insert(var.clone(), elem_type);
                }
                collect_local_types_with_mixed_tracking_impl(
                    &body.stmts,
                    locals,
                    protected,
                    struct_table,
                    global_types,
                    mixed_type_vars,
                    engine,
                );
                if let Some(outer_ty) = shadow_outer_ty {
                    locals.insert(var.clone(), outer_ty);
                }
            }
            Stmt::While { body, .. } | Stmt::Timed { body, .. } => {
                collect_local_types_with_mixed_tracking_impl(
                    &body.stmts,
                    locals,
                    protected,
                    struct_table,
                    global_types,
                    mixed_type_vars,
                    engine,
                );
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // Julia evaluates the condition in the surrounding function
                // scope before either branch. A condition may be a begin block
                // or macro-expanded value block that assigns locals used by the
                // branches/fall-through (Issues #7617, #7621), so collect it
                // before snapshotting the branch base environment.
                collect_expr_locals(
                    condition,
                    locals,
                    protected,
                    struct_table,
                    global_types,
                    mixed_type_vars,
                );
                // Issue #3536: Branch environments must start from the same
                // pre-branch state and be joined at the merge point — not
                // threaded sequentially through a single shared map. Otherwise
                // the else branch sees the then branch's assignments and the final
                // type depends on traversal order rather than control flow.
                let base = locals.clone();
                let mut then_locals = base.clone();
                collect_local_types_with_mixed_tracking_impl(
                    &then_branch.stmts,
                    &mut then_locals,
                    protected,
                    struct_table,
                    global_types,
                    mixed_type_vars,
                    engine,
                );
                let merged = if let Some(eb) = else_branch {
                    let mut else_locals = base.clone();
                    collect_local_types_with_mixed_tracking_impl(
                        &eb.stmts,
                        &mut else_locals,
                        protected,
                        struct_table,
                        global_types,
                        mixed_type_vars,
                        engine,
                    );
                    join_branch_locals(&base, &then_locals, &else_locals, mixed_type_vars)
                } else {
                    // No else branch: the then branch may not execute, so join with
                    // the pre-branch state for any variable assigned only in `then`.
                    join_branch_locals(&base, &then_locals, &base, mixed_type_vars)
                };
                *locals = merged;
            }
            Stmt::Try {
                try_block,
                catch_var,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                // Issue #9131: mirror the Stmt::If fix — each branch must start
                // from an independent copy of the pre-try locals, and their
                // results are joined at the merge point, not threaded sequentially
                // through a shared map. Catch entry = pre-try locals (sound
                // over-approximation: the exception can fire at any point in try,
                // so catch cannot assume any try assignment has completed).
                let base = locals.clone();

                // Try branch.
                let mut try_locals = base.clone();
                collect_local_types_with_mixed_tracking_impl(
                    &try_block.stmts,
                    &mut try_locals,
                    protected,
                    struct_table,
                    global_types,
                    mixed_type_vars,
                    engine,
                );

                // Else branch (runs after successful try, starts from post-try locals).
                let normal_path_locals = if let Some(eb) = else_block {
                    let mut else_locals = try_locals.clone();
                    collect_local_types_with_mixed_tracking_impl(
                        &eb.stmts,
                        &mut else_locals,
                        protected,
                        struct_table,
                        global_types,
                        mixed_type_vars,
                        engine,
                    );
                    else_locals
                } else {
                    try_locals
                };

                // Catch branch (starts from pre-try locals — Issue #9131).
                let merged = if let Some(cb) = catch_block {
                    let mut catch_locals = base.clone();
                    // Issue #10999: `catch e` binds the caught exception (statically
                    // `Any`) to `e` at catch entry. Upstream does NOT shadow/restore
                    // the binding — a same-named outer local is permanently
                    // overwritten — so `e` must appear in the catch branch's
                    // environment as `Any`. Without this the whole-function slot
                    // pre-scan froze `e` at the outer assignment's concrete type
                    // (e.g. `String`) and the catch-entry `StoreAny` then failed the
                    // slot type check at runtime.
                    if let Some(var) = catch_var {
                        if !protected.contains(var) {
                            catch_locals.insert(var.clone(), ValueType::Any);
                            // A concrete pre-existing type for the same name means the
                            // physical slot must stay dynamic across both paths; the
                            // Any/concrete join in `join_branch_locals` does not mark
                            // it on its own (it skips pairs where one side is Any).
                            if base.get(var).is_some_and(|t| *t != ValueType::Any)
                                || normal_path_locals
                                    .get(var)
                                    .is_some_and(|t| *t != ValueType::Any)
                            {
                                mixed_type_vars.insert(var.clone());
                            }
                        }
                    }
                    collect_local_types_with_mixed_tracking_impl(
                        &cb.stmts,
                        &mut catch_locals,
                        protected,
                        struct_table,
                        global_types,
                        mixed_type_vars,
                        engine,
                    );
                    join_branch_locals(&base, &normal_path_locals, &catch_locals, mixed_type_vars)
                } else {
                    // No catch: exception path falls through with at most the
                    // pre-try locals (variables assigned in try may not exist).
                    join_branch_locals(&base, &normal_path_locals, &base, mixed_type_vars)
                };
                *locals = merged;

                // Finally always runs; process it on the merged post-try/catch env.
                if let Some(fb) = finally_block {
                    collect_local_types_with_mixed_tracking_impl(
                        &fb.stmts,
                        locals,
                        protected,
                        struct_table,
                        global_types,
                        mixed_type_vars,
                        engine,
                    );
                }
            }
            // @testset introduces its own local body scope. Do not let reused
            // names such as `arr`/`v` in separate testsets overwrite the outer
            // pre-scan map and statically poison later bodies (Issues #5588).
            Stmt::TestSet { body, .. } => {
                let mut scoped_locals = locals.clone();
                let mut scoped_mixed_type_vars = mixed_type_vars.clone();
                collect_local_types_with_mixed_tracking_impl(
                    &body.stmts,
                    &mut scoped_locals,
                    protected,
                    struct_table,
                    global_types,
                    &mut scoped_mixed_type_vars,
                    engine,
                );
            }
            Stmt::Block(block) => {
                collect_local_types_with_mixed_tracking_impl(
                    &block.stmts,
                    locals,
                    protected,
                    struct_table,
                    global_types,
                    mixed_type_vars,
                    engine,
                );
            }
            // Handle Stmt::Expr containing LetBlock - these appear from macro expansions
            // like @testset where the body is wrapped in nested LetBlocks (Issue #2358)
            Stmt::Expr { expr, .. } => {
                collect_expr_locals(
                    expr,
                    locals,
                    protected,
                    struct_table,
                    global_types,
                    mixed_type_vars,
                );
            }
            _ => {}
        }
    }
}

/// Collect the *names* of the local bindings the typed pre-scan
/// ([`collect_local_types_with_mixed_tracking`]) would introduce for `stmts`,
/// without computing any types (Issue #5922 — pre-scan shrink).
///
/// Module-level lambda capture analysis only consumes the binding *name set*
/// (`analyze_free_variables` takes a `HashSet<String>`), so running the full
/// typed pre-scan there paid for type inference whose results were discarded.
/// This walker mirrors the typed pre-scan's traversal and scoping exactly
/// (with an empty `protected` set):
///
/// - `Assign` targets and `For`/`ForEach` loop variables are bindings.
/// - `While`/`Timed`/`Block`/`If`/`Try` bodies are walked transparently.
/// - `@testset` bodies (both `Stmt::TestSet` and macro-expanded `LetBlock`s
///   that open a testset scope) are *skipped*: their names do not escape the
///   typed pre-scan either (Issues #5588, #6256). The separate
///   `collect_testset_local_binding_names_for_capture` pass re-adds testset
///   binding names for capture analysis.
/// - Non-testset `LetBlock` bodies under any expression position contribute
///   their bindings, exactly like `collect_expr_locals`.
///
/// The equivalence with the typed pre-scan's key set is pinned by
/// `capture_binding_names_match_typed_prescan_keys_issue_5922`.
pub fn collect_local_binding_names_for_capture(stmts: &[Stmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { var, value, .. } => {
                out.insert(var.clone());
                // A closure returned from a value-position `let`/`begin`
                // (`f = let; c = 10; () -> c end`) must capture that block's
                // locals, exactly like a statement-position block's closure does.
                // Descend the RHS so the block-local is a capturable name rather
                // than a leaked/undefined module global — a hard `let` now
                // discards its locals at block exit (Issue #9313), so a
                // non-captured lazy global read would fail after the block.
                collect_expr_binding_names_for_capture(value, out);
            }
            Stmt::DestructuringAssign { targets, value, .. } => {
                out.extend(targets.iter().cloned());
                collect_expr_binding_names_for_capture(value, out);
            }
            Stmt::For { var, body, .. } | Stmt::ForEach { var, body, .. } => {
                out.insert(var.clone());
                collect_local_binding_names_for_capture(&body.stmts, out);
            }
            Stmt::While { body, .. } | Stmt::Timed { body, .. } | Stmt::Block(body) => {
                collect_local_binding_names_for_capture(&body.stmts, out);
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_local_binding_names_for_capture(&then_branch.stmts, out);
                if let Some(eb) = else_branch {
                    collect_local_binding_names_for_capture(&eb.stmts, out);
                }
            }
            Stmt::Try {
                try_block,
                catch_var,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                collect_local_binding_names_for_capture(&try_block.stmts, out);
                // `catch e` binds `e` as a local (Issue #10999) — mirror the typed
                // pre-scan, whose key set this walker must match.
                if catch_block.is_some() {
                    if let Some(var) = catch_var {
                        out.insert(var.clone());
                    }
                }
                for block in [catch_block, else_block, finally_block]
                    .into_iter()
                    .flatten()
                {
                    collect_local_binding_names_for_capture(&block.stmts, out);
                }
            }
            // @testset bodies are their own local scope; names do not escape
            // (mirrors the typed pre-scan's scoped handling, Issue #5588).
            Stmt::TestSet { .. } => {}
            Stmt::Expr { expr, .. } => {
                collect_expr_binding_names_for_capture(expr, out);
            }
            _ => {}
        }
    }
}

/// Expression-position helper for [`collect_local_binding_names_for_capture`]:
/// mirror [`collect_expr_locals`] — recurse `LetBlock` binding values for
/// nested `LetBlock`s, then collect the body's binding names unless the body
/// opens a `@testset` scope (whose names do not escape).
fn collect_expr_binding_names_for_capture(expr: &Expr, out: &mut HashSet<String>) {
    visit_outermost_letblocks(expr, &mut |bindings, body| {
        for (_, value) in bindings {
            collect_expr_binding_names_for_capture(value, out);
        }
        if !block_opens_testset_scope(body) {
            collect_local_binding_names_for_capture(&body.stmts, out);
        }
    });
}

/// Recursively collect local variable types from expressions (Issue #2358, #3537).
///
/// This handles `LetBlock` expressions that contain statements, which appear
/// from macro expansions like `@testset` where the body is wrapped in nested
/// `LetBlock`s, as well as `begin...end` blocks lowered to `LetBlock` that may
/// appear under any expression position (binary ops, indexing, tuple/array
/// literals, unary ops, ...). Without recursing into all subexpressions the
/// nested locals would be missed and downstream inference / closure capture
/// would compile against the wrong slot type.
///
/// Only walks expression structure — does not insert variables on its own;
/// any locals are introduced by the inner
/// `collect_local_types_with_mixed_tracking` call when a
/// `LetBlock` body is reached.
fn collect_expr_locals(
    expr: &Expr,
    locals: &mut HashMap<String, ValueType>,
    protected: &HashSet<String>,
    struct_table: &StructRegistry,
    global_types: &HashMap<String, ValueType>,
    mixed_type_vars: &mut HashSet<String>,
) {
    visit_outermost_letblocks(expr, &mut |bindings, body| {
        // Recurse into binding values first (they may themselves contain
        // nested LetBlocks).
        for (_, value) in bindings {
            collect_expr_locals(
                value,
                locals,
                protected,
                struct_table,
                global_types,
                mixed_type_vars,
            );
        }
        // Macro-expanded @testset bodies are LetBlocks containing
        // _testset_begin! / _testset_end!. They are Julia local scopes, so
        // same-named locals in separate testsets must not pre-poison the
        // outer pre-scan map (Issue #6256).
        if block_opens_testset_scope(body) {
            let mut scoped_locals = locals.clone();
            let mut scoped_mixed_type_vars = mixed_type_vars.clone();
            collect_local_types_with_mixed_tracking(
                &body.stmts,
                &mut scoped_locals,
                protected,
                struct_table,
                global_types,
                &mut scoped_mixed_type_vars,
            );
        } else {
            collect_local_types_with_mixed_tracking(
                &body.stmts,
                locals,
                protected,
                struct_table,
                global_types,
                mixed_type_vars,
            );
        }
    });
}

/// Visit every *outermost* `LetBlock` under `expr`, in syntactic order,
/// invoking `f(bindings, body)` for each.
///
/// This is the single traversal shared by the typed pre-scan
/// ([`collect_expr_locals`]) and the name-only capture pre-scan
/// ([`collect_local_binding_names_for_capture`]), so the two consumers cannot
/// drift apart structurally (Issue #5922). Nested `LetBlock`s inside a found
/// `LetBlock`'s binding values or body are *not* visited here — the callback
/// decides how to recurse (the binding values via this visitor again, the body
/// via the statement walker).
fn visit_outermost_letblocks<'e>(
    expr: &'e Expr,
    f: &mut dyn FnMut(&'e [(crate::ir::core::InternedStr, Expr)], &'e Block),
) {
    // Fast path: if there's no LetBlock anywhere in the tree, there's nothing
    // for us to record. This keeps the previous narrow behavior for normal
    // expressions while still letting us reach LetBlocks that appear under
    // any expression form (Issue #3537).
    if !contains_letblock(expr) {
        return;
    }

    // Helper to recurse into a subexpression with the same callback.
    macro_rules! recurse {
        ($e:expr) => {
            visit_outermost_letblocks($e, f)
        };
    }
    match expr {
        Expr::LetBlock { bindings, body, .. } => f(bindings, body),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            recurse!(condition);
            recurse!(then_expr);
            recurse!(else_expr);
        }
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                recurse!(arg);
            }
            for (_, v) in kwargs {
                recurse!(v);
            }
        }
        Expr::Builtin { args, .. } => {
            for arg in args {
                recurse!(arg);
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            recurse!(left);
            recurse!(right);
        }
        Expr::UnaryOp { operand, .. } => {
            recurse!(operand);
        }
        Expr::Index { array, indices, .. } => {
            recurse!(array);
            for idx in indices {
                recurse!(idx);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            recurse!(start);
            if let Some(s) = step {
                recurse!(s);
            }
            recurse!(stop);
        }
        Expr::FieldAccess { object, .. } => {
            recurse!(object);
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for el in elements {
                recurse!(el);
            }
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, v) in fields {
                recurse!(v);
            }
        }
        Expr::Pair { key, value, .. } => {
            recurse!(key);
            recurse!(value);
        }
        Expr::DictLiteral { pairs, .. } => {
            for (k, v) in pairs {
                recurse!(k);
                recurse!(v);
            }
        }
        Expr::StringConcat { parts, .. } => {
            for p in parts {
                recurse!(p);
            }
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            recurse!(body);
            recurse!(iter);
            if let Some(f) = filter {
                recurse!(f);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            recurse!(body);
            for (_, it) in iterations {
                recurse!(it);
            }
            if let Some(f) = filter {
                recurse!(f);
            }
        }
        Expr::New { args, .. } => {
            for a in args {
                recurse!(a);
            }
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(base_expr) = base_expr {
                recurse!(base_expr);
            }
            for a in type_args {
                recurse!(a);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => {
            recurse!(constructor);
        }
        Expr::AssignExpr { value, .. } => {
            recurse!(value);
        }
        Expr::ReturnExpr { value: Some(v), .. } => {
            recurse!(v);
        }
        // Leaf expressions and other variants that do not contain LetBlocks
        // (we already early-exited if no LetBlock is present anywhere).
        _ => {}
    }
}

/// Return `true` iff `expr` syntactically contains an `Expr::LetBlock` anywhere.
/// Used as a fast pre-check to avoid traversing expression trees that contain
/// no statements with assignments (Issue #3537).
pub(crate) fn contains_letblock(expr: &Expr) -> bool {
    match expr {
        Expr::LetBlock { .. } => true,
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            contains_letblock(condition)
                || contains_letblock(then_expr)
                || contains_letblock(else_expr)
        }
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            args.iter().any(contains_letblock) || kwargs.iter().any(|(_, v)| contains_letblock(v))
        }
        Expr::Builtin { args, .. } => args.iter().any(contains_letblock),
        Expr::BinaryOp { left, right, .. } => contains_letblock(left) || contains_letblock(right),
        Expr::UnaryOp { operand, .. } => contains_letblock(operand),
        Expr::Index { array, indices, .. } => {
            contains_letblock(array) || indices.iter().any(contains_letblock)
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            contains_letblock(start)
                || step.as_ref().map(|s| contains_letblock(s)).unwrap_or(false)
                || contains_letblock(stop)
        }
        Expr::FieldAccess { object, .. } => contains_letblock(object),
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            elements.iter().any(contains_letblock)
        }
        Expr::NamedTupleLiteral { fields, .. } => fields.iter().any(|(_, v)| contains_letblock(v)),
        Expr::Pair { key, value, .. } => contains_letblock(key) || contains_letblock(value),
        Expr::DictLiteral { pairs, .. } => pairs
            .iter()
            .any(|(k, v)| contains_letblock(k) || contains_letblock(v)),
        Expr::StringConcat { parts, .. } => parts.iter().any(contains_letblock),
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            contains_letblock(body)
                || contains_letblock(iter)
                || filter
                    .as_ref()
                    .map(|f| contains_letblock(f))
                    .unwrap_or(false)
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            contains_letblock(body)
                || iterations.iter().any(|(_, it)| contains_letblock(it))
                || filter
                    .as_ref()
                    .map(|f| contains_letblock(f))
                    .unwrap_or(false)
        }
        Expr::New { args, .. } => args.iter().any(contains_letblock),
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            base_expr.as_deref().is_some_and(contains_letblock)
                || type_args.iter().any(contains_letblock)
        }
        Expr::QuoteLiteral { constructor, .. } => contains_letblock(constructor),
        Expr::AssignExpr { value, .. } => contains_letblock(value),
        Expr::ReturnExpr { value, .. } => value
            .as_ref()
            .map(|v| contains_letblock(v))
            .unwrap_or(false),
        _ => false,
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

/// Collect global variable types from top-level assignments, with
/// struct-awareness, and record const struct constructors for inlining in
/// functions.
///
/// Issue #6603 (pre-scan retirement 3/3): the global-binding RHS typing — the
/// authority for GLOBAL variable types — is routed through the shared
/// lattice-based abstract-interpretation engine's own expression inference
/// (`infer_expr_result`), then bridged back via `bridge::lattice_to_value_type`
/// (engine injection), mirroring the For/ForEach migration (Issue #6602). This
/// replaces the legacy `infer_value_type_with_structs` pre-scan for this
/// consumer.
///
/// The engine is seeded with the struct table but **no** function table —
/// matching the capability scope of the legacy `infer_value_type_with_structs`
/// pre-scan it replaces (that routine likewise could not resolve user calls).
/// The accumulating `globals` map is fed to the engine **per statement** as the
/// expression's `TypeEnv` (via `type_env_from_locals`) so a global RHS that
/// reads a previously-typed global resolves correctly; the engine is built
/// lazily (only when the first assignment is reached) and threaded through the
/// recursion, so it is constructed at most once per pre-scan.
pub fn collect_global_types_for_inference(
    stmts: &[Stmt],
    globals: &mut HashMap<String, ValueType>,
    struct_table: &StructRegistry,
    const_structs: &mut HashMap<String, (String, usize, usize)>,
) {
    let mut engine: Option<crate::compile::abstract_interp::InferenceEngine> = None;
    collect_global_types_for_inference_impl(
        stmts,
        globals,
        struct_table,
        const_structs,
        &mut engine,
    );
}

/// Recursive worker for [`collect_global_types_for_inference`].
///
/// Carries the lazily-built shared inference `engine` (Issue #6603) so the
/// global-binding RHS typing can use engine injection without rebuilding the
/// engine at every nested block.
fn collect_global_types_for_inference_impl(
    stmts: &[Stmt],
    globals: &mut HashMap<String, ValueType>,
    struct_table: &StructRegistry,
    const_structs: &mut HashMap<String, (String, usize, usize)>,
    engine: &mut Option<crate::compile::abstract_interp::InferenceEngine>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { var, value, .. } => {
                // Issue #6603/#8911: type the RHS through the same assignment
                // seam used by function-body slot inference. The accumulating
                // `globals` map already holds the types of previously seen
                // global assignments, so it serves as the expression
                // environment for this statement. This also preserves the
                // scoped `FunctionRef -> ValueType::Function` shim in
                // `assign_rhs_value_type`; calling the engine directly widens
                // concrete function objects to `Any`, so const aliases such as
                // `const lt = (<:)` were invisible to function-body callable
                // resolution.
                let ty = match value {
                    Expr::Literal(lit, _) => {
                        crate::compile::abstract_interp::local_authority::literal_assignment_value_type(
                            lit,
                        )
                        .unwrap_or_else(|| literal_rhs_value_type(lit, struct_table))
                    }
                    _ => assign_rhs_value_type(
                        value,
                        globals,
                        struct_table,
                        &HashMap::new(),
                        engine,
                    ),
                };
                // Non-const globals can be observed before and after reassignment.
                // If the same global binding is assigned incompatible storage types,
                // do not lock functions compiled against that binding to the final
                // top-level type; use dynamic loads instead (Issue #4285).
                let global_ty = match globals.get(var) {
                    Some(old_ty) if old_ty != &ty => ValueType::Any,
                    Some(old_ty) => old_ty.clone(),
                    None => ty,
                };
                globals.insert(var.clone(), global_ty);

                // Check if this is an empty struct constructor call for const inlining
                // e.g., `const M = MyType()` - only inline when args is empty
                // For non-empty structs like `im = Complex{Bool}(false, true)`, we need
                // to load the actual global value, not inline the constructor
                if let Expr::Call { function, args, .. } = value {
                    if args.is_empty() {
                        if let Some(struct_info) = struct_table.get(function.as_str()) {
                            // Store (struct_name, type_id, field_count) for inlining
                            const_structs.insert(
                                var.clone(),
                                (function.to_string(), struct_info.type_id, 0),
                            );
                        }
                    }
                }
            }
            Stmt::DestructuringAssign { targets, value, .. } => {
                let _ =
                    assign_rhs_value_type(value, globals, struct_table, &HashMap::new(), engine);
                for target in targets {
                    globals.insert(target.clone(), ValueType::Any);
                }
            }
            Stmt::Block(block) => {
                collect_global_types_for_inference_impl(
                    &block.stmts,
                    globals,
                    struct_table,
                    const_structs,
                    engine,
                );
            }
            _ => {}
        }
    }
}

fn is_const_declaration_block(block: &crate::ir::core::Block) -> bool {
    matches!(
        block.stmts.first(),
        Some(Stmt::Expr {
            expr:
                Expr::Call {
                    function,
                    args,
                    ..
                },
            ..
        }) if function == "#__sjulia_declare_const__" && args.len() == 1
    )
}

/// Widen non-const top-level bindings before reflection/type-stability inference.
///
/// The compiler still keeps precise `global_types` for code generation and
/// runtime global loads, but Julia's inference treats ordinary global bindings
/// conservatively because they can be rebound without invalidating every reader.
/// Const declarations are lowered as marker blocks and remain precise.
pub fn widen_non_const_globals_for_binding_inference(
    stmts: &[Stmt],
    globals: &mut HashMap<String, ValueType>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { var, .. } => {
                globals.insert(var.clone(), ValueType::Any);
            }
            Stmt::DestructuringAssign { targets, .. } => {
                globals.extend(
                    targets
                        .iter()
                        .cloned()
                        .map(|target| (target, ValueType::Any)),
                );
            }
            Stmt::Block(block) if is_const_declaration_block(block) => {}
            Stmt::Block(block) => {
                widen_non_const_globals_for_binding_inference(&block.stmts, globals);
            }
            _ => {}
        }
    }
}

/// Build a shared `InferenceEngine` pre-populated with the struct table and all
/// program functions. Creating the engine once and reusing it across the entire
/// function compilation loop avoids O(n^2) function cloning and struct-table
/// conversion from rebuilding one-shot inference engines inside the loop.
///
/// The returned engine can be used with `engine.infer_function(func)` and its
/// return-type cache is shared across calls, further reducing redundant work.
fn build_shared_inference_engine_empty(
    struct_table: &StructRegistry,
    global_types: &HashMap<String, ValueType>,
) -> crate::compile::abstract_interp::InferenceEngine {
    use crate::compile::abstract_interp::{InferenceEngine, StructTypeInfo};
    use crate::compile::lattice::types::LatticeType;
    use crate::runtime_types::bridge::value_type_to_lattice_with_struct_table;
    use std::collections::HashMap as StdHashMap;

    // Convert StructInfo to StructTypeInfo (done once)
    let lattice_struct_table: StdHashMap<String, StructTypeInfo> = struct_table
        .iter()
        .map(|(name, info)| {
            let fields_map: StdHashMap<String, LatticeType> = info
                .fields
                .iter()
                .map(|(fname, ftype)| {
                    let lattice_type = value_type_to_lattice_with_struct_table(ftype, struct_table);
                    (fname.clone(), lattice_type)
                })
                .collect();

            (
                name.clone(),
                StructTypeInfo {
                    type_id: info.type_id,
                    is_mutable: info.is_mutable,
                    field_order: info
                        .fields
                        .iter()
                        .map(|(field_name, _)| field_name.clone())
                        .collect(),
                    fields: fields_map,
                    has_inner_constructor: info.has_inner_constructor,
                },
            )
        })
        .collect();

    // Create engine and clone all functions once
    let lattice_global_types: StdHashMap<String, LatticeType> = global_types
        .iter()
        .map(|(name, ty)| {
            (
                name.clone(),
                value_type_to_lattice_with_struct_table(ty, struct_table),
            )
        })
        .collect();

    let mut engine = InferenceEngine::with_struct_table(lattice_struct_table);
    engine.set_global_types(lattice_global_types);
    engine
}

pub fn build_shared_inference_engine<'a>(
    struct_table: &StructRegistry,
    global_types: &HashMap<String, ValueType>,
    all_functions: impl IntoIterator<Item = &'a Function>,
) -> crate::compile::abstract_interp::InferenceEngine {
    let mut engine = build_shared_inference_engine_empty(struct_table, global_types);
    for f in all_functions {
        engine.add_function(f.clone());
    }

    engine
}

pub fn build_shared_inference_engine_owned(
    struct_table: &StructRegistry,
    global_types: &HashMap<String, ValueType>,
    all_functions: impl IntoIterator<Item = Function>,
) -> crate::compile::abstract_interp::InferenceEngine {
    let mut engine = build_shared_inference_engine_empty(struct_table, global_types);
    engine.add_functions(all_functions);
    engine
}

/// Like [`build_shared_inference_engine_owned`], but seeds the Base+prelude
/// portion of the function table from a precomputed `(function_table,
/// ambiguous_functions)` snapshot instead of re-inserting each of those
/// functions through `add_function` (Issue #10114).
///
/// `prefetched_function_table`/`prefetched_ambiguous_functions` must have
/// been built by `abstract_interp::engine::build_function_table` over
/// exactly the same functions, in the same order, that `add_functions` would
/// otherwise see for that prefix — see
/// `compile::cache::take_prefetched_base_function_table`, which is the only
/// production caller and upholds that invariant. `suffix_functions` are the
/// remaining (non-prefetched) functions, added normally.
pub fn build_shared_inference_engine_owned_with_prefetched_base(
    struct_table: &StructRegistry,
    global_types: &HashMap<String, ValueType>,
    prefetched_function_table: HashMap<String, Function>,
    prefetched_ambiguous_functions: HashSet<String>,
    suffix_functions: impl IntoIterator<Item = Function>,
) -> crate::compile::abstract_interp::InferenceEngine {
    let mut engine = build_shared_inference_engine_empty(struct_table, global_types);
    engine.seed_function_table(prefetched_function_table, prefetched_ambiguous_functions);
    engine.add_functions(suffix_functions);
    engine
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::context::StructInfo;
    use crate::ir::core::{BinaryOp, Expr, Literal, Stmt};
    use crate::runtime_types::ValueType;
    use crate::span::Span;
    use crate::types::JuliaType;

    fn span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn int_lit(v: i64) -> Expr {
        Expr::Literal(Literal::Int(v), span())
    }

    fn float_lit(v: f64) -> Expr {
        Expr::Literal(Literal::Float(v), span())
    }

    fn var_expr(name: &str) -> Expr {
        Expr::Var(name.to_string().into(), span())
    }

    fn function_ref(name: &str) -> Expr {
        Expr::FunctionRef {
            name: name.to_string().into(),
            span: span(),
        }
    }

    fn call_expr(function: &str, args: Vec<Expr>) -> Expr {
        Expr::Call {
            function: function.to_string().into(),
            args,
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span: span(),
        }
    }

    fn binary_expr(op: BinaryOp, left: Expr, right: Expr) -> Expr {
        Expr::BinaryOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
            span: span(),
        }
    }

    #[test]
    fn test_shared_engine_getfield_preserves_union_field_issue_4270() {
        let mut struct_table = StructRegistry::new();
        struct_table.insert(
            "Box4270".to_string(),
            StructInfo {
                type_id: 1,
                is_mutable: false,
                fields: vec![(
                    "value".to_string(),
                    ValueType::Union(vec![ValueType::I64, ValueType::Nothing]),
                )],
                has_inner_constructor: false,
            },
        );
        let func = Function {
            name: "field_get_4270".to_string(),
            params: vec![crate::ir::core::TypedParam {
                name: "x".to_string(),
                type_annotation: Some(JuliaType::Struct("Box4270".to_string())),
                is_varargs: false,
                vararg_count: None,
                span: span(),
            }],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: crate::ir::core::Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Call {
                        function: "getfield".to_string().into(),
                        args: vec![
                            var_expr("x"),
                            Expr::QuoteLiteral {
                                constructor: Box::new(Expr::Builtin {
                                    name: crate::ir::core::BuiltinOp::SymbolNew,
                                    args: vec![Expr::Literal(
                                        Literal::Str("value".to_string()),
                                        span(),
                                    )],
                                    span: span(),
                                }),
                                span: span(),
                            },
                        ],
                        kwargs: vec![],
                        kwargs_splat_mask: vec![],
                        splat_mask: vec![false, false],
                        span: span(),
                    }),
                    span: span(),
                }],
                span: span(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: span(),
            new_struct_name: None,
        };
        let mut engine = build_shared_inference_engine(&struct_table, &HashMap::new(), [&func]);
        let result = engine.infer_function(&func);
        assert!(
            matches!(
                result,
                crate::compile::lattice::types::LatticeType::Union(_)
            ),
            "expected union return, got {:?}",
            result
        );
    }

    fn assign_stmt(var: &str, value: Expr) -> Stmt {
        Stmt::Assign {
            var: var.to_string(),
            value,
            span: span(),
        }
    }

    // ── Issue #5922: literal locals routed through the shared authority ──────

    /// Pin that the live function-body pre-scan types each literal assignment
    /// RHS as the expected concrete `ValueType`. Migrated scalar/Symbol classes
    /// flow through the shared authority
    /// (`local_authority::literal_assignment_value_type`); the deferred
    /// array/module classes (authority returns `None`) flow through the driver's
    /// `literal_rhs_value_type`. Both must keep typing literals *precisely* — the
    /// double-inference removal (Issue #5922) and the pre-scan retirement
    /// (Issue #6601) leave the live pre-scan output for literals unchanged.
    #[test]
    fn literal_assignment_prescan_matches_shared_authority_issue_5922() {
        let struct_table: StructRegistry = StructRegistry::new();
        let global_types: HashMap<String, ValueType> = HashMap::new();
        let protected: HashSet<String> = HashSet::new();

        // Each literal paired with the precise `ValueType` the live pre-scan must
        // produce for it. Scalars/Symbol come from the shared authority's
        // migrated table (mirrors `local_authority::migrated_literals_match_legacy_value_types`);
        // the two deferred classes come from `literal_rhs_value_type`.
        let cases: Vec<(Expr, ValueType)> = vec![
            (Expr::Literal(Literal::Int(7), span()), ValueType::I64),
            (Expr::Literal(Literal::Int128(7), span()), ValueType::I128),
            (
                Expr::Literal(Literal::BigInt("7".to_string()), span()),
                ValueType::BigInt,
            ),
            (
                Expr::Literal(Literal::BigFloat("1.0".to_string()), span()),
                ValueType::BigFloat,
            ),
            (Expr::Literal(Literal::Float(1.5), span()), ValueType::F64),
            (Expr::Literal(Literal::Float32(1.5), span()), ValueType::F32),
            (Expr::Literal(Literal::Bool(true), span()), ValueType::Bool),
            (
                Expr::Literal(Literal::Str("hi".to_string()), span()),
                ValueType::Str,
            ),
            (Expr::Literal(Literal::Char('a'), span()), ValueType::Char),
            (Expr::Literal(Literal::Nothing, span()), ValueType::Nothing),
            (Expr::Literal(Literal::Missing, span()), ValueType::Missing),
            (
                Expr::Literal(Literal::Symbol("foo".to_string()), span()),
                ValueType::Symbol,
            ),
            // Deferred classes (authority returns None -> driver's
            // `literal_rhs_value_type`):
            (
                Expr::Literal(Literal::ArrayI64(vec![1, 2], vec![2]), span()),
                ValueType::ArrayOf(ArrayElementType::I64, None),
            ),
            (
                Expr::Literal(Literal::Module("Base".to_string()), span()),
                ValueType::Module,
            ),
        ];

        for (rhs, expected) in cases {
            // Run the live pre-scan over a single assignment and read back the
            // local's type.
            let stmts = vec![assign_stmt("x", rhs.clone())];
            let mut locals: HashMap<String, ValueType> = HashMap::new();
            let mut mixed: HashSet<String> = HashSet::new();
            collect_local_types_with_mixed_tracking(
                &stmts,
                &mut locals,
                &protected,
                &struct_table,
                &global_types,
                &mut mixed,
            );

            assert_eq!(
                locals.get("x"),
                Some(&expected),
                "live pre-scan literal local for {:?} must be {:?}",
                rhs,
                expected
            );
        }
    }

    // ── Issue #6601: function-body slot-typing pre-scan characterization ─────

    /// Run the live function-body pre-scan over `stmts` (no protected params,
    /// optional seeded locals / globals / struct table) and return the resulting
    /// `(locals, mixed_type_vars)`. This pins the *load-bearing* slot-typing
    /// behavior the function-body / inner-ctor / main consumers depend on
    /// (Issue #6601): the whole-body widened slot type read before the first
    /// `Store` is emitted (forward references), and the `mixed_type_vars` set
    /// that decides `StoreAny`/`LoadAny` dynamic slots.
    fn prescan_slots(
        seed_locals: &[(&str, ValueType)],
        seed_globals: &[(&str, ValueType)],
        struct_table: &StructRegistry,
        stmts: &[Stmt],
    ) -> (HashMap<String, ValueType>, HashSet<String>) {
        let global_types: HashMap<String, ValueType> = seed_globals
            .iter()
            .map(|(n, t)| (n.to_string(), t.clone()))
            .collect();
        let protected: HashSet<String> = HashSet::new();
        let mut locals: HashMap<String, ValueType> = seed_locals
            .iter()
            .map(|(n, t)| (n.to_string(), t.clone()))
            .collect();
        let mut mixed: HashSet<String> = HashSet::new();
        collect_local_types_with_mixed_tracking(
            stmts,
            &mut locals,
            &protected,
            struct_table,
            &global_types,
            &mut mixed,
        );
        (locals, mixed)
    }

    #[test]
    fn assign_rhs_value_type_resolves_local_var_issue_6601() {
        let struct_table: StructRegistry = StructRegistry::new();
        let globals: HashMap<String, ValueType> = HashMap::new();
        let mut locals: HashMap<String, ValueType> = HashMap::new();
        locals.insert("a".to_string(), ValueType::I64);
        let mut engine: Option<crate::compile::abstract_interp::InferenceEngine> = None;
        let value = var_expr("a");
        let got = assign_rhs_value_type(&value, &locals, &struct_table, &globals, &mut engine);
        // A `Var` resolving to a seeded local keeps that local's concrete type.
        assert_eq!(got, ValueType::I64);
    }

    /// Engine-value pin (Issue #6601, Var slice): for a `Var` that resolves to a
    /// seeded local, the engine path returns that local's exact `ValueType`. This
    /// is the round-trip guarantee behind routing the `Var` class onto the engine.
    #[test]
    fn prescan_engine_equiv_var_resolved_local_issue_6601() {
        let struct_table: StructRegistry = StructRegistry::new();
        let globals: HashMap<String, ValueType> = HashMap::new();
        for vt in [ValueType::I64, ValueType::F64, ValueType::Bool] {
            let mut locals: HashMap<String, ValueType> = HashMap::new();
            locals.insert("a".to_string(), vt.clone());
            let mut engine = None;
            let value = var_expr("a");
            assert_eq!(
                assign_rhs_value_type_via_engine(
                    &value,
                    &locals,
                    &struct_table,
                    &globals,
                    &mut engine
                ),
                vt.clone(),
                "engine must resolve local Var to its seeded type {vt:?}",
            );
        }
    }

    /// Migration pin (Issue #6601, UnaryOp slice): drive the `UnaryOp`
    /// Assign-RHS class through the engine path and assert the upstream-correct
    /// concrete result for every operand class. `!` always yields `Bool`, and
    /// `-` (negation) preserves the operand's concrete type (including
    /// `ComplexF64`). The shared engine previously under-approximated `!i` and
    /// `-c` to `Any`; this pin guards the fix.
    #[test]
    fn prescan_engine_equiv_unaryop_issue_6601() {
        use crate::ir::core::UnaryOp;
        let struct_table: StructRegistry = StructRegistry::new();
        let globals: HashMap<String, ValueType> = HashMap::new();
        let mut locals: HashMap<String, ValueType> = HashMap::new();
        locals.insert("i".to_string(), ValueType::I64);
        locals.insert("f".to_string(), ValueType::F64);
        locals.insert("c".to_string(), ValueType::ComplexF64);
        let cases: Vec<(Expr, ValueType)> = vec![
            (unary_expr(UnaryOp::Not, var_expr("i")), ValueType::Bool), // !i  -> Bool
            (
                unary_expr(UnaryOp::Neg, var_expr("c")),
                ValueType::ComplexF64,
            ), // -c  -> ComplexF64
            (unary_expr(UnaryOp::Neg, var_expr("i")), ValueType::I64),  // -i  -> I64
            (unary_expr(UnaryOp::Neg, var_expr("f")), ValueType::F64),  // -f  -> F64
        ];
        for (value, expected) in cases {
            let mut engine = None;
            assert_eq!(
                assign_rhs_value_type_via_engine(
                    &value,
                    &locals,
                    &struct_table,
                    &globals,
                    &mut engine
                ),
                expected,
                "UnaryOp engine value diverged for {value:?}",
            );
        }
    }

    /// The seam (not the raw engine path) must preserve legacy bare-pi F64.
    /// The empty-table engine has no pi special-case, so flipping `Var` onto
    /// the engine would regress bare `pi`/`π` to `Any` unless the seam keeps
    /// the legacy F64 guard (Issue #6601).
    #[test]
    fn prescan_var_pi_keeps_f64_through_seam_issue_6601() {
        let struct_table: StructRegistry = StructRegistry::new();
        let globals: HashMap<String, ValueType> = HashMap::new();
        let locals: HashMap<String, ValueType> = HashMap::new();
        for name in ["pi", "π"] {
            let mut engine = None;
            let value = var_expr(name);
            assert_eq!(
                assign_rhs_value_type(&value, &locals, &struct_table, &globals, &mut engine),
                ValueType::F64,
                "seam must keep bare {name} typed as F64",
            );
        }
    }

    /// A `FunctionRef` Assign-RHS slot (`f = sin`) is typed without the legacy
    /// pre-scan path. Legacy returns `ValueType::Function` *unconditionally*
    /// (`infer_value_type_with_structs` -> `Expr::FunctionRef { .. } =>
    /// ValueType::Function`), independent of the function name, locals, globals,
    /// or struct_table. Routing FunctionRef through the shared engine would
    /// instead widen to `ValueType::Any` (the bridge maps
    /// `ConcreteType::Function -> ValueType::Any`), so the seam reproduces the
    /// legacy `Function` result with a scoped shim rather than changing the
    /// global bridge. This pin guards that the seam keeps matching legacy across
    /// the migration (Issue #6601).
    #[test]
    fn prescan_funcref_matches_legacy_through_seam_issue_6601() {
        let struct_table: StructRegistry = StructRegistry::new();
        let globals: HashMap<String, ValueType> = HashMap::new();
        let locals: HashMap<String, ValueType> = HashMap::new();
        let mut engine = None;
        for name in ["sin", "cos", "my_user_fn", "+"] {
            let value = function_ref(name);
            assert_eq!(
                assign_rhs_value_type(&value, &locals, &struct_table, &globals, &mut engine),
                ValueType::Function,
                "seam must type FunctionRef {name} as Function",
            );
        }
    }

    /// Forward reference: `s = 0; s = s + 1.5` — the first store for `s` is
    /// emitted *after* the pre-scan has seen the whole body, so the slot type
    /// read at the first `Store` is the widened result of `I64` (`s=0`) joined
    /// with `F64` (`s + 1.5`). Under [`widen_type`] an `I64`→`F64` sequence
    /// joins to `Any` (a dynamic slot, NOT a narrow `F64`), and because the
    /// reassignment RHS is a compound expression (not a *direct* numeric
    /// literal) the variable is NOT added to `mixed_type_vars`. This is the
    /// canonical forward-reference case the function-body pre-scan exists to
    /// resolve before codegen (PR #6540 body, class (b)); pinning the exact
    /// `(Any, not-mixed)` outcome guards it across any internal migration.
    #[test]
    fn prescan_forward_ref_widens_int_then_float_issue_6601() {
        let struct_table: StructRegistry = StructRegistry::new();
        let stmts = vec![
            assign_stmt("s", int_lit(0)),
            assign_stmt(
                "s",
                binary_expr(BinaryOp::Add, var_expr("s"), float_lit(1.5)),
            ),
        ];
        let (locals, mixed) = prescan_slots(&[], &[], &struct_table, &stmts);
        // Whole-body widened slot type is the dynamic `Any` (I64 ⊔ F64).
        assert_eq!(locals.get("s"), Some(&ValueType::Any));
        // A compound `s = s + 1.5` is NOT a direct-literal reassignment, so it
        // does NOT mark `s` mixed (the slot is dynamic by widening, not by the
        // direct-literal F64/I64 rule).
        assert!(!mixed.contains("s"));
    }

    /// A *pure* forward float reassignment with no prior integer type — `s` is
    /// first written `F64` then `F64` again — keeps the narrow `F64` slot and
    /// stays non-dynamic. Pins that the pre-scan does not over-widen a stable
    /// numeric slot (Issue #6601).
    #[test]
    fn prescan_forward_ref_stable_float_slot_issue_6601() {
        let struct_table: StructRegistry = StructRegistry::new();
        let stmts = vec![
            assign_stmt("s", float_lit(0.0)),
            assign_stmt(
                "s",
                binary_expr(BinaryOp::Add, var_expr("s"), float_lit(1.5)),
            ),
        ];
        let (locals, mixed) = prescan_slots(&[], &[], &struct_table, &stmts);
        assert_eq!(locals.get("s"), Some(&ValueType::F64));
        assert!(!mixed.contains("s"));
    }

    /// Direct mixed `F64` then `I64` (and vice versa) literal reassignment must
    /// flag the variable in `mixed_type_vars` (→ dynamic `StoreAny`/`LoadAny`
    /// slot), so `die = 7.0; die = 6` keeps `typeof(die) == Int64` at runtime.
    /// Pins the F64+I64 mixed-tracking contract (Issue #6601).
    #[test]
    fn prescan_mixed_float_int_literal_marks_dynamic_issue_6601() {
        let struct_table: StructRegistry = StructRegistry::new();

        let f_then_i = vec![
            assign_stmt("die", float_lit(7.0)),
            assign_stmt("die", int_lit(6)),
        ];
        let (locals, mixed) = prescan_slots(&[], &[], &struct_table, &f_then_i);
        assert!(
            mixed.contains("die"),
            "F64-then-I64 direct literal reassignment must be dynamic"
        );
        assert_eq!(locals.get("die"), Some(&ValueType::Any));

        let i_then_f = vec![
            assign_stmt("x", int_lit(6)),
            assign_stmt("x", float_lit(7.0)),
        ];
        let (locals, mixed) = prescan_slots(&[], &[], &struct_table, &i_then_f);
        assert!(
            mixed.contains("x"),
            "I64-then-F64 direct literal reassignment must be dynamic"
        );
        assert_eq!(locals.get("x"), Some(&ValueType::Any));
    }

    /// Issue #4285 / #3535: a local that widens to `Any` because of an
    /// *incompatible non-numeric* reassignment (e.g. `Int64` then `String`)
    /// must be flagged dynamic so every assignment compiles to `StoreAny`.
    /// Pins the reassignment-to-Any widening contract (Issue #6601).
    #[test]
    fn prescan_incompatible_reassignment_marks_dynamic_issue_6601() {
        let struct_table: StructRegistry = StructRegistry::new();
        let stmts = vec![
            assign_stmt("v", int_lit(1)),
            assign_stmt("v", Expr::Literal(Literal::Str("s".to_string()), span())),
        ];
        let (locals, mixed) = prescan_slots(&[], &[], &struct_table, &stmts);
        assert_eq!(locals.get("v"), Some(&ValueType::Any));
        assert!(
            mixed.contains("v"),
            "Int64-then-String reassignment must mark the slot dynamic (#4285/#3535)"
        );
    }

    /// An `I64`→`F64` numeric reassignment via a compound RHS (`acc = acc / 2`,
    /// `Div → F64`) widens the slot to the dynamic `Any` (the `I64 ⊔ F64`
    /// join), but does NOT flag the variable in `mixed_type_vars`: the
    /// direct-literal F64/I64 rule does not apply to a compound RHS, and the
    /// incompatible-non-numeric rule does not apply to two numeric types. Pins
    /// the "compound numeric I64→F64 widens to Any, stays non-mixed" boundary
    /// (Issue #6601).
    #[test]
    fn prescan_numeric_widening_not_dynamic_issue_6601() {
        let struct_table: StructRegistry = StructRegistry::new();
        // acc = 0; acc = acc / 2   (Div → F64, a compound non-literal RHS)
        let stmts = vec![
            assign_stmt("acc", int_lit(0)),
            assign_stmt(
                "acc",
                binary_expr(BinaryOp::Div, var_expr("acc"), int_lit(2)),
            ),
        ];
        let (locals, mixed) = prescan_slots(&[], &[], &struct_table, &stmts);
        assert_eq!(locals.get("acc"), Some(&ValueType::Any));
        assert!(
            !mixed.contains("acc"),
            "compound numeric I64→F64 widening must NOT be flagged dynamic"
        );
    }

    /// Forward reference through a global const read: `s = 0; s = s + g` where
    /// `g::Float64` is a global. The RHS resolves `g` from the seeded globals
    /// (→ `F64`), so the second store types `F64`, which joins with the prior
    /// `I64` to the dynamic `Any` slot. Pins that the pre-scan resolves globals
    /// when typing a forward-ref RHS and that the I64⊔F64 join is `Any`
    /// (Issue #6601).
    #[test]
    fn prescan_forward_ref_through_global_issue_6601() {
        let struct_table: StructRegistry = StructRegistry::new();
        let stmts = vec![
            assign_stmt("s", int_lit(0)),
            assign_stmt(
                "s",
                binary_expr(BinaryOp::Add, var_expr("s"), var_expr("g")),
            ),
        ];
        let (locals, _mixed) = prescan_slots(&[], &[("g", ValueType::F64)], &struct_table, &stmts);
        // The RHS being typed F64 (not Any) proves the global `g` was resolved;
        // I64 ⊔ F64 widens the slot to Any.
        assert_eq!(locals.get("s"), Some(&ValueType::Any));
    }

    /// A forward reference through a global where the global keeps the slot
    /// stable: `s = 0.0; s = s + g` with `g::Float64` resolves `g` to `F64`,
    /// so the slot stays the narrow `F64` (no I64 in the mix). This proves the
    /// global was resolved (an unresolved `g` would make the RHS `Any`) AND
    /// that a stable-numeric forward ref keeps its narrow slot (Issue #6601).
    #[test]
    fn prescan_forward_ref_through_global_stable_issue_6601() {
        let struct_table: StructRegistry = StructRegistry::new();
        let stmts = vec![
            assign_stmt("s", float_lit(0.0)),
            assign_stmt(
                "s",
                binary_expr(BinaryOp::Add, var_expr("s"), var_expr("g")),
            ),
        ];
        let (locals, _mixed) = prescan_slots(&[], &[("g", ValueType::F64)], &struct_table, &stmts);
        assert_eq!(locals.get("s"), Some(&ValueType::F64));
    }

    // ── Issue #6602: For/ForEach loop-var typing via engine injection ────────

    /// Helper: run the live pre-scan over a single loop statement and read the
    /// inferred loop-variable type back out of the locals map.
    fn loop_var_type(seed: &[(&str, ValueType)], loop_stmt: Stmt) -> Option<ValueType> {
        let struct_table: StructRegistry = StructRegistry::new();
        let global_types: HashMap<String, ValueType> = HashMap::new();
        let protected: HashSet<String> = HashSet::new();
        let mut locals: HashMap<String, ValueType> = seed
            .iter()
            .map(|(n, t)| (n.to_string(), t.clone()))
            .collect();
        let mut mixed: HashSet<String> = HashSet::new();
        collect_local_types_with_mixed_tracking(
            &[loop_stmt],
            &mut locals,
            &protected,
            &struct_table,
            &global_types,
            &mut mixed,
        );
        locals.get("__loopvar__").cloned()
    }

    fn empty_block() -> crate::ir::core::Block {
        crate::ir::core::Block {
            stmts: vec![],
            span: span(),
        }
    }

    fn for_stmt(start: Expr, end: Expr, step: Option<Expr>) -> Stmt {
        Stmt::For {
            var: "__loopvar__".to_string(),
            start,
            end,
            step,
            body: empty_block(),
            span: span(),
        }
    }

    fn foreach_stmt(iterable: Expr) -> Stmt {
        Stmt::ForEach {
            var: "__loopvar__".to_string(),
            iterable,
            body: empty_block(),
            span: span(),
        }
    }

    /// Pin the `Stmt::For` loop-variable typing now produced by the engine
    /// injection (Issue #6602): numeric integer / float / mixed ranges promote
    /// exactly as before, so loop-variable type-preservation is unchanged for
    /// the load-bearing cases (`for i in 1:n`, `for x in 1.0:0.5:2.0`).
    #[test]
    fn for_loopvar_numeric_ranges_via_engine_issue_6602() {
        // for i in 1:10  →  Int64
        assert_eq!(
            loop_var_type(&[], for_stmt(int_lit(1), int_lit(10), None)),
            Some(ValueType::I64),
        );
        // for x in 1.0:2.0  →  Float64
        assert_eq!(
            loop_var_type(&[], for_stmt(float_lit(1.0), float_lit(2.0), None)),
            Some(ValueType::F64),
        );
        // for x in 1.0:0.5:2.0  →  Float64 (Issue #3518 promotion preserved)
        assert_eq!(
            loop_var_type(
                &[],
                for_stmt(float_lit(1.0), float_lit(2.0), Some(float_lit(0.5)))
            ),
            Some(ValueType::F64),
        );
        // Mixed Int/Float endpoints promote to Float64.
        assert_eq!(
            loop_var_type(&[], for_stmt(int_lit(1), float_lit(2.0), None)),
            Some(ValueType::F64),
        );
        // Endpoint read from a previously-typed local: for i in 1:n where n::Int.
        assert_eq!(
            loop_var_type(
                &[("n", ValueType::I64)],
                for_stmt(int_lit(1), var_expr("n"), None)
            ),
            Some(ValueType::I64),
        );
        // for x in 1:y where y::Float64 promotes to Float64.
        assert_eq!(
            loop_var_type(
                &[("y", ValueType::F64)],
                for_stmt(int_lit(1), var_expr("y"), None)
            ),
            Some(ValueType::F64),
        );
    }

    /// Pin the `Stmt::ForEach` loop-variable typing now produced by the engine
    /// injection (Issue #6602): iterating a typed array local yields the element
    /// type, and iterating a String yields Char — the same downstream
    /// `loop_analysis::element_type` the engine uses.
    #[test]
    fn foreach_loopvar_element_type_via_engine_issue_6602() {
        // for v in arr where arr::Vector{Int64}  →  Int64
        assert_eq!(
            loop_var_type(
                &[("arr", ValueType::ArrayOf(ArrayElementType::I64, None))],
                foreach_stmt(var_expr("arr")),
            ),
            Some(ValueType::I64),
        );
        // for v in arr where arr::Vector{Float64}  →  Float64
        assert_eq!(
            loop_var_type(
                &[("arr", ValueType::ArrayOf(ArrayElementType::F64, None))],
                foreach_stmt(var_expr("arr")),
            ),
            Some(ValueType::F64),
        );
        // for c in s where s::String  →  Char
        assert_eq!(
            loop_var_type(&[("s", ValueType::Str)], foreach_stmt(var_expr("s")),),
            Some(ValueType::Char),
        );
    }

    // ── Issue #6603: global type typing via engine injection ────────────────

    /// Helper: run `collect_global_types_for_inference` over `stmts` with the
    /// given struct table and read back the resulting global-type map plus the
    /// const-struct map. This drives the live pre-scan retirement consumer.
    fn run_global_types(
        stmts: &[Stmt],
        struct_table: &StructRegistry,
    ) -> (
        HashMap<String, ValueType>,
        HashMap<String, (String, usize, usize)>,
    ) {
        let mut globals: HashMap<String, ValueType> = HashMap::new();
        let mut const_structs: HashMap<String, (String, usize, usize)> = HashMap::new();
        collect_global_types_for_inference(stmts, &mut globals, struct_table, &mut const_structs);
        (globals, const_structs)
    }

    /// Pin the scalar-literal global typing (`x = 1`, `y = 1.5`, `s = "hi"`,
    /// `b = true`) that `collect_global_types_for_inference` produces. Engine
    /// injection (Issue #6603) must keep these load-bearing global types stable.
    #[test]
    fn global_literal_types_via_engine_issue_6603() {
        let struct_table: StructRegistry = StructRegistry::new();
        let stmts = vec![
            assign_stmt("x", int_lit(1)),
            assign_stmt("y", float_lit(1.5)),
            assign_stmt("s", Expr::Literal(Literal::Str("hi".to_string()), span())),
            assign_stmt("b", Expr::Literal(Literal::Bool(true), span())),
        ];
        let (globals, _) = run_global_types(&stmts, &struct_table);
        assert_eq!(globals.get("x"), Some(&ValueType::I64));
        assert_eq!(globals.get("y"), Some(&ValueType::F64));
        assert_eq!(globals.get("s"), Some(&ValueType::Str));
        assert_eq!(globals.get("b"), Some(&ValueType::Bool));
    }

    /// Pin that a global whose binding is reassigned to an *incompatible* type
    /// widens to `Any` (Issue #4285 — non-const globals observed before/after
    /// reassignment must not lock readers to the final concrete type), while a
    /// global reassigned to the *same* type keeps that concrete type.
    #[test]
    fn global_reassignment_widens_to_any_issue_6603() {
        let struct_table: StructRegistry = StructRegistry::new();
        // g = 1; g = 2.0  → Any (Int64 vs Float64 storage mismatch).
        let mixed = vec![
            assign_stmt("g", int_lit(1)),
            assign_stmt("g", float_lit(2.0)),
        ];
        let (globals, _) = run_global_types(&mixed, &struct_table);
        assert_eq!(globals.get("g"), Some(&ValueType::Any));

        // h = 1; h = 2  → Int64 (same storage type preserved).
        let same = vec![assign_stmt("h", int_lit(1)), assign_stmt("h", int_lit(2))];
        let (globals2, _) = run_global_types(&same, &struct_table);
        assert_eq!(globals2.get("h"), Some(&ValueType::I64));
    }

    /// Pin struct-constructor global typing and the const-struct inlining map:
    /// `m = MyType()` (empty ctor) types the global as `Struct(type_id)` AND
    /// records `(name, type_id, 0)` in `const_structs` for const inlining.
    #[test]
    fn global_struct_ctor_types_and_const_struct_issue_6603() {
        let mut struct_table: StructRegistry = StructRegistry::new();
        struct_table.insert(
            "MyType6603".to_string(),
            StructInfo {
                type_id: 42,
                is_mutable: false,
                fields: vec![],
                has_inner_constructor: false,
            },
        );
        let stmts = vec![assign_stmt("m", call_expr("MyType6603", vec![]))];
        let (globals, const_structs) = run_global_types(&stmts, &struct_table);
        assert_eq!(globals.get("m"), Some(&ValueType::Struct(42)));
        assert_eq!(
            const_structs.get("m"),
            Some(&("MyType6603".to_string(), 42, 0)),
        );
    }

    /// Pin that a global RHS reading another previously-typed global resolves
    /// through the accumulating global map: `a = 1; b = a` types `b` as the
    /// type of `a`. The accumulating `globals` map is the global lookup, so the
    /// engine env must be seeded from it for each statement.
    #[test]
    fn global_rhs_reads_prior_global_issue_6603() {
        let struct_table: StructRegistry = StructRegistry::new();
        let stmts = vec![
            assign_stmt("a", int_lit(7)),
            assign_stmt("b", var_expr("a")),
        ];
        let (globals, _) = run_global_types(&stmts, &struct_table);
        assert_eq!(globals.get("a"), Some(&ValueType::I64));
        assert_eq!(globals.get("b"), Some(&ValueType::I64));
    }

    /// Pin nested-block global collection: assignments inside a `Stmt::Block`
    /// still register in the global map (the pre-scan recurses into blocks).
    #[test]
    fn global_nested_block_collection_issue_6603() {
        let struct_table: StructRegistry = StructRegistry::new();
        let stmts = vec![Stmt::Block(crate::ir::core::Block {
            stmts: vec![assign_stmt("nested", int_lit(3))],
            span: span(),
        })];
        let (globals, _) = run_global_types(&stmts, &struct_table);
        assert_eq!(globals.get("nested"), Some(&ValueType::I64));
    }

    // ── Issue #5922: name-only capture pre-scan mirrors the typed pre-scan ──

    /// Pin that [`collect_local_binding_names_for_capture`] (the name-only
    /// walker used by module-level lambda capture analysis) produces exactly
    /// the binding-name set the typed pre-scan would have produced as its key
    /// set, across every statement class the typed pre-scan handles —
    /// including the scoping rules (testset bodies do not escape, LetBlock
    /// bodies do). If either walker's traversal changes, this test must be
    /// revisited deliberately.
    #[test]
    fn capture_binding_names_match_typed_prescan_keys_issue_5922() {
        let block = |stmts: Vec<Stmt>| crate::ir::core::Block {
            stmts,
            span: span(),
        };

        let stmts = vec![
            assign_stmt("a", int_lit(1)),
            Stmt::For {
                var: "i".to_string(),
                start: int_lit(1),
                end: int_lit(10),
                step: None,
                body: block(vec![assign_stmt(
                    "acc",
                    binary_expr(BinaryOp::Add, var_expr("acc"), var_expr("i")),
                )]),
                span: span(),
            },
            Stmt::ForEach {
                var: "e".to_string(),
                iterable: var_expr("a"),
                body: block(vec![assign_stmt("seen", var_expr("e"))]),
                span: span(),
            },
            Stmt::While {
                condition: Expr::Literal(Literal::Bool(true), span()),
                body: block(vec![assign_stmt("w", float_lit(2.0))]),
                span: span(),
            },
            Stmt::If {
                condition: Expr::Literal(Literal::Bool(true), span()),
                then_branch: block(vec![assign_stmt("t_only", int_lit(1))]),
                else_branch: Some(block(vec![assign_stmt("e_only", float_lit(1.0))])),
                span: span(),
            },
            Stmt::Try {
                try_block: block(vec![assign_stmt("try_v", int_lit(1))]),
                catch_var: None,
                catch_block: Some(block(vec![assign_stmt("catch_v", int_lit(2))])),
                else_block: None,
                finally_block: Some(block(vec![assign_stmt("fin_v", int_lit(3))])),
                span: span(),
            },
            // @testset body: names must NOT escape either collector
            // (Issue #5588 scoping).
            Stmt::TestSet {
                name: "ts".to_string(),
                body: block(vec![assign_stmt("ts_local", int_lit(1))]),
                span: span(),
            },
            Stmt::Block(block(vec![assign_stmt("blk", int_lit(4))])),
            // A begin...end block lowered to a LetBlock in expression position
            // (Issue #3537): its body's bindings escape.
            Stmt::Expr {
                expr: Expr::LetBlock {
                    bindings: vec![],
                    body: block(vec![assign_stmt("let_v", int_lit(5))]),
                    span: span(),
                },
                span: span(),
            },
        ];

        let struct_table: StructRegistry = StructRegistry::new();
        let global_types: HashMap<String, ValueType> = HashMap::new();
        let protected: HashSet<String> = HashSet::new();
        let mut typed_locals: HashMap<String, ValueType> = HashMap::new();
        let mut mixed: HashSet<String> = HashSet::new();
        collect_local_types_with_mixed_tracking(
            &stmts,
            &mut typed_locals,
            &protected,
            &struct_table,
            &global_types,
            &mut mixed,
        );

        let mut names: HashSet<String> = HashSet::new();
        collect_local_binding_names_for_capture(&stmts, &mut names);

        let typed_keys: HashSet<String> = typed_locals.keys().cloned().collect();
        assert_eq!(
            names, typed_keys,
            "name-only capture pre-scan must agree with the typed pre-scan's binding-name set"
        );
        assert!(
            !names.contains("ts_local"),
            "testset-scoped names must not escape the capture pre-scan"
        );
    }

    // ── promote_numeric_value_types ──────────────────────────────────────────

    #[test]
    fn test_promote_numeric_same_type() {
        assert_eq!(
            promote_numeric_value_types(&ValueType::I64, &ValueType::I64),
            Some(ValueType::I64)
        );
        assert_eq!(
            promote_numeric_value_types(&ValueType::F64, &ValueType::F64),
            Some(ValueType::F64)
        );
    }

    #[test]
    fn test_promote_numeric_int_float_to_float() {
        assert_eq!(
            promote_numeric_value_types(&ValueType::I64, &ValueType::F64),
            Some(ValueType::F64)
        );
        assert_eq!(
            promote_numeric_value_types(&ValueType::F64, &ValueType::I64),
            Some(ValueType::F64)
        );
    }

    #[test]
    fn test_promote_numeric_any_returns_none() {
        // Any is not a concrete numeric type
        assert_eq!(
            promote_numeric_value_types(&ValueType::Any, &ValueType::I64),
            None
        );
        assert_eq!(
            promote_numeric_value_types(&ValueType::I64, &ValueType::Any),
            None
        );
    }

    #[test]
    fn test_promote_numeric_non_numeric_returns_none() {
        assert_eq!(
            promote_numeric_value_types(&ValueType::Str, &ValueType::I64),
            None
        );
        assert_eq!(
            promote_numeric_value_types(&ValueType::I64, &ValueType::Str),
            None
        );
    }

    // ── collect_global_types_for_inference ───────────────────────────────────

    #[test]
    fn test_collect_global_types_int_literal() {
        let stmts = vec![assign_stmt("x", int_lit(42))];
        let mut globals = HashMap::new();
        let struct_table = StructRegistry::new();
        let mut const_structs = HashMap::new();
        collect_global_types_for_inference(&stmts, &mut globals, &struct_table, &mut const_structs);
        assert_eq!(globals.get("x"), Some(&ValueType::I64));
    }

    #[test]
    fn test_collect_global_types_float_literal() {
        let stmts = vec![assign_stmt("y", float_lit(1.25))];
        let mut globals = HashMap::new();
        let struct_table = StructRegistry::new();
        let mut const_structs = HashMap::new();
        collect_global_types_for_inference(&stmts, &mut globals, &struct_table, &mut const_structs);
        assert_eq!(globals.get("y"), Some(&ValueType::F64));
    }

    #[test]
    fn test_collect_global_types_reference_previously_defined() {
        // `b = a` where `a = 42` was defined before should pick up a's type
        let stmts = vec![
            assign_stmt("a", int_lit(42)),
            assign_stmt("b", var_expr("a")),
        ];
        let mut globals = HashMap::new();
        let struct_table = StructRegistry::new();
        let mut const_structs = HashMap::new();
        collect_global_types_for_inference(&stmts, &mut globals, &struct_table, &mut const_structs);
        assert_eq!(globals.get("a"), Some(&ValueType::I64));
        assert_eq!(globals.get("b"), Some(&ValueType::I64));
    }

    #[test]
    fn test_collect_global_types_mixed_reassignment_widens_to_any() {
        let stmts = vec![
            assign_stmt("g", int_lit(1)),
            assign_stmt("g", float_lit(1.5)),
        ];
        let mut globals = HashMap::new();
        let struct_table = StructRegistry::new();
        let mut const_structs = HashMap::new();
        collect_global_types_for_inference(&stmts, &mut globals, &struct_table, &mut const_structs);
        assert_eq!(globals.get("g"), Some(&ValueType::Any));
    }

    #[test]
    fn test_collect_global_types_bigint_chained_call_expr_issue_4337() {
        let big_1m = || call_expr("big", vec![int_lit(1_000_000)]);
        let large_a = binary_expr(
            BinaryOp::Mul,
            binary_expr(BinaryOp::Mul, big_1m(), big_1m()),
            big_1m(),
        );
        let large_b = binary_expr(BinaryOp::Mul, big_1m(), big_1m());
        let stmts = vec![
            assign_stmt("large_a", large_a),
            assign_stmt("large_b", large_b),
        ];
        let mut globals = HashMap::new();
        let struct_table = StructRegistry::new();
        let mut const_structs = HashMap::new();

        collect_global_types_for_inference(&stmts, &mut globals, &struct_table, &mut const_structs);

        assert_eq!(globals.get("large_a"), Some(&ValueType::BigInt));
        assert_eq!(globals.get("large_b"), Some(&ValueType::BigInt));
    }

    #[test]
    fn test_infer_value_type_math_call_with_complex_arg_is_any_issue_4341() {
        // `tan(z)` where `z` is an unknown `Struct` (type_id not in the struct
        // table): the shared engine cannot resolve a concrete result, so the
        // pre-scan seam types the slot dynamically (`Any`).
        let mut locals = HashMap::new();
        locals.insert("z".to_string(), ValueType::Struct(0));
        let struct_table: StructRegistry = StructRegistry::new();
        let globals: HashMap<String, ValueType> = HashMap::new();
        let mut engine = None;
        let expr = call_expr("tan", vec![var_expr("z")]);
        assert_eq!(
            assign_rhs_value_type(&expr, &locals, &struct_table, &globals, &mut engine),
            ValueType::Any
        );
    }

    #[test]
    fn test_widen_non_const_globals_preserves_const_markers_issue_4285() {
        let const_block = Stmt::Block(crate::ir::core::Block {
            stmts: vec![
                Stmt::Expr {
                    expr: Expr::Call {
                        function: "#__sjulia_declare_const__".to_string().into(),
                        args: vec![Expr::Literal(Literal::Str("c".to_string()), span())],
                        kwargs: vec![],
                        splat_mask: vec![false],
                        kwargs_splat_mask: vec![],
                        span: span(),
                    },
                    span: span(),
                },
                assign_stmt("c", int_lit(1)),
            ],
            span: span(),
        });
        let stmts = vec![assign_stmt("g", int_lit(1)), const_block];
        let mut globals = HashMap::new();
        globals.insert("g".to_string(), ValueType::I64);
        globals.insert("c".to_string(), ValueType::I64);

        widen_non_const_globals_for_binding_inference(&stmts, &mut globals);

        assert_eq!(globals.get("g"), Some(&ValueType::Any));
        assert_eq!(globals.get("c"), Some(&ValueType::I64));
    }

    #[test]
    fn test_collect_global_types_struct_constructor() {
        // `m = MyType()` → globals["m"] = Struct(type_id)
        let mut struct_table = StructRegistry::new();
        struct_table.insert(
            "MyType".to_string(),
            StructInfo {
                type_id: 5,
                is_mutable: false,
                fields: vec![],
                has_inner_constructor: false,
            },
        );
        let call_expr = Expr::Call {
            function: "MyType".to_string().into(),
            args: vec![],
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span: span(),
        };
        let stmts = vec![assign_stmt("m", call_expr)];
        let mut globals = HashMap::new();
        let mut const_structs = HashMap::new();
        collect_global_types_for_inference(&stmts, &mut globals, &struct_table, &mut const_structs);
        assert_eq!(globals.get("m"), Some(&ValueType::Struct(5)));
        // Empty-arg struct → also tracked in const_structs
        assert!(const_structs.contains_key("m"));
    }

    // ── Var resolution through the pre-scan seam (Issue #3088) ───────────────

    #[test]
    fn test_infer_var_with_global_types_fallback() {
        // When a Var is not in locals, the seam resolves it from global_types.
        // The shared engine preserves the global's concrete `Struct(3)` type
        // (the seam routes `Var` through the engine, whose env is seeded from
        // globals), matching the previous legacy global-types fallback.
        let expr = var_expr("im_const");
        let locals = HashMap::new();
        let struct_table: StructRegistry = StructRegistry::new();
        let mut global_types = HashMap::new();
        global_types.insert("im_const".to_string(), ValueType::Struct(3));
        let mut engine = None;
        let result =
            assign_rhs_value_type(&expr, &locals, &struct_table, &global_types, &mut engine);
        assert_eq!(result, ValueType::Struct(3));
    }

    #[test]
    fn test_infer_function_ref_is_function_issue_4313() {
        // A `FunctionRef` slot is typed `Function` by the seam's explicit arm.
        let expr = function_ref("add");
        let locals = HashMap::new();
        let struct_table = StructRegistry::new();
        let global_types = HashMap::new();
        let mut engine = None;
        assert_eq!(
            assign_rhs_value_type(&expr, &locals, &struct_table, &global_types, &mut engine),
            ValueType::Function
        );
    }

    #[test]
    fn test_infer_var_locals_take_priority_over_globals() {
        // locals takes precedence over global_types: the engine's TypeEnv is
        // seeded from locals, so a local `x: F64` shadows the global `x: I64`.
        let expr = var_expr("x");
        let mut locals = HashMap::new();
        locals.insert("x".to_string(), ValueType::F64);
        let struct_table = StructRegistry::new();
        let mut global_types = HashMap::new();
        global_types.insert("x".to_string(), ValueType::I64); // different in globals
        let mut engine = None;
        let result =
            assign_rhs_value_type(&expr, &locals, &struct_table, &global_types, &mut engine);
        assert_eq!(result, ValueType::F64); // locals wins
    }

    #[test]
    fn test_infer_var_unknown_returns_any() {
        // An unbound, non-pi Var yields `Any` through the seam.
        let expr = var_expr("totally_unknown");
        let locals = HashMap::new();
        let struct_table = StructRegistry::new();
        let global_types = HashMap::new();
        let mut engine = None;
        let result =
            assign_rhs_value_type(&expr, &locals, &struct_table, &global_types, &mut engine);
        assert_eq!(result, ValueType::Any);
    }

    // ── Issue #6601: broad engine-vs-legacy equivalence discovery harness ────
    //
    // Build a representative corpus of `Assign`-RHS expression classes still on
    // the legacy `_ =>` fallback in `assign_rhs_value_type` and, for each,
    // compare the shared-engine path (`assign_rhs_value_type_via_engine`) against
    // the legacy path (`infer_value_type_with_structs`). The corpus is seeded
    // with a non-empty `locals` map carrying one operand of each interesting
    // `ValueType` so binary/index/field classes exercise real types, not `Any`.

    /// Build a struct_table + locals + globals fixture for the #6601 discovery
    /// harness. The struct table contains a `Complex{Float64}` instantiation so
    /// the struct-aware Complex-promotion legacy paths are reachable, plus a
    /// plain `Pt` struct with a typed `x::F64` field for FieldAccess.
    #[allow(clippy::type_complexity)]
    fn discovery_fixture_6601() -> (
        StructRegistry,
        HashMap<String, ValueType>,
        HashMap<String, ValueType>,
    ) {
        let mut struct_table: StructRegistry = StructRegistry::new();
        struct_table.insert(
            "Complex{Float64}".to_string(),
            StructInfo {
                type_id: 100,
                is_mutable: false,
                fields: vec![
                    ("re".to_string(), ValueType::F64),
                    ("im".to_string(), ValueType::F64),
                ],
                has_inner_constructor: false,
            },
        );
        struct_table.insert(
            "Pt".to_string(),
            StructInfo {
                type_id: 101,
                is_mutable: false,
                fields: vec![
                    ("x".to_string(), ValueType::F64),
                    ("y".to_string(), ValueType::I64),
                ],
                has_inner_constructor: false,
            },
        );

        let mut locals: HashMap<String, ValueType> = HashMap::new();
        locals.insert("i".to_string(), ValueType::I64);
        locals.insert("f".to_string(), ValueType::F64);
        locals.insert(
            "arr".to_string(),
            ValueType::ArrayOf(ArrayElementType::I64, None),
        );
        locals.insert("d".to_string(), ValueType::Dict);
        locals.insert("c".to_string(), ValueType::ComplexF64);
        // A struct-typed local resolving to the Complex{Float64} instantiation
        // (type_id 100) and to the plain Pt struct (type_id 101).
        locals.insert("z".to_string(), ValueType::Struct(100));
        locals.insert("p".to_string(), ValueType::Struct(101));
        locals.insert("s".to_string(), ValueType::Str);
        locals.insert("a".to_string(), ValueType::Any);
        locals.insert("ex".to_string(), ValueType::Expr);

        let globals: HashMap<String, ValueType> = HashMap::new();
        (struct_table, locals, globals)
    }

    fn unary_expr(op: crate::ir::core::UnaryOp, operand: Expr) -> Expr {
        Expr::UnaryOp {
            op,
            operand: Box::new(operand),
            span: span(),
        }
    }

    fn index_expr(array: Expr, indices: Vec<Expr>) -> Expr {
        Expr::Index {
            array: Box::new(array),
            indices,
            span: span(),
        }
    }

    fn range_expr(start: Expr, stop: Expr) -> Expr {
        Expr::Range {
            start: Box::new(start),
            step: None,
            stop: Box::new(stop),
            span: span(),
        }
    }

    fn field_access(object: Expr, field: &str) -> Expr {
        Expr::FieldAccess {
            object: Box::new(object),
            field: field.to_string().into(),
            span: span(),
        }
    }

    fn tuple_lit(elements: Vec<Expr>) -> Expr {
        Expr::TupleLiteral {
            elements,
            span: span(),
        }
    }

    /// The full discovery corpus: `(label, expr)` pairs spanning the remaining
    /// legacy-fallback `Assign`-RHS classes. Shared between the discovery harness
    /// (which collects divergences) and the migrated-equivalence pin (which
    /// asserts equality for the subset of classes proven equal).
    fn discovery_corpus_6601() -> Vec<(&'static str, Expr)> {
        use crate::ir::core::UnaryOp;
        vec![
            // ── numeric BinaryOp ──
            (
                "BinaryOp I64+I64",
                binary_expr(BinaryOp::Add, var_expr("i"), var_expr("i")),
            ),
            (
                "BinaryOp I64+F64",
                binary_expr(BinaryOp::Add, var_expr("i"), var_expr("f")),
            ),
            (
                "BinaryOp F64+F64",
                binary_expr(BinaryOp::Add, var_expr("f"), var_expr("f")),
            ),
            (
                "BinaryOp I64/I64 (Div)",
                binary_expr(BinaryOp::Div, var_expr("i"), var_expr("i")),
            ),
            (
                "BinaryOp I64^I64 (Pow)",
                binary_expr(BinaryOp::Pow, var_expr("i"), var_expr("i")),
            ),
            (
                "BinaryOp Str*Str (Mul concat)",
                binary_expr(BinaryOp::Mul, var_expr("s"), var_expr("s")),
            ),
            (
                "BinaryOp I64<I64 (cmp)",
                binary_expr(BinaryOp::Lt, var_expr("i"), var_expr("i")),
            ),
            // ── Complex / struct BinaryOp ──
            (
                "BinaryOp ComplexF64+F64",
                binary_expr(BinaryOp::Add, var_expr("c"), var_expr("f")),
            ),
            (
                "BinaryOp Struct(Complex)+F64",
                binary_expr(BinaryOp::Add, var_expr("z"), var_expr("f")),
            ),
            (
                "BinaryOp Struct(Complex)*Struct(Complex)",
                binary_expr(BinaryOp::Mul, var_expr("z"), var_expr("z")),
            ),
            // ── UnaryOp ──
            (
                "UnaryOp -i (Neg I64)",
                unary_expr(UnaryOp::Neg, var_expr("i")),
            ),
            (
                "UnaryOp -f (Neg F64)",
                unary_expr(UnaryOp::Neg, var_expr("f")),
            ),
            ("UnaryOp !i (Not)", unary_expr(UnaryOp::Not, var_expr("i"))),
            (
                "UnaryOp -c (Neg Complex)",
                unary_expr(UnaryOp::Neg, var_expr("c")),
            ),
            // ── Index ──
            (
                "Index arr[i]",
                index_expr(var_expr("arr"), vec![var_expr("i")]),
            ),
            (
                "Index d[i] (Dict)",
                index_expr(var_expr("d"), vec![var_expr("i")]),
            ),
            (
                "Index s[i] (String char)",
                index_expr(var_expr("s"), vec![var_expr("i")]),
            ),
            (
                "Index arr[1:2] (slice)",
                index_expr(var_expr("arr"), vec![range_expr(int_lit(1), int_lit(2))]),
            ),
            (
                "Index s[1:2] (String slice)",
                index_expr(var_expr("s"), vec![range_expr(int_lit(1), int_lit(2))]),
            ),
            // ── FieldAccess ──
            (
                "FieldAccess p.x (F64 field)",
                field_access(var_expr("p"), "x"),
            ),
            (
                "FieldAccess p.y (I64 field)",
                field_access(var_expr("p"), "y"),
            ),
            (
                "FieldAccess z.re (Complex field)",
                field_access(var_expr("z"), "re"),
            ),
            (
                "FieldAccess p.unknown (missing field)",
                field_access(var_expr("p"), "nope"),
            ),
            (
                "FieldAccess i.x (non-struct object)",
                field_access(var_expr("i"), "x"),
            ),
            (
                "FieldAccess arr.x (array object)",
                field_access(var_expr("arr"), "x"),
            ),
            // ── Call to builtins ──
            (
                "Call length(arr)",
                call_expr("length", vec![var_expr("arr")]),
            ),
            ("Call sqrt(f)", call_expr("sqrt", vec![var_expr("f")])),
            ("Call abs(i)", call_expr("abs", vec![var_expr("i")])),
            (
                "Call abs(c) (Complex)",
                call_expr("abs", vec![var_expr("c")]),
            ),
            (
                "Call abs(z) (Struct)",
                call_expr("abs", vec![var_expr("z")]),
            ),
            (
                "Call exp(z) (struct-preserving)",
                call_expr("exp", vec![var_expr("z")]),
            ),
            ("Call Int64(f)", call_expr("Int64", vec![var_expr("f")])),
            ("Call Float64(i)", call_expr("Float64", vec![var_expr("i")])),
            ("Call zeros(i)", call_expr("zeros", vec![var_expr("i")])),
            ("Call user_fn(i)", call_expr("user_fn", vec![var_expr("i")])),
            // ── Range ──
            ("Range 1:i", range_expr(int_lit(1), var_expr("i"))),
            ("Range 1:10", range_expr(int_lit(1), int_lit(10))),
            (
                "Range f:f (float endpoints)",
                range_expr(var_expr("f"), var_expr("f")),
            ),
            (
                "Range i:f (mixed endpoints)",
                range_expr(var_expr("i"), var_expr("f")),
            ),
            // ── Tuple ──
            (
                "TupleLiteral (i, f)",
                tuple_lit(vec![var_expr("i"), var_expr("f")]),
            ),
            ("TupleLiteral () (empty)", tuple_lit(vec![])),
            (
                "TupleLiteral (i, z, s) (mixed)",
                tuple_lit(vec![var_expr("i"), var_expr("z"), var_expr("s")]),
            ),
            (
                "TupleLiteral (i, a) (Any element)",
                tuple_lit(vec![var_expr("i"), var_expr("a")]),
            ),
            (
                "FieldAccess a.x (Any object)",
                field_access(var_expr("a"), "x"),
            ),
            (
                "FieldAccess ex.head (Expr.head -> Symbol)",
                field_access(var_expr("ex"), "head"),
            ),
            (
                "FieldAccess ex.args (Expr.args -> Array)",
                field_access(var_expr("ex"), "args"),
            ),
        ]
    }

    /// Migrated-value pin (Issue #6601): the classes routed onto the shared engine
    /// in the seam `match` must produce the documented upstream-correct `ValueType`
    /// for EVERY corpus case of that class, on BOTH the direct engine path and the
    /// live seam (they must agree). `Expr::Range` yields `ValueType::Range`
    /// unconditionally; `Expr::UnaryOp` yields `!`->`Bool`, `-` type-preserving
    /// (after the shared-engine `tfunc_not`/`tfunc_sub` fixes); `Expr::TupleLiteral`
    /// yields `Tuple` for any shape (after the shared-engine `TupleLiteral` arm
    /// fix); `Expr::FieldAccess` resolves struct fields, falls back to `Any` for
    /// unknown/non-struct/array/Any objects, and types the `Expr` builtin's fixed
    /// fields (`ex.head`->Symbol, `ex.args`->`Vector{Any}` = `ArrayOf(Any)`). These
    /// are the proven values the deleted legacy pre-scan also produced; this pin
    /// guards that the migrated classes never drift now that legacy is gone.
    #[test]
    fn prescan_engine_equiv_migrated_issue_6601() {
        let (struct_table, locals, globals) = discovery_fixture_6601();
        // Documented upstream-correct expected `ValueType` per migrated corpus
        // label (the values the deleted legacy path produced).
        let expected_for = |label: &str| -> ValueType {
            match label {
                // Range: always `Range`, regardless of endpoint types.
                "Range 1:i"
                | "Range 1:10"
                | "Range f:f (float endpoints)"
                | "Range i:f (mixed endpoints)" => ValueType::Range,
                // UnaryOp: `-` preserves operand type, `!` -> Bool.
                "UnaryOp -i (Neg I64)" => ValueType::I64,
                "UnaryOp -f (Neg F64)" => ValueType::F64,
                "UnaryOp !i (Not)" => ValueType::Bool,
                "UnaryOp -c (Neg Complex)" => ValueType::ComplexF64,
                // TupleLiteral: always `Tuple`, regardless of element types.
                "TupleLiteral (i, f)"
                | "TupleLiteral () (empty)"
                | "TupleLiteral (i, z, s) (mixed)"
                | "TupleLiteral (i, a) (Any element)" => ValueType::Tuple,
                // FieldAccess: resolved struct fields, Any fallbacks, Expr fields.
                "FieldAccess p.x (F64 field)" => ValueType::F64,
                "FieldAccess p.y (I64 field)" => ValueType::I64,
                "FieldAccess z.re (Complex field)" => ValueType::F64,
                "FieldAccess p.unknown (missing field)"
                | "FieldAccess i.x (non-struct object)"
                | "FieldAccess arr.x (array object)"
                | "FieldAccess a.x (Any object)" => ValueType::Any,
                "FieldAccess ex.head (Expr.head -> Symbol)" => ValueType::Symbol,
                "FieldAccess ex.args (Expr.args -> Array)" => {
                    ValueType::ArrayOf(ArrayElementType::Any, None)
                }
                other => panic!("migrated corpus label without expected value: {other}"),
            }
        };
        for (label, value) in discovery_corpus_6601() {
            if !matches!(
                value,
                Expr::Range { .. }
                    | Expr::UnaryOp { .. }
                    | Expr::TupleLiteral { .. }
                    | Expr::FieldAccess { .. }
            ) {
                continue;
            }
            let expected = expected_for(label);
            let mut engine = None;
            let got = assign_rhs_value_type_via_engine(
                &value,
                &locals,
                &struct_table,
                &globals,
                &mut engine,
            );
            assert_eq!(
                got, expected,
                "migrated class wrong for {label}: engine {got:?} != expected {expected:?}",
            );
            // And the live seam (with the migrated arm) must agree.
            let mut seam_engine = None;
            let seam =
                assign_rhs_value_type(&value, &locals, &struct_table, &globals, &mut seam_engine);
            assert_eq!(
                seam, expected,
                "seam result wrong for migrated {label}: {seam:?} != expected {expected:?}",
            );
        }
    }

    /// Tuple-literal migration pin (Issue #6601): a tuple literal is *always* a
    /// `Tuple` in upstream Julia (`typeof((1, "x", [])) == Tuple{...}`), so the
    /// shared-engine path must yield `ValueType::Tuple` for EVERY tuple shape —
    /// all-concrete, mixed, empty, struct-containing, and with a non-concrete
    /// (`Any`) element. Before the engine fix, the with-`Any` case collapsed the
    /// engine to `LatticeType::Top` -> `ValueType::Any`; this pin RED-flags any
    /// regression of that fix.
    #[test]
    fn prescan_engine_equiv_tuple_issue_6601() {
        let (struct_table, locals, globals) = discovery_fixture_6601();
        let cases: Vec<(&'static str, Expr)> = vec![
            (
                "all-concrete (i, f)",
                tuple_lit(vec![var_expr("i"), var_expr("f")]),
            ),
            ("empty ()", tuple_lit(vec![])),
            (
                "struct-containing (i, z, s)",
                tuple_lit(vec![var_expr("i"), var_expr("z"), var_expr("s")]),
            ),
            (
                "with-Any (i, a)",
                tuple_lit(vec![var_expr("i"), var_expr("a")]),
            ),
        ];
        for (label, value) in cases {
            let mut engine = None;
            let got = assign_rhs_value_type_via_engine(
                &value,
                &locals,
                &struct_table,
                &globals,
                &mut engine,
            );
            assert_eq!(
                got,
                ValueType::Tuple,
                "tuple engine path must be Tuple for {label}: got {got:?}",
            );
        }
    }

    /// FieldAccess migration pin (Issue #6601): every `FieldAccess` corpus case
    /// must produce the documented upstream-correct `ValueType` on the shared-engine
    /// path. Struct-field cases (`p.x`->F64, `p.y`->I64, `z.re`->F64), unknown-field
    /// / non-struct / array / Any object cases (all ->Any), and the `Expr` builtin's
    /// fixed fields (`ex.head`->Symbol, `ex.args`->`Vector{Any}` = `ArrayOf(Any)`)
    /// are all typed by the shared-engine `Expr`-field special-case. RED-flags any
    /// regression of those fixes.
    #[test]
    fn prescan_engine_equiv_fieldaccess_issue_6601() {
        let (struct_table, locals, globals) = discovery_fixture_6601();
        let cases: Vec<(&'static str, Expr, ValueType)> = vec![
            ("p.x", field_access(var_expr("p"), "x"), ValueType::F64),
            ("p.y", field_access(var_expr("p"), "y"), ValueType::I64),
            ("z.re", field_access(var_expr("z"), "re"), ValueType::F64),
            (
                "p.nope",
                field_access(var_expr("p"), "nope"),
                ValueType::Any,
            ),
            ("i.x", field_access(var_expr("i"), "x"), ValueType::Any),
            ("arr.x", field_access(var_expr("arr"), "x"), ValueType::Any),
            ("a.x", field_access(var_expr("a"), "x"), ValueType::Any),
            (
                "ex.head",
                field_access(var_expr("ex"), "head"),
                ValueType::Symbol,
            ),
            (
                "ex.args",
                field_access(var_expr("ex"), "args"),
                ValueType::ArrayOf(ArrayElementType::Any, None),
            ),
        ];
        for (label, value, expected) in cases {
            let mut engine = None;
            let got = assign_rhs_value_type_via_engine(
                &value,
                &locals,
                &struct_table,
                &globals,
                &mut engine,
            );
            assert_eq!(got, expected, "unexpected ValueType for {label}");
        }
    }

    /// Index migration pin (Issue #6601): `Index` is an *engine-better* class —
    /// the shared engine types `arr[i]` precisely as the element type (`I64`)
    /// where the legacy pre-scan returned `Any`, and (after this slice's engine
    /// fixes) types String element access `s[i]` as `Char` and String slices
    /// `s[1:2]` as `Str`. Because the engine is the more upstream-correct path
    /// (legacy is slated for deletion), this pin asserts the engine produces the
    /// upstream-correct `ValueType` directly (NOT compared against legacy), and
    /// the live seam (now routed to the engine) agrees. `Index` is filtered out of
    /// `prescan_engine_divergence_map_issue_6601` (see `is_migrated_assign_rhs_class`).
    #[test]
    fn prescan_engine_value_index_issue_6601() {
        let (struct_table, locals, globals) = discovery_fixture_6601();
        let cases: Vec<(&'static str, Expr, ValueType)> = vec![
            (
                "arr[i]",
                index_expr(var_expr("arr"), vec![var_expr("i")]),
                ValueType::I64,
            ),
            (
                "d[i]",
                index_expr(var_expr("d"), vec![var_expr("i")]),
                ValueType::Any,
            ),
            (
                "s[i]",
                index_expr(var_expr("s"), vec![var_expr("i")]),
                ValueType::Char,
            ),
            (
                "arr[1:2]",
                index_expr(var_expr("arr"), vec![range_expr(int_lit(1), int_lit(2))]),
                ValueType::ArrayOf(ArrayElementType::I64, None),
            ),
            (
                "s[1:2]",
                index_expr(var_expr("s"), vec![range_expr(int_lit(1), int_lit(2))]),
                ValueType::Str,
            ),
        ];
        for (label, value, expected) in cases {
            let mut engine = None;
            let got = assign_rhs_value_type_via_engine(
                &value,
                &locals,
                &struct_table,
                &globals,
                &mut engine,
            );
            assert_eq!(got, expected, "engine ValueType for {label}");
            // The live seam (with the migrated Index arm) must agree.
            let mut seam_engine = None;
            let seam =
                assign_rhs_value_type(&value, &locals, &struct_table, &globals, &mut seam_engine);
            assert_eq!(seam, expected, "seam ValueType for {label}");
        }
    }

    /// BinaryOp migration pin (Issue #6601): `BinaryOp` is an *engine-better*
    /// class. After this slice the shared engine types `i^i`->I64 (new `tfunc_pow`)
    /// and `s*s`->Str (string concat in `tfunc_mul`); the Complex cases already
    /// yield the canonical `ComplexF64` (the bridge canonicalizes
    /// `Struct{Complex{Float64}}` -> `ComplexF64`) where legacy returned `F64` /
    /// `Struct(100)`. This pin asserts the engine's upstream-correct `ValueType`
    /// directly (NOT compared against legacy); `BinaryOp` is filtered from the
    /// divergence map via `is_migrated_assign_rhs_class`.
    #[test]
    fn prescan_engine_value_binaryop_issue_6601() {
        let (struct_table, locals, globals) = discovery_fixture_6601();
        let cases: Vec<(&'static str, Expr, ValueType)> = vec![
            (
                "i+i",
                binary_expr(BinaryOp::Add, var_expr("i"), var_expr("i")),
                ValueType::I64,
            ),
            (
                "i+f",
                binary_expr(BinaryOp::Add, var_expr("i"), var_expr("f")),
                ValueType::F64,
            ),
            (
                "f+f",
                binary_expr(BinaryOp::Add, var_expr("f"), var_expr("f")),
                ValueType::F64,
            ),
            (
                "i/i",
                binary_expr(BinaryOp::Div, var_expr("i"), var_expr("i")),
                ValueType::F64,
            ),
            (
                "i^i",
                binary_expr(BinaryOp::Pow, var_expr("i"), var_expr("i")),
                ValueType::I64,
            ),
            (
                "s*s",
                binary_expr(BinaryOp::Mul, var_expr("s"), var_expr("s")),
                ValueType::Str,
            ),
            (
                "i<i",
                binary_expr(BinaryOp::Lt, var_expr("i"), var_expr("i")),
                ValueType::Bool,
            ),
            (
                "c+f",
                binary_expr(BinaryOp::Add, var_expr("c"), var_expr("f")),
                ValueType::ComplexF64,
            ),
            (
                "z+f",
                binary_expr(BinaryOp::Add, var_expr("z"), var_expr("f")),
                ValueType::ComplexF64,
            ),
            (
                "z*z",
                binary_expr(BinaryOp::Mul, var_expr("z"), var_expr("z")),
                ValueType::ComplexF64,
            ),
        ];
        for (label, value, expected) in cases {
            let mut engine = None;
            let got = assign_rhs_value_type_via_engine(
                &value,
                &locals,
                &struct_table,
                &globals,
                &mut engine,
            );
            assert_eq!(got, expected, "engine ValueType for {label}");
            let mut seam_engine = None;
            let seam =
                assign_rhs_value_type(&value, &locals, &struct_table, &globals, &mut seam_engine);
            assert_eq!(seam, expected, "seam ValueType for {label}");
        }
    }

    /// Call migration pin (Issue #6601): `Call` is an *engine-better* class. After
    /// this slice the shared engine types `exp(z)`->ComplexF64 (the `tfunc_sqrt`
    /// family — sqrt/exp/sin/cos/log — now preserves `Complex`), and accepts the
    /// already-correct `abs(c)`/`abs(z)`->F64 (`abs(Complex)::Float64`) and
    /// `zeros(i)`->`ArrayOf(F64)` (`zeros(n)::Vector{Float64}`) where legacy was
    /// imprecise (`ComplexF64` / `Array`). This pin asserts the engine's
    /// upstream-correct `ValueType` directly; `Call` is filtered from the
    /// divergence map via `is_migrated_assign_rhs_class`.
    #[test]
    fn prescan_engine_value_call_issue_6601() {
        let (struct_table, locals, globals) = discovery_fixture_6601();
        let cases: Vec<(&'static str, Expr, ValueType)> = vec![
            (
                "length(arr)",
                call_expr("length", vec![var_expr("arr")]),
                ValueType::I64,
            ),
            (
                "sqrt(f)",
                call_expr("sqrt", vec![var_expr("f")]),
                ValueType::F64,
            ),
            (
                "abs(i)",
                call_expr("abs", vec![var_expr("i")]),
                ValueType::I64,
            ),
            (
                "abs(c)",
                call_expr("abs", vec![var_expr("c")]),
                ValueType::F64,
            ),
            (
                "abs(z)",
                call_expr("abs", vec![var_expr("z")]),
                ValueType::F64,
            ),
            (
                "exp(z)",
                call_expr("exp", vec![var_expr("z")]),
                ValueType::ComplexF64,
            ),
            (
                "Int64(f)",
                call_expr("Int64", vec![var_expr("f")]),
                ValueType::I64,
            ),
            (
                "Float64(i)",
                call_expr("Float64", vec![var_expr("i")]),
                ValueType::F64,
            ),
            (
                "zeros(i)",
                call_expr("zeros", vec![var_expr("i")]),
                ValueType::ArrayOf(ArrayElementType::F64, None),
            ),
            (
                "user_fn(i)",
                call_expr("user_fn", vec![var_expr("i")]),
                ValueType::Any,
            ),
        ];
        for (label, value, expected) in cases {
            let mut engine = None;
            let got = assign_rhs_value_type_via_engine(
                &value,
                &locals,
                &struct_table,
                &globals,
                &mut engine,
            );
            assert_eq!(got, expected, "engine ValueType for {label}");
            let mut seam_engine = None;
            let seam =
                assign_rhs_value_type(&value, &locals, &struct_table, &globals, &mut seam_engine);
            assert_eq!(seam, expected, "seam ValueType for {label}");
        }
    }
}
