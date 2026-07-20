#[test]
fn callable_singleton_identity_is_provenance_aware_and_injective_11685() {
    use crate::vm::value::CallableSingletonIdentity;

    let source = CallableSingletonIdentity::source("same_name_11685");
    let helper = CallableSingletonIdentity::from_provenance("same_name_11685", true);
    assert_ne!(source, helper);
    assert_ne!(source.type_name(), helper.type_name());

    // A legal source name can imitate the internal prefix. It must be escaped
    // into the source domain rather than collide with a helper encoding.
    let imitating_source = CallableSingletonIdentity::source(helper.encoded_name());
    assert_ne!(imitating_source.type_name(), helper.type_name());

    let imported =
        CallableSingletonIdentity::with_owners("f", vec!["OwnerA11685.f".to_string()], false);
    let qualified = CallableSingletonIdentity::with_owners(
        "OwnerA11685.f",
        vec!["OwnerA11685.f".to_string()],
        false,
    );
    let sibling =
        CallableSingletonIdentity::with_owners("f", vec!["OwnerB11685.f".to_string()], false);
    assert!(imported.same_callable(&qualified));
    assert_eq!(imported.dispatch_key(), qualified.dispatch_key());
    assert!(!imported.same_callable(&sibling));
    assert_ne!(imported.dispatch_key(), sibling.dispatch_key());
}
