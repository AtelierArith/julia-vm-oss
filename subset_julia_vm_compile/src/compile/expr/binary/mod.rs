//! Binary operation compilation.
//!
//! Handles compilation of:
//! - User-defined binary operator overloads
//! - Builtin binary operators (arithmetic, comparison)
//! - Type promotion and intrinsic dispatch

mod builtin;
mod user_defined;

use std::collections::HashSet;

use crate::builtins::BuiltinId;
use crate::bytecode::{Instr, ValueType};
use crate::compile::lattice::types::ConstValue;
use crate::inference_core::dispatch_resolver::{
    binary_dispatch_compare_enabled, binary_dispatch_compare_log, binary_static_verdict,
    BinaryStaticVerdict,
};
use crate::inference_core::{CoreAbstract, CorePrimitive, CoreType};
use crate::intrinsics::Intrinsic;
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal};
use crate::types::{DispatchError, JuliaType};
use subset_julia_vm_bytecode::typed_scalar_binary_instr;

use crate::compile::inference::promote_numeric_value_types;
use crate::compile::{
    binary_op_to_function_name, err, is_float_type, is_integer_type, is_numeric_type,
    is_singleton_type, julia_type_to_value_type, CResult, CoreCompiler, MethodSig,
};

/// Compare-mode hook for binary dispatch (Issue #8620, parent #8609).
///
/// Called with the operand `ValueType`s and the compile-time decision
/// (`"UniqueBuiltin"` or `"NeedsRuntime"`) for each binary call site when
/// `SJULIA_BINARY_DISPATCH_COMPARE=1`.  Converts the `ValueType` pair to
/// `LatticeType` via the bridge, calls [`binary_static_verdict`], and logs a
/// `SJULIA_BINARY_DISPATCH_COMPARE` line to stderr if the resolver verdict
/// differs from the compile-time decision.
///
/// This function is a pure annotation — it does **not** modify the emitted
/// bytecode or change any dispatch decision.
pub(super) fn binary_compare_check(
    op: &BinaryOp,
    left_vt: &ValueType,
    right_vt: &ValueType,
    compile_choice: &str,
) {
    if !binary_dispatch_compare_enabled() {
        return;
    }
    let left_lattice = crate::runtime_types::bridge::value_type_to_lattice(left_vt);
    let right_lattice = crate::runtime_types::bridge::value_type_to_lattice(right_vt);
    let resolver_verdict = binary_static_verdict(&left_lattice, &right_lattice);
    let verdict_str = match resolver_verdict {
        BinaryStaticVerdict::UniqueBuiltin => "UniqueBuiltin",
        BinaryStaticVerdict::NeedsRuntime => "NeedsRuntime",
        BinaryStaticVerdict::NoCandidates => "NoCandidates",
    };
    if compile_choice != verdict_str {
        binary_dispatch_compare_log(format_args!(
            "SJULIA_BINARY_DISPATCH_COMPARE: op={op:?} left={left_vt:?} right={right_vt:?} compile={compile_choice} resolver={verdict_str}"
        ));
    }
}

/// Determine if both operands are the same small integer type (Issue #2278).
/// In Julia, `Int8(1) + Int8(2)` returns `Int8`, not `Int64`.
/// Returns Some(ValueType) for the preserved type, None if not a same-type small int pair.
pub(super) fn same_small_int_type(left: &ValueType, right: &ValueType) -> Option<ValueType> {
    // Both operands must be the same type
    if left != right {
        return None;
    }
    match left {
        ValueType::I8
        | ValueType::I16
        | ValueType::I32
        | ValueType::U8
        | ValueType::U16
        | ValueType::U32
        | ValueType::U64 => Some(left.clone()),
        _ => None,
    }
}

/// Issue #9123: Maximum Union component count for the small-union arithmetic fast path.
///
/// When a lattice-inferred result carries `Union{Int64, Float64}` (typical for
/// a call to a function whose loop phi-joins an Int64 initializer with a
/// Float64 update), the binary-op compiler would otherwise treat the operand
/// like `Any` — a fully dynamic `+` call per evaluation, which also degrades
/// the destination slot to boxed `unknown`.  This constant caps the number of
/// union alternatives we are willing to union-split: if the Union has
/// ≤ `MAX_UNION_SPLITTING` components and EVERY component is a promotable
/// machine numeric (I8..U128, Bool, F16/F32/F64), we can replace the dynamic
/// dispatch with a single `DynamicToF64` + typed float instruction, which is
/// semantically equivalent for float-result arithmetic.
///
/// 4 matches upstream Julia's own `MAX_UNION_SPLITTING` constant.
const MAX_UNION_SPLITTING: usize = 4;

/// Check whether a `ValueType` is a small Union whose members are ALL
/// promotable machine numerics (I8..U128, Bool, F16/F32/F64).
///
/// Such a Union can be coerced to `F64` with a single `DynamicToF64` — the
/// exact conversion Julia's `promote` performs when the OTHER operand of an
/// arithmetic op is a concrete `Float64` (F64 dominates every machine numeric
/// in the promotion lattice, so the result is always `Float64`).
///
/// NOTE this predicate alone does NOT make float-promotion sound: e.g.
/// `Union{Int64, Float64} + Int64` is `Int64` in Julia when the union holds an
/// Int64 at runtime.  The caller must additionally require the other operand
/// to be a concrete `F64` (see the Issue #9123 fast path in
/// [`CoreCompiler::compile_binary_op`]).
fn small_all_machine_numeric_union(ty: &ValueType) -> bool {
    fn is_promotable_machine_numeric(t: &ValueType) -> bool {
        matches!(
            t,
            ValueType::I8
                | ValueType::I16
                | ValueType::I32
                | ValueType::I64
                | ValueType::U8
                | ValueType::U16
                | ValueType::U32
                | ValueType::U64
                | ValueType::I128
                | ValueType::U128
                | ValueType::Bool
                | ValueType::F16
                | ValueType::F32
                | ValueType::F64
        )
    }
    let ValueType::Union(members) = ty else {
        return false;
    };
    !members.is_empty()
        && members.len() <= MAX_UNION_SPLITTING
        && members.iter().all(is_promotable_machine_numeric)
}

/// Issue #8183: is `left op right` (for `op` ∈ `+ - * /`) a mixed-type primitive
/// pair whose result is exactly a Julia float promotion?
///
/// When the two operands are *different* concrete machine numeric types and at
/// least one is a float, Julia's `+`/`-`/`*`/`/` promotes the integer operand to
/// the float type and computes in that float (`Float64(int) op float`) — which is
/// bit-identical to emitting `…ToF64; <op>F64` through the typed builtin path
/// (`compile_builtin_binary_op`). Specializing it avoids a per-execution dynamic
/// method `Call` to the Base operator and keeps the loop body on typed
/// instructions so the native typed-loop recognizer (`vm::executable`) can match.
///
/// Bounded to ≤64-bit integers / `Bool` / machine floats, where the builtin
/// path's `compile_expr_as` promotion to the float operand type is well-defined;
/// `Int128`/`UInt128`/`BigInt`/`BigFloat` fall back to exact method dispatch.
///
/// Comparisons (`==`, `<`, …) are deliberately NOT covered: an integer-vs-float
/// comparison must keep Julia's exact semantics (correct beyond 2^53), which a
/// promote-to-Float64 would break.
pub(super) fn mixed_float_arith_specializable(left: &ValueType, right: &ValueType) -> bool {
    fn promotable_machine_numeric(t: &ValueType) -> bool {
        matches!(
            t,
            ValueType::I8
                | ValueType::I16
                | ValueType::I32
                | ValueType::I64
                | ValueType::U8
                | ValueType::U16
                | ValueType::U32
                | ValueType::U64
                | ValueType::Bool
                | ValueType::F16
                | ValueType::F32
                | ValueType::F64
        )
    }
    fn is_machine_float(t: &ValueType) -> bool {
        matches!(t, ValueType::F16 | ValueType::F32 | ValueType::F64)
    }
    left != right
        && promotable_machine_numeric(left)
        && promotable_machine_numeric(right)
        && (is_machine_float(left) || is_machine_float(right))
}

/// Get the DynamicTo* back-conversion instruction for a small integer ValueType (Issue #2278).
/// Returns the instruction that converts an I64 result back to the original small integer type.
pub(super) fn small_int_back_conversion(ty: &ValueType) -> Option<Instr> {
    match ty {
        ValueType::I8 => Some(Instr::DynamicToI8),
        ValueType::I16 => Some(Instr::DynamicToI16),
        ValueType::I32 => Some(Instr::DynamicToI32),
        ValueType::U8 => Some(Instr::DynamicToU8),
        ValueType::U16 => Some(Instr::DynamicToU16),
        ValueType::U32 => Some(Instr::DynamicToU32),
        ValueType::U64 => Some(Instr::DynamicToU64),
        _ => None,
    }
}

/// Map a typed integer/float arithmetic or comparison intrinsic onto the shared
/// scalar binary-op table ([`typed_scalar_binary_instr`], Issue #8192).
/// Intrinsics with no typed I64/F64 instruction (e.g. `SdivInt`, `NegInt`)
/// return `None`, exactly as before the unification.
pub(super) fn typed_instr_for_intrinsic(intrinsic: Intrinsic) -> Option<Instr> {
    let (op, result_is_float) = match intrinsic {
        Intrinsic::AddInt => (BinaryOp::Add, false),
        Intrinsic::SubInt => (BinaryOp::Sub, false),
        Intrinsic::MulInt => (BinaryOp::Mul, false),
        Intrinsic::SremInt => (BinaryOp::Mod, false),
        Intrinsic::EqInt => (BinaryOp::Eq, false),
        Intrinsic::NeInt => (BinaryOp::Ne, false),
        Intrinsic::SltInt => (BinaryOp::Lt, false),
        Intrinsic::SleInt => (BinaryOp::Le, false),
        Intrinsic::SgtInt => (BinaryOp::Gt, false),
        Intrinsic::SgeInt => (BinaryOp::Ge, false),
        Intrinsic::DynamicAdd => (BinaryOp::Add, true),
        Intrinsic::DynamicSub => (BinaryOp::Sub, true),
        Intrinsic::DynamicMul => (BinaryOp::Mul, true),
        Intrinsic::DynamicDiv => (BinaryOp::Div, true),
        Intrinsic::EqFloat => (BinaryOp::Eq, true),
        Intrinsic::NeFloat => (BinaryOp::Ne, true),
        Intrinsic::LtFloat => (BinaryOp::Lt, true),
        Intrinsic::LeFloat => (BinaryOp::Le, true),
        Intrinsic::GtFloat => (BinaryOp::Gt, true),
        Intrinsic::GeFloat => (BinaryOp::Ge, true),
        _ => return None,
    };
    typed_scalar_binary_instr(op, result_is_float)
}

fn is_array_value_type(ty: &ValueType) -> bool {
    matches!(ty, ValueType::Array | ValueType::ArrayOf(_, _))
}

fn is_memory_value_type(ty: &ValueType) -> bool {
    matches!(ty, ValueType::Memory | ValueType::MemoryOf(_))
}

fn is_array_or_memory_value_type(ty: &ValueType) -> bool {
    is_array_value_type(ty) || is_memory_value_type(ty)
}

fn is_scalar_numeric_or_complex_value_type(ty: &ValueType) -> bool {
    // Complex scalars may be represented by dedicated ValueTypes rather than Struct(_).
    // Keep array-scalar operators on the dynamic fallback path (Issue #6294).
    is_numeric_type(ty)
        || matches!(
            ty,
            ValueType::Struct(_) | ValueType::ComplexF32 | ValueType::ComplexF64
        )
}

fn is_slice_index_expr(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Index { indices, .. }
            if indices
                .iter()
                .any(|idx| matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. }))
    )
}

fn is_exact_i64_literal_in_f64(expr: &Expr) -> bool {
    const MAX_EXACT_I64_IN_F64: u64 = 1 << 53;
    matches!(
        expr,
        Expr::Literal(crate::ir::core::Literal::Int(value), _)
            if value.unsigned_abs() <= MAX_EXACT_I64_IN_F64
    )
}

/// Retired from production at Issue #6495 stage 7c-ii: the projection-side
/// reads now consume the `core_signature` projection only. Retained as the
/// parity-gate / unit-test oracle until the projection fields are deleted.
#[cfg(test)]
pub(crate) fn is_linalg_array_dispatch_type(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::MatrixOf(_) | JuliaType::VectorOf(_) | JuliaType::AbstractArray => true,
        JuliaType::Struct(name) | JuliaType::AbstractUser(name, _) => {
            let base = name.find('{').map_or(name.as_str(), |idx| &name[..idx]);
            matches!(
                base,
                "Matrix" | "Vector" | "AbstractMatrix" | "AbstractVector"
            )
        }
        _ => false,
    }
}

fn is_diagonal_julia_type(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::Struct(name) | JuliaType::AbstractUser(name, _) => {
            let base = name
                .rsplit('.')
                .next()
                .unwrap_or(name.as_str())
                .split('{')
                .next()
                .unwrap_or(name.as_str());
            base == "Diagonal"
        }
        _ => false,
    }
}

fn is_diagonal_constructor_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Call { function, .. } => {
            function == "Diagonal"
                || function.ends_with(".Diagonal")
                || function.ends_with("::Diagonal")
        }
        Expr::ModuleCall { function, .. } => function == "Diagonal",
        _ => false,
    }
}

fn mul_involves_diagonal(
    left: &Expr,
    right: &Expr,
    left_ty: &JuliaType,
    right_ty: &JuliaType,
) -> bool {
    is_diagonal_julia_type(left_ty)
        || is_diagonal_julia_type(right_ty)
        || is_diagonal_constructor_expr(left)
        || is_diagonal_constructor_expr(right)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArrayDispatchRank {
    Vector,
    Matrix,
    Unknown,
}

pub(crate) fn linalg_array_dispatch_rank(ty: &JuliaType) -> Option<ArrayDispatchRank> {
    match ty {
        JuliaType::VectorOf(_) => Some(ArrayDispatchRank::Vector),
        JuliaType::MatrixOf(_) => Some(ArrayDispatchRank::Matrix),
        JuliaType::Array | JuliaType::AbstractArray => Some(ArrayDispatchRank::Unknown),
        JuliaType::Struct(name) | JuliaType::AbstractUser(name, _) => {
            let base = name.find('{').map_or(name.as_str(), |idx| &name[..idx]);
            match base {
                "Vector" | "AbstractVector" => Some(ArrayDispatchRank::Vector),
                "Matrix" | "AbstractMatrix" => Some(ArrayDispatchRank::Matrix),
                "Array" | "AbstractArray" => Some(ArrayDispatchRank::Unknown),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Retired from production at Issue #6495 stage 7c-ii: the projection-side
/// reads now consume the `core_signature` projection only. Retained as the
/// parity-gate / unit-test oracle until the projection fields are deleted.
#[cfg(test)]
pub(crate) fn is_string_concat_dispatch_type(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::String
        | JuliaType::Char
        | JuliaType::AbstractString
        | JuliaType::AbstractChar => true,
        JuliaType::Union(types) => types.iter().all(is_string_concat_dispatch_type),
        _ => false,
    }
}

/// True for the StaticArrays struct families (`SVector`, `SMatrix`, `SArray`,
/// the mutable `M*` variants, and the abstract `StaticArray`/`StaticVector`/
/// `StaticMatrix` supertypes). These are `AbstractArray` subtypes carried as
/// `JuliaType::Struct` rather than a native array `ValueType`, so equality
/// against a native `Array`/`Vector` must compare element values at runtime
/// rather than through the static `==` identity fallback (Issue #8132).
///
/// Fast path for the binary `==`/`!=` array-routing decision: a hardcoded name
/// list of the StaticArrays struct families. It is a guaranteed, zero-cost,
/// zero-regression cover for the only bundled package whose structs carry an
/// `AbstractArray` supertype. The GENERAL case — any user-defined
/// `AbstractArray`-subtype struct hit by a #8132-style override return-type
/// mismatch — is now also handled, by `CoreCompiler::is_abstractarray_subtype_struct`
/// falling back to the strict registered-hierarchy walk
/// `struct_is_registered_subtype_of_abstract` when a type is not in this list
/// (Issue #8149). The strict walk (no "conservatively accept unknown struct"
/// branch) is what makes the general resolution safe on this global compile path.
pub(crate) fn is_static_array_struct_julia_type(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::Struct(name) | JuliaType::AbstractUser(name, _) => {
            let short = name.rsplit('.').next().unwrap_or(name.as_str());
            let base = short.split('{').next().unwrap_or(short);
            matches!(
                base,
                "SVector"
                    | "SMatrix"
                    | "SArray"
                    | "MVector"
                    | "MMatrix"
                    | "MArray"
                    | "StaticArray"
                    | "StaticVector"
                    | "StaticMatrix"
                    | "StaticScalar"
                    | "StaticVecOrMat"
                    | "FieldVector"
            )
        }
        _ => false,
    }
}

pub(crate) fn is_user_array_runtime_dispatch_candidate_type(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::VectorOf(_)
        | JuliaType::MatrixOf(_)
        | JuliaType::Array
        | JuliaType::AbstractArray => true,
        JuliaType::Struct(name) | JuliaType::AbstractUser(name, _) => {
            let base = name.find('{').map_or(name.as_str(), |idx| &name[..idx]);
            matches!(
                base,
                "Array" | "Matrix" | "Vector" | "AbstractMatrix" | "AbstractVector"
            )
        }
        _ => false,
    }
}

/// Retired from production at Issue #6495 stage 7c-ii: the projection-side
/// reads now consume the `core_signature` projection only. Retained as the
/// parity-gate / unit-test oracle until the projection fields are deleted.
#[cfg(test)]
pub(crate) fn is_binary_runtime_dispatch_candidate_type(ty: &JuliaType) -> bool {
    matches!(
        ty,
        JuliaType::Struct(_)
            | JuliaType::AbstractUser(_, _)
            | JuliaType::Number
            | JuliaType::Real
            | JuliaType::Integer
            | JuliaType::Signed
            | JuliaType::Unsigned
            | JuliaType::AbstractFloat
            | JuliaType::String
            | JuliaType::AbstractString
            | JuliaType::Symbol
            | JuliaType::Bool
            | JuliaType::Char
            | JuliaType::AbstractChar
            | JuliaType::Dict
            | JuliaType::Set
            | JuliaType::Type
            | JuliaType::DataType
            | JuliaType::TypeOf(_)
    )
}

fn is_irrational_dispatch_type(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::Struct(name) => {
            let base = name.rsplit('.').next().unwrap_or(name);
            base == "Irrational" || base.starts_with("Irrational{")
        }
        JuliaType::AbstractUser(name, _) => name == "AbstractIrrational",
        _ => false,
    }
}

fn is_vm_known_irrational_type(ty: &JuliaType) -> bool {
    matches!(
        irrational_struct_symbol(ty),
        Some("Irrational{:π}" | "Irrational{:ℯ}")
    )
}

/// Operand ValueTypes for which the Float64/Float32 irrational-constant fast
/// path in `compile_binary_op` matches the pure-Julia irrational methods it is
/// short-cutting. `+(x::Integer, y::AbstractIrrational) = Float64(x) + Float64(y)`
/// yields `Float64` for every integer (including `Bool` and `BigInt`), and
/// `+(x::AbstractFloat, y::AbstractIrrational) = x + typeof(x)(y)` yields
/// `Float64`/`Float32` for `Float64`/`Float32` operands, so forcing both
/// operands to `Float64` (or `Float32`) is exact for those types
/// (base/irrationals.jl).
///
/// This is a WHITELIST: it deliberately EXCLUDES the types whose method result
/// is *wider* than the fast path would produce — `Float16` (the mixed method
/// converts the irrational to `Float16`, so `Float16 + pi -> Float16`, not
/// `Float64`), `BigFloat` (`-> BigFloat` at the active precision, Issue #9317)
/// and `BigInt` (`-> BigFloat` at the active precision via the pure-Julia
/// `+(::AbstractIrrational, ::BigInt) = BigFloat(x) + BigFloat(y)` methods,
/// Issue #9341/#9317) — plus any `Any`/`Union`/struct/unknown operand, whose
/// runtime type is not known at compile time. All of those must promote through
/// pure-Julia method dispatch instead so the wider type is preserved.
fn is_irrational_fast_path_concrete(ty: &ValueType) -> bool {
    matches!(
        ty,
        ValueType::Bool
            | ValueType::I8
            | ValueType::I16
            | ValueType::I32
            | ValueType::I64
            | ValueType::I128
            | ValueType::U8
            | ValueType::U16
            | ValueType::U32
            | ValueType::U64
            | ValueType::U128
            | ValueType::F32
            | ValueType::F64
    )
}

/// Dedupe `(func_index, left_name, right_name)` candidates on their rendered
/// `(left, right)` signature, keeping the first method per signature, and
/// project the survivors onto the structured index-only payload
/// (Issue #6496: the runtime re-derives the names from `FunctionInfo`).
fn dedupe_binary_candidates_keep_first(candidates: Vec<(usize, String, String)>) -> Vec<usize> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::with_capacity(candidates.len());
    for (func_index, left, right) in candidates {
        if seen.insert((left, right)) {
            deduped.push(func_index);
        }
    }
    deduped
}

fn is_irrational_struct_julia_type(ty: &JuliaType) -> bool {
    irrational_struct_symbol(ty).is_some()
}

fn irrational_struct_symbol(ty: &JuliaType) -> Option<&str> {
    match ty {
        JuliaType::Struct(name) => {
            let base = name.rsplit('.').next().unwrap_or(name);
            if base.starts_with("Irrational{") {
                Some(base)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn is_dispatch_first_equality_type(ty: &JuliaType) -> bool {
    matches!(
        ty,
        JuliaType::String
            | JuliaType::Symbol
            | JuliaType::Bool
            | JuliaType::Char
            | JuliaType::Dict
            | JuliaType::Set
            | JuliaType::Type
            | JuliaType::DataType
            | JuliaType::TypeOf(_)
            // `Expr`/`QuoteNode` are struct-like AST datatypes whose `==` is
            // field-structural (Base/expr.jl), not numeric. Route their scalar
            // `==`/`!=` to `==` method dispatch like String/Symbol instead of
            // the numeric fast path, which would try to coerce `Expr` to `I64`
            // and error `Cannot convert Expr to I64` (Issue #9183).
            | JuliaType::Expr
            | JuliaType::QuoteNode
    ) || is_irrational_struct_julia_type(ty)
}

// ---------------------------------------------------------------------------
// CoreType-native ports of the binary dispatch-candidate heuristics
// (Issue #6495, stage 6b-ii).
//
// Port rule (same as the stage 6b-i collect/iterate ports in expr/builtin.rs):
// each `core_*` predicate equals its legacy predicate composed with the
// canonical inverse `inference_core::core_type_to_julia_type`, evaluated on
// the `core_signature` projection. Where the `JuliaType -> CoreType` bridge is
// non-injective the port follows the spelling the canonical inverse
// reconstructs (which IS what the legacy reads see post-deserialization).
// Base-corpus parity with the legacy predicates is pinned by
// `compile::cache::tests::base_method_core_binary_heuristics_parity_issue_6495`.
// ---------------------------------------------------------------------------

/// Read of a binary method's declared parameter pair on the structured
/// `core_signature` projection. False for non-binary arities. Stage 7c-ii:
/// the legacy `params` fallback is retired — a test-only `Bottom`
/// placeholder (unobservable in production since stage 7b) never matches.
fn method_binary_params_match(
    method: &MethodSig,
    core_pred: impl FnOnce(&CoreType, &CoreType) -> bool,
) -> bool {
    if method.param_count() != 2 {
        return false;
    }
    match method.structured_arg_core_types() {
        Some(cores) => core_pred(&cores[0], &cores[1]),
        None => false,
    }
}

/// `name` with its `{...}` parameter suffix stripped — the same base-name
/// extraction the legacy heuristics apply to `JuliaType::Struct` /
/// `JuliaType::AbstractUser` name spellings.
fn type_name_base(name: &str) -> &str {
    name.find('{').map_or(name, |idx| &name[..idx])
}

/// CoreType-native port of [`is_linalg_array_dispatch_type`]. Core `Struct`
/// names are already module- and parameter-stripped; `Struct {"Vector", 1}` /
/// `Struct {"Matrix", 1}` follow the `VectorOf`/`MatrixOf` verdict (true),
/// `Struct {"Array", 0}` follows the bare `JuliaType::Array` verdict (false —
/// the legacy base-name list has no `Array`), and the dedicated-variant-less
/// abstracts `AbstractVector`/`AbstractMatrix` follow their
/// `JuliaType::Struct(name)` spelling (true).
pub(crate) fn core_is_linalg_array_dispatch_type(core: &CoreType) -> bool {
    let base_in_list = |name: &str| {
        matches!(
            type_name_base(name),
            "Matrix" | "Vector" | "AbstractMatrix" | "AbstractVector"
        )
    };
    match core {
        CoreType::Abstract(
            CoreAbstract::AbstractArray
            | CoreAbstract::AbstractVector
            | CoreAbstract::AbstractMatrix,
        ) => true,
        CoreType::Struct { name, .. } => base_in_list(name),
        CoreType::AbstractUser { name, .. } | CoreType::Named(name) => base_in_list(name),
        _ => false,
    }
}

/// CoreType-native port of [`linalg_array_dispatch_rank`] for the method-param
/// (`expected`) side of the linalg compatibility checks.
pub(crate) fn core_linalg_array_dispatch_rank(core: &CoreType) -> Option<ArrayDispatchRank> {
    let base_rank = |name: &str| match type_name_base(name) {
        "Vector" | "AbstractVector" => Some(ArrayDispatchRank::Vector),
        "Matrix" | "AbstractMatrix" => Some(ArrayDispatchRank::Matrix),
        "Array" | "AbstractArray" => Some(ArrayDispatchRank::Unknown),
        _ => None,
    };
    match core {
        CoreType::Abstract(CoreAbstract::AbstractArray) => Some(ArrayDispatchRank::Unknown),
        CoreType::Abstract(CoreAbstract::AbstractVector) => Some(ArrayDispatchRank::Vector),
        CoreType::Abstract(CoreAbstract::AbstractMatrix) => Some(ArrayDispatchRank::Matrix),
        CoreType::Struct { name, .. } => base_rank(name),
        CoreType::AbstractUser { name, .. } | CoreType::Named(name) => base_rank(name),
        _ => None,
    }
}

/// CoreType-native port of [`linalg_array_candidate_compatible`] with the
/// method-param (`expected`) side read from the `core_signature` projection.
fn core_linalg_array_candidate_compatible(
    actual_value_ty: &ValueType,
    actual: &JuliaType,
    expected: &CoreType,
) -> bool {
    let Some(expected_rank) = core_linalg_array_dispatch_rank(expected) else {
        return false;
    };
    if is_array_value_type(actual_value_ty) {
        return true;
    }
    let Some(actual_rank) = linalg_array_dispatch_rank(actual) else {
        return true;
    };
    matches!(expected_rank, ArrayDispatchRank::Unknown)
        || matches!(actual_rank, ArrayDispatchRank::Unknown)
        || actual_rank == expected_rank
}

/// CoreType-native port of [`linalg_array_candidate_compatible_for_value_type`].
fn core_linalg_array_candidate_compatible_for_value_type(
    actual: &JuliaType,
    actual_value_type: &ValueType,
    expected: &CoreType,
) -> bool {
    if matches!(
        actual_value_type,
        ValueType::Array | ValueType::ArrayOf(_, _)
    ) && core_linalg_array_dispatch_rank(expected).is_some()
    {
        return true;
    }
    core_linalg_array_candidate_compatible(actual_value_type, actual, expected)
}

/// CoreType-native port of [`is_string_concat_dispatch_type`]: arm-for-arm
/// (the legacy `Union` recursion maps elementwise onto `CoreType::Union`).
pub(crate) fn core_is_string_concat_dispatch_type(core: &CoreType) -> bool {
    match core {
        CoreType::Primitive(CorePrimitive::String | CorePrimitive::Char) => true,
        CoreType::Abstract(CoreAbstract::AbstractString | CoreAbstract::AbstractChar) => true,
        CoreType::Union(arms) => arms.iter().all(core_is_string_concat_dispatch_type),
        _ => false,
    }
}

/// CoreType-native port of [`is_user_array_runtime_dispatch_candidate_type`].
pub(crate) fn core_is_user_array_runtime_dispatch_candidate_type(core: &CoreType) -> bool {
    let base_in_list = |name: &str| {
        matches!(
            type_name_base(name),
            "Array" | "Matrix" | "Vector" | "AbstractMatrix" | "AbstractVector"
        )
    };
    match core {
        CoreType::Abstract(
            CoreAbstract::AbstractArray
            | CoreAbstract::AbstractVector
            | CoreAbstract::AbstractMatrix,
        ) => true,
        CoreType::Struct { name, .. } => base_in_list(name),
        CoreType::AbstractUser { name, .. } | CoreType::Named(name) => base_in_list(name),
        _ => false,
    }
}

/// CoreType-native port of [`is_binary_runtime_dispatch_candidate_type`].
///
/// The legacy `JuliaType::Struct(_)` arm accepts every struct-name spelling,
/// so the port accepts exactly the images whose canonical inverse keeps a
/// `Struct` spelling: `Struct` images EXCEPT the `(name, len)` pairs whose
/// inverse normalizes to a dedicated non-`Struct` variant, the
/// dedicated-variant-less abstracts, plus `Named`/`NamedTuple`/`Vararg`/
/// `VarargLen`/`Value` images. The dedicated abstract variants in the legacy
/// accept list (`Number`/`Real`/... /`Type`/`DataType`) map arm-for-arm; the
/// rejected ones are exactly `AbstractArray`/`AbstractRange`/`Function`/`IO`.
pub(crate) fn core_is_binary_runtime_dispatch_candidate_type(core: &CoreType) -> bool {
    match core {
        CoreType::Primitive(p) => matches!(
            p,
            CorePrimitive::String
                | CorePrimitive::Symbol
                | CorePrimitive::Bool
                | CorePrimitive::Char
        ),
        CoreType::Abstract(a) => !matches!(
            a,
            CoreAbstract::AbstractArray
                | CoreAbstract::AbstractRange
                | CoreAbstract::Function
                | CoreAbstract::IO
        ),
        CoreType::AbstractUser { .. } | CoreType::TypeOf(_) => true,
        CoreType::Struct { name, params } => !matches!(
            (name.as_str(), params.len()),
            ("Vector", 1)
                | ("Matrix", 1)
                | ("Tuple", 0)
                | ("Array", 0)
                | ("NamedTuple", 0)
                | ("UnitRange", 0)
                | ("StepRange", 0)
                | ("Generator", 0)
                | ("IOBuffer", 0)
                | ("Expr", 0)
                | ("QuoteNode", 0)
                | ("LineNumberNode", 0)
                | ("GlobalRef", 0)
        ),
        CoreType::Named(_)
        | CoreType::NamedTuple(_)
        | CoreType::Vararg(_)
        | CoreType::VarargLen { .. }
        | CoreType::Value(_) => true,
        _ => false,
    }
}

/// CoreType-native port of [`is_dispatch_first_equality_type`]: arm-for-arm
/// (every legacy arm has a dedicated `CoreType` image and vice versa).
pub(crate) fn core_is_dispatch_first_equality_type(core: &CoreType) -> bool {
    // `JuliaType::Expr` / `JuliaType::QuoteNode` map to a zero-param
    // `CoreType::Struct { name, params: [] }` through the canonical bridge, so
    // their arm here is a `Struct`-name check rather than a dedicated variant.
    // Keep in lockstep with the `Expr`/`QuoteNode` arms of
    // `is_dispatch_first_equality_type` (parity pinned by the #6495 base-corpus
    // gate; Issue #9183).
    if let CoreType::Struct { name, params } = core {
        if name == "Irrational" && params.len() == 1 {
            return true;
        }
        if params.is_empty() && matches!(name.as_str(), "Expr" | "QuoteNode" | "Dict" | "Set") {
            return true;
        }
    }
    matches!(
        core,
        CoreType::Primitive(
            CorePrimitive::String
                | CorePrimitive::Symbol
                | CorePrimitive::Bool
                | CorePrimitive::Char
        ) | CoreType::Abstract(CoreAbstract::Type | CoreAbstract::DataType)
            | CoreType::TypeOf(_)
    )
}

/// CoreType-native port of the legacy Complex-method filter
/// `matches!(ty, JuliaType::Struct(s) if s.starts_with("Complex"))`: a
/// canonical-inverse `Struct` rendering starts with `"Complex"` iff the core
/// family name does (the rendered string starts with the family name, and no
/// dedicated-variant family name starts with `"Complex"`). The other
/// `Struct`-spelling images (`NamedTuple`/`Vararg`/`VarargLen`/`Value`) render
/// as `NamedTuple{...}`/`Vararg{...}`/value literals, which never start with
/// `"Complex"`.
pub(crate) fn core_is_complex_struct_param(core: &CoreType) -> bool {
    match core {
        CoreType::Struct { name, .. } | CoreType::Named(name) => name.starts_with("Complex"),
        _ => false,
    }
}

/// True exactly when the canonical inverse of `core` reconstructs a
/// `JuliaType::Struct(_)` spelling — the CoreType-native port of the legacy
/// `matches!(ty, JuliaType::Struct(_))` method-filter arm (Issue #6495,
/// stage 6b-ii). Mirrors `CoreCompiler::core_param_struct_base(..).is_some()`
/// plus the `Value(_)` images (which also render as `Struct` spellings).
pub(crate) fn core_param_is_struct_spelling(core: &CoreType) -> bool {
    match core {
        CoreType::Struct { name, params } => !matches!(
            (name.as_str(), params.len()),
            ("Vector", 1)
                | ("Matrix", 1)
                | ("Tuple", 0)
                | ("Array", 0)
                | ("Set", 0)
                | ("Dict", 0)
                | ("NamedTuple", 0)
                | ("UnitRange", 0)
                | ("StepRange", 0)
                | ("Generator", 0)
                | ("IOBuffer", 0)
                | ("Expr", 0)
                | ("QuoteNode", 0)
                | ("LineNumberNode", 0)
                | ("GlobalRef", 0)
        ),
        // Abstract families WITHOUT a dedicated `JuliaType` variant keep a
        // `JuliaType::Struct(name)` spelling through the canonical inverse.
        CoreType::Abstract(a) => matches!(
            a,
            CoreAbstract::AbstractVector
                | CoreAbstract::AbstractMatrix
                | CoreAbstract::DenseArray
                | CoreAbstract::AbstractDict
                | CoreAbstract::AbstractSet
                | CoreAbstract::AbstractUnitRange
                | CoreAbstract::Builtin
        ),
        CoreType::Named(_)
        | CoreType::NamedTuple(_)
        | CoreType::Vararg(_)
        | CoreType::VarargLen { .. }
        | CoreType::Value(_) => true,
        _ => false,
    }
}

impl CoreCompiler<'_> {
    /// The shared runtime-dispatch candidate filter for binary operators:
    /// keep methods where at least one declared operand is a struct type or
    /// an abstract numeric type (promotion fallbacks like
    /// `+(::Number, ::Number)`), or — for Base extensions / methods with IR —
    /// a user-visible array family. Reads the `core_signature` projection
    /// when available (Issue #6495, stage 6b-ii).
    fn is_binary_runtime_dispatch_candidate_method(&self, m: &MethodSig) -> bool {
        let user_array_eligible = m.is_base_extension
            || self
                .shared_ctx
                .function_ir_by_global_index
                .contains_key(&m.global_index);
        method_binary_params_match(m, |c0, c1| {
            core_is_binary_runtime_dispatch_candidate_type(c0)
                || core_is_binary_runtime_dispatch_candidate_type(c1)
                || (user_array_eligible
                    && (core_is_user_array_runtime_dispatch_candidate_type(c0)
                        || core_is_user_array_runtime_dispatch_candidate_type(c1)))
        }) || self.user_ir_binary_runtime_dispatch_candidate_method(m)
    }

    fn user_ir_binary_runtime_dispatch_candidate_method(&self, m: &MethodSig) -> bool {
        let Some(func) = self
            .shared_ctx
            .function_ir_by_global_index
            .get(&m.global_index)
        else {
            return false;
        };
        if func.params.len() != 2 {
            return false;
        }
        // Issue #7643: in-session user methods can be reachable through
        // FunctionInfo runtime signatures even when the MethodSig core projection
        // filter above misses them. Add user IR binary methods to the candidate
        // set and let the VM's runtime signature resolver select or reject them.
        true
    }

    /// The shared linalg `*` candidate filter (`A * B` matrix/vector products
    /// plus the string-slice concat arm), reading the declared operand pair
    /// from the `core_signature` projection (Issue #6495, stage 6b-ii).
    /// `core_compat` receives the operand index and its declared core image.
    fn is_linalg_mul_candidate_method(
        m: &MethodSig,
        may_be_string_slice: bool,
        core_compat: impl Fn(usize, &CoreType) -> bool,
    ) -> bool {
        method_binary_params_match(m, |c0, c1| {
            (core_is_linalg_array_dispatch_type(c0) && core_is_linalg_array_dispatch_type(c1))
                && core_compat(0, c0)
                && core_compat(1, c1)
                || (may_be_string_slice
                    && core_is_string_concat_dispatch_type(c0)
                    && core_is_string_concat_dispatch_type(c1))
        })
    }

    /// Rendered display spelling of a binary method's declared operand pair
    /// for the candidate dedupe keys, sourced from the `core_signature`
    /// projection through the canonical inverse when available (Issue #6495,
    /// stage 6b-ii; equal to the legacy rendering by the #6336 round-trip).
    fn binary_param_display_pair(m: &MethodSig) -> (String, String) {
        (
            m.projected_param_julia_type(0).to_string(),
            m.projected_param_julia_type(1).to_string(),
        )
    }

    fn has_dispatch_first_equality_candidate(&self) -> bool {
        self.method_tables.get("==").is_some_and(|table| {
            table.methods.iter().any(|method| {
                method_binary_params_match(method, |c0, c1| {
                    core_is_dispatch_first_equality_type(c0)
                        && core_is_dispatch_first_equality_type(c1)
                })
            })
        })
    }

    fn should_route_ne_through_eq_for_dispatch_first_types(
        &mut self,
        left: &Expr,
        right: &Expr,
    ) -> bool {
        if !self.has_dispatch_first_equality_candidate() {
            return false;
        }

        let left_ty = self.infer_julia_type(left);
        let right_ty = self.infer_julia_type(right);
        let may_be_dispatch_first =
            |ty: &JuliaType| matches!(ty, JuliaType::Any) || is_dispatch_first_equality_type(ty);
        let left_value_ty = self.infer_expr_type(left);
        let right_value_ty = self.infer_expr_type(right);
        let value_may_be_dispatch_first = |ty: &ValueType| {
            matches!(
                ty,
                ValueType::Any
                    | ValueType::Str
                    | ValueType::Symbol
                    | ValueType::Bool
                    | ValueType::Char
                    | ValueType::DataType
            )
        };

        (may_be_dispatch_first(&left_ty) || value_may_be_dispatch_first(&left_value_ty))
            && (may_be_dispatch_first(&right_ty) || value_may_be_dispatch_first(&right_value_ty))
    }

    fn matching_dispatch_first_equality_method(
        &self,
        left_ty: &JuliaType,
        right_ty: &JuliaType,
    ) -> Option<crate::compile::MethodSig> {
        if !(is_dispatch_first_equality_type(left_ty) && is_dispatch_first_equality_type(right_ty))
        {
            return None;
        }

        // Bridge the (dispatch-first-narrowed) operand types once; the param
        // side compares on the core_signature projection when available. On
        // the dispatch-first subdomain (String/Symbol/Bool/Char/Type/DataType)
        // the bridge is injective, so core equality == legacy JuliaType
        // equality; only a `TypeOf(<single-letter struct>)` spelling could
        // collide (Issue #6495, stage 6b-ii; gate + suite referee).
        let left_core = CoreType::from(left_ty);
        let right_core = CoreType::from(right_ty);
        self.method_tables.get("==").and_then(|table| {
            table
                .methods
                .iter()
                .find(|method| {
                    method_binary_params_match(method, |c0, c1| {
                        *c0 == left_core && *c1 == right_core
                    })
                })
                .cloned()
        })
    }

    fn emit_typed_intrinsic_or_call(&mut self, intrinsic: Intrinsic) {
        if let Some(instr) = typed_instr_for_intrinsic(intrinsic) {
            self.emit(instr);
        } else {
            self.emit(Instr::CallIntrinsic(intrinsic));
        }
    }

    /// Issue #9409: a statically-proven no-method binary call is still a
    /// *runtime* `MethodError` in Julia (catchable via try/catch), not a
    /// compile-time abort. Evaluate both operands for their side effects,
    /// discard the values, then raise a catchable runtime `MethodError`
    /// (same shape as the call-dispatch path, Issue #6007).
    fn emit_binary_no_method_error(
        &mut self,
        left: &Expr,
        right: &Expr,
        op_name: &str,
        left_ty: &JuliaType,
        right_ty: &JuliaType,
    ) -> CResult<ValueType> {
        self.compile_expr(left)?;
        self.compile_expr(right)?;
        self.emit(Instr::Pop);
        self.emit(Instr::Pop);
        self.emit(Instr::ThrowMethodError(format!(
            "no method matching {}(::{}, ::{})",
            op_name, left_ty, right_ty
        )));
        Ok(ValueType::Any)
    }

    fn expr_is_current_type_param_var(&self, expr: &Expr) -> bool {
        matches!(
            expr,
            Expr::Var(name, _) if self.current_type_param_index.contains_key(name.as_str())
        )
    }

    /// A static binary-dispatch match that binds an `Any`- or `Bottom`-typed
    /// operand to a concretely struct-typed parameter is unsound: the unknown
    /// slot can hold any runtime value — e.g. a plain String bound to a
    /// `==(::SubstitutionString, ::String)` parameter when `last(tuple)`
    /// infers `Bottom` (Issue #10735). Such matches must be routed through
    /// value-based runtime dispatch (`emit_dynamic_binary_both_op`), whose
    /// resolver re-checks the actual operand types and whose String/numeric
    /// fast paths cover the non-struct outcomes. Restricted to the ops that
    /// helper supports; other ops keep the static match (status quo).
    fn static_binary_match_binds_unknown_to_struct(
        op: &BinaryOp,
        method: &MethodSig,
        arg_types: &[JuliaType],
    ) -> bool {
        matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::IntDiv
                | BinaryOp::Mod
                | BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
        ) && method.structured_arg_core_types().is_some_and(|cores| {
            cores.iter().zip(arg_types.iter()).any(|(core, arg)| {
                matches!(arg, JuliaType::Any | JuliaType::Bottom)
                    && core_param_is_struct_spelling(core)
            })
        })
    }

    /// Issue #9343: emit a dynamic binary-both dispatch for `op(left, right)`,
    /// collecting the operator's method candidates exactly like the other
    /// `CallDynamicBinaryBoth` sites. Used to route `Bool × AbstractFloat`
    /// multiply away from the typed `BoolToI64; ToF64; MulF64` specialization
    /// (which turns the strong zero `false * Inf` into `0.0 * Inf == NaN`) and
    /// into `execute_binary_both`, whose Bool strong-zero arm and the pure-Julia
    /// `*(::Bool, ::AbstractFloat)` method preserve `false * Inf == 0.0`.
    pub(in crate::compile) fn emit_dynamic_binary_both_op(
        &mut self,
        op: &BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> CResult<ValueType> {
        let fallback_intrinsic = match op {
            BinaryOp::Add => Intrinsic::DynamicAdd,
            BinaryOp::Sub => Intrinsic::DynamicSub,
            BinaryOp::Mul => Intrinsic::DynamicMul,
            BinaryOp::Div => Intrinsic::DynamicDiv,
            BinaryOp::IntDiv => Intrinsic::SdivInt,
            BinaryOp::Mod => Intrinsic::SremInt,
            BinaryOp::Eq => Intrinsic::EqFloat,
            BinaryOp::Ne => Intrinsic::NeFloat,
            BinaryOp::Lt => Intrinsic::LtFloat,
            BinaryOp::Le => Intrinsic::LeFloat,
            BinaryOp::Gt => Intrinsic::GtFloat,
            BinaryOp::Ge => Intrinsic::GeFloat,
            _ => return err(
                "internal: emit_dynamic_binary_both_op only supports arithmetic and comparison ops",
            ),
        };
        let op_name = binary_op_to_function_name(op);
        let candidates: Vec<usize> = if let Some(table) = self.method_tables.get(op_name) {
            table
                .methods
                .iter()
                .filter(|m| self.is_binary_runtime_dispatch_candidate_method(m))
                .map(|m| m.global_index)
                .collect()
        } else {
            vec![]
        };
        self.compile_expr(left)?;
        self.compile_expr(right)?;
        self.emit(Instr::CallDynamicBinaryBoth(fallback_intrinsic, candidates));
        Ok(match op {
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => ValueType::Bool,
            _ => ValueType::Any,
        })
    }

    fn compile_value_type_param_binary_op(
        &mut self,
        op: &BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> CResult<Option<ValueType>> {
        if !matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::IntDiv
                | BinaryOp::Mod
                | BinaryOp::Pow
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Ne
        ) || !(self.expr_is_current_type_param_var(left)
            || self.expr_is_current_type_param_var(right))
        {
            return Ok(None);
        }

        self.compile_expr(left)?;
        self.compile_expr(right)?;

        if matches!(op, BinaryOp::Pow) {
            self.emit(Instr::DynamicPow);
            return Ok(Some(ValueType::Any));
        }

        let fallback_intrinsic = match op {
            BinaryOp::Add => Intrinsic::DynamicAdd,
            BinaryOp::Sub => Intrinsic::DynamicSub,
            BinaryOp::Mul => Intrinsic::DynamicMul,
            BinaryOp::Div => Intrinsic::DynamicDiv,
            BinaryOp::IntDiv => Intrinsic::SdivInt,
            BinaryOp::Mod => Intrinsic::SremInt,
            BinaryOp::Lt => Intrinsic::LtFloat,
            BinaryOp::Le => Intrinsic::LeFloat,
            BinaryOp::Gt => Intrinsic::GtFloat,
            BinaryOp::Ge => Intrinsic::GeFloat,
            BinaryOp::Eq => Intrinsic::EqFloat,
            BinaryOp::Ne => Intrinsic::NeFloat,
            BinaryOp::Pow
            | BinaryOp::And
            | BinaryOp::Or
            | BinaryOp::Egal
            | BinaryOp::NotEgal
            | BinaryOp::Subtype => {
                return err("internal: unsupported type-parameter binary dispatch")
            }
        };

        let op_name = binary_op_to_function_name(op);
        let candidates: Vec<usize> = if let Some(table) = self.method_tables.get(op_name) {
            table
                .methods
                .iter()
                .filter(|m| self.is_binary_runtime_dispatch_candidate_method(m))
                .map(|m| m.global_index)
                .collect()
        } else {
            vec![]
        };
        self.emit(Instr::CallDynamicBinaryBoth(fallback_intrinsic, candidates));

        let result_ty = match op {
            BinaryOp::Lt
            | BinaryOp::Gt
            | BinaryOp::Le
            | BinaryOp::Ge
            | BinaryOp::Eq
            | BinaryOp::Ne => ValueType::Bool,
            _ => ValueType::Any,
        };
        Ok(Some(result_ty))
    }

    fn emit_const_value(&mut self, value: &ConstValue) -> ValueType {
        match value {
            ConstValue::Int64(v) => {
                self.emit(Instr::PushI64(*v));
                ValueType::I64
            }
            ConstValue::Float64(v) => {
                self.emit(Instr::PushF64(*v));
                ValueType::F64
            }
            ConstValue::Bool(v) => {
                self.emit(Instr::PushBool(*v));
                ValueType::Bool
            }
            ConstValue::String(v) => {
                self.emit(Instr::PushStr(v.clone()));
                ValueType::Str
            }
            ConstValue::Symbol(v) => {
                self.emit(Instr::PushSymbol(v.clone()));
                ValueType::Symbol
            }
            ConstValue::Nothing => {
                self.emit(Instr::PushNothing);
                ValueType::Nothing
            }
        }
    }

    /// Main (ahead-of-time) binary-op codegen. NOTE (Issue #8192): a *second*
    /// codegen path — the runtime arg-type specializer
    /// `vm::specialize::expr::FunctionSpecializer::compile_binary_op` — also
    /// generates binary-op bytecode for untyped functions. Typed `Int64`/`Float64`
    /// instruction selection is shared via [`typed_scalar_binary_instr`] so the
    /// two cannot drift; promotion strategy is *not* shared, so changes here that
    /// affect numeric instruction choice or operand promotion usually need a
    /// mirror in the specializer. See `docs/vm/BINARY_DISPATCH.md`
    /// ("Two binary-op codegen paths").
    /// Whether `ty` is an `AbstractArray`-subtype struct for the purposes of the
    /// native-array-vs-struct `==`/`!=` routing (Issue #8132, generalized by
    /// #8149). Tries the hardcoded StaticArrays fast path first
    /// ([`is_static_array_struct_julia_type`]); otherwise consults the registered
    /// declared/built-in hierarchy via the strict
    /// [`struct_is_registered_subtype_of_abstract`] (which returns `false` for any
    /// struct not genuinely registered as `<: AbstractArray`, so an unrelated
    /// `native-array == struct` pair is never mis-routed). The shared
    /// `MethodTableProjection` is read from any user method table (they share one
    /// Arc, Issue #6348); with no method tables present, only the fast path applies.
    ///
    /// Note: the gate decides *routing* only. Whether the routed comparison then
    /// yields the correct element-wise result depends on the downstream `isequal`
    /// builtin being able to read the operand: it reads native arrays and the
    /// `Value::StaticArray` carriers, but NOT a generic `StructRef` carrying a
    /// user `<: AbstractArray` struct (nor a `SubArray` view) — that downstream
    /// gap is tracked separately in #8229.
    fn is_abstractarray_subtype_struct(&self, ty: &JuliaType) -> bool {
        if is_static_array_struct_julia_type(ty) {
            return true;
        }
        let name = match ty {
            JuliaType::Struct(name) | JuliaType::AbstractUser(name, _) => name.as_str(),
            _ => return false,
        };
        let Some(projection) = self.method_tables.values().next().map(|t| t.projection()) else {
            return false;
        };
        crate::compile::method_table::struct_is_registered_subtype_of_abstract(
            name,
            "AbstractArray",
            projection,
        )
    }

    pub(in crate::compile) fn compile_binary_op(
        &mut self,
        op: &BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> CResult<ValueType> {
        // Short-circuit operators are special forms and are not overloadable.
        if matches!(op, BinaryOp::And) {
            return self.compile_and_expr(left, right);
        }
        if matches!(op, BinaryOp::Or) {
            return self.compile_or_expr(left, right);
        }

        let lookup_const = |name: &str| self.const_values.get(name).cloned();
        if matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::IntDiv
                | BinaryOp::Mod
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Ne
        ) {
            let left_const = crate::compile::const_prop::fold_expr_const_value(left, &lookup_const);
            let right_const =
                crate::compile::const_prop::fold_expr_const_value(right, &lookup_const);
            if let (Some(left_value), Some(right_value)) = (left_const, right_const) {
                let numeric_operands = matches!(
                    (&left_value, &right_value),
                    (
                        ConstValue::Int64(_) | ConstValue::Float64(_),
                        ConstValue::Int64(_) | ConstValue::Float64(_)
                    )
                );
                if numeric_operands {
                    let op_str = binary_op_to_function_name(op);
                    if let Some(value) = crate::compile::const_prop::eval_const_binary(
                        op_str,
                        &left_value,
                        &right_value,
                    ) {
                        return Ok(self.emit_const_value(&value));
                    }
                }
            }
        }

        // Object identity operators use BuiltinId::Egal
        if matches!(op, BinaryOp::Egal | BinaryOp::NotEgal) {
            self.compile_expr(left)?;
            self.compile_expr(right)?;
            self.emit(Instr::CallBuiltin(crate::builtins::BuiltinId::Egal, 2));
            if matches!(op, BinaryOp::NotEgal) {
                // Negate the boolean result
                self.emit(Instr::NotBool);
            }
            return Ok(ValueType::Bool);
        }

        // Subtype operator uses BuiltinId::Subtype
        if matches!(op, BinaryOp::Subtype) {
            self.compile_expr(left)?;
            self.compile_expr(right)?;
            self.emit(Instr::CallBuiltin(crate::builtins::BuiltinId::Subtype, 2));
            return Ok(ValueType::Bool);
        }

        // Issue #8183: mixed-type primitive arithmetic (e.g. `Int64 / Float64`,
        // `Float64 + Int64`) is otherwise dispatched to the Base operator as a
        // dynamic method `Call` on every execution — a large cost in numeric hot
        // loops and an opaque op that aborts native typed-loop recognition. When
        // the operands are different concrete machine numerics with a float in
        // the pair, Julia's promotion makes the typed `…ToF64; <op>F64` form
        // output-identical, so route to the builtin typed path. This mirrors the
        // existing same-type primitive fast path (Int64+Int64 already bypasses
        // dispatch). Only arithmetic — see `mixed_float_arith_specializable`.
        if matches!(
            op,
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div
        ) {
            let left_ty = self.infer_expr_type(left);
            let right_ty = self.infer_expr_type(right);
            if mixed_float_arith_specializable(&left_ty, &right_ty) {
                return self.compile_builtin_binary_op(op, left, right);
            }

            // Issue #9123: Small all-numeric Union fast path for arithmetic.
            //
            // `ValueType::Union` operands reach the binary-op compiler from
            // lattice-inferred results — most prominently calls whose return
            // type phi-joins `Int64`/`Float64` across a loop:
            //   `f(n) = (x = 0; for i in 1:n; x += 0.1; end; x)`  →  f ->
            //   Union([I64, F64]), so `s = s + f(10)` previously fell through
            //   to a fully dynamic `+` call AND degraded the accumulator slot
            //   `s` to a boxed `unknown` slot.
            //
            // Soundness: the promotion is only valid when the result is
            // guaranteed `Float64` for EVERY runtime combination of union
            // members.  That requires the OTHER operand to be a concrete `F64`
            // (F64 dominates every machine numeric in Julia's promotion
            // lattice, so `F64 op T = F64` for all members T).  A union+union
            // or union+int pair must NOT fire: `Union{Int64,Float64} + Int64`
            // is `Int64` in Julia when the union holds an Int64 at runtime.
            //
            // Restricted to arithmetic — NOT comparisons, which require exact
            // integer semantics beyond 2^53 (Issue #8183 reasoning applies here too).
            let union_f64_applicable = (matches!(left_ty, ValueType::F64)
                && small_all_machine_numeric_union(&right_ty))
                || (matches!(right_ty, ValueType::F64)
                    && small_all_machine_numeric_union(&left_ty));

            if union_f64_applicable {
                // Resolve the typed instruction FIRST — only compile the
                // operands once we know the fast path applies (otherwise a
                // fall-through would leave stray operand pushes behind).
                let result_is_float = true;
                if let Some(instr) = typed_scalar_binary_instr(*op, result_is_float) {
                    // Coerce both sides to F64, then emit the typed instr.
                    // `compile_expr_as` handles Union → F64 via `DynamicToF64`
                    // (a no-op at runtime when the value is already F64, the
                    // promoting cast otherwise) and F64 → F64 as a no-op.
                    self.compile_expr_as(left, ValueType::F64)?;
                    self.compile_expr_as(right, ValueType::F64)?;
                    self.emit(instr);
                    return Ok(ValueType::F64);
                }
            }
        }

        // `==` / `!=` with a statically-known range operand: the numeric fast path
        // below cannot coerce a `Range` (it errors "Cannot convert Range to I64").
        // Route to the pure-Julia `==(::AbstractRange, …)` methods (ranges compare
        // element-wise, like arrays); a range vs a non-array scalar falls back to
        // identity (`false`). Issue #5666. (Dynamic/`Any`-typed range operands —
        // e.g. inside `in`/`findfirst` — go through runtime dispatch instead.)
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let lt = self.infer_julia_type(left);
            let rt = self.infer_julia_type(right);
            let is_range_jt = |t: &JuliaType| {
                matches!(
                    t,
                    JuliaType::UnitRange | JuliaType::StepRange | JuliaType::AbstractRange
                ) || matches!(t, JuliaType::Struct(name) if {
                    // Pure-Julia range structs (`OneTo`, `LinRange`, ...) are
                    // `AbstractRange` subtypes but carry a `Struct` JuliaType, not a
                    // native range variant. Without this, `OneTo(3) == OneTo(3)`
                    // (both operands structs) fell to the numeric fast path and
                    // errored "Cannot convert Struct(..) to Range" (Issue #5814),
                    // while `OneTo(3) == 1:3` worked via the UnitRange operand.
                    let base = name
                        .split('{')
                        .next()
                        .unwrap_or(name.as_str())
                        .rsplit('.')
                        .next()
                        .unwrap_or(name.as_str());
                    matches!(
                        base,
                        "OneTo" | "LinRange" | "StepRangeLen" | "LogRange" | "UnitRange" | "StepRange"
                    )
                })
            };
            let is_range_vt = |t: &ValueType| matches!(t, ValueType::Range | ValueType::Rng);
            if is_range_jt(&lt)
                || is_range_jt(&rt)
                || is_range_vt(&self.infer_expr_type(left))
                || is_range_vt(&self.infer_expr_type(right))
            {
                let dispatched = self.method_tables.get("==").and_then(|table| {
                    table
                        .dispatch(&[lt.clone(), rt.clone()])
                        .ok()
                        .map(|m| m.global_index)
                });
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                if let Some(global_index) = dispatched {
                    self.emit(Instr::Call(global_index, 2));
                } else if matches!(lt, JuliaType::Any | JuliaType::AbstractRange)
                    || matches!(rt, JuliaType::Any | JuliaType::AbstractRange)
                {
                    // Dynamic/abstract range operands can arrive as a native
                    // Value::Range or as first-class UnitRange/StepRange
                    // structs during the #10150 migration. If compile-time
                    // method dispatch cannot prove the exact `==` method, keep
                    // the comparison range-aware at runtime instead of falling
                    // to `===` object identity (Issue #5842 regression).
                    // Keep statically-known range-vs-scalar comparisons on the
                    // identity fallback below so `(1:5) == 3` remains `false`
                    // rather than dispatching to `isequal(::AbstractArray, ...)`
                    // and raising a MethodError.
                    self.emit(Instr::CallBuiltin(crate::builtins::BuiltinId::Isequal, 2));
                } else {
                    // Range vs a non-array scalar: identity comparison (`false`).
                    self.emit(Instr::CallBuiltin(crate::builtins::BuiltinId::Egal, 2));
                }
                if matches!(op, BinaryOp::Ne) {
                    self.emit(Instr::NotBool);
                }
                return Ok(ValueType::Bool);
            }
        }

        // Missing propagation for all binary operators (arithmetic and comparison)
        // In Julia, any operation involving missing returns missing (propagation of unknown values).
        // Note: === and !== (identity operators) are handled above and return Bool
        // We only apply this at compile-time for literal `missing` values.
        // For runtime values that might be Missing, the VM handles it appropriately.
        {
            let left_is_missing_lit =
                matches!(left, Expr::Literal(crate::ir::core::Literal::Missing, _));
            let right_is_missing_lit =
                matches!(right, Expr::Literal(crate::ir::core::Literal::Missing, _));
            if left_is_missing_lit || right_is_missing_lit {
                // For missing literals, just push missing as result
                // No need to compile operands since they're literals with no side effects
                self.emit(Instr::PushMissing);
                return Ok(ValueType::Missing);
            }
        }

        if let Some(result_ty) = self.compile_value_type_param_binary_op(op, left, right)? {
            return Ok(result_ty);
        }

        if matches!(op, BinaryOp::Pow)
            && matches!(right, Expr::Literal(crate::ir::core::Literal::Int(2), _))
        {
            match self.infer_expr_type(left) {
                ValueType::I64 => {
                    self.compile_expr_as(left, ValueType::I64)?;
                    self.emit(Instr::DupI64);
                    self.emit(Instr::MulI64);
                    return Ok(ValueType::I64);
                }
                ValueType::F64 => {
                    self.compile_expr_as(left, ValueType::F64)?;
                    self.emit(Instr::DupF64);
                    self.emit(Instr::MulF64);
                    return Ok(ValueType::F64);
                }
                _ => {}
            }
        }

        if matches!(
            op,
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge | BinaryOp::Eq | BinaryOp::Ne
        ) {
            let left_ty = self.infer_expr_type(left);
            let right_ty = self.infer_expr_type(right);
            let left_exact_int = is_exact_i64_literal_in_f64(left);
            let right_exact_int = is_exact_i64_literal_in_f64(right);
            if (left_ty == ValueType::F64 && right_exact_int)
                || (right_ty == ValueType::F64 && left_exact_int)
            {
                self.compile_expr_as(left, ValueType::F64)?;
                self.compile_expr_as(right, ValueType::F64)?;
                match op {
                    BinaryOp::Lt => self.emit(Instr::LtF64),
                    BinaryOp::Le => self.emit(Instr::LeF64),
                    BinaryOp::Gt => self.emit(Instr::GtF64),
                    BinaryOp::Ge => self.emit(Instr::GeF64),
                    BinaryOp::Eq => self.emit(Instr::EqF64),
                    BinaryOp::Ne => self.emit(Instr::NeF64),
                    _ => unreachable!("guarded by outer matches!"),
                }
                return Ok(ValueType::Bool);
            }
        }

        // Mixed integer/float comparison with non-(small-literal) operands
        // (integer variables, or int literals above the float's exact range):
        // route through the dynamic binary path so the VM performs a value-based
        // comparison rather than promoting the integer to the float type and
        // rounding (Issue #8187, generalized to every fixed Int*/UInt* ×
        // Float16/Float32/Float64 mix in #8199). The exact-int-literal fast path
        // above already handled the safely-promotable Int64/Float64 case; this
        // keeps the precision fix off the BigFloat/other-numeric promote path
        // (which has no concrete method to coercion-mis-match).
        if matches!(
            op,
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge | BinaryOp::Eq | BinaryOp::Ne
        ) {
            let left_ty = self.infer_expr_type(left);
            let right_ty = self.infer_expr_type(right);
            let is_mixed_int_float = (is_integer_type(&left_ty) && is_float_type(&right_ty))
                || (is_float_type(&left_ty) && is_integer_type(&right_ty));
            if is_mixed_int_float {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                let intrinsic = match op {
                    BinaryOp::Lt => Intrinsic::LtFloat,
                    BinaryOp::Le => Intrinsic::LeFloat,
                    BinaryOp::Gt => Intrinsic::GtFloat,
                    BinaryOp::Ge => Intrinsic::GeFloat,
                    BinaryOp::Eq => Intrinsic::EqFloat,
                    BinaryOp::Ne => Intrinsic::NeFloat,
                    _ => unreachable!("guarded by outer matches!"),
                };
                self.emit(Instr::CallDynamicBinaryBoth(intrinsic, vec![]));
                return Ok(ValueType::Bool);
            }
        }

        // Power operator: use DynamicPow for scalar values to preserve Rational/Complex semantics.
        // Special case: String ^ Int dispatches to repeat(s, n) instead of DynamicPow.
        // Special case: BigInt ^ Int uses PowBigInt intrinsic (Issue #1708).
        if matches!(op, BinaryOp::Pow) {
            let left_ty = self.infer_expr_type(left);
            let right_ty = self.infer_expr_type(right);

            // String ^ Int: dispatch to repeat(s, n) - Julia's string repeat syntax
            if left_ty == ValueType::Str {
                // Look up the repeat function in method tables
                if let Some(table) = self.method_tables.get("repeat") {
                    let arg_types = vec![JuliaType::String, JuliaType::Int64];
                    if let Ok(method) = table.dispatch(&arg_types) {
                        self.compile_expr(left)?;
                        self.compile_expr(right)?;
                        self.emit(Instr::Call(method.global_index, 2));
                        return Ok(ValueType::Str);
                    }
                }
                // Fallback error if repeat method not found
                return err("MethodError: no method matching ^(String, Int64) - repeat function not available");
            }

            // BigInt ^ Integer: use PowBigInt intrinsic (Issue #1708). Fixed-width
            // integer bases, including Int128, must stay on DynamicPow so the
            // result preserves the base type (Issue #9608). BigInt ^ Float must
            // also stay on DynamicPow so it reaches the BigFloat runtime path,
            // matching upstream Julia (Issue #9653).
            let is_bigint_pow = left_ty == ValueType::BigInt && is_integer_type(&right_ty);
            if is_bigint_pow {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::CallIntrinsic(Intrinsic::PowBigInt));
                return Ok(ValueType::BigInt);
            }

            let left_is_array = is_array_value_type(&left_ty);
            let right_is_array = is_array_value_type(&right_ty);
            if !(left_is_array || right_is_array) {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::DynamicPow);
                return Ok(ValueType::Any);
            }
        }

        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let left_julia_ty = self.infer_julia_type(left);
            let right_julia_ty = self.infer_julia_type(right);
            if let (Some(left_sym), Some(right_sym)) = (
                irrational_struct_symbol(&left_julia_ty),
                irrational_struct_symbol(&right_julia_ty),
            ) {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::Pop);
                self.emit(Instr::Pop);
                let same_symbol = left_sym == right_sym;
                self.emit(Instr::PushBool(if matches!(op, BinaryOp::Eq) {
                    same_symbol
                } else {
                    !same_symbol
                }));
                return Ok(ValueType::Bool);
            }
            if let Some(method) =
                self.matching_dispatch_first_equality_method(&left_julia_ty, &right_julia_ty)
            {
                self.compile_user_defined_binary_op(&BinaryOp::Eq, left, right, &method)?;
                if matches!(op, BinaryOp::Ne) {
                    self.emit(Instr::NotBool);
                }
                return Ok(ValueType::Bool);
            }
        }

        if matches!(op, BinaryOp::Ne)
            && self.should_route_ne_through_eq_for_dispatch_first_types(left, right)
        {
            self.compile_binary_op(&BinaryOp::Eq, left, right)?;
            self.emit(Instr::NotBool);
            return Ok(ValueType::Bool);
        }

        // Module equality - handle early before the numeric/method-table path
        // so `Base == Base` / `Base != Core` compares module identity instead of
        // coercing the Module operand to I64 (Issue #4959). Modules are singletons
        // in Julia, so `==` for modules is just identity (`===`).
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let left_ty = self.infer_expr_type(left);
            let right_ty = self.infer_expr_type(right);
            if matches!(left_ty, ValueType::Module) || matches!(right_ty, ValueType::Module) {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Egal, 2));
                if matches!(op, BinaryOp::Ne) {
                    self.emit(Instr::NotBool);
                }
                return Ok(ValueType::Bool);
            }
        }

        // Tuple comparison - handle early before method table dispatch
        // to avoid MethodError for Tuple == Tuple
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let left_ty = self.infer_expr_type(left);
            let right_ty = self.infer_expr_type(right);
            let left_julia_ty = self.infer_julia_type(left);
            let right_julia_ty = self.infer_julia_type(right);
            // Named tuples now infer as the concrete `@NamedTuple{...}` struct
            // type (Issue #5063) so type-level NamedTuple dispatch can match on
            // field names. Route their `==`/`!=` through the same `TupleEquals`
            // builtin the bare-tuple path uses, instead of falling through to
            // method-table dispatch (which has no `==(::@NamedTuple, ::@NamedTuple)`
            // method and would mis-compare).
            let is_named_tuple_ty = |ty: &JuliaType| {
                matches!(ty, JuliaType::NamedTuple)
                    || matches!(ty, JuliaType::Struct(name) if name.starts_with("@NamedTuple{"))
            };
            let is_tuple_producing_expr = |expr: &Expr| {
                matches!(
                    expr,
                    Expr::Call { function, args, .. }
                        if matches!(function.as_str(), "size" | "Base.size") && args.len() == 1
                ) || matches!(
                    expr,
                    Expr::ModuleCall {
                        module,
                        function,
                        args,
                        ..
                    } if module == "Base" && function == "size" && args.len() == 1
                ) || matches!(
                    expr,
                    Expr::Builtin {
                        name: BuiltinOp::Size,
                        args,
                        ..
                    } if args.len() == 1
                )
            };
            if matches!(left_ty, ValueType::Tuple)
                || matches!(right_ty, ValueType::Tuple)
                || matches!(left_julia_ty, JuliaType::Tuple | JuliaType::TupleOf(_))
                || matches!(right_julia_ty, JuliaType::Tuple | JuliaType::TupleOf(_))
                || matches!(left_ty, ValueType::NamedTuple)
                || matches!(right_ty, ValueType::NamedTuple)
                || is_named_tuple_ty(&left_julia_ty)
                || is_named_tuple_ty(&right_julia_ty)
                || is_tuple_producing_expr(left)
                || is_tuple_producing_expr(right)
            {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                // Issue #5267: tuple/named-tuple `==` must fold `==` (not
                // `isequal`) over the elements so float edge cases match
                // upstream Julia (`(0.0,) == (-0.0,)` is true, `(NaN,) == (NaN,)`
                // is false). `TupleEquals` implements `==` element semantics
                // with three-valued `missing` propagation; `!=` negates it
                // (`NotBool` now also passes `missing` through).
                self.emit(Instr::CallBuiltin(BuiltinId::TupleEquals, 2));
                if matches!(op, BinaryOp::Ne) {
                    self.emit(Instr::NotBool);
                }
                return Ok(ValueType::Bool);
            }
        }

        // Generic AbstractArray-subtype `==`/`!=` (Issue #8229). When an operand
        // is an `AbstractArray` subtype the equality builtin cannot read
        // element-wise — a user `struct <: AbstractArray` (carried as a generic
        // struct ref) or a `SubArray` view, i.e. an `AbstractArray`-subtype
        // struct that is NOT a StaticArrays carrier — neither the native /
        // StaticArray `isequal` builtin (it returns object-identity `false`) nor
        // the static array-coercion paths below (they try to coerce the struct
        // to `ValueType::Array`, erroring `Cannot convert Struct(..) to Array`)
        // can handle it. Route through the `isequal` builtin (as the #8132/#8149
        // native-vs-StaticArray gate does), whose own dispatch fallback
        // element-compares the unreadable operand via the Pure-Julia
        // `isequal(::AbstractArray, ::AbstractArray)` method. This covers the
        // `struct == struct` case the #8132/#8149 gate (which needs one native
        // operand) does not. StaticArrays and the native/`Memory` carriers keep
        // their direct builtin fast path — this gate excludes them.
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let left_jt = self.infer_julia_type(left);
            let right_jt = self.infer_julia_type(right);
            let is_general_abstractarray_struct = |this: &Self, ty: &JuliaType| {
                if !this.is_abstractarray_subtype_struct(ty)
                    || is_static_array_struct_julia_type(ty)
                {
                    return false;
                }
                // Builtin array carriers (`Memory`, and a native
                // `Array`/`Vector`/`Matrix` spelled as a struct) are read
                // element-wise directly by the equality builtin, so they keep
                // their existing fast path. Only generic user `<: AbstractArray`
                // structs and view types (`SubArray`/`ReshapedArray`) — which the
                // builtin cannot read — need the routing this gate applies.
                // Without this exclusion `mem == arr` (Memory) was mis-routed and
                // errored `Cannot convert MemoryOf(..) to Range` (Issue #8229).
                match ty {
                    JuliaType::Struct(name) | JuliaType::AbstractUser(name, _) => {
                        let base = name
                            .split('{')
                            .next()
                            .unwrap_or(name)
                            .rsplit('.')
                            .next()
                            .unwrap_or(name);
                        !matches!(
                            base,
                            "Memory"
                                | "MemoryRef"
                                | "GenericMemory"
                                | "Array"
                                | "Vector"
                                | "Matrix"
                        )
                    }
                    _ => false,
                }
            };
            let left_aas = is_general_abstractarray_struct(self, &left_jt);
            let right_aas = is_general_abstractarray_struct(self, &right_jt);
            // The OTHER operand must itself be array-like for an element-wise
            // comparison to make sense. Without this guard, `v == 5`
            // (AbstractArray struct vs a scalar) would route here, where upstream
            // falls back to `==(x, y) = x === y` and returns `false`.
            let left_arraylike = left_aas
                || is_array_or_memory_value_type(&self.infer_expr_type(left))
                || is_user_array_runtime_dispatch_candidate_type(&left_jt);
            let right_arraylike = right_aas
                || is_array_or_memory_value_type(&self.infer_expr_type(right))
                || is_user_array_runtime_dispatch_candidate_type(&right_jt);
            let fire = (left_aas && right_arraylike) || (right_aas && left_arraylike);
            if fire {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::CallBuiltin(BuiltinId::Isequal, 2));
                if matches!(op, BinaryOp::Ne) {
                    self.emit(Instr::NotBool);
                }
                return Ok(ValueType::Bool);
            }
        }

        // Native-array vs StaticArray equality (Issue #8132). A binding inferred
        // as a native array type (`Vector{T}`, `Array`) can actually hold a
        // StaticArray (`SVector`/`SMatrix`) at runtime when a package override
        // returned a concrete type that differs from the visible generic's
        // return type. Static `==` dispatch on `(VectorOf, Struct{SVector})`
        // finds no element-wise method and falls back to the identity default
        // `==(x, y) = x === y`, which yields `false` even when the elements are
        // equal. Route such a mixed comparison through the same array-`==`
        // builtin the all-native-array fallback uses, so the actual
        // runtime values are compared element-wise. Restricted to the
        // mixed (native-array, StaticArray) pairing so all-native `==` keeps its
        // `==`-vs-`isequal` float semantics (NaN/-0.0) and a StaticArray-vs-
        // StaticArray comparison keeps its existing struct-equality path.
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let left_jt = self.infer_julia_type(left);
            let right_jt = self.infer_julia_type(right);
            let left_vt = self.infer_expr_type(left);
            let right_vt = self.infer_expr_type(right);
            // A native-array operand may be spelled as an array `ValueType`
            // (`infer_expr_type`) and/or an array `JuliaType` (`infer_julia_type`);
            // an overridden call site can report `Any` for the `ValueType` while
            // still carrying the generic `VectorOf` `JuliaType`, so consult both.
            let left_native = is_array_or_memory_value_type(&left_vt)
                || is_user_array_runtime_dispatch_candidate_type(&left_jt);
            let right_native = is_array_or_memory_value_type(&right_vt)
                || is_user_array_runtime_dispatch_candidate_type(&right_jt);
            let left_static = self.is_abstractarray_subtype_struct(&left_jt);
            let right_static = self.is_abstractarray_subtype_struct(&right_jt);
            if (left_native && right_static) || (left_static && right_native) {
                if matches!(op, BinaryOp::Ne) {
                    self.compile_binary_op(&BinaryOp::Eq, left, right)?;
                    self.emit(Instr::NotBool);
                    return Ok(ValueType::Bool);
                }
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::CallBuiltin(BuiltinId::TupleEquals, 2));
                return Ok(ValueType::Bool);
            }
        }

        // BigInt/BigFloat operations - handle early before method table dispatch
        // to avoid MethodError when infer_julia_type correctly returns BigInt/BigFloat
        // (Issue #1910: big() type inference now returns precise types)
        // Issue #2497: Only when the other operand is also a primitive numeric type,
        // not a struct like Rational or Complex (which needs promotion-based dispatch).
        // Issue #3621: Int128 is intentionally NOT included here. Routing Int128
        // through the BigInt intrinsic widens the result to BigInt; Int128 must be
        // handled by the dedicated Int128 early-route (below) so the result type
        // is preserved.
        {
            let left_julia_ty = self.infer_julia_type(left);
            let right_julia_ty = self.infer_julia_type(right);
            let has_bigint = matches!(left_julia_ty, JuliaType::BigInt)
                || matches!(right_julia_ty, JuliaType::BigInt);
            let has_bigfloat = matches!(left_julia_ty, JuliaType::BigFloat)
                || matches!(right_julia_ty, JuliaType::BigFloat);
            // Skip BigInt/BigFloat intrinsic shortcut if either operand is a struct type
            // (e.g., Rational, Complex) or Any (unknown at compile time).
            // These need full method dispatch via promote(). (Issue #2497)
            let needs_dispatch = matches!(left_julia_ty, JuliaType::Struct(_) | JuliaType::Any)
                || matches!(right_julia_ty, JuliaType::Struct(_) | JuliaType::Any);
            if (has_bigfloat || has_bigint) && !needs_dispatch {
                // Defer to builtin handling below (which checks ValueType and uses
                // BigInt/BigFloat intrinsics). Skip method dispatch to avoid MethodError.
                let left_ty = self.infer_expr_type(left);
                let right_ty = self.infer_expr_type(right);
                // Issue #3743: when BigInt meets a Float* operand the result must
                // be BigFloat (matches official Julia: Integer + AbstractFloat -> Float).
                // Detect that case here and route through the BigFloat path below;
                // `pop_bigfloat` promotes the BigInt operand to BigFloat at runtime.
                let bigint_meets_float = (left_ty == ValueType::BigInt && is_float_type(&right_ty))
                    || (right_ty == ValueType::BigInt && is_float_type(&left_ty));
                let is_bigint_expr = (left_ty == ValueType::BigInt
                    || right_ty == ValueType::BigInt
                    || left_ty == ValueType::I128
                    || right_ty == ValueType::I128)
                    && !bigint_meets_float;
                let is_bigfloat_expr = left_ty == ValueType::BigFloat
                    || right_ty == ValueType::BigFloat
                    || bigint_meets_float;
                if is_bigfloat_expr {
                    self.compile_expr(left)?;
                    self.compile_expr(right)?;
                    let intrinsic = match op {
                        BinaryOp::Add => Intrinsic::AddBigFloat,
                        BinaryOp::Sub => Intrinsic::SubBigFloat,
                        BinaryOp::Mul => Intrinsic::MulBigFloat,
                        BinaryOp::Div => Intrinsic::DivBigFloat,
                        BinaryOp::Mod => Intrinsic::RemBigFloat,
                        BinaryOp::Lt => Intrinsic::LtBigFloat,
                        BinaryOp::Le => Intrinsic::LeBigFloat,
                        BinaryOp::Gt => Intrinsic::GtBigFloat,
                        BinaryOp::Ge => Intrinsic::GeBigFloat,
                        BinaryOp::Eq => Intrinsic::EqBigFloat,
                        BinaryOp::Ne => Intrinsic::NeBigFloat,
                        _ => {
                            return err(format!("Unsupported BigFloat operation: {:?}", op));
                        }
                    };
                    self.emit(Instr::CallIntrinsic(intrinsic));
                    let result_ty = match op {
                        BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                        | BinaryOp::Eq
                        | BinaryOp::Ne => ValueType::Bool,
                        _ => ValueType::BigFloat,
                    };
                    return Ok(result_ty);
                }
                if is_bigint_expr {
                    self.compile_expr(left)?;
                    self.compile_expr(right)?;
                    let intrinsic = match op {
                        BinaryOp::Add => Intrinsic::AddBigInt,
                        BinaryOp::Sub => Intrinsic::SubBigInt,
                        BinaryOp::Mul => Intrinsic::MulBigInt,
                        // Issue #8900: BigInt `/` (float division) returns BigFloat, matching
                        // upstream Julia where `big(a) / big(b)` always yields BigFloat.
                        // Only `÷` (IntDiv) stays as DivBigInt for integer-truncating division.
                        BinaryOp::Div => Intrinsic::DivBigFloat,
                        BinaryOp::IntDiv => Intrinsic::DivBigInt,
                        BinaryOp::Mod => Intrinsic::RemBigInt,
                        BinaryOp::Pow => Intrinsic::PowBigInt,
                        BinaryOp::Lt => Intrinsic::LtBigInt,
                        BinaryOp::Le => Intrinsic::LeBigInt,
                        BinaryOp::Gt => Intrinsic::GtBigInt,
                        BinaryOp::Ge => Intrinsic::GeBigInt,
                        BinaryOp::Eq => Intrinsic::EqBigInt,
                        BinaryOp::Ne => Intrinsic::NeBigInt,
                        _ => {
                            return err(format!("Unsupported BigInt operation: {:?}", op));
                        }
                    };
                    self.emit(Instr::CallIntrinsic(intrinsic));
                    let result_ty = match op {
                        BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                        | BinaryOp::Eq
                        | BinaryOp::Ne => ValueType::Bool,
                        // Issue #8900: `/` on BigInt returns BigFloat (float division)
                        BinaryOp::Div => ValueType::BigFloat,
                        _ => ValueType::BigInt,
                    };
                    return Ok(result_ty);
                }
            }
        }

        // Int128 arithmetic - handle early before method table dispatch (Issue #3621).
        // Without this, Int128 + Int128 falls through the lower BigInt path and
        // produces BigInt; routing through CallDynamicBinaryBoth lets the runtime
        // I128 path (vm/exec/binary_both.rs) preserve Int128 (or promote to the
        // appropriate float when mixed with F16/F32/F64).
        if matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::IntDiv
                | BinaryOp::Mod
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Ne
        ) {
            let left_ty = self.infer_expr_type(left);
            let right_ty = self.infer_expr_type(right);
            let has_i128 = left_ty == ValueType::I128 || right_ty == ValueType::I128;
            // Skip when the other operand needs full method dispatch (struct/Any/BigInt).
            // BigInt is excluded because the BigInt early-route above already handles
            // BigInt+Int128. Struct/Any need promotion-based dispatch.
            let needs_dispatch = matches!(
                left_ty,
                ValueType::Struct(_) | ValueType::Any | ValueType::BigInt | ValueType::BigFloat
            ) || matches!(
                right_ty,
                ValueType::Struct(_) | ValueType::Any | ValueType::BigInt | ValueType::BigFloat
            );
            if has_i128 && !needs_dispatch {
                self.compile_expr(left)?;
                self.compile_expr(right)?;

                let intrinsic = match op {
                    BinaryOp::Add => Intrinsic::DynamicAdd,
                    BinaryOp::Sub => Intrinsic::DynamicSub,
                    BinaryOp::Mul => Intrinsic::DynamicMul,
                    BinaryOp::Div => Intrinsic::DynamicDiv,
                    BinaryOp::IntDiv => Intrinsic::SdivInt,
                    BinaryOp::Mod => Intrinsic::SremInt,
                    BinaryOp::Lt => Intrinsic::LtFloat,
                    BinaryOp::Le => Intrinsic::LeFloat,
                    BinaryOp::Gt => Intrinsic::GtFloat,
                    BinaryOp::Ge => Intrinsic::GeFloat,
                    BinaryOp::Eq => Intrinsic::EqFloat,
                    BinaryOp::Ne => Intrinsic::NeFloat,
                    _ => unreachable!("guarded by outer matches!"),
                };
                self.emit(Instr::CallDynamicBinaryBoth(intrinsic, vec![]));

                let has_f64 = left_ty == ValueType::F64 || right_ty == ValueType::F64;
                let has_f32 = left_ty == ValueType::F32 || right_ty == ValueType::F32;
                let has_f16 = left_ty == ValueType::F16 || right_ty == ValueType::F16;
                let result_ty = match op {
                    BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
                    | BinaryOp::Eq
                    | BinaryOp::Ne => ValueType::Bool,
                    BinaryOp::Div => {
                        // Julia's `/` always returns a float type
                        if has_f64 {
                            ValueType::F64
                        } else if has_f32 {
                            ValueType::F32
                        } else if has_f16 {
                            ValueType::F16
                        } else {
                            ValueType::F64
                        }
                    }
                    _ => {
                        // Add/Sub/Mul/IntDiv/Mod: float dominates, otherwise Int128
                        if has_f64 {
                            ValueType::F64
                        } else if has_f32 {
                            ValueType::F32
                        } else if has_f16 {
                            ValueType::F16
                        } else {
                            ValueType::I128
                        }
                    }
                };
                return Ok(result_ty);
            }
        }

        // UInt128 arithmetic - handle early before method table dispatch (Issue #3697).
        // Mirrors the Int128 early-route: U128 + U128 (and mixed with smaller
        // unsigned / non-negative signed) must preserve UInt128 instead of
        // truncating through I64. Comparisons go through the U64/U128 early-route
        // below. Mixed with I128 falls back to method dispatch (would need BigInt
        // to represent the union of signed and unsigned 128-bit ranges).
        if matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::IntDiv
                | BinaryOp::Mod
        ) {
            let left_ty = self.infer_expr_type(left);
            let right_ty = self.infer_expr_type(right);
            let has_u128 = left_ty == ValueType::U128 || right_ty == ValueType::U128;
            let needs_dispatch = matches!(
                left_ty,
                ValueType::Struct(_)
                    | ValueType::Any
                    | ValueType::BigInt
                    | ValueType::BigFloat
                    | ValueType::I128
            ) || matches!(
                right_ty,
                ValueType::Struct(_)
                    | ValueType::Any
                    | ValueType::BigInt
                    | ValueType::BigFloat
                    | ValueType::I128
            );
            if has_u128 && !needs_dispatch {
                self.compile_expr(left)?;
                self.compile_expr(right)?;

                let intrinsic = match op {
                    BinaryOp::Add => Intrinsic::DynamicAdd,
                    BinaryOp::Sub => Intrinsic::DynamicSub,
                    BinaryOp::Mul => Intrinsic::DynamicMul,
                    BinaryOp::Div => Intrinsic::DynamicDiv,
                    BinaryOp::IntDiv => Intrinsic::SdivInt,
                    BinaryOp::Mod => Intrinsic::SremInt,
                    _ => unreachable!("guarded by outer matches!"),
                };
                self.emit(Instr::CallDynamicBinaryBoth(intrinsic, vec![]));

                let has_f64 = left_ty == ValueType::F64 || right_ty == ValueType::F64;
                let has_f32 = left_ty == ValueType::F32 || right_ty == ValueType::F32;
                let has_f16 = left_ty == ValueType::F16 || right_ty == ValueType::F16;
                let result_ty = match op {
                    BinaryOp::Div => {
                        if has_f64 {
                            ValueType::F64
                        } else if has_f32 {
                            ValueType::F32
                        } else if has_f16 {
                            ValueType::F16
                        } else {
                            ValueType::F64
                        }
                    }
                    _ => {
                        if has_f64 {
                            ValueType::F64
                        } else if has_f32 {
                            ValueType::F32
                        } else if has_f16 {
                            ValueType::F16
                        } else {
                            ValueType::U128
                        }
                    }
                };
                return Ok(result_ty);
            }
        }

        // Mixed narrow primitive numeric promotion (Issue #3742).
        // Without this, mixed-width narrow integer pairs (Int8+Int16, UInt32+UInt64)
        // and narrow Int+Float pairs (Int+Float32, Bool+Float32, Float16+Float32)
        // fall through to dispatch on +(::Number, ::Number). That call goes through
        // promote() correctly, but the function body `px + py` infers as Any+Any and
        // the runtime widens both to I64/F64, dropping the result back to Int64/Float64
        // instead of preserving the promoted narrow type.
        //
        // Same-type pairs (Int8+Int8 etc.) and pairs whose promotion already hits the
        // I64/F64 default (e.g., Int8+Int64, Int8+Float64) are skipped — the existing
        // builtin path handles them correctly.
        if matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::IntDiv
                | BinaryOp::Mod
        ) {
            let left_ty = self.infer_expr_type(left);
            let right_ty = self.infer_expr_type(right);
            let is_narrow_primitive = |t: &ValueType| {
                matches!(
                    t,
                    ValueType::I8
                        | ValueType::I16
                        | ValueType::I32
                        | ValueType::I64
                        | ValueType::U8
                        | ValueType::U16
                        | ValueType::U32
                        | ValueType::U64
                        | ValueType::F16
                        | ValueType::F32
                        | ValueType::F64
                        | ValueType::Bool
                )
            };
            if left_ty != right_ty
                && is_narrow_primitive(&left_ty)
                && is_narrow_primitive(&right_ty)
            {
                if let Some(promoted) = promote_numeric_value_types(&left_ty, &right_ty) {
                    // Julia: `/` always returns a float; if promoted is integer, use F64.
                    let promoted_is_float =
                        matches!(promoted, ValueType::F16 | ValueType::F32 | ValueType::F64);
                    let result_ty = if matches!(op, BinaryOp::Div) && !promoted_is_float {
                        ValueType::F64
                    } else {
                        promoted.clone()
                    };
                    // Skip if promoted is I64/F64 — existing builtin path handles those.
                    if !matches!(result_ty, ValueType::I64 | ValueType::F64) {
                        if matches!(op, BinaryOp::Add)
                            && matches!(result_ty, ValueType::F16 | ValueType::F32)
                            && (matches!(left_ty, ValueType::Bool)
                                || matches!(right_ty, ValueType::Bool)
                                || matches!(left, Expr::Literal(Literal::Bool(_), _))
                                || matches!(right, Expr::Literal(Literal::Bool(_), _)))
                        {
                            self.compile_expr(left)?;
                            self.compile_expr(right)?;
                            self.emit(Instr::CallDynamicBinaryBoth(Intrinsic::DynamicAdd, vec![]));
                            return Ok(result_ty);
                        }
                        let result_is_float =
                            matches!(result_ty, ValueType::F16 | ValueType::F32 | ValueType::F64);
                        let operand_ty = if result_is_float || matches!(op, BinaryOp::Div) {
                            ValueType::F64
                        } else {
                            ValueType::I64
                        };
                        self.compile_expr_as(left, operand_ty.clone())?;
                        self.compile_expr_as(right, operand_ty.clone())?;
                        match (op, &operand_ty) {
                            (BinaryOp::Add, ValueType::I64) => {
                                self.emit_typed_intrinsic_or_call(Intrinsic::AddInt)
                            }
                            (BinaryOp::Sub, ValueType::I64) => {
                                self.emit_typed_intrinsic_or_call(Intrinsic::SubInt)
                            }
                            (BinaryOp::Mul, ValueType::I64) => {
                                self.emit_typed_intrinsic_or_call(Intrinsic::MulInt)
                            }
                            (BinaryOp::IntDiv, ValueType::I64) => {
                                self.emit_typed_intrinsic_or_call(Intrinsic::SdivInt)
                            }
                            (BinaryOp::Mod, ValueType::I64) => {
                                self.emit_typed_intrinsic_or_call(Intrinsic::SremInt)
                            }
                            (BinaryOp::Add, ValueType::F64) => {
                                self.emit_typed_intrinsic_or_call(Intrinsic::DynamicAdd)
                            }
                            (BinaryOp::Sub, ValueType::F64) => {
                                self.emit_typed_intrinsic_or_call(Intrinsic::DynamicSub)
                            }
                            (BinaryOp::Mul, ValueType::F64) => {
                                self.emit_typed_intrinsic_or_call(Intrinsic::DynamicMul)
                            }
                            (BinaryOp::Div, ValueType::F64) => {
                                self.emit_typed_intrinsic_or_call(Intrinsic::DynamicDiv)
                            }
                            (BinaryOp::Mod, ValueType::F64) => self.emit(Instr::DynamicMod),
                            (BinaryOp::IntDiv, ValueType::F64) => self.emit(Instr::DynamicIntDiv),
                            _ => unreachable!("guarded by outer matches!"),
                        }
                        match &result_ty {
                            ValueType::F32 => self.emit(Instr::DynamicToF32),
                            ValueType::F16 => self.emit(Instr::DynamicToF16),
                            ValueType::I8 => self.emit(Instr::DynamicToI8),
                            ValueType::I16 => self.emit(Instr::DynamicToI16),
                            ValueType::I32 => self.emit(Instr::DynamicToI32),
                            ValueType::U8 => self.emit(Instr::DynamicToU8),
                            ValueType::U16 => self.emit(Instr::DynamicToU16),
                            ValueType::U32 => self.emit(Instr::DynamicToU32),
                            ValueType::U64 => self.emit(Instr::DynamicToU64),
                            _ => {}
                        }
                        return Ok(result_ty);
                    }
                }
            }
        }

        // Abstract numeric type dispatch - handle early before method table dispatch (Issue #2498)
        // When a parameter has an abstract numeric type annotation (Number, Real, Integer, etc.),
        // the actual runtime value could be BigInt, BigFloat, or any numeric type.
        // We must use runtime dispatch instead of hardcoded intrinsics like DynamicAdd/AddInt
        // which would fail for BigInt/BigFloat values.
        {
            let has_abstract_numeric = |expr: &Expr| -> bool {
                if let Expr::Var(name, _) = expr {
                    self.abstract_numeric_params.contains(name.as_str())
                } else {
                    false
                }
            };
            if has_abstract_numeric(left) || has_abstract_numeric(right) {
                self.compile_expr(left)?;
                self.compile_expr(right)?;

                // For power operations, use DynamicPow which handles I64^I64 -> I64
                if matches!(op, BinaryOp::Pow) {
                    self.emit(Instr::DynamicPow);
                    return Ok(ValueType::Any);
                }

                let fallback_intrinsic = match op {
                    BinaryOp::Add => Intrinsic::DynamicAdd,
                    BinaryOp::Sub => Intrinsic::DynamicSub,
                    BinaryOp::Mul => Intrinsic::DynamicMul,
                    BinaryOp::Div => Intrinsic::DynamicDiv,
                    BinaryOp::IntDiv => Intrinsic::SdivInt,
                    BinaryOp::Pow => return err("internal: Pow should be handled by DynamicPow"),
                    BinaryOp::Lt => Intrinsic::LtFloat,
                    BinaryOp::Le => Intrinsic::LeFloat,
                    BinaryOp::Gt => Intrinsic::GtFloat,
                    BinaryOp::Ge => Intrinsic::GeFloat,
                    BinaryOp::Eq => Intrinsic::EqFloat,
                    BinaryOp::Ne => Intrinsic::NeFloat,
                    BinaryOp::Mod => Intrinsic::SremInt,
                    BinaryOp::And
                    | BinaryOp::Or
                    | BinaryOp::Egal
                    | BinaryOp::NotEgal
                    | BinaryOp::Subtype => Intrinsic::EqInt,
                };

                // Build candidates from method tables for runtime dispatch
                let op_name = binary_op_to_function_name(op);
                let candidates: Vec<usize> = if let Some(table) = self.method_tables.get(op_name) {
                    table
                        .methods
                        .iter()
                        .filter(|m| self.is_binary_runtime_dispatch_candidate_method(m))
                        .map(|m| m.global_index)
                        .collect()
                } else {
                    vec![]
                };

                self.emit(Instr::CallDynamicBinaryBoth(fallback_intrinsic, candidates));

                let result_ty = match op {
                    BinaryOp::Lt
                    | BinaryOp::Gt
                    | BinaryOp::Le
                    | BinaryOp::Ge
                    | BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Egal
                    | BinaryOp::NotEgal
                    | BinaryOp::And
                    | BinaryOp::Or
                    | BinaryOp::Subtype => ValueType::Bool,
                    _ => ValueType::Any,
                };
                return Ok(result_ty);
            }
        }

        // Char arithmetic - handle early before method table dispatch (Issue #2122)
        // Julia: Char + Int → Char, Int + Char → Char, Char - Int → Char, Char - Char → Int
        if matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Ne
        ) {
            let left_ty = self.infer_expr_type(left);
            let right_ty = self.infer_expr_type(right);
            let left_is_char = left_ty == ValueType::Char;
            let right_is_char = right_ty == ValueType::Char;
            let has_char = left_is_char || right_is_char;

            if has_char {
                // Compile both operands
                self.compile_expr(left)?;
                self.compile_expr(right)?;

                // Emit integer intrinsic (Char values are converted to codepoints at runtime)
                let intrinsic = match op {
                    BinaryOp::Add => Intrinsic::AddInt,
                    BinaryOp::Sub => Intrinsic::SubInt,
                    BinaryOp::Lt => Intrinsic::SltInt,
                    BinaryOp::Le => Intrinsic::SleInt,
                    BinaryOp::Gt => Intrinsic::SgtInt,
                    BinaryOp::Ge => Intrinsic::SgeInt,
                    BinaryOp::Eq => Intrinsic::EqInt,
                    BinaryOp::Ne => Intrinsic::NeInt,
                    _ => return err(format!("internal: unexpected Char operation {:?}", op)),
                };

                // For Char arithmetic, the runtime dispatch handles type-correct results.
                // Use CallDynamicBinaryBoth to let runtime handle Char+Int/Char-Char/etc.
                // since the intrinsic execution path already handles Value::Char
                self.emit(Instr::CallDynamicBinaryBoth(intrinsic, vec![]));

                let result_ty = match op {
                    BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
                    | BinaryOp::Eq
                    | BinaryOp::Ne => ValueType::Bool,
                    BinaryOp::Sub if left_is_char && right_is_char => ValueType::I64,
                    BinaryOp::Add | BinaryOp::Sub => ValueType::Char,
                    _ => {
                        return err(format!(
                            "internal: unexpected Char result type for {:?}",
                            op
                        ))
                    }
                };
                return Ok(result_ty);
            }
        }

        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let left_julia_ty = self.infer_julia_type(left);
            let right_julia_ty = self.infer_julia_type(right);
            let left_is_irrational = is_irrational_dispatch_type(&left_julia_ty);
            let right_is_irrational = is_irrational_dispatch_type(&right_julia_ty);
            if left_is_irrational || right_is_irrational {
                if left_is_irrational && right_is_irrational {
                    if is_vm_known_irrational_type(&left_julia_ty)
                        && is_vm_known_irrational_type(&right_julia_ty)
                    {
                        self.compile_expr_as(left, ValueType::F64)?;
                        self.compile_expr_as(right, ValueType::F64)?;
                        let intrinsic = match op {
                            BinaryOp::Eq => Intrinsic::EqFloat,
                            BinaryOp::Ne => Intrinsic::NeFloat,
                            _ => unreachable!("guarded by outer matches!"),
                        };
                        self.emit_typed_intrinsic_or_call(intrinsic);
                    } else {
                        self.compile_expr(left)?;
                        self.compile_expr(right)?;
                        self.emit(Instr::Pop);
                        self.emit(Instr::Pop);
                        let same_singleton_type = irrational_struct_symbol(&left_julia_ty)
                            .zip(irrational_struct_symbol(&right_julia_ty))
                            .is_some_and(|(left_name, right_name)| left_name == right_name);
                        self.emit(Instr::PushBool(if matches!(op, BinaryOp::Eq) {
                            same_singleton_type
                        } else {
                            !same_singleton_type
                        }));
                    }
                } else {
                    self.compile_expr(left)?;
                    self.compile_expr(right)?;
                    self.emit(Instr::Pop);
                    self.emit(Instr::Pop);
                    self.emit(Instr::PushBool(matches!(op, BinaryOp::Ne)));
                }
                return Ok(ValueType::Bool);
            }
        }

        // Irrational singleton arithmetic converts through float constructors.
        // Otherwise the primitive numeric fallback tries to coerce
        // `Irrational{:π}` to Int64 for expressions like `pi + 1` (Issue #5133).
        if matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
        ) {
            let left_julia_ty = self.infer_julia_type(left);
            let right_julia_ty = self.infer_julia_type(right);
            let left_is_irrational = is_vm_known_irrational_type(&left_julia_ty);
            let right_is_irrational = is_vm_known_irrational_type(&right_julia_ty);
            if left_is_irrational || right_is_irrational {
                let left_ty = self.infer_expr_type(left);
                let right_ty = self.infer_expr_type(right);

                // The Float64/Float32 fast path below forces BOTH operands to
                // Float64 (or Float32) before pure-Julia method dispatch. That is
                // only correct when EACH operand is either the VM-known irrational
                // singleton itself or a concrete numeric whose upstream promotion
                // with an irrational is Float64/Float32 (see
                // `is_irrational_fast_path_concrete`). Gate on that WHITELIST and
                // fall through to normal method dispatch for everything else —
                // BigFloat/Float16/Any/Union/struct operands — which
                // promotes through the pure-Julia
                // `+(x::AbstractFloat, y::AbstractIrrational) = x + typeof(x)(y)`
                // and `promote(::BigFloat, ::AbstractIrrational)` methods
                // (base/irrationals.jl, base/promotion.jl) and preserves the wider
                // type: e.g. `BigFloat(1) + pi` yields BigFloat at the active
                // precision instead of degrading to Float64 (Issue #9317). A
                // *blacklist* (bail only on a statically BigFloat operand) still
                // forced the common dynamically-typed cases onto the Float64 fast
                // path — `f(x) = x + pi; f(BigFloat(1))`, `Any[BigFloat(1)][1] + pi`,
                // `g() = BigFloat(1); g() + pi` — all of which the compiler sees as
                // Any. The non-fast-path `max`/`min` irrational methods already
                // dispatch this way.
                let left_ok = left_is_irrational || is_irrational_fast_path_concrete(&left_ty);
                let right_ok = right_is_irrational || is_irrational_fast_path_concrete(&right_ty);
                if left_ok && right_ok {
                    let result_is_f32 =
                        matches!(left_ty, ValueType::F32) || matches!(right_ty, ValueType::F32);

                    if result_is_f32 {
                        self.compile_expr_as(left, ValueType::F64)?;
                        self.emit(Instr::DynamicToF32);
                        self.compile_expr_as(right, ValueType::F64)?;
                        self.emit(Instr::DynamicToF32);
                    } else {
                        self.compile_expr_as(left, ValueType::F64)?;
                        self.compile_expr_as(right, ValueType::F64)?;
                    }

                    let fallback_intrinsic = match op {
                        BinaryOp::Add => Intrinsic::DynamicAdd,
                        BinaryOp::Sub => Intrinsic::DynamicSub,
                        BinaryOp::Mul => Intrinsic::DynamicMul,
                        BinaryOp::Div => Intrinsic::DynamicDiv,
                        BinaryOp::Lt => Intrinsic::LtFloat,
                        BinaryOp::Le => Intrinsic::LeFloat,
                        BinaryOp::Gt => Intrinsic::GtFloat,
                        BinaryOp::Ge => Intrinsic::GeFloat,
                        _ => unreachable!("guarded by outer matches!"),
                    };
                    self.emit_typed_intrinsic_or_call(fallback_intrinsic);

                    let result_ty = match op {
                        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                            ValueType::Bool
                        }
                        _ if result_is_f32 => {
                            self.emit(Instr::DynamicToF32);
                            ValueType::F32
                        }
                        _ => ValueType::F64,
                    };
                    return Ok(result_ty);
                }
            }
        }

        // Memory is a DenseVector in upstream Julia. Route Memory-involving
        // array-like arithmetic to the VM dynamic array path before generic
        // operator method probing, because sjulia does not yet model the full
        // GenericMemory abstract-array method lattice in Pure Julia.
        let early_left_ty = self.infer_expr_type(left);
        let early_right_ty = self.infer_expr_type(right);
        let early_left_is_memory = is_memory_value_type(&early_left_ty);
        let early_right_is_memory = is_memory_value_type(&early_right_ty);
        let early_left_is_array_like = is_array_or_memory_value_type(&early_left_ty);
        let early_right_is_array_like = is_array_or_memory_value_type(&early_right_ty);
        if early_left_is_memory || early_right_is_memory {
            let dynamic_instr = match op {
                BinaryOp::Add | BinaryOp::Sub
                    if early_left_is_array_like && early_right_is_array_like =>
                {
                    Some(if matches!(op, BinaryOp::Add) {
                        Instr::DynamicAdd
                    } else {
                        Instr::DynamicSub
                    })
                }
                BinaryOp::Div
                    if early_left_is_memory
                        && is_scalar_numeric_or_complex_value_type(&early_right_ty) =>
                {
                    Some(Instr::DynamicDiv)
                }
                BinaryOp::Mul
                    if (early_left_is_memory
                        && is_scalar_numeric_or_complex_value_type(&early_right_ty))
                        || (is_scalar_numeric_or_complex_value_type(&early_left_ty)
                            && early_right_is_memory) =>
                {
                    Some(Instr::DynamicMul)
                }
                _ => None,
            };
            if let Some(instr) = dynamic_instr {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(instr);
                return Ok(ValueType::Array);
            }
        }

        // Array equality where one operand arrived through an Any-typed path
        // (for example an untyped struct field) must not statically enter the
        // Pure-Julia Array == method with a stale operand type. Route it through
        // the VM array equality fallback used below for array-like operands.
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let left_julia_ty = self.infer_julia_type(left);
            let right_julia_ty = self.infer_julia_type(right);
            let left_julia_is_array_like =
                is_user_array_runtime_dispatch_candidate_type(&left_julia_ty);
            let right_julia_is_array_like =
                is_user_array_runtime_dispatch_candidate_type(&right_julia_ty);
            let left_is_any_array_path = matches!(early_left_ty, ValueType::Any)
                && (early_right_is_array_like
                    || left_julia_is_array_like
                    || right_julia_is_array_like);
            let right_is_any_array_path = matches!(early_right_ty, ValueType::Any)
                && (early_left_is_array_like
                    || left_julia_is_array_like
                    || right_julia_is_array_like);

            if left_is_any_array_path || right_is_any_array_path {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::CallBuiltin(BuiltinId::TupleEquals, 2));
                if matches!(op, BinaryOp::Ne) {
                    self.emit(Instr::NotBool);
                }
                return Ok(ValueType::Bool);
            }
        }

        // Matrix/vector multiplication must stay dispatch-first, but compile-time
        // inference often only knows `Array`. Route array-array `*` through
        // runtime rank-aware dispatch before static operator probing so a later
        // `*(::AbstractMatrix, ::AbstractVector)` override cannot capture a
        // matrix RHS (Issue #5624).
        if matches!(op, BinaryOp::Mul) {
            let left_is_array = is_array_value_type(&early_left_ty);
            let right_is_array = is_array_value_type(&early_right_ty);
            if left_is_array && right_is_array {
                let left_julia_ty = self.infer_julia_type(left);
                let right_julia_ty = self.infer_julia_type(right);
                let involves_diagonal =
                    mul_involves_diagonal(left, right, &left_julia_ty, &right_julia_ty);
                if !involves_diagonal {
                    let may_be_string_slice =
                        is_slice_index_expr(left) || is_slice_index_expr(right);
                    let result_ty = if may_be_string_slice {
                        ValueType::Any
                    } else {
                        ValueType::Array
                    };
                    if let Some(table) = self.method_tables.get("*") {
                        let candidates = dedupe_binary_candidates_keep_first(
                            table
                                .methods
                                .iter()
                                .filter(|m| {
                                    Self::is_linalg_mul_candidate_method(
                                        m,
                                        may_be_string_slice,
                                        |idx, core| {
                                            let (actual, actual_vt) = if idx == 0 {
                                                (&left_julia_ty, &early_left_ty)
                                            } else {
                                                (&right_julia_ty, &early_right_ty)
                                            };
                                            core_linalg_array_candidate_compatible_for_value_type(
                                                actual, actual_vt, core,
                                            )
                                        },
                                    )
                                })
                                .map(|m| {
                                    let (left_name, right_name) =
                                        Self::binary_param_display_pair(m);
                                    (m.global_index, left_name, right_name)
                                })
                                .collect(),
                        );

                        if !candidates.is_empty() {
                            self.compile_expr(left)?;
                            self.compile_expr(right)?;
                            self.emit(Instr::CallDynamicBinaryBoth(
                                Intrinsic::DynamicMul,
                                candidates,
                            ));
                            return Ok(result_ty);
                        }
                    }

                    self.compile_expr(left)?;
                    self.compile_expr(right)?;
                    self.emit(Instr::MatMul);
                    return Ok(result_ty);
                }
            }
        }

        // String concatenation: "a" * "b" => "ab" (Julia uses * for string
        // concatenation). Keep String/Char paired with runtime-unknown operands
        // ahead of method-table probing so Base's `Union{Char,String}` signatures
        // do not force `Any` through compile-time coercion, but let known
        // non-string operands (e.g. `"a" * 1`) dispatch and raise MethodError
        // like upstream Julia (Issues #3465/#6268).
        // Issue #2127: Include Char operands since Julia's * converts Char to String.
        let left_is_string_concat_known = matches!(early_left_ty, ValueType::Str | ValueType::Char);
        let right_is_string_concat_known =
            matches!(early_right_ty, ValueType::Str | ValueType::Char);
        let left_may_be_string_concat = left_is_string_concat_known
            || matches!(early_left_ty, ValueType::Any | ValueType::Union(_));
        let right_may_be_string_concat = right_is_string_concat_known
            || matches!(early_right_ty, ValueType::Any | ValueType::Union(_));
        if matches!(op, BinaryOp::Mul)
            && (left_is_string_concat_known || right_is_string_concat_known)
            && left_may_be_string_concat
            && right_may_be_string_concat
        {
            self.compile_expr(left)?;
            self.compile_expr(right)?;
            self.emit(Instr::StringConcat(2));
            return Ok(ValueType::Str);
        }

        // Check for user-defined operator overload
        let op_name = binary_op_to_function_name(op);
        if let Some(table) = self.method_tables.get(op_name) {
            // Infer argument types for dispatch
            let left_julia_ty = self.infer_julia_type(left);
            let right_julia_ty = self.infer_julia_type(right);
            let arg_types = vec![left_julia_ty.clone(), right_julia_ty.clone()];

            // Check if all methods are Base extensions
            if table.all_base_extensions() {
                // All methods are Base extensions (e.g., `function Base.:+(...)`)
                // Base extensions do NOT shadow builtins for primitive types.
                // Only use Base extension if:
                // 1. At least one operand is a struct type, AND
                // 2. A matching method exists
                let left_is_struct = matches!(left_julia_ty, JuliaType::Struct(_));
                let right_is_struct = matches!(right_julia_ty, JuliaType::Struct(_));
                let left_is_any = matches!(left_julia_ty, JuliaType::Any);
                let right_is_any = matches!(right_julia_ty, JuliaType::Any);
                let is_bare_parametric_struct = |ty: &JuliaType| {
                    matches!(ty, JuliaType::Struct(name)
                        if !name.contains('{')
                            && self.shared_ctx.parametric_structs.contains_key(name))
                };
                let has_bare_parametric_struct = is_bare_parametric_struct(&left_julia_ty)
                    || is_bare_parametric_struct(&right_julia_ty);

                if left_is_struct || right_is_struct {
                    // IMPORTANT: Check for (Struct, Any) case BEFORE trying static dispatch.
                    // When one operand is Struct and the other is Any, static dispatch might
                    // incorrectly match a method like (Rational{T}, Int64) because `Any` matches
                    // primitive types at compile time. But at runtime, the Any value might
                    // actually be the same struct type, so we need runtime dispatch to check.
                    if (left_is_struct && right_is_any) || (left_is_any && right_is_struct) {
                        // Build candidates for runtime dispatch
                        // We need to find all methods that match the struct operand
                        let check_position = if right_is_any { 1 } else { 0 };
                        let struct_ty = if left_is_struct {
                            &left_julia_ty
                        } else {
                            &right_julia_ty
                        };

                        // Find methods where the struct position matches
                        // For parametric structs, compare base names (e.g., Rational{Int64} matches Rational{T})
                        let struct_base = if let JuliaType::Struct(name) = struct_ty {
                            if let Some(idx) = name.find('{') {
                                &name[..idx]
                            } else {
                                name.as_str()
                            }
                        } else {
                            ""
                        };
                        let candidates: Vec<usize> = table
                            .methods
                            .iter()
                            .filter(|m| {
                                let struct_pos = if check_position == 1 { 0 } else { 1 };
                                // Compare base struct names for parametric types
                                method_binary_params_match(m, |c0, c1| {
                                    let core = if struct_pos == 0 { c0 } else { c1 };
                                    CoreCompiler::core_param_struct_base(core) == Some(struct_base)
                                })
                            })
                            .map(|m| m.global_index)
                            .collect();
                        if !candidates.is_empty() {
                            // Use two-operand runtime dispatch. Checking only
                            // the Any side can select an unrelated same-side
                            // specialization (Issue #6270).
                            self.compile_expr(left)?;
                            self.compile_expr(right)?;
                            self.emit(Instr::CallDynamicBinaryNoFallback(candidates));

                            // Return the struct type as result (Complex + Any -> Complex)
                            let result_ty = julia_type_to_value_type(struct_ty);
                            return Ok(result_ty);
                        }
                    }

                    // If (Struct, Any) runtime dispatch had no candidates, or this is not a
                    // (Struct, Any) case, try static dispatch
                    if !has_bare_parametric_struct {
                        match table.dispatch(&arg_types) {
                            Ok(method) => {
                                return self
                                    .compile_user_defined_binary_op(op, left, right, method);
                            }
                            Err(_) => {
                                // Static dispatch failed; fall through to the
                                // runtime-dispatch eligibility checks below.
                            }
                        }
                    }

                    // Static dispatch failed or was deliberately skipped; continue
                    // with runtime-dispatch eligibility checks.
                    // Special case: == comparison on structs without user-defined ==
                    // Falls back to field-by-field comparison (Julia default behavior)
                    if matches!(op, BinaryOp::Eq)
                        && left_is_struct
                        && right_is_struct
                        && !left_is_any
                        && !right_is_any
                    {
                        // Emit EqStruct instruction for default struct comparison
                        self.compile_expr(left)?;
                        self.compile_expr(right)?;
                        self.emit(Instr::EqStruct);
                        return Ok(ValueType::Bool);
                    }
                    // Special case: Complex types with primitives - use runtime dispatch
                    // This handles cases like Complex{Bool} / Float64 where type promotion
                    // should happen at runtime (Complex{Bool} + Float64 -> Complex{Float64})
                    let left_is_complex =
                        matches!(&left_julia_ty, JuliaType::Struct(s) if s.starts_with("Complex"));
                    let right_is_complex =
                        matches!(&right_julia_ty, JuliaType::Struct(s) if s.starts_with("Complex"));
                    let left_is_numeric = left_julia_ty.is_builtin_numeric();
                    let right_is_numeric = right_julia_ty.is_builtin_numeric();

                    if (left_is_complex && right_is_numeric)
                        || (left_is_numeric && right_is_complex)
                        || (left_is_complex && right_is_complex)
                    {
                        // Emit runtime dispatch for Complex operations
                        // Build candidates from all Complex-related methods
                        let candidates: Vec<usize> = table
                            .methods
                            .iter()
                            .filter(|m| {
                                method_binary_params_match(m, |c0, c1| {
                                    core_is_complex_struct_param(c0)
                                        || core_is_complex_struct_param(c1)
                                })
                            })
                            .map(|m| m.global_index)
                            .collect();

                        self.compile_expr(left)?;
                        self.compile_expr(right)?;

                        let fallback_intrinsic = match op {
                            BinaryOp::Add => Intrinsic::DynamicAdd,
                            BinaryOp::Sub => Intrinsic::DynamicSub,
                            BinaryOp::Mul => Intrinsic::DynamicMul,
                            BinaryOp::Div => Intrinsic::DynamicDiv,
                            BinaryOp::Lt => Intrinsic::LtFloat,
                            BinaryOp::Le => Intrinsic::LeFloat,
                            BinaryOp::Gt => Intrinsic::GtFloat,
                            BinaryOp::Ge => Intrinsic::GeFloat,
                            BinaryOp::Eq => Intrinsic::EqFloat,
                            BinaryOp::Ne => Intrinsic::NeFloat,
                            _ => {
                                // Issue #9409: the operands were already compiled
                                // onto the stack above; discard them and raise a
                                // catchable runtime MethodError instead of
                                // aborting compilation.
                                self.emit(Instr::Pop);
                                self.emit(Instr::Pop);
                                self.emit(Instr::ThrowMethodError(format!(
                                    "no method matching {}(::{}, ::{})",
                                    op_name, left_julia_ty, right_julia_ty
                                )));
                                return Ok(ValueType::Any);
                            }
                        };

                        self.emit(Instr::CallDynamicBinaryBoth(fallback_intrinsic, candidates));
                        // Return Complex type for Complex operations
                        return Ok(ValueType::Any);
                    }

                    // For Struct+Struct case where dispatch failed (e.g., Complex{Bool} + Complex{Bool}
                    // when no exact method exists), fall through to runtime dispatch.
                    // The actual runtime types may differ from inferred types (Issue #1055).
                    // For non-struct cases with fully known types, this IS a MethodError.
                    // Issue #2127: Allow String*Char and Char*String to fall through to
                    // builtin string concatenation handler.
                    let is_str_char_mul = matches!(op, BinaryOp::Mul)
                        && (matches!(left_julia_ty, JuliaType::String | JuliaType::Char)
                            || matches!(right_julia_ty, JuliaType::String | JuliaType::Char));
                    // Issue #2475: Allow (Primitive, Struct) and (Struct, Primitive) to
                    // fall through to runtime dispatch. E.g., Int32 + Rational{Int32}
                    // needs promotion via +(::Number, ::Number) at runtime.
                    let left_is_numeric = left_julia_ty.is_builtin_numeric();
                    let right_is_numeric = right_julia_ty.is_builtin_numeric();
                    let is_primitive_struct_mix = (left_is_numeric && right_is_struct)
                        || (left_is_struct && right_is_numeric);
                    let is_array_equality = matches!(op, BinaryOp::Eq | BinaryOp::Ne)
                        && (is_user_array_runtime_dispatch_candidate_type(&left_julia_ty)
                            || is_user_array_runtime_dispatch_candidate_type(&right_julia_ty));
                    if !(left_is_any
                        || right_is_any
                        || (left_is_struct && right_is_struct)
                        || is_str_char_mul
                        || is_primitive_struct_mix
                        || is_array_equality)
                    {
                        return self.emit_binary_no_method_error(
                            left,
                            right,
                            op_name,
                            &left_julia_ty,
                            &right_julia_ty,
                        );
                    }
                }

                // Handle (Primitive, Any), (Any, Primitive), (Primitive, Struct), and
                // (Struct, Primitive) cases with runtime dispatch.
                // This is needed for cases like `1 + rational(3, 2)` where one operand
                // is a known primitive and the other could be a struct at runtime,
                // or `Int32(1) + Rational{Int32}(1, 2)` where promotion is needed (Issue #2475).
                let runtime_numeric = |ty: &JuliaType| {
                    ty.is_builtin_numeric() || matches!(ty, JuliaType::BigInt | JuliaType::BigFloat)
                };
                let left_is_primitive = runtime_numeric(&left_julia_ty);
                let right_is_primitive = runtime_numeric(&right_julia_ty);
                let left_is_struct_here = matches!(left_julia_ty, JuliaType::Struct(_));
                let right_is_struct_here = matches!(right_julia_ty, JuliaType::Struct(_));

                if (left_is_primitive && right_is_any)
                    || (left_is_any && right_is_primitive)
                    || (left_is_primitive && right_is_struct_here)
                    || (left_is_struct_here && right_is_primitive)
                {
                    // Build candidates for runtime dispatch
                    let candidates: Vec<usize> = table
                        .methods
                        .iter()
                        .filter(|m| self.is_binary_runtime_dispatch_candidate_method(m))
                        .map(|m| m.global_index)
                        .collect();

                    if !candidates.is_empty() {
                        self.compile_expr(left)?;
                        self.compile_expr(right)?;

                        // Power operator uses DynamicPow to preserve I64^I64 -> I64
                        if matches!(op, BinaryOp::Pow) {
                            self.emit(Instr::DynamicPow);
                            return Ok(ValueType::Any);
                        }

                        let fallback_intrinsic = match op {
                            BinaryOp::Add => Intrinsic::DynamicAdd,
                            BinaryOp::Sub => Intrinsic::DynamicSub,
                            BinaryOp::Mul => Intrinsic::DynamicMul,
                            BinaryOp::Div => Intrinsic::DynamicDiv,
                            BinaryOp::IntDiv => Intrinsic::SdivInt,
                            BinaryOp::Pow => {
                                return err("internal: Pow should be handled by DynamicPow")
                            }
                            BinaryOp::Lt => Intrinsic::LtFloat,
                            BinaryOp::Le => Intrinsic::LeFloat,
                            BinaryOp::Gt => Intrinsic::GtFloat,
                            BinaryOp::Ge => Intrinsic::GeFloat,
                            BinaryOp::Eq => Intrinsic::EqFloat,
                            BinaryOp::Ne => Intrinsic::NeFloat,
                            BinaryOp::Mod => Intrinsic::SremInt,
                            // Logical/special operators don't have intrinsic fallbacks
                            BinaryOp::And
                            | BinaryOp::Or
                            | BinaryOp::Egal
                            | BinaryOp::NotEgal
                            | BinaryOp::Subtype => {
                                return err(format!(
                                    "No method found for operator {:?} with dynamic types",
                                    op
                                ));
                            }
                        };

                        self.emit(Instr::CallDynamicBinaryBoth(fallback_intrinsic, candidates));
                        return Ok(ValueType::Any);
                    }
                }

                // Handle (Any, Any) and (Struct, Struct) cases with runtime dispatch.
                // For (Struct, Struct), the static dispatch may have failed because the
                // inferred types (e.g., Complex{Bool}) don't match the actual methods
                // (e.g., Complex{Int64}). We need runtime dispatch. (fixes Issue #1055)
                if ((left_is_any && right_is_any) || (left_is_struct && right_is_struct))
                    && !table.methods.is_empty()
                {
                    // Build candidates from methods that take struct or abstract numeric types
                    let candidates: Vec<usize> = table
                        .methods
                        .iter()
                        .filter(|m| self.is_binary_runtime_dispatch_candidate_method(m))
                        .map(|m| m.global_index)
                        .collect();

                    if !candidates.is_empty() {
                        // Compile both operands
                        self.compile_expr(left)?;
                        self.compile_expr(right)?;

                        // Power operator uses DynamicPow to preserve I64^I64 -> I64
                        if matches!(op, BinaryOp::Pow) {
                            self.emit(Instr::DynamicPow);
                            return Ok(ValueType::Any);
                        }

                        // Determine fallback intrinsic based on operation
                        let fallback_intrinsic = match op {
                            BinaryOp::Add => Intrinsic::DynamicAdd,
                            BinaryOp::Sub => Intrinsic::DynamicSub,
                            BinaryOp::Mul => Intrinsic::DynamicMul,
                            BinaryOp::Div => Intrinsic::DynamicDiv,
                            BinaryOp::IntDiv => Intrinsic::SdivInt,
                            BinaryOp::Pow => {
                                return err("internal: Pow should be handled by DynamicPow")
                            }
                            BinaryOp::Lt => Intrinsic::LtFloat,
                            BinaryOp::Le => Intrinsic::LeFloat,
                            BinaryOp::Gt => Intrinsic::GtFloat,
                            BinaryOp::Ge => Intrinsic::GeFloat,
                            BinaryOp::Eq => Intrinsic::EqFloat,
                            BinaryOp::Ne => Intrinsic::NeFloat,
                            BinaryOp::Mod => Intrinsic::SremInt,
                            // Logical/special operators don't have intrinsic fallbacks
                            BinaryOp::And
                            | BinaryOp::Or
                            | BinaryOp::Egal
                            | BinaryOp::NotEgal
                            | BinaryOp::Subtype => {
                                // These should be handled by method dispatch, not intrinsics
                                return err(format!(
                                    "No method found for operator {:?} with dynamic types",
                                    op
                                ));
                            }
                        };

                        // Emit runtime dispatch instruction
                        self.emit(Instr::CallDynamicBinaryBoth(fallback_intrinsic, candidates));

                        // Return Any since we don't know the result type at compile time
                        return Ok(ValueType::Any);
                    }
                }

                // Issue #1759: Try dispatch for ALL types, not just structs.
                // Skip dispatch only when both operands are the SAME primitive numeric type
                // (e.g., Int64 + Int64, Float64 + Float64) — these use intrinsics directly.
                // Mixed-type primitives (e.g., Int64 + Float64) go through Julia's
                // +(::Number, ::Number) → promote() → convert() chain from promotion.jl,
                // matching official Julia behavior.
                let left_is_builtin_numeric = left_julia_ty.is_builtin_numeric();
                let right_is_builtin_numeric = right_julia_ty.is_builtin_numeric();
                let both_same_primitive = left_is_builtin_numeric
                    && right_is_builtin_numeric
                    && left_julia_ty == right_julia_ty;
                if !both_same_primitive {
                    if let Ok(method) = table.dispatch(&arg_types) {
                        if Self::static_binary_match_binds_unknown_to_struct(op, method, &arg_types)
                        {
                            return self.emit_dynamic_binary_both_op(op, left, right);
                        }
                        return self.compile_user_defined_binary_op(op, left, right, method);
                    }
                }

                // Fall through to builtin handling only if no method matches
            } else {
                // At least one method is NOT a Base extension (regular user-defined operator).
                // Julia semantics: this shadows Base.op completely.
                // However, Base extension methods should still be available for dispatch.

                // IMPORTANT: When any arg is Any and struct-typed methods exist, skip static
                // dispatch (Issue #1055, #1783). Static dispatch with Any incorrectly matches
                // primitive methods (e.g., +(::Float32, ::Int64)) over struct methods
                // (e.g., +(::Rational{T}, ::Int64)) because Any is a subtype of all primitives
                // and Float32 has higher specificity than Rational{T}. At runtime, the Any
                // value could be a struct, so we need runtime dispatch.
                let any_arg = arg_types.iter().any(|t| matches!(t, JuliaType::Any));
                let left_is_any = matches!(left_julia_ty, JuliaType::Any);
                let right_is_any = matches!(right_julia_ty, JuliaType::Any);
                let runtime_numeric = |ty: &JuliaType| {
                    ty.is_builtin_numeric() || matches!(ty, JuliaType::BigInt | JuliaType::BigFloat)
                };
                let left_is_primitive = runtime_numeric(&left_julia_ty);
                let right_is_primitive = runtime_numeric(&right_julia_ty);
                let has_struct_methods = any_arg
                    && table.methods.iter().any(|m| {
                        method_binary_params_match(m, |c0, c1| {
                            core_param_is_struct_spelling(c0) || core_param_is_struct_spelling(c1)
                        })
                    });
                let has_any_primitive_pair =
                    (left_is_any && right_is_primitive) || (left_is_primitive && right_is_any);
                let skip_static_dispatch =
                    (any_arg && has_struct_methods) || has_any_primitive_pair;

                // First, try to dispatch to any method (user-defined or Base extension)
                // Skip dispatch when Any args + struct methods exist to avoid wrong matches.
                // Only skip for same-type primitive numerics (e.g., Int64+Int64) which use
                // intrinsics directly. Mixed-type primitives (e.g., Int64+Float64) go through
                // Julia's +(::Number, ::Number) → promote() chain from promotion.jl.
                let both_same_primitive = left_julia_ty.is_builtin_numeric()
                    && right_julia_ty.is_builtin_numeric()
                    && left_julia_ty == right_julia_ty;
                let dispatch_result = if skip_static_dispatch || both_same_primitive {
                    Err(DispatchError::NoMethodFound {
                        name: op_name.to_string(),
                        arg_types: arg_types.clone(),
                    })
                } else {
                    table.dispatch(&arg_types)
                };
                match dispatch_result {
                    Ok(method) => {
                        if Self::static_binary_match_binds_unknown_to_struct(op, method, &arg_types)
                        {
                            return self.emit_dynamic_binary_both_op(op, left, right);
                        }
                        return self.compile_user_defined_binary_op(op, left, right, method);
                    }
                    Err(_) => {
                        // No matching method found. For primitive types, we should still
                        // allow builtin operators to work. But if the user has defined
                        // a non-Base-extension operator, it shadows the builtin for all types.
                        // However, Base extension methods should still be available.
                        // Check if there are any Base extension methods that could match
                        // (for runtime dispatch cases)
                        let has_base_extensions = table.methods.iter().any(|m| m.is_base_extension);

                        // Handle dynamic dispatch cases (fixes Issue #1055, #1783)
                        // Case 1: Both operands are Any at compile time but could be structs at runtime
                        // Case 2: Both operands are Struct but dispatch failed (e.g., Complex{Bool} vs Complex{Int64})
                        // Case 3: One operand is Any and the other is Primitive (Issue #1783)
                        //         The Any value could be a struct at runtime, so runtime dispatch is needed
                        let left_is_struct = matches!(left_julia_ty, JuliaType::Struct(_));
                        let right_is_struct = matches!(right_julia_ty, JuliaType::Struct(_));
                        // Issue #2425: Include (Struct, Any) and (Any, Struct) cases.
                        // When one operand is a known struct type and the other is Any
                        // (e.g., Complex{Float64} * log(z) where log returns Any at compile time),
                        // runtime dispatch is needed to find the correct struct method.
                        // Without these cases, the code falls through to builtin handling
                        // which incorrectly converts the struct to F64 via DynamicToF64.
                        let needs_runtime_dispatch = (left_is_struct || left_is_any)
                            && (right_is_struct || right_is_any)
                            || (left_is_any && right_is_primitive)
                            || (left_is_primitive && right_is_any);
                        if needs_runtime_dispatch && has_base_extensions {
                            // Resolver adapter for arithmetic and comparison operators
                            // (Issues #8621/#8622, parent #8609).
                            //
                            // The `needs_runtime_dispatch` flag above was set from the
                            // JuliaType-level view, which can be conservative: when a
                            // function returns a generic `T` (abstract) but its concrete
                            // return type is deducible from the struct field type,
                            // `infer_julia_type` returns `Any` while `infer_expr_type`
                            // already knows the concrete type (e.g., `Int64`).
                            //
                            // For arithmetic and comparison operators, ask the
                            // LatticeType resolver: if both operands are concrete
                            // primitive numerics, skip `CallDynamicBinaryBoth` and fall
                            // through to the typed-intrinsic path below.  This
                            // eliminates the `compile=NeedsRuntime resolver=UniqueBuiltin`
                            // divergences surfaced by Issue #8620 for `+`, `-`, `*`,
                            // `/`, `^`, `÷`, `%`, `==`, `!=`, `<`, `<=`, `>`, `>=`.
                            let resolver_overrides_to_builtin = matches!(
                                op,
                                BinaryOp::Add
                                    | BinaryOp::Sub
                                    | BinaryOp::Mul
                                    | BinaryOp::Div
                                    | BinaryOp::Pow
                                    | BinaryOp::IntDiv
                                    | BinaryOp::Mod
                                    | BinaryOp::Eq    // Issue #8622
                                    | BinaryOp::Ne
                                    | BinaryOp::Lt
                                    | BinaryOp::Le
                                    | BinaryOp::Gt
                                    | BinaryOp::Ge
                            ) && matches!(
                                binary_static_verdict(
                                    &crate::runtime_types::bridge::value_type_to_lattice(
                                        &self.infer_expr_type(left)
                                    ),
                                    &crate::runtime_types::bridge::value_type_to_lattice(
                                        &self.infer_expr_type(right)
                                    ),
                                ),
                                BinaryStaticVerdict::UniqueBuiltin
                            );

                            if !resolver_overrides_to_builtin {
                                // Build candidates from methods that take struct or abstract numeric types
                                let candidates: Vec<usize> = table
                                    .methods
                                    .iter()
                                    .filter(|m| self.is_binary_runtime_dispatch_candidate_method(m))
                                    .map(|m| m.global_index)
                                    .collect();

                                if !candidates.is_empty() {
                                    self.compile_expr(left)?;
                                    self.compile_expr(right)?;

                                    let fallback_intrinsic = match op {
                                        BinaryOp::Add => Intrinsic::DynamicAdd,
                                        BinaryOp::Sub => Intrinsic::DynamicSub,
                                        BinaryOp::Mul => Intrinsic::DynamicMul,
                                        BinaryOp::Div => Intrinsic::DynamicDiv,
                                        BinaryOp::IntDiv => Intrinsic::SdivInt,
                                        BinaryOp::Pow => Intrinsic::DynamicPow,
                                        BinaryOp::Lt => Intrinsic::LtFloat,
                                        BinaryOp::Le => Intrinsic::LeFloat,
                                        BinaryOp::Gt => Intrinsic::GtFloat,
                                        BinaryOp::Ge => Intrinsic::GeFloat,
                                        BinaryOp::Eq => Intrinsic::EqFloat,
                                        BinaryOp::Ne => Intrinsic::NeFloat,
                                        BinaryOp::Mod => Intrinsic::SremInt,
                                        _ => Intrinsic::DynamicAdd, // Default fallback
                                    };

                                    // Compare-mode annotation (Issue #8620).
                                    binary_compare_check(
                                        op,
                                        &self.infer_expr_type(left),
                                        &self.infer_expr_type(right),
                                        "NeedsRuntime",
                                    );
                                    self.emit(Instr::CallDynamicBinaryBoth(
                                        fallback_intrinsic,
                                        candidates,
                                    ));
                                    return Ok(ValueType::Any);
                                }
                            }
                            // resolver_overrides_to_builtin == true: fall through to
                            // the typed-intrinsic path (builtin section below).
                        }

                        if has_base_extensions {
                            // There are Base extension methods, but they didn't match at compile time.
                            // This could be a runtime dispatch case. Fall through to builtin handling
                            // for primitive types, which will use builtin operators.
                        } else {
                            // No Base extension methods, and no user-defined method matched.
                            // This is a MethodError - no fallback to builtin.
                            // Issue #2127: Allow String*Char to fall through to string concatenation
                            let is_str_char_mul = matches!(op, BinaryOp::Mul)
                                && (matches!(left_julia_ty, JuliaType::String | JuliaType::Char)
                                    || matches!(
                                        right_julia_ty,
                                        JuliaType::String | JuliaType::Char
                                    ));
                            let left_value_ty = self.infer_expr_type(left);
                            let right_value_ty = self.infer_expr_type(right);
                            let left_is_array_like = is_array_or_memory_value_type(&left_value_ty);
                            let right_is_array_like =
                                is_array_or_memory_value_type(&right_value_ty);
                            let left_julia_is_array_like =
                                is_user_array_runtime_dispatch_candidate_type(&left_julia_ty);
                            let right_julia_is_array_like =
                                is_user_array_runtime_dispatch_candidate_type(&right_julia_ty);
                            let right_is_scalar =
                                is_scalar_numeric_or_complex_value_type(&right_value_ty);
                            let array_like_builtin = match op {
                                BinaryOp::Eq | BinaryOp::Ne => {
                                    left_is_array_like
                                        || right_is_array_like
                                        || left_julia_is_array_like
                                        || right_julia_is_array_like
                                }
                                BinaryOp::Add | BinaryOp::Sub => {
                                    left_is_array_like && right_is_array_like
                                }
                                BinaryOp::Div => {
                                    (left_is_array_like && right_is_scalar)
                                        || (is_array_value_type(&left_value_ty)
                                            && is_array_value_type(&right_value_ty))
                                }
                                _ => false,
                            };
                            if !is_str_char_mul && !array_like_builtin {
                                return self.emit_binary_no_method_error(
                                    left,
                                    right,
                                    op_name,
                                    &left_julia_ty,
                                    &right_julia_ty,
                                );
                            }
                        }
                    }
                }
            }
        }

        // Builtin operator handling
        let left_ty = self.infer_expr_type(left);
        let right_ty = self.infer_expr_type(right);
        // Default struct equality comparison (when no user-defined == exists)
        // This handles structs that have no operator methods defined at all
        if matches!(op, BinaryOp::Eq) {
            let left_is_struct = matches!(left_ty, ValueType::Struct(_));
            let right_is_struct = matches!(right_ty, ValueType::Struct(_));
            if left_is_struct && right_is_struct {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::EqStruct);
                return Ok(ValueType::Bool);
            }
        }

        // String comparison: "a" == "b", "a" != "b", "a" < "b", etc.
        if matches!(
            op,
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
        ) {
            let left_is_str = matches!(left_ty, ValueType::Str);
            let right_is_str = matches!(right_ty, ValueType::Str);
            if left_is_str && right_is_str {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
                    self.emit(Instr::EqStr);
                    if matches!(op, BinaryOp::Ne) {
                        self.emit(Instr::NotBool);
                    }
                } else {
                    // String ordering comparison (lexicographic, Issue #2025)
                    match op {
                        BinaryOp::Lt => self.emit(Instr::LtStr),
                        BinaryOp::Le => self.emit(Instr::LeStr),
                        BinaryOp::Gt => self.emit(Instr::GtStr),
                        BinaryOp::Ge => self.emit(Instr::GeStr),
                        _ => {
                            return err(format!(
                                "internal: unexpected string comparison operator {:?}",
                                op
                            ))
                        }
                    };
                }
                return Ok(ValueType::Bool);
            }
        }

        // Symbol ordering: `:a < :b`, `:a <= :b`, etc. Runtime fallback already
        // implements upstream's lexicographic `isless(::Symbol, ::Symbol)` rule.
        if matches!(
            op,
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
        ) && matches!(left_ty, ValueType::Symbol)
            && matches!(right_ty, ValueType::Symbol)
        {
            self.compile_expr(left)?;
            self.compile_expr(right)?;
            let fallback_intrinsic = match op {
                BinaryOp::Lt => Intrinsic::LtFloat,
                BinaryOp::Le => Intrinsic::LeFloat,
                BinaryOp::Gt => Intrinsic::GtFloat,
                BinaryOp::Ge => Intrinsic::GeFloat,
                _ => return err("internal: unexpected symbol ordering operator"),
            };
            self.emit(Instr::CallDynamicBinaryBoth(fallback_intrinsic, Vec::new()));
            return Ok(ValueType::Bool);
        }

        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let left_is_str = matches!(left_ty, ValueType::Str);
            let right_is_str = matches!(right_ty, ValueType::Str);
            // Union types must be treated as runtime-unknown for this
            // constant-folded `String vs non-String` shortcut: the
            // dynamic value behind a `Union{Int64, String}` slot may
            // actually be a `String` that should compare equal to the
            // literal on the other side. Folding to `false` here would
            // silently miscompile `f() == "s"` when inference returns
            // `Union{Int64, String}` (Issue #4686 / latent cause of
            // Issue #4682). The runtime-dispatch path below already
            // groups `Union(_)` with `Any` via `has_any`, so deferring
            // here picks up the correct String-equality at runtime.
            let left_is_runtime_unknown = matches!(left_ty, ValueType::Any | ValueType::Union(_));
            let right_is_runtime_unknown = matches!(right_ty, ValueType::Any | ValueType::Union(_));
            if left_is_str != right_is_str && !(left_is_runtime_unknown || right_is_runtime_unknown)
            {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::Pop);
                self.emit(Instr::Pop);
                self.emit(Instr::PushBool(matches!(op, BinaryOp::Ne)));
                return Ok(ValueType::Bool);
            }
        }

        // Singleton equality comparison: x == nothing, :foo == :bar, 'a' == 'b'
        // For singleton types, equality (==) and identity (===) are semantically equivalent.
        // Uses identity comparison (===) via BuiltinId::Egal for all singleton types.
        // This ensures proper type narrowing: `if x != nothing` works like `if x !== nothing`.
        // Type values are deliberately excluded: `==(::Type, ::Type)` is
        // semantic type equality, while `===` remains object identity.
        // SINGLETON_HANDLING: When modifying identity ops, update equality ops too.
        // See also: is_singleton_type() in compile/mod.rs
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let left_is_singleton = is_singleton_type(&left_ty);
            let right_is_singleton = is_singleton_type(&right_ty);
            // Also check if the literal is nothing (for cases where type inference returns Any)
            let left_is_nothing_lit =
                matches!(left, Expr::Literal(crate::ir::core::Literal::Nothing, _));
            let right_is_nothing_lit =
                matches!(right, Expr::Literal(crate::ir::core::Literal::Nothing, _));
            if left_is_singleton
                || right_is_singleton
                || left_is_nothing_lit
                || right_is_nothing_lit
            {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::CallBuiltin(crate::builtins::BuiltinId::Egal, 2));
                if matches!(op, BinaryOp::Ne) {
                    self.emit(Instr::NotBool);
                }
                return Ok(ValueType::Bool);
            }
        }

        // Mixed / cross-type `==`/`!=` where one operand is a *non-numeric*
        // dispatch-first-equality datatype (its `==` is field-structural or
        // identity, never numeric) and the operand pair has no specific
        // structural `==` method: upstream falls back to `==(x, y) = x === y`
        // (Base.operators). Route to identity (`Egal`) instead of the numeric
        // coercion path below, which would try to coerce the datatype to `I64`
        // and error "Cannot convert Expr to I64" (Issue #9183 sibling cases:
        // `:(x+1) == 5`, `5 == :(x+1)`, `:(x+1) == QuoteNode(:x)`).
        //
        // This is the same `===`-fallback mechanism the String- and
        // singleton-equality branches above already give `String`/`Symbol`/
        // `Char`/`Type` mixed pairs; it structurally covers the remaining
        // dispatch-first AST datatypes (`Expr`/`QuoteNode`) that are neither
        // string- nor singleton-typed. Guards:
        //  - `Bool` is excluded (`is_builtin_numeric`): it is numeric-promotable,
        //    so `true == 1` must stay on the numeric path and yield `true`.
        //  - Pairs WITH a specific structural method (`Expr == Expr`,
        //    `QuoteNode == QuoteNode`) already returned earlier via
        //    `matching_dispatch_first_equality_method`; requiring `.is_none()`
        //    here keeps this branch from stealing them under any reordering.
        //  - A runtime-unknown (`Any`/`Union`) partner defers to the runtime
        //    dispatch below: `===` only equals `==` when both operand types are
        //    statically known (an `Any` partner could hold an `AbstractString`
        //    or an equal `Expr` whose `==` is value- not identity-based).
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let left_jt = self.infer_julia_type(left);
            let right_jt = self.infer_julia_type(right);
            let non_numeric_dispatch_first =
                |ty: &JuliaType| is_dispatch_first_equality_type(ty) && !ty.is_builtin_numeric();
            let statically_known =
                |ty: &ValueType| !matches!(ty, ValueType::Any | ValueType::Union(_));
            if (non_numeric_dispatch_first(&left_jt) || non_numeric_dispatch_first(&right_jt))
                && statically_known(&left_ty)
                && statically_known(&right_ty)
                && self
                    .matching_dispatch_first_equality_method(&left_jt, &right_jt)
                    .is_none()
            {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::CallBuiltin(crate::builtins::BuiltinId::Egal, 2));
                if matches!(op, BinaryOp::Ne) {
                    self.emit(Instr::NotBool);
                }
                return Ok(ValueType::Bool);
            }
        }

        // Array/Memory equality fallback: [1,2,3] == [1,2,3], Memory{T}(n) == Memory{T}(n).
        // Uses the internal element-wise `==` builtin without lowering to operator().
        //
        // For `!=`, explicitly mirror upstream's `!=(x, y) = !(x == y)` so
        // user-defined `==` methods remain visible before this fallback.
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let left_is_array_like = is_array_or_memory_value_type(&left_ty);
            let right_is_array_like = is_array_or_memory_value_type(&right_ty);
            if left_is_array_like || right_is_array_like {
                if matches!(op, BinaryOp::Ne) {
                    self.compile_binary_op(&BinaryOp::Eq, left, right)?;
                    self.emit(Instr::NotBool);
                    return Ok(ValueType::Bool);
                }
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::CallBuiltin(BuiltinId::TupleEquals, 2));
                return Ok(ValueType::Bool);
            }
        }

        if matches!(op, BinaryOp::Mul) {
            let left_julia_ty = self.infer_julia_type(left);
            let right_julia_ty = self.infer_julia_type(right);
            let left_is_string_like = matches!(left_julia_ty, JuliaType::String | JuliaType::Char);
            let right_is_string_like =
                matches!(right_julia_ty, JuliaType::String | JuliaType::Char);
            if left_is_string_like && right_is_string_like {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::StringConcat(2));
                return Ok(ValueType::Str);
            }
            if left_is_string_like || right_is_string_like {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                let candidates: Vec<usize> = self
                    .method_tables
                    .get("*")
                    .map(|table| {
                        table
                            .methods
                            .iter()
                            .filter(|m| m.param_count() == 2)
                            .map(|m| m.global_index)
                            .collect()
                    })
                    .unwrap_or_default();
                self.emit(Instr::CallDynamicBinaryBoth(
                    Intrinsic::DynamicMul,
                    candidates,
                ));
                return Ok(ValueType::Any);
            }
        }

        // Matrix/vector multiplication: A * v or A * B
        if matches!(op, BinaryOp::Mul) {
            let left_is_array = is_array_value_type(&left_ty);
            let right_is_array = is_array_value_type(&right_ty);
            if left_is_array && right_is_array {
                let left_julia_ty = self.infer_julia_type(left);
                let right_julia_ty = self.infer_julia_type(right);
                let involves_diagonal =
                    mul_involves_diagonal(left, right, &left_julia_ty, &right_julia_ty);
                if !involves_diagonal {
                    let may_be_string_slice =
                        is_slice_index_expr(left) || is_slice_index_expr(right);
                    let result_ty = if may_be_string_slice {
                        ValueType::Any
                    } else {
                        ValueType::Array
                    };
                    if let Some(table) = self.method_tables.get("*") {
                        let candidates = dedupe_binary_candidates_keep_first(
                            table
                                .methods
                                .iter()
                                .filter(|m| {
                                    Self::is_linalg_mul_candidate_method(
                                        m,
                                        may_be_string_slice,
                                        |idx, core| {
                                            let (actual_vt, actual) = if idx == 0 {
                                                (&left_ty, &left_julia_ty)
                                            } else {
                                                (&right_ty, &right_julia_ty)
                                            };
                                            core_linalg_array_candidate_compatible(
                                                actual_vt, actual, core,
                                            )
                                        },
                                    )
                                })
                                .map(|m| {
                                    let (left_name, right_name) =
                                        Self::binary_param_display_pair(m);
                                    (m.global_index, left_name, right_name)
                                })
                                .collect(),
                        );

                        if !candidates.is_empty() {
                            self.compile_expr(left)?;
                            self.compile_expr(right)?;
                            self.emit(Instr::CallDynamicBinaryBoth(
                                Intrinsic::DynamicMul,
                                candidates,
                            ));
                            return Ok(result_ty);
                        }
                    }

                    // Compile both operands as arrays
                    self.compile_expr(left)?;
                    self.compile_expr(right)?;
                    self.emit(Instr::MatMul);
                    return Ok(result_ty);
                }
            }

            // Scalar * Array or Array * Scalar: use dynamic dispatch
            // This handles cases like Float64 * Complex array or Complex scalar * Float64 array
            let left_is_scalar = is_scalar_numeric_or_complex_value_type(&left_ty);
            let right_is_scalar = is_scalar_numeric_or_complex_value_type(&right_ty);

            if (left_is_scalar && right_is_array) || (left_is_array && right_is_scalar) {
                // Use CallDynamicBinaryBoth with DynamicMul fallback
                // Runtime will detect complex arrays and handle appropriately
                self.compile_expr(left)?;
                self.compile_expr(right)?;

                // Collect any user-defined * methods for dynamic dispatch
                let candidates: Vec<usize> = if let Some(table) = self.method_tables.get("*") {
                    table
                        .methods
                        .iter()
                        .filter(|m| m.param_count() == 2)
                        .map(|m| m.global_index)
                        .collect()
                } else {
                    vec![]
                };

                self.emit(Instr::CallDynamicBinaryBoth(
                    Intrinsic::DynamicMul,
                    candidates,
                ));
                return Ok(ValueType::Array);
            }
        }

        // Array / Scalar or Scalar / Array: use dynamic dispatch (Issue #1929)
        // In Julia, v / n is equivalent to v ./ n (element-wise broadcast division)
        if matches!(op, BinaryOp::Div) {
            let left_is_array = is_array_value_type(&left_ty);
            let right_is_array = is_array_value_type(&right_ty);
            let left_is_memory = is_memory_value_type(&left_ty);
            let right_is_memory = is_memory_value_type(&right_ty);
            let left_is_scalar = is_scalar_numeric_or_complex_value_type(&left_ty);
            let right_is_scalar = is_scalar_numeric_or_complex_value_type(&right_ty);

            if (left_is_memory && right_is_scalar) || (left_is_scalar && right_is_memory) {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::DynamicDiv);
                return Ok(ValueType::Array);
            }

            if (left_is_scalar && right_is_array) || (left_is_array && right_is_scalar) {
                self.compile_expr(left)?;
                self.compile_expr(right)?;

                // Collect any user-defined / methods for dynamic dispatch
                let candidates: Vec<usize> = if let Some(table) = self.method_tables.get("/") {
                    table
                        .methods
                        .iter()
                        .filter(|m| m.param_count() == 2)
                        .map(|m| m.global_index)
                        .collect()
                } else {
                    vec![]
                };

                self.emit(Instr::CallDynamicBinaryBoth(
                    Intrinsic::DynamicDiv,
                    candidates,
                ));
                return Ok(ValueType::Array);
            }
        }

        // Array element-wise arithmetic: A + B, A - B, A / B
        // In Julia, + and - on arrays are element-wise, while * is matrix multiplication.
        // These are dispatched through DynamicAdd/Sub/Div which handle Array operands at runtime.
        let left_is_array_like = is_array_or_memory_value_type(&left_ty);
        let right_is_array_like = is_array_or_memory_value_type(&right_ty);
        if left_is_array_like && right_is_array_like {
            let dynamic_instr = match op {
                BinaryOp::Add => Some(Instr::DynamicAdd),
                BinaryOp::Sub => Some(Instr::DynamicSub),
                BinaryOp::Div
                    if is_array_value_type(&left_ty) && is_array_value_type(&right_ty) =>
                {
                    Some(Instr::DynamicDiv)
                }
                _ => None,
            };
            if let Some(instr) = dynamic_instr {
                let op_name = binary_op_to_function_name(op);
                let user_array_candidates: Vec<usize> = self
                    .method_tables
                    .get(op_name)
                    .map(|table| {
                        table
                            .methods
                            .iter()
                            .filter(|m| {
                                method_binary_params_match(m, |c0, c1| {
                                    core_is_user_array_runtime_dispatch_candidate_type(c0)
                                        || core_is_user_array_runtime_dispatch_candidate_type(c1)
                                })
                            })
                            .map(|m| m.global_index)
                            .collect()
                    })
                    .unwrap_or_default();
                if !user_array_candidates.is_empty() {
                    let fallback_intrinsic = match op {
                        BinaryOp::Add => Intrinsic::DynamicAdd,
                        BinaryOp::Sub => Intrinsic::DynamicSub,
                        BinaryOp::Div => Intrinsic::DynamicDiv,
                        _ => unreachable!("guarded by dynamic_instr"),
                    };
                    self.compile_expr(left)?;
                    self.compile_expr(right)?;
                    self.emit(Instr::CallDynamicBinaryBoth(
                        fallback_intrinsic,
                        user_array_candidates,
                    ));
                    return Ok(ValueType::Any);
                }
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(instr);
                return Ok(ValueType::Array);
            }
        }

        // BigInt arithmetic operations - if either operand is BigInt, use BigInt intrinsics
        // Issue #2497: Skip when other operand is a struct (Rational, Complex) or Any
        // (which could be a struct at runtime) — these need promotion-based dispatch.
        // Issue #3621: Int128 is intentionally NOT included here so it preserves its
        // type instead of widening to BigInt. The Int128 early-route above handles
        // I128 cases; BigInt+I128 is still caught by the upper BigInt early-route.
        // Issue #3743: BigInt + Float* must produce BigFloat — defer to the
        // BigFloat route below.
        let bigint_meets_float = (left_ty == ValueType::BigInt && is_float_type(&right_ty))
            || (right_ty == ValueType::BigInt && is_float_type(&left_ty));
        let is_bigint =
            (left_ty == ValueType::BigInt || right_ty == ValueType::BigInt) && !bigint_meets_float;
        let other_needs_dispatch = matches!(left_ty, ValueType::Struct(_) | ValueType::Any)
            || matches!(right_ty, ValueType::Struct(_) | ValueType::Any);
        if is_bigint && !other_needs_dispatch {
            // Compile both operands (BigInt intrinsics handle I64 promotion)
            self.compile_expr(left)?;
            self.compile_expr(right)?;
            let intrinsic = match op {
                BinaryOp::Add => Intrinsic::AddBigInt,
                BinaryOp::Sub => Intrinsic::SubBigInt,
                BinaryOp::Mul => Intrinsic::MulBigInt,
                // Issue #8900: BigInt `/` returns BigFloat; only `÷` (IntDiv) stays integer.
                BinaryOp::Div => Intrinsic::DivBigFloat,
                BinaryOp::IntDiv => Intrinsic::DivBigInt,
                BinaryOp::Mod => Intrinsic::RemBigInt,
                BinaryOp::Pow => Intrinsic::PowBigInt, // Issue #1708: BigInt power with Int64 exponent
                BinaryOp::Lt => Intrinsic::LtBigInt,
                BinaryOp::Le => Intrinsic::LeBigInt,
                BinaryOp::Gt => Intrinsic::GtBigInt,
                BinaryOp::Ge => Intrinsic::GeBigInt,
                BinaryOp::Eq => Intrinsic::EqBigInt,
                BinaryOp::Ne => Intrinsic::NeBigInt,
                _ => return err(format!("Unsupported BigInt operation: {:?}", op)),
            };
            self.emit(Instr::CallIntrinsic(intrinsic));
            // Comparison operations return Bool; `/` returns BigFloat; others return BigInt
            let result_ty = match op {
                BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Ne => ValueType::Bool,
                // Issue #8900: BigInt `/` (float division) returns BigFloat
                BinaryOp::Div => ValueType::BigFloat,
                _ => ValueType::BigInt,
            };
            return Ok(result_ty);
        }

        // BigFloat arithmetic operations - if either operand is BigFloat, use BigFloat intrinsics
        // Issue #2497: Skip when other operand is a struct/Any — needs promotion dispatch
        // Issue #3743: also include BigInt + Float* (promoted to BigFloat at runtime).
        let is_bigfloat =
            left_ty == ValueType::BigFloat || right_ty == ValueType::BigFloat || bigint_meets_float;
        if is_bigfloat && !other_needs_dispatch {
            // Compile both operands (BigFloat intrinsics handle F64/I64 promotion)
            self.compile_expr(left)?;
            self.compile_expr(right)?;
            let intrinsic = match op {
                BinaryOp::Add => Intrinsic::AddBigFloat,
                BinaryOp::Sub => Intrinsic::SubBigFloat,
                BinaryOp::Mul => Intrinsic::MulBigFloat,
                BinaryOp::Div => Intrinsic::DivBigFloat,
                BinaryOp::Mod => Intrinsic::RemBigFloat,
                BinaryOp::Lt => Intrinsic::LtBigFloat,
                BinaryOp::Le => Intrinsic::LeBigFloat,
                BinaryOp::Gt => Intrinsic::GtBigFloat,
                BinaryOp::Ge => Intrinsic::GeBigFloat,
                BinaryOp::Eq => Intrinsic::EqBigFloat,
                BinaryOp::Ne => Intrinsic::NeBigFloat,
                _ => return err(format!("Unsupported BigFloat operation: {:?}", op)),
            };
            self.emit(Instr::CallIntrinsic(intrinsic));
            // Comparison operations return Bool, others return BigFloat
            let result_ty = match op {
                BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Ne => ValueType::Bool,
                _ => ValueType::BigFloat,
            };
            return Ok(result_ty);
        }

        // Non-primitive power (e.g., Rational, Complex): use DynamicPow for runtime dispatch.
        if matches!(op, BinaryOp::Pow) {
            let left_primitive = matches!(left_ty, ValueType::I64 | ValueType::F64);
            let right_primitive = matches!(right_ty, ValueType::I64 | ValueType::F64);
            if !(left_primitive && right_primitive) {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.emit(Instr::DynamicPow);
                return Ok(ValueType::Any);
            }
        }

        // Note: Complex operations are handled via Pure Julia method dispatch above.
        // This fallback code is for primitive types only (I64, F64).
        // Complex arithmetic/comparison uses base/complex.jl with Base.:+ etc.

        // Check if either operand is a float type (F16, F32, F64)
        let has_float = is_float_type(&left_ty) || is_float_type(&right_ty);
        // Check if both operands are F32 (for type preservation)
        let both_f32 = left_ty == ValueType::F32 && right_ty == ValueType::F32;
        // Check if one operand is F32 and the other is promotable to F32 (not F64)
        // Issue #1759: Float32 + Bool should return Float32, not Float64
        let has_f64 = left_ty == ValueType::F64 || right_ty == ValueType::F64;
        let has_f32 = left_ty == ValueType::F32 || right_ty == ValueType::F32;
        let one_f32_other_promotable = has_f32 && !has_f64;
        // Check if both operands are F16 or one is F16 with promotable type (Issue #1972)
        let both_f16 = left_ty == ValueType::F16 && right_ty == ValueType::F16;
        let one_f16_other_promotable =
            (left_ty == ValueType::F16 || right_ty == ValueType::F16) && !has_f64 && !has_f32;
        // Issue #2123: Char arithmetic type tracking
        let left_is_char = left_ty == ValueType::Char;
        let right_is_char = right_ty == ValueType::Char;
        let has_char = left_is_char || right_is_char;

        // Julia defines *(::Bool, ::Bool) as Bool-preserving logical AND.
        // Keep this scalar path distinct from integer multiplication so Bool
        // slots do not receive an Int64 result.
        if matches!(op, BinaryOp::Mul) && left_ty == ValueType::Bool && right_ty == ValueType::Bool
        {
            self.compile_expr_as(left, ValueType::Bool)?;
            self.compile_expr_as(right, ValueType::Bool)?;
            self.emit(Instr::CallIntrinsic(Intrinsic::AndInt));
            return Ok(ValueType::Bool);
        }

        // Check if either operand is Any type (e.g., function call results)
        // When Any is involved, use runtime dispatch to determine the correct operation
        // based on actual runtime types (e.g., real(z) + imag(z) where types are unknown at compile time)
        // Issues #3535/#3536: Union types behave like Any here — the slot can hold any
        // of several types at runtime, so the static numeric promotion path is invalid.
        let has_any = matches!(left_ty, ValueType::Any | ValueType::Union(_))
            || matches!(right_ty, ValueType::Any | ValueType::Union(_));

        // Issue #2497: Also need runtime dispatch when mixing BigInt/BigFloat with struct types
        // (e.g., big(2) + Rational{Int64}(1,3)). The early BigInt guard already skips intrinsics
        // for these cases, but we also need to prevent the I64 fallback below from being reached.
        let needs_mixed_dispatch = matches!(
            (&left_ty, &right_ty),
            (
                ValueType::BigInt | ValueType::BigFloat,
                ValueType::Struct(_)
            ) | (
                ValueType::Struct(_),
                ValueType::BigInt | ValueType::BigFloat
            )
        );

        // If either operand is Any or needs mixed dispatch, use runtime dispatch
        if has_any || needs_mixed_dispatch {
            // Compare-mode annotation (Issue #8620): compile-time chose NeedsRuntime
            // because at least one operand is Any/Union or requires mixed dispatch.
            binary_compare_check(op, &left_ty, &right_ty, "NeedsRuntime");
            // Compile both operands without type conversion
            self.compile_expr(left)?;
            self.compile_expr(right)?;

            // For power operations, use DynamicPow which correctly handles I64^I64 -> I64
            if matches!(op, BinaryOp::Pow) {
                self.emit(Instr::DynamicPow);
                return Ok(ValueType::Any);
            }

            // Determine fallback intrinsic based on operation
            let fallback_intrinsic = match op {
                BinaryOp::Add => Intrinsic::DynamicAdd,
                BinaryOp::Sub => Intrinsic::DynamicSub,
                BinaryOp::Mul => Intrinsic::DynamicMul,
                BinaryOp::Div => Intrinsic::DynamicDiv,
                BinaryOp::IntDiv => Intrinsic::SdivInt,
                BinaryOp::Pow => return err("internal: Pow should be handled by DynamicPow"),
                BinaryOp::Lt => Intrinsic::LtFloat,
                BinaryOp::Le => Intrinsic::LeFloat,
                BinaryOp::Gt => Intrinsic::GtFloat,
                BinaryOp::Ge => Intrinsic::GeFloat,
                BinaryOp::Eq => Intrinsic::EqFloat,
                BinaryOp::Ne => Intrinsic::NeFloat,
                BinaryOp::Mod => Intrinsic::SremInt, // Runtime dispatch handles BigInt via RemBigInt
                BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Egal
                | BinaryOp::NotEgal
                | BinaryOp::Subtype => Intrinsic::EqInt,
            };

            // Build candidates from struct/abstract-accepting methods for runtime dispatch (fixes Issue #1055)
            // When Any is involved at compile time, operands could be structs at runtime.
            let op_name = binary_op_to_function_name(op);
            let candidates: Vec<usize> = if let Some(table) = self.method_tables.get(op_name) {
                table
                    .methods
                    .iter()
                    .filter(|m| self.is_binary_runtime_dispatch_candidate_method(m))
                    .map(|m| m.global_index)
                    .collect()
            } else {
                vec![]
            };

            self.emit(Instr::CallDynamicBinaryBoth(fallback_intrinsic, candidates));

            // Return Any since we don't know the result type at compile time
            // (except for comparisons which always return Bool)
            let result_ty = match op {
                BinaryOp::Lt
                | BinaryOp::Gt
                | BinaryOp::Le
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Egal
                | BinaryOp::NotEgal
                | BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Subtype => ValueType::Bool,
                _ => ValueType::Any,
            };
            return Ok(result_ty);
        }

        // Check if both operands are numeric (for type promotion)

        // Compare-mode annotation (Issue #8620): compile-time chose UniqueBuiltin —
        // both operands are concrete primitive numerics and we are about to emit a
        // typed intrinsic (or DynamicPow/DynamicMod/etc.).  Log if the resolver
        // disagrees (e.g. for I128/U128 pairs that go through CallDynamicBinaryBoth
        // in the early routes above before reaching this fallback).
        binary_compare_check(op, &left_ty, &right_ty, "UniqueBuiltin");

        let result_ty = match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
                if both_f32 || one_f32_other_promotable {
                    ValueType::F32 // Preserve Float32 when F32 + promotable type (Issue #1759)
                } else if both_f16 || one_f16_other_promotable {
                    ValueType::F16 // Preserve Float16 (Issue #1972)
                } else if has_float {
                    ValueType::F64
                } else if has_char && matches!(op, BinaryOp::Add) {
                    // Issue #2123: Char + Int -> Char, Int + Char -> Char
                    ValueType::Char
                } else if has_char && matches!(op, BinaryOp::Sub) && left_is_char && !right_is_char
                {
                    // Issue #2123: Char - Int -> Char (but Char - Char -> Int)
                    ValueType::Char
                } else if let Some(small_ty) = same_small_int_type(&left_ty, &right_ty) {
                    // Issue #2278: Preserve small integer type (e.g., Int8+Int8 -> Int8)
                    small_ty
                } else {
                    ValueType::I64
                }
            }
            BinaryOp::Div => {
                if both_f32 || one_f32_other_promotable {
                    ValueType::F32 // Float32 / promotable -> Float32 (Issue #1759)
                } else if both_f16 || one_f16_other_promotable {
                    ValueType::F16 // Float16 / promotable -> Float16 (Issue #1972)
                } else {
                    ValueType::F64
                }
            }
            BinaryOp::Pow => {
                // Power operator: Int^Int -> Int, otherwise -> Float64 (Julia semantics)
                if !has_float {
                    if let Some(small_ty) = same_small_int_type(&left_ty, &right_ty) {
                        // Issue #2278: Preserve small integer type
                        small_ty
                    } else {
                        ValueType::I64
                    }
                } else if both_f32 || one_f32_other_promotable {
                    ValueType::F32
                } else if both_f16 || one_f16_other_promotable {
                    ValueType::F16 // Issue #1972
                } else {
                    ValueType::F64
                }
            }
            BinaryOp::IntDiv => {
                // Integer division: preserve float type, otherwise I64
                if both_f32 || one_f32_other_promotable {
                    ValueType::F32
                } else if both_f16 || one_f16_other_promotable {
                    ValueType::F16 // Issue #1972
                } else if has_float {
                    ValueType::F64
                } else if let Some(small_ty) = same_small_int_type(&left_ty, &right_ty) {
                    // Issue #2278: Preserve small integer type
                    small_ty
                } else {
                    ValueType::I64
                }
            }
            BinaryOp::Mod => {
                // Modulo: preserve float type, otherwise I64
                if both_f32 || one_f32_other_promotable {
                    ValueType::F32
                } else if both_f16 || one_f16_other_promotable {
                    ValueType::F16 // Issue #1972
                } else if has_float {
                    ValueType::F64
                } else if let Some(small_ty) = same_small_int_type(&left_ty, &right_ty) {
                    // Issue #2278: Preserve small integer type
                    small_ty
                } else {
                    ValueType::I64
                }
            }
            BinaryOp::Lt
            | BinaryOp::Gt
            | BinaryOp::Le
            | BinaryOp::Ge
            | BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Egal
            | BinaryOp::NotEgal
            | BinaryOp::Subtype => {
                ValueType::Bool // Comparisons return Bool (Julia semantics)
            }
            BinaryOp::And | BinaryOp::Or => ValueType::Bool, // Logical operators return Bool
        };

        // Issue #3566: UInt64 comparison must compare natively as u64, not via Int64
        // (which wraps for values > i64::MAX). Route through CallDynamicBinaryBoth so
        // the runtime u64 comparison path in execute_binary_both handles it.
        // Issue #3696: same applies to UInt128 — i64 conversion overflows for any
        // value above i64::MAX. Route through CallDynamicBinaryBoth where the
        // runtime u128 comparison path handles it natively.
        if matches!(
            op,
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
        ) && (left_ty == ValueType::U64
            || right_ty == ValueType::U64
            || left_ty == ValueType::U128
            || right_ty == ValueType::U128)
        {
            self.compile_expr(left)?;
            self.compile_expr(right)?;
            let intrinsic = match op {
                BinaryOp::Eq => Intrinsic::EqInt,
                BinaryOp::Ne => Intrinsic::NeInt,
                BinaryOp::Lt => Intrinsic::SltInt,
                BinaryOp::Le => Intrinsic::SleInt,
                BinaryOp::Gt => Intrinsic::SgtInt,
                BinaryOp::Ge => Intrinsic::SgeInt,
                _ => unreachable!(),
            };
            self.emit(Instr::CallDynamicBinaryBoth(intrinsic, vec![]));
            return Ok(ValueType::Bool);
        }

        // For comparisons, use the operand types, not the result type (which is always Bool)
        // Note: has_any cases are handled above with CallDynamicBinaryBoth
        // Issue #1759: Use F64 operand type for all float operations because intrinsics only
        // support I64 and F64. For F32 results, we convert F64 back to F32 at the end.
        let operand_ty = match op {
            BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => {
                if has_float || both_f32 || one_f32_other_promotable {
                    ValueType::F64 // Intrinsics require F64
                } else {
                    ValueType::I64
                }
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                if has_float || both_f32 || one_f32_other_promotable {
                    ValueType::F64 // Intrinsics require F64
                } else {
                    ValueType::I64
                }
            }
            // For division: always use F64 for computation
            BinaryOp::Div => ValueType::F64,
            // For multiplication with floats, use F64
            BinaryOp::Mul => {
                if has_float || both_f32 || one_f32_other_promotable {
                    ValueType::F64 // Intrinsics require F64
                } else {
                    ValueType::I64
                }
            }
            // For Add/Sub with floats, use F64
            BinaryOp::Add | BinaryOp::Sub => {
                if has_float || both_f32 || one_f32_other_promotable {
                    ValueType::F64 // Intrinsics require F64
                } else {
                    ValueType::I64
                }
            }
            // Mod with floats: use F64 operand type for computation, result converted back at end
            BinaryOp::Mod => {
                if has_float || both_f32 || one_f32_other_promotable {
                    ValueType::F64 // Intrinsics require F64
                } else {
                    ValueType::I64
                }
            }
            _ => result_ty.clone(),
        };

        self.compile_expr_as(left, operand_ty.clone())?;
        self.compile_expr_as(right, operand_ty.clone())?;

        // Map (BinaryOp, ValueType) to Intrinsic, using CallIntrinsic for Julia-like semantics.
        // This mirrors Julia's design where `1 + 2` calls `Base.add_int(1, 2)`.
        let intrinsic_opt = match (op, operand_ty) {
            // Integer arithmetic -> add_int, sub_int, mul_int, sdiv_int, srem_int
            (BinaryOp::Add, ValueType::I64) => Some(Intrinsic::AddInt),
            (BinaryOp::Sub, ValueType::I64) => Some(Intrinsic::SubInt),
            (BinaryOp::Mul, ValueType::I64) => Some(Intrinsic::MulInt),
            (BinaryOp::IntDiv, ValueType::I64) => Some(Intrinsic::SdivInt),
            (BinaryOp::Mod, ValueType::I64) => Some(Intrinsic::SremInt),
            // Float mod: use DynamicMod for fmod semantics and type preservation (Issue #1762)
            (BinaryOp::Mod, ValueType::F64) => None, // Will use DynamicMod below
            // Float int div: use DynamicIntDiv for type preservation (Issue #1970)
            (BinaryOp::IntDiv, ValueType::F64) => None, // Will use DynamicIntDiv below
            // Power: Use DynamicPow for all cases to preserve integer arithmetic when appropriate
            (BinaryOp::Pow, ValueType::I64) => None, // Will use DynamicPow below
            (BinaryOp::Pow, ValueType::F64) => None, // Will use DynamicPow below

            // Float64 arithmetic -> add_float, sub_float, mul_float, div_float, pow_float
            // Note: Float32 operations also use these intrinsics with F64 operand_ty,
            // and the result is converted back to F32 at the end (Issue #1759).
            (BinaryOp::Add, ValueType::F64) => Some(Intrinsic::DynamicAdd),
            (BinaryOp::Sub, ValueType::F64) => Some(Intrinsic::DynamicSub),
            (BinaryOp::Mul, ValueType::F64) => Some(Intrinsic::DynamicMul),
            (BinaryOp::Div, ValueType::F64) => Some(Intrinsic::DynamicDiv),

            // Integer comparisons -> eq_int, ne_int, slt_int, sle_int, sgt_int, sge_int
            (BinaryOp::Eq, ValueType::I64) => Some(Intrinsic::EqInt),
            (BinaryOp::Ne, ValueType::I64) => Some(Intrinsic::NeInt),
            (BinaryOp::Lt, ValueType::I64) => Some(Intrinsic::SltInt),
            (BinaryOp::Le, ValueType::I64) => Some(Intrinsic::SleInt),
            (BinaryOp::Gt, ValueType::I64) => Some(Intrinsic::SgtInt),
            (BinaryOp::Ge, ValueType::I64) => Some(Intrinsic::SgeInt),

            // Float64 comparisons -> eq_float, ne_float, lt_float, le_float, gt_float, ge_float
            // Note: Float32 comparisons also use these intrinsics with F64 operand_ty (Issue #1759).
            (BinaryOp::Eq, ValueType::F64) => Some(Intrinsic::EqFloat),
            (BinaryOp::Ne, ValueType::F64) => Some(Intrinsic::NeFloat),
            (BinaryOp::Lt, ValueType::F64) => Some(Intrinsic::LtFloat),
            (BinaryOp::Le, ValueType::F64) => Some(Intrinsic::LeFloat),
            (BinaryOp::Gt, ValueType::F64) => Some(Intrinsic::GtFloat),
            (BinaryOp::Ge, ValueType::F64) => Some(Intrinsic::GeFloat),

            // Note: Complex operations use Pure Julia with method dispatch (base/complex.jl)
            // Division always uses F64 intrinsics for primitives
            (BinaryOp::Div, _) => Some(Intrinsic::DynamicDiv),
            // Power operator: use DynamicPow for all cases to preserve integer arithmetic
            (BinaryOp::Pow, _) => None, // Will use DynamicPow in special cases block
            (BinaryOp::Mod, _) => Some(Intrinsic::SremInt),
            // Integer division (÷) - use DynamicIntDiv for float types (Issue #1970)
            (BinaryOp::IntDiv, _) => None, // Will use DynamicIntDiv below

            // Logical AND: use mul_int (both must be non-zero)
            (BinaryOp::And, _) => Some(Intrinsic::MulInt),

            // Logical OR: special handling required
            (BinaryOp::Or, _) => None,

            _ => None,
        };

        match intrinsic_opt {
            Some(intrinsic) => {
                if let Some(instr) = typed_instr_for_intrinsic(intrinsic) {
                    self.emit(instr);
                } else {
                    self.emit(Instr::CallIntrinsic(intrinsic));
                }
            }
            None => {
                // Special cases that need custom handling
                if matches!(op, BinaryOp::Pow) {
                    // Power operator: use DynamicPow to preserve integer arithmetic
                    self.emit(Instr::DynamicPow);
                } else if matches!(op, BinaryOp::Mod) {
                    // Float mod: use DynamicMod for fmod semantics (Issue #1762)
                    self.emit(Instr::DynamicMod);
                } else if matches!(op, BinaryOp::IntDiv) {
                    // Float int div: use DynamicIntDiv for type preservation (Issue #1970)
                    self.emit(Instr::DynamicIntDiv);
                } else if matches!(op, BinaryOp::Or) {
                    // a || b: if (a + b) != 0 then 1 else 0
                    return self.compile_or_expr(left, right);
                } else {
                    // If we reach here, it means dispatch failed - report error
                    return err(format!("Unsupported binary op: {:?}", op));
                }
            }
        }

        // Issue #1759: Convert F64 result back to F32 if needed for arithmetic operations.
        // Intrinsics only operate on F64, so when result should be F32 we need to convert.
        if result_ty == ValueType::F32 {
            self.emit(Instr::DynamicToF32);
        }
        // Issue #1975: Convert F64 result back to F16 if needed for arithmetic operations.
        if result_ty == ValueType::F16 {
            self.emit(Instr::DynamicToF16);
        }
        // Issue #2123: Convert I64 result back to Char for Char+Int/Int+Char/Char-Int arithmetic.
        if result_ty == ValueType::Char {
            self.emit(Instr::CallBuiltin(crate::builtins::BuiltinId::IntToChar, 1));
        }
        // Issue #2278: Convert I64 result back to small integer type
        if let Some(back_conv) = small_int_back_conversion(&result_ty) {
            self.emit(back_conv);
        }

        // Comparisons return Bool (Julia semantics)
        match op {
            BinaryOp::Lt
            | BinaryOp::Gt
            | BinaryOp::Le
            | BinaryOp::Ge
            | BinaryOp::Eq
            | BinaryOp::Ne => Ok(ValueType::Bool),
            _ => Ok(result_ty),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        core_is_binary_runtime_dispatch_candidate_type, core_is_complex_struct_param,
        core_is_dispatch_first_equality_type, core_is_linalg_array_dispatch_type,
        core_is_string_concat_dispatch_type, core_is_user_array_runtime_dispatch_candidate_type,
        core_linalg_array_dispatch_rank, core_param_is_struct_spelling,
        is_binary_runtime_dispatch_candidate_type, is_dispatch_first_equality_type,
        is_linalg_array_dispatch_type, is_string_concat_dispatch_type,
        is_user_array_runtime_dispatch_candidate_type, linalg_array_dispatch_rank,
        typed_instr_for_intrinsic, typed_scalar_binary_instr,
    };
    use crate::bytecode::Instr;
    use crate::inference_core::{core_type_to_julia_type, CoreType};
    use crate::intrinsics::Intrinsic;
    use crate::ir::core::BinaryOp;
    use crate::types::JuliaType;

    #[test]
    fn typed_instr_for_intrinsic_maps_hot_int_and_float_ops() {
        assert!(matches!(
            typed_instr_for_intrinsic(Intrinsic::AddInt),
            Some(Instr::AddI64)
        ));
        assert!(matches!(
            typed_instr_for_intrinsic(Intrinsic::SltInt),
            Some(Instr::LtI64)
        ));
        assert!(matches!(
            typed_instr_for_intrinsic(Intrinsic::DynamicMul),
            Some(Instr::MulF64)
        ));
        assert!(matches!(
            typed_instr_for_intrinsic(Intrinsic::LeFloat),
            Some(Instr::LeF64)
        ));
        assert!(typed_instr_for_intrinsic(Intrinsic::SdivInt).is_none());
    }

    #[test]
    fn typed_scalar_binary_instr_table_matches_per_op_expectations_8192() {
        // Issue #8192: the shared `op + result-kind → Instr` table is the single
        // source of truth for both the main compiler and the runtime specializer.
        // Pin every supported (op, is_float) pair so an accidental edit to the
        // table is caught here rather than silently diverging one codegen path.
        // (`Instr` is not `PartialEq`, so we match on the variant explicitly.)
        macro_rules! assert_instr {
            ($actual:expr, $pat:pat, $msg:expr) => {
                assert!(matches!($actual, Some($pat)), $msg)
            };
        }
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Add, false),
            Instr::AddI64,
            "Add int"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Sub, false),
            Instr::SubI64,
            "Sub int"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Mul, false),
            Instr::MulI64,
            "Mul int"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Mod, false),
            Instr::ModI64,
            "Mod int"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Eq, false),
            Instr::EqI64,
            "Eq int"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Ne, false),
            Instr::NeI64,
            "Ne int"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Lt, false),
            Instr::LtI64,
            "Lt int"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Le, false),
            Instr::LeI64,
            "Le int"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Gt, false),
            Instr::GtI64,
            "Gt int"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Ge, false),
            Instr::GeI64,
            "Ge int"
        );

        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Add, true),
            Instr::AddF64,
            "Add float"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Sub, true),
            Instr::SubF64,
            "Sub float"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Mul, true),
            Instr::MulF64,
            "Mul float"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Div, true),
            Instr::DivF64,
            "Div float"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Eq, true),
            Instr::EqF64,
            "Eq float"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Ne, true),
            Instr::NeF64,
            "Ne float"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Lt, true),
            Instr::LtF64,
            "Lt float"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Le, true),
            Instr::LeF64,
            "Le float"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Gt, true),
            Instr::GtF64,
            "Gt float"
        );
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Ge, true),
            Instr::GeF64,
            "Ge float"
        );

        // `/` is always Float64; "integer `/`" still resolves to DivF64.
        assert_instr!(
            typed_scalar_binary_instr(BinaryOp::Div, false),
            Instr::DivF64,
            "Div int"
        );

        // Ops with no single typed I64/F64 instruction stay on the dynamic path.
        for op in [
            BinaryOp::Pow,
            BinaryOp::IntDiv,
            BinaryOp::And,
            BinaryOp::Or,
            BinaryOp::Egal,
            BinaryOp::NotEgal,
            BinaryOp::Subtype,
        ] {
            assert!(
                typed_scalar_binary_instr(op, false).is_none(),
                "{op:?} (int)"
            );
            assert!(
                typed_scalar_binary_instr(op, true).is_none(),
                "{op:?} (float)"
            );
        }
        // Float `%` has no typed op (DynamicMod); integer `%` does.
        assert!(typed_scalar_binary_instr(BinaryOp::Mod, true).is_none());
    }

    #[test]
    fn typed_instr_for_intrinsic_delegates_to_shared_table_8192() {
        // Issue #8192: `typed_instr_for_intrinsic` must stay a thin adapter over
        // the shared table — confirm the full mapping including the `None`s, so a
        // future edit can't reintroduce a second divergent instruction table.
        macro_rules! assert_instr {
            ($actual:expr, $pat:pat) => {
                assert!(matches!($actual, Some($pat)), stringify!($pat))
            };
        }
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::AddInt), Instr::AddI64);
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::SubInt), Instr::SubI64);
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::MulInt), Instr::MulI64);
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::SremInt), Instr::ModI64);
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::EqInt), Instr::EqI64);
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::NeInt), Instr::NeI64);
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::SltInt), Instr::LtI64);
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::SleInt), Instr::LeI64);
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::SgtInt), Instr::GtI64);
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::SgeInt), Instr::GeI64);
        assert_instr!(
            typed_instr_for_intrinsic(Intrinsic::DynamicAdd),
            Instr::AddF64
        );
        assert_instr!(
            typed_instr_for_intrinsic(Intrinsic::DynamicSub),
            Instr::SubF64
        );
        assert_instr!(
            typed_instr_for_intrinsic(Intrinsic::DynamicMul),
            Instr::MulF64
        );
        assert_instr!(
            typed_instr_for_intrinsic(Intrinsic::DynamicDiv),
            Instr::DivF64
        );
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::EqFloat), Instr::EqF64);
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::NeFloat), Instr::NeF64);
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::LtFloat), Instr::LtF64);
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::LeFloat), Instr::LeF64);
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::GtFloat), Instr::GtF64);
        assert_instr!(typed_instr_for_intrinsic(Intrinsic::GeFloat), Instr::GeF64);
        assert!(typed_instr_for_intrinsic(Intrinsic::SdivInt).is_none());
        assert!(typed_instr_for_intrinsic(Intrinsic::NegInt).is_none());
    }

    #[test]
    fn string_concat_dispatch_accepts_upstream_union_vararg_signature() {
        let upstream_string_mul_arg =
            JuliaType::Union(vec![JuliaType::AbstractChar, JuliaType::AbstractString]);

        assert!(is_string_concat_dispatch_type(&upstream_string_mul_arg));
        assert!(!is_string_concat_dispatch_type(&JuliaType::Union(vec![
            JuliaType::AbstractString,
            JuliaType::Int64
        ])));
    }

    #[test]
    fn bare_dict_set_equality_methods_stay_runtime_candidates_issue_5231() {
        for ty in [JuliaType::Dict, JuliaType::Set] {
            let core = CoreType::from(&ty);
            assert!(
                core_is_binary_runtime_dispatch_candidate_type(&core),
                "{ty:?} equality methods must stay visible to Any-typed operands"
            );
            assert!(
                is_binary_runtime_dispatch_candidate_type(&ty),
                "{ty:?} must stay in the legacy parity oracle"
            );
            assert!(
                core_is_dispatch_first_equality_type(&core),
                "{ty:?} should prefer equality dispatch over numeric fallback"
            );
            assert!(
                is_dispatch_first_equality_type(&ty),
                "{ty:?} must stay in the dispatch-first parity oracle"
            );
        }
    }

    /// User-visible binary-operand spellings: primitives, abstracts, dedicated
    /// container variants, struct/parametric/abstract-user names, unions,
    /// typevars, `Type{...}` shapes (Issue #6495, stage 6b-ii).
    fn user_shapes() -> Vec<JuliaType> {
        vec![
            JuliaType::Any,
            JuliaType::Int64,
            JuliaType::Float64,
            JuliaType::Bool,
            JuliaType::Char,
            JuliaType::String,
            JuliaType::Symbol,
            JuliaType::Nothing,
            JuliaType::Number,
            JuliaType::Real,
            JuliaType::Integer,
            JuliaType::Signed,
            JuliaType::Unsigned,
            JuliaType::AbstractFloat,
            JuliaType::AbstractString,
            JuliaType::AbstractChar,
            JuliaType::AbstractArray,
            JuliaType::AbstractRange,
            JuliaType::Function,
            JuliaType::Type,
            JuliaType::DataType,
            JuliaType::Array,
            JuliaType::Tuple,
            JuliaType::Set,
            JuliaType::Dict,
            JuliaType::NamedTuple,
            JuliaType::UnitRange,
            JuliaType::StepRange,
            JuliaType::Generator,
            JuliaType::IOBuffer,
            JuliaType::VectorOf(Box::new(JuliaType::Int64)),
            JuliaType::MatrixOf(Box::new(JuliaType::Float64)),
            JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Float64]),
            JuliaType::Union(vec![JuliaType::AbstractChar, JuliaType::AbstractString]),
            JuliaType::Union(vec![JuliaType::String, JuliaType::Int64]),
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::TypeVar("T".to_string(), Some("Real".to_string())),
            JuliaType::TypeOf(Box::new(JuliaType::Int64)),
            JuliaType::Struct("Complex{Float64}".to_string()),
            JuliaType::Struct("Complex{T}".to_string()),
            JuliaType::Struct("Rational{Int64}".to_string()),
            JuliaType::Struct("MyStruct".to_string()),
            JuliaType::Struct("MyStruct{Int64}".to_string()),
            JuliaType::Struct("Vector{Int64}".to_string()),
            JuliaType::Struct("Matrix{T}".to_string()),
            JuliaType::Struct("AbstractVector".to_string()),
            JuliaType::Struct("AbstractMatrix{Float64}".to_string()),
            JuliaType::Struct("AbstractArray{T, 2}".to_string()),
            JuliaType::Struct("Diagonal{Float64}".to_string()),
            JuliaType::Struct("Irrational{:π}".to_string()),
            JuliaType::AbstractUser("AbstractPoint".to_string(), None),
            JuliaType::AbstractUser("AbstractVector".to_string(), None),
            JuliaType::UnionAll {
                var: "T".to_string(),
                lower_bound: None,
                bound: Some(Box::new("Real".to_string())),
                body: Box::new(JuliaType::Struct("Complex{T}".to_string())),
            },
        ]
    }

    /// Definitional invariant of the stage 6b-ii ports: every `core_*`
    /// predicate equals its legacy predicate composed with the canonical
    /// inverse, evaluated on the bridged image (Issue #6495).
    #[test]
    fn binary_core_predicates_match_legacy_on_canonical_inverse_issue_6495() {
        for ty in &user_shapes() {
            let core = CoreType::from(ty);
            let inverse = core_type_to_julia_type(&core);
            assert_eq!(
                core_is_binary_runtime_dispatch_candidate_type(&core),
                is_binary_runtime_dispatch_candidate_type(&inverse),
                "binary candidate (inverse) parity for {ty:?} (core {core:?})"
            );
            assert_eq!(
                core_is_user_array_runtime_dispatch_candidate_type(&core),
                is_user_array_runtime_dispatch_candidate_type(&inverse),
                "user-array candidate (inverse) parity for {ty:?} (core {core:?})"
            );
            assert_eq!(
                core_is_linalg_array_dispatch_type(&core),
                is_linalg_array_dispatch_type(&inverse),
                "linalg dispatch (inverse) parity for {ty:?} (core {core:?})"
            );
            assert_eq!(
                core_linalg_array_dispatch_rank(&core),
                linalg_array_dispatch_rank(&inverse),
                "linalg rank (inverse) parity for {ty:?} (core {core:?})"
            );
            assert_eq!(
                core_is_string_concat_dispatch_type(&core),
                is_string_concat_dispatch_type(&inverse),
                "string concat (inverse) parity for {ty:?} (core {core:?})"
            );
            assert_eq!(
                core_is_dispatch_first_equality_type(&core),
                is_dispatch_first_equality_type(&inverse),
                "dispatch-first equality (inverse) parity for {ty:?} (core {core:?})"
            );
            assert_eq!(
                core_is_complex_struct_param(&core),
                matches!(&inverse, JuliaType::Struct(s) if s.starts_with("Complex")),
                "complex-struct (inverse) parity for {ty:?} (core {core:?})"
            );
            assert_eq!(
                core_param_is_struct_spelling(&core),
                matches!(&inverse, JuliaType::Struct(_)),
                "struct-spelling (inverse) parity for {ty:?} (core {core:?})"
            );
        }
    }

    /// Direct parity on round-tripping spellings — the canonical inverse is
    /// what production reconstructs post-deserialization, and the #6336
    /// round-trip gate pins Base method params to these spellings.
    #[test]
    fn binary_core_predicates_match_legacy_for_lowering_spellings_issue_6495() {
        let mut covered = 0usize;
        for ty in &user_shapes() {
            let core = CoreType::from(ty);
            if &core_type_to_julia_type(&core) != ty {
                continue;
            }
            covered += 1;
            assert_eq!(
                core_is_binary_runtime_dispatch_candidate_type(&core),
                is_binary_runtime_dispatch_candidate_type(ty),
                "binary candidate parity for {ty:?}"
            );
            assert_eq!(
                core_is_user_array_runtime_dispatch_candidate_type(&core),
                is_user_array_runtime_dispatch_candidate_type(ty),
                "user-array candidate parity for {ty:?}"
            );
            assert_eq!(
                core_is_linalg_array_dispatch_type(&core),
                is_linalg_array_dispatch_type(ty),
                "linalg dispatch parity for {ty:?}"
            );
            assert_eq!(
                core_linalg_array_dispatch_rank(&core),
                linalg_array_dispatch_rank(ty),
                "linalg rank parity for {ty:?}"
            );
            assert_eq!(
                core_is_string_concat_dispatch_type(&core),
                is_string_concat_dispatch_type(ty),
                "string concat parity for {ty:?}"
            );
            assert_eq!(
                core_is_dispatch_first_equality_type(&core),
                is_dispatch_first_equality_type(ty),
                "dispatch-first equality parity for {ty:?}"
            );
            assert_eq!(
                core_is_complex_struct_param(&core),
                matches!(ty, JuliaType::Struct(s) if s.starts_with("Complex")),
                "complex-struct parity for {ty:?}"
            );
            assert_eq!(
                core_param_is_struct_spelling(&core),
                matches!(ty, JuliaType::Struct(_)),
                "struct-spelling parity for {ty:?}"
            );
        }
        assert!(covered > 20, "round-tripping corpus too small: {covered}");
    }
}
