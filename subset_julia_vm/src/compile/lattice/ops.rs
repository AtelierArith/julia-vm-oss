//! Lattice operations for type inference.
//!
//! This module implements the core operations on the type lattice:
//! - join (⊔): least upper bound (union of types)
//! - meet (⊓): greatest lower bound (intersection of types)
//! - is_subtype_of (⊑): subtype relation
//! - subtract: type subtraction for narrowing
//!
//! These operations follow Julia's type lattice semantics.

use super::types::{ConcreteType, LatticeType};
use super::widening::{MAX_UNION_COMPLEXITY, MAX_UNION_LENGTH};
use crate::compile::diagnostics::{emit_conditional_join, emit_union_widened, DiagnosticReason};
use std::collections::BTreeSet;

impl LatticeType {
    /// Join operation (⊔): compute the least upper bound of two types.
    ///
    /// This creates a type that is a supertype of both inputs.
    /// In Julia, this corresponds to creating a Union type.
    ///
    /// # Examples
    /// ```text
    /// Int64.join(Float64) = Union{Int64, Float64}
    /// Int64.join(Int64) = Int64
    /// Const(42).join(Const(42)) = Const(42)
    /// Const(42).join(Const(43)) = Concrete(Int64)
    /// Const(42).join(Int64) = Concrete(Int64)
    /// Bottom.join(T) = T
    /// T.join(Top) = Top
    /// ```
    pub fn join(&self, other: &LatticeType) -> LatticeType {
        match (self, other) {
            // Bottom is the identity element for join
            (LatticeType::Bottom, t) | (t, LatticeType::Bottom) => t.clone(),

            // Top is the absorbing element for join
            (LatticeType::Top, _) | (_, LatticeType::Top) => LatticeType::Top,

            // Same constant value → keep constant
            (LatticeType::Const(a), LatticeType::Const(b)) if a == b => {
                LatticeType::Const(a.clone())
            }

            // Different constant values → widen to concrete type
            (LatticeType::Const(a), LatticeType::Const(b)) => {
                LatticeType::Concrete(a.to_concrete_type())
                    .join(&LatticeType::Concrete(b.to_concrete_type()))
            }

            // Const + Concrete → widen to concrete
            (LatticeType::Const(cv), LatticeType::Concrete(ct))
            | (LatticeType::Concrete(ct), LatticeType::Const(cv)) => {
                if &cv.to_concrete_type() == ct {
                    LatticeType::Concrete(ct.clone())
                } else {
                    // Different concrete types
                    LatticeType::Concrete(cv.to_concrete_type())
                        .join(&LatticeType::Concrete(ct.clone()))
                }
            }

            // Const + Union → widen const to concrete and join with union
            (LatticeType::Const(cv), LatticeType::Union(us))
            | (LatticeType::Union(us), LatticeType::Const(cv)) => {
                let concrete = cv.to_concrete_type();
                let mut new_set = us.clone();
                new_set.insert(concrete);
                Self::simplify_union(new_set)
            }

            // Same concrete type
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) if a == b => {
                LatticeType::Concrete(a.clone())
            }

            // Different concrete types → Union
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) => {
                let mut set = BTreeSet::new();
                set.insert(a.clone());
                set.insert(b.clone());
                Self::simplify_union(set)
            }

            // Union + Concrete
            (LatticeType::Union(us), LatticeType::Concrete(c))
            | (LatticeType::Concrete(c), LatticeType::Union(us)) => {
                let mut new_set = us.clone();
                new_set.insert(c.clone());
                Self::simplify_union(new_set)
            }

            // Union + Union
            (LatticeType::Union(a), LatticeType::Union(b)) => {
                let combined: BTreeSet<_> = a.union(b).cloned().collect();
                Self::simplify_union(combined)
            }

            // Conditional types are conservatively handled as Top
            (LatticeType::Conditional { .. }, _) | (_, LatticeType::Conditional { .. }) => {
                emit_conditional_join();
                LatticeType::Top
            }
        }
    }

    /// Meet operation (⊓): compute the greatest lower bound of two types.
    ///
    /// This creates a type that is a subtype of both inputs.
    /// In Julia, this corresponds to type intersection.
    ///
    /// # Examples
    /// ```text
    /// Int64.meet(Float64) = Bottom
    /// Int64.meet(Int64) = Int64
    /// Const(42).meet(Const(42)) = Const(42)
    /// Const(42).meet(Const(43)) = Bottom
    /// Const(42).meet(Int64) = Const(42)
    /// Union{Int, Float}.meet(Int) = Int
    /// Top.meet(T) = T
    /// ```
    pub fn meet(&self, other: &LatticeType) -> LatticeType {
        match (self, other) {
            // Top is the identity element for meet
            (LatticeType::Top, t) | (t, LatticeType::Top) => t.clone(),

            // Bottom is the absorbing element for meet
            (LatticeType::Bottom, _) | (_, LatticeType::Bottom) => LatticeType::Bottom,

            // Same constant → keep constant
            (LatticeType::Const(a), LatticeType::Const(b)) if a == b => {
                LatticeType::Const(a.clone())
            }

            // Different constants → Bottom (empty intersection)
            (LatticeType::Const(_), LatticeType::Const(_)) => LatticeType::Bottom,

            // Const ⊓ Concrete → Const if types match, Bottom otherwise
            (LatticeType::Const(cv), LatticeType::Concrete(ct))
            | (LatticeType::Concrete(ct), LatticeType::Const(cv)) => {
                if &cv.to_concrete_type() == ct {
                    LatticeType::Const(cv.clone())
                } else {
                    LatticeType::Bottom
                }
            }

            // Same concrete type
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) if a == b => {
                LatticeType::Concrete(a.clone())
            }

            // Different concrete types → Bottom (empty intersection)
            (LatticeType::Concrete(_), LatticeType::Concrete(_)) => LatticeType::Bottom,

            // Union and Concrete intersection
            (LatticeType::Union(us), LatticeType::Concrete(c))
            | (LatticeType::Concrete(c), LatticeType::Union(us)) => {
                if us.contains(c) {
                    LatticeType::Concrete(c.clone())
                } else {
                    LatticeType::Bottom
                }
            }

            // Union and Union intersection
            (LatticeType::Union(a), LatticeType::Union(b)) => {
                let intersection: BTreeSet<_> = a.intersection(b).cloned().collect();
                if intersection.is_empty() {
                    LatticeType::Bottom
                } else if intersection.len() == 1 {
                    if let Some(only) = intersection.into_iter().next() {
                        LatticeType::Concrete(only)
                    } else {
                        LatticeType::Bottom
                    }
                } else {
                    LatticeType::Union(intersection)
                }
            }

            // Conditional types are conservatively handled
            _ => LatticeType::Bottom,
        }
    }

    /// Subtype relation (⊑): check if self is a subtype of other.
    ///
    /// Returns true if every value of type `self` is also of type `other`.
    ///
    /// # Examples
    /// ```text
    /// Bottom ⊑ T (for all T)
    /// T ⊑ Top (for all T)
    /// Int64 ⊑ Union{Int64, Float64}
    /// Int64 ⊑ Int64
    /// ```
    pub fn is_subtype_of(&self, other: &LatticeType) -> bool {
        match (self, other) {
            // Bottom is a subtype of everything
            (LatticeType::Bottom, _) => true,

            // Everything is a subtype of Top
            (_, LatticeType::Top) => true,

            // Top is not a subtype of anything except itself
            (LatticeType::Top, _) => false,

            // Concrete types must be equal
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) => a == b,

            // Concrete is a subtype of Union if it's an element
            (LatticeType::Concrete(c), LatticeType::Union(us)) => us.contains(c),

            // Union is a subtype of Union if all elements are contained
            (LatticeType::Union(a), LatticeType::Union(b)) => a.is_subset(b),

            // Union is never a subtype of a single Concrete
            (LatticeType::Union(_), LatticeType::Concrete(_)) => false,

            // Conservative handling for Conditional
            _ => false,
        }
    }

    /// Type subtraction for narrowing: compute `self - other`.
    ///
    /// Used for control-flow sensitive type narrowing.
    /// For example, after checking `x isa Int` is false,
    /// we know x is not Int, so we subtract Int from its type.
    ///
    /// # Examples
    /// ```text
    /// Union{Int, Float, String}.subtract(Int) = Union{Float, String}
    /// Int64.subtract(Int64) = Bottom
    /// Int64.subtract(Float64) = Int64
    /// ```
    pub fn subtract(&self, other: &LatticeType) -> LatticeType {
        match (self, other) {
            // Subtracting from Bottom or Top
            (LatticeType::Bottom, _) => LatticeType::Bottom,
            (LatticeType::Top, _) => LatticeType::Top, // Conservative

            // Subtracting Bottom or Top
            (t, LatticeType::Bottom) => t.clone(),
            (_, LatticeType::Top) => LatticeType::Bottom, // Everything is removed

            // Concrete - Concrete
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) => {
                if a == b {
                    LatticeType::Bottom
                } else {
                    LatticeType::Concrete(a.clone())
                }
            }

            // Concrete - Union
            (LatticeType::Concrete(c), LatticeType::Union(us)) => {
                if us.contains(c) {
                    LatticeType::Bottom
                } else {
                    LatticeType::Concrete(c.clone())
                }
            }

            // Union - Concrete
            (LatticeType::Union(us), LatticeType::Concrete(c)) => {
                let remaining: BTreeSet<_> = us.iter().filter(|t| *t != c).cloned().collect();
                Self::simplify_union(remaining)
            }

            // Union - Union
            (LatticeType::Union(a), LatticeType::Union(b)) => {
                let remaining: BTreeSet<_> = a.difference(b).cloned().collect();
                Self::simplify_union(remaining)
            }

            // Conservative for Conditional
            _ => self.clone(),
        }
    }

    /// Simplify a Union type, applying widening if necessary.
    ///
    /// Rules:
    /// - Empty set → Bottom
    /// - Single element → Concrete
    /// - Too many elements (> MAX_UNION_LENGTH) → widen
    /// - Too complex (> MAX_UNION_COMPLEXITY) → widen
    /// - Otherwise → Union
    fn simplify_union(types: BTreeSet<ConcreteType>) -> LatticeType {
        if types.is_empty() {
            return LatticeType::Bottom;
        }

        if types.len() == 1 {
            if let Some(only) = types.into_iter().next() {
                return LatticeType::Concrete(only);
            }
            return LatticeType::Bottom;
        }

        // Check if widening is needed based on length
        if types.len() > MAX_UNION_LENGTH {
            emit_union_widened(DiagnosticReason::UnionTooLarge(types.len()));
            return Self::widen_union(&types);
        }

        // Check complexity (maximum depth of nested types)
        let complexity = Self::compute_complexity(&types);
        if complexity > MAX_UNION_COMPLEXITY {
            emit_union_widened(DiagnosticReason::UnionTooComplex(complexity));
            return Self::widen_union(&types);
        }

        LatticeType::Union(types)
    }

    /// Widen a Union type to prevent infinite growth.
    ///
    /// Strategy:
    /// - If all types are numeric, widen to Union{Int64, Float64}
    /// - Otherwise, widen to Top
    fn widen_union(types: &BTreeSet<ConcreteType>) -> LatticeType {
        // Check if all types are numeric
        if types.iter().all(|t| t.is_numeric()) {
            // Normalize to Int64 + Float64
            let mut normalized = BTreeSet::new();
            normalized.insert(ConcreteType::Int64);
            normalized.insert(ConcreteType::Float64);
            return LatticeType::Union(normalized);
        }

        // Otherwise, widen to Top
        LatticeType::Top
    }

    /// Compute the complexity of a Union (maximum nesting depth).
    fn compute_complexity(types: &BTreeSet<ConcreteType>) -> usize {
        types.iter().map(Self::type_depth).max().unwrap_or(0)
    }

    /// Compute the nesting depth of a type.
    fn type_depth(ty: &ConcreteType) -> usize {
        match ty {
            // Simple types have depth 1
            ConcreteType::Int8
            | ConcreteType::Int16
            | ConcreteType::Int32
            | ConcreteType::Int64
            | ConcreteType::Int128
            | ConcreteType::BigInt
            | ConcreteType::UInt8
            | ConcreteType::UInt16
            | ConcreteType::UInt32
            | ConcreteType::UInt64
            | ConcreteType::UInt128
            | ConcreteType::Float16
            | ConcreteType::Float32
            | ConcreteType::Float64
            | ConcreteType::BigFloat
            | ConcreteType::Bool
            | ConcreteType::String
            | ConcreteType::Char
            | ConcreteType::Nothing
            | ConcreteType::Missing
            | ConcreteType::Symbol
            | ConcreteType::Pairs
            | ConcreteType::IO
            | ConcreteType::Expr
            | ConcreteType::QuoteNode
            | ConcreteType::LineNumberNode
            | ConcreteType::GlobalRef
            | ConcreteType::Regex
            | ConcreteType::RegexMatch
            // Abstract types have depth 1
            | ConcreteType::Number
            | ConcreteType::Integer
            | ConcreteType::AbstractFloat => 1,

            // Composite types have depth 1 + max element depth
            ConcreteType::Array { element } => 1 + Self::type_depth(element),
            ConcreteType::Tuple { elements } => {
                1 + elements.iter().map(Self::type_depth).max().unwrap_or(0)
            }
            ConcreteType::NamedTuple { fields } => {
                1 + fields
                    .iter()
                    .map(|(_, ty)| Self::type_depth(ty))
                    .max()
                    .unwrap_or(0)
            }

            // Range, Dict, Set, Generator types
            ConcreteType::Range { element } => 1 + Self::type_depth(element),
            ConcreteType::Dict { key, value } => {
                1 + Self::type_depth(key).max(Self::type_depth(value))
            }
            ConcreteType::Set { element } => 1 + Self::type_depth(element),
            ConcreteType::Generator { element } => 1 + Self::type_depth(element),

            // User-defined and type system types have depth 1
            ConcreteType::Struct { .. }
            | ConcreteType::Function { .. }
            | ConcreteType::DataType { .. }
            | ConcreteType::Module { .. }
            // Enum types have depth 1 (they are simple integer-backed types)
            | ConcreteType::Enum { .. } => 1,

            // Any is a top type with depth 1
            ConcreteType::Any => 1,

            // Union types have depth 1 + max element depth
            ConcreteType::UnionOf(types) => {
                1 + types.iter().map(Self::type_depth).max().unwrap_or(0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_concrete_same() {
        let int = LatticeType::Concrete(ConcreteType::Int64);
        let result = int.join(&int);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_join_concrete_different() {
        let int = LatticeType::Concrete(ConcreteType::Int64);
        let float = LatticeType::Concrete(ConcreteType::Float64);

        let result = int.join(&float);
        assert!(
            matches!(&result, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            result
        );
        if let LatticeType::Union(types) = result {
            assert_eq!(types.len(), 2);
            assert!(types.contains(&ConcreteType::Int64));
            assert!(types.contains(&ConcreteType::Float64));
        }
    }

    #[test]
    fn test_join_with_bottom() {
        let int = LatticeType::Concrete(ConcreteType::Int64);
        let bottom = LatticeType::Bottom;

        assert_eq!(int.join(&bottom), int);
        assert_eq!(bottom.join(&int), int);
    }

    #[test]
    fn test_join_with_top() {
        let int = LatticeType::Concrete(ConcreteType::Int64);
        let top = LatticeType::Top;

        assert_eq!(int.join(&top), LatticeType::Top);
        assert_eq!(top.join(&int), LatticeType::Top);
    }

    #[test]
    fn test_meet_concrete_same() {
        let int = LatticeType::Concrete(ConcreteType::Int64);
        let result = int.meet(&int);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_meet_concrete_different() {
        let int = LatticeType::Concrete(ConcreteType::Int64);
        let float = LatticeType::Concrete(ConcreteType::Float64);

        let result = int.meet(&float);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_meet_union_concrete() {
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::Float64);
        let union = LatticeType::Union(union_types);

        let int = LatticeType::Concrete(ConcreteType::Int64);

        let result = union.meet(&int);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_is_subtype_of_bottom() {
        let bottom = LatticeType::Bottom;
        let int = LatticeType::Concrete(ConcreteType::Int64);
        let top = LatticeType::Top;

        assert!(bottom.is_subtype_of(&int));
        assert!(bottom.is_subtype_of(&top));
        assert!(bottom.is_subtype_of(&bottom));
    }

    #[test]
    fn test_is_subtype_of_top() {
        let int = LatticeType::Concrete(ConcreteType::Int64);
        let top = LatticeType::Top;

        assert!(int.is_subtype_of(&top));
        assert!(!top.is_subtype_of(&int));
    }

    #[test]
    fn test_is_subtype_of_concrete_union() {
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::Float64);
        let union = LatticeType::Union(union_types);

        let int = LatticeType::Concrete(ConcreteType::Int64);
        let string = LatticeType::Concrete(ConcreteType::String);

        assert!(int.is_subtype_of(&union));
        assert!(!string.is_subtype_of(&union));
    }

    #[test]
    fn test_subtract_concrete() {
        let int = LatticeType::Concrete(ConcreteType::Int64);
        let float = LatticeType::Concrete(ConcreteType::Float64);

        let result = int.subtract(&int);
        assert_eq!(result, LatticeType::Bottom);

        let result = int.subtract(&float);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_subtract_union_concrete() {
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::Float64);
        union_types.insert(ConcreteType::String);
        let union = LatticeType::Union(union_types);

        let int = LatticeType::Concrete(ConcreteType::Int64);
        let result = union.subtract(&int);

        assert!(
            matches!(&result, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            result
        );
        if let LatticeType::Union(types) = result {
            assert_eq!(types.len(), 2);
            assert!(types.contains(&ConcreteType::Float64));
            assert!(types.contains(&ConcreteType::String));
            assert!(!types.contains(&ConcreteType::Int64));
        }
    }

    #[test]
    fn test_union_widening_by_length() {
        // Create a union with more than MAX_UNION_LENGTH (8) elements
        let mut types = BTreeSet::new();
        types.insert(ConcreteType::Int64);
        types.insert(ConcreteType::Float64);
        types.insert(ConcreteType::String);
        types.insert(ConcreteType::Bool);
        types.insert(ConcreteType::Char);
        types.insert(ConcreteType::Nothing);
        types.insert(ConcreteType::Symbol);
        types.insert(ConcreteType::Missing);
        types.insert(ConcreteType::Any); // 9 elements, MAX_UNION_LENGTH = 8

        let result = LatticeType::simplify_union(types);
        // Should widen to Top (since they're not all numeric)
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn test_union_widening_numeric() {
        // Create a union with more than MAX_UNION_LENGTH (8) numeric types
        let mut types = BTreeSet::new();
        types.insert(ConcreteType::Int8);
        types.insert(ConcreteType::Int16);
        types.insert(ConcreteType::Int32);
        types.insert(ConcreteType::Int64);
        types.insert(ConcreteType::Int128);
        types.insert(ConcreteType::UInt8);
        types.insert(ConcreteType::UInt16);
        types.insert(ConcreteType::UInt32);
        types.insert(ConcreteType::UInt64); // 9 numeric types

        let result = LatticeType::simplify_union(types);
        // Should widen to Union{Int64, Float64}
        assert!(
            matches!(&result, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            result
        );
        if let LatticeType::Union(widened) = result {
            assert_eq!(widened.len(), 2);
            assert!(widened.contains(&ConcreteType::Int64));
            assert!(widened.contains(&ConcreteType::Float64));
        }
    }

    #[test]
    fn test_complexity_computation() {
        // Simple types have depth 1
        assert_eq!(LatticeType::type_depth(&ConcreteType::Int64), 1);
        assert_eq!(LatticeType::type_depth(&ConcreteType::String), 1);

        // Array has depth 1 + element depth
        let array_int = ConcreteType::Array {
            element: Box::new(ConcreteType::Int64),
        };
        assert_eq!(LatticeType::type_depth(&array_int), 2);

        // Nested array has higher depth
        let nested_array = ConcreteType::Array {
            element: Box::new(array_int),
        };
        assert_eq!(LatticeType::type_depth(&nested_array), 3);
    }
}
