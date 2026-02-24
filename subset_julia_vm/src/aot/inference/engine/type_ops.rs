use super::super::types::StaticType;
use crate::ir::core::Expr;

use super::TypeInferenceEngine;

impl TypeInferenceEngine {
    /// Numeric type promotion following Julia's type promotion rules.
    pub(crate) fn numeric_promote(&self, left: &StaticType, right: &StaticType) -> StaticType {
        if left == right {
            return left.clone();
        }

        if matches!(left, StaticType::Struct { .. }) && right.is_numeric() {
            return left.clone();
        }
        if matches!(right, StaticType::Struct { .. }) && left.is_numeric() {
            return right.clone();
        }

        fn numeric_rank(ty: &StaticType) -> Option<i32> {
            match ty {
                StaticType::Bool => Some(0),
                StaticType::I8 => Some(1),
                StaticType::U8 => Some(2),
                StaticType::I16 => Some(3),
                StaticType::U16 => Some(4),
                StaticType::I32 => Some(5),
                StaticType::U32 => Some(6),
                StaticType::I64 => Some(7),
                StaticType::U64 => Some(8),
                StaticType::F32 => Some(100),
                StaticType::F64 => Some(101),
                _ => None,
            }
        }

        fn is_float(ty: &StaticType) -> bool {
            matches!(ty, StaticType::F32 | StaticType::F64)
        }

        let left_rank = numeric_rank(left);
        let right_rank = numeric_rank(right);

        match (left_rank, right_rank) {
            (Some(l), Some(r)) => {
                if is_float(left) && is_float(right) {
                    if l >= r {
                        left.clone()
                    } else {
                        right.clone()
                    }
                } else if is_float(left) {
                    left.clone()
                } else if is_float(right) {
                    right.clone()
                } else {
                    let max_rank = l.max(r);
                    if max_rank <= 0 {
                        StaticType::I64
                    } else if max_rank >= 7 {
                        if l >= r {
                            left.clone()
                        } else {
                            right.clone()
                        }
                    } else {
                        StaticType::I64
                    }
                }
            }
            (Some(_), None) => {
                if left.is_numeric() {
                    left.clone()
                } else {
                    StaticType::Any
                }
            }
            (None, Some(_)) => {
                if right.is_numeric() {
                    right.clone()
                } else {
                    StaticType::Any
                }
            }
            _ => StaticType::Any,
        }
    }

    /// Get common integer type for integer division and modulo.
    pub(crate) fn integer_type(&self, left: &StaticType, right: &StaticType) -> StaticType {
        if left.is_integer() && right.is_integer() {
            self.numeric_promote(left, right)
        } else if left.is_numeric() && right.is_numeric() {
            StaticType::I64
        } else {
            StaticType::Any
        }
    }

    /// Join two types (for control flow merge points).
    pub fn join_types(&self, t1: &StaticType, t2: &StaticType) -> StaticType {
        if t1 == t2 {
            return t1.clone();
        }

        if matches!(t1, StaticType::Any) {
            return t2.clone();
        }
        if matches!(t2, StaticType::Any) {
            return t1.clone();
        }

        if t1.is_numeric() && t2.is_numeric() {
            return self.numeric_promote(t1, t2);
        }

        StaticType::Union {
            variants: vec![t1.clone(), t2.clone()],
        }
    }

    /// Meet two types (for intersection).
    pub fn meet_types(&self, t1: &StaticType, t2: &StaticType) -> StaticType {
        if t1 == t2 {
            t1.clone()
        } else if matches!(t1, StaticType::Any) {
            t2.clone()
        } else if matches!(t2, StaticType::Any) {
            t1.clone()
        } else {
            StaticType::Any
        }
    }

    /// Lookup type of global constant or well-known value.
    pub fn lookup_global_or_const(&self, name: &str) -> StaticType {
        if let Some(ty) = self.env.get(name) {
            return ty.clone();
        }

        match name {
            "pi" | "π" => StaticType::F64,
            "ℯ" | "e" => StaticType::F64,
            "Inf" | "Inf64" => StaticType::F64,
            "Inf32" => StaticType::F32,
            "NaN" | "NaN64" => StaticType::F64,
            "NaN32" => StaticType::F32,
            "true" | "false" => StaticType::Bool,
            "nothing" => StaticType::Nothing,
            "missing" => StaticType::Missing,
            "typemax" | "typemin" => StaticType::Any,
            _ => StaticType::Any,
        }
    }

    /// Infer element type of an iterator expression.
    pub fn infer_iterator_element_type(&self, iter: &Expr) -> StaticType {
        let iter_ty = self.infer_expr_type(iter);
        match &iter_ty {
            StaticType::Array { element, .. } => (**element).clone(),
            StaticType::Range { element } => (**element).clone(),
            StaticType::Str => StaticType::Char,
            StaticType::Tuple(elements) => {
                if !elements.is_empty() && elements.iter().all(|e| e == &elements[0]) {
                    elements[0].clone()
                } else if elements.is_empty() {
                    StaticType::Any
                } else {
                    StaticType::Union {
                        variants: elements.clone(),
                    }
                }
            }
            StaticType::Dict { key, value } => {
                StaticType::Tuple(vec![(**key).clone(), (**value).clone()])
            }
            _ => StaticType::Any,
        }
    }

    /// Unify two types (alias for join_types with promotion).
    pub fn unify_types(&self, t1: &StaticType, t2: &StaticType) -> StaticType {
        if t1 == t2 {
            return t1.clone();
        }

        match (t1, t2) {
            (StaticType::I64, StaticType::F64) | (StaticType::F64, StaticType::I64) => {
                StaticType::F64
            }
            (StaticType::I32, StaticType::F64) | (StaticType::F64, StaticType::I32) => {
                StaticType::F64
            }
            (StaticType::I32, StaticType::F32) | (StaticType::F32, StaticType::I32) => {
                StaticType::F32
            }
            (StaticType::I64, StaticType::I32) | (StaticType::I32, StaticType::I64) => {
                StaticType::I64
            }
            (StaticType::F64, StaticType::F32) | (StaticType::F32, StaticType::F64) => {
                StaticType::F64
            }
            (StaticType::Any, other) | (other, StaticType::Any) => other.clone(),
            _ => self.join_types(t1, t2),
        }
    }
}
