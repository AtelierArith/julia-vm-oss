#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

// Submodules
mod broadcast;
mod builtins_arrays;
mod builtins_collections;
mod builtins_dicts;
mod builtins_equality;
mod builtins_exec;
mod builtins_io;
pub mod builtins_linalg;
mod builtins_macro;
mod builtins_math;
mod builtins_numeric;
mod builtins_reflection;
mod builtins_stacktrace;
mod builtins_stats;
mod builtins_strings;
mod builtins_types;
mod builtins_types_conversion;
mod convert;
mod dispatch;
mod dispatch_binding;
mod dynamic_ops;
mod equality;
// Owned by subset_julia_vm_bytecode (Issue #8656); alias keeps `crate::vm::error::*` paths valid.
pub use subset_julia_vm_bytecode::error;
pub(crate) mod exec;
mod executable;
pub(crate) use executable::try_predecode_f64_function;
pub(crate) use executable::try_predecode_i64_function;
#[doc(hidden)]
pub use executable::{
    F64FunctionBlock, F64FunctionBuilder, F64FunctionOp, F64FunctionSlot, F64Relation,
    ScalarFunctionBlock, ScalarFunctionBuilder, ScalarFunctionOp, ScalarFunctionSlot,
    ScalarRelation,
};
pub(crate) use subset_julia_vm_bytecode::field_indices;
mod formatting;
// Display-only Complex{FloatNN} → ComplexFNN alias helper, needed by the FFI and
// the `sjulia` binary's own value formatter (a separate crate) — Issue #5704.
pub use formatting::apply_complex_float_aliases;
// Julia 1.12-faithful BigFloat display, needed by the FFI value formatter
// (which has its own `format_value`) — Issue #6789.
pub use formatting::format_bigfloat_julia;
mod frame;
mod hof_exec;
pub mod instr;
pub(crate) mod intrinsics_exec;
mod main_scope_visibility;
mod matmul;
mod narrow_int_arith;
pub(crate) mod native_array_compat;
mod numeric_identity;
// peephole/slot live in subset_julia_vm_bytecode (Issue #8656); the compile
// facade and runtime call them there directly.
mod builtins_tasks;
pub(crate) mod complex_fastpath_gate;
pub(crate) mod generator_fastpath_gate;
pub mod profiler;
pub(crate) mod register_gate;
pub mod repl_support;
pub mod specialize;
pub(crate) mod splat;
pub mod stack_metrics;
pub mod stack_ops;
mod state;
mod struct_setup;
#[cfg(test)]
mod tests;
mod type_objects;
mod type_ops;
pub(crate) mod type_utils;
mod value_field_projection;
pub use subset_julia_vm_bytecode::program as types;
pub mod util;
pub use subset_julia_vm_bytecode::value;

// Re-exports from types module
pub use types::{
    AbstractTypeDefInfo, CompiledProgram, EvalDefinedMethod, FunctionInfo, I64SpecDispatch,
    KwParamInfo, PrimitiveTypeDefInfo, RuntimeCompileContext, ShowMethodEntry,
    SpecializableFunction, SpecializationKey, SpecializedCode, StructDefInfo,
};

// Re-exports
pub use error::{SpannedVmError, VmError, VmStackFrame};
pub use frame::VarTypeTag;
pub use instr::Instr;
pub use subset_julia_vm_bytecode::{
    ArrayLiteralPayload, CallDirectSlots, CallSpecializeSlots, CallVarKwargsSplat,
    DefineRuntimeNominalOperands, DynamicCallCandidate, EnumDefInfo, GeneratorCallableSpec,
    InvokeWithKwargs, MakeGeneratorOperands, ModuleOperands, NativeIteratorKind,
    RegisterEnumOperands, ReplDefinitionActivation, ResolvedClosureOperands,
    ResolvedFunctionOperands, RuntimeNominalActivation, RuntimeNominalDefInfo, StaticParamBinding,
    StaticParametricCall, StaticParametricFallback, TypedDispatchStoreDict,
};
// Issue #8559 measurement gates: the register VM process override (for hosts
// without an environment, e.g. wasm32) and the opt-in stack VM counters.
pub use complex_fastpath_gate::set_complex_fastpath_disabled;
pub use generator_fastpath_gate::set_generator_fastpath_disabled;
pub use register_gate::set_register_vm_forced;
pub use stack_metrics::{set_stack_vm_metrics_forced, StackVmMetrics};
// Issue #8562 handler-table dispatch experiment gate (feature builds only).
#[cfg(feature = "vm-handler-table")]
pub use exec::handler_table::set_handler_table_forced;
pub use stack_ops::{StackOps, StackOpsExt};
pub use value::{
    new_array_ref,
    new_typed_array_ref,
    new_weak_ref,
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
    WeakRefCell,
};

// Internal imports
use crate::inference_core::dispatch_resolver::runtime_julia_type_contains_type_var;
use dispatch_binding::{
    bind_array_rank_type_param, bind_val_parameter_value, parse_val_char_parameter,
    parse_val_constructor_parameter, parse_val_tuple_parameter, parse_value_type_param_literal,
    split_top_level_comma,
};
// Shared candidate-signature derivation for the structured Instr payload
// migration (Issue #6496).
use frame::{Frame, Handler};
use hof_exec::state::{
    BroadcastState, ComposedCallState, GeneratorIterateState, RedirectState, SprintState,
};
use native_array_compat::{
    base_function_accepts_native_array_value, is_native_array_value, native_array_value_ptr_eq,
    params_cross_native_array_wrapper_boundary,
};
use numeric_identity::{numeric_integer_values_equal, numeric_integer_values_identical};
use struct_setup::{
    append_type_ancestors, build_struct_hierarchy_from_program, compute_type_ancestors,
    normalize_method_struct_def,
};
pub(crate) use subset_julia_vm_bytecode::expanded_param_types_for_call;
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
// Issue #9197 slice 2: the L1 call-site inline cache keys on interned concrete
// type ids instead of an unverified u64 hash.
use crate::types::{nominal_family_name, StructHierarchy};
use smallvec::SmallVec;
use std::cell::Cell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::CString;
use std::hash::{Hash, Hasher};
use std::os::raw::{c_char, c_void};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FinalizerTarget {
    Struct(usize),
    Shared(usize),
}

#[derive(Debug, Clone)]
pub(crate) struct FinalizerEntry {
    target: FinalizerTarget,
    callback: Value,
    object_snapshot: Value,
    active: bool,
}
use subset_julia_vm_bytecode::{
    ConcreteTypeId, ConcreteTypeKey, ModuleId, ModuleInternTable, TypeInternTable,
};

/// Hash a type name string to a u64 key for the dispatch cache (Issue #3355).
/// Avoids storing String keys in the hot dispatch path.
#[inline]
pub(crate) fn hash_type_name(name: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    hasher.finish()
}

/// L1 call-site inline cache key: the interned dispatch-type [`ConcreteTypeId`]
/// of each argument, in order (Issue #9197, slice 2).
///
/// Small-buffer-optimized for the common ≤4-argument case so a cache hit
/// compares a handful of `u32`s inline with no heap access — the exact-sequence
/// equality that replaces the old probabilistic `u64` hash match (the upstream
/// `sig_match_fast` analogue).
pub(crate) type CallSiteArgIds = SmallVec<[ConcreteTypeId; 4]>;

/// Canonical Julia names for the scalar `Value` kinds that participate in the L1
/// call-site cache, in a fixed order. The index into this array is the index
/// into [`CallSitePrimitiveTables::value_scalar`]; the order MUST stay in sync
/// with [`call_site_value_scalar_index`].
const CALL_SITE_VALUE_PRIMITIVE_NAMES: [&str; 23] = [
    "Int8",
    "Int16",
    "Int32",
    "Int64",
    "Int128",
    "BigInt",
    "UInt8",
    "UInt16",
    "UInt32",
    "UInt64",
    "UInt128",
    "Float16",
    "Float32",
    "Float64",
    "BigFloat",
    "Bool",
    "String",
    "Char",
    "Nothing",
    "Missing",
    "Symbol",
    "Regex",
    "RegexMatch",
];

/// Canonical Julia names for the payload-free `ArrayElementType` tags that get a
/// pre-interned id for the array/memory element fast path. The index is the
/// index into [`CallSitePrimitiveTables::array_scalar`]; the order MUST stay in
/// sync with [`array_element_scalar_index`].
const CALL_SITE_ARRAY_ELEMENT_NAMES: [&str; 22] = [
    "Float32",
    "Float64",
    "Complex{Float32}",
    "Complex{Float64}",
    "Int8",
    "Int16",
    "Int32",
    "Int64",
    "Int128",
    "UInt8",
    "UInt16",
    "UInt32",
    "UInt64",
    "UInt128",
    "Bool",
    "String",
    "SubString{String}",
    "Char",
    "Symbol",
    "Nothing",
    "Any",
    "Float16",
];

/// Pre-interned [`ConcreteTypeId`]s for the scalar value kinds and scalar array
/// element kinds that dominate the L1 call-site path (Issue #9197, slice 2).
///
/// Built once per VM against the VM's [`TypeInternTable`]; the hot id-derivation
/// path ([`call_site_arg_type_id`] / [`intern_array_element_type`]) indexes these
/// arrays instead of re-building a string key and probing the intern `HashMap`,
/// so mapping a `Float64` — or a `Vector{Float64}` element — to its id is a plain
/// array read. This is why the exact-id key is *cheaper* than the old hash fold
/// on the hot path (the id-comparison-should-be-faster-than-hashing requirement
/// of slice 2).
#[derive(Debug, Clone)]
pub(crate) struct CallSitePrimitiveTables {
    value_scalar: [ConcreteTypeId; CALL_SITE_VALUE_PRIMITIVE_NAMES.len()],
    array_scalar: [ConcreteTypeId; CALL_SITE_ARRAY_ELEMENT_NAMES.len()],
}

/// Build a fresh [`TypeInternTable`] pre-seeded with the scalar value /
/// array-element ids and the matching [`CallSitePrimitiveTables`] (Issue #9197,
/// slice 2). Both VM constructors call this so the two fields share one table.
pub(crate) fn build_call_site_intern_tables() -> (TypeInternTable, CallSitePrimitiveTables) {
    let mut table = TypeInternTable::new();
    let value_scalar =
        std::array::from_fn(|i| table.intern_primitive(CALL_SITE_VALUE_PRIMITIVE_NAMES[i]));
    let array_scalar =
        std::array::from_fn(|i| table.intern_primitive(CALL_SITE_ARRAY_ELEMENT_NAMES[i]));
    (
        table,
        CallSitePrimitiveTables {
            value_scalar,
            array_scalar,
        },
    )
}

/// Index of a scalar `Value` kind in [`CallSitePrimitiveTables::value_scalar`],
/// or `None` for a non-scalar kind (which then interns a structural key or skips
/// L1). MUST stay in sync with [`CALL_SITE_VALUE_PRIMITIVE_NAMES`]; this is the
/// exact set of scalar kinds the removed `hash_call_site_value_tag` tagged.
#[inline]
fn call_site_value_scalar_index(value: &Value) -> Option<usize> {
    Some(match value {
        Value::I8(_) => 0,
        Value::I16(_) => 1,
        Value::I32(_) => 2,
        Value::I64(_) => 3,
        Value::I128(_) => 4,
        Value::BigInt(_) => 5,
        Value::U8(_) => 6,
        Value::U16(_) => 7,
        Value::U32(_) => 8,
        Value::U64(_) => 9,
        Value::U128(_) => 10,
        Value::F16(_) => 11,
        Value::F32(_) => 12,
        Value::F64(_) => 13,
        Value::BigFloat(_) => 14,
        Value::Bool(_) => 15,
        Value::Str(_) => 16,
        Value::Char(_) | Value::CharMalformed(_) => 17,
        Value::Nothing => 18,
        Value::Missing => 19,
        Value::Symbol(_) => 20,
        Value::Regex(_) => 21,
        Value::RegexMatch(_) => 22,
        _ => return None,
    })
}

/// Index of a payload-free `ArrayElementType` in
/// [`CallSitePrimitiveTables::array_scalar`], or `None` for a structured tag
/// (`StructOf`/`StructInlineOf`/`TupleOf`/`UnionOf`/`Abstract`). MUST stay in
/// sync with [`CALL_SITE_ARRAY_ELEMENT_NAMES`].
#[inline]
fn array_element_scalar_index(elem: &ArrayElementType) -> Option<usize> {
    Some(match elem {
        ArrayElementType::F32 => 0,
        ArrayElementType::F64 => 1,
        ArrayElementType::ComplexF32 => 2,
        ArrayElementType::ComplexF64 => 3,
        ArrayElementType::I8 => 4,
        ArrayElementType::I16 => 5,
        ArrayElementType::I32 => 6,
        ArrayElementType::I64 => 7,
        ArrayElementType::I128 => 8,
        ArrayElementType::U8 => 9,
        ArrayElementType::U16 => 10,
        ArrayElementType::U32 => 11,
        ArrayElementType::U64 => 12,
        ArrayElementType::U128 => 13,
        ArrayElementType::Bool => 14,
        ArrayElementType::String => 15,
        ArrayElementType::SubString => 16,
        ArrayElementType::Char => 17,
        ArrayElementType::Symbol => 18,
        ArrayElementType::Nothing => 19,
        ArrayElementType::Any => 20,
        ArrayElementType::F16 => 21,
        _ => return None,
    })
}

/// Intern an array/memory element `ArrayElementType` to its [`ConcreteTypeId`],
/// injectively (distinct element types ⇒ distinct ids), matching the identity
/// the removed hasher folded via `ArrayElementType::hash`.
///
/// Scalar (payload-free) tags resolve through the pre-interned
/// `tables.array_scalar` by index — allocation-free, so `Vector{Float64}`
/// dispatch (the dominant array-wrapper case, e.g. `norm2([x, y])` in the
/// dispatch benchmark) stays on the fast path. The rare structured tags intern a
/// `Debug`-derived primitive name, which is injective over the whole value;
/// structured decomposition of those is slice S4.
fn intern_array_element_type(
    intern: &mut TypeInternTable,
    elem: &ArrayElementType,
    tables: &CallSitePrimitiveTables,
) -> ConcreteTypeId {
    if let Some(idx) = array_element_scalar_index(elem) {
        return tables.array_scalar[idx];
    }
    intern.intern(ConcreteTypeKey::Primitive(
        format!("array-elem::{elem:?}").into(),
    ))
}

/// Interned dispatch-type id of a struct instance (Issue #9197, slice 2), the
/// id-producing analogue of the removed `hash_struct_dispatch_identity`.
///
/// * **Array wrapper structs** (`Array{T,N}`): the dispatch identity is
///   `(element, ndims)`, re-derived exactly as before — NOT the wrapper's
///   `type_id` (one `type_id` covers `SubArray{Int64,1}` and `SubArray{Float64,
///   2}`). A non-Memory-backed legacy carrier yields `None` → skip L1.
/// * **All other structs**: the dispatch identity is the fully-resolved
///   `struct_name`, cloned into the key by an `Rc<str>` refcount bump. The whole
///   resolved name carries the parameters (empty `params`); structured param
///   decomposition is slice S4.
fn struct_dispatch_type_id(
    s: &StructInstance,
    intern: &mut TypeInternTable,
    tables: &CallSitePrimitiveTables,
) -> Option<ConcreteTypeId> {
    if s.array_wrapper_julia_type().is_some() {
        let (elem_type, ndims) = s.array_wrapper_element_array_type()?;
        let element = intern_array_element_type(intern, &elem_type, tables);
        let ndims = u16::try_from(ndims).ok()?;
        Some(intern.intern(ConcreteTypeKey::Array { element, ndims }))
    } else {
        Some(intern.intern(ConcreteTypeKey::Struct {
            name: Rc::clone(&s.struct_name),
            params: Vec::new(),
        }))
    }
}

fn range_call_site_is_step(r: &RangeValue) -> bool {
    r.is_explicit_float_type()
        || matches!(r.element_type, value::RangeElementType::Char)
        || !r.is_unit_range()
}

fn range_call_site_step_type_name(r: &RangeValue) -> &'static str {
    if r.is_explicit_float_type() && matches!(r.step_type, value::RangeElementType::Default) {
        r.element_type_name()
    } else {
        r.step_type.julia_type_name()
    }
}

/// Interned dispatch-type id of one argument value (Issue #9197, slice 2; the
/// previously-untracked kinds re-caching is slice 5, pulled forward by Issue
/// #9427), the id-producing analogue of the removed `hash_call_site_value_tag`.
///
/// Scalar primitives resolve through the pre-interned `tables.value_scalar` by
/// array index (no allocation, no `HashMap` probe); composite kinds
/// (`Struct`/`Array`/`Tuple`/`NamedTuple`/`Range`/`Memory`/`Enum`) intern a small
/// structural key.
///
/// **Opaque / singleton kinds (Issue #9427).** Closures, function values,
/// `DataType` (`Type{T}`), `Module`, `IO`, generators, RNGs, and the macro-AST
/// / reflection singletons intern a [`ConcreteTypeKey::Opaque`] carrying the
/// value's dispatch-name string — *exactly* the pre-#9404 `get_type_name` /
/// `dynamic_dispatch_type_name` string the retired L2 cache keyed on. This
/// re-caches them (they had regressed to full re-resolution every call under
/// S3, Issue #9427). `Ref(T)` recurses into its element for a structural
/// `Base.RefValue{T}` key. Distinct dispatch names ⇒ distinct ids, so the id
/// partition equals the (correct) pre-#9404 partition — no new conflation.
///
/// Returns `None` (skip L1/L2, re-resolve) only for the rare, dispatch-lossy
/// carriers `ExprArgs` (legacy `Vector{Any}` expr-args carrier), `Pairs`
/// (kwargs carrier; its old name key was already first-value-lossy), and
/// `Undef` (`#undef`) — none of which are hot dispatch args (see
/// `docs/vm/TYPE_INTERNING.md` §"Slice 5"), or when any nested element of a
/// composite is itself untracked.
fn call_site_arg_type_id(
    value: &Value,
    struct_heap: &[StructInstance],
    intern: &mut TypeInternTable,
    tables: &CallSitePrimitiveTables,
) -> Option<ConcreteTypeId> {
    if let Some(idx) = call_site_value_scalar_index(value) {
        return Some(tables.value_scalar[idx]);
    }
    match value {
        // `Struct` and `StructRef` share the same helper because both dispatch by
        // the same struct type identity. A `StructRef` resolves through the heap;
        // safe-point compaction preserves the instance, and a dangling index
        // (should not happen mid-dispatch) conservatively skips L1.
        Value::Struct(s) => struct_dispatch_type_id(s, intern, tables),
        Value::StructRef(idx) => {
            let s = struct_heap.get(*idx)?;
            struct_dispatch_type_id(s, intern, tables)
        }
        // Tuple dispatch identity is `Tuple{T1, …}` — recurse into elements so
        // `(1, 2.0)` and `(1, 2)` never share an id. Any untracked element skips
        // L1 for the whole call site.
        Value::Tuple(t) => {
            let mut params = Vec::with_capacity(t.elements.len());
            for e in &t.elements {
                params.push(call_site_arg_type_id(e, struct_heap, intern, tables)?);
            }
            Some(intern.intern(ConcreteTypeKey::Tuple(params)))
        }
        // NamedTuple identity includes the field names AND the element types.
        Value::NamedTuple(nt) => {
            let mut params = Vec::with_capacity(nt.values.len());
            for v in &nt.values {
                params.push(call_site_arg_type_id(v, struct_heap, intern, tables)?);
            }
            let names = nt.names.iter().map(|n| n.as_str().into()).collect();
            Some(intern.intern(ConcreteTypeKey::NamedTuple { names, params }))
        }
        // Range identity: include both promoted element type `T` and explicit
        // step type `S` so `StepRange{T,S}` method caches do not conflate
        // ranges whose endpoints promote differently from the step (Issue #9519).
        Value::Range(r) => {
            let element = intern.intern_type_name(r.element_type_name());
            let is_step = range_call_site_is_step(r);
            let step = if is_step {
                intern.intern_type_name(range_call_site_step_type_name(r))
            } else {
                element
            };
            Some(intern.intern(ConcreteTypeKey::Range {
                element,
                step,
                is_float: r.is_float,
                is_step,
            }))
        }
        // Memory{T}: identity is a pure function of the element type.
        Value::Memory(mem) => {
            let elem = mem.borrow().element_type().clone();
            let element = intern_array_element_type(intern, &elem, tables);
            Some(intern.intern(ConcreteTypeKey::Memory { element }))
        }
        // Enum: dispatch identity is the enum type name itself.
        Value::Enum { type_name, .. } => {
            Some(intern.intern(ConcreteTypeKey::Enum(type_name.as_str().into())))
        }

        // ── Opaque / singleton dispatch kinds (Issue #9427) ───────────────────
        // Each interns the pre-#9404 `get_type_name`/`dynamic_dispatch_type_name`
        // dispatch-name string as `Opaque`, restoring the L2 caching that S3
        // dropped. The strings mirror `type_ops/introspection.rs::get_type_name`
        // arm-for-arm so the id partition equals the retired string-key one.

        // `Type{T}` type-object: `dynamic_dispatch_type_name` rendered a
        // `DataType` as `Type{<name>}` (parameter-inclusive via `JuliaType::name`),
        // so `f(Int)` and `f(Float64)` keep distinct dispatch keys. This is the
        // one kind whose key is finer than bare `get_type_name` ("DataType").
        Value::DataType(jt) => Some(intern.intern(ConcreteTypeKey::Opaque(
            format!("Type{{{}}}", jt.name()).into(),
        ))),
        // Named functions and closures carry a position-independent singleton
        // identity. Candidate indices are deliberately absent from this cache
        // key because REPL rebuilds relocate them; the stable identity already
        // distinguishes module owners and source/lowering-helper provenance
        // (Issues #11203/#11216/#11685).
        Value::Function(f) => {
            Some(intern.intern(ConcreteTypeKey::Opaque(f.singleton_dispatch_key().into())))
        }
        Value::Closure(cv) => {
            Some(intern.intern(ConcreteTypeKey::Opaque(cv.singleton_dispatch_key().into())))
        }
        // All composed functions share the `ComposedFunction` dispatch identity
        // (matching the retired `get_type_name`).
        Value::ComposedFunction(_) => {
            Some(intern.intern(ConcreteTypeKey::Opaque("ComposedFunction".into())))
        }
        // Every module value dispatches as the single `Module` type (Issue #5005).
        Value::Module(_) => Some(intern.intern(ConcreteTypeKey::Opaque("Module".into()))),
        Value::IO(io_ref) => {
            let name = if io_ref.borrow().is_pipe() {
                "Pipe"
            } else {
                "IOBuffer"
            };
            Some(intern.intern(ConcreteTypeKey::Opaque(name.into())))
        }
        Value::Generator(_) => {
            Some(intern.intern(ConcreteTypeKey::Opaque("Base.Generator".into())))
        }
        // Concrete RNG type (the global handle reports `TaskLocalRNG`, #7230/#7231).
        Value::Rng(rng) => {
            let name = match rng {
                RngInstance::Stable(_) => "StableRNG",
                RngInstance::Xoshiro(_) => "Xoshiro",
                RngInstance::Mersenne(_) => "MersenneTwister",
                RngInstance::Global => "TaskLocalRNG",
            };
            Some(intern.intern(ConcreteTypeKey::Opaque(name.into())))
        }
        Value::RuntimeTypeVar(_) => Some(intern.intern(ConcreteTypeKey::Opaque("TypeVar".into()))),
        Value::RuntimeTypeName(_) => {
            Some(intern.intern(ConcreteTypeKey::Opaque("Core.TypeName".into())))
        }
        Value::SimpleVector(_) => {
            Some(intern.intern(ConcreteTypeKey::Opaque("Core.SimpleVector".into())))
        }
        // Macro-system AST singletons (hot in Symbolics / macro-heavy packages).
        Value::Expr(_) => Some(intern.intern(ConcreteTypeKey::Opaque("Expr".into()))),
        Value::QuoteNode(_) => Some(intern.intern(ConcreteTypeKey::Opaque("QuoteNode".into()))),
        Value::LineNumberNode(_) => {
            Some(intern.intern(ConcreteTypeKey::Opaque("LineNumberNode".into())))
        }
        Value::GlobalRef(_) => Some(intern.intern(ConcreteTypeKey::Opaque("GlobalRef".into()))),
        Value::Binding(_) => Some(intern.intern(ConcreteTypeKey::Opaque("Core.Binding".into()))),
        // `:` slice marker dispatches as `Colon`.
        Value::SliceAll => Some(intern.intern(ConcreteTypeKey::Opaque("Colon".into()))),
        // Flat static-array reps carry their full parametric Julia type name
        // (pure method, no VM context) — sound exact identity.
        Value::StaticArray(sv) => {
            Some(intern.intern(ConcreteTypeKey::Opaque(sv.julia_type_name().into())))
        }
        Value::StaticArrayInline(sv) => {
            Some(intern.intern(ConcreteTypeKey::Opaque(sv.julia_type_name_owned())))
        }
        Value::MemoryRef(memref) => {
            let elem = memref.element_type();
            let element = intern_array_element_type(intern, &elem, tables);
            Some(intern.intern(ConcreteTypeKey::Struct {
                name: Rc::from("MemoryRef"),
                params: vec![element],
            }))
        }
        // `Base.RefValue{T}` recurses structurally into the boxed element; an
        // untracked element (`None`) skips the whole call, matching the composite
        // policy above.
        Value::Ref(inner) => {
            let inner_id = call_site_arg_type_id(&inner.borrow(), struct_heap, intern, tables)?;
            Some(intern.intern(ConcreteTypeKey::Struct {
                name: Rc::from("Base.RefValue"),
                params: vec![inner_id],
            }))
        }
        Value::WeakRef(_) => Some(intern.intern(ConcreteTypeKey::Opaque("WeakRef".into()))),

        // Dispatch-lossy / cold carriers keep skipping L1/L2 (Issue #9427 audit):
        // `ExprArgs` (legacy Vector{Any} expr-args carrier), `Pairs` (kwargs;
        // old name key was first-value-lossy), `Undef` (#undef). Not hot dispatch
        // args; re-resolving them is authoritative and correctness-neutral.
        _ => None,
    }
}

/// Interned per-argument id sequence for the L1 call-site inline cache key
/// (Issue #9197, slice 2), replacing the unverified `u64` fingerprint.
///
/// Returns `None` — skip L1, go straight to L2 — for an empty argument list, or
/// as soon as any argument has no tracked dispatch identity: exactly the old
/// `hash_call_site_fingerprint` policy (`None` ⇒ non-cacheable). On a hit the
/// cache compares these ids by *exact sequence equality*, so unlike the old hash
/// it cannot conflate two distinct argument signatures.
pub(crate) fn call_site_arg_type_ids(
    values: &[&Value],
    struct_heap: &[StructInstance],
    intern: &mut TypeInternTable,
    tables: &CallSitePrimitiveTables,
) -> Option<CallSiteArgIds> {
    if values.is_empty() {
        return None;
    }
    let mut ids = CallSiteArgIds::new();
    for value in values {
        ids.push(call_site_arg_type_id(value, struct_heap, intern, tables)?);
    }
    Some(ids)
}

// The runtime type-name string parsers `parametric_type_args` /
// `split_parametric_args` / `substitute_parent_type_arg` were retired here in
// Issue #9197 slice 4. Their only remaining consumer was the cold display-path
// show-method supertype walk (`projected_direct_parent_type_name`), which now
// walks parent *family* names directly. The canonical structural type-name
// decomposition (rendered name → interned `ConcreteTypeId` DAG, parsed once) now
// lives in `subset_julia_vm_bytecode::TypeInternTable::intern_type_name`.

#[derive(Clone)]
struct RuntimeCandidateMatch {
    idx: usize,
    param_types: Vec<crate::types::JuliaType>,
    score: u32,
    specificity: i32,
    is_vararg: bool,
}

/// Process-wide off-switch for the per-call-site dispatch inline caches
/// (Issue #8561), read at `Vm` construction. Exists for A/B measurement
/// (`bin/dispatch_inline_cache_bench_8561.rs`) and for debugging suspected
/// cache misbehavior; mirrors the `set_register_vm_forced` /
/// `set_stack_vm_metrics_forced` process-override pattern (#8558/#8559) so
/// it also works on targets without an environment (wasm32, iOS harnesses).
/// OR-ed with `SJULIA_DISPATCH_INLINE_CACHE_OFF=1`.
static INLINE_CACHE_FORCED_OFF: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Disable (or re-enable) the call-site dispatch inline caches for
/// subsequently constructed `Vm`s (Issue #8561). Baseline switch for
/// dispatch benchmarks; production hosts never call this.
pub fn set_call_site_inline_cache_disabled(disabled: bool) {
    INLINE_CACHE_FORCED_OFF.store(disabled, std::sync::atomic::Ordering::Relaxed);
}

fn call_site_inline_cache_disabled_from_env() -> bool {
    INLINE_CACHE_FORCED_OFF.load(std::sync::atomic::Ordering::Relaxed)
        || std::env::var("SJULIA_DISPATCH_INLINE_CACHE_OFF")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
}

/// Function-index encoding inside a [`CallSiteCache`] way (Issue #8561).
///
/// Ways store `u32` indices to keep the per-instruction side table small
/// (`call_site_caches` has one entry per bytecode instruction). `u32::MAX`
/// encodes the callers' `usize::MAX` "no method / builtin fallback" sentinel;
/// any other index ≥ `u32::MAX` is refused at store time (never produced in
/// practice — the function table is far smaller).
const CALL_SITE_WAY_SENTINEL: u32 = u32::MAX;

#[inline]
fn call_site_way_encode(func_index: usize) -> Option<u32> {
    if func_index == usize::MAX {
        Some(CALL_SITE_WAY_SENTINEL)
    } else {
        u32::try_from(func_index)
            .ok()
            .filter(|&idx| idx != CALL_SITE_WAY_SENTINEL)
    }
}

#[inline]
fn call_site_way_decode(encoded: u32) -> usize {
    if encoded == CALL_SITE_WAY_SENTINEL {
        usize::MAX
    } else {
        encoded as usize
    }
}

/// Per-call-site inline cache slot for dynamic dispatch (Issues #6345, #8561).
///
/// One entry per bytecode instruction, indexed by the call-site IP — an
/// in-memory side table, deliberately NOT part of the serialized `Instr`
/// layout (bincode caches stay untouched; no #8444 fingerprint churn). The
/// table is built at `Vm` construction, i.e. **after** the compile-time
/// #8555 `refresh_cached_base_dispatch_candidates` pass has rewritten the
/// `CallTypedDispatch`/`CallDynamic`-family candidate lists in cached Base
/// bytecode, so no slot can ever be filled from a pre-refresh candidate
/// list.
///
/// Two ways of `(exact interned arg-id sequence, resolved function index)` in
/// MRU order, plus the [`Vm::dispatch_generation`] value the slot was last
/// filled in. A slot whose generation differs from the VM's current
/// generation is entirely stale (miss); the generation is bumped by
/// [`Vm::note_method_table_mutation`] — the coarse whole-clear used when the
/// mutated generic function is not known to the caller. When it *is* known
/// (the sole production path, `activate_eval_function`),
/// [`Vm::note_method_table_mutation_for`] instead vacates only the ways whose
/// resolved target belongs to the mutated generic function (or the builtin
/// fallback), leaving unrelated slots warm — the Issue #9197 S6 per-name
/// backedge invalidation, mirroring upstream `invalidate_backedges`
/// (`julia/src/gf.c`) adapted to sjulia's flat function table.
///
/// Issue #9197 slice 2: a way's key is the [`CallSiteArgIds`] of interned
/// [`ConcreteTypeId`]s, and a hit requires **exact id-sequence equality**, not
/// the old unverified `u64` hash match — so the L1 cache can no longer conflate
/// two distinct argument signatures (e.g. `SubArray{Int64,1}` vs
/// `SubArray{Float64,2}`). An empty key is the vacant-way sentinel; a real key
/// always has ≥1 element (empty argument lists skip L1).
#[derive(Debug, Clone)]
struct CallSiteCache {
    /// `Vm::dispatch_generation` at fill time; mismatch = whole slot stale.
    generation: u64,
    /// MRU way key (empty = vacant).
    key: CallSiteArgIds,
    func_index: u32,
    /// LRU way key (empty = vacant).
    key2: CallSiteArgIds,
    func_index2: u32,
}

impl Default for CallSiteCache {
    fn default() -> Self {
        Self {
            generation: 0,
            key: CallSiteArgIds::new(),
            func_index: CALL_SITE_WAY_SENTINEL,
            key2: CallSiteArgIds::new(),
            func_index2: CALL_SITE_WAY_SENTINEL,
        }
    }
}

impl CallSiteCache {
    /// Look up `key` (a non-empty interned arg-id sequence) in this slot by exact
    /// sequence equality. A hit on the LRU way promotes it to MRU. Entries filled
    /// in an older generation are misses.
    #[inline]
    fn lookup(&mut self, key: &[ConcreteTypeId], generation: u64) -> Option<usize> {
        if key.is_empty() || self.generation != generation {
            return None;
        }
        if self.key.as_slice() == key {
            return Some(call_site_way_decode(self.func_index));
        }
        if self.key2.as_slice() == key {
            std::mem::swap(&mut self.key, &mut self.key2);
            std::mem::swap(&mut self.func_index, &mut self.func_index2);
            return Some(call_site_way_decode(self.func_index));
        }
        None
    }

    /// Fill the MRU way with `(key, func_index)` at `generation`, demoting the
    /// previous MRU way to LRU. A stale-generation slot is reset first so no
    /// old-generation way can survive alongside a fresh one. The way keys reuse
    /// their existing `SmallVec` capacity (`clear` + `extend_from_slice`), so a
    /// re-store on the hot path does not re-allocate.
    #[inline]
    fn store(&mut self, key: &[ConcreteTypeId], func_index: usize, generation: u64) {
        if key.is_empty() {
            return;
        }
        let Some(encoded) = call_site_way_encode(func_index) else {
            return;
        };
        if self.generation != generation {
            self.generation = generation;
            self.key.clear();
            self.key.extend_from_slice(key);
            self.func_index = encoded;
            self.key2.clear();
            self.func_index2 = CALL_SITE_WAY_SENTINEL;
            return;
        }
        if self.key.as_slice() == key {
            self.func_index = encoded;
            return;
        }
        // Demote the current MRU way to LRU, then install the new MRU key. The
        // swap moves the old MRU key into `key2` (old LRU key lands in `key`,
        // about to be overwritten) without cloning.
        std::mem::swap(&mut self.key, &mut self.key2);
        self.func_index2 = self.func_index;
        self.key.clear();
        self.key.extend_from_slice(key);
        self.func_index = encoded;
    }

    /// Vacate only the occupied ways whose resolved function index satisfies
    /// `affected`, keeping every other way warm (Issue #9197 S6 — precise
    /// per-name backedge invalidation instead of a whole-slot generation bump).
    ///
    /// `affected(func_index)` returns `true` for a way that must be dropped
    /// because a method (re)definition may change its dispatch decision. A
    /// surviving LRU way is compacted up to the MRU slot so the fast MRU path
    /// stays warm. A vacant way (empty key) is never inspected — its sentinel
    /// `func_index` would otherwise be misread as the builtin fallback.
    #[inline]
    fn invalidate_ways(&mut self, mut affected: impl FnMut(usize) -> bool) {
        let drop_mru = !self.key.is_empty() && affected(call_site_way_decode(self.func_index));
        let drop_lru = !self.key2.is_empty() && affected(call_site_way_decode(self.func_index2));
        if drop_mru {
            if drop_lru || self.key2.is_empty() {
                // Both ways go (or the LRU way was already vacant): clear MRU.
                self.key.clear();
                self.func_index = CALL_SITE_WAY_SENTINEL;
            } else {
                // Promote the surviving LRU way into the MRU slot.
                std::mem::swap(&mut self.key, &mut self.key2);
                self.func_index = self.func_index2;
            }
            self.key2.clear();
            self.func_index2 = CALL_SITE_WAY_SENTINEL;
        } else if drop_lru {
            self.key2.clear();
            self.func_index2 = CALL_SITE_WAY_SENTINEL;
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

/// Lightweight VM memory/cache counters for long-running hosts (Issue #8453).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmMemoryStats {
    pub struct_heap_len: usize,
    pub struct_heap_capacity: usize,
    pub frame_pool_len: usize,
    pub frame_pool_capacity: usize,
    pub dispatch_cache_entries: usize,
    pub binary_both_dispatch_cache_entries: usize,
    pub method_dispatch_cache_entries: usize,
    pub specialization_cache_entries: usize,
    pub specialization_i64_cache_entries: usize,
    /// Float64 mirror of the all-typed specialize dispatch cache (Issue #10491).
    pub specialization_f64_cache_entries: usize,
    pub i64_function_cache_entries: usize,
    pub f64_function_cache_entries: usize,
    pub binary_method_cache_entries: usize,
    pub generated_expr_cache_entries: usize,
    /// Number of hard-cap cache clears fired since VM construction
    /// (Issue #8625 observability for the #8610 bound).
    pub cache_clears: u64,
    /// Total cache entries discarded by hard-cap clears (Issue #8625).
    pub cache_cleared_entries: u64,
    /// Effective dispatch-family cache entry cap for this VM (Issue #8625).
    pub dispatch_cache_entry_limit: usize,
    /// Effective specialization-family cache entry cap for this VM
    /// (Issue #8625).
    pub specialization_cache_entry_limit: usize,
    /// Effective host memory budget for this VM in bytes, when configured
    /// (Issues #8702/#8703).
    pub memory_budget_bytes: Option<usize>,
    /// Approximate VM-side waterline in bytes, used by the intermittent budget
    /// safe-point check (Issue #8703). This is intentionally not a precise
    /// allocator counter; it estimates reachable VM containers and cache entries.
    pub estimated_memory_waterline_bytes: usize,
}

/// Runtime definition boundary captured after a REPL delta is installed but
/// before its source-ordered main executes (Issue #9784).
#[derive(Debug, Clone, Copy)]
pub struct ReplDefinitionWorldFingerprint {
    functions_len: usize,
    active_structs_len: usize,
    pending_structs_len: usize,
    active_abstract_types_len: usize,
    pending_abstract_types_len: usize,
    active_primitive_types_len: usize,
    pending_primitive_types_len: usize,
    active_enums_len: usize,
    pending_enums_len: usize,
    current_world: u64,
}

/// Exact per-kind counts represented by a validated typed activation prefix.
#[derive(Debug, Clone, PartialEq)]
pub struct ReachedReplDefinitionPrefix {
    pub function_count: usize,
    /// Inner-constructor bodies activated by reached runtime nominal sites.
    /// They participate in method worlds but are not source function markers.
    pub runtime_constructor_indices: Vec<usize>,
    pub struct_count: usize,
    pub abstract_type_count: usize,
    pub primitive_type_count: usize,
    pub enum_count: usize,
    pub runtime_nominal_activations: Vec<RuntimeNominalActivation>,
    /// Module/nested function markers that executed without mutating the
    /// definition world. Recovery uses the exact indices instead of guessing
    /// ownership from an unqualified leaf name (Issue #11721).
    pub runtime_function_indices: Vec<usize>,
}

/// Projected definition-table growth for one validated live REPL append.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplAppendDefinitionCounts {
    pub function_bodies: usize,
    pub source_functions: usize,
    pub structs: usize,
    pub abstract_types: usize,
    pub primitive_types: usize,
    pub enums: usize,
}

/// First registry index for each definition family validated after a REPL run.
/// Fresh full compiles may place source functions before compiler-generated
/// helper bodies, so these starts are authoritative rather than inferred from
/// the tail length (Issue #11688).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplAppendDefinitionStarts {
    pub functions: usize,
    pub structs: usize,
    pub abstract_types: usize,
    pub primitive_types: usize,
    pub enums: usize,
}

/// Opaque preflight result installed only after its live-VM append is spliced.
pub struct PreparedReplAppendSetup {
    expected_functions_len: usize,
    expected_specializable_prefix_len: usize,
    new_specializable_functions: Vec<SpecializableFunction>,
    refresh_groups: HashMap<usize, Vec<usize>>,
    specializable_updates: HashMap<usize, Vec<(usize, SpecializableFunction)>>,
    world_sensitive_specializable_indices: HashSet<usize>,
}

/// Default hard-cap on the dispatch-family runtime caches (Issue #8610).
/// Hosts can override per VM via [`Vm::set_cache_entry_limits`] or
/// process-wide via [`set_default_cache_entry_limits`] (Issue #8625).
pub const RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT: usize = 4096;
/// Default hard-cap on the specialization-family runtime caches (Issue #8610).
pub const RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT: usize = 4096;

/// Process-wide dispatch-family cache cap override (Issue #8625), `0` = use
/// the [`RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT`] default. Mirrors the
/// `INLINE_CACHE_FORCED_OFF` pattern (#8561) so hosts without an environment
/// (wasm32, iOS harnesses) — and the FFI REPL session, which builds a fresh
/// VM per eval — can tune the cap before VMs are constructed.
static DISPATCH_CACHE_ENTRY_LIMIT_OVERRIDE: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
/// Process-wide specialization-family cache cap override (Issue #8625).
static SPECIALIZATION_CACHE_ENTRY_LIMIT_OVERRIDE: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
/// Process-wide memory budget override in bytes (Issue #8703), `0` = no
/// process default. `SJULIA_MEMORY_BUDGET_BYTES` remains a CLI/test fallback
/// for hosts that can use an environment.
static MEMORY_BUDGET_BYTES_OVERRIDE: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Set (or clear, with `None`) the process-wide default runtime cache entry
/// caps applied to subsequently constructed `Vm`s (Issue #8625). A host that
/// keeps a VM (or an FFI REPL session that rebuilds one per eval) alive for a
/// long time can lower these on a memory-constrained device or raise them on
/// a roomy one. `None` restores the built-in defaults.
///
/// The value is a per-cache entry count, not a byte budget: each dispatch- or
/// specialization-family cache is cleared wholesale once it exceeds its cap
/// (the #8610 hard-cap mechanism), so the ceiling is roughly
/// `caps × number_of_caches` entries.
pub fn set_default_cache_entry_limits(dispatch: Option<usize>, specialization: Option<usize>) {
    DISPATCH_CACHE_ENTRY_LIMIT_OVERRIDE.store(
        dispatch.filter(|&n| n > 0).unwrap_or(0),
        std::sync::atomic::Ordering::Relaxed,
    );
    SPECIALIZATION_CACHE_ENTRY_LIMIT_OVERRIDE.store(
        specialization.filter(|&n| n > 0).unwrap_or(0),
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// Set (or clear, with `None`) the process-wide default VM memory budget in
/// bytes for subsequently constructed `Vm`s (Issue #8703). Hosts without a
/// reliable process environment, including iOS via FFI, use this to inject a
/// device-specific budget before creating/evaluating sessions.
pub fn set_default_memory_budget_bytes(bytes: Option<usize>) {
    MEMORY_BUDGET_BYTES_OVERRIDE.store(
        bytes.filter(|&n| n > 0).unwrap_or(0),
        std::sync::atomic::Ordering::Relaxed,
    );
}

fn memory_budget_bytes_default() -> Option<usize> {
    let forced = MEMORY_BUDGET_BYTES_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);
    if forced > 0 {
        Some(forced)
    } else {
        None
    }
}

fn dispatch_cache_entry_limit_default() -> usize {
    let forced = DISPATCH_CACHE_ENTRY_LIMIT_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);
    if forced > 0 {
        forced
    } else {
        RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT
    }
}

fn specialization_cache_entry_limit_default() -> usize {
    let forced =
        SPECIALIZATION_CACHE_ENTRY_LIMIT_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);
    if forced > 0 {
        forced
    } else {
        RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT
    }
}

/// Result of a safe-point `struct_heap` compaction pass (Issue #8453).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructHeapCompaction {
    pub before_len: usize,
    pub after_len: usize,
    pub reclaimed: usize,
    pub compacted: bool,
}

/// Cached specialization dispatch plus an optional predecoded I64 function
/// block for the hottest `CallSpecializeI64Slots` path. Indexed directly by
/// `spec_func_index` to avoid HashMap lookups on every call.
#[derive(Clone)]
pub(crate) struct I64SpecFastCacheEntry {
    pub arity: usize,
    pub dispatch: I64SpecDispatch,
    /// `None` = not yet attempted, `Some(None)` = predecode failed,
    /// `Some(Some(block))` = success.
    pub predecoded: Option<Option<executable::I64FunctionBlock>>,
}

/// Float64 mirror of [`I64SpecFastCacheEntry`] (Issue #10491). The dispatch
/// metadata is type-agnostic, so it reuses [`I64SpecDispatch`]; only the
/// predecoded frame-less block is F64-typed.
pub(crate) struct F64SpecFastCacheEntry {
    pub arity: usize,
    pub dispatch: I64SpecDispatch,
    /// `None` = not yet attempted, `Some(None)` = predecode failed,
    /// `Some(Some(callee))` = success (pure-F64 or mixed-type frame-less body).
    pub predecoded: Option<Option<executable::ResolvedSpecF64Callee>>,
}

/// Structural identity key for one `UnionAll` binder position in the
/// `runtime_typevar_projection_identities` cache (Issues #10252/#10261/#10987).
///
/// Equality/hashing is decided entirely by structured types — no rendered
/// `String`/`Option<String>` participates (Issue #10987). Component roles:
///
/// - `owner`: the normalized `CoreType` of the wrapper chain's FINAL body
///   (`Vm::runtime_typevar_projection_owner_key`), shared by every suffix
///   view of the same wrapper so `outer.body` reuses `outer`'s identities.
///   The normalization PRESERVES nested `UnionAll` binders (their bounds
///   included) rather than stripping them to their bodies: a wrapper whose
///   outer binder occurs only inside a nested binder's bound
///   (`Tuple{Vector{S} where S<:T} where T` vs `... S<:U} where U`) is
///   otherwise indistinguishable from its renamed sibling, and the two are
///   distinct objects upstream (found by adversarial codex review of the
///   first #10987 attempt).
/// - `binder_depth`: depth from that final body (Issue #10261) — separates
///   nested same-name binders across suffix views.
/// - `declared_lower` / `declared_upper`: the binder's AS-DECLARED bounds,
///   parsed into structural `JuliaType`s (`Bottom`/`Any` when absent). These
///   MUST stay in the key: the owner is derived from the body alone, and
///   under the legacy string-shaped `UnionAll` representation the body does
///   not encode the binder's bounds — so two distinct wrappers sharing a
///   rendered body (`Vector{Int64} where Int64>:Signed` vs
///   `Vector{Int64} where Signed<:Int64<:Real`) collide on `(owner, depth)`
///   and only the bounds distinguish their genuinely distinct binder objects
///   (upstream gives each `where` its own TypeVar). They are `JuliaType`,
///   not `CoreType`: the `CoreType` bridge strips module qualification from
///   parametric names, which would collapse `T<:M1.Box{Int}` with
///   `T<:M2.Box{Int}` (also caught by the codex review), while
///   `JuliaType::from_name_or_struct` keeps the qualified spelling and still
///   normalizes aliases (`Int` -> `Int64`). Structured `UnionAll` bodies
///   that would let the owner itself carry the bounds are Issue #10460's
///   scope. Parsing (rather than comparing rendered strings, the pre-#10987
///   shape) makes the key insensitive to spelling drift: `"Int"` vs
///   `"Int64"`, interval-format reconstruction, whitespace.
///
/// The display NAME is deliberately NOT a component: it lives only on the
/// stored `RuntimeTypeVarValue`. With nested-binder structure preserved in
/// the owner (above), every occurrence position of a binder's name inside
/// the body survives into the owner key, so two wrappers differing only in
/// binder spelling have either identical semantics (phantom binders, which
/// upstream `jl_type_unionall` collapses at construction) or different
/// owner keys — keying on the name only re-minted distinct identities for
/// renamed spellings of the same position.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct TypeVarProjectionKey {
    pub(crate) owner: CoreType,
    pub(crate) binder_depth: usize,
    pub(crate) declared_lower: crate::types::JuliaType,
    pub(crate) declared_upper: crate::types::JuliaType,
}

/// Stable handle into the VM's native-reentry root stack.
///
/// Rust-side preparation code must not retain an authoritative `Value` across
/// a Julia call: explicit `GC.gc()` can compact `StructRef` indices while that
/// call runs. Handles remain stable when nested native operations append more
/// roots, and their slots are remapped by the same GC pass as stack/frame roots
/// (Issue #11372).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::vm) struct TransientRootId {
    index: usize,
    generation: u64,
}

struct TransientRootSlot {
    generation: u64,
    value: Value,
}

/// One enclosing `@testset`'s saved bookkeeping while a nested testset runs
/// (Issue #10338): the counts the enclosing scope had accumulated when the
/// nested `_testset_begin!` fired, plus the name to restore as
/// `current_testset` when the nested set finishes. See `testset_begin_frame`
/// / `testset_end_frame`.
struct TestSetFrame {
    enclosing_name: Option<String>,
    saved_pass: usize,
    saved_fail: usize,
    saved_broken: usize,
    saved_error: usize,
}

/// One module/main lexical environment, separate from frame 0 globals.
///
/// `None` records a declared-but-uninitialized binding, which must shadow an
/// outer lexical binding and a same-named module global (Issues #11569/#9784).
#[derive(Debug, Clone)]
pub(super) struct RootLexicalScope {
    bindings: HashMap<String, Option<Value>>,
}

impl RootLexicalScope {
    fn new(names: &[String]) -> Self {
        Self {
            bindings: names.iter().cloned().map(|name| (name, None)).collect(),
        }
    }

    fn get(&self, name: &str) -> Option<&Option<Value>> {
        self.bindings.get(name)
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut Option<Value>> {
        self.bindings.get_mut(name)
    }

    fn values(&self) -> impl Iterator<Item = &Value> {
        self.bindings.values().flatten()
    }

    fn values_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        self.bindings.values_mut().flatten()
    }

    fn entries(&self) -> impl Iterator<Item = (&String, &Option<Value>)> {
        self.bindings.iter()
    }
}

pub struct Vm<R: RngLike> {
    ip: usize,
    stack: Vec<Value>,
    /// Native Rust locals that must remain live and remappable across
    /// synchronous Julia re-entry. Scopes append roots and truncate back to
    /// their starting depth, so nested splat/indexed-iteration calls compose.
    transient_roots: Vec<TransientRootSlot>,
    /// Monotonic token preventing a handle from silently aliasing a later root
    /// that reused the same vector slot after scope truncation.
    next_transient_root_generation: u64,
    frames: Vec<Frame>,
    /// Task-local module/main lexical environments. These stay distinct from
    /// frame 0 so callees continue to resolve module globals while code emitted
    /// for the lexical body uses the explicit lexical opcodes.
    lexical_scopes: Vec<RootLexicalScope>,
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
    /// Pool of reusable positional-argument vectors for the direct-call paths
    /// (Issue #10103). The general direct-call path pops the callee's arguments
    /// off the value stack into a temporary `Vec<Value>` on every fall-through
    /// call (e.g. every `fib(n)` recursion). Those `Value`s are only *cloned*
    /// into the callee frame's slots — the `Vec` itself is pure scratch and is
    /// dropped once binding completes. Recycling the emptied `Vec` here (mirror
    /// of `frame_pool`, Issue #5172) keeps its heap capacity across calls,
    /// eliminating the per-call `Vec::with_capacity` in tight recursion. Capped
    /// at `MAX_POOLED_ARG_VECS` so deep stacks do not retain unbounded memory.
    arg_vec_pool: Vec<Vec<Value>>,
    return_ips: Vec<usize>,
    handlers: Vec<Handler>,
    /// VM-owned cooperative task continuations (Issue #10349). Slot 0 is the
    /// main task; every other slot owns a suspended frame/stack suffix.
    tasks: Vec<builtins_tasks::VmTask>,
    runnable_tasks: std::collections::VecDeque<usize>,
    sleeping_tasks: Vec<(std::time::Instant, usize)>,
    current_task_id: usize,
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
    /// Concrete definitions reserved by compilation but not yet visible at
    /// their source position. IDs remain stable because the queue must activate
    /// contiguously onto `struct_defs` (Issues #9784/#11546).
    pending_eval_struct_defs: VecDeque<(usize, StructDefInfo)>,
    pending_eval_abstract_types: VecDeque<(usize, AbstractTypeDefInfo)>,
    pending_eval_primitive_types: VecDeque<(usize, PrimitiveTypeDefInfo)>,
    enum_defs: Vec<EnumDefInfo>,
    pending_eval_enum_defs: VecDeque<(usize, EnumDefInfo)>,
    active_enum_name_index: HashMap<String, usize>,
    /// Enum constant stores expected immediately after the current
    /// `RegisterEnum`. This lets `PushEnum` distinguish declaration-time
    /// publication (which must reject an existing mutable global) from ordinary
    /// enum construction using the same opcode (Issue #11652).
    pending_eval_enum_member_bindings: VecDeque<(String, String, i64)>,
    /// Source declarations that cannot be split from a fresh program's dense
    /// metadata because compiler-generated concrete instantiations follow them.
    /// Only their Julia bindings are hidden until the marker executes.
    hidden_eval_struct_type_ids: HashSet<usize>,
    hidden_eval_abstract_type_ids: HashSet<usize>,
    hidden_eval_primitive_type_ids: HashSet<usize>,
    hidden_eval_enum_type_ids: HashSet<usize>,
    /// Main-owned nominal bindings whose source-order publication marker has
    /// executed. Compiler registries also contain bare aliases for module-owned
    /// types, so their mere presence cannot prove that an unqualified global
    /// binding belongs to Main (Issue #11655).
    published_eval_nominal_type_names: HashSet<String>,
    /// Typed source-order transaction log for the current appended main.
    repl_definition_activations: Vec<ReplDefinitionActivation>,
    /// Distinct source-ordered `(owner module, local usings index)` identities
    /// whose runtime marker executed in the current appended main (#11748).
    repl_using_activations: Vec<(String, usize)>,
    /// Qualified source module paths whose binding was published and whose body
    /// began executing in the current appended main (#11761).
    repl_module_activations: Vec<String>,
    /// Executed `DefineFunction` markers for module/nested functions. These are
    /// already present in the compiled table, so they do not belong in the
    /// definition-world activation log above (Issue #11721).
    repl_runtime_function_indices: Vec<usize>,
    /// Exact frame-0 bindings written while executing the current appended
    /// main. Error recovery uses this execution trace rather than the input's
    /// syntactic assignment set, which also contains unreachable suffixes.
    repl_written_globals: HashSet<String>,
    /// Executed stores whose instruction semantics target the module-global
    /// binding table regardless of the active call-frame depth (Issue #9784).
    repl_explicit_global_writes: HashSet<String>,
    /// Refresh callers keyed by the source method marker that publishes them.
    /// Each group receives one shared world stamp (Issue #9784).
    repl_function_refresh_groups: HashMap<usize, Vec<usize>>,
    /// Specializable-table row replacements keyed by the appended fallback
    /// function whose source marker makes them visible.
    repl_specializable_updates: HashMap<usize, Vec<(usize, SpecializableFunction)>>,
    /// Specializable rows whose source-order transaction is still executing.
    /// Their fallback bytecode is world-aware; the runtime specializer is not.
    repl_world_sensitive_specializable_indices: HashSet<usize>,
    abstract_types: Vec<AbstractTypeDefInfo>, // User-defined abstract types
    show_methods: std::collections::HashMap<String, usize>, // fallback: type_name -> func_index
    print_methods: std::collections::HashMap<String, usize>, // fallback: type_name -> func_index
    show_method_candidates: std::collections::HashMap<String, Vec<usize>>,
    print_method_candidates: std::collections::HashMap<String, Vec<usize>>,
    struct_heap: Vec<StructInstance>, // Heap for mutable struct instances
    weak_refs: Vec<std::rc::Weak<std::cell::RefCell<Value>>>,
    finalizers: Vec<FinalizerEntry>,
    pending_finalizers: Vec<(Value, Value)>,
    in_finalizer: bool,
    rng: R,
    output: String, // Buffer for println output
    /// Buffer for stderr output (Issue #3573).
    /// Forwarded to actual stderr by the runner / FFI consumer on exit.
    stderr_output: String,
    stdin_stream: value::IORef,
    current_stdout: value::IORef,
    current_stderr: value::IORef,
    devnull_stream: value::IORef,
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
    redirect_states: Vec<RedirectState>,
    pending_error: Option<VmError>,
    /// The pending exception value for catch blocks (preserves struct instances)
    pending_exception_value: Option<Value>,
    /// Backtrace captured when the pending exception entered VM error handling.
    pending_backtrace: Option<Vec<VmStackFrame>>,
    /// Stack of exceptions currently active in catch blocks for `rethrow()`.
    caught_exceptions: Vec<(VmError, Option<Value>, Vec<VmStackFrame>)>,
    /// Stack of exceptions unwinding through a currently-executing
    /// finally-only handler, one entry per still-open finally instance
    /// (Issue #11306). Pushed by `handle_error` when it routes into a
    /// `finally` with no `catch` of its own; the compiler-emitted trailing
    /// `Instr::Rethrow` at the end of that finally's body pops its own entry
    /// to resume propagating the original exception. A stack (not a scalar)
    /// is required because nested/sibling `try`/`catch`/`finally` activity
    /// *inside* the finally body — including an explicit `rethrow()` caught
    /// by its own nested `catch` — must not clobber an enclosing finally's
    /// still-pending marker; `Handler::finally_pending_len` truncates this
    /// stack back to the right depth whenever a handler is popped.
    pending_finally_rethrows: Vec<(VmError, Option<Value>, Option<Vec<VmStackFrame>>)>,
    // Test state for @test and @testset macros
    test_pass_count: usize,
    test_fail_count: usize,
    test_broken_count: usize,
    // Errored tests: the `@test` expression threw an exception or evaluated to
    // a non-Boolean value — upstream `Test.Error`, a distinct outcome from a
    // recorded failure (Issue #10093).
    test_error_count: usize,
    current_testset: Option<String>,
    // Enclosing-testset frames (Issue #10338). `_testset_begin!` pushes the
    // counts the ENCLOSING scope accumulated so far (plus its testset name)
    // and resets the scalar counters for the new set; `_testset_end!` prints
    // the finished set's own counts, pops, and folds them back into the
    // restored enclosing counts — upstream `DefaultTestSet`'s parent
    // aggregation (`record(parent, child)`) collapsed to count frames, so an
    // outer `@testset` summary aggregates its nested testsets instead of
    // echoing the last inner set's counters.
    testset_stack: Vec<TestSetFrame>,
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
    // Cached name -> callee lookup for cross-function runtime specialization
    // (Issue #10749), keyed by the `(functions.len(), specializable_functions.len())`
    // snapshot it was built from so it can be cheaply reused across the many
    // `CallSpecialize` sites in one program run. See
    // `exec::call::specializable_callable_registry`.
    specializable_callable_registry_cache: Option<(usize, usize, Rc<specialize::CallableRegistry>)>,
    specialization_cache: HashMap<SpecializationKey, SpecializedCode>,
    // Negative specialization cache (Issue #8603): signatures whose
    // `specialize_function` attempt already failed. Without it every call with
    // an unspecializable signature (e.g. BigFloat operands) re-ran
    // `collect_type_object_names` + the full specializer (~75 µs/call) just to
    // fail again. A hit means "skip specialization, run the interpreter
    // fallback" — semantically identical to the failed attempt, so a stale
    // entry can only cost speed, never correctness. Cleared together with the
    // dispatch caches on method-table mutations (a new method/struct
    // definition can turn a failure into a success) and by
    // `clear_runtime_caches`.
    specialization_failure_cache: HashSet<SpecializationKey>,
    // Cheap monomorphic fast cache for the all-`I64` specialize-call hot path,
    // keyed by `(spec_func_index, arity)` so the dispatch avoids per-call `Vec`
    // allocation and `Vec`-keyed hashing (Issue #8167). Populated lazily from
    // `specialization_cache` on the first all-`I64` call to an eligible callee.
    specialization_i64_cache: HashMap<(usize, usize), I64SpecDispatch>,
    // Even-cheaper per-VM cache indexed directly by `spec_func_index` with the
    // arity stored inline. This avoids the HashMap lookup on the hot path of
    // `CallSpecializeI64Slots` for small-arity I64 calls. The optional
    // predecoded I64 block is cached here too, avoiding a second HashMap lookup
    // in `i64_function_cache` on every call.
    specialization_i64_fast_cache: Vec<Option<I64SpecFastCacheEntry>>,
    // Float64 mirrors of the two caches above (Issue #10491): the all-`F64`
    // specialize-call hot path for `CallSpecializeF64Slots`, keyed the same
    // way and enforced/cleared together with the I64 caches.
    specialization_f64_cache: HashMap<(usize, usize), I64SpecDispatch>,
    specialization_f64_fast_cache: Vec<Option<F64SpecFastCacheEntry>>,
    // Narrow mixed-arg mirror (Issue #10567 round 2): dispatch metadata for a
    // specialized callee whose argument types are NOT uniformly I64 or F64
    // (so neither cache above populates). Unlike the I64/F64 caches, keyed by
    // the FULL argument-type vector, not just arity: a heterogeneous-type
    // callee can have several DIFFERENT concrete instantiations sharing one
    // arity (e.g. a generic `+(x::Real, z::Complex)` method specialized once
    // for `(Int64, Complex{Float64})` and again for `(Int64, Complex{Int64})`
    // — both arity 2, "not uniformly I64/F64", but genuinely different
    // specialized bodies). The I64/F64 caches above are safe to key by arity
    // alone only because "all I64" / "all F64" already pins the concrete
    // types exactly; that invariant does not hold here, so an arity-only key
    // would let one arg-type combination's dispatch silently overwrite
    // another's and later resolve to the WRONG specialized body for a
    // same-arity mixed call elsewhere in the program (found via
    // `complex_int_literal_arith_9169` regressing when this cache was
    // arity-keyed — a shared generic method reached with both
    // `Complex{Float64}` and `Complex{Int64}` operands). No "fast cache" Vec
    // twin — unlike the I64/F64 hot paths this is resolved once per
    // typed-loop block ENTRY (see `Vm::resolve_specialize_complex_i64_callee`),
    // not once per call, so a `Vec`-keyed `HashMap` lookup there is not hot.
    specialization_mixed_cache: HashMap<(usize, Vec<ValueType>), I64SpecDispatch>,
    i64_function_cache: HashMap<usize, Option<executable::I64FunctionBlock>>,
    // Frame-less predecoded F64 function blocks keyed by entry ip; `None` =
    // predecode already failed for this entry (negative cache). Mirrors
    // `i64_function_cache` for Float64-typed callees (Task 7).
    f64_function_cache: HashMap<usize, Option<executable::F64FunctionBlock>>,
    // Frame-less typed scalar function blocks keyed by entry ip (Issue #9693);
    // `None` = predecode already failed for this entry (negative cache).
    typed_function_cache: HashMap<usize, Option<executable::TypedScalarFunctionBlock>>,
    binary_method_cache: HashMap<BinaryDispatchKey, usize>,
    compile_context: Option<RuntimeCompileContext>,
    /// Macro bindings visible per module (`ModuleId -> {"@name", ...}`), so
    /// function-form `isdefined(::Module, Symbol("@name"))` can consult the
    /// macro binding table that macros are otherwise erased from at runtime
    /// (Issue #7948). Keyed by `ModuleId` since Issue #10988 Phase 2a
    /// (previously a bare module-path `String`); resolve a module-name string
    /// to its id via `module_registry` before indexing.
    macro_bindings: HashMap<ModuleId, std::collections::HashSet<String>>,
    /// Module-path <-> `ModuleId` interning table backing `macro_bindings`
    /// (Issue #10988 Phase 2a). Carried verbatim from `CompiledProgram` at
    /// `Vm` construction, so it already agrees with the ids `macro_bindings`
    /// was built with.
    module_registry: ModuleInternTable,
    global_slot_names: Vec<String>,
    global_slot_map: HashMap<String, usize>,
    // Macro system support
    gensym_counter: u64, // Counter for generating unique symbol names
    runtime_typevar_counter: u64,
    // Issue #10252: `UnionAll.var` projections also need identity preservation
    // within one wrapper chain (`Vector.var === Vector.body.parameters[1]`), but
    // must not leak across unrelated wrappers that reuse the same TypeVar names.
    // The `binder_depth` is depth from the final body, which stays stable when
    // reflection starts from an inner suffix of the same wrapper (Issue #10261).
    // The key is fully structural (Issue #10987): no rendered `String`/
    // `Option<String>` participates in equality or hashing. See
    // `TypeVarProjectionKey` for why the as-declared bounds must stay in the
    // key while the display name must not.
    runtime_typevar_projection_identities: HashMap<TypeVarProjectionKey, RuntimeTypeVarValue>,
    // Cached well-known struct type IDs (Issue #2940)
    cached_cartesian_index_type_id: Cell<Option<usize>>,
    cached_pair_type_id: Cell<Option<usize>>,
    cached_complex_type_id: Cell<Option<usize>>,
    cached_array_type_id: Cell<Option<usize>>,
    // Struct name -> index lookup (Issue #2938)
    struct_def_name_index: HashMap<String, usize>,
    // Abstract type name -> index lookup (Issue #2896)
    abstract_type_name_index: HashMap<String, usize>,
    // L2 method dispatch cache: call_site_ip → (interned arg-id sequence →
    // func_index) (Issues #2943, #3355; keyed on `ConcreteTypeId`s in #9197 S3).
    // The inner key is the exact `CallSiteArgIds` of the call's argument
    // dispatch identities — the same interned id sequence the L1 inline cache
    // uses — so a hit is deterministic exact-sequence equality rather than the
    // pre-S3 unverified type-name hash. `usize::MAX` is the negative/builtin
    // fallback sentinel. Runtime-only (never serialized).
    dispatch_cache: HashMap<usize, HashMap<CallSiteArgIds, usize>>,
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
    // Issue #9197 slice 2: session-scoped interned concrete-type ids backing the
    // exact-match L1 call-site cache key. Single-threaded VM state (the `Struct`
    // keys hold `Rc<str>`); append-only within a session, invalidation rides
    // `dispatch_generation` on the caches, never this table.
    type_intern: TypeInternTable,
    // Pre-interned scalar value / array-element ids so the hot L1 id-derivation
    // path maps a primitive to its `ConcreteTypeId` by array index instead of a
    // per-call allocation + intern-table probe (Issue #9197 slice 2).
    call_site_type_id_tables: CallSitePrimitiveTables,
    // Global dispatch generation for the call-site inline caches (Issue #8561).
    // Bumped by the coarse `note_method_table_mutation` whole-clear and by
    // `clear_runtime_caches`; `CallSiteCache` entries filled in an older
    // generation are misses. The Issue #9197 S6 precise path
    // (`note_method_table_mutation_for`) deliberately does NOT bump it — it
    // vacates only the affected L1 ways so unrelated slots stay valid. See
    // `CallSiteCache` for the #8555 candidate-refresh ordering.
    dispatch_generation: u64,
    // Per-VM hard-cap thresholds for the runtime caches (Issue #8625). Seeded
    // at construction from the process-wide override or the built-in default,
    // so a long-running host (or the FFI REPL session that rebuilds a VM per
    // eval) can tune the ceiling to device memory. The `enforce_*_cache_limit`
    // methods read these instead of the bare constants.
    dispatch_cache_entry_limit: usize,
    specialization_cache_entry_limit: usize,
    // Issue #8625 observability for the #8610 hard-cap bound: how many times a
    // cache was cleared for exceeding its cap, and how many entries that
    // discarded in total. Surfaced through `memory_stats()`.
    cache_clear_count: u64,
    cache_cleared_entry_count: u64,
    /// Optional per-VM allocation budget in bytes (Issue #8702). `None` preserves
    /// default CLI/test behavior; hosts can opt in through
    /// `set_default_memory_budget_bytes`, `Vm::set_memory_budget_bytes`, or
    /// `SJULIA_MEMORY_BUDGET_BYTES`.
    memory_budget_bytes: Option<usize>,
    /// Whether the intermittent sampled waterline check is active (Issue #8703).
    /// Env-only budgets retain the #8702 single-allocation behavior; host/per-VM
    /// setters enable waterline containment.
    memory_waterline_enabled: bool,
    /// Instruction-boundary countdown for the intermittent memory-waterline
    /// check (Issue #8703). A value of 0 means the next safe point checks.
    memory_waterline_check_countdown: usize,
    // Benchmark/debug off-switch for the call-site inline caches, read from
    // `SJULIA_DISPATCH_INLINE_CACHE_OFF` / the process override at
    // construction (Issue #8561).
    call_site_inline_cache_disabled: bool,
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
    /// Frame-depth floors for active module-level `eval` calls (Issue #11071).
    /// Eval may read frames it creates at or above its own floor plus frame 0
    /// globals, but never the compiled caller's lexical frames below the floor.
    /// A stack keeps nested `eval` from inheriting an outer eval's local scope.
    module_eval_scope_floors: Vec<usize>,
    /// Lexical floor for temporary scopes created while tree-walking a
    /// generated expression or eval-defined method body (Issue #11075). Unlike
    /// module eval, the active function frame at the floor is a legitimate
    /// parent; caller frames below it are not.
    lexical_eval_scope_floors: Vec<usize>,
    // Function name → indices lookup for O(1) name-based dispatch (Issue #3361)
    function_name_index: HashMap<String, Vec<usize>>,
    /// Private name index for compiler-synthesized lowering helpers. Keeping it
    /// disjoint from `function_name_index` prevents a legal source definition
    /// with the same spelling from joining the helper's generic family (#9784).
    lowering_helper_name_index: HashMap<String, Vec<usize>>,
    // Bodies of methods defined by runtime `eval` of a quoted function
    // definition, keyed by `functions` index (Issue #8647). See
    // `EvalDefinedMethod` for why this exists alongside a fixed trampoline
    // `FunctionInfo` body instead of compiled bytecode.
    eval_defined_bodies: HashMap<usize, EvalDefinedMethod>,
    /// The one VM-local, keyed, one-shot carrier for exact runtime fields that
    /// do not fit in `VmError`. Every producer parks and constructs its error
    /// atomically; the exception funnel consumes the carrier before classifying
    /// the error, and recovery boundaries clear it (Issue #11647).
    pending_exception_payload: exec::exception_payload::PendingExceptionPayloadCarrier,
    /// Struct names activated by runtime eval/include. These constructors have
    /// no precompiled FunctionInfo, unlike ordinary structs such as `Val`.
    eval_defined_struct_names: HashSet<String>,
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
    // `SJULIA_REGISTER_VM=1` prototype gate (Issue #8558): `Some` routes
    // eligible direct calls through the side-by-side register VM; `None`
    // (the default) leaves the production stack VM untouched apart from one
    // `Option` check on the direct-call path.
    register_gate: Option<register_gate::RegisterGateState>,
    // `SJULIA_STACK_VM_METRICS=1` opt-in stack VM execution counters
    // (Issue #8559): `Some` records interpreter dispatches, executable-block
    // runs, and operand-stack/frame high-water marks; `None` (the default)
    // costs one null check per dispatched instruction.
    stack_metrics: Option<Box<stack_metrics::StackVmMetrics>>,
    // Multimedia display stack: host-controlled "graphical display" state
    // (Issue #9262). `graphical_display_active` is set true by graphical hosts
    // (iOS/web REPL, Editor, `sjulia --emit-artifact`) before `run()`; it stays
    // false for a plain CLI script / interactive terminal REPL, so `display(x)`
    // falls back to text output there, matching a headless Julia session whose
    // display stack holds only a `TextDisplay`. When active, the `_display_artifact`
    // builtin buffers each `display(x)`-rendered artifact here; the host reads the
    // sink after the run and routes it through the same single artifact channel as
    // the trailing-expression render (no C ABI change).
    graphical_display_active: bool,
    display_artifacts: Vec<crate::plotting::DisplayArtifact>,
    // `SJULIA_HANDLER_TABLE=1` handler-table dispatch experiment gate
    // (Issue #8562; only compiled under the `vm-handler-table` feature):
    // `Some` routes `dispatch_instr` through the function-pointer table and
    // counts hot-row hits vs fallback dispatches; `None` costs one `is_some`
    // check per dispatched instruction in feature builds. Default builds do
    // not contain the field or the check.
    #[cfg(feature = "vm-handler-table")]
    handler_table: Option<Box<exec::handler_table::HandlerTableState>>,
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
    /// keeping the worst-case native stack usage safely bounded even in
    /// unoptimized dev builds, where each eval re-entry carries much larger
    /// native frames; exceeding it surfaces as a `VmError::StackOverflow`
    /// (Julia's `StackOverflowError`), matching upstream's behaviour for
    /// runaway recursion.
    pub(crate) const MAX_EVAL_DISPATCH_DEPTH: usize = 4;

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

    /// Upper bound on the number of positional-argument vectors kept in
    /// `arg_vec_pool` (Issue #10103). Mirrors `MAX_POOLED_FRAMES`: a live
    /// direct-call chain only needs one scratch arg vector per in-flight call,
    /// so bounding retention keeps idle memory small while still covering the
    /// common recursion / tight-call-loop depths.
    pub(crate) const MAX_POOLED_ARG_VECS: usize = 256;

    /// Obtain an empty positional-argument vector, reusing a pooled one when
    /// available (Issue #10103). The returned vector is cleared but keeps its
    /// backing capacity, so the caller can `push` the callee's arguments without
    /// reallocating. Pair every `acquire_arg_vec` with a `release_arg_vec` once
    /// the arguments have been bound into the callee frame.
    #[inline]
    pub(crate) fn acquire_arg_vec(&mut self) -> Vec<Value> {
        match self.arg_vec_pool.pop() {
            Some(mut v) => {
                v.clear();
                v
            }
            None => Vec::new(),
        }
    }

    /// Return a spent positional-argument vector to `arg_vec_pool` for reuse
    /// (Issue #10103). The vector is emptied here (dropping any residual
    /// `Value`s, retaining capacity) before being pooled. Retention is capped at
    /// `MAX_POOLED_ARG_VECS`; beyond that the vector is simply dropped.
    #[inline]
    pub(crate) fn release_arg_vec(&mut self, mut args: Vec<Value>) {
        if self.arg_vec_pool.len() < Self::MAX_POOLED_ARG_VECS {
            args.clear();
            self.arg_vec_pool.push(args);
        }
    }

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

    /// Return a call frame that was acquired for validation but never pushed.
    pub(crate) fn release_unpushed_frame(&mut self, mut frame: Frame) {
        if self.frame_pool.len() < Self::MAX_POOLED_FRAMES {
            frame.clear_for_pool();
            self.frame_pool.push(frame);
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
        frame.pending_kw_default_type_checks.clear();
        if let Some(func_index) = frame.func_index {
            if let Some(func) = self.functions.get(func_index) {
                for kwparam in func.kwparams.iter().filter(|kwparam| {
                    !kwparam.required && !kwparam.is_varargs && kwparam.declared_type.is_some()
                }) {
                    let omitted_body_default = matches!(&kwparam.default, Value::Undef)
                        && matches!(
                            frame
                                .locals_slots
                                .get(kwparam.slot)
                                .and_then(Option::as_ref),
                            Some(Value::Undef)
                        );
                    frame
                        .pending_kw_default_type_checks
                        .insert(kwparam.slot, usize::from(omitted_body_default));
                }
            }
        }
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
        self.check_memory_waterline_safepoint()
    }

    pub(crate) fn pop_call_frame(&mut self) {
        let depth = self.frames.len().saturating_sub(1);
        self.generated_expr_pending_keys.remove(&depth);
        self.generated_expr_pending_eval_frames.remove(&depth);
        while self
            .redirect_states
            .last()
            .is_some_and(|state| state.call_frame_depth == depth)
        {
            if let Some(state) = self.redirect_states.pop() {
                self.restore_redirect_stream(state);
            }
        }
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
        let io = Value::IO(crate::vm::value::IOValue::buffer_ref());
        self.user_show_method_for_io(v, &io)
    }

    pub(crate) fn user_show_method_for_io(&self, v: &Value, io: &Value) -> Option<usize> {
        if let Value::Struct(s) = v {
            let full = &*s.struct_name;
            if let Some(func_index) = self.show_method_for_type_name(full, full, io) {
                return Some(func_index);
            }

            let mut current = full.to_string();
            let mut seen_families = vec![nominal_family_name(full).to_string()];
            for _ in 0..64 {
                let Some(parent) = self.projected_direct_parent_type_name(&current) else {
                    break;
                };
                let parent_family = nominal_family_name(&parent).to_string();
                if seen_families.iter().any(|seen| seen == &parent_family) {
                    break;
                }
                if let Some(func_index) = self.show_method_for_type_name(full, &parent, io) {
                    return Some(func_index);
                }
                seen_families.push(parent_family);
                current = parent;
            }
            None
        } else {
            None
        }
    }

    pub(crate) fn user_show_method_for_print(&self, v: &Value) -> Option<usize> {
        let io = Value::IO(crate::vm::value::IOValue::buffer_ref());
        self.user_show_method_for_print_io(v, &io)
    }

    pub(crate) fn user_show_method_for_print_io(&self, v: &Value, io: &Value) -> Option<usize> {
        self.user_print_method_for_io(v, io)
            .or_else(|| self.user_show_method_for_io(v, io))
    }

    pub(crate) fn user_print_method_for(&self, v: &Value) -> Option<usize> {
        let io = Value::IO(crate::vm::value::IOValue::buffer_ref());
        self.user_print_method_for_io(v, &io)
    }

    pub(crate) fn user_print_method_for_io(&self, v: &Value, io: &Value) -> Option<usize> {
        if let Value::Struct(s) = v {
            let full = &*s.struct_name;
            if let Some(func_index) = self.print_method_for_type_name(full, full, io) {
                return Some(func_index);
            }

            let mut current = full.to_string();
            let mut seen_families = vec![nominal_family_name(full).to_string()];
            for _ in 0..64 {
                let Some(parent) = self.projected_direct_parent_type_name(&current) else {
                    break;
                };
                let parent_family = nominal_family_name(&parent).to_string();
                if seen_families.iter().any(|seen| seen == &parent_family) {
                    break;
                }
                if let Some(func_index) = self.print_method_for_type_name(full, &parent, io) {
                    return Some(func_index);
                }
                seen_families.push(parent_family);
                current = parent;
            }
            None
        } else {
            None
        }
    }

    pub(crate) fn current_frame_is_generic_show_fallback(&self) -> bool {
        let Some(func_index) = self.frames.last().and_then(|frame| frame.func_index) else {
            return false;
        };
        self.is_generic_show_fallback_index(func_index)
    }

    pub(crate) fn dynamic_show_method_for_io_value(
        &self,
        io: &Value,
        value: &Value,
    ) -> Result<Option<usize>, VmError> {
        let show_func = Value::Function(FunctionValue::new("show"));
        let args = vec![io.clone(), value.clone()];
        let candidates = self.collect_runtime_callable_candidates(&show_func, "show")?;
        let arg_type_names = self.callable_dispatch_type_names(&args);
        self.dispatch_function_variable_for_values("show", &candidates, &arg_type_names, &args)
            .map(|idx| idx.filter(|idx| !self.is_generic_show_fallback_index(*idx)))
    }

    pub(crate) fn try_start_exact_print_display(
        &mut self,
        io: &Value,
        value: &Value,
    ) -> Result<bool, VmError> {
        let resolved =
            crate::vm::formatting::resolve_struct_refs_for_format(value, &self.struct_heap);
        if !matches!(resolved, Value::Struct(_)) {
            return Ok(false);
        }

        let display_func_index = if self.current_frame_is_generic_show_fallback() {
            self.user_show_method_for_io(&resolved, io)
        } else {
            self.user_show_method_for_print_io(&resolved, io)
        };
        let display_func_index = match display_func_index {
            Some(idx) => Some(idx),
            None => self.dynamic_show_method_for_io_value(io, &resolved)?,
        };
        if let Some(func_index) = display_func_index {
            self.start_function_call(func_index, vec![io.clone(), resolved])?;
            return Ok(true);
        }
        Ok(false)
    }

    fn is_generic_show_fallback_index(&self, func_index: usize) -> bool {
        let Some(func) = self.functions.get(func_index) else {
            return false;
        };
        let name = func.name.rsplit('.').next().unwrap_or(&func.name);
        name == "show"
            && func.params.len() == 2
            && matches!(
                func.param_julia_types.first(),
                Some(crate::types::JuliaType::IO | crate::types::JuliaType::IOBuffer)
            )
            && matches!(
                func.param_julia_types.get(1),
                Some(crate::types::JuliaType::Any)
            )
    }

    fn show_method_for_type_name(
        &self,
        value_type_name: &str,
        lookup_type_name: &str,
        io: &Value,
    ) -> Option<usize> {
        self.io_method_for_type_name(
            value_type_name,
            lookup_type_name,
            io,
            &self.show_method_candidates,
            &self.show_methods,
        )
    }

    fn print_method_for_type_name(
        &self,
        value_type_name: &str,
        lookup_type_name: &str,
        io: &Value,
    ) -> Option<usize> {
        self.io_method_for_type_name(
            value_type_name,
            lookup_type_name,
            io,
            &self.print_method_candidates,
            &self.print_methods,
        )
    }

    fn io_method_for_type_name(
        &self,
        value_type_name: &str,
        lookup_type_name: &str,
        io: &Value,
        method_candidates: &std::collections::HashMap<String, Vec<usize>>,
        fallback_methods: &std::collections::HashMap<String, usize>,
    ) -> Option<usize> {
        // A struct defined inside a module carries a module-qualified name
        // (e.g. "Primes.Factorization"), but a `Base.show(io, ::Factorization)`
        // method registers under the name as written in the signature - usually
        // the bare "Factorization". Try, in order: the full qualified name, the
        // qualified name without parametric braces, then the same two with the
        // module prefix stripped, so module-defined show methods are found
        // regardless of how the value's type name is qualified (Issue #7171/#7172).
        let base_full =
            &lookup_type_name[..lookup_type_name.find('{').unwrap_or(lookup_type_name.len())];
        let no_mod = match lookup_type_name.rfind('.') {
            Some(pos) => &lookup_type_name[pos + 1..],
            None => lookup_type_name,
        };
        let base_no_mod = &no_mod[..no_mod.find('{').unwrap_or(no_mod.len())];
        self.best_io_method_for_keys(
            &[lookup_type_name, base_full, no_mod, base_no_mod],
            value_type_name,
            io,
            method_candidates,
            fallback_methods,
        )
    }

    fn best_io_method_for_keys(
        &self,
        keys: &[&str],
        value_type_name: &str,
        io: &Value,
        method_candidates: &std::collections::HashMap<String, Vec<usize>>,
        fallback_methods: &std::collections::HashMap<String, usize>,
    ) -> Option<usize> {
        let mut candidates = Vec::new();
        for key in keys {
            if let Some(indices) = method_candidates.get(*key) {
                for &idx in indices {
                    if !candidates.contains(&idx) {
                        candidates.push(idx);
                    }
                }
            }
        }

        if !candidates.is_empty() {
            let dispatch_io =
                crate::vm::formatting::resolve_struct_refs_for_format(io, &self.struct_heap);
            let args = vec![
                dispatch_io,
                Value::Struct(StructInstance::with_name(
                    0,
                    value_type_name.to_string(),
                    Vec::new(),
                )),
            ];
            return self
                .find_best_method_index_from_candidates(&candidates, &args)
                .unwrap_or_default();
        }

        keys.iter()
            .find_map(|key| fallback_methods.get(*key).copied())
    }

    /// The direct parent type name for the show-method supertype walk in
    /// [`Self::user_show_method_for`] — **display path only** (print / string /
    /// repr / REPL echo of a struct value with no `show` of its own).
    ///
    /// Returns the parent type name **as declared** in the struct hierarchy.
    /// Issue #9197 slice 4 retired the previous concrete-parameter substitution
    /// here, which re-parsed the value's rendered type name at runtime with
    /// `parametric_type_args` and rebuilt a projected parent spelling like
    /// `AbstractFoo{Int64}`. That substitution was functionally dead: the walk
    /// keys every lookup on the *family* name — [`Self::show_method_for_type_name`]
    /// always falls back to the bare base, and `pipeline_ctx::register_show_type_name`
    /// registers that bare base for *every* parametric `show` signature — so the
    /// parent's concrete type parameters never changed which method was found.
    ///
    /// This is not a dispatch path. The runtime dispatcher's own supertype
    /// handling lives in `vm/dispatch.rs`; moving *that* off type-name strings to
    /// structural interned ids is Issue #9197 slice 5 (typemap first-arg indexing).
    fn projected_direct_parent_type_name(&self, type_name: &str) -> Option<String> {
        let entry = self.struct_hierarchy.entry(type_name)?;
        entry.parent().map(str::to_string)
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
    pub fn render_value_via_user_show(&mut self, value: &Value) -> Option<String> {
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
        self.render_value_via_io_method(func_index, resolved)
    }

    fn render_value_via_io_method(&mut self, func_index: usize, resolved: Value) -> Option<String> {
        let io = crate::vm::value::IOValue::buffer_ref();
        let target_depth = self.frames.len();
        // `start_sprint_call` pushes the `show(io, value)` frame and arranges for
        // the eventual return to extract the buffer as a `Value::Str`; the driver
        // then unwinds back to `target_depth` and hands us that string.
        // Preserve an outer `sprint(show, array)` while pre-rendering array
        // elements through nested sprint calls (Issue #8819).
        let outer_sprint_state = self.sprint_state.take();
        let rendered = if self
            .start_sprint_call(func_index, io, vec![resolved])
            .is_ok()
        {
            match self.run_until_frame_return(target_depth) {
                Ok(Value::Str(s)) => Some(s.to_string()),
                _ => None,
            }
        } else {
            None
        };
        self.sprint_state = outer_sprint_state;
        rendered
    }

    pub(crate) fn render_value_via_user_show_for_print(&mut self, value: &Value) -> Option<String> {
        let resolved =
            crate::vm::formatting::resolve_struct_refs_for_format(value, &self.struct_heap);
        if let Some(func_index) = self.user_print_method_for(&resolved) {
            return self.render_value_via_io_method(func_index, resolved);
        }
        self.render_value_via_user_show(&resolved)
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
                {
                    let els = crate::vm::formatting::array_wrapper_elements(s)?;
                    els.into_iter().take(100).collect()
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
