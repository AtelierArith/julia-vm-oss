//! Shared method-dispatch matching helpers.
//!
//! This module is the migration point for dispatch semantics that used to live
//! separately in compiler method tables and VM runtime call instructions.

pub mod core_match;

use std::collections::{HashMap, HashSet};

use crate::runtime_types::LatticeType;
use crate::types::{JuliaType, StructHierarchy, TypeParam};
use subset_julia_vm_ir::Span;

use super::selection::unique_dominant_index;
use super::{
    core_type_var_to_type_param, specificity, CorePrimitive, CoreSubtypeEngine, CoreType,
    CoreTypeSubstitution, CoreTypeVar, CoreTypeVarId,
};

/// Source-level `where` bindings for one dispatch candidate match.
///
/// Names are lexical lookup keys only; bound values retain structured
/// [`CoreType`] identity and the map never crosses a candidate boundary.
type LexicalTypeBindings = HashMap<String, CoreType>;

/// Bonus for exact match between two concrete primitive-ish dispatch leaves.
///
/// This mirrors the compile-time MethodTable policy while giving the shared
/// resolver ownership of the scoring constants used during CoreType migration.
///
/// NOT retirable under the Issue #8438 ratchet: on precise call tuples the
/// dominance prechecks usually decide before the score, but the bonus still
/// ranks precise slots inside DeferImprecise/DeferSignature tuples and the
/// runtime/callable-value paths. Zeroing evidence (2026-07-02):
/// `score_julia_signature_prefers_exact_type_any_over_typevar_issue_4574` and
/// `score_julia_signature_exact_uppercase_struct_beats_any_issue_5314` both
/// regress (exact `Type{...}` / exact uppercase-struct params lose their
/// ranking), and `test_scoring_constants_invariant` pins the penalty pairing.
pub const EXACT_PRIMITIVE_MATCH_BONUS: i32 = 10;

/// Penalty when an argument is statically `Any` but the candidate parameter is
/// more specific. This keeps `f(::Any)` preferred for unknown values.
///
/// NOT retirable under the Issue #8438 ratchet: a statically-`Any` slot makes
/// the whole call tuple [`TypemapVerdict::DeferImprecise`], so this penalty's
/// entire firing domain is the deferred region the typemap verdict does not
/// own. Removal evidence (2026-07-02 full-fixture sweep): `sprint(show,
/// [1 => 2])` mis-dispatched a statically-imprecise `Pair`-element array into
/// a Complex show method ("type Array{Pair, 2} has no field re",
/// `array/show_eltype_prefix_5236_5237.jl`).
pub const ANY_ARG_SPECIFIC_PARAM_PENALTY: i32 = -EXACT_PRIMITIVE_MATCH_BONUS;

/// Bonus for a structured parametric pattern match, such as
/// `Matrix{<:Integer}` accepting `Matrix{Int64}`.
///
/// NOT retirable under the Issue #8438 ratchet: the shapes it discriminates
/// (bounded/variable parametric containers, `Type{T}` singletons) are
/// [`TypemapVerdict::DeferSignature`] shapes, and it also ranks the
/// callable-value and runtime type-object paths where the verdict only gates
/// `where` candidates. Removal evidence (2026-07-02):
/// `callable_value_candidates_enforce_where_bounds_issue_6539` (bounded
/// `Holder{T} where {T<:Real}` loses to the bare `Holder` sibling for
/// `Holder{Int64}`) and
/// `test_type_value_dispatch_does_not_match_value_level_parametric_patterns_issue_6251`
/// (`Type{Vector{T}}` loses the type-object ranking) both regress.
pub const PARAMETRIC_PATTERN_MATCH_BONUS: i32 = 3;

/// Runtime fallback score for declared subtype relations not represented by
/// the slot's structural matcher. This must outrank an untyped `Any` slot while
/// staying below exact and parametric structural matches.
const SUBTYPE_FALLBACK_MATCH_SCORE: u32 = 2;

/// Bonus for typed varargs that bind a method `where` variable.
///
/// Without this, a keyword-forwarding fallback such as `f(xs...; kws...)` ties a
/// diagonal typed vararg like `f(xs::T...; kw=nothing, kws...) where T` and wins
/// by insertion order, which recursively forwards QuadGK `segbuf` calls
/// (Issue #8407).
///
/// NOT consolidatable into the typemap verdict under the Issue #8438 ratchet:
/// the verdict decides candidacy only, while this bonus decides *ranking*
/// between a typed vararg and its untyped fallback — the dominance prechecks
/// skip vararg candidates (`compute_core_signature` renders `args...` as a
/// fixed slot) and the fewest-`where`-params tie-breaker prefers the wrong
/// (untyped) method. Zeroing evidence (2026-07-02):
/// `dispatch/typed_varargs_diagonal_8565.jl` drops to 3/9 (`v1(1, 2)` selects
/// the untyped fallback, upstream selects the diagonal method). Consolidation
/// needs an arity-expanded vararg dominance relation first.
pub(crate) const VARARG_TYPE_PARAM_BINDING_BONUS: u32 = 2;

/// Result of matching a Julia method signature projection against call-site
/// argument types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JuliaSignatureScore {
    pub binding_count: usize,
    pub fixed_param_count: usize,
    pub score: u32,
}

/// Convert a Julia type at a method-dispatch boundary.
///
/// The ordinary `CoreType::from(&JuliaType)` bridge intentionally keeps the
/// historical owner-erased image used by Julia type operators. Dispatch has a
/// stricter nominal identity requirement: explicit user/package owners must
/// survive so two sibling modules can declare unrelated same-named structs.
/// This conversion is recursive for structural container projections, while
/// runtime identity-bearing type variables keep their dedicated conversion.
pub fn dispatch_core_type_from_julia(ty: &JuliaType) -> CoreType {
    match ty {
        JuliaType::Struct(name) => CoreType::from_julia_name_for_dispatch(name),
        JuliaType::VectorOf(element) => CoreType::Struct {
            name: "Vector".to_string(),
            params: vec![dispatch_core_type_from_julia(element)],
        },
        JuliaType::MatrixOf(element) => CoreType::Struct {
            name: "Matrix".to_string(),
            params: vec![dispatch_core_type_from_julia(element)],
        },
        JuliaType::TupleOf(elements) => {
            CoreType::Tuple(elements.iter().map(dispatch_core_type_from_julia).collect())
        }
        JuliaType::Union(types) => {
            CoreType::Union(types.iter().map(dispatch_core_type_from_julia).collect())
        }
        JuliaType::TypeOf(inner) => {
            CoreType::TypeOf(Box::new(dispatch_core_type_from_julia(inner)))
        }
        JuliaType::RuntimeParametric { base, params } => match (base.as_str(), params.as_slice()) {
            ("Vararg", [element]) => {
                CoreType::Vararg(Box::new(dispatch_core_type_from_julia(element)))
            }
            ("Vararg", [element, len]) => CoreType::VarargLen {
                element: Box::new(dispatch_core_type_from_julia(element)),
                len: Box::new(dispatch_core_type_from_julia(len)),
            },
            _ => CoreType::Struct {
                name: base.clone(),
                params: params.iter().map(dispatch_core_type_from_julia).collect(),
            },
        },
        // Binder identity takes precedence over rendered-name parsing. In
        // particular, a legal `where {Float64}` binder must stay a TypeVar
        // instead of being reparsed as the builtin Float64 primitive (#10407).
        JuliaType::TypeVar(..)
        | JuliaType::RuntimeTypeVar { .. }
        | JuliaType::UnionAll { .. }
        | JuliaType::RuntimeUnionAll { .. } => CoreType::from(ty),
        _ => CoreType::from(ty),
    }
}

/// Runtime callable-value candidate metadata.
///
/// This mirrors the callable function-variable dispatch input while allowing
/// the score policy to live in the shared resolver during the #3910 migration.
#[derive(Debug, Clone, Copy)]
pub struct CallableValueCandidate<'a> {
    pub idx: usize,
    pub param_types: &'a [JuliaType],
    pub param_count: usize,
    pub vararg_param_index: Option<usize>,
    pub vararg_fixed_count: Option<usize>,
    /// `where` type parameters of the candidate method. Used to enforce the
    /// diagonal rule when a type variable appears in more than one covariant
    /// parameter position (Issue #5050).
    pub type_params: &'a [TypeParam],
}

/// Stable method identity at the shared call-resolution boundary (Issue #10461).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MethodId(pub usize);

/// Stable intrinsic identity at the shared call-resolution boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IntrinsicId(pub u32);

/// Stable intrinsic contract identity. The executor owns the concrete contract;
/// call resolution only records which checked contract was selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IntrinsicContractId(pub u32);

/// Stable constructor executor identity after the constructed type is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConstructorTargetId(pub u32);

/// Semantic identity of a callee after lexical lookup, before method selection.
///
/// Names retain their complete owner path; callable values and constructors use
/// structured types rather than a rendered leaf spelling. Executors may attach
/// VM-local IDs only after this identity has been resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalleeIdentity {
    GenericFunction { owner: Vec<String>, name: String },
    Closure { method: MethodId },
    Builtin { id: IntrinsicId },
    Constructor { ty: CoreType },
    CallableValue { ty: CoreType },
}

impl CalleeIdentity {
    /// Preserve a qualified function spelling as an owner path plus leaf name.
    pub fn from_function_name(name: &str) -> Self {
        let mut segments: Vec<String> = name.split('.').map(str::to_string).collect();
        let leaf = segments.pop().unwrap_or_default();
        Self::GenericFunction {
            owner: segments,
            name: leaf,
        }
    }
}

/// Whether a keyword value was supplied by the caller or obtained from the
/// selected method's default expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordOrigin {
    Explicit,
    Default,
}

/// One keyword argument visible to call resolution and frame construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordArg {
    pub name: String,
    pub ty: CoreType,
    pub origin: KeywordOrigin,
}

/// Lexical context in which a call was resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalScopeId {
    pub module: Vec<String>,
    pub method: Option<MethodId>,
}

/// Candidate methods considered for a dynamic Julia call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSet(pub Vec<MethodId>);

/// One resolved method type-variable binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeBinding {
    pub variable: CoreTypeVarId,
    pub value: CoreType,
}

/// Type bindings are explicit when the resolver owns them. Comparison adapters
/// use `NotObserved` until their legacy frame builder exposes the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeBindings {
    Complete(Vec<TypeBinding>),
    NotObserved,
}

/// Complete semantic input to call resolution (Issue #10461).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallRequest {
    pub callee: CalleeIdentity,
    pub positional: Vec<CoreType>,
    pub keywords: Vec<KeywordArg>,
    pub lexical_scope: LexicalScopeId,
    pub world: u64,
    pub call_span: Span,
    pub candidates: CandidateSet,
}

/// Resolution failures are semantic outcomes, not host runtime failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallResolutionError {
    NoMatchingMethod,
    AmbiguousMethod,
    Unsupported(String),
}

/// Semantic target selected before any compiler or VM execution fast path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedCall {
    JuliaMethod {
        method: MethodId,
        bindings: TypeBindings,
    },
    Intrinsic {
        id: IntrinsicId,
        contract: IntrinsicContractId,
    },
    Constructor {
        ty: CoreType,
        method: ConstructorTargetId,
    },
    Dynamic {
        candidate_set: CandidateSet,
    },
    Error(CallResolutionError),
}

/// Debug compare-mode gate for the call-resolution differential validator.
///
/// `SJULIA_CALL_RESOLVER_COMPARE=1` compares the callable-value scorer with the
/// runtime semantic method selector on the same [`CallRequest`]. It is
/// default-off and never changes the production selection (Issue #10461).
pub fn call_resolver_compare_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("SJULIA_CALL_RESOLVER_COMPARE")
            .is_ok_and(|value| !value.is_empty() && value != "0")
    })
}

/// Emit a call-resolver comparison diagnostic without panicking on a closed
/// stderr stream.
pub fn call_resolver_compare_log(args: std::fmt::Arguments<'_>) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{args}");
}

/// Whether comparison mode should report a legacy/proposed resolver pair.
pub fn call_resolutions_differ(legacy: &ResolvedCall, proposed: &ResolvedCall) -> bool {
    legacy != proposed
}

/// Build the structured dispatch cache key for a call-site argument tuple.
pub fn core_tuple_signature_from_julia_types(arg_types: &[JuliaType]) -> CoreType {
    CoreType::Tuple(
        arg_types
            .iter()
            .map(dispatch_core_type_from_julia)
            .collect(),
    )
}

/// Debug compare-mode gate for the binary-dispatch differential validator
/// (Issue #8620, parent #8609).
///
/// When `SJULIA_BINARY_DISPATCH_COMPARE` is set to a non-empty value other
/// than `"0"`, the compile-time binary-dispatch path logs a stderr line for
/// every call site where the compile-time decision (UniqueBuiltin / NeedsRuntime)
/// diverges from what [`binary_static_verdict`] would predict from the same
/// operand `LatticeType` pair.  Default-off diagnostic.
pub fn binary_dispatch_compare_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("SJULIA_BINARY_DISPATCH_COMPARE").is_ok_and(|v| !v.is_empty() && v != "0")
    })
}

/// Emit a compare-mode diagnostic line for binary dispatch (see
/// [`binary_dispatch_compare_enabled`]) without panicking on a closed stderr
/// (PANIC_FREE).
pub fn binary_dispatch_compare_log(args: std::fmt::Arguments<'_>) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{args}");
}

/// Debug compare-mode gate for the CoreType-native typemap candidate filter
/// (Issue #8548, parent #8438).
///
/// When `SJULIA_DISPATCH_COMPARE` is set to a non-empty value other than
/// `"0"`, `MethodTable::dispatch_inner` runs the typemap candidate filter
/// ([`typemap_candidate_accepts`]) alongside the production scoring-path
/// matcher and reports every divergence — per-candidate acceptance and the
/// final method selection — on stderr with the stable
/// `SJULIA_DISPATCH_COMPARE` line prefix, so fixture sweeps can grep for
/// disagreements before the filtering flip. Default-off diagnostic: one
/// cached env read, no effect on selection.
pub fn dispatch_compare_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("SJULIA_DISPATCH_COMPARE").is_ok_and(|v| !v.is_empty() && v != "0")
    })
}

/// Emit a compare-mode diagnostic line (see [`dispatch_compare_enabled`])
/// without panicking on a closed stderr (PANIC_FREE).
pub fn dispatch_compare_log(args: std::fmt::Arguments<'_>) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{args}");
}

/// Where-wrapped `Tuple{...}` signature over an arity-expanded parameter row:
/// `Tuple{params...}` wrapped by one `UnionAll` per `where` variable
/// (outermost wrapper = first declared variable, the same construction as
/// `MethodSig::compute_core_signature` / [`runtime_core_signature`]).
///
/// This is the subtype-faithful per-arity signature the typemap filter
/// consumes: `MethodSig::core_signature` itself renders a trailing `args...`
/// as an ordinary fixed parameter, so vararg methods must be expanded to the
/// call arity (`MethodSig::expanded_core_param_types_for_arity`) before the
/// signature participates in subtype queries (Issue #8548).
pub fn typemap_expanded_signature(
    expanded_param_cores: &[CoreType],
    type_vars: &[CoreTypeVar],
) -> CoreType {
    let mut sig = CoreType::Tuple(expanded_param_cores.to_vec());
    for var in type_vars.iter().rev() {
        sig = CoreType::UnionAll {
            var: var.clone(),
            body: Box::new(sig),
        };
    }
    sig
}

/// Verdict of the `findall`-style typemap candidate filter
/// ([`typemap_candidate_verdict`], Issue #8548).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypemapVerdict {
    /// The call tuple is precise and is a subtype of the where-wrapped
    /// canonical signature: the method is a candidate.
    Accept,
    /// The call tuple is precise and is NOT a subtype of the signature: the
    /// method is not a candidate.
    Reject,
    /// The call tuple carries statically-imprecise components (`Any`,
    /// abstract supertypes, bare parametric families, `DataType` type
    /// objects, ...): candidate acceptance is an *intersection* question
    /// (upstream `ml_matches` switches from `jl_subtype` to
    /// `jl_type_intersection` for widened call tuples), which the shared
    /// `CoreType` engine does not yet answer with method-env support. The
    /// caller keeps the scoring matcher's runtime-deferral policy for these.
    DeferImprecise,
    /// The signature carries a shape the subtype engine is known (by the
    /// Issue #8548 compare-mode evidence) not to decide faithfully yet —
    /// nested `where`-variable occurrences (`Wrap{T}` in invariant position,
    /// `Vector{<:Real}` anonymous bounds), lower-bounded or var-dependent
    /// `where` clauses (`S<:T`), value-parameterized abstract families
    /// (`AbsM{2,2,T}`, Issue #7960 — the value parameters live inside the
    /// `AbstractUser` *name string*, opaque to the engine), bare native
    /// array-family parameters (Issue #8804: the tuple-wrapped pattern
    /// matcher erases dimensionality), and abstract-element containers (Issue
    /// #8806: deliberate loose acceptance the verdict would overturn). The
    /// caller keeps the scoring matcher for these until the corresponding gap
    /// is closed or the looseness is retired.
    ///
    /// Note: nested `Union` components (e.g. `Vector{Union{Int64,Float64}}`,
    /// `Type{Union{...}}`) are **no longer deferred** — the engine was fixed
    /// by Issue #8582 and the slot-support predicate was updated by Issue
    /// #8817 to use [`core_type_is_sig_invariant_ground`] (which accepts
    /// ground `Union`s) for invariant positions in signatures.
    DeferSignature,
}

/// `findall`-style typemap candidate filtering (Issue #8548, parent #8438):
/// whether a method whose arity-expanded canonical signature is
/// `Tuple{expanded_param_cores...} where {type_vars...}` can be selected for
/// a call whose argument tuple type is `Tuple{arg_cores...}`, decided by the
/// shared [`CoreType`] subtype relation instead of the per-slot scoring-path
/// pattern matcher.
///
/// Mirrors upstream `ml_matches` (`julia/src/gf.c`): for a *precise*
/// (concrete, fully-known) call tuple the candidate relation is exact
/// subtyping `args <: sig` — including the diagonal `where` rule enforced by
/// the `UnionAll` pattern match — and the verdict is definitive
/// ([`TypemapVerdict::Accept`] / [`TypemapVerdict::Reject`]). For an
/// imprecise call tuple upstream switches to type intersection; the shared
/// engine can now use its conservative intersection in the safe direction only:
/// a proven `Bottom` rejects the candidate, while every non-Bottom result still
/// returns [`TypemapVerdict::DeferImprecise`] and stays on the scoring
/// matcher's documented compile-time runtime-deferral policies
/// (statically-`Any` slots vs primitive-ish parameters, `DataType` type
/// objects vs `Type{...}` parameters, bare parametric families, and the
/// Issue #6663 `Vector{Bool}`-vs-`BitArray` tag ambiguity). Signature
/// shapes the engine cannot yet decide faithfully return
/// [`TypemapVerdict::DeferSignature`] (see the variant doc).
pub fn typemap_candidate_verdict(
    hierarchy: &StructHierarchy,
    expanded_param_cores: &[CoreType],
    type_vars: &[CoreTypeVar],
    arg_cores: &[CoreType],
) -> TypemapVerdict {
    if expanded_param_cores.len() != arg_cores.len() {
        return TypemapVerdict::Reject;
    }
    // Normalize the Issue #5314 spelling before deciding: a single-letter
    // struct name (`Q5314`) images as `CoreType::TypeVar` in the canonical
    // signature; when it is NOT a declared `where` variable and carries no
    // bound it denotes a concrete struct leaf, exactly like the scoring
    // matcher's struct-leaf rule.
    let type_params: Vec<TypeParam> = type_vars.iter().map(core_type_var_to_type_param).collect();
    let normalized: Vec<CoreType> = expanded_param_cores
        .iter()
        .map(|param| {
            normalize_typemap_slot(
                &embed_type_param_bounds(param.clone(), &type_params),
                type_vars,
            )
        })
        .collect();
    if !arg_cores.iter().all(core_type_is_dispatch_precise) {
        if typemap_signature_supported(&normalized, type_vars) {
            let arg_tuple = CoreType::Tuple(arg_cores.to_vec());
            let signature = typemap_expanded_signature(&normalized, type_vars);
            if !core_type_contains_nominal_or_user_shape(&arg_tuple)
                && !core_type_contains_nominal_or_user_shape(&signature)
                && matches!(arg_tuple.type_intersect(&signature), CoreType::Bottom)
            {
                return TypemapVerdict::Reject;
            }
        }
        return TypemapVerdict::DeferImprecise;
    }
    if !typemap_signature_supported(&normalized, type_vars) {
        return TypemapVerdict::DeferSignature;
    }
    let arg_tuple = CoreType::Tuple(arg_cores.to_vec());
    let signature = typemap_expanded_signature(&normalized, type_vars);
    if core_match::dispatch_core_is_subtype_with_hierarchy(&arg_tuple, &signature, hierarchy) {
        TypemapVerdict::Accept
    } else {
        TypemapVerdict::Reject
    }
}

fn core_type_contains_nominal_or_user_shape(ty: &CoreType) -> bool {
    match ty {
        CoreType::AbstractUser { .. }
        | CoreType::Struct { .. }
        | CoreType::Module(_)
        | CoreType::Named(_) => true,
        CoreType::Tuple(elements) | CoreType::Union(elements) => elements
            .iter()
            .any(core_type_contains_nominal_or_user_shape),
        CoreType::NamedTuple(fields) => fields
            .iter()
            .any(|(_, ty)| core_type_contains_nominal_or_user_shape(ty)),
        CoreType::Vararg(element)
        | CoreType::VarargLen { element, .. }
        | CoreType::TypeOf(element) => core_type_contains_nominal_or_user_shape(element),
        CoreType::TypeVar(var) => {
            var.lower_bound
                .as_deref()
                .is_some_and(core_type_contains_nominal_or_user_shape)
                || var
                    .upper_bound
                    .as_deref()
                    .is_some_and(core_type_contains_nominal_or_user_shape)
        }
        CoreType::UnionAll { var, body } => {
            core_type_contains_nominal_or_user_shape(&CoreType::TypeVar(var.clone()))
                || core_type_contains_nominal_or_user_shape(body)
        }
        CoreType::Bottom
        | CoreType::Any
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::Value(_) => false,
    }
}

/// Slot normalization for the typemap filter (Issue #8548):
///
/// - The Issue #5314 struct-leaf rule: a `TypeVar`-imaged slot whose base
///   name is not a declared `where` variable and carries no bounds names a
///   concrete struct — replace it with the nominal `Named` leaf so the
///   subtype engine treats it nominally instead of as a free variable
///   (a free `TypeVar` on the right accepts everything).
/// - A `Named`-imaged slot that IS a declared `where` variable becomes the
///   declared `TypeVar` so the `UnionAll` pattern match binds it.
fn normalize_typemap_slot(param: &CoreType, type_vars: &[CoreTypeVar]) -> CoreType {
    match param {
        CoreType::TypeVar(var) if var.upper_bound.is_none() && var.lower_bound.is_none() => {
            let base = typemap_var_base_name(&var.name);
            if base != "_" && find_core_type_var(type_vars, base).is_none() {
                return CoreType::Named(base.to_string());
            }
            param.clone()
        }
        CoreType::Named(name) => match find_core_type_var(type_vars, name) {
            Some(declared) => CoreType::TypeVar(declared.clone()),
            None => param.clone(),
        },
        _ => param.clone(),
    }
}

/// Whether the normalized signature is within the fragment the subtype
/// engine decides faithfully (Issue #8548 compare-mode evidence; see
/// [`TypemapVerdict::DeferSignature`]). Supported: ground nominal /
/// primitive / abstract / container / tuple / `Type{ground}` slots, and
/// *whole-slot* `where`-variable occurrences (including the diagonal rule).
/// Deferred: nested variable occurrences, anonymous `<:` bounds inside
/// containers, lower-bounded or var-dependent `where` clauses, and
/// value-parameterized abstract families (Issue #7960).
fn typemap_signature_supported(params: &[CoreType], type_vars: &[CoreTypeVar]) -> bool {
    if type_vars.iter().any(|var| {
        var.lower_bound.is_some()
            || var
                .upper_bound
                .as_deref()
                .is_some_and(|bound| core_type_mentions_var_like(bound, type_vars))
    }) {
        return false;
    }
    params
        .iter()
        .all(|param| typemap_slot_supported(param, type_vars))
}

/// Per-slot support rule (Issue #8548). The guiding invariant, extracted
/// from the compare-mode evidence: **the subtype engine owns a slot only
/// when every invariant position inside it is sig-invariant-ground.**
/// Everything else — nested `where` variables, anonymous `{<:X}` bounds,
/// abstract / `Any` / bare-parametric components in invariant positions
/// (`Vector{Number}` — Issue #8806 loose acceptance, `Vector{Any}` — the
/// deliberate Issue #2352 erased-element looseness, `Type{Array{Pair}}` /
/// `Type{Vector}` bare families — Issue #8804) — stays on the scoring
/// matcher's policies via [`TypemapVerdict::DeferSignature`].
///
/// Ground `Union` components (e.g. `Vector{Union{Int64,Float64}}`,
/// `Type{Union{Int64,String}}`) are now owned by the engine: the subtype
/// engine was fixed to handle them correctly by Issue #8582, and this
/// slot-support predicate was updated (Issue #8817) to use the
/// signature-side [`core_type_is_sig_invariant_ground`] predicate in
/// invariant positions instead of the call-site-arg predicate
/// [`core_type_is_dispatch_precise`] which conservatively rejects `Union`
/// (an arg's static `Union` type only upper-bounds the runtime value).
fn typemap_slot_supported(param: &CoreType, type_vars: &[CoreTypeVar]) -> bool {
    match param {
        // Whole-slot variable: the `UnionAll` wrap (declared) or the
        // embedded bound (undeclared spelling) decides it. A bound that
        // itself mentions variables is deferred.
        CoreType::TypeVar(var) => var
            .upper_bound
            .as_deref()
            .is_none_or(|bound| !core_type_mentions_var_like(bound, type_vars)),
        // Value-parameterized abstract families compare by stripped family
        // name in the engine, ignoring the value parameters (Issue #7960):
        // defer to the scoring matcher's value-parameter checks.
        CoreType::AbstractUser { name, .. } => !name.contains('{'),
        // Whole-slot nominal / leaf / abstract parameters: the slot position
        // is covariant and the engine's nominal arms decide it.
        CoreType::Any
        | CoreType::Bottom
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::Value(_)
        | CoreType::Module(_)
        | CoreType::Named(_) => true,
        // A bare native array-family parameter (`Matrix` with no parameters)
        // erases its dimensionality in the tuple-wrapped pattern-matcher path
        // the verdict consumes (Issue #8804: `Tuple{Vector{Int64}} <:
        // Tuple{Matrix}` is spuriously true even though the bare arm was
        // fixed by #8560), so the scoring matcher keeps the dimension check.
        // Other bare families (`Dict`, `Pair`) accept any instantiation,
        // matching upstream `Dict{Int,Int} <: Dict`.
        CoreType::Struct { name, params } if params.is_empty() => {
            let family = name.split('{').next().unwrap_or(name);
            let family = family.rsplit('.').next().unwrap_or(family);
            !matches!(
                family,
                "Array" | "Vector" | "Matrix" | "BitArray" | "BitVector" | "BitMatrix"
            )
        }
        // Instantiated container: every invariant parameter must be a
        // sig-invariant-ground type. Abstract (`Vector{Number}` — the
        // scoring matcher accepts `Vector{Int64}` loosely, Issue #8806),
        // `Any` (`Vector{Any}` — Issue #2352 erased-element semantics),
        // and bare-parametric parameters all defer. Ground `Union` params
        // (e.g. `Vector{Union{Int64,Float64}}`) are now owned by the engine
        // via `core_type_is_sig_invariant_ground` (Issue #8817).
        CoreType::Struct { params, .. } => params.iter().all(core_type_is_sig_invariant_ground),
        // Tuple elements are covariant sub-slots, but unlike a whole slot
        // they must be variable-free: an anonymous bounded element
        // (`Tuple{<:Real, <:Real}`) is a set of INDEPENDENT fallback bounds
        // (Issue #6251), while the engine's pattern matcher would bind the
        // same-named anonymous variables diagonally and reject `(1, 2.0)`.
        CoreType::Tuple(elems) => elems.iter().all(|elem| {
            !core_type_mentions_var_like(elem, type_vars) && typemap_slot_supported(elem, type_vars)
        }),
        CoreType::NamedTuple(fields) => fields
            .iter()
            .all(|(_, field_ty)| core_type_is_dispatch_precise(field_ty)),
        // A whole-slot `Union` is a covariant position; its members are
        // decided like sub-slots but must be variable-free.
        CoreType::Union(members) => members.iter().all(|member| {
            !core_type_mentions_var_like(member, type_vars)
                && typemap_slot_supported(member, type_vars)
        }),
        // `Type{...}` compares its inner invariantly: own it only for exact
        // ground inners (concrete, or a whole abstract/nominal name — the
        // engine and the scoring matcher both use equality there). A bare
        // parametric (`Type{Vector}`, `Type{Array{Pair}}`) inner defers
        // (Issue #8804: `Tuple{Type{Vector{Int64}}} <: Tuple{Type{Vector}}`
        // is spuriously true in the tuple-wrapped path). Ground `Union`
        // inners (`Type{Union{Int64,String}}`) are now owned by the engine
        // via `core_type_is_sig_invariant_ground` (Issue #8817).
        CoreType::TypeOf(inner) => match inner.as_ref() {
            CoreType::Any | CoreType::Abstract(_) | CoreType::Named(_) => true,
            CoreType::AbstractUser { name, .. } => !name.contains('{'),
            other => core_type_is_sig_invariant_ground(other),
        },
        // Vararg / VarargLen / UnionAll slot shapes are not subtype-faithful
        // in the expanded row.
        _ => false,
    }
}

/// Whether a type expression mentions any type-variable-bearing component:
/// `TypeVar` / `UnionAll` / `Vararg` structure, or a `Named` occurrence of a
/// declared `where` variable.
fn core_type_mentions_var_like(core: &CoreType, type_vars: &[CoreTypeVar]) -> bool {
    match core {
        CoreType::TypeVar(_) | CoreType::UnionAll { .. } | CoreType::Vararg(_) => true,
        CoreType::Named(name) => find_core_type_var(type_vars, name).is_some(),
        CoreType::Struct { params, .. } => params
            .iter()
            .any(|param| core_type_mentions_var_like(param, type_vars)),
        CoreType::Tuple(elems) | CoreType::Union(elems) => elems
            .iter()
            .any(|elem| core_type_mentions_var_like(elem, type_vars)),
        CoreType::NamedTuple(fields) => fields
            .iter()
            .any(|(_, field_ty)| core_type_mentions_var_like(field_ty, type_vars)),
        CoreType::TypeOf(inner) => core_type_mentions_var_like(inner, type_vars),
        CoreType::VarargLen { element, len } => {
            core_type_mentions_var_like(element, type_vars)
                || core_type_mentions_var_like(len, type_vars)
        }
        _ => false,
    }
}

/// Base variable name before any embedded bound spelling (`T<:Number` /
/// `T>:Integer` legacy names survive verbatim in `CoreTypeVar::name`).
fn typemap_var_base_name(name: &str) -> &str {
    name.split_once("<:")
        .or_else(|| name.split_once(">:"))
        .map_or(name, |(base, _)| base)
        .trim()
}

fn find_core_type_var<'a>(type_vars: &'a [CoreTypeVar], var_name: &str) -> Option<&'a CoreTypeVar> {
    type_vars
        .iter()
        .find(|var| typemap_var_base_name(&var.name) == var_name)
}

/// Whether a call-site argument core type is *dispatch-precise*: the static
/// type pins the runtime value's dispatch behavior exactly, so subtyping
/// against the canonical method signature is the complete candidate relation
/// (Issue #8548).
///
/// Imprecise shapes — where the runtime value may satisfy a strictly more
/// specific parameter than the static type suggests — are:
///
/// - `Any`, abstract supertypes (built-in and user), and `Union`s: the value
///   is only bounded above.
/// - Type variables / `UnionAll` / `Vararg`: not ground.
/// - `Named`: user structs and user abstract types share this image, so a
///   `Named` argument cannot be classified without a nominal registry —
///   conservatively imprecise (growing `Named` precision is follow-up work).
/// - `TypeOf` whose inner type is not ground, and the bare `DataType`
///   abstract (a type object of statically-unknown identity, which the
///   scoring matcher deliberately lets match `Type{...}` parameters).
/// - Bare parametric families (`Rational`, `Dict`, `Memory`, ... with no
///   parameters) and containers with imprecise parameters (`Vector{Any}` in
///   sjulia's lattice means "element type unknown", not the concrete
///   `Vector{Any}`).
/// - `Bool`-element array families: compile-time type propagation cannot
///   distinguish a `Vector{Bool}` from a `BitVector` reusing the same native
///   Bool storage, so `BitArray` methods must stay candidates (Issue #6663).
pub fn core_type_is_dispatch_precise(core: &CoreType) -> bool {
    match core {
        CoreType::Primitive(_) | CoreType::Value(_) | CoreType::Module(_) | CoreType::Bottom => {
            true
        }
        CoreType::Struct { name, params } => {
            !params.is_empty()
                && params.iter().all(core_type_is_dispatch_precise)
                && !bool_array_family(name, params)
        }
        CoreType::Tuple(elems) => elems.iter().all(core_type_is_dispatch_precise),
        CoreType::NamedTuple(fields) => fields
            .iter()
            .all(|(_, field_ty)| core_type_is_dispatch_precise(field_ty)),
        // A `TypeOf` argument is an exact type object; it is precise whenever
        // the carried type is ground (no free variables). The carried type
        // itself may be abstract (`Type{Integer}` is a concrete singleton).
        CoreType::TypeOf(inner) => core_type_is_ground(inner),
        _ => false,
    }
}

/// Whether a type expression is suitable as the *signature-side* counterpart
/// to an invariant ground check (Issue #8817): the slot's shape is fully
/// known (ground), so the subtype engine can faithfully decide membership.
///
/// Differs from the call-site-arg predicate [`core_type_is_dispatch_precise`]
/// in one key respect: a **ground `Union`** is a valid invariant signature
/// shape.  The subtype engine was fixed by Issue #8582 to correctly decide
/// `T <: Union{A,B}` when A and B are concrete.  On the arg side, a
/// `Union`-typed argument means the runtime value is only bounded above (it
/// may be any branch), so call-tuple `Union` components remain imprecise and
/// `core_type_is_dispatch_precise` conservatively rejects them.
/// Signature-side, the union *is* the exact bound set declared by the method
/// author, so it is a valid ground shape the engine can own.
fn core_type_is_sig_invariant_ground(core: &CoreType) -> bool {
    match core {
        CoreType::Primitive(_) | CoreType::Value(_) | CoreType::Module(_) | CoreType::Bottom => {
            true
        }
        // A ground Union is a valid invariant signature shape; the engine
        // handles `T <: Union{A,B}` correctly when A and B are ground
        // (Issue #8582 / Issue #8817).
        CoreType::Union(members) => members.iter().all(core_type_is_sig_invariant_ground),
        CoreType::Struct { name, params } => {
            !params.is_empty()
                && params.iter().all(core_type_is_sig_invariant_ground)
                && !bool_array_family(name, params)
        }
        CoreType::Tuple(elems) => elems.iter().all(core_type_is_sig_invariant_ground),
        CoreType::NamedTuple(fields) => fields
            .iter()
            .all(|(_, field_ty)| core_type_is_sig_invariant_ground(field_ty)),
        // `TypeOf` inner comparison is invariant; it is ground when the
        // inner carries no free variables (same rule as the arg side).
        CoreType::TypeOf(inner) => core_type_is_ground(inner),
        _ => false,
    }
}

/// Whether a `Bool`-element native array family type is statically ambiguous
/// with the `BitArray` family (Issue #6663): BitArrays reuse the native Bool
/// array storage and compile-time type propagation cannot distinguish the
/// type tag, so a `Vector{Bool}`-typed argument may be a runtime `BitVector`.
fn bool_array_family(name: &str, params: &[CoreType]) -> bool {
    let family = name.split('{').next().unwrap_or(name);
    let family = family.rsplit('.').next().unwrap_or(family);
    matches!(family, "Array" | "Vector" | "Matrix")
        && matches!(
            params.first(),
            Some(CoreType::Primitive(CorePrimitive::Bool))
        )
}

/// Whether a type expression is ground: no free type variables, `UnionAll`
/// binders, or open `Vararg`/`Named` components anywhere inside.
fn core_type_is_ground(core: &CoreType) -> bool {
    match core {
        CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::Value(_)
        | CoreType::Module(_)
        | CoreType::Any
        | CoreType::Bottom => true,
        CoreType::AbstractUser { .. } => true,
        CoreType::Struct { params, .. } => params.iter().all(core_type_is_ground),
        CoreType::Tuple(elems) => elems.iter().all(core_type_is_ground),
        CoreType::NamedTuple(fields) => fields
            .iter()
            .all(|(_, field_ty)| core_type_is_ground(field_ty)),
        CoreType::Union(members) => members.iter().all(core_type_is_ground),
        CoreType::TypeOf(inner) => core_type_is_ground(inner),
        // `Named` can be a nominal user type or a rendered method type
        // parameter without its `where` context; conservatively not ground.
        _ => false,
    }
}

/// Resolve callable-value candidates with the existing VM score policy.
///
/// The VM still owns runtime representation matching and exactness checks. The
/// shared resolver owns arity, fixed-prefix bonuses, exact-match bonuses, and
/// fixed-arity preference, and structured dominance for otherwise tied top
/// candidates so callable-value dispatch no longer depends on storage order.
pub fn resolve_callable_value_candidates<'a, I, M, E>(
    hierarchy: &StructHierarchy,
    candidates: I,
    actual_type_names: &[String],
    mut type_matches: M,
    mut exact_matches: E,
) -> Option<(usize, u32)>
where
    I: IntoIterator<Item = CallableValueCandidate<'a>>,
    M: FnMut(&str, &JuliaType) -> bool,
    E: FnMut(&str, &JuliaType) -> bool,
{
    let mut best_match: Option<(CallableValueCandidate<'a>, u32, bool)> = None;
    let mut tied_best: Vec<CallableValueCandidate<'a>> = Vec::new();
    for candidate in candidates {
        let candidate_is_vararg = candidate.vararg_param_index.is_some();
        let required_arity = candidate
            .vararg_param_index
            .unwrap_or(candidate.param_count);
        let arity_match = if candidate.vararg_param_index.is_some() {
            if let Some(fixed_count) = candidate.vararg_fixed_count {
                actual_type_names.len() == required_arity + fixed_count
            } else {
                actual_type_names.len() >= required_arity
            }
        } else {
            actual_type_names.len() == required_arity
        };
        if !arity_match {
            continue;
        }

        let Some(specificity) = callable_value_candidate_score(
            &candidate,
            actual_type_names,
            &mut type_matches,
            &mut exact_matches,
        ) else {
            continue;
        };
        // Typemap matcher gate (Issue #8438): route callable-value `where`
        // candidates through the same CoreType-native matcher used by
        // MethodTable. This centralizes diagonal binding consistency and
        // explicit bound checks instead of layering callable-value-specific
        // predicates after the loose per-argument VM matcher.
        if !candidate.type_params.is_empty()
            && !callable_value_candidate_structured_match_ok(
                hierarchy,
                &candidate,
                actual_type_names,
            )
        {
            continue;
        }
        match best_match {
            None => best_match = Some((candidate, specificity, candidate_is_vararg)),
            Some((_, best_score, _)) if specificity > best_score => {
                best_match = Some((candidate, specificity, candidate_is_vararg));
                tied_best.clear();
            }
            Some((_, best_score, true)) if specificity == best_score && !candidate_is_vararg => {
                best_match = Some((candidate, specificity, false));
                tied_best.clear();
            }
            Some((best_candidate, best_score, best_is_vararg))
                if specificity == best_score && candidate_is_vararg == best_is_vararg =>
            {
                if tied_best.is_empty() {
                    tied_best.push(best_candidate);
                }
                tied_best.push(candidate);
            }
            _ => {}
        }
    }
    let (best_candidate, specificity, _) = best_match?;
    if tied_best.is_empty() {
        return Some((best_candidate.idx, specificity));
    }

    let dominant = unique_dominant_index(
        tied_best.len(),
        |_| true,
        |candidate, other| {
            callable_value_candidate_strictly_dominates(
                hierarchy,
                &tied_best[candidate],
                &tied_best[other],
                actual_type_names.len(),
            )
        },
    )
    .map(|index| tied_best[index])
    // Equivalent/incomparable legacy rows retain the established first-row
    // precedence. Only a proven unique dominant candidate may replace it.
    .unwrap_or(best_candidate);
    Some((dominant.idx, specificity))
}

fn callable_value_candidate_strictly_dominates(
    hierarchy: &StructHierarchy,
    candidate: &CallableValueCandidate<'_>,
    other: &CallableValueCandidate<'_>,
    actual_arity: usize,
) -> bool {
    let Some(candidate_signature) =
        callable_value_candidate_expanded_signature(candidate, actual_arity)
    else {
        return false;
    };
    let Some(other_signature) = callable_value_candidate_expanded_signature(other, actual_arity)
    else {
        return false;
    };
    candidate_signature.strict_subtype_dominates_with_hierarchy(&other_signature, hierarchy)
}

fn callable_value_candidate_expanded_signature(
    candidate: &CallableValueCandidate<'_>,
    actual_arity: usize,
) -> Option<CoreType> {
    let slots = (0..actual_arity)
        .map(|arg_idx| {
            let param = candidate.param_types.get(arg_idx).or_else(|| {
                candidate
                    .vararg_param_index
                    .and_then(|vararg_idx| candidate.param_types.get(vararg_idx))
            })?;
            Some(embed_type_param_bounds(
                runtime_candidate_core_type(param, &param.to_string()),
                candidate.type_params,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(runtime_core_signature(&slots, candidate.type_params))
}

fn callable_value_candidate_structured_match_ok(
    hierarchy: &StructHierarchy,
    candidate: &CallableValueCandidate<'_>,
    actual_type_names: &[String],
) -> bool {
    let mut slot_cores: Vec<CoreType> = Vec::with_capacity(actual_type_names.len());
    let mut actual_cores: Vec<CoreType> = Vec::with_capacity(actual_type_names.len());
    let mut relevant_slot_cores: Vec<CoreType> = Vec::new();
    let mut relevant_actual_cores: Vec<CoreType> = Vec::new();
    for (arg_idx, actual_type_name) in actual_type_names.iter().enumerate() {
        let param_jt = if arg_idx < candidate.param_types.len() {
            Some(&candidate.param_types[arg_idx])
        } else if let Some(vararg_idx) = candidate.vararg_param_index {
            candidate.param_types.get(vararg_idx)
        } else {
            None
        };
        let Some(param_jt) = param_jt else {
            // Arity shapes the scorer accepted but the declared parameter
            // list cannot describe: leave dispatch unchanged.
            return true;
        };
        let rendered = param_jt.to_string();
        let slot_core = embed_type_param_bounds(
            runtime_candidate_core_type(param_jt, &rendered),
            candidate.type_params,
        );
        let actual_core = CoreType::from_julia_name_for_dispatch(actual_type_name);
        if julia_type_mentions_type_params(param_jt, candidate.type_params) {
            relevant_slot_cores.push(slot_core.clone());
            relevant_actual_cores.push(actual_core.clone());
        }
        slot_cores.push(slot_core);
        actual_cores.push(actual_core);
    }
    let core_vars: Vec<CoreTypeVar> = candidate
        .type_params
        .iter()
        .map(CoreTypeVar::from)
        .collect();
    if !relevant_slot_cores.is_empty()
        && core_match::core_signature_match_with_bindings_with_hierarchy(
            &relevant_slot_cores,
            &relevant_actual_cores,
            &core_vars,
            hierarchy,
        )
        .is_none()
    {
        return false;
    }
    if slot_cores.iter().all(core_type_is_ground) {
        let signature = runtime_core_signature(&slot_cores, candidate.type_params);
        let tuple = CoreType::Tuple(actual_cores.clone());
        return core_match::dispatch_core_is_subtype_with_hierarchy(&tuple, &signature, hierarchy);
    }
    let has_explicit_bounds = candidate.type_params.iter().any(|tp| {
        tp.upper_bound.as_deref().is_some_and(|b| b != "Any")
            || tp.lower_bound.as_deref().is_some_and(|b| b != "Union{}")
    });
    if has_explicit_bounds {
        let signature = runtime_core_signature(&slot_cores, candidate.type_params);
        let tuple = CoreType::Tuple(actual_cores);
        return core_match::dispatch_core_is_subtype_with_hierarchy(&tuple, &signature, hierarchy);
    }
    true
}

/// Resolve string-encoded runtime candidates against string-encoded actual
/// argument type names.
///
/// The input shape matches `Instr::CallTypedDispatch` candidates while the
/// matching logic is expressed through [`CoreType`] instead of local parsers in
/// the VM instruction handler.
pub fn resolve_type_name_candidates<'a, I>(
    candidates: I,
    actual_type_names: &[String],
) -> Option<(usize, i32)>
where
    I: IntoIterator<Item = (usize, &'a [String])>,
{
    let mut best_match: Option<(usize, u32, i32)> = None;
    for (idx, expected_types) in candidates {
        if type_name_pattern_matches(expected_types, actual_type_names) {
            let quality = type_name_pattern_match_quality(expected_types, actual_type_names);
            let specificity = type_name_pattern_specificity(expected_types);
            if best_match.is_none_or(|(_, best_quality, best_specificity)| {
                quality > best_quality
                    || (quality == best_quality && specificity > best_specificity)
            }) {
                best_match = Some((idx, quality, specificity));
            }
        }
    }
    best_match.map(|(idx, _, specificity)| (idx, specificity))
}

fn callable_value_candidate_score<M, E>(
    candidate: &CallableValueCandidate<'_>,
    actual_type_names: &[String],
    type_matches: &mut M,
    exact_matches: &mut E,
) -> Option<u32>
where
    M: FnMut(&str, &JuliaType) -> bool,
    E: FnMut(&str, &JuliaType) -> bool,
{
    let mut specificity = if candidate.vararg_param_index.is_none() {
        1
    } else {
        0
    };
    for (arg_idx, arg_type_name) in actual_type_names.iter().enumerate() {
        let param_jt = if arg_idx < candidate.param_types.len() {
            Some(&candidate.param_types[arg_idx])
        } else if let Some(vararg_idx) = candidate.vararg_param_index {
            candidate.param_types.get(vararg_idx)
        } else {
            None
        };
        let Some(param_jt) = param_jt else {
            break;
        };

        let mentions_type_params = julia_type_mentions_type_params(param_jt, candidate.type_params);
        let arg_jt = if mentions_type_params {
            Some(JuliaType::from_name_or_struct(arg_type_name))
        } else {
            JuliaType::from_name(arg_type_name)
        };
        let mut matched_with_type_param_scope = false;
        if let Some(arg_jt) = arg_jt.as_ref() {
            let mut bindings = HashMap::new();
            if !julia_type_pattern_matches(param_jt, arg_jt, candidate.type_params, &mut bindings) {
                return None;
            }
            matched_with_type_param_scope = mentions_type_params;
        }

        if !matched_with_type_param_scope && !type_matches(arg_type_name, param_jt) {
            return None;
        }

        let mut param_score = u32::from(param_jt.specificity());
        if param_score == 0 {
            param_score = 1;
        }
        if candidate
            .vararg_param_index
            .is_none_or(|vararg_idx| arg_idx < vararg_idx)
        {
            param_score += 5;
        } else if mentions_type_params {
            param_score += VARARG_TYPE_PARAM_BINDING_BONUS;
        }
        specificity += param_score;
        if exact_matches(arg_type_name, param_jt) {
            specificity += 10;
        } else if callable_singleton_matches_function_param(arg_type_name, param_jt) {
            specificity += u32::try_from(PARAMETRIC_PATTERN_MATCH_BONUS).unwrap_or(0);
        } else {
            let param_core =
                embed_type_param_bounds(CoreType::from(param_jt), candidate.type_params);
            let arg_core = CoreType::from_julia_name_for_dispatch(arg_type_name);
            let pattern_score = param_core.dispatch_pattern_score(&arg_core);
            if matches!(param_jt, JuliaType::Function)
                && runtime_function_singleton_matches(arg_type_name, &arg_core, &param_core)
            {
                specificity += SUBTYPE_FALLBACK_MATCH_SCORE;
            }
            match pattern_score {
                3 => {
                    specificity += u32::try_from(PARAMETRIC_PATTERN_MATCH_BONUS).unwrap_or(0);
                }
                2 => {
                    specificity += SUBTYPE_FALLBACK_MATCH_SCORE;
                }
                _ if callable_subtype_fallback_score_eligible(param_jt, pattern_score) => {
                    specificity += SUBTYPE_FALLBACK_MATCH_SCORE;
                }
                _ => {}
            }
        }
    }
    Some(specificity)
}

fn callable_subtype_fallback_score_eligible(param_jt: &JuliaType, pattern_score: u32) -> bool {
    pattern_score == 0 && !matches!(param_jt, JuliaType::Any | JuliaType::TypeVar(_, _))
}

fn callable_singleton_matches_function_param(arg_type_name: &str, param_jt: &JuliaType) -> bool {
    matches!(param_jt, JuliaType::Function)
        && arg_type_name.starts_with("typeof(")
        && arg_type_name.ends_with(')')
}

/// Whether a declared parameter type mentions any of the method's `where`
/// type variables, recursing into parametric containers (Issue #5050).
fn julia_type_mentions_type_params(ty: &JuliaType, type_params: &[TypeParam]) -> bool {
    type_params
        .iter()
        .any(|tp| julia_type_mentions_type_param_name(ty, type_param_base_name(&tp.name)))
}

fn julia_type_mentions_type_param_name(ty: &JuliaType, name: &str) -> bool {
    match ty {
        JuliaType::TypeVar(type_name, _) => type_name == name,
        JuliaType::TypeOf(inner) | JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) => {
            julia_type_mentions_type_param_name(inner, name)
        }
        JuliaType::TupleOf(types) | JuliaType::Union(types) => types
            .iter()
            .any(|t| julia_type_mentions_type_param_name(t, name)),
        JuliaType::UnionAll { body, .. } => julia_type_mentions_type_param_name(body, name),
        JuliaType::Struct(type_name) => {
            if type_name == name {
                return true;
            }
            match type_name.find('{') {
                Some(brace) if type_name.ends_with('}') => type_name
                    [brace + 1..type_name.len() - 1]
                    .split(',')
                    .any(|arg| arg.trim() == name),
                _ => false,
            }
        }
        _ => false,
    }
}

/// Resolve string-encoded runtime candidates, allowing VM-owned subtype facts
/// to satisfy covariant bound patterns that CoreType cannot represent yet.
///
/// This keeps the typed-dispatch path on the existing i32 specificity policy
/// while moving the handler-local covariant fallback loops into the resolver.
pub fn resolve_type_name_candidates_with_subtype_fallback<'a, I, F>(
    candidates: I,
    actual_type_names: &[String],
    mut subtype_matches: F,
) -> Option<(usize, i32)>
where
    I: IntoIterator<Item = (usize, &'a [String])>,
    F: FnMut(&str, &str) -> bool,
{
    let candidates: Vec<(usize, &'a [String])> = candidates.into_iter().collect();
    if let Some(primary_match) = resolve_type_name_candidates(
        candidates
            .iter()
            .filter(|(_, sig)| {
                !sig.iter()
                    .any(|expected| expected.contains("_<:") || expected.contains("<:"))
            })
            .map(|(idx, sig)| (*idx, *sig)),
        actual_type_names,
    ) {
        return Some(primary_match);
    }

    let mut best_match: Option<(usize, u32, i32)> = None;
    for (idx, expected_types) in candidates {
        if type_name_pattern_matches_with_subtype_fallback(
            expected_types,
            actual_type_names,
            &mut subtype_matches,
        ) {
            let quality = type_name_pattern_match_quality(expected_types, actual_type_names);
            let specificity = type_name_pattern_specificity(expected_types);
            if best_match.is_none_or(|(_, best_quality, best_specificity)| {
                quality > best_quality
                    || (quality == best_quality && specificity > best_specificity)
            }) {
                best_match = Some((idx, quality, specificity));
            }
        }
    }
    best_match.map(|(idx, _, specificity)| (idx, specificity))
}

/// Score one runtime method-signature pattern against runtime type names.
///
/// Structural scoring is owned by [`CoreType::dispatch_pattern_score`].  VM
/// callers can inject a subtype fallback for user-defined ancestry that is not
/// fully represented in `CoreType` yet.  Higher scores are more specific.
pub fn runtime_type_pattern_score<F>(
    expected_types: &[&str],
    actual_type_names: &[&str],
    subtype_matches: &mut F,
) -> Option<u32>
where
    F: FnMut(&str, &str) -> bool,
{
    if expected_types.len() != actual_type_names.len() {
        return None;
    }

    let mut total_score = 0;
    for (expected, actual) in expected_types.iter().zip(actual_type_names.iter()) {
        let mut score = CoreType::from_julia_name_for_dispatch(expected)
            .dispatch_pattern_score(&CoreType::from_julia_name_for_dispatch(actual));
        if score == 0 && subtype_matches(actual, expected) {
            score = SUBTYPE_FALLBACK_MATCH_SCORE;
        }
        if score == 0 {
            return None;
        }
        total_score += score;
    }
    Some(total_score)
}

fn core_type_allows_family_fallback(expected: &CoreType) -> bool {
    match expected {
        CoreType::Struct { name, params } => params.is_empty() && !name.contains('.'),
        CoreType::Named(name) => !name.contains('.'),
        _ => false,
    }
}

/// A structured runtime dispatch candidate projected from the method's
/// canonical `core_signature` (Issue #6502 slice 2).
///
/// `slots` carries one expected [`CoreType`] per call argument position with
/// `where`-clause bounds embedded into the typevars (see
/// [`embed_type_param_bounds`]); `signature` carries the full
/// `core_signature`-shaped form (`Tuple{slots...}` wrapped by one `UnionAll`
/// per `where` parameter) when the method has `where` parameters, so the
/// resolver can enforce bounds AND cross-slot typevar binding consistency
/// through the shared subtype engine (Issue #6536). Typevar-free methods set
/// `signature: None` and skip the gate entirely.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeCoreCandidate<'a, const N: usize> {
    pub idx: usize,
    pub slots: [&'a CoreType; N],
    pub signature: Option<&'a CoreType>,
}

/// Runtime dispatch candidate for dynamically sized call-site arity.
///
/// This is the slice-backed counterpart of [`RuntimeCoreCandidate`], used by
/// fallback paths such as `IterateDynamic` where arity is known only from the
/// instruction operand. It keeps the same `core_signature` gate semantics.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeCoreSliceCandidate<'a> {
    pub idx: usize,
    pub slots: &'a [CoreType],
    pub signature: Option<&'a CoreType>,
}

/// Runtime typed-dispatch candidate with both the structured signature and the
/// rendered names used by the legacy typed-dispatch ordering policy.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeTypedCoreCandidate<'a> {
    pub idx: usize,
    pub rendered: &'a [String],
    pub slots: &'a [CoreType],
    pub signature: Option<&'a CoreType>,
}

/// Score one structured runtime signature against structured actual argument
/// types — the `core_signature`-based replacement for the string-encoded
/// [`runtime_type_pattern_score`] (Issue #6502 slice 2).
///
/// Per-slot structural scoring is owned by
/// [`CoreType::dispatch_pattern_score_in`] (hierarchy-aware, so user-declared
/// ancestry inside typevar bounds keeps its structural tier); the injected
/// `subtype_matches` fallback admits user-hierarchy matches the structural
/// tiers do not cover, above an untyped `Any` slot and below exact/parametric
/// structural matches.
pub fn runtime_core_pattern_score<F>(
    hierarchy: &StructHierarchy,
    expected_types: &[&CoreType],
    actual_types: &[CoreType],
    subtype_matches: &mut F,
) -> Option<u32>
where
    F: FnMut(&CoreType, &CoreType) -> bool,
{
    if expected_types.len() != actual_types.len() {
        return None;
    }

    let mut total_score = 0;
    for (expected, actual) in expected_types.iter().zip(actual_types.iter()) {
        let mut score = expected.dispatch_pattern_score_in(hierarchy, actual);
        if score == 0 && subtype_matches(actual, expected) {
            score = SUBTYPE_FALLBACK_MATCH_SCORE;
        }
        if score == 0 {
            return None;
        }
        total_score += score;
    }
    Some(total_score)
}

/// Structured scorer with an explicit same-family fallback tier.
///
/// Structured scoring owns exact/parametric/container tiers, `family_matches`
/// admits legacy wrapper families at tier 2 only for bare `Struct`/`Named`
/// candidates, and `subtype_matches` uses the same tier so declared abstract
/// supertypes outrank untyped `Any` fallbacks.
pub fn runtime_core_pattern_score_with_family_fallback<M, F>(
    hierarchy: &StructHierarchy,
    expected_types: &[&CoreType],
    actual_types: &[CoreType],
    family_matches: &mut M,
    subtype_matches: &mut F,
) -> Option<u32>
where
    M: FnMut(&CoreType, &CoreType) -> bool,
    F: FnMut(&CoreType, &CoreType) -> bool,
{
    if expected_types.len() != actual_types.len() {
        return None;
    }

    let mut total_score = 0;
    for (expected, actual) in expected_types.iter().zip(actual_types.iter()) {
        let mut score = expected.dispatch_pattern_score_in(hierarchy, actual);
        if score == 0
            && core_type_allows_family_fallback(expected)
            && family_matches(actual, expected)
        {
            score = 2;
        }
        if score == 0 && subtype_matches(actual, expected) {
            score = SUBTYPE_FALLBACK_MATCH_SCORE;
        }
        if score == 0 {
            return None;
        }
        total_score += score;
    }
    Some(total_score)
}

fn runtime_core_pattern_score_slice_with_family_fallback<M, F>(
    hierarchy: &StructHierarchy,
    expected_types: &[CoreType],
    actual_types: &[CoreType],
    family_matches: &mut M,
    subtype_matches: &mut F,
) -> Option<u32>
where
    M: FnMut(&CoreType, &CoreType) -> bool,
    F: FnMut(&CoreType, &CoreType) -> bool,
{
    if expected_types.len() != actual_types.len() {
        return None;
    }

    let mut total_score = 0;
    for (expected, actual) in expected_types.iter().zip(actual_types.iter()) {
        let mut score = expected.dispatch_pattern_score_in(hierarchy, actual);
        if score == 0
            && core_type_allows_family_fallback(expected)
            && family_matches(actual, expected)
        {
            score = 2;
        }
        if score == 0 && subtype_matches(actual, expected) {
            score = SUBTYPE_FALLBACK_MATCH_SCORE;
        }
        if score == 0 {
            return None;
        }
        total_score += score;
    }
    Some(total_score)
}

/// Resolve structured runtime candidates with the shared score ordering —
/// the `core_signature`-based primary path used by the VM's dynamic dispatch
/// call sites (Issue #6502 slice 2).
///
/// Ties keep the first candidate, matching the string path. Candidates whose
/// method has `where` parameters additionally pass through the
/// `core_signature` subtype gate (`Tuple{actuals...} <: signature` via the
/// shared engine), which enforces `where` bounds and cross-slot typevar
/// binding consistency that the per-slot string scoring missed (Issue #6536).
pub fn resolve_runtime_core_signature_candidates<'a, const N: usize, I, F>(
    hierarchy: &StructHierarchy,
    candidates: I,
    actual_types: &[CoreType; N],
    mut subtype_matches: F,
) -> Option<(usize, u32)>
where
    I: IntoIterator<Item = RuntimeCoreCandidate<'a, N>>,
    F: FnMut(&CoreType, &CoreType) -> bool,
{
    let mut actual_tuple: Option<CoreType> = None;
    let mut best_match: Option<(usize, u32, i32)> = None;
    for candidate in candidates {
        let Some(score) = runtime_core_pattern_score(
            hierarchy,
            &candidate.slots,
            actual_types,
            &mut subtype_matches,
        ) else {
            continue;
        };
        if let Some(signature) = candidate.signature {
            let tuple = actual_tuple.get_or_insert_with(|| CoreType::Tuple(actual_types.to_vec()));
            if !core_match::dispatch_core_is_subtype_with_hierarchy(tuple, signature, hierarchy) {
                continue;
            }
        }
        let specificity = core_type_pattern_specificity_refs(&candidate.slots);
        if best_match.is_none_or(|(_, best_score, best_specificity)| {
            score > best_score || (score == best_score && specificity > best_specificity)
        }) {
            best_match = Some((candidate.idx, score, specificity));
        }
    }
    best_match.map(|(idx, score, _)| (idx, score))
}

/// Resolve slice-backed structured runtime candidates with an explicit
/// same-family fallback tier (Issue #6502 residual string fallback removal).
pub fn resolve_runtime_core_signature_slice_candidates_with_family_fallback<'a, I, M, F>(
    hierarchy: &StructHierarchy,
    candidates: I,
    actual_types: &[CoreType],
    mut family_matches: M,
    mut subtype_matches: F,
) -> Option<(usize, u32)>
where
    I: IntoIterator<Item = RuntimeCoreSliceCandidate<'a>>,
    M: FnMut(&CoreType, &CoreType) -> bool,
    F: FnMut(&CoreType, &CoreType) -> bool,
{
    let mut actual_tuple: Option<CoreType> = None;
    let mut best_match: Option<(usize, u32, i32)> = None;
    for candidate in candidates {
        let Some(score) = runtime_core_pattern_score_slice_with_family_fallback(
            hierarchy,
            candidate.slots,
            actual_types,
            &mut family_matches,
            &mut subtype_matches,
        ) else {
            continue;
        };
        if let Some(signature) = candidate.signature {
            let tuple = actual_tuple.get_or_insert_with(|| CoreType::Tuple(actual_types.to_vec()));
            if !core_match::dispatch_core_is_subtype_with_hierarchy(tuple, signature, hierarchy) {
                continue;
            }
        }
        let specificity = core_type_pattern_specificity(candidate.slots);
        if best_match.is_none_or(|(_, best_score, best_specificity)| {
            score > best_score || (score == best_score && specificity > best_specificity)
        }) {
            best_match = Some((candidate.idx, score, specificity));
        }
    }
    best_match.map(|(idx, score, _)| (idx, score))
}

/// Like [`resolve_runtime_core_signature_slice_candidates_with_family_fallback`]
/// but returns `Err(())` when two or more candidates tie at the top score,
/// instead of silently picking the first winner.  Callers convert `Err(())` to
/// [`VmError::MethodError`] (ambiguous) so ties are surfaced the same way
/// `MethodTable::dispatch_inner` surfaces them (Issue #8999).
#[allow(clippy::result_unit_err)] // () is intentional — callers map it to a concrete error
pub fn resolve_runtime_core_signature_slice_candidates_with_family_fallback_or_tie<'a, I, M, F>(
    hierarchy: &StructHierarchy,
    candidates: I,
    actual_types: &[CoreType],
    mut family_matches: M,
    mut subtype_matches: F,
) -> Result<Option<(usize, u32)>, ()>
where
    I: IntoIterator<Item = RuntimeCoreSliceCandidate<'a>>,
    M: FnMut(&CoreType, &CoreType) -> bool,
    F: FnMut(&CoreType, &CoreType) -> bool,
{
    let mut actual_tuple: Option<CoreType> = None;
    let mut best_match: Option<(usize, u32, i32)> = None;
    let mut tied = false;
    for candidate in candidates {
        let Some(score) = runtime_core_pattern_score_slice_with_family_fallback(
            hierarchy,
            candidate.slots,
            actual_types,
            &mut family_matches,
            &mut subtype_matches,
        ) else {
            continue;
        };
        if let Some(signature) = candidate.signature {
            let tuple = actual_tuple.get_or_insert_with(|| CoreType::Tuple(actual_types.to_vec()));
            if !core_match::dispatch_core_is_subtype_with_hierarchy(tuple, signature, hierarchy) {
                continue;
            }
        }
        let specificity = core_type_pattern_specificity(candidate.slots);
        match best_match {
            None => {
                best_match = Some((candidate.idx, score, specificity));
                tied = false;
            }
            Some((_, best_score, best_specificity))
                if score > best_score
                    || (score == best_score && specificity > best_specificity) =>
            {
                best_match = Some((candidate.idx, score, specificity));
                tied = false;
            }
            Some((_, best_score, best_specificity))
                if score == best_score && specificity == best_specificity =>
            {
                tied = true;
            }
            _ => {}
        }
    }
    if tied {
        Err(())
    } else {
        Ok(best_match.map(|(idx, score, _)| (idx, score)))
    }
}

/// Resolve typed-dispatch candidates from structured per-slot [`CoreType`]s.
///
/// This is the `core_signature`-backed counterpart of
/// [`resolve_type_name_candidates_with_subtype_fallback`] for
/// `CallTypedDispatch[OrBuiltin*]`. Candidate matching uses structured slots
/// and the optional full-signature gate, while the final tie-break uses the
/// same typed-dispatch quality/specificity policy over the structured slots so
/// this slice can replace the VM's production string resolver without changing
/// method ordering.
pub fn resolve_typed_runtime_core_candidates_with_subtype_fallback<'a, I, F>(
    hierarchy: &StructHierarchy,
    candidates: I,
    actual_types: &[CoreType],
    mut subtype_matches: F,
) -> Option<(usize, i32)>
where
    I: IntoIterator<Item = RuntimeTypedCoreCandidate<'a>>,
    F: FnMut(&CoreType, &CoreType) -> bool,
{
    let candidates: Vec<_> = candidates.into_iter().collect();
    if let Some(primary_match) = resolve_typed_runtime_core_candidates(
        hierarchy,
        candidates.iter().copied().filter(|candidate| {
            !candidate
                .slots
                .iter()
                .any(core_type_pattern_has_explicit_bound)
        }),
        actual_types,
    ) {
        return Some(primary_match);
    }

    let mut actual_tuple: Option<CoreType> = None;
    let mut best_match: Option<(usize, u32, i32)> = None;
    for candidate in candidates {
        if !typed_core_candidate_matches_with_subtype_fallback(
            hierarchy,
            &candidate,
            actual_types,
            &mut subtype_matches,
            &mut actual_tuple,
        ) {
            continue;
        }
        let quality = typed_core_pattern_match_quality(candidate.slots, actual_types);
        let specificity = core_type_pattern_specificity(candidate.slots);
        if best_match.is_none_or(|(_, best_quality, best_specificity)| {
            quality > best_quality || (quality == best_quality && specificity > best_specificity)
        }) {
            best_match = Some((candidate.idx, quality, specificity));
        }
    }
    best_match.map(|(idx, _, specificity)| (idx, specificity))
}

fn resolve_typed_runtime_core_candidates<'a, I>(
    hierarchy: &StructHierarchy,
    candidates: I,
    actual_types: &[CoreType],
) -> Option<(usize, i32)>
where
    I: IntoIterator<Item = RuntimeTypedCoreCandidate<'a>>,
{
    let mut actual_tuple: Option<CoreType> = None;
    let mut best_match: Option<(usize, u32, i32)> = None;
    for candidate in candidates {
        if !typed_core_candidate_matches(hierarchy, &candidate, actual_types, &mut actual_tuple) {
            continue;
        }
        let quality = typed_core_pattern_match_quality(candidate.slots, actual_types);
        let specificity = core_type_pattern_specificity(candidate.slots);
        if best_match.is_none_or(|(_, best_quality, best_specificity)| {
            quality > best_quality || (quality == best_quality && specificity > best_specificity)
        }) {
            best_match = Some((candidate.idx, quality, specificity));
        }
    }
    best_match.map(|(idx, _, specificity)| (idx, specificity))
}

fn typed_core_candidate_matches(
    hierarchy: &StructHierarchy,
    candidate: &RuntimeTypedCoreCandidate<'_>,
    actual_types: &[CoreType],
    actual_tuple: &mut Option<CoreType>,
) -> bool {
    if candidate.slots.len() != actual_types.len() {
        return false;
    }

    let mut bindings = HashMap::new();
    for ((expected, actual), rendered) in candidate
        .slots
        .iter()
        .zip(actual_types.iter())
        .zip(candidate.rendered.iter())
    {
        if exact_typeof_rendered_concrete_miss(rendered, actual, candidate.signature) {
            return false;
        }
        if !core_pattern_matches(expected, actual, &mut bindings) {
            return false;
        }
    }
    typed_core_signature_gate_passes(hierarchy, candidate, actual_types, actual_tuple)
}

fn typed_core_candidate_matches_with_subtype_fallback<F>(
    hierarchy: &StructHierarchy,
    candidate: &RuntimeTypedCoreCandidate<'_>,
    actual_types: &[CoreType],
    subtype_matches: &mut F,
    actual_tuple: &mut Option<CoreType>,
) -> bool
where
    F: FnMut(&CoreType, &CoreType) -> bool,
{
    if candidate.slots.len() != actual_types.len() {
        return false;
    }

    let mut bindings = HashMap::new();
    for ((expected, actual), rendered) in candidate
        .slots
        .iter()
        .zip(actual_types.iter())
        .zip(candidate.rendered.iter())
    {
        if exact_typeof_rendered_concrete_miss(rendered, actual, candidate.signature) {
            return false;
        }
        if same_invariant_container_family_concrete_miss_core(expected, actual) {
            return false;
        }
        if core_pattern_matches(expected, actual, &mut bindings) {
            continue;
        }
        if core_type_has_previously_bound_typevars(expected, &bindings) {
            return false;
        }
        if !subtype_matches(actual, expected) {
            return false;
        }
    }
    typed_core_signature_gate_passes(hierarchy, candidate, actual_types, actual_tuple)
}

fn exact_typeof_rendered_concrete_miss(
    rendered: &str,
    actual: &CoreType,
    signature: Option<&CoreType>,
) -> bool {
    let Some(inner) = rendered
        .strip_prefix("Type{")
        .and_then(|rest| rest.strip_suffix('}'))
        .map(str::trim)
    else {
        return false;
    };
    if inner.contains('{')
        || inner.contains("<:")
        || inner.contains(">:")
        || inner.contains(" where ")
    {
        return false;
    }
    if signature.is_some_and(|signature| {
        core_unionall_var_names(signature)
            .iter()
            .any(|name| name == inner)
    }) {
        return false;
    }
    let Some(actual_inner) = core_typeof_inner_name(actual) else {
        return false;
    };
    inner != actual_inner
}

fn core_unionall_var_names(core: &CoreType) -> Vec<String> {
    match core {
        CoreType::UnionAll { var, body } => {
            let mut names = vec![var.name.clone()];
            names.extend(core_unionall_var_names(body));
            names
        }
        _ => vec![],
    }
}

fn core_typeof_inner_name(actual: &CoreType) -> Option<String> {
    let CoreType::TypeOf(inner) = actual else {
        return None;
    };
    match inner.as_ref() {
        CoreType::Primitive(_) => Some(inner.to_julia_name()),
        CoreType::Struct { name, .. } | CoreType::AbstractUser { name, .. } => Some(name.clone()),
        CoreType::Named(name) => Some(name.clone()),
        CoreType::TypeVar(var) => Some(var.name.clone()),
        CoreType::Any => Some("Any".to_string()),
        CoreType::Bottom => Some("Union{}".to_string()),
        CoreType::Abstract(_) => Some(inner.to_julia_name()),
        CoreType::TypeOf(_)
        | CoreType::Tuple(_)
        | CoreType::Vararg(_)
        | CoreType::VarargLen { .. }
        | CoreType::NamedTuple(_)
        | CoreType::Union(_)
        | CoreType::Value(_)
        | CoreType::UnionAll { .. } => None,
        CoreType::Module(name) => Some(name.clone()),
    }
}

fn typed_core_signature_gate_passes(
    hierarchy: &StructHierarchy,
    candidate: &RuntimeTypedCoreCandidate<'_>,
    actual_types: &[CoreType],
    actual_tuple: &mut Option<CoreType>,
) -> bool {
    let Some(signature) = candidate.signature else {
        return true;
    };
    let tuple = actual_tuple.get_or_insert_with(|| CoreType::Tuple(actual_types.to_vec()));
    core_match::dispatch_core_is_subtype_with_hierarchy(tuple, signature, hierarchy)
}

fn typed_core_pattern_match_quality(expected_types: &[CoreType], actual_types: &[CoreType]) -> u32 {
    expected_types
        .iter()
        .zip(actual_types.iter())
        .map(|(expected, actual)| {
            if expected == actual {
                2
            } else if expected.dispatch_pattern_score(actual) == 3 {
                1
            } else {
                0
            }
        })
        .sum()
}

fn core_type_pattern_specificity(expected_types: &[CoreType]) -> i32 {
    let mut specificity = 0;
    let mut type_var_count = 0;
    let mut same_type_var_bonus = 0;
    let mut seen_type_vars = HashSet::new();

    for expected in expected_types {
        let type_vars = core_typevar_names(expected);
        for name in type_vars {
            type_var_count += 1;
            if !seen_type_vars.insert(name) {
                same_type_var_bonus += 100;
            }
        }

        if !matches!(expected, CoreType::TypeVar(_)) {
            let param_bonus = i32::from(core_type_pattern_has_parametric_surface(expected));
            specificity += expected.specificity() as i32
                + param_bonus
                + core_type_user_abstract_specificity_bonus(expected);
        }
    }

    specificity - type_var_count + same_type_var_bonus
}

pub fn core_signature_pattern_specificity(expected_types: &[CoreType]) -> i32 {
    core_type_pattern_specificity(expected_types)
}

fn core_type_pattern_specificity_refs(expected_types: &[&CoreType]) -> i32 {
    let mut specificity = 0;
    let mut type_var_count = 0;
    let mut same_type_var_bonus = 0;
    let mut seen_type_vars = HashSet::new();

    for expected in expected_types.iter().copied() {
        let type_vars = core_typevar_names(expected);
        for name in type_vars {
            type_var_count += 1;
            if !seen_type_vars.insert(name) {
                same_type_var_bonus += 100;
            }
        }

        if !matches!(expected, CoreType::TypeVar(_)) {
            let param_bonus = i32::from(core_type_pattern_has_parametric_surface(expected));
            specificity += expected.specificity() as i32
                + param_bonus
                + core_type_user_abstract_specificity_bonus(expected);
        }
    }

    specificity - type_var_count + same_type_var_bonus
}

fn core_type_user_abstract_specificity_bonus(core: &CoreType) -> i32 {
    match core {
        CoreType::AbstractUser { name, .. }
            if !CoreType::is_builtin_abstract_datatype_for_julia_name(name) =>
        {
            1
        }
        _ => 0,
    }
}

fn core_type_pattern_has_parametric_surface(core: &CoreType) -> bool {
    match core {
        CoreType::Bottom => true,
        CoreType::Struct { params, .. } => !params.is_empty(),
        CoreType::Tuple(_) | CoreType::Union(_) | CoreType::TypeOf(_) => true,
        CoreType::Vararg(_) | CoreType::VarargLen { .. } => true,
        CoreType::NamedTuple(_) => true,
        CoreType::UnionAll { body, .. } => core_type_pattern_has_parametric_surface(body),
        CoreType::AbstractUser { name, .. } => name.contains('{'),
        CoreType::Any
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::TypeVar(_)
        | CoreType::Value(_)
        | CoreType::Module(_)
        | CoreType::Named(_) => false,
    }
}

fn core_type_pattern_has_explicit_bound(core: &CoreType) -> bool {
    match core {
        CoreType::TypeVar(var) => var.upper_bound.is_some() || var.lower_bound.is_some(),
        CoreType::Named(name) => name.contains("_<:") || name.contains("<:"),
        CoreType::Struct { params, .. } | CoreType::Tuple(params) | CoreType::Union(params) => {
            params.iter().any(core_type_pattern_has_explicit_bound)
        }
        CoreType::TypeOf(inner) | CoreType::Vararg(inner) => {
            core_type_pattern_has_explicit_bound(inner)
        }
        CoreType::VarargLen { element, len } => {
            core_type_pattern_has_explicit_bound(element)
                || core_type_pattern_has_explicit_bound(len)
        }
        CoreType::NamedTuple(fields) => fields
            .iter()
            .any(|(_, ty)| core_type_pattern_has_explicit_bound(ty)),
        CoreType::UnionAll { var, body } => {
            var.upper_bound.is_some()
                || var.lower_bound.is_some()
                || core_type_pattern_has_explicit_bound(body)
        }
        CoreType::Any
        | CoreType::Bottom
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::AbstractUser { .. }
        | CoreType::Value(_)
        | CoreType::Module(_) => false,
    }
}

fn same_invariant_container_family_concrete_miss_core(
    expected: &CoreType,
    actual: &CoreType,
) -> bool {
    if let (CoreType::TypeOf(expected_inner), CoreType::TypeOf(actual_inner)) = (expected, actual) {
        return expected_inner != actual_inner && core_typevar_names(expected_inner).is_empty();
    }

    let (
        CoreType::Struct {
            name: expected_name,
            params: expected_params,
        },
        CoreType::Struct {
            name: actual_name,
            params: actual_params,
        },
    ) = (expected, actual)
    else {
        return false;
    };

    expected_name == actual_name
        && matches!(
            expected_name.as_str(),
            "Array" | "Vector" | "Matrix" | "Dict" | "Set"
        )
        && !expected_params.is_empty()
        && expected_params.len() == actual_params.len()
        && core_typevar_names(expected).is_empty()
}

/// Embed `where`-clause bounds into the typevars of a structurally converted
/// parameter core type.
///
/// Lowering keeps bounds for typevars inside parametric struct annotations
/// only on the method's `type_params` (`convert_type_with_type_vars` does not
/// descend into `JuliaType::Struct("Wrap{T}")`, so the rendered string carries
/// no bound — Issue #6536). The structured candidate path re-attaches them so
/// per-slot matching enforces the same bounds the compile-time matcher checks
/// via `core_signature`. `UnionAll` binders inside the type shadow outer
/// `where` parameters of the same name, mirroring scope rules.
pub fn embed_type_param_bounds(core: CoreType, type_params: &[TypeParam]) -> CoreType {
    if type_params.is_empty() {
        return core;
    }
    embed_type_param_bounds_scoped(core, type_params, &mut Vec::new())
}

/// Assign stable lexical identities to a method signature's declared `where`
/// parameters and rebind only unresolved references in its structural body.
/// Free runtime TypeVars remain rigid even when they share a display name with
/// a declared parameter (Issue #10460).
pub fn scope_method_type_params(
    core: CoreType,
    type_params: &[TypeParam],
) -> (CoreType, Vec<CoreTypeVar>) {
    let mut binders = Vec::with_capacity(type_params.len());
    let first_scope_id = max_core_scope_id(&core).saturating_add(1).max(1);
    for (index, type_param) in type_params.iter().enumerate() {
        let scope_id = first_scope_id.saturating_add(index as u32);
        let mut binder = CoreTypeVar::from(type_param).with_scope_id(scope_id);
        let prior_substitutions = method_binder_substitutions(&binders);
        binder.lower_bound = binder.lower_bound.as_deref().map(|bound| {
            Box::new(scope_unresolved_method_typevars(
                bound,
                &prior_substitutions,
            ))
        });
        binder.upper_bound = binder.upper_bound.as_deref().map(|bound| {
            Box::new(scope_unresolved_method_typevars(
                bound,
                &prior_substitutions,
            ))
        });
        binders.push(binder);
    }

    let substitutions = method_binder_substitutions(&binders);
    (
        scope_unresolved_method_typevars(&core, &substitutions),
        binders,
    )
}

fn scope_unresolved_method_typevars(
    core: &CoreType,
    substitutions: &[CoreTypeSubstitution],
) -> CoreType {
    let unresolved_substitution = |name: &str| {
        substitutions
            .iter()
            .rev()
            .find(|substitution| substitution.variable.name == name)
            .map(|substitution| substitution.value.clone())
    };

    match core {
        CoreType::TypeVar(var) if matches!(var.typevar_id(), CoreTypeVarId::Unresolved) => {
            unresolved_substitution(&var.name).unwrap_or_else(|| CoreType::TypeVar(var.clone()))
        }
        CoreType::TypeVar(var) => {
            let mut var = var.clone();
            var.lower_bound = var
                .lower_bound
                .as_deref()
                .map(|bound| Box::new(scope_unresolved_method_typevars(bound, substitutions)));
            var.upper_bound = var
                .upper_bound
                .as_deref()
                .map(|bound| Box::new(scope_unresolved_method_typevars(bound, substitutions)));
            CoreType::TypeVar(var)
        }
        CoreType::Named(name) => unresolved_substitution(name).unwrap_or_else(|| core.clone()),
        CoreType::Struct { name, params } if params.is_empty() => {
            unresolved_substitution(name).unwrap_or_else(|| core.clone())
        }
        CoreType::Struct { name, params } => CoreType::Struct {
            name: name.clone(),
            params: params
                .iter()
                .map(|param| scope_unresolved_method_typevars(param, substitutions))
                .collect(),
        },
        CoreType::Tuple(elements) => CoreType::Tuple(
            elements
                .iter()
                .map(|element| scope_unresolved_method_typevars(element, substitutions))
                .collect(),
        ),
        CoreType::Union(types) => CoreType::Union(
            types
                .iter()
                .map(|ty| scope_unresolved_method_typevars(ty, substitutions))
                .collect(),
        ),
        CoreType::Vararg(inner) => CoreType::Vararg(Box::new(scope_unresolved_method_typevars(
            inner,
            substitutions,
        ))),
        CoreType::VarargLen { element, len } => CoreType::VarargLen {
            element: Box::new(scope_unresolved_method_typevars(element, substitutions)),
            len: Box::new(scope_unresolved_method_typevars(len, substitutions)),
        },
        CoreType::TypeOf(inner) => CoreType::TypeOf(Box::new(scope_unresolved_method_typevars(
            inner,
            substitutions,
        ))),
        CoreType::NamedTuple(fields) => CoreType::NamedTuple(
            fields
                .iter()
                .map(|(name, ty)| {
                    (
                        name.clone(),
                        scope_unresolved_method_typevars(ty, substitutions),
                    )
                })
                .collect(),
        ),
        CoreType::UnionAll { var, body } => {
            let mut scoped_var = var.clone();
            scoped_var.lower_bound = scoped_var
                .lower_bound
                .as_deref()
                .map(|bound| Box::new(scope_unresolved_method_typevars(bound, substitutions)));
            scoped_var.upper_bound = scoped_var
                .upper_bound
                .as_deref()
                .map(|bound| Box::new(scope_unresolved_method_typevars(bound, substitutions)));
            let inner_substitutions = substitutions
                .iter()
                .filter(|substitution| substitution.variable.name != var.name)
                .cloned()
                .collect::<Vec<_>>();
            CoreType::UnionAll {
                var: scoped_var,
                body: Box::new(scope_unresolved_method_typevars(body, &inner_substitutions)),
            }
        }
        CoreType::AbstractUser { name, parent } => CoreType::AbstractUser {
            name: name.clone(),
            parent: parent
                .as_deref()
                .map(|parent| Box::new(scope_unresolved_method_typevars(parent, substitutions))),
        },
        CoreType::Bottom
        | CoreType::Any
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::Value(_)
        | CoreType::Module(_) => core.clone(),
    }
}

fn max_core_scope_id(core: &CoreType) -> u32 {
    let bounds_max = |var: &CoreTypeVar| {
        var.scope_id
            .max(var.lower_bound.as_deref().map_or(0, max_core_scope_id))
            .max(var.upper_bound.as_deref().map_or(0, max_core_scope_id))
    };
    match core {
        CoreType::TypeVar(var) => bounds_max(var),
        CoreType::UnionAll { var, body } => bounds_max(var).max(max_core_scope_id(body)),
        CoreType::Struct { params, .. } | CoreType::Tuple(params) | CoreType::Union(params) => {
            params.iter().map(max_core_scope_id).max().unwrap_or(0)
        }
        CoreType::Vararg(inner) | CoreType::TypeOf(inner) => max_core_scope_id(inner),
        CoreType::VarargLen { element, len } => {
            max_core_scope_id(element).max(max_core_scope_id(len))
        }
        CoreType::NamedTuple(fields) => fields
            .iter()
            .map(|(_, field_type)| max_core_scope_id(field_type))
            .max()
            .unwrap_or(0),
        CoreType::AbstractUser { parent, .. } => parent.as_deref().map_or(0, max_core_scope_id),
        CoreType::Bottom
        | CoreType::Any
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::Value(_)
        | CoreType::Module(_)
        | CoreType::Named(_) => 0,
    }
}

fn method_binder_substitutions(binders: &[CoreTypeVar]) -> Vec<CoreTypeSubstitution> {
    binders
        .iter()
        .map(|binder| {
            CoreTypeSubstitution::new(
                CoreTypeVar::unscoped(binder.name.clone()),
                CoreType::TypeVar(binder.clone()),
            )
        })
        .collect()
}

fn embed_type_param_bounds_scoped(
    core: CoreType,
    type_params: &[TypeParam],
    shadowed: &mut Vec<String>,
) -> CoreType {
    match core {
        CoreType::TypeVar(var)
            if matches!(var.typevar_id(), CoreTypeVarId::Unresolved)
                && var.upper_bound.is_none()
                && var.lower_bound.is_none()
                && !shadowed.contains(&var.name) =>
        {
            match type_params.iter().find(|tp| tp.name == var.name) {
                Some(tp) => CoreType::TypeVar(CoreTypeVar::from(tp)),
                None => CoreType::TypeVar(var),
            }
        }
        CoreType::Named(name) if !shadowed.contains(&name) => {
            match type_params.iter().find(|tp| tp.name == name) {
                Some(tp) => CoreType::TypeVar(CoreTypeVar::from(tp)),
                None => CoreType::Named(name),
            }
        }
        CoreType::Struct { name, params } => CoreType::Struct {
            name,
            params: params
                .into_iter()
                .map(|p| embed_type_param_bounds_scoped(p, type_params, shadowed))
                .collect(),
        },
        CoreType::Tuple(elems) => CoreType::Tuple(
            elems
                .into_iter()
                .map(|e| embed_type_param_bounds_scoped(e, type_params, shadowed))
                .collect(),
        ),
        CoreType::Union(arms) => CoreType::Union(
            arms.into_iter()
                .map(|a| embed_type_param_bounds_scoped(a, type_params, shadowed))
                .collect(),
        ),
        CoreType::TypeOf(inner) => CoreType::TypeOf(Box::new(embed_type_param_bounds_scoped(
            *inner,
            type_params,
            shadowed,
        ))),
        CoreType::Vararg(inner) => CoreType::Vararg(Box::new(embed_type_param_bounds_scoped(
            *inner,
            type_params,
            shadowed,
        ))),
        CoreType::VarargLen { element, len } => CoreType::VarargLen {
            element: Box::new(embed_type_param_bounds_scoped(
                *element,
                type_params,
                shadowed,
            )),
            len: Box::new(embed_type_param_bounds_scoped(*len, type_params, shadowed)),
        },
        CoreType::UnionAll { var, body } => {
            shadowed.push(var.name.clone());
            let body = embed_type_param_bounds_scoped(*body, type_params, shadowed);
            shadowed.pop();
            CoreType::UnionAll {
                var,
                body: Box::new(body),
            }
        }
        other => other,
    }
}

/// Build the runtime mirror of `MethodSig::core_signature` from per-call slot
/// core types and the method's `where` parameters: `Tuple{slots...}` wrapped
/// by one `UnionAll` per `where` parameter (outermost wrapper = first
/// parameter, same construction as `MethodSig::compute_core_signature`).
pub fn runtime_core_signature(slot_cores: &[CoreType], type_params: &[TypeParam]) -> CoreType {
    let mut sig = CoreType::Tuple(slot_cores.to_vec());
    for type_param in type_params.iter().rev() {
        sig = CoreType::UnionAll {
            var: CoreTypeVar::from(type_param),
            body: Box::new(sig),
        };
    }
    sig
}

/// Project a declared parameter `JuliaType` onto the [`CoreType`] used for
/// structured runtime candidate matching (Issue #6502 slice 2).
///
/// The structural `CoreType::from(&JuliaType)` conversion is the default source
/// for runtime candidate slots (the same shape `MethodSig::core_signature`
/// serializes). Some method payloads still carry erased `JuliaType::Array`
/// declarations while their rendered signature preserves `Vector{T}` /
/// `Matrix{T}` / `Array{T}` parameters; keep that parametric array shape
/// structured here so runtime typed dispatch can enforce diagonal bindings
/// without returning to string matching at the call site. Divergent rendered
/// forms such as `Module` keep their structured image and rely on `CoreType`'s
/// nominal bridge rules. Parametric `AbstractUser` spellings are the exception:
/// their value/type parameters live inside the name string (`AbsM{2,2,T}`), so
/// they are parsed into a structured nominal application before matching.
pub fn runtime_candidate_core_type(declared: &JuliaType, rendered: &str) -> CoreType {
    if matches!(declared, JuliaType::Array) && rendered_parametric_array_core(rendered).is_some() {
        return CoreType::from_julia_name_for_dispatch(rendered);
    }
    if matches!(declared, JuliaType::AbstractUser(name, _) if name.contains('{')) {
        return CoreType::from_julia_name_for_dispatch(rendered);
    }
    dispatch_core_type_from_julia(declared)
}

fn rendered_parametric_array_core(rendered: &str) -> Option<&str> {
    let base = rendered
        .split_once('{')?
        .0
        .rsplit('.')
        .next()
        .unwrap_or(rendered);
    matches!(
        base,
        "Array" | "Vector" | "Matrix" | "AbstractArray" | "AbstractVector" | "AbstractMatrix"
    )
    .then_some(base)
}

/// Check if a string-encoded method signature pattern matches actual runtime
/// type names.
pub fn type_name_pattern_matches(expected_types: &[String], actual_types: &[String]) -> bool {
    if expected_types.len() != actual_types.len() {
        return false;
    }

    let type_params = inferred_type_params_from_expected_names(expected_types);
    let mut bindings = HashMap::new();
    expected_types
        .iter()
        .zip(actual_types.iter())
        .all(|(expected, actual)| {
            let expected_core = embed_type_param_bounds(
                CoreType::from_julia_name_for_dispatch(expected),
                &type_params,
            );
            let actual_core = CoreType::from_julia_name_for_dispatch(actual);
            core_pattern_matches(&expected_core, &actual_core, &mut bindings)
        })
}

fn type_name_pattern_matches_with_subtype_fallback<F>(
    expected_types: &[String],
    actual_types: &[String],
    subtype_matches: &mut F,
) -> bool
where
    F: FnMut(&str, &str) -> bool,
{
    if expected_types.len() != actual_types.len() {
        return false;
    }

    let type_params = inferred_type_params_from_expected_names(expected_types);
    let mut bindings = HashMap::new();
    expected_types
        .iter()
        .zip(actual_types.iter())
        .all(|(expected, actual)| {
            if same_invariant_container_family_concrete_miss(expected, actual) {
                return false;
            }
            let param_ty = JuliaType::from_name_or_struct(expected);
            let arg_ty = JuliaType::from_name_or_struct(actual);
            if julia_type_pattern_matches(&param_ty, &arg_ty, &type_params, &mut bindings) {
                return true;
            }
            if julia_type_mentions_type_params(&param_ty, &type_params) {
                return false;
            }
            if expected.contains("_<:") || expected.contains("<:") {
                covariant_bound_matches(expected, actual, subtype_matches)
            } else if same_invariant_container_family_concrete_miss(expected, actual) {
                false
            } else {
                type_name_pattern_matches(
                    std::slice::from_ref(expected),
                    std::slice::from_ref(actual),
                ) || subtype_matches(actual, expected)
            }
        })
}

fn same_invariant_container_family_concrete_miss(expected: &str, actual: &str) -> bool {
    let expected_core = CoreType::from_julia_name_for_dispatch(expected);
    let actual_core = CoreType::from_julia_name_for_dispatch(actual);
    let (
        CoreType::Struct {
            name: expected_name,
            params: expected_params,
        },
        CoreType::Struct {
            name: actual_name,
            params: actual_params,
        },
    ) = (&expected_core, &actual_core)
    else {
        return false;
    };

    expected_name == actual_name
        && matches!(
            expected_name.as_str(),
            "Array" | "Vector" | "Matrix" | "Dict" | "Set"
        )
        && !expected_params.is_empty()
        && expected_params.len() == actual_params.len()
        && core_typevar_names(&expected_core).is_empty()
}

fn inferred_type_params_from_expected_names(expected_types: &[String]) -> Vec<TypeParam> {
    let mut seen = HashSet::new();
    let mut params = Vec::new();
    for expected in expected_types {
        let core = CoreType::from_julia_name_for_dispatch(expected);
        let mut names = core_typevar_names(&core);
        collect_implicit_pattern_type_params(&core, true, &mut names);
        for name in names {
            if name == "_" || !seen.insert(name.clone()) {
                continue;
            }
            params.push(TypeParam::new(name));
        }
    }
    params
}

fn collect_implicit_pattern_type_params(core: &CoreType, root: bool, names: &mut Vec<String>) {
    match core {
        CoreType::Named(name) if !crate::types::is_registered_type_name(name) && root => {
            names.push(name.clone());
        }
        CoreType::Struct { params, .. } => {
            for param in params {
                collect_implicit_pattern_type_params(param, true, names);
            }
        }
        CoreType::Tuple(elems) | CoreType::Union(elems) => {
            for elem in elems {
                collect_implicit_pattern_type_params(elem, true, names);
            }
        }
        CoreType::Vararg(inner) | CoreType::TypeOf(inner) => {
            collect_implicit_pattern_type_params(inner, true, names);
        }
        CoreType::VarargLen { element, len } => {
            collect_implicit_pattern_type_params(element, true, names);
            collect_implicit_pattern_type_params(len, true, names);
        }
        CoreType::NamedTuple(fields) => {
            for (_, ty) in fields {
                collect_implicit_pattern_type_params(ty, true, names);
            }
        }
        CoreType::UnionAll { var, body } => {
            names.push(var.name.clone());
            collect_implicit_pattern_type_params(body, false, names);
        }
        _ => {}
    }
}

fn covariant_bound_matches<F>(expected: &str, actual: &str, subtype_matches: &mut F) -> bool
where
    F: FnMut(&str, &str) -> bool,
{
    if subtype_matches(actual, expected) {
        return true;
    }

    if let Some(expected_inner) = type_singleton_inner(expected) {
        let Some(bound) = strip_covariant_bound(expected_inner) else {
            return false;
        };
        let Some(actual_inner) = type_singleton_inner(actual) else {
            return false;
        };
        return subtype_matches(actual_inner, bound);
    }

    if expected.contains('{') {
        return false;
    }

    strip_covariant_bound(expected).is_some_and(|bound| subtype_matches(actual, bound))
}

fn strip_covariant_bound(type_name: &str) -> Option<&str> {
    type_name
        .strip_prefix("_<:")
        .or_else(|| type_name.strip_prefix("<:"))
        .or_else(|| type_name.split_once("<:").map(|(_, bound)| bound.trim()))
}

fn type_singleton_inner(type_name: &str) -> Option<&str> {
    type_name
        .strip_prefix("Type{")
        .and_then(|inner| inner.strip_suffix('}'))
}

/// Calculate relative pattern specificity for string-encoded dispatch
/// candidates. Higher is more specific.
pub fn type_name_pattern_specificity(expected_types: &[String]) -> i32 {
    let mut specificity = 0;
    let mut type_var_count = 0;
    let mut same_type_var_bonus = 0;
    let mut seen_type_vars = HashSet::new();
    let type_params = inferred_type_params_from_expected_names(expected_types);

    for expected in expected_types {
        let core = embed_type_param_bounds(
            CoreType::from_julia_name_for_dispatch(expected),
            &type_params,
        );
        let type_vars = core_typevar_names(&core);
        for name in type_vars {
            type_var_count += 1;
            if !seen_type_vars.insert(name) {
                same_type_var_bonus += 100;
            }
        }

        if !matches!(core, CoreType::TypeVar(_)) {
            let param_bonus = i32::from(expected.contains('{'));
            specificity += core.specificity() as i32 + param_bonus;
        }
    }

    specificity - type_var_count + same_type_var_bonus
}

fn type_name_pattern_match_quality(expected_types: &[String], actual_types: &[String]) -> u32 {
    let type_params = inferred_type_params_from_expected_names(expected_types);
    expected_types
        .iter()
        .zip(actual_types.iter())
        .map(|(expected, actual)| {
            let expected_core = embed_type_param_bounds(
                CoreType::from_julia_name_for_dispatch(expected),
                &type_params,
            );
            let actual_core = CoreType::from_julia_name_for_dispatch(actual);
            if expected_core == actual_core {
                2
            } else if expected_core.dispatch_pattern_score(&actual_core) == 3 {
                1
            } else {
                0
            }
        })
        .sum()
}

/// Check if JuliaType method parameters match argument types while tracking
/// `where` type-variable bindings.
pub fn julia_signature_match_with_bindings(
    param_types: &[JuliaType],
    arg_types: &[JuliaType],
    type_params: &[TypeParam],
) -> Option<usize> {
    let mut bindings: HashMap<String, JuliaType> = HashMap::new();

    for (param_ty, arg_ty) in param_types.iter().zip(arg_types.iter()) {
        if !julia_type_pattern_matches(param_ty, arg_ty, type_params, &mut bindings) {
            return None;
        }
    }

    if !bindings.is_empty() && !JuliaType::check_diagonal_rule_for_params(param_types, &bindings) {
        return None;
    }

    Some(bindings.len())
}

/// Match and score a Julia method signature using the shared CoreType scoring
/// policy. `param_types` and `arg_types` must already be arity-normalized for
/// fixed/trailing varargs.
pub fn score_julia_signature(
    param_types: &[JuliaType],
    arg_types: &[JuliaType],
    type_params: &[TypeParam],
    has_varargs: bool,
    fixed_varargs: bool,
) -> Option<JuliaSignatureScore> {
    let binding_count = julia_signature_match_with_bindings(param_types, arg_types, type_params)?;
    Some(score_julia_signature_with_binding_count(
        param_types,
        arg_types,
        binding_count,
        has_varargs,
        fixed_varargs,
    ))
}

pub fn typed_vararg_where_bonus_julia(
    param_types: &[JuliaType],
    type_params: &[TypeParam],
    vararg_param_index: Option<usize>,
) -> u32 {
    if vararg_param_mentions_type_params_julia(param_types, type_params, vararg_param_index) {
        VARARG_TYPE_PARAM_BINDING_BONUS
    } else {
        0
    }
}

pub(crate) fn vararg_param_mentions_type_params_julia(
    param_types: &[JuliaType],
    type_params: &[TypeParam],
    vararg_param_index: Option<usize>,
) -> bool {
    let Some(vararg_idx) = vararg_param_index else {
        return false;
    };
    param_types
        .get(vararg_idx)
        .is_some_and(|param| julia_type_mentions_type_params(param, type_params))
}

/// Score a signature that was matched by a caller-owned fallback.
///
/// This is used by MethodTable's user-defined struct-parent fallback so the
/// fallback keeps its existing matching policy while sharing CoreType scoring.
/// Base specificity of a value-position parameter for method scoring.
///
/// A bounded type variable `x::T where {T<:B}` is as specific as a concrete `B`
/// parameter (in Julia `Tuple{T} where T<:B == Tuple{B}`), so it must outrank an
/// untyped `Any` parameter. `CoreType::specificity()` scores every type variable
/// as 0 (ignoring the bound), and the `type_reuse_bonus` below additionally
/// rewards a parameter that binds no type variable; together those made an
/// untyped fallback out-score a bounded type variable. The `+1` here compensates
/// the single-binding `type_reuse_bonus`, keeping `T<:B` tied with a concrete `B`
/// and strictly above `Any`. An unbounded `T` (≡ `Any`) stays at 0.
///
/// This is intentionally local to value-position scoring: it does not perturb
/// `CoreType::specificity()` itself, so type-position dispatch (`Type{<:B}`
/// patterns, e.g. `eltype(::Type{<:Pairs{K,V,I,A}})`) keeps its existing
/// ordering (Issue #5375).
///
/// The bound's specificity is read from `CoreType::from(ty)`, which derives it
/// from the bound's type name; it is exact for built-in abstract bounds
/// (`Number`, `Real`, `Integer`, ...) used by the reported cases and remains a
/// heuristic for more exotic bounds.
fn value_param_base_specificity(ty: &JuliaType) -> u32 {
    let core = CoreType::from(ty);

    if let CoreType::AbstractUser {
        parent: Some(parent),
        ..
    } = &core
    {
        // Bug #5582 / parent #5072: a user abstract that sits below a built-in
        // abstract, e.g. `AbstractIrrational <: Real`, must outrank that parent.
        // The declared parent is carried structurally on `CoreType::AbstractUser`
        // (Issue #6594: replaces the legacy `JuliaType::from_name(parent)` string
        // re-parse), so the parent boost reads the structured `CoreType` directly
        // and only fires for a parent that resolves to a recognized built-in
        // abstract/concrete type. An `Any` parent, or a parent that names another
        // (unresolved) user abstract — which the legacy `from_name` parse rejected
        // — keeps the flat `AbstractUser` floor.
        if user_abstract_parent_is_boostable(parent) {
            return u32::from(parent.specificity()).saturating_add(1);
        }
        return u32::from(core.specificity());
    }

    if let CoreType::TypeVar(var) = &core {
        if let Some(bound) = &var.upper_bound {
            // `T<:Any` is equivalent to an unbounded `T` (≡ `Any`); it must not
            // outrank an untyped parameter, so keep it at 0.
            if matches!(bound.as_ref(), CoreType::Any) {
                return 0;
            }
            // Floor the bound at 1 so a structurally narrow bound whose
            // `specificity()` collapses to 0 (e.g. `Vector{S}` with a
            // type-variable element) still ranks strictly above an untyped `Any`
            // parameter, then add 1 to compensate the single-binding
            // `type_reuse_bonus`.
            return u32::from(bound.specificity().max(1)).saturating_add(1);
        }
    }
    u32::from(core.specificity())
}

/// Whether a structured `CoreType::AbstractUser` parent resolves to a recognized
/// built-in type whose specificity should boost the user abstract above its
/// parent (Issue #6594). This mirrors the legacy `JuliaType::from_name(parent)`
/// gate structurally: that parse returned `None` for `Any`, for names that map to
/// another (unresolved) user abstract, and for bare type-variable spellings, all
/// of which kept the flat `AbstractUser` floor. Built-in abstracts/concretes
/// (`Number`, `Real`, `Integer`, `AbstractVector`, ...) resolve and contribute
/// their specificity.
fn user_abstract_parent_is_boostable(parent: &CoreType) -> bool {
    !matches!(
        parent,
        CoreType::Any
            | CoreType::Bottom
            | CoreType::Named(_)
            | CoreType::AbstractUser { .. }
            | CoreType::TypeVar(_)
    )
}

pub fn score_julia_signature_with_binding_count(
    param_types: &[JuliaType],
    arg_types: &[JuliaType],
    binding_count: usize,
    has_varargs: bool,
    fixed_varargs: bool,
) -> JuliaSignatureScore {
    let fixed_param_count = param_types.len().min(arg_types.len());
    let base_score: u32 = param_types
        .iter()
        .take(fixed_param_count)
        .map(value_param_base_specificity)
        .sum();

    let match_quality_bonus: i32 = param_types
        .iter()
        .take(fixed_param_count)
        .zip(arg_types.iter().take(fixed_param_count))
        .map(|(param_ty, arg_ty)| {
            let exact_struct_match = matches!(
                (param_ty, arg_ty),
                (JuliaType::Struct(param_name), JuliaType::Struct(arg_name))
                    if param_name == arg_name
            );
            let param_core = CoreType::from(param_ty);
            let arg_core = CoreType::from(arg_ty);
            let pattern_score = param_core.dispatch_pattern_score(&arg_core);
            let exact_bonus_eligible = (param_core.is_builtin_dispatch_primitive()
                && arg_core.is_builtin_dispatch_primitive())
                || (matches!(param_core, CoreType::TypeOf(_))
                    && matches!(arg_core, CoreType::TypeOf(_)))
                || (matches!(param_core, CoreType::Struct { .. })
                    && matches!(arg_core, CoreType::Struct { .. }));

            if exact_struct_match {
                EXACT_PRIMITIVE_MATCH_BONUS
            } else if exact_bonus_eligible {
                if param_core == arg_core {
                    EXACT_PRIMITIVE_MATCH_BONUS
                } else if is_typevar_singleton_match(param_ty, arg_ty) {
                    PARAMETRIC_PATTERN_MATCH_BONUS
                } else if pattern_score == 3 {
                    PARAMETRIC_PATTERN_MATCH_BONUS
                        + i32::from(matches!(param_core, CoreType::TypeOf(_)))
                } else {
                    0
                }
            } else if matches!(arg_core, CoreType::Any) && !matches!(param_core, CoreType::Any) {
                ANY_ARG_SPECIFIC_PARAM_PENALTY
            } else if is_typevar_singleton_match(param_ty, arg_ty) {
                PARAMETRIC_PATTERN_MATCH_BONUS
            } else {
                0
            }
        })
        .sum();

    let score_i32 = (base_score as i32 + match_quality_bonus).max(0);
    let score = u32::try_from(score_i32).unwrap_or(0);
    let type_reuse_bonus = if binding_count < fixed_param_count {
        (fixed_param_count - binding_count) as u32
    } else {
        0
    };
    let score = if has_varargs {
        if fixed_varargs || base_score > 0 {
            score + type_reuse_bonus
        } else {
            score.saturating_sub(1) + type_reuse_bonus
        }
    } else {
        score + type_reuse_bonus
    };

    JuliaSignatureScore {
        binding_count,
        fixed_param_count,
        score,
    }
}

fn is_typevar_singleton_match(param_ty: &JuliaType, arg_ty: &JuliaType) -> bool {
    matches!(
        (param_ty, arg_ty),
        (
            JuliaType::TypeOf(param_inner),
            JuliaType::TypeOf(_),
        ) if matches!(param_inner.as_ref(), JuliaType::TypeVar(_, _))
    )
}

fn type_object_inner_nominal_family_mismatch(param_ty: &JuliaType, arg_ty: &JuliaType) -> bool {
    let Some(param_family) = type_object_inner_nominal_family(param_ty) else {
        return false;
    };
    let Some(arg_family) = type_object_inner_nominal_family(arg_ty) else {
        return false;
    };

    match (&param_family, &arg_family) {
        (
            TypeObjectInnerFamily::Array { rank: param_rank },
            TypeObjectInnerFamily::Array { rank: arg_rank },
        ) => param_rank != arg_rank,
        _ => param_family != arg_family,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeObjectInnerFamily {
    Array { rank: Option<usize> },
    Nominal(String),
}

fn type_object_inner_nominal_family(ty: &JuliaType) -> Option<TypeObjectInnerFamily> {
    match ty {
        JuliaType::UnionAll { body, .. } | JuliaType::RuntimeUnionAll { body, .. } => {
            type_object_inner_nominal_family(body)
        }
        JuliaType::RuntimeParametric { base, params } => {
            let base = base.rsplit('.').next().unwrap_or(base);
            if base == "Array" {
                let rank = params.get(1).and_then(|rank| match rank {
                    JuliaType::Struct(value) => value.parse::<usize>().ok(),
                    _ => None,
                });
                return Some(TypeObjectInnerFamily::Array { rank });
            }
            Some(TypeObjectInnerFamily::Nominal(base.to_string()))
        }
        JuliaType::VectorOf(_) => Some(TypeObjectInnerFamily::Array { rank: Some(1) }),
        JuliaType::MatrixOf(_) => Some(TypeObjectInnerFamily::Array { rank: Some(2) }),
        JuliaType::Array => Some(TypeObjectInnerFamily::Array { rank: None }),
        JuliaType::Struct(name) => {
            let (base, params) = split_nominal_type_name(name);
            let base = base.rsplit('.').next().unwrap_or(base);
            if base == "Array" {
                let rank = params
                    .get(1)
                    .and_then(|rank| rank.trim().parse::<usize>().ok());
                return Some(TypeObjectInnerFamily::Array { rank });
            }
            Some(TypeObjectInnerFamily::Nominal(base.to_string()))
        }
        JuliaType::TypeVar(_, _) | JuliaType::Any => None,
        _ => Some(TypeObjectInnerFamily::Nominal(ty.name().to_string())),
    }
}

fn split_nominal_type_name(name: &str) -> (&str, Vec<&str>) {
    let Some(brace_idx) = name.find('{') else {
        return (name, Vec::new());
    };
    if !name.ends_with('}') {
        return (name, Vec::new());
    }
    let inner = &name[brace_idx + 1..name.len() - 1];
    (&name[..brace_idx], split_top_level_type_args(inner))
}

fn split_top_level_type_args(s: &str) -> Vec<&str> {
    if s.trim().is_empty() {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut brace_depth = 0i32;
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut start = 0;
    for (idx, ch) in s.char_indices() {
        match ch {
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            ',' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                result.push(s[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    result.push(s[start..].trim());
    result
}

fn ntuple_pattern_matches_tuple(
    param_name: &str,
    arg_elems: &[JuliaType],
    type_params: &[TypeParam],
    bindings: &mut HashMap<String, JuliaType>,
    hierarchy: Option<&StructHierarchy>,
) -> Option<bool> {
    let (base, params) = split_nominal_type_name(param_name);
    if base.rsplit('.').next().unwrap_or(base) != "NTuple" {
        return None;
    }
    if !(params.len() == 1 || params.len() == 2) {
        return Some(false);
    }
    if !ntuple_length_slot_matches(params[0], arg_elems.len(), type_params, bindings) {
        return Some(false);
    }

    if let Some(elem_slot) = params.get(1) {
        let elem_pattern = JuliaType::from_parametric_arg(elem_slot.trim());
        return Some(arg_elems.iter().all(|arg| {
            ntuple_element_pattern_matches(&elem_pattern, arg, type_params, bindings, hierarchy)
        }));
    }

    // `NTuple{N}` is `NTuple{N, T} where T`, so the element type is an
    // implicit diagonal variable. It accepts homogeneous tuples such as
    // `Tuple{Float64, Float64}` and rejects mixed tuples such as
    // `Tuple{Float64, Int64}`.
    Some(
        arg_elems
            .split_first()
            .is_none_or(|(first, rest)| rest.iter().all(|arg| arg == first)),
    )
}

fn ntuple_element_pattern_matches(
    elem_pattern: &JuliaType,
    arg: &JuliaType,
    type_params: &[TypeParam],
    bindings: &mut HashMap<String, JuliaType>,
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    let mut trial_bindings = bindings.clone();
    if julia_type_pattern_matches(elem_pattern, arg, type_params, &mut trial_bindings) {
        *bindings = trial_bindings;
        return true;
    }

    let Some(hierarchy) = hierarchy else {
        return false;
    };

    let mut trial_bindings = bindings.clone();
    if runtime_value_type_matches_param_with_bindings(
        hierarchy,
        arg,
        None,
        elem_pattern,
        type_params,
        &mut trial_bindings,
        || false,
    ) {
        *bindings = trial_bindings;
        return true;
    }
    false
}

fn ntuple_length_slot_matches(
    slot: &str,
    actual_len: usize,
    type_params: &[TypeParam],
    bindings: &mut HashMap<String, JuliaType>,
) -> bool {
    let slot = slot.trim();
    if slot == "_" {
        return true;
    }
    if let Ok(expected_len) = slot.parse::<usize>() {
        return expected_len == actual_len;
    }
    if find_type_param(type_params, slot).is_some() {
        let len_ty = JuliaType::Struct(actual_len.to_string());
        return bind_or_check_julia_type_var(slot, None, &len_ty, bindings);
    }
    false
}

fn julia_type_pattern_matches(
    param_ty: &JuliaType,
    arg_ty: &JuliaType,
    type_params: &[TypeParam],
    bindings: &mut HashMap<String, JuliaType>,
) -> bool {
    let param_core = dispatch_core_type_from_julia(param_ty);
    let arg_core = dispatch_core_type_from_julia(arg_ty);
    if core_match::dispatch_has_explicit_sibling_owner_conflict(&arg_core, &param_core) {
        return false;
    }

    if let (JuliaType::TupleOf(param_elems), JuliaType::TupleOf(arg_elems)) = (param_ty, arg_ty) {
        // Trailing unbounded `Vararg{T}` pattern: match leading slots
        // positionally, then match every remaining argument element against the
        // vararg element type (Issue #4857).
        if let Some(last) = param_elems.last() {
            if let Some(vararg_elem) = crate::types::unbounded_vararg_element(last) {
                let lead_count = param_elems.len() - 1;
                if arg_elems.len() < lead_count {
                    return false;
                }
                let leads_ok =
                    param_elems[..lead_count]
                        .iter()
                        .zip(arg_elems.iter())
                        .all(|(param, arg)| {
                            julia_type_pattern_matches(param, arg, type_params, bindings)
                        });
                if !leads_ok {
                    return false;
                }
                return arg_elems[lead_count..].iter().all(|arg| {
                    julia_type_pattern_matches(&vararg_elem, arg, type_params, bindings)
                });
            }
        }
        return param_elems.len() == arg_elems.len()
            && param_elems
                .iter()
                .zip(arg_elems.iter())
                .all(|(param, arg)| julia_type_pattern_matches(param, arg, type_params, bindings));
    }
    if let (JuliaType::Struct(param_name), JuliaType::TupleOf(arg_elems)) = (param_ty, arg_ty) {
        if let Some(matches) =
            ntuple_pattern_matches_tuple(param_name, arg_elems, type_params, bindings, None)
        {
            return matches;
        }
    }

    if let JuliaType::TypeVar(var_name, bound) = param_ty {
        let type_param = find_type_param(type_params, var_name);
        let upper = usable_upper_bound(bound.as_deref())
            .or_else(|| type_param.and_then(type_param_upper_bound));
        if var_name == "_" {
            return upper.is_none_or(|bound_name| {
                core_is_subtype(
                    &CoreType::from(arg_ty),
                    &CoreType::from_julia_name_for_dispatch(bound_name),
                )
            });
        }
        if let Some(bound_pattern) = parametric_typevar_bound_pattern(upper, type_params) {
            return julia_type_pattern_matches(&bound_pattern, arg_ty, type_params, bindings)
                && bind_or_check_julia_type_var(var_name, None, arg_ty, bindings);
        }
        return bind_or_check_julia_type_var(var_name, upper, arg_ty, bindings);
    }
    if let JuliaType::Struct(var_name) = param_ty {
        if let Some(type_param) = find_type_param(type_params, var_name) {
            let upper = type_param_upper_bound(type_param);
            if let Some(bound_pattern) = parametric_typevar_bound_pattern(upper, type_params) {
                return julia_type_pattern_matches(&bound_pattern, arg_ty, type_params, bindings)
                    && bind_or_check_julia_type_var(var_name, None, arg_ty, bindings);
            }
            return bind_or_check_julia_type_var(var_name, upper, arg_ty, bindings);
        }
        // Issue #5314: `var_name` is NOT a method type parameter, so `::var_name`
        // names a concrete (possibly parametric) struct type. A struct is a final
        // leaf type, so a primitive argument (`Float64`, `Int64`, ...) can never
        // be a subtype of it. Reject it here instead of falling through to broad
        // parametric subtype checks;
        // without this, adding a `min(::Q, ::Q)` method broke `min(1.0, 2.0)`
        // (AmbiguousMethod) and made `oneunit(3.0)` mis-dispatch a `Float64` into
        // the struct method. Non-primitive arguments (tuples, value-parameter
        // bindings, ...) keep their existing matching so parametric value
        // parameters are unaffected.
        if arg_ty.is_primitive() {
            return false;
        }
    }

    if let JuliaType::TypeOf(inner_param) = param_ty {
        if let JuliaType::TypeOf(inner_arg) = arg_ty {
            // `Type{T}` binds `T` invariantly to the argument, so both the
            // upper and lower bounds of `T` are enforced here (Issue #5051).
            if let JuliaType::TypeVar(var_name, bound) = inner_param.as_ref() {
                let type_param = find_type_param(type_params, var_name);
                return bind_or_check_julia_type_var_bounded(
                    var_name,
                    usable_upper_bound(bound.as_deref())
                        .or_else(|| type_param.and_then(type_param_upper_bound)),
                    type_param.and_then(type_param_lower_bound),
                    inner_arg.as_ref(),
                    bindings,
                );
            }
            if let JuliaType::Struct(var_name) = inner_param.as_ref() {
                if let Some(type_param) = find_type_param(type_params, var_name) {
                    return bind_or_check_julia_type_var_bounded(
                        var_name,
                        type_param_upper_bound(type_param),
                        type_param_lower_bound(type_param),
                        inner_arg.as_ref(),
                        bindings,
                    );
                }
            }
            if julia_type_mentions_type_params(inner_param, type_params) {
                if type_object_inner_nominal_family_mismatch(inner_param, inner_arg.as_ref()) {
                    return false;
                }
                if let Some(extracted) = inner_arg.extract_type_bindings(inner_param, type_params) {
                    return extracted.into_iter().all(|(name, bound_ty)| {
                        bind_or_check_julia_type_var(&name, None, &bound_ty, bindings)
                    });
                }
            }
            if matches!(inner_param.as_ref(), JuliaType::Any) {
                return matches!(inner_arg.as_ref(), JuliaType::Any);
            }
            return inner_arg.as_ref() == inner_param.as_ref();
        }
    }

    if matches!(arg_ty, JuliaType::TypeOf(_))
        && !matches!(
            param_ty,
            JuliaType::Any | JuliaType::Type | JuliaType::DataType | JuliaType::TypeOf(_)
        )
    {
        return false;
    }

    // Nested diagonal binding (Issue #5050): a parametric parameter such as
    // `x::Vector{T}` mentions a `where` type variable below the top level. The
    // structural cases above only bind a variable that sits at the very top of
    // the parameter type, so without this branch the inner `T` from `Vector{T}`
    // is never recorded in the shared binding map and a later `y::T` could not
    // enforce the diagonal rule.
    //
    // The match decision still rests on the existing subtype check below — we
    // only *record* the inner binding(s) so that a repeated occurrence of the
    // same variable is later rejected by `bind_or_check_julia_type_var` (and so
    // the post-match `check_diagonal_rule_for_params` can see them). The binding
    // is only recorded when the argument actually subtypes the parameter, so we
    // never change which arguments a parameter accepts in isolation.
    if julia_type_mentions_type_params(param_ty, type_params)
        && arg_ty.is_subtype_of_parametric(param_ty, type_params)
    {
        if let Some(extracted) = arg_ty.extract_type_bindings(param_ty, type_params) {
            return extracted.into_iter().all(|(name, bound_ty)| {
                let upper = find_type_param(type_params, &name).and_then(type_param_upper_bound);
                bind_or_check_julia_type_var(&name, upper, &bound_ty, bindings)
            });
        }
        return !same_family_parametric_binding_definitely_rejected(param_ty, arg_ty, type_params);
    }

    arg_ty.is_subtype_of_parametric(param_ty, type_params)
}

fn same_family_parametric_binding_definitely_rejected(
    param_ty: &JuliaType,
    arg_ty: &JuliaType,
    type_params: &[TypeParam],
) -> bool {
    let (JuliaType::Struct(param_name), JuliaType::Struct(arg_name)) = (param_ty, arg_ty) else {
        return false;
    };
    if !param_name.contains('{') || !arg_name.contains('{') {
        return false;
    }
    let (param_base, param_slots) = super::type_core::parse_parametric_type_name(param_name);
    let (arg_base, arg_slots) = super::type_core::parse_parametric_type_name(arg_name);
    if strip_parametric_module_prefix(param_base) != strip_parametric_module_prefix(arg_base)
        || arg_slots.len() < param_slots.len()
    {
        return false;
    }

    param_slots
        .iter()
        .zip(arg_slots.iter())
        .any(|(param_slot, arg_slot)| {
            let param_slot = param_slot.trim();
            let Some(type_param) = type_params.iter().find(|tp| tp.name == param_slot) else {
                return false;
            };
            let Some(upper) = type_param_upper_bound(type_param) else {
                return false;
            };
            let bound_core = CoreType::from_julia_name_for_dispatch(upper);
            if bound_is_undecidable_user_type(&bound_core) {
                return false;
            }
            let arg_core = CoreType::from(&JuliaType::from_name_or_struct(arg_slot.trim()));
            !core_is_subtype(&arg_core, &bound_core)
        })
}

fn strip_parametric_module_prefix(name: &str) -> &str {
    let base = name.split('{').next().unwrap_or(name);
    base.rfind('.').map_or(base, |idx| &base[idx + 1..])
}

fn bound_is_undecidable_user_type(bound: &CoreType) -> bool {
    match bound {
        CoreType::Named(_) | CoreType::AbstractUser { .. } => true,
        CoreType::Union(members) => members.iter().any(bound_is_undecidable_user_type),
        CoreType::UnionAll { var, body } => {
            var.upper_bound
                .as_deref()
                .is_some_and(bound_is_undecidable_user_type)
                || bound_is_undecidable_user_type(body)
        }
        CoreType::TypeVar(var) => var
            .upper_bound
            .as_deref()
            .is_some_and(bound_is_undecidable_user_type),
        CoreType::TypeOf(inner) | CoreType::Vararg(inner) => bound_is_undecidable_user_type(inner),
        CoreType::VarargLen { element, len } => {
            bound_is_undecidable_user_type(element) || bound_is_undecidable_user_type(len)
        }
        CoreType::Struct { params, .. } | CoreType::Tuple(params) => {
            params.iter().any(bound_is_undecidable_user_type)
        }
        CoreType::NamedTuple(fields) => fields
            .iter()
            .any(|(_, field_ty)| bound_is_undecidable_user_type(field_ty)),
        CoreType::Bottom
        | CoreType::Any
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::Value(_)
        | CoreType::Module(_) => false,
    }
}

fn core_pattern_matches(
    expected: &CoreType,
    actual: &CoreType,
    bindings: &mut LexicalTypeBindings,
) -> bool {
    match expected {
        CoreType::TypeVar(var) => bind_or_check_core_type_var(var, actual, bindings),
        CoreType::Named(name) if name.starts_with("_<:") => {
            let bound = CoreType::from_julia_name_for_dispatch(name.trim_start_matches("_<:"));
            core_is_subtype(actual, &bound)
        }
        CoreType::Struct { name, params } => {
            let CoreType::Struct {
                name: actual_name,
                params: actual_params,
            } = actual
            else {
                return core_is_subtype(actual, expected);
            };
            if !struct_family_matches(name, actual_name) {
                return false;
            }
            params.is_empty()
                || (params.len() == actual_params.len()
                    && params
                        .iter()
                        .zip(actual_params.iter())
                        .all(|(param, actual)| {
                            if core_typevar_names(param).is_empty() {
                                param == actual
                            } else {
                                core_pattern_matches(param, actual, bindings)
                            }
                        }))
        }
        CoreType::Tuple(expected_elements) => {
            let CoreType::Tuple(actual_elements) = actual else {
                return false;
            };
            expected_elements.len() == actual_elements.len()
                && expected_elements
                    .iter()
                    .zip(actual_elements.iter())
                    .all(|(expected, actual)| core_pattern_matches(expected, actual, bindings))
        }
        CoreType::TypeOf(expected_inner) => {
            let CoreType::TypeOf(actual_inner) = actual else {
                return false;
            };
            match expected_inner.as_ref() {
                CoreType::TypeVar(_) => {
                    core_pattern_matches(expected_inner, actual_inner, bindings)
                }
                CoreType::Named(name) if name.starts_with("_<:") => {
                    core_pattern_matches(expected_inner, actual_inner, bindings)
                }
                _ if !core_typevar_names(expected_inner).is_empty() => {
                    core_pattern_matches(expected_inner, actual_inner, bindings)
                }
                _ => expected_inner == actual_inner,
            }
        }
        _ => expected == actual || core_is_subtype(actual, expected),
    }
}

fn core_is_subtype(actual: &CoreType, expected: &CoreType) -> bool {
    core_match::dispatch_core_is_subtype(actual, expected)
}

fn bind_or_check_core_type_var(
    var: &CoreTypeVar,
    actual: &CoreType,
    bindings: &mut LexicalTypeBindings,
) -> bool {
    if let Some(lower_bound) = &var.lower_bound {
        if !core_is_subtype(lower_bound, actual) {
            return false;
        }
    }
    if let Some(upper_bound) = &var.upper_bound {
        if !core_is_subtype(actual, upper_bound) {
            return false;
        }
    }

    let var_name = &var.name;
    if var_name == "_" {
        return true;
    }
    if let Some(existing) = bindings.get(var_name) {
        actual == existing
    } else {
        bindings.insert(var_name.clone(), actual.clone());
        true
    }
}

fn bind_or_check_julia_type_var(
    var_name: &str,
    bound: Option<&str>,
    arg_ty: &JuliaType,
    bindings: &mut HashMap<String, JuliaType>,
) -> bool {
    bind_or_check_julia_type_var_bounded(var_name, bound, None, arg_ty, bindings)
}

/// Bind/check a `where` type variable against an argument type, enforcing both
/// the upper and (optional) lower bound.
///
/// The lower bound (`lower <: arg`) is only meaningful in invariant positions
/// such as `Type{T}` where `T` is bound to the argument exactly. In covariant
/// positions (`x::T`) Julia widens `T` to a union that absorbs the declared
/// lower bound, so callers there must pass `lower = None` (Issue #5051).
fn bind_or_check_julia_type_var_bounded(
    var_name: &str,
    upper: Option<&str>,
    lower: Option<&str>,
    arg_ty: &JuliaType,
    bindings: &mut HashMap<String, JuliaType>,
) -> bool {
    let arg_core = CoreType::from(arg_ty);
    if let Some(bound_name) = usable_upper_bound(upper) {
        let bound_core = CoreType::from_julia_name_for_dispatch(bound_name);
        if !core_is_subtype(&arg_core, &bound_core) {
            return false;
        }
    }
    if let Some(lower_name) = lower {
        let lower_core = CoreType::from_julia_name_for_dispatch(lower_name);
        if !core_is_subtype(&lower_core, &arg_core) {
            return false;
        }
    }

    if let Some(existing) = bindings.get(var_name) {
        arg_ty == existing
    } else {
        bindings.insert(var_name.to_string(), arg_ty.clone());
        true
    }
}

fn find_type_param<'a>(type_params: &'a [TypeParam], var_name: &str) -> Option<&'a TypeParam> {
    type_params
        .iter()
        .find(|tp| type_param_base_name(&tp.name) == var_name)
}

fn type_param_upper_bound(type_param: &TypeParam) -> Option<&str> {
    type_param
        .get_upper_bound()
        .map(String::as_str)
        .and_then(|bound| usable_upper_bound(Some(bound)))
        .or_else(|| {
            type_param
                .name
                .contains("<:")
                .then(|| usable_upper_bound(Some(&type_param.name)))
                .flatten()
        })
}

/// The declared lower bound of a `where` type parameter (`Lower<:T` or
/// `Lower<:T<:Upper`), if any. Used to enforce `Lower <: arg` in invariant
/// positions such as `Type{T}` (Issue #5051).
fn type_param_lower_bound(type_param: &TypeParam) -> Option<&str> {
    type_param
        .lower_bound
        .as_deref()
        .map(str::trim)
        .filter(|lower| !lower.is_empty())
}

fn type_param_base_name(name: &str) -> &str {
    name.split_once("<:")
        .or_else(|| name.split_once(">:"))
        .map_or(name, |(base, _)| base)
        .trim()
}

fn upper_bound_type_name(bound: &str) -> &str {
    bound
        .rsplit_once("<:")
        .map_or(bound, |(_, upper)| upper)
        .trim()
}

fn usable_upper_bound(bound: Option<&str>) -> Option<&str> {
    let normalized = upper_bound_type_name(bound?);
    (!normalized.is_empty() && normalized != "<:").then_some(normalized)
}

/// When a `where` type variable's upper bound is itself a *parametric* type that
/// mentions another `where` variable — `T<:Vector{S}` with `S<:Number` — the
/// covariant parameter `x::T` is equivalent to `x::Vector{S}`. Parse such a
/// bound into a pattern so the argument can be matched structurally (binding the
/// inner variable and enforcing its own bound) via the existing parametric path,
/// instead of an opaque `from_julia_name` subtype check that drops `S<:Number`
/// and rejects the concrete element (Issue #5383, sub-case 2).
///
/// Returns `None` for non-parametric bounds (`T<:Number`) and for parametric
/// bounds with no inner type variable (`T<:Vector{Int64}`), both of which the
/// ordinary bound check already handles correctly.
fn parametric_typevar_bound_pattern(
    upper: Option<&str>,
    type_params: &[TypeParam],
) -> Option<JuliaType> {
    let bound = upper?;
    if !bound.contains('{') {
        return None;
    }
    let parsed = JuliaType::from_name_or_struct(bound);
    julia_type_mentions_type_params(&parsed, type_params).then_some(parsed)
}

/// Whether the actual struct base name belongs to the expected pattern's
/// nominal family. The membership decision is delegated to the shared
/// subtype engine instead of a hand-rolled alias list (Issue #5915):
/// `is_subtype_by_name("Vector", "Array")` is true, `"BitVector"` /
/// `"Dict"` are not. The rank-erasing direction (`expected == "Vector"`,
/// `actual == "Array"`) must stay false as in upstream Julia
/// (`Array <: Vector` is false because the rank is not fixed), but the
/// engine's bare-name query is existentially loose there, so only the
/// fixed-rank-erased `Array` family question is delegated.
fn struct_family_matches(expected: &str, actual: &str) -> bool {
    expected == actual
        || (expected == "Array" && CoreSubtypeEngine::new().is_subtype_by_name(actual, expected))
}

fn core_typevar_names(core: &CoreType) -> Vec<String> {
    match core {
        CoreType::TypeVar(var) => vec![var.name.clone()],
        CoreType::Struct { params, .. } | CoreType::Tuple(params) | CoreType::Union(params) => {
            params.iter().flat_map(core_typevar_names).collect()
        }
        CoreType::Vararg(inner) | CoreType::TypeOf(inner) => core_typevar_names(inner),
        CoreType::VarargLen { element, len } => {
            let mut names = core_typevar_names(element);
            names.extend(core_typevar_names(len));
            names
        }
        CoreType::NamedTuple(fields) => fields
            .iter()
            .flat_map(|(_, ty)| core_typevar_names(ty))
            .collect(),
        CoreType::UnionAll { var, body } => {
            let mut names = vec![var.name.clone()];
            names.extend(core_typevar_names(body));
            names
        }
        CoreType::Bottom
        | CoreType::Any
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::AbstractUser { .. }
        | CoreType::Value(_)
        | CoreType::Module(_)
        | CoreType::Named(_) => vec![],
    }
}

fn core_type_has_previously_bound_typevars(
    core: &CoreType,
    bindings: &LexicalTypeBindings,
) -> bool {
    core_typevar_names(core)
        .iter()
        .any(|name| name.as_str() != "_" && bindings.contains_key(name))
}

/// Runtime (value-side) twin of [`julia_type_pattern_matches`]: match one call
/// argument's runtime type against a declared parameter type while tracking
/// `where` type-variable bindings (Issue #5915).
///
/// The VM derives `arg_value_type` from the argument value
/// (`Vm::get_value_julia_type`) and passes the type-object payload when the
/// argument is a first-class type (`Value::DataType`). Judgments that need the
/// runtime *value* representation (native array wrappers, dict payloads, ...)
/// stay VM-owned behind `value_fallback`; everything binding-aware lives here,
/// with the `<:` legs engine-backed through `JuliaType::is_subtype_of` (which
/// delegates to the shared `CoreSubtypeEngine`).
///
/// This intentionally preserves the historical runtime matcher semantics
/// (previously the private `Vm::value_matches_param_with_bindings`); merging it
/// with the compile-side [`julia_type_pattern_matches`] is the remaining #6502
/// unification step.
pub fn runtime_value_type_matches_param_with_bindings(
    hierarchy: &StructHierarchy,
    arg_value_type: &JuliaType,
    arg_type_object: Option<&JuliaType>,
    param_ty: &JuliaType,
    type_params: &[TypeParam],
    bindings: &mut HashMap<String, JuliaType>,
    value_fallback: impl FnOnce() -> bool,
) -> bool {
    if let JuliaType::TypeVar(var_name, bound) = param_ty {
        let type_param = specificity::find_type_param(type_params, var_name);
        return bind_or_check_runtime_type_var(
            hierarchy,
            var_name,
            specificity::usable_upper_bound(bound.as_deref())
                .or_else(|| type_param.and_then(specificity::type_param_upper_bound)),
            arg_value_type,
            bindings,
        );
    }
    if let (JuliaType::Struct(param_name), JuliaType::TupleOf(arg_elems)) =
        (param_ty, arg_value_type)
    {
        if let Some(matches) = ntuple_pattern_matches_tuple(
            param_name,
            arg_elems,
            type_params,
            bindings,
            Some(hierarchy),
        ) {
            return matches;
        }
    }
    if let JuliaType::Struct(var_name) = param_ty {
        if let Some(type_param) = specificity::find_type_param(type_params, var_name) {
            return bind_or_check_runtime_type_var(
                hierarchy,
                var_name,
                specificity::type_param_upper_bound(type_param),
                arg_value_type,
                bindings,
            );
        }
        if runtime_concrete_leaf_struct_param(hierarchy, var_name) {
            return matches!(
                arg_value_type,
                JuliaType::Struct(arg_name)
                    if runtime_same_concrete_struct_name(arg_name, var_name)
            );
        }
    }

    if let (Some(dt), JuliaType::TypeOf(inner)) = (arg_type_object, param_ty) {
        if let JuliaType::TypeVar(var_name, bound) = inner.as_ref() {
            let type_param = specificity::find_type_param(type_params, var_name);
            return bind_or_check_runtime_type_var(
                hierarchy,
                var_name,
                specificity::usable_upper_bound(bound.as_deref())
                    .or_else(|| type_param.and_then(specificity::type_param_upper_bound)),
                dt,
                bindings,
            );
        }
        if let JuliaType::Struct(var_name) = inner.as_ref() {
            if let Some(type_param) = specificity::find_type_param(type_params, var_name) {
                return bind_or_check_runtime_type_var(
                    hierarchy,
                    var_name,
                    specificity::type_param_upper_bound(type_param),
                    dt,
                    bindings,
                );
            }
        }
        if runtime_julia_type_contains_type_var(inner)
            || runtime_julia_type_mentions_type_params(inner, type_params)
            || runtime_julia_type_needs_array_projection_match(inner)
        {
            if let Some(extracted) = dt.extract_type_bindings_in(inner, type_params, hierarchy) {
                if !extracted_bindings_cover_mentioned_type_params(inner, type_params, &extracted) {
                    return false;
                }
                for (var_name, bound_type) in extracted {
                    let Some(type_param) = specificity::find_type_param(type_params, &var_name)
                    else {
                        continue;
                    };
                    if !bind_or_check_runtime_type_var(
                        hierarchy,
                        &var_name,
                        specificity::type_param_upper_bound(type_param),
                        &bound_type,
                        bindings,
                    ) {
                        return false;
                    }
                }
                return true;
            }
        }
        return dt == inner.as_ref();
    }

    if runtime_julia_type_contains_type_var(param_ty)
        || runtime_julia_type_mentions_type_params(param_ty, type_params)
        || runtime_julia_type_needs_array_projection_match(param_ty)
    {
        // A first-class type argument dispatches as `Type{T}` (the runtime
        // analogue of `Vm::dispatch_julia_type_for_value`); plain values use
        // their derived runtime type.
        let type_object_dispatch_type;
        let arg_jtype = if let Some(dt) = arg_type_object {
            type_object_dispatch_type = JuliaType::TypeOf(Box::new(dt.clone()));
            &type_object_dispatch_type
        } else {
            arg_value_type
        };
        let Some(extracted) = arg_jtype.extract_type_bindings_in(param_ty, type_params, hierarchy)
        else {
            return false;
        };
        for (var_name, bound_type) in extracted {
            let Some(type_param) = specificity::find_type_param(type_params, &var_name) else {
                continue;
            };
            if !bind_or_check_runtime_type_var(
                hierarchy,
                &var_name,
                specificity::type_param_upper_bound(type_param),
                &bound_type,
                bindings,
            ) {
                return false;
            }
        }
        return true;
    }

    value_fallback()
}

fn extracted_bindings_cover_mentioned_type_params(
    pattern: &JuliaType,
    type_params: &[TypeParam],
    extracted: &HashMap<String, JuliaType>,
) -> bool {
    let mentions_method_param = type_params
        .iter()
        .any(|tp| pattern.mentions_free_var(specificity::type_param_base_name(&tp.name)));
    !mentions_method_param || !extracted.is_empty()
}

/// Bind/check a `where` type variable against a runtime-derived argument type.
///
/// Runtime flavor of [`bind_or_check_julia_type_var`]: the upper bound is
/// enforced by the shared [`CoreSubtypeEngine`] with the VM's
/// [`StructHierarchy`], so user-defined abstract bounds no longer fall back to
/// the older `JuliaType::from_name`-only gate.
fn bind_or_check_runtime_type_var(
    hierarchy: &StructHierarchy,
    var_name: &str,
    bound: Option<&str>,
    arg_ty: &JuliaType,
    bindings: &mut HashMap<String, JuliaType>,
) -> bool {
    if let Some(bound_name) = specificity::usable_upper_bound(bound) {
        let arg_core = CoreType::from(arg_ty);
        let bound_core = CoreType::from_julia_name_for_dispatch(bound_name);
        if !core_match::dispatch_core_is_subtype_with_hierarchy(&arg_core, &bound_core, hierarchy) {
            return false;
        }
    }

    if var_name == "_" {
        return true;
    }

    if let Some(existing) = bindings.get(var_name) {
        arg_ty == existing
    } else {
        bindings.insert(var_name.to_string(), arg_ty.clone());
        true
    }
}

/// Structural scan for an unbound `TypeVar` anywhere inside a declared
/// parameter type (runtime matcher gate; also used by the VM's tuple
/// `type_matches` to decide between pure-subtype and wildcard matching).
pub fn runtime_julia_type_contains_type_var(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::TypeVar(_, _) => true,
        JuliaType::TupleOf(types) | JuliaType::Union(types) => {
            types.iter().any(runtime_julia_type_contains_type_var)
        }
        JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) | JuliaType::TypeOf(inner) => {
            runtime_julia_type_contains_type_var(inner)
        }
        JuliaType::UnionAll { body, .. } => runtime_julia_type_contains_type_var(body),
        JuliaType::Struct(_) => {
            let core = CoreType::from(ty);
            !core_typevar_names(&core).is_empty() || core_type_pattern_has_explicit_bound(&core)
        }
        _ => false,
    }
}

/// Whether `ty` mentions any of the method's `where` parameters as a free
/// variable. Unlike the compile-side [`julia_type_mentions_type_params`] this
/// uses `JuliaType::mentions_free_var` (whole-token struct-parameter scan with
/// `UnionAll` binder shadowing) — the historical runtime matcher gate.
fn runtime_julia_type_mentions_type_params(ty: &JuliaType, type_params: &[TypeParam]) -> bool {
    if matches!(ty, JuliaType::AbstractUser(name, _) if !name.contains('{')) {
        return false;
    }
    type_params
        .iter()
        .any(|tp| ty.mentions_free_var(specificity::type_param_base_name(&tp.name)))
}

fn runtime_concrete_leaf_struct_param(hierarchy: &StructHierarchy, name: &str) -> bool {
    !name.contains('{')
        && hierarchy
            .entry(name)
            .is_some_and(|entry| entry.type_params().is_empty())
}

fn runtime_same_concrete_struct_name(actual: &str, expected: &str) -> bool {
    let actual_base = actual.split('{').next().unwrap_or(actual);
    let expected_base = expected.split('{').next().unwrap_or(expected);
    if actual_base.contains('.') && expected_base.contains('.') {
        return actual_base == expected_base;
    }
    actual_base.rsplit('.').next().unwrap_or(actual_base)
        == expected_base.rsplit('.').next().unwrap_or(expected_base)
}

/// Whether the declared parameter is an array-shaped pattern that must be
/// matched through `extract_type_bindings` (projecting the runtime array type
/// onto `Vector{T}` / `Matrix{T}` / `AbstractVector{T}` / `AbstractMatrix{T}`)
/// even when it binds no type variable.
fn runtime_julia_type_needs_array_projection_match(ty: &JuliaType) -> bool {
    matches!(ty, JuliaType::VectorOf(_) | JuliaType::MatrixOf(_))
        || (matches!(ty, JuliaType::Struct(_))
            && (specificity::abstract_vector_param_type(ty).is_some()
                || specificity::abstract_matrix_param_type(ty).is_some()))
}

/// Runtime single-argument matcher between a rendered runtime type name and a
/// declared parameter `JuliaType` (Issue #5915): the matching policy of the
/// VM's typed dynamic dispatch (`Vm::check_type_match`), centralized next to
/// the other shared matchers.
///
/// The VM supplies `is_known_struct_base` (declared-struct lookup for the
/// Issue #5314 leaf-struct guard) and `subtype_by_name` (the engine-backed
/// runtime `<:` authority, `Vm::check_subtype`).
pub fn runtime_type_name_matches_param(
    arg_type_name: &str,
    param_jt: &JuliaType,
    is_known_struct_base: impl FnOnce(&str) -> bool,
    subtype_by_name: impl FnOnce(&str, &str) -> bool,
) -> bool {
    // Any parameter type matches any argument.
    if matches!(param_jt, JuliaType::Any) {
        return true;
    }

    let param_type_name = param_jt.name();

    // Exact match.
    if arg_type_name == param_type_name.as_ref() {
        return true;
    }

    // Issue #5314: when the parameter names a known, concrete (non-parametric)
    // struct, only an argument of that same struct may match. A struct is a
    // final leaf type with no subtypes, so a primitive argument (`Int64`,
    // `Float64`, ...) must be rejected here. Otherwise the later structural
    // fallback can treat the opaque name too broadly and match *any* argument, making a
    // dynamic `min(a.I, b.I)` (untyped fields) mis-dispatch `Int64` values into
    // the `min(::Q, ::Q)` method.
    if let JuliaType::Struct(name) = param_jt {
        if !name.contains('{') {
            let param_base = name.rsplit('.').next().unwrap_or(name);
            if !runtime_leaf_struct_guard_exempt(param_base) && is_known_struct_base(param_base) {
                return crate::types::nominal_family_names_compatible(arg_type_name, name);
            }
        }
    }

    let arg_core = CoreType::from_julia_name_for_dispatch(arg_type_name);
    let param_core = CoreType::from(param_jt);
    if matches!(param_jt, JuliaType::Function)
        && runtime_function_singleton_matches(arg_type_name, &arg_core, &param_core)
    {
        return true;
    }
    if param_core.dispatch_pattern_score(&arg_core) > 0 {
        return true;
    }

    subtype_by_name(arg_type_name, &param_type_name)
}

fn runtime_leaf_struct_guard_exempt(param_base: &str) -> bool {
    matches!(
        param_base,
        "Array"
            | "Vector"
            | "Matrix"
            | "AbstractArray"
            | "AbstractVector"
            | "AbstractMatrix"
            | "DenseArray"
    )
}

fn runtime_function_singleton_matches(
    arg_type_name: &str,
    arg_core: &CoreType,
    param_core: &CoreType,
) -> bool {
    arg_core.is_subtype_of(param_core)
        || (arg_type_name.starts_with("typeof(") && arg_type_name.ends_with(')'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn strings(types: &[&str]) -> Vec<String> {
        types.iter().map(|ty| (*ty).to_string()).collect()
    }

    #[test]
    fn call_request_preserves_qualified_identity_context_and_candidates_10461() {
        let request = CallRequest {
            callee: CalleeIdentity::from_function_name("Pkg.Sub.f"),
            positional: vec![CoreType::Struct {
                name: "Pkg.Box".to_string(),
                params: vec![CoreType::Primitive(CorePrimitive::Int64)],
            }],
            keywords: vec![KeywordArg {
                name: "op".to_string(),
                ty: CoreType::Named("typeof(Base.sqrt)".to_string()),
                origin: KeywordOrigin::Default,
            }],
            lexical_scope: LexicalScopeId {
                module: vec!["Pkg".to_string(), "Sub".to_string()],
                method: Some(MethodId(7)),
            },
            world: 42,
            call_span: Span::new(10, 14, 2, 2, 3, 7),
            candidates: CandidateSet(vec![MethodId(11), MethodId(12)]),
        };

        assert_eq!(
            request.callee,
            CalleeIdentity::GenericFunction {
                owner: vec!["Pkg".to_string(), "Sub".to_string()],
                name: "f".to_string(),
            }
        );
        assert_eq!(request.world, 42);
        assert_eq!(request.call_span.start, 10);
        assert_eq!(request.candidates.0, vec![MethodId(11), MethodId(12)]);
        assert!(matches!(
            request.positional.as_slice(),
            [CoreType::Struct { name, params }]
                if name == "Pkg.Box"
                    && params == &[CoreType::Primitive(CorePrimitive::Int64)]
        ));
    }

    #[test]
    fn call_resolver_comparison_reports_target_or_binding_difference_10461() {
        let selected = ResolvedCall::JuliaMethod {
            method: MethodId(3),
            bindings: TypeBindings::Complete(Vec::new()),
        };
        assert!(!call_resolutions_differ(&selected, &selected));
        assert!(call_resolutions_differ(
            &selected,
            &ResolvedCall::JuliaMethod {
                method: MethodId(4),
                bindings: TypeBindings::Complete(Vec::new()),
            }
        ));
        assert!(call_resolutions_differ(
            &selected,
            &ResolvedCall::JuliaMethod {
                method: MethodId(3),
                bindings: TypeBindings::NotObserved,
            }
        ));
    }

    fn candidate_pair_is_order_independent<C, R>(
        candidates: [C; 2],
        mut resolve: impl FnMut([C; 2]) -> R,
    ) -> bool
    where
        C: Copy,
        R: PartialEq,
    {
        resolve(candidates) == resolve([candidates[1], candidates[0]])
    }

    fn assert_candidate_pair_order_independent<C, R>(
        adapter: &str,
        candidates: [C; 2],
        expected: R,
        mut resolve: impl FnMut([C; 2]) -> R,
    ) where
        C: Copy,
        R: std::fmt::Debug + PartialEq,
    {
        let forward = resolve(candidates);
        let reverse = resolve([candidates[1], candidates[0]]);
        assert_eq!(forward, expected, "{adapter}: forward order");
        assert_eq!(reverse, expected, "{adapter}: reverse order");
    }

    #[test]
    fn candidate_order_permutation_harness_detects_first_winner_issue_11252() {
        assert!(!candidate_pair_is_order_independent([1, 2], |rows| rows[0]));
    }

    #[test]
    fn dispatch_runtime_vararg_uses_canonical_core_shapes_10460() -> Result<(), String> {
        let runtime_var = |id, name: &str| JuliaType::RuntimeTypeVar {
            id,
            name: name.to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let free_element = runtime_var(10460, "E");
        let free_len = runtime_var(10461, "N");

        let vararg = dispatch_core_type_from_julia(&JuliaType::RuntimeParametric {
            base: "Vararg".to_string(),
            params: vec![free_element],
        });
        let CoreType::Vararg(element) = vararg else {
            return Err("dispatch Vararg must use the canonical CoreType shape".to_string());
        };
        let CoreType::TypeVar(element) = element.as_ref() else {
            return Err("free Vararg element must remain a runtime TypeVar".to_string());
        };
        assert_eq!(element.rigid_identity, Some(10460));

        let vararg_len = dispatch_core_type_from_julia(&JuliaType::RuntimeParametric {
            base: "Vararg".to_string(),
            params: vec![JuliaType::Int64, free_len],
        });
        let CoreType::VarargLen { element, len } = vararg_len else {
            return Err("dispatch Vararg{T,N} must use the canonical VarargLen shape".to_string());
        };
        assert_eq!(element.as_ref(), &CoreType::Primitive(CorePrimitive::Int64));
        let CoreType::TypeVar(len) = len.as_ref() else {
            return Err("free Vararg length must remain a runtime TypeVar".to_string());
        };
        assert_eq!(len.rigid_identity, Some(10461));
        Ok(())
    }

    #[test]
    fn runtime_struct_leaf_guard_survives_structured_matcher_issue_5314() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Q5314", Some("Any".to_string()), Vec::new());

        let mut bindings = HashMap::new();
        assert!(runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &JuliaType::Struct("Q5314".to_string()),
            None,
            &JuliaType::Struct("Q5314".to_string()),
            &[],
            &mut bindings,
            || false,
        ));

        let mut bindings = HashMap::new();
        assert!(!runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &JuliaType::Float64,
            None,
            &JuliaType::Struct("Q5314".to_string()),
            &[],
            &mut bindings,
            || true,
        ));
    }

    #[test]
    fn runtime_type_object_pattern_requires_bound_params_issue_9981() {
        let hierarchy = StructHierarchy::new();
        let type_params = vec![
            TypeParam::new("K".to_string()),
            TypeParam::new("V".to_string()),
        ];
        let pattern = JuliaType::TypeOf(Box::new(JuliaType::Struct("Pair{K,V}".to_string())));

        let mut bindings = HashMap::new();
        assert!(!runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &JuliaType::TypeOf(Box::new(JuliaType::Bottom)),
            Some(&JuliaType::Bottom),
            &pattern,
            &type_params,
            &mut bindings,
            || false,
        ));
        assert!(bindings.is_empty());

        let mut bindings = HashMap::new();
        assert!(runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &JuliaType::TypeOf(Box::new(JuliaType::Struct("Pair{Int64,Int8}".to_string(),))),
            Some(&JuliaType::Struct("Pair{Int64,Int8}".to_string())),
            &pattern,
            &type_params,
            &mut bindings,
            || false,
        ));
        assert_eq!(bindings.get("K"), Some(&JuliaType::Int64));
        assert_eq!(bindings.get("V"), Some(&JuliaType::Int8));
    }

    #[test]
    fn runtime_exact_type_object_rejects_unrelated_datatype_issue_10782() {
        let hierarchy = StructHierarchy::new();
        let pattern = JuliaType::TypeOf(Box::new(JuliaType::Struct(
            "LayoutPredicateDispatchBox3911".to_string(),
        )));

        let mut bindings = HashMap::new();
        assert!(!runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &JuliaType::TypeOf(Box::new(JuliaType::Int64)),
            Some(&JuliaType::Int64),
            &pattern,
            &[],
            &mut bindings,
            || true,
        ));

        let mut bindings = HashMap::new();
        assert!(runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &JuliaType::TypeOf(Box::new(JuliaType::Struct(
                "LayoutPredicateDispatchBox3911".to_string()
            ))),
            Some(&JuliaType::Struct(
                "LayoutPredicateDispatchBox3911".to_string()
            )),
            &pattern,
            &[],
            &mut bindings,
            || false,
        ));
    }

    /// Prevention regression matrix for Issue #9987 (completing the
    /// `runtime_type_object_pattern_requires_bound_params_issue_9981` cases
    /// above with the "reject" side of a `Type{Parametric{T...}}` pattern):
    /// a runtime `Type{...}` value whose underlying concrete type does NOT
    /// belong to the pattern's struct family must be rejected — same as the
    /// `Union{}` case, but for an ordinary non-matching concrete type object,
    /// so a future edit cannot narrow the guard to only the `Bottom` case.
    /// Covers both a wholly different family name and a same-family arity
    /// mismatch (fewer concrete type arguments than the pattern declares).
    #[test]
    fn runtime_type_object_pattern_rejects_non_matching_concrete_type_issue_9987() {
        let hierarchy = StructHierarchy::new();
        let type_params = vec![
            TypeParam::new("K".to_string()),
            TypeParam::new("V".to_string()),
        ];
        let pattern = JuliaType::TypeOf(Box::new(JuliaType::Struct("Pair{K,V}".to_string())));

        // A concrete type object from an unrelated struct family (`Foo`, not
        // `Pair{...}`) must not bind K/V and must not match.
        let mut bindings = HashMap::new();
        assert!(!runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &JuliaType::TypeOf(Box::new(JuliaType::Struct("Foo".to_string()))),
            Some(&JuliaType::Struct("Foo".to_string())),
            &pattern,
            &type_params,
            &mut bindings,
            || false,
        ));
        assert!(bindings.is_empty());

        // Same family (`Pair`) but fewer concrete type arguments than the
        // pattern declares (`Pair{Int64}` vs. `Pair{K,V}`) is an arity
        // mismatch, not a partial match — must still be rejected.
        let mut bindings = HashMap::new();
        assert!(!runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &JuliaType::TypeOf(Box::new(JuliaType::Struct("Pair{Int64}".to_string()))),
            Some(&JuliaType::Struct("Pair{Int64}".to_string())),
            &pattern,
            &type_params,
            &mut bindings,
            || false,
        ));
        assert!(bindings.is_empty());
    }

    /// `struct_family_matches` is decided by the shared subtype engine
    /// (Issue #5915): the legacy Array alias family keeps matching, and
    /// engine-known abstract container bases admit their concrete
    /// carriers (julia: `Vector{Int64} <: AbstractVector{Int64}`).
    #[test]
    fn struct_family_matching_uses_subtype_engine_issue_5915() {
        // Legacy Array alias family preserved.
        assert!(type_name_pattern_matches(
            &strings(&["Array{Int64}"]),
            &strings(&["Vector{Int64}"])
        ));
        assert!(!type_name_pattern_matches(
            &strings(&["Array{Int64}"]),
            &strings(&["Vector{Float64}"])
        ));
        // Abstract container bases stay outside the strict pattern tier;
        // they are admitted (at a lower score) by the subtype-fallback
        // channel in `resolve_type_name_candidates_with_subtype_fallback`,
        // whose `subtype_matches` closure is the VM's engine-backed
        // `check_subtype`.
        assert!(!type_name_pattern_matches(
            &strings(&["AbstractVector{Int64}"]),
            &strings(&["Vector{Float64}"])
        ));
        // Unrelated families stay rejected.
        assert!(!type_name_pattern_matches(
            &strings(&["Vector{Int64}"]),
            &strings(&["Dict{String, Int64}"])
        ));
        // Bare nominal family question (julia: Matrix <: Array).
        assert!(struct_family_matches("Array", "Matrix"));
        assert!(!struct_family_matches("Array", "BitVector"));
        assert!(!struct_family_matches("Vector", "Array"));
    }

    #[test]
    fn runtime_resolver_matches_exact_abstract_parametric_and_typevars() {
        let candidates = [
            (10usize, strings(&["Any"])),
            (11usize, strings(&["Real"])),
            (12usize, strings(&["Vector{T}"])),
            (13usize, strings(&["Vector{Int64}"])),
        ];

        assert_eq!(
            resolve_type_name_candidates(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Vector{Int64}"])
            ),
            Some((13, 6))
        );
        assert_eq!(
            resolve_type_name_candidates(
                candidates[..2]
                    .iter()
                    .map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Int64"])
            ),
            Some((11, 2))
        );
    }

    #[test]
    fn runtime_resolver_prefers_bare_memory_family_over_any_issue_4052() {
        let candidates = [
            (10usize, strings(&["Any", "Any"])),
            (11usize, strings(&["Any", "Memory"])),
        ];

        assert!(type_name_pattern_matches(
            &strings(&["Memory"]),
            &strings(&["Memory{Int64}"])
        ));
        assert_eq!(
            resolve_type_name_candidates(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Function", "Memory{Int64}"])
            ),
            Some((11, 5))
        );
    }

    #[test]
    fn runtime_value_match_binds_memory_element_typevar_issue_9472() {
        let hierarchy = StructHierarchy::new();
        let type_params = [TypeParam::new("T".to_string())];
        let mut bindings = HashMap::new();

        assert!(runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &JuliaType::Struct("Memory{Foo}".to_string()),
            None,
            &JuliaType::Struct("Memory{T}".to_string()),
            &type_params,
            &mut bindings,
            || false,
        ));
        assert_eq!(
            bindings.get("T"),
            Some(&JuliaType::Struct("Foo".to_string()))
        );
    }

    #[test]
    fn runtime_value_match_binds_subarray_abstract_vector_element_issue_9776() {
        let hierarchy = StructHierarchy::new();
        let type_params = [TypeParam::new("T".to_string())];
        let mut bindings = HashMap::new();
        let subarray = JuliaType::Struct(
            "SubArray{Float64, 1, Vector{Float64}, Tuple{UnitRange{Int64}}, true}".to_string(),
        );

        assert!(runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &subarray,
            None,
            &JuliaType::Struct("AbstractVector{T}".to_string()),
            &type_params,
            &mut bindings,
            || false,
        ));
        assert_eq!(bindings.get("T"), Some(&JuliaType::Float64));
    }

    #[test]
    fn runtime_value_match_rejects_vector_for_concrete_subarray_pattern_issue_9778() {
        let hierarchy = StructHierarchy::new();
        let type_params = [
            TypeParam::new("T".to_string()),
            TypeParam::new("N".to_string()),
            TypeParam::new("P".to_string()),
            TypeParam::new("I".to_string()),
            TypeParam::new("L".to_string()),
        ];
        let subarray_pattern = JuliaType::Struct("SubArray{T, N, P, I, L}".to_string());
        let vector = JuliaType::VectorOf(Box::new(JuliaType::Float64));
        let mut bindings = HashMap::new();

        assert!(!runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &vector,
            None,
            &subarray_pattern,
            &type_params,
            &mut bindings,
            || false,
        ));

        let subarray = JuliaType::Struct(
            "SubArray{Float64, 1, Vector{Float64}, Tuple{UnitRange{Int64}}, true}".to_string(),
        );
        let mut bindings = HashMap::new();
        assert!(runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &subarray,
            None,
            &subarray_pattern,
            &type_params,
            &mut bindings,
            || false,
        ));
        assert_eq!(bindings.get("T"), Some(&JuliaType::Float64));
    }

    #[test]
    fn runtime_typeof_exact_param_is_invariant_issue_9472() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Symbolics.Num", Some("Real".to_string()), Vec::new());

        let symbolics_num = JuliaType::Struct("Symbolics.Num".to_string());
        let type_of_symbolics_num = JuliaType::TypeOf(Box::new(symbolics_num.clone()));

        let mut bindings = HashMap::new();
        assert!(!runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &type_of_symbolics_num,
            Some(&symbolics_num),
            &JuliaType::TypeOf(Box::new(JuliaType::Real)),
            &[],
            &mut bindings,
            || false,
        ));

        let mut bindings = HashMap::new();
        assert!(runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &JuliaType::TypeOf(Box::new(JuliaType::Real)),
            Some(&JuliaType::Real),
            &JuliaType::TypeOf(Box::new(JuliaType::Real)),
            &[],
            &mut bindings,
            || true,
        ));

        let type_params = [TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )];
        let mut bindings = HashMap::new();
        assert!(runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &type_of_symbolics_num,
            Some(&symbolics_num),
            &JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
            &type_params,
            &mut bindings,
            || false,
        ));
        assert_eq!(bindings.get("T"), Some(&symbolics_num));
    }

    #[test]
    fn runtime_resolver_reuses_typevar_bindings() {
        assert!(type_name_pattern_matches(
            &strings(&["T", "T"]),
            &strings(&["Int64", "Int64"])
        ));
        assert!(!type_name_pattern_matches(
            &strings(&["T", "T"]),
            &strings(&["Int64", "Float64"])
        ));
        assert!(
            type_name_pattern_specificity(&strings(&["T", "T"]))
                > type_name_pattern_specificity(&strings(&["T", "S"]))
        );
    }

    #[test]
    fn typed_core_specificity_matches_rendered_policy_issue_6502() {
        let signatures = [
            vec!["Any"],
            vec!["T", "T"],
            vec!["T", "S"],
            vec!["Type"],
            vec!["Type{T}"],
            vec!["Type{<:Number}"],
            vec!["Type{Int64}"],
            vec!["Vector{T}", "Vector{T}"],
            vec!["Vector{<:Real}", "Vector{<:Real}"],
            vec!["Tuple{}"],
            vec!["Tuple{Int64, Float64}"],
            vec!["Union{}"],
            vec!["Union{Int64, String}"],
            vec!["Vector{T} where T<:Real"],
        ];

        for signature in signatures {
            let rendered = strings(&signature);
            let type_params = inferred_type_params_from_expected_names(&rendered);
            let slots: Vec<_> = rendered
                .iter()
                .map(|name| embed_type_param_bounds(CoreType::from_julia_name(name), &type_params))
                .collect();
            assert_eq!(
                core_type_pattern_specificity(&slots),
                type_name_pattern_specificity(&rendered),
                "signature {signature:?}"
            );
        }
    }

    #[test]
    fn runtime_resolver_handles_covariant_bound_patterns() {
        assert!(type_name_pattern_matches(
            &strings(&["Vector{_<:Real}"]),
            &strings(&["Vector{Int64}"])
        ));
        assert!(!type_name_pattern_matches(
            &strings(&["Vector{_<:Integer}"]),
            &strings(&["Vector{Float64}"])
        ));
    }

    #[test]
    fn runtime_resolver_keeps_invariant_vector_params_issue_4276() {
        let sig = strings(&["Vector{Int64}", "Int64"]);
        let actual = strings(&["Vector{Any}", "Int64"]);
        assert!(!type_name_pattern_matches(&sig, &actual));
        assert!(same_invariant_container_family_concrete_miss(
            &sig[0], &actual[0]
        ));
        assert_eq!(
            resolve_type_name_candidates_with_subtype_fallback(
                std::iter::once((1usize, sig.as_slice())),
                &actual,
                |actual, expected| CoreType::from_julia_name(actual)
                    .is_subtype_of(&CoreType::from_julia_name(expected)),
            ),
            None
        );
    }

    #[test]
    fn runtime_resolver_uses_type_singleton_specificity_issue_4131() {
        let exact_any_candidates = [(1, strings(&["Type"])), (2, strings(&["Type{Any}"]))];
        assert_eq!(
            resolve_type_name_candidates(
                exact_any_candidates
                    .iter()
                    .map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Any}"])
            ),
            Some((2, 8))
        );
        assert_eq!(
            resolve_type_name_candidates(
                exact_any_candidates
                    .iter()
                    .map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Int64}"])
            ),
            Some((1, 5))
        );

        let typevar_candidates = [(1, strings(&["Type"])), (2, strings(&["Type{T}"]))];
        assert_eq!(
            resolve_type_name_candidates(
                typevar_candidates
                    .iter()
                    .map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Int64}"])
            ),
            Some((2, 7))
        );
    }

    #[test]
    fn runtime_resolver_binds_typeof_parametric_inner_typevars_issue_4569() {
        assert!(type_name_pattern_matches(
            &strings(&["Type{Array{T}}"]),
            &strings(&["Type{Array{Int64}}"])
        ));
        assert!(!type_name_pattern_matches(
            &strings(&["Type{Array{Real}}"]),
            &strings(&["Type{Array{Int64}}"])
        ));

        let candidates = [
            (1, strings(&["Array{T}", "Tuple"])),
            (2, strings(&["Type{Array{T}}", "Tuple"])),
        ];
        assert_eq!(
            resolve_type_name_candidates(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Array{Int64}}", "Tuple"])
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn runtime_resolver_prefers_parametric_type_pattern_over_bare_type_issue_4636() {
        let candidates = [
            (1, strings(&["Type{Pair}", "Tuple"])),
            (2, strings(&["Type{Pair{K,V}}", "Tuple"])),
            (3, strings(&["Type{T}", "Tuple"])),
        ];

        assert!(type_name_pattern_matches(
            &strings(&["Type{Pair{K,V}}", "Tuple"]),
            &strings(&["Type{Pair{Int64,Int8}}", "Tuple{Int64}"])
        ));
        assert_eq!(
            resolve_type_name_candidates(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Pair{Int64,Int8}}", "Tuple{Int64}"])
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn typed_resolver_uses_covariant_subtype_fallback_issue_3910() {
        crate::types::register_type_name("Dog");

        let candidates = [
            (1, strings(&["Type{_<:Animal}"])),
            (2, strings(&["Type{Dog}"])),
        ];

        assert_eq!(
            resolve_type_name_candidates_with_subtype_fallback(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Dog}"]),
                |actual, bound| matches!((actual, bound), ("Dog", "Animal") | ("Cat", "Animal"))
            ),
            Some((2, 13))
        );

        assert_eq!(
            resolve_type_name_candidates_with_subtype_fallback(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Cat}"]),
                |actual, bound| matches!((actual, bound), ("Dog", "Animal") | ("Cat", "Animal"))
            ),
            Some((1, 7))
        );

        assert_eq!(
            resolve_type_name_candidates_with_subtype_fallback(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Rock}"]),
                |actual, bound| matches!((actual, bound), ("Dog", "Animal") | ("Cat", "Animal"))
            ),
            None
        );
    }

    /// Issue #6502: the typed-dispatch structured resolver preserves the
    /// legacy quality/specificity ordering while matching on cached CoreType
    /// slots instead of reparsing rendered names at each call site.
    #[test]
    fn typed_core_resolver_matches_legacy_string_order_issue_6502() {
        let hierarchy = StructHierarchy::new();
        let rows: Vec<(usize, Vec<String>, Vec<CoreType>)> = vec![
            (1, strings(&["Any"]), vec![CoreType::Any]),
            (
                2,
                strings(&["Type{T}"]),
                vec![CoreType::from_julia_name("Type{T}")],
            ),
            (
                3,
                strings(&["Type{<:Number}"]),
                vec![CoreType::from_julia_name("Type{<:Number}")],
            ),
            (
                4,
                strings(&["Type{Int64}"]),
                vec![CoreType::from_julia_name("Type{Int64}")],
            ),
        ];
        let actual = strings(&["Type{Int64}"]);
        let actual_cores: Vec<_> = actual
            .iter()
            .map(|name| CoreType::from_julia_name(name))
            .collect();

        let legacy = resolve_type_name_candidates_with_subtype_fallback(
            rows.iter()
                .map(|(idx, rendered, _)| (*idx, rendered.as_slice())),
            &actual,
            |actual, expected| {
                CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(
                    &CoreType::from_julia_name(actual),
                    &CoreType::from_julia_name(expected),
                )
            },
        );
        let structured = resolve_typed_runtime_core_candidates_with_subtype_fallback(
            &hierarchy,
            rows.iter()
                .map(|(idx, rendered, slots)| RuntimeTypedCoreCandidate {
                    idx: *idx,
                    rendered: rendered.as_slice(),
                    slots: slots.as_slice(),
                    signature: None,
                }),
            &actual_cores,
            |actual, expected| {
                CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
            },
        );

        assert_eq!(structured, legacy);
        assert_eq!(structured.map(|(idx, _)| idx), Some(4));
    }

    #[test]
    fn typed_core_resolver_uses_covariant_slots_without_rendered_bridge_issue_6502() {
        let hierarchy = StructHierarchy::new();
        let rendered = strings(&["Vector{Any}"]);
        let slots = [CoreType::from_julia_name("Vector{<:Real}")];
        let rows = [RuntimeTypedCoreCandidate {
            idx: 1,
            rendered: rendered.as_slice(),
            slots: slots.as_slice(),
            signature: None,
        }];
        let run = |actual: &str| {
            let actual = [CoreType::from_julia_name(actual)];
            resolve_typed_runtime_core_candidates_with_subtype_fallback(
                &hierarchy,
                rows,
                &actual,
                |actual, expected| {
                    CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                },
            )
            .map(|(idx, _)| idx)
        };

        assert_eq!(run("Vector{Int64}"), Some(1));
        assert_eq!(run("Vector{String}"), None);
    }

    #[test]
    fn typed_core_resolver_tier_split_uses_bounded_slots_issue_6502() {
        let hierarchy = StructHierarchy::new();
        let broad_rendered = strings(&["Any"]);
        let broad_slots = [CoreType::Any];
        let bounded_rendered_without_marker = strings(&["Vector{Any}"]);
        let bounded_slots = [CoreType::from_julia_name("Vector{<:Real}")];
        let rows = [
            RuntimeTypedCoreCandidate {
                idx: 1,
                rendered: broad_rendered.as_slice(),
                slots: broad_slots.as_slice(),
                signature: None,
            },
            RuntimeTypedCoreCandidate {
                idx: 2,
                rendered: bounded_rendered_without_marker.as_slice(),
                slots: bounded_slots.as_slice(),
                signature: None,
            },
        ];
        let actual = [CoreType::from_julia_name("Vector{Int64}")];

        let selected = resolve_typed_runtime_core_candidates_with_subtype_fallback(
            &hierarchy,
            rows,
            &actual,
            |actual, expected| {
                CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
            },
        )
        .map(|(idx, _)| idx);

        assert_eq!(selected, Some(1));
    }

    /// Issue #6502 / #6229: typed-dispatch candidates may still declare
    /// `JuliaType::Array` while their rendered runtime signature preserves the
    /// parametric `Vector{T}` shape. Keep that shape in structured slots so
    /// repeated typevars reject mixed element types.
    #[test]
    fn typed_core_resolver_keeps_rendered_array_diagonal_issue_6502() {
        let hierarchy = StructHierarchy::new();
        let type_params = [TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )];
        let diagonal_rendered = strings(&["Vector{T}", "Vector{T}"]);
        let diagonal_slots: Vec<CoreType> = diagonal_rendered
            .iter()
            .map(|name| {
                embed_type_param_bounds(
                    runtime_candidate_core_type(&JuliaType::Array, name),
                    &type_params,
                )
            })
            .collect();
        assert_eq!(
            diagonal_slots[0],
            embed_type_param_bounds(CoreType::from_julia_name("Vector{T}"), &type_params)
        );

        let diagonal_signature = runtime_core_signature(&diagonal_slots, &type_params);
        let independent_rendered = strings(&["Vector{<:Real}", "Vector{<:Real}"]);
        let independent_slots: Vec<CoreType> = independent_rendered
            .iter()
            .map(|name| CoreType::from_julia_name(name))
            .collect();
        let rows = [
            RuntimeTypedCoreCandidate {
                idx: 1,
                rendered: diagonal_rendered.as_slice(),
                slots: diagonal_slots.as_slice(),
                signature: Some(&diagonal_signature),
            },
            RuntimeTypedCoreCandidate {
                idx: 2,
                rendered: independent_rendered.as_slice(),
                slots: independent_slots.as_slice(),
                signature: None,
            },
        ];

        let run = |left: &str, right: &str| {
            let actual = [
                CoreType::from_julia_name(left),
                CoreType::from_julia_name(right),
            ];
            resolve_typed_runtime_core_candidates_with_subtype_fallback(
                &hierarchy,
                rows,
                &actual,
                |actual, expected| {
                    CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                },
            )
            .map(|(idx, _)| idx)
        };

        assert_eq!(run("Vector{Int64}", "Vector{Int64}"), Some(1));
        assert_eq!(run("Vector{Int64}", "Vector{Float64}"), Some(2));
    }

    #[test]
    fn typed_resolver_rejects_mismatched_type_vector_diagonal_issue_6239() {
        let candidates = [
            (1, strings(&["Type{T}", "AbstractVector{T}"])),
            (2, strings(&["Type{Integer}", "AbstractVector{<:Real}"])),
        ];

        assert_eq!(
            resolve_type_name_candidates_with_subtype_fallback(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Integer}", "Vector{Int64}"]),
                |actual, bound| matches!(
                    (actual, bound),
                    ("Vector{Int64}", "AbstractVector{<:Real}")
                        | ("Vector{Int64}", "AbstractVector")
                        | ("Int64", "Real")
                )
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn typed_core_resolver_rejects_mismatched_type_vector_diagonal_issue_6573() {
        let hierarchy = StructHierarchy::new();
        let type_params = [TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )];
        let diagonal_rendered = strings(&["Type{T}", "AbstractVector{T}"]);
        let diagonal_slots: Vec<CoreType> = diagonal_rendered
            .iter()
            .map(|name| embed_type_param_bounds(CoreType::from_julia_name(name), &type_params))
            .collect();
        let diagonal_signature = runtime_core_signature(&diagonal_slots, &type_params);
        let fixed_rendered = strings(&["Type{Integer}", "AbstractVector{<:Real}"]);
        let fixed_slots: Vec<CoreType> = fixed_rendered
            .iter()
            .map(|name| CoreType::from_julia_name(name))
            .collect();
        let rows = [
            RuntimeTypedCoreCandidate {
                idx: 1,
                rendered: diagonal_rendered.as_slice(),
                slots: diagonal_slots.as_slice(),
                signature: Some(&diagonal_signature),
            },
            RuntimeTypedCoreCandidate {
                idx: 2,
                rendered: fixed_rendered.as_slice(),
                slots: fixed_slots.as_slice(),
                signature: None,
            },
        ];
        let actual = [
            CoreType::from_julia_name("Type{Integer}"),
            CoreType::from_julia_name("Vector{Int64}"),
        ];

        assert_eq!(
            resolve_typed_runtime_core_candidates_with_subtype_fallback(
                &hierarchy,
                rows,
                &actual,
                |actual, expected| {
                    CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                },
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn typed_core_resolver_rejects_bigint_for_int64_candidate_issue_9768() {
        let hierarchy = StructHierarchy::new();
        let int64_rendered = strings(&["Int64"]);
        let int64_slots = [CoreType::from_julia_name("Int64")];
        let integer_rendered = strings(&["Integer"]);
        let integer_slots = [CoreType::from_julia_name("Integer")];
        let rows = [
            RuntimeTypedCoreCandidate {
                idx: 1,
                rendered: int64_rendered.as_slice(),
                slots: int64_slots.as_slice(),
                signature: None,
            },
            RuntimeTypedCoreCandidate {
                idx: 2,
                rendered: integer_rendered.as_slice(),
                slots: integer_slots.as_slice(),
                signature: None,
            },
        ];
        let actual = [CoreType::from_julia_name("BigInt")];

        assert_eq!(
            resolve_typed_runtime_core_candidates_with_subtype_fallback(
                &hierarchy,
                rows,
                &actual,
                |actual, expected| {
                    CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                },
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn typed_core_resolver_matches_rank_typevar_abstract_array_issue_6577() {
        let hierarchy = StructHierarchy::new();
        let diagonal_type_params = [
            TypeParam::with_upper_bound("T".to_string(), "Real".to_string()),
            TypeParam::new("N".to_string()),
        ];
        let diagonal_rendered = strings(&["Type{T}", "AbstractArray{T,N}"]);
        let diagonal_slots: Vec<CoreType> = diagonal_rendered
            .iter()
            .map(|name| {
                embed_type_param_bounds(CoreType::from_julia_name(name), &diagonal_type_params)
            })
            .collect();
        let diagonal_signature = runtime_core_signature(&diagonal_slots, &diagonal_type_params);

        let fixed_type_params = [TypeParam::new("N".to_string())];
        let fixed_rendered = strings(&["Type{Integer}", "AbstractArray{<:Real,N}"]);
        let fixed_slots: Vec<CoreType> = fixed_rendered
            .iter()
            .map(|name| {
                embed_type_param_bounds(CoreType::from_julia_name(name), &fixed_type_params)
            })
            .collect();
        let fixed_signature = runtime_core_signature(&fixed_slots, &fixed_type_params);
        let rows = [
            RuntimeTypedCoreCandidate {
                idx: 1,
                rendered: diagonal_rendered.as_slice(),
                slots: diagonal_slots.as_slice(),
                signature: Some(&diagonal_signature),
            },
            RuntimeTypedCoreCandidate {
                idx: 2,
                rendered: fixed_rendered.as_slice(),
                slots: fixed_slots.as_slice(),
                signature: Some(&fixed_signature),
            },
        ];
        let actual = [
            CoreType::from_julia_name("Type{Integer}"),
            CoreType::from_julia_name("Vector{Int64}"),
        ];

        assert_eq!(
            resolve_typed_runtime_core_candidates_with_subtype_fallback(
                &hierarchy,
                rows,
                &actual,
                |actual, expected| {
                    CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                },
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn typed_resolver_prefers_type_value_diagonal_issue_6233() {
        let candidates = [
            (1, strings(&["Type{T}", "T<:Real"])),
            (2, strings(&["Type{Integer}", "Integer"])),
        ];

        assert_eq!(
            resolve_type_name_candidates_with_subtype_fallback(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Int64}", "Int64"]),
                |actual, bound| CoreType::from_julia_name(actual)
                    .is_subtype_of(&CoreType::from_julia_name(bound)),
            )
            .map(|(idx, _)| idx),
            Some(1)
        );
        assert_eq!(
            resolve_type_name_candidates_with_subtype_fallback(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Integer}", "Int64"]),
                |actual, bound| CoreType::from_julia_name(actual)
                    .is_subtype_of(&CoreType::from_julia_name(bound)),
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn runtime_type_pattern_score_uses_shared_scores_issue_3910() {
        let actual = ["Rational{Int64}", "Int64"];

        assert_eq!(
            runtime_type_pattern_score(
                &["Rational{Int64}", "Int64"],
                &actual,
                &mut |actual, expected| {
                    CoreType::from_julia_name(actual)
                        .is_subtype_of(&CoreType::from_julia_name(expected))
                }
            ),
            Some(8)
        );
    }

    #[test]
    fn runtime_type_pattern_score_uses_subtype_fallback_issue_3910() {
        let mut subtype_matches = |actual: &str, expected: &str| {
            CoreType::from_julia_name(actual).is_subtype_of(&CoreType::from_julia_name(expected))
        };

        assert_eq!(
            runtime_type_pattern_score(
                &["Real", "Number"],
                &["Int64", "Float64"],
                &mut subtype_matches
            ),
            Some(4)
        );

        let mut no_subtype_fallback = |_: &str, _: &str| false;
        assert_eq!(
            runtime_type_pattern_score(
                &["Real", "Number"],
                &["Int64", "Float64"],
                &mut no_subtype_fallback
            ),
            None
        );
    }

    /// Issue #6539: the callable-value channel must enforce explicit `where`
    /// bounds through the `core_signature` subtype gate. A bounded
    /// `f(::Holder{T}) where {T<:Real}` must be rejected for
    /// `Holder{String}` (selecting the bare `f(::Holder)` sibling) while
    /// still winning for `Holder{Int64}`.
    #[test]
    fn callable_value_candidates_enforce_where_bounds_issue_6539() {
        let bounded_params = vec![JuliaType::Struct("Holder{T}".to_string())];
        let bare_params = vec![JuliaType::Struct("Holder".to_string())];
        let bounded_type_params = vec![TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )];
        let candidates = || {
            [
                CallableValueCandidate {
                    idx: 1,
                    param_types: &bounded_params,
                    param_count: 1,
                    vararg_param_index: None,
                    vararg_fixed_count: None,
                    type_params: &bounded_type_params,
                },
                CallableValueCandidate {
                    idx: 2,
                    param_types: &bare_params,
                    param_count: 1,
                    vararg_param_index: None,
                    vararg_fixed_count: None,
                    type_params: &[],
                },
            ]
        };
        // The VM's loose matcher accepts both candidates for any Holder
        // instantiation; the bound gate is what must discriminate.
        let loose = |actual: &str, param: &JuliaType| {
            actual.starts_with("Holder") && param.name().starts_with("Holder")
        };

        // Out-of-bound element type: the bounded method is rejected by the
        // gate, the bare sibling wins.
        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates(),
                &strings(&["Holder{String}"]),
                loose,
                |_, _| false
            )
            .map(|(idx, _)| idx),
            Some(2)
        );

        // In-bound element type: the bounded parametric method passes the
        // gate and outscores the bare sibling.
        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates(),
                &strings(&["Holder{Int64}"]),
                loose,
                |_, _| false
            )
            .map(|(idx, _)| idx),
            Some(1)
        );
    }

    /// Issue #6539: candidates with *unbounded* `where` parameters skip the
    /// signature gate entirely — legacy loose matching is preserved (the
    /// diagonal rule owns their cross-slot consistency).
    #[test]
    fn callable_value_candidates_unbounded_where_skips_gate_issue_6539() {
        let unbounded_params = vec![JuliaType::Struct("Holder{T}".to_string())];
        let unbounded_type_params = vec![TypeParam::new("T".to_string())];
        let candidates = [CallableValueCandidate {
            idx: 1,
            param_types: &unbounded_params,
            param_count: 1,
            vararg_param_index: None,
            vararg_fixed_count: None,
            type_params: &unbounded_type_params,
        }];

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &strings(&["Holder{String}"]),
                |actual, param| {
                    actual.starts_with("Holder") && param.name().starts_with("Holder")
                },
                |_, _| false
            )
            .map(|(idx, _)| idx),
            Some(1)
        );
    }

    #[test]
    fn callable_value_candidates_use_shared_vm_score_policy_issue_3910() {
        let any_params = vec![JuliaType::Any];
        let int_params = vec![JuliaType::Int64];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &any_params,
                param_count: 1,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &int_params,
                param_count: 1,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
        ];
        let actual = strings(&["Int64"]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |actual, param| param == &JuliaType::Any || param.name() == actual,
                |actual, param| param.name() == actual
            ),
            Some((2, 21))
        );
    }

    #[test]
    fn callable_value_candidates_prefer_function_singleton_over_any_issue_9979() {
        let any_any_params = vec![JuliaType::Any, JuliaType::Any];
        let any_function_params = vec![JuliaType::Any, JuliaType::Function];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &any_any_params,
                param_count: 2,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &any_function_params,
                param_count: 2,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
        ];
        let actual = strings(&["StepRangeLen{Float64}", "typeof(cos)"]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |actual, param| {
                    runtime_type_name_matches_param(
                        actual,
                        param,
                        |_| true,
                        |actual, expected| {
                            CoreType::from_julia_name(actual)
                                .is_subtype_of(&CoreType::from_julia_name(expected))
                        },
                    )
                },
                |actual, param| param.name() == actual
            )
            .map(|(idx, _)| idx),
            Some(2)
        );

        let any_any_params = vec![JuliaType::Any, JuliaType::Any];
        let function_any_params = vec![JuliaType::Function, JuliaType::Any];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &any_any_params,
                param_count: 2,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &function_any_params,
                param_count: 2,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
        ];
        let actual = strings(&["typeof(__lambda_0)", "Vector{Int64}"]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |actual, param| {
                    runtime_type_name_matches_param(
                        actual,
                        param,
                        |_| true,
                        |actual, expected| {
                            CoreType::from_julia_name(actual)
                                .is_subtype_of(&CoreType::from_julia_name(expected))
                        },
                    )
                },
                |actual, param| param.name() == actual
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn callable_value_candidates_prefer_array_family_over_any_vararg_issue_9979() {
        for array_param in [JuliaType::Array, JuliaType::Struct("Array".to_string())] {
            let any_any_vararg_params = vec![JuliaType::Any, JuliaType::Any, JuliaType::Any];
            let any_array_params = vec![JuliaType::Any, array_param];
            let candidates = [
                CallableValueCandidate {
                    idx: 1,
                    param_types: &any_any_vararg_params,
                    param_count: 3,
                    vararg_param_index: Some(2),
                    vararg_fixed_count: None,
                    type_params: &[],
                },
                CallableValueCandidate {
                    idx: 2,
                    param_types: &any_array_params,
                    param_count: 2,
                    vararg_param_index: None,
                    vararg_fixed_count: None,
                    type_params: &[],
                },
            ];
            let actual = strings(&["typeof(__lambda_0)", "Vector{Int64}"]);

            assert_eq!(
                resolve_callable_value_candidates(
                    &StructHierarchy::new(),
                    candidates,
                    &actual,
                    |actual, param| {
                        runtime_type_name_matches_param(
                            actual,
                            param,
                            |_| true,
                            |actual, expected| {
                                CoreType::from_julia_name(actual)
                                    .is_subtype_of(&CoreType::from_julia_name(expected))
                            },
                        )
                    },
                    |actual, param| param.name() == actual
                )
                .map(|(idx, _)| idx),
                Some(2)
            );
        }
    }

    #[test]
    fn callable_value_candidates_prefer_fixed_arity_over_vararg_tie_issue_9979() {
        let vararg_params = vec![JuliaType::Any, JuliaType::Any, JuliaType::Any];
        let fixed_params = vec![JuliaType::Any, JuliaType::Any];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &vararg_params,
                param_count: 3,
                vararg_param_index: Some(2),
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &fixed_params,
                param_count: 2,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
        ];

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &strings(&["typeof(__lambda_0)", "Vector{Int64}"]),
                |_, param| matches!(param, JuliaType::Any),
                |_, _| false
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn callable_value_function_singleton_prefers_function_param_issue_9741() {
        let any_any_params = vec![JuliaType::Any, JuliaType::Any];
        let any_function_params = vec![JuliaType::Any, JuliaType::Function];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &any_any_params,
                param_count: 2,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &any_function_params,
                param_count: 2,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
        ];
        let actual = strings(&["Vector{Float64}", "typeof(cos)"]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |actual, param| {
                    param == &JuliaType::Any
                        || param.name() == actual
                        || (matches!(param, JuliaType::Function)
                            && actual.starts_with("typeof(")
                            && actual.ends_with(')'))
                },
                |actual, param| param.name() == actual
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn callable_value_subtype_fallback_prefers_array_over_any_vararg_issue_9741() {
        let lazy_iterators_map_params = vec![JuliaType::Any, JuliaType::Any, JuliaType::Any];
        let eager_array_map_params = vec![JuliaType::Any, JuliaType::Array];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &lazy_iterators_map_params,
                param_count: 3,
                vararg_param_index: Some(2),
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &eager_array_map_params,
                param_count: 2,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
        ];
        let actual = strings(&["typeof(__lambda_0)", "Vector{Int64}"]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |actual, param| {
                    runtime_type_name_matches_param(
                        actual,
                        param,
                        |_| true,
                        |actual, expected| {
                            CoreType::from_julia_name(actual)
                                .is_subtype_of(&CoreType::from_julia_name(expected))
                        },
                    )
                },
                |actual, param| param.name() == actual
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn callable_value_tie_prefers_fixed_arity_over_vararg_issue_9981() {
        let lazy_iterators_map_params = vec![JuliaType::Any, JuliaType::Any, JuliaType::Any];
        let eager_base_map_params = vec![JuliaType::Any, JuliaType::Any];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &lazy_iterators_map_params,
                param_count: 3,
                vararg_param_index: Some(2),
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &eager_base_map_params,
                param_count: 2,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
        ];
        let actual = strings(&["typeof(__lambda_0)", "Vector{Int64}"]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |_, param| matches!(param, JuliaType::Any),
                |_, _| false
            ),
            Some((2, 13))
        );
    }

    #[test]
    fn callable_value_candidates_reject_non_exact_type_any_issue_8438() {
        let type_any_params = vec![
            JuliaType::TypeOf(Box::new(JuliaType::Any)),
            JuliaType::Tuple,
        ];
        let typevar_params = vec![
            JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
            JuliaType::Tuple,
        ];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &type_any_params,
                param_count: 2,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &typevar_params,
                param_count: 2,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
        ];
        let actual = strings(&["Type{Union{Nothing, Int64}}", "Tuple"]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |actual, param| match param {
                    JuliaType::TypeOf(inner) => {
                        actual.starts_with("Type{")
                            && (matches!(inner.as_ref(), JuliaType::Any | JuliaType::TypeVar(_, _))
                                || param.name() == actual)
                    }
                    _ => param.name() == actual,
                },
                |actual, param| param.name() == actual
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn callable_value_candidates_preserve_fixed_prefix_vararg_bonus_issue_3910() {
        let pure_vararg_params = vec![JuliaType::Any];
        let fixed_prefix_params = vec![JuliaType::Int64, JuliaType::Any];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &pure_vararg_params,
                param_count: 1,
                vararg_param_index: Some(0),
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &fixed_prefix_params,
                param_count: 2,
                vararg_param_index: Some(1),
                vararg_fixed_count: None,
                type_params: &[],
            },
        ];
        let actual = strings(&["Int64", "Int64"]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |actual, param| param == &JuliaType::Any || param.name() == actual,
                |actual, param| param.name() == actual
            ),
            Some((2, 21))
        );
    }

    #[test]
    fn callable_value_candidates_prefer_partial_parametric_fixed_vararg_issue_8407() {
        let generic_params = vec![JuliaType::Any, JuliaType::Any];
        let batch_params = vec![
            JuliaType::Struct("BatchIntegrand{Y, Nothing}".to_string()),
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::TypeVar("T".to_string(), None),
        ];
        let type_params = vec![
            TypeParam::new("Y".to_string()),
            TypeParam::new("T".to_string()),
        ];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &generic_params,
                param_count: 2,
                vararg_param_index: Some(1),
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &batch_params,
                param_count: 4,
                vararg_param_index: Some(3),
                vararg_fixed_count: None,
                type_params: &type_params,
            },
        ];
        let actual = strings(&[
            "QuadGK.BatchIntegrand{Float64, Nothing, Vector{Float64}, Vector{Nothing}, typeof(f!)}",
            "Float64",
            "Float64",
        ]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |actual, param| {
                    runtime_type_name_matches_param(
                        actual,
                        param,
                        |_| true,
                        |actual, expected| {
                            CoreType::from_julia_name(actual)
                                .is_subtype_of(&CoreType::from_julia_name(expected))
                        },
                    )
                },
                |actual, param| param.name() == actual
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn callable_value_candidates_reject_ground_vector_for_tuple_issue_9563() {
        let vector_params = vec![
            JuliaType::Struct("QuadGK.BatchIntegrand".to_string()),
            JuliaType::Struct("Vector".to_string()),
            JuliaType::Any,
            JuliaType::Any,
            JuliaType::Any,
            JuliaType::Any,
        ];
        let ntuple_params = vec![
            JuliaType::Struct("QuadGK.BatchIntegrand".to_string()),
            JuliaType::Struct("NTuple{N}".to_string()),
            JuliaType::Any,
            JuliaType::Any,
            JuliaType::Any,
            JuliaType::Any,
        ];
        let ntuple_type_params = vec![TypeParam::new("N".to_string())];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &vector_params,
                param_count: 6,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &ntuple_params,
                param_count: 6,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &ntuple_type_params,
            },
        ];
        let actual = strings(&[
            "QuadGK.BatchIntegrand{Float64, Nothing, Vector{Float64}, Vector{Float64}, typeof(f!)}",
            "Tuple{Float64, Float64}",
            "Vector{Float64}",
            "Vector{Float64}",
            "Vector{Float64}",
            "typeof(abs)",
        ]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |actual, param| {
                    runtime_type_name_matches_param(
                        actual,
                        param,
                        |_| true,
                        |actual, expected| {
                            CoreType::from_julia_name(actual)
                                .is_subtype_of(&CoreType::from_julia_name(expected))
                        },
                    )
                },
                |actual, param| param.name() == actual
            )
            .map(|(idx, _)| idx),
            Some(2)
        );

        let mixed_actual = strings(&[
            "QuadGK.BatchIntegrand{Float64, Nothing, Vector{Float64}, Vector{Float64}, typeof(f!)}",
            "Tuple{Float64, Int64}",
            "Vector{Float64}",
            "Vector{Float64}",
            "Vector{Float64}",
            "typeof(abs)",
        ]);
        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &mixed_actual,
                |actual, param| {
                    runtime_type_name_matches_param(
                        actual,
                        param,
                        |_| true,
                        |actual, expected| {
                            CoreType::from_julia_name(actual)
                                .is_subtype_of(&CoreType::from_julia_name(expected))
                        },
                    )
                },
                |actual, param| param.name() == actual
            )
            .map(|(idx, _)| idx),
            None
        );
    }

    #[test]
    fn runtime_value_match_accepts_ntuple_tuple_issue_9563() {
        let hierarchy = StructHierarchy::new();
        let type_params = vec![TypeParam::new("N".to_string())];
        let param = JuliaType::Struct("NTuple{N}".to_string());
        let homogeneous = JuliaType::TupleOf(vec![JuliaType::Float64, JuliaType::Float64]);
        let mut bindings = HashMap::new();

        assert!(runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &homogeneous,
            None,
            &param,
            &type_params,
            &mut bindings,
            || false
        ));
        assert_eq!(bindings.get("N"), Some(&JuliaType::Struct("2".to_string())));

        let mixed = JuliaType::TupleOf(vec![JuliaType::Float64, JuliaType::Int64]);
        let mut bindings = HashMap::new();
        assert!(!runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &mixed,
            None,
            &param,
            &type_params,
            &mut bindings,
            || false
        ));
    }

    #[test]
    fn runtime_value_match_accepts_bounded_ntuple_tuple_issue_9410() {
        let hierarchy = StructHierarchy::new();
        let type_params = vec![TypeParam::new("N".to_string())];
        let param = JuliaType::Struct("NTuple{N, <:Number}".to_string());
        let numeric = JuliaType::TupleOf(vec![JuliaType::Float64, JuliaType::Int64]);
        let mut bindings = HashMap::new();

        assert!(runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &numeric,
            None,
            &param,
            &type_params,
            &mut bindings,
            || false
        ));
        assert_eq!(bindings.get("N"), Some(&JuliaType::Struct("2".to_string())));

        let non_numeric = JuliaType::TupleOf(vec![JuliaType::Float64, JuliaType::String]);
        let mut bindings = HashMap::new();
        assert!(!runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &non_numeric,
            None,
            &param,
            &type_params,
            &mut bindings,
            || false
        ));
    }

    #[test]
    fn runtime_value_match_accepts_bounded_ntuple_complex_issue_9410() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Complex", Some("Number".to_string()), vec!["T".to_string()]);
        let type_params = vec![TypeParam::new("N".to_string())];
        let param = JuliaType::Struct("NTuple{N, <:Number}".to_string());
        let numeric = JuliaType::TupleOf(vec![
            JuliaType::Float64,
            JuliaType::Struct("Complex{Float64}".to_string()),
            JuliaType::Float64,
        ]);
        let mut bindings = HashMap::new();

        assert!(runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &numeric,
            None,
            &param,
            &type_params,
            &mut bindings,
            || false
        ));
        assert_eq!(bindings.get("N"), Some(&JuliaType::Struct("3".to_string())));
    }

    #[test]
    fn runtime_typeof_parametric_struct_binding_rejects_later_mismatch_issue_7460() {
        let hierarchy = StructHierarchy::new();
        let type_params = vec![
            TypeParam::new("M".to_string()),
            TypeParam::new("N".to_string()),
            TypeParam::new("T".to_string()),
        ];
        let target = JuliaType::Struct("StaticArrays.SMatrix{2, 2, Float64}".to_string());
        let type_param = JuliaType::TypeOf(Box::new(JuliaType::Struct(
            "StaticArrays.SMatrix{M, N, T}".to_string(),
        )));
        let mut bindings = HashMap::new();

        assert!(runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &JuliaType::TypeOf(Box::new(target.clone())),
            Some(&target),
            &type_param,
            &type_params,
            &mut bindings,
            || false
        ));
        assert_eq!(bindings.get("M"), Some(&JuliaType::Struct("2".to_string())));
        assert_eq!(bindings.get("N"), Some(&JuliaType::Struct("2".to_string())));
        assert_eq!(bindings.get("T"), Some(&JuliaType::Float64));

        let int_matrix = JuliaType::Struct("StaticArrays.SMatrix{2, 2, Int64}".to_string());
        let same_t_param = JuliaType::Struct("StaticArrays.SMatrix{M, N, T}".to_string());
        assert!(!runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &int_matrix,
            None,
            &same_t_param,
            &type_params,
            &mut bindings,
            || false
        ));
    }

    #[test]
    fn callable_value_candidates_prefer_diagonal_typevar_vararg_issue_8407() {
        let forwarding_params = vec![JuliaType::Any, JuliaType::Any];
        let diagonal_params = vec![JuliaType::Any, JuliaType::TypeVar("T".to_string(), None)];
        let type_params = vec![TypeParam::new("T".to_string())];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &forwarding_params,
                param_count: 2,
                vararg_param_index: Some(1),
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &diagonal_params,
                param_count: 2,
                vararg_param_index: Some(1),
                vararg_fixed_count: None,
                type_params: &type_params,
            },
        ];
        let actual = strings(&["Function", "Float64", "Float64"]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |actual, param| {
                    runtime_type_name_matches_param(
                        actual,
                        param,
                        |_| true,
                        |actual, expected| {
                            CoreType::from_julia_name(actual)
                                .is_subtype_of(&CoreType::from_julia_name(expected))
                        },
                    )
                },
                |actual, param| param.name() == actual
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn julia_signature_reuses_implicit_typevar_bindings() {
        let same_type_params = vec![
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::TypeVar("T".to_string(), None),
        ];

        assert!(julia_signature_match_with_bindings(
            &same_type_params,
            &[JuliaType::BigInt, JuliaType::BigInt],
            &[]
        )
        .is_some());
        assert!(julia_signature_match_with_bindings(
            &same_type_params,
            &[JuliaType::BigInt, JuliaType::Int64],
            &[]
        )
        .is_none());
    }

    #[test]
    fn julia_signature_keeps_anonymous_tuple_bounds_independent_issue_6251() {
        let broad_tuple = vec![JuliaType::TupleOf(vec![
            JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
            JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
        ])];
        let diagonal_tuple = vec![JuliaType::TupleOf(vec![
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::TypeVar("T".to_string(), None),
        ])];
        let diagonal_type_params = vec![TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )];

        assert!(
            julia_signature_match_with_bindings(
                &broad_tuple,
                &[JuliaType::TupleOf(vec![
                    JuliaType::Int64,
                    JuliaType::Float64
                ])],
                &[],
            )
            .is_some(),
            "Tuple{{<:Real,<:Real}} should match mixed real tuple elements independently"
        );
        assert!(
            julia_signature_match_with_bindings(
                &broad_tuple,
                &[JuliaType::TupleOf(vec![
                    JuliaType::Int64,
                    JuliaType::String
                ])],
                &[],
            )
            .is_none(),
            "Tuple{{<:Real,<:Real}} must still reject non-Real elements"
        );
        assert!(
            julia_signature_match_with_bindings(
                &diagonal_tuple,
                &[JuliaType::TupleOf(vec![
                    JuliaType::Int64,
                    JuliaType::Float64
                ])],
                &diagonal_type_params,
            )
            .is_none(),
            "Tuple{{T,T}} where T<:Real must still reject mixed element types"
        );
    }

    #[test]
    fn julia_signature_enforces_nested_diagonal_rule_issue_5050() {
        // nest(x::Vector{T}, y::T) where T: the element type of the vector and
        // the bare argument must share a single concrete `T`.
        let nested_params = vec![
            JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
            JuliaType::TypeVar("T".to_string(), None),
        ];
        let type_params = vec![TypeParam::new("T".to_string())];

        // Vector{Int64} + Int64 -> T = Int64 consistently: accepted.
        assert!(julia_signature_match_with_bindings(
            &nested_params,
            &[
                JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                JuliaType::Int64,
            ],
            &type_params,
        )
        .is_some());

        // Vector{Int64} + Float64 -> T would be both Int64 and Float64: rejected.
        assert!(julia_signature_match_with_bindings(
            &nested_params,
            &[
                JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                JuliaType::Float64,
            ],
            &type_params,
        )
        .is_none());

        // Vector{Float64} + Int64: also rejected (order of conflict is symmetric).
        assert!(julia_signature_match_with_bindings(
            &nested_params,
            &[
                JuliaType::VectorOf(Box::new(JuliaType::Float64)),
                JuliaType::Int64,
            ],
            &type_params,
        )
        .is_none());
    }

    #[test]
    fn core_tuple_cache_key_preserves_parametric_value_and_vararg_shape() {
        assert_eq!(
            core_tuple_signature_from_julia_types(&[
                JuliaType::Struct("Val{1}".to_string()),
                JuliaType::Struct("Array{Int64, 2}".to_string()),
                JuliaType::Struct("Tuple{Vararg{Int64, 3}}".to_string()),
            ]),
            CoreType::Tuple(vec![
                CoreType::from_julia_name("Val{1}"),
                CoreType::from_julia_name("Array{Int64, 2}"),
                CoreType::from_julia_name("Tuple{Vararg{Int64, 3}}"),
            ])
        );
    }

    #[test]
    fn typeof_array_pattern_binds_inner_typevars() {
        let type_params = vec![TypeParam::new("T".to_string())];
        let pattern = vec![JuliaType::TypeOf(Box::new(JuliaType::Struct(
            "Array{T}".to_string(),
        )))];

        assert!(julia_signature_match_with_bindings(
            &pattern,
            &[JuliaType::TypeOf(Box::new(JuliaType::Struct(
                "Array{Int64}".to_string(),
            )))],
            &type_params,
        )
        .is_some());

        assert!(julia_signature_match_with_bindings(
            &pattern,
            &[JuliaType::TypeOf(Box::new(JuliaType::VectorOf(Box::new(
                JuliaType::Float64,
            ))))],
            &type_params,
        )
        .is_none());
    }

    #[test]
    fn typeof_double_bound_enforces_lower_and_upper_invariantly() {
        // Type{T} where Integer<:T<:Real binds T invariantly, so both bounds
        // are enforced (Issue #5051).
        let type_params = vec![TypeParam::with_both_bounds(
            "T".to_string(),
            "Integer".to_string(),
            "Real".to_string(),
        )];
        let pattern = vec![JuliaType::TypeOf(Box::new(JuliaType::TypeVar(
            "T".to_string(),
            Some("Real".to_string()),
        )))];
        let matches = |arg: JuliaType| {
            julia_signature_match_with_bindings(
                &pattern,
                &[JuliaType::TypeOf(Box::new(arg))],
                &type_params,
            )
            .is_some()
        };

        // Within [Integer, Real]: matches.
        assert!(matches(JuliaType::Struct("Integer".to_string())));
        assert!(matches(JuliaType::Struct("Real".to_string())));
        // Below the lower bound (Integer <: Int64 is false): rejected.
        assert!(!matches(JuliaType::Int64));
        assert!(!matches(JuliaType::Float64));
        // Above the upper bound (Number <: Real is false): rejected.
        assert!(!matches(JuliaType::Struct("Number".to_string())));
    }

    #[test]
    fn typeof_lower_bound_only_enforced_invariantly() {
        // Type{T} where T>:Integer: T must be a supertype of Integer
        // (Integer <: T) (Issue #5051).
        let type_params = vec![TypeParam::with_lower_bound(
            "T".to_string(),
            "Integer".to_string(),
        )];
        let pattern = vec![JuliaType::TypeOf(Box::new(JuliaType::Struct(
            "T".to_string(),
        )))];
        let matches = |arg: JuliaType| {
            julia_signature_match_with_bindings(
                &pattern,
                &[JuliaType::TypeOf(Box::new(arg))],
                &type_params,
            )
            .is_some()
        };

        assert!(matches(JuliaType::Struct("Integer".to_string())));
        assert!(matches(JuliaType::Struct("Real".to_string())));
        assert!(matches(JuliaType::Struct("Number".to_string())));
        // Int64 is not a supertype of Integer.
        assert!(!matches(JuliaType::Int64));
    }

    #[test]
    fn covariant_typevar_ignores_lower_bound() {
        // x::T where Integer<:T<:Real binds T covariantly; the lower bound does
        // not restrict matching, so Float64 (not a supertype of Integer) still
        // matches as long as it is <: Real (Issue #5051).
        let type_params = vec![TypeParam::with_both_bounds(
            "T".to_string(),
            "Integer".to_string(),
            "Real".to_string(),
        )];
        let pattern = vec![JuliaType::TypeVar(
            "T".to_string(),
            Some("Real".to_string()),
        )];

        assert!(
            julia_signature_match_with_bindings(&pattern, &[JuliaType::Float64], &type_params,)
                .is_some()
        );
        assert!(
            julia_signature_match_with_bindings(&pattern, &[JuliaType::Int64], &type_params,)
                .is_some()
        );
        // Above the upper bound still rejected even covariantly.
        assert!(
            julia_signature_match_with_bindings(&pattern, &[JuliaType::String], &type_params,)
                .is_none()
        );
    }

    #[test]
    fn score_julia_signature_uses_coretype_exact_and_any_policies() {
        let int_score =
            score_julia_signature(&[JuliaType::Int64], &[JuliaType::Int64], &[], false, false)
                .expect("Int64 should match Int64");
        let any_score =
            score_julia_signature(&[JuliaType::Any], &[JuliaType::Int64], &[], false, false)
                .expect("Any should match Int64");

        assert!(int_score.score > any_score.score);

        let unknown_specific_score =
            score_julia_signature(&[JuliaType::Int64], &[JuliaType::Any], &[], false, false)
                .expect("specific param should still match Any for compile-time fallback");
        let unknown_any_score =
            score_julia_signature(&[JuliaType::Any], &[JuliaType::Any], &[], false, false)
                .expect("Any should match Any");

        assert!(unknown_any_score.score >= unknown_specific_score.score);
    }

    #[test]
    fn score_julia_signature_exact_uppercase_struct_beats_any_issue_5314() {
        let concrete = JuliaType::Struct("Q5314".to_string());
        let concrete_arg = concrete.clone();
        let concrete_score = score_julia_signature(
            std::slice::from_ref(&concrete),
            std::slice::from_ref(&concrete_arg),
            &[],
            false,
            false,
        )
        .expect("concrete struct should match itself");
        let any_score = score_julia_signature(
            &[JuliaType::Any],
            &[JuliaType::Struct("Q5314".to_string())],
            &[],
            false,
            false,
        )
        .expect("Any should match concrete struct");

        assert!(concrete_score.score > any_score.score);
    }

    #[test]
    fn score_julia_signature_rejects_non_exact_type_any_singleton_issue_8438() {
        let bare_type_score = score_julia_signature(
            &[JuliaType::Type],
            &[JuliaType::TypeOf(Box::new(JuliaType::Int64))],
            &[],
            false,
            false,
        )
        .expect("bare Type should match concrete type objects");
        assert!(
            score_julia_signature(
                &[JuliaType::TypeOf(Box::new(JuliaType::Any))],
                &[JuliaType::TypeOf(Box::new(JuliaType::Int64))],
                &[],
                false,
                false,
            )
            .is_none(),
            "Type{{Any}} must not match a non-Any type object"
        );

        let exact_any_score = score_julia_signature(
            &[JuliaType::TypeOf(Box::new(JuliaType::Any))],
            &[JuliaType::TypeOf(Box::new(JuliaType::Any))],
            &[],
            false,
            false,
        )
        .expect("Type{Any} should exactly match Any");
        assert!(exact_any_score.score > bare_type_score.score);
    }

    #[test]
    fn score_julia_signature_prefers_exact_type_any_over_typevar_issue_4574() {
        let exact_any_score = score_julia_signature(
            &[
                JuliaType::TypeOf(Box::new(JuliaType::Any)),
                JuliaType::Tuple,
            ],
            &[
                JuliaType::TypeOf(Box::new(JuliaType::Any)),
                JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]),
            ],
            &[],
            false,
            false,
        )
        .expect("Type{Any}, Tuple should match Any and tuple dims");
        let generic_typevar_score = score_julia_signature(
            &[
                JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                JuliaType::Tuple,
            ],
            &[
                JuliaType::TypeOf(Box::new(JuliaType::Any)),
                JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]),
            ],
            &[TypeParam::new("T".to_string())],
            false,
            false,
        )
        .expect("Type{T}, Tuple should also match Any and tuple dims");

        assert!(exact_any_score.score > generic_typevar_score.score);
    }

    #[test]
    fn score_julia_signature_prefers_typevar_over_non_exact_type_any_issue_4577() {
        assert!(
            score_julia_signature(
                &[
                    JuliaType::TypeOf(Box::new(JuliaType::Any)),
                    JuliaType::Tuple,
                ],
                &[
                    JuliaType::TypeOf(Box::new(JuliaType::Symbol)),
                    JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]),
                ],
                &[],
                false,
                false,
            )
            .is_none(),
            "Type{{Any}}, Tuple must not match Symbol and tuple dims"
        );
        let generic_typevar_score = score_julia_signature(
            &[
                JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                JuliaType::Tuple,
            ],
            &[
                JuliaType::TypeOf(Box::new(JuliaType::Symbol)),
                JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]),
            ],
            &[TypeParam::new("T".to_string())],
            false,
            false,
        )
        .expect("Type{T}, Tuple should match Symbol and tuple dims");

        assert!(generic_typevar_score.score > 0);
    }

    #[test]
    fn type_object_actual_does_not_match_value_level_parametric_pattern_issue_6251() {
        let actual = [JuliaType::TypeOf(Box::new(JuliaType::VectorOf(Box::new(
            JuliaType::Int64,
        ))))];
        let type_params = [TypeParam::new("T".to_string())];

        assert!(
            julia_signature_match_with_bindings(
                &[JuliaType::Struct("Array{T, 1}".to_string())],
                &actual,
                &type_params,
            )
            .is_none(),
            "a type object argument must not satisfy a value-level Array{{T,1}} parameter"
        );
        assert!(
            julia_signature_match_with_bindings(
                &[JuliaType::TypeOf(Box::new(JuliaType::Struct(
                    "LinRange{T}".to_string(),
                )))],
                &actual,
                &type_params,
            )
            .is_none(),
            "Type{{LinRange{{T}}}} must not satisfy Type{{Vector{{Int64}}}} via range projection"
        );
        assert!(
            julia_signature_match_with_bindings(
                &[JuliaType::TypeOf(Box::new(JuliaType::VectorOf(Box::new(
                    JuliaType::TypeVar("T".to_string(), None),
                ))))],
                &actual,
                &type_params,
            )
            .is_some(),
            "the same type object should still satisfy Type{{Vector{{T}}}}"
        );
    }

    #[test]
    fn bounded_typevar_param_outranks_untyped_any_issue_5375() {
        // `h(x::T) where {T<:Number}` must be ranked strictly more specific than
        // the untyped fallback `h(x)` when called as `h(5)`. Previously the
        // bounded type variable scored 0 (the bound was ignored) while the
        // untyped `Any` param earned the `type_reuse_bonus`, so the fallback won.
        let bounded = score_julia_signature(
            &[JuliaType::TypeVar(
                "T".to_string(),
                Some("Number".to_string()),
            )],
            &[JuliaType::Int64],
            &[TypeParam::with_upper_bound(
                "T".to_string(),
                "Number".to_string(),
            )],
            false,
            false,
        )
        .expect("bounded T<:Number must match an Int64 argument");
        let untyped =
            score_julia_signature(&[JuliaType::Any], &[JuliaType::Int64], &[], false, false)
                .expect("untyped Any must match an Int64 argument");
        assert!(
            bounded.score > untyped.score,
            "bounded T<:Number ({}) must outrank untyped Any ({})",
            bounded.score,
            untyped.score
        );
    }

    #[test]
    fn bounded_typevar_param_loses_to_tighter_concrete_param_issue_5375() {
        // The bound must not be over-weighted: a concrete `Int64` parameter is a
        // subtype of `Number`, so it stays at least as specific as `T<:Number`.
        let bounded = score_julia_signature(
            &[JuliaType::TypeVar(
                "T".to_string(),
                Some("Number".to_string()),
            )],
            &[JuliaType::Int64],
            &[TypeParam::with_upper_bound(
                "T".to_string(),
                "Number".to_string(),
            )],
            false,
            false,
        )
        .expect("bounded T<:Number must match an Int64 argument");
        let concrete =
            score_julia_signature(&[JuliaType::Int64], &[JuliaType::Int64], &[], false, false)
                .expect("Int64 must match an Int64 argument");
        assert!(
            concrete.score >= bounded.score,
            "concrete Int64 ({}) must stay at least as specific as T<:Number ({})",
            concrete.score,
            bounded.score
        );
    }

    #[test]
    fn bounded_typevar_any_bound_does_not_outrank_untyped_issue_5375() {
        // `T<:Any` is equivalent to an unbounded `T` (≡ `Any`), so it must not be
        // scored above an untyped fallback parameter (review hardening for #5375).
        let any_bound = score_julia_signature(
            &[JuliaType::TypeVar("T".to_string(), Some("Any".to_string()))],
            &[JuliaType::Int64],
            &[TypeParam::with_upper_bound(
                "T".to_string(),
                "Any".to_string(),
            )],
            false,
            false,
        )
        .expect("T<:Any must match an Int64 argument");
        let untyped =
            score_julia_signature(&[JuliaType::Any], &[JuliaType::Int64], &[], false, false)
                .expect("untyped Any must match an Int64 argument");
        assert!(
            any_bound.score <= untyped.score,
            "T<:Any ({}) must not outrank untyped Any ({})",
            any_bound.score,
            untyped.score
        );
    }

    #[test]
    fn user_abstract_with_builtin_parent_outranks_parent_issue_5582() {
        let abstract_irrational =
            JuliaType::AbstractUser("AbstractIrrational".to_string(), Some("Real".to_string()));
        assert!(
            value_param_base_specificity(&abstract_irrational)
                > value_param_base_specificity(&JuliaType::Real),
            "AbstractIrrational <: Real must score above the Real fallback"
        );

        let any_rooted = JuliaType::AbstractUser("MyAbstract".to_string(), Some("Any".to_string()));
        assert_eq!(
            value_param_base_specificity(&any_rooted),
            1,
            "Any-rooted user abstracts keep the previous flat abstract score"
        );
    }

    /// Issue #6594: pin the *exact* `value_param_base_specificity` scores for the
    /// full `AbstractUser` parent matrix BEFORE migrating the legacy
    /// `JuliaType::from_name(parent)` string parse to structured `CoreType`
    /// matching. The structured replacement must reproduce every value here.
    #[test]
    fn user_abstract_base_specificity_parent_matrix_issue_6594() {
        let cases: &[(Option<&str>, u32)] = &[
            // No declared parent: the structural `CoreType::AbstractUser` floor.
            (None, 1),
            // `Any` parent collapses to the flat abstract floor (≡ unbounded).
            (Some("Any"), 1),
            // Built-in abstract parents add 1 to the parent's CoreType specificity.
            (Some("Number"), 2),  // Number spec 1 -> 2
            (Some("Real"), 3),    // Real spec 2 -> 3
            (Some("Integer"), 4), // Integer spec 3 -> 4
            // Built-in abstract container parents (resolve via `from_name`).
            (Some("AbstractVector"), 2), // AbstractVector spec 1 -> 2
            (Some("AbstractArray"), 2),  // AbstractArray spec 1 -> 2
            // Unknown / user-abstract parent names are NOT resolvable by
            // `from_name`, so the legacy path falls through to the flat floor.
            (Some("Animal"), 1),
            (Some("MyOtherAbstract"), 1),
        ];
        for (parent, expected) in cases {
            let ty =
                JuliaType::AbstractUser("MyAbstract".to_string(), parent.map(|p| p.to_string()));
            assert_eq!(
                value_param_base_specificity(&ty),
                *expected,
                "AbstractUser parent {parent:?} must score {expected}"
            );
        }
    }

    /// Issue #6594: pin the exact-name tier-4 bridge that the legacy rendered
    /// parse provided for `AbstractUser`/`Module` candidate slots. A structured
    /// `AbstractUser`/`Module` slot must score 4 (exact) against the rendered
    /// runtime name's parsed `Named(_)` image, while a child user struct still
    /// passes the structured signature gate through the shared subtype engine.
    #[test]
    fn user_abstract_and_module_keep_exact_name_tier4_issue_6594() {
        // AbstractUser: structured slot scores tier-4 against the rendered name.
        let user_abstract = JuliaType::AbstractUser("Animal".to_string(), Some("Any".to_string()));
        let structural = runtime_candidate_core_type(&user_abstract, &user_abstract.to_string());
        let rendered = CoreType::from_julia_name(&user_abstract.to_string());
        assert!(matches!(structural, CoreType::AbstractUser { .. }));
        assert_eq!(
            structural.dispatch_pattern_score(&rendered),
            4,
            "AbstractUser slot must keep the exact-name tier-4 bridge"
        );

        // Child user structs still pass the structured signature gate.
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Dog", Some("Animal".to_string()), vec![]);
        hierarchy.insert("Animal", Some("Any".to_string()), vec![]);
        let dog = CoreType::from_julia_name("Dog");
        assert!(
            CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(&dog, &structural),
            "Dog <: Animal must hold through the shared subtype engine"
        );
        assert!(
            !CoreSubtypeEngine::with_hierarchy(&hierarchy)
                .is_subtype(&CoreType::from_julia_name("Int64"), &structural),
            "Int64 must not be admitted by the AbstractUser slot"
        );

        // Module: structured slot scores tier-4 against the rendered name.
        let module = JuliaType::Module;
        let module_core = runtime_candidate_core_type(&module, &module.to_string());
        let module_rendered = CoreType::from_julia_name(&module.to_string());
        assert!(matches!(module_core, CoreType::Module(_)));
        assert_eq!(
            module_core.dispatch_pattern_score(&module_rendered),
            4,
            "Module slot must keep the exact-name tier-4 bridge"
        );
    }

    /// Issue #8999: the production runtime `CallDynamic*` family no longer uses
    /// the old string-channel candidate resolver; the structured resolver owns
    /// the runtime order directly.
    #[test]
    fn structured_resolver_orders_runtime_core_candidates_issue_8999() {
        let hierarchy = StructHierarchy::new();
        let core_slots = [
            (1usize, CoreType::from_julia_name("Real")),
            (2usize, CoreType::from_julia_name("Rational")),
            (3usize, CoreType::from_julia_name("Rational{T}")),
            (4usize, CoreType::from_julia_name("Rational{Int64}")),
        ];
        let actual_cores = [CoreType::from_julia_name("Rational{Int64}")];
        let core_result = resolve_runtime_core_signature_candidates(
            &hierarchy,
            core_slots.iter().map(|(idx, slot)| RuntimeCoreCandidate {
                idx: *idx,
                slots: [slot],
                signature: None,
            }),
            &actual_cores,
            |actual, expected| actual.is_subtype_of(expected),
        );

        assert_eq!(core_result, Some((4, 4)));
    }

    /// Issue #9340: when two candidates match only via subtype fallback with the
    /// same structural score, the narrower pattern must win. `Rational` is a
    /// declared subtype of `Real`, and `AbstractFloat` is narrower than `Real`, so
    /// `(Rational, AbstractFloat)` must outrank `(Real, Real)` and
    /// `(Number, Number)` for `Rational{Int64}, Float64`.
    #[test]
    fn structured_resolver_breaks_subtype_fallback_ties_by_specificity_issue_9340() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Rational", Some("Real".to_string()), vec!["T".to_string()]);

        let number = CoreType::from_julia_name("Number");
        let real = CoreType::from_julia_name("Real");
        let rational = CoreType::from_julia_name("Rational");
        let abstract_float = CoreType::from_julia_name("AbstractFloat");
        let actual = [
            CoreType::from_julia_name("Rational{Int64}"),
            CoreType::from_julia_name("Float64"),
        ];

        let result = resolve_runtime_core_signature_candidates(
            &hierarchy,
            [
                RuntimeCoreCandidate {
                    idx: 1,
                    slots: [&number, &number],
                    signature: None,
                },
                RuntimeCoreCandidate {
                    idx: 2,
                    slots: [&real, &real],
                    signature: None,
                },
                RuntimeCoreCandidate {
                    idx: 3,
                    slots: [&rational, &abstract_float],
                    signature: None,
                },
            ],
            &actual,
            |actual, expected| {
                CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
            },
        );

        assert_eq!(result, Some((3, 4)));
    }

    /// Issues #11036/#11230: independently loaded dependencies can reverse
    /// global function-index order. The production runtime represents package
    /// abstract parameters as `AbstractUser` patterns with structured parents;
    /// genuine user abstracts must outrank canonical built-in aliases regardless
    /// of which candidate was stored first.
    #[test]
    fn structured_resolver_user_abstract_specificity_is_order_independent_issue_11230() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert(
            "StaticArrays.SMatrix",
            Some("StaticArrays.StaticMatrix{M,N,T}".to_string()),
            vec![
                "M".to_string(),
                "N".to_string(),
                "T".to_string(),
                "L".to_string(),
            ],
        );
        hierarchy.insert(
            "SVector",
            Some("StaticArrays.StaticVector{N,T}".to_string()),
            vec!["N".to_string(), "T".to_string()],
        );
        hierarchy.insert(
            "StaticArrays.StaticMatrix",
            Some("StaticArrays.StaticVecOrMat{Tuple{M,N},T,2}".to_string()),
            vec!["M".to_string(), "N".to_string(), "T".to_string()],
        );
        hierarchy.insert(
            "StaticArrays.StaticVector",
            Some("StaticArrays.StaticVecOrMat{Tuple{N},T,1}".to_string()),
            vec!["N".to_string(), "T".to_string()],
        );
        hierarchy.insert(
            "StaticArrays.StaticVecOrMat",
            Some("StaticArrays.StaticArray{S,T,N}".to_string()),
            vec!["S".to_string(), "T".to_string(), "N".to_string()],
        );
        hierarchy.insert(
            "StaticArrays.StaticArray",
            Some("AbstractArray{T,N}".to_string()),
            vec!["S".to_string(), "T".to_string(), "N".to_string()],
        );

        let abstract_array = CoreType::from_julia_name("AbstractArray");
        let generic_matrix = CoreType::AbstractUser {
            name: "AbstractMatrix".to_string(),
            parent: Some(Box::new(abstract_array.clone())),
        };
        let generic_vector = CoreType::AbstractUser {
            name: "AbstractVector".to_string(),
            parent: Some(Box::new(abstract_array)),
        };
        let static_matrix = CoreType::AbstractUser {
            name: "StaticMatrix".to_string(),
            parent: Some(Box::new(CoreType::Struct {
                name: "StaticVecOrMat".to_string(),
                params: vec![
                    CoreType::Tuple(vec![
                        CoreType::Named("M".to_string()),
                        CoreType::Named("N".to_string()),
                    ]),
                    CoreType::Named("T".to_string()),
                    CoreType::from_julia_name("2"),
                ],
            })),
        };
        let static_vector = CoreType::AbstractUser {
            name: "StaticVector".to_string(),
            parent: Some(Box::new(CoreType::Struct {
                name: "StaticVecOrMat".to_string(),
                params: vec![
                    CoreType::Tuple(vec![CoreType::Named("N".to_string())]),
                    CoreType::Named("T".to_string()),
                    CoreType::from_julia_name("1"),
                ],
            })),
        };
        let generic_signature =
            CoreType::Tuple(vec![generic_matrix.clone(), generic_vector.clone()]);
        let static_signature = CoreType::Tuple(vec![static_matrix.clone(), static_vector.clone()]);
        let actual = [
            CoreType::from_julia_name("StaticArrays.SMatrix{3,3,Float64}"),
            CoreType::from_julia_name("SVector{3,Float64}"),
        ];

        assert!(
            core_signature_pattern_specificity(&[static_matrix.clone(), static_vector.clone()])
                > core_signature_pattern_specificity(&[
                    generic_matrix.clone(),
                    generic_vector.clone(),
                ])
        );
        assert!(static_signature
            .strict_subtype_dominates_with_hierarchy(&generic_signature, &hierarchy));

        let generic = RuntimeCoreCandidate {
            idx: 1,
            slots: [&generic_matrix, &generic_vector],
            signature: Some(&generic_signature),
        };
        let specific = RuntimeCoreCandidate {
            idx: 2,
            slots: [&static_matrix, &static_vector],
            signature: Some(&static_signature),
        };
        assert_candidate_pair_order_independent(
            "fixed runtime",
            [generic, specific],
            Some(2),
            |candidates| {
                resolve_runtime_core_signature_candidates(
                    &hierarchy,
                    candidates,
                    &actual,
                    |actual, expected| {
                        CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                    },
                )
                .map(|(idx, _)| idx)
            },
        );

        let generic_slots = [generic_matrix.clone(), generic_vector.clone()];
        let specific_slots = [static_matrix.clone(), static_vector.clone()];
        let generic_slice = RuntimeCoreSliceCandidate {
            idx: 1,
            slots: &generic_slots,
            signature: Some(&generic_signature),
        };
        let specific_slice = RuntimeCoreSliceCandidate {
            idx: 2,
            slots: &specific_slots,
            signature: Some(&static_signature),
        };
        assert_candidate_pair_order_independent(
            "slice runtime",
            [generic_slice, specific_slice],
            Some(2),
            |candidates| {
                resolve_runtime_core_signature_slice_candidates_with_family_fallback(
                    &hierarchy,
                    candidates,
                    &actual,
                    |_, _| false,
                    |actual, expected| {
                        CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                    },
                )
                .map(|(idx, _)| idx)
            },
        );

        let generic_rendered = strings(&["AbstractMatrix", "AbstractVector"]);
        let specific_rendered = strings(&["StaticMatrix", "StaticVector"]);
        let generic_typed = RuntimeTypedCoreCandidate {
            idx: 1,
            rendered: &generic_rendered,
            slots: &generic_slots,
            signature: Some(&generic_signature),
        };
        let specific_typed = RuntimeTypedCoreCandidate {
            idx: 2,
            rendered: &specific_rendered,
            slots: &specific_slots,
            signature: Some(&static_signature),
        };
        assert_candidate_pair_order_independent(
            "typed runtime",
            [generic_typed, specific_typed],
            Some(2),
            |candidates| {
                resolve_typed_runtime_core_candidates_with_subtype_fallback(
                    &hierarchy,
                    candidates,
                    &actual,
                    |actual, expected| {
                        CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                    },
                )
                .map(|(idx, _)| idx)
            },
        );

        let generic_callable_params = vec![
            JuliaType::AbstractUser(
                "AbstractMatrix".to_string(),
                Some("AbstractArray".to_string()),
            ),
            JuliaType::AbstractUser(
                "AbstractVector".to_string(),
                Some("AbstractArray".to_string()),
            ),
        ];
        let specific_callable_params = vec![
            JuliaType::AbstractUser(
                "StaticMatrix".to_string(),
                Some("StaticVecOrMat{Tuple{M,N},T,2}".to_string()),
            ),
            JuliaType::AbstractUser(
                "StaticVector".to_string(),
                Some("StaticVecOrMat{Tuple{N},T,1}".to_string()),
            ),
        ];
        let generic_callable = CallableValueCandidate {
            idx: 1,
            param_types: &generic_callable_params,
            param_count: 2,
            vararg_param_index: None,
            vararg_fixed_count: None,
            type_params: &[],
        };
        let specific_callable = CallableValueCandidate {
            idx: 2,
            param_types: &specific_callable_params,
            param_count: 2,
            vararg_param_index: None,
            vararg_fixed_count: None,
            type_params: &[],
        };
        let actual_names = strings(&["StaticArrays.SMatrix{3,3,Float64}", "SVector{3,Float64}"]);
        assert_candidate_pair_order_independent(
            "callable value",
            [generic_callable, specific_callable],
            Some(2),
            |candidates| {
                resolve_callable_value_candidates(
                    &hierarchy,
                    candidates,
                    &actual_names,
                    |actual, expected| {
                        let actual = CoreType::from_julia_name_for_dispatch(actual);
                        let expected = runtime_candidate_core_type(expected, &expected.to_string());
                        CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(&actual, &expected)
                    },
                    |_, _| false,
                )
                .map(|(idx, _)| idx)
            },
        );
    }

    #[test]
    fn parametric_user_abstract_specificity_beats_bare_family_issue_7960() {
        let bare = [CoreType::AbstractUser {
            name: "AbsM".to_string(),
            parent: None,
        }];
        let parametric = [CoreType::AbstractUser {
            name: "AbsM{2, 2, T}".to_string(),
            parent: None,
        }];

        assert!(
            core_signature_pattern_specificity(&parametric)
                > core_signature_pattern_specificity(&bare)
        );
    }

    #[test]
    fn runtime_value_match_projects_concrete_parent_value_params_issue_7960() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert(
            "AbsM",
            Some("Any".to_string()),
            vec!["M".to_string(), "N".to_string(), "T".to_string()],
        );
        hierarchy.insert(
            "ConM",
            Some("AbsM{M,N,T}".to_string()),
            vec!["M".to_string(), "N".to_string(), "T".to_string()],
        );
        let type_params = [TypeParam::new("T".to_string())];
        let pattern = JuliaType::AbstractUser("AbsM{2, 2, T}".to_string(), Some("Any".to_string()));
        let actual = JuliaType::Struct("ConM{2, 2, Float64}".to_string());
        let mut bindings = HashMap::new();

        assert!(runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &actual,
            None,
            &pattern,
            &type_params,
            &mut bindings,
            || false,
        ));
        assert_eq!(bindings.get("T"), Some(&JuliaType::Float64));

        let wrong_shape = JuliaType::Struct("ConM{3, 3, Float64}".to_string());
        let mut wrong_bindings = HashMap::new();
        assert!(!runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &wrong_shape,
            None,
            &pattern,
            &type_params,
            &mut wrong_bindings,
            || false,
        ));
        assert!(wrong_bindings.is_empty());
    }

    #[test]
    fn runtime_core_candidate_projects_parametric_abstract_issue_7960() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert(
            "AbsM",
            Some("Any".to_string()),
            vec!["M".to_string(), "N".to_string(), "T".to_string()],
        );
        hierarchy.insert(
            "ConM",
            Some("AbsM{M,N,T}".to_string()),
            vec!["M".to_string(), "N".to_string(), "T".to_string()],
        );
        let type_params = [TypeParam::new("T".to_string())];
        let abstract_slot = embed_type_param_bounds(
            runtime_candidate_core_type(
                &JuliaType::AbstractUser("AbsM{2, 2, T}".to_string(), Some("Any".to_string())),
                "AbsM{2, 2, T}",
            ),
            &type_params,
        );
        let abstract_signature =
            runtime_core_signature(std::slice::from_ref(&abstract_slot), &type_params);
        let generic_slot = CoreType::Any;

        let matching_actual = [CoreType::from_julia_name("ConM{2, 2, Float64}")];
        let matching = resolve_runtime_core_signature_candidates(
            &hierarchy,
            [
                RuntimeCoreCandidate {
                    idx: 1,
                    slots: [&generic_slot],
                    signature: None,
                },
                RuntimeCoreCandidate {
                    idx: 2,
                    slots: [&abstract_slot],
                    signature: Some(&abstract_signature),
                },
            ],
            &matching_actual,
            |actual, expected| {
                CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
            },
        );
        assert_eq!(matching, Some((2, 3)));

        let wrong_shape_actual = [CoreType::from_julia_name("ConM{3, 3, Float64}")];
        let wrong_shape = resolve_runtime_core_signature_candidates(
            &hierarchy,
            [
                RuntimeCoreCandidate {
                    idx: 1,
                    slots: [&generic_slot],
                    signature: None,
                },
                RuntimeCoreCandidate {
                    idx: 2,
                    slots: [&abstract_slot],
                    signature: Some(&abstract_signature),
                },
            ],
            &wrong_shape_actual,
            |actual, expected| {
                CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
            },
        );
        assert_eq!(wrong_shape, Some((1, 1)));
    }

    /// Issue #6502 residual slice: the structured fallback resolver keeps the
    /// legacy same-family tier for native/legacy sentinel names without
    /// returning to string-encoded candidate matching.
    #[test]
    fn structured_slice_resolver_uses_family_fallback_issue_6502() {
        let hierarchy = StructHierarchy::new();
        let sentinel = CoreType::Named("Base.Generator".to_string());
        let catch_all = CoreType::Any;
        let actual = CoreType::Struct {
            name: "Base.Generator".to_string(),
            params: vec![CoreType::Any],
        };
        let actual_cores = [actual];

        let result = resolve_runtime_core_signature_slice_candidates_with_family_fallback(
            &hierarchy,
            [
                RuntimeCoreSliceCandidate {
                    idx: usize::MAX,
                    slots: std::slice::from_ref(&sentinel),
                    signature: None,
                },
                RuntimeCoreSliceCandidate {
                    idx: 2,
                    slots: std::slice::from_ref(&catch_all),
                    signature: None,
                },
            ],
            &actual_cores,
            // Issue #6593: structured family match via the `core_signature`
            // accessor, not a `to_julia_name()` string round-trip.
            |actual, expected| match (actual.nominal_family_name(), expected.nominal_family_name())
            {
                (Some(actual_family), Some(expected_family)) => actual_family == expected_family,
                _ => false,
            },
            |actual, expected| {
                CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
            },
        );

        assert_eq!(result, Some((usize::MAX, 2)));
    }

    /// Issue #6502: family fallback must not admit parametric candidates that
    /// the old string fallback intentionally rejected via
    /// `core_type_allows_family_fallback`.
    #[test]
    fn structured_slice_family_fallback_rejects_parametric_expected_issue_6502() {
        let hierarchy = StructHierarchy::new();
        let expected = CoreType::Struct {
            name: "Box".to_string(),
            params: vec![CoreType::Any],
        };
        let actual = CoreType::Struct {
            name: "Box".to_string(),
            params: vec![CoreType::from_julia_name("String")],
        };
        let actual_cores = [actual];

        let result = resolve_runtime_core_signature_slice_candidates_with_family_fallback(
            &hierarchy,
            [RuntimeCoreSliceCandidate {
                idx: 1,
                slots: std::slice::from_ref(&expected),
                signature: None,
            }],
            &actual_cores,
            |_, _| true,
            |_, _| false,
        );

        assert_eq!(result, None);
    }

    /// Issue #11076: the legacy bare-family fallback is for representation
    /// sentinels only. An explicitly-qualified sibling owner must not re-enter
    /// the candidate set after the primary nominal matcher rejects it.
    #[test]
    fn structured_slice_family_fallback_rejects_qualified_sibling_owner_issue_11076() {
        let hierarchy = StructHierarchy::new();
        let expected = CoreType::Struct {
            name: "OwnerB11076.Box".to_string(),
            params: vec![],
        };
        let actual = CoreType::Struct {
            name: "OwnerA11076.Box".to_string(),
            params: vec![],
        };

        let result = resolve_runtime_core_signature_slice_candidates_with_family_fallback(
            &hierarchy,
            [RuntimeCoreSliceCandidate {
                idx: 1,
                slots: std::slice::from_ref(&expected),
                signature: None,
            }],
            std::slice::from_ref(&actual),
            |actual, expected| actual.nominal_family_name() == expected.nominal_family_name(),
            |_, _| false,
        );

        assert_eq!(result, None);
    }

    /// Issue #6536: `where`-clause bounds embedded into parametric slots are
    /// enforced — `Wrap{T} where T<:Real` must reject `Wrap{String}` and keep
    /// the tier-3 parametric score for `Wrap{Int64}`.
    #[test]
    fn structured_resolver_enforces_embedded_bounds_issue_6536() {
        let hierarchy = StructHierarchy::new();
        let type_params = [TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )];
        let raw = CoreType::from_julia_name("Wrap{T}");
        let bounded = embed_type_param_bounds(raw.clone(), &type_params);
        assert_ne!(bounded, raw, "bound must be embedded into the typevar");

        let signature = runtime_core_signature(std::slice::from_ref(&bounded), &type_params);
        let generic_slot = embed_type_param_bounds(
            CoreType::from_julia_name("Wrap{S}"),
            &[TypeParam::new("S".to_string())],
        );

        let run = |actual_name: &str| {
            let actual_cores = [CoreType::from_julia_name(actual_name)];
            resolve_runtime_core_signature_candidates(
                &hierarchy,
                [
                    RuntimeCoreCandidate {
                        idx: 1,
                        slots: [&bounded],
                        signature: Some(&signature),
                    },
                    RuntimeCoreCandidate {
                        idx: 2,
                        slots: [&generic_slot],
                        signature: None,
                    },
                ],
                &actual_cores,
                |_, _| false,
            )
            .map(|(idx, _)| idx)
        };

        assert_eq!(run("Wrap{Int64}"), Some(1));
        assert_eq!(run("Wrap{String}"), Some(2));
    }

    /// Issue #5137/#6536: multi-letter `where` names such as `MI` are not
    /// parsed as type variables without method context, but runtime candidate
    /// signature building has that context through `type_params` (Issue #5915
    /// cross-credit).
    #[test]
    fn embed_type_param_bounds_recovers_named_multi_letter_typevars_issue_5137() {
        let hierarchy = StructHierarchy::new();
        let type_params = [
            TypeParam::new("T".to_string()),
            TypeParam::new("P".to_string()),
            TypeParam::new("MI".to_string()),
        ];
        let slot = embed_type_param_bounds(
            CoreType::from_julia_name("ReshapedArray{T, 1, P, MI}"),
            &type_params,
        );
        let CoreType::Struct { params, .. } = &slot else {
            panic!("ReshapedArray slot should stay a struct");
        };
        assert!(matches!(params.get(3), Some(CoreType::TypeVar(var)) if var.name == "MI"));

        let signature = runtime_core_signature(std::slice::from_ref(&slot), &type_params);
        let actual = [CoreType::from_julia_name(
            "ReshapedArray{Int64, 1, SubArray{Int64, 2, Matrix{Int64}, Tuple{UnitRange{Int64}, UnitRange{Int64}}, false}, Tuple{}}",
        )];

        let result = resolve_runtime_core_signature_candidates(
            &hierarchy,
            [RuntimeCoreCandidate {
                idx: 1,
                slots: [&slot],
                signature: Some(&signature),
            }],
            &actual,
            |actual, expected| {
                CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
            },
        );

        assert_eq!(result.map(|(idx, _)| idx), Some(1));
    }

    /// Issue #6536: the `core_signature` gate enforces cross-slot typevar
    /// binding consistency — `(Holder{T}, Holder{T}) where T` must reject
    /// `(Holder{Int64}, Holder{String})`.
    #[test]
    fn structured_resolver_enforces_cross_slot_bindings_issue_6536() {
        let hierarchy = StructHierarchy::new();
        let type_params = [TypeParam::new("T".to_string())];
        let slot = embed_type_param_bounds(CoreType::from_julia_name("Holder{T}"), &type_params);
        let signature = runtime_core_signature(&[slot.clone(), slot.clone()], &type_params);
        let bare = CoreType::from_julia_name("Holder");

        let run = |left: &str, right: &str| {
            let actual_cores = [
                CoreType::from_julia_name(left),
                CoreType::from_julia_name(right),
            ];
            resolve_runtime_core_signature_candidates(
                &hierarchy,
                [
                    RuntimeCoreCandidate {
                        idx: 1,
                        slots: [&slot, &slot],
                        signature: Some(&signature),
                    },
                    RuntimeCoreCandidate {
                        idx: 2,
                        slots: [&bare, &bare],
                        signature: None,
                    },
                ],
                &actual_cores,
                // Mirror the VM's `check_subtype_core` fallback (the engine
                // admits bare `Named` family patterns at tier 1).
                |actual, expected| {
                    CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                },
            )
            .map(|(idx, _)| idx)
        };

        assert_eq!(run("Holder{Int64}", "Holder{Int64}"), Some(1));
        assert_eq!(run("Holder{Int64}", "Holder{String}"), Some(2));
    }

    /// Issue #6502/#6536: user-abstract bounds keep the structural tier when
    /// the hierarchy resolves them — `Box{T} where T<:Animal` scores tier 3
    /// for `Box{Dog}` (beating the bare `Box` tier-2 catch-all) and rejects
    /// `Box{Int64}`.
    #[test]
    fn structured_resolver_resolves_user_bounds_through_hierarchy_issue_6536() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Animal", Some("Any".to_string()), Vec::new());
        hierarchy.insert("Dog", Some("Animal".to_string()), Vec::new());
        hierarchy.insert("Box", Some("Any".to_string()), vec!["T".to_string()]);

        let type_params = [TypeParam::with_upper_bound(
            "T".to_string(),
            "Animal".to_string(),
        )];
        let slot = embed_type_param_bounds(CoreType::from_julia_name("Box{T}"), &type_params);
        let signature = runtime_core_signature(&[slot.clone(), slot.clone()], &type_params);
        let bare = CoreType::from_julia_name("Box");

        let run = |name: &str| {
            let actual = CoreType::from_julia_name(name);
            let actual_cores = [actual.clone(), actual];
            resolve_runtime_core_signature_candidates(
                &hierarchy,
                [
                    RuntimeCoreCandidate {
                        idx: 1,
                        slots: [&slot, &slot],
                        signature: Some(&signature),
                    },
                    RuntimeCoreCandidate {
                        idx: 2,
                        slots: [&bare, &bare],
                        signature: None,
                    },
                ],
                &actual_cores,
                // Mirror the VM's `check_subtype_core` fallback.
                |actual, expected| {
                    CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                },
            )
            .map(|(idx, _)| idx)
        };

        assert_eq!(run("Box{Dog}"), Some(1), "tier-3 bounded beats bare tier-2");
        assert_eq!(run("Box{Int64}"), Some(2), "bound rejects non-Animal");
    }

    /// Issue #6502 residual slice: runtime candidate slots are projected from
    /// `JuliaType` structurally, while `CoreType` keeps the exact-name and
    /// subtype behavior that the old rendered-name parse provided for
    /// `AbstractUser` and `Module`.
    #[test]
    fn runtime_candidate_core_type_replaces_legacy_parse_issue_6502() {
        // Faithful shapes: structural == parsed.
        for jt in [
            JuliaType::Int64,
            JuliaType::Real,
            JuliaType::String,
            JuliaType::VectorOf(Box::new(JuliaType::Int64)),
            JuliaType::TypeVar("T".to_string(), Some("Real".to_string())),
            JuliaType::Struct("Wrap{T}".to_string()),
            JuliaType::TypeOf(Box::new(JuliaType::Int64)),
            JuliaType::Union(vec![JuliaType::Int64, JuliaType::Nothing]),
        ] {
            let rendered = jt.to_string();
            assert_eq!(
                CoreType::from(&jt),
                CoreType::from_julia_name(&rendered),
                "expected structural == parsed for {rendered}"
            );
        }

        // User abstract annotations diverge from rendered parsing, but runtime
        // candidates now keep the structured `AbstractUser` image and preserve
        // the old exact-name tier against rendered runtime names via CoreType.
        let user_abstract = JuliaType::AbstractUser("Animal".to_string(), Some("Any".to_string()));
        let rendered = user_abstract.to_string();
        let structural = CoreType::from(&user_abstract);
        let parsed = CoreType::from_julia_name(&rendered);
        assert_ne!(structural, parsed);
        assert_eq!(
            runtime_candidate_core_type(&user_abstract, &rendered),
            structural
        );
        assert_eq!(structural.dispatch_pattern_score(&parsed), 4);

        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Dog", Some("Animal".to_string()), vec![]);
        hierarchy.insert("Animal", Some("Any".to_string()), vec![]);
        let dog = CoreType::from_julia_name("Dog");
        assert!(
            CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(&dog, &structural),
            "child user structs still pass the structured AbstractUser signature gate"
        );

        // Module has the same divergent shape: declared `JuliaType::Module`
        // becomes `CoreType::Module("Module")`, while rendered runtime type
        // names parse as `Named("Module")`. Keep it structural and bridge the
        // exact runtime annotation match in CoreType.
        let module = JuliaType::Module;
        let rendered = module.to_string();
        let structural = CoreType::from(&module);
        let parsed = CoreType::from_julia_name(&rendered);
        assert_ne!(structural, parsed);
        assert_eq!(runtime_candidate_core_type(&module, &rendered), structural);
        assert_eq!(structural.dispatch_pattern_score(&parsed), 4);
        assert!(CoreSubtypeEngine::new().is_subtype(&parsed, &structural));
    }

    #[test]
    fn typemap_verdict_subtype_decides_precise_tuples_issue_8548() {
        let hierarchy = StructHierarchy::new();
        let int64 = CoreType::from_julia_name("Int64");
        let float64 = CoreType::from_julia_name("Float64");
        let number = CoreType::from_julia_name("Number");

        // Exact and abstract-supertype parameters accept via subtyping.
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                std::slice::from_ref(&int64),
                &[],
                std::slice::from_ref(&int64),
            ),
            TypemapVerdict::Accept
        );
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                std::slice::from_ref(&number),
                &[],
                std::slice::from_ref(&int64),
            ),
            TypemapVerdict::Accept
        );
        // A disjoint concrete slot rejects; so does an arity mismatch.
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                std::slice::from_ref(&int64),
                &[],
                std::slice::from_ref(&float64),
            ),
            TypemapVerdict::Reject
        );
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                std::slice::from_ref(&int64),
                &[],
                &[int64.clone(), int64.clone()],
            ),
            TypemapVerdict::Reject
        );
    }

    #[test]
    fn typemap_verdict_enforces_diagonal_where_issue_8548() {
        let hierarchy = StructHierarchy::new();
        let var = CoreTypeVar::unscoped("T");
        let slot = CoreType::TypeVar(var.clone());
        let params = [slot.clone(), slot];
        let vars = [var];
        let int64 = CoreType::from_julia_name("Int64");
        let float64 = CoreType::from_julia_name("Float64");

        assert_eq!(
            typemap_candidate_verdict(&hierarchy, &params, &vars, &[int64.clone(), int64.clone()]),
            TypemapVerdict::Accept
        );
        // The diagonal rule requires both precise slots to agree.
        assert_eq!(
            typemap_candidate_verdict(&hierarchy, &params, &vars, &[int64, float64]),
            TypemapVerdict::Reject
        );
    }

    #[test]
    fn typemap_verdict_defers_imprecise_call_tuples_issue_8548() {
        let hierarchy = StructHierarchy::new();
        let int64 = CoreType::from_julia_name("Int64");
        let params = [int64.clone()];

        // Statically-unknown / upper-bounded-only shapes that may still
        // intersect the candidate stay deferred to the scoring matcher's
        // runtime-deferral policy.
        for imprecise in [CoreType::Any, CoreType::from_julia_name("Integer")] {
            assert_eq!(
                typemap_candidate_verdict(
                    &hierarchy,
                    &params,
                    &[],
                    std::slice::from_ref(&imprecise),
                ),
                TypemapVerdict::DeferImprecise,
                "expected DeferImprecise for {imprecise:?}",
            );
        }
        // `Named` cannot distinguish user structs from user abstracts, so it may
        // overlap a method whose parameter is the same family.
        let dog = CoreType::from_julia_name("Dog8548");
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                std::slice::from_ref(&dog),
                &[],
                std::slice::from_ref(&dog),
            ),
            TypemapVerdict::DeferImprecise
        );

        // Issue #8999: imprecise calls now use conservative intersection in the
        // safe direction, but only for non-nominal shapes whose Bottom result
        // does not need struct hierarchy evidence.
        for imprecise in [
            CoreType::from_julia_name("AbstractFloat"),
            CoreType::from_julia_name("DataType"),
        ] {
            assert_eq!(
                typemap_candidate_verdict(
                    &hierarchy,
                    &params,
                    &[],
                    std::slice::from_ref(&imprecise),
                ),
                TypemapVerdict::Reject,
                "{imprecise:?} and Int64 are proven-disjoint without nominal hierarchy",
            );
        }

        // Nominal/container imprecise shapes keep deferring because
        // `CoreType::type_intersect` does not consult the runtime struct
        // hierarchy (`ReverseOrdering <: Ordering` is the motivating #8999
        // counterexample).
        for imprecise in [
            CoreType::from_julia_name("Rational"),
            CoreType::from_julia_name("Vector{Any}"),
            // Issue #6663: a static `Vector{Bool}` may be a runtime
            // `BitVector` (shared native Bool storage), so `BitArray`
            // methods must stay candidates.
            CoreType::from_julia_name("Vector{Bool}"),
        ] {
            assert_eq!(
                typemap_candidate_verdict(
                    &hierarchy,
                    &params,
                    &[],
                    std::slice::from_ref(&imprecise),
                ),
                TypemapVerdict::DeferImprecise,
                "expected nominal/container DeferImprecise for {imprecise:?}",
            );
        }

        // A single imprecise slot defers the whole call tuple.
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                &[int64.clone(), int64.clone()],
                &[],
                &[int64, CoreType::Any],
            ),
            TypemapVerdict::DeferImprecise
        );
    }

    #[test]
    fn typemap_verdict_defers_unsupported_signatures_issue_8548() {
        let hierarchy = StructHierarchy::new();
        let int64 = CoreType::from_julia_name("Int64");
        let var = CoreTypeVar::unscoped("T");

        // Nested `where`-variable occurrence in an invariant position.
        let wrap_t = CoreType::Struct {
            name: "Wrap8548".to_string(),
            params: vec![CoreType::TypeVar(var.clone())],
        };
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                &[wrap_t.clone(), wrap_t],
                std::slice::from_ref(&var),
                &[int64.clone(), int64.clone()],
            ),
            TypemapVerdict::DeferSignature
        );

        // Anonymous covariant bound inside a container (`Vector{<:Real}`).
        let vec_le_real = CoreType::Struct {
            name: "Vector".to_string(),
            params: vec![CoreType::TypeVar(CoreTypeVar::with_bounds(
                "_",
                None,
                Some(Box::new(CoreType::from_julia_name("Real"))),
            ))],
        };
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                std::slice::from_ref(&vec_le_real),
                &[],
                &[CoreType::from_julia_name("Vector{Int64}")],
            ),
            TypemapVerdict::DeferSignature
        );

        // Lower-bounded `where` clause.
        let lower = CoreTypeVar::with_bounds("T", Some(Box::new(int64.clone())), None);
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                &[CoreType::TypeVar(lower.clone())],
                std::slice::from_ref(&lower),
                &[CoreType::from_julia_name("Float64")],
            ),
            TypemapVerdict::DeferSignature
        );

        // An `Any` in an invariant parameter position (`Vector{Any}`): the
        // scoring matcher deliberately treats it as the erased bare family
        // (a `Vector{String}` argument selects the `Vector{Any}` method —
        // Issue #2352 fixture semantics), so the filter defers instead of
        // enforcing upstream invariance.
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                &[CoreType::Struct {
                    name: "Vector".to_string(),
                    params: vec![CoreType::Any],
                }],
                &[],
                &[CoreType::from_julia_name("Vector{String}")],
            ),
            TypemapVerdict::DeferSignature
        );

        // Anonymous bounded tuple elements (`Tuple{<:Real, <:Real}`) are
        // independent fallback bounds (Issue #6251); the engine's pattern
        // matcher would bind the same-named anonymous variables diagonally
        // and reject `(1, 2.0)`, so the filter defers.
        let anon_real = CoreType::TypeVar(CoreTypeVar::with_bounds(
            "_",
            None,
            Some(Box::new(CoreType::from_julia_name("Real"))),
        ));
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                &[CoreType::Tuple(vec![anon_real.clone(), anon_real])],
                &[],
                &[CoreType::Tuple(vec![
                    int64.clone(),
                    CoreType::from_julia_name("Float64"),
                ])],
            ),
            TypemapVerdict::DeferSignature
        );

        // An abstract element in an invariant position (`Vector{Number}`):
        // the scoring matcher accepts a `Vector{Int64}` argument loosely
        // (abstract container coercion), so the filter defers instead of
        // enforcing upstream invariance.
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                &[CoreType::Struct {
                    name: "Vector".to_string(),
                    params: vec![CoreType::from_julia_name("Number")],
                }],
                &[],
                &[CoreType::from_julia_name("Vector{Int64}")],
            ),
            TypemapVerdict::DeferSignature
        );

        // A bare parametric family under `Type{...}` (`Type{Vector}`,
        // `Type{Array{Pair}}`): the engine's bare-family acceptance is
        // bidirectionally loose under `Type` (compare evidence:
        // `eltype(::Type{Vector})` accepted a `Type{Matrix{Float64}}`
        // argument), so the filter defers.
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                &[CoreType::TypeOf(Box::new(CoreType::Struct {
                    name: "Vector".to_string(),
                    params: Vec::new(),
                }))],
                &[],
                &[CoreType::TypeOf(Box::new(CoreType::from_julia_name(
                    "Matrix{Float64}",
                )))],
            ),
            TypemapVerdict::DeferSignature
        );

        // NOTE (Issue #8817): `Type{Union{...}}` and `Vector{Union{...}}`
        // signatures are NO LONGER deferred. The engine was fixed by
        // Issue #8582 to handle ground Union in invariant positions
        // correctly, and `typemap_slot_supported` was updated to use
        // `core_type_is_sig_invariant_ground` for those slots. The correct
        // verdict for a `Type{Int64}` argument against a
        // `Type{Union{Nothing,Int64}}` signature is `Reject` (invariant
        // comparison: `Int64 !== Union{Nothing,Int64}`).
        // Regressions for this case are covered by the fixture
        // `dispatch/nested_union_typemap_dispatch_8817.jl`.
    }

    #[test]
    fn typemap_verdict_ground_union_in_sig_retired_issue_8817() {
        // Issue #8817: `Type{Union{...}}` and `Vector{Union{...}}` signatures
        // are no longer deferred after introducing `core_type_is_sig_invariant_ground`.
        // The engine (fixed by Issue #8582) correctly handles ground Union in
        // invariant positions.
        let hierarchy = StructHierarchy::new();
        let int64 = CoreType::from_julia_name("Int64");
        let nothing = CoreType::Primitive(CorePrimitive::Nothing);

        // --- Type{Union{Nothing,Int64}} signature ---
        //
        // `Type{Union{Nothing,Int64}}` in a signature is now "sig-invariant-ground"
        // (owned by the engine). On the arg side, `Type{Union{Nothing,Int64}}`
        // is dispatch-precise because `core_type_is_ground(Union{...})` is true.
        let union_ty = CoreType::Union(vec![nothing.clone(), int64.clone()]);
        let type_of_union = CoreType::TypeOf(Box::new(union_ty.clone()));

        // Arg = Type{Int64}: invariant, Int64 ≠ Union{Nothing,Int64} → Reject.
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                std::slice::from_ref(&type_of_union),
                &[],
                &[CoreType::TypeOf(Box::new(int64.clone()))],
            ),
            TypemapVerdict::Reject
        );
        // Arg = Type{Union{Nothing,Int64}}: exact match → Accept.
        // (Type{Union{...}} IS dispatch-precise: `core_type_is_ground` accepts
        // ground Union, so the arg is not deferred as imprecise.)
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                std::slice::from_ref(&type_of_union),
                &[],
                std::slice::from_ref(&type_of_union),
            ),
            TypemapVerdict::Accept
        );

        // --- Vector{Union{Nothing,Int64}} signature ---
        //
        // The signature is now owned by the engine.  But a `Vector{Union{...}}`
        // *argument* is NOT dispatch-precise (Union in a struct parameter is
        // imprecise on the call side), so it defers as DeferImprecise.
        // A concrete `Vector{Int64}` arg against this sig → Reject.
        let vec_union = CoreType::Struct {
            name: "Vector".to_string(),
            params: vec![union_ty.clone()],
        };
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                std::slice::from_ref(&vec_union),
                &[],
                &[CoreType::from_julia_name("Vector{Int64}")],
            ),
            TypemapVerdict::Reject
        );
    }

    #[test]
    fn typemap_verdict_treats_undeclared_typevar_as_struct_leaf_issue_5314() {
        // `Q5314` names a concrete struct, but the context-free bridge images
        // it as `CoreType::TypeVar`; a free variable on the signature side
        // would accept every argument, so the filter must normalize it to the
        // nominal leaf and reject a primitive argument (the scoring matcher's
        // Issue #5314 struct-leaf rule).
        let hierarchy = StructHierarchy::new();
        let leaf = CoreType::TypeVar(CoreTypeVar::unscoped("Q5314"));
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                std::slice::from_ref(&leaf),
                &[],
                &[CoreType::from_julia_name("Float64")],
            ),
            TypemapVerdict::Reject
        );
    }

    #[test]
    fn typemap_verdict_precise_parametric_and_type_objects_issue_8548() {
        let hierarchy = StructHierarchy::new();
        let vec_int = CoreType::from_julia_name("Vector{Int64}");
        let vec_float = CoreType::from_julia_name("Vector{Float64}");
        let type_int = CoreType::TypeOf(Box::new(CoreType::from_julia_name("Int64")));

        // Fully-instantiated containers are precise: invariant parameters
        // decide the verdict.
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                std::slice::from_ref(&vec_int),
                &[],
                std::slice::from_ref(&vec_int),
            ),
            TypemapVerdict::Accept
        );
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                std::slice::from_ref(&vec_float),
                &[],
                std::slice::from_ref(&vec_int),
            ),
            TypemapVerdict::Reject
        );
        // An exact type object (`Type{Int64}`) is precise.
        assert_eq!(
            typemap_candidate_verdict(
                &hierarchy,
                std::slice::from_ref(&type_int),
                &[],
                std::slice::from_ref(&type_int),
            ),
            TypemapVerdict::Accept
        );
    }
}

// ---------------------------------------------------------------------------
// Static LatticeType query layer (Issue #8619, parent #8609)
//
// This layer lets the compile-time binary-dispatch path ask the shared
// resolver "what would runtime pick?" using the same CoreType projection the
// runtime already uses — with no JuliaType string bridge (Issue #8553 lesson).
//
// IMPORTANT: this layer is intentionally *unused* until Issue #8621 wires the
// compile-time binary dispatcher to it.  Adding it here keeps the resolver
// as the single source of the classify-then-emit policy.
// ---------------------------------------------------------------------------

/// 3-valued verdict for a binary operator call site with statically-known
/// operand `LatticeType`s (Issue #8619, parent #8609).
///
/// The verdict drives the compile-time decision:
/// - `UniqueBuiltin` → both operands are concrete primitive numerics; a typed
///   builtin / fused instruction can be emitted without consulting the method
///   table at runtime.
/// - `NeedsRuntime` → at least one operand is abstract, union, or a struct
///   type; a `CallDynamicBinary*` instruction must be emitted so the VM
///   consults the method table at runtime.
/// - `NoCandidates` → at least one operand is `Bottom` (unreachable code);
///   no instruction should be emitted for this branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryStaticVerdict {
    /// Both operand types are concrete primitive numerics (integers, floats,
    /// Bool); the binary op maps to a unique typed builtin or fused
    /// instruction with no method-table lookup.
    UniqueBuiltin,
    /// At least one operand is abstract, a union, a struct, or otherwise
    /// non-primitive: the VM must consult the method table at runtime via
    /// a `CallDynamicBinary*` instruction.
    NeedsRuntime,
    /// At least one operand lattice type is `Bottom` (unreachable); no
    /// binary instruction should be emitted for this path.
    NoCandidates,
}

/// Widen a `LatticeType` to the `CoreType` projection used for binary-dispatch
/// classification (Issue #8619).
///
/// Widen rules (no `JuliaType` string bridge — Issue #8553):
/// - `Bottom`         → `CoreType::Bottom`
/// - `Const(v)`       → `CoreType::from(&v.to_concrete_type())` (the value's concrete type)
/// - `Concrete(c)`    → `CoreType::from(c)`
/// - `Union(members)` → `CoreType::Union([CoreType::from(m) for m in members])`
/// - `Conditional`    → join of `then_type` and `else_type` (same as `CoreType::from`)
/// - `PartialStruct`  → the struct's nominal `CoreType` (field refinements discarded)
/// - `Top`            → `CoreType::Any`
///
/// For `Union` and `Conditional` the result is never `CoreType::Primitive(_)`,
/// so both map to `BinaryStaticVerdict::NeedsRuntime` inside
/// [`binary_static_verdict`].
pub fn widen_lattice_for_binary_dispatch(lattice: &LatticeType) -> CoreType {
    // `From<&LatticeType> for CoreType` (runtime_types/lattice.rs) already
    // implements exactly these widen rules, so we delegate directly.
    CoreType::from(lattice)
}

/// Whether a widened `CoreType` is a concrete primitive numeric — i.e. the
/// set of types for which every binary arithmetic / comparison operator maps
/// to a unique typed builtin or fused instruction with no method-table lookup.
///
/// The check mirrors `CorePrimitive::primitive_numeric()` (returns `Some` for
/// Bool + the integer and float machine types, `None` for `BigInt`/`BigFloat`/
/// `String`/`Char`/`Symbol`/`Nothing`/`Missing`).
///
/// `BigInt` / `BigFloat` are deliberately excluded: they dispatch to Pure
/// Julia methods via the normal method table, not through typed builtins.
fn core_is_primitive_numeric_for_binary_dispatch(core: &CoreType) -> bool {
    matches!(
        core,
        CoreType::Primitive(p) if p.primitive_numeric().is_some()
    )
}

/// Static binary-dispatch verdict for a `(left, right)` LatticeType pair
/// (Issue #8619, parent #8609).
///
/// Applies the widen rules in [`widen_lattice_for_binary_dispatch`] and
/// classifies the resulting `CoreType` pair:
///
/// | left core           | right core          | verdict        |
/// |---------------------|---------------------|----------------|
/// | `Bottom`            | _any_               | `NoCandidates` |
/// | _any_               | `Bottom`            | `NoCandidates` |
/// | primitive numeric   | primitive numeric   | `UniqueBuiltin` |
/// | _other_             | _any_               | `NeedsRuntime` |
/// | _any_               | _other_             | `NeedsRuntime` |
///
/// **This function has no side effects and does not consult any method table.**
/// It classifies purely from the lattice type pair.  The compile-time
/// dispatcher (Issue #8621) and the differential comparison mode (Issue #8620)
/// will call this to validate their own decisions.
pub fn binary_static_verdict(left: &LatticeType, right: &LatticeType) -> BinaryStaticVerdict {
    let left_core = widen_lattice_for_binary_dispatch(left);
    let right_core = widen_lattice_for_binary_dispatch(right);

    if matches!(left_core, CoreType::Bottom) || matches!(right_core, CoreType::Bottom) {
        return BinaryStaticVerdict::NoCandidates;
    }

    if core_is_primitive_numeric_for_binary_dispatch(&left_core)
        && core_is_primitive_numeric_for_binary_dispatch(&right_core)
    {
        return BinaryStaticVerdict::UniqueBuiltin;
    }

    BinaryStaticVerdict::NeedsRuntime
}

#[cfg(test)]
mod static_verdict_tests {
    use std::collections::BTreeSet;

    use crate::inference_core::type_core::CoreType;
    use crate::inference_core::{CoreAbstract, CorePrimitive};
    use crate::runtime_types::{ConcreteType, ConstValue, LatticeType};

    use super::{binary_static_verdict, widen_lattice_for_binary_dispatch, BinaryStaticVerdict};

    // --- widen rules ---

    /// `Const(Int64(42))` widens to the concrete `Int64` primitive.
    #[test]
    fn widen_const_int64_gives_primitive_int64_issue_8619() {
        let lattice = LatticeType::Const(ConstValue::Int64(42));
        let core = widen_lattice_for_binary_dispatch(&lattice);
        assert_eq!(core, CoreType::Primitive(CorePrimitive::Int64));
    }

    /// `Const(Float64(1.0))` widens to the concrete `Float64` primitive.
    #[test]
    fn widen_const_float64_gives_primitive_float64_issue_8619() {
        let lattice = LatticeType::Const(ConstValue::Float64(1.0));
        let core = widen_lattice_for_binary_dispatch(&lattice);
        assert_eq!(core, CoreType::Primitive(CorePrimitive::Float64));
    }

    /// `Concrete(Int64)` widens to `CoreType::Primitive(Int64)`.
    #[test]
    fn widen_concrete_int64_gives_primitive_int64_issue_8619() {
        let lattice = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let core = widen_lattice_for_binary_dispatch(&lattice);
        assert_eq!(core, CoreType::Primitive(CorePrimitive::Int64));
    }

    /// `Union{Int64, Float64}` widens to `CoreType::Union(...)`, which is
    /// NOT a primitive (needs runtime dispatch).
    #[test]
    fn widen_union_of_numerics_gives_core_union_issue_8619() {
        let mut members = BTreeSet::new();
        members.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        members.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        let lattice = LatticeType::Union(members);
        let core = widen_lattice_for_binary_dispatch(&lattice);
        // The result is a CoreType::Union, not a primitive.
        assert!(
            matches!(core, CoreType::Union(_)),
            "Union{{Int64,Float64}} must widen to CoreType::Union, got {core:?}"
        );
    }

    /// `Conditional { then: Int64, else: Float64 }` widens to the join of
    /// both branches — an abstract numeric or Any — which is not primitive.
    #[test]
    fn widen_conditional_joins_branches_issue_8619() {
        let lattice = LatticeType::Conditional {
            slot: "x".to_string(),
            then_type: Box::new(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64),
            ))),
            else_type: Box::new(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64),
            ))),
        };
        let core = widen_lattice_for_binary_dispatch(&lattice);
        // The join of Int64 and Float64 is at least AbstractFloat or Number;
        // it is NOT a primitive.
        assert!(
            !matches!(core, CoreType::Primitive(_)),
            "Conditional must widen to a non-primitive type, got {core:?}"
        );
    }

    /// `Top` widens to `CoreType::Any`.
    #[test]
    fn widen_top_gives_any_issue_8619() {
        let core = widen_lattice_for_binary_dispatch(&LatticeType::Top);
        assert_eq!(core, CoreType::Any);
    }

    /// `Bottom` widens to `CoreType::Bottom`.
    #[test]
    fn widen_bottom_gives_bottom_issue_8619() {
        let core = widen_lattice_for_binary_dispatch(&LatticeType::Bottom);
        assert_eq!(core, CoreType::Bottom);
    }

    // --- verdict: UniqueBuiltin ---

    /// Two concrete primitive numeric types → `UniqueBuiltin`.
    #[test]
    fn verdict_both_primitive_numerics_gives_unique_builtin_issue_8619() {
        let i64 = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let f64 = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        assert_eq!(
            binary_static_verdict(&i64, &i64),
            BinaryStaticVerdict::UniqueBuiltin
        );
        assert_eq!(
            binary_static_verdict(&f64, &f64),
            BinaryStaticVerdict::UniqueBuiltin
        );
        // Const values also give UniqueBuiltin.
        let c42 = LatticeType::Const(ConstValue::Int64(42));
        assert_eq!(
            binary_static_verdict(&c42, &i64),
            BinaryStaticVerdict::UniqueBuiltin
        );
    }

    /// `Bool` is a primitive numeric → `UniqueBuiltin`.
    #[test]
    fn verdict_bool_gives_unique_builtin_issue_8619() {
        let bool_ty =
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)));
        assert_eq!(
            binary_static_verdict(&bool_ty, &bool_ty),
            BinaryStaticVerdict::UniqueBuiltin
        );
    }

    // --- verdict: NoCandidates ---

    /// `Bottom` on the left → `NoCandidates`.
    #[test]
    fn verdict_bottom_left_gives_no_candidates_issue_8619() {
        let i64 = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert_eq!(
            binary_static_verdict(&LatticeType::Bottom, &i64),
            BinaryStaticVerdict::NoCandidates
        );
    }

    /// `Bottom` on the right → `NoCandidates`.
    #[test]
    fn verdict_bottom_right_gives_no_candidates_issue_8619() {
        let i64 = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert_eq!(
            binary_static_verdict(&i64, &LatticeType::Bottom),
            BinaryStaticVerdict::NoCandidates
        );
    }

    // --- verdict: NeedsRuntime ---

    /// `Top` (Any) on either side → `NeedsRuntime`.
    #[test]
    fn verdict_top_gives_needs_runtime_issue_8619() {
        let i64 = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert_eq!(
            binary_static_verdict(&LatticeType::Top, &i64),
            BinaryStaticVerdict::NeedsRuntime
        );
        assert_eq!(
            binary_static_verdict(&i64, &LatticeType::Top),
            BinaryStaticVerdict::NeedsRuntime
        );
    }

    /// `Union{Int64, Float64}` → `NeedsRuntime` (multiple candidates at runtime).
    #[test]
    fn verdict_union_gives_needs_runtime_issue_8619() {
        let i64 = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let mut members = BTreeSet::new();
        members.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        members.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        let union_ty = LatticeType::Union(members);
        assert_eq!(
            binary_static_verdict(&union_ty, &i64),
            BinaryStaticVerdict::NeedsRuntime
        );
    }

    /// Abstract numeric type (`Number`) → `NeedsRuntime`.
    #[test]
    fn verdict_abstract_numeric_gives_needs_runtime_issue_8619() {
        let number =
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number)));
        let i64 = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert_eq!(
            binary_static_verdict(&number, &i64),
            BinaryStaticVerdict::NeedsRuntime
        );
    }

    /// `BigInt` is NOT a primitive numeric (dispatches via Pure Julia methods)
    /// → `NeedsRuntime`.
    #[test]
    fn verdict_bigint_gives_needs_runtime_issue_8619() {
        let bigint = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigInt,
        )));
        let i64 = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert_eq!(
            binary_static_verdict(&bigint, &i64),
            BinaryStaticVerdict::NeedsRuntime
        );
        assert_eq!(
            binary_static_verdict(&bigint, &bigint),
            BinaryStaticVerdict::NeedsRuntime
        );
    }

    /// Conditional lattice type → `NeedsRuntime` (join is non-primitive).
    #[test]
    fn verdict_conditional_gives_needs_runtime_issue_8619() {
        let cond = LatticeType::Conditional {
            slot: "x".to_string(),
            then_type: Box::new(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64),
            ))),
            else_type: Box::new(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64),
            ))),
        };
        let i64 = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert_eq!(
            binary_static_verdict(&cond, &i64),
            BinaryStaticVerdict::NeedsRuntime
        );
    }
}
