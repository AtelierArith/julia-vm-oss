//! Abstract interpretation engine for type inference.
//!
//! This module implements the core fixpoint loop for abstract interpretation,
//! inferring types through control flow analysis.

// SAFETY: i64→usize cast is for a 1-based index guarded by `if idx_0based < elements.len()`.
#![allow(clippy::cast_sign_loss)]

mod cache_key;
mod world;

use crate::inference_core::{CoreAbstract, CorePrimitive, CoreType};
use cache_key::{cache_fn_id_base_name, MethodInstanceKey};
pub use cache_key::{
    const_specialization, is_const_eligible, widen_argtype_for_cache_key,
    widen_argtypes_for_cache_key, CacheArgType, InferenceCacheKey, SpecializationConst,
    SMALL_INT_CONST_THRESHOLD,
};
pub(crate) use world::{World, WorldRange};

use crate::compile::abstract_interp::conditional::split_env_by_condition;
use crate::compile::abstract_interp::{
    lower_block_to_cfg, run_to_fixpoint_with_edges, BlockId, BranchOutcome, StructTypeInfo, TypeEnv,
};
use crate::compile::const_prop::{try_eval_binary, try_eval_unary};
use crate::compile::diagnostics::{
    emit_limited_accuracy, emit_recursive_cycle, emit_union_split_bailout,
    emit_unknown_array_element, emit_unknown_field, DiagnosticReason, DiagnosticsCollector,
    TypeInferenceDiagnostic,
};
use crate::compile::effects::{inference as effect_inference, Effects};
use crate::compile::inference_trace::{
    record_event, snapshot_env, stmt_kind, BranchKind, InferenceTracer, TraceEvent,
};
use crate::compile::lattice::types::{ConcreteType, ConstValue, LatticeType};
use crate::compile::lattice::widening::MAX_INFERENCE_ITERATIONS;
use crate::compile::method_table::{MethodSig, MethodTable};
use crate::compile::tfuncs::{TFuncContext, TransferFunctions};
use crate::compile::ParametricStructDef;
use crate::inference_core::dispatch_resolver;
use crate::ir::core::{
    BinaryOp, Block, BuiltinOp, Expr, Function, KwParam, Literal, Stmt, UnaryOp,
};
use crate::types::{DispatchError, JuliaType};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

const MAX_LOOP_FIXPOINT_ITERATIONS: usize = 10;
const MAX_INTERPROCEDURAL_ANALYSIS_DEPTH: usize = 10;
/// Per-root interprocedural return-type WORK budget — a catastrophe backstop
/// (Issue #8185).
///
/// `MAX_INTERPROCEDURAL_ANALYSIS_DEPTH` bounds recursion DEPTH but not total
/// WORK: a closure threaded into a deep mutually-recursive call tree under a
/// loop fixpoint re-specializes the whole tree per concrete closure, so even at
/// depth ≤ 10 the number of callee-body re-inferences (each a
/// `infer_block_with_fixpoint` invocation) can grow super-linearly. `_bfgs` hit
/// this and made `compile.build_method_tables` ~5.3 s / 97 % of `using Optim`
/// load time (#8182). Every interprocedural return-type expansion funnels
/// through `infer_block_with_fixpoint`, so the per-root invocation counter
/// (`analysis_work`) bounds the work: when exhausted, inference widens to `Top`
/// (the safe over-approximation), mirroring the exception-type `depth > 16` guard.
///
/// IMPORTANT (empirical, #8185/#8213): this cap is a CATASTROPHE backstop, NOT a
/// tight package-load performance fix. Historical per-root peaks before the
/// package annotations were added: `using Symbolics` ≈ 159k, and the un-annotated
/// `_bfgs` blow-up ≈ 174k — i.e. the blow-up was *indistinguishable from heavy
/// package inference by work-count alone*. The cap is therefore set high enough
/// to trip only truly pathological (host-OOM-class) blow-ups. The actual package
/// regression guards are declared return-type annotations on recursive load-time
/// helpers plus per-package load-time smoke tests; see `docs/vm/CHECKLISTS.md`.
const MAX_INTERPROCEDURAL_ANALYSIS_WORK: usize = 2_000_000;
/// Maximum outer-loop iterations to refine a recursive call's return type
/// between body re-analyses. Bounded to avoid pathological compile times
/// (Issue #3527).
const MAX_RECURSIVE_FIXPOINT_ITERATIONS: usize = 4;
/// Julia's regular method-match union splitting budget
/// (`InferenceParams.max_union_splitting`) is 4. Keep this separate from
/// `MAX_UNION_LENGTH`, which controls lattice widening and is intentionally larger.
const MAX_METHOD_UNION_SPLIT_VARIANTS: usize = 4;

/// Always-on, deterministic metrics for the interprocedural return-type work
/// budget (Issue #8185). Unlike `infer_metrics` (gated behind the `profiling`
/// feature), these are compiled into every build so a default-feature regression
/// test can assert that a bundled package's `using X` inference stays under a
/// per-package work threshold — the only mechanism that catches a #8182-style
/// closure-threaded blow-up (e.g. removal of the `_bfgs` return-type annotation)
/// while functional tests stay green — and that a synthetic blow-up DOES trip the
/// catastrophe backstop (proving the guard fires). Thread-local: compilation is
/// synchronous on the calling thread, so a test reads back what its compile wrote.
///
/// `peak_work` is updated once per `infer_block_with_fixpoint` invocation (a
/// single `Cell` compare/set — negligible even at the ~10⁵ calls a heavy package
/// reaches); `budget_exceeded` is bumped only on the rare backstop trip.
pub(crate) mod work_budget_metrics {
    use std::cell::Cell;

    thread_local! {
        static BUDGET_EXCEEDED: Cell<u64> = const { Cell::new(0) };
        static PEAK_WORK: Cell<usize> = const { Cell::new(0) };
    }

    /// Reset the metrics; call before a measured compilation/inference.
    /// Test-only: the metrics are written by inference in every build but only
    /// read back by the #8185 regression tests.
    #[cfg(test)]
    pub(crate) fn clear() {
        BUDGET_EXCEEDED.with(|c| c.set(0));
        PEAK_WORK.with(|c| c.set(0));
    }

    /// Update the peak per-root interprocedural work seen.
    pub(crate) fn record_work(work: usize) {
        PEAK_WORK.with(|c| {
            if work > c.get() {
                c.set(work);
            }
        });
    }

    /// Record that a root inference exhausted `MAX_INTERPROCEDURAL_ANALYSIS_WORK`
    /// and widened to `Top`.
    pub(crate) fn record_budget_exceeded() {
        BUDGET_EXCEEDED.with(|c| c.set(c.get().saturating_add(1)));
    }

    /// Number of budget-exhaustion events since the last `clear()`.
    #[cfg(test)]
    pub(crate) fn budget_exceeded_count() -> u64 {
        BUDGET_EXCEEDED.with(Cell::get)
    }

    /// Peak per-root interprocedural work observed since the last `clear()`.
    #[cfg(test)]
    pub(crate) fn peak_work() -> usize {
        PEAK_WORK.with(Cell::get)
    }
}

/// Precise method-table callee edge recorded after successful dispatch.
///
/// Bare `function_dependencies` edges remain the conservative fallback for
/// function-table calls and imprecise method-table calls. When a method-table
/// call dispatches with precise argument types, this edge lets a later method
/// mutation invalidate only callers whose observed dispatch could match the
/// changed signature (Issue #5603).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DispatchedMethodEdge {
    callee: String,
    arg_types: Vec<JuliaType>,
}

/// Result of statement inference.
#[derive(Debug)]
enum StmtResult {
    /// Statement completed normally
    Continue,
    /// Statement returned a value (explicit `return` statement).
    /// Subsequent statements are unreachable.
    Return(LatticeType),
    /// Statement may have returned a value (e.g., loop body with `return`).
    /// Subsequent statements are still reachable when the return path
    /// did not fire (e.g., loop with zero iterations). Used to model
    /// conditional returns so post-loop fallthrough is joined with the
    /// loop-body return type at the function-return level (Issue #3547).
    MaybeReturn(LatticeType),
    /// Statement never transfers control to the following statement
    /// without an explicit `return`. Emitted today by a `while true`
    /// loop whose body contains no `break` and no explicit `return`,
    /// matching upstream Julia's `Union{}` post-loop env. Treated like
    /// `Continue` for the explicit-return channel but suppresses the
    /// block's fallthrough (the surrounding block's fallthrough becomes
    /// `Bottom`, and subsequent statements are unreachable). (Issue #4679)
    Diverges,
}

#[derive(Clone, Debug, PartialEq)]
struct ConstructorPartial {
    result_type: LatticeType,
    struct_name: String,
    fields: HashMap<String, LatticeType>,
    /// Field names in declaration / constructor order.
    ///
    /// Upstream `Core.PartialStruct` stores its per-field facts in a positional
    /// `fields` vector, so `getfield(s, i::Int)` resolves via
    /// `_getfield_fieldindex` (a 1-based positional lookup). The lattice value
    /// here keys facts by name for ergonomic by-name lookup, so the declaration
    /// order is retained separately to support integer-index field access with
    /// the same precision as upstream (Issue #4269). When unavailable (e.g. a
    /// partial recovered from the by-name side table without order info), this
    /// is empty and integer-index access conservatively falls back.
    field_order: Vec<String>,
    /// Per-field nested PartialStruct facts. Upstream `Core.PartialStruct` is
    /// recursive: a field whose value is itself built by an analyzable
    /// immutable constructor carries a nested `PartialStruct`, so a
    /// `getfield`/dot/index chain through the outer struct recovers the inner
    /// field's precise type. Only fields built by a constructor we can model
    /// appear here; other fields fall back to their widened `fields` entry
    /// (Issue #4269).
    nested: HashMap<String, Box<ConstructorPartial>>,
}

/// Cached `ConstructorPartial` side-cache entry with the same world/backedge
/// validity model as [`CachedReturn`] (Issue #5603).
#[derive(Clone, Debug, PartialEq)]
struct CachedConstructorPartial {
    /// `None` is a *negative* result: this `(callee, arg_types)` was analyzed
    /// and provably has no precise `ConstructorPartial` to recover (e.g. it
    /// returns a `Union`, a bare number, or differs across branches). Caching
    /// the negative outcome is what keeps a recursive constructor walker whose
    /// helpers all return `Union{Number, Struct}` from re-analyzing the same
    /// `(callee, arg_types)` on every nested call site (Issue #7186). It carries
    /// the same world/edge validity stamp as a positive result, so a later
    /// method addition that *would* make the call recover a partial invalidates
    /// the negative entry through the normal dependency machinery.
    partial: Option<ConstructorPartial>,
    valid_worlds: WorldRange,
    edges: BTreeSet<String>,
    method_edges: Vec<DispatchedMethodEdge>,
    global_reads: BTreeSet<String>,
}

impl CachedConstructorPartial {
    fn new(
        partial: Option<ConstructorPartial>,
        world: World,
        edges: BTreeSet<String>,
        method_edges: Vec<DispatchedMethodEdge>,
        global_reads: BTreeSet<String>,
    ) -> Self {
        Self {
            partial,
            valid_worlds: WorldRange::from_world(world),
            edges,
            method_edges,
            global_reads,
        }
    }
}

/// Cached limited-accuracy marker with CodeInstance-style validity metadata
/// (Issue #5603). This lets diagnostics/flags survive unrelated method or
/// binding changes instead of being dropped wholesale.
#[derive(Clone, Debug, PartialEq)]
struct CachedLimitedAccuracy {
    valid_worlds: WorldRange,
    edges: BTreeSet<String>,
    method_edges: Vec<DispatchedMethodEdge>,
    global_reads: BTreeSet<String>,
}

impl CachedLimitedAccuracy {
    fn new(
        world: World,
        edges: BTreeSet<String>,
        method_edges: Vec<DispatchedMethodEdge>,
        global_reads: BTreeSet<String>,
    ) -> Self {
        Self {
            valid_worlds: WorldRange::from_world(world),
            edges,
            method_edges,
            global_reads,
        }
    }
}

/// Tentative recursive inference result with CodeInstance-style validity
/// metadata (Issue #5603). These entries are still cleared between recursive
/// fixpoint iterations, but method/binding invalidation can now retire only
/// affected entries instead of dropping the side cache wholesale.
#[derive(Clone, Debug, PartialEq)]
struct CachedTentativeResult {
    ty: LatticeType,
    valid_worlds: WorldRange,
    edges: BTreeSet<String>,
    method_edges: Vec<DispatchedMethodEdge>,
    global_reads: BTreeSet<String>,
}

impl CachedTentativeResult {
    fn new(
        ty: LatticeType,
        world: World,
        edges: BTreeSet<String>,
        method_edges: Vec<DispatchedMethodEdge>,
        global_reads: BTreeSet<String>,
    ) -> Self {
        Self {
            ty,
            valid_worlds: WorldRange::from_world(world),
            edges,
            method_edges,
            global_reads,
        }
    }
}

impl ConstructorPartial {
    /// Resolve a field's inferred lattice type by 1-based positional index,
    /// mirroring upstream `getfield_tfunc` on a `PartialStruct` with a
    /// constant integer `name` (`_getfield_fieldindex` + bounds check).
    ///
    /// Returns `None` when the index is out of range or the field order is
    /// unknown, so the caller can fall back to the declared field type.
    fn field_type_by_index(&self, index_1based: i64) -> Option<&LatticeType> {
        if index_1based < 1 {
            return None;
        }
        let idx = usize::try_from(index_1based - 1).ok()?;
        let name = self.field_order.get(idx)?;
        self.fields.get(name)
    }

    /// Resolve a field's nested PartialStruct fact by name, if recorded. Mirrors
    /// upstream `getfield_tfunc` returning a `PartialStruct` field that is itself
    /// a `PartialStruct` (Issue #4269).
    fn nested_field(&self, field: &str) -> Option<&ConstructorPartial> {
        self.nested.get(field).map(|boxed| boxed.as_ref())
    }

    /// Resolve a field's nested PartialStruct fact by 1-based positional index,
    /// the integer-index counterpart of [`Self::nested_field`] (Issue #4269).
    fn nested_field_by_index(&self, index_1based: i64) -> Option<&ConstructorPartial> {
        if index_1based < 1 {
            return None;
        }
        let idx = usize::try_from(index_1based - 1).ok()?;
        let name = self.field_order.get(idx)?;
        self.nested_field(name)
    }
}

/// Extract a constant `Int64` field index from an already-inferred argument
/// lattice type, used for `getfield(s, i::Int)`. Returns `None` unless the
/// index is a known integer constant, since a non-constant index cannot select
/// a specific field's inferred type (Issue #4269).
fn const_int_index(arg_ty: &LatticeType) -> Option<i64> {
    match arg_ty {
        LatticeType::Const(cv) => cv.as_int(),
        _ => None,
    }
}

/// Lattice-join two optional `ConstructorPartial`s coming from the two
/// branches of a ternary / if-else. The result preserves per-field facts
/// only when both branches build the *same* immutable struct shape (same
/// `struct_name` and matching field set), joining each field type. If either
/// branch is absent or the shapes differ, the field facts cannot be
/// soundly preserved, so `None` is returned and the caller falls back to
/// the widened result type. (Issue #4847)
fn join_constructor_partials(
    then_partial: Option<ConstructorPartial>,
    else_partial: Option<ConstructorPartial>,
) -> Option<ConstructorPartial> {
    let then_partial = then_partial?;
    let else_partial = else_partial?;

    if then_partial.struct_name != else_partial.struct_name
        || then_partial.fields.len() != else_partial.fields.len()
    {
        return None;
    }

    let mut fields = HashMap::new();
    for (field, then_ty) in &then_partial.fields {
        let else_ty = else_partial.fields.get(field)?;
        fields.insert(field.clone(), then_ty.join_limited(else_ty, then_ty));
    }

    // Recursively join nested PartialStruct facts: a field's nested partial
    // survives the branch join only when both branches record a nested partial
    // for that field and the two join to a compatible shape. Otherwise the
    // field keeps only its (already joined) widened `fields` entry, which is the
    // sound fallback (Issue #4269).
    let mut nested: HashMap<String, Box<ConstructorPartial>> = HashMap::new();
    for (field, then_nested) in &then_partial.nested {
        if let Some(else_nested) = else_partial.nested.get(field) {
            if let Some(joined) = join_constructor_partials(
                Some(then_nested.as_ref().clone()),
                Some(else_nested.as_ref().clone()),
            ) {
                nested.insert(field.clone(), Box::new(joined));
            }
        }
    }

    // Both branches build the same struct shape, so the positional field order
    // is identical; keep it so integer-index field access survives the join
    // (Issue #4269).
    Some(ConstructorPartial {
        result_type: then_partial.result_type.join(&else_partial.result_type),
        struct_name: then_partial.struct_name,
        fields,
        field_order: then_partial.field_order,
        nested,
    })
}

/// Abstract exception result for expression inference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExceptionType {
    /// Expression is known not to throw.
    Bottom,
    /// Expression may throw a known exception type.
    Known(&'static str),
    /// Expression may throw one of several known exception types.
    /// Mirrors upstream Julia's `Union{ExcA, ExcB, ...}` exception
    /// inference (e.g. `Base.infer_exception_type(() -> log(-1))`
    /// → `Union{DomainError, InexactError}`). Sorted set ensures
    /// canonical equality independent of insertion order
    /// (Issue #4700). Always has length >= 2 — single-element
    /// unions canonicalize to `Known(...)` via the `merge` helper,
    /// and empty unions canonicalize to `Bottom`.
    Union(std::collections::BTreeSet<&'static str>),
    /// Expression may throw an unknown exception type.
    Any,
}

impl ExceptionType {
    /// Lattice-merge two `ExceptionType` values (set union),
    /// canonicalizing the result. `Any` absorbs; `Bottom` is the
    /// identity; matching `Known` stays `Known`; differing `Known`
    /// promote to `Union`. Used by inference paths that join the
    /// exception types of multiple branches (Issue #4700).
    pub fn merge(&self, other: &ExceptionType) -> ExceptionType {
        match (self, other) {
            (ExceptionType::Any, _) | (_, ExceptionType::Any) => ExceptionType::Any,
            (ExceptionType::Bottom, x) | (x, ExceptionType::Bottom) => x.clone(),
            (ExceptionType::Known(a), ExceptionType::Known(b)) => {
                if a == b {
                    ExceptionType::Known(a)
                } else {
                    let mut set = std::collections::BTreeSet::new();
                    set.insert(*a);
                    set.insert(*b);
                    ExceptionType::Union(set)
                }
            }
            (ExceptionType::Known(a), ExceptionType::Union(set))
            | (ExceptionType::Union(set), ExceptionType::Known(a)) => {
                let mut merged = set.clone();
                merged.insert(*a);
                ExceptionType::canonical_union(merged)
            }
            (ExceptionType::Union(a), ExceptionType::Union(b)) => {
                let mut merged = a.clone();
                merged.extend(b.iter());
                ExceptionType::canonical_union(merged)
            }
        }
    }

    /// Construct a `Union` from a set, canonicalizing 0-element to
    /// `Bottom` and 1-element to `Known`. Internal helper for
    /// `merge`.
    fn canonical_union(set: std::collections::BTreeSet<&'static str>) -> ExceptionType {
        match set.len() {
            0 => ExceptionType::Bottom,
            1 => ExceptionType::Known(set.iter().next().unwrap()),
            _ => ExceptionType::Union(set),
        }
    }
}

/// Consulted during reflection-time interprocedural exception inference to
/// obtain a Base callee's exception type from the pure-Julia reflection
/// classification (`Base._classified_exception_type`) instead of recursively
/// walking the callee's pure-Julia body (Issue #6272).
///
/// This mirrors upstream Julia's abstract interpreter, which composes a
/// caller's exception type by joining each callee's *already-inferred* (cached)
/// exception type rather than re-deriving it from the callee body on every
/// reference (`julia/Compiler/src/abstractinterpretation.jl`,
/// `state.exctype = state.exctype ⊔ₚ this_exct`). The pure-Julia classification
/// table plays the role of that cached per-callee result here, keeping the
/// exception semantics of `gcd`/`lcm` owned by pure Julia rather than encoded
/// as Rust name special-cases.
///
/// Returns `Some(exct)` when the pure-Julia layer classifies the callee, or
/// `None` when it has no classification — in which case the composer treats the
/// call conservatively (no proven throw) without descending into the body.
pub trait BaseCalleeExceptionClassifier {
    fn classify_base_callee(
        &mut self,
        name: &str,
        arg_types: &[LatticeType],
    ) -> Option<ExceptionType>;
}

/// Minimal CallMeta-style result for expression inference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferredExpr {
    /// Inferred return/value type.
    pub ty: LatticeType,
    /// Inferred exception type (`Bottom` means no exception path).
    pub exct: ExceptionType,
    /// Inferred computational effects.
    pub effects: Effects,
}

/// A cached interprocedural return-type inference result, stamped with the
/// world range over which it is valid (Issue #4271).
///
/// This is sjulia's miniature analogue of an upstream `CodeInstance`: it pairs
/// the inferred `rettype` with the `[min_world, max_world]` window from
/// `julia/Compiler/src/cicache.jl`, plus the set of interprocedural callee
/// function names the inference depended on (a conservative backedge
/// approximation). When a method is added/replaced, the engine bumps the world
/// and caps `valid_worlds.max_world` of every entry whose own function or one
/// of whose `edges` names the mutated function — mirroring upstream's targeted
/// backedge invalidation rather than wiping the whole cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CachedReturn {
    /// The inferred return type.
    ty: LatticeType,
    /// World range over which `ty` is a valid cached result.
    valid_worlds: WorldRange,
    /// Interprocedural callee function names observed while inferring `ty`.
    ///
    /// Conservative backedge approximation: every function whose
    /// function-table or method-table inference was consulted (transitively)
    /// to produce this result. A mutation to any of these names invalidates
    /// this entry, matching upstream's "edge to a changed method" rule.
    edges: BTreeSet<String>,
    /// Precise method-table callees observed while inferring `ty` (Issue #5603).
    ///
    /// Persisted cache snapshots produced before this field existed decode with
    /// an empty edge list; those older entries keep the conservative bare
    /// `edges` behavior.
    #[serde(default)]
    method_edges: Vec<DispatchedMethodEdge>,
    /// Global/const binding names this result read while it was inferred
    /// (Issue #4285).
    ///
    /// Mirrors upstream's per-`CodeInstance` `GlobalRef` edges
    /// (`julia/Compiler/src/bindinginvalidations.jl`,
    /// `should_invalidate_code_for_globalref`): each cached result records
    /// exactly which top-level bindings its body referenced, so when a binding
    /// is (re)defined or retyped, only the results that read it are invalidated
    /// — the rest stay valid. This is the binding-edge analogue of the method
    /// `edges` above.
    global_reads: BTreeSet<String>,
}

impl CachedReturn {
    /// A freshly inferred result that is valid from `world` onward.
    fn new(
        ty: LatticeType,
        world: World,
        edges: BTreeSet<String>,
        method_edges: Vec<DispatchedMethodEdge>,
        global_reads: BTreeSet<String>,
    ) -> Self {
        Self {
            ty,
            valid_worlds: WorldRange::from_world(world),
            edges,
            method_edges,
            global_reads,
        }
    }
}

/// Abstract interpretation engine for type inference.
///
/// The engine performs fixpoint iteration to infer types for variables
/// and function return values using abstract interpretation.
pub struct InferenceEngine {
    /// Transfer functions for inferring call return types
    tfuncs: TransferFunctions,
    /// Cache of inferred function return types by (function name, arg types) key.
    /// This allows polymorphic functions to return different types based on argument types.
    ///
    /// Each value is a [`CachedReturn`] carrying the inferred type plus the
    /// world range over which it is valid and the callee functions it depends
    /// on (Issue #4271). Lookups are world-gated: an entry is only a hit when
    /// the engine's current `method_world` lies inside its `valid_worlds`,
    /// mirroring upstream's `jl_rettype_inferred` world check.
    return_type_cache: HashMap<InferenceCacheKey, CachedReturn>,
    /// Cache of PartialStruct-style return facts by (function name, arg types).
    ///
    /// Julia's inference can return `Core.PartialStruct` for immutable constructor
    /// results. sjulia keeps the public return type as the struct itself, but this
    /// side cache lets field-value facts survive one interprocedural call.
    partial_struct_return_cache: HashMap<InferenceCacheKey, CachedConstructorPartial>,
    /// Struct type information table (struct name -> StructTypeInfo)
    struct_table: HashMap<String, StructTypeInfo>,
    /// Parametric struct definitions keyed by base name (e.g. `Foo` for
    /// `struct Foo{T} ... end`). Used to recover the concrete instantiated
    /// struct (e.g. `Foo{Int64}`) plus per-field facts from a default
    /// constructor call when the type parameters can be bound from the actual
    /// argument types (Issues #4849 / #4850 / #4851). Empty during the main
    /// compile pipeline; populated only for reflection-time inference.
    parametric_structs: HashMap<String, ParametricStructDef>,
    /// Names of Base/prelude functions present in `function_table` (Issue #6272).
    ///
    /// Reflection-time interprocedural exception inference consults the
    /// pure-Julia reflection classification for these callees (the analogue of
    /// upstream's cached `CodeInstance` exception type, see
    /// `julia/Compiler/src/abstractinterpretation.jl`) instead of recursively
    /// walking their implementation bodies — which for self-recursive Base
    /// helpers such as `gcd`/`lcm` would otherwise explode. Empty during the
    /// main compile pipeline; populated only for reflection-time inference.
    base_function_names: HashSet<String>,
    /// Function table for interprocedural analysis (function name -> Function)
    function_table: HashMap<String, Function>,
    /// Function names with multiple definitions in the current table.
    ambiguous_functions: HashSet<String>,
    /// Conservative callee-dependency ("backedge") approximation per dependency
    /// identity (Issue #4271 / #5939). While inferring a function `F`'s body, every
    /// interprocedural callee `C` resolved through the function/method tables
    /// is recorded here as `F -> {C, ...}` (transitively including `C`'s own
    /// recorded callees). Snapshotted into each [`CachedReturn::edges`] at
    /// commit so a later mutation to any depended-on callee invalidates the
    /// dependent cached result, mirroring upstream backedge invalidation in
    /// `julia/src/gf.c`.
    function_dependencies: HashMap<String, BTreeSet<String>>,
    /// Precise method-table backedges per dependency identity (Issue #5603 / #5939).
    ///
    /// Entries are recorded only when a method-table dispatch or function-table
    /// call resolves with precise argument types. Unlike
    /// [`Self::function_dependencies`], these edges intentionally do not include
    /// the direct callee as a bare name, so a mutation to `callee(::Float64)` can
    /// preserve a caller that only dispatched to `callee(::Int64)`.
    method_dependencies: HashMap<String, Vec<DispatchedMethodEdge>>,
    /// Conservative global-binding dependency approximation per dependency
    /// identity (Issue #4285 / #5939). While inferring a function `F`'s body, every top-level
    /// global/const binding `G` read through the `global_types` fallback is
    /// recorded here as `F -> {G, ...}` (transitively including the recorded
    /// global reads of any callee `C` resolved while inferring `F`). Snapshotted
    /// into each [`CachedReturn::global_reads`] at commit so a later change to a
    /// depended-on binding invalidates the dependent cached result, mirroring
    /// upstream's per-`CodeInstance` `GlobalRef` edges
    /// (`julia/Compiler/src/bindinginvalidations.jl`).
    global_binding_dependencies: HashMap<String, BTreeSet<String>>,
    /// Method tables for inference-only dispatch by call-site argument types.
    method_tables: HashMap<String, MethodTable>,
    /// Monotonic method-table world used to conservatively invalidate cached
    /// inference results after method additions/replacements.
    ///
    /// Julia stores precise `MethodInstance` / `CodeInstance` valid-world
    /// ranges and walks backedges on invalidation. sjulia does not yet model
    /// those identities fully, so each method-table mutation advances this
    /// single world and drops all dependent inference caches (Issue #4271).
    method_world: u64,
    /// Monotonic global-binding world used to conservatively invalidate cached
    /// inference results after a top-level binding environment change
    /// (Issue #4285).
    ///
    /// Analogue of `method_world` for global bindings. Julia advances
    /// `jl_world_counter` and walks per-binding `GlobalRef` edges when a binding
    /// partition changes. sjulia does not model full binding partitions, so
    /// each observed binding change (a binding whose recorded type differs, was
    /// added, or was removed in a [`Self::set_global_types`] call) advances this
    /// single world and caps the `valid_worlds` of exactly the cached results
    /// that read a changed binding.
    binding_world: u64,
    /// Top-level global binding types available to function inference.
    global_types: HashMap<String, LatticeType>,
    /// In-progress return-type estimates for currently-analyzing calls.
    /// On a recursive call cycle (Issue #3527) we return the latest estimate
    /// (initially `Bottom`) instead of `Top`, so that the outer fixpoint
    /// iteration can refine the recursive return as base cases settle.
    analyzing_functions: HashMap<InferenceCacheKey, LatticeType>,
    /// In-progress estimates by method identity, ignoring argument-key
    /// refinement. This catches recursive re-entry into the same method with a
    /// different call-site argument lattice without changing
    /// `analyzing_functions`' exact-key cycle-commit semantics.
    active_function_estimates: HashMap<String, LatticeType>,
    /// PartialStruct-return inference frames currently on the analysis stack,
    /// keyed exactly like [`Self::analyzing_functions`]. Recovering the
    /// `ConstructorPartial` of a call recurses into the callee's body, which
    /// can re-enter the same `(callee, arg_types)` either directly or through a
    /// cycle of mutually-recursive constructors. Unlike the regular
    /// return-type path, there is no useful "tentative partial" to fold on a
    /// back-edge, so re-entry simply yields `None` (the conservative widening:
    /// no precise struct shape recovered). Without this guard a recursive
    /// constructor walker whose branches build nested `Struct(...)` calls (e.g.
    /// the Symbolics `_deriv`/`_mk*` family) fans out into an exponential
    /// re-analysis that never terminates in practice (Issue #7186).
    analyzing_partial_structs: HashSet<InferenceCacheKey>,
    /// Tentative return-type estimates for callees that finished analysis
    /// while one or more enclosing frames were still in progress (i.e. the
    /// callee participated in an active inference cycle). These results
    /// depended on a non-final in-progress estimate and therefore must NOT
    /// be promoted to `return_type_cache` until the outermost cycle frame
    /// converges. This is sjulia's analogue of Julia's `LimitedAccuracy`
    /// marker — see `julia/Compiler/src/typeinfer.jl` (`finish_cycle`,
    /// `cycle_fix_limited`). Entries are world-gated so method/binding
    /// invalidation can target affected temporary facts without discarding
    /// unrelated ones (Issue #5603). Issue #3505.
    tentative_results: HashMap<InferenceCacheKey, CachedTentativeResult>,
    /// Call signatures whose cached or returned estimate is known to have
    /// limited accuracy because a recursion/depth fixpoint cap was reached.
    /// Entries are world-gated so unrelated method/binding changes do not wipe
    /// the side cache wholesale (Issue #5603).
    limited_results: HashMap<InferenceCacheKey, CachedLimitedAccuracy>,
    /// Per-function statement type side table keyed by lowered CFG statement
    /// payload id (Issues #3506 / #4267).
    statement_types: HashMap<String, Vec<LatticeType>>,
    /// Per-function CFG block input environments recorded by the production
    /// inference observation pass (Issue #5602).
    cfg_block_inputs: HashMap<String, Vec<Option<TypeEnv>>>,
    /// Per-function CFG block output environments recorded by the production
    /// inference observation pass (Issue #5602).
    cfg_block_outputs: HashMap<String, Vec<Option<TypeEnv>>>,
    /// Function currently being analyzed; used to populate `statement_types`
    /// and resolve nested function names.
    active_function: Option<String>,
    /// Method/cache identity currently receiving dependency edges.
    ///
    /// This is deliberately separate from [`Self::active_function`]: nested
    /// lookup still needs the lexical function name, while invalidation
    /// precision needs dependencies to be stamped against the same primary
    /// method identity used by [`InferenceCacheKey`] (Issue #5939).
    active_dependency_key: Option<String>,
    /// Concrete instantiated struct name (e.g. `Foo{Int64}`) and its type_id
    /// while analyzing an explicit parametric inner constructor body, so a
    /// `new{T}(...)` expression resolves to the concrete parametric struct
    /// rather than the bare base name (Issue #4850).
    active_parametric_instance: Option<(String, usize)>,
    /// Current recursion depth for interprocedural analysis
    analysis_depth: usize,
    /// Per-root interprocedural return-type work counter (Issue #8185): number of
    /// `infer_block_with_fixpoint` invocations since the current root inference
    /// began. Reset when a root inference starts (`analysis_depth == 0`) and
    /// checked against `MAX_INTERPROCEDURAL_ANALYSIS_WORK` to bound the
    /// closure-threaded deep-recursion blow-up that depth alone cannot.
    analysis_work: usize,
    /// Stack of per-loop break-exit environments. Each entry corresponds to
    /// one enclosing loop (while/for/foreach); `Stmt::Break` snapshots the
    /// current env into the top-of-stack slot. After a `while true` loop
    /// the post-loop environment can be set to the join of these break
    /// envs, since the condition-false exit edge is unreachable and the
    /// only way to leave the loop is via `break` (Issue #4267). For loops
    /// where the condition is not statically true the slot is still
    /// maintained so that nested `break`s target the correct loop.
    loop_break_envs: Vec<Option<TypeEnv>>,
}

/// Compares the fixed-parameter signatures of two functions for equality.
///
/// Two functions have equal signatures when they have the same number of
/// positional parameters and, for each parameter, the same type annotation,
/// varargs flag, and fixed vararg count. This is used by `add_function` to
/// distinguish a same-signature redefinition (a replacement, last-wins) from a
/// genuinely different overload (ambiguous in the untyped function table).
/// Parameter *names*, keyword parameters, `where` clauses, and return-type
/// annotations are intentionally ignored — only the positional-dispatch shape
/// matters for distinguishing redefinition from overload (Issue #5938).
fn signatures_equal(a: &Function, b: &Function) -> bool {
    if a.params.len() != b.params.len() {
        return false;
    }
    a.params.iter().zip(&b.params).all(|(pa, pb)| {
        pa.type_annotation == pb.type_annotation
            && pa.is_varargs == pb.is_varargs
            && pa.vararg_count == pb.vararg_count
    })
}

impl InferenceEngine {
    /// Creates a new inference engine with registered transfer functions.
    pub fn new() -> Self {
        Self::with_struct_table(HashMap::new())
    }

    /// Creates a new inference engine with a given struct table.
    pub fn with_struct_table(struct_table: HashMap<String, StructTypeInfo>) -> Self {
        Self::with_tables(struct_table, HashMap::new())
    }

    /// Creates a new inference engine with struct table and function table.
    ///
    /// The function table enables interprocedural analysis by allowing
    /// the engine to analyze called functions to determine their return types.
    pub fn with_tables(
        struct_table: HashMap<String, StructTypeInfo>,
        function_table: HashMap<String, Function>,
    ) -> Self {
        Self::with_tables_and_method_tables(struct_table, function_table, HashMap::new())
    }

    /// Creates a new inference engine with struct table, function table, and method tables.
    pub(crate) fn with_tables_and_method_tables(
        struct_table: HashMap<String, StructTypeInfo>,
        function_table: HashMap<String, Function>,
        method_tables: HashMap<String, MethodTable>,
    ) -> Self {
        let mut tfuncs = TransferFunctions::new();
        crate::compile::tfuncs::register_all(&mut tfuncs);

        Self {
            tfuncs,
            return_type_cache: HashMap::new(),
            partial_struct_return_cache: HashMap::new(),
            struct_table,
            parametric_structs: HashMap::new(),
            base_function_names: HashSet::new(),
            function_table,
            ambiguous_functions: HashSet::new(),
            function_dependencies: HashMap::new(),
            method_dependencies: HashMap::new(),
            global_binding_dependencies: HashMap::new(),
            method_tables,
            method_world: 1,
            binding_world: 1,
            global_types: HashMap::new(),
            analyzing_functions: HashMap::new(),
            active_function_estimates: HashMap::new(),
            analyzing_partial_structs: HashSet::new(),
            tentative_results: HashMap::new(),
            limited_results: HashMap::new(),
            statement_types: HashMap::new(),
            cfg_block_inputs: HashMap::new(),
            cfg_block_outputs: HashMap::new(),
            active_function: None,
            active_dependency_key: None,
            active_parametric_instance: None,
            analysis_depth: 0,
            analysis_work: 0,
            loop_break_envs: Vec::new(),
        }
    }

    /// Pushes a fresh break-env slot for the loop we are about to enter.
    /// Paired with [`Self::exit_loop`]. (Issue #4267)
    fn enter_loop(&mut self) {
        self.loop_break_envs.push(None);
    }

    /// Pops the break-env slot for the loop we are leaving and returns
    /// the joined env from all `break`s that occurred inside it (or `None`
    /// when the loop contained no break paths). (Issue #4267)
    fn exit_loop(&mut self) -> Option<TypeEnv> {
        self.loop_break_envs.pop().flatten()
    }

    /// Records the current environment as a break-exit env of the innermost
    /// enclosing loop. Multiple breaks join via [`TypeEnv::merge`] so the
    /// post-loop env is the lattice join of every break path. No-op when
    /// there is no enclosing loop (a `break` outside any loop is a parse
    /// error caught earlier). (Issue #4267)
    fn record_break(&mut self, env: &TypeEnv) {
        if let Some(slot) = self.loop_break_envs.last_mut() {
            match slot {
                Some(existing) => existing.merge(env),
                None => *slot = Some(env.clone()),
            }
        }
    }

    /// Adds a function to the function table for interprocedural analysis.
    pub fn add_function(&mut self, func: Function) {
        let name = func.name.clone();
        if self.ambiguous_functions.contains(&name) {
            return;
        }
        if let Some(existing) = self.function_table.get(&name) {
            // An untyped, same-signature redefinition is a REPLACEMENT
            // (last-definition-wins), not an ambiguous overload. Only treat
            // differing signatures as ambiguity; an equal-signature redefinition
            // overwrites in place so caller inference stays precise instead of
            // collapsing to `Any` (Issue #5938). Typed multiple dispatch and
            // typed same-signature redefinition route through `method_tables`
            // and are unaffected by this path.
            if signatures_equal(existing, &func) {
                self.function_table.insert(name, func);
                return;
            }
            self.function_table.remove(&name);
            self.ambiguous_functions.insert(name);
            return;
        }
        self.function_table.insert(name, func);
    }

    /// Adds multiple functions to the function table.
    pub fn add_functions(&mut self, funcs: impl IntoIterator<Item = Function>) {
        for func in funcs {
            self.add_function(func);
        }
    }

    /// Adds or updates one method signature in the inference-only method table.
    pub(crate) fn add_method(&mut self, table_name: String, sig: MethodSig) {
        // `MethodSig` now stores only the canonical `core_signature`, so the
        // invalidation operand can be compared directly (Issue #6495).
        let sig_for_invalidation = sig.clone();
        self.add_method_without_invalidation(table_name.clone(), sig);
        self.invalidate_inference_caches_after_method_mutation(
            &table_name,
            Some(&sig_for_invalidation),
        );
    }

    /// Registers a method during the compiler's initial method-table build.
    ///
    /// This deliberately avoids cache invalidation: the initial build path
    /// registers thousands of Base methods while the shared inference engine is
    /// warming up, and those entries are not runtime method mutations. Dynamic
    /// additions/replacements must use `add_method` so stale caches are dropped
    /// (Issue #4271).
    pub(crate) fn add_initial_method(&mut self, table_name: String, sig: MethodSig) {
        self.add_method_without_invalidation(table_name, sig);
    }

    /// Seeds the inference-only method tables wholesale from cached Base
    /// method tables (Issue #6538).
    ///
    /// On the cached-Base compile path `build_method_tables` short-circuits
    /// every cached Base function without calling [`Self::add_initial_method`],
    /// so cached Base `MethodSig`s (including their `return_julia_type`
    /// snapshots) were never visible to the engine and calls to multi-method
    /// Base functions fell through to the tfunc registry — inferring `Any`
    /// where a fresh full compile infers precisely via the method-table
    /// snapshot path. Installing the cached tables wholesale restores
    /// cached-vs-uncached inference parity.
    ///
    /// Cost: `MethodTable::methods` is an `Arc<Vec<MethodSig>>`, so this is
    /// O(#tables) pointer clones — no per-signature dedup work (the cached
    /// tables were already deduped when phase 1 built them).
    /// `clone_for_reprojection` resets the projection / dispatch cache to the
    /// same default state the uncached path's engine tables have (those are
    /// built via `MethodTable::new` + `add_method` and never receive the
    /// compiler's shared hierarchy projection either).
    ///
    /// Existing engine tables are kept (`or_insert_with`): seeding happens
    /// during the initial pipeline build, before any user method is
    /// registered, and must never clobber a runtime mutation.
    pub(crate) fn seed_initial_method_tables<'t>(
        &mut self,
        tables: impl IntoIterator<Item = (&'t String, &'t MethodTable)>,
    ) {
        for (name, table) in tables {
            self.method_tables.entry(name.clone()).or_insert_with(|| {
                // `clone_for_reprojection` resets `base_function_count` to 0, but
                // method-origin queries (`is_base_program_global_index`) need it to
                // distinguish user overrides from Base methods (e.g. dynamic
                // `getindex` dispatch inference, Issue #6657). Carry it over only
                // for the `getindex` table (the sole origin-sensitive consumer) so
                // other tables keep their previous dominance-fence behavior.
                let mut cloned = table.clone_for_reprojection();
                if name == "getindex" || name == "Base.getindex" {
                    cloned.set_base_function_count(table.base_function_count());
                }
                cloned
            });
        }
    }

    fn add_method_without_invalidation(&mut self, table_name: String, sig: MethodSig) {
        self.method_tables
            .entry(table_name.clone())
            .or_insert_with(|| MethodTable::new(table_name))
            .add_method(sig);
    }

    /// Invalidates cached inference results affected by a mutation to the
    /// method table `mutated_fn` (Issue #4271).
    ///
    /// Mirrors upstream's targeted backedge invalidation
    /// (`julia/src/gf.c`): the global world counter advances, and only the
    /// `CodeInstance`s reachable from the changed method via backedges have
    /// their `max_world` capped. Here we cap the `valid_worlds` of every
    /// `return_type_cache` entry that either *is* the mutated function or has a
    /// recorded callee edge to it. Entries with no dependency on `mutated_fn`
    /// keep their open-ended validity and remain reusable at the new world —
    /// the precision win over the previous table-wide `.clear()`.
    ///
    /// `partial_struct_return_cache` and `limited_results` use the same
    /// world/backedge stamp as the main return cache. Same-name entries are
    /// matched against the mutated signature when available, while callee
    /// dependency edges remain bare-name conservative. `tentative_results`
    /// remains temporary fixpoint state between recursive iterations, but
    /// method mutations retire only affected entries.
    fn invalidate_inference_caches_after_method_mutation(
        &mut self,
        mutated_fn: &str,
        mutated_sig: Option<&MethodSig>,
    ) {
        let new_world = self.method_world.saturating_add(1);
        self.method_world = new_world;
        let mutated_table = self.method_tables.get(mutated_fn);
        let mut invalidated_fns = HashSet::new();
        invalidated_fns.insert(mutated_fn.to_string());

        // Targeted, world-range-aware invalidation of return-type results.
        //
        // A primary cache key's `fn_id` may be specialized (`name(types)`)
        // while the mutated method table is keyed by the bare function name,
        // so match against the *base* name (`name` before any `(`), then
        // narrow same-name hits through the changed method signature. Edges
        // are always recorded under bare names, so compare them directly.
        for (key, cached) in self.return_type_cache.iter_mut() {
            let affected =
                cache_key_matches_mutated_signature(key, mutated_fn, mutated_sig, mutated_table)
                    || cached.edges.contains(mutated_fn)
                    || cached.method_edges.iter().any(|edge| {
                        cached_method_edge_matches_mutated_signature(
                            edge,
                            mutated_fn,
                            mutated_sig,
                            mutated_table,
                        )
                    });
            if affected {
                record_invalidated_dependency_keys(&mut invalidated_fns, key);
                cached.valid_worlds.cap_before(new_world);
            }
        }
        // Retire fully-expired entries so the map does not grow without bound
        // across many mutations. (An entry capped below the current world can
        // never become a hit again, since the world counter is monotonic.)
        self.return_type_cache
            .retain(|_, cached| !cached.valid_worlds.is_expired_at(new_world));

        for (key, cached) in self.partial_struct_return_cache.iter_mut() {
            let affected =
                cache_key_matches_mutated_signature(key, mutated_fn, mutated_sig, mutated_table)
                    || cached.edges.contains(mutated_fn)
                    || cached.method_edges.iter().any(|edge| {
                        cached_method_edge_matches_mutated_signature(
                            edge,
                            mutated_fn,
                            mutated_sig,
                            mutated_table,
                        )
                    });
            if affected {
                record_invalidated_dependency_keys(&mut invalidated_fns, key);
                cached.valid_worlds.cap_before(new_world);
            }
        }
        self.partial_struct_return_cache
            .retain(|_, cached| !cached.valid_worlds.is_expired_at(new_world));

        for (key, cached) in self.limited_results.iter_mut() {
            let affected =
                cache_key_matches_mutated_signature(key, mutated_fn, mutated_sig, mutated_table)
                    || cached.edges.contains(mutated_fn)
                    || cached.method_edges.iter().any(|edge| {
                        cached_method_edge_matches_mutated_signature(
                            edge,
                            mutated_fn,
                            mutated_sig,
                            mutated_table,
                        )
                    });
            if affected {
                record_invalidated_dependency_keys(&mut invalidated_fns, key);
                cached.valid_worlds.cap_before(new_world);
            }
        }
        self.limited_results
            .retain(|_, cached| !cached.valid_worlds.is_expired_at(new_world));

        for (key, cached) in self.tentative_results.iter_mut() {
            let affected =
                cache_key_matches_mutated_signature(key, mutated_fn, mutated_sig, mutated_table)
                    || cached.edges.contains(mutated_fn)
                    || cached.method_edges.iter().any(|edge| {
                        cached_method_edge_matches_mutated_signature(
                            edge,
                            mutated_fn,
                            mutated_sig,
                            mutated_table,
                        )
                    });
            if affected {
                record_invalidated_dependency_keys(&mut invalidated_fns, key);
                cached.valid_worlds.cap_before(new_world);
            }
        }
        self.tentative_results
            .retain(|_, cached| !cached.valid_worlds.is_expired_at(new_world));

        // A function whose cached facts were invalidated must also drop recorded
        // dependency edges and global reads so re-inference rebuilds them fresh.
        self.function_dependencies
            .retain(|fn_name, _| !invalidated_fns.contains(fn_name));
        self.method_dependencies
            .retain(|fn_name, _| !invalidated_fns.contains(fn_name));
        self.global_binding_dependencies
            .retain(|fn_name, _| !invalidated_fns.contains(fn_name));
    }

    #[cfg(test)]
    pub(crate) fn method_world_for_tests(&self) -> u64 {
        self.method_world
    }

    /// Number of live `return_type_cache` entries (test introspection for the
    /// targeted-invalidation behavior, Issue #4271).
    #[cfg(test)]
    pub(crate) fn return_cache_len_for_tests(&self) -> usize {
        self.return_type_cache.len()
    }

    /// Number of live PartialStruct side-cache entries (Issue #5603).
    #[cfg(test)]
    pub(crate) fn partial_struct_return_cache_len_for_tests(&self) -> usize {
        self.partial_struct_return_cache.len()
    }

    #[cfg(test)]
    pub(crate) fn has_cached_partial_struct_return_for_tests(
        &self,
        function_name: &str,
        arg_types: &[LatticeType],
    ) -> bool {
        let cache_key = InferenceCacheKey::new(function_name, arg_types);
        // A *positive* cache entry (a recovered partial), not merely a cache hit:
        // the outer `Some` is the hit, the inner `Some` the recovered partial.
        // A negative entry (inner `None`, Issue #7186) reports `false`, matching
        // the pre-negative-cache meaning of this helper.
        matches!(
            self.lookup_partial_struct_return_cache(&cache_key),
            Some(Some(_))
        )
    }

    #[cfg(test)]
    pub(crate) fn seed_tentative_result_for_tests(
        &mut self,
        function_name: &str,
        arg_types: &[LatticeType],
        ty: LatticeType,
    ) {
        let cache_key = InferenceCacheKey::new(function_name, arg_types);
        self.insert_tentative_result(cache_key, ty);
    }

    #[cfg(test)]
    pub(crate) fn has_tentative_result_for_tests(
        &self,
        function_name: &str,
        arg_types: &[LatticeType],
    ) -> bool {
        let cache_key = InferenceCacheKey::new(function_name, arg_types);
        self.lookup_tentative_result(&cache_key).is_some()
    }

    #[cfg(test)]
    pub(crate) fn record_global_read_for_tests(&mut self, function_name: &str, binding: &str) {
        self.global_binding_dependencies
            .entry(function_name.to_string())
            .or_default()
            .insert(binding.to_string());
    }

    #[cfg(test)]
    pub(crate) fn has_global_read_for_tests(&self, function_name: &str, binding: &str) -> bool {
        self.global_binding_dependencies
            .get(function_name)
            .is_some_and(|bindings| bindings.contains(binding))
    }

    /// Returns a deterministic snapshot of live return-type cache entries.
    ///
    /// This is used by the Base compilation cache to carry already-computed
    /// inference results into the next compile in the same process (Issue #5093).
    pub(crate) fn snapshot_return_cache(&self) -> Vec<(InferenceCacheKey, CachedReturn)> {
        let mut entries: Vec<_> = self
            .return_type_cache
            .iter()
            .filter(|(_, cached)| cached.valid_worlds.contains(self.method_world))
            .map(|(key, cached)| (key.clone(), cached.clone()))
            .collect();
        entries.sort_by(|(left_key, left_cached), (right_key, right_cached)| {
            left_key
                .fn_id
                .cmp(&right_key.fn_id)
                .then_with(|| {
                    format!("{:?}", left_key.argtypes).cmp(&format!("{:?}", right_key.argtypes))
                })
                .then_with(|| {
                    left_cached
                        .valid_worlds
                        .min_world
                        .cmp(&right_cached.valid_worlds.min_world)
                })
                .then_with(|| {
                    left_cached
                        .valid_worlds
                        .max_world
                        .cmp(&right_cached.valid_worlds.max_world)
                })
        });
        entries
    }

    /// Seeds live return-type cache entries from a previous Base compile.
    ///
    /// Entries are rebased onto this engine's current method world so persisted
    /// Base cache snapshots do not depend on the world counter value from the
    /// process that produced them. Only open-ended entries are accepted; capped
    /// entries are already invalidated in their source engine and must not be
    /// revived. Later method additions still run through the normal targeted
    /// invalidation path, so stale seeded results are capped before use.
    pub(crate) fn seed_return_cache(
        &mut self,
        entries: impl IntoIterator<Item = (InferenceCacheKey, CachedReturn)>,
    ) {
        for (key, mut cached) in entries {
            if cached.valid_worlds.max_world == World::MAX {
                cached.valid_worlds = WorldRange::from_world(self.method_world);
                self.return_type_cache.entry(key).or_insert(cached);
            }
        }
    }

    fn replace_active_context(
        &mut self,
        function_name: String,
        dependency_key: String,
    ) -> (Option<String>, Option<String>) {
        let previous_active = self.active_function.replace(function_name);
        let previous_dependency_key = self.active_dependency_key.replace(dependency_key);
        (previous_active, previous_dependency_key)
    }

    fn restore_active_context(&mut self, previous: (Option<String>, Option<String>)) {
        let (previous_active, previous_dependency_key) = previous;
        self.active_function = previous_active;
        self.active_dependency_key = previous_dependency_key;
    }

    fn current_dependency_key(&self) -> Option<String> {
        self.active_dependency_key
            .clone()
            .or_else(|| self.active_function.clone())
    }

    fn dependency_keys_for_callee(&self, callee: &str) -> Vec<String> {
        let mut keys = vec![callee.to_string()];
        if let Some(func) = self.function_table.get(callee) {
            let method_key = inference_cache_function_id(func);
            if method_key != callee {
                keys.push(method_key);
            }
        }
        keys
    }

    fn transitive_dependencies_for_callee(
        &self,
        callee: &str,
    ) -> (BTreeSet<String>, Vec<DispatchedMethodEdge>) {
        let mut function_deps = BTreeSet::new();
        let mut method_edges = Vec::new();
        for key in self.dependency_keys_for_callee(callee) {
            if let Some(transitive) = self.function_dependencies.get(&key) {
                function_deps.extend(transitive.iter().cloned());
            }
            if let Some(transitive_method_edges) = self.method_dependencies.get(&key) {
                for edge in transitive_method_edges {
                    if !method_edges.contains(edge) {
                        method_edges.push(edge.clone());
                    }
                }
            }
        }
        (function_deps, method_edges)
    }

    /// Records that the currently-analyzed function (if any) has an
    /// interprocedural dependency on callee `callee` (Issue #4271).
    ///
    /// Transitively folds in `callee`'s own recorded dependencies so that a
    /// mutation to a transitive callee invalidates this function too, matching
    /// upstream's transitive backedge reachability. No-op when not inside a
    /// function body (`active_dependency_key == None`) or for self-edges.
    fn record_call_dependency(&mut self, callee: &str) {
        let Some(caller) = self.current_dependency_key() else {
            return;
        };
        if caller == callee || cache_fn_id_base_name(&caller) == callee {
            return;
        }
        // Snapshot the callee's transitive dependencies before borrowing the
        // caller's entry mutably.
        let (transitive, transitive_method_edges) = self.transitive_dependencies_for_callee(callee);
        let edges = self
            .function_dependencies
            .entry(caller.clone())
            .or_default();
        edges.insert(callee.to_string());
        edges.extend(transitive);
        if !transitive_method_edges.is_empty() {
            let method_edges = self.method_dependencies.entry(caller).or_default();
            for edge in transitive_method_edges {
                if !method_edges.contains(&edge) {
                    method_edges.push(edge);
                }
            }
        }
    }

    fn record_method_call_dependency(&mut self, callee: &str, arg_types: Vec<JuliaType>) {
        let Some(caller) = self.current_dependency_key() else {
            return;
        };
        if caller == callee || cache_fn_id_base_name(&caller) == callee {
            return;
        }

        let (transitive, transitive_method_edges) = self.transitive_dependencies_for_callee(callee);
        self.function_dependencies
            .entry(caller.clone())
            .or_default()
            .extend(transitive);

        let method_edges = self.method_dependencies.entry(caller).or_default();
        let direct_edge = DispatchedMethodEdge {
            callee: callee.to_string(),
            arg_types,
        };
        if !method_edges.contains(&direct_edge) {
            method_edges.push(direct_edge);
        }
        for edge in transitive_method_edges {
            if !method_edges.contains(&edge) {
                method_edges.push(edge);
            }
        }
    }

    /// Snapshot of the recorded callee dependencies for `fn_id`, used to stamp
    /// a [`CachedReturn`] at commit (Issue #4271).
    fn dependency_edges_for(&self, fn_id: &str) -> BTreeSet<String> {
        self.function_dependencies
            .get(fn_id)
            .cloned()
            .unwrap_or_default()
    }

    fn method_edges_for(&self, fn_id: &str) -> Vec<DispatchedMethodEdge> {
        self.method_dependencies
            .get(fn_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Records that the currently-analyzed function (if any) read top-level
    /// global/const binding `binding` (Issue #4285).
    ///
    /// This is the global-binding analogue of [`Self::record_call_dependency`]:
    /// the read is attributed to the active dependency key so a later change to
    /// `binding` invalidates exactly the cached results that read it. No-op when
    /// not inside a function body (`active_dependency_key == None`).
    fn record_global_read(&mut self, binding: &str) {
        let Some(caller) = self.current_dependency_key() else {
            return;
        };
        self.global_binding_dependencies
            .entry(caller)
            .or_default()
            .insert(binding.to_string());
    }

    /// Snapshot of the recorded global-binding reads for `fn_id`, used to stamp
    /// a [`CachedReturn`] at commit (Issue #4285).
    ///
    /// Folds in the recorded global reads of every callee `fn_id` depends on
    /// (per `function_dependencies`) so that a function which only *transitively*
    /// reads a binding (through a callee) is still invalidated when that binding
    /// changes — mirroring upstream's transitive `GlobalRef` edge reachability.
    fn global_reads_for(&self, fn_id: &str) -> BTreeSet<String> {
        let mut reads = self
            .global_binding_dependencies
            .get(fn_id)
            .cloned()
            .unwrap_or_default();
        if let Some(callees) = self.function_dependencies.get(fn_id) {
            for callee in callees {
                for key in self.dependency_keys_for_callee(callee) {
                    if let Some(callee_reads) = self.global_binding_dependencies.get(&key) {
                        reads.extend(callee_reads.iter().cloned());
                    }
                }
            }
        }
        if let Some(method_edges) = self.method_dependencies.get(fn_id) {
            for edge in method_edges {
                for key in self.dependency_keys_for_callee(&edge.callee) {
                    if let Some(callee_reads) = self.global_binding_dependencies.get(&key) {
                        reads.extend(callee_reads.iter().cloned());
                    }
                }
            }
        }
        reads
    }

    /// World-gated read of the return-type cache (Issue #4271).
    ///
    /// Mirrors upstream `jl_rettype_inferred`: a cached entry is only a hit
    /// when the engine's current `method_world` falls inside its
    /// `valid_worlds`. A present-but-expired entry (capped by a later method
    /// mutation) is treated as a miss so inference recomputes against the new
    /// world.
    fn lookup_return_cache(&self, key: &InferenceCacheKey) -> Option<&LatticeType> {
        self.return_type_cache.get(key).and_then(|cached| {
            cached
                .valid_worlds
                .contains(self.method_world)
                .then_some(&cached.ty)
        })
    }

    fn lookup_tentative_result(&self, key: &InferenceCacheKey) -> Option<&LatticeType> {
        self.tentative_results.get(key).and_then(|cached| {
            cached
                .valid_worlds
                .contains(self.method_world)
                .then_some(&cached.ty)
        })
    }

    fn lookup_in_progress_function_estimate(&self, fn_id: &str) -> Option<LatticeType> {
        self.active_function_estimates.get(fn_id).cloned()
    }

    /// Look up a cached PartialStruct-return result.
    ///
    /// The outer `Option` is the *cache hit*: `Some` means this
    /// `(callee, arg_types)` was already analyzed in a still-valid world. The
    /// inner `Option<&ConstructorPartial>` is that analyzed outcome — `Some`
    /// when a precise partial was recovered, `None` when it provably was not
    /// (a negative cache entry, Issue #7186). A cache *miss* is `None` at the
    /// outer level; callers must analyze the body in that case.
    fn lookup_partial_struct_return_cache(
        &self,
        key: &InferenceCacheKey,
    ) -> Option<Option<&ConstructorPartial>> {
        self.partial_struct_return_cache
            .get(key)
            .filter(|cached| cached.valid_worlds.contains(self.method_world))
            .map(|cached| cached.partial.as_ref())
    }

    /// Inserts a fresh return-type result valid from the current world onward,
    /// stamped with the caller's recorded callee dependencies (Issue #4271).
    fn insert_return_cache(&mut self, key: InferenceCacheKey, ty: LatticeType) {
        let dependency_key = key.fn_id.as_str();
        let edges = self.dependency_edges_for(dependency_key);
        let method_edges = self.method_edges_for(dependency_key);
        let global_reads = self.global_reads_for(dependency_key);
        self.return_type_cache.insert(
            key,
            CachedReturn::new(ty, self.method_world, edges, method_edges, global_reads),
        );
    }

    /// Inserts a return-type result only if no live entry exists, preserving
    /// the world/edge stamp of any current valid entry (Issue #4271). Analogue
    /// of the previous `entry(..).or_insert(..)` on a plain-type cache.
    fn insert_return_cache_if_absent(&mut self, key: InferenceCacheKey, ty: LatticeType) {
        if self.return_type_cache.contains_key(&key) {
            return;
        }
        self.insert_return_cache(key, ty);
    }

    fn insert_tentative_result(&mut self, key: InferenceCacheKey, ty: LatticeType) {
        let dependency_key = key.fn_id.as_str();
        let edges = self.dependency_edges_for(dependency_key);
        let method_edges = self.method_edges_for(dependency_key);
        let global_reads = self.global_reads_for(dependency_key);
        self.tentative_results.insert(
            key,
            CachedTentativeResult::new(ty, self.method_world, edges, method_edges, global_reads),
        );
    }

    fn insert_partial_struct_return_cache(
        &mut self,
        key: InferenceCacheKey,
        partial: Option<ConstructorPartial>,
    ) {
        let dependency_key = key.fn_id.as_str();
        let edges = self.dependency_edges_for(dependency_key);
        let method_edges = self.method_edges_for(dependency_key);
        let global_reads = self.global_reads_for(dependency_key);
        self.partial_struct_return_cache.insert(
            key,
            CachedConstructorPartial::new(
                partial,
                self.method_world,
                edges,
                method_edges,
                global_reads,
            ),
        );
    }

    /// Replaces the global binding type environment used as a fallback after
    /// locals, invalidating only the cached inference results that read a
    /// changed binding (Issue #4285).
    ///
    /// Previously this replaced `global_types` wholesale with no dependency
    /// tracking, so a reused engine could serve a cached return type that was
    /// inferred against a *stale* global/const value. Now the new environment is
    /// diffed against the current one: a binding is "changed" when its recorded
    /// type differs, was added, or was removed. If anything changed, the
    /// inference world advances and the `valid_worlds` of exactly the
    /// `return_type_cache` entries whose recorded [`CachedReturn::global_reads`]
    /// intersect the changed set are capped — the binding-edge analogue of
    /// [`Self::invalidate_inference_caches_after_method_mutation`], mirroring
    /// upstream's per-`GlobalRef` invalidation in
    /// `julia/Compiler/src/bindinginvalidations.jl`.
    ///
    /// Results that read no changed binding keep their open-ended validity and
    /// remain reusable, matching upstream's targeted binding invalidation; the
    /// previous behavior silently kept *all* entries, risking stale precision.
    pub(crate) fn set_global_types(&mut self, global_types: HashMap<String, LatticeType>) {
        let changed = self.changed_bindings(&global_types);
        self.global_types = global_types;
        if !changed.is_empty() {
            self.invalidate_inference_caches_after_binding_change(&changed);
        }
    }

    /// Computes the set of top-level binding names whose value/type changed
    /// between the current `global_types` and the incoming `next` environment
    /// (Issue #4285).
    ///
    /// A binding is changed when it is present in exactly one of the two maps,
    /// or present in both with a different recorded [`LatticeType`]. Over-
    /// approximation is acceptable and preferred: it is always sound to report
    /// a binding as changed (it only triggers recomputation), never sound to
    /// miss a real change.
    fn changed_bindings(&self, next: &HashMap<String, LatticeType>) -> BTreeSet<String> {
        let mut changed = BTreeSet::new();
        for (name, new_ty) in next {
            match self.global_types.get(name) {
                Some(old_ty) if old_ty == new_ty => {}
                _ => {
                    changed.insert(name.clone());
                }
            }
        }
        // Bindings that existed before but are absent now also count as changed
        // (e.g. a binding being removed from the inference environment).
        for name in self.global_types.keys() {
            if !next.contains_key(name) {
                changed.insert(name.clone());
            }
        }
        changed
    }

    /// Invalidates cached inference results that read any binding in `changed`
    /// (Issue #4285).
    ///
    /// Advances the inference world and caps the `valid_worlds` of every
    /// `return_type_cache` / `partial_struct_return_cache` entry whose recorded
    /// `global_reads` intersect `changed`. Entries that read no changed binding
    /// keep their validity, and the global-read dependency records of the
    /// invalidated functions are cleared so a re-inference rebuilds them fresh.
    fn invalidate_inference_caches_after_binding_change(&mut self, changed: &BTreeSet<String>) {
        let new_world = self.method_world.saturating_add(1);
        self.method_world = new_world;
        self.binding_world = self.binding_world.saturating_add(1);

        let mut invalidated_fns: BTreeSet<String> = BTreeSet::new();
        for (key, cached) in self.return_type_cache.iter_mut() {
            if cached.global_reads.iter().any(|g| changed.contains(g)) {
                cached.valid_worlds.cap_before(new_world);
                record_invalidated_dependency_keys(&mut invalidated_fns, key);
            }
        }
        // Retire fully-expired entries so the map does not grow without bound.
        self.return_type_cache
            .retain(|_, cached| !cached.valid_worlds.is_expired_at(new_world));

        for (key, cached) in self.partial_struct_return_cache.iter_mut() {
            if cached.global_reads.iter().any(|g| changed.contains(g)) {
                cached.valid_worlds.cap_before(new_world);
                record_invalidated_dependency_keys(&mut invalidated_fns, key);
            }
        }
        self.partial_struct_return_cache
            .retain(|_, cached| !cached.valid_worlds.is_expired_at(new_world));

        for (key, cached) in self.limited_results.iter_mut() {
            if cached.global_reads.iter().any(|g| changed.contains(g)) {
                cached.valid_worlds.cap_before(new_world);
                record_invalidated_dependency_keys(&mut invalidated_fns, key);
            }
        }
        self.limited_results
            .retain(|_, cached| !cached.valid_worlds.is_expired_at(new_world));

        for (key, cached) in self.tentative_results.iter_mut() {
            if cached.global_reads.iter().any(|g| changed.contains(g)) {
                cached.valid_worlds.cap_before(new_world);
                record_invalidated_dependency_keys(&mut invalidated_fns, key);
            }
        }
        self.tentative_results
            .retain(|_, cached| !cached.valid_worlds.is_expired_at(new_world));

        // Re-inference must rebuild dependency records for invalidated functions.
        for fn_name in &invalidated_fns {
            self.global_binding_dependencies.remove(fn_name);
            self.function_dependencies.remove(fn_name);
            self.method_dependencies.remove(fn_name);
        }
    }

    #[cfg(test)]
    pub(crate) fn binding_world_for_tests(&self) -> u64 {
        self.binding_world
    }

    /// Registers parametric struct definitions so the engine can recover the
    /// concrete instantiated struct (e.g. `Foo{Int64}`) and its field facts
    /// from a default constructor call (Issues #4849 / #4850 / #4851).
    pub fn set_parametric_structs(
        &mut self,
        parametric_structs: HashMap<String, ParametricStructDef>,
    ) {
        self.parametric_structs = parametric_structs;
    }

    /// Registers the set of Base/prelude function names present in the function
    /// table so reflection-time interprocedural exception inference can consult
    /// the pure-Julia classification for them rather than walking their bodies
    /// (Issue #6272). See [`Self::base_function_names`].
    pub fn set_base_function_names(&mut self, base_function_names: HashSet<String>) {
        self.base_function_names = base_function_names;
    }

    /// Infers the return type of a function.
    ///
    /// Uses fixpoint iteration to handle recursive calls and loops.
    /// Returns the inferred return type or Top if inference fails.
    pub fn infer_function(&mut self, func: &Function) -> LatticeType {
        let cache_fn_id = inference_cache_function_id(func);
        // Build argument types from parameter annotations (for cache key)
        let arg_types: Vec<LatticeType> = func
            .params
            .iter()
            .map(|param| {
                if param.is_varargs {
                    // For varargs, use an empty Tuple as the default type
                    // since we don't know how many arguments will be passed
                    LatticeType::Concrete(ConcreteType::Tuple { elements: vec![] })
                } else if let Some(ty) = &param.type_annotation {
                    self.julia_type_to_lattice(ty)
                } else {
                    LatticeType::Top
                }
            })
            .collect();

        // Check cache first using the primary method-identity key.
        let cache_key = InferenceCacheKey::new(&cache_fn_id, &arg_types);
        if let Some(cached) = self.lookup_return_cache(&cache_key) {
            return cached.clone();
        }

        // Initialize environment with parameter types
        let mut env = TypeEnv::new();
        for (param, param_type) in func.params.iter().zip(arg_types.iter()) {
            env.set(&param.name, param_type.clone());
        }
        self.bind_kwparam_default_types(&func.kwparams, &mut env);

        // Run fixpoint iteration
        self.statement_types.insert(func.name.clone(), Vec::new());
        let previous_active_estimate = self
            .active_function_estimates
            .insert(cache_key.fn_id.clone(), LatticeType::Bottom);
        let previous_active =
            self.replace_active_context(func.name.clone(), cache_key.fn_id.clone());
        let return_type = self.infer_block_with_fixpoint(&func.body, &mut env);
        self.restore_active_context(previous_active);
        if let Some(previous) = previous_active_estimate {
            self.active_function_estimates
                .insert(cache_key.fn_id.clone(), previous);
        } else {
            self.active_function_estimates.remove(&cache_key.fn_id);
        }

        // Cache the result
        self.insert_return_cache(cache_key, return_type.clone());

        return_type
    }

    /// Infers the return type of a function using explicit argument types.
    ///
    /// This enables call-site specialization without requiring parameter annotations.
    pub fn infer_function_with_arg_types(
        &mut self,
        func: &Function,
        arg_types: &[LatticeType],
    ) -> LatticeType {
        let cache_fn_id = inference_cache_function_id(func);
        let cache_key = InferenceCacheKey::new(&cache_fn_id, arg_types);
        if let Some(cached) = self.lookup_return_cache(&cache_key) {
            return cached.clone();
        }

        // Use the shared binding helper so both this entry point and the
        // recursive call path apply the same varargs packing semantics
        // (Issue #3526).
        let bindings = {
            let engine = &*self;
            bind_call_args_to_params(&func.params, arg_types, |ty| {
                engine.julia_type_to_lattice(ty)
            })
        };
        let mut env = TypeEnv::new();
        for (name, ty) in bindings {
            env.set(&name, ty);
        }
        self.bind_kwparam_default_types(&func.kwparams, &mut env);

        self.statement_types.insert(func.name.clone(), Vec::new());
        let previous_active_estimate = self
            .active_function_estimates
            .insert(cache_key.fn_id.clone(), LatticeType::Bottom);
        let previous_active =
            self.replace_active_context(func.name.clone(), cache_key.fn_id.clone());
        let return_type = self.infer_block_with_fixpoint(&func.body, &mut env);
        self.restore_active_context(previous_active);
        if let Some(previous) = previous_active_estimate {
            self.active_function_estimates
                .insert(cache_key.fn_id.clone(), previous);
        } else {
            self.active_function_estimates.remove(&cache_key.fn_id);
        }
        self.insert_return_cache(cache_key, return_type.clone());
        return_type
    }

    /// Infer a function call with an existing environment, used for callable
    /// values that carry captured locals into reflection-time inference.
    pub fn infer_function_with_arg_types_and_base_env(
        &mut self,
        func: &Function,
        arg_types: &[LatticeType],
        base_env: &TypeEnv,
    ) -> LatticeType {
        let bindings = {
            let engine = &*self;
            bind_call_args_to_params(&func.params, arg_types, |ty| {
                engine.julia_type_to_lattice(ty)
            })
        };
        let mut env = base_env.clone();
        for (name, ty) in bindings {
            env.set(&name, ty);
        }
        self.bind_kwparam_default_types(&func.kwparams, &mut env);

        self.statement_types.insert(func.name.clone(), Vec::new());
        let dependency_key = inference_cache_function_id(func);
        let previous_active = self.replace_active_context(func.name.clone(), dependency_key);
        let return_type = self.infer_block_with_fixpoint(&func.body, &mut env);
        self.restore_active_context(previous_active);
        return_type
    }

    /// Interprocedural exception-type inference (Issue #5600): compose a
    /// function's exception type from each sub-expression's immediate exception
    /// (the `getindex`→BoundsError / division→DivideError / `sqrt`→DomainError /
    /// `gcd`→OverflowError families) PLUS, for a call to another *user* function,
    /// that callee's own composed exception type — recursively, so a caller's
    /// exception is the union of its callees' exceptions rather than the
    /// name-table miss that previously widened every user function to `Union{}`.
    /// A `try` with a handler suppresses the protected block's exceptions.
    pub fn infer_function_exception_type(
        &mut self,
        classifier: &mut dyn BaseCalleeExceptionClassifier,
        func: &Function,
        arg_types: &[LatticeType],
    ) -> ExceptionType {
        self.infer_function_exception_type_depth(classifier, func, arg_types, 0)
    }

    fn infer_function_exception_type_depth(
        &mut self,
        classifier: &mut dyn BaseCalleeExceptionClassifier,
        func: &Function,
        arg_types: &[LatticeType],
        depth: usize,
    ) -> ExceptionType {
        if depth > 16 {
            // Interprocedural recursion limit: contribute no new exception
            // (`Bottom` is the merge identity) so the result is whatever the
            // bounded walk already proved, rather than widening everything to
            // `Any`. This keeps clean deep recursion at `Union{}` now that `Any`
            // is surfaced to callers (Issues #5600 / #6284).
            return ExceptionType::Bottom;
        }
        let bindings = {
            let engine = &*self;
            bind_call_args_to_params(&func.params, arg_types, |ty| {
                engine.julia_type_to_lattice(ty)
            })
        };
        let mut env = TypeEnv::new();
        for (name, ty) in bindings {
            env.set(&name, ty);
        }
        self.bind_kwparam_default_types(&func.kwparams, &mut env);
        self.block_exception_type(classifier, &func.body, &mut env, depth)
    }

    fn block_exception_type(
        &mut self,
        classifier: &mut dyn BaseCalleeExceptionClassifier,
        block: &Block,
        env: &mut TypeEnv,
        depth: usize,
    ) -> ExceptionType {
        let mut acc = ExceptionType::Bottom;
        for stmt in &block.stmts {
            acc = acc.merge(&self.stmt_exception_type(classifier, stmt, env, depth));
        }
        acc
    }

    fn stmt_exception_type(
        &mut self,
        classifier: &mut dyn BaseCalleeExceptionClassifier,
        stmt: &Stmt,
        env: &mut TypeEnv,
        depth: usize,
    ) -> ExceptionType {
        match stmt {
            Stmt::Return { value: Some(e), .. } | Stmt::Expr { expr: e, .. } => {
                self.expr_exception_type(classifier, e, env, depth)
            }
            Stmt::Return { value: None, .. } => ExceptionType::Bottom,
            Stmt::Assign { var, value, .. } => {
                let exct = self.expr_exception_type(classifier, value, env, depth);
                let ty = self.infer_expr(value, env);
                env.set(var, ty);
                exct
            }
            Stmt::AddAssign { value, .. } => {
                self.expr_exception_type(classifier, value, env, depth)
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let mut acc = self.expr_exception_type(classifier, condition, env, depth);
                let mut then_env = env.clone();
                acc = acc.merge(&self.block_exception_type(
                    classifier,
                    then_branch,
                    &mut then_env,
                    depth,
                ));
                if let Some(eb) = else_branch {
                    let mut else_env = env.clone();
                    acc =
                        acc.merge(&self.block_exception_type(classifier, eb, &mut else_env, depth));
                }
                acc
            }
            Stmt::While {
                condition, body, ..
            } => {
                let acc = self.expr_exception_type(classifier, condition, env, depth);
                let mut body_env = env.clone();
                acc.merge(&self.block_exception_type(classifier, body, &mut body_env, depth))
            }
            Stmt::For {
                start,
                end,
                step,
                body,
                ..
            } => {
                let mut acc = self
                    .expr_exception_type(classifier, start, env, depth)
                    .merge(&self.expr_exception_type(classifier, end, env, depth));
                if let Some(s) = step {
                    acc = acc.merge(&self.expr_exception_type(classifier, s, env, depth));
                }
                let mut body_env = env.clone();
                acc.merge(&self.block_exception_type(classifier, body, &mut body_env, depth))
            }
            Stmt::ForEach { iterable, body, .. } | Stmt::ForEachTuple { iterable, body, .. } => {
                let acc = self.expr_exception_type(classifier, iterable, env, depth);
                let mut body_env = env.clone();
                acc.merge(&self.block_exception_type(classifier, body, &mut body_env, depth))
            }
            Stmt::Block(b) => {
                let mut e = env.clone();
                self.block_exception_type(classifier, b, &mut e, depth)
            }
            Stmt::Try {
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                // The `try` block's exceptions are caught (suppressed); only the
                // catch / else / finally handlers propagate.
                let mut acc = ExceptionType::Bottom;
                for handler in [catch_block, else_block, finally_block]
                    .into_iter()
                    .flatten()
                {
                    let mut e = env.clone();
                    acc = acc.merge(&self.block_exception_type(classifier, handler, &mut e, depth));
                }
                acc
            }
            Stmt::IndexAssign { indices, value, .. } => {
                let mut acc = self.expr_exception_type(classifier, value, env, depth);
                for idx in indices {
                    acc = acc.merge(&self.expr_exception_type(classifier, idx, env, depth));
                }
                acc
            }
            Stmt::DestructuringAssign { value, .. }
            | Stmt::FieldAssign { value, .. }
            | Stmt::DictAssign { value, .. } => {
                self.expr_exception_type(classifier, value, env, depth)
            }
            _ => ExceptionType::Bottom,
        }
    }

    fn expr_exception_type(
        &mut self,
        classifier: &mut dyn BaseCalleeExceptionClassifier,
        expr: &Expr,
        env: &TypeEnv,
        depth: usize,
    ) -> ExceptionType {
        // Immediate exception of THIS expression. We only emit a KNOWN
        // exception when the operand types make it CERTAIN upstream throws —
        // otherwise `Bottom`, so the composition never over-approximates a throw
        // that Julia would not infer (which would regress functions Julia proves
        // total). Indexing is container-type-aware (`Array`→BoundsError,
        // `Dict`→KeyError, `Tuple`→none); `sqrt`/`log` and integer division are
        // genuine VM builtins/intrinsics classified by the type-gated classifier.
        // The recursion below then carries sub-expression and callee exceptions
        // (Issue #5600).
        let immediate = self.immediate_exception_type(expr, env);
        // Whether THIS call node's own immediate classifier already proved a
        // throw (a builtin op such as integer `div` / `getindex`). Captured
        // before `immediate` is merged into `acc`, because the Base-callee
        // consultation below must key off this node alone — not the
        // sub-expression exceptions that are also folded into `acc` (Issue #6272).
        let immediate_is_bottom = matches!(immediate, ExceptionType::Bottom);
        let mut acc = immediate;

        // Recurse into sub-expressions.
        for sub in expr_subexpressions(expr) {
            acc = acc.merge(&self.expr_exception_type(classifier, sub, env, depth));
        }

        // Compose each callee's exception type into `acc`: a caller joins (`⊔ₚ`)
        // every callee's exception type. The upstream analogy and source
        // reference live on the `BaseCalleeExceptionClassifier` trait doc
        // (Issue #5600).
        if let Expr::Call { function, args, .. } = expr {
            if self.is_base_exception_callee(function) {
                // A pure-Julia *Base* callee (e.g. `gcd`/`lcm`) is NOT walked.
                // Its exception type comes from the pure-Julia reflection
                // classification (`Base._classified_exception_type`, consulted
                // via `classifier`) — the analogue of upstream's cached
                // per-callee `CodeInstance` exception type. This keeps `gcd`/`lcm`
                // semantics owned by pure Julia and avoids descending into their
                // self-recursive implementation loops (Issue #6272). When the
                // immediate classifier already proved this node's throw (a
                // builtin op such as integer `div`), there is nothing more to
                // consult — but a throwing *argument* must not suppress it.
                if immediate_is_bottom {
                    let callee_arg_types: Vec<LatticeType> =
                        args.iter().map(|a| self.infer_expr(a, env)).collect();
                    if let Some(exct) = classifier
                        .classify_base_callee(base_callee_name(function), &callee_arg_types)
                    {
                        acc = acc.merge(&exct);
                    }
                }
            } else if depth < 16 {
                // A *user* callee is composed by recursively walking its body.
                if let Some(callee) = self.function_table.get(function).cloned() {
                    let callee_arg_types: Vec<LatticeType> =
                        args.iter().map(|a| self.infer_expr(a, env)).collect();
                    let callee_exct = self.infer_function_exception_type_depth(
                        classifier,
                        &callee,
                        &callee_arg_types,
                        depth + 1,
                    );
                    acc = acc.merge(&callee_exct);
                }
            }
        }

        acc
    }

    /// Returns whether `function` names a pure-Julia Base callee whose exception
    /// type should be obtained from the pure-Julia reflection classification
    /// rather than by walking its body (Issue #6272). See
    /// [`Self::base_function_names`].
    fn is_base_exception_callee(&self, function: &str) -> bool {
        self.base_function_names.contains(function)
            || self
                .base_function_names
                .contains(base_callee_name(function))
    }

    /// The exception THIS expression node is certain to be able to throw, given
    /// its operand types — conservative so the composition never claims a throw
    /// Julia would not (Issue #5600). Unknown / type-uncertain operations return
    /// `Bottom`; the caller's recursion supplies sub-expression exceptions.
    fn immediate_exception_type(&mut self, expr: &Expr, env: &TypeEnv) -> ExceptionType {
        // Container-type-aware indexing.
        let index_array = match expr {
            Expr::Index { array, .. } => Some(array.as_ref()),
            Expr::Call { function, args, .. } if function == "getindex" && !args.is_empty() => {
                Some(&args[0])
            }
            _ => None,
        };
        if let Some(array) = index_array {
            return match self.infer_expr(array, env) {
                LatticeType::Concrete(ConcreteType::Array { .. }) => {
                    ExceptionType::Known("BoundsError")
                }
                LatticeType::Concrete(ConcreteType::Dict { .. }) => {
                    ExceptionType::Known("KeyError")
                }
                // Tuples have a statically-known length, and an unknown container
                // type must not be assumed to throw.
                _ => ExceptionType::Bottom,
            };
        }

        match expr {
            // `sqrt`/`log` of a real argument can hit `DomainError`; over a
            // Complex (or unknown) argument they do not, so require a real arg.
            Expr::Call { function, args, .. }
                if matches!(function.as_str(), "sqrt" | "log" | "log10" | "log2")
                    && args.len() == 1 =>
            {
                let arg_ty = self.infer_expr(&args[0], env);
                if is_real_lattice(&arg_ty) {
                    ExceptionType::Known("DomainError")
                } else {
                    ExceptionType::Bottom
                }
            }
            // Pure-Julia Base callees such as `gcd`/`lcm` are intentionally NOT
            // classified here by name (Issue #6272): their exception types are
            // owned by the pure-Julia reflection classification and consulted by
            // the interprocedural composer in `expr_exception_type` instead.
            //
            // Division reuses the existing type-gated classifier (integers →
            // DivideError); its `Any` fallback means "no known throw" here.
            _ => {
                let mut effects = effect_inference::infer_expr_effects(expr);
                let arg_context = self.exception_arg_context(expr, env);
                match exception_type_for_expr(expr, arg_context.as_deref(), &mut effects) {
                    ExceptionType::Any | ExceptionType::Known("BoundsError") => {
                        ExceptionType::Bottom
                    }
                    other => other,
                }
            }
        }
    }

    /// Build the `(type, effects)` argument context that the typed exception
    /// classifier (`div`/`rem`-on-integers → DivideError, etc.) needs.
    fn exception_arg_context(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Option<Vec<(LatticeType, Effects)>> {
        match expr {
            Expr::Call { args, .. } => Some(
                args.iter()
                    .map(|arg| {
                        (
                            self.infer_expr(arg, env),
                            effect_inference::infer_expr_effects(arg),
                        )
                    })
                    .collect(),
            ),
            Expr::BinaryOp { left, right, .. } => Some(vec![
                (
                    self.infer_expr(left, env),
                    effect_inference::infer_expr_effects(left),
                ),
                (
                    self.infer_expr(right, env),
                    effect_inference::infer_expr_effects(right),
                ),
            ]),
            _ => None,
        }
    }

    /// Infers types for a block using fixpoint iteration.
    ///
    /// Iterates until types stabilize or max iterations reached.
    fn infer_block_with_fixpoint(&mut self, block: &Block, env: &mut TypeEnv) -> LatticeType {
        // Issue #8185: interprocedural return-type WORK budget. Every callee-body
        // re-inference (all return-type call sites) funnels through here, so the
        // per-root invocation count tracks total interprocedural work. Reset the
        // counter at a root inference (`analysis_depth == 0`) so each top-level
        // function gets a fresh budget, then bump and check it. When the budget is
        // exhausted the closure-threaded deep-recursion blow-up (#8182) is cut off
        // by widening to `Top` (safe over-approximation) instead of letting work
        // grow without bound. The cap is far above any legitimate function's
        // reachable specialization count, so this is a pure pathological-case net.
        if self.analysis_depth == 0 {
            self.analysis_work = 0;
        }
        self.analysis_work = self.analysis_work.saturating_add(1);
        work_budget_metrics::record_work(self.analysis_work);
        if self.analysis_work > MAX_INTERPROCEDURAL_ANALYSIS_WORK {
            work_budget_metrics::record_budget_exceeded();
            return LatticeType::Top;
        }

        if let Some(return_type) = self.try_infer_straightline_cfg_return(block, env) {
            return return_type;
        }
        if let Some(return_type) = self.try_infer_all_return_cfg(block, env) {
            return return_type;
        }

        let entry_env = env.clone();
        let mut iteration = 0;
        let mut prev_return_type = LatticeType::Bottom;

        while iteration < MAX_INFERENCE_ITERATIONS {
            iteration += 1;

            let current_return_type = self.infer_block(block, env);

            // Check if we've reached a fixpoint
            if current_return_type == prev_return_type {
                self.record_cfg_worklist_states(block, &entry_env);
                return current_return_type;
            }

            prev_return_type = current_return_type.clone();
        }

        // Max iterations reached: return current best guess
        crate::compile::infer_metrics::record_inference_iteration_limit_hit();
        self.record_cfg_worklist_states(block, &entry_env);
        prev_return_type
    }

    /// Uses the lowered CFG as the authoritative return path for straight-line blocks.
    ///
    /// This is the first #5602 production slice: blocks that lower to a
    /// single basic block with no successors can be interpreted directly from
    /// CFG payload order. Structured control flow stays on the legacy path
    /// until the corresponding CFG transfers are complete.
    fn try_infer_straightline_cfg_return(
        &mut self,
        block: &Block,
        env: &mut TypeEnv,
    ) -> Option<LatticeType> {
        if block
            .stmts
            .iter()
            .any(|stmt| matches!(stmt, Stmt::While { .. }))
        {
            return None;
        }
        let lowered = lower_block_to_cfg(block);
        let cfg = &lowered.cfg;
        if cfg.block_count() != 1 {
            return None;
        }
        let entry = cfg.block(cfg.entry())?;
        if !entry.succ.is_empty()
            || !entry.instructions.iter().all(|stmt_id| {
                cfg_authoritative_straightline_stmt_supported(lowered.statements[*stmt_id])
            })
        {
            return None;
        }

        let entry_env = env.clone();
        let mut last_stmt_type = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        )));
        for stmt_id in &entry.instructions {
            let stmt = lowered.statements[*stmt_id];
            if matches!(stmt, Stmt::Return { .. }) {
                let StmtResult::Return(return_type) = self.infer_stmt(stmt, env) else {
                    return None;
                };
                if InferenceTracer::is_enabled() {
                    record_event(TraceEvent::Statement {
                        index: *stmt_id,
                        kind: stmt_kind(stmt),
                        span: stmt.span(),
                        env_after: snapshot_env(env),
                    });
                }
                self.record_statement_type(*stmt_id, Some(return_type.clone()));
                self.record_straightline_cfg_states(entry.id, entry_env.clone(), env.clone());
                return Some(return_type);
            }

            last_stmt_type = self.cfg_authoritative_statement_value(stmt, env);
            match self.infer_stmt(stmt, env) {
                StmtResult::Continue => {}
                StmtResult::Return(return_type) => {
                    if InferenceTracer::is_enabled() {
                        record_event(TraceEvent::Statement {
                            index: *stmt_id,
                            kind: stmt_kind(stmt),
                            span: stmt.span(),
                            env_after: snapshot_env(env),
                        });
                    }
                    self.record_statement_type(*stmt_id, Some(return_type.clone()));
                    self.record_straightline_cfg_states(entry.id, entry_env.clone(), env.clone());
                    return Some(return_type);
                }
                StmtResult::MaybeReturn(_) | StmtResult::Diverges => return None,
            }
            if InferenceTracer::is_enabled() {
                record_event(TraceEvent::Statement {
                    index: *stmt_id,
                    kind: stmt_kind(stmt),
                    span: stmt.span(),
                    env_after: snapshot_env(env),
                });
            }
            self.record_statement_type(*stmt_id, Some(last_stmt_type.clone()));
        }

        self.record_straightline_cfg_states(entry.id, entry_env, env.clone());
        Some(last_stmt_type)
    }

    /// Uses the lowered CFG as the authoritative return path when every
    /// reachable exit block terminates with `return`.
    ///
    /// This #5602 slice is intentionally narrow: literal/variable/call
    /// payloads and simple `if` edges, including `isa`, `typeof`, and
    /// `nothing` identity predicates, are accepted. Loops and richer expression
    /// transfer stay on the legacy tree walker until the CFG transfer can
    /// model them with full precision.
    fn try_infer_all_return_cfg(
        &mut self,
        block: &Block,
        env: &mut TypeEnv,
    ) -> Option<LatticeType> {
        if InferenceTracer::is_enabled() {
            return None;
        }
        if block
            .stmts
            .iter()
            .any(|stmt| matches!(stmt, Stmt::While { .. }))
        {
            return None;
        }
        let function_name = self.active_function.clone()?;
        let lowered = lower_block_to_cfg(block);
        let cfg = &lowered.cfg;
        if cfg.block_count() <= 1
            || !lowered
                .statements
                .iter()
                .all(|stmt| cfg_authoritative_all_return_stmt_supported(stmt))
        {
            return None;
        }

        let mut cfg_statement_types = vec![LatticeType::Bottom; lowered.statements.len()];
        let mut supported_transfer = true;
        let run = run_to_fixpoint_with_edges(
            cfg,
            env.clone(),
            |id, input| {
                let mut output = input.clone();
                if let Some(bb) = cfg.block(id) {
                    for stmt_id in &bb.instructions {
                        let stmt = lowered.statements[*stmt_id];
                        if let Some(ty) =
                            self.infer_cfg_authoritative_payload_stmt(stmt, &mut output)
                        {
                            cfg_statement_types[*stmt_id] = ty;
                        } else {
                            supported_transfer = false;
                        }
                    }
                }
                output
            },
            |from, to, output| {
                let Some(edge) = lowered.edge_predicate(from, to) else {
                    return output.clone();
                };
                let split = split_env_by_condition(output, edge.condition);
                match edge.outcome {
                    BranchOutcome::Then => split.then_env,
                    BranchOutcome::Else => split.else_env,
                }
            },
        );
        if !supported_transfer || !run.converged {
            return None;
        }

        let mut return_type: Option<LatticeType> = None;
        let mut terminal_env: Option<TypeEnv> = None;
        for bb in cfg.blocks() {
            if !run.seen.contains(&bb.id) {
                continue;
            }
            if !bb.succ.is_empty() {
                continue;
            }
            let last_stmt_id = *bb.instructions.last()?;
            if !matches!(lowered.statements[last_stmt_id], Stmt::Return { .. }) {
                return None;
            }

            let ty = cfg_statement_types[last_stmt_id].clone();
            return_type = Some(if let Some(existing) = return_type {
                existing.join_limited(&ty, &existing)
            } else {
                ty
            });
            if let Some(output) = run.block_outputs[bb.id.index()].as_ref() {
                if let Some(existing) = &mut terminal_env {
                    existing.merge(output);
                } else {
                    terminal_env = Some(output.clone());
                }
            }
        }

        let return_type = return_type?;
        if let Some(output) = terminal_env {
            *env = output;
        }
        self.statement_types
            .insert(function_name.clone(), cfg_statement_types);
        self.cfg_block_inputs
            .insert(function_name.clone(), run.block_inputs);
        self.cfg_block_outputs
            .insert(function_name, run.block_outputs);
        Some(return_type)
    }

    fn infer_cfg_authoritative_payload_stmt(
        &mut self,
        stmt: &Stmt,
        env: &mut TypeEnv,
    ) -> Option<LatticeType> {
        match stmt {
            Stmt::Assign { var, value, .. } => {
                let value_type = self.infer_expr(value, env);
                let alias_source = match value {
                    Expr::Var(src, _) if src != var => Some(src.clone()),
                    _ => None,
                };
                let field_alias_source = self.extract_field_path_alias_source(value);
                env.set(var, value_type.clone());
                env.invalidate_var_paths(var);
                if let Some(src) = alias_source {
                    env.alias_root(var, &src);
                }
                if let Some(path) = field_alias_source {
                    env.alias_field_path(var, &path);
                }
                Some(value_type)
            }
            Stmt::Expr { expr, .. } => Some(self.infer_expr(expr, env)),
            Stmt::Return { value, .. } => Some(if let Some(expr) = value {
                self.infer_expr(expr, env)
            } else {
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Nothing,
                )))
            }),
            Stmt::If { condition, .. } => Some(self.infer_expr(condition, env)),
            _ => None,
        }
    }

    fn cfg_authoritative_statement_value(&mut self, stmt: &Stmt, env: &TypeEnv) -> LatticeType {
        match stmt {
            Stmt::Assign { value, .. } => self.infer_expr(value, env),
            Stmt::Expr { expr, .. } => self.infer_expr(expr, env),
            _ => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        }
    }

    fn record_straightline_cfg_states(
        &mut self,
        block_id: BlockId,
        input: TypeEnv,
        output: TypeEnv,
    ) {
        let Some(function_name) = self.active_function.clone() else {
            return;
        };
        let mut inputs = vec![None; block_id.index() + 1];
        inputs[block_id.index()] = Some(input);
        let mut outputs = vec![None; block_id.index() + 1];
        outputs[block_id.index()] = Some(output);
        self.cfg_block_inputs.insert(function_name.clone(), inputs);
        self.cfg_block_outputs.insert(function_name, outputs);
    }

    /// Runs the lowered CFG/worklist path as a production observation pass.
    ///
    /// The legacy tree walker remains authoritative for return types while
    /// #5602 migrates control-flow transfer in slices. This pass records the
    /// per-block input/output environments and refreshes the statement type
    /// side table from CFG payloads, matching the shape of Julia's per-BB
    /// inference state without changing current return semantics.
    fn record_cfg_worklist_states(&mut self, block: &Block, entry_env: &TypeEnv) {
        let Some(function_name) = self.active_function.clone() else {
            return;
        };
        let lowered = lower_block_to_cfg(block);
        let cfg = &lowered.cfg;
        if cfg.block_count() <= 1 {
            return;
        }
        let mut cfg_statement_types = vec![LatticeType::Bottom; lowered.statements.len()];

        let run = run_to_fixpoint_with_edges(
            cfg,
            entry_env.clone(),
            |id, input| {
                let mut output = input.clone();
                if let Some(bb) = cfg.block(id) {
                    for stmt_id in &bb.instructions {
                        let stmt = lowered.statements[*stmt_id];
                        let ty = self.infer_cfg_payload_stmt(stmt, &mut output);
                        cfg_statement_types[*stmt_id] = ty;
                    }
                }
                output
            },
            |from, to, output| {
                let Some(edge) = lowered.edge_predicate(from, to) else {
                    return output.clone();
                };
                let split = split_env_by_condition(output, edge.condition);
                match edge.outcome {
                    BranchOutcome::Then => split.then_env,
                    BranchOutcome::Else => split.else_env,
                }
            },
        );

        self.statement_types
            .insert(function_name.clone(), cfg_statement_types);
        self.cfg_block_inputs
            .insert(function_name.clone(), run.block_inputs);
        self.cfg_block_outputs
            .insert(function_name, run.block_outputs);
    }

    /// Statement transfer used by the #5602 CFG observation pass.
    ///
    /// Structured control-flow statements are represented by CFG edges, so `if`
    /// and `while` payloads infer only their condition here; branch/body payloads
    /// are transferred in their own basic blocks.
    fn infer_cfg_payload_stmt(&mut self, stmt: &Stmt, env: &mut TypeEnv) -> LatticeType {
        match stmt {
            Stmt::Assign { var, value, .. } => {
                let value_type = self.infer_cfg_payload_expr(value, env);
                let alias_source = match value {
                    Expr::Var(src, _) if src != var => Some(src.clone()),
                    _ => None,
                };
                let field_alias_source = self.extract_field_path_alias_source(value);
                env.set(var, value_type.clone());
                env.invalidate_var_paths(var);
                if let Some(src) = alias_source {
                    env.alias_root(var, &src);
                }
                if let Some(path) = field_alias_source {
                    env.alias_field_path(var, &path);
                }
                value_type
            }
            Stmt::Expr { expr, .. } => self.infer_cfg_payload_expr(expr, env),
            Stmt::Return { value, .. } => {
                if let Some(expr) = value {
                    self.infer_cfg_payload_expr(expr, env)
                } else {
                    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                        CorePrimitive::Nothing,
                    )))
                }
            }
            Stmt::If { condition, .. } | Stmt::While { condition, .. } => {
                self.infer_cfg_payload_expr(condition, env)
            }
            Stmt::FieldAssign {
                object,
                field,
                value,
                ..
            } => {
                let ty = self.infer_cfg_payload_expr(value, env);
                env.invalidate_field_path(object, field);
                ty
            }
            Stmt::IndexAssign {
                array,
                indices,
                value,
                ..
            } => {
                let ty = self.infer_cfg_payload_expr(value, env);
                let precise_key = if indices.len() == 1 {
                    match &indices[0] {
                        Expr::Literal(Literal::Int(i), _) => Some(format!("{}[{}]", array, i)),
                        Expr::Literal(Literal::Bool(b), _) => {
                            Some(format!("{}[{}]", array, if *b { "true" } else { "false" }))
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(key) = precise_key {
                    env.remove(&key);
                } else {
                    env.invalidate_index_paths(array);
                }
                ty
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => LatticeType::Concrete(
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
            ),
            _ => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        }
    }

    fn infer_cfg_payload_expr(&self, expr: &Expr, env: &TypeEnv) -> LatticeType {
        match expr {
            Expr::Literal(lit, _) => self.infer_literal(lit),
            Expr::Var(name, _) => env.get(name).cloned().unwrap_or(LatticeType::Top),
            _ => LatticeType::Top,
        }
    }

    /// Infers types for a block of statements.
    ///
    /// Returns the inferred return type of the block.
    /// In Julia, the value of a block is the value of its last expression/statement.
    fn infer_block(&mut self, block: &Block, env: &mut TypeEnv) -> LatticeType {
        let (return_type, fallthrough, may_fallthrough) = self.infer_block_branch(block, env);
        match (return_type, may_fallthrough) {
            // Block has both explicit/conditional returns AND fallthrough is reachable
            // (e.g., `for i in 1:n; return i; end; "default"`) — join them so the
            // function return type captures both execution outcomes (Issue #3547).
            // Issue #4273: comparison-aware join with the accumulated return as
            // `compare_to`, so a small structured return union is preserved when
            // merging the post-loop fallthrough value.
            (Some(rt), true) => rt.join_limited(&fallthrough, &rt),
            (Some(rt), false) => rt,
            (None, _) => fallthrough,
        }
    }

    /// Infers types for a block but only returns non-Nothing if there is an explicit
    /// `return` statement inside the block. The implicit block value (last statement's
    /// value) is ignored for the return type.
    ///
    /// This is used for loop bodies (while, for, foreach) where the body's implicit value
    /// does not contribute to the enclosing function's return type. Only explicit `return`
    /// statements inside the loop body should propagate as function returns.
    fn infer_block_explicit_return_only(
        &mut self,
        block: &Block,
        env: &mut TypeEnv,
    ) -> LatticeType {
        let mut return_type: Option<LatticeType> = None;

        for stmt in &block.stmts {
            match self.infer_stmt(stmt, env) {
                StmtResult::Continue => {}
                StmtResult::Return(ty) | StmtResult::MaybeReturn(ty) => {
                    // Issue #4273: comparison-aware aggregation of the explicit
                    // loop-body returns against the previously-accumulated
                    // return, so a small structured union of branch returns
                    // (e.g. `Union{Int64, Tuple{...}}`) is preserved instead of
                    // being widened to `Any` by plain `join`'s complexity bound.
                    return_type = Some(if let Some(existing) = return_type {
                        existing.join_limited(&ty, &existing)
                    } else {
                        ty
                    });
                }
                StmtResult::Diverges => {
                    // Inner `while true` with no exit: subsequent statements
                    // in the enclosing loop body are unreachable in the
                    // abstract domain. No explicit return contribution
                    // beyond what was already accumulated. (Issue #4679)
                    break;
                }
            }
        }

        return_type.unwrap_or(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Nothing),
        )))
    }

    /// Infers types for a block, returning explicit returns separated from the
    /// fallthrough (implicit) value.
    ///
    /// Returns `(explicit_return, fallthrough_value, may_fallthrough)`:
    /// - `explicit_return`: `Some(ty)` if any explicit or conditional `return`
    ///   statement was encountered inside this block. This is what propagates
    ///   as the enclosing function return.
    /// - `fallthrough_value`: the implicit value of the block (its last
    ///   statement or, for a final `if`/`try`, its branch fallthroughs joined).
    ///   Used when the block itself supplies a value to a surrounding expression.
    /// - `may_fallthrough`: `true` if execution can reach the end of the block
    ///   without taking an explicit `return`. `false` only when the block ends
    ///   with an unconditional `return`. This lets `infer_block` join
    ///   `explicit_return` with `fallthrough_value` for conditional returns
    ///   (e.g., `for i in 1:n; return i; end; "default"` — Issue #3547) while
    ///   still treating an unconditional `return` as discarding fallthrough.
    ///
    /// This separation is required to fix Issues #3513, #3514, #3515: implicit
    /// branch values must NOT be conflated with explicit `return` statements
    /// when the surrounding `if`/`try` is followed by additional code.
    fn infer_block_branch(
        &mut self,
        block: &Block,
        env: &mut TypeEnv,
    ) -> (Option<LatticeType>, LatticeType, bool) {
        let mut return_type: Option<LatticeType> = None;
        let mut last_stmt_type: Option<LatticeType> = None;
        // Tracks whether execution can reach the end of the block. Set to false
        // only when an unconditional `return` is encountered (Issue #3547).
        let mut may_fallthrough = true;

        let last_index = block.stmts.len().saturating_sub(1);
        for (i, stmt) in block.stmts.iter().enumerate() {
            let is_last = i == last_index;

            // For final `if`/`try`, compute fallthrough AND explicit returns inline,
            // then skip the post-loop infer_stmt call to avoid duplicate work.
            let mut handled_inline = false;
            match stmt {
                Stmt::Expr { expr, .. } => {
                    last_stmt_type = Some(self.infer_expr(expr, env));
                }
                Stmt::Assign { value, .. } => {
                    last_stmt_type = Some(self.infer_expr(value, env));
                }
                Stmt::Break { .. } | Stmt::Continue { .. } => {
                    // Flow control: implicit value unchanged
                }
                Stmt::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } if is_last => {
                    // Final `if`: compute branch fallthroughs and explicit returns
                    // in one pass. We do NOT call `infer_stmt` afterwards.
                    let _ = self.infer_expr(condition, env);
                    let split = crate::compile::abstract_interp::conditional::split_env_by_condition_with_predicates_and_structs(
                        env,
                        condition,
                        &self.function_table,
                        &self.struct_table,
                    );
                    let mut then_env = split.then_env;
                    if InferenceTracer::is_enabled() {
                        // Inference trace (Issue #3512): record both branch
                        // environments at the split point so the developer
                        // can see post-narrowing types for each side.
                        record_event(TraceEvent::Branch {
                            kind: BranchKind::If,
                            then_env: snapshot_env(&then_env),
                            else_env: snapshot_env(&split.else_env),
                        });
                    }
                    let (then_ret, then_fall, then_may_fall) =
                        self.infer_block_branch(then_branch, &mut then_env);
                    let mut else_env = split.else_env;
                    let (else_ret, else_fall, else_may_fall) = if let Some(else_blk) = else_branch {
                        self.infer_block_branch(else_blk, &mut else_env)
                    } else {
                        (
                            None,
                            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                                CorePrimitive::Nothing,
                            ))),
                            true,
                        )
                    };
                    if InferenceTracer::is_enabled() {
                        // After both branches, snapshot post-branch
                        // environments so the trace records what each side
                        // produced (Issue #3512).
                        record_event(TraceEvent::Branch {
                            kind: BranchKind::If,
                            then_env: snapshot_env(&then_env),
                            else_env: snapshot_env(&else_env),
                        });
                    }

                    // Merge environments from both branches into env
                    *env = then_env;
                    env.merge(&else_env);

                    last_stmt_type = Some(then_fall.join(&else_fall));

                    // Propagate explicit returns. Issue #4273: aggregate the
                    // branch returns with comparison-aware `join_limited` using
                    // the previously-accumulated return as `compare_to`, so a
                    // small structured union built from `if`/`elseif` returns
                    // (e.g. `Union{Int64, Tuple{...}}`) is preserved instead of
                    // being collapsed to `Any` by plain `join`'s unconditional
                    // complexity bound.
                    for opt in [then_ret, else_ret].into_iter().flatten() {
                        return_type = Some(if let Some(prev) = return_type {
                            prev.join_limited(&opt, &prev)
                        } else {
                            opt
                        });
                    }
                    // The final `if` falls through if either branch can fall through.
                    if !then_may_fall && !else_may_fall {
                        may_fallthrough = false;
                    }
                    handled_inline = true;
                }
                Stmt::Try {
                    try_block,
                    catch_block,
                    else_block,
                    finally_block,
                    ..
                } if is_last => {
                    // Final `try`: similar inline handling
                    let (try_ret, try_fall, try_may_fall) = self.infer_block_branch(try_block, env);
                    let (else_ret, else_fall_val, else_may_fall) = if let Some(blk) = else_block {
                        self.infer_block_branch(blk, env)
                    } else {
                        (
                            None,
                            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                                CorePrimitive::Nothing,
                            ))),
                            true,
                        )
                    };
                    let (catch_ret, catch_fall, catch_may_fall) = if let Some(blk) = catch_block {
                        self.infer_block_branch(blk, env)
                    } else {
                        (
                            None,
                            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                                CorePrimitive::Nothing,
                            ))),
                            true,
                        )
                    };
                    if let Some(blk) = finally_block {
                        let _ = self.infer_block(blk, env);
                    }

                    let primary_fall = if else_block.is_some() {
                        else_fall_val
                    } else {
                        try_fall
                    };
                    last_stmt_type = Some(primary_fall.join(&catch_fall));

                    // Issue #4273: comparison-aware aggregation of `try`/`catch`/
                    // `else` returns, mirroring the `if` slice above.
                    for opt in [try_ret, else_ret, catch_ret].into_iter().flatten() {
                        return_type = Some(if let Some(prev) = return_type {
                            prev.join_limited(&opt, &prev)
                        } else {
                            opt
                        });
                    }
                    // The final `try` falls through if any reachable arm can fall through.
                    let primary_may_fall = if else_block.is_some() {
                        else_may_fall
                    } else {
                        try_may_fall
                    };
                    if !primary_may_fall && !catch_may_fall {
                        may_fallthrough = false;
                    }
                    handled_inline = true;
                }
                _ => {
                    last_stmt_type = Some(LatticeType::Concrete(ConcreteType::Core(
                        CoreType::Primitive(CorePrimitive::Nothing),
                    )));
                }
            }

            if !handled_inline {
                match self.infer_stmt(stmt, env) {
                    StmtResult::Continue => {}
                    StmtResult::Return(ty) => {
                        // Issue #4273: comparison-aware aggregation of explicit
                        // returns against the previously-accumulated return.
                        return_type = Some(if let Some(existing) = return_type {
                            existing.join_limited(&ty, &existing)
                        } else {
                            ty
                        });
                        // Unconditional return: subsequent statements are
                        // unreachable, and the block does not fall through.
                        may_fallthrough = false;
                    }
                    StmtResult::MaybeReturn(ty) => {
                        // Issue #4273: comparison-aware aggregation (a loop-body
                        // / conditional return joined into the block return).
                        return_type = Some(if let Some(existing) = return_type {
                            existing.join_limited(&ty, &existing)
                        } else {
                            ty
                        });
                        // Conditional return (e.g., loop body return): subsequent
                        // statements are still reachable when the return path
                        // does not fire (Issue #3547).
                    }
                    StmtResult::Diverges => {
                        // The statement never falls through (e.g., a
                        // `while true` loop with no `break` / `return`).
                        // Drop the implicit value to `Bottom` so the
                        // enclosing block's fallthrough collapses to
                        // `Union{}`, mark the block as non-falling, and
                        // stop iterating — subsequent statements are
                        // unreachable in the abstract domain. (Issue #4679)
                        last_stmt_type = Some(LatticeType::Bottom);
                        may_fallthrough = false;
                        if InferenceTracer::is_enabled() {
                            record_event(TraceEvent::Statement {
                                index: i,
                                kind: stmt_kind(stmt),
                                span: stmt.span(),
                                env_after: snapshot_env(env),
                            });
                        }
                        self.record_statement_type(i, last_stmt_type.clone());
                        break;
                    }
                }
            }

            // Inference trace (Issue #3512): snapshot the env after each
            // statement so the developer can see how types evolve. Cheap
            // when the tracer is disabled (it's the default).
            if InferenceTracer::is_enabled() {
                record_event(TraceEvent::Statement {
                    index: i,
                    kind: stmt_kind(stmt),
                    span: stmt.span(),
                    env_after: snapshot_env(env),
                });
            }
            self.record_statement_type(i, last_stmt_type.clone());
        }

        let fallthrough = last_stmt_type.unwrap_or(LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Nothing),
        )));
        (return_type, fallthrough, may_fallthrough)
    }

    /// Applies MustAlias-style field-path refinement invalidation for any
    /// `setfield!`/`setproperty!` builtin call form found within `expr`.
    ///
    /// Issue #4854: the surface `x.f = v` form lowers to `Stmt::FieldAssign`,
    /// which drops the `x.f` path refinement established by a guard such as
    /// `x.f isa T`. The builtin call forms `setfield!(x, :f, v)` and
    /// `setproperty!(x, :f, v)` perform the same mutation but appear as plain
    /// `Expr::Call`s (in statement position or as an assignment RHS), so they
    /// must invalidate the same refinement path or stale precision survives.
    ///
    /// A statically known `Symbol`/`String` field invalidates exactly that
    /// `x.field` path (matching `Stmt::FieldAssign`); a dynamic/unknown field
    /// conservatively drops every field/index path rooted at the object. Calls
    /// are walked recursively through arguments so nested writes are covered.
    fn apply_call_field_side_effects(&self, expr: &Expr, env: &mut TypeEnv) {
        if let Expr::Call { function, args, .. } = expr {
            if matches!(function.as_str(), "setfield!" | "setproperty!") && args.len() == 3 {
                if let Expr::Var(obj, _) = &args[0] {
                    match crate::compile::abstract_interp::conditional::extract_static_field_name(
                        &args[1],
                    ) {
                        Some(field) => env.invalidate_field_path(obj, field),
                        // Dynamic / unknown field: the write may land on any
                        // field, so conservatively drop all path refinements
                        // rooted at the object (keeps the object's own binding).
                        None => env.invalidate_var_paths(obj),
                    }
                }
            }
            for arg in args {
                self.apply_call_field_side_effects(arg, env);
            }
        }
    }

    fn extract_field_path_alias_source(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::FieldAccess { object, field, .. } => {
                crate::compile::abstract_interp::conditional::extract_field_narrow_path(
                    object, field,
                )
            }
            Expr::Call {
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                ..
            } => crate::compile::abstract_interp::conditional::extract_getfield_narrow_path(
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
            ),
            _ => None,
        }
    }

    /// Infers types for a statement.
    fn infer_stmt(&mut self, stmt: &Stmt, env: &mut TypeEnv) -> StmtResult {
        match stmt {
            Stmt::Block(block) => {
                let (return_type, _fallthrough, may_fallthrough) =
                    self.infer_block_branch(block, env);
                match return_type {
                    Some(ty) if may_fallthrough => StmtResult::MaybeReturn(ty),
                    Some(ty) => StmtResult::Return(ty),
                    None => StmtResult::Continue,
                }
            }

            Stmt::Assign { var, value, .. } => {
                let value_type = self.infer_expr(value, env);
                // Issue #4854: an assignment RHS like `y = setfield!(x, :f, v)`
                // still mutates `x.f`, so invalidate that field-path refinement
                // before rebinding `var` (matches Stmt::FieldAssign).
                self.apply_call_field_side_effects(value, env);
                let partial = self.infer_partial_struct_expr(value, env);
                let alias_source = match value {
                    Expr::Var(src, _) if src != var => Some(src.clone()),
                    _ => None,
                };
                let field_alias_source = self.extract_field_path_alias_source(value);
                env.set(var, value_type);
                // Issue #3504: rebinding the root variable invalidates any
                // MustAlias-style path refinements rooted at it (e.g.
                // `var.field` from a previous `isa(var.field, T)` guard) —
                // the new value may be a wholly different object.
                env.invalidate_var_paths(var);
                if let Some(src) = alias_source {
                    env.alias_root(var, &src);
                }
                if let Some(path) = field_alias_source {
                    env.alias_field_path(var, &path);
                }
                if let Some(partial) = partial {
                    env.set_partial_struct_fields(
                        var,
                        partial.struct_name,
                        partial.fields,
                        partial.field_order,
                    );
                }
                StmtResult::Continue
            }

            Stmt::FieldAssign {
                object,
                field,
                value,
                ..
            } => {
                // Issue #3504: assigning to one field invalidates only that
                // field's refinement; sibling fields of the same object are
                // unaffected. We still infer `value` for diagnostics.
                let _ = self.infer_expr(value, env);
                // Issue #4854: a `setfield!`/`setproperty!` embedded in the RHS
                // mutates its own object's field too.
                self.apply_call_field_side_effects(value, env);
                env.invalidate_field_path(object, field);
                StmtResult::Continue
            }

            Stmt::IndexAssign {
                array,
                indices,
                value,
                ..
            } => {
                // Issue #3504: index assignment invalidates element
                // refinements. If the index is a single constant we matched
                // when storing the path (see `extract_constant_index`), drop
                // exactly `arr[N]`; otherwise drop every `arr[*]` path.
                let _ = self.infer_expr(value, env);
                // Issue #4854: cover field writes embedded in RHS/index exprs.
                self.apply_call_field_side_effects(value, env);
                for idx in indices {
                    let _ = self.infer_expr(idx, env);
                    self.apply_call_field_side_effects(idx, env);
                }
                let precise_key: Option<String> = if indices.len() == 1 {
                    match &indices[0] {
                        Expr::Literal(Literal::Int(i), _) => Some(format!("{}[{}]", array, i)),
                        Expr::Literal(Literal::Bool(b), _) => {
                            Some(format!("{}[{}]", array, if *b { "true" } else { "false" }))
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(key) = precise_key {
                    env.remove(&key);
                } else {
                    env.invalidate_index_paths(array);
                }
                StmtResult::Continue
            }

            Stmt::DictAssign {
                dict, key, value, ..
            } => {
                // Issue #3504: like `IndexAssign` for dicts. Without symbolic
                // key tracking we conservatively drop all element refinements
                // of `dict`.
                let _ = self.infer_expr(key, env);
                let _ = self.infer_expr(value, env);
                // Issue #4854: cover field writes embedded in key/value exprs.
                self.apply_call_field_side_effects(key, env);
                self.apply_call_field_side_effects(value, env);
                env.invalidate_index_paths(dict);
                StmtResult::Continue
            }

            Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
                let name = self
                    .resolve_callable_name(&func.name)
                    .unwrap_or_else(|| func.name.clone());
                let callable = if self.active_function.is_some() {
                    closure_lattice_type(name, env)
                } else {
                    function_lattice_type(name)
                };
                env.set(&func.name, callable);
                StmtResult::Continue
            }

            Stmt::DestructuringAssign { targets, value, .. } => {
                // Issue #3504: each rebound target loses its path refinements
                // for the same reason as `Stmt::Assign`. The bare-binding
                // type is widened to `Top` since destructuring doesn't tell
                // us per-element types here — that matches the pre-existing
                // (no-op) behavior; we only add the invalidation.
                let _ = self.infer_expr(value, env);
                for target in targets {
                    env.invalidate_var_paths(target);
                }
                StmtResult::Continue
            }

            Stmt::Return { value, .. } => {
                let return_type = if let Some(expr) = value {
                    let ty = self.infer_expr(expr, env);
                    // Issue #4854: a `setfield!`/`setproperty!` in the returned
                    // expression mutates a field; invalidate its refinement so a
                    // later read (in nested control flow) cannot see stale type.
                    self.apply_call_field_side_effects(expr, env);
                    ty
                } else {
                    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                        CorePrimitive::Nothing,
                    )))
                };
                StmtResult::Return(return_type)
            }

            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // Infer condition type
                let _ = self.infer_expr(condition, env);

                // Apply conditional narrowing
                let split = crate::compile::abstract_interp::conditional::split_env_by_condition_with_predicates_and_structs(
                    env,
                    condition,
                    &self.function_table,
                    &self.struct_table,
                );

                // Infer then branch with narrowed environment, separating explicit
                // returns from fallthrough values (Issues #3513, #3515).
                let mut then_env = split.then_env;
                if InferenceTracer::is_enabled() {
                    // Inference trace (Issue #3512): record post-narrowing
                    // branch envs at the split point.
                    record_event(TraceEvent::Branch {
                        kind: BranchKind::If,
                        then_env: snapshot_env(&then_env),
                        else_env: snapshot_env(&split.else_env),
                    });
                }
                let (then_return, then_fall, _then_may_fall) =
                    self.infer_block_branch(then_branch, &mut then_env);

                // Infer else branch with narrowed environment
                let mut else_env = split.else_env;
                let (else_return, else_fall, _else_may_fall) = if let Some(else_blk) = else_branch {
                    self.infer_block_branch(else_blk, &mut else_env)
                } else {
                    (
                        None,
                        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                            CorePrimitive::Nothing,
                        ))),
                        true,
                    )
                };

                if InferenceTracer::is_enabled() {
                    // Snapshot the post-branch envs so the trace shows what
                    // each side produced after running its body (Issue #3512).
                    record_event(TraceEvent::Branch {
                        kind: BranchKind::If,
                        then_env: snapshot_env(&then_env),
                        else_env: snapshot_env(&else_env),
                    });
                }

                // Merge environments from both branches
                *env = then_env;
                env.merge(&else_env);

                // Combine explicit returns from both branches.
                let mut combined_return: Option<LatticeType> = None;
                if let Some(t) = &then_return {
                    combined_return = Some(t.clone());
                }
                if let Some(e) = &else_return {
                    combined_return = Some(if let Some(prev) = combined_return {
                        prev.join(e)
                    } else {
                        e.clone()
                    });
                }

                // Backward-compat widening: when a branch has NO explicit return AND
                // its fallthrough is `Top` or `Bottom`, it typically signals an
                // exception path (`throw(...)`) or unknown call result. Earlier
                // inference treated such branches as contributing the fallthrough
                // type to the function return; preserve that to avoid breaking
                // dispatch in Pure Julia code that relies on widened returns
                // (e.g., `factorial(::Int64)`). Concrete branch values (assignments,
                // expressions) are excluded — those are the actual bugs (#3513).
                let is_widening =
                    |fall: &LatticeType| matches!(fall, LatticeType::Top | LatticeType::Bottom);
                if then_return.is_none() && is_widening(&then_fall) {
                    combined_return = Some(if let Some(prev) = combined_return {
                        prev.join(&then_fall)
                    } else {
                        then_fall.clone()
                    });
                }
                if else_return.is_none() && is_widening(&else_fall) {
                    combined_return = Some(if let Some(prev) = combined_return {
                        prev.join(&else_fall)
                    } else {
                        else_fall.clone()
                    });
                }

                match combined_return {
                    Some(ty)
                        if ty
                            != LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                                CorePrimitive::Nothing,
                            ))) =>
                    {
                        StmtResult::Return(ty)
                    }
                    _ => StmtResult::Continue,
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
                // Range-based for loop: for var in start:end or for var in start:step:end
                let start_ty = self.infer_expr(start, env);
                let end_ty = self.infer_expr(end, env);
                let step_ty = step.as_ref().map(|s| self.infer_expr(s, env));

                // Detect a provably non-empty constant range so a `break`
                // can narrow the post-loop env to the joined break-exit
                // env only — analogous to the `while true` slice in
                // Issue #4267. When the range may be empty (dynamic
                // bounds, or `start > end` with positive step) the
                // pre-loop env must still fall through. (Issue #4680)
                let range_non_empty =
                    Self::range_provably_non_empty(&start_ty, &end_ty, step_ty.as_ref());

                // Loop variable type derived from range element promotion (Issue #3518)
                let elem_ty = self.range_element_type(&start_ty, &end_ty, step_ty.as_ref());
                env.set(var, elem_ty);

                // Snapshot environment before loop
                let pre_loop_env = env.snapshot();

                // Maintain the break-env stack so any `Stmt::Break` inside
                // the body targets this for-loop's slot, not a misleading
                // enclosing loop slot (Issue #4267). For a provably
                // non-empty constant range the popped env is consumed
                // below to narrow the post-loop env (Issue #4680).
                self.enter_loop();

                // Infer loop body with fixpoint iteration (Issue #3360: reuse body_env)
                let mut changed = true;
                let mut iterations = 0;
                let mut body_env = env.clone();
                let mut accumulated_return: Option<LatticeType> = None;

                while changed && iterations < MAX_LOOP_FIXPOINT_ITERATIONS {
                    iterations += 1;

                    body_env.clone_from(env);
                    let body_return = self.infer_block_explicit_return_only(body, &mut body_env);

                    // Merge updated types from loop body
                    changed = env.merge_changed(&body_env);

                    // Collect body returns (Issue #3516): a loop with a body return
                    // may execute zero iterations, so we must not short-circuit. The
                    // accumulated return is surfaced via `StmtResult::Return` below
                    // and joined with post-loop fallthrough by `infer_block_branch`.
                    //
                    // Issue #3507: use comparison-aware `join_limited` here — the
                    // previous accumulated return type provides a natural
                    // `compare_to` so that already-seen members of a growing
                    // union are not counted as new complexity. This is the first
                    // call site to migrate away from fixed-length union widening;
                    // the remaining sites (other loop accumulators, branch joins,
                    // arithmetic dispatch joins, etc.) still use plain `join` and
                    // will be migrated in follow-up PRs.
                    if !matches!(
                        body_return,
                        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                            CorePrimitive::Nothing
                        )))
                    ) {
                        accumulated_return = Some(if let Some(prev) = accumulated_return {
                            prev.join_limited(&body_return, &prev)
                        } else {
                            body_return
                        });
                    }
                }

                let break_env = self.exit_loop();

                // When the range is provably non-empty (Issue #4680) and
                // the body contains a `break` at top level — not nested
                // inside an `if`/`try`/etc. — that break is the dominant
                // exit: every iteration eventually reaches it, so the
                // pre-loop env never falls through and the body's natural
                // end-of-iteration env is unreachable post-loop. Replace
                // the env with the joined break-exit env, mirroring the
                // statically-true while slice in Issue #4267.
                //
                // A break nested inside an `if`/`try`/etc. (conditional
                // break) still leaves the end-of-range exit reachable for
                // paths that never break, so we conservatively fall back
                // to the existing pre-loop merge and accept the wider
                // `Union{pre, body_fixpoint, break_env}` post-loop env.
                // Upstream Julia behaves the same way for that pattern.
                let body_has_direct_break =
                    body.stmts.iter().any(|s| matches!(s, Stmt::Break { .. }));
                if range_non_empty && body_has_direct_break {
                    if let Some(b) = break_env {
                        *env = b;
                    } else {
                        // No break records reached `loop_break_envs`
                        // despite a top-level `Stmt::Break`; keep the
                        // existing behavior as a safety net.
                        env.merge(&pre_loop_env);
                    }
                } else if range_non_empty {
                    // A provably non-empty constant range runs at least once,
                    // so the pre-loop env never falls through: the post-loop
                    // state is the body's fixpoint exit env. Taking it directly
                    // (instead of re-merging the pre-loop snapshot) avoids
                    // over-widening a body-reassigned carried variable — e.g.
                    // `x = 0; for i in 1:10; x = x + 1.0 end` infers `Float64`
                    // post-loop, not `Union{Float64, Int64}`, matching upstream
                    // Julia. Mirrors the non-empty-range break narrowing of
                    // Issue #4680. (Issue #4267)
                    *env = body_env;
                } else {
                    // After loop: merge pre-loop types directly into post-loop env (Issue #3360)
                    env.merge(&pre_loop_env);
                }

                match accumulated_return {
                    // Loops may execute zero iterations, so a body return is
                    // conditional — surface as MaybeReturn so post-loop fallthrough
                    // is joined into the function return (Issue #3547).
                    Some(ty) => StmtResult::MaybeReturn(ty),
                    None => StmtResult::Continue,
                }
            }

            Stmt::ForEach {
                var,
                iterable,
                body,
                ..
            } => {
                // Infer the type of the iterable
                let iterable_ty = self.infer_expr(iterable, env);

                // Extract element type from the iterable
                let elem_ty =
                    crate::compile::abstract_interp::loop_analysis::element_type(&iterable_ty);

                // Set initial loop variable type
                env.set(var, elem_ty.clone());

                // Snapshot environment before loop
                let pre_loop_env = env.snapshot();

                // Maintain the break-env stack so any `Stmt::Break` inside
                // the body targets this foreach-loop's slot (Issue #4267).
                self.enter_loop();

                // Infer loop body with fixpoint iteration (Issue #3360: reuse body_env)
                let mut changed = true;
                let mut iterations = 0;
                let mut body_env = env.clone();
                let mut accumulated_return: Option<LatticeType> = None;

                while changed && iterations < MAX_LOOP_FIXPOINT_ITERATIONS {
                    iterations += 1;

                    body_env.clone_from(env);
                    let body_return = self.infer_block_explicit_return_only(body, &mut body_env);

                    // Merge updated types from loop body
                    changed = env.merge_changed(&body_env);

                    // Issue #4273: use comparison-aware `join_limited` for the
                    // accumulated loop-body return, mirroring the range-based
                    // `Stmt::For` slice above. The previous accumulated return
                    // type is the natural `compare_to`, so members already seen
                    // across iterations are not re-counted as new complexity
                    // and a small union (`Union{T, Nothing}`, etc.) carried out
                    // of a `for x in collection` loop is preserved rather than
                    // collapsed by the unconditional complexity/length bound in
                    // plain `join`.
                    if !matches!(
                        body_return,
                        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                            CorePrimitive::Nothing
                        )))
                    ) {
                        accumulated_return = Some(if let Some(prev) = accumulated_return {
                            prev.join_limited(&body_return, &prev)
                        } else {
                            body_return
                        });
                    }
                }

                let _ = self.exit_loop();

                // After loop: merge pre-loop types directly into post-loop env (Issue #3360)
                env.merge(&pre_loop_env);

                match accumulated_return {
                    // Loops may execute zero iterations, so a body return is
                    // conditional — surface as MaybeReturn so post-loop fallthrough
                    // is joined into the function return (Issue #3547).
                    Some(ty) => StmtResult::MaybeReturn(ty),
                    None => StmtResult::Continue,
                }
            }

            Stmt::While {
                condition, body, ..
            } => {
                // A statically-true condition has no condition-false exit
                // edge; the loop can only leave via `break` (or
                // `return`/exception). Track that fact up front so we can
                // skip the pre-loop fallthrough merge and use the collected
                // break-exit envs as the post-loop env. The detection
                // covers two equivalent surface forms: a literal `true`
                // condition, and a variable whose abstract value is the
                // constant `true`. (Issue #4267)
                let condition_always_true = match condition {
                    Expr::Literal(Literal::Bool(true), _) => true,
                    Expr::Var(name, _) => matches!(
                        env.get(name),
                        Some(LatticeType::Const(ConstValue::Bool(true)))
                    ),
                    _ => false,
                };

                // Snapshot environment before loop
                let pre_loop_env = env.snapshot();

                // Push a break-env slot so any `Stmt::Break` inside the body
                // (including breaks inside nested if/try) records its
                // path-sensitive env into this loop's slot (Issue #4267).
                self.enter_loop();

                // Infer loop body with fixpoint iteration (Issue #3360: reuse body_env)
                let mut changed = true;
                let mut iterations = 0;
                let mut body_env = env.clone();
                let mut accumulated_return: Option<LatticeType> = None;
                let mut last_split_else: Option<TypeEnv> = None;

                while changed && iterations < MAX_LOOP_FIXPOINT_ITERATIONS {
                    iterations += 1;

                    // Infer condition type
                    let _ = self.infer_expr(condition, env);

                    // Apply type narrowing based on condition (Issue #2303)
                    // The loop body executes when condition is true, so use then_env
                    let split = crate::compile::abstract_interp::conditional::split_env_by_condition_with_predicates_and_structs(
                        env,
                        condition,
                        &self.function_table,
                        &self.struct_table,
                    );

                    // Track else_env from final iteration so we can apply false-branch
                    // narrowing to the post-loop environment (Issue #3517).
                    last_split_else = Some(split.else_env);

                    // Reuse body_env allocation across iterations (Issue #3360)
                    body_env.clone_from(&split.then_env);

                    // Infer the body with narrowed environment - only propagate explicit
                    // return statements. The loop body's implicit value (last expression)
                    // does NOT contribute to the enclosing function's return type (Issue #2241)
                    let body_return = self.infer_block_explicit_return_only(body, &mut body_env);

                    // Merge updated types from loop body back into main env
                    changed = env.merge_changed(&body_env);

                    // Issue #4273: comparison-aware `join_limited` for the
                    // accumulated `while`-body return, matching the `Stmt::For`
                    // slice. `prev` is the natural `compare_to` so an already
                    // accumulated small union is preserved across iterations
                    // instead of being widened by plain `join`'s unconditional
                    // complexity/length bound.
                    if !matches!(
                        body_return,
                        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                            CorePrimitive::Nothing
                        )))
                    ) {
                        accumulated_return = Some(if let Some(prev) = accumulated_return {
                            prev.join_limited(&body_return, &prev)
                        } else {
                            body_return
                        });
                    }
                }

                let break_env = self.exit_loop();

                if condition_always_true {
                    // No condition-false exit edge exists, so neither the
                    // pre-loop env nor the legacy `else_env` narrowing is
                    // valid as a post-loop contribution. The only paths
                    // that leave the loop are `break` (captured above) and
                    // `return`/exception (already routed via
                    // `accumulated_return`).
                    //
                    // When at least one `break` was seen, replace the
                    // body-fixpoint env with the joined break-exit env
                    // (Issue #4267).
                    //
                    // When no `break` was seen AND no explicit `return`
                    // accumulated, the loop never falls through: surface
                    // `Diverges` so the surrounding block's fallthrough
                    // collapses to `Bottom`, matching upstream Julia's
                    // `Union{}` post-loop env (Issue #4679). When a
                    // `return` was seen but no `break`, the only exit is
                    // the return; preserve the prior `MaybeReturn`
                    // behavior for now — tightening it to `Return` is
                    // tracked as a separate follow-up.
                    if let Some(b) = break_env {
                        *env = b;
                    } else if accumulated_return.is_none() {
                        return StmtResult::Diverges;
                    }
                } else {
                    // After loop: merge pre-loop types directly into post-loop env (Issue #3360)
                    env.merge(&pre_loop_env);

                    // Apply false-branch narrowing to the post-loop env: a normally-terminated
                    // while loop has condition false on exit (Issue #3517). Intersecting the
                    // loop fixpoint with the exit environment keeps loop-carried assignments
                    // precise when the body itself makes the condition false (Issue #4267).
                    if let Some(else_env) = last_split_else {
                        for var_name in else_env.vars().cloned().collect::<Vec<_>>() {
                            let post = env.get(&var_name).cloned();
                            if let (Some(post_ty), Some(exit_ty)) =
                                (post, else_env.get(&var_name).cloned())
                            {
                                let narrowed = post_ty.meet(&exit_ty);
                                if !matches!(narrowed, LatticeType::Bottom) {
                                    env.set(&var_name, narrowed);
                                }
                            }
                        }
                    }
                }

                match accumulated_return {
                    // Loops may execute zero iterations, so a body return is
                    // conditional — surface as MaybeReturn so post-loop fallthrough
                    // is joined into the function return (Issue #3547).
                    Some(ty) => StmtResult::MaybeReturn(ty),
                    None => StmtResult::Continue,
                }
            }

            Stmt::Expr { expr, .. } => {
                let _ = self.infer_expr(expr, env);
                // Issue #4854: a bare `setfield!(x, :f, v)` / `setproperty!`
                // statement mutates `x.f`, so invalidate the same field-path
                // refinement the surface `x.f = v` (Stmt::FieldAssign) form does.
                self.apply_call_field_side_effects(expr, env);
                StmtResult::Continue
            }

            Stmt::Break { .. } => {
                // Capture the path-sensitive env at this break for the
                // enclosing loop's exit join (Issue #4267).
                self.record_break(env);
                StmtResult::Continue
            }
            Stmt::Continue { .. } => StmtResult::Continue,

            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                // Analyze try/catch/else, separating explicit returns from
                // implicit branch values (Issue #3514).
                let (try_return, try_fall, _try_may_fall) = self.infer_block_branch(try_block, env);

                let (catch_return, catch_fall, _catch_may_fall) =
                    if let Some(catch_blk) = catch_block {
                        self.infer_block_branch(catch_blk, env)
                    } else {
                        (
                            None,
                            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                                CorePrimitive::Nothing,
                            ))),
                            true,
                        )
                    };

                let (else_return, else_fall, _else_may_fall) = if let Some(else_blk) = else_block {
                    self.infer_block_branch(else_blk, env)
                } else {
                    (
                        None,
                        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                            CorePrimitive::Nothing,
                        ))),
                        true,
                    )
                };

                if let Some(finally_blk) = finally_block {
                    let _ = self.infer_block(finally_blk, env);
                }

                // Combine explicit returns from try, catch, else
                let mut combined: Option<LatticeType> = None;
                for opt in [&try_return, &catch_return, &else_return]
                    .iter()
                    .filter_map(|x| x.as_ref())
                {
                    combined = Some(if let Some(prev) = combined {
                        prev.join(opt)
                    } else {
                        opt.clone()
                    });
                }

                // Backward-compat: widen with branches whose fallthrough is Top/Bottom
                // (exception/unknown paths). See `Stmt::If` arm above.
                let is_widening =
                    |fall: &LatticeType| matches!(fall, LatticeType::Top | LatticeType::Bottom);
                for (ret, fall) in [
                    (&try_return, &try_fall),
                    (&catch_return, &catch_fall),
                    (&else_return, &else_fall),
                ] {
                    if ret.is_none() && is_widening(fall) {
                        combined = Some(if let Some(prev) = combined {
                            prev.join(fall)
                        } else {
                            fall.clone()
                        });
                    }
                }

                match combined {
                    Some(ty)
                        if ty
                            != LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                                CorePrimitive::Nothing,
                            ))) =>
                    {
                        StmtResult::Return(ty)
                    }
                    _ => StmtResult::Continue,
                }
            }

            _ => StmtResult::Continue,
        }
    }

    /// Infers the type of an expression.
    fn infer_expr(&mut self, expr: &Expr, env: &TypeEnv) -> LatticeType {
        match expr {
            Expr::Literal(lit, _) => self.infer_literal(lit),
            Expr::QuoteLiteral { constructor, .. } => {
                if let Some(symbol) = infer_simple_symbol_quote(constructor) {
                    LatticeType::Const(ConstValue::Symbol(symbol.to_string()))
                } else {
                    LatticeType::Top
                }
            }

            Expr::Var(name, _) => {
                if let Some(local) = env.get(name).cloned() {
                    // A local (parameter / assigned var) shadows any global of
                    // the same name; reading it creates no global-binding edge.
                    local
                } else if let Some(global) = self.global_types.get(name).cloned() {
                    // Issue #4285: record that the function currently under
                    // inference read this top-level binding, so a later change
                    // to it invalidates exactly this result.
                    self.record_global_read(name);
                    global
                } else {
                    self.resolve_callable_name(name)
                        .map(function_lattice_type)
                        .unwrap_or(LatticeType::Top)
                }
            }

            Expr::FunctionRef { name, .. } => {
                let name = self
                    .resolve_callable_name(name)
                    .unwrap_or_else(|| name.clone());
                function_lattice_type(name)
            }

            Expr::BinaryOp {
                op, left, right, ..
            } => {
                let left_ty = self.infer_expr(left, env);
                let right_ty = self.infer_expr(right, env);
                let op_name = binary_op_to_function(op);
                // Try constant folding first: if both operands are constants,
                // evaluate the operation at compile time
                if let Some(const_result) = try_eval_binary(&op_name, &left_ty, &right_ty) {
                    return const_result;
                }
                // Issue #3524: operators that always return Bool short-circuit
                // even when no tfunc is registered (===, !==, <:, &&, ||, !=).
                if binary_op_always_bool(op) {
                    let result = self
                        .tfuncs
                        .infer_return_type(&op_name, &[left_ty, right_ty]);
                    if matches!(result, LatticeType::Top) {
                        return LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                            CorePrimitive::Bool,
                        )));
                    }
                    return result;
                }
                // Fall back to transfer function
                self.tfuncs
                    .infer_return_type(&op_name, &[left_ty, right_ty])
            }

            Expr::UnaryOp { op, operand, .. } => {
                let operand_ty = self.infer_expr(operand, env);
                let op_name = unary_op_to_function(op);
                if matches!(op, crate::ir::core::UnaryOp::Not) {
                    if let Some(inner) = concrete_callable_from_lattice(&operand_ty) {
                        return LatticeType::Concrete(ConcreteType::ComposedFunction {
                            outer: Box::new(ConcreteType::Function {
                                name: "!".to_string(),
                            }),
                            inner: Box::new(inner),
                        });
                    }
                }
                // Try constant folding first: if operand is a constant,
                // evaluate the operation at compile time
                if let Some(const_result) = try_eval_unary(&op_name, &operand_ty) {
                    return const_result;
                }
                // Fall back to transfer function
                self.tfuncs.infer_return_type(&op_name, &[operand_ty])
            }

            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                let condition_ty = self.infer_expr(condition, env);
                if matches!(condition_ty, LatticeType::Const(ConstValue::Bool(true))) {
                    return self.infer_expr(then_expr, env);
                }
                if matches!(condition_ty, LatticeType::Const(ConstValue::Bool(false))) {
                    return self.infer_expr(else_expr, env);
                }

                let split =
                    crate::compile::abstract_interp::conditional::split_env_by_condition_with_predicates_and_structs(
                        env,
                        condition,
                        &self.function_table,
                        &self.struct_table,
                    );
                let then_ty = self.infer_expr(then_expr, &split.then_env);
                let else_ty = self.infer_expr(else_expr, &split.else_env);
                then_ty.join(&else_ty)
            }

            Expr::Call {
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                ..
            } => {
                if let Some(path) =
                    crate::compile::abstract_interp::conditional::extract_getfield_narrow_path(
                        function,
                        args,
                        kwargs,
                        splat_mask,
                        kwargs_splat_mask,
                    )
                {
                    if let Some(refined) = env.get_refinement(&path).or_else(|| env.get(&path)) {
                        return refined.clone();
                    }
                }

                let raw_arg_types: Vec<_> =
                    args.iter().map(|arg| self.infer_expr(arg, env)).collect();
                let Some(arg_types) =
                    expand_static_tuple_splat_arg_types(&raw_arg_types, splat_mask)
                else {
                    return LatticeType::Top;
                };
                let raw_kwarg_types: Vec<_> = kwargs
                    .iter()
                    .enumerate()
                    .map(|(idx, (name, value))| {
                        (
                            name.clone(),
                            self.infer_expr(value, env),
                            kwargs_splat_mask.get(idx).copied().unwrap_or(false),
                        )
                    })
                    .collect();
                let Some(explicit_kwarg_types) =
                    expand_static_namedtuple_kwarg_types(&raw_kwarg_types)
                else {
                    return LatticeType::Top;
                };

                // `sqrt` has Pure Julia methods for structs such as Complex, but
                // primitive numeric arguments use the builtin Float64 path. Prefer
                // the transfer function for arguments that are not definitely
                // structs so method-table inference cannot widen `sqrt(::Float64)`
                // to a struct.
                if function == "sqrt"
                    && arg_types.len() == 1
                    && !matches!(
                        &arg_types[0],
                        LatticeType::Concrete(ConcreteType::Struct { .. })
                    )
                {
                    return self.tfuncs.infer_return_type(function, &arg_types);
                }

                if function == "Dict" || function.starts_with("Dict{") {
                    let result = self.tfuncs.infer_return_type("Dict", &arg_types);
                    if !matches!(result, LatticeType::Top) {
                        return result;
                    }
                }

                if matches!(
                    function.as_str(),
                    "clamp" | "copysign" | "binomial" | "ndigits" | "widen"
                ) {
                    let result = self.tfuncs.infer_return_type(function, &arg_types);
                    if !matches!(result, LatticeType::Top) {
                        return result;
                    }
                }

                // Special handling for getfield with struct table lookup
                if function == "getfield" && args.len() >= 2 {
                    let field_name =
                        crate::compile::abstract_interp::conditional::extract_static_field_name(
                            &args[1],
                        )
                        .map(str::to_string);

                    if let Some(field) = &field_name {
                        if let Some(field_ty) =
                            self.infer_partial_struct_field(&args[0], field, env)
                        {
                            return field_ty;
                        }
                    }

                    // `getfield(s, i::Int)` with a constant integer index.
                    // Upstream `getfield_tfunc` resolves a `PartialStruct` field
                    // positionally via `_getfield_fieldindex`; mirror that so
                    // index access is as precise as named access (Issue #4269).
                    let const_index = field_name
                        .is_none()
                        .then(|| const_int_index(&arg_types[1]))
                        .flatten();
                    if let Some(index) = const_index {
                        if let Some(field_ty) =
                            self.infer_partial_struct_field_by_index(&args[0], index, env)
                        {
                            return field_ty;
                        }
                    }

                    if let LatticeType::Concrete(ConcreteType::Struct { name, .. }) = &arg_types[0]
                    {
                        // Try to extract field name from the second argument (literal Symbol)
                        if let Some(field) = field_name {
                            if let Some(struct_info) = self.struct_table.get(name) {
                                if let Some(field_ty) = struct_info.get_field_type(&field) {
                                    return field_ty.clone();
                                }
                            }
                        } else if let Some(index) = const_index {
                            // Plain (non-partial) struct value indexed positionally:
                            // resolve the declared field type via the struct's
                            // declaration order, matching named-field fallback.
                            if let Some(struct_info) = self.struct_table.get(name) {
                                if let Some(field) = usize::try_from(index - 1)
                                    .ok()
                                    .and_then(|i| struct_info.field_order().get(i))
                                {
                                    if let Some(field_ty) = struct_info.get_field_type(field) {
                                        return field_ty.clone();
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(partial) = self.infer_default_struct_constructor(function, &arg_types) {
                    return partial.result_type;
                }

                if function == "compose" && arg_types.len() == 2 {
                    if let (Some(outer), Some(inner)) = (
                        concrete_callable_from_lattice(&arg_types[0]),
                        concrete_callable_from_lattice(&arg_types[1]),
                    ) {
                        return LatticeType::Concrete(ConcreteType::ComposedFunction {
                            outer: Box::new(outer),
                            inner: Box::new(inner),
                        });
                    }
                }

                if let Some(return_type) =
                    self.infer_hof_call_return_type(function, args, &arg_types, env)
                {
                    return return_type;
                }

                if function == "ntuple" && args.len() == 2 {
                    if let Some(return_type) =
                        self.infer_ntuple_return_type(&arg_types[0], &arg_types[1], env)
                    {
                        return return_type;
                    }
                }

                if function == "collect" && args.len() == 1 {
                    if let LatticeType::Concrete(ConcreteType::Generator { element }) =
                        &arg_types[0]
                    {
                        return LatticeType::Concrete(ConcreteType::Array {
                            element: element.clone(),
                            ndims: None,
                        });
                    }
                    // An argument of unknown type (e.g. an unannotated parameter,
                    // inferred as Top or Any) must NOT fall through to the
                    // interprocedural `collect` analysis, which wrongly defaults the
                    // result element type to Float64. That made `collect(arr)[i]`
                    // infer Float64 and coerce a genuine Int64 (or even a String) on
                    // the function return (Issue #5669). Report `Array{Any}` so
                    // indexing the result infers `Any` and stays type-honest.
                    if matches!(
                        arg_types[0],
                        LatticeType::Top | LatticeType::Concrete(ConcreteType::Core(CoreType::Any))
                    ) {
                        return LatticeType::Concrete(ConcreteType::Array {
                            element: Box::new(ConcreteType::Core(CoreType::Any)),
                            ndims: None,
                        });
                    }
                }

                if let Some(return_type) =
                    self.infer_local_callable_call_return_type(function, &arg_types, env)
                {
                    return return_type;
                }

                // Create cache key with function name AND argument types
                // This enables polymorphic functions to be analyzed with different arg types
                let cache_identity = self
                    .function_table
                    .get(function)
                    .map(inference_cache_function_id)
                    .unwrap_or_else(|| function.to_string());
                let (cache_fn_id, cache_arg_types) =
                    call_cache_parts(&cache_identity, &arg_types, &explicit_kwarg_types);
                let cache_key = InferenceCacheKey::new(&cache_fn_id, &cache_arg_types);

                // Issue #4271 / #5603 / #5939: record the interprocedural dependency
                // edge from the function currently being inferred to this
                // callee before cache lookup. Precise dispatches get
                // signature-aware edges; imprecise calls keep conservative
                // bare-name edges.
                if !self.record_method_table_dependency_if_precise(function, &arg_types)
                    && !self.record_function_table_dependency_if_precise(function, &arg_types)
                {
                    self.record_call_dependency(function);
                }

                // Check if we have a cached return type for this (function,
                // arg_types) combination. World-gated (Issue #4271): a present
                // entry that was capped by a later method mutation is skipped.
                if let Some(cached) = self.lookup_return_cache(&cache_key) {
                    return cached.clone();
                }
                // Issue #3505: also consult tentative results (callees that
                // finished while inside an outer cycle). Reusing them within
                // the same outer iteration short-circuits redundant
                // re-analysis; they are invalidated at the start of every
                // outer iteration so the next iteration observes the
                // refined in-progress estimate.
                if let Some(tentative) = self.lookup_tentative_result(&cache_key) {
                    return tentative.clone();
                }

                if let Some(return_type) =
                    self.infer_method_table_call_return_type(function, &arg_types)
                {
                    if !matches!(return_type, LatticeType::Top)
                        || !self.function_table.contains_key(function)
                    {
                        return return_type;
                    }
                    // Method-table inference was too imprecise and the
                    // function table will be consulted below, so keep the
                    // conservative direct function edge as well.
                    self.record_call_dependency(function);
                }

                // Try interprocedural analysis if the function is in our function table
                // Limit recursion depth to prevent stack overflow
                if let Some(func) = self.function_table.get(function).cloned() {
                    // Issue #7215: when the callee declares an explicit return
                    // type (`f(...)::T`), Julia guarantees the result is
                    // `convert(T, …)::T`, so `T` is a sound upper bound for the
                    // call-site return type. Short-circuit to the declared type
                    // instead of re-inferring the callee's body. For mutually
                    // recursive symbolic code (Symbolics' `_deriv` ⇄ `_deriv_*`),
                    // re-inferring the body at every call site blows up
                    // combinatorially: tentative cycle results are evicted at the
                    // start of each outer fixpoint iteration and never reach the
                    // long-lived cache until the whole cycle unwinds, so the same
                    // `(callee, arg_types)` work is repeated `depth × iterations ×
                    // branching` times. The declared type breaks that expansion.
                    // This mirrors the top-level `infer_function` fast path
                    // (compile/pipeline_ctx.rs builds a function's registered
                    // return type from its annotation without inferring the body),
                    // keeping call-site inference consistent with that. The
                    // declared type is world-stable and independent of any
                    // in-progress cycle estimate, so it is always safe to commit
                    // to the long-lived cache (the call dependency was already
                    // recorded above).
                    if let Some(declared_rt) = &func.return_type {
                        let declared_lattice = self.julia_type_to_lattice(declared_rt);
                        self.insert_return_cache(cache_key, declared_lattice.clone());
                        return declared_lattice;
                    }
                    // Issue #3527: on a recursive call cycle, return the
                    // current best estimate (initially `Bottom`) instead of
                    // `Top`. The outer fixpoint iteration of the function
                    // body then refines the recursive return as base cases
                    // settle.
                    if let Some(in_progress) = self.analyzing_functions.get(&cache_key) {
                        emit_recursive_cycle(vec![function.to_string()]);
                        if InferenceTracer::is_enabled() {
                            // Inference trace (Issue #3512): mirror the
                            // recursive cycle as a TraceEvent.
                            record_event(TraceEvent::RecursiveCycle {
                                functions: vec![function.to_string()],
                            });
                        }
                        return in_progress.clone();
                    }
                    if let Some(in_progress) =
                        self.lookup_in_progress_function_estimate(&cache_key.fn_id)
                    {
                        emit_recursive_cycle(vec![function.to_string()]);
                        if InferenceTracer::is_enabled() {
                            record_event(TraceEvent::RecursiveCycle {
                                functions: vec![function.to_string()],
                            });
                        }
                        return in_progress;
                    }
                    if self.analysis_depth < MAX_INTERPROCEDURAL_ANALYSIS_DEPTH {
                        // Seed with `Bottom` so recursive callees join into
                        // the base-case result rather than poisoning to Top.
                        self.analyzing_functions
                            .insert(cache_key.clone(), LatticeType::Bottom);
                        let previous_active_estimate = self
                            .active_function_estimates
                            .insert(cache_key.fn_id.clone(), LatticeType::Bottom);
                        self.analysis_depth += 1;

                        // Create a fresh environment with argument types
                        // bound to parameters, including varargs packing
                        // (Issue #3526). The shared helper reuses the same
                        // binding rules as `infer_function_with_arg_types`.
                        let bindings = {
                            let engine = &*self;
                            bind_call_args_to_params(&func.params, &arg_types, |ty| {
                                engine.julia_type_to_lattice(ty)
                            })
                        };
                        let mut call_env = TypeEnv::new();
                        for (name, ty) in bindings {
                            call_env.set(&name, ty);
                        }
                        self.bind_kwparam_default_types(&func.kwparams, &mut call_env);
                        Self::bind_explicit_kwarg_types(
                            &func.kwparams,
                            &explicit_kwarg_types,
                            &mut call_env,
                        );

                        // Outer fixpoint: between iterations, update the
                        // in-progress estimate so recursive calls observe
                        // the latest type and the recursive edge converges.
                        // (Issue #3527 — bounded fixpoint over recursive cycles.)
                        //
                        // Issue #3505: at the start of each outer iteration
                        // we evict tentative results so callees from the
                        // previous iteration get re-analyzed against the
                        // latest in-progress estimate. Without this, mutual
                        // recursion (f → g → f) caches g's result against an
                        // initial `Bottom` for f and never refines it,
                        // producing a poisoned cache entry for g.
                        let mut return_type = LatticeType::Bottom;
                        let mut converged = false;
                        for _ in 0..MAX_RECURSIVE_FIXPOINT_ITERATIONS {
                            self.tentative_results.clear();
                            let mut iter_env = call_env.clone();
                            self.statement_types.insert(func.name.clone(), Vec::new());
                            let previous_active = self
                                .replace_active_context(func.name.clone(), cache_key.fn_id.clone());
                            let next = self.infer_block_with_fixpoint(&func.body, &mut iter_env);
                            self.restore_active_context(previous_active);
                            if next == return_type {
                                converged = true;
                                break;
                            }
                            return_type = next;
                            self.analyzing_functions
                                .insert(cache_key.clone(), return_type.clone());
                            self.active_function_estimates
                                .insert(cache_key.fn_id.clone(), return_type.clone());
                        }
                        if !converged {
                            crate::compile::infer_metrics::record_recursive_fixpoint_limit_hit();
                            self.mark_limited(
                                cache_key.clone(),
                                function,
                                "recursive fixpoint iteration limit",
                                &return_type,
                            );
                        }

                        self.analysis_depth -= 1;
                        self.analyzing_functions.remove(&cache_key);
                        if let Some(previous) = previous_active_estimate {
                            self.active_function_estimates
                                .insert(cache_key.fn_id.clone(), previous);
                        } else {
                            self.active_function_estimates.remove(&cache_key.fn_id);
                        }
                        // Issue #3505: only commit to the long-lived cache
                        // once we are out of the cycle entirely. While any
                        // ancestor frame is still in progress, this result
                        // depends on its non-final in-progress estimate —
                        // store it as tentative so the ancestor's outer
                        // fixpoint can re-evaluate it next iteration.
                        if self.analyzing_functions.is_empty() {
                            self.insert_return_cache(cache_key, return_type.clone());
                            // Also promote any sibling tentative entries
                            // from this cycle. They are now consistent with
                            // the converged result of the cycle leader.
                            for (k, v) in self.tentative_results.drain().collect::<Vec<_>>() {
                                if v.valid_worlds.contains(self.method_world) {
                                    self.insert_return_cache_if_absent(k, v.ty);
                                }
                            }
                        } else {
                            self.insert_tentative_result(cache_key, return_type.clone());
                        }
                        // The initial caller -> callee edge is recorded before
                        // cold callee inference, when the callee's own method
                        // edges may not exist yet. Re-record after inference so
                        // transitive method-identity dependencies are folded in
                        // before this caller cache is committed (Issue #6179).
                        if !self.record_method_table_dependency_if_precise(function, &arg_types)
                            && !self
                                .record_function_table_dependency_if_precise(function, &arg_types)
                        {
                            self.record_call_dependency(function);
                        }
                        return return_type;
                    }
                    // Depth limit reached: return the best in-progress estimate
                    // if this is part of an active cycle, otherwise Top.
                    let best_guess = self
                        .analyzing_functions
                        .get(&cache_key)
                        .cloned()
                        .unwrap_or(LatticeType::Top);
                    self.mark_limited(
                        cache_key,
                        function,
                        "interprocedural analysis depth limit",
                        &best_guess,
                    );
                    return best_guess;
                }

                // Use transfer function for built-in functions
                // Create context with struct table for contextual transfer functions
                let ctx = TFuncContext::with_struct_table(&self.struct_table);
                self.tfuncs
                    .infer_return_type_with_context(function, &arg_types, &ctx)
            }

            Expr::ModuleCall {
                module,
                function,
                args,
                ..
            } => {
                let arg_types: Vec<_> = args.iter().map(|arg| self.infer_expr(arg, env)).collect();

                if module == "Base" {
                    if let Some(return_type) =
                        self.infer_hof_call_return_type(function, args, &arg_types, env)
                    {
                        return return_type;
                    }

                    let ctx = TFuncContext::with_struct_table(&self.struct_table);
                    return self
                        .tfuncs
                        .infer_return_type_with_context(function, &arg_types, &ctx);
                }

                LatticeType::Top
            }

            Expr::Builtin { name, args, .. } => {
                // Issue #3525: certain builtins (zeros, ones, push!, etc.)
                // historically returned Top via "unknown_builtin", and downstream
                // method dispatch / coercion has come to depend on that
                // conservative result. Preserve the legacy behavior for those
                // ops while still routing the precise builtins (length, size,
                // zero, first, last, ...) through the registry.
                if builtin_op_should_widen_unknown(name) {
                    let _arg_types: Vec<_> =
                        args.iter().map(|arg| self.infer_expr(arg, env)).collect();
                    return LatticeType::Top;
                }
                let arg_types: Vec<_> = args.iter().map(|arg| self.infer_expr(arg, env)).collect();
                let builtin_name = builtin_op_to_function(name);
                let ctx = TFuncContext::with_struct_table(&self.struct_table);
                self.tfuncs
                    .infer_return_type_with_context(&builtin_name, &arg_types, &ctx)
            }

            Expr::ArrayLiteral { elements, .. } => {
                if elements.is_empty() {
                    // Empty array [] in Julia defaults to Vector{Any}
                    LatticeType::Concrete(ConcreteType::Array {
                        element: Box::new(ConcreteType::Core(CoreType::Any)),
                        ndims: None,
                    })
                } else {
                    // Infer element type as join of all elements
                    let mut element_type = self.infer_expr(&elements[0], env);
                    for elem in &elements[1..] {
                        let elem_ty = self.infer_expr(elem, env);
                        element_type = element_type.join(&elem_ty);
                    }

                    // Return Array{element_type}.
                    // Issue #3528: heterogeneous array literals (e.g.
                    // `[1, nothing]`) produce a Union element type. Preserve
                    // the array container shape with `Array{UnionOf(...)}`
                    // rather than collapsing the whole expression to `Top`.
                    match element_type {
                        LatticeType::Concrete(ct) => LatticeType::Concrete(ConcreteType::Array {
                            element: Box::new(ct),
                            ndims: None,
                        }),
                        LatticeType::Const(cv) => LatticeType::Concrete(ConcreteType::Array {
                            element: Box::new(cv.to_concrete_type()),
                            ndims: None,
                        }),
                        LatticeType::Union(types) => LatticeType::Concrete(ConcreteType::Array {
                            element: Box::new(ConcreteType::UnionOf(types.into_iter().collect())),
                            ndims: None,
                        }),
                        _ => {
                            // Element type is Top/Bottom/Conditional — we
                            // still know this expression is an Array. Use
                            // Array{Any} so downstream length/iterate/getindex
                            // remain useful (Issue #3528).
                            emit_unknown_array_element();
                            LatticeType::Concrete(ConcreteType::Array {
                                element: Box::new(ConcreteType::Core(CoreType::Any)),
                                ndims: None,
                            })
                        }
                    }
                }
            }

            Expr::Index { array, indices, .. } => {
                let array_ty = self.infer_expr(array, env);

                // For single-index access on Tuple, use constant index to get precise element type
                if indices.len() == 1 {
                    let index_ty = self.infer_expr(&indices[0], env);

                    // Check if we have a Tuple with a constant integer index
                    if let LatticeType::Concrete(ConcreteType::Tuple { elements }) = &array_ty {
                        if let LatticeType::Const(ConstValue::Int64(idx)) = &index_ty {
                            // Julia uses 1-based indexing
                            let idx_0based = (*idx - 1) as usize;
                            if idx_0based < elements.len() {
                                return LatticeType::Concrete(elements[idx_0based].clone());
                            }
                        }
                    }

                    // A single Range / AbstractVector index selects a SUB-ARRAY,
                    // not an element (`a[2:3]`, or `f(a, k) = a[k]` specialized for
                    // `f(arr, 2:3)` / `f(arr, [1,3])` where `k` is a runtime
                    // Range/Vector). Mirror the multi-index slice check below so the
                    // inferred result is an Array with the same element type rather
                    // than the scalar element type — otherwise the call-site return
                    // type stayed `Int64` and a `ReturnI64`/`StoreI64` coerced the
                    // sub-array (Issue #5747).
                    if matches!(
                        &index_ty,
                        LatticeType::Concrete(ConcreteType::Range { .. })
                            | LatticeType::Concrete(ConcreteType::Array { .. })
                    ) {
                        if let LatticeType::Concrete(ConcreteType::Array { element, .. }) =
                            &array_ty
                        {
                            return LatticeType::Concrete(ConcreteType::Array {
                                element: element.clone(),
                                ndims: None,
                            });
                        }
                        // Issue #6601: a String slice (`s[1:2]`) yields a String.
                        if let LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                            CorePrimitive::String,
                        ))) = &array_ty
                        {
                            return LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                                CorePrimitive::String,
                            )));
                        }
                        return LatticeType::Top;
                    }

                    // Issue #6657: a user `getindex` override for the receiver type
                    // must win over the builtin element-type transfer function.
                    if let Some(rt) = self.infer_user_override_index_return_type(&[
                        array_ty.clone(),
                        index_ty.clone(),
                    ]) {
                        return rt;
                    }

                    // Use getindex transfer function with actual index type
                    self.tfuncs
                        .infer_return_type("getindex", &[array_ty, index_ty])
                } else {
                    // Multi-dimensional indexing: pass all real index types so
                    // the transfer function can distinguish scalar indexing
                    // (`m[1, 1]`) from slice/range indexing (`m[1, :]`,
                    // `m[1:2, 1]`) (Issue #3529).
                    let mut tf_args = Vec::with_capacity(indices.len() + 1);
                    tf_args.push(array_ty.clone());
                    let mut any_slice_or_range = false;
                    for idx in indices {
                        if matches!(idx, Expr::SliceAll { .. }) {
                            any_slice_or_range = true;
                        }
                        let idx_ty = self.infer_expr(idx, env);
                        if matches!(idx_ty, LatticeType::Concrete(ConcreteType::Range { .. })) {
                            any_slice_or_range = true;
                        }
                        tf_args.push(idx_ty);
                    }

                    // If any index is a range / slice marker, the result is
                    // an array slice with the same element type.
                    if any_slice_or_range {
                        if let LatticeType::Concrete(ConcreteType::Array { element, .. }) =
                            &array_ty
                        {
                            return LatticeType::Concrete(ConcreteType::Array {
                                element: element.clone(),
                                ndims: None,
                            });
                        }
                    }

                    // Issue #6657: a user multi-dimensional `getindex` override
                    // (e.g. `getindex(::Matrix{Float64}, ::Int, ::Int)`) must win
                    // over the builtin element-type transfer function.
                    if !any_slice_or_range {
                        if let Some(rt) = self.infer_user_override_index_return_type(&tf_args) {
                            return rt;
                        }
                    }

                    self.tfuncs.infer_return_type("getindex", &tf_args)
                }
            }

            Expr::TupleLiteral { elements, .. } => {
                // A tuple literal is ALWAYS a `Tuple` in upstream Julia
                // (`typeof((1, "x", [])) == Tuple{...}`), regardless of element
                // types. Map each element's inferred lattice type to a concrete
                // element type, widening any non-concrete element (Top / Bottom /
                // Union / Conditional) to `ConcreteType::Core(CoreType::Any)` — i.e. `Tuple{...,
                // Any, ...}` — instead of collapsing the whole tuple to `Top`
                // (which the bridge maps to `ValueType::Any`). This keeps the
                // shared engine's tuple typing equal to the legacy pre-scan, which
                // returns `ValueType::Tuple` unconditionally (Issue #6601).
                let element_types: Vec<_> = elements
                    .iter()
                    .map(|e| match self.infer_expr(e, env) {
                        LatticeType::Concrete(ct) => ct,
                        LatticeType::Const(cv) => cv.to_concrete_type(),
                        _ => ConcreteType::Core(CoreType::Any),
                    })
                    .collect();

                LatticeType::Concrete(ConcreteType::Tuple {
                    elements: element_types,
                })
            }

            Expr::NamedTupleLiteral { fields, .. } => {
                let field_types: Vec<_> = fields
                    .iter()
                    .filter_map(|(name, expr)| match self.infer_expr(expr, env) {
                        LatticeType::Concrete(ct) => Some((name.clone(), ct)),
                        LatticeType::Const(cv) => Some((name.clone(), cv.to_concrete_type())),
                        _ => None,
                    })
                    .collect();

                if field_types.len() == fields.len() {
                    LatticeType::Concrete(ConcreteType::NamedTuple {
                        fields: field_types,
                    })
                } else {
                    LatticeType::Top
                }
            }

            Expr::Pair { key, value, .. } => {
                let key_ty = self.infer_expr(key, env);
                let value_ty = self.infer_expr(value, env);
                let key_concrete =
                    concrete_from_lattice(&key_ty).unwrap_or(ConcreteType::Core(CoreType::Any));
                let value_concrete =
                    concrete_from_lattice(&value_ty).unwrap_or(ConcreteType::Core(CoreType::Any));
                let pair_type_id = self
                    .struct_table
                    .get("Pair")
                    .map(|info| info.type_id)
                    .unwrap_or(0);

                LatticeType::Concrete(ConcreteType::Struct {
                    name: pair_type_name(&key_concrete, &value_concrete),
                    type_id: pair_type_id,
                })
            }

            Expr::DictLiteral { pairs, .. } => {
                if pairs.is_empty() {
                    return LatticeType::Concrete(ConcreteType::Dict {
                        key: Box::new(ConcreteType::Core(CoreType::Any)),
                        value: Box::new(ConcreteType::Core(CoreType::Any)),
                    });
                }

                let mut key_ty = self.infer_expr(&pairs[0].0, env);
                let mut value_ty = self.infer_expr(&pairs[0].1, env);
                for (key, value) in &pairs[1..] {
                    key_ty = key_ty.join(&self.infer_expr(key, env));
                    value_ty = value_ty.join(&self.infer_expr(value, env));
                }

                LatticeType::Concrete(ConcreteType::Dict {
                    key: Box::new(
                        concrete_from_lattice(&key_ty).unwrap_or(ConcreteType::Core(CoreType::Any)),
                    ),
                    value: Box::new(
                        concrete_from_lattice(&value_ty)
                            .unwrap_or(ConcreteType::Core(CoreType::Any)),
                    ),
                })
            }

            Expr::LetBlock { bindings, body, .. } => {
                let mut local_env = env.clone();
                for (name, value) in bindings {
                    let value_type = self.infer_expr(value, &local_env);
                    local_env.set(name, value_type);
                }
                self.infer_block(body, &mut local_env)
            }

            Expr::FieldAccess { object, field, .. } => {
                // Issue #3520: Conditional narrowing records `obj.field` in the
                // env for simple var.field access. Consult the env first so a
                // refined type from `isa(obj.field, T)` or `obj.field !== nothing`
                // guards is preserved instead of falling back to the struct's
                // declared field type.
                if let Some(path) =
                    crate::compile::abstract_interp::conditional::extract_field_narrow_path(
                        object, field,
                    )
                {
                    if let Some(refined) = env.get_refinement(&path).or_else(|| env.get(&path)) {
                        return refined.clone();
                    }
                }

                // Infer the type of the object
                let object_ty = self.infer_expr(object, env);

                if let Some(field_ty) = self.infer_partial_struct_field(object, field, env) {
                    return field_ty;
                }

                // If the object is a struct, look up the field type
                if let LatticeType::Concrete(ConcreteType::Struct { name, .. }) = &object_ty {
                    if let Some(struct_info) = self.struct_table.get(name) {
                        if let Some(field_ty) = struct_info.get_field_type(field) {
                            return field_ty.clone();
                        }
                        // Known struct but unknown field
                        emit_unknown_field(name, field);
                    } else {
                        // Emit diagnostic for unknown struct
                        DiagnosticsCollector::emit(
                            TypeInferenceDiagnostic::new(DiagnosticReason::UnknownStruct(Some(
                                name.clone(),
                            )))
                            .with_context(format!("field access on {}.{}", name, field)),
                        );
                    }
                }

                // Issue #6601: `Expr` is a builtin metaprogramming struct whose
                // fields have fixed types — `head::Symbol` and `args::Vector{Any}`.
                // It is not in the user struct table, so the struct-field path
                // above can't see it; mirror the legacy pre-scan's special-case
                // here. Benefits MAIN compilation too (was falling through to Top).
                if let LatticeType::Concrete(ConcreteType::Expr) = &object_ty {
                    match field.as_str() {
                        "head" => {
                            return LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                                CorePrimitive::Symbol,
                            )))
                        }
                        "args" => {
                            return LatticeType::Concrete(ConcreteType::Array {
                                element: Box::new(ConcreteType::Core(CoreType::Any)),
                                ndims: None,
                            })
                        }
                        _ => {}
                    }
                }

                // Field access over a Union: upstream `getfield_tfunc` joins the
                // per-member field types (Issue #5601). `(x::Union{A,B}).n` with
                // `A.n::Int64`, `B.n::Int64` infers `Int64`; divergent field types
                // join (e.g. `Union{Int64,Float64}`). A member that is not a struct
                // with the field would throw at runtime, so it contributes nothing
                // (`Bottom`) to the join, exactly mirroring upstream — the result is
                // the join over the members that DO have the field. Only fall
                // through to `Top` when no member can supply the field.
                if let LatticeType::Union(members) = &object_ty {
                    let mut joined: Option<LatticeType> = None;
                    for member in members {
                        let field_ty = match member {
                            ConcreteType::Struct { name, .. } => self
                                .struct_table
                                .get(name)
                                .and_then(|info| info.get_field_type(field)),
                            _ => None,
                        };
                        if let Some(ft) = field_ty {
                            joined = Some(match joined {
                                None => ft.clone(),
                                Some(acc) => acc.join(ft),
                            });
                        }
                    }
                    if let Some(result) = joined {
                        return result;
                    }
                }

                // Unknown struct or field: fall back to Top
                LatticeType::Top
            }

            Expr::Range {
                start, step, stop, ..
            } => {
                // Infer the element type from start, step, stop using Julia-style
                // numeric promotion (Issue #3519). `lattice::join` would produce a
                // Union for mixed numeric endpoints (e.g., Int64 ∪ Float64), but
                // ranges actually promote to a single element type.
                let start_ty = self.infer_expr(start, env);
                let stop_ty = self.infer_expr(stop, env);
                let step_ty = step.as_ref().map(|s| self.infer_expr(s, env));

                let element_ty = self.range_element_type(&start_ty, &stop_ty, step_ty.as_ref());

                match element_ty {
                    LatticeType::Concrete(ct) => LatticeType::Concrete(ConcreteType::Range {
                        element: Box::new(ct),
                    }),
                    // Genuinely heterogeneous (non-numeric or unknown) — fall back
                    _ => LatticeType::Concrete(ConcreteType::Range {
                        element: Box::new(ConcreteType::Core(CoreType::Primitive(
                            CorePrimitive::Int64,
                        ))),
                    }),
                }
            }

            Expr::Comprehension {
                body,
                var,
                iter,
                filter: _,
                ..
            }
            | Expr::Generator {
                body,
                var,
                iter,
                filter: _,
                ..
            } => {
                let iter_ty = self.infer_expr(iter, env);
                let element_ty = self
                    .iterator_element_lattice_type(&iter_ty)
                    .unwrap_or(ConcreteType::Core(CoreType::Any));
                let mut body_env = env.clone();
                body_env.set(var, LatticeType::Concrete(element_ty));
                let body_ty = self.infer_expr(body, &body_env);
                let body_concrete =
                    concrete_from_lattice(&body_ty).unwrap_or(ConcreteType::Core(CoreType::Any));

                if matches!(expr, Expr::Comprehension { .. }) {
                    LatticeType::Concrete(ConcreteType::Array {
                        element: Box::new(body_concrete),
                        ndims: None,
                    })
                } else {
                    LatticeType::Concrete(ConcreteType::Generator {
                        element: Box::new(body_concrete),
                    })
                }
            }

            _ => LatticeType::Top,
        }
    }

    /// Infers the type, exception result, and effects for an expression.
    ///
    /// This preserves the legacy `infer_expr` API as the return-type adapter
    /// while exposing the CallMeta-style fields needed by downstream call
    /// inference.
    pub fn infer_expr_result(&mut self, expr: &Expr, env: &TypeEnv) -> InferredExpr {
        let ty = self.infer_expr(expr, env);
        let mut effects = effect_inference::infer_expr_effects(expr);
        let arg_context = match expr {
            Expr::Call { args, .. } => Some(
                args.iter()
                    .map(|arg| {
                        (
                            self.infer_expr(arg, env),
                            effect_inference::infer_expr_effects(arg),
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
            Expr::BinaryOp { left, right, .. } => Some(vec![
                (
                    self.infer_expr(left, env),
                    effect_inference::infer_expr_effects(left),
                ),
                (
                    self.infer_expr(right, env),
                    effect_inference::infer_expr_effects(right),
                ),
            ]),
            _ => None,
        };
        let exct = exception_type_for_expr(expr, arg_context.as_deref(), &mut effects);
        InferredExpr { ty, exct, effects }
    }

    /// Infers the type of a literal.
    ///
    /// Returns `LatticeType::Const` for basic literals (Int, Float, Bool, String, Nothing)
    /// to enable constant propagation and folding during type inference.
    /// Falls back to `LatticeType::Concrete` for types not supported by ConstValue.
    /// Issue #3530: Int128/BigInt/BigFloat literals preserve their identity
    /// rather than being narrowed to Int64/Float64.
    ///
    /// Issue #5922: delegates to
    /// [`crate::compile::abstract_interp::local_authority::literal_to_lattice`],
    /// the single source of truth for literal → lattice typing that is now also
    /// consumed by the compiler pre-scan (`compile::inference`), so the engine
    /// and the pre-scan cannot drift apart on literal right-hand sides.
    fn infer_literal(&self, lit: &Literal) -> LatticeType {
        crate::compile::abstract_interp::local_authority::literal_to_lattice(lit)
    }

    /// Converts a Julia type annotation to a LatticeType.
    ///
    /// Issue #5916: delegates to the canonical
    /// [`crate::compile::bridge::julia_type_to_lattice_with_struct_resolver`]
    /// with the engine's `struct_table` as the struct-id resolver, so the
    /// engine and the compiler bridge cannot drift apart on annotation
    /// lowering (bit-width preservation, abstract numeric supertypes —
    /// Issue #3531 — union handling, and struct identity). Note: this is the
    /// *annotation* mapping; literal-side narrowing for `big"…"` is governed
    /// separately by `infer_literal` (Issue #3530) to avoid changing method
    /// specialization for big-literal callees.
    fn julia_type_to_lattice(&self, ty: &JuliaType) -> LatticeType {
        crate::compile::bridge::julia_type_to_lattice_with_struct_resolver(
            ty,
            Some(&|name: &str| self.struct_table.get(name).map(|info| info.type_id)),
        )
    }

    /// Gets the cached return type for a function with specific argument types, if available.
    ///
    /// This is a legacy name-based lookup. The primary cache is keyed by
    /// method identity (`inference_cache_function_id(func)`), so a bare function
    /// name can only fall back to primary entries when that base-name/argtype
    /// projection is unambiguous (Issue #5939).
    pub fn get_cached_return_type(
        &self,
        function_name: &str,
        arg_types: &[LatticeType],
    ) -> Option<&LatticeType> {
        let cache_key = InferenceCacheKey::new(function_name, arg_types);
        // World-gated (Issue #4271): expired entries (capped by a later method
        // mutation) are reported as absent so callers never observe a stale
        // inference result.
        if let Some(cached) = self.lookup_return_cache(&cache_key) {
            return Some(cached);
        }

        self.lookup_unique_return_cache_by_base_id(function_name, &cache_key.argtypes)
    }

    fn lookup_unique_return_cache_by_base_id(
        &self,
        function_name: &str,
        argtypes: &[CacheArgType],
    ) -> Option<&LatticeType> {
        let mut hit = None;
        for (key, cached) in &self.return_type_cache {
            if key.base_fn_id() == function_name
                && key.argtypes == argtypes
                && cached.valid_worlds.contains(self.method_world)
            {
                if hit.is_some() {
                    return None;
                }
                hit = Some(&cached.ty);
            }
        }
        hit
    }

    /// Test helper for the #5939 lookup-contract migration: query the primary
    /// method-identity cache key instead of the legacy bare-name key.
    #[cfg(test)]
    fn get_cached_return_type_for_function(
        &self,
        func: &Function,
        arg_types: &[LatticeType],
    ) -> Option<&LatticeType> {
        let cache_fn_id = inference_cache_function_id(func);
        let cache_key = InferenceCacheKey::new(&cache_fn_id, arg_types);
        self.lookup_return_cache(&cache_key)
    }

    /// Returns true if the given call signature was inferred with limited accuracy.
    pub fn is_limited_return_type(&self, function_name: &str, arg_types: &[LatticeType]) -> bool {
        let cache_key = InferenceCacheKey::new(function_name, arg_types);
        self.limited_results
            .get(&cache_key)
            .is_some_and(|cached| cached.valid_worlds.contains(self.method_world))
    }

    /// Returns the inferred type recorded for a lowered CFG statement payload.
    pub fn statement_type(&self, function_name: &str, stmt_id: usize) -> Option<&LatticeType> {
        self.statement_types
            .get(function_name)
            .and_then(|types| types.get(stmt_id))
    }

    /// Returns the CFG/worklist input environment recorded for a function block.
    pub fn cfg_block_input(&self, function_name: &str, block_id: BlockId) -> Option<&TypeEnv> {
        self.cfg_block_inputs
            .get(function_name)
            .and_then(|inputs| inputs.get(block_id.index()))
            .and_then(Option::as_ref)
    }

    /// Returns the CFG/worklist output environment recorded for a function block.
    pub fn cfg_block_output(&self, function_name: &str, block_id: BlockId) -> Option<&TypeEnv> {
        self.cfg_block_outputs
            .get(function_name)
            .and_then(|outputs| outputs.get(block_id.index()))
            .and_then(Option::as_ref)
    }

    fn record_statement_type(&mut self, stmt_id: usize, ty: Option<LatticeType>) {
        let Some(function_name) = self.active_function.clone() else {
            return;
        };
        let Some(ty) = ty else {
            return;
        };
        let types = self.statement_types.entry(function_name).or_default();
        if types.len() <= stmt_id {
            types.resize(stmt_id + 1, LatticeType::Bottom);
        }
        types[stmt_id] = ty;
    }

    fn bind_kwparam_default_types(&mut self, kwparams: &[KwParam], env: &mut TypeEnv) {
        for kwparam in kwparams {
            let ty = if kwparam.is_varargs {
                LatticeType::Concrete(ConcreteType::Pairs)
            } else if crate::compile::utils::is_required_kwarg(&kwparam.default) {
                kwparam
                    .type_annotation
                    .as_ref()
                    .map(|annotation| self.julia_type_to_lattice(annotation))
                    .unwrap_or(LatticeType::Top)
            } else if let Some(annotation) = &kwparam.type_annotation {
                self.julia_type_to_lattice(annotation)
            } else if is_unannotated_nothing_default_kwparam(kwparam) {
                LatticeType::Top
            } else {
                // A `nothing` default does not constrain an unannotated kwarg: a caller
                // may pass any value. Inferring `Nothing` (a singleton) would let the
                // body constant-fold `return kw` to the constant `nothing`, silently
                // dropping a passed value — `f(x; by=nothing) = by` would return
                // `nothing` for `f(1, by=10)` (Issue #5416). Widen to `Top` (Any).
                // This matches the slot-type handling in `compile/mod.rs`
                // (`KwParamInfo.ty` / `julia_type_locals`, PR #5424). Non-`nothing`
                // defaults keep their inferred type so reflection
                // (`Base.infer_return_type`) and call-site specialization stay precise
                // for the omitted-kwarg path. The compiled body's typed-return hazard
                // for non-`nothing` defaults is handled separately by widening the
                // function's `FunctionInfo.return_type` to `Any` in `compile/mod.rs`
                // (Issue #5425).
                let ty = self.infer_expr(&kwparam.default, env);
                if matches!(
                    ty,
                    LatticeType::Const(ConstValue::Nothing)
                        | LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                            CorePrimitive::Nothing
                        )))
                ) {
                    LatticeType::Top
                } else {
                    ty
                }
            };
            env.set(&kwparam.name, ty);
        }
    }

    fn bind_explicit_kwarg_types(
        kwparams: &[KwParam],
        explicit_kwarg_types: &[(String, LatticeType)],
        env: &mut TypeEnv,
    ) {
        for (name, ty) in explicit_kwarg_types {
            if kwparams
                .iter()
                .any(|kwparam| !kwparam.is_varargs && kwparam.name == *name)
            {
                env.set(name, ty.clone());
            }
        }
    }

    fn mark_limited(
        &mut self,
        cache_key: InferenceCacheKey,
        function_name: &str,
        reason: &str,
        result: &LatticeType,
    ) {
        let dependency_key = cache_key.fn_id.as_str();
        let edges = self.dependency_edges_for(dependency_key);
        let method_edges = self.method_edges_for(dependency_key);
        let global_reads = self.global_reads_for(dependency_key);
        self.limited_results.insert(
            cache_key,
            CachedLimitedAccuracy::new(self.method_world, edges, method_edges, global_reads),
        );
        emit_limited_accuracy(function_name, reason, &format!("{:?}", result));
    }

    /// Gets the cached return type for a function by name only (legacy compatibility).
    ///
    /// This method looks for any cached entry with the given function name.
    /// For precise lookups with argument types, use `get_cached_return_type` instead.
    pub fn get_cached_return_type_by_name(&self, function_name: &str) -> Option<&LatticeType> {
        // Find any live entry matching the function name. World-gated
        // (Issue #4271): entries expired by a later method mutation are skipped.
        for (key, cached) in &self.return_type_cache {
            if key.base_fn_id() == function_name && cached.valid_worlds.contains(self.method_world)
            {
                return Some(&cached.ty);
            }
        }
        None
    }

    /// Infer the return type of a `map(f, arr)` call by analyzing the function argument.
    ///
    /// This enables type inference like:
    /// - `map(x -> x + 1, [1, 2, 3])` returns `Array{Int64}`
    /// - `map(x -> Float64(x), [1, 2, 3])` returns `Array{Float64}`
    /// - `map(abs, [-1, -2])` returns `Array{Int64}`
    /// Computes the element type of a range expression from its endpoint and step
    /// types using Julia-style numeric promotion (Issues #3518, #3519).
    ///
    /// Unlike `LatticeType::join`, which yields `Union` for mixed numeric types
    /// (e.g., `Int64` ∪ `Float64`), Julia ranges promote element types so that
    /// `1:0.5:2` has `Float64` elements and `UInt8(1):UInt8(3)` keeps `UInt8`.
    /// Falls back to lattice join for non-numeric endpoint types.
    /// Returns `true` when a range with the given start, end, and optional
    /// step is provably non-empty at inference time. Only constant integer
    /// bounds are recognized today — Float64 and other numeric kinds are
    /// left as a follow-up because float ranges have subtle rounding edge
    /// cases that interact with step sign. (Issue #4680)
    fn range_provably_non_empty(
        start_ty: &LatticeType,
        end_ty: &LatticeType,
        step_ty: Option<&LatticeType>,
    ) -> bool {
        let start = match start_ty {
            LatticeType::Const(ConstValue::Int64(v)) => *v,
            _ => return false,
        };
        let end = match end_ty {
            LatticeType::Const(ConstValue::Int64(v)) => *v,
            _ => return false,
        };
        let step = match step_ty {
            None => 1,
            Some(LatticeType::Const(ConstValue::Int64(v))) => *v,
            _ => return false,
        };
        if step == 0 {
            return false;
        }
        if step > 0 {
            start <= end
        } else {
            start >= end
        }
    }

    pub(crate) fn range_element_type(
        &self,
        start_ty: &LatticeType,
        stop_ty: &LatticeType,
        step_ty: Option<&LatticeType>,
    ) -> LatticeType {
        fn lattice_to_name(ty: &LatticeType) -> Option<String> {
            match ty {
                LatticeType::Concrete(ct) => ct.to_type_name(),
                LatticeType::Const(cv) => cv.to_concrete_type().to_type_name(),
                _ => None,
            }
        }

        let start_name = lattice_to_name(start_ty);
        let stop_name = lattice_to_name(stop_ty);
        let step_name = step_ty.and_then(lattice_to_name);

        let names: Vec<String> = [start_name, stop_name, step_name]
            .into_iter()
            .flatten()
            .collect();

        if !names.is_empty()
            && names
                .iter()
                .all(|n| crate::compile::promotion::is_numeric_type_name(n))
        {
            let mut promoted = names[0].clone();
            for n in names.iter().skip(1) {
                promoted = crate::compile::promotion::promote_type(&promoted, n);
            }
            if let Some(ct) = ConcreteType::from_type_name(&promoted) {
                return LatticeType::Concrete(ct);
            }
        }

        // Fallback: lattice join for unknown/non-numeric types
        let mut joined = start_ty.join(stop_ty);
        if let Some(s) = step_ty {
            joined = joined.join(s);
        }
        joined
    }

    fn infer_partial_struct_expr(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Option<ConstructorPartial> {
        match expr {
            Expr::Var(var, _) => env
                .get_partial_struct(var)
                .map(|partial| ConstructorPartial {
                    result_type: env.get(var).cloned().unwrap_or(LatticeType::Top),
                    struct_name: partial.struct_name.clone(),
                    fields: partial.fields.clone(),
                    field_order: partial.field_order.clone(),
                    // The by-name side table does not carry recursive nested
                    // partials, so nested field facts conservatively widen here.
                    nested: HashMap::new(),
                }),
            // `getfield(obj, :field)` / `getfield(obj, i::Int)` where the field
            // is itself an analyzable immutable constructor: recover the nested
            // PartialStruct so a chained access keeps inner field precision,
            // mirroring upstream `getfield_tfunc` returning a `PartialStruct`
            // field that is itself a `PartialStruct` (Issue #4269).
            Expr::Call {
                function,
                args,
                splat_mask,
                ..
            } if function == "getfield"
                && args.len() >= 2
                && !splat_mask.iter().any(|is_splat| *is_splat) =>
            {
                if let Some(nested) =
                    self.infer_nested_partial_struct_field(&args[0], &args[1], env)
                {
                    return Some(nested);
                }
                // Not a nested-partial getfield; fall through to the generic
                // call handling (a user function literally named `getfield`
                // would be unusual, but the constructor/return paths below stay
                // sound for any non-getfield-builtin call).
                None
            }
            Expr::Call {
                function,
                args,
                splat_mask,
                ..
            } if !splat_mask.iter().any(|is_splat| *is_splat) => {
                let arg_types: Vec<_> = args.iter().map(|arg| self.infer_expr(arg, env)).collect();
                self.infer_default_struct_constructor(function, &arg_types)
                    .or_else(|| {
                        self.infer_function_partial_struct_return(function, &arg_types, env)
                    })
                    .map(|partial| self.attach_nested_constructor_facts(partial, args, env))
            }
            // Dot access `obj.field` over a partial whose field is itself an
            // analyzable immutable constructor (Issue #4269).
            Expr::FieldAccess { object, field, .. } => {
                let obj_partial = self.infer_partial_struct_expr(object, env)?;
                obj_partial.nested_field(field).cloned()
            }
            // Issue #4848: `new(...)` inside an analyzable immutable inner
            // constructor body. The constructor's struct is the function being
            // analyzed (its name equals the struct name), so the active function
            // identifies which struct's fields these `new` arguments populate.
            Expr::New {
                args,
                is_splat: false,
                ..
            } => self.infer_new_partial_struct(args, env),
            // A ternary whose both branches build the same immutable struct
            // shape produces a `ConstructorPartial` whose fields are the
            // lattice join of each branch's field types. This lets field
            // facts survive an inline branch constructor such as
            // `getfield(flag ? Ctor(1, "x") : Ctor(2, "y"), :b)` so the
            // surrounding `getfield` recovers the precise field type rather
            // than widening to `Any`. (Issue #4847)
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                let condition_ty = self.infer_expr(condition, env);
                if matches!(condition_ty, LatticeType::Const(ConstValue::Bool(true))) {
                    return self.infer_partial_struct_expr(then_expr, env);
                }
                if matches!(condition_ty, LatticeType::Const(ConstValue::Bool(false))) {
                    return self.infer_partial_struct_expr(else_expr, env);
                }

                let split =
                    crate::compile::abstract_interp::conditional::split_env_by_condition_with_predicates_and_structs(
                        env,
                        condition,
                        &self.function_table,
                        &self.struct_table,
                    );
                let then_partial = self.infer_partial_struct_expr(then_expr, &split.then_env);
                let else_partial = self.infer_partial_struct_expr(else_expr, &split.else_env);
                join_constructor_partials(then_partial, else_partial)
            }
            _ => None,
        }
    }

    /// Recover a nested PartialStruct fact from `getfield(obj, field_arg)` where
    /// `field_arg` is a literal `Symbol` or constant `Int64` index. Returns the
    /// nested partial recorded for that field on `obj`'s partial, if any. This
    /// mirrors upstream `getfield_tfunc` returning a `PartialStruct` field that
    /// is itself a `PartialStruct`, so a chained `getfield` keeps inner field
    /// precision (Issue #4269).
    fn infer_nested_partial_struct_field(
        &mut self,
        object: &Expr,
        field_arg: &Expr,
        env: &TypeEnv,
    ) -> Option<ConstructorPartial> {
        let obj_partial = self.infer_partial_struct_expr(object, env)?;

        if let Some(field) =
            crate::compile::abstract_interp::conditional::extract_static_field_name(field_arg)
        {
            return obj_partial.nested_field(field).cloned();
        }

        let index = const_int_index(&self.infer_expr(field_arg, env))?;
        obj_partial.nested_field_by_index(index).cloned()
    }

    /// Populate the per-field nested PartialStruct facts of a freshly built
    /// constructor partial by inferring each constructor argument that is itself
    /// an analyzable immutable constructor. This makes the partial recursive,
    /// matching upstream `Core.PartialStruct` (Issue #4269).
    fn attach_nested_constructor_facts(
        &mut self,
        mut partial: ConstructorPartial,
        args: &[Expr],
        env: &TypeEnv,
    ) -> ConstructorPartial {
        // Only the default-constructor positional case (argument count matches
        // the declared field order) can map arguments onto fields soundly; the
        // interprocedural return path produces no `field_order`-aligned args, so
        // skip when the shapes do not line up.
        if partial.field_order.len() != args.len() {
            return partial;
        }
        for (field, arg) in partial.field_order.clone().iter().zip(args.iter()) {
            if let Some(nested) = self.infer_partial_struct_expr(arg, env) {
                partial.nested.insert(field.clone(), Box::new(nested));
            }
        }
        partial
    }

    /// Builds PartialStruct-style field facts from a `new(...)` expression
    /// inside an inner constructor body (Issue #4848).
    ///
    /// The enclosing constructor is identified by `self.active_function`, whose
    /// name equals the struct name. The `new` arguments map positionally onto
    /// the struct's declared field order. Only immutable structs with a matching
    /// argument count are modeled — mutable structs do not preserve field-value
    /// facts and a mismatched count means this `new` form is not the simple
    /// positional case we can reason about.
    fn infer_new_partial_struct(
        &mut self,
        args: &[Expr],
        env: &TypeEnv,
    ) -> Option<ConstructorPartial> {
        // Inside an explicit parametric inner constructor (`Foo{Int64}(...)`),
        // `new{T}(...)` builds the concrete parametric instance, so prefer the
        // instantiated name and field order from the parametric definition
        // (Issue #4850).
        if let Some((instance_name, instance_type_id)) = self.active_parametric_instance.clone() {
            let base_name = self.active_function.clone()?;
            let parametric = self.parametric_structs.get(&base_name)?;
            if parametric.def.is_mutable {
                return None;
            }
            let field_order: Vec<String> = parametric
                .def
                .fields
                .iter()
                .map(|f| f.name.clone())
                .collect();
            if args.len() != field_order.len() {
                return None;
            }
            let arg_types: Vec<_> = args.iter().map(|arg| self.infer_expr(arg, env)).collect();
            let fields: HashMap<String, LatticeType> =
                field_order.iter().cloned().zip(arg_types).collect();
            let partial = ConstructorPartial {
                result_type: LatticeType::Concrete(ConcreteType::Struct {
                    name: instance_name.clone(),
                    type_id: instance_type_id,
                }),
                struct_name: instance_name,
                fields,
                field_order,
                nested: HashMap::new(),
            };
            return Some(self.attach_nested_constructor_facts(partial, args, env));
        }

        let struct_name = self.active_function.clone()?;
        let struct_info = self.struct_table.get(&struct_name)?;
        if struct_info.is_mutable {
            return None;
        }
        let field_order = struct_info.field_order().to_vec();
        let type_id = struct_info.type_id;
        if args.len() != field_order.len() {
            return None;
        }

        let arg_types: Vec<_> = args.iter().map(|arg| self.infer_expr(arg, env)).collect();
        let fields: HashMap<String, LatticeType> =
            field_order.iter().cloned().zip(arg_types).collect();

        let partial = ConstructorPartial {
            result_type: LatticeType::Concrete(ConcreteType::Struct {
                name: struct_name.clone(),
                type_id,
            }),
            struct_name,
            fields,
            field_order,
            nested: HashMap::new(),
        };
        Some(self.attach_nested_constructor_facts(partial, args, env))
    }

    fn infer_function_partial_struct_return(
        &mut self,
        function: &str,
        arg_types: &[LatticeType],
        env: &TypeEnv,
    ) -> Option<ConstructorPartial> {
        let resolved = self.resolve_callable_name(function)?;
        let func = self.function_table.get(&resolved)?.clone();
        if !self.record_function_table_dependency_if_precise(&resolved, arg_types) {
            self.record_call_dependency(&resolved);
        }
        let cache_key = InferenceCacheKey::new(&resolved, arg_types);
        if let Some(cached) = self.lookup_partial_struct_return_cache(&cache_key) {
            // `cached` is the analyzed outcome: `Some` (a recovered partial) or
            // `None` (a cached *negative* result — provably no precise partial,
            // Issue #7186). Either way the work is already done; serve it.
            return cached.cloned();
        }

        if self.analysis_depth >= MAX_INTERPROCEDURAL_ANALYSIS_DEPTH {
            return None;
        }

        // Recursion guard (Issue #7186). Recovering this callee's
        // `ConstructorPartial` re-analyses its body, which may re-enter the same
        // `(resolved, arg_types)` either directly (a self-recursive walker) or
        // through a cycle of mutually-recursive constructor-returning helpers.
        // The regular return-type path folds a tentative estimate on such a
        // back-edge (`analyzing_functions`); there is no equally-precise partial
        // to fold here, so a re-entry returns `None`. The result is *provisional*
        // (the value at the top of the cycle may yet be `Some`), so it is NOT
        // committed to the cache here — only a fully-analyzed outcome is.
        if !self.analyzing_partial_structs.insert(cache_key.clone()) {
            return None;
        }

        let bindings = {
            let engine = &*self;
            bind_call_args_to_params(&func.params, arg_types, |ty| {
                engine.julia_type_to_lattice(ty)
            })
        };

        let mut call_env = TypeEnv::new();
        if resolved.contains('#') {
            call_env = env.clone();
        }
        for (param_name, ty) in bindings {
            call_env.set(&param_name, ty);
        }

        self.analysis_depth += 1;
        let previous_active =
            self.replace_active_context(func.name.clone(), cache_key.fn_id.clone());
        let partial = self.infer_block_partial_struct_return(&func.body, &mut call_env);
        self.restore_active_context(previous_active);
        self.analysis_depth -= 1;
        self.analyzing_partial_structs.remove(&cache_key);

        // As with return-type inference, the first edge is recorded before
        // cold callee PartialStruct inference has discovered its own
        // dependencies. Re-record after inference so caller side-cache
        // entries inherit transitive method-identity edges (Issue #6181).
        if !self.record_function_table_dependency_if_precise(&resolved, arg_types) {
            self.record_call_dependency(&resolved);
        }
        // Memoize the fully-analyzed outcome — positive *and* negative. Caching
        // the negative case is what bounds a recursive constructor walker whose
        // helpers return `Union{Number, Struct}`: each `(callee, arg_types)` is
        // analyzed once per world instead of re-analyzed at every nested call
        // site, which otherwise fans out into a non-terminating re-analysis
        // (Issue #7186).
        self.insert_partial_struct_return_cache(cache_key, partial.clone());
        partial
    }

    fn infer_block_partial_struct_return(
        &mut self,
        block: &Block,
        env: &mut TypeEnv,
    ) -> Option<ConstructorPartial> {
        let mut last_expr_partial = None;

        for stmt in &block.stmts {
            match stmt {
                Stmt::Return {
                    value: Some(expr), ..
                } => return self.infer_partial_struct_expr(expr, env),
                Stmt::Return { value: None, .. } => return None,
                Stmt::Expr { expr, .. } => {
                    last_expr_partial = self.infer_partial_struct_expr(expr, env);
                    let _ = self.infer_stmt(stmt, env);
                }
                _ => {
                    last_expr_partial = None;
                    match self.infer_stmt(stmt, env) {
                        StmtResult::Return(_) => return None,
                        // `Diverges` means subsequent statements (including
                        // any constructor-bearing trailing expr) are
                        // unreachable, so there is no partial-struct
                        // result to surface. (Issue #4679)
                        StmtResult::Diverges => return None,
                        StmtResult::Continue | StmtResult::MaybeReturn(_) => {}
                    }
                }
            }
        }

        last_expr_partial
    }

    fn infer_partial_struct_field(
        &mut self,
        object: &Expr,
        field: &str,
        env: &TypeEnv,
    ) -> Option<LatticeType> {
        match object {
            Expr::Var(var, _) => env.get_partial_struct_field(var, field).cloned(),
            // `Call` builds a struct directly; `Ternary` joins the
            // per-branch constructor partials (Issue #4847). `FieldAccess`
            // recovers a nested partial from a `obj.inner.field` chain
            // (Issue #4269). All route through `infer_partial_struct_expr` so
            // the field type can be recovered from the object's partial.
            Expr::Call { .. } | Expr::Ternary { .. } | Expr::FieldAccess { .. } => self
                .infer_partial_struct_expr(object, env)
                .and_then(|partial| partial.fields.get(field).cloned()),
            _ => None,
        }
    }

    /// Resolve a struct field's inferred type from a 1-based positional index,
    /// mirroring upstream `getfield_tfunc` on a `PartialStruct` whose `name`
    /// argument is a constant integer (`_getfield_fieldindex` + bounds check).
    ///
    /// This complements [`Self::infer_partial_struct_field`] (the by-name path):
    /// `getfield(s, 1)` and `getfield(s, :x)` produce the same precise field
    /// type when the partial's field order is known. The 1-based index follows
    /// Julia's `getfield(x, i::Int)` indexing convention (Issue #4269).
    fn infer_partial_struct_field_by_index(
        &mut self,
        object: &Expr,
        index_1based: i64,
        env: &TypeEnv,
    ) -> Option<LatticeType> {
        match object {
            Expr::Var(var, _) => {
                let partial = env.get_partial_struct(var)?;
                let idx = usize::try_from(index_1based.checked_sub(1)?).ok()?;
                let name = partial.field_order.get(idx)?;
                partial.fields.get(name).cloned()
            }
            Expr::Call { .. } | Expr::Ternary { .. } => self
                .infer_partial_struct_expr(object, env)
                .and_then(|partial| partial.field_type_by_index(index_1based).cloned()),
            _ => None,
        }
    }

    /// Recover the concrete instantiated parametric struct (e.g. `Foo{Int64}`)
    /// plus per-field facts from a default constructor call whose type
    /// parameters can be bound from the actual argument types.
    ///
    /// Parametric structs are stored only by base name in `parametric_structs`
    /// (not in `struct_table`, which holds concrete instantiations registered
    /// lazily). Mirrors `SharedCompileContext::infer_type_args` so reflection's
    /// `infer_return_type` reports `Foo{Int64}` rather than widening to `Any`,
    /// and field facts survive `getfield(make_foo(...), :b)` lookups.
    /// (Issues #4849 / #4850 / #4851)
    fn infer_parametric_struct_constructor(
        &mut self,
        function: &str,
        arg_types: &[LatticeType],
    ) -> Option<ConstructorPartial> {
        // An explicit parametric constructor call such as `Foo{Int64}(...)`
        // arrives with the full instantiated name and the concrete type args
        // already spelled out. Route it to the inner-constructor analysis,
        // which binds those type args and resolves `new{T}(...)` to the
        // concrete struct (Issue #4850).
        if function.contains('{') {
            return self.infer_explicit_parametric_constructor(function, arg_types);
        }

        let parametric = self.parametric_structs.get(function)?;
        let def = &parametric.def;

        // Default-constructor inference only: a custom inner constructor may
        // transform arguments before `new`, so positional field facts would be
        // unsound without analyzing that body. The explicit `Foo{T}(...)` /
        // `new{T}(...)` inner-constructor cases are handled separately.
        if !def.inner_constructors.is_empty() || arg_types.len() != def.fields.len() {
            return None;
        }

        let julia_args: Vec<JuliaType> = arg_types
            .iter()
            .map(crate::compile::bridge::lattice_to_julia_type)
            .collect();
        let type_args =
            crate::compile::infer_parametric_type_args(def, function, &julia_args).ok()?;

        let type_args_str: Vec<String> = type_args.iter().map(|jt| jt.name().to_string()).collect();
        let instantiated_name = format!("{}{{{}}}", function, type_args_str.join(", "));

        // Prefer the real type_id if this instantiation is already registered;
        // otherwise the name (which carries braces) is sufficient for reflection.
        let type_id = self
            .struct_table
            .get(&instantiated_name)
            .map(|info| info.type_id)
            .unwrap_or(0);

        // Mutable structs do not preserve field-value facts (a later write can
        // change them), so leave both the facts and the positional order empty.
        let (fields, field_order): (HashMap<String, LatticeType>, Vec<String>) = if def.is_mutable {
            (HashMap::new(), Vec::new())
        } else {
            let field_order: Vec<String> = def.fields.iter().map(|f| f.name.clone()).collect();
            let fields = field_order
                .iter()
                .cloned()
                .zip(arg_types.iter().cloned())
                .collect();
            (fields, field_order)
        };

        Some(ConstructorPartial {
            result_type: LatticeType::Concrete(ConcreteType::Struct {
                name: instantiated_name.clone(),
                type_id,
            }),
            struct_name: instantiated_name,
            fields,
            field_order,
            // Populated by `attach_nested_constructor_facts` at the call site,
            // which has access to the argument expressions (Issue #4269).
            nested: HashMap::new(),
        })
    }

    /// Analyze an explicit parametric inner constructor call such as
    /// `Foo{Int64}(x)` and recover the concrete instantiated struct plus
    /// per-field facts from the constructor body's `new{T}(...)` (Issue #4850).
    ///
    /// The full instantiated name (`Foo{Int64}`) already carries the concrete
    /// type arguments, so analysis binds the constructor parameters to the
    /// actual argument types, marks the active parametric instance, and runs
    /// the standard partial-struct body analysis. The `new{T}(...)` inside the
    /// body then resolves to `Foo{Int64}` via [`Self::infer_new_partial_struct`].
    fn infer_explicit_parametric_constructor(
        &mut self,
        function: &str,
        arg_types: &[LatticeType],
    ) -> Option<ConstructorPartial> {
        if self.analysis_depth >= MAX_INTERPROCEDURAL_ANALYSIS_DEPTH {
            return None;
        }

        let (base_name, _type_args) = crate::compile::types::parse_parametric_call(function)?;
        let parametric = self.parametric_structs.get(&base_name)?.clone();
        let def = &parametric.def;
        if def.is_mutable {
            return None;
        }

        // Select the inner constructor whose (non-vararg) arity matches the call.
        let ctor = def
            .inner_constructors
            .iter()
            .find(|ctor| ctor.params.len() == arg_types.len())?;

        let bindings = {
            let engine = &*self;
            bind_call_args_to_params(&ctor.params, arg_types, |ty| {
                engine.julia_type_to_lattice(ty)
            })
        };
        let mut call_env = TypeEnv::new();
        for (param_name, ty) in bindings {
            call_env.set(&param_name, ty);
        }

        // The instantiated name is the call's full name (e.g. `Foo{Int64}`);
        // prefer the registered type_id if available.
        let instantiated_name = function.to_string();
        let type_id = self
            .struct_table
            .get(&instantiated_name)
            .map(|info| info.type_id)
            .unwrap_or(0);
        let body = ctor.body.clone();

        self.analysis_depth += 1;
        let previous_active = self.replace_active_context(base_name.clone(), base_name);
        let previous_instance = self
            .active_parametric_instance
            .replace((instantiated_name, type_id));
        let partial = self.infer_block_partial_struct_return(&body, &mut call_env);
        self.active_parametric_instance = previous_instance;
        self.restore_active_context(previous_active);
        self.analysis_depth -= 1;

        partial
    }

    fn infer_default_struct_constructor(
        &mut self,
        function: &str,
        arg_types: &[LatticeType],
    ) -> Option<ConstructorPartial> {
        if let Some(partial) = self.infer_parametric_struct_constructor(function, arg_types) {
            return Some(partial);
        }

        let struct_info = self.struct_table.get(function)?;

        // This side-table implementation only models default constructors.
        // Inner/custom constructors may transform arguments before `new`, so
        // preserving call argument types as field values would be unsound
        // without analyzing that constructor body.
        if struct_info.has_inner_constructor || arg_types.len() != struct_info.field_order().len() {
            return None;
        }

        let result_type = LatticeType::Concrete(ConcreteType::Struct {
            name: function.to_string(),
            type_id: struct_info.type_id,
        });

        // Mutable structs do not preserve field-value facts, so leave both the
        // facts and the positional order empty for them.
        let (fields, field_order): (HashMap<String, LatticeType>, Vec<String>) =
            if struct_info.is_mutable {
                (HashMap::new(), Vec::new())
            } else {
                let field_order = struct_info.field_order().to_vec();
                let fields = field_order
                    .iter()
                    .cloned()
                    .zip(arg_types.iter().cloned())
                    .collect();
                (fields, field_order)
            };

        Some(ConstructorPartial {
            result_type,
            struct_name: function.to_string(),
            fields,
            field_order,
            // Populated by `attach_nested_constructor_facts` at the call site,
            // which has access to the argument expressions (Issue #4269).
            nested: HashMap::new(),
        })
    }

    fn infer_method_table_call_return_type(
        &self,
        function: &str,
        arg_types: &[LatticeType],
    ) -> Option<LatticeType> {
        let table = self.method_tables.get(function)?;

        if let Some(split_arg_types) = split_union_call_arg_types(function, arg_types) {
            let mut joined = LatticeType::Bottom;
            for variant_arg_types in split_arg_types {
                if !method_table_args_are_precise(&variant_arg_types) {
                    return None;
                }
                let julia_arg_types = lattice_argtypes_to_julia(&variant_arg_types);
                let return_type = match table.dispatch(&julia_arg_types) {
                    Ok(method) => self.method_return_type_to_lattice(method),
                    Err(DispatchError::AmbiguousMethod { .. }) => LatticeType::Bottom,
                    Err(_) => return None,
                };
                joined = joined.join(&return_type);
            }
            return Some(joined);
        }

        if !method_table_args_are_precise(arg_types) {
            return None;
        }

        let julia_arg_types = lattice_argtypes_to_julia(arg_types);
        match table.dispatch(&julia_arg_types) {
            Ok(method) => Some(self.method_return_type_to_lattice(method)),
            Err(DispatchError::AmbiguousMethod { .. }) => Some(LatticeType::Bottom),
            Err(_) => None,
        }
    }

    /// Issue #6657: `xs[i]` / `m[i,j]` are inferred via the builtin `getindex`
    /// transfer function (element type), bypassing method-table dispatch. When
    /// the user has overridden `getindex` for the receiver type, the call must
    /// use the override's declared return type instead (e.g. a `Vector{Int64}`
    /// override returning `Symbol`). Returns `Some(return_type)` only when the
    /// `getindex` dispatch winner is a *user* method, so native arrays keep the
    /// precise element-type tfunc path untouched. `arg_types` is the full row
    /// `[array_ty, index_ty...]`.
    fn infer_user_override_index_return_type(
        &self,
        arg_types: &[LatticeType],
    ) -> Option<LatticeType> {
        let table = self.method_tables.get("getindex")?;
        if !method_table_args_are_precise(arg_types) {
            return None;
        }
        let julia_arg_types = lattice_argtypes_to_julia(arg_types);
        let method = table.dispatch(&julia_arg_types).ok()?;
        if table.is_base_program_global_index(method.global_index)
            || method.param_matches_at(
                0,
                crate::compile::method_table::core_type_is_free_typevar_array,
            )
        {
            // Base `getindex(::Array{T,N}, ::Int64)` etc. win: keep the builtin
            // element-type tfunc, whose array element precision the interprocedural
            // body inference cannot match. The free-typevar-array check also covers
            // origin classification being unavailable (e.g. a double-merged program).
            return None;
        }
        Some(self.method_return_type_to_lattice(method))
    }

    fn record_method_table_dependency_if_precise(
        &mut self,
        function: &str,
        arg_types: &[LatticeType],
    ) -> bool {
        let Some(table) = self.method_tables.get(function) else {
            return false;
        };

        let dispatched_arg_types =
            if let Some(split_arg_types) = split_union_call_arg_types(function, arg_types) {
                let mut dispatched = Vec::with_capacity(split_arg_types.len());
                for variant_arg_types in split_arg_types {
                    if !method_table_args_are_precise(&variant_arg_types) {
                        return false;
                    }
                    let julia_arg_types = lattice_argtypes_to_julia(&variant_arg_types);
                    match table.dispatch(&julia_arg_types) {
                        Ok(_) => dispatched.push(julia_arg_types),
                        Err(_) => return false,
                    }
                }
                dispatched
            } else {
                if !method_table_args_are_precise(arg_types) {
                    return false;
                }
                let julia_arg_types = lattice_argtypes_to_julia(arg_types);
                if table.dispatch(&julia_arg_types).is_err() {
                    return false;
                }
                vec![julia_arg_types]
            };

        for julia_arg_types in dispatched_arg_types {
            self.record_method_call_dependency(function, julia_arg_types);
        }
        true
    }

    fn record_function_table_dependency_if_precise(
        &mut self,
        function: &str,
        arg_types: &[LatticeType],
    ) -> bool {
        let Some(func) = self.function_table.get(function).cloned() else {
            return false;
        };

        let dispatched_arg_types =
            if let Some(split_arg_types) = split_union_call_arg_types(function, arg_types) {
                let mut dispatched = Vec::with_capacity(split_arg_types.len());
                for variant_arg_types in split_arg_types {
                    if !method_table_args_are_precise(&variant_arg_types)
                        || !Self::function_params_accept_arg_types(&func, &variant_arg_types)
                    {
                        return false;
                    }
                    dispatched.push(lattice_argtypes_to_julia(&variant_arg_types));
                }
                dispatched
            } else {
                if !method_table_args_are_precise(arg_types)
                    || !Self::function_params_accept_arg_types(&func, arg_types)
                {
                    return false;
                }
                vec![lattice_argtypes_to_julia(arg_types)]
            };

        for julia_arg_types in dispatched_arg_types {
            self.record_method_call_dependency(function, julia_arg_types);
        }
        true
    }

    fn function_params_accept_arg_types(func: &Function, arg_types: &[LatticeType]) -> bool {
        let Some(param_types) = expanded_function_param_types_for_arity(func, arg_types.len())
        else {
            return false;
        };
        let julia_arg_types = lattice_argtypes_to_julia(arg_types);
        dispatch_resolver::julia_signature_match_with_bindings(
            &param_types,
            &julia_arg_types,
            &func.type_params,
        )
        .is_some()
    }

    fn method_return_type_to_lattice(&self, method: &MethodSig) -> LatticeType {
        if let Some(return_julia_type) = &method.return_julia_type {
            self.julia_type_to_lattice(return_julia_type)
        } else {
            LatticeType::from(&method.return_type)
        }
    }

    fn infer_hof_call_return_type(
        &mut self,
        function: &str,
        args: &[Expr],
        arg_types: &[LatticeType],
        env: &TypeEnv,
    ) -> Option<LatticeType> {
        match function {
            "map" | "Base.map" if args.len() == 2 => {
                self.infer_map_return_type(&args[0], &arg_types[1], env)
            }
            "map" | "Base.map" if args.len() == 3 => {
                self.infer_binary_map_return_type(&args[0], &arg_types[1], &arg_types[2], env)
            }
            "map" | "Base.map" if args.len() >= 4 => {
                self.infer_nary_map_return_type(&args[0], &arg_types[1..], env)
            }
            "broadcast" | "Base.broadcast" if args.len() == 2 => {
                self.infer_map_return_type(&args[0], &arg_types[1], env)
            }
            "broadcast" | "Base.broadcast" if args.len() == 3 => {
                self.infer_binary_map_return_type(&args[0], &arg_types[1], &arg_types[2], env)
            }
            "broadcast" | "Base.broadcast" if args.len() >= 4 => {
                self.infer_nary_map_return_type(&args[0], &arg_types[1..], env)
            }
            "filter" | "Base.filter" if args.len() == 2 => {
                self.infer_filter_return_type(&arg_types[1])
            }
            "mapreduce" | "mapfoldl" | "mapfoldr" | "Base.mapreduce" | "Base.mapfoldl"
            | "Base.mapfoldr"
                if args.len() >= 3 =>
            {
                self.infer_mapreduce_return_type(&args[0], &args[1], &arg_types[2], env)
            }
            "reduce" | "foldl" | "foldr" | "Base.reduce" | "Base.foldl" | "Base.foldr"
                if args.len() >= 2 =>
            {
                self.infer_reduce_return_type(&args[0], &arg_types[1], env)
            }
            _ => None,
        }
    }

    fn infer_map_return_type(
        &mut self,
        func_arg: &Expr,
        array_type: &LatticeType,
        _env: &TypeEnv,
    ) -> Option<LatticeType> {
        let tuple_elements = match array_type {
            LatticeType::Concrete(ConcreteType::Tuple { elements }) => Some(elements.clone()),
            _ => None,
        };
        let element_types = if let Some(elements) = tuple_elements {
            elements
        } else if let LatticeType::Concrete(ConcreteType::Array { element, .. }) = array_type {
            vec![element.as_ref().clone()]
        } else {
            return None;
        };

        if matches!(
            func_arg,
            Expr::FunctionRef { name, .. } | Expr::Var(name, _)
                if matches!(
                    name.strip_prefix("function ").unwrap_or(name),
                    "iszero" | "isone" | "signbit" | "iseven" | "isodd"
                )
        ) {
            return single_or_tuple_map_return_type(vec![
                ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Bool
                ));
                element_types.len()
            ]);
        }

        if matches!(
            func_arg,
            Expr::FunctionRef { name, .. } | Expr::Var(name, _)
                if matches!(
                    name.strip_prefix("function ").unwrap_or(name),
                    "identity" | "abs" | "abs2" | "-"
                )
        ) {
            return single_or_tuple_map_return_type(element_types);
        }

        let (func_name, func) = self.map_callable_function(func_arg)?;

        // Limit recursion depth to prevent stack overflow
        let mut return_elements = Vec::with_capacity(element_types.len());
        for element_type in element_types {
            let return_type =
                self.infer_unary_callable_return_type(&func_name, &func, element_type, _env)?;
            return_elements.push(return_type);
        }

        single_or_tuple_map_return_type(return_elements)
    }

    fn infer_binary_map_return_type(
        &mut self,
        func_arg: &Expr,
        left_array_type: &LatticeType,
        right_array_type: &LatticeType,
        _env: &TypeEnv,
    ) -> Option<LatticeType> {
        let left_element = match left_array_type {
            LatticeType::Concrete(ConcreteType::Array { element, .. }) => element.as_ref().clone(),
            _ => return None,
        };
        let right_element = match right_array_type {
            LatticeType::Concrete(ConcreteType::Array { element, .. }) => element.as_ref().clone(),
            _ => return None,
        };

        if let Expr::FunctionRef { name, .. } | Expr::Var(name, _) = func_arg {
            let normalized_name = name.strip_prefix("function ").unwrap_or(name);
            if let Some(element) = binary_numeric_map_concrete_return_type(
                normalized_name,
                &left_element,
                &right_element,
            ) {
                return Some(LatticeType::Concrete(ConcreteType::Array {
                    element: Box::new(element),
                    ndims: None,
                }));
            }
        }

        None
    }

    fn infer_nary_map_return_type(
        &mut self,
        func_arg: &Expr,
        array_types: &[LatticeType],
        _env: &TypeEnv,
    ) -> Option<LatticeType> {
        if array_types.len() < 3 {
            return None;
        }

        let elements = array_types
            .iter()
            .map(|array_type| match array_type {
                LatticeType::Concrete(ConcreteType::Array { element, .. }) => {
                    Some(element.as_ref().clone())
                }
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;

        if let Expr::FunctionRef { name, .. } | Expr::Var(name, _) = func_arg {
            let normalized_name = name.strip_prefix("function ").unwrap_or(name);
            if let Some(element) = nary_numeric_map_concrete_return_type(normalized_name, &elements)
            {
                return Some(LatticeType::Concrete(ConcreteType::Array {
                    element: Box::new(element),
                    ndims: None,
                }));
            }
        }

        None
    }

    fn infer_ntuple_return_type(
        &mut self,
        callable_ty: &LatticeType,
        n_ty: &LatticeType,
        env: &TypeEnv,
    ) -> Option<LatticeType> {
        let n = match n_ty {
            LatticeType::Const(ConstValue::Int64(n)) if *n >= 0 => *n as usize,
            _ => return None,
        };

        let mut elements = Vec::with_capacity(n);
        for idx in 1..=n {
            let arg = LatticeType::Const(ConstValue::Int64(idx as i64));
            let return_ty =
                self.infer_callable_lattice_call(callable_ty, std::slice::from_ref(&arg), env)?;
            elements.push(concrete_from_lattice(&return_ty)?);
        }

        Some(LatticeType::Concrete(ConcreteType::Tuple { elements }))
    }

    fn infer_filter_return_type(&self, array_type: &LatticeType) -> Option<LatticeType> {
        match array_type {
            LatticeType::Concrete(ConcreteType::Array { element, .. }) => {
                Some(LatticeType::Concrete(ConcreteType::Array {
                    element: element.clone(),
                    ndims: None,
                }))
            }
            // `filter(f, d::Dict{K,V})` → `Dict{K,V}` and `filter(f, s::Set{T})` →
            // `Set{T}`: filtering only drops entries, so the container type is
            // preserved (matching upstream `base/dict.jl` / `base/set.jl`).
            // Resolving these here keeps the result deterministic. Returning `None`
            // previously deferred to the interprocedural analysis of `filter`'s body
            // (`result = copy(h); filter!(f, result); result`), whose depth-limited
            // estimate varies with cache/dispatch order and intermittently widened
            // to `Top`/`Any`. That made the inferred type of a `filter` result
            // disagree with the freshly-built dict it came from, demoting
            // `empty!(filtered)` to a legacy `DictEmpty` boundary instead of native
            // struct-backed dispatch and failing the Issue #6621 guard
            // non-deterministically (Issue #6672).
            LatticeType::Concrete(ConcreteType::Dict { .. } | ConcreteType::Set { .. }) => {
                Some(array_type.clone())
            }
            _ => None,
        }
    }

    fn infer_reduce_return_type(
        &mut self,
        op_arg: &Expr,
        array_type: &LatticeType,
        env: &TypeEnv,
    ) -> Option<LatticeType> {
        let element_type = self.iterator_element_lattice_type(array_type)?;
        self.infer_reduce_operator_return_type(op_arg, element_type, env)
    }

    fn infer_mapreduce_return_type(
        &mut self,
        func_arg: &Expr,
        op_arg: &Expr,
        array_type: &LatticeType,
        env: &TypeEnv,
    ) -> Option<LatticeType> {
        let element_type = self.iterator_element_lattice_type(array_type)?;
        let mapped_type = self.infer_mapped_element_return_type(func_arg, element_type, env)?;
        self.infer_reduce_operator_return_type(op_arg, mapped_type, env)
    }

    fn infer_mapped_element_return_type(
        &mut self,
        func_arg: &Expr,
        element_type: ConcreteType,
        env: &TypeEnv,
    ) -> Option<ConcreteType> {
        if let Expr::FunctionRef { name, .. } | Expr::Var(name, _) = func_arg {
            if matches!(name.strip_prefix("function ").unwrap_or(name), "identity") {
                return Some(element_type);
            }
        }

        let array_type = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(element_type),
            ndims: None,
        });
        let mapped_array = self.infer_map_return_type(func_arg, &array_type, env)?;
        match mapped_array {
            LatticeType::Concrete(ConcreteType::Array { element, .. }) => Some(*element),
            _ => None,
        }
    }

    fn infer_reduce_operator_return_type(
        &mut self,
        op_arg: &Expr,
        element_type: ConcreteType,
        env: &TypeEnv,
    ) -> Option<LatticeType> {
        if let Expr::FunctionRef { name, .. } | Expr::Var(name, _) = op_arg {
            let op_name = name.strip_prefix("function ").unwrap_or(name);
            if let Some(return_type) =
                named_reduce_operator_concrete_return_type(op_name, &element_type)
            {
                return Some(LatticeType::Concrete(return_type));
            }
        }

        let (func_name, func) = self.map_callable_function(op_arg)?;
        let return_type = self.infer_callable_return_type_with_concrete_args(
            &func_name,
            &func,
            &[element_type.clone(), element_type],
            env,
        )?;
        Some(LatticeType::Concrete(return_type))
    }

    fn map_callable_function(&self, func_arg: &Expr) -> Option<(String, Function)> {
        match func_arg {
            Expr::FunctionRef { name, .. } | Expr::Var(name, _) => {
                let resolved = self.resolve_callable_name(name)?;
                let func = self.function_table.get(&resolved)?.clone();
                Some((resolved, func))
            }
            Expr::LetBlock { body, .. } => {
                let name = body.stmts.iter().rev().find_map(|stmt| match stmt {
                    Stmt::Expr {
                        expr: Expr::FunctionRef { name, .. },
                        ..
                    }
                    | Stmt::Expr {
                        expr: Expr::Var(name, _),
                        ..
                    } => Some(name.as_str()),
                    _ => None,
                })?;

                if let Some(resolved) = self.resolve_callable_name(name) {
                    let func = self.function_table.get(&resolved)?.clone();
                    return Some((resolved, func));
                }

                body.stmts.iter().find_map(|stmt| match stmt {
                    Stmt::FunctionDef { func, .. } if func.name == name => {
                        Some((func.name.clone(), (*func.clone()).clone()))
                    }
                    _ => None,
                })
            }
            _ => None,
        }
    }

    fn resolve_callable_name(&self, name: &str) -> Option<String> {
        if let Some(active_function) = &self.active_function {
            let nested_name = format!("{active_function}#{name}");
            if self.function_table.contains_key(&nested_name) {
                return Some(nested_name);
            }
        }

        if self.function_table.contains_key(name) {
            return Some(name.to_string());
        }

        None
    }

    fn infer_local_callable_call_return_type(
        &mut self,
        name: &str,
        arg_types: &[LatticeType],
        env: &TypeEnv,
    ) -> Option<LatticeType> {
        if let Some(bound) = env.get(name) {
            if let Some(return_type) = self.infer_callable_lattice_call(bound, arg_types, env) {
                return Some(return_type);
            }
        }

        let resolved_from_env = self.function_name_from_lattice(env.get(name));
        let resolved = resolved_from_env
            .clone()
            .or_else(|| self.resolve_callable_name(name))?;
        if resolved_from_env.is_none() && resolved == name {
            return None;
        }
        self.infer_named_callable_call(&resolved, arg_types, env)
    }

    fn infer_named_callable_call(
        &mut self,
        resolved: &str,
        arg_types: &[LatticeType],
        env: &TypeEnv,
    ) -> Option<LatticeType> {
        self.infer_named_callable_call_with_base_env(resolved, arg_types, env)
    }

    fn infer_named_callable_call_with_base_env(
        &mut self,
        resolved: &str,
        arg_types: &[LatticeType],
        base_env: &TypeEnv,
    ) -> Option<LatticeType> {
        if self.analysis_depth >= MAX_INTERPROCEDURAL_ANALYSIS_DEPTH {
            return None;
        }

        let func = self.function_table.get(resolved)?.clone();
        let bindings = {
            let engine = &*self;
            bind_call_args_to_params(&func.params, arg_types, |ty| {
                engine.julia_type_to_lattice(ty)
            })
        };

        let mut call_env = base_env.clone();
        for (param_name, ty) in bindings {
            call_env.set(&param_name, ty);
        }

        self.analysis_depth += 1;
        let dependency_key = inference_cache_function_id(&func);
        let previous_active = self.replace_active_context(func.name.clone(), dependency_key);
        let return_type = self.infer_block_with_fixpoint(&func.body, &mut call_env);
        self.restore_active_context(previous_active);
        self.analysis_depth -= 1;

        Some(return_type)
    }

    fn infer_callable_lattice_call(
        &mut self,
        callable: &LatticeType,
        arg_types: &[LatticeType],
        env: &TypeEnv,
    ) -> Option<LatticeType> {
        match callable {
            LatticeType::Concrete(concrete) => {
                self.infer_callable_concrete_call(concrete, arg_types, env)
            }
            _ => None,
        }
    }

    fn infer_callable_concrete_call(
        &mut self,
        callable: &ConcreteType,
        arg_types: &[LatticeType],
        env: &TypeEnv,
    ) -> Option<LatticeType> {
        match callable {
            ConcreteType::Function { name } => self.infer_named_callable_call(name, arg_types, env),
            ConcreteType::Closure { name, captures } => {
                let mut capture_env = env.clone();
                for (capture_name, capture_type) in captures {
                    capture_env.set(capture_name, LatticeType::Concrete(capture_type.clone()));
                }
                let inferred =
                    self.infer_named_callable_call_with_base_env(name, arg_types, &capture_env)?;
                if matches!(
                    inferred,
                    LatticeType::Concrete(ConcreteType::Core(CoreType::Any)) | LatticeType::Top
                ) {
                    return self
                        .infer_named_callable_call_with_base_env(name, arg_types, env)
                        .or(Some(inferred));
                }
                Some(inferred)
            }
            ConcreteType::ComposedFunction { outer, inner } => {
                let inner_return = self.infer_callable_concrete_call(inner, arg_types, env)?;
                self.infer_callable_concrete_call(outer, &[inner_return], env)
            }
            _ => None,
        }
    }

    fn function_name_from_lattice(&self, ty: Option<&LatticeType>) -> Option<String> {
        match ty {
            Some(LatticeType::Concrete(ConcreteType::Function { name })) => Some(name.clone()),
            _ => None,
        }
    }

    fn infer_unary_callable_return_type(
        &mut self,
        func_name: &str,
        func: &Function,
        element_type: ConcreteType,
        env: &TypeEnv,
    ) -> Option<ConcreteType> {
        let normalized_func_name = func_name.strip_prefix("function ").unwrap_or(func_name);
        if matches!(
            normalized_func_name,
            "iszero" | "isone" | "signbit" | "iseven" | "isodd"
        ) {
            return Some(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)));
        }

        let element_lattice = LatticeType::Concrete(element_type);
        let cache_key = InferenceCacheKey::new(func_name, std::slice::from_ref(&element_lattice));
        let cacheable = !func_name.contains('#');

        if self.analyzing_functions.contains_key(&cache_key)
            || self.analysis_depth >= MAX_INTERPROCEDURAL_ANALYSIS_DEPTH
        {
            if self.analyzing_functions.contains_key(&cache_key) && InferenceTracer::is_enabled() {
                record_event(TraceEvent::RecursiveCycle {
                    functions: vec![func_name.to_string()],
                });
            }
            return None;
        }

        if cacheable {
            // World-gated read (Issue #4271).
            if let Some(cached) = self.lookup_return_cache(&cache_key) {
                return concrete_from_lattice(cached);
            }
            if let Some(tentative) = self.lookup_tentative_result(&cache_key) {
                return concrete_from_lattice(tentative);
            }
        }

        self.analyzing_functions
            .insert(cache_key.clone(), LatticeType::Bottom);
        self.analysis_depth += 1;

        let mut call_env = env.clone();
        if let Some(param) = func.params.first() {
            call_env.set(&param.name, element_lattice);
        }

        // Attribute interprocedural callee edges seen in this body to
        // `func_name` so its cached entry records them (Issue #4271).
        let previous_active =
            self.replace_active_context(func_name.to_string(), cache_key.fn_id.clone());
        let return_type = self.infer_block_with_fixpoint(&func.body, &mut call_env);
        self.restore_active_context(previous_active);

        self.analysis_depth -= 1;
        self.analyzing_functions.remove(&cache_key);

        if cacheable {
            if self.analyzing_functions.is_empty() {
                self.insert_return_cache(cache_key, return_type.clone());
                for (k, v) in self.tentative_results.drain().collect::<Vec<_>>() {
                    if v.valid_worlds.contains(self.method_world) {
                        self.insert_return_cache_if_absent(k, v.ty);
                    }
                }
            } else {
                self.insert_tentative_result(cache_key, return_type.clone());
            }
        }

        concrete_from_lattice(&return_type)
    }

    fn infer_callable_return_type_with_concrete_args(
        &mut self,
        func_name: &str,
        func: &Function,
        arg_types: &[ConcreteType],
        env: &TypeEnv,
    ) -> Option<ConcreteType> {
        let arg_lattice_types: Vec<LatticeType> = arg_types
            .iter()
            .cloned()
            .map(LatticeType::Concrete)
            .collect();
        let cache_key = InferenceCacheKey::new(func_name, &arg_lattice_types);
        let cacheable = !func_name.contains('#');

        if self.analyzing_functions.contains_key(&cache_key)
            || self.analysis_depth >= MAX_INTERPROCEDURAL_ANALYSIS_DEPTH
        {
            if self.analyzing_functions.contains_key(&cache_key) && InferenceTracer::is_enabled() {
                record_event(TraceEvent::RecursiveCycle {
                    functions: vec![func_name.to_string()],
                });
            }
            return None;
        }

        if cacheable {
            if let Some(cached) = self.lookup_return_cache(&cache_key) {
                return concrete_from_lattice(cached);
            }
            if let Some(tentative) = self.lookup_tentative_result(&cache_key) {
                return concrete_from_lattice(tentative);
            }
        }

        self.analyzing_functions
            .insert(cache_key.clone(), LatticeType::Bottom);
        self.analysis_depth += 1;

        let mut call_env = env.clone();
        for (param, arg_type) in func.params.iter().zip(arg_lattice_types.iter()) {
            call_env.set(&param.name, arg_type.clone());
        }

        let previous_active =
            self.replace_active_context(func_name.to_string(), cache_key.fn_id.clone());
        let return_type = self.infer_block_with_fixpoint(&func.body, &mut call_env);
        self.restore_active_context(previous_active);

        self.analysis_depth -= 1;
        self.analyzing_functions.remove(&cache_key);

        if cacheable {
            if self.analyzing_functions.is_empty() {
                self.insert_return_cache(cache_key, return_type.clone());
                for (k, v) in self.tentative_results.drain().collect::<Vec<_>>() {
                    if v.valid_worlds.contains(self.method_world) {
                        self.insert_return_cache_if_absent(k, v.ty);
                    }
                }
            } else {
                self.insert_tentative_result(cache_key, return_type.clone());
            }
        }

        concrete_from_lattice(&return_type)
    }

    fn iterator_element_lattice_type(&self, iter_ty: &LatticeType) -> Option<ConcreteType> {
        match iter_ty {
            LatticeType::Concrete(ConcreteType::Array { element, .. })
            | LatticeType::Concrete(ConcreteType::Range { element })
            | LatticeType::Concrete(ConcreteType::Generator { element }) => {
                Some(element.as_ref().clone())
            }
            LatticeType::Concrete(ConcreteType::Tuple { elements }) => {
                let first = elements.first()?.clone();
                if elements.iter().all(|element| element == &first) {
                    Some(first)
                } else {
                    Some(ConcreteType::Core(CoreType::Any))
                }
            }
            _ => None,
        }
    }
}

fn is_unannotated_nothing_default_kwparam(kwparam: &KwParam) -> bool {
    kwparam.type_annotation.is_none()
        && !crate::compile::utils::is_required_kwarg(&kwparam.default)
        && (matches!(&kwparam.default, Expr::Literal(Literal::Nothing, _))
            || matches!(&kwparam.default, Expr::Var(name, _) if name == "nothing"))
}

fn concrete_from_lattice(ty: &LatticeType) -> Option<ConcreteType> {
    match ty {
        LatticeType::Concrete(ct) => Some(ct.clone()),
        LatticeType::Const(cv) => Some(cv.to_concrete_type()),
        LatticeType::Union(types) => Some(ConcreteType::UnionOf(types.iter().cloned().collect())),
        _ => None,
    }
}

fn single_or_tuple_map_return_type(return_elements: Vec<ConcreteType>) -> Option<LatticeType> {
    if return_elements.len() > 1 {
        return Some(LatticeType::Concrete(ConcreteType::Tuple {
            elements: return_elements,
        }));
    }

    return_elements.into_iter().next().map(|ct| {
        LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ct),
            ndims: None,
        })
    })
}

fn named_reduce_operator_concrete_return_type(
    op_name: &str,
    element_type: &ConcreteType,
) -> Option<ConcreteType> {
    match (op_name, element_type) {
        ("min" | "max", ty) => Some(ty.clone()),
        (
            "+" | "*" | "-",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
        (
            "+" | "*" | "-",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt64,
        ))),
        (
            "+" | "*" | "-",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float16))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
        (
            "/",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        )
        | (
            "/",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)),
        )
        | ("/", ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))) => Some(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ),
        (
            "/",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float16))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
        ("&" | "|" | "xor", ty) => Some(ty.clone()),
        _ => None,
    }
}

fn binary_numeric_map_concrete_return_type(
    func_name: &str,
    left_element: &ConcreteType,
    right_element: &ConcreteType,
) -> Option<ConcreteType> {
    match (func_name, left_element, right_element) {
        (
            "+" | "-" | "*",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
        (
            "+" | "-" | "*",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int32,
        ))),
        (
            "+" | "-" | "*",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
        (
            "+" | "-" | "*",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float32,
        ))),
        (
            "+",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
        (
            "*",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        (
            "/",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32))
            | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
        (
            "/",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float32,
        ))),
        (
            "/",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
        (
            "min" | "max",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
        (
            "min" | "max",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int32,
        ))),
        (
            "min" | "max",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
        (
            "min" | "max",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float32,
        ))),
        (
            "min" | "max",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
        ) => Some(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        _ => None,
    }
}

fn nary_numeric_map_concrete_return_type(
    func_name: &str,
    element_types: &[ConcreteType],
) -> Option<ConcreteType> {
    if !matches!(func_name, "+" | "*" | "min" | "max") || element_types.len() < 3 {
        return None;
    }
    let first = element_types.first()?;
    if !element_types.iter().all(|ty| ty == first) {
        return None;
    }

    match (func_name, first) {
        ("+" | "*", ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))) => Some(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        ),
        ("+" | "*", ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32))) => Some(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
        ),
        ("+" | "*", ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))) => Some(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ),
        ("+" | "*", ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32))) => Some(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
        ),
        ("+", ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))) => Some(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        ),
        ("*", ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))) => {
            Some(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        }
        ("min" | "max", ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))) => Some(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        ),
        ("min" | "max", ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32))) => Some(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
        ),
        ("min" | "max", ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))) => Some(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ),
        ("min" | "max", ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32))) => Some(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
        ),
        ("min" | "max", ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))) => {
            Some(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        }
        _ => None,
    }
}

fn function_lattice_type(name: String) -> LatticeType {
    LatticeType::Concrete(ConcreteType::Function { name })
}

fn closure_lattice_type(name: String, env: &TypeEnv) -> LatticeType {
    let mut captures: Vec<(String, ConcreteType)> = env
        .bindings()
        .filter_map(|(capture_name, capture_type)| {
            let ty = concrete_from_lattice(capture_type)?;
            if matches!(
                ty,
                ConcreteType::Function { .. }
                    | ConcreteType::Closure { .. }
                    | ConcreteType::ComposedFunction { .. }
            ) {
                return None;
            }
            Some((capture_name.clone(), ty))
        })
        .collect();
    captures.sort_by(|(lhs, _), (rhs, _)| lhs.cmp(rhs));

    LatticeType::Concrete(ConcreteType::Closure { name, captures })
}

fn concrete_callable_from_lattice(ty: &LatticeType) -> Option<ConcreteType> {
    match ty {
        LatticeType::Concrete(
            concrete @ (ConcreteType::Function { .. }
            | ConcreteType::Closure { .. }
            | ConcreteType::ComposedFunction { .. }),
        ) => Some(concrete.clone()),
        _ => None,
    }
}

fn split_union_call_arg_types(
    function: &str,
    arg_types: &[LatticeType],
) -> Option<Vec<Vec<LatticeType>>> {
    let mut saw_union = false;
    let mut total_variants = 1usize;
    let mut variants: Vec<Vec<LatticeType>> = vec![Vec::with_capacity(arg_types.len())];

    for arg_type in arg_types {
        let alternatives: Vec<LatticeType> = match arg_type {
            LatticeType::Union(types) => {
                if types.is_empty() {
                    return None;
                }
                saw_union = true;
                total_variants = total_variants.checked_mul(types.len())?;
                if total_variants > MAX_METHOD_UNION_SPLIT_VARIANTS {
                    emit_union_split_bailout(
                        total_variants,
                        MAX_METHOD_UNION_SPLIT_VARIANTS,
                        format!("call to {}", function),
                    );
                    return None;
                }
                types.iter().cloned().map(LatticeType::Concrete).collect()
            }
            _ => vec![arg_type.clone()],
        };

        let mut next = Vec::with_capacity(variants.len() * alternatives.len());
        for prefix in variants {
            for alternative in &alternatives {
                let mut variant = prefix.clone();
                variant.push(alternative.clone());
                next.push(variant);
            }
        }
        variants = next;
    }

    saw_union.then_some(variants)
}

fn method_table_args_are_precise(arg_types: &[LatticeType]) -> bool {
    arg_types.iter().all(|ty| match ty {
        LatticeType::Bottom | LatticeType::Top => false,
        LatticeType::Concrete(ConcreteType::Core(CoreType::Any)) => false,
        LatticeType::Const(_) => true,
        LatticeType::Concrete(_) => true,
        LatticeType::Union(types) => {
            !types.is_empty() && !types.contains(&ConcreteType::Core(CoreType::Any))
        }
        LatticeType::Conditional { .. } => method_table_args_are_precise(&[ty.widen_conditional()]),
    })
}

fn expanded_function_param_types_for_arity(
    func: &Function,
    arg_len: usize,
) -> Option<Vec<JuliaType>> {
    if let Some(vararg_idx) = func.params.iter().position(|param| param.is_varargs) {
        if arg_len < vararg_idx {
            return None;
        }
        if let Some(fixed_count) = func
            .params
            .get(vararg_idx)
            .and_then(|param| param.vararg_count)
        {
            if arg_len != vararg_idx + fixed_count {
                return None;
            }
        }

        let vararg_ty = func
            .params
            .get(vararg_idx)
            .map(|param| param.effective_type())
            .unwrap_or(JuliaType::Any);
        let mut expanded: Vec<_> = func
            .params
            .iter()
            .take(vararg_idx)
            .map(|param| param.effective_type())
            .collect();
        for _ in vararg_idx..arg_len {
            expanded.push(vararg_ty.clone());
        }
        Some(expanded)
    } else {
        (func.params.len() == arg_len).then(|| {
            func.params
                .iter()
                .map(|param| param.effective_type())
                .collect()
        })
    }
}

fn cache_key_matches_mutated_signature(
    key: &InferenceCacheKey,
    mutated_fn: &str,
    mutated_sig: Option<&MethodSig>,
    mutated_table: Option<&MethodTable>,
) -> bool {
    if key.base_fn_id() != mutated_fn {
        return false;
    }

    let Some(mutated_sig) = mutated_sig else {
        return true;
    };
    let Some(mutated_table) = mutated_table else {
        return true;
    };

    let arg_types: Vec<_> = key
        .argtypes
        .iter()
        .map(|arg| lattice_type_to_julia(&arg.widened()))
        .collect();
    mutated_signature_is_dispatch_winner(mutated_sig, mutated_table, &arg_types)
}

fn record_invalidated_dependency_keys<T>(invalidated: &mut T, key: &InferenceCacheKey)
where
    T: Extend<String>,
{
    invalidated.extend([key.fn_id.clone(), key.base_fn_id().to_string()]);
}

fn cached_method_edge_matches_mutated_signature(
    edge: &DispatchedMethodEdge,
    mutated_fn: &str,
    mutated_sig: Option<&MethodSig>,
    mutated_table: Option<&MethodTable>,
) -> bool {
    if edge.callee != mutated_fn {
        return false;
    }

    let Some(mutated_sig) = mutated_sig else {
        return true;
    };
    let Some(mutated_table) = mutated_table else {
        return true;
    };

    mutated_signature_is_dispatch_winner(mutated_sig, mutated_table, &edge.arg_types)
}

fn mutated_signature_is_dispatch_winner(
    mutated_sig: &MethodSig,
    mutated_table: &MethodTable,
    arg_types: &[JuliaType],
) -> bool {
    match mutated_table.dispatch(arg_types) {
        Ok(selected) => method_signature_equivalent(selected, mutated_sig),
        Err(DispatchError::AmbiguousMethod { .. }) => {
            mutated_table.signature_matches_arg_types(mutated_sig, arg_types)
        }
        Err(_) => false,
    }
}

/// Whether two method signatures denote the same declared method for
/// inference-cache invalidation purposes.
///
/// Compares the canonical structured `core_signature` (Issue #6495, stage
/// 6c). `compute_core_signature` is a deterministic projection of
/// (`params`, `type_params`), so projection-equal signatures are always
/// `core_signature`-equal; the reverse can differ only for non-canonical
/// spellings the `CoreType::from` bridge normalizes (e.g. a `Struct("Int")`
/// word-alias bound vs `Int64`) — those compare *equivalent* on the core path,
/// which at worst invalidates an extra cache entry (conservative, never
/// stale). Base-corpus parity is pinned by
/// `compile::cache::tests::base_method_core_signature_equivalence_parity_issue_6495`.
/// The vararg markers are not part of either projection, so they stay
/// explicit (a fixed-arity and a vararg method can share projected params).
///
/// Stage 7c-i: the legacy `params`/`type_params` fallback arm for
/// pre-`core_signature` `Bottom` placeholders is retired — every production
/// `MethodSig` carries a refreshed structured signature (stage 7b), so a
/// `Bottom` placeholder (test-only) simply compares unequal to any refreshed
/// signature.
pub(crate) fn method_signature_equivalent(a: &MethodSig, b: &MethodSig) -> bool {
    a.vararg_param_index == b.vararg_param_index
        && a.vararg_fixed_count == b.vararg_fixed_count
        && a.core_signature == b.core_signature
}

fn inference_cache_function_id(func: &Function) -> String {
    MethodInstanceKey::from_function(func).legacy_fn_id()
}

fn infer_simple_symbol_quote(expr: &Expr) -> Option<&str> {
    let Expr::Builtin {
        name: BuiltinOp::SymbolNew,
        args,
        ..
    } = expr
    else {
        return None;
    };
    let [Expr::Literal(Literal::Str(symbol), _)] = args.as_slice() else {
        return None;
    };
    Some(symbol)
}

fn lattice_argtypes_to_julia(arg_types: &[LatticeType]) -> Vec<JuliaType> {
    arg_types.iter().map(lattice_type_to_julia).collect()
}

fn lattice_type_to_julia(ty: &LatticeType) -> JuliaType {
    crate::compile::bridge::lattice_to_julia_type(ty)
}

fn pair_type_name(key: &ConcreteType, value: &ConcreteType) -> String {
    format!(
        "Pair{{{},{}}}",
        concrete_type_parameter_name(key),
        concrete_type_parameter_name(value)
    )
}

fn concrete_type_parameter_name(ty: &ConcreteType) -> String {
    crate::compile::bridge::lattice_to_julia_type(&LatticeType::Concrete(ty.clone()))
        .name()
        .to_string()
}

impl Default for InferenceEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Bind call-site argument lattice types to a function's parameters,
/// packing remaining arguments into a `Tuple` for varargs parameters
/// (Issue #3526). Missing positional arguments fall back to the parameter
/// annotation (mapped via `annotation_resolver`) or `Top`.
///
/// Returns a vector of `(parameter_name, bound_lattice_type)` pairs in
/// declaration order so that callers can write directly into a `TypeEnv`.
fn bind_call_args_to_params<F>(
    params: &[crate::ir::core::TypedParam],
    arg_types: &[LatticeType],
    mut annotation_resolver: F,
) -> Vec<(String, LatticeType)>
where
    F: FnMut(&JuliaType) -> LatticeType,
{
    let num_args = arg_types.len();
    let mut bindings = Vec::with_capacity(params.len());
    for (idx, param) in params.iter().enumerate() {
        let bound = if param.is_varargs {
            if idx < num_args {
                let remaining = &arg_types[idx..];
                let elements: Vec<_> = remaining
                    .iter()
                    .filter_map(|t| match t {
                        LatticeType::Concrete(ct) => Some(ct.clone()),
                        LatticeType::Const(cv) => Some(cv.to_concrete_type()),
                        _ => None,
                    })
                    .collect();
                if elements.len() == remaining.len() {
                    // Issue #3511: keep short tails as a precise flat tuple,
                    // but normalize long homogeneous (or mostly-homogeneous)
                    // tails to `Tuple{T, Vararg{Tail}}` so the inference cache
                    // key and bound parameter type stay bounded in size.
                    LatticeType::Concrete(ConcreteType::normalize_tuple_vararg(elements))
                } else {
                    // Fall back to an empty tuple shape if any arg is non-concrete
                    LatticeType::Concrete(ConcreteType::Tuple { elements: vec![] })
                }
            } else {
                LatticeType::Concrete(ConcreteType::Tuple { elements: vec![] })
            }
        } else if let Some(arg_ty) = arg_types.get(idx) {
            arg_ty.clone()
        } else if let Some(ann) = &param.type_annotation {
            annotation_resolver(ann)
        } else {
            LatticeType::Top
        };
        bindings.push((param.name.clone(), bound));
    }
    bindings
}

/// Converts a binary operator to its function name.
///
/// Issue #3524: Map all supported BinaryOp variants. Operators without a
/// registered tfunc are still routed to the right name; the engine handles
/// boolean-result operators (`===`, `!==`, `<:`, `&&`, `||`, `!=`) directly
/// in `infer_expr` so they never fall through to the unknown-name fallback.
fn binary_op_to_function(op: &BinaryOp) -> String {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::IntDiv => "div",
        BinaryOp::Mod => "mod",
        BinaryOp::Pow => "^",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::Egal => "===",
        BinaryOp::NotEgal => "!==",
        BinaryOp::Subtype => "<:",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
    }
    .to_string()
}

/// Returns true if `op` always produces a `Bool` regardless of operand types.
///
/// Issue #3524: comparison/identity/subtype/logical operators yield Bool, so
/// inference can short-circuit even without a registered tfunc.
fn binary_op_always_bool(op: &BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Egal
            | BinaryOp::NotEgal
            | BinaryOp::Subtype
            | BinaryOp::And
            | BinaryOp::Or,
    )
}

/// Converts a unary operator to its function name.
fn unary_op_to_function(op: &crate::ir::core::UnaryOp) -> String {
    match op {
        crate::ir::core::UnaryOp::Neg => "-",
        crate::ir::core::UnaryOp::Not => "!",
        _ => "unknown_unop",
    }
    .to_string()
}

/// Converts a builtin operation to its function name.
///
/// Issue #3525: Map every BuiltinOp to the function name used by the
/// transfer-function registry so that inference does not collapse to
/// `unknown_builtin` and silently widen to `Top`.
fn builtin_op_to_function(op: &BuiltinOp) -> String {
    match op {
        // Math / RNG / time
        BuiltinOp::Rand => "rand",
        BuiltinOp::Sqrt => "sqrt",
        BuiltinOp::IfElse => "ifelse",
        BuiltinOp::TimeNs => "time_ns",
        BuiltinOp::Randn => "randn",
        BuiltinOp::Seed => "seed!",
        BuiltinOp::StableRNG => "StableRNG",
        BuiltinOp::XoshiroRNG => "Xoshiro",
        BuiltinOp::MersenneTwisterRNG => "MersenneTwister",

        // Array operations
        BuiltinOp::Zeros => "zeros",
        BuiltinOp::Ones => "ones",
        BuiltinOp::Reshape => "reshape",
        BuiltinOp::Length => "length",
        BuiltinOp::Size => "size",
        BuiltinOp::Ndims => "ndims",
        BuiltinOp::Push => "push!",
        BuiltinOp::Pop => "pop!",
        BuiltinOp::PushFirst => "pushfirst!",
        BuiltinOp::PopFirst => "popfirst!",
        BuiltinOp::Insert => "insert!",
        BuiltinOp::DeleteAt => "deleteat!",
        BuiltinOp::Zero => "zero",

        // Linear algebra
        BuiltinOp::Lu => "lu",
        BuiltinOp::Det => "det",

        // Tuple operations
        BuiltinOp::TupleFirst => "first",
        BuiltinOp::TupleLast => "last",

        // Dict operations
        BuiltinOp::HasKey => "haskey",
        BuiltinOp::DictGet => "get",
        BuiltinOp::DictDelete => "delete!",
        BuiltinOp::DictKeys => "keys",
        BuiltinOp::DictValues => "values",
        BuiltinOp::DictPairs => "pairs",
        BuiltinOp::DictMerge => "merge",
        BuiltinOp::DictGetBang => "get!",
        BuiltinOp::DictMergeBang => "merge!",
        BuiltinOp::DictEmpty => "empty!",
        BuiltinOp::DictGetkey => "getkey",

        // Broadcasting
        BuiltinOp::Ref => "Ref",

        // Type operations
        BuiltinOp::TypeOf => "typeof",
        BuiltinOp::Isa => "isa",
        BuiltinOp::Eltype => "eltype",
        BuiltinOp::Keytype => "keytype",
        BuiltinOp::Valtype => "valtype",
        BuiltinOp::Sizeof => "sizeof",
        // BuiltinOp::Isbits removed - pure Julia (Issue #6738)
        BuiltinOp::Isbitstype => "isbitstype",
        BuiltinOp::Supertype => "_supertype",
        BuiltinOp::Typename => "_typename",
        BuiltinOp::FunctionName => "_function_name",
        BuiltinOp::Subtypes => "subtypes",
        // BuiltinOp::Hasfield removed - pure Julia (Issue #6738)
        // BuiltinOp::Ismutable removed - pure Julia (Issue #6738)
        BuiltinOp::Objectid => "objectid",
        BuiltinOp::Isunordered => "isunordered",

        // Reflection
        BuiltinOp::Methods => "_methods_by_ftype",
        BuiltinOp::HasMethod => "hasmethod",

        // Set operations
        BuiltinOp::In => "in",

        // Iterator protocol
        BuiltinOp::Iterate => "iterate",
        BuiltinOp::Collect => "collect",
        BuiltinOp::Generator => "Generator",

        // Metaprogramming
        BuiltinOp::SymbolNew => "Symbol",
        BuiltinOp::ExprNew => "Expr",
        BuiltinOp::LineNumberNodeNew => "LineNumberNode",
        BuiltinOp::QuoteNodeNew => "QuoteNode",
        BuiltinOp::GlobalRefNew => "GlobalRef",
        BuiltinOp::Gensym => "gensym",
        BuiltinOp::Esc => "esc",
        BuiltinOp::Eval => "eval",
        BuiltinOp::GeneratedEval => "_generated_eval",
        BuiltinOp::MacroExpand => "macroexpand",
        BuiltinOp::MacroExpandBang => "macroexpand!",
        BuiltinOp::IncludeString => "include_string",
        BuiltinOp::EvalFile => "evalfile",
        BuiltinOp::SplatInterpolation => "splat_interpolation",

        // Test operations
        BuiltinOp::TestRecord => "_test_record!",
        BuiltinOp::TestRecordBroken => "_test_record_broken!",
        BuiltinOp::TestSetBegin => "_testset_begin!",
        BuiltinOp::TestSetEnd => "_testset_end!",

        // Variable reflection
        BuiltinOp::IsDefined => "isdefined",
    }
    .to_string()
}

/// Returns true if the given builtin op produces a result whose type we
/// can confidently widen back to `Top` instead of using the registry. This
/// list is used to retain pre-#3525 semantics for collection-creation
/// builtins (zeros, ones, push!, pop!, insert!, etc.) when the registry's
/// transfer function is too aggressive (e.g. infers Array{Float64} for
/// `zeros(n)` instead of Array{?}). Issue #3525.
fn builtin_op_should_widen_unknown(op: &BuiltinOp) -> bool {
    matches!(
        op,
        BuiltinOp::Zeros
            | BuiltinOp::Ones
            | BuiltinOp::Reshape
            | BuiltinOp::Push
            | BuiltinOp::Pop
            | BuiltinOp::PushFirst
            | BuiltinOp::PopFirst
            | BuiltinOp::Insert
            | BuiltinOp::DeleteAt
            | BuiltinOp::Lu
            | BuiltinOp::Det
            | BuiltinOp::Size
            | BuiltinOp::Rand
            | BuiltinOp::Randn
            | BuiltinOp::IfElse
    )
}

fn is_real_lattice(ty: &LatticeType) -> bool {
    matches!(
        ty,
        LatticeType::Concrete(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8))
                | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16))
                | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32))
                | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
                | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128))
                | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8))
                | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16))
                | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32))
                | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64))
                | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt128))
                | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float16))
                | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32))
                | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))
                | ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt))
                | ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat))
                | ConcreteType::Core(CoreType::Abstract(CoreAbstract::Integer))
                | ConcreteType::Core(CoreType::Abstract(CoreAbstract::AbstractFloat))
                | ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number))
        )
    )
}

/// Immediate sub-expressions of `expr` that are evaluated when `expr` is
/// evaluated, for the interprocedural exception walk (Issue #5600). Leaf nodes
/// and type-level operands return an empty list.
fn expr_subexpressions(expr: &Expr) -> Vec<&Expr> {
    let mut out: Vec<&Expr> = Vec::new();
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            out.push(left);
            out.push(right);
        }
        Expr::UnaryOp { operand, .. } => out.push(operand),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            out.extend(args.iter());
            out.extend(kwargs.iter().map(|(_, v)| v));
        }
        Expr::Builtin { args, .. }
        | Expr::ArrayLiteral { elements: args, .. }
        | Expr::TupleLiteral { elements: args, .. }
        | Expr::StringConcat { parts: args, .. }
        | Expr::New { args, .. } => out.extend(args.iter()),
        Expr::Index { array, indices, .. } => {
            out.push(array);
            out.extend(indices.iter());
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            out.push(start);
            if let Some(s) = step {
                out.push(s);
            }
            out.push(stop);
        }
        Expr::FieldAccess { object, .. } => out.push(object),
        Expr::Pair { key, value, .. } => {
            out.push(key);
            out.push(value);
        }
        Expr::NamedTupleLiteral { fields, .. } => out.extend(fields.iter().map(|(_, v)| v)),
        Expr::DictLiteral { pairs, .. } => {
            for (k, v) in pairs {
                out.push(k);
                out.push(v);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            out.push(condition);
            out.push(then_expr);
            out.push(else_expr);
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            out.push(body);
            out.push(iter);
            if let Some(f) = filter {
                out.push(f);
            }
        }
        _ => {}
    }
    out
}

/// The bare callee name used for Base-callee exception classification: strips a
/// dotted module prefix (e.g. `Base.gcd` → `gcd`) so the lookup matches the
/// function table's bare names (Issue #6272).
fn base_callee_name(function: &str) -> &str {
    function.rsplit_once('.').map_or(function, |(_, name)| name)
}

fn exception_type_for_expr(
    expr: &Expr,
    arg_context: Option<&[(LatticeType, Effects)]>,
    effects: &mut Effects,
) -> ExceptionType {
    if let Some(exct) = typed_exception_type_for_expr(expr, arg_context, effects) {
        return exct;
    }

    if effects.nothrow {
        return ExceptionType::Bottom;
    }

    match expr {
        Expr::Call { function, .. }
            if matches!(function.as_str(), "sqrt" | "log" | "log10" | "log2") =>
        {
            ExceptionType::Known("DomainError")
        }
        // `gcd`/`lcm` are pure-Julia Base callees: their exception types are
        // owned by the pure-Julia reflection classification and composed by the
        // interprocedural walker, not classified by name here (Issue #6272).
        Expr::Call { function, .. } if function == "getindex" => {
            ExceptionType::Known("BoundsError")
        }
        Expr::BinaryOp {
            op: BinaryOp::IntDiv | BinaryOp::Mod,
            ..
        } => ExceptionType::Known("DivideError"),
        Expr::Index { .. } => ExceptionType::Known("BoundsError"),
        _ => ExceptionType::Any,
    }
}

fn typed_exception_type_for_expr(
    expr: &Expr,
    arg_context: Option<&[(LatticeType, Effects)]>,
    effects: &mut Effects,
) -> Option<ExceptionType> {
    let arg_context = arg_context?;
    let div_like_call = matches!(expr, Expr::Call { function, .. } if matches!(function.as_str(), "div" | "rem" | "mod" | "%"));
    let div_like_op = matches!(
        expr,
        Expr::BinaryOp {
            op: BinaryOp::IntDiv | BinaryOp::Mod,
            ..
        }
    );
    if div_like_call || div_like_op {
        let all_integer = arg_context.iter().all(|(ty, _)| ty.is_integer());
        let any_float = arg_context.iter().any(|(ty, _)| ty.is_float());
        let all_float_or_integer = arg_context
            .iter()
            .all(|(ty, _)| ty.is_float() || ty.is_integer());

        if all_integer {
            *effects = merge_arg_effects_with_base(Effects::effect_free_may_throw(), arg_context);
            return Some(ExceptionType::Known("DivideError"));
        }

        if all_float_or_integer && any_float {
            *effects = merge_arg_effects_with_base(Effects::pure_arithmetic(), arg_context);
            return Some(if effects.nothrow {
                ExceptionType::Bottom
            } else {
                ExceptionType::Any
            });
        }

        return None;
    }

    // Issue #4274: const-value refinement for DomainError-bearing math
    // calls. When the argument is a known non-negative constant, the call
    // cannot throw DomainError and is therefore nothrow. Falls back to the
    // conservative DomainError path in `exception_type_for_expr` when the
    // constant is negative or the argument is non-constant.
    //
    // Issue #4700 follow-up: when negative, refine the exception type
    // family — `log` / `log10` / `log2` widen to
    // `Union{DomainError, InexactError}` (matching upstream
    // `Base.infer_exception_type(() -> log(-1))`), `sqrt` stays at
    // the single `DomainError`. The Union variant landed in PR #4838
    // (same Issue #4700).
    let log_family_call = matches!(expr, Expr::Call { function, args, .. } if args.len() == 1
        && matches!(function.as_str(), "log" | "log10" | "log2"));
    let sqrt_call = matches!(expr, Expr::Call { function, args, .. } if args.len() == 1
        && function.as_str() == "sqrt");
    if log_family_call || sqrt_call {
        if let Some((arg_ty, _)) = arg_context.first() {
            if let Some(nonneg) = constant_is_non_negative(arg_ty) {
                if nonneg {
                    *effects = merge_arg_effects_with_base(Effects::pure_arithmetic(), arg_context);
                    return Some(ExceptionType::Bottom);
                }
                // Known-negative: refine the exception type family
                // (Issue #4700 follow-up).
                *effects =
                    merge_arg_effects_with_base(Effects::effect_free_may_throw(), arg_context);
                if log_family_call {
                    let mut set = std::collections::BTreeSet::new();
                    set.insert("DomainError");
                    set.insert("InexactError");
                    return Some(ExceptionType::Union(set));
                } else {
                    return Some(ExceptionType::Known("DomainError"));
                }
            }
        }
        return None;
    }

    None
}

fn constant_is_non_negative(ty: &LatticeType) -> Option<bool> {
    use crate::compile::lattice::types::ConstValue;
    match ty {
        LatticeType::Const(ConstValue::Int64(v)) => Some(*v >= 0),
        LatticeType::Const(ConstValue::Float64(v)) => Some(!v.is_nan() && *v >= 0.0),
        _ => None,
    }
}

fn merge_arg_effects_with_base(base: Effects, arg_context: &[(LatticeType, Effects)]) -> Effects {
    arg_context
        .iter()
        .fold(base, |acc, (_, arg_effects)| acc.merge(arg_effects))
}

fn cfg_authoritative_straightline_stmt_supported(stmt: &Stmt) -> bool {
    matches!(
        stmt,
        Stmt::Assign { .. }
            | Stmt::FieldAssign { .. }
            | Stmt::IndexAssign { .. }
            | Stmt::DictAssign { .. }
            | Stmt::FunctionDef { .. }
            | Stmt::DestructuringAssign { .. }
            | Stmt::Return { .. }
            | Stmt::Expr { .. }
            | Stmt::Meta { .. }
            | Stmt::Using { .. }
            | Stmt::Export { .. }
            | Stmt::EnumDef { .. }
            | Stmt::Global { .. }
    )
}

fn cfg_authoritative_all_return_stmt_supported(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Assign { value, .. } => cfg_authoritative_payload_expr_supported(value),
        Stmt::Expr { expr, .. } => cfg_authoritative_payload_expr_supported(expr),
        Stmt::Return { value, .. } => value
            .as_ref()
            .is_none_or(cfg_authoritative_payload_expr_supported),
        Stmt::If { condition, .. } => cfg_authoritative_condition_expr_supported(condition),
        _ => false,
    }
}

fn cfg_authoritative_payload_expr_supported(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(_, _) | Expr::Var(_, _) => true,
        Expr::Call {
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } => {
            splat_mask.len() <= args.len()
                && kwargs_splat_mask.len() <= kwargs.len()
                && args.iter().all(cfg_authoritative_payload_expr_supported)
                && kwargs
                    .iter()
                    .all(|(_, value)| cfg_authoritative_payload_expr_supported(value))
        }
        Expr::BinaryOp {
            op, left, right, ..
        } if !matches!(op, BinaryOp::And | BinaryOp::Or) => {
            cfg_authoritative_payload_expr_supported(left)
                && cfg_authoritative_payload_expr_supported(right)
        }
        Expr::UnaryOp {
            op: UnaryOp::Neg | UnaryOp::Not,
            operand,
            ..
        } => cfg_authoritative_payload_expr_supported(operand),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            cfg_authoritative_condition_expr_supported(condition)
                && cfg_authoritative_payload_expr_supported(then_expr)
                && cfg_authoritative_payload_expr_supported(else_expr)
        }
        Expr::FieldAccess { object, .. } => cfg_authoritative_payload_expr_supported(object),
        Expr::Index { array, indices, .. } => {
            cfg_authoritative_payload_expr_supported(array)
                && indices.iter().all(cfg_authoritative_payload_expr_supported)
        }
        Expr::ArrayLiteral { elements, .. } => elements
            .iter()
            .all(cfg_authoritative_payload_expr_supported),
        Expr::TupleLiteral { elements, .. } => elements
            .iter()
            .all(cfg_authoritative_payload_expr_supported),
        Expr::NamedTupleLiteral { fields, .. } => fields
            .iter()
            .all(|(_, value)| cfg_authoritative_payload_expr_supported(value)),
        Expr::Pair { key, value, .. } => {
            cfg_authoritative_payload_expr_supported(key)
                && cfg_authoritative_payload_expr_supported(value)
        }
        Expr::DictLiteral { pairs, .. } => pairs.iter().all(|(key, value)| {
            cfg_authoritative_payload_expr_supported(key)
                && cfg_authoritative_payload_expr_supported(value)
        }),
        Expr::LetBlock { bindings, body, .. } => {
            bindings
                .iter()
                .all(|(_, value)| cfg_authoritative_payload_expr_supported(value))
                && body
                    .stmts
                    .iter()
                    .all(cfg_authoritative_letblock_stmt_supported)
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            cfg_authoritative_payload_expr_supported(start)
                && step
                    .as_deref()
                    .is_none_or(cfg_authoritative_payload_expr_supported)
                && cfg_authoritative_payload_expr_supported(stop)
        }
        Expr::SliceAll { .. } => true,
        _ => false,
    }
}

fn cfg_authoritative_letblock_stmt_supported(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Assign { value, .. } | Stmt::Expr { expr: value, .. } => {
            cfg_authoritative_payload_expr_supported(value)
        }
        Stmt::Return { value, .. } => value
            .as_ref()
            .is_none_or(cfg_authoritative_payload_expr_supported),
        _ => false,
    }
}

fn cfg_authoritative_condition_expr_supported(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(_, _) | Expr::Var(_, _) => true,
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if function == "isa" => {
            args.len() == 2
                && kwargs.is_empty()
                && splat_mask.iter().all(|is_splat| !is_splat)
                && kwargs_splat_mask.iter().all(|is_splat| !is_splat)
                && args.iter().all(cfg_authoritative_payload_expr_supported)
        }
        Expr::Builtin {
            name: BuiltinOp::Isa,
            args,
            ..
        } => args.iter().all(cfg_authoritative_payload_expr_supported),
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(
            op,
            BinaryOp::Egal | BinaryOp::Eq | BinaryOp::NotEgal | BinaryOp::Ne
        ) && (cfg_authoritative_typeof_value(left).is_some()
            || cfg_authoritative_typeof_value(right).is_some()) =>
        {
            if let Some(value) = cfg_authoritative_typeof_value(left) {
                cfg_authoritative_payload_expr_supported(value)
                    && cfg_authoritative_payload_expr_supported(right)
            } else if let Some(value) = cfg_authoritative_typeof_value(right) {
                cfg_authoritative_payload_expr_supported(left)
                    && cfg_authoritative_payload_expr_supported(value)
            } else {
                false
            }
        }
        Expr::BinaryOp {
            op: BinaryOp::Egal | BinaryOp::NotEgal,
            left,
            right,
            ..
        } => {
            (cfg_authoritative_payload_expr_supported(left)
                && cfg_authoritative_nothing_literal(right))
                || (cfg_authoritative_nothing_literal(left)
                    && cfg_authoritative_payload_expr_supported(right))
        }
        _ => false,
    }
}

fn call_cache_parts(
    function: &str,
    arg_types: &[LatticeType],
    explicit_kwarg_types: &[(String, LatticeType)],
) -> (String, Vec<LatticeType>) {
    if explicit_kwarg_types.is_empty() {
        return (function.to_string(), arg_types.to_vec());
    }

    let kw_names = explicit_kwarg_types
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let mut cache_arg_types = Vec::with_capacity(arg_types.len() + explicit_kwarg_types.len());
    cache_arg_types.extend_from_slice(arg_types);
    cache_arg_types.extend(explicit_kwarg_types.iter().map(|(_, ty)| ty.clone()));

    (format!("{function};kw={kw_names}"), cache_arg_types)
}

fn expand_static_tuple_splat_arg_types(
    raw_arg_types: &[LatticeType],
    splat_mask: &[bool],
) -> Option<Vec<LatticeType>> {
    let mut arg_types = Vec::with_capacity(raw_arg_types.len());
    for (idx, arg_ty) in raw_arg_types.iter().enumerate() {
        if !splat_mask.get(idx).copied().unwrap_or(false) {
            arg_types.push(arg_ty.clone());
            continue;
        }

        let LatticeType::Concrete(ConcreteType::Tuple { elements }) = arg_ty else {
            return None;
        };
        arg_types.extend(elements.iter().cloned().map(LatticeType::Concrete));
    }
    Some(arg_types)
}

fn expand_static_namedtuple_kwarg_types(
    raw_kwarg_types: &[(String, LatticeType, bool)],
) -> Option<Vec<(String, LatticeType)>> {
    let mut kwarg_types = Vec::with_capacity(raw_kwarg_types.len());
    for (name, kwarg_ty, is_splat) in raw_kwarg_types {
        if !*is_splat {
            kwarg_types.push((name.clone(), kwarg_ty.clone()));
            continue;
        }

        let LatticeType::Concrete(ConcreteType::NamedTuple { fields }) = kwarg_ty else {
            return None;
        };
        kwarg_types.extend(
            fields
                .iter()
                .cloned()
                .map(|(field, ty)| (field, LatticeType::Concrete(ty))),
        );
    }
    Some(kwarg_types)
}

fn cfg_authoritative_nothing_literal(expr: &Expr) -> bool {
    matches!(expr, Expr::Literal(Literal::Nothing, _))
}

fn cfg_authoritative_typeof_value(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Builtin {
            name: BuiltinOp::TypeOf,
            args,
            ..
        } if args.len() == 1 => Some(&args[0]),
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } if function == "typeof"
            && args.len() == 1
            && kwargs.is_empty()
            && splat_mask.iter().all(|is_splat| !is_splat)
            && kwargs_splat_mask.iter().all(|is_splat| !is_splat) =>
        {
            Some(&args[0])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
