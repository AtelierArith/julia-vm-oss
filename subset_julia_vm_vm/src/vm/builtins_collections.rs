//! Collection-related builtin execution.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::builtins::BuiltinId;
use crate::rng::RngLike;
use crate::vm::value::is_native_array_value;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::type_utils::type_values_subtype;
use super::value::{
    array_wrapper_value_to_array_value, is_scalar_carrier, native_array_value_ref,
    GeneratorCallable, GeneratorValue, MemoryRefValue, Value,
};
use super::{StructInstance, Vm};

impl<R: RngLike> Vm<R> {
    pub(super) fn execute_builtin_collections(
        &mut self,
        builtin: &BuiltinId,
        _argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            BuiltinId::Length => {
                let val = self.stack.pop_value()?;
                if matches!(val, Value::Struct(_) | Value::StructRef(_)) {
                    let args = vec![val];
                    if let Some(func_index) =
                        self.find_best_method_index(&["length", "Base.length"], &args)
                    {
                        self.start_function_call(func_index, args)?;
                        return Ok(Some(()));
                    }
                    // Native fallback for a MemoryRef-backed `Array{T,N}` wrapper
                    // when no user/Base `length` method is available (e.g. a bare
                    // VM without Base loaded): count elements directly from the
                    // wrapper storage. Gated on the dispatch miss so a user
                    // `length` override still wins when present, and a no-op for
                    // Base-loaded programs where `length(::AbstractArray)` exists
                    // (Issue #6807).
                    if let Some(arr) =
                        array_wrapper_value_to_array_value(&args[0], &self.struct_heap)?
                    {
                        self.stack.push(Value::I64(arr.element_count() as i64));
                        return Ok(Some(()));
                    }
                    let type_name = self.get_type_name(&args[0]);
                    return Err(VmError::MethodError(format!(
                        "no method matching length({})",
                        type_name
                    )));
                }
                if let Value::Range(r) = &val {
                    self.stack.push(r.length_value());
                    return Ok(Some(()));
                }
                let len = match &val {
                    _ if is_native_array_value(&val) => match native_array_value_ref(&val) {
                        Some(arr) => arr.borrow().element_count() as i64,
                        None => 0,
                    },
                    Value::Tuple(items) => items.len() as i64,
                    // Core.SimpleVector length (Issue #4722).
                    Value::SimpleVector(items) => items.len() as i64,
                    Value::NamedTuple(nt) => nt.values.len() as i64,
                    Value::Str(s) => s.chars().count() as i64,
                    // Julia char segmentation, not WHATWG-lossy (Issue #8995).
                    Value::StrBytes(bytes) => {
                        crate::vm::value::julia_char_count(bytes.as_ref()) as i64
                    }
                    Value::Pairs(p) => p.data.values.len() as i64,
                    Value::Memory(mem) => mem.borrow().len() as i64,
                    // Issue #7964: flat static-array reps.
                    Value::StaticArray(sv) => sv.len() as i64,
                    Value::StaticArrayInline(sv) => sv.len() as i64,
                    // A FILTERED generator wraps its base iterator in a
                    // conceptual `Base.Iterators.Filter`, which upstream declares
                    // `IteratorSize(::Type{<:Filter}) == SizeUnknown()`. sjulia
                    // collapses `Generator(map, Filter(pred, iter))` into a single
                    // `Value::Generator` whose `iter` field is the *base*
                    // iterator and whose filter lives in `callable`, so we cannot
                    // let `length` fall through to the base length — that reports
                    // the UNFILTERED count (Issue #9320). Upstream's
                    // `length(g::Generator) = length(g.iter)` reaches
                    // `length(::Filter)`, which is undefined → MethodError. Mirror
                    // that error class here (not the base count, not a bespoke
                    // error string) so `try/catch e isa MethodError` matches.
                    //
                    // Deferred consumer retirement (Issue #9200 S6): this native
                    // `length(::Generator)` special case shadows the pure-Julia
                    // `length(g::Generator) = length(g.iter)` precisely because the
                    // collapsed filter lives in `callable`, not in `g.iter`. It can
                    // be retired only once a filtered generator carries a real
                    // `Iterators.Filter` in `g.iter`, so the pure-Julia method
                    // reaches the `SizeUnknown` MethodError on its own.
                    Value::Generator(g) if self.generator_is_filtered(g) => {
                        let filter_type = self.filtered_generator_iter_type_name(g);
                        return Err(VmError::MethodError(format!(
                            "no method matching length(::{})",
                            filter_type
                        )));
                    }
                    Value::Generator(g) => {
                        match self.native_iterator_length_for_generator_iter(g.iter.as_ref())? {
                            Some(len) => len,
                            None => {
                                return Err(VmError::TypeError(format!(
                                    "length not defined for Generator's underlying iterator {:?}",
                                    g.iter
                                )))
                            }
                        }
                    }
                    // Issue #4814 / #4871 / #4875: scalars in upstream Julia
                    // behave as 0-dimensional collections — `length(x) == 1`
                    // for every `Number` and `AbstractChar` subtype. The
                    // carrier predicate lives in `vm/value/predicates.rs`
                    // so this stays in lock-step with the `IndexLoad`
                    // scalar arm and any future scalar-aware builtin.
                    other if is_scalar_carrier(other) => 1,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "length not defined for {:?}",
                            val
                        )))
                    }
                };
                self.stack.push(Value::I64(len));
                Ok(Some(()))
            }
            BuiltinId::Eltype | BuiltinId::_Eltype => {
                let val = self.stack.pop_value()?;
                if matches!(builtin, BuiltinId::Eltype) {
                    if let Value::DataType(jt) = &val {
                        let element_type = self.datatype_eltype(jt);
                        if !matches!(element_type, crate::types::JuliaType::Any) {
                            self.stack.push(Value::DataType(Box::new(element_type)));
                            return Ok(Some(()));
                        }
                    }
                }
                if matches!(builtin, BuiltinId::Eltype) {
                    let direct_args = if matches!(
                        val,
                        Value::Struct(_) | Value::StructRef(_) | Value::DataType(_)
                    ) {
                        Some(vec![val.clone()])
                    } else {
                        None
                    };
                    if let Some(args) = direct_args {
                        if let Some(func_index) =
                            self.find_best_method_index(&["eltype", "Base.eltype"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(Some(()));
                        }
                    }
                    if matches!(val, Value::Struct(_) | Value::StructRef(_)) {
                        let args = vec![Value::DataType(Box::new(self.get_value_julia_type(&val)))];
                        if let Some(func_index) =
                            self.find_best_method_index(&["eltype", "Base.eltype"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(Some(()));
                        }
                    }
                }
                if matches!(builtin, BuiltinId::Eltype)
                    && matches!(val, Value::Struct(_) | Value::StructRef(_))
                {
                    self.stack
                        .push(Value::DataType(Box::new(crate::types::JuliaType::Any)));
                    return Ok(Some(()));
                }
                let element_type = match &val {
                    _ if is_native_array_value(&val) => match native_array_value_ref(&val) {
                        Some(arr) => {
                            let arr_borrow = arr.borrow();
                            self.array_value_declared_element_julia_type(&arr_borrow)
                        }
                        None => crate::types::JuliaType::Any,
                    },
                    Value::DataType(jt) if matches!(builtin, BuiltinId::Eltype) => {
                        self.datatype_eltype(jt)
                    }
                    Value::Memory(mem) if matches!(builtin, BuiltinId::Eltype) => {
                        let elem_type_name = mem.borrow().element_type().julia_type_name();
                        crate::types::JuliaType::from_name_or_struct(&elem_type_name)
                    }
                    Value::Tuple(t) if matches!(builtin, BuiltinId::Eltype) => {
                        if t.elements.is_empty() {
                            crate::types::JuliaType::Any
                        } else {
                            let first_type = t.elements[0].runtime_type();
                            let all_same =
                                t.elements.iter().all(|e| e.runtime_type() == first_type);
                            if all_same {
                                first_type
                            } else {
                                crate::types::JuliaType::Any
                            }
                        }
                    }
                    // Issue #5196: upstream `eltype(::Core.SimpleVector)` is always
                    // `Any` (it does not narrow to the common element type the way
                    // a Tuple does). Returning `Any` here keeps the `collect` path
                    // building a `Vector{Any}` regardless of element homogeneity.
                    Value::SimpleVector(_) if matches!(builtin, BuiltinId::Eltype) => {
                        crate::types::JuliaType::Any
                    }
                    Value::Bool(_)
                    | Value::I8(_)
                    | Value::I16(_)
                    | Value::I32(_)
                    | Value::I64(_)
                    | Value::I128(_)
                    | Value::U8(_)
                    | Value::U16(_)
                    | Value::U32(_)
                    | Value::U64(_)
                    | Value::U128(_)
                    | Value::F16(_)
                    | Value::F32(_)
                    | Value::F64(_)
                    | Value::BigInt(_)
                    | Value::BigFloat(_)
                        if matches!(builtin, BuiltinId::Eltype) =>
                    {
                        // Upstream Base defines eltype(::Type{T}) where T<:Number = T
                        // and the value fallback delegates through typeof(x). Generic
                        // sjulia call sites can reach this builtin fallback (Issue #4665).
                        val.runtime_type()
                    }
                    Value::Pairs(p)
                        if matches!(builtin, BuiltinId::Eltype | BuiltinId::_Eltype) =>
                    {
                        crate::types::JuliaType::Struct(format!(
                            "Pair{{Symbol, {}}}",
                            self.pairs_value_element_type_name(&p.data.values)
                        ))
                    }
                    Value::Range(r) if matches!(builtin, BuiltinId::Eltype) => {
                        // Issue #3550: respect the range's typed element tag.
                        match r.element_type {
                            crate::vm::value::RangeElementType::Default => {
                                if r.is_float {
                                    crate::types::JuliaType::Float64
                                } else {
                                    crate::types::JuliaType::Int64
                                }
                            }
                            crate::vm::value::RangeElementType::Int8 => {
                                crate::types::JuliaType::Int8
                            }
                            crate::vm::value::RangeElementType::Int16 => {
                                crate::types::JuliaType::Int16
                            }
                            crate::vm::value::RangeElementType::Int32 => {
                                crate::types::JuliaType::Int32
                            }
                            crate::vm::value::RangeElementType::Int64 => {
                                crate::types::JuliaType::Int64
                            }
                            crate::vm::value::RangeElementType::UInt8 => {
                                crate::types::JuliaType::UInt8
                            }
                            crate::vm::value::RangeElementType::UInt16 => {
                                crate::types::JuliaType::UInt16
                            }
                            crate::vm::value::RangeElementType::UInt32 => {
                                crate::types::JuliaType::UInt32
                            }
                            crate::vm::value::RangeElementType::UInt64 => {
                                crate::types::JuliaType::UInt64
                            }
                            crate::vm::value::RangeElementType::Float16 => {
                                crate::types::JuliaType::Float16
                            }
                            crate::vm::value::RangeElementType::Float32 => {
                                crate::types::JuliaType::Float32
                            }
                            crate::vm::value::RangeElementType::Float64 => {
                                crate::types::JuliaType::Float64
                            }
                            crate::vm::value::RangeElementType::Char => {
                                crate::types::JuliaType::Char
                            }
                            crate::vm::value::RangeElementType::BigInt => {
                                crate::types::JuliaType::BigInt
                            }
                        }
                    }
                    // Static arrays (StaticArrays.jl SVector/SMatrix/SArray):
                    // report the concrete element type from the flat carrier's
                    // type tag instead of widening to `Any`. Without this, a
                    // statically-`Any` static-array value reaching this builtin
                    // (e.g. `eltype(itr)` inside the generic `_collect`) produced
                    // `Vector{Any}` for `collect(SVector(1,2,3))` (Issue #8131).
                    Value::StaticArrayInline(sv)
                        if matches!(builtin, BuiltinId::Eltype | BuiltinId::_Eltype) =>
                    {
                        crate::types::JuliaType::from_name_or_struct(sv.tag.julia_name())
                    }
                    Value::StaticArray(sv)
                        if matches!(builtin, BuiltinId::Eltype | BuiltinId::_Eltype) =>
                    {
                        crate::types::JuliaType::from_name_or_struct(sv.elems.element_type_name())
                    }
                    Value::Str(_) | Value::StrBytes(_) => crate::types::JuliaType::Char,
                    _ => crate::types::JuliaType::Any,
                };
                self.stack.push(Value::DataType(Box::new(element_type)));
                Ok(Some(()))
            }
            BuiltinId::MemoryRefNew => {
                let _check_bounds = if _argc >= 3 {
                    matches!(self.stack.pop_value()?, Value::Bool(true))
                } else {
                    true
                };
                let index = if _argc >= 2 {
                    match self.stack.pop_value()? {
                        Value::I64(i) if i >= 1 => usize::try_from(i).map_err(|_| {
                            VmError::TypeError(format!(
                                "memoryref: index out of range for usize, got {}",
                                i
                            ))
                        })?,
                        Value::U64(i) if i >= 1 => usize::try_from(i).map_err(|_| {
                            VmError::TypeError(format!(
                                "memoryref: index out of range for usize, got {}",
                                i
                            ))
                        })?,
                        other => {
                            return Err(VmError::TypeError(format!(
                                "memoryref: expected positive integer index, got {:?}",
                                other
                            )))
                        }
                    }
                } else {
                    1
                };
                let target = self.stack.pop_value()?;
                let memref = match target {
                    Value::Memory(mem) => MemoryRefValue::new(mem, index)?,
                    Value::MemoryRef(memref) => {
                        let parent = memref.parent();
                        let new_index = memref.memory_index().saturating_add(index - 1);
                        MemoryRefValue::new(parent, new_index)?
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "memoryref: expected Memory or MemoryRef, got {:?}",
                            other
                        )))
                    }
                };
                self.stack.push(Value::MemoryRef(Box::new(memref)));
                Ok(Some(()))
            }
            BuiltinId::MemoryRefGet => {
                for _ in 1.._argc {
                    let _ = self.stack.pop_value()?;
                }
                let memref = match self.stack.pop_value()? {
                    Value::MemoryRef(memref) => memref,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "memoryrefget: expected MemoryRef, got {:?}",
                            other
                        )))
                    }
                };
                self.stack.push(memref.get(1)?);
                Ok(Some(()))
            }
            BuiltinId::MemoryRefSet => {
                for _ in 2.._argc {
                    let _ = self.stack.pop_value()?;
                }
                let value = self.stack.pop_value()?;
                let memref = match self.stack.pop_value()? {
                    Value::MemoryRef(memref) => memref,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "memoryrefset!: expected MemoryRef, got {:?}",
                            other
                        )))
                    }
                };
                // Issue #9198 S4: a contiguous `StructInlineF64` Memory packs the
                // struct's fields; resolve a heap `StructRef` to an inline
                // `Value::Struct` here (the low-level `memoryrefset!` primitive
                // reached by Pure-Julia `push!`/`setindex!`).
                let value = self.resolve_struct_ref_for_inline_store(&memref.parent(), value);
                memref.set(1, value)?;
                self.stack.push(Value::MemoryRef(memref));
                Ok(Some(()))
            }
            BuiltinId::MemoryRefOffset => {
                if _argc != 1 {
                    return Err(VmError::TypeError(
                        "memoryrefoffset requires exactly 1 argument".to_string(),
                    ));
                }
                let memref = match self.stack.pop_value()? {
                    Value::MemoryRef(memref) => memref,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "memoryrefoffset: expected MemoryRef, got {:?}",
                            other
                        )))
                    }
                };
                self.stack.push(Value::I64(memref.memory_index() as i64));
                Ok(Some(()))
            }
            BuiltinId::MemoryRefParent => {
                if _argc != 1 {
                    return Err(VmError::TypeError(
                        "memoryrefparent requires exactly 1 argument".to_string(),
                    ));
                }
                let memref = match self.stack.pop_value()? {
                    Value::MemoryRef(memref) => memref,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "memoryrefparent: expected MemoryRef, got {:?}",
                            other
                        )))
                    }
                };
                self.stack.push(Value::Memory(memref.parent()));
                Ok(Some(()))
            }
            _ => Ok(None),
        }
    }

    /// Build the `Base.Iterators.Filter{F, I}` type name reported by the
    /// MethodError when `length` is applied to a filtered generator
    /// (Issue #9320). sjulia collapses `Generator(map, Filter(pred, iter))`
    /// into one `Value::Generator` — the base iterator in `g.iter`, the filter
    /// predicate in `g.callable` — so we reconstruct the conceptual `Filter`
    /// type name here to mirror upstream's `no method matching length(::Filter)`
    /// error (upstream `length(g::Generator) = length(g.iter)` reaches the
    /// undefined `length(::Filter)` because `IteratorSize(::Filter)` is
    /// `SizeUnknown()`).
    pub(in crate::vm) fn filtered_generator_iter_type_name(&self, g: &GeneratorValue) -> String {
        // Issue #9200 S3: the desugared shape wraps a REAL `Iterators.Filter` in
        // `g.iter`, so report the filter value's own type directly (built from
        // its `flt` / `itr` fields — `get_type_name` unwraps the filter via the
        // single-array-field wrapper heuristic and loses the `Filter` spelling).
        if let Some(name) = self.filter_value_type_name(g.iter.as_ref()) {
            return name;
        }
        let pred_type = match &g.callable {
            GeneratorCallable::FilteredFunctionIndex {
                predicate_func_index,
                ..
            } => self
                .function_index_singleton_type_name(*predicate_func_index)
                .unwrap_or_else(|| "Function".to_string()),
            GeneratorCallable::FilteredRuntimeValue { predicate, .. } => match predicate.as_ref() {
                Value::Function(function) => function.singleton_type_name(),
                other => self.get_type_name(other),
            },
            // Non-filtered callables never reach this helper (the caller guards
            // on the filtered variants); keep a benign fallback over a panic.
            _ => "Function".to_string(),
        };
        let iter_type = self.get_type_name(g.iter.as_ref());
        format!("Base.Iterators.Filter{{{}, {}}}", pred_type, iter_type)
    }

    /// Whether `val` is a real pure-Julia `Iterators.Filter` struct value
    /// (Issue #9200 S3). Used to decide the size/length/IteratorSize traits of a
    /// generator that wraps a `Filter` in `g.iter`.
    pub(in crate::vm) fn value_is_filter_struct(&self, val: &Value) -> bool {
        self.filter_struct_instance(val).is_some()
    }

    fn filter_struct_instance<'a>(&'a self, val: &'a Value) -> Option<&'a StructInstance> {
        let s = match val {
            Value::Struct(s) => s,
            Value::StructRef(idx) => self.struct_heap.get(*idx)?,
            _ => return None,
        };
        struct_name_is_filter(&s.struct_name).then_some(s)
    }

    /// Type name of a real `Iterators.Filter` value, spelled
    /// `Base.Iterators.Filter{typeof(pred), typeof(iter)}` from its `flt`/`itr`
    /// fields — mirroring upstream's parametric `Filter{F, I}` (Issue #9200 S3).
    /// `None` for non-`Filter` values.
    pub(in crate::vm) fn filter_value_type_name(&self, val: &Value) -> Option<String> {
        let s = self.filter_struct_instance(val)?;
        let pred_type = match s.values.first() {
            Some(Value::Function(function)) => function.singleton_type_name(),
            Some(other) => self.get_type_name(other),
            None => "Function".to_string(),
        };
        let iter_type = s
            .values
            .get(1)
            .map(|v| self.get_type_name(v))
            .unwrap_or_else(|| "Any".to_string());
        Some(format!(
            "Base.Iterators.Filter{{{}, {}}}",
            pred_type, iter_type
        ))
    }

    /// Whether a generator is a FILTERED generator for the purpose of the
    /// size/length/IteratorSize traits (Issue #9200 S3 / #9320 / #9379). True for
    /// either the desugared real-`Filter`-in-`g.iter` shape (S3) or the collapsed
    /// `Filtered*` callable variants still emitted by the tuple-destructuring lift
    /// path. Upstream reports `IteratorSize(::Filter) == SizeUnknown()`, so both
    /// shapes yield `SizeUnknown()` / a `length`/`size` MethodError.
    pub(in crate::vm) fn generator_is_filtered(&self, g: &GeneratorValue) -> bool {
        matches!(
            g.callable,
            GeneratorCallable::FilteredFunctionIndex { .. }
                | GeneratorCallable::FilteredRuntimeValue { .. }
        ) || self.value_is_filter_struct(g.iter.as_ref())
    }

    fn datatype_eltype(&self, jt: &crate::types::JuliaType) -> crate::types::JuliaType {
        let static_eltype = datatype_eltype(jt);
        if !matches!(static_eltype, crate::types::JuliaType::Any) {
            return static_eltype;
        }
        if self.check_subtype(jt.name().as_ref(), "Number") {
            return jt.clone();
        }
        crate::types::JuliaType::Any
    }
}

/// Whether `name` is the pure-Julia `Iterators.Filter` struct name (Issue #9200
/// S3), tolerating both the bare `Filter` spelling and a parametric
/// `Filter{...}` / `Base.Iterators.Filter{...}` rendering.
pub(in crate::vm) fn struct_name_is_filter(name: &str) -> bool {
    let base = name.split('{').next().unwrap_or(name);
    let base = base.rsplit('.').next().unwrap_or(base);
    base == "Filter"
}

pub(in crate::vm) fn datatype_eltype(jt: &crate::types::JuliaType) -> crate::types::JuliaType {
    use crate::types::JuliaType;

    match jt {
        JuliaType::VectorOf(elem) | JuliaType::MatrixOf(elem) => {
            resolve_typevar_bound(elem.as_ref(), jt)
        }
        JuliaType::UnionAll { body, .. } => datatype_eltype(body),
        JuliaType::String => JuliaType::Char,
        numeric if type_values_subtype(numeric, &JuliaType::Number) => numeric.clone(),
        JuliaType::Struct(name) => parametric_collection_eltype(name).unwrap_or(JuliaType::Any),
        _ => JuliaType::Any,
    }
}

fn resolve_typevar_bound(
    candidate: &crate::types::JuliaType,
    owner: &crate::types::JuliaType,
) -> crate::types::JuliaType {
    use crate::types::JuliaType;

    match (candidate, owner) {
        (
            JuliaType::TypeVar(name, candidate_bound),
            JuliaType::UnionAll {
                var,
                lower_bound: _,
                bound,
                body: _,
            },
        ) if name == var => bound
            .as_ref()
            .map(|b| b.as_str())
            .or(candidate_bound.as_deref())
            .map(JuliaType::from_name_or_struct)
            .unwrap_or(JuliaType::Any),
        (JuliaType::TypeVar(_, bound), _) => bound
            .as_deref()
            .map(JuliaType::from_name_or_struct)
            .unwrap_or(JuliaType::Any),
        _ => candidate.clone(),
    }
}

fn parametric_collection_eltype(type_name: &str) -> Option<crate::types::JuliaType> {
    let open = type_name.find('{')?;
    let base = &type_name[..open];
    if !matches!(base, "Vector" | "Matrix" | "Array" | "Set") {
        return None;
    }
    parse_first_type_parameter(type_name)
}

fn parse_first_type_parameter(type_name: &str) -> Option<crate::types::JuliaType> {
    let open = type_name.find('{')?;
    let close = type_name.rfind('}')?;
    if close <= open + 1 {
        return None;
    }

    let inner = &type_name[open + 1..close];
    let first = split_top_level_params(inner).into_iter().next()?;
    let trimmed = first.trim();
    if trimmed.is_empty() || trimmed.chars().all(|c| c.is_ascii_digit()) {
        None
    } else {
        Some(crate::types::JuliaType::from_name_or_struct(trimmed))
    }
}

fn split_top_level_params(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (idx, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(s[start..idx].to_string());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(s[start..].to_string());
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::StableRng;
    use crate::types::{JuliaType, StructHierarchy};
    use crate::vm::{FunctionInfo, StructInstance, ValueType};
    use std::rc::Rc;

    fn add_eltype_method(
        vm: &mut Vm<StableRng>,
        param_julia_type: JuliaType,
        entry: usize,
    ) -> usize {
        let idx = vm.functions.len();
        vm.functions.push(Rc::new(FunctionInfo {
            name: "Base.eltype".to_string(),
            params: vec![("x".to_string(), ValueType::DataType)],
            kwparams: vec![],
            entry,
            return_type: ValueType::DataType,
            return_julia_type: None,
            is_base_extension: false,
            is_generated: false,
            is_lowering_helper: false,
            definition_order: 0,
            min_world: 1,
            type_params: vec![],
            param_julia_types: vec![param_julia_type],
            code_start: entry,
            code_end: entry,
            slot_names: vec!["x".to_string()],
            slot_types: vec![Some(crate::vm::VarTypeTag::Any)],
            local_slot_count: 1,
            param_slots: vec![0],
            vararg_param_index: None,
            vararg_fixed_count: None,
            inlining_meta: 0,
            constprop_meta: 0,
            nospecialize_meta: 0,
            propagate_inbounds_meta: false,
            nospecializeinfer_meta: false,
            purity_meta: 0,
            direct_return_type_param: None,
            // Builtin stub FunctionInfo: no source line (Issue #5125).
            def_line: 0,
            suppress_short_name_alias: false,
            shared_plan: None,
        }));
        vm.function_name_index
            .entry("Base.eltype".to_string())
            .or_default()
            .push(idx);
        idx
    }

    #[test]
    fn eltype_type_object_dispatches_before_datatype_fallback() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        let method_idx = add_eltype_method(
            &mut vm,
            JuliaType::TypeOf(Box::new(JuliaType::Struct("MyIter".to_string()))),
            17,
        );

        vm.stack.push(Value::DataType(Box::new(JuliaType::Struct(
            "MyIter".to_string(),
        ))));

        assert!(matches!(
            vm.execute_builtin_collections(&BuiltinId::Eltype, 1),
            Ok(Some(()))
        ));
        assert_eq!(vm.ip, 17);
        assert_eq!(
            vm.frames.last().and_then(|frame| frame.func_index),
            Some(method_idx)
        );
        assert!(vm.stack.is_empty());
    }

    #[test]
    fn eltype_struct_value_falls_back_to_type_object_method() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        let method_idx = add_eltype_method(
            &mut vm,
            JuliaType::TypeOf(Box::new(JuliaType::Struct("MyIter".to_string()))),
            23,
        );

        vm.stack.push(Value::Struct(StructInstance::with_name(
            0,
            "MyIter".to_string(),
            vec![],
        )));

        assert!(matches!(
            vm.execute_builtin_collections(&BuiltinId::Eltype, 1),
            Ok(Some(()))
        ));
        assert_eq!(vm.ip, 23);
        assert_eq!(
            vm.frames.last().and_then(|frame| frame.func_index),
            Some(method_idx)
        );
        assert!(vm.stack.is_empty());
    }

    #[test]
    fn eltype_parametric_numeric_type_object_returns_itself() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Complex", Some("Number".to_string()), vec!["T".to_string()]);
        hierarchy.insert("Rational", Some("Real".to_string()), vec!["T".to_string()]);
        vm.struct_hierarchy = hierarchy;
        assert_eq!(
            vm.datatype_eltype(&JuliaType::Struct("Complex{Float64}".to_string())),
            JuliaType::Struct("Complex{Float64}".to_string())
        );
        assert_eq!(
            vm.datatype_eltype(&JuliaType::Struct("Rational{Int64}".to_string())),
            JuliaType::Struct("Rational{Int64}".to_string())
        );
    }

    #[test]
    fn parametric_collection_eltype_reads_first_type_parameter() {
        assert_eq!(
            parametric_collection_eltype("Vector{Int64}"),
            Some(JuliaType::Int64)
        );
        assert_eq!(
            parametric_collection_eltype("Matrix{Float64}"),
            Some(JuliaType::Float64)
        );
        assert_eq!(
            parametric_collection_eltype("Array{Float64, 2}"),
            Some(JuliaType::Float64)
        );
        assert_eq!(
            parametric_collection_eltype("Set{String}"),
            Some(JuliaType::String)
        );
    }

    #[test]
    fn parametric_collection_eltype_preserves_current_name_matching() {
        assert_eq!(parametric_collection_eltype("Array{2}"), None);
        assert_eq!(parametric_collection_eltype("Complex{Float64}"), None);
        assert_eq!(parametric_collection_eltype("Base.Vector{Int64}"), None);
    }

    #[test]
    fn datatype_eltype_uses_parametric_collection_type_names() {
        assert_eq!(
            datatype_eltype(&JuliaType::Struct("Vector{Int64}".to_string())),
            JuliaType::Int64
        );
        assert_eq!(
            datatype_eltype(&JuliaType::Struct("Array{Float64, 2}".to_string())),
            JuliaType::Float64
        );
        assert_eq!(
            datatype_eltype(&JuliaType::Struct("Set{String}".to_string())),
            JuliaType::String
        );
    }
}
