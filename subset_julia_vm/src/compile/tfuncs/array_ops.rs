//! Transfer functions for array operations.
//!
//! This module implements type inference for Julia's array operations,
//! including indexing, length, and construction.

use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::inference_core::{CorePrimitive, CoreType};
use std::collections::BTreeSet;

/// Transfer function for `getindex` (array indexing: `arr[i]`).
///
/// Type rules:
/// - Array{T}[Int] → T
/// - Tuple{T1, T2, ...}[Int] → Union of element types (conservative)
///
/// # Examples
/// ```text
/// getindex(Array{Int64}, Int64) → Int64
/// getindex(Array{Float64}, Int64) → Float64
/// ```
pub fn tfunc_getindex(args: &[LatticeType]) -> LatticeType {
    if args.len() < 2 {
        return LatticeType::Top;
    }

    match &args[0] {
        // Array{T}[i] → T
        LatticeType::Concrete(ConcreteType::Array { element, .. }) => match &**element {
            ConcreteType::UnionOf(types) if types.is_empty() => {
                LatticeType::Concrete(ConcreteType::Core(CoreType::Any))
            }
            ConcreteType::UnionOf(types) => {
                LatticeType::Union(types.iter().cloned().collect::<BTreeSet<_>>())
            }
            other => LatticeType::Concrete(other.clone()),
        },

        // Tuple{T1, T2, ...}[i] → Union of element types (conservative)
        LatticeType::Concrete(ConcreteType::Tuple { elements }) => {
            if elements.is_empty() {
                LatticeType::Bottom
            } else if elements.len() == 1 {
                LatticeType::Concrete(elements[0].clone())
            } else {
                // Join all element types
                let mut result = LatticeType::Concrete(elements[0].clone());
                for elem in &elements[1..] {
                    result = result.join(&LatticeType::Concrete(elem.clone()));
                }
                result
            }
        }

        // Issue #6601: `String[i]` (scalar integer index) yields a `Char`.
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)))
        }

        // Unknown collection type
        _ => LatticeType::Top,
    }
}

/// Transfer function for `setindex!` (array assignment: `arr[i] = val`).
///
/// Julia rule: `setindex!(collection, value, indices...) → collection`
///
/// # Examples
/// ```text
/// setindex!(Array{Int64}, Int64, Int64) → Array{Int64}
/// ```
pub fn tfunc_setindex(args: &[LatticeType]) -> LatticeType {
    if args.len() < 3 {
        return LatticeType::Top;
    }

    // setindex! returns the mutated collection (args[0]), not the value (Issue #3477)
    args[0].clone()
}

/// Transfer function for `length` (array/tuple/string length).
///
/// Type rules:
/// - length(Array{T}) → Int64
/// - length(Tuple) → Int64
/// - length(String) → Int64
///
/// # Examples
/// ```text
/// length(Array{Int64}) → Int64
/// length(Tuple{Int64, Float64}) → Int64
/// ```
pub fn tfunc_length(args: &[LatticeType]) -> LatticeType {
    use crate::compile::lattice::types::ConstValue;

    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        // Issue #5142: a fixed-arity tuple has a statically known length, so
        // propagate it as a constant. Mirrors upstream `nfields_tfunc`, which
        // returns `Const(length(x.types))` for concrete tuple/struct types.
        // This enables constant folding, branch elimination, and `Val(N)`
        // specialization downstream.
        LatticeType::Concrete(ConcreteType::Tuple { elements }) => {
            LatticeType::Const(ConstValue::Int64(elements.len() as i64))
        }

        // A tuple with a variadic tail (`Tuple{T..., Vararg{Tail}}`) has a
        // length that is only known at run time, so widen to Int64 rather than
        // a constant.
        LatticeType::Concrete(ConcreteType::TupleVararg { .. }) => LatticeType::Concrete(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        ),

        // length always returns Int64 for arrays and strings
        LatticeType::Concrete(ConcreteType::Array { .. })
        | LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            )))
        }

        // Unknown type
        _ => LatticeType::Top,
    }
}

/// Transfer function for `push!` (append element to array).
///
/// Returns the modified array.
///
/// # Examples
/// ```text
/// push!(Array{Int64}, Int64) → Array{Int64}
/// ```
pub fn tfunc_push(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // push! returns the array
    args[0].clone()
}

/// Transfer function for `pop!` (remove last element from array).
///
/// Returns the removed element.
///
/// # Examples
/// ```text
/// pop!(Array{Int64}) → Int64
/// ```
pub fn tfunc_pop(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Array { element, .. }) => {
            LatticeType::Concrete(*element.clone())
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `first` (get first element).
///
/// # Examples
/// ```text
/// first(Array{Int64}) → Int64
/// first(Tuple{Int64, Float64}) → Int64
/// ```
pub fn tfunc_first(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Array { element, .. }) => {
            LatticeType::Concrete(*element.clone())
        }
        LatticeType::Concrete(ConcreteType::Tuple { elements }) => {
            if let Some(first_elem) = elements.first() {
                LatticeType::Concrete(first_elem.clone())
            } else {
                LatticeType::Bottom
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `last` (get last element).
pub fn tfunc_last(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Array { element, .. }) => {
            LatticeType::Concrete(*element.clone())
        }
        LatticeType::Concrete(ConcreteType::Tuple { elements }) => {
            if let Some(last_elem) = elements.last() {
                LatticeType::Concrete(last_elem.clone())
            } else {
                LatticeType::Bottom
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `size` (get array dimensions).
///
/// Returns a Tuple of Int64s.
///
/// # Examples
/// ```text
/// size(Array{Int64}) → Tuple{Int64}
/// ```
pub fn tfunc_size(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Array { .. })
        | LatticeType::Concrete(ConcreteType::Tuple { .. })
        | LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))) => {
            if args.len() >= 2 {
                // size(arr, dim) -> Int64
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                )))
            } else {
                // size(arr) -> Tuple{Int64, ...}; conservatively single Int64
                LatticeType::Concrete(ConcreteType::Tuple {
                    elements: vec![ConcreteType::Core(CoreType::Primitive(
                        CorePrimitive::Int64,
                    ))],
                })
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `map` (apply function to each element).
///
/// Type rules:
/// - map(f, Array{T}) → Array{U} where U is the return type of f
/// - Simplified implementation: returns Array with same element type as input
///   (more precise inference would require analyzing the function f)
///
/// # Examples
/// ```text
/// map(x -> x + 1, Array{Int64}) → Array{Int64} (simplified)
/// ```
pub fn tfunc_map(args: &[LatticeType]) -> LatticeType {
    if args.len() < 2 {
        return LatticeType::Top;
    }

    // map(f, arr) - first argument is the function, second is the array
    let fn_name = match &args[0] {
        LatticeType::Concrete(ConcreteType::Function { name }) => Some(name.as_str()),
        LatticeType::Concrete(ConcreteType::DataType { name }) => Some(name.as_str()),
        _ => None,
    };

    let elem_from_fn = fn_name.and_then(infer_result_for_type_converter);

    match &args[1] {
        LatticeType::Concrete(ConcreteType::Array { element, .. }) => {
            // Use function return type when known; otherwise preserve input element type (Issue #3480)
            let result_elem = if let Some(out_elem) = elem_from_fn {
                Box::new(out_elem)
            } else {
                element.clone()
            };
            LatticeType::Concrete(ConcreteType::Array {
                element: result_elem,
                ndims: None,
            })
        }
        _ => LatticeType::Top,
    }
}

/// Map well-known type-converter function names to their output ConcreteType.
/// Used by tfunc_map and tfunc_broadcast to infer element types (Issue #3480).
fn infer_result_for_type_converter(name: &str) -> Option<ConcreteType> {
    match name {
        "Float64" | "float64" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        ))),
        "Float32" | "float32" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float32,
        ))),
        "Float16" | "float16" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float16,
        ))),
        "Int64" | "int64" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
        "Int32" | "int32" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int32,
        ))),
        "Int16" | "int16" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int16,
        ))),
        "Int8" | "int8" => Some(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8))),
        "UInt64" | "uint64" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt64,
        ))),
        "UInt32" | "uint32" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt32,
        ))),
        "UInt16" | "uint16" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt16,
        ))),
        "UInt8" | "uint8" => Some(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt8,
        ))),
        "Bool" | "bool" => Some(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        "abs" | "sign" | "ceil" | "floor" | "round" | "trunc" => None, // depends on input
        _ => None,
    }
}

/// Transfer function for `filter` (select elements satisfying predicate).
///
/// Type rules:
/// - filter(f, Array{T}) → Array{T} (same element type as input)
/// - filter(f, Dict{K,V}) → Dict{K,V} (same key/value types as input)
///
/// # Examples
/// ```text
/// filter(x -> x > 0, Array{Int64}) → Array{Int64}
/// ```
pub fn tfunc_filter(args: &[LatticeType]) -> LatticeType {
    if args.len() < 2 {
        return LatticeType::Top;
    }

    // filter(f, arr) - second argument is the array
    match &args[1] {
        LatticeType::Concrete(ConcreteType::Array { element, .. }) => {
            // filter returns an array with the same element type as the input
            LatticeType::Concrete(ConcreteType::Array {
                element: element.clone(),
                ndims: None,
            })
        }
        LatticeType::Concrete(ConcreteType::Dict { key, value }) => {
            LatticeType::Concrete(ConcreteType::Dict {
                key: key.clone(),
                value: value.clone(),
            })
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `reduce` (reduce array with binary operation).
///
/// Type rules:
/// - reduce(op, Array{T}) → T (element type of array)
/// - reduce(op, Array{T}, init) → Union{T, typeof(init)} (conservative)
///
/// # Examples
/// ```text
/// reduce(+, Array{Int64}) → Int64
/// reduce(+, Array{Int64}, 0) → Int64
/// ```
pub fn tfunc_reduce(args: &[LatticeType]) -> LatticeType {
    if args.len() < 2 {
        return LatticeType::Top;
    }

    // reduce(op, arr) or reduce(op, arr, init)
    match &args[1] {
        LatticeType::Concrete(ConcreteType::Array { element, .. }) => {
            if args.len() >= 3 {
                // With init value, join element type with init type
                LatticeType::Concrete(*element.clone()).join(&args[2])
            } else {
                LatticeType::Concrete(*element.clone())
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `foldl` (left fold with binary operation).
///
/// Type rules:
/// - foldl(op, Array{T}, init) → similar to reduce
///
/// # Examples
/// ```text
/// foldl(+, Array{Int64}, 0) → Int64
/// ```
pub fn tfunc_foldl(args: &[LatticeType]) -> LatticeType {
    tfunc_reduce(args)
}

/// Transfer function for `foldr` (right fold with binary operation).
///
/// Type rules:
/// - foldr(op, Array{T}, init) → similar to reduce
///
/// # Examples
/// ```text
/// foldr(+, Array{Int64}, 0) → Int64
/// ```
pub fn tfunc_foldr(args: &[LatticeType]) -> LatticeType {
    tfunc_reduce(args)
}

/// Apply Julia's reduction widening rules for sum/prod element types. (Issue #3478)
/// Bool and small integers widen to Int64/UInt64; floats preserve their type.
fn sum_result_type(elem: &ConcreteType) -> ConcreteType {
    match elem {
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))
        | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8))
        | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16))
        | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32))
        | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8))
        | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16))
        | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32))
        | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64))
        }
        // Floats preserve their type
        other => other.clone(),
    }
}

/// Transfer function for `sum` (sum all elements).
///
/// Julia rule: Bool/small-int element types widen; floats preserve. (Issue #3478)
pub fn tfunc_sum(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Array { element, .. }) => {
            LatticeType::Concrete(sum_result_type(element))
        }
        LatticeType::Concrete(ConcreteType::Range { element }) => {
            LatticeType::Concrete(sum_result_type(element))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `prod` (product of all elements).
///
/// Type rules:
/// - prod(Array{Int}) → Int64
/// - prod(Array{Float}) → Float64
///
/// # Examples
/// ```text
/// prod(Array{Int64}) → Int64
/// prod(Array{Float64}) → Float64
/// ```
pub fn tfunc_prod(args: &[LatticeType]) -> LatticeType {
    tfunc_sum(args)
}

/// Transfer function for `maximum` (find maximum element).
///
/// Type rules:
/// - maximum(Array{T}) → T
///
/// # Examples
/// ```text
/// maximum(Array{Int64}) → Int64
/// maximum(Array{Float64}) → Float64
/// ```
pub fn tfunc_maximum(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Array { element, .. }) => {
            LatticeType::Concrete(*element.clone())
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `minimum` (find minimum element).
///
/// Type rules:
/// - minimum(Array{T}) → T
///
/// # Examples
/// ```text
/// minimum(Array{Int64}) → Int64
/// minimum(Array{Float64}) → Float64
/// ```
pub fn tfunc_minimum(args: &[LatticeType]) -> LatticeType {
    tfunc_maximum(args)
}

/// Transfer function for `any` (check if any element satisfies predicate).
///
/// Type rules:
/// - any(f, Array{T}) → Bool
/// - any(Array{Bool}) → Bool
///
/// # Examples
/// ```text
/// any(x -> x > 0, Array{Int64}) → Bool
/// any(Array{Bool}) → Bool
/// ```
pub fn tfunc_any(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
}

/// Transfer function for `all` (check if all elements satisfy predicate).
///
/// Type rules:
/// - all(f, Array{T}) → Bool
/// - all(Array{Bool}) → Bool
///
/// # Examples
/// ```text
/// all(x -> x > 0, Array{Int64}) → Bool
/// all(Array{Bool}) → Bool
/// ```
pub fn tfunc_all(args: &[LatticeType]) -> LatticeType {
    tfunc_any(args)
}

/// Transfer function for `collect` (materialize iterator to array).
///
/// Type rules:
/// - collect(Range{T}) → Array{T}
/// - collect(Generator{T}) → Array{T}
///
/// # Examples
/// ```text
/// collect(1:10) → Array{Int64}
/// collect(x for x in 1:10) → Array{Int64}
/// ```
pub fn tfunc_collect(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Range { element }) => {
            LatticeType::Concrete(ConcreteType::Array {
                element: element.clone(),
                ndims: None,
            })
        }
        LatticeType::Concrete(ConcreteType::Generator { element }) => {
            LatticeType::Concrete(ConcreteType::Array {
                element: element.clone(),
                ndims: None,
            })
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for colon operator `:` (range construction).
///
/// Type rules:
/// - :(start, stop) → Range{T} where T is the join of start and stop types
/// - :(start, step, stop) → Range{T} where T is the join of all three types
///
/// # Examples
/// ```text
/// :(1, 10) → Range{Int64}
/// :(1.0, 10.0) → Range{Float64}
/// :(1, 2, 10) → Range{Int64}
/// ```
pub fn tfunc_colon(args: &[LatticeType]) -> LatticeType {
    if args.len() < 2 {
        return LatticeType::Top;
    }

    // Join all argument types to determine element type
    let mut element_ty = args[0].clone();
    for arg in &args[1..] {
        element_ty = element_ty.join(arg);
    }

    // Return Range{element_type}
    match element_ty {
        LatticeType::Concrete(ct) => LatticeType::Concrete(ConcreteType::Range {
            element: Box::new(ct),
        }),
        // If element type is not concrete (e.g., Union or Top), default to Int64
        _ => LatticeType::Concrete(ConcreteType::Range {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        }),
    }
}

/// Transfer function for `range` function (explicit range construction).
///
/// Type rules:
/// - range(start, stop) → Range{T}
/// - range(start, stop, length) → Range{T}
///
/// # Examples
/// ```text
/// range(1, 10) → Range{Int64}
/// range(1.0, 10.0, length=100) → Range{Float64}
/// ```
pub fn tfunc_range(args: &[LatticeType]) -> LatticeType {
    // Same logic as colon operator
    tfunc_colon(args)
}

// ============================================================================
// Extended Array Mutation Functions
// ============================================================================

/// Transfer function for `append!` (append elements from another collection).
///
/// Type rules:
/// - append!(Array{T}, iterable) → Array{T}
///
/// # Examples
/// ```text
/// append!(Array{Int64}, Array{Int64}) → Array{Int64}
/// append!(Array{Int64}, Range{Int64}) → Array{Int64}
/// ```
pub fn tfunc_append(args: &[LatticeType]) -> LatticeType {
    if args.len() < 2 {
        return LatticeType::Top;
    }

    // append! returns the modified array (first argument)
    args[0].clone()
}

/// Transfer function for `prepend!` (prepend elements from another collection).
///
/// Type rules:
/// - prepend!(Array{T}, iterable) → Array{T}
///
/// # Examples
/// ```text
/// prepend!(Array{Int64}, Array{Int64}) → Array{Int64}
/// ```
pub fn tfunc_prepend(args: &[LatticeType]) -> LatticeType {
    if args.len() < 2 {
        return LatticeType::Top;
    }

    // prepend! returns the modified array (first argument)
    args[0].clone()
}

/// Transfer function for `insert!` (insert element at position).
///
/// Type rules:
/// - insert!(Array{T}, index, value) → Array{T}
///
/// # Examples
/// ```text
/// insert!(Array{Int64}, 1, 42) → Array{Int64}
/// ```
pub fn tfunc_insert(args: &[LatticeType]) -> LatticeType {
    if args.len() < 3 {
        return LatticeType::Top;
    }

    // insert! returns the modified array (first argument)
    args[0].clone()
}

/// Transfer function for `deleteat!` (delete element at position).
///
/// Type rules:
/// - deleteat!(Array{T}, index) → Array{T}
/// - deleteat!(Array{T}, indices) → Array{T}
///
/// # Examples
/// ```text
/// deleteat!(Array{Int64}, 1) → Array{Int64}
/// deleteat!(Array{Int64}, [1, 2, 3]) → Array{Int64}
/// ```
pub fn tfunc_deleteat(args: &[LatticeType]) -> LatticeType {
    if args.len() < 2 {
        return LatticeType::Top;
    }

    // deleteat! returns the modified array (first argument)
    args[0].clone()
}

/// Transfer function for `popfirst!` (remove and return first element).
///
/// Type rules:
/// - popfirst!(Array{T}) → T
///
/// # Examples
/// ```text
/// popfirst!(Array{Int64}) → Int64
/// ```
pub fn tfunc_popfirst(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Array { element, .. }) => {
            LatticeType::Concrete(*element.clone())
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `pushfirst!` (add element to front).
///
/// Type rules:
/// - pushfirst!(Array{T}, value) → Array{T}
///
/// # Examples
/// ```text
/// pushfirst!(Array{Int64}, 42) → Array{Int64}
/// ```
pub fn tfunc_pushfirst(args: &[LatticeType]) -> LatticeType {
    if args.len() < 2 {
        return LatticeType::Top;
    }

    // pushfirst! returns the modified array (first argument)
    args[0].clone()
}

/// Transfer function for `empty!` (remove all elements).
///
/// Type rules:
/// - empty!(Array{T}) → Array{T}
/// - empty!(Dict{K,V}) → Dict{K,V}
///
/// # Examples
/// ```text
/// empty!(Array{Int64}) → Array{Int64}
/// ```
pub fn tfunc_empty_bang(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    // empty! returns the emptied collection (same type) for built-in
    // collections. A user *struct* may override it with a different return type,
    // so defer only for a struct receiver instead of assuming the receiver type
    // — otherwise the override's value is constant-folded (Issue #6610). Only a
    // struct reaches this tfunc at the comparison site (call-site inference
    // specializes wrappers to the concrete argument type); leaving other
    // receivers untouched avoids widening native-array `_mem` inference.
    match &args[0] {
        LatticeType::Concrete(ConcreteType::Struct { .. }) => LatticeType::Top,
        other => other.clone(),
    }
}

/// Transfer function for `resize!` (resize array).
///
/// Type rules:
/// - resize!(Array{T}, n) → Array{T}
///
/// # Examples
/// ```text
/// resize!(Array{Int64}, 10) → Array{Int64}
/// ```
pub fn tfunc_resize(args: &[LatticeType]) -> LatticeType {
    if args.len() < 2 {
        return LatticeType::Top;
    }

    // resize! returns the resized array (first argument)
    args[0].clone()
}

/// Transfer function for `splice!` (remove and optionally replace elements).
///
/// Type rules:
/// - splice!(Array{T}, index) → T
/// - splice!(Array{T}, range) → Array{T}
/// - splice!(Array{T}, range, replacement) → Array{T}
///
/// # Examples
/// ```text
/// splice!(Array{Int64}, 1) → Int64
/// splice!(Array{Int64}, 1:3) → Array{Int64}
/// ```
pub fn tfunc_splice(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    let is_range_index =
        args.len() >= 2 && matches!(&args[1], LatticeType::Concrete(ConcreteType::Range { .. }));

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Array { element, .. }) => {
            if is_range_index {
                // splice!(arr, range) or splice!(arr, range, ins) returns Array{T} (Issue #3481)
                LatticeType::Concrete(ConcreteType::Array {
                    element: element.clone(),
                    ndims: None,
                })
            } else {
                // splice!(arr, i::Integer) returns element T
                LatticeType::Concrete(*element.clone())
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `reverse!` (reverse array in place).
///
/// Type rules:
/// - reverse!(Array{T}) → Array{T}
///
/// # Examples
/// ```text
/// reverse!(Array{Int64}) → Array{Int64}
/// ```
pub fn tfunc_reverse_bang(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    // reverse! returns the reversed array (same type)
    args[0].clone()
}

/// Transfer function for `sort!` (sort array in place).
///
/// Type rules:
/// - sort!(Array{T}) → Array{T}
///
/// # Examples
/// ```text
/// sort!(Array{Int64}) → Array{Int64}
/// ```
pub fn tfunc_sort_bang(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    // sort! returns the sorted array (first argument)
    args[0].clone()
}

/// Transfer function for `reverse` (return reversed copy).
///
/// Type rules:
/// - reverse(Array{T}) → Array{T}
///
/// # Examples
/// ```text
/// reverse(Array{Int64}) → Array{Int64}
/// ```
pub fn tfunc_reverse(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    // reverse returns a new array with same type
    args[0].clone()
}

/// Transfer function for `sort` (return sorted copy).
///
/// Type rules:
/// - sort(Array{T}) → Array{T}
///
/// # Examples
/// ```text
/// sort(Array{Int64}) → Array{Int64}
/// ```
pub fn tfunc_sort(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    // sort returns a new array with same type
    args[0].clone()
}

/// Transfer function for `unique` (return unique elements).
///
/// Type rules:
/// - unique(Array{T}) → Array{T}
///
/// # Examples
/// ```text
/// unique(Array{Int64}) → Array{Int64}
/// ```
pub fn tfunc_unique(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    // unique returns array of same element type
    args[0].clone()
}

/// Transfer function for `unique!` (remove duplicate elements in place).
///
/// Type rules:
/// - unique!(Array{T}) → Array{T}
///
/// # Examples
/// ```text
/// unique!(Array{Int64}) → Array{Int64}
/// ```
pub fn tfunc_unique_bang(args: &[LatticeType]) -> LatticeType {
    tfunc_unique(args)
}

/// Transfer function for `copy` (shallow copy).
///
/// Type rules:
/// - copy(Array{T}) → Array{T}
/// - copy(Dict{K,V}) → Any
///
/// # Examples
/// ```text
/// copy(Array{Int64}) → Array{Int64}
/// ```
pub fn tfunc_copy(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        // Issue #5867: the current pure-Julia `copy(::Dict)` method returns the
        // legacy VM Dict value, not the `Dict{K,V}` struct representation.
        // Keeping the input type here lets nested calls such as
        // `length(copy(Dict{String,Int64}()))` compile to struct-field access
        // and then fail at runtime on Value::Dict.
        LatticeType::Concrete(ConcreteType::Dict { .. }) => LatticeType::Top,
        _ => args[0].clone(),
    }
}

/// Transfer function for `deepcopy` (deep copy).
///
/// Type rules:
/// - deepcopy(T) → T
///
/// # Examples
/// ```text
/// deepcopy(Array{Array{Int64}}) → Array{Array{Int64}}
/// ```
pub fn tfunc_deepcopy(args: &[LatticeType]) -> LatticeType {
    tfunc_copy(args)
}

/// Transfer function for `fill!` (fill array with value).
///
/// Type rules:
/// - fill!(Array{T}, value) → Array{T}
///
/// # Examples
/// ```text
/// fill!(Array{Int64}, 0) → Array{Int64}
/// ```
pub fn tfunc_fill_bang(args: &[LatticeType]) -> LatticeType {
    if args.len() < 2 {
        return LatticeType::Top;
    }

    // fill! returns the modified array
    args[0].clone()
}

/// Transfer function for `fill` (create array filled with value).
///
/// Type rules:
/// - fill(value, dims...) → Array{typeof(value)}
///
/// # Examples
/// ```text
/// fill(0, 10) → Array{Int64}
/// fill(0.0, 3, 3) → Array{Float64}
/// ```
pub fn tfunc_fill(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) => LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ct.clone()),
            ndims: None,
        }),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `zeros` (create array of zeros).
///
/// Type rules:
/// - zeros(dims...) → Array{Float64}
/// - zeros(T, dims...) → Array{T}
///
/// # Examples
/// ```text
/// zeros(10) → Array{Float64}
/// zeros(Int64, 10) → Array{Int64}
/// ```
pub fn tfunc_zeros(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    // Match Julia's base/array.jl:
    //   zeros(dims...) -> zeros(Float64, dims)
    //   zeros(::Type{T}, dims...) -> Array{T}
    // A concrete numeric first argument is a dimension value, not a type object.
    match &args[0] {
        LatticeType::Concrete(ConcreteType::DataType { name }) => {
            let element =
                ConcreteType::from_type_name(name).unwrap_or(ConcreteType::Core(CoreType::Any));
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(element),
                ndims: None,
            })
        }
        _ => {
            // Default to Float64
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64,
                ))),
                ndims: None,
            })
        }
    }
}

/// Transfer function for `ones` (create array of ones).
///
/// Type rules:
/// - ones(dims...) → Array{Float64}
/// - ones(T, dims...) → Array{T}
pub fn tfunc_ones(args: &[LatticeType]) -> LatticeType {
    tfunc_zeros(args) // Same type rules
}

/// Transfer function for `similar` (create uninitialized array of same type).
///
/// Type rules:
/// - similar(Array{T}) → Array{T}
/// - similar(Array{T}, dims...) → Array{T}
///
/// # Examples
/// ```text
/// similar(Array{Int64}) → Array{Int64}
/// ```
pub fn tfunc_similar(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Array { element, .. }) => {
            LatticeType::Concrete(ConcreteType::Array {
                element: element.clone(),
                ndims: None,
            })
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `trues` / `falses` (BitArray-family constructors).
///
/// Type rules (Issue #5922): the dimension count selects the container:
/// - 1 argument → `BitVector`
/// - 2 arguments → `BitMatrix`
/// - n arguments → `BitArray{n}`
pub fn tfunc_trues(args: &[LatticeType]) -> LatticeType {
    let name = match args.len() {
        1 => "BitVector".to_string(),
        2 => "BitMatrix".to_string(),
        n => format!("BitArray{{{n}}}"),
    };
    LatticeType::Concrete(ConcreteType::Struct { name, type_id: 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_getindex_array() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
                ndims: None,
            }),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_getindex(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_issue_4270_getindex_preserves_union_element() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::UnionOf(vec![
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ])),
                ndims: None,
            }),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_getindex(&args);

        let mut expected = BTreeSet::new();
        expected.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        )));
        expected.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert_eq!(result, LatticeType::Union(expected));
    }

    #[test]
    fn test_getindex_tuple() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Tuple {
                elements: vec![
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
                ],
            }),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_getindex(&args);
        // Should join Int64 and Float64
        assert!(result.is_numeric());
    }

    #[test]
    fn test_length_array() {
        let args = vec![LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            ndims: None,
        })];
        let result = tfunc_length(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_length_string() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String),
        ))];
        let result = tfunc_length(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_length_fixed_tuple_is_const() {
        // Issue #5142: length of a fixed-arity tuple has a statically known
        // value, so inference must propagate it as Const(N), matching upstream
        // `nfields_tfunc` which returns `Const(length(x.types))`.
        use crate::compile::lattice::types::ConstValue;
        let args = vec![LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ],
        })];
        let result = tfunc_length(&args);
        assert_eq!(result, LatticeType::Const(ConstValue::Int64(2)));
    }

    #[test]
    fn test_length_empty_tuple_is_const_zero() {
        // Issue #5142: `length(())` folds to Const(0).
        use crate::compile::lattice::types::ConstValue;
        let args = vec![LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![],
        })];
        let result = tfunc_length(&args);
        assert_eq!(result, LatticeType::Const(ConstValue::Int64(0)));
    }

    #[test]
    fn test_length_vararg_tuple_stays_int64() {
        // Issue #5142: a tuple with a variadic tail has an unknown length, so
        // it must NOT be folded to a constant — it widens to Int64.
        let args = vec![LatticeType::Concrete(ConcreteType::TupleVararg {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))],
            tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        })];
        let result = tfunc_length(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_push_returns_array() {
        let array_type = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        });
        let args = vec![
            array_type.clone(),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_push(&args);
        assert_eq!(result, array_type);
    }

    #[test]
    fn test_pop_returns_element() {
        let args = vec![LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            ndims: None,
        })];
        let result = tfunc_pop(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_empty_bang_returns_collection_but_defers_for_struct_6610() {
        // Built-in collection: empty! returns the emptied collection (same type).
        let array_type = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        });
        assert_eq!(
            tfunc_empty_bang(std::slice::from_ref(&array_type)),
            array_type
        );
        // A user struct may override empty! with a different return type → defer.
        let struct_arg = LatticeType::Concrete(ConcreteType::Struct {
            name: "Tagged".to_string(),
            type_id: 1,
        });
        assert_eq!(tfunc_empty_bang(&[struct_arg]), LatticeType::Top);
    }

    #[test]
    fn test_first_array() {
        let args = vec![LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        })];
        let result = tfunc_first(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_size_array() {
        let args = vec![LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            ndims: None,
        })];
        let result = tfunc_size(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Tuple {
                elements: vec![ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))]
            })
        );
    }

    #[test]
    fn test_map_preserves_element_type() {
        let array_type = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        });
        let args = vec![
            LatticeType::Top, // function type (unknown)
            array_type.clone(),
        ];
        let result = tfunc_map(&args);
        assert_eq!(result, array_type);
    }

    #[test]
    fn test_filter_preserves_element_type() {
        let array_type = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            ndims: None,
        });
        let args = vec![
            LatticeType::Top, // predicate function type (unknown)
            array_type.clone(),
        ];
        let result = tfunc_filter(&args);
        assert_eq!(result, array_type);
    }

    #[test]
    fn test_colon_int() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_colon(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Range {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
            })
        );
    }

    #[test]
    fn test_colon_float() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        ];
        let result = tfunc_colon(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Range {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64
                ))),
            })
        );
    }

    #[test]
    fn test_colon_with_step() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_colon(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Range {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
            })
        );
    }

    #[test]
    fn test_range_function() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        ];
        let result = tfunc_range(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Range {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64
                ))),
            })
        );
    }

    #[test]
    fn test_append_returns_array() {
        let array_type = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        });
        let args = vec![array_type.clone(), array_type.clone()];
        let result = tfunc_append(&args);
        assert_eq!(result, array_type);
    }

    #[test]
    fn test_popfirst_returns_element() {
        let args = vec![LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            ndims: None,
        })];
        let result = tfunc_popfirst(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_pushfirst_returns_array() {
        let array_type = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        });
        let args = vec![
            array_type.clone(),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_pushfirst(&args);
        assert_eq!(result, array_type);
    }

    #[test]
    fn test_sort_returns_array() {
        let array_type = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        });
        let args = vec![array_type.clone()];
        let result = tfunc_sort(&args);
        assert_eq!(result, array_type);
    }

    #[test]
    fn test_copy_dict_widens_to_top_issue_5867() {
        let args = vec![LatticeType::Concrete(ConcreteType::Dict {
            key: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
            value: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        })];
        let result = tfunc_copy(&args);
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn test_fill() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_fill(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
                ndims: None
            })
        );
    }

    #[test]
    fn test_zeros_default() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = tfunc_zeros(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64
                ))),
                ndims: None
            })
        );
    }

    #[test]
    fn test_zeros_typed_prefix() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::DataType {
                name: "Int64".to_string(),
            }),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_zeros(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
                ndims: None
            })
        );
    }

    #[test]
    fn test_similar() {
        let args = vec![LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            ndims: None,
        })];
        let result = tfunc_similar(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64
                ))),
                ndims: None
            })
        );
    }
}
