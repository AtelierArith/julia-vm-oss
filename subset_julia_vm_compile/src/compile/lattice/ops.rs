//! Lattice operations for type inference.
//!
//! This module implements the core operations on the type lattice:
//! - join (⊔): least upper bound (union of types)
//! - meet (⊓): greatest lower bound (intersection of types)
//! - is_subtype_of (⊑): subtype relation
//! - subtract: type subtraction for narrowing
//!
//! These operations follow Julia's type lattice semantics.
//!
//! # Where the algebra lives (Issue #6605)
//!
//! The meet/join/widen family is consolidated behind the
//! [`AbstractLattice`](super::abstract_lattice::AbstractLattice) trait,
//! mirroring upstream Julia's `AbstractLattice` abstraction
//! (`julia/Compiler/src/abstractlattice.jl`). The operation *bodies* are the
//! `lattice_*` associated functions in this module — the single source of
//! truth. Two thin layers forward to them:
//!
//! - the trait `impl` in `abstract_lattice.rs`, and
//! - the public inherent methods below (`join`, `meet`, …) kept for
//!   source-compatibility with existing call sites and tests.
//!
//! Because the bodies are shared, the inherent-method API and the trait API
//! always agree by construction — there is no second implementation to drift.

// All `impl LatticeType` bodies have been moved to `subset_julia_vm_types`
// (Issue #8655). The inherent methods `join`, `join_limited`, `meet`,
// `is_subtype_of`, `subtract`, and the canonical `lattice_*` functions now
// live in `subset_julia_vm_types::runtime_types::lattice`.
// This file retains only the test suite that exercises those operations.

// All imports are used only by the test suite below; gated with #[cfg(test)].
#[cfg(test)]
use super::types::{ConcreteType, LatticeType};
#[cfg(test)]
use crate::inference_core::{CoreAbstract, CorePrimitive, CoreType};
#[cfg(test)]
use std::collections::BTreeSet;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_join_concrete_same() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let result = int.join(&int);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_join_concrete_different() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let float = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));

        let result = int.join(&float);
        assert!(
            matches!(&result, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            result
        );
        if let LatticeType::Union(types) = result {
            assert_eq!(types.len(), 2);
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))));
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ))));
        }
    }

    #[test]
    fn test_join_with_bottom() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let bottom = LatticeType::Bottom;

        assert_eq!(int.join(&bottom), int);
        assert_eq!(bottom.join(&int), int);
    }

    #[test]
    fn test_join_with_top() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let top = LatticeType::Top;

        assert_eq!(int.join(&top), LatticeType::Top);
        assert_eq!(top.join(&int), LatticeType::Top);
    }

    #[test]
    fn test_join_concrete_subtype_returns_supertype() {
        // LUB of a concrete type and its abstract supertype is the supertype:
        // join(Int64, Integer) = Integer (not Union{Int64, Integer}). The
        // concrete×concrete join arm previously only consulted tuple-aware
        // subtyping, leaving redundant union members that are semantically the
        // supertype anyway (Issue #5940, symmetric to the meet fix).
        let int64 = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let integer = LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
            CoreAbstract::Integer,
        )));

        assert_eq!(int64.join(&integer), integer);
        // join is commutative.
        assert_eq!(integer.join(&int64), integer);
    }

    #[test]
    fn test_meet_concrete_same() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let result = int.meet(&int);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_meet_concrete_different() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let float = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));

        let result = int.meet(&float);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_meet_concrete_subtype_returns_more_specific() {
        // GLB of a concrete type and its abstract supertype is the concrete
        // type: meet(Int64, Integer) = Int64 (not Bottom). The concrete×concrete
        // meet arm previously only consulted tuple-aware subtyping, leaving it
        // inconsistent with is_subtype_of (which uses the core hierarchy) and
        // collapsing valid narrowings to Bottom (Issue #5940).
        let int64 = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let integer = LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
            CoreAbstract::Integer,
        )));

        assert_eq!(int64.meet(&integer), int64);
        // meet is commutative.
        assert_eq!(integer.meet(&int64), int64);

        // Sanity: the lattice already agrees Int64 <: Integer.
        assert!(int64.is_subtype_of(&integer));
    }

    #[test]
    fn test_meet_union_concrete() {
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        let union = LatticeType::Union(union_types);

        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));

        let result = union.meet(&int);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_is_subtype_of_bottom() {
        let bottom = LatticeType::Bottom;
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let top = LatticeType::Top;

        assert!(bottom.is_subtype_of(&int));
        assert!(bottom.is_subtype_of(&top));
        assert!(bottom.is_subtype_of(&bottom));
    }

    #[test]
    fn test_is_subtype_of_top() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let top = LatticeType::Top;

        assert!(int.is_subtype_of(&top));
        assert!(!top.is_subtype_of(&int));
    }

    #[test]
    fn test_is_subtype_of_concrete_union() {
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        let union = LatticeType::Union(union_types);

        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let string = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));

        assert!(int.is_subtype_of(&union));
        assert!(!string.is_subtype_of(&union));
    }

    #[test]
    fn test_is_subtype_of_uses_core_hierarchy_for_concrete_types() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let integer = LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
            CoreAbstract::Integer,
        )));
        let number =
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number)));
        let string = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));

        assert!(int.is_subtype_of(&integer));
        assert!(int.is_subtype_of(&number));
        assert!(!string.is_subtype_of(&number));
    }

    #[test]
    fn test_is_subtype_of_uses_core_hierarchy_for_union_members() {
        let mut numeric_variants = BTreeSet::new();
        numeric_variants.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        numeric_variants.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        let numeric_union = LatticeType::Union(numeric_variants);

        let number =
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number)));
        assert!(numeric_union.is_subtype_of(&number));

        let mut abstract_union_variants = BTreeSet::new();
        abstract_union_variants.insert(ConcreteType::Core(CoreType::Abstract(
            CoreAbstract::Integer,
        )));
        abstract_union_variants.insert(ConcreteType::Core(CoreType::Abstract(
            CoreAbstract::AbstractFloat,
        )));
        let abstract_union = LatticeType::Union(abstract_union_variants);

        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let float = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        let string = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));

        assert!(int.is_subtype_of(&abstract_union));
        assert!(float.is_subtype_of(&abstract_union));
        assert!(!string.is_subtype_of(&abstract_union));
    }

    #[test]
    fn test_subtract_concrete() {
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let float = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));

        let result = int.subtract(&int);
        assert_eq!(result, LatticeType::Bottom);

        let result = int.subtract(&float);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_subtract_union_concrete() {
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        union_types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        let union = LatticeType::Union(union_types);

        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let result = union.subtract(&int);

        assert!(
            matches!(&result, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            result
        );
        if let LatticeType::Union(types) = result {
            assert_eq!(types.len(), 2);
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ))));
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            ))));
            assert!(!types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))));
        }
    }

    #[test]
    fn test_union_widening_by_length() {
        // Create a union with more than MAX_UNION_LENGTH (8) elements
        let mut types = BTreeSet::new();
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)));
        types.insert(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Nothing,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Symbol,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Missing,
        )));
        types.insert(ConcreteType::Core(CoreType::Any)); // 9 elements, MAX_UNION_LENGTH = 8

        let result = LatticeType::simplify_union(types);
        // Should widen to Top (since they're not all numeric)
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn test_union_widening_all_integers() {
        // Issue #3539: a large all-integer union widens to the abstract
        // `Integer` supertype, not to `Union{Int64, Float64}`.
        let mut types = BTreeSet::new();
        types.insert(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int16,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int32,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int128,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt8,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt16,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt32,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt64,
        ))); // 9 elements, exceeds MAX_UNION_LENGTH

        let result = LatticeType::simplify_union(types);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))
        );
    }

    #[test]
    fn test_union_widening_mixed_numeric_includes_unsigned_and_big() {
        // Issue #3539: a mixed-numeric union (unsigned + big + float + bool)
        // must not be normalized to `Union{Int64, Float64}`. It widens to the
        // abstract `Number` supertype.
        let mut types = BTreeSet::new();
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt64,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::UInt128,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigInt,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigFloat,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float32,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)));
        types.insert(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int16,
        ))); // 9 elements, exceeds MAX_UNION_LENGTH
        let result = LatticeType::simplify_union(types);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number)))
        );
    }

    #[test]
    fn test_union_widening_all_floats_to_abstract_float() {
        // Issue #3539: a wide union of only float types widens to AbstractFloat.
        let mut types = BTreeSet::new();
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float16,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float32,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigFloat,
        )));
        let result = LatticeType::widen_union(&types);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::AbstractFloat
            )))
        );
    }

    #[test]
    fn test_is_subtype_of_const_and_concrete_3538() {
        // Issue #3538: Const must be a subtype of its concrete type and unions.
        use crate::compile::lattice::types::ConstValue;

        let c1 = LatticeType::Const(ConstValue::Int64(1));
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let float = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));

        // Const(1) <: Concrete(Int64)
        assert!(c1.is_subtype_of(&int));
        // Const(1) </: Concrete(Float64)
        assert!(!c1.is_subtype_of(&float));

        // Const(1) <: Union{Int64, Float64}
        let mut us = BTreeSet::new();
        us.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        us.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        let u = LatticeType::Union(us);
        assert!(c1.is_subtype_of(&u));

        // Const(1) <: Top
        assert!(c1.is_subtype_of(&LatticeType::Top));

        // Const(1) <: Const(1) but not Const(1) <: Const(2)
        let c1b = LatticeType::Const(ConstValue::Int64(1));
        let c2 = LatticeType::Const(ConstValue::Int64(2));
        assert!(c1.is_subtype_of(&c1b));
        assert!(!c1.is_subtype_of(&c2));

        // Concrete(Int64) </: Const(1) (Const is more specific)
        assert!(!int.is_subtype_of(&c1));
    }

    // Issue #3511: tuple subtyping with Vararg tail.
    #[test]
    fn test_tuple_homogeneous_subtype_of_vararg() {
        // Tuple{Int,Int,Int} <: Tuple{Int, Vararg{Int}}
        let flat = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ],
        });
        let vararg = LatticeType::Concrete(ConcreteType::TupleVararg {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))],
            tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        assert!(flat.is_subtype_of(&vararg));
        // The reverse should not hold.
        assert!(!vararg.is_subtype_of(&flat));
    }

    #[test]
    fn test_tuple_zero_tail_is_subtype_of_vararg() {
        // Tuple{Int} <: Tuple{Int, Vararg{Int}} (empty tail)
        let flat = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))],
        });
        let vararg = LatticeType::Concrete(ConcreteType::TupleVararg {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))],
            tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        assert!(flat.is_subtype_of(&vararg));
    }

    #[test]
    fn test_tuple_heterogeneous_not_subtype_of_int_vararg() {
        // Tuple{Int, String} </: Tuple{Int, Vararg{Int}}
        let flat = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ],
        });
        let vararg = LatticeType::Concrete(ConcreteType::TupleVararg {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))],
            tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        assert!(!flat.is_subtype_of(&vararg));
    }

    #[test]
    fn test_tuple_short_prefix_not_subtype_of_vararg_with_long_prefix() {
        // Tuple{Int} </: Tuple{Int, Int, Vararg{Int}} — needs at least 2 fixed.
        let flat = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))],
        });
        let vararg = LatticeType::Concrete(ConcreteType::TupleVararg {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ],
            tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        assert!(!flat.is_subtype_of(&vararg));
    }

    #[test]
    fn test_normalize_tuple_vararg_short_unchanged() {
        // Short tuples are kept flat.
        let elements = vec![ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)); 3];
        let normalized = ConcreteType::normalize_tuple_vararg(elements.clone());
        assert_eq!(normalized, ConcreteType::Tuple { elements });
    }

    #[test]
    fn test_normalize_tuple_vararg_long_homogeneous() {
        // 16 Int64 args -> Tuple{Int64, Vararg{Int64}}.
        let elements = vec![ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)); 16];
        let normalized = ConcreteType::normalize_tuple_vararg(elements);
        match normalized {
            ConcreteType::TupleVararg { elements, tail } => {
                assert_eq!(
                    elements,
                    vec![ConcreteType::Core(CoreType::Primitive(
                        CorePrimitive::Int64
                    ))]
                );
                assert_eq!(
                    *tail,
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
                );
            }
            other => panic!("expected TupleVararg, got {:?}", other),
        }
    }

    #[test]
    fn test_normalize_tuple_vararg_long_heterogeneous() {
        // Mixed Int64/Float64 in a long tail -> Vararg with UnionOf tail.
        let mut elements = vec![ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)); 6];
        elements.extend(vec![
            ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ));
            6
        ]);
        let normalized = ConcreteType::normalize_tuple_vararg(elements);
        match normalized {
            ConcreteType::TupleVararg { elements, tail } => {
                assert_eq!(
                    elements,
                    vec![ConcreteType::Core(CoreType::Primitive(
                        CorePrimitive::Int64
                    ))]
                );
                match *tail {
                    ConcreteType::UnionOf(types) => {
                        assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                            CorePrimitive::Int64
                        ))));
                        assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                            CorePrimitive::Float64
                        ))));
                    }
                    other => panic!("expected UnionOf tail, got {:?}", other),
                }
            }
            other => panic!("expected TupleVararg, got {:?}", other),
        }
    }

    #[test]
    fn test_join_tuple_with_vararg_collapses() {
        // Tuple{Int,Int} ⊔ Tuple{Int, Vararg{Int}} = Tuple{Int, Vararg{Int}}.
        let flat = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ],
        });
        let vararg = LatticeType::Concrete(ConcreteType::TupleVararg {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))],
            tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        let joined = flat.join(&vararg);
        assert_eq!(joined, vararg);
    }

    #[test]
    fn test_complexity_computation() {
        // Simple types have depth 1
        assert_eq!(
            LatticeType::type_depth(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))),
            1
        );
        assert_eq!(
            LatticeType::type_depth(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            ))),
            1
        );

        // Array has depth 1 + element depth
        let array_int = ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ndims: None,
        };
        assert_eq!(LatticeType::type_depth(&array_int), 2);

        // Nested array has higher depth
        let nested_array = ConcreteType::Array {
            element: Box::new(array_int),
            ndims: None,
        };
        assert_eq!(LatticeType::type_depth(&nested_array), 3);
    }

    #[test]
    fn test_join_limited_preserves_nullable_pattern() {
        // Issue #3507: a `Union{Int64, Nothing}` joined with itself, given
        // itself as the comparison type, must come out unchanged. Pure
        // `join` would also return the same value, but we additionally
        // check that the limit step does not over-widen.
        let nullable = LatticeType::Union(
            [
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
            ]
            .iter()
            .cloned()
            .collect(),
        );
        let joined = nullable.join_limited(&nullable, &nullable);
        assert_eq!(joined, nullable);
    }

    // ====== Issue #3503: Conditional in lattice ops ======

    fn cond(slot: &str, then_t: LatticeType, else_t: LatticeType) -> LatticeType {
        LatticeType::make_conditional(slot, then_t, else_t)
    }

    fn ty(c: ConcreteType) -> LatticeType {
        LatticeType::Concrete(c)
    }

    fn union_of(items: &[ConcreteType]) -> LatticeType {
        let set: BTreeSet<ConcreteType> = items.iter().cloned().collect();
        if set.len() == 1 {
            LatticeType::Concrete(set.into_iter().next().unwrap())
        } else {
            LatticeType::Union(set)
        }
    }

    #[test]
    fn test_make_conditional_collapses_when_branches_equal() {
        // make_conditional drops the wrapper when then == else, since the
        // Conditional carries no narrowing information.
        let int = ty(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let result = LatticeType::make_conditional("x", int.clone(), int.clone());
        assert_eq!(result, int);
        assert!(!result.is_conditional());
    }

    #[test]
    fn test_make_conditional_preserves_when_branches_differ() {
        let result = LatticeType::make_conditional(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        assert!(result.is_conditional());
        match result {
            LatticeType::Conditional { ref slot, .. } => assert_eq!(slot, "x"),
            other => panic!("expected Conditional, got {:?}", other),
        }
    }

    #[test]
    fn test_widen_conditional_yields_branch_join() {
        // widen_conditional ≡ then ⊔ else.
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let widened = c.widen_conditional();
        assert_eq!(
            widened,
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing))
            ])
        );
    }

    #[test]
    fn test_widen_conditional_identity_for_non_conditional() {
        let int = ty(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert_eq!(int.widen_conditional(), int);
        assert_eq!(LatticeType::Top.widen_conditional(), LatticeType::Top);
    }

    #[test]
    fn test_join_two_conditionals_same_slot_branchwise() {
        // Conditional(x; Int, Nothing) ⊔ Conditional(x; String, Nothing) =
        //   Conditional(x; Union{Int, String}, Nothing)
        let a = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let b = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let joined = a.join(&b);
        match joined {
            LatticeType::Conditional {
                ref slot,
                ref then_type,
                ref else_type,
            } => {
                assert_eq!(slot, "x");
                assert_eq!(
                    **then_type,
                    union_of(&[
                        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                        ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))
                    ])
                );
                assert_eq!(
                    **else_type,
                    ty(ConcreteType::Core(CoreType::Primitive(
                        CorePrimitive::Nothing
                    )))
                );
            }
            other => panic!("expected Conditional, got {:?}", other),
        }
    }

    #[test]
    fn test_join_two_conditionals_different_slot_widens_both() {
        // Different slots → widen both → join the widenings.
        let a = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let b = cond(
            "y",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let joined = a.join(&b);
        // widen(a) = Union{Int, Nothing}, widen(b) = Union{String, Nothing},
        // joined = Union{Int, String, Nothing}.
        let expected = union_of(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
        ]);
        assert_eq!(joined, expected);
    }

    #[test]
    fn test_join_conditional_with_concrete_no_longer_collapses_to_top() {
        // Pre-Issue #3503 this returned Top. Now: widen the conditional and
        // join with the concrete type, preserving the relevant Union.
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let s = ty(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        let joined = c.join(&s);
        let expected = union_of(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
        ]);
        assert_eq!(joined, expected);
        assert_ne!(joined, LatticeType::Top);
    }

    #[test]
    fn test_join_conditional_with_compatible_union_preserves_nullable() {
        // Acceptance criterion: nullable pattern preserved when joining
        // a Conditional with a compatible Union (Issue #3503).
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let nullable = union_of(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
        ]);
        let joined = c.join(&nullable);
        assert_eq!(joined, nullable);
    }

    #[test]
    fn test_meet_two_conditionals_same_slot_branchwise() {
        // Conditional(x; Union{Int, Float}, Nothing) ⊓
        // Conditional(x; Union{Int, String}, Nothing) =
        //   Conditional(x; Int, Nothing)
        let a = cond(
            "x",
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ]),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let b = cond(
            "x",
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ]),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let met = a.meet(&b);
        // The Conditional(x; Int64, Nothing) collapses through
        // make_conditional rules.
        assert_eq!(
            met,
            cond(
                "x",
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Nothing
                )))
            )
        );
    }

    #[test]
    fn test_meet_conditional_with_concrete_no_longer_collapses_to_bottom() {
        // Pre-Issue #3503 this returned Bottom. Now: widen the Conditional
        // and meet with the concrete type — the intersection is not empty.
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let int = ty(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let met = c.meet(&int);
        assert_eq!(met, int);
        assert_ne!(met, LatticeType::Bottom);
    }

    #[test]
    fn test_meet_conditional_with_compatible_union() {
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let nullable = union_of(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
        ]);
        let met = c.meet(&nullable);
        assert_eq!(met, nullable);
    }

    #[test]
    fn test_is_subtype_of_conditional_uses_widening() {
        // Conditional(x; Int, Nothing) <: Union{Int, Nothing} via widening.
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let nullable = union_of(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
        ]);
        assert!(c.is_subtype_of(&nullable));

        // Conditional(x; Int, Nothing) </: Int (else branch is Nothing,
        // which is not a subtype of Int).
        let int = ty(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        assert!(!c.is_subtype_of(&int));
    }

    #[test]
    fn test_is_subtype_of_two_conditionals_same_slot_branchwise() {
        // Conditional(x; Int, Nothing) <: Conditional(x; Union{Int, Float}, Nothing).
        let lhs = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let rhs = cond(
            "x",
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ]),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        assert!(lhs.is_subtype_of(&rhs));
        // The reverse should not hold.
        assert!(!rhs.is_subtype_of(&lhs));
    }

    #[test]
    fn test_subtract_conditional_distributes_branchwise() {
        // (Conditional(x; Union{Int, String}, Nothing)) - String =
        //   Conditional(x; Int, Nothing).
        let c = cond(
            "x",
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ]),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        let s = ty(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        )));
        let result = c.subtract(&s);
        assert_eq!(
            result,
            cond(
                "x",
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Nothing
                )))
            )
        );
    }

    #[test]
    fn test_subtract_conditional_collapses_when_branches_match() {
        // Build an explicit Conditional with diverging branches (the
        // public `make_conditional` would have collapsed it if branches
        // were already identical). After subtracting `Union{String,
        // Nothing}`, both branches become `Int64` and `make_conditional`
        // collapses the wrapper.
        let c = LatticeType::Conditional {
            slot: "x".to_string(),
            then_type: Box::new(union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ])),
            else_type: Box::new(union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
            ])),
        };
        let drop = union_of(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
        ]);
        let result = c.subtract(&drop);
        assert_eq!(
            result,
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        assert!(!result.is_conditional());
    }

    #[test]
    fn test_join_conditional_top_passes_through_to_top() {
        // join with Top still gives Top (Top is the absorbing element,
        // checked before the Conditional arms). Issue #3503.
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        assert_eq!(c.join(&LatticeType::Top), LatticeType::Top);
        assert_eq!(LatticeType::Top.join(&c), LatticeType::Top);
    }

    #[test]
    fn test_join_conditional_bottom_yields_conditional() {
        // join with Bottom is the identity (Bottom is the identity element,
        // checked before Conditional arms). Issue #3503.
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        assert_eq!(c.join(&LatticeType::Bottom), c);
        assert_eq!(LatticeType::Bottom.join(&c), c);
    }

    #[test]
    fn test_meet_conditional_bottom_yields_bottom() {
        let c = cond(
            "x",
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
        );
        assert_eq!(c.meet(&LatticeType::Bottom), LatticeType::Bottom);
        assert_eq!(LatticeType::Bottom.meet(&c), LatticeType::Bottom);
    }

    #[test]
    fn test_join_limited_widens_runaway_union_against_small_compare_to() {
        // A "growing" loop accumulator: previously known as `Int64`, the
        // body produces a wide unrelated mixed-numeric union. After the
        // join, `limit_type_size` must collapse the result rather than
        // letting the union grow without bound.
        let mut wide = BTreeSet::new();
        for c in [
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)),
        ] {
            wide.insert(c);
        }
        let body = LatticeType::Union(wide);
        let prev = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        let result = prev.join_limited(&body, &prev);
        // 9 distinct integer members exceed MAX_UNION_LENGTH=8 → widened
        // to the `Integer` supertype (Issue #3539 widening strategy).
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))
        );
    }

    #[test]
    fn test_join_limited_preserves_known_wide_union_against_self() {
        let wide = union_of(&[
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Missing)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Symbol)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)),
        ]);

        assert_eq!(wide.join_limited(&wide, &wide), wide);
    }

    /// Build a single-element tuple nested `depth` levels: depth 1 = `Tuple{T}`,
    /// depth 2 = `Tuple{Tuple{T}}`, etc.
    fn nested_tuple(leaf: ConcreteType, depth: usize) -> ConcreteType {
        let mut ty = leaf;
        for _ in 0..depth {
            ty = ConcreteType::Tuple { elements: vec![ty] };
        }
        ty
    }

    #[test]
    fn test_join_limited_preserves_deep_member_against_seeding_compare_to() {
        // Issue #4273: a structured return union whose deepest member exceeds
        // the absolute complexity cap is preserved when that member is already
        // present in the comparison type (it seeded the accumulator). This is
        // the branch/loop return-aggregation case: a deep tuple return seen
        // first, then a shallow `Int` return joined against it.
        //
        // depth 6 == 1 (outer) + 5 nesting levels → above MAX_UNION_COMPLEXITY.
        let deep = LatticeType::Concrete(nested_tuple(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            6,
        ));
        let shallow = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));

        // Plain join widens the deep-member union all the way to `Top` because
        // its complexity exceeds the unconditional bound.
        assert_eq!(deep.join(&shallow), LatticeType::Top);

        // Comparison-aware join against the deep seed preserves the union: the
        // deep member is derived from `compare_to`, so no new complexity is
        // introduced and only the shallow `Int` counts as a new member.
        let mut expected = BTreeSet::new();
        expected.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        expected.insert(nested_tuple(
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            6,
        ));
        let expected = LatticeType::Union(expected);

        assert_eq!(deep.join_limited(&shallow, &deep), expected);
    }

    // ====== Issue #6605: AbstractLattice trait consolidation ======
    //
    // These tests are the authoritative verification layer for the
    // behavior-preserving consolidation. GLB/LUB precision bugs do not
    // surface in VM output (runtime fallback), so we PIN exact meet/join/
    // widen results AND assert the new `AbstractLattice` trait methods agree
    // with the public inherent methods bit-for-bit across concrete×concrete,
    // abstract-hierarchy, tuple, and union cases (symmetric-pair hazard,
    // #5940 lesson).

    use super::super::abstract_lattice::AbstractLattice;

    /// A representative spread of lattice values covering every variant and
    /// the cases the issue calls out (concrete×concrete, abstract hierarchy,
    /// tuple/vararg, union, const, conditional, top/bottom).
    fn sample_lattice_values() -> Vec<LatticeType> {
        vec![
            LatticeType::Bottom,
            LatticeType::Top,
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
            ty(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer,
            ))),
            ty(ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number))),
            ty(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::AbstractFloat,
            ))),
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ]),
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
            ]),
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ]),
            LatticeType::Const(crate::compile::lattice::types::ConstValue::Int64(42)),
            LatticeType::Const(crate::compile::lattice::types::ConstValue::Int64(43)),
            LatticeType::Concrete(ConcreteType::Tuple {
                elements: vec![
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ],
            }),
            LatticeType::Concrete(ConcreteType::TupleVararg {
                elements: vec![ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))],
                tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
            }),
            cond(
                "x",
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Nothing,
                ))),
            ),
            cond(
                "y",
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::String,
                ))),
                ty(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Nothing,
                ))),
            ),
        ]
    }

    #[test]
    fn test_trait_join_matches_inherent_method() {
        for a in sample_lattice_values() {
            for b in sample_lattice_values() {
                assert_eq!(
                    AbstractLattice::join(&a, &b),
                    a.join(&b),
                    "trait join != inherent join for {a:?} ⊔ {b:?}"
                );
            }
        }
    }

    #[test]
    fn test_trait_meet_matches_inherent_method() {
        for a in sample_lattice_values() {
            for b in sample_lattice_values() {
                assert_eq!(
                    AbstractLattice::meet(&a, &b),
                    a.meet(&b),
                    "trait meet != inherent meet for {a:?} ⊓ {b:?}"
                );
            }
        }
    }

    #[test]
    fn test_trait_is_subtype_matches_inherent_method() {
        for a in sample_lattice_values() {
            for b in sample_lattice_values() {
                assert_eq!(
                    AbstractLattice::is_subtype(&a, &b),
                    a.is_subtype_of(&b),
                    "trait is_subtype != inherent is_subtype_of for {a:?} ⊑ {b:?}"
                );
            }
        }
    }

    #[test]
    fn test_trait_subtract_matches_inherent_method() {
        for a in sample_lattice_values() {
            for b in sample_lattice_values() {
                assert_eq!(
                    AbstractLattice::subtract(&a, &b),
                    a.subtract(&b),
                    "trait subtract != inherent subtract for {a:?} ∖ {b:?}"
                );
            }
        }
    }

    #[test]
    fn test_trait_join_limited_matches_inherent_method() {
        let values = sample_lattice_values();
        for a in &values {
            for b in &values {
                for c in &values {
                    assert_eq!(
                        AbstractLattice::join_limited(a, b, c),
                        a.join_limited(b, c),
                        "trait join_limited != inherent join_limited"
                    );
                }
            }
        }
    }

    #[test]
    fn test_trait_widen_is_identity_for_non_union() {
        // `widen` only collapses Union elements; everything else is identity.
        for v in sample_lattice_values() {
            if !matches!(v, LatticeType::Union(_)) {
                assert_eq!(
                    AbstractLattice::widen(&v),
                    v,
                    "widen changed non-Union {v:?}"
                );
            }
        }
    }

    #[test]
    fn test_trait_widen_collapses_all_integer_union_to_integer() {
        // Pins the widen result for a homogeneous integer union: the abstract
        // `Integer` supertype (Issue #3539 strategy), reached via the trait.
        let mut ints = BTreeSet::new();
        for c in [
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        ] {
            ints.insert(c);
        }
        let u = LatticeType::Union(ints);
        assert_eq!(
            AbstractLattice::widen(&u),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))
        );
    }

    #[test]
    fn test_trait_widen_matches_widen_union_helper_for_unions() {
        // The trait-level `widen` over a Union must equal the underlying
        // `widen_union(&BTreeSet)` building block — same widening semantics,
        // just lifted to a whole LatticeType.
        for v in sample_lattice_values() {
            if let LatticeType::Union(types) = &v {
                assert_eq!(AbstractLattice::widen(&v), LatticeType::widen_union(types));
            }
        }
    }

    #[test]
    fn test_pin_join_concrete_abstract_hierarchy() {
        // Abstract-hierarchy LUB: join(Int64, Integer) = Integer.
        assert_eq!(
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
            .join(&ty(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))),
            ty(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))
        );
        // join(Int64, Float64) = Union{Int64, Float64} (no subtype relation).
        assert_eq!(
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
            .join(&ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))),
            union_of(&[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))
            ])
        );
    }

    #[test]
    fn test_pin_meet_concrete_abstract_hierarchy() {
        // Abstract-hierarchy GLB: meet(Int64, Integer) = Int64.
        assert_eq!(
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
            .meet(&ty(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))),
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        // Disjoint concrete types meet to Bottom.
        assert_eq!(
            ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
            .meet(&ty(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))),
            LatticeType::Bottom
        );
    }

    #[test]
    fn test_pin_tuple_vararg_join_meet() {
        // Tuple ⊔ Vararg collapses to the Vararg supertype; ⊓ to the flat tuple.
        let flat = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ],
        });
        let vararg = LatticeType::Concrete(ConcreteType::TupleVararg {
            elements: vec![ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))],
            tail: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        });
        assert_eq!(flat.join(&vararg), vararg);
        assert_eq!(flat.meet(&vararg), flat);
        // And the trait agrees.
        assert_eq!(AbstractLattice::join(&flat, &vararg), vararg);
        assert_eq!(AbstractLattice::meet(&flat, &vararg), flat);
    }

    // ====== Issue #8544: PartialStruct lattice element ======

    fn ps_int() -> LatticeType {
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )))
    }

    fn ps_float() -> LatticeType {
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )))
    }

    fn ps_box(fields: Vec<LatticeType>) -> LatticeType {
        LatticeType::PartialStruct {
            struct_name: "Box".to_string(),
            type_id: 7,
            field_names: vec!["a".to_string(), "b".to_string()],
            fields,
        }
    }

    fn box_widened() -> LatticeType {
        LatticeType::Concrete(ConcreteType::Struct {
            name: "Box".to_string(),
            type_id: 7,
        })
    }

    #[test]
    fn partial_struct_join_same_shape_joins_fieldwise_8544() {
        use crate::compile::lattice::types::ConstValue;
        let a = ps_box(vec![LatticeType::Const(ConstValue::Int64(1)), ps_float()]);
        let b = ps_box(vec![LatticeType::Const(ConstValue::Int64(2)), ps_float()]);
        let joined = a.join(&b);
        // Conflicting Const facts widen to the field's concrete type; the
        // agreeing field is preserved; the struct shape survives the join.
        assert_eq!(joined, ps_box(vec![ps_int(), ps_float()]));
    }

    #[test]
    fn partial_struct_join_with_widened_struct_drops_facts_8544() {
        let ps = ps_box(vec![ps_int(), ps_float()]);
        assert_eq!(ps.join(&box_widened()), box_widened());
        assert_eq!(box_widened().join(&ps), box_widened());
    }

    #[test]
    fn partial_struct_join_different_structs_widens_both_8544() {
        let ps = ps_box(vec![ps_int(), ps_float()]);
        let other = LatticeType::PartialStruct {
            struct_name: "Pair".to_string(),
            type_id: 9,
            field_names: vec!["x".to_string()],
            fields: vec![ps_int()],
        };
        let joined = ps.join(&other);
        let LatticeType::Union(members) = joined else {
            panic!("expected Union of the two widened structs");
        };
        assert!(members.contains(&ConcreteType::Struct {
            name: "Box".to_string(),
            type_id: 7,
        }));
        assert!(members.contains(&ConcreteType::Struct {
            name: "Pair".to_string(),
            type_id: 9,
        }));
    }

    #[test]
    fn partial_struct_meet_keeps_refinement_under_supertype_8544() {
        // tmeet(PartialStruct, T) == PartialStruct when widenconst ⊑ T.
        let ps = ps_box(vec![ps_int(), ps_float()]);
        assert_eq!(ps.meet(&box_widened()), ps);
        assert_eq!(box_widened().meet(&ps), ps);
        assert_eq!(ps.meet(&LatticeType::Top), ps);
        // Disjoint type: meet through the widened struct type → Bottom.
        assert_eq!(ps.meet(&ps_int()), LatticeType::Bottom);
    }

    #[test]
    fn partial_struct_meet_fieldwise_bottom_propagates_8544() {
        use crate::compile::lattice::types::ConstValue;
        let a = ps_box(vec![LatticeType::Const(ConstValue::Int64(1)), ps_float()]);
        let b = ps_box(vec![LatticeType::Const(ConstValue::Int64(2)), ps_float()]);
        // Field `a` intersects to Bottom → no such instance exists.
        assert_eq!(a.meet(&b), LatticeType::Bottom);
        let c = ps_box(vec![ps_int(), ps_float()]);
        // Const(1) ⊓ Int64 keeps the Const fact.
        assert_eq!(a.meet(&c), a);
    }

    #[test]
    fn partial_struct_subtype_rules_8544() {
        use crate::compile::lattice::types::ConstValue;
        let refined = ps_box(vec![LatticeType::Const(ConstValue::Int64(1)), ps_float()]);
        let wider = ps_box(vec![ps_int(), ps_float()]);
        // Field-wise ⊑ on the same shape.
        assert!(refined.is_subtype_of(&wider));
        assert!(!wider.is_subtype_of(&refined));
        // PartialStruct ⊑ its widened struct type ⊑ Top.
        assert!(refined.is_subtype_of(&box_widened()));
        assert!(refined.is_subtype_of(&LatticeType::Top));
        // The widened type is NOT a subtype of the refinement.
        assert!(!box_widened().is_subtype_of(&refined));
        // Bottom ⊑ PartialStruct.
        assert!(LatticeType::Bottom.is_subtype_of(&refined));
    }

    #[test]
    fn partial_struct_widenconst_and_accessors_8544() {
        let ps = ps_box(vec![ps_int(), ps_float()]);
        assert_eq!(ps.widen_partial_struct(), box_widened());
        assert!(ps.is_partial_struct());
        assert_eq!(ps.partial_struct_field_by_name("a"), Some(&ps_int()));
        assert_eq!(ps.partial_struct_field_by_name("b"), Some(&ps_float()));
        assert_eq!(ps.partial_struct_field_by_name("c"), None);
        assert_eq!(ps.partial_struct_field_by_index(1), Some(&ps_int()));
        assert_eq!(ps.partial_struct_field_by_index(2), Some(&ps_float()));
        assert_eq!(ps.partial_struct_field_by_index(0), None);
        assert_eq!(ps.partial_struct_field_by_index(3), None);
        // Smart constructor collapses misaligned shapes to the widened type.
        assert_eq!(
            LatticeType::partial_struct("Box", 7, vec!["a".to_string()], vec![]),
            box_widened()
        );
        assert_eq!(
            LatticeType::partial_struct("Box", 7, vec![], vec![]),
            box_widened()
        );
    }

    #[test]
    fn partial_struct_join_limited_bounds_nesting_8544() {
        use super::super::widening::{MAX_UNION_COMPLEXITY, MAX_UNION_LENGTH};
        // Build a PartialStruct nested deeper than the complexity budget.
        let mut nested = ps_box(vec![ps_int(), ps_float()]);
        for _ in 0..(MAX_UNION_COMPLEXITY + 2) {
            nested = ps_box(vec![nested, ps_float()]);
        }
        let limited = super::super::widening::limit_type_size(
            &nested,
            None,
            MAX_UNION_LENGTH,
            MAX_UNION_COMPLEXITY,
        );
        // The over-deep facts are dropped to the widened struct type.
        assert_eq!(limited, box_widened());
    }
}
