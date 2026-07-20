//! Simple bytecode-level return-type inference helpers.
//!
//! These helpers use only Core IR plus bytecode `ValueType` metadata, so they
//! live below both compiler and VM during the #9090 crate-split cleanup.

use subset_julia_vm_types::ir::core::{BinaryOp, Expr, Function, Literal, Stmt, UnaryOp};
use subset_julia_vm_types::{promotion, types};

use crate::ValueType;

/// Promote two simple numeric `ValueType`s using the shared Julia promotion table.
pub fn promote_numeric_value_types(left: &ValueType, right: &ValueType) -> Option<ValueType> {
    if !is_simple_numeric_value_type(left) || !is_simple_numeric_value_type(right) {
        return None;
    }
    let left_name = value_type_to_type_name(left);
    let right_name = value_type_to_type_name(right);
    if left_name == "Any" || right_name == "Any" {
        return None;
    }
    let promoted = promotion::promote_type(left_name, right_name);
    type_name_to_value_type(&promoted)
}

/// Infer the return `ValueType` for straight-line one-return functions over known argument tags.
pub fn infer_simple_function_return_type_for_value_args(
    func: &Function,
    arg_types: &[ValueType],
) -> Option<ValueType> {
    if func.params.len() != arg_types.len() {
        return None;
    }
    let bindings: Vec<(&str, ValueType)> = func
        .params
        .iter()
        .zip(arg_types.iter())
        .map(|(param, ty)| (param.name.as_str(), ty.clone()))
        .collect();
    let Stmt::Return {
        value: Some(expr), ..
    } = func.body.stmts.first()?
    else {
        return None;
    };
    infer_simple_bound_expr_type(expr, &bindings).filter(|ty| !matches!(ty, ValueType::Any))
}

fn value_type_to_type_name(vt: &ValueType) -> &'static str {
    match vt {
        ValueType::F16 => "Float16",
        ValueType::F64 => "Float64",
        ValueType::F32 => "Float32",
        ValueType::BigFloat => "BigFloat",
        ValueType::I64 => "Int64",
        ValueType::I32 => "Int32",
        ValueType::I16 => "Int16",
        ValueType::I8 => "Int8",
        ValueType::I128 => "Int128",
        ValueType::BigInt => "BigInt",
        ValueType::U8 => "UInt8",
        ValueType::U16 => "UInt16",
        ValueType::U32 => "UInt32",
        ValueType::U64 => "UInt64",
        ValueType::U128 => "UInt128",
        ValueType::Bool => "Bool",
        _ => "Any",
    }
}

fn type_name_to_value_type(name: &str) -> Option<ValueType> {
    match name {
        "Int8" => Some(ValueType::I8),
        "Int16" => Some(ValueType::I16),
        "Int32" => Some(ValueType::I32),
        "Int" if types::native_int_type_name() == "Int32" => Some(ValueType::I32),
        "Int64" | "Int" => Some(ValueType::I64),
        "Int128" => Some(ValueType::I128),
        "BigInt" => Some(ValueType::BigInt),
        "UInt8" => Some(ValueType::U8),
        "UInt16" => Some(ValueType::U16),
        "UInt32" => Some(ValueType::U32),
        "UInt" if types::native_uint_type_name() == "UInt32" => Some(ValueType::U32),
        "UInt64" | "UInt" => Some(ValueType::U64),
        "UInt128" => Some(ValueType::U128),
        "Float16" => Some(ValueType::F16),
        "Float32" => Some(ValueType::F32),
        "Float64" => Some(ValueType::F64),
        "BigFloat" => Some(ValueType::BigFloat),
        "Bool" => Some(ValueType::Bool),
        "Any" => Some(ValueType::Any),
        _ => None,
    }
}

fn is_simple_numeric_value_type(vt: &ValueType) -> bool {
    matches!(
        vt,
        ValueType::I8
            | ValueType::I16
            | ValueType::I32
            | ValueType::I64
            | ValueType::I128
            | ValueType::BigInt
            | ValueType::U8
            | ValueType::U16
            | ValueType::U32
            | ValueType::U64
            | ValueType::U128
            | ValueType::F16
            | ValueType::F32
            | ValueType::F64
            | ValueType::BigFloat
            | ValueType::Bool
    )
}

fn infer_simple_bound_expr_type(expr: &Expr, bindings: &[(&str, ValueType)]) -> Option<ValueType> {
    match expr {
        Expr::Var(name, _) => bindings
            .iter()
            .find_map(|(binding_name, ty)| (*binding_name == name).then(|| ty.clone())),
        Expr::Literal(literal, _) => simple_literal_value_type(literal),
        Expr::UnaryOp { op, operand, .. } => {
            let operand_type = infer_simple_bound_expr_type(operand, bindings)?;
            match op {
                UnaryOp::Neg | UnaryOp::Pos if is_simple_numeric_value_type(&operand_type) => {
                    Some(operand_type)
                }
                _ => None,
            }
        }
        Expr::BinaryOp {
            op, left, right, ..
        } => {
            let left_type = infer_simple_bound_expr_type(left, bindings)?;
            let right_type = infer_simple_bound_expr_type(right, bindings)?;
            infer_simple_binary_result_type(*op, &left_type, &right_type)
        }
        Expr::Call { function, args, .. } => {
            let arg_types: Option<Vec<ValueType>> = args
                .iter()
                .map(|arg| infer_simple_bound_expr_type(arg, bindings))
                .collect();
            infer_simple_call_result_type(function, &arg_types?)
        }
        _ => None,
    }
}

fn simple_literal_value_type(literal: &Literal) -> Option<ValueType> {
    match literal {
        Literal::Int(_) => Some(ValueType::I64),
        Literal::Int128(_) => Some(ValueType::I128),
        Literal::BigInt(_) => Some(ValueType::BigInt),
        Literal::BigFloat(_) => Some(ValueType::BigFloat),
        Literal::Float(_) => Some(ValueType::F64),
        Literal::Float32(_) => Some(ValueType::F32),
        Literal::Float16(_) => Some(ValueType::F16),
        Literal::Bool(_) => Some(ValueType::Bool),
        Literal::Str(_) => Some(ValueType::Str),
        Literal::Char(_) => Some(ValueType::Char),
        _ => None,
    }
}

fn infer_simple_call_result_type(function: &str, arg_types: &[ValueType]) -> Option<ValueType> {
    if arg_types.is_empty() {
        return None;
    }
    match function {
        "+" => fold_numeric_result_types(arg_types),
        "*" => {
            if arg_types.iter().all(|ty| matches!(ty, ValueType::Str)) {
                Some(ValueType::Str)
            } else {
                fold_numeric_result_types(arg_types)
            }
        }
        "-" => fold_numeric_result_types(arg_types),
        "/" => {
            if arg_types.iter().all(is_simple_numeric_value_type) {
                Some(ValueType::F64)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn infer_simple_binary_result_type(
    op: BinaryOp,
    left: &ValueType,
    right: &ValueType,
) -> Option<ValueType> {
    match op {
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => promote_numeric_value_types(left, right),
        BinaryOp::Div => {
            if is_simple_numeric_value_type(left) && is_simple_numeric_value_type(right) {
                Some(ValueType::F64)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn fold_numeric_result_types(arg_types: &[ValueType]) -> Option<ValueType> {
    let mut iter = arg_types.iter();
    let first = iter.next()?.clone();
    if !is_simple_numeric_value_type(&first) {
        return None;
    }
    iter.try_fold(first, |acc, ty| promote_numeric_value_types(&acc, ty))
}
