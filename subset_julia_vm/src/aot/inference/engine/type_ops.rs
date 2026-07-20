use super::super::super::types::StaticType;
use crate::inference_core::PrimitiveNumeric;
use crate::ir::core::Expr;
use crate::promotion::promote_type;

use super::TypeInferenceEngine;

impl TypeInferenceEngine {
    /// Numeric type promotion following Julia's type promotion rules.
    ///
    /// Primitive numeric pairs are routed through the **shared** string
    /// `promote_type` path used by the VM runtime and compiler inference
    /// (Issue #9351). Historically the AoT engine had its own rank-based
    /// widener (`PrimitiveNumeric::promote`) that force-widened `Bool` and
    /// every integer up to `UInt32` to `Int64`, diverging from upstream Julia
    /// (`Int8 + Int8 === Int8`, `Int8 + Int16 === Int16`) and from the runtime.
    /// This is the pure promotion of the operand *types*; op-specific result
    /// rules (e.g. `Bool + Bool === Int64`) live in `binop_result_type`.
    pub(crate) fn numeric_promote(&self, left: &StaticType, right: &StaticType) -> StaticType {
        if let Some(promoted) = Self::promote_bare_complex_with_numeric(left, right) {
            return promoted;
        }
        if matches!(left, StaticType::Struct { .. }) && right.is_numeric() {
            return left.clone();
        }
        if matches!(right, StaticType::Struct { .. }) && left.is_numeric() {
            return right.clone();
        }

        // Same type: no promotion needed (covers primitives, structs, and
        // non-numeric variants). `promote_type` short-circuits identical names
        // as well, but this keeps struct/non-numeric identities exact.
        if left == right {
            return left.clone();
        }

        match (left.primitive_numeric(), right.primitive_numeric()) {
            (Some(l), Some(r)) => Self::promote_primitive_pair(l, r),
            (Some(_), None) => left.clone(),
            (None, Some(_)) => right.clone(),
            _ => StaticType::Any,
        }
    }

    /// Promote a pair of primitive numerics through the shared string
    /// `promote_type` path (Issue #9351), preserving upstream narrow-int result
    /// kinds instead of widening to `Int64`.
    fn promote_primitive_pair(left: PrimitiveNumeric, right: PrimitiveNumeric) -> StaticType {
        let promoted = promote_type(left.julia_name(), right.julia_name());
        PrimitiveNumeric::from_julia_name(&promoted)
            .map(StaticType::from_primitive_numeric)
            .unwrap_or(StaticType::Any)
    }

    fn promote_bare_complex_with_numeric(
        left: &StaticType,
        right: &StaticType,
    ) -> Option<StaticType> {
        match (left, right) {
            (
                StaticType::Struct {
                    name: bare_name, ..
                },
                concrete @ StaticType::Struct {
                    name: concrete_name,
                    ..
                },
            ) if Self::is_bare_complex_name(bare_name)
                && StaticType::complex_param_type_from_name(concrete_name).is_some() =>
            {
                Some(concrete.clone())
            }
            (
                concrete @ StaticType::Struct {
                    name: concrete_name,
                    ..
                },
                StaticType::Struct {
                    name: bare_name, ..
                },
            ) if StaticType::complex_param_type_from_name(concrete_name).is_some()
                && Self::is_bare_complex_name(bare_name) =>
            {
                Some(concrete.clone())
            }
            (complex @ StaticType::Struct { name, .. }, numeric)
                if Self::is_bare_complex_name(name) =>
            {
                Self::complex_type_for_numeric(numeric).or_else(|| Some(complex.clone()))
            }
            (numeric, complex @ StaticType::Struct { name, .. })
                if Self::is_bare_complex_name(name) =>
            {
                Self::complex_type_for_numeric(numeric).or_else(|| Some(complex.clone()))
            }
            _ => None,
        }
    }

    fn is_bare_complex_name(name: &str) -> bool {
        name == "Complex"
    }

    fn complex_type_for_numeric(numeric: &StaticType) -> Option<StaticType> {
        let param = numeric.primitive_numeric()?.julia_name();
        Some(StaticType::Struct {
            type_id: 0,
            name: format!("Complex{{{param}}}"),
        })
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
    ///
    /// Numeric pairs are delegated to `join_types` → `numeric_promote`, which
    /// routes through the shared `promote_type` path. The previous hardcoded
    /// mixed-pair table (I32/I64/F32/F64 only) was an independent promotion
    /// table that duplicated — and could drift from — that shared path
    /// (Issue #9351); it is removed in favour of the single converged route.
    pub fn unify_types(&self, t1: &StaticType, t2: &StaticType) -> StaticType {
        if t1 == t2 {
            return t1.clone();
        }

        match (t1, t2) {
            (StaticType::Any, _) | (_, StaticType::Any) => StaticType::Any,
            _ => self.join_types(t1, t2),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bare_complex() -> StaticType {
        StaticType::Struct {
            type_id: 0,
            name: "Complex".to_string(),
        }
    }

    fn complex_type(param: &str) -> StaticType {
        StaticType::Struct {
            type_id: 0,
            name: format!("Complex{{{param}}}"),
        }
    }

    #[test]
    fn bare_complex_numeric_promotion_uses_shared_numeric_taxonomy_9909() {
        let engine = TypeInferenceEngine::new();
        for (numeric, param) in [
            (StaticType::Bool, "Bool"),
            (StaticType::I128, "Int128"),
            (StaticType::U128, "UInt128"),
            (StaticType::F16, "Float16"),
        ] {
            assert_eq!(
                engine.numeric_promote(&bare_complex(), &numeric),
                complex_type(param)
            );
            assert_eq!(
                engine.numeric_promote(&numeric, &bare_complex()),
                complex_type(param)
            );
        }
    }
}
