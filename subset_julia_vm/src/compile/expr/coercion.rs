use crate::intrinsics::Intrinsic;
use crate::ir::core::Expr;
use crate::vm::{Instr, ValueType};

use super::super::types::{err, CResult};
use super::CoreCompiler;

impl CoreCompiler<'_> {
    pub(crate) fn compile_expr_as(&mut self, expr: &Expr, target: ValueType) -> CResult<()> {
        let actual = self.compile_expr(expr)?;
        if actual != target {
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
                // Any -> Struct: accept at compile time, runtime will validate
                (ValueType::Any, ValueType::Struct(_)) => {}
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
                // ArrayOf(X) -> Array: no conversion needed (ArrayOf is a subtype of Array)
                (ValueType::ArrayOf(_), ValueType::Array) => {}
                // Array -> Any: no conversion needed
                (ValueType::Array, ValueType::Any) => {}
                // ArrayOf(X) -> Any: no conversion needed
                (ValueType::ArrayOf(_), ValueType::Any) => {}
                // Str -> Any: no conversion needed (Any accepts all types including String)
                (ValueType::Str, ValueType::Any) => {}
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
                // NamedTuple -> Any: no conversion needed
                (ValueType::NamedTuple, ValueType::Any) => {}
                // Dict -> Any: no conversion needed
                (ValueType::Dict, ValueType::Any) => {}
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
                // Union -> Any: no conversion needed (Union is a subtype of Any)
                (ValueType::Union(_), ValueType::Any) => {}
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
                _ if actual == target => {}
                _ => {
                    return err(format!("Cannot convert {:?} to {:?}", actual, target));
                }
            }
        }
        Ok(())
    }
}
