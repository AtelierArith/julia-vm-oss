// ---------------------------------------------------------------------------
// Recursive PartialStruct (nested field-fact) semantics — Issue #4269.
// A field whose value is itself an analyzable immutable constructor carries a
// nested `LatticeType::PartialStruct` fact, so a chained `getfield`/index
// access keeps inner precision, mirroring upstream `Core.PartialStruct`
// recursion. Since Issue #8739 these facts live directly on the lattice value
// (the ConstructorPartial side channel is retired), so the invariants are
// asserted on `LatticeType` itself.
// ---------------------------------------------------------------------------

use super::super::*;
use crate::inference_core::{CorePrimitive, CoreType};

fn int64() -> LatticeType {
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int64,
    )))
}

fn string_ty() -> LatticeType {
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::String,
    )))
}

fn inner_partial() -> LatticeType {
    LatticeType::partial_struct(
        "InnerBox",
        1,
        vec!["a".to_string(), "b".to_string()],
        vec![int64(), string_ty()],
    )
}

/// Outer struct whose `inner` field fact is `inner_fact` (a nested
/// PartialStruct or its widened struct form) and whose `tag` field is a plain
/// String fact.
fn outer_partial(inner_fact: LatticeType) -> LatticeType {
    LatticeType::partial_struct(
        "OuterBox",
        2,
        vec!["inner".to_string(), "tag".to_string()],
        vec![inner_fact, string_ty()],
    )
}

#[test]
fn nested_field_by_name_and_index_agree() {
    let outer = outer_partial(inner_partial());
    // `:inner` is field 1, and both name- and index-keyed lookups recover
    // the same nested fact.
    let by_name = outer
        .partial_struct_field_by_name("inner")
        .expect("nested by name");
    let by_index = outer
        .partial_struct_field_by_index(1)
        .expect("nested by index");
    assert_eq!(by_name, by_index);
    assert!(by_name.is_partial_struct());
    assert_eq!(
        by_name.widen_partial_struct(),
        LatticeType::Concrete(ConcreteType::Struct {
            name: "InnerBox".to_string(),
            type_id: 1,
        })
    );
    // The chained inner access resolves the precise inner field type.
    assert_eq!(
        by_name.partial_struct_field_by_name("b"),
        Some(&string_ty())
    );
    // The non-partial `tag` field carries only its flat fact.
    let tag = outer
        .partial_struct_field_by_name("tag")
        .expect("tag fact present");
    assert!(!tag.is_partial_struct());
    assert_eq!(tag, &string_ty());
    // Out-of-range positional access falls back to `None`.
    assert!(outer.partial_struct_field_by_index(0).is_none());
    assert!(outer.partial_struct_field_by_index(3).is_none());
}

#[test]
fn join_preserves_matching_nested_facts() {
    let then_outer = outer_partial(inner_partial());
    let else_outer = outer_partial(inner_partial());
    let joined = then_outer.join(&else_outer);
    assert!(
        joined.is_partial_struct(),
        "matching shapes join field-wise"
    );
    let nested = joined
        .partial_struct_field_by_name("inner")
        .expect("nested survives join");
    assert!(nested.is_partial_struct());
    assert_eq!(nested.partial_struct_field_by_name("b"), Some(&string_ty()));
}

#[test]
fn join_drops_nested_when_only_one_branch_has_it() {
    let then_outer = outer_partial(inner_partial());
    // The else branch has the same struct shape but only the widened struct
    // type for `inner` (no nested fact), so the joined fact for that field
    // cannot soundly keep the inner refinement.
    let else_outer = outer_partial(inner_partial().widen_partial_struct());
    let joined = then_outer.join(&else_outer);
    assert!(joined.is_partial_struct(), "matching shapes still join");
    let inner_fact = joined
        .partial_struct_field_by_name("inner")
        .expect("inner field fact present");
    assert!(
        !inner_fact.is_partial_struct(),
        "one-sided nested fact must widen to the struct type in the join"
    );
    assert_eq!(
        inner_fact,
        &LatticeType::Concrete(ConcreteType::Struct {
            name: "InnerBox".to_string(),
            type_id: 1,
        })
    );
    // The sibling flat field fact still survives.
    assert_eq!(
        joined.partial_struct_field_by_name("tag"),
        Some(&string_ty())
    );
}
