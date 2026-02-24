//! Type constructor compilation.
//!
//! Handles compilation of Julia type constructors:
//! - String(chars): Construct string from Vector{Char}
//! - Char(n): Codepoint to character
//! - Int(x): Convert to Int64
//! - BigInt(x): Arbitrary precision integer
//! - BigFloat(x): Arbitrary precision float
//! - Int8, Int16, Int32, Int64, Int128: Signed integer types
//! - UInt8, UInt16, UInt32, UInt64, UInt128: Unsigned integer types
//! - Float32, Float64: Floating point types

use crate::builtins::BuiltinId;
use crate::ir::core::Expr;
use crate::vm::{Instr, ValueType};

use super::super::{err, CResult, CoreCompiler};

impl CoreCompiler<'_> {
    /// Compile type constructor calls.
    /// Returns Some(type) if handled, None if not a type constructor.
    pub(in super::super) fn compile_builtin_types(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        match name {
            "String" => {
                // String(chars) - construct string from Vector{Char} (Issue #2038)
                if args.len() != 1 {
                    return err("String() requires exactly one argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::StringFromChars, 1));
                Ok(Some(ValueType::Str))
            }
            "Char" => {
                // Char(n) - codepoint to char
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::IntToChar, 1));
                Ok(Some(ValueType::Char))
            }
            "Int" => {
                // Int(x) - convert to Int64 (works for Char, Int*, Float*, BigInt, etc.)
                // Note: Int64 builtin's convert_to_i64() handles Char -> codepoint conversion
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Int64, 1));
                Ok(Some(ValueType::I64))
            }
            "BigInt" => {
                // BigInt(x) - convert to arbitrary precision integer
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::BigInt, 1));
                Ok(Some(ValueType::BigInt))
            }
            "BigFloat" => {
                // BigFloat(x) - convert to arbitrary precision float
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::BigFloat, 1));
                Ok(Some(ValueType::BigFloat))
            }
            "big" => {
                // big(x) - convert to BigInt or BigFloat depending on argument type
                // big(::Type{T}) - type to type conversion
                if args.is_empty() {
                    return err("big() requires an argument");
                }

                // First check if argument is a type name (like Int64, Float64)
                if let Expr::Var(type_name, _) = &args[0] {
                    match type_name.as_str() {
                        // Float types -> BigFloat type
                        "Float16" | "Float32" | "Float64" | "BigFloat" => {
                            self.emit(Instr::PushDataType("BigFloat".to_string()));
                            return Ok(Some(ValueType::DataType));
                        }
                        // Integer types -> BigInt type
                        "Int8" | "Int16" | "Int32" | "Int64" | "Int128" | "UInt8" | "UInt16"
                        | "UInt32" | "UInt64" | "UInt128" | "BigInt" => {
                            self.emit(Instr::PushDataType("BigInt".to_string()));
                            return Ok(Some(ValueType::DataType));
                        }
                        _ => {} // Fall through to value conversion
                    }
                }

                // Value conversion: big(48) -> BigInt(48), big(1.5) -> BigFloat(1.5)
                let arg_type = self.infer_expr_type(&args[0]);
                match arg_type {
                    ValueType::F32 | ValueType::F64 | ValueType::BigFloat => {
                        self.compile_expr(&args[0])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::BigFloat, 1));
                        Ok(Some(ValueType::BigFloat))
                    }
                    _ => {
                        self.compile_expr(&args[0])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::BigInt, 1));
                        Ok(Some(ValueType::BigInt))
                    }
                }
            }
            // Signed integer constructors
            "Int8" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Int8, 1));
                Ok(Some(ValueType::I8))
            }
            "Int16" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Int16, 1));
                Ok(Some(ValueType::I16))
            }
            "Int32" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Int32, 1));
                Ok(Some(ValueType::I32))
            }
            "Int64" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Int64, 1));
                Ok(Some(ValueType::I64))
            }
            "Int128" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Int128, 1));
                Ok(Some(ValueType::I128))
            }
            // Unsigned integer constructors
            "UInt8" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::UInt8, 1));
                Ok(Some(ValueType::U8))
            }
            "UInt16" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::UInt16, 1));
                Ok(Some(ValueType::U16))
            }
            "UInt32" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::UInt32, 1));
                Ok(Some(ValueType::U32))
            }
            "UInt64" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::UInt64, 1));
                Ok(Some(ValueType::U64))
            }
            "UInt128" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::UInt128, 1));
                Ok(Some(ValueType::U128))
            }
            // Floating point constructors
            "Float16" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Float16, 1));
                Ok(Some(ValueType::F16))
            }
            "Float32" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Float32, 1));
                Ok(Some(ValueType::F32))
            }
            "Float64" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Float64, 1));
                Ok(Some(ValueType::F64))
            }
            // Module introspection (Julia 1.11+)
            "isexported" => {
                // isexported(m::Module, s::Symbol) -> Bool
                if args.len() != 2 {
                    return err(
                        "isexported requires exactly 2 arguments: isexported(module, symbol)",
                    );
                }
                self.compile_expr(&args[0])?; // module
                self.compile_expr(&args[1])?; // symbol
                self.emit(Instr::CallBuiltin(BuiltinId::IsExported, 2));
                Ok(Some(ValueType::Bool))
            }
            "ispublic" => {
                // ispublic(m::Module, s::Symbol) -> Bool
                if args.len() != 2 {
                    return err("ispublic requires exactly 2 arguments: ispublic(module, symbol)");
                }
                self.compile_expr(&args[0])?; // module
                self.compile_expr(&args[1])?; // symbol
                self.emit(Instr::CallBuiltin(BuiltinId::IsPublic, 2));
                Ok(Some(ValueType::Bool))
            }
            // BigFloat precision control (Issue #345)
            "_bigfloat_precision" => {
                // _bigfloat_precision(x::BigFloat) -> Int64
                if args.len() != 1 {
                    return err("_bigfloat_precision requires exactly 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::BigFloatPrecision, 1));
                Ok(Some(ValueType::I64))
            }
            "_bigfloat_default_precision" => {
                // _bigfloat_default_precision() -> Int64
                if !args.is_empty() {
                    return err("_bigfloat_default_precision takes no arguments");
                }
                self.emit(Instr::CallBuiltin(BuiltinId::BigFloatDefaultPrecision, 0));
                Ok(Some(ValueType::I64))
            }
            "_set_bigfloat_default_precision!" => {
                // _set_bigfloat_default_precision!(n::Int64) -> Int64
                if args.len() != 1 {
                    return err("_set_bigfloat_default_precision! requires exactly 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(
                    BuiltinId::SetBigFloatDefaultPrecision,
                    1,
                ));
                Ok(Some(ValueType::I64))
            }
            "_bigfloat_rounding" => {
                // _bigfloat_rounding() -> Int64 (rounding mode as integer)
                if !args.is_empty() {
                    return err("_bigfloat_rounding takes no arguments");
                }
                self.emit(Instr::CallBuiltin(BuiltinId::BigFloatRounding, 0));
                Ok(Some(ValueType::I64))
            }
            "_set_bigfloat_rounding!" => {
                // _set_bigfloat_rounding!(mode::Int64) -> Int64
                if args.len() != 1 {
                    return err("_set_bigfloat_rounding! requires exactly 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::SetBigFloatRounding, 1));
                Ok(Some(ValueType::I64))
            }
            // Subnormal float control (Issue #441)
            "get_zero_subnormals" => {
                // get_zero_subnormals() -> Bool
                if !args.is_empty() {
                    return err("get_zero_subnormals takes no arguments");
                }
                self.emit(Instr::CallBuiltin(BuiltinId::GetZeroSubnormals, 0));
                Ok(Some(ValueType::Bool))
            }
            "set_zero_subnormals" => {
                // set_zero_subnormals(yes::Bool) -> Bool
                if args.len() != 1 {
                    return err("set_zero_subnormals requires exactly 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::SetZeroSubnormals, 1));
                Ok(Some(ValueType::Bool))
            }
            // Missing value utilities (Issue #1316)
            "nonmissingtype" => {
                // nonmissingtype(T::Type) -> Type
                // Returns T with Missing removed from Union
                if args.len() != 1 {
                    return err("nonmissingtype requires exactly 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::NonMissingType, 1));
                Ok(Some(ValueType::DataType))
            }
            _ => Ok(None),
        }
    }
}
