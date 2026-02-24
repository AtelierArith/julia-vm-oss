use super::*;

impl<'a> IrConverter<'a> {
    pub(crate) fn literal_numeric_to_f64(lit: &Literal) -> Option<f64> {
        match lit {
            Literal::Float(v) => Some(*v),
            Literal::Int(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub(crate) fn is_im_unit_literal(expr: &Expr) -> bool {
        match expr {
            Expr::Literal(Literal::Struct(name, fields), _) => {
                name.starts_with("Complex{")
                    && fields.len() == 2
                    && matches!(fields[0], Literal::Bool(false))
                    && matches!(fields[1], Literal::Bool(true))
            }
            _ => false,
        }
    }

    /// Fold parser-lowered numeric complex literals:
    /// `a + b*im` -> `Complex(a, b)`.
    pub(crate) fn builtin_op_to_aot(op: &crate::ir::core::BuiltinOp) -> Option<AotBuiltinOp> {
        use crate::ir::core::BuiltinOp;
        match op {
            // Math/random
            BuiltinOp::Sqrt => Some(AotBuiltinOp::Sqrt),
            BuiltinOp::Rand => Some(AotBuiltinOp::Rand),
            BuiltinOp::Randn => Some(AotBuiltinOp::Randn),

            // Array operations (available in BuiltinOp)
            BuiltinOp::Length => Some(AotBuiltinOp::Length),
            BuiltinOp::Size => Some(AotBuiltinOp::Size),
            BuiltinOp::Ndims => Some(AotBuiltinOp::Ndims),
            BuiltinOp::Push => Some(AotBuiltinOp::Push),
            BuiltinOp::Pop => Some(AotBuiltinOp::Pop),
            BuiltinOp::PushFirst => Some(AotBuiltinOp::PushFirst),
            BuiltinOp::PopFirst => Some(AotBuiltinOp::PopFirst),
            BuiltinOp::Insert => Some(AotBuiltinOp::Insert),
            BuiltinOp::DeleteAt => Some(AotBuiltinOp::DeleteAt),
            BuiltinOp::Zeros => Some(AotBuiltinOp::Zeros),
            BuiltinOp::Ones => Some(AotBuiltinOp::Ones),
            // Note: BuiltinOp::Fill removed — fill is now Pure Julia (Issue #2640)
            BuiltinOp::Reshape => Some(AotBuiltinOp::Reshape),
            // Note: BuiltinOp::Sum removed — sum is now Pure Julia
            BuiltinOp::Collect => Some(AotBuiltinOp::Collect),

            // Dedicated tuple element access operations
            BuiltinOp::TupleFirst => Some(AotBuiltinOp::TupleFirst),
            BuiltinOp::TupleLast => Some(AotBuiltinOp::TupleLast),
            // Note: TupleLength removed — dead code (Issue #2643)

            // Type operations
            BuiltinOp::TypeOf => Some(AotBuiltinOp::TypeOf),
            BuiltinOp::Isa => Some(AotBuiltinOp::Isa),

            // Unknown or unsupported builtins
            _ => None,
        }
    }

    /// Convert a literal to AoT expression
    pub(crate) fn convert_literal(&self, lit: &Literal) -> AotResult<AotExpr> {
        match lit {
            Literal::Int(v) => Ok(AotExpr::LitI64(*v)),
            Literal::Int128(v) => {
                let narrowed = i64::try_from(*v).map_err(|_| {
                    crate::aot::AotError::ConversionError(format!(
                        "Int128 literal out of Int64 range in AoT conversion: {}",
                        v
                    ))
                })?;
                Ok(AotExpr::LitI64(narrowed))
            }
            Literal::Float(v) => Ok(AotExpr::LitF64(*v)),
            Literal::Float32(v) => Ok(AotExpr::LitF32(*v)),
            Literal::Float16(v) => Ok(AotExpr::LitF32(v.to_f32())), // AoT has no LitF16; widen to F32
            Literal::Bool(v) => Ok(AotExpr::LitBool(*v)),
            Literal::Str(v) => Ok(AotExpr::LitStr(v.clone())),
            Literal::Char(v) => Ok(AotExpr::LitChar(*v)),
            Literal::Nothing => Ok(AotExpr::LitNothing),
            Literal::Struct(name, fields) => {
                // Normalize Julia literal `Complex{Bool}(false, true)` (e.g. `im`) to `Complex`.
                let normalized_name = if name.starts_with("Complex{") {
                    "Complex".to_string()
                } else {
                    name.clone()
                };

                let mut converted_fields = Vec::with_capacity(fields.len());
                for field in fields {
                    // For normalized Complex literals, coerce Bool fields to Float64.
                    if normalized_name == "Complex" {
                        if let Literal::Bool(b) = field {
                            converted_fields.push(AotExpr::LitF64(if *b { 1.0 } else { 0.0 }));
                            continue;
                        }
                    }
                    converted_fields.push(self.convert_literal(field)?);
                }

                Ok(AotExpr::StructNew {
                    name: normalized_name,
                    fields: converted_fields,
                })
            }
            // Workaround: `missing` literal has no AotExpr::LitMissing variant (Issue #3343).
            // Fails explicitly until the full 12-file literal pipeline is extended.
            Literal::Missing => Err(crate::aot::AotError::ConversionError(
                "AoT does not support `missing` literals (no AotExpr::LitMissing)".to_string(),
            )),
            _ => Err(crate::aot::AotError::ConversionError(format!(
                "unsupported literal kind in AoT conversion: {lit:?}"
            ))),
        }
    }

    /// Convert a type name string to StaticType
    /// Used to resolve convert(Type, value) calls to AotExpr::Convert
    pub(crate) fn type_name_to_static(&self, name: &str) -> Option<StaticType> {
        match name {
            "Int64" | "Int" => Some(StaticType::I64),
            "Int32" => Some(StaticType::I32),
            "Float64" => Some(StaticType::F64),
            "Float32" => Some(StaticType::F32),
            "Bool" => Some(StaticType::Bool),
            "String" => Some(StaticType::Str),
            "Char" => Some(StaticType::Char),
            "Nothing" => Some(StaticType::Nothing),
            "Any" => Some(StaticType::Any),
            _ => None,
        }
    }

    /// Map an operator function name to AotBinOp
    /// Used to unfold multi-argument operator calls like *(a, b, c) to nested binops
    pub(crate) fn map_operator_to_binop(&self, name: &str) -> Option<AotBinOp> {
        match name {
            "+" => Some(AotBinOp::Add),
            "-" => Some(AotBinOp::Sub),
            "*" => Some(AotBinOp::Mul),
            "/" => Some(AotBinOp::Div),
            "÷" | "div" => Some(AotBinOp::IntDiv),
            "%" | "mod" => Some(AotBinOp::Mod),
            "^" => Some(AotBinOp::Pow),
            "&" => Some(AotBinOp::BitAnd),
            "|" => Some(AotBinOp::BitOr),
            "⊻" | "xor" => Some(AotBinOp::BitXor),
            "<<" => Some(AotBinOp::Shl),
            ">>" => Some(AotBinOp::Shr),
            _ => None,
        }
    }

    /// Convert JuliaType to StaticType
    pub(crate) fn julia_type_to_static(&self, jt: &crate::types::JuliaType) -> StaticType {
        use crate::types::JuliaType as JT;
        match jt {
            JT::Int64 => StaticType::I64,
            JT::Int32 => StaticType::I32,
            JT::Float64 => StaticType::F64,
            JT::Float32 => StaticType::F32,
            JT::Bool => StaticType::Bool,
            JT::String => StaticType::Str,
            JT::Char => StaticType::Char,
            JT::Nothing => StaticType::Nothing,
            JT::Any => StaticType::Any,
            _ => StaticType::Any,
        }
    }

    /// Check if a function name corresponds to an operation handled directly by the AoT compiler
    /// These are operators and conversion functions that don't need user-defined implementations
    pub(crate) fn is_aot_builtin_function(name: &str) -> bool {
        matches!(
            name,
            // Arithmetic operators
            "+" | "-" | "*" | "/" | "÷" | "%" | "^" | "\\" |
            // Comparison operators
            "==" | "!=" | "<" | "<=" | ">" | ">=" | "===" | "!==" |
            // Logical operators
            "!" | "&&" | "||" |
            // Bitwise operators
            "&" | "|" | "⊻" | "xor" | "~" | "<<" | ">>" | ">>>" |
            // Type conversion
            "convert" | "promote" | "promote_type" |
            // Built-in math functions
            "abs" | "sqrt" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" |
            "exp" | "log" | "floor" | "ceil" | "round" | "trunc" |
            "min" | "max" | "clamp" | "sign" | "copysign" |
            // Type constructors (handled as casts)
            "Int64" | "Int32" | "Int16" | "Int8" |
            "UInt64" | "UInt32" | "UInt16" | "UInt8" |
            "Float64" | "Float32" | "Bool" |
            // Array operations
            "length" | "size" | "ndims" | "push!" | "pop!" |
            "pushfirst!" | "popfirst!" | "insert!" | "deleteat!" |
            "zeros" | "ones" | "fill" | "reshape" | "sum" | "collect" |
            // Other built-ins
            "println" | "print" | "string" | "repr" | "show" |
            // Error/throw (intercepted in IR converter) (Issue #3410)
            "error" | "throw" |
            // Range dispatch functions — nothing-dispatch patterns don't translate to Rust (Issue #3413)
            "range" | "_range" | "range_start_stop" | "range_start_stop_length" |
            "range_start_step_length" | "range_start_length" | "range_start_step_stop" |
            // Broadcast internals (Issue #3415)
            "materialize" | "copy" | "instantiate" |
            // Transpose/adjoint (Issue #3415)
            "adjoint" |
            // Complex intrinsics
            "abs2" | "real" | "imag" |
            // Constructors handled in prelude
            "Complex" | "Broadcasted" | "LinRange" | "StepRangeLen" | "OneTo"
        )
    }
}
