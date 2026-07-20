//! Static dispatch resolution for per-method effect summaries (Issue #9495).
//!
//! Follow-up of #9205: that issue moved effect summaries from name-keyed to
//! per-method (`EffectSummaries.by_method`, keyed by the stable `MethodKey`),
//! but the SSA opt passes (`ssa_ir::opt`) still consume the conservative
//! name-level merge (`by_name`). This module wires the precise summaries into
//! the opt passes at call sites whose dispatch is **statically resolved to a
//! unique method**, so a pure `f(::Int)` call can be CSE'd / hoisted / folded
//! even when an impure same-name `f(::IO)` sibling exists.
//!
//! # Soundness (paramount — this enables more aggressive transforms)
//!
//! An over-claimed effect enables a wrong CSE/LICM/DCE transform, so the
//! resolver uses the precise summary **only when it can prove the call's
//! runtime dispatch target is a single, unambiguous method**. Every step errs
//! toward the conservative name-level merge:
//!
//! 1. **Complete candidate set.** Only *fully visible* multi-method generics
//!    participate: a name defined in Base (`program.functions[..base_count]`)
//!    or recognized by the curated builtin effect table is skipped, because a
//!    hidden, possibly more-specific method could otherwise dispatch and be
//!    mis-summarized. What remains is a name whose entire method set lives in
//!    the analysed non-base slice.
//! 2. **Precise argument types.** Resolution fires only when every argument's
//!    static type pins the runtime value's dispatch behavior exactly
//!    ([`dispatch_resolver::core_type_is_dispatch_precise`]) and stays inside
//!    the hierarchy-free builtin fragment, so an empty [`StructHierarchy`] is
//!    a sound oracle for the subtype/intersection queries (builtin primitive /
//!    abstract subtyping does not consult the user-struct hierarchy).
//! 3. **Exactly one applicable method.** The per-candidate verdict reuses the
//!    production typemap filter [`dispatch_resolver::typemap_candidate_verdict`]
//!    (Issue #8548), so acceptance is *exactly as sound as production
//!    dispatch*. The precise summary is returned only when exactly one
//!    candidate is [`TypemapVerdict::Accept`] and every other candidate is
//!    [`TypemapVerdict::Reject`]. Two applicable methods (dispatch would pick
//!    the most-specific, which this slice does not compute), any `Defer*`
//!    verdict, or an unclassifiable signature all fall back to `by_name`.
//!
//! Mirrors upstream, where a statically-resolved call site sees only the
//! dispatched `CodeInstance`'s `ipo_effects` (`julia/Compiler/src/typeinfer.jl`),
//! while an unresolved site stays conservative.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::inference::infer_builtin_effects;
use super::propagation::{compute_function_effects, FuncId};
use super::Effects;
use crate::inference_core::dispatch_resolver::{self, TypemapVerdict};
use crate::inference_core::{CoreType, CoreTypeVar};
use crate::ir::core::{Function, Program};
use crate::types::StructHierarchy;

/// One method of a fully-visible multi-method generic, prepared for the
/// typemap candidate filter.
#[derive(Debug)]
struct ResolvableMethod {
    /// Declared parameter core types (fixed arity). `None` marks a signature
    /// this slice cannot classify soundly (vararg, or any parameter outside
    /// the hierarchy-free builtin fragment); its presence forces resolution to
    /// bail for the whole name.
    param_cores: Option<Vec<CoreType>>,
    /// `where` variables of the method signature.
    type_vars: Vec<CoreTypeVar>,
    /// This method's own (per-method) effect summary — the #9205 precision.
    effects: Effects,
}

/// Per-name candidate table for statically-resolved dispatch (Issue #9495).
///
/// Only names whose complete method set is visible in the non-base slice and
/// has ≥2 methods are recorded (single-method names gain nothing: their
/// per-method summary equals the name-level merge). Built once per gated
/// program compile.
#[derive(Debug)]
pub(crate) struct StaticDispatchResolver {
    /// Empty struct hierarchy: resolution is restricted to the builtin
    /// fragment where subtyping is hierarchy-independent (see module docs,
    /// soundness point 2), so no user-struct hierarchy is needed.
    hierarchy: StructHierarchy,
    candidates: HashMap<FuncId, Vec<ResolvableMethod>>,
}

impl StaticDispatchResolver {
    /// Build the resolver from a program and its converged name-level effect
    /// map. Returns `None` when no name qualifies (the common case), so callers
    /// pay nothing beyond the grouping scan.
    pub(crate) fn build(program: &Program, by_name: &HashMap<FuncId, Effects>) -> Option<Self> {
        let total = program.functions.len();
        let base_count = program.base_function_count.min(total);
        let (base_slice, non_base) = program.functions.split_at(base_count);

        // Names Base defines: their method set is not visible in `non_base`, so
        // a call could dispatch to a Base method we cannot summarize here.
        let base_names: HashSet<&str> = base_slice.iter().map(|f| f.name.as_str()).collect();

        // Group non-base methods by generic-function name.
        let mut grouped: HashMap<&str, Vec<&Arc<Function>>> = HashMap::new();
        for func in non_base {
            grouped.entry(func.name.as_str()).or_default().push(func);
        }

        let mut candidates: HashMap<FuncId, Vec<ResolvableMethod>> = HashMap::new();
        for (name, methods) in grouped {
            // Single-method names gain no precision over `by_name`.
            if methods.len() < 2 {
                continue;
            }
            // Completeness guard: skip any name Base defines or the curated
            // builtin table knows — a hidden method could dispatch.
            if base_names.contains(name) {
                continue;
            }
            if infer_builtin_effects(name, &[]) != Effects::arbitrary() {
                continue;
            }
            let resolvable = methods
                .iter()
                .map(|func| ResolvableMethod {
                    param_cores: classifiable_param_cores(func),
                    type_vars: func.type_params.iter().map(CoreTypeVar::from).collect(),
                    effects: compute_function_effects(func, by_name),
                })
                .collect();
            candidates.insert(name.to_string(), resolvable);
        }

        if candidates.is_empty() {
            return None;
        }
        Some(Self {
            hierarchy: StructHierarchy::new(),
            candidates,
        })
    }

    /// Resolve a bare call `name(arg_cores...)` to a unique method's effect
    /// summary, or `None` when dispatch is not provably resolved (the caller
    /// then uses the conservative `by_name` merge). `arg_cores` are the
    /// statically-known argument core types in positional order.
    pub(crate) fn resolve(&self, name: &str, arg_cores: &[CoreType]) -> Option<Effects> {
        let methods = self.candidates.get(name)?;
        // Every argument must pin dispatch exactly and stay in the builtin
        // fragment (empty-hierarchy soundness).
        if !arg_cores.iter().all(is_resolvable_arg_core) {
            return None;
        }
        let mut resolved: Option<Effects> = None;
        for method in methods {
            // Any unclassifiable sibling means the candidate set is not fully
            // decidable — bail to the sound name-level merge.
            let param_cores = method.param_cores.as_deref()?;
            match dispatch_resolver::typemap_candidate_verdict(
                &self.hierarchy,
                param_cores,
                &method.type_vars,
                arg_cores,
            ) {
                TypemapVerdict::Accept => {
                    if resolved.is_some() {
                        // ≥2 applicable methods: dispatch picks the most
                        // specific, which this slice does not compute. Bail.
                        return None;
                    }
                    resolved = Some(method.effects);
                }
                TypemapVerdict::Reject => {}
                // An intersection question or a signature shape the subtype
                // engine does not decide faithfully — not provably resolved.
                TypemapVerdict::DeferImprecise | TypemapVerdict::DeferSignature => return None,
            }
        }
        resolved
    }
}

/// Declared parameter core types when the method is soundly classifiable by
/// the empty-hierarchy typemap filter, else `None`.
fn classifiable_param_cores(func: &Function) -> Option<Vec<CoreType>> {
    // No arity expansion in this slice: a vararg method's fixed-shape cores
    // would misclassify against a call arity.
    if func.params.iter().any(|param| param.is_varargs) {
        return None;
    }
    let cores: Vec<CoreType> = func
        .params
        .iter()
        .map(|param| CoreType::from(&param.effective_type()))
        .collect();
    if cores.iter().all(is_hierarchy_free_builtin) {
        Some(cores)
    } else {
        None
    }
}

/// Whether an argument core type may drive resolution: it must both pin
/// dispatch exactly and lie in the hierarchy-free builtin fragment.
fn is_resolvable_arg_core(core: &CoreType) -> bool {
    dispatch_resolver::core_type_is_dispatch_precise(core) && is_hierarchy_free_builtin(core)
}

/// Whether subtype/intersection over `core` is decided without consulting the
/// user-struct hierarchy (so an empty [`StructHierarchy`] is a sound oracle).
/// Builtin primitives, builtin abstracts, value params, and tuples/unions/`Type`
/// wrappers thereof qualify; anything nominal (`Struct`, `AbstractUser`,
/// `Named`, `Module`, `NamedTuple`) or non-ground (`TypeVar`, `UnionAll`,
/// `Vararg`) does not.
fn is_hierarchy_free_builtin(core: &CoreType) -> bool {
    match core {
        CoreType::Bottom
        | CoreType::Any
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::Value(_) => true,
        CoreType::Tuple(elems) | CoreType::Union(elems) => {
            elems.iter().all(is_hierarchy_free_builtin)
        }
        CoreType::TypeOf(inner) => is_hierarchy_free_builtin(inner),
        _ => false,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::compile::effects::propagation::infer_program_effects;
    use crate::inference_core::CorePrimitive;
    use crate::ir::core::{BinaryOp, Block, Expr, Literal, Program, Stmt, TypedParam};
    use crate::span::Span;
    use crate::types::JuliaType;

    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn int64() -> CoreType {
        CoreType::Primitive(CorePrimitive::Int64)
    }

    fn float64() -> CoreType {
        CoreType::Primitive(CorePrimitive::Float64)
    }

    fn ret(expr: Expr) -> Stmt {
        Stmt::Return {
            value: Some(expr),
            span: dummy_span(),
        }
    }

    fn call(function: &str, args: Vec<Expr>) -> Expr {
        Expr::Call {
            function: function.to_string().into(),
            args,
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span: dummy_span(),
        }
    }

    /// One method `name(x::param_type) = <body>` (or untyped when `param_type`
    /// is `None`).
    fn method(name: &str, param_type: Option<JuliaType>, body: Expr) -> Function {
        Function {
            name: name.to_string(),
            params: vec![TypedParam::new("x".to_string(), param_type, dummy_span())],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![ret(body)],
                span: dummy_span(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: dummy_span(),
            new_struct_name: None,
        }
    }

    /// `x + 1` — a foldable (pure) body.
    fn pure_body() -> Expr {
        Expr::BinaryOp {
            op: BinaryOp::Add,
            left: Box::new(Expr::Var("x".to_string().into(), dummy_span())),
            right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
            span: dummy_span(),
        }
    }

    /// `println(x)` — an impure (IO) body.
    fn impure_body() -> Expr {
        call(
            "println",
            vec![Expr::Var("x".to_string().into(), dummy_span())],
        )
    }

    fn program_with(functions: Vec<Function>, base_function_count: usize) -> Program {
        Program {
            abstract_types: vec![],
            primitive_types: vec![],
            type_aliases: vec![],
            functions: functions.into_iter().map(std::sync::Arc::new).collect(),
            base_function_count,
            structs: vec![],
            modules: vec![],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            main: Block {
                stmts: vec![],
                span: dummy_span(),
            },
        }
    }

    fn resolver_for(program: &Program) -> Option<StaticDispatchResolver> {
        let by_name = infer_program_effects(program);
        StaticDispatchResolver::build(program, &by_name)
    }

    #[test]
    fn resolves_pure_method_shadowed_by_impure_sibling_issue_9495() {
        // f(x::Int)=x+1 (pure) and f(x::Float64)=println(x) (impure). An Int
        // call statically resolves to the pure method — foldable despite the
        // impure sibling; a Float64 call resolves to the impure method.
        let program = program_with(
            vec![
                method("f", Some(JuliaType::Int64), pure_body()),
                method("f", Some(JuliaType::Float64), impure_body()),
            ],
            0,
        );
        let resolver = resolver_for(&program).expect("multi-method f qualifies");

        let int_call = resolver.resolve("f", &[int64()]).expect("Int resolves");
        assert!(
            int_call.is_foldable(),
            "pure f(::Int) must resolve foldable: {int_call:?}"
        );

        let float_call = resolver
            .resolve("f", &[float64()])
            .expect("Float64 resolves");
        assert!(
            !float_call.is_foldable(),
            "impure f(::Float64) must resolve non-foldable"
        );
    }

    #[test]
    fn does_not_resolve_when_two_methods_apply_issue_9495() {
        // f(x::Int)=x+1 (pure) and f(x)=println(x) (untyped => matches Any).
        // An Int call matches BOTH; dispatch would pick the most specific,
        // which this slice does not compute, so resolution must bail (the
        // conservative name-level merge is used instead).
        let program = program_with(
            vec![
                method("f", Some(JuliaType::Int64), pure_body()),
                method("f", None, impure_body()),
            ],
            0,
        );
        let resolver = resolver_for(&program).expect("multi-method f qualifies");
        assert!(
            resolver.resolve("f", &[int64()]).is_none(),
            "two applicable methods must NOT resolve to a unique summary"
        );
    }

    #[test]
    fn does_not_resolve_imprecise_argument_type_issue_9495() {
        // The same pure/impure pair, but the argument's static type does not
        // pin dispatch (`Any`): resolution must bail.
        let program = program_with(
            vec![
                method("f", Some(JuliaType::Int64), pure_body()),
                method("f", Some(JuliaType::Float64), impure_body()),
            ],
            0,
        );
        let resolver = resolver_for(&program).expect("multi-method f qualifies");
        assert!(
            resolver.resolve("f", &[CoreType::Any]).is_none(),
            "an imprecise (Any) argument must NOT resolve"
        );
    }

    #[test]
    fn skips_names_base_defines_issue_9495() {
        // `f` has a Base method (index 0) plus user methods: its complete
        // method set is not visible here, so a call could dispatch to the Base
        // method. The name must not be resolvable.
        let program = program_with(
            vec![
                method("f", Some(JuliaType::Int64), impure_body()), // Base method
                method("f", Some(JuliaType::Int64), pure_body()),
                method("f", Some(JuliaType::Float64), impure_body()),
            ],
            1, // first function is Base
        );
        assert!(
            resolver_for(&program).is_none(),
            "a Base-defined name must be excluded (hidden dispatch target)"
        );
    }

    #[test]
    fn skips_curated_builtin_names_issue_9495() {
        // `+` is recognized by the curated builtin effect table, so a builtin
        // implementation could dispatch: never resolve it, even with two user
        // methods.
        let program = program_with(
            vec![
                method("+", Some(JuliaType::Int64), pure_body()),
                method("+", Some(JuliaType::Float64), impure_body()),
            ],
            0,
        );
        assert!(
            resolver_for(&program).is_none(),
            "a curated builtin name must be excluded"
        );
    }

    #[test]
    fn single_method_name_is_not_recorded_issue_9495() {
        // A single-method name gains no precision over `by_name`, so the
        // resolver records nothing and `build` returns `None`.
        let program = program_with(vec![method("g", Some(JuliaType::Int64), pure_body())], 0);
        assert!(resolver_for(&program).is_none());
    }
}
