//! Transfer functions for field access operations.
//!
//! This module implements type inference for Julia's field access operations,
//! including getfield, setfield!, fieldnames, and fieldtypes.

use crate::compile::lattice::types::{ConcreteType, ConstValue, LatticeType};
use crate::compile::tfuncs::registry::TFuncContext;

/// Transfer function for `getfield` (field access).
///
/// Type rules:
/// - getfield(Struct, Symbol) → Field type
/// - getfield(NamedTuple, Symbol) → Field type
/// - getfield(Module, Symbol) → Top (any exported value)
///
/// # Examples
/// ```text
/// getfield(Point{x::Int64, y::Float64}, :x) → Int64
/// getfield(Point{x::Int64, y::Float64}, :y) → Float64
/// ```
pub fn tfunc_getfield(args: &[LatticeType]) -> LatticeType {
    if args.len() < 2 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::NamedTuple { fields }) => {
            // For NamedTuple, we can look up the field type if the field name is known
            if args.len() >= 2 {
                // Extract field name from either String or Symbol constant
                let field_name = match &args[1] {
                    LatticeType::Const(ConstValue::String(s)) => Some(s.as_str()),
                    LatticeType::Const(ConstValue::Symbol(s)) => Some(s.as_str()),
                    _ => None,
                };

                if let Some(field_name) = field_name {
                    // Find the field in the NamedTuple
                    for (name, ty) in fields {
                        if name == field_name {
                            return LatticeType::Concrete(ty.clone());
                        }
                    }
                }
            }
            // If we can't determine the exact field, return Top
            LatticeType::Top
        }
        LatticeType::Concrete(ConcreteType::Struct { .. }) => {
            // Struct field type lookup is handled by special-case code in
            // InferenceEngine::infer_expr() for Expr::Call with function == "getfield".
            // The engine has access to the struct_table and can resolve field types there.
            // This transfer function is only used as a fallback when the special handling
            // doesn't apply (e.g., dynamic field names), so we conservatively return Top.
            LatticeType::Top
        }
        LatticeType::Concrete(ConcreteType::Module { .. }) => {
            // Module field access can return any type
            LatticeType::Top
        }
        _ => LatticeType::Top,
    }
}

/// Contextual transfer function for `getfield` with struct table access.
///
/// This function can resolve field types for structs by looking up the
/// struct definition in the provided context's struct table.
///
/// # Type Rules
/// - getfield(Struct, Symbol) → Field type (if found in struct table)
/// - getfield(NamedTuple, Symbol) → Field type
/// - Falls back to Top for unknown types/fields
///
/// # Examples
/// ```text
/// getfield(Point{x::Int64, y::Float64}, :x) → Int64 (with struct table)
/// getfield(Point{x::Int64, y::Float64}, :y) → Float64 (with struct table)
/// ```
pub fn tfunc_getfield_contextual(args: &[LatticeType], ctx: &TFuncContext) -> LatticeType {
    if args.len() < 2 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Struct { name, .. }) => {
            // Try to look up the struct in the struct table
            if let Some(struct_table) = ctx.struct_table {
                if let Some(struct_info) = struct_table.get(name) {
                    // Try to extract field name from the second argument (String or Symbol)
                    let field_name = match &args[1] {
                        LatticeType::Const(ConstValue::String(s)) => Some(s.as_str()),
                        LatticeType::Const(ConstValue::Symbol(s)) => Some(s.as_str()),
                        _ => None,
                    };

                    if let Some(field) = field_name {
                        if let Some(field_ty) = struct_info.get_field_type(field) {
                            return field_ty.clone();
                        }
                    }
                }
            }
            // Struct not found in table or field not found - fall back to Top
            LatticeType::Top
        }
        LatticeType::Concrete(ConcreteType::NamedTuple { fields }) => {
            // For NamedTuple, look up the field type if the field name is known
            if let LatticeType::Const(ConstValue::String(field_name)) = &args[1] {
                for (name, ty) in fields {
                    if name == field_name {
                        return LatticeType::Concrete(ty.clone());
                    }
                }
            }
            LatticeType::Top
        }
        LatticeType::Concrete(ConcreteType::Module { .. }) => {
            // Module field access can return any type
            LatticeType::Top
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `setfield!` (field mutation).
///
/// Type rules:
/// - setfield!(Struct, Symbol, value) → typeof(value)
///
/// # Examples
/// ```text
/// setfield!(point, :x, 42) → Int64
/// ```
pub fn tfunc_setfield(args: &[LatticeType]) -> LatticeType {
    if args.len() != 3 {
        return LatticeType::Top;
    }

    // setfield! returns the assigned value's type
    args[2].clone()
}

/// Transfer function for `fieldnames` (get field names of a type).
///
/// Type rules:
/// - fieldnames(Type) → Tuple{Vararg{Symbol}}
///
/// # Examples
/// ```text
/// fieldnames(Point) → Tuple{Symbol, Symbol}
/// ```
pub fn tfunc_fieldnames(_args: &[LatticeType]) -> LatticeType {
    // fieldnames returns a tuple of symbols
    // For simplicity, we return a Tuple of Symbols
    LatticeType::Concrete(ConcreteType::Tuple {
        elements: vec![ConcreteType::Symbol],
    })
}

/// Transfer function for `fieldtypes` (get field types of a type).
///
/// Type rules:
/// - fieldtypes(Type) → Tuple{Vararg{DataType}}
///
/// # Examples
/// ```text
/// fieldtypes(Point) → Tuple{Type{Int64}, Type{Float64}}
/// ```
pub fn tfunc_fieldtypes(_args: &[LatticeType]) -> LatticeType {
    // fieldtypes returns a tuple of DataType objects
    LatticeType::Concrete(ConcreteType::Tuple {
        elements: vec![ConcreteType::DataType {
            name: "Type".to_string(),
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::ConstValue;

    #[test]
    fn test_getfield_named_tuple_known_field() {
        let named_tuple = LatticeType::Concrete(ConcreteType::NamedTuple {
            fields: vec![
                ("x".to_string(), ConcreteType::Int64),
                ("y".to_string(), ConcreteType::Float64),
            ],
        });
        let field_name = LatticeType::Const(ConstValue::String("x".to_string()));
        let args = vec![named_tuple, field_name];
        let result = tfunc_getfield(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_getfield_named_tuple_unknown_field() {
        let named_tuple = LatticeType::Concrete(ConcreteType::NamedTuple {
            fields: vec![
                ("x".to_string(), ConcreteType::Int64),
                ("y".to_string(), ConcreteType::Float64),
            ],
        });
        let field_name = LatticeType::Top; // Unknown field
        let args = vec![named_tuple, field_name];
        let result = tfunc_getfield(&args);
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn test_getfield_struct() {
        let struct_type = LatticeType::Concrete(ConcreteType::Struct {
            name: "Point".to_string(),
            type_id: 1,
        });
        let field_name = LatticeType::Const(ConstValue::String("x".to_string()));
        let args = vec![struct_type, field_name];
        let result = tfunc_getfield(&args);
        // Returns Top as a fallback - actual struct field lookup is done in InferenceEngine
        // via special handling for getfield calls (see engine.rs test_getfield_call_with_struct_table)
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn test_setfield_returns_value_type() {
        let struct_type = LatticeType::Concrete(ConcreteType::Struct {
            name: "Point".to_string(),
            type_id: 1,
        });
        let field_name = LatticeType::Const(ConstValue::String("x".to_string()));
        let value = LatticeType::Concrete(ConcreteType::Int64);
        let args = vec![struct_type, field_name, value.clone()];
        let result = tfunc_setfield(&args);
        assert_eq!(result, value);
    }

    #[test]
    fn test_fieldnames() {
        let type_arg = LatticeType::Top;
        let args = vec![type_arg];
        let result = tfunc_fieldnames(&args);
        assert!(matches!(
            result,
            LatticeType::Concrete(ConcreteType::Tuple { .. })
        ));
    }

    #[test]
    fn test_fieldtypes() {
        let type_arg = LatticeType::Top;
        let args = vec![type_arg];
        let result = tfunc_fieldtypes(&args);
        assert!(matches!(
            result,
            LatticeType::Concrete(ConcreteType::Tuple { .. })
        ));
    }
}
