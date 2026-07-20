//! Type constructor compilation.
//!
//! Handles compilation of Julia type constructors:
//! - _string_from_chars(chars): Construct string from Vector{Char}/Vector{UInt8}
//! - _int_to_char(n): Codepoint to character
//! - _char_to_int(c): Character to codepoint
//! - Int(x): Convert to native signed word integer
//! - UInt(x): Convert to native unsigned word integer
//! - BigInt(x): Arbitrary precision integer
//! - BigFloat(x): Arbitrary precision float
//! - Int64: Signed integer type
//! - Float64: Floating point type
//! - _to_int8/_to_uint8/etc.: internal conversion boundaries for pure Julia wrappers

use crate::builtins::BuiltinId;
use crate::bytecode::{Instr, ValueType};
use crate::ir::core::Expr;

use super::super::{err, CResult, CoreCompiler};

fn fixed_width_conversion_boundary(name: &str) -> Option<(BuiltinId, ValueType, &'static str)> {
    Some(match name {
        "_to_int8" => (
            BuiltinId::Int8,
            ValueType::I8,
            "_to_int8 requires exactly 1 argument",
        ),
        "_to_int16" => (
            BuiltinId::Int16,
            ValueType::I16,
            "_to_int16 requires exactly 1 argument",
        ),
        "_to_int32" => (
            BuiltinId::Int32,
            ValueType::I32,
            "_to_int32 requires exactly 1 argument",
        ),
        "_to_int128" => (
            BuiltinId::Int128,
            ValueType::I128,
            "_to_int128 requires exactly 1 argument",
        ),
        "_to_uint8" => (
            BuiltinId::UInt8,
            ValueType::U8,
            "_to_uint8 requires exactly 1 argument",
        ),
        "_to_uint16" => (
            BuiltinId::UInt16,
            ValueType::U16,
            "_to_uint16 requires exactly 1 argument",
        ),
        "_to_uint32" => (
            BuiltinId::UInt32,
            ValueType::U32,
            "_to_uint32 requires exactly 1 argument",
        ),
        "_to_uint64" => (
            BuiltinId::UInt64,
            ValueType::U64,
            "_to_uint64 requires exactly 1 argument",
        ),
        "_to_uint128" => (
            BuiltinId::UInt128,
            ValueType::U128,
            "_to_uint128 requires exactly 1 argument",
        ),
        "_to_float16" => (
            BuiltinId::Float16,
            ValueType::F16,
            "_to_float16 requires exactly 1 argument",
        ),
        "_to_float32" => (
            BuiltinId::Float32,
            ValueType::F32,
            "_to_float32 requires exactly 1 argument",
        ),
        _ => return None,
    })
}

impl CoreCompiler<'_> {
    /// Compile type constructor calls.
    /// Returns Some(type) if handled, None if not a type constructor.
    pub(in super::super) fn compile_builtin_types(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        if let Some((builtin, return_type, arity_error)) = fixed_width_conversion_boundary(name) {
            if args.len() != 1 {
                return err(arity_error);
            }
            self.compile_expr(&args[0])?;
            self.emit(Instr::CallBuiltin(builtin, 1));
            return Ok(Some(return_type));
        }

        if matches!(
            name,
            "Int" | "UInt" | "BigInt" | "BigFloat" | "Int64" | "Float64"
        ) && args.len() != 1
        {
            return err(format!("{} requires exactly 1 argument", name));
        }

        match name {
            "_string_from_chars" => {
                // _string_from_chars(chars) - construct string from Vector{Char}
                // / Vector{UInt8}; public String(...) is a Julia wrapper.
                if args.len() != 1 {
                    return err("_string_from_chars requires exactly one argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::StringFromChars, 1));
                Ok(Some(ValueType::Str))
            }
            "_int_to_char" => {
                // _int_to_char(n) - codepoint to char; public Char(...) is a
                // Julia wrapper so it can be used as an ordinary function value.
                if args.len() != 1 {
                    return err("_int_to_char requires exactly 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::IntToChar, 1));
                Ok(Some(ValueType::Char))
            }
            "_char_to_int" => {
                // _char_to_int(c) - char to codepoint; public Int(::Char) is a
                // Julia method layered over this storage boundary.
                if args.len() != 1 {
                    return err("_char_to_int requires exactly 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::CharToInt, 1));
                Ok(Some(ValueType::I64))
            }
            "Int" => {
                // Int(x) - convert to the platform-native signed word type.
                self.compile_expr(&args[0])?;
                match crate::types::native_int_type_name() {
                    "Int32" => {
                        self.emit(Instr::CallBuiltin(BuiltinId::Int32, 1));
                        Ok(Some(ValueType::I32))
                    }
                    _ => {
                        self.emit(Instr::CallBuiltin(BuiltinId::Int64, 1));
                        Ok(Some(ValueType::I64))
                    }
                }
            }
            "UInt" => {
                // UInt(x) - convert to the platform-native unsigned word type.
                self.compile_expr(&args[0])?;
                match crate::types::native_uint_type_name() {
                    "UInt32" => {
                        self.emit(Instr::CallBuiltin(BuiltinId::UInt32, 1));
                        Ok(Some(ValueType::U32))
                    }
                    _ => {
                        self.emit(Instr::CallBuiltin(BuiltinId::UInt64, 1));
                        Ok(Some(ValueType::U64))
                    }
                }
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
                        "Int" | "Int8" | "Int16" | "Int32" | "Int64" | "Int128" | "UInt"
                        | "UInt8" | "UInt16" | "UInt32" | "UInt64" | "UInt128" | "BigInt" => {
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
            "Int64" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Int64, 1));
                Ok(Some(ValueType::I64))
            }
            // Floating point constructors
            "Float64" => {
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Float64, 1));
                Ok(Some(ValueType::F64))
            }
            "names" => {
                // names(m::Module) -> Vector{Symbol}. This default form is the
                // upstream path used by AbstractAlgebra's @alias macro (Issue
                // #7938).
                if args.len() != 1 {
                    return err("names currently supports exactly one Module argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Names, 1));
                Ok(Some(ValueType::Array))
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
            "_isdefined_module_binding" => {
                // _isdefined_module_binding(m::Module, s::Symbol) -> Bool
                // Internal reflection primitive backing function-form
                // isdefined(::Module, ::Symbol) (Issue #5002/#4958).
                if args.len() != 2 {
                    return err(
                        "_isdefined_module_binding requires exactly 2 arguments: (module, symbol)",
                    );
                }
                self.compile_expr(&args[0])?; // module
                self.compile_expr(&args[1])?; // symbol
                self.emit(Instr::CallBuiltin(BuiltinId::IsdefinedModuleBinding, 2));
                Ok(Some(ValueType::Bool))
            }
            "_module_name" => {
                // _module_name(m::Module) -> Symbol
                // Internal reflection primitive backing Pure Julia
                // nameof(::Module) (Issue #11171). Returns the module's own
                // unqualified binding name (e.g. `S` for `module P; module S
                // end; end`, not the qualified `P.S` path).
                if args.len() != 1 {
                    return err("_module_name requires exactly 1 argument: _module_name(Module)");
                }
                self.compile_expr(&args[0])?; // module
                self.emit(Instr::CallBuiltin(BuiltinId::_ModuleName, 1));
                Ok(Some(ValueType::Symbol))
            }
            "_isdefined_binding_field" => {
                // _isdefined_binding_field(b::Core.Binding, s::Symbol) -> Bool
                // Internal reflection primitive backing function-form
                // isdefined(::Core.Binding, ::Symbol) (Issue #10067):
                // `:globalref`/`:flags` are set, `:value`/`:partitions`/
                // `:backedges` exist upstream but are unset in sjulia.
                if args.len() != 2 {
                    return err(
                        "_isdefined_binding_field requires exactly 2 arguments: (binding, symbol)",
                    );
                }
                self.compile_expr(&args[0])?; // binding
                self.compile_expr(&args[1])?; // symbol
                self.emit(Instr::CallBuiltin(BuiltinId::IsdefinedBindingField, 2));
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
            "_bigfloat_nextfloat" => {
                // _bigfloat_nextfloat(x::BigFloat, up::Bool) -> BigFloat
                // One-ULP step at the value's own precision (nextfloat when up,
                // prevfloat when !up), mirroring MPFR mpfr_nextabove/nextbelow
                // (Issue #9280). Backs the BigFloat methods of nextfloat/prevfloat.
                if args.len() != 2 {
                    return err(
                        "_bigfloat_nextfloat requires exactly 2 arguments: (x::BigFloat, up::Bool)",
                    );
                }
                self.compile_expr(&args[0])?; // x
                self.compile_expr(&args[1])?; // up
                self.emit(Instr::CallBuiltin(BuiltinId::BigFloatNextfloat, 2));
                Ok(Some(ValueType::BigFloat))
            }
            "_bigfloat_get_exp" => {
                // _bigfloat_get_exp(x::BigFloat) -> Int64
                // Base-2 exponent E (x = m·2^E, m ∈ [0.5, 1)) read from the
                // astro_float exponent field. Backs exponent/frexp/significand of
                // BigFloat (Issue #9286).
                if args.len() != 1 {
                    return err("_bigfloat_get_exp requires exactly 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::BigFloatGetExp, 1));
                Ok(Some(ValueType::I64))
            }
            "_bigfloat_scale2" => {
                // _bigfloat_scale2(x::BigFloat, n::Int64) -> BigFloat
                // x · 2^n by exact exponent shift; used to normalize the mantissa
                // for BigFloat frexp/significand (Issue #9286).
                if args.len() != 2 {
                    return err(
                        "_bigfloat_scale2 requires exactly 2 arguments: (x::BigFloat, n::Int64)",
                    );
                }
                self.compile_expr(&args[0])?; // x
                self.compile_expr(&args[1])?; // n
                self.emit(Instr::CallBuiltin(BuiltinId::BigFloatScale2, 2));
                Ok(Some(ValueType::BigFloat))
            }
            "_bigfloat_signbit" => {
                // _bigfloat_signbit(x::BigFloat) -> Bool
                // The sign bit read from the astro_float sign field, so a
                // negative BigFloat zero is observable — the generic
                // `signbit(x) = x < 0` cannot see it (Issue #9450). Backs
                // signbit(::BigFloat) in base/gmp.jl.
                if args.len() != 1 {
                    return err("_bigfloat_signbit requires exactly 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::BigFloatSignbit, 1));
                Ok(Some(ValueType::Bool))
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
            // Missing value utility boundary (Issue #8779)
            "_nonmissingtype" => {
                // _nonmissingtype(T::Type) -> Type
                // Returns T with Missing removed from Union.
                if args.len() != 1 {
                    return err("_nonmissingtype requires exactly 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::NonMissingType, 1));
                Ok(Some(ValueType::DataType))
            }
            _ => Ok(None),
        }
    }
}
