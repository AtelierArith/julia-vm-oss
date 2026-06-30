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
            BuiltinOp::In => Some(AotBuiltinOp::In),
            BuiltinOp::HasKey => Some(AotBuiltinOp::HasKey),
            BuiltinOp::DictGet => Some(AotBuiltinOp::DictGet),
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
            BuiltinOp::TimeNs => Some(AotBuiltinOp::TimeNs),

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
            Literal::Missing => Ok(AotExpr::LitMissing),
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
            _ => Err(crate::aot::AotError::ConversionError(format!(
                "unsupported literal kind in AoT conversion: {lit:?}"
            ))),
        }
    }

    /// Convert a type name string to StaticType
    /// Used to resolve convert(Type, value) calls to AotExpr::Convert
    pub(crate) fn type_name_to_static(&self, name: &str) -> Option<StaticType> {
        StaticType::from_julia_name_lossy(name)
    }

    /// Extract the interned name of a bare Symbol-literal quote constructor,
    /// i.e. the `"foo"` of `:foo`'s lowered `Builtin { SymbolNew, ["foo"] }`.
    /// Returns `None` for quoted expressions that build runtime Expr objects
    /// (Issue #7051).
    pub(crate) fn quote_symbol_name(constructor: &Expr) -> Option<String> {
        match constructor {
            Expr::Builtin { name, args, .. }
                if matches!(name, crate::ir::core::BuiltinOp::SymbolNew) && args.len() == 1 =>
            {
                match &args[0] {
                    Expr::Literal(Literal::Str(s), _) => Some(s.clone()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Whether `name` is a recognized builtin type name (primitive, abstract,
    /// `Any`, or `Union{}`). User-defined types are tracked separately via the
    /// struct / abstract-type maps. Used to recognize type-name operands of the
    /// subtype operator `<:` (Issue #7037).
    fn is_builtin_type_name(name: &str) -> bool {
        use crate::inference_core::CoreType;
        matches!(
            CoreType::from_julia_name(name),
            CoreType::Primitive(_) | CoreType::Abstract(_) | CoreType::Any | CoreType::Bottom
        )
    }

    /// Whether `name` denotes a statically known type: a builtin type, a
    /// user-declared `struct`, or a user-declared `abstract type` (Issue #7037).
    pub(crate) fn is_known_type_name(&self, name: &str) -> bool {
        Self::is_builtin_type_name(name)
            || self.engine.structs.contains_key(name)
            || self.abstract_types.iter().any(|(n, _, _)| n == name)
    }

    /// Build the nominal type hierarchy for the program from user struct and
    /// abstract-type declarations so the Core subtype solver can resolve
    /// user-defined `<:` relations (Issue #7037).
    pub(crate) fn build_struct_hierarchy(&self) -> crate::types::StructHierarchy {
        use std::collections::HashMap;
        let mut map: HashMap<String, (Option<String>, Vec<String>)> = HashMap::new();
        for (name, info) in &self.engine.structs {
            map.insert(
                name.clone(),
                (info.parent.clone(), info.type_params.clone()),
            );
        }
        for (name, parent, type_params) in &self.abstract_types {
            map.insert(name.clone(), (parent.clone(), type_params.clone()));
        }
        crate::types::StructHierarchy::from_parent_map(&map)
    }

    /// Extract the static type name an expression denotes, when it is a bare
    /// reference to a statically known type (e.g. `Int`, `Real`, a user struct
    /// or abstract type). Returns `None` for runtime type values, parametric
    /// forms, or non-type expressions (Issue #7037).
    pub(crate) fn expr_as_static_type_name(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Var(name, _) if self.is_known_type_name(name) => Some(name.clone()),
            _ => None,
        }
    }

    /// Try to const-fold a static subtype relation `left <: right`. Returns the
    /// boolean result when both operands are statically known type names;
    /// `None` when the relation involves runtime type values and must fall back
    /// to the dynamic gate (Issue #7037).
    pub(crate) fn try_fold_static_subtype(&self, left: &Expr, right: &Expr) -> Option<bool> {
        let left_name = self.expr_as_static_type_name(left)?;
        let right_name = self.expr_as_static_type_name(right)?;
        let hierarchy = self.build_struct_hierarchy();
        let engine = crate::inference_core::CoreSubtypeEngine::with_hierarchy(&hierarchy);
        Some(engine.is_subtype_by_name(&left_name, &right_name))
    }

    pub(crate) fn array_constructor_element_and_dims<'b>(
        &self,
        function: &str,
        args: &'b [&'b Expr],
    ) -> Option<(StaticType, Vec<&'b Expr>)> {
        if !matches!(function, "zeros" | "ones") {
            return None;
        }

        let mut element_ty = StaticType::F64;
        let mut dim_args = args;
        if let Some(Expr::Var(type_name, _)) = args.first().copied() {
            if let Some(ty) = self.type_name_to_static(type_name) {
                element_ty = ty;
                dim_args = &args[1..];
            }
        }

        if dim_args.len() == 1 {
            if let Expr::TupleLiteral { elements, .. } = dim_args[0] {
                return Some((element_ty, elements.iter().collect()));
            }
        }
        Some((element_ty, dim_args.to_vec()))
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
            "%" => Some(AotBinOp::Mod),
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
        StaticType::from_vm_julia_type_lossy(jt).unwrap_or(StaticType::Any)
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
            "zeros" | "ones" | "fill" | "reshape" | "sum" | "collect" | "in" | "∈" |
            // Other built-ins
            "println" | "print" | "time_ns" | "string" | "repr" | "show" |
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
