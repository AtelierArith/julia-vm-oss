use crate::intrinsics::Intrinsic;
use crate::ir::core::Expr;
use crate::vm::{ArrayElementType, Instr, ValueType};

use super::super::types::{err, CResult};
use super::CoreCompiler;

impl CoreCompiler<'_> {
    /// Side-effect-free predicate mirroring the accept/reject decision of
    /// [`CoreCompiler::compile_expr_as`]: returns `true` iff coercing a value of
    /// `actual` ValueType into `target` would be accepted (i.e. would NOT hit the
    /// catch-all `Cannot convert ...` error arm). It emits NOTHING.
    ///
    /// This is the single source of truth for which `(actual, target)` pairs are
    /// coercible: `compile_expr_as` consults it first and only errors when it
    /// returns `false`, so the two cannot drift. Callers that must decide
    /// convertibility WITHOUT emitting bytecode (e.g. the struct field-count
    /// default-constructor fallback, Issue #7793 regression guard) use this
    /// directly. The arm bodies that *follow* in `compile_expr_as` only choose
    /// which conversion instruction (if any) to emit for an accepted pair.
    pub(crate) fn coercion_accepts(&self, actual: &ValueType, target: &ValueType) -> bool {
        if actual == target {
            return true;
        }
        match (actual, target) {
            (ValueType::I64, ValueType::F64) => true,
            (ValueType::F64, ValueType::I64) => true,
            (
                ValueType::I8
                | ValueType::I16
                | ValueType::I32
                | ValueType::I128
                | ValueType::U8
                | ValueType::U16
                | ValueType::U32
                | ValueType::U64
                | ValueType::U128
                | ValueType::F32
                | ValueType::F16,
                ValueType::I64,
            ) => true,
            (
                ValueType::I8
                | ValueType::I16
                | ValueType::I32
                | ValueType::I128
                | ValueType::U8
                | ValueType::U16
                | ValueType::U32
                | ValueType::U64
                | ValueType::U128
                | ValueType::F32
                | ValueType::F16,
                ValueType::F64,
            ) => true,
            (ValueType::Any, ValueType::F64) => true,
            (ValueType::Any, ValueType::I64) => true,
            (ValueType::Any, ValueType::F32) => true,
            (ValueType::Any, ValueType::F16) => true,
            (
                ValueType::I8
                | ValueType::I16
                | ValueType::I32
                | ValueType::I64
                | ValueType::I128
                | ValueType::U8
                | ValueType::U16
                | ValueType::U32
                | ValueType::U64
                | ValueType::U128
                | ValueType::F64
                | ValueType::F16,
                ValueType::F32,
            ) => true,
            (
                ValueType::I8
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
                | ValueType::F64,
                ValueType::F16,
            ) => true,
            (ValueType::Struct(_), ValueType::Any) => true,
            (ValueType::ComplexF32 | ValueType::ComplexF64, ValueType::Any) => true,
            (ValueType::Memory | ValueType::MemoryOf(_), ValueType::Any) => true,
            (ValueType::Memory, ValueType::MemoryOf(_))
            | (ValueType::MemoryOf(_), ValueType::Memory) => true,
            (ValueType::MemoryOf(actual_elem), ValueType::MemoryOf(target_elem)) => {
                memory_element_coercion_is_lossless(actual_elem, target_elem)
            }
            (ValueType::Struct(_), ValueType::I64) => true,
            (ValueType::Struct(_), ValueType::F64) => true,
            (ValueType::ComplexF32 | ValueType::ComplexF64, ValueType::F64) => true,
            (
                ValueType::Struct(type_id),
                complex @ (ValueType::ComplexF32 | ValueType::ComplexF64),
            ) => self
                .shared_ctx
                .get_struct_name(*type_id)
                .is_some_and(|name| complex_value_type_matches_struct_name(complex, &name)),
            (
                complex @ (ValueType::ComplexF32 | ValueType::ComplexF64),
                ValueType::Struct(type_id),
            ) => self
                .shared_ctx
                .get_struct_name(*type_id)
                .is_some_and(|name| complex_value_type_matches_struct_name(complex, &name)),
            (ValueType::Any, ValueType::Struct(_)) => true,
            (ValueType::Any, ValueType::ComplexF32 | ValueType::ComplexF64) => true,
            (ValueType::Any, ValueType::Array) => true,
            (ValueType::Any, ValueType::Memory | ValueType::MemoryOf(_)) => true,
            (ValueType::BigInt, ValueType::Any) => true,
            (
                ValueType::I8
                | ValueType::I16
                | ValueType::I32
                | ValueType::I128
                | ValueType::U8
                | ValueType::U16
                | ValueType::U32
                | ValueType::U64
                | ValueType::U128
                | ValueType::F32
                | ValueType::F16,
                ValueType::Any,
            ) => true,
            (ValueType::I64 | ValueType::F64, ValueType::Any) => true,
            (ValueType::Bool, ValueType::Any) => true,
            (ValueType::Bool, ValueType::Bool) => true,
            (ValueType::Bool, ValueType::I64) => true,
            (ValueType::Bool, ValueType::F64) => true,
            (ValueType::Bool, ValueType::F32) => true,
            (ValueType::I64, ValueType::Bool) => true,
            (ValueType::Any, ValueType::Bool) => true,
            (ValueType::DataType, ValueType::Any) => true,
            (ValueType::Any, ValueType::DataType) => true,
            (ValueType::ArrayOf(_, _), ValueType::Array) => true,
            (ValueType::Array, ValueType::Any) => true,
            (ValueType::ArrayOf(_, _), ValueType::Any) => true,
            (ValueType::Str, ValueType::Any) => true,
            (ValueType::Any, ValueType::Str) => true,
            (ValueType::Char, ValueType::Any) => true,
            (ValueType::Char, ValueType::I64) => true,
            (ValueType::Char, ValueType::F64) => true,
            (ValueType::Tuple, ValueType::Any) => true,
            (ValueType::Any, ValueType::Tuple) => true,
            (ValueType::NamedTuple, ValueType::Any) => true,
            (ValueType::Dict, ValueType::Any) => true,
            (ValueType::Set, ValueType::Any) => true,
            (ValueType::Any, ValueType::Set) => true,
            (ValueType::Range, ValueType::Any) => true,
            (ValueType::Rng, ValueType::Any) => true,
            (ValueType::Nothing, ValueType::Any) => true,
            (ValueType::Missing, ValueType::Any) => true,
            (ValueType::Symbol, ValueType::Any) => true,
            (ValueType::Expr, ValueType::Any) => true,
            (ValueType::Module, ValueType::Any) => true,
            (ValueType::Regex, ValueType::Any) => true,
            (ValueType::RegexMatch, ValueType::Any) => true,
            (ValueType::I64, ValueType::BigInt) => true,
            (ValueType::F64, ValueType::BigFloat) => true,
            (ValueType::BigInt, ValueType::I64) => true,
            (ValueType::BigFloat, ValueType::F64) => true,
            (ValueType::BigInt, ValueType::F64) => true,
            (ValueType::BigFloat, ValueType::Any) => true,
            (ValueType::Any, ValueType::BigInt) => true,
            (ValueType::Any, ValueType::BigFloat) => true,
            (ValueType::Any, ValueType::Function) => true,
            (ValueType::Function, ValueType::Any) => true,
            (ValueType::Any, ValueType::Nothing) => true,
            (ValueType::Struct(type_id), ValueType::Array | ValueType::ArrayOf(_, _)) => self
                .shared_ctx
                .get_struct_name(*type_id)
                .is_some_and(|name| name == "Array" || name.starts_with("Array{")),
            (ValueType::Struct(type_id), ValueType::Set) => self
                .shared_ctx
                .get_struct_name(*type_id)
                .is_some_and(|name| name == "Set" || name.starts_with("Set{")),
            (ValueType::Set, ValueType::Struct(type_id)) => self
                .shared_ctx
                .get_struct_name(*type_id)
                .is_some_and(|name| name == "Set" || name.starts_with("Set{")),
            (actual_ty, ValueType::Union(types)) => value_type_union_accepts(actual_ty, types),
            (ValueType::Union(_), ValueType::Any) => true,
            (ValueType::Pairs, ValueType::Any) => true,
            (ValueType::Union(_), ValueType::Bool) => true,
            (ValueType::Union(_), ValueType::I64) => true,
            (ValueType::Union(_), ValueType::F64) => true,
            (ValueType::Union(_), ValueType::F32) => true,
            (ValueType::Union(_), ValueType::F16) => true,
            _ => false,
        }
    }

    pub(crate) fn compile_expr_as(&mut self, expr: &Expr, target: ValueType) -> CResult<()> {
        let actual = self.compile_expr(expr)?;
        if actual != target {
            // Single source of truth for accept/reject: error here rather than in
            // the per-arm match below, so the emit arms only choose *which*
            // conversion instruction to emit for an already-accepted pair and can
            // never drift from `coercion_accepts` (Issue #7793 regression guard).
            if !self.coercion_accepts(&actual, &target) {
                return err(format!("Cannot convert {:?} to {:?}", actual, target));
            }
            match (actual.clone(), target.clone()) {
                (ValueType::I64, ValueType::F64) => self.emit(Instr::ToF64),
                (ValueType::F64, ValueType::I64) => self.emit(Instr::ToI64),
                // Note: Numeric -> Complex conversions are handled via Pure Julia convert().
                // New numeric types -> I64: use DynamicToI64 which handles all numeric types
                (
                    ValueType::I8
                    | ValueType::I16
                    | ValueType::I32
                    | ValueType::I128
                    | ValueType::U8
                    | ValueType::U16
                    | ValueType::U32
                    | ValueType::U64
                    | ValueType::U128
                    | ValueType::F32
                    | ValueType::F16,
                    ValueType::I64,
                ) => {
                    self.emit(Instr::DynamicToI64);
                }
                // New numeric types -> F64: use DynamicToF64 which handles all numeric types
                (
                    ValueType::I8
                    | ValueType::I16
                    | ValueType::I32
                    | ValueType::I128
                    | ValueType::U8
                    | ValueType::U16
                    | ValueType::U32
                    | ValueType::U64
                    | ValueType::U128
                    | ValueType::F32
                    | ValueType::F16,
                    ValueType::F64,
                ) => {
                    self.emit(Instr::DynamicToF64);
                }
                // Any -> specific type: dynamic conversion at runtime
                // For Any typed values, assume they can be used as the target type
                // The VM will handle runtime type checking
                (ValueType::Any, ValueType::F64) => {
                    // At runtime, Any might be I64 or F64 - ToF64 handles both
                    self.emit(Instr::DynamicToF64);
                }
                (ValueType::Any, ValueType::I64) => {
                    self.emit(Instr::DynamicToI64);
                }
                // Any -> F32: convert dynamically at runtime (for struct field access)
                (ValueType::Any, ValueType::F32) => {
                    self.emit(Instr::DynamicToF32);
                }
                // Any -> F16: convert dynamically at runtime (for struct field access)
                (ValueType::Any, ValueType::F16) => {
                    self.emit(Instr::DynamicToF16);
                }
                // Numeric types -> F32: use DynamicToF32 for narrowing conversions
                // This is needed for user-defined operators that use Float32 types
                (
                    ValueType::I8
                    | ValueType::I16
                    | ValueType::I32
                    | ValueType::I64
                    | ValueType::I128
                    | ValueType::U8
                    | ValueType::U16
                    | ValueType::U32
                    | ValueType::U64
                    | ValueType::U128
                    | ValueType::F64
                    | ValueType::F16,
                    ValueType::F32,
                ) => {
                    self.emit(Instr::DynamicToF32);
                }
                // Numeric types -> F16: use DynamicToF16 for narrowing conversions
                (
                    ValueType::I8
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
                    | ValueType::F64,
                    ValueType::F16,
                ) => {
                    self.emit(Instr::DynamicToF16);
                }
                // Struct -> Any: no conversion needed (Any accepts all types)
                (ValueType::Struct(_), ValueType::Any) => {}
                // Concrete Complex scalar tags still carry immutable struct
                // values at runtime; they box into Any the same way Struct does.
                (ValueType::ComplexF32 | ValueType::ComplexF64, ValueType::Any) => {}
                // Memory -> Any: no conversion needed (Any accepts all types)
                (ValueType::Memory | ValueType::MemoryOf(_), ValueType::Any) => {}
                // Memory family conversions are representation-preserving. A
                // bare `Memory` annotation means the element type is not known
                // statically, while runtime storage still carries it.
                (ValueType::Memory, ValueType::MemoryOf(_))
                | (ValueType::MemoryOf(_), ValueType::Memory) => {}
                (ValueType::MemoryOf(actual_elem), ValueType::MemoryOf(target_elem))
                    if memory_element_coercion_is_lossless(&actual_elem, &target_elem) => {}
                // Struct -> I64: allow conversion (e.g., value extraction for Date structs)
                // DynamicToI64 handles structs by extracting their integer value if possible
                (ValueType::Struct(_), ValueType::I64) => {
                    self.emit(Instr::DynamicToI64);
                }
                // Struct -> F64: allow conversion (e.g., real(Complex) -> F64)
                // DynamicToF64 handles Complex structs by extracting the real part
                (ValueType::Struct(_), ValueType::F64) => {
                    self.emit(Instr::DynamicToF64);
                }
                (ValueType::ComplexF32 | ValueType::ComplexF64, ValueType::F64) => {
                    self.emit(Instr::DynamicToF64);
                }
                (
                    ValueType::Struct(type_id),
                    complex @ (ValueType::ComplexF32 | ValueType::ComplexF64),
                ) if self
                    .shared_ctx
                    .get_struct_name(type_id)
                    .is_some_and(|name| {
                        complex_value_type_matches_struct_name(&complex, &name)
                    }) => {}
                (
                    complex @ (ValueType::ComplexF32 | ValueType::ComplexF64),
                    ValueType::Struct(type_id),
                ) if self
                    .shared_ctx
                    .get_struct_name(type_id)
                    .is_some_and(|name| {
                        complex_value_type_matches_struct_name(&complex, &name)
                    }) => {}
                // Any -> Struct: accept at compile time, runtime will validate
                (ValueType::Any, ValueType::Struct(_)) => {}
                // Any -> Complex: accept at compile time, runtime will validate
                (ValueType::Any, ValueType::ComplexF32 | ValueType::ComplexF64) => {}
                // Any -> Array: accept at compile time, runtime will validate
                (ValueType::Any, ValueType::Array) => {}
                // Any -> Memory: accept at compile time, runtime will validate.
                // A `MemoryRef` value (e.g. `memoryref(mem)`) infers as `Any` but
                // is representation-compatible with a `Memory{T}` / `MemoryRef{T}`
                // struct field (Issue #6626 / #6624 Array.ref). Runtime carries the
                // real Memory/MemoryRef value.
                (ValueType::Any, ValueType::Memory | ValueType::MemoryOf(_)) => {}
                // BigInt -> Any: no conversion needed
                (ValueType::BigInt, ValueType::Any) => {}
                // New numeric types -> Any: no conversion needed
                (
                    ValueType::I8
                    | ValueType::I16
                    | ValueType::I32
                    | ValueType::I128
                    | ValueType::U8
                    | ValueType::U16
                    | ValueType::U32
                    | ValueType::U64
                    | ValueType::U128
                    | ValueType::F32
                    | ValueType::F16,
                    ValueType::Any,
                ) => {}
                // I64 and F64 -> Any: no conversion needed
                (ValueType::I64 | ValueType::F64, ValueType::Any) => {}
                // Bool -> Any: no conversion needed
                (ValueType::Bool, ValueType::Any) => {}
                // Bool -> Bool: no conversion needed (identity)
                (ValueType::Bool, ValueType::Bool) => {}
                // Bool -> I64: Julia treats true as 1, false as 0
                // Many control flow constructs expect I64 conditions
                (ValueType::Bool, ValueType::I64) => {
                    // Bool values on stack are already 0/1, just need type annotation
                    // Actually we need to convert Value::Bool to Value::I64
                    self.emit(Instr::BoolToI64);
                }
                // Bool -> F64: true -> 1.0, false -> 0.0
                // Julia allows Bool to participate in float arithmetic
                (ValueType::Bool, ValueType::F64) => {
                    self.emit(Instr::BoolToI64);
                    self.emit(Instr::ToF64);
                }
                // Bool -> F32: true -> 1.0f0, false -> 0.0f0
                (ValueType::Bool, ValueType::F32) => {
                    self.emit(Instr::BoolToI64);
                    self.emit(Instr::ToF64);
                    self.emit(Instr::DynamicToF32); // Convert F64 to F32
                }
                // I64 -> Bool: treat 0 as false, non-zero as true
                // This is for backwards compatibility with old code paths
                (ValueType::I64, ValueType::Bool) => {
                    self.emit(Instr::I64ToBool);
                }
                // Any -> Bool: runtime check - only allow if value is actually Bool
                // This is needed for && and || operators with variables
                (ValueType::Any, ValueType::Bool) => {
                    // At runtime, the VM will check if the value is Bool
                    // If not, it will raise TypeError
                    self.emit(Instr::DynamicToBool);
                }
                // DataType -> Any: no conversion needed
                (ValueType::DataType, ValueType::Any) => {}
                // Any -> DataType: accept at compile time for generic Type-valued
                // arguments such as cconvert(::Type{T}, x) where T. Runtime call
                // sites that actually consume the value as a type still validate.
                (ValueType::Any, ValueType::DataType) => {}
                // ArrayOf(X) -> Array: no conversion needed (ArrayOf is a subtype of Array)
                (ValueType::ArrayOf(_, _), ValueType::Array) => {}
                // Array -> Any: no conversion needed
                (ValueType::Array, ValueType::Any) => {}
                // ArrayOf(X) -> Any: no conversion needed
                (ValueType::ArrayOf(_, _), ValueType::Any) => {}
                // Str -> Any: no conversion needed (Any accepts all types including String)
                (ValueType::Str, ValueType::Any) => {}
                // Any -> Str: accept at compile time, runtime carries the real value.
                // Arises when a multi-method function whose argument types include a
                // singleton (`::Nothing` / `::Missing`) or abstract parameter cannot be
                // resolved statically, so its return type collapses to `Any`, yet the
                // call site (e.g. `println(f(nothing))`) expects a String. Mirrors the
                // existing `Any -> Struct` / `Any -> Array` runtime-validated arms.
                // No DynamicToStr instruction exists: a String already on the stack is
                // used as-is. (Issue #5069)
                (ValueType::Any, ValueType::Str) => {}
                // Char -> Any: no conversion needed
                (ValueType::Char, ValueType::Any) => {}
                // Char -> I64: convert codepoint to integer (Issue #2035)
                (ValueType::Char, ValueType::I64) => {
                    self.emit(Instr::DynamicToI64);
                }
                // Char -> F64: convert codepoint to float (Issue #2035)
                (ValueType::Char, ValueType::F64) => {
                    self.emit(Instr::DynamicToF64);
                }
                // Tuple -> Any: no conversion needed
                (ValueType::Tuple, ValueType::Any) => {}
                // Any -> Tuple: accept boxed values for Tuple-typed slots. This mirrors
                // Any -> Struct/Array and covers local varargs helper functions whose
                // return value is known only dynamically during @testset lowering
                // (Issue #8482).
                (ValueType::Any, ValueType::Tuple) => {}
                // NamedTuple -> Any: no conversion needed
                (ValueType::NamedTuple, ValueType::Any) => {}
                // Dict -> Any: no conversion needed
                (ValueType::Dict, ValueType::Any) => {}
                // Set <-> Any: Set{T} is a pure-Julia struct over Dict{T,Nothing}
                // (Issue #6721). A `::Set`-typed value widens to Any when
                // boxed/stored, and an Any-typed value flowing into a `::Set`
                // param/return slot is accepted at compile time (the runtime
                // value is already the Set struct). There is no DynamicToSet
                // instruction; the struct value passes through unchanged.
                (ValueType::Set, ValueType::Any) => {}
                (ValueType::Any, ValueType::Set) => {}
                // Range -> Any: no conversion needed
                (ValueType::Range, ValueType::Any) => {}
                // Rng -> Any: no conversion needed
                (ValueType::Rng, ValueType::Any) => {}
                // Nothing -> Any: no conversion needed
                (ValueType::Nothing, ValueType::Any) => {}
                // Missing -> Any: no conversion needed
                (ValueType::Missing, ValueType::Any) => {}
                // Symbol -> Any: no conversion needed
                (ValueType::Symbol, ValueType::Any) => {}
                // Expr -> Any: no conversion needed
                (ValueType::Expr, ValueType::Any) => {}
                // Module -> Any: no conversion needed
                (ValueType::Module, ValueType::Any) => {}
                // Regex -> Any: no conversion needed (Issue #5678). A `::Regex`-typed
                // parameter widens to `Any` when boxed/stored, just like every other
                // concrete value type above.
                (ValueType::Regex, ValueType::Any) => {}
                // RegexMatch -> Any: no conversion needed (Issue #5678)
                (ValueType::RegexMatch, ValueType::Any) => {}
                // I64 -> BigInt: convert via intrinsic (for big() function)
                (ValueType::I64, ValueType::BigInt) => {
                    self.emit(Instr::CallIntrinsic(Intrinsic::I64ToBigInt));
                }
                // F64 -> BigFloat: convert via intrinsic (for big() function)
                (ValueType::F64, ValueType::BigFloat) => {
                    self.emit(Instr::CallIntrinsic(Intrinsic::F64ToBigFloat));
                }
                // BigInt -> I64: convert via intrinsic (may lose precision)
                (ValueType::BigInt, ValueType::I64) => {
                    self.emit(Instr::CallIntrinsic(Intrinsic::BigIntToI64));
                }
                // BigFloat -> F64: convert via DynamicToF64 which handles BigFloat
                (ValueType::BigFloat, ValueType::F64) => {
                    self.emit(Instr::DynamicToF64);
                }
                // BigInt -> F64: convert via DynamicToF64 which handles BigInt
                (ValueType::BigInt, ValueType::F64) => {
                    self.emit(Instr::DynamicToF64);
                }
                // BigFloat -> Any: no conversion needed
                (ValueType::BigFloat, ValueType::Any) => {}
                // Any -> BigInt: accept at compile time, runtime will handle
                (ValueType::Any, ValueType::BigInt) => {}
                // Any -> BigFloat: accept at compile time, runtime will handle
                (ValueType::Any, ValueType::BigFloat) => {}
                // Same types are fine - covers (Any, Any)
                // Any -> Function: accept at compile time for HOFs like map(f, A)
                // Issue #1665: Function variables may infer to Any at compile time
                (ValueType::Any, ValueType::Function) => {}
                // Function -> Any: callable values are ordinary Julia values.
                (ValueType::Function, ValueType::Any) => {}
                // Issue #4580: when an expression is only known as Any but the
                // statically selected context expects Nothing, keep the runtime
                // value on the stack. There is no DynamicToNothing instruction;
                // call/return handling must preserve the value and let the
                // selected runtime path decide whether it is valid.
                (ValueType::Any, ValueType::Nothing) => {}
                // Pure Julia Array wrappers are mutable structs backed by Memory.
                // Let them flow into Array-typed Pure Julia methods without forcing
                // conversion to the legacy native-array container.
                (ValueType::Struct(type_id), ValueType::Array | ValueType::ArrayOf(_, _))
                    if self
                        .shared_ctx
                        .get_struct_name(type_id)
                        .is_some_and(|name| name == "Array" || name.starts_with("Array{")) => {}
                // Set{T} is now a pure-Julia struct over Dict{T,Nothing} (Issue
                // #6721). Let a Set struct flow into a `::Set`/`::Set{T}` param or
                // return slot (typed as the legacy `ValueType::Set`) without
                // forcing conversion to the legacy native `Value::Set` carrier,
                // and accept a legacy native Set where a Set struct is expected.
                // Mirrors the Array-wrapper arm above.
                (ValueType::Struct(type_id), ValueType::Set)
                    if self
                        .shared_ctx
                        .get_struct_name(type_id)
                        .is_some_and(|name| name == "Set" || name.starts_with("Set{")) => {}
                (ValueType::Set, ValueType::Struct(type_id))
                    if self
                        .shared_ctx
                        .get_struct_name(type_id)
                        .is_some_and(|name| name == "Set" || name.starts_with("Set{")) => {}
                // T -> Union{T, ...}: no conversion is needed when the
                // inferred source is already one of the union alternatives.
                // `Any` alternatives are accepted as a conservative fallback
                // for legacy no-table union conversions.
                (actual_ty, ValueType::Union(types))
                    if value_type_union_accepts(&actual_ty, &types) => {}
                // Union -> Any: no conversion needed (Union is a subtype of Any)
                (ValueType::Union(_), ValueType::Any) => {}
                // kwargs... Pairs are ordinary boxed values when stored or returned as Any.
                (ValueType::Pairs, ValueType::Any) => {}
                // Union -> Bool: runtime check needed (e.g., Union{Bool, Nothing})
                // This is used for iterate() return values which are Union{Nothing, Tuple}
                (ValueType::Union(_), ValueType::Bool) => {
                    self.emit(Instr::DynamicToBool);
                }
                // Union -> I64: runtime conversion
                (ValueType::Union(_), ValueType::I64) => {
                    self.emit(Instr::DynamicToI64);
                }
                // Union -> F64: runtime conversion
                (ValueType::Union(_), ValueType::F64) => {
                    self.emit(Instr::DynamicToF64);
                }
                // Union -> F32: runtime conversion (Issue #1771)
                (ValueType::Union(_), ValueType::F32) => {
                    self.emit(Instr::DynamicToF32);
                }
                // Union -> F16: runtime conversion (Issue #1851)
                (ValueType::Union(_), ValueType::F16) => {
                    self.emit(Instr::DynamicToF16);
                }
                // All remaining accepted pairs (identity, and every no-op accept
                // arm such as `T -> Any`, `T -> Union{..}`, struct/array wrappers)
                // need no conversion instruction. Rejection already returned above
                // via `coercion_accepts`, so this arm cannot mask a real error.
                _ => {}
            }
        }
        Ok(())
    }
}

fn value_type_union_accepts(actual: &ValueType, union_types: &[ValueType]) -> bool {
    union_types
        .iter()
        .any(|candidate| matches!(candidate, ValueType::Any) || candidate == actual)
}

fn complex_value_type_matches_struct_name(ty: &ValueType, name: &str) -> bool {
    match ty {
        ValueType::ComplexF32 => matches!(name, "Complex{Float32}" | "ComplexF32"),
        ValueType::ComplexF64 => matches!(name, "Complex{Float64}" | "ComplexF64"),
        _ => false,
    }
}

fn memory_element_coercion_is_lossless(
    actual: &ArrayElementType,
    target: &ArrayElementType,
) -> bool {
    actual == target
        || matches!(
            target,
            ArrayElementType::Any | ArrayElementType::Abstract(_) | ArrayElementType::UnionOf(_)
        )
        || matches!(actual, ArrayElementType::Any)
}
