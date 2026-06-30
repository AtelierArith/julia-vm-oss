// ---------------------------------------------------------------------------
// Recursive `ConstructorPartial` (nested PartialStruct) helpers — Issue #4269.
// A field whose value is itself an analyzable immutable constructor carries a
// nested partial so a chained `getfield`/index access keeps inner precision,
// mirroring upstream `Core.PartialStruct` recursion.
// ---------------------------------------------------------------------------

use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

fn inner_partial() -> ConstructorPartial {
    let mut fields = HashMap::new();
    fields.insert(
        "a".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        ))),
    );
    fields.insert(
        "b".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        ))),
    );
    ConstructorPartial {
        result_type: LatticeType::Concrete(ConcreteType::Struct {
            name: "InnerBox".to_string(),
            type_id: 1,
        }),
        struct_name: "InnerBox".to_string(),
        fields,
        field_order: vec!["a".to_string(), "b".to_string()],
        nested: HashMap::new(),
    }
}

fn outer_partial(inner: ConstructorPartial) -> ConstructorPartial {
    let mut fields = HashMap::new();
    fields.insert(
        "inner".to_string(),
        LatticeType::Concrete(ConcreteType::Struct {
            name: "InnerBox".to_string(),
            type_id: 1,
        }),
    );
    fields.insert(
        "tag".to_string(),
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::String,
        ))),
    );
    let mut nested = HashMap::new();
    nested.insert("inner".to_string(), Box::new(inner));
    ConstructorPartial {
        result_type: LatticeType::Concrete(ConcreteType::Struct {
            name: "OuterBox".to_string(),
            type_id: 2,
        }),
        struct_name: "OuterBox".to_string(),
        fields,
        field_order: vec!["inner".to_string(), "tag".to_string()],
        nested,
    }
}

#[test]
fn nested_field_by_name_and_index_agree() {
    let outer = outer_partial(inner_partial());
    // `:inner` is field 1, and both name- and index-keyed lookups recover
    // the same nested partial.
    let by_name = outer.nested_field("inner").expect("nested by name");
    let by_index = outer.nested_field_by_index(1).expect("nested by index");
    assert_eq!(by_name, by_index);
    assert_eq!(by_name.struct_name, "InnerBox");
    assert_eq!(
        by_name.fields.get("b"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String)
        )))
    );
    // The non-partial `tag` field has no nested fact.
    assert!(outer.nested_field("tag").is_none());
    assert!(outer.nested_field_by_index(2).is_none());
}

#[test]
fn join_preserves_matching_nested_facts() {
    let then_outer = outer_partial(inner_partial());
    let else_outer = outer_partial(inner_partial());
    let joined = join_constructor_partials(Some(then_outer), Some(else_outer))
        .expect("matching shapes join");
    let nested = joined.nested_field("inner").expect("nested survives join");
    assert_eq!(
        nested.fields.get("b"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String)
        )))
    );
}

#[test]
fn join_drops_nested_when_only_one_branch_has_it() {
    let then_outer = outer_partial(inner_partial());
    // The else branch has the same struct shape but no nested fact for
    // `inner`, so the joined partial cannot soundly keep it.
    let mut else_outer = outer_partial(inner_partial());
    else_outer.nested.clear();
    let joined = join_constructor_partials(Some(then_outer), Some(else_outer))
        .expect("matching shapes still join");
    assert!(joined.nested_field("inner").is_none());
    // The widened by-name field type still survives.
    assert_eq!(
        joined.fields.get("tag"),
        Some(&LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::String)
        )))
    );
}
