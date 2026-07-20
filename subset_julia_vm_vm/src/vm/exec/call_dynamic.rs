//! Dynamic dispatch call instructions.
//!
//! This module serves as the entry point for all dynamic dispatch operations.
//! The main dispatcher delegates to specialized submodules:
//!
//! - `call_dynamic_binary`: Binary operator dispatch (CallDynamicBinary, CallDynamicBinaryBoth, etc.)
//! - `call_dynamic_typed`: Typed dispatch (CallTypedDispatch, CallTypeConstructor)
//! - `call_function_variable`: Function variable calls (CallGlobalRef, CallFunctionVariable, etc.)
//!
//! ## Debug Logging
//!
//! Set `SJULIA_DISPATCH_DEBUG=1` to enable dispatch tracing for binary operations.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::super::*;
use super::call::bind_kwargs_defaults;
use super::util::{
    bind_value_to_slot, extract_base_type, is_rust_dict_parametric_mismatch,
    is_struct_dict_bare_mismatch, strip_module_prefix,
};
use super::DispatchAction;
use crate::builtins::BuiltinId;
use crate::inference_core::dispatch_resolver::{
    resolve_runtime_core_signature_candidates,
    resolve_runtime_core_signature_slice_candidates_with_family_fallback_or_tie, CalleeIdentity,
    RuntimeCoreCandidate, RuntimeCoreSliceCandidate,
};
use crate::inference_core::selection;
use crate::inference_core::CoreType;
use crate::rng::RngLike;
use crate::types::JuliaType;
use crate::vm::intrinsics_exec::apply_unary_rounding_op_named_with_heap;
use crate::vm::value::{
    array_wrapper_value_to_array_value, native_array_value_ref, GeneratorCallable, GeneratorValue,
    RustBigFloat, StructInstance,
};

fn matches_native_collect_iterator(struct_name: &str) -> bool {
    matches!(
        struct_name,
        "Zip" | "Zip3" | "Zip4" | "Zip5" | "Zip6" | "Zip7"
    ) || struct_name.starts_with("Zip{")
        || struct_name.starts_with("Zip3{")
        || struct_name.starts_with("Zip4{")
        || struct_name.starts_with("Zip5{")
        || struct_name.starts_with("Zip6{")
        || struct_name.starts_with("Zip7{")
}

fn matches_enumerate_iterator(struct_name: &str) -> bool {
    struct_name == "Enumerate" || struct_name.starts_with("Enumerate{")
}

fn matches_count_iterator(struct_name: &str) -> bool {
    struct_name == "Count" || struct_name.starts_with("Count{")
}

fn range_struct_field_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Char(c) => Some(f64::from(u32::from(*c))),
        Value::I8(v) => Some(f64::from(*v)),
        Value::I16(v) => Some(f64::from(*v)),
        Value::I32(v) => Some(f64::from(*v)),
        Value::I64(v) => Some(*v as f64),
        Value::U8(v) => Some(f64::from(*v)),
        Value::U16(v) => Some(f64::from(*v)),
        Value::U32(v) => Some(f64::from(*v)),
        Value::U64(v) => Some(*v as f64),
        _ => None,
    }
}

fn range_length_from_parts(start: f64, step: f64, stop: f64) -> Option<i64> {
    if step == 0.0 {
        return None;
    }
    if (step > 0.0 && start > stop) || (step < 0.0 && start < stop) {
        return Some(0);
    }
    Some(((stop - start) / step).floor() as i64 + 1)
}

fn range_struct_length(struct_name: &str, fields: &[Value]) -> Option<i64> {
    match strip_module_prefix(extract_base_type(struct_name)) {
        "UnitRange" => range_length_from_parts(
            range_struct_field_to_f64(fields.first()?)?,
            1.0,
            range_struct_field_to_f64(fields.get(1)?)?,
        ),
        "StepRange" => range_length_from_parts(
            range_struct_field_to_f64(fields.first()?)?,
            range_struct_field_to_f64(fields.get(1)?)?,
            range_struct_field_to_f64(fields.get(2)?)?,
        ),
        _ => None,
    }
}

fn uses_builtin_iterate_for_struct(struct_name: &str) -> bool {
    struct_name == "CartesianIndices" || struct_name == "Array" || struct_name.starts_with("Array{")
}

fn is_array_wrapper_value(value: &Value, struct_heap: &[StructInstance]) -> bool {
    match value {
        Value::Struct(s) => s.array_wrapper_julia_type().is_some(),
        Value::StructRef(idx) => struct_heap
            .get(*idx)
            .and_then(StructInstance::array_wrapper_julia_type)
            .is_some(),
        _ => false,
    }
}

#[cfg(test)]
mod native_array_candidate_tests {
    use super::native_array_view_wrapper_candidate_mismatch;
    use crate::vm::value::{native_array_value_from_array, ArrayValue};

    #[test]
    fn native_array_view_wrapper_dynamic_candidate_mismatch_issue_9778() {
        let array = native_array_value_from_array(ArrayValue::memory_first_from_f64(
            vec![1.0, 2.0],
            vec![2],
        ));

        assert!(native_array_view_wrapper_candidate_mismatch(
            &array,
            "SubArray{Float64}"
        ));
        assert!(native_array_view_wrapper_candidate_mismatch(
            &array,
            "Base.ReshapedArray{Float64,1,Vector{Float64},Tuple{}}"
        ));
        assert!(native_array_view_wrapper_candidate_mismatch(
            &array,
            "MatrixView{Float64}"
        ));
        assert!(!native_array_view_wrapper_candidate_mismatch(
            &array,
            "Vector{Float64}"
        ));
        assert!(!native_array_view_wrapper_candidate_mismatch(
            &array,
            "Array{Float64,1}"
        ));
    }
}

fn can_score_iterate_dynamic_candidates(value: &Value) -> bool {
    // Structs are always scored against the full candidate set so Base
    // iterators (Zip, SubArray, ...) and user iterators both win on merit.
    //
    // Native arrays have a dedicated VM iterator, so historically they bypassed
    // candidate scoring entirely. But that made `iterate(::Any)` unable to reach
    // a user-defined `iterate(::Vector{Int64})` method, unlike the `collect`
    // CallDynamic path (Issue #6638). Native arrays are now scored too, but only
    // against *user-defined* candidates (see `scored_iterate_candidates`): there
    // are no Base `iterate` methods over Array/Vector, so the VM builtin iterator
    // still runs whenever the user has not explicitly overridden it (Issue #5584).
    // Native-array carrier check goes through the shared destructure helper
    // rather than a direct carrier-variant match (Issue #6806).
    matches!(value, Value::Struct(_) | Value::StructRef(_)) || is_native_array_value(value)
}

/// The candidate index subset to score for an `IterateDynamic` collection.
///
/// Structs score against the full candidate set; native arrays score only
/// against user-defined candidates (`idx >= base_function_count`) so the
/// dedicated VM array iterator is overridden only by an explicit user
/// `iterate(::Vector{...})` method, never by loosely matching a Base struct
/// iterator (Issue #6638 / #5584).
fn scored_iterate_candidates(
    coll: &Value,
    candidates: &[usize],
    base_function_count: usize,
    struct_heap: &[StructInstance],
) -> Vec<usize> {
    if is_native_array_value(coll) || is_array_wrapper_value(coll, struct_heap) {
        candidates
            .iter()
            .copied()
            .filter(|idx| *idx >= base_function_count)
            .collect()
    } else {
        candidates.to_vec()
    }
}

fn is_native_range_candidate_mismatch(arg: &Value, expected_type: &str) -> bool {
    let Value::Range(range) = arg else {
        return false;
    };

    // Char ranges are always `StepRange{Char, Int64}` in upstream Julia, and
    // native float ranges are represented as `StepRangeLen{T,...}`. Route
    // runtime method dispatch to the same family that `typeof(::RangeValue)`
    // reports so dynamic calls do not fall into struct-field methods for the
    // wrong range family (Issues #4830/#9815).
    let is_char_range = matches!(range.element_type, crate::vm::value::RangeElementType::Char);
    let is_float_range = range.is_explicit_float_type();

    let base_name = strip_module_prefix(extract_base_type(expected_type));
    match base_name {
        "AbstractRange" => false,
        "AbstractUnitRange" | "UnitRange" => {
            is_char_range || is_float_range || !range.is_unit_range()
        }
        "StepRange" => is_float_range || (!is_char_range && range.is_unit_range()),
        "StepRangeLen" => !is_float_range,
        "LinRange" | "OneTo" | "LogRange" => true,
        _ => false,
    }
}

/// Same-family fallback for the CallDynamic / IterateDynamic structured
/// resolvers: a legacy wrapper sentinel (e.g. native-iterator `Generator`) and
/// the actual wrapper struct share a family when their bare nominal names match
/// (module prefix + parametric `{...}` stripped). This compares the structured
/// `core_signature` family name directly via [`CoreType::nominal_family_name`]
/// instead of rendering each type back to a Julia name string and re-parsing it
/// (Issue #6593). The `expected` side is always a bare `Struct`/`Named`
/// candidate (gated by `core_type_allows_family_fallback`), so a non-nominal
/// `actual` simply has no family to match.
fn runtime_core_nominal_name(ty: &CoreType) -> Option<&str> {
    match ty {
        CoreType::Struct { name, .. }
        | CoreType::Named(name)
        | CoreType::AbstractUser { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

fn runtime_core_family_fallback_matches(actual: &CoreType, expected: &CoreType) -> bool {
    match (
        runtime_core_nominal_name(actual),
        runtime_core_nominal_name(expected),
    ) {
        (Some(actual_name), Some(expected_name)) => {
            crate::types::nominal_family_names_compatible(actual_name, expected_name)
        }
        _ => false,
    }
}

fn native_array_view_wrapper_candidate_mismatch(arg: &Value, expected_type: &str) -> bool {
    if native_array_value_ref(arg).is_none() {
        return false;
    }
    matches!(
        strip_module_prefix(extract_base_type(expected_type)),
        "SubArray" | "ReshapedArray" | "MatrixView"
    )
}

fn tier_fallback_gate_key_for_dedup(
    slot_key: &CoreType,
    gate: Option<&CoreType>,
) -> Option<CoreType> {
    let gate_key = gate.map(CoreType::canonicalize_signature_for_dedup)?;
    match &gate_key {
        CoreType::Tuple(elements) if elements.as_slice() == std::slice::from_ref(slot_key) => None,
        _ => Some(gate_key),
    }
}

fn native_array_rank_count(iter: &Value) -> Option<(usize, usize, bool)> {
    let arr_ref = native_array_value_ref(iter)?;
    let arr = arr_ref.borrow();
    Some((arr.shape.len(), arr.element_count(), arr.shape.is_empty()))
}

fn array_wrapper_size_rank_count(size_value: &Value) -> Option<(usize, usize, bool)> {
    let dims = match size_value {
        Value::Tuple(size_tuple) => match size_tuple.elements.first() {
            Some(Value::Tuple(dims_tuple)) => dims_tuple.elements.as_slice(),
            _ => size_tuple.elements.as_slice(),
        },
        Value::I64(len) if *len >= 0 => {
            let count = usize::try_from(*len).ok()?;
            return Some((1, count, false));
        }
        _ => return None,
    };

    let mut count = 1usize;
    for dim in dims {
        let Value::I64(dim) = dim else {
            return None;
        };
        if *dim < 0 {
            return None;
        }
        let dim = usize::try_from(*dim).ok()?;
        count = count.checked_mul(dim)?;
    }
    Some((dims.len(), count, dims.is_empty()))
}

fn array_wrapper_rank_count(
    iter: &Value,
    struct_heap: &[StructInstance],
) -> Option<(usize, usize, bool)> {
    let instance = match iter {
        Value::Struct(instance) => instance,
        Value::StructRef(idx) => struct_heap.get(*idx)?,
        _ => return None,
    };
    instance.array_wrapper_julia_type()?;
    array_wrapper_size_rank_count(instance.values.get(1)?)
}

fn iterable_array_rank_count(
    iter: &Value,
    struct_heap: &[StructInstance],
) -> Option<(usize, usize, bool)> {
    native_array_rank_count(iter).or_else(|| array_wrapper_rank_count(iter, struct_heap))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NativeIteratorSizeKind {
    HasLength,
    HasShape(usize),
    SizeUnknown,
    IsInfinite,
}

fn native_zip_size_kind(
    a: NativeIteratorSizeKind,
    b: NativeIteratorSizeKind,
) -> NativeIteratorSizeKind {
    use NativeIteratorSizeKind::{HasLength, HasShape, IsInfinite, SizeUnknown};

    match (a, b) {
        (IsInfinite, IsInfinite) => IsInfinite,
        (HasLength | HasShape(_), IsInfinite) => HasLength,
        (IsInfinite, other) => native_zip_size_kind(other, IsInfinite),
        (left, right) if left == right => left,
        (HasLength, HasShape(_)) | (HasShape(_), HasLength) => HasLength,
        _ => SizeUnknown,
    }
}

fn generator_iter_known_nonempty(iter: &Value, struct_heap: &[StructInstance]) -> bool {
    if let Some((_rank, count, shape_is_empty)) = iterable_array_rank_count(iter, struct_heap) {
        return !shape_is_empty && count > 0;
    }

    match iter {
        Value::Memory(mem) => !mem.borrow().is_empty(),
        Value::Range(r) => {
            if r.step == 0.0 {
                false
            } else if r.step > 0.0 {
                r.stop >= r.start
            } else {
                r.start >= r.stop
            }
        }
        _ => false,
    }
}

// Runtime parse of a `Base.Generator{...}` type-name string to recover the
// generator's element type for iterator-size inference. Operates on the opaque
// `JuliaType::Struct(name)` spelling; retiring it to structured ids waits on a
// structured `JuliaType` parameter representation. Not on the sealed-primitive
// first-arg path S5 landed (`FirstArgIndex`, method_table.rs) — deferred to
// Issue #9197 S6/S7 (see TYPE_INTERNING.md "Slice 5 deliverable").
fn top_level_generic_args(type_name: &str, prefix: &str) -> Option<Vec<String>> {
    let inner = type_name
        .strip_prefix(prefix)?
        .strip_prefix('{')?
        .strip_suffix('}')?;
    let mut args = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '{' => depth = depth.saturating_add(1),
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                args.push(inner[start..idx].trim().to_string());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    args.push(inner[start..].trim().to_string());
    Some(args)
}

pub(super) fn generator_iter_type_name(julia_type: &JuliaType) -> Option<String> {
    let JuliaType::Struct(name) = julia_type else {
        return None;
    };
    top_level_generic_args(name, "Base.Generator")
        .or_else(|| top_level_generic_args(name, "Generator"))
        .and_then(|args| args.into_iter().next())
}

/// Check if dispatch debug logging is enabled via `SJULIA_DISPATCH_DEBUG` env var.
/// Only available in debug builds to avoid performance impact in release.
#[cfg(debug_assertions)]
pub(super) fn dispatch_debug_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("SJULIA_DISPATCH_DEBUG").is_ok())
}

/// Emit dispatch debug logs in debug builds without relying on `eprintln!`.
#[cfg(debug_assertions)]
pub(super) fn dispatch_debug_log(args: std::fmt::Arguments<'_>) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{args}");
}

impl<R: RngLike> Vm<R> {
    /// Expected first-parameter type name for a `CallDynamic` method
    /// candidate, derived from the candidate's `FunctionInfo` (Issue #6496).
    /// Candidates are emitted only for single-parameter methods, and the
    /// Issue #6496 parity gates pin this rendering against the historical
    /// compile-time baked string.
    fn dynamic_candidate_expected_type_name(&self, func_index: usize) -> String {
        self.functions
            .get(func_index)
            .and_then(|func| func.param_julia_types.first())
            .map(ToString::to_string)
            .unwrap_or_else(|| "Any".to_string())
    }

    /// Structured counterpart of
    /// [`Self::dynamic_candidate_expected_type_name`] (Issue #6502 slice 2):
    /// the rendered first-parameter name (kept for the VM representation
    /// fences) plus the per-slot `core_signature` projection and, for
    /// `where`-parametric methods, the full signature gate (Issue #6536).
    fn dynamic_candidate_expected_signature(
        &self,
        func_index: usize,
    ) -> (String, CoreType, Option<CoreType>) {
        let Some(jt) = self
            .functions
            .get(func_index)
            .and_then(|func| func.param_julia_types.first())
        else {
            return ("Any".to_string(), CoreType::Any, None);
        };
        let type_params = self
            .functions
            .get(func_index)
            .map(|func| func.type_params.as_slice())
            .unwrap_or(&[]);
        let signature = crate::vm::dispatch_binding::build_runtime_candidate_core_signature(
            std::slice::from_ref(jt),
            type_params,
        );
        let rendered = signature
            .rendered
            .into_iter()
            .next()
            .unwrap_or_else(|| "Any".to_string());
        let slot = signature.slots.into_iter().next().unwrap_or(CoreType::Any);
        (rendered, slot, signature.signature)
    }

    /// Project a structured [`DynamicCallCandidate`] onto the runtime
    /// `core_signature` fallback shape; native-iterator sentinels keep their
    /// `usize::MAX` index and carry their legacy family name as a `CoreType`.
    fn resolve_dynamic_call_candidate_signature(
        &self,
        candidate: DynamicCallCandidate,
    ) -> (usize, String, CoreType, Option<CoreType>) {
        match candidate {
            DynamicCallCandidate::Method(idx) => {
                let (rendered, slot, gate) = self.dynamic_candidate_expected_signature(idx);
                (idx, rendered, slot, gate)
            }
            DynamicCallCandidate::NativeIterator(kind) => {
                let rendered = kind.type_name().to_string();
                let slot = CoreType::from_julia_name(&rendered);
                (usize::MAX, rendered, slot, None)
            }
        }
    }

    fn has_user_collect_candidate(&self, candidates: &[DynamicCallCandidate]) -> bool {
        candidates.iter().any(|candidate| {
            matches!(candidate, DynamicCallCandidate::Method(idx) if *idx >= self.base_function_count)
        })
    }

    fn has_applicable_native_range_collect_candidate(
        &self,
        arg: &Value,
        candidates: &[DynamicCallCandidate],
    ) -> bool {
        if !matches!(arg, Value::Range(_)) {
            return false;
        }
        candidates.iter().any(|candidate| {
            let DynamicCallCandidate::Method(idx) = candidate else {
                return false;
            };
            let name = self.dynamic_candidate_expected_type_name(*idx);
            let base_name = strip_module_prefix(extract_base_type(&name));
            matches!(base_name, "UnitRange" | "StepRange")
                && !is_native_range_candidate_mismatch(arg, &name)
        })
    }

    fn has_generator_collect_candidate(&self, candidates: &[DynamicCallCandidate]) -> bool {
        candidates.iter().any(|candidate| {
            let DynamicCallCandidate::Method(idx) = candidate else {
                return false;
            };
            let name = self.dynamic_candidate_expected_type_name(*idx);
            strip_module_prefix(extract_base_type(&name)) == "Generator"
        })
    }

    fn generator_can_use_generic_collect(
        &self,
        generator: &GeneratorValue,
    ) -> Result<bool, VmError> {
        let GeneratorCallable::FunctionIndex(func_index) = &generator.callable else {
            return Ok(false);
        };
        let func = self.get_function_checked(*func_index)?;
        let first = func.name.chars().next();
        Ok(first.is_some_and(|ch| ch.is_lowercase())
            && generator_iter_known_nonempty(generator.iter.as_ref(), &self.struct_heap))
    }

    pub(super) fn zero_field_struct_value(&mut self, struct_name: &str) -> Result<Value, VmError> {
        let base_name = strip_module_prefix(extract_base_type(struct_name));
        let type_id = self
            .struct_defs
            .iter()
            .position(|def| {
                def.name == struct_name
                    || strip_module_prefix(extract_base_type(&def.name)) == base_name
            })
            .ok_or_else(|| VmError::TypeError(format!("{base_name} type is not loaded")))?;
        let idx = self.struct_heap.len();
        self.struct_heap.push(StructInstance::with_name(
            type_id,
            struct_name.to_string(),
            Vec::new(),
        ));
        Ok(Value::StructRef(idx))
    }

    pub(super) fn iterator_size_value_for_native_generator_iter(
        &mut self,
        iter: &Value,
    ) -> Result<Value, VmError> {
        match self.native_iterator_size_kind_for_generator_iter(iter)? {
            NativeIteratorSizeKind::HasShape(rank) if (1..=8).contains(&rank) => {
                self.zero_field_struct_value(&format!("HasShape{{{rank}}}"))
            }
            NativeIteratorSizeKind::HasShape(_) | NativeIteratorSizeKind::HasLength => {
                self.zero_field_struct_value("HasLength")
            }
            NativeIteratorSizeKind::SizeUnknown => self.zero_field_struct_value("SizeUnknown"),
            NativeIteratorSizeKind::IsInfinite => self.zero_field_struct_value("IsInfinite"),
        }
    }

    pub(in crate::vm) fn native_iterator_length_for_generator_iter(
        &self,
        iter: &Value,
    ) -> Result<Option<i64>, VmError> {
        if let Some((_rank, count, _shape_is_empty)) =
            iterable_array_rank_count(iter, &self.struct_heap)
        {
            return i64::try_from(count)
                .map(Some)
                .map_err(|_| VmError::TypeError("iterator length exceeds Int64".to_string()));
        }
        if let Some(arr) = array_wrapper_value_to_array_value(iter, &self.struct_heap)? {
            return i64::try_from(arr.element_count())
                .map(Some)
                .map_err(|_| VmError::TypeError("iterator length exceeds Int64".to_string()));
        }

        match iter {
            Value::Memory(mem) => i64::try_from(mem.borrow().len())
                .map(Some)
                .map_err(|_| VmError::TypeError("iterator length exceeds Int64".to_string())),
            Value::Range(r) => Ok(Some(r.length())),
            Value::Tuple(t) => i64::try_from(t.len())
                .map(Some)
                .map_err(|_| VmError::TypeError("iterator length exceeds Int64".to_string())),
            Value::SimpleVector(items) => i64::try_from(items.len())
                .map(Some)
                .map_err(|_| VmError::TypeError("iterator length exceeds Int64".to_string())),
            Value::NamedTuple(nt) => i64::try_from(nt.values.len())
                .map(Some)
                .map_err(|_| VmError::TypeError("iterator length exceeds Int64".to_string())),
            Value::Str(s) => i64::try_from(s.chars().count())
                .map(Some)
                .map_err(|_| VmError::TypeError("iterator length exceeds Int64".to_string())),
            Value::StrBytes(bytes) => {
                i64::try_from(crate::vm::value::julia_char_count(bytes.as_ref()))
                    .map(Some)
                    .map_err(|_| VmError::TypeError("iterator length exceeds Int64".to_string()))
            }
            Value::Pairs(p) => i64::try_from(p.data.values.len())
                .map(Some)
                .map_err(|_| VmError::TypeError("iterator length exceeds Int64".to_string())),
            Value::StaticArray(sv) => i64::try_from(sv.len())
                .map(Some)
                .map_err(|_| VmError::TypeError("iterator length exceeds Int64".to_string())),
            Value::StaticArrayInline(sv) => i64::try_from(sv.len())
                .map(Some)
                .map_err(|_| VmError::TypeError("iterator length exceeds Int64".to_string())),
            Value::Generator(g) if self.generator_is_filtered(g) => Ok(None),
            Value::Generator(g) => self.native_iterator_length_for_generator_iter(g.iter.as_ref()),
            Value::Struct(s) => self.native_iterator_struct_length(&s.struct_name, &s.values),
            Value::StructRef(idx) => {
                let Some(s) = self.struct_heap.get(*idx) else {
                    return Ok(None);
                };
                self.native_iterator_struct_length(&s.struct_name, &s.values)
            }
            _ => Ok(None),
        }
    }

    fn native_iterator_size_kind_for_generator_iter(
        &self,
        iter: &Value,
    ) -> Result<NativeIteratorSizeKind, VmError> {
        if let Some((rank, _count, _shape_is_empty)) =
            iterable_array_rank_count(iter, &self.struct_heap)
        {
            return Ok(NativeIteratorSizeKind::HasShape(rank));
        }
        if let Some(arr) = array_wrapper_value_to_array_value(iter, &self.struct_heap)? {
            return Ok(NativeIteratorSizeKind::HasShape(arr.shape.len()));
        }

        match iter {
            Value::Memory(_) | Value::Range(_) => Ok(NativeIteratorSizeKind::HasShape(1)),
            Value::Tuple(_) | Value::Str(_) | Value::StrBytes(_) => {
                Ok(NativeIteratorSizeKind::HasLength)
            }
            Value::Generator(g) if self.generator_is_filtered(g) => {
                Ok(NativeIteratorSizeKind::SizeUnknown)
            }
            Value::Generator(g) => {
                self.native_iterator_size_kind_for_generator_iter(g.iter.as_ref())
            }
            Value::Struct(s) => self.native_iterator_struct_size_kind(&s.struct_name, &s.values),
            Value::StructRef(idx) => {
                let Some(s) = self.struct_heap.get(*idx) else {
                    return Ok(NativeIteratorSizeKind::HasLength);
                };
                self.native_iterator_struct_size_kind(&s.struct_name, &s.values)
            }
            _ => Ok(NativeIteratorSizeKind::HasLength),
        }
    }

    fn native_iterator_struct_length(
        &self,
        struct_name: &str,
        fields: &[Value],
    ) -> Result<Option<i64>, VmError> {
        if let Some(len) = range_struct_length(struct_name, fields) {
            return Ok(Some(len));
        }
        if matches_enumerate_iterator(struct_name) {
            let Some(iter) = fields.first() else {
                return Ok(None);
            };
            return self.native_iterator_length_for_generator_iter(iter);
        }
        if matches_native_collect_iterator(struct_name) {
            let mut min_len: Option<i64> = None;
            for field in fields {
                match self.native_iterator_length_for_generator_iter(field)? {
                    Some(len) => {
                        min_len = Some(min_len.map_or(len, |current| current.min(len)));
                    }
                    None if matches!(
                        self.native_iterator_size_kind_for_generator_iter(field)?,
                        NativeIteratorSizeKind::IsInfinite
                    ) => {}
                    None => return Ok(None),
                }
            }
            return Ok(min_len);
        }
        if matches_count_iterator(struct_name) {
            return Ok(None);
        }
        Ok(None)
    }

    fn native_iterator_struct_size_kind(
        &self,
        struct_name: &str,
        fields: &[Value],
    ) -> Result<NativeIteratorSizeKind, VmError> {
        if range_struct_length(struct_name, fields).is_some() {
            return Ok(NativeIteratorSizeKind::HasShape(1));
        }
        if matches_enumerate_iterator(struct_name) {
            let Some(iter) = fields.first() else {
                return Ok(NativeIteratorSizeKind::HasLength);
            };
            return self.native_iterator_size_kind_for_generator_iter(iter);
        }
        if matches_native_collect_iterator(struct_name) {
            let mut fields = fields.iter().rev();
            let Some(last) = fields.next() else {
                return Ok(NativeIteratorSizeKind::HasLength);
            };
            let mut result = self.native_iterator_size_kind_for_generator_iter(last)?;
            for field in fields {
                result = native_zip_size_kind(
                    self.native_iterator_size_kind_for_generator_iter(field)?,
                    result,
                );
            }
            return Ok(result);
        }
        if matches_count_iterator(struct_name) {
            return Ok(NativeIteratorSizeKind::IsInfinite);
        }
        Ok(NativeIteratorSizeKind::HasLength)
    }

    pub(super) fn pure_generator_iter_value(&self, value: &Value) -> Option<Value> {
        let generator = match value {
            Value::Struct(s) => Some(s),
            Value::StructRef(idx) => self.struct_heap.get(*idx),
            _ => None,
        }?;
        if strip_module_prefix(extract_base_type(&generator.struct_name)) == "Generator" {
            generator.values.get(1).cloned()
        } else {
            None
        }
    }

    pub(super) fn iterator_size_value_for_generator_iter_type_name(
        &mut self,
        iter_type: &str,
    ) -> Result<Value, VmError> {
        let size_kind = self.native_iterator_size_kind_for_generator_iter_type_name(iter_type)?;
        match size_kind {
            NativeIteratorSizeKind::HasShape(rank) if (1..=8).contains(&rank) => {
                self.zero_field_struct_value(&format!("HasShape{{{rank}}}"))
            }
            NativeIteratorSizeKind::HasShape(_) | NativeIteratorSizeKind::HasLength => {
                self.zero_field_struct_value("HasLength")
            }
            NativeIteratorSizeKind::SizeUnknown => self.zero_field_struct_value("SizeUnknown"),
            NativeIteratorSizeKind::IsInfinite => self.zero_field_struct_value("IsInfinite"),
        }
    }

    fn native_iterator_size_kind_for_generator_iter_type_name(
        &mut self,
        iter_type: &str,
    ) -> Result<NativeIteratorSizeKind, VmError> {
        let base_name = strip_module_prefix(extract_base_type(iter_type));
        match base_name {
            "Vector" | "UnitRange" | "StepRange" | "Memory" => {
                Ok(NativeIteratorSizeKind::HasShape(1))
            }
            "Matrix" => Ok(NativeIteratorSizeKind::HasShape(2)),
            "Array" => {
                let rank = top_level_generic_args(iter_type, "Array")
                    .and_then(|args| args.get(1).and_then(|rank| rank.parse::<usize>().ok()))
                    .or_else(|| {
                        top_level_generic_args(iter_type, "Base.Array").and_then(|args| {
                            args.get(1).and_then(|rank| rank.parse::<usize>().ok())
                        })
                    });
                if let Some(rank) = rank {
                    Ok(NativeIteratorSizeKind::HasShape(rank))
                } else {
                    Ok(NativeIteratorSizeKind::HasLength)
                }
            }
            "Tuple" | "String" => Ok(NativeIteratorSizeKind::HasLength),
            // A filtered generator's `I` param is `Iterators.Filter`, whose
            // `IteratorSize(::Type{<:Filter}) == SizeUnknown()` (Issue #9200 S3 /
            // #9379). Recognized here so `IteratorSize(typeof(filtered_gen))`
            // reports `SizeUnknown()` at the type level.
            "Filter" => Ok(NativeIteratorSizeKind::SizeUnknown),
            "Generator" => {
                if let Some(inner_iter) = top_level_generic_args(iter_type, "Base.Generator")
                    .or_else(|| top_level_generic_args(iter_type, "Generator"))
                    .and_then(|args| args.into_iter().next())
                {
                    self.native_iterator_size_kind_for_generator_iter_type_name(&inner_iter)
                } else {
                    Ok(NativeIteratorSizeKind::SizeUnknown)
                }
            }
            "Enumerate" => {
                if let Some(inner_iter) = top_level_generic_args(iter_type, "Base.Enumerate")
                    .or_else(|| top_level_generic_args(iter_type, "Enumerate"))
                    .and_then(|args| args.into_iter().next())
                {
                    self.native_iterator_size_kind_for_generator_iter_type_name(&inner_iter)
                } else {
                    Ok(NativeIteratorSizeKind::HasLength)
                }
            }
            "Zip" | "Zip3" | "Zip4" | "Zip5" | "Zip6" | "Zip7" => {
                let args = top_level_generic_args(iter_type, &format!("Base.{base_name}"))
                    .or_else(|| top_level_generic_args(iter_type, base_name));
                let Some(args) = args else {
                    return Ok(NativeIteratorSizeKind::HasLength);
                };
                let mut args = args.iter().rev();
                let Some(last) = args.next() else {
                    return Ok(NativeIteratorSizeKind::HasLength);
                };
                let mut result =
                    self.native_iterator_size_kind_for_generator_iter_type_name(last)?;
                for arg in args {
                    result = native_zip_size_kind(
                        self.native_iterator_size_kind_for_generator_iter_type_name(arg)?,
                        result,
                    );
                }
                Ok(result)
            }
            "Count" => Ok(NativeIteratorSizeKind::IsInfinite),
            _ => Ok(NativeIteratorSizeKind::HasLength),
        }
    }

    fn dynamic_call_generator_trait_name(
        &self,
        fallback_func_index: usize,
        candidates: &[DynamicCallCandidate],
    ) -> Option<&'static str> {
        let name_matches = |idx: usize| {
            self.functions
                .get(idx)
                .and_then(|func| match strip_module_prefix(func.name.as_str()) {
                    "IteratorSize" => Some("IteratorSize"),
                    "IteratorEltype" => Some("IteratorEltype"),
                    _ => None,
                })
        };

        if fallback_func_index != usize::MAX {
            if let Some(name) = name_matches(fallback_func_index) {
                return Some(name);
            }
        }

        candidates.iter().find_map(|candidate| match candidate {
            DynamicCallCandidate::Method(idx) => name_matches(*idx),
            DynamicCallCandidate::NativeIterator(_) => None,
        })
    }

    fn dynamic_call_has_function_name(
        &self,
        fallback_func_index: usize,
        candidates: &[DynamicCallCandidate],
        target: &str,
    ) -> bool {
        let name_matches = |idx: usize| {
            self.functions
                .get(idx)
                .is_some_and(|func| strip_module_prefix(func.name.as_str()) == target)
        };

        if fallback_func_index != usize::MAX && name_matches(fallback_func_index) {
            return true;
        }

        candidates.iter().any(|candidate| match candidate {
            DynamicCallCandidate::Method(idx) => name_matches(*idx),
            DynamicCallCandidate::NativeIterator(_) => false,
        })
    }

    fn dynamic_call_has_user_function_name(
        &self,
        fallback_func_index: usize,
        candidates: &[DynamicCallCandidate],
        target: &str,
    ) -> bool {
        let user_name_matches = |idx: usize| {
            idx >= self.base_function_count
                && self
                    .functions
                    .get(idx)
                    .is_some_and(|func| strip_module_prefix(func.name.as_str()) == target)
        };

        if fallback_func_index != usize::MAX && user_name_matches(fallback_func_index) {
            return true;
        }

        candidates.iter().any(|candidate| match candidate {
            DynamicCallCandidate::Method(idx) => user_name_matches(*idx),
            DynamicCallCandidate::NativeIterator(_) => false,
        })
    }

    fn candidate_indices_have_user_function_name(
        &self,
        candidates: &[usize],
        target: &str,
    ) -> bool {
        candidates.iter().any(|idx| {
            *idx >= self.base_function_count
                && self
                    .functions
                    .get(*idx)
                    .is_some_and(|func| strip_module_prefix(func.name.as_str()) == target)
        })
    }

    fn dynamic_call_native_range_unary_accessor(
        &self,
        fallback_func_index: usize,
        candidates: &[DynamicCallCandidate],
        args: &[Value],
    ) -> Option<Value> {
        if args.len() != 1 {
            return None;
        }
        let Value::Range(range) = &args[0] else {
            return None;
        };
        let accessor = ["first", "last", "step", "length"]
            .into_iter()
            .find(|target| {
                self.dynamic_call_has_function_name(fallback_func_index, candidates, target)
                    && !self.dynamic_call_has_user_function_name(
                        fallback_func_index,
                        candidates,
                        target,
                    )
            })?;
        match accessor {
            "first" => range.first_value(),
            "last" => range.last_value(),
            "step" => Some(range.typed_step()),
            "length" => Some(range.length_value()),
            _ => None,
        }
    }

    fn dynamic_call_native_range_iterate(
        &mut self,
        fallback_func_index: usize,
        candidates: &[DynamicCallCandidate],
        args: &[Value],
    ) -> Result<Option<Value>, VmError> {
        if !(args.len() == 1 || args.len() == 2)
            || !self.dynamic_call_has_function_name(fallback_func_index, candidates, "iterate")
            || self.dynamic_call_has_user_function_name(fallback_func_index, candidates, "iterate")
            || !matches!(args.first(), Some(Value::Range(_)))
        {
            return Ok(None);
        }

        let result = if let Some(state) = args.get(1) {
            self.iterate_next(&args[0], state)?
        } else {
            self.iterate_first(&args[0])?
        };
        Ok(Some(result))
    }

    fn dynamic_call_generator_trait_result(
        &mut self,
        trait_name: &str,
        value: &Value,
    ) -> Result<Option<Value>, VmError> {
        let result = match value {
            Value::Generator(g) => match trait_name {
                "IteratorSize" if self.generator_is_filtered(g) => {
                    self.zero_field_struct_value("SizeUnknown")?
                }
                "IteratorSize" => self.iterator_size_value_for_native_generator_iter(&g.iter)?,
                "IteratorEltype" => self.zero_field_struct_value("EltypeUnknown")?,
                _ => return Ok(None),
            },
            Value::DataType(julia_type) => {
                let Some(iter_type) = generator_iter_type_name(julia_type) else {
                    return Ok(None);
                };
                match trait_name {
                    "IteratorSize" => {
                        self.iterator_size_value_for_generator_iter_type_name(&iter_type)?
                    }
                    "IteratorEltype" => self.zero_field_struct_value("EltypeUnknown")?,
                    _ => return Ok(None),
                }
            }
            _ => {
                let Some(iter) = self.pure_generator_iter_value(value) else {
                    return Ok(None);
                };
                match trait_name {
                    "IteratorSize" if self.value_is_filter_struct(&iter) => {
                        self.zero_field_struct_value("SizeUnknown")?
                    }
                    "IteratorSize" => self.iterator_size_value_for_native_generator_iter(&iter)?,
                    "IteratorEltype" => self.zero_field_struct_value("EltypeUnknown")?,
                    _ => return Ok(None),
                }
            }
        };

        Ok(Some(result))
    }

    /// Whether `arg` cannot satisfy a candidate's `expected_type` slot because of
    /// a container-shape mismatch. Extracted from the dynamic-dispatch candidate
    /// filter to keep it flat (Issue #6833).
    fn dynamic_candidate_arg_mismatch(&self, arg: &Value, expected_type: &str) -> bool {
        is_rust_dict_parametric_mismatch(arg, expected_type)
            || is_native_range_candidate_mismatch(arg, expected_type)
            || is_struct_dict_bare_mismatch(arg, expected_type, &self.struct_heap)
            || native_array_view_wrapper_candidate_mismatch(arg, expected_type)
    }

    fn dynamic_method_candidate_value_mismatch(&self, idx: usize, args: &[Value]) -> bool {
        let Some(func) = self.functions.get(idx) else {
            return true;
        };
        let Some(param_types) = expanded_param_types_for_call(func, args.len()) else {
            return true;
        };
        if self.is_base_program_function_index(idx)
            && !self.is_native_array_exempt_function(idx)
            && params_cross_native_array_wrapper_boundary(args, &param_types)
        {
            return true;
        }
        self.function_candidate_binding_count(idx, args, &param_types, &func.type_params)
            .is_none()
    }

    /// Excludes candidates whose declared Base concrete-struct parameter has a
    /// nominal-origin conflict with the actual runtime argument, routing the
    /// `IterateDynamic` family-fallback resolvers through the same
    /// origin-aware fence (`function_candidate_has_nominal_origin_conflict`,
    /// Issue #10295) that already gates the primary metadata scorer via
    /// [`Self::dynamic_method_candidate_value_mismatch`] (Issue #10879).
    ///
    /// Without this, a same-named external struct with no `iterate` method of
    /// its own could reach a Base `iterate` method meant for Base's own
    /// same-named type through `runtime_core_family_fallback_matches`, which
    /// intentionally treats bare and qualified spellings as the same family
    /// and has no origin awareness of its own.
    fn origin_safe_iterate_candidates(
        &self,
        candidates: &[usize],
        dispatch_args: &[Value],
    ) -> Vec<usize> {
        candidates
            .iter()
            .copied()
            .filter(|&idx| {
                let Some(func) = self.functions.get(idx) else {
                    return true;
                };
                let Some(param_types) = expanded_param_types_for_call(func, dispatch_args.len())
                else {
                    return true;
                };
                !self.function_candidate_has_nominal_origin_conflict(
                    idx,
                    dispatch_args,
                    &param_types,
                    &func.type_params,
                )
            })
            .collect()
    }

    /// User-defined subset of `indices` (function index past the Base allotment).
    /// Tier-1 of the metadata-backed dynamic selection (Issue #6833 flatten).
    fn user_metadata_candidate_indices(&self, indices: &[usize]) -> Vec<usize> {
        indices
            .iter()
            .copied()
            .filter(|idx| *idx >= self.base_function_count)
            .collect()
    }

    /// Base-only allowlist subset of `indices`: Base-program functions named
    /// `empty`. Tier-2 of the metadata-backed dynamic selection (Issue #6833).
    fn base_empty_metadata_candidate_indices(&self, indices: &[usize]) -> Vec<usize> {
        indices
            .iter()
            .copied()
            .filter(|idx| {
                self.is_base_program_function_index(*idx)
                    && self.functions.get(*idx).is_some_and(|func| {
                        matches!(
                            func.name.strip_prefix("Base.").unwrap_or(&func.name),
                            "empty"
                        )
                    })
            })
            .collect()
    }

    fn dedup_tier_filtered_candidates<'a>(
        &self,
        filtered_candidates: &'a [(usize, &'a str, &'a CoreType, Option<&'a CoreType>)],
    ) -> Vec<(usize, &'a str, &'a CoreType, Option<&'a CoreType>)> {
        let mut deduped: Vec<(
            (usize, &'a str, &'a CoreType, Option<&'a CoreType>),
            (CoreType, Option<CoreType>),
        )> = Vec::with_capacity(filtered_candidates.len());

        for &(idx, name, slot, gate) in filtered_candidates {
            let slot_key = slot.canonicalize_signature_for_dedup();
            let key = (
                slot_key.clone(),
                tier_fallback_gate_key_for_dedup(&slot_key, gate),
            );
            if let Some(pos) = deduped
                .iter()
                .position(|(_, existing_key)| existing_key == &key)
            {
                if idx > deduped[pos].0 .0 {
                    deduped[pos] = ((idx, name, slot, gate), key);
                }
            } else {
                deduped.push(((idx, name, slot, gate), key));
            }
        }

        deduped
            .into_iter()
            .map(|(candidate, _)| candidate)
            .collect()
    }

    /// Tier-dispatch fallback over the metadata-filtered candidates: resolve the
    /// `(slot, gate)` signatures via the family-fallback matcher, falling back to
    /// `fallback_func_index`. Extracted from `execute_call_dynamic` (Issue #6833).
    ///
    /// Returns `Err(VmError::MethodError)` when two or more candidates are tied
    /// at the top score (Issue #8999 tiebreak surfacing).
    fn resolve_tier_filtered_fallback(
        &self,
        filtered_candidates: &[(usize, &str, &CoreType, Option<&CoreType>)],
        actual_cores: &[CoreType],
        fallback_func_index: usize,
    ) -> Result<usize, VmError> {
        let filtered_candidates = self.dedup_tier_filtered_candidates(filtered_candidates);
        resolve_runtime_core_signature_slice_candidates_with_family_fallback_or_tie(
            &self.struct_hierarchy,
            filtered_candidates
                .iter()
                .map(|(idx, _, slot, gate)| RuntimeCoreSliceCandidate {
                    idx: *idx,
                    slots: std::slice::from_ref(*slot),
                    signature: *gate,
                }),
            actual_cores,
            runtime_core_family_fallback_matches,
            |actual, expected| self.check_subtype_core(actual, expected),
        )
        .map_err(|()| {
            // Tie at top score: surface as ambiguous (Issue #8999).
            let name = filtered_candidates
                .first()
                .map(|(_, n, _, _)| *n)
                .unwrap_or("<unknown>");
            let candidate_list: String = filtered_candidates
                .iter()
                .map(|(_, n, slot, _)| format!("  {}(::{:?})\n", n, slot))
                .collect();
            VmError::MethodError(format!(
                "{name} is ambiguous. Multiple candidates matched with equal specificity:\n{candidate_list}"
            ))
        })
        .map(|opt| opt.map(|(idx, _)| idx).unwrap_or(fallback_func_index))
    }

    /// Scored-dispatch fallback: derive each scored candidate's per-arity core
    /// signature from its `FunctionInfo` and resolve via the family-fallback
    /// matcher (Issues #6336/#6502). Extracted from `execute_call_dynamic` to
    /// keep it flat (Issue #6833).
    ///
    /// Returns `Err(VmError::MethodError)` when two or more candidates are tied
    /// at the top score (Issue #8999 tiebreak surfacing).
    fn resolve_scored_family_fallback(
        &self,
        scored: &[usize],
        actual_cores: &[CoreType],
    ) -> Result<Option<usize>, VmError> {
        let derived_signatures: Vec<(
            usize,
            crate::vm::dispatch_binding::RuntimeCandidateCoreSignature,
        )> = scored
            .iter()
            .filter_map(|&idx| {
                let func = self.functions.get(idx)?;
                let param_types = expanded_param_types_for_call(func, actual_cores.len())?;
                let signature = crate::vm::dispatch_binding::build_runtime_candidate_core_signature(
                    &param_types,
                    &func.type_params,
                );
                Some((idx, signature))
            })
            .collect();
        resolve_runtime_core_signature_slice_candidates_with_family_fallback_or_tie(
            &self.struct_hierarchy,
            derived_signatures
                .iter()
                .map(|(idx, signature)| RuntimeCoreSliceCandidate {
                    idx: *idx,
                    slots: signature.slots.as_slice(),
                    signature: signature.signature.as_ref(),
                }),
            actual_cores,
            runtime_core_family_fallback_matches,
            |actual, expected| self.check_subtype_core(actual, expected),
        )
        .map_err(|()| {
            // Tie at top score: surface as ambiguous (Issue #8999).
            let name = scored
                .first()
                .and_then(|&idx| self.functions.get(idx))
                .map(|f| f.name.as_str())
                .unwrap_or("<unknown>");
            let candidate_list: String = scored
                .iter()
                .filter_map(|&idx| self.functions.get(idx))
                .map(|f| {
                    let sig: Vec<_> =
                        f.param_julia_types.iter().map(|t| format!("::{t}")).collect();
                    format!("  {}({})\n", f.name, sig.join(", "))
                })
                .collect();
            VmError::MethodError(format!(
                "{name} is ambiguous. Multiple candidates matched with equal specificity:\n{candidate_list}"
            ))
        })
        .map(|opt| opt.map(|(func_index, _score)| func_index))
    }

    fn resolve_iterate_struct_family_fallback(
        &self,
        scored: &[usize],
        actual_cores: &[CoreType],
    ) -> Option<usize> {
        let actual_first = actual_cores.first()?;
        let matches: Vec<usize> = scored
            .iter()
            .filter_map(|&idx| {
                let func = self.functions.get(idx)?;
                let param_types = expanded_param_types_for_call(func, actual_cores.len())?;
                let signature = crate::vm::dispatch_binding::build_runtime_candidate_core_signature(
                    &param_types,
                    &func.type_params,
                );
                let first_slot = signature.slots.first()?;
                if !runtime_core_family_fallback_matches(actual_first, first_slot) {
                    return None;
                }
                let remaining_match = signature.slots.iter().zip(actual_cores.iter()).skip(1).all(
                    |(expected, actual)| {
                        matches!(expected, CoreType::Any)
                            || self.check_subtype_core(actual, expected)
                            || runtime_core_family_fallback_matches(actual, expected)
                    },
                );
                remaining_match.then_some(idx)
            })
            .collect();
        match matches.as_slice() {
            [idx] => Some(*idx),
            _ => None,
        }
    }

    fn resolve_runtime_iterate_struct_family_fallback(
        &self,
        actual_cores: &[CoreType],
        dispatch_args: &[Value],
    ) -> Option<usize> {
        let runtime_iterate_candidates: Vec<usize> = self
            .functions
            .iter()
            .enumerate()
            .filter_map(|(idx, func)| (strip_module_prefix(&func.name) == "iterate").then_some(idx))
            .collect();
        let runtime_iterate_candidates =
            self.origin_safe_iterate_candidates(&runtime_iterate_candidates, dispatch_args);
        self.resolve_iterate_struct_family_fallback(&runtime_iterate_candidates, actual_cores)
    }

    fn cached_iterate_candidate(&self, cached: usize, candidates: &[usize]) -> Option<usize> {
        if cached == usize::MAX {
            return None;
        }
        if candidates.contains(&cached) {
            return Some(cached);
        }
        self.functions
            .get(cached)
            .is_some_and(|func| strip_module_prefix(&func.name) == "iterate")
            .then_some(cached)
    }

    fn visible_dynamic_candidates(
        &self,
        candidates: &[DynamicCallCandidate],
    ) -> Vec<DynamicCallCandidate> {
        let world = self.current_dispatch_world();
        candidates
            .iter()
            .copied()
            .filter(|candidate| match *candidate {
                DynamicCallCandidate::Method(idx) => self.function_visible_in_world(idx, world),
                DynamicCallCandidate::NativeIterator(_) => true,
            })
            .collect()
    }

    /// Execute dynamic dispatch call instructions.
    ///
    /// Returns an `unhandled` error if the instruction is not a dynamic call operation.
    /// Delegates to specialized handlers for binary, typed, and function variable dispatch.
    #[inline]
    pub(super) fn execute_call_dynamic(
        &mut self,
        instr: &Instr,
    ) -> Result<DispatchAction, VmError> {
        // Try specialized handlers first
        match instr {
            Instr::CallDynamicBinary(..)
            | Instr::CallDynamicBinaryBoth(..)
            | Instr::CallDynamicBinaryNoFallback(..) => {
                return self.execute_call_dynamic_binary(instr);
            }
            Instr::CallTypedDispatch(..)
            | Instr::CallTypedDispatchOrBuiltin(..)
            | Instr::CallTypedDispatchOrBuiltinResult(..)
            | Instr::CallTypedDispatchOrBuiltinStoreDict(..)
            | Instr::CallTypedDispatchOrBuiltinStoreDictResult(..)
            | Instr::CallParametricConstructorDispatch(..)
            | Instr::CallTypeConstructor => {
                return self.execute_call_dynamic_typed(instr);
            }
            Instr::CallGlobalRef(..)
            | Instr::CallFunctionVariable(..)
            | Instr::InvokeFunctionVariable(..)
            | Instr::InvokeFunctionVariableWithKwargs(..)
            | Instr::InvokeFunctionVariableDynamicSignature(..)
            | Instr::InvokeFunctionVariableDynamicSignatureWithKwargs(..)
            | Instr::CallFunctionVariableWithSplat(..)
            | Instr::CallFunctionVariableWithKwargsSplat(..) => {
                return self.execute_call_function_variable(instr);
            }
            _ => {}
        }

        match instr {
            Instr::CallDynamic(operands) => {
                let fallback_func_index = &operands.fallback_func_index;
                let arg_count = &operands.arg_count;
                let candidates = &operands.candidates;
                // Runtime method dispatch: check argument types and select best match
                #[cfg(debug_assertions)]
                if dispatch_debug_enabled() {
                    dispatch_debug_log(format_args!(
                        "[DISPATCH] CallDynamic: arg_count={}, candidates={}, fallback=#{}",
                        arg_count,
                        candidates.len(),
                        fallback_func_index
                    ));
                }
                // Pop arguments to inspect their types
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

                let visible_candidates = self.visible_dynamic_candidates(candidates);
                let single_payload_method = match candidates.as_slice() {
                    [DynamicCallCandidate::Method(idx)] => Some(*idx),
                    _ => None,
                };
                // A source-ordered forward reference may carry the later
                // method's aligned index before its definition marker ran. The
                // `u64::MAX` min-world sentinel means the generic is not bound
                // yet at all, so the single-candidate world-age compatibility
                // fallback below must not execute that dormant body.
                if let Some(dormant_name) = single_payload_method.and_then(|idx| {
                    self.functions
                        .get(idx)
                        .filter(|function| function.min_world == u64::MAX)
                        .map(|function| function.name.clone())
                }) {
                    self.raise(VmError::UndefVarError(dormant_name))?;
                    return Ok(DispatchAction::Continue);
                }
                let carried_single_candidate;
                // Persistent REPL can execute a freshly compiled top-level call
                // under an older eval frame world while the bytecode payload
                // already names the single source-visible method. With no
                // competing methods in the payload, keeping that method preserves
                // the compiled source order without weakening multi-method
                // world-age filtering.
                let candidates = if visible_candidates.is_empty() {
                    if let Some(idx) = single_payload_method {
                        carried_single_candidate = [DynamicCallCandidate::Method(idx)];
                        carried_single_candidate.as_slice()
                    } else {
                        visible_candidates.as_slice()
                    }
                } else {
                    visible_candidates.as_slice()
                };
                let world = self.current_dispatch_world();
                let fallback_func_index = if *fallback_func_index != usize::MAX
                    && self.function_visible_in_world(*fallback_func_index, world)
                {
                    *fallback_func_index
                } else {
                    usize::MAX
                };

                if let Some(value) =
                    self.dynamic_call_native_range_iterate(fallback_func_index, candidates, &args)?
                {
                    self.stack.push(value);
                    return Ok(DispatchAction::Continue);
                }

                if args.len() == 1 {
                    if let Some(value) = self.dynamic_call_native_range_unary_accessor(
                        fallback_func_index,
                        candidates,
                        &args,
                    ) {
                        self.stack.push(value);
                        return Ok(DispatchAction::Continue);
                    }

                    if let Some(trait_name) =
                        self.dynamic_call_generator_trait_name(fallback_func_index, candidates)
                    {
                        if let Some(result) =
                            self.dynamic_call_generator_trait_result(trait_name, &args[0])?
                        {
                            self.stack.push(result);
                            return Ok(DispatchAction::Continue);
                        }
                    }
                }

                // `collect(x::Any)` uses this CallDynamic path with native
                // candidates for VM-backed containers. Generators are not Pure
                // Julia structs, so keep their representation boundary before
                // normal candidate scoring. Struct-backed iterators such as Zip
                // are scored first so user/Pure Julia methods can win before
                // the native collect compatibility sentinel.
                if *arg_count == 1
                    && candidates.contains(&DynamicCallCandidate::NativeIterator(
                        NativeIteratorKind::Generator,
                    ))
                {
                    let has_user_collect_candidate = self.has_user_collect_candidate(candidates);
                    let has_range_collect_candidate =
                        self.has_applicable_native_range_collect_candidate(&args[0], candidates);

                    if matches!(args[0], Value::Range(_))
                        && !has_user_collect_candidate
                        && !has_range_collect_candidate
                    {
                        // CollectFallback: runtime-range-pre-score-boundary
                        let result = self.collect_iterator(&args[0])?;
                        self.stack.push(result);
                        return Ok(DispatchAction::Continue);
                    }

                    if !has_user_collect_candidate
                        && !self.has_generator_collect_candidate(candidates)
                    {
                        if let Value::Generator(g) = &args[0] {
                            // Issue #9200 S6: bypassable for the retirement A/B
                            // measurement — collect the generator purely through
                            // its `iterate` protocol (the upstream iterate-only
                            // ideal) instead of the `collect_generator` HOF fast
                            // path. Default off = shipping.
                            if crate::vm::generator_fastpath_gate::generator_fastpath_disabled() {
                                let result =
                                    self.collect_iterator_via_iterate_protocol(&args[0])?;
                                self.stack.push(result);
                                return Ok(DispatchAction::Continue);
                            }
                            if self.generator_can_use_generic_collect(g)? {
                                self.start_function_call(fallback_func_index, args)?;
                                return Ok(DispatchAction::Continue);
                            }
                            // CollectFallback: runtime-generator-pre-score-boundary
                            let iter = (*g.iter).clone();
                            if let Some(result) = self.collect_generator(
                                g.callable.clone(),
                                &iter,
                                g.result_element_type.clone(),
                            )? {
                                self.stack.push(result);
                            }
                            return Ok(DispatchAction::Continue);
                        }
                    }

                    // Issue #5196: `Core.SimpleVector` (svec) collects to a
                    // `Vector{Any}` preserving heterogeneous elements. Route it
                    // through the native `collect_iterator` boundary directly so
                    // it never enters the Pure Julia `_collect` element-type
                    // widening path (which mis-coerces type-object elements such
                    // as `Tuple{Int,String}.parameters`).
                    if matches!(args[0], Value::SimpleVector(_)) && !has_user_collect_candidate {
                        // CollectFallback: runtime-simplevector-pre-score-boundary
                        let result = self.collect_iterator(&args[0])?;
                        self.stack.push(result);
                        return Ok(DispatchAction::Continue);
                    }
                }

                // Dispatch based on the full argument type tuple. Candidate
                // pre-filtering still uses the first slot for representation
                // boundary checks, but the cache key must include every
                // dispatch argument so one call site can alternate between
                // `f(dest, ::Broadcasted)` and `f(dest, ::Float64)` safely
                // (Issue #8368).
                let selected_func_index = {
                    let call_site_ip = self.ip - 1;
                    let arg_refs: Vec<&Value> = args.iter().collect();
                    let arg_fingerprint = self.call_site_arg_fingerprints(&arg_refs);

                    if let Some(cached) = arg_fingerprint
                        .as_deref()
                        .and_then(|fp| self.lookup_call_site_inline_cache(call_site_ip, fp))
                    {
                        cached
                    } else {
                        // L2 dispatch cache keyed on the interned arg-id sequence
                        // (Issue #9197 S3): reuse the L1 `arg_fingerprint` instead
                        // of deriving per-arg type-name strings. When any argument
                        // has no tracked dispatch identity, `arg_fingerprint` is
                        // `None` and the call skips L2 and re-resolves — symmetric
                        // with the L1 skip policy.
                        // Check dispatch cache first (Issue #2943, #3355)
                        if let Some(cached) = arg_fingerprint
                            .as_deref()
                            .and_then(|key| self.lookup_call_site_dispatch_cache(call_site_ip, key))
                        {
                            self.store_call_site_inline_cache(
                                call_site_ip,
                                arg_fingerprint.as_deref(),
                                cached,
                            );
                            cached
                        } else {
                            let actual_cores: Vec<_> = args
                                .iter()
                                .map(|arg| {
                                    let ty = self.dispatch_julia_type_for_value(arg);
                                    crate::vm::dispatch_binding::runtime_actual_core_type(&ty)
                                })
                                .collect();
                            let fallback_actual_core = actual_cores;
                            // Scored dispatch: prefer the FunctionInfo-backed VM
                            // resolver so runtime `where` bounds remain available
                            // for cases such as `Type{T}` / `Vector{T}` methods
                            // reached through an `Any` container (Issue #6202).
                            // Keep the string-pattern resolver as fallback for
                            // sentinel/native candidates and legacy projections.
                            // The payload carries only structured candidates
                            // (Issue #6496); the expected type name pairs are
                            // derived here, once per call site + argument type
                            // (the result is dispatch-cached below).
                            let named_candidates: Vec<(usize, String, CoreType, Option<CoreType>)> =
                                candidates
                                    .iter()
                                    .map(|candidate| {
                                        self.resolve_dynamic_call_candidate_signature(*candidate)
                                    })
                                    .collect();
                            let filtered_candidates: Vec<_> = named_candidates
                                .iter()
                                .filter_map(|(idx, expected_type, slot, gate)| {
                                    if args.first().is_some_and(|arg| {
                                        self.dynamic_candidate_arg_mismatch(arg, expected_type)
                                    }) {
                                        return None;
                                    }
                                    if *idx != usize::MAX
                                        && self.dynamic_method_candidate_value_mismatch(*idx, &args)
                                    {
                                        return None;
                                    }
                                    Some((*idx, expected_type.as_str(), slot, gate.as_ref()))
                                })
                                .collect();
                            let metadata_candidate_indices: Vec<_> = filtered_candidates
                                .iter()
                                .filter_map(|(idx, _, _, _)| (*idx != usize::MAX).then_some(*idx))
                                .collect();
                            let callee_identity =
                                CalleeIdentity::from_function_name(&operands.callee_name);
                            let resolve_candidates = |candidate_indices: &[usize]| {
                                let request = self.runtime_call_request(
                                    callee_identity.clone(),
                                    candidate_indices,
                                    &args,
                                );
                                self.resolve_runtime_call_request(&request, &args)
                            };
                            // Metadata-backed selection tiers, narrowing the
                            // candidate index list: all candidates → user-defined
                            // only → Base-only allowlist (`empty`). The ordered
                            // first-winner control flow is owned by the shared
                            // selection core (`selection::pick_first_tier`,
                            // Issue #6502); each tier's index list is still built
                            // lazily only when the previous tier found nothing.
                            let tier_pick = selection::pick_first_tier(3, |tier| match tier {
                                0 => resolve_candidates(&metadata_candidate_indices),
                                1 => {
                                    let user = self.user_metadata_candidate_indices(
                                        &metadata_candidate_indices,
                                    );
                                    resolve_candidates(&user)
                                }
                                _ => {
                                    let base = self.base_empty_metadata_candidate_indices(
                                        &metadata_candidate_indices,
                                    );
                                    resolve_candidates(&base)
                                }
                            });
                            let result = match tier_pick {
                                Ok(Some(idx)) => idx,
                                Ok(None) => {
                                    match self.resolve_tier_filtered_fallback(
                                        &filtered_candidates,
                                        &fallback_actual_core,
                                        fallback_func_index,
                                    ) {
                                        Ok(idx) => idx,
                                        Err(err) => {
                                            self.raise(err)?;
                                            return Ok(DispatchAction::Continue);
                                        }
                                    }
                                }
                                Err(err) => {
                                    self.raise(err)?;
                                    return Ok(DispatchAction::Continue);
                                }
                            };
                            // Store in the L2 cache keyed by the interned arg-id
                            // sequence (Issue #9197 S3); untracked kinds have no
                            // key and are simply not L2-cached.
                            if let Some(key) = arg_fingerprint.as_deref() {
                                self.store_call_site_dispatch_cache(call_site_ip, key, result);
                            }
                            self.store_call_site_inline_cache(
                                call_site_ip,
                                arg_fingerprint.as_deref(),
                                result,
                            );
                            result
                        }
                    }
                };

                if selected_func_index == usize::MAX {
                    let has_native_collect_sentinel = candidates
                        .iter()
                        .any(|c| matches!(c, DynamicCallCandidate::NativeIterator(_)));
                    if has_native_collect_sentinel && args.len() == 1 {
                        if let Value::Generator(g) = &args[0] {
                            // Issue #9200 S6: bypassable for the retirement A/B
                            // measurement (see the pre-score-boundary site above).
                            if crate::vm::generator_fastpath_gate::generator_fastpath_disabled() {
                                let result =
                                    self.collect_iterator_via_iterate_protocol(&args[0])?;
                                self.stack.push(result);
                                return Ok(DispatchAction::Continue);
                            }
                            if self.generator_can_use_generic_collect(g)? {
                                self.start_function_call(fallback_func_index, args)?;
                                return Ok(DispatchAction::Continue);
                            }
                            let iter = (*g.iter).clone();
                            if let Some(result) = self.collect_generator(
                                g.callable.clone(),
                                &iter,
                                g.result_element_type.clone(),
                            )? {
                                self.stack.push(result);
                            }
                            return Ok(DispatchAction::Continue);
                        }

                        let route_to_native_collect = match &args[0] {
                            Value::Range(_) => true,
                            Value::Struct(s) => matches_native_collect_iterator(&s.struct_name),
                            Value::StructRef(idx) => self
                                .struct_heap
                                .get(*idx)
                                .map(|s| matches_native_collect_iterator(&s.struct_name))
                                .unwrap_or(false),
                            _ => false,
                        };
                        if route_to_native_collect {
                            // CollectFallback: native-collect-sentinel-boundary
                            let result = self.collect_iterator(&args[0])?;
                            self.stack.push(result);
                            return Ok(DispatchAction::Continue);
                        }
                    }
                    // A builtin-backed name that gains a Pure Julia method
                    // (e.g. `ncodeunits(::SubstitutionString)`) turns every
                    // call site into a CallDynamic with only that method as
                    // candidate; arguments the builtin handles (String) then
                    // miss dispatch here. Fall back to the builtin handler,
                    // mirroring the identical fallback in the
                    // CallFunctionVariable miss path (Issue #10735).
                    if let Some(builtin_id) =
                        BuiltinId::from_name(strip_module_prefix(&operands.callee_name))
                    {
                        if let Some(value) = self.execute_runtime_builtin_immediate(
                            builtin_id,
                            &operands.callee_name,
                            &args,
                        )? {
                            self.stack.push(value);
                        }
                        return Ok(DispatchAction::Continue);
                    }
                    // A runtime dispatch failure must raise a CATCHABLE
                    // `MethodError`, like upstream Julia — route it through
                    // `self.raise` so an enclosing `try/catch` can intercept it
                    // (Issue #5648). `return Err(..)` aborted the VM uncatchably.
                    // With no handler, `raise` re-propagates the error (still
                    // aborts), preserving the prior top-level behavior.
                    self.raise(VmError::MethodError(
                        "no matching runtime method candidate".to_string(),
                    ))?;
                    return Ok(DispatchAction::Continue);
                }

                let func = match self.get_function_cloned_or_raise(selected_func_index)? {
                    Some(f) => f,
                    None => return Ok(DispatchAction::Continue),
                };

                if args.len() == 1
                    && strip_module_prefix(&func.name) == "collect"
                    && self.is_base_program_function_index(selected_func_index)
                {
                    if let Value::Generator(g) = &args[0] {
                        // Issue #9200 S6: bypassable for the retirement A/B
                        // measurement — when the gate is set, collect the
                        // generator purely through its `iterate` protocol
                        // (the upstream iterate-only ideal) instead of the
                        // `collect_generator` HOF fast path.
                        if crate::vm::generator_fastpath_gate::generator_fastpath_disabled() {
                            let result = self.collect_iterator_via_iterate_protocol(&args[0])?;
                            self.stack.push(result);
                            return Ok(DispatchAction::Continue);
                        }
                        let iter = (*g.iter).clone();
                        if let Some(result) = self.collect_generator(
                            g.callable.clone(),
                            &iter,
                            g.result_element_type.clone(),
                        )? {
                            self.stack.push(result);
                        }
                        return Ok(DispatchAction::Continue);
                    }
                }

                if args.len() == 1 {
                    if let Value::Generator(g) = &args[0] {
                        match strip_module_prefix(func.name.as_str()) {
                            "_generator_empty_sum_value" => {
                                if let Some(result) = self.generator_empty_sum_value(g)? {
                                    self.stack.push(result);
                                }
                                return Ok(DispatchAction::Continue);
                            }
                            "IteratorSize" => {
                                // A FILTERED generator wraps its base iterator in
                                // `Iterators.Filter`, whose
                                // `IteratorSize(::Type{<:Filter})` is
                                // `SizeUnknown()`. The Issue #9200 S3 desugar puts a
                                // real `Filter` in `g.iter`; the tuple-destructuring
                                // lift path still collapses the filter into a
                                // `Filtered*` callable. Either way, delegating to the
                                // base iterator's trait would wrongly report
                                // `HasShape`/`HasLength` (Issue #9379), so report
                                // `SizeUnknown()` structurally (Issue #9320).
                                if self.generator_is_filtered(g) {
                                    let result = self.zero_field_struct_value("SizeUnknown")?;
                                    self.stack.push(result);
                                    return Ok(DispatchAction::Continue);
                                }
                                let iter = (*g.iter).clone();
                                let result =
                                    self.iterator_size_value_for_native_generator_iter(&iter)?;
                                self.stack.push(result);
                                return Ok(DispatchAction::Continue);
                            }
                            "IteratorEltype" => {
                                let result = self.zero_field_struct_value("EltypeUnknown")?;
                                self.stack.push(result);
                                return Ok(DispatchAction::Continue);
                            }
                            _ => {}
                        }
                    }
                    if let Some(iter) = self.pure_generator_iter_value(&args[0]) {
                        match strip_module_prefix(func.name.as_str()) {
                            "IteratorSize" if self.value_is_filter_struct(&iter) => {
                                let result = self.zero_field_struct_value("SizeUnknown")?;
                                self.stack.push(result);
                                return Ok(DispatchAction::Continue);
                            }
                            "IteratorSize" => {
                                let result =
                                    self.iterator_size_value_for_native_generator_iter(&iter)?;
                                self.stack.push(result);
                                return Ok(DispatchAction::Continue);
                            }
                            "IteratorEltype" => {
                                let result = self.zero_field_struct_value("EltypeUnknown")?;
                                self.stack.push(result);
                                return Ok(DispatchAction::Continue);
                            }
                            _ => {}
                        }
                    }
                    let generator_iter_type = match &args[0] {
                        Value::DataType(julia_type) => generator_iter_type_name(julia_type),
                        _ => None,
                    };
                    if let Some(iter_type) = generator_iter_type {
                        match strip_module_prefix(func.name.as_str()) {
                            "IteratorSize" => {
                                let result = self
                                    .iterator_size_value_for_generator_iter_type_name(&iter_type)?;
                                self.stack.push(result);
                                return Ok(DispatchAction::Continue);
                            }
                            "IteratorEltype" => {
                                let result = self.zero_field_struct_value("EltypeUnknown")?;
                                self.stack.push(result);
                                return Ok(DispatchAction::Continue);
                            }
                            _ => {}
                        }
                    }
                }
                if args.len() == 2 && func.name == "collect_similar" {
                    if let (container, Value::Generator(g)) = (&args[0], &args[1]) {
                        if matches!(container, Value::Memory(_)) {
                            // Let Pure Julia dispatch choose the container-aware
                            // collect_similar(::Memory, ::Generator) method.
                        } else {
                            // CollectFallback: collect-similar-generator-runtime-boundary
                            let iter = (*g.iter).clone();
                            if let Some(result) = self.collect_generator(
                                g.callable.clone(),
                                &iter,
                                g.result_element_type.clone(),
                            )? {
                                self.stack.push(result);
                            }
                            return Ok(DispatchAction::Continue);
                        }
                    }
                }

                let mut frame =
                    self.acquire_frame(func.local_slot_count, Some(selected_func_index));

                // Bind type parameters from where clauses (Issue #2468)
                self.bind_type_params(&func, &args, &mut frame);

                // Bind arguments (with varargs support), consuming args to avoid cloning
                if let Some(vararg_idx) = func.vararg_param_index {
                    let vararg_values: Vec<Value> = args.drain(vararg_idx..).collect();
                    for (slot, val) in func.param_slots[..vararg_idx].iter().zip(args) {
                        bind_value_to_slot(&mut frame, *slot, val, &mut self.struct_heap);
                    }
                    let vararg_tuple = Value::Tuple(TupleValue {
                        elements: vararg_values,
                    });
                    if let Some(slot) = func.param_slots.get(vararg_idx) {
                        bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
                    }
                } else {
                    // No varargs: bind 1-to-1, consuming args
                    for (slot, val) in func.param_slots.iter().zip(args) {
                        bind_value_to_slot(&mut frame, *slot, val, &mut self.struct_heap);
                    }
                }

                bind_kwargs_defaults(
                    &func,
                    &mut frame,
                    &mut self.struct_heap,
                    &self.code,
                    &self.functions,
                    self.frames.first(),
                    &self.global_slot_map,
                )?;

                self.return_ips.push(self.ip);
                self.try_push_call_frame(frame)?;
                self.ip = func.entry;
                Ok(DispatchAction::Continue)
            }

            Instr::CallDynamicOrBuiltin(builtin_id, ref candidates) => {
                // Runtime dispatch for unary functions with builtin fallback.
                // Pop the argument to inspect its type
                let arg = self.stack.pop_value()?;
                if matches!(builtin_id, BuiltinId::Length)
                    && !self.candidate_indices_have_user_function_name(candidates, "length")
                {
                    if let Value::Range(range) = &arg {
                        self.stack.push(range.length_value());
                        return Ok(DispatchAction::Continue);
                    }
                }
                let call_site_ip = self.ip - 1;
                let arg_fingerprint = self.call_site_arg_fingerprint(&arg);

                let matched = if let Some(cached) = arg_fingerprint
                    .as_deref()
                    .and_then(|fp| self.lookup_call_site_inline_cache(call_site_ip, fp))
                {
                    // Cache stores usize::MAX as sentinel for "no match" (use builtin)
                    if cached == usize::MAX {
                        None
                    } else {
                        Some(cached)
                    }
                } else {
                    // L2 dispatch cache keyed on the interned arg-id sequence
                    // (Issue #9197 S3): reuse the L1 `arg_fingerprint` rather than
                    // deriving the arg's type-name string. An untracked value kind
                    // has no id, so `arg_fingerprint` is `None` and the call skips
                    // L2 and re-resolves — symmetric with the L1 skip policy.
                    // Check dispatch cache first (Issue #2943, #3355)
                    if let Some(cached) = arg_fingerprint
                        .as_deref()
                        .and_then(|key| self.lookup_call_site_dispatch_cache(call_site_ip, key))
                    {
                        self.store_call_site_inline_cache(
                            call_site_ip,
                            arg_fingerprint.as_deref(),
                            cached,
                        );
                        // Cache stores usize::MAX as sentinel for "no match" (use builtin)
                        if cached == usize::MAX {
                            None
                        } else {
                            Some(cached)
                        }
                    } else {
                        // Scored dispatch before builtin fallback (Issue #3910).
                        // VM representation filters remain local; candidate score
                        // ordering is shared with other migrated dynamic calls.
                        // Issue #6496: the payload carries only candidate function
                        // indices; the expected first-parameter signatures are
                        // derived here, once per call site + argument type (the
                        // result is dispatch-cached below). Issue #6502 slice 2:
                        // matching runs on the structured `core_signature`
                        // projection.
                        let named_candidates: Vec<(usize, String, CoreType, Option<CoreType>)> =
                            candidates
                                .iter()
                                .map(|&idx| {
                                    let (rendered, slot, gate) =
                                        self.dynamic_candidate_expected_signature(idx);
                                    (idx, rendered, slot, gate)
                                })
                                .collect();
                        let actual_core_ty = self.dispatch_julia_type_for_value(&arg);
                        let actual_cores = [crate::vm::dispatch_binding::runtime_actual_core_type(
                            &actual_core_ty,
                        )];
                        let best_match = resolve_runtime_core_signature_candidates(
                            &self.struct_hierarchy,
                            named_candidates.iter().filter_map(
                                |(idx, expected_type, slot, gate)| {
                                    if self.dynamic_candidate_arg_mismatch(&arg, expected_type) {
                                        return None;
                                    }
                                    if self.dynamic_method_candidate_value_mismatch(
                                        *idx,
                                        std::slice::from_ref(&arg),
                                    ) {
                                        return None;
                                    }
                                    Some(RuntimeCoreCandidate {
                                        idx: *idx,
                                        slots: [slot],
                                        signature: gate.as_ref(),
                                    })
                                },
                            ),
                            &actual_cores,
                            |actual, expected| self.check_subtype_core(actual, expected),
                        );
                        let best_idx = best_match.map(|(idx, _score)| idx);
                        // Store in the L2 cache keyed by the interned arg-id
                        // sequence (Issue #9197 S3); untracked kinds are not cached.
                        let cache_val = best_idx.unwrap_or(usize::MAX);
                        if let Some(key) = arg_fingerprint.as_deref() {
                            self.store_call_site_dispatch_cache(call_site_ip, key, cache_val);
                        }
                        self.store_call_site_inline_cache(
                            call_site_ip,
                            arg_fingerprint.as_deref(),
                            cache_val,
                        );
                        best_idx
                    }
                };

                if let Some(func_index) = matched {
                    // Call the user-defined method
                    let func = match self.get_function_cloned_or_raise(func_index)? {
                        Some(f) => f,
                        None => return Ok(DispatchAction::Continue),
                    };

                    let mut frame = self.acquire_frame(func.local_slot_count, Some(func_index));

                    // Bind type parameters from where clauses (Issue #2468)
                    self.bind_type_params(&func, std::slice::from_ref(&arg), &mut frame);

                    if let Some(slot) = func.param_slots.first() {
                        bind_value_to_slot(&mut frame, *slot, arg, &mut self.struct_heap);
                    }

                    bind_kwargs_defaults(
                        &func,
                        &mut frame,
                        &mut self.struct_heap,
                        &self.code,
                        &self.functions,
                        self.frames.first(),
                        &self.global_slot_map,
                    )?;

                    self.return_ips.push(self.ip);
                    self.try_push_call_frame(frame)?;
                    self.ip = func.entry;
                } else {
                    // No matching struct method - fall back to builtin
                    if matches!(
                        builtin_id,
                        BuiltinId::Length
                            | BuiltinId::Size
                            | BuiltinId::Ndims
                            | BuiltinId::Eltype
                            | BuiltinId::Similar
                    ) {
                        self.stack.push(arg);
                        self.execute_builtin(*builtin_id, 1)?;
                        return Ok(DispatchAction::Continue);
                    }
                    // Special case for NegAny: preserve type
                    if matches!(builtin_id, BuiltinId::NegAny) {
                        let result = match arg {
                            Value::I64(v) => Value::I64(-v),
                            Value::F64(v) => Value::F64(-v),
                            Value::I8(v) => Value::I8(-v),
                            Value::I16(v) => Value::I16(-v),
                            Value::I32(v) => Value::I32(-v),
                            Value::I128(v) => Value::I128(-v),
                            Value::F16(v) => Value::F16(-v),
                            Value::F32(v) => Value::F32(-v),
                            _ => {
                                let arg_type = self.get_type_name(&arg);
                                self.raise(VmError::TypeError(format!(
                                    "expected numeric for NegAny, got {}",
                                    arg_type
                                )))?;
                                return Ok(DispatchAction::Continue);
                            }
                        };
                        self.stack.push(result);
                    } else {
                        // Resolve builtin to an f64 operation plus a matching
                        // BigFloat op, then preserve primitive float width while
                        // allowing heap-backed numeric structs to coerce to
                        // Float64 and BigFloat to keep arbitrary precision
                        // (Issue #6801).
                        type F64Op = fn(f64) -> f64;
                        type BfOp = fn(&RustBigFloat) -> RustBigFloat;
                        let (f64_op, bf_op): (F64Op, BfOp) = match builtin_id {
                            // Note: Exp, Log, Sin, Cos, Tan removed — now Pure Julia (base/math.jl)
                            BuiltinId::Floor => {
                                (f64::floor, RustBigFloat::floor_at_current_precision)
                            }
                            BuiltinId::Ceil => (f64::ceil, RustBigFloat::ceil_at_current_precision),
                            // Julia's default RoundNearest is round-half-to-even
                            // (banker's rounding): round(2.5)==2.0, round(0.5)==0.0.
                            // f64::round rounds half away from zero, so use
                            // round_ties_even to match the direct builtin handler
                            // and upstream (Issue #6742).
                            BuiltinId::Round => (
                                f64::round_ties_even,
                                RustBigFloat::round_nearest_even_at_current_precision,
                            ),
                            BuiltinId::Trunc => {
                                (f64::trunc, RustBigFloat::trunc_at_current_precision)
                            }
                            _ => {
                                self.raise(VmError::MethodError(format!(
                                    "unsupported builtin for CallDynamicOrBuiltin: {:?}",
                                    builtin_id
                                )))?;
                                return Ok(DispatchAction::Continue);
                            }
                        };
                        // A non-numeric operand is a dispatch miss upstream
                        // (`floor("a")` raises MethodError), not a conversion
                        // TypeError (Issue #10481).
                        let result = apply_unary_rounding_op_named_with_heap(
                            builtin_id.name(),
                            arg,
                            &self.struct_heap,
                            f64_op,
                            bf_op,
                        )?;
                        self.stack.push(result);
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::IterateDynamic(argc, ref candidates) => {
                // Dynamic dispatch for iterate() when collection type is Any at compile time.
                // Supports both 1-arg (initial) and 2-arg (subsequent) forms.
                let (coll, state_opt) = if *argc == 2 {
                    let state = self.stack.pop_value()?;
                    let coll = self.stack.pop_value()?;
                    (coll, Some(state))
                } else {
                    let coll = self.stack.pop_value()?;
                    (coll, None)
                };

                // Check if collection is a struct type
                let is_struct = matches!(&coll, Value::StructRef(_) | Value::Struct(_));

                // Some VM-backed Pure Julia wrappers use builtin iteration.
                let uses_builtin_iterate = match &coll {
                    Value::Struct(s) => uses_builtin_iterate_for_struct(&s.struct_name),
                    Value::StructRef(idx) => self
                        .struct_heap
                        .get(*idx)
                        .is_some_and(|s| uses_builtin_iterate_for_struct(&s.struct_name)),
                    _ => false,
                } && !is_array_wrapper_value(&coll, &self.struct_heap);

                if !uses_builtin_iterate && can_score_iterate_dynamic_candidates(&coll) {
                    let call_site_ip = self.ip - 1;
                    let arg_fingerprint = if let Some(state) = &state_opt {
                        self.call_site_arg_fingerprints(&[&coll, state])
                    } else {
                        self.call_site_arg_fingerprint(&coll)
                    };

                    let matched = if let Some(cached) = arg_fingerprint
                        .as_deref()
                        .and_then(|fp| self.lookup_call_site_inline_cache(call_site_ip, fp))
                    {
                        self.cached_iterate_candidate(cached, candidates)
                    } else {
                        // Find the matching iterate method by scoring the runtime
                        // argument cores (`coll` + optional `state`).
                        let mut dispatch_args = vec![coll.clone()];
                        if let Some(state) = &state_opt {
                            dispatch_args.push(state.clone());
                        }
                        let actual_cores: Vec<CoreType> = dispatch_args
                            .iter()
                            .map(|arg| {
                                let ty = self.dispatch_julia_type_for_value(arg);
                                crate::vm::dispatch_binding::runtime_actual_core_type(&ty)
                            })
                            .collect();

                        // L2 dispatch cache keyed on the interned arg-id sequence
                        // (Issue #9197 S3): reuse the L1 `arg_fingerprint` instead
                        // of joining `coll`/`state` type-name strings. An untracked
                        // value kind has no id, so the call skips L2 and re-resolves.
                        // Check dispatch cache first (Issue #2943, #3355)
                        if let Some(cached) = arg_fingerprint
                            .as_deref()
                            .and_then(|key| self.lookup_call_site_dispatch_cache(call_site_ip, key))
                        {
                            self.store_call_site_inline_cache(
                                call_site_ip,
                                arg_fingerprint.as_deref(),
                                cached,
                            );
                            self.cached_iterate_candidate(cached, candidates)
                        } else {
                            let dispatch_args: Vec<Value> = if let Some(ref state) = state_opt {
                                vec![coll.clone(), state.clone()]
                            } else {
                                vec![coll.clone()]
                            };
                            // Native arrays only score against user-defined
                            // candidates so the VM builtin iterator stays the
                            // default; structs score the full set (Issue #6638).
                            let scored = scored_iterate_candidates(
                                &coll,
                                candidates,
                                self.base_function_count,
                                &self.struct_heap,
                            );
                            // Issue #10879: fence the family-fallback resolvers
                            // (below) with the same origin-aware check the
                            // metadata scorer uses, before either sees a
                            // candidate whose owner was erased by the Base
                            // signature cache (Issue #10295).
                            let scored =
                                self.origin_safe_iterate_candidates(&scored, &dispatch_args);
                            let iterate_family_best = self
                                .resolve_iterate_struct_family_fallback(&scored, &actual_cores)
                                .or_else(|| {
                                    self.resolve_runtime_iterate_struct_family_fallback(
                                        &actual_cores,
                                        &dispatch_args,
                                    )
                                });
                            let best = if let Some(func_index) = iterate_family_best {
                                Some(func_index)
                            } else {
                                match self
                                    .find_best_method_index_from_candidates(&scored, &dispatch_args)
                                {
                                    Ok(Some(func_index)) => Some(func_index),
                                    Ok(None) => {
                                        // Shared structured scored dispatch fallback
                                        // (Issues #6336/#6502).
                                        match self
                                            .resolve_scored_family_fallback(&scored, &actual_cores)
                                        {
                                            Ok(opt) => opt,
                                            Err(err) => {
                                                self.raise(err)?;
                                                return Ok(DispatchAction::Continue);
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        self.raise(err)?;
                                        return Ok(DispatchAction::Continue);
                                    }
                                }
                            };
                            // Store in the L2 cache keyed by the interned arg-id
                            // sequence (Issue #9197 S3); untracked kinds are not cached.
                            let cache_val = best.unwrap_or(usize::MAX);
                            if let Some(key) = arg_fingerprint.as_deref() {
                                self.store_call_site_dispatch_cache(call_site_ip, key, cache_val);
                            }
                            self.store_call_site_inline_cache(
                                call_site_ip,
                                arg_fingerprint.as_deref(),
                                cache_val,
                            );
                            best
                        }
                    };

                    if let Some(func_index) = matched {
                        // Call the user-defined iterate method
                        let func = match self.get_function_cloned_or_raise(func_index)? {
                            Some(f) => f,
                            None => return Ok(DispatchAction::Continue),
                        };

                        let mut frame = self.acquire_frame(func.local_slot_count, Some(func_index));

                        // Bind type parameters from where clauses (Issue #2468)
                        {
                            let type_bind_args: Vec<Value> = if let Some(ref state) = state_opt {
                                vec![coll.clone(), state.clone()]
                            } else {
                                vec![coll.clone()]
                            };
                            self.bind_type_params(&func, &type_bind_args, &mut frame);
                        }

                        // Bind arguments to parameter slots
                        if let Some(slot) = func.param_slots.first() {
                            bind_value_to_slot(&mut frame, *slot, coll, &mut self.struct_heap);
                        }
                        if let Some(state) = state_opt {
                            if let Some(slot) = func.param_slots.get(1) {
                                bind_value_to_slot(&mut frame, *slot, state, &mut self.struct_heap);
                            }
                        }

                        bind_kwargs_defaults(
                            &func,
                            &mut frame,
                            &mut self.struct_heap,
                            &self.code,
                            &self.functions,
                            self.frames.first(),
                            &self.global_slot_map,
                        )?;

                        self.return_ips.push(self.ip);
                        self.try_push_call_frame(frame)?;
                        self.ip = func.entry;
                    } else {
                        if is_struct && !is_array_wrapper_value(&coll, &self.struct_heap) {
                            // No matching method found - error
                            // User-visible: user's struct type has no iterate method — triggered by for-loops over custom types
                            return Err(VmError::TypeError(format!(
                                "iterate: no method matching iterate(::{}{})",
                                self.get_type_name(&coll),
                                if *argc == 2 { ", ...)" } else { ")" }
                            )));
                        }

                        // Native VM collections can still use builtin iteration
                        // when no user/runtime method candidate matches.
                        if let Value::Generator(generator) = &coll {
                            if self
                                .start_lazy_generator_iterate_call(generator, state_opt.as_ref())?
                            {
                                return Ok(DispatchAction::Continue);
                            }
                        }
                        let result = if let Some(state) = state_opt {
                            self.iterate_next(&coll, &state)?
                        } else {
                            self.iterate_first(&coll)?
                        };
                        self.stack.push(result);
                    }
                } else {
                    // Not a struct or CartesianIndices - use builtin iterate
                    if let Value::Generator(generator) = &coll {
                        if self.start_lazy_generator_iterate_call(generator, state_opt.as_ref())? {
                            return Ok(DispatchAction::Continue);
                        }
                    }
                    let result = if let Some(state) = state_opt {
                        self.iterate_next(&coll, &state)?
                    } else {
                        self.iterate_first(&coll)?
                    };
                    self.stack.push(result);
                }
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_fallback_gate_key_ignores_redundant_single_slot_tuple() {
        let slot = CoreType::Struct {
            name: "QuadGK.BatchIntegrand".to_string(),
            params: vec![],
        };
        let gate = CoreType::Tuple(vec![slot.clone()]);

        assert_eq!(tier_fallback_gate_key_for_dedup(&slot, Some(&gate)), None);
    }

    #[test]
    fn tier_fallback_gate_key_keeps_non_redundant_tuple() {
        let slot = CoreType::Struct {
            name: "QuadGK.BatchIntegrand".to_string(),
            params: vec![],
        };
        let other = CoreType::Struct {
            name: "Other".to_string(),
            params: vec![],
        };
        let gate = CoreType::Tuple(vec![other]);

        assert_eq!(
            tier_fallback_gate_key_for_dedup(&slot, Some(&gate)),
            Some(gate)
        );
    }
}
