// Submodules
mod broadcast;
mod builtins_arrays;
mod builtins_collections;
mod builtins_dicts;
mod builtins_equality;
mod builtins_exec;
mod builtins_io;
pub(crate) mod builtins_linalg;
mod builtins_macro;
mod builtins_math;
mod builtins_numeric;
mod builtins_reflection;
mod builtins_stats;
mod builtins_strings;
mod builtins_types;
mod builtins_types_conversion;
mod convert;
mod dispatch;
mod dispatch_binding;
mod dynamic_ops;
mod equality;
pub mod error;
pub(crate) mod exec;
mod executable;
mod field_indices;
mod formatting;
// Display-only Complex{FloatNN} → ComplexFNN alias helper, needed by the FFI and
// the `sjulia` binary's own value formatter (a separate crate) — Issue #5704.
pub use formatting::apply_complex_float_aliases;
// Julia 1.12-faithful BigFloat display, needed by the FFI value formatter
// (which has its own `format_value`) — Issue #6789.
pub(crate) use formatting::format_bigfloat_julia;
mod frame;
mod hof_exec;
pub mod instr;
pub(crate) mod intrinsics_exec;
mod matmul;
mod narrow_int_arith;
pub(crate) mod native_array_compat;
mod numeric_identity;
pub mod profiler;
pub(crate) mod slot;
pub mod specialize;
pub(crate) mod splat;
pub mod stack_ops;
mod state;
mod struct_setup;
#[cfg(test)]
mod tests;
mod type_objects;
mod type_ops;
pub(crate) mod type_utils;
pub mod types;
pub(crate) mod util;
pub mod value;

// Re-exports from types module
pub use types::{
    AbstractTypeDefInfo, CompiledProgram, FunctionInfo, I64SpecDispatch, KwParamInfo,
    PrimitiveTypeDefInfo, RuntimeCompileContext, ShowMethodEntry, SpecializableFunction,
    SpecializationKey, SpecializedCode, StructDefInfo,
};

// Re-exports
pub use error::{SpannedVmError, VmError};
pub use frame::VarTypeTag;
pub use instr::Instr;
pub use instr::{
    CallDirectSlots, CallSpecializeSlots, CallVarKwargsSplat, DynamicCallCandidate,
    InvokeWithKwargs, MakeGeneratorOperands, ModuleOperands, NativeIteratorKind,
    ResolvedFunctionOperands, StaticParamBinding, StaticParametricCall, TypedDispatchStoreDict,
};
pub use stack_ops::{StackOps, StackOpsExt};
pub use value::{
    new_array_ref,
    new_typed_array_ref,
    ArrayData,
    ArrayElementType,
    ArrayRef,
    ArrayValue,
    ClosureValue,
    ComposedFunctionValue,
    DictKey,
    ExprValue,
    FunctionValue,
    GeneratorValue,
    GlobalRefValue,
    IOKind,
    IOValue,
    LineNumberNodeValue,
    MemoryRef,
    ModuleValue,
    NamedTupleValue,
    PairsValue,
    RangeValue,
    RuntimeTypeVarValue,
    StructInstance,
    // Macro system types
    SymbolValue,
    TupleValue,
    TypedArrayRef,
    TypedArrayValue,
    Value,
    ValueType,
};

// Internal imports
use crate::inference_core::dispatch_resolver::runtime_julia_type_contains_type_var;
use dispatch_binding::{
    bind_array_rank_type_param, bind_val_parameter_value, parse_val_char_parameter,
    parse_val_constructor_parameter, parse_val_tuple_parameter, split_top_level_comma,
};
// Shared candidate-signature derivation for the structured Instr payload
// migration (Issue #6496).
pub(crate) use dispatch_binding::expanded_param_types_for_call;
// Test-only parity gate hook that compares the derived rendered signature with
// the historical compile-time baking.
#[cfg(test)]
pub(crate) use dispatch_binding::derived_runtime_signature;
use frame::{Frame, Handler};
use hof_exec::state::{BroadcastState, ComposedCallState, GeneratorIterateState, SprintState};
use native_array_compat::{
    base_function_accepts_native_array_value, is_native_array_value, native_array_value_ptr_eq,
    params_cross_native_array_wrapper_boundary,
};
use numeric_identity::{numeric_integer_values_equal, numeric_integer_values_identical};
use struct_setup::{
    build_struct_hierarchy_from_program, compute_type_ancestors, normalize_method_struct_def,
};
use util::bind_value_to_slot;
use value::{
    array_element_type_to_julia_type, is_complex_type_name, julia_array_type_for_ndims,
    native_array_ref_value, native_array_value_ref, value_type_for_struct_instance,
};

use crate::inference_core::{selection, specificity, CoreType};
use crate::intrinsics::Intrinsic;
#[cfg(test)]
use crate::rng::StableRng;
use crate::rng::{RngInstance, RngLike};
use crate::types::StructHierarchy;
use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::CString;
use std::hash::{Hash, Hasher};
use std::os::raw::{c_char, c_void};
use std::rc::Rc;

/// Hash a type name string to a u64 key for the dispatch cache (Issue #3355).
/// Avoids storing String keys in the hot dispatch path.
#[inline]
pub(crate) fn hash_type_name(name: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    hasher.finish()
}

#[inline]
fn exact_call_site_type_tag(value: &Value) -> Option<u64> {
    let tag = match value {
        Value::I8(_) => 1,
        Value::I16(_) => 2,
        Value::I32(_) => 3,
        Value::I64(_) => 4,
        Value::I128(_) => 5,
        Value::BigInt(_) => 6,
        Value::U8(_) => 7,
        Value::U16(_) => 8,
        Value::U32(_) => 9,
        Value::U64(_) => 10,
        Value::U128(_) => 11,
        Value::F16(_) => 12,
        Value::F32(_) => 13,
        Value::F64(_) => 14,
        Value::BigFloat(_) => 15,
        Value::Bool(_) => 16,
        Value::Str(_) => 17,
        Value::Char(_) => 18,
        Value::Nothing => 19,
        Value::Missing => 20,
        Value::Symbol(_) => 21,
        Value::Regex(_) => 22,
        Value::RegexMatch(_) => 23,
        _ => return None,
    };
    Some(tag)
}

#[inline]
fn exact_call_site_fingerprint(values: &[&Value]) -> Option<u64> {
    if values.is_empty() || values.len() > 7 {
        return None;
    }

    let mut fingerprint = (values.len() as u64) << 56;
    for (idx, value) in values.iter().enumerate() {
        let tag = exact_call_site_type_tag(value)?;
        fingerprint |= tag << (idx * 8);
    }
    (fingerprint != CALL_SITE_INLINE_CACHE_EMPTY).then_some(fingerprint)
}

struct RuntimeCandidateMatch {
    idx: usize,
    param_types: Vec<crate::types::JuliaType>,
    score: u32,
    is_vararg: bool,
}

const CALL_SITE_INLINE_CACHE_EMPTY: u64 = 0;

#[derive(Debug, Clone, Copy)]
struct CallSiteCache {
    arg_fingerprint: u64,
    func_index: usize,
}

impl Default for CallSiteCache {
    fn default() -> Self {
        Self {
            arg_fingerprint: CALL_SITE_INLINE_CACHE_EMPTY,
            func_index: usize::MAX,
        }
    }
}

impl CallSiteCache {
    #[inline]
    fn lookup(self, arg_fingerprint: u64) -> Option<usize> {
        (arg_fingerprint != CALL_SITE_INLINE_CACHE_EMPTY && self.arg_fingerprint == arg_fingerprint)
            .then_some(self.func_index)
    }

    #[inline]
    fn store(&mut self, arg_fingerprint: u64, func_index: usize) {
        if arg_fingerprint != CALL_SITE_INLINE_CACHE_EMPTY {
            self.arg_fingerprint = arg_fingerprint;
            self.func_index = func_index;
        }
    }
}

/// Output callback function type for streaming output.
/// Takes a context pointer and the output string (null-terminated C string).
pub type OutputCallback = extern "C" fn(context: *mut c_void, output: *const c_char);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BinaryDispatchOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    IntDiv,
    Pow,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct BinaryDispatchKey {
    pub op: BinaryDispatchOp,
    pub left: ValueType,
    pub right: ValueType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MethodDispatchKey {
    names: Vec<u64>,
    arg_types: Vec<u64>,
}

pub struct Vm<R: RngLike> {
    ip: usize,
    stack: Vec<Value>,
    frames: Vec<Frame>,
    /// Pool of retired call frames kept for reuse (Issue #5172).
    ///
    /// On `pop_call_frame` a returning frame is pushed here (after its slots are
    /// dropped) instead of being deallocated, so its slot vector and backing
    /// `HashMap`s keep their allocated storage. A subsequent call reuses one via
    /// `acquire_frame`,
    /// avoiding the per-call map allocations that dominate tight recursion /
    /// small-function workloads. Capped at `MAX_POOLED_FRAMES` so deep stacks
    /// that later unwind do not retain unbounded memory.
    frame_pool: Vec<Frame>,
    return_ips: Vec<usize>,
    handlers: Vec<Handler>,
    /// Shared, mostly-immutable instruction slice (Issue #5177).
    ///
    /// Wrapped in `Rc` so the dispatch loop can hold a cheap snapshot clone and
    /// keep an immutable `&Instr` reference into it across `dispatch_instr`
    /// (which borrows `&mut self`), instead of swapping each instruction out to
    /// `Instr::Nop` and back on every cycle. Instructions are never mutated in
    /// place at run time; the only run-time write is the rare `CallSpecialize`
    /// append (`exec/call.rs`), which uses `Rc::make_mut` to copy-on-write into
    /// a fresh vector — the loop then follows `self.code` for the next fetch.
    code: Rc<Vec<Instr>>,
    executable: executable::ExecutableProgram,
    next_executable_ip: usize,
    functions: Vec<Rc<FunctionInfo>>,
    /// Number of Base/prelude functions at the front of `functions`.
    ///
    /// Runtime dispatch has its own #5926 dominance selection mirror, separate
    /// from `MethodTable::dispatch_inner`, so it needs the same origin context
    /// as compile-time dispatch before origin fences can be applied there.
    base_function_count: usize,
    /// Per-function flag: this Base function's wrapper-typed methods may
    /// receive the transitional native array carrier across the
    /// native-array wrapper dispatch fence (#3908/#4189). Derived once from
    /// the function names at program install (Issue #6336: dispatch reads a
    /// precomputed flag instead of matching name strings per call). Indexed by
    /// func_index; out-of-range = not exempt.
    native_array_exempt_functions: Vec<bool>,
    /// Per-function `name -> slot index` lookup, indexed by func_index and
    /// mirroring `slot_names`. Replaces the O(slots) linear scan of
    /// `func.slot_names.iter().position(..)` in `slot_index_for_frame` with an
    /// O(1) hash probe for the string-keyed `Load*/Store*` paths (Issue #5179).
    function_slot_maps: Vec<HashMap<String, usize>>,
    /// Memoized `(left, right)` expected signatures for binary dispatch
    /// candidates, keyed by function index (Issue #6496), carrying both the
    /// historical rendered names (VM representation fences, debug logs) and
    /// the structured `core_signature` projection consumed by the structured
    /// resolver (Issue #6502 slice 2).
    ///
    /// `CallDynamicBinaryBoth` has no call-site dispatch cache, so its shared
    /// resolver runs on every non-primitive dispatch; deriving the candidate
    /// signature from `FunctionInfo` each time would re-derive the same
    /// values per dispatch. `None` records a candidate whose signature
    /// cannot be derived for arity 2 (excluded from scoring).
    binary_signature_cache: HashMap<usize, Option<dispatch_binding::RuntimeCandidateCoreSignature>>,
    /// Memoized per-arity expected signatures for
    /// `CallTypedDispatch[OrBuiltin*]` candidates, keyed by
    /// `(func_index, arity)` (Issue #6496).
    ///
    /// The typed-dispatch family has no call-site dispatch cache, so its
    /// shared resolver runs on every dispatch; deriving the candidate
    /// signatures from `FunctionInfo` each time would re-render and re-project
    /// the same types per dispatch. Signatures are shared behind `Rc` so each
    /// dispatch clones pointers, not strings/CoreTypes. `None` records a
    /// candidate that cannot accept the arity (excluded from scoring, matching
    /// the historical emit-time `runtime_type_names_for_arity` gate which never
    /// baked such a candidate).
    typed_signature_cache: HashMap<
        (usize, usize),
        Option<std::rc::Rc<dispatch_binding::RuntimeCandidateCoreSignature>>,
    >,
    struct_defs: Vec<StructDefInfo>,
    abstract_types: Vec<AbstractTypeDefInfo>, // User-defined abstract types
    show_methods: std::collections::HashMap<String, usize>, // type_name -> func_index
    struct_heap: Vec<StructInstance>,         // Heap for mutable struct instances
    rng: R,
    output: String, // Buffer for println output
    /// Buffer for stderr output (Issue #3573).
    /// Forwarded to actual stderr by the runner / FFI consumer on exit.
    stderr_output: String,
    output_callback: Option<OutputCallback>,
    output_callback_context: *mut c_void,
    /// Stack of in-flight value-mode HOF (`map`/`filter`/...) broadcasts. A
    /// stack (not a single slot) is required because a HOF's mapping function
    /// may itself perform another HOF call (e.g. `map(x -> map(...), v)`). The
    /// inner broadcast must not clobber the outer's pending state (Issue #5229).
    /// Each `BroadcastState` carries the `hof_frame_depth` of the function frame
    /// that owns it, so returns route to the correct (top-of-stack) broadcast.
    broadcast_states: Vec<BroadcastState>,
    composed_call_state: Option<ComposedCallState>,
    /// Stack of pending lazy `iterate(::Generator)` continuations. A stack
    /// (not a single slot) is required because a generator's mapping function
    /// may itself perform a generator iteration (e.g. `map(x -> map(...), v)`).
    /// The inner iteration must not clobber the outer's pending continuation
    /// (Issue #5229).
    generator_iterate_state: Vec<GeneratorIterateState>,
    sprint_state: Option<SprintState>,
    pending_error: Option<VmError>,
    /// The pending exception value for catch blocks (preserves struct instances)
    pending_exception_value: Option<Value>,
    /// Stack of exceptions currently active in catch blocks for `rethrow()`.
    caught_exceptions: Vec<(VmError, Option<Value>)>,
    rethrow_on_finally: bool,
    // Test state for @test and @testset macros
    test_pass_count: usize,
    test_fail_count: usize,
    test_broken_count: usize,
    current_testset: Option<String>,
    // Sticky flag: set whenever ANY `@test`/`@testset` records a failure (or a
    // `@test_broken` unexpectedly passes). The per-testset counts above reset at
    // each `@testset`, so this accumulates failures across the whole run. The CLI
    // reads it via `any_test_failed()` to exit non-zero, matching upstream Julia
    // where a failing top-level `@testset` throws a `TestSetException` → exit 1
    // (Issue #8191).
    any_test_failed: bool,
    // Test throws state: (expected_exception_type, was_thrown)
    test_throws_state: Option<(String, bool)>,
    // === Lazy AoT Compilation Support ===
    specializable_functions: Vec<SpecializableFunction>,
    specialization_cache: HashMap<SpecializationKey, SpecializedCode>,
    // Cheap monomorphic fast cache for the all-`I64` specialize-call hot path,
    // keyed by `(spec_func_index, arity)` so the dispatch avoids per-call `Vec`
    // allocation and `Vec`-keyed hashing (Issue #8167). Populated lazily from
    // `specialization_cache` on the first all-`I64` call to an eligible callee.
    specialization_i64_cache: HashMap<(usize, usize), I64SpecDispatch>,
    i64_function_cache: HashMap<usize, Option<executable::I64FunctionBlock>>,
    binary_method_cache: HashMap<BinaryDispatchKey, usize>,
    compile_context: Option<RuntimeCompileContext>,
    /// Macro bindings visible per module (`module path -> {"@name", ...}`),
    /// so function-form `isdefined(::Module, Symbol("@name"))` can consult the
    /// macro binding table that macros are otherwise erased from at runtime
    /// (Issue #7948).
    macro_bindings: HashMap<String, std::collections::HashSet<String>>,
    global_slot_names: Vec<String>,
    global_slot_map: HashMap<String, usize>,
    // Macro system support
    gensym_counter: u64, // Counter for generating unique symbol names
    runtime_typevar_counter: u64,
    // Issue #4698: preserve fresh-TypeVar identity across parametric type
    // construction. When `Vector{T}` is built from a `Value::RuntimeTypeVar`,
    // its id-bearing `RuntimeTypeVarValue` is stashed here keyed by
    // (name, upper-bound name) so reflection (`Vector{T}.parameters[1]`) can
    // return the *same* TypeVar object, keeping `parameters[1] === T` true.
    runtime_typevar_identities: HashMap<(String, Option<String>), RuntimeTypeVarValue>,
    // Cached well-known struct type IDs (Issue #2940)
    cached_cartesian_index_type_id: Cell<Option<usize>>,
    cached_pair_type_id: Cell<Option<usize>>,
    cached_complex_type_id: Cell<Option<usize>>,
    cached_array_type_id: Cell<Option<usize>>,
    // Struct name -> index lookup (Issue #2938)
    struct_def_name_index: HashMap<String, usize>,
    // Abstract type name -> index lookup (Issue #2896)
    abstract_type_name_index: HashMap<String, usize>,
    // Method dispatch cache: (call_site_ip, hashed_type_name) → func_index (Issue #2943, #3355)
    dispatch_cache: HashMap<usize, HashMap<u64, usize>>,
    // Issue #8168: per-call-site cache for the `CallDynamicBinaryBoth` resolver
    // decision, keyed `call_site_ip → (left_type_hash, right_type_hash) →
    // Option<func_index>`. Only populated for struct/struct operand pairs, where
    // the matched method is fully determined by the operand type names — the
    // value-dependent Dict/Memory guards inside the resolver never fire for two
    // `Struct`/`StructRef` operands — so a name-keyed cache returns exactly what
    // the resolver would. Mirrors `dispatch_cache`'s never-invalidated lifetime.
    binary_both_dispatch_cache: HashMap<usize, HashMap<(u64, u64), Option<usize>>>,
    // Monomorphic call-site cache indexed directly by bytecode IP (Issue #6345).
    call_site_caches: Vec<CallSiteCache>,
    // Global method dispatch cache: (function names, argument type names) -> func_index.
    // `None` is a negative-cache entry (Issue #5087).
    method_dispatch_cache: HashMap<MethodDispatchKey, Option<usize>>,
    // `@generated` compatibility cache: (function index, concrete argument
    // Julia type names) -> returned staged Expr (Issue #5936).
    generated_expr_cache: HashMap<(usize, Vec<String>), Value>,
    // In-flight generated frame depth -> cache key. The key is known at call
    // entry, while the staged Expr is only available when the body reaches the
    // compiler-internal `GeneratedEval` builtin (Issue #5936).
    generated_expr_pending_keys: HashMap<usize, (usize, Vec<String>)>,
    // In-flight generated frame depth -> runtime-argument frame used to eval
    // the staged Expr returned by the generated body on the first miss.
    generated_expr_pending_eval_frames: HashMap<usize, Frame>,
    // Function name → indices lookup for O(1) name-based dispatch (Issue #3361)
    function_name_index: HashMap<String, Vec<usize>>,
    current_world: u64,
    // Source map: instruction IP → source span (Issue #2856)
    source_map: Vec<Option<crate::span::Span>>,
    // IP of the last instruction that caused an error (Issue #2856)
    last_error_ip: Option<usize>,
    // Pre-computed transitive closure of abstract type hierarchy (Issue #3356).
    // Maps type name -> list of all ancestor type names (including parametric base names).
    type_ancestors: HashMap<String, Vec<String>>,
    // Declared struct/abstract parent graph shared with compile-time type logic
    // (Issue #5920). Runtime keeps this alongside the legacy ancestor closure
    // while call sites migrate away from thread-local inference registries.
    struct_hierarchy: StructHierarchy,
    // Current nesting depth of `eval`-initiated VM dispatch calls (Issue #5014).
    // `eval_dispatch_call` recurses on the native (Rust) call stack for every
    // nested VM call started from the `eval` builtin, so an `eval`-driven
    // self-recursion could otherwise exhaust the host stack and crash the
    // process. The depth is bounded by `Self::MAX_EVAL_DISPATCH_DEPTH`.
    eval_dispatch_depth: usize,
    // Frame-depth floor for the innermost active `eval`-driven nested dispatch
    // (Issue #5972). `None` outside any `run_until_frame_return`; `Some(d)` while
    // that loop is awaiting a frame pushed at depth `d`. `handle_error` must NOT
    // route an error to a handler installed by an *ancestor* frame (one whose
    // `frame_len <= d`, i.e. a `try` opened outside this `eval` call): catching
    // it inside the nested loop truncates `self.frames` below the floor and the
    // loop's return check fires mid-catch, abandoning the catch body and
    // swallowing the exception. Instead the error propagates as `Err` out of
    // `run_until_frame_return`/`eval_dispatch_call`, and the outer `run()` loop's
    // `CallBuiltin` handler re-routes it to that ancestor handler via `self.raise`
    // at the correct level. Saved/restored around each nested dispatch so nested
    // `eval`s see their own (deeper) floor and ancestors see theirs.
    eval_dispatch_floor: Option<usize>,
    // Set when a call boundary pushes beyond `MAX_CALL_DEPTH`. The dispatch
    // loop raises it after the call handler finishes installing the callee
    // instruction pointer, so catch handlers are not overwritten by call setup.
    call_depth_overflow_pending: bool,
}

impl<R: RngLike> Vm<R> {
    /// Whether any `@test` / `@testset` recorded a failure during this run (or a
    /// `@test_broken` unexpectedly passed). The CLI uses this to exit non-zero,
    /// matching upstream Julia where a failing top-level `@testset` throws a
    /// `TestSetException` and the process exits 1 (Issue #8191).
    #[inline]
    pub fn any_test_failed(&self) -> bool {
        self.any_test_failed
    }

    /// Top of the in-flight value-mode HOF broadcast stack, if any (Issue #5229).
    #[inline]
    pub(crate) fn broadcast_state(&self) -> Option<&BroadcastState> {
        self.broadcast_states.last()
    }

    /// Whether the runtime specializer must skip its native-indexing fast path
    /// for scalar `xs[i]` because the program defines a user `getindex` override
    /// on a native array receiver (Issue #6657). Defaults to `false` when no
    /// compile context is present.
    #[inline]
    pub(crate) fn disable_array_getindex_specialization(&self) -> bool {
        self.compile_context
            .as_ref()
            .is_some_and(|ctx| ctx.disable_array_getindex_specialization)
    }

    /// Whether the `IndexStore` native write fast path must be skipped for a
    /// MemoryRef-backed `Array{T,N}` wrapper because the program defines a user
    /// `setindex!` override on a native array receiver (Issue #6806). Defaults to
    /// `false` when no compile context is present.
    #[inline]
    pub(crate) fn disable_array_setindex_specialization(&self) -> bool {
        self.compile_context
            .as_ref()
            .is_some_and(|ctx| ctx.disable_array_setindex_specialization)
    }

    /// Whether the function specializer must skip its direct-`GetField` fast path
    /// for `obj.field` reads because the program defines a user `getproperty`
    /// override (Issue #8127). Defaults to `false` when no compile context is
    /// present.
    #[inline]
    pub(crate) fn disable_field_access_specialization(&self) -> bool {
        self.compile_context
            .as_ref()
            .is_some_and(|ctx| ctx.disable_field_access_specialization)
    }

    /// Push a new broadcast onto the stack (start a value-mode HOF). Nested HOFs
    /// (`map(x -> map(...), v)`) push without destroying the outer state.
    #[inline]
    pub(crate) fn push_broadcast_state(&mut self, state: BroadcastState) {
        self.broadcast_states.push(state);
    }

    /// Pop the completed (top) broadcast, restoring any enclosing broadcast.
    #[inline]
    pub(crate) fn clear_broadcast_state(&mut self) {
        self.broadcast_states.pop();
    }

    /// Maximum nesting depth of `eval`-initiated VM dispatch calls (Issue #5014).
    ///
    /// Each nested `eval(...)` call that dispatches into a VM frame recurses on
    /// the native Rust call stack (`eval_dispatch_call` -> `run_until_frame_return`
    /// -> ... -> `eval` builtin -> `eval_dispatch_call`). Without a bound, an
    /// `eval`-driven self-recursion would exhaust the host stack and crash the
    /// process. This limit is generous enough for any realistic metaprogramming
    /// use (ordinary nested `eval` is rarely more than a handful deep) while
    /// keeping the worst-case native stack usage safely bounded; exceeding it
    /// surfaces as a `VmError::StackOverflow` (Julia's `StackOverflowError`),
    /// matching upstream's behaviour for runaway recursion.
    pub(crate) const MAX_EVAL_DISPATCH_DEPTH: usize = 96;

    /// Maximum depth of the VM call-frame stack (`self.frames`) — Issue #5969.
    ///
    /// Unlike `eval`-driven dispatch (bounded by `MAX_EVAL_DISPATCH_DEPTH`),
    /// ordinary Julia calls execute *iteratively* in the `run()` loop: each call
    /// pushes a `Frame` onto `self.frames` and jumps the instruction pointer, so
    /// a runaway recursion does not exhaust the native Rust stack — it grows
    /// `self.frames` (a heap `Vec`) without bound until the **host runs out of
    /// memory**. That is exactly the failure mode of Issue #5966: a mixed
    /// Complex/Real `==` that fell into a self-recursive promote fallback grew
    /// each worker to ~30 GB RSS (host > 80 GB) before being SIGTERM'd, with no
    /// clear diagnostic.
    ///
    /// This bound converts that OOM into an immediate, catchable
    /// `StackOverflowError` (Julia's behaviour for infinite recursion). The
    /// limit is checked at a clean instruction boundary at the top of the
    /// dispatch loop, so the offending frames are never executed and the error
    /// is raised through the normal `try`/`catch` machinery.
    ///
    /// The value is chosen for the memory-constrained no-JIT iOS runtime, which
    /// is the primary target: a runaway recursion must error *before* the OS
    /// memory-killer reaps the app. It is ~100x the deepest recursion anywhere
    /// in the codebase — the deepest legitimate recursion in the fixtures,
    /// benchmarks and iOS samples is on the order of a hundred frames (e.g.
    /// `is_even(100)`); every "deep" loop (`countdown_loop(10000)`,
    /// `estimate_pi(100000)`) is *iterative*, not recursive, so it has depth 1.
    /// At this bound a measured worst-case runaway adds only ~80 MB of transient
    /// frame/stack growth (freed the instant the error unwinds), versus the
    /// ~30 GB/worker of the unguarded path. It is not a parity-exact match for
    /// Julia's stack-size-derived limit (impossible for a heap-allocated frame
    /// stack); the *behaviour* — a catchable `StackOverflowError` instead of an
    /// OOM — is what matches upstream.
    pub(crate) const MAX_CALL_DEPTH: usize = 10_000;

    /// Enter one level of `eval`-initiated VM dispatch, returning the new depth
    /// or `VmError::StackOverflow` if the bound would be exceeded (Issue #5014).
    pub(crate) fn enter_eval_dispatch(&mut self) -> Result<(), VmError> {
        if self.eval_dispatch_depth >= Self::MAX_EVAL_DISPATCH_DEPTH {
            return Err(VmError::StackOverflow);
        }
        self.eval_dispatch_depth += 1;
        Ok(())
    }

    /// Leave one level of `eval`-initiated VM dispatch (Issue #5014).
    pub(crate) fn exit_eval_dispatch(&mut self) {
        self.eval_dispatch_depth = self.eval_dispatch_depth.saturating_sub(1);
    }

    /// Start a function call by index with positional arguments.
    fn start_function_call(&mut self, func_index: usize, args: Vec<Value>) -> Result<(), VmError> {
        let func = self
            .functions
            .get(func_index)
            .ok_or_else(|| VmError::TypeError(format!("Unknown function index: {}", func_index)))?
            .clone();

        let mut frame = self.acquire_frame(func.local_slot_count, Some(func_index));

        // Bind type parameters from where clauses (Issue #2468)
        self.bind_type_params(&func, &args, &mut frame);

        if let Some(vararg_idx) = func.vararg_param_index {
            for idx in 0..vararg_idx {
                if let Some(val) = args.get(idx) {
                    if let Some(slot) = func.param_slots.get(idx) {
                        bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                    }
                }
            }
            let vararg_values: Vec<Value> = args[vararg_idx..].to_vec();
            let vararg_tuple = Value::Tuple(TupleValue {
                elements: vararg_values,
            });
            if let Some(slot) = func.param_slots.get(vararg_idx) {
                bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
            }
        } else {
            for (idx, slot) in func.param_slots.iter().enumerate() {
                if let Some(val) = args.get(idx) {
                    bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                }
            }
        }

        for kwparam in &func.kwparams {
            if kwparam.required {
                return Err(VmError::UndefKeywordError(kwparam.name.clone()));
            }
            bind_value_to_slot(
                &mut frame,
                kwparam.slot,
                kwparam.default.clone(),
                &mut self.struct_heap,
            );
        }

        if let Some(result) =
            self.try_eval_cached_generated_expr(func_index, &func, &args, &frame)?
        {
            self.stack.push(result);
            return Ok(());
        }

        let generated_eval_frame = func.is_generated.then(|| frame.clone());
        self.bind_generated_body_arg_types(&func, &args, &mut frame);
        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.remember_current_generated_expr_cache_key(
            &func,
            func_index,
            &args,
            generated_eval_frame,
        );
        self.ip = func.entry;
        Ok(())
    }

    /// Upper bound on the number of retired frames kept in `frame_pool`
    /// (Issue #5172). Deep call stacks that later unwind would otherwise return
    /// every frame to the pool; capping retention keeps idle memory bounded
    /// while still covering the common recursion / tight-call-loop depths.
    pub(crate) const MAX_POOLED_FRAMES: usize = 256;

    /// Obtain a fresh call frame, reusing a retired one from `frame_pool` when
    /// available (Issue #5172). A recycled frame is reset in place so its slot
    /// vector and backing maps keep their allocated capacity, eliminating the
    /// per-call allocations of `Frame::new_with_slots`.
    pub(crate) fn acquire_frame(&mut self, slot_count: usize, func_index: Option<usize>) -> Frame {
        match self.frame_pool.pop() {
            Some(mut frame) => {
                frame.prepare_for_reuse(slot_count, func_index);
                frame
            }
            None => Frame::new_with_slots(slot_count, func_index),
        }
    }

    /// Like [`acquire_frame`], but additionally seeds the frame's
    /// `captured_vars` from a closure's captured environment (Issue #5172).
    /// Issue #5189: takes the captures by shared slice (the closure stores them
    /// behind an `Rc`), so the per-call hot path borrows the closure's frozen
    /// capture set instead of deep-cloning the whole `Vec<(String, Value)>`.
    /// Only the individual captured `Value`s are cloned into the (possibly
    /// pooled, Issue #5172) frame's `captured_vars` map.
    pub(crate) fn acquire_frame_with_captures(
        &mut self,
        slot_count: usize,
        func_index: Option<usize>,
        captures: &[(String, Value)],
    ) -> Frame {
        let mut frame = self.acquire_frame(slot_count, func_index);
        frame.captured_vars.reserve(captures.len());
        for (name, value) in captures {
            frame.captured_vars.insert(name.clone(), value.clone());
        }
        frame
    }

    pub(crate) fn push_call_frame(&mut self, mut frame: Frame) {
        frame.stack_base = self.stack.len();
        frame.world_age = self.current_world;
        self.frames.push(frame);
    }

    #[inline(always)]
    pub(crate) fn check_cancel_boundary(&mut self) -> Result<(), VmError> {
        if crate::cancel::is_requested() {
            return Err(VmError::Cancelled);
        }
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn try_push_call_frame(&mut self, frame: Frame) -> Result<(), VmError> {
        self.check_cancel_boundary()?;
        self.push_call_frame(frame);
        if self.frames.len() > Self::MAX_CALL_DEPTH {
            self.call_depth_overflow_pending = true;
        }
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn try_push_temporary_call_frame(&mut self, frame: Frame) -> Result<(), VmError> {
        self.check_cancel_boundary()?;
        self.push_call_frame(frame);
        if self.frames.len() > Self::MAX_CALL_DEPTH {
            self.pop_call_frame();
            return Err(VmError::StackOverflow);
        }
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn handle_pending_call_depth_overflow(&mut self) -> Result<(), VmError> {
        if self.call_depth_overflow_pending {
            self.call_depth_overflow_pending = false;
            self.raise(VmError::StackOverflow)?;
        }
        Ok(())
    }

    pub(crate) fn pop_call_frame(&mut self) {
        let depth = self.frames.len().saturating_sub(1);
        self.generated_expr_pending_keys.remove(&depth);
        self.generated_expr_pending_eval_frames.remove(&depth);
        if let Some(mut frame) = self.frames.pop() {
            self.stack.truncate(frame.stack_base);
            // Retire the frame into the pool for reuse instead of dropping it,
            // so its backing maps' allocations are recycled (Issue #5172). The
            // frame is emptied here (releasing its values, retaining capacity)
            // before being pooled; `acquire_frame` later re-sizes its slots.
            if self.frame_pool.len() < Self::MAX_POOLED_FRAMES {
                frame.clear_for_pool();
                self.frame_pool.push(frame);
            }
        }
    }

    /// Look up a user-defined `show(io::IO, ::T)` method for the given value.
    /// Returns the function index when one exists for the value's exact struct
    /// name, or for the bare base name of a parametric struct (e.g.
    /// `Complex{Float64}` → `Complex`). Returns `None` for non-struct values or
    /// when no specific `show` method has been registered.
    ///
    /// Used to route `print(io, ::Struct)` and `string(::Struct)` through the
    /// user's `show` method instead of the Rust struct-field dump
    /// (Issue #4761). Mirrors the dispatch in `Instr::PrintAnyNoNewline`.
    pub(crate) fn user_show_method_for(&self, v: &Value) -> Option<usize> {
        if let Value::Struct(s) = v {
            // A struct defined inside a module carries a module-qualified name
            // (e.g. "Primes.Factorization"), but a `Base.show(io, ::Factorization)`
            // method registers under the name as written in the signature — usually
            // the bare "Factorization". Try, in order: the full qualified name, the
            // qualified name without parametric braces, then the same two with the
            // module prefix stripped, so module-defined show methods are found
            // regardless of how the value's type name is qualified (Issue #7171/#7172).
            let full = &*s.struct_name;
            let base_full = &full[..full.find('{').unwrap_or(full.len())];
            let no_mod = match full.rfind('.') {
                Some(pos) => &full[pos + 1..],
                None => full,
            };
            let base_no_mod = &no_mod[..no_mod.find('{').unwrap_or(no_mod.len())];
            [full, base_full, no_mod, base_no_mod]
                .iter()
                .find_map(|key| self.show_methods.get(*key).copied())
        } else {
            None
        }
    }

    /// Render `value` to a string via its user-defined `show(io, ::T)` method, if
    /// one is registered, by running that method on a throwaway `IOBuffer` — the
    /// same path `string(x)` uses. Returns `None` when the value has no user
    /// `show` (callers fall back to the default formatter).
    ///
    /// Intended to be called after `run()` has returned: it drives the show
    /// method to completion with [`Self::run_until_frame_return`] (the re-entrant
    /// `eval` driver) and reads the buffer. Used by the REPL/FFI result echo so a
    /// user type displays the same as `string(x)` / `println(x)` instead of the
    /// Rust struct-field dump (Issue #7168).
    pub(crate) fn render_value_via_user_show(&mut self, value: &Value) -> Option<String> {
        // Resolve a heap `StructRef` to its `Value::Struct` so the show-method
        // lookup can key on the struct name.
        let resolved =
            crate::vm::formatting::resolve_struct_refs_for_format(value, &self.struct_heap);
        // Types with a dedicated Rust display formatter (Complex, Rational,
        // LinRange, array wrappers) keep that formatter: for them it is the
        // canonical, upstream-matching form, and the bundled Julia `show` may
        // differ — e.g. `LinRange`'s `show` prints the struct form rather than
        // the `a:step:b` range. Returning `None` here leaves those on the Rust
        // path; everything else (user types like Symbolics) uses `show`.
        if let Value::Struct(s) = &resolved {
            let short = s.struct_name.rsplit('.').next().unwrap_or(&s.struct_name);
            if s.is_complex()
                || s.is_rational()
                || s.array_wrapper_julia_type().is_some()
                || short.starts_with("LinRange")
            {
                return None;
            }
        }
        let func_index = self.user_show_method_for(&resolved)?;
        let io = crate::vm::value::IOValue::buffer_ref();
        let target_depth = self.frames.len();
        // `start_sprint_call` pushes the `show(io, value)` frame and arranges for
        // the eventual return to extract the buffer as a `Value::Str`; the driver
        // then unwinds back to `target_depth` and hands us that string.
        self.start_sprint_call(func_index, io, vec![resolved])
            .ok()?;
        match self.run_until_frame_return(target_depth) {
            Ok(Value::Str(s)) => Some(s),
            _ => None,
        }
    }

    /// When `value` is an array whose struct elements carry a registered
    /// `Base.show(io, ::T)`, render the whole array string by running that show
    /// method for each such element and splicing the result into the array form
    /// (Issue #7893). Returns `None` when `value` is not an array, when no
    /// element has a user `show`, or when none of the rendered elements differ
    /// from the default formatter — in which case callers keep the pure-Rust
    /// formatter (so numeric arrays and structs without a registered `show` are
    /// untouched).
    ///
    /// Upstream array `print`/`string`/`repr` all call `show` on each element
    /// (see `julia/base/arrayshow.jl`), so a single per-element `show` pass is
    /// correct for every textual array path. The pure formatter cannot do this
    /// itself because it has no way to re-enter the interpreter; this method
    /// runs each element's show via the same re-entrant driver as
    /// [`Self::render_value_via_user_show`].
    pub(crate) fn render_array_via_user_show(&mut self, value: &Value) -> Option<String> {
        let resolved =
            crate::vm::formatting::resolve_struct_refs_for_format(value, &self.struct_heap);

        // The element values to consider, in column-major linear order. Two
        // array carriers reach display: the ExprArgs native vector
        // (`expr.args` / `Vector{Any}`) and the pure-Julia `Array{T,N}` wrapper
        // struct (`Value::Struct`, the form a `Matrix{Num}` literal produces —
        // Issue #7893).
        let elements: Vec<Value> =
            if let Some(arr) = crate::vm::value::native_array_value_ref(&resolved) {
                let arr_borrow = arr.borrow();
                let display_count = arr_borrow.element_count().min(100);
                (0..display_count)
                    .map(|i| arr_borrow.get_linear(i).unwrap_or(Value::Nothing))
                    .collect()
            } else if let Value::Struct(s) = &resolved {
                match crate::vm::formatting::array_wrapper_elements(s) {
                    Some(els) => els.into_iter().take(100).collect(),
                    None => return None,
                }
            } else {
                return None;
            };

        let mut prerendered: Vec<Option<String>> = vec![None; elements.len()];
        let mut any_rendered = false;
        for (i, elt) in elements.iter().enumerate() {
            let elt_resolved =
                crate::vm::formatting::resolve_struct_refs_for_format(elt, &self.struct_heap);
            if self.user_show_method_for(&elt_resolved).is_none() {
                continue;
            }
            if let Some(s) = self.render_value_via_user_show(&elt_resolved) {
                prerendered[i] = Some(s);
                any_rendered = true;
            }
        }

        if !any_rendered {
            return None;
        }

        // Re-dispatch to the carrier-appropriate formatter with the spliced-in
        // per-element `show` output.
        if let Some(arr) = crate::vm::value::native_array_value_ref(&resolved) {
            Some(crate::vm::formatting::format_array_value_prerendered(
                arr,
                &prerendered,
            ))
        } else if let Value::Struct(s) = &resolved {
            crate::vm::formatting::format_array_wrapper_prerendered(s, &prerendered)
        } else {
            None
        }
    }
}
