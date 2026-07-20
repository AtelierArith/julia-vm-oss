#[test]
fn empty_heap_transplant_detaches_shared_ref_graph_transactionally_9784() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let original = Rc::new(RefCell::new(Value::I64(7)));
    let mut roots = vec![
        ("a".to_string(), Value::Ref(original.clone())),
        ("b".to_string(), Value::Ref(original.clone())),
    ];

    let (compacted, remap) = reachable_compacted_struct_heap(&[], &mut roots);

    assert!(compacted.is_empty());
    assert!(remap.is_empty());
    let (Value::Ref(detached_a), Value::Ref(detached_b)) = (&roots[0].1, &roots[1].1) else {
        panic!("shared Ref roots must remain Refs");
    };
    assert!(Rc::ptr_eq(detached_a, detached_b));
    assert!(!Rc::ptr_eq(detached_a, &original));
    *detached_a.borrow_mut() = Value::I64(9);
    assert!(matches!(*detached_b.borrow(), Value::I64(9)));
    assert!(matches!(*original.borrow(), Value::I64(7)));
}
