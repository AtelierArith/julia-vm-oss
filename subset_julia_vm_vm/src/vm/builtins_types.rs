//! Type builtin functions for the VM.
//!
//! Type operations: typeof, isa, convert, promote, subtype checks.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::builtins::BuiltinId;
use crate::inference_core::CoreType;
use crate::vm::value::is_native_array_value;
use subset_julia_vm_bytecode::RuntimeCompileContext;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::type_objects::RuntimeTypeRegistry;
use super::type_utils::{
    normalize_type_for_isa, type_objects_equal, type_values_subtype,
    unbounded_unionall_alias_equivalent,
};
use super::value::{
    julia_array_type_for_ndims, native_array_value_ref, ArrayElementType, DictKey,
    GeneratorCallable, MemoryValue, RangeElementType, RangeValue, RuntimeTypeVarValue, RustBigInt,
    StructInstance, SymbolValue, Value,
};
use super::Vm;

/// Validate the argument count of a fixed-arity builtin, mirroring upstream
/// Julia's `JL_NARGS(fname, min, max)` machinery (`julia/src/julia.h` /
/// `julia/src/rtutils.c`). On a mismatch this raises a catchable
/// `ArgumentError` whose message matches `jl_too_few_args` /
/// `jl_too_many_args` exactly (Issue #5493).
///
/// It used to raise the `TypeError` variant carrying a message that merely
/// began with the *text* `ArgumentError:` — the message said one class while
/// `typeof(caught)` was another. That contradiction is the exact shape the
/// Issue #11146 taxonomy funnel exists to make impossible, and
/// `scripts/check_exception_taxonomy_funnel.sh` now rejects it.
fn check_builtin_arity(fname: &str, argc: usize, expected: usize) -> Result<(), VmError> {
    if argc < expected {
        Err(VmError::ArgumentError(format!(
            "{}: too few arguments (expected {})",
            fname, expected
        )))
    } else if argc > expected {
        Err(VmError::ArgumentError(format!(
            "{}: too many arguments (expected {})",
            fname, expected
        )))
    } else {
        Ok(())
    }
}

fn range_struct_base_name(name: &str) -> &str {
    let unqualified = name.rsplit('.').next().unwrap_or(name);
    unqualified.split('{').next().unwrap_or(unqualified)
}

fn range_struct_element_type(name: &str) -> RangeElementType {
    if name.contains("BigInt") {
        RangeElementType::BigInt
    } else if name.contains("UInt8") {
        RangeElementType::UInt8
    } else if name.contains("UInt16") {
        RangeElementType::UInt16
    } else if name.contains("UInt32") {
        RangeElementType::UInt32
    } else if name.contains("UInt64") {
        RangeElementType::UInt64
    } else if name.contains("Int8") {
        RangeElementType::Int8
    } else if name.contains("Int16") {
        RangeElementType::Int16
    } else if name.contains("Int32") {
        RangeElementType::Int32
    } else if name.contains("Char") {
        RangeElementType::Char
    } else {
        RangeElementType::Default
    }
}

fn range_struct_step_type(name: &str, step: Option<&Value>) -> RangeElementType {
    match step {
        Some(Value::BigInt(_)) => RangeElementType::BigInt,
        Some(Value::U8(_)) => RangeElementType::UInt8,
        Some(Value::U16(_)) => RangeElementType::UInt16,
        Some(Value::U32(_)) => RangeElementType::UInt32,
        Some(Value::U64(_)) => RangeElementType::UInt64,
        Some(Value::I8(_)) => RangeElementType::Int8,
        Some(Value::I16(_)) => RangeElementType::Int16,
        Some(Value::I32(_)) => RangeElementType::Int32,
        Some(Value::Char(_)) => RangeElementType::Char,
        Some(_) => RangeElementType::Default,
        None => range_struct_element_type(name),
    }
}

fn range_struct_value_to_bigint(value: &Value) -> Option<RustBigInt> {
    match value {
        Value::BigInt(v) => Some(v.clone()),
        Value::I64(v) => Some(RustBigInt::from(*v)),
        Value::I32(v) => Some(RustBigInt::from(*v)),
        Value::I16(v) => Some(RustBigInt::from(*v)),
        Value::I8(v) => Some(RustBigInt::from(*v)),
        Value::I128(v) => Some(RustBigInt::from(*v)),
        Value::U64(v) => Some(RustBigInt::from(*v)),
        Value::U32(v) => Some(RustBigInt::from(*v)),
        Value::U16(v) => Some(RustBigInt::from(*v)),
        Value::U8(v) => Some(RustBigInt::from(*v)),
        Value::U128(v) => Some(RustBigInt::from(*v)),
        _ => None,
    }
}

fn range_struct_value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::I64(v) => Some(*v as f64),
        Value::I32(v) => Some(*v as f64),
        Value::I16(v) => Some(*v as f64),
        Value::I8(v) => Some(*v as f64),
        Value::I128(v) => Some(*v as f64),
        Value::U64(v) => Some(*v as f64),
        Value::U32(v) => Some(*v as f64),
        Value::U16(v) => Some(*v as f64),
        Value::U8(v) => Some(*v as f64),
        Value::U128(v) => Some(*v as f64),
        Value::Char(c) => Some(u32::from(*c) as f64),
        _ => None,
    }
}

fn struct_instance_as_range_value(instance: &StructInstance) -> Option<RangeValue> {
    let base = range_struct_base_name(&instance.struct_name);
    let unit_step = Value::I64(1);
    let (start, step, stop, is_step_range) = match base {
        "UnitRange" => (
            instance.values.first()?,
            &unit_step,
            instance.values.get(1)?,
            false,
        ),
        "StepRange" => (
            instance.values.first()?,
            instance.values.get(1)?,
            instance.values.get(2)?,
            true,
        ),
        _ => return None,
    };
    let element_type = range_struct_element_type(&instance.struct_name);
    let step_type = range_struct_step_type(&instance.struct_name, is_step_range.then_some(step));
    if matches!(element_type, RangeElementType::BigInt) {
        return Some(RangeValue::bigint_range(
            range_struct_value_to_bigint(start)?,
            range_struct_value_to_bigint(step)?,
            range_struct_value_to_bigint(stop)?,
            is_step_range,
            element_type,
            step_type,
        ));
    }
    Some(RangeValue {
        start: range_struct_value_to_f64(start)?,
        step: range_struct_value_to_f64(step)?,
        stop: range_struct_value_to_f64(stop)?,
        is_float: false,
        element_type,
        step_type,
        is_step_range,
        linspace_len: None,
        step_defined: false,
        bigint: None,
    })
}

fn value_as_range_value(value: &Value, struct_heap: &[StructInstance]) -> Option<RangeValue> {
    match value {
        Value::Range(range) => Some(range.clone()),
        Value::Struct(instance) => struct_instance_as_range_value(instance),
        Value::StructRef(idx) => struct_heap
            .get(*idx)
            .and_then(struct_instance_as_range_value),
        _ => None,
    }
}

fn reflection_type_name(type_val: &Value) -> Option<String> {
    match type_val {
        Value::DataType(jt) => Some(jt.name().to_string()),
        Value::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

fn reflection_julia_type(type_name: &str) -> crate::types::JuliaType {
    crate::types::JuliaType::from_name(type_name)
        .unwrap_or_else(|| crate::types::JuliaType::Struct(type_name.to_string()))
}

fn reflection_julia_type_value(type_val: &Value) -> Option<crate::types::JuliaType> {
    match type_val {
        Value::DataType(jt) => Some((**jt).clone()),
        Value::RuntimeTypeVar(tv) => Some(tv.projection()),
        Value::Str(s) => Some(reflection_julia_type(s)),
        _ => None,
    }
}

fn structured_typeintersect(
    left: &crate::types::JuliaType,
    right: &crate::types::JuliaType,
    mut is_subtype: impl FnMut(&CoreType, &CoreType) -> bool,
) -> crate::types::JuliaType {
    let left_core = CoreType::from_julia_type_preserving_owner(left);
    let right_core = CoreType::from_julia_type_preserving_owner(right);
    if is_subtype(&left_core, &right_core) {
        return left.clone();
    }
    if is_subtype(&right_core, &left_core) {
        return right.clone();
    }
    let result =
        if matches!(left_core, CoreType::TypeOf(_)) || matches!(right_core, CoreType::TypeOf(_)) {
            CoreType::Bottom
        } else {
            left_core.type_intersect(&right_core)
        };
    crate::inference_core::core_type_to_julia_type(&result)
}

/// Recover the type name an operand of `<:`/`>:` denotes.
///
/// Bare `Ref`/`RefValue` evaluate to their callable constructors rather than
/// `DataType` values, so the abstract-type name must be recovered from the
/// function name for `<:` to work (e.g. `Ref{Int} <: Ref`) (Issue #5130).
/// Shared by the `<:` (Subtype) and `>:` (SupertypeOp) builtins (Issue #5115).
fn subtype_operand_name(v: &Value) -> String {
    match v {
        Value::DataType(jt) => jt.name().to_string(),
        Value::Str(s) => s.to_string(),
        Value::Struct(s) => s
            .array_wrapper_julia_type()
            .map(|jt| jt.name().to_string())
            .unwrap_or_else(|| s.struct_name.to_string()),
        Value::Function(f) if ref_type_callable_name(v).is_some() => f.name.clone(),
        _ => format!("{:?}", v),
    }
}

/// Whether a runtime value is a valid operand of Julia's `<:` relation.
///
/// Type objects and runtime TypeVars participate directly. `Ref`/`RefValue`
/// are the existing callable-constructor representation of those UnionAll type
/// objects (Issue #5130). Ordinary functions, modules, instances, and scalar
/// values are values rather than types and must raise `TypeError` instead of
/// being stringified into a nominal subtype query (Issue #11176).
fn is_subtype_operand(v: &Value) -> bool {
    matches!(v, Value::DataType(_) | Value::RuntimeTypeVar(_))
        || ref_type_callable_name(v).is_some()
}

fn ref_type_callable_name(v: &Value) -> Option<&str> {
    match v {
        Value::Function(f) if matches!(f.name.as_str(), "Ref" | "RefValue") => Some(&f.name),
        _ => None,
    }
}

fn structured_subtype_operand(v: &Value) -> Option<CoreType> {
    match v {
        Value::DataType(ty) => Some(CoreType::from(ty.as_ref())),
        Value::RuntimeTypeVar(var) => Some(CoreType::from(&var.projection())),
        _ => None,
    }
}

fn subtype_operand_has_runtime_identity(v: &Value) -> bool {
    match v {
        Value::DataType(ty) => ty.contains_runtime_typevar(),
        Value::RuntimeTypeVar(_) => true,
        _ => false,
    }
}

// Identity-bearing operands cannot survive the name-based subtype bridge.
// Keep ordinary nominal pairs on that bridge so VM registry classification
// remains authoritative for user-defined hierarchy edges.
fn structured_runtime_subtype_operands(
    left: &Value,
    right: &Value,
) -> Option<(CoreType, CoreType)> {
    if !subtype_operand_has_runtime_identity(left) && !subtype_operand_has_runtime_identity(right) {
        return None;
    }
    Some((
        structured_subtype_operand(left)?,
        structured_subtype_operand(right)?,
    ))
}

/// Issue #4694: walk a `JuliaType` looking for a `TypeVar` whose `name`
/// matches `var_name`, also scanning embedded parametric type strings such
/// as `"Vector{T}"` or `"Dict{K, V}"`. Used by the `UnionAll(var, body)`
/// constructor to skip wrapping when the body does not reference the
/// bound variable (matching upstream `jl_type_unionall`).
fn julia_type_references_typevar(ty: &crate::types::JuliaType, var_name: &str) -> bool {
    use crate::types::JuliaType;
    match ty {
        JuliaType::TypeVar(name, bound) => {
            name == var_name
                || bound
                    .as_deref()
                    .is_some_and(|bound| type_name_references_typevar(bound, var_name))
        }
        JuliaType::RuntimeTypeVar {
            lower_bound,
            upper_bound,
            ..
        } => {
            julia_type_references_typevar(lower_bound, var_name)
                || julia_type_references_typevar(upper_bound, var_name)
        }
        JuliaType::RuntimeParametric { params, .. } => params
            .iter()
            .any(|param| julia_type_references_typevar(param, var_name)),
        JuliaType::RuntimeUnionAll { var, body } => {
            let (shadows_outer, bounds_reference_outer) = match var.as_ref() {
                JuliaType::RuntimeTypeVar {
                    name,
                    lower_bound,
                    upper_bound,
                    ..
                } => (
                    name == var_name,
                    julia_type_references_typevar(lower_bound, var_name)
                        || julia_type_references_typevar(upper_bound, var_name),
                ),
                _ => (false, false),
            };
            bounds_reference_outer
                || (!shadows_outer && julia_type_references_typevar(body, var_name))
        }
        JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) => {
            julia_type_references_typevar(inner, var_name)
        }
        JuliaType::TypeOf(inner) => julia_type_references_typevar(inner, var_name),
        JuliaType::TupleOf(elements) | JuliaType::Union(elements) => elements
            .iter()
            .any(|elem| julia_type_references_typevar(elem, var_name)),
        JuliaType::UnionAll {
            lower_bound,
            var,
            bound,
            body,
        } => {
            // The inner UnionAll shadows the same-named variable; only
            // recurse into its body when the names differ.
            lower_bound
                .as_deref()
                .is_some_and(|lower| type_name_references_typevar(lower, var_name))
                || bound
                    .as_deref()
                    .is_some_and(|upper| type_name_references_typevar(upper, var_name))
                || (var != var_name && julia_type_references_typevar(body, var_name))
        }
        JuliaType::Struct(name) => type_name_references_typevar(name, var_name),
        // A where-binder can shadow a name that resolves all the way to a
        // concrete/abstract leaf variant (`Int64`, `Real`, `Bool`, ...)
        // rather than staying a generic `Struct(name)` wrapper, because
        // those names are recognized by `JuliaType::from_name` before the
        // "unknown name -> Struct" fallback ever runs (Issue #10100 / epic
        // #10049). `var_name` here is always the EXPLICIT binder of the
        // enclosing `UnionAll(var, body)` call being constructed, not a
        // spelling guess (Issue #9563's "no name-shape TypeVar heuristic"
        // policy is about inventing TypeVar-ness from identifier spelling
        // with no declared binder in scope; here the binder is already
        // declared and we are only recognizing this specific leaf as ITS
        // shadowed occurrence). Matching `Vector{T} where T<:Real`'s
        // existing behavior (kept via the `Struct("T")` arm above), a leaf
        // whose canonical name equals the binder's name is the same kind of
        // occurrence and must count as a reference too, or the `where` gets
        // silently dropped. This is display/construction-only: the
        // `JuliaType::UnionAll { var, body, .. }` node still holds `body`'s
        // raw (unrebound) concrete leaf — the `<:`/`==`/`isa`-facing
        // correctness for the shadowed occurrence comes from
        // `CoreType::rebind_where_binders` on the `CoreType::from` path
        // (`inference_core/type_core.rs`, extended by the same Issue for
        // `Primitive`/`Abstract` leaves), reusing the one canonical
        // mechanism (Issue #9464) instead of a second, JuliaType-level one.
        other => other.name() == var_name,
    }
}

pub(super) fn type_name_references_typevar(name: &str, var_name: &str) -> bool {
    let bytes = name.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx].is_ascii_alphanumeric() || bytes[idx] == b'_' {
            let start = idx;
            while idx < bytes.len() && (bytes[idx].is_ascii_alphanumeric() || bytes[idx] == b'_') {
                idx += 1;
            }
            if &name[start..idx] == var_name {
                // A `where B` binder shadows only the BARE (unqualified) name
                // `B`; an explicitly module-qualified reference (`Core.B`,
                // `M.N.B`) is never shadowed by it, matching upstream Julia's
                // lexical `where`-binder scoping (Issue #10280 / epic #10049).
                // The last component of a qualified path is preceded by `.`;
                // skip it so `Vector{Core.Builtin} where Builtin<:Function`
                // sees `Builtin` as UNUSED and drops the `where` (yielding the
                // concrete `Vector{Core.Builtin}` DataType) instead of keeping
                // a spurious `UnionAll`. This is general over any qualified
                // path, not a `Core.`-name special case.
                let module_qualified = start > 0 && bytes[start - 1] == b'.';
                if !module_qualified {
                    return true;
                }
            }
        } else {
            idx += 1;
        }
    }
    false
}

fn canonicalize_builtin_unionall_alias(
    var_name: &str,
    lower: &Option<String>,
    bound: &Option<String>,
    body: &crate::types::JuliaType,
) -> Option<crate::types::JuliaType> {
    if lower.is_some() || bound.is_some() {
        return None;
    }

    if !matches!(
        body,
        crate::types::JuliaType::UnionAll { .. } | crate::types::JuliaType::RuntimeUnionAll { .. }
    ) {
        let compact = compact_type_name(body.name().as_ref());
        if var_name == "T" && compact == format!("Vector{{{}}}", var_name) {
            return Some(crate::types::JuliaType::Struct("Vector".to_string()));
        }
        if var_name == "T" && compact == format!("Matrix{{{}}}", var_name) {
            return Some(crate::types::JuliaType::Struct("Matrix".to_string()));
        }
        if var_name == "T" && compact == format!("DenseVector{{{}}}", var_name) {
            return Some(crate::types::JuliaType::Struct("DenseVector".to_string()));
        }
        if var_name == "T" && compact == format!("DenseMatrix{{{}}}", var_name) {
            return Some(crate::types::JuliaType::Struct("DenseMatrix".to_string()));
        }
        if var_name == "T" && compact == format!("Array{{{},1}}", var_name) {
            return Some(crate::types::JuliaType::Struct("Vector".to_string()));
        }
        if var_name == "T" && compact == format!("Array{{{},2}}", var_name) {
            return Some(crate::types::JuliaType::Struct("Matrix".to_string()));
        }
        if var_name == "T" && compact == format!("DenseArray{{{},1}}", var_name) {
            return Some(crate::types::JuliaType::Struct("DenseVector".to_string()));
        }
        if var_name == "T" && compact == format!("DenseArray{{{},2}}", var_name) {
            return Some(crate::types::JuliaType::Struct("DenseMatrix".to_string()));
        }
        if var_name == "T" && compact == format!("Set{{{}}}", var_name) {
            return Some(crate::types::JuliaType::Set);
        }
    }

    match body {
        crate::types::JuliaType::VectorOf(inner)
            if var_name == "T" && is_unbounded_typevar_named(inner, var_name) =>
        {
            Some(crate::types::JuliaType::Struct("Vector".to_string()))
        }
        crate::types::JuliaType::MatrixOf(inner)
            if var_name == "T" && is_unbounded_typevar_named(inner, var_name) =>
        {
            Some(crate::types::JuliaType::Struct("Matrix".to_string()))
        }
        crate::types::JuliaType::Struct(name) => {
            let compact = compact_type_name(name);
            if var_name == "T" && compact == format!("Vector{{{}}}", var_name) {
                Some(crate::types::JuliaType::Struct("Vector".to_string()))
            } else if var_name == "T" && compact == format!("Matrix{{{}}}", var_name) {
                Some(crate::types::JuliaType::Struct("Matrix".to_string()))
            } else if var_name == "T" && compact == format!("DenseVector{{{}}}", var_name) {
                Some(crate::types::JuliaType::Struct("DenseVector".to_string()))
            } else if var_name == "T" && compact == format!("DenseMatrix{{{}}}", var_name) {
                Some(crate::types::JuliaType::Struct("DenseMatrix".to_string()))
            } else if var_name == "T" && compact == format!("Array{{{},1}}", var_name) {
                Some(crate::types::JuliaType::Struct("Vector".to_string()))
            } else if var_name == "T" && compact == format!("Array{{{},2}}", var_name) {
                Some(crate::types::JuliaType::Struct("Matrix".to_string()))
            } else if var_name == "T" && compact == format!("DenseArray{{{},1}}", var_name) {
                Some(crate::types::JuliaType::Struct("DenseVector".to_string()))
            } else if var_name == "T" && compact == format!("DenseArray{{{},2}}", var_name) {
                Some(crate::types::JuliaType::Struct("DenseMatrix".to_string()))
            } else if var_name == "T" && compact == format!("Set{{{}}}", var_name) {
                Some(crate::types::JuliaType::Set)
            } else {
                None
            }
        }
        crate::types::JuliaType::UnionAll {
            lower_bound: inner_lower,
            var: inner_var,
            bound: inner_bound,
            body: inner_body,
        } if inner_lower.is_none() && inner_bound.is_none() => {
            let compact = compact_type_name(inner_body.name().as_ref());
            if var_name == "T"
                && inner_var == "N"
                && compact == format!("Array{{{},{}}}", var_name, inner_var)
            {
                Some(crate::types::JuliaType::Array)
            } else if var_name == "T"
                && inner_var == "N"
                && compact == format!("DenseArray{{{},{}}}", var_name, inner_var)
            {
                Some(crate::types::JuliaType::Struct("DenseArray".to_string()))
            } else if var_name == "K"
                && inner_var == "V"
                && compact == format!("Dict{{{},{}}}", var_name, inner_var)
            {
                Some(crate::types::JuliaType::Dict)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn rebind_runtime_unionall_source_bound(
    ty: &crate::types::JuliaType,
    binder_name: &str,
    replacement: &crate::types::JuliaType,
    runtime_typevar_counter: &mut u64,
) -> crate::types::JuliaType {
    use crate::types::JuliaType;

    match ty {
        JuliaType::RuntimeTypeVar {
            id,
            name,
            lower_bound,
            upper_bound,
        } => JuliaType::RuntimeTypeVar {
            id: *id,
            name: name.clone(),
            lower_bound: Box::new(rebind_runtime_unionall_source_bound(
                lower_bound,
                binder_name,
                replacement,
                runtime_typevar_counter,
            )),
            upper_bound: Box::new(rebind_runtime_unionall_source_bound(
                upper_bound,
                binder_name,
                replacement,
                runtime_typevar_counter,
            )),
        },
        JuliaType::RuntimeParametric { base, params } => JuliaType::from_structured_parametric(
            base.clone(),
            params
                .iter()
                .map(|param| {
                    rebind_runtime_unionall_source_bound(
                        param,
                        binder_name,
                        replacement,
                        runtime_typevar_counter,
                    )
                })
                .collect(),
        ),
        JuliaType::VectorOf(inner) => {
            JuliaType::VectorOf(Box::new(rebind_runtime_unionall_source_bound(
                inner,
                binder_name,
                replacement,
                runtime_typevar_counter,
            )))
        }
        JuliaType::MatrixOf(inner) => {
            JuliaType::MatrixOf(Box::new(rebind_runtime_unionall_source_bound(
                inner,
                binder_name,
                replacement,
                runtime_typevar_counter,
            )))
        }
        JuliaType::TupleOf(types) => JuliaType::TupleOf(
            types
                .iter()
                .map(|ty| {
                    rebind_runtime_unionall_source_bound(
                        ty,
                        binder_name,
                        replacement,
                        runtime_typevar_counter,
                    )
                })
                .collect(),
        ),
        JuliaType::Union(types) => JuliaType::Union(
            types
                .iter()
                .map(|ty| {
                    rebind_runtime_unionall_source_bound(
                        ty,
                        binder_name,
                        replacement,
                        runtime_typevar_counter,
                    )
                })
                .collect(),
        ),
        JuliaType::TypeOf(inner) => {
            JuliaType::TypeOf(Box::new(rebind_runtime_unionall_source_bound(
                inner,
                binder_name,
                replacement,
                runtime_typevar_counter,
            )))
        }
        // Nested wrappers establish their own lexical scope. Reuse the
        // shadow-aware body visitor rather than rebinding through that scope.
        JuliaType::UnionAll { .. } | JuliaType::RuntimeUnionAll { .. } => {
            rebind_runtime_unionall_source_body(
                ty,
                binder_name,
                replacement,
                runtime_typevar_counter,
            )
        }
        _ => rebind_runtime_unionall_source_body(
            ty,
            binder_name,
            replacement,
            runtime_typevar_counter,
        ),
    }
}

fn rebind_runtime_unionall_source_body(
    ty: &crate::types::JuliaType,
    binder_name: &str,
    replacement: &crate::types::JuliaType,
    runtime_typevar_counter: &mut u64,
) -> crate::types::JuliaType {
    use crate::types::JuliaType;

    match ty {
        JuliaType::RuntimeTypeVar {
            id,
            name,
            lower_bound,
            upper_bound,
        } => JuliaType::RuntimeTypeVar {
            id: *id,
            name: name.clone(),
            // A nested binder's bounds are evaluated in the enclosing scope.
            // Preserve the nested binder identity itself, but structurally
            // rebind any outer source binder referenced by either bound.
            lower_bound: Box::new(rebind_runtime_unionall_source_body(
                lower_bound,
                binder_name,
                replacement,
                runtime_typevar_counter,
            )),
            upper_bound: Box::new(rebind_runtime_unionall_source_body(
                upper_bound,
                binder_name,
                replacement,
                runtime_typevar_counter,
            )),
        },
        JuliaType::RuntimeParametric { base, params } => JuliaType::from_structured_parametric(
            base.clone(),
            params
                .iter()
                .map(|param| {
                    rebind_runtime_unionall_source_body(
                        param,
                        binder_name,
                        replacement,
                        runtime_typevar_counter,
                    )
                })
                .collect(),
        ),
        JuliaType::RuntimeUnionAll { var, body } => {
            let shadows_outer = matches!(var.as_ref(), JuliaType::RuntimeTypeVar { name, .. } if name == binder_name);
            let rebound_var = match var.as_ref() {
                JuliaType::RuntimeTypeVar {
                    id,
                    name,
                    lower_bound,
                    upper_bound,
                } => JuliaType::RuntimeTypeVar {
                    id: *id,
                    name: name.clone(),
                    // A nested binder's bounds are evaluated before the binder
                    // enters scope. Rebind legacy source placeholders there,
                    // while identity-bearing RuntimeTypeVars remain rigid; body
                    // TypeVars are handled by the ordinary visitor (#10460).
                    lower_bound: Box::new(rebind_runtime_unionall_source_bound(
                        lower_bound,
                        binder_name,
                        replacement,
                        runtime_typevar_counter,
                    )),
                    upper_bound: Box::new(rebind_runtime_unionall_source_bound(
                        upper_bound,
                        binder_name,
                        replacement,
                        runtime_typevar_counter,
                    )),
                },
                _ => rebind_runtime_unionall_source_body(
                    var,
                    binder_name,
                    replacement,
                    runtime_typevar_counter,
                ),
            };
            let rebound_body = if shadows_outer {
                body.as_ref().clone()
            } else {
                rebind_runtime_unionall_source_body(
                    body,
                    binder_name,
                    replacement,
                    runtime_typevar_counter,
                )
            };
            JuliaType::RuntimeUnionAll {
                var: Box::new(rebound_var),
                body: Box::new(rebound_body),
            }
        }
        JuliaType::VectorOf(inner) => {
            JuliaType::VectorOf(Box::new(rebind_runtime_unionall_source_body(
                inner,
                binder_name,
                replacement,
                runtime_typevar_counter,
            )))
        }
        JuliaType::MatrixOf(inner) => {
            JuliaType::MatrixOf(Box::new(rebind_runtime_unionall_source_body(
                inner,
                binder_name,
                replacement,
                runtime_typevar_counter,
            )))
        }
        JuliaType::TupleOf(types) => JuliaType::TupleOf(
            types
                .iter()
                .map(|ty| {
                    rebind_runtime_unionall_source_body(
                        ty,
                        binder_name,
                        replacement,
                        runtime_typevar_counter,
                    )
                })
                .collect(),
        ),
        JuliaType::Union(types) => JuliaType::Union(
            types
                .iter()
                .map(|ty| {
                    rebind_runtime_unionall_source_body(
                        ty,
                        binder_name,
                        replacement,
                        runtime_typevar_counter,
                    )
                })
                .collect(),
        ),
        JuliaType::TypeOf(inner) => {
            JuliaType::TypeOf(Box::new(rebind_runtime_unionall_source_body(
                inner,
                binder_name,
                replacement,
                runtime_typevar_counter,
            )))
        }
        JuliaType::UnionAll { var, body, .. } => {
            // Legacy UnionAll bounds cannot carry an enclosing runtime ID.
            // Promote the nested wrapper through CoreType, then bind its body
            // to a fresh inner runtime TypeVar. Display alpha-renames only
            // binders that co-occur in the final body, preserving upstream's
            // same-name dependent-bound spelling (#10460 / #10572).
            let mut core = CoreType::from(ty);
            core.rebind_source_where_binder(binder_name);
            let CoreType::UnionAll {
                var: mut core_var, ..
            } = core
            else {
                return ty.clone();
            };
            if let Some(lower) = core_var.lower_bound.as_deref_mut() {
                lower.rebind_source_where_binder(binder_name);
            }
            if let Some(upper) = core_var.upper_bound.as_deref_mut() {
                upper.rebind_source_where_binder(binder_name);
            }
            let lower_bound = core_var
                .lower_bound
                .as_deref()
                .map(crate::inference_core::core_type_to_julia_type)
                .unwrap_or(JuliaType::Bottom)
                .substitute(binder_name, replacement);
            let upper_bound = core_var
                .upper_bound
                .as_deref()
                .map(crate::inference_core::core_type_to_julia_type)
                .unwrap_or(JuliaType::Any)
                .substitute(binder_name, replacement);
            let inner_id = *runtime_typevar_counter;
            *runtime_typevar_counter += 1;
            let inner = JuliaType::RuntimeTypeVar {
                id: inner_id,
                name: var.clone(),
                lower_bound: Box::new(lower_bound),
                upper_bound: Box::new(upper_bound),
            };
            let outer_rebound_body = if var == binder_name {
                body.as_ref().clone()
            } else {
                rebind_runtime_unionall_source_body(
                    body,
                    binder_name,
                    replacement,
                    runtime_typevar_counter,
                )
            };
            let rebound_body = rebind_runtime_unionall_source_body(
                &outer_rebound_body,
                var,
                &inner,
                runtime_typevar_counter,
            );
            JuliaType::RuntimeUnionAll {
                var: Box::new(inner),
                body: Box::new(rebound_body),
            }
        }
        // A source `where` binder shadows only an unqualified identifier.
        // Keep a simple module-qualified nominal leaf in JuliaType form so
        // `Core.Builtin` beside a bare `Builtin` is not collapsed by the
        // CoreType projection and rebound with the bare occurrence (#10460).
        JuliaType::Struct(name) if name.contains('.') && !name.contains('{') => ty.clone(),
        _ => {
            let mut core = CoreType::from(ty);
            core.rebind_source_where_binder(binder_name);
            let projected = crate::inference_core::core_type_to_julia_type(&core)
                .substitute(binder_name, replacement);
            if julia_type_contains_legacy_unionall(&projected) {
                rebind_runtime_unionall_source_body(
                    &projected,
                    binder_name,
                    replacement,
                    runtime_typevar_counter,
                )
            } else {
                projected
            }
        }
    }
}

fn julia_type_contains_legacy_unionall(ty: &crate::types::JuliaType) -> bool {
    use crate::types::JuliaType;
    match ty {
        JuliaType::UnionAll { .. } => true,
        JuliaType::RuntimeTypeVar {
            lower_bound,
            upper_bound,
            ..
        } => {
            julia_type_contains_legacy_unionall(lower_bound)
                || julia_type_contains_legacy_unionall(upper_bound)
        }
        JuliaType::RuntimeParametric { params, .. }
        | JuliaType::TupleOf(params)
        | JuliaType::Union(params) => params.iter().any(julia_type_contains_legacy_unionall),
        JuliaType::RuntimeUnionAll { var, body } => {
            julia_type_contains_legacy_unionall(var) || julia_type_contains_legacy_unionall(body)
        }
        JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) | JuliaType::TypeOf(inner) => {
            julia_type_contains_legacy_unionall(inner)
        }
        _ => false,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct UnionAllAliasBinder {
    name: String,
    lower: Option<String>,
    upper: Option<String>,
}

fn canonicalize_user_unionall_alias(
    compile_context: Option<&RuntimeCompileContext>,
    var_name: &str,
    lower: &Option<String>,
    bound: &Option<String>,
    body: &crate::types::JuliaType,
    partial_projection: Option<&crate::types::JuliaType>,
) -> Option<crate::types::JuliaType> {
    let ctx = compile_context?;
    let (family, params, binders) = user_unionall_alias_parts(var_name, lower, bound, body)?;
    let schema = parametric_struct_schema_for_family(ctx, &family)?;
    let declared_params = &schema.def.type_params;
    if params.len() != declared_params.len() || binders.len() > declared_params.len() {
        return None;
    }

    let fixed_prefix_len = declared_params.len() - binders.len();
    let is_declared_alias = params[fixed_prefix_len..]
        .iter()
        .zip(binders.iter())
        .zip(declared_params[fixed_prefix_len..].iter())
        .all(|((param, binder), declared)| {
            param == &declared.name
                && binder.name == declared.name
                && optional_bound_eq(&binder.lower, &declared.lower_bound)
                && optional_bound_eq(&binder.upper, &declared.upper_bound)
        });
    if !is_declared_alias {
        return None;
    }
    if fixed_prefix_len == 0 {
        Some(crate::types::JuliaType::Struct(family))
    } else {
        // A canonical partial application is still a UnionAll, not a DataType.
        // Retain the identity-bearing runtime wrapper so array metadata and
        // reflection observe the same binder/body graph.
        partial_projection.cloned()
    }
}

fn user_unionall_alias_parts(
    var_name: &str,
    lower: &Option<String>,
    bound: &Option<String>,
    body: &crate::types::JuliaType,
) -> Option<(String, Vec<String>, Vec<UnionAllAliasBinder>)> {
    let mut binders = vec![UnionAllAliasBinder {
        name: var_name.to_string(),
        lower: lower.clone(),
        upper: bound.clone(),
    }];
    let mut current = body;
    while let crate::types::JuliaType::UnionAll {
        lower_bound,
        var,
        bound,
        body,
    } = current
    {
        binders.push(UnionAllAliasBinder {
            name: var.clone(),
            lower: lower_bound.as_deref().cloned(),
            upper: bound.as_deref().cloned(),
        });
        current = body.as_ref();
    }

    let crate::types::JuliaType::Struct(name) = current else {
        return None;
    };
    let compact = compact_type_name(name);
    let brace_idx = compact.find('{')?;
    if !compact.ends_with('}') {
        return None;
    }
    let family = compact[..brace_idx].to_string();
    let params: Vec<String> = subset_julia_vm_bytecode::parse_parametric_params(&compact)
        .into_iter()
        .map(str::to_string)
        .collect();
    (params.len() >= binders.len()).then_some((family, params, binders))
}

fn parametric_struct_schema_for_family<'a>(
    ctx: &'a RuntimeCompileContext,
    family: &str,
) -> Option<&'a crate::runtime_types::struct_info::ParametricStructDef> {
    if let Some(schema) = ctx.parametric_structs.get(family) {
        return Some(schema);
    }
    if let Some(schema) = ctx.base_parametric_structs.get(family) {
        return Some(schema);
    }

    let mut matches = ctx
        .parametric_structs
        .iter()
        .chain(ctx.base_parametric_structs.iter())
        .filter(|(name, _)| nominal_family_name(name) == family);
    let first = matches.next().map(|(_, schema)| schema)?;
    matches.next().is_none().then_some(first)
}

fn nominal_family_name(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn optional_bound_eq(left: &Option<String>, right: &Option<String>) -> bool {
    left.as_deref().map(compact_type_name) == right.as_deref().map(compact_type_name)
}

fn is_unbounded_typevar_named(ty: &crate::types::JuliaType, var_name: &str) -> bool {
    matches!(ty, crate::types::JuliaType::TypeVar(name, None) if name == var_name)
}

fn compact_type_name(name: &str) -> String {
    name.chars().filter(|c| !c.is_whitespace()).collect()
}

fn runtime_type_projection_is_subtype_of_type(ty: &crate::types::JuliaType) -> bool {
    // `DataType`, `UnionAll`, and `Union` are the type kinds `<: Type`
    // upstream; a `TypeVar` is NOT a type (`TypeVar <: Type` is false), so it
    // stays excluded (Issue #10313).
    matches!(ty, crate::types::JuliaType::DataType)
        || matches!(ty, crate::types::JuliaType::Struct(name) if name == "UnionAll" || name == "Union")
}

fn type_object_argument(value: Value) -> Result<crate::types::JuliaType, VmError> {
    match value {
        Value::DataType(jt) => Ok(*jt),
        Value::RuntimeTypeVar(tv) => Ok(tv.projection()),
        other => Err(VmError::TypeError(format!(
            "TypeVar bound must be a Type or TypeVar, got {:?}",
            other.value_type()
        ))),
    }
}

fn array_value_size_bytes(element_type: &ArrayElementType, shape: &[usize]) -> i64 {
    let total_elems: i64 = shape.iter().map(|&d| d as i64).product();
    array_element_type_size_bytes(element_type) * total_elems
}

fn array_element_type_size_bytes(element_type: &ArrayElementType) -> i64 {
    match element_type {
        ArrayElementType::F64 | ArrayElementType::I64 | ArrayElementType::U64 => 8,
        ArrayElementType::F32 | ArrayElementType::I32 | ArrayElementType::U32 => 4,
        ArrayElementType::F16 | ArrayElementType::I16 | ArrayElementType::U16 => 2,
        ArrayElementType::I8 | ArrayElementType::U8 | ArrayElementType::Bool => 1,
        ArrayElementType::Char => 4,
        ArrayElementType::I128 | ArrayElementType::U128 | ArrayElementType::ComplexF64 => 16,
        ArrayElementType::ComplexF32 => 8,
        ArrayElementType::Nothing => 0,
        // All-`Float64` isbits struct stored contiguously: `field_count` f64
        // fields × 8 bytes, matching upstream's unboxed layout — e.g. a
        // 2-field `V2{Float64,Float64}` or `Complex{Float64}` is 16 B
        // (Issue #9198 S4).
        ArrayElementType::StructInlineF64(_, field_count) => 8 * *field_count as i64,
        ArrayElementType::String
        | ArrayElementType::SubString
        | ArrayElementType::Symbol
        | ArrayElementType::Struct
        | ArrayElementType::StructOf(_)
        | ArrayElementType::StructInlineOf(_, _)
        | ArrayElementType::Any
        | ArrayElementType::TupleOf(_)
        | ArrayElementType::UnionOf(_)
        | ArrayElementType::Abstract(_)
        | ArrayElementType::Structured(_) => 8,
    }
}

fn memory_value_size_bytes(memory: &MemoryValue) -> i64 {
    array_element_type_size_bytes(memory.element_type()) * memory.len() as i64
}

fn type_param_matches_memory_element(param: &str, element_type_name: &str) -> bool {
    let normalized = normalize_type_for_isa(param);
    let param = normalized.as_ref();
    if let Some(bound) = param.strip_prefix("<:") {
        let element_type = crate::types::JuliaType::from_name_or_struct(element_type_name);
        let bound_type = crate::types::JuliaType::from_name_or_struct(bound.trim());
        return type_values_subtype(&element_type, &bound_type);
    }
    param == element_type_name
}

/// Whether `value` is an `Array{T,N}`/`Vector`/`Matrix` value in any of its VM
/// carriers: the `StructRef`/`Struct` array-wrapper (the normal
/// `FinalizeArray`/literal result, Issue #6807) or the legacy native carrier
/// (`Value::ExprArgs`). Used by the `isa` handler to route every array carrier
/// through the runtime `check_subtype`, so `x isa T` agrees with
/// `typeof(x) <: T` for user-defined element/bound types (Issue #10576).
fn value_is_array_wrapper(value: &Value, struct_heap: &[StructInstance]) -> bool {
    if is_native_array_value(value) {
        return true;
    }
    match value {
        Value::StructRef(idx) => struct_heap
            .get(*idx)
            .is_some_and(|si| si.array_wrapper_julia_type().is_some()),
        Value::Struct(si) => si.array_wrapper_julia_type().is_some(),
        _ => false,
    }
}

fn memory_isa_target(element_type_name: &str, target_type_name: &str) -> bool {
    let normalized_target = normalize_type_for_isa(target_type_name);
    let target = normalized_target.as_ref();
    let base = target.find('{').map_or(target, |idx| &target[..idx]);
    let params = subset_julia_vm_bytecode::parse_parametric_params(target);

    let element_param_matches = |idx: usize| {
        params
            .get(idx)
            .is_some_and(|param| type_param_matches_memory_element(param, element_type_name))
    };
    let rank_param_matches = |idx: usize| params.get(idx).is_none_or(|rank| *rank == "1");

    match base {
        "Any" | "Memory" => params.is_empty() || element_param_matches(0),
        "GenericMemory" => {
            params.is_empty()
                || match params.len() {
                    1 => element_param_matches(0),
                    _ => {
                        let kind_matches = params
                            .first()
                            .is_none_or(|kind| matches!(*kind, ":not_atomic" | "not_atomic"));
                        kind_matches && element_param_matches(1)
                    }
                }
        }
        "AbstractArray" | "DenseArray" => {
            params.is_empty() || (element_param_matches(0) && rank_param_matches(1))
        }
        "AbstractVector" | "DenseVector" => params.is_empty() || element_param_matches(0),
        "Array" | "Vector" | "Matrix" | "AbstractMatrix" | "DenseMatrix" => false,
        _ => false,
    }
}

impl<R: crate::rng::RngLike> Vm<R> {
    pub(in crate::vm) fn reflection_supertype_name(&self, type_name: &str) -> String {
        RuntimeTypeRegistry::new_with_struct_defs(
            self.compile_context.as_ref(),
            &self.abstract_types,
            &self.struct_defs,
        )
        .supertype_name(type_name)
    }

    /// Execute type builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not a type builtin.
    pub(super) fn execute_builtin_types(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<Option<()>, VmError> {
        if self
            .execute_builtin_types_conversion(builtin, argc)?
            .is_some()
        {
            return Ok(Some(()));
        }

        match builtin {
            BuiltinId::TypeVar => {
                if argc != 1 && argc != 3 {
                    return Err(VmError::TypeError(
                        "TypeVar requires 1 or 3 arguments".to_string(),
                    ));
                }

                let (name_val, lower_bound, upper_bound) = if argc == 1 {
                    (
                        self.stack.pop_value()?,
                        crate::types::JuliaType::Bottom,
                        crate::types::JuliaType::Any,
                    )
                } else {
                    let upper = self.stack.pop_value()?;
                    let lower = self.stack.pop_value()?;
                    let name = self.stack.pop_value()?;
                    (
                        name,
                        type_object_argument(lower)?,
                        type_object_argument(upper)?,
                    )
                };

                let name = match name_val {
                    Value::Symbol(symbol) => symbol.into_string(),
                    Value::QuoteNode(inner) => match *inner {
                        Value::Symbol(symbol) => symbol.into_string(),
                        other => {
                            return Err(VmError::TypeError(format!(
                                "TypeVar name must be a Symbol, got {:?}",
                                other.value_type()
                            )));
                        }
                    },
                    other => {
                        return Err(VmError::TypeError(format!(
                            "TypeVar name must be a Symbol, got {:?}",
                            other.value_type()
                        )));
                    }
                };

                let id = self.runtime_typevar_counter;
                self.runtime_typevar_counter += 1;
                self.stack
                    .push(Value::RuntimeTypeVar(Box::new(RuntimeTypeVarValue {
                        id,
                        name,
                        lower_bound,
                        upper_bound,
                    })));
            }

            BuiltinId::UnionAll => {
                // Issue #4694: UnionAll(var::TypeVar, body) constructs a
                // `JuliaType::UnionAll` so Pure Julia helpers such as
                // `Base.rewrap_unionall` and `Base.rename_unionall` can wrap a
                // body back into a UnionAll after `unwrap_unionall` peeled the
                // existing layers (Issue #3909).
                if argc != 2 {
                    return Err(VmError::TypeError(
                        "UnionAll requires 2 arguments (var::TypeVar, body)".to_string(),
                    ));
                }
                let body_val = self.stack.pop_value()?;
                let var_val = self.stack.pop_value()?;

                let (var_name, lower, bound, runtime_id) = match &var_val {
                    Value::RuntimeTypeVar(tv) => {
                        let bound = if matches!(tv.upper_bound, crate::types::JuliaType::Any) {
                            None
                        } else {
                            Some(match &tv.upper_bound {
                                crate::types::JuliaType::RuntimeTypeVar { name, .. } => {
                                    name.clone()
                                }
                                upper => upper.name().to_string(),
                            })
                        };
                        // A `Union{}` (Bottom) lower bound is the implicit default
                        // and is not displayed; only a declared lower bound (e.g.
                        // `where Int<:T`) is carried through (#5650).
                        let lower = if matches!(tv.lower_bound, crate::types::JuliaType::Bottom) {
                            None
                        } else {
                            Some(match &tv.lower_bound {
                                crate::types::JuliaType::RuntimeTypeVar { name, .. } => {
                                    name.clone()
                                }
                                lower => lower.name().to_string(),
                            })
                        };
                        (tv.name.clone(), lower, bound, Some(tv.id))
                    }
                    Value::DataType(jt)
                        if matches!(jt.as_ref(), crate::types::JuliaType::TypeVar(..)) =>
                    {
                        let crate::types::JuliaType::TypeVar(name, bound) = jt.as_ref() else {
                            unreachable!()
                        };
                        (name.clone(), None, bound.clone(), None)
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "UnionAll var must be a TypeVar, got {:?}",
                            other.value_type()
                        )));
                    }
                };

                let mut body = match body_val {
                    Value::DataType(jt) => jt,
                    Value::RuntimeTypeVar(tv) => Box::new(tv.projection()),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "UnionAll body must be a Type, got {:?}",
                            other.value_type()
                        )));
                    }
                };
                if let Some(id) = runtime_id {
                    let Value::RuntimeTypeVar(tv) = &var_val else {
                        unreachable!("runtime TypeVar id requires runtime TypeVar value")
                    };
                    // The body literal is parsed before this explicit binder is
                    // available. Rebind builtin-shadowing nominal leaves in the
                    // canonical structural graph, then attach the runtime binder
                    // identity. This turns `Vector{Int64} where Int64` into a
                    // real `RuntimeUnionAll` body instead of relying on later
                    // display-name matching (Issue #10460).
                    let core_body = CoreType::from(body.as_ref());
                    let preserve_source_identity = core_body
                        .contains_registered_nominal_leaf_named(&var_name, &self.struct_hierarchy);
                    let source_references_binder =
                        julia_type_references_typevar(body.as_ref(), &var_name);
                    if source_references_binder {
                        body = Box::new(rebind_runtime_unionall_source_body(
                            body.as_ref(),
                            &var_name,
                            &tv.projection(),
                            &mut self.runtime_typevar_counter,
                        ));
                    }
                    if matches!(body.as_ref(), crate::types::JuliaType::RuntimeTypeVar { id: body_id, .. } if *body_id == id)
                    {
                        self.stack
                            .push(Value::DataType(Box::new(tv.upper_bound.clone())));
                        return Ok(Some(()));
                    }
                    if body.references_runtime_typevar(id) || source_references_binder {
                        let projection = crate::types::JuliaType::RuntimeUnionAll {
                            var: Box::new(tv.projection()),
                            body,
                        };
                        let canonical = if preserve_source_identity {
                            projection
                        } else {
                            projection
                                .semantic_alpha_projection()
                                .and_then(|semantic| {
                                    let crate::types::JuliaType::UnionAll {
                                        var,
                                        lower_bound,
                                        bound,
                                        body,
                                    } = semantic
                                    else {
                                        return None;
                                    };
                                    let lower = lower_bound.as_deref().cloned();
                                    let upper = bound.as_deref().cloned();
                                    canonicalize_builtin_unionall_alias(
                                        &var,
                                        &lower,
                                        &upper,
                                        body.as_ref(),
                                    )
                                    .or_else(|| {
                                        canonicalize_user_unionall_alias(
                                            self.compile_context.as_ref(),
                                            &var,
                                            &lower,
                                            &upper,
                                            body.as_ref(),
                                            Some(&projection),
                                        )
                                    })
                                })
                                .unwrap_or(projection)
                        };
                        self.stack.push(Value::DataType(Box::new(canonical)));
                        return Ok(Some(()));
                    }
                }

                // Match upstream `jl_type_unionall`: if the body does not
                // reference the bound variable, return the body unchanged.
                // This keeps `rewrap_unionall(Int64, Vector) === Int64` and
                // `UnionAll(T, Vector{Int64}) === Vector{Int64}` in agreement
                // with Julia's `Vector{Int64}` (not a UnionAll).
                let projection = if matches!(body.as_ref(), crate::types::JuliaType::TypeVar(name, _) if name == &var_name)
                {
                    // A bare bound variable body denotes its upper bound:
                    // `T where T === Any`, `T where T<:Real === Real`
                    // (Issue #5570).
                    bound
                        .as_deref()
                        .map(crate::types::JuliaType::from_name_or_struct)
                        .unwrap_or(crate::types::JuliaType::Any)
                } else if let Some(alias) =
                    canonicalize_builtin_unionall_alias(&var_name, &lower, &bound, body.as_ref())
                {
                    alias
                } else if let Some(alias) = canonicalize_user_unionall_alias(
                    self.compile_context.as_ref(),
                    &var_name,
                    &lower,
                    &bound,
                    body.as_ref(),
                    None,
                ) {
                    alias
                } else if julia_type_references_typevar(&body, &var_name) {
                    crate::types::JuliaType::UnionAll {
                        lower_bound: lower.map(Box::new),
                        var: var_name,
                        bound: bound.map(Box::new),
                        body,
                    }
                } else {
                    *body
                };
                self.stack.push(Value::DataType(Box::new(projection)));
            }

            BuiltinId::TypeOf => {
                let val = self.stack.pop_value()?;

                let julia_type = match &val {
                    // Issue #9741: `Union{}` is the bottom type object, whose
                    // kind is `Core.TypeofBottom`, not `DataType`.
                    Value::DataType(jt)
                        if matches!(jt.as_ref(), crate::types::JuliaType::Bottom) =>
                    {
                        crate::types::JuliaType::Struct("Core.TypeofBottom".to_string())
                    }
                    // Issue #5335: typeof(Union{...}) is `Union`, not `DataType`.
                    // The registry's kind classification owns this projection
                    // (RuntimeTypeObjectKind::Union), so `typeof` and `isa`
                    // cannot drift apart again (Issue #10313).
                    Value::DataType(jt) => {
                        let registry = RuntimeTypeRegistry::new_with_struct_defs(
                            self.compile_context.as_ref(),
                            &self.abstract_types,
                            &self.struct_defs,
                        );
                        registry.object(jt).runtime_type_projection()
                    }
                    Value::RuntimeTypeVar(_) => {
                        crate::types::JuliaType::Struct("TypeVar".to_string())
                    }
                    Value::StructRef(idx) => {
                        if let Some(s) = self.struct_heap.get(*idx) {
                            // Special case: Generator struct maps to Base.Generator type
                            if let Some(array_type) = self.array_wrapper_julia_type_resolved(s) {
                                array_type
                            } else if &*s.struct_name == "Generator" {
                                crate::types::JuliaType::Generator
                            } else {
                                // Issue #10577: project the concrete parametric name
                                // (Pair -> Pair{typeof(first), typeof(second)}).
                                crate::types::JuliaType::Struct(self.concrete_struct_type_name(s))
                            }
                        } else {
                            crate::types::JuliaType::Any
                        }
                    }
                    Value::Struct(s) => {
                        if let Some(array_type) = self.array_wrapper_julia_type_resolved(s) {
                            array_type
                        } else if s.struct_name.is_empty() {
                            crate::types::JuliaType::Any
                        } else if &*s.struct_name == "Generator" {
                            // Special case: Generator struct maps to Base.Generator type
                            crate::types::JuliaType::Generator
                        } else {
                            // Issue #10577: project the concrete parametric name
                            // (Pair -> Pair{typeof(first), typeof(second)}).
                            crate::types::JuliaType::Struct(self.concrete_struct_type_name(s))
                        }
                    }
                    _ if is_native_array_value(&val) => match native_array_value_ref(&val) {
                        Some(arr) => {
                            let arr_borrow = arr.borrow();
                            if let Some(container_type) = arr_borrow.array_type_override() {
                                crate::types::JuliaType::Struct(container_type.to_string())
                            } else {
                                let elem_type =
                                    self.array_value_declared_element_julia_type(&arr_borrow);
                                julia_array_type_for_ndims(elem_type, arr_borrow.shape.len())
                            }
                        }
                        None => val.runtime_type(),
                    },
                    Value::Memory(mem) => {
                        let mem = mem.borrow();
                        let elem_type_name = self.memory_element_type_name(mem.element_type());
                        crate::types::JuliaType::Struct(format!("Memory{{{}}}", elem_type_name))
                    }
                    Value::MemoryRef(memref) => {
                        crate::types::JuliaType::Struct(self.memory_ref_type_name(memref))
                    }
                    Value::NamedTuple(nt) => {
                        // Julia shows @NamedTuple{a::T1, b::T2} (short form)
                        let fields: Vec<String> = nt
                            .names
                            .iter()
                            .zip(nt.values.iter())
                            .map(|(name, val)| format!("{}::{}", name, self.get_type_name(val)))
                            .collect();
                        crate::types::JuliaType::Struct(format!(
                            "@NamedTuple{{{}}}",
                            fields.join(", ")
                        ))
                    }
                    Value::Pairs(p) => {
                        crate::types::JuliaType::Struct(self.pairs_runtime_type_name(p))
                    }
                    Value::Tuple(t) => {
                        let types = t
                            .elements
                            .iter()
                            .map(|element| {
                                let type_name = self.get_type_name(element);
                                crate::types::JuliaType::from_name_or_struct(&type_name)
                            })
                            .collect();
                        crate::types::JuliaType::TupleOf(types)
                    }
                    Value::Generator(g) => {
                        // Issue #9200 S3: a FILTERED generator's `I` type param is
                        // conceptually `Iterators.Filter`. Report the `Filter`
                        // spelling (reconstructed from the collapsed `callable`
                        // predicate + base, or read off a real `Filter` in `g.iter`
                        // — `filtered_generator_iter_type_name`), so the type-level
                        // `IteratorSize(typeof(g))` resolves to `SizeUnknown()`
                        // (`IteratorSize(::Type{<:Filter}) == SizeUnknown()`).
                        let iter_type = if self.generator_is_filtered(g) {
                            self.filtered_generator_iter_type_name(g)
                        } else {
                            self.get_type_name(g.iter.as_ref())
                        };
                        let callable_type = match &g.callable {
                            GeneratorCallable::TypeObject(jt) => {
                                format!("Type{{{}}}", jt.name())
                            }
                            GeneratorCallable::TupleSplatTypeObject(jt) => {
                                format!("Type{{{}}}", jt.name())
                            }
                            GeneratorCallable::FunctionIndex(func_index) => self
                                .function_index_singleton_type_name(*func_index)
                                .unwrap_or_else(|| "Function".to_string()),
                            GeneratorCallable::FilteredFunctionIndex { .. }
                            | GeneratorCallable::FilteredRuntimeValue { .. } => {
                                "Function".to_string()
                            }
                            GeneratorCallable::TupleSplatFunctionIndex(func_index) => self
                                .function_index_singleton_type_name(*func_index)
                                .unwrap_or_else(|| "Function".to_string()),
                            GeneratorCallable::RuntimeValue(callable)
                            | GeneratorCallable::TupleSplatRuntimeValue(callable) => {
                                if let Value::Function(function) = callable.as_ref() {
                                    function.singleton_type_name()
                                } else if let Value::DataType(jt) = callable.as_ref() {
                                    format!("Type{{{}}}", jt.name())
                                } else {
                                    self.get_type_name(callable.as_ref())
                                }
                            }
                            GeneratorCallable::Eager => "Any".to_string(),
                        };
                        crate::types::JuliaType::Struct(format!(
                            "Base.Generator{{{}, {}}}",
                            iter_type, callable_type
                        ))
                    }
                    Value::Function(function) => {
                        crate::types::JuliaType::Struct(function.singleton_type_name())
                    }
                    _ => val.runtime_type(),
                };
                self.stack.push(Value::DataType(Box::new(julia_type)));
            }

            BuiltinId::Isa => {
                // Upstream `jl_f_isa` validates `JL_NARGS(isa, 2, 2)` before
                // touching its arguments (Issue #5493). Without this the
                // immediate call path underflowed on `isa(x)` and silently
                // ignored extras on `isa(x, T, extra)`.
                check_builtin_arity("isa", argc, 2)?;
                let type_val = self.stack.pop_value()?;
                let val = self.stack.pop_value()?;

                let target_type_name = match &type_val {
                    Value::DataType(jt) => jt.name().to_string(),
                    Value::Str(s) => s.to_string(),
                    // Allow constructor functions as type arguments (e.g., isa(x, Ref))
                    // In Julia, Ref is both a type and a callable constructor (Issue #2687)
                    Value::Function(f) => f.name.clone(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "isa: second argument must be a type, got {:?}",
                            type_val
                        )));
                    }
                };

                // `isa(x, Type{B})` is true iff `x` is itself a type and `x`
                // satisfies the `Type{B}` parameter: invariant `x === B` for a
                // concrete `B`, covariant `x <: B` for the `Type{<:B}` spelling,
                // and any type for the unbounded `Type{T} where T` ≡ `Type`
                // (Issue #5068). A non-type value (e.g. the integer `3`) is never
                // `isa` a `Type{...}`.
                if let Some(crate::types::JuliaType::TypeOf(inner)) =
                    crate::types::JuliaType::from_name(&target_type_name)
                {
                    let value_type_object = match &val {
                        Value::DataType(jt) => Some(*jt.clone()),
                        Value::RuntimeTypeVar(tv) => Some(tv.projection()),
                        _ => None,
                    };
                    let is_match = value_type_object.is_some_and(|x| {
                        let value_type = crate::types::JuliaType::TypeOf(Box::new(x)).name();
                        let target_type = crate::types::JuliaType::TypeOf(inner.clone()).name();
                        self.check_subtype(value_type.as_ref(), target_type.as_ref())
                    });
                    self.stack.push(Value::Bool(is_match));
                    return Ok(Some(()));
                }

                let (struct_name_opt, resolved_val_type) = match &val {
                    // Issue #3909: route type-object values through the runtime
                    // type registry so the projected kind (DataType / UnionAll /
                    // TypeVar) matches typeof(). Without this, isa(Vector,
                    // UnionAll) would disagree with typeof(Vector) === UnionAll.
                    Value::DataType(jt) => {
                        let registry = RuntimeTypeRegistry::new_with_struct_defs(
                            self.compile_context.as_ref(),
                            &self.abstract_types,
                            &self.struct_defs,
                        );
                        (None, registry.object(jt).runtime_type_projection())
                    }
                    // Array-wrapper values (`Vector`/`Matrix`/`Array{T,N}`,
                    // carried as a `StructRef`/`Struct` wrapper or the legacy
                    // native carrier): `x isa T` must agree with
                    // `typeof(x) <: T` — upstream defines isa exactly this way.
                    // Route the value's concrete parametric type name through
                    // the runtime `check_subtype`, the same engine used by
                    // `typeof(x) <: T`, which consults the user abstract-type
                    // hierarchy and covariant bounds. The former
                    // `type_values_subtype` fallback (reached via the
                    // `(None, array_type)` tuple) only knew builtin abstract
                    // bounds, so `[DogIsa()] isa Vector{<:AniIsa}` (user
                    // abstract covariant bound) and `[DogIsa()] isa
                    // Vector{DogIsa}` (user struct element) were wrongly `false`
                    // while `typeof(...) <: ...` was `true` (Issue #10576).
                    // Mirrors the already-correct `Value::Tuple(_)` and
                    // `Value::StaticArray(_)` arms.
                    val_ref if value_is_array_wrapper(val_ref, &self.struct_heap) => {
                        let array_type_name = self.get_type_name(&val);
                        let is_match = self.check_subtype(&array_type_name, &target_type_name);
                        self.stack.push(Value::Bool(is_match));
                        return Ok(Some(()));
                    }
                    Value::StructRef(idx) => {
                        if let Some(si) = self.struct_heap.get(*idx) {
                            if let Some(array_type) = si.array_wrapper_julia_type() {
                                (None, array_type)
                            } else {
                                // Issue #10577: project the concrete parametric name
                                // (Pair -> Pair{typeof(first), typeof(second)}) so
                                // `isa` agrees with `typeof` on Pair value membership.
                                let name = self.concrete_struct_type_name(si);
                                (Some(name.clone()), crate::types::JuliaType::Struct(name))
                            }
                        } else {
                            (None, crate::types::JuliaType::Any)
                        }
                    }
                    Value::Struct(si) => {
                        if let Some(array_type) = si.array_wrapper_julia_type() {
                            (None, array_type)
                        } else {
                            // Issue #10577: project the concrete parametric name
                            // (Pair -> Pair{typeof(first), typeof(second)}) so
                            // `isa` agrees with `typeof` on Pair value membership.
                            let name = self.concrete_struct_type_name(si);
                            (Some(name.clone()), crate::types::JuliaType::Struct(name))
                        }
                    }
                    Value::Memory(mem) => {
                        let mem_ref = mem.borrow();
                        let element_type_name =
                            self.memory_element_type_name(mem_ref.element_type());
                        self.stack.push(Value::Bool(memory_isa_target(
                            &element_type_name,
                            &target_type_name,
                        )));
                        return Ok(Some(()));
                    }
                    Value::MemoryRef(_) => {
                        let target = crate::types::JuliaType::Struct(target_type_name.clone());
                        let is_match = self.value_matches_param(&val, &target);
                        self.stack.push(Value::Bool(is_match));
                        return Ok(Some(()));
                    }
                    Value::NamedTuple(nt) => {
                        // NamedTuple is a special type - use concrete type name
                        let fields: Vec<String> = nt
                            .names
                            .iter()
                            .zip(nt.values.iter())
                            .map(|(name, val)| format!("{}::{}", name, self.get_type_name(val)))
                            .collect();
                        let type_name = format!("@NamedTuple{{{}}}", fields.join(", "));
                        (
                            Some(type_name.clone()),
                            crate::types::JuliaType::Struct(type_name),
                        )
                    }
                    // Ref / RefValue: isa(Ref(x), Ref), isa(Ref(5), Ref{Int}) and
                    // isa(Ref(5), Base.RefValue{Int}) should all be true (Issue #2687, #5130).
                    // typeof() yields "Base.RefValue{T}"; reuse the dispatch matcher so the
                    // element type and the Ref/RefValue alias are handled consistently.
                    Value::Ref(_) => {
                        let target = crate::types::JuliaType::Struct(target_type_name.clone());
                        let is_match = self.value_matches_param(&val, &target);
                        self.stack.push(Value::Bool(is_match));
                        return Ok(Some(()));
                    }
                    // Core.SimpleVector: isa(<DataType>.parameters, Core.SimpleVector)
                    // should be true (Issue #4722). Route through the struct-name
                    // path so module-prefix normalization matches the target name.
                    Value::SimpleVector(_) => (
                        Some("Core.SimpleVector".to_string()),
                        crate::types::JuliaType::Struct("Core.SimpleVector".to_string()),
                    ),
                    // Tuple values: build the value's full type name through the
                    // VM context (`Tuple{Foo{Int64}}`, `Tuple{Dog}`, ...) so
                    // user-defined struct elements keep their parametric name and
                    // route the covariant comparison through the runtime
                    // `check_subtype`, which knows the user abstract hierarchy
                    // (`Dog <: Animal`) and the `Int`/`Int64` alias. The
                    // context-free `Value::runtime_type()` reports `Any` for
                    // `StructRef` tuple elements, breaking covariant Tuple `isa`
                    // such as `(Foo(1),) isa Tuple{Foo{Int}}` (Issue #5064).
                    Value::Tuple(_) => {
                        let tuple_type_name = self.get_type_name(&val);
                        let is_match = self.check_subtype(&tuple_type_name, &target_type_name);
                        self.stack.push(Value::Bool(is_match));
                        return Ok(Some(()));
                    }
                    // RNG values: route the concrete RNG type name through the
                    // struct-name path so `check_subtype` resolves
                    // `Xoshiro`/`StableRNG`/`TaskLocalRNG <: AbstractRNG`
                    // (Issues #7230, #7231).
                    Value::Rng(_) => {
                        let rng_type_name = self.get_type_name(&val);
                        (
                            Some(rng_type_name.clone()),
                            crate::types::JuliaType::Struct(rng_type_name),
                        )
                    }
                    // StaticArray flat representations carry their concrete Julia
                    // type; route through the struct-name path so abstract-type
                    // checks (e.g. `v isa AbstractArray`) resolve via
                    // `check_isa_with_abstract_resolved` (Issue #7964).
                    Value::StaticArray(sv) => {
                        let type_name = sv.julia_type_name().to_string();
                        (
                            Some(type_name.clone()),
                            crate::types::JuliaType::Struct(type_name),
                        )
                    }
                    Value::StaticArrayInline(sv) => {
                        let type_name = sv.julia_type_name_owned().to_string();
                        (
                            Some(type_name.clone()),
                            crate::types::JuliaType::Struct(type_name),
                        )
                    }
                    _ => (None, val.runtime_type()),
                };

                let normalized_target = normalize_type_for_isa(&target_type_name);

                // A bare (unqualified) type name that resolves to a *builtin
                // concrete* DataType (e.g. `Module`, `Int64`, `String`) binds to
                // that builtin in the current scope. A user struct value is `isa`
                // such a type only when its own concrete type matches — never via
                // short-name family matching against a same-short-name
                // module-local user type. Without this gate `Box() isa Module`
                // (bare `Base.Module`) wrongly matched a module-local
                // `TypeOwner7955.Module` abstract type, whose short name `Module`
                // is also recorded as `Box`'s supertype in the struct hierarchy
                // and the abstract-type index (Issue #7963). Qualified targets
                // (`TypeOwner7955.Module`) keep the dot, so they skip this gate
                // and still resolve through the family/abstract paths.
                let target_is_bare_builtin_concrete = !target_type_name.contains('.')
                    && crate::types::JuliaType::from_name(&target_type_name)
                        .is_some_and(|t| CoreType::from(&t).is_builtin_concrete_datatype());

                let is_match = if normalized_target.as_ref() == "Type"
                    && runtime_type_projection_is_subtype_of_type(&resolved_val_type)
                {
                    true
                } else if let Some(ref struct_name) = struct_name_opt {
                    let normalized_struct = normalize_type_for_isa(struct_name);
                    // Normalization strips module owners, so both the equality
                    // and the subtype query below would collapse a USER
                    // module's `Faux.Array` into the Base-owned bare `Array`
                    // spelling. When the two families are not owner-compatible,
                    // compare the RAW spellings instead so the owner-aware
                    // engine gate decides (Issues #11388/#11395).
                    let normalized_names_hit = if crate::types::nominal_family_names_compatible(
                        struct_name,
                        &target_type_name,
                    ) {
                        normalized_struct == normalized_target
                            || self.check_subtype(&normalized_struct, &normalized_target)
                    } else {
                        self.check_subtype(struct_name, &target_type_name)
                    };
                    if !target_is_bare_builtin_concrete && normalized_names_hit {
                        true
                    } else {
                        let is_abstract_type = !target_is_bare_builtin_concrete
                            && self
                                .abstract_type_name_index
                                .contains_key(&target_type_name);

                        if is_abstract_type {
                            self.check_isa_with_abstract_resolved(
                                &struct_name_opt,
                                &target_type_name,
                            )
                        } else if self.struct_hierarchy.contains_name(&target_type_name) {
                            // A registered struct/abstract target spelled without
                            // parameters: `check_subtype` (consulted above with the
                            // hierarchy + the typevar-vs-nominal classification, see
                            // `classify_subtype_operand`) is authoritative for a
                            // declared struct value, so its `false` is final here.
                            // The permissive enum-only `type_values_subtype` residue
                            // wrongly accepted `P{Int}() isa Q` for an unrelated
                            // registered parametric struct `Q` because `from_name`
                            // infers a free type variable for short uppercase[+digit]
                            // names (#8092).
                            false
                        } else {
                            let target_type = crate::types::JuliaType::from_name(&target_type_name)
                                .unwrap_or_else(|| {
                                    crate::types::JuliaType::Struct(target_type_name.clone())
                                });
                            type_values_subtype(&resolved_val_type, &target_type)
                        }
                    }
                } else {
                    let target_type = crate::types::JuliaType::from_name(&target_type_name)
                        .unwrap_or_else(|| {
                            crate::types::JuliaType::Struct(target_type_name.clone())
                        });
                    type_values_subtype(&resolved_val_type, &target_type)
                };
                self.stack.push(Value::Bool(is_match));
            }

            BuiltinId::Subtype => {
                // Upstream `jl_f_issubtype` validates `JL_NARGS(<:, 2, 2)`
                // before touching its arguments (Issue #5493). Without this the
                // immediate call path underflowed on `(<:)(A)` and silently
                // ignored extras on `(<:)(A, B, extra)`.
                check_builtin_arity("<:", argc, 2)?;
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;

                for operand in [&left, &right] {
                    if !is_subtype_operand(operand) {
                        return Err(VmError::TypeError(format!(
                            "in <:, expected Type, got a value of type {}",
                            self.get_type_name(operand)
                        )));
                    }
                }

                let equal_generic_aliases = match (&left, &right) {
                    (Value::DataType(left), Value::DataType(right)) => {
                        unbounded_unionall_alias_equivalent(left, right)
                    }
                    _ => false,
                };
                let is_subtype = equal_generic_aliases
                    || match structured_runtime_subtype_operands(&left, &right) {
                        Some((left, right)) => self.check_subtype_core(&left, &right),
                        None => self.check_subtype(
                            &subtype_operand_name(&left),
                            &subtype_operand_name(&right),
                        ),
                    };
                self.stack.push(Value::Bool(is_subtype));
            }
            BuiltinId::SupertypeOp => {
                // `A >: B` is the supertype operator, equivalent to `B <: A`
                // (julia/base/operators.jl). Reachable when `>:` is used as a
                // first-class function value, e.g. `(>:)(Number, Int)` or
                // `f = (>:); f(Number, Int)` (Issue #5115). The binary infix
                // `A >: B` is instead lowered to `BinaryOp::Subtype` with the
                // operands swapped (lowering/expr/binary.rs).
                //
                // Unlike `<:`/`isa`, `>:` is an ordinary 2-arg Julia function
                // (`>:(a, b) = (b <: a)`), not a builtin, so upstream raises a
                // `MethodError` — not an `ArgumentError` — on the wrong arity
                // (Issue #5493). Without this guard the immediate call path
                // underflowed on `(>:)(A)` and silently ignored extras on
                // `(>:)(A, B, extra)`.
                if argc != 2 {
                    let args = self.peek_stack_top(argc);
                    let arg_type_names = args
                        .iter()
                        .map(|arg| self.get_type_name(arg))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(VmError::MethodError(format!(
                        "no method matching >:({})",
                        arg_type_names
                    )));
                }
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;

                // `A >: B` is implemented as `B <: A` upstream, including
                // `<:`'s operand validation and catchable TypeError shape.
                for operand in [&right, &left] {
                    if !is_subtype_operand(operand) {
                        return Err(VmError::TypeError(format!(
                            "in <:, expected Type, got a value of type {}",
                            self.get_type_name(operand)
                        )));
                    }
                }

                // A >: B  ⟺  B <: A
                let is_supertype = match structured_runtime_subtype_operands(&right, &left) {
                    Some((right, left)) => self.check_subtype_core(&right, &left),
                    None => self
                        .check_subtype(&subtype_operand_name(&right), &subtype_operand_name(&left)),
                };
                self.stack.push(Value::Bool(is_supertype));
            }
            BuiltinId::_Typeintersect => {
                // _typeintersect(a, b) - semantic type intersection for Pure Julia
                // typeintersect(). User-defined hierarchy still uses the VM
                // registry via check_subtype; structured built-ins use CoreType.
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;
                let Some(left_type) = reflection_julia_type_value(&left) else {
                    self.stack
                        .push(Value::DataType(Box::new(crate::types::JuliaType::Bottom)));
                    return Ok(Some(()));
                };
                let Some(right_type) = reflection_julia_type_value(&right) else {
                    self.stack
                        .push(Value::DataType(Box::new(crate::types::JuliaType::Bottom)));
                    return Ok(Some(()));
                };
                let result = structured_typeintersect(&left_type, &right_type, |left, right| {
                    self.check_subtype_core(left, right)
                });
                self.stack.push(Value::DataType(Box::new(result)));
            }
            BuiltinId::_TypeEqual => {
                check_builtin_arity("_type_equal", argc, 2)?;
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;
                let Some(left_type) = reflection_julia_type_value(&left) else {
                    return Err(VmError::TypeError(format!(
                        "_type_equal left argument must be a Type, got {:?}",
                        left.value_type()
                    )));
                };
                let Some(right_type) = reflection_julia_type_value(&right) else {
                    return Err(VmError::TypeError(format!(
                        "_type_equal right argument must be a Type, got {:?}",
                        right.value_type()
                    )));
                };
                self.stack
                    .push(Value::Bool(type_objects_equal(&left_type, &right_type)));
            }
            BuiltinId::Sizeof => {
                // sizeof(x) - return size of value in bytes
                // For primitive types, use the shared CoreType layout table.
                // For composite types (structs, arrays), return approximate size.
                let val = self.stack.pop_value()?;
                let size: i64 =
                    match &val {
                        // Scalar bits-type values must report the logical TYPE
                        // size (== sizeof(typeof(x))), NOT the boxed `Value`
                        // representation size. Each narrow variant maps to its
                        // own type so e.g. Int32(4) is 4, not 8 (Issue #6766).
                        Value::Bool(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Bool")
                            .unwrap_or(8) as i64,
                        Value::I8(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Int8")
                            .unwrap_or(1) as i64,
                        Value::I16(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Int16")
                            .unwrap_or(2) as i64,
                        Value::I32(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Int32")
                            .unwrap_or(4) as i64,
                        Value::I64(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Int64")
                            .unwrap_or(8) as i64,
                        Value::I128(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Int128")
                            .unwrap_or(16) as i64,
                        Value::U8(_) => CoreType::builtin_sizeof_bytes_for_julia_name("UInt8")
                            .unwrap_or(1) as i64,
                        Value::U16(_) => CoreType::builtin_sizeof_bytes_for_julia_name("UInt16")
                            .unwrap_or(2) as i64,
                        Value::U32(_) => CoreType::builtin_sizeof_bytes_for_julia_name("UInt32")
                            .unwrap_or(4) as i64,
                        Value::U64(_) => CoreType::builtin_sizeof_bytes_for_julia_name("UInt64")
                            .unwrap_or(8) as i64,
                        Value::U128(_) => CoreType::builtin_sizeof_bytes_for_julia_name("UInt128")
                            .unwrap_or(16) as i64,
                        Value::F16(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Float16")
                            .unwrap_or(2) as i64,
                        Value::F32(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Float32")
                            .unwrap_or(4) as i64,
                        Value::F64(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Float64")
                            .unwrap_or(8) as i64,
                        Value::Char(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Char")
                            .unwrap_or(8) as i64,
                        Value::Str(s) => s.len() as i64, // Number of bytes in UTF-8 encoding
                        _ if is_native_array_value(&val) => match native_array_value_ref(&val) {
                            Some(arr) => {
                                let arr_ref = arr.borrow();
                                array_value_size_bytes(&arr_ref.element_type(), &arr_ref.shape)
                            }
                            None => 8,
                        },
                        _ if self.array_wrapper_memory_and_shape(&val).is_some() => {
                            match self.array_wrapper_memory_and_shape(&val) {
                                Some((mem, shape)) => {
                                    let mem_ref = mem.borrow();
                                    array_value_size_bytes(mem_ref.element_type(), &shape)
                                }
                                None => 8,
                            }
                        }
                        Value::Memory(mem) => {
                            let mem_ref = mem.borrow();
                            memory_value_size_bytes(&mem_ref)
                        }
                        Value::Tuple(t) => {
                            // Sum of sizes of all elements
                            t.elements.len() as i64 * 8 // Approximate: 8 bytes per element pointer
                        }
                        Value::Nothing => CoreType::builtin_sizeof_bytes_for_julia_name("Nothing")
                            .unwrap_or(8) as i64,
                        Value::Missing => CoreType::builtin_sizeof_bytes_for_julia_name("Missing")
                            .unwrap_or(8) as i64,
                        Value::DataType(jt) => {
                            let registry = RuntimeTypeRegistry::new(
                                self.compile_context.as_ref(),
                                &self.abstract_types,
                            );
                            registry.object(jt).size_bytes().unwrap_or(8) as i64
                        }
                        Value::StructRef(_) | Value::Struct(_) => 8, // Pointer/reference size
                        _ => 8, // Default to pointer size for other types
                    };
                self.stack.push(Value::I64(size));
            }

            // BuiltinId::Isbits removed - pure Julia (Issue #6738)
            BuiltinId::Isbitstype => {
                // isbitstype(T) - check if T is a bits type
                let type_val = self.stack.pop_value()?;
                let type_object = match &type_val {
                    Value::DataType(jt) => jt.clone(),
                    Value::Str(s) => Box::new(reflection_julia_type(s)),
                    _ => {
                        self.stack.push(Value::Bool(false));
                        return Ok(Some(()));
                    }
                };

                let registry =
                    RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);
                let is_bits = registry.object(&type_object).is_bits_type();
                self.stack.push(Value::Bool(is_bits));
            }

            BuiltinId::_Supertype => {
                // _supertype(T) - get parent type for Pure Julia supertype().
                let type_val = self.stack.pop_value()?;
                let Some(type_name) = reflection_type_name(&type_val) else {
                    self.stack
                        .push(Value::DataType(Box::new(crate::types::JuliaType::Any)));
                    return Ok(Some(()));
                };

                let supertype = self.reflection_supertype_name(&type_name);
                self.stack
                    .push(Value::DataType(Box::new(reflection_julia_type(&supertype))));
            }

            BuiltinId::_Typename => {
                // _typename(T) - canonical TypeName symbol for Pure Julia
                // nameof(::Type) / Base.typename (Issue #5106). Resolves the
                // base name through the type registry, collapsing the `Array`
                // display aliases (`Vector`/`Matrix`) onto the shared
                // `TypeName` so `nameof(Vector{Int}) === :Array`.
                let type_val = self.stack.pop_value()?;
                let symbol = match &type_val {
                    Value::DataType(jt) => {
                        let registry = RuntimeTypeRegistry::new_with_struct_defs(
                            self.compile_context.as_ref(),
                            &self.abstract_types,
                            &self.struct_defs,
                        );
                        registry.object(jt).typename_symbol()
                    }
                    Value::RuntimeTypeVar(tv) => tv.name.clone(),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_typename expects a type, got {:?}",
                            other
                        )));
                    }
                };
                self.stack.push(Value::Symbol(SymbolValue::new(&symbol)));
            }

            BuiltinId::_FunctionName => {
                // _function_name(f) - direct function-name symbol for Pure
                // Julia nameof(::Function), avoiding string slicing of
                // `string(f)` (Issue #5580).
                let val = self.stack.pop_value()?;
                let name = match &val {
                    Value::Function(fv) => fv.name.clone(),
                    Value::Closure(cv) => cv.name.clone(),
                    Value::Str(s) => s.to_string(),
                    Value::Symbol(sym) => sym.as_str().to_string(),
                    Value::DataType(jt) => jt.name().to_string(),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_function_name expects a function, got {:?}",
                            other
                        )));
                    }
                };
                self.stack.push(Value::Symbol(SymbolValue::new(&name)));
            }

            BuiltinId::Subtypes => {
                // subtypes(T) - vector of direct subtypes
                let type_val = self.stack.pop_value()?;
                let type_name = match &type_val {
                    Value::DataType(jt) => jt.name().to_string(),
                    Value::Str(s) => s.to_string(),
                    _ => {
                        // For non-type values, return empty array
                        self.push_array_value_as_wrapper(super::value::ArrayValue::any_vector(
                            Vec::new(),
                        ))?;
                        return Ok(Some(()));
                    }
                };

                let registry = RuntimeTypeRegistry::new_with_struct_defs(
                    self.compile_context.as_ref(),
                    &self.abstract_types,
                    &self.struct_defs,
                );
                let subtypes: Vec<Value> = registry
                    .direct_subtypes(&type_name)
                    .into_iter()
                    .map(|t| Value::DataType(Box::new(t)))
                    .collect();

                self.push_array_value_as_wrapper(super::value::ArrayValue::any_vector(subtypes))?;
            }

            // BuiltinId::Typeintersect/Typejoin removed - now Pure Julia (base/reflection.jl)
            // BuiltinId::Fieldcount removed - now Pure Julia (base/reflection.jl)
            // BuiltinId::Hasfield removed - pure Julia (Issue #6738)

            // BuiltinId::Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype
            // removed - now Pure Julia (base/reflection.jl) with internal intrinsics
            BuiltinId::_Isabstracttype => {
                // _isabstracttype(T) - internal intrinsic: check if T is an abstract type
                let type_val = self.stack.pop_value()?;
                let registry = RuntimeTypeRegistry::new_with_struct_defs(
                    self.compile_context.as_ref(),
                    &self.abstract_types,
                    &self.struct_defs,
                );
                let is_abstract = match &type_val {
                    Value::DataType(jt) => registry.object(jt).is_abstract_type(),
                    _ => false,
                };
                self.stack.push(Value::Bool(is_abstract));
            }

            BuiltinId::_Isconcretetype => {
                // _isconcretetype(T) - internal intrinsic: check if T is a concrete type
                let type_val = self.stack.pop_value()?;
                let registry = RuntimeTypeRegistry::new_with_struct_defs(
                    self.compile_context.as_ref(),
                    &self.abstract_types,
                    &self.struct_defs,
                );
                let is_concrete = match &type_val {
                    Value::DataType(jt) => registry.object(jt).is_concrete_type(),
                    _ => false,
                };
                self.stack.push(Value::Bool(is_concrete));
            }

            BuiltinId::_Isprimitivetype => {
                // _isprimitivetype(T) - internal intrinsic: check if T is a primitive
                // type (fixed bit-width, no fields). Mirrors upstream Julia's
                // `isprimitivetype`, which `unwrap_unionall`s then flag-checks the
                // DataType (`(t.flags & 0x0080) == 0x0080`). Routed through the
                // runtime type registry so all five type predicates share one
                // classification path (Issue #5102). CoreType owns the built-in
                // primitive DataType set; in particular `String` is not primitive.
                let type_val = self.stack.pop_value()?;
                let registry = RuntimeTypeRegistry::new_with_struct_defs(
                    self.compile_context.as_ref(),
                    &self.abstract_types,
                    &self.struct_defs,
                );
                let is_primitive = match &type_val {
                    Value::DataType(jt) => registry.object(jt).is_primitive_type(),
                    _ => false,
                };
                self.stack.push(Value::Bool(is_primitive));
            }

            BuiltinId::_Isstructtype => {
                // _isstructtype(T) - internal intrinsic: check Julia's DataType
                // struct flag. CoreType owns built-in struct facts; VM registry
                // supplies user-defined struct definitions.
                let type_val = self.stack.pop_value()?;
                let registry = RuntimeTypeRegistry::new_with_struct_defs(
                    self.compile_context.as_ref(),
                    &self.abstract_types,
                    &self.struct_defs,
                );
                let is_struct_type = match &type_val {
                    Value::DataType(jt) => registry.object(jt).is_struct_type(),
                    _ => false,
                };
                self.stack.push(Value::Bool(is_struct_type));
            }

            BuiltinId::_Ismutabletype => {
                // _ismutabletype(T) - internal intrinsic: check if T is a mutable type
                let type_val = self.stack.pop_value()?;
                let registry = RuntimeTypeRegistry::new_with_struct_defs(
                    self.compile_context.as_ref(),
                    &self.abstract_types,
                    &self.struct_defs,
                );
                let is_mutable_type = match &type_val {
                    Value::DataType(jt) => registry.object(jt).is_mutable_type(),
                    _ => false,
                };
                self.stack.push(Value::Bool(is_mutable_type));
            }

            // BuiltinId::Ismutable removed - pure Julia (Issue #6738)

            // BuiltinId::Ismutabletype removed - now Pure Julia (base/reflection.jl)
            // BuiltinId::NameOf removed - now Pure Julia (base/reflection.jl)
            BuiltinId::Objectid => {
                // objectid(x) - unique object identifier
                // In Julia, this returns a UInt that uniquely identifies the object.
                // We use a hash-based approach for simplicity.
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                let val = self.stack.pop_value()?;
                let mut hasher = DefaultHasher::new();

                // Hash based on value type and content
                match &val {
                    Value::I64(n) => {
                        "I64".hash(&mut hasher);
                        n.hash(&mut hasher);
                    }
                    Value::F64(f) => {
                        "F64".hash(&mut hasher);
                        f.to_bits().hash(&mut hasher);
                    }
                    Value::Bool(b) => {
                        "Bool".hash(&mut hasher);
                        b.hash(&mut hasher);
                    }
                    Value::Str(s) => {
                        "Str".hash(&mut hasher);
                        s.hash(&mut hasher);
                    }
                    Value::Char(c) => {
                        "Char".hash(&mut hasher);
                        c.hash(&mut hasher);
                    }
                    Value::Nothing => {
                        "Nothing".hash(&mut hasher);
                    }
                    Value::Missing => {
                        "Missing".hash(&mut hasher);
                    }
                    _ if is_native_array_value(&val) => {
                        "Array".hash(&mut hasher);
                        // Use pointer-like identity for arrays (mutable objects)
                        if let Some(arr) = native_array_value_ref(&val) {
                            (arr.as_ptr() as usize).hash(&mut hasher);
                        }
                    }
                    // Memory → Array (Issue #2764)
                    Value::Memory(mem) => {
                        "Array".hash(&mut hasher);
                        // Use pointer-like identity for memory-backed arrays (mutable objects)
                        (mem.as_ptr() as usize).hash(&mut hasher);
                    }
                    Value::Struct(s) => {
                        "Struct".hash(&mut hasher);
                        s.struct_name.hash(&mut hasher);
                        s.type_id.hash(&mut hasher);
                    }
                    Value::Tuple(t) => {
                        "Tuple".hash(&mut hasher);
                        t.len().hash(&mut hasher);
                    }
                    Value::DataType(jt) => {
                        let registry = RuntimeTypeRegistry::new_with_struct_defs(
                            self.compile_context.as_ref(),
                            &self.abstract_types,
                            &self.struct_defs,
                        );
                        let object = registry.object(jt);
                        object.kind().hash(&mut hasher);
                        object.identity().stable_hash().hash(&mut hasher);
                    }
                    Value::RuntimeTypeVar(tv) => {
                        "RuntimeTypeVar".hash(&mut hasher);
                        tv.id.hash(&mut hasher);
                    }
                    _ => {
                        // For other types, use a simple discriminant
                        std::mem::discriminant(&val).hash(&mut hasher);
                    }
                }

                let id = hasher.finish();
                self.stack.push(Value::U64(id));
            }

            BuiltinId::Isunordered => {
                // isunordered(x) - check if x is unordered (NaN, Missing)
                // Returns true for values where comparisons are undefined
                let val = self.stack.pop_value()?;
                let is_unordered = match &val {
                    Value::F64(f) => f.is_nan(),
                    Value::Missing => true,
                    // Complex with NaN components
                    Value::Struct(s) if &*s.struct_name == "Complex" => s
                        .values
                        .iter()
                        .any(|v| matches!(v, Value::F64(f) if f.is_nan())),
                    _ => false,
                };
                self.stack.push(Value::Bool(is_unordered));
            }

            BuiltinId::In => {
                // in(x, collection) - check if element is in collection
                let collection = self.stack.pop_value()?;
                let element = self.stack.pop_value()?;

                let heap = self.struct_heap.as_slice();

                let complex_parts = |value: &Value| match value {
                    Value::StructRef(idx) => heap.get(*idx).and_then(|s| s.as_complex_parts()),
                    _ => value.as_complex_parts(),
                };

                // `as_complex_parts` also reports bare reals as `(v as f64, 0.0)`,
                // so the complex branch below must only fire when at least one
                // operand is GENUINELY complex; otherwise two reals would compare
                // via the lossy `v as f64` instead of the value-based numeric arms
                // (Issue #8187).
                let is_complex = |value: &Value| match value {
                    Value::StructRef(idx) => heap
                        .get(*idx)
                        .is_some_and(|s| s.as_complex_parts().is_some()),
                    Value::Struct(s) => s.as_complex_parts().is_some(),
                    _ => false,
                };

                #[derive(Clone, Copy, Debug, Eq, PartialEq)]
                enum NumericInteger {
                    NonNegative(u128),
                    Negative(i128),
                }

                let signed_integer_value = |value: i128| {
                    if value >= 0 {
                        NumericInteger::NonNegative(value.cast_unsigned())
                    } else {
                        NumericInteger::Negative(value)
                    }
                };

                let integer_value = |value: &Value| match value {
                    Value::I8(v) => Some(signed_integer_value(i128::from(*v))),
                    Value::I16(v) => Some(signed_integer_value(i128::from(*v))),
                    Value::I32(v) => Some(signed_integer_value(i128::from(*v))),
                    Value::I64(v) => Some(signed_integer_value(i128::from(*v))),
                    Value::I128(v) => Some(signed_integer_value(*v)),
                    Value::U8(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
                    Value::U16(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
                    Value::U32(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
                    Value::U64(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
                    Value::U128(v) => Some(NumericInteger::NonNegative(*v)),
                    _ => None,
                };

                // Helper function to compare two values for equality (like Julia's ==)
                let values_equal = |a: &Value, b: &Value| -> bool {
                    if let (Some(a_range), Some(b_range)) =
                        (value_as_range_value(a, heap), value_as_range_value(b, heap))
                    {
                        return a_range.elements_equal(&b_range);
                    }

                    if is_complex(a) || is_complex(b) {
                        if let (Some((a_re, a_im)), Some((b_re, b_im))) =
                            (complex_parts(a), complex_parts(b))
                        {
                            return a_re == b_re && a_im == b_im;
                        }
                    }

                    if let (Some(a_int), Some(b_int)) = (integer_value(a), integer_value(b)) {
                        return a_int == b_int;
                    }

                    match (a, b) {
                        (Value::I64(x), Value::I64(y)) => x == y,
                        (Value::F64(x), Value::F64(y)) => x == y,
                        // Mixed integer/float `==` (membership uses `==`, not
                        // `isequal`, so no sign-of-zero rule): value-based, no
                        // rounding of the integer (Issue #8187, all widths #8199).
                        (x, y)
                            if crate::vm::numeric_identity::mixed_int_float_values_equal(x, y)
                                .is_some() =>
                        {
                            crate::vm::numeric_identity::mixed_int_float_values_equal(x, y)
                                .unwrap_or(false)
                        }
                        (Value::Bool(x), Value::Bool(y)) => x == y,
                        (Value::Str(x), Value::Str(y)) => x == y,
                        (Value::Char(x), Value::Char(y)) => x == y,
                        (Value::Symbol(x), Value::Symbol(y)) => x == y,
                        (Value::Nothing, Value::Nothing) => true,
                        (Value::Missing, Value::Missing) => true,
                        // Ranges compare element-wise (`(1:3) in [1:3, 4:6]`,
                        // Issue #5725). Two ranges are equal iff they produce the
                        // same elements — including all empty ranges being equal.
                        (Value::Range(x), Value::Range(y)) => x.elements_equal(y),
                        // Type objects compare by canonical structural identity
                        // so `Int in [Float64, Int]` works (Issue #5108). Route
                        // both operands through the same DictKey canonicalization
                        // used for type-keyed Dict/Set membership.
                        (Value::DataType(_) | Value::RuntimeTypeVar(_), _)
                        | (_, Value::DataType(_) | Value::RuntimeTypeVar(_)) => {
                            match (DictKey::from_value(a), DictKey::from_value(b)) {
                                (Ok(ka), Ok(kb)) => ka == kb,
                                _ => false,
                            }
                        }
                        // Tuple / named-tuple / struct elements: the scalar arms
                        // above cannot compare them, so route through the shared
                        // `==` helper, which resolves heap struct refs and folds
                        // `==` over elements (Issue #6691, builds on #6685). This
                        // makes `(1, 2) in [(1, 2)]` and `(OneTo(3),) in [...]`
                        // behave like upstream.
                        _ => super::builtins_equality::values_equal_for_membership(a, b, heap),
                    }
                };

                let found = match &collection {
                    _ if is_native_array_value(&collection) => {
                        match native_array_value_ref(&collection) {
                            Some(arr) => {
                                let arr_ref = arr.borrow();
                                let len = arr_ref.element_count();
                                let mut found = false;
                                for i in 0..len {
                                    let v = arr_ref.get_linear(i)?;
                                    if values_equal(&element, &v) {
                                        found = true;
                                        break;
                                    }
                                }
                                found
                            }
                            None => false,
                        }
                    }
                    _ if self.array_wrapper_memory_and_shape(&collection).is_some() => {
                        match self.array_wrapper_memory_and_shape(&collection) {
                            Some((mem, shape)) => {
                                let mem_ref = mem.borrow();
                                let len = shape.iter().product::<usize>().min(mem_ref.len());
                                let mut found = false;
                                for i in 0..len {
                                    if let Some(v) = mem_ref.data.get_value(i) {
                                        if values_equal(&element, &v) {
                                            found = true;
                                            break;
                                        }
                                    }
                                }
                                found
                            }
                            None => false,
                        }
                    }
                    Value::Memory(mem) => {
                        let mem_ref = mem.borrow();
                        let len = mem_ref.len();
                        let mut found = false;
                        for i in 0..len {
                            if let Some(v) = mem_ref.data.get_value(i) {
                                if values_equal(&element, &v) {
                                    found = true;
                                    break;
                                }
                            }
                        }
                        found
                    }
                    Value::Tuple(t) => t.elements.iter().any(|v| values_equal(&element, v)),
                    Value::Str(s) => {
                        // For strings, check if element is a Char contained in the string
                        match &element {
                            Value::Char(c) => s.contains(*c),
                            Value::Str(substr) => s.contains(substr.as_ref()),
                            _ => false,
                        }
                    }
                    // `x in start:step:stop` — membership in a range (Issue #5728).
                    // During the #10150 migration, parser-created non-float
                    // ranges may be first-class UnitRange/StepRange structs;
                    // view them as RangeValue here to reuse the exact BigInt
                    // and Char membership implementation.
                    _ if value_as_range_value(&collection, heap).is_some() => {
                        value_as_range_value(&collection, heap)
                            .is_some_and(|range| range.contains_value(&element))
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "in requires Array, Tuple, String, Dict, or Set, got {:?}",
                            collection
                        )));
                    }
                };
                self.stack.push(Value::Bool(found));
            }

            BuiltinId::NonMissingType => {
                // nonmissingtype(T) - remove Missing from Union type
                // If T is Union{X, Missing}, return X
                // If T is Missing, return Union{} (Bottom)
                // If T doesn't contain Missing, return T unchanged
                let type_val = self.stack.pop_value()?;

                let result_type = match &type_val {
                    Value::DataType(jt) => {
                        match jt.as_ref() {
                            // If the type is Missing itself, return Bottom (Union{})
                            crate::types::JuliaType::Missing => crate::types::JuliaType::Bottom,

                            // If it's a Union type, filter out Missing
                            crate::types::JuliaType::Union(types) => {
                                let filtered: Vec<crate::types::JuliaType> = types
                                    .iter()
                                    .filter(|t| !matches!(t, crate::types::JuliaType::Missing))
                                    .cloned()
                                    .collect();

                                match filtered.len() {
                                    0 => crate::types::JuliaType::Bottom, // All types were Missing
                                    1 => {
                                        // Safety: len()==1 guarantees next() is Some
                                        match filtered.into_iter().next() {
                                            Some(t) => t,
                                            None => crate::types::JuliaType::Bottom,
                                        }
                                    }
                                    _ => crate::types::JuliaType::Union(filtered), // Multiple types remaining
                                }
                            }

                            // For any other JuliaType (Int64, Float64, etc.), return unchanged
                            // nonmissingtype only filters Missing from Union types
                            other => other.clone(),
                        }
                    }
                    Value::Str(type_name) => {
                        // Parse the type name (Issue #8630: `type_name` is now
                        // `Rc<str>`; view it as `&str` for the string ops below).
                        let type_name: &str = type_name.as_ref();
                        if type_name == "Missing" {
                            crate::types::JuliaType::Bottom
                        } else if type_name.starts_with("Union{") && type_name.ends_with('}') {
                            // Parse Union{T1, T2, ...} and filter out Missing
                            let inner = &type_name[6..type_name.len() - 1];
                            let types: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                            let filtered: Vec<crate::types::JuliaType> = types
                                .iter()
                                .filter(|&t| *t != "Missing")
                                .filter_map(|t| crate::types::JuliaType::from_name(t))
                                .collect();

                            match filtered.len() {
                                0 => crate::types::JuliaType::Bottom,
                                1 => {
                                    // Safety: len()==1 guarantees next() is Some
                                    match filtered.into_iter().next() {
                                        Some(t) => t,
                                        None => crate::types::JuliaType::Bottom,
                                    }
                                }
                                _ => crate::types::JuliaType::Union(filtered),
                            }
                        } else {
                            // Not a Union, return as-is
                            crate::types::JuliaType::from_name(type_name).unwrap_or_else(|| {
                                crate::types::JuliaType::Struct(type_name.to_string())
                            })
                        }
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "nonmissingtype requires a type, got {:?}",
                            type_val.value_type()
                        )));
                    }
                };

                self.stack.push(Value::DataType(Box::new(result_type)));
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::StructDef;
    use crate::runtime_types::struct_info::ParametricStructDef;
    use crate::types::JuliaType;
    use crate::types::TypeParam;
    use std::collections::HashMap;

    fn test_span() -> subset_julia_vm_ir::Span {
        subset_julia_vm_ir::Span::new(0, 0, 1, 1, 1, 1)
    }

    fn type_param(name: &str, lower: Option<&str>, upper: Option<&str>) -> TypeParam {
        TypeParam {
            name: name.to_string(),
            upper_bound: upper.map(str::to_string),
            lower_bound: lower.map(str::to_string),
            bound: upper.map(str::to_string),
        }
    }

    fn runtime_context_with_parametric_struct(
        name: &str,
        type_params: Vec<TypeParam>,
    ) -> RuntimeCompileContext {
        let mut parametric_structs = HashMap::new();
        parametric_structs.insert(
            name.to_string(),
            ParametricStructDef {
                def: StructDef {
                    name: name.to_string(),
                    is_mutable: false,
                    is_base_origin: false,
                    type_params,
                    parent_type: None,
                    fields: vec![],
                    inner_constructors: vec![],
                    span: test_span(),
                    global_new_helpers: Vec::new(),
                },
            },
        );
        RuntimeCompileContext {
            struct_table: subset_julia_vm_bytecode::StructRegistry::new(),
            struct_defs: vec![],
            parametric_structs,
            base_parametric_structs: HashMap::new(),
            type_aliases: HashMap::new(),
            module_imported_bindings: HashMap::new(),
            module_base_exports_visibility: HashMap::new(),
            module_implicit_standard_bindings: HashMap::new(),
            base_exported_names: Default::default(),
            inference_global_types: HashMap::new(),
            primitive_types: vec![],
            disable_array_getindex_specialization: false,
            disable_array_setindex_specialization: false,
            disable_field_access_specialization: false,
            module_registry: Default::default(),
        }
    }

    #[test]
    fn structured_typeintersect_keeps_subtype_side_issue_10460() {
        assert_eq!(
            structured_typeintersect(&JuliaType::Int64, &JuliaType::Real, CoreType::is_subtype_of),
            JuliaType::Int64
        );
        assert_eq!(
            structured_typeintersect(&JuliaType::Real, &JuliaType::Int64, CoreType::is_subtype_of),
            JuliaType::Int64
        );
        let concrete_array = JuliaType::from_name_or_struct("Array{Int64}");
        let dense_array = JuliaType::from_name_or_struct("DenseArray{Int64}");
        let concrete_array_core = CoreType::from(&concrete_array);
        let dense_array_core = CoreType::from(&dense_array);
        assert_eq!(
            structured_typeintersect(&dense_array, &concrete_array, |left, right| {
                left == &concrete_array_core && right == &dense_array_core
            },),
            concrete_array
        );
    }

    #[test]
    fn canonicalize_user_unionall_alias_requires_declared_params_issue_9563() {
        let ctx =
            runtime_context_with_parametric_struct("Box3909", vec![type_param("T", None, None)]);
        assert_eq!(
            canonicalize_user_unionall_alias(
                Some(&ctx),
                "T",
                &None,
                &None,
                &JuliaType::Struct("Box3909{T}".to_string()),
                None,
            ),
            Some(JuliaType::Struct("Box3909".to_string()))
        );
        assert_eq!(
            canonicalize_user_unionall_alias(
                Some(&ctx),
                "Q",
                &None,
                &None,
                &JuliaType::Struct("Box3909{Q}".to_string()),
                None,
            ),
            None
        );

        let bounded = runtime_context_with_parametric_struct(
            "BBox",
            vec![type_param("T", None, Some("Real"))],
        );
        assert_eq!(
            canonicalize_user_unionall_alias(
                Some(&bounded),
                "T",
                &None,
                &Some("Real".to_string()),
                &JuliaType::Struct("BBox{T}".to_string()),
                None,
            ),
            Some(JuliaType::Struct("BBox".to_string()))
        );
        assert_eq!(
            canonicalize_user_unionall_alias(
                Some(&bounded),
                "T",
                &None,
                &None,
                &JuliaType::Struct("BBox{T}".to_string()),
                None,
            ),
            None
        );

        let pair = runtime_context_with_parametric_struct(
            "PairBox",
            vec![type_param("A", None, None), type_param("B", None, None)],
        );
        let pair_body = JuliaType::UnionAll {
            lower_bound: None,
            var: "B".to_string(),
            bound: None,
            body: Box::new(JuliaType::Struct("PairBox{A, B}".to_string())),
        };
        assert_eq!(
            canonicalize_user_unionall_alias(Some(&pair), "A", &None, &None, &pair_body, None,),
            Some(JuliaType::Struct("PairBox".to_string()))
        );
        let partial_projection = JuliaType::RuntimeUnionAll {
            var: Box::new(JuliaType::RuntimeTypeVar {
                id: 104_602,
                name: "B".to_string(),
                lower_bound: Box::new(JuliaType::Bottom),
                upper_bound: Box::new(JuliaType::Any),
            }),
            body: Box::new(JuliaType::Struct("PairBox{Int8, B}".to_string())),
        };
        assert_eq!(
            canonicalize_user_unionall_alias(
                Some(&pair),
                "B",
                &None,
                &None,
                &JuliaType::Struct("PairBox{Int8, B}".to_string()),
                Some(&partial_projection),
            ),
            Some(partial_projection)
        );
        assert_eq!(
            canonicalize_user_unionall_alias(
                Some(&pair),
                "X",
                &None,
                &None,
                &JuliaType::Struct("PairBox{Int8, X}".to_string()),
                None,
            ),
            None,
            "a fresh alpha-renamed binder is equal but not identical to the declared partial alias"
        );
    }

    #[test]
    fn structured_typeintersect_handles_non_subtype_cases_issue_10460() {
        assert_eq!(
            structured_typeintersect(
                &JuliaType::String,
                &JuliaType::Number,
                CoreType::is_subtype_of,
            ),
            JuliaType::Bottom
        );
        assert_eq!(
            structured_typeintersect(
                &JuliaType::Struct("Dog".to_string()),
                &JuliaType::Any,
                CoreType::is_subtype_of,
            ),
            JuliaType::Struct("Dog".to_string())
        );

        crate::types::register_type_name("OwnerA10460.IntersectBox10460");
        crate::types::register_type_name("OwnerB10460.IntersectBox10460");
        assert_eq!(
            structured_typeintersect(
                &JuliaType::Struct("OwnerA10460.IntersectBox10460{Int64}".to_string()),
                &JuliaType::Struct("OwnerB10460.IntersectBox10460{Int64}".to_string()),
                CoreType::is_subtype_of,
            ),
            JuliaType::Bottom
        );
    }

    #[test]
    fn julia_type_references_typevar_scans_unionall_bounds_issue_7924() {
        let ty = JuliaType::UnionAll {
            lower_bound: None,
            var: "S".to_string(),
            bound: Some(Box::new("T".to_string())),
            body: Box::new(JuliaType::TupleOf(vec![JuliaType::TypeVar(
                "S".to_string(),
                None,
            )])),
        };

        assert!(julia_type_references_typevar(&ty, "T"));
        assert!(julia_type_references_typevar(
            &JuliaType::TypeVar("S".to_string(), Some("T".to_string())),
            "T"
        ));
    }

    #[test]
    fn type_name_references_typevar_ignores_module_qualified_issue_10280() {
        // A bare occurrence references the binder (the `where` is kept).
        assert!(type_name_references_typevar("Vector{Builtin}", "Builtin"));
        assert!(type_name_references_typevar("Builtin", "Builtin"));
        // A module-qualified reference whose last component equals the binder
        // is NOT a reference: upstream shadows only the bare name, so the
        // `where` binder is unused and the clause drops (Issue #10280).
        assert!(!type_name_references_typevar(
            "Vector{Core.Builtin}",
            "Builtin"
        ));
        // Generalizes to any qualified path, not a `Core.` special case.
        assert!(!type_name_references_typevar(
            "Vector{Base.RefValue{Int64}}",
            "RefValue"
        ));
        assert!(!type_name_references_typevar("A.B.Builtin", "Builtin"));
        // Mixed body: a later BARE occurrence still counts even when an earlier
        // occurrence of the same name was module-qualified (scan continues).
        assert!(type_name_references_typevar(
            "Tuple{Core.Builtin, Builtin}",
            "Builtin"
        ));
        // A binder matching the MODULE prefix (first component) is still a bare
        // reference (upstream then treats `Core` as a shadowed TypeVar).
        assert!(type_name_references_typevar("Core.Builtin", "Core"));
    }

    #[test]
    fn canonicalize_builtin_unionall_aliases_issue_5105() {
        assert_eq!(
            canonicalize_builtin_unionall_alias(
                "T",
                &None,
                &None,
                &JuliaType::Struct("Array{T, 1}".to_string()),
            ),
            Some(JuliaType::Struct("Vector".to_string()))
        );
        assert_eq!(
            canonicalize_builtin_unionall_alias(
                "T",
                &None,
                &None,
                &JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
            ),
            Some(JuliaType::Struct("Vector".to_string()))
        );
        assert_eq!(
            canonicalize_builtin_unionall_alias(
                "T",
                &None,
                &None,
                &JuliaType::Struct("Vector{T}".to_string()),
            ),
            Some(JuliaType::Struct("Vector".to_string()))
        );
        assert_eq!(
            canonicalize_builtin_unionall_alias(
                "S",
                &None,
                &None,
                &JuliaType::VectorOf(Box::new(JuliaType::TypeVar("S".to_string(), None))),
            ),
            None
        );
        assert_eq!(
            canonicalize_builtin_unionall_alias(
                "T",
                &None,
                &None,
                &JuliaType::Struct("DenseArray{T, 2}".to_string()),
            ),
            Some(JuliaType::Struct("DenseMatrix".to_string()))
        );
        assert_eq!(
            canonicalize_builtin_unionall_alias(
                "K",
                &None,
                &None,
                &JuliaType::UnionAll {
                    lower_bound: None,
                    var: "V".to_string(),
                    bound: None,
                    body: Box::new(JuliaType::Struct("Dict{K, V}".to_string())),
                },
            ),
            Some(JuliaType::Dict)
        );
        assert_eq!(
            canonicalize_builtin_unionall_alias(
                "T",
                &None,
                &Some("Number".to_string()),
                &JuliaType::Struct("Array{T, 1}".to_string()),
            ),
            None
        );
    }

    #[test]
    fn canonicalize_projected_runtime_unionall_aliases_10613() {
        assert_eq!(
            canonicalize_builtin_unionall_alias(
                "T",
                &None,
                &None,
                &JuliaType::RuntimeParametric {
                    base: "Array".to_string(),
                    params: vec![
                        JuliaType::TypeVar("T".to_string(), None),
                        JuliaType::Struct("1".to_string()),
                    ],
                },
            ),
            Some(JuliaType::Struct("Vector".to_string()))
        );
        assert_eq!(
            canonicalize_builtin_unionall_alias(
                "K",
                &None,
                &None,
                &JuliaType::UnionAll {
                    lower_bound: None,
                    var: "V".to_string(),
                    bound: None,
                    body: Box::new(JuliaType::RuntimeParametric {
                        base: "Dict".to_string(),
                        params: vec![
                            JuliaType::TypeVar("K".to_string(), None),
                            JuliaType::TypeVar("V".to_string(), None),
                        ],
                    }),
                },
            ),
            Some(JuliaType::Dict)
        );
    }

    #[test]
    fn runtime_type_projection_type_relation_issue_3909() {
        assert!(runtime_type_projection_is_subtype_of_type(
            &JuliaType::DataType
        ));
        assert!(runtime_type_projection_is_subtype_of_type(
            &JuliaType::Struct("UnionAll".to_string())
        ));
        assert!(!runtime_type_projection_is_subtype_of_type(
            &JuliaType::Struct("TypeVar".to_string())
        ));
    }
}

#[cfg(test)]
mod issue_10460_rebind_tests {
    use super::rebind_runtime_unionall_source_body;
    use crate::types::JuliaType;

    #[test]
    fn source_rebind_reaches_nested_runtime_binder_bounds_10460() -> Result<(), String> {
        let outer = JuliaType::RuntimeTypeVar {
            id: 10460,
            name: "Int64".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Real),
        };
        let free_same_name = JuliaType::RuntimeTypeVar {
            id: 10464,
            name: "Int64".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::String),
        };
        let inner = JuliaType::RuntimeTypeVar {
            id: 10461,
            name: "S".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(free_same_name.clone()),
        };
        let body = JuliaType::RuntimeUnionAll {
            var: Box::new(inner.clone()),
            body: Box::new(JuliaType::RuntimeParametric {
                base: "Pair".to_string(),
                params: vec![inner, free_same_name],
            }),
        };
        let mut counter = 10462;
        let rebound = rebind_runtime_unionall_source_body(&body, "Int64", &outer, &mut counter);
        let JuliaType::RuntimeUnionAll { var, body } = rebound else {
            return Err("nested runtime wrapper must remain identity-bearing".to_string());
        };
        let JuliaType::RuntimeTypeVar { upper_bound, .. } = var.as_ref() else {
            return Err("nested runtime binder must remain a runtime TypeVar".to_string());
        };
        assert!(matches!(
            upper_bound.as_ref(),
            JuliaType::RuntimeTypeVar { id: 10464, .. }
        ));
        let JuliaType::RuntimeParametric { params, .. } = body.as_ref() else {
            return Err("nested runtime body must remain structured".to_string());
        };
        assert!(matches!(
            params.as_slice(),
            [_, JuliaType::RuntimeTypeVar { id: 10464, .. }]
        ));

        let legacy = JuliaType::UnionAll {
            var: "S".to_string(),
            lower_bound: None,
            bound: Some(Box::new("Int64".to_string())),
            body: Box::new(JuliaType::Struct("Vector{S}".to_string())),
        };
        let rebound = rebind_runtime_unionall_source_body(&legacy, "Int64", &outer, &mut counter);
        let JuliaType::RuntimeUnionAll { var, .. } = rebound else {
            return Err("nested legacy wrapper must be promoted to a runtime wrapper".to_string());
        };
        let JuliaType::RuntimeTypeVar { upper_bound, .. } = var.as_ref() else {
            return Err("promoted nested binder must remain a runtime TypeVar".to_string());
        };
        assert!(matches!(
            upper_bound.as_ref(),
            JuliaType::RuntimeTypeVar { id: 10460, .. }
        ));

        let opaque = JuliaType::TupleOf(vec![JuliaType::Struct(
            "Vector{S} where S<:Int64".to_string(),
        )]);
        let rebound = rebind_runtime_unionall_source_body(&opaque, "Int64", &outer, &mut counter);
        let JuliaType::TupleOf(elements) = rebound else {
            return Err("opaque nested wrapper must retain its tuple owner".to_string());
        };
        let [JuliaType::RuntimeUnionAll { var, .. }] = elements.as_slice() else {
            return Err(
                "opaque nested where must be promoted after CoreType projection".to_string(),
            );
        };
        let JuliaType::RuntimeTypeVar { upper_bound, .. } = var.as_ref() else {
            return Err("promoted opaque binder must remain a runtime TypeVar".to_string());
        };
        assert!(matches!(
            upper_bound.as_ref(),
            JuliaType::RuntimeTypeVar { id: 10460, .. }
        ));
        Ok(())
    }
}
