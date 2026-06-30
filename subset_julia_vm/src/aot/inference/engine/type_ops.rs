use super::super::super::types::StaticType;
use crate::inference_core::PrimitiveNumeric;
use crate::ir::core::Expr;

use super::TypeInferenceEngine;

impl TypeInferenceEngine {
    /// Numeric type promotion following Julia's type promotion rules.
    pub(crate) fn numeric_promote(&self, left: &StaticType, right: &StaticType) -> StaticType {
        if matches!(left, StaticType::Struct { .. }) && right.is_numeric() {
            return left.clone();
        }
        if matches!(right, StaticType::Struct { .. }) && left.is_numeric() {
            return right.clone();
        }

        // Same non-small types short-circuit (float, I64/U64, Struct, non-numeric).
        // Small integers (Bool..U32) and mixed types fall through to rank-based widening.
        if left == right
            && !matches!(
                left,
                StaticType::Bool
                    | StaticType::I8
                    | StaticType::U8
                    | StaticType::I16
                    | StaticType::U16
                    | StaticType::I32
                    | StaticType::U32
            )
        {
            return left.clone();
        }

        match (left.primitive_numeric(), right.primitive_numeric()) {
            (Some(l), Some(r)) => primitive_numeric_to_static(l.promote(r)),
            (Some(_), None) => left.clone(),
            (None, Some(_)) => right.clone(),
            _ => StaticType::Any,
        }
    }

    /// Get common integer type for integer division and modulo.
    pub(crate) fn integer_type(&self, left: &StaticType, right: &StaticType) -> StaticType {
        if left.is_integer() && right.is_integer() {
            if matches!((left, right), (StaticType::Bool, StaticType::Bool)) {
                return StaticType::Bool;
            }
            self.numeric_promote(left, right)
        } else if left.is_numeric() && right.is_numeric() {
            StaticType::I64
        } else {
            StaticType::Any
        }
    }

    /// Join two types (for control flow merge points).
    ///
    /// `Any` is the top element and absorbs: join(Any, T) = Any. (Issue #3461)
    pub fn join_types(&self, t1: &StaticType, t2: &StaticType) -> StaticType {
        if t1 == t2 {
            return t1.clone();
        }

        if matches!(t1, StaticType::Any) || matches!(t2, StaticType::Any) {
            return StaticType::Any;
        }

        if t1.is_numeric() && t2.is_numeric() {
            return self.numeric_promote(t1, t2);
        }

        if let Some(joined) = t1.core_typejoin(t2) {
            return joined;
        }

        StaticType::Union {
            variants: vec![t1.clone(), t2.clone()],
        }
    }

    /// Meet two types (for intersection).
    ///
    /// `Any` is the top element, so meet(Any, T) = T. Other pairs route through
    /// the shared `CoreType` lattice (subtype/intersection semantics) so AoT
    /// narrowing matches VM/compiler dispatch instead of over-widening to a
    /// misleading backend type. A provably-disjoint meet yields the empty union
    /// (`Union{}`); only meets whose narrowed `CoreType` has no stable AoT
    /// projection fall back to `Any` (Issue #3912).
    pub fn meet_types(&self, t1: &StaticType, t2: &StaticType) -> StaticType {
        if t1 == t2 {
            return t1.clone();
        }
        if matches!(t1, StaticType::Any) {
            return t2.clone();
        }
        if matches!(t2, StaticType::Any) {
            return t1.clone();
        }

        t1.core_typeintersect(t2).unwrap_or(StaticType::Any)
    }

    /// Lookup type of global constant or well-known value.
    pub fn lookup_global_or_const(&self, name: &str) -> StaticType {
        if let Some(ty) = self.env.get(name) {
            return ty.clone();
        }
        // `@enum` members persist across per-function `env.clear()` (Issue #7050).
        if let Some(ty) = self.enum_members.get(name) {
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
            // Complex imaginary unit (Issue #3410)
            "im" => StaticType::Struct {
                type_id: 0,
                name: "Complex".to_string(),
            },
            _ => StaticType::Any,
        }
    }

    /// Infer element type of an iterator expression.
    pub fn infer_iterator_element_type(&self, iter: &Expr) -> StaticType {
        let iter_ty = self.infer_expr_type(iter);
        match &iter_ty {
            StaticType::Array { element, .. } => (**element).clone(),
            StaticType::Range { element } => (**element).clone(),
            StaticType::Generator { element } => (**element).clone(),
            StaticType::Set { element } => (**element).clone(),
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
    ///
    /// `Any` is absorbing: unify(Any, T) = Any. (Issue #3461)
    pub fn unify_types(&self, t1: &StaticType, t2: &StaticType) -> StaticType {
        if t1 == t2 {
            return t1.clone();
        }

        match (t1, t2) {
            (StaticType::Any, _) | (_, StaticType::Any) => StaticType::Any,
            (StaticType::I64, StaticType::F64) | (StaticType::F64, StaticType::I64) => {
                StaticType::F64
            }
            (StaticType::I32, StaticType::F64) | (StaticType::F64, StaticType::I32) => {
                StaticType::F64
            }
            (StaticType::I32, StaticType::F32) | (StaticType::F32, StaticType::I32) => {
                StaticType::F32
            }
            (StaticType::I64, StaticType::F32) | (StaticType::F32, StaticType::I64) => {
                StaticType::F32
            }
            (StaticType::I64, StaticType::I32) | (StaticType::I32, StaticType::I64) => {
                StaticType::I64
            }
            (StaticType::F64, StaticType::F32) | (StaticType::F32, StaticType::F64) => {
                StaticType::F64
            }
            _ => self.join_types(t1, t2),
        }
    }
}

fn primitive_numeric_to_static(kind: PrimitiveNumeric) -> StaticType {
    StaticType::from_primitive_numeric(kind)
}
